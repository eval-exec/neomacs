//! The `#ifdef` half of DIVERGENCES.md 138's question, asked of FUNCTIONS:
//! **which of GNU's own subr names does the reference build declare, and which
//! does this one?**
//!
//! `crates/neovm-core/src/emacs_core/gnu_subr_surface_test.rs` owns the half that can
//! be settled offline -- a name GNU's `src/*.c` has no `DEFUN` for at all.
//! This file owns the half that needs a running GNU: whether GNU's
//! `#ifdef HAVE_XWIDGETS` (or `HAVE_GPM`, or `HAVE_NATIVE_COMP`) is true for
//! the build we compare against is a fact about a binary, not about a source
//! tree.
//!
//! The reference GNU is 31.0.90 (mirror `0ee48ac4df2`) configured
//!
//! ```text
//! --with-native-compilation=no --with-tree-sitter --with-x-toolkit=gtk3
//! ```
//!
//! so it compiles `xfns.c`'s GTK dialogs and `dbusbind.c`, and compiles neither
//! `xwidget.c` nor `comp.c`'s guarded body nor `term.c`'s Gpm block.  This port
//! has xwidgets (the WPE/WebKit render path) and is not an X client, so the two
//! builds legitimately declare different names -- ledger 183 ruled the xwidget
//! pair not a divergence, and ledger 190 measured the original set for functions:
//! **63 names this build declared and GNU did not, 9 the other way.**  The
//! typed PRIMARY ownership primitive added one port-owned name.  Ledger
//! 192 made the second number **12**: it deleted the three `dbus` subrs 190 had
//! left standing, because this build has no D-Bus transport and GNU's whole
//! `src/dbusbind.c` is one `#ifdef HAVE_DBUS`.
//!
//! ## Why the GNU-side statement is a PARITY test and not a GNU-side pin
//!
//! `run_oracle_eval` runs the **neomacs binary** in the default snapshot mode
//! (`common.rs:690-696`), so a test shaped "assert GNU answers X while neomacs
//! answers Y" asserts nothing about GNU unless the suite happens to be running
//! in verify/live mode.  What CAN be pinned in every mode is agreement, so the
//! 27 names ledger 190 deleted are pinned as agreement: GNU does not declare
//! them and, after 190, neither does this port.  That is the direction the
//! entry actually changed, and it is checked in both editors on every run.
//!
//! Ledger 190.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Names this build declares as primitive subrs and the reference GNU does not.
///
/// 23 xwidget subrs (`src/xwidget.c`, whole file behind `XWIDGETS_OBJ`,
/// `configure.ac:4455-4507`), `x-load-color-file` (`src/xfaces.c:7583`, whose
/// guard is `#ifndef HAVE_X_WINDOWS` -- GNU declares it in the NON-X branch,
/// which is this build's branch), and 40 primitives of this port's own in this
/// port's own namespace, as GNU carries `w32-`, `ns-` and `haiku-` ones.
const DECLARED_HERE_ONLY: &str = "
  delete-xwidget-view get-buffer-xwidgets kill-xwidget
  make-xwidget neomacs--debug-lose-device neomacs--frame-snapshot
  neomacs--heap-layout-stats neomacs--write-frame-snapshot neomacs-buffer-text-backend
  neomacs-clipboard-get neomacs-clipboard-set neomacs-core-backend
  neomacs-default-buffer-text-backend neomacs-display-monitor-attributes-list neomacs-effect-get
  neomacs-effect-names neomacs-effect-reset neomacs-effect-set
  neomacs-effects-apply neomacs-frame-edges neomacs-frame-geometry
  neomacs-frame-shader neomacs-frame-shader-set-uniform neomacs-image-extent
  neomacs-mouse-absolute-pixel-position neomacs-open-tls-stream neomacs-primary-selection-get
  neomacs-primary-selection-owner neomacs-primary-selection-set neomacs-set-buffer-text-backend
  neomacs-set-default-buffer-text-backend
  neomacs-set-mouse-absolute-pixel-position neomacs-surface-available-p neomacs-surface-create
  neomacs-surface-destroy neomacs-surface-set-uniform neomacs-terminal-create
  neomacs-terminal-destroy neomacs-terminal-get-text neomacs-terminal-resize
  neomacs-terminal-set-float neomacs-terminal-write neomacs-tls-available-p
  neovm--internal-panic set-xwidget-buffer set-xwidget-plist
  set-xwidget-query-on-exit-flag x-load-color-file xwidget-buffer
  xwidget-info xwidget-live-p xwidget-plist
  xwidget-query-on-exit-flag xwidget-resize xwidget-size-request
  xwidget-view-info xwidget-view-lookup xwidget-view-model
  xwidget-view-p xwidget-view-window xwidget-webkit-goto-uri
  xwidget-webkit-title xwidget-webkit-uri xwidgetp
