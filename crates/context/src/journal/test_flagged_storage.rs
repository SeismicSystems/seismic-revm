use super::entry::JournalEntry;
use super::inner::JournalInner;
use database::InMemoryDB;
use database_interface::Database;
use primitives::{hardfork::SpecId, Address, FlaggedStorage, U256};
use state::{Account, AccountInfo, AccountStatus, EvmStorage};

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

// Helper function to create test accounts with different statuses
fn create_test_account(status: AccountStatus) -> Account {
    Account {
        info: AccountInfo {
            nonce: 1,
            balance: U256::from(1000),
            ..Default::default()
        },
        transaction_id: 0,
        status,
        storage: EvmStorage::new(),
    }
}

// Helper function to setup journal with account
fn setup_journal_with_account(
    address: Address,
    status: AccountStatus,
) -> JournalInner<JournalEntry> {
    let mut journal = JournalInner::<JournalEntry>::new();
    journal.spec = SpecId::MERCURY;
    journal.state.insert(address, create_test_account(status));
    journal
}

// Helper function to verify storage state with detailed assertions
fn verify_storage_state(
    journal: &mut JournalInner<JournalEntry>,
    db: &mut InMemoryDB,
    address: Address,
    key: U256,
    expected_value: U256,
    expected_private: bool,
    context: &str,
) {
    let result = journal.sload(db, address, key, false).unwrap();
    assert_eq!(
        result.data, expected_value,
        "{}: Storage value mismatch. Expected {}, got {}",
        context, expected_value, result.data
    );
    assert_eq!(
        result.is_private, expected_private,
        "{}: Privacy flag mismatch. Expected {}, got {}",
        context, expected_private, result.is_private
    );
}

// Helper function to store data using appropriate operation based on shielded parameter
fn store_value(
    shielded: bool,
    journal: &mut JournalInner<JournalEntry>,
    db: &mut InMemoryDB,
    address: Address,
    key: U256,
    value: U256,
) -> context_interface::context::StateLoad<context_interface::context::SStoreResult> {
    if shielded {
        journal.cstore(db, address, key, value, false).unwrap()
    } else {
        journal.sstore(db, address, key, value, false).unwrap()
    }
}

// Helper function to load data using appropriate operation based on shielded parameter
fn load_value(
    shielded: bool,
    journal: &mut JournalInner<JournalEntry>,
    db: &mut InMemoryDB,
    address: Address,
    key: U256,
) -> context_interface::context::StateLoad<U256> {
    if shielded {
        journal.cload(db, address, key, false).unwrap()
    } else {
        journal.sload(db, address, key, false).unwrap()
    }
}

// Helper function to verify account status
fn verify_account_status(
    journal: &JournalInner<JournalEntry>,
    address: Address,
    expected_created: bool,
    context: &str,
) {
    if let Some(account) = journal.state.get(&address) {
        assert_eq!(
            account.is_created(),
            expected_created,
            "{}: Account creation status mismatch. Expected is_created={}, got is_created={}",
            context,
            expected_created,
            account.is_created()
        );

        // Additional verification of account status details
        if account.status.contains(AccountStatus::Created) {
            assert!(
                expected_created,
                "{}: Account status contains Created flag but expected is_created={}",
                context, expected_created
            );
        } else {
            assert!(
                !expected_created,
                "{}: Account status does not contain Created flag but expected is_created={}",
                context, expected_created
            );
        }
    } else {
        panic!("{}: Account not found in journal state", context);
    }
}

// =============================================================================
// TEST IMPLEMENTATION FUNCTIONS
// =============================================================================

