use rand::RngCore;
use revm::{
    precompile::PrecompileError,
    primitives::{keccak256, Bytes, B256},
};

use super::rng_container::derive_rng_output;

#[derive(Clone)]
pub struct SeismicChain {
    /// HKDF input key material for the RNG precompile (64 bytes).
    live_rng_key: [u8; 64],
    /// Parent block hash for RNG domain separation. Set once at block start.
    parent_block_hash: B256,
    /// Running hash of prior transaction hashes in the current block.
    /// Advanced after each committed transaction via `advance_tx_accumulator`.
    tx_hash_accumulator: B256,
    /// Total remaining gas across all active call frames, set before precompile dispatch.
    gas_remaining_all_frames: u64,
    /// If the current transaction used erc20 as gas
    used_erc20_gas: bool,
}

/// Redacted: `live_rng_key` seeds every RNG-precompile output.
impl core::fmt::Debug for SeismicChain {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SeismicChain")
            .field("parent_block_hash", &self.parent_block_hash)
            .field("tx_hash_accumulator", &self.tx_hash_accumulator)
            .field("gas_remaining_all_frames", &self.gas_remaining_all_frames)
            .field("used_erc20_gas", &self.used_erc20_gas)
            .finish_non_exhaustive()
    }
}

impl SeismicChain {
    pub fn new(rng_ikm: [u8; 64]) -> Self {
        Self {
            live_rng_key: rng_ikm,
            parent_block_hash: B256::ZERO,
            tx_hash_accumulator: B256::ZERO,
            gas_remaining_all_frames: 0,
            used_erc20_gas: false,
        }
    }

    /// A chain seeded with a fresh random rng key, for execution contexts that
    /// run without provisioned network keys (sforge, sanvil, state tests).
    /// Consensus nodes inject their network's key instead — RNG-precompile
    /// outputs are consensus-visible.
    pub fn with_random_rng_key() -> Self {
        let mut random_ikm = [0u8; 64];
        rand::rng().fill_bytes(&mut random_ikm);
        Self {
            live_rng_key: random_ikm,
            parent_block_hash: B256::ZERO,
            tx_hash_accumulator: B256::ZERO,
            gas_remaining_all_frames: 0,
            used_erc20_gas: false,
        }
    }

    pub fn with_live_rng_key(live_rng_key: [u8; 64]) -> Self {
        Self {
            live_rng_key,
            parent_block_hash: B256::ZERO,
            tx_hash_accumulator: B256::ZERO,
            gas_remaining_all_frames: 0,
            used_erc20_gas: false,
        }
    }

    pub fn set_rng_key(&mut self, rng_ikm: [u8; 64]) {
        self.live_rng_key = rng_ikm;
    }

    pub fn gas_remaining_all_frames(&self) -> u64 {
        self.gas_remaining_all_frames
    }

    pub fn set_gas_remaining_all_frames(&mut self, gas: u64) {
        self.gas_remaining_all_frames = gas;
    }

    pub fn parent_block_hash(&self) -> &B256 {
        &self.parent_block_hash
    }

    pub fn set_parent_block_hash(&mut self, hash: B256) {
        self.parent_block_hash = hash;
    }

    pub fn tx_hash_accumulator(&self) -> &B256 {
        &self.tx_hash_accumulator
    }

    pub fn set_tx_hash_accumulator(&mut self, acc: B256) {
        self.tx_hash_accumulator = acc;
    }

    pub fn set_used_erc20_gas(&mut self) {
        self.used_erc20_gas = true;
    }

    pub fn used_erc20_gas(&self) -> bool {
        self.used_erc20_gas
    }

    pub fn reset_erc20_gas(&mut self) {
        self.used_erc20_gas = false;
    }

    /// Advance the tx hash accumulator by hashing in the given tx_hash.
    /// Called after each committed transaction in a block.
    pub fn advance_tx_accumulator(&mut self, tx_hash: &B256) {
        let mut data = [0u8; 64];
        data[..32].copy_from_slice(self.tx_hash_accumulator.as_ref());
        data[32..].copy_from_slice(tx_hash.as_ref());
        self.tx_hash_accumulator = keccak256(data);
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
            self.live_rng_key,
            &self.parent_block_hash,
            &self.tx_hash_accumulator,
            total_gas_remaining,
        )
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use seismic_crypto::well_known_rng_ikm;

