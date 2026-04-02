//!Handler related to Seismic chain
use crate::{api::exec::SeismicContextTr, transaction::abstraction::SeismicTxTr};
use revm::{
    Database, context::{
        Block as _, Cfg as _, ContextTr, JournalTr, LocalContextTr, result::{ExecutionResult, InvalidTransaction}
    }, context_interface::{
        context::ContextError,
        result::{FromStringError, HaltReason},
        transaction::Transaction,
    }, handler::{
        EthFrame, EvmTr, FrameResult, FrameTr, Handler, MainnetHandler, handler::EvmTrError, post_execution, pre_execution::validate_account_nonce_and_code
    }, inspector::{Inspector, InspectorEvmTr, InspectorHandler}, interpreter::{
        CallOutcome, Gas, InitialAndFloorGas, InstructionResult, InterpreterResult, interpreter::EthInterpreter, interpreter_action::FrameInit
    }, primitives::{Address, Bytes, U256, address, keccak256}
};



/// ERC20 token address used for gas payment on Seismic.
/// TODO: replace with the actual Seismic token contract address.
pub const TOKEN: Address = address!("0x215dfD51D1e6C05C1f7e322c0f9ddc607300e053");

/// Treasury address: collects gas payments and disburses reimbursements/rewards.
/// TODO: replace with the actual Seismic treasury address.
pub const TREASURY: Address = address!("0x0000000000000000000000000000000000000169");

/// Divisor to convert 18-decimal wei amounts to 6-decimal USDC amounts.
/// USDC uses 6 decimals while ETH/wei uses 18, so we divide by 10^(18-6) = 10^12.
const WEI_TO_USDC_DIVISOR: U256 = U256::from_limbs([1_000_000_000_000u64, 0, 0, 0]);

/// Returns the Solady ERC20 `_balances` storage slot for `address`.
/// Matches the Solady `_BALANCE_SLOT_SEED` (`0x87a211a2`) layout:
///   mstore(0x0c, 0x87a211a2)
///   mstore(0x00, owner)
///   slot := keccak256(0x0c, 0x20)
/// This produces: keccak256(addr[20 bytes] ++ 0x0000000000000087a211a2[12 bytes])
pub(crate) fn erc_address_storage(addr: Address) -> U256 {
    let mut buf = [0u8; 32];
    buf[0..20].copy_from_slice(addr.as_slice()); // address (20 bytes)
    // bytes 20..28 stay zero
    // Solady _BALANCE_SLOT_SEED = 0x87a211a2
    buf[28] = 0x87;
    buf[29] = 0xa2;
    buf[30] = 0x11;
    buf[31] = 0xa2;
    keccak256(buf).into()
}

/// Transfers `amount` ERC20 tokens from `sender` to `recipient` via journal sload/sstore.
fn token_operation<CTX, ERROR>(
    context: &mut CTX,
    sender: Address,
    recipient: Address,
    amount: U256,
) -> Result<(), ERROR>
where
    CTX: ContextTr,
    ERROR: From<InvalidTransaction> + From<<CTX::Db as Database>::Error>,
{
    let sender_slot = erc_address_storage(sender);
    let sender_balance = context.journal_mut().sload(TOKEN, sender_slot)?.data;

    if sender_balance < amount {
        return Err(InvalidTransaction::LackOfFundForMaxFee {
            fee: Box::new(amount),
            balance: Box::new(sender_balance),
        }
        .into());
    }

    context
        .journal_mut()
        .sstore(TOKEN, sender_slot, sender_balance.saturating_sub(amount))?;

    let recipient_slot = erc_address_storage(recipient);
    let recipient_balance = context.journal_mut().sload(TOKEN, recipient_slot)?.data;
    let new_recipient_balance = recipient_balance.checked_add(amount).ok_or(
        InvalidTransaction::LackOfFundForMaxFee {
            fee: Box::new(amount),
            balance: Box::new(recipient_balance),
        },
    )?;
    context
        .journal_mut()
        .sstore(TOKEN, recipient_slot, new_recipient_balance)?;

    Ok(())
}

pub struct SeismicHandler<EVM, ERROR, FRAME> {
    pub mainnet: MainnetHandler<EVM, ERROR, FRAME>,
    pub _phantom: core::marker::PhantomData<(EVM, ERROR, FRAME)>,
}

