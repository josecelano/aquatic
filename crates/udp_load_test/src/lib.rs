use std::iter::repeat_with;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::AtomicUsize;
use std::sync::{atomic::Ordering, Arc};
use std::thread::{self, Builder};
use std::time::{Duration, Instant};

#[cfg(feature = "cpu-pinning")]
use aquatic_common::cpu_pinning::{pin_current_if_configured_to, WorkerIndex};
use aquatic_common::IndexMap;
use aquatic_udp_protocol::{InfoHash, Port};
use hdrhistogram::Histogram;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, WeightedAliasIndex};

mod common;
pub mod config;
mod worker;

use common::*;
use config::Config;
use worker::*;

impl aquatic_common::cli::Config for Config {
    fn get_log_level(&self) -> Option<aquatic_common::cli::LogLevel> {
        Some(self.log_level)
    }
}

pub fn run(config: Config) -> ::anyhow::Result<()> {
    if config.requests.weight_announce
        + config.requests.weight_connect
        + config.requests.weight_scrape
        == 0
    {
        panic!("Error: at least one weight must be larger than zero.");
    }

    if config.summarize_last > config.duration {
        panic!("Error: report_last_seconds can't be larger than duration");
    }

    println!("Starting client with config: {:#?}", config);

    let info_hash_dist = InfoHashDist::new(&config)?;
    let peers_by_worker = create_peers(&config, &info_hash_dist);

    let state = LoadTestState {
        info_hashes: info_hash_dist.into_arc_info_hashes(),
        statistics: Arc::new(SharedStatistics::default()),
    };

    // Start workers

    for (i, peers) in (0..config.workers).zip(peers_by_worker) {
        let port = config.network.first_port + (i as u16);

        let ip = if config.server_address.is_ipv6() {
            Ipv6Addr::LOCALHOST.into()
        } else if config.network.multiple_client_ipv4s {
            Ipv4Addr::new(127, 0, 0, 1 + i).into()
        } else {
            Ipv4Addr::LOCALHOST.into()
        };

        let addr = SocketAddr::new(ip, port);
        let config = config.clone();
        let state = state.clone();

        Builder::new().name("load-test".into()).spawn(move || {
            #[cfg(feature = "cpu-pinning")]
            pin_current_if_configured_to(
                &config.cpu_pinning,
                config.workers as usize,
                0,
                WorkerIndex::SocketWorker(i as usize),
            );

            Worker::run(config, state, peers, addr)
        })?;
    }

    #[cfg(feature = "cpu-pinning")]
    pin_current_if_configured_to(
        &config.cpu_pinning,
        config.workers as usize,
        0,
        WorkerIndex::Util,
    );

    monitor_statistics(state, &config);

    Ok(())
}

fn monitor_statistics(state: LoadTestState, config: &Config) {
    let mut report_avg_connect: Vec<f64> = Vec::new();
    let mut report_avg_announce: Vec<f64> = Vec::new();
    let mut report_avg_scrape: Vec<f64> = Vec::new();
    let mut report_avg_error: Vec<f64> = Vec::new();

    const INTERVAL: u64 = 5;

    let start_time = Instant::now();
    let duration = Duration::from_secs(config.duration as u64);

    let mut last = start_time;

    let time_elapsed = loop {
        thread::sleep(Duration::from_secs(INTERVAL));

        let requests = fetch_and_reset(&state.statistics.requests);
        let response_peers = fetch_and_reset(&state.statistics.response_peers);
        let responses_connect = fetch_and_reset(&state.statistics.responses_connect);
        let responses_announce = fetch_and_reset(&state.statistics.responses_announce);
        let responses_scrape = fetch_and_reset(&state.statistics.responses_scrape);
        let responses_error = fetch_and_reset(&state.statistics.responses_error);

        let now = Instant::now();

        let elapsed = (now - last).as_secs_f64();

        last = now;

        let peers_per_announce_response = response_peers / responses_announce;

        let avg_requests = requests / elapsed;
        let avg_responses_connect = responses_connect / elapsed;
        let avg_responses_announce = responses_announce / elapsed;
        let avg_responses_scrape = responses_scrape / elapsed;
        let avg_responses_error = responses_error / elapsed;

        let avg_responses = avg_responses_connect
            + avg_responses_announce
            + avg_responses_scrape
            + avg_responses_error;

        report_avg_connect.push(avg_responses_connect);
        report_avg_announce.push(avg_responses_announce);
        report_avg_scrape.push(avg_responses_scrape);
        report_avg_error.push(avg_responses_error);

        println!();
        println!("Requests out: {:.2}/second", avg_requests);
        println!("Responses in: {:.2}/second", avg_responses);
        println!("  - Connect responses:  {:.2}", avg_responses_connect);
        println!("  - Announce responses: {:.2}", avg_responses_announce);
        println!("  - Scrape responses:   {:.2}", avg_responses_scrape);
        println!("  - Error responses:    {:.2}", avg_responses_error);
        println!(
            "Peers per announce response: {:.2}",
            peers_per_announce_response
        );

        let time_elapsed = start_time.elapsed();

        if config.duration != 0 && time_elapsed >= duration {
            break time_elapsed;
        }
    };

    if config.summarize_last != 0 {
        let split_at = (config.duration - config.summarize_last) / INTERVAL as usize;

        report_avg_connect = report_avg_connect.split_off(split_at);
        report_avg_announce = report_avg_announce.split_off(split_at);
        report_avg_scrape = report_avg_scrape.split_off(split_at);
        report_avg_error = report_avg_error.split_off(split_at);
    }

    let len = report_avg_connect.len() as f64;

    let avg_connect: f64 = report_avg_connect.into_iter().sum::<f64>() / len;
    let avg_announce: f64 = report_avg_announce.into_iter().sum::<f64>() / len;
    let avg_scrape: f64 = report_avg_scrape.into_iter().sum::<f64>() / len;
    let avg_error: f64 = report_avg_error.into_iter().sum::<f64>() / len;

    let avg_total = avg_connect + avg_announce + avg_scrape + avg_error;

    println!();
    println!("# aquatic load test report");
    println!();
    println!(
        "Test ran for {} seconds {}",
        time_elapsed.as_secs(),
        if config.summarize_last != 0 {
            format!("(only last {} included in summary)", config.summarize_last)
        } else {
            "".to_string()
        }
    );
    println!("Average responses per second: {:.2}", avg_total);
    println!("  - Connect responses:  {:.2}", avg_connect);
    println!("  - Announce responses: {:.2}", avg_announce);
    println!("  - Scrape responses:   {:.2}", avg_scrape);
    println!("  - Error responses:    {:.2}", avg_error);
    println!();
    println!("Config: {:#?}", config);
    println!();
}

