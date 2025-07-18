use clap::{Args, Parser};
use reth_bsc::{
    chainspec::parser::BscChainSpecParser,
    node::{consensus::BscConsensus, evm::config::BscEvmConfig, BscNode},
};
use reth::{builder::NodeHandle, cli::Cli};
use reth_cli_util::sigsegv_handler;
use reth_network_api::NetworkInfo;
use tracing::info;

// We use jemalloc for performance reasons
#[cfg(all(feature = "jemalloc", unix))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// BSC Reth CLI arguments
#[derive(Debug, Clone, Default, Args)]
#[non_exhaustive]
pub struct BscArgs {
    /// Enable debug logging
    #[arg(long)]
    pub debug: bool,
    
    /// Enable validator mode 
    #[arg(long)]
    pub validator: bool,
}

fn main() -> eyre::Result<()> {
    sigsegv_handler::install();

    // Enable backtraces unless a RUST_BACKTRACE value has already been explicitly provided.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    println!("🚀 BSC Reth - High Performance BSC Client");
    println!("Version: {}", env!("CARGO_PKG_VERSION"));
    println!("🌐 Starting with BSC consensus...");

    Cli::<BscChainSpecParser, BscArgs>::parse().run_with_components::<BscNode>(
        |spec| (BscEvmConfig::new(spec.clone()), BscConsensus::new(spec)),
        async move |builder, args| {
            if args.debug {
                info!("🐛 Debug mode enabled");
            }
            
            if args.validator {
                info!("⚡ Validator mode enabled");
            }

            let (node, engine_handle_tx) = BscNode::new();
            let NodeHandle { node, node_exit_future: exit_future } =
                builder.node(node).launch().await?;

            engine_handle_tx.send(node.beacon_engine_handle.clone()).unwrap();
            
            info!("✅ BSC Reth node started successfully!");
            info!("📡 P2P listening on: {}", node.network.local_addr());
            
            exit_future.await
        },
    )?;
    Ok(())
}


