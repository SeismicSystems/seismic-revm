use revm::{
    context_interface::transaction::Transaction,
    precompile::{u64_to_address, PrecompileError, PrecompileOutput, PrecompileResult},
    primitives::Bytes,
};

use crate::{
    api::exec::SeismicContextTr, precompiles::stateful_precompile::StatefulPrecompileWithAddress,
};

// Returns the current transaction's EIP-2718 type byte (74 = Seismic) so a contract can detect a
// Seismic execution context without a dedicated opcode. Input is ignored; output is the type as a
// 32-byte big-endian word (abi `uint256`).
pub const TX_INFO_ADDRESS: u64 = 106; // Hex address `0x6A`.

// Flat cost for a single transaction-context read.
pub const TX_INFO_BASE_COST: u64 = 20;

pub fn tx_info_precompile<CTX: SeismicContextTr>() -> StatefulPrecompileWithAddress<CTX> {
    StatefulPrecompileWithAddress(u64_to_address(TX_INFO_ADDRESS), tx_info::<CTX>)
}

fn tx_info<CTX: SeismicContextTr>(
    evmctx: &mut CTX,
    _input: &Bytes,
    gas_limit: u64,
) -> PrecompileResult {
    if gas_limit < TX_INFO_BASE_COST {
        return Err(PrecompileError::OutOfGas);
    }

    let mut out = [0u8; 32];
    out[31] = evmctx.tx().tx_type();
    Ok(PrecompileOutput::new(
        TX_INFO_BASE_COST,
        Bytes::copy_from_slice(&out),
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
    fn tx_info_returns_tx_type_as_uint256() {
        for ty in [0u8, 1, 2, 74, 255] {
            let mut ctx = ctx_with_tx_type(ty);
            let out =
                tx_info_precompile::<SeismicContext<EmptyDB>>().1(&mut ctx, &Bytes::new(), 1000)
                    .unwrap();
            assert_eq!(out.gas_used, TX_INFO_BASE_COST);
            assert_eq!(out.bytes.len(), 32, "must be a 32-byte abi uint256");
            assert_eq!(out.bytes[31], ty, "low byte is the tx type ({ty})");
            assert!(
                out.bytes[..31].iter().all(|b| *b == 0),
                "upper bytes must be zero"
            );
        }
    }

    #[test]
    fn tx_info_ignores_input() {
        let mut ctx = ctx_with_tx_type(74);
        let out = tx_info_precompile::<SeismicContext<EmptyDB>>().1(
            &mut ctx,
            &Bytes::from_static(b"arbitrary input"),
            1000,
        )
        .unwrap();
        assert_eq!(out.bytes[31], 74);
    }

    #[test]
    fn tx_info_out_of_gas() {
        let mut ctx = ctx_with_tx_type(74);
        let res = tx_info_precompile::<SeismicContext<EmptyDB>>().1(
            &mut ctx,
            &Bytes::new(),
            TX_INFO_BASE_COST - 1,
        );
        assert!(matches!(res, Err(PrecompileError::OutOfGas)));
    }
}
