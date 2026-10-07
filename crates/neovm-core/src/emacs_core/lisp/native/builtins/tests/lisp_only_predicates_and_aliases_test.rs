//! The six `subr.el`/`simple.el` type predicates and the five names GNU
//! creates with `defalias` are Lisp, and only Lisp -- DIVERGENCES.md 148.
//!
//! GNU's `src/` has no `DEFUN` for any of these eleven names
//! (`grep 'DEFUN ("NAME"' src/*.c` against emacs-mirror 31.0.90, 0ee48ac4df2,
//! finds nothing), so before the defining `.el` loads they are simply void,
//! and afterwards the cell holds what the `.el` put there: a byte-code
//! function for the six `defun`s, and the ALIAS SYMBOL for the five
//! `defalias`es.  A subr in any of those cells is a Rust reimplementation of
//! Lisp we already ship, which the standing rule forbids.
//!
//! `rust_subrs_shadowed_by_lisp_test.rs` is the scan that finds new ones;
//! this is the per-name statement for the eleven that entry 148 deleted.

use crate::emacs_core::eval::Context;
use crate::test_utils::{runtime_startup_context, runtime_startup_eval_one};

/// `lisp/subr.el` and `lisp/simple.el` `defun`s: `(1 . 1)`, no C version.
const LISP_DEFUN_PREDICATES: &[&str] = &[
    "booleanp",          // lisp/subr.el:4775
    "char-uppercase-p",  // lisp/simple.el:6683
    "integer-or-null-p", // lisp/subr.el:4809
    "list-of-strings-p", // lisp/subr.el:4768
    "macrop",            // lisp/subr.el:4793
    "string-or-null-p",  // lisp/subr.el:4762
];

/// `lisp/subr.el` `defalias`es: the cell holds the TARGET SYMBOL, so
/// `symbol-function` answers a symbol and `subrp` of it is nil.
const LISP_DEFALIASES: &[(&str, &str)] = &[
    ("move-marker", "set-marker"),  // lisp/subr.el:2280
    ("not", "null"),                // lisp/subr.el:71
    ("string<", "string-lessp"),    // lisp/subr.el:2278
    ("string=", "string-equal"),    // lisp/subr.el:2277
    ("string>", "string-greaterp"), // lisp/subr.el:2279
];

/// Before the defining `.el` loads there is nothing, exactly as in GNU.
#[test]
fn the_eleven_names_are_void_on_a_bare_evaluator_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    // A control: the primitives these are BUILT ON are `DEFUN`ed in GNU's
    // `src/`, so a bare evaluator must still answer for them.
    for primitive in ["null", "set-marker", "string-equal", "string-lessp"] {
        let result = eval.eval_str(&format!("(fboundp '{primitive})"));
        assert_eq!(
            crate::emacs_core::error::format_eval_result_with_eval(&eval, &result),
            "OK t",
            "{primitive} is DEFUN'ed in GNU src/ and must remain a subr",
        );
    }
    for name in LISP_DEFUN_PREDICATES
        .iter()
        .chain(LISP_DEFALIASES.iter().map(|(n, _)| n))
    {
        let probe = format!("(fboundp '{name})");
        let result = eval.eval_str(&probe);
        let printed = crate::emacs_core::error::format_eval_result_with_eval(&eval, &result);
        assert_eq!(
            printed, "OK nil",
            "{name} must be void before its .el loads: GNU's src/ has no \
             DEFUN of that name, so a bare evaluator has nothing to answer with",
        );
    }
}

/// After `loadup.el`, every one of them holds what the `.el` put there.
#[test]
fn the_six_predicates_are_lisp_defuns_in_the_loaded_runtime_like_gnu() {
    crate::test_utils::init_test_tracing();
    for name in LISP_DEFUN_PREDICATES {
        assert_eq!(
            runtime_startup_eval_one(&format!("(subrp (symbol-function '{name}))")),
            "OK nil",
            "{name} must be the `.el' definition, not a Rust subr",
        );
        // GNU: every one of the six is `(1 . 1)'.  The Rust subrs answered
        // `(0 . many)' for five of them.
        assert_eq!(
            runtime_startup_eval_one(&format!("(func-arity '{name})")),
            "OK (1 . 1)",
            "{name} arity must match GNU's one-argument `defun'",
        );
    }
}

#[test]
fn the_five_alias_cells_hold_a_symbol_in_the_loaded_runtime_like_gnu() {
    crate::test_utils::init_test_tracing();
    for (name, target) in LISP_DEFALIASES {
        assert_eq!(
            runtime_startup_eval_one(&format!("(symbol-function '{name})")),
            format!("OK {target}"),
            "GNU defines {name} with `defalias', so its cell holds the SYMBOL \
             {target} -- a subr there is a different observable",
        );
        assert_eq!(
            runtime_startup_eval_one(&format!("(subrp (symbol-function '{name}))")),
            "OK nil",
        );
        // ...and the alias really resolves to the target's own definition.
        // GNU 31.0.90 answers t for all five.
        assert_eq!(
            runtime_startup_eval_one(&format!(
                "(eq (indirect-function '{name}) (symbol-function '{target}))"
            )),
            "OK t",
            "{name} must indirect to {target}'s definition, as in GNU",
        );
    }
}

