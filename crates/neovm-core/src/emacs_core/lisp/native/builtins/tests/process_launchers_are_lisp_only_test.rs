//! The four process launchers are Lisp, and only Lisp -- DIVERGENCES.md 149.
//!
//! `grep 'DEFUN ("start-process"' src/*.c` (and the same for the other three)
//! against emacs-mirror 31.0.90 (0ee48ac4df2) finds nothing: GNU builds all
//! four on `make-process', which IS in C (src/process.c:1767).
//!
//! * `start-process' -- `defun', lisp/subr.el:3466
//! * `start-process-shell-command' -- `defun', lisp/subr.el:5063
//! * `start-file-process' -- `defun', lisp/simple.el:5249
//! * `start-file-process-shell-command' -- `defun', lisp/subr.el:5076
//!
//! So before those files load the names are void, and afterwards the cell
//! holds a byte-code function.  Unlike the `defalias' names of entry 148,
//! GNU's byte compiler gives these four no opcode and no compiler macro: a
//! compiled caller names the function in its constants vector and reads the
//! CELL, which is why a Rust subr in that cell was never reachable and could
//! drift unnoticed -- twice (DIVERGENCES.md 131).
//!
//! `rust_subrs_shadowed_by_lisp_test.rs` is the scan that finds new shadows;
//! this is the per-name statement for the four that entry 149 deleted.

use crate::emacs_core::eval::Context;
use crate::test_utils::{runtime_startup_context, runtime_startup_eval_one};

/// The four names, with the `.el` line that owns each and the arity GNU's
/// `defun` reports.
const LISP_ONLY_LAUNCHERS: &[(&str, &str)] = &[
    ("start-file-process", "(3 . many)"), // lisp/simple.el:5249
    ("start-file-process-shell-command", "(3 . 3)"), // lisp/subr.el:5076
    ("start-process", "(3 . many)"),      // lisp/subr.el:3466
    ("start-process-shell-command", "(3 . 3)"), // lisp/subr.el:5063
];

/// Before `subr.el` and `simple.el` load there is nothing, exactly as in GNU.
#[test]
fn the_four_launchers_are_void_on_a_bare_evaluator_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    // Control: the primitive all four are built on IS `DEFUN'ed in GNU's
    // src/process.c:1767, so a bare evaluator must still answer for it.
    for primitive in ["make-process", "processp", "process-command"] {
        let result = eval.eval_str(&format!("(fboundp '{primitive})"));
        assert_eq!(
            crate::emacs_core::error::format_eval_result_with_eval(&eval, &result),
            "OK t",
            "{primitive} is DEFUN'ed in GNU src/ and must remain a subr",
        );
    }
    for (name, _) in LISP_ONLY_LAUNCHERS {
        let result = eval.eval_str(&format!("(fboundp '{name})"));
        assert_eq!(
            crate::emacs_core::error::format_eval_result_with_eval(&eval, &result),
            "OK nil",
            "{name} must be void before its .el loads: GNU's src/ has no \
             DEFUN of that name, so a bare evaluator has nothing to answer with",
        );
    }
}

/// After `loadup.el`, every one of them is the `.el` definition.
#[test]
fn the_four_launchers_are_lisp_defuns_in_the_loaded_runtime_like_gnu() {
    crate::test_utils::init_test_tracing();
    for (name, arity) in LISP_ONLY_LAUNCHERS {
        assert_eq!(
            runtime_startup_eval_one(&format!("(subrp (symbol-function '{name}))")),
            "OK nil",
            "{name} must be the `.el' definition, not a Rust subr",
        );
        assert_eq!(
            runtime_startup_eval_one(&format!("(func-arity '{name})")),
            format!("OK {arity}"),
            "{name} arity must match GNU's `defun'",
        );
    }
    // The docstring comes from the `.el' too -- the Rust subrs answered the
    // generic "Built-in function." on a bare evaluator.  GNU 31.0.90 measured.
    assert_eq!(
        runtime_startup_eval_one("(substring (documentation 'start-process) 0 48)"),
        "OK \"Start a program in a subprocess.  Return the pro\"",
    );
}

