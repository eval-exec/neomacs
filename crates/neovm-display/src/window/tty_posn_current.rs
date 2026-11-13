//! TTY current-matrix admission independent of physical numeric frame backing.
//! One exclusive Frame mutator publishes initialized Arc values. Readers borrow
//! immutable numeric records; several Lisp mutators must use the existing
//! exclusive Frame ownership discipline. No record contains Lisp values or TLS.

pub(crate) use neovm_eval::emacs_core::eval::data::window_state::TtyPosnCurrentOwner;
pub use neovm_eval::emacs_core::eval::data::window_state::{
    TtyCurrentMatrixAuthority, TtyMatrixAllocation,
};
