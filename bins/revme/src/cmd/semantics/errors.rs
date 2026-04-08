use std::fmt;
use std::io::Error as IoError;

use primitives::Bytes;

use crate::cmd::semantics::test_cases::TestCase;

/// Reason a test file was skipped during processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Test uses multi-source layout (`==== Source:`)
    MultiSource,
    /// Test requires `allowNonExistingFunctions: true`
    NonExistingFunctions,
    /// Test requires `revertStrings: debug`
    DebugRevertStrings,
    /// Test requires via-IR but `--unsafe-via-ir` was not passed
    ViaIrUnsafeRequired,
    /// Test requires via-IR but `--skip-via-ir` was passed
    ViaIrSkipped,
    /// Test opts out of via-IR (`compileViaYul: false`) but `--via-ir` is forced
    ViaIrOptOut,
    /// Test requires EOF but `--eof` was not passed
    EofNotEnabled,
    /// Test's `// optimize:` filter excludes the current optimizer configuration
    OptimizerFiltered,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MultiSource => write!(f, "unsupported: multi-source files (==== Source:)"),
            Self::NonExistingFunctions => write!(f, "unsupported: allowNonExistingFunctions"),
            Self::DebugRevertStrings => write!(f, "unsupported: revertStrings: debug"),
            Self::ViaIrUnsafeRequired => {
                write!(f, "via-IR required (use --unsafe-via-ir to enable)")
            }
            Self::ViaIrSkipped => write!(f, "via-IR required (excluded by --skip-via-ir)"),
            Self::ViaIrOptOut => write!(f, "via-IR opt-out (excluded by --via-ir)"),
            Self::EofNotEnabled => write!(f, "EOF required (use --eof to enable)"),
            Self::OptimizerFiltered => {
                write!(f, "optimizer config excluded by // optimize: filter")
            }
        }
    }
}

impl SkipReason {
    /// All variants, used for iterating when printing summaries.
    pub const ALL: [SkipReason; 8] = [
        Self::MultiSource,
        Self::NonExistingFunctions,
        Self::DebugRevertStrings,
        Self::ViaIrUnsafeRequired,
        Self::ViaIrSkipped,
        Self::ViaIrOptOut,
        Self::EofNotEnabled,
        Self::OptimizerFiltered,
    ];

    /// Stable index for use with `SkipCounts`.
    pub fn index(self) -> usize {
        match self {
            Self::MultiSource => 0,
            Self::NonExistingFunctions => 1,
            Self::DebugRevertStrings => 2,
            Self::ViaIrUnsafeRequired => 3,
            Self::ViaIrSkipped => 4,
            Self::ViaIrOptOut => 5,
            Self::EofNotEnabled => 6,
            Self::OptimizerFiltered => 7,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Errors {
    #[error("The specified path does not exist")]
    PathNotExists,
    #[error("Invalid bytecode")]
    InvalidBytecode,
    #[error("Invalid input")]
    InvalidInput,
    #[error("Log Mismatch")]
    LogMismatch,
    #[error("Balance Mismatch")]
    BalanceMismatch,
    #[error("Storage Mismatch")]
    StorageMismatch,
    #[error("EVM Error: {0}")]
    EVMError(String),
    #[error(transparent)]
    Io(#[from] IoError),
    #[error("Invalid Test Format")]
    InvalidTestFormat,
    #[error("Skipped: {0}")]
    Skipped(SkipReason),
    #[error("Invalid function signature")]
    InvalidFunctionSignature,
    #[error("Invalid Test Output")]
    InvalidTestOutput,
    #[error("Invalid Argument Format")]
    InvalidArgumentFormat,
    #[error("Invalid Argument Count given Function Signature")]
    InvalidArgumentCount,
    #[error("Compilation Failed")]
    CompilationFailed,
    #[error("Compiler Not Found, Download Solc")]
    CompilerNotFound,
    #[error("Unexpected output. Received {0:0x}, Expected {1:0x}")]
    UnexpectedOutput(Bytes, Bytes),
    #[error("{0} test(s) failed")]
    TestsFailed(usize),
}

impl Into<Vec<(Errors, Option<TestCase>)>> for Errors {
    fn into(self) -> Vec<(Errors, Option<TestCase>)> {
        vec![(self, None)]
    }
}
