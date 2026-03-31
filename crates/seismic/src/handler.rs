//!Handler related to Seismic chain
use crate::{api::exec::SeismicContextTr, transaction::abstraction::SeismicTxTr};
use revm::{
    context::{
        result::{ExecutionResult, InvalidTransaction},
        ContextTr, JournalTr, LocalContextTr,
    },
    context_interface::{
        context::ContextError,
        result::{FromStringError, HaltReason},
        transaction::Transaction,
    },
    handler::{
        handler::EvmTrError, post_execution, EthFrame, EvmTr, FrameResult, FrameTr, Handler,
        MainnetHandler,
    },
    inspector::{Inspector, InspectorEvmTr, InspectorHandler},
    interpreter::{
        interpreter::EthInterpreter, interpreter_action::FrameInit, CallOutcome, Gas,
        InitialAndFloorGas, InstructionResult, InterpreterResult,
    },
    primitives::Bytes,
};

pub struct SeismicHandler<EVM, ERROR, FRAME> {
    pub mainnet: MainnetHandler<EVM, ERROR, FRAME>,
    pub _phantom: core::marker::PhantomData<(EVM, ERROR, FRAME)>,
}

impl<EVM, ERROR, FRAME> SeismicHandler<EVM, ERROR, FRAME> {
    pub fn new() -> Self {
        Self {
            mainnet: MainnetHandler::default(),
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<EVM, ERROR, FRAME> Default for SeismicHandler<EVM, ERROR, FRAME> {
    fn default() -> Self {
        Self::new()
    }
}

impl<EVM, ERROR, FRAME> Handler for SeismicHandler<EVM, ERROR, FRAME>
where
    EVM: EvmTr<Context: SeismicContextTr, Frame = FRAME>,
    ERROR: EvmTrError<EVM> + From<InvalidTransaction> + FromStringError + core::fmt::Debug,
    FRAME: FrameTr<FrameResult = FrameResult, FrameInit = FrameInit>,
{
    type Evm = EVM;
    type Error = ERROR;
    type HaltReason = HaltReason;

    /// Overrides the execution phase to short-circuit when a transaction's calldata
    /// decryption has failed. In this case, bytecode execution is skipped and a Revert
    /// is returned with all execution gas unspent (only intrinsic gas is charged).
    ///
    /// The validate and pre_execution phases still run normally, ensuring the sender's
    /// balance is deducted and nonce is incremented. The post_execution phase reimburses
    /// the unused execution gas and credits the coinbase.
    #[inline]
    fn execution(
        &mut self,
        evm: &mut Self::Evm,
        init_and_floor_gas: &InitialAndFloorGas,
    ) -> Result<FrameResult, Self::Error> {
        if evm.ctx().tx().decryption_failed() {
            // All gas beyond intrinsic is returned to the sender.
            let execution_gas = evm.ctx().tx().gas_limit() - init_and_floor_gas.initial_gas;
            return Ok(FrameResult::Call(CallOutcome::new(
                InterpreterResult {
                    result: InstructionResult::Revert,
                    output: Bytes::new(),
                    gas: Gas::new(execution_gas),
                },
                0..0,
            )));
        }
        self.mainnet.execution(evm, init_and_floor_gas)
    }

    /// Processes the final execution output.
    ///
    /// Retrieves the final state from the journal, converts internal results
    /// to the external output format. Internal state is cleared and EVM is
    /// prepared for the next transaction.
    ///
    /// Seismic Addendum:
    /// Given that we can't yet pass instruction_result which aren't in the
    /// InstructionResult enum, we leverage context_error to bubble up our
    /// instruction set specific errors.
    #[inline]
    fn execution_result(
        &mut self,
        evm: &mut Self::Evm,
        result: <<Self::Evm as EvmTr>::Frame as FrameTr>::FrameResult,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        match core::mem::replace(evm.ctx().error(), Ok(())) {
            Err(ContextError::Db(e)) => return Err(e.into()),
            Err(ContextError::Custom(e)) => {
                return Err(Self::Error::from_string(e));
            }
            Ok(_) => (),
        }

        let exec_result = post_execution::output(evm.ctx(), result);

        evm.ctx().journal_mut().commit_tx();
        evm.ctx().local_mut().clear();
        evm.frame_stack().clear();

        Ok(exec_result)
    }

    /// Handles cleanup when an error occurs during execution.
    ///
    /// Ensures the journal state is properly cleared before propagating the error.
    /// On happy path journal is cleared in [`Handler::output`] method.
    #[inline]
    fn catch_error(
        &self,
        evm: &mut Self::Evm,
        error: Self::Error,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        evm.ctx().local_mut().clear();
        evm.ctx().journal_mut().discard_tx();
        evm.frame_stack().clear();
        Err(error)
    }
}

// Fix for the first error: Simplify the InspectorHandler implementation with proper bounds
impl<EVM, ERROR> InspectorHandler for SeismicHandler<EVM, ERROR, EthFrame<EthInterpreter>>
where
    EVM: InspectorEvmTr<Context: SeismicContextTr>,
    ERROR: EvmTrError<EVM> + From<InvalidTransaction> + FromStringError + core::fmt::Debug,
    EVM::Inspector: Inspector<EVM::Context, EthInterpreter>,
{
    type IT = EthInterpreter;
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
    use crate::{api::default_ctx::SeismicContext, DefaultSeismicContext, SeismicBuilder};
    use revm::{
        context::{result::EVMError, Context},
        context_interface::context::ContextError,
        database_interface::EmptyDB,
        handler::EthFrame,
        interpreter::{CallOutcome, Gas, InstructionResult, InterpreterResult},
        primitives::Bytes,
    };

    /// Creates frame result.
    fn call_last_frame_return(
        ctx: SeismicContext<EmptyDB>,
        instruction_result: InstructionResult,
        gas: Gas,
    ) -> Gas {
        let mut evm = ctx.build_seismic_evm();

        let mut exec_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: instruction_result,
                output: Bytes::new(),
                gas,
            },
            0..0,
        ));

        let mut handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        handler
            .last_frame_result(&mut evm, &mut exec_result)
            .unwrap();
        handler.refund(&mut evm, &mut exec_result, 0);
        *exec_result.gas()
    }

