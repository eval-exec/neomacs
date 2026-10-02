//! GNU `Fmaphash` uses `DOHASH_SAFE`: visit live occupied slots and read the
//! value immediately before each callback. Refresh these expectations from
//! GNU only with `NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1`.
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

const KNOB_MATRIX: &[&[(&str, &str)]] = &[
    &[("NEOVM_MAPHASH_BYTECODE", "off")],
    &[("NEOVM_MAPHASH_BYTECODE", "on")],
    &[
        ("NEOVM_MAPHASH_BYTECODE", "on"),
        ("NEOVM_JIT_THRESHOLD", "1"),
    ],
    &[
        ("NEOVM_MAPHASH_BYTECODE", "on"),
        ("NEOVM_JIT_INLINE", "off"),
    ],
];

#[test]
fn oracle_maphash_bytecode_order_current_removal_and_value_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (require 'bytecomp)
  (defvar mh-bc-table nil)
  (defvar mh-bc-log nil)
  (let ((mh-bc-table (make-hash-table :test 'eq))
        (mh-bc-log nil)
        (callback
         (byte-compile
          (lambda (key value)
            (push (cons key value) mh-bc-log)
            (if (eq key 'b)
                (remhash key mh-bc-table)
              (puthash key (+ value 100) mh-bc-table))))))
    (mapc (lambda (pair) (puthash (car pair) (cdr pair) mh-bc-table))
          '((a . 1) (b . 2) (c . 3) (d . 4)))
    (remhash 'c mh-bc-table)
    (puthash 'late 9 mh-bc-table)
    (list (byte-code-function-p callback)
          (maphash callback mh-bc-table)
          (nreverse mh-bc-log)
          (mapcar (lambda (key) (gethash key mh-bc-table 'gone))
                  '(a b c late d))
          (hash-table-count mh-bc-table))))
"#;
    let expect = expect_test::expect![[
        r#""OK (t nil ((a . 1) (b . 2) (late . 9) (d . 4)) (101 gone gone 109 104) 3)""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

#[test]
fn oracle_maphash_bytecode_reads_live_future_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (require 'bytecomp)
  (defvar mh-bc-table nil)
  (defvar mh-bc-log nil)
  (let ((mh-bc-table (make-hash-table :test 'eq)) (mh-bc-log nil))
    (dotimes (i 5) (puthash i (+ i 10) mh-bc-table))
    (let ((callback
           (byte-compile
            (lambda (key value)
              (push (cons key value) mh-bc-log)
              (when (= key 0)
                (puthash 1 101 mh-bc-table)
                (remhash 2 mh-bc-table))
              (when (= key 1)
                (remhash 3 mh-bc-table)
                (puthash 8 108 mh-bc-table))))))
      (maphash callback mh-bc-table)
      (list (nreverse mh-bc-log) (hash-table-count mh-bc-table)
            (let (entries)
              (maphash (lambda (key value) (push (cons key value) entries)) mh-bc-table)
              (nreverse entries))))))
"#;
    let expect = expect_test::expect![[
        r#""OK (((0 . 10) (1 . 101) (8 . 108) (4 . 14)) 4 ((0 . 10) (1 . 101) (8 . 108) (4 . 14)))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

#[test]
fn oracle_maphash_bytecode_growth_and_clear_are_safe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (require 'bytecomp)
  (defvar mh-bc-table nil)
  (defvar mh-bc-log nil)
  (let ((mh-bc-table (make-hash-table :test 'eq :size 1)) (mh-bc-log nil))
    (puthash 0 10 mh-bc-table)
    (maphash
     (byte-compile
      (lambda (key value)
        (push (cons key value) mh-bc-log)
        (when (= key 0)
          (dotimes (i 30) (puthash (1+ i) (+ i 11) mh-bc-table)))))
     mh-bc-table)
    (let ((grown (nreverse mh-bc-log)))
      (setq mh-bc-log nil)
      (maphash
       (byte-compile
        (lambda (key value)
          (push (cons key value) mh-bc-log)
          (when (= key 0)
            (clrhash mh-bc-table)
            (puthash 91 191 mh-bc-table)
            (puthash 92 192 mh-bc-table))))
       mh-bc-table)
      (list grown (nreverse mh-bc-log) (hash-table-count mh-bc-table)))))
"#;
    let expect = expect_test::expect![[
        r#""OK (((0 . 10) (1 . 11) (2 . 12) (3 . 13) (4 . 14) (5 . 15) (6 . 16) (7 . 17) (8 . 18) (9 . 19) (10 . 20) (11 . 21) (12 . 22) (13 . 23) (14 . 24) (15 . 25) (16 . 26) (17 . 27) (18 . 28) (19 . 29) (20 . 30) (21 . 31) (22 . 32) (23 . 33) (24 . 34) (25 . 35) (26 . 36) (27 . 37) (28 . 38) (29 . 39) (30 . 40)) ((0 . 10) (92 . 192)) 2)""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

#[test]
fn oracle_maphash_bytecode_gc_error_throw_and_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (require 'bytecomp)
  (defvar mh-bc-table nil)
  (defvar mh-bc-log nil)
  (let ((mh-bc-table (make-hash-table :test 'eq)) (mh-bc-log nil))
    (dotimes (i 4) (puthash i (list i (+ i 10)) mh-bc-table))
    (let ((failure
           (condition-case err
               (maphash
                (byte-compile
                 (lambda (key value)
                   (remhash key mh-bc-table)
                   (garbage-collect)
                   (push (cons key value) mh-bc-log)
                   (when (= key 2) (error "mh-bc-error"))))
                mh-bc-table)
             (error err))))
      (list failure (nreverse mh-bc-log) (hash-table-count mh-bc-table)
            (catch 'mh-bc-throw
              (maphash
               (byte-compile (lambda (key value)
                               (garbage-collect)
                               (throw 'mh-bc-throw (cons key value))))
               mh-bc-table))
            (maphash (byte-compile (lambda (_key _value) nil)) mh-bc-table)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((error \"mh-bc-error\") ((0 0 10) (1 1 11) (2 2 12)) 1 (3 3 13) nil)""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

#[test]
fn oracle_maphash_bytecode_optional_rest_and_wrong_arity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (require 'bytecomp)
  (defvar mh-bc-log nil)
  (let ((table (make-hash-table :test 'eq)) (mh-bc-log nil))
    (puthash 'a 1 table)
    (puthash 'b 2 table)
    (list
     (progn
       (maphash (byte-compile (lambda (key &optional value unused &rest rest)
                               (push (list key value unused rest) mh-bc-log))) table)
       (nreverse mh-bc-log))
     (progn
       (setq mh-bc-log nil)
       (maphash (byte-compile (lambda (&rest args) (push args mh-bc-log))) table)
       (nreverse mh-bc-log))
     (condition-case err
         (maphash (byte-compile (lambda (_key) nil)) table)
       (error (list (car err) (nth 2 err))))
     (condition-case err
         (maphash (byte-compile (lambda (_key _value _extra) nil)) table)
       (error (list (car err) (nth 2 err))))
     (maphash 42 (make-hash-table)))))
"#;
    let expect = expect_test::expect![[
        r#""OK (((a 1 nil nil) (b 2 nil nil)) ((a 1) (b 2)) (wrong-number-of-arguments 2) (wrong-number-of-arguments 2) nil)""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

#[test]
fn oracle_maphash_bytecode_capture_nested_call_and_symbol_redefinition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (require 'bytecomp)
  (defvar mh-bc-log nil)
  (let ((table (make-hash-table :test 'eq))
        (mh-bc-log nil)
        (name (make-symbol "mh-bc-redefinition")))
    (puthash 'a 1 table)
    (puthash 'b 2 table)
    (puthash 'c 3 table)
    (fset name
          (byte-compile
           (eval `(lambda (key value)
                    (push (list 'old key value) mh-bc-log)
                    (fset ',name (lambda (key value)
                                   (push (list 'new key value) mh-bc-log)))) t)))
    (maphash name table)
    (let ((named (nreverse mh-bc-log)))
      (setq mh-bc-log nil)
      (let ((captured (symbol-function name)))
        (fset name (lambda (_key _value) (error "redefined")))
        (maphash captured table))
      (list named (nreverse mh-bc-log)
            (funcall
             (byte-compile
              (eval '(lambda ()
                       (let ((outer (make-hash-table))
                             (inner (make-hash-table))
                             log)
                         (puthash 'o 10 outer)
                         (puthash 'i 20 inner)
                         (maphash (lambda (ok ov)
                                    (maphash (lambda (ik iv)
                                               (garbage-collect)
                                               (push (list ok ov ik iv) log))
                                             inner))
                                  outer)
                         log)) t)))))))
"#;
    let expect = expect_test::expect![[
        r#""OK (((old a 1) (new b 2) (new c 3)) ((new a 1) (new b 2) (new c 3)) ((o 10 i 20)))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}
