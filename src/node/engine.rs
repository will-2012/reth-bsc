use std::sync::Arc;

use crate::node::primitives::BscPrimitives;
use reth_primitives::{Block, BlockBody};
use alloy_eips::eip7685::Requests;
use alloy_primitives::U256;
use reth::{
    api::{FullNodeTypes, NodeTypes},
    builder::{components::PayloadServiceBuilder, BuilderContext},
    payload::{PayloadBuilderHandle},
    transaction_pool::TransactionPool,
    tasks::TaskSpawner,
};
use reth_evm::ConfigureEvm;
use reth_payload_primitives::{BuiltPayload, PayloadBuilderError, PayloadBuilderAttributes};
use reth_primitives::{SealedBlock, Header};

// Additional imports for the actual payload builder
use reth_basic_payload_builder::{
    BasicPayloadJobGenerator, BasicPayloadJobGeneratorConfig, PayloadBuilder, BuildArguments, 
    BuildOutcome, PayloadConfig
};
use reth_payload_builder::{PayloadBuilderService};
use reth_node_builder::PayloadBuilderConfig;
use reth_provider::{CanonStateSubscriptions};

// Additional imports for execution payload conversions
use alloy_rpc_types_engine::{
    BlobsBundleV1, BlobsBundleV2, ExecutionPayloadEnvelopeV2, ExecutionPayloadEnvelopeV3,
    ExecutionPayloadEnvelopeV4, ExecutionPayloadEnvelopeV5, ExecutionPayloadFieldV2,
    ExecutionPayloadV1, ExecutionPayloadV3,
};
// Bring `Block` trait into scope so we can use its extension methods
use reth_primitives_traits::Block as _;
use core::convert::Infallible;
use alloy_consensus::EMPTY_OMMER_ROOT_HASH;
use alloy_eips::merge::BEACON_NONCE;
use alloy_consensus::proofs::{calculate_transaction_root, calculate_receipt_root};
use alloy_primitives::{Sealable, keccak256};
use reth_primitives::TransactionSigned;
use reth_ethereum_primitives::Receipt;

/// Built payload for BSC. This is similar to [`EthBuiltPayload`] but without sidecars as those
/// included into [`BscBlock`].
#[derive(Debug, Clone)]
pub struct BscBuiltPayload {
    /// The built block
    pub(crate) block: Arc<SealedBlock<Block>>,
    /// The fees of the block
    pub(crate) fees: U256,
    /// The requests of the payload
    pub(crate) requests: Option<Requests>,
}

impl BscBuiltPayload {
    /// Creates a new BSC built payload
    pub fn new(
        block: Arc<SealedBlock<Block>>, 
        fees: U256, 
        requests: Option<Requests>,
    ) -> Self {
        Self { block, fees, requests }
    }

    /// Creates a simple empty BSC block for testing
    pub fn empty_for_test(
        parent_header: &Header,
        attributes: &crate::node::rpc::engine_api::payload::BscPayloadBuilderAttributes,
    ) -> Self {
        // Create a simple empty block
        let header = Header {
            parent_hash: parent_header.hash_slow(),
            ommers_hash: EMPTY_OMMER_ROOT_HASH,
            beneficiary: attributes.suggested_fee_recipient(),
            state_root: parent_header.state_root, // Use parent's state root for empty block
            transactions_root: calculate_transaction_root::<TransactionSigned>(&[]),
            receipts_root: calculate_receipt_root::<Receipt>(&[]),
            withdrawals_root: None,
            logs_bloom: Default::default(),
            timestamp: attributes.timestamp(),
            mix_hash: attributes.prev_randao(),
            nonce: BEACON_NONCE.into(),
            base_fee_per_gas: parent_header.base_fee_per_gas,
            number: parent_header.number + 1,
            gas_limit: parent_header.gas_limit,
            difficulty: U256::ZERO,
            gas_used: 0,
            extra_data: Default::default(),
            parent_beacon_block_root: attributes.parent_beacon_block_root(),
            blob_gas_used: None,
            excess_blob_gas: None,
            requests_hash: None,
        };

        let body = BlockBody {
            transactions: vec![],
            ommers: vec![],
            withdrawals: None,
        };

        let block = Block::new(header, body);
        let sealed_block = block.seal_slow();

        Self {
            block: Arc::new(sealed_block),
            fees: U256::ZERO,
            requests: None,
        }
    }

    /// Returns the payload ID (for now, use the block hash)
    pub fn id(&self) -> alloy_rpc_types_engine::PayloadId {
        alloy_rpc_types_engine::PayloadId::new([0u8; 8]) // Simplified for now
    }
}