// Implementation of simple revert test for both private and public storage
fn _test_storage_simple_revert(shielded: bool) {
    let mut db = InMemoryDB::default();
    let address = Address::from_slice(&[0x1; 20]);
    let storage_key = U256::from(1);

    let mut journal = setup_journal_with_account(address, AccountStatus::Created);

    // Verify initial state - should be empty
    verify_storage_state(
        &mut journal,
        &mut db,
        address,
        storage_key,
        U256::ZERO,
        false,
        "Initial state",
    );

    // Create checkpoint before storing data
    let checkpoint = journal.checkpoint();
    assert_eq!(journal.depth, 1, "Journal depth must be 1 after checkpoint");

    let storage_type = if shielded { "private" } else { "public" };
    let operation_name = if shielded { "CSTORE" } else { "SSTORE" };

    // Store data using appropriate operation
    let store_result = store_value(
        shielded,
        &mut journal,
        &mut db,
        address,
        storage_key,
        U256::from(42),
    );
    assert_eq!(
        store_result.is_private, shielded,
        "{} operation must mark storage as {}",
        operation_name, storage_type
    );
    assert_eq!(
        store_result.data.new_value.value,
        U256::from(42),
        "{} must store correct value",
        operation_name
    );
    assert_eq!(
        store_result.data.new_value.is_private, shielded,
        "{} must mark new value as {}",
        operation_name, storage_type
    );

    // Verify storage was stored with correct value using consistent load operation
    let before_revert = load_value(shielded, &mut journal, &mut db, address, storage_key);
    assert_eq!(
        before_revert.data,
        U256::from(42),
        "Storage must contain the stored value before revert"
    );
    assert_eq!(
        before_revert.is_private, shielded,
        "Storage must be marked {} before revert",
        storage_type
    );

    // Also verify the journal entry was created
    assert!(
        !journal.journal.is_empty(),
        "Journal must contain entries after {} operation",
        operation_name
    );

    // Revert to checkpoint
    journal.checkpoint_revert(checkpoint);
    assert_eq!(
        journal.depth, 0,
        "Journal depth must return to 0 after revert"
    );

    // Verify state after revert - should be back to original (zero/empty)
    let after_revert = load_value(shielded, &mut journal, &mut db, address, storage_key);
    assert_eq!(
        after_revert.data,
        U256::ZERO,
        "Storage value must be zero after reverting {} storage write",
        storage_type
    );

    if shielded {
        assert!(
            !after_revert.is_private,
            "BUGGY: Storage must be public (not private) after revert to empty state"
        );
    } else {
        assert!(
            !after_revert.is_private,
            "Storage must remain public after revert to empty state"
        );
    }
}

// Implementation of account creation with storage followed by revert
fn _test_account_creation_storage_revert(shielded: bool) {
    let mut db = InMemoryDB::default();
    let caller_address = Address::from_slice(&[0x1; 20]);
    let created_address = Address::from_slice(&[0x3; 20]);
    let storage_key = U256::from(1);

    let mut journal = JournalInner::<JournalEntry>::new();
    journal.spec = SpecId::MERCURY;

    let storage_type = if shielded { "private" } else { "public" };
    let operation_name = if shielded { "CSTORE" } else { "SSTORE" };

    // Setup caller account with sufficient balance
    journal.load_account(&mut db, caller_address).unwrap();
    let caller_account = journal.state.get_mut(&caller_address).unwrap();
    caller_account.info.balance = U256::from(10000);

    // Load target account (initially non-existing)
    journal.load_account(&mut db, created_address).unwrap();

    // Use proper high-level API to create account with journal entries
    let checkpoint = journal
        .create_account_checkpoint(
            caller_address,
            created_address,
            U256::from(1000), // Transfer 1000 from caller to created account
            SpecId::MERCURY,
        )
        .unwrap();

    // Verify account was properly created
    verify_account_status(
        &journal,
        created_address,
        true,
        "After create_account_checkpoint",
    );
    let created_account = journal.state.get(&created_address).unwrap();
    assert_eq!(
        created_account.info.balance,
        U256::from(1000),
        "Created account must have transferred balance"
    );

    // Store data in the newly created account
    let store_result = store_value(
        shielded,
        &mut journal,
        &mut db,
        created_address,
        storage_key,
        U256::from(99),
    );
    assert_eq!(
        store_result.is_private, shielded,
        "{} in new account must mark storage as {}",
        operation_name, storage_type
    );

    // Verify storage exists with correct value and privacy
    let read_result = load_value(
        shielded,
        &mut journal,
        &mut db,
        created_address,
        storage_key,
    );
    assert_eq!(
        read_result.data,
        U256::from(99),
        "Storage must contain the stored value"
    );
    assert_eq!(
        read_result.is_private, shielded,
        "Storage must be marked {} after {}",
        storage_type, operation_name
    );

    // Revert account creation using the checkpoint from create_account_checkpoint
    journal.checkpoint_revert(checkpoint);

    // Verify account creation was properly reverted
    verify_account_status(&journal, created_address, false, "After checkpoint revert");
    if let Some(account) = journal.state.get(&created_address) {
        assert_eq!(
            account.info.balance,
            U256::ZERO,
            "Account balance must be reverted to zero"
        );

        // Verify storage was also reverted
        let read_after_revert = load_value(
            shielded,
            &mut journal,
            &mut db,
            created_address,
            storage_key,
        );
        assert_eq!(
            read_after_revert.data,
            U256::ZERO,
            "Storage must be zero after account creation revert"
        );
        assert!(
            !read_after_revert.is_private,
            "Storage must be public after account creation revert"
        );
    }

    // Verify caller balance was properly reverted
    let caller_after_revert = journal.state.get(&caller_address).unwrap();
    assert_eq!(
        caller_after_revert.info.balance,
        U256::from(10000),
        "Caller balance must be reverted to original amount"
    );
}

