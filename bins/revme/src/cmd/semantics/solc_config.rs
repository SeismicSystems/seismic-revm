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
}
