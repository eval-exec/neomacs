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
    // each family string (src/xfaces.c:5877). lcms2: C
    // `Fprovide (intern_c_string ...)` (src/lcms.c:601). reverse-italic /
    // reverse-oblique: font.c's BUILD_STYLE_TABLE names. utf-16le-dos:
    // make_subsidiaries (src/coding.c:10968-10981).
    //
    // `misc` (indian.el's dynamic-binding variable) is NOT in this row: GNU's
    // final image interns it while reading `indian.elc`, this port's dump-mode
    // bytecode load resolves the same symbol through the name-only interner
    // and never marks membership. It is the one open row of the peer-5786
    // membership audit and is tracked as W2, not pinned green here.
    let form = r#"(list (and (intern-soft "Sans Serif") t)
       (and (intern-soft "lcms2") t)
       (and (intern-soft "reverse-italic") t)
       (and (intern-soft "reverse-oblique") t)
       (and (intern-soft "utf-16le-dos") t))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    assert_oracle_parity_expect(form, expect);
}

/// Spellings a profile without the matching `#ifdef` must not intern.
///
/// `fontsize` is a Linux absence and a macOS presence: `nsfns.m` DEFSYMs it,
/// so the NS reference build and a macOS Neo both intern it. The row is
/// host-keyed instead of pinned to the Linux answer.
#[test]
fn oracle_prop_fontsize_follows_the_compiled_object_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    #[cfg(all(not(windows), not(target_os = "macos")))]
    let expect = expect_test::expect![[r#""OK (nil)""#]];
    #[cfg(all(not(windows), target_os = "macos"))]
    let expect = expect_test::expect![[r#""OK (t)""#]];
    #[cfg(windows)]
    let expect = expect_test::expect![[r#""OK (nil)""#]];
    assert_oracle_parity_expect(r#"(list (and (intern-soft "fontsize") t))"#, expect);
}

/// Whether the neomacs binary under test advertises the xwidget layer.
///
/// The decision is the build's own `xwidgets` feature (the port's key for
/// GNU's HAVE_XWIDGETS); the macOS product enables it through `webview`.
/// Rows branch on this rather than on the host OS, so a
/// `--features xwidgets` Linux build is exercised the same way as the
/// macOS product.
pub(crate) fn neovm_advertises_xwidget_internal() -> bool {
    crate::common::run_neovm_eval("(and (featurep 'xwidget-internal) t)")
        .expect("neomacs featurep probe should run")
        .trim()
        == "OK t"
}

/// The xwidget spelling/definition surface follows the RUNNING build's
/// feature, not the host OS.
///
/// - Feature absent (the reference profiles this row usually measures): every
///   xwidget spelling is absent from both editors, matching the reference GNU
///   built without `--with-xwidgets`.
/// - Feature present (macOS product through `webview`): the layer's three
///   surfaces must agree inside this build (`featurep` / `fboundp
///   make-xwidget` / `boundp xwidget-list`). GNU's reference oracle has no
///   xwidgets, so this half is an internal-agreement claim, not parity.
#[test]
fn oracle_prop_xwidget_surface_follows_the_running_feature() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    if neovm_advertises_xwidget_internal() {
        let form = r#"(list (and (featurep 'xwidget-internal) t)
                         (and (fboundp 'make-xwidget) t)
                         (and (boundp 'xwidget-list) t))"#;
        let neovm = crate::common::run_neovm_eval(form).expect("neomacs eval should run");
        assert_eq!(
            neovm, "OK (t t t)",
            "a build that advertises xwidget-internal must also declare              make-xwidget and bind xwidget-list (PEER 5796 R8)"
        );
        return;
    }

    let spellings = r#"(list (and (intern-soft "xwidgetp") t)
       (and (intern-soft "xwidget-live-p") t)
       (and (intern-soft "xwidget-view-p") t)
       (and (intern-soft "xwidget-list") t))"#;
    assert_oracle_parity_expect(
        spellings,
        expect_test::expect![[r#""OK (nil nil nil nil)""#]],
    );
    let definitions = r#"(list (fboundp 'xwidgetp)
       (fboundp 'xwidget-live-p)
       (fboundp 'xwidget-view-p)
       (boundp 'xwidget-list)
       (boundp 'xwidget-webkit-disable-javascript))"#;
    assert_oracle_parity_expect(
        definitions,
        expect_test::expect![[r#""OK (nil nil nil nil nil)""#]],
    );
}

/// Literal C startup interns (`intern_c_string`) land in the profile that
/// compiles the file, exactly as GNU's object list decides.
#[test]
fn oracle_prop_c_intern_spellings_follow_the_profile() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `accelerate-menu` is xmenu.c, `bidi-class` is bidi.c (both compiled in
    // the Linux/GTK reference); `kqueue` is kqueue.c (macOS); `w32notify` is
    // w32notify.c (Windows).
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let (form, expect) = (
        r#"(list (and (intern-soft "accelerate-menu") t)
           (and (intern-soft "bidi-class") t)
           (and (intern-soft "kqueue") t))"#,
        expect_test::expect![[r#""OK (t t nil)""#]],
    );
    #[cfg(all(not(windows), target_os = "macos"))]
    let (form, expect) = (
        r#"(list (and (intern-soft "accelerate-menu") t)
           (and (intern-soft "kqueue") t)
           (and (intern-soft "w32notify") t))"#,
        expect_test::expect![[r#""OK (nil t nil)""#]],
    );
    #[cfg(windows)]
    let (form, expect) = (
        r#"(list (and (intern-soft "w32notify") t)
           (and (intern-soft "accelerate-menu") t)
           (and (intern-soft "kqueue") t))"#,
        expect_test::expect![[r#""OK (t nil nil)""#]],
    );
    assert_oracle_parity_expect(form, expect);
}
