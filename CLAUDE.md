# CLAUDE.md

> **This repo is `seismic-revm`, a fork of [REVM](https://github.com/bluealloy/revm).**
> The first part of this file is the upstream CLAUDE.md, preserved verbatim so upstream merges apply cleanly.
> The [Seismic Fork Extensions](#seismic-fork-extensions) section below contains all Seismic-specific context and **takes precedence** where it overlaps with upstream (build commands, test commands, lint, architecture, etc.).

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

REVM is a highly efficient Rust implementation of the Ethereum Virtual Machine (EVM). It serves both as:
1. A standard EVM for executing Ethereum transactions
2. A framework for building custom EVM variants (like Optimism's op-revm)

The project is used by major Ethereum infrastructure including Reth, Foundry, Hardhat, Optimism, Scroll, and many zkVMs.

## Build and Development Commands

### Essential Commands
```bash
# Build the project
cargo build
cargo build --release

# Run all tests
cargo nexttest run --workspace

# Lint and format
cargo clippy --workspace --all-targets --all-features
cargo fmt --all

# Check no_std compatibility
cargo check --target riscv32imac-unknown-none-elf --no-default-features
cargo check --target riscv64imac-unknown-none-elf --no-default-features

# Run Ethereum state tests
cargo run -p revme statetest legacytests/Cancun/GeneralStateTests
```

### Test Scripts
```bash
# Download and run ethereum tests
./scripts/run-tests.sh

# Clean test fixtures and re-run
./scripts/run-tests.sh clean

# Run with specific profile
./scripts/run-tests.sh release
```

## Architecture

The workspace consists of these core crates:

- **revm**: Main crate that re-exports all others
- **revm-primitives**: Constants, primitive types, and core data structures
- **revm-interpreter**: EVM opcode implementations and execution engine
- **revm-context**: Execution context, environment, and journaled state
- **revm-handler**: Execution flow control and call frame management
- **revm-database**: State database traits and implementations
- **revm-precompile**: Ethereum precompiled contracts
- **revm-inspector**: Tracing and debugging framework
- **op-revm**: Example of custom EVM variant (Optimism)

### Key Design Patterns

1. **Trait-based Architecture**: Core functionality is defined through traits, allowing custom implementations
2. **Handler Pattern**: Execution flow is controlled through customizable handlers
3. **no_std Support**: All core crates support no_std environments
4. **Feature Flags**: Extensive use of feature flags for optional functionality

### Important Interfaces

1. **Database Trait** (`revm-database`): Defines how state is accessed
2. **Inspector Trait** (`revm-inspector`): Hooks for transaction tracing
3. **Handler Interface** (`revm-handler`): Customizable execution logic
4. **Context** (`revm-context`): Manages execution state and environment

## Current Development Context

When working on the `frame_stack` branch, note that significant refactoring is happening around:
- Frame and FrameData structures (moved from handler to context)
- Execution loop simplification
- Inspector trait cleanup

## Testing Strategy

1. Unit tests in each crate
2. Integration tests using Ethereum official test suite
3. Example projects demonstrating features
4. Benchmarking with CodSpeed

When adding new features:
- Ensure no_std compatibility
- Add appropriate feature flags
- Include tests for new functionality
- Update relevant examples if needed

---

# Seismic Fork Extensions

> **This is a Seismic fork of REVM.** The sections above are from upstream REVM and are preserved verbatim for merge compatibility. The sections below are specific to this Seismic fork and **take precedence** where they overlap (e.g., build commands, test commands, lint configuration).

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
- Git (workspace has git dependencies on `seismic-crypto` and `seismic-alloy-core`)

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

This runs all unit and integration tests across every crate (~398 tests). One `alloydb` RPC test is `#[ignore]`d by default (requires live RPC endpoint).

### Formatting

```bash
cargo fmt --all --check
```

### Lint

The Seismic CI runs two levels of checks:

```bash
# Standard warnings check (matches CI "warnings" job)
# Note: CI uses -A elided_named_lifetimes for Rust < 1.91 compat
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

Semantic tests validate Seismic Solidity compiler output against this VM. Requires the `ssolc` binary from [seismic-solidity](https://github.com/SeismicSystems/seismic-solidity). See the [Semantic Tests](#semantic-tests) section below for details. For comprehensive instructions (including building ssolc, running soltest.sh, and isoltest), use the [`/ssolc-tests` skill](https://github.com/SeismicSystems/seismic/tree/main/.claude/skills/ssolc-tests).

```bash
SSOLC=/path/to/ssolc
TESTS=/path/to/seismic-solidity/test/libsolidity/semanticTests

cargo run -p revme -- semantics --keep-going --unsafe-via-ir -s "$SSOLC" -t "$TESTS"
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
examples/                  10 example binaries (contract deployment, custom opcodes, uniswap, etc.)
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

### Storage opcode semantics

Each storage slot is a tuple `(value, is_private)`. The access rules form a strict matrix:

|           | (0, public)  | (x, public) | (0, private) | (x, private) |
| --------- | ------------ | ----------- | ------------ | ------------ |
| SLOAD     | 0            | x           | HALT         | HALT         |
| CLOAD     | 0            | x           | 0            | x            |
| SSTORE(y) | (y, public)  | (y, public) | HALT         | HALT         |
| CSTORE(y) | (y, private) | HALT        | (y, private) | (y, private) |

Key invariants:

- SLOAD/SSTORE cannot access private slots (halts with `InvalidPrivateStorageAccess`)
- CSTORE cannot overwrite non-zero public slots (halts with `InvalidPublicStorageAccess`)
- CLOAD/CSTORE use flat gas costs (no cold/warm distinction) to prevent information leakage
- CSTORE never issues gas refunds (would leak zero-clearing information)

### Seismic precompiles

All at fixed addresses, using flat gas to prevent side-channel leaks:

| Address | Name           | Function                                                    |
| ------- | -------------- | ----------------------------------------------------------- |
| 0x64    | RNG            | Cryptographically secure random bytes (VRF-based, stateful) |
| 0x65    | ECDH           | secp256k1 ECDH symmetric key derivation                     |
| 0x66    | AES-GCM Enc    | AES-GCM authenticated encryption                            |
| 0x67    | AES-GCM Dec    | AES-GCM authenticated decryption                            |
| 0x68    | HKDF           | HMAC-based key derivation                                   |
| 0x69    | secp256k1 Sign | secp256k1 ECDSA signing                                     |

The RNG precompile is **stateful** — it maintains an internal counter per transaction via `RngContainer` in `chain/rng_container.rs`, reset at the start of each transaction by the handler's pre-execution hook.

## Architecture Notes

1. **Trait-based extensibility**: Custom EVM variants (Seismic, Optimism) plug in via traits — `Database`, `Inspector`, `Host`, `Transaction`, `Block`. The handler pattern controls execution flow.
2. **no_std support**: All core crates work in `no_std` environments. Use `--no-default-features` to disable `std`.
3. **Feature flags**: Key features — `std` (default), `serde`, `c-kzg`, `blst`, `secp256k1`, `portable`, `hashbrown`. The `dev` feature enables relaxed validation for testing.
4. **Git-patched dependencies**: `Cargo.toml` patches `seismic-crypto` and `alloy-primitives` to Seismic forks (see `[patch.crates-io]`).

## Code Style

- Edition 2021, no `rustfmt.toml` (uses defaults)
- Workspace lints: `missing_debug_implementations` warn, `missing_docs` warn, `rust_2018_idioms` deny, `unreachable_pub` warn, `unused_must_use` deny
- The `seismic-revm` crate has stricter clippy rules: no `unwrap`, `expect`, `indexing_slicing`, `panic`, `unreachable`, or `todo`
- `#[cfg(not(feature = "std"))] extern crate alloc as std;` pattern for no_std compat
- Test modules use `#![allow(clippy::unwrap_used, ...)]` to relax strict rules

## CI

The only CI workflow that runs on the `seismic` branch is:

- **seismic.yml**: `rustfmt`, `cargo build`, warnings check (`-A elided_named_lifetimes -D warnings`), `cargo test --workspace`, strict clippy on `seismic-revm`, semantic tests with `ssolc`

Other workflow files in `.github/workflows/` (ci.yml, ethereum-tests.yml, bench.yml, release-plz.yml) are from upstream and do not run on Seismic branches.

## Branches

- `seismic` — default/production branch (PR target)
- `main` — upstream REVM tracking branch (latest merged upstream commit)

## Semantic Tests

The `revme semantics` subcommand (`bins/revme/src/cmd/semantics/`) compiles `.sol` files with `ssolc`, deploys them in the Seismic EVM, and validates output against expectation comments in the test files. This is how the Seismic Solidity compiler is tested against this VM.

### How it works

1. Parses `.sol` test files for function signatures and expected return values
2. Compiles each file with `ssolc` (the Seismic Solidity compiler)
3. Deploys the contract bytecode in a Seismic EVM instance
4. Calls each test function and compares output to expected values
5. Reports pass/fail per file, with `--keep-going` to continue past failures

### Running locally

```bash
# Clone seismic-solidity for test files
git clone https://github.com/SeismicSystems/seismic-solidity.git /tmp/seismic-solidity

# Get ssolc binary (build from source or download a release)
SSOLC=/path/to/ssolc
TESTS=/tmp/seismic-solidity/test/libsolidity/semanticTests

# Without optimizer, without --via-ir (default)
cargo run -p revme -- semantics --keep-going --unsafe-via-ir -s "$SSOLC" -t "$TESTS"

# With optimizer, without --via-ir
cargo run -p revme -- semantics --keep-going --unsafe-via-ir --optimize --optimizer-runs 200 -s "$SSOLC" -t "$TESTS"

# Without optimizer, with --via-ir
cargo run -p revme -- semantics --keep-going --unsafe-via-ir --via-ir -s "$SSOLC" -t "$TESTS"

# With optimizer, with --via-ir
cargo run -p revme -- semantics --keep-going --unsafe-via-ir --via-ir --optimize --optimizer-runs 200 -s "$SSOLC" -t "$TESTS"

# Run a single test file
cargo run -p revme -- semantics --unsafe-via-ir -s "$SSOLC" -t "$TESTS/path/to/test.sol"

# Run with trace output for debugging
cargo run -p revme -- semantics --trace --unsafe-via-ir -s "$SSOLC" -t "$TESTS/path/to/test.sol"

# Run with verbose logging
cargo run -p revme -- semantics -vvv --unsafe-via-ir -s "$SSOLC" -t "$TESTS/path/to/test.sol"
```

**Note:** `--unsafe-via-ir` bypasses a restriction in Seismic Solidity that prevents compiling `--via-ir` or `--experimental-via-ir`. It does not run all tests via IR — it just unlocks the ability to do so. See `cargo run -p revme -- semantics --help` for details.

### CI configuration

The CI semantic test job (`seismic.yml`) currently uses the `test--new-storage-opcode-semantics` branch of seismic-solidity for test files. It downloads the latest `ssolc` release binary and runs the full semantic test suite without the optimizer.

### Key files

```
bins/revme/src/cmd/semantics/
  semantic_tests.rs        Contract compilation and test case parsing
  test_cases.rs            Test case execution and result validation
  evm_handler.rs           Seismic EVM setup for test execution
  parser.rs                .sol file expectation comment parser
  compiler_evm_versions.rs EVM version mapping for ssolc
  solc_config.rs           ssolc compiler configuration
  errors.rs                Error types for test failures
  utils.rs                 Helper functions (EOF detection, function extraction)
```

## Troubleshooting

| Problem                                     | Fix                                                                                                                                     |
| ------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `gmp-mpfr-sys` build fails: "GMP not found" | Install GMP: `brew install gmp` (macOS) or `sudo apt-get install libgmp-dev m4` (Linux)                                                 |
| `cargo build` hangs downloading git deps    | Ensure SSH/HTTPS access to github.com. Set `CARGO_NET_GIT_FETCH_WITH_CLI=true` if behind a proxy                                        |
| `elided_named_lifetimes` lint warning       | Harmless on Rust >= 1.91 (renamed to `mismatched_lifetime_syntaxes`). CI uses `-A elided_named_lifetimes` for compat                    |
| clippy `--all-features` fails on test code  | Known: test code in `revm-state` has `useless_conversion` and `useless_vec` clippy warnings. Use `-p seismic-revm` for the strict check |
| `alloydb` test ignored / flaky              | The `can_get_basic` test requires a live RPC endpoint and is `#[ignore]`d by default                                                    |
| Semantic tests require `ssolc` binary       | Build from [seismic-solidity](https://github.com/SeismicSystems/seismic-solidity) or download a release. Not needed for unit tests      |
| `./scripts/run-tests.sh` downloads ~2GB     | First run downloads Ethereum test fixtures. Use `run-tests.sh clean` to redownload if corrupted                                         |
