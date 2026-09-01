//! The run-time half of [`provide_coupled_vars`]: ask the obarray, not the
//! table.
//!
//! `c_features_test.rs` does this for the `Fprovide` half -- a feature this
//! build advertises must have an implementation behind it -- and this is the
//! variable-side twin.  A table of GTK-only names is only as good as a check
//! that runs after loadup, because the two ways a name gets onto this obarray
//! are a Rust declaration and a preloaded `.el`, and neither is visible to
//! `rustc`.
//!
//! Ledger 199.

use crate::emacs_core::provide_coupled_vars::{
    CoupledFeature, HereDecision, PROVIDE_COUPLED_VARIABLES,
};
use crate::test_utils::runtime_startup_eval_one;

/// The features every row in the table is conditioned on are absent here.
///
/// The table says nothing at all about a build that HAS one of these; it is a
/// list of things this build cannot have.  If a future build gains GTK, the
/// rows stop applying and this test is what says so, loudly, before the scan
/// below starts reporting nonsense.
#[test]
fn the_features_the_table_is_conditioned_on_are_all_absent() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        "(list (featurep 'x) (featurep 'gtk) (featurep 'cairo) (featurep 'motif)
               (featurep 'pgtk) (featurep 'ns) (featurep 'haiku) (featurep 'w32)
               (featurep 'android) (eq system-type 'ms-dos)
               (featurep 'xwidget-internal) (featurep 'dbusbind)
               (featurep 'native-compile) (featurep 'dynamic-setting))",
    );
    // macOS provides `xwidget-internal': `backend/wkwebview' is a real
    // inline web view, so the eleventh probe answers t there. The three
    // xwidget rows below stop being policy exceptions on that platform and
    // become GNU-consistent -- the variables are bound because the feature is
    // present, which is exactly what GNU does.
    let expected = if cfg!(neomacs_have_wkwebview) {
        "OK (nil nil nil nil nil nil nil nil nil nil t nil nil nil)"
    } else {
        "OK (nil nil nil nil nil nil nil nil nil nil nil nil nil nil)"
    };
    assert_eq!(result, expected);
}

/// Every `Absent` row is really unbound, and every `BoundByPolicy` row is
/// really bound.
///
/// Both directions matter.  A row that says `Absent` while the name is bound is
/// the divergence this module was built for -- `gtk-version-string` held the
/// string `"3.24.51"` here, in a port with no GTK.  A row that says
/// `BoundByPolicy` while the name is gone is a policy outliving the thing it
/// excused, which is how a hand-written exception list rots (ledger 186).
///
/// The whole table is scanned in one runtime and every mismatch is reported,
/// so a regression that moves several names shows all of them rather than the
/// first.
#[test]
fn the_obarray_agrees_with_every_row() {
    crate::test_utils::init_test_tracing();

    let names: Vec<&str> = PROVIDE_COUPLED_VARIABLES.iter().map(|v| v.name).collect();
    let form = format!(
        "(mapconcat (lambda (s) (if (boundp s) \"t\" \"-\")) '({}) \"\")",
        names.join(" ")
    );
    let result = runtime_startup_eval_one(&form);
    let bits = result
        .strip_prefix("OK \"")
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or_else(|| panic!("unexpected scan result: {result}"));
    assert_eq!(
        bits.chars().count(),
        PROVIDE_COUPLED_VARIABLES.len(),
        "the scan did not answer for every row: {result}"
    );

    let mut wrong = Vec::new();
    for (var, bit) in PROVIDE_COUPLED_VARIABLES.iter().zip(bits.chars()) {
        let bound = bit == 't';
        match (var.here, bound) {
            (HereDecision::Absent, true) => wrong.push(format!(
                "{} is BOUND but the table says Absent -- GNU declares it only at {} \
                 ({:?}), and this build provides none of those",
                var.name, var.gnu, var.features
            )),
            (HereDecision::BoundByPolicy { policy }, false) => wrong.push(format!(
                "{} is UNBOUND but the table excuses it under a policy that is now \
                 stale: {policy}",
                var.name
            )),
            _ => {}
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} provide-coupled rows disagree with the obarray:\n  {}",
        wrong.len(),
        PROVIDE_COUPLED_VARIABLES.len(),
        wrong.join("\n  ")
    );
}