/// The behaviour the Rust `char-uppercase-p` got wrong: GNU consults the
/// Unicode `lowercase` property, which U+0130 LATIN CAPITAL LETTER I WITH DOT
/// ABOVE has, so GNU answers t.  The deleted subr compared against the case
/// table's downcase mapping, which leaves U+0130 alone, and answered nil.
#[test]
fn char_uppercase_p_answers_for_the_unicode_property_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU 31.0.90 -Q --batch, measured.
    for (probe, expected) in [
        ("(char-uppercase-p ?A)", "OK t"),
        ("(char-uppercase-p ?a)", "OK nil"),
        ("(char-uppercase-p 452)", "OK t"), // U+01C4 DZ WITH CARON
        ("(char-uppercase-p 453)", "OK t"), // U+01C5 Dz WITH CARON (title)
        ("(char-uppercase-p 223)", "OK nil"), // U+00DF sharp s
        ("(char-uppercase-p 304)", "OK t"), // U+0130 I WITH DOT ABOVE
    ] {
        assert_eq!(runtime_startup_eval_one(probe), expected, "{probe}");
    }
}

/// The five `defalias` names are compiled to OPCODES by GNU's byte compiler,
/// so a compiled caller never reads the cell at all.  Measured byte-for-byte
/// against GNU 31.0.90; recorded because it is why deleting the Rust subrs
/// cannot be a performance change for compiled code.
#[test]
fn byte_compiled_callers_of_the_aliases_use_opcodes_like_gnu() {
    crate::test_utils::init_test_tracing();
    // Byte values, not printed forms: the compiled code strings hold control
    // characters that a printed comparison cannot see.  Every row measured on
    // GNU 31.0.90 with `lexical-binding' t.
    // 63 = Bnot, 152 = Bstringeqlsign, 153 = Bstringlss, 147 = Bset_marker,
    // 33 = Bcall1, 135 = Breturn.
    for (form, codes, constants) in [
        ("(lambda (x) (not x))", "(63 135)", "[]"),
        ("(lambda (a b) (string= a b))", "(1 1 152 135)", "[]"),
        ("(lambda (a b) (string< a b))", "(1 1 153 135)", "[]"),
        ("(lambda (a b) (string> a b))", "(137 2 153 135)", "[]"),
        (
            "(lambda (m p) (move-marker m p))",
            "(1 1 192 147 135)",
            "[nil]",
        ),
        // Control: the six predicates are ordinary calls in GNU too, so the
        // constants vector names the function and the cell IS read.
        ("(lambda (x) (booleanp x))", "(192 1 33 135)", "[booleanp]"),
    ] {
        assert_eq!(
            runtime_startup_eval_one(&format!("(append (aref (byte-compile '{form}) 1) nil)")),
            format!("OK {codes}"),
            "{form} should compile to GNU's opcode sequence",
        );
        assert_eq!(
            runtime_startup_eval_one(&format!("(aref (byte-compile '{form}) 2)")),
            format!("OK {constants}"),
            "{form} should compile to GNU's constants vector",
        );
    }
}

/// The seven arms that used to be asserted against `subr_info::macrop_check`
/// on a bare `Context` (`subr/info_tests.rs`), plus the keyword-designator arm
/// from `builtins/tests.rs`, asked of the Lisp that actually runs.  Every
/// expected value measured on GNU 31.0.90 `-Q --batch` first.
#[test]
fn macrop_arms_match_gnu() {
    crate::test_utils::init_test_tracing();
    for (probe, expected) in [
        ("(macrop (cons 'macro (lambda (form) form)))", "OK t"),
        ("(macrop (lambda (x) x))", "OK nil"),
        ("(macrop nil)", "OK nil"),
        ("(macrop '(macro . 1))", "OK t"),
        (
            "(macrop '(autoload \"dummy-file\" nil nil macro))",
            "OK (macro t)",
        ),
        ("(macrop '(autoload \"dummy-file\" nil t nil))", "OK nil"),
        ("(macrop '(autoload \"dummy-file\" nil nil t))", "OK (t)"),
        // A macro, a special form and an ordinary subr.
        ("(macrop 'when)", "OK t"),
        ("(macrop 'if)", "OK nil"),
        ("(macrop 'car)", "OK nil"),
        ("(macrop 42)", "OK nil"),
        // Keyword designator: `indirect-function' reads a keyword's own
        // function cell, so a macro fset there answers t.
        (
            "(progn (fset :pw54-kw (cons 'macro (lambda (&rest args) args))) \
             (macrop :pw54-kw))",
            "OK t",
        ),
        // And through an alias chain.
        (
            "(progn (fset 'pw54-alias-a (cons 'macro (lambda (&rest args) args))) \
             (defalias 'pw54-alias-b 'pw54-alias-a) (macrop 'pw54-alias-b))",
            "OK t",
        ),
    ] {
        assert_eq!(runtime_startup_eval_one(probe), expected, "{probe}");
    }
}

