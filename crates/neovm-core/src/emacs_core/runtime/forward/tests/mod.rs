//! GNU parity tests for the `Lisp_Fwd` family's assignment rules.
//!
//! Every expectation here was produced by running the same form under GNU
//! Emacs 31.0.90 (`emacs -Q --batch`), never derived from the C source.

use crate::emacs_core::error::format_eval_result;
use crate::emacs_core::eval::Context;

fn ev() -> Context {
    crate::test_utils::init_test_tracing();
    Context::new()
}

/// `with-temp-buffer` / `setq-local` / `setq-default` are Lisp macros that a
/// bare [`Context`] has not loaded, so the probes spell them with the special
/// forms and subrs they expand to.
fn in_fresh_buffer(body: &str) -> String {
    format!(
        "(save-current-buffer
           (set-buffer (get-buffer-create \"fwd132\"))
           (prog1 (progn {body}) (kill-buffer \"fwd132\")))"
    )
}

/// GNU `store_symval_forwarding`'s `Lisp_Fwd_Int` arm (`src/data.c:1475-1483`)
/// runs `CHECK_INTEGER` before the store, so a `DEFVAR_INT` variable can never
/// hold a string, a float, `nil` or `t`.  Measured under GNU:
///
/// ```elisp
/// (setq undo-limit "x")   ;; => (wrong-type-argument integerp "x")
/// undo-limit              ;; => 160000
/// ```
#[test]
fn defvar_int_setq_signals_wrong_type_like_gnu() {
    let mut eval = ev();

    for (form, expected) in [
        (
            r#"(condition-case e (setq undo-limit "x") (error e))"#,
            r#"OK (wrong-type-argument integerp "x")"#,
        ),
        (
            r#"(condition-case e (setq gc-cons-threshold "x") (error e))"#,
            r#"OK (wrong-type-argument integerp "x")"#,
        ),
        (
            "(condition-case e (setq gc-cons-threshold 1.5) (error e))",
            "OK (wrong-type-argument integerp 1.5)",
        ),
        (
            "(condition-case e (setq gc-cons-threshold nil) (error e))",
            "OK (wrong-type-argument integerp nil)",
        ),
        (
            "(condition-case e (setq gc-cons-threshold t) (error e))",
            "OK (wrong-type-argument integerp t)",
        ),
        (
            "(condition-case e (setq undo-strong-limit \"x\") (error e))",
            r#"OK (wrong-type-argument integerp "x")"#,
        ),
    ] {
        assert_eq!(format_eval_result(&eval.eval_str(form)), expected, "{form}");
    }

    // The refused write leaves the old value in place, exactly as GNU's
    // longjmp out of `store_symval_forwarding` does.
    assert_eq!(
        format_eval_result(&eval.eval_str("undo-limit")),
        "OK 160000"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("gc-cons-threshold")),
        "OK 800000"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("undo-strong-limit")),
        "OK 240000"
    );
}

/// Integers still go through, including a bignum inside `intmax_t` range.
/// Measured under GNU: `(setq gc-cons-threshold (* most-positive-fixnum 4))`
/// => 9223372036854775804, and reading it back returns the same bignum.
#[test]
fn defvar_int_accepts_every_integer_intmax_can_hold_like_gnu() {
    let mut eval = ev();

    assert_eq!(
        format_eval_result(&eval.eval_str("(setq gc-cons-threshold 777777)")),
        "OK 777777"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("gc-cons-threshold")),
        "OK 777777"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("(setq gc-cons-threshold -1)")),
        "OK -1"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("gc-cons-threshold")),
        "OK -1"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("(setq gc-cons-threshold most-positive-fixnum)")),
        "OK 2305843009213693951"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("(setq gc-cons-threshold (* most-positive-fixnum 4))")),
        "OK 9223372036854775804"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("gc-cons-threshold")),
        "OK 9223372036854775804"
    );
}

/// GNU's integer arm signals `overflow-error` -- not `wrong-type-argument` --
/// for an integer too large for the `intmax_t` slot (`src/data.c:1479-1480`).
/// Measured under GNU: `(setq gc-cons-threshold (expt 2 200))` =>
/// `(overflow-error 1606938044258990275541962092341162602522202993782792835301376)`
/// and `gc-cons-threshold` is left at its previous value.
#[test]
fn defvar_int_signals_overflow_error_past_intmax_like_gnu() {
    let mut eval = ev();

    assert_eq!(
        format_eval_result(
            &eval.eval_str("(condition-case e (setq gc-cons-threshold (expt 2 200)) (error e))")
        ),
        "OK (overflow-error 1606938044258990275541962092341162602522202993782792835301376)"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("gc-cons-threshold")),
        "OK 800000"
    );
}

/// GNU checks the integer arm below `set_internal`, so every assignment
/// spelling reaches it: `set`, `set-default`, `setq-default`, a `let` binding,
/// and `make-local-variable` + `set`.  All five measured under GNU as
/// `(wrong-type-argument integerp "x")`.
#[test]
fn defvar_int_check_covers_every_assignment_spelling_like_gnu() {
    let mut eval = ev();

    let local_set = in_fresh_buffer(r#"(set (make-local-variable 'undo-limit) "x")"#);
    for form in [
        r#"(condition-case e (set 'undo-limit "x") (error e))"#.to_string(),
        r#"(condition-case e (set-default 'undo-limit "x") (error e))"#.to_string(),
        r#"(condition-case e (let ((undo-limit "x")) undo-limit) (error e))"#.to_string(),
        format!("(condition-case e {local_set} (error e))"),
    ] {
        assert_eq!(
            format_eval_result(&eval.eval_str(&form)),
            r#"OK (wrong-type-argument integerp "x")"#,
            "{form}"
        );
    }

    assert_eq!(
        format_eval_result(&eval.eval_str("undo-limit")),
        "OK 160000"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("(default-value 'undo-limit)")),
        "OK 160000"
    );
}

