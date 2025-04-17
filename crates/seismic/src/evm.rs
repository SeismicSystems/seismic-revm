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
    use crate::{DefaultSeismic, SeismicBuilder, SeismicHaltReason};
    use revm::context::{Context, ContextTr};
    use revm::context::result::{ExecutionResult, Output};
    use revm::database::{InMemoryDB, BENCH_CALLER};
    use revm::primitives::{Bytes, TxKind, U256};
    
    fn get_mata_data() -> (Bytes, Bytes) {
        // Create bytecode that will execute CLOAD
        //contract C {
        //    function f() external returns (uint256 result) {
        //        assembly {
        //            sstore(0, 1)
        //            result := cload(0)
        //        }
        //    }
        //}
        let bytecode = Bytes::from_str("6080604052348015600e575f5ffd5b5060d880601a5f395ff3fe6080604052348015600e575f5ffd5b50600436106026575f3560e01c806326121ff014602a575b5f5ffd5b60306044565b604051603b91906066565b60405180910390f35b5f60015f555fb0905090565b5f819050919050565b6060816050565b82525050565b5f60208201905060775f8301846059565b9291505056fea26469706673582212203976fb983ef7119eeabfd96d1698e9bca8ad8a92c6f39e22bc2c6b412755a16864736f6c637827302e382e32382d63692e323032342e31312e342b636f6d6d69742e64396333323834372e6d6f640058").unwrap();
        let function_selector = Bytes::from_str("26121ff0").unwrap();
        (bytecode, function_selector)
    }
    
    #[test]
    fn test_cload_error_bubbles_up() -> anyhow::Result<()> {
        let (bytecode, function_selector) = get_mata_data();

        let ctx = Context::seismic()
            .modify_tx_chained(|tx| {
            tx.base.kind = TxKind::Create;
            tx.base.data = bytecode.clone();
        })
        .with_db(InMemoryDB::default());
        
        let mut evm = ctx.build_seismic();
        let ref_tx = evm.replay_commit()?;
            let ExecutionResult::Success {
                output: Output::Create(_, Some(address)),
                ..
            } = ref_tx
            else {
                bail!("Failed to create contract: {ref_tx:#?}");
            };
        

        let account_balance = 1_000_000;
        let gas_limit = 59_000;
        let gas_price = 10;

        let account = evm.ctx().journal().load_account(BENCH_CALLER).unwrap();
        account.data.info.balance = U256::from(account_balance);

        evm.ctx().modify_tx(|tx| {
            tx.base.kind = TxKind::Call(address);
            tx.base.data = function_selector;
            tx.base.gas_limit = gas_limit;
            tx.base.gas_priority_fee = None;
            tx.base.gas_price = gas_price;
            tx.base.caller = BENCH_CALLER;
        });
        
        let result = evm.replay()?;

        // Check correct execution result 
        assert!(matches!(
            result.result,
            ExecutionResult::Halt {
                reason: SeismicHaltReason::InvalidPublicStorageAccess,
                ..
            } 
        ));

        // Check correct State Output
    
        // Check if the balance got deducted by gas_limit * gas_price 
        assert_eq!(result.state.get(&BENCH_CALLER).unwrap().info.balance,
            U256::from(account_balance - gas_limit * gas_price as u64));
        
        // Check correct nonce increment 
        assert_eq!(result.state.get(&BENCH_CALLER).unwrap().info.nonce,
            1 as u64);

        Ok(())
    }
    
    //#[test]
    //fn test_cload_handler_error_handling() {
    //    // Create the test context
    //    let ctx = create_cload_test_context();
    //    
    //    // Build the SeismicEvm with a host that will trigger the error
    //    // You'll need to implement a test host that returns a value where:
    //    // value.is_private == false && !value.data.is_zero()
    //    let mut evm = create_evm_with_error_triggering_host(ctx);
    //    
    //    // Create the handler
    //    let handler = SeismicHandler::
    //        SeismicEvm<_, _>, 
    //        EVMError<_, _>, 
    //        EthFrame<_, _, _>
    //    >::new();
    //    
    //    // Execute a frame with CLOAD instruction
    //    let frame_input = FrameInput::new(
    //        Address::ZERO,
    //        Address::ZERO,
    //        Bytes::from_static(&[0x60, 0x0A, 0xB0]),
    //        U256::ZERO,
    //        u64::MAX
    //    );
    //    
    //    let result = handler.execute_frame(&mut evm, frame_input);
    //    
    //    // Check that the error was properly handled
    //    assert!(matches!(
    //        result,
    //        Err(EVMError::Custom(reason)) if reason == "rekt"
    //    ));
    //}
}
