use clap::{Args, Parser};
use reth::{builder::NodeHandle, cli::Cli};
use reth_bsc::{
    chainspec::parser::BscChainSpecParser,
    node::{evm::config::BscEvmConfig, BscNode},
};

// We use jemalloc for performance reasons
#[cfg(all(feature = "jemalloc", unix))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// No Additional arguments
#[derive(Debug, Clone, Copy, Default, Args)]
#[non_exhaustive]
struct NoArgs;

fn main() -> eyre::Result<()> {
    reth_cli_util::sigsegv_handler::install();

    // Enable backtraces unless a RUST_BACKTRACE value has already been explicitly provided.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    Cli::<BscChainSpecParser, NoArgs>::parse().run_with_components::<BscNode>(
        |spec| {
            // 🚀 Create enhanced ParliaConsensus with PERSISTENT MDBX snapshots for CLI fullnode!
            use reth_bsc::consensus::parlia::{
                provider::DbSnapshotProvider, 
                ParliaConsensus, EPOCH
            };
            use reth_db::{init_db, mdbx::DatabaseArguments};

            use reth_chainspec::EthChainSpec;
            use std::sync::Arc;

            tracing::info!("🚀 [BSC] CLI: Creating fullnode with persistent MDBX snapshots");

            // Create database path for persistent snapshots in the same datadir as the main node
            // This ensures proper permissions and avoids conflicts
            use reth_node_core::dirs::data_dir;
            let base_dir = data_dir().unwrap_or_else(|| {
                // On macOS, use ~/Library/Application Support/reth as fallback
                dirs::data_dir()
                    .map(|d| d.join("reth"))
                    .unwrap_or_else(|| std::env::current_dir().unwrap().join("data"))
            });
            let db_path = base_dir.join(spec.chain().to_string()).join("parlia_snapshots");
            
            // Ensure the parent directory exists
            if let Err(e) = std::fs::create_dir_all(&db_path) {
                panic!("Failed to create snapshot database directory at {:?}: {}", db_path, e);
            }

            // Initialize persistent MDBX database for snapshots
            let snapshot_db = init_db(&db_path, DatabaseArguments::new(Default::default()))
                .unwrap_or_else(|e| {
                    panic!("Failed to initialize snapshot database at {:?}: {}", db_path, e);
                });

            tracing::info!("🚀 [BSC] CLI: SNAPSHOT DATABASE READY! Using DbSnapshotProvider with MDBX persistence");
            let snapshot_provider = Arc::new(DbSnapshotProvider::new(Arc::new(snapshot_db), 2048));
            let consensus = ParliaConsensus::new(spec.clone(), snapshot_provider, EPOCH, 3);
            (BscEvmConfig::new(spec.clone()), consensus)
        },
        async move |builder, _| {
            // Create a simple node without the complex engine handle setup
            // The consensus was already provided in the components above
            let node = BscNode::default();
            let NodeHandle { node, node_exit_future: exit_future } =
                builder.node(node).launch().await?;

            exit_future.await
        },
    )?;
    Ok(())
}


