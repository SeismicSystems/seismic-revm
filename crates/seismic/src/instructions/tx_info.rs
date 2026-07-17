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
