# Seismic block timestamps

Seismic uses standard Ethereum seconds for `BlockEnv.timestamp` and `TIMESTAMP`
(`0x42`). Sub-second precision lives in `SeismicBlockEnv.timestamp_millis_part`.
Valid block headers supply a part in `0..1000`; header and Engine API validation
are responsible for enforcing that range.

`SeismicBlockEnv` wraps the standard `BlockEnv`, delegates the `Block` trait to
it, and exposes `timestamp_millis()` as `seconds * 1000 + part` (saturating at
`U256::MAX`). The Seismic instruction table uses this full value for the existing
`TIMESTAMPMS` opcode (`0x4B`, gas cost 2). Normal transactions, inspected execution,
and system calls share the same instruction table and block environment.

## Constructing an environment

```rust
use revm::{context::BlockEnv, primitives::U256};
use seismic_revm::SeismicBlockEnv;

let block = SeismicBlockEnv {
    inner: BlockEnv {
        timestamp: U256::from(1_700_000_000u64),
        ..Default::default()
    },
    timestamp_millis_part: 123,
};
assert_eq!(block.timestamp_millis(), U256::from(1_700_000_000_123u64));
```

Default Seismic contexts use `SeismicBlockEnv`. An existing standard environment
can be converted with `SeismicBlockEnv::from(block_env)`; this assigns a zero
millisecond part and does not change the seconds timestamp. Execution clients
must explicitly populate the part from the header or payload attributes.

The `timestamp-in-seconds` features of `revm`, `revm-interpreter`, and
`seismic-revm` have been removed. Seconds are now the only standard timestamp
interpretation; consumers must remove those feature flags when updating their
pins. Standard, non-Seismic instruction tables retain a whole-second
`TIMESTAMPMS` conversion; exact sub-second execution requires a Seismic context.

## Beacon roots

Seismic's genesis beacon-roots contract indexes entries with `TIMESTAMPMS`, not
`TIMESTAMP`. Supplying the millisecond part to the EVM keeps roots for consecutive
same-second blocks distinct. Consumers querying the contract must pass the full
millisecond timestamp, not the RPC header's seconds field alone.

The execution client and RPC consumers still need to be updated separately;
this crate cannot infer a missing millisecond part from a seconds timestamp.
