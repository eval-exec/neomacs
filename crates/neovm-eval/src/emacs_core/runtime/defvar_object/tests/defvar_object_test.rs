use super::*;

/// Measured against GNU Emacs 31.1's `src/*.c`, 2026-09-08:
/// 563 `DEFVAR_LISP`/`DEFVAR_LISP_NOPRO` names and 14 `DEFVAR_KBOARD`
/// ones.  The generator keeps the first declaration of a name it sees, so
/// a variable several window-system files declare counts once.
///
/// GNU 31.1 adds `delete-frame-choose-selected` as a Lisp object in place
/// of its old boolean.  Before that it was 564 + 14 until ledger 183:
/// the extractor scanned raw C text and
/// therefore saw seven `DEFVAR` heads parked inside `#if 0`, of which five
/// (`x-pointer-shape` and four cursor names in `w32fns.c`) are also
/// declared in `xfns.c` and belong here anyway.  Two are not declared
/// anywhere else and had no business being rows -- `echo-area-clear-hook`
/// (`src/keyboard.c:14059`) and `w32-generate-fake-inodes`
/// (`src/w32proc.c:4831`) -- and the first of them cost two Lisp-visible
/// facts, because this port DOES have the variable.
#[test]
fn table_matches_gnu_counts() {
    let table = gnu_table::GNU_OBJECT_VARIABLES;
    assert_eq!(table.len(), 577);
    assert_eq!(
        table
            .iter()
            .filter(|v| v.kind == GnuObjectForward::Keyboard)
            .count(),
        14
    );
}

#[test]
fn table_has_no_duplicate_rows() {
    let mut names: Vec<&str> = gnu_table::GNU_OBJECT_VARIABLES
        .iter()
        .map(|v| v.name)
        .collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(names.len(), before, "duplicate DEFVAR_LISP/KBOARD row");
}

/// `DEFVAR_BOOL` and `DEFVAR_INT` names must NOT be in this table: those
/// two macros install their own forward types, whose store rules an
/// `Lisp_Fwd_Obj` would silently drop.
#[test]
fn table_and_the_defvar_bool_table_are_disjoint() {
    let bools: std::collections::HashSet<&str> = crate::emacs_core::defvar_bool::GNU_BOOL_VARIABLES
        .iter()
        .map(|v| v.name)
        .collect();
    let overlap: Vec<&str> = gnu_table::GNU_OBJECT_VARIABLES
        .iter()
        .map(|v| v.name)
        .filter(|n| bools.contains(n))
        .collect();
    assert_eq!(overlap, Vec::<&str>::new());
}
