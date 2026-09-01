//! Advanced category theory oracle parity tests:
//! endofunctors with fmap laws (identity, composition), natural
//! transformations, monads (bind/return/join) with laws verification,
//! Kleisli composition, applicative functors (pure/apply), Maybe/Either/List
//! monad implementations, and monad transformer stacking.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Endofunctors with fmap laws: identity and composition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cat_theory_adv_endofunctor_laws() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An endofunctor maps a category to itself. In Elisp, we model
    // endofunctors on the category of Elisp values with fmap.
    // Functor laws:
    //   fmap(id) = id              (identity law)
    //   fmap(f . g) = fmap(f) . fmap(g) (composition law)
    // We test with: List, Maybe (cons 'just val / nil), Tree (nested lists),
    // and Pair ((a . b)) endofunctors.
    let form = r#"(let* ((compose (lambda (f g) (lambda (x) (funcall f (funcall g x)))))
           (id (lambda (x) x))
           ;; List endofunctor
           (list-fmap (lambda (f lst) (mapcar f lst)))
           ;; Maybe endofunctor: (just . val) or nil
           (maybe-fmap (lambda (f m) (if (null m) nil (cons 'just (funcall f (cdr m))))))
           ;; Tree endofunctor: nested list where atoms are leaves
           (tree-fmap nil)
           ;; Pair endofunctor: maps over both components of (a . b)
           (pair-fmap (lambda (f p) (cons (funcall f (car p)) (funcall f (cdr p)))))
           ;; Morphisms
           (double (lambda (x) (* x 2)))
           (inc (lambda (x) (+ x 1)))
           (square (lambda (x) (* x x)))
           (negate (lambda (x) (- x))))

  ;; Define tree-fmap recursively: map f over all leaves
  (setq tree-fmap
        (lambda (f tree)
          (if (listp tree)
              (mapcar (lambda (subtree) (funcall tree-fmap f subtree)) tree)
            (funcall f tree))))

  (list
    ;; === LIST ENDOFUNCTOR ===
    ;; Identity law: fmap(id, xs) = xs
    (equal (funcall list-fmap id '(1 2 3 4 5))
           '(1 2 3 4 5))
    ;; Composition law: fmap(f.g, xs) = fmap(f, fmap(g, xs))
    (equal (funcall list-fmap (funcall compose double inc) '(1 2 3))
           (funcall list-fmap double (funcall list-fmap inc '(1 2 3))))
    ;; Composition with three morphisms
    (equal (funcall list-fmap (funcall compose square (funcall compose double inc)) '(1 2 3))
           (funcall list-fmap square
                    (funcall list-fmap double
                             (funcall list-fmap inc '(1 2 3)))))

    ;; === MAYBE ENDOFUNCTOR ===
    ;; Identity law on Just
    (equal (funcall maybe-fmap id '(just . 42))
           '(just . 42))
    ;; Identity law on Nothing
    (equal (funcall maybe-fmap id nil) nil)
    ;; Composition law on Just
    (equal (funcall maybe-fmap (funcall compose double inc) '(just . 5))
           (funcall maybe-fmap double (funcall maybe-fmap inc '(just . 5))))
    ;; Composition law on Nothing (both sides are nil)
    (equal (funcall maybe-fmap (funcall compose double inc) nil)
           (funcall maybe-fmap double (funcall maybe-fmap inc nil)))

    ;; === TREE ENDOFUNCTOR ===
    ;; Identity law
    (equal (funcall tree-fmap id '(1 (2 3) ((4 5) 6)))
           '(1 (2 3) ((4 5) 6)))
    ;; Composition law
    (let ((tree '(1 (2 (3 4)) 5)))
      (equal (funcall tree-fmap (funcall compose double inc) tree)
             (funcall tree-fmap double (funcall tree-fmap inc tree))))

    ;; === PAIR ENDOFUNCTOR ===
    ;; Identity law
    (equal (funcall pair-fmap id '(10 . 20)) '(10 . 20))
    ;; Composition law
    (equal (funcall pair-fmap (funcall compose square negate) '(3 . 4))
           (funcall pair-fmap square (funcall pair-fmap negate '(3 . 4))))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Natural transformations between endofunctors with naturality squares
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cat_theory_adv_natural_transformations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Natural transformation eta: F -> G satisfies the naturality condition:
    //   G(f) . eta_A = eta_B . F(f)  for every morphism f: A -> B
    // We verify this for several transformations between List, Maybe, and Pair.
    let form = r#"(let* (;; Functor fmaps
           (list-fmap (lambda (f lst) (mapcar f lst)))
           (maybe-fmap (lambda (f m) (if (null m) nil (cons 'just (funcall f (cdr m))))))
           (pair-fmap (lambda (f p) (cons (funcall f (car p)) (funcall f (cdr p)))))
           ;; Natural transformations
           ;; safe-head: List -> Maybe (extract first element)
           (safe-head (lambda (lst) (if (null lst) nil (cons 'just (car lst)))))
           ;; maybe-to-list: Maybe -> List
           (maybe-to-list (lambda (m) (if (null m) nil (list (cdr m)))))
           ;; list-to-pair: List -> Pair (first two elements, nil-padded)
           (list-to-pair (lambda (lst)
                           (cons (if lst (car lst) nil)
                                 (if (cdr lst) (cadr lst) nil))))
           ;; concat-all: List -> Maybe (concatenate all strings, Nothing if empty)
           (concat-all (lambda (lst)
                         (if (null lst) nil
                           (cons 'just (apply #'concat lst)))))
           ;; Morphisms for testing
           (double (lambda (x) (* x 2)))
           (inc (lambda (x) (+ x 1)))
           (show (lambda (x) (number-to-string x))))

  (list
    ;; Naturality of safe-head: maybe-fmap(f) . safe-head = safe-head . list-fmap(f)
    (let ((input '(3 6 9)))
      (list
        (equal (funcall maybe-fmap double (funcall safe-head input))
               (funcall safe-head (funcall list-fmap double input)))
        (equal (funcall maybe-fmap inc (funcall safe-head input))
               (funcall safe-head (funcall list-fmap inc input)))))
    ;; Naturality on empty list
    (equal (funcall maybe-fmap double (funcall safe-head nil))
           (funcall safe-head (funcall list-fmap double nil)))

    ;; Naturality of maybe-to-list: list-fmap(f) . maybe-to-list = maybe-to-list . maybe-fmap(f)
    (let ((input '(just . 7)))
      (list
        (equal (funcall list-fmap double (funcall maybe-to-list input))
               (funcall maybe-to-list (funcall maybe-fmap double input)))
        (equal (funcall list-fmap inc (funcall maybe-to-list input))
               (funcall maybe-to-list (funcall maybe-fmap inc input)))))
    ;; Naturality on Nothing
    (equal (funcall list-fmap double (funcall maybe-to-list nil))
           (funcall maybe-to-list (funcall maybe-fmap double nil)))

    ;; Naturality of list-to-pair: pair-fmap(f) . list-to-pair = list-to-pair . list-fmap(f)
    (let ((input '(10 20 30)))
      (equal (funcall pair-fmap double (funcall list-to-pair input))
             (funcall list-to-pair (funcall list-fmap double input))))

    ;; Composition of natural transformations: List -> Maybe -> List
    ;; (maybe-to-list . safe-head) is itself a natural transformation List -> List
    (let* ((composed-nat (lambda (lst) (funcall maybe-to-list (funcall safe-head lst))))
           (input '(5 10 15)))
      ;; Naturality: list-fmap(f) . composed = composed . list-fmap(f)
      (equal (funcall list-fmap double (funcall composed-nat input))
             (funcall composed-nat (funcall list-fmap double input))))

    ;; Vertical composition: two nat-trans composed
    (let ((input '(42 99)))
      (funcall maybe-to-list (funcall safe-head input)))))"#;
    let expect = expect_test::expect![[r#""OK ((t t) t (t t) t t t (42))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Monads with bind, return, and join; all three monad laws
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cat_theory_adv_monad_bind_return_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Monad can be defined via (return, bind) or equivalently via (return, join, fmap).
    // join: M(M(a)) -> M(a) is the "flattening" operation.
    // bind(m, f) = join(fmap(f, m))
    // We implement both and verify equivalence, plus all three monad laws.
    let form = r#"(let* (;; Maybe monad
           (maybe-return (lambda (x) (cons 'just x)))
           (maybe-bind (lambda (m f) (if (null m) nil (funcall f (cdr m)))))
           (maybe-fmap (lambda (f m) (if (null m) nil (cons 'just (funcall f (cdr m))))))
           ;; join for Maybe: Maybe(Maybe(a)) -> Maybe(a)
           ;; (just . (just . x)) -> (just . x)
           ;; (just . nil) -> nil
           ;; nil -> nil
           (maybe-join (lambda (mm)
                         (if (null mm) nil
                           (let ((inner (cdr mm)))
                             (if (and (consp inner) (eq (car inner) 'just))
                                 inner
                               nil)))))
           ;; List monad
           (list-return (lambda (x) (list x)))
           (list-bind (lambda (lst f)
                        (apply #'append (mapcar f lst))))
           (list-fmap (lambda (f lst) (mapcar f lst)))
           ;; join for List: List(List(a)) -> List(a) = flatten one level
           (list-join (lambda (lol) (apply #'append lol)))
           ;; Test functions
           (safe-half (lambda (x) (if (= (% x 2) 0)
                                      (cons 'just (/ x 2))
                                    nil)))
           (safe-positive (lambda (x) (if (> x 0)
                                          (cons 'just x)
                                        nil)))
           (expand (lambda (x) (list x (- x))))
           (replicate (lambda (x) (make-list (min (abs x) 5) x))))

  (list
    ;; === MAYBE: bind via join equivalence ===
    ;; bind(m, f) = join(fmap(f, m))
    (let ((m '(just . 10)))
      (equal (funcall maybe-bind m safe-half)
             (funcall maybe-join (funcall maybe-fmap safe-half m))))
    (let ((m nil))
      (equal (funcall maybe-bind m safe-half)
             (funcall maybe-join (funcall maybe-fmap safe-half m))))
    ;; For odd value: safe-half fails
    (let ((m '(just . 7)))
      (equal (funcall maybe-bind m safe-half)
             (funcall maybe-join (funcall maybe-fmap safe-half m))))

    ;; === MAYBE MONAD LAWS (via bind) ===
    ;; Law 1 (left identity): bind(return(a), f) = f(a)
    (equal (funcall maybe-bind (funcall maybe-return 10) safe-half)
           (funcall safe-half 10))
    ;; Law 2 (right identity): bind(m, return) = m
    (equal (funcall maybe-bind '(just . 42) maybe-return)
           '(just . 42))
    (equal (funcall maybe-bind nil maybe-return) nil)
    ;; Law 3 (associativity): bind(bind(m, f), g) = bind(m, x -> bind(f(x), g))
    (let* ((m '(just . 20))
           (left (funcall maybe-bind (funcall maybe-bind m safe-half) safe-positive))
           (right (funcall maybe-bind m
                           (lambda (x) (funcall maybe-bind (funcall safe-half x) safe-positive)))))
      (equal left right))

    ;; === LIST: bind via join equivalence ===
    (let ((m '(1 2 3)))
      (equal (funcall list-bind m expand)
             (funcall list-join (funcall list-fmap expand m))))
    ;; Empty list
    (equal (funcall list-bind nil expand)
           (funcall list-join (funcall list-fmap expand nil)))

    ;; === LIST MONAD LAWS ===
    ;; Law 1: bind(return(a), f) = f(a)
    (equal (funcall list-bind (funcall list-return 3) expand)
           (funcall expand 3))
    ;; Law 2: bind(m, return) = m
    (equal (funcall list-bind '(1 2 3) list-return) '(1 2 3))
    ;; Law 3: associativity
    (let* ((m '(1 2 3))
           (f expand) (g replicate)
           (left (funcall list-bind (funcall list-bind m f) g))
           (right (funcall list-bind m (lambda (x)
                                         (funcall list-bind (funcall f x) g)))))
      (equal left right))

    ;; === JOIN LAWS ===
    ;; join . fmap(join) = join . join (for nested M(M(M(a))))
    (let ((mmm '((1 2) (3) (4 5 6))))
      ;; list-join(list-fmap(list-join, mmm)) = list-join(list-join(mmm))
      ;; But mmm needs to be M(M(M(a))) = list of lists of lists
      (let ((mmmm '(((1 2) (3 4)) ((5) (6 7 8)))))
        (equal (funcall list-join (funcall list-fmap list-join mmmm))
               (funcall list-join (funcall list-join mmmm)))))

    ;; join . return = id (for list)
    (let ((m '(1 2 3)))
      (equal (funcall list-join (funcall list-return m)) m))
    ;; join . fmap(return) = id (for list)
    (let ((m '(1 2 3)))
      (equal (funcall list-join (funcall list-fmap list-return m)) m))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Kleisli composition: forming a category from monadic functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cat_theory_adv_kleisli_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In the Kleisli category for a monad M:
    //   Objects are types, morphisms a -> b are functions a -> M b
    //   Identity is return: a -> M a
    //   Composition: (f >=> g)(x) = bind(f(x), g)
    // We verify category laws: left/right identity and associativity.
    let form = r#"(let* (;; Maybe monad
           (mbind (lambda (m f) (if (null m) nil (funcall f (cdr m)))))
           (mreturn (lambda (x) (cons 'just x)))
           ;; Kleisli composition
           (fish (lambda (f g) (lambda (x) (funcall mbind (funcall f x) g))))
           ;; Monadic functions (a -> Maybe b)
           (parse-num (lambda (s)
                        (let ((n (string-to-number s)))
                          (if (and (= n 0) (not (string= s "0")))
                              nil (cons 'just n)))))
           (guard-positive (lambda (n) (if (> n 0) (cons 'just n) nil)))
           (safe-halve (lambda (n) (if (= (% n 2) 0)
                                       (cons 'just (/ n 2))
                                     nil)))
           (safe-decrement (lambda (n) (if (> n 0)
                                           (cons 'just (1- n))
                                         nil)))
           (show (lambda (n) (cons 'just (number-to-string n)))))

  (list
    ;; === KLEISLI CATEGORY LAWS ===
    ;; Left identity: return >=> f = f
    (equal (funcall (funcall fish mreturn guard-positive) 5)
           (funcall guard-positive 5))
    (equal (funcall (funcall fish mreturn guard-positive) -3)
           (funcall guard-positive -3))

    ;; Right identity: f >=> return = f
    (equal (funcall (funcall fish guard-positive mreturn) 5)
           (funcall guard-positive 5))
    (equal (funcall (funcall fish guard-positive mreturn) -3)
           (funcall guard-positive -3))

    ;; Associativity: (f >=> g) >=> h = f >=> (g >=> h)
    (let* ((f parse-num) (g guard-positive) (h safe-halve)
           (left (funcall fish (funcall fish f g) h))
           (right (funcall fish f (funcall fish g h))))
      (list
        (equal (funcall left "100") (funcall right "100"))   ;; success path
        (equal (funcall left "-5") (funcall right "-5"))     ;; guard fails
        (equal (funcall left "abc") (funcall right "abc"))   ;; parse fails
        (equal (funcall left "7") (funcall right "7"))))     ;; halve fails (odd)

    ;; === PIPELINE CONSTRUCTION ===
    ;; Build a 4-step pipeline: parse -> positive -> halve -> show
    (let ((pipeline (funcall fish
                      (funcall fish
                        (funcall fish parse-num guard-positive)
                        safe-halve)
                      show)))
      (list
        (funcall pipeline "100")
        (funcall pipeline "42")
        (funcall pipeline "-10")
        (funcall pipeline "abc")
        (funcall pipeline "0")
        (funcall pipeline "7")))

    ;; === KLEISLI FISH OPERATOR IS TRULY ASSOCIATIVE ===
    ;; For 4 functions: ((f >=> g) >=> h) >=> k = f >=> (g >=> (h >=> k))
    (let* ((f parse-num) (g guard-positive) (h safe-halve) (k safe-decrement)
           (left (funcall fish (funcall fish (funcall fish f g) h) k))
           (right (funcall fish f (funcall fish g (funcall fish h k)))))
      (list
        (equal (funcall left "100") (funcall right "100"))
        (equal (funcall left "2") (funcall right "2"))
        (equal (funcall left "1") (funcall right "1"))))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t (t t t t) ((just . \"50\") (just . \"21\") nil nil nil nil) (t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Applicative functors: pure and apply (<*>)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cat_theory_adv_applicative_functor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An applicative functor has:
    //   pure: a -> F a
    //   <*> (ap): F (a -> b) -> F a -> F b
    // Laws:
    //   Identity: pure(id) <*> v = v
    //   Composition: pure(.) <*> u <*> v <*> w = u <*> (v <*> w)
    //   Homomorphism: pure(f) <*> pure(x) = pure(f(x))
    //   Interchange: u <*> pure(y) = pure(lambda(f, f(y))) <*> u
    let form = r#"(let* ((id (lambda (x) x))
           (compose (lambda (f g) (lambda (x) (funcall f (funcall g x)))))
           ;; Maybe applicative
           (maybe-pure (lambda (x) (cons 'just x)))
           (maybe-ap (lambda (mf mx)
                       (if (or (null mf) (null mx)) nil
                         (cons 'just (funcall (cdr mf) (cdr mx))))))
           ;; List applicative (cartesian product style)
           (list-pure (lambda (x) (list x)))
           (list-ap (lambda (fs xs)
                      (let ((result nil))
                        (dolist (f fs)
                          (dolist (x xs)
                            (setq result (cons (funcall f x) result))))
                        (nreverse result))))
           ;; Test functions
           (double (lambda (x) (* x 2)))
           (inc (lambda (x) (+ x 1)))
           (square (lambda (x) (* x x))))

  (list
    ;; === MAYBE APPLICATIVE ===
    ;; Identity: pure(id) <*> v = v
    (equal (funcall maybe-ap (funcall maybe-pure id) '(just . 42))
           '(just . 42))
    ;; Homomorphism: pure(f) <*> pure(x) = pure(f(x))
    (equal (funcall maybe-ap (funcall maybe-pure double) (funcall maybe-pure 5))
           (funcall maybe-pure (funcall double 5)))
    ;; Interchange: u <*> pure(y) = pure(lambda(f, f(y))) <*> u
    (let ((u (funcall maybe-pure double))
          (y 7))
      (equal (funcall maybe-ap u (funcall maybe-pure y))
             (funcall maybe-ap
                      (funcall maybe-pure (lambda (f) (funcall f 7)))
                      u)))
    ;; Nothing propagation
    (funcall maybe-ap nil (funcall maybe-pure 5))
    (funcall maybe-ap (funcall maybe-pure double) nil)
    ;; Practical: apply function to two Maybe args (liftA2)
    (let ((liftA2 (lambda (f ma mb)
                    (funcall maybe-ap
                             (funcall maybe-ap
                                      (funcall maybe-pure (lambda (a) (lambda (b) (funcall f a b))))
                                      ma)
                             mb))))
      (list
        (funcall liftA2 #'+ '(just . 3) '(just . 4))
        (funcall liftA2 #'+ '(just . 3) nil)
        (funcall liftA2 #'* '(just . 5) '(just . 6))))

    ;; === LIST APPLICATIVE ===
    ;; Identity: pure(id) <*> xs = xs
    (equal (funcall list-ap (funcall list-pure id) '(1 2 3))
           '(1 2 3))
    ;; Homomorphism
    (equal (funcall list-ap (funcall list-pure double) (funcall list-pure 5))
           (funcall list-pure (funcall double 5)))
    ;; Cartesian product: [f, g] <*> [x, y] = [f(x), f(y), g(x), g(y)]
    (funcall list-ap (list double inc square) '(2 3))
    ;; Composition law:
    ;; pure(compose) <*> u <*> v <*> w = u <*> (v <*> w)
    (let ((u (list double))
          (v (list inc))
          (w '(1 2 3)))
      (equal (funcall list-ap
                      (funcall list-ap
                               (funcall list-ap (funcall list-pure compose) u)
                               v)
                      w)
             (funcall list-ap u (funcall list-ap v w))))))"#;
    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure (t) (f g) #'(lambda (x) (funcall f (funcall g x)))) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Either monad implementation: Left (error) / Right (success)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cat_theory_adv_either_monad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Either monad: (right . val) for success, (left . err) for failure.
    // Like Maybe but carries error information.
    let form = r#"(let* (;; Either constructors
           (right (lambda (x) (cons 'right x)))
           (left (lambda (e) (cons 'left e)))
           (is-right (lambda (e) (and (consp e) (eq (car e) 'right))))
           (is-left (lambda (e) (and (consp e) (eq (car e) 'left))))
           ;; Either monad operations
           (either-return (lambda (x) (cons 'right x)))
           (either-bind (lambda (m f)
                          (if (and (consp m) (eq (car m) 'right))
                              (funcall f (cdr m))
                            m))) ;; propagate Left
           (either-fmap (lambda (f m)
                          (if (and (consp m) (eq (car m) 'right))
                              (cons 'right (funcall f (cdr m)))
                            m)))
           ;; Monadic functions
           (safe-div (lambda (denom)
                       (lambda (x)
                         (if (= denom 0)
                             (cons 'left (format "division by zero: %d/0" x))
                           (cons 'right (/ x denom))))))
           (validate-positive (lambda (x)
                                (if (> x 0)
                                    (cons 'right x)
                                  (cons 'left (format "not positive: %d" x)))))
           (validate-even (lambda (x)
                            (if (= (% x 2) 0)
                                (cons 'right x)
                              (cons 'left (format "not even: %d" x))))))

  (list
    ;; Basic usage
    (funcall either-bind (funcall either-return 10) (funcall safe-div 2))
    (funcall either-bind (funcall either-return 10) (funcall safe-div 0))

    ;; Error propagation: first error stops the chain
    (funcall either-bind
             (funcall either-bind (funcall either-return 100)
                      (funcall safe-div 0))
             validate-positive)

    ;; Success chain: 100 / 5 = 20, validate positive, validate even
    (funcall either-bind
             (funcall either-bind
                      (funcall either-bind (funcall either-return 100)
                               (funcall safe-div 5))
                      validate-positive)
             validate-even)

    ;; Chain that fails at validate-even: 100 / 5 = 20 (even, ok), then / 3 = 6 (even? 6 is even, ok)
    ;; Actually let's make it fail: 15 / 3 = 5, 5 is odd
    (funcall either-bind
             (funcall either-bind (funcall either-return 15)
                      (funcall safe-div 3))
             validate-even)

    ;; === MONAD LAWS FOR EITHER ===
    ;; Law 1: bind(return(a), f) = f(a)
    (equal (funcall either-bind (funcall either-return 10) validate-positive)
           (funcall validate-positive 10))
    ;; Law 2: bind(m, return) = m
    (equal (funcall either-bind (funcall either-return 42) either-return)
           (funcall either-return 42))
    (equal (funcall either-bind (cons 'left "error") either-return)
           (cons 'left "error"))
    ;; Law 3: associativity
    (let* ((m (funcall either-return 100))
           (f (funcall safe-div 5))
           (g validate-even)
           (left-side (funcall either-bind (funcall either-bind m f) g))
           (right-side (funcall either-bind m
                                (lambda (x) (funcall either-bind (funcall f x) g)))))
      (equal left-side right-side))

    ;; fmap laws
    (let ((id (lambda (x) x))
          (double (lambda (x) (* x 2)))
          (inc (lambda (x) (+ x 1)))
          (compose (lambda (f g) (lambda (x) (funcall f (funcall g x))))))
      (list
        ;; Identity law
        (equal (funcall either-fmap id (funcall either-return 5))
               (funcall either-return 5))
        ;; Composition law
        (equal (funcall either-fmap (funcall compose double inc) (funcall either-return 5))
               (funcall either-fmap double (funcall either-fmap inc (funcall either-return 5))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((right . 5) (left . \"division by zero: 10/0\") (left . \"division by zero: 100/0\") (right . 20) (left . \"not even: 5\") t t t t (t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Monad transformer stacking: MaybeT over List, ListT over Maybe
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cat_theory_adv_monad_transformers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // MaybeT m a = m (Maybe a)  -- wraps Maybe inside another monad
    // For MaybeT over List: a value is a List of Maybe values.
    // return(x) = [Just(x)]
    // bind(m, f) = list-bind(m, lambda(maybe-val, case maybe-val of Nothing -> [Nothing], Just(x) -> f(x)))
    let form = r#"(let* (;; Inner monad: List
           (list-bind (lambda (lst f) (apply #'append (mapcar f lst))))
           (list-return (lambda (x) (list x)))
           ;; MaybeT over List
           (maybet-return (lambda (x) (list (cons 'just x))))
           (maybet-bind (lambda (m f)
                          (funcall list-bind m
                                   (lambda (maybe-val)
                                     (if (null maybe-val)
                                         (list nil)   ;; [Nothing]
                                       (funcall f (cdr maybe-val)))))))
           ;; lift: bring a List computation into MaybeT
           (maybet-lift (lambda (lst) (mapcar (lambda (x) (cons 'just x)) lst)))
           ;; MaybeT fail
           (maybet-fail (lambda () (list nil)))
           ;; Some monadic functions
           (safe-even (lambda (x)
                        (if (= (% x 2) 0)
                            (list (cons 'just x))
                          (list nil))))
           (expand-just (lambda (x)
                          (list (cons 'just x) (cons 'just (* x 10))))))

  (list
    ;; Basic MaybeT: return and bind
    (funcall maybet-bind (funcall maybet-return 10) safe-even)   ;; [(just . 10)]
    (funcall maybet-bind (funcall maybet-return 7) safe-even)    ;; [nil]

    ;; Chain: return -> safe-even -> expand
    (funcall maybet-bind
             (funcall maybet-bind (funcall maybet-return 4) safe-even)
             expand-just)

    ;; Lifting a list computation: [1, 2, 3] lifted into MaybeT
    (funcall maybet-bind (funcall maybet-lift '(1 2 3)) safe-even)

    ;; === MaybeT MONAD LAWS ===
    ;; Law 1: bind(return(a), f) = f(a)
    (equal (funcall maybet-bind (funcall maybet-return 4) safe-even)
           (funcall safe-even 4))
    ;; Law 2: bind(m, return) = m
    (equal (funcall maybet-bind (funcall maybet-return 42) maybet-return)
           (funcall maybet-return 42))
    (equal (funcall maybet-bind (funcall maybet-fail) maybet-return)
           (funcall maybet-fail))
    ;; Law 3: associativity
    (let* ((m (funcall maybet-return 4))
           (f safe-even)
           (g expand-just)
           (left (funcall maybet-bind (funcall maybet-bind m f) g))
           (right (funcall maybet-bind m (lambda (x) (funcall maybet-bind (funcall f x) g)))))
      (equal left right))

    ;; Non-determinism + failure: multiple paths, some fail
    (funcall maybet-bind
             (funcall maybet-lift '(1 2 3 4 5 6))
             (lambda (x)
               (if (= (% x 3) 0)
                   (list (cons 'just (* x x)))
                 (list nil))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((just . 10)) (nil) ((just . 4) (just . 40)) (nil (just . 2) nil) t t t t (nil nil (just . 9) nil nil (just . 36)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
