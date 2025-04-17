use crate::{
    api::exec::SeismicContextTr, instructions::instruction_provider::SeismicInstructions,
    precompiles::SeismicPrecompiles,
};
use revm::{
    context::{ContextSetters, Evm, EvmData},
    handler::{instructions::InstructionProvider, EvmTr},
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
            data: EvmData { ctx, inspector },
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
{
    type Inspector = INSP;

    fn inspector(&mut self) -> &mut Self::Inspector {
        &mut self.0.data.inspector
    }

    fn ctx_inspector(&mut self) -> (&mut Self::Context, &mut Self::Inspector) {
        (&mut self.0.data.ctx, &mut self.0.data.inspector)
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
        let context = &mut self.0.data.ctx;
        let instructions = &mut self.0.instruction;
        interpreter.run_plain(instructions.instruction_table(), context)
    }

    fn ctx(&mut self) -> &mut Self::Context {
        &mut self.0.data.ctx
    }

    fn ctx_ref(&self) -> &Self::Context {
        &self.0.data.ctx
    }

    fn ctx_instructions(&mut self) -> (&mut Self::Context, &mut Self::Instructions) {
        (&mut self.0.data.ctx, &mut self.0.instruction)
    }

    fn ctx_precompiles(&mut self) -> (&mut Self::Context, &mut Self::Precompiles) {
        (&mut self.0.data.ctx, &mut self.0.precompiles)
    }
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use super::*;
    use anyhow::bail;
    use revm::{ExecuteCommitEvm, ExecuteEvm};
    use crate::{DefaultSeismic, SeismicBuilder, SeismicContext, SeismicHaltReason};
    use revm::context::{Context, ContextTr};
    use revm::context::result::{ExecutionResult, Output, ResultAndState};
    use revm::database::{InMemoryDB, BENCH_CALLER};
    use revm::primitives::{Bytes, TxKind, U256, Address};


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
             3032342e31312e342b636f6d6d69742e64396333323834372e6d6f640058"
        ).unwrap();
        let selector = Bytes::from_str("26121ff0").unwrap();
        (bytecode, selector)
    }

    // === Test helpers ===

    /// Deploys the test contract and returns its address.
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
        if let ExecutionResult::Success { output: Output::Create(_, Some(addr)), .. } = receipt {
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

    /// Asserts that the CLLOAD error bubbled up and gas & nonce are handled correctly.
    fn assert_cload_error(
        result: &ResultAndState<SeismicHaltReason>,
        starting_balance: u64,
        gas_limit: u64,
        gas_price: u64,
    ) {
        // error bubbled
        assert!(matches!(
            result.result,
            ExecutionResult::Halt {
                reason: SeismicHaltReason::InvalidPublicStorageAccess,
                ..
            }
        ));

        // balance deduction
        let expected = U256::from(starting_balance - gas_limit * gas_price);
        assert_eq!(result.state.get(&BENCH_CALLER).unwrap().info.balance, expected, "Caller balance after gas");

        // nonce increment
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
        let call_ctx = prepare_call(
            ctx,
            contract,
            selector,
            gas_limit,
            gas_price,
        );

        let mut evm = call_ctx.build_seismic();

        let account = evm.ctx().journal().load_account(BENCH_CALLER).unwrap();
        account.data.info.balance = U256::from(balance);

        let result = evm.replay()?;

        assert_cload_error(&result, balance, gas_limit, gas_price);
        Ok(())
    }
}
