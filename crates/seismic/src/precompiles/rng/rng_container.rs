use core::fmt;
use rand_core::RngCore;
use revm::primitives::{B256, Bytes};

use crate::transaction::abstraction::RngMode;
use seismic_enclave::get_sample_schnorrkel_keypair;

use super::precompile::{calculate_fill_cost, calculate_init_cost};

pub struct RngContainer {
    rng: RootRng,
    leaf_rng: Option<LeafRng>,
}

impl Clone for RngContainer {
    fn clone(&self) -> Self {
        Self {
            rng: self.rng.clone(),
            leaf_rng: None,
        }
    }
}

impl fmt::Debug for RngContainer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Hide internal details of the RNG container.
        write!(f, "Kernel {{  }}")
    }
}

impl Default for RngContainer {
    fn default() -> Self {
        Self {
            rng: RootRng::new(get_sample_schnorrkel_keypair()),
            leaf_rng: None,
        }
    }
}

impl RngContainer {
    pub fn new(root_vrf_key: SchnorrkelKeypair) -> Self {
        Self {
            rng: RootRng::new(root_vrf_key),
            leaf_rng: None,
        }
    }
}

impl RngContainer {
    pub fn reset_rng(&mut self) {
        let root_vrf_key = self.rng.get_root_vrf_key();
        self.rng = RootRng::new(root_vrf_key);
        self.leaf_rng = None;
    }

    /// Appends entropy to the root RNG if in Simulation mode.
    pub fn maybe_append_entropy(&mut self, mode: RngMode) {
        if mode == RngMode::Simulation {
            self.rng.append_local_entropy();
        }
    }

    pub fn calculate_gas_cost(&self, pers: &[u8], requested_output_len: usize) -> u64 {
        match self.leaf_rng.as_ref() {
            Some(_) => calculate_fill_cost(requested_output_len),
            None => calculate_init_cost(pers.len()) + calculate_fill_cost(requested_output_len),
        }
    }

    pub fn process_rng(
        &mut self,
        pers: &[u8],
        requested_output_len: usize,
        kernel_mode: RngMode,
        tx_hash: &B256,
    ) -> Result<Bytes, PrecompileError> {
        // Domain separation: update the root RNG
        self.maybe_append_entropy(kernel_mode);
        self.rng.append_tx(tx_hash);

        // Initialize the leaf RNG if not done already.
        if self.leaf_rng.is_none() {
            let leaf_rng = self.rng.fork(pers);
            self.leaf_rng = Some(leaf_rng);
        }

        // Get the random bytes.
        let leaf_rng = self.leaf_rng.as_mut().unwrap();
        let mut rng_bytes = vec![0u8; requested_output_len];
        leaf_rng.fill_bytes(&mut rng_bytes);
        Ok(Bytes::from(rng_bytes))
    }
}
