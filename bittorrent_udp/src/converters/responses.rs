use byteorder::{ReadBytesExt, WriteBytesExt, NetworkEndian};

use std::io;
use std::io::{Cursor, Write};
use std::net::{IpAddr, Ipv6Addr, Ipv4Addr};

use crate::types::{self, *};


#[inline]
pub fn response_to_bytes(
    bytes: &mut impl Write,
    response: types::Response,
    ip_version: types::IpVersion
){
    match response {
        types::Response::Connect(r) => {
            bytes.write_i32::<NetworkEndian>(0).unwrap();
            bytes.write_i32::<NetworkEndian>(r.transaction_id.0).unwrap();
            bytes.write_i64::<NetworkEndian>(r.connection_id.0).unwrap();
        },

        types::Response::Announce(r) => {
            bytes.write_i32::<NetworkEndian>(1).unwrap();
            bytes.write_i32::<NetworkEndian>(r.transaction_id.0).unwrap();
            bytes.write_i32::<NetworkEndian>(r.announce_interval.0).unwrap();
            bytes.write_i32::<NetworkEndian>(r.leechers.0).unwrap();
            bytes.write_i32::<NetworkEndian>(r.seeders.0).unwrap();

            // Write peer IPs and ports. Silently ignore peers with wrong
            // IP version
            for peer in r.peers {
                let mut correct = false;

                match peer.ip_address {
                    IpAddr::V4(ip) => {
                        if let types::IpVersion::IPv4 = ip_version {
                            bytes.write_all(&ip.octets()).unwrap();

                            correct = true;
                        }
                    },
                    IpAddr::V6(ip) => {
                        if let types::IpVersion::IPv6 = ip_version {
                            bytes.write_all(&ip.octets()).unwrap();

                            correct = true;
                        }
                    }
                }

                if correct {
                    bytes.write_u16::<NetworkEndian>(peer.port.0).unwrap();
                }
            }
        },

        types::Response::Scrape(r) => {
            bytes.write_i32::<NetworkEndian>(2).unwrap();
            bytes.write_i32::<NetworkEndian>(r.transaction_id.0).unwrap();

            for torrent_stat in r.torrent_stats {
                bytes.write_i32::<NetworkEndian>(torrent_stat.seeders.0)
                    .unwrap();
                bytes.write_i32::<NetworkEndian>(torrent_stat.completed.0)
                    .unwrap();
                bytes.write_i32::<NetworkEndian>(torrent_stat.leechers.0)
                    .unwrap();
            }
        },

        types::Response::Error(r) => {
            bytes.write_i32::<NetworkEndian>(3).unwrap();
            bytes.write_i32::<NetworkEndian>(r.transaction_id.0).unwrap();

            bytes.write_all(r.message.as_bytes()).unwrap();
        },
    }
}


#[inline]
pub fn response_from_bytes(
    bytes: &[u8],
    ip_version: IpVersion,
) -> Result<Response, io::Error> {
    let mut cursor = Cursor::new(bytes);

    let action = cursor.read_i32::<NetworkEndian>()?;
    let transaction_id = cursor.read_i32::<NetworkEndian>()?;

    match action {
        // Connect
        0 => {
            let connection_id = cursor.read_i64::<NetworkEndian>()?;

            Ok(Response::Connect(ConnectResponse {
                connection_id: ConnectionId(connection_id),
                transaction_id: TransactionId(transaction_id)
            }))
        },
        // Announce
        1 => {
            let announce_interval = cursor.read_i32::<NetworkEndian>()?;
            let leechers = cursor.read_i32::<NetworkEndian>()?;
            let seeders = cursor.read_i32::<NetworkEndian>()?;

            let position = cursor.position() as usize;
            let inner = cursor.into_inner();

            let peers = if ip_version == IpVersion::IPv4 {
                inner[position..].chunks_exact(6).map(|chunk| {
                    let ip_address = IpAddr::V4(
                        Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3])
                    );

                    let port = (&chunk[4..]).read_u16::<NetworkEndian>().unwrap();

                    ResponsePeer {
                        ip_address,
                        port: Port(port),
                    }
                }).collect()
            } else {
                inner[position..].chunks_exact(18).map(|chunk| {
                    let mut cursor: Cursor<&[u8]> = Cursor::new(&chunk[..]);

                    let ip_address = IpAddr::V6(Ipv6Addr::new(
                        cursor.read_u16::<NetworkEndian>().unwrap(),
                        cursor.read_u16::<NetworkEndian>().unwrap(),
                        cursor.read_u16::<NetworkEndian>().unwrap(),
                        cursor.read_u16::<NetworkEndian>().unwrap(),
                        cursor.read_u16::<NetworkEndian>().unwrap(),
                        cursor.read_u16::<NetworkEndian>().unwrap(),
                        cursor.read_u16::<NetworkEndian>().unwrap(),
                        cursor.read_u16::<NetworkEndian>().unwrap(),
                    ));

                    let port = cursor.read_u16::<NetworkEndian>().unwrap();

                    ResponsePeer {
                        ip_address,
                        port: Port(port),
                    }
                }).collect()
            };

            Ok(Response::Announce(AnnounceResponse {
                transaction_id: TransactionId(transaction_id),
                announce_interval: AnnounceInterval(announce_interval),
                leechers: NumberOfPeers(leechers),
                seeders: NumberOfPeers(seeders),
                peers
            }))

        },
        // Scrape
        2 => {
            let position = cursor.position() as usize;
            let inner = cursor.into_inner();

            let stats = inner[position..].chunks_exact(12).map(|chunk| {
                let mut cursor: Cursor<&[u8]> = Cursor::new(&chunk[..]);

                let seeders = cursor.read_i32::<NetworkEndian>().unwrap();
                let downloads = cursor.read_i32::<NetworkEndian>().unwrap();
                let leechers = cursor.read_i32::<NetworkEndian>().unwrap();

                TorrentScrapeStatistics {
                    seeders: NumberOfPeers(seeders),
                    completed: NumberOfDownloads(downloads),
                    leechers:NumberOfPeers(leechers)
                }
            }).collect();

            Ok(Response::Scrape(ScrapeResponse {
                transaction_id: TransactionId(transaction_id),
                torrent_stats: stats
            }))
        },
        // Error
        3 => {
            let position = cursor.position() as usize;
            let inner = cursor.into_inner();

            Ok(Response::Error(ErrorResponse {
                transaction_id: TransactionId(transaction_id),
                message: String::from_utf8_lossy(&inner[position..]).into()
            }))
        },
        _ => {
            Ok(Response::Error(ErrorResponse {
                transaction_id: TransactionId(transaction_id),
                message: "Invalid action".to_string()
            }))
        }
    }
}