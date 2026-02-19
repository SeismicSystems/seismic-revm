//!Handler related to Seismic chain
use crate::{api::exec::SeismicContextTr, SeismicHaltReason};
use revm::{
    context::{
        result::{ExecutionResult, InvalidTransaction},
        Cfg, ContextTr, JournalTr, LocalContextTr, Transaction,
    },
    context_interface::{context::ContextError, result::FromStringError, Block, Database},
    handler::{
        handler::EvmTrError,
        post_execution,
        pre_execution::validate_account_nonce_and_code,
        EthFrame, EvmTr, FrameResult, FrameTr, Handler, MainnetHandler,
    },
    inspector::{Inspector, InspectorEvmTr, InspectorHandler},
    interpreter::{interpreter::EthInterpreter, interpreter_action::FrameInit},
    primitives::{address, hardfork::SpecId, keccak256, Address, U256},
};

/// ERC20 token address used for gas payment on Seismic.
/// TODO: replace with the actual Seismic token contract address.
pub const TOKEN: Address = address!("0x215dfD51D1e6C05C1f7e322c0f9ddc607300e053");

/// Treasury address: collects gas payments and disburses reimbursements/rewards.
/// TODO: replace with the actual Seismic treasury address.
pub const TREASURY: Address = address!("0x0000000000000000000000000000000000000169");

/// Returns the ERC20 `balances` mapping storage slot for `address`.
/// Implements standard Solidity mapping layout: keccak256(abi.encode(address, slot_index=4))
pub(crate) fn erc_address_storage(addr: Address) -> U256 {
    let mut buf = [0u8; 64];
    buf[12..32].copy_from_slice(addr.as_slice()); // address padded to 32 bytes
    buf[63] = 4; // U256::from(4) in big-endian
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
    context
        .journal_mut()
        .sstore(TOKEN, recipient_slot, recipient_balance.saturating_add(amount))?;

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
    type HaltReason = SeismicHaltReason;

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

        if tx.kind().is_call() {
            caller_account.info.nonce = caller_account.info.nonce.saturating_add(1);
        }

        // Touch account so we know it is changed.
        caller_account.mark_touch();

        let max_balance_spending = tx.max_balance_spending()?;
        let effective_balance_spending = tx
            .effective_balance_spending(basefee, blob_price)
            .expect("effective balance is always smaller than max balance so it can't overflow");

        let account_balance_slot = erc_address_storage(caller);
        context.journal_mut().load_account(TOKEN)?.data.mark_touch();

        let account_balance = context
            .journal_mut()
            .sload(TOKEN, account_balance_slot)
            .map(|v| v.data)
            .unwrap_or_default();

        if !is_balance_check_disabled {
            if max_balance_spending > account_balance {
                return Err(InvalidTransaction::LackOfFundForMaxFee {
                    fee: Box::new(max_balance_spending),
                    balance: Box::new(account_balance),
                }
                .into());
            }

            // Subtract max balance spending minus the value (value is transferred during execution).
            let gas_balance_spending = effective_balance_spending - value;
            token_operation::<EVM::Context, ERROR>(context, caller, TREASURY, gas_balance_spending)?;
        }

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
        let basefee = context.block().basefee() as u128;
        let caller = context.tx().caller();
        let effective_gas_price = context.tx().effective_gas_price(basefee);
        let gas = exec_result.gas();

        let reimbursement = effective_gas_price
            .saturating_mul((gas.remaining() + gas.refunded() as u64) as u128);

        token_operation::<EVM::Context, ERROR>(
            context,
            TREASURY,
            caller,
            U256::from(reimbursement),
        )?;

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
        let tx = context.tx();
        let beneficiary = context.block().beneficiary();
        let basefee = context.block().basefee() as u128;
        let effective_gas_price = tx.effective_gas_price(basefee);
        let gas = exec_result.gas();

        let coinbase_gas_price =
            if SpecId::from(context.cfg().spec()).is_enabled_in(SpecId::LONDON) {
                effective_gas_price.saturating_sub(basefee)
            } else {
                effective_gas_price
            };

        let reward = coinbase_gas_price.saturating_mul(gas.used() as u128);
        token_operation::<EVM::Context, ERROR>(
            context,
            TREASURY,
            beneficiary,
            U256::from(reward),
        )?;

        Ok(())
    }

    /// Processes the final execution output.
    ///
    /// This method, retrieves the final state from the journal, converts internal results to the external output format.
    /// Internal state is cleared and EVM is prepared for the next transaction.
    ///
    /// Seismic Addendum
    /// Given that we can't yet pass instruction_result which aren't in the InstructionResult enum,
    /// We leverage context_error to bubble up our instruction set specific errors! We also clear
    /// the rng state on returns that won't go through catch_error.
    #[inline]
    fn execution_result(
        &mut self,
        evm: &mut Self::Evm,
        result: <<Self::Evm as EvmTr>::Frame as FrameTr>::FrameResult,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        match core::mem::replace(evm.ctx().error(), Ok(())) {
            Err(ContextError::Db(e)) => return Err(e.into()),
            Err(ContextError::Custom(e)) => {
                if let Some(seismic_reason) =
                    SeismicHaltReason::try_from_error_string(&e.to_string())
                {
                    // Same as catch error, except don't discard tx
                    evm.ctx().local_mut().clear();
                    evm.frame_stack().clear();
                    evm.ctx().chain_mut().reset_rng();

                    return Ok(ExecutionResult::Halt {
                        reason: seismic_reason,
                        gas_used: evm.ctx().tx().gas_limit(),
                    });
                }
                return Err(Self::Error::from_string(e));
            }
            Ok(_) => (),
        }

        let exec_result = post_execution::output(evm.ctx(), result);

        // commit transaction
        evm.ctx().journal_mut().commit_tx();
        evm.ctx().local_mut().clear();
        evm.frame_stack().clear();
        // ...and we also reset the RNG
        evm.ctx().chain_mut().reset_rng();

        Ok(exec_result)
    }

    /// Handles cleanup when an error occurs during execution.
    ///
    /// Ensures the journal state is properly cleared before propagating the error.
    /// Also ensures the rng has been reset.
    /// On happy path journal is cleared in [`Handler::output`] method.
    #[inline]
    fn catch_error(
        &self,
        evm: &mut Self::Evm,
        error: Self::Error,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        // Same as in normal ETH handler...
        evm.ctx().local_mut().clear();
        evm.ctx().journal_mut().discard_tx();
        evm.frame_stack().clear();
        // ...except we also reset the RNG
        evm.ctx().chain_mut().reset_rng();
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
        let ctx = Context::seismic().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let gas = call_last_frame_return(ctx, InstructionResult::Revert, Gas::new(90));
        assert_eq!(gas.remaining(), 90);
        assert_eq!(gas.spent(), 10);
        assert_eq!(gas.refunded(), 0);
    }

    #[test]
    fn test_fatal_external_error_gas() {
        let ctx = Context::seismic().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let gas = call_last_frame_return(ctx, InstructionResult::FatalExternalError, Gas::new(90));
        assert_eq!(gas.remaining(), 0);
        assert_eq!(gas.spent(), 100);
        assert_eq!(gas.refunded(), 0);
    }
}
