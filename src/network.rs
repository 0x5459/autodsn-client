use clap::Parser;
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::sync::Arc;
use subspace_networking::libp2p::Multiaddr;
use subspace_networking::libp2p::identity::Keypair;
use subspace_networking::libp2p::multiaddr::Protocol;
use subspace_networking::protocols::request_response::handlers::cached_piece_by_index::{
    CachedPieceByIndexRequest, CachedPieceByIndexRequestHandler,
};
use subspace_networking::protocols::request_response::handlers::piece_by_index::{
    PieceByIndexRequest, PieceByIndexRequestHandler,
};
use subspace_networking::protocols::request_response::handlers::segment_header::SegmentHeaderBySegmentIndexesRequestHandler;
use subspace_networking::utils::strip_peer_id;
use subspace_networking::{
    Config, KademliaMode, KnownPeersManager, KnownPeersManagerConfig, Node, NodeRunner, construct,
};
use tracing::{debug, info};

/// Configuration for network stack
#[derive(Debug, Parser)]
pub(crate) struct NetworkArgs {
    /// Multiaddrs of bootstrap nodes to connect to on startup, multiple are supported
    #[arg(long = "bootstrap-node")]
    pub(crate) bootstrap_nodes: Vec<Multiaddr>,
    /// Multiaddrs to listen on for subspace networking, for instance `/ip4/0.0.0.0/tcp/0`,
    /// multiple are supported.
    #[arg(long, default_values_t = [
        Multiaddr::from(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Tcp(30533)),
        Multiaddr::from(IpAddr::V6(Ipv6Addr::UNSPECIFIED))
            .with(Protocol::Tcp(30533))
    ])]
    pub(crate) listen_on: Vec<Multiaddr>,
    /// Enable non-global (private, shared, loopback..) addresses in Kademlia DHT.
    /// By default, these addresses are excluded from the DHT.
    #[arg(long, default_value_t = false)]
    pub(crate) allow_private_ips: bool,
    /// Multiaddrs of reserved nodes to maintain a connection to, multiple are supported
    #[arg(long)]
    pub(crate) reserved_peers: Vec<Multiaddr>,
    /// Maximum established incoming connection limit.
    #[arg(long, default_value_t = 300)]
    pub(crate) in_connections: u32,
    /// Maximum established outgoing swarm connection limit.
    #[arg(long, default_value_t = 100)]
    pub(crate) out_connections: u32,
    /// Maximum pending incoming connection limit.
    #[arg(long, default_value_t = 100)]
    pub(crate) pending_in_connections: u32,
    /// Maximum pending outgoing swarm connection limit.
    #[arg(long, default_value_t = 100)]
    pub(crate) pending_out_connections: u32,
    /// Known external addresses.
    #[arg(long = "external-address")]
    pub(crate) external_addresses: Vec<Multiaddr>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn configure_network(
    protocol_prefix: String,
    base_path: &Path,
    keypair: Keypair,
    NetworkArgs {
        listen_on,
        bootstrap_nodes,
        allow_private_ips,
        reserved_peers,
        in_connections,
        out_connections,
        pending_in_connections,
        pending_out_connections,
        external_addresses,
    }: NetworkArgs,
) -> Result<(Node, NodeRunner), anyhow::Error>
{
    let known_peers_registry = KnownPeersManager::new(KnownPeersManagerConfig {
        path: Some(base_path.join("known_addresses.bin").into_boxed_path()),
        ignore_peer_list: strip_peer_id(bootstrap_nodes.clone())
            .into_iter()
            .map(|(peer_id, _)| peer_id)
            .collect::<HashSet<_>>(),
        cache_size: 100,
        ..Default::default()
    })
    .map(Box::new)?;

    let default_config = Config::new(protocol_prefix, keypair, None);
    let config = Config {
        reserved_peers,
        listen_on,
        allow_non_global_addresses_in_dht: allow_private_ips,
        known_peers_registry,
        request_response_protocols: vec![
            CachedPieceByIndexRequestHandler::create(move |peer_id, request| {
                let CachedPieceByIndexRequest { piece_index, .. } = request;
                debug!(?piece_index, ?peer_id, "Cached piece request received");
                async { None }
            }),
            PieceByIndexRequestHandler::create(move |_, request| {
                let PieceByIndexRequest { piece_index, .. } = request;
                debug!(?piece_index, "Piece request received. Trying cache...");
                async { None }
            }),
            SegmentHeaderBySegmentIndexesRequestHandler::create(move |_, req| {
                debug!(?req, "Segment headers request received.");
                async { None }
            }),
        ],
        max_established_outgoing_connections: out_connections,
        max_pending_outgoing_connections: pending_out_connections,
        max_established_incoming_connections: in_connections,
        max_pending_incoming_connections: pending_in_connections,
        bootstrap_addresses: bootstrap_nodes,
        kademlia_mode: KademliaMode::Dynamic,
        external_addresses,
        ..default_config
    };

    let (node, node_runner) = construct(config)?;

    node.on_new_listener(Arc::new({
        let node = node.clone();

        move |address| {
            info!(
                "DSN listening on {}",
                address.clone().with(Protocol::P2p(node.id()))
            );
        }
    }))
    .detach();

    // Consider returning HandlerId instead of each `detach()` calls for other usages.
    Ok((node, node_runner))
}
