use std::sync::Arc;

use reth_bsc::{chainspec::bsc::bsc_mainnet, node::BscNode, chainspec::BscChainSpec};
use reth_e2e_test_utils::setup_engine;
use reth_node_api::{TreeConfig, PayloadBuilderAttributes, BuiltPayload};

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
    
    // Try building 2 blocks to verify everything works
    println!("Trying to build 2 blocks...");
    
    for i in 0..2 {
        println!("Building block {}", i + 1);
        
        // Use the proper new_payload method from NodeTestContext
        // This handles the entire flow internally
        match node.new_payload().await {
            Ok(payload) => {
                println!("✓ Successfully created payload with {} transactions", 
                        payload.block().body().transactions().count());
                
                // Submit the payload
                node.submit_payload(payload).await?;
                println!("✓ Successfully submitted block {}", i + 1);
            }
            Err(e) => {
                println!("✗ Failed to build payload: {:?}", e);
                
                // Let's try to understand what's happening
                println!("Error details: {:#}", e);
                
                // Check if it's the unwrap error we saw before
                if e.to_string().contains("called `Option::unwrap()` on a `None` value") {
                    println!("This is the 'None' unwrap error - payload builder is not producing payloads");
                    println!("This suggests our SimpleBscPayloadBuilder might not be working correctly");
                }
                
                return Err(e);
            }
        }
    }

    println!("✓ E2E test completed successfully!");
    Ok(())
} 