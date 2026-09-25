use super::super::*;
use neovm_host_abi::ChannelId;

/// Minimal in-crate TaskScheduler implementation proving the trait
/// contract is implementable and its surface behaves as documented
/// (neovm-worker holds the production implementation).
#[derive(Default)]
struct MockScheduler;

impl TaskScheduler for MockScheduler {
    fn spawn_task(&self, _form: LispValue, _opts: TaskOptions) -> Result<TaskHandle, Signal> {
        Ok(TaskHandle(42))
    }

    fn task_cancel(&self, handle: TaskHandle) -> bool {
        handle.0 == 42
    }

    fn task_status(&self, handle: TaskHandle) -> Option<TaskStatus> {
        if handle.0 == 42 {
            Some(TaskStatus::Completed)
        } else {
            None
        }
    }

    fn task_await(
        &self,
        handle: TaskHandle,
        _timeout: Option<Duration>,
    ) -> Result<LispValue, TaskError> {
        if handle.0 == 42 {
            Ok(LispValue {
                bytes: vec![1, 2, 3],
            })
        } else {
            Err(TaskError::TimedOut)
        }
    }

    fn select(&self, _ops: &[SelectOp], _timeout: Option<Duration>) -> SelectResult {
        SelectResult::Ready {
            op_index: 0,
            value: Some(LispValue { bytes: vec![9] }),
        }
    }
}

#[test]
fn task_scheduler_trait_contract() {
    crate::test_utils::init_test_tracing();
    let sched = MockScheduler;
    let handle = sched
        .spawn_task(LispValue::default(), TaskOptions::default())
        .expect("spawn should succeed");
    assert_eq!(handle, TaskHandle(42));
    assert_eq!(sched.task_status(handle), Some(TaskStatus::Completed));
    assert_eq!(sched.task_status(TaskHandle(7)), None);
    assert!(sched.task_cancel(handle));
    assert!(!sched.task_cancel(TaskHandle(7)));
    assert_eq!(
        sched
            .task_await(handle, Some(Duration::from_millis(10)))
            .expect("await should return result")
            .bytes,
        vec![1, 2, 3]
    );
    assert!(matches!(
        sched.task_await(TaskHandle(7), None).unwrap_err(),
        TaskError::TimedOut
    ));
    assert!(matches!(
        sched.select(&[SelectOp::Recv(ChannelId(1))], None),
        SelectResult::Ready { op_index: 0, .. }
    ));
}

#[test]
fn task_handle_eq_hash() {
    crate::test_utils::init_test_tracing();
    use std::collections::HashSet;
    let h1 = TaskHandle(1);
    let h2 = TaskHandle(1);
    let h3 = TaskHandle(2);
    assert_eq!(h1, h2);
    assert_ne!(h1, h3);
    let mut set = HashSet::new();
    set.insert(h1);
    assert!(set.contains(&h2));
    assert!(!set.contains(&h3));
}

#[test]
fn facade_reexports_name_the_engine_front_door() {
    crate::test_utils::init_test_tracing();
    // The curated lib.rs facade must keep working: Context, Value,
    // ValueKind, EvalError, Flow at the crate root.
    let mut ctx: crate::Context = crate::Context::new();
    let v: crate::Value = ctx
        .eval_str_each("(+ 1 2)")
        .pop()
        .expect("one form")
        .expect("eval succeeds");
    assert!(matches!(v.kind(), crate::ValueKind::Fixnum(3)));
}

#[test]
fn regex_fuzz_support_checks_each_differential_through_one_interface() {
    use crate::fuzz_support::{RegexCase, RegexCheck, RegexDifferential, check_regex_differential};
    use strum::IntoEnumIterator;

    let case = RegexCase::new(
        "prefix\\(alpha\\|beta\\)suffix",
        b"noise prefixbetasuffix tail",
        false,
        0,
        0,
    );

    for differential in RegexDifferential::iter() {
        assert!(
            matches!(
                check_regex_differential(case, differential),
                Ok(RegexCheck::Equivalent { comparisons }) if comparisons > 0
            ),
            "{differential} should compare at least one operation",
        );
    }
}

#[test]
fn regex_fuzz_support_checks_search_optimizations_without_a_prefilter() {
    use crate::fuzz_support::{RegexCase, RegexCheck, RegexDifferential, check_regex_differential};

    let case = RegexCase::new("a", b"zzza", false, 0, 0);
    assert!(matches!(
        check_regex_differential(case, RegexDifferential::SearchOptimizations),
        Ok(RegexCheck::Equivalent { comparisons: 2 })
    ));
}

/// The search-optimization differential compares a forward AND a backward
/// search, over multibyte and unibyte texts, case-folded or not.
#[test]
fn regex_fuzz_support_checks_search_optimizations_both_ways_in_both_representations() {
    use crate::fuzz_support::{
        RegexCase, RegexCheck, RegexDifferential, SearchTarget, check_regex_differential,
    };
    use strum::IntoEnumIterator;

    let text = b"x\xc9y\xe9 (DEFUN a) (defun b) z";
    for target in SearchTarget::iter() {
        for case_fold in [false, true] {
            for pattern in ["(defun \\([a-z]+\\)", "é", "[É]", "y"] {
                for start in [0, 5, text.len()] {
                    let case =
                        RegexCase::new(pattern, text, case_fold, start, start).with_target(target);
                    assert_eq!(
                        check_regex_differential(case, RegexDifferential::SearchOptimizations),
                        Ok(RegexCheck::Equivalent { comparisons: 2 }),
                        "{pattern:?} fold={case_fold} target={target} start={start}"
                    );
                }
            }
        }
    }
}

/// A multibyte search target is valid internal text, as every Lisp string and
/// buffer is.  Raw bytes that are not (an overlong `E0 81 81`, which the
/// matcher decodes to `A`) become raw-byte characters first, as they would in
/// a Lisp string: GNU's byte-indexed fastmap never sees an overlong form.
#[test]
fn regex_fuzz_support_searches_multibyte_targets_as_valid_internal_text() {
    use crate::fuzz_support::{RegexCase, RegexCheck, RegexDifferential, check_regex_differential};

    for case_fold in [false, true] {
        let case = RegexCase::new("A", &[0xE0, 0x81, 0x81, b'x'], case_fold, 0, 0);
        assert_eq!(
            check_regex_differential(case, RegexDifferential::SearchOptimizations),
            Ok(RegexCheck::Equivalent { comparisons: 2 }),
            "fold={case_fold}"
        );
    }
}
