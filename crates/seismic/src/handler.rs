//!Handler related to Seismic chain
use crate::{api::exec::SeismicContextTr, SeismicHaltReason};
use revm::{
    context::{
        result::{ExecutionResult, InvalidTransaction},
        ContextTr, JournalTr, Transaction,
    },
    context_interface::{context::ContextError, result::FromStringError},
    handler::{
        handler::EvmTrError, post_execution, EthFrame, EvmTr, FrameResult, FrameTr, Handler,
        MainnetHandler,
    },
    inspector::{Inspector, InspectorEvmTr, InspectorHandler},
    interpreter::{interpreter::EthInterpreter, interpreter_action::FrameInit},
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
    type HaltReason = SeismicHaltReason;

    /// Processes the final execution output.
    ///
    /// This method, retrieves the final state from the journal, converts internal results to the external output format.
    /// Internal state is cleared and EVM is prepared for the next transaction.
    ///
    /// Seismic Addendum
    /// Given that we can't yet pass instruction_result which aren't in the InstructionResult enum,
    /// We leverage context_error to bubble up our instruction set specific errors! We also clear
    /// the rng state on returns that won't go through catch_error.
    #[inline]
    fn execution_result(
        &mut self,
        evm: &mut Self::Evm,
        result: <<Self::Evm as EvmTr>::Frame as FrameTr>::FrameResult,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        println!("Error: {:?}", result);
        match core::mem::replace(evm.ctx().error(), Ok(())) {
            Err(ContextError::Db(e)) => Err(e.into()),
            Err(ContextError::Custom(e)) => {
                if let Some(seismic_reason) =
                    SeismicHaltReason::try_from_error_string(&e.to_string())
                {
                    evm.ctx().journal_mut().clear();
                    evm.ctx().chain_mut().reset_rng();

                    return Ok(ExecutionResult::Halt {
                        reason: seismic_reason,
                        gas_used: evm.ctx().tx().gas_limit(),
                    });
                }

                Err(Self::Error::from_string(e))
            }
            Ok(_) => {
                let output = post_execution::output(evm.ctx(), result);
                evm.ctx().journal_mut().clear();
                evm.ctx().chain_mut().reset_rng();
                Ok(output)
            }
        }
    }

    /// Handles cleanup when an error occurs during execution.
    ///
    /// Ensures the journal state is properly cleared before propagating the error.
    /// Also ensures the rng has been reset.
    /// On happy path journal is cleared in [`Handler::output`] method.
    #[inline]
    fn catch_error(
        &self,
        evm: &mut Self::Evm,
        error: Self::Error,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        println!("We hit an error: {error:#?}");
        // Clean up journal state if error occurs
        evm.ctx().journal_mut().clear();
        evm.ctx().chain_mut().reset_rng();
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
    use super::*;
    use crate::{api::default_ctx::SeismicContext, DefaultSeismicContext, SeismicBuilder};
    use revm::{
        context::{result::EVMError, Context},
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
        let ctx = Context::seismic().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let gas = call_last_frame_return(ctx, InstructionResult::Revert, Gas::new(90));
        assert_eq!(gas.remaining(), 90);
        assert_eq!(gas.spent(), 10);
        assert_eq!(gas.refunded(), 0);
    }

    #[test]
    fn test_fatal_external_error_gas() {
        let ctx = Context::seismic().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let gas = call_last_frame_return(ctx, InstructionResult::FatalExternalError, Gas::new(90));
        assert_eq!(gas.remaining(), 0);
        assert_eq!(gas.spent(), 100);
        assert_eq!(gas.refunded(), 0);
    }
}
