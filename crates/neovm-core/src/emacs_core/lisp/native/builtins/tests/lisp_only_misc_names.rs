//! The thirteen miscellaneous names of the shadowed-subr class are Lisp, and
//! only Lisp -- DIVERGENCES.md 152.
//!
//! Ledger 146 enumerated the class -- Rust subrs whose function cell is
//! overwritten by the Lisp `loadup.el` preloads -- and 148, 149 and 150 took
//! the groups with a theme.  What was left was labelled "Everything else":
//! thirteen names from six different files, related only by being Lisp GNU has
//! no C version of.  `grep 'DEFUN ("NAME"' src/*.c` against emacs-mirror
//! 31.0.90 (0ee48ac4df2) finds nothing for any of them.
//!
//! Two of the thirteen have a C NEIGHBOUR that must not be deleted with them,
//! and the neighbour is the point of the pairing:
//!
//! * `string-match-p` is a `defsubst` (lisp/subr.el:5941) over `string-match`,
//!   which IS `DEFUN`ed (src/search.c:442).  Deleting "the string-match subr"
//!   would have removed a primitive.
//! * `transient-mark-mode` the COMMAND is `define-minor-mode`
//!   (lisp/simple.el:7614); `transient-mark-mode` the VARIABLE is
//!   `DEFVAR_LISP` (src/buffer.c:5835).  The variable stays.
//!
//! `rust_subrs_shadowed_by_lisp_test.rs` is the scan that finds new shadows;
//! this is the per-name statement for the thirteen entry 152 deleted, and the
//! statement that the C names beside them are still here.

use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::lookup_global_subr_entry;
use crate::emacs_core::intern::intern;
use crate::test_utils::{runtime_startup_eval_all, runtime_startup_eval_one};

/// GNU has no C version of these, so a bare evaluator -- which is GNU before
/// `loadup.el` -- must have nothing to answer with.
const LISP_ONLY_MISC_NAMES: &[&str] = &[
    "emacs-repository-get-branch",   // lisp/version.el:231
    "emacs-repository-get-version",  // lisp/version.el:183
    "global-set-key",                // lisp/subr.el:1545
    "ignore",                        // lisp/subr.el:501
    "local-set-key",                 // lisp/subr.el:1569
    "make-auto-save-file-name",      // lisp/files.el:7699
    "memory-limit",                  // lisp/subr.el:3574
    "read-number",                   // lisp/subr.el:3725
    "set-buffer-file-coding-system", // lisp/international/mule.el:1302
    "string-greaterp",               // lisp/subr.el:6283
    "string-match-p",                // lisp/subr.el:5941 (a `defsubst')
    "symbol-file",                   // lisp/subr.el:3351
    "transient-mark-mode",           // lisp/simple.el:7614 (`define-minor-mode')
];

/// The C primitives each of the thirteen is written over, or sits beside.
/// Deleting the Lisp names is not a licence to delete these.
const C_PRIMITIVES_BESIDE_THEM: &[&str] = &[
    "current-global-map",   // src/keymap.c -- `global-set-key's map
    "current-local-map",    // src/keymap.c -- `local-set-key's map
    "define-key",           // src/keymap.c -- both set-key bodies
    "do-auto-save",         // src/fileio.c -- reads the auto-save name
    "make-local-variable",  // src/data.c -- a C DEFUN that IS interactive
    "process-attributes",   // src/process.c -- `memory-limit's source
    "read-from-minibuffer", // src/minibuf.c -- `read-number's reader
    "string-lessp",         // src/fns.c -- `string-greaterp' swaps into it
    "string-match",         // src/search.c:442 -- `string-match-p' inlines it
];

