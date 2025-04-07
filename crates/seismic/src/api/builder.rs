use crate::{SeismicEvm, SeismicSpecId};
use revm::{
    context::{Cfg, ContextTr, JournalOutput},
    context_interface::{Block, JournalTr, Transaction},
    handler::instructions::EthInstructions,
    interpreter::interpreter::EthInterpreter,
    Context, Database,
};

/// Trait that allows for optimism SeismicEvm to be built.
pub trait SeismicBuilder: Sized {
    /// Type of the context.
    type Context: ContextTr;

    /// Build the op.
    fn build_seismic(self) -> SeismicEvm<Self::Context, (), EthInstructions<EthInterpreter, Self::Context>>;

    /// Build the op with an inspector.
    fn build_seismic_with_inspector<INSP>(
        self,
        inspector: INSP,
    ) -> SeismicEvm<Self::Context, INSP, EthInstructions<EthInterpreter, Self::Context>>;
}

impl<BLOCK, TX, CFG, DB, JOURNAL> SeismicBuilder for Context<BLOCK, TX, CFG, DB, JOURNAL>
where
    BLOCK: Block,
    TX: Transaction,
    CFG: Cfg<Spec = SeismicSpecId>,
    DB: Database,
    JOURNAL: JournalTr<Database = DB, FinalOutput = JournalOutput>,
{
    type Context = Self;

    fn build_seismic(self) -> SeismicEvm<Self::Context, (), EthInstructions<EthInterpreter, Self::Context>> {
        SeismicEvm::new(self, ())
    }

    fn build_seismic_with_inspector<INSP>(
        self,
        inspector: INSP,
    ) -> SeismicEvm<Self::Context, INSP, EthInstructions<EthInterpreter, Self::Context>> {
        SeismicEvm::new(self, inspector)
    }
}

