//! Comprehensive oracle parity tests for association list operations.
//!
//! Tests `assoc` with various TESTFN (eq, equal, string=), `assq`, `rassoc`/`rassq`,
//! `assoc-default`, `alist-get` with default/testfn/remove, `copy-alist` deep/shallow
//! semantics, nested multi-level alists, and complex alist building/querying.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// assoc with different test functions: eq, equal, string=, custom lambda
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_alist_testfn_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; equal (default) finds string keys by content
  (assoc "key2" '(("key1" . 10) ("key2" . 20) ("key3" . 30)))
  ;; eq testfn: symbol keys work (interned)
  (assoc 'beta '((alpha . 1) (beta . 2) (gamma . 3)) #'eq)
  ;; eq testfn: string keys fail (not identity-equal)
  (assoc "key2" '(("key1" . 10) ("key2" . 20)) #'eq)
  ;; string= as testfn
  (assoc "hello" '(("hello" . found) ("world" . other)) #'string=)
  ;; Custom lambda: case-insensitive match
  (assoc "FOO" '(("foo" . 1) ("bar" . 2) ("baz" . 3))
         (lambda (a b) (string= (downcase a) (downcase b))))
  ;; Custom lambda: prefix match
  (assoc "hel" '(("hello" . 1) ("world" . 2) ("help" . 3))
         (lambda (a b) (string-prefix-p a b)))
  ;; assoc returns nil on no match
  (assoc 'missing '((a . 1) (b . 2)))
  ;; assoc on empty alist
  (assoc 'anything '())
  ;; assoc with integer keys and = comparison (equal works for fixnums)
  (assoc 42 '((10 . "ten") (42 . "forty-two") (99 . "ninety-nine")))
  ;; assoc with list keys (equal does deep structural comparison)
  (assoc '(1 2 3) '(((1 2) . "pair") ((1 2 3) . "triple") ((4) . "single"))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"key2\" . 20) (beta . 2) nil (\"hello\" . found) (\"foo\" . 1) nil nil nil (42 . \"forty-two\") ((1 2 3) . \"triple\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// assq: identity-based key lookup
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_alist_assq_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; assq finds symbol keys (interned, identity works)
  (assq 'b '((a . 1) (b . 2) (c . 3)))
  ;; assq finds fixnum keys (eq for small integers)
  (assq 5 '((1 . "one") (5 . "five") (10 . "ten")))
  ;; assq does NOT find string keys (different objects)
  (assq "hello" '(("hello" . 1) ("world" . 2)))
  ;; assq does NOT find list keys (different cons cells)
  (assq '(a) '(((a) . 1) ((b) . 2)))
  ;; assq with nil key
  (assq nil '((nil . "found-nil") (t . "found-t")))
  ;; assq with t key
  (assq t '((nil . "nil-val") (t . "t-val")))
  ;; assq on alist with non-cons elements (skips them)
  (assq 'b '(a (b . 2) c (d . 4)))
  ;; assq returns the full pair, not just the value
  (let ((result (assq 'x '((x . 100)))))
    (list (car result) (cdr result)))
  ;; multiple matches: assq returns first
  (assq 'a '((a . 1) (b . 2) (a . 3)))
  ;; nested symbol alists
  (assq 'inner (cdr (assq 'outer '((outer (inner . deep-val) (other . xx)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((b . 2) (5 . \"five\") nil nil (nil . \"found-nil\") (t . \"t-val\") (b . 2) (x 100) (a . 1) (inner . deep-val))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// rassoc / rassq: reverse lookup by value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_alist_rassoc_rassq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; rassoc with string values (equal comparison)
  (rassoc "beta" '((1 . "alpha") (2 . "beta") (3 . "gamma")))
  ;; rassoc with symbol values
  (rassoc 'yes '((q1 . yes) (q2 . no) (q3 . yes)))
  ;; rassoc returns first match
  (rassoc 'dup '((a . dup) (b . unique) (c . dup)))
  ;; rassoc with nil value
  (rassoc nil '((a . 1) (b . nil) (c . 3)))
  ;; rassq with symbol values (eq comparison)
  (rassq 'found '((x . found) (y . lost) (z . found)))
  ;; rassq fails with string values (not identity)
  (rassq "beta" '((1 . "alpha") (2 . "beta")))
  ;; rassq with integer values
  (rassq 42 '((a . 10) (b . 42) (c . 99)))
  ;; rassoc on empty alist
  (rassoc 'anything '())
  ;; rassoc with list values
  (rassoc '(1 2) '((a . (1 2)) (b . (3 4))))
  ;; rassq with list values does NOT match (different cons cells)
  (rassq '(1 2) '((a . (1 2)) (b . (3 4)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 . \"beta\") (q1 . yes) (a . dup) (b) (x . found) nil (b . 42) nil (a 1 2) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// assoc-default with all parameter combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_alist_assoc_default_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic: returns cdr of found pair
  (assoc-default 'b '((a . 10) (b . 20) (c . 30)))
  ;; Not found: returns nil
  (assoc-default 'z '((a . 1) (b . 2)))
  ;; With TESTFN: custom comparison
  (assoc-default "HELLO" '(("hello" . "world"))
                 (lambda (a b) (string= (downcase a) (downcase b))))
  ;; With DEFAULT parameter when key not found
  (assoc-default 'missing '((a . 1)) nil 'fallback-value)
  ;; With DEFAULT parameter when key IS found (default ignored)
  (assoc-default 'a '((a . 1)) nil 'fallback-value)
  ;; TESTFN is nil means use equal (default)
  (assoc-default "x" '(("x" . 100) ("y" . 200)) nil)
  ;; string keys with default
  (assoc-default "nope" '(("yes" . 1)) nil "default-str")
  ;; Numeric keys
  (assoc-default 3 '((1 . "one") (2 . "two") (3 . "three")))
  ;; TESTFN with string-prefix-p for partial matching
  (assoc-default "hel" '(("hello" . 1) ("world" . 2))
                 (lambda (key elt) (string-prefix-p key elt)))
  ;; assoc-default with vector keys
  (assoc-default [1 2] '(([1 2] . "match") ([3 4] . "no"))))"#;
    let expect =
        expect_test::expect![[r#""OK (20 nil \"world\" nil 1 100 nil \"three\" nil \"match\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_assoc_default_pseudo_alist_and_tail_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU lisp/subr.el:assoc-default walks a pseudo-alist in Lisp: atom
    // elements compare directly with KEY and return DEFAULT when they match,
    // while malformed tails are only touched if no earlier element matches.
    let form = r#"
(list
 (assoc-default 'foo '(bar foo (foo . pair-value)) nil 'atom-default)
 (assoc-default 'foo '((bar . no) foo (foo . pair-value)) nil 'atom-default)
 (assoc-default 'foo '((foo . pair-value) foo) nil 'atom-default)
 (assoc-default "needle"
                '(("prefix-needle" . hit))
                (lambda (element key)
                  (string-match-p (symbol-name key) element))
                'miss)
 (condition-case err
     (assoc-default 'missing '((a . 1) loose . bad-tail) nil 'fallback)
   (error (list (car err) (cdr err))))
 (condition-case err
     (assoc-default 'loose '((a . 1) loose . bad-tail) nil 'fallback)
   (error (list (car err) (cdr err)))))
"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp \"needle\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// alist-get: with default, testfn, and remove
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_alist_alist_get_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic alist-get
  (alist-get 'b '((a . 1) (b . 2) (c . 3)))
  ;; Key not found returns nil
  (alist-get 'z '((a . 1) (b . 2)))
  ;; With DEFAULT
  (alist-get 'z '((a . 1) (b . 2)) 'my-default)
  ;; With DEFAULT when key IS found (default ignored)
  (alist-get 'a '((a . 1) (b . 2)) 'my-default)
  ;; With nil REMOVE and nil TESTFN (uses eq)
  (alist-get 'b '((a . 1) (b . 2) (c . 3)) nil nil)
  ;; With TESTFN equal (finds string keys)
  (alist-get "hello" '(("hello" . 1) ("world" . 2)) nil nil #'equal)
  ;; With TESTFN eq (does not find string keys)
  (alist-get "hello" '(("hello" . 1) ("world" . 2)) 'default nil #'eq)
  ;; alist-get on empty alist
  (alist-get 'a '() 'empty-default)
  ;; alist-get with numeric keys (eq works for fixnums)
  (alist-get 42 '((10 . "ten") (42 . "forty-two")))
  ;; alist-get with nil key
  (alist-get nil '((nil . "nil-value") (t . "t-value"))))"#;
    let expect = expect_test::expect![[
        r#""OK (2 nil my-default 1 2 1 default empty-default \"forty-two\" \"nil-value\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-alist: shallow copy semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_alist_copy_alist_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((original '((a . 1) (b . 2) (c . 3))))
  (let ((copied (copy-alist original)))
    (list
     ;; Copy has same content
     (equal original copied)
     ;; But is not the same object (different cons cells for the spine)
     (eq original copied)
     ;; Each pair in copy is a NEW cons cell
     (eq (car original) (car copied))
     ;; But the keys and values themselves are shared (shallow)
     (eq (caar original) (caar copied))
     (eq (cdar original) (cdar copied))
     ;; Mutating copy does not affect original
     (progn
       (setcdr (assq 'a copied) 999)
       (list (cdr (assq 'a original))
             (cdr (assq 'a copied))))
     ;; copy-alist of nil
     (copy-alist nil)
     ;; copy-alist of empty list
     (copy-alist '())
     ;; copy-alist with non-cons elements (they pass through unchanged)
     (copy-alist '(a (b . 2) c (d . 4)))
     ;; copy-alist preserves order
     (let ((al '((z . 26) (a . 1) (m . 13))))
       (equal al (copy-alist al))))))"#;
    let expect =
        expect_test::expect![r#""OK (t nil nil t t (1 999) nil nil (a (b . 2) c (d . 4)) t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-alist: deep value mutation isolation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_alist_copy_alist_deep_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((original '((a . (1 2 3)) (b . (4 5 6)))))
  (let ((copied (copy-alist original)))
    (list
     ;; Values are lists; copy-alist does NOT deep-copy values
     ;; The value lists are shared between original and copy
     (eq (cdr (assq 'a original)) (cdr (assq 'a copied)))
     ;; Replacing the cdr of a copied pair isolates from original
     (progn
       (setcdr (assq 'a copied) '(new value))
       (list (cdr (assq 'a original))
             (cdr (assq 'a copied))))
     ;; But mutating the value list in-place affects both (shared structure)
     (progn
       (setcar (cdr (assq 'b original)) 999)
       (list (cdr (assq 'b original))
             (cdr (assq 'b copied)))))))"#;
    let expect = expect_test::expect![r#""OK (t ((1 2 3) (new value)) ((999 5 6) (999 5 6)))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building multi-level alists and deep querying
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_alist_multi_level_build_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Build a 3-level alist structure: organization > department > person
  (fset 'neovm--deep-get
    (lambda (alist keys)
      (let ((current alist))
        (dolist (k keys)
          (setq current (cdr (assq k current))))
        current)))

  (fset 'neovm--deep-set
    (lambda (alist keys value)
      (if (= (length keys) 1)
          (let ((pair (assq (car keys) alist)))
            (if pair
                (progn (setcdr pair value) alist)
              (cons (cons (car keys) value) alist)))
        (let ((pair (assq (car keys) alist)))
          (if pair
              (progn
                (setcdr pair (funcall 'neovm--deep-set (cdr pair) (cdr keys) value))
                alist)
            (cons (cons (car keys)
                        (funcall 'neovm--deep-set nil (cdr keys) value))
                  alist))))))

  (unwind-protect
      (let ((org nil))
        ;; Build the org structure
        (setq org (funcall 'neovm--deep-set org '(eng backend alice) 'senior))
        (setq org (funcall 'neovm--deep-set org '(eng backend bob) 'junior))
        (setq org (funcall 'neovm--deep-set org '(eng frontend carol) 'mid))
        (setq org (funcall 'neovm--deep-set org '(sales domestic dave) 'lead))
        (setq org (funcall 'neovm--deep-set org '(sales intl eve) 'senior))

        (list
         ;; Query at various depths
         (funcall 'neovm--deep-get org '(eng backend alice))
         (funcall 'neovm--deep-get org '(eng backend bob))
         (funcall 'neovm--deep-get org '(sales intl eve))
         ;; Get entire sub-alist
         (funcall 'neovm--deep-get org '(eng backend))
         ;; Missing path returns nil
         (funcall 'neovm--deep-get org '(hr recruiting))
         ;; Update existing value
         (progn
           (setq org (funcall 'neovm--deep-set org '(eng backend bob) 'senior))
           (funcall 'neovm--deep-get org '(eng backend bob)))
         ;; List all departments
         (mapcar #'car org)))
    (fmakunbound 'neovm--deep-get)
    (fmakunbound 'neovm--deep-set)))"#;
    let expect = expect_test::expect![
        r#""OK (senior junior senior ((bob . senior) (alice . senior)) nil senior (sales eng))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// alist as ordered map: insertion-order iteration and key collection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_alist_ordered_map_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; alist-keys: extract all keys preserving insertion order
  (fset 'neovm--alist-keys (lambda (al) (mapcar #'car al)))
  ;; alist-values: extract all values
  (fset 'neovm--alist-values (lambda (al) (mapcar #'cdr al)))
  ;; alist-pairs->plist: convert alist to plist
  (fset 'neovm--alist-to-plist
    (lambda (al)
      (let ((result nil))
        (dolist (pair (reverse al))
          (setq result (cons (cdr pair) result))
          (setq result (cons (car pair) result)))
        result)))
  ;; plist->alist: convert plist to alist
  (fset 'neovm--plist-to-alist
    (lambda (pl)
      (let ((result nil))
        (while pl
          (push (cons (car pl) (cadr pl)) result)
          (setq pl (cddr pl)))
        (nreverse result))))
  ;; alist-filter: keep only pairs matching predicate
  (fset 'neovm--alist-filter
    (lambda (al pred)
      (let ((result nil))
        (dolist (pair al)
          (when (funcall pred (car pair) (cdr pair))
            (push pair result)))
        (nreverse result))))

  (unwind-protect
      (let ((al '((x . 10) (y . 20) (z . 30) (w . 40))))
        (list
         (funcall 'neovm--alist-keys al)
         (funcall 'neovm--alist-values al)
         (funcall 'neovm--alist-to-plist al)
         (funcall 'neovm--plist-to-alist '(a 1 b 2 c 3))
         ;; Round-trip: alist -> plist -> alist
         (equal al (funcall 'neovm--plist-to-alist
                            (funcall 'neovm--alist-to-plist al)))
         ;; Filter: keep only values > 15
         (funcall 'neovm--alist-filter al
                  (lambda (k v) (> v 15)))
         ;; Filter: keep only symbol keys starting with specific letter
         (funcall 'neovm--alist-filter '((apple . 1) (banana . 2) (avocado . 3) (cherry . 4))
                  (lambda (k v) (string-prefix-p "a" (symbol-name k))))))
    (fmakunbound 'neovm--alist-keys)
    (fmakunbound 'neovm--alist-values)
    (fmakunbound 'neovm--alist-to-plist)
    (fmakunbound 'neovm--plist-to-alist)
    (fmakunbound 'neovm--alist-filter)))"#;
    let expect = expect_test::expect![
        r#""OK ((x y z w) (10 20 30 40) (x 10 y 20 z 30 w 40) ((a . 1) (b . 2) (c . 3)) t ((y . 20) (z . 30) (w . 40)) ((apple . 1) (avocado . 3)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: alist-based LRU eviction with size-bounded cache
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_alist_lru_cache() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; LRU cache as alist: most-recently-used at front.
  ;; Cache state: (max-size . alist)

  (fset 'neovm--lru-make (lambda (max-size) (cons max-size nil)))

  ;; Get: move found key to front (MRU), return (cache . value) or (cache . nil)
  (fset 'neovm--lru-get
    (lambda (cache key)
      (let* ((max-sz (car cache))
             (al (cdr cache))
             (pair (assq key al)))
        (if pair
            ;; Move to front
            (let ((new-al (cons pair (delq pair al))))
              (cons (cons max-sz new-al) (cdr pair)))
          (cons cache nil)))))

  ;; Put: insert at front, evict LRU if over capacity
  (fset 'neovm--lru-put
    (lambda (cache key value)
      (let* ((max-sz (car cache))
             (al (cdr cache))
             (existing (assq key al))
             (new-al (if existing
                         (progn (setcdr existing value)
                                (cons existing (delq existing al)))
                       (cons (cons key value) al))))
        ;; Evict if over capacity
        (when (> (length new-al) max-sz)
          (setq new-al (butlast new-al)))
        (cons max-sz new-al))))

  ;; Keys in MRU order
  (fset 'neovm--lru-keys
    (lambda (cache)
      (mapcar #'car (cdr cache))))

  (unwind-protect
      (let ((c (funcall 'neovm--lru-make 3)))
        (setq c (funcall 'neovm--lru-put c 'a 1))
        (setq c (funcall 'neovm--lru-put c 'b 2))
        (setq c (funcall 'neovm--lru-put c 'c 3))
        (let ((keys-after-3 (funcall 'neovm--lru-keys c)))
          ;; Adding 4th evicts LRU (a)
          (setq c (funcall 'neovm--lru-put c 'd 4))
          (let ((keys-after-evict (funcall 'neovm--lru-keys c)))
            ;; Access 'b to make it MRU
            (let* ((get-result (funcall 'neovm--lru-get c 'b))
                   (got-val (cdr get-result)))
              (setq c (car get-result))
              (let ((keys-after-access (funcall 'neovm--lru-keys c)))
                ;; Add 'e, should evict LRU (c, since b was moved to front)
                (setq c (funcall 'neovm--lru-put c 'e 5))
                (list
                 keys-after-3        ;; (c b a)
                 keys-after-evict    ;; (d c b) -- a evicted
                 got-val             ;; 2
                 keys-after-access   ;; (b d c) -- b moved to front
                 (funcall 'neovm--lru-keys c) ;; (e b d) -- c evicted
                 ;; Update existing key
                 (progn
                   (setq c (funcall 'neovm--lru-put c 'd 999))
                   (list (funcall 'neovm--lru-keys c)
                         (cdr (funcall 'neovm--lru-get c 'd))))))))))
    (fmakunbound 'neovm--lru-make)
    (fmakunbound 'neovm--lru-get)
    (fmakunbound 'neovm--lru-put)
    (fmakunbound 'neovm--lru-keys)))"#;
    let expect = expect_test::expect![r#""OK ((c b a) (d c b) 2 (b d c) (e b d) ((d e b) 999))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
