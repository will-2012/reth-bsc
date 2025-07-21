//! Test suite for BSC engine API validation

use alloy_primitives::{Address, Bytes, B256, U256};
use alloy_rpc_types_engine::{ExecutionData, ExecutionDataV1};
use reth_bsc::{
    chainspec::bsc::bsc_mainnet,
    consensus::parlia::{InMemorySnapshotProvider, Snapshot, SnapshotProvider},
    node::rpc::engine_api::validator::BscEngineValidator,
};
use reth_engine_primitives::PayloadValidator;
use std::sync::Arc;

/// Create a test execution payload
fn create_test_payload() -> ExecutionData {
    ExecutionData::V1(ExecutionDataV1 {
        parent_hash: B256::default(),
        fee_recipient: Address::repeat_byte(0x01),
        state_root: B256::default(),
        receipts_root: B256::default(),
        logs_bloom: alloy_primitives::Bloom::default(),
        prev_randao: B256::default(),
        block_number: 1,
        gas_limit: 30_000_000,
        gas_used: 0,
        timestamp: 1000,
        extra_data: Bytes::default(),
        base_fee_per_gas: U256::from(1_000_000_000),
        block_hash: B256::default(),
        transactions: vec![],
    })
}

#[test]
fn test_engine_validator_creation() {
    let snapshot_provider = Arc::new(InMemorySnapshotProvider::default());
    let chain_spec = Arc::new(bsc_mainnet());
    
    let validator = BscEngineValidator::new(snapshot_provider, chain_spec);
    
    // Validator should be created successfully
    println!("✓ Engine validator created successfully");
}

#[test]
fn test_valid_payload_validation() {
    let snapshot_provider = Arc::new(InMemorySnapshotProvider::default());
    let chain_spec = Arc::new(bsc_mainnet());
    
    // Add a snapshot for block 0 (parent of our test block)
    let mut snapshot = Snapshot::default();
    snapshot.validators.push(Address::repeat_byte(0x01));
    snapshot.block_number = 0;
    snapshot_provider.insert(snapshot);
    
    let validator = BscEngineValidator::new(snapshot_provider, chain_spec);
    
    let payload = create_test_payload();
    
    // Should validate successfully
    match validator.ensure_well_formed_payload(payload) {
        Ok(recovered_block) => {
            assert_eq!(recovered_block.block.header.number, 1);
            assert_eq!(recovered_block.block.header.beneficiary, Address::repeat_byte(0x01));
            println!("✓ Valid payload validated successfully");
        }
        Err(e) => panic!("Valid payload should pass validation: {}", e),
    }
}

#[test]
fn test_invalid_payload_no_validator() {
    let snapshot_provider = Arc::new(InMemorySnapshotProvider::default());
    let chain_spec = Arc::new(bsc_mainnet());
    
    // Add a snapshot with different validators (not including our beneficiary)
    let mut snapshot = Snapshot::default();
    snapshot.validators.push(Address::repeat_byte(0x02));
    snapshot.validators.push(Address::repeat_byte(0x03));
    snapshot.block_number = 0;
    snapshot_provider.insert(snapshot);
    
    let validator = BscEngineValidator::new(snapshot_provider, chain_spec);
    
    let payload = create_test_payload(); // beneficiary is 0x01, not in validator set
    
    // Should fail validation
    match validator.ensure_well_formed_payload(payload) {
        Ok(_) => panic!("Payload with unauthorized validator should fail"),
        Err(e) => {
            let error_msg = format!("{}", e);
            assert!(error_msg.contains("unauthorised validator"));
            println!("✓ Invalid payload with unauthorized validator rejected");
        }
    }
}

#[test]
fn test_payload_with_transactions() {
    use alloy_consensus::{Transaction as _, TxLegacy};
    use alloy_rlp::Encodable;
    
    let snapshot_provider = Arc::new(InMemorySnapshotProvider::default());
    let chain_spec = Arc::new(bsc_mainnet());
    
    // Add a snapshot
    let mut snapshot = Snapshot::default();
    snapshot.validators.push(Address::repeat_byte(0x01));
    snapshot.block_number = 0;
    snapshot_provider.insert(snapshot);
    
    let validator = BscEngineValidator::new(snapshot_provider, chain_spec);
    
    // Create a transaction
    let tx = TxLegacy {
        chain_id: Some(56),
        nonce: 0,
        gas_price: 1_000_000_000,
        gas_limit: 21000,
        to: alloy_primitives::TxKind::Call(Address::repeat_byte(0x02)),
        value: U256::from(1_000_000_000_000_000_000u128), // 1 ETH
        input: Bytes::default(),
    };
    
    // Encode transaction
    let mut tx_bytes = Vec::new();
    tx.encode(&mut tx_bytes);
    
    let mut payload = create_test_payload();
    if let ExecutionData::V1(ref mut data) = payload {
        data.transactions.push(tx_bytes.into());
    }
    
    // Should validate successfully
    match validator.ensure_well_formed_payload(payload) {
        Ok(recovered_block) => {
            assert_eq!(recovered_block.block.body.transactions.len(), 1);
            println!("✓ Payload with transactions validated successfully");
        }
        Err(e) => panic!("Payload with valid transaction should pass: {}", e),
    }
}

#[test]
fn test_payload_difficulty_validation() {
    let snapshot_provider = Arc::new(InMemorySnapshotProvider::default());
    let chain_spec = Arc::new(bsc_mainnet());
    
    // Add a snapshot where 0x01 is the in-turn validator
    let mut snapshot = Snapshot::default();
    snapshot.validators.push(Address::repeat_byte(0x01));
    snapshot.validators.push(Address::repeat_byte(0x02));
    snapshot.block_number = 0;
    snapshot_provider.insert(snapshot);
    
    let validator = BscEngineValidator::new(snapshot_provider, chain_spec);
    
    // Test in-turn validator (should have difficulty 2)
    let mut payload = create_test_payload();
    if let ExecutionData::V1(ref mut data) = payload {
        data.difficulty = U256::from(2); // in-turn difficulty
    }
    
    match validator.ensure_well_formed_payload(payload.clone()) {
        Ok(_) => println!("✓ In-turn validator with correct difficulty validated"),
        Err(e) => panic!("In-turn validator should pass: {}", e),
    }
    
    // Test wrong difficulty
    if let ExecutionData::V1(ref mut data) = payload {
        data.difficulty = U256::from(1); // wrong difficulty for in-turn
    }
    
    match validator.ensure_well_formed_payload(payload) {
        Ok(_) => panic!("Wrong difficulty should fail validation"),
        Err(e) => {
            let error_msg = format!("{}", e);
            assert!(error_msg.contains("wrong difficulty"));
            println!("✓ Wrong difficulty rejected");
        }
    }
}

#[test]
fn test_empty_payload_validation() {
    let snapshot_provider = Arc::new(InMemorySnapshotProvider::default());
    let chain_spec = Arc::new(bsc_mainnet());
    
    // Genesis block (0) doesn't need validators
    let validator = BscEngineValidator::new(snapshot_provider, chain_spec);
    
    let mut payload = create_test_payload();
    if let ExecutionData::V1(ref mut data) = payload {
        data.block_number = 0; // Genesis block
    }
    
    // Genesis block should validate without snapshot
    match validator.ensure_well_formed_payload(payload) {
        Ok(recovered_block) => {
            assert_eq!(recovered_block.block.header.number, 0);
            println!("✓ Genesis block validated successfully");
        }
        Err(e) => panic!("Genesis block should pass validation: {}", e),
    }
} 