//! NoValueTransferInspector. Helper Inspector to prevent value transfers during EVM execution
use crate::inspector::Inspector;
use interpreter::{
    CallInputs, CallOutcome, CreateInputs, CreateOutcome, Gas, InterpreterResult, InterpreterTypes,
};
use primitives::{Bytes, U256};

/// Helper to prevent value transfers during EVM execution
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoValueTransferInspector;

impl<CTX, INTR: InterpreterTypes> Inspector<CTX, INTR> for NoValueTransferInspector {
    /// Prevent value transfers in CALL instructions
    fn call(&mut self, _context: &mut CTX, inputs: &mut CallInputs) -> Option<CallOutcome> {
        if inputs.value.get() > U256::ZERO {
            return Some(CallOutcome::new(
                InterpreterResult::new(
                    interpreter::InstructionResult::ValueTransferNotAllowed,
                    Bytes::new(),
                    Gas::new(0),
                ),
                0..0,
            ));
        }
        None // Continue with normal execution
    }

    /// Prevent value transfers in CREATE and CREATE2 instructions
    fn create(&mut self, _context: &mut CTX, inputs: &mut CreateInputs) -> Option<CreateOutcome> {
        if inputs.value > U256::ZERO {
            return Some(CreateOutcome::new(
                InterpreterResult::new(
                    interpreter::InstructionResult::ValueTransferNotAllowed,
                    Bytes::new(),
                    Gas::new(0),
                ),
                None,
            ));
        }
        None // Continue with normal execution
    }
}
