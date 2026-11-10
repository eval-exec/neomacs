//! Chartable concurrent-write shells.
//! The shell supplies the text crate's char-table range builtins; the bodies
//! and their assertions stay in value. One #[test] per former value row.
use neovm_value::error::EvalResult;
use neovm_value::tagged::value::TaggedValue;
use neovm_value::test_support::authority::{
    ChartableConcurrentWriteCase, run_chartable_concurrent_write_test_for_test,
};

fn range(args: Vec<TaggedValue>) -> EvalResult {
    crate::emacs_core::chartable::builtin_char_table_range(args, None)
}

fn set_range(args: Vec<TaggedValue>) -> EvalResult {
    crate::emacs_core::chartable::builtin_set_char_table_range(args, None)
}

fn run(case: ChartableConcurrentWriteCase) {
    run_chartable_concurrent_write_test_for_test(case, range, set_range);
}

macro_rules! shells {
    ($($name:ident => $case:ident),* $(,)?) => {
        $(#[test]
        fn $name() {
            run(ChartableConcurrentWriteCase::$case);
        })*
        const SHELL_CASES: &[ChartableConcurrentWriteCase] =
            &[$(ChartableConcurrentWriteCase::$case),*];
    };
}

shells! {
    concurrent_mapped_chartable_scan_and_builtin_range_writes_are_atomic =>
        ConcurrentMappedChartableScanAndBuiltinRangeWritesAreAtomic,
    concurrent_subchartable_snapshot_retains_mapped_span_after_builtin_cow =>
        ConcurrentSubchartableSnapshotRetainsMappedSpanAfterBuiltinCow,
}

#[test]
fn shell_cases_cover_all_chartable_concurrent_write_cases() {
    let mut listed = SHELL_CASES.to_vec();
    listed.sort();
    let mut all = ChartableConcurrentWriteCase::ALL.to_vec();
    all.sort();
    assert_eq!(listed, all, "one core shell per former value row");
}
