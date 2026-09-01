//! Oracle parity tests for advanced `identity` function usage:
//! as argument to higher-order functions (mapcar, sort, seq-filter,
//! seq-remove), as default function parameter, identity in composition
//! chains, and identity with various types.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// identity with various types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_identity_various_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Numbers
  (identity 0)
  (identity 42)
  (identity -17)
  (identity 3.14159)
  (identity most-positive-fixnum)
  (identity most-negative-fixnum)
  ;; Strings
  (identity "")
  (identity "hello world")
  (identity "multi\nline\tstring")
  ;; Symbols
  (identity 'foo)
  (identity 'bar-baz)
  (identity nil)
  (identity t)
  ;; Lists
  (identity '())
  (identity '(1 2 3))
  (identity '(a (b c) (d (e f))))
  ;; Vectors
  (identity [])
  (identity [1 2 3])
  (identity [a "b" 3])
  ;; Cons cells
  (identity '(1 . 2))
  ;; Characters
  (identity ?A)
  (identity ?\n)
  ;; eq preservation: identity returns its exact argument
  (let ((x (list 1 2 3)))
    (eq x (identity x)))
  (let ((s "test"))
    (eq s (identity s)))
  (let ((v (vector 1 2)))
    (eq v (identity v))))"#;
    let expect = expect_test::expect![[
        r#""OK (0 42 -17 3.14159 0 0 \"\" \"hello world\" \"multi\\nline\tstring\" foo bar-baz nil t nil (1 2 3) (a (b c) (d (e f))) [] [1 2 3] [a \"b\" 3] (1 . 2) 65 10 t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// identity as argument to higher-order functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_identity_higher_order_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; mapcar with identity is a shallow copy
  (let* ((orig '(1 2 3 4 5))
         (copied (mapcar #'identity orig)))
    (list copied
          (equal orig copied)
          (eq orig copied)))
  ;; mapcar identity on mixed-type list
  (mapcar #'identity '(1 "two" three (4 . 5) [6 7] nil t))
  ;; sort with identity as key (via funcall in comparator)
  (let ((data (list 5 3 8 1 9 2 7 4 6)))
    (sort (copy-sequence data)
          (lambda (a b) (< (funcall #'identity a)
                           (funcall #'identity b)))))
  ;; mapc with identity (side-effect: none, returns first arg)
  (let ((acc nil))
    (mapc (lambda (x)
            (setq acc (cons (identity x) acc)))
          '(a b c d))
    (nreverse acc))
  ;; mapconcat with identity on string list
  (mapconcat #'identity '("hello" "world" "foo") "-")
  (mapconcat #'identity '("a" "b" "c" "d") "")
  (mapconcat #'identity '() ",")
  ;; identity in nested mapcar
  (mapcar (lambda (lst) (mapcar #'identity lst))
          '((1 2) (3 4) (5 6))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2 3 4 5) t nil) (1 \"two\" three (4 . 5) [6 7] nil t) (1 2 3 4 5 6 7 8 9) (a b c d) \"hello-world-foo\" \"abcd\" \"\" ((1 2) (3 4) (5 6)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// identity as default function parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_identity_default_function_parameter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-transform-list
    (lambda (lst &optional transform-fn filter-fn)
      "Transform LST: apply TRANSFORM-FN to each element (default identity),
       then keep only elements where FILTER-FN returns non-nil (default identity)."
      (let ((tf (or transform-fn #'identity))
            (ff (or filter-fn #'identity))
            (result nil))
        (dolist (x lst)
          (let ((transformed (funcall tf x)))
            (when (funcall ff transformed)
              (setq result (cons transformed result)))))
        (nreverse result))))

  (fset 'neovm--test-reduce
    (lambda (fn lst &optional init key-fn)
      "Reduce LST with FN. KEY-FN extracts value from each element (default identity)."
      (let ((kf (or key-fn #'identity))
            (acc init)
            (started (not (null init))))
        (dolist (x lst)
          (let ((val (funcall kf x)))
            (if (not started)
                (setq acc val started t)
              (setq acc (funcall fn acc val)))))
        acc)))

  (unwind-protect
      (list
        ;; No transform, no filter (both default to identity)
        (funcall 'neovm--test-transform-list '(1 2 3 4 5))
        ;; With transform, default filter
        (funcall 'neovm--test-transform-list '(1 2 3 4 5)
                 (lambda (x) (* x x)))
        ;; Default transform, with filter
        (funcall 'neovm--test-transform-list '(1 2 3 nil 4 nil 5)
                 nil
                 (lambda (x) (and x (> x 2))))
        ;; Both specified
        (funcall 'neovm--test-transform-list '(1 2 3 4 5 6)
                 (lambda (x) (* x 10))
                 (lambda (x) (= 0 (% x 20))))
        ;; Reduce with default key-fn (identity)
        (funcall 'neovm--test-reduce #'+ '(1 2 3 4 5))
        ;; Reduce with key-fn extracting car from alist
        (funcall 'neovm--test-reduce #'+
                 '((10 . a) (20 . b) (30 . c))
                 0
                 #'car)
        ;; Reduce with key-fn extracting string length
        (funcall 'neovm--test-reduce #'+
                 '("hello" "world" "!")
                 0
                 #'length))
    (fmakunbound 'neovm--test-transform-list)
    (fmakunbound 'neovm--test-reduce)))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 2 3 4 5) (1 4 9 16 25) (3 4 5) (20 40 60) 15 60 11)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// identity in composition chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_identity_composition_chains() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-compose2
    (lambda (f g)
      "Compose two functions: (f . g)(x) = f(g(x))."
      (lambda (x) (funcall f (funcall g x)))))

  (unwind-protect
      (let* ((double (lambda (x) (* x 2)))
             (add1 (lambda (x) (+ x 1)))
             (negate (lambda (x) (- x)))
             ;; identity composed with anything = that thing
             (id-then-double (funcall 'neovm--test-compose2 double #'identity))
             (double-then-id (funcall 'neovm--test-compose2 #'identity double))
             ;; identity composed with identity = identity
             (id-id (funcall 'neovm--test-compose2 #'identity #'identity))
             ;; Multi-step chain with identity sprinkled in
             (chain (funcall 'neovm--test-compose2
                             #'identity
                             (funcall 'neovm--test-compose2
                                      add1
                                      (funcall 'neovm--test-compose2
                                               #'identity
                                               double)))))
        (list
          ;; identity . double = double
          (funcall id-then-double 7)
          ;; double . identity = double
          (funcall double-then-id 7)
          ;; identity . identity = identity
          (funcall id-id 42)
          (funcall id-id "hello")
          (funcall id-id '(1 2 3))
          ;; chain: identity(add1(identity(double(5)))) = add1(double(5)) = 11
          (funcall chain 5)
          ;; Verify identity is left and right identity of composition
          (let ((test-vals '(0 -5 100 42)))
            (mapcar (lambda (v)
                      (list (= (funcall id-then-double v) (funcall double v))
                            (= (funcall double-then-id v) (funcall double v))))
                    test-vals))
          ;; identity as no-op in a pipeline
          (let ((pipeline (list #'identity double add1 #'identity negate #'identity))
                (val 10))
            (dolist (f pipeline)
              (setq val (funcall f val)))
            val)))
    (fmakunbound 'neovm--test-compose2)))"#;
    let expect = expect_test::expect![[
        r#""OK (14 14 42 \"hello\" (1 2 3) 11 ((t t) (t t) (t t) (t t)) -21)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// identity in filtering and partitioning
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_identity_filter_partition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-filter
    (lambda (pred lst)
      "Return elements of LST where PRED returns non-nil."
      (let ((result nil))
        (dolist (x lst)
          (when (funcall pred x)
            (setq result (cons x result))))
        (nreverse result))))

  (fset 'neovm--test-remove
    (lambda (pred lst)
      "Remove elements of LST where PRED returns non-nil."
      (let ((result nil))
        (dolist (x lst)
          (unless (funcall pred x)
            (setq result (cons x result))))
        (nreverse result))))

  (fset 'neovm--test-partition
    (lambda (pred lst)
      "Split LST into (matching . non-matching)."
      (let ((yes nil) (no nil))
        (dolist (x lst)
          (if (funcall pred x)
              (setq yes (cons x yes))
            (setq no (cons x no))))
        (cons (nreverse yes) (nreverse no)))))

  (unwind-protect
      (list
        ;; Filter with identity: removes nil values
        (funcall 'neovm--test-filter #'identity
                 '(1 nil 2 nil nil 3 t "hello" nil))
        ;; Remove with identity: keeps only nil values
        (funcall 'neovm--test-remove #'identity
                 '(1 nil 2 nil nil 3 t "hello" nil))
        ;; Partition by identity: truthy vs falsy
        (funcall 'neovm--test-partition #'identity
                 '(0 nil 1 nil "" t 42 nil))
        ;; identity as a "truthy" check in complex data
        (let ((records '((:name "Alice" :active t)
                         (:name "Bob" :active nil)
                         (:name "Carol" :active t)
                         (:name "Dave" :active nil)
                         (:name "Eve" :active t))))
          (funcall 'neovm--test-filter
                   (lambda (r) (identity (plist-get r :active)))
                   records))
        ;; Nested filter: remove nil sublists then filter non-nil elements
        (let ((data '((1 nil 2) nil (nil nil) (3 4) nil (5))))
          (mapcar (lambda (sub)
                    (funcall 'neovm--test-filter #'identity sub))
                  (funcall 'neovm--test-filter #'identity data)))
        ;; Count truthy values using identity
        (let ((mixed '(0 1 nil t "" "a" () (x) [] [1])))
          (length (funcall 'neovm--test-filter #'identity mixed))))
    (fmakunbound 'neovm--test-filter)
    (fmakunbound 'neovm--test-remove)
    (fmakunbound 'neovm--test-partition)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 t \"hello\") (nil nil nil nil) ((0 1 \"\" t 42) nil nil nil) ((:name \"Alice\" :active t) (:name \"Carol\" :active t) (:name \"Eve\" :active t)) ((1 2) nil (3 4) (5)) 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// identity in generic dispatch and wrapper patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_identity_dispatch_wrapper() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-make-processor
    (lambda (&optional pre-fn process-fn post-fn)
      "Create a data processor with optional pre/post hooks.
       Each defaults to identity if not provided."
      (let ((pre (or pre-fn #'identity))
            (proc (or process-fn #'identity))
            (post (or post-fn #'identity)))
        (lambda (data)
          (funcall post (funcall proc (funcall pre data)))))))

  (fset 'neovm--test-make-validator
    (lambda (validators)
      "Create a validator that runs all VALIDATORS on input.
       Each validator is (name . fn) where fn returns nil on failure.
       Uses identity as pass-through for unconditional acceptance."
      (lambda (value)
        (let ((results nil)
              (all-pass t))
          (dolist (v validators)
            (let* ((name (car v))
                   (fn (cdr v))
                   (ok (funcall fn value)))
              (setq results (cons (cons name (if ok 'pass 'fail)) results))
              (unless ok (setq all-pass nil))))
          (list :valid all-pass :checks (nreverse results))))))

  (unwind-protect
      (list
        ;; Processor with all defaults (triple identity)
        (funcall (funcall 'neovm--test-make-processor) 42)
        (funcall (funcall 'neovm--test-make-processor) "hello")
        ;; Processor with only pre-fn
        (funcall (funcall 'neovm--test-make-processor
                          (lambda (x) (* x 2)))
                 5)
        ;; Processor with pre and post, default process
        (funcall (funcall 'neovm--test-make-processor
                          (lambda (x) (+ x 10))
                          nil
                          (lambda (x) (* x 3)))
                 5)
        ;; Full pipeline
        (funcall (funcall 'neovm--test-make-processor
                          (lambda (x) (number-to-string x))
                          (lambda (s) (concat "[" s "]"))
                          #'upcase)
                 42)
        ;; Validator with identity as always-pass validator
        (let ((v (funcall 'neovm--test-make-validator
                          (list (cons "always-pass" #'identity)
                                (cons "is-number" #'numberp)
                                (cons "positive" (lambda (x) (and (numberp x) (> x 0))))))))
          (list (funcall v 42)
                (funcall v -5)
                (funcall v nil)
                (funcall v "hello")))
        ;; Compose processors
        (let* ((p1 (funcall 'neovm--test-make-processor
                            (lambda (x) (+ x 1))))
               (p2 (funcall 'neovm--test-make-processor
                            nil
                            (lambda (x) (* x 2))))
               (combined (funcall 'neovm--test-make-processor p1 p2)))
          (funcall combined 10)))
    (fmakunbound 'neovm--test-make-processor)
    (fmakunbound 'neovm--test-make-validator)))"#;
    let expect = expect_test::expect![[
        r#""OK (42 \"hello\" 10 45 \"[42]\" ((:valid t :checks ((\"always-pass\" . pass) (\"is-number\" . pass) (\"positive\" . pass))) (:valid nil :checks ((\"always-pass\" . pass) (\"is-number\" . pass) (\"positive\" . fail))) (:valid nil :checks ((\"always-pass\" . fail) (\"is-number\" . fail) (\"positive\" . fail))) (:valid nil :checks ((\"always-pass\" . pass) (\"is-number\" . fail) (\"positive\" . fail)))) 22)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// identity with sorting, deduplication, grouping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_identity_sort_dedup_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-sort-by
    (lambda (lst key-fn cmp-fn)
      "Sort LST by (KEY-FN element), using CMP-FN for comparison."
      (sort (copy-sequence lst)
            (lambda (a b)
              (funcall cmp-fn
                       (funcall key-fn a)
                       (funcall key-fn b))))))

  (fset 'neovm--test-group-by
    (lambda (lst key-fn)
      "Group elements of LST by (KEY-FN element). Returns alist."
      (let ((groups nil))
        (dolist (x lst)
          (let* ((k (funcall key-fn x))
                 (existing (assoc k groups)))
            (if existing
                (setcdr existing (cons x (cdr existing)))
              (setq groups (cons (cons k (list x)) groups)))))
        ;; Reverse each group to maintain order
        (mapcar (lambda (g) (cons (car g) (nreverse (cdr g))))
                (nreverse groups)))))

  (fset 'neovm--test-dedup
    (lambda (lst &optional key-fn)
      "Remove consecutive duplicates, comparing by KEY-FN (default identity)."
      (let ((kf (or key-fn #'identity))
            (result nil)
            (prev-key :neovm--sentinel))
        (dolist (x lst)
          (let ((k (funcall kf x)))
            (unless (equal k prev-key)
              (setq result (cons x result)
                    prev-key k))))
        (nreverse result))))

  (unwind-protect
      (list
        ;; Sort by identity (natural order)
        (funcall 'neovm--test-sort-by '(5 3 8 1 9 2) #'identity #'<)
        (funcall 'neovm--test-sort-by '("banana" "apple" "cherry") #'identity #'string<)
        ;; Sort by extracted key vs identity
        (funcall 'neovm--test-sort-by
                 '((3 . "c") (1 . "a") (2 . "b"))
                 #'car #'<)
        ;; Group by identity: each unique value is its own group
        (funcall 'neovm--test-group-by '(1 2 1 3 2 1 3 3) #'identity)
        ;; Group by key function
        (funcall 'neovm--test-group-by
                 '("apple" "ant" "banana" "avocado" "berry")
                 (lambda (s) (aref s 0)))
        ;; Dedup with default identity key
        (funcall 'neovm--test-dedup '(1 1 2 2 2 3 3 1 1))
        (funcall 'neovm--test-dedup '("a" "a" "b" "b" "a"))
        ;; Dedup with custom key
        (funcall 'neovm--test-dedup
                 '((1 . "a") (1 . "b") (2 . "c") (2 . "d") (3 . "e"))
                 #'car)
        ;; Empty and single-element cases
        (funcall 'neovm--test-dedup '())
        (funcall 'neovm--test-dedup '(42)))
    (fmakunbound 'neovm--test-sort-by)
    (fmakunbound 'neovm--test-group-by)
    (fmakunbound 'neovm--test-dedup)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 5 8 9) (\"apple\" \"banana\" \"cherry\") ((1 . \"a\") (2 . \"b\") (3 . \"c\")) ((1 1 1 1) (2 2 2) (3 3 3 3)) ((97 \"apple\" \"ant\" \"avocado\") (98 \"banana\" \"berry\")) (1 2 3 1) (\"a\" \"b\" \"a\") ((1 . \"a\") (2 . \"c\") (3 . \"e\")) nil (42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// identity preserves eq, used in memoization key extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_identity_eq_preservation_and_caching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-make-cache
    (lambda (&optional key-fn)
      "Create a cache that extracts keys via KEY-FN (default identity).
       Returns a closure with :get, :put, :has, :keys operations."
      (let ((store (make-hash-table :test 'equal))
            (kf (or key-fn #'identity)))
        (lambda (op &rest args)
          (cond
            ((eq op :put)
             (let* ((item (car args))
                    (value (cadr args))
                    (key (funcall kf item)))
               (puthash key value store)
               value))
            ((eq op :get)
             (let ((key (funcall kf (car args))))
               (gethash key store)))
            ((eq op :has)
             (let ((key (funcall kf (car args))))
               (not (eq (gethash key store :neovm--miss) :neovm--miss))))
            ((eq op :keys)
             (let ((ks nil))
               (maphash (lambda (k _v) (setq ks (cons k ks))) store)
               (sort ks (lambda (a b)
                          (string< (format "%s" a) (format "%s" b))))))
            ((eq op :size)
             (hash-table-count store)))))))

  (unwind-protect
      (let ((c1 (funcall 'neovm--test-make-cache))
            (c2 (funcall 'neovm--test-make-cache #'car)))
        ;; Cache with identity key: key is the item itself
        (funcall c1 :put "hello" 1)
        (funcall c1 :put "world" 2)
        (funcall c1 :put 42 3)
        ;; Cache with car as key: key is extracted from item
        (funcall c2 :put '(alice 30) "person-a")
        (funcall c2 :put '(bob 25) "person-b")
        (funcall c2 :put '(alice 35) "person-a-updated")
        (list
          ;; identity-keyed cache lookups
          (funcall c1 :get "hello")
          (funcall c1 :get "world")
          (funcall c1 :get 42)
          (funcall c1 :has "hello")
          (funcall c1 :has "missing")
          (funcall c1 :size)
          (funcall c1 :keys)
          ;; car-keyed cache: alice was overwritten
          (funcall c2 :get '(alice))
          (funcall c2 :get '(bob))
          (funcall c2 :size)
          (funcall c2 :keys)))
    (fmakunbound 'neovm--test-make-cache)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 2 3 t nil 3 (42 \"hello\" \"world\") \"person-a-updated\" \"person-b\" 2 (alice bob))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
