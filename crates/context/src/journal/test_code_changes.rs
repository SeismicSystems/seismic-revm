use super::{JournalEntry, JournalInner};
use bytecode::Bytecode;
use primitives::{Address, Bytes};
use state::{Account, AccountInfo};

#[test]
fn code_changes_restore_previous_code_and_hash_on_checkpoint_revert() {
    let address = Address::repeat_byte(0x11);
    let original_code = Bytecode::new_raw(Bytes::from_static(&[0x00]));
    for loaded in [false, true] {
        for touched in [false, true] {
            let original = AccountInfo {
                code_hash: original_code.hash_slow(),
                code: loaded.then(|| original_code.clone()),
                ..Default::default()
            };
            let mut account = Account::from(original.clone());
            if touched {
                account.mark_touch();
            }
            let mut journal = JournalInner::<JournalEntry>::new();
            journal.state.insert(address, account);
            let checkpoint = journal.checkpoint();
            journal.set_code(address, Bytecode::new_eip7702(Address::repeat_byte(0x22)));
            journal.set_code(address, Bytecode::default());
            journal.checkpoint_revert(checkpoint);
            let state = journal.finalize();
            let restored = &state[&address];
            assert_eq!(restored.info.code_hash, original.code_hash);
            assert_eq!(restored.info.code, original.code);
            assert_eq!(restored.is_touched(), touched);
        }
    }
}

#[test]
fn discarding_code_change_keeps_previous_committed_code() {
    let address = Address::repeat_byte(0x11);
    let mut journal = JournalInner::<JournalEntry>::new();
    journal.state.insert(address, Account::default());
    let committed_code = Bytecode::new_eip7702(Address::repeat_byte(0x22));
    journal.set_code(address, committed_code.clone());
    journal.commit_tx();
    journal.set_code(address, Bytecode::default());
    journal.discard_tx();
    let state = journal.finalize();
    let restored = &state[&address];
    assert_eq!(restored.info.code_hash, committed_code.hash_slow());
    assert_eq!(restored.info.code, Some(committed_code));
    assert!(restored.is_touched());
}
