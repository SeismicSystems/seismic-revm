use crate::{check, SeismicHaltReason, SeismicHost};
use revm::interpreter::{
    gas::{self, CALL_STIPEND}, interpreter_types::{InputsTr, InterpreterTypes, LoopControl, RuntimeFlag, StackTr}, popn, popn_top, require_non_staticcall, Host, Instruction, InstructionContext, InstructionResult, Interpreter
};
use revm::primitives::hardfork::SpecId::*;

pub fn cload<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>(
    context: InstructionContext<'_, H, WIRE>
) {
    check!(context.interpreter, MERCURY);
    popn_top!([], index, context.interpreter);

    if let Some(value) = context.host.cload(context.interpreter.input.target_address(), *index) {
        if !value.is_private && !value.data.is_zero() {
            context.interpreter
                .halt_fatal();
            context.host.set_halt_reason(SeismicHaltReason::InvalidPublicStorageAccess);
            return;
        }
        // gas!(
        //     context.interpreter,
        //     gas::sload_cost(interpreter.runtime_flag.spec_id(), value.is_cold)
        // );
        *index = value.data;
    } else {
        context.interpreter
            .halt_fatal();
        return;
    }
}

pub fn cstore<WIRE: InterpreterTypes, H: Host + ?Sized>(
    context: InstructionContext<'_, H, WIRE>
) {
    check!(context.interpreter, MERCURY);
    require_non_staticcall!(context.interpreter);
    popn!([index, value], context.interpreter);

    let Some(state_load) = context.host.cstore(context.interpreter.input.target_address(), index, value) else {
        context.interpreter
            .halt_fatal();
        return;
    };

    // EIP-1706 Disable SSTORE with gasleft lower than call stipend
    if context.interpreter.runtime_flag.spec_id().is_enabled_in(ISTANBUL)
        && context.interpreter.gas.remaining() <= CALL_STIPEND
    {
        context.interpreter
            .halt(InstructionResult::ReentrancySentryOOG);
        return;
    }
    // gas!(
    //     context.interpreter,
    //     gas::sstore_cost(
    //         context.interpreter.runtime_flag.spec_id(),
    //         &state_load.data,
    //         state_load.is_cold
    //     )
    // );

    context.interpreter
        .gas
        .record_refund(gas::sstore_refund(
            context.interpreter.runtime_flag.spec_id(),
            &state_load.data,
        ));
}

pub fn sload<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>(
    context: InstructionContext<'_, H, WIRE>
) {
    popn_top!([], index, context.interpreter);

    if let Some(value) = context.host.sload(context.interpreter.input.target_address(), *index) {
        if value.is_private {
            context.interpreter
                .halt_fatal();
            context.host.set_halt_reason(SeismicHaltReason::InvalidPrivateStorageAccess);
            return;
        }
        // gas!(
        //     context.interpreter,
        //     gas::sload_cost(interpreter.runtime_flag.spec_id(), value.is_cold)
        // );
        *index = value.data;
    } else {
        context.interpreter.halt_fatal();
        return;
    }
}

pub fn sstore<WIRE: InterpreterTypes, H: Host + ?Sized>(
    context: InstructionContext<'_, H, WIRE>
) {
    require_non_staticcall!(context.interpreter);

    popn!([index, value], context.interpreter);

    let Some(state_load) = context.host.sstore(context.interpreter.input.target_address(), index, value.into())
    else {
        context.interpreter.halt_fatal();
        return;
    };

    // EIP-1706 Disable SSTORE with gasleft lower than call stipend
    if context.interpreter.runtime_flag.spec_id().is_enabled_in(ISTANBUL)
        && context.interpreter.gas.remaining() <= CALL_STIPEND
    {
        context.interpreter.halt(InstructionResult::ReentrancySentryOOG);
        return;
    }
    // gas!(
    //     context.interpreter,
    //     gas::sstore_cost(
    //         context.interpreter.runtime_flag.spec_id(),
    //         &state_load.data,
    //         state_load.is_cold
    //     )
    // );

    context.interpreter
        .gas
        .record_refund(gas::sstore_refund(
            context.interpreter.runtime_flag.spec_id(),
            &state_load.data,
        ));
}

// NOTE: static_gas is 0 for these, because gas is dynamic
pub fn cload_instruction<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>() -> Instruction<WIRE, H> {    
    Instruction::new(cload, 0)
}

pub fn cstore_instruction<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>() -> Instruction<WIRE, H> {
    Instruction::new(cstore, 0)
}

