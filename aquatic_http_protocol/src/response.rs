use std::net::{Ipv4Addr, Ipv6Addr};
use std::io::Write;

use std::collections::BTreeMap;
use serde::{Serialize, Deserialize};

use super::common::*;
use super::utils::*;


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsePeer<I: Eq>{
    pub ip_address: I,
    pub port: u16
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct ResponsePeerListV4(
    #[serde(
        serialize_with = "serialize_response_peers_ipv4",
        deserialize_with = "deserialize_response_peers_ipv4",
    )]
    pub Vec<ResponsePeer<Ipv4Addr>>
);


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct ResponsePeerListV6(
    #[serde(
        serialize_with = "serialize_response_peers_ipv6",
        deserialize_with = "deserialize_response_peers_ipv6",
    )]
    pub Vec<ResponsePeer<Ipv6Addr>>
);


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeStatistics {
    pub complete: usize,
    pub incomplete: usize,
    pub downloaded: usize,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnounceResponse {
    #[serde(rename = "interval")]
    pub announce_interval: usize,
    pub complete: usize,
    pub incomplete: usize,
    #[serde(default)]
    pub peers: ResponsePeerListV4,
    #[serde(default)]
    pub peers6: ResponsePeerListV6,
}


impl AnnounceResponse {
    fn write<W: Write>(&self, output: &mut W) -> ::std::io::Result<usize> {
        let mut bytes_written = 0usize;

        bytes_written += output.write(b"d8:completei")?;
        bytes_written += output.write(
            itoa::Buffer::new().format(self.complete).as_bytes()
        )?;

        bytes_written += output.write(b"e10:incompletei")?;
        bytes_written += output.write(
            itoa::Buffer::new().format(self.incomplete).as_bytes()
        )?;

        bytes_written += output.write(b"e8:intervali")?;
        bytes_written += output.write(
            itoa::Buffer::new().format(self.announce_interval).as_bytes()
        )?;

        bytes_written += output.write(b"e5:peers")?;
        bytes_written += output.write(
            itoa::Buffer::new().format(self.peers.0.len() * 6).as_bytes()
        )?;
        bytes_written += output.write(b":")?;
        for peer in self.peers.0.iter() {
            bytes_written += output.write(
                &u32::from(peer.ip_address).to_be_bytes()
            )?;
            bytes_written += output.write(&peer.port.to_be_bytes())?;
        }

        bytes_written += output.write(b"6:peers6")?;
        bytes_written += output.write(
            itoa::Buffer::new().format(self.peers6.0.len() * 18).as_bytes()
        )?;
        bytes_written += output.write(b":")?;
        for peer in self.peers6.0.iter() {
            bytes_written += output.write(
                &u128::from(peer.ip_address).to_be_bytes()
            )?;
            bytes_written += output.write(&peer.port.to_be_bytes())?;
        }
        bytes_written += output.write(b"e")?;

        Ok(bytes_written)
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeResponse {
    /// BTreeMap instead of HashMap since keys need to be serialized in order
    pub files: BTreeMap<InfoHash, ScrapeStatistics>,
}


impl ScrapeResponse {
    fn write<W: Write>(&self, output: &mut W) -> ::std::io::Result<usize> {
        let mut bytes_written = 0usize;

        bytes_written += output.write(b"d5:filesd")?;
        
        for (info_hash, statistics) in self.files.iter(){
            bytes_written += output.write(b"20:")?;
            bytes_written += output.write(&info_hash.0)?;
            bytes_written += output.write(b"d8:completei")?;
            bytes_written += output.write(
                itoa::Buffer::new().format(statistics.complete).as_bytes()
            )?;
            bytes_written += output.write(b"e10:downloadedi0e10:incompletei")?;
            bytes_written += output.write(
                itoa::Buffer::new().format(statistics.incomplete).as_bytes()
            )?;
            bytes_written += output.write(b"ee")?;
        }

        bytes_written += output.write(b"ee")?;

        Ok(bytes_written)
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureResponse {
    #[serde(rename = "failure reason")]
    pub failure_reason: String,
}


impl FailureResponse {
    fn write<W: Write>(&self, output: &mut W) -> ::std::io::Result<usize> {
        let mut bytes_written = 0usize;

        let reason_bytes = self.failure_reason.as_bytes();

        bytes_written += output.write(b"d14:failure reason")?;
        bytes_written += output.write(
            itoa::Buffer::new().format(reason_bytes.len()).as_bytes()
        )?;
        bytes_written += output.write(b":")?;
        bytes_written += output.write(reason_bytes)?;
        bytes_written += output.write(b"e")?;

        Ok(bytes_written)
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Announce(AnnounceResponse),
    Scrape(ScrapeResponse),
    Failure(FailureResponse),
}


impl Response {
    pub fn write<W: Write>(&self, output: &mut W) -> ::std::io::Result<usize> {
        match self {
            Response::Announce(r) => r.write(output),
            Response::Failure(r) => r.write(output),
            Response::Scrape(r) => r.write(output),
        }
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ::serde_bencode::Error> {
        ::serde_bencode::from_bytes(bytes)
    }
}


#[cfg(test)]
impl quickcheck::Arbitrary for ResponsePeer<Ipv4Addr> {
    fn arbitrary<G: quickcheck::Gen>(g: &mut G) -> Self {
        Self {
            ip_address: Ipv4Addr::arbitrary(g),
            port: u16::arbitrary(g)
        }
    }
}


#[cfg(test)]
impl quickcheck::Arbitrary for ResponsePeer<Ipv6Addr> {
    fn arbitrary<G: quickcheck::Gen>(g: &mut G) -> Self {
        Self {
            ip_address: Ipv6Addr::arbitrary(g),
            port: u16::arbitrary(g)
        }
    }
}


#[cfg(test)]
impl quickcheck::Arbitrary for ResponsePeerListV4 {
    fn arbitrary<G: quickcheck::Gen>(g: &mut G) -> Self {
        Self(Vec::arbitrary(g))
    }
}


#[cfg(test)]
impl quickcheck::Arbitrary for ResponsePeerListV6 {
    fn arbitrary<G: quickcheck::Gen>(g: &mut G) -> Self {
        Self(Vec::arbitrary(g))
    }
}


#[cfg(test)]
impl quickcheck::Arbitrary for ScrapeStatistics {
    fn arbitrary<G: quickcheck::Gen>(g: &mut G) -> Self {
        Self {
            complete: usize::arbitrary(g),
            incomplete: usize::arbitrary(g),
            downloaded: 0,
        }
    }
}


#[cfg(test)]
impl quickcheck::Arbitrary for AnnounceResponse {
    fn arbitrary<G: quickcheck::Gen>(g: &mut G) -> Self {
        Self {
            announce_interval: usize::arbitrary(g),
            complete: usize::arbitrary(g),
            incomplete: usize::arbitrary(g),
            peers: ResponsePeerListV4::arbitrary(g),
            peers6: ResponsePeerListV6::arbitrary(g),
        }
    }
}


#[cfg(test)]
impl quickcheck::Arbitrary for ScrapeResponse {
    fn arbitrary<G: quickcheck::Gen>(g: &mut G) -> Self {
        Self {
            files: BTreeMap::arbitrary(g),
        }
    }
}


#[cfg(test)]
impl quickcheck::Arbitrary for FailureResponse {
    fn arbitrary<G: quickcheck::Gen>(g: &mut G) -> Self {
        Self {
            failure_reason: String::arbitrary(g),
        }
    }
}


#[cfg(test)]
mod tests {
    use quickcheck_macros::*;

    use super::*;

    #[quickcheck]
    fn test_announce_response_to_bytes(response: AnnounceResponse) -> bool {
        let reference = bendy::serde::to_bytes(
            &Response::Announce(response.clone())
        ).unwrap();

        let mut output = Vec::new();

        response.write(&mut output).unwrap();

        output == reference
    }

    #[quickcheck]
    fn test_scrape_response_to_bytes(response: ScrapeResponse) -> bool {
        let reference = bendy::serde::to_bytes(
            &Response::Scrape(response.clone())
        ).unwrap();

        let mut hand_written = Vec::new();

        response.write(&mut hand_written).unwrap();

        let success = hand_written == reference;

        if !success {
            println!("reference: {}", String::from_utf8_lossy(&reference));
            println!("hand_written: {}", String::from_utf8_lossy(&hand_written));
        }

        success
    }

    #[quickcheck]
    fn test_failure_response_to_bytes(response: FailureResponse) -> bool {
        let reference = bendy::serde::to_bytes(
            &Response::Failure(response.clone())
        ).unwrap();

        let mut hand_written = Vec::new();

        response.write(&mut hand_written).unwrap();

        let success = hand_written == reference;

        if !success {
            println!("reference:    {}", String::from_utf8_lossy(&reference));
            println!("hand_written: {}", String::from_utf8_lossy(&hand_written));
        }

        success
    }
}