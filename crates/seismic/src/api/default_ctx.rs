use crate::SeismicSpecId;
use revm::{
    context::{BlockEnv, CfgEnv, TxEnv},
    database_interface::EmptyDB,
    Context, Journal, MainContext 
};

/// Type alias for the default context type of the OpEvm.
pub type SeismicContext<DB> =
    Context<BlockEnv, TxEnv, CfgEnv<SeismicSpecId>, DB, Journal<DB>>;

/// Trait that allows for a default context to be created.
pub trait DefaultSeismic {
    /// Create a default context.
    fn seismic() -> SeismicContext<EmptyDB>;
}

impl DefaultSeismic for SeismicContext<EmptyDB> {
    fn seismic() -> Self {
        Context::mainnet()
            .with_cfg(CfgEnv::new_with_spec(SeismicSpecId::MERCURY))
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
        // convert to optimism context
        let mut evm = ctx.build_seismic_with_inspector(NoOpInspector {});
        // execute
        let _ = evm.replay();
        // inspect
        let _ = evm.inspect_replay();
    }
}

