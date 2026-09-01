//! Oracle guards for GNU's `SKIP` doc marker -- the one string GNU's C says,
//! in as many words, is not documentation.
//!
//! A variable several window systems declare carries its doc text in exactly
//! one file and a placeholder in the rest: `x-pointer-shape` is `DEFVAR_LISP`
//! in `src/xfns.c:10327`, `src/w32fns.c:11809`, `src/haikufns.c:3284` and
//! `src/androidfns.c:3587`, and three of those four read
//! `doc: /* SKIP: real doc in xfns.c.  */`.
//!
//! **171 `DEF*` blocks across GNU's `src/*.c` carry the marker, and only 65 of
//! them are `DEFVAR`s** -- corrected 2026-08-21, ledger 181.  Entry 168 wrote
//! "170 `DEFVAR` blocks", which is the whole-tree count off by one and
//! attributed to the wrong declaration form: the other **106 are `DEFUN`s**,
//! and the function-side extractor had no `SKIP` filter at all, so 37 of them
//! reached `subr_docs::gnu_table` and `(documentation 'x-display-list)`
//! answered "SKIP: real doc in xfns.c.".  Both counts come from parsing
//! `make-docfile`'s own output rather than from a grep; the function-side
//! guards are in `subr_doc_snarf_rules.rs`.
//!
//! `Fsnarf_documentation` is where it is honoured, and the guard is explicit:
//!
//! ```c
//! /* Ignore docs that start with SKIP.  These mark
//!    placeholders where the real doc is elsewhere.  */
//! if ((!NILP (Fboundp (sym)) || !NILP (Fmemq (sym, delayed_init)))
//!     && strncmp (end, "\nSKIP", 5))
//!   Fput (sym, Qvariable_documentation, make_fixnum (pos + end + 1 - buf));
//! ```
//!
//! (`src/doc.c:600-608`.)  So no GNU build ever shows a `SKIP` string to a
//! user, in any window system, for any variable -- which makes it exactly the
//! kind of value that must be unrepresentable rather than merely absent.
//! Neomacs's `var_docs::gnu_table` is generated from all of GNU's `src/*.c`
//! and kept the alphabetically first copy of a duplicated name, so 35 rows
//! held the placeholder and `C-h v x-pointer-shape` answered
//! "SKIP: real text in xfns.c.".
//!
//! Both pins below are about the same table from two directions: the first
//! says the marker never reaches Lisp, the second says the text that reaches
//! Lisp instead is GNU's own.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// No built-in variable's documentation is GNU's placeholder.
///
/// Asked over every name whose generated row held one, and asked by prefix
/// rather than by equality so a future generator that invents a *different*
/// placeholder ("TODO", "see xterm.c") is caught by the same pin.  GNU's own
/// test is `strncmp (end, "\nSKIP", 5)`, a prefix test, for the same reason.
#[test]
fn oracle_no_builtin_variable_documentation_is_gnus_skip_placeholder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let (bad)
  (dolist (s '(font-use-system-font next-selection-coding-system
               selection-coding-system selection-converter-alist x-alt-keysym
               x-ctrl-keysym x-cursor-fore-pixel x-gtk-file-dialog-help-text
               x-gtk-show-hidden-files x-gtk-use-old-file-dialog
               x-hourglass-pointer-shape x-hyper-keysym x-max-tooltip-size
               x-meta-keysym x-mode-pointer-shape x-no-window-manager
               x-nontext-pointer-shape x-pixel-size-width-font-regexp
               x-pointer-shape x-sensitive-text-pointer-shape x-super-keysym
               x-toolkit-scroll-bars x-underline-at-descent-line
               x-use-underline-position-properties x-wait-for-event-timeout
               x-window-bottom-edge-cursor x-window-bottom-left-corner-cursor
               x-window-bottom-right-corner-cursor
               x-window-horizontal-drag-cursor x-window-left-edge-cursor
               x-window-right-edge-cursor x-window-top-edge-cursor
               x-window-top-left-corner-cursor x-window-top-right-corner-cursor
               x-window-vertical-drag-cursor))
    (let ((doc (documentation-property s 'variable-documentation)))
      (when (and (stringp doc) (string-prefix-p "SKIP" doc))
        (push s bad))))
  (nreverse bad))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// And the text that appears instead is the canonical file's.
///
/// The two names left out are `x-mode-pointer-shape` and
/// `x-nontext-pointer-shape`: GNU's only declarations of either are inside
/// `#if false /* This doesn't really do anything.  */` (`src/xfns.c:10333-10338`,
/// `10347-10352`, and the same pair in `src/androidfns.c`), so no build
/// declares them, `Fsnarf_documentation`'s `Fboundp` gate never fires, and GNU
/// answers nil.  Their *existence* is pinned by
/// `oracle_defvars_inside_a_dead_preprocessor_branch_are_unbound` below; their
/// text still differs, because only GNU's `Fboundp` clause can reject a doc the
/// generator can see, and applying that clause needs the 49 names GNU binds and
/// Neomacs does not (ledger 168, "found and NOT fixed").
#[test]
fn oracle_platform_duplicated_variables_carry_the_canonical_doc_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar (lambda (s)
          (let ((doc (documentation-property s 'variable-documentation)))
            (cons s (and (stringp doc) (car (split-string doc "\n"))))))
        '(font-use-system-font next-selection-coding-system
          selection-coding-system selection-converter-alist x-alt-keysym
          x-ctrl-keysym x-cursor-fore-pixel x-gtk-file-dialog-help-text
          x-gtk-show-hidden-files x-gtk-use-old-file-dialog
          x-hourglass-pointer-shape x-hyper-keysym x-max-tooltip-size
          x-meta-keysym x-no-window-manager x-pixel-size-width-font-regexp
          x-pointer-shape x-sensitive-text-pointer-shape x-super-keysym
          x-toolkit-scroll-bars x-underline-at-descent-line
          x-use-underline-position-properties x-wait-for-event-timeout
          x-window-bottom-edge-cursor x-window-bottom-left-corner-cursor
          x-window-bottom-right-corner-cursor x-window-horizontal-drag-cursor
          x-window-left-edge-cursor x-window-right-edge-cursor
          x-window-top-edge-cursor x-window-top-left-corner-cursor
          x-window-top-right-corner-cursor x-window-vertical-drag-cursor))"#;
    let expect = expect_test::expect![[
        r#""OK ((font-use-system-font . \"Non-nil means to apply the system defined font dynamically.\") (next-selection-coding-system . \"Coding system for the next communication with other programs.\") (selection-coding-system . \"Coding system for communicating with other programs.\") (selection-converter-alist . \"An alist associating X Windows selection-types with functions.\") (x-alt-keysym . \"Which modifier value Emacs reports when Alt is depressed.\") (x-ctrl-keysym . \"Which modifier value Emacs reports when Ctrl is depressed.\") (x-cursor-fore-pixel . \"A string indicating the foreground color of the cursor box.\") (x-gtk-file-dialog-help-text . \"If non-nil, the GTK file chooser will show additional help text.\") (x-gtk-show-hidden-files . \"If non-nil, the GTK file chooser will by default show hidden files.\") (x-gtk-use-old-file-dialog . \"Non-nil means prompt with the old GTK file selection dialog.\") (x-hourglass-pointer-shape . \"The shape of the pointer when Emacs is busy.\") (x-hyper-keysym . \"Which modifier value Emacs reports when Hyper is depressed.\") (x-max-tooltip-size . \"Maximum size for tooltips.\") (x-meta-keysym . \"Which modifier value Emacs reports when Meta is depressed.\") (x-no-window-manager . \"Non-nil if no X window manager is in use.\") (x-pixel-size-width-font-regexp . \"Regexp matching a font name whose width is the same as ‘PIXEL_SIZE’.\") (x-pointer-shape . \"The shape of the pointer when over text.\") (x-sensitive-text-pointer-shape . \"The shape of the pointer when over mouse-sensitive text.\") (x-super-keysym . \"Which modifier value Emacs reports when Super is depressed.\") (x-toolkit-scroll-bars . \"Which toolkit scroll bars Emacs uses, if any.\") (x-underline-at-descent-line . \"Non-nil means to draw the underline at the same place as the descent line.\") (x-use-underline-position-properties . \"Non-nil means make use of UNDERLINE_POSITION font properties.\") (x-wait-for-event-timeout . \"How long to wait for X events.\") (x-window-bottom-edge-cursor . \"Pointer shape indicating a bottom x-window edge can be dragged.\") (x-window-bottom-left-corner-cursor . \"Pointer shape indicating a bottom left x-window corner can be dragged.\") (x-window-bottom-right-corner-cursor . \"Pointer shape indicating a bottom right x-window corner can be dragged.\") (x-window-horizontal-drag-cursor . \"Pointer shape to use for indicating a window can be dragged horizontally.\") (x-window-left-edge-cursor . \"Pointer shape indicating a left x-window edge can be dragged.\") (x-window-right-edge-cursor . \"Pointer shape indicating a right x-window edge can be dragged.\") (x-window-top-edge-cursor . \"Pointer shape indicating a top x-window edge can be dragged.\") (x-window-top-left-corner-cursor . \"Pointer shape indicating a top left x-window corner can be dragged.\") (x-window-top-right-corner-cursor . \"Pointer shape indicating a top right x-window corner can be dragged.\") (x-window-vertical-drag-cursor . \"Pointer shape to use for indicating a window can be dragged vertically.\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The two names GNU compiles out of every build.
///
/// `#if false /* This doesn't really do anything.  */` wraps both
/// `DEFVAR_LISP`s in `src/xfns.c` (`10333-10338`, `10347-10352`) and both in
/// `src/androidfns.c`; `w32fns.c` and `haikufns.c` do not declare them at all.
/// A declaration inside a dead preprocessor branch is not a declaration, so
/// GNU leaves the symbols unbound -- entry 138's rule ("a build without MS-DOS
/// support needs `dos-hyper-key` to NOT exist") reaching a case that is not
/// about a platform at all.  `Vx_mode_pointer_shape` the C global still
/// exists and is still assigned `Qnil` on the line after the `#endif`, which
/// is what makes the seed look justified from the C side.
#[test]
fn oracle_defvars_inside_a_dead_preprocessor_branch_are_unbound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list (mapcar #'boundp '(x-mode-pointer-shape x-nontext-pointer-shape))
      ;; The live neighbours in the same `syms_of_xfns' block, so the pin
      ;; fails if the answer swings the other way and the group is deleted.
      (mapcar #'boundp '(x-pointer-shape x-hourglass-pointer-shape
                         x-sensitive-text-pointer-shape)))"#;
    let expect = expect_test::expect![[r#""OK ((nil nil) (t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
