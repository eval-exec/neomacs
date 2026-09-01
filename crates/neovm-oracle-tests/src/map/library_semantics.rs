//! Oracle parity tests for GNU `emacs-lisp/map.el` generic map semantics.
//!
//! `map.el` dispatches over alists, plists, hash tables, and arrays.  These
//! tests focus on exact lookup, conversion, pcase binding, and mutation
//! behavior exposed by the public API.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_map_lookup_contains_and_nested_elt() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'map)
  (let ((ht (make-hash-table :test 'equal)))
    (puthash "name" "gnu" ht)
    (puthash "nested" '(:answer 42) ht)
    (list
     (map-elt '((a . 1) ("b" . 2)) "b")
     (map-elt '(:a 1 :b nil) :b 'missing)
     (map-contains-key '(:a 1 :b nil) :b)
     (map-elt [zero one two] 1 'missing)
     (map-contains-key [zero one two] 3)
     (map-elt ht "name")
     (map-nested-elt ht '("nested" :answer))
     (map-nested-elt ht '("nested" :missing) 'fallback))))
"#;

    let expect = expect_test::expect![[r#""OK (2 nil (:b nil) one nil \"gnu\" 42 fallback)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_map_keys_values_pairs_and_apply_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'map)
  (list
   (map-keys '((a . 1) (b . 2) (a . 3)))
   (map-values '(:a 1 :b 2 :c 3))
   (map-pairs [x y z])
   (map-apply (lambda (k v) (list k v)) '(:a 1 :b 2))
   (map-filter (lambda (_k v) (> v 1)) '((a . 1) (b . 2) (c . 3)))
   (map-remove (lambda (k _v) (eq k 'b)) '((a . 1) (b . 2) (c . 3)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((a b a) (1 2 3) ((0 . x) (1 . y) (2 . z)) ((:a 1) (:b 2)) ((b . 2) (c . 3)) ((a . 1) (c . 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_map_into_merge_and_merge_with() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'map)
  (let* ((alist '((a . 1) (b . 2)))
         (plist '(:b 20 :c 30))
         (merged-alist (map-merge 'alist alist '((a . 10) (d . 4))))
         (merged-plist (map-merge 'plist plist '(:b 5 :d 6)))
         (summed (map-merge-with 'alist #'+ '((a . 1) (b . 2))
                                 '((a . 10) (c . 30)))))
    (list
     (map-into alist 'plist)
     (map-into plist 'alist)
     (let ((ht (map-into alist '(hash-table :test equal))))
       (list (hash-table-test ht) (gethash 'a ht) (gethash 'b ht)))
     merged-alist
     merged-plist
     summed)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((a 1 b 2) ((:b . 20) (:c . 30)) (equal 1 2) ((a . 10) (b . 2) (d . 4)) (:b 5 :c 30 :d 6) ((a . 11) (b . 2) (c . 30)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_map_let_and_mutation_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'map)
  (let ((alist '((a . 1) (b . 2)))
        (plist '(:a 1 :b 2))
        (vec [a b c]))
    (list
     (map-let (a (b-val b) (missing fallback 99)) alist
       (list a b-val fallback))
     (map-let (:a :b (:missing missing 'fallback)) plist
       (list a b missing))
     (let ((copy (map-insert alist 'c 3)))
       (list alist copy))
     (condition-case err
         (map-put! nil 'a 1)
       (error (car err)))
     (progn
       (map-put! plist :b 22)
       plist)
     (progn
       (map-put! vec 1 'B)
       vec))))
"#;

    let expect = expect_test::expect![[r#""ERR (void-variable b-val)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_map_nested_and_inplace_edge_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'map)
  (let ((alist (list (cons 'a 1)))
        (plist (list :a 1 :b nil))
        (vec [a b c]))
    (list
     ;; GNU map-nested-elt uses `or', so a found nil value returns DEFAULT.
     (map-nested-elt '(:a (:b nil)) '(:a :b) 'fallback)
     (map-nested-elt '(:a (:b 0)) '(:a :b) 'fallback)
     ;; Inserting a new alist key cannot mutate the original cons cell and
     ;; therefore signals map-not-inplace.
     (condition-case err
         (map-put! alist 'b 2)
       (error (list (car err) (cadr err))))
     alist
     ;; Existing alist keys and plists are updated in place.
     (let ((alist2 (list (cons 'a 1))))
       (list (map-put! alist2 'a 9) alist2))
     (let ((p (list :a 1)))
       (list (map-put! p :b 2) p))
     ;; List deletion return values may differ from the original list object.
     (let ((p (list :a 1 :b 2)))
       (list (map-delete p :a) p))
     (let ((a (list (cons 'a 1) (cons 'b 2))))
       (list (map-delete a 'b) a))
     ;; Array lookup outside bounds returns DEFAULT through map-contains-key.
     (condition-case err
         (map-elt vec 9 'missing)
       (error (list (car err) (cadr err)))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (fallback 0 (map-not-inplace ((a . 1))) ((a . 1)) (9 ((a . 9))) (2 (:a 1 :b 2)) ((:b 2) (:a 1 :b 2)) (((a . 1)) ((a . 1))) missing)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_map_predicate_iteration_and_copy_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'map)
  ;; GNU lisp/emacs-lisp/map.el defines these public functions as generic
  ;; dispatch over lists, hash tables, and arrays.  This case locks down the
  ;; details that are easy to miss: plist membership returns a tail object,
  ;; alist duplicate keys count in map-length, map-some short-circuits, and
  ;; map-copy uses copy-alist for alists but copy-sequence for plists/arrays.
  (let ((ht (make-hash-table :test 'equal)))
    (puthash "nil" nil ht)
    (puthash "two" 2 ht)
    (list
     ;; Presence is distinct from value nil.
     (map-contains-key '((a . nil)) 'a)
     (map-contains-key '((a . nil)) 'missing)
     (map-contains-key '(:a nil :b 2) :a)
     (map-contains-key ht "nil")
     ;; Plists default to eq, but the deprecated TESTFN still affects list
     ;; maps in GNU's compatibility shims.
     (let ((plist (list (copy-sequence "k") 1)))
       (list (map-contains-key plist "k")
             (map-contains-key plist "k" #'equal)
             (map-elt plist "k" 'missing)
             (map-elt plist "k" 'missing #'equal)))
     ;; Length and emptiness per map type.
     (list (map-empty-p nil)
           (map-empty-p [])
           (map-empty-p ht)
           (map-length '((a . 1) (a . 2)))
           (map-length '(:a 1 :b 2))
           (map-length [x y z])
           (map-length ht))
     ;; Iteration helpers preserve GNU's map-do / map-apply ordering for
     ;; sequence maps.
     (map-keys-apply #'identity '(:a 1 :b 2))
     (map-values-apply (lambda (v) (and v (* v 10))) [1 nil 3])
     (let ((seen '()))
       (list (map-some (lambda (k v)
                         (push k seen)
                         (and (= v 2) (list k v)))
                       '((a . 1) (b . 2) (c . 3)))
             (nreverse seen)))
     (let ((seen '()))
       (list (map-every-p (lambda (k v)
                            (push k seen)
                            (< v 3))
                          '((a . 1) (b . 2) (c . 3)))
             (nreverse seen)))
     ;; Copy behavior is observable through later mutation.
     (let* ((alist (list (cons 'a (cons 'inner 1))))
            (copy (map-copy alist)))
       (setcdr (assq 'a copy) 'changed)
       (list alist copy))
     (let* ((plist (list :a 1 :b 2))
            (copy (map-copy plist)))
       (setcar copy :changed)
       (list plist copy))
     (let* ((vec [a b])
            (copy (map-copy vec)))
       (aset copy 0 'changed)
       (list vec copy))
     (condition-case err
         (map-put! [a] 2 'x)
       (error (list (car err) (cadr err)))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t nil (:a nil :b 2) t (nil (\"k\" 1) missing 1) (t t nil 2 2 3 2) (:a :b) (10 nil 30) ((b 2) (a b)) (nil (a b c)) (((a inner . 1)) ((a . changed))) ((:a 1 :b 2) (:changed 1 :b 2)) ([a b] [changed b]) (args-out-of-range [a]))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
