use super::*;

/// GNU's `src/*.c` holds 1733 `DEFUN` doc records once
/// `Fsnarf_documentation`'s `SKIP` clause has refused the placeholders.
///
/// A floor rather than an equality, because the mirror moves; a floor
/// rather than a `is_empty()` check, because the failure this guards is
/// silent shrinkage.  The number is what the generator printed against
/// GNU 31.0.90 (`0ee48ac4df2`).  Ledger 181's whole finding was 30 rows
/// missing from a table nobody had counted, so the count is pinned.
#[test]
fn gnu_subr_docs_is_not_quietly_smaller_than_gnus_source() {
    assert!(
        gnu_table::GNU_SUBR_DOCS.len() >= 1733,
        "GNU_SUBR_DOCS has {} rows; GNU's src/*.c has 1733 non-SKIP DEFUN \
         doc records. A generator that drops rows is silent from every \
         direction except this one.",
        gnu_table::GNU_SUBR_DOCS.len()
    );
}

/// The table is sorted and free of duplicate names, so a linear scan and a
/// future binary search cannot disagree about which row a name resolves to.
#[test]
fn gnu_subr_docs_is_sorted_and_has_no_duplicate_names() {
    let mut previous: Option<&str> = None;
    for (name, _) in gnu_table::GNU_SUBR_DOCS {
        if let Some(prev) = previous {
            assert!(
                prev < *name,
                "GNU_SUBR_DOCS is not strictly sorted: {prev:?} then {name:?}"
            );
        }
        previous = Some(name);
    }
}

/// The six names ledger 181 found missing or wrong, one per mechanism, so
/// a regression names its own cause.
///
/// * `native-comp-function-p` -- the head regex needed a line break after
///   the interactive spec and `data.c:1067` has none.
/// * `define-charset-internal` -- MIN is the C identifier
///   `charset_arg_max` (`charset.c:845`), not a literal.
/// * `window-parameter` -- `window.c:2390` spells the marker `doc:  /*`
///   with two spaces, so the unbounded literal search took
///   `set-window-parameter`'s comment instead.
/// * `set-window-parameter` -- and was then skipped as collateral.
/// * `x-display-list` -- `androidfns.c` supplies the alphabetically first
///   copy, which is GNU's `SKIP` placeholder.
/// * `string` -- `character.c`-style `doc: /*` followed by a newline left
///   a leading newline in the stored text.
#[test]
fn the_six_mechanisms_ledger_181_found_are_each_represented() {
    let rows: std::collections::HashMap<&str, &str> =
        gnu_table::GNU_SUBR_DOCS.iter().copied().collect();
    for (name, first_line) in [
        (
            "native-comp-function-p",
            "Return t if the object is native-compiled Lisp function, nil otherwise.",
        ),
        ("define-charset-internal", "For internal use only."),
        ("window-parameter", "Return WINDOW's value for PARAMETER."),
        (
            "set-window-parameter",
            "Set WINDOW's value of PARAMETER to VALUE.",
        ),
        (
            "x-display-list",
            "Return the list of display names that Emacs has connections to.",
        ),
        (
            "string",
            "Concatenate all the argument characters and make the result a string.",
        ),
    ] {
        let doc = rows
            .get(name)
            .unwrap_or_else(|| panic!("GNU_SUBR_DOCS has no row for {name:?}"));
        assert_eq!(
            doc.lines().next().unwrap_or(""),
            first_line,
            "GNU_SUBR_DOCS row for {name:?} is not GNU's text"
        );
    }
}