impl BuiltPayload for BscBuiltPayload {
    type Primitives = BscPrimitives;

    fn block(&self) -> &SealedBlock<Block> {
        self.block.as_ref()
    }

    fn fees(&self) -> U256 {
        self.fees
    }

    fn requests(&self) -> Option<Requests> {
        self.requests.clone()
    }
}

// === Conversion impls to satisfy `EngineTypes` bounds ===

// V1 engine_getPayloadV1 response
impl From<BscBuiltPayload> for ExecutionPayloadV1 {
    fn from(value: BscBuiltPayload) -> Self {
        let sealed_block = Arc::unwrap_or_clone(value.block);
        // Convert custom BSC block into the canonical ethereum block representation so that
        // `from_block_unchecked` accepts it.
        let eth_block = sealed_block.clone().into_block().into_ethereum_block();

        Self::from_block_unchecked(sealed_block.hash(), &eth_block)
    }
}

// V2 engine_getPayloadV2 response
impl From<BscBuiltPayload> for ExecutionPayloadEnvelopeV2 {
    fn from(value: BscBuiltPayload) -> Self {
        let BscBuiltPayload { block, fees, .. } = value;

        let sealed_block = Arc::unwrap_or_clone(block);
        let eth_block = sealed_block.clone().into_block().into_ethereum_block();

        Self {
            block_value: fees,
            execution_payload: ExecutionPayloadFieldV2::from_block_unchecked(
                sealed_block.hash(),
                &eth_block,
            ),
        }
    }
}

impl TryFrom<BscBuiltPayload> for ExecutionPayloadEnvelopeV3 {
    type Error = Infallible;

    fn try_from(value: BscBuiltPayload) -> Result<Self, Self::Error> {
        let BscBuiltPayload { block, fees, .. } = value;

        let sealed_block = Arc::unwrap_or_clone(block);
        let eth_block = sealed_block.clone().into_block().into_ethereum_block();

        Ok(ExecutionPayloadEnvelopeV3 {
            execution_payload: ExecutionPayloadV3::from_block_unchecked(
                sealed_block.hash(),
                &eth_block,
            ),
            block_value: fees,
            should_override_builder: false,
            blobs_bundle: BlobsBundleV1::empty(),
        })
    }
}

impl TryFrom<BscBuiltPayload> for ExecutionPayloadEnvelopeV4 {
    type Error = Infallible;

    fn try_from(value: BscBuiltPayload) -> Result<Self, Self::Error> {
        let requests = value.requests.clone().unwrap_or_default();
        let envelope_inner: ExecutionPayloadEnvelopeV3 = value.try_into()?;

        Ok(ExecutionPayloadEnvelopeV4 { execution_requests: requests, envelope_inner })
    }
}

impl TryFrom<BscBuiltPayload> for ExecutionPayloadEnvelopeV5 {
    type Error = Infallible;

    fn try_from(value: BscBuiltPayload) -> Result<Self, Self::Error> {
        let BscBuiltPayload { block, fees, requests, .. } = value;

        let sealed_block = Arc::unwrap_or_clone(block);
        let eth_block = sealed_block.clone().into_block().into_ethereum_block();

        Ok(ExecutionPayloadEnvelopeV5 {
            execution_payload: ExecutionPayloadV3::from_block_unchecked(
                sealed_block.hash(),
                &eth_block,
            ),
            block_value: fees,
            should_override_builder: false,
            blobs_bundle: BlobsBundleV2::empty(),
            execution_requests: requests.unwrap_or_default(),
        })
    }
}

/// Simple BSC payload builder that creates empty blocks for testing
#[derive(Debug, Clone)]
pub struct SimpleBscPayloadBuilder;

impl PayloadBuilder for SimpleBscPayloadBuilder {
    type Attributes = crate::node::rpc::engine_api::payload::BscPayloadBuilderAttributes;
    type BuiltPayload = BscBuiltPayload;

    fn try_build(
        &self,
        args: BuildArguments<Self::Attributes, Self::BuiltPayload>,
    ) -> Result<BuildOutcome<Self::BuiltPayload>, PayloadBuilderError> {
        // Create a simple empty block for testing
        let payload = BscBuiltPayload::empty_for_test(&args.config.parent_header, &args.config.attributes);
        
        Ok(BuildOutcome::Better { 
            payload, 
            cached_reads: args.cached_reads 
        })
    }

    fn build_empty_payload(
        &self,
        config: PayloadConfig<Self::Attributes>,
    ) -> Result<Self::BuiltPayload, PayloadBuilderError> {
        // Create a simple empty block
        let payload = BscBuiltPayload::empty_for_test(&config.parent_header, &config.attributes);
        Ok(payload)
    }
}

