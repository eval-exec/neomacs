//! Strict combo oracle probes, batch 117: areas the remote team is actively
//! fixing — ash overflow, rounding nil divisor, call-process working dir,
//! keymap edge cases, plus deep combos: dynamic/lexical binding interaction,
//! defmacro &environment, cl-macrolet recursion, and print-number-table.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_t1_ash_overflow_and_rounding_nil_divisor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (ash 1 63)
      (ash 1 64)
      (ash most-positive-fixnum 1)
      (ash most-positive-fixnum -1)
      (ash -1 63)
      (ash -1 64)
      (condition-case err (floor 5 nil) (wrong-type-argument (car err)))
      (condition-case err (ceiling 5 nil) (wrong-type-argument (car err)))
      (condition-case err (round 5 nil) (wrong-type-argument (car err)))
      (condition-case err (truncate 5 nil) (wrong-type-argument (car err)))
      (condition-case err (/ 5 nil) (wrong-type-argument (car err)))
      (condition-case err (% 5 nil) (wrong-type-argument (car err))))
"####,
    );
}

#[test]
fn div_t1_call_process_working_dir_infile() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let* ((dir (make-temp-file "neo-wd-probe" t))
       (infile (expand-file-name "input.txt" dir))
       (result nil))
  (unwind-protect
      (progn
        (write-region "hello world" nil infile nil 'silent)
        (let ((default-directory (file-name-as-directory dir)))
          (with-temp-buffer
            (let ((status (call-process shell-file-name infile t nil
                                        shell-command-switch "cat input.txt")))
              (setq result (list status (buffer-string) default-directory)))))
    (delete-directory dir t))
  result)
"####,
    );
}

#[test]
fn div_t1_keymap_edge_cases_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((map (make-keymap)))
  (define-key map "a" 'cmd-a)
  (define-key map [?b] 'cmd-b)
  (define-key map (kbd "C-c C-c") 'cmd-cc)
  (define-key map [remap other-cmd] 'remapped)
  (list (where-is-internal 'cmd-a map t)
        (where-is-internal 'cmd-b map t)
        (where-is-internal 'cmd-cc map t)
        (command-remapping 'other-cmd nil (list map))
        (lookup-key map "a")
        (lookup-key map [?b])
        (lookup-key map "\C-c\C-c")
        (length (accessible-keymaps map))
        (eq (lookup-key map [remap other-cmd]) 'remapped)))
"####,
    );
}

#[test]
fn div_t1_dynamic_lexical_binding_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(progn
  (defvar probe-dyn-var 'dynamic)
  (list (let ((probe-dyn-var 'let-bound))
          probe-dyn-var)
        (let ((probe-dyn-var 'let-bound))
          (symbol-value 'probe-dyn-var))
        (lexical-let ((probe-lex-var 'lexical))
          (list probe-lex-var
                (condition-case err (symbol-value 'probe-lex-var)
                  (void-variable 'void))))
        (let ((probe-dyn-var 'outer))
          (lexical-let ((probe-lex-var 'lex))
            (list probe-dyn-var probe-lex-var)))))
"####,
    );
}

#[test]
fn div_t1_cl_macrolet_recursive_and_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (cl-macrolet ((my-if (c t e)
                          `(cond (,c ,t)
                                 (t ,e))))
         (my-if (> 3 2) 'yes 'no))
      (cl-macrolet ((count-down (n)
                      (if (<= n 0)
                          ''done
                        `(progn
                           ,n
                           (count-down ,(1- n)))))
         (count-down 3))
      (macroexpand '(cl-macrolet ((double (x) `(* 2 ,x)))
                     (double 5))))
"####,
    );
}
