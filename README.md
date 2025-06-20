# Mercury Specification – Seismic’s REVM

<<<<<<< HEAD
Mercury is an EVM specification built by Seismic. This repository outlines the differences from standard EVM/REVM implementations. It will host our modifications to the EVM, as well as newly introduced features. This document serves as a diff report against REVM and assumes familiarity with both REVM and, more broadly, the EVM.
=======
[![CI](https://github.com/bluealloy/revm/actions/workflows/ci.yml/badge.svg)][gh-ci]
[![License](https://img.shields.io/badge/License-MIT-orange.svg)][mit-license]
[![crates.io](https://img.shields.io/crates/v/revm.svg)](https://crates.io/crates/revm)
[![Chat][tg-badge]][tg-url]
>>>>>>> b287ce025565c6f9206d5959c08acc401c8be5d4

This work stands on the shoulders of giants and would not have been possible without [REVM](https://github.com/bluealloy/revm)’s world-class codebase.

---

## Overview

We introduce several features:

- **Instruction Set:** CLOAD and CSTORE for accessing private storage.
- **Flagged Storage:** [Flagged Storage](#flagged-storage) introduces a novel mechanism where each slot is represented as a tuple `(value, is_private)` with strict access rules.
- **Precompiles:** [Precompiles](#precompiles) extend the functionality of the EVM.
- **Semantic Tests:** [Semantic Tests](#semantic-tests) help us catch regressions and validate new features.

---

## Semantic Tests

A new suite of semantic tests has been added to ensure that changes to the compiler do not introduce regressions. **Current limitations include:**
- No support for nested dependencies.
- Missing gas metering.
- Incomplete support for libraries and event emission.
- Lack of balance checks and handling of edge cases (e.g., non-existent function calls).

---

## Flagged Storage

Mercury introduces **Flagged Storage**, where each storage slot is now represented as a tuple:  

`(value, is_private)`

To support private storage, Mercury provides new instructions:
- **CLOAD:** Loads data from a slot marked as private.
- **CSTORE:** Stores data into a slot, tagging it as private.

**Access Rules:**
- **Loading:** The operation must match the slot’s privacy flag. Attempting to load a slot using an instruction that doesn’t match its privacy (e.g., using SLOAD on a private slot or CLOAD on a public slot) is disallowed. The only caveat is that CLOAD can load public slot with value 0.
- **Storing:** Writing to a slot is allowed regardless of its current privacy flag, enabling seamless transitions between public and private states.

**Gas Costs:**  
Confidential storage operations (both load and store) incur the same gas costs as their public counterparts.

---

## Precompiles

Mercury adds several new precompiles to enhance the functionality of the REVM. These precompiles are available at fixed addresses:

| **Precompile**             | **Address (Hex)** | **Address (Dec)** |
|----------------------------|-------------------|-------------------|
| RNG                        | `0x64`            | 100               |
| ECDH                       | `0x65`            | 101               |
| AES-GCM Encryption         | `0x66`            | 102               |
| AES-GCM Decryption         | `0x67`            | 103               |
| HDKF                       | `0x68`            | 104               |
| SECP256K1 Signature        | `0x69`            | 105               |

---

## Enhanced RNG Logic

The RNG precompile works jointly with two additional parameters in the transaction environment (`TX_ENV`):

- **tx_hash:** Provides domain separation.
- **RNG_mode:** Introduces extra entropy for simulation calls.

**State Management:**  
Since RNG is stateful, a pre-execution hook resets its state at the start of every transaction, ensuring consistency and improved security.

Note that the inner logic of this precompile is strongly inspired from [Oasis Sapphire work](https://oasisprotocol.org/sapphire).

---

## Upstream

The upstream repository lives [here](https://github.com/bluealloy/revm). This fork is up-to-date with it through commit `398ef74`. You can see this by viewing the [main](https://github.com/SeismicSystems/seismic-revm/tree/main) branch on this repository

You can view all of our changes vs. upstream on this [pull request](https://github.com/SeismicSystems/seismic-revm/pull/2). The sole purpose of this PR is to display our diff; it will never be merged in to the main branch of this repo

### Structure

Seismic's forks of the [reth](https://github.com/paradigmxyz/reth) stack all have the same branch structure:
- `main` or `master`: this branch only consists of commits from the upstream repository. However it will rarely be up-to-date with upstream. The latest commit from this branch reflects how recently Seismic has merged in upstream commits to the seismic branch
- `seismic`: the default and production branch for these repositories. This includes all Seismic-specific code essential to make our network run

---

## Conclusion

We are working on many more features, so you can expect this diff documentation to grow over time. At this stage, this is still **experimental** software, so tread with caution!

Don't hesitate to get in touch—we'd also be delighted to onboard new contributors to this repository.
