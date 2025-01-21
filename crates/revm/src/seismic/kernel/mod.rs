pub mod kernel_interface;
use dyn_clone::clone_box;
pub use kernel_interface::KernelInterface;
mod context;
mod test_environment_kernel;
use schnorrkel::{keys::Keypair as SchnorrkelKeypair, ExpansionMode, MiniSecretKey};
pub use test_environment_kernel::TestKernel;

use crate::primitives::Env;
use std::ops::{Deref, DerefMut};

#[derive(Debug)]
pub struct Kernel(Box<dyn KernelInterface>);

impl Kernel {
    pub fn from_boxed(inner: Box<dyn KernelInterface>) -> Self {
        Self(inner)
    }
}

impl Clone for Kernel {
    fn clone(&self) -> Self {
        Kernel(clone_box(&*self.0))
    }
}

impl Default for Kernel {
    fn default() -> Self {
        Self(Box::new(TestKernel::default()))
    }
}

impl Deref for Kernel {
    type Target = dyn KernelInterface;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl DerefMut for Kernel {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.0
    }
}

pub fn new_test_kernel_box(env: &Env) -> Kernel {
    let kernel = TestKernel::new(env);
    Kernel::from_boxed(Box::new(kernel))
}

pub fn get_sample_schnorrkel_keypair() -> SchnorrkelKeypair {
    let mini_secret_key = MiniSecretKey::from_bytes(&[
        221, 143, 4, 149, 139, 56, 101, 208, 232, 50, 47, 39, 112, 211, 4, 111, 63, 63, 202, 141,
        138, 195, 190, 41, 139, 177, 214, 90, 176, 210, 173, 14,
    ])
    .unwrap();
    mini_secret_key.expand(ExpansionMode::Uniform).into()
}
