#![allow(clippy::owned_cow)]
use alloy_consensus::{BlobTransactionSidecar, Header};
use alloy_primitives::B256;
use alloy_rlp::{Encodable, RlpDecodable, RlpEncodable};
use reth_ethereum_primitives::{BlockBody, Receipt};
use reth_primitives::{NodePrimitives, TransactionSigned};
use reth_primitives_traits::{Block, BlockBody as BlockBodyTrait, InMemorySize};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use tracing::{debug, error};

/// Primitive types for BSC.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct BscPrimitives;

impl NodePrimitives for BscPrimitives {
    type Block = BscBlock;
    type BlockHeader = Header;
    type BlockBody = BscBlockBody;
    type SignedTx = TransactionSigned;
    type Receipt = Receipt;
}

/// BSC representation of a EIP-4844 sidecar.
#[derive(Debug, Clone, PartialEq, Eq, RlpEncodable, RlpDecodable, Serialize, Deserialize)]
pub struct BscBlobTransactionSidecar {
    pub inner: BlobTransactionSidecar,
    pub block_number: u64,
    pub block_hash: B256,
    pub tx_index: u64,
    pub tx_hash: B256,
}

/// Block body for BSC. It is equivalent to Ethereum [`BlockBody`] but additionally stores sidecars
/// for blob transactions.
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_more::Deref,
    derive_more::DerefMut,
)]
pub struct BscBlockBody {
    #[serde(flatten)]
    #[deref]
    #[deref_mut]
    pub inner: BlockBody,
    pub sidecars: Option<Vec<BscBlobTransactionSidecar>>,
}

impl InMemorySize for BscBlockBody {
    fn size(&self) -> usize {
        self.inner.size() +
            self.sidecars
                .as_ref()
                .map_or(0, |s| s.capacity() * core::mem::size_of::<BscBlobTransactionSidecar>())
    }
}

impl BlockBodyTrait for BscBlockBody {
    type Transaction = TransactionSigned;
    type OmmerHeader = Header;

    fn transactions(&self) -> &[Self::Transaction] {
        BlockBodyTrait::transactions(&self.inner)
    }

    fn into_ethereum_body(self) -> BlockBody {
        self.inner
    }

    fn into_transactions(self) -> Vec<Self::Transaction> {
        self.inner.into_transactions()
    }

    fn withdrawals(&self) -> Option<&alloy_rpc_types::Withdrawals> {
        self.inner.withdrawals()
    }

    fn ommers(&self) -> Option<&[Self::OmmerHeader]> {
        self.inner.ommers()
    }
}

/// Block for BSC
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BscBlock {
    pub header: Header,
    pub body: BscBlockBody,
}

impl InMemorySize for BscBlock {
    fn size(&self) -> usize {
        self.header.size() + self.body.size()
    }
}

impl Block for BscBlock {
    type Header = Header;
    type Body = BscBlockBody;

    fn new(header: Self::Header, body: Self::Body) -> Self {
        Self { header, body }
    }

    fn header(&self) -> &Self::Header {
        &self.header
    }

    fn body(&self) -> &Self::Body {
        &self.body
    }

    fn split(self) -> (Self::Header, Self::Body) {
        (self.header, self.body)
    }

    fn rlp_length(header: &Self::Header, body: &Self::Body) -> usize {
        rlp::BlockHelper {
            header: Cow::Borrowed(header),
            transactions: Cow::Borrowed(&body.inner.transactions),
            ommers: Cow::Borrowed(&body.inner.ommers),
            withdrawals: body.inner.withdrawals.as_ref().map(Cow::Borrowed),
            sidecars: body.sidecars.as_ref().map(Cow::Borrowed),
        }
        .length()
    }
}

mod rlp {
    use super::*;
    use alloy_eips::eip4895::Withdrawals;
    use alloy_rlp::Decodable;