/// `builtin_move_marker_matches_set_marker_behavior` (`marker_test.rs`) moved
/// onto the runtime: the alias must really move a marker, not merely exist.
/// GNU 31.0.90 measured: the call returns the marker itself, position 3, and
/// the current buffer.
#[test]
fn move_marker_is_the_set_marker_alias_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        runtime_startup_eval_one(
            "(with-temp-buffer (insert \"abcdef\") \
               (let ((m (make-marker))) \
                 (list (eq (move-marker m 3) m) \
                       (marker-position m) \
                       (eq (marker-buffer m) (current-buffer)))))"
        ),
        "OK (t 3 t)",
    );
}

/// The six predicates' answer arms, measured against GNU.  These were never
/// pinned on the Rust subrs; they are recorded so the deletion has a
/// behavioural gate and not only a shape gate.
#[test]
fn the_six_predicates_answer_like_gnu() {
    crate::test_utils::init_test_tracing();
    for (probe, expected) in [
        ("(booleanp t)", "OK t"),
        ("(booleanp nil)", "OK t"),
        ("(booleanp 'x)", "OK nil"),
        ("(string-or-null-p \"a\")", "OK t"),
        ("(string-or-null-p nil)", "OK t"),
        ("(string-or-null-p 'a)", "OK nil"),
        ("(integer-or-null-p 1)", "OK t"),
        ("(integer-or-null-p nil)", "OK t"),
        ("(integer-or-null-p 1.0)", "OK nil"),
        ("(list-of-strings-p nil)", "OK t"),
        ("(list-of-strings-p '(\"a\" \"b\"))", "OK t"),
        ("(list-of-strings-p '(\"a\" . \"b\"))", "OK nil"),
        ("(list-of-strings-p '(\"a\" 1))", "OK nil"),
        ("(list-of-strings-p \"a\")", "OK nil"),
        (
            "(char-uppercase-p \"a\")",
            "ERR (wrong-type-argument (characterp \"a\"))",
        ),
        (
            "(char-uppercase-p nil)",
            "ERR (wrong-type-argument (characterp nil))",
        ),
        (
            "(char-uppercase-p -1)",
            "ERR (wrong-type-argument (characterp -1))",
        ),
        // A `defun' reports its own arity in the datum; the deleted subrs
        // reported the function SYMBOL.
        (
            "(condition-case e (booleanp) (error e))",
            "OK (wrong-number-of-arguments (1 . 1) 0)",
        ),
        (
            "(condition-case e (string-or-null-p \"a\" \"b\") (error e))",
            "OK (wrong-number-of-arguments (1 . 1) 2)",
        ),
        // `not' is a `defalias' to the C subr `null', so GNU's datum is the
        // function symbol, not an arity cons.
        (
            "(condition-case e (not) (error e))",
            "OK (wrong-number-of-arguments not 0)",
        ),
    ] {
        assert_eq!(runtime_startup_eval_one(probe), expected, "{probe}");
    }
}

/// Neither of the two static-subr dispatch paths can reach a deleted name,
/// and neither could reach these eleven even before the deletion: both are
/// gated on a VOID function cell (`vm.rs:6679` and `:7150`), and all eleven
/// cells are written by `loadup.el`.
#[test]
fn no_rust_subr_is_registered_for_the_eleven_names() {
    crate::test_utils::init_test_tracing();
    // The global subr registry is populated by `init_builtins`, which runs
    // when an evaluator is built; ask for one before reading the table.
    let _eval = Context::new();
    for name in LISP_DEFUN_PREDICATES
        .iter()
        .chain(LISP_DEFALIASES.iter().map(|(n, _)| n))
    {
        assert!(
            crate::emacs_core::eval::lookup_global_subr_entry(crate::emacs_core::intern::intern(
                name
            ))
            .is_none(),
            "{name} must have no Rust subr entry: GNU implements it in Lisp \
             and nowhere in src/",
        );
    }
    // Control: the primitives they delegate to ARE C subrs in GNU.
    for name in [
        "null",
        "set-marker",
        "string-equal",
        "string-lessp",
        "stringp",
        "integerp",
    ] {
        assert!(
            crate::emacs_core::eval::lookup_global_subr_entry(crate::emacs_core::intern::intern(
                name
            ))
            .is_some(),
            "{name} is DEFUN'ed in GNU's src/ and must stay a Rust subr",
        );
    }
    // And the booted runtime still answers through them.
    let _ = runtime_startup_context();
}
