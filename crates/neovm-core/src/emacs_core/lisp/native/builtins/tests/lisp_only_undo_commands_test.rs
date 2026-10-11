//! `undo` and `buffer-disable-undo` are Lisp, and only Lisp --
//! DIVERGENCES.md 150.
//!
//! GNU's undo machinery is split, and the split is not where the names
//! suggest.  `syms_of_undo` (src/undo.c:423-490) has exactly one `defsubr`,
//! `&Sundo_boundary` (:435, DEFUN at src/undo.c:251).  `buffer-enable-undo`
//! IS in C -- `DEFUN ("buffer-enable-undo", ...)` at src/buffer.c:1829 -- but
//! its partner `buffer-disable-undo` is not: it is
//! `(defun buffer-disable-undo (&optional buffer) ...)` at
//! lisp/simple.el:3591.  `undo` is likewise Lisp only,
//! `(defun undo (&optional arg) ...)` at lisp/simple.el:3466.
//! `grep 'DEFUN ("undo"' src/*.c` and `grep 'DEFUN ("buffer-disable-undo"'
//! src/*.c` against emacs-mirror 31.0.90 (0ee48ac4df2) find nothing.
//!
//! So the pair a reader expects to be symmetric is not, and the asymmetry is
//! the whole point: `buffer-enable-undo` must stay a Rust subr and
//! `buffer-disable-undo` must not exist as one.
//!
//! `rust_subrs_shadowed_by_lisp_test.rs` is the scan that finds new shadows;
//! this is the per-name statement for the two entry 150 deleted, and the
//! statement that the C one is still here.

use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::lookup_global_subr_entry;
use crate::emacs_core::intern::intern;
use crate::test_utils::{runtime_startup_eval_all, runtime_startup_eval_one};

/// GNU has no C version of these, so a bare evaluator -- which is GNU before
/// `loadup.el` -- must have nothing to answer with.
const LISP_ONLY_UNDO_COMMANDS: &[&str] = &[
    "buffer-disable-undo", // lisp/simple.el:3591
    "undo",                // lisp/simple.el:3466
];

/// The two names GNU really does implement in C, next door to them.
const C_UNDO_PRIMITIVES: &[&str] = &[
    "buffer-enable-undo", // src/buffer.c:1829
    "undo-boundary",      // src/undo.c:251
];

#[test]
fn the_two_undo_commands_are_void_on_a_bare_evaluator_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    for primitive in C_UNDO_PRIMITIVES {
        let result = eval.eval_str(&format!("(fboundp '{primitive})"));
        assert_eq!(
            crate::emacs_core::error::format_eval_result_with_eval(&eval, &result),
            "OK t",
            "{primitive} is DEFUN'ed in GNU src/ and must remain a subr",
        );
    }

    for name in LISP_ONLY_UNDO_COMMANDS {
        let result = eval.eval_str(&format!("(fboundp '{name})"));
        assert_eq!(
            crate::emacs_core::error::format_eval_result_with_eval(&eval, &result),
            "OK nil",
            "{name} must be void before lisp/simple.el loads: GNU's src/ has \
             no DEFUN of that name, so a bare evaluator has nothing to answer \
             with",
        );
    }
}

#[test]
fn no_rust_subr_is_registered_for_the_two_undo_commands() {
    crate::test_utils::init_test_tracing();
    // The global subr registry is populated by `init_builtins`, which runs
    // when an evaluator is built; ask for one before reading the table.
    let _eval = Context::new();
    for name in LISP_ONLY_UNDO_COMMANDS {
        assert!(
            lookup_global_subr_entry(intern(name)).is_none(),
            "{name} must have no Rust subr: GNU implements it in \
             lisp/simple.el and nowhere in src/",
        );
    }
    for name in C_UNDO_PRIMITIVES {
        assert!(
            lookup_global_subr_entry(intern(name)).is_some(),
            "{name} IS a C DEFUN in GNU and must stay registered here",
        );
    }
}

