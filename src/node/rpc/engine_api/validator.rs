use crate::{
    node::primitives::{BscBlock, BscPrimitives},
    chainspec::BscChainSpec,
};
use reth::{
    api::{FullNodeComponents, NodeTypes},
    builder::{rpc::EngineValidatorBuilder, AddOnsContext},
};
use reth_engine_primitives::{EngineValidator, PayloadValidator};
use reth_node_api::PayloadTypes;
use reth_payload_primitives::{
    EngineApiMessageVersion, EngineObjectValidationError, NewPayloadError, PayloadOrAttributes,
};

/// A BSC engine validator that bypasses all validation.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct BscEngineValidator;

impl PayloadValidator for BscEngineValidator {
    type Block = BscBlock;
    type ExecutionData = alloy_rpc_types_engine::ExecutionData;

    fn ensure_well_formed_payload(
        &self,
        _payload: Self::ExecutionData,
    ) -> Result<reth_primitives::RecoveredBlock<Self::Block>, NewPayloadError> {
        // This is a no-op validator, so we can just return an empty block.
        // The block will be properly validated by the consensus engine.
        let block = BscBlock::default();
        let recovered = reth_primitives::RecoveredBlock::new(
            block.clone(),
            Vec::new(),
            block.header.hash_slow(),
        );
        Ok(recovered)
    }
}

impl<Types> EngineValidator<Types> for BscEngineValidator
where
    Types: PayloadTypes<ExecutionData = alloy_rpc_types_engine::ExecutionData>,
{
    fn validate_version_specific_fields(
        &self,
        _version: EngineApiMessageVersion,
        _payload_or_attrs: PayloadOrAttributes<'_, <Types as PayloadTypes>::ExecutionData, <Types as PayloadTypes>::PayloadAttributes>,
    ) -> Result<(), EngineObjectValidationError> {
        Ok(())
    }

    fn ensure_well_formed_attributes(
        &self,
        _version: EngineApiMessageVersion,
        attributes: &<Types as PayloadTypes>::PayloadAttributes,
    ) -> Result<(), EngineObjectValidationError> {
        tracing::debug!(target:"bsc_validator","ensure_well_formed_attributes:{:?}", attributes);
        Ok(())
    }

    fn validate_payload_attributes_against_header(
        &self,
        _attr: &<Types as PayloadTypes>::PayloadAttributes,
        _header: &<Self::Block as reth_primitives_traits::Block>::Header,
    ) -> Result<(), reth_payload_primitives::InvalidPayloadAttributesError> {
        // Skip timestamp validation for BSC
        Ok(())
    }
}

/// Builder that instantiates the `BscEngineValidator`.
#[derive(Debug, Default, Clone)]
pub struct BscEngineValidatorBuilder;

impl<Node> EngineValidatorBuilder<Node> for BscEngineValidatorBuilder
where
    Node: FullNodeComponents,
    Node::Types: NodeTypes<
        ChainSpec = BscChainSpec,
        Primitives = BscPrimitives,
        Payload = crate::node::rpc::engine_api::payload::BscPayloadTypes,
    >,
{
    type Validator = BscEngineValidator;

    async fn build(self, _ctx: &AddOnsContext<'_, Node>) -> eyre::Result<Self::Validator> {
        Ok(BscEngineValidator::default())
    }
}
