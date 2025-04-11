use crate::{api::exec::SeismicContextTr, instructions::instruction_provider::SeismicInstructions, precompiles::SeismicPrecompiles};
use revm::{
    context::{ContextSetters, Evm, EvmData},
    handler::{
        instructions::InstructionProvider,
        EvmTr,
    },
    inspector::{InspectorEvmTr, JournalExt},
    interpreter::{interpreter::EthInterpreter, Interpreter, InterpreterAction, InterpreterTypes},
    Inspector,
};

pub struct SeismicEvm<CTX, INSP, I = SeismicInstructions<EthInterpreter, CTX>, P = SeismicPrecompiles<CTX>>(
    pub Evm<CTX, INSP, I, P>,
);

impl<CTX: SeismicContextTr, INSP> SeismicEvm<CTX, INSP, SeismicInstructions<EthInterpreter, CTX>, SeismicPrecompiles<CTX>> {
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

