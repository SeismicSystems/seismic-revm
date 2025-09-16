use crate::{transaction::abstraction::SeismicTransaction, SeismicChain, SeismicSpecId};
use revm::{
    context::{BlockEnv, CfgEnv, TxEnv},
    database_interface::EmptyDB,
    Context, Journal, MainContext,
};

/// Type alias for the default context type of the SeismicEvm.
pub type SeismicContext<DB> = Context<
    BlockEnv,
    SeismicTransaction<TxEnv>,
    CfgEnv<SeismicSpecId>,
    DB,
    Journal<DB>,
    SeismicChain,
>;

/// Trait that allows for a default context to be created.
pub trait DefaultSeismicContext {
    /// Create a default context.
    fn seismic() -> SeismicContext<EmptyDB>;
    /// Create a context with a specific RNG keypair.
    fn seismic_with_rng_key(rng_keypair: schnorrkel::Keypair) -> SeismicContext<EmptyDB>;
}

impl DefaultSeismicContext for SeismicContext<EmptyDB> {
    fn seismic() -> Self {
        Context::mainnet()
            .with_tx(SeismicTransaction::default())
            .with_cfg(CfgEnv::new_with_spec(SeismicSpecId::MERCURY))
            .with_chain(SeismicChain::default())
    }

    fn seismic_with_rng_key(rng_keypair: schnorrkel::Keypair) -> Self {
        Context::mainnet()
            .with_tx(SeismicTransaction::default())
            .with_cfg(CfgEnv::new_with_spec(SeismicSpecId::MERCURY))
            .with_chain(SeismicChain::with_live_rng_key(Some(rng_keypair)))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::api::builder::SeismicBuilder;
    use revm::{
        inspector::{InspectEvm, NoOpInspector},
        ExecuteEvm,
    };

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
