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
    /// Requires --unsafe-via-ir (the seismic solc compiler requires
    /// --unsafe-via-ir whenever --via-ir is used).
    ///
    /// Mutually exclusive with --skip-via-ir.
    ///
    ///   compileViaYul | --via-ir | --skip-via-ir | --unsafe-via-ir | result
    ///  ───────────────+──────────+───────────────+─────────────────+──────────────────────────────
    ///   true          |  off     |  off          | off             | SKIP (needs --unsafe-via-ir)
    ///   true          |  off     |  off          | on              | compile with --via-ir --unsafe-via-ir
    ///   true          |  on*     |  —            | on              | compile with --via-ir --unsafe-via-ir
    ///   true          |  —       |  on           | —               | SKIP
    ///   false         |  off     |  any          | any             | compile without --via-ir
    ///   false         |  on*     |  —            | on              | SKIP
    ///   (not set)     |  off     |  any          | any             | compile without --via-ir
    ///   (not set)     |  on*     |  —            | on              | compile with --via-ir --unsafe-via-ir
    ///
    ///   *--via-ir requires --unsafe-via-ir (clap validation).
    ///
    /// Commonly combined with --optimize. Without --optimize, via-IR
    /// can produce inefficient bytecode and "stack too deep" errors.
    /// Example: --via-ir --unsafe-via-ir --optimize --optimizer-runs 200
    #[clap(long, conflicts_with = "skip_via_ir", requires = "unsafe_via_ir")]
    pub via_ir: bool,

    /// Skip tests that require via-IR compilation (compileViaYul: true).
    ///
    /// Use this when the solc binary does not support --via-ir.
    /// Mutually exclusive with --via-ir and --unsafe-via-ir.
    #[clap(long, conflicts_with = "via_ir")]
    pub skip_via_ir: bool,

    /// Allow via-IR compilation by also passing --unsafe-via-ir to the compiler.
    ///
    /// The seismic solc compiler requires this flag whenever --via-ir is used.
    /// Without this flag, tests that require via-IR (compileViaYul: true) are
    /// automatically skipped.
    ///
    /// Mutually exclusive with --skip-via-ir.
    #[clap(long, conflicts_with = "skip_via_ir")]
    pub unsafe_via_ir: bool,
}
