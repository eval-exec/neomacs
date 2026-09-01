//! Oracle parity tests for rope data structure operations in Elisp.
//!
//! Implements a rope as a binary tree of strings. Leaves are strings,
//! inner nodes carry a weight (total length). Operations: concatenation,
//! index, split, rebalance, insert-at-position, delete-range.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Rope concatenation and basic properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rope_ops_concat_and_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build ropes, concatenate them, verify lengths and conversions.
    // Rope representation:
    //   Leaf: (leaf . "string")
    //   Node: (node weight left right)
    let form = r#"(progn
  (fset 'neovm--rop-leaf (lambda (s) (cons 'leaf s)))

  (fset 'neovm--rop-weight
    (lambda (rope)
      (cond ((null rope) 0)
            ((eq (car rope) 'leaf) (length (cdr rope)))
            ((eq (car rope) 'node) (cadr rope))
            (t 0))))

  (fset 'neovm--rop-concat
    (lambda (a b)
      (cond ((null a) b)
            ((null b) a)
            (t (list 'node
                     (+ (funcall 'neovm--rop-weight a)
                        (funcall 'neovm--rop-weight b))
                     a b)))))

  (fset 'neovm--rop-to-string
    (lambda (rope)
      (cond ((null rope) "")
            ((eq (car rope) 'leaf) (cdr rope))
            ((eq (car rope) 'node)
             (concat (funcall 'neovm--rop-to-string (caddr rope))
                     (funcall 'neovm--rop-to-string (cadddr rope))))
            (t ""))))

  (fset 'neovm--rop-depth
    (lambda (rope)
      (cond ((null rope) 0)
            ((eq (car rope) 'leaf) 1)
            ((eq (car rope) 'node)
             (1+ (max (funcall 'neovm--rop-depth (caddr rope))
                      (funcall 'neovm--rop-depth (cadddr rope)))))
            (t 0))))

  (fset 'neovm--rop-leaf-count
    (lambda (rope)
      (cond ((null rope) 0)
            ((eq (car rope) 'leaf) 1)
            ((eq (car rope) 'node)
             (+ (funcall 'neovm--rop-leaf-count (caddr rope))
                (funcall 'neovm--rop-leaf-count (cadddr rope))))
            (t 0))))

  (unwind-protect
      (let* ((r1 (funcall 'neovm--rop-leaf "Hello"))
             (r2 (funcall 'neovm--rop-leaf " "))
             (r3 (funcall 'neovm--rop-leaf "World"))
             (r4 (funcall 'neovm--rop-leaf "!"))
             (r5 (funcall 'neovm--rop-leaf ""))
             ;; Build various trees
             (ab (funcall 'neovm--rop-concat r1 r2))
             (cd (funcall 'neovm--rop-concat r3 r4))
             (full (funcall 'neovm--rop-concat ab cd))
             ;; Concat with nil
             (with-nil-left (funcall 'neovm--rop-concat nil r1))
             (with-nil-right (funcall 'neovm--rop-concat r1 nil))
             ;; Concat with empty leaf
             (with-empty (funcall 'neovm--rop-concat r5 r1))
             ;; Chain of single chars
             (chain (funcall 'neovm--rop-concat
                     (funcall 'neovm--rop-concat
                      (funcall 'neovm--rop-leaf "a")
                      (funcall 'neovm--rop-leaf "b"))
                     (funcall 'neovm--rop-concat
                      (funcall 'neovm--rop-leaf "c")
                      (funcall 'neovm--rop-leaf "d")))))
        (list
         ;; Weights
         (funcall 'neovm--rop-weight r1)
         (funcall 'neovm--rop-weight ab)
         (funcall 'neovm--rop-weight full)
         (funcall 'neovm--rop-weight nil)
         (funcall 'neovm--rop-weight r5)
         ;; To-string
         (funcall 'neovm--rop-to-string full)
         (funcall 'neovm--rop-to-string ab)
         (funcall 'neovm--rop-to-string with-nil-left)
         (funcall 'neovm--rop-to-string with-nil-right)
         (funcall 'neovm--rop-to-string with-empty)
         (funcall 'neovm--rop-to-string chain)
         ;; Depth
         (funcall 'neovm--rop-depth r1)
         (funcall 'neovm--rop-depth ab)
         (funcall 'neovm--rop-depth full)
         (funcall 'neovm--rop-depth chain)
         ;; Leaf count
         (funcall 'neovm--rop-leaf-count full)
         (funcall 'neovm--rop-leaf-count chain)
         ;; Structure checks
         (eq (car r1) 'leaf)
         (eq (car full) 'node)))
    (fmakunbound 'neovm--rop-leaf)
    (fmakunbound 'neovm--rop-weight)
    (fmakunbound 'neovm--rop-concat)
    (fmakunbound 'neovm--rop-to-string)
    (fmakunbound 'neovm--rop-depth)
    (fmakunbound 'neovm--rop-leaf-count)))"#;
    let expect = expect_test::expect![[
        r#""OK (5 6 12 0 0 \"Hello World!\" \"Hello \" \"Hello\" \"Hello\" \"Hello\" \"abcd\" 1 2 3 3 4 4 t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Index into rope (find character at position)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rope_ops_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rop-leaf (lambda (s) (cons 'leaf s)))
  (fset 'neovm--rop-weight
    (lambda (rope)
      (cond ((null rope) 0)
            ((eq (car rope) 'leaf) (length (cdr rope)))
            ((eq (car rope) 'node) (cadr rope))
            (t 0))))
  (fset 'neovm--rop-concat
    (lambda (a b)
      (cond ((null a) b) ((null b) a)
            (t (list 'node (+ (funcall 'neovm--rop-weight a)
                               (funcall 'neovm--rop-weight b)) a b)))))

  (fset 'neovm--rop-char-at
    (lambda (rope idx)
      "Return char at 0-based IDX, or nil if out of bounds."
      (cond
       ((null rope) nil)
       ((< idx 0) nil)
       ((>= idx (funcall 'neovm--rop-weight rope)) nil)
       ((eq (car rope) 'leaf)
        (aref (cdr rope) idx))
       ((eq (car rope) 'node)
        (let ((left-w (funcall 'neovm--rop-weight (caddr rope))))
          (if (< idx left-w)
              (funcall 'neovm--rop-char-at (caddr rope) idx)
            (funcall 'neovm--rop-char-at (cadddr rope) (- idx left-w)))))
       (t nil))))

  (fset 'neovm--rop-to-string
    (lambda (rope)
      (cond ((null rope) "")
            ((eq (car rope) 'leaf) (cdr rope))
            ((eq (car rope) 'node)
             (concat (funcall 'neovm--rop-to-string (caddr rope))
                     (funcall 'neovm--rop-to-string (cadddr rope))))
            (t ""))))

  (unwind-protect
      (let* ((r (funcall 'neovm--rop-concat
                 (funcall 'neovm--rop-concat
                  (funcall 'neovm--rop-leaf "abc")
                  (funcall 'neovm--rop-leaf "de"))
                 (funcall 'neovm--rop-concat
                  (funcall 'neovm--rop-leaf "fgh")
                  (funcall 'neovm--rop-leaf "ij")))))
        ;; Rope represents "abcdefghij"
        (list
         ;; Index into each leaf region
         (funcall 'neovm--rop-char-at r 0)   ;; ?a
         (funcall 'neovm--rop-char-at r 2)   ;; ?c
         (funcall 'neovm--rop-char-at r 3)   ;; ?d (boundary: left->right of first subtree)
         (funcall 'neovm--rop-char-at r 4)   ;; ?e
         (funcall 'neovm--rop-char-at r 5)   ;; ?f (boundary: left subtree -> right subtree)
         (funcall 'neovm--rop-char-at r 7)   ;; ?h
         (funcall 'neovm--rop-char-at r 8)   ;; ?i
         (funcall 'neovm--rop-char-at r 9)   ;; ?j (last char)
         ;; Out of bounds
         (funcall 'neovm--rop-char-at r 10)
         (funcall 'neovm--rop-char-at r -1)
         ;; Verify by collecting all chars into a string
         (let ((result nil) (i 0) (len (funcall 'neovm--rop-weight r)))
           (while (< i len)
             (setq result (cons (funcall 'neovm--rop-char-at r i) result))
             (setq i (1+ i)))
           (concat (nreverse result)))
         ;; Cross-check with to-string
         (string= (funcall 'neovm--rop-to-string r)
                  (let ((result nil) (i 0) (len (funcall 'neovm--rop-weight r)))
                    (while (< i len)
                      (setq result (cons (funcall 'neovm--rop-char-at r i) result))
                      (setq i (1+ i)))
                    (concat (nreverse result))))))
    (fmakunbound 'neovm--rop-leaf)
    (fmakunbound 'neovm--rop-weight)
    (fmakunbound 'neovm--rop-concat)
    (fmakunbound 'neovm--rop-char-at)
    (fmakunbound 'neovm--rop-to-string)))"#;
    let expect =
        expect_test::expect![[r#""OK (97 99 100 101 102 104 105 106 nil nil \"abcdefghij\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Split rope at position
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rope_ops_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rop-leaf (lambda (s) (cons 'leaf s)))
  (fset 'neovm--rop-weight
    (lambda (rope)
      (cond ((null rope) 0) ((eq (car rope) 'leaf) (length (cdr rope)))
            ((eq (car rope) 'node) (cadr rope)) (t 0))))
  (fset 'neovm--rop-concat
    (lambda (a b)
      (cond ((null a) b) ((null b) a)
            (t (list 'node (+ (funcall 'neovm--rop-weight a)
                               (funcall 'neovm--rop-weight b)) a b)))))
  (fset 'neovm--rop-to-string
    (lambda (rope)
      (cond ((null rope) "") ((eq (car rope) 'leaf) (cdr rope))
            ((eq (car rope) 'node)
             (concat (funcall 'neovm--rop-to-string (caddr rope))
                     (funcall 'neovm--rop-to-string (cadddr rope))))
            (t ""))))

  (fset 'neovm--rop-split
    (lambda (rope pos)
      "Split ROPE at POS -> (left . right)."
      (cond
       ((null rope) (cons nil nil))
       ((<= pos 0) (cons nil rope))
       ((>= pos (funcall 'neovm--rop-weight rope)) (cons rope nil))
       ((eq (car rope) 'leaf)
        (let ((s (cdr rope)))
          (cons (funcall 'neovm--rop-leaf (substring s 0 pos))
                (funcall 'neovm--rop-leaf (substring s pos)))))
       ((eq (car rope) 'node)
        (let ((lw (funcall 'neovm--rop-weight (caddr rope))))
          (cond
           ((= pos lw) (cons (caddr rope) (cadddr rope)))
           ((< pos lw)
            (let ((sub (funcall 'neovm--rop-split (caddr rope) pos)))
              (cons (car sub)
                    (funcall 'neovm--rop-concat (cdr sub) (cadddr rope)))))
           (t
            (let ((sub (funcall 'neovm--rop-split (cadddr rope) (- pos lw))))
              (cons (funcall 'neovm--rop-concat (caddr rope) (car sub))
                    (cdr sub)))))))
       (t (cons rope nil)))))

  (unwind-protect
      (let* ((rope (funcall 'neovm--rop-concat
                    (funcall 'neovm--rop-leaf "Hello")
                    (funcall 'neovm--rop-concat
                     (funcall 'neovm--rop-leaf " Beautiful")
                     (funcall 'neovm--rop-leaf " World")))))
        (let ((original (funcall 'neovm--rop-to-string rope)))
          ;; Split at various positions
          (let ((s0 (funcall 'neovm--rop-split rope 0))
                (s5 (funcall 'neovm--rop-split rope 5))
                (s10 (funcall 'neovm--rop-split rope 10))
                (s15 (funcall 'neovm--rop-split rope 15))
                (s21 (funcall 'neovm--rop-split rope 21))
                (s99 (funcall 'neovm--rop-split rope 99)))
            (list
             original
             ;; Split at 0
             (funcall 'neovm--rop-to-string (car s0))
             (funcall 'neovm--rop-to-string (cdr s0))
             ;; Split at 5 (between "Hello" and " Beautiful")
             (funcall 'neovm--rop-to-string (car s5))
             (funcall 'neovm--rop-to-string (cdr s5))
             ;; Split at 10 (middle of " Beautiful")
             (funcall 'neovm--rop-to-string (car s10))
             (funcall 'neovm--rop-to-string (cdr s10))
             ;; Split at 15 (between " Beautiful" and " World")
             (funcall 'neovm--rop-to-string (car s15))
             (funcall 'neovm--rop-to-string (cdr s15))
             ;; Split at end
             (funcall 'neovm--rop-to-string (car s21))
             (funcall 'neovm--rop-to-string (cdr s21))
             ;; Split beyond end
             (funcall 'neovm--rop-to-string (car s99))
             (funcall 'neovm--rop-to-string (cdr s99))
             ;; Verify concatenation of halves reconstructs original
             (string= (concat (funcall 'neovm--rop-to-string (car s5))
                              (funcall 'neovm--rop-to-string (cdr s5)))
                      original)
             (string= (concat (funcall 'neovm--rop-to-string (car s10))
                              (funcall 'neovm--rop-to-string (cdr s10)))
                      original)))))
    (fmakunbound 'neovm--rop-leaf)
    (fmakunbound 'neovm--rop-weight)
    (fmakunbound 'neovm--rop-concat)
    (fmakunbound 'neovm--rop-to-string)
    (fmakunbound 'neovm--rop-split)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello Beautiful World\" \"\" \"Hello Beautiful World\" \"Hello\" \" Beautiful World\" \"Hello Beau\" \"tiful World\" \"Hello Beautiful\" \" World\" \"Hello Beautiful World\" \"\" \"Hello Beautiful World\" \"\" t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Rebalance rope
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rope_ops_rebalance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rop-leaf (lambda (s) (cons 'leaf s)))
  (fset 'neovm--rop-weight
    (lambda (rope)
      (cond ((null rope) 0) ((eq (car rope) 'leaf) (length (cdr rope)))
            ((eq (car rope) 'node) (cadr rope)) (t 0))))
  (fset 'neovm--rop-concat
    (lambda (a b)
      (cond ((null a) b) ((null b) a)
            (t (list 'node (+ (funcall 'neovm--rop-weight a)
                               (funcall 'neovm--rop-weight b)) a b)))))
  (fset 'neovm--rop-to-string
    (lambda (rope)
      (cond ((null rope) "") ((eq (car rope) 'leaf) (cdr rope))
            ((eq (car rope) 'node)
             (concat (funcall 'neovm--rop-to-string (caddr rope))
                     (funcall 'neovm--rop-to-string (cadddr rope))))
            (t ""))))
  (fset 'neovm--rop-depth
    (lambda (rope)
      (cond ((null rope) 0) ((eq (car rope) 'leaf) 1)
            ((eq (car rope) 'node)
             (1+ (max (funcall 'neovm--rop-depth (caddr rope))
                      (funcall 'neovm--rop-depth (cadddr rope)))))
            (t 0))))

  ;; Collect all leaves in order
  (fset 'neovm--rop-leaves
    (lambda (rope)
      (cond ((null rope) nil)
            ((eq (car rope) 'leaf)
             (if (> (length (cdr rope)) 0) (list (cdr rope)) nil))
            ((eq (car rope) 'node)
             (append (funcall 'neovm--rop-leaves (caddr rope))
                     (funcall 'neovm--rop-leaves (cadddr rope))))
            (t nil))))

  ;; Build balanced tree from leaf list
  (fset 'neovm--rop-from-leaves
    (lambda (leaves)
      (let ((n (length leaves)))
        (cond
         ((= n 0) nil)
         ((= n 1) (funcall 'neovm--rop-leaf (car leaves)))
         (t (let ((mid (/ n 2)))
              (let ((left nil) (right nil) (i 0))
                (dolist (l leaves)
                  (if (< i mid)
                      (setq left (cons l left))
                    (setq right (cons l right)))
                  (setq i (1+ i)))
                (funcall 'neovm--rop-concat
                         (funcall 'neovm--rop-from-leaves (nreverse left))
                         (funcall 'neovm--rop-from-leaves (nreverse right))))))))))

  (fset 'neovm--rop-rebalance
    (lambda (rope)
      (funcall 'neovm--rop-from-leaves (funcall 'neovm--rop-leaves rope))))

  (unwind-protect
      ;; Build a degenerate (right-linear) rope from 10 single-char leaves
      (let ((degenerate nil))
        (dolist (ch (reverse '("j" "i" "h" "g" "f" "e" "d" "c" "b" "a")))
          (setq degenerate
                (funcall 'neovm--rop-concat (funcall 'neovm--rop-leaf ch) degenerate)))
        (let ((deg-str (funcall 'neovm--rop-to-string degenerate))
              (deg-depth (funcall 'neovm--rop-depth degenerate))
              (deg-weight (funcall 'neovm--rop-weight degenerate)))
          (let* ((balanced (funcall 'neovm--rop-rebalance degenerate))
                 (bal-str (funcall 'neovm--rop-to-string balanced))
                 (bal-depth (funcall 'neovm--rop-depth balanced))
                 (bal-weight (funcall 'neovm--rop-weight balanced)))
            (list
             deg-str deg-depth deg-weight
             bal-str bal-depth bal-weight
             ;; Content preserved
             (string= deg-str bal-str)
             ;; Weight preserved
             (= deg-weight bal-weight)
             ;; Depth reduced
             (< bal-depth deg-depth)
             ;; Balanced depth should be ~log2(10) + 1 = 4 or 5
             (<= bal-depth 5)))))
    (fmakunbound 'neovm--rop-leaf)
    (fmakunbound 'neovm--rop-weight)
    (fmakunbound 'neovm--rop-concat)
    (fmakunbound 'neovm--rop-to-string)
    (fmakunbound 'neovm--rop-depth)
    (fmakunbound 'neovm--rop-leaves)
    (fmakunbound 'neovm--rop-from-leaves)
    (fmakunbound 'neovm--rop-rebalance)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"jihgfedcba\" 10 10 \"jihgfedcba\" 5 10 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: insert at position using split + concat
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rope_ops_insert_at_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rop-leaf (lambda (s) (cons 'leaf s)))
  (fset 'neovm--rop-weight
    (lambda (rope)
      (cond ((null rope) 0) ((eq (car rope) 'leaf) (length (cdr rope)))
            ((eq (car rope) 'node) (cadr rope)) (t 0))))
  (fset 'neovm--rop-concat
    (lambda (a b)
      (cond ((null a) b) ((null b) a)
            (t (list 'node (+ (funcall 'neovm--rop-weight a)
                               (funcall 'neovm--rop-weight b)) a b)))))
  (fset 'neovm--rop-to-string
    (lambda (rope)
      (cond ((null rope) "") ((eq (car rope) 'leaf) (cdr rope))
            ((eq (car rope) 'node)
             (concat (funcall 'neovm--rop-to-string (caddr rope))
                     (funcall 'neovm--rop-to-string (cadddr rope))))
            (t ""))))
  (fset 'neovm--rop-split
    (lambda (rope pos)
      (cond
       ((null rope) (cons nil nil))
       ((<= pos 0) (cons nil rope))
       ((>= pos (funcall 'neovm--rop-weight rope)) (cons rope nil))
       ((eq (car rope) 'leaf)
        (let ((s (cdr rope)))
          (cons (funcall 'neovm--rop-leaf (substring s 0 pos))
                (funcall 'neovm--rop-leaf (substring s pos)))))
       ((eq (car rope) 'node)
        (let ((lw (funcall 'neovm--rop-weight (caddr rope))))
          (cond
           ((= pos lw) (cons (caddr rope) (cadddr rope)))
           ((< pos lw)
            (let ((sub (funcall 'neovm--rop-split (caddr rope) pos)))
              (cons (car sub) (funcall 'neovm--rop-concat (cdr sub) (cadddr rope)))))
           (t
            (let ((sub (funcall 'neovm--rop-split (cadddr rope) (- pos lw))))
              (cons (funcall 'neovm--rop-concat (caddr rope) (car sub))
                    (cdr sub)))))))
       (t (cons rope nil)))))

  (fset 'neovm--rop-insert
    (lambda (rope pos text)
      "Insert TEXT at POS in ROPE."
      (let ((halves (funcall 'neovm--rop-split rope pos)))
        (funcall 'neovm--rop-concat
                 (funcall 'neovm--rop-concat
                  (car halves)
                  (funcall 'neovm--rop-leaf text))
                 (cdr halves)))))

  (unwind-protect
      (let* ((rope (funcall 'neovm--rop-leaf "Hello World")))
        ;; Insert at beginning
        (let ((r1 (funcall 'neovm--rop-insert rope 0 ">>> ")))
          ;; Insert at end
          (let ((r2 (funcall 'neovm--rop-insert rope 11 " <<<")))
            ;; Insert in middle
            (let ((r3 (funcall 'neovm--rop-insert rope 5 " Beautiful")))
              ;; Multiple insertions (chain)
              (let* ((r4 (funcall 'neovm--rop-insert rope 0 "["))
                     (r5 (funcall 'neovm--rop-insert r4
                           (funcall 'neovm--rop-weight r4) "]"))
                     (r6 (funcall 'neovm--rop-insert r5 6 "...")))
                ;; Insert into nil rope
                (let ((r7 (funcall 'neovm--rop-insert nil 0 "fresh")))
                  (list
                   (funcall 'neovm--rop-to-string r1)
                   (funcall 'neovm--rop-to-string r2)
                   (funcall 'neovm--rop-to-string r3)
                   (funcall 'neovm--rop-to-string r4)
                   (funcall 'neovm--rop-to-string r5)
                   (funcall 'neovm--rop-to-string r6)
                   (funcall 'neovm--rop-to-string r7)
                   ;; Weights correct
                   (funcall 'neovm--rop-weight r1)
                   (funcall 'neovm--rop-weight r3)
                   ;; Original unchanged (functional/immutable)
                   (funcall 'neovm--rop-to-string rope))))))))
    (fmakunbound 'neovm--rop-leaf)
    (fmakunbound 'neovm--rop-weight)
    (fmakunbound 'neovm--rop-concat)
    (fmakunbound 'neovm--rop-to-string)
    (fmakunbound 'neovm--rop-split)
    (fmakunbound 'neovm--rop-insert)))"#;
    let expect = expect_test::expect![[
        r#""OK (\">>> Hello World\" \"Hello World <<<\" \"Hello Beautiful World\" \"[Hello World\" \"[Hello World]\" \"[Hello... World]\" \"fresh\" 15 21 \"Hello World\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: delete range using split + concat
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rope_ops_delete_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rop-leaf (lambda (s) (cons 'leaf s)))
  (fset 'neovm--rop-weight
    (lambda (rope)
      (cond ((null rope) 0) ((eq (car rope) 'leaf) (length (cdr rope)))
            ((eq (car rope) 'node) (cadr rope)) (t 0))))
  (fset 'neovm--rop-concat
    (lambda (a b)
      (cond ((null a) b) ((null b) a)
            (t (list 'node (+ (funcall 'neovm--rop-weight a)
                               (funcall 'neovm--rop-weight b)) a b)))))
  (fset 'neovm--rop-to-string
    (lambda (rope)
      (cond ((null rope) "") ((eq (car rope) 'leaf) (cdr rope))
            ((eq (car rope) 'node)
             (concat (funcall 'neovm--rop-to-string (caddr rope))
                     (funcall 'neovm--rop-to-string (cadddr rope))))
            (t ""))))
  (fset 'neovm--rop-split
    (lambda (rope pos)
      (cond
       ((null rope) (cons nil nil))
       ((<= pos 0) (cons nil rope))
       ((>= pos (funcall 'neovm--rop-weight rope)) (cons rope nil))
       ((eq (car rope) 'leaf)
        (let ((s (cdr rope)))
          (cons (funcall 'neovm--rop-leaf (substring s 0 pos))
                (funcall 'neovm--rop-leaf (substring s pos)))))
       ((eq (car rope) 'node)
        (let ((lw (funcall 'neovm--rop-weight (caddr rope))))
          (cond
           ((= pos lw) (cons (caddr rope) (cadddr rope)))
           ((< pos lw)
            (let ((sub (funcall 'neovm--rop-split (caddr rope) pos)))
              (cons (car sub) (funcall 'neovm--rop-concat (cdr sub) (cadddr rope)))))
           (t
            (let ((sub (funcall 'neovm--rop-split (cadddr rope) (- pos lw))))
              (cons (funcall 'neovm--rop-concat (caddr rope) (car sub))
                    (cdr sub)))))))
       (t (cons rope nil)))))

  (fset 'neovm--rop-delete
    (lambda (rope start end)
      "Delete characters from START to END in ROPE."
      (let* ((s1 (funcall 'neovm--rop-split rope start))
             (s2 (funcall 'neovm--rop-split (cdr s1) (- end start))))
        (funcall 'neovm--rop-concat (car s1) (cdr s2)))))

  (fset 'neovm--rop-replace
    (lambda (rope start end text)
      "Replace characters from START to END with TEXT."
      (let* ((s1 (funcall 'neovm--rop-split rope start))
             (s2 (funcall 'neovm--rop-split (cdr s1) (- end start))))
        (funcall 'neovm--rop-concat
                 (funcall 'neovm--rop-concat
                  (car s1)
                  (funcall 'neovm--rop-leaf text))
                 (cdr s2)))))

  (unwind-protect
      (let* ((rope (funcall 'neovm--rop-concat
                    (funcall 'neovm--rop-leaf "Hello")
                    (funcall 'neovm--rop-leaf " Beautiful World"))))
        ;; Delete from beginning
        (let ((d1 (funcall 'neovm--rop-delete rope 0 5)))
          ;; Delete from end
          (let ((d2 (funcall 'neovm--rop-delete rope 15 21)))
            ;; Delete from middle
            (let ((d3 (funcall 'neovm--rop-delete rope 5 15)))
              ;; Delete everything
              (let ((d4 (funcall 'neovm--rop-delete rope 0 21)))
                ;; Delete nothing (start=end)
                (let ((d5 (funcall 'neovm--rop-delete rope 5 5)))
                  ;; Replace: delete + insert in one step
                  (let ((r1 (funcall 'neovm--rop-replace rope 6 15 "Wonderful")))
                    ;; Chain: delete then insert
                    (let* ((r2 (funcall 'neovm--rop-delete rope 5 21))
                           (r3 (funcall 'neovm--rop-concat r2 (funcall 'neovm--rop-leaf " Elisp!"))))
                      (list
                       (funcall 'neovm--rop-to-string rope)
                       (funcall 'neovm--rop-to-string d1)
                       (funcall 'neovm--rop-to-string d2)
                       (funcall 'neovm--rop-to-string d3)
                       (funcall 'neovm--rop-to-string d4)
                       (funcall 'neovm--rop-to-string d5)
                       (funcall 'neovm--rop-to-string r1)
                       (funcall 'neovm--rop-to-string r3)
                       ;; Weight checks
                       (funcall 'neovm--rop-weight d1)
                       (funcall 'neovm--rop-weight d4)
                       ;; Original unchanged
                       (funcall 'neovm--rop-to-string rope)))))))))
    (fmakunbound 'neovm--rop-leaf)
    (fmakunbound 'neovm--rop-weight)
    (fmakunbound 'neovm--rop-concat)
    (fmakunbound 'neovm--rop-to-string)
    (fmakunbound 'neovm--rop-split)
    (fmakunbound 'neovm--rop-delete)
    (fmakunbound 'neovm--rop-replace)))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: rope-based find and replace
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rope_ops_find_and_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find a pattern in the rope (by converting to string), then replace
    // occurrences using split/concat operations
    let form = r#"(progn
  (fset 'neovm--rop-leaf (lambda (s) (cons 'leaf s)))
  (fset 'neovm--rop-weight
    (lambda (rope)
      (cond ((null rope) 0) ((eq (car rope) 'leaf) (length (cdr rope)))
            ((eq (car rope) 'node) (cadr rope)) (t 0))))
  (fset 'neovm--rop-concat
    (lambda (a b)
      (cond ((null a) b) ((null b) a)
            (t (list 'node (+ (funcall 'neovm--rop-weight a)
                               (funcall 'neovm--rop-weight b)) a b)))))
  (fset 'neovm--rop-to-string
    (lambda (rope)
      (cond ((null rope) "") ((eq (car rope) 'leaf) (cdr rope))
            ((eq (car rope) 'node)
             (concat (funcall 'neovm--rop-to-string (caddr rope))
                     (funcall 'neovm--rop-to-string (cadddr rope))))
            (t ""))))
  (fset 'neovm--rop-split
    (lambda (rope pos)
      (cond
       ((null rope) (cons nil nil))
       ((<= pos 0) (cons nil rope))
       ((>= pos (funcall 'neovm--rop-weight rope)) (cons rope nil))
       ((eq (car rope) 'leaf)
        (let ((s (cdr rope)))
          (cons (funcall 'neovm--rop-leaf (substring s 0 pos))
                (funcall 'neovm--rop-leaf (substring s pos)))))
       ((eq (car rope) 'node)
        (let ((lw (funcall 'neovm--rop-weight (caddr rope))))
          (cond
           ((= pos lw) (cons (caddr rope) (cadddr rope)))
           ((< pos lw)
            (let ((sub (funcall 'neovm--rop-split (caddr rope) pos)))
              (cons (car sub) (funcall 'neovm--rop-concat (cdr sub) (cadddr rope)))))
           (t
            (let ((sub (funcall 'neovm--rop-split (cadddr rope) (- pos lw))))
              (cons (funcall 'neovm--rop-concat (caddr rope) (car sub))
                    (cdr sub)))))))
       (t (cons rope nil)))))

  ;; Find first occurrence of pattern in rope (naive search on string)
  (fset 'neovm--rop-find
    (lambda (rope pattern)
      "Find first index of PATTERN in ROPE, or nil."
      (let* ((s (funcall 'neovm--rop-to-string rope))
             (plen (length pattern))
             (slen (length s))
             (result nil) (i 0))
        (while (and (not result) (<= (+ i plen) slen))
          (when (string= (substring s i (+ i plen)) pattern)
            (setq result i))
          (setq i (1+ i)))
        result)))

  ;; Replace first occurrence
  (fset 'neovm--rop-replace-first
    (lambda (rope pattern replacement)
      (let ((pos (funcall 'neovm--rop-find rope pattern)))
        (if pos
            (let* ((s1 (funcall 'neovm--rop-split rope pos))
                   (s2 (funcall 'neovm--rop-split (cdr s1) (length pattern))))
              (funcall 'neovm--rop-concat
                       (funcall 'neovm--rop-concat
                        (car s1)
                        (funcall 'neovm--rop-leaf replacement))
                       (cdr s2)))
          rope))))

  ;; Replace all occurrences (iterative)
  (fset 'neovm--rop-replace-all
    (lambda (rope pattern replacement)
      (let ((changed t))
        (while changed
          (let ((pos (funcall 'neovm--rop-find rope pattern)))
            (if pos
                (setq rope (funcall 'neovm--rop-replace-first rope pattern replacement))
              (setq changed nil))))
        rope)))

  (unwind-protect
      (let* ((rope (funcall 'neovm--rop-concat
                    (funcall 'neovm--rop-leaf "foo bar foo baz foo"))))
        (list
         ;; Find
         (funcall 'neovm--rop-find rope "foo")
         (funcall 'neovm--rop-find rope "bar")
         (funcall 'neovm--rop-find rope "baz")
         (funcall 'neovm--rop-find rope "xyz")
         ;; Replace first
         (funcall 'neovm--rop-to-string
          (funcall 'neovm--rop-replace-first rope "foo" "QUX"))
         ;; Replace all
         (funcall 'neovm--rop-to-string
          (funcall 'neovm--rop-replace-all rope "foo" "X"))
         ;; Replace with longer string
         (funcall 'neovm--rop-to-string
          (funcall 'neovm--rop-replace-all rope "foo" "LONGWORD"))
         ;; Replace with empty string (deletion)
         (funcall 'neovm--rop-to-string
          (funcall 'neovm--rop-replace-all rope "foo " ""))
         ;; Original unchanged
         (funcall 'neovm--rop-to-string rope)))
    (fmakunbound 'neovm--rop-leaf)
    (fmakunbound 'neovm--rop-weight)
    (fmakunbound 'neovm--rop-concat)
    (fmakunbound 'neovm--rop-to-string)
    (fmakunbound 'neovm--rop-split)
    (fmakunbound 'neovm--rop-find)
    (fmakunbound 'neovm--rop-replace-first)
    (fmakunbound 'neovm--rop-replace-all)))"#;
    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure (t) (a b) (cond ((null a) b) ((null b) a) (t (list 'node (+ (funcall 'neovm--rop-weight a) (funcall 'neovm--rop-weight b)) a b)))) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
