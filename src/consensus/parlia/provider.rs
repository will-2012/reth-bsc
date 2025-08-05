use super::snapshot::Snapshot;
use super::validator::SnapshotProvider;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::Arc;
use reth_provider::{HeaderProvider, BlockReader};
use crate::chainspec::BscChainSpec;
use crate::hardforks::BscHardforks;


/// Trait for creating snapshots on-demand when parent snapshots are missing
/// This will be removed in favor of integrating the logic into DbSnapshotProvider
pub trait OnDemandSnapshotCreator {
    /// Create a snapshot for the given block by working backwards to find an existing snapshot
    /// and then building forward
    fn create_snapshot_on_demand(&self, target_block_number: u64) -> Option<Snapshot>;
}

/// Very simple `SnapshotProvider` that keeps the most recent `max_entries` snapshots in memory.
/// Keys are the **block number** the snapshot is valid for (i.e. the last block of the snapshot’s
/// epoch). For historical sync this is sufficient – we can switch to an MDBX-backed provider later.
#[derive(Clone, Debug)]
pub struct InMemorySnapshotProvider {
    inner: Arc<RwLock<BTreeMap<u64, Snapshot>>>,
    max_entries: usize,
}

impl InMemorySnapshotProvider {
    /// Create a new provider keeping at most `max_entries` snapshots.
    pub fn new(max_entries: usize) -> Self {
        Self { inner: Arc::new(RwLock::new(BTreeMap::new())), max_entries }
    }
}

impl Default for InMemorySnapshotProvider {
    fn default() -> Self { Self::new(2048) }
}

impl SnapshotProvider for InMemorySnapshotProvider {
    fn snapshot(&self, block_number: u64) -> Option<Snapshot> {
        let guard = self.inner.read();
        tracing::info!("🔍 [BSC-PROVIDER] InMemorySnapshotProvider::snapshot called for block {}, cache size: {}", 
            block_number, guard.len());
        
        if guard.is_empty() {
            tracing::warn!("⚠️ [BSC-PROVIDER] InMemorySnapshotProvider cache is empty!");
        } else {
            let cache_keys: Vec<u64> = guard.keys().cloned().collect();
            tracing::info!("🔍 [BSC-PROVIDER] Cache keys: {:?}", cache_keys);
        }
        
        // Find the greatest key <= block_number.
        if let Some((found_block, snap)) = guard.range(..=block_number).next_back() {
            tracing::info!("✅ [BSC-PROVIDER] Found snapshot for block {} (requested {}): validators={}, epoch_num={}", 
                found_block, block_number, snap.validators.len(), snap.epoch_num);
            return Some(snap.clone());
        }
        
        tracing::warn!("⚠️ [BSC-PROVIDER] No snapshot found for block {}", block_number);
        None
    }

    fn insert(&self, snapshot: Snapshot) {
        let mut guard = self.inner.write();
        tracing::info!("📝 [BSC-PROVIDER] InMemorySnapshotProvider::insert called for block {}, cache size before: {}", 
            snapshot.block_number, guard.len());
        guard.insert(snapshot.block_number, snapshot.clone());
        tracing::info!("✅ [BSC-PROVIDER] Inserted snapshot for block {}: validators={}, epoch_num={}", 
            snapshot.block_number, snapshot.validators.len(), snapshot.epoch_num);
        
        // clamp size
        while guard.len() > self.max_entries {
            // remove the smallest key
            if let Some(first_key) = guard.keys().next().cloned() {
                tracing::debug!("🗑️ [BSC-PROVIDER] Removing old snapshot for block {} (cache full)", first_key);
                guard.remove(&first_key);
            }
        }
        tracing::debug!("🔍 [BSC-PROVIDER] Cache size after insert: {}", guard.len());
    }
}

impl SnapshotProvider for Arc<InMemorySnapshotProvider> {
    fn snapshot(&self, block_number: u64) -> Option<Snapshot> {
        (**self).snapshot(block_number)
    }

    fn insert(&self, snapshot: Snapshot) {
        (**self).insert(snapshot)
    }
}

// ---------------------------------------------------------------------------
// MDBX‐backed snapshot provider with LRU front‐cache
// ---------------------------------------------------------------------------

use reth_db::{Database, DatabaseError};
use reth_db::table::{Compress, Decompress};
use reth_db::models::ParliaSnapshotBlob;
use reth_db::transaction::{DbTx, DbTxMut};
use reth_db::cursor::DbCursorRO;
use schnellru::{ByLength, LruMap};

