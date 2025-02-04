// kernel.rs

use schnorrkel::keys::Keypair as SchnorrkelKeypair;
use std::fmt;
use crate::seismic::rng::{RngContainer, RootRng, LeafRng};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Indicates the runtime context for the kernel.
/// Use `Simulation` for endpoints (like eth_call) that need unique entropy,
/// and `Execution` for normal transaction execution (used for both tests and production).
pub enum KernelMode {
    Simulation,
    Execution,
}

/// A unified, concrete kernel implementation.
/// It holds an RNG container and a mode flag.
#[derive(Clone)]
pub struct Kernel {
    rng_container: RngContainer,
    mode: KernelMode,
}

impl Kernel {
    /// Creates a new kernel from a given VRF key and mode.
    pub fn new(root_vrf_key: SchnorrkelKeypair, mode: KernelMode) -> Self {
        Self {
            rng_container: RngContainer::new(root_vrf_key),
            mode,
        }
    }

    /// Convenience constructor for simulation mode.
    pub fn new_simulation(root_vrf_key: SchnorrkelKeypair) -> Self {
        Self::new(root_vrf_key, KernelMode::Simulation)
    }

    /// Convenience constructor for production mode.
    pub fn new_production(root_vrf_key: SchnorrkelKeypair) -> Self {
        Self::new(root_vrf_key, KernelMode::Execution)
    }
    
    /// Convenience constructor for test mode.
    pub fn new_test(root_vrf_key: SchnorrkelKeypair) -> Self {
        Self::new(root_vrf_key, KernelMode::Execution)
    }

    /// Resets the RNG while preserving the root VRF key.
    pub fn reset_rng(&mut self) {
        self.rng_container.reset_rng();
    }

    /// Returns a reference to the root RNG.
    pub fn root_rng_ref(&self) -> &RootRng {
        self.rng_container.root_rng_ref()
    }

    /// Returns a mutable reference to the root RNG.
    pub fn root_rng_mut_ref(&mut self) -> &mut RootRng {
        self.rng_container.root_rng_mut_ref()
    }

    /// Returns a mutable reference to the optional leaf RNG.
    pub fn leaf_rng_mut_ref(&mut self) -> &mut Option<LeafRng> {
        self.rng_container.leaf_rng_mut_ref()
    }

    /// Appends entropy to the root RNG if in Simulation mode.
    pub fn maybe_append_entropy(&mut self) {
        if self.mode == KernelMode::Simulation {
            self.rng_container.root_rng_mut_ref().append_local_entropy();
        }
    }

    /// Returns a copy of the root VRF key.
    pub fn get_root_vrf_key(&self) -> SchnorrkelKeypair {
        self.root_rng_ref().get_root_vrf_key()
    }
}

impl fmt::Debug for Kernel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Hide internal details of the RNG container.
        write!(f, "Kernel {{ mode: {:?}, ... }}", self.mode)
    }
}

impl Default for Kernel {
    fn default() -> Self {
        Self {
            rng_container: RngContainer::default(),
            mode: KernelMode::Execution,
        }
    }
}