";

/// Names the reference GNU declares as primitive subrs and this build does not.
///
/// **Twelve**, and ledger 192 is why it is not ledger 190's nine.
///
/// All six of `syms_of_dbusbind`'s subrs (`src/dbusbind.c:2003-2010`) are here
/// now, not three.  190 left `dbus--init-bus`, `dbus-get-unique-name` and
/// `dbus-message-internal` declared and declined the missing `dbus--fd-*`,
/// noting that "the three present ones are the defect, not the three absent
/// ones" and handing on the question of whether `(featurep 'dbusbind)` should
/// be `t` here at all.  192 settled it: this build has no D-Bus transport, so
/// it is in GNU's own `--without-dbus` configuration, in which the whole of
/// `src/dbusbind.c` -- guarded by one `#ifdef HAVE_DBUS` at `:21`, closed at
/// `:2178` -- is not compiled.  The three that used to be declared answered a
/// hardcoded `2`, a fabricated `":1.1"` unique name, and an invented
/// `dbus-event` reply naming `"org.freedesktop.DBus"`.
///
/// The other six are the X toolkit dialogs (`src/xfns.c:10632-10655`, under
/// `USE_MOTIF`/`USE_GTK`/`USE_CAIRO`/`HAVE_GTK3`), declined by ledger 190 with
/// the reason recorded there.  All twelve are pinned so the declines stay
/// visible.
const DECLARED_BY_GNU_ONLY: &str = "
  dbus--fd-close dbus--fd-open dbus--init-bus
  dbus--registered-fds dbus-get-unique-name dbus-message-internal
  x-file-dialog x-get-page-setup x-gtk-debug
  x-page-setup-dialog x-print-frames-dialog x-select-font
";

/// The 27 names ledger 190 DELETED, and neither editor may declare.
///
/// Thirteen `comp`/`native-elisp-load` names GNU registers only inside
/// `#ifdef HAVE_NATIVE_COMP` (`src/comp.c:5693-5706`, while
/// `native-comp-available-p` at `:5828` sits after the `#endif`); the two Gpm
/// entry points (`src/term.c:5282-5286`); `fontset-list-all`
/// (`src/fontset.c:2254`, `#ifdef ENABLE_CHECKING`); `overlay-tree`
/// (`src/buffer.c:6117`, `#ifdef ITREE_DEBUG`, and `ITREE_DEBUG` is defined
/// nowhere in GNU's tree); four names with no occurrence anywhere in GNU;
/// two whose only occurrence in the whole GNU tree is a stale
/// `declare-function` in `lisp/treesit.el`; and four Rust reimplementations of
/// Lisp this port ships (`lisp/kmacro.el`, `lisp/obsolete/tls.el`).
const DELETED_BY_LEDGER_190: &str = "
  comp--compile-ctxt-to-file0 comp--init-ctxt comp--install-trampoline
  comp--late-register-subr comp--register-lambda comp--register-subr
  comp--release-ctxt comp--subr-signature comp-el-to-eln-filename
  comp-el-to-eln-rel-filename comp-native-compiler-options-effective-p
  comp-native-driver-options-effective-p defining-kbd-macro-p
  executing-kbd-macro-p fontset-list-all gpm-mouse-start
  gpm-mouse-stop kmacro-add-counter kmacro-set-counter
  kmacro-set-format native-elisp-load open-tls-stream
  overlay-tree treesit-language-version treesit-parser-changed-ranges
  x-scroll-bar-background x-scroll-bar-foreground