// Implementation of nested checkpoint reverts with storage
fn _test_nested_checkpoint_storage_reverts(shielded: bool) {
    let mut db = InMemoryDB::default();
    let address = Address::from_slice(&[0x4; 20]);
    let key1 = U256::from(1);
    let key2 = U256::from(2);

    let mut journal = setup_journal_with_account(address, AccountStatus::Created);

    let storage_type = if shielded { "private" } else { "public" };
    let operation_name = if shielded { "CSTORE" } else { "SSTORE" };

    // Verify initial state
    verify_storage_state(
        &mut journal,
        &mut db,
        address,
        key1,
        U256::ZERO,
        false,
        "Initial key1 state",
    );
    verify_storage_state(
        &mut journal,
        &mut db,
        address,
        key2,
        U256::ZERO,
        false,
        "Initial key2 state",
    );
    assert_eq!(journal.depth, 0, "Initial journal depth must be 0");

    // Level 0: Store initial value
    let initial_store = store_value(
        shielded,
        &mut journal,
        &mut db,
        address,
        key1,
        U256::from(10),
    );
    assert_eq!(
        initial_store.is_private, shielded,
        "Level 0 {} must mark storage as {}",
        operation_name, storage_type
    );
    verify_storage_state(
        &mut journal,
        &mut db,
        address,
        key1,
        U256::from(10),
        shielded,
        "After level 0 store",
    );

    // Level 1: Create checkpoint and store more data
    let checkpoint1 = journal.checkpoint();
    assert_eq!(
        journal.depth, 1,
        "Journal depth must be 1 after first checkpoint"
    );
    let journal_size_cp1 = journal.journal.len();

    let modify_store = store_value(
        shielded,
        &mut journal,
        &mut db,
        address,
        key1,
        U256::from(20),
    ); // Modify existing
    assert_eq!(
        modify_store.is_private, shielded,
        "Level 1 key1 {} must maintain {} flag",
        operation_name, storage_type
    );
    assert_eq!(
        modify_store.data.present_value.value,
        U256::from(10),
        "Level 1 key1 {} must see previous value",
        operation_name
    );

    let new_store = store_value(
        shielded,
        &mut journal,
        &mut db,
        address,
        key2,
        U256::from(30),
    ); // New key
    assert_eq!(
        new_store.is_private, shielded,
        "Level 1 key2 {} must mark storage as {}",
        operation_name, storage_type
    );

    verify_storage_state(
        &mut journal,
        &mut db,
        address,
        key1,
        U256::from(20),
        shielded,
        "After level 1 key1 store",
    );
    verify_storage_state(
        &mut journal,
        &mut db,
        address,
        key2,
        U256::from(30),
        shielded,
        "After level 1 key2 store",
    );

    // Level 2: Create another checkpoint and store even more data
    let checkpoint2 = journal.checkpoint();
    assert_eq!(
        journal.depth, 2,
        "Journal depth must be 2 after second checkpoint"
    );
    let journal_size_cp2 = journal.journal.len();

    let modify_again = store_value(
        shielded,
        &mut journal,
        &mut db,
        address,
        key1,
        U256::from(40),
    ); // Modify again
    assert_eq!(
        modify_again.is_private, shielded,
        "Level 2 {} must maintain {} flag",
        operation_name, storage_type
    );
    assert_eq!(
        modify_again.data.present_value.value,
        U256::from(20),
        "Level 2 {} must see level 1 value",
        operation_name
    );

    // Verify current state at deepest level
    verify_storage_state(
        &mut journal,
        &mut db,
        address,
        key1,
        U256::from(40),
        shielded,
        "Deepest level key1",
    );
    verify_storage_state(
        &mut journal,
        &mut db,
        address,
        key2,
        U256::from(30),
        shielded,
        "Deepest level key2",
    );

    // Revert level 2 (should restore key1 to 20, keep key2 at 30)
    journal.checkpoint_revert(checkpoint2);
    assert_eq!(
        journal.depth, 1,
        "Journal depth must be 1 after level 2 revert"
    );
    assert_eq!(
        journal.journal.len(),
        journal_size_cp2,
        "Journal size must revert to checkpoint 2"
    );

    verify_storage_state(
        &mut journal,
        &mut db,
        address,
        key1,
        U256::from(20),
        shielded,
        "After level 2 revert key1",
    );
    verify_storage_state(
        &mut journal,
        &mut db,
        address,
        key2,
        U256::from(30),
        shielded,
        "After level 2 revert key2",
    );

    // Revert level 1 (should restore key1 to 10, key2 to 0)
    journal.checkpoint_revert(checkpoint1);
    assert_eq!(
        journal.depth, 0,
        "Journal depth must be 0 after level 1 revert"
    );
    assert_eq!(
        journal.journal.len(),
        journal_size_cp1,
        "Journal size must revert to checkpoint 1"
    );

    verify_storage_state(
        &mut journal,
        &mut db,
        address,
        key1,
        U256::from(10),
        shielded,
        "After level 1 revert key1",
    );

    // Check key2 after complete revert - behavior differs between private and public
    let key2_after_revert = load_value(shielded, &mut journal, &mut db, address, key2);
    assert_eq!(
        key2_after_revert.data,
        U256::ZERO,
        "key2 must revert to zero after level 1 revert"
    );

    if shielded {
        assert!(
            !key2_after_revert.is_private,
            "BUGGY: key2 must be public (empty) after level 1 revert for private storage"
        );
    } else {
        assert!(
            !key2_after_revert.is_private,
            "key2 must be public (empty) after level 1 revert for public storage"
        );
    }
}

