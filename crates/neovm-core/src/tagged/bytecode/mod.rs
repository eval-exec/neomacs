//! Lower bytecode storage vocabulary (T1 storage cut).
//!
//! The immutable storage/IR/parameter maps that the bytecode objects carry
//! live here, in the tagged lower root. Every decoder, verifier, VM, JIT and
//! pdump implementation stays in `emacs_core::bytecode` (as inherent impls on
//! these types during this stage); the upper module re-exports the moved
//! names so all existing paths keep resolving.
//!
//! The two facade statements below are the only references from this tree to
//! the still-upper value/intern vocabulary; child modules use `super::` and
//! add no crate-rooted upper paths. No compiler type (`CompiledLeaf`,
//! `jit::compile`) is named here.

pub(crate) use crate::emacs_core::intern::SymId;
pub(crate) use crate::emacs_core::value::{LambdaParams, Value, jit_runtime::Runtime};

pub(crate) mod chunk;
pub(crate) mod decode_data;
pub(crate) mod function_slots;
pub(crate) mod opcode;
