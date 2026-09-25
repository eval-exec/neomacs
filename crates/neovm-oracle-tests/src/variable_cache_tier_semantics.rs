//! Oracle pins for the cached variable tiers (P1.4 Stage A): byte-compiled
//! `varref`, `varset`, `varbind` and `unbind` of buffer-local (GNU
//! `SYMBOL_LOCALIZED`) and forwarded (`SYMBOL_FORWARDED`) variables.
//!
//! Neomacs answers each of these through a cache-hit fast tier when the BLV
//! cache or the forwarder already holds the answer, and through the general
//! path (GNU `find_symbol_value`, `set_internal`, `specbind`, `do_one_unbind`)
//! otherwise.  Every form below is checked against GNU once and against
//! Neomacs under a knob matrix -- the default JIT, every function compiled
//! (`NEOVM_JIT_THRESHOLD=1`), and each of those with the fast tiers switched
//! off (`NEOVM_VAR_CACHE=0`) -- so a tier that answers differently from the
//! path it shortcuts cannot hide behind the configuration a run happens to
//! use.
//!
//! Regenerate the expectations from GNU only:
//! `NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1 cargo nextest run -p neovm-oracle-tests -E 'test(variable_cache_tier)'`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// The Neomacs configurations every form must answer identically under.
const KNOB_MATRIX: &[&[(&str, &str)]] = &[
    &[],
    &[("NEOVM_JIT_THRESHOLD", "1")],
    &[("NEOVM_VAR_CACHE", "0")],
    &[("NEOVM_VAR_CACHE", "0"), ("NEOVM_JIT_THRESHOLD", "1")],
];

