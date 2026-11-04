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

/// The two running values GNU's `main` installs after the declarations, and
/// the declaration value it replaces: `text_quoting_flag` from the process
/// locale (`src/emacs.c:1653-1656`) and `noninteractive1` from argv
/// (`src/emacs.c:1953`).  Any other row marked here would claim a value no
/// GNU `main` sets; a missing row would leave a library context at the
/// pre-main value.
#[test]
fn only_the_main_decided_rows_carry_a_running_value() {
    let mut main_decided: Vec<(&str, bool)> = GNU_BOOL_VARIABLES
        .iter()
        .filter_map(|v| v.main_value.map(|running| (v.name, running)))
        .collect();
    main_decided.sort_unstable();
    assert_eq!(
        main_decided,
        [
            ("internal--text-quoting-flag", true),
            ("noninteractive", true)
        ]
    );
    for var in GNU_BOOL_VARIABLES.iter().filter(|v| v.main_value.is_some()) {
        assert!(
            !var.initial,
            "{}: GNU's declaration leaves the slot false",
            var.name
        );
    }
}
