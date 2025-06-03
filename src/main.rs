use clap::{Args, Parser};
use reth::builder::NodeHandle;
use reth_bsc::{
    chainspec::parser::BscChainSpecParser,
    node::{cli::Cli, BscNode},
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

    Cli::<BscChainSpecParser, NoArgs>::parse().run(|builder, _| async move {
        let (node, engine_handle_tx) = BscNode::new();
        let NodeHandle { node, node_exit_future: exit_future } =
            builder.node(node).launch().await?;

        engine_handle_tx.send(node.beacon_engine_handle.clone()).unwrap();

        exit_future.await
    })?;
    Ok(())
}