/// A per-buffer binding of a `DEFVAR_INT` variable is still an integer slot.
/// Measured under GNU:
/// `(with-temp-buffer (setq-local undo-limit 5) (list undo-limit (default-value 'undo-limit)))`
/// => `(5 160000)`.
#[test]
fn defvar_int_buffer_local_binding_keeps_the_default_like_gnu() {
    let mut eval = ev();

    let form = in_fresh_buffer(
        "(progn (set (make-local-variable 'undo-limit) 5)
                (list undo-limit (default-value 'undo-limit)))",
    );
    assert_eq!(format_eval_result(&eval.eval_str(&form)), "OK (5 160000)");
    assert_eq!(
        format_eval_result(&eval.eval_str("undo-limit")),
        "OK 160000"
    );
}

/// A forwarded slot has no "unbound" bit pattern, so GNU refuses to create
/// one: `error ("Built-in variable may not be unbound : %s")`
/// (`src/data.c:1725-1728` and `:1805-1807`).  Measured under GNU:
///
/// ```elisp
/// (makunbound 'gc-cons-threshold)
/// ;; => (error "Built-in variable may not be unbound : gc-cons-threshold")
/// (boundp 'gc-cons-threshold)  ;; => t
/// ```
#[test]
fn forwarded_variables_refuse_makunbound_like_gnu() {
    let mut eval = ev();

    assert_eq!(
        format_eval_result(
            &eval.eval_str("(condition-case e (makunbound 'gc-cons-threshold) (error e))")
        ),
        r#"OK (error "Built-in variable may not be unbound : gc-cons-threshold")"#
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("(boundp 'gc-cons-threshold)")),
        "OK t"
    );
    assert_eq!(
        format_eval_result(
            &eval.eval_str("(condition-case e (makunbound 'inhibit-message) (error e))")
        ),
        r#"OK (error "Built-in variable may not be unbound : inhibit-message")"#
    );
}

/// GNU's `Lisp_Fwd_Bool` arm does NOT signal -- it coerces, storing
/// `!NILP (newval)` (`src/data.c:1485-1487`).  `setq` still returns the value
/// it was given; only the variable is canonical.  Measured under GNU:
///
/// ```elisp
/// (setq inhibit-message 5)   ;; => 5
/// inhibit-message            ;; => t
/// ```
#[test]
fn defvar_bool_coerces_instead_of_signalling_like_gnu() {
    let mut eval = ev();

    assert_eq!(
        format_eval_result(&eval.eval_str("(setq inhibit-message 5)")),
        "OK 5"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("inhibit-message")),
        "OK t"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str(r#"(progn (setq inhibit-message "s") inhibit-message)"#)),
        "OK t"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("(progn (setq inhibit-message nil) inhibit-message)")),
        "OK nil"
    );
}

/// The Boolean coercion is a property of the forwarder, so it survives every
/// assignment spelling too.  `(set 'inhibit-message 9)` returns 9 but leaves
/// `t` behind, and `set-default` / `setq-default` do the same -- all measured
/// under GNU.
#[test]
fn defvar_bool_coercion_covers_every_assignment_spelling_like_gnu() {
    let mut eval = ev();

    assert_eq!(
        format_eval_result(&eval.eval_str("(progn (set 'inhibit-message 9) inhibit-message)")),
        "OK t"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str(
            "(progn (setq inhibit-message nil)
                    (set-default 'inhibit-message 9)
                    (default-value 'inhibit-message))"
        )),
        "OK t"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("(let ((inhibit-message 5)) inhibit-message)")),
        "OK t"
    );
}

