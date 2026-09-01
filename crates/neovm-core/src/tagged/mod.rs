//! Tagged pointer value system — replaces ObjId-based Value enum.
//!
//! Every Lisp value fits in a single `usize` (8 bytes on 64-bit).
//! The low 3 bits encode the type tag; heap pointers are 8-byte aligned
//! so those bits are always zero and available for tagging.
//!
//! This matches GNU Emacs's `Lisp_Object` design exactly.

pub mod gc;
pub mod header;
pub mod mutate;
pub mod value;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_test;
