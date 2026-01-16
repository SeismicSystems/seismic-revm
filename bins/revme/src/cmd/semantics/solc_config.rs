use super::compiler_evm_versions::EVMVersion;
use clap::Args;

/// Configuration for solc compilation
#[derive(Debug, Clone, Args)]
pub struct SolcConfig {
    /// Enable optimizer
    #[clap(long)]
    pub optimize: bool,

    /// Set the runs parameter for optimizer (requires --optimize)
    #[clap(long)]
    pub optimizer_runs: Option<usize>,
}

impl SolcConfig {
    pub fn new(optimize: bool, optimizer_runs: Option<usize>) -> Self {
        Self {
            optimize,
            optimizer_runs,
        }
    }
}

/// Internal compilation parameters
#[derive(Debug, Clone)]
pub struct CompilationParams {
    pub evm_version: Option<EVMVersion>,
    pub via_ir: bool,
    pub eof_mode: bool,
    pub runtime: bool,
    pub optimize: bool,
    pub optimizer_runs: Option<usize>,
}

impl CompilationParams {
    pub fn new(
        evm_version: Option<EVMVersion>,
        via_ir: bool,
        eof_mode: bool,
        runtime: bool,
        optimize: bool,
        optimizer_runs: Option<usize>,
    ) -> Self {
        Self {
            evm_version,
            via_ir,
            eof_mode,
            runtime,
            optimize,
            optimizer_runs,
        }
    }

    pub fn from_solc_config(
        solc_config: &SolcConfig,
        evm_version: Option<EVMVersion>,
        via_ir: bool,
        eof_mode: bool,
        runtime: bool,
    ) -> Self {
        Self {
            evm_version,
            via_ir,
            eof_mode,
            runtime,
            optimize: solc_config.optimize,
            optimizer_runs: solc_config.optimizer_runs,
        }
    }
}
