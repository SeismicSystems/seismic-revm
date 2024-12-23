use std::fmt;

use crate::primitives::{B256, BlockEnv, TxEnv, Env};

mod kernel_interface;
use kernel_interface::KernelInterface;
mod context;
mod test_environment_kernel;
use secp256k1::SecretKey;
use test_environment_kernel::TestEnvKernel;

use super::rng::RootRng;

#[derive(Clone)]
pub struct Kernel {
    inner: Box<dyn KernelInterface>,
}

impl fmt::Debug for Kernel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // We can’t easily peek into the trait object, so just say "Kernel { ... }"
        write!(f, "Kernel {{ ... }}")
    }
}

impl Kernel {
    pub fn new(env: &Env) -> Self {
        if env.tx.tx_hash.is_zero() {
            Self {
                inner: Box::new(TestEnvKernel::new())
            }
        }
        //the below should create a mainnet or sim kernel instance
        else {
            Self {
                inner: Box::new(TestEnvKernel::new())
            }
        }
    }

    pub fn default() -> Self {
        Self {
            inner: Box::new(TestEnvKernel::new()),
        }
    }

    pub fn new_testnet() -> Self {
        Self {
            inner: Box::new(TestEnvKernel::new()),
        }
    }
    //todo
    //pub fn new_mainnet(real_key: SecretKey) -> Self {
    //    Self {
    //        inner: Box::new(MainnetKernel::new(real_key)),
    //    }
    //}
    //pub fn new_mainnet_sim(real_key: SecretKey) -> Self {
    //    Self {
    //        inner: Box::new(MainnetSimKernel::new(real_key)),
    //    }
    //}

    // Delegate trait calls
    pub fn get_private_key(&self) -> SecretKey {
        self.inner.get_private_key()
    }
    
    pub fn maybe_append_entropy(&mut self) {
        self.inner.maybe_append_entropy();
    }
    
    pub fn is_sim(&self) -> bool {
        self.inner.is_sim()
    }
    
    pub fn get_rng(&mut self) -> &mut RootRng {
        self.inner.get_rng()
    }

    pub fn build_ctx_from_env(&mut self, tx_env: &TxEnv, block_env: &BlockEnv) {
        self.inner.build_ctx_from_env(tx_env, block_env)
    }
    
    pub fn build_ctx_from_hashes(&mut self, tx_hash: B256, block_hash: B256) {
        self.inner.build_ctx_from_hashes(tx_hash, block_hash)
    }
    
    pub fn ctx_is_empty(&self) -> bool {
        self.inner.ctx_is_empty()
    }
}
