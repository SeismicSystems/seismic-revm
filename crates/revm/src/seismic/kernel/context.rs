use crate::primitives::B256;

#[derive(Debug, Clone, Default)]
pub struct Ctx {
    pub transaction_hash: B256,
    pub previous_block_hash: B256,
}

