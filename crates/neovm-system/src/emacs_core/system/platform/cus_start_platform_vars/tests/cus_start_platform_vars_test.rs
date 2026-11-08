use super::*;

/// Measured under GNU Emacs 31.0.90 on GNU/Linux, `-Q --batch`: of the 32
/// platform names Neomacs used to seed, GNU binds 7 and leaves 25 unbound.
/// The NS reference build is the same table read through `binding_on(true)`:
/// all 15 `ns-*` rows plus the four cross-surface names are GNU-bound there
/// (19), and four of the `ns-*` rows are recorded as GNU-declared but
/// Neo-missing (`GnuDeclaresNotNeo`), so this port's own NS dump binds 15.
#[test]
fn table_matches_gnu_measurement() {
    assert_eq!(CUS_START_PLATFORM_VARIABLES.len(), 32);
    let ns_surface = cfg!(target_os = "macos");
    let mut gnu_bound = 0;
    let mut neo_bound = 0;
    let mut gnu_only = 0;
    for var in CUS_START_PLATFORM_VARIABLES {
        match var.binding_on(ns_surface) {
            GnuBinding::UnboundHere => {}
            GnuBinding::GnuDeclaresNotNeo => {
                gnu_bound += 1;
                gnu_only += 1;
            }
            GnuBinding::DeclaredInC { .. } | GnuBinding::DeclaredInPreloadedLisp { .. } => {
                gnu_bound += 1;
                neo_bound += 1;
            }
        }
    }
    if ns_surface {
        assert_eq!((gnu_bound, neo_bound, gnu_only), (19, 15, 4));
    } else {
        assert_eq!((gnu_bound, neo_bound, gnu_only), (7, 7, 0));
    }
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
            GnuBinding::UnboundHere | GnuBinding::GnuDeclaresNotNeo => {}
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
