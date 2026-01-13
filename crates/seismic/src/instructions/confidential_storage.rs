use crate::{check, SeismicHaltReason, SeismicHost};
use revm::primitives::hardfork::SpecId::*;
use revm::{
    context::host::LoadError,
    interpreter::{
        gas::{
            CALL_STIPEND, COLD_SLOAD_COST_ADDITIONAL, CSTORE_FIXED_GAS, ISTANBUL_SLOAD_GAS,
            WARM_STORAGE_READ_COST,
        },
        interpreter_types::{InputsTr, InterpreterTypes, RuntimeFlag, StackTr},
        popn, popn_top, require_non_staticcall, Host, Instruction, InstructionContext,
        InstructionResult, _count, gas,
    },
};

/// Implements the SLOAD instruction.
///
/// Loads a word from storage.
pub fn sload<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>(
    context: InstructionContext<'_, H, WIRE>,
) {
    popn_top!([], index, context.interpreter);
    let spec_id = context.interpreter.runtime_flag.spec_id();
    let target = context.interpreter.input.target_address();

    // `SLOAD` opcode cost calculation.
    let gas = if spec_id.is_enabled_in(BERLIN) {
        WARM_STORAGE_READ_COST
    } else if spec_id.is_enabled_in(ISTANBUL) {
        // EIP-1884: Repricing for trie-size-dependent opcodes
        ISTANBUL_SLOAD_GAS
    } else if spec_id.is_enabled_in(TANGERINE) {
        // EIP-150: Gas cost changes for IO-heavy operations
        200
    } else {
        50
    };
    gas!(context.interpreter, gas);
    if spec_id.is_enabled_in(BERLIN) {
        let skip_cold = context.interpreter.gas.remaining() < COLD_SLOAD_COST_ADDITIONAL;
        let res = context.host.sload_skip_cold_load(target, *index, skip_cold);
        match res {
            Ok(storage) => {
                if storage.is_cold {
                    gas!(context.interpreter, COLD_SLOAD_COST_ADDITIONAL);
                }
                if storage.is_private {
                    context.interpreter.halt_fatal();
                    context
                        .host
                        .set_halt_reason(SeismicHaltReason::InvalidPrivateStorageAccess);
                    return;
                }

                *index = storage.data;
            }
            Err(LoadError::ColdLoadSkipped) => context.interpreter.halt_oog(),
            Err(LoadError::DBError) => context.interpreter.halt_fatal(),
        }
    } else {
        let Some(storage) = context.host.sload(target, *index) else {
            return context.interpreter.halt_fatal();
        };
        if storage.is_private {
            context.interpreter.halt_fatal();
            context
                .host
                .set_halt_reason(SeismicHaltReason::InvalidPrivateStorageAccess);
            return;
        }
        *index = storage.data;
    };
}

/// Implements the CLOAD instruction.
///
/// Loads a word from shielded storage with flat gas cost to prevent information leakage.
pub fn cload<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>(
    context: InstructionContext<'_, H, WIRE>,
) {
    check!(context.interpreter, MERCURY);
    popn_top!([], index, context.interpreter);
    let target = context.interpreter.input.target_address();

    gas!(
        context.interpreter,
        WARM_STORAGE_READ_COST + COLD_SLOAD_COST_ADDITIONAL
    );

    let Some(storage) = context.host.sload(target, *index) else {
        return context.interpreter.halt_fatal();
    };

    if !storage.is_private && !storage.data.is_zero() {
        context.interpreter.halt_fatal();
        context
            .host
            .set_halt_reason(SeismicHaltReason::InvalidPublicStorageAccess);
        return;
    }

    *index = storage.data;
}

