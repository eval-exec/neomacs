use super::*;
use crate::emacs_core::eval::Context;

#[test]
fn equal_compares_char_tables_by_gnu_pseudovector_slots() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"
(let* ((a (make-char-table 'neomacs-equal nil))
       (b (make-char-table 'neomacs-equal nil))
       (different (make-char-table 'neomacs-equal nil))
       (parent-a (make-char-table 'neomacs-equal-parent nil))
       (parent-b (make-char-table 'neomacs-equal-parent nil))
       (child-a (make-char-table 'neomacs-equal nil))
       (child-b (make-char-table 'neomacs-equal nil))
       (props-a (make-char-table 'neomacs-equal nil))
       (props-b (make-char-table 'neomacs-equal nil))
       (cycle-a (make-char-table 'neomacs-equal nil))
       (cycle-b (make-char-table 'neomacs-equal nil)))
  (set-char-table-range a #x1f600 'same)
  (set-char-table-range b #x1f600 'same)
  (set-char-table-range different #x1f600 'different)
  (set-char-table-range parent-a ?a 'parent)
  (set-char-table-range parent-b ?a 'parent)
  (set-char-table-parent child-a parent-a)
  (set-char-table-parent child-b parent-b)
  (set-char-table-range props-a ?x (propertize "v" 'face 'bold))
  (set-char-table-range props-b ?x (propertize "v" 'face 'italic))
  (set-char-table-range cycle-a nil cycle-a)
  (set-char-table-range cycle-b nil cycle-b)
  (list
   (equal (make-char-table 'neomacs-equal nil)
          (make-char-table 'neomacs-equal nil))
   (equal (make-char-table 'one nil)
          (make-char-table 'two nil))
   (equal a b)
   (equal a different)
   (equal child-a child-b)
   (equal props-a props-b)
   (equal-including-properties props-a props-b)
   (equal cycle-a cycle-b)))"#,
        )
        .expect("char-table equality probe should evaluate");

    assert_eq!(
        result,
        Value::list(vec![
            Value::T,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::T,
            Value::T,
            Value::NIL,
            Value::T,
        ])
    );
}


#[test]
fn equal_char_tables_share_hashes_and_equal_hash_table_keys() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"
(let ((left (make-char-table 'neomacs-equal-hash nil))
      (right (make-char-table 'neomacs-equal-hash nil))
      (table (make-hash-table :test 'equal)))
  (set-char-table-range left #x1f600 'same)
  (set-char-table-range right #x1f600 'same)
  (puthash left 'found table)
  (list
   (equal left right)
   (= (sxhash-equal left) (sxhash-equal right))
   (gethash right table 'missing)))"#,
        )
        .expect("char-table hash contract probe should evaluate");

    assert_eq!(
        result,
        Value::list(vec![Value::T, Value::T, Value::symbol("found")])
    );
}


#[test]
fn equal_on_circular_structures_matches_gnu_cycle_semantics() {
    // GNU 31.0.90 `equal` (internal_equal_1 / internal_equal_cycle) detects cycles
    // and returns t/nil instead of signaling `circular-list`.  Regression for the
    // cons-loop tortoise-hare that used to error on circular structures; goes
    // through the lisp `equal` (try_equal_value_inner), not the bool equal_value.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::Context::new();
    // (form, expected truthy)
    let cases: [(&str, bool); 5] = [
        // two separate self-circular lists, equal cars -> t
        (
            "(let ((a (list 'x)) (b (list 'x))) (setcdr a a) (setcdr b b) (equal a b))",
            true,
        ),
        // self-circular lists, differing cars -> nil
        (
            "(let ((a (list 'x)) (c (list 'y))) (setcdr a a) (setcdr c c) (equal a c))",
            false,
        ),
        // two separate self-circular vectors, equal -> t
        (
            "(let ((v1 (vector 'x nil)) (v2 (vector 'x nil))) (aset v1 1 v1) (aset v2 1 v2) (equal v1 v2))",
            true,
        ),
        // self-circular vectors, differing -> nil
        (
            "(let ((v1 (vector 'x nil)) (v2 (vector 'y nil))) (aset v1 1 v1) (aset v2 1 v2) (equal v1 v2))",
            false,
        ),
        // circular left vs finite right -> nil (right terminates first)
        (
            "(let ((a (list 'x))) (setcdr a a) (equal a (list 'x 'x 'x)))",
            false,
        ),
    ];
    for (src, truthy) in cases {
        let got = ev
            .eval_str(src)
            .ok()
            .unwrap_or_else(|| panic!("`equal` signaled instead of returning for: {src}"));
        let is_truthy = got.bits() != Value::NIL.bits();
        assert_eq!(is_truthy, truthy, "wrong `equal` result for: {src}");
    }
}


/// GNU `internal_equal_1` (src/fns.c:2984-2998 in emacs-31.1, :2987-3001 on
/// master) compares a `PVEC_CLOSURE` exactly like a vector: the size check
/// first (`ASIZE`, so a five-slot closure never equals a four-slot one), then
/// every slot element-wise -- arglist, bytecode string, constants vector,
/// max depth, doc, interactive spec, and any extras.  `sxhash_obj`
/// (:5525-5536) hashes the same slots through `sxhash_vector`, so two `equal`
/// closures land in the same `equal` hash-table bucket.
///
/// The Lisp-visible consequence lsp-mode depends on: it removes the request
/// cancel closure it put on the global `post-command-hook` by rebuilding an
/// `equal` closure and calling `remove-hook` -> `delete`.  When byte-code
/// objects compare only by identity, that hook is never removed, fires on
/// every later command, and keeps sending `$/cancelRequest` to a server that
/// has since been shut down ("Sending to process failed ... not running").
///
/// Expected values are GNU Emacs 32's for this exact probe.
#[test]
fn equal_compares_byte_code_functions_element_wise_like_gnu_closures() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"
(let* ((a (make-byte-code 257 "\300\207" [42] 2))
       (b (make-byte-code 257 "\300\207" [42] 2))
       (c (make-byte-code 257 "\300\207" [43] 2))
       (d (make-byte-code 257 "\300\207" [42] 3))
       (e (make-byte-code 257 "\300\207" [42] 2 "doc"))
       (f (make-byte-code 257 "\300\207" [42] 2 "doc"))
       (g (make-byte-code 257 "\300\207" [42] 2 nil))
       (h (make-byte-code 257 "\300\207" [42] 2 nil (list 'interactive "p")))
       (i (make-byte-code 257 "\300\207" [42] 2 nil (list 'interactive "p")))
       (j (make-byte-code 257 "\301\207" [42] 2))
       (k (make-byte-code '(x) "\300\207" [42] 2))
       (proto (make-byte-code 257 "\300\207" [placeholder] 2))
       (k1 (make-closure proto 'w))
       (k2 (make-closure proto 'w))
       (k3 (make-closure proto 'z))
       (table (make-hash-table :test 'equal))
       (v (vector 257 "\300\207" [42] 2)))
  (puthash a 'found table)
  (list (equal a b)                       ; same slots, distinct objects
        (equal a c)                       ; constants differ
        (equal a d)                       ; max depth differs
        (equal a e)                       ; doc slot differs
        (equal e f)                       ; same doc
        (equal a g)                       ; GNU ASIZE: 4 slots vs 5 slots
        (equal h i)                       ; same interactive spec
        (equal a j)                       ; bytecode differs
        (equal a k)                       ; arglist differs
        (equal k1 k2)                     ; make-closure: same captures
        (equal k1 k3)                     ; make-closure: different capture
        (eq k1 k2)
        (equal a v)                       ; a plain vector is not a closure
        (equal (list a 1) (list b 1))
        (length (delete b (list a 1)))    ; what remove-hook does
        (eq (car (member b (list 1 a))) a)
        (= (sxhash-equal a) (sxhash-equal b))
        (= (sxhash-equal k1) (sxhash-equal k2))
        (gethash b table)
        (gethash k1 (let ((tb (make-hash-table :test 'equal)))
                      (puthash k2 'closure-found tb)
                      tb))))"#,
        )
        .expect("byte-code equality probe should evaluate");

    assert_eq!(
        list_to_vec(&result).expect("probe returns a list"),
        vec![
            Value::T,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::fixnum(1),
            Value::T,
            Value::T,
            Value::T,
            Value::symbol("found"),
            Value::symbol("closure-found"),
        ]
    );
}


#[path = "hash_probe_test.rs"]
mod hash_probe;