/// GNU compiles a call to any of the four as an ORDINARY call: the function
/// is named in the constants vector and the cell is read at call time.  No
/// `byte-compile' property and no `compiler-macro' is involved (contrast
/// entry 148's five `defalias' names, four of which become opcodes).
/// Measured byte-for-byte against GNU 31.0.90 with `lexical-binding' t.
#[test]
fn byte_compiled_callers_of_the_launchers_match_gnu() {
    crate::test_utils::init_test_tracing();
    // 192 = Bconstant+0, 3/4 = Bstack-ref3/4, 35/36 = Bcall3/Bcall4,
    // 135 = Breturn.
    for (form, codes, constants) in [
        (
            "(lambda (n b p) (start-process n b p))",
            "(192 3 3 3 35 135)",
            "[start-process]",
        ),
        (
            "(lambda (n b p a) (start-process n b p a))",
            "(192 4 4 4 4 36 135)",
            "[start-process]",
        ),
        (
            "(lambda (n b c) (start-process-shell-command n b c))",
            "(192 3 3 3 35 135)",
            "[start-process-shell-command]",
        ),
        (
            "(lambda (n b p) (start-file-process n b p))",
            "(192 3 3 3 35 135)",
            "[start-file-process]",
        ),
        (
            "(lambda (n b c) (start-file-process-shell-command n b c))",
            "(192 3 3 3 35 135)",
            "[start-file-process-shell-command]",
        ),
        // Control: the C primitive underneath them compiles the same way.
        (
            "(lambda (n c) (make-process :name n :command c))",
            "(192 193 3 194 4 36 135)",
            "[make-process :name :command]",
        ),
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

/// What the deleted Rust subrs got wrong, asked of the Lisp that runs.  Every
/// expected value was measured on GNU 31.0.90 `-Q --batch` first; the comment
/// on each row is what the Rust subr answered instead.
///
/// `process-connection-type' is nil throughout: a pty EOF drops the decoder's
/// carryover (DIVERGENCES.md 139), which would measure that instead of these.
#[test]
fn the_four_launchers_answer_like_gnu() {
    crate::test_utils::init_test_tracing();
    for (probe, expected) in [
        // PROGRAM nil means "just associate a pty with the buffer": GNU's
        // `start-process' passes NO `:command' at all, so `process-command'
        // is nil.  The Rust subr turned the nil PROGRAM into the literal
        // string "nil" and reported ("nil").
        (
            "(let* ((process-connection-type nil) \
               (p (start-process \"pw149-b1\" nil nil))) \
               (list (processp p) (process-command p) (process-status p)))",
            "OK (t nil run)",
        ),
        // A `defun' signals `wrong-number-of-arguments' with an arity cons;
        // the Rust subr's `expect_min_args' signalled with the SYMBOL.
        (
            "(condition-case e (start-process \"pw149-c1\") (error e))",
            "OK (wrong-number-of-arguments (3 . 3) 1)",
        ),
        (
            "(condition-case e (start-file-process \"pw149-c1b\" nil) (error e))",
            "OK (wrong-number-of-arguments (3 . 3) 2)",
        ),
        (
            "(condition-case e (start-process-shell-command \"pw149-c1c\") (error e))",
            "OK (wrong-number-of-arguments (3 . 3) 1)",
        ),
        // The type contracts come from `make-process' and always matched.
        (
            "(condition-case e (start-process \"pw149-c3\" nil 'true) (error e))",
            "OK (wrong-type-argument stringp true)",
        ),
        (
            "(condition-case e (start-process 'pw149-c2 nil \"/bin/sh\") (error e))",
            "OK (error \":name value not a string\")",
        ),
        // `start-process-shell-command' hands `shell-file-name' and
        // `shell-command-switch' STRAIGHT to `start-process' -- a nil
        // `shell-file-name' therefore means "no PROGRAM", and a nil
        // `shell-command-switch' reaches `make-process' and is refused.  The
        // Rust wrappers substituted "sh" and "-c" for a nil variable, so both
        // spawned a shell GNU does not.
        (
            "(let* ((process-connection-type nil) (shell-file-name nil) \
               (p (start-process-shell-command \"pw149-e1\" nil \"true\"))) \
               (process-command p))",
            "OK nil",
        ),
        (
            "(let ((process-connection-type nil) (shell-command-switch nil)) \
               (condition-case e (start-process-shell-command \"pw149-e2\" nil \"true\") \
                 (error e)))",
            "OK (wrong-type-argument stringp nil)",
        ),
        (
            "(let* ((process-connection-type nil) (shell-file-name nil) \
               (p (start-file-process-shell-command \"pw149-e4\" nil \"true\"))) \
               (process-command p))",
            "OK nil",
        ),
        (
            "(let ((process-connection-type nil) (shell-file-name 42)) \
               (condition-case e (start-process-shell-command \"pw149-e3\" nil \"true\") \
                 (error e)))",
            "OK (wrong-type-argument stringp 42)",
        ),
        // The shell wrappers really do run the shell named by the variable.
        (
            "(let* ((process-connection-type nil) \
               (shell-file-name \"/bin/sh\") (shell-command-switch \"-c\") \
               (p (start-process-shell-command \"pw149-e5\" nil \"exit 0\"))) \
               (process-command p))",
            "OK (\"/bin/sh\" \"-c\" \"exit 0\")",
        ),
        // A program that is not on `exec-path' is refused before any child
        // exists, by `make-process' (src/process.c:1965-1971).
        (
            "(let ((process-connection-type nil)) \
               (condition-case e (start-process \"pw149-f1\" nil \"neomacs-no-such-program-xyz\") \
                 (error e)))",
            "OK (file-missing \"Searching for program\" \"No such file or directory\" \
             \"neomacs-no-such-program-xyz\")",
        ),
        // `make-process' uniquifies the name, and a string BUFFER is created.
        (
            "(let* ((process-connection-type nil) \
               (a (start-process \"pw149-d1\" nil nil)) \
               (b (start-process \"pw149-d1\" nil nil))) \
               (list (process-name a) (process-name b)))",
            "OK (\"pw149-d1\" \"pw149-d1<1>\")",
        ),
        (
            "(let* ((process-connection-type nil) \
               (p (start-process \"pw149-d2\" \"pw149-d2-buffer\" nil))) \
               (list (bufferp (process-buffer p)) (buffer-name (process-buffer p))))",
            "OK (t \"pw149-d2-buffer\")",
        ),
    ] {
        assert_eq!(runtime_startup_eval_one(probe), expected, "{probe}");
    }
}

/// `start-file-process' consults `file-name-handler-alist' for its own
/// operation symbol, which is the only thing it adds over `start-process'
/// (lisp/simple.el:5266-5268).  Kept next to the deletion because it is the
/// one arm whose Rust version had a hand-written handler lookup.
#[test]
fn start_file_process_dispatches_to_a_file_name_handler_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        runtime_startup_eval_one(
            "(let ((default-directory \"/pw149mock:/\")) \
               (defun pw149-handler (operation &rest args) (list :handled operation args)) \
               (let ((file-name-handler-alist '((\"\\\\`/pw149mock:\" . pw149-handler)))) \
                 (start-file-process \"pw149-sfp\" nil \"prog\" \"arg\")))"
        ),
        "OK (:handled start-file-process (\"pw149-sfp\" nil \"prog\" \"arg\"))",
    );
}

