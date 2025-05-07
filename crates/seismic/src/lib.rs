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
    default_ctx::{DefaultSeismic, SeismicContext},
};
pub use chain::seismic_chain::SeismicChain;
pub use evm::SeismicEvm;
pub use instructions::seismic_host::SeismicHost;
pub use result::SeismicHaltReason;
pub use spec::*;
