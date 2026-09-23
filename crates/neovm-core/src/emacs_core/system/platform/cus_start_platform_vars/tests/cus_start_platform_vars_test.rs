use super::*;

/// Measured under GNU Emacs 31.0.90 on GNU/Linux, `-Q --batch`: of the 32
/// platform names Neomacs used to seed, GNU binds 7 and leaves 25 unbound.
#[test]
fn table_matches_gnu_measurement() {
    assert_eq!(CUS_START_PLATFORM_VARIABLES.len(), 32);
    assert_eq!(
        CUS_START_PLATFORM_VARIABLES
            .iter()
            .filter(|v| !matches!(v.binding, GnuBinding::UnboundHere))
            .count(),
        7
    );
}

#[test]
fn table_has_no_duplicate_rows() {
    let mut names: Vec<&str> = CUS_START_PLATFORM_VARIABLES
        .iter()
        .map(|v| v.name)
        .collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(names.len(), before, "duplicate cus-start platform row");
}

/// Every row records where GNU declares it, and every bound row records
/// where Neomacs declares it.  Neither is optional, because the two
/// questions this table exists to answer are exactly those.
#[test]
fn every_row_cites_gnu_and_every_bound_row_cites_its_site() {
    for var in CUS_START_PLATFORM_VARIABLES {
        assert!(!var.gnu.is_empty(), "{} has no GNU citation", var.name);
        match var.binding {
            GnuBinding::DeclaredInC { site } | GnuBinding::DeclaredInPreloadedLisp { site } => {
                assert!(!site.is_empty(), "{} has no declaration site", var.name);
            }
            GnuBinding::UnboundHere => {}
        }
    }
}

#[test]
fn unbound_names_are_the_ones_without_documentation() {
    assert!(is_name_gnu_leaves_unbound_here("dos-hyper-key"));
    assert!(is_name_gnu_leaves_unbound_here("imagemagick-render-type"));
    assert!(!is_name_gnu_leaves_unbound_here("x-bitmap-file-path"));
    assert!(!is_name_gnu_leaves_unbound_here("window-combination-limit"));
    // Not a row at all: Neomacs really ships an xwidget layer.
    assert!(!is_name_gnu_leaves_unbound_here(
        "xwidget-webkit-disable-javascript"
    ));
}
