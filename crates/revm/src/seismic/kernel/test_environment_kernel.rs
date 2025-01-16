use crate::primitives::Env;
use core::fmt;
use secp256k1::SecretKey;
use tee_service_api::get_sample_secp256k1_sk;
use alloy_primitives::B256;

use crate::seismic::rng::RootRng;

use super::context::Ctx;
use super::kernel_interface::{KernelContext, KernelKeys, KernelRng};

pub(crate) struct TestKernel {
    rng: RootRng,
    secret_key: SecretKey,
    block_rng_entropy: B256,
    ctx: Option<Ctx>,
}

impl fmt::Debug for TestKernel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // We can’t easily peek into the trait object, so just say "Kernel { ... }"
        write!(f, "Kernel {{ ... }}")
    }
}

impl KernelRng for TestKernel {
    fn rng_mut_ref(&mut self) -> &mut RootRng {
        &mut self.rng
    }

    fn maybe_append_entropy(&mut self) {
        // noop for tests
    }
}

impl KernelKeys for TestKernel {
    fn get_secret_key(&self) -> SecretKey {
        self.secret_key
    }
    fn get_block_rng_entropy(&self) -> revm_precompile::B256 {
        self.block_rng_entropy
    }
}

impl KernelContext for TestKernel {
    fn ctx_mut(&mut self) -> &mut Option<Ctx> {
        &mut self.ctx
    }

    fn ctx_ref(&self) -> &Option<Ctx> {
        &self.ctx
    }
}

//Dummy clone
impl Clone for TestKernel {
    fn clone(&self) -> Self {
        Self {
            rng: self.rng.clone(),
            secret_key: self.secret_key,
            block_rng_entropy: self.block_rng_entropy,
            ctx: self.ctx.clone(),
        }
    }
}

impl TestKernel {
    pub(crate) fn new(env: &Env) -> Self {
        Self {
            rng: RootRng::new(),
            secret_key: get_sample_secp256k1_sk(),
            block_rng_entropy: B256::ZERO,
            ctx: Some(Ctx::new_from_env(env)),
        }
    }

    pub(crate) fn default() -> Self {
        Self {
            rng: RootRng::new(),
            secret_key: get_sample_secp256k1_sk(),
            block_rng_entropy: B256::ZERO,
            ctx: None,
        }
    }
}
