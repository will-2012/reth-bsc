use reth_bsc::chainspec::bsc;
use reth_bsc::consensus::parlia::{ParliaConsensus, InMemorySnapshotProvider, EPOCH, SnapshotProvider};
use std::sync::Arc;
use alloy_consensus::BlockHeader;
use alloy_primitives::hex;
use reth_chainspec::EthChainSpec;

fn main() -> eyre::Result<()> {
    println!("🚀 Testing BSC Mainnet Genesis Snapshot Creation");

    // Create BSC mainnet chain spec
    let chain_spec = bsc::bsc_mainnet();
    let bsc_chain_spec = reth_bsc::chainspec::BscChainSpec { inner: chain_spec };
    let bsc_chain_spec_arc = Arc::new(bsc_chain_spec);

    // Create consensus with in-memory snapshot provider
    let snapshot_provider = Arc::new(InMemorySnapshotProvider::new(100));
    let _consensus = ParliaConsensus::new(
        bsc_chain_spec_arc.clone(),
        snapshot_provider.clone(),
        EPOCH,
        3
    );

    // Check if genesis snapshot was created
    if let Some(genesis_snapshot) = snapshot_provider.snapshot(0) {
        println!("🎯 Genesis snapshot created successfully!");
        println!("   Number of validators: {}", genesis_snapshot.validators.len());
        println!("   Block number: {}", genesis_snapshot.block_number);
        println!("   Block hash: {:#x}", genesis_snapshot.block_hash);
        println!("   Epoch: {}", genesis_snapshot.epoch_num);
        
        println!("\n✅ Validators in genesis snapshot:");
        for (i, validator) in genesis_snapshot.validators.iter().enumerate() {
            println!("      {}. {:#x}", i + 1, validator);
        }
    } else {
        println!("❌ Genesis snapshot was not created");
    }

    // Test extraData length
    println!("\n🔍 Testing BSC mainnet extraData...");
    let genesis_header = bsc_chain_spec_arc.genesis_header();
    let extra_data = genesis_header.extra_data();
    println!("   ExtraData length: {} bytes", extra_data.len());
    
    if extra_data.len() > 97 {
        println!("   ✅ ExtraData has sufficient length for validators");
        
        // Analyze the structure
        let vanity_len = 32;
        let seal_len = 65;
        let validator_data_len = extra_data.len() - vanity_len - seal_len;
        let validator_count = validator_data_len / 20;
        
        println!("   Validator data section: {} bytes", validator_data_len);
        println!("   Expected validator count: {}", validator_count);
    } else {
        println!("   ❌ ExtraData length insufficient");
    }

    Ok(())
}