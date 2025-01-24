use super::get_sample_schnorrkel_keypair;
use core::fmt;
use schnorrkel::keys::Keypair as SchnorrkelKeypair;
use secp256k1::SecretKey;
use tee_service_api::get_sample_secp256k1_sk;

use crate::seismic::rng::{LeafRng, RootRng};
use crate::seismic::Kernel;

use super::kernel_interface::{KernelKeys, KernelRng};

pub struct TestKernel {
    pub rng: RootRng,
    pub leaf_rng: Option<LeafRng>,
    pub secret_key: SecretKey,
    pub eph_rng_keypair: SchnorrkelKeypair,
}

impl fmt::Debug for TestKernel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // We can’t easily peek into the trait object, so just say "Kernel { ... }"
        write!(f, "Kernel {{ ... }}")
    }
}

impl KernelRng for TestKernel {
    fn root_rng_mut_ref(&mut self) -> &mut RootRng {
        &mut self.rng
    }

    fn leaf_rng_mut_ref(&mut self) -> &mut Option<LeafRng> {
        &mut self.leaf_rng
    }

    fn maybe_append_entropy(&mut self) {
        // noop for tests
    }
}

impl KernelKeys for TestKernel {
    fn get_io_key(&self) -> SecretKey {
        self.secret_key
    }
    fn get_eph_rng_keypair(&self) -> schnorrkel::Keypair {
        self.eph_rng_keypair.clone()
    }
}

impl Into<Kernel> for TestKernel {
    fn into(self) -> Kernel {
        Kernel::from_boxed(Box::new(self))
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
            rng: self.rng.clone(),
            leaf_rng: None,
            secret_key: self.secret_key,
            eph_rng_keypair: self.eph_rng_keypair.clone(),
        }
    }
}

impl TestKernel {
    pub fn new() -> Self {
        Self {
            rng: RootRng::new(get_sample_schnorrkel_keypair()),
            leaf_rng: None,
            secret_key: get_sample_secp256k1_sk(),
            eph_rng_keypair: get_sample_schnorrkel_keypair(),
        }
    }

    pub fn default() -> Self {
        Self {
            rng: RootRng::new(get_sample_schnorrkel_keypair()),
            leaf_rng: None,
            secret_key: get_sample_secp256k1_sk(),
            eph_rng_keypair: get_sample_schnorrkel_keypair(),
        }
    }
}