/// Implements the SSTORE instruction.
///
/// Stores a word to public storage.
pub fn sstore<WIRE: InterpreterTypes, H: Host + ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    require_non_staticcall!(context.interpreter);
    popn!([index, value], context.interpreter);

    let target = context.interpreter.input.target_address();
    let spec_id = context.interpreter.runtime_flag.spec_id();

    // EIP-1706 Disable SSTORE with gasleft lower than call stipend
    if context
        .interpreter
        .runtime_flag
        .spec_id()
        .is_enabled_in(ISTANBUL)
        && context.interpreter.gas.remaining() <= CALL_STIPEND
    {
        context
            .interpreter
            .halt(InstructionResult::ReentrancySentryOOG);
        return;
    }

    // static gas
    gas!(
        context.interpreter,
        gas::static_sstore_cost(context.interpreter.runtime_flag.spec_id())
    );

    let state_load = if spec_id.is_enabled_in(BERLIN) {
        let skip_cold = context.interpreter.gas.remaining() < COLD_SLOAD_COST_ADDITIONAL;
        let res = context
            .host
            .sstore_skip_cold_load(target, index, value, skip_cold);
        match res {
            Ok(load) => load,
            Err(LoadError::ColdLoadSkipped) => return context.interpreter.halt_oog(),
            Err(LoadError::DBError) => return context.interpreter.halt_fatal(),
        }
    } else {
        let Some(load) = context.host.sstore(target, index, value) else {
            return context.interpreter.halt_fatal();
        };
        load
    };

    // dynamic gas
    gas!(
        context.interpreter,
        gas::dyn_sstore_cost(
            context.interpreter.runtime_flag.spec_id(),
            &state_load.data,
            state_load.is_cold
        )
    );

    // refund
    context.interpreter.gas.record_refund(gas::sstore_refund(
        context.interpreter.runtime_flag.spec_id(),
        &state_load.data,
    ));
}

/// Implements the CSTORE instruction.
///
/// Stores a word to shielded storage with flat gas cost to prevent information leakage.
/// Unlike SSTORE, CSTORE charges constant gas regardless of value transitions to avoid
/// leaking information about secret values through gas observations.
pub fn cstore<WIRE: InterpreterTypes, H: Host + ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    check!(context.interpreter, MERCURY);
    require_non_staticcall!(context.interpreter);
    popn!([index, value], context.interpreter);

    let target = context.interpreter.input.target_address();

    // EIP-1706 Disable SSTORE with gasleft lower than call stipend
    if context
        .interpreter
        .runtime_flag
        .spec_id()
        .is_enabled_in(ISTANBUL)
        && context.interpreter.gas.remaining() <= CALL_STIPEND
    {
        context
            .interpreter
            .halt(InstructionResult::ReentrancySentryOOG);
        return;
    }

    let flat_gas = gas::static_sstore_cost(context.interpreter.runtime_flag.spec_id())
        + CSTORE_FIXED_GAS
        + COLD_SLOAD_COST_ADDITIONAL;
    gas!(context.interpreter, flat_gas);

    if context.host.cstore(target, index, value, false).is_err() {
        context.interpreter.halt_fatal()
    }
}

// NOTE: static_gas is 0 for these, because gas is dynamic
pub fn cload_instruction<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>() -> Instruction<WIRE, H>
{
    Instruction::new(cload, 0)
}

pub fn cstore_instruction<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>() -> Instruction<WIRE, H>
{
    Instruction::new(cstore, 0)
}

pub fn seismic_sload_instruction<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>(
) -> Instruction<WIRE, H> {
    Instruction::new(sload, 0)
}

