use super::executor::BscBlockExecutor;
use crate::evm::transaction::BscTxEnv;
use reth_chainspec::{EthChainSpec, EthereumHardforks, Hardforks};
use reth_evm::{eth::receipt_builder::ReceiptBuilder, execute::BlockExecutionError, Database, Evm, FromRecoveredTx, FromTxWithEncoded, IntoTxEnv};
use reth_primitives::TransactionSigned;
use reth_revm::State;
use revm::context::BlockEnv;
use alloy_consensus::TxReceipt;
// use consensus trait object for cascading validation

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
    /// check the new block, pre check and prepare some intermediate data for commit parlia snapshot in finish function.
    /// depends on parlia, header and snapshot.
    pub(crate) fn check_new_block(&mut self, block: &BlockEnv) -> Result<(), BlockExecutionError> {
        let block_number = block.number.to::<u64>();

        let header = self
            .snapshot_provider
            .as_ref()
            .unwrap()
            .get_checkpoint_header(block_number)
            .ok_or(BlockExecutionError::msg("Failed to get header from snapshot provider"))?;

        let parent_header = self
            .snapshot_provider
            .as_ref()
            .unwrap()
            .get_checkpoint_header(block_number - 1)
            .ok_or(BlockExecutionError::msg("Failed to get parent header from snapshot provider"))?;

        let snap = self
            .snapshot_provider
            .as_ref()
            .unwrap()
            .snapshot(block_number-1)
            .ok_or(BlockExecutionError::msg("Failed to get snapshot from snapshot provider"))?;

        // Delegate to Parlia consensus object; no ancestors available here, pass None
        // TODO: move this part logic codes to executor to ensure parlia is lightly.
        let verify_res = self
            .parlia_consensus
            .as_ref()
            .unwrap()
            .verify_cascading_fields(&header, &parent_header, None, &snap);

        if let Err(err) = verify_res {
            let proposer = header.beneficiary;
            let is_inturn = snap.is_inturn(proposer);
            let expected_difficulty: u64 = if is_inturn { 2 } else { 1 };
            let recent_counts = snap.count_recent_proposers();

            tracing::error!(
                target: "bsc::pre_execution",
                error = ?err,
                block_number = header.number,
                parent_number = parent_header.number,
                header_timestamp = header.timestamp,
                parent_timestamp = parent_header.timestamp,
                proposer = %format!("0x{:x}", proposer),
                validators_len = snap.validators.len(),
                epoch_len = snap.epoch_num,
                turn_length = snap.turn_length.map(|v| v as u64),
                block_interval = snap.block_interval,
                is_inturn,
                expected_difficulty,
                header_difficulty = %format!("{}", header.difficulty),
                miner_history_check_len = snap.miner_history_check_len(),
                recent_proposers = %format!("{:?}", snap.recent_proposers),
                recent_counts = %format!("{:?}", recent_counts),
                "Consensus verify_cascading_fields failed with detailed diagnostics"
            );

            return Err(err);
        }

        // TODO: query finalise input from parlia consensus object.

        Ok(())
    }
}