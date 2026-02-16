use crate::{
    evm::SeismicEmptyDB, transaction::abstraction::SeismicTransaction, SeismicChain, SeismicSpecId,
};
use revm::{
    context::{BlockEnv, CfgEnv, TxEnv},
    primitives::FlaggedStorage,
    Context, Journal, JournalEntry, MainContext,
};

/// Type alias for a Seismic journal using FlaggedStorage entries.
pub type SeismicJournal<DB> = Journal<DB, JournalEntry<FlaggedStorage>>;

/// Type alias for the default context type of the SeismicEvm.
pub type SeismicContext<DB> = Context<
    BlockEnv,
    SeismicTransaction<TxEnv>,
    CfgEnv<SeismicSpecId>,
    DB,
    SeismicJournal<DB>,
    SeismicChain,
>;

/// Trait that allows for a default context to be created.
pub trait DefaultSeismicContext {
    /// Create a default context.
    fn seismic() -> SeismicContext<SeismicEmptyDB>;
    /// Create a context with a specific RNG keypair.
    fn seismic_with_rng_key(rng_keypair: schnorrkel::Keypair) -> SeismicContext<SeismicEmptyDB>;
}

impl DefaultSeismicContext for SeismicContext<SeismicEmptyDB> {
    fn seismic() -> Self {
        Context::mainnet()
            .with_db(SeismicEmptyDB::new())
            .with_tx(SeismicTransaction::default())
            .with_cfg(CfgEnv::new_with_spec(SeismicSpecId::MERCURY))
            .with_chain(SeismicChain::default())
    }

    fn seismic_with_rng_key(rng_keypair: schnorrkel::Keypair) -> Self {
        Context::mainnet()
            .with_db(SeismicEmptyDB::new())
            .with_tx(SeismicTransaction::default())
            .with_cfg(CfgEnv::new_with_spec(SeismicSpecId::MERCURY))
            .with_chain(SeismicChain::with_live_rng_key(Some(rng_keypair)))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::api::builder::SeismicBuilder;
    use revm::{inspector::NoOpInspector, ExecuteEvm};

    #[test]
    fn default_run_seismic() {
        let ctx = Context::seismic();
        // convert to seismic context
        let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector {});
        // execute
        let _ = evm.replay();
        // inspect
        // TODO: needs a tx
        // let _ = evm.inspect();
    }
}
