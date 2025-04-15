//!Handler related to Seismic chain
use crate::{api::exec::SeismicContextTr, SeismicHaltReason};
use revm::{
    context::{
        result::{InvalidTransaction, ResultAndState},
        ContextTr,
    },
    context_interface::result::{EVMError, FromStringError},
    handler::{handler::EvmTrError, EvmTr, Frame, FrameResult, Handler, MainnetHandler},
    inspector::{Inspector, InspectorEvmTr, InspectorFrame, InspectorHandler},
    interpreter::{interpreter::EthInterpreter, FrameInput, InstructionResult},
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
    EVM: EvmTr<Context: SeismicContextTr>,
    ERROR: EvmTrError<EVM> + From<InvalidTransaction> + FromStringError,
    FRAME: Frame<Evm = EVM, Error = ERROR, FrameResult = FrameResult, FrameInit = FrameInput>,
{
    type Evm = EVM;
    type Error = ERROR;
    type Frame = FRAME;
    type HaltReason = SeismicHaltReason;

    fn pre_execution(&self, evm: &mut Self::Evm) -> Result<u64, Self::Error> {
        evm.ctx().chain().reset_rng();
        self.mainnet.pre_execution(evm)
    }
    
    //fn catch_error(
    //    &self,
    //    evm: &mut Self::Evm,
    //    error: Self::Error,
    //) -> Result<ResultAndState<SeismicHaltReason>, Self::Error> {
    //    if evm.ctx_instructions().() == Some(InstructionResult::FatalExternalError) {
    //        if let Some(custom_reason) = evm.ctx().chain().take_halt_reason() {
    //            if let Some(result_and_state) = error.into_result() {
    //                return Ok(result_and_state.map_haltreason(|_| custom_reason));
    //            }
    //            
    //            // Otherwise create a minimal result with our custom reason
    //            let gas_used = evm.ctx().tx().gas_limit();
    //            return Ok(ResultAndState {
    //                result: ExecutionResult::Halt { reason: custom_reason, gas_used },
    //                state: HashMap::new(),
    //            });
    //        }
    //    }
    //    
    //    // For non-FatalExternalError cases, convert standard HaltReason to SeismicHaltReason
    //    if let Some(result_and_state) = error.into_result() {
    //        Ok(result_and_state.map_haltreason(SeismicHaltReason::from))
    //    } else {
    //        Err(error)
    //    }
    //}
}

// Fix for the first error: Simplify the InspectorHandler implementation with proper bounds
impl<EVM, ERROR, FRAME> InspectorHandler for SeismicHandler<EVM, ERROR, FRAME>
where
    EVM: InspectorEvmTr<Context: SeismicContextTr>,
    ERROR: EvmTrError<EVM> + From<InvalidTransaction> + FromStringError,
    FRAME: InspectorFrame<
        Evm = EVM,
        Error = ERROR,
        FrameResult = FrameResult,
        FrameInit = FrameInput,
        IT = EthInterpreter,
    >,
    EVM::Inspector: Inspector<EVM::Context, EthInterpreter>,
{
    type IT = EthInterpreter;
}
