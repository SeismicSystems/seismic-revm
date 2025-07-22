//! This module contains the logic for the GAS_SRC20 token contract.
//! Seismic uses a custom ERC20 token contract to handle gas fees

use alloy_sol_types::SolValue;
use anyhow::Result;
use revm::{
    bytecode::Bytecode,
    context_interface::{
        result::{InvalidHeader, InvalidTransaction},
        ContextTr, JournalTr,
    },
    primitives::{address, bytes, keccak256, Address, Bytes, U256},
    state::AccountInfo,
    Database,
};

// ********** GAS_SRC20 constants **********
/// The address of the GAS_SRC20 contract
pub const GAS_SRC20_ADDRESS: Address = address!("1000000000000000000000000000000000000003");
/// The storage slot of the balances mapping in the GAS_SRC20 contract
pub const GAS_SRC20_BALANCE_SLOT: u64 = 0;
/// The code of the GAS_SRC20 contract
pub const GAS_SRC20_CODE: Bytes = bytes!("0x608060405234801561000f575f5ffd5b50600436106100fb575f3560e01c80635687f2b81161009357806395d89b411161006357806395d89b4114610218578063aa8beaf014610238578063be09129c1461024b578063f43064fb1461025e575f5ffd5b80635687f2b8146101a85780635f84a3cd146101df57806388412c20146101f25780638cdb7b3414610205575f5ffd5b8063221b7180116100ce578063221b71801461019557806323de6651146101a8578063313ce567146101bd5780633ab43673146101cc575f5ffd5b806306fdde03146100ff57806314f228031461013857806318160ddd1461015b5780632176518a1461016b575b5f5ffd5b60408051808201909152600b81526a577261707065642047617360a81b60208201525b60405161012f9190610734565b60405180910390f35b61014b610146366004610780565b610271565b604051901515815260200161012f565b5f5b60405190815260200161012f565b61017e6101793660046107aa565b6102ef565b60408051921515835260208301919091520161012f565b61015d6101a33660046107e1565b61035d565b6101bb6101b6366004610803565b505050565b005b6040516012815260200161012f565b61017e6101da3660046107e1565b6103a2565b61015d6101ed3660046107aa565b6103dc565b61014b610200366004610780565b610437565b61014b610213366004610780565b61044e565b6040805180820190915260048152635747415360e01b6020820152610122565b61014b610246366004610780565b61045b565b61014b610259366004610803565b610495565b61014b61026c366004610780565b6104b8565b335f8181526001602090815260408083206001600160a01b03871684529091528120b0909190838110156102d557604051637dc7a0d960e11b81526001600160a01b03861660048201525f6024820181905260448201526064015b60405180910390fd5b6102e282868684036104cc565b6001925050505b92915050565b5f80336001600160a01b03851681148061031a5750836001600160a01b0316816001600160a01b0316145b1561034e575050506001600160a01b038083165f9081526001602081815260408084209486168452939052919020b0610356565b5f5f92509250505b9250929050565b5f336001600160a01b0383160361038957506001600160a01b03165f908152602081905260409020b090565b60405163263e159360e11b815260040160405180910390fd5b5f80336001600160a01b038416036103d25750506001600160a01b03165f908152602081905260409020b0600191565b505f928392509050565b5f336001600160a01b0384168114806104065750826001600160a01b0316816001600160a01b0316145b156103895750506001600160a01b038083165f908152600160209081526040808320938516835292905220b06102e9565b5f336104448185856104cc565b5060019392505050565b5f3361044481858561054e565b335f8181526001602090815260408083206001600160a01b03871684529091528120b09091906102e282866104908785610855565b6104cc565b5f336104a28582856105ab565b6104ad85858561054e565b506001949350505050565b5f6104c38383610625565b50600192915050565b6001600160a01b0383166104f55760405163e602df0560e01b81525f60048201526024016102cc565b6001600160a01b03821661051e57604051634a1406b160e11b81525f60048201526024016102cc565b6001600160a01b038084165f9081526001602090815260408083209386168352929052208190b16101b683838383565b6001600160a01b03831661057757604051634b637e8f60e11b81525f60048201526024016102cc565b6001600160a01b0382166105a05760405163ec442f0560e01b81525f60048201526024016102cc565b6101b683838361065d565b6001600160a01b038084165f908152600160209081526040808320938616835292905220b05f1981101561061f578181101561061257604051637dc7a0d960e11b81526001600160a01b03841660048201525f6024820181905260448201526064016102cc565b61061f84848484036104cc565b50505050565b6001600160a01b03821661064e5760405163ec442f0560e01b81525f60048201526024016102cc565b6106595f838361065d565b5050565b6001600160a01b038316610687578060025f82825461067c9190610855565b909155506106f69050565b6001600160a01b0383165f908152602081905260409020b0818110156106d85760405163391434e360e21b81526001600160a01b03851660048201525f6024820181905260448201526064016102cc565b6001600160a01b0384165f9081526020819052604090209082900390b15b6001600160a01b03821661071257600280548290039055505050565b6001600160a01b0382165f90815260208190526040902080b0820190b1505050565b602081525f82518060208401528060208501604085015e5f604082850101526040601f19601f83011684010191505092915050565b6001600160a01b038116811461077d575f5ffd5b50565b5f5f60408385031215610791575f5ffd5b823561079c81610769565b946020939093013593505050565b5f5f604083850312156107bb575f5ffd5b82356107c681610769565b915060208301356107d681610769565b809150509250929050565b5f602082840312156107f1575f5ffd5b81356107fc81610769565b9392505050565b5f5f5f60608486031215610815575f5ffd5b833561082081610769565b9250602084013561083081610769565b929592945050506040919091013590565b634e487b7160e01b5f52601160045260245ffd5b808201808211156102e9576102e961084156fea26469706673582212204a01d1adaa4e7bd539c389daeea5cb99e1fd2f73b65a98e415c7b17605f932c764736f6c637828302e382e32382d646576656c6f702e323032352e322e31332b636f6d6d69742e39363462353035320059");
/// The amount of gas covered by a single GAS_SRC20
/// The goal is to denominate the token in USD, and have 21000 GAS cost 1 cent
pub const GAS_SRC20_CONVERATION_RATIO: u64 = 2100000; // TODO: actually use this somewhere
/// Treasury address for gas fees
pub const TREASURY: Address = Address::new([0u8; 20]); // TODO: update this to a seismic controlled address

