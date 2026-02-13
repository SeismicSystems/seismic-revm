use revm::{
    precompile::PrecompileError,
    primitives::{Bytes, B256},
};

use crate::transaction::abstraction::RngMode;

use super::rng_container::RngContainer;

#[derive(Clone, Debug, Default)]
pub struct SeismicChain {
    rng_container: RngContainer,
    live_rng_key: Option<schnorrkel::Keypair>,
    /// Total remaining gas across all active call frames, set before precompile dispatch.
    gas_remaining_all_frames: u64,
}

impl SeismicChain {
    pub fn new(root_vrf_key: schnorrkel::Keypair) -> Self {
        Self {
            rng_container: RngContainer::new(root_vrf_key.clone()),
            live_rng_key: Some(root_vrf_key),
            gas_remaining_all_frames: 0,
        }
    }

    pub fn with_live_rng_key(live_rng_key: Option<schnorrkel::Keypair>) -> Self {
        Self {
            rng_container: RngContainer::default(),
            live_rng_key,
            gas_remaining_all_frames: 0,
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

    pub fn gas_remaining_all_frames(&self) -> u64 {
        self.gas_remaining_all_frames
    }

    pub fn set_gas_remaining_all_frames(&mut self, gas: u64) {
        self.gas_remaining_all_frames = gas;
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
        total_gas_remaining: u64
    ) -> Result<Bytes, PrecompileError> {
        // Check if we should use live key for Execute mode
        let rng_key = match (&kernel_mode, &self.live_rng_key) {
            (RngMode::Execution, Some(live_key)) => Some(live_key.clone()),
            _ => None,
        };

        self.rng_container.process_rng_with_key(
            pers,
            requested_output_len,
            kernel_mode,
            tx_hash,
            rng_key,
            total_gas_remaining
        )
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use seismic_enclave::get_unsecure_sample_schnorrkel_keypair;

    #[test]
    fn test_execution_mode_same_inputs_same_output() {
        let keypair = get_unsecure_sample_schnorrkel_keypair();
        let mut chain = SeismicChain::new(keypair);

        let tx_hash = B256::from([1u8; 32]);
        let pers = b"test_pers";

        let output1 = chain
            .process_rng(pers, 32, RngMode::Execution, &tx_hash,1000)
            .unwrap();
        let output2 = chain
            .process_rng(pers, 32, RngMode::Execution, &tx_hash,1000)
            .unwrap();

        assert_eq!(
            output1, output2,
            "execution mode should produce identical output for same tx_hash and pers"
        );
    }

    #[test]
    fn test_execution_mode_different_pers_different_output() {
        let keypair = get_unsecure_sample_schnorrkel_keypair();
        let mut chain = SeismicChain::new(keypair);

        let tx_hash = B256::from([1u8; 32]);

        let output1 = chain
            .process_rng(b"pers_a", 32, RngMode::Execution, &tx_hash,1000)
            .unwrap();
        let output2 = chain
            .process_rng(b"pers_b", 32, RngMode::Execution, &tx_hash,1000)
            .unwrap();

        assert_ne!(
            output1, output2,
            "execution mode should produce different output for different pers"
        );
    }

    #[test]
    fn test_execution_mode_different_tx_hash_different_output() {
        let keypair = get_unsecure_sample_schnorrkel_keypair();
        let mut chain = SeismicChain::new(keypair);

        let pers = b"test_pers";

        let output1 = chain
            .process_rng(pers, 32, RngMode::Execution, &B256::from([1u8; 32]),1000)
            .unwrap();
        let output2 = chain
            .process_rng(pers, 32, RngMode::Execution, &B256::from([2u8; 32]), 1000)
            .unwrap();

        assert_ne!(
            output1, output2,
            "execution mode should produce different output for different tx_hash"
        );
    }

    #[test]
    fn test_execution_mode_deterministic_across_chains() {
        let keypair = get_unsecure_sample_schnorrkel_keypair();
        let mut chain1 = SeismicChain::new(keypair.clone());
        let mut chain2 = SeismicChain::new(keypair);

        let tx_hash = B256::from([1u8; 32]);
        let pers = b"test_pers";

        let output1 = chain1
            .process_rng(pers, 32, RngMode::Execution, &tx_hash, 1000)
            .unwrap();
        let output2 = chain2
            .process_rng(pers, 32, RngMode::Execution, &tx_hash, 1000)
            .unwrap();

        assert_eq!(
            output1, output2,
            "execution mode should be deterministic across separate chains with same key"
        );
    }
}
