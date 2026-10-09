//! GNU sort.c:137,489-509,604-635 retains and reuses the stack merge array.
// Shared callback state uses special bindings to preserve identity across
// byte-compilation while this regression tests merge-array lifetimes.
use super::*;

#[test]
fn oracle_sort_vector_stack_temporary_root_lifetimes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
      (defvar neovm--gde-stack-v nil)
      (defvar neovm--gde-stack-h nil)
      (defvar neovm--gde-stack-state nil)
      (defvar neovm--gde-stack-trigger nil)
      (defvar neovm--gde-stack-removed nil)
      (defvar neovm--gde-stack-keyed nil)
      (let ((out nil))
      (dolist (shape '(small prefix-tail heap-transition))
        (dolist (compiled '(nil t))
          (dolist (neovm--gde-stack-keyed '(nil t))
          (let* ((numbers
                   (cond
                     ((eq shape 'small)
                      (append (number-sequence 1 127 2) (number-sequence 0 126 2)))
                     ((eq shape 'prefix-tail)
                      (append (number-sequence 1 255 2) (number-sequence 0 254 2)
                              (number-sequence 257 383 2) (number-sequence 256 382 2)
                              (number-sequence 384 511)))
                     (t
                      (append (number-sequence 1 1023 2) (number-sequence 0 1022 2)
                              (number-sequence 1025 1151 2) (number-sequence 1024 1150 2)
                              (number-sequence 1152 2047)))))
                 (neovm--gde-stack-v (vconcat (mapcar (lambda (x) (cons x nil)) numbers)))
                 (neovm--gde-stack-h (make-hash-table :test 'eq :weakness 'key))
                 (neovm--gde-stack-state (vector 0 nil))
                 (neovm--gde-stack-trigger (cond ((eq shape 'small) 2)
                                ((eq shape 'prefix-tail) 258) (t 1026)))
                 (neovm--gde-stack-removed (cond ((eq shape 'small) '(1))
                                ((eq shape 'prefix-tail) '(129)) (t '(1025))))
                 (pred (lambda (a b)
                   (aset neovm--gde-stack-state 0 (1+ (aref neovm--gde-stack-state 0)))
                   (when (and (= (if neovm--gde-stack-keyed a (car a)) neovm--gde-stack-trigger)
                              (= (if neovm--gde-stack-keyed b (car b)) (1+ neovm--gde-stack-trigger)))
                     (dotimes (i (length neovm--gde-stack-v))
                       (when (memq (car-safe (aref neovm--gde-stack-v i)) neovm--gde-stack-removed) (aset neovm--gde-stack-v i nil)))
                     (garbage-collect)
                     (aset neovm--gde-stack-state 1 (cons (hash-table-count neovm--gde-stack-h) (aref neovm--gde-stack-state 1)))
                     (error "stop"))
                   (< (if neovm--gde-stack-keyed a (car a)) (if neovm--gde-stack-keyed b (car b))))))
            (dotimes (i (length neovm--gde-stack-v)) (puthash (aref neovm--gde-stack-v i) t neovm--gde-stack-h))
            (when compiled (setq pred (byte-compile pred)))
            (condition-case nil
                (sort neovm--gde-stack-v :in-place t :key (and neovm--gde-stack-keyed #'car) :lessp pred)
              (error nil))
            (push (list shape compiled neovm--gde-stack-keyed (aref neovm--gde-stack-state 0) (aref neovm--gde-stack-state 1)) out)))))
      (nreverse out)))"#;
    assert_oracle_parity_under_envs_expect(
        form,
        ENVS,
        expect_test::expect![[
            r#""OK ((small nil nil 131 (128)) (small nil t 131 (128)) (small t nil 131 (128)) (small t t 131 (128)) (prefix-tail nil nil 811 (512)) (prefix-tail nil t 811 (512)) (prefix-tail t nil 811 (512)) (prefix-tail t t 811 (512)) (heap-transition nil nil 3122 (2047)) (heap-transition nil t 3122 (2047)) (heap-transition t nil 3122 (2047)) (heap-transition t t 3122 (2047)))""#
        ]],
    );
}
