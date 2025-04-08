use revm::{
    interpreter::{interpreter_types::{InputsTr, InterpreterTypes, LoopControl, RuntimeFlag, StackTr}, popn, popn_top, require_non_staticcall, gas, Interpreter},
    interpreter::Host, interpreter::InstructionResult, interpreter::gas::CALL_STIPEND,
};
use revm::primitives::hardfork::SpecId::*;

pub fn cload<WIRE: InterpreterTypes, H: Host + ?Sized>(
    interpreter: &mut Interpreter<WIRE>,
    host: &mut H,
) {
    popn_top!([], index, interpreter);

    if let Some(value) = host.cload(interpreter.input.target_address(), *index) {
        if !value.is_private {
            interpreter
            .control
            .set_instruction_result(InstructionResult::InvalidPrivateStorageAccess);
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

