use reth_bsc::consensus::parlia::hooks::{ParliaHooks, PreExecutionHook};
use reth_bsc::consensus::parlia::snapshot::Snapshot;
use bytes::Bytes;
use reth_bsc::consensus::parlia::hooks::SystemTxMaker;
use reth_bsc::system_contracts::SLASH_CONTRACT;
use alloy_primitives::{Address, U256};

#[test]
fn reward_tx_sent_to_beneficiary() {
    struct DummyMaker;
    impl SystemTxMaker for DummyMaker {
        type Tx = reth_primitives::TransactionSigned;
        fn make_system_tx(&self, _from: Address, to: Address, _data: Bytes, _value: U256) -> Self::Tx {
            // minimal tx with to address for testing
            reth_primitives::TransactionSigned::new_unhashed(
                reth_primitives::Transaction::Legacy(alloy_consensus::TxLegacy {
                    chain_id: None,
                    nonce: 0,
                    gas_limit: 21000,
                    gas_price: 0,
                    value: U256::ZERO,
                    input: alloy_primitives::Bytes::default(),
                    to: alloy_primitives::TxKind::Call(to),
                }),
                alloy_primitives::Signature::new(Default::default(), Default::default(), false),
            )
        }
    }
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
    // mark beneficiary as recently proposer to trigger sign_recently true
    snap.recent_proposers.insert(0, beneficiary);

    let out = (ParliaHooks, &maker).on_pre_execution(&snap, beneficiary, true);
    assert_eq!(out.system_txs.len(), 1);
    let tx = &out.system_txs[0];
    assert_eq!(tx.to().unwrap(), SLASH_CONTRACT.parse::<Address>().unwrap());
} 