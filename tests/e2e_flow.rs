use std::sync::Arc;

use reth_bsc::{chainspec::bsc::bsc_mainnet, node::BscNode, chainspec::BscChainSpec};
use reth_e2e_test_utils::setup_engine;
use reth_node_api::TreeConfig;

#[tokio::test]
async fn bsc_e2e_produce_blocks() -> eyre::Result<()> {
    // Ensure tracing is initialised for easier debugging when tests fail.
    reth_tracing::init_test_tracing();

    // Create a simple BSC-specific payload attributes generator
    let bsc_attributes_generator = |timestamp: u64| {
        use reth_payload_builder::EthPayloadBuilderAttributes;
        use alloy_rpc_types_engine::PayloadAttributes;
        use alloy_primitives::{B256, Address};
        
        let attrs = PayloadAttributes {
            timestamp,
            prev_randao: B256::random(),
            suggested_fee_recipient: Address::random(),
            withdrawals: None, // BSC doesn't support withdrawals
            parent_beacon_block_root: None,
        };
        
        // Convert to BSC payload builder attributes
        reth_bsc::node::rpc::engine_api::payload::BscPayloadBuilderAttributes::from(
            EthPayloadBuilderAttributes::new(B256::ZERO, attrs)
        )
    };

    // Set up a single BSC node with our custom attributes generator
    let chain_spec = Arc::new(BscChainSpec { inner: bsc_mainnet() });
    let (mut nodes, _task_manager, _wallet) = setup_engine::<BscNode>(
        1,
        chain_spec,
        true,
        TreeConfig::default(),
        bsc_attributes_generator,
    ).await?;

    let node = &mut nodes[0];
    
    // Try to build 2 empty blocks using the payload builder directly
    for i in 0..2 {
        let payload = node.new_payload().await?;
        node.submit_payload(payload).await?;
        println!("Successfully built and submitted block {}", i + 1);
    }

    Ok(())
} 