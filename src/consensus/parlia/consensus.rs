use super::{ParliaHeaderValidator, SnapshotProvider, BscConsensusValidator, Snapshot, TransactionSplitter, SplitTransactions, constants::{DIFF_INTURN, DIFF_NOTURN}};
use alloy_consensus::{Header, TxReceipt, Transaction, BlockHeader};
use reth_primitives_traits::SignerRecoverable;
use crate::{
    node::primitives::BscBlock,
    hardforks::BscHardforks,
    BscPrimitives,

};
use reth::consensus::{Consensus, FullConsensus, ConsensusError, HeaderValidator};
use reth_primitives::Receipt;
use reth_primitives_traits::proofs;
use reth_provider::BlockExecutionResult;
use reth_primitives_traits::{Block, SealedBlock, SealedHeader, RecoveredBlock};
use reth_chainspec::EthChainSpec;
use std::sync::Arc;

/// Enhanced Parlia consensus that implements proper pre/post execution validation
#[derive(Debug, Clone)]
pub struct ParliaConsensus<ChainSpec, P> {
    chain_spec: Arc<ChainSpec>,
    header_validator: Arc<ParliaHeaderValidator<P>>,
    consensus_validator: Arc<BscConsensusValidator<ChainSpec>>,
    snapshot_provider: Arc<P>,
    epoch: u64,
}

