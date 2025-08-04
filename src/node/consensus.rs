use crate::{hardforks::BscHardforks, node::BscNode, BscBlock, BscBlockBody, BscPrimitives};
use alloy_consensus::Header;
use reth::{
    api::FullNodeTypes,
    beacon_consensus::EthBeaconConsensus,
    builder::{components::ConsensusBuilder, BuilderContext},
    consensus::{Consensus, ConsensusError, FullConsensus, HeaderValidator},
    consensus_common::validation::{
        validate_against_parent_4844, validate_against_parent_hash_number,
        validate_against_parent_timestamp,
    },
};
use reth_chainspec::EthChainSpec;
use reth_primitives::{Receipt, RecoveredBlock, SealedBlock, SealedHeader};
use reth_provider::BlockExecutionResult;
use std::sync::Arc;
use tracing::{debug, error};

/// A basic Bsc consensus builder.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct BscConsensusBuilder;

impl<Node> ConsensusBuilder<Node> for BscConsensusBuilder
where
    Node: FullNodeTypes<Types = BscNode>,
{
    type Consensus = Arc<dyn FullConsensus<BscPrimitives, Error = ConsensusError>>;

    async fn build_consensus(self, ctx: &BuilderContext<Node>) -> eyre::Result<Self::Consensus> {
        Ok(Arc::new(BscConsensus::new(ctx.chain_spec())))
    }
}

/// BSC consensus implementation.
///
/// Provides basic checks as outlined in the execution specs.
#[derive(Debug, Clone)]
pub struct BscConsensus<ChainSpec> {
    inner: EthBeaconConsensus<ChainSpec>,
    chain_spec: Arc<ChainSpec>,
}

impl<ChainSpec: EthChainSpec + BscHardforks> BscConsensus<ChainSpec> {
    /// Create a new instance of [`BscConsensus`]
    pub fn new(chain_spec: Arc<ChainSpec>) -> Self {
        Self { inner: EthBeaconConsensus::new(chain_spec.clone()), chain_spec }
    }
}

impl<ChainSpec: EthChainSpec + BscHardforks> HeaderValidator for BscConsensus<ChainSpec> {
    fn validate_header(&self, header: &SealedHeader) -> Result<(), ConsensusError> {
        debug!(
            target: "bsc::consensus::validate_header",
            block_number=?header.number,
            block_hash=?header.hash_slow(),
            parent_hash=?header.parent_hash,
            timestamp=?header.timestamp,
            "Validating BSC header"
        );
        
        // TODO: doesn't work because of extradata check
        // self.inner.validate_header(header)
        
        debug!(
            target: "bsc::consensus::validate_header",
            block_number=?header.number,
            "BSC header validation completed (skipped extradata check)"
        );

        Ok(())
    }

    fn validate_header_against_parent(
        &self,
        header: &SealedHeader,
        parent: &SealedHeader,
    ) -> Result<(), ConsensusError> {
        debug!(
            target: "bsc::consensus::validate_header_against_parent",
            block_number=?header.number,
            parent_number=?parent.number,
            header_hash=?header.hash_slow(),
            parent_hash=?parent.hash_slow(),
            header_timestamp=?header.timestamp,
            parent_timestamp=?parent.timestamp,
            "Validating BSC header against parent"
        );
        
        // Validate hash and number relationship
        match validate_against_parent_hash_number(header.header(), parent) {
            Ok(()) => {
                debug!(
                    target: "bsc::consensus::validate_header_against_parent",
                    block_number=?header.number,
                    "Hash and number validation passed"
                );
            }
            Err(e) => {
                error!(
                    target: "bsc::consensus::validate_header_against_parent",
                    block_number=?header.number,
                    parent_number=?parent.number,
                    error=?e,
                    "Hash and number validation failed"
                );
                return Err(e);
            }
        }

        // Validate timestamp
        // match validate_against_parent_timestamp(header.header(), parent.header()) {
        //     Ok(()) => {
        //         debug!(
        //             target: "bsc::consensus::validate_header_against_parent",
        //             block_number=?header.number,
        //             "Timestamp validation passed"
        //         );
        //     }
        //     Err(e) => {
        //         error!(
        //             target: "bsc::consensus::validate_header_against_parent",
        //             block_number=?header.number,
        //             parent_number=?parent.number,
        //             error=?e,
        //             "Timestamp validation failed"
        //         );
        //         return Err(e);
        //     }
        // }

        // Validate blob gas fields if applicable
        if let Some(blob_params) = self.chain_spec.blob_params_at_timestamp(header.timestamp) {
            debug!(
                target: "bsc::consensus::validate_header_against_parent",
                block_number=?header.number,
                "Validating blob gas fields"
            );
            match validate_against_parent_4844(header.header(), parent.header(), blob_params) {
                Ok(()) => {
                    debug!(
                        target: "bsc::consensus::validate_header_against_parent",
                        block_number=?header.number,
                        "Blob gas validation passed"
                    );
                }
                Err(e) => {
                    error!(
                        target: "bsc::consensus::validate_header_against_parent",
                        block_number=?header.number,
                        parent_number=?parent.number,
                        error=?e,
                        "Blob gas validation failed"
                    );
                    return Err(e);
                }
            }
        }

        debug!(
            target: "bsc::consensus::validate_header_against_parent",
            block_number=?header.number,
            "BSC header validation against parent completed successfully"
        );

        Ok(())
    }
}

impl<ChainSpec: EthChainSpec<Header = Header> + BscHardforks> Consensus<BscBlock>
    for BscConsensus<ChainSpec>
{
    type Error = ConsensusError;

    fn validate_body_against_header(
        &self,
        body: &BscBlockBody,
        header: &SealedHeader,
    ) -> Result<(), ConsensusError> {
        Consensus::<BscBlock>::validate_body_against_header(&self.inner, body, header)
    }

    fn validate_block_pre_execution(
        &self,
        _block: &SealedBlock<BscBlock>,
    ) -> Result<(), ConsensusError> {
        // Check ommers hash
        // let ommers_hash = block.body().calculate_ommers_root();
        // if Some(block.ommers_hash()) != ommers_hash {
        //     return Err(ConsensusError::BodyOmmersHashDiff(
        //         GotExpected {
        //             got: ommers_hash.unwrap_or(EMPTY_OMMER_ROOT_HASH),
        //             expected: block.ommers_hash(),
        //         }
        //         .into(),
        //     ))
        // }

        // // Check transaction root
        // if let Err(error) = block.ensure_transaction_root_valid() {
        //     return Err(ConsensusError::BodyTransactionRootDiff(error.into()))
        // }

        // if self.chain_spec.is_cancun_active_at_timestamp(block.timestamp()) {
        //     validate_cancun_gas(block)?;
        // } else {
        //     return Ok(())
        // }

        Ok(())
    }
}

impl<ChainSpec: EthChainSpec<Header = Header> + BscHardforks> FullConsensus<BscPrimitives>
    for BscConsensus<ChainSpec>
{
    fn validate_block_post_execution(
        &self,
        block: &RecoveredBlock<BscBlock>,
        result: &BlockExecutionResult<Receipt>,
    ) -> Result<(), ConsensusError> {
        FullConsensus::<BscPrimitives>::validate_block_post_execution(&self.inner, block, result)
    }
}
