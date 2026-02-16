use super::TransitionAccount;
use primitives::{hash_map::Entry, Address, HashMap, StorageValueTr, U256};
use std::vec::Vec;

/// State of accounts in transition between transaction executions.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct TransitionState<SV: StorageValueTr = U256> {
    /// Block state account with account state
    pub transitions: HashMap<Address, TransitionAccount<SV>>,
}

impl<SV: StorageValueTr> TransitionState<SV> {
    /// Create new transition state containing one [`TransitionAccount`].
    pub fn single(address: Address, transition: TransitionAccount<SV>) -> Self {
        let mut transitions = HashMap::default();
        transitions.insert(address, transition);
        TransitionState { transitions }
    }

    /// Take the contents of this [`TransitionState`] and replace it with an
    /// empty one.
    ///
    /// See [core::mem::take].
    pub fn take(&mut self) -> TransitionState<SV> {
        core::mem::take(self)
    }

    /// Add transitions to the transition state.
    ///
    /// This will insert new [`TransitionAccount`]s, or update existing ones via
    /// [`update`][TransitionAccount::update].
    pub fn add_transitions(&mut self, transitions: Vec<(Address, TransitionAccount<SV>)>) {
        for (address, account) in transitions {
            match self.transitions.entry(address) {
                Entry::Occupied(entry) => {
                    let entry = entry.into_mut();
                    entry.update(account);
                }
                Entry::Vacant(entry) => {
                    entry.insert(account);
                }
            }
        }
    }
}