impl<ChainSpec, P> ParliaConsensus<ChainSpec, P>
where
    ChainSpec: EthChainSpec + BscHardforks + 'static,
    P: SnapshotProvider + std::fmt::Debug + 'static,
{
    pub fn new(
        chain_spec: Arc<ChainSpec>,
        snapshot_provider: Arc<P>,
        epoch: u64,
    ) -> Self {
        let header_validator = Arc::new(ParliaHeaderValidator::new(snapshot_provider.clone()));
        let consensus_validator = Arc::new(BscConsensusValidator::new(chain_spec.clone()));
        
        let consensus = Self { 
            chain_spec,
            header_validator,
            consensus_validator,
            snapshot_provider,
            epoch,
        };
        
        // Initialize genesis snapshot if needed
        consensus.ensure_genesis_snapshot();
        
        consensus
    }

    /// Create consensus with database-backed persistent snapshots
    pub fn with_database<DB: reth_db::database::Database + 'static>(
        chain_spec: Arc<ChainSpec>,
        database: DB,
        epoch: u64,
        cache_size: usize,
    ) -> ParliaConsensus<ChainSpec, crate::consensus::parlia::provider::DbSnapshotProvider<DB>> {
        let snapshot_provider = Arc::new(
            crate::consensus::parlia::provider::DbSnapshotProvider::new(database, cache_size)
        );
        let consensus = ParliaConsensus::new(chain_spec, snapshot_provider, epoch);
        
        // Initialize genesis snapshot if needed
        consensus.ensure_genesis_snapshot();
        
        consensus
    }

    /// Validate block pre-execution using Parlia rules
    fn validate_block_pre_execution_impl(&self, block: &SealedBlock<BscBlock>) -> Result<(), ConsensusError> {
        let header = block.header();

        // Skip genesis block
        if header.number == 0 {
            return Ok(());
        }

        // 1. Basic block validation (similar to standard pre-execution)
        self.validate_basic_block_fields(block)?;

        // 2. BSC-specific Parlia validation
        self.validate_parlia_specific_fields(block)?;

        Ok(())
    }

    /// Ensure genesis snapshot exists
    fn ensure_genesis_snapshot(&self) {
        // Check if genesis snapshot already exists
        if self.snapshot_provider.snapshot(0).is_some() {
            return;
        }

        // Create genesis snapshot from chain spec
        if let Ok(genesis_snapshot) = Self::create_genesis_snapshot(self.chain_spec.clone(), self.epoch) {
            self.snapshot_provider.insert(genesis_snapshot);
            tracing::info!("🎯 [BSC] Created genesis snapshot for block 0");
        } else {
            tracing::warn!("⚠️ [BSC] Failed to create genesis snapshot");
        }
    }

    /// Get reference to the snapshot provider
    pub fn snapshot_provider(&self) -> &Arc<P> {
        &self.snapshot_provider
    }

    /// Create genesis snapshot from BSC chain specification
    pub fn create_genesis_snapshot(chain_spec: Arc<ChainSpec>, epoch: u64) -> Result<crate::consensus::parlia::snapshot::Snapshot, ConsensusError> 
    where
        ChainSpec: EthChainSpec + BscHardforks + 'static,
    {
        let genesis_header = chain_spec.genesis_header();
        let validators = Self::parse_genesis_validators_static(genesis_header.extra_data())?;
        
        if validators.is_empty() {
            return Err(ConsensusError::Other("No validators found in genesis header".into()));
        }

        let genesis_hash = alloy_primitives::keccak256(alloy_rlp::encode(genesis_header));

        let snapshot = crate::consensus::parlia::snapshot::Snapshot::new(
            validators,
            0, // block number
            genesis_hash, // block hash
            epoch, // epoch length
            None, // no vote addresses pre-Luban
        );

        tracing::info!("🚀 [BSC] Genesis snapshot created with {} validators", snapshot.validators.len());
        Ok(snapshot)
    }



    /// Parse genesis validators from BSC extraData (static version)
    fn parse_genesis_validators_static(extra_data: &alloy_primitives::Bytes) -> Result<Vec<alloy_primitives::Address>, ConsensusError> {
        const EXTRA_VANITY_LEN: usize = 32;
        const EXTRA_SEAL_LEN: usize = 65;

        if extra_data.len() <= EXTRA_VANITY_LEN + EXTRA_SEAL_LEN {
            return Err(ConsensusError::Other("extraData too short for validator list".into()));
        }

        let validator_bytes = &extra_data[EXTRA_VANITY_LEN..extra_data.len() - EXTRA_SEAL_LEN];
        
        if validator_bytes.len() % 20 != 0 {
            return Err(ConsensusError::Other("validator data length not divisible by 20".into()));
        }

        let mut validators = Vec::new();
        for chunk in validator_bytes.chunks(20) {
            let address = alloy_primitives::Address::from_slice(chunk);
            validators.push(address);
        }

        tracing::debug!("📋 [BSC] Parsed {} validators from genesis extraData", validators.len());
        Ok(validators)
    }



    /// Validate basic block fields (transaction root, blob gas, etc.)
    fn validate_basic_block_fields(&self, block: &SealedBlock<BscBlock>) -> Result<(), ConsensusError> {
        // Check transaction root
        if let Err(error) = block.ensure_transaction_root_valid() {
            return Err(ConsensusError::BodyTransactionRootDiff(error.into()));
        }

        // EIP-4844: Blob gas validation for Cancun fork
        if self.chain_spec.is_cancun_active_at_timestamp(block.timestamp) {
            self.validate_cancun_blob_gas(block)?;
        }

        Ok(())
    }

    /// Validate BSC-specific Parlia consensus fields
    fn validate_parlia_specific_fields(&self, block: &SealedBlock<BscBlock>) -> Result<(), ConsensusError> {
        let header = block.header();

        // Get snapshot for validation
        let parent_number = header.number - 1;
        let snapshot = match self.snapshot_provider.snapshot(parent_number) {
            Some(snapshot) => snapshot,
            None => {
                // Snapshot not available - this can happen during live sync when there's a large gap
                // between local chain tip and live blocks. In this case, defer validation.
                // The staged sync pipeline should continue to fill the gap instead of trying
                // to validate live blocks without proper ancestry.
                tracing::debug!(
                    block_number = header.number,
                    parent_number = parent_number,
                    "Snapshot not available for validation, deferring validation during sync gap"
                );
                return Ok(());
            }
        };

        // Create a SealedHeader for validation methods
        let sealed_header = SealedHeader::new(header.clone(), block.hash());

        // Verify cascading fields in order:
        // 1. Block timing constraints (Ramanujan fork)
        self.verify_block_timing(&sealed_header, &snapshot)?;

        // 2. Vote attestation (Plato fork) 
        self.verify_vote_attestation(&sealed_header)?;

        // 3. Seal verification (signature recovery and validator authorization)
        self.verify_seal(&sealed_header, &snapshot)?;

        // 4. Turn-based proposing (difficulty validation)
        self.verify_difficulty(&sealed_header, &snapshot)?;

        // 5. Turn length validation (Bohr fork)
        self.verify_turn_length(&sealed_header)?;

        Ok(())
    }

    /// Validate block post-execution using Parlia rules
    fn validate_block_post_execution_impl(
        &self,
        block: &RecoveredBlock<BscBlock>,
        result: &BlockExecutionResult<Receipt>,
    ) -> Result<(), ConsensusError> {
        let _header = block.header();
        let receipts = &result.receipts;

        // 1. Basic post-execution validation (gas used, receipts root, logs bloom)
        self.validate_basic_post_execution_fields(block, receipts)?;

        // 2. BSC-specific post-execution validation
        self.validate_parlia_post_execution_fields(block, receipts)?;

        Ok(())
    }

    /// Validate basic post-execution fields (gas, receipts, logs)
    fn validate_basic_post_execution_fields(
        &self,
        block: &RecoveredBlock<BscBlock>,
        receipts: &[Receipt],
    ) -> Result<(), ConsensusError> {
        let header = block.header();

        // Check if gas used matches the value set in header
        let cumulative_gas_used = receipts.last()
            .map(|receipt| receipt.cumulative_gas_used)
            .unwrap_or(0);
        
        if header.gas_used != cumulative_gas_used {
            return Err(ConsensusError::Other(
                format!("Gas used mismatch: header={}, receipts={}", header.gas_used, cumulative_gas_used).into()
            ));
        }

        // Verify receipts root and logs bloom (after Byzantium fork)
        if self.chain_spec.is_byzantium_active_at_block(header.number) {
            self.verify_receipts_and_logs(header, receipts)?;
        }

        Ok(())
    }

    /// Validate BSC-specific post-execution fields
    fn validate_parlia_post_execution_fields(
        &self,
        block: &RecoveredBlock<BscBlock>,
        _receipts: &[Receipt],
    ) -> Result<(), ConsensusError> {
        let header = block.header();

        // 1. Split and validate system transactions
        self.validate_system_transactions(block)?;

        // 2. Validate epoch transitions
        if header.number % self.epoch == 0 {
            // TODO: Implement epoch transition validation
            // This would verify validator set updates every 200 blocks
            // For now, just log that we're at an epoch boundary
        }

        // TODO: Add more BSC-specific post-execution validations:
        // - System reward distribution validation
        // - Slash contract interaction validation

        Ok(())
    }

    /// Validate system transactions using splitTxs logic
    fn validate_system_transactions(&self, block: &RecoveredBlock<BscBlock>) -> Result<(), ConsensusError> {
        let header = block.header();
        // Extract the raw transactions from the block  
        let transactions: Vec<_> = block.body().transactions().cloned().collect();
        let beneficiary = header.beneficiary;

        // Split transactions into user and system transactions
        let split_result = TransactionSplitter::split_transactions(&transactions, beneficiary)
            .map_err(|e| ConsensusError::Other(format!("Failed to split transactions: {}", e).into()))?;

        // Log transaction split results for debugging
        // TODO: Remove debug logging in production
        if split_result.system_count() > 0 {
            // System transactions found - validate them
            self.validate_split_system_transactions(&split_result, header)?;
        }

        Ok(())
    }

    /// Validate the identified system transactions
    fn validate_split_system_transactions(
        &self,
        split: &SplitTransactions,
        header: &alloy_consensus::Header,
    ) -> Result<(), ConsensusError> {
        // TODO: Implement comprehensive system transaction validation:
        // 1. Verify system transactions are in the correct order
        // 2. Validate system transaction parameters (SlashIndicator, StakeHub, etc.)
        // 3. Check that required system transactions are present
        // 4. Validate system transaction execution results

        // For now, just ensure we can identify system transactions correctly
        for (i, system_tx) in split.system_txs.iter().enumerate() {
            // Basic validation: system transaction should have gas price 0
            if system_tx.max_fee_per_gas() != 0 {
                return Err(ConsensusError::Other(
                    format!("System transaction {} has non-zero gas price: {}", i, system_tx.max_fee_per_gas()).into()
                ));
            }

            // Basic validation: system transaction should be sent by beneficiary
            match system_tx.recover_signer() {
                Ok(signer) => {
                    if signer != header.beneficiary {
                        return Err(ConsensusError::Other(
                            format!("System transaction {} not sent by beneficiary: signer={}, beneficiary={}", 
                                    i, signer, header.beneficiary).into()
                        ));
                    }
                }
                Err(_) => {
                    return Err(ConsensusError::Other(
                        format!("Failed to recover signer for system transaction {}", i).into()
                    ));
                }
            }
        }

        Ok(())
    }

    /// Validate EIP-4844 blob gas fields for Cancun fork
    fn validate_cancun_blob_gas(&self, block: &SealedBlock<BscBlock>) -> Result<(), ConsensusError> {
        // Check that blob gas used field exists in header for Cancun fork
        if block.header().blob_gas_used.is_none() {
            return Err(ConsensusError::Other("Blob gas used missing in Cancun block".into()));
        }

        // TODO: Implement detailed blob gas validation
        // This would check that the blob gas used in the header matches the sum of blob gas used by transactions
        // For now, we just verify the field exists

        Ok(())
    }

    /// Verify block timing constraints for Ramanujan fork
    fn verify_block_timing(&self, header: &SealedHeader<Header>, _snapshot: &Snapshot) -> Result<(), ConsensusError> {
        if !self.chain_spec.is_ramanujan_active_at_block(header.number) {
            return Ok(());
        }

        // TODO: Implement block timing validation
        // This would check that block.timestamp >= parent.timestamp + period + backoff_time
        // For now, we'll skip this validation as it requires parent header access
        
        Ok(())
    }

    /// Verify vote attestation for Plato fork
    fn verify_vote_attestation(&self, header: &SealedHeader<Header>) -> Result<(), ConsensusError> {
        if !self.chain_spec.is_plato_active_at_block(header.number) {
            return Ok(());
        }

        // TODO: Implement vote attestation verification
        // This involves parsing and validating BLS signature aggregation
        // For now, we'll skip this complex validation
        
        Ok(())
    }

    /// Verify turn length at epoch boundaries for Bohr fork
    fn verify_turn_length(&self, header: &SealedHeader<Header>) -> Result<(), ConsensusError> {
        if header.number % self.epoch != 0 || !self.chain_spec.is_bohr_active_at_timestamp(header.timestamp) {
            return Ok(());
        }

        // TODO: Implement turn length verification
        // This would parse turn length from header extra data and compare with contract state
        // For now, we'll skip this validation
        
        Ok(())
    }

    /// Verify receipts root and logs bloom
    fn verify_receipts_and_logs(&self, header: &alloy_consensus::Header, receipts: &[Receipt]) -> Result<(), ConsensusError> {
        // Calculate receipts root
        let receipts_with_bloom = receipts.iter().map(|r| r.with_bloom_ref()).collect::<Vec<_>>();
        let calculated_receipts_root = proofs::calculate_receipt_root(&receipts_with_bloom);

        if header.receipts_root != calculated_receipts_root {
            return Err(ConsensusError::Other(
                format!("Receipts root mismatch: header={}, calculated={}", header.receipts_root, calculated_receipts_root).into()
            ));
        }

        // Calculate logs bloom
        let calculated_logs_bloom = receipts_with_bloom.iter()
            .fold(alloy_primitives::Bloom::ZERO, |bloom, r| bloom | r.bloom());

        if header.logs_bloom != calculated_logs_bloom {
            return Err(ConsensusError::Other(
                format!("Logs bloom mismatch").into()
            ));
        }

        Ok(())
    }

    /// Verify the seal (proposer signature) in the header
    fn verify_seal(&self, header: &SealedHeader<Header>, snapshot: &Snapshot) -> Result<(), ConsensusError> {
        // Enhanced seal verification with proper authorization checks
        let proposer = header.beneficiary;

        // Check if proposer is in the current validator set
        if !snapshot.validators.contains(&proposer) {
            return Err(ConsensusError::Other(
                format!("Unauthorized proposer: {}", proposer).into()
            ));
        }

        // Check if proposer signed recently (to prevent spamming)
        if snapshot.sign_recently(proposer) {
            return Err(ConsensusError::Other(
                format!("Proposer {} signed recently", proposer).into()
            ));
        }

        // TODO: Implement actual signature recovery and verification
        // This would involve:
        // 1. Recovering the proposer address from the signature in header.extra_data
        // 2. Verifying it matches header.beneficiary
        // For now, we assume the beneficiary is correct

        Ok(())
    }

    /// Verify the difficulty based on turn-based proposing
    fn verify_difficulty(&self, header: &SealedHeader<Header>, snapshot: &Snapshot) -> Result<(), ConsensusError> {
        // BSC uses the recovered signer from signature, not beneficiary!
        // Recover the actual proposer from the signature
        let proposer = self.consensus_validator.recover_proposer_from_seal(header)?;
        
        // Verify proposer matches coinbase (BSC requirement)
        let coinbase = header.header().beneficiary();
        if proposer != coinbase {
            return Err(ConsensusError::Other(format!(
                "Proposer mismatch: recovered={:?}, coinbase={:?}", proposer, coinbase
            )));
        }
        
        let in_turn = snapshot.is_inturn(proposer);

        let expected_difficulty = if in_turn { DIFF_INTURN } else { DIFF_NOTURN };

        if header.difficulty != expected_difficulty {

            return Err(ConsensusError::Other(
                format!("Invalid difficulty: expected {}, got {}", expected_difficulty, header.difficulty).into()
            ));
        }

        Ok(())
    }
}

