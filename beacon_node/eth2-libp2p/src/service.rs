use crate::behaviour::{Behaviour, BehaviourEvent, PubsubMessage};
use crate::config::*;
use crate::error;
use crate::multiaddr::Protocol;
use crate::rpc::RPCEvent;
use crate::NetworkConfig;
use crate::{Topic, TopicHash};
use futures::prelude::*;
use futures::Stream;
use libp2p::core::{
    identity::Keypair, multiaddr::Multiaddr, muxing::StreamMuxerBox, nodes::Substream,
    transport::boxed::Boxed, ConnectedPoint,
};
use libp2p::{core, secio, swarm::NetworkBehaviour, PeerId, Swarm, Transport};
use slog::{crit, debug, info, trace, warn};
use smallvec::SmallVec;
use std::fs::File;
use std::io::prelude::*;
use std::io::{Error, ErrorKind};
use std::time::Duration;
use std::time::Instant;

type Libp2pStream = Boxed<(PeerId, StreamMuxerBox), Error>;
type Libp2pBehaviour = Behaviour<Substream<StreamMuxerBox>>;

const NETWORK_KEY_FILENAME: &str = "key";
/// The time in milliseconds to wait before banning a peer. This allows for any Goodbye messages to be
/// flushed and protocols to be negotiated.
const BAN_PEER_TIMEOUT: u64 = 200;

/// The configuration and state of the libp2p components for the beacon node.
pub struct Service {
    /// The libp2p Swarm handler.
    //TODO: Make this private
    pub swarm: Swarm<Libp2pStream, Libp2pBehaviour>,

    /// This node's PeerId.
    pub local_peer_id: PeerId,

    /// A current list of peers to ban after a given timeout.
    peers_to_ban: SmallVec<[(PeerId, Instant); 4]>,

    /// Indicates if the listening address have been verified and compared to the expected ENR.
    verified_listen_address: bool,

    /// The libp2p logger handle.
    pub log: slog::Logger,
}

impl Service {
    pub fn new(config: NetworkConfig, log: slog::Logger) -> error::Result<Self> {
        trace!(log, "Libp2p Service starting");

        let local_keypair = if let Some(hex_bytes) = &config.secret_key_hex {
            keypair_from_hex(hex_bytes)?
        } else {
            load_private_key(&config, &log)
        };

        // load the private key from CLI flag, disk or generate a new one
        let local_peer_id = PeerId::from(local_keypair.public());
        info!(log, "Libp2p Service"; "peer_id" => format!("{:?}", local_peer_id));

        let mut swarm = {
            // Set up the transport - tcp/ws with secio and mplex/yamux
            let transport = build_transport(local_keypair.clone());
            // Lighthouse network behaviour
            let behaviour = Behaviour::new(&local_keypair, &config, &log)?;
            Swarm::new(transport, behaviour, local_peer_id.clone())
        };

        // listen on the specified address
        let listen_multiaddr = {
            let mut m = Multiaddr::from(config.listen_address);
            m.push(Protocol::Tcp(config.libp2p_port));
            m
        };

        match Swarm::listen_on(&mut swarm, listen_multiaddr.clone()) {
            Ok(_) => {
                let mut log_address = listen_multiaddr;
                log_address.push(Protocol::P2p(local_peer_id.clone().into()));
                info!(log, "Listening established"; "address" => format!("{}", log_address));
            }
            Err(err) => {
                crit!(
                    log,
                    "Unable to listen on libp2p address";
                    "error" => format!("{:?}", err),
                    "listen_multiaddr" => format!("{}", listen_multiaddr),
                );
                return Err("Libp2p was unable to listen on the given listen address.".into());
            }
        };

        // helper closure for dialing peers
        let mut dial_addr = |multiaddr: Multiaddr| {
            match Swarm::dial_addr(&mut swarm, multiaddr.clone()) {
                Ok(()) => debug!(log, "Dialing libp2p peer"; "address" => format!("{}", multiaddr)),
                Err(err) => debug!(
                    log,
                    "Could not connect to peer"; "address" => format!("{}", multiaddr), "error" => format!("{:?}", err)
                ),
            };
        };

        // attempt to connect to user-input libp2p nodes
        for multiaddr in config.libp2p_nodes {
            dial_addr(multiaddr);
        }

        // attempt to connect to any specified boot-nodes
        for bootnode_enr in config.boot_nodes {
            for multiaddr in bootnode_enr.multiaddr() {
                // ignore udp multiaddr if it exists
                let components = multiaddr.iter().collect::<Vec<_>>();
                if let Protocol::Udp(_) = components[1] {
                    continue;
                }
                dial_addr(multiaddr);
            }
        }

        // subscribe to default gossipsub topics
        let mut topics = vec![];

        /* Here we subscribe to all the required gossipsub topics required for interop.
         * The topic builder adds the required prefix and postfix to the hardcoded topics that we
         * must subscribe to.
         */
        let topic_builder = |topic| {
            Topic::new(format!(
                "/{}/{}/{}",
                TOPIC_PREFIX, topic, TOPIC_ENCODING_POSTFIX,
            ))
        };
        topics.push(topic_builder(BEACON_BLOCK_TOPIC));
        topics.push(topic_builder(BEACON_ATTESTATION_TOPIC));
        topics.push(topic_builder(VOLUNTARY_EXIT_TOPIC));
        topics.push(topic_builder(PROPOSER_SLASHING_TOPIC));
        topics.push(topic_builder(ATTESTER_SLASHING_TOPIC));

        // Add any topics specified by the user
        topics.append(&mut config.topics.iter().cloned().map(Topic::new).collect());

        let mut subscribed_topics = vec![];
        for topic in topics {
            if swarm.subscribe(topic.clone()) {
                trace!(log, "Subscribed to topic"; "topic" => format!("{}", topic));
                subscribed_topics.push(topic);
            } else {
                warn!(log, "Could not subscribe to topic"; "topic" => format!("{}", topic));
            }
        }
        info!(log, "Subscribed to topics"; "topics" => format!("{:?}", subscribed_topics.iter().map(|t| format!("{}", t)).collect::<Vec<String>>()));

        Ok(Service {
            local_peer_id,
            swarm,
            peers_to_ban: SmallVec::new(),
            verified_listen_address: false,
            log,
        })
    }

