use crate::src20_gas::gas_contract_account_info;
use crate::src20_gas::GAS_SRC20_ADDRESS;
use crate::{transaction::abstraction::SeismicTransaction, SeismicChain, SeismicSpecId};
use revm::context::ContextTr;
use revm::database::InMemoryDB;
use revm::{
    context::{BlockEnv, CfgEnv, TxEnv},
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

pub type DefaultSeismicDB = InMemoryDB;

/// Trait that allows for a default context to be created.
pub trait DefaultSeismicContext {
    /// Create a default context.
    fn seismic() -> SeismicContext<DefaultSeismicDB>;
}

impl DefaultSeismicContext for SeismicContext<DefaultSeismicDB> {
    fn seismic() -> Self {
        let mut ctx = Context::mainnet()
            .with_tx(SeismicTransaction::default())
            .with_cfg(CfgEnv::new_with_spec(SeismicSpecId::MERCURY))
            .with_chain(SeismicChain::default())
            .with_db(DefaultSeismicDB::default());

        // Set up the seismic gas contract
        let gas_contract_info = gas_contract_account_info();
        ctx.db()
            .insert_account_info(GAS_SRC20_ADDRESS, gas_contract_info);

        ctx
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
        let _ = evm.inspect_replay();
    }
}
