use clap::{Args, Parser};
use reth::{builder::NodeHandle, cli::Cli};
use reth_bsc::{
    chainspec::parser::BscChainSpecParser,
    node::{evm::config::BscEvmConfig, BscNode},
    consensus::parlia::{ParliaConsensus, EPOCH},
};
use std::sync::Arc;

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
            // Create components: (EVM config, Consensus)
            // Note: Consensus will be created by BscConsensusBuilder with correct datadir
            let evm_config = BscEvmConfig::new(spec.clone());
            
            // Create a temporary consensus for CLI components
            // This will be replaced by BscConsensusBuilder's consensus with proper database
            use reth_bsc::consensus::parlia::provider::InMemorySnapshotProvider;
            let temp_provider = Arc::new(InMemorySnapshotProvider::new(1));
            let consensus = ParliaConsensus::new(spec, temp_provider, EPOCH);
            
            (evm_config, consensus)
        },
        async move |builder, _| {
            // Create node with proper engine handle communication (matches official BSC)
            let (node, engine_handle_tx) = BscNode::new();
            
            let NodeHandle { node, node_exit_future: exit_future } =
                builder.node(node)
                    .extend_rpc_modules(move |ctx| {
                        // 🚀 [BSC] Register Parlia RPC API for snapshot queries
                        use reth_bsc::rpc::parlia::{ParliaApiImpl, ParliaApiServer, DynSnapshotProvider};


                        tracing::info!("🚀 [BSC] Registering Parlia RPC API: parlia_getSnapshot");
                        
                        // Get the snapshot provider from the global shared instance
                        let snapshot_provider = if let Some(provider) = reth_bsc::shared::get_snapshot_provider() {
                            tracing::info!("✅ [BSC] Using shared persistent snapshot provider from consensus builder");
                            provider.clone()
                        } else {
                            // Fallback to an empty in-memory provider
                            tracing::error!("❌ [BSC] Shared snapshot provider not available, using fallback");
                            use reth_bsc::consensus::parlia::{InMemorySnapshotProvider, SnapshotProvider};
                            Arc::new(InMemorySnapshotProvider::new(1000)) as Arc<dyn SnapshotProvider + Send + Sync>
                        };
                        
                        let wrapped_provider = Arc::new(DynSnapshotProvider::new(snapshot_provider));
                        let parlia_api = ParliaApiImpl::new(wrapped_provider);
                        ctx.modules.merge_configured(parlia_api.into_rpc())?;

                        tracing::info!("✅ [BSC] Parlia RPC API registered successfully!");
                        Ok(())
                    })
                    .launch().await?;

            // Send the engine handle to the network
            engine_handle_tx.send(node.beacon_engine_handle.clone()).unwrap();

            exit_future.await
        },
    )?;
    Ok(())
}