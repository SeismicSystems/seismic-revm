//! Seismic-specific block information instructions.

use crate::SeismicHost;
use revm::interpreter::{
    interpreter_types::{InterpreterTypes, StackTr},
    push, InstructionContext,
};

/// Pushes the full Unix millisecond block timestamp for `TIMESTAMPMS` (`0x4B`).
pub fn timestamp_milliseconds<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>(
    context: InstructionContext<'_, H, WIRE>,
) {
    push!(context.interpreter, context.host.timestamp_millis());
}
