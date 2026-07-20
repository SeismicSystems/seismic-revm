//! Instructions exposing transaction-level metadata to contract code.

use crate::{check, SeismicHost};
use revm::{
    interpreter::{
        gas,
        interpreter_types::{InterpreterTypes, RuntimeFlag, StackTr},
        push, Instruction, InstructionContext,
    },
    primitives::U256,
};

/// Implements the TXTYPE instruction (`0xB2`).
///
/// Pushes the EIP-2718 transaction-type byte of the currently executing
/// transaction onto the stack. A Seismic (encrypted-calldata) transaction has
/// type `74` (`0x4A`); standard transactions push their usual type (`0` legacy,
/// `1` EIP-2930, `2` EIP-1559, ...).
///
/// Zero inputs, one output. Gated to the Mercury hardfork.
pub fn txtype<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>(
    context: InstructionContext<'_, H, WIRE>,
) {
    check!(context.interpreter, MERCURY);
    gas!(context.interpreter, gas::BASE);
    push!(context.interpreter, U256::from(context.host.tx_type()));
}

// NOTE: static_gas is 0 because gas is charged dynamically inside the handler,
// matching the convention used by the other Seismic instructions.
pub fn txtype_instruction<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>() -> Instruction<WIRE, H>
{
    Instruction::new(txtype, 0)
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
    use crate::instructions::seismic_host::SeismicDummyHost;
    use revm::interpreter::interpreter::{EthInterpreter, ExtBytecode};
    use revm::interpreter::interpreter_types::LoopControl;
    use revm::interpreter::{CallInput, InputsImpl, InstructionResult, Interpreter, SharedMemory};
    use revm::primitives::hardfork::SpecId;
    use revm::primitives::{Address, Bytes};
    use revm::state::Bytecode;

    // Seismic transaction type (0x4A). Kept local to avoid a dependency on the
    // seismic-alloy consensus crate; the semantic `== 74` comparison lives in
    // the std-lib helper, not in the EVM.
    const SEISMIC_TX_TYPE_ID: u8 = 74;

    fn build_interpreter(spec_id: SpecId) -> Interpreter<EthInterpreter> {
        Interpreter::<EthInterpreter>::new(
            SharedMemory::new(),
            ExtBytecode::new(Bytecode::new_raw(Bytes::from(&[0x00][..]))),
            InputsImpl {
                target_address: Address::ZERO,
                caller_address: Address::ZERO,
                input: CallInput::Bytes(Bytes::default()),
                call_value: U256::ZERO,
                bytecode_address: None,
            },
            false,
            spec_id,
            u64::MAX,
        )
    }

    #[test]
    fn test_txtype_before_mercury() {
        // SpecId < Mercury => the check should fail with NotActivated.
        let mut host = SeismicDummyHost::new().with_tx_type(SEISMIC_TX_TYPE_ID);
        let mut interpreter = build_interpreter(SpecId::LONDON);
        let context = InstructionContext {
            interpreter: &mut interpreter,
            host: &mut host,
        };

        txtype(context);

        assert_eq!(
            interpreter.bytecode.instruction_result(),
            Some(InstructionResult::NotActivated)
        );
    }

    #[test]
    fn test_txtype_pushes_seismic_tx_type() {
        let mut host = SeismicDummyHost::new().with_tx_type(SEISMIC_TX_TYPE_ID);
        let mut interpreter = build_interpreter(SpecId::MERCURY);
        let context = InstructionContext {
            interpreter: &mut interpreter,
            host: &mut host,
        };

        txtype(context);

        assert_ne!(
            interpreter.bytecode.instruction_result(),
            Some(InstructionResult::NotActivated)
        );
        assert_eq!(
            interpreter.stack.pop().unwrap(),
            U256::from(SEISMIC_TX_TYPE_ID)
        );
    }

    #[test]
    fn test_txtype_pushes_standard_tx_type() {
        // A standard (EIP-1559) transaction reports its own type byte.
        let mut host = SeismicDummyHost::new().with_tx_type(2);
        let mut interpreter = build_interpreter(SpecId::MERCURY);
        let context = InstructionContext {
            interpreter: &mut interpreter,
            host: &mut host,
        };

        txtype(context);

        assert_eq!(interpreter.stack.pop().unwrap(), U256::from(2));
    }
}

/// End-to-end tests that run TXTYPE through the full Seismic EVM, exercising the
/// production `SeismicHost::tx_type` blanket impl (`ctx.tx().tx_type()`) — the
/// path the unit tests above do not cover, since they use a mock host.
#[cfg(test)]
mod e2e_tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic
    )]

    use crate::{DefaultSeismicContext, SeismicBuilder};
    use revm::{
        bytecode::opcode,
        database::{BenchmarkDB, BENCH_CALLER, BENCH_TARGET},
        primitives::{TxKind, U256},
        state::Bytecode,
        Context, ExecuteEvm,
    };

    use super::super::instruction_provider::TXTYPE;

    /// Deploys a contract that returns the current transaction type, runs a
    /// transaction of the given `tx_type` against it, and returns the value the
    /// contract observed via the TXTYPE opcode.
    fn observed_tx_type(tx_type: u8) -> U256 {
        // TXTYPE; PUSH1 0; MSTORE; PUSH1 32; PUSH1 0; RETURN
        // -> stores the tx-type byte at mem[0..32] and returns it.
        let code = [
            TXTYPE,
            opcode::PUSH1,
            0x00,
            opcode::MSTORE,
            opcode::PUSH1,
            0x20,
            opcode::PUSH1,
            0x00,
            opcode::RETURN,
        ];

        // seismic_with_random_rng_key() defaults the spec to Mercury, so TXTYPE
        // is activated.
        let ctx = Context::seismic_with_random_rng_key()
            .modify_tx_chained(|tx| {
                tx.base.tx_type = tx_type;
                tx.base.kind = TxKind::Call(BENCH_TARGET);
                tx.base.caller = BENCH_CALLER;
                tx.base.gas_limit = 100_000;
            })
            .with_db(BenchmarkDB::new_bytecode(Bytecode::new_legacy(
                code.to_vec().into(),
            )));

        let mut evm = ctx.build_seismic_evm();
        let out = evm.replay().unwrap();
        assert!(
            out.result.is_success(),
            "execution should succeed: {:#?}",
            out.result
        );
        U256::from_be_slice(out.result.into_output().unwrap().as_ref())
    }

    #[test]
    fn txtype_reports_seismic_type_end_to_end() {
        // Seismic (encrypted-calldata) transaction: type 74 (0x4A).
        assert_eq!(observed_tx_type(74), U256::from(74));
    }

    #[test]
    fn txtype_reports_standard_types_end_to_end() {
        // Standard transaction types flow through unchanged.
        assert_eq!(observed_tx_type(0), U256::from(0)); // legacy
        assert_eq!(observed_tx_type(2), U256::from(2)); // EIP-1559
    }
}