pub fn seismic_sload_instruction<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>() -> Instruction<WIRE, H> {
    Instruction::new(sload, 0)
}

pub fn seismic_sstore_instruction<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>() -> Instruction<WIRE, H> {
    Instruction::new(sstore, 0)
}

#[cfg(test)]
mod tests {
    use crate::instructions::seismic_host::SeismicDummyHost;

    use super::*;
    use revm::interpreter::interpreter::{EthInterpreter, ExtBytecode};
    use revm::interpreter::{CallInput, InputsImpl, SharedMemory};
    use revm::interpreter::{InstructionResult, Interpreter};
    use revm::primitives::hardfork::SpecId;
    use revm::primitives::{Address, Bytes, U256};
    use revm::state::Bytecode;

    // Helper to build an interpreter with a given SpecId.
    fn build_interpreter(spec_id: SpecId, bytecode: Bytecode) -> Interpreter<EthInterpreter> {
        let interp = Interpreter::<EthInterpreter>::new(
            SharedMemory::new(),
            ExtBytecode::new(bytecode),
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
        );
        interp
    }

    #[test]
    fn test_cload_before_mercury() {
        // SpecId < PRAGUE => Mercury check should fail => NotActivated
        let bytecode = Bytecode::new_raw(Bytes::from(&[0x60, 0x00, 0x60, 0x00, 0x01][..]));
        let mut host = SeismicDummyHost::new();
        let mut interpreter = build_interpreter(SpecId::LONDON, bytecode);
        let context = InstructionContext {
            interpreter: &mut interpreter,
            host: &mut host,
        };

        cload(context);

        assert_eq!(
            interpreter.bytecode.instruction_result(),
            Some(InstructionResult::NotActivated)
        );
    }

    #[test]
    fn test_cstore_mercury_or_later() {
        // SpecId >= PRAGUE => Mercury is "enabled", so it shouldn't fail at the macro check
        let mut host = SeismicDummyHost::new();

        let bytecode = Bytecode::new_raw(Bytes::from(&[0x00][..]));
        let mut interpreter = build_interpreter(SpecId::PRAGUE, bytecode);
        let context = InstructionContext {
            interpreter: &mut interpreter,
            host: &mut host,
        };

        //60 2A          PUSH1 0x2A    ; push decimal 42 as "value"
        //60 0A          PUSH1 0x0A    ; push decimal 10 as "index"
        //0xB1           CSTORE        ; CSTORE
        let _ = context.interpreter.stack.push(U256::from(0x0A)); // index
        let _ = context.interpreter.stack.push(U256::from(0x2A)); // value
        cstore(context);

        assert_ne!(
            interpreter.bytecode.instruction_result(),
            Some(InstructionResult::NotActivated)
        );

        //Should get Fatal External Error given DummyHost returns None
        assert_eq!(
            interpreter.bytecode.instruction_result(),
            Some(InstructionResult::FatalExternalError)
        );
    }

    #[test]
    fn test_cstore_before_mercury() {
        let bytecode = Bytecode::new_raw(Bytes::from(&[0x60, 0x00, 0x60, 0x00, 0x01][..]));
        let mut host = SeismicDummyHost::new();
        let mut interpreter = build_interpreter(SpecId::LONDON, bytecode);
        let context = InstructionContext {
            interpreter: &mut interpreter,
            host: &mut host,
        };
        cstore(context);

        assert_eq!(
            interpreter.bytecode.instruction_result(),
            Some(InstructionResult::NotActivated)
        );
    }

    #[test]
    fn test_cload_mercury_or_later() {
        // SpecId >= PRAGUE => Mercury is "enabled", so it shouldn't fail at the macro check
        let mut host = SeismicDummyHost::new();

        let bytecode = Bytecode::new_raw(Bytes::from(&[0x00][..]));
        let mut interpreter = build_interpreter(SpecId::PRAGUE, bytecode);
        let context = InstructionContext {
            interpreter: &mut interpreter,
            host: &mut host,
        };

        //60 0A          PUSH1 0x0A    ; push decimal 10 as "index"
        //0xB            CLOAD         ; CLOAD
        let _ = context.interpreter.stack.push(U256::from(0x0A)); // index
        cload(context);

        assert_ne!(
            interpreter.bytecode.instruction_result(),
            Some(InstructionResult::NotActivated)
        );

        //Should get Fatal External Error given DummyHost returns None
        assert_eq!(
            interpreter.bytecode.instruction_result(),
            Some(InstructionResult::FatalExternalError)
        );
    }
}
