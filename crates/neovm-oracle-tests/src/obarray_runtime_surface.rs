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

use crate::common::assert_oracle_deviation_with_env_expect;
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
    // misc: menu-bar's load runs `substitute-key-definition` over the
    // language-environment menu; looking up the mixed-case `Misc` entry
    // falls into Flookup_key's backwards-compatibility fallback, which
    // lowercases the key symbols and `Fintern`s them
    // (src/keymap.c:1330-1432, `Fintern` at :1386). The dump image
    // therefore holds the member. This port's fallback resolved the
    // lowercased name through the name-only interner, which was the one
    // open row of the peer-5786 membership audit (W2).
    let form = r#"(list (and (intern-soft "Sans Serif") t)
       (and (intern-soft "lcms2") t)
       (and (intern-soft "reverse-italic") t)
       (and (intern-soft "reverse-oblique") t)
       (and (intern-soft "utf-16le-dos") t)
       (and (intern-soft "misc") t))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    assert_oracle_parity_expect(form, expect);
}

/// Flookup_key's mixed-case menu fallback interns the converted names.
///
/// GNU's fallback (`src/keymap.c:1330-1432`) calls `Fintern` for every
/// lowercased symbol and again for its space-to-dash variant, so both
/// spellings are obarray members even when only the dashed one is bound.
/// All names are built with `intern`/`concat` so the form's reader cannot
/// pre-intern them and hide the effect.
#[test]
fn oracle_prop_menu_case_and_space_fallback_membership() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((m (make-sparse-keymap))
                 (lower (intern (concat "fo" "o-bar"))))
             (define-key m (vector 'menu-bar 'fallback-test lower) 'ignore)
             (list (lookup-key m (vector 'menu-bar 'fallback-test
                                         (intern (concat "Fo" "o Bar"))))
                   (and (intern-soft (concat "foo" " bar")) t)
                   (and (intern-soft (concat "fo" "o-bar")) t)))"#;
    let expect = expect_test::expect![[r#""OK (ignore t t)""#]];
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

/// Declined X toolkit DEFUN names are obarray members where GNU compiles them.
///
/// GNU's `defsubr` interns every compiled DEFUN, including the X toolkit
/// dialogs this port declines as subrs (DIVERGENCES 190/192). The names are
/// members on Linux (xfns.c) and absent where the reference does not compile
/// xfns.c (macOS NS, Windows); the port's generated DEFUN ledger follows the
/// same object lists. Only membership is asserted: the subr itself stays
/// declined, which the subr-surface ledger records.
#[test]
fn oracle_prop_declined_x_toolkit_defun_names_follow_the_profile() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (and (intern-soft "x-gtk-debug") t)
       (and (intern-soft "x-page-setup-dialog") t)
       (and (intern-soft "x-get-page-setup") t)
       (and (intern-soft "x-print-frames-dialog") t))"#;
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    #[cfg(any(windows, target_os = "macos"))]
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    assert_oracle_parity_expect(form, expect);
}

/// The same five names are members here but NOT fboundp: the documented
/// decline of the subrs, pinned so membership parity cannot read as
/// closing the subr gap.
///
/// GNU's `defsubr` leaves a function cell on every one of them in the
/// reference Linux build (xfns.c); this port interns the names through the
/// DEFUN ledger and declines the subrs (DIVERGENCES 190 §4). "Member, no
/// function" is a state no GNU build has, so the row pins BOTH sides:
/// Neomacs answers nil for all five while GNU answers t on Linux, and for
/// `x-select-font` on the other profiles too -- NS `src/nsfns.m:1688`,
/// w32 `src/w32font.c:2748`, pgtk `src/pgtkfns.c:3676`, Haiku
/// `src/haikufont.c:1242` compile the same DEFUN, while the four dialogs
/// are xfns.c-only and absent there.
#[test]
fn oracle_prop_declined_x_toolkit_defun_names_stay_members_without_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (fboundp 'x-get-page-setup)
       (fboundp 'x-gtk-debug)
       (fboundp 'x-page-setup-dialog)
       (fboundp 'x-print-frames-dialog)
       (fboundp 'x-select-font))"#;
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let gnu = expect_test::expect![[r#""OK (t t t t t)""#]];
    #[cfg(all(not(windows), target_os = "macos"))]
    let gnu = expect_test::expect![[r#""OK (nil nil nil nil t)""#]];
    #[cfg(windows)]
    let gnu = expect_test::expect![[r#""OK (nil nil nil nil t)""#]];
    assert_oracle_deviation_with_env_expect(form, &[], gnu, r#""OK (nil nil nil nil nil)""#);
}
