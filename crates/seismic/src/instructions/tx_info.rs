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
        build_interpreter_with_gas(spec_id, u64::MAX)
    }

    fn build_interpreter_with_gas(spec_id: SpecId, gas_limit: u64) -> Interpreter<EthInterpreter> {
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
            gas_limit,
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

    #[test]
    fn test_txtype_is_faithful_byte_mover() {
        // TXTYPE pushes whatever byte the tx reports, with no clamping or
        // validation — every value in [0, 255], including unknown types.
        for ty in [0u8, 1, 2, 3, 4, 42, 74, 0xFF] {
            let mut host = SeismicDummyHost::new().with_tx_type(ty);
            let mut interpreter = build_interpreter(SpecId::MERCURY);
            let context = InstructionContext {
                interpreter: &mut interpreter,
                host: &mut host,
            };

            txtype(context);

            assert_eq!(
                interpreter.stack.pop().unwrap(),
                U256::from(ty),
                "type {ty}"
            );
        }
    }

    #[test]
    fn test_txtype_charges_exactly_base_gas() {
        let mut host = SeismicDummyHost::new().with_tx_type(SEISMIC_TX_TYPE_ID);
        let mut interpreter = build_interpreter(SpecId::MERCURY);
        let context = InstructionContext {
            interpreter: &mut interpreter,
            host: &mut host,
        };

        txtype(context);

        // TXTYPE is priced at gas::BASE (2), matching CHAINID and other
        // tx/context reads. Charged exactly once.
        assert_eq!(interpreter.gas.spent(), 2);
    }

    #[test]
    fn test_txtype_succeeds_at_exact_gas() {
        // Exactly BASE (2) gas is enough.
        let mut host = SeismicDummyHost::new().with_tx_type(SEISMIC_TX_TYPE_ID);
        let mut interpreter = build_interpreter_with_gas(SpecId::MERCURY, 2);
        let context = InstructionContext {
            interpreter: &mut interpreter,
            host: &mut host,
        };

        txtype(context);

        assert_ne!(
            interpreter.bytecode.instruction_result(),
            Some(InstructionResult::OutOfGas)
        );
        assert_eq!(
            interpreter.stack.pop().unwrap(),
            U256::from(SEISMIC_TX_TYPE_ID)
        );
    }

    #[test]
    fn test_txtype_out_of_gas() {
        // One gas short of BASE => OutOfGas, and nothing is pushed.
        let mut host = SeismicDummyHost::new().with_tx_type(SEISMIC_TX_TYPE_ID);
        let mut interpreter = build_interpreter_with_gas(SpecId::MERCURY, 1);
        let context = InstructionContext {
            interpreter: &mut interpreter,
            host: &mut host,
        };

        txtype(context);

        assert_eq!(
            interpreter.bytecode.instruction_result(),
            Some(InstructionResult::OutOfGas)
        );
        assert_eq!(interpreter.stack.len(), 0);
    }

    #[test]
    fn test_txtype_stack_overflow() {
        // With a full (1024) stack, TXTYPE must halt with StackOverflow rather
        // than panic, and must not grow the stack.
        let mut host = SeismicDummyHost::new().with_tx_type(SEISMIC_TX_TYPE_ID);
        let mut interpreter = build_interpreter(SpecId::MERCURY);
        for _ in 0..1024 {
            assert!(interpreter.stack.push(U256::ZERO));
        }
        let context = InstructionContext {
            interpreter: &mut interpreter,
            host: &mut host,
        };

        txtype(context);

        assert_eq!(
            interpreter.bytecode.instruction_result(),
            Some(InstructionResult::StackOverflow)
        );
        assert_eq!(interpreter.stack.len(), 1024);
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
        database::{BenchmarkDB, InMemoryDB, BENCH_CALLER, BENCH_TARGET},
        primitives::{address, Address, TxKind, U256},
        state::{AccountInfo, Bytecode},
        Context, ExecuteEvm,
    };

    use super::super::instruction_provider::TXTYPE;

    // Callee runtime code: TXTYPE; PUSH1 0; MSTORE; PUSH1 32; PUSH1 0; RETURN
    // -> returns the observed tx-type byte as 32 bytes.
    const CALLEE_CODE: [u8; 9] = [
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
        // Standard types that validate cleanly with a bare tx env flow through
        // unchanged. (Types 3/4 need blob/auth-list fields to pass tx
        // validation, which is unrelated to TXTYPE; the byte-mover property for
        // arbitrary values is covered by the unit test above.)
        for ty in [0u8, 1, 2] {
            assert_eq!(observed_tx_type(ty), U256::from(ty), "tx_type {ty}");
        }
    }

    /// Runs a transaction of type `tx_type` where the entrypoint contract reaches
    /// TXTYPE through a nested frame built from `caller_code`, and returns the
    /// type observed inside that nested frame.
    fn observed_tx_type_nested(caller_code: Vec<u8>, tx_type: u8) -> U256 {
        let callee = address!("00000000000000000000000000000000000000cc");
        let caller = address!("00000000000000000000000000000000000000ca");
        let sender = BENCH_CALLER;

        let mut db = InMemoryDB::default();
        db.insert_account_info(
            callee,
            AccountInfo {
                code: Some(Bytecode::new_legacy(CALLEE_CODE.to_vec().into())),
                ..Default::default()
            },
        );
        db.insert_account_info(
            caller,
            AccountInfo {
                code: Some(Bytecode::new_legacy(caller_code.into())),
                ..Default::default()
            },
        );
        db.insert_account_info(
            sender,
            AccountInfo {
                balance: U256::from(1_000_000_000_000u64),
                ..Default::default()
            },
        );

        let ctx = Context::seismic_with_random_rng_key()
            .modify_tx_chained(|tx| {
                tx.base.tx_type = tx_type;
                tx.base.kind = TxKind::Call(caller);
                tx.base.caller = sender;
                tx.base.gas_limit = 1_000_000;
                tx.base.gas_price = 0;
            })
            .with_db(db);

        let mut evm = ctx.build_seismic_evm();
        let out = evm.replay().unwrap();
        assert!(
            out.result.is_success(),
            "nested execution should succeed: {:#?}",
            out.result
        );
        U256::from_be_slice(out.result.into_output().unwrap().as_ref())
    }

    // Builds caller code that forwards to `callee` via `call_op` (CALL or
    // DELEGATECALL) and returns the 32-byte result. CALL takes a value arg,
    // DELEGATECALL does not, so the value push is included only for CALL.
    fn nested_caller_code(callee: Address, call_op: u8) -> Vec<u8> {
        let mut code = vec![
            opcode::PUSH1,
            0x20, // retSize
            opcode::PUSH1,
            0x00, // retOffset
            opcode::PUSH1,
            0x00, // argsSize
            opcode::PUSH1,
            0x00, // argsOffset
        ];
        if call_op == opcode::CALL {
            code.extend_from_slice(&[opcode::PUSH1, 0x00]); // value
        }
        code.push(opcode::PUSH20);
        code.extend_from_slice(callee.as_slice()); // 20-byte address
        code.extend_from_slice(&[
            opcode::GAS,
            call_op,
            opcode::POP, // discard success flag
            opcode::PUSH1,
            0x20,
            opcode::PUSH1,
            0x00,
            opcode::RETURN,
        ]);
        code
    }

    #[test]
    fn txtype_is_tx_level_through_call() {
        // A callee reached via CALL still observes the outer transaction's type,
        // not a frame-local value.
        let callee = address!("00000000000000000000000000000000000000cc");
        let code = nested_caller_code(callee, opcode::CALL);
        assert_eq!(observed_tx_type_nested(code, 74), U256::from(74));
    }

    #[test]
    fn txtype_is_tx_level_through_delegatecall() {
        let callee = address!("00000000000000000000000000000000000000cc");
        let code = nested_caller_code(callee, opcode::DELEGATECALL);
        assert_eq!(observed_tx_type_nested(code, 74), U256::from(74));
    }
}
