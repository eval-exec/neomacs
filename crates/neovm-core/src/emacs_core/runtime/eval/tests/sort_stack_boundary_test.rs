//! A Rust stack probe is not a Lisp/GC publication boundary.
//! GNU sort.c:198-214 compares key scalars through ordinary funcall;
//! eval.c:3194-3216 retains that frame and depth until the body returns.
#![deny(clippy::wildcard_enum_match_arm)]
use super::{
    Context, EVAL_STACK_RED_ZONE, STACK_GROWTH_PROBE_INTERVAL, STACK_GROWTH_PROBE_START_DEPTH,
};
use crate::emacs_core::builtins::higher_order::{SortPredicate, SortRuntime, builtin_sort_slice};
use crate::emacs_core::value::Value;
use crate::tagged::collection_reads::capture;
use crate::tagged::mutate::LispCollectionRevision;
use strum::{EnumIter, IntoEnumIterator, IntoStaticStr};

/// Immutable depth observation, never a specpdl count or stack address.
/// The borrowed guard restores it on its owning mutator thread.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct LispEvalDepth(usize);

impl LispEvalDepth {
    fn capture(context: &Context) -> Self {
        Self(context.depth)
    }

    fn restore(self, context: &mut Context) {
        context.depth = self.0;
    }
}

/// Immutable specpdl count; observations do not repair native frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct SpecpdlFrameCount(usize);

impl SpecpdlFrameCount {
    fn capture(context: &Context) -> Self {
        Self(context.specpdl.len())
    }
}

/// Immutable JIT bounds observation; the captured address is never dereferenced.
/// Untracked bounds preserve Context's valid zero state explicitly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JitStackLimitSnapshot {
    Untracked,
    Tracked(std::num::NonZeroUsize),
}

impl JitStackLimitSnapshot {
    fn capture(context: &Context) -> Self {
        match std::num::NonZeroUsize::new(context.jit_stack_limit) {
            Some(address) => Self::Tracked(address),
            None => Self::Untracked,
        }
    }
}

static_assertions::assert_type_ne_all!(LispEvalDepth, SpecpdlFrameCount, JitStackLimitSnapshot);
const _: () = {
    assert!(std::mem::size_of::<LispEvalDepth>() == std::mem::size_of::<usize>());
    assert!(std::mem::align_of::<LispEvalDepth>() == std::mem::align_of::<usize>());
    assert!(std::mem::size_of::<SpecpdlFrameCount>() == std::mem::size_of::<usize>());
    assert!(std::mem::align_of::<SpecpdlFrameCount>() == std::mem::align_of::<usize>());
    assert!(std::mem::size_of::<JitStackLimitSnapshot>() == std::mem::size_of::<usize>());
    assert!(std::mem::align_of::<JitStackLimitSnapshot>() == std::mem::align_of::<usize>());
    assert!(STACK_GROWTH_PROBE_START_DEPTH > 0);
};

#[derive(Clone, Copy, Debug, EnumIter, IntoStaticStr)]
enum NativePredicate {
    #[strum(serialize = "<")]
    NumericLessp,
    #[strum(serialize = "string<")]
    StringLessp,
}

impl NativePredicate {
    fn callable(self) -> Value {
        let name: &'static str = self.into();
        Value::symbol(name)
    }

    fn element(self, ordinal: i64) -> Value {
        match self {
            Self::NumericLessp => Value::fixnum(ordinal),
            Self::StringLessp => Value::string(format!("{ordinal:04}")),
        }
    }

    fn assert_element(self, actual: Value, ordinal: usize) {
        match self {
            Self::NumericLessp => assert_eq!(actual, Value::fixnum(ordinal as i64)),
            Self::StringLessp => {
                assert_eq!(actual.as_utf8_str(), Some(format!("{ordinal:04}").as_str()));
            }
        }
    }
}

#[derive(Clone, Copy, Debug, EnumIter)]
enum SampledEntryDepth {
    FirstProbe,
    LaterProbe,
}

impl SampledEntryDepth {
    fn caller_depth(self) -> LispEvalDepth {
        LispEvalDepth(match self {
            Self::FirstProbe => STACK_GROWTH_PROBE_START_DEPTH - 1,
            Self::LaterProbe => STACK_GROWTH_PROBE_START_DEPTH + STACK_GROWTH_PROBE_INTERVAL - 1,
        })
    }
}

/// Borrow the test's Context until its injected caller depth has been restored.
/// Drop restores only that injected state and cannot panic, including when an
/// assertion fails. Native-call frame and stack-limit restoration are checked
/// before dropping the guard rather than repaired by the fixture.
#[must_use = "the guard restores the injected caller depth on drop"]
struct SampledCallerDepth<'ctx> {
    context: &'ctx mut Context,
    original_depth: LispEvalDepth,
    sampled_caller_depth: LispEvalDepth,
    original_frames: SpecpdlFrameCount,
    original_stack_limit: JitStackLimitSnapshot,
    _owner: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl std::fmt::Debug for SampledCallerDepth<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SampledCallerDepth")
            .field("original_depth", &self.original_depth)
            .field("sampled_caller_depth", &self.sampled_caller_depth)
            .field("original_frames", &self.original_frames)
            .field("original_stack_limit", &self.original_stack_limit)
            .finish_non_exhaustive()
    }
}

impl<'ctx> SampledCallerDepth<'ctx> {
    fn enter(context: &'ctx mut Context, sampled: SampledEntryDepth) -> Self {
        let original_depth = LispEvalDepth::capture(context);
        let sampled_caller_depth = sampled.caller_depth();
        let original_frames = SpecpdlFrameCount::capture(context);
        let original_stack_limit = JitStackLimitSnapshot::capture(context);
        sampled_caller_depth.restore(context);
        Self {
            context,
            original_depth,
            sampled_caller_depth,
            original_frames,
            original_stack_limit,
            _owner: std::marker::PhantomData,
        }
    }

