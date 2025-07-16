use bytes::Bytes;

use reth_bsc::consensus::parlia::hooks::{ParliaHooks, PreExecutionHook, SystemTxMaker};
use reth_bsc::consensus::parlia::snapshot::Snapshot;
use reth_bsc::SLASH_CONTRACT;
use alloy_primitives::{Address, U256};
use alloy_consensus::Transaction as _;

// Dummy maker that builds minimal transactions for testing
struct DummyMaker;

impl SystemTxMaker for DummyMaker {
    type Tx = reth_primitives::TransactionSigned;

    fn make_system_tx(&self, _from: Address, to: Address, _data: Bytes, value: U256) -> Self::Tx {
        // minimal tx that preserves `value` for testing
        reth_primitives::TransactionSigned::new_unhashed(
            reth_primitives::Transaction::Legacy(alloy_consensus::TxLegacy {
                chain_id: None,
                nonce: 0,
                gas_limit: 21000,
                gas_price: 0,
                value,
                input: alloy_primitives::Bytes::default(),
                to: alloy_primitives::TxKind::Call(to),
            }),
            alloy_primitives::Signature::new(Default::default(), Default::default(), false),
        )
    }
}

// Implement SystemTxMaker for a reference to DummyMaker since the hooks expect &M
impl<'a> SystemTxMaker for &'a DummyMaker {
    type Tx = reth_primitives::TransactionSigned;

    fn make_system_tx(
        &self,
        from: Address,
        to: Address,
        data: Bytes,
        value: U256,
    ) -> Self::Tx {
        (*self).make_system_tx(from, to, data, value)
    }
}

#[test]
fn reward_tx_sent_to_beneficiary() {
    let maker = DummyMaker;

    let snap = Snapshot::default();
    let beneficiary = Address::repeat_byte(0x01);
    let out = (ParliaHooks, &maker).on_pre_execution(&snap, beneficiary, true);
    assert_eq!(out.system_txs.len(), 1);
    let tx = &out.system_txs[0];
    assert_eq!(tx.to().unwrap(), beneficiary);
    assert_eq!(tx.value(), U256::from(4_000_000_000_000_000_000u128)); // double reward in-turn
}

#[test]
fn slash_tx_sent_when_over_proposed() {
    let maker = DummyMaker;

    let mut snap = Snapshot::default();
    let beneficiary = Address::repeat_byte(0x02);
    // Set up snapshot so that beneficiary appears in recent proposer window
    snap.block_number = 1;
    // Provide a minimal validator set so `miner_history_check_len` becomes >0.
    snap.validators.push(beneficiary);
    snap.validators.push(Address::repeat_byte(0x03));
    snap.recent_proposers.insert(1, beneficiary);

    let out = (ParliaHooks, &maker).on_pre_execution(&snap, beneficiary, true);
    assert_eq!(out.system_txs.len(), 1);
    let tx = &out.system_txs[0];
    assert_eq!(tx.to().unwrap(), SLASH_CONTRACT.parse::<Address>().unwrap());
} 