//!Handler related to Seismic chain
use crate::{api::exec::SeismicContextTr, evm::SeismicEvm, SeismicSpecId};
use revm::{
    context::{result::{HaltReason, HaltReasonTr, InvalidTransaction}, transaction::TransactionError}, context_interface::{
        result::{EVMError, ExecutionResult, FromStringError, ResultAndState},
        Block, Cfg, ContextTr, JournalTr, Transaction,
    }, handler::{
        handler::EvmTrError, validation::validate_tx_against_account, EvmTr, Frame, FrameResult,
        Handler, MainnetHandler,
    }, inspector::{Inspector, InspectorEvmTr, InspectorFrame, InspectorHandler}, interpreter::{interpreter::EthInterpreter, FrameInput, Gas}, primitives::{hardfork::SpecId, HashMap, U256}, state::Account, Database
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

    fn validate_env(&self, evm: &mut Self::Evm) -> Result<(), Self::Error> {
        self.mainnet.validate_env(evm)
    }

    fn validate_tx_against_state(&self, evm: &mut Self::Evm) -> Result<(), Self::Error> {
        self.mainnet.validate_tx_against_state(evm)
    }

    fn deduct_caller(&self, evm: &mut Self::Evm) -> Result<(), Self::Error> {
        self.mainnet.deduct_caller(evm)
    }

    fn last_frame_result(
        &self,
        evm: &mut Self::Evm,
        frame_result: &mut <Self::Frame as Frame>::FrameResult,
    ) -> Result<(), Self::Error> {
        self.mainnet.last_frame_result(evm, frame_result)
    }

    fn reimburse_caller(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <Self::Frame as Frame>::FrameResult,
    ) -> Result<(), Self::Error> {
        self.mainnet.reimburse_caller(evm, exec_result)
    }

    fn refund(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <Self::Frame as Frame>::FrameResult,
        eip7702_refund: i64,
    ) {
        self.mainnet.refund(evm, exec_result, eip7702_refund)
    }

    fn reward_beneficiary(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <Self::Frame as Frame>::FrameResult,
    ) -> Result<(), Self::Error> {
        self.mainnet.reward_beneficiary(evm, exec_result)
    }

    fn output(
        &self,
        evm: &mut Self::Evm,
        result: <Self::Frame as Frame>::FrameResult,
    ) -> Result<ResultAndState<Self::HaltReason>, Self::Error> {
        self.mainnet.output(evm, result)
    }

    fn catch_error(
        &self,
        evm: &mut Self::Evm,
        error: Self::Error,
    ) -> Result<ResultAndState<Self::HaltReason>, Self::Error> {
        self.mainnet.catch_error(evm, error) 
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
