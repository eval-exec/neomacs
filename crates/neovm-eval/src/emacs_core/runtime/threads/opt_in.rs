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
use crate::emacs_core::eval::parallel_entry::live_admission::SpawnParallelError;
use neovm_value::error::EvalResult;
use neovm_value::value::Value;

/// Single-point production admission switch (`#14342` Q3, `E6-P4-ADMISSION-PREREAD.md` §4
/// Option A): Step 1 (E6-P.4) lands the full production route dormant (`false`)
/// with zero temporary env doors; Step 3 (ADMISSION-ON) flips this single switch
/// to `true` after `b6` passes on the attested GNU Emacs 31.1 oracle.
const PRODUCTION_ADMISSION_ENABLED: bool = true;

/// Test-only override of the single-point switch: `DEFAULT` follows the
/// compiled constant, `OFF` and `ON` force one configuration. Both directions
/// exist so a row can assert the admitted arm and the Baton-fallback arm of the
/// same primitive; with production admission on, the fallback arm is only
/// reachable through `OFF`. `test-support` builds expose both cross-crate so
/// re-homed tests (neovm-core) can cover the same pair.
#[cfg(any(test, feature = "test-support"))]
static TEST_ADMISSION_OVERRIDE: std::sync::atomic::AtomicU8 =
    std::sync::atomic::AtomicU8::new(ADMISSION_OVERRIDE_DEFAULT);
#[cfg(any(test, feature = "test-support"))]
const ADMISSION_OVERRIDE_DEFAULT: u8 = 0;
#[cfg(any(test, feature = "test-support"))]
const ADMISSION_OVERRIDE_OFF: u8 = 1;
#[cfg(any(test, feature = "test-support"))]
const ADMISSION_OVERRIDE_ON: u8 = 2;

#[cfg(any(test, feature = "test-support"))]
fn with_admission_override<R>(mode: u8, body: impl FnOnce() -> R) -> R {
    struct Reset(u8);
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_ADMISSION_OVERRIDE.store(self.0, std::sync::atomic::Ordering::Release);
        }
    }
    let prev = TEST_ADMISSION_OVERRIDE.swap(mode, std::sync::atomic::Ordering::AcqRel);
    let _reset = Reset(prev);
    body()
}

/// Run `body` with the switch forced ON, whatever the compiled constant says:
/// the rows that exercise the live production route use this so they keep
/// meaning if the constant is ever flipped back.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn with_parallel_admission_enabled_for_test<R>(body: impl FnOnce() -> R) -> R {
    with_admission_override(ADMISSION_OVERRIDE_ON, body)
}

/// Run `body` with the switch forced OFF: the shape a refused opt-in request
/// takes (an ordinary Baton thread with the hint nil). With production
/// admission on, this is the only way to keep that fallback covered.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn with_parallel_admission_disabled_for_test<R>(body: impl FnOnce() -> R) -> R {
    with_admission_override(ADMISSION_OVERRIDE_OFF, body)
}

#[inline]
fn parallel_admission_enabled() -> bool {
    #[cfg(any(test, feature = "test-support"))]
    match TEST_ADMISSION_OVERRIDE.load(std::sync::atomic::Ordering::Acquire) {
        ADMISSION_OVERRIDE_ON => return true,
        ADMISSION_OVERRIDE_OFF => return false,
        _ => {}
    }
    PRODUCTION_ADMISSION_ENABLED
}

/// A granted N-mutator admission (`E6-P.4`).
pub(crate) struct ParallelAdmission(());

/// Why an opt-in request runs Baton-serial instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NMutatorEntryError {
    /// Production admission is dormant, or the current runtime/context is not
    /// eligible for live-Green N-mutator admission (e.g. Sim mode).
    Unavailable,
    /// The installed heap is GEN0. DESIGN's name for
    /// [`neovm_value::tagged::gc::Gen1AdmissionError::GenerationalRequired`].
    GenerationalDisabled,
}

/// The admission the opt-in creator asks for.
///
/// Read-only preflight before opening or enrolling in a live parallel era.
fn admit_parallel(eval: &Context) -> Result<ParallelAdmission, NMutatorEntryError> {
    // The subr runs with this heap installed. A caller that has not installed
    // it yet must not observe a different thread-local heap.
    let eligible = if neovm_value::tagged::gc::tagged_heap_is_current(&eval.tagged_heap) {
        // SAFETY: reads the generational-GC configuration flag only; never
        // collects, re-enters `with_tagged_heap`, mints capability tokens, or
        // mutates any Lisp payload.
        unsafe {
            neovm_value::tagged::gc::with_tagged_heap(|heap| {
                heap.tagged_heap_generational_enabled()
            })
        }
    } else {
        eval.tagged_heap.tagged_heap_generational_enabled()
    };
    if !eligible {
        return Err(NMutatorEntryError::GenerationalDisabled);
    }
    if !parallel_admission_enabled() || !crate::emacs_core::eval::green_world(eval) {
        return Err(NMutatorEntryError::Unavailable);
    }
    Ok(ParallelAdmission(()))
}

/// `(neomacs-make-parallel-thread FUNCTION &optional NAME BUFFER-DISPOSITION)`.
///
/// Ask for parallel execution of FUNCTION; when it is refused, start the
/// ordinary thread `make-thread` would. Either way the result is a thread
/// object with the standard semantics.
#[doc(hidden)]
pub fn builtin_neomacs_make_parallel_thread(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    match admit_parallel(eval) {
        Ok(_admission) => match eval.spawn_parallel_opt_in_thread(&args) {
            Ok(handle) => Ok(handle),
            Err(SpawnParallelError::Lisp(flow)) => Err(flow),
            Err(SpawnParallelError::Refused(reason)) => {
                tracing::warn!(
                    ?reason,
                    "neomacs-make-parallel-thread spawn refused; falling back to make-thread"
                );
                builtin_make_thread(eval, args)
            }
        },
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
    let id = expect_thread_id(&eval.threads, &args[0])?;
    Ok(Value::bool_val(eval.is_thread_parallel_admitted(id)))
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