    #[test]
    fn test_execution_mode_same_inputs_same_output() {
        let ikm = well_known_rng_ikm();
        let chain = SeismicChain::new(ikm);

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
        let ikm = well_known_rng_ikm();
        let chain = SeismicChain::new(ikm);

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
        let ikm = well_known_rng_ikm();
        let chain = SeismicChain::new(ikm);

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
        let ikm = well_known_rng_ikm();
        let chain1 = SeismicChain::new(ikm);
        let chain2 = SeismicChain::new(ikm);

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
    fn test_different_parent_block_hash_different_output() {
        let ikm = well_known_rng_ikm();
        let mut chain1 = SeismicChain::new(ikm);
        let mut chain2 = SeismicChain::new(ikm);

        chain1.set_parent_block_hash(B256::from([1u8; 32]));
        chain2.set_parent_block_hash(B256::from([2u8; 32]));

        let tx_hash = B256::from([0xAA; 32]);
        let pers = b"test_pers";

        let output1 = chain1.process_rng(pers, 32, &tx_hash, 1000).unwrap();
        let output2 = chain2.process_rng(pers, 32, &tx_hash, 1000).unwrap();

        assert_ne!(
            output1, output2,
            "different parent_block_hash should produce different RNG output"
        );
    }

    #[test]
    fn test_different_tx_hash_accumulator_different_output() {
        let ikm = well_known_rng_ikm();
        let mut chain1 = SeismicChain::new(ikm);
        let mut chain2 = SeismicChain::new(ikm);

        chain1.set_tx_hash_accumulator(B256::from([1u8; 32]));
        chain2.set_tx_hash_accumulator(B256::from([2u8; 32]));

        let tx_hash = B256::from([0xAA; 32]);
        let pers = b"test_pers";

        let output1 = chain1.process_rng(pers, 32, &tx_hash, 1000).unwrap();
        let output2 = chain2.process_rng(pers, 32, &tx_hash, 1000).unwrap();

        assert_ne!(
            output1, output2,
            "different tx_hash_accumulator should produce different RNG output"
        );
    }

    #[test]
    fn test_advance_tx_accumulator() {
        let ikm = well_known_rng_ikm();
        let mut chain = SeismicChain::new(ikm);

        assert_eq!(*chain.tx_hash_accumulator(), B256::ZERO);

        let tx1 = B256::from([1u8; 32]);
        chain.advance_tx_accumulator(&tx1);
        let acc_after_tx1 = *chain.tx_hash_accumulator();
        assert_ne!(acc_after_tx1, B256::ZERO);

        // Advancing with same tx again should produce different accumulator
        chain.advance_tx_accumulator(&tx1);
        let acc_after_tx1_twice = *chain.tx_hash_accumulator();
        assert_ne!(acc_after_tx1, acc_after_tx1_twice);

        // Verify accumulator = keccak(prev || tx_hash)
        let mut expected_data = [0u8; 64];
        expected_data[..32].copy_from_slice(acc_after_tx1.as_ref());
        expected_data[32..].copy_from_slice(tx1.as_ref());
        assert_eq!(acc_after_tx1_twice, keccak256(expected_data));
    }

    #[test]
    fn test_accumulator_affects_rng_output() {
        let ikm = well_known_rng_ikm();
        let mut chain = SeismicChain::new(ikm);
        chain.set_parent_block_hash(B256::from([0xFF; 32]));

        let tx_hash = B256::from([0xAA; 32]);
        let pers = b"test_pers";

        // RNG output before any accumulator advance
        let output_before = chain.process_rng(pers, 32, &tx_hash, 1000).unwrap();

        // Advance accumulator (simulating a prior tx in the block)
        chain.advance_tx_accumulator(&B256::from([0xBB; 32]));

        // RNG output after accumulator advance — should differ
        let output_after = chain.process_rng(pers, 32, &tx_hash, 1000).unwrap();

        assert_ne!(
            output_before, output_after,
            "RNG output should differ after tx_hash_accumulator is advanced"
        );
    }
}
