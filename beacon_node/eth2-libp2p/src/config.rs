use crate::Multiaddr;
use libp2p::gossipsub::{GossipsubConfig, GossipsubConfigBuilder};
//use std::time::Duration;

/// The beacon node topic string to subscribe to.
pub const BEACON_PUBSUB_TOPIC: &str = "beacon_node";
pub const SHARD_TOPIC_PREFIX: &str = "attestations"; // single topic for all attestation for the moment.

#[derive(Clone, Debug)]
/// Network configuration for lighthouse.
pub struct Config {
    //TODO: stubbing networking initial params, change in the future
    /// IP address to listen on.
    pub listen_addresses: Vec<Multiaddr>,
    /// Listen port UDP/TCP.
    pub listen_port: u16,
    /// Gossipsub configuration parameters.
    pub gs_config: GossipsubConfig,
    /// Configuration parameters for node identification protocol.
    pub identify_config: IdentifyConfig,
    /// List of nodes to initially connect to.
    pub boot_nodes: Vec<Multiaddr>,
    /// Client version
    pub client_version: String,
    /// List of extra topics to initially subscribe to as strings.
    pub topics: Vec<String>,
}

impl Default for Config {
    /// Generate a default network configuration.
    fn default() -> Self {
        Config {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/9000"
                .parse()
                .expect("is a correct multi-address")],
            listen_port: 9000,
            gs_config: GossipsubConfigBuilder::new()
                .max_gossip_size(4_000_000)
                //                .inactivity_timeout(Duration::from_secs(90))
                .build(),
            identify_config: IdentifyConfig::default(),
            boot_nodes: Vec::new(),
            client_version: version::version(),
            topics: Vec::new(),
        }
    }
}

/// Generates a default Config.
impl Config {
    pub fn new() -> Self {
        Config::default()
    }
}

/// The configuration parameters for the Identify protocol
#[derive(Debug, Clone)]
pub struct IdentifyConfig {
    /// The protocol version to listen on.
    pub version: String,
    /// The client's name and version for identification.
    pub user_agent: String,
}

impl Default for IdentifyConfig {
    fn default() -> Self {
        Self {
            version: "/eth/serenity/1.0".to_string(),
            user_agent: version::version(),
        }
    }
}

/// Creates a standard network config from a chain_id.
///
/// This creates specified network parameters for each chain type.
impl From<ChainType> for Config {
    fn from(chain_type: ChainType) -> Self {
        match chain_type {
            ChainType::Foundation => Config::default(),

            ChainType::LighthouseTestnet => {
                let boot_nodes = vec!["/ip4/127.0.0.1/tcp/9000"
                    .parse()
                    .expect("correct multiaddr")];
                Self {
                    boot_nodes,
                    ..Config::default()
                }
            }

            ChainType::Other => Config::default(),
        }
    }
}

pub enum ChainType {
    Foundation,
    LighthouseTestnet,
    Other,
}

/// Maps a chain id to a ChainType.
impl From<u8> for ChainType {
    fn from(chain_id: u8) -> Self {
        match chain_id {
            1 => ChainType::Foundation,
            2 => ChainType::LighthouseTestnet,
            _ => ChainType::Other,
        }
    }
}
