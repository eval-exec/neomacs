//! GNU `readevalloop' specbinds `macroexp--dynvars' for every evaluated
//! stream (`src/lread.c'), so the top-level `defvar's that eager
//! macroexpansion records there stay local to one `load' or `eval-buffer'.
//! When they leaked, every source file a byte-compilation pulled in added its
//! variable names to the environment of each interpreted closure that a later
//! `eval-and-compile' built, and the compiler embedded those closures -- and
//! their whole environment -- in the `.elc' (diary-icalendar.elc grew by
//! 36 KB of repeated variable names).

use crate::common;

use common::{run_neovm_eval, run_oracle_eval};

fn assert_case(name: &str, form: &str) {
    let gnu = run_oracle_eval(form)
        .unwrap_or_else(|err| panic!("GNU Emacs evaluation failed for {name}: {err}"));
    let neovm = run_neovm_eval(form)
        .unwrap_or_else(|err| panic!("NeoVM evaluation failed for {name}: {err}"));
    assert_eq!(
        neovm, gnu,
        "readevalloop dynvars mismatch for {name}:\nGNU: {gnu}\nNeoVM: {neovm}"
    );
}

/// A nested source `load' sees the caller's `macroexp--dynvars', records its
/// own top-level `defvar's on top, and leaves the caller's value untouched.
/// `eval-and-compile' captures the stream-local list as the closure
/// environment.
#[test]
fn compat_readevalloop_load_keeps_dynvars_local() {
    assert_case(
        "load_keeps_dynvars_local",
        r#"(let* ((dir (file-name-as-directory (make-temp-file "neovm-dynvars-" t)))
       (dep (expand-file-name "neovm-dynvars-dep.el" dir))
       (user (expand-file-name "neovm-dynvars-user.el" dir)))
  (unwind-protect
      (progn
        (with-temp-file dep
          (insert ";;; -*- lexical-binding: t -*-\n"
                  "(defvar neovm-dynvars-dep-a 1)\n"
                  "(defvar neovm-dynvars-dep-b)\n"
                  "(defconst neovm-dynvars-dep-c 3)\n"
                  "(setq neovm-dynvars-dep-seen macroexp--dynvars)\n"))
        (with-temp-file user
          (insert ";;; -*- lexical-binding: t -*-\n"
                  "(defvar neovm-dynvars-user-a 1)\n"
                  "(load " (prin1-to-string dep) " nil t)\n"
                  "(setq neovm-dynvars-after-load macroexp--dynvars)\n"
                  "(defvar neovm-dynvars-user-b 2)\n"
                  "(eval-and-compile (setq neovm-dynvars-closure (lambda (x) x)))\n"))
        (let ((before macroexp--dynvars))
          (load user nil t)
          (list (equal before macroexp--dynvars)
                neovm-dynvars-dep-seen
                neovm-dynvars-after-load
                (aref neovm-dynvars-closure 2))))
    (delete-directory dir t)))"#,
    );
}

/// `eval-buffer' runs its own `readevalloop': the buffer's top-level
/// `defvar's do not survive into the caller's `macroexp--dynvars'.
#[test]
fn compat_readevalloop_eval_buffer_keeps_dynvars_local() {
    assert_case(
        "eval_buffer_keeps_dynvars_local",
        r#"(let ((before macroexp--dynvars))
  (with-temp-buffer
    (setq-local lexical-binding t)
    (insert "(defvar neovm-dynvars-buffer-a 1)\n"
            "(defvar neovm-dynvars-buffer-b)\n"
            "(setq neovm-dynvars-buffer-seen macroexp--dynvars)\n")
    (eval-buffer))
  (list (equal before macroexp--dynvars) neovm-dynvars-buffer-seen))"#,
    );
}

/// The compile-time shape of the bug: a byte-compiled file whose
/// dependencies load from source embeds the interpreted predicate of a
/// `cl-deftype'.  Its environment holds only the variables of the file that
/// defined it, not those of every source file loaded before.
#[test]
fn compat_readevalloop_compiled_closure_env_excludes_other_files() {
    assert_case(
        "compiled_closure_env_excludes_other_files",
        r#"(let* ((_ (require 'bytecomp))
       (dir (file-name-as-directory (make-temp-file "neovm-dynvars-" t)))
       (load-path (cons dir load-path)))
  (unwind-protect
      (progn
        (with-temp-file (expand-file-name "neovm-dynvars-dep-a.el" dir)
          (insert ";;; -*- lexical-binding: t -*-\n"
                  "(defvar neovm-dynvars-dep-a-var 1)\n"
                  "(defconst neovm-dynvars-dep-a-const 2)\n"
                  "(provide 'neovm-dynvars-dep-a)\n"))
        (with-temp-file (expand-file-name "neovm-dynvars-dep-b.el" dir)
          (insert ";;; -*- lexical-binding: t -*-\n"
                  "(require 'cl-lib)\n"
                  "(defvar neovm-dynvars-dep-b-var 1)\n"
                  "(cl-deftype neovm-dynvars-positive ()\n"
                  "  '(and integer (satisfies cl-plusp)))\n"
                  "(provide 'neovm-dynvars-dep-b)\n"))
        (with-temp-file (expand-file-name "neovm-dynvars-user.el" dir)
          (insert ";;; -*- lexical-binding: t -*-\n"
                  "(require 'cl-lib)\n"
                  "(require 'neovm-dynvars-dep-a)\n"
                  "(require 'neovm-dynvars-dep-b)\n"
                  "(defun neovm-dynvars-user-p (x)\n"
                  "  (cl-typep x 'neovm-dynvars-positive))\n"))
        (let ((byte-compile-warnings nil))
          (byte-compile-file (expand-file-name "neovm-dynvars-user.el" dir)))
        (load (expand-file-name "neovm-dynvars-user.elc" dir) nil t)
        (let ((f (symbol-function 'neovm-dynvars-user-p)))
          (list (byte-code-function-p f)
                (mapcar (lambda (c) (list (aref c 0) (aref c 1) (aref c 2)))
                        (seq-filter #'interpreted-function-p
                                    (append (aref f 2) nil)))
                (funcall f 3)
                (funcall f -1))))
    (setq features (delq 'neovm-dynvars-dep-a
                         (delq 'neovm-dynvars-dep-b features)))
    (delete-directory dir t)))"#,
    );
}

/// `eval-region' runs a `readevalloop' too: the region starts from a fresh
/// interpreter environment (the caller's lexical variables are not visible)
/// and its top-level `defvar's stay out of the caller's `macroexp--dynvars'.
#[test]
fn compat_readevalloop_eval_region_opens_fresh_scope() {
    assert_case(
        "eval_region_opens_fresh_scope",
        r#"(progn
  (defvar neovm-dynvars-region-seen nil)
  (defvar neovm-dynvars-region-lexical nil)
  (let ((before macroexp--dynvars)
        (neovm-dynvars-lexical-probe 7))
    (ignore neovm-dynvars-lexical-probe)
    (with-temp-buffer
      (setq-local lexical-binding t)
      (insert "(defvar neovm-dynvars-region-a 1)\n"
              "(setq neovm-dynvars-region-seen macroexp--dynvars)\n"
              "(setq neovm-dynvars-region-lexical\n"
              "      (condition-case nil neovm-dynvars-lexical-probe\n"
              "        (void-variable 'void)))\n")
      (eval-region (point-min) (point-max)))
    (list (equal before macroexp--dynvars)
          neovm-dynvars-region-seen
          neovm-dynvars-region-lexical)))"#,
    );
}
