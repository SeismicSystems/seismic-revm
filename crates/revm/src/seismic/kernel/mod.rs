pub mod kernel_interface;
use dyn_clone::clone_box;
pub use kernel_interface::KernelInterface;
mod context;
mod test_environment_kernel;
use test_environment_kernel::TestKernel;

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
