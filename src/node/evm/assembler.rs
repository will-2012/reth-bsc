use crate::{
    node::evm::config::{BscBlockExecutorFactory, BscEvmConfig},
    BscBlock, BscBlockBody,
};
use alloy_consensus::{Block, Header};
use reth_evm::{
    block::BlockExecutionError,
    execute::{BlockAssembler, BlockAssemblerInput},
};

impl BlockAssembler<BscBlockExecutorFactory> for BscEvmConfig {
    type Block = BscBlock;

    fn assemble_block(
        &self,
        input: BlockAssemblerInput<'_, '_, BscBlockExecutorFactory, Header>,
    ) -> Result<Self::Block, BlockExecutionError> {
        let Block { header, body: inner } = self.block_assembler.assemble_block(input)?;
        Ok(BscBlock {
            header,
            body: BscBlockBody {
                inner,
                // HACK: we're setting sidecars to `None` here but ideally we should somehow get
                // them from the payload builder.
                //
                // Payload building is out of scope of reth-bsc for now, so this is not critical
                sidecars: None,
            },
        })
    }
}
