use crate::{
    evm::SeismicEvm, handler::SeismicHandler,
    instructions::instruction_provider::SeismicInstructions, transaction::abstraction::SeismicTxTr,
    SeismicChain, SeismicHaltReason, SeismicSpecId,
};
use revm::{
    context::{result::InvalidTransaction, ContextSetters, JournalOutput},
    context_interface::{
        result::{EVMError, ExecutionResult, ResultAndState},
        Cfg, ContextTr, Database, JournalTr,
    },
    handler::{EthFrame, EvmTr, Handler, PrecompileProvider},
    inspector::{InspectCommitEvm, InspectEvm, Inspector, InspectorHandler, JournalExt},
    interpreter::{interpreter::EthInterpreter, InterpreterResult},
    DatabaseCommit, ExecuteCommitEvm, ExecuteEvm,
};

// Type alias for Seismic context
pub trait SeismicContextTr:
    ContextTr<
    Journal: JournalTr<FinalOutput = JournalOutput>,
    Tx: SeismicTxTr,
    Cfg: Cfg<Spec = SeismicSpecId>,
    Chain = SeismicChain,
>
{
}

impl<T> SeismicContextTr for T where
    T: ContextTr<
        Journal: JournalTr<FinalOutput = JournalOutput>,
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
    type Output = Result<ResultAndState<SeismicHaltReason>, SeismicError<CTX>>;

    type Tx = <CTX as ContextTr>::Tx;

    type Block = <CTX as ContextTr>::Block;

    fn set_tx(&mut self, tx: Self::Tx) {
        self.0.ctx.set_tx(tx);
    }

    fn set_block(&mut self, block: Self::Block) {
        self.0.ctx.set_block(block);
    }

    fn replay(&mut self) -> Self::Output {
        let mut h = SeismicHandler::<_, _, EthFrame<_, _, _>>::new();
        h.run(self)
    }
}

impl<CTX, INSP, PRECOMPILE> ExecuteCommitEvm
    for SeismicEvm<CTX, INSP, SeismicInstructions<EthInterpreter, CTX>, PRECOMPILE>
where
    CTX: SeismicContextTr<Db: DatabaseCommit> + ContextSetters,
    PRECOMPILE: PrecompileProvider<CTX, Output = InterpreterResult>,
{
    type CommitOutput = Result<ExecutionResult<SeismicHaltReason>, SeismicError<CTX>>;

    fn replay_commit(&mut self) -> Self::CommitOutput {
        self.replay().map(|r| {
            self.ctx().db().commit(r.state);
            r.result
        })
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

    fn inspect_replay(&mut self) -> Self::Output {
        let mut h = SeismicHandler::<_, _, EthFrame<_, _, _>>::new();
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
    fn inspect_replay_commit(&mut self) -> Self::CommitOutput {
        self.inspect_replay().map(|r| {
            self.ctx().db().commit(r.state);
            r.result
        })
    }
}
