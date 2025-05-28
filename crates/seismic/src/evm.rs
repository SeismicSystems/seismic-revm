use crate::{
    api::exec::SeismicContextTr, instructions::instruction_provider::SeismicInstructions,
    precompiles::SeismicPrecompiles,
};
use revm::{
    context::{ContextSetters, Evm},
    handler::{instructions::InstructionProvider, EvmTr, PrecompileProvider},
    inspector::{InspectorEvmTr, JournalExt},
    interpreter::{interpreter::EthInterpreter, Interpreter, InterpreterAction, InterpreterTypes},
    Inspector,
};

pub struct SeismicEvm<
    CTX,
    INSP,
    I = SeismicInstructions<EthInterpreter, CTX>,
    P = SeismicPrecompiles<CTX>,
>(pub Evm<CTX, INSP, I, P>);

impl<CTX: SeismicContextTr, INSP>
    SeismicEvm<CTX, INSP, SeismicInstructions<EthInterpreter, CTX>, SeismicPrecompiles<CTX>>
{
    pub fn new(ctx: CTX, inspector: INSP) -> Self {
        Self(Evm {
            ctx,
            inspector,
            instruction: SeismicInstructions::new_mainnet(),
            precompiles: SeismicPrecompiles::<CTX>::default(),
        })
    }
}

impl<CTX, INSP, I, P> InspectorEvmTr for SeismicEvm<CTX, INSP, I, P>
where
    CTX: SeismicContextTr<Journal: JournalExt> + ContextSetters,
    I: InstructionProvider<
        Context = CTX,
        InterpreterTypes: InterpreterTypes<Output = InterpreterAction>,
    >,
    INSP: Inspector<CTX, I::InterpreterTypes>,
    P: PrecompileProvider<CTX>,
{
    type Inspector = INSP;

    fn inspector(&mut self) -> &mut Self::Inspector {
        &mut self.0.inspector
    }

    fn ctx_inspector(&mut self) -> (&mut Self::Context, &mut Self::Inspector) {
        (&mut self.0.ctx, &mut self.0.inspector)
    }

    fn run_inspect_interpreter(
        &mut self,
        interpreter: &mut Interpreter<
            <Self::Instructions as InstructionProvider>::InterpreterTypes,
        >,
    ) -> <<Self::Instructions as InstructionProvider>::InterpreterTypes as InterpreterTypes>::Output
    {
        self.0.run_inspect_interpreter(interpreter)
    }
}

