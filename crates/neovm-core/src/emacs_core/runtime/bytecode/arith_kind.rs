//! The arithmetic opcodes' generic slow arm, named.
//!
//! GNU's `Bplus`, `Bdiff`, `Bmult`, `Bgtr` and friends call `Fplus (2, &TOP)`,
//! `arithcompare` and so on directly when their operands are not both
//! fixnums (`src/bytecode.c:1267-1373`): no funcall, no backtrace frame. The
//! interpreter's opcode arms and the JIT's generic fallback
//! (`neovm_jit_arith_generic`) share that slow arm; this enum is the one
//! name for which builtin it calls and with how many operands.
//!
//! THE DISCRIMINANTS ARE AN ABI: generated code bakes `kind as i64` into
//! every generic arithmetic site as an immediate (`lower_generic_arith_site`)
//! and AOT artifacts carry those immediates. Renumbering a kind requires
//! bumping `ABI_TAG_VERSION` (`jit/aot.rs`); `tests/arith_kind.rs` pins
//! every value.

use super::opcode::Op;

/// One arithmetic opcode's generic slow arm (see the module doc).
#[repr(i64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, strum::EnumIter)]
pub(crate) enum ArithGenericKind {
    /// `Bplus` → `+`
    Add = 0,
    /// `Bdiff` → `-`
    Sub = 1,
    /// `Bmult` → `*`
    Mul = 2,
    /// `Bquo` → `/`
    Div = 3,
    /// `Brem` → `%`
    Rem = 4,
    /// `Bmax` → `max`
    Max = 5,
    /// `Bmin` → `min`
    Min = 6,
    /// `Beqlsign` → `=`
    NumEq = 7,
    /// `Blss` → `<`
    Lt = 8,
    /// `Bgtr` → `>`
    Gt = 9,
    /// `Bleq` → `<=`
    Le = 10,
    /// `Bgeq` → `>=`
    Ge = 11,
    /// `Badd1` → `1+`
    Add1 = 12,
    /// `Bsub1` → `1-`
    Sub1 = 13,
    /// `Bnegate` → `-` with one operand
    Negate = 14,
}

impl ArithGenericKind {
    /// The kind of an arithmetic opcode with a generic fallback, `None` for
    /// any other opcode.
    #[inline]
    pub(crate) fn from_op(op: &Op) -> Option<Self> {
        Some(match op {
            Op::Add => Self::Add,
            Op::Sub => Self::Sub,
            Op::Mul => Self::Mul,
            Op::Div => Self::Div,
            Op::Rem => Self::Rem,
            Op::Max => Self::Max,
            Op::Min => Self::Min,
            Op::Eqlsign => Self::NumEq,
            Op::Lss => Self::Lt,
            Op::Gtr => Self::Gt,
            Op::Leq => Self::Le,
            Op::Geq => Self::Ge,
            Op::Add1 => Self::Add1,
            Op::Sub1 => Self::Sub1,
            Op::Negate => Self::Negate,
            _ => return None,
        })
    }

    /// The kind generated code passed as `kind as i64`; `None` for a value
    /// no kind has (a corrupted immediate).
    #[inline]
    pub(crate) fn from_raw(raw: i64) -> Option<Self> {
        Some(match raw {
            0 => Self::Add,
            1 => Self::Sub,
            2 => Self::Mul,
            3 => Self::Div,
            4 => Self::Rem,
            5 => Self::Max,
            6 => Self::Min,
            7 => Self::NumEq,
            8 => Self::Lt,
            9 => Self::Gt,
            10 => Self::Le,
            11 => Self::Ge,
            12 => Self::Add1,
            13 => Self::Sub1,
            14 => Self::Negate,
            _ => return None,
        })
    }

    /// Operands the opcode takes off the stack.
    #[inline]
    pub(crate) fn arity(self) -> usize {
        match self {
            Self::Add1 | Self::Sub1 | Self::Negate => 1,
            _ => 2,
        }
    }
}

#[cfg(test)]
#[path = "tests/arith_kind.rs"]
mod tests;
