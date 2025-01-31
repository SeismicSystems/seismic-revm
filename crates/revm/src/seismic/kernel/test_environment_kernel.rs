use core::fmt;
use schnorrkel::keys::Keypair as SchnorrkelKeypair;

use crate::seismic::rng::{LeafRng, RngContainer, RootRng};
use crate::seismic::Kernel;

use super::kernel_interface::{KernelKeys, KernelRng};

pub struct TestKernel {
    pub rng_container: RngContainer,
}

impl fmt::Debug for TestKernel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // We can’t easily peek into the trait object, so just say "Kernel { ... }"
        write!(f, "Kernel {{ ... }}")
    }
}

impl KernelRng for TestKernel {
    fn reset_rng(&mut self, root_vrf_key: SchnorrkelKeypair) {
        self.rng_container.reset_rng(root_vrf_key);
    }

    fn root_rng_ref(&self) -> &RootRng {
        self.rng_container.root_rng_ref()
    }

    fn root_rng_mut_ref(&mut self) -> &mut RootRng {
        self.rng_container.root_rng_mut_ref()
    }

    fn leaf_rng_mut_ref(&mut self) -> &mut Option<LeafRng> {
        self.rng_container.leaf_rng_mut_ref()
    }

    fn maybe_append_entropy(&mut self) {
        // noop for tests
    }
}

impl KernelKeys for TestKernel {
    fn get_eph_rng_keypair(&self) -> schnorrkel::Keypair {
        self.root_rng_ref().get_root_vrf_key()
    }
}

impl From<TestKernel> for Kernel {
    fn from(val: TestKernel) -> Self {
        Kernel::from_boxed(Box::new(val))
    }
}

/// TestKernel::clone() does not clone the leaf_rng
/// becayse cloning merlin::TranscriptRng is intentionally difficult
/// by the underlying merlin crate
/// leaf_rng is meant to be used once per call simulation, so
/// it should not be cloned mid-simulation
impl Clone for TestKernel {
    fn clone(&self) -> Self {
        Self {
            rng_container: self.rng_container.clone(),
        }
    }
}

impl Default for TestKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl TestKernel {
    pub fn new() -> Self {
        Self {
            rng_container: RngContainer::default(),
        }
    }
}
