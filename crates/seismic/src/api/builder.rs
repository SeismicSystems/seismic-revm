use crate::{instructions::instruction_provider::SeismicInstructions, transaction::abstraction::SeismicTxTr, RngContainer, SeismicEvm, SeismicSpecId};
use revm::{
    context::{Cfg, JournalOutput},
    context_interface::{Block, JournalTr},
    interpreter::interpreter::EthInterpreter,
    Context, Database,
};

use super::exec::SeismicContextTr;

/// Trait that allows for SeismicEvm to be built.
pub trait SeismicBuilder: Sized {
    /// Type of the context.
    type Context: SeismicContextTr;

    /// Build seismic.
    fn build_seismic(self) -> SeismicEvm<Self::Context, (), SeismicInstructions<EthInterpreter, Self::Context>>;

    /// Build seismic with an inspector.
    fn build_seismic_with_inspector<INSP>(
        self,
        inspector: INSP,
    ) -> SeismicEvm<Self::Context, INSP, SeismicInstructions<EthInterpreter, Self::Context>>;
}

impl<BLOCK, TX, CFG, DB, JOURNAL> SeismicBuilder for Context<BLOCK, TX, CFG, DB, JOURNAL, RngContainer>
where
    BLOCK: Block,
    TX: SeismicTxTr,
    CFG: Cfg<Spec = SeismicSpecId>,
    DB: Database,
    JOURNAL: JournalTr<Database = DB, FinalOutput = JournalOutput>,
{
    type Context = Self;

    fn build_seismic(self) -> SeismicEvm<Self::Context, (), SeismicInstructions<EthInterpreter, Self::Context>> {
        SeismicEvm::new(self, ())
    }

    fn build_seismic_with_inspector<INSP>(
        self,
        inspector: INSP,
    ) -> SeismicEvm<Self::Context, INSP, SeismicInstructions<EthInterpreter, Self::Context>> {
        SeismicEvm::new(self, inspector)
    }
}

