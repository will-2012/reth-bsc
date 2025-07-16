use std::sync::Arc;

use reth_bsc::{chainspec::bsc::bsc_mainnet, node::BscNode};
use reth_bsc::node::rpc::engine_api::payload::BscPayloadTypes;

use reth_e2e_test_utils::testsuite::{
    actions::{MakeCanonical, ProduceBlocks},
    setup::{NetworkSetup, Setup},
    TestBuilder,
};

#[tokio::test]
async fn bsc_e2e_produce_blocks() -> eyre::Result<()> {
    // Ensure tracing is initialised for easier debugging when tests fail.
    reth_tracing::init_test_tracing();

    // Configure a single-node setup running on the BSC mainnet chain-spec.
    let setup = Setup::<BscPayloadTypes>::default()
        .with_chain_spec(Arc::new(bsc_mainnet()))
        .with_network(NetworkSetup::single_node());

    // Build the test: produce two blocks and make them canonical.
    let test = TestBuilder::new()
        .with_setup(setup)
        .with_action(ProduceBlocks::<BscPayloadTypes>::new(2))
        .with_action(MakeCanonical::new());

    // Launch the node(s) and run the scripted actions.
    test.run::<BscNode>().await
} 