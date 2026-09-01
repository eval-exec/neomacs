//! Oracle parity tests for `pcase` pattern matching:
//! literal patterns, `_` wildcard, `pred` predicate patterns, `guard` patterns,
//! `app` application patterns, backquote structural patterns, `and`/`or`
//! pattern combinators, `let` binding patterns, `pcase-let`/`pcase-let*`,
//! `pcase-dolist`, `pcase-exhaustive`, and nested pattern combinations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Literal patterns and wildcard
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pcase_literal_and_wildcard() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Integer literal matching
    (pcase 42
      (42 :matched-42)
      (_ :no-match))
    ;; String literal matching
    (pcase "hello"
      ("hello" :matched-hello)
      ("world" :matched-world)
      (_ :no-match))
    ;; Symbol literal matching (quoted)
    (pcase 'foo
      ('foo :is-foo)
      ('bar :is-bar)
      (_ :other))
    ;; nil literal
    (pcase nil
      ('nil :matched-nil)
      (_ :not-nil))
    ;; t literal
    (pcase t
      ('t :matched-t)
      (_ :not-t))
    ;; Wildcard catches anything
    (pcase '(some complex thing)
      (_ :wildcard-catches-all))
    ;; Multiple literal branches
    (mapcar (lambda (x)
              (pcase x
                (1 :one)
                (2 :two)
                (3 :three)
                (_ :other)))
            '(1 2 3 4 5))
    ;; Keyword literal
    (pcase :alpha
      (:alpha :matched-alpha)
      (:beta :matched-beta)
      (_ :other))))"#;
    let expect = expect_test::expect![[
        r#""OK (:matched-42 :matched-hello :is-foo :matched-nil :matched-t :wildcard-catches-all (:one :two :three :other :other) :matched-alpha)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// pred (predicate) patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pcase_pred_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Basic predicate: check type
    (pcase 42
      ((pred numberp) :is-number)
      ((pred stringp) :is-string)
      (_ :other))
    ;; String predicate
    (pcase "hello"
      ((pred stringp) :is-string)
      (_ :other))
    ;; Lambda predicate
    (pcase 15
      ((pred (lambda (x) (> x 10))) :greater-than-10)
      (_ :not-greater))
    ;; Predicate on list
    (pcase '(1 2 3)
      ((pred listp) :is-list)
      (_ :not-list))
    ;; Multiple pred patterns with fallthrough
    (mapcar (lambda (val)
              (pcase val
                ((pred integerp) :integer)
                ((pred floatp) :float)
                ((pred stringp) :string)
                ((pred symbolp) :symbol)
                ((pred consp) :cons)
                (_ :unknown)))
            (list 42 3.14 "hi" 'foo '(1 . 2) [1 2]))
    ;; pred with zerop
    (mapcar (lambda (n)
              (pcase n
                ((pred zerop) :zero)
                ((pred (lambda (x) (> x 0))) :positive)
                (_ :negative)))
            '(-3 -1 0 1 5))
    ;; pred with null
    (mapcar (lambda (x)
              (pcase x
                ((pred null) :empty)
                ((pred consp) :pair)
                (_ :atom)))
            '(nil (1 2) 42 "str"))))"#;
    let expect = expect_test::expect![[
        r#""OK (:is-number :is-string :greater-than-10 :is-list (:integer :float :string :symbol :cons :unknown) (:negative :negative :zero :positive :positive) (:empty :pair :atom :atom))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// guard patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pcase_guard_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Guard on bound variable
    (pcase 42
      ((and x (guard (> x 30))) (list :large x))
      ((and x (guard (> x 10))) (list :medium x))
      (_ :small))
    ;; Guard with multiple conditions
    (mapcar (lambda (n)
              (pcase n
                ((and x (guard (= (% x 15) 0))) :fizzbuzz)
                ((and x (guard (= (% x 3) 0))) :fizz)
                ((and x (guard (= (% x 5) 0))) :buzz)
                (x x)))
            '(1 2 3 4 5 6 9 10 15 30))
    ;; Guard in combination with pred
    (pcase '(42 "hello")
      ((and x (pred listp) (guard (= (length x) 2)))
       (list :pair-of-two (car x) (cadr x)))
      (_ :not-pair))
    ;; Guard with string length
    (mapcar (lambda (s)
              (pcase s
                ((and x (guard (> (length x) 5))) :long)
                ((and x (guard (> (length x) 2))) :medium)
                (_ :short)))
            '("hi" "hey" "hello" "greetings"))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:large 42) (1 2 :fizz 4 :buzz :fizz :fizz :buzz :fizzbuzz :fizzbuzz) (:pair-of-two 42 \"hello\") (:short :medium :medium :long))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// app (application) patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pcase_app_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; app applies function and matches result
    (pcase '(1 2 3 4 5)
      ((app length 5) :five-elements)
      ((app length 3) :three-elements)
      (_ :other-length))
    ;; app with car/cdr
    (pcase '(hello world)
      ((app car 'hello) :starts-with-hello)
      (_ :other))
    ;; app with custom function binding result
    (pcase "HELLO WORLD"
      ((app downcase result) (list :downcased result)))
    ;; app with string-to-number
    (pcase "42"
      ((app string-to-number n) (list :parsed n (* n 2))))
    ;; Nested app: apply length then check
    (mapcar (lambda (lst)
              (pcase lst
                ((app length (pred zerop)) :empty)
                ((app length (pred (lambda (n) (> n 3)))) :long)
                (_ :short-or-medium)))
            '(nil (1) (1 2 3) (1 2 3 4 5)))
    ;; app with abs for magnitude classification
    (mapcar (lambda (n)
              (pcase n
                ((app abs (and mag (guard (> mag 100)))) (list :huge mag))
                ((app abs (and mag (guard (> mag 10)))) (list :big mag))
                (_ :small)))
            '(5 -15 42 -200 3 150))))"#;
    let expect = expect_test::expect![[
        r#""OK (:five-elements :starts-with-hello (:downcased \"hello world\") (:parsed 42 84) (:empty :short-or-medium :short-or-medium :long) (:small (:big 15) (:big 42) (:huge 200) :small (:huge 150)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backquote structural patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pcase_backquote_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; Match fixed structure with variable parts
    (pcase '(point 3 7)
      (`(point ,x ,y) (list :point-at x y))
      (_ :not-a-point))
    ;; Match nested structure
    (pcase '(rect (point 0 0) (point 10 20))
      (`(rect (point ,x1 ,y1) (point ,x2 ,y2))
       (list :area (* (- x2 x1) (- y2 y1))))
      (_ :not-a-rect))
    ;; Match cons cell structure
    (pcase '(1 . 2)
      (`(,a . ,b) (list :pair a b)))
    ;; Match list prefix
    (pcase '(define foo (+ 1 2))
      (`(define ,name ,body) (list :defined name body))
      (_ :unknown-form))
    ;; Match with literal symbols in structure
    (mapcar (lambda (expr)
              (pcase expr
                (`(+ ,a ,b) (+ a b))
                (`(- ,a ,b) (- a b))
                (`(* ,a ,b) (* a b))
                (`(/ ,a ,b) (/ a b))
                (_ :unknown-op)))
            '((+ 3 4) (- 10 3) (* 5 6) (/ 20 4) (% 7 3)))
    ;; Nested backquote: match a let-like form
    (pcase '(let ((x 10) (y 20)) (+ x y))
      (`(let ,bindings ,body)
       (list :bindings bindings :body body))
      (_ :not-let))
    ;; Match with rest element using ,@rest style via nested
    (pcase '(fn alpha beta gamma)
      (`(fn . ,args) (list :fn-args args (length args)))
      (_ :not-fn))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:point-at 3 7) (:area 200) (:pair 1 2) (:defined foo (+ 1 2)) (7 7 30 5 :unknown-op) (:bindings ((x 10) (y 20)) :body (+ x y)) (:fn-args (alpha beta gamma) 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// and / or pattern combinators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pcase_and_or_combinators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; or: match any of several patterns
    (mapcar (lambda (x)
              (pcase x
                ((or 1 2 3) :low)
                ((or 4 5 6) :mid)
                ((or 7 8 9) :high)
                (_ :out-of-range)))
            '(0 1 3 5 7 9 10))
    ;; and: require multiple conditions
    (mapcar (lambda (x)
              (pcase x
                ((and (pred integerp) (pred (lambda (n) (> n 0))) n)
                 (list :positive-int n))
                ((and (pred integerp) n)
                 (list :non-positive-int n))
                (_ :not-int)))
            '(5 -3 0 "hello" 42))
    ;; or with binding: first matching branch binds
    (pcase '(error "file not found")
      ((or `(error ,msg) `(warning ,msg))
       (list :message msg))
      (_ :ok))
    ;; and combining pred + guard + binding
    (pcase 42
      ((and (pred integerp)
            (pred (lambda (x) (= (% x 2) 0)))
            x
            (guard (> x 10)))
       (list :even-and-big x))
      (_ :nope))
    ;; Nested or inside and
    (mapcar (lambda (x)
              (pcase x
                ((and (pred numberp)
                      (or (pred (lambda (n) (> n 100)))
                          (pred (lambda (n) (< n -100)))))
                 :extreme)
                ((pred numberp) :moderate)
                (_ :not-number)))
            '(50 200 -150 "hi" 0 999))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:out-of-range :low :low :mid :high :high :out-of-range) ((:positive-int 5) (:non-positive-int -3) (:non-positive-int 0) :not-int (:positive-int 42)) (:message \"file not found\") (:even-and-big 42) (:moderate :extreme :extreme :not-number :moderate :extreme))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let binding patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pcase_let_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; let pattern: match if bound expression is non-nil
    (pcase '((name . "Alice") (age . 30) (city . "NYC"))
      ((and x (let name (cdr (assq 'name x)))
              (let age (cdr (assq 'age x))))
       (list :person name age))
      (_ :not-found))
    ;; let for computed match
    (pcase 100
      ((and x (let half (/ x 2))
              (guard (= (* half 2) x)))
       (list :even-number x :half half))
      (_ :odd))
    ;; let with string operations
    (pcase "hello world"
      ((and s (let len (length s))
              (let up (upcase s)))
       (list :original s :length len :upper up)))
    ;; Multiple let bindings in sequence
    (pcase '(3 4)
      ((and `(,a ,b)
            (let hyp (sqrt (+ (* a a) (* b b)))))
       (list :sides a b :hypotenuse hyp))
      (_ :not-a-pair))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:person \"Alice\" 30) (:even-number 100 :half 50) (:original \"hello world\" :length 11 :upper \"HELLO WORLD\") (:sides 3 4 :hypotenuse 5.0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// pcase-let and pcase-let*
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pcase_let_star() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; pcase-let: destructure with backquote patterns
    (pcase-let ((`(,a ,b ,c) '(10 20 30)))
      (list :sum (+ a b c) :product (* a b c)))
    ;; pcase-let with dotted pair
    (pcase-let ((`(,key . ,val) '(alpha . 42)))
      (list :key key :val val))
    ;; pcase-let with multiple bindings
    (pcase-let ((`(,x ,y) '(3 4))
                (`(,a ,b) '(5 6)))
      (list :sum-xy (+ x y) :sum-ab (+ a b)
            :cross (- (* x b) (* y a))))
    ;; pcase-let*: sequential destructuring (earlier bindings visible later)
    (pcase-let* ((`(,a ,b) '(10 20))
                 (`(,sum ,diff) (list (+ a b) (- a b))))
      (list :a a :b b :sum sum :diff diff))
    ;; pcase-let* with nested structure
    (pcase-let* ((`(,op . ,args) '(+ 1 2 3 4))
                 (result (apply op args)))
      (list :op op :args args :result result))
    ;; pcase-let with vector-like patterns via app
    (pcase-let ((`(,first ,second . ,rest) '(a b c d e f)))
      (list :first first :second second :rest rest))
    ;; pcase-let* chained computation
    (pcase-let* ((`(,w ,h) '(640 480))
                 (area (* w h))
                 (aspect (/ (float w) h))
                 (diagonal (sqrt (+ (* w w) (* h h)))))
      (list :width w :height h :area area
            :aspect aspect :diagonal diagonal))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:sum 60 :product 6000) (:key alpha :val 42) (:sum-xy 7 :sum-ab 11 :cross -2) (:a 10 :b 20 :sum 30 :diff -10) (:op + :args (1 2 3 4) :result 10) (:first a :second b :rest (c d e f)) (:width 640 :height 480 :area 307200 :aspect 1.3333333333333333 :diagonal 800.0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// pcase-dolist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pcase_dolist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; pcase-dolist: destructure each element
    (let ((result nil))
      (pcase-dolist (`(,name . ,score)
                     '(("Alice" . 95) ("Bob" . 87) ("Carol" . 92)))
        (when (>= score 90)
          (push (list name :honor-roll) result)))
      (nreverse result))
    ;; pcase-dolist with three-element lists
    (let ((total-area 0))
      (pcase-dolist (`(,shape ,w ,h)
                     '((rect 10 20) (rect 5 8) (rect 3 3)))
        (setq total-area (+ total-area (* w h))))
      total-area)
    ;; pcase-dolist building transformed output
    (let ((output nil))
      (pcase-dolist (`(,op ,a ,b)
                     '((+ 1 2) (- 10 3) (* 4 5) (+ 100 200)))
        (push (list op (cond ((eq op '+) (+ a b))
                             ((eq op '-) (- a b))
                             ((eq op '*) (* a b))
                             (t 0)))
              output))
      (nreverse output))
    ;; pcase-dolist with deeper nesting
    (let ((coords nil))
      (pcase-dolist (`(,label (,x ,y))
                     '(("origin" (0 0)) ("unit-x" (1 0))
                       ("unit-y" (0 1)) ("diag" (1 1))))
        (push (list label :dist (sqrt (+ (* x x) (* y y)))) coords))
      (nreverse coords))
    ;; pcase-dolist filtering with when
    (let ((evens nil) (odds nil))
      (pcase-dolist (`(,idx ,val)
                     '((0 10) (1 15) (2 20) (3 25) (4 30) (5 35)))
        (if (= (% val 2) 0)
            (push (list idx val) evens)
          (push (list idx val) odds)))
      (list :evens (nreverse evens) :odds (nreverse odds)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"Alice\" :honor-roll) (\"Carol\" :honor-roll)) 249 ((+ 3) (- 7) (* 20) (+ 300)) ((\"origin\" :dist 0.0) (\"unit-x\" :dist 1.0) (\"unit-y\" :dist 1.0) (\"diag\" :dist 1.4142135623730951)) (:evens ((0 10) (2 20) (4 30)) :odds ((1 15) (3 25) (5 35))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// pcase-exhaustive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pcase_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
    ;; pcase-exhaustive: matches succeed
    (pcase-exhaustive 42
      ((pred integerp) :integer))
    ;; pcase-exhaustive with multiple branches
    (mapcar (lambda (x)
              (pcase-exhaustive x
                ((pred integerp) :int)
                ((pred stringp) :str)
                ((pred symbolp) :sym)
                ((pred consp) :cons)
                (_ :other)))
            '(1 "hello" foo (a . b) [1 2]))
    ;; pcase-exhaustive with backquote
    (pcase-exhaustive '(color red 255)
      (`(color ,name ,intensity)
       (list :color name :intensity intensity)))
    ;; pcase-exhaustive error case: should signal error
    (condition-case err
        (pcase-exhaustive 42
          ((pred stringp) :string))
      (error (list :caught (car err))))))"#;
    let expect = expect_test::expect![[
        r#""OK (:integer (:int :str :sym :cons :other) (:color red :intensity 255) (:caught error))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex nested pattern matching (combining multiple pattern types)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pcase_complex_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  ;; Build a simple expression evaluator using pcase
  (fset 'neovm--pcase-eval-expr
    (lambda (expr env)
      "Evaluate a tiny expression language using pcase dispatch."
      (pcase expr
        ;; Literal numbers
        ((pred numberp) expr)
        ;; Variable lookup
        ((pred symbolp)
         (let ((binding (assq expr env)))
           (if binding (cdr binding)
             (error "Unbound: %s" expr))))
        ;; Arithmetic operations
        (`(+ ,a ,b) (+ (funcall 'neovm--pcase-eval-expr a env)
                       (funcall 'neovm--pcase-eval-expr b env)))
        (`(- ,a ,b) (- (funcall 'neovm--pcase-eval-expr a env)
                       (funcall 'neovm--pcase-eval-expr b env)))
        (`(* ,a ,b) (* (funcall 'neovm--pcase-eval-expr a env)
                       (funcall 'neovm--pcase-eval-expr b env)))
        ;; Let binding
        (`(let1 ,var ,val ,body)
         (let ((v (funcall 'neovm--pcase-eval-expr val env)))
           (funcall 'neovm--pcase-eval-expr body (cons (cons var v) env))))
        ;; Conditional
        (`(if0 ,cond ,then ,else)
         (if (= 0 (funcall 'neovm--pcase-eval-expr cond env))
             (funcall 'neovm--pcase-eval-expr then env)
           (funcall 'neovm--pcase-eval-expr else env)))
        (_ (error "Unknown expression: %S" expr)))))

  (unwind-protect
      (let ((env '((x . 10) (y . 20) (z . 3))))
        (list
         ;; Literal
         (funcall 'neovm--pcase-eval-expr 42 nil)
         ;; Variable
         (funcall 'neovm--pcase-eval-expr 'x env)
         ;; Arithmetic
         (funcall 'neovm--pcase-eval-expr '(+ x y) env)
         (funcall 'neovm--pcase-eval-expr '(* z (+ x y)) env)
         ;; Nested let
         (funcall 'neovm--pcase-eval-expr
                  '(let1 a 5 (let1 b 7 (+ a (* b z))))
                  env)
         ;; Conditional
         (funcall 'neovm--pcase-eval-expr
                  '(if0 0 (+ 100 x) (- x 100))
                  env)
         (funcall 'neovm--pcase-eval-expr
                  '(if0 1 (+ 100 x) (- x 100))
                  env)))
    (fmakunbound 'neovm--pcase-eval-expr)))"#;
    let expect = expect_test::expect![[r#""OK (42 10 30 90 26 110 -90)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
