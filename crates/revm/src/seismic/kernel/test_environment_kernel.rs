use secp256k1::SecretKey;
use tee_service_api::get_sample_secp256k1_sk;

use crate::primitives::{B256, BlockEnv, TxEnv};
use crate::seismic::rng::{env_hash::{hash_block_env, hash_tx_env}, RootRng};

use super::context::Ctx;
use super::kernel_interface::KernelInterface;


pub struct TestEnvKernel {
    rng: RootRng,
    private_key: SecretKey,
    ctx: Option<Ctx>,
}


//Dummy clone
impl Clone for TestEnvKernel {
    fn clone(&self) -> Self {
        Self {
            rng: self.rng.clone(),
            private_key: self.private_key.clone(),
            ctx: self.ctx.clone(),
        }
    }
}

impl TestEnvKernel {
    pub fn new() -> Self {
        Self {
            rng: RootRng::new(),
            private_key: get_sample_secp256k1_sk(),
            ctx: None,
        }
    }
}

impl KernelInterface for TestEnvKernel {
    fn get_private_key(&self) -> SecretKey {
        self.private_key
    }
    fn maybe_append_entropy(&mut self) {
        // no-op
    }

    fn get_rng(&mut self) -> &mut RootRng {
        &mut self.rng
    }

    fn build_ctx_from_env(&mut self, tx_env: &TxEnv, block_env: &BlockEnv) {
        let tx_hash = hash_tx_env(tx_env);
        let block_hash = hash_block_env(block_env);
        self.ctx = Some(Ctx {
            transaction_hash: tx_hash,
            previous_block_hash: block_hash,
        })
    }

    fn build_ctx_from_hashes(&mut self, tx_hash: B256, block_hash: B256) {
        // Testnet rarely needs this, but we can do it anyway
        self.ctx = Some(Ctx {
            transaction_hash: tx_hash,
            previous_block_hash: block_hash,
        })
    }
    
    fn ctx_is_empty(&self) -> bool {
        !self.ctx.is_some() 
    }
}
