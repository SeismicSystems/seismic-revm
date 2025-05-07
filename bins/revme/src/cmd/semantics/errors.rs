use std::io::Error as IoError;
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
    #[error("EVM Error")]
    EVMError,
    #[error(transparent)]
    Io(#[from] IoError),
    #[error("Invalid Test Format")]
    InvalidTestFormat,
    #[error("Unhandled Test Format: === Source:")]
    UnhandledTestFormat,
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
}
