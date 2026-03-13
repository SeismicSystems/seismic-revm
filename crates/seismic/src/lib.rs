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

/// The address that receives the base fee (instead of burning it).
pub const BASE_FEE_RECIPIENT: revm::primitives::Address =
    revm::primitives::address!("0x1111000000000000000000000000000000000001");

pub use api::{
    builder::SeismicBuilder,
    default_ctx::{DefaultSeismicContext, SeismicContext},
};
pub use chain::seismic_chain::SeismicChain;
pub use evm::SeismicEvm;
pub use instructions::seismic_host::SeismicHost;
pub use result::SeismicHaltReason;
pub use spec::*;
pub use transaction::abstraction::SeismicTransaction;
