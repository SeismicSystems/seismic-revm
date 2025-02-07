# Mercury Specification – Seismic’s REVM

Mercury is an EVM specification built by Seismic. This repository outlines the differences from standard EVM/REVM implementations. It will host our modifications to the EVM, as well as newly introduced features. This document serves as a diff report against REVM and assumes familiarity with both REVM and, more broadly, the EVM.

This work stands on the shoulders of giants and would not have been possible without [REVM](https://github.com/bluealloy/revm)’s world-class codebase.

---

## Overview

We introduces several features:

- **Instruction Set:** [CLOAD and CSTORE](#flagged-storage) for accessing private storage.
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
- **Loading:** The operation must match the slot’s privacy flag. Attempting to load a slot using an instruction that doesn’t match its privacy (e.g., using SLOAD on a private slot or CLOAD on a public slot) is disallowed.
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
| HDFK                       | `0x68`            | 104               |
| SECP256K1 Signature        | `0x69`            | 105               |

---

## Enhanced RNG Logic

The RNG precompile works jointly with two additional parameters in the transaction environment (`TX_ENV`):

- **tx_hash:** Provides domain separation.
- **RNG_mode:** Introduces extra entropy for simulation calls.

**State Management:**  
Since RNG is stateful, a pre-execution hook resets its state at the start of every transaction, ensuring consistency and improved security.

---

## Conclusion

We are working on many more features, so you can expect this diff documentation to grow over time.

Don't hesitate to get in touch—we'd also be delighted to onboard new contributors to this repository.