/// Computes the storage slot for solidity mapping
///
/// This function calculates the keccak256 hash of the concatenated padded address and slot number,
/// which is the standard way to compute storage slots for Solidity mappings.
///
/// # Arguments
/// * `address` - The address to compute the storage slot for
/// * `slot_number` - The slot number (usually 0 for _balances mapping)
///
/// # Returns
/// The storage slot as a B256 (32-byte array)
pub fn map_storage(address: Address, slot: u64) -> U256 {
    keccak256((address, U256::from(slot)).abi_encode()).into()
}

pub fn gas_caller_key(caller: Address) -> U256 {
    map_storage(caller, GAS_SRC20_BALANCE_SLOT)
}

/// Transfers SRC20 gas tokens from one address to another by modifying the balances mapping in storage.
///
/// This function performs a confidential token transfer operation using Seismic's confidential storage
/// operations (`cload` and `cstore`). It transfers the specified amount from the sender's balance to
/// the recipient's balance within the SRC20 token contract's storage.
///
/// # Returns
///
/// Returns `Ok(())` on successful transfer, or an error if:
/// * The sender has insufficient balance
/// * A storage operation fails
pub fn gas_token_operation<CTX, ERROR>(
    context: &mut CTX,
    sender: Address,
    recipient: Address,
    amount: U256,
) -> Result<(), ERROR>
where
    CTX: ContextTr,
    ERROR: From<InvalidTransaction> + From<InvalidHeader> + From<<CTX::Db as Database>::Error>,
{
    let sender_balance_slot = gas_caller_key(sender);
    let sender_balance = context
        .journal()
        .cload(GAS_SRC20_ADDRESS, sender_balance_slot)?
        .data;

    if sender_balance < amount {
        return Err(ERROR::from(InvalidTransaction::LackOfFundForMaxFee {
            fee: Box::new(amount),
            balance: Box::new(sender_balance),
        }));
    }
    // Subtract the amount from the sender's balance
    let sender_new_balance = sender_balance.saturating_sub(amount);
    context
        .journal()
        .cstore(GAS_SRC20_ADDRESS, sender_balance_slot, sender_new_balance)?;

    // Add the amount to the recipient's balance
    let recipient_balance_slot = gas_caller_key(recipient);
    let recipient_balance = context
        .journal()
        .cload(GAS_SRC20_ADDRESS, recipient_balance_slot)?
        .data;

    let recipient_new_balance = recipient_balance.saturating_add(amount);
    context.journal().cstore(
        GAS_SRC20_ADDRESS,
        recipient_balance_slot,
        recipient_new_balance,
    )?;

    Ok(())
}

