//! Constant-time replacements for EVM comparison opcodes.
//!
//! Standard U256 comparisons (==, <, >) short-circuit on the first differing
//! 64-bit limb, leaking partial information about confidential values through
//! timing side channels. These replacements always process all 4 limbs.
//!
//! `core::hint::black_box` is applied to intermediate results to discourage
//! the compiler from reassembling a short-circuit pattern. The `subtle` crate
//! is often recommended for constant-time code, but since Rust 1.66 its
//! optimization barrier is itself just `core::hint::black_box` (enabled via
//! the `core_hint_black_box` feature flag; before 1.66 it used a volatile
//! read). Using `black_box` directly avoids the extra dependency while
//! providing the same underlying mechanism.
//!
//! Note that all of this is **best-effort**: `black_box` is a hint, not a
//! guarantee, and the compiler or LLVM is free to ignore it. A true fix
//! would require language-level secret types (RFC 2859) with backend support
//! to preserve constant-time semantics across all optimization passes, but
//! that RFC is currently postponed.
//!
//! ## Scope
//!
//! Only the 6 comparison/branch opcodes are replaced here (EQ, LT, GT, SLT,
//! SGT, ISZERO). Other opcodes were audited and fall into three categories:
//!
//! **Already constant-time:** AND, OR, XOR, NOT, ADD, SUB, MUL — these are
//! bitwise or wrapping-arithmetic operations that unconditionally process all
//! 4 limbs with no data-dependent branches.
//!
//! **Variable-time but impractical to fix:** DIV, MOD, SDIV, SMOD, EXP,
//! ADDMOD, MULMOD — multi-precision division and modular reduction are
//! inherently variable-time. Constant-time alternatives exist but incur
//! 10-100x overhead. EXP already leaks exponent size via gas cost.
//! Developers should avoid these on secret values and use precompiles for
//! cryptographic operations instead.
//!
//! **Minor leaks (1 bit):** SAR and SIGNEXTEND branch on the sign bit of the
//! value. SHL, SHR, and BYTE branch on the index/shift amount (typically not
//! secret). These could be made branchless cheaply but are lower priority.

use revm::{
    interpreter::{
        _count,
        interpreter_types::{InterpreterTypes, StackTr},
        popn_top, Host, Instruction, InstructionContext,
    },
    primitives::U256,
};

// ---------------------------------------------------------------------------
// Constant-time primitives
// ---------------------------------------------------------------------------

/// Constant-time equality: XOR all limbs, OR the diffs, compare once.
#[inline]
fn ct_eq(a: &U256, b: &U256) -> bool {
    let al = a.as_limbs();
    let bl = b.as_limbs();
    let d0 = core::hint::black_box(al[0] ^ bl[0]);
    let d1 = core::hint::black_box(al[1] ^ bl[1]);
    let d2 = core::hint::black_box(al[2] ^ bl[2]);
    let d3 = core::hint::black_box(al[3] ^ bl[3]);
    (d0 | d1 | d2 | d3) == 0
}

/// Constant-time unsigned less-than on raw limbs (little-endian `[u64; 4]`).
///
/// Performs multi-limb subtraction `a - b` and returns `true` when the final
/// borrow bit is set (i.e. `a < b`). Every limb is always visited.
#[inline]
fn ct_lt_limbs(al: &[u64; 4], bl: &[u64; 4]) -> bool {
    let mut borrow: u64 = 0;
    for i in 0..4 {
        let wide = core::hint::black_box(
            (al[i] as u128)
                .wrapping_sub(bl[i] as u128)
                .wrapping_sub(borrow as u128),
        );
        borrow = (wide >> 127) as u64;
    }
    borrow != 0
}

/// Constant-time unsigned less-than for U256.
#[inline]
fn ct_lt(a: &U256, b: &U256) -> bool {
    ct_lt_limbs(a.as_limbs(), b.as_limbs())
}

/// Constant-time signed less-than for i256 (two's complement U256).
///
/// Flips the sign bit (bit 255) so that unsigned comparison on the
/// transformed values gives the correct signed ordering.
#[inline]
fn ct_i256_lt(a: &U256, b: &U256) -> bool {
    let al = a.as_limbs();
    let bl = b.as_limbs();
    let a_flipped = [al[0], al[1], al[2], al[3] ^ 0x8000_0000_0000_0000];
    let b_flipped = [bl[0], bl[1], bl[2], bl[3] ^ 0x8000_0000_0000_0000];
    ct_lt_limbs(&a_flipped, &b_flipped)
}

/// Constant-time is-zero: OR all limbs, compare once.
#[inline]
fn ct_is_zero(a: &U256) -> bool {
    let al = a.as_limbs();
    let d = core::hint::black_box(al[0] | al[1] | al[2] | al[3]);
    d == 0
}

