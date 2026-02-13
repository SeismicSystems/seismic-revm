# Seismic REVM (seismic-revm)

Fork of [REVM](https://github.com/bluealloy/revm) (Rust Ethereum Virtual Machine) that adds **confidential storage** and privacy-preserving precompiles to the EVM. This is the "Mercury" specification — Seismic's EVM variant. Upstream is tracked through the `main` branch.

## What This Does

Standard EVM storage is publicly readable. Seismic extends REVM with:

- **CLOAD/CSTORE opcodes** (0xB0/0xB1) — load/store to confidential storage slots. Each slot is a tuple `(value, is_private)` with strict access rules: SLOAD on a private slot halts, CSTORE on a non-zero public slot halts.
- **Privacy-preserving precompiles** — RNG (0x64), ECDH (0x65), AES-GCM encrypt/decrypt (0x66/0x67), HKDF (0x68), secp256k1 sign (0x69).
- **Flat gas costs** for confidential ops to prevent gas-based side-channel leaks.
- **Semantic test runner** in `revme` for testing Seismic Solidity (`ssolc`) compiler output against this VM.

## Build

Rust workspace using Cargo. MSRV: **1.88.0**. Output binary: `target/debug/revme` (or `target/release/revme`).

### Prerequisites (all platforms)

- Rust toolchain >= 1.88.0 (`rustup update stable`)
- C compiler (for native crypto deps: `blst`, `c-kzg`, `secp256k1-sys`, `gmp-mpfr-sys`)
- GMP library (for `rug`/`gmp-mpfr-sys` crate used by modexp precompile)
- Git (workspace has git dependencies on `seismic-enclave` and `seismic-alloy-core`)

### macOS

```bash
# Install system deps
brew install gmp

# Build (debug)
cargo build --workspace

# Build (release)
cargo build --workspace --release
```

### Linux (Ubuntu/Debian)

```bash
# Install system deps
sudo apt-get update
sudo apt-get install -y build-essential libgmp-dev m4

# Build (debug)
cargo build --workspace

# Build (release)
cargo build --workspace --release
```

### Verify

```bash
cargo run -p revme -- --help
# Expected: Usage: revme <COMMAND>
# Commands: statetest, stest, evm, semantics, bytecode, bench, blockchaintest, btest
```

## Test

### Unit & integration tests

```bash
cargo test --workspace
```

This runs all unit and integration tests across every crate (~400 tests). One `alloydb` RPC test is `#[ignore]`d by default (flaky remote RPC).

### Formatting

```bash
cargo fmt --all --check
```

### Lint

The Seismic CI runs two levels of clippy:

```bash
# Standard warnings check (matches CI "warnings" job)
RUSTFLAGS="-D warnings" cargo check

# Strict clippy on seismic crate only (matches CI "clippy-strict" job)
cargo clippy -p seismic-revm --lib --tests --no-deps \
  -- -D warnings \
  -W clippy::unwrap_used \
  -W clippy::expect_used \
  -W clippy::indexing_slicing \
  -W clippy::panic \
  -W clippy::unreachable \
  -W clippy::todo
```

### Ethereum state & blockchain tests (requires fixture download)

```bash
# Downloads ~2GB of test fixtures on first run, then executes state/blockchain tests via revme
./scripts/run-tests.sh

# Clean fixtures and redownload
./scripts/run-tests.sh clean

# Run with release profile (faster)
./scripts/run-tests.sh release
```

### Semantic tests (requires `ssolc` binary)

Semantic tests validate Seismic Solidity compiler output against this VM. Requires the `ssolc` binary from [seismic-solidity](https://github.com/SeismicSystems/seismic-solidity).

```bash
# Build ssolc from source, or download a release binary
SSOLC=/path/to/ssolc
TESTS=/path/to/seismic-solidity/test/libsolidity/semanticTests

cargo run -p revme -- semantics --keep-going -s "$SSOLC" -t "$TESTS"
```

## Project Layout

```
bins/revme/                CLI binary — state tests, blockchain tests, semantic tests, benchmarks, EVM runner
crates/
  revm/                    Main crate, re-exports all sub-crates
  seismic/                 Seismic EVM variant (CLOAD/CSTORE, precompiles, SeismicHost, SeismicBuilder)
  primitives/              Core types: Address, B256, U256, SpecId, etc.
  bytecode/                EVM bytecode parsing, analysis, jump maps
  interpreter/             Opcode dispatch, stack, memory, instruction execution
  context/                 Execution context, journaled state, environment
  context/interface/       Trait interfaces for context (Transaction, Block, Cfg)
  handler/                 Execution flow: validation, pre/post execution, call frames
  database/                State DB impls: CacheDB, AlloyDB, BundleState
  database/interface/      Database trait definitions
  state/                   Account/storage state types, status flags
  precompile/              Ethereum precompiles (ecRecover, SHA-256, BN254, BLS12-381, KZG, P256)
  inspector/               Transaction tracing, gas inspection, step debugging
  op-revm/                 Optimism EVM variant (L1 cost, deposit tx, system calls)
  statetest-types/         Types for Ethereum state test JSON format
  ee-tests/                Shared end-to-end test utilities
examples/                  9 example binaries (contract deployment, custom opcodes, uniswap, etc.)
scripts/
  run-tests.sh             Download Ethereum test fixtures and run state/blockchain tests
  publish.sh               Publish crates to crates.io in dependency order
```

## Key Seismic Modifications

The `crates/seismic/` crate is the core Seismic layer, built on top of upstream REVM:

- **Confidential storage**: `instructions/confidential_storage.rs` — CLOAD (0xB0) and CSTORE (0xB1) opcode implementations with privacy flag semantics
- **SeismicHost trait**: `instructions/seismic_host.rs` — extends the Host trait with confidential storage and RNG access
- **Custom precompiles**: `precompiles/` — RNG, ECDH, AES-GCM, HKDF, secp256k1 signing
- **SeismicBuilder**: `api/builder.rs` — builder pattern for constructing Seismic EVM instances
- **SeismicEvm**: `evm.rs` — the top-level Seismic EVM type
- **Spec IDs**: `spec.rs` — Mercury spec ID extending Ethereum's SpecId
- **Handler overrides**: `handler.rs` — custom pre-execution hooks (RNG state reset)
- **Flagged storage in context**: `crates/context/src/journal/` — journaled state tracks `(value, is_private)` tuples per slot

The `revme semantics` subcommand (`bins/revme/src/cmd/semantics/`) compiles `.sol` files with `ssolc`, deploys them in the Seismic EVM, and validates output against expectation comments in the test files.

## Architecture Notes

1. **Trait-based extensibility**: Custom EVM variants (Seismic, Optimism) plug in via traits — `Database`, `Inspector`, `Host`, `Transaction`, `Block`. The handler pattern controls execution flow.
2. **no_std support**: All core crates work in `no_std` environments. Use `--no-default-features` to disable `std`.
3. **Feature flags**: Key features — `std` (default), `serde`, `c-kzg`, `blst`, `secp256k1`, `portable`, `hashbrown`. The `dev` feature enables relaxed validation for testing.
4. **Git-patched dependencies**: `Cargo.toml` patches `seismic-enclave` and `alloy-primitives` to Seismic forks (see `[patch.crates-io]`).

## Code Style

- Edition 2021, no `rustfmt.toml` (uses defaults)
- Workspace lints: `missing_debug_implementations` warn, `missing_docs` warn, `rust_2018_idioms` deny, `unreachable_pub` warn, `unused_must_use` deny
- The `seismic-revm` crate has stricter clippy rules: no `unwrap`, `expect`, `indexing_slicing`, `panic`, `unreachable`, or `todo`
- `#[cfg(not(feature = "std"))] extern crate alloc as std;` pattern for no_std compat

## CI

GitHub Actions (`.github/workflows/`):

- **seismic.yml** (primary, on `seismic` branch): `rustfmt`, `cargo build`, warnings check, `cargo test --workspace`, strict clippy on `seismic-revm`, semantic tests with `ssolc`
- **ci.yml** (upstream, on `main`): matrix test (stable/nightly, feature combos), no_std check, clippy, docs, fmt, feature propagation via zepter, typos
- **ethereum-tests.yml**: Full Ethereum state & blockchain test suite via `scripts/run-tests.sh`
- **bench.yml**: CodSpeed benchmarks
- **release-plz.yml**: Automated releases

## Branches

- `seismic` — default/production branch (PR target)
- `main` — upstream REVM tracking branch (latest merged upstream commit)

## Troubleshooting

| Problem                                       | Fix                                                                                                                                     |
| --------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `gmp-mpfr-sys` build fails: "GMP not found"   | Install GMP: `brew install gmp` (macOS) or `sudo apt-get install libgmp-dev m4` (Linux)                                                 |
| `cargo build` hangs downloading git deps      | Ensure SSH/HTTPS access to github.com. Set `CARGO_NET_GIT_FETCH_WITH_CLI=true` if behind a proxy                                        |
| `elided_named_lifetimes` lint renamed warning | Harmless on Rust >= 1.91. The lint was renamed to `mismatched_lifetime_syntaxes`. Does not affect compilation                           |
| clippy `--all-features` fails on test code    | Known: test code in `revm-state` has `useless_conversion` and `useless_vec` clippy warnings. Use `-p seismic-revm` for the strict check |
| `alloydb` test ignored / flaky                | The `can_get_basic` test requires a live RPC endpoint and is `#[ignore]`d by default                                                    |
| Semantic tests require `ssolc` binary         | Build from [seismic-solidity](https://github.com/SeismicSystems/seismic-solidity) or download a release. Not needed for unit tests      |
| `./scripts/run-tests.sh` downloads ~2GB       | First run downloads Ethereum test fixtures. Use `run-tests.sh clean` to redownload if corrupted                                         |
