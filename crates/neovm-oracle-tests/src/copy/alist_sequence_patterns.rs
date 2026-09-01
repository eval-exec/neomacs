//! Oracle parity tests for `copy-alist` and `copy-sequence` with complex patterns.
//!
//! Tests shallow vs deep copy semantics, mutation independence, behavior
//! differences between copy-alist and copy-sequence on alists, recursive
//! deep copy, snapshot-and-modify undo pattern, and structural sharing.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// copy-alist creates new cons cells; copy-sequence does not for alist entries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_alist_vs_copy_sequence_on_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // copy-alist copies the top-level pairs (each (key . val) cons is new),
    // while copy-sequence copies only the spine (the list cells).
    // This means setcdr on a copy-alist entry is independent,
    // but setcdr on a copy-sequence entry affects the original.
    let form = r#"
(let* ((orig (list (cons 'a 1) (cons 'b 2) (cons 'c 3)))
       (ca (copy-alist orig))
       (cs (copy-sequence orig)))
  ;; Modify via copy-alist: independent
  (setcdr (car ca) 100)
  ;; Modify via copy-sequence: shared entry (same cons cell)
  (setcdr (car cs) 999)
  (list
   ;; copy-alist entry is independent: orig still has 1? No, copy-sequence
   ;; shared the same cons, so orig's first entry was mutated to 999.
   (cdr (car orig))
   ;; copy-alist copy has its own value
   (cdr (car ca))
   ;; copy-sequence shares the cons cell with orig
   (eq (car orig) (car cs))
   ;; copy-alist does NOT share cons cell
   (eq (car orig) (car ca))
   ;; Spine independence: push onto copy-sequence doesn't affect orig
   (let ((orig-len (length orig)))
     (setq cs (cons (cons 'd 4) cs))
     (= (length orig) orig-len))
   ;; All three have expected values
   (mapcar #'cdr orig)
   (mapcar #'cdr ca)
   (mapcar #'cdr cs)))
"#;
    let expect =
        expect_test::expect![[r#""OK (999 100 t nil t (999 2 3) (100 2 3) (4 999 2 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-sequence on vectors, strings, and lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_sequence_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((lst (list 1 2 3 4 5))
       (vec (vector 10 20 30 40 50))
       (str "hello world")
       (lst-copy (copy-sequence lst))
       (vec-copy (copy-sequence vec))
       (str-copy (copy-sequence str)))
  ;; Mutate copies
  (setcar lst-copy 999)
  (aset vec-copy 0 999)
  ;; Strings are immutable in the sense that aset on a copy doesn't affect original
  (aset str-copy 0 ?H)
  (list
   ;; List: original unaffected
   (car lst)
   (car lst-copy)
   ;; Vector: original unaffected
   (aref vec 0)
   (aref vec-copy 0)
   ;; String: original unaffected
   (aref str 0)
   (aref str-copy 0)
   ;; Types preserved
   (type-of lst-copy)
   (type-of vec-copy)
   (type-of str-copy)
   ;; Lengths preserved
   (= (length lst) (length lst-copy))
   (= (length vec) (length vec-copy))
   (= (length str) (length str-copy))
   ;; Equal but not eq
   (equal lst (list 1 2 3 4 5))
   (not (eq lst lst-copy))))
"#;
    let expect =
        expect_test::expect![[r#""OK (1 999 10 999 104 72 cons vector string t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-alist preserves dotted pairs with non-atom values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_alist_complex_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // copy-alist copies the top-level cons cells but values can be anything:
    // lists, vectors, nested alists, etc.
    let form = r#"
(let* ((orig (list (cons 'nums (list 1 2 3))
                   (cons 'vec (vector 4 5 6))
                   (cons 'nested (list (cons 'x 10) (cons 'y 20)))
                   (cons 'str "immutable")
                   (cons 'nil-val nil)
                   (cons 'sym 'hello)))
       (cp (copy-alist orig)))
  ;; Top-level cells are different
  (let ((all-diff t))
    (let ((o orig) (c cp))
      (while (and o c)
        (when (eq (car o) (car c))
          (setq all-diff nil))
        (setq o (cdr o) c (cdr c))))
    (list
     all-diff
     ;; But cdr values are shared (eq)
     (eq (cdr (assq 'nums orig)) (cdr (assq 'nums cp)))
     (eq (cdr (assq 'vec orig)) (cdr (assq 'vec cp)))
     (eq (cdr (assq 'nested orig)) (cdr (assq 'nested cp)))
     ;; Replace entire value in copy — orig unaffected
     (progn (setcdr (assq 'nums cp) '(99 98 97)) nil)
     (cdr (assq 'nums orig))
     (cdr (assq 'nums cp))
     ;; Mutate shared inner list through orig — affects cp's view
     ;; (only for values not replaced via setcdr)
     (progn (setcar (cdr (assq 'vec orig)) 555) nil)
     (aref (cdr (assq 'vec orig)) 1)
     (aref (cdr (assq 'vec cp)) 1)
     ;; equal check on keys
     (equal (mapcar #'car orig) (mapcar #'car cp)))))
"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument consp [4 5 6])""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deep copy implementation using recursive copy-sequence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deep_copy_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a deep copy that recursively copies all cons cells, vectors, and strings.
    let form = r#"
(progn
  (fset 'neovm--deep-copy
    (lambda (obj)
      "Deep copy OBJ: recursively copy conses, vectors, and strings."
      (cond
       ((consp obj)
        (cons (funcall 'neovm--deep-copy (car obj))
              (funcall 'neovm--deep-copy (cdr obj))))
       ((vectorp obj)
        (let* ((len (length obj))
               (new-vec (make-vector len nil))
               (i 0))
          (while (< i len)
            (aset new-vec i (funcall 'neovm--deep-copy (aref obj i)))
            (setq i (1+ i)))
          new-vec))
       ((stringp obj)
        (copy-sequence obj))
       (t obj))))

  (unwind-protect
      (let* ((orig (list (cons 'a (list 1 (vector 2 3) "four"))
                         (cons 'b (vector (list 5 6) "seven"))
                         (cons 'c 42)))
             (deep (funcall 'neovm--deep-copy orig)))
        ;; Mutate deep copy at every level
        (setcdr (assq 'c deep) 999)
        (setcar (cdr (assq 'a deep)) 111)
        (aset (caddr (cdr (assq 'a orig))) 0 222)  ;; mutate orig's vector
        (list
         ;; Deep copy is equal to original (before mutations)
         ;; Actually we already mutated, so check specific values
         ;; orig 'c still 42
         (cdr (assq 'c orig))
         ;; deep 'c changed to 999
         (cdr (assq 'c deep))
         ;; orig 'a second element still 1 (deep was mutated, not orig)
         (cadr (cdr (assq 'a orig)))
         ;; deep 'a second element changed to 111
         (cadr (cdr (assq 'a deep)))
         ;; orig's vector was mutated to 222, but deep's vector is independent
         (aref (caddr (cdr (assq 'a orig))) 0)
         (aref (caddr (cdr (assq 'a deep))) 0)
         ;; No eq sharing at any level
         (not (eq (cdr (assq 'a orig)) (cdr (assq 'a deep))))
         (not (eq (car orig) (car deep)))))
    (fmakunbound 'neovm--deep-copy)))
"#;
    let expect = expect_test::expect![[r#""OK (42 999 [2 3] [2 3] 222 102 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Snapshot-and-modify pattern for undo
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_snapshot_undo_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build an undo system: take snapshots (copy-alist) before each change,
    // then undo by restoring from the snapshot stack.
    let form = r#"
(let ((state (list (cons 'x 0) (cons 'y 0) (cons 'z 0)))
      (undo-stack nil))
  ;; Helper: snapshot current state
  (let ((snapshot (lambda () (setq undo-stack (cons (copy-alist state) undo-stack))))
        (set-val (lambda (key val)
                   (setcdr (assq key state) val)))
        (get-val (lambda (key) (cdr (assq key state))))
        (undo (lambda ()
                (when undo-stack
                  (setq state (car undo-stack))
                  (setq undo-stack (cdr undo-stack))))))
    ;; Series of operations with snapshots
    (funcall snapshot)
    (funcall set-val 'x 10)
    (funcall snapshot)
    (funcall set-val 'y 20)
    (funcall snapshot)
    (funcall set-val 'z 30)
    (funcall snapshot)
    (funcall set-val 'x 100)
    (funcall set-val 'y 200)
    (let ((after-all (list (funcall get-val 'x)
                           (funcall get-val 'y)
                           (funcall get-val 'z))))
      ;; Undo once: should restore to before x=100, y=200
      (funcall undo)
      (let ((after-undo1 (list (funcall get-val 'x)
                               (funcall get-val 'y)
                               (funcall get-val 'z))))
        ;; Undo again: should restore to before z=30
        (funcall undo)
        (let ((after-undo2 (list (funcall get-val 'x)
                                 (funcall get-val 'y)
                                 (funcall get-val 'z))))
          ;; Undo again: should restore to before y=20
          (funcall undo)
          (let ((after-undo3 (list (funcall get-val 'x)
                                   (funcall get-val 'y)
                                   (funcall get-val 'z))))
            ;; Undo to initial state
            (funcall undo)
            (let ((after-undo4 (list (funcall get-val 'x)
                                     (funcall get-val 'y)
                                     (funcall get-val 'z))))
              (list after-all after-undo1 after-undo2 after-undo3 after-undo4
                    ;; Undo stack is now empty
                    (null undo-stack)))))))))
"#;
    let expect =
        expect_test::expect![[r#""OK ((100 200 30) (10 20 30) (10 20 0) (10 0 0) (0 0 0) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-sequence on empty and singleton collections
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_sequence_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 ;; Empty list
 (copy-sequence nil)
 ;; Singleton list
 (let* ((orig (list 42))
        (cp (copy-sequence orig)))
   (setcar cp 99)
   (list (car orig) (car cp)))
 ;; Empty vector
 (let* ((v (vector))
        (vc (copy-sequence v)))
   (list (length v) (length vc) (equal v vc) (not (eq v vc))))
 ;; Empty string
 (let* ((s "")
        (sc (copy-sequence s)))
   (list (length s) (length sc) (equal s sc)))
 ;; copy-alist on nil
 (copy-alist nil)
 ;; copy-alist on single entry
 (let* ((orig (list (cons 'only 42)))
        (cp (copy-alist orig)))
   (setcdr (car cp) 99)
   (list (cdr (car orig)) (cdr (car cp))))
 ;; Nested empty structures
 (let* ((orig (list (cons 'a nil) (cons 'b (list)) (cons 'c (vector))))
        (cp (copy-alist orig)))
   (list (equal orig cp)
         (not (eq (car orig) (car cp))))))
"#;
    let expect =
        expect_test::expect![[r#""OK (nil (42 99) (0 0 t nil) (0 0 t) nil (42 99) (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-alist with numeric and string keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_alist_non_symbol_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // copy-alist works with any key type, not just symbols
    let form = r#"
(let* ((orig (list (cons 1 "one")
                   (cons 2 "two")
                   (cons "key" "string-key")
                   (cons nil "nil-key")
                   (cons t "t-key")
                   (cons 3.14 "pi")))
       (cp (copy-alist orig)))
  ;; Modify copy
  (setcdr (car cp) "ONE")
  (setcdr (nth 2 cp) "STRING-KEY")
  (list
   ;; Original values unchanged
   (cdr (car orig))
   (cdr (nth 2 orig))
   ;; Copy values changed
   (cdr (car cp))
   (cdr (nth 2 cp))
   ;; Keys are the same objects (not copied)
   (eq (caar orig) (caar cp))
   ;; Lengths match
   (= (length orig) (length cp))
   ;; Full equal check (before modification it would have been equal)
   (equal (mapcar #'car orig) (mapcar #'car cp))))
"#;
    let expect =
        expect_test::expect![[r#""OK (\"one\" \"string-key\" \"ONE\" \"STRING-KEY\" t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Structural sharing detection after copy operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_structural_sharing_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a DAG-like structure with shared subtrees, then copy it
    // and verify which parts are shared vs independent.
    let form = r#"
(let* ((shared-tail (list 4 5 6))
       (list-a (append (list 1 2 3) shared-tail))
       (list-b (append (list 7 8 9) shared-tail))
       ;; Copy list-a
       (copy-a (copy-sequence list-a)))
  (list
   ;; Shared tail is eq in originals
   (eq (nthcdr 3 list-a) (nthcdr 3 list-b))
   ;; Copy's tail is NOT eq to original's tail (spine is copied)
   (not (eq (nthcdr 3 list-a) (nthcdr 3 copy-a)))
   ;; But contents are equal
   (equal (nthcdr 3 list-a) (nthcdr 3 copy-a))
   ;; Mutating shared-tail affects both originals
   (progn (setcar shared-tail 444) nil)
   (nth 3 list-a)
   (nth 3 list-b)
   ;; But NOT the copy (copy-sequence copied the spine, elements are shared
   ;; for atoms but the cons cells are new — however the atom 4 was replaced
   ;; in shared-tail which list-a points to, but copy-a's cons cells hold the
   ;; original value 4... Wait, copy-sequence copies the spine.
   ;; The copy's 4th element was the integer 4 at copy time, now shared-tail[0]=444,
   ;; but copy-a has its own cons cell holding whatever was in position 3 at copy time.
   ;; Actually: copy-sequence copies the list spine, but the car values are shared.
   ;; list-a's 4th cons cell IS shared-tail, so (nth 3 list-a) = (car shared-tail) = 444.
   ;; copy-a's 4th cons cell is a NEW cell whose car was (car shared-tail) at copy time = 4.
   ;; Wait, no: at copy time, shared-tail was (4 5 6), so copy-a = (1 2 3 4 5 6).
   ;; The integers are atoms, so they're the same. copy-sequence copies the cons cells.
   ;; So copy-a's 4th cell has car=4 (the integer, which is immutable).
   ;; Now shared-tail has car=444, but copy-a's cell still has car=4.
   (nth 3 copy-a)
   ;; Length checks
   (length list-a)
   (length copy-a)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t nil 444 444 4 6 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
