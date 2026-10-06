# Registry-based gas payment (fresh-chain Mercury)

The `seismic-revm` handler resolves fee assets from the current execution journal. This replaces the hardcoded USDC address, one-byte mapping root, and six-decimal assumption. The fresh chain activates registry selection, internal fee reserves, and non-warming system access from genesis; there is no legacy execution or migration path.

## Selection and metadata

`SeismicTransaction.gas_payment` carries public, authenticated metadata from the consensus transaction. Standard transactions use `GasPayment::Auto`; only type `0x4A` may carry `Native` or `Token(address)`.

- **Auto:** native first, otherwise the first individually affordable eligible token in registry insertion order. The native fast path does not read the registry.
- **Native:** native only, with no registry reads or token fallback.
- **Token(address):** that registered token only, even if native currency could fund gas. Invalid/inactive/unsupported/incompatible/insufficient selections fail deterministically, without fallback.

Native currency always funds execution-environment value. The failed-decryption adapter must preserve the selector and signed transaction while setting only the environment value to zero before validation. No fees may be split across balances.

The registry address is `0x0000000000000000000000476173546f6b656e73`. Slot 1 contains a bounded count (at most 32). Entries have a two-word stride at `keccak256(uint256(1))`: address at bits 0–159, active byte at 160, mode byte at 168, and decimals byte at 176. The next word contains the full 256-bit balance mapping root. Upper padding and configuration privacy flags are ignored at runtime. The registry is intended to use public configuration storage; this crate does not enforce genesis configuration or node-startup policy.

Mode 0 is **Shielded**: private balances and zero-public initialization candidates are eligible. Mode 1 is **Public**: only public balances are eligible. Contradictory balances are skipped under Auto and rejected under explicit selection. Decimals are immutable owner-supplied metadata in `0..=18`; zero means zero precision. Execution does not call token code or discover decimals dynamically. One whole token pays for one whole native unit; no exchange rates are introduced.

`gas_token_registry` exposes snapshot readers, lazy exact selection/lookup, full eligible-token visitation, approximate aggregate pool balances, mapping keys, and `TokenPrecision` conversion/allowance helpers. Required storage errors are separate from typed transaction invalidity. Exact Auto selection stops at the first sufficient candidate. Explicit lookup reads only metadata until the match. Aggregate reporting scans every eligible candidate and must propagate required later read failures.

`visit_registered_tokens()` provides a configuration-only traversal for targeted pool maintenance. It visits active entries with supported mode/precision and their full-width roots, without reading holder balances. Inactive/unsupported entries require no root reads. Consumers can match execution storage changes to known pooled senders' mapping keys, then use `aggregate_balance()` for the affected holders. Required registry failures propagate; partial discoveries must not be treated as complete results.

## Settlement and access

For `D = 10^(18 - decimals)`:

```text
maximum requirement = ceil(maximum gas wei / D)
upfront debit       = ceil(effective upfront wei / D)
refund              = floor(unused-gas reimbursement wei / D)
beneficiary reward  = upfront debit - refund
```

The handler debits the caller, executes bytecode, credits the refund from an internal `TokenFeeReserve`, and only then rewards the beneficiary. The reserve is not a spendable contract balance. Settlement retains the selected token, root, mode, and precision even if execution changes the registry or the reserve is zero. Token arithmetic is checked; saturation is confined to explicitly approximate pool aggregate reporting. RPC capacity uses a wide product and divides before applying its gas cap.

System reads/writes are journaled but do not warm accounts or slots. Nonzero writes introduce a rollback-safe touch so they persist on commit. Existing warmth and touches are preserved. Every nonzero debit or credit revalidates the current slot's mode; zero amounts cause no destination reads, writes, touches, or privacy initialization. Direct fee writes bypass token hooks, pause/blacklist checks, and transfer events, which the registry owner must approve.

Ordinary bytecode revert/halt still settles fees. Transaction-level invalidity or database failure discards caller accounting, fee changes, body effects, and introduced touches. Pending body database errors are surfaced before settlement can mask them. Cleanup clears both the context error and reserve so a retained journal can process the next transaction safely.

With disabled balance checks, every selector skips fee affordability, registry/balance reads, deductions, refunds, rewards, and reserve creation. Caller nonce and introduced touch remain journaled; ordinary execution and value-transfer rules still apply. This internal configuration is not a wallet fee exemption.

## Verification

```sh
cargo test -p seismic-revm
cargo test -p revm-context test_system_access
cargo clippy -p seismic-revm --lib --tests --no-deps -- -D warnings \
  -W clippy::unwrap_used -W clippy::expect_used -W clippy::indexing_slicing \
  -W clippy::panic -W clippy::unreachable -W clippy::todo
```

Real execution tests cover every precision in both modes, ordinary and inspected execution, successful/reverted execution, same-address settlement, database commit/visibility, non-warming fee access, beneficiary burns, same-block registry changes, zero fees, typed mode failures, required-read failures, CSTORE propagation, and retained-journal rollback/cleanup. The optional balance-check switch is enabled only in test dependencies to exercise the disabled-check branch.
