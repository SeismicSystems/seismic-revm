use dyn_clone::DynClone;
use secp256k1::SecretKey;

use crate::{primitives::{BlockEnv, TxEnv, B256}, seismic::rng::RootRng};

pub trait KernelInterface: DynClone {
    /// === Kernel Functionalities ===
    /// Return the private key (dummy for testnet, real for mainnet, etc.).
    fn get_private_key(&self) -> SecretKey;

    /// Append entropy if needed. (E.g. only for mainnet-simulation.)
    fn maybe_append_entropy(&mut self);

    /// Whether this kernel is in "simulation" mode.
    fn is_sim(&self) -> bool;

    /// === RNG Functionalities ===
    fn get_rng(&mut self) -> &mut RootRng;

    // ===  Ctx functionalities ===
    // Build Ctx with different entrypoints for test environment and mainnet.
    fn build_ctx_from_env(&mut self, tx_env: &TxEnv, block_env: &BlockEnv);
    fn build_ctx_from_hashes(&mut self, tx_hash: B256, block_hash: B256);
    fn ctx_is_empty(&self) -> bool;
}

dyn_clone::clone_trait_object!(KernelInterface);

