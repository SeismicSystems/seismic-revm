use core::fmt;
use std::convert::Infallible;

use revm::context_interface::{context::ContextError, result::HaltReason};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SeismicHaltReason {
    Base(HaltReason),
    /// Invalid Private Storage Access: Cannot access private storage with public instructions
    InvalidPrivateStorageAccess,
    /// Invalid Public Storage Access: Cannot access public storage with private instructions
    InvalidPublicStorageAccess,
}

impl From<HaltReason> for SeismicHaltReason {
    fn from(value: HaltReason) -> Self {
        Self::Base(value)
    }
}

impl fmt::Display for SeismicHaltReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeismicHaltReason::Base(_) => write!(f, "Base error"),
            SeismicHaltReason::InvalidPrivateStorageAccess => write!(f, "InvalidPrivateStorageAccess"),
            SeismicHaltReason::InvalidPublicStorageAccess => write!(f, "InvalidPublicStorageAccess"),
        }
    }
}

impl From<SeismicHaltReason> for ContextError<Infallible> {
    fn from(reason: SeismicHaltReason) -> Self {
        ContextError::Custom(reason.to_string())
    }
}
