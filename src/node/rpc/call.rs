use super::{BscEthApi, BscNodeCore};
use crate::evm::transaction::BscTxEnv;
use alloy_rpc_types::TransactionRequest;
use reth::rpc::server_types::eth::EthApiError;
use reth_evm::{block::BlockExecutorFactory, ConfigureEvm, EvmFactory, TxEnvFor};
use reth_primitives::NodePrimitives;
use reth_provider::{ProviderError, ProviderHeader, ProviderTx};
use reth_rpc_eth_api::{
    helpers::{estimate::EstimateCall, Call, EthCall, LoadBlock, LoadState, SpawnBlocking},
    FromEvmError, FullEthApiTypes, RpcConvert, RpcTypes,
};

impl<N> EthCall for BscEthApi<N>
where
    Self: EstimateCall + LoadBlock + FullEthApiTypes,
    N: BscNodeCore,
{
}

impl<N> EstimateCall for BscEthApi<N>
where
    Self: Call,
    Self::Error: From<EthApiError>,
    N: BscNodeCore,
{
}

impl<N> Call for BscEthApi<N>
where
    Self: LoadState<
            Evm: ConfigureEvm<
                Primitives: NodePrimitives<
                    BlockHeader = ProviderHeader<Self::Provider>,
                    SignedTx = ProviderTx<Self::Provider>,
                >,
                BlockExecutorFactory: BlockExecutorFactory<EvmFactory: EvmFactory<Tx = BscTxEnv>>,
            >,
            Error: FromEvmError<Self::Evm>,
            RpcConvert: RpcConvert<
                TxEnv = TxEnvFor<Self::Evm>,
                Network: RpcTypes<TransactionRequest: From<TransactionRequest>>,
            >,
        > + SpawnBlocking,
    Self::Error:
        From<EthApiError> + From<<Self::RpcConvert as RpcConvert>::Error> + From<ProviderError>,
    N: BscNodeCore,
{
    #[inline]
    fn call_gas_limit(&self) -> u64 {
        self.inner.eth_api.gas_cap()
    }

    #[inline]
    fn max_simulate_blocks(&self) -> u64 {
        self.inner.eth_api.max_simulate_blocks()
    }
}