    #[derive(RlpEncodable, RlpDecodable)]
    #[rlp(trailing)]
    struct BlockBodyHelper<'a> {
        transactions: Cow<'a, Vec<TransactionSigned>>,
        ommers: Cow<'a, Vec<Header>>,
        withdrawals: Option<Cow<'a, Withdrawals>>,
        sidecars: Option<Cow<'a, Vec<BscBlobTransactionSidecar>>>,
    }

    #[derive(RlpEncodable, RlpDecodable)]
    #[rlp(trailing)]
    pub(crate) struct BlockHelper<'a> {
        pub(crate) header: Cow<'a, Header>,
        pub(crate) transactions: Cow<'a, Vec<TransactionSigned>>,
        pub(crate) ommers: Cow<'a, Vec<Header>>,
        pub(crate) withdrawals: Option<Cow<'a, Withdrawals>>,
        pub(crate) sidecars: Option<Cow<'a, Vec<BscBlobTransactionSidecar>>>,
    }

    impl<'a> From<&'a BscBlockBody> for BlockBodyHelper<'a> {
        fn from(value: &'a BscBlockBody) -> Self {
            let BscBlockBody { inner: BlockBody { transactions, ommers, withdrawals }, sidecars } =
                value;

            Self {
                transactions: Cow::Borrowed(transactions),
                ommers: Cow::Borrowed(ommers),
                withdrawals: withdrawals.as_ref().map(Cow::Borrowed),
                sidecars: sidecars.as_ref().map(Cow::Borrowed),
            }
        }
    }

    impl<'a> From<&'a BscBlock> for BlockHelper<'a> {
        fn from(value: &'a BscBlock) -> Self {
            let BscBlock {
                header,
                body:
                    BscBlockBody { inner: BlockBody { transactions, ommers, withdrawals }, sidecars },
            } = value;

            Self {
                header: Cow::Borrowed(header),
                transactions: Cow::Borrowed(transactions),
                ommers: Cow::Borrowed(ommers),
                withdrawals: withdrawals.as_ref().map(Cow::Borrowed),
                sidecars: sidecars.as_ref().map(Cow::Borrowed),
            }
        }
    }

    impl Encodable for BscBlockBody {
        fn encode(&self, out: &mut dyn bytes::BufMut) {
            BlockBodyHelper::from(self).encode(out);
        }

        fn length(&self) -> usize {
            BlockBodyHelper::from(self).length()
        }
    }

    impl Decodable for BscBlockBody {
        fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
            debug!(
                target: "bsc::primitives::rlp_decode",
                buffer_length=?buf.len(),
                "Decoding BscBlockBody from RLP"
            );
            
            let result = BlockBodyHelper::decode(buf);
            match &result {
                Ok(helper) => {
                    debug!(
                        target: "bsc::primitives::rlp_decode",
                        transactions_count=?helper.transactions.len(),
                        ommers_count=?helper.ommers.len(),
                        has_withdrawals=?helper.withdrawals.is_some(),
                        has_sidecars=?helper.sidecars.is_some(),
                        "Successfully decoded BscBlockBody components"
                    );
                }
                Err(e) => {
                    error!(
                        target: "bsc::primitives::rlp_decode",
                        error=?e,
                        buffer_length=?buf.len(),
                        "Failed to decode BscBlockBody from RLP"
                    );
                }
            }
            
            result.map(|BlockBodyHelper { transactions, ommers, withdrawals, sidecars }| {
                Self {
                    inner: BlockBody {
                        transactions: transactions.into_owned(),
                        ommers: ommers.into_owned(),
                        withdrawals: withdrawals.map(|w| w.into_owned()),
                    },
                    sidecars: sidecars.map(|s| s.into_owned()),
                }
            })
        }
    }

    impl Encodable for BscBlock {
        fn encode(&self, out: &mut dyn bytes::BufMut) {
            BlockHelper::from(self).encode(out);
        }

        fn length(&self) -> usize {
            BlockHelper::from(self).length()
        }
    }

    impl Decodable for BscBlock {
        fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
            debug!(
                target: "bsc::primitives::rlp_decode",
                buffer_length=?buf.len(),
                "Decoding BscBlock from RLP"
            );
            
            let result = BlockHelper::decode(buf);
            match &result {
                Ok(helper) => {
                    debug!(
                        target: "bsc::primitives::rlp_decode",
                        header_number=?helper.header.number,
                        header_hash=?helper.header.hash_slow(),
                        transactions_count=?helper.transactions.len(),
                        ommers_count=?helper.ommers.len(),
                        has_withdrawals=?helper.withdrawals.is_some(),
                        has_sidecars=?helper.sidecars.is_some(),
                        "Successfully decoded BscBlock components"
                    );
                }
                Err(e) => {
                    error!(
                        target: "bsc::primitives::rlp_decode",
                        error=?e,
                        buffer_length=?buf.len(),
                        "Failed to decode BscBlock from RLP"
                    );
                }
            }
            
            result.map(|BlockHelper { header, transactions, ommers, withdrawals, sidecars }| {
                Self {
                    header: header.into_owned(),
                    body: BscBlockBody {
                        inner: BlockBody {
                            transactions: transactions.into_owned(),
                            ommers: ommers.into_owned(),
                            withdrawals: withdrawals.map(|w| w.into_owned()),
                        },
                        sidecars: sidecars.map(|s| s.into_owned()),
                    },
                }
            })
        }
    }
}

pub mod serde_bincode_compat {
    use super::*;
    use reth_primitives_traits::serde_bincode_compat::{BincodeReprFor, SerdeBincodeCompat};

    #[derive(Debug, Serialize, Deserialize)]
    pub struct BscBlockBodyBincode<'a> {
        inner: BincodeReprFor<'a, BlockBody>,
        sidecars: Option<Cow<'a, Vec<BscBlobTransactionSidecar>>>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct BscBlockBincode<'a> {
        header: BincodeReprFor<'a, Header>,
        body: BincodeReprFor<'a, BscBlockBody>,
    }

    impl SerdeBincodeCompat for BscBlockBody {
        type BincodeRepr<'a> = BscBlockBodyBincode<'a>;

        fn as_repr(&self) -> Self::BincodeRepr<'_> {
            BscBlockBodyBincode {
                inner: self.inner.as_repr(),
                sidecars: self.sidecars.as_ref().map(Cow::Borrowed),
            }
        }

        fn from_repr(repr: Self::BincodeRepr<'_>) -> Self {
            let BscBlockBodyBincode { inner, sidecars } = repr;
            Self { inner: BlockBody::from_repr(inner), sidecars: sidecars.map(|s| s.into_owned()) }
        }
    }

    impl SerdeBincodeCompat for BscBlock {
        type BincodeRepr<'a> = BscBlockBincode<'a>;

        fn as_repr(&self) -> Self::BincodeRepr<'_> {
            BscBlockBincode { header: self.header.as_repr(), body: self.body.as_repr() }
        }

        fn from_repr(repr: Self::BincodeRepr<'_>) -> Self {
            let BscBlockBincode { header, body } = repr;
            Self { header: Header::from_repr(header), body: BscBlockBody::from_repr(body) }
        }
    }
}
