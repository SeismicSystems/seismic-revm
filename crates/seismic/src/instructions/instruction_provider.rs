use revm::{
    bytecode::opcode::{SLOAD, SSTORE},
    handler::instructions::InstructionProvider,
    interpreter::{
        instructions::{instruction_table, InstructionTable},
        Host, Instruction, InterpreterTypes,
    },
};
use std::boxed::Box;

use crate::{instructions::confidential_storage::{cload_instruction, cstore_instruction, seismic_cstore_instruction, seismic_sload_instruction, seismic_sstore_instruction}, SeismicHost};

use super::confidential_storage::{
    cload, cstore, sload as seismic_sload, sstore as seismic_sstore,
};

/// Custom opcodes for CLOAD and CSTORE
pub const CLOAD: u8 = 0xB0;
pub const CSTORE: u8 = 0xB1;

/// Seismic instruction provider that adds our instruction set
pub struct SeismicInstructions<WIRE: InterpreterTypes, HOST> {
    pub instruction_table: Box<InstructionTable<WIRE, HOST>>,
}

impl<WIRE: InterpreterTypes, HOST: SeismicHost> Default for SeismicInstructions<WIRE, HOST> {
    fn default() -> Self {
        Self::new_mainnet()
    }
}

impl<WIRE, HOST> SeismicInstructions<WIRE, HOST>
where
    WIRE: InterpreterTypes,
    HOST: SeismicHost,
{
    /// Create a new SeismicInstructions with standard EVM opcodes plus our ISA
    pub fn new_mainnet() -> Self {
        let mut table = instruction_table::<WIRE, HOST>();

        // NOTE: static_gas is 0 because gas is dynamic for these
        table[CLOAD as usize] = cload_instruction();
        table[CSTORE as usize] = cstore_instruction();
        table[SLOAD as usize] = seismic_sload_instruction();
        table[SSTORE as usize] = seismic_sstore_instruction();

        Self {
            instruction_table: Box::new(table),
        }
    }

    /// Create a new SeismicInstructions from a provided base table
    pub fn new(base_table: InstructionTable<WIRE, HOST>) -> Self {
        Self {
            instruction_table: Box::new(base_table),
        }
    }

    /// Method to insert or override a single instruction
    pub fn insert_instruction(&mut self, opcode: u8, instruction: Instruction<WIRE, HOST>) {
        self.instruction_table[opcode as usize] = instruction;
    }
}