// =============================================================================
// TESTS
// =============================================================================

/// Test that demonstrates the database correctly preserves privacy flags
#[test]
fn test_database_privacy_flag_preservation() {
    let mut db = InMemoryDB::default();
    let address = Address::from_slice(&[0x2; 20]);
    let storage_key = U256::from(1);

    // Store private data directly in database
    let private_storage = FlaggedStorage::from(U256::from(42)).mark_private();
    db.insert_account_storage(address, storage_key, private_storage)
        .unwrap();

    // Verify database preserves privacy flag and value correctly
    let db_result = db.storage(address, storage_key).unwrap();
    assert!(
        db_result.is_private,
        "Database must preserve private flag for stored data"
    );
    assert_eq!(
        db_result.value,
        U256::from(42),
        "Database must preserve stored value"
    );

    // Verify journal ZERO has expected defaults
    let journal_zero = FlaggedStorage::ZERO.set_visibility(false);
    assert!(
        !journal_zero.is_private,
        "FlaggedStorage::ZERO must be public by default"
    );
    assert_eq!(
        journal_zero.value,
        U256::ZERO,
        "FlaggedStorage::ZERO must have zero value"
    );

    // Test both private and public storage in database
    let public_storage = FlaggedStorage::from(U256::from(99));
    let public_key = U256::from(2);
    db.insert_account_storage(address, public_key, public_storage)
        .unwrap();

    let public_result = db.storage(address, public_key).unwrap();
    assert!(
        !public_result.is_private,
        "Database must preserve public flag for public data"
    );
    assert_eq!(
        public_result.value,
        U256::from(99),
        "Database must preserve public stored value"
    );

    // Demonstrate the privacy flag inconsistency potential
    assert_ne!(
        db_result.is_private,
        journal_zero.is_private,
        "Database storage and ZERO storage must have different privacy flags - this demonstrates the potential for inconsistency"
    );
    assert_ne!(
        private_storage.is_private, public_storage.is_private,
        "Private and public storage must have different privacy flags"
    );
}

