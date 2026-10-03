//! GNU callback immutability, GC inhibition, and test-designator parity.
//!
//! Refresh expectations only with `NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1`.
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_hash_callback_gc_settings_localize_during_active_guard() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (dolist (callback '(hash compare))
    (dolist (exit '(normal signal throw))
      (let ((gc-cons-threshold (ash 1 62)) (gc-cons-percentage 0.0)
            (outer nil) (inner nil) (phase nil) (inner-armed nil)
            (before-state nil) (after-state nil) (inner-state nil)
            (inner-result nil) (mutation nil))
        (with-temp-buffer
          (let* ((probe
                  (lambda ()
                    (let ((before gcs-done))
                      (make-list 32 nil)
                      (list (garbage-collect-maybe most-positive-fixnum)
                            (null (garbage-collect)) (= before gcs-done)))))
                 (inner-action
                  (lambda ()
                    (when inner-armed
                      (setq inner-state (funcall probe))
                      (cond ((eq exit 'signal) (error "gp local callback failed"))
                            ((eq exit 'throw) (throw 'gp-local-callback-exit 'thrown))))))
                 (outer-action
                  (lambda ()
                    (when (eq phase 'outer)
                      (setq phase 'busy)
                      (make-local-variable 'gc-cons-threshold)
                      (setq gc-cons-threshold (ash 1 62))
                      (make-variable-buffer-local 'gc-cons-percentage)
                      (setq gc-cons-percentage 0.0)
                      (make-local-variable 'memory-full)
                      (setq memory-full nil)
                      (setq before-state (funcall probe) inner-armed t)
                      (setq inner-result
                            (condition-case err
                                (catch 'gp-local-callback-exit
                                  (gethash (copy-sequence "inner") inner))
                              (error (list (car err) (cadr err)))))
                      (setq inner-armed nil)
                      (setq after-state (funcall probe))
                      (setq mutation
                            (condition-case err (clrhash outer)
                              (error (list (car err) (cadr err)
                                           (eq (nth 2 err) outer)))))))))
            (define-hash-table-test
             'gp-local-outer-settings
             (lambda (a b) (when (eq callback 'compare) (funcall outer-action)) (equal a b))
             (lambda (_key) (when (eq callback 'hash) (funcall outer-action)) 0))
            (define-hash-table-test
             'gp-local-inner-settings
             (lambda (a b) (when (eq callback 'compare) (funcall inner-action)) (equal a b))
             (lambda (_key) (when (eq callback 'hash) (funcall inner-action)) 0))
            (setq outer (make-hash-table :test 'gp-local-outer-settings)
                  inner (make-hash-table :test 'gp-local-inner-settings))
            (puthash (copy-sequence "outer") 7 outer)
            (puthash (copy-sequence "inner") 22 inner)
            (setq phase 'outer)
            (let ((value (gethash (copy-sequence "outer") outer)))
              (setq phase nil)
              (let ((outer-after (puthash "outer" 9 outer))
                    (inner-after (puthash "inner" 44 inner))
                    (before gcs-done))
                (garbage-collect)
                (push (list callback exit value before-state inner-result inner-state
                            after-state mutation outer-after inner-after
                            (> gcs-done before)) answers))))))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((hash normal 7 (t t t) 22 (t t t) (t t t) (error \"hash table test modifies table\" t) 9 44 t) (hash signal 7 (t t t) (error \"gp local callback failed\") (t t t) (t t t) (error \"hash table test modifies table\" t) 9 44 t) (hash throw 7 (t t t) thrown (t t t) (t t t) (error \"hash table test modifies table\" t) 9 44 t) (compare normal 7 (t t t) 22 (t t t) (t t t) (error \"hash table test modifies table\" t) 9 44 t) (compare signal 7 (t t t) (error \"gp local callback failed\") (t t t) (t t t) (error \"hash table test modifies table\" t) 9 44 t) (compare throw 7 (t t t) thrown (t t t) (t t t) (error \"hash table test modifies table\" t) 9 44 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_hash_callback_gc_settings_aliases_and_assignment_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (defvaralias 'gp-alias-threshold 'gc-cons-threshold)
  (defvaralias 'gp-alias-percentage 'gc-cons-percentage)
  (dolist (callback '(hash compare))
    (dolist (setter '(setq set set-default let let*))
      (dolist (setting '(threshold percentage))
        (dolist (alias '(nil t))
          (let* ((gc-cons-threshold 800000) (gc-cons-percentage 0.0)
                 (sym (if (eq setting 'threshold)
                          (if alias 'gp-alias-threshold 'gc-cons-threshold)
                        (if alias 'gp-alias-percentage 'gc-cons-percentage)))
                 (high (if (eq setting 'threshold) (ash 1 62) 1e100))
                 (table nil) (armed nil) (observed nil)
                 (lookup (lambda ()
                           (setq observed nil)
                           (let ((value (gethash (copy-sequence "key") table)))
                             (list value observed))))
                 (probe (lambda ()
                          (when armed
                            (let ((gc-cons-threshold 800000)
                                  (gc-cons-percentage 0.0)
                                  (before gcs-done))
                              (setq observed
                                    (list (garbage-collect-maybe most-positive-fixnum)
                                          (null (garbage-collect)) (= before gcs-done))))))))
            (define-hash-table-test
             'gp-alias-settings
             (lambda (a b) (when (eq callback 'compare) (funcall probe)) (equal a b))
             (lambda (_key) (when (eq callback 'hash) (funcall probe)) 0))
            (setq table (make-hash-table :test 'gp-alias-settings))
            (puthash (copy-sequence "key") 7 table)
            (setq armed t)
            (let ((value
                   (cond
                    ((memq setter '(let let*))
                     (eval (list setter (list (list sym (list 'quote high)))
                                 (list 'funcall (list 'quote lookup)))))
                    ((eq setter 'setq)
                     (eval (list 'setq sym (list 'quote high))) (funcall lookup))
                    ((eq setter 'set)
                     (set sym high) (funcall lookup))
                    (t (set-default sym high) (funcall lookup)))))
              (setq armed nil)
              (push (list callback setter setting alias value
                          (puthash "after" 'mutable table)) answers)))))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((hash setq threshold nil (7 (t t t)) mutable) (hash setq threshold t (7 (t t t)) mutable) (hash setq percentage nil (7 (t t t)) mutable) (hash setq percentage t (7 (t t t)) mutable) (hash set threshold nil (7 (t t t)) mutable) (hash set threshold t (7 (t t t)) mutable) (hash set percentage nil (7 (t t t)) mutable) (hash set percentage t (7 (t t t)) mutable) (hash set-default threshold nil (7 (t t t)) mutable) (hash set-default threshold t (7 (t t t)) mutable) (hash set-default percentage nil (7 (t t t)) mutable) (hash set-default percentage t (7 (t t t)) mutable) (hash let threshold nil (7 (t t t)) mutable) (hash let threshold t (7 (t t t)) mutable) (hash let percentage nil (7 (t t t)) mutable) (hash let percentage t (7 (t t t)) mutable) (hash let* threshold nil (7 (t t t)) mutable) (hash let* threshold t (7 (t t t)) mutable) (hash let* percentage nil (7 (t t t)) mutable) (hash let* percentage t (7 (t t t)) mutable) (compare setq threshold nil (7 (t t t)) mutable) (compare setq threshold t (7 (t t t)) mutable) (compare setq percentage nil (7 (t t t)) mutable) (compare setq percentage t (7 (t t t)) mutable) (compare set threshold nil (7 (t t t)) mutable) (compare set threshold t (7 (t t t)) mutable) (compare set percentage nil (7 (t t t)) mutable) (compare set percentage t (7 (t t t)) mutable) (compare set-default threshold nil (7 (t t t)) mutable) (compare set-default threshold t (7 (t t t)) mutable) (compare set-default percentage nil (7 (t t t)) mutable) (compare set-default percentage t (7 (t t t)) mutable) (compare let threshold nil (7 (t t t)) mutable) (compare let threshold t (7 (t t t)) mutable) (compare let percentage nil (7 (t t t)) mutable) (compare let percentage t (7 (t t t)) mutable) (compare let* threshold nil (7 (t t t)) mutable) (compare let* threshold t (7 (t t t)) mutable) (compare let* percentage nil (7 (t t t)) mutable) (compare let* percentage t (7 (t t t)) mutable))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_hash_callback_gc_settings_variable_watcher_nested_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Watchers reenter a table before variable publication. The zero-percent
    // GC query exercises accounting without depending on allocation volume.
    let form = r#"
(let (answers)
  (dolist (callback '(hash compare))
    (dolist (setter '(set set-default let let*))
      (let ((gc-cons-threshold (ash 1 62)) (gc-cons-percentage 0.0)
            (table nil) (armed nil) (observed nil) (events nil) (watcher nil))
        (let* ((probe
                (lambda ()
                  (when armed
                    (let ((before gcs-done))
                      (setq observed
                            (list (garbage-collect-maybe 0)
                                  (null (garbage-collect)) (= before gcs-done)))))))
               (lookup
                (lambda ()
                  (setq observed nil)
                  (let ((value (gethash (copy-sequence "key") table)))
                    (list value observed)))))
          (define-hash-table-test
           'gp-watcher-settings
           (lambda (a b) (when (eq callback 'compare) (funcall probe)) (equal a b))
           (lambda (_key) (when (eq callback 'hash) (funcall probe)) 0))
          (setq table (make-hash-table :test 'gp-watcher-settings))
          (puthash (copy-sequence "key") 7 table)
          (setq watcher
                (lambda (_symbol newvalue operation _where)
                  (push (list operation (= newvalue 800000)
                              (= gc-cons-threshold 800000) (funcall lookup)) events)))
          (add-variable-watcher 'gc-cons-threshold watcher)
          (setq armed t)
          (unwind-protect
              (let ((value
                     (cond
                      ((eq setter 'set) (set 'gc-cons-threshold 800000) (funcall lookup))
                      ((eq setter 'set-default) (set-default 'gc-cons-threshold 800000) (funcall lookup))
                      ((eq setter 'let) (let ((gc-cons-threshold 800000)) (funcall lookup)))
                      (t (let* ((gc-cons-threshold 800000)) (funcall lookup))))))
                (setq armed nil)
                (push (list callback setter value (nreverse events)
                            (puthash "after" 'mutable table)) answers))
            (setq armed nil)
            (remove-variable-watcher 'gc-cons-threshold watcher))))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((hash set (7 (nil t t)) ((set t nil (7 (nil t t)))) mutable) (hash set-default (7 (nil t t)) ((set t nil (7 (nil t t))) (set t nil (7 (nil t t)))) mutable) (hash let (7 (nil t t)) ((let t nil (7 (nil t t))) (set nil t (7 (nil t t))) (unlet nil t (7 (nil t t)))) mutable) (hash let* (7 (nil t t)) ((let t nil (7 (nil t t))) (set nil t (7 (nil t t))) (unlet nil t (7 (nil t t)))) mutable) (compare set (7 (nil t t)) ((set t nil (7 (nil t t)))) mutable) (compare set-default (7 (nil t t)) ((set t nil (7 (nil t t))) (set t nil (7 (nil t t)))) mutable) (compare let (7 (nil t t)) ((let t nil (7 (nil t t))) (set nil t (7 (nil t t))) (unlet nil t (7 (nil t t)))) mutable) (compare let* (7 (nil t t)) ((let t nil (7 (nil t t))) (set nil t (7 (nil t t))) (unlet nil t (7 (nil t t)))) mutable))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_hash_callback_gc_maybe_preserves_entry_accounting_after_live_settings_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (dolist (callback '(hash compare))
    (dolist (binding '(setq let))
      (dolist (change '(raise-threshold lower-threshold lower-percentage))
        (let* ((start-threshold (if (eq change 'lower-threshold) (ash 1 62) 800000))
               (start-percentage (if (eq change 'lower-percentage) 1e100 0.0))
               (next-threshold (if (eq change 'raise-threshold) (ash 1 62) 800000))
               (gc-cons-threshold start-threshold)
               (gc-cons-percentage start-percentage)
               (table nil) (armed nil) (observed nil) (after nil))
          (let* ((probe
                  (lambda (before)
                    (list (garbage-collect-maybe most-positive-fixnum)
                          (garbage-collect-maybe 1)
                          (garbage-collect-maybe 0)
                          (null (garbage-collect))
                          (= before gcs-done))))
                 (action
                  (lambda ()
                    (when armed
                      (let ((before gcs-done))
                        (make-list 32 nil)
                        (if (eq binding 'let)
                            (let ((gc-cons-threshold next-threshold)
                                  (gc-cons-percentage 0.0))
                              (setq observed (funcall probe before)))
                          (setq gc-cons-threshold next-threshold
                                gc-cons-percentage 0.0)
                          (setq observed (funcall probe before))
                          (setq gc-cons-threshold start-threshold
                                gc-cons-percentage start-percentage))
                        (setq after
                              (list (garbage-collect-maybe most-positive-fixnum)
                                    (= before gcs-done))))))))
            (define-hash-table-test
             'gp-live-settings-callback
             (lambda (a b)
               (when (eq callback 'compare) (funcall action))
               (equal a b))
             (lambda (_key)
               (when (eq callback 'hash) (funcall action))
               0))
            (setq table (make-hash-table :test 'gp-live-settings-callback))
            (puthash (copy-sequence "key") 'payload table)
            (setq armed t)
            (let ((value (gethash (copy-sequence "key") table)))
              (setq armed nil)
              (push (list callback binding change value observed after
                          (puthash "after" 'mutable table)) answers)))))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((hash setq raise-threshold payload (nil nil nil t t) (nil t) mutable) (hash setq lower-threshold payload (t nil nil t t) (t t) mutable) (hash setq lower-percentage payload (t nil nil t t) (t t) mutable) (hash let raise-threshold payload (nil nil nil t t) (nil t) mutable) (hash let lower-threshold payload (t nil nil t t) (t t) mutable) (hash let lower-percentage payload (t nil nil t t) (t t) mutable) (compare setq raise-threshold payload (nil nil nil t t) (nil t) mutable) (compare setq lower-threshold payload (t nil nil t t) (t t) mutable) (compare setq lower-percentage payload (t nil nil t t) (t t) mutable) (compare let raise-threshold payload (nil nil nil t t) (nil t) mutable) (compare let lower-threshold payload (t nil nil t t) (t t) mutable) (compare let lower-percentage payload (t nil nil t t) (t t) mutable))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_hash_callback_gc_maybe_nested_other_table_restores_outer_accounting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (dolist (callback '(hash compare))
    (dolist (exit '(normal signal throw))
      (let ((gc-cons-threshold 800000) (gc-cons-percentage 0.0)
            (outer nil) (inner nil) (phase nil) (inner-armed nil)
            (inner-result nil) (inner-state nil) (same-result nil)
            (same-state nil) (outer-state nil) (mutation nil) (other-write nil))
        (let* ((inner-action
                (lambda ()
                  (when inner-armed
                    (let ((before gcs-done))
                      (make-list 32 nil)
                      (setq inner-state
                            (list (garbage-collect-maybe most-positive-fixnum)
                                  (null (garbage-collect)) (= before gcs-done))))
                    (cond ((eq exit 'signal) (error "gp nested callback failed"))
                          ((eq exit 'throw) (throw 'gp-nested-callback-exit 'thrown))))))
               (outer-action
                (lambda ()
                  (cond
                   ((eq phase 'outer)
                    (setq phase 'busy)
                    (let ((gc-cons-threshold (ash 1 62)))
                      (setq inner-armed t)
                      (setq inner-result
                            (condition-case err
                                (catch 'gp-nested-callback-exit
                                  (gethash (copy-sequence "inner") inner))
                              (error (list (car err) (cadr err)))))
                      (setq inner-armed nil)
                      (setq other-write (puthash "after" 'allowed inner))
                      (setq phase 'same)
                      (setq same-result (gethash (copy-sequence "outer") outer))
                      (let ((before gcs-done))
                        (setq outer-state
                              (list (garbage-collect-maybe most-positive-fixnum)
                                    (null (garbage-collect)) (= before gcs-done))))
                      (setq mutation
                            (condition-case err (clrhash outer)
                              (error (list (car err) (cadr err)
                                           (eq (nth 2 err) outer)))))))
                   ((eq phase 'same)
                    (setq phase 'done)
                    (let ((before gcs-done))
                      (make-list 32 nil)
                      (setq same-state
                            (list (garbage-collect-maybe most-positive-fixnum)
                                  (null (garbage-collect)) (= before gcs-done)))))))))
          (define-hash-table-test
           'gp-nested-outer-accounting
           (lambda (a b)
             (when (eq callback 'compare) (funcall outer-action))
             (equal a b))
           (lambda (_key)
             (when (eq callback 'hash) (funcall outer-action))
             0))
          (define-hash-table-test
           'gp-nested-inner-accounting
           (lambda (a b)
             (when (eq callback 'compare) (funcall inner-action))
             (equal a b))
           (lambda (_key)
             (when (eq callback 'hash) (funcall inner-action))
             0))
          (setq outer (make-hash-table :test 'gp-nested-outer-accounting)
                inner (make-hash-table :test 'gp-nested-inner-accounting))
          (puthash (copy-sequence "outer") 7 outer)
          (puthash (copy-sequence "inner") 22 inner)
          (setq phase 'outer)
          (let ((value (gethash (copy-sequence "outer") outer)))
            (setq phase nil)
            (let ((outer-after (puthash "outer" 9 outer))
                  (inner-after (puthash "inner" 44 inner))
                  (before gcs-done))
              (garbage-collect)
              (push (list callback exit value inner-result inner-state
                          same-result same-state outer-state mutation other-write
                          outer-after inner-after (> gcs-done before)) answers)))))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((hash normal 7 22 (t t t) 7 (nil t t) (nil t t) (error \"hash table test modifies table\" t) allowed 9 44 t) (hash signal 7 (error \"gp nested callback failed\") (t t t) 7 (nil t t) (nil t t) (error \"hash table test modifies table\" t) allowed 9 44 t) (hash throw 7 thrown (t t t) 7 (nil t t) (nil t t) (error \"hash table test modifies table\" t) allowed 9 44 t) (compare normal 7 22 (t t t) 7 (nil t t) (nil t t) (error \"hash table test modifies table\" t) allowed 9 44 t) (compare signal 7 (error \"gp nested callback failed\") (t t t) 7 (nil t t) (nil t t) (error \"hash table test modifies table\" t) allowed 9 44 t) (compare throw 7 thrown (t t t) 7 (nil t t) (nil t t) (error \"hash table test modifies table\" t) allowed 9 44 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

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
