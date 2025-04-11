//! Seismic-specific constants, types, and helpers.
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc as std;

pub mod api;
pub mod evm;
pub mod handler;
pub mod instructions;
pub mod precompiles;
pub mod rng_container;
pub mod spec;
pub mod transaction;

pub use api::{
    builder::SeismicBuilder,
    default_ctx::{DefaultSeismic, SeismicContext},
};
pub use evm::SeismicEvm;
pub use rng_container::RngContainer;
pub use spec::*;