impl<CTX, INSP, I, P> EvmTr for SeismicEvm<CTX, INSP, I, P>
where
    CTX: SeismicContextTr,
    I: InstructionProvider<
        Context = CTX,
        InterpreterTypes: InterpreterTypes<Output = InterpreterAction>,
    >,
    P: PrecompileProvider<CTX>,
{
    type Context = CTX;
    type Instructions = I;
    type Precompiles = P;

    fn run_interpreter(
        &mut self,
        interpreter: &mut Interpreter<
            <Self::Instructions as InstructionProvider>::InterpreterTypes,
        >,
    ) -> <<Self::Instructions as InstructionProvider>::InterpreterTypes as InterpreterTypes>::Output
    {
        let context = &mut self.0.ctx;
        let instructions = &mut self.0.instruction;
        interpreter.run_plain(instructions.instruction_table(), context)
    }

    fn ctx(&mut self) -> &mut Self::Context {
        &mut self.0.ctx
    }

    fn ctx_ref(&self) -> &Self::Context {
        &self.0.ctx
    }

    fn ctx_instructions(&mut self) -> (&mut Self::Context, &mut Self::Instructions) {
        (&mut self.0.ctx, &mut self.0.instruction)
    }

    fn ctx_precompiles(&mut self) -> (&mut Self::Context, &mut Self::Precompiles) {
        (&mut self.0.ctx, &mut self.0.precompiles)
    }
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use super::*;
    use crate::precompiles::rng;
    use crate::precompiles::rng::domain_sep_rng::RootRng;
    use crate::precompiles::rng::precompile::{calculate_fill_cost, calculate_init_cost};
    use crate::transaction::abstraction::SeismicTransaction;
    use crate::{
        DefaultSeismic, SeismicBuilder, SeismicChain, SeismicContext, SeismicHaltReason,
        SeismicSpecId,
    };
    use anyhow::bail;
    use rand_core::RngCore;
    use revm::context::result::{ExecutionResult, Output, ResultAndState};
    use revm::context::{BlockEnv, CfgEnv, Context, ContextTr, JournalTr, TxEnv};
    use revm::database::{EmptyDB, InMemoryDB, BENCH_CALLER};
    use revm::interpreter::gas::calculate_initial_tx_gas;
    use revm::interpreter::InitialAndFloorGas;
    use revm::precompile::u64_to_address;
    use revm::primitives::{Address, Bytes, TxKind, B256, U256};
    use revm::{ExecuteCommitEvm, ExecuteEvm, Journal};
    use seismic_enclave::get_unsecure_sample_schnorrkel_keypair;

    // === Fixture data ===

    fn get_meta_data() -> (Bytes, Bytes) {
        // bytecode for a contract whose function f() does `cload(0)` after `sstore(0,1)`
        let bytecode = Bytes::from_str(
            "6080604052348015600e575f5ffd5b5060d880601a5f395ff3fe6080604052348015600e\
             575f5ffd5b50600436106026575f3560e01c806326121ff014602a575b5f5ffd5b60306\
             044565b604051603b91906066565b60405180910390f35b5f60015f555fb0905090565b\
             5f819050919050565b6060816050565b82525050565b5f60208201905060775f830184\
             6059565b9291505056fea26469706673582212203976fb983ef7119eeabfd96d1698e9\
             bca8ad8a92c6f39e22bc2c6b412755a16864736f6c637827302e382e32382d63692e32\
             3032342e31312e342b636f6d6d69742e64396333323834372e6d6f640058",
        )
        .unwrap();
        let selector = Bytes::from_str("26121ff0").unwrap();
        (bytecode, selector)
    }

    // === Test helpers ===

    fn deploy_contract() -> anyhow::Result<(SeismicContext<InMemoryDB>, Address)> {
        let (bytecode, _) = get_meta_data();
        let ctx = Context::seismic()
            .modify_tx_chained(|tx| {
                tx.base.kind = TxKind::Create;
                tx.base.data = bytecode.clone();
            })
            .with_db(InMemoryDB::default());

        let mut evm = ctx.build_seismic();
        let receipt = evm.replay_commit()?;
        if let ExecutionResult::Success {
            output: Output::Create(_, Some(addr)),
            ..
        } = receipt
        {
            Ok((evm.ctx().clone(), addr))
        } else {
            bail!("Contract deployment failed: {receipt:#?}");
        }
    }

    fn prepare_call(
        ctx: SeismicContext<InMemoryDB>,
        contract: Address,
        selector: Bytes,
        gas_limit: u64,
        gas_price: u64,
    ) -> SeismicContext<InMemoryDB> {
        let mut ctx = ctx;

        ctx.modify_tx(|tx| {
            tx.base.kind = TxKind::Call(contract);
            tx.base.data = selector.clone();
            tx.base.gas_limit = gas_limit;
            tx.base.gas_price = gas_price as u128;
            tx.base.caller = BENCH_CALLER;
            tx.base.gas_priority_fee = None;
        });

        ctx
    }

    fn assert_cload_error(
        result: &ResultAndState<SeismicHaltReason>,
        starting_balance: u64,
        gas_limit: u64,
        gas_price: u64,
    ) {
        assert!(matches!(
            result.result,
            ExecutionResult::Halt {
                reason: SeismicHaltReason::InvalidPublicStorageAccess,
                ..
            }
        ));

        let expected = U256::from(starting_balance - gas_limit * gas_price);
        assert_eq!(
            result.state.get(&BENCH_CALLER).unwrap().info.balance,
            expected,
            "Caller balance after gas"
        );

        let final_nonce = result.state.get(&BENCH_CALLER).unwrap().info.nonce;
        assert_eq!(final_nonce, 1, "Caller nonce incremented by 1");
    }

    #[test]
    fn cload_access_violation_bubbles_up_and_charges_gas() -> anyhow::Result<()> {
        let (ctx, contract) = deploy_contract()?;

        let balance = 1_000_000;
        let gas_limit = 59_000;
        let gas_price = 10;

        let (_, selector) = get_meta_data();
        let call_ctx = prepare_call(ctx, contract, selector, gas_limit, gas_price);

        let mut evm = call_ctx.build_seismic();
        let account = evm.ctx().journal().load_account(BENCH_CALLER).unwrap();
        account.data.info.balance = U256::from(balance);

        let result = evm.replay()?;

        assert_cload_error(&result, balance, gas_limit, gas_price);
        Ok(())
    }

    fn rng_test_tx(
        spec: SeismicSpecId,
        bytes_requested: u32,
        personalization: Vec<u8>,
    ) -> Context<
        BlockEnv,
        SeismicTransaction<TxEnv>,
        CfgEnv<SeismicSpecId>,
        EmptyDB,
        Journal<EmptyDB>,
        SeismicChain,
    > {
        let mut input_data = bytes_requested.to_be_bytes().to_vec();
        input_data.extend(personalization.clone());
        let input = Bytes::from(input_data);

        let InitialAndFloorGas { initial_gas, .. } =
            calculate_initial_tx_gas(spec.into(), &input[..], false, 0, 0, 0);

        let total_gas = initial_gas
            + calculate_init_cost(personalization.len())
            + calculate_fill_cost(bytes_requested as usize);

        Context::seismic()
            .modify_tx_chained(|tx| {
                tx.base.kind = TxKind::Call(u64_to_address(rng::precompile::RNG_ADDRESS));
                tx.base.data = input;
                tx.base.gas_limit = total_gas;
            })
            .modify_cfg_chained(|cfg| cfg.spec = spec)
    }

    #[test]
    fn test_rng_precompile_expected_output_and_cleared() {
        // Variables
        let bytes_requested: u32 = 32;
        let personalization = vec![0xAA, 0xBB, 0xCC, 0xDD];

        // Get EVM output
        let ctx = rng_test_tx(
            SeismicSpecId::MERCURY,
            bytes_requested,
            personalization.clone(),
        );

        let mut evm = ctx.build_seismic();
        let output = evm.replay().unwrap();

        let evm_output = output.result.into_output().unwrap();

        // reconstruct expected output
        let root_rng = RootRng::test_default();
        root_rng.append_tx(&B256::default());
        let mut leaf_rng = root_rng.fork(&personalization);
        let mut rng_bytes = vec![0u8; bytes_requested as usize];
        leaf_rng.fill_bytes(&mut rng_bytes);
        assert_eq!(
            Bytes::from(rng_bytes),
            evm_output,
            "expected output and evm output should be equal"
        );

        // check root rng state is reset post execution
        let expected_root_rng_state = (
            get_unsecure_sample_schnorrkel_keypair().public.to_bytes(),
            true,
            true,
            0 as u64,
        );
        assert!(
            evm.ctx().chain().rng_container().leaf_rng().is_none(),
            "leaf rng should be none post execution"
        );
        assert_eq!(
            evm.ctx()
                .chain()
                .rng_container()
                .root_rng()
                .state_snapshot(),
            expected_root_rng_state,
            "root rng state should be as expected"
        );
    }
}