    fn context(&mut self) -> &mut Context {
        self.context
    }

    fn assert_native_call_restored(&self) {
        assert_eq!(
            LispEvalDepth::capture(self.context),
            self.sampled_caller_depth
        );
        assert_eq!(
            SpecpdlFrameCount::capture(self.context),
            self.original_frames
        );
        assert_eq!(
            JitStackLimitSnapshot::capture(self.context),
            self.original_stack_limit
        );
    }
}

impl Drop for SampledCallerDepth<'_> {
    fn drop(&mut self) {
        self.original_depth.restore(self.context);
    }
}

#[derive(Clone, Copy, Debug, EnumIter)]
enum NativeStackPlacement {
    CallerSegment,
    SmallSegment,
}

impl NativeStackPlacement {
    fn run<T>(self, action: impl FnOnce() -> T) -> T {
        match self {
            Self::CallerSegment => action(),
            Self::SmallSegment => stacker::grow(64 * 1024, || {
                let remaining = stacker::remaining_stack().expect("known test stack bounds");
                assert!(
                    remaining < EVAL_STACK_RED_ZONE,
                    "small segment must force the ordinary stack-growth path: {remaining}"
                );
                action()
            }),
        }
    }
}

#[test]
fn native_sort_batches_at_sampled_depths_on_both_stack_placements() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::runtime_startup_context();
    // Bootstrap may leave an incremental mark active. This fixture isolates
    // batching across Rust stack probes, whose native admission cannot collect;
    // the publication/pressure fixture below exercises the GC fallback.
    let mut gc_scope = super::super::GcInhibitGuard::enter(&mut context);
    let eval = gc_scope.context();
    for placement in NativeStackPlacement::iter() {
        for sampled in SampledEntryDepth::iter() {
            for predicate in NativePredicate::iter() {
                // Allocate fixtures on the caller's segment, leaving the small
                // segment for the sort and its verified native comparisons.
                let vector = Value::vector(
                    (0..50)
                        .map(|index| predicate.element((index * 17 + 13) % 50))
                        .collect(),
                );
                let original_depth = LispEvalDepth::capture(eval);
                let revisions = {
                    let mut caller_depth = SampledCallerDepth::enter(eval, sampled);
                    let (revisions, _) = capture(|| {
                        let _ = vector.as_vector_data().unwrap();
                        placement.run(|| {
                            let before = LispCollectionRevision::current();
                            let returned = builtin_sort_slice(
                                caller_depth.context(),
                                &[vector, predicate.callable()],
                            );
                            assert_eq!(returned.unwrap(), vector);
                            LispCollectionRevision::current().steps_since_for_test(before)
                        })
                    });
                    caller_depth.assert_native_call_restored();
                    revisions
                };
                assert_eq!(LispEvalDepth::capture(eval), original_depth);
                assert!(
                    revisions <= 2,
                    "{predicate:?} at {sampled:?} on {placement:?} journalled {revisions} moves"
                );
                for (ordinal, value) in vector.as_vector_data().unwrap().iter().enumerate() {
                    predicate.assert_element(*value, ordinal);
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, EnumIter)]
enum NativeCallOutcome {
    Success,
    Signal,
}

#[derive(Clone, Copy, Debug, EnumIter)]
enum CollectionPressure {
    Idle,
    Stress,
}

impl CollectionPressure {
    fn apply(self, context: &mut Context) {
        context.gc_stress = match self {
            Self::Idle => false,
            Self::Stress => true,
        };
    }
}

#[test]
fn published_native_sort_calls_restore_frames_on_success_and_signal() {
    crate::test_utils::init_test_tracing();
    for placement in NativeStackPlacement::iter() {
        for sampled in SampledEntryDepth::iter() {
            for predicate in NativePredicate::iter() {
                for pressure in CollectionPressure::iter() {
                    for outcome in NativeCallOutcome::iter() {
                        let mut eval = crate::test_utils::runtime_startup_context();
                        let captured = eval.resolve_sort_predicate(predicate.callable());
                        let left = match outcome {
                            NativeCallOutcome::Success => predicate.element(0),
                            NativeCallOutcome::Signal => Value::list(vec![Value::symbol("bad")]),
                        };
                        let right = predicate.element(1);
                        let list = Value::list(vec![right, left]);
                        // Published callers own these roots for their complete
                        // sort, including a signal-hook collection at finish.
                        // This fresh Context owns them until it is dropped.
                        let subr = match captured {
                            SortPredicate::NumericLessp { subr, .. }
                            | SortPredicate::StringLessp { subr, .. } => subr,
                            SortPredicate::ValueLt
                            | SortPredicate::Generic(_)
                            | SortPredicate::Subr { .. } => {
                                panic!("fixture must capture a native body")
                            }
                        };
                        eval.push_specpdl_root(subr);
                        eval.push_specpdl_root(left);
                        eval.push_specpdl_root(right);
                        eval.push_specpdl_root(list);
                        pressure.apply(&mut eval);
                        let mut caller = SampledCallerDepth::enter(&mut eval, sampled);
                        let result = placement.run(|| {
                            builtin_sort_slice(caller.context(), &[list, predicate.callable()])
                        });
                        match outcome {
                            NativeCallOutcome::Success => {
                                assert_eq!(result.unwrap(), list);
                                assert_eq!(list.cons_car(), left);
                                assert_eq!(list.cons_cdr().cons_car(), right);
                            }
                            NativeCallOutcome::Signal => assert!(result.is_err()),
                        }
                        caller.assert_native_call_restored();
                    }
                }
            }
        }
    }
}