    #[test]
    fn test_revert_gas() {
        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let gas = call_last_frame_return(ctx, InstructionResult::Revert, Gas::new(90));
        assert_eq!(gas.remaining(), 90);
        assert_eq!(gas.spent(), 10);
        assert_eq!(gas.refunded(), 0);
    }

    #[test]
    fn test_fatal_external_error_gas() {
        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let gas = call_last_frame_return(ctx, InstructionResult::FatalExternalError, Gas::new(90));
        assert_eq!(gas.remaining(), 0);
        assert_eq!(gas.spent(), 100);
        assert_eq!(gas.refunded(), 0);
    }

    /// Regression test: catch_error must call discard_tx() so that
    /// transaction_id advances and warm slot/account tracking is reset.
    /// Without discard_tx(), slots warmed in a failed tx would remain warm
    /// for the next tx, causing incorrect (cheaper) gas accounting.
    #[test]
    fn test_catch_error_discards_tx_and_advances_transaction_id() {
        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let mut evm = ctx.build_seismic_evm();

        let tx_id_before = evm.ctx().journal().inner.transaction_id;

        // Inject a context error that triggers the catch_error path.
        *evm.ctx().error() = Err(ContextError::Custom("some error".to_string()));

        let frame_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: InstructionResult::Stop,
                output: Bytes::new(),
                gas: Gas::new(90),
            },
            0..0,
        ));

        let mut handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        // execution_result returns Err for context errors, which the
        // execution loop passes to catch_error.
        let err = handler
            .execution_result(&mut evm, frame_result)
            .unwrap_err();
        let _ = handler.catch_error(&mut evm, err);

        // transaction_id must have advanced, proving discard_tx() was called.
        let tx_id_after = evm.ctx().journal().inner.transaction_id;
        assert_eq!(
            tx_id_after,
            tx_id_before + 1,
            "transaction_id should advance after catch_error (discard_tx must be called)"
        );
    }

    /// When `decryption_failed` is set on the transaction, the execution phase
    /// should short-circuit with a Revert and return all execution gas as
    /// remaining (only intrinsic gas is consumed).
    #[test]
    fn test_decryption_failed_skips_execution_and_returns_all_execution_gas() {
        let gas_limit: u64 = 100_000;
        let intrinsic_gas: u64 = 21_000;

        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = gas_limit;
            tx.decryption_failed = true;
        });

        let mut evm = ctx.build_seismic_evm();

        let mut handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        let init_and_floor_gas = InitialAndFloorGas::new(intrinsic_gas, 0);
        let result = handler.execution(&mut evm, &init_and_floor_gas).unwrap();

        let gas = result.gas();
        let execution_budget = gas_limit - intrinsic_gas;
        assert_eq!(
            gas.remaining(),
            execution_budget,
            "all execution gas should be remaining (returned to sender)"
        );
        assert_eq!(gas.spent(), 0, "no execution gas should be spent");

        // Verify it's a Revert
        match &result {
            FrameResult::Call(outcome) => {
                assert_eq!(
                    outcome.result.result,
                    InstructionResult::Revert,
                    "should be a Revert"
                );
                assert!(
                    outcome.result.output.is_empty(),
                    "revert output should be empty"
                );
            }
            _ => panic!("expected FrameResult::Call"),
        }
    }

    /// When `decryption_failed` is NOT set, execution should proceed normally
    /// (delegating to the mainnet handler). This verifies the flag check doesn't
    /// accidentally short-circuit normal transactions.
    #[test]
    fn test_decryption_not_failed_proceeds_normally() {
        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
            // decryption_failed defaults to false
        });

        let mut evm = ctx.build_seismic_evm();

        let mut handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        let init_and_floor_gas = InitialAndFloorGas::new(21, 0);
        // Normal execution — this will attempt to run a transaction against
        // the empty DB, which may fail, but the point is it does NOT take
        // the decryption_failed short-circuit path.
        let result = handler.execution(&mut evm, &init_and_floor_gas);

        // If execution reached the mainnet handler (not short-circuited),
        // the result might be Ok or Err depending on the empty state, but
        // it should NOT be the specific Revert-with-full-gas pattern from
        // the decryption_failed path.
        if let Ok(frame_result) = result {
            let gas = frame_result.gas();
            // Normal execution would consume some gas, not return full budget
            let execution_budget = 100 - 21;
            // If it returned full budget as remaining, that means it hit the
            // decryption_failed path (which it shouldn't)
            if gas.remaining() == execution_budget && gas.spent() == 0 {
                panic!("normal tx should not take the decryption_failed short-circuit path");
            }
        }
        // Any other result (error, different gas values) means normal execution
        // was attempted, which is correct.
    }
}