/// A per-buffer binding of a `DEFVAR_BOOL` variable is a Boolean slot too.
/// Measured under GNU:
/// `(with-temp-buffer (setq-local inhibit-message 3) (list inhibit-message
///  (default-value 'inhibit-message)))` => `(t nil)`.
///
/// And making one buffer's binding must not disarm the forwarder for the
/// global cell: measured under GNU, a later `(setq inhibit-message 7)` still
/// reads back `t`.
#[test]
fn defvar_bool_survives_make_local_variable_like_gnu() {
    let mut eval = ev();

    let form = in_fresh_buffer(
        "(progn (set (make-local-variable 'inhibit-message) 3)
                (list inhibit-message (default-value 'inhibit-message)))",
    );
    assert_eq!(format_eval_result(&eval.eval_str(&form)), "OK (t nil)");
    assert_eq!(
        format_eval_result(&eval.eval_str("(progn (setq inhibit-message 7) inhibit-message)")),
        "OK t"
    );
}

/// Registering a `DEFVAR_BOOL` variable also puts its symbol on
/// `byte-boolean-vars` -- GNU does it inside `defvar_bool` itself
/// (`src/lread.c:5261`).  The byte optimizer reads that list before folding a
/// `varset X; varref X` pair back into the stored value, because "what we put
/// in might not be what we get out"
/// (`lisp/emacs-lisp/byte-opt.el:2285-2300`).  Measured under GNU 31.0.90:
/// `(memq 'inhibit-message byte-boolean-vars)` is non-nil, and
/// `(special-variable-p 'byte-boolean-vars)` is t.
#[test]
fn defvar_bool_registration_lists_the_symbol_in_byte_boolean_vars_like_gnu() {
    let mut eval = ev();

    assert_eq!(
        format_eval_result(&eval.eval_str("(and (memq 'inhibit-message byte-boolean-vars) t)")),
        "OK t"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("(special-variable-p 'byte-boolean-vars)")),
        "OK t"
    );
}

/// `defvar_bool` conses the symbol onto `byte-boolean-vars`
/// (`src/lread.c:5261`), but `syms_of_lread` then writes
/// `Vbyte_boolean_vars = Qnil` (`src/lread.c:5774`), which throws away every
/// cons `main` had made before it got there.  Measured under GNU 31.0.90,
/// `emacs -Q --batch`:
///
/// ```elisp
/// (length byte-boolean-vars)                          ;; => 117
/// (and (memq 'visible-bell byte-boolean-vars) t)      ;; => t   (dispnew.c, after)
/// (and (memq 'use-short-answers byte-boolean-vars) t) ;; => nil (fns.c, before)
/// ```
#[test]
fn byte_boolean_vars_holds_gnus_117_and_not_the_31_erased_ones() {
    let mut eval = ev();

    assert_eq!(
        format_eval_result(&eval.eval_str("(length byte-boolean-vars)")),
        "OK 117"
    );
    for name in [
        "visible-bell",
        "inhibit-message",
        "indent-tabs-mode",
        "print-quoted",
        "noninteractive",
        "font-use-system-font",
        "load-dangerous-libraries",
    ] {
        assert_eq!(
            format_eval_result(
                &eval.eval_str(&format!("(and (memq '{name} byte-boolean-vars) t)"))
            ),
            "OK t",
            "{name} should be on byte-boolean-vars"
        );
    }
    for name in [
        "use-short-answers",
        "use-dialog-box",
        "garbage-collection-messages",
        "symbols-with-pos-enabled",
        "load-in-progress",
        "load-force-doc-strings",
        "write-region-inhibit-fsync",
        "inhibit-eol-conversion",
    ] {
        assert_eq!(
            format_eval_result(
                &eval.eval_str(&format!("(and (memq '{name} byte-boolean-vars) t)"))
            ),
            "OK nil",
            "{name} is erased by syms_of_lread's own initializer"
        );
    }
}

/// The list is in reverse declaration order because `defvar_bool` prepends.
/// Measured under GNU 31.0.90: `(car byte-boolean-vars)` is the last
/// `DEFVAR_BOOL` `main` reaches (`xsettings.c`) and `(nth 116 ...)` the first
/// one after `syms_of_lread` cleared the list (`lread.c`, immediately below
/// the `DEFVAR_LISP` for the list itself).
#[test]
fn byte_boolean_vars_is_in_gnus_reverse_declaration_order() {
    let mut eval = ev();

    assert_eq!(
        format_eval_result(&eval.eval_str("(car byte-boolean-vars)")),
        "OK font-use-system-font"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("(nth 116 byte-boolean-vars)")),
        "OK load-dangerous-libraries"
    );
}

/// `store_symval_forwarding`'s `Lisp_Fwd_Bool` arm never signals -- it is
/// `*XBOOLVAR (valcontents) = !NILP (newval);` (`src/data.c:1485-1487`), so
/// `setq` returns what it was handed and the next read is `t` or `nil`.  The
/// coercion is a property of the declaration, not of the list: it applies to
/// the 31 variables `byte-boolean-vars` does not mention too.  Measured under
/// GNU 31.0.90:
///
/// ```elisp
/// (list (setq visible-bell 5) visible-bell)           ;; => (5 t)
/// (list (setq use-short-answers 5) use-short-answers) ;; => (5 t)
/// (list (setq create-lockfiles nil) create-lockfiles) ;; => (nil nil)
/// (list (setq print-quoted (list 1)) print-quoted)    ;; => ((1) t)
/// ```
#[test]
fn defvar_bool_coerces_every_variable_in_the_table_like_gnu() {
    let mut eval = ev();

    for (form, expected) in [
        ("(list (setq visible-bell 5) visible-bell)", "OK (5 t)"),
        (
            "(list (setq use-short-answers 5) use-short-answers)",
            "OK (5 t)",
        ),
        (
            "(list (setq create-lockfiles nil) create-lockfiles)",
            "OK (nil nil)",
        ),
        (
            "(list (setq print-quoted (list 1)) print-quoted)",
            "OK ((1) t)",
        ),
    ] {
        assert_eq!(format_eval_result(&eval.eval_str(form)), expected, "{form}");
    }
}

/// Every row of the table is registered, is `special`, and still holds the
/// value the table gives it once the rest of the bootstrap has run -- which is
/// what stops a leftover plain-cell seed elsewhere from quietly deciding a
/// `DEFVAR_BOOL` variable's default.
#[test]
fn every_gnu_defvar_bool_variable_is_bound_and_reads_back_canonically() {
    use crate::emacs_core::defvar_bool::GNU_BOOL_VARIABLES;
    let mut eval = ev();

    for var in GNU_BOOL_VARIABLES {
        let name = var.name;
        assert_eq!(
            format_eval_result(&eval.eval_str(&format!("(boundp '{name})"))),
            "OK t",
            "{name} should be bound"
        );
        assert_eq!(
            format_eval_result(&eval.eval_str(&format!("(special-variable-p '{name})"))),
            "OK t",
            "{name} should be special"
        );
        assert_eq!(
            format_eval_result(&eval.eval_str(&format!("(default-value '{name})"))),
            if var.initial { "OK t" } else { "OK nil" },
            "{name} default"
        );
    }
}