/// `DbSnapshotProvider` wraps an MDBX database; it keeps a small in-memory LRU to avoid hitting
/// storage for hot epochs. The DB layer persists snapshots as CBOR blobs via the `ParliaSnapshots`
/// table that is already defined in `db.rs`.
/// 
/// Enhanced to include backward walking logic like reth-bsc-trail and bsc-erigon.
#[derive(Debug)]
pub struct DbSnapshotProvider<DB: Database> {
    db: DB,
    /// Front cache keyed by *block number*.
    cache: RwLock<LruMap<u64, Snapshot, ByLength>>,
}

/// Enhanced version with backward walking capability
#[derive(Debug)]
pub struct EnhancedDbSnapshotProvider<DB: Database, Provider> {
    base: DbSnapshotProvider<DB>,
    /// Header provider for backward walking
    header_provider: Arc<Provider>,
    /// Chain spec for genesis snapshot creation
    chain_spec: Arc<BscChainSpec>,
}

impl<DB: Database> DbSnapshotProvider<DB> {
    pub fn new(db: DB, capacity: usize) -> Self {
        Self { 
            db, 
            cache: RwLock::new(LruMap::new(ByLength::new(capacity as u32))),
        }
    }
}

impl<DB: Database, Provider> EnhancedDbSnapshotProvider<DB, Provider> 
where
    Provider: HeaderProvider<Header = alloy_consensus::Header> + BlockReader + Send + Sync + 'static,
{
    pub fn new(
        db: DB, 
        capacity: usize, 
        header_provider: Arc<Provider>,
        chain_spec: Arc<BscChainSpec>,
    ) -> Self {
        Self { 
            base: DbSnapshotProvider::new(db, capacity),
            header_provider,
            chain_spec,
        }
    }
}

impl<DB: Database + Clone> Clone for DbSnapshotProvider<DB> {
    fn clone(&self) -> Self {
        // Create a new instance with the same database but a fresh cache
        Self::new(self.db.clone(), 2048)
    }
}

impl<DB: Database + Clone, Provider: Clone> Clone for EnhancedDbSnapshotProvider<DB, Provider> {
    fn clone(&self) -> Self {
        Self {
            base: self.base.clone(),
            header_provider: self.header_provider.clone(),
            chain_spec: self.chain_spec.clone(),
        }
    }
}

impl<DB: Database> DbSnapshotProvider<DB> {
    fn load_from_db(&self, block_number: u64) -> Option<Snapshot> {
        let tx = self.db.tx().ok()?;
        let mut cursor = tx
            .cursor_read::<crate::consensus::parlia::db::ParliaSnapshots>()
            .ok()?;
        let mut iter = cursor.walk_range(..=block_number).ok()?;
        let mut last: Option<Snapshot> = None;
        while let Some(Ok((_, raw_blob))) = iter.next() {
            let raw = &raw_blob.0;
            if let Ok(decoded) = Snapshot::decompress(raw) {
                last = Some(decoded);
            }
        }
        last
    }

    fn persist_to_db(&self, snap: &Snapshot) -> Result<(), DatabaseError> {
        let tx = self.db.tx_mut()?;
        tx.put::<crate::consensus::parlia::db::ParliaSnapshots>(snap.block_number, ParliaSnapshotBlob(snap.clone().compress()))?;
        tx.commit()?;
        Ok(())
    }
}

impl<DB: Database + 'static> SnapshotProvider for DbSnapshotProvider<DB> {
    fn snapshot(&self, block_number: u64) -> Option<Snapshot> {
        // fast path: cache
        {
            let mut guard = self.cache.write();
            if let Some(snap) = guard.get(&block_number) {
                return Some(snap.clone());
            }
        }

        // slow path: DB scan
        let snap = self.load_from_db(block_number)?;
        self.cache.write().insert(block_number, snap.clone());
        Some(snap)
    }

    fn insert(&self, snapshot: Snapshot) {
        // update cache
        self.cache.write().insert(snapshot.block_number, snapshot.clone());
        // Persist only at checkpoint boundaries to reduce I/O.
        if snapshot.block_number % crate::consensus::parlia::snapshot::CHECKPOINT_INTERVAL == 0 {
            // fire-and-forget DB write; errors are logged but not fatal
            let _ = self.persist_to_db(&snapshot);
        }
    }
}

