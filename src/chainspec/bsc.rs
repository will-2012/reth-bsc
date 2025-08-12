//! Chain specification for BSC, credits to: <https://github.com/bnb-chain/reth/blob/main/crates/bsc/chainspec/src/bsc.rs>
use crate::hardforks::bsc::BscHardfork;
use alloy_primitives::{BlockHash, U256};
use reth_chainspec::{
    make_genesis_header, BaseFeeParams, BaseFeeParamsKind, Chain, ChainSpec, Head, NamedChain,
};
use alloy_eips::{eip7840::BlobParams, eip7892::BlobScheduleBlobParams};
use alloy_eips::eip4844::BLOB_TX_MIN_BLOB_GASPRICE;
use reth_primitives::SealedHeader;
use std::str::FromStr;

pub fn bsc_mainnet() -> ChainSpec {
    let genesis = serde_json::from_str(include_str!("genesis.json"))
        .expect("Can't deserialize BSC Mainnet genesis json");
    let hardforks = BscHardfork::bsc_mainnet();
    ChainSpec {
        chain: Chain::from_named(NamedChain::BinanceSmartChain),
        genesis: serde_json::from_str(include_str!("genesis.json"))
            .expect("Can't deserialize BSC Mainnet genesis json"),
        paris_block_and_final_difficulty: Some((0, U256::from(0))),
        hardforks: BscHardfork::bsc_mainnet(),
        deposit_contract: None,
        base_fee_params: BaseFeeParamsKind::Constant(BaseFeeParams::new(1, 1)),
        blob_params: BlobScheduleBlobParams {
            cancun: BlobParams {
                target_blob_count: 3,
                max_blob_count: 6,
                update_fraction: 3_338_477,
                min_blob_fee: BLOB_TX_MIN_BLOB_GASPRICE,
                max_blobs_per_tx: 6,
            },
            prague: BlobParams {
                target_blob_count: 3, // BSC keeps same values in Prague
                max_blob_count: 6,
                update_fraction: 3_338_477,
                min_blob_fee: BLOB_TX_MIN_BLOB_GASPRICE,
                max_blobs_per_tx: 6,
            },
            ..Default::default()
        },
        prune_delete_limit: 3500,
        genesis_header: SealedHeader::new(
            make_genesis_header(&genesis, &hardforks),
            BlockHash::from_str(
                "0x0d21840abff46b96c84b2ac9e10e4f5cdaeb5693cb665db62a2f3b02d2d57b5b",
            )
            .unwrap(),
        ),
        ..Default::default()
    }
}

pub fn head() -> Head {
    Head { number: 40_000_000, timestamp: 1751250600, ..Default::default() }
}

pub fn current_head() -> Head {
    // ACTUAL BSC mainnet state as of July 19, 2025
    // Block: 54,522,626, Timestamp: 1752889876 (2025-07-19 01:51:16 UTC)
    Head { number: 54_522_626, timestamp: 1752889876, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use crate::chainspec::bsc::{bsc_mainnet, head, current_head};
    use alloy_primitives::hex;
    use reth_chainspec::{ForkHash, ForkId};

    #[test]
    fn can_create_forkid() {
        let b = hex::decode("098d24ac").unwrap();
        let expected = [b[0], b[1], b[2], b[3]];
        let expected_f_id = ForkId { hash: ForkHash(expected), next: 0 };

        let fork_id = bsc_mainnet().fork_id(&head());
        assert_eq!(fork_id, expected_f_id);
    }

    #[test]
    fn current_mainnet_forkid() {
        let fork_id = bsc_mainnet().fork_id(&current_head());
        println!("Current BSC mainnet fork ID: {:?}", fork_id);
        
        // Convert to hex for easier comparison
        let hash_bytes = fork_id.hash.0;
        let hash_hex = hex::encode(hash_bytes);
        println!("Current fork ID as hex: {}", hash_hex);
    }
}
