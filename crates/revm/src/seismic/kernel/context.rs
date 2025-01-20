use crate::{
    primitives::{Env, B256},
    seismic::rng::env_hash::{hash_block_env, hash_tx_env},
};

#[derive(Debug, Clone, Default, Copy)]
pub struct Ctx {
    pub transaction_hash: B256,
    pub previous_block_hash: B256,
}

impl Ctx {
    pub fn new_from_hashes(tx_hash: B256, block_hash: B256) -> Self {
        Self {
            transaction_hash: tx_hash,
            previous_block_hash: block_hash,
        }
    }

    pub fn new_from_env(env: &Env) -> Self {
        let tx_hash = hash_tx_env(&env.tx);
        let block_hash = hash_block_env(&env.block);
        Self {
            transaction_hash: tx_hash,
            previous_block_hash: block_hash,
        }
    }
}