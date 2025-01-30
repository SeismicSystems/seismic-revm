use core::fmt::Debug;

use dyn_clone::DynClone;
use schnorrkel::keys::Keypair as SchnorrkelKeypair;

use crate::seismic::rng::{LeafRng, RootRng};

pub trait KernelInterface: KernelRng + KernelKeys + DynClone + Debug {}
impl<T: KernelRng + KernelKeys + DynClone + Debug> KernelInterface for T {}

pub trait KernelRng {
    /// Resets the RNG with a new root VRF key.
    fn reset_rng(&mut self, root_vrf_key: SchnorrkelKeypair);

    // returns the root rng for the entire block
    fn root_rng_mut_ref(&mut self) -> &mut RootRng;

    // returns a LeafRng Option for the current transaction, None if not initialized
    fn leaf_rng_mut_ref(&mut self) -> &mut Option<LeafRng>;

    // maybe appends entropy to the root rng
    fn maybe_append_entropy(&mut self);
}

pub trait KernelKeys {
    // returns the key for decrypting transaction data
    fn get_io_key(&self) -> secp256k1::SecretKey;

    // returns the vrf key for rng transcripts
    fn get_eph_rng_keypair(&self) -> SchnorrkelKeypair;
}