    /// Adds a peer to be banned after a timeout period.
    pub fn disconnect_and_ban_peer(&mut self, peer_id: PeerId) {
        self.peers_to_ban.push((
            peer_id,
            Instant::now() + Duration::from_millis(BAN_PEER_TIMEOUT),
        ));
    }
}

impl Stream for Service {
    type Item = Libp2pEvent;
    type Error = crate::error::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            match self.swarm.poll() {
                Ok(Async::Ready(Some(event))) => match event {
                    BehaviourEvent::GossipMessage {
                        id,
                        source,
                        topics,
                        message,
                    } => {
                        trace!(self.log, "Gossipsub message received"; "service" => "Swarm");
                        return Ok(Async::Ready(Some(Libp2pEvent::PubsubMessage {
                            id,
                            source,
                            topics,
                            message,
                        })));
                    }
                    BehaviourEvent::RPC(peer_id, event) => {
                        return Ok(Async::Ready(Some(Libp2pEvent::RPC(peer_id, event))));
                    }
                    BehaviourEvent::PeerDialed(peer_id) => {
                        return Ok(Async::Ready(Some(Libp2pEvent::PeerDialed(peer_id))));
                    }
                    BehaviourEvent::PeerDisconnected(peer_id) => {
                        return Ok(Async::Ready(Some(Libp2pEvent::PeerDisconnected(peer_id))));
                    }
                    BehaviourEvent::PeerSubscribed(peer_id, topic) => {
                        return Ok(Async::Ready(Some(Libp2pEvent::PeerSubscribed(
                            peer_id, topic,
                        ))));
                    }
                },
                Ok(Async::Ready(None)) => unreachable!("Swarm stream shouldn't end"),
                Ok(Async::NotReady) => break,
                _ => break,
            }
        }
        // swarm is not ready
        // check to see if the address is different to the config. If so, update our ENR
        if !self.verified_listen_address {
            let multiaddr = Swarm::listeners(&self.swarm).next();
            if let Some(multiaddr) = multiaddr {
                self.verified_listen_address = true;
                if let Some(socket_addr) = multiaddr_to_socket_addr(multiaddr) {
                    self.swarm.update_local_enr_socket(socket_addr, true);
                }
            }
        }

        // check if there are peers to ban
        while !self.peers_to_ban.is_empty() {
            if self.peers_to_ban[0].1 < Instant::now() {
                let (peer_id, _) = self.peers_to_ban.remove(0);
                warn!(self.log, "Disconnecting and banning peer"; "peer_id" => format!("{:?}", peer_id));
                Swarm::ban_peer_id(&mut self.swarm, peer_id.clone());
                // TODO: Correctly notify protocols of the disconnect
                // TODO: Also remove peer from the DHT: https://github.com/sigp/lighthouse/issues/629
                let dummy_connected_point = ConnectedPoint::Dialer {
                    address: "/ip4/0.0.0.0"
                        .parse::<Multiaddr>()
                        .expect("valid multiaddr"),
                };
                self.swarm
                    .inject_disconnected(&peer_id, dummy_connected_point);
                // inform the behaviour that the peer has been banned
                self.swarm.peer_banned(peer_id);
            } else {
                break;
            }
        }

        Ok(Async::NotReady)
    }
}

