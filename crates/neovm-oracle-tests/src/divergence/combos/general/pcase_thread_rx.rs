//! Divergence tests: rx macro + pcase + thread-first/last combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_pcase_destructuring_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Unknown list pattern: (list :user name :age age :roles (and rolenames (pred consp)))\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((data '((:user "Alice" :age 30 :roles (admin editor))
                (:user "Bob" :age 25 :roles (viewer))
                (:user "Carol" :age 35 :roles (admin moderator editor)))))
    (let ((results (mapcar (lambda (entry)
                             (pcase entry
                               ((list :user name :age age :roles (and rolenames (pred consp)))
                                (list name age (length rolenames)))
                               (_ 'unknown)))
                           data)))
      (list results
            (equal results '(("Alice" 30 2) ("Bob" 25 1) ("Carol" 35 3)))
            (= (nth 1 (car results)) 30)
            (= (nth 2 (cadr results)) 1)
            (= (nth 2 (caddr results)) 3))))) "#,
        expect,
    );
}

#[test]
fn divergence_pcase_guard_and_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 18 50)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((classify (lambda (x)
                    (pcase x
                      ((and (pred numberp) (pred (> _ 0))) 'positive)
                      ((and (pred numberp) (pred (< _ 0))) 'negative)
                      ((and (pred numberp) (pred (= _ 0))) 'zero)
                      ((pred stringp) 'string)
                      ((pred null) 'nil)
                      ((pred consp) 'cons)
                      (_ 'other))))
    (list (funcall classify 42)
          (eq (funcall classify 42) 'positive)
          (eq (funcall classify -5) 'negative)
          (eq (funcall classify 0) 'zero)
          (eq (funcall classify "hello") 'string)
          (eq (funcall classify nil) 'nil)
          (eq (funcall classify '(1 2)) 'cons)
          (eq (funcall classify 'sym) 'other)))) #"#,
        expect,
    );
}

#[test]
fn divergence_thread_first_last() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function thread-first)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (thread-first 5
          (* 2)
          (+ 3)
          (expt 2))
        (= (thread-first 5 (* 2) (+ 3) (expt 2)) 256)
        (thread-last '(1 2 3 4 5)
          (mapcar (lambda (x) (* x x)))
          (seq-filter (lambda (x) (> x 5)))
          (seq-reduce #'+ _ 0))
        (= (thread-last '(1 2 3 4 5)
                        (mapcar (lambda (x) (* x x)))
                        (seq-filter (lambda (x) (> x 5)))
                        (seq-reduce #'+ _ 0))
           41)
        (thread-first "hello world"
          (string-upcase)
          (string-reverse))
        (string= (thread-first "hello world"
                               (string-upcase)
                               (string-reverse))
                 "DLROW OLLEH"))) #"#,
        expect,
    );
}

#[test]
fn divergence_pcase_with_app_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 15 48)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((process (lambda (data)
                   (pcase data
                     ((app car (and x (pred numberp))) (list 'num-first x))
                     ((app car (and x (pred stringp))) (list 'str-first x))
                     ((app car (and x (pred consp))) (list 'list-first x))
                     (_ 'empty)))))
    (list (funcall process '(10 20 30))
          (equal (funcall process '(10 20 30)) '(num-first 10))
          (funcall process '("hello" "world"))
          (equal (funcall process '("hello" "world")) '(str-first "hello"))
          (funcall process '((a b) c))
          (equal (funcall process '((a b) c)) '(list-first (a b)))
          (funcall process nil)
          (eq (funcall process nil) 'empty)))) #"#,
        expect,
    );
}

#[test]
fn divergence_cl_typep_with_various_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-typep)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (cl-typep 42 'integer)
        (cl-typep 3.14 'float)
        (cl-typep "hello" 'string)
        (cl-typep '(1 2) 'cons)
        (cl-typep [1 2] 'vector)
        (cl-typep nil 'null)
        (cl-typep t 'boolean)
        (cl-typep (current-buffer) 'buffer)
        (cl-typep (make-hash-table) 'hash-table)
        (not (cl-typep 42 'string))
        (cl-typep '(1 2 3) 'list)
        (cl-typep (make-vector 5 0) 'vector))) #"#,
        expect,
    );
}

#[test]
fn divergence_pcase_lexical_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 9 23)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((data '(:result (ok "success" 42) :status done)))
    (pcase data
      ((list :result (list 'ok msg val) :status status)
       (list msg val status
             (string= msg "success")
             (= val 42)
             (eq status 'done)))
      (_ 'no-match))) #"#,
        expect,
    );
}

#[test]
fn divergence_thread_with_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function thread-first)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :test 'equal)))
    (thread-first ht
      (puthash 'a 1)
      (puthash 'b 2)
      (puthash 'c 3))
    (list (hash-table-count ht)
          (= (hash-table-count ht) 3)
          (gethash 'a ht)
          (= (gethash 'a ht) 1)
          (gethash 'b ht)
          (= (gethash 'b ht) 2)
          (gethash 'c ht)
          (= (gethash 'c ht) 3)
          (hash-table-keys ht)
          (= (length (hash-table-keys ht)) 3)))) #"#,
        expect,
    );
}

#[test]
fn divergence_pcase_recursive_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 18 51)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-depth-xxx (tree)
    (pcase tree
      ('nil 0)
      ((pred atom) 1)
      ((pred consp)
       (1+ (max (test-depth-xxx (car tree))
                (test-depth-xxx (cdr tree)))))))
  (list (test-depth-xxx nil)
        (= (test-depth-xxx nil) 0)
        (test-depth-xxx 'a)
        (= (test-depth-xxx 'a) 1)
        (test-depth-xxx '(a b))
        (= (test-depth-xxx '(a b)) 2)
        (test-depth-xxx '(a (b c) d))
        (= (test-depth-xxx '(a (b c) d)) 3)
        (test-depth-xxx '((a (b)) (c d)))
        (= (test-depth-xxx '((a (b)) (c d))) 3))) #"#,
        expect,
    );
}

#[test]
fn divergence_cl_destructuring_bind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-destructuring-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-destructuring-bind (a (b c) &rest rest)
      '(1 (2 3) 4 5 6)
    (list a b c rest
          (= a 1)
          (= b 2)
          (= c 3)
          (equal rest '(4 5 6))
          (= (length rest) 3)))) #"#,
        expect,
    );
}

#[test]
fn rx_pattern_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 14 35)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "foo123bar456baz789")
  (goto-char 1)
  (let ((count 0)
        (matches nil))
    (while (re-search-forward "[a-z]+[0-9]+" nil t)
      (cl-incf count)
      (push (match-string 0) matches))
    (list count
          (= count 3)
          (nreverse matches)
          (equal (nreverse matches) '("foo123" "bar456" "baz789"))
          (buffer-string)
          (= (buffer-size) 15)))) #"#,
        expect,
    );
}