// ---------------------------------------------------------------------------
// Opcode implementations (drop-in replacements for upstream bitwise::*)
// ---------------------------------------------------------------------------

/// Constant-time EQ (0x14).
pub fn eq<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    popn_top!([op1], op2, context.interpreter);
    *op2 = U256::from(ct_eq(&op1, op2));
}

/// Constant-time LT (0x10).
pub fn lt<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    popn_top!([op1], op2, context.interpreter);
    *op2 = U256::from(ct_lt(&op1, op2));
}

/// Constant-time GT (0x11).
pub fn gt<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    popn_top!([op1], op2, context.interpreter);
    *op2 = U256::from(ct_lt(op2, &op1));
}

/// Constant-time SLT (0x12).
pub fn slt<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    popn_top!([op1], op2, context.interpreter);
    *op2 = U256::from(ct_i256_lt(&op1, op2));
}

/// Constant-time SGT (0x13).
pub fn sgt<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    popn_top!([op1], op2, context.interpreter);
    *op2 = U256::from(ct_i256_lt(op2, &op1));
}

/// Constant-time ISZERO (0x15).
pub fn iszero<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    popn_top!([], op1, context.interpreter);
    *op1 = U256::from(ct_is_zero(op1));
}

// ---------------------------------------------------------------------------
// Instruction constructors (same pattern as confidential_storage.rs)
// ---------------------------------------------------------------------------

/// Gas cost for the comparison opcodes (VERYLOW = 3).
const VERYLOW: u64 = 3;

pub fn ct_eq_instruction<WIRE: InterpreterTypes, H: Host + ?Sized>() -> Instruction<WIRE, H> {
    Instruction::new(eq, VERYLOW)
}

pub fn ct_lt_instruction<WIRE: InterpreterTypes, H: Host + ?Sized>() -> Instruction<WIRE, H> {
    Instruction::new(lt, VERYLOW)
}

pub fn ct_gt_instruction<WIRE: InterpreterTypes, H: Host + ?Sized>() -> Instruction<WIRE, H> {
    Instruction::new(gt, VERYLOW)
}

pub fn ct_slt_instruction<WIRE: InterpreterTypes, H: Host + ?Sized>() -> Instruction<WIRE, H> {
    Instruction::new(slt, VERYLOW)
}

pub fn ct_sgt_instruction<WIRE: InterpreterTypes, H: Host + ?Sized>() -> Instruction<WIRE, H> {
    Instruction::new(sgt, VERYLOW)
}

pub fn ct_iszero_instruction<WIRE: InterpreterTypes, H: Host + ?Sized>() -> Instruction<WIRE, H> {
    Instruction::new(iszero, VERYLOW)
}

#[cfg(test)]
mod tests {
    use super::*;
    use revm::{
        interpreter::{host::DummyHost, interpreter::EthInterpreter, Interpreter},
        primitives::uint,
    };

    type TestInterp = Interpreter<EthInterpreter>;

