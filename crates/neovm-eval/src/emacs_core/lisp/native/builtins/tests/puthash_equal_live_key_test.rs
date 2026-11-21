//! `puthash` into an `equal` table files a cons, vector, record or string key
//! by the key object, without materializing a copy of it. What Lisp observes
//! must not change: the key `maphash' hands back is the object that was put,
//! `copy-hash-table', `remhash', leaf keys, a key mutated after insertion and
//! user-defined tests all answer as before -- with and without GC stress.

use crate::emacs_core::{Context, format_eval_result};

#[test]
fn puthash_equal_structural_keys_are_observably_unchanged() {
    crate::test_utils::init_test_tracing();
    let cases: &[(&str, &str)] = &[
        // `maphash' yields the very key object; an equal copy finds it.
        (
            "(let ((h (make-hash-table :test 'equal)) (k (list 1 \"two\" 3.5)) got)
               (puthash k 'v h)
               (maphash (lambda (key _) (setq got key)) h)
               (list (eq got k) (gethash (list 1 \"two\" 3.5) h) (hash-table-count h)))",
            "(t v 1)",
        ),
        // A large vector key, through `copy-hash-table'.
        (
            "(let ((h (make-hash-table :test 'equal)) (k (make-vector 1000 0)) got)
               (aset k 0 7)
               (puthash k 'v h)
               (let ((c (copy-hash-table h)))
                 (maphash (lambda (key _) (setq got key)) c)
                 (list (gethash (copy-sequence k) c) (eq got k) (hash-table-count c))))",
            "(v t 1)",
        ),
        // An equal copy replaces the value and keeps the first key object.
        (
            "(let ((h (make-hash-table :test 'equal)) (k (list 'a 'b)) got)
               (puthash k 1 h)
               (puthash (list 'a 'b) 2 h)
               (maphash (lambda (key _) (setq got key)) h)
               (list (gethash k h) (eq got k) (hash-table-count h)))",
            "(2 t 1)",
        ),
        // `remhash' by an equal copy.
        (
            "(let ((h (make-hash-table :test 'equal)))
               (puthash \"abc\" 1 h)
               (puthash (cons 'a 'b) 2 h)
               (puthash (vector 1 2) 3 h)
               (remhash (copy-sequence \"abc\") h)
               (remhash (cons 'a 'b) h)
               (list (hash-table-count h) (gethash \"abc\" h) (gethash (cons 'a 'b) h)
                     (gethash (vector 1 2) h)))",
            "(1 nil nil 3)",
        ),
        // A string with properties: `equal' ignores them, the key keeps them.
        (
            "(let ((h (make-hash-table :test 'equal)) (k (propertize \"key\" 'face 'bold)) got)
               (puthash k 1 h)
               (maphash (lambda (key _) (setq got key)) h)
               (list (gethash \"key\" h) (eq got k) (get-text-property 0 'face got)))",
            "(1 t bold)",
        ),
        // Leaf keys: float, bignum, bool-vector.
        (
            "(let ((h (make-hash-table :test 'equal)))
               (puthash 1.5 'f h)
               (puthash (expt 2 70) 'b h)
               (puthash (make-bool-vector 3 t) 'bv h)
               (list (gethash 1.5 h) (gethash (expt 2 70) h) (gethash (make-bool-vector 3 t) h)
                     (hash-table-count h)))",
            "(f b bv 3)",
        ),
        // A list key mutated after insertion: neither the object (it now
        // hashes elsewhere) nor an equal copy of either shape finds it, and
        // the new shape adds an entry.
        (
            "(let ((h (make-hash-table :test 'equal)) (k (list 1 2 3)))
               (puthash k 'v h)
               (setcar k 9)
               (list (gethash k h) (gethash (list 1 2 3) h) (gethash (list 9 2 3) h)
                     (progn (puthash (list 9 2 3) 'w h) (hash-table-count h))))",
            "(nil nil nil 2)",
        ),
        // A long list key.
        (
            "(let ((h (make-hash-table :test 'equal)))
               (puthash (make-list 10000 'x) 'v h)
               (gethash (make-list 10000 'x) h))",
            "v",
        ),
        // A user-defined test.
        (
            "(progn
               (define-hash-table-test 'np-puthash-ci
                 (lambda (a b) (string-equal (upcase a) (upcase b)))
                 (lambda (k) (sxhash-equal (upcase k))))
               (let ((h (make-hash-table :test 'np-puthash-ci)))
                 (puthash \"Abc\" 1 h)
                 (puthash \"aBC\" 2 h)
                 (list (gethash \"ABC\" h) (hash-table-count h))))",
            "(2 1)",
        ),
    ];
    for gc_stress in [false, true] {
        let mut ev = Context::new(crate::test_utils::unit_runtime());
        ev.gc_stress = gc_stress;
        for (form, want) in cases {
            let got = format_eval_result(&ev.eval_str(form));
            assert_eq!(got, format!("OK {want}"), "{form} (gc_stress {gc_stress})");
        }
    }
}
