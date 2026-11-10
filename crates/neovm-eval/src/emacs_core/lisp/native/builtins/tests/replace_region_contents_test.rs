//! Myers-diff unit tests for `replace-region-contents`.
//!
//! The Elisp rows that load GNU `subr.el` live in core: the minimal
//! backquote runtime is minted there. These rows call the public edit
//! diff directly. Assertions are unchanged.

use neovm_edit::emacs_core::buffer::{ReplaceRegionChangeRun, replace_region_minimal_change_runs};

/// Convenience to turn an `&str` into a codepoint vector for the diff tests.
fn codes(s: &str) -> Vec<u32> {
    s.chars().map(|c| c as u32).collect()
}

fn run_bounds(runs: &[ReplaceRegionChangeRun]) -> Vec<(usize, usize, usize, usize)> {
    runs.iter()
        .map(|run| (run.a_start(), run.a_end(), run.b_start(), run.b_end()))
        .collect()
}

#[test]
fn minimal_change_runs_pure_insertion() {
    // "abcdef" -> "abcXYZdef": insert "XYZ" between index 3 and 3.
    assert_eq!(
        run_bounds(
            &replace_region_minimal_change_runs(&codes("abcdef"), &codes("abcXYZdef"), None)
                .unwrap()
        ),
        vec![(3, 3, 3, 6)]
    );
}

#[test]
fn minimal_change_runs_identical_is_empty() {
    assert_eq!(
        replace_region_minimal_change_runs(&codes("abcdef"), &codes("abcdef"), None).unwrap(),
        Vec::<ReplaceRegionChangeRun>::new()
    );
}

#[test]
fn minimal_change_runs_two_insertions_keep_interior_match() {
    // "abc" -> "aXbYc": insert X before b, insert Y before c. The "b" between
    // them must remain a matched (untouched) character, i.e. two separate runs.
    assert_eq!(
        run_bounds(
            &replace_region_minimal_change_runs(&codes("abc"), &codes("aXbYc"), None).unwrap()
        ),
        vec![(1, 1, 1, 2), (2, 2, 3, 4)]
    );
}

#[test]
fn minimal_change_runs_two_deletions_keep_interior_match() {
    // "abcde" -> "ace": delete b and d, keeping c between them.
    assert_eq!(
        run_bounds(
            &replace_region_minimal_change_runs(&codes("abcde"), &codes("ace"), None).unwrap()
        ),
        vec![(1, 2, 1, 1), (3, 4, 2, 2)]
    );
}

#[test]
fn minimal_change_runs_substitution() {
    // "abZZef" region "ZZ" (indices 2..4) replaced by "CD": a single run.
    assert_eq!(
        run_bounds(&replace_region_minimal_change_runs(&codes("ZZ"), &codes("CD"), None).unwrap()),
        vec![(0, 2, 0, 2)]
    );
}

#[test]
fn minimal_change_runs_stops_when_positive_deadline_expires() {
    // A large edit distance used to copy the entire Myers frontier thousands
    // of times even when dsh requested only 10ms of comparison work.
    let a = vec![u32::from('a'); 10_000];
    let b = vec![u32::from('b'); 10_000];
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1);
    assert!(replace_region_minimal_change_runs(&a, &b, Some(deadline)).is_none());
}
