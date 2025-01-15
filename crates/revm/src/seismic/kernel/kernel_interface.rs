use core::fmt::Debug;

use dyn_clone::DynClone;
use secp256k1::SecretKey;

use crate::{primitives::{B256, Env}, seismic::rng::{env_hash::{hash_block_env, hash_tx_env}, RootRng}};

use super::context::Ctx;

pub trait KernelInterface: KernelRng + KernelKeys + KernelContextBuilder + DynClone + Debug {}
impl<T: KernelRng + KernelKeys + KernelContextBuilder + DynClone + Debug> KernelInterface for T {}


pub trait KernelRng {
    fn rng_mut_ref(&mut self) -> &mut RootRng;
    fn maybe_append_entropy(&mut self);
}

pub trait KernelKeys {
    fn get_secret_key(&self) -> SecretKey;
}

pub trait KernelContextBuilder {
    fn ctx_mut(&mut self) -> &mut Option<Ctx>;
    fn ctx_ref(&self) -> &Option<Ctx>;
    fn build_ctx_from_env(&mut self, env: &Env) {
        *self.ctx_mut() = Some(Ctx::new_from_env(env));
    }
    fn build_ctx_from_raw(&mut self, tx_hash: B256, block_hash: B256) {
        *self.ctx_mut() = Some(Ctx::new_from_hashes(tx_hash, block_hash));
    }
    fn ctx_is_empty(&self) -> bool {
        self.ctx_ref().is_none()
    }
}

