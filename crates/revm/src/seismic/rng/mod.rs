//! # Module Overview
//! This module provides functionality a RNG precompile and related utilities.
//!
//! ## Submodules
//! - `domain_sep_rng`: Implements a domain-separated random number generator.
//! - `test`: Contains test cases for the RNG logic, e.g. domain separation, cloning, etc

mod domain_sep_rng;
use core::fmt;

use crate::primitives::RngMode;
pub use domain_sep_rng::{LeafRng, RootRng, SchnorrkelKeypair};
use tee_service_api::get_sample_schnorrkel_keypair;

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
    pub fn root_rng_mut_ref(&mut self) -> &mut RootRng {
        &mut self.rng
    }

    pub fn root_rng_ref(&self) -> &RootRng {
        &self.rng
    }

    pub fn leaf_rng_mut_ref(&mut self) -> &mut Option<LeafRng> {
        &mut self.leaf_rng
    }

    pub fn get_root_vrf_key(&self) -> SchnorrkelKeypair {
        self.root_rng_ref().get_root_vrf_key()
    }

    pub fn reset_rng(&mut self) {
        let root_vrf_key = self.root_rng_ref().get_root_vrf_key();
        self.rng = RootRng::new(root_vrf_key);
        self.leaf_rng = None;
    }

    /// Appends entropy to the root RNG if in Simulation mode.
    pub fn maybe_append_entropy(&mut self, mode: RngMode) {
        if mode == RngMode::Simulation {
            self.root_rng_mut_ref().append_local_entropy();
        }
    }
}
