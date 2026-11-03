//! B8 serial weak-table semantics, refreshed only from GNU 31.1.
use super::*;

// Includes the aliases and rooted-GC cases from the existing strict
// hash_table_weakness_key_value_gc_combo / weak_hash_lifecycle families.
pub(super) const SETUP_FORM: &str = r#"(list
  (hash-table-weakness (make-hash-table :weakness t))
  (hash-table-weakness (make-hash-table :weakness 'key))
  (hash-table-weakness (make-hash-table :weakness 'value))
  (hash-table-weakness (make-hash-table :weakness 'key-or-value))
  (hash-table-weakness (make-hash-table))
  (let ((h (make-hash-table :test 'equal :weakness t)))
    (puthash "key" 'val h)
    (list (hash-table-count h) (hash-table-weakness h)))
  (let ((h (make-hash-table :test 'eq :weakness t))
        (k (cons 1 2)))
    (puthash k 'weak-val h)
    (puthash 'stable-key 'stable-val h)
    (garbage-collect)
    (list (hash-table-count h) (gethash k h 'gone)
          (gethash 'stable-key h))))"#;

pub(super) const MATRIX_FORM: &str = r#"(let ((weaknesses '(key value key-or-value key-and-value))
      (answers nil))
  (while weaknesses
    (let ((h (make-hash-table :test 'eq :weakness (car weaknesses)))
          (roots nil) (state 0))
      (while (< state 4)
        (let ((key (cons state nil)) (value (cons state nil)))
          (puthash key value h)
          (if (= state 0) (setq roots (cons key roots)))
          (if (= state 1) (setq roots (cons value roots)))
          (if (= state 2) (setq roots (cons key (cons value roots)))))
        (setq state (+ state 1)))
      (garbage-collect)
      (garbage-collect)
      (let ((kept (hash-table-count h)))
        (setq roots nil)
        (garbage-collect)
        (garbage-collect)
        (setq answers (cons (list kept (hash-table-count h)) answers))))
    (setq weaknesses (cdr weaknesses)))
  (nreverse answers))"#;

pub(super) const CHAIN_FORM: &str = r#"(progn
  ;; The producer frame must end before GC: GNU conservatively protects
  ;; evaluated arguments and let initializers until their C frame returns.
  (fset 'a6-b8-oracle-produce-chain
    (lambda (h0 h1)
      (let ((a nil) (b nil) (c nil) (d nil) (e nil) (x nil) (y nil))
        (setq a (cons 1 nil) b (cons 2 nil) c (cons 3 nil)
              d (cons 4 nil) e (cons 5 nil) x (cons 6 nil) y (cons 7 nil))
        (puthash a b h0)
        (puthash c d h0)
        (puthash b c h1)
        (puthash d e h1)
        (puthash x y h0)
        (puthash y x h1)
        (setq a6-b8-oracle-root-a a)
        ;; Clear the lexical cells too: a suspended evaluator may retain the
        ;; completed call environment until its next serial root census.
        (setq a nil b nil c nil d nil e nil x nil y nil)
        nil)))
  (let ((h0 (make-hash-table :test 'eq :weakness 'key))
        (h1 (make-hash-table :test 'eq :weakness 'key)))
    (funcall 'a6-b8-oracle-produce-chain h0 h1)
    (garbage-collect)
    (garbage-collect)
    (let ((kept (list (hash-table-count h0) (hash-table-count h1))))
      (setq a6-b8-oracle-root-a nil)
      (garbage-collect)
      (garbage-collect)
      (list kept (hash-table-count h0) (hash-table-count h1)))))"#;

pub(super) const MEMO_FORM: &str = r#"(progn
  (define-hash-table-test 'a6-b8-oracle-collisions
    (lambda (a b) (eq a b)) (lambda (_key) 7))
  (let ((h (make-hash-table :test 'a6-b8-oracle-collisions
                            :weakness 'value :size 4))
        (roots nil) (batch 0) (answers nil))
    (while (< batch 2)
      (setq roots nil)
      (let ((index 0))
        (while (< index 24)
          (let ((key (+ (* batch 24) index)))
            (let ((value (cons key nil)))
              (puthash key value h)
              (if (= (% index 8) 0) (setq roots (cons value roots)))))
          (setq index (+ index 1))))
      (garbage-collect)
      (garbage-collect)
      (setq answers
            (cons (list (hash-table-count h)
                        (consp (gethash (* batch 24) h))
                        (gethash (+ (* batch 24) 1) h 'gone))
                  answers))
      (setq batch (+ batch 1)))
    (setq roots nil)
    (garbage-collect)
    (garbage-collect)
    (list (nreverse answers) (hash-table-count h))))"#;

fn check(form: &str, line: usize) {
    let mut context = Context::new();
    let value = context
        .eval_str(form)
        .unwrap_or_else(|error| panic!("B8 weak oracle eval: {error:?}"));
    assert_eq!(crate::emacs_core::print_value(&value), oracle_lines()[line]);
}

#[test]
fn a6_ht_b8_weak_setup_and_existing_family_twins_match_gnu_oracle() {
    check(SETUP_FORM, 39);
}

#[test]
fn a6_ht_b8_weak_root_matrix_matches_gnu_oracle() {
    check(MATRIX_FORM, 40);
}

#[test]
fn a6_ht_b8_weak_fixpoint_and_dead_cycle_match_gnu_oracle() {
    check(CHAIN_FORM, 41);
}

#[test]
fn a6_ht_b8_weak_custom_removal_and_reuse_match_gnu_oracle() {
    check(MEMO_FORM, 42);
}