pub fn seismic_sstore_instruction<WIRE: InterpreterTypes, H: SeismicHost + ?Sized>(
) -> Instruction<WIRE, H> {
    Instruction::new(sstore, 0)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

    use crate::instructions::seismic_host::SeismicDummyHost;

    use super::*;
    use revm::context_interface::context::SStoreResult;
    use revm::interpreter::gas::{CSTORE_FIXED_GAS, WARM_STORAGE_READ_COST};
    use revm::interpreter::interpreter::{EthInterpreter, ExtBytecode};
    use revm::interpreter::interpreter_types::LoopControl;
    use revm::interpreter::{CallInput, InputsImpl, SharedMemory};
    use revm::interpreter::{InstructionResult, Interpreter};
    use revm::primitives::hardfork::SpecId;
    use revm::primitives::{Address, Bytes, FlaggedStorage, U256};
    use revm::state::Bytecode;

    // Helper to build an interpreter with a given SpecId.
    fn build_interpreter(spec_id: SpecId, bytecode: Bytecode) -> Interpreter<EthInterpreter> {
        Interpreter::<EthInterpreter>::new(
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
        )
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
        let mut interpreter = build_interpreter(SpecId::MERCURY, bytecode);
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
        let mut interpreter = build_interpreter(SpecId::MERCURY, bytecode);
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

    mod gas_tests {
        use super::*;
        use revm::context::host::LoadError;
        use revm::context_interface::journaled_state::{AccountInfoLoad, AccountLoad, StateLoad};
        use revm::database::EmptyDB;
        use revm::database_interface::Database;
        use revm::interpreter::gas::COLD_SLOAD_COST_ADDITIONAL;
        use revm::primitives::{Log, B256};

        const CSTORE_FLAT_GAS: u64 =
            WARM_STORAGE_READ_COST + CSTORE_FIXED_GAS + COLD_SLOAD_COST_ADDITIONAL;

        struct MockCstoreHost {
            sstore_result: SStoreResult,
            #[allow(dead_code)]
            is_cold: bool,
        }

        impl MockCstoreHost {
            fn new(sstore_result: SStoreResult, is_cold: bool) -> Self {
                Self {
                    sstore_result,
                    is_cold,
                }
            }

            fn zero_to_nonzero(is_cold: bool) -> Self {
                Self::new(
                    SStoreResult {
                        original_value: FlaggedStorage::ZERO,
                        present_value: FlaggedStorage::ZERO,
                        new_value: FlaggedStorage::new(U256::from(42), true),
                    },
                    is_cold,
                )
            }

            fn nonzero_to_zero(is_cold: bool) -> Self {
                Self::new(
                    SStoreResult {
                        original_value: FlaggedStorage::new(U256::from(100), true),
                        present_value: FlaggedStorage::new(U256::from(100), true),
                        new_value: FlaggedStorage::ZERO,
                    },
                    is_cold,
                )
            }

            fn nonzero_to_nonzero(is_cold: bool) -> Self {
                Self::new(
                    SStoreResult {
                        original_value: FlaggedStorage::new(U256::from(100), true),
                        present_value: FlaggedStorage::new(U256::from(100), true),
                        new_value: FlaggedStorage::new(U256::from(200), true),
                    },
                    is_cold,
                )
            }
        }

        impl crate::instructions::seismic_host::SeismicHost for MockCstoreHost {
            type Db = EmptyDB;
            #[allow(static_mut_refs)]
            fn ctx_error(
                &mut self,
            ) -> &mut Result<
                (),
                revm::context_interface::context::ContextError<<Self::Db as Database>::Error>,
            > {
                static mut ERR: Result<
                    (),
                    revm::context_interface::context::ContextError<std::convert::Infallible>,
                > = Ok(());
                unsafe { &mut ERR }
            }
        }

        impl Host for MockCstoreHost {
            fn basefee(&self) -> U256 {
                U256::ZERO
            }
            fn blob_gasprice(&self) -> U256 {
                U256::ZERO
            }
            fn gas_limit(&self) -> U256 {
                U256::MAX
            }
            fn difficulty(&self) -> U256 {
                U256::ZERO
            }
            fn prevrandao(&self) -> Option<U256> {
                None
            }
            fn block_number(&self) -> U256 {
                U256::ZERO
            }
            fn timestamp(&self) -> U256 {
                U256::ZERO
            }
            fn beneficiary(&self) -> Address {
                Address::ZERO
            }
            fn chain_id(&self) -> U256 {
                U256::from(1)
            }
            fn effective_gas_price(&self) -> U256 {
                U256::ZERO
            }
            fn caller(&self) -> Address {
                Address::ZERO
            }
            fn blob_hash(&self, _: usize) -> Option<U256> {
                None
            }
            fn max_initcode_size(&self) -> usize {
                0
            }
            fn block_hash(&mut self, _: u64) -> Option<B256> {
                None
            }
            fn selfdestruct(
                &mut self,
                _: Address,
                _: Address,
            ) -> Option<StateLoad<revm::interpreter::SelfDestructResult>> {
                None
            }
            fn log(&mut self, _: Log) {}
            fn tstore(&mut self, _: Address, _: U256, _: U256) {}
            fn tload(&mut self, _: Address, _: U256) -> U256 {
                U256::ZERO
            }
            fn sstore(&mut self, _: Address, _: U256, _: U256) -> Option<StateLoad<SStoreResult>> {
                None
            }
            fn sload(&mut self, _: Address, _: U256) -> Option<StateLoad<U256>> {
                None
            }
            fn balance(&mut self, _: Address) -> Option<StateLoad<U256>> {
                None
            }
            fn load_account_delegated(&mut self, _: Address) -> Option<StateLoad<AccountLoad>> {
                None
            }
            fn load_account_code(&mut self, _: Address) -> Option<StateLoad<Bytes>> {
                None
            }
            fn load_account_code_hash(&mut self, _: Address) -> Option<StateLoad<B256>> {
                None
            }
            fn load_account_info_skip_cold_load(
                &mut self,
                _: Address,
                _: bool,
                _: bool,
            ) -> Result<AccountInfoLoad<'_>, LoadError> {
                Err(LoadError::DBError)
            }
            fn sstore_skip_cold_load(
                &mut self,
                _: Address,
                _: U256,
                _: U256,
                _: bool,
            ) -> Result<StateLoad<SStoreResult>, LoadError> {
                Err(LoadError::DBError)
            }
            fn sload_skip_cold_load(
                &mut self,
                _: Address,
                _: U256,
                _: bool,
            ) -> Result<StateLoad<U256>, LoadError> {
                Err(LoadError::DBError)
            }
            fn cstore(
                &mut self,
                _: Address,
                _: U256,
                _: U256,
                _: bool,
            ) -> Result<StateLoad<SStoreResult>, LoadError> {
                Ok(StateLoad::new(self.sstore_result.clone(), false, true))
            }
            fn cload(
                &mut self,
                _: Address,
                _: U256,
                _: bool,
            ) -> Result<StateLoad<U256>, LoadError> {
                Err(LoadError::DBError)
            }
        }

        #[test]
        fn test_cstore_gas_constant_for_zero_vs_nonzero() {
            let bytecode = Bytecode::new_raw(Bytes::from(&[0x00][..]));

            let mut host1 = MockCstoreHost::zero_to_nonzero(true);
            let mut interp1 = build_interpreter(SpecId::MERCURY, bytecode.clone());
            let _ = interp1.stack.push(U256::from(1));
            let _ = interp1.stack.push(U256::from(42));
            let gas_before_1 = interp1.gas.remaining();
            cstore(InstructionContext {
                interpreter: &mut interp1,
                host: &mut host1,
            });
            let gas_used_1 = gas_before_1 - interp1.gas.remaining();

            let mut host2 = MockCstoreHost::nonzero_to_nonzero(true);
            let mut interp2 = build_interpreter(SpecId::MERCURY, bytecode.clone());
            let _ = interp2.stack.push(U256::from(1));
            let _ = interp2.stack.push(U256::from(200));
            let gas_before_2 = interp2.gas.remaining();
            cstore(InstructionContext {
                interpreter: &mut interp2,
                host: &mut host2,
            });
            let gas_used_2 = gas_before_2 - interp2.gas.remaining();

            assert_eq!(
                gas_used_1, gas_used_2,
                "Gas must be constant regardless of value transition"
            );
            assert_eq!(gas_used_1, CSTORE_FLAT_GAS);
        }

        #[test]
        fn test_cstore_gas_flat_cold_vs_warm() {
            let bytecode = Bytecode::new_raw(Bytes::from(&[0x00][..]));

            let mut host_cold = MockCstoreHost::zero_to_nonzero(true);
            let mut interp_cold = build_interpreter(SpecId::MERCURY, bytecode.clone());
            let _ = interp_cold.stack.push(U256::from(1));
            let _ = interp_cold.stack.push(U256::from(42));
            let gas_before_cold = interp_cold.gas.remaining();
            cstore(InstructionContext {
                interpreter: &mut interp_cold,
                host: &mut host_cold,
            });
            let gas_cold = gas_before_cold - interp_cold.gas.remaining();

            let mut host_warm = MockCstoreHost::zero_to_nonzero(false);
            let mut interp_warm = build_interpreter(SpecId::MERCURY, bytecode.clone());
            let _ = interp_warm.stack.push(U256::from(1));
            let _ = interp_warm.stack.push(U256::from(42));
            let gas_before_warm = interp_warm.gas.remaining();
            cstore(InstructionContext {
                interpreter: &mut interp_warm,
                host: &mut host_warm,
            });
            let gas_warm = gas_before_warm - interp_warm.gas.remaining();

            assert_eq!(
                gas_cold, gas_warm,
                "Gas must be identical for cold vs warm access"
            );
            assert_eq!(gas_cold, CSTORE_FLAT_GAS);
            assert_eq!(gas_warm, CSTORE_FLAT_GAS);
        }

        #[test]
        fn test_cstore_gas_flat_all_scenarios() {
            let bytecode = Bytecode::new_raw(Bytes::from(&[0x00][..]));

            let scenarios: Vec<(&str, MockCstoreHost)> = vec![
                (
                    "cold_zero_to_nonzero",
                    MockCstoreHost::zero_to_nonzero(true),
                ),
                (
                    "warm_zero_to_nonzero",
                    MockCstoreHost::zero_to_nonzero(false),
                ),
                (
                    "cold_nonzero_to_nonzero",
                    MockCstoreHost::nonzero_to_nonzero(true),
                ),
                (
                    "warm_nonzero_to_nonzero",
                    MockCstoreHost::nonzero_to_nonzero(false),
                ),
                (
                    "cold_nonzero_to_zero",
                    MockCstoreHost::nonzero_to_zero(true),
                ),
                (
                    "warm_nonzero_to_zero",
                    MockCstoreHost::nonzero_to_zero(false),
                ),
            ];

            for (name, mut host) in scenarios {
                let mut interp = build_interpreter(SpecId::MERCURY, bytecode.clone());
                let _ = interp.stack.push(U256::from(1));
                let _ = interp.stack.push(U256::from(42));
                let gas_before = interp.gas.remaining();
                cstore(InstructionContext {
                    interpreter: &mut interp,
                    host: &mut host,
                });
                let gas_used = gas_before - interp.gas.remaining();

                assert_eq!(
                    gas_used, CSTORE_FLAT_GAS,
                    "Scenario '{}': expected flat gas {}, got {}",
                    name, CSTORE_FLAT_GAS, gas_used
                );
            }
        }

        #[test]
        fn test_cstore_no_refunds() {
            let bytecode = Bytecode::new_raw(Bytes::from(&[0x00][..]));

            let mut host = MockCstoreHost::nonzero_to_zero(true);
            let mut interp = build_interpreter(SpecId::MERCURY, bytecode);
            let _ = interp.stack.push(U256::from(1));
            let _ = interp.stack.push(U256::ZERO);

            cstore(InstructionContext {
                interpreter: &mut interp,
                host: &mut host,
            });

            assert_eq!(
                interp.gas.refunded(),
                0,
                "CSTORE must never issue refunds (would leak zero-clearing)"
            );
        }
    }
}
