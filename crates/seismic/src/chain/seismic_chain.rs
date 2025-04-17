use revm::{precompile::PrecompileError, primitives::{Bytes, B256}};

use crate::transaction::abstraction::RngMode;

use super::rng_container::RngContainer;

#[derive(Clone, Debug)]
pub struct SeismicChain {
    rng_container: RngContainer,
}

impl Default for SeismicChain {
    fn default() -> Self {
        Self {
            rng_container: RngContainer::default(),
        }
    }
}

impl SeismicChain {
    pub fn new(root_vrf_key: schnorrkel::Keypair) -> Self {
        Self {
            rng_container: RngContainer::new(root_vrf_key),
        }
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
        self.rng_container.calculate_gas_cost(pers, requested_output_len)
    }

    pub fn process_rng(
        &mut self,
        pers: &[u8],
        requested_output_len: usize,
        kernel_mode: RngMode,
        tx_hash: &B256,
    ) -> Result<Bytes, PrecompileError> {
        self.rng_container.process_rng(pers, requested_output_len, kernel_mode, tx_hash)
    }
}
