use crate::{
    evm::SeismicEvm, handler::SeismicHandler,
    instructions::instruction_provider::SeismicInstructions, transaction::abstraction::SeismicTxTr,
    SeismicChain, SeismicSpecId,
};
use revm::{
    context::{
        result::{ExecResultAndState, InvalidTransaction},
        ContextSetters,
    },
    context_interface::{
        result::{EVMError, ExecutionResult, HaltReason},
        Cfg, ContextTr, Database, JournalTr,
    },
    handler::{system_call::SystemCallEvm, EthFrame, Handler, PrecompileProvider, SystemCallTx},
    inspector::{
        InspectCommitEvm, InspectEvm, InspectSystemCallEvm, Inspector, InspectorHandler, JournalExt,
    },
    interpreter::{interpreter::EthInterpreter, InterpreterResult},
    primitives::{Address, Bytes},
    state::EvmState,
    DatabaseCommit, ExecuteCommitEvm, ExecuteEvm,
};

// Type alias for Seismic context
pub trait SeismicContextTr:
    ContextTr<
    Journal: JournalTr<State = EvmState>,
    Tx: SeismicTxTr,
    Cfg: Cfg<Spec = SeismicSpecId>,
    Chain = SeismicChain,
>
{
}

impl<T> SeismicContextTr for T where
    T: ContextTr<
        Journal: JournalTr<State = EvmState>,
        Tx: SeismicTxTr,
        Cfg: Cfg<Spec = SeismicSpecId>,
        Chain = SeismicChain,
    >
{
}

/// Type alias for the error type of the SeismicEvm.
type SeismicError<CTX> = EVMError<<<CTX as ContextTr>::Db as Database>::Error, InvalidTransaction>;

impl<CTX, INSP, PRECOMPILE> ExecuteEvm
    for SeismicEvm<CTX, INSP, SeismicInstructions<EthInterpreter, CTX>, PRECOMPILE>
where
    CTX: SeismicContextTr + ContextSetters,
    PRECOMPILE: PrecompileProvider<CTX, Output = InterpreterResult>,
{
    type Tx = <CTX as ContextTr>::Tx;
    type Block = <CTX as ContextTr>::Block;
    type State = EvmState;
    type Error = SeismicError<CTX>;
    type ExecutionResult = ExecutionResult<HaltReason>;

    fn set_block(&mut self, block: Self::Block) {
        self.0.ctx.set_block(block);
    }

    fn transact_one(&mut self, tx: Self::Tx) -> Result<Self::ExecutionResult, Self::Error> {
        self.0.ctx.set_tx(tx);
        let mut h = SeismicHandler::<_, _, EthFrame<EthInterpreter>>::new();
        h.run(self)
    }

    fn finalize(&mut self) -> Self::State {
        self.0.ctx.journal_mut().finalize()
    }

    fn replay(
        &mut self,
    ) -> Result<ExecResultAndState<Self::ExecutionResult, Self::State>, Self::Error> {
        let mut h = SeismicHandler::<_, _, EthFrame<EthInterpreter>>::new();
        h.run(self).map(|result| {
            let state = self.finalize();
            ExecResultAndState::new(result, state)
        })
    }
}

impl<CTX, INSP, PRECOMPILE> ExecuteCommitEvm
    for SeismicEvm<CTX, INSP, SeismicInstructions<EthInterpreter, CTX>, PRECOMPILE>
where
    CTX: SeismicContextTr<Db: DatabaseCommit> + ContextSetters,
    PRECOMPILE: PrecompileProvider<CTX, Output = InterpreterResult>,
{
    fn commit(&mut self, state: Self::State) {
        self.0.ctx.db_mut().commit(state);
    }
}

impl<CTX, INSP, PRECOMPILE> InspectEvm
    for SeismicEvm<CTX, INSP, SeismicInstructions<EthInterpreter, CTX>, PRECOMPILE>
where
    CTX: SeismicContextTr<Journal: JournalExt> + ContextSetters,
    INSP: Inspector<CTX, EthInterpreter>,
    PRECOMPILE: PrecompileProvider<CTX, Output = InterpreterResult>,
{
    type Inspector = INSP;

    fn set_inspector(&mut self, inspector: Self::Inspector) {
        self.0.inspector = inspector;
    }

    fn inspect_one_tx(&mut self, tx: Self::Tx) -> Result<Self::ExecutionResult, Self::Error> {
        self.0.ctx.set_tx(tx);
        let mut h = SeismicHandler::<_, _, EthFrame<EthInterpreter>>::new();
        h.inspect_run(self)
    }
}

impl<CTX, INSP, PRECOMPILE> InspectCommitEvm
    for SeismicEvm<CTX, INSP, SeismicInstructions<EthInterpreter, CTX>, PRECOMPILE>
where
    CTX: SeismicContextTr<Journal: JournalExt, Db: DatabaseCommit> + ContextSetters,
    INSP: Inspector<CTX, EthInterpreter>,
    PRECOMPILE: PrecompileProvider<CTX, Output = InterpreterResult>,
{
}

impl<CTX, INSP, PRECOMPILE> SystemCallEvm
    for SeismicEvm<CTX, INSP, SeismicInstructions<EthInterpreter, CTX>, PRECOMPILE>
where
    CTX: SeismicContextTr<Tx: SystemCallTx> + ContextSetters,
    PRECOMPILE: PrecompileProvider<CTX, Output = InterpreterResult>,
{
    fn system_call_one_with_caller(
        &mut self,
        caller: Address,
        system_contract_address: Address,
        data: Bytes,
    ) -> Result<Self::ExecutionResult, Self::Error> {
        self.0.ctx.set_tx(CTX::Tx::new_system_tx_with_caller(
            caller,
            system_contract_address,
            data,
        ));
        let mut h = SeismicHandler::<_, _, EthFrame<EthInterpreter>>::new();
        h.run_system_call(self)
    }
}

impl<CTX, INSP, PRECOMPILE> InspectSystemCallEvm
    for SeismicEvm<CTX, INSP, SeismicInstructions<EthInterpreter, CTX>, PRECOMPILE>
where
    CTX: SeismicContextTr<Journal: JournalExt, Tx: SystemCallTx> + ContextSetters,
    INSP: Inspector<CTX, EthInterpreter>,
    PRECOMPILE: PrecompileProvider<CTX, Output = InterpreterResult>,
{
    fn inspect_one_system_call_with_caller(
        &mut self,
        caller: Address,
        system_contract_address: Address,
        data: Bytes,
    ) -> Result<Self::ExecutionResult, Self::Error> {
        self.0.ctx.set_tx(CTX::Tx::new_system_tx_with_caller(
            caller,
            system_contract_address,
            data,
        ));
        let mut h = SeismicHandler::<_, _, EthFrame<EthInterpreter>>::new();
        h.inspect_run_system_call(self)
    }
}