/// The coercion has to survive `let`, `set-default` and a buffer-local
/// binding, because `do_specbind` and `set_default_internal` both route a
/// forwarded symbol through `store_symval_forwarding` (`src/eval.c:3594-3622`,
/// `src/data.c:2077`).  Measured under GNU 31.0.90:
///
/// ```elisp
/// (let ((inverse-video 3)) inverse-video)                   ;; => t
/// (progn (set-default 'inverse-video 9)
///        (default-value 'inverse-video))                    ;; => t
/// (with-temp-buffer (setq-local indent-tabs-mode 4)
///                   indent-tabs-mode)                       ;; => t
/// ```
#[test]
fn defvar_bool_coercion_survives_let_set_default_and_buffer_local_like_gnu() {
    let mut eval = ev();

    assert_eq!(
        format_eval_result(&eval.eval_str("(let ((inverse-video 3)) inverse-video)")),
        "OK t"
    );
    assert_eq!(
        format_eval_result(
            &eval.eval_str("(progn (set-default 'inverse-video 9) (default-value 'inverse-video))")
        ),
        "OK t"
    );
    let form =
        in_fresh_buffer("(progn (set (make-local-variable 'indent-tabs-mode) 4) indent-tabs-mode)");
    assert_eq!(format_eval_result(&eval.eval_str(&form)), "OK t");
}

