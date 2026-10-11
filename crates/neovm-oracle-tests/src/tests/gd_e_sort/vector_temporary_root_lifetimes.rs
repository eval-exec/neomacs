use super::*;

#[test]
fn oracle_sort_vector_temporary_root_lifetimes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((out nil))
      (dolist (keyed '(nil t))
        (let ((v (vector (cons 3 nil) (cons 2 nil) (cons 1 nil) (cons 5 nil) (cons 4 nil)))
              (h (make-hash-table :test 'eq :weakness 'key)) (n 0) (seen nil))
          (dotimes (i (length v)) (puthash (aref v i) t h))
          (condition-case nil
              (sort v :in-place t :key (and keyed #'car)
                :lessp (lambda (a b)
                  (setq n (1+ n))
                  (when (= n (if keyed 4 6))
                    (dotimes (i (length v))
                      (when (eq (car-safe (aref v i)) 5) (aset v i nil)))
                    (garbage-collect) (push (hash-table-count h) seen) (error "stop"))
                  (< (if keyed a (car a)) (if keyed b (car b)))))
            (error nil))
          (push (list keyed n seen v) out)))
      (let* ((v (vconcat (mapcar (lambda (x) (cons x nil)) (number-sequence 1 2047 2))
                         (mapcar (lambda (x) (cons x nil)) (number-sequence 0 2046 2))))
             (h (make-hash-table :test 'eq :weakness 'key)) (n 0) (seen nil))
        (dotimes (i (length v)) (puthash (aref v i) t h))
        (condition-case nil
            (sort v :in-place t :lessp (lambda (a b)
              (setq n (1+ n))
              (when (and (= (car a) 2) (= (car b) 3))
                (dotimes (i (length v))
                  (when (eq (car-safe (aref v i)) 1) (aset v i nil)))
                (garbage-collect) (push (hash-table-count h) seen) (error "stop"))
              (< (car a) (car b))))
          (error nil))
        (push (list n seen (aref v 0) (aref v 1)) out))
      (nreverse out))"#;
    assert_oracle_parity_under_envs_expect(
        form,
        ENVS,
        expect_test::expect![[
            r#""OK ((nil 6 (4) [(1) (2) (3) nil (4)]) (t 4 (4) [(1) (2) (3) nil (4)]) (2051 (2047) (0) nil))""#
        ]],
    );
}
