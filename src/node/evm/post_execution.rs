use super::executor::BscBlockExecutor;
use crate::consensus::parlia::{DIFF_INTURN, VoteAddress, Snapshot};
use crate::evm::transaction::BscTxEnv;
use reth_chainspec::{EthChainSpec, EthereumHardforks, Hardforks};
use reth_evm::{eth::receipt_builder::ReceiptBuilder, execute::BlockExecutionError, Database, Evm, FromRecoveredTx, FromTxWithEncoded, IntoTxEnv};
use reth_primitives::TransactionSigned;
use reth_revm::State;
use revm::context::BlockEnv;
use alloy_consensus::{Header, TxReceipt};
use alloy_primitives::{Address, hex};
use std::collections::HashMap;
use tracing::debug;

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
    pub(crate) fn finalize_new_block(&mut self, block: &BlockEnv) -> Result<(), BlockExecutionError> {
        tracing::info!("Finalize new block, block_number: {}", block.number);

        // Consensus: Verify validators
        self.verify_validators(self.inner_ctx.current_validators.clone(), self.inner_ctx.header.clone())?;
        // Consensus: Verify turn length
        self.verify_turn_length(self.inner_ctx.snap.clone(), self.inner_ctx.header.clone())?;

        // TODO: finalize the system txs.
        if block.difficulty != DIFF_INTURN {

        }
        Ok(())
    }

    fn verify_validators(&mut self, current_validators: Option<(Vec<Address>, HashMap<Address, VoteAddress>)>, header: Option<Header>) -> Result<(), BlockExecutionError> {
        let header_ref = header.as_ref().unwrap();
        let epoch_length = self.parlia_consensus.as_ref().unwrap().get_epoch_length(header_ref);
        if header_ref.number % epoch_length != 0 {
            return Ok(());
        }

        let (mut validators, mut vote_addrs_map) =
            current_validators.ok_or(BlockExecutionError::msg("Invalid current validators data"))?;
        validators.sort();

        let validator_num = validators.len();
        if self.spec.is_luban_transition_at_block(header_ref.number) {
            vote_addrs_map = validators
                .iter()
                .copied()
                .zip(vec![VoteAddress::default(); validator_num])
                .collect::<HashMap<_, _>>();
        }

        let validator_bytes: Vec<u8> = validators
            .into_iter()
            .flat_map(|v| {
                let mut bytes = v.to_vec();
                if self.spec.is_luban_active_at_block(header_ref.number) {
                    bytes.extend_from_slice(vote_addrs_map[&v].as_ref());
                }
                bytes
            })
            .collect();

        let expected = self.parlia_consensus.as_ref().unwrap().get_validator_bytes_from_header(header_ref).unwrap();
        if !validator_bytes.as_slice().eq(expected.as_slice()) {
            debug!("validator bytes: {:?}", hex::encode(validator_bytes));
            debug!("expected: {:?}", hex::encode(expected));
            return Err(BlockExecutionError::msg("Invalid validators"));
        }

        Ok(())
    }

    fn verify_turn_length(&mut self, _snap: Option<Snapshot>, _header: Option<Header>) -> Result<(), BlockExecutionError> {
        Ok(())
    }
}