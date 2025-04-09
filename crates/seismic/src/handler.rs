//!Handler related to Seismic chain
use crate::api::exec::SeismicContextTr;
use revm::{
    context::{result::{HaltReason, InvalidTransaction}, ContextTr}, context_interface::
        result::{EVMError, FromStringError},
    handler::{
        handler::EvmTrError, EvmTr, Frame, FrameResult,
        Handler, MainnetHandler,
    }, inspector::{Inspector, InspectorEvmTr, InspectorFrame, InspectorHandler}, interpreter::{interpreter::EthInterpreter, FrameInput}
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

// Helper trait to check if an error is a transaction error
pub trait IsTxError {
    fn is_tx_error(&self) -> bool;
}

impl<DB, TX> IsTxError for EVMError<DB, TX> {
    fn is_tx_error(&self) -> bool {
        matches!(self, EVMError::Transaction(_))
    }
}

impl<EVM, ERROR, FRAME> Handler for SeismicHandler<EVM, ERROR, FRAME>
where
    EVM: EvmTr<Context: SeismicContextTr>,
    ERROR: EvmTrError<EVM> + From<InvalidTransaction> + FromStringError + IsTxError,
    FRAME: Frame<Evm = EVM, Error = ERROR, FrameResult = FrameResult, FrameInit = FrameInput>,
{
    type Evm = EVM;
    type Error = ERROR;
    type Frame = FRAME;
    type HaltReason = HaltReason;

    fn pre_execution(&self, evm: &mut Self::Evm) -> Result<u64, Self::Error> {
        evm.ctx().chain().reset_rng();
        self.mainnet.pre_execution(evm)
    }
}

// Fix for the first error: Simplify the InspectorHandler implementation with proper bounds
impl<EVM, ERROR, FRAME> InspectorHandler for SeismicHandler<EVM, ERROR, FRAME>
where
    EVM: InspectorEvmTr<Context: SeismicContextTr>,
    ERROR: EvmTrError<EVM> + From<InvalidTransaction> + FromStringError + IsTxError,
    FRAME: InspectorFrame<
        Evm = EVM, 
        Error = ERROR,
        FrameResult = FrameResult,
        FrameInit = FrameInput,
        IT = EthInterpreter
    >,
    EVM::Inspector: Inspector<EVM::Context, EthInterpreter>,
{
    type IT = EthInterpreter;
}
