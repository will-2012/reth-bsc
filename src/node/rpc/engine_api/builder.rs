use super::{BscEngineApi, BSC_ENGINE_CAPABILITIES};
use alloy_rpc_types_engine::ClientVersionV1;
use reth_chainspec::EthereumHardforks;
use reth_node_api::{AddOnsContext, EngineTypes, FullNodeComponents, NodeTypes};
use reth_node_builder::rpc::{EngineApiBuilder, EngineValidatorBuilder};
use reth_node_core::version::{CARGO_PKG_VERSION, CLIENT_CODE, VERGEN_GIT_SHA};
use reth_payload_builder::PayloadStore;
use reth_rpc_engine_api::{EngineApi, EngineCapabilities};
use reth_payload_primitives::PayloadTypes;
use alloy_rpc_types_engine::ExecutionData;

/// Generic builder that wires the BSC Engine API into the node depending on the
/// concrete `EngineValidatorBuilder` supplied by the node‐builder macros.
#[derive(Debug, Default, Clone)]
pub struct BscEngineApiBuilder<EV> {
    pub(crate) engine_validator_builder: EV,
}

impl<N, EV> EngineApiBuilder<N> for BscEngineApiBuilder<EV>
where
    // The node must expose all the usual full-node components (provider, pool, etc.).
    N: FullNodeComponents,
    // Additional bounds so we can extract associated types.
    N::Types: NodeTypes,
    // The node's payload type must implement `EngineTypes` and expose the canonical ExecutionData.
    <N::Types as NodeTypes>::Payload: EngineTypes + PayloadTypes<ExecutionData = ExecutionData>,
    // The chain spec must support hardfork checks required by EngineApi.
    <N::Types as NodeTypes>::ChainSpec: EthereumHardforks + Send + Sync + 'static,
    // Make sure the payload’s `ExecutionData` is declared (we don’t depend on it here).
    EV: EngineValidatorBuilder<N>,
{
    type EngineApi = BscEngineApi<
        N::Provider,
        <N::Types as NodeTypes>::Payload,
        N::Pool,
        EV::Validator,
        <N::Types as NodeTypes>::ChainSpec,
    >;

    async fn build_engine_api(self, ctx: &AddOnsContext<'_, N>) -> eyre::Result<Self::EngineApi> {
        let Self { engine_validator_builder } = self;

        // Build the execution-payload validator first.
        let engine_validator = engine_validator_builder.build(ctx).await?;

        // Version info that the consensus client will read via engine_getClientVersionV1.
        let client = ClientVersionV1 {
            code: CLIENT_CODE,
            name: "reth-bsc".to_string(),
            version: CARGO_PKG_VERSION.to_string(),
            commit: VERGEN_GIT_SHA.to_string(),
        };

        // Construct the generic EngineApi instance that does all the heavy lifting.
        let inner = EngineApi::new(
            ctx.node.provider().clone(),
            ctx.config.chain.clone(),
            ctx.beacon_engine_handle.clone(),
            PayloadStore::new(ctx.node.payload_builder_handle().clone()),
            ctx.node.pool().clone(),
            Box::new(ctx.node.task_executor().clone()),
            client,
            EngineCapabilities::new(BSC_ENGINE_CAPABILITIES.iter().copied()),
            engine_validator,
            ctx.config.engine.accept_execution_requests_hash,
        );

        Ok(BscEngineApi::new(inner))
    }
}