    // Helper to run a binary comparison opcode on two values.
    fn run_binary(
        f: fn(InstructionContext<'_, DummyHost, EthInterpreter>),
        a: U256,
        b: U256,
    ) -> U256 {
        let mut interp = TestInterp::default();
        assert!(interp.stack.push(b));
        assert!(interp.stack.push(a));
        let ctx = InstructionContext {
            host: &mut DummyHost,
            interpreter: &mut interp,
        };
        f(ctx);
        interp.stack.pop().unwrap()
    }

    // Helper to run a unary opcode on one value.
    fn run_unary(f: fn(InstructionContext<'_, DummyHost, EthInterpreter>), a: U256) -> U256 {
        let mut interp = TestInterp::default();
        assert!(interp.stack.push(a));
        let ctx = InstructionContext {
            host: &mut DummyHost,
            interpreter: &mut interp,
        };
        f(ctx);
        interp.stack.pop().unwrap()
    }

    // ---- EQ ----

    #[test]
    fn test_eq_equal() {
        uint! {
            assert_eq!(run_binary(eq, 42_U256, 42_U256), 1_U256);
            assert_eq!(run_binary(eq, 0_U256, 0_U256), 1_U256);
            assert_eq!(run_binary(eq, U256::MAX, U256::MAX), 1_U256);
        }
    }

    #[test]
    fn test_eq_not_equal() {
        uint! {
            assert_eq!(run_binary(eq, 1_U256, 2_U256), 0_U256);
            assert_eq!(run_binary(eq, 0_U256, 1_U256), 0_U256);
        }
    }

    // ---- LT ----

    #[test]
    fn test_lt() {
        uint! {
            assert_eq!(run_binary(lt, 1_U256, 2_U256), 1_U256);
            assert_eq!(run_binary(lt, 2_U256, 1_U256), 0_U256);
            assert_eq!(run_binary(lt, 1_U256, 1_U256), 0_U256);
            assert_eq!(run_binary(lt, 0_U256, U256::MAX), 1_U256);
            assert_eq!(run_binary(lt, U256::MAX, 0_U256), 0_U256);
        }
    }

    // ---- GT ----

    #[test]
    fn test_gt() {
        uint! {
            assert_eq!(run_binary(gt, 2_U256, 1_U256), 1_U256);
            assert_eq!(run_binary(gt, 1_U256, 2_U256), 0_U256);
            assert_eq!(run_binary(gt, 1_U256, 1_U256), 0_U256);
        }
    }

    // ---- SLT (signed) ----

    #[test]
    fn test_slt() {
        uint! {
            // Positive comparisons
            assert_eq!(run_binary(slt, 1_U256, 2_U256), 1_U256);
            assert_eq!(run_binary(slt, 2_U256, 1_U256), 0_U256);

            // -1 (0xFFF...F) < 0
            assert_eq!(run_binary(slt, -1_U256, 0_U256), 1_U256);
            // 0 < -1 is false
            assert_eq!(run_binary(slt, 0_U256, -1_U256), 0_U256);

            // -1 < -2 is false (-1 > -2)
            assert_eq!(run_binary(slt, -1_U256, -2_U256), 0_U256);
            // -2 < -1
            assert_eq!(run_binary(slt, -2_U256, -1_U256), 1_U256);

            // Equal
            assert_eq!(run_binary(slt, -1_U256, -1_U256), 0_U256);
        }
    }

    // ---- SGT (signed) ----

    #[test]
    fn test_sgt() {
        uint! {
            assert_eq!(run_binary(sgt, 2_U256, 1_U256), 1_U256);
            assert_eq!(run_binary(sgt, 1_U256, 2_U256), 0_U256);
            assert_eq!(run_binary(sgt, 0_U256, -1_U256), 1_U256);
            assert_eq!(run_binary(sgt, -1_U256, 0_U256), 0_U256);
        }
    }

    // ---- ISZERO ----

    #[test]
    fn test_iszero() {
        uint! {
            assert_eq!(run_unary(iszero, 0_U256), 1_U256);
            assert_eq!(run_unary(iszero, 1_U256), 0_U256);
            assert_eq!(run_unary(iszero, U256::MAX), 0_U256);
        }
    }

    // ---- Edge cases: values that differ only in one limb ----
    // These are the exact cases where short-circuiting leaks information.

    #[test]
    fn test_eq_single_limb_difference() {
        // Differ only in limb 0 (least significant)
        let a = U256::from_limbs([1, 0, 0, 0]);
        let b = U256::from_limbs([2, 0, 0, 0]);
        assert_eq!(run_binary(eq, a, b), U256::ZERO);

        // Differ only in limb 3 (most significant)
        let a = U256::from_limbs([0, 0, 0, 1]);
        let b = U256::from_limbs([0, 0, 0, 2]);
        assert_eq!(run_binary(eq, a, b), U256::ZERO);

        // Match on limbs 0-2, differ on limb 3
        let a = U256::from_limbs([42, 42, 42, 1]);
        let b = U256::from_limbs([42, 42, 42, 2]);
        assert_eq!(run_binary(eq, a, b), U256::ZERO);
    }

    #[test]
    fn test_lt_boundary_values() {
        // Values that share a prefix of matching limbs
        let a = U256::from_limbs([0, 0, 0, 0]);
        let b = U256::from_limbs([1, 0, 0, 0]);
        assert_eq!(run_binary(lt, a, b), U256::from(1));

        // Differ only in the most significant limb
        let a = U256::from_limbs([0, 0, 0, 1]);
        let b = U256::from_limbs([0, 0, 0, 2]);
        assert_eq!(run_binary(lt, a, b), U256::from(1));

        // Borrow propagation: a = 0, b = 1 (through all limbs)
        let a = U256::from_limbs([0, 0, 0, 0]);
        let b = U256::from_limbs([u64::MAX, u64::MAX, u64::MAX, u64::MAX]);
        assert_eq!(run_binary(lt, a, b), U256::from(1));
    }

    #[test]
    fn test_slt_sign_boundary() {
        // Max positive vs min negative (in two's complement)
        // 0x7FFF...F (max positive) vs 0x8000...0 (min negative)
        let max_pos = U256::from_limbs([u64::MAX, u64::MAX, u64::MAX, 0x7FFF_FFFF_FFFF_FFFF]);
        let min_neg = U256::from_limbs([0, 0, 0, 0x8000_0000_0000_0000]);
        // max_pos > min_neg in signed
        assert_eq!(run_binary(slt, max_pos, min_neg), U256::ZERO);
        assert_eq!(run_binary(slt, min_neg, max_pos), U256::from(1));
    }
}
