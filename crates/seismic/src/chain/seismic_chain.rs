use revm::{precompile::PrecompileError, primitives::{Bytes, B256}};

use crate::{transaction::abstraction::RngMode, SeismicHaltReason};

use super::rng_container::RngContainer;

#[derive(Clone, Debug)]
pub struct SeismicChain {
    rng_container: RngContainer,
    halt_reason: Option<SeismicHaltReason>,
}

impl Default for SeismicChain {
    fn default() -> Self {
        Self {
            rng_container: RngContainer::default(),
            halt_reason: None,
        }
    }
}

impl SeismicChain {
    pub fn new(root_vrf_key: schnorrkel::Keypair) -> Self {
        Self {
            rng_container: RngContainer::new(root_vrf_key),
            halt_reason: None,
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
    
    pub fn set_halt_reason(&mut self, reason: SeismicHaltReason) {
        self.halt_reason = Some(reason);
    }
    
    pub fn has_halt_reason(&self) -> bool {
        self.halt_reason.is_some()
    }
    
    pub fn take_halt_reason(&mut self) -> Option<SeismicHaltReason> {
        self.halt_reason.take()
    }
    
    pub fn clear_halt_reason(&mut self) {
        self.halt_reason = None;
    }
}
