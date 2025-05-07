use std::{path::PathBuf, pin::pin, sync::Arc, time::Instant};

use anyhow::{Result, anyhow};
use async_lock::Semaphore;
use clap::{Parser, ValueHint};
use futures::{FutureExt, select};
use identity::Identity;
use network::{NetworkArgs, configure_network};
use rpc_node_client::RpcNodeClient;
use rustyline::{Config, Editor, error::ReadlineError};
use subspace_core_primitives::pieces::PieceIndex;
use subspace_networking::{
    Node,
    utils::piece_provider::{NoPieceValidator, PieceProvider},
};
use tracing::{error, info};
use utils::{derive_libp2p_keypair, run_future_in_dedicated_thread, shutdown_signal};

mod identity;
mod logging;
mod network;
mod rpc_node_client;
mod utils;

#[derive(Parser)]
struct Cli {
    #[clap(long, default_value = ".")]
    base_path: PathBuf,
    /// WebSocket RPC URL of the Subspace node to connect to
    #[arg(long, value_hint = ValueHint::Url, default_value = "ws://127.0.0.1:9944")]
    node_rpc_url: String,
    /// Network parameters
    #[clap(flatten)]
    network_args: NetworkArgs,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    logging::init();
    let signal = shutdown_signal();

    let mut network_args = cli.network_args;
    info!(url = %cli.node_rpc_url, "Connecting to node RPC");
    let node_client = RpcNodeClient::new(&cli.node_rpc_url)
        .await
        .map_err(|error| anyhow!("Failed to connect to node RPC: {error}"))?;

    let farmer_app_info = node_client
        .farmer_app_info()
        .await
        .map_err(|error| anyhow!("Failed to get farmer app info: {error}"))?;

    let identity = Identity::open_or_create(&cli.base_path)?;
    let keypair = derive_libp2p_keypair(identity.secret_key());
    let (node, mut node_runner) = {
        if network_args.bootstrap_nodes.is_empty() {
            network_args
                .bootstrap_nodes
                .clone_from(&farmer_app_info.dsn_bootstrap_nodes);
        }

        configure_network(
            hex::encode(farmer_app_info.genesis_hash),
            &cli.base_path,
            keypair,
            network_args,
        )
        .map_err(|error| anyhow!("Failed to configure networking: {error}"))?
    };

    let run_interactive_cli_fut = run_interactive_cli(node);

    let networking_fut = run_future_in_dedicated_thread(
        move || async move { node_runner.run().await },
        "dsn-networking".to_string(),
    )?;

    // This defines order in which things are dropped
    let networking_fut = networking_fut;
    let run_interactive_cli_fut = run_interactive_cli_fut;

    let networking_fut = pin!(networking_fut);
    let run_interactive_cli_fut = pin!(run_interactive_cli_fut);

    select! {
        // Signal future
        _ = signal.fuse() => {},

        // Networking future
        _ = networking_fut.fuse() => {
            info!("Node runner exited.")
        },

        // Networking future
        result = run_interactive_cli_fut.fuse() => {
            result?;
        },
    }

    Ok(())
}

async fn run_interactive_cli(node: Node) -> Result<()> {
    let config = Config::builder().auto_add_history(true).build();
    let history = rustyline::sqlite_history::SQLiteHistory::with_config(config)?;

    let mut rl: Editor<(), _> = Editor::with_history(config, history)?;
    let piece_downloading_semaphore = Arc::new(Semaphore::new(100));
    let piece_provider =
        PieceProvider::new(node.clone(), NoPieceValidator, piece_downloading_semaphore);

    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => match line.as_ref() {
                "show listeners" => {
                    for addr in node.listeners() {
                        println!("{addr}");
                    }
                }
                "show connected_peers" => {
                    for peer_id in node.connected_peers().await? {
                        println!("{peer_id}");
                    }
                }
                "show connected_servers" => {
                    for peer_id in node.connected_servers().await? {
                        println!("{peer_id}");
                    }
                }
                download if download.starts_with("download ") => {
                    let Ok(piece_index) = download.trim_start_matches("download ").parse::<u64>()
                    else {
                        println!("Invalid piece index");
                        continue;
                    };
                    let now = Instant::now();
                    let piece = piece_provider
                        .get_piece_from_archival_storage(PieceIndex::from(piece_index), 100)
                        .await;
                    let elapsed = now.elapsed();
                    match piece {
                        Some(_) => println!("Piece {piece_index} downloaded: elapsed: {elapsed:?}."),
                        None => println!("No piece found for index {piece_index}"),
                    }
                }
                "exit" => break,
                "quit" => break,

                _ => {
                    println!("Unknown command: {line}");
                }
            },
            Err(ReadlineError::Interrupted) => {}
            Err(ReadlineError::Eof) => break,
            Err(err) => {
                error!("Error: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}