/// `display-line-numbers-offset` is GNU's one variable that is `DEFVAR_INT`
/// AND `Fmake_variable_buffer_local`, in that order (`src/xdisp.c:38999-39005`).
/// `make_blv` copies the descriptor into the BLV (`src/data.c:2112-2140`), so
/// the integer rule has to survive into the per-buffer binding.  Measured under
/// GNU 31.0.90, `-Q --batch`:
///
/// ```elisp
/// (local-variable-if-set-p 'display-line-numbers-offset)     ;; => t
/// (set-default 'display-line-numbers-offset "x")             ;; => wrong-type-argument
/// (with-temp-buffer (setq-local display-line-numbers-offset 3)
///                   (list display-line-numbers-offset
///                         (default-value 'display-line-numbers-offset)))  ;; => (3 0)
/// ```
#[test]
fn buffer_local_defvar_int_keeps_its_type_rule_like_gnu() {
    use crate::emacs_core::intern::intern;
    let mut eval = ev();

    assert_eq!(
        format_eval_result(&eval.eval_str("(default-value 'display-line-numbers-offset)")),
        "OK 0"
    );
    assert!(
        eval.obarray()
            .blv(intern("display-line-numbers-offset"))
            .is_some_and(|blv| blv.local_if_set && blv.fwd.is_some()),
        "the BLV must carry the descriptor `make_blv' copied into it"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str(
            "(condition-case e (set-default 'display-line-numbers-offset \"x\") (error (car e)))"
        )),
        "OK wrong-type-argument"
    );
    let form = in_fresh_buffer(
        "(progn (set (make-local-variable 'display-line-numbers-offset) 3)
                (list display-line-numbers-offset
                      (default-value 'display-line-numbers-offset)))",
    );
    assert_eq!(format_eval_result(&eval.eval_str(&form)), "OK (3 0)");
    let refuse = in_fresh_buffer(
        "(condition-case e
             (set (make-local-variable 'display-line-numbers-offset) \"x\")
           (error (car e)))",
    );
    assert_eq!(
        format_eval_result(&eval.eval_str(&refuse)),
        "OK wrong-type-argument"
    );

    // GNU's other `DEFVAR_INT' + `Fmake_variable_buffer_local' pair
    // (`src/syntax.c:3773-3778').  Measured under GNU:
    // `(set-default 'syntax-propertize--done "x")' => wrong-type-argument,
    // `(default-value 'syntax-propertize--done)' => -1.
    assert!(
        eval.obarray()
            .blv(intern("syntax-propertize--done"))
            .is_some_and(|blv| blv.local_if_set && blv.fwd.is_some())
    );
    assert_eq!(
        format_eval_result(&eval.eval_str(
            "(condition-case e (set-default 'syntax-propertize--done \"x\") (error (car e)))"
        )),
        "OK wrong-type-argument"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("(default-value 'syntax-propertize--done)")),
        "OK -1"
    );
}

/// The eight `DEFVAR_INT` variables entry 132 recorded as having no Neomacs
/// declaration at all.  Values measured under GNU 31.0.90, `-Q --batch`;
/// `command-line-max-length` and `strings-consed` are asserted by shape because
/// one is `sysconf (_SC_ARG_MAX) / 4` and the other is a live counter.
#[test]
fn the_remaining_gnu_defvar_int_variables_are_declared_with_gnus_values() {
    let mut eval = ev();

    for (name, expected) in [
        ("large-hscroll-threshold", "OK 10000"),
        ("long-line-optimizations-bol-search-limit", "OK 128"),
        ("long-line-optimizations-region-size", "OK 500000"),
        ("max-redisplay-ticks", "OK 0"),
        ("x-color-cache-bucket-size", "OK 128"),
        ("x-mouse-click-focus-ignore-time", "OK 200"),
    ] {
        assert_eq!(format_eval_result(&eval.eval_str(name)), expected, "{name}");
    }
    assert_eq!(
        format_eval_result(
            &eval
                .eval_str("(and (integerp command-line-max-length) (> command-line-max-length 0))")
        ),
        "OK t"
    );
    assert_eq!(format_eval_result(&eval.eval_str("strings-consed")), "OK 0");
    // Each one is a real forwarder, not a plain cell that happens to hold an
    // integer: `(setq X "x")` is GNU's `wrong-type-argument`.
    for name in [
        "command-line-max-length",
        "large-hscroll-threshold",
        "long-line-optimizations-bol-search-limit",
        "long-line-optimizations-region-size",
        "max-redisplay-ticks",
        "strings-consed",
        "x-color-cache-bucket-size",
        "x-mouse-click-focus-ignore-time",
    ] {
        let form = format!("(condition-case e (setq {name} \"x\") (error (car e)))");
        assert_eq!(
            format_eval_result(&eval.eval_str(&form)),
            "OK wrong-type-argument",
            "{name}"
        );
    }
}

/// `baud-rate` is the only `DEFVAR_INT` GNU declares with no initializer
/// (`src/dispnew.c:7488`): the C global starts at 0 and only a terminal
/// initialization ever writes it.  A [`Context`] has no terminal, which is the
/// `--batch` case GNU reports 0 for.
#[test]
fn baud_rate_starts_at_the_c_globals_zero_like_gnu() {
    let mut eval = ev();

    assert_eq!(format_eval_result(&eval.eval_str("baud-rate")), "OK 0");
    assert_eq!(
        format_eval_result(
            &eval.eval_str("(condition-case e (setq baud-rate \"x\") (error (car e)))")
        ),
        "OK wrong-type-argument"
    );
    assert_eq!(format_eval_result(&eval.eval_str("baud-rate")), "OK 0");
}

/// The platform names `cus-start.el` mentions but this build does not declare.
/// GNU leaves all of them unbound on GNU/Linux, so binding them is invented
/// existence -- `(boundp 'dos-hyper-key)` must answer `nil`.
#[test]
fn platform_variables_gnu_leaves_unbound_here_are_not_bound() {
    let mut eval = ev();

    for name in [
        "dos-hyper-key",
        "dos-super-key",
        "dos-keypad-mode",
        "imagemagick-render-type",
        "xwidget-internal",
        "ns-antialias-text",
        "w32-follow-system-dark-mode",
        "haiku-use-system-tooltips",
    ] {
        assert_eq!(
            format_eval_result(&eval.eval_str(&format!("(boundp '{name})"))),
            "OK nil",
            "{name} is unbound under GNU on this platform"
        );
    }
    // The `cus-start.el` platform names GNU DOES bind here stay bound.
    for name in [
        "window-combination-limit",
        "void-text-area-pointer",
        "vertical-centering-font-regexp",
    ] {
        assert_eq!(
            format_eval_result(&eval.eval_str(&format!("(boundp '{name})"))),
            "OK t",
            "{name} is bound under GNU on this platform"
        );
    }
}

/// The whole `cus-start.el` platform table, walked row by row against a live
/// [`Context`].
///
/// The table is the record of two GNU measurements -- "does GNU bind this in a
/// build like this one" and "where is it declared here" -- and nothing else
/// enforces it, because the module deliberately seeds nothing.  This is that
/// enforcement: an `UnboundHere` row must not be bound, and a `DeclaredInC`
/// row must be bound AND special, since in GNU one `DEFVAR_LISP` produces both
/// and a row cannot claim a declaration site while behaving like a bare
/// obarray cell.  A `DeclaredInPreloadedLisp` row is deliberately absent from
/// a bare [`Context`] -- its declaration is a `defvaralias` in
/// `lisp/term/neo-preload.el`, which only loadup runs -- so this test asserts
/// that absence, and the oracle pins the post-loadup state.
#[test]
fn every_cus_start_platform_row_matches_its_declaration_claim() {
    use crate::emacs_core::cus_start_platform_vars::{CUS_START_PLATFORM_VARIABLES, GnuBinding};

    let mut eval = ev();

    for var in CUS_START_PLATFORM_VARIABLES {
        let bound = format_eval_result(&eval.eval_str(&format!("(boundp '{})", var.name)));
        match var.binding {
            GnuBinding::UnboundHere => {
                assert_eq!(bound, "OK nil", "{} ({})", var.name, var.gnu);
                // Nothing declares it, so nothing documents it either.
                assert_eq!(
                    format_eval_result(&eval.eval_str(&format!(
                        "(documentation-property '{} 'variable-documentation)",
                        var.name
                    ))),
                    "OK nil",
                    "{} has documentation but no declaration",
                    var.name
                );
            }
            GnuBinding::DeclaredInC { site } => {
                assert_eq!(
                    bound, "OK t",
                    "{} ({}) declared at {site}",
                    var.name, var.gnu
                );
                assert_eq!(
                    format_eval_result(
                        &eval.eval_str(&format!("(special-variable-p '{})", var.name))
                    ),
                    "OK t",
                    "{} is bound but not special; {site} owes it a declaration",
                    var.name
                );
            }
            GnuBinding::DeclaredInPreloadedLisp { site } => {
                assert_eq!(
                    bound, "OK nil",
                    "{} ({}) is declared by {site}, which loadup runs, so a bare \
                     Context must not have it",
                    var.name, var.gnu
                );
            }
        }
    }
}

/// The five platform names entry 141 gave a declaration to, measured under GNU
/// Emacs 31.0.90, `-Q --batch`:
///
/// ```elisp
/// x-bitmap-file-path             ;; => ("/usr/include/X11/bitmaps")
/// x-scroll-event-delta-factor    ;; => 1.0
/// x-auto-preserve-selections     ;; => (CLIPBOARD PRIMARY)
/// ```
///
/// `vertical-centering-font-regexp` and `x-gtk-use-system-tooltips` get their
/// values from preloaded Lisp (`international/fontset.el:1266` and the
/// `defvaralias` in `term/neo-preload.el`), so a bare [`Context`] sees the C
/// initializers instead and they are checked in the oracle rather than here.
#[test]
fn the_x_platform_variables_hold_gnus_defaults_and_bind_dynamically() {
    let mut eval = ev();

    for (name, value) in [
        ("x-bitmap-file-path", r#"("/usr/include/X11/bitmaps")"#),
        ("x-scroll-event-delta-factor", "1.0"),
        ("x-auto-preserve-selections", "(CLIPBOARD PRIMARY)"),
    ] {
        assert_eq!(
            format_eval_result(&eval.eval_str(name)),
            format!("OK {value}"),
            "{name}"
        );
        // The declaration is what makes `let' dynamic: a plain obarray cell
        // gets a lexical binding under lexical-binding and `symbol-value'
        // inside the `let' still answers the global.
        assert_eq!(
            format_eval_result(
                &eval.eval_str(&format!("(let (({name} 'probe)) (symbol-value '{name}))"))
            ),
            "OK probe",
            "{name} binds lexically, so it has no declaration"
        );
        assert_eq!(
            format_eval_result(&eval.eval_str(name)),
            format!("OK {value}"),
            "{name} did not restore after the let"
        );
    }
}

// ===========================================================================
// Ledger 170 -- `DEFVAR_LISP` / `DEFVAR_KBOARD` are SYMBOL_FORWARDED too
// ===========================================================================

/// GNU's `set_internal` refuses an unbind for any symbol whose redirect is
/// `SYMBOL_FORWARDED` -- the arm reads the forwarder pointer and then signals
/// without looking at what it points to (`src/data.c:1802-1809`).  So the
/// refusal is a property of the DECLARATION, and `DEFVAR_LISP` names are
/// refused exactly like `DEFVAR_INT` ones.  Measured under GNU Emacs 31.0.90
/// `-Q --batch`:
///
/// ```elisp
/// (condition-case e (makunbound 'after-load-alist) (error e))
/// ;; => (error "Built-in variable may not be unbound : after-load-alist")
/// ```
///
/// Swept over all 563 `DEFVAR_LISP` names plus the 14 `DEFVAR_KBOARD` ones,
/// GNU refuses 487 + 14 and allows only the 3 it does not declare in this
/// build (ledger 168, re-derived by ledger 170).
#[test]
fn defvar_lisp_makunbound_is_refused_like_gnu() {
    let mut eval = ev();

    // Names a bare [`Context`] already binds -- the sweep over all 578 GNU
    // declarations lives in the oracle, where preloaded Lisp has run.
    for name in [
        "after-load-alist",
        "command-line-args",
        "features",
        "load-path",
        "obarray",
        "purify-flag",
        "standard-output",
    ] {
        assert_eq!(
            format_eval_result(&eval.eval_str(&format!(
                "(condition-case e (makunbound '{name}) (error e))"
            ))),
            format!(r#"OK (error "Built-in variable may not be unbound : {name}")"#),
            "{name}"
        );
        assert_eq!(
            format_eval_result(&eval.eval_str(&format!("(boundp '{name})"))),
            "OK t",
            "{name} lost its binding"
        );
    }
}

/// The same refusal, reached through GNU's OTHER arm: once Lisp has localized
/// the variable the symbol is `SYMBOL_LOCALIZED`, and `set_internal` signals
/// from `if (unbinding_p && blv->fwd)` instead (`src/data.c:1723-1727`).
/// Measured under GNU:
///
/// ```elisp
/// (progn (make-local-variable 'after-load-alist)
///        (condition-case e (makunbound 'after-load-alist) (error e)))
/// ;; => (error "Built-in variable may not be unbound : after-load-alist")
/// ```
#[test]
fn defvar_lisp_makunbound_is_refused_through_a_buffer_local_binding_like_gnu() {
    let mut eval = ev();

    for form in [
        "(progn (make-local-variable 'after-load-alist)
                (condition-case e (makunbound 'after-load-alist) (error e)))",
        "(progn (make-variable-buffer-local 'after-load-alist)
                (condition-case e (makunbound 'after-load-alist) (error e)))",
        "(let ((after-load-alist nil))
           (condition-case e (makunbound 'after-load-alist) (error e)))",
    ] {
        assert_eq!(
            format_eval_result(&eval.eval_str(form)),
            r#"OK (error "Built-in variable may not be unbound : after-load-alist")"#,
            "{form}"
        );
    }
}

/// `Fdefvaralias` switches on the same tag and refuses a forwarded NEW-ALIAS
/// with a different message (`src/eval.c:665-668`).  Measured under GNU:
///
/// ```elisp
/// (condition-case e (defvaralias 'after-load-alist 'l170b) (error e))
/// ;; => (error "Cannot make a built-in variable an alias: after-load-alist")
/// ```
#[test]
fn defvar_lisp_cannot_become_a_variable_alias_like_gnu() {
    let mut eval = ev();

    assert_eq!(
        format_eval_result(&eval.eval_str(
            "(progn (defvar l170b 7)
                    (condition-case e (defvaralias 'after-load-alist 'l170b) (error e)))"
        )),
        r#"OK (error "Cannot make a built-in variable an alias: after-load-alist")"#
    );
}

/// A `DEFVAR_KBOARD` variable is `SYMBOL_FORWARDED` with a
/// `Lisp_Kboard_Objfwd`, and both `Fmake_variable_buffer_local`
/// (`src/data.c:2220-2223`) and `Fmake_local_variable` (`src/data.c:2287-2290`)
/// refuse it by name.  Measured under GNU:
///
/// ```elisp
/// (condition-case e (make-local-variable 'prefix-arg) (error e))
/// ;; => (error "Symbol prefix-arg may not be buffer-local")
/// (condition-case e (makunbound 'prefix-arg) (error e))
/// ;; => (error "Built-in variable may not be unbound : prefix-arg")
/// ```
#[test]
fn defvar_kboard_is_forwarded_like_gnu() {
    let mut eval = ev();

    for name in ["prefix-arg", "last-command", "real-last-command"] {
        assert_eq!(
            format_eval_result(&eval.eval_str(&format!(
                "(condition-case e (makunbound '{name}) (error e))"
            ))),
            format!(r#"OK (error "Built-in variable may not be unbound : {name}")"#),
            "{name}"
        );
        assert_eq!(
            format_eval_result(&eval.eval_str(&format!(
                "(condition-case e (make-local-variable '{name}) (error e))"
            ))),
            format!(r#"OK (error "Symbol {name} may not be buffer-local")"#),
            "{name}"
        );
        assert_eq!(
            format_eval_result(&eval.eval_str(&format!(
                "(condition-case e (make-variable-buffer-local '{name}) (error e))"
            ))),
            format!(r#"OK (error "Symbol {name} may not be buffer-local")"#),
            "{name}"
        );
    }
}

/// The other side of the same tag: `Fmake_local_variable`'s SYMBOL_FORWARDED
/// arm answers `Fframe_terminal (selected_frame)` for a `Lisp_Kboard_Objfwd`
/// (`src/data.c:2519-2521`), which is what `variable-binding-locus` returns.
/// A `DEFVAR_LISP` variable with no buffer-local binding answers nil from the
/// PLAINVAL/FORWARDED fall-through.  Measured under GNU Emacs 31.0.90
/// `-Q --batch`:
///
/// ```elisp
/// (list (type-of (variable-binding-locus 'prefix-arg))
///       (type-of (variable-binding-locus 'last-command))
///       (variable-binding-locus 'after-load-alist))
/// ;; => (terminal terminal nil)
/// ```
///
/// Pinned as `type-of` rather than the printed form because the terminal's
/// name is a property of the display this build opened, not of the tag.
#[test]
fn defvar_kboard_binding_locus_is_the_terminal_like_gnu() {
    let mut eval = ev();

    for name in ["prefix-arg", "last-command", "real-last-command"] {
        assert_eq!(
            format_eval_result(
                &eval.eval_str(&format!("(type-of (variable-binding-locus '{name}))"))
            ),
            "OK terminal",
            "{name}"
        );
    }
    assert_eq!(
        format_eval_result(&eval.eval_str("(variable-binding-locus 'after-load-alist)")),
        "OK nil"
    );
}

// ===========================================================================
// Ledger 183 -- ledger 170's fifth refusal: a LET-BOUND new alias
// ===========================================================================

/// GNU scans the whole specpdl for any `kind >= SPECPDL_LET` binding of
/// NEW-ALIAS and refuses (`src/eval.c:704-711`).  Measured under GNU Emacs
/// 31.0.90 `-Q --batch` (`tmp/l183-p6.el`), four rows:
///
/// ```text
/// 1 plain-let      (refused "Don’t know how to make a let-bound variable an alias: l183p")
/// 2 unwound-let    allowed
/// 3 base-let-bound allowed
/// 4 let-local      (refused "Don’t know how to make a buffer-local variable an alias: l183v")
/// ```
///
/// Row 2 is what makes this a specpdl question and not a symbol-flag one: the
/// same symbol, once the binding has been unwound, is accepted.  Row 3 is what
/// makes it a question about NEW-ALIAS only.  Row 4 is the pre-existing
/// LOCALIZED refusal reached first, and both editors already agreed on it.
#[test]
fn a_let_bound_variable_cannot_become_an_alias_like_gnu() {
    let mut eval = ev();
    eval.eval_str(
        "(progn (defvar l183p 1) (defvar l183q 2)
                (defvar l183r 3) (defvar l183s 4)
                (defvar l183t 5) (defvar l183u 6) nil)",
    )
    .expect("setup should evaluate");

    // 1. NEW-ALIAS is let-bound right now.
    assert_eq!(
        format_eval_result(&eval.eval_str(
            "(let ((l183p 9))
               (condition-case e (defvaralias 'l183p 'l183q) (error e)))"
        )),
        "OK (error \"Don\u{2019}t know how to make a let-bound variable an alias: l183p\")"
    );
    // 2. the binding is gone again: accepted.
    assert_eq!(
        format_eval_result(&eval.eval_str(
            "(progn (let ((l183r 9)) nil)
                    (condition-case e (defvaralias 'l183r 'l183s) (error e)))"
        )),
        "OK l183s"
    );
    // 3. BASE let-bound, NEW-ALIAS not: accepted.
    assert_eq!(
        format_eval_result(&eval.eval_str(
            "(let ((l183u 9))
               (condition-case e (defvaralias 'l183t 'l183u) (error e)))"
        )),
        "OK l183u"
    );
}

/// GNU performs the refusal AFTER the value migration and the "Overwriting
/// value" warning, not with the redirect switch (`src/eval.c:682-711`) -- the
/// warning for row 1 of `tmp/l183-p6.el` is printed by the run that then
/// refuses.  So the refused call is not a no-op, and pinning the order is what
/// stops a future reader from folding the check into
/// `Obarray::check_variable_alias` where it would be cheaper and wrong.
#[test]
fn the_let_bound_refusal_happens_after_the_value_migration() {
    let mut eval = ev();
    // BASE unbound, NEW-ALIAS bound and let-bound: GNU's `if (NILP (Fboundp
    // (base_variable)))` arm copies NEW-ALIAS's *current* (let-bound) value
    // into BASE before the specpdl scan refuses.
    assert_eq!(
        format_eval_result(&eval.eval_str(
            "(progn (defvar l183x 1)
                    (let ((l183x 9))
                      (condition-case nil (defvaralias 'l183x 'l183y) (error nil)))
                    (list (boundp 'l183y) l183y))"
        )),
        "OK (t 9)"
    );
}

/// GNU's `DEFVAR_LISP ("echo-area-clear-hook", ...)` is parked inside a dead
/// preprocessor region (`src/keyboard.c:14057-14061`), so no build has ever
/// compiled it -- the name is an ordinary plain Lisp variable there.  The
/// generated declaration table scraped the head anyway and this port therefore
/// forwarded it, which costs two Lisp-visible facts.  Measured, `-Q --batch`:
///
/// ```text
/// (list (boundp 'echo-area-clear-hook)          GNU      (t nil nil)
///       (symbol-value 'echo-area-clear-hook)    before   (t nil t)
///       (special-variable-p 'echo-area-clear-hook))
/// (condition-case e (makunbound 'echo-area-clear-hook) (error e))
///                                               GNU      echo-area-clear-hook
///                                               before   (error "Built-in variable may not be unbound : echo-area-clear-hook")
/// ```
///
/// The fix is in `scripts/extract_gnu_defvar_object_names.py`, which now
/// blanks those regions before scanning; this pin is what says the table was
/// regenerated.  Ledger 183, same failure class as ledger 176's `features`.
#[test]
fn a_defvar_parked_in_a_dead_preprocessor_region_is_not_a_declaration() {
    assert!(
        !crate::emacs_core::defvar_object::gnu_table::GNU_OBJECT_VARIABLES
            .iter()
            .any(|var| matches!(
                var.name,
                "echo-area-clear-hook" | "w32-generate-fake-inodes"
            )),
        "a dead-region DEFVAR head is back in the generated table"
    );

    let mut eval = ev();
    assert_eq!(
        format_eval_result(&eval.eval_str(
            "(list (boundp 'echo-area-clear-hook)
                   (symbol-value 'echo-area-clear-hook)
                   (special-variable-p 'echo-area-clear-hook))"
        )),
        "OK (t nil nil)"
    );
    assert_eq!(
        format_eval_result(
            &eval.eval_str("(condition-case e (makunbound 'echo-area-clear-hook) (error e))")
        ),
        "OK echo-area-clear-hook"
    );
}

/// `default-minibuffer-frame` is `DEFVAR_KBOARD` (`src/frame.c:7555`), and
/// this port bound it only from the post-image reset table -- which runs after
/// `defvar_object::adopt`, so the symbol never got GNU's redirect tag.  Two
/// Lisp facts followed.  Measured, `-Q --batch`:
///
/// ```text
/// (list (boundp ...) (symbol-value ...) (special-variable-p ...))
///                                   GNU (t nil t)   this port, before (t nil nil)
/// (condition-case e (makunbound 'default-minibuffer-frame) (error e))
///                                   GNU (error "Built-in variable may not be unbound : ...")
///                                       this port, before: the symbol
/// ```
///
/// This is the whole of ledger 170's `refused -> allowed` residual as it reads
/// today -- one name of 578, re-measured on the image path (ledger 183 §8) --
/// and the reason is the one-shot pass, not the name.
#[test]
fn a_defvar_kboard_name_bound_after_the_adoption_pass_still_gets_gnus_tag() {
    let mut eval = ev();
    assert_eq!(
        format_eval_result(&eval.eval_str(
            "(list (boundp 'default-minibuffer-frame)
                   default-minibuffer-frame
                   (special-variable-p 'default-minibuffer-frame))"
        )),
        "OK (t nil t)"
    );
    assert_eq!(
        format_eval_result(
            &eval.eval_str("(condition-case e (makunbound 'default-minibuffer-frame) (error e))")
        ),
        r#"OK (error "Built-in variable may not be unbound : default-minibuffer-frame")"#
    );
    // `DEFVAR_KBOARD` also refuses `make-local-variable` (`src/data.c:2287-2290`).
    assert_eq!(
        format_eval_result(&eval.eval_str(
            "(condition-case e (make-local-variable 'default-minibuffer-frame) (error e))"
        )),
        r#"OK (error "Symbol default-minibuffer-frame may not be buffer-local")"#
    );
}
