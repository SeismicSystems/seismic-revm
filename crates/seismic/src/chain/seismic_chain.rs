use revm::{
    precompile::PrecompileError,
    primitives::{Bytes, B256},
};

use crate::transaction::abstraction::RngMode;

use super::rng_container::RngContainer;

#[derive(Clone, Debug)]
pub struct SeismicChain {
    rng_container: RngContainer,
    live_rng_key: Option<schnorrkel::Keypair>,
}

impl Default for SeismicChain {
    fn default() -> Self {
        Self {
            rng_container: RngContainer::default(),
            live_rng_key: None,
        }
    }
}

impl SeismicChain {
    pub fn new(root_vrf_key: schnorrkel::Keypair) -> Self {
        Self {
            rng_container: RngContainer::new(root_vrf_key.clone()),
            live_rng_key: Some(root_vrf_key),
        }
    }

    pub fn with_live_rng_key(live_rng_key: Option<schnorrkel::Keypair>) -> Self {
        Self {
            rng_container: RngContainer::default(),
            live_rng_key,
        }
    }

    pub fn set_rng_key(&mut self, root_vrf_key: schnorrkel::Keypair) {
        self.rng_container = RngContainer::new(root_vrf_key);
    }

    pub fn rng_container(&self) -> &RngContainer {
        &self.rng_container
    }

    pub fn rng_container_mut(&mut self) -> &mut RngContainer {
        &mut self.rng_container
    }

    pub fn reset_rng(&mut self) {
        self.rng_container.reset_rng();
    }

    pub fn maybe_append_entropy(&mut self, mode: RngMode) {
        self.rng_container.maybe_append_entropy(mode);
    }

    pub fn calculate_gas_cost(&self, pers: &[u8], requested_output_len: usize) -> u64 {
        self.rng_container
            .calculate_gas_cost(pers, requested_output_len)
    }

    pub fn process_rng(
        &mut self,
        pers: &[u8],
        requested_output_len: usize,
        kernel_mode: RngMode,
        tx_hash: &B256,
    ) -> Result<Bytes, PrecompileError> {
        // Check if we should use live key for Execute mode
        let rng_key = match (&kernel_mode, &self.live_rng_key) {
            (RngMode::Execution, Some(live_key)) => Some(live_key.clone()),
            _ => None,
        };
        
        self.rng_container
            .process_rng_with_key(pers, requested_output_len, kernel_mode, tx_hash, rng_key)
    }

}
