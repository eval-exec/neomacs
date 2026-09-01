//! Divergence tests: cl-macs + pcase + rx + thread combo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_cl_destructuring_bind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-destructuring-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-destructuring-bind (a (b c) &rest rest) '(1 (2 3) 4 5 6)
    (list a (= a 1)
          b (= b 2)
          c (= c 3)
          rest (equal rest '(4 5 6))))
  (cl-destructuring-bind (&key x y z) '(:y 20 :x 10 :z 30)
    (list x (= x 10) y (= y 20) z (= z 30)))
  (cl-destructuring-bind (&aux (extra 99) val) '(:val 42)
    (list extra val (= extra 99) (null val)))) "#,
        expect,
    );
}

#[test]
fn divergence_pcase_complex_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Unknown list pattern: (list a b c)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (pcase '(1 2 3)
          ((or `(,a ,b ,c) (list a b c)) (+ a b c)))
        (= (pcase '(1 2 3)
             ((or `(,a ,b ,c) (list a b c)) (+ a b c)))
           6)
        (pcase 'hello
          ((or 'hello 'world) 'greeting)
          (_ 'unknown))
        (eq (pcase 'hello
              ((or 'hello 'world) 'greeting)
              (_ 'unknown))
            'greeting)
        (pcase '(1 "two" three)
          (`(1 ,b ,c) (list b c)))
        (equal (pcase '(1 "two" three)
                 (`(1 ,b ,c) (list b c)))
               '("two" three))
        (pcase '(a b c d e)
          (`(a ,b . ,rest) (list b rest)))
        (equal (pcase '(a b c d e)
                 (`(a ,b . ,rest) (list b rest)))
               '(b (c d e)))
        (pcase 42
          ((and (pred integerp) x) (+ x 1)))
        (= (pcase 42
             ((and (pred integerp) x) (+ x 1)))
           43)
        (pcase '(1 2)
          ((app length 2) 'two-elements))
        (eq (pcase '(1 2)
              ((app length 2) 'two-elements))
            'two-elements)))) "#,
        expect,
    );
}

#[test]
fn divergence_pcase_guard_and_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable it)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (pcase 42
          ((and x (guard (> x 10))) 'big)
          ((and x (guard (<= x 10))) 'small))
        (eq (pcase 42
              ((and x (guard (> x 10))) 'big)
              ((and x (guard (<= x 10))) 'small))
            'big)
        (pcase 5
          ((and x (guard (> x 10))) 'big)
          ((and x (guard (<= x 10))) 'small))
        (eq (pcase 5
              ((and x (guard (> x 10))) 'big)
              ((and x (guard (<= x 10))) 'small))
            'small)
        (pcase '(1 2 3)
          (`(,a ,b ,c) (let ((sum (+ a b c))) (* sum sum))))
        (= (pcase '(1 2 3)
             (`(,a ,b ,c) (let ((sum (+ a b c))) (* sum sum))))
           36)
        (pcase "hello"
          ((pred stringp) (length it)))
        (= (pcase "hello"
              ((pred stringp) (length it)))
           5)
        (pcase '(ok 42)
          (`(ok ,v) v))
        (= (pcase '(ok 42)
              (`(ok ,v) v))
           42)
        (pcase '(error "bad")
          (`(error ,msg) msg))
        (string= (pcase '(error "bad")
                   (`(error ,msg) msg))
                 "bad")))) "#,
        expect,
    );
}

#[test]
fn divergence_rx_macro_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 0 t t t 0 t 0 t t 0 t \"123\" t \"456\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((r1 (rx bos (+ (any "a-zA-Z")) eos))
        (r2 (rx "hello" (zero-or-more space) "world"))
        (r3 (rx (group (+ digit)) "-" (group (+ digit)))))
    (list (stringp r1)
          (string-match r1 "Hello")
          (= (string-match r1 "Hello") 0)
          (not (string-match r1 "123"))
          (stringp r2)
          (string-match r2 "hello   world")
          (= (string-match r2 "hello   world") 0)
          (string-match r2 "helloworld")
          (= (string-match r2 "helloworld") 0)
          (stringp r3)
          (string-match r3 "123-456")
          (= (string-match r3 "123-456") 0)
          (match-string 1 "123-456")
          (string= (match-string 1 "123-456") "123")
          (match-string 2 "123-456")
          (string= (match-string 2 "123-456") "456")))) "#,
        expect,
    );
}

#[test]
fn divergence_rx_with_substitution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 4 nil t \"\\\\(?:hello[[:digit:]]+\\\\)\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((word "test")
        (count 3))
    (let ((r (rx-to-string `(seq (or ,word ,word) (repeat ,count digit)))))
      (list (stringp r)
            (string-match r "testtest123")
            (= (string-match r "testtest123") 0)
            (not (string-match r "nope123"))
            (rx-to-string '(: "hello" (+ digit)))
            (stringp (rx-to-string '(: "hello" (+ digit)))))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_typep_and_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-typep)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (cl-typep 42 'integer)
        (cl-typep "hello" 'string)
        (cl-typep '(1 2 3) 'list)
        (cl-typep [1 2 3] 'vector)
        (cl-typep 3.14 'float)
        (cl-typep nil 'null)
        (cl-typep t 'boolean)
        (not (cl-typep 42 'string))
        (not (cl-typep "hello" 'integer))
        (cl-typep '(1 2 3) '(list integer))
        (cl-typep '(1 "two" 3) '(list integer string integer))
        (cl-check-type 42 integer)
        (null (cl-check-type 42 integer))
        (condition-case err
            (cl-check-type "not-int" integer)
          (wrong-type-argument (car err)))
        (eq (condition-case err
               (cl-check-type "not-int" integer)
             (wrong-type-argument (car err)))
            'wrong-type-argument)))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_with_accumulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (cl-loop for x in '(1 2 3 4 5) sum (* x x))
        (= (cl-loop for x in '(1 2 3 4 5) sum (* x x)) 55)
        (cl-loop for x in '(1 2 3 4 5) count (cl-evenp x))
        (= (cl-loop for x in '(1 2 3 4 5) count (cl-evenp x)) 2)
        (cl-loop for x in '(1 2 3 4 5) maximize x)
        (= (cl-loop for x in '(1 2 3 4 5) maximize x) 5)
        (cl-loop for x in '(5 4 3 2 1) minimize x)
        (= (cl-loop for x in '(5 4 3 2 1) minimize x) 1)
        (cl-loop for x from 1 to 5 collect (* x 10))
        (equal (cl-loop for x from 1 to 5 collect (* x 10))
               '(10 20 30 40 50))
        (cl-loop for x across [10 20 30] sum x)
        (= (cl-loop for x across [10 20 30] sum x) 60)
        (cl-loop for i from 0 below 5
                 for c = (char-to-string (+ ?A i))
                 concat c)
        (string= (cl-loop for i from 0 below 5
                          for c = (char-to-string (+ ?A i))
                          concat c)
                 "ABCDE")
        (cl-loop for x in '(a b c d e)
                 for y in '(1 2 3 4 5)
                 collect (cons x y))
        (equal (cl-loop for x in '(a b c d e)
                        for y in '(1 2 3 4 5)
                        collect (cons x y))
               '((a . 1) (b . 2) (c . 3) (d . 4) (e . 5))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_with_conditions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (cl-loop for x in '(1 2 3 4 5 6 7 8 9)
                 when (cl-evenp x) collect x)
        (equal (cl-loop for x in '(1 2 3 4 5 6 7 8 9)
                        when (cl-evenp x) collect x)
               '(2 4 6 8))
        (cl-loop for x in '(1 2 3 4 5) if (cl-oddp x) sum x)
        (= (cl-loop for x in '(1 2 3 4 5) if (cl-oddp x) sum x) 9)
        (cl-loop for x from 1 to 10
                 while (< x 5) collect x)
        (equal (cl-loop for x from 1 to 10
                        while (< x 5) collect x)
               '(1 2 3 4))
        (cl-loop for x from 1
                 repeat 5 collect x)
        (equal (cl-loop for x from 1
                        repeat 5 collect x)
               '(1 2 3 4 5))
        (cl-loop for x in '(1 nil 3 nil 5)
                 when x collect (* x 10))
        (equal (cl-loop for x in '(1 nil 3 nil 5)
                        when x collect (* x 10))
               '(10 30 50))
        (cl-loop with total = 0
                 for x in '(1 2 3 4 5)
                 do (setq total (+ total x))
                 finally return total)
        (= (cl-loop with total = 0
                    for x in '(1 2 3 4 5)
                    do (setq total (+ total x))
                    finally return total)
           15))) "#,
        expect,
    );
}

#[test]
fn divergence_thread_last_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function thread-last)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (thread-last 5 (+ 1) (* 2) (- 3))
        (= (thread-last 5 (+ 1) (* 2) (- 3)) 9)
        (thread-last '(3 1 4 1 5)
          (seq-filter #'cl-evenp)
          (seq-map #'1+))
        (equal (thread-last '(3 1 4 1 5)
                  (seq-filter #'cl-evenp)
                  (seq-map #'1+))
               '(5))
        (thread-first '(1 2 3 4 5)
          (seq-map #'1+)
          (seq-filter (lambda (x) (> x 3)))
          (seq-reduce #'+ nil 0))
        (= (thread-first '(1 2 3 4 5)
              (seq-map #'1+)
              (seq-filter (lambda (x) (> x 3)))
              (seq-reduce #'+ nil 0))
           14)
        (thread-last "  hello world  "
          (string-trim)
          (upcase)
          (substring 0 5))
        (string= (thread-last "  hello world  "
                    (string-trim)
                    (upcase)
                    (substring 0 5))
                 "HELLO")
        (thread-first 10
          (+ 5)
          (* 2)
          (number-to-string)
          (concat "result: "))
        (string= (thread-first 10
                    (+ 5)
                    (* 2)
                    (number-to-string)
                    (concat "result: "))
                 "result: 30"))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_defmacro_with_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defmacro)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defmacro test-dmd-xxx ((a b) &key (c 10) (d 20))
    `(list ,a ,b ,c ,d))
  (list (eval (macroexpand '(test-dmd-xxx (1 2))))
        (equal (eval (macroexpand '(test-dmd-xxx (1 2)))) '(1 2 10 20))
        (eval (macroexpand '(test-dmd-xxx (3 4) :c 30 :d 40)))
        (equal (eval (macroexpand '(test-dmd-xxx (3 4) :c 30 :d 40)))
               '(3 4 30 40))
        (cl-defmacro test-dmd2-xxx (x &rest args)
          `(list ,x (length ',args) ',args))
        (eval (macroexpand '(test-dmd2-xxx 'a 1 2 3)))
        (equal (eval (macroexpand '(test-dmd2-xxx 'a 1 2 3)))
               '(a 3 (1 2 3))))) "#,
        expect,
    );
}