";

/// One probe: which of MUST does the editor running it fail to declare, and
/// which of MUST-NOT does it declare?
///
/// It answers the disagreements BY NAME rather than as a count, because two
/// groupings can agree on every count and disagree on membership (ledger 179
/// §4 paid for that).  The third element is the anti-vacuity check: an editor
/// that failed to boot, or a `mapatoms` that walked nothing, reports `nil`
/// there instead of passing two empty-list assertions.
fn probe(must_declare: &str, must_not_declare: &str) -> String {
    format!(
        "(let ((declared (let (r)
                           (mapatoms
                            (lambda (s)
                              (let ((f (and (fboundp s) (symbol-function s))))
                                (if (and f (subr-primitive-p f))
                                    (push (symbol-name s) r)))))
                           r))
               (must '({must_declare}))
               (must-not '({must_not_declare}))
               missing present)
           (dolist (n must)
             (unless (member (symbol-name n) declared) (push (symbol-name n) missing)))
           (dolist (n must-not)
             (if (member (symbol-name n) declared) (push (symbol-name n) present)))
           (list (sort missing #'string<)
                 (sort present #'string<)
                 (> (length declared) 1000)))"
    )
}

/// Neither editor declares any of the 27 names ledger 190 deleted.
///
/// A parity assertion, so it is checked in both editors in every oracle mode.
/// It is also the regression pin for the deletions: re-registering
/// `comp--init-ctxt`, `gpm-mouse-start` or `overlay-tree` turns this RED and
/// names the row.
///
/// RED before ledger 190, against a `fresh-build --release` binary of
/// `79b418443`: all 27 present on the neomacs side, none on GNU's.
#[test]
fn oracle_neither_editor_declares_the_names_ledger_190_deleted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The MUST half is three names both editors really do declare, so the
    // probe's own machinery is exercised rather than trusted: a broken
    // `member`, a broken `mapatoms` or a `declared` list built from the wrong
    // predicate reports them as missing instead of passing vacuously.
    let form = probe(
        "start-kbd-macro re-search-forward native-comp-available-p",
        DELETED_BY_LEDGER_190,
    );
    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

