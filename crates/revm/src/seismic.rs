//! Seismic specific precompiles and handler_register.

mod handler_register;
pub mod kernel;
pub mod precompiles;
pub mod rng;


pub use handler_register::{load_precompiles, seismic_handle_register};
pub use kernel::{Kernel, KernelInterface};
