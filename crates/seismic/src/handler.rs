//!Handler related to Seismic chain
use crate::{api::exec::SeismicContextTr, transaction::abstraction::SeismicTxTr};
use revm::{
    context::{
        result::{ExecutionResult, InvalidTransaction},
        Block as _, Cfg as _, ContextTr, JournalTr, LocalContextTr,
    },
    context_interface::{
        context::ContextError,
        result::{FromStringError, HaltReason},
        transaction::Transaction,
    },
    handler::{
        handler::EvmTrError, post_execution, pre_execution::validate_account_nonce_and_code,
        EthFrame, EvmTr, FrameResult, FrameTr, Handler, MainnetHandler,
    },
    inspector::{Inspector, InspectorEvmTr, InspectorHandler},
    interpreter::{
        interpreter::EthInterpreter, interpreter_action::FrameInit, CallOutcome, Gas,
        InitialAndFloorGas, InstructionResult, InterpreterResult,
    },
    primitives::{address, keccak256, Address, Bytes, U256},
    Database,
};

/// ERC20 token address used for gas payment on Seismic.
/// TODO: replace with the actual Seismic token contract address.
pub const TOKEN: Address = address!("0x790701048922E265105fd6a4467a2901c2201C43");

/// Divisor to convert 18-decimal wei amounts to 6-decimal USDC amounts.
/// USDC uses 6 decimals while ETH/wei uses 18, so we divide by 10^(18-6) = 10^12.
const WEI_TO_USDC_DIVISOR: U256 = U256::from_limbs([1_000_000_000_000u64, 0, 0, 0]);

/// Storage slot of the `_balances` mapping on the Seismic USDC token contract.
const BALANCES_SLOT: u8 = 3;

