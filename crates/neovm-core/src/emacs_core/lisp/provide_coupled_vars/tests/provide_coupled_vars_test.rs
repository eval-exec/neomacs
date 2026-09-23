use super::*;

/// The shape of the table, checked without booting a runtime.
/// The `dbusbind.c` rows, present only in a build without libdbus: with
/// it the feature exists and those nine names are real variables.
const DBUS_ROWS: usize = if cfg!(neomacs_have_dbus) { 0 } else { 9 };

#[test]
fn every_row_names_a_feature_and_cites_gnu() {
    assert_eq!(PROVIDE_COUPLED_VARIABLES.len(), 225 + DBUS_ROWS);
    for var in PROVIDE_COUPLED_VARIABLES {
        assert!(!var.name.is_empty());
        assert!(
            !var.features.is_empty(),
            "{} has no coupled feature, so nothing decides it",
            var.name
        );
        assert!(
            var.gnu.contains(".c:") || var.gnu.contains(".m:"),
            "{} does not cite a GNU DEFVAR site: {:?}",
            var.name,
            var.gnu
        );
        if let HereDecision::BoundByPolicy { policy } = var.here {
            assert!(
                policy.starts_with("ledger "),
                "{} is bound by a policy that names no entry: {:?}",
                var.name,
                policy
            );
        }
    }
}

#[test]
fn the_table_has_no_duplicate_rows() {
    let mut names: Vec<&str> = PROVIDE_COUPLED_VARIABLES.iter().map(|v| v.name).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(names.len(), before, "duplicate provide-coupled row");
}

/// The counts the module doc states, so the prose cannot drift from the
/// table.
#[test]
fn seventy_four_of_the_rows_are_bound_by_a_named_policy() {
    let bound = PROVIDE_COUPLED_VARIABLES
        .iter()
        .filter(|v| matches!(v.here, HereDecision::BoundByPolicy { .. }))
        .count();
    assert_eq!(bound, 74);
    assert_eq!(PROVIDE_COUPLED_VARIABLES.len() - bound, 151 + DBUS_ROWS);
}

/// The two names ledger 199 removed, and the two it deliberately did not.
///
/// `use-system-tooltips` and `scroll-bar-adjust-thumb-portion` are
/// `DEFVAR_BOOL` in `src/frame.c` outside every `#ifdef`, so no coupling
/// reaches them and they must have no row at all.  A row appearing for
/// either one would mean the derivation had started deleting GNU's
/// platform-neutral surface.
#[test]
fn the_platform_neutral_names_have_no_row() {
    assert!(lookup("use-system-tooltips").is_none());
    assert!(lookup("scroll-bar-adjust-thumb-portion").is_none());
    assert_eq!(
        lookup("gtk-version-string").map(|v| v.here),
        Some(HereDecision::Absent)
    );
    assert_eq!(
        lookup("cairo-version-string").map(|v| v.here),
        Some(HereDecision::Absent)
    );
}