/// Test mixed private/public storage reverts to ensure privacy flags are handled correctly
#[test]
fn test_mixed_storage_revert() {
    let mut db = InMemoryDB::default();
    let address = Address::from_slice(&[0x2; 20]);
    let private_key = U256::from(1);
    let public_key = U256::from(2);

    let mut journal = setup_journal_with_account(address, AccountStatus::Created);

    // Verify initial state for both keys
    verify_storage_state(
        &mut journal,
        &mut db,
        address,
        private_key,
        U256::ZERO,
        false,
        "Initial private key state",
    );
    verify_storage_state(
        &mut journal,
        &mut db,
        address,
        public_key,
        U256::ZERO,
        false,
        "Initial public key state",
    );

    // Store initial public data using SSTORE
    let initial_public_store = store_value(
        false,
        &mut journal,
        &mut db,
        address,
        public_key,
        U256::from(100),
    );
    assert!(
        !initial_public_store.is_private,
        "SSTORE operation must mark storage as public"
    );
    assert_eq!(
        initial_public_store.data.new_value.value,
        U256::from(100),
        "SSTORE must store correct value"
    );

    // Verify public storage was set correctly
    verify_storage_state(
        &mut journal,
        &mut db,
        address,
        public_key,
        U256::from(100),
        false,
        "After initial SSTORE",
    );

    // Create checkpoint
    let checkpoint = journal.checkpoint();
    let initial_journal_size = journal.journal.len();

    // Store private data using CSTORE
    let private_store = store_value(
        true,
        &mut journal,
        &mut db,
        address,
        private_key,
        U256::from(200),
    );
    assert!(
        private_store.is_private,
        "CSTORE operation must mark storage as private"
    );
    assert_eq!(
        private_store.data.new_value.value,
        U256::from(200),
        "CSTORE must store correct value"
    );

    // Modify existing public data using SSTORE
    let public_modify = store_value(
        false,
        &mut journal,
        &mut db,
        address,
        public_key,
        U256::from(300),
    );
    assert!(
        !public_modify.is_private,
        "SSTORE operation must keep storage public"
    );
    assert_eq!(
        public_modify.data.present_value.value,
        U256::from(100),
        "SSTORE must see previous value correctly"
    );
    assert_eq!(
        public_modify.data.new_value.value,
        U256::from(300),
        "SSTORE must store new value correctly"
    );

    // Verify both storages exist with correct values and privacy flags before revert
    // Use consistent read operations: CLOAD for private, SLOAD for public
    let private_read = load_value(true, &mut journal, &mut db, address, private_key);
    let public_read = load_value(false, &mut journal, &mut db, address, public_key);
    assert_eq!(
        private_read.data,
        U256::from(200),
        "Private storage must contain correct value before revert"
    );
    assert!(
        private_read.is_private,
        "Private storage must be marked private before revert"
    );
    assert_eq!(
        public_read.data,
        U256::from(300),
        "Public storage must contain correct value before revert"
    );
    assert!(
        !public_read.is_private,
        "Public storage must be marked public before revert"
    );

    // Verify journal entries were added
    assert!(
        journal.journal.len() > initial_journal_size,
        "Journal must have new entries after storage operations"
    );

    // Revert to checkpoint
    journal.checkpoint_revert(checkpoint);

    // Verify state after revert: private should be gone, public should be restored
    // Use consistent read operations again
    let private_after = load_value(true, &mut journal, &mut db, address, private_key);
    let public_after = load_value(false, &mut journal, &mut db, address, public_key);

    assert_eq!(
        private_after.data,
        U256::ZERO,
        "Private storage value must be zero after revert"
    );
    assert!(
        !private_after.is_private,
        "BUGGY: Reverted private storage must be public (empty)"
    );
    assert_eq!(
        public_after.data,
        U256::from(100),
        "Public storage value must be reverted to original"
    );
    assert!(
        !public_after.is_private,
        "Public storage must remain public after revert"
    );

    // Verify journal was properly reverted
    assert_eq!(
        journal.journal.len(),
        initial_journal_size,
        "Journal must be reverted to checkpoint size"
    );
}

/// Test private storage behavior during simple revert scenarios
#[test]
fn test_private_storage_simple_revert() {
    _test_storage_simple_revert(true);
}

/// Test public storage behavior during simple revert scenarios
#[test]
fn test_public_storage_simple_revert() {
    _test_storage_simple_revert(false);
}

/// Test account creation with private storage followed by revert using proper high-level APIs
#[test]
fn test_account_creation_private_storage_revert() {
    _test_account_creation_storage_revert(true);
}

/// Test account creation with public storage followed by revert using proper high-level APIs
#[test]
fn test_account_creation_public_storage_revert() {
    _test_account_creation_storage_revert(false);
}

/// Test nested checkpoint reverts with private storage
#[test]
fn test_nested_checkpoint_private_storage_reverts() {
    _test_nested_checkpoint_storage_reverts(true);
}

/// Test nested checkpoint reverts with public storage
#[test]
fn test_nested_checkpoint_public_storage_reverts() {
    _test_nested_checkpoint_storage_reverts(false);
}
