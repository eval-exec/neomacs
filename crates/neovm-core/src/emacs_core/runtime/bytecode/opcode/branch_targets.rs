//! Compile-time view of an instruction's encoded control-flow targets.
//!
//! This view carries no Lisp state and adds no field to the serialized Op.
//! Switch tables are runtime operands and require a function/CFG to resolve.
use super::Op;

/// An instruction-index target already carried by a decoded Op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InstructionPc(u32);

impl InstructionPc {
    /// Original decoded instruction index, without converting byte offsets.
    pub(crate) const fn get(self) -> u32 {
        self.0
    }
}

/// Direct jumps, exceptional resume edges, and implicit switch tables differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BranchTargets {
    None,
    Direct(InstructionPc),
    Handler(InstructionPc),
    /// The table is a runtime stack operand; no static table ID exists in Op.
    SwitchTable,
}

impl Op {
    #[deny(clippy::wildcard_enum_match_arm)]
    pub(crate) fn branch_targets(&self) -> BranchTargets {
        match self {
            Self::Goto(target)
            | Self::GotoIfNil(target)
            | Self::GotoIfNotNil(target)
            | Self::GotoIfNilElsePop(target)
            | Self::GotoIfNotNilElsePop(target) => BranchTargets::Direct(InstructionPc(*target)),
            Self::PushConditionCase(target)
            | Self::PushConditionCaseRaw(target)
            | Self::PushCatch(target) => BranchTargets::Handler(InstructionPc(*target)),
            Self::Switch => BranchTargets::SwitchTable,
            Self::Constant(..)
            | Self::Nil
            | Self::True
            | Self::Pop
            | Self::Dup
            | Self::StackRef(..)
            | Self::StackSet(..)
            | Self::DiscardN(..)
            | Self::VarRef(..)
            | Self::VarSet(..)
            | Self::VarBind(..)
            | Self::Unbind(..)
            | Self::Call(..)
            | Self::Apply(..)
            | Self::Return
            | Self::Add
            | Self::Sub
            | Self::Mul
            | Self::Div
            | Self::Rem
            | Self::Add1
            | Self::Sub1
            | Self::Negate
            | Self::Eqlsign
            | Self::Gtr
            | Self::Lss
            | Self::Leq
            | Self::Geq
            | Self::Max
            | Self::Min
            | Self::Car
            | Self::Cdr
            | Self::Cons
            | Self::List(..)
            | Self::Length
            | Self::Nth
            | Self::Nthcdr
            | Self::Setcar
            | Self::Setcdr
            | Self::CarSafe
            | Self::CdrSafe
            | Self::Elt
            | Self::Nconc
            | Self::Nreverse
            | Self::Member
            | Self::Memq
            | Self::Assq
            | Self::Symbolp
            | Self::Consp
            | Self::Stringp
            | Self::Listp
            | Self::Integerp
            | Self::Numberp
            | Self::Null
            | Self::Not
            | Self::Eq
            | Self::Equal
            | Self::Concat(..)
            | Self::Substring
            | Self::StringEqual
            | Self::StringLessp
            | Self::Aref
            | Self::Aset
            | Self::SymbolValue
            | Self::SymbolFunction
            | Self::Set
            | Self::Fset
            | Self::Get
            | Self::Put
            | Self::PopHandler
            | Self::UnwindProtectPop
            | Self::Throw
            | Self::SaveCurrentBuffer
            | Self::SaveExcursion
            | Self::SaveRestriction
            | Self::SaveWindowExcursion
            | Self::MakeClosure(..)
            | Self::CallBuiltin(..)
            | Self::CallBuiltinSym(..)
            | Self::TrapOutOfRangeConstant(..) => BranchTargets::None,
        }
    }
}

#[cfg(test)]
#[path = "../tests/branch_targets.rs"]
mod tests;
