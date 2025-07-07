//! Parlia snapshot database table definitions.
//!
//! Stored value is the CBOR‐compressed `Snapshot` blob returned by
//! `Compress` implementation.

use crate::consensus::parlia::snapshot::Snapshot;
use reth_db::table::{Compress, Decompress, Encode, Decode, Table};
use reth_db::DatabaseError;

/// Table: epoch boundary block number (u64) -> compressed snapshot bytes.
#[derive(Debug)]
pub struct ParliaSnapshots;

impl Table for ParliaSnapshots {
    const NAME: &'static str = "ParliaSnapshots";
    const DUPSORT: bool = false;
    type Key = u64;
    /// Raw compressed bytes produced by `Snapshot::compress()`.
    type Value = Snapshot;
}

// Implement Encode / Decode via the `Compress` + `Decompress` impls on Snapshot.
impl Encode for Snapshot {
    type Encoded = Vec<u8>;
    fn encode(self) -> Self::Encoded { Compress::compress(self) }
}

impl Decode for Snapshot {
    fn decode(value: &[u8]) -> Result<Self, DatabaseError> { Decompress::decompress(value) }
} 