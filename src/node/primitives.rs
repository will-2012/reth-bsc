#![allow(clippy::owned_cow)]
use alloy_consensus::{BlobTransactionSidecar, Header};
use alloy_primitives::B256;
use alloy_rlp::{RlpDecodable, RlpEncodable};
use reth_ethereum_primitives::Receipt;
use reth_primitives::{Block, BlockBody, NodePrimitives, TransactionSigned};
use serde::{Deserialize, Serialize};

/// Primitive types for BSC.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct BscPrimitives;

impl NodePrimitives for BscPrimitives {
    type Block = Block;  // Use standard reth Block type like zoro_reth
    type BlockHeader = Header;
    type BlockBody = BlockBody;  // Use standard BlockBody type like zoro_reth
    type SignedTx = TransactionSigned;
    type Receipt = Receipt;
}

/// BSC representation of a EIP-4844 sidecar.
/// This matches zoro_reth's BlobSidecar structure for BSC-specific blob data.
#[derive(Debug, Clone, PartialEq, Eq, RlpEncodable, RlpDecodable, Serialize, Deserialize)]
pub struct BscBlobTransactionSidecar {
    pub inner: BlobTransactionSidecar,
    pub block_number: u64,
    pub block_hash: B256,
    pub tx_index: u64,
    pub tx_hash: B256,
}