#[test]
fn the_thirteen_misc_names_are_void_on_a_bare_evaluator_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    for primitive in C_PRIMITIVES_BESIDE_THEM {
        let result = eval.eval_str(&format!("(fboundp '{primitive})"));
        assert_eq!(
            crate::emacs_core::error::format_eval_result_with_eval(&eval, &result),
            "OK t",
            "{primitive} is DEFUN'ed in GNU src/ and must remain a subr",
        );
    }

    for name in LISP_ONLY_MISC_NAMES {
        let result = eval.eval_str(&format!("(fboundp '{name})"));
        assert_eq!(
            crate::emacs_core::error::format_eval_result_with_eval(&eval, &result),
            "OK nil",
            "{name} must be void before its .el loads: GNU's src/ has no DEFUN \
             of that name, so a bare evaluator has nothing to answer with",
        );
    }
}

#[test]
fn no_rust_subr_is_registered_for_the_thirteen_misc_names() {
    crate::test_utils::init_test_tracing();
    // The global subr registry is populated by `init_builtins`, which runs
    // when an evaluator is built; ask for one before reading the table.
    let _eval = Context::new();
    for name in LISP_ONLY_MISC_NAMES {
        assert!(
            lookup_global_subr_entry(intern(name)).is_none(),
            "{name} must have no Rust subr: GNU implements it in Lisp and \
             nowhere in src/",
        );
    }
    for name in C_PRIMITIVES_BESIDE_THEM {
        assert!(
            lookup_global_subr_entry(intern(name)).is_some(),
            "{name} IS a C DEFUN in GNU and must stay registered here",
        );
    }
}

