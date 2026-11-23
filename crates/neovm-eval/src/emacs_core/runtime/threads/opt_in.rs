//! The opt-in parallel creator (pentry DESIGN section 6, E6-P.1).
//!
//! `neomacs-make-parallel-thread` is the only route that asks for N-mutator
//! admission; `make-thread` never asks. A refused request is not an error:
//! the creator starts an ordinary Baton thread, with the same object, the
//! same thread, mutex and condition APIs, and `neomacs-thread-parallel-p`
//! nil. No production admission exists yet (G3 wires `Gen1Admission` into
//! [`admit_parallel`]), so every request takes that fallback today.

use super::{builtin_make_thread, expect_args, expect_thread_id};
use crate::emacs_core::eval::Context;
use neovm_value::error::EvalResult;
use neovm_value::value::Value;

/// A granted N-mutator admission. It is uninhabited until a production
/// route exists, so the creator's admitted arm is statically unreachable,
/// and making the type inhabited forces that arm to be written.
pub(crate) enum ParallelAdmission {}

/// Why an opt-in request runs Baton-serial instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NMutatorEntryError {
    /// This build has no production N-mutator route yet.
    Unavailable,
}

/// The admission the opt-in creator asks for.
fn admit_parallel(_eval: &Context) -> Result<ParallelAdmission, NMutatorEntryError> {
    Err(NMutatorEntryError::Unavailable)
}

/// `(neomacs-make-parallel-thread FUNCTION &optional NAME BUFFER-DISPOSITION)`.
///
/// Ask for parallel execution of FUNCTION; when it is refused, start the
/// ordinary thread `make-thread` would. Either way the result is a thread
/// object with the standard semantics.
#[doc(hidden)]
pub fn builtin_neomacs_make_parallel_thread(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    match admit_parallel(eval) {
        Ok(admission) => match admission {},
        Err(NMutatorEntryError::Unavailable) => builtin_make_thread(eval, args),
    }
}

/// `(neomacs-thread-parallel-p THREAD)`: t when THREAD was admitted to
/// parallel execution. An execution hint only; a thread's semantics do not
/// depend on it.
#[doc(hidden)]
pub fn builtin_neomacs_thread_parallel_p(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-thread-parallel-p", &args, 1)?;
    expect_thread_id(&eval.threads, &args[0])?;
    // `ParallelAdmission` is uninhabited, so no thread was admitted.
    Ok(Value::NIL)
}
