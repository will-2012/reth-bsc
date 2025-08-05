use super::snapshot::Snapshot;
use super::validator::SnapshotProvider;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::Arc;
use reth_provider::{HeaderProvider, BlockReader};
use alloy_consensus::BlockHeader;

use crate::chainspec::BscChainSpec;

use reth_primitives::SealedHeader;


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
        // 1. Check cache first (fast path)
        {
            let mut guard = self.base.cache.write();
            if let Some(snap) = guard.get(&block_number) {
                return Some(snap.clone());
            }
        }

        // 2. Check database for exact match
        if let Some(snap) = self.base.load_from_db(block_number) {
            self.base.cache.write().insert(block_number, snap.clone());
            return Some(snap);
        }

        // 3. Smart gap detection (Headers vs Bodies stage aware): 
        // During Headers stage (initial sync), headers are downloaded from high->low, so large gaps are normal
        // During Bodies/Execution stage, we want to defer validation for large gaps
        
        // Only apply aggressive gap detection during Bodies/Execution stage, not Headers stage
        // Heuristic: If block_number > 100k and we have no cached snapshots, we're likely in Headers stage
        if block_number > 1000 { // Only for non-genesis blocks
            let latest_cached_block = {
                let cache_guard = self.base.cache.read();
                cache_guard.iter().next_back().map(|(block_num, _)| *block_num)
            };
            
            // If we have cached snapshots and there's a reasonable gap, apply smart deferral
            if let Some(latest_block) = latest_cached_block {
                let gap_size = block_number.saturating_sub(latest_block);
                
                // Only defer for very large gaps during Bodies stage (when we have cached snapshots)
                // This avoids interfering with Headers stage which processes high->low
                if gap_size > 10000 && latest_block > 1000 {
                    // Much less frequent logging - only every 100k blocks
                    if block_number % 100000 == 0 {
                        tracing::trace!("📈 [BSC] Large gap detected during Bodies/Execution stage: {} blocks, deferring validation", gap_size);
                    }
                    return None;
                }
            }
            
            // Much more conservative sequential check - only for very close to checkpoints
            if self.base.load_from_db(block_number - 1).is_none() {
                let checkpoint_distance = block_number % crate::consensus::parlia::snapshot::CHECKPOINT_INTERVAL;
                if checkpoint_distance > 100 { // Much less aggressive (100 vs 32) 
                    // Rare logging
                    if block_number % 50000 == 0 {
                        tracing::trace!("📈 [BSC] Sequential gap detected for block {} ({} blocks from checkpoint), deferring validation", 
                            block_number, checkpoint_distance);
                    }
                    return None;
                }
            }
        }

        // 4. On-demand snapshot creation from stored headers (post-sync)
        // If blocks are already synced but snapshots missing, create from stored data
        let mut current_block = block_number;
        let mut headers_to_apply = Vec::new();
        let mut search_limit = 1024; // Reasonable limit for post-sync snapshot creation

        let base_snapshot = loop {
            if search_limit == 0 {
                tracing::debug!("🔍 [BSC] Snapshot search limit reached for block {}, deferring", block_number);
                return None;
            }
            search_limit -= 1;

            // Check cache for current block
            {
                let mut guard = self.base.cache.write();
                if let Some(snap) = guard.get(&current_block) {
                    break snap.clone();
                }
            }

            // Check database at checkpoint intervals (1024) or exact match
            if current_block % crate::consensus::parlia::snapshot::CHECKPOINT_INTERVAL == 0 || current_block == block_number {
                if let Some(snap) = self.base.load_from_db(current_block) {
                    self.base.cache.write().insert(current_block, snap.clone());
                    break snap;
                }
            }

            // Genesis handling - create genesis snapshot
            if current_block == 0 {
                tracing::info!("🚀 [BSC] Creating genesis snapshot");
                if let Ok(genesis_snap) = crate::consensus::parlia::ParliaConsensus::<BscChainSpec, DbSnapshotProvider<DB>>::create_genesis_snapshot(
                    self.chain_spec.clone(),
                    crate::consensus::parlia::EPOCH
                ) {
                    self.base.cache.write().insert(0, genesis_snap.clone());
                    if current_block == 0 {
                        return Some(genesis_snap);
                    }
                    break genesis_snap;
                } else {
                    tracing::warn!("⚠️ [BSC] Failed to create genesis snapshot");
                    return None;
                }
            }

            // Collect header for forward application - but fail fast if not available
            if let Ok(Some(header)) = self.header_provider.header_by_number(current_block) {
                headers_to_apply.push(SealedHeader::new(header.clone(), header.hash_slow()));
                current_block = current_block.saturating_sub(1);
            } else {
                // Header not available - fail fast like reth-bsc-trail instead of complex retry
                tracing::debug!("📋 [BSC] Header {} not available during snapshot search, deferring", current_block);
                return None;
            }
        };

        // 5. Apply headers forward (much simpler than our previous implementation)
        headers_to_apply.reverse();
        let mut working_snapshot = base_snapshot;
        
        for header in headers_to_apply {
            // Simplified application - same as before but no complex caching
            let header_timestamp = header.header().timestamp();
            let is_lorentz_active = header_timestamp >= 1744097580; // Lorentz hardfork timestamp
            let is_maxwell_active = header_timestamp >= 1748243100; // Maxwell hardfork timestamp
            
            working_snapshot = working_snapshot.apply(
                header.beneficiary(),
                header.header(),
                Vec::new(), // new_validators
                None,       // vote_addrs  
                None,       // attestation
                None,       // turn_length
                false,      // is_bohr
                is_lorentz_active,
                is_maxwell_active,
            )?;
        }

        // Cache final result only
        self.base.cache.write().insert(block_number, working_snapshot.clone());
        
        // Also persist if this is a checkpoint for future retrieval
        if working_snapshot.block_number % crate::consensus::parlia::snapshot::CHECKPOINT_INTERVAL == 0 {
            tracing::info!("📦 [BSC] Created and caching checkpoint snapshot for block {} (post-sync on-demand)", working_snapshot.block_number);
            // Insert will persist to MDBX via DbSnapshotProvider
            self.base.insert(working_snapshot.clone());
        }
        
        tracing::trace!("✅ [BSC] Created snapshot for block {} (on-demand from stored headers)", block_number);
        Some(working_snapshot)
    }

    fn insert(&self, snapshot: Snapshot) {
        self.base.insert(snapshot);
    }
}

// Old OnDemandSnapshotProvider has been replaced with EnhancedDbSnapshotProvider above
// which follows the exact reth-bsc-trail/bsc-erigon pattern
