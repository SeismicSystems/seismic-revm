//! Seismic-specific constants, types, and helpers.
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc as std;

pub mod api;
pub mod chain;
pub mod evm;
pub mod handler;
pub mod instructions;
pub mod precompiles;
pub mod result;
pub mod spec;
pub mod transaction;

pub use api::{
    builder::SeismicBuilder,
    default_ctx::{DefaultSeismicContext, SeismicContext},
};
pub use chain::seismic_chain::SeismicChain;
pub use evm::SeismicEvm;
pub use instructions::seismic_host::SeismicHost;
pub use result::SeismicHaltReason;
use schnorrkel::Keypair as SchnorrkelKeypair;
use schnorrkel::{ExpansionMode, MiniSecretKey};
pub use spec::*;
pub use transaction::abstraction::SeismicTransaction;

// used for testing purposes and defaults
pub(crate) fn get_unsecure_sample_schnorrkel_keypair() -> SchnorrkelKeypair {
    let mini_secret_key = MiniSecretKey::from_bytes(&[
        221, 143, 4, 149, 139, 56, 101, 208, 232, 50, 47, 39, 112, 211, 4, 111, 63, 63, 202, 141,
        138, 195, 190, 41, 139, 177, 214, 90, 176, 210, 173, 14,
    ])
    .unwrap();
    mini_secret_key.expand(ExpansionMode::Uniform).into()
}
