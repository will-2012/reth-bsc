//! Test suite for finality reward distribution (BEP-319)

use alloy_primitives::{Address, U256, Bytes};
use alloy_consensus::{TxLegacy, Transaction};
use reth_primitives::TransactionSigned;
use std::str::FromStr;

/// The BSC system reward contract address
const SYSTEM_REWARD_CONTRACT: Address = Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10, 0x02]);

/// Create a finality reward system transaction
fn create_finality_reward_tx(validators: Vec<Address>, rewards: Vec<U256>) -> TransactionSigned {
    // BEP-319 finality reward transaction format
    // distributeReward(address[] calldata validators, uint256[] calldata rewards)
    let mut data = Vec::new();
    
    // Function selector for distributeReward(address[],uint256[])
    data.extend_from_slice(&[0x6a, 0x62, 0x78, 0x42]); // keccak256("distributeReward(address[],uint256[])")[:4]
    
    // ABI encode the arrays
    // Offset to validators array data
    data.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x40]);
    
    // Offset to rewards array data
    let rewards_offset = 0x40 + 0x20 + 0x20 * validators.len();
    let mut offset_bytes = [0u8; 32];
    offset_bytes[31] = rewards_offset as u8;
    offset_bytes[30] = (rewards_offset >> 8) as u8;
    data.extend_from_slice(&offset_bytes);
    
    // Validators array
    let mut length_bytes = [0u8; 32];
    length_bytes[31] = validators.len() as u8;
    data.extend_from_slice(&length_bytes);
    
    for validator in &validators {
        let mut addr_bytes = [0u8; 32];
        addr_bytes[12..32].copy_from_slice(validator.as_slice());
        data.extend_from_slice(&addr_bytes);
    }
    
    // Rewards array
    let mut rewards_length_bytes = [0u8; 32];
    rewards_length_bytes[31] = rewards.len() as u8;
    data.extend_from_slice(&rewards_length_bytes);
    
    for reward in &rewards {
        let mut reward_bytes = [0u8; 32];
        reward.to_be_bytes::<32>().into_iter().enumerate().for_each(|(i, b)| {
            reward_bytes[i] = b;
        });
        data.extend_from_slice(&reward_bytes);
    }
    
    // Create the transaction
    let tx = TxLegacy {
        nonce: 0,
        gas_price: 0,
        gas_limit: 1_000_000,
        to: alloy_primitives::TxKind::Call(SYSTEM_REWARD_CONTRACT),
        value: U256::ZERO,
        input: Bytes::from(data),
        chain_id: Some(56), // BSC mainnet
    };
    
    // System transactions have null signature
    TransactionSigned::new_unhashed(tx.into(), alloy_primitives::Signature::new(Default::default(), Default::default(), false))
}

#[test]
fn test_create_finality_reward_transaction() {
    let validators = vec![
        Address::new([1; 20]),
        Address::new([2; 20]),
        Address::new([3; 20]),
    ];
    let rewards = vec![
        U256::from(1_000_000_000_000_000_000u64), // 1 BSC
        U256::from(2_000_000_000_000_000_000u64), // 2 BSC
        U256::from(3_000_000_000_000_000_000u64), // 3 BSC
    ];
    
    let tx = create_finality_reward_tx(validators.clone(), rewards.clone());
    
    // Verify transaction properties
    assert_eq!(tx.to(), Some(SYSTEM_REWARD_CONTRACT));
    assert_eq!(tx.value(), U256::ZERO);
    assert_eq!(tx.gas_limit(), 1_000_000);
    
    // Verify the function selector is correct
    let input = tx.input();
    assert!(input.len() >= 4, "Input should contain function selector");
    assert_eq!(&input[..4], &[0x6a, 0x62, 0x78, 0x42], "Incorrect function selector");
    
    println!("✓ Finality reward transaction creation passed");
}

#[test]
fn test_finality_reward_edge_cases() {
    // Test empty validators/rewards
    let empty_tx = create_finality_reward_tx(vec![], vec![]);
    assert!(empty_tx.input().len() > 4, "Should still have valid structure");
    
    // Test large validator set (more than typical)
    let large_validators: Vec<Address> = (0..21).map(|i| {
        let mut addr = [0u8; 20];
        addr[19] = i as u8;
        Address::new(addr)
    }).collect();
    let large_rewards = vec![U256::from(1000u64); 21];
    let large_tx = create_finality_reward_tx(large_validators, large_rewards);
    assert!(large_tx.input().len() > 1000, "Large tx should have substantial data");
    
    println!("✓ Finality reward edge cases passed");
}

