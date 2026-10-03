//! GNU callback immutability, GC inhibition, and test-designator parity.
//!
//! Refresh expectations only with `NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1`.
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_hash_callback_user_mutation_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // 42 cases. Equal but distinct strings ensure the comparison callback runs.
    let form = r#"
(let ((answers nil))
  (dolist (callback '(hash compare))
    (dolist (operation '(gethash puthash remhash))
      (dolist (mutation '(put-noop put-existing put-new grow rem-existing rem-missing clear))
        (let ((table nil) (armed nil) (fired nil))
          (let ((mutate
                 (lambda ()
                   (when (and armed (not fired))
                     (setq fired t)
                     (cond
                      ((eq mutation 'put-noop) (puthash "a" 1 table))
                      ((eq mutation 'put-existing) (puthash "a" 9 table))
                      ((eq mutation 'put-new) (puthash "new" 9 table))
                      ((eq mutation 'grow)
                       (dotimes (i 100) (puthash i i table)))
                      ((eq mutation 'rem-existing) (remhash "a" table))
                      ((eq mutation 'rem-missing) (remhash 'absent table))
                      ((eq mutation 'clear) (clrhash table)))))))
            (define-hash-table-test
             'gp-callback-matrix
             (lambda (a b)
               (when (eq callback 'compare) (funcall mutate))
               (equal a b))
             (lambda (_key)
               (when (eq callback 'hash) (funcall mutate))
               0))
            (setq table (make-hash-table :test 'gp-callback-matrix :size 1))
            (puthash (copy-sequence "a") 1 table)
            (setq armed t)
            (let ((result
                   (condition-case err
                       (cond
                        ((eq operation 'gethash) (gethash (copy-sequence "a") table))
                        ((eq operation 'puthash) (puthash (copy-sequence "a") 2 table))
                        ((eq operation 'remhash) (remhash (copy-sequence "a") table)))
                     (error (list (car err) (cadr err) (eq (nth 2 err) table))))))
              (setq armed nil)
              (push (list callback operation mutation result fired
                          (hash-table-count table) (gethash "a" table)) answers)))))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((hash gethash put-noop (error \"hash table test modifies table\" t) t 1 1) (hash gethash put-existing (error \"hash table test modifies table\" t) t 1 1) (hash gethash put-new (error \"hash table test modifies table\" t) t 1 1) (hash gethash grow (error \"hash table test modifies table\" t) t 1 1) (hash gethash rem-existing (error \"hash table test modifies table\" t) t 1 1) (hash gethash rem-missing (error \"hash table test modifies table\" t) t 1 1) (hash gethash clear (error \"hash table test modifies table\" t) t 1 1) (hash puthash put-noop (error \"hash table test modifies table\" t) t 1 1) (hash puthash put-existing (error \"hash table test modifies table\" t) t 1 1) (hash puthash put-new (error \"hash table test modifies table\" t) t 1 1) (hash puthash grow (error \"hash table test modifies table\" t) t 1 1) (hash puthash rem-existing (error \"hash table test modifies table\" t) t 1 1) (hash puthash rem-missing (error \"hash table test modifies table\" t) t 1 1) (hash puthash clear (error \"hash table test modifies table\" t) t 1 1) (hash remhash put-noop (error \"hash table test modifies table\" t) t 1 1) (hash remhash put-existing (error \"hash table test modifies table\" t) t 1 1) (hash remhash put-new (error \"hash table test modifies table\" t) t 1 1) (hash remhash grow (error \"hash table test modifies table\" t) t 1 1) (hash remhash rem-existing (error \"hash table test modifies table\" t) t 1 1) (hash remhash rem-missing (error \"hash table test modifies table\" t) t 1 1) (hash remhash clear (error \"hash table test modifies table\" t) t 1 1) (compare gethash put-noop (error \"hash table test modifies table\" t) t 1 1) (compare gethash put-existing (error \"hash table test modifies table\" t) t 1 1) (compare gethash put-new (error \"hash table test modifies table\" t) t 1 1) (compare gethash grow (error \"hash table test modifies table\" t) t 1 1) (compare gethash rem-existing (error \"hash table test modifies table\" t) t 1 1) (compare gethash rem-missing (error \"hash table test modifies table\" t) t 1 1) (compare gethash clear (error \"hash table test modifies table\" t) t 1 1) (compare puthash put-noop (error \"hash table test modifies table\" t) t 1 1) (compare puthash put-existing (error \"hash table test modifies table\" t) t 1 1) (compare puthash put-new (error \"hash table test modifies table\" t) t 1 1) (compare puthash grow (error \"hash table test modifies table\" t) t 1 1) (compare puthash rem-existing (error \"hash table test modifies table\" t) t 1 1) (compare puthash rem-missing (error \"hash table test modifies table\" t) t 1 1) (compare puthash clear (error \"hash table test modifies table\" t) t 1 1) (compare remhash put-noop (error \"hash table test modifies table\" t) t 1 1) (compare remhash put-existing (error \"hash table test modifies table\" t) t 1 1) (compare remhash put-new (error \"hash table test modifies table\" t) t 1 1) (compare remhash grow (error \"hash table test modifies table\" t) t 1 1) (compare remhash rem-existing (error \"hash table test modifies table\" t) t 1 1) (compare remhash rem-missing (error \"hash table test modifies table\" t) t 1 1) (compare remhash clear (error \"hash table test modifies table\" t) t 1 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_hash_callback_empty_clear_and_missing_remove_are_rejected() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (dolist (mutation '(clear rem-missing))
    (let ((table nil) (armed nil))
      (define-hash-table-test
       'gp-empty-callback (lambda (a b) (equal a b))
       (lambda (_key)
         (when armed
           (if (eq mutation 'clear) (clrhash table) (remhash 'missing table)))
         0))
      (setq table (make-hash-table :test 'gp-empty-callback) armed t)
      (let ((result (condition-case err (gethash 'missing table)
                      (error (list (car err) (cadr err) (eq (nth 2 err) table))))))
        (setq armed nil)
        (push (list mutation result (hash-table-count table)
                    (puthash 'after 'mutable table)) answers))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((clear (error \"hash table test modifies table\" t) 0 mutable) (rem-missing (error \"hash table test modifies table\" t) 0 mutable))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_hash_callback_nested_reads_other_writes_and_caught_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (dolist (callback '(hash compare))
    (let ((table nil) (armed nil) (fired nil) (observed nil)
          (other (make-hash-table)))
      (let ((action
             (lambda ()
               (when (and armed (not fired))
                 (setq fired t)
                 (puthash 'outside 'allowed other)
                 (setq observed
                       (list (gethash (copy-sequence "a") table)
                             (hash-table-count table)
                             (let ((before gcs-done))
                               (list (null (garbage-collect)) (= before gcs-done)))
                             (condition-case err (clrhash table)
                               (error (list (car err) (cadr err)
                                            (eq (nth 2 err) table))))))))))
        (define-hash-table-test
         'gp-nested-callback
         (lambda (a b)
           (when (eq callback 'compare) (funcall action))
           (equal a b))
         (lambda (_key)
           (when (eq callback 'hash) (funcall action))
           0))
        (setq table (make-hash-table :test 'gp-nested-callback))
        (puthash (copy-sequence "a") 1 table)
        (setq armed t)
        (let ((result (gethash (copy-sequence "a") table)))
          (setq armed nil)
          (push (list callback result observed (gethash 'outside other)
                      (puthash "a" 2 table) (gethash "a" table)) answers)))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((hash 1 (1 1 (t t) (error \"hash table test modifies table\" t)) allowed 2 2) (compare 1 (1 1 (t t) (error \"hash table test modifies table\" t)) allowed 2 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_hash_callback_signal_and_throw_restore_mutability_and_gc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (dolist (callback '(hash compare))
    (dolist (exit '(signal throw))
      (let ((table nil) (armed nil) (gc-state nil))
        (let ((action
               (lambda ()
                 (when armed
                   (let ((before gcs-done))
                     (setq gc-state (list (null (garbage-collect)) (= before gcs-done))))
                   (if (eq exit 'signal)
                       (error "gp callback failed")
                     (throw 'gp-callback-exit 'thrown))))))
          (define-hash-table-test
           'gp-unwind-callback
           (lambda (a b)
             (when (eq callback 'compare) (funcall action))
             (equal a b))
           (lambda (_key)
             (when (eq callback 'hash) (funcall action))
             0))
          (setq table (make-hash-table :test 'gp-unwind-callback))
          (puthash (copy-sequence "a") 1 table)
          (setq armed t)
          (let ((result
                 (condition-case err
                     (catch 'gp-callback-exit (gethash (copy-sequence "a") table))
                   (error (list (car err) (cadr err))))))
            (setq armed nil)
            (let ((write (puthash "a" 2 table)) (before gcs-done))
              (garbage-collect)
              (push (list callback exit result gc-state write
                          (gethash "a" table) (> gcs-done before)) answers)))))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((hash signal (error \"gp callback failed\") (t t) 2 2 t) (hash throw thrown (t t) 2 2 t) (compare signal (error \"gp callback failed\") (t t) 2 2 t) (compare throw thrown (t t) 2 2 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_hash_callback_interpreted_gc_and_gc_maybe_are_inhibited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (dolist (callback '(hash compare))
    (dolist (threshold-mode '(ordinary huge))
      (let ((table nil) (armed nil) (gc-state nil)
            (gc-cons-threshold (if (eq threshold-mode 'huge)
                                   (ash 1 62) gc-cons-threshold))
            (gc-cons-percentage 0.0))
      (let ((action
             (lambda ()
               (when armed
                 (let ((before gcs-done))
                   (setq gc-state
                         (list (null (garbage-collect))
                               (garbage-collect-maybe 0)
                               (progn (make-list 32 nil)
                                      (garbage-collect-maybe most-positive-fixnum))
                               (= before gcs-done))))))))
        (define-hash-table-test
         'gp-gc-callback
         (lambda (a b)
           (when (eq callback 'compare) (funcall action))
           (equal a b))
         (lambda (_key)
           (when (eq callback 'hash) (funcall action))
           0))
        (setq table (make-hash-table :test 'gp-gc-callback :weakness 'key))
        (puthash (copy-sequence "a") 'payload table)
        (setq armed t)
        (let* ((before gcs-done) (value (gethash (copy-sequence "a") table)))
          (setq armed nil)
          (push (list callback threshold-mode value gc-state (= before gcs-done)
                      (hash-table-count table)) answers))))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((hash ordinary payload (t nil nil t) t 1) (hash huge payload (t nil t t) t 1) (compare ordinary payload (t nil nil t) t 1) (compare huge payload (t nil t t) t 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_hash_callback_bytecode_hash_and_compare_suppress_explicit_gc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // This call-zero opcode reaches Vm::builtin_garbage_collect_shared.
    let form = r#"
(progn
  (defvar gp-gc-result nil)
  (let (answers)
    (dolist (callback '(hash compare))
      (let* ((gp-gc-result 'not-called)
             (function
              (make-byte-code
               (if (eq callback 'hash) 257 514)
               "\300\040\022\301\207"
               (vector 'garbage-collect (if (eq callback 'hash) 0 t) 'gp-gc-result)
               4))
             (name (make-symbol "gp-bytecode-gc"))
             (table nil))
        (define-hash-table-test
         name
         (if (eq callback 'compare) function (lambda (a b) (equal a b)))
         (if (eq callback 'hash) function (lambda (_key) 0)))
        (setq table (make-hash-table :test name))
        (puthash "key" 'payload table)
        (setq gp-gc-result 'not-called)
        (let* ((before gcs-done) (value (gethash (copy-sequence "key") table)))
          (push (list callback (byte-code-function-p function) value
                      (null gp-gc-result) (= before gcs-done)
                      (hash-table-count table)) answers))))
    (nreverse answers)))
"#;
    let expect =
        expect_test::expect![[r#""OK ((hash t payload t t 1) (compare t payload t t 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_hash_callback_bytecompiled_gc_maybe_is_inhibited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (require 'bytecomp)
  (defvar gp-bytecompiled-gc-state nil)
  (defvar gp-bytecompiled-armed nil)
  (let (answers)
    (dolist (callback '(hash compare))
      (dolist (threshold-mode '(ordinary huge))
        (let* ((gp-bytecompiled-gc-state nil) (gp-bytecompiled-armed nil)
             (gc-cons-threshold (if (eq threshold-mode 'huge)
                                    (ash 1 62) gc-cons-threshold))
             (gc-cons-percentage 0.0)
             (action
              (byte-compile
               (lambda ()
                 (when gp-bytecompiled-armed
                   (let ((before gcs-done))
                     (setq gp-bytecompiled-gc-state
                           (list (null (garbage-collect))
                                 (garbage-collect-maybe 0)
                                 (progn (make-list 32 nil)
                                        (garbage-collect-maybe most-positive-fixnum))
                                 (= before gcs-done))))))))
             (table nil))
        (define-hash-table-test
         'gp-bytecompiled-gc
         (lambda (a b)
           (when (eq callback 'compare) (funcall action))
           (equal a b))
         (lambda (_key)
           (when (eq callback 'hash) (funcall action))
           0))
        (setq table (make-hash-table :test 'gp-bytecompiled-gc))
        (puthash (copy-sequence "a") 'payload table)
        (setq gp-bytecompiled-armed t)
        (let* ((before gcs-done) (value (gethash (copy-sequence "a") table)))
          (setq gp-bytecompiled-armed nil)
          (push (list callback threshold-mode (byte-code-function-p action) value
                      gp-bytecompiled-gc-state (= before gcs-done)) answers)))))
    (nreverse answers)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((hash ordinary t payload (t nil nil t) t) (hash huge t payload (t nil t t) t) (compare ordinary t payload (t nil nil t) t) (compare huge t payload (t nil t t) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_hash_callback_copy_is_mutable_and_original_stays_guarded() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (dolist (callback '(hash compare))
    (let ((table nil) (armed nil) (fired nil) (copy nil) (observed nil))
      (let ((action
             (lambda ()
               (when (and armed (not fired))
                 (setq fired t copy (copy-hash-table table))
                 (setq observed
                       (list (eq (hash-table-test copy) (hash-table-test table))
                             (puthash "a" 2 copy)
                             (puthash "b" 3 copy)
                             (remhash "missing" copy)
                             (hash-table-count copy)
                             (progn (clrhash copy) (hash-table-count copy))
                             (condition-case err (puthash "a" 4 table)
                               (error (list (car err) (cadr err)
                                            (eq (nth 2 err) table))))))))))
        (define-hash-table-test
         'gp-copy-callback
         (lambda (a b)
           (when (eq callback 'compare) (funcall action))
           (equal a b))
         (lambda (_key)
           (when (eq callback 'hash) (funcall action))
           0))
        (setq table (make-hash-table :test 'gp-copy-callback))
        (puthash (copy-sequence "a") 1 table)
        (setq armed t)
        (let ((result (gethash (copy-sequence "a") table)))
          (setq armed nil)
          (push (list callback result observed (hash-table-count table)
                      (gethash "a" table) (puthash "after" 5 copy)) answers)))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((hash 1 (t 2 3 nil 2 0 (error \"hash table test modifies table\" t)) 1 1 5) (compare 1 (t 2 3 nil 2 0 (error \"hash table test modifies table\" t)) 1 1 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_hash_callback_canonical_names_and_comparison_designators() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (dolist (canonical '(eq eql equal))
    (let ((hits 0))
      (define-hash-table-test canonical (lambda (_a _b) nil)
        (lambda (_key) (setq hits (1+ hits)) 0))
      (let ((table (make-hash-table :test canonical)))
        (puthash 7 'payload table)
        (push (list 'canonical-name canonical (gethash 7 table)
                    hits (hash-table-test table)) answers)))
    (let* ((hits 0) (name (make-symbol (symbol-name canonical))))
      (define-hash-table-test name canonical
        (lambda (_key) (setq hits (1+ hits)) 0))
      (let ((table (make-hash-table :test name)))
        (puthash 7 'payload table)
        (push (list 'uninterned-name canonical (gethash 7 table) hits
                    (eq (hash-table-test table) name)) answers)))
    (dolist (designator '(symbol subr alias wrapper))
      (let* ((hits 0) (name (make-symbol "gp-user-designator"))
             (alias (make-symbol "gp-comparison-alias"))
             (comparison
              (cond ((eq designator 'symbol) canonical)
                    ((eq designator 'subr) (symbol-function canonical))
                    ((eq designator 'alias) (fset alias canonical) alias)
                    (t (lambda (a b) (funcall canonical a b))))))
        (define-hash-table-test name comparison
          (lambda (_key) (setq hits (1+ hits)) 0))
        (let ((table (make-hash-table :test name)))
          (puthash 7 'payload table)
          (push (list canonical designator (gethash 7 table) hits
                      (eq (hash-table-test table) name)
                      (hash-table-count table)) answers)))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((canonical-name eq payload 0 eq) (uninterned-name eq payload 2 t) (eq symbol payload 2 t 1) (eq subr payload 2 t 1) (eq alias payload 2 t 1) (eq wrapper payload 2 t 1) (canonical-name eql payload 0 eql) (uninterned-name eql payload 2 t) (eql symbol payload 2 t 1) (eql subr payload 2 t 1) (eql alias payload 2 t 1) (eql wrapper payload 2 t 1) (canonical-name equal payload 0 equal) (uninterned-name equal payload 2 t) (equal symbol payload 2 t 1) (equal subr payload 2 t 1) (equal alias payload 2 t 1) (equal wrapper payload 2 t 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
