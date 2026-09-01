//! Oracle parity tests for Church encoding in Elisp (pure lambda calculus
//! encoded as Elisp closures): Church numerals, successor, addition,
//! multiplication, Church booleans (true, false, if), Church pairs (cons, car,
//! cdr), Church-to-integer conversion, predecessor (Kleene's method),
//! and Church list operations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Church numerals: constructing 0..5, converting to integers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_church_encoding_numerals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* (;; Church numeral 0: apply f zero times
           (c0 (lambda (f) (lambda (x) x)))
           ;; Successor: given n, apply f one more time
           (succ (lambda (n)
                   (lambda (f)
                     (lambda (x)
                       (funcall f (funcall (funcall n f) x))))))
           ;; to-int: convert Church numeral to Elisp integer
           (to-int (lambda (n) (funcall (funcall n #'1+) 0)))
           ;; from-int: convert Elisp integer to Church numeral
           (from-int (lambda (k)
                       (let ((result c0)
                             (i 0))
                         (while (< i k)
                           (setq result (funcall succ result))
                           (setq i (1+ i)))
                         result)))
           ;; Build numerals
           (c1 (funcall succ c0))
           (c2 (funcall succ c1))
           (c3 (funcall succ c2))
           (c4 (funcall succ c3))
           (c5 (funcall succ c4))
           (c6 (funcall succ c5))
           (c7 (funcall succ c6)))
  (list
   ;; Basic conversion
   (funcall to-int c0)
   (funcall to-int c1)
   (funcall to-int c2)
   (funcall to-int c3)
   (funcall to-int c5)
   (funcall to-int c7)
   ;; Roundtrip: int -> church -> int
   (funcall to-int (funcall from-int 0))
   (funcall to-int (funcall from-int 4))
   (funcall to-int (funcall from-int 10))
   ;; Church numeral as function application count
   (funcall (funcall c3 (lambda (s) (concat s "!"))) "hi")
   (funcall (funcall c5 (lambda (n) (* n 2))) 1)))"#;
    let expect = expect_test::expect![[r#""OK (0 1 2 3 5 7 0 4 10 \"hi!!!\" 32)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church successor, addition, multiplication
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_church_encoding_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((c0 (lambda (f) (lambda (x) x)))
           (succ (lambda (n) (lambda (f) (lambda (x) (funcall f (funcall (funcall n f) x))))))
           (to-int (lambda (n) (funcall (funcall n #'1+) 0)))
           ;; Addition: m + n = apply f m times, then n times
           (add (lambda (m n) (lambda (f) (lambda (x)
                  (funcall (funcall m f) (funcall (funcall n f) x))))))
           ;; Multiplication: m * n = apply (n f) m times
           (mul (lambda (m n) (lambda (f) (funcall m (funcall n f)))))
           ;; Power: m^n = funcall n m (Church encoding identity!)
           (power (lambda (base exp) (funcall exp base)))
           ;; Build numerals
           (c1 (funcall succ c0))
           (c2 (funcall succ c1))
           (c3 (funcall succ c2))
           (c4 (funcall succ c3))
           (c5 (funcall succ c4)))
  (list
   ;; Addition
   (funcall to-int (funcall add c0 c0))    ;; 0+0 = 0
   (funcall to-int (funcall add c0 c3))    ;; 0+3 = 3
   (funcall to-int (funcall add c2 c3))    ;; 2+3 = 5
   (funcall to-int (funcall add c5 c5))    ;; 5+5 = 10
   ;; Commutativity: a+b = b+a
   (= (funcall to-int (funcall add c2 c3))
      (funcall to-int (funcall add c3 c2)))
   ;; Multiplication
   (funcall to-int (funcall mul c0 c5))    ;; 0*5 = 0
   (funcall to-int (funcall mul c1 c5))    ;; 1*5 = 5
   (funcall to-int (funcall mul c2 c3))    ;; 2*3 = 6
   (funcall to-int (funcall mul c3 c4))    ;; 3*4 = 12
   (funcall to-int (funcall mul c5 c5))    ;; 5*5 = 25
   ;; Commutativity: a*b = b*a
   (= (funcall to-int (funcall mul c3 c4))
      (funcall to-int (funcall mul c4 c3)))
   ;; Distributivity: a*(b+c) = a*b + a*c
   (= (funcall to-int (funcall mul c2 (funcall add c3 c4)))
      (funcall to-int (funcall add (funcall mul c2 c3) (funcall mul c2 c4))))
   ;; Power
   (funcall to-int (funcall power c2 c3))  ;; 2^3 = 8
   (funcall to-int (funcall power c3 c2))  ;; 3^2 = 9
   ;; Combined: (2+3) * 2 = 10
   (funcall to-int (funcall mul (funcall add c2 c3) c2))
   ;; (2*3) + (4*1) = 10
   (funcall to-int (funcall add (funcall mul c2 c3) (funcall mul c4 c1)))))"#;
    let expect = expect_test::expect![[r#""OK (0 3 5 10 t 0 5 6 12 25 t t 8 9 10 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church booleans: true, false, if, and, or, not
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_church_encoding_booleans() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* (;; Church true: select first
           (ctrue  (lambda (a) (lambda (b) a)))
           ;; Church false: select second
           (cfalse (lambda (a) (lambda (b) b)))
           ;; Church if: just apply the boolean
           (cif (lambda (cond then else) (funcall (funcall cond then) else)))
           ;; Church and
           (cand (lambda (p q) (funcall (funcall p q) p)))
           ;; Church or
           (cor  (lambda (p q) (funcall (funcall p p) q)))
           ;; Church not
           (cnot (lambda (p) (funcall (funcall p cfalse) ctrue)))
           ;; Convert to Elisp boolean
           (to-bool (lambda (cb) (funcall (funcall cb t) nil))))
  (list
   ;; Basic
   (funcall to-bool ctrue)
   (funcall to-bool cfalse)
   ;; If-then-else
   (funcall cif ctrue 'yes 'no)
   (funcall cif cfalse 'yes 'no)
   ;; AND truth table
   (funcall to-bool (funcall cand ctrue ctrue))
   (funcall to-bool (funcall cand ctrue cfalse))
   (funcall to-bool (funcall cand cfalse ctrue))
   (funcall to-bool (funcall cand cfalse cfalse))
   ;; OR truth table
   (funcall to-bool (funcall cor ctrue ctrue))
   (funcall to-bool (funcall cor ctrue cfalse))
   (funcall to-bool (funcall cor cfalse ctrue))
   (funcall to-bool (funcall cor cfalse cfalse))
   ;; NOT
   (funcall to-bool (funcall cnot ctrue))
   (funcall to-bool (funcall cnot cfalse))
   ;; Double negation
   (funcall to-bool (funcall cnot (funcall cnot ctrue)))
   (funcall to-bool (funcall cnot (funcall cnot cfalse)))
   ;; De Morgan: NOT(AND(p,q)) = OR(NOT(p), NOT(q))
   (equal (funcall to-bool (funcall cnot (funcall cand ctrue cfalse)))
          (funcall to-bool (funcall cor (funcall cnot ctrue) (funcall cnot cfalse))))
   (equal (funcall to-bool (funcall cnot (funcall cand cfalse cfalse)))
          (funcall to-bool (funcall cor (funcall cnot cfalse) (funcall cnot cfalse))))
   ;; Nested: if (and true (not false)) then 1 else 0
   (funcall cif (funcall cand ctrue (funcall cnot cfalse)) 1 0)
   ;; XOR via (or (and a (not b)) (and (not a) b))
   (let ((xor (lambda (a b)
                (funcall cor
                         (funcall cand a (funcall cnot b))
                         (funcall cand (funcall cnot a) b)))))
     (list (funcall to-bool (funcall xor ctrue ctrue))
           (funcall to-bool (funcall xor ctrue cfalse))
           (funcall to-bool (funcall xor cfalse ctrue))
           (funcall to-bool (funcall xor cfalse cfalse))))))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil yes no t nil nil nil t t t nil nil t t nil t t 1 (nil t t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church pairs: cons, car, cdr
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_church_encoding_pairs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((ctrue  (lambda (a) (lambda (b) a)))
           (cfalse (lambda (a) (lambda (b) b)))
           ;; Church pair: store two values, select with a boolean
           (cpair (lambda (a b) (lambda (sel) (funcall (funcall sel a) b))))
           ;; Church car: select first
           (ccar (lambda (p) (funcall p ctrue)))
           ;; Church cdr: select second
           (ccdr (lambda (p) (funcall p cfalse)))
           ;; Nested pair: triple
           (ctriple (lambda (a b c)
                      (funcall cpair a (funcall cpair b c))))
           (cfirst  (lambda (t) (funcall ccar t)))
           (csecond (lambda (t) (funcall ccar (funcall ccdr t))))
           (cthird  (lambda (t) (funcall ccdr (funcall ccdr t)))))
  (let* ((p1 (funcall cpair 10 20))
         (p2 (funcall cpair 'hello 'world))
         ;; Nested pairs: ((1,2), (3,4))
         (nested (funcall cpair (funcall cpair 1 2) (funcall cpair 3 4)))
         ;; Triple: (a, b, c)
         (tri (funcall ctriple 'x 'y 'z)))
    (list
     ;; Basic pair operations
     (funcall ccar p1)
     (funcall ccdr p1)
     (funcall ccar p2)
     (funcall ccdr p2)
     ;; Nested pair access
     (funcall ccar (funcall ccar nested))   ;; 1
     (funcall ccdr (funcall ccar nested))   ;; 2
     (funcall ccar (funcall ccdr nested))   ;; 3
     (funcall ccdr (funcall ccdr nested))   ;; 4
     ;; Triple access
     (funcall cfirst tri)
     (funcall csecond tri)
     (funcall cthird tri)
     ;; Swap pair: (a,b) -> (b,a)
     (let* ((swap (lambda (p) (funcall cpair (funcall ccdr p) (funcall ccar p))))
            (swapped (funcall swap p1)))
       (list (funcall ccar swapped) (funcall ccdr swapped)))
     ;; Pair equality check
     (and (= (funcall ccar p1) 10) (= (funcall ccdr p1) 20)))))"#;
    let expect = expect_test::expect![[r#""OK (10 20 hello world 1 2 3 4 x y z (20 10) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church predecessor (Kleene's method) -- the hard one
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_church_encoding_predecessor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Predecessor is the famously tricky Church encoding operation.
    // Uses the pair trick: start with (0,0), apply n times: (snd, succ(snd)),
    // then take fst. This gives pred(n) = n-1, with pred(0) = 0.
    let form = r#"(let* ((c0 (lambda (f) (lambda (x) x)))
           (succ (lambda (n) (lambda (f) (lambda (x) (funcall f (funcall (funcall n f) x))))))
           (to-int (lambda (n) (funcall (funcall n #'1+) 0)))
           (ctrue  (lambda (a) (lambda (b) a)))
           (cfalse (lambda (a) (lambda (b) b)))
           (cpair (lambda (a b) (lambda (sel) (funcall (funcall sel a) b))))
           (ccar (lambda (p) (funcall p ctrue)))
           (ccdr (lambda (p) (funcall p cfalse)))
           ;; PREDECESSOR: the hard one!
           ;; pred(n) = fst(n (lambda p. (snd(p), succ(snd(p)))) (0, 0))
           (pred (lambda (n)
                   (funcall ccar
                            (funcall (funcall n
                                              (lambda (p)
                                                (funcall cpair
                                                         (funcall ccdr p)
                                                         (funcall succ (funcall ccdr p)))))
                                     (funcall cpair c0 c0)))))
           ;; Subtraction: sub(m,n) = apply pred n times to m
           (sub (lambda (m n) (funcall (funcall n pred) m)))
           ;; Build numerals
           (c1 (funcall succ c0))
           (c2 (funcall succ c1))
           (c3 (funcall succ c2))
           (c4 (funcall succ c3))
           (c5 (funcall succ c4))
           (c6 (funcall succ c5))
           (c7 (funcall succ c6))
           (c8 (funcall succ c7)))
  (list
   ;; pred(0) = 0
   (funcall to-int (funcall pred c0))
   ;; pred(1) = 0
   (funcall to-int (funcall pred c1))
   ;; pred(2) = 1
   (funcall to-int (funcall pred c2))
   ;; pred(5) = 4
   (funcall to-int (funcall pred c5))
   ;; pred(8) = 7
   (funcall to-int (funcall pred c8))
   ;; succ(pred(n)) = n for n > 0
   (funcall to-int (funcall succ (funcall pred c3)))
   (funcall to-int (funcall succ (funcall pred c7)))
   ;; pred(succ(n)) = n for all n
   (funcall to-int (funcall pred (funcall succ c0)))
   (funcall to-int (funcall pred (funcall succ c5)))
   ;; Subtraction
   (funcall to-int (funcall sub c5 c2))    ;; 5-2 = 3
   (funcall to-int (funcall sub c8 c3))    ;; 8-3 = 5
   (funcall to-int (funcall sub c4 c4))    ;; 4-4 = 0
   ;; Underflow: sub(2,5) = 0 (clamped)
   (funcall to-int (funcall sub c2 c5))
   ;; Double pred
   (funcall to-int (funcall pred (funcall pred c5)))  ;; 3
   ;; pred applied 4 times to 4 = 0
   (funcall to-int (funcall pred (funcall pred (funcall pred (funcall pred c4)))))))"#;
    let expect = expect_test::expect![[r#""OK (0 0 1 4 7 3 7 0 5 3 5 0 0 3 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church list operations: nil, cons, head, tail, map, fold, length
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_church_encoding_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Church lists (Scott encoding): a list is a function that takes
    // two args (for cons and nil cases) and selects the appropriate one.
    let form = r#"(let* ((ctrue  (lambda (a) (lambda (b) a)))
           (cfalse (lambda (a) (lambda (b) b)))
           (to-bool (lambda (cb) (funcall (funcall cb t) nil)))
           ;; Church nil: select the nil-case
           (cnil (lambda (on-cons on-nil) (funcall on-nil)))
           ;; Church cons: store head and tail, select the cons-case
           (ccons (lambda (h t)
                    (lambda (on-cons on-nil)
                      (funcall on-cons h t))))
           ;; Church head
           (chead (lambda (lst)
                    (funcall lst
                             (lambda (h t) h)
                             (lambda () nil))))
           ;; Church tail
           (ctail (lambda (lst)
                    (funcall lst
                             (lambda (h t) t)
                             (lambda () cnil))))
           ;; Church is-nil
           (cnil-p (lambda (lst)
                     (funcall lst
                              (lambda (h t) cfalse)
                              (lambda () ctrue))))
           ;; to-list: convert Church list to Elisp list
           (to-list nil))
      ;; Define to-list recursively using fset
      (fset 'neovm--test-church-to-list
        (lambda (lst)
          (funcall lst
                   (lambda (h t)
                     (cons h (funcall 'neovm--test-church-to-list t)))
                   (lambda () nil))))
      (setq to-list (lambda (lst) (funcall 'neovm--test-church-to-list lst)))
      (unwind-protect
          (let* (;; Build list: [10, 20, 30, 40]
                 (l1 (funcall ccons 10
                              (funcall ccons 20
                                       (funcall ccons 30
                                                (funcall ccons 40 cnil)))))
                 ;; from-elisp-list
                 (from-list nil))
            (fset 'neovm--test-church-from-list
              (lambda (lst)
                (if (null lst) cnil
                  (funcall ccons (car lst)
                           (funcall 'neovm--test-church-from-list (cdr lst))))))
            (setq from-list (lambda (lst) (funcall 'neovm--test-church-from-list lst)))
            (unwind-protect
                (let* ((l2 (funcall from-list '(5 6 7)))
                       ;; map: apply function to each element
                       (cmap nil))
                  (fset 'neovm--test-church-map
                    (lambda (f lst)
                      (funcall lst
                               (lambda (h t)
                                 (funcall ccons (funcall f h)
                                          (funcall 'neovm--test-church-map f t)))
                               (lambda () cnil))))
                  (setq cmap (lambda (f lst) (funcall 'neovm--test-church-map f lst)))
                  ;; fold-right
                  (fset 'neovm--test-church-foldr
                    (lambda (f init lst)
                      (funcall lst
                               (lambda (h t)
                                 (funcall f h (funcall 'neovm--test-church-foldr f init t)))
                               (lambda () init))))
                  (unwind-protect
                      (let* ((cfoldr (lambda (f init lst) (funcall 'neovm--test-church-foldr f init lst)))
                             ;; length via fold
                             (clength (lambda (lst)
                                        (funcall cfoldr (lambda (h acc) (1+ acc)) 0 lst)))
                             ;; sum via fold
                             (csum (lambda (lst)
                                     (funcall cfoldr (lambda (h acc) (+ h acc)) 0 lst)))
                             ;; append two Church lists
                             (cappend (lambda (l1 l2)
                                        (funcall cfoldr (lambda (h acc) (funcall ccons h acc)) l2 l1))))
                        (list
                         ;; Basic operations
                         (funcall to-list l1)
                         (funcall chead l1)
                         (funcall to-list (funcall ctail l1))
                         ;; nil checks
                         (funcall to-bool (funcall cnil-p cnil))
                         (funcall to-bool (funcall cnil-p l1))
                         ;; Map: double each
                         (funcall to-list (funcall cmap (lambda (x) (* x 2)) l1))
                         ;; Length
                         (funcall clength l1)
                         (funcall clength cnil)
                         ;; Sum
                         (funcall csum l1)
                         ;; Append
                         (funcall to-list (funcall cappend l1 l2))
                         (funcall clength (funcall cappend l1 l2))
                         ;; Roundtrip
                         (funcall to-list (funcall from-list '(100 200 300)))
                         ;; Map then sum
                         (funcall csum (funcall cmap (lambda (x) (* x x)) l2))))
                    (fmakunbound 'neovm--test-church-map)
                    (fmakunbound 'neovm--test-church-foldr)))
              (fmakunbound 'neovm--test-church-from-list)))
        (fmakunbound 'neovm--test-church-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 20 30 40) 10 (20 30 40) t nil (20 40 60 80) 4 0 100 (10 20 30 40 5 6 7) 7 (100 200 300) 110)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: Church-encoded is-zero and comparison operators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_church_encoding_comparisons() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((c0 (lambda (f) (lambda (x) x)))
           (succ (lambda (n) (lambda (f) (lambda (x) (funcall f (funcall (funcall n f) x))))))
           (to-int (lambda (n) (funcall (funcall n #'1+) 0)))
           (ctrue  (lambda (a) (lambda (b) a)))
           (cfalse (lambda (a) (lambda (b) b)))
           (to-bool (lambda (cb) (funcall (funcall cb t) nil)))
           (cpair (lambda (a b) (lambda (sel) (funcall (funcall sel a) b))))
           (ccar (lambda (p) (funcall p ctrue)))
           (ccdr (lambda (p) (funcall p cfalse)))
           (pred (lambda (n)
                   (funcall ccar
                            (funcall (funcall n
                                              (lambda (p)
                                                (funcall cpair
                                                         (funcall ccdr p)
                                                         (funcall succ (funcall ccdr p)))))
                                     (funcall cpair c0 c0)))))
           (sub (lambda (m n) (funcall (funcall n pred) m)))
           ;; is-zero: true if n is zero
           (is-zero (lambda (n)
                      (funcall (funcall n (lambda (_) cfalse)) ctrue)))
           ;; LEQ: m <= n iff sub(m,n) = 0
           (leq (lambda (m n) (funcall is-zero (funcall sub m n))))
           ;; EQ: m = n iff m <= n and n <= m
           (ceq (lambda (m n)
                  (let ((cand (lambda (p q) (funcall (funcall p q) p))))
                    (funcall cand (funcall leq m n) (funcall leq n m)))))
           ;; LT: m < n iff m <= n and not(n <= m)
           (clt (lambda (m n)
                  (let ((cand (lambda (p q) (funcall (funcall p q) p)))
                        (cnot (lambda (p) (funcall (funcall p cfalse) ctrue))))
                    (funcall cand (funcall leq m n) (funcall cnot (funcall leq n m))))))
           ;; Build numerals
           (c1 (funcall succ c0))
           (c2 (funcall succ c1))
           (c3 (funcall succ c2))
           (c4 (funcall succ c3))
           (c5 (funcall succ c4)))
  (list
   ;; is-zero
   (funcall to-bool (funcall is-zero c0))
   (funcall to-bool (funcall is-zero c1))
   (funcall to-bool (funcall is-zero c5))
   ;; LEQ
   (funcall to-bool (funcall leq c0 c0))
   (funcall to-bool (funcall leq c0 c3))
   (funcall to-bool (funcall leq c3 c3))
   (funcall to-bool (funcall leq c3 c5))
   (funcall to-bool (funcall leq c5 c3))
   ;; EQ
   (funcall to-bool (funcall ceq c0 c0))
   (funcall to-bool (funcall ceq c3 c3))
   (funcall to-bool (funcall ceq c2 c5))
   (funcall to-bool (funcall ceq c5 c2))
   ;; LT
   (funcall to-bool (funcall clt c0 c1))
   (funcall to-bool (funcall clt c2 c5))
   (funcall to-bool (funcall clt c3 c3))
   (funcall to-bool (funcall clt c5 c2))
   ;; min/max via Church comparisons
   (let ((cmin (lambda (a b) (funcall (funcall (funcall leq a b) a) b)))
         (cmax (lambda (a b) (funcall (funcall (funcall leq a b) b) a))))
     (list
      (funcall to-int (funcall cmin c2 c5))
      (funcall to-int (funcall cmax c2 c5))
      (funcall to-int (funcall cmin c4 c1))
      (funcall to-int (funcall cmax c4 c1))))))"#;
    let expect =
        expect_test::expect![[r#""OK (t nil nil t t t t nil t t nil nil t t nil nil (2 5 1 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: Church-encoded integer-to-Church conversion with validation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_church_encoding_roundtrip_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that Church arithmetic identities hold for a range of values
    let form = r#"(let* ((c0 (lambda (f) (lambda (x) x)))
           (succ (lambda (n) (lambda (f) (lambda (x) (funcall f (funcall (funcall n f) x))))))
           (to-int (lambda (n) (funcall (funcall n #'1+) 0)))
           (add (lambda (m n) (lambda (f) (lambda (x)
                  (funcall (funcall m f) (funcall (funcall n f) x))))))
           (mul (lambda (m n) (lambda (f) (funcall m (funcall n f)))))
           ;; from-int via iterated succ
           (from-int (lambda (k)
                       (let ((r c0) (i 0))
                         (while (< i k)
                           (setq r (funcall succ r))
                           (setq i (1+ i)))
                         r))))
  ;; Verify identities for 0..5
  (let ((results nil)
        (k 0))
    (while (<= k 5)
      (let* ((ck (funcall from-int k))
             ;; Identity: k + 0 = k
             (sum-zero (funcall to-int (funcall add ck c0)))
             ;; Identity: k * 1 = k
             (c1 (funcall succ c0))
             (mul-one (funcall to-int (funcall mul ck c1)))
             ;; Identity: k * 0 = 0
             (mul-zero (funcall to-int (funcall mul ck c0)))
             ;; Identity: succ applied k times to 0 = k
             (roundtrip (funcall to-int ck)))
        (push (list k roundtrip sum-zero mul-one mul-zero
                    (= roundtrip k)
                    (= sum-zero k)
                    (= mul-one k)
                    (= mul-zero 0))
              results))
      (setq k (1+ k)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 0 0 0 0 t t t t) (1 1 1 1 0 t t t t) (2 2 2 2 0 t t t t) (3 3 3 3 0 t t t t) (4 4 4 4 0 t t t t) (5 5 5 5 0 t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