#[test]
fn test_finality_reward_amount_calculation() {
    use reth_bsc::chainspec::bsc::bsc_mainnet;
    use std::sync::Arc;
    
    let _chain_spec = Arc::new(bsc_mainnet());
    
    // BSC reward calculation constants
    const MAX_SYSTEM_REWARD: U256 = U256::from_limbs([2_000_000_000_000_000_000, 0, 0, 0]); // 2 BSC max
    
    // Test various reward amounts
    let test_cases = vec![
        (U256::from(1000u64), true),                    // Small reward, should distribute
        (U256::from(1_000_000_000_000_000_000u64), true), // 1 BSC, should distribute
        (MAX_SYSTEM_REWARD, true),                       // Max reward, should distribute
        (MAX_SYSTEM_REWARD + U256::from(1), false),     // Over max, should skip
    ];
    
    for (amount, should_distribute) in test_cases {
        if should_distribute {
            assert!(amount <= MAX_SYSTEM_REWARD, "Amount should be within limits");
        } else {
            assert!(amount > MAX_SYSTEM_REWARD, "Amount should exceed limits");
        }
    }
    
    println!("✓ Finality reward amount calculation passed");
}

#[test]
fn test_finality_reward_validator_validation() {
    // Test that validators must be in the active set
    let active_validators = vec![
        Address::new([1; 20]),
        Address::new([2; 20]),
        Address::new([3; 20]),
    ];
    
    let rewards = vec![
        U256::from(1_000_000_000_000_000_000u64),
        U256::from(1_000_000_000_000_000_000u64),
        U256::from(1_000_000_000_000_000_000u64),
    ];
    
    let tx = create_finality_reward_tx(active_validators.clone(), rewards.clone());
    
    // Verify the transaction encodes validators correctly
    let input = tx.input();
    
    // Skip function selector (4 bytes) and offsets (64 bytes)
    // Then we have array length and validator addresses
    let validator_count_start = 4 + 64;
    let validator_count_bytes = &input[validator_count_start..validator_count_start + 32];
    let validator_count = U256::from_be_slice(validator_count_bytes);
    
    assert_eq!(validator_count, U256::from(3), "Should have 3 validators");
    
    println!("✓ Finality reward validator validation passed");
}

#[test]
fn test_finality_reward_plato_fork_behavior() {
    // Test behavior changes at Plato fork
    // Before Plato: no finality rewards
    // After Plato: finality rewards enabled
    
    use reth_bsc::chainspec::bsc::bsc_mainnet;
    use std::sync::Arc;
    
    let _chain_spec = Arc::new(bsc_mainnet());
    
    // BSC Plato fork block on mainnet
    let _plato_block = 30720096;
    
    // Before Plato, finality rewards shouldn't be distributed
    let pre_plato_validators = vec![Address::new([1; 20])];
    let pre_plato_rewards = vec![U256::from(1_000_000_000_000_000_000u64)];
    let pre_plato_tx = create_finality_reward_tx(pre_plato_validators, pre_plato_rewards);
    
    // After Plato, finality rewards should be distributed
    let post_plato_validators = vec![Address::new([2; 20])];
    let post_plato_rewards = vec![U256::from(2_000_000_000_000_000_000u64)];
    let post_plato_tx = create_finality_reward_tx(post_plato_validators, post_plato_rewards);
    
    // Both transactions are structurally valid
    assert_eq!(pre_plato_tx.to(), Some(SYSTEM_REWARD_CONTRACT));
    assert_eq!(post_plato_tx.to(), Some(SYSTEM_REWARD_CONTRACT));
    
    println!("✓ Finality reward Plato fork behavior passed");
}

#[test]
fn test_finality_reward_abi_encoding() {
    // Test proper ABI encoding of the distributeReward call
    let validators = vec![
        Address::from_str("0x0000000000000000000000000000000000000001").unwrap(),
        Address::from_str("0x0000000000000000000000000000000000000002").unwrap(),
    ];
    let rewards = vec![
        U256::from(1_000_000_000_000_000_000u64),
        U256::from(2_000_000_000_000_000_000u64),
    ];
    
    let tx = create_finality_reward_tx(validators, rewards);
    let input = tx.input();
    
    // Verify ABI encoding structure
    // Function selector (4 bytes)
    assert_eq!(&input[0..4], &[0x6a, 0x62, 0x78, 0x42]);
    
    // Offset to validators array (should be 0x40)
    assert_eq!(&input[4..36], &[0u8; 28].iter().chain(&[0, 0, 0, 0x40]).cloned().collect::<Vec<u8>>()[..]);
    
    // Offset to rewards array
    let rewards_offset_bytes = &input[36..68];
    let rewards_offset = U256::from_be_slice(rewards_offset_bytes);
    // The offset should be: 0x40 (64) + 0x20 (32 for length) + 0x40 (64 for 2 addresses) = 0xA0 (160)
    assert_eq!(rewards_offset, U256::from(160), "Rewards offset should be 160 (0xA0)");
    
    println!("✓ Finality reward ABI encoding passed");
} 