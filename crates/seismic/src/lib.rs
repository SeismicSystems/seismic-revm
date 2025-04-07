//! Seismic-specific constants, types, and helpers.
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc as std;

pub mod api;
pub mod handler;
pub mod precompiles;
pub mod transaction;
pub mod spec;
pub mod evm;

pub use spec::*;
pub use api::{
    builder::SeismicBuilder,
    default_ctx::{DefaultSeismic, SeismicContext},
};

pub use evm::SeismicEvm;
