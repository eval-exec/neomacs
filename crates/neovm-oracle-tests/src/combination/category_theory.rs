//! Oracle parity tests for basic category theory concepts in Elisp:
//! morphism composition, identity morphism, functor (map over structure),
//! natural transformation, Maybe monad (Just/Nothing) with bind/return,
//! List monad with bind/return, and monad laws verification.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Morphism composition and identity morphism
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_category_theory_morphisms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In category theory, morphisms are arrows between objects.
    // We represent them as functions (lambdas).
    // compose(f, g)(x) = f(g(x))
    // identity(x) = x
    // Laws: compose(f, id) = f, compose(id, f) = f, compose(f, compose(g, h)) = compose(compose(f, g), h)
    let form = r#"(let* ((compose (lambda (f g) (lambda (x) (funcall f (funcall g x)))))
           (id (lambda (x) x))
           ;; Some morphisms in the category of integers
           (double (lambda (x) (* x 2)))
           (inc (lambda (x) (+ x 1)))
           (square (lambda (x) (* x x)))
           (negate (lambda (x) (- x))))
  (list
    ;; Basic composition: (double . inc)(3) = double(inc(3)) = double(4) = 8
    (funcall (funcall compose double inc) 3)
    ;; (inc . double)(3) = inc(double(3)) = inc(6) = 7
    (funcall (funcall compose inc double) 3)
    ;; Triple composition: (square . double . inc)(2) = square(double(inc(2))) = square(6) = 36
    (funcall (funcall compose square (funcall compose double inc)) 2)

    ;; Identity laws
    ;; Right identity: compose(f, id) = f
    (let ((f-id (funcall compose double id)))
      (= (funcall f-id 5) (funcall double 5)))
    ;; Left identity: compose(id, f) = f
    (let ((id-f (funcall compose id double)))
      (= (funcall id-f 5) (funcall double 5)))
    ;; Identity on multiple values
    (mapcar id '(1 2 3 4 5))

    ;; Associativity: compose(f, compose(g, h)) = compose(compose(f, g), h)
    (let* ((f square)
           (g double)
           (h inc)
           (left (funcall compose f (funcall compose g h)))
           (right (funcall compose (funcall compose f g) h)))
      (list
        ;; Test on multiple inputs
        (= (funcall left 1) (funcall right 1))
        (= (funcall left 3) (funcall right 3))
        (= (funcall left 7) (funcall right 7))
        ;; Compute actual values
        (funcall left 2)    ;; square(double(inc(2))) = square(6) = 36
        (funcall right 2))) ;; (square.double)(inc(2)) = (square.double)(3) = square(6) = 36

    ;; Composition with negate
    (funcall (funcall compose negate square) 3)    ;; -(3^2) = -9
    (funcall (funcall compose square negate) 3)    ;; (-3)^2 = 9
    ;; Not commutative in general
    (not (= (funcall (funcall compose negate square) 3)
            (funcall (funcall compose square negate) 3)))))"#;
    let expect = expect_test::expect![[r#""OK (8 7 36 t t (1 2 3 4 5) (t t t 36 36) -9 9 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Functor: map over structure preserving composition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_category_theory_functor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A functor F maps objects and morphisms from one category to another.
    // For lists: fmap = mapcar
    // Functor laws:
    //   fmap(id) = id                    (preserves identity)
    //   fmap(f . g) = fmap(f) . fmap(g)  (preserves composition)
    let form = r#"(let* ((compose (lambda (f g) (lambda (x) (funcall f (funcall g x)))))
           (id (lambda (x) x))
           ;; List functor (fmap = mapcar)
           (list-fmap (lambda (f lst) (mapcar f lst)))
           ;; Maybe functor: (just . val) or nil
           (maybe-fmap (lambda (f m)
                         (if (null m) nil
                           (cons 'just (funcall f (cdr m))))))
           ;; Some morphisms
           (double (lambda (x) (* x 2)))
           (inc (lambda (x) (+ x 1)))
           (show (lambda (x) (format "%d" x))))
  (list
    ;; List functor: basic fmap
    (funcall list-fmap double '(1 2 3 4 5))
    (funcall list-fmap inc '(10 20 30))
    (funcall list-fmap show '(1 2 3))

    ;; List functor law 1: fmap(id) = id
    (equal (funcall list-fmap id '(1 2 3))
           (funcall id '(1 2 3)))

    ;; List functor law 2: fmap(f . g) = fmap(f) . fmap(g)
    (let ((input '(1 2 3 4 5)))
      (equal (funcall list-fmap (funcall compose double inc) input)
             (funcall list-fmap double (funcall list-fmap inc input))))

    ;; Maybe functor: basic fmap
    (funcall maybe-fmap double '(just . 5))    ;; (just . 10)
    (funcall maybe-fmap double nil)             ;; nil (Nothing stays Nothing)
    (funcall maybe-fmap inc '(just . 0))        ;; (just . 1)

    ;; Maybe functor law 1: fmap(id) = id
    (equal (funcall maybe-fmap id '(just . 42))
           (funcall id '(just . 42)))
    (equal (funcall maybe-fmap id nil)
           (funcall id nil))

    ;; Maybe functor law 2: fmap(f . g) = fmap(f) . fmap(g)
    (let ((val '(just . 3)))
      (equal (funcall maybe-fmap (funcall compose double inc) val)
             (funcall maybe-fmap double (funcall maybe-fmap inc val))))

    ;; Composition of functors: list of maybes
    (let ((data '((just . 1) nil (just . 3) (just . 4) nil)))
      (mapcar (lambda (m) (funcall maybe-fmap double m)) data))))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 4 6 8 10) (11 21 31) (\"1\" \"2\" \"3\") t t (just . 10) nil (just . 1) t t t ((just . 2) nil (just . 6) (just . 8) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Natural transformation: between functors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_category_theory_natural_transformation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A natural transformation eta: F -> G is a family of morphisms
    // such that for every morphism f: A -> B,
    //   G(f) . eta_A = eta_B . F(f)
    // (the naturality condition / commuting square)
    let form = r#"(let* (;; List functor
           (list-fmap (lambda (f lst) (mapcar f lst)))
           ;; Maybe functor
           (maybe-fmap (lambda (f m) (if (null m) nil (cons 'just (funcall f (cdr m))))))
           ;; Natural transformation: head-maybe (List -> Maybe)
           ;; Extracts first element of list as Maybe
           (head-maybe (lambda (lst)
                         (if (null lst) nil
                           (cons 'just (car lst)))))
           ;; Natural transformation: maybe-to-list (Maybe -> List)
           ;; Converts Maybe to a singleton or empty list
           (maybe-to-list (lambda (m)
                            (if (null m) nil
                              (list (cdr m)))))
           ;; Natural transformation: list-to-maybe (safe head)
           (safe-head (lambda (lst)
                        (if (null lst) nil
                          (cons 'just (car lst)))))
           ;; Morphisms to test naturality
           (double (lambda (x) (* x 2)))
           (inc (lambda (x) (+ x 1))))
  (list
    ;; head-maybe basic
    (funcall head-maybe '(10 20 30))   ;; (just . 10)
    (funcall head-maybe nil)            ;; nil

    ;; maybe-to-list basic
    (funcall maybe-to-list '(just . 42))  ;; (42)
    (funcall maybe-to-list nil)            ;; nil

    ;; Naturality of head-maybe: maybe-fmap(f) . head-maybe = head-maybe . list-fmap(f)
    ;; For f = double, input = (3 6 9)
    (let ((input '(3 6 9)))
      (equal (funcall maybe-fmap double (funcall head-maybe input))
             (funcall head-maybe (funcall list-fmap double input))))

    ;; Naturality with inc
    (let ((input '(5 10 15)))
      (equal (funcall maybe-fmap inc (funcall head-maybe input))
             (funcall head-maybe (funcall list-fmap inc input))))

    ;; Naturality on empty list
    (let ((input nil))
      (equal (funcall maybe-fmap double (funcall head-maybe input))
             (funcall head-maybe (funcall list-fmap double input))))

    ;; Naturality of maybe-to-list: list-fmap(f) . maybe-to-list = maybe-to-list . maybe-fmap(f)
    (let ((input '(just . 7)))
      (equal (funcall list-fmap double (funcall maybe-to-list input))
             (funcall maybe-to-list (funcall maybe-fmap double input))))

    ;; Naturality of maybe-to-list on Nothing
    (equal (funcall list-fmap double (funcall maybe-to-list nil))
           (funcall maybe-to-list (funcall maybe-fmap double nil)))

    ;; Composition of natural transformations:
    ;; list -> maybe -> list should give singleton-or-nil
    (let ((roundtrip (lambda (lst)
                       (funcall maybe-to-list (funcall head-maybe lst)))))
      (list
        (funcall roundtrip '(1 2 3))    ;; (1)
        (funcall roundtrip nil)          ;; nil
        (funcall roundtrip '(99))))))"#;
    let expect =
        expect_test::expect![[r#""OK ((just . 10) nil (42) nil t t t t t ((1) nil (99)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: Maybe monad (Just/Nothing) with bind/return and monad laws
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_category_theory_maybe_monad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Maybe monad: (just . val) for Just, nil for Nothing.
    // return(x) = (just . x)
    // bind(m, f) = if m is Nothing then Nothing else f(value(m))
    // Monad laws:
    //   1. Left identity:  bind(return(a), f) = f(a)
    //   2. Right identity:  bind(m, return) = m
    //   3. Associativity:   bind(bind(m, f), g) = bind(m, lambda(x, bind(f(x), g)))
    let form = r#"(let* ((mreturn (lambda (x) (cons 'just x)))
           (mbind (lambda (m f)
                    (if (null m) nil
                      (funcall f (cdr m)))))
           ;; Some monadic functions (a -> Maybe b)
           (safe-div (lambda (n)
                       (lambda (x)
                         (if (= n 0) nil
                           (cons 'just (/ x n))))))
           (safe-sqrt (lambda (x)
                        (if (< x 0) nil
                          (cons 'just (sqrt x)))))
           (safe-head (lambda (lst)
                        (if (null lst) nil
                          (cons 'just (car lst)))))
           (inc-maybe (lambda (x) (cons 'just (1+ x))))
           (double-maybe (lambda (x) (cons 'just (* x 2)))))

  (list
    ;; Basic bind
    (funcall mbind (funcall mreturn 10) (funcall safe-div 2))       ;; (just . 5)
    (funcall mbind (funcall mreturn 10) (funcall safe-div 0))       ;; nil
    (funcall mbind nil (funcall safe-div 2))                        ;; nil

    ;; Chained bind: 100 / 5 / 2 = 10
    (funcall mbind
      (funcall mbind (funcall mreturn 100) (funcall safe-div 5))
      (funcall safe-div 2))

    ;; Chained bind with failure: 100 / 5 / 0 = nil
    (funcall mbind
      (funcall mbind (funcall mreturn 100) (funcall safe-div 5))
      (funcall safe-div 0))

    ;; --- MONAD LAW 1: Left identity ---
    ;; bind(return(a), f) = f(a) for various f and a
    (equal (funcall mbind (funcall mreturn 10) inc-maybe)
           (funcall inc-maybe 10))
    (equal (funcall mbind (funcall mreturn 5) double-maybe)
           (funcall double-maybe 5))
    (equal (funcall mbind (funcall mreturn 0) (funcall safe-div 0))
           (funcall (funcall safe-div 0) 0))

    ;; --- MONAD LAW 2: Right identity ---
    ;; bind(m, return) = m
    (equal (funcall mbind (funcall mreturn 42) mreturn)
           (funcall mreturn 42))
    (equal (funcall mbind nil mreturn)
           nil)

    ;; --- MONAD LAW 3: Associativity ---
    ;; bind(bind(m, f), g) = bind(m, lambda(x, bind(f(x), g)))
    (let* ((m (funcall mreturn 100))
           (f (funcall safe-div 5))
           (g double-maybe)
           (left (funcall mbind (funcall mbind m f) g))
           (right (funcall mbind m (lambda (x) (funcall mbind (funcall f x) g)))))
      (equal left right))

    ;; Associativity with nil propagation
    (let* ((m (funcall mreturn 100))
           (f (funcall safe-div 0))
           (g double-maybe)
           (left (funcall mbind (funcall mbind m f) g))
           (right (funcall mbind m (lambda (x) (funcall mbind (funcall f x) g)))))
      (equal left right))

    ;; Practical: safe nested data access
    (let ((data '((users . ((alice . ((age . 30)))
                             (bob . nil))))))
      (list
        ;; Successful lookup
        (funcall mbind (funcall safe-head data)
          (lambda (pair)
            (funcall mbind (funcall safe-head (cdr pair))
              (lambda (user-pair)
                (funcall safe-head (cdr user-pair))))))
        ;; Access pattern for 'bob' who has nil data
        (funcall mbind (cons 'just (cdr (assq 'bob (cdr (assq 'users data)))))
          (lambda (val) (cons 'just val)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((just . 5) nil nil (just . 10) nil t t t t t t t ((just age . 30) (just)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: List monad with bind/return
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_category_theory_list_monad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // List monad: return(x) = (list x), bind = flatmap
    // Represents non-deterministic computation.
    let form = r#"(let* ((mreturn (lambda (x) (list x)))
           (mbind (lambda (lst f)
                    (let ((result nil))
                      (dolist (x lst)
                        (setq result (append result (funcall f x))))
                      result)))
           ;; Monadic functions
           (expand (lambda (x) (list x (- x) (* x x))))
           (replicate (lambda (x) (make-list (abs x) x)))
           (inc-list (lambda (x) (list (1+ x))))
           (double-list (lambda (x) (list (* x 2)))))

  (list
    ;; Basic bind: each element generates multiple results
    (funcall mbind '(1 2 3) expand)
    ;; (1 -1 1 2 -2 4 3 -3 9)

    ;; Nested bind: cartesian product
    (funcall mbind '(1 2 3)
      (lambda (x)
        (funcall mbind '(a b)
          (lambda (y)
            (funcall mreturn (list x y))))))

    ;; Guard pattern: filter via empty list
    (funcall mbind '(1 2 3 4 5 6 7 8 9 10)
      (lambda (x)
        (if (= (% x 3) 0)
            (list x)
          nil)))

    ;; Replicate: each number N generates N copies of itself
    (funcall mbind '(1 2 3) replicate)
    ;; (1 2 2 3 3 3)

    ;; --- LIST MONAD LAW 1: Left identity ---
    ;; bind(return(a), f) = f(a)
    (equal (funcall mbind (funcall mreturn 3) expand)
           (funcall expand 3))

    ;; --- LIST MONAD LAW 2: Right identity ---
    ;; bind(m, return) = m
    (equal (funcall mbind '(1 2 3) mreturn)
           '(1 2 3))
    (equal (funcall mbind nil mreturn)
           nil)

    ;; --- LIST MONAD LAW 3: Associativity ---
    ;; bind(bind(m, f), g) = bind(m, lambda(x, bind(f(x), g)))
    (let* ((m '(1 2 3))
           (f inc-list)
           (g double-list)
           (left (funcall mbind (funcall mbind m f) g))
           (right (funcall mbind m (lambda (x) (funcall mbind (funcall f x) g)))))
      (equal left right))

    ;; Associativity with expand and replicate
    (let* ((m '(2 3))
           (f expand)
           (g (lambda (x) (if (> x 0) (list x) nil)))
           (left (funcall mbind (funcall mbind m f) g))
           (right (funcall mbind m (lambda (x) (funcall mbind (funcall f x) g)))))
      (equal left right))

    ;; Practical: Pythagorean triples up to 10
    (funcall mbind (number-sequence 1 10)
      (lambda (a)
        (funcall mbind (number-sequence a 10)
          (lambda (b)
            (funcall mbind (number-sequence b 10)
              (lambda (c)
                (if (= (+ (* a a) (* b b)) (* c c))
                    (list (list a b c))
                  nil)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 -1 1 2 -2 4 3 -3 9) ((1 a) (1 b) (2 a) (2 b) (3 a) (3 b)) (3 6 9) (1 2 2 3 3 3) t t t t t ((3 4 5) (6 8 10)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: monad laws verification across multiple monads
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_category_theory_monad_laws_verification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Systematically verify all three monad laws for both Maybe and List monads
    // across multiple test values and functions.
    let form = r#"(let* (;; Maybe monad operations
           (maybe-return (lambda (x) (cons 'just x)))
           (maybe-bind (lambda (m f)
                         (if (null m) nil
                           (funcall f (cdr m)))))
           ;; List monad operations
           (list-return (lambda (x) (list x)))
           (list-bind (lambda (lst f)
                        (let ((result nil))
                          (dolist (x lst)
                            (setq result (append result (funcall f x))))
                          result)))
           ;; Test functions for Maybe: a -> Maybe b
           (maybe-f (lambda (x) (if (> x 0) (cons 'just (* x 2)) nil)))
           (maybe-g (lambda (x) (if (< x 100) (cons 'just (+ x 1)) nil)))
           ;; Test functions for List: a -> List b
           (list-f (lambda (x) (list x (+ x 10))))
           (list-g (lambda (x) (list (* x 2))))
           ;; Generic monad law checker
           ;; Returns (law1-ok law2-ok law3-ok) for given return, bind, f, g, and test values
           (check-laws
             (lambda (ret bnd f g values)
               (let ((law1-results nil)
                     (law2-results nil)
                     (law3-results nil))
                 (dolist (a values)
                   ;; Law 1: bind(return(a), f) = f(a)
                   (push (equal (funcall bnd (funcall ret a) f)
                                (funcall f a))
                         law1-results)
                   ;; Law 2: bind(return(a), return) = return(a)
                   ;; Actually: bind(m, return) = m, so m = return(a)
                   (push (equal (funcall bnd (funcall ret a) ret)
                                (funcall ret a))
                         law2-results)
                   ;; Law 3: bind(bind(return(a), f), g) = bind(return(a), lambda(x, bind(f(x), g)))
                   (let* ((m (funcall ret a))
                          (left (funcall bnd (funcall bnd m f) g))
                          (right (funcall bnd m (lambda (x) (funcall bnd (funcall f x) g)))))
                     (push (equal left right) law3-results)))
                 (list
                   (cons 'law1 (cl-every #'identity (nreverse law1-results)))
                   (cons 'law2 (cl-every #'identity (nreverse law2-results)))
                   (cons 'law3 (cl-every #'identity (nreverse law3-results))))))))
  (list
    ;; Verify Maybe monad laws for values: -5, 0, 1, 10, 50, 200
    (funcall check-laws maybe-return maybe-bind maybe-f maybe-g
             '(-5 0 1 10 50 200))
    ;; Verify List monad laws for values: 1, 2, 3, 5
    (funcall check-laws list-return list-bind list-f list-g
             '(1 2 3 5))
    ;; Additional: bind with nil (Nothing) always gives nil
    (list
      (funcall maybe-bind nil maybe-f)
      (funcall maybe-bind nil maybe-g)
      (funcall maybe-bind nil maybe-return))
    ;; Additional: bind with empty list always gives empty list
    (list
      (funcall list-bind nil list-f)
      (funcall list-bind nil list-g)
      (funcall list-bind nil list-return))
    ;; Cross-check: Maybe and List agree on singleton inputs
    ;; return(5) via list = (5), bind((5), f) should act like f(5)
    (let ((val 5))
      (list
        (equal (funcall list-bind (funcall list-return val) list-f)
               (funcall list-f val))
        (equal (funcall maybe-bind (funcall maybe-return val) maybe-f)
               (funcall maybe-f val))))))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Endofunctor composition and Kleisli arrows
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_category_theory_kleisli() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Kleisli composition: given monadic functions f: A -> M B and g: B -> M C,
    // the Kleisli composition (f >=> g)(x) = bind(f(x), g)
    // This forms a category (the Kleisli category) with return as identity.
    let form = r#"(let* (;; Maybe monad
           (mreturn (lambda (x) (cons 'just x)))
           (mbind (lambda (m f) (if (null m) nil (funcall f (cdr m)))))
           ;; Kleisli composition for Maybe
           (kleisli (lambda (f g)
                      (lambda (x) (funcall mbind (funcall f x) g))))
           ;; Monadic functions
           (parse-int (lambda (s)
                        (let ((n (string-to-number s)))
                          (if (and (= n 0) (not (string= s "0")))
                              nil
                            (cons 'just n)))))
           (check-pos (lambda (n)
                        (if (> n 0) (cons 'just n) nil)))
           (halve (lambda (n)
                    (if (= (% n 2) 0)
                        (cons 'just (/ n 2))
                      nil)))
           (show-result (lambda (n)
                          (cons 'just (format "result=%d" n)))))

  (list
    ;; Basic Kleisli composition
    (funcall (funcall kleisli parse-int check-pos) "42")   ;; (just . 42)
    (funcall (funcall kleisli parse-int check-pos) "-5")   ;; nil
    (funcall (funcall kleisli parse-int check-pos) "abc")  ;; nil

    ;; Triple Kleisli composition: parse -> check-pos -> halve
    (let ((pipeline (funcall kleisli
                      (funcall kleisli parse-int check-pos)
                      halve)))
      (list
        (funcall pipeline "100")    ;; (just . 50)
        (funcall pipeline "7")      ;; nil (7 is odd)
        (funcall pipeline "-4")     ;; nil (negative)
        (funcall pipeline "0")))    ;; nil (not positive)

    ;; Full pipeline: parse -> check-pos -> halve -> show
    (let ((full (funcall kleisli
                  (funcall kleisli
                    (funcall kleisli parse-int check-pos)
                    halve)
                  show-result)))
      (list
        (funcall full "100")        ;; (just . "result=50")
        (funcall full "42")         ;; nil (42/2=21, but 42 is even so (just . 21))
        (funcall full "20")))       ;; (just . "result=10")

    ;; Kleisli category laws:
    ;; 1. Left identity: kleisli(return, f) = f
    (equal (funcall (funcall kleisli mreturn check-pos) 5)
           (funcall check-pos 5))
    ;; 2. Right identity: kleisli(f, return) = f
    (equal (funcall (funcall kleisli check-pos mreturn) 5)
           (funcall check-pos 5))
    ;; 3. Associativity: kleisli(kleisli(f, g), h) = kleisli(f, kleisli(g, h))
    (let* ((f parse-int) (g check-pos) (h halve)
           (left (funcall kleisli (funcall kleisli f g) h))
           (right (funcall kleisli f (funcall kleisli g h))))
      (list
        (equal (funcall left "100") (funcall right "100"))
        (equal (funcall left "-5") (funcall right "-5"))
        (equal (funcall left "abc") (funcall right "abc"))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((just . 42) nil nil ((just . 50) nil nil nil) ((just . \"result=50\") (just . \"result=21\") (just . \"result=10\")) t t (t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