// Simplified version based on reth-bsc-trail's approach - much faster and simpler
impl<DB: Database + 'static, Provider> SnapshotProvider for EnhancedDbSnapshotProvider<DB, Provider> 
where
    Provider: HeaderProvider<Header = alloy_consensus::Header> + BlockReader + Send + Sync + 'static,
{
    fn snapshot(&self, block_number: u64) -> Option<Snapshot> {
        // Early return for cached snapshots to avoid expensive computation
        {
            let mut cache_guard = self.base.cache.write();
            if let Some(cached_snap) = cache_guard.get(&block_number) {
                tracing::trace!("✅ [BSC] Cache hit for snapshot block {}", block_number);
                return Some(cached_snap.clone());
            }
        }

        // simple backward walking + proper epoch updates
        let mut current_block = block_number;
        let mut headers_to_apply = Vec::new();

        // 1. Backward walking loop 
        let base_snapshot = loop {
            // Check cache first (need write lock for LRU get operation)
            {
                let mut cache_guard = self.base.cache.write();
                if let Some(snap) = cache_guard.get(&current_block) {
                    break snap.clone();
                }
            }

            // Check database at checkpoint intervals (every 1024 blocks)
            if current_block % crate::consensus::parlia::snapshot::CHECKPOINT_INTERVAL == 0 {
                if let Some(snap) = self.base.load_from_db(current_block) {
                    self.base.cache.write().insert(current_block, snap.clone());
                    break snap;
                }
            }

            // Genesis handling - create genesis snapshot 
            if current_block == 0 {
                tracing::debug!("🚀 [BSC] Creating genesis snapshot for backward walking");
                if let Ok(genesis_snap) = crate::consensus::parlia::ParliaConsensus::<BscChainSpec, DbSnapshotProvider<DB>>::create_genesis_snapshot(
                    self.chain_spec.clone(),
                    crate::consensus::parlia::EPOCH
                ) {
                    self.base.cache.write().insert(0, genesis_snap.clone());
                    break genesis_snap;
                } else {
                    tracing::error!("❌ [BSC] Failed to create genesis snapshot");
                    return None;
                }
            }

                            // Collect header for forward application - fail if not available 
                if let Ok(Some(header)) = self.header_provider.header_by_number(current_block) {
                    headers_to_apply.push(header);
                    current_block = current_block.saturating_sub(1);
                } else {
                    // Header not available - this is common during Bodies stage validation
                    // where headers might not be available in dependency order.
                    // Fail gracefully to defer validation to Execution stage.
                    if block_number % 100000 == 0 { // only log every 100k blocks to reduce spam
                        tracing::debug!("🔄 [BSC] Header {} not available for snapshot creation (block {}), deferring to execution stage", current_block, block_number);
                    }
                    return None;
                }
        };

        // 2. Apply headers forward with epoch updates 
        headers_to_apply.reverse();
        let mut working_snapshot = base_snapshot;

        for header in headers_to_apply {
            // Check for epoch boundary 
            let (new_validators, vote_addrs, turn_length) = if header.number > 0 &&
                header.number % working_snapshot.epoch_num == 0 // This is the epoch boundary check
            {
                // Parse validator set from epoch header 
                super::validator::parse_epoch_update(&header, 
                    self.chain_spec.is_luban_active_at_block(header.number),
                    self.chain_spec.is_bohr_active_at_timestamp(header.timestamp)
                )
            } else {
                (Vec::new(), None, None)
            };

            // Apply header to snapshot (now determines hardfork activation internally)
            working_snapshot = match working_snapshot.apply(
                header.beneficiary,
                &header,
                new_validators,
                vote_addrs,
                None, // TODO: Parse attestation from header like reth-bsc-trail for vote tracking
                turn_length,
                &*self.chain_spec,
            ) {
                Some(snap) => snap,
                None => {
                    if header.number % 100000 == 0 { // only log every 100k blocks to reduce spam
                        tracing::debug!("🔄 [BSC] Failed to apply header {} to snapshot during Bodies stage", header.number);
                    }
                    return None;
                }
            };

            // Cache intermediate snapshots (like reth-bsc-trail)
            self.base.cache.write().insert(working_snapshot.block_number, working_snapshot.clone());

            // Persist checkpoint snapshots to database (like reth-bsc-trail)
            if working_snapshot.block_number % crate::consensus::parlia::snapshot::CHECKPOINT_INTERVAL == 0 {
                tracing::info!("📦 [BSC] Persisting checkpoint snapshot for block {}", working_snapshot.block_number);
                self.base.insert(working_snapshot.clone());
            }
        }

        tracing::debug!("✅ [BSC] Created snapshot for block {} via reth-bsc-trail-style backward walking", block_number);
        Some(working_snapshot)
    }

    fn insert(&self, snapshot: Snapshot) {
        self.base.insert(snapshot);
    }
}