/// Reads and `setq`s of buffer-local variables from byte code, in the buffer
/// that has the local binding, in one that does not, and after the binding
/// is killed; `make-variable-buffer-local` auto-creates on the first `setq`.
#[test]
fn oracle_bytecode_reads_and_setqs_of_buffer_local_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defvar vco-loc 1)
  (defvar vco-auto 2)
  (make-variable-buffer-local 'vco-auto)
  (defalias 'vco-get-loc (byte-compile (lambda () vco-loc)))
  (defalias 'vco-put-loc (byte-compile (lambda (v) (setq vco-loc v) nil)))
  (defalias 'vco-get-auto (byte-compile (lambda () vco-auto)))
  (defalias 'vco-put-auto (byte-compile (lambda (v) (setq vco-auto v) nil)))
  (let ((a (generate-new-buffer " vco-a"))
        (b (generate-new-buffer " vco-b"))
        (out nil))
    (unwind-protect
        (progn
          (with-current-buffer a
            (set (make-local-variable 'vco-loc) 10)
            (vco-put-loc 11)
            (push (list (vco-get-loc) (default-value 'vco-loc)) out)
            (vco-put-auto 20)
            (push (list (vco-get-auto) (local-variable-p 'vco-auto)
                        (default-value 'vco-auto))
                  out))
          (with-current-buffer b
            (push (vco-get-loc) out)
            (vco-put-loc 12)
            (push (list (vco-get-loc) (default-value 'vco-loc)
                        (buffer-local-value 'vco-loc a))
                  out)
            (vco-put-auto 21)
            (push (list (vco-get-auto) (buffer-local-value 'vco-auto a)
                        (default-value 'vco-auto))
                  out))
          (with-current-buffer a
            (push (list (vco-get-loc) (vco-get-auto)) out)
            (kill-local-variable 'vco-loc)
            (push (vco-get-loc) out)
            (vco-put-loc 13)
            (push (list (vco-get-loc) (default-value 'vco-loc)
                        (local-variable-p 'vco-loc))
                  out)))
      (kill-buffer a)
      (kill-buffer b))
    (nreverse out)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((11 1) (20 t 2) 1 (12 12 11) (21 20 2) (11 20) 12 (13 13 nil))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

/// `let` of a buffer-local variable from byte code: `SPECPDL_LET_LOCAL` when
/// the buffer has a binding, `SPECPDL_LET_DEFAULT` when it does not, the
/// restore into the binding's own buffer when the body switches buffers, the
/// kill winning over the restore, nesting, `kill-all-local-variables`, and a
/// `setq` under a default binding (GNU `let_shadows_buffer_binding_p`: no
/// auto-created local).
#[test]
fn oracle_bytecode_let_of_buffer_local_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defvar vco-l 1)
  (defvar vco-m 5)
  (make-variable-buffer-local 'vco-m)
  (defalias 'vco-let-l
    (byte-compile (lambda (v f) (let ((vco-l v)) (funcall f)))))
  (defalias 'vco-let2-l
    (byte-compile (lambda (v w f) (let ((vco-l v)) (let ((vco-l w)) (funcall f))))))
  (defalias 'vco-let-m-setq
    (byte-compile (lambda (v w f) (let ((vco-m v)) (setq vco-m w) (funcall f)))))
  (let ((a (generate-new-buffer " vco-a"))
        (b (generate-new-buffer " vco-b"))
        (out nil))
    (unwind-protect
        (with-current-buffer a
          (set (make-local-variable 'vco-l) 10)
          (push (list (vco-let-l 11 (lambda () (list vco-l (default-value 'vco-l))))
                      vco-l (default-value 'vco-l))
                out)
          (push (list (vco-let-l 12 (lambda ()
                                      (set-buffer b)
                                      (list vco-l (buffer-local-value 'vco-l a))))
                      (eq (current-buffer) b)
                      (buffer-local-value 'vco-l a))
                out)
          (set-buffer a)
          (push (list (vco-let-l 13 (lambda () (kill-local-variable 'vco-l) vco-l))
                      vco-l (local-variable-p 'vco-l))
                out)
          (push (list (vco-let-l 14 (lambda ()
                                      (list vco-l (default-value 'vco-l)
                                            (with-current-buffer b vco-l))))
                      vco-l (default-value 'vco-l))
                out)
          (push (list (vco-let-l 15 (lambda () (set (make-local-variable 'vco-l) 16) vco-l))
                      vco-l (default-value 'vco-l) (local-variable-p 'vco-l))
                out)
          (kill-local-variable 'vco-l)
          (set (make-local-variable 'vco-l) 17)
          (push (list (vco-let2-l 18 19 (lambda () vco-l)) vco-l) out)
          (push (list (vco-let-l 20 (lambda () (kill-all-local-variables) vco-l))
                      vco-l (local-variable-p 'vco-l))
                out)
          (push (list (vco-let-m-setq 6 7 (lambda ()
                                            (list vco-m (default-value 'vco-m)
                                                  (local-variable-p 'vco-m))))
                      vco-m (local-variable-p 'vco-m) (default-value 'vco-m))
                out))
      (kill-buffer a)
      (kill-buffer b))
    (nreverse out)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((11 1) 10 1) ((1 12) t 10) (1 1 nil) ((14 14 14) 1 1) (16 16 1 t) (19 17) (1 1 nil) ((7 7 nil) 5 nil 5))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

/// Variable watchers see every byte-compiled write, bind and unbind of a
/// buffer-local variable with the right operation and WHERE, including an
/// unbind of a binding made before the watcher was added.
#[test]
fn oracle_bytecode_var_ops_notify_watchers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defvar vco-w 1)
  (defvar vco-x 1)
  (defvar vco-log nil)
  (defun vco-watch (sym newval op where)
    (push (list sym op newval (if (bufferp where) (buffer-name where) where))
          vco-log))
  (add-variable-watcher 'vco-w #'vco-watch)
  (defalias 'vco-let-w (byte-compile (lambda (v f) (let ((vco-w v)) (funcall f)))))
  (defalias 'vco-put-w (byte-compile (lambda (v) (setq vco-w v) nil)))
  (defalias 'vco-let-x (byte-compile (lambda (v f) (let ((vco-x v)) (funcall f)))))
  (prog1
      (with-temp-buffer
        (rename-buffer " vco-t")
        (vco-put-w 2)
        (vco-let-w 3 (lambda () vco-w))
        (set (make-local-variable 'vco-w) 4)
        (vco-put-w 5)
        (vco-let-w 6 (lambda () vco-w))
        (make-local-variable 'vco-x)
        (vco-let-x 7 (lambda () (add-variable-watcher 'vco-x #'vco-watch) vco-x))
        (nreverse vco-log))
    (remove-variable-watcher 'vco-w #'vco-watch)
    (remove-variable-watcher 'vco-x #'vco-watch)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((vco-w set 2 nil) (vco-w let 3 nil) (vco-w unlet 2 nil) (vco-w set 4 \" vco-t\") (vco-w set 5 \" vco-t\") (vco-w let 6 \" vco-t\") (vco-w unlet 5 \" vco-t\") (vco-x unlet 1 \" vco-t\"))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

/// Built-in forwarded variables from byte code: a `DEFVAR_BOOL` binds `t`
/// for any non-nil value, a `DEFVAR_INT` refuses a string before the body
/// runs and keeps its value, a `DEFVAR_LISP` binds and restores anything, and
/// `case-fold-search` (automatically buffer-local) binds its default when the
/// buffer has no local value and its local value when it has one.
#[test]
fn oracle_bytecode_var_ops_on_forwarded_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defun vco-opaque () nil)
  (defalias 'vco-let-itm
    (byte-compile (lambda (v) (let ((indent-tabs-mode v)) (vco-opaque) indent-tabs-mode))))
  (defalias 'vco-let-gct
    (byte-compile (lambda (v) (let ((gc-cons-threshold v)) (vco-opaque) gc-cons-threshold))))
  (defalias 'vco-let-iro
    (byte-compile (lambda (v f) (let ((inhibit-read-only v)) (funcall f)))))
  (defalias 'vco-put-iro (byte-compile (lambda (v) (setq inhibit-read-only v) nil)))
  (defalias 'vco-get-iro (byte-compile (lambda () inhibit-read-only)))
  (defalias 'vco-let-cfs
    (byte-compile (lambda (v f) (let ((case-fold-search v)) (funcall f)))))
  (defalias 'vco-put-cfs (byte-compile (lambda (v) (setq case-fold-search v) nil)))
  (defalias 'vco-get-cfs (byte-compile (lambda () case-fold-search)))
  (let ((gct gc-cons-threshold)
        (out nil))
    (with-temp-buffer
      (push (vco-let-itm 5) out)
      (push (vco-let-itm nil) out)
      (push (condition-case e (vco-let-gct "x") (error (car e))) out)
      (push (= gc-cons-threshold gct) out)
      (push (= (vco-let-gct (+ gct 1)) (+ gct 1)) out)
      (push (= gc-cons-threshold gct) out)
      (push (vco-let-iro 'x (lambda () (list inhibit-read-only (vco-get-iro)))) out)
      (push inhibit-read-only out)
      (vco-put-iro 'y)
      (push (vco-get-iro) out)
      (vco-put-iro nil)
      (push (list (vco-get-cfs) (local-variable-p 'case-fold-search)) out)
      (push (vco-let-cfs 'let1 (lambda ()
                                 (list case-fold-search
                                       (default-value 'case-fold-search)
                                       (local-variable-p 'case-fold-search))))
            out)
      (push (list (vco-get-cfs) (local-variable-p 'case-fold-search)) out)
      (vco-put-cfs nil)
      (push (list (vco-get-cfs) (local-variable-p 'case-fold-search)
                  (default-value 'case-fold-search))
            out)
      (push (vco-let-cfs 'let2 (lambda ()
                                 (list case-fold-search
                                       (default-value 'case-fold-search))))
            out)
      (push (list (vco-get-cfs) (default-value 'case-fold-search)) out))
    (nreverse out)))
"#;
    let expect = expect_test::expect![[
        r#""OK (t nil wrong-type-argument t t t (x x) nil y (t nil) (let1 let1 nil) (t nil) (nil t t) (let2 t) (nil t))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

/// Byte-compiled reads after `makunbound` signal `void-variable`, and writes,
/// reads and binds through a variable alias reach the aliased buffer-local
/// variable.
#[test]
fn oracle_bytecode_var_ops_on_void_and_aliased_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defun vco-opaque () nil)
  (defvar vco-v 1)
  (make-variable-buffer-local 'vco-v)
  (defalias 'vco-get-v (byte-compile (lambda () vco-v)))
  (defvar vco-u 1)
  (defalias 'vco-get-u (byte-compile (lambda () vco-u)))
  (defvar vco-base 1)
  (defvaralias 'vco-al 'vco-base)
  (defalias 'vco-get-al (byte-compile (lambda () vco-al)))
  (defalias 'vco-put-al (byte-compile (lambda (v) (setq vco-al v) nil)))
  (defalias 'vco-let-al
    (byte-compile (lambda (v) (let ((vco-al v)) (vco-opaque) (list vco-base vco-al)))))
  (with-temp-buffer
    (list (vco-get-v)
          (progn (setq vco-v 2) (vco-get-v))
          (progn (makunbound 'vco-u)
                 (condition-case e (vco-get-u) (void-variable (car e))))
          (progn (make-local-variable 'vco-base)
                 (vco-put-al 5)
                 (list vco-base (default-value 'vco-base) (vco-get-al)))
          (vco-let-al 6)
          (list vco-base (vco-get-al)))))
"#;
    let expect = expect_test::expect![[r#""OK (1 2 void-variable (5 1 5) (6 6) (5 5))""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}
