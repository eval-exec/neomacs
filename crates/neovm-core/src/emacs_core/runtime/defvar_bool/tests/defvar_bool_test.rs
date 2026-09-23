use super::*;

/// GNU 31.1 replaces the frame-deletion boolean with a DEFVAR_LISP,
/// removing one entry from the 31.0.90 baseline of 148 / 117.
#[test]
fn table_matches_gnu_counts() {
    assert_eq!(GNU_BOOL_VARIABLES.len(), 147);
    assert_eq!(
        GNU_BOOL_VARIABLES
            .iter()
            .filter(|v| v.byte_boolean_vars == ByteBooleanVars::Listed)
            .count(),
        116
    );
}

#[test]
fn table_has_no_duplicate_rows() {
    let mut names: Vec<&str> = GNU_BOOL_VARIABLES.iter().map(|v| v.name).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(names.len(), before, "duplicate DEFVAR_BOOL row");
}
