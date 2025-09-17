use crate::{
    evm::SeismicEvm, handler::SeismicHandler,
    instructions::instruction_provider::SeismicInstructions, transaction::abstraction::SeismicTxTr,
    SeismicChain, SeismicHaltReason, SeismicSpecId,
};
use revm::{
    context::{
        result::{ExecResultAndState, InvalidTransaction},
        ContextSetters,
    },
    context_interface::{
        result::{EVMError, ExecutionResult},
        Cfg, ContextTr, Database, JournalTr,
    },
    handler::{EthFrame, Handler, PrecompileProvider},
    inspector::{InspectCommitEvm, InspectEvm, Inspector, InspectorHandler, JournalExt},
    interpreter::{interpreter::EthInterpreter, InterpreterResult},
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
    type ExecutionResult = ExecutionResult<SeismicHaltReason>;

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
