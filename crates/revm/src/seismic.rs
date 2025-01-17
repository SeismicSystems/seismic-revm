//! Seismic specific precompiles and handler_register.

pub mod eph_key;
mod handler_register;
pub mod kernel;
pub mod rng;

pub use handler_register::{load_precompiles, seismic_handle_register, set_up_seismic_kernel};
pub use kernel::{new_test_kernel_box, Kernel, KernelInterface};
