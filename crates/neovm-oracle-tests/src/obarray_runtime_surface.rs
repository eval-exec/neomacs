//! Runtime-interned obarray surface: the two directions of the peer 5786
//! membership audit.
//!
//! GNU interns symbols at startup that no `DEFSYM` in the compiled sources
//! names - font style tables, C `Fprovide` names, coding-system subsidiaries,
//! face-family alternatives, and spellings the dump-build reader saw in Lisp
//! sources - and it does not intern spellings whose `#ifdef` never compiled
//! (`xwidget.c` without `--with-xwidgets`, `nsfns.m` on Linux). Each row is
//! measured against the attested oracle on the shipped binary.
//!
//! Every name is queried as a STRING: a quoted symbol in the form would be
//! interned by the reader before `intern-soft` runs and hide the answer.

use crate::common::assert_oracle_parity_expect;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Spellings GNU interns from its fonts / C features / coding / face startup.
#[test]
fn oracle_prop_runtime_interned_boot_spellings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sans Serif: faces.el's face-font-family-alternatives setter interning
    // each family string (src/xfaces.c:5877). misc: read from
    // lisp/language/indian.el's source during the dump build. lcms2: C
    // `Fprovide (intern_c_string ...)` (src/lcms.c:601). reverse-italic /
    // reverse-oblique: font.c's BUILD_STYLE_TABLE names. utf-16le-dos:
    // make_subsidiaries (src/coding.c:10968-10981).
    let form = r#"(list (and (intern-soft "Sans Serif") t)
       (and (intern-soft "misc") t)
       (and (intern-soft "lcms2") t)
       (and (intern-soft "reverse-italic") t)
       (and (intern-soft "reverse-oblique") t)
       (and (intern-soft "utf-16le-dos") t))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    assert_oracle_parity_expect(form, expect);
}

/// Spellings a profile without the matching `#ifdef` must not intern.
#[test]
fn oracle_prop_compiled_out_spellings_are_absent_from_the_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // fontsize is DEFSYM'd in nsfns.m; xwidgetp / xwidget-live-p /
    // xwidget-view-p and the xwidget variables exist only with GNU's
    // `--with-xwidgets` (configure.ac:4456-4497). The reference linux-gtk3
    // profile has none of them.
    let form = r#"(list (and (intern-soft "fontsize") t)
       (and (intern-soft "xwidgetp") t)
       (and (intern-soft "xwidget-live-p") t)
       (and (intern-soft "xwidget-view-p") t)
       (and (intern-soft "xwidget-list") t))"#;
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    assert_oracle_parity_expect(form, expect);
}

/// The same compiled-out names are not functions or variables either.
#[test]
fn oracle_prop_compiled_out_definitions_are_absent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (fboundp 'xwidgetp)
       (fboundp 'xwidget-live-p)
       (fboundp 'xwidget-view-p)
       (boundp 'xwidget-list)
       (boundp 'xwidget-webkit-disable-javascript))"#;
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    assert_oracle_parity_expect(form, expect);
}