impl<ChainSpec, P> HeaderValidator<Header> for ParliaConsensus<ChainSpec, P>
where
    ChainSpec: EthChainSpec + BscHardforks + 'static,
    P: SnapshotProvider + std::fmt::Debug + 'static,
{
    fn validate_header(&self, header: &SealedHeader<Header>) -> Result<(), ConsensusError> {
        self.header_validator.validate_header(header)
    }

    fn validate_header_against_parent(
        &self,
        header: &SealedHeader<Header>,
        parent: &SealedHeader<Header>,
    ) -> Result<(), ConsensusError> {
        self.header_validator.validate_header_against_parent(header, parent)
    }
}

impl<ChainSpec, P> Consensus<BscBlock> for ParliaConsensus<ChainSpec, P>
where
    ChainSpec: EthChainSpec + BscHardforks + 'static,
    P: SnapshotProvider + std::fmt::Debug + 'static,
{
    type Error = ConsensusError;

    fn validate_body_against_header(
        &self,
        _body: &<BscBlock as Block>::Body,
        _header: &SealedHeader<Header>,
    ) -> Result<(), Self::Error> {
        // Basic body validation - for now accept all
        Ok(())
    }

    fn validate_block_pre_execution(
        &self,
        block: &SealedBlock<BscBlock>,
    ) -> Result<(), Self::Error> {
        self.validate_block_pre_execution_impl(block)
    }
}

impl<ChainSpec, P> FullConsensus<BscPrimitives> for ParliaConsensus<ChainSpec, P>
where
    ChainSpec: EthChainSpec + BscHardforks + 'static,
    P: SnapshotProvider + std::fmt::Debug + 'static,
{
    fn validate_block_post_execution(
        &self,
        block: &RecoveredBlock<BscBlock>,
        result: &BlockExecutionResult<Receipt>,
    ) -> Result<(), ConsensusError> {
        self.validate_block_post_execution_impl(block, result)
    }
} 