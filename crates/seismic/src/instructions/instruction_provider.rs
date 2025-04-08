use revm::{handler::instructions::InstructionProvider, interpreter::{
    instructions::{instruction_table, InstructionTable}, Host, Instruction, InterpreterTypes
}};
use std::boxed::Box;

/// Custom opcodes for CLOAD and CSTORE
pub const CLOAD: u8 = 0xB0;
pub const CSTORE: u8 = 0xB1;

/// Seismic instruction provider that adds our instruction set 
pub struct SeismicInstructions<WIRE: InterpreterTypes, HOST> {
    pub instruction_table: Box<InstructionTable<WIRE, HOST>>,
}

impl<WIRE, HOST> SeismicInstructions<WIRE, HOST>
where
    WIRE: InterpreterTypes,
    HOST: Host,
{
    /// Create a new SeismicInstructions with standard EVM opcodes plus our ISA
    pub fn new_mainnet(
        cload_handler: Instruction<WIRE, HOST>,
        cstore_handler: Instruction<WIRE, HOST>,
    ) -> Self {
        let mut table = instruction_table::<WIRE, HOST>();
        
        // Insert our custom instructions
        table[CLOAD as usize] = cload_handler;
        table[CSTORE as usize] = cstore_handler;
        
        Self {
            instruction_table: Box::new(table),
        }
    }

    /// Create a new SeismicInstructions from a provided base table
    pub fn new(
        mut base_table: InstructionTable<WIRE, HOST>,
        cload_handler: Instruction<WIRE, HOST>,
        cstore_handler: Instruction<WIRE, HOST>,
    ) -> Self {
        // Insert our custom instructions
        base_table[CLOAD as usize] = cload_handler;
        base_table[CSTORE as usize] = cstore_handler;
        
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
    use crate::instructions::confidential_storage::{cload, cstore};
    use super::*;
    use revm::interpreter::{
        host::DummyHost,
        interpreter::{Interpreter, EthInterpreter},
        instructions::control,
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
        let seismic_instructions = SeismicInstructions::<EthInterpreter, DummyHost>::new_mainnet(
            cload,
            cstore,
        );
        
        // Get reference to the instruction table
        let table = seismic_instructions.instruction_table();
        
        // Get the standard unknown instruction for comparison
        let unknown_instruction = control::unknown::<EthInterpreter, DummyHost>;
        
        // Verify CLOAD is not the unknown instruction
        assert!(!instructions_equal(table[CLOAD as usize], unknown_instruction),
            "CLOAD (0xB0) should not be the unknown instruction");
        
        // Verify CSTORE is not the unknown instruction
        assert!(!instructions_equal(table[CSTORE as usize], unknown_instruction),
                "CSTORE (0xB1) should not be the unknown instruction");
        
        // Verify CLOAD is our cload
        assert!(instructions_equal(table[CLOAD as usize], cload),
                "CLOAD (0xB0) should be our cload handler");
        
        // Verify CSTORE is our cstore
        assert!(instructions_equal(table[CSTORE as usize], cstore),
                "CSTORE (0xB1) should be our cstore handler");
    }
    
    #[test]
    fn test_instruction_provider_implementation() {
        // Create a SeismicInstructions
        let seismic_instructions = SeismicInstructions::<EthInterpreter, DummyHost>::new_mainnet(
            cload,
            cstore,
        );
        
        // Access the instruction table through the InstructionProvider trait
        let table = seismic_instructions.instruction_table();
        
        // Verify we can access our custom opcodes through the trait
        assert!(instructions_equal(table[CLOAD as usize], cload),
                "Should be able to access CLOAD through InstructionProvider trait");
        assert!(instructions_equal(table[CSTORE as usize], cstore),
                "Should be able to access CSTORE through InstructionProvider trait");
    }
    
    #[test]
    fn test_insert_instruction() {
        // Create a base SeismicInstructions
        let mut seismic_instructions = SeismicInstructions::<EthInterpreter, DummyHost>::new_mainnet(
            cload,
            cstore,
        );
        
        // Create an alternative handler
        fn alternative_handler<W, H>(_: &mut Interpreter<W>, _: &mut H)
        where
            W: InterpreterTypes,
            H: Host,
        {
            // Empty alternative implementation
        }
        
        // Override the CLOAD instruction
        seismic_instructions.insert_instruction(CLOAD, alternative_handler);
        
        // Verify the override worked
        let table = seismic_instructions.instruction_table();
        assert!(instructions_equal(table[CLOAD as usize], alternative_handler),
                "CLOAD should be updated to alternative_handler");
        assert!(instructions_equal(table[CSTORE as usize], cstore),
                "CSTORE should remain unchanged");
    }
    
    #[test]
    fn test_new_constructor() {
        // Get a standard instruction table
        let base_table = instruction_table::<EthInterpreter, DummyHost>();
        
        // Create a SeismicInstructions using the new constructor
        let seismic_instructions = SeismicInstructions::<EthInterpreter, DummyHost>::new(
            base_table,
            cload,
            cstore,
        );
        
        // Verify our custom opcodes were inserted
        let table = seismic_instructions.instruction_table();
        assert!(instructions_equal(table[CLOAD as usize], cload),
                "CLOAD should be added to the base table");
        assert!(instructions_equal(table[CSTORE as usize], cstore),
                "CSTORE should be added to the base table");
    }
    
    #[test]
    fn test_preserve_original_instructions() {
        // Get a standard instruction table
        let standard_table = instruction_table::<EthInterpreter, DummyHost>();
        
        // Create a SeismicInstructions
        let seismic_instructions = SeismicInstructions::<EthInterpreter, DummyHost>::new_mainnet(
            cload,
            cstore,
        );
        
        // Get our custom table
        let custom_table = seismic_instructions.instruction_table();
        
        // Verify all standard opcodes remain unchanged (except our custom ones)
        for i in 0..256 {
            if i != CLOAD as usize && i != CSTORE as usize {
                assert!(instructions_equal(custom_table[i], standard_table[i]),
                        "Opcode 0x{:X?} should remain unchanged", i);
            }
        }
    }
}
