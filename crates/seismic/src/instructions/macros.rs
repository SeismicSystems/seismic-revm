/// Check if a [`SeismicSpecId`] is enabled, and halt the instruction if it is not.
///
/// This works by converting the SeismicSpecId to its corresponding SpecId
/// (e.g., MERCURY -> PRAGUE) and checking against the interpreter's runtime spec.
/// This is a pragmatic approach: the interpreter only knows about Ethereum SpecIds,
/// while SeismicSpecId lives at the EVM context/handler layer (via `Cfg<Spec = SeismicSpecId>`).
///
/// If Seismic ever has multiple forks mapping to different Ethereum specs, this still works.
/// If two Seismic forks map to the *same* Ethereum spec, this approach would be insufficient
/// and we'd need to go through the host to access `ctx.cfg().spec()` directly (which is how
/// Optimism gates their fork-specific logic in handlers).
#[macro_export]
macro_rules! check {
    ($interpreter:expr, $min:ident) => {
        if !$interpreter
            .runtime_flag
            .spec_id()
            .is_enabled_in($crate::spec::SeismicSpecId::$min.into())
        {
            $interpreter.halt(revm::interpreter::InstructionResult::NotActivated);
            return;
        }
    };
}
