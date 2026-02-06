use clap::Args;

/// Solc CLI arguments
#[derive(Debug, Clone, Args)]
pub struct SolcArgs {
    /// Enable optimizer
    #[clap(long)]
    pub optimize: bool,

    /// Set the runs parameter for optimizer (requires --optimize)
    #[clap(long)]
    pub optimizer_runs: Option<usize>,

    /// Skip tests that require via-IR compilation (compileViaYul: true).
    ///
    /// Use this when the solc binary does not support --via-ir.
    #[clap(long)]
    pub skip_via_ir: bool,
}
