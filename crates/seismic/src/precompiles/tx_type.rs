//! Transaction-type precompile.

use revm::{
    context_interface::transaction::Transaction,
    precompile::{u64_to_address, PrecompileError, PrecompileOutput, PrecompileResult},
    primitives::{Bytes, U256},
};

use crate::{
    api::exec::SeismicContextTr, precompiles::stateful_precompile::StatefulPrecompileWithAddress,
};

/// Address of the transaction-type precompile (`0x65`).
pub const TX_TYPE_ADDRESS: u64 = 101;

/// Fixed gas cost for querying the current transaction type.
pub const TX_TYPE_GAS_COST: u64 = 100;

/// Returns the transaction-type precompile at [`TX_TYPE_ADDRESS`].
pub fn tx_type_precompile<CTX: SeismicContextTr>() -> StatefulPrecompileWithAddress<CTX> {
    StatefulPrecompileWithAddress(u64_to_address(TX_TYPE_ADDRESS), tx_type::<CTX>)
}

/// Returns the current transaction type as an ABI-encoded `uint256`.
fn tx_type<CTX: SeismicContextTr>(
    context: &mut CTX,
    input: &Bytes,
    gas_limit: u64,
) -> PrecompileResult {
    if !input.is_empty() {
        return Err(PrecompileError::Other(
            "transaction-type precompile accepts no input".into(),
        ));
    }
    if gas_limit < TX_TYPE_GAS_COST {
        return Err(PrecompileError::OutOfGas);
    }

    let output = Bytes::copy_from_slice(&U256::from(context.tx().tx_type()).to_be_bytes::<32>());
    Ok(PrecompileOutput::new(TX_TYPE_GAS_COST, output))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use crate::{DefaultSeismicContext, SeismicContext};
    use revm::{context::TxEnv, database::EmptyDB, Context};

    #[test]
    fn returns_the_context_transaction_type() {
        let tx = TxEnv {
            tx_type: 74,
            ..Default::default()
        };
        let mut context = Context::seismic_with_random_rng_key().with_tx(tx.into());

        let output = tx_type::<SeismicContext<EmptyDB>>(&mut context, &Bytes::new(), 100).unwrap();

        assert_eq!(output.gas_used, TX_TYPE_GAS_COST);
        assert_eq!(
            output.bytes,
            Bytes::copy_from_slice(&U256::from(74).to_be_bytes::<32>())
        );
    }

    #[test]
    fn rejects_input_and_insufficient_gas() {
        let mut context = Context::seismic_with_random_rng_key();

        assert!(tx_type::<SeismicContext<EmptyDB>>(&mut context, &Bytes::from([0]), 100).is_err());
        assert!(tx_type::<SeismicContext<EmptyDB>>(&mut context, &Bytes::new(), 99).is_err());
    }
}
