//! # Module Overview
//! This module provides functionality a RNG precompile and related utilities.
//!
//! ## Submodules
//! - `domain_sep_rng`: Implements a domain-separated random number generator.
//! - `test`: Contains test cases for the RNG logic, e.g. domain separation, cloning, etc

mod domain_sep_rng;
pub use domain_sep_rng::{LeafRng, RootRng, SchnorrkelKeypair};
use tee_service_api::get_sample_schnorrkel_keypair;

use super::kernel::kernel_interface::KernelRng;

#[cfg(test)]
mod test;

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

impl KernelRng for RngContainer {
    fn root_rng_mut_ref(&mut self) -> &mut RootRng {
        &mut self.rng
    }

    fn root_rng_ref(&self) -> &RootRng {
        &self.rng
    }

    fn leaf_rng_mut_ref(&mut self) -> &mut Option<LeafRng> {
        &mut self.leaf_rng
    }

    fn reset_rng(&mut self, root_vrf_key: SchnorrkelKeypair) {
        self.rng = RootRng::new(root_vrf_key);
        self.leaf_rng = None;
    }

    fn maybe_append_entropy(&mut self) {
        // noop
    }
}
