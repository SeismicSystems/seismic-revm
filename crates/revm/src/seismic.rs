//! Seismic specific precompiles and handler_register.

mod handler_register;
pub mod rng;

pub use handler_register::{seismic_handle_register, load_precompiles};