/// `transient-mark-mode` is the split name: GNU has the VARIABLE in C and the
/// COMMAND in Lisp.  Deleting the command must not have taken the variable.
#[test]
fn transient_mark_mode_keeps_its_c_variable_on_a_bare_evaluator() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval.eval_str(
        "(list (boundp 'transient-mark-mode)
               (default-value 'transient-mark-mode)
               (fboundp 'transient-mark-mode))",
    );
    // DEFVAR_LISP ("transient-mark-mode", ...) is src/buffer.c:5835, and GNU
    // initialises it to nil there; the `define-minor-mode' at
    // lisp/simple.el:7614 is what later turns it on.
    assert_eq!(
        crate::emacs_core::error::format_eval_result_with_eval(&eval, &result),
        "OK (t nil nil)",
    );
}

/// Every observable a Lisp caller can ask about the thirteen, measured on GNU
/// 31.0.90 `-Q --batch` first (tmp/pw59/gnu-observables.txt).
///
/// The Rust subrs got EIGHT arities wrong and FOUR `commandp`s wrong, and all
/// thirteen answered the generic "Built-in function." for `documentation`.
#[test]
fn the_thirteen_misc_names_are_lisp_in_the_loaded_runtime_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        runtime_startup_eval_all(
            r#"
;; name                          subrp  func-arity   commandp
(list (subrp (symbol-function 'ignore)) (func-arity 'ignore) (and (commandp 'ignore) t))
(list (subrp (symbol-function 'global-set-key)) (func-arity 'global-set-key)
      (and (commandp 'global-set-key) t))
(list (subrp (symbol-function 'local-set-key)) (func-arity 'local-set-key)
      (and (commandp 'local-set-key) t))
(list (subrp (symbol-function 'symbol-file)) (func-arity 'symbol-file)
      (and (commandp 'symbol-file) t))
(list (subrp (symbol-function 'string-match-p)) (func-arity 'string-match-p)
      (and (commandp 'string-match-p) t))
(list (subrp (symbol-function 'transient-mark-mode)) (func-arity 'transient-mark-mode)
      (and (commandp 'transient-mark-mode) t))
(list (subrp (symbol-function 'emacs-repository-get-branch))
      (func-arity 'emacs-repository-get-branch)
      (and (commandp 'emacs-repository-get-branch) t))
(list (subrp (symbol-function 'emacs-repository-get-version))
      (func-arity 'emacs-repository-get-version)
      (and (commandp 'emacs-repository-get-version) t))
(list (subrp (symbol-function 'make-auto-save-file-name))
      (func-arity 'make-auto-save-file-name)
      (and (commandp 'make-auto-save-file-name) t))
(list (subrp (symbol-function 'memory-limit)) (func-arity 'memory-limit)
      (and (commandp 'memory-limit) t))
(list (subrp (symbol-function 'read-number)) (func-arity 'read-number)
      (and (commandp 'read-number) t))
(list (subrp (symbol-function 'set-buffer-file-coding-system))
      (func-arity 'set-buffer-file-coding-system)
      (and (commandp 'set-buffer-file-coding-system) t))
(list (subrp (symbol-function 'string-greaterp)) (func-arity 'string-greaterp)
      (and (commandp 'string-greaterp) t))
;; The interactive forms of the four that ARE commands, and the one whose
;; spec is a plain control string.
(interactive-form 'ignore)
;; The C primitives beside them are untouched.
(list (subrp (symbol-function 'string-match)) (func-arity 'string-match))
(list (subrp (symbol-function 'string-lessp)) (func-arity 'string-lessp))
(list (subrp (symbol-function 'define-key)) (func-arity 'define-key))
;; The first line of each docstring is `.elc' text, not "Built-in function."
(car (split-string (documentation 'ignore) "\n"))
(car (split-string (documentation 'string-greaterp) "\n"))
(car (split-string (documentation 'memory-limit) "\n"))
"#,
        ),
        vec![
            "OK (nil (0 . many) t)",
            "OK (nil (2 . 2) t)",
            "OK (nil (2 . 2) t)",
            "OK (nil (1 . 3) nil)",
            "OK (nil (2 . 3) nil)",
            "OK (nil (0 . 1) t)",
            "OK (nil (0 . 1) nil)",
            "OK (nil (0 . 2) nil)",
            "OK (nil (0 . 0) nil)",
            "OK (nil (0 . 0) nil)",
            "OK (nil (1 . 3) nil)",
            "OK (nil (1 . 3) t)",
            "OK (nil (2 . 2) nil)",
            "OK (interactive nil)",
            "OK (t (2 . 4))",
            "OK (t (2 . 2))",
            "OK (t (3 . 4))",
            "OK \"Ignore ARGUMENTS, do nothing, and return nil.\"",
            "OK \"Return non-nil if STRING1 is greater than STRING2 in lexicographic order.\"",
            "OK \"Return an estimate of Emacs virtual memory usage, divided by 1024.\"",
        ],
    );
}

/// Three of the thirteen are names a compiled caller NEVER looks up, and each
/// gets there by a different door.  This is the reason 148's warning applies
/// here too: for these three the shadow was never even consulted.
///
/// Measured on GNU 31.0.90 with `lexical-binding` t
/// (tmp/pw59/gnu-observables.txt).
///
/// 192/193/194 = Bconstant+N, 1/2/3 = Bstack_ref, 32+N = Bcall N, 33 = Bcall1,
/// 34 = Bcall2, 36 = Bcall4, 135 = Breturn, 137 = Bdup, 153 = Bstringlss.
#[test]
fn three_of_the_thirteen_are_never_looked_up_by_a_compiled_caller_like_gnu() {
    crate::test_utils::init_test_tracing();
    for (form, codes, constants) in [
        // `(byte-defop-compiler-1 ignore)', lisp/emacs-lisp/bytecomp.el:4429:
        // the arguments are compiled for effect and the result is a constant
        // nil.  The name never reaches the constants vector.
        ("(lambda (x) (ignore x))", "(192 135)", "[nil]"),
        ("(lambda () (ignore))", "(192 135)", "[nil]"),
        // A `defsubst' is inlined: `(string-match REGEXP STRING START t)',
        // lisp/subr.el:5941, with START defaulting to nil.
        (
            "(lambda (r s) (string-match-p r s))",
            "(1 1 192 193 3 3 3 194 36 135)",
            "[nil string-match t]",
        ),
        (
            "(lambda (r s n) (string-match-p r s n))",
            "(2 2 2 192 3 3 3 193 36 135)",
            "[string-match t]",
        ),
        // A `compiler-macro' (lisp/subr.el:6287-6290) swaps the arguments into
        // `string-lessp', which has an opcode.  Empty constants vector.
        (
            "(lambda (a b) (string-greaterp a b))",
            "(137 2 153 135)",
            "[]",
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

/// The other ten compile to an ordinary call through the constants vector, so
/// a compiled caller DOES read the function cell -- the shadow was the only
/// thing between those callers and the Rust subr.
#[test]
fn the_other_ten_are_ordinary_calls_that_read_the_cell_like_gnu() {
    crate::test_utils::init_test_tracing();
    for (form, codes, constants) in [
        (
            "(lambda (k c) (global-set-key k c))",
            "(192 2 2 34 135)",
            "[global-set-key]",
        ),
        (
            "(lambda (k c) (local-set-key k c))",
            "(192 2 2 34 135)",
            "[local-set-key]",
        ),
        (
            "(lambda (s) (symbol-file s))",
            "(192 1 33 135)",
            "[symbol-file]",
        ),
        (
            "(lambda (a) (transient-mark-mode a))",
            "(192 1 33 135)",
            "[transient-mark-mode]",
        ),
        (
            "(lambda () (emacs-repository-get-branch))",
            "(192 32 135)",
            "[emacs-repository-get-branch]",
        ),
        (
            "(lambda () (emacs-repository-get-version))",
            "(192 32 135)",
            "[emacs-repository-get-version]",
        ),
        (
            "(lambda () (make-auto-save-file-name))",
            "(192 32 135)",
            "[make-auto-save-file-name]",
        ),
        (
            "(lambda () (memory-limit))",
            "(192 32 135)",
            "[memory-limit]",
        ),
        (
            "(lambda (p) (read-number p))",
            "(192 1 33 135)",
            "[read-number]",
        ),
        (
            "(lambda (c) (set-buffer-file-coding-system c))",
            "(192 1 33 135)",
            "[set-buffer-file-coding-system]",
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

/// The symbol properties GNU's `declare` forms install, which is where the
/// three "never looked up" doors above come from.
#[test]
fn the_declared_symbol_properties_match_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        runtime_startup_eval_all(
            "(function-get 'ignore 'byte-compile)
             (progn (require 'bytecomp) (function-get 'ignore 'byte-compile))
             (function-get 'ignore 'pure)
             (function-get 'ignore 'side-effect-free)
             (and (function-get 'string-greaterp 'compiler-macro) t)
             (function-get 'string-match-p 'side-effect-free)
             (and (function-get 'string-match-p 'byte-optimizer) t)
             (function-get 'memory-limit 'side-effect-free)
             (function-get 'symbol-file 'important-return-value)",
        ),
        vec![
            // The property lives in `bytecomp.el', which neither GNU nor this
            // runtime preloads -- measured: GNU 31.0.90 -Q --batch answers nil
            // here too, and `byte-compile-ignore' only after the `require'.
            "OK nil",
            // lisp/emacs-lisp/bytecomp.el:4429 puts this on the name itself.
            "OK byte-compile-ignore",
            "OK t",
            // Deliberately NOT side-effect-free: lisp/subr.el:505-506 says so,
            // "because we don't want calls to it elided".
            "OK nil",
            "OK t",
            "OK t",
            // A `defsubst' carries a byte-optimizer; that is the inlining.
            "OK t",
            "OK error-free",
            "OK t",
        ],
    );
}

/// What a Lisp caller observes from the thirteen, measured under GNU 31.0.90
/// `-Q --batch` first (tmp/pw59/gnu-arms.txt) and re-asked of the runtime,
/// where the `.el` definitions are what reply.
///
/// Rows marked DIVERGED are ones the deleted Rust subr got wrong.
#[test]
fn misc_name_arms_match_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        runtime_startup_eval_all(
            r#"
;; -------------------------------------------------------------- ignore
(ignore)
(ignore 1 2 3)
(apply #'ignore '(1 2))
;; ------------------------------------------------------- global-set-key
;; `define-key' returns DEF, and so does `global-set-key'.
(global-set-key [f13] 'pw59-cmd)
(key-binding [f13])
;; DIVERGED: the Rust subr resolved the keymap BEFORE checking KEY, so a
;; non-array key reported (wrong-type-argument keymapp nil).  GNU checks the
;; key first, at lisp/subr.el:1566.
(condition-case e (global-set-key 42 'pw59-cmd) (error e))
(condition-case e (global-set-key 'f13 'pw59-cmd) (error e))
;; DIVERGED: registered (0 . many), so the datum was the subr, not (2 . 2).
(condition-case e (funcall 'global-set-key "a") (error e))
(condition-case e (funcall 'global-set-key "a" 'b 'c) (error e))
;; -------------------------------------------------------- local-set-key
(with-temp-buffer
  (list (local-set-key "\C-c\C-q" 'pw59-loc)
        (and (current-local-map) t)
        (key-binding "\C-c\C-q")))
(condition-case e (with-temp-buffer (local-set-key 42 'pw59-loc)) (error e))
(condition-case e (funcall 'local-set-key "a") (error e))
;; ------------------------------------------------------------ symbol-file
;; DIVERGED: the Rust subr knew only about autoloads and never consulted
;; `load-history', so every one of these was nil.
(let ((f (symbol-file 'ignore))) (and (stringp f) (file-name-nondirectory f)))
(let ((f (symbol-file 'ignore 'defun))) (and (stringp f) (file-name-nondirectory f)))
(symbol-file 'ignore 'defvar)
(symbol-file 'car)
(symbol-file 42)
(symbol-file 'pw59-no-such-symbol)
;; An autoload: GNU answers (locate-library (nth 1 (symbol-function SYM))),
;; which is nil when that library is not on `load-path'.  DIVERGED -- the Rust
;; subr answered the RAW autoload file string and never called
;; `locate-library' at all.
(progn (autoload 'pw59-sym-file-probe "pw59-sym-file-probe-file")
       (list (symbol-file 'pw59-sym-file-probe)
             (symbol-file 'pw59-sym-file-probe 'defun)
             (symbol-file 'pw59-sym-file-probe 'var)
             (symbol-file 'pw59-sym-file-probe 'defun t)
             (symbol-function 'pw59-sym-file-probe)))
(symbol-file "x")
(symbol-file 'car 1)
(condition-case e (funcall 'symbol-file) (error e))
(condition-case e (funcall 'symbol-file 'ignore nil nil nil) (error e))
;; --------------------------------------------------------- string-match-p
(string-match-p "b" "abc")
(string-match-p "z" "abc")
(string-match-p "b" "abcb" 2)
(progn (string-match "x\\(y\\)" "xy") (string-match-p "b" "abc") (match-data))
(let ((case-fold-search t)) (string-match-p "B" "abc"))
(let ((case-fold-search nil)) (string-match-p "B" "abc"))
(condition-case e (string-match-p "b" 42) (error e))
(condition-case e (string-match-p 42 "abc") (error e))
(condition-case e (string-match-p "b" "abc" 10) (error e))
(string-match-p "b" "abc" -1)
(condition-case e (string-match-p "b" 'abc) (error e))
(condition-case e (funcall 'string-match-p "a") (error e))
(condition-case e (funcall 'string-match-p "a" "a" 0 t) (error e))
;; ----------------------------------------------------- transient-mark-mode
(default-value 'transient-mark-mode)
(list (transient-mark-mode 0) (default-value 'transient-mark-mode))
(list (transient-mark-mode 1) (default-value 'transient-mark-mode))
(list (transient-mark-mode 'toggle) (default-value 'transient-mark-mode))
(list (transient-mark-mode 'toggle) (default-value 'transient-mark-mode))
(list (transient-mark-mode nil) (default-value 'transient-mark-mode))
(list (transient-mark-mode -1) (default-value 'transient-mark-mode))
(list (transient-mark-mode '(4)) (default-value 'transient-mark-mode))
(list (transient-mark-mode 0.5) (default-value 'transient-mark-mode))
;; DIVERGED: the Rust subr set the value and returned; it never ran the hook
;; `define-minor-mode' generates.
(let ((seen nil))
  (let ((transient-mark-mode-hook (list (lambda () (setq seen 'ran)))))
    (transient-mark-mode 1))
  seen)
;; It is a GLOBAL minor mode -- `:variable (default-value ...)' -- so it never
;; makes a buffer-local binding.
(progn (setq-default transient-mark-mode t)
       (with-temp-buffer
         (transient-mark-mode 0)
         (list transient-mark-mode
               (default-value 'transient-mark-mode)
               (local-variable-p 'transient-mark-mode))))
;; DIVERGED: registered (0 . many).
(condition-case e (funcall 'transient-mark-mode 1 2) (error e))
(progn (setq-default transient-mark-mode t) (default-value 'transient-mark-mode))
;; -------------------------------------------- emacs-repository-get-{branch,version}
;; DIVERGED: registered (0 . 0) both, and both answered a constant nil.
(condition-case e (funcall 'emacs-repository-get-version nil nil nil) (error e))
(condition-case e (funcall 'emacs-repository-get-branch nil nil) (error e))
;; ------------------------------------------------ make-auto-save-file-name
(with-temp-buffer
  (setq buffer-file-name "/pw59dir/foo.txt")
  (prog1 (make-auto-save-file-name) (setq buffer-file-name nil)))
;; DIVERGED: the Rust subr also WROTE `buffer-auto-save-file-name'.  GNU's
;; `defun' only returns the name; `auto-save-mode' is what stores it.
(with-temp-buffer
  (setq buffer-file-name "/pw59dir/foo.txt")
  (make-auto-save-file-name)
  (prog1 buffer-auto-save-file-name (setq buffer-file-name nil)))
(condition-case e (funcall 'make-auto-save-file-name 1) (error e))
;; ------------------------------------------------------------ memory-limit
(integerp (memory-limit))
(> (memory-limit) 0)
(condition-case e (funcall 'memory-limit 1) (error e))
;; ------------------------------------------------------------- read-number
;; DIVERGED: registered (1 . 2); GNU's third argument is HIST.
(condition-case e (funcall 'read-number) (error e))
(condition-case e (funcall 'read-number "p" 1 'h 'x) (error e))
(boundp 'read-number-history)
;; ------------------------------------------ set-buffer-file-coding-system
;; DIVERGED four ways: the Rust subr ignored FORCE and NOMODIFY, never merged
;; the previous coding system, and never set
;; `buffer-file-coding-system-explicit'.
(with-temp-buffer
  (set-buffer-modified-p nil)
  (list (set-buffer-file-coding-system 'utf-8-unix)
        buffer-file-coding-system
        (buffer-modified-p)
        buffer-file-coding-system-explicit))
(with-temp-buffer
  (set-buffer-modified-p nil)
  (set-buffer-file-coding-system 'utf-8-unix nil t)
  (list buffer-file-coding-system (buffer-modified-p)))
(with-temp-buffer
  (setq buffer-file-coding-system 'utf-8-dos)
  (set-buffer-file-coding-system 'latin-1 nil t)
  buffer-file-coding-system)
(with-temp-buffer
  (setq buffer-file-coding-system 'utf-8-dos)
  (set-buffer-file-coding-system 'latin-1 t t)
  buffer-file-coding-system)
(with-temp-buffer (set-buffer-file-coding-system nil nil t) buffer-file-coding-system)
(condition-case e
    (with-temp-buffer (set-buffer-file-coding-system 'pw59-no-coding nil t))
  (error e))
(condition-case e (with-temp-buffer (set-buffer-file-coding-system 42 nil t)) (error e))
(with-temp-buffer
  (set-buffer-file-coding-system 'utf-8-unix nil t)
  (set-buffer-file-coding-system 'latin-1 t t)
  buffer-file-coding-system-explicit)
(condition-case e (funcall 'set-buffer-file-coding-system) (error e))
(condition-case e (funcall 'set-buffer-file-coding-system 'utf-8 nil t t) (error e))
;; --------------------------------------------------------- string-greaterp
(string-greaterp "b" "a")
(string-greaterp "a" "b")
(string-greaterp "a" "a")
(string-greaterp "abc" "ab")
(string-greaterp 'b 'a)
(string-greaterp "b" 'a)
(string-greaterp "é" "z")
(condition-case e (string-greaterp 42 "a") (error e))
(condition-case e (string-greaterp "b" ?a) (error e))
(condition-case e (funcall 'string-greaterp "a") (error e))
(funcall 'string-greaterp "b" "a")
"#,
        ),
        vec![
            // ignore
            "OK nil",
            "OK nil",
            "OK nil",
            // global-set-key
            "OK pw59-cmd",
            "OK pw59-cmd",
            "OK (wrong-type-argument arrayp 42)",
            "OK (wrong-type-argument arrayp f13)",
            "OK (wrong-number-of-arguments (2 . 2) 1)",
            "OK (wrong-number-of-arguments (2 . 2) 3)",
            // local-set-key
            "OK (pw59-loc t pw59-loc)",
            "OK (wrong-type-argument arrayp 42)",
            "OK (wrong-number-of-arguments (2 . 2) 1)",
            // symbol-file
            "OK \"subr.elc\"",
            "OK \"subr.elc\"",
            "OK nil",
            "OK nil",
            "OK nil",
            "OK nil",
            "OK (nil nil nil nil (autoload \"pw59-sym-file-probe-file\" nil nil nil))",
            "OK nil",
            "OK nil",
            "OK (wrong-number-of-arguments (1 . 3) 0)",
            "OK (wrong-number-of-arguments (1 . 3) 4)",
            // string-match-p
            "OK 1",
            "OK nil",
            "OK 3",
            "OK (0 2 1 2)",
            "OK 1",
            "OK nil",
            "OK (wrong-type-argument stringp 42)",
            "OK (wrong-type-argument stringp 42)",
            "OK (args-out-of-range \"abc\" 10)",
            "OK nil",
            "OK (wrong-type-argument stringp abc)",
            "OK (wrong-number-of-arguments (2 . 3) 1)",
            "OK (wrong-number-of-arguments (2 . 3) 4)",
            // transient-mark-mode
            "OK nil",
            "OK (nil nil)",
            "OK (t t)",
            "OK (nil nil)",
            "OK (t t)",
            "OK (t t)",
            "OK (nil nil)",
            "OK (t t)",
            "OK (nil nil)",
            "OK ran",
            "OK (nil nil nil)",
            "OK (wrong-number-of-arguments (0 . 1) 2)",
            "OK t",
            // emacs-repository-get-*
            "OK (wrong-number-of-arguments (0 . 2) 3)",
            "OK (wrong-number-of-arguments (0 . 1) 2)",
            // make-auto-save-file-name
            "OK \"/pw59dir/#foo.txt#\"",
            "OK nil",
            "OK (wrong-number-of-arguments (0 . 0) 1)",
            // memory-limit
            "OK t",
            "OK t",
            "OK (wrong-number-of-arguments (0 . 0) 1)",
            // read-number
            "OK (wrong-number-of-arguments (1 . 3) 0)",
            "OK (wrong-number-of-arguments (1 . 3) 4)",
            "OK t",
            // set-buffer-file-coding-system
            "OK (nil utf-8-unix t (nil . utf-8-unix))",
            "OK (utf-8-unix nil)",
            "OK iso-latin-1-dos",
            "OK latin-1",
            "OK nil",
            "OK (coding-system-error pw59-no-coding)",
            "OK (wrong-type-argument symbolp 42)",
            "OK (nil . latin-1)",
            "OK (wrong-number-of-arguments (1 . 3) 0)",
            "OK (wrong-number-of-arguments (1 . 3) 4)",
            // string-greaterp
            "OK t",
            "OK nil",
            "OK nil",
            "OK t",
            "OK t",
            "OK t",
            "OK t",
            "OK (wrong-type-argument stringp 42)",
            "OK (wrong-type-argument stringp 97)",
            "OK (wrong-number-of-arguments (2 . 2) 1)",
            "OK t",
        ],
    );
}
