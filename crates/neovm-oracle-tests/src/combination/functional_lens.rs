//! Functional lens/optics oracle parity tests:
//! getter/setter lens construction, lens composition, over (modify through lens),
//! lens laws (get-put, put-get, put-put), prism for sum types, traversal for
//! collections, lens-based data transformation pipeline, immutable record update.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Getter/setter lens construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functional_lens_basic_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A lens is a pair of (getter . setter) functions.
    // Getter: record -> value
    // Setter: record -> new-value -> new-record
    // Records are alists.
    let form = r#"(progn
  ;; Make a lens for an alist key
  (fset 'neovm--lens-make
    (lambda (key)
      (cons
       ;; getter
       (lambda (record) (cdr (assq key record)))
       ;; setter: returns new record with key updated
       (lambda (record new-val)
         (mapcar (lambda (pair)
                   (if (eq (car pair) key)
                       (cons key new-val)
                     pair))
                 record)))))

  ;; Lens operations
  (fset 'neovm--lens-get
    (lambda (lens record)
      (funcall (car lens) record)))

  (fset 'neovm--lens-set
    (lambda (lens record new-val)
      (funcall (cdr lens) record new-val)))

  ;; Over: apply a function through a lens
  (fset 'neovm--lens-over
    (lambda (lens record f)
      (let ((old-val (funcall 'neovm--lens-get lens record)))
        (funcall 'neovm--lens-set lens record (funcall f old-val)))))

  (unwind-protect
      (let ((name-lens (funcall 'neovm--lens-make 'name))
            (age-lens (funcall 'neovm--lens-make 'age))
            (score-lens (funcall 'neovm--lens-make 'score))
            (record '((name . alice) (age . 30) (score . 95))))
        (list
         ;; Get operations
         (funcall 'neovm--lens-get name-lens record)
         (funcall 'neovm--lens-get age-lens record)
         (funcall 'neovm--lens-get score-lens record)
         ;; Set operations (immutable: returns new record)
         (funcall 'neovm--lens-set age-lens record 31)
         ;; Original record unchanged
         (funcall 'neovm--lens-get age-lens record)
         ;; Over: increment age
         (funcall 'neovm--lens-over age-lens record #'1+)
         ;; Over: double score
         (funcall 'neovm--lens-over score-lens record (lambda (s) (* s 2)))
         ;; Chain: set name then increment age
         (let ((r2 (funcall 'neovm--lens-set name-lens record 'bob)))
           (funcall 'neovm--lens-over age-lens r2 #'1+))))
    (fmakunbound 'neovm--lens-make)
    (fmakunbound 'neovm--lens-get)
    (fmakunbound 'neovm--lens-set)
    (fmakunbound 'neovm--lens-over)))"#;
    let expect = expect_test::expect![[
        r#""OK (alice 30 95 ((name . alice) (age . 31) (score . 95)) 30 ((name . alice) (age . 31) (score . 95)) ((name . alice) (age . 30) (score . 190)) ((name . bob) (age . 31) (score . 95)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lens composition: nested field access
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functional_lens_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compose two lenses to access nested structures
    let form = r#"(progn
  (fset 'neovm--lens-make
    (lambda (key)
      (cons
       (lambda (record) (cdr (assq key record)))
       (lambda (record new-val)
         (mapcar (lambda (pair)
                   (if (eq (car pair) key) (cons key new-val) pair))
                 record)))))

  (fset 'neovm--lens-get
    (lambda (lens record) (funcall (car lens) record)))

  (fset 'neovm--lens-set
    (lambda (lens record new-val) (funcall (cdr lens) record new-val)))

  (fset 'neovm--lens-over
    (lambda (lens record f)
      (funcall 'neovm--lens-set lens record
               (funcall f (funcall 'neovm--lens-get lens record)))))

  ;; Compose: outer then inner lens
  (fset 'neovm--lens-compose
    (lambda (outer inner)
      (cons
       ;; getter: get outer, then get inner from that
       (lambda (record)
         (funcall 'neovm--lens-get inner
                  (funcall 'neovm--lens-get outer record)))
       ;; setter: get outer sub-record, set inner on it, set outer back
       (lambda (record new-val)
         (let ((sub (funcall 'neovm--lens-get outer record)))
           (funcall 'neovm--lens-set outer record
                    (funcall 'neovm--lens-set inner sub new-val)))))))

  (unwind-protect
      (let ((address-lens (funcall 'neovm--lens-make 'address))
            (city-lens (funcall 'neovm--lens-make 'city))
            (zip-lens (funcall 'neovm--lens-make 'zip))
            (person '((name . alice)
                      (address . ((city . "Portland")
                                  (zip . "97201")
                                  (state . "OR"))))))
        (let ((city-of-address (funcall 'neovm--lens-compose address-lens city-lens))
              (zip-of-address (funcall 'neovm--lens-compose address-lens zip-lens)))
          (list
           ;; Get nested field
           (funcall 'neovm--lens-get city-of-address person)
           (funcall 'neovm--lens-get zip-of-address person)
           ;; Set nested field (immutable update)
           (funcall 'neovm--lens-set city-of-address person "Seattle")
           ;; Over on nested field
           (funcall 'neovm--lens-over zip-of-address person
                    (lambda (z) (concat z "-0000")))
           ;; Original unchanged
           (funcall 'neovm--lens-get city-of-address person)
           ;; Triple composition: person -> address -> city
           ;; Verify composing the same way gives identical result
           (equal (funcall 'neovm--lens-get
                           (funcall 'neovm--lens-compose address-lens city-lens)
                           person)
                  "Portland"))))
    (fmakunbound 'neovm--lens-make)
    (fmakunbound 'neovm--lens-get)
    (fmakunbound 'neovm--lens-set)
    (fmakunbound 'neovm--lens-over)
    (fmakunbound 'neovm--lens-compose)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Portland\" \"97201\" ((name . alice) (address (city . \"Seattle\") (zip . \"97201\") (state . \"OR\"))) ((name . alice) (address (city . \"Portland\") (zip . \"97201-0000\") (state . \"OR\"))) \"Portland\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lens laws: get-put, put-get, put-put
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functional_lens_laws() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify the three lens laws hold for our lens implementation
    let form = r#"(progn
  (fset 'neovm--lens-make
    (lambda (key)
      (cons
       (lambda (record) (cdr (assq key record)))
       (lambda (record new-val)
         (mapcar (lambda (pair)
                   (if (eq (car pair) key) (cons key new-val) pair))
                 record)))))

  (fset 'neovm--lens-get
    (lambda (lens record) (funcall (car lens) record)))

  (fset 'neovm--lens-set
    (lambda (lens record new-val) (funcall (cdr lens) record new-val)))

  (unwind-protect
      (let ((lens (funcall 'neovm--lens-make 'x))
            (r '((x . 10) (y . 20) (z . 30))))
        (list
         ;; Law 1: get-put (set then get returns what was set)
         ;; get(set(r, v)) = v
         (equal (funcall 'neovm--lens-get lens
                         (funcall 'neovm--lens-set lens r 42))
                42)
         ;; Law 2: put-get (setting with what you got is identity)
         ;; set(r, get(r)) = r
         (equal (funcall 'neovm--lens-set lens r
                         (funcall 'neovm--lens-get lens r))
                r)
         ;; Law 3: put-put (setting twice is same as setting once with last value)
         ;; set(set(r, v1), v2) = set(r, v2)
         (equal (funcall 'neovm--lens-set lens
                         (funcall 'neovm--lens-set lens r 100)
                         200)
                (funcall 'neovm--lens-set lens r 200))
         ;; Laws with different lens
         (let ((y-lens (funcall 'neovm--lens-make 'y)))
           (list
            ;; get-put for y
            (equal (funcall 'neovm--lens-get y-lens
                            (funcall 'neovm--lens-set y-lens r 99))
                   99)
            ;; put-get for y
            (equal (funcall 'neovm--lens-set y-lens r
                            (funcall 'neovm--lens-get y-lens r))
                   r)
            ;; put-put for y
            (equal (funcall 'neovm--lens-set y-lens
                            (funcall 'neovm--lens-set y-lens r 50)
                            75)
                   (funcall 'neovm--lens-set y-lens r 75))))))
    (fmakunbound 'neovm--lens-make)
    (fmakunbound 'neovm--lens-get)
    (fmakunbound 'neovm--lens-set)))"#;
    let expect = expect_test::expect![[r#""OK (t t t (t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Prism for sum types (optional/either)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functional_lens_prism() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A prism is a lens for sum types: it may fail to get (returns nil)
    // and only sets if the variant matches.
    // Represent sum types as tagged values: (tag . value)
    let form = r#"(progn
  ;; Make a prism for a specific tag
  (fset 'neovm--prism-make
    (lambda (tag)
      (cons
       ;; preview: returns value if tag matches, else nil
       (lambda (tagged)
         (if (and (consp tagged) (eq (car tagged) tag))
             (cons t (cdr tagged))   ;; (t . value) = success
           (cons nil nil)))          ;; (nil . nil) = failure
       ;; review: wrap a value in the tag
       (lambda (value) (cons tag value)))))

  (fset 'neovm--prism-preview
    (lambda (prism tagged)
      (funcall (car prism) tagged)))

  (fset 'neovm--prism-review
    (lambda (prism value)
      (funcall (cdr prism) value)))

  ;; Over for prism: apply f only if preview succeeds
  (fset 'neovm--prism-over
    (lambda (prism tagged f)
      (let ((result (funcall 'neovm--prism-preview prism tagged)))
        (if (car result)
            (funcall 'neovm--prism-review prism (funcall f (cdr result)))
          tagged))))  ;; Return unchanged if wrong variant

  (unwind-protect
      (let ((some-prism (funcall 'neovm--prism-make 'some))
            (none-prism (funcall 'neovm--prism-make 'none))
            (left-prism (funcall 'neovm--prism-make 'left))
            (right-prism (funcall 'neovm--prism-make 'right)))
        (list
         ;; Preview some value with some-prism: succeeds
         (funcall 'neovm--prism-preview some-prism '(some . 42))
         ;; Preview none value with some-prism: fails
         (funcall 'neovm--prism-preview some-prism '(none))
         ;; Review: construct a some value
         (funcall 'neovm--prism-review some-prism 99)
         ;; Over: double the value if it's some
         (funcall 'neovm--prism-over some-prism '(some . 5) (lambda (x) (* x 2)))
         ;; Over on wrong variant: unchanged
         (funcall 'neovm--prism-over some-prism '(none) (lambda (x) (* x 2)))
         ;; Either type: left and right
         (funcall 'neovm--prism-preview left-prism '(left . "error"))
         (funcall 'neovm--prism-preview right-prism '(left . "error"))
         (funcall 'neovm--prism-over right-prism '(right . 10) #'1+)
         (funcall 'neovm--prism-over right-prism '(left . "err") #'1+)))
    (fmakunbound 'neovm--prism-make)
    (fmakunbound 'neovm--prism-preview)
    (fmakunbound 'neovm--prism-review)
    (fmakunbound 'neovm--prism-over)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t . 42) (nil) (some . 99) (some . 10) (none) (t . \"error\") (nil) (right . 11) (left . \"err\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Traversal for collections
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functional_lens_traversal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A traversal focuses on all elements of a collection.
    // traverse-over applies a function to every element.
    // traverse-get collects all focused values.
    let form = r#"(progn
  (fset 'neovm--lens-make
    (lambda (key)
      (cons
       (lambda (record) (cdr (assq key record)))
       (lambda (record new-val)
         (mapcar (lambda (pair)
                   (if (eq (car pair) key) (cons key new-val) pair))
                 record)))))

  (fset 'neovm--lens-get
    (lambda (lens record) (funcall (car lens) record)))

  (fset 'neovm--lens-set
    (lambda (lens record new-val) (funcall (cdr lens) record new-val)))

  ;; Traversal: operates on all elements of a list
  (fset 'neovm--traversal-each
    (lambda ()
      (cons
       ;; get-all: return the list itself
       (lambda (lst) lst)
       ;; over-all: map a function over all elements
       (lambda (lst f) (mapcar f lst)))))

  ;; Compose a lens with a traversal: focus on a field of each element
  (fset 'neovm--lens-then-traverse
    (lambda (outer-lens traversal)
      (cons
       ;; get-all: get the collection via lens, then traverse
       (lambda (record)
         (funcall (car traversal)
                  (funcall 'neovm--lens-get outer-lens record)))
       ;; over-all: get collection, map f, set back
       (lambda (record f)
         (funcall 'neovm--lens-set outer-lens record
                  (funcall (cdr traversal)
                           (funcall 'neovm--lens-get outer-lens record)
                           f))))))

  (fset 'neovm--traversal-get-all
    (lambda (trav record) (funcall (car trav) record)))

  (fset 'neovm--traversal-over-all
    (lambda (trav record f) (funcall (cdr trav) record f)))

  (unwind-protect
      (let ((each (funcall 'neovm--traversal-each))
            (data '(1 2 3 4 5)))
        (let ((scores-lens (funcall 'neovm--lens-make 'scores))
              (student '((name . alice)
                         (scores . (85 90 78 92 88)))))
          (let ((score-trav (funcall 'neovm--lens-then-traverse scores-lens each)))
            (list
             ;; Get all scores
             (funcall 'neovm--traversal-get-all score-trav student)
             ;; Over: add 5 to each score (curve)
             (funcall 'neovm--traversal-over-all score-trav student
                      (lambda (s) (+ s 5)))
             ;; Over: cap at 100
             (funcall 'neovm--traversal-over-all score-trav student
                      (lambda (s) (min s 100)))
             ;; Simple traversal: double each
             (funcall 'neovm--traversal-over-all each data
                      (lambda (x) (* x 2)))
             ;; Chain: add 5 then double
             (funcall 'neovm--traversal-over-all each
                      (funcall 'neovm--traversal-over-all each data
                               (lambda (x) (+ x 5)))
                      (lambda (x) (* x 2)))
             ;; Original unchanged
             (funcall 'neovm--traversal-get-all score-trav student)))))
    (fmakunbound 'neovm--lens-make)
    (fmakunbound 'neovm--lens-get)
    (fmakunbound 'neovm--lens-set)
    (fmakunbound 'neovm--traversal-each)
    (fmakunbound 'neovm--lens-then-traverse)
    (fmakunbound 'neovm--traversal-get-all)
    (fmakunbound 'neovm--traversal-over-all)))"#;
    let expect = expect_test::expect![[
        r#""OK ((85 90 78 92 88) ((name . alice) (scores 90 95 83 97 93)) ((name . alice) (scores 85 90 78 92 88)) (2 4 6 8 10) (12 14 16 18 20) (85 90 78 92 88))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lens-based data transformation pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functional_lens_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a pipeline of lens-based transformations on a data structure
    let form = r#"(progn
  (fset 'neovm--lens-make
    (lambda (key)
      (cons
       (lambda (record) (cdr (assq key record)))
       (lambda (record new-val)
         (mapcar (lambda (pair)
                   (if (eq (car pair) key) (cons key new-val) pair))
                 record)))))

  (fset 'neovm--lens-get
    (lambda (lens record) (funcall (car lens) record)))

  (fset 'neovm--lens-set
    (lambda (lens record new-val) (funcall (cdr lens) record new-val)))

  (fset 'neovm--lens-over
    (lambda (lens record f)
      (funcall 'neovm--lens-set lens record
               (funcall f (funcall 'neovm--lens-get lens record)))))

  ;; Pipeline: apply a list of (lens . transform-fn) pairs in sequence
  (fset 'neovm--lens-pipeline
    (lambda (record transforms)
      (let ((result record))
        (dolist (tr transforms)
          (setq result (funcall 'neovm--lens-over (car tr) result (cdr tr))))
        result)))

  (unwind-protect
      (let ((name-lens (funcall 'neovm--lens-make 'name))
            (age-lens (funcall 'neovm--lens-make 'age))
            (score-lens (funcall 'neovm--lens-make 'score))
            (level-lens (funcall 'neovm--lens-make 'level)))
        (let ((record '((name . "alice") (age . 30) (score . 85) (level . 1))))
          (list
           ;; Pipeline: upcase name, increment age, add 10 to score, double level
           (funcall 'neovm--lens-pipeline record
                    (list (cons name-lens #'upcase)
                          (cons age-lens #'1+)
                          (cons score-lens (lambda (s) (+ s 10)))
                          (cons level-lens (lambda (l) (* l 2)))))
           ;; Pipeline with conditional transforms
           (funcall 'neovm--lens-pipeline record
                    (list (cons score-lens
                                (lambda (s) (if (>= s 90) s (+ s 15))))
                          (cons level-lens
                                (lambda (l) (if (> l 1) l (+ l 1))))))
           ;; Empty pipeline: identity
           (equal (funcall 'neovm--lens-pipeline record nil) record)
           ;; Original unchanged after all pipelines
           (funcall 'neovm--lens-get score-lens record))))
    (fmakunbound 'neovm--lens-make)
    (fmakunbound 'neovm--lens-get)
    (fmakunbound 'neovm--lens-set)
    (fmakunbound 'neovm--lens-over)
    (fmakunbound 'neovm--lens-pipeline)))"#;
    let expect = expect_test::expect![[
        r#""OK (((name . \"ALICE\") (age . 31) (score . 95) (level . 2)) ((name . \"alice\") (age . 30) (score . 100) (level . 2)) t 85)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Immutable record update via lenses (functional record update pattern)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functional_lens_record_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Demonstrate immutable record updates: each update produces a new record,
    // original is preserved, and all intermediate versions are accessible
    let form = r#"(progn
  (fset 'neovm--lens-make
    (lambda (key)
      (cons
       (lambda (record) (cdr (assq key record)))
       (lambda (record new-val)
         (mapcar (lambda (pair)
                   (if (eq (car pair) key) (cons key new-val) pair))
                 record)))))

  (fset 'neovm--lens-get
    (lambda (lens record) (funcall (car lens) record)))

  (fset 'neovm--lens-set
    (lambda (lens record new-val) (funcall (cdr lens) record new-val)))

  (fset 'neovm--lens-over
    (lambda (lens record f)
      (funcall 'neovm--lens-set lens record
               (funcall f (funcall 'neovm--lens-get lens record)))))

  ;; Compose for nested
  (fset 'neovm--lens-compose
    (lambda (outer inner)
      (cons
       (lambda (record)
         (funcall 'neovm--lens-get inner
                  (funcall 'neovm--lens-get outer record)))
       (lambda (record new-val)
         (let ((sub (funcall 'neovm--lens-get outer record)))
           (funcall 'neovm--lens-set outer record
                    (funcall 'neovm--lens-set inner sub new-val)))))))

  ;; Record with nested address
  (unwind-protect
      (let ((name-lens (funcall 'neovm--lens-make 'name))
            (addr-lens (funcall 'neovm--lens-make 'address))
            (city-lens (funcall 'neovm--lens-make 'city))
            (zip-lens (funcall 'neovm--lens-make 'zip))
            (age-lens (funcall 'neovm--lens-make 'age)))
        (let ((city-of-addr (funcall 'neovm--lens-compose addr-lens city-lens))
              (zip-of-addr (funcall 'neovm--lens-compose addr-lens zip-lens)))
          (let ((v0 '((name . "Alice")
                      (age . 30)
                      (address . ((city . "Portland") (zip . "97201"))))))
            ;; Chain of immutable updates
            (let* ((v1 (funcall 'neovm--lens-over age-lens v0 #'1+))
                   (v2 (funcall 'neovm--lens-set city-of-addr v1 "Seattle"))
                   (v3 (funcall 'neovm--lens-set zip-of-addr v2 "98101"))
                   (v4 (funcall 'neovm--lens-set name-lens v3 "Alice B.")))
              (list
               ;; All versions are independent
               (funcall 'neovm--lens-get age-lens v0)
               (funcall 'neovm--lens-get age-lens v1)
               (funcall 'neovm--lens-get city-of-addr v0)
               (funcall 'neovm--lens-get city-of-addr v2)
               (funcall 'neovm--lens-get zip-of-addr v2)
               (funcall 'neovm--lens-get zip-of-addr v3)
               (funcall 'neovm--lens-get name-lens v4)
               ;; v0 completely unchanged
               (equal (funcall 'neovm--lens-get name-lens v0) "Alice")
               (equal (funcall 'neovm--lens-get city-of-addr v0) "Portland")
               ;; v4 has all accumulated changes
               (funcall 'neovm--lens-get age-lens v4))))))
    (fmakunbound 'neovm--lens-make)
    (fmakunbound 'neovm--lens-get)
    (fmakunbound 'neovm--lens-set)
    (fmakunbound 'neovm--lens-over)
    (fmakunbound 'neovm--lens-compose)))"#;
    let expect = expect_test::expect![[
        r#""OK (30 31 \"Portland\" \"Seattle\" \"97201\" \"98101\" \"Alice B.\" t t 31)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