/// Sets the balance of a given address to a specific amount in the GAS_SRC20 token contract.
pub fn gas_set_balance<CTX, ERROR>(
    context: &mut CTX,
    address: Address,
    amount: U256,
) -> Result<(), ERROR>
where
    CTX: ContextTr,
    ERROR: From<InvalidTransaction> + From<InvalidHeader> + From<<CTX::Db as Database>::Error>,
{
    let balance_slot = gas_caller_key(address);
    context
        .journal()
        .cstore(GAS_SRC20_ADDRESS, balance_slot, amount)?;

    Ok(())
}

/// Get the balance of an address from the GAS_SRC20 token contract
///
/// This reads the active balance from the journal, as opposed to the db's persistent balance
///
/// # Returns
/// The balance as U256 wrapped in a Result
pub fn gas_balance_of<CTX, ERROR>(context: &mut CTX, address: Address) -> Result<U256, ERROR>
where
    CTX: ContextTr,
    ERROR: From<InvalidTransaction> + From<InvalidHeader> + From<<CTX::Db as Database>::Error>,
{
    let key = gas_caller_key(address);
    let storage = context.journal().cload(GAS_SRC20_ADDRESS, key)?;
    Ok(storage.data)
}

/// Creates the expected AccountInfo for the GAS_SRC20 contract.
///
/// This function creates the account info with the proper bytecode and code hash
/// for the gas contract, which can then be inserted into a database.
pub fn gas_contract_account_info() -> revm::state::AccountInfo {
    // Create the bytecode from the hex string
    let bytecode = Bytecode::new_raw(GAS_SRC20_CODE);
    let code_hash = bytecode.hash_slow();

    // Create the account info for the gas contract
    AccountInfo {
        balance: U256::ZERO,
        nonce: 0,
        code: Some(bytecode),
        code_hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::builder::SeismicBuilder;
    use crate::api::default_ctx::{DefaultSeismicContext, DefaultSeismicDB, SeismicContext};
    use revm::context::result::{EVMError, InvalidTransaction};
    use revm::handler::EvmTr;
    use revm::primitives::FlaggedStorage;
    use revm::primitives::TxKind;
    use revm::primitives::{Bytes, U256};
    use revm::InspectCommitEvm;
    use std::convert::Infallible;

    /// Helper function to create the call data for a transfer transaction
    fn transfer_call_data(recipient: Address, amount: U256) -> Bytes {
        // Function selector for transfer(saddress,suint256)
        let selector = bytes!("0x8cdb7b34");

        // ABI encode just the arguments (not including selector)
        let encoded_args = (recipient, amount).abi_encode();

        // Concatenate selector and encoded args
        let mut data = selector.to_vec();
        data.extend_from_slice(&encoded_args);

        data.into()
    }

    /// Test good path for a transfer transaction
    #[test]
    fn test_transfer_good_path() {
        // Create a context with the gas contract
        let mut ctx = SeismicContext::<DefaultSeismicDB>::seismic();

        // Set up test addresses
        let sender = Address::from([0x01; 20]);
        let recipient = Address::from([0x02; 20]);
        let treasury = TREASURY;

        // Set initial balance
        let sender_initial_balance = U256::from(1000000);
        let sender_balance_slot = gas_caller_key(sender);
        ctx.db()
            .insert_account_storage(
                GAS_SRC20_ADDRESS,
                sender_balance_slot,
                FlaggedStorage::new(sender_initial_balance, true),
            )
            .unwrap();
        JournalTr::load_account(ctx.journal(), GAS_SRC20_ADDRESS).unwrap();

        // make a transfer tx
        let transfer_amount = U256::from(5000);
        let call_data = transfer_call_data(recipient, transfer_amount);
        let tx = ctx.modify_tx_chained(|tx| {
            tx.base.kind = TxKind::Call(GAS_SRC20_ADDRESS);
            tx.base.caller = sender;
            tx.base.data = call_data;
            tx.base.gas_limit = 100000;
            tx.base.gas_price = 1;
        });
        let inspector = revm::inspector::NoOpInspector::default(); // use revm::inspector::inspectors::TracerEip3155::new_stdout() for more detailed output
        let mut evm = tx.build_seismic_evm_with_inspector(inspector);
        let result = evm.inspect_replay_commit().unwrap();

        assert!(
            matches!(
                result,
                revm::context::result::ExecutionResult::Success { .. }
            ),
            "Transaction should succeed. Result: {:?}",
            result
        );

        // Check the balances in the resulting evm context
        let mut post_tx_ctx = evm.ctx().clone();
        JournalTr::load_account(post_tx_ctx.journal(), GAS_SRC20_ADDRESS).unwrap();
        let sender_final_balance =
            gas_balance_of::<_, EVMError<Infallible, InvalidTransaction>>(&mut post_tx_ctx, sender)
                .unwrap();
        let recipient_final_balance =
            gas_balance_of::<_, EVMError<Infallible, InvalidTransaction>>(
                &mut post_tx_ctx,
                recipient,
            )
            .unwrap();
        let treasury_final_balance = gas_balance_of::<_, EVMError<Infallible, InvalidTransaction>>(
            &mut post_tx_ctx,
            treasury,
        )
        .unwrap();

        assert_eq!(recipient_final_balance, transfer_amount);
        let total_token = sender_final_balance + recipient_final_balance + treasury_final_balance;
        assert_eq!(
            total_token, sender_initial_balance,
            "Total token should be conserved in a tx"
        );
        assert!(sender_final_balance < sender_initial_balance - transfer_amount);
    }

    /// Test that beneficiary rewards are distributed correctly
    #[test]
    fn test_beneficiary_rewards() {}

    /// Test that if gas + transfer is over the limit, the transaction fails
    #[test]
    fn test_gas_plus_transfer_over_the_limit() {}

    /// Test that state is persisted correctly across multiple transactions
    #[test]
    fn test_gas_across_multiple_transactions() {}

    /// Test that if the storage slot is not written, the balance is 0
    #[test]
    fn test_gas_balance_of_unwritten_storage_returns_zero() {
        // Create a context with the gas contract
        let mut ctx = SeismicContext::<DefaultSeismicDB>::seismic();

        // Try to get balance for an address that has never had any balance set
        // This should return 0 even though the storage slot has never been written to
        let test_address = Address::from([0x42; 20]);

        // This should return 0, not panic
        JournalTr::load_account(ctx.journal(), GAS_SRC20_ADDRESS).unwrap();
        let balance =
            gas_balance_of::<_, EVMError<Infallible, InvalidTransaction>>(&mut ctx, test_address)
                .unwrap();
        assert_eq!(
            balance,
            U256::ZERO,
            "Balance should be zero for unwritten storage slot"
        );
    }
}
