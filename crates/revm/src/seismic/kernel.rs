use std::fmt;

use crate::primitives::{Env, B256};

use crate::seismic::rng::RootRng;

use super::rng::env_hash::{hash_block_env, hash_tx_env};

/// A simple wrapper around RootRng, with more to come in the future
pub struct Kernel {
    pub root_rng: RootRng,
    ctx: Ctx,
}

//Dummy debug for now, so that we can pass it into the innerEvmContext
impl fmt::Display for Kernel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Kernel {{ rng }}")
    }
}

impl fmt::Debug for Kernel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Kernel {{ rng }}")
    }
}

impl Clone for Kernel {
    fn clone(&self) -> Self {
        Self {
            root_rng: self.root_rng.clone(),
            ctx: self.ctx.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Ctx {
    transaction_hash: B256,
    previous_block_hash: B256,
}

impl Ctx {
    fn new(env: &Env) -> Self {
        Self {
            //Note, we can access block hash from database implementation
            transaction_hash: hash_tx_env(&env.tx),
            previous_block_hash: hash_block_env(&env.block),
        }
    }

    fn is_empty(&self) -> bool {
        self.transaction_hash.is_empty() || self.previous_block_hash.is_empty()
    }
}

impl Kernel {
    /// Create a new root RNG.
    pub fn new(env: &Env) -> Self {
        Self {
            root_rng: RootRng::new(),
            ctx: Ctx::new(env),
        }
    }

    pub fn default() -> Self {
        Self {
            root_rng: RootRng::new(),
            ctx: Ctx::default(),
        }
    }

    pub fn ctx_is_empty(&self) -> bool {
        self.ctx.is_empty()
    }
}