/// Converts a multiaddr to a `SocketAddr` if the multiaddr has the TCP/IP form. Libp2p currently
/// only supports TCP, so the UDP case is currently ignored.
fn multiaddr_to_socket_addr(multiaddr: &Multiaddr) -> Option<std::net::SocketAddr> {
    let protocols = multiaddr.iter().collect::<Vec<_>>();
    // assume the IP protocol
    match protocols[0] {
        Protocol::Ip4(address) => {
            if let Protocol::Tcp(port) = protocols[1] {
                Some(std::net::SocketAddr::new(address.into(), port))
            } else {
                None
            }
        }
        Protocol::Ip6(address) => {
            if let Protocol::Tcp(port) = protocols[1] {
                Some(std::net::SocketAddr::new(address.into(), port))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The implementation supports TCP/IP, WebSockets over TCP/IP, secio as the encryption layer, and
/// mplex or yamux as the multiplexing layer.
fn build_transport(local_private_key: Keypair) -> Boxed<(PeerId, StreamMuxerBox), Error> {
    // TODO: The Wire protocol currently doesn't specify encryption and this will need to be customised
    // in the future.
    let transport = libp2p::tcp::TcpConfig::new().nodelay(true);
    let transport = libp2p::dns::DnsConfig::new(transport);
    #[cfg(feature = "libp2p-websocket")]
    let transport = {
        let trans_clone = transport.clone();
        transport.or_transport(websocket::WsConfig::new(trans_clone))
    };
    transport
        .upgrade(core::upgrade::Version::V1)
        .authenticate(secio::SecioConfig::new(local_private_key))
        .multiplex(core::upgrade::SelectUpgrade::new(
            libp2p::yamux::Config::default(),
            libp2p::mplex::MplexConfig::new(),
        ))
        .map(|(peer, muxer), _| (peer, core::muxing::StreamMuxerBox::new(muxer)))
        .timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(20))
        .map_err(|err| Error::new(ErrorKind::Other, err))
        .boxed()
}

/// Events that can be obtained from polling the Libp2p Service.
pub enum Libp2pEvent {
    /// An RPC response request has been received on the swarm.
    RPC(PeerId, RPCEvent),
    /// Initiated the connection to a new peer.
    PeerDialed(PeerId),
    /// A peer has disconnected.
    PeerDisconnected(PeerId),
    /// Received pubsub message.
    PubsubMessage {
        id: String,
        source: PeerId,
        topics: Vec<TopicHash>,
        message: PubsubMessage,
    },
    /// Subscribed to peer for a topic hash.
    PeerSubscribed(PeerId, TopicHash),
}

fn keypair_from_hex(hex_bytes: &str) -> error::Result<Keypair> {
    let hex_bytes = if hex_bytes.starts_with("0x") {
        hex_bytes[2..].to_string()
    } else {
        hex_bytes.to_string()
    };

    hex::decode(&hex_bytes)
        .map_err(|e| format!("Failed to parse p2p secret key bytes: {:?}", e).into())
        .and_then(keypair_from_bytes)
}

fn keypair_from_bytes(mut bytes: Vec<u8>) -> error::Result<Keypair> {
    libp2p::core::identity::secp256k1::SecretKey::from_bytes(&mut bytes)
        .map(|secret| {
            let keypair: libp2p::core::identity::secp256k1::Keypair = secret.into();
            Keypair::Secp256k1(keypair)
        })
        .map_err(|e| format!("Unable to parse p2p secret key: {:?}", e).into())
}

/// Loads a private key from disk. If this fails, a new key is
/// generated and is then saved to disk.
///
/// Currently only secp256k1 keys are allowed, as these are the only keys supported by discv5.
fn load_private_key(config: &NetworkConfig, log: &slog::Logger) -> Keypair {
    // TODO: Currently using secp256k1 keypairs - currently required for discv5
    // check for key from disk
    let network_key_f = config.network_dir.join(NETWORK_KEY_FILENAME);
    if let Ok(mut network_key_file) = File::open(network_key_f.clone()) {
        let mut key_bytes: Vec<u8> = Vec::with_capacity(36);
        match network_key_file.read_to_end(&mut key_bytes) {
            Err(_) => debug!(log, "Could not read network key file"),
            Ok(_) => {
                // only accept secp256k1 keys for now
                if let Ok(secret_key) =
                    libp2p::core::identity::secp256k1::SecretKey::from_bytes(&mut key_bytes)
                {
                    let kp: libp2p::core::identity::secp256k1::Keypair = secret_key.into();
                    debug!(log, "Loaded network key from disk.");
                    return Keypair::Secp256k1(kp);
                } else {
                    debug!(log, "Network key file is not a valid secp256k1 key");
                }
            }
        }
    }

    // if a key could not be loaded from disk, generate a new one and save it
    let local_private_key = Keypair::generate_secp256k1();
    if let Keypair::Secp256k1(key) = local_private_key.clone() {
        let _ = std::fs::create_dir_all(&config.network_dir);
        match File::create(network_key_f.clone())
            .and_then(|mut f| f.write_all(&key.secret().to_bytes()))
        {
            Ok(_) => {
                debug!(log, "New network key generated and written to disk");
            }
            Err(e) => {
                warn!(
                    log,
                    "Could not write node key to file: {:?}. error: {}", network_key_f, e
                );
            }
        }
    }
    local_private_key
}
