//! The opt-in parallel creator (pentry DESIGN section 6, E6-P.1).
//!
//! `neomacs-make-parallel-thread` is the only route that asks for N-mutator
//! admission; `make-thread` never asks. A refused request is not an error:
//! the creator starts an ordinary Baton thread, with the same object, the
//! same thread, mutex and condition APIs, and `neomacs-thread-parallel-p`
//! nil. The creator preflight reads the installed heap: GEN0 is
//! [`NMutatorEntryError::GenerationalDisabled`], and an eligible heap stays
//! [`NMutatorEntryError::Unavailable`] until E6-P.2. `Gen1Admission::check`
//! stays at era construction.

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
    /// The installed heap is GEN0. DESIGN's name for
    /// [`neovm_value::tagged::gc::Gen1AdmissionError::GenerationalRequired`].
    GenerationalDisabled,
}

/// The admission the opt-in creator asks for.
///
/// Read-only. The subr runs with this thread's heap installed, and
/// `HeapTransfer::with_collector` refuses that state, so this does not take
/// a collector loan and does not call `ParallelHeapEra::admit_rooted`.
fn admit_parallel(eval: &Context) -> Result<ParallelAdmission, NMutatorEntryError> {
    // The subr runs with this heap installed. A caller that has not installed
    // it yet must not observe a different thread-local heap.
    let eligible = if neovm_value::tagged::gc::tagged_heap_is_current(&eval.tagged_heap) {
        neovm_value::tagged::gc::with_tagged_heap(|heap| heap.tagged_heap_generational_enabled())
    } else {
        eval.tagged_heap.tagged_heap_generational_enabled()
    };
    if eligible {
        Err(NMutatorEntryError::Unavailable)
    } else {
        Err(NMutatorEntryError::GenerationalDisabled)
    }
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
        Err(NMutatorEntryError::Unavailable | NMutatorEntryError::GenerationalDisabled) => {
            builtin_make_thread(eval, args)
        }
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

#[cfg(test)]
mod tests {
    use super::{NMutatorEntryError, admit_parallel};
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::print::print_value;

    fn context_in(generational: bool) -> Context {
        let mut context = Context::new(crate::test_utils::unit_runtime());
        // SAFETY: serial fixture setup. No young objects or compiled leaves
        // exist under the mode this call installs.
        unsafe {
            neovm_value::test_support::authority::set_generational_enabled_for_test(
                &mut context.tagged_heap,
                generational,
            );
        }
        context.setup_thread_locals();
        assert!(neovm_value::tagged::gc::tagged_heap_is_current(
            &context.tagged_heap
        ));
        context
    }

    /// An `Err` is the only creator outcome. `ParallelAdmission` is
    /// uninhabited, so the creator cannot call `era.register`.
    fn assert_no_permit(reason: &Result<super::ParallelAdmission, NMutatorEntryError>) {
        assert!(reason.is_err());
    }

    fn hint_nil(context: &mut Context) {
        let value = context
            .eval_str(
                "(let ((th (neomacs-make-parallel-thread (lambda () nil))))
                   (thread-join th)
                   (neomacs-thread-parallel-p th))",
            )
            .unwrap();
        assert_eq!(print_value(&value), "nil");
    }

    #[test]
    fn gen0_opt_in_is_baton_with_hint_nil_because_generational_is_disabled() {
        let mut context = context_in(false);
        let reason = admit_parallel(&context);
        assert!(matches!(
            reason,
            Err(NMutatorEntryError::GenerationalDisabled)
        ));
        assert_no_permit(&reason);
        hint_nil(&mut context);
    }

    #[test]
    fn gen1_opt_in_is_still_baton_with_hint_nil_until_a_route_exists() {
        let mut context = context_in(true);
        let reason = admit_parallel(&context);
        assert!(matches!(reason, Err(NMutatorEntryError::Unavailable)));
        assert_no_permit(&reason);
        hint_nil(&mut context);
    }
}