/// No toolkit-coupled variable reaches this obarray, and the `gtk`-named ones
/// that DO are exactly the set GNU binds without GTK.
///
/// `Gtk` and `Cairo` are the tightest couplings in GNU's C:
/// `Fprovide (intern_c_string ("gtk"))` and
/// `DEFVAR_LISP ("gtk-version-string", ...)` are two statements apart inside
/// one `#ifdef USE_GTK` (`src/xfns.c:10540-10549`), `#ifdef USE_CAIRO` pairs
/// `Fprovide ("cairo")` with `cairo-version-string` at `:10552-10558`, and
/// `src/pgtkfns.c:3786-3802` repeats both for the PGTK backend.  There is no
/// GNU build in which one exists without the other.
///
/// The second half is the boundary, and it is the half that keeps this test
/// honest.  A sweep for `gtk` in the *name* would be wrong: `src/xfns.c:10459`
/// says "This is not ifdef:ed, so other builds than GTK can customize it"
/// above four of them, so an X build with no GTK binds
/// `x-gtk-use-old-file-dialog`, `x-gtk-show-hidden-files`,
/// `x-gtk-file-dialog-help-text` and `x-gtk-resize-child-frames`, and
/// `src/xterm.c:32825` / `:32839` add `x-gtk-use-window-move` and
/// `x-gtk-use-native-input` on the same terms.  Those six are coupled to `x`,
/// not to `gtk`, and this build keeps them under the X-surface policy that
/// ledger 189 recorded.  `x-gtk-use-system-tooltips` is the seventh and is not
/// a C variable at all: `term/x-win.el:1572` aliases it onto the `DEFVAR_BOOL`
/// `use-system-tooltips` (`src/frame.c:7725`), and `lisp/term/neo-preload.el`
/// installs the same alias here.  Ledger 199 found it and did NOT remove it --
/// three oracle parity tests pin it against a GNU built with GTK.
#[test]
fn no_toolkit_coupled_variable_is_bound_and_the_gtk_named_survivors_are_gnus() {
    crate::test_utils::init_test_tracing();

    // Every toolkit version string GNU has, each declared beside the
    // `Fprovide` for its own toolkit and nowhere else, so `boundp` and
    // `featurep` cannot disagree: `motif-version-string` at
    // `src/xfns.c:10528` under `USE_X_TOOLKIT` + `USE_MOTIF`,
    // `gtk-version-string` at `:10542`, `cairo-version-string` at `:10555`,
    // and `ns-version-string` in `src/nsterm.m` under `HAVE_NS`.
    for name in ["gtk-version-string", "cairo-version-string"] {
        let row = PROVIDE_COUPLED_VARIABLES
            .iter()
            .find(|v| v.name == name)
            .unwrap_or_else(|| panic!("{name} has no row, so this test proves nothing"));
        assert_eq!(row.here, HereDecision::Absent, "{name}");
        assert!(
            row.features
                .iter()
                .any(|f| matches!(f, CoupledFeature::Gtk | CoupledFeature::Cairo)),
            "{name} is not toolkit-coupled: {:?}",
            row.features
        );
    }

    let result = runtime_startup_eval_one(
        "(list (mapcar (lambda (s) (and (boundp s) s))
                       '(motif-version-string gtk-version-string
                         cairo-version-string ns-version-string))
               (let (n) (mapatoms (lambda (s)
                                    (and (boundp s)
                                         (string-match-p \"gtk\" (symbol-name s))
                                         (push (symbol-name s) n))))
                    (sort n #'string<)))",
    );
    assert_eq!(
        result,
        "OK ((nil nil nil nil) (\"x-gtk-file-dialog-help-text\" \
         \"x-gtk-resize-child-frames\" \"x-gtk-show-hidden-files\" \
         \"x-gtk-use-native-input\" \"x-gtk-use-old-file-dialog\" \
         \"x-gtk-use-system-tooltips\" \"x-gtk-use-window-move\"))"
    );
}

/// `use-system-tooltips` stays, with GNU's `t`, and stays inert.
///
/// It is `DEFVAR_BOOL` at `src/frame.c:7725` -- outside every `#ifdef`, so
/// `syms_of_frame` binds it in a GNU tty build too -- and its own docstring
/// says it is "only meaningful when Emacs is built with GTK+, NS or Haiku
/// windowing support".  Every C reader of it is a toolkit backend (`xfns.c`,
/// `pgtkfns.c`, `haikufns.c`, `haikumenu.c`).  Removing it because its
/// docstring mentions GTK would be the mirror image of binding
/// `gtk-version-string`, so the two facts are pinned side by side here.
#[test]
fn the_platform_neutral_tooltip_variable_keeps_gnus_default() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        "(list (boundp 'use-system-tooltips) use-system-tooltips
               (special-variable-p 'use-system-tooltips)
               (boundp 'scroll-bar-adjust-thumb-portion)
               scroll-bar-adjust-thumb-portion)",
    );
    assert_eq!(result, "OK (t t t t t)");
}
