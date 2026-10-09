use crate::{
    transaction::abstraction::SeismicTransaction, SeismicBlockEnv, SeismicChain, SeismicSpecId,
};
use revm::{
    context::{CfgEnv, TxEnv},
    database_interface::EmptyDB,
    Context, Journal, MainContext,
};

/// Type alias for the default context type of the SeismicEvm.
pub type SeismicContext<DB> = Context<
    SeismicBlockEnv,
    SeismicTransaction<TxEnv>,
    CfgEnv<SeismicSpecId>,
    DB,
    Journal<DB>,
    SeismicChain,
>;

/// Trait that allows for a default context to be created.
pub trait DefaultSeismicContext {
    /// Create a context that uses a random rng key for precompile everytime.
    fn seismic_with_random_rng_key() -> SeismicContext<EmptyDB>;
    /// Create a context with specific RNG input key material.
    fn seismic_with_rng_key(rng_ikm: [u8; 64]) -> SeismicContext<EmptyDB>;
}

impl DefaultSeismicContext for SeismicContext<EmptyDB> {
    fn seismic_with_random_rng_key() -> Self {
        Context::mainnet()
            .with_block(SeismicBlockEnv::default())
            .with_tx(SeismicTransaction::default())
            .with_cfg(CfgEnv::new_with_spec(SeismicSpecId::MERCURY))
            .with_chain(SeismicChain::with_random_rng_key())
    }

    fn seismic_with_rng_key(rng_ikm: [u8; 64]) -> Self {
        Context::mainnet()
            .with_block(SeismicBlockEnv::default())
            .with_tx(SeismicTransaction::default())
            .with_cfg(CfgEnv::new_with_spec(SeismicSpecId::MERCURY))
            .with_chain(SeismicChain::with_live_rng_key(rng_ikm))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::api::builder::SeismicBuilder;
    use revm::{inspector::NoOpInspector, ExecuteEvm};

    #[test]
    fn default_run_seismic() {
        let ctx = Context::seismic_with_random_rng_key();
        // convert to seismic context
        let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector {});
        // execute
        let _ = evm.replay();
        // inspect
        // TODO: needs a tx
        // let _ = evm.inspect();
    }
}
