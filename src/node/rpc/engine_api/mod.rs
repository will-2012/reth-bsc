use jsonrpsee_core::server::RpcModule;
use reth_rpc_api::IntoEngineApiRpcModule;
use reth_rpc_engine_api::EngineApi;
use reth_node_api::EngineTypes;
use reth_payload_primitives::PayloadTypes;

/// Re-export sub-modules
pub mod builder;
pub mod payload;
pub mod validator;

/// List of Engine API capabilities that the BSC execution client supports.
/// This mirrors the Cancun-capable capability set of regular Ethereum clients
/// (no Optimism-specific extensions).
pub const BSC_ENGINE_CAPABILITIES: &[&str] = &[
    "engine_forkchoiceUpdatedV1",
    "engine_forkchoiceUpdatedV2",
    "engine_forkchoiceUpdatedV3",
    "engine_getPayloadV2",
    "engine_getPayloadV3",
    "engine_getPayloadV4",
    "engine_newPayloadV2",
    "engine_newPayloadV3",
    "engine_newPayloadV4",
    "engine_getPayloadBodiesByHashV1",
    "engine_getPayloadBodiesByRangeV1",
    "engine_getClientVersionV1",
    "engine_exchangeCapabilities",
];

/// Thin wrapper around the generic [`EngineApi`] that only adds a new-type
/// so we can hook it into the node-builder’s add-on system.
#[derive(Clone)]
pub struct BscEngineApi<Provider, EngineT, Pool, Validator, ChainSpec>
where
    EngineT: PayloadTypes + EngineTypes,
{
    inner: EngineApi<Provider, EngineT, Pool, Validator, ChainSpec>,
}

impl<Provider, EngineT, Pool, Validator, ChainSpec>
    BscEngineApi<Provider, EngineT, Pool, Validator, ChainSpec>
where
    EngineT: PayloadTypes + EngineTypes,
{
    pub fn new(inner: EngineApi<Provider, EngineT, Pool, Validator, ChainSpec>) -> Self {
        Self { inner }
    }
}

impl<Provider, EngineT, Pool, Validator, ChainSpec> std::fmt::Debug
    for BscEngineApi<Provider, EngineT, Pool, Validator, ChainSpec>
where
    EngineT: PayloadTypes + EngineTypes,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BscEngineApi").finish_non_exhaustive()
    }
}

// ---- RPC glue ----
impl<Provider, EngineT, Pool, Validator, ChainSpec> IntoEngineApiRpcModule
    for BscEngineApi<Provider, EngineT, Pool, Validator, ChainSpec>
where
    EngineT: PayloadTypes + EngineTypes,
    Provider: reth_provider::HeaderProvider
        + reth_provider::BlockReader
        + reth_provider::StateProviderFactory
        + Send
        + Sync
        + 'static,
    Pool: reth_transaction_pool::TransactionPool + Clone + 'static,
    Validator: reth_engine_primitives::EngineValidator<EngineT> + Clone + 'static,
    ChainSpec: reth_chainspec::EthereumHardforks + Send + Sync + 'static,
    EngineApi<Provider, EngineT, Pool, Validator, ChainSpec>: reth_rpc_api::servers::EngineApiServer<EngineT>,
{
    fn into_rpc_module(self) -> RpcModule<()> {
        // Delegates to the inner EngineApi implementation so that all Engine API methods
        // (`engine_forkchoiceUpdatedV*`, `engine_getPayloadV*`, etc.) are exposed over JSON-RPC.
        self.inner.into_rpc_module()
    }
}

// Note: we intentionally do NOT implement `EngineApiServer` for the wrapper here.  
// The node-builder only requires the Engine API object to implement `IntoEngineApiRpcModule`,
// which we provide below by delegating to the inner `EngineApi` instance.