/// Implement InstructionProvider trait for SeismicInstructions
impl<IT, CTX> InstructionProvider for SeismicInstructions<IT, CTX>
where
    IT: InterpreterTypes,
    CTX: Host,
{
    type InterpreterTypes = IT;
    type Context = CTX;

    fn instruction_table(&self) -> &InstructionTable<Self::InterpreterTypes, Self::Context> {
        &self.instruction_table
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instructions::{
        confidential_storage::{cload, cstore},
        seismic_host::SeismicDummyHost,
    };
    use revm::interpreter::{
        instructions::control,
        interpreter::{EthInterpreter, Interpreter}, InstructionContext,
    };
    use std::mem;

    fn instructions_equal<W, H>(a: Instruction<W, H>, b: Instruction<W, H>) -> bool
    where
        W: InterpreterTypes,
        H: Host,
    {
        // mem::transmute: convert function pointers to raw addresses for comparison
        let a_ptr: usize = unsafe { mem::transmute(a) };
        let b_ptr: usize = unsafe { mem::transmute(b) };
        a_ptr == b_ptr
    }

    #[test]
    fn test_custom_opcodes_are_registered() {
        // Create a SeismicInstructions with our mock handlers
        let seismic_instructions =
            SeismicInstructions::<EthInterpreter, SeismicDummyHost>::new_mainnet();

        // Get reference to the instruction table
        let table = seismic_instructions.instruction_table();

        // Get the standard unknown instruction for comparison
        let unknown_instruction = Instruction::new(control::unknown::<EthInterpreter, SeismicDummyHost>, 0);

        // Verify CLOAD is not the unknown instruction
        assert!(
            !instructions_equal(table[CLOAD as usize], unknown_instruction),
            "CLOAD (0xB0) should not be the unknown instruction"
        );

        // Verify CSTORE is not the unknown instruction
        assert!(
            !instructions_equal(table[CSTORE as usize], unknown_instruction),
            "CSTORE (0xB1) should not be the unknown instruction"
        );

        // Verify CLOAD is our cload
        assert!(
            instructions_equal(table[CLOAD as usize], cload_instruction()),
            "CLOAD (0xB0) should be our cload handler"
        );

        // Verify CSTORE is our cstore
        assert!(
            instructions_equal(table[CSTORE as usize], cstore_instruction()),
            "CSTORE (0xB1) should be our cstore handler"
        );

        // Verify SSTORE is our SSTORE
        assert!(
            instructions_equal(table[SSTORE as usize], seismic_sstore_instruction()),
            "CLOAD (0xB0) should be our cload handler"
        );

        // Verify SLOAD is our SLOAD
        assert!(
            instructions_equal(table[SLOAD as usize], seismic_sload_instruction()),
            "CLOAD (0xB0) should be our cload handler"
        );
    }

    #[test]
    fn test_insert_instruction() {
        // Create a base SeismicInstructions
        let mut seismic_instructions =
            SeismicInstructions::<EthInterpreter, SeismicDummyHost>::new_mainnet();

        // Create an alternative handler
        fn alternative_handler<W, H>(_: InstructionContext<'_, H, W>)
        where
            W: InterpreterTypes,
            H: Host,
        {
        }

        let alt_handler_instruction = Instruction::new(alternative_handler, 0);

        // Override the CLOAD instruction
        seismic_instructions.insert_instruction(CLOAD, alt_handler_instruction);

        // Verify the override worked
        let table = seismic_instructions.instruction_table();
        assert!(
            instructions_equal(table[CLOAD as usize], alt_handler_instruction),
            "CLOAD should be updated to alternative_handler"
        );
        assert!(
            instructions_equal(table[CSTORE as usize], cstore_instruction()),
            "CSTORE should remain unchanged"
        );
    }

    #[test]
    fn test_new_constructor() {
        // Get a standard instruction table
        let base_table = instruction_table::<EthInterpreter, SeismicDummyHost>();

        // Create a SeismicInstructions using the new constructor
        let seismic_instructions =
            SeismicInstructions::<EthInterpreter, SeismicDummyHost>::new(base_table);

        // Verify our custom opcodes weren't inserted
        let table = seismic_instructions.instruction_table();
        assert!(
            !instructions_equal(table[CLOAD as usize], cload_instruction()),
            "CLOAD shouldn't be added to the base table by default"
        );
        assert!(
            !instructions_equal(table[CSTORE as usize], cstore_instruction()),
            "CSTORE shouldn't be added to the base table by default"
        );
    }

    #[test]
    fn test_preserve_original_instructions() {
        // Get a standard instruction table
        let standard_table = instruction_table::<EthInterpreter, SeismicDummyHost>();

        // Create a SeismicInstructions
        let seismic_instructions =
            SeismicInstructions::<EthInterpreter, SeismicDummyHost>::new_mainnet();

        // Get our custom table
        let custom_table = seismic_instructions.instruction_table();

        // Verify all standard opcodes remain unchanged (except our custom ones)
        for i in 0..256 {
            if i != CLOAD as usize
                && i != CSTORE as usize
                && i != SLOAD as usize
                && i != SSTORE as usize
            {
                assert!(
                    instructions_equal(custom_table[i], standard_table[i]),
                    "Opcode 0x{:X?} should remain unchanged",
                    i
                );
            }
        }
    }
}
