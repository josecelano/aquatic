use std::time::Duration;

pub mod common;
pub mod config;
pub mod handlers;
pub mod network;
pub mod tasks;

use config::Config;
use common::State;


pub fn run(){
    let config = Config::default();
    let state = State::new();

    for i in 0..config.num_threads {
        let state = state.clone();
        let config = config.clone();

        ::std::thread::spawn(move || {
            network::run_event_loop(state, config, i);
        });
    }

    if config.statistics.interval != 0 {
        let state = state.clone();
        let config = config.clone();

        ::std::thread::spawn(move || {
            loop {
                ::std::thread::sleep(Duration::from_secs(
                    config.statistics.interval
                ));

                tasks::gather_and_print_statistics(&state, &config);
            }
        });
    }

    loop {
        ::std::thread::sleep(Duration::from_secs(config.cleaning.interval));

        tasks::clean_connections(&state, &config);
        tasks::clean_torrents(&state, &config);
    }
}
