/// Check if the `SeismicSPEC` is enabled, and fail the instruction if it is not.
#[macro_export]
macro_rules! check {
    ($interpreter:expr, $min:ident) => {
        if !$interpreter
            .runtime_flag
            .spec_id()
            .is_enabled_in(crate::spec::SeismicSpecId::$min)
        {
            $interpreter
                .control
                .set_instruction_result($crate::InstructionResult::NotActivated);
            return;
        }
    };
}

