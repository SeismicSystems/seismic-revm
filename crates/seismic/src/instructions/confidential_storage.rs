use revm::interpreter::{gas::CALL_STIPEND, interpreter_types::{InputsTr, InterpreterTypes, LoopControl, RuntimeFlag, StackTr}, popn, popn_top, require_non_staticcall, gas, Host, InstructionResult, Interpreter};
use crate::check;
use revm::primitives::hardfork::SpecId::*;

pub fn cload<WIRE: InterpreterTypes, H: Host + ?Sized>(
    interpreter: &mut Interpreter<WIRE>,
    host: &mut H,
) {
    check!(interpreter, MERCURY);
    popn_top!([], index, interpreter);

    if let Some(value) = host.cload(interpreter.input.target_address(), *index) {
        if !value.is_private && !value.data.is_zero() {
            interpreter
            .control
            .set_instruction_result(InstructionResult::InvalidPublicStorageAccess);
            return
        }
        gas!(
            interpreter,
            gas::sload_cost(interpreter.runtime_flag.spec_id(), value.is_cold)
        );
        *index = value.data;
    } else {
        interpreter
        .control
        .set_instruction_result(InstructionResult::FatalExternalError);
        return
    }
}

pub fn cstore<WIRE: InterpreterTypes, H: Host + ?Sized>(
    interpreter: &mut Interpreter<WIRE>,
    host: &mut H,
) {
    check!(interpreter, MERCURY);
    require_non_staticcall!(interpreter);
    popn!([index, value], interpreter);

    let Some(state_load) = host.cstore(interpreter.input.target_address(), index, value) else {
        interpreter
            .control
            .set_instruction_result(InstructionResult::FatalExternalError);
        return;
    };

    // EIP-1706 Disable SSTORE with gasleft lower than call stipend
    if interpreter.runtime_flag.spec_id().is_enabled_in(ISTANBUL)
        && interpreter.control.gas().remaining() <= CALL_STIPEND
    {
        interpreter
            .control
            .set_instruction_result(InstructionResult::ReentrancySentryOOG);
        return;
    }
    gas!(
        interpreter,
        gas::sstore_cost(
            interpreter.runtime_flag.spec_id(),
            &state_load.data,
            state_load.is_cold
        )
    );

    interpreter
        .control
        .gas_mut()
        .record_refund(gas::sstore_refund(
            interpreter.runtime_flag.spec_id(),
            &state_load.data,
        ));
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use revm::interpreter::{InputsImpl, SharedMemory};
    use revm::primitives::{Address, Bytes, U256};
    use revm::interpreter::host::DummyHost;
    use revm::interpreter::interpreter::{EthInterpreter, ExtBytecode};
    use revm::interpreter::{
        InstructionResult, Interpreter,  
    };
    use revm::primitives::hardfork::SpecId;
    use revm::state::Bytecode;

    // Helper to build an interpreter with a given SpecId.
    fn build_interpreter(spec_id: SpecId, bytecode: Bytecode) -> Interpreter<EthInterpreter> {
        let interp = Interpreter::<EthInterpreter>::new(
            Rc::new(RefCell::new(SharedMemory::new())),
            ExtBytecode::new(bytecode),
            InputsImpl {
                target_address: Address::ZERO,
                caller_address: Address::ZERO,
                input: Bytes::default(),
                call_value: U256::ZERO,
            },
            false,
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
        let mut host = DummyHost;
        let mut interpreter = build_interpreter(SpecId::LONDON, bytecode);

        cload(&mut interpreter, &mut host);

        assert_eq!(
            interpreter.control.instruction_result(),
            InstructionResult::NotActivated
        );
    }

    #[test]
    fn test_cstore_mercury_or_later() {
        // SpecId >= PRAGUE => Mercury is "enabled", so it shouldn't fail at the macro check
        let mut host = DummyHost;

        let bytecode = Bytecode::new_raw(Bytes::from(&[0x00][..]));
        let mut interpreter = build_interpreter(SpecId::PRAGUE, bytecode);
        
        //60 2A          PUSH1 0x2A    ; push decimal 42 as "value"
        //60 0A          PUSH1 0x0A    ; push decimal 10 as "index"
        //0xB1           CSTORE        ; CSTORE 
        let _ = interpreter.stack.push(U256::from(0x0A)); // index
        let _ = interpreter.stack.push(U256::from(0x2A)); // value
        cstore(&mut interpreter, &mut host);

        assert_ne!(
            interpreter.control.instruction_result(),
            InstructionResult::NotActivated
        );

        //Should get Fatal External Error given DummyHost returns None
        assert_eq!(
            interpreter.control.instruction_result(),
            InstructionResult::FatalExternalError
        );
    }

    #[test]
    fn test_cstore_before_mercury() {
        let bytecode = Bytecode::new_raw(Bytes::from(&[0x60, 0x00, 0x60, 0x00, 0x01][..]));
        let mut host = DummyHost;
        let mut interpreter = build_interpreter(SpecId::LONDON, bytecode);

        cstore(&mut interpreter, &mut host);

        assert_eq!(
            interpreter.control.instruction_result(),
            InstructionResult::NotActivated
        );
    }

    #[test]
    fn test_cload_mercury_or_later() {
        // SpecId >= PRAGUE => Mercury is "enabled", so it shouldn't fail at the macro check
        let mut host = DummyHost;

        let bytecode = Bytecode::new_raw(Bytes::from(&[0x00][..]));
        let mut interpreter = build_interpreter(SpecId::PRAGUE, bytecode);
        
        //60 0A          PUSH1 0x0A    ; push decimal 10 as "index"
        //0xB            CLOAD         ; CLOAD 
        let _ = interpreter.stack.push(U256::from(0x0A)); // index
        cload(&mut interpreter, &mut host);

        assert_ne!(
            interpreter.control.instruction_result(),
            InstructionResult::NotActivated
        );

        //Should get Fatal External Error given DummyHost returns None
        assert_eq!(
            interpreter.control.instruction_result(),
            InstructionResult::FatalExternalError
        );
    }
}
