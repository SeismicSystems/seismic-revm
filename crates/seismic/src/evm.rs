use crate::{
    api::exec::SeismicContextTr,
    instructions::instruction_provider::SeismicInstructions,
    precompiles::{mercury_with_extra, SeismicPrecompiles},
};
use revm::{
    context::{ContextError, ContextSetters, ContextTr, Evm, FrameStack},
    handler::{
        instructions::InstructionProvider, EthFrame, EvmTr, FrameInitOrResult, FrameTr,
        ItemOrResult, PrecompileProvider,
    },
    inspector::{InspectorEvmTr, JournalExt},
    interpreter::{interpreter::EthInterpreter, InterpreterResult},
    precompile::Precompiles,
    Database, Inspector,
};

pub struct SeismicEvm<
    CTX,
    INSP,
    I = SeismicInstructions<EthInterpreter, CTX>,
    P = SeismicPrecompiles<CTX>,
    F = EthFrame<EthInterpreter>,
>(pub Evm<CTX, INSP, I, P, F>);

impl<CTX: SeismicContextTr, INSP>
    SeismicEvm<CTX, INSP, SeismicInstructions<EthInterpreter, CTX>, SeismicPrecompiles<CTX>>
{
    pub fn new(ctx: CTX, inspector: INSP) -> Self {
        Self(Evm {
            ctx,
            inspector,
            instruction: SeismicInstructions::new_mainnet(),
            precompiles: SeismicPrecompiles::<CTX>::default(),
            frame_stack: FrameStack::new(),
        })
    }
}

impl<CTX: SeismicContextTr, I, INSP> SeismicEvm<CTX, INSP, I> {
    /// Create a new EVM instance with a given context, inspector, instruction set, and precompile provider.
    pub fn new_with_inspector(
        ctx: CTX,
        inspector: INSP,
        instruction: I,
        precompiles: &'static Precompiles,
    ) -> Self {
        let p = mercury_with_extra::<CTX>(Some(precompiles));
        Self(Evm {
            ctx,
            inspector,
            instruction,
            precompiles: SeismicPrecompiles::<CTX>::new(p),
            frame_stack: FrameStack::new(),
        })
    }
}

impl<CTX, INSP, I, P> InspectorEvmTr for SeismicEvm<CTX, INSP, I, P>
where
    CTX: SeismicContextTr<Journal: JournalExt> + ContextSetters,
    I: InstructionProvider<Context = CTX, InterpreterTypes = EthInterpreter>,
    P: PrecompileProvider<CTX, Output = InterpreterResult>,
    INSP: Inspector<CTX, I::InterpreterTypes>,
{
    type Inspector = INSP;

    fn inspector(&mut self) -> &mut Self::Inspector {
        &mut self.0.inspector
    }

    fn ctx_inspector(&mut self) -> (&mut Self::Context, &mut Self::Inspector) {
        (&mut self.0.ctx, &mut self.0.inspector)
    }

    fn ctx_inspector_frame(
        &mut self,
    ) -> (&mut Self::Context, &mut Self::Inspector, &mut Self::Frame) {
        (
            &mut self.0.ctx,
            &mut self.0.inspector,
            self.0.frame_stack.get(),
        )
    }

    fn ctx_inspector_frame_instructions(
        &mut self,
    ) -> (
        &mut Self::Context,
        &mut Self::Inspector,
        &mut Self::Frame,
        &mut Self::Instructions,
    ) {
        (
            &mut self.0.ctx,
            &mut self.0.inspector,
            self.0.frame_stack.get(),
            &mut self.0.instruction,
        )
    }
}