/// Returns the storage slot for the caller's USDC balance entry.
///
/// The token uses the standard Solidity `mapping(address => uint256)` layout:
///   slot := keccak256(abi.encode(key, p))
/// where `key` is the 32-byte (left-padded) address and `p` is the 32-byte
/// mapping slot position. With `_balances` declared at slot 3, this produces:
///   keccak256(0x00..<12 bytes>..00 ++ addr[20 bytes] ++ 0x00..<31 bytes>..03)
pub(crate) fn erc_address_storage(addr: Address) -> U256 {
    let mut buf = [0u8; 64];
    // key: 32-byte left-padded address (first 12 bytes already zero).
    buf[12..32].copy_from_slice(addr.as_slice());
    // slot position: 32-byte big-endian encoding of BALANCES_SLOT.
    buf[63] = BALANCES_SLOT;
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
    context.journal_mut().sstore(
        TOKEN,
        recipient_slot,
        recipient_balance.saturating_add(amount),
    )?;

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

    fn validate_against_state_and_deduct_caller(&self, evm: &mut Self::Evm) -> Result<(), ERROR> {
        let context = evm.ctx();
        let basefee = context.block().basefee() as u128;
        let blob_price = context.block().blob_gasprice().unwrap_or_default();
        let is_balance_check_disabled = context.cfg().is_balance_check_disabled();
        let is_eip3607_disabled = context.cfg().is_eip3607_disabled();
        let is_nonce_check_disabled = context.cfg().is_nonce_check_disabled();
        let caller = context.tx().caller();
        let value = context.tx().value();
        let beneficiary = context.block().beneficiary();

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
            // unchanged in this path (old_balance == current balance → revert is a
            // no-op for balance).
            journal.caller_accounting_journal_entry(caller, eth_balance, is_call);
            // NLL: (tx, journal) borrow ends here; context.journal_mut() reborrows below.

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
            // Scale wei (18 decimals) → USDC (6 decimals) with ceiling division
            // so we don't under-require.
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

            // Subtract gas spending (scaled to USDC) — value is transferred during
            // execution. Gas goes directly to the block beneficiary (no treasury
            // middleman, no burn); unused gas is reimbursed later.
            //
            // Use ceiling division so sub-divisor gas costs still charge at least 1
            // unit of USDC. Floor division would allow free transactions when
            // gas_limit * effective_gas_price < WEI_TO_USDC_DIVISOR (10^12 wei).
            // This matches the ceiling division used in max_gas_spending_usdc above,
            // so if the pre-flight check passes, the caller's balance can cover this
            // deduction exactly.
            let gas_spending_usdc = (gas_balance_spending + WEI_TO_USDC_DIVISOR - U256::from(1))
                / WEI_TO_USDC_DIVISOR;
            token_operation::<EVM::Context, ERROR>(
                context,
                caller,
                beneficiary,
                gas_spending_usdc,
            )?;
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
            // ERC20 path: return unused gas from beneficiary → caller in tokens.
            // Beneficiary holds the gas upfront (see validate_against_state_and_deduct_caller).
            let basefee = context.block().basefee() as u128;
            let caller = context.tx().caller();
            let beneficiary = context.block().beneficiary();
            let effective_gas_price = context.tx().effective_gas_price(basefee);
            let gas = exec_result.gas();
            let reimbursement_wei = effective_gas_price
                .saturating_mul((gas.remaining() + gas.refunded() as u64) as u128);
            // Scale wei → USDC with floor division (beneficiary keeps rounding
            // dust). This is intentional: deduction ceils and refund floors, so
            // the beneficiary always captures at most 1 USDC unit of rounding
            // margin per tx. Using ceil here would risk the refund exceeding
            // what was deducted in some rounding edge cases.
            let reimbursement_usdc = U256::from(reimbursement_wei) / WEI_TO_USDC_DIVISOR;
            token_operation::<EVM::Context, ERROR>(
                context,
                beneficiary,
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

        if context.chain().used_erc20_gas() {
            // ERC20 path: no-op. The beneficiary already received the full gas
            // payment upfront in validate_against_state_and_deduct_caller, and
            // unused gas was returned in reimburse_caller. The net balance they
            // keep is effective_gas_price * gas_used (in USDC), with nothing
            // burned and no treasury middleman.
        } else {
            // Native ETH path: credit beneficiary with the full effective_gas_price
            // (Seismic does not burn basefee, unlike mainnet EIP-1559).
            let basefee = context.block().basefee() as u128;
            let effective_gas_price = context.tx().effective_gas_price(basefee);
            let gas = exec_result.gas();
            let reward_wei = effective_gas_price.saturating_mul(gas.used() as u128);
            let beneficiary = context.block().beneficiary();
            context
                .journal_mut()
                .balance_incr(beneficiary, U256::from(reward_wei))?;
        }

        Ok(())
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

    use revm::{database::InMemoryDB, primitives::TxKind, state::AccountInfo};

    /// Build a SeismicContext on an InMemoryDB with the TOKEN contract account
    /// pre-created, the caller seeded with `eth_balance` ETH and `usdc_balance`
    /// USDC, and a tx with the given gas_limit, gas_price, and value.
    fn build_erc20_ctx(
        caller: Address,
        eth_balance: U256,
        usdc_balance: U256,
        gas_limit: u64,
        gas_price: u128,
        value: U256,
    ) -> SeismicContext<InMemoryDB> {
        let mut db = InMemoryDB::default();

        // Seed caller's ETH balance.
        db.insert_account_info(
            caller,
            AccountInfo {
                balance: eth_balance,
                ..Default::default()
            },
        );

        // Seed TOKEN contract account (must exist for sload/sstore).
        db.insert_account_info(TOKEN, AccountInfo::default());

        // Seed caller's USDC balance in the TOKEN storage.
        if usdc_balance > U256::ZERO {
            let slot = erc_address_storage(caller);
            db.insert_account_storage(TOKEN, slot, usdc_balance.into())
                .unwrap();
        }

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

    /// Read the USDC balance of `addr` from the journal.
    fn read_usdc_balance<CTX: ContextTr>(ctx: &mut CTX, addr: Address) -> U256
    where
        <CTX::Db as Database>::Error: core::fmt::Debug,
    {
        let slot = erc_address_storage(addr);
        ctx.journal_mut().sload(TOKEN, slot).unwrap().data
    }

    #[test]
    fn test_erc20_gas_fallback_deducts_to_beneficiary() {
        // Caller has 0 ETH but plenty of USDC. Gas is paid directly to the
        // beneficiary — no TREASURY middleman.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000; // 1 gwei
        let usdc_balance = U256::from(1_000_000_000u64); // 1000 USDC

        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            usdc_balance,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(
            evm.ctx().chain().used_erc20_gas(),
            "should use ERC20 gas when ETH balance is insufficient"
        );

        // gas cost in wei = 100_000 * 1e9 = 1e14
        // gas cost in USDC = 1e14 / 1e12 = 100 USDC
        let expected = U256::from(100u64);
        let caller_after = read_usdc_balance(evm.ctx(), caller);
        let beneficiary_after = read_usdc_balance(evm.ctx(), beneficiary);
        assert_eq!(caller_after, usdc_balance - expected);
        assert_eq!(
            beneficiary_after, expected,
            "beneficiary should receive gas payment directly (no TREASURY)"
        );
    }

    #[test]
    fn test_eth_path_when_balance_sufficient() {
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        let eth_balance = U256::from(1_000_000_000_000_000u128); // 1e15, plenty

        let ctx = build_erc20_ctx(
            caller,
            eth_balance,
            U256::ZERO,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(
            !evm.ctx().chain().used_erc20_gas(),
            "sufficient ETH should use native path"
        );
    }

    #[test]
    fn test_erc20_gas_insufficient_usdc_balance_fails() {
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        // Need at least 100 USDC; give 50.
        let usdc_balance = U256::from(50u64);

        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            usdc_balance,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let result = handler.validate_against_state_and_deduct_caller(&mut evm);
        assert!(result.is_err());
    }

    #[test]
    fn test_erc20_gas_reimburse_from_beneficiary() {
        // After deduction, half the gas is used — the other half is returned
        // from the beneficiary back to the caller in USDC.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        let usdc_balance = U256::from(1_000_000_000u64);

        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            usdc_balance,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        let beneficiary_before_reimburse = read_usdc_balance(evm.ctx(), beneficiary);
        let caller_before_reimburse = read_usdc_balance(evm.ctx(), caller);

        // Build a gas object with 50k remaining out of 79k execution budget.
        let mut gas = Gas::new(gas_limit - 21_000);
        let _ = gas.record_cost(29_000);
        let mut exec_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: InstructionResult::Stop,
                output: Bytes::new(),
                gas,
            },
            0..0,
        ));

        handler
            .reimburse_caller(&mut evm, &mut exec_result)
            .unwrap();

        // 50_000 * 1e9 / 1e12 = 50 USDC refunded.
        let expected_refund = U256::from(50u64);
        assert_eq!(
            read_usdc_balance(evm.ctx(), caller),
            caller_before_reimburse + expected_refund
        );
        assert_eq!(
            read_usdc_balance(evm.ctx(), beneficiary),
            beneficiary_before_reimburse - expected_refund
        );
    }

    #[test]
    fn test_erc20_gas_beneficiary_gets_net_reward_no_burn() {
        // End-to-end invariant test: deduct → reimburse → reward_beneficiary.
        // The key invariant: no tokens are burned. caller_final + beneficiary_final
        // must equal the initial USDC supply.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 2_000_000_000; // 2 gwei
        let usdc_balance = U256::from(1_000_000_000u64);

        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            usdc_balance,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let ctx = ctx.modify_block_chained(|b| {
            b.basefee = 1_000_000_000; // 1 gwei — would be burned on mainnet
        });
        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        // 1. Deduct. 2e9 * 100_000 / 1e12 = 200 USDC goes to beneficiary.
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();
        assert_eq!(
            read_usdc_balance(evm.ctx(), beneficiary),
            U256::from(200u64)
        );

        // 2. Reimburse: 50k gas remaining. 50_000 * 2e9 / 1e12 = 100 USDC back.
        let mut gas = Gas::new(gas_limit - 21_000);
        let _ = gas.record_cost(29_000);
        let mut exec_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: InstructionResult::Stop,
                output: Bytes::new(),
                gas,
            },
            0..0,
        ));
        handler
            .reimburse_caller(&mut evm, &mut exec_result)
            .unwrap();

        // 3. Reward beneficiary: no-op for ERC20.
        let beneficiary_before_reward = read_usdc_balance(evm.ctx(), beneficiary);
        handler
            .reward_beneficiary(&mut evm, &mut exec_result)
            .unwrap();
        let beneficiary_after_reward = read_usdc_balance(evm.ctx(), beneficiary);
        assert_eq!(
            beneficiary_before_reward, beneficiary_after_reward,
            "reward_beneficiary must be a no-op for ERC20 path"
        );

        // Invariant: no tokens burned.
        let caller_final = read_usdc_balance(evm.ctx(), caller);
        let beneficiary_final = read_usdc_balance(evm.ctx(), beneficiary);
        assert_eq!(
            caller_final + beneficiary_final,
            usdc_balance,
            "no tokens should be burned: caller + beneficiary = initial supply"
        );
    }

    #[test]
    fn test_erc20_gas_with_value_checks_eth_for_value() {
        // Caller has 0 ETH but value > 0 — must fail (value transferred in ETH).
        let caller = address!("0x0000000000000000000000000000000000001234");
        let value = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
        let usdc_balance = U256::from(1_000_000_000u64);

        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            usdc_balance,
            100_000,
            1_000_000_000,
            value,
        );
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let result = handler.validate_against_state_and_deduct_caller(&mut evm);
        assert!(
            result.is_err(),
            "tx with value > 0 and 0 ETH should fail even with USDC"
        );
    }

    #[test]
    fn test_erc20_gas_with_value_succeeds_when_eth_covers_value() {
        // Caller has enough ETH for value (but not gas+value) and enough USDC
        // for gas only. Should succeed via ERC20 path.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let value = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
        let eth_balance = value; // exactly covers value
        let usdc_balance = U256::from(1_000_000_000u64);

        let ctx = build_erc20_ctx(
            caller,
            eth_balance,
            usdc_balance,
            100_000,
            1_000_000_000,
            value,
        );
        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(evm.ctx().chain().used_erc20_gas());
        // Only gas is deducted in USDC (100), not gas+value.
        let expected_gas_usdc = U256::from(100u64);
        assert_eq!(
            read_usdc_balance(evm.ctx(), caller),
            usdc_balance - expected_gas_usdc
        );
        assert_eq!(read_usdc_balance(evm.ctx(), beneficiary), expected_gas_usdc);
    }

    #[test]
    fn test_erc20_gas_usdc_check_excludes_value() {
        // Regression: USDC check must only cover gas, not gas+value.
        // 100 USDC is exactly enough for gas. If value were included, it would
        // require 1_000_100 USDC and fail.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let value = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
        let eth_balance = value;
        let usdc_balance = U256::from(100u64);

        let ctx = build_erc20_ctx(
            caller,
            eth_balance,
            usdc_balance,
            100_000,
            1_000_000_000,
            value,
        );
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let result = handler.validate_against_state_and_deduct_caller(&mut evm);
        assert!(
            result.is_ok(),
            "USDC check should cover gas only, not value"
        );
    }

    #[test]
    fn test_erc20_gas_sub_divisor_charges_at_least_one_unit() {
        // Regression test for floor-division bug: when gas_balance_spending is
        // less than WEI_TO_USDC_DIVISOR (10^12 wei), floor division would yield
        // 0 USDC deducted, allowing free transactions. The fix uses ceiling
        // division so at least 1 unit of USDC is charged.
        //
        // Repro from writeup: baseFee ≈ 7 wei, gas_limit=200_000,
        // effective_gas_price ≈ 10^6 wei → gas_balance_spending = 2*10^11 wei,
        // which is < 10^12. Under floor: 0 USDC charged. Under ceil: 1 USDC.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 200_000;
        let gas_price: u128 = 1_000_000; // 10^6 wei — sub-divisor
        let usdc_balance = U256::from(100u64);

        let ctx =
            build_erc20_ctx(caller, U256::ZERO, usdc_balance, gas_limit, gas_price, U256::ZERO);
        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        // gas_balance_spending = 200_000 * 10^6 = 2*10^11 wei (< 10^12).
        // Ceiling division: ceil(2*10^11 / 10^12) = 1 USDC unit.
        let caller_after = read_usdc_balance(evm.ctx(), caller);
        let beneficiary_after = read_usdc_balance(evm.ctx(), beneficiary);
        assert_eq!(
            caller_after,
            usdc_balance - U256::from(1u64),
            "caller must be charged at least 1 USDC unit for sub-divisor gas costs"
        );
        assert_eq!(
            beneficiary_after,
            U256::from(1u64),
            "beneficiary must receive at least 1 USDC unit (no free txs)"
        );
    }

    #[test]
    fn test_erc20_gas_ceil_deduct_matches_pre_flight_check() {
        // The deduction's ceiling division must produce the same value as the
        // pre-flight check's ceiling. If the caller has exactly max_gas_spending_usdc
        // in USDC, the deduction must succeed (not fail with insufficient balance).
        //
        // Scenario: gas_limit=100_001, gas_price=10^9 → gas_balance_spending =
        // 100_001_000_000_000 wei. ceil(./10^12) = 101 USDC.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_001;
        let gas_price: u128 = 1_000_000_000;
        let usdc_balance = U256::from(101u64);

        let ctx =
            build_erc20_ctx(caller, U256::ZERO, usdc_balance, gas_limit, gas_price, U256::ZERO);
        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        // Exactly 101 USDC deducted → caller: 0, beneficiary: 101.
        assert_eq!(read_usdc_balance(evm.ctx(), caller), U256::ZERO);
        assert_eq!(read_usdc_balance(evm.ctx(), beneficiary), U256::from(101u64));
    }

    #[test]
    fn test_wei_to_usdc_ceiling_division() {
        // gas_limit=100_001 gives 100_001_000_000_000 wei; ceil(./1e12) = 101.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_001;
        let gas_price: u128 = 1_000_000_000;

        // 100 USDC should NOT be enough.
        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            U256::from(100u64),
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();
        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        assert!(handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .is_err());

        // 101 USDC should be sufficient.
        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            U256::from(101u64),
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();
        assert!(handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .is_ok());
    }

    #[test]
    fn test_erc20_gas_exact_eth_boundary_uses_native() {
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        let exact_eth = U256::from(100_000_000_000_000u128); // = max_balance_spending

        let ctx = build_erc20_ctx(
            caller,
            exact_eth,
            U256::ZERO,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(!evm.ctx().chain().used_erc20_gas());
    }

    #[test]
    fn test_erc20_gas_one_wei_short_falls_to_erc20() {
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        let one_short = U256::from(100_000_000_000_000u128) - U256::from(1);
        let usdc_balance = U256::from(1_000_000_000u64);

        let ctx = build_erc20_ctx(
            caller,
            one_short,
            usdc_balance,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(evm.ctx().chain().used_erc20_gas());
    }

    #[test]
    fn test_native_eth_no_basefee_burn() {
        // Native ETH path with non-zero basefee. The beneficiary must receive
        // the full effective_gas_price * gas_used (no EIP-1559 burn).
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 2_000_000_000; // 2 gwei
        let eth_balance = U256::from(1_000_000_000_000_000_000u128); // 1 ETH

        let ctx = build_erc20_ctx(
            caller,
            eth_balance,
            U256::ZERO,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
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

        // Simulate all 79k execution gas used.
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

        // No basefee burn: reward = effective_gas_price * gas_used.
        // = 2 gwei * 79_000 = 1.58e14 wei.
        let expected_reward = U256::from(2_000_000_000u128 * 79_000u128);
        assert_eq!(
            ben_balance_after - ben_balance_before,
            expected_reward,
            "beneficiary must receive full effective_gas_price (no basefee burn)"
        );

        // Mainnet (with burn) would give half as much.
        let mainnet_reward = U256::from(1_000_000_000u128 * 79_000u128);
        assert!(expected_reward > mainnet_reward);
    }

    #[test]
    fn test_erc20_gas_flag_reset_on_execution_result() {
        let caller = address!("0x0000000000000000000000000000000000001234");
        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            U256::from(1_000_000_000u64),
            100_000,
            1_000_000_000,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();
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

        assert!(!evm.ctx().chain().used_erc20_gas());
    }

    #[test]
    fn test_erc20_gas_flag_reset_on_catch_error() {
        let caller = address!("0x0000000000000000000000000000000000001234");
        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            U256::from(1_000_000_000u64),
            100_000,
            1_000_000_000,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();
        evm.ctx().chain_mut().set_used_erc20_gas();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let err: EVMError<_, InvalidTransaction> = EVMError::Custom("test".to_string());
        let _ = handler.catch_error(&mut evm, err);

        assert!(!evm.ctx().chain().used_erc20_gas());
    }

    #[test]
    fn test_erc_address_storage_deterministic() {
        let addr = address!("0x0000000000000000000000000000000000001234");
        assert_eq!(erc_address_storage(addr), erc_address_storage(addr));
    }

    #[test]
    fn test_erc_address_storage_different_addresses() {
        let a = address!("0x0000000000000000000000000000000000001234");
        let b = address!("0x0000000000000000000000000000000000005678");
        assert_ne!(erc_address_storage(a), erc_address_storage(b));
    }

    #[test]
    fn test_erc_address_storage_matches_solidity_mapping_layout() {
        // Pin the expected slot: standard Solidity `mapping(address => uint256)`
        // at position 3 computes slot as keccak256(addr_32 || slot_32).
        // Use web3-style hand computation to guard against accidental layout
        // changes (e.g., back to Solady) in the future.
        let addr = address!("0x1234567890abcdef1234567890abcdef12345678");

        let mut expected_input = [0u8; 64];
        expected_input[12..32].copy_from_slice(addr.as_slice());
        expected_input[63] = 3; // BALANCES_SLOT
        let expected: U256 = keccak256(expected_input).into();

        assert_eq!(erc_address_storage(addr), expected);
    }

    #[test]
    fn test_token_operation_transfer() {
        let sender = address!("0x0000000000000000000000000000000000001111");
        let recipient = address!("0x0000000000000000000000000000000000002222");
        let initial = U256::from(1000u64);
        let amount = U256::from(300u64);

        let mut db = InMemoryDB::default();
        db.insert_account_info(TOKEN, AccountInfo::default());
        db.insert_account_info(sender, AccountInfo::default());
        db.insert_account_info(recipient, AccountInfo::default());
        db.insert_account_storage(TOKEN, erc_address_storage(sender), initial.into())
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

        token_operation::<_, EVMError<_, InvalidTransaction>>(&mut ctx, sender, recipient, amount)
            .unwrap();

        assert_eq!(read_usdc_balance(&mut ctx, sender), initial - amount);
        assert_eq!(read_usdc_balance(&mut ctx, recipient), amount);
    }

    #[test]
    fn test_token_operation_insufficient_balance() {
        let sender = address!("0x0000000000000000000000000000000000001111");
        let recipient = address!("0x0000000000000000000000000000000000002222");

        let mut db = InMemoryDB::default();
        db.insert_account_info(TOKEN, AccountInfo::default());
        db.insert_account_info(sender, AccountInfo::default());
        db.insert_account_info(recipient, AccountInfo::default());
        db.insert_account_storage(
            TOKEN,
            erc_address_storage(sender),
            U256::from(100u64).into(),
        )
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
            U256::from(200u64),
        );
        assert!(result.is_err());
    }
}
