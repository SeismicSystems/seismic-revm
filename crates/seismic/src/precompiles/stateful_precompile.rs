use auto_impl::auto_impl;
use revm::context_interface::ContextTr;
use revm::interpreter::{Gas, InstructionResult, InterpreterResult};
use revm::precompile::{PrecompileError, PrecompileResult};
use revm::precompile::{PrecompileSpecId, Precompiles};
use revm::primitives::{hardfork::SpecId, Address, Bytes};
use std::collections::{HashMap, HashSet};

pub type StatefulPrecompileFn<CTX> = fn(&mut CTX, &Bytes, u64) -> PrecompileResult;

#[derive(Clone, Debug)]
pub struct StatefulPrecompileWithAddress<CTX>(pub Address, pub StatefulPrecompileFn<CTX>); 

#[derive(Clone, Debug)]
pub struct StatefulPrecompiles<CTX> {
    inner: HashMap<Address, StatefulPrecompileFn<CTX>>,
    addresses: HashSet<Address>,
}

impl<CTX> StatefulPrecompiles<CTX> {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
            addresses: HashSet::new(),
        }
    }

    pub fn insert(&mut self, address: Address, precompile: StatefulPrecompileFn<CTX>) {
        self.addresses.insert(address.clone());
        self.inner.insert(address, precompile);
    }

    pub fn extend<I: IntoIterator<Item = (Address, StatefulPrecompileFn<CTX>)>>(&mut self, iter: I) {
        for (address, precompile) in iter {
            self.insert(address, precompile);
        }
    }

    pub fn get(&self, address: &Address) -> Option<&StatefulPrecompileFn<CTX>> {
        self.inner.get(address)
    }

    pub fn contains(&self, address: &Address) -> bool {
        self.addresses.contains(address)
    }

    pub fn addresses(&self) -> impl Iterator<Item = &Address> {
        self.addresses.iter()
    }
}

impl<CTX> From<(Address, StatefulPrecompileFn<CTX>)> for StatefulPrecompileWithAddress<CTX> { // Fixed generic parameter
    fn from(value: (Address, StatefulPrecompileFn<CTX>)) -> Self {
        StatefulPrecompileWithAddress(value.0, value.1)
    }
}

impl<CTX> From<StatefulPrecompileWithAddress<CTX>> for (Address, StatefulPrecompileFn<CTX>) { // Fixed generic parameter
    fn from(value: StatefulPrecompileWithAddress<CTX>) -> Self {
        (value.0, value.1)
    }
}

impl<CTX> StatefulPrecompileWithAddress<CTX> { // Fixed generic parameter
    /// Returns reference of address.
    #[inline]
    pub fn address(&self) -> &Address {
        &self.0
    }
    
    /// Returns reference of precompile.
    #[inline]
    pub fn precompile(&self) -> &StatefulPrecompileFn<CTX> {
        &self.1
    }
}
