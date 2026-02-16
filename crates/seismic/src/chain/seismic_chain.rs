use revm::{
    precompile::PrecompileError,
    primitives::{Bytes, B256},
};

use super::rng_container::{derive_rng_output, rng_gas_cost};

#[derive(Clone, Debug, Default)]
pub struct SeismicChain {
    live_rng_key: Option<schnorrkel::Keypair>,
    /// Total remaining gas across all active call frames, set before precompile dispatch.
    gas_remaining_all_frames: u64,
}

impl SeismicChain {
    pub fn new(root_vrf_key: schnorrkel::Keypair) -> Self {
        Self {
            live_rng_key: Some(root_vrf_key),
            gas_remaining_all_frames: 0,
        }
    }

    pub fn with_live_rng_key(live_rng_key: Option<schnorrkel::Keypair>) -> Self {
        Self {
            live_rng_key,
            gas_remaining_all_frames: 0,
        }
    }

    pub fn set_rng_key(&mut self, root_vrf_key: schnorrkel::Keypair) {
        self.live_rng_key = Some(root_vrf_key);
    }

    pub fn gas_remaining_all_frames(&self) -> u64 {
        self.gas_remaining_all_frames
    }

    pub fn set_gas_remaining_all_frames(&mut self, gas: u64) {
        self.gas_remaining_all_frames = gas;
    }

    pub fn calculate_gas_cost(&self, requested_output_len: usize) -> u64 {
        rng_gas_cost(requested_output_len)
    }

    pub fn process_rng(
        &self,
        pers: &[u8],
        requested_output_len: usize,
        tx_hash: &B256,
        total_gas_remaining: u64,
    ) -> Result<Bytes, PrecompileError> {
        derive_rng_output(
            pers,
            requested_output_len,
            tx_hash,
            self.live_rng_key.clone(),
            total_gas_remaining,
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
        let chain = SeismicChain::new(keypair);

        let tx_hash = B256::from([1u8; 32]);
        let pers = b"test_pers";

        let output1 = chain.process_rng(pers, 32, &tx_hash, 1000).unwrap();
        let output2 = chain.process_rng(pers, 32, &tx_hash, 1000).unwrap();

        assert_eq!(
            output1, output2,
            "execution mode should produce identical output for same tx_hash and pers"
        );
    }

    #[test]
    fn test_execution_mode_different_pers_different_output() {
        let keypair = get_unsecure_sample_schnorrkel_keypair();
        let chain = SeismicChain::new(keypair);

        let tx_hash = B256::from([1u8; 32]);

        let output1 = chain.process_rng(b"pers_a", 32, &tx_hash, 1000).unwrap();
        let output2 = chain.process_rng(b"pers_b", 32, &tx_hash, 1000).unwrap();

        assert_ne!(
            output1, output2,
            "execution mode should produce different output for different pers"
        );
    }

    #[test]
    fn test_execution_mode_different_tx_hash_different_output() {
        let keypair = get_unsecure_sample_schnorrkel_keypair();
        let chain = SeismicChain::new(keypair);

        let pers = b"test_pers";

        let output1 = chain
            .process_rng(pers, 32, &B256::from([1u8; 32]), 1000)
            .unwrap();
        let output2 = chain
            .process_rng(pers, 32, &B256::from([2u8; 32]), 1000)
            .unwrap();

        assert_ne!(
            output1, output2,
            "execution mode should produce different output for different tx_hash"
        );
    }

    #[test]
    fn test_execution_mode_deterministic_across_chains() {
        let keypair = get_unsecure_sample_schnorrkel_keypair();
        let chain1 = SeismicChain::new(keypair.clone());
        let chain2 = SeismicChain::new(keypair);

        let tx_hash = B256::from([1u8; 32]);
        let pers = b"test_pers";

        let output1 = chain1.process_rng(pers, 32, &tx_hash, 1000).unwrap();
        let output2 = chain2.process_rng(pers, 32, &tx_hash, 1000).unwrap();

        assert_eq!(
            output1, output2,
            "execution mode should be deterministic across separate chains with same key"
        );
    }

    #[test]
    fn test_simulation_mode_non_deterministic() {
        // No live key = simulation mode (random key per call)
        let chain = SeismicChain::default();
        let tx_hash = B256::from([1u8; 32]);
        let pers = b"test_pers";

        let output1 = chain.process_rng(pers, 32, &tx_hash, 1000).unwrap();
        let output2 = chain.process_rng(pers, 32, &tx_hash, 1000).unwrap();

        assert_ne!(
            output1, output2,
            "simulation mode should produce different output each call"
        );
    }
}
