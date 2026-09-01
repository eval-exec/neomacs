//! Oracle guards for the seven `lisp/cus-start.el` platform names GNU DOES
//! bind in a build like this one -- the residual entry 138 handed to entry 141.
//!
//! Entry 138 answered the *existence* question for these names and stopped
//! there, seeding each one to nil.  A `DEFVAR_LISP` is not a nil cell: it
//! supplies the initializer AND sets `declared_special`, in one statement
//! (`src/lisp.h:3500-3512`, `src/eval.c:defvar_lisp_nopro`).  Five of the seven
//! had neither, so the pins here are about both halves at once:
//!
//! 1. **The default.**  `x-bitmap-file-path` is
//!    `decode_env_path (0, PATH_BITMAPS, 0)` (`src/image.c:13265-13267`),
//!    `x-scroll-event-delta-factor` is `make_float (1.0)`
//!    (`src/xterm.c:32833-32837`) and `x-auto-preserve-selections` is
//!    `list2 (QCLIPBOARD, QPRIMARY)` (`src/xterm.c:32976-32984`).  Each of
//!    those is read back by C that treats nil as a different mode, not as a
//!    smaller version of the same one -- `x_should_preserve_selection`
//!    preserves nothing at all when the list is nil
//!    (`src/xselect.c:1385-1401`).
//! 2. **The special bit.**  Being non-special is observable from Lisp: under
//!    lexical binding a `let` over a plain obarray cell makes a LEXICAL
//!    binding, so `symbol-value` inside the `let` still answers the global.
//!    That is the probe every case here ends with.
//! 3. **`x-gtk-use-system-tooltips` is not a variable in any GNU build.**  It
//!    is `(defvaralias 'x-gtk-use-system-tooltips 'use-system-tooltips)` in
//!    `lisp/term/x-win.el:1572`, onto the `DEFVAR_BOOL` at `src/frame.c:7725`,
//!    so it must answer `indirect-variable`, inherit the target's `t`, and
//!    coerce like a Boolean slot.  A separate Rust cell reproduces none of
//!    those and lets the two names drift apart.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// All seven platform names GNU binds here, in one shot: value, the `special`
/// bit the declaration carries, the default, and buffer-locality.
#[test]
fn oracle_cus_start_platform_names_gnu_binds_here_carry_gnus_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar (lambda (s)
          (list s (boundp s) (symbol-value s) (special-variable-p s)
                (equal (symbol-value s) (default-value s))
                (local-variable-if-set-p s)))
        '(window-combination-limit void-text-area-pointer x-bitmap-file-path
          x-gtk-use-system-tooltips x-scroll-event-delta-factor
          x-auto-preserve-selections vertical-centering-font-regexp))"#;
    let expect = expect_test::expect![[
        r#""OK ((window-combination-limit t window-size t t nil) (void-text-area-pointer t arrow t t nil) (x-bitmap-file-path t (\"/usr/include/X11/bitmaps\") t t nil) (x-gtk-use-system-tooltips t t t t nil) (x-scroll-event-delta-factor t 1.0 t t nil) (x-auto-preserve-selections t (CLIPBOARD PRIMARY) t t nil) (vertical-centering-font-regexp t \"gb2312\\\\|gbk\\\\|gb18030\\\\|jisx0208\\\\|jisx0212\\\\|ksc5601\\\\|cns11643\\\\|big5\" t t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The `special` bit, exercised rather than merely reported.
///
/// `special-variable-p` answering `nil` is not cosmetic: `eval` with
/// lexical binding turns `(let ((x-bitmap-file-path V)) ...)` into a lexical
/// binding for a non-special variable, and `symbol-value` -- which only ever
/// reads the dynamic value -- keeps answering the global.  GNU's `DEFVAR_LISP`
/// makes all five of these dynamic.
#[test]
fn oracle_platform_names_bind_dynamically_under_let_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar (lambda (s)
          (cons s (eval `(let ((,s 'probe-marker)) (symbol-value ',s)) t)))
        '(x-bitmap-file-path x-scroll-event-delta-factor
          x-auto-preserve-selections vertical-centering-font-regexp
          window-combination-limit void-text-area-pointer))"#;
    let expect = expect_test::expect![[
        r#""OK ((x-bitmap-file-path . probe-marker) (x-scroll-event-delta-factor . probe-marker) (x-auto-preserve-selections . probe-marker) (vertical-centering-font-regexp . probe-marker) (window-combination-limit . probe-marker) (void-text-area-pointer . probe-marker))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// `x-gtk-use-system-tooltips` is an alias onto a `DEFVAR_BOOL`, so it must
/// resolve to `use-system-tooltips`, share its value cell, and coerce every
/// assignment to `t`/`nil` the way `store_symval_forwarding`'s `Lisp_Fwd_Bool`
/// arm does (`src/data.c:1485-1487`).
#[test]
fn oracle_x_gtk_use_system_tooltips_is_an_alias_onto_the_defvar_bool() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list (indirect-variable 'x-gtk-use-system-tooltips)
      (eq (symbol-value 'x-gtk-use-system-tooltips) use-system-tooltips)
      (progn (setq x-gtk-use-system-tooltips 5)
             (list x-gtk-use-system-tooltips use-system-tooltips))
      (progn (setq use-system-tooltips nil)
             (list x-gtk-use-system-tooltips use-system-tooltips))
      (let ((x-gtk-use-system-tooltips 7)) (list use-system-tooltips))
      (progn (setq x-gtk-use-system-tooltips t) use-system-tooltips))"#;
    let expect = expect_test::expect![[r#""OK (use-system-tooltips t (t t) (nil nil) (t) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// A variable no build reachable from here declares has no documentation
/// either, because in GNU the `DEFVAR_*` that binds the symbol is the same
/// statement that attaches its doc string.
///
/// Neomacs generates `var_docs::gnu_table` from ALL of GNU's `src/*.c`, the
/// MS-DOS and ImageMagick files included, so it answered a doc string for
/// `dos-hyper-key` while `boundp` correctly answered nil -- documentation for
/// a variable that does not exist.
#[test]
fn oracle_platform_names_no_build_declares_have_no_documentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar (lambda (s)
          (list s (boundp s)
                (and (documentation-property s 'variable-documentation) t)))
        '(dos-hyper-key dos-keypad-mode dos-unsupported-char-glyph
          imagemagick-render-type ns-antialias-text
          haiku-use-system-tooltips w32-follow-system-dark-mode))"#;
    let expect = expect_test::expect![[
        r#""OK ((dos-hyper-key nil nil) (dos-keypad-mode nil nil) (dos-unsupported-char-glyph nil nil) (imagemagick-render-type nil nil) (ns-antialias-text nil nil) (haiku-use-system-tooltips nil nil) (w32-follow-system-dark-mode nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The `x-` defaults are read back by real code paths, so pin the shapes those
/// paths depend on rather than only the printed form: a list of directories, a
/// float `NUMBERP` accepts, and a cons whose members are the selection symbols
/// `x_should_preserve_selection` compares with `EQ`.
///
/// `makunbound` is deliberately NOT probed here.  GNU refuses to unbind any
/// `DEFVAR_*` variable -- `Fset (symbol, Qunbound)` (`src/data.c:788`) reaches
/// `set_internal`'s `SYMBOL_FORWARDED` arm, which answers
/// `error ("Built-in variable may not be unbound : %s")`
/// (`src/data.c:1805-1808`) -- and Neomacs refuses only for the two forward
/// types entries 132 and 135 wired, `Lisp_Fwd_Int` and `Lisp_Fwd_Bool`.  Every
/// `Lisp_Fwd_Obj` variable here can still be unbound, these five included and
/// `global-mode-string`, `standard-display-table` and `glyph-table` alongside
/// them.  That is the standing unwired-`Lisp_Fwd_Obj` residual 132, 135 and
/// 138 each recorded, unchanged in either direction by entry 141, and it wants
/// its own entry rather than a pin that would fail for a reason this file is
/// not about.
#[test]
fn oracle_x_platform_defaults_have_gnus_types_not_just_gnus_printed_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list (and (listp x-bitmap-file-path)
           (> (length x-bitmap-file-path) 0)
           (not (memq nil (mapcar #'stringp x-bitmap-file-path))))
      (floatp x-scroll-event-delta-factor)
      (and (consp x-auto-preserve-selections)
           (and (memq 'CLIPBOARD x-auto-preserve-selections) t)
           (and (memq 'PRIMARY x-auto-preserve-selections) t))
      (stringp vertical-centering-font-regexp)
      ;; A `let' restores the declaration's value rather than leaving the
      ;; probe's, which a lexical binding could not have done in either
      ;; direction.
      (let ((x-auto-preserve-selections '(SECONDARY)))
        (car x-auto-preserve-selections))
      (car x-auto-preserve-selections))"#;
    let expect = expect_test::expect![[r#""OK (t t t t SECONDARY CLIPBOARD)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