/// Every observable a Lisp caller can ask about the two names, measured on
/// GNU 31.0.90 `-Q --batch` first (tmp/pw56-observables-gnu.txt).
///
/// The Rust subrs got five of these wrong even though their answers were
/// often right: `subrp` was `t`, `documentation` was the generic
/// "Built-in function.", `interactive-form` was nil and `commandp` was nil --
/// so neither name was a command at all, which is what `M-x undo` and
/// `C-/` need.
#[test]
fn the_two_undo_commands_are_lisp_defuns_in_the_loaded_runtime_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        runtime_startup_eval_all(
            "(subrp (symbol-function 'undo))
             (func-arity 'undo)
             (and (commandp 'undo) t)
             (interactive-form 'undo)
             (car (split-string (documentation 'undo) \"\\n\"))
             (subrp (symbol-function 'buffer-disable-undo))
             (func-arity 'buffer-disable-undo)
             (and (commandp 'buffer-disable-undo) t)
             (interactive-form 'buffer-disable-undo)
             (car (split-string (documentation 'buffer-disable-undo) \"\\n\"))
             ;; The C half of the pair is untouched.
             (subrp (symbol-function 'buffer-enable-undo))
             (func-arity 'buffer-enable-undo)
             (subrp (symbol-function 'undo-boundary))",
        ),
        vec![
            "OK nil",
            "OK (0 . 1)",
            "OK t",
            "OK (interactive \"*P\")",
            "OK \"Undo some previous changes.\"",
            "OK nil",
            "OK (0 . 1)",
            "OK t",
            "OK (interactive nil)",
            "OK \"Make BUFFER stop keeping undo information.\"",
            "OK t",
            "OK (0 . 1)",
            "OK t",
        ],
    );
}

/// Neither name carries a `byte-compile` property or a `compiler-macro`, so
/// a compiled caller emits an ordinary `Bcall1` and reads the function cell.
///
/// That is the reason the shadow mattered: unlike entry 148's `not` and
/// `string=`, which compile to opcodes and never look anything up, a compiled
/// `(undo n)` goes through the cell -- and would have reached the Rust subr
/// the moment anything failed to load `simple.el`.
///
/// 192 = Bconstant, 1 = Bstack_ref1, 33 = Bcall1, 32 = Bcall, 135 = Breturn.
/// Measured on GNU 31.0.90 with `lexical-binding' t (tmp/pw56-opcodes-gnu.txt).
#[test]
fn byte_compiled_callers_of_the_two_undo_commands_read_the_cell_like_gnu() {
    crate::test_utils::init_test_tracing();
    for (form, codes, constants) in [
        ("(lambda (x) (undo x))", "(192 1 33 135)", "[undo]"),
        (
            "(lambda (b) (buffer-disable-undo b))",
            "(192 1 33 135)",
            "[buffer-disable-undo]",
        ),
        (
            "(lambda (b) (buffer-enable-undo b))",
            "(192 1 33 135)",
            "[buffer-enable-undo]",
        ),
        (
            "(lambda () (undo-boundary))",
            "(192 32 135)",
            "[undo-boundary]",
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

/// What a Lisp caller observes from `undo`, measured under GNU 31.0.90
/// `-Q --batch` first (tmp/pw56-undo-behaviour-gnu.txt,
/// tmp/pw56-undo-mirror-gnu.txt) and re-asked of the runtime, where
/// lisp/simple.el's `undo` is what replies.
///
/// Rows marked DIVERGED are ones `BufferManager::undo_buffer` -- the third
/// undo replay loop, reachable only through the deleted Rust `undo` subr --
/// got wrong.
#[test]
fn undo_arms_match_gnu() {
    crate::test_utils::init_test_tracing();

    let results = runtime_startup_eval_all(
        r#"
;; An `apply' entry is funcalled.  DIVERGED: the buffer layer has no
;; evaluator, so `undo_buffer' could not run one and skipped it silently.
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abc")
  (setq buffer-undo-list (list nil (list 'apply 'set-buffer-multibyte nil) nil))
  (setq last-command nil)
  (undo)
  enable-multibyte-characters)
;; The (apply DELTA BEG END FUN . ARGS) shape too.  DIVERGED the same way.
(with-temp-buffer
  (buffer-enable-undo)
  (let ((ran nil))
    (fset 'neo-undo-apply-probe (lambda (&rest a) (setq ran (cons 'ran a))))
    (insert "abc")
    (setq buffer-undo-list (list (list 'apply 0 1 4 'neo-undo-apply-probe 'x)))
    (undo-boundary)
    (setq last-command nil)
    (undo)
    ran))
;; `undo' does NOT consume buffer-undo-list: it walks `pending-undo-list' and
;; PUSHES redo records, so the history it just replayed is still there, with
;; its boundary and its first-change entry.  DIVERGED, and this is the one
;; that mattered: `undo_buffer' popped groups off buffer-undo-list
;; destructively, so the history was gone and a redo was impossible.
(with-temp-buffer
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (insert "one")
  (setq buffer-undo-list (cons nil buffer-undo-list))
  (setq last-command nil)
  (undo)
  (list (buffer-string) buffer-undo-list))
;; `pending-undo-list' is the cursor `undo' leaves behind.  DIVERGED: the
;; variable did not exist at all on the path the Rust subr took.
(with-temp-buffer
  (buffer-enable-undo)
  (insert "one")
  (undo-boundary)
  (insert "two")
  (undo-boundary)
  (setq last-command nil)
  (undo)
  (list (buffer-string) (listp pending-undo-list) (eq pending-undo-list t)))
;; `undo-equiv-table' gets the redo mapping.  DIVERGED: never touched.
(with-temp-buffer
  (buffer-enable-undo)
  (insert "one")
  (undo-boundary)
  (insert "two")
  (undo-boundary)
  (setq last-command nil)
  (undo)
  (> (hash-table-count undo-equiv-table) 0))
;; Two consecutive undos peel two groups when the previous command was `undo'.
(with-temp-buffer
  (buffer-enable-undo)
  (insert "one") (undo-boundary)
  (insert "two") (undo-boundary)
  (insert "three") (undo-boundary)
  (setq last-command nil)
  (undo)
  (let ((a (buffer-string)))
    (setq last-command 'undo)
    (undo)
    (list a (buffer-string))))
;; Undoing an undo redoes it -- what `undo-equiv-table' buys.  DIVERGED: the
;; Rust loop answered ("one" "two"), re-applying its own redo record because
;; it had no notion of an undo chain.
(with-temp-buffer
  (buffer-enable-undo)
  (insert "one") (undo-boundary)
  (insert "two") (undo-boundary)
  (setq last-command nil)
  (undo)
  (let ((a (buffer-string)))
    (setq last-command nil)
    (undo)
    (list a (buffer-string))))
;; `this-command' is set to `undo', which is how the NEXT command knows to
;; continue the chain.  DIVERGED: left alone.
(with-temp-buffer
  (buffer-enable-undo)
  (insert "one") (undo-boundary)
  (setq last-command nil this-command nil)
  (undo)
  this-command)
;; Return value.
(with-temp-buffer
  (buffer-enable-undo)
  (insert "one") (undo-boundary)
  (setq last-command nil)
  (undo))
;; The two error messages, which the Rust subr had SWAPPED.
(with-temp-buffer
  (setq buffer-undo-list t)
  (insert "one")
  (setq last-command nil)
  (condition-case e (undo) (error e)))
(with-temp-buffer
  (buffer-enable-undo)
  (setq last-command nil)
  (condition-case e (undo) (error e)))
;; ARG reaches `prefix-numeric-value', so a float is legal.  DIVERGED:
;; `(wrong-type-argument integerp 1.5)'.
(with-temp-buffer
  (buffer-enable-undo)
  (insert "one") (undo-boundary)
  (setq last-command nil)
  (undo 1.5)
  (buffer-string))
;; Arity, as a `defun' reports it.  DIVERGED: the subr's own object.
(condition-case e (funcall 'undo 2 3) (error e))
;; A non-numeric, non-nil ARG asks for undo-in-region; it is NOT a count.
;; DIVERGED: `(wrong-type-argument integerp (4))'.
(with-temp-buffer
  (buffer-enable-undo)
  (insert "onetwo") (undo-boundary)
  (setq last-command nil)
  (condition-case e (undo '(4)) (error e)))
;; A point entry is `goto-char'.
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcdef")
  (goto-char 2)
  (setq buffer-undo-list (list nil 5 (cons "XY" 3) nil))
  (setq last-command nil)
  (undo)
  (list (buffer-string) (point)))
;; `undo' strips point entries out of the redo records it generates, so
;; undoing the undo moves point to the change instead of to a stale position.
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcdef") (undo-boundary)
  (setq last-command nil)
  (undo)
  (let ((n 0))
    (dolist (e buffer-undo-list) (when (integerp e) (setq n (1+ n))))
    n))
;; An entry of no known shape is an error.  DIVERGED: silently skipped, and
;; `undo' returned "Undo" as if it had worked.
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abc")
  (setq buffer-undo-list (list nil (vector 1 2) nil))
  (setq last-command nil)
  (condition-case e (undo) (error e)))
;; A marker-adjustment entry and a text-property entry: these two the Rust
;; loop happened to agree on.
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcdef")
  (let ((mk (copy-marker 4)))
    (setq buffer-undo-list (list nil (cons "cd" 3) (cons mk 2) nil))
    (setq last-command nil)
    (undo)
    (list (buffer-string) (marker-position mk))))
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcd")
  (put-text-property 2 4 'face 'bold)
  (setq buffer-undo-list (list nil '(nil face nil 2 . 4) nil))
  (setq last-command nil)
  (undo)
  (list (get-text-property 1 'face) (get-text-property 2 'face)
        (get-text-property 3 'face)))
"#,
    );

    // Transcribed from GNU Emacs 31.0.90 -Q --batch.
    assert_eq!(
        results,
        vec![
            "OK nil",
            "OK (ran x)",
            "OK (\"\" ((\"one\" . 1) nil (1 . 4) (t . 0)))",
            "OK (\"one\" t nil)",
            "OK t",
            "OK (\"onetwo\" \"one\")",
            "OK (\"one\" \"one\")",
            "OK undo",
            "OK \"Undo\"",
            "OK (user-error \"No undo information in this buffer\")",
            "OK (user-error \"No further undo information\")",
            "OK \"\"",
            "OK (wrong-number-of-arguments (0 . 1) 2)",
            "OK (error \"The mark is not set now, so there is no region\")",
            "OK (\"abXYcdef\" 3)",
            "OK 0",
            "OK (error \"Unrecognized entry in undo list [1 2]\")",
            "OK (\"abcdcdef\" 6)",
            "OK (nil nil nil)",
        ],
    );
}

/// The `(t . MODTIME)` arm reached through `undo`, which is the arm ledger
/// 145 fixed the RECORDING half of.
///
/// `undo_buffer` skipped `(t . MODTIME)` entirely -- the arm 145 had just
/// taught to record the base buffer's modtime was read by nobody on this
/// path, so undoing back to the saved text through `undo` never cleared the
/// modified flag.
#[test]
fn undo_clears_the_modified_flag_through_the_first_change_entry_like_gnu() {
    crate::test_utils::init_test_tracing();

    let dir = std::env::temp_dir().join(format!("neo-undo-modtime-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let dir = dir.to_string_lossy().to_string();

    let results = runtime_startup_eval_all(&format!(
        r#"
;; A buffer visiting a real file: the recorded modtime equals the buffer's,
;; so `undo' clears the modified flag on the way back to the saved text.
(let ((f "{dir}/a.txt"))
  (with-temp-file f (insert "hello\n"))
  (with-current-buffer (find-file-noselect f)
    (let ((mt (visited-file-modtime)))
      (insert "X")
      (set-buffer-modified-p t)
      (setq buffer-undo-list (list nil (cons 2 3) (cons t mt) nil))
      (setq last-command nil)
      (undo)
      (prog1 (list (buffer-string) (buffer-modified-p))
        (set-buffer-modified-p nil) (kill-buffer)))))
;; A buffer visiting no file has modtime 0, which a recorded 0 matches.
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcdef")
  (set-buffer-modified-p t)
  (setq buffer-undo-list (list nil (cons 2 3) (cons t 0) nil))
  (setq last-command nil)
  (undo)
  (list (buffer-string) (buffer-modified-p)))
"#
    ));

    // Transcribed from GNU Emacs 31.0.90 -Q --batch.
    assert_eq!(results, vec!["OK (\"Xello\n\" nil)", "OK (\"acdef\" nil)"]);

    let _ = std::fs::remove_dir_all(&dir);
}

/// `buffer-disable-undo`, whose Rust subr got the ANSWERS right and the
/// SHAPE wrong.
///
/// Every arm below agreed with GNU before the deletion -- the return value,
/// the buffer designators, all three refusals.  What did not agree was
/// `subrp`, `documentation`, `interactive-form`, `commandp` and the
/// `wrong-number-of-arguments` datum, which the test above pins.  This one
/// pins the answers so the deletion is provably not a behaviour change.
#[test]
fn buffer_disable_undo_arms_match_gnu() {
    crate::test_utils::init_test_tracing();

    let results = runtime_startup_eval_all(
        r#"
;; `(setq buffer-undo-list t)' is the body, so the return value is t.
(with-temp-buffer (buffer-enable-undo) (buffer-disable-undo))
(with-temp-buffer (buffer-enable-undo) (buffer-disable-undo) buffer-undo-list)
(with-temp-buffer (buffer-enable-undo) (buffer-disable-undo nil) buffer-undo-list)
;; A buffer NAME and a buffer OBJECT both work, through `get-buffer'.
(let ((b (generate-new-buffer "neo-bdu")))
  (with-current-buffer b (buffer-enable-undo))
  (buffer-disable-undo (buffer-name b))
  (prog1 (with-current-buffer b buffer-undo-list) (kill-buffer b)))
(let ((b (generate-new-buffer "neo-bdu2")))
  (with-current-buffer b (buffer-enable-undo))
  (buffer-disable-undo b)
  (prog1 (with-current-buffer b buffer-undo-list) (kill-buffer b)))
;; `with-current-buffer' restores the caller's buffer.
(let ((b (generate-new-buffer "neo-bdu3")))
  (with-temp-buffer
    (rename-buffer "neo-bdu-caller" t)
    (buffer-disable-undo b)
    (prog1 (buffer-name) (kill-buffer b))))
;; A name with no buffer: `get-buffer' answers nil and `set-buffer' refuses.
(condition-case e (buffer-disable-undo "neo-bdu-no-such-buffer") (error e))
;; A killed buffer object.
(let ((b (generate-new-buffer "neo-bdu4")))
  (kill-buffer b)
  (condition-case e (buffer-disable-undo b) (error e)))
;; Anything that is neither: `get-buffer' is the one that refuses, so the
;; datum is `stringp' and the value is what was passed.
(condition-case e (buffer-disable-undo 42) (error e))
(condition-case e (buffer-disable-undo 'neo-bdu-sym) (error e))
(condition-case e (funcall 'buffer-disable-undo nil nil) (error e))
;; Disabling undo through an INDIRECT buffer disables the base buffer's too,
;; because GNU copies `undo_list' between the two on every
;; `set_buffer_internal_1' (src/buffer.c:2357,2367) -- ledger 120's machinery,
;; asked through this door.
(let ((base (generate-new-buffer "neo-bdu-base")))
  (with-current-buffer base (buffer-enable-undo) (insert "hello"))
  (let ((ind (make-indirect-buffer base "neo-bdu-ind")))
    (with-current-buffer ind (buffer-enable-undo))
    (buffer-disable-undo ind)
    (prog1 (list (with-current-buffer ind buffer-undo-list)
                 (eq t (with-current-buffer base buffer-undo-list)))
      (kill-buffer ind) (kill-buffer base))))
"#,
    );

    // Transcribed from GNU Emacs 31.0.90 -Q --batch.
    assert_eq!(
        results,
        vec![
            "OK t",
            "OK t",
            "OK t",
            "OK t",
            "OK t",
            "OK \"neo-bdu-caller\"",
            "OK (wrong-type-argument stringp nil)",
            "OK (error \"Selecting deleted buffer\")",
            "OK (wrong-type-argument stringp 42)",
            "OK (wrong-type-argument stringp neo-bdu-sym)",
            "OK (wrong-number-of-arguments (0 . 1) 2)",
            "OK (t t)",
        ],
    );
}
