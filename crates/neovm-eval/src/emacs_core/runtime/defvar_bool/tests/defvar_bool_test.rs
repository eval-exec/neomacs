use super::*;
use crate::platform_surface::PlatformSurface;

fn declared(surface: PlatformSurface) -> usize {
    GNU_BOOL_VARIABLES
        .iter()
        .filter(|v| v.surfaces.contains(surface))
        .count()
}

fn listed(surface: PlatformSurface) -> usize {
    GNU_BOOL_VARIABLES
        .iter()
        .filter(|v| v.surfaces.contains(surface))
        .filter(|v| v.byte_boolean_vars == ByteBooleanVars::Listed)
        .count()
}

/// GNU 31.1 replaces the frame-deletion boolean with a DEFVAR_LISP,
/// removing one entry from the 31.0.90 baseline of 148 / 117.
///
/// The table is the union over the reference surfaces; the counts are per
/// surface. X11 keeps 147 declared / 116 byte-boolean-vars; NS drops the 15
/// X11-only rows and adds the 7 NS-only rows: 139 / 108.
#[test]
fn table_matches_gnu_counts_per_reference_surface() {
    assert_eq!(declared(PlatformSurface::X11), 147);
    assert_eq!(listed(PlatformSurface::X11), 116);
    assert_eq!(declared(PlatformSurface::Ns), 139);
    assert_eq!(listed(PlatformSurface::Ns), 108);
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