#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct BscPayloadServiceBuilder;

impl<Node, Pool, Evm> PayloadServiceBuilder<Node, Pool, Evm> for BscPayloadServiceBuilder
where
    Node: FullNodeTypes,
    Node::Types: NodeTypes<Primitives = crate::node::primitives::BscPrimitives, ChainSpec = crate::chainspec::BscChainSpec, Payload = crate::node::rpc::engine_api::payload::BscPayloadTypes, StateCommitment = reth_trie_db::MerklePatriciaTrie, Storage = crate::node::storage::BscStorage>,
    Pool: TransactionPool + Unpin + 'static,
    Evm: ConfigureEvm + Clone + Unpin + 'static,
{
    async fn spawn_payload_builder_service(
        self,
        ctx: &BuilderContext<Node>,
        pool: Pool,
        _evm_config: Evm,
    ) -> eyre::Result<PayloadBuilderHandle<crate::node::rpc::engine_api::payload::BscPayloadTypes>> {
        // Create the simple BSC payload builder
        let payload_builder = SimpleBscPayloadBuilder;
        let conf = ctx.payload_builder_config();

        // Configure the payload job generator
        let payload_job_config = BasicPayloadJobGeneratorConfig::default()
            .interval(conf.interval())
            .deadline(conf.deadline())
            .max_payload_tasks(conf.max_payload_tasks());

        // Create the payload job generator with BSC payload builder
        let payload_generator = BasicPayloadJobGenerator::with_builder(
            ctx.provider().clone(),
            ctx.task_executor().clone(),
            payload_job_config,
            payload_builder,
        );

        // Create the payload builder service
        let (payload_service, payload_builder_handle) =
            PayloadBuilderService::new(payload_generator, ctx.provider().canonical_state_stream());

        // Spawn the service
        ctx.task_executor().spawn_critical("bsc payload builder service", Box::pin(payload_service));

        Ok(payload_builder_handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, B256};
    use reth_primitives::Header;
    use crate::node::rpc::engine_api::payload::BscPayloadBuilderAttributes;
    use reth_payload_builder::EthPayloadBuilderAttributes;
    use alloy_rpc_types_engine::PayloadAttributes;
    use alloy_consensus::BlockHeader;
    use reth_primitives_traits::{SealedHeader, Block as _};
    
    #[test]
    fn test_simple_bsc_payload_builder() {
        // Create a test parent header
        let parent_header = Header::default();
        
        // Create test attributes
        let eth_attrs = PayloadAttributes {
            timestamp: 1000,
            prev_randao: B256::random(),
            suggested_fee_recipient: Address::random(),
            withdrawals: None,
            parent_beacon_block_root: None,
        };
        let bsc_attrs = BscPayloadBuilderAttributes::from(
            EthPayloadBuilderAttributes::new(B256::ZERO, eth_attrs)
        );
        
        // Test empty_for_test
        let payload = BscBuiltPayload::empty_for_test(&parent_header, &bsc_attrs);
        
        // Verify the payload was created correctly
        assert_eq!(payload.block().number, parent_header.number + 1);
        assert_eq!(payload.block().timestamp, bsc_attrs.timestamp());
        assert_eq!(payload.block().body().transactions().count(), 0);
        assert_eq!(payload.fees(), U256::ZERO);
        
        println!("✓ BscBuiltPayload::empty_for_test works correctly");
        
        // Test the payload builder
        let builder = SimpleBscPayloadBuilder;
        let config = PayloadConfig::new(
            Arc::new(SealedHeader::new(parent_header.clone(), parent_header.hash_slow())), 
            bsc_attrs.clone()
        );
        
        // Test build_empty_payload
        let empty_payload = builder.build_empty_payload(config.clone()).unwrap();
        assert_eq!(empty_payload.block().number, parent_header.number + 1);
        
        println!("✓ SimpleBscPayloadBuilder::build_empty_payload works correctly");
        
        // Test try_build with BuildArguments
        let args = BuildArguments::new(
            Default::default(), // cached_reads
            config,
            Default::default(), // cancel
            None, // best_payload
        );
        
        let result = builder.try_build(args).unwrap();
        match result {
            BuildOutcome::Better { payload, .. } => {
                assert_eq!(payload.block().number, parent_header.number + 1);
                println!("✓ SimpleBscPayloadBuilder::try_build works correctly");
            }
            _ => panic!("Expected Better outcome"),
        }
    }
}