impl<CTX, INSP, I, P> EvmTr for SeismicEvm<CTX, INSP, I, P>
where
    CTX: SeismicContextTr,
    I: InstructionProvider<Context = CTX, InterpreterTypes = EthInterpreter>,
    P: PrecompileProvider<CTX, Output = InterpreterResult>,
{
    type Context = CTX;
    type Instructions = I;
    type Precompiles = P;
    type Frame = EthFrame<EthInterpreter>;

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

    fn frame_stack(&mut self) -> &mut FrameStack<Self::Frame> {
        &mut self.0.frame_stack
    }

    fn frame_init(
        &mut self,
        frame_input: <Self::Frame as FrameTr>::FrameInit,
    ) -> Result<
        ItemOrResult<&mut Self::Frame, <Self::Frame as FrameTr>::FrameResult>,
        ContextError<<<Self::Context as ContextTr>::Db as Database>::Error>,
    > {
        // Sum remaining gas across all active frames and stash on SeismicChain
        // so precompiles can read the total gas available across the call stack.
        let total_gas: u64 = self
            .0
            .frame_stack
            .active_frames()
            .iter()
            .map(|frame| frame.interpreter.gas.remaining())
            .sum();
        self.0
            .ctx
            .chain_mut()
            .set_gas_remaining_all_frames(total_gas);

        self.0.frame_init(frame_input)
    }

    fn frame_run(
        &mut self,
    ) -> Result<
        FrameInitOrResult<Self::Frame>,
        ContextError<<<Self::Context as ContextTr>::Db as Database>::Error>,
    > {
        self.0.frame_run()
    }

    #[doc = " Returns the result of the frame to the caller. Frame is popped from the frame stack."]
    #[doc = " Consumes the frame result or returns it if there is more frames to run."]
    fn frame_return_result(
        &mut self,
        result: <Self::Frame as FrameTr>::FrameResult,
    ) -> Result<
        Option<<Self::Frame as FrameTr>::FrameResult>,
        ContextError<<<Self::Context as ContextTr>::Db as Database>::Error>,
    > {
        self.0.frame_return_result(result)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unnecessary_cast
    )]

    use core::str::FromStr;

    use super::*;
    use crate::precompiles::rng;
    use crate::precompiles::rng::precompile::calculate_gas_cost;
    use crate::transaction::abstraction::SeismicTransaction;
    use crate::{
        DefaultSeismicContext, SeismicBuilder, SeismicChain, SeismicContext, SeismicRevertReason,
        SeismicSpecId,
    };
    use anyhow::bail;
    use revm::context::result::{ExecutionResult, Output, ResultAndState};
    use revm::context::{BlockEnv, CfgEnv, Context, ContextTr, JournalTr, TxEnv};
    use revm::database::{EmptyDB, InMemoryDB, BENCH_CALLER};
    use revm::interpreter::gas::calculate_initial_tx_gas;
    use revm::interpreter::InitialAndFloorGas;
    use revm::precompile::u64_to_address;
    use revm::primitives::{Address, Bytes, TxKind, U256};
    use revm::{ExecuteCommitEvm, ExecuteEvm, Journal};

    // === Fixture data ===

    /// Returns bytecode for a contract that does `sstore(0, 1)` then `cstore(0, 2)`.
    /// This tests CSTORE's InvalidPublicStorageAccess: cannot overwrite non-zero public slot.
    fn get_cstore_violation_bytecode() -> (Bytes, Bytes) {
        // Contract that does: sstore(0, 1) then cstore(0, 2)
        // The sstore makes slot 0 public with value 1
        // The cstore should fail because slot 0 is (1, public) - non-zero public
        let bytecode = Bytes::from_str(
            "6080604052348015600e575f5ffd5b5060d880601a5f395ff3fe6080604052348015600e\
             575f5ffd5b50600436106026575f3560e01c806326121ff014602a575b5f5ffd5b60306\
             044565b604051603b91906066565b60405180910390f35b5f60015f5560025fb1905090565b\
             5f819050919050565b6060816050565b82525050565b5f60208201905060775f830184\
             6059565b9291505056fea26469706673582212203976fb983ef7119eeabfd96d1698e9\
             bca8ad8a92c6f39e22bc2c6b412755a16864736f6c637827302e382e32382d63692e32\
             3032342e31312e342b636f6d6d69742e64396333323834372e6d6f640058",
        )
        .unwrap();
        let selector = Bytes::from_str("26121ff0").unwrap();
        (bytecode, selector)
    }

    /// Returns bytecode for a contract that does `cstore(0, 1)` then `sstore(0, 2)`.
    /// This tests SSTORE's InvalidPrivateStorageAccess: cannot write to private slot.
    fn get_sstore_violation_bytecode() -> (Bytes, Bytes) {
        // Contract that does: cstore(0, 1) then sstore(0, 2)
        // The cstore makes slot 0 private with value 1
        // The sstore should fail because slot 0 is private
        let bytecode = Bytes::from_str(
            "6080604052348015600e575f5ffd5b5060d880601a5f395ff3fe6080604052348015600e\
             575f5ffd5b50600436106026575f3560e01c806326121ff014602a575b5f5ffd5b60306\
             044565b604051603b91906066565b60405180910390f35b60015fb160025f55905090565b\
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

    fn deploy_contract_with_bytecode(
        bytecode: Bytes,
    ) -> anyhow::Result<(SeismicContext<InMemoryDB>, Address)> {
        let ctx = Context::seismic_with_random_rng_key()
            .modify_tx_chained(|tx| {
                tx.base.kind = TxKind::Create;
                tx.base.data = bytecode.clone();
            })
            .with_db(InMemoryDB::default());

        let mut evm = ctx.build_seismic_evm();
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

    fn assert_storage_access_reverts(
        result: &ResultAndState,
        expected_reason: SeismicRevertReason,
    ) {
        match &result.result {
            ExecutionResult::Revert { output, .. } => {
                assert_eq!(
                    *output,
                    expected_reason.revert_bytes(),
                    "Revert output should contain the reason"
                );
            }
            other => panic!("Expected Revert, received result: {:?}", other),
        }
    }

    /// Tests that CSTORE reverts when trying to overwrite a non-zero public
    /// slot (x, public) where x != 0.
    #[test]
    fn cstore_on_nonzero_public_slot_reverts() -> anyhow::Result<()> {
        let (bytecode, selector) = get_cstore_violation_bytecode();
        let (ctx, contract) = deploy_contract_with_bytecode(bytecode)?;

        let balance = 1_000_000;
        let gas_limit = 100_000;
        let gas_price = 10;

        let call_ctx = prepare_call(ctx, contract, selector, gas_limit, gas_price);

        let mut evm = call_ctx.build_seismic_evm();
        let account = evm.ctx().journal_mut().load_account(BENCH_CALLER).unwrap();
        account.data.info.balance = U256::from(balance);

        let result = evm.replay()?;

        assert_storage_access_reverts(&result, SeismicRevertReason::InvalidPublicStorageAccess);
        Ok(())
    }

    /// Tests that SSTORE reverts when trying to write to a private slot.
    #[test]
    fn sstore_on_private_slot_reverts() -> anyhow::Result<()> {
        let (bytecode, selector) = get_sstore_violation_bytecode();
        let (ctx, contract) = deploy_contract_with_bytecode(bytecode)?;

        let balance = 1_000_000;
        let gas_limit = 100_000;
        let gas_price = 10;

        let call_ctx = prepare_call(ctx, contract, selector, gas_limit, gas_price);

        let mut evm = call_ctx.build_seismic_evm();
        let account = evm.ctx().journal_mut().load_account(BENCH_CALLER).unwrap();
        account.data.info.balance = U256::from(balance);

        let result = evm.replay()?;

        assert_storage_access_reverts(&result, SeismicRevertReason::InvalidPrivateStorageAccess);
        Ok(())
    }

    fn rng_test_tx(
        spec: SeismicSpecId,
        bytes_requested: u32,
        personalization: Vec<u8>,
        rng_ikm: [u8; 64],
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

        let total_gas =
            initial_gas + calculate_gas_cost(bytes_requested as usize, personalization.len());

        Context::seismic_with_rng_key(rng_ikm)
            .modify_tx_chained(|tx| {
                tx.base.kind = TxKind::Call(u64_to_address(rng::precompile::RNG_ADDRESS));
                tx.base.data = input;
                tx.base.gas_limit = total_gas;
            })
            .modify_cfg_chained(|cfg| cfg.spec = spec)
    }

    #[test]
    fn test_rng_precompile_expected_output() {
        use seismic_crypto::well_known_rng_ikm;

        let bytes_requested: u32 = 32;
        let personalization = vec![0xAA, 0xBB, 0xCC, 0xDD];
        let ikm = well_known_rng_ikm();

        // Get EVM output
        let ctx = rng_test_tx(
            SeismicSpecId::MERCURY,
            bytes_requested,
            personalization.clone(),
            ikm,
        );

        let mut evm = ctx.build_seismic_evm();
        let output = evm.replay().unwrap();
        let evm_output = output.result.into_output().unwrap();

        // Verify output is 32 bytes and non-zero
        assert_eq!(evm_output.len(), 32, "RNG output should be 32 bytes");
        assert_ne!(
            evm_output,
            Bytes::from(vec![0u8; 32]),
            "RNG output should not be all zeros"
        );

        // Verify determinism: same inputs produce same output
        let ctx2 = rng_test_tx(
            SeismicSpecId::MERCURY,
            bytes_requested,
            personalization,
            ikm,
        );
        let mut evm2 = ctx2.build_seismic_evm();
        let output2 = evm2.replay().unwrap();
        let evm_output2 = output2.result.into_output().unwrap();
        assert_eq!(
            evm_output, evm_output2,
            "same inputs should produce same output"
        );
    }
}
