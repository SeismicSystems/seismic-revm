use revm::interpreter::{
    interpreter_types::{InterpreterTypes, StackTr},
    push, Host, Instruction, InstructionContext,
};

/// Implements the TIMESTAMP_MS instruction.
///
/// Pushes the current block's timestamp in milliseconds onto the stack.
pub fn timestamp_milliseconds<WIRE: InterpreterTypes, H: Host + ?Sized>(
    context: InstructionContext<'_, H, WIRE>,
) {
    //gas!(context.interpreter, gas::BASE);

    #[cfg(feature = "timestamp-in-seconds")]
    {
        use revm::primitives::U256;
        // Host returns seconds, convert to milliseconds for reth compatibility
        let timestamp_ms = context.host.timestamp() * U256::from(1000);
        push!(context.interpreter, timestamp_ms);
    }

    #[cfg(not(feature = "timestamp-in-seconds"))]
    {
        // Host returns milliseconds, use as is
        push!(context.interpreter, context.host.timestamp());
    }
}

pub fn timestampms_instruction<WIRE: InterpreterTypes, H: Host + ?Sized>() -> Instruction<WIRE, H>
{
    Instruction::new(timestamp_milliseconds, 2)
}