/// The capability answers behind the largest of those deletions agree, and are
/// GNU's own.
///
/// GNU registers exactly ONE `comp.c` subr outside `#ifdef HAVE_NATIVE_COMP`
/// (`src/comp.c:5828`) and `Fprovide`s `native-compile` only inside it
/// (`:5825`).  Both editors answer "no native compilation" -- so the thirteen
/// names this port used to declare were a capability claim its own
/// `native-comp-available-p` denied.  Same shape for Gpm: GNU's
/// `lisp/t-mouse.el:49` uses `(fboundp 'gpm-mouse-start)` as the build test,
/// and both editors now answer nil.
#[test]
fn oracle_native_compilation_and_gpm_capability_answers_agree() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list (and (fboundp 'native-comp-available-p) t)
               (native-comp-available-p)
               (featurep 'native-compile)
               (fboundp 'comp--init-ctxt)
               (fboundp 'gpm-mouse-start)
               (fboundp 'gpm-mouse-stop))",
        expect,
    );
}

/// `defining-kbd-macro` is `lisp/help.el:356`'s `fset` of `start-kbd-macro`'s
/// subr object, in both editors.
///
/// GNU has no `DEFUN` of that name; the `fset` exists so the two names share
/// one subr and one doc string -- "So keyboard macro definitions are documented
/// correctly", says the comment above it.  This port shipped the same
/// `help.el` line AND a Rust subr of the same name registered after loadup,
/// which silently replaced the `fset`'s result.
///
/// RED before ledger 190, `fresh-build --release` of `79b418443`:
/// `("start-kbd-macro" "defining-kbd-macro" nil nil)` against GNU's
/// `("start-kbd-macro" "start-kbd-macro" t "Record subsequent keyboard input,
/// defining a keyboard macro.")`.
#[test]
fn oracle_defining_kbd_macro_is_help_els_fset_of_start_kbd_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"start-kbd-macro\" \"start-kbd-macro\" t \"Record subsequent keyboard input, defining a keyboard macro.\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(list (subr-name (symbol-function 'start-kbd-macro))
               (subr-name (symbol-function 'defining-kbd-macro))
               (eq (symbol-function 'start-kbd-macro)
                   (symbol-function 'defining-kbd-macro))
               (car (split-string (documentation 'defining-kbd-macro) \"\\n\")))",
        expect,
    );
}

/// `syms_of_macros`'s whole subr surface, both editors, by name.
///
/// The tests above all look in ONE direction -- a name this build declares and
/// GNU does not.  Nothing in them can see a name GNU declares and this build
/// stops declaring, and ledger 190 needed that lesson taught by its own
/// tooling: the script that deleted `defining-kbd-macro`'s registration walked
/// back to the nearest `ctx.defsubr(` and, because that name was registered
/// with `ctx.defsubr_interactive(`, swallowed the block above it and removed
/// `store-kbd-macro-event` as well.  Nothing caught it -- `kmacro_test.rs`
/// calls the Rust function directly, and `grep -r store-kbd-macro-event` over
/// `neovm-oracle-tests` and `neomacs-melpa-tests` finds **zero** occurrences --
/// so a subr GNU has had since Emacs 19 was gone from the binary and every
/// suite was green.  Re-measuring the symmetric difference is what found it.
///
/// This pin is the narrow repair: `src/macros.c:420-425` is GNU's whole list
/// for that file, unguarded by any `#ifdef`, so both editors must declare all
/// six.  The general guard -- "no name GNU's reference build declares may
/// silently leave this one" -- is sized in DIVERGENCES.md 190 and not built.
#[test]
fn oracle_both_editors_declare_all_six_macros_c_subrs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // src/macros.c:420-425, in GNU's own order.
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(mapcar (lambda (s) (and (fboundp s)
                                  (subr-primitive-p (symbol-function s))
                                  t))
                 '(start-kbd-macro end-kbd-macro call-last-kbd-macro
                   execute-kbd-macro cancel-kbd-macro-events
                   store-kbd-macro-event))",
        expect,
    );
}

/// This build declares all 64 of its own names and none of GNU's 12.
///
/// Runs the neomacs binary unconditionally (`run_neovm_eval`), so unlike a
/// snapshot parity test it is a measurement of THIS build in every mode.  The
/// GNU side of the same statement is recorded in DIVERGENCES.md 190 and is
/// re-measured with
///
/// ```text
/// emacs -Q --batch --eval '(let (r) (mapatoms (lambda (s) (let ((f (and (fboundp s) (symbol-function s)))) (when (and f (subr-primitive-p f)) (push (symbol-name s) r))))) (dolist (n (sort r #\'string<)) (princ n) (terpri)))'
/// ```
///
/// rather than pinned here, for the reason in the module header.
#[test]
fn oracle_this_build_declares_its_sixty_four_and_none_of_gnus_twelve() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = probe(DECLARED_HERE_ONLY, DECLARED_BY_GNU_ONLY);
    let neovm = crate::common::run_neovm_eval(&form).expect("neomacs eval should run");
    assert_eq!(
        neovm, "OK (nil nil t)",
        "this build's subr surface moved: the first list is the name(s) of the \
         sixty-four it no longer declares, the second is the name(s) of GNU's \
         twelve it has started declaring, and a nil third element means the \
         probe never saw a booted obarray.  See DIVERGENCES.md 190 and 192."
    );
}
