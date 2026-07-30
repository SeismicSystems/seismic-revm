use revm::{
    context_interface::transaction::Transaction,
    precompile::{u64_to_address, PrecompileError, PrecompileOutput, PrecompileResult},
    primitives::{Bytes, U256},
};

use crate::{
    api::exec::SeismicContextTr, precompiles::stateful_precompile::StatefulPrecompileWithAddress,
};

// Returns the current transaction's EIP-2718 type byte (74 = Seismic) so a contract can detect a
// Seismic execution context without a dedicated opcode. Takes no input (non-empty calldata is
// rejected); output is the type as a 32-byte big-endian word (abi `uint256`).
pub const TX_TYPE_ADDRESS: u64 = 106; // Hex address `0x6A`.

// Flat cost for a single transaction-context read. NOTE: this is a consensus parameter — changing
// it after activation is itself a hardfork.
pub const TX_TYPE_GAS_COST: u64 = 20;

pub fn tx_type_precompile<CTX: SeismicContextTr>() -> StatefulPrecompileWithAddress<CTX> {
    StatefulPrecompileWithAddress(u64_to_address(TX_TYPE_ADDRESS), tx_type::<CTX>)
}

fn tx_type<CTX: SeismicContextTr>(
    evmctx: &mut CTX,
    input: &Bytes,
    gas_limit: u64,
) -> PrecompileResult {
    if gas_limit < TX_TYPE_GAS_COST {
        return Err(PrecompileError::OutOfGas);
    }
    // Takes no input. Reject non-empty calldata so a future versioned/selector ABI stays open.
    if !input.is_empty() {
        return Err(PrecompileError::Other(
            "tx-type precompile takes no input".to_string(),
        ));
    }

    let output = U256::from(evmctx.tx().tx_type()).to_be_bytes::<32>();
    Ok(PrecompileOutput::new(
        TX_TYPE_GAS_COST,
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

    #[test]
    fn tx_type_returns_type_as_uint256() {
        for ty in [0u8, 1, 2, 74, 255] {
            let mut ctx = ctx_with_tx_type(ty);
            let out =
                tx_type_precompile::<SeismicContext<EmptyDB>>().1(&mut ctx, &Bytes::new(), 1000)
                    .unwrap();
            assert_eq!(out.gas_used, TX_TYPE_GAS_COST);
            assert_eq!(out.bytes.len(), 32, "must be a 32-byte abi uint256");
            assert_eq!(out.bytes[31], ty, "low byte is the tx type ({ty})");
            assert!(
                out.bytes[..31].iter().all(|b| *b == 0),
                "upper bytes must be zero"
            );
        }
    }

    #[test]
    fn tx_type_rejects_nonempty_input() {
        let mut ctx = ctx_with_tx_type(74);
        let res = tx_type_precompile::<SeismicContext<EmptyDB>>().1(
            &mut ctx,
            &Bytes::from_static(b"arbitrary input"),
            1000,
        );
        assert!(
            matches!(res, Err(PrecompileError::Other(_))),
            "non-empty input must be rejected"
        );
    }

    #[test]
    fn tx_type_charges_exactly_base_cost() {
        let mut ctx = ctx_with_tx_type(74);
        // Exact threshold succeeds; one below is OutOfGas.
        assert!(tx_type_precompile::<SeismicContext<EmptyDB>>().1(
            &mut ctx,
            &Bytes::new(),
            TX_TYPE_GAS_COST
        )
        .is_ok());
        let res = tx_type_precompile::<SeismicContext<EmptyDB>>().1(
            &mut ctx,
            &Bytes::new(),
            TX_TYPE_GAS_COST - 1,
        );
        assert!(matches!(res, Err(PrecompileError::OutOfGas)));
    }
}
