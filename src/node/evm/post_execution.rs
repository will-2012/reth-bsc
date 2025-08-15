use super::executor::BscBlockExecutor;
use crate::evm::transaction::BscTxEnv;
use reth_chainspec::{EthChainSpec, EthereumHardforks, Hardforks};
use reth_evm::{eth::receipt_builder::ReceiptBuilder, execute::BlockExecutionError, Database, Evm, FromRecoveredTx, FromTxWithEncoded, IntoTxEnv};
use reth_primitives::TransactionSigned;
use reth_revm::State;
use revm::context::BlockEnv;
use alloy_consensus::TxReceipt;

impl<'a, DB, EVM, Spec, R: ReceiptBuilder> BscBlockExecutor<'a, EVM, Spec, R>
where
    DB: Database + 'a,
    EVM: Evm<
        DB = &'a mut State<DB>,
        Tx: FromRecoveredTx<R::Transaction>
                + FromRecoveredTx<TransactionSigned>
                + FromTxWithEncoded<TransactionSigned>,
    >,
    Spec: EthereumHardforks + crate::hardforks::BscHardforks + EthChainSpec + Hardforks + Clone,
    R: ReceiptBuilder<Transaction = TransactionSigned, Receipt: TxReceipt>,
    <R as ReceiptBuilder>::Transaction: Unpin + From<TransactionSigned>,
    <EVM as alloy_evm::Evm>::Tx: FromTxWithEncoded<<R as ReceiptBuilder>::Transaction>,
    BscTxEnv: IntoTxEnv<<EVM as alloy_evm::Evm>::Tx>,
    R::Transaction: Into<TransactionSigned>,
{
    /// finalize the new block, post check and finalize the system tx.
    /// depends on parlia, header and snapshot.
    pub(crate) fn finalize_new_block(&mut self, _block: &BlockEnv) -> Result<(), BlockExecutionError> {
        // TODO: implement this function.
        // unimplemented!();
        Ok(())
    }
}