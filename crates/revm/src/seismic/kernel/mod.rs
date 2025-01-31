pub mod kernel_interface;
pub use kernel_interface::KernelInterface;
mod test_environment_kernel;
pub use test_environment_kernel::TestKernel;

use dyn_clone::clone_box;
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
