use core::fmt::Debug;

use dyn_clone::DynClone;
use schnorrkel::keys::Keypair as SchnorrkelKeypair;

use crate::{
    primitives::{Env, B256},
    seismic::rng::{LeafRng, RootRng},
};

use super::context::Ctx;

pub trait KernelInterface: KernelRng + KernelKeys + KernelContext + DynClone + Debug {}
impl<T: KernelRng + KernelKeys + KernelContext + DynClone + Debug> KernelInterface for T {}

pub trait KernelRng {
    // returns the root rng for the entire block
    fn root_rng_mut_ref(&mut self) -> &mut RootRng;

    // returns a LeafRng Option for the current transaction, None if not initialized
    fn leaf_rng_mut_ref(&mut self) -> &mut Option<LeafRng>;

    // maybe appends entropy to the root rng
    fn maybe_append_entropy(&mut self);
}

pub trait KernelKeys {
    // returns the key for decrypting transaction data
    fn get_io_key(&self) -> secp256k1::SecretKey;

    // returns the vrf key for rng transcripts
    fn get_eph_rng_keypair(&self) -> SchnorrkelKeypair;
}

pub trait KernelContext {
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
