use revm::{
    context_interface::transaction::Transaction,
    precompile::{u64_to_address, PrecompileError, PrecompileOutput, PrecompileResult},
    primitives::{Bytes, U256},
};

use crate::{
    api::exec::SeismicContextTr, precompiles::stateful_precompile::StatefulPrecompileWithAddress,
    transaction::abstraction::SeismicTxTr,
};

// Exposes read-only transaction-context flags so a contract can detect a Seismic execution context
// without a dedicated opcode. The 1-byte input selects the field; output is a 32-byte big-endian
// word (abi `uint256`):
//   (empty)  -> EIP-2718 tx type (74 = Seismic)
//   [0x01]   -> signed_read flag (1 = authenticated signed read, 0 otherwise)
// Any other input is rejected, keeping the selector space open for future fields.
pub const TX_CONTEXT_ADDRESS: u64 = 106; // Hex address `0x6A`.

// Selector for the signed-read flag. An empty input keeps returning the tx type (backward compat).
pub const SIGNED_READ_SELECTOR: u8 = 0x01;

pub const SEISMIC_TX_TYPE: u8 = 74;

// Flat cost for a single transaction-context read. NOTE: this is a consensus parameter — changing
// it after activation is itself a hardfork.
pub const TX_CONTEXT_GAS_COST: u64 = 20;

pub fn tx_context_precompile<CTX: SeismicContextTr>() -> StatefulPrecompileWithAddress<CTX> {
    StatefulPrecompileWithAddress(u64_to_address(TX_CONTEXT_ADDRESS), tx_context::<CTX>)
}

fn tx_context<CTX: SeismicContextTr>(
    evmctx: &mut CTX,
    input: &Bytes,
    gas_limit: u64,
) -> PrecompileResult {
    if gas_limit < TX_CONTEXT_GAS_COST {
        return Err(PrecompileError::OutOfGas);
    }
    // The input selects which context field to read; unknown selectors are rejected so the ABI
    // stays extensible (and so nodes without a given selector fail closed rather than return 0).
    let value: u64 = match input.as_ref() {
        [] => evmctx.tx().tx_type() as u64,
        [SIGNED_READ_SELECTOR] => {
            (evmctx.tx().signed_read() && evmctx.tx().tx_type() == SEISMIC_TX_TYPE) as u64
        }
        _ => {
            return Err(PrecompileError::Other(
                "tx-context precompile: unknown selector".to_string(),
            ))
        }
    };

    let output = U256::from(value).to_be_bytes::<32>();
    Ok(PrecompileOutput::new(
        TX_CONTEXT_GAS_COST,
        Bytes::copy_from_slice(&output),
    ))
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic
    )]

    use super::*;
    use crate::{DefaultSeismicContext, SeismicContext};
    use revm::{database::EmptyDB, primitives::Bytes, Context};

    fn ctx_with_tx_type(tx_type: u8) -> SeismicContext<EmptyDB> {
        Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.tx_type = tx_type;
        })
    }

    fn ctx_with_signed_read(signed_read: bool) -> SeismicContext<EmptyDB> {
        Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.tx_type = 74; // a signed read is always Seismic-typed
            tx.signed_read = signed_read;
        })
    }

    #[test]
    fn tx_type_returns_type_as_uint256() {
        for ty in [0u8, 1, 2, 74, 255] {
            let mut ctx = ctx_with_tx_type(ty);
            let out =
                tx_context_precompile::<SeismicContext<EmptyDB>>().1(&mut ctx, &Bytes::new(), 1000)
                    .unwrap();
            assert_eq!(out.gas_used, TX_CONTEXT_GAS_COST);
            assert_eq!(out.bytes.len(), 32, "must be a 32-byte abi uint256");
            assert_eq!(out.bytes[31], ty, "low byte is the tx type ({ty})");
            assert!(
                out.bytes[..31].iter().all(|b| *b == 0),
                "upper bytes must be zero"
            );
        }
    }

    #[test]
    fn signed_read_selector_returns_flag() {
        for sr in [false, true] {
            let mut ctx = ctx_with_signed_read(sr);
            let out = tx_context_precompile::<SeismicContext<EmptyDB>>().1(
                &mut ctx,
                &Bytes::from_static(&[SIGNED_READ_SELECTOR]),
                1000,
            )
            .unwrap();
            assert_eq!(out.gas_used, TX_CONTEXT_GAS_COST);
            assert_eq!(out.bytes.len(), 32, "must be a 32-byte abi uint256");
            assert_eq!(
                out.bytes[31], sr as u8,
                "low byte is the signed_read flag ({sr})"
            );
            assert!(
                out.bytes[..31].iter().all(|b| *b == 0),
                "upper bytes must be zero"
            );
        }
    }

    #[test]
    fn signed_read_requires_seismic_type() {
        for ty in [0u8, 1, 2, 255] {
            let mut ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
                tx.base.tx_type = ty;
                tx.signed_read = true;
            });
            let out = tx_context_precompile::<SeismicContext<EmptyDB>>().1(
                &mut ctx,
                &Bytes::from_static(&[SIGNED_READ_SELECTOR]),
                1000,
            )
            .unwrap();
            assert_eq!(
                out.bytes[31], 0,
                "signed_read must be 0 for non-Seismic type {ty}"
            );
        }
    }

    #[test]
    fn unknown_selector_rejected() {
        let mut ctx = ctx_with_tx_type(74);
        // An unknown 1-byte selector and any multi-byte input are both rejected (fail-closed).
        for bad in [vec![0x00u8], vec![0x02], b"arbitrary input".to_vec()] {
            let res = tx_context_precompile::<SeismicContext<EmptyDB>>().1(
                &mut ctx,
                &Bytes::from(bad),
                1000,
            );
            assert!(
                matches!(res, Err(PrecompileError::Other(_))),
                "unknown selector must be rejected"
            );
        }
    }

    #[test]
    fn tx_type_charges_exactly_base_cost() {
        let mut ctx = ctx_with_tx_type(74);
        // Exact threshold succeeds; one below is OutOfGas.
        assert!(tx_context_precompile::<SeismicContext<EmptyDB>>().1(
            &mut ctx,
            &Bytes::new(),
            TX_CONTEXT_GAS_COST
        )
        .is_ok());
        let res = tx_context_precompile::<SeismicContext<EmptyDB>>().1(
            &mut ctx,
            &Bytes::new(),
            TX_CONTEXT_GAS_COST - 1,
        );
        assert!(matches!(res, Err(PrecompileError::OutOfGas)));
    }
}
