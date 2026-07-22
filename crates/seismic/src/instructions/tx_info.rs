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

/// TXTYPE (`0xB2`): pushes the EIP-2718 transaction-type byte. Mercury-gated.
pub fn txtype<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>(
    context: InstructionContext<'_, H, WIRE>,
) {
    check!(context.interpreter, MERCURY);
    gas!(context.interpreter, gas::BASE);
    push!(context.interpreter, U256::from(context.host.tx_type()));
}

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

        assert_eq!(interpreter.gas.spent(), 2);
    }

    #[test]
    fn test_txtype_succeeds_at_exact_gas() {
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

/// End-to-end tests exercising the production `ctx.tx().tx_type()` path.
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

    // TXTYPE; PUSH1 0; MSTORE; PUSH1 32; PUSH1 0; RETURN
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

    fn observed_tx_type(tx_type: u8) -> U256 {
        // TXTYPE; PUSH1 0; MSTORE; PUSH1 32; PUSH1 0; RETURN
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
        assert_eq!(observed_tx_type(74), U256::from(74));
    }

    #[test]
    fn txtype_reports_standard_types_end_to_end() {
        // Types 3/4 need blob/auth-list fields to pass tx validation; byte-mover
        // coverage for arbitrary values is in the unit test above.
        for ty in [0u8, 1, 2] {
            assert_eq!(observed_tx_type(ty), U256::from(ty), "tx_type {ty}");
        }
    }

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

    // CALL/DELEGATECALL to `callee`, returning its 32-byte result (CALL has a value arg).
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
