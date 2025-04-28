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

impl SeismicHaltReason {
    pub fn try_from_error_string(error_str: &str) -> Option<Self> {
        match () {
            _ if error_str.contains("InvalidPublicStorageAccess") => {
                Some(Self::InvalidPublicStorageAccess)
            }
            _ if error_str.contains("InvalidPrivateStorageAccess") => {
                Some(Self::InvalidPrivateStorageAccess)
            }
            _ => None,
        }
    }

    pub fn try_from_error_string_exact(error_str: &str) -> Option<Self> {
        match error_str {
            "FatalExternalError: InvalidPublicStorageAccess" => {
                Some(Self::InvalidPublicStorageAccess)
            }
            "FatalExternalError: InvalidPrivateStorageAccess" => {
                Some(Self::InvalidPrivateStorageAccess)
            }
            _ => None,
        }
    }
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
            SeismicHaltReason::InvalidPrivateStorageAccess => {
                write!(f, "InvalidPrivateStorageAccess")
            }
            SeismicHaltReason::InvalidPublicStorageAccess => {
                write!(f, "InvalidPublicStorageAccess")
            }
        }
    }
}

impl From<SeismicHaltReason> for ContextError<Infallible> {
    fn from(reason: SeismicHaltReason) -> Self {
        ContextError::Custom(reason.to_string())
    }
}