/// Neither static-subr dispatch path can reach a deleted name, and neither
/// could reach these four even before the deletion: both are gated on a VOID
/// function cell (`vm.rs:6679` and `:7150`), and `loadup.el` writes all four
/// cells.  The JIT's `Op::CallBuiltinSym` fast path cannot name them either --
/// that op is only ever emitted for GNU bytecodes 96..=127, the buffer/point
/// ops (`bytecode/decode.rs:642-649`).
#[test]
fn no_rust_subr_is_registered_for_the_four_names() {
    crate::test_utils::init_test_tracing();
    // The global subr registry is populated by `init_builtins', which runs
    // when an evaluator is built; ask for one before reading the table.
    let _eval = Context::new();
    for (name, _) in LISP_ONLY_LAUNCHERS {
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
    for name in ["make-process", "processp", "process-command", "get-buffer"] {
        assert!(
            crate::emacs_core::eval::lookup_global_subr_entry(crate::emacs_core::intern::intern(
                name
            ))
            .is_some(),
            "{name} is DEFUN'ed in GNU's src/ and must stay a Rust subr",
        );
    }
    // And the booted runtime still answers through them.
    assert_eq!(runtime_startup_eval_one("(fboundp 'start-process)"), "OK t",);
    let _ = runtime_startup_context();
}
