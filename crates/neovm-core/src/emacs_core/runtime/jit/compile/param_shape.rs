//! Native parameter slots validated before compiler allocations.
//!
//! Threading: immutable scalar metadata for one compile. This view borrows no
//! Lisp object, publishes no mutator state, and can be used by either compiler.

use super::CompileError;
use crate::emacs_core::bytecode::{ByteCodeFunction, RestSlot, StackParamShape};

/// A stack shape that fits its declared frame and the optimizer's u16 slot IDs.
#[derive(Clone, Copy, Debug)]
pub(crate) struct JitParamShape {
    shape: StackParamShape,
    optional: usize,
    entry_depth: usize,
}

impl TryFrom<&ByteCodeFunction> for JitParamShape {
    type Error = CompileError;

    #[inline]
    fn try_from(function: &ByteCodeFunction) -> Result<Self, Self::Error> {
        let shape = function
            .params
            .stack_shape()
            .ok_or(CompileError::TakesArguments)?;
        let optional = shape.optional().ok_or(CompileError::TakesArguments)?;
        let entry_depth = shape
            .entry_depth()
            .map_err(|_| CompileError::TakesArguments)?;
        if entry_depth > function.max_stack.get() {
            return Err(CompileError::TakesArguments);
        }
        // Opcode::Arg/OsrSlot carry u16 indices. Validate the last index before
        // the builder reserves a Vec for every slot; zero slots need no index.
        if let Some(last_slot) = entry_depth.checked_sub(1) {
            u16::try_from(last_slot).map_err(|_| CompileError::BadOperand)?;
        }
        Ok(Self {
            shape,
            optional,
            entry_depth,
        })
    }
}

impl JitParamShape {
    #[inline]
    pub(crate) const fn required(self) -> usize {
        self.shape.required()
    }
    #[inline]
    pub(crate) const fn nonrest(self) -> usize {
        self.shape.nonrest()
    }
    #[inline]
    pub(crate) const fn rest(self) -> RestSlot {
        self.shape.rest()
    }
    #[inline]
    pub(crate) const fn optional(self) -> usize {
        self.optional
    }
    #[inline]
    pub(crate) const fn entry_depth(self) -> usize {
        self.entry_depth
    }
    #[inline]
    pub(crate) fn fixed_arity(self) -> Option<usize> {
        (!self.rest().is_present() && self.optional == 0).then_some(self.required())
    }
}

#[cfg(test)]
#[path = "tests/param_shape.rs"]
mod tests;