impl<EVM, ERROR, FRAME> SeismicHandler<EVM, ERROR, FRAME> {
    pub fn new() -> Self {
        Self {
            mainnet: MainnetHandler::default(),
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<EVM, ERROR, FRAME> Default for SeismicHandler<EVM, ERROR, FRAME> {
    fn default() -> Self {
        Self::new()
    }
}

impl<EVM, ERROR, FRAME> Handler for SeismicHandler<EVM, ERROR, FRAME>
where
    EVM: EvmTr<Context: SeismicContextTr, Frame = FRAME>,
    ERROR: EvmTrError<EVM> + From<InvalidTransaction> + FromStringError + core::fmt::Debug,
    FRAME: FrameTr<FrameResult = FrameResult, FrameInit = FrameInit>,
{
    type Evm = EVM;
    type Error = ERROR;
    type HaltReason = HaltReason;

        fn validate_against_state_and_deduct_caller(&self, evm: &mut Self::Evm) -> Result<(), ERROR> {
        let context = evm.ctx();
        let basefee = context.block().basefee() as u128;
        let blob_price = context.block().blob_gasprice().unwrap_or_default();
        let is_balance_check_disabled = context.cfg().is_balance_check_disabled();
        let is_eip3607_disabled = context.cfg().is_eip3607_disabled();
        let is_nonce_check_disabled = context.cfg().is_nonce_check_disabled();
        let caller = context.tx().caller();
        let value = context.tx().value();

        let (tx, journal) = context.tx_journal_mut();

        // Load caller's account.
        let caller_account = journal.load_account_code(tx.caller())?.data;

        validate_account_nonce_and_code(
            &mut caller_account.info,
            tx.nonce(),
            is_eip3607_disabled,
            is_nonce_check_disabled,
        )?;

        let max_balance_spending = tx.max_balance_spending()?;
        let effective_balance_spending = tx
            .effective_balance_spending(basefee, blob_price)
            .expect("effective balance is always smaller than max balance so it can't overflow");
        let gas_balance_spending = effective_balance_spending - value;
        let eth_balance = caller_account.info.balance;
        let is_call = tx.kind().is_call();

        // Preamble: mark touch and bump nonce (common to all paths).
        caller_account.mark_touch();
        if is_call {
            caller_account.info.nonce = caller_account.info.nonce.saturating_add(1);
        }

        if !is_balance_check_disabled && eth_balance >= max_balance_spending {
            // Native ETH path (standard mainnet behavior).
            let old_balance = eth_balance;
            caller_account.info.balance = eth_balance.saturating_sub(gas_balance_spending);
            journal.caller_accounting_journal_entry(caller, old_balance, is_call);
            // used_erc20_gas flag remains false (default).
        } else if !is_balance_check_disabled {
            // Record journal entries for the nonce bump and account touch so that
            // discard_tx() can revert them if the transaction fails. ETH balance is
            // unchanged in this path (old_balance == current balance → revert is a no-op).
            journal.caller_accounting_journal_entry(caller, eth_balance, is_call);
            // NLL: (tx, journal) borrow ends here; context.journal_mut() borrows below.

            // ERC20 fallback path: gas is paid in USDC, value is still paid in ETH.
            // Verify the caller has enough ETH to cover the value transfer.
            if value > eth_balance {
                return Err(InvalidTransaction::LackOfFundForMaxFee {
                    fee: Box::new(value),
                    balance: Box::new(eth_balance),
                }
                .into());
            }

            let account_balance_slot = erc_address_storage(caller);
            context.journal_mut().load_account(TOKEN)?.data.mark_touch();

            let account_balance = context
                .journal_mut()
                .sload(TOKEN, account_balance_slot)
                .map(|v| v.data)
                .unwrap_or_default();

            // Check USDC covers gas costs only (value is paid in ETH, not USDC).
            // Scale wei (18 decimals) → USDC (6 decimals) for comparison.
            // Use ceiling division for the check so we don't under-require.
            let max_gas_spending = max_balance_spending - value;
            let max_gas_spending_usdc =
                (max_gas_spending + WEI_TO_USDC_DIVISOR - U256::from(1)) / WEI_TO_USDC_DIVISOR;
            if max_gas_spending_usdc > account_balance {
                return Err(InvalidTransaction::LackOfFundForMaxFee {
                    fee: Box::new(max_gas_spending_usdc),
                    balance: Box::new(account_balance),
                }
                .into());
            }

            // Subtract gas spending (scaled to USDC) — value is transferred during execution.
            let gas_spending_usdc = gas_balance_spending / WEI_TO_USDC_DIVISOR;
            token_operation::<EVM::Context, ERROR>(context, caller, TREASURY, gas_spending_usdc)?;
            context.chain_mut().set_used_erc20_gas();
        }
        // is_balance_check_disabled: preamble (touch + nonce) already done, no deduction needed.

        Ok(())
    }

    fn reimburse_caller(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <<Self::Evm as EvmTr>::Frame as FrameTr>::FrameResult,
    ) -> Result<(), Self::Error> {
        let context = evm.ctx();
        if context.cfg().is_balance_check_disabled() {
            return Ok(());
        }

        if context.chain().used_erc20_gas() {
            // ERC20 path: return unused gas from TREASURY → caller in tokens.
            let basefee = context.block().basefee() as u128;
            let caller = context.tx().caller();
            let effective_gas_price = context.tx().effective_gas_price(basefee);
            let gas = exec_result.gas();
            let reimbursement_wei = effective_gas_price
                .saturating_mul((gas.remaining() + gas.refunded() as u64) as u128);
            // Scale wei → USDC (round down; treasury keeps dust).
            let reimbursement_usdc = U256::from(reimbursement_wei) / WEI_TO_USDC_DIVISOR;
            token_operation::<EVM::Context, ERROR>(
                context,
                TREASURY,
                caller,
                reimbursement_usdc,
            )?;
        } else {
            // Native ETH path: standard balance_incr.
            post_execution::reimburse_caller(evm.ctx(), exec_result.gas(), U256::ZERO)?;
        }

        Ok(())
    }

    fn reward_beneficiary(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <<Self::Evm as EvmTr>::Frame as FrameTr>::FrameResult,
    ) -> Result<(), Self::Error> {
        let context = evm.ctx();
        if context.cfg().is_balance_check_disabled() {
            return Ok(());
        }

        // Seismic does not burn basefee — the full effective gas price is paid to
        // the beneficiary (unlike mainnet EIP-1559 which burns the basefee portion).
        let basefee = context.block().basefee() as u128;
        let beneficiary = context.block().beneficiary();
        let effective_gas_price = context.tx().effective_gas_price(basefee);
        let gas = exec_result.gas();
        let reward_wei = effective_gas_price.saturating_mul(gas.used() as u128);

        if context.chain().used_erc20_gas() {
            // ERC20 path: pay beneficiary from TREASURY in tokens.
            // Scale wei → USDC (round down; treasury keeps dust).
            let reward_usdc = U256::from(reward_wei) / WEI_TO_USDC_DIVISOR;
            token_operation::<EVM::Context, ERROR>(
                context,
                TREASURY,
                beneficiary,
                reward_usdc,
            )?;
        } else {
            // Native ETH path: credit beneficiary directly.
            context
                .journal_mut()
                .balance_incr(beneficiary, U256::from(reward_wei))?;
        }

        Ok(())
    }

    /// Overrides the execution phase to short-circuit when a transaction's calldata
    /// decryption has failed. In this case, bytecode execution is skipped and a Revert
    /// is returned with all execution gas unspent (only intrinsic gas is charged).
    ///
    /// The validate and pre_execution phases still run normally, ensuring the sender's
    /// balance is deducted and nonce is incremented. The post_execution phase reimburses
    /// the unused execution gas and credits the coinbase.
    #[inline]
    fn execution(
        &mut self,
        evm: &mut Self::Evm,
        init_and_floor_gas: &InitialAndFloorGas,
    ) -> Result<FrameResult, Self::Error> {
        if evm.ctx().tx().decryption_failed() {
            // All gas beyond intrinsic is returned to the sender.
            let execution_gas = evm.ctx().tx().gas_limit() - init_and_floor_gas.initial_gas;
            return Ok(FrameResult::Call(CallOutcome::new(
                InterpreterResult {
                    result: InstructionResult::Revert,
                    output: Bytes::new(),
                    gas: Gas::new(execution_gas),
                },
                0..0,
            )));
        }
        self.mainnet.execution(evm, init_and_floor_gas)
    }

    /// Processes the final execution output.
    ///
    /// Retrieves the final state from the journal, converts internal results
    /// to the external output format. Internal state is cleared and EVM is
    /// prepared for the next transaction.
    ///
    /// Seismic Addendum:
    /// Given that we can't yet pass instruction_result which aren't in the
    /// InstructionResult enum, we leverage context_error to bubble up our
    /// instruction set specific errors.
    #[inline]
    fn execution_result(
        &mut self,
        evm: &mut Self::Evm,
        result: <<Self::Evm as EvmTr>::Frame as FrameTr>::FrameResult,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        match core::mem::replace(evm.ctx().error(), Ok(())) {
            Err(ContextError::Db(e)) => return Err(e.into()),
            Err(ContextError::Custom(e)) => {
                return Err(Self::Error::from_string(e));
            }
            Ok(_) => (),
        }

        let exec_result = post_execution::output(evm.ctx(), result);

        evm.ctx().journal_mut().commit_tx();
        evm.ctx().local_mut().clear();
        evm.frame_stack().clear();
        evm.ctx().chain_mut().reset_erc20_gas();

        Ok(exec_result)
    }

    /// Handles cleanup when an error occurs during execution.
    ///
    /// Ensures the journal state is properly cleared before propagating the error.
    /// On happy path journal is cleared in [`Handler::output`] method.
    #[inline]
    fn catch_error(
        &self,
        evm: &mut Self::Evm,
        error: Self::Error,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        evm.ctx().local_mut().clear();
        evm.ctx().journal_mut().discard_tx();
        evm.frame_stack().clear();
        evm.ctx().chain_mut().reset_erc20_gas();
        Err(error)
    }
}

// Fix for the first error: Simplify the InspectorHandler implementation with proper bounds
impl<EVM, ERROR> InspectorHandler for SeismicHandler<EVM, ERROR, EthFrame<EthInterpreter>>
where
    EVM: InspectorEvmTr<Context: SeismicContextTr>,
    ERROR: EvmTrError<EVM> + From<InvalidTransaction> + FromStringError + core::fmt::Debug,
    EVM::Inspector: Inspector<EVM::Context, EthInterpreter>,
{
    type IT = EthInterpreter;
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic
    )]

    use super::*;
    use crate::{api::default_ctx::SeismicContext, DefaultSeismicContext, SeismicBuilder};
    use revm::{
        context::{result::EVMError, Context},
        context_interface::context::ContextError,
        database_interface::EmptyDB,
        handler::EthFrame,
        interpreter::{CallOutcome, Gas, InstructionResult, InterpreterResult},
        primitives::Bytes,
    };

    /// Creates frame result.
    fn call_last_frame_return(
        ctx: SeismicContext<EmptyDB>,
        instruction_result: InstructionResult,
        gas: Gas,
    ) -> Gas {
        let mut evm = ctx.build_seismic_evm();

        let mut exec_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: instruction_result,
                output: Bytes::new(),
                gas,
            },
            0..0,
        ));

        let mut handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        handler
            .last_frame_result(&mut evm, &mut exec_result)
            .unwrap();
        handler.refund(&mut evm, &mut exec_result, 0);
        *exec_result.gas()
    }

    #[test]
    fn test_revert_gas() {
        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let gas = call_last_frame_return(ctx, InstructionResult::Revert, Gas::new(90));
        assert_eq!(gas.remaining(), 90);
        assert_eq!(gas.spent(), 10);
        assert_eq!(gas.refunded(), 0);
    }

    #[test]
    fn test_fatal_external_error_gas() {
        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let gas = call_last_frame_return(ctx, InstructionResult::FatalExternalError, Gas::new(90));
        assert_eq!(gas.remaining(), 0);
        assert_eq!(gas.spent(), 100);
        assert_eq!(gas.refunded(), 0);
    }

    /// Regression test: catch_error must call discard_tx() so that
    /// transaction_id advances and warm slot/account tracking is reset.
    /// Without discard_tx(), slots warmed in a failed tx would remain warm
    /// for the next tx, causing incorrect (cheaper) gas accounting.
    #[test]
    fn test_catch_error_discards_tx_and_advances_transaction_id() {
        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let mut evm = ctx.build_seismic_evm();

        let tx_id_before = evm.ctx().journal().inner.transaction_id;

        // Inject a context error that triggers the catch_error path.
        *evm.ctx().error() = Err(ContextError::Custom("some error".to_string()));

        let frame_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: InstructionResult::Stop,
                output: Bytes::new(),
                gas: Gas::new(90),
            },
            0..0,
        ));

        let mut handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        // execution_result returns Err for context errors, which the
        // execution loop passes to catch_error.
        let err = handler
            .execution_result(&mut evm, frame_result)
            .unwrap_err();
        let _ = handler.catch_error(&mut evm, err);

        // transaction_id must have advanced, proving discard_tx() was called.
        let tx_id_after = evm.ctx().journal().inner.transaction_id;
        assert_eq!(
            tx_id_after,
            tx_id_before + 1,
            "transaction_id should advance after catch_error (discard_tx must be called)"
        );
    }

    /// When `decryption_failed` is set on the transaction, the execution phase
    /// should short-circuit with a Revert and return all execution gas as
    /// remaining (only intrinsic gas is consumed).
    #[test]
    fn test_decryption_failed_skips_execution_and_returns_all_execution_gas() {
        let gas_limit: u64 = 100_000;
        let intrinsic_gas: u64 = 21_000;

        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = gas_limit;
            tx.decryption_failed = true;
        });

        let mut evm = ctx.build_seismic_evm();

        let mut handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        let init_and_floor_gas = InitialAndFloorGas::new(intrinsic_gas, 0);
        let result = handler.execution(&mut evm, &init_and_floor_gas).unwrap();

        let gas = result.gas();
        let execution_budget = gas_limit - intrinsic_gas;
        assert_eq!(
            gas.remaining(),
            execution_budget,
            "all execution gas should be remaining (returned to sender)"
        );
        assert_eq!(gas.spent(), 0, "no execution gas should be spent");

        // Verify it's a Revert
        match &result {
            FrameResult::Call(outcome) => {
                assert_eq!(
                    outcome.result.result,
                    InstructionResult::Revert,
                    "should be a Revert"
                );
                assert!(
                    outcome.result.output.is_empty(),
                    "revert output should be empty"
                );
            }
            _ => panic!("expected FrameResult::Call"),
        }
    }

    /// When `decryption_failed` is NOT set, execution should proceed normally
    /// (delegating to the mainnet handler). This verifies the flag check doesn't
    /// accidentally short-circuit normal transactions.
    #[test]
    fn test_decryption_not_failed_proceeds_normally() {
        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
            // decryption_failed defaults to false
        });

        let mut evm = ctx.build_seismic_evm();

        let mut handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        let init_and_floor_gas = InitialAndFloorGas::new(21, 0);
        // Normal execution — this will attempt to run a transaction against
        // the empty DB, which may fail, but the point is it does NOT take
        // the decryption_failed short-circuit path.
        let result = handler.execution(&mut evm, &init_and_floor_gas);

        // If execution reached the mainnet handler (not short-circuited),
        // the result might be Ok or Err depending on the empty state, but
        // it should NOT be the specific Revert-with-full-gas pattern from
        // the decryption_failed path.
        if let Ok(frame_result) = result {
            let gas = frame_result.gas();
            // Normal execution would consume some gas, not return full budget
            let execution_budget = 100 - 21;
            // If it returned full budget as remaining, that means it hit the
            // decryption_failed path (which it shouldn't)
            if gas.remaining() == execution_budget && gas.spent() == 0 {
                panic!("normal tx should not take the decryption_failed short-circuit path");
            }
        }
        // Any other result (error, different gas values) means normal execution
        // was attempted, which is correct.
    }

    // ==================== ERC20 Gas Tests ====================

    use revm::database::InMemoryDB;
    use revm::primitives::TxKind;

    /// Helper: build a SeismicContext backed by InMemoryDB with the TOKEN contract account
    /// pre-created, and a caller with a given ETH balance and USDC (ERC20) balance.
    fn build_erc20_ctx(
        caller: Address,
        eth_balance: U256,
        usdc_balance: U256,
        gas_limit: u64,
        gas_price: u128,
        value: U256,
    ) -> SeismicContext<InMemoryDB> {
        use revm::state::AccountInfo;

        let mut db = InMemoryDB::default();

        // Seed caller ETH balance.
        db.insert_account_info(
            caller,
            AccountInfo {
                balance: eth_balance,
                ..Default::default()
            },
        );

        // Seed TOKEN contract account (must exist for sload/sstore).
        db.insert_account_info(TOKEN, AccountInfo::default());

        // Seed caller's USDC balance in the TOKEN contract storage.
        if usdc_balance > U256::ZERO {
            let slot = erc_address_storage(caller);
            db.insert_account_storage(TOKEN, slot, usdc_balance.into())
                .unwrap();
        }

        // Seed TREASURY account.
        db.insert_account_info(TREASURY, AccountInfo::default());

        Context::seismic_with_random_rng_key()
            .modify_tx_chained(|tx| {
                tx.base.caller = caller;
                tx.base.gas_limit = gas_limit;
                tx.base.gas_price = gas_price;
                tx.base.gas_priority_fee = None;
                tx.base.value = value;
                tx.base.kind = TxKind::Call(Address::ZERO);
            })
            .with_db(db)
    }

    /// Helper: read the USDC balance of `addr` from the journal.
    fn read_usdc_balance<CTX: ContextTr>(ctx: &mut CTX, addr: Address) -> U256
    where
        <CTX::Db as Database>::Error: core::fmt::Debug,
    {
        let slot = erc_address_storage(addr);
        ctx.journal_mut().sload(TOKEN, slot).unwrap().data
    }

    #[test]
    fn test_erc20_gas_fallback_deducts_from_token() {
        // Caller has 0 ETH but plenty of USDC. Gas should be paid in USDC.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000; // 1 gwei
        let usdc_balance = U256::from(1_000_000_000u64); // 1000 USDC (6 decimals)

        let ctx = build_erc20_ctx(caller, U256::ZERO, usdc_balance, gas_limit, gas_price, U256::ZERO);
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        // Should have flagged ERC20 gas usage.
        assert!(
            evm.ctx().chain().used_erc20_gas(),
            "should use ERC20 gas when ETH balance is insufficient"
        );

        // Gas cost in wei = gas_limit * gas_price = 100_000 * 1e9 = 1e14 wei
        // Gas cost in USDC = 1e14 / 1e12 = 100 USDC units
        let expected_deduction = U256::from(100u64);
        let caller_balance_after = read_usdc_balance(evm.ctx(), caller);
        assert_eq!(
            caller_balance_after,
            usdc_balance - expected_deduction,
            "caller USDC should be reduced by gas cost"
        );

        // Treasury should have received the deduction.
        let treasury_balance = read_usdc_balance(evm.ctx(), TREASURY);
        assert_eq!(
            treasury_balance, expected_deduction,
            "treasury should receive the gas payment in USDC"
        );
    }

    #[test]
    fn test_eth_path_when_balance_sufficient() {
        // Caller has enough ETH — should take the native path, not ERC20.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        // max_balance_spending = gas_limit * gas_price = 1e14 wei
        let eth_balance = U256::from(1_000_000_000_000_000u128); // 1e15, more than enough

        let ctx = build_erc20_ctx(caller, eth_balance, U256::ZERO, gas_limit, gas_price, U256::ZERO);
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(
            !evm.ctx().chain().used_erc20_gas(),
            "should NOT use ERC20 gas when ETH balance is sufficient"
        );
    }

    #[test]
    fn test_erc20_gas_insufficient_usdc_balance_fails() {
        // Caller has 0 ETH and insufficient USDC — should fail.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        // Need at least ceil(1e14 / 1e12) = 100 USDC
        let usdc_balance = U256::from(50u64); // only 50 USDC units

        let ctx = build_erc20_ctx(caller, U256::ZERO, usdc_balance, gas_limit, gas_price, U256::ZERO);
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let result = handler.validate_against_state_and_deduct_caller(&mut evm);

        assert!(result.is_err(), "should fail when USDC balance is insufficient");
    }

    #[test]
    fn test_erc20_gas_reimburse_caller() {
        // Set up a tx that used ERC20 gas, then reimburse unused gas.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000; // 1 gwei

        let usdc_balance = U256::from(1_000_000_000u64); // 1000 USDC
        let ctx = build_erc20_ctx(caller, U256::ZERO, usdc_balance, gas_limit, gas_price, U256::ZERO);
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        // Deduct gas first.
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        let treasury_after_deduct = read_usdc_balance(evm.ctx(), TREASURY);
        let caller_after_deduct = read_usdc_balance(evm.ctx(), caller);

        // Create a frame result where 50,000 gas remains (half used).
        let mut gas = Gas::new(gas_limit - 21_000); // execution gas budget
        let _ = gas.record_cost(29_000); // use 29k, leaving 50k remaining
        let mut exec_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: InstructionResult::Stop,
                output: Bytes::new(),
                gas,
            },
            0..0,
        ));

        handler.reimburse_caller(&mut evm, &mut exec_result).unwrap();

        // Reimbursement: remaining_gas * gas_price / WEI_TO_USDC_DIVISOR
        // = 50_000 * 1e9 / 1e12 = 50 USDC
        let expected_reimbursement = U256::from(50u64);
        let caller_after_reimburse = read_usdc_balance(evm.ctx(), caller);
        let treasury_after_reimburse = read_usdc_balance(evm.ctx(), TREASURY);

        assert_eq!(
            caller_after_reimburse,
            caller_after_deduct + expected_reimbursement,
            "caller should be reimbursed for unused gas"
        );
        assert_eq!(
            treasury_after_reimburse,
            treasury_after_deduct - expected_reimbursement,
            "treasury should have reimbursement deducted"
        );
    }

    #[test]
    fn test_erc20_gas_reward_beneficiary() {
        // Test that the block beneficiary gets rewarded from treasury in USDC.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 2_000_000_000; // 2 gwei
        let usdc_balance = U256::from(1_000_000_000u64);

        let ctx = build_erc20_ctx(caller, U256::ZERO, usdc_balance, gas_limit, gas_price, U256::ZERO);
        // Set basefee to 1 gwei so coinbase_gas_price = gas_price - basefee = 1 gwei
        let ctx = ctx.modify_block_chained(|b| {
            b.basefee = 1_000_000_000; // 1 gwei
        });

        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        // Simulate: all gas used (100k gas spent).
        let mut gas = Gas::new(gas_limit - 21_000);
        let _ = gas.record_cost(gas_limit - 21_000); // all execution gas used
        let mut exec_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: InstructionResult::Stop,
                output: Bytes::new(),
                gas,
            },
            0..0,
        ));

        // Need to load beneficiary account for sstore.
        evm.ctx()
            .journal_mut()
            .load_account(beneficiary)
            .unwrap();

        handler
            .reward_beneficiary(&mut evm, &mut exec_result)
            .unwrap();

        // Seismic does not burn basefee — full effective_gas_price goes to beneficiary.
        // Reward = effective_gas_price * gas_used / WEI_TO_USDC_DIVISOR
        // effective_gas_price = 2 gwei (gas_price, since no priority fee)
        // gas_used = execution gas budget = 79_000 (all execution gas used)
        // reward_wei = 2e9 * 79_000 = 1.58e14
        // reward_usdc = 1.58e14 / 1e12 = 158
        let beneficiary_balance = read_usdc_balance(evm.ctx(), beneficiary);
        assert_eq!(
            beneficiary_balance,
            U256::from(158u64),
            "beneficiary should receive full reward in USDC (no basefee burn)"
        );
    }

    #[test]
    fn test_erc20_gas_flag_reset_on_execution_result() {
        // Verify that the ERC20 gas flag is cleared after execution_result.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let usdc_balance = U256::from(1_000_000_000u64);
        let ctx = build_erc20_ctx(caller, U256::ZERO, usdc_balance, 100_000, 1_000_000_000, U256::ZERO);
        let mut evm = ctx.build_seismic_evm();

        // Manually set the flag.
        evm.ctx().chain_mut().set_used_erc20_gas();
        assert!(evm.ctx().chain().used_erc20_gas());

        let frame_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: InstructionResult::Stop,
                output: Bytes::new(),
                gas: Gas::new(0),
            },
            0..0,
        ));

        let mut handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let _ = handler.execution_result(&mut evm, frame_result);

        assert!(
            !evm.ctx().chain().used_erc20_gas(),
            "ERC20 gas flag should be reset after execution_result"
        );
    }

    #[test]
    fn test_erc20_gas_flag_reset_on_catch_error() {
        // Verify that the ERC20 gas flag is cleared on error path.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let usdc_balance = U256::from(1_000_000_000u64);
        let ctx = build_erc20_ctx(caller, U256::ZERO, usdc_balance, 100_000, 1_000_000_000, U256::ZERO);
        let mut evm = ctx.build_seismic_evm();

        evm.ctx().chain_mut().set_used_erc20_gas();
        assert!(evm.ctx().chain().used_erc20_gas());

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let error: EVMError<_, InvalidTransaction> =
            EVMError::Custom("test error".to_string());
        let _ = handler.catch_error(&mut evm, error);

        assert!(
            !evm.ctx().chain().used_erc20_gas(),
            "ERC20 gas flag should be reset after catch_error"
        );
    }

    #[test]
    fn test_erc_address_storage_deterministic() {
        // Same address should always produce the same storage slot.
        let addr = address!("0x0000000000000000000000000000000000001234");
        let slot1 = erc_address_storage(addr);
        let slot2 = erc_address_storage(addr);
        assert_eq!(slot1, slot2, "storage slot must be deterministic");
    }

    #[test]
    fn test_erc_address_storage_different_addresses() {
        // Different addresses should produce different storage slots.
        let addr1 = address!("0x0000000000000000000000000000000000001234");
        let addr2 = address!("0x0000000000000000000000000000000000005678");
        assert_ne!(
            erc_address_storage(addr1),
            erc_address_storage(addr2),
            "different addresses should have different storage slots"
        );
    }

    #[test]
    fn test_token_operation_transfer() {
        // Test direct token_operation transfer between two accounts.
        let sender = address!("0x0000000000000000000000000000000000001111");
        let recipient = address!("0x0000000000000000000000000000000000002222");
        let initial_balance = U256::from(1000u64);
        let transfer_amount = U256::from(300u64);

        let mut db = InMemoryDB::default();
        db.insert_account_info(TOKEN, Default::default());
        db.insert_account_info(sender, Default::default());
        db.insert_account_info(recipient, Default::default());
        db.insert_account_storage(TOKEN, erc_address_storage(sender), initial_balance.into())
            .unwrap();

        let ctx: SeismicContext<InMemoryDB> = Context::seismic_with_random_rng_key()
            .modify_tx_chained(|tx| {
                tx.base.caller = sender;
                tx.base.gas_limit = 100_000;
                tx.base.kind = TxKind::Call(Address::ZERO);
            })
            .with_db(db);

        let mut ctx = ctx;
        // Load TOKEN account into journal for sload/sstore.
        ctx.journal_mut().load_account(TOKEN).unwrap();

        token_operation::<_, EVMError<_, InvalidTransaction>>(&mut ctx, sender, recipient, transfer_amount)
            .unwrap();

        let sender_bal = read_usdc_balance(&mut ctx, sender);
        let recipient_bal = read_usdc_balance(&mut ctx, recipient);

        assert_eq!(sender_bal, initial_balance - transfer_amount);
        assert_eq!(recipient_bal, transfer_amount);
    }

    #[test]
    fn test_token_operation_insufficient_balance() {
        // Transferring more than the sender has should fail.
        let sender = address!("0x0000000000000000000000000000000000001111");
        let recipient = address!("0x0000000000000000000000000000000000002222");

        let mut db = InMemoryDB::default();
        db.insert_account_info(TOKEN, Default::default());
        db.insert_account_info(sender, Default::default());
        db.insert_account_info(recipient, Default::default());
        db.insert_account_storage(TOKEN, erc_address_storage(sender), U256::from(100u64).into())
            .unwrap();

        let ctx: SeismicContext<InMemoryDB> = Context::seismic_with_random_rng_key()
            .modify_tx_chained(|tx| {
                tx.base.caller = sender;
                tx.base.gas_limit = 100_000;
                tx.base.kind = TxKind::Call(Address::ZERO);
            })
            .with_db(db);

        let mut ctx = ctx;
        ctx.journal_mut().load_account(TOKEN).unwrap();

        let result = token_operation::<_, EVMError<_, InvalidTransaction>>(
            &mut ctx,
            sender,
            recipient,
            U256::from(200u64), // more than available
        );
        assert!(result.is_err(), "should fail when sender has insufficient token balance");
    }

    #[test]
    fn test_wei_to_usdc_conversion_ceiling_division() {
        // Verify the ceiling division used in the balance check.
        // For a gas cost that doesn't divide evenly, the ceiling should round up.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_001; // produces non-round max_balance_spending
        let gas_price: u128 = 1_000_000_000; // 1 gwei
        // max_balance_spending = 100_001 * 1e9 = 100_001_000_000_000 wei
        // ceil(100_001_000_000_000 / 1e12) = ceil(100.001) = 101 USDC
        // So 100 USDC should NOT be enough.
        let usdc_balance = U256::from(100u64);

        let ctx = build_erc20_ctx(caller, U256::ZERO, usdc_balance, gas_limit, gas_price, U256::ZERO);
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let result = handler.validate_against_state_and_deduct_caller(&mut evm);

        assert!(
            result.is_err(),
            "100 USDC should not be enough due to ceiling division (needs 101)"
        );

        // Now try with 101 USDC — should succeed.
        let usdc_balance = U256::from(101u64);
        let ctx = build_erc20_ctx(caller, U256::ZERO, usdc_balance, gas_limit, gas_price, U256::ZERO);
        let mut evm = ctx.build_seismic_evm();

        let result = handler.validate_against_state_and_deduct_caller(&mut evm);
        assert!(
            result.is_ok(),
            "101 USDC should be sufficient with ceiling division"
        );
    }

    #[test]
    fn test_erc20_gas_exact_boundary_eth_balance() {
        // When ETH balance is exactly max_balance_spending, ETH path should be used.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        // max_balance_spending = 100_000 * 1e9 = 1e14 wei
        let exact_eth = U256::from(100_000_000_000_000u128);

        let ctx = build_erc20_ctx(caller, exact_eth, U256::ZERO, gas_limit, gas_price, U256::ZERO);
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(
            !evm.ctx().chain().used_erc20_gas(),
            "exact ETH balance should use native path, not ERC20"
        );
    }

    #[test]
    fn test_erc20_gas_one_wei_short_falls_to_erc20() {
        // When ETH is 1 wei short of max_balance_spending, should fall back to ERC20.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        let one_short = U256::from(100_000_000_000_000u128) - U256::from(1);
        let usdc_balance = U256::from(1_000_000_000u64);

        let ctx = build_erc20_ctx(caller, one_short, usdc_balance, gas_limit, gas_price, U256::ZERO);
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(
            evm.ctx().chain().used_erc20_gas(),
            "1 wei short should fall back to ERC20 gas path"
        );
    }

    #[test]
    fn test_erc20_gas_with_value_checks_eth_for_value() {
        // Caller has 0 ETH, plenty of USDC, but tx has value > 0.
        // Should fail because the caller can't cover the value transfer in ETH.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        let value = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
        let usdc_balance = U256::from(1_000_000_000u64); // plenty of USDC

        let ctx = build_erc20_ctx(caller, U256::ZERO, usdc_balance, gas_limit, gas_price, value);
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let result = handler.validate_against_state_and_deduct_caller(&mut evm);

        assert!(
            result.is_err(),
            "should fail when caller has 0 ETH but tx has non-zero value"
        );
    }

    #[test]
    fn test_erc20_gas_with_value_succeeds_when_eth_covers_value() {
        // Caller has enough ETH for value but not for gas+value.
        // Has enough USDC for gas. Should succeed via ERC20 path.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000; // 1 gwei
        let value = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
        // max_balance_spending = gas (1e14 wei) + value (1e18 wei) = 1.0001e18
        // Caller has 1 ETH — enough for value, not for gas+value.
        let eth_balance = U256::from(1_000_000_000_000_000_000u128);
        let usdc_balance = U256::from(1_000_000_000u64); // 1000 USDC, plenty for gas

        let ctx = build_erc20_ctx(caller, eth_balance, usdc_balance, gas_limit, gas_price, value);
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(
            evm.ctx().chain().used_erc20_gas(),
            "should use ERC20 gas when ETH covers value but not gas+value"
        );

        // USDC check should only require gas, not gas+value.
        // gas cost in USDC = 1e14 / 1e12 = 100 USDC
        let expected_gas_deduction = U256::from(100u64);
        let caller_usdc = read_usdc_balance(evm.ctx(), caller);
        assert_eq!(
            caller_usdc,
            usdc_balance - expected_gas_deduction,
            "only gas should be deducted from USDC, not value"
        );
    }

    #[test]
    fn test_erc20_gas_usdc_check_excludes_value() {
        // Caller has enough ETH for value and enough USDC for gas-only,
        // but NOT enough USDC for gas+value. Should succeed because
        // the USDC check now only covers gas costs.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        let value = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
        let eth_balance = U256::from(1_000_000_000_000_000_000u128); // 1 ETH (covers value)
        // Gas in USDC = ceil(1e14 / 1e12) = 100. Give exactly 100 USDC.
        // If value were included: ceil((1e14 + 1e18) / 1e12) = 1_000_100 — way more than 100.
        let usdc_balance = U256::from(100u64);

        let ctx = build_erc20_ctx(caller, eth_balance, usdc_balance, gas_limit, gas_price, value);
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let result = handler.validate_against_state_and_deduct_caller(&mut evm);

        assert!(
            result.is_ok(),
            "should succeed: USDC check covers gas only, not value"
        );
    }

    #[test]
    fn test_native_eth_no_basefee_burn() {
        // Verify that with native ETH gas, the full effective_gas_price goes to beneficiary
        // (no basefee burning like mainnet EIP-1559).
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 2_000_000_000; // 2 gwei
        // Enough ETH for gas + some extra.
        let eth_balance = U256::from(1_000_000_000_000_000_000u128); // 1 ETH

        let ctx = build_erc20_ctx(caller, eth_balance, U256::ZERO, gas_limit, gas_price, U256::ZERO);
        let ctx = ctx.modify_block_chained(|b| {
            b.basefee = 1_000_000_000; // 1 gwei basefee
        });
        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(
            !evm.ctx().chain().used_erc20_gas(),
            "should use native ETH path"
        );

        // Simulate: all execution gas used (79k).
        let mut gas = Gas::new(gas_limit - 21_000);
        let _ = gas.record_cost(gas_limit - 21_000);
        let mut exec_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: InstructionResult::Stop,
                output: Bytes::new(),
                gas,
            },
            0..0,
        ));

        // Load beneficiary into journal.
        evm.ctx().journal_mut().load_account(beneficiary).unwrap();

        // Get beneficiary ETH balance before reward.
        let ben_balance_before = evm
            .ctx()
            .journal_mut()
            .load_account(beneficiary)
            .unwrap()
            .data
            .info
            .balance;

        handler
            .reward_beneficiary(&mut evm, &mut exec_result)
            .unwrap();

        let ben_balance_after = evm
            .ctx()
            .journal_mut()
            .load_account(beneficiary)
            .unwrap()
            .data
            .info
            .balance;

        // Reward should be effective_gas_price * gas_used (no basefee subtraction).
        // effective_gas_price = 2 gwei, gas_used = 79_000
        // reward = 2e9 * 79_000 = 1.58e14 wei
        let expected_reward = U256::from(2_000_000_000u128 * 79_000u128);
        assert_eq!(
            ben_balance_after - ben_balance_before,
            expected_reward,
            "beneficiary should receive full effective_gas_price * gas_used (no basefee burn)"
        );

        // On mainnet, reward would be (2 gwei - 1 gwei) * 79_000 = 7.9e13
        // which is half. Verify we're giving more than that.
        let mainnet_reward = U256::from(1_000_000_000u128 * 79_000u128);
        assert!(
            expected_reward > mainnet_reward,
            "Seismic reward should be greater than mainnet (no burn)"
        );
    }
}
