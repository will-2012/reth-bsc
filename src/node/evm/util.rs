use reth_primitives::Transaction;

pub fn set_nonce(transaction: Transaction, nonce: u64) -> Transaction {
    match transaction {
        Transaction::Legacy(mut tx) => {
            tx.nonce = nonce;
            Transaction::Legacy(tx)
        },
        Transaction::Eip2930(mut tx) => {
            tx.nonce = nonce;
            Transaction::Eip2930(tx)
        },
        Transaction::Eip1559(mut tx) => {
            tx.nonce = nonce;
            Transaction::Eip1559(tx)
        },
        Transaction::Eip4844(mut tx) => {
            tx.nonce = nonce;
            Transaction::Eip4844(tx)
        },
        Transaction::Eip7702(mut tx) => {
            tx.nonce = nonce;
            Transaction::Eip7702(tx)
        },
    }
}