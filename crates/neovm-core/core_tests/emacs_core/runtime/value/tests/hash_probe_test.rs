//! Core-owned evaluator cases from the historical hash-probe test module.
use super::super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;

#[test]
fn gethash_puthash_remhash_match_gnu() {
    let mut eval = crate::test_utils::new_context();
    // Expectation taken from GNU Emacs 31.0.90 --batch.
    let result = eval
        .eval_str(
            r#"(let ((h (make-hash-table :test 'equal)) (e (make-hash-table :test 'eql)) (q (make-hash-table)))
                 (puthash (list 1 2) 'x h) (puthash "s" 'y h) (puthash 1.5 'f h)
                 (puthash 1.5 'g e) (puthash 'sym 's q) (puthash 7 'seven q)
                 (format "%S" (list (gethash (list 1 2) h) (gethash "s" h) (gethash (list 1 2 3) h)
                                    (gethash 1.5 h) (gethash 1.5 e) (gethash 1.5 q) (gethash 'sym q)
                                    (gethash 7 q) (gethash "s" q)
                                    (progn (remhash (list 1 2) h) (gethash (list 1 2) h 'gone))
                                    (progn (puthash "s" 'z h) (gethash "s" h))
                                    (hash-table-count h))))"#,
        )
        .expect("hash table forms evaluate");
    assert_eq!(
        result.as_utf8_str(),
        Some("(x y nil f g nil s seven nil gone z 2)")
    );
}

/// With `symbols-with-pos-enabled`, a positioned key `eq`s its bare symbol,
/// which no bit comparison sees: the scan must decline.
#[test]
fn small_table_scans_decline_under_symbols_with_pos() {
    let mut ctx = crate::test_utils::new_context();
    let sym = Value::from_sym_id(intern("small-scan-positioned"));
    // SAFETY: test-only owner-thread heap setup through the non-swappable mutable accessor contract; the view is used in place for allocation/policy and is never swapped or moved across Worlds.
    let positioned = unsafe { ctx.tagged_heap_mut() }.alloc_symbol_with_pos(sym, Value::fixnum(3));
    for test in [HashTableTest::Eq, HashTableTest::Eql] {
        let mut storage = HashTableStorage::default();
        storage.insert(
            positioned.to_hash_key_swp(&test, true),
            positioned,
            Value::fixnum(7),
        );
        let by_key = storage.get(&sym.to_hash_key_swp(&test, true)).copied();
        assert_eq!(
            storage.lookup(sym, test, true).copied().map(|v| v.bits()),
            by_key.map(|v| v.bits()),
            "bare symbol against a positioned key under {test:?}"
        );
    }
}

/// The probe and the stored key must still agree after the sampling change --
/// including for keys built to defeat sampling.
///
/// Sampling reads the length, eight strided words and the tail, so the keys
/// that stress it are ones sharing a long prefix AND a long suffix and
/// differing only in between.  Those may COLLIDE, which is fine: `equal`
/// decides.  What must never happen is a key failing to find its own entry.
#[test]
fn sampled_string_hashing_still_finds_every_key() {
    let mut eval = crate::test_utils::new_context();
    // Only special forms and subrs: `crate::test_utils::new_context()` is bare, so `push`
    // and `dolist` (subr.el macros) are not available here.
    let program = r#"(let ((h (make-hash-table :test 'equal))
                          (pre (make-string 5000 ?a))
                          (suf (make-string 5000 ?b))
                          (keys nil)
                          (i 0)
                          (found 0)
                          (rest nil))
                      (while (< i 200)
                        (setq keys (cons (concat pre (number-to-string i) suf) keys))
                        (setq i (1+ i)))
                      (setq rest keys)
                      (while rest
                        (puthash (car rest) (length (car rest)) h)
                        (setq rest (cdr rest)))
                      (setq rest keys)
                      (while rest
                        (if (gethash (car rest) h) (setq found (1+ found)))
                        (setq rest (cdr rest)))
                      (list (hash-table-count h)
                            found
                            (gethash (concat pre "999999" suf) h)
                            (progn (setq rest keys)
                                   (while rest
                                     (puthash (car rest) 0 h)
                                     (setq rest (cdr rest)))
                                   (hash-table-count h))))"#;
    let observed = crate::emacs_core::format_eval_result(&eval.eval_str(program));
    assert_eq!(observed, "OK (200 200 nil 200)");
}