fn fetch_and_reset(atomic_usize: &AtomicUsize) -> f64 {
    atomic_usize.fetch_and(0, Ordering::Relaxed) as f64
}

fn create_peers(config: &Config, info_hash_dist: &InfoHashDist) -> Vec<Box<[Peer]>> {
    let mut rng = SmallRng::seed_from_u64(0xc3a58be617b3acce);

    let mut opt_peers_per_info_hash: Option<IndexMap<usize, u64>> =
        config.peer_histogram.then_some(IndexMap::default());

    let mut all_peers = repeat_with(|| {
        let num_scrape_indices = rng.gen_range(1..config.requests.scrape_max_torrents + 1);

        let scrape_info_hash_indices = repeat_with(|| info_hash_dist.get_random_index(&mut rng))
            .take(num_scrape_indices)
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let (announce_info_hash_index, announce_info_hash) = info_hash_dist.get_random(&mut rng);

        if let Some(peers_per_info_hash) = opt_peers_per_info_hash.as_mut() {
            *peers_per_info_hash
                .entry(announce_info_hash_index)
                .or_default() += 1;
        }

        Peer {
            announce_info_hash,
            announce_port: Port::new(rng.gen()),
            scrape_info_hash_indices,
        }
    })
    .take(config.requests.number_of_peers)
    .collect::<Vec<_>>();

    if let Some(peers_per_info_hash) = opt_peers_per_info_hash {
        let mut histogram = Histogram::<u64>::new(2).unwrap();

        for num_peers in peers_per_info_hash.values() {
            histogram.record(*num_peers).unwrap();
        }

        let percentiles = [
            1.0, 10.0, 25.0, 50.0, 75.0, 85.0, 90.0, 95.0, 98.0, 99.9, 100.0,
        ];

        for p in percentiles {
            let value = histogram.value_at_percentile(p);

            println!("Peers at info hash percentile {}: {}", p, value);
        }
    }

    let mut peers_by_worker = Vec::new();

    let num_peers_per_worker = all_peers.len() / config.workers as usize;

    for _ in 0..(config.workers as usize) {
        peers_by_worker.push(
            all_peers
                .split_off(all_peers.len() - num_peers_per_worker)
                .into_boxed_slice(),
        );

        all_peers.shrink_to_fit();
    }

    peers_by_worker
}

struct InfoHashDist {
    info_hashes: Box<[InfoHash]>,
    dist: WeightedAliasIndex<f64>,
}

impl InfoHashDist {
    fn new(config: &Config) -> anyhow::Result<Self> {
        let mut rng = SmallRng::seed_from_u64(0xc3aa8be617b3acce);

        let info_hashes = repeat_with(|| {
            let mut bytes = [0u8; 20];

            for byte in bytes.iter_mut() {
                *byte = rng.gen();
            }

            InfoHash(bytes)
        })
        .take(config.requests.number_of_torrents)
        .collect::<Vec<InfoHash>>()
        .into_boxed_slice();

        let num_torrents = config.requests.number_of_torrents as u32;

        let weights = (0..num_torrents)
            .map(|i| {
                let floor = num_torrents as f64 / config.requests.number_of_peers as f64;

                floor + (7.0f64 - ((300.0 * f64::from(i)) / f64::from(num_torrents))).exp()
            })
            .collect();

        let dist = WeightedAliasIndex::new(weights)?;

        Ok(Self { info_hashes, dist })
    }

    fn get_random(&self, rng: &mut impl Rng) -> (usize, InfoHash) {
        let index = self.dist.sample(rng);

        (index, self.info_hashes[index])
    }

    fn get_random_index(&self, rng: &mut impl Rng) -> usize {
        self.dist.sample(rng)
    }

    fn into_arc_info_hashes(self) -> Arc<[InfoHash]> {
        Arc::from(self.info_hashes)
    }
}
