pub mod kernel_interface;
pub use kernel_interface::KernelInterface;
mod test_environment_kernel;
pub use test_environment_kernel::TestKernel;

use dyn_clone::clone_box;
use schnorrkel::{keys::Keypair as SchnorrkelKeypair, ExpansionMode, MiniSecretKey};
use std::ops::{Deref, DerefMut};

#[derive(Debug)]
pub struct Kernel(Box<dyn KernelInterface>);

impl Kernel {
    pub fn from_boxed(inner: Box<dyn KernelInterface>) -> Self {
        Self(inner)
    }

    /// A default kernel for testing that loads sample keys.
    /// We do not implement the Default trait becuase
    /// it might be misleading or error-prone.
    pub fn test_default() -> Self {
        Self(Box::new(TestKernel::default()))
    }
}

impl Clone for Kernel {
    fn clone(&self) -> Self {
        Kernel(clone_box(&*self.0))
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

pub fn get_sample_schnorrkel_keypair() -> SchnorrkelKeypair {
    let mini_secret_key = MiniSecretKey::from_bytes(&[
        221, 143, 4, 149, 139, 56, 101, 208, 232, 50, 47, 39, 112, 211, 4, 111, 63, 63, 202, 141,
        138, 195, 190, 41, 139, 177, 214, 90, 176, 210, 173, 14,
    ])
    .unwrap();
    mini_secret_key.expand(ExpansionMode::Uniform).into()
}
