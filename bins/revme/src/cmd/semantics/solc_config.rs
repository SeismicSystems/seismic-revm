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

    /// Compile via Yul intermediate representation (--via-ir).
    ///
    /// Forces all tests through solc's via-IR pipeline, UNLESS a test
    /// explicitly opts out with `// compileViaYul: false` — in which
    /// case the test is skipped entirely.
    ///
    /// Mutually exclusive with --skip-via-ir.
    ///
    ///   compileViaYul directive | --via-ir | --skip-via-ir | result
    ///  ────────────────────────-+─────────-+───────────────+──────────────────────
    ///   true                    |  off     |  off          | compile with --via-ir
    ///   true                    |  on      |  n/a          | compile with --via-ir
    ///   true                    |  n/a     |  on           | SKIP test
    ///   false                   |  off     |  any          | compile without --via-ir
    ///   false                   |  on      |  n/a          | SKIP test
    ///   (not set)               |  off     |  any          | compile without --via-ir
    ///   (not set)               |  on      |  n/a          | compile with --via-ir
    ///
    /// Commonly combined with --optimize. Without --optimize, via-IR
    /// can produce inefficient bytecode and "stack too deep" errors.
    /// Example: --via-ir --optimize --optimizer-runs 200
    #[clap(long, conflicts_with = "skip_via_ir")]
    pub via_ir: bool,

    /// Skip tests that require via-IR compilation (compileViaYul: true).
    ///
    /// Use this when the solc binary does not support --via-ir.
    /// Mutually exclusive with --via-ir.
    #[clap(long, conflicts_with = "via_ir")]
    pub skip_via_ir: bool,
}
