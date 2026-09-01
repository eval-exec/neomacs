//! Oracle parity tests for a rope-like string data structure in Elisp.
//!
//! Implements a functional rope where nodes are either leaf strings or
//! concat nodes (left, right). Tests: construction, concatenation,
//! character-at-index, split-at-position, to-string conversion,
//! and a text editor buffer model using rope operations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Rope construction from string and basic operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rope_construction_and_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build ropes from strings, concatenate them, compute lengths,
    // and verify structural properties. Rope representation:
    //   Leaf: (leaf . "string")
    //   Concat: (concat length left right)
    let form = r#"(progn
  (fset 'neovm--rope-leaf
    (lambda (s) (cons 'leaf s)))

  (fset 'neovm--rope-length
    (lambda (rope)
      (cond
       ((null rope) 0)
       ((eq (car rope) 'leaf) (length (cdr rope)))
       ((eq (car rope) 'concat) (cadr rope))
       (t (error "Invalid rope node: %S" rope)))))

  (fset 'neovm--rope-concat
    (lambda (left right)
      (cond
       ((null left) right)
       ((null right) left)
       (t (list 'concat
                (+ (funcall 'neovm--rope-length left)
                   (funcall 'neovm--rope-length right))
                left right)))))

  (fset 'neovm--rope-to-string
    (lambda (rope)
      (cond
       ((null rope) "")
       ((eq (car rope) 'leaf) (cdr rope))
       ((eq (car rope) 'concat)
        (concat (funcall 'neovm--rope-to-string (caddr rope))
                (funcall 'neovm--rope-to-string (cadddr rope))))
       (t (error "Invalid rope node: %S" rope)))))

  (fset 'neovm--rope-depth
    (lambda (rope)
      (cond
       ((null rope) 0)
       ((eq (car rope) 'leaf) 1)
       ((eq (car rope) 'concat)
        (1+ (max (funcall 'neovm--rope-depth (caddr rope))
                 (funcall 'neovm--rope-depth (cadddr rope)))))
       (t 0))))

  (unwind-protect
      (let* ((r1 (funcall 'neovm--rope-leaf "Hello"))
             (r2 (funcall 'neovm--rope-leaf " "))
             (r3 (funcall 'neovm--rope-leaf "World"))
             (r4 (funcall 'neovm--rope-leaf "!"))
             ;; Build tree: ((Hello . " ") . (World . !))
             (left (funcall 'neovm--rope-concat r1 r2))
             (right (funcall 'neovm--rope-concat r3 r4))
             (full (funcall 'neovm--rope-concat left right))
             ;; Empty rope
             (empty (funcall 'neovm--rope-leaf ""))
             ;; Concat with nil
             (with-nil (funcall 'neovm--rope-concat r1 nil)))
        (list
         ;; Lengths
         (funcall 'neovm--rope-length r1)
         (funcall 'neovm--rope-length left)
         (funcall 'neovm--rope-length full)
         (funcall 'neovm--rope-length empty)
         (funcall 'neovm--rope-length with-nil)
         ;; To-string
         (funcall 'neovm--rope-to-string r1)
         (funcall 'neovm--rope-to-string left)
         (funcall 'neovm--rope-to-string full)
         (funcall 'neovm--rope-to-string empty)
         (funcall 'neovm--rope-to-string with-nil)
         ;; Depth
         (funcall 'neovm--rope-depth r1)
         (funcall 'neovm--rope-depth left)
         (funcall 'neovm--rope-depth full)
         ;; Structure type checks
         (eq (car r1) 'leaf)
         (eq (car full) 'concat)))
    (fmakunbound 'neovm--rope-leaf)
    (fmakunbound 'neovm--rope-length)
    (fmakunbound 'neovm--rope-concat)
    (fmakunbound 'neovm--rope-to-string)
    (fmakunbound 'neovm--rope-depth)))"#;
    let expect = expect_test::expect![[
        r#""OK (5 6 12 0 5 \"Hello\" \"Hello \" \"Hello World!\" \"\" \"Hello\" 1 2 3 t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Rope index: character at position
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rope_index_char_at() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Index into a rope to retrieve the character at a given 0-based position.
    // Handles navigation through concat nodes by comparing index with left subtree length.
    let form = r#"(progn
  (fset 'neovm--rope-leaf (lambda (s) (cons 'leaf s)))
  (fset 'neovm--rope-length
    (lambda (rope)
      (cond ((null rope) 0) ((eq (car rope) 'leaf) (length (cdr rope)))
            ((eq (car rope) 'concat) (cadr rope)) (t 0))))
  (fset 'neovm--rope-concat
    (lambda (left right)
      (cond ((null left) right) ((null right) left)
            (t (list 'concat (+ (funcall 'neovm--rope-length left)
                                (funcall 'neovm--rope-length right))
                     left right)))))
  (fset 'neovm--rope-char-at
    (lambda (rope idx)
      "Return character at 0-based index IDX in ROPE, or nil if out of bounds."
      (cond
       ((null rope) nil)
       ((eq (car rope) 'leaf)
        (let ((s (cdr rope)))
          (if (and (>= idx 0) (< idx (length s)))
              (aref s idx)
            nil)))
       ((eq (car rope) 'concat)
        (let ((left-len (funcall 'neovm--rope-length (caddr rope))))
          (if (< idx left-len)
              (funcall 'neovm--rope-char-at (caddr rope) idx)
            (funcall 'neovm--rope-char-at (cadddr rope) (- idx left-len)))))
       (t nil))))

  (unwind-protect
      (let* ((r1 (funcall 'neovm--rope-leaf "abcd"))
             (r2 (funcall 'neovm--rope-leaf "efgh"))
             (r3 (funcall 'neovm--rope-leaf "ijkl"))
             ;; Build: (("abcd" . "efgh") . "ijkl")
             (left (funcall 'neovm--rope-concat r1 r2))
             (full (funcall 'neovm--rope-concat left r3)))
        (list
         ;; Chars from left subtree
         (funcall 'neovm--rope-char-at full 0)   ;; a
         (funcall 'neovm--rope-char-at full 3)   ;; d
         ;; Boundary: cross from r1 to r2
         (funcall 'neovm--rope-char-at full 4)   ;; e
         (funcall 'neovm--rope-char-at full 7)   ;; h
         ;; Boundary: cross from left to r3
         (funcall 'neovm--rope-char-at full 8)   ;; i
         (funcall 'neovm--rope-char-at full 11)  ;; l
         ;; Out of bounds
         (funcall 'neovm--rope-char-at full 12)
         (funcall 'neovm--rope-char-at full -1)
         ;; Verify the full string for cross-reference
         (let ((result nil) (i 0) (len (funcall 'neovm--rope-length full)))
           (while (< i len)
             (setq result (cons (funcall 'neovm--rope-char-at full i) result))
             (setq i (1+ i)))
           (concat (nreverse result)))))
    (fmakunbound 'neovm--rope-leaf)
    (fmakunbound 'neovm--rope-length)
    (fmakunbound 'neovm--rope-concat)
    (fmakunbound 'neovm--rope-char-at)))"#;
    let expect =
        expect_test::expect![[r#""OK (97 100 101 104 105 108 nil nil \"abcdefghijkl\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Rope split at position
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rope_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Split a rope at a given position into (left-rope . right-rope).
    // The split descends into concat nodes and splits leaf strings.
    let form = r#"(progn
  (fset 'neovm--rope-leaf (lambda (s) (cons 'leaf s)))
  (fset 'neovm--rope-length
    (lambda (rope)
      (cond ((null rope) 0) ((eq (car rope) 'leaf) (length (cdr rope)))
            ((eq (car rope) 'concat) (cadr rope)) (t 0))))
  (fset 'neovm--rope-concat
    (lambda (left right)
      (cond ((null left) right) ((null right) left)
            (t (list 'concat (+ (funcall 'neovm--rope-length left)
                                (funcall 'neovm--rope-length right))
                     left right)))))
  (fset 'neovm--rope-to-string
    (lambda (rope)
      (cond ((null rope) "") ((eq (car rope) 'leaf) (cdr rope))
            ((eq (car rope) 'concat)
             (concat (funcall 'neovm--rope-to-string (caddr rope))
                     (funcall 'neovm--rope-to-string (cadddr rope))))
            (t ""))))
  (fset 'neovm--rope-split
    (lambda (rope pos)
      "Split ROPE at POS, returning (left-rope . right-rope)."
      (cond
       ((null rope) (cons nil nil))
       ((<= pos 0) (cons nil rope))
       ((>= pos (funcall 'neovm--rope-length rope)) (cons rope nil))
       ((eq (car rope) 'leaf)
        (let ((s (cdr rope)))
          (cons (funcall 'neovm--rope-leaf (substring s 0 pos))
                (funcall 'neovm--rope-leaf (substring s pos)))))
       ((eq (car rope) 'concat)
        (let ((left-len (funcall 'neovm--rope-length (caddr rope))))
          (cond
           ((= pos left-len)
            (cons (caddr rope) (cadddr rope)))
           ((< pos left-len)
            (let ((sub-split (funcall 'neovm--rope-split (caddr rope) pos)))
              (cons (car sub-split)
                    (funcall 'neovm--rope-concat (cdr sub-split) (cadddr rope)))))
           (t
            (let ((sub-split (funcall 'neovm--rope-split (cadddr rope) (- pos left-len))))
              (cons (funcall 'neovm--rope-concat (caddr rope) (car sub-split))
                    (cdr sub-split)))))))
       (t (cons rope nil)))))

  (unwind-protect
      (let* ((r1 (funcall 'neovm--rope-leaf "Hello"))
             (r2 (funcall 'neovm--rope-leaf " World"))
             (full (funcall 'neovm--rope-concat r1 r2)))
        ;; Split at various positions
        (let ((s0 (funcall 'neovm--rope-split full 0))
              (s3 (funcall 'neovm--rope-split full 3))
              (s5 (funcall 'neovm--rope-split full 5))
              (s8 (funcall 'neovm--rope-split full 8))
              (s11 (funcall 'neovm--rope-split full 11))
              (s99 (funcall 'neovm--rope-split full 99)))
          (list
           ;; Split at 0: empty left, full right
           (funcall 'neovm--rope-to-string (car s0))
           (funcall 'neovm--rope-to-string (cdr s0))
           ;; Split at 3: "Hel" | "lo World"
           (funcall 'neovm--rope-to-string (car s3))
           (funcall 'neovm--rope-to-string (cdr s3))
           ;; Split at 5 (boundary): "Hello" | " World"
           (funcall 'neovm--rope-to-string (car s5))
           (funcall 'neovm--rope-to-string (cdr s5))
           ;; Split at 8: "Hello Wo" | "rld"
           (funcall 'neovm--rope-to-string (car s8))
           (funcall 'neovm--rope-to-string (cdr s8))
           ;; Split at 11 (full length): full left, empty right
           (funcall 'neovm--rope-to-string (car s11))
           (funcall 'neovm--rope-to-string (cdr s11))
           ;; Split beyond length: same as full length
           (funcall 'neovm--rope-to-string (car s99))
           (funcall 'neovm--rope-to-string (cdr s99))
           ;; Verify concatenation of split halves reconstructs original
           (string= (concat (funcall 'neovm--rope-to-string (car s3))
                            (funcall 'neovm--rope-to-string (cdr s3)))
                    "Hello World")
           (string= (concat (funcall 'neovm--rope-to-string (car s8))
                            (funcall 'neovm--rope-to-string (cdr s8)))
                    "Hello World"))))
    (fmakunbound 'neovm--rope-leaf)
    (fmakunbound 'neovm--rope-length)
    (fmakunbound 'neovm--rope-concat)
    (fmakunbound 'neovm--rope-to-string)
    (fmakunbound 'neovm--rope-split)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\" \"Hello World\" \"Hel\" \"lo World\" \"Hello\" \" World\" \"Hello Wo\" \"rld\" \"Hello World\" \"\" \"Hello World\" \"\" t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Rope insert and delete operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rope_insert_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build insert and delete on top of split and concat.
    // Insert: split at position, concat left + new + right.
    // Delete: split at start, split right at (end-start), concat left + right-right.
    let form = r#"(progn
  (fset 'neovm--rope-leaf (lambda (s) (cons 'leaf s)))
  (fset 'neovm--rope-length
    (lambda (rope)
      (cond ((null rope) 0) ((eq (car rope) 'leaf) (length (cdr rope)))
            ((eq (car rope) 'concat) (cadr rope)) (t 0))))
  (fset 'neovm--rope-concat
    (lambda (left right)
      (cond ((null left) right) ((null right) left)
            (t (list 'concat (+ (funcall 'neovm--rope-length left)
                                (funcall 'neovm--rope-length right))
                     left right)))))
  (fset 'neovm--rope-to-string
    (lambda (rope)
      (cond ((null rope) "") ((eq (car rope) 'leaf) (cdr rope))
            ((eq (car rope) 'concat)
             (concat (funcall 'neovm--rope-to-string (caddr rope))
                     (funcall 'neovm--rope-to-string (cadddr rope))))
            (t ""))))
  (fset 'neovm--rope-split
    (lambda (rope pos)
      (cond
       ((null rope) (cons nil nil))
       ((<= pos 0) (cons nil rope))
       ((>= pos (funcall 'neovm--rope-length rope)) (cons rope nil))
       ((eq (car rope) 'leaf)
        (let ((s (cdr rope)))
          (cons (funcall 'neovm--rope-leaf (substring s 0 pos))
                (funcall 'neovm--rope-leaf (substring s pos)))))
       ((eq (car rope) 'concat)
        (let ((left-len (funcall 'neovm--rope-length (caddr rope))))
          (cond
           ((= pos left-len) (cons (caddr rope) (cadddr rope)))
           ((< pos left-len)
            (let ((sub (funcall 'neovm--rope-split (caddr rope) pos)))
              (cons (car sub) (funcall 'neovm--rope-concat (cdr sub) (cadddr rope)))))
           (t
            (let ((sub (funcall 'neovm--rope-split (cadddr rope) (- pos left-len))))
              (cons (funcall 'neovm--rope-concat (caddr rope) (car sub))
                    (cdr sub)))))))
       (t (cons rope nil)))))
  (fset 'neovm--rope-insert
    (lambda (rope pos text)
      "Insert TEXT at POS in ROPE."
      (let ((halves (funcall 'neovm--rope-split rope pos)))
        (funcall 'neovm--rope-concat
                 (funcall 'neovm--rope-concat (car halves) (funcall 'neovm--rope-leaf text))
                 (cdr halves)))))
  (fset 'neovm--rope-delete
    (lambda (rope start end)
      "Delete characters from START to END in ROPE."
      (let* ((split1 (funcall 'neovm--rope-split rope start))
             (right-part (cdr split1))
             (split2 (funcall 'neovm--rope-split right-part (- end start))))
        (funcall 'neovm--rope-concat (car split1) (cdr split2)))))

  (unwind-protect
      (let* ((r (funcall 'neovm--rope-leaf "Hello World")))
        ;; Insert " Beautiful" at position 5
        (let ((r2 (funcall 'neovm--rope-insert r 5 " Beautiful")))
          ;; Delete " World" (positions 15..21) from r2
          (let ((r3 (funcall 'neovm--rope-delete r2 15 21)))
            ;; Insert at beginning
            (let ((r4 (funcall 'neovm--rope-insert r 0 ">>> ")))
              ;; Insert at end
              (let ((r5 (funcall 'neovm--rope-insert r 11 " <<<")))
                ;; Multiple operations: delete then insert (replace)
                (let* ((r6 (funcall 'neovm--rope-delete r 5 11))
                       (r7 (funcall 'neovm--rope-insert r6 5 " Elisp")))
                  (list
                   (funcall 'neovm--rope-to-string r)
                   (funcall 'neovm--rope-to-string r2)
                   (funcall 'neovm--rope-to-string r3)
                   (funcall 'neovm--rope-to-string r4)
                   (funcall 'neovm--rope-to-string r5)
                   (funcall 'neovm--rope-to-string r7)
                   ;; Lengths
                   (funcall 'neovm--rope-length r2)
                   (funcall 'neovm--rope-length r3)
                   ;; Original unchanged (immutable)
                   (funcall 'neovm--rope-to-string r))))))))
    (fmakunbound 'neovm--rope-leaf)
    (fmakunbound 'neovm--rope-length)
    (fmakunbound 'neovm--rope-concat)
    (fmakunbound 'neovm--rope-to-string)
    (fmakunbound 'neovm--rope-split)
    (fmakunbound 'neovm--rope-insert)
    (fmakunbound 'neovm--rope-delete)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello World\" \"Hello Beautiful World\" \"Hello Beautiful\" \">>> Hello World\" \"Hello World <<<\" \"Hello Elisp\" 21 15 \"Hello World\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: text editor buffer model using rope operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rope_editor_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Model a text editor buffer with cursor position, insert-at-cursor,
    // delete-before-cursor (backspace), move cursor, and undo stack.
    // The buffer state is (rope cursor-pos undo-stack).
    let form = r#"(progn
  ;; Rope primitives (same as above, re-defined for self-containment)
  (fset 'neovm--re-leaf (lambda (s) (cons 'leaf s)))
  (fset 'neovm--re-length
    (lambda (rope)
      (cond ((null rope) 0) ((eq (car rope) 'leaf) (length (cdr rope)))
            ((eq (car rope) 'concat) (cadr rope)) (t 0))))
  (fset 'neovm--re-concat
    (lambda (left right)
      (cond ((null left) right) ((null right) left)
            (t (list 'concat (+ (funcall 'neovm--re-length left)
                                (funcall 'neovm--re-length right))
                     left right)))))
  (fset 'neovm--re-to-string
    (lambda (rope)
      (cond ((null rope) "") ((eq (car rope) 'leaf) (cdr rope))
            ((eq (car rope) 'concat)
             (concat (funcall 'neovm--re-to-string (caddr rope))
                     (funcall 'neovm--re-to-string (cadddr rope))))
            (t ""))))
  (fset 'neovm--re-split
    (lambda (rope pos)
      (cond
       ((null rope) (cons nil nil))
       ((<= pos 0) (cons nil rope))
       ((>= pos (funcall 'neovm--re-length rope)) (cons rope nil))
       ((eq (car rope) 'leaf)
        (let ((s (cdr rope)))
          (cons (funcall 'neovm--re-leaf (substring s 0 pos))
                (funcall 'neovm--re-leaf (substring s pos)))))
       ((eq (car rope) 'concat)
        (let ((left-len (funcall 'neovm--re-length (caddr rope))))
          (cond
           ((= pos left-len) (cons (caddr rope) (cadddr rope)))
           ((< pos left-len)
            (let ((sub (funcall 'neovm--re-split (caddr rope) pos)))
              (cons (car sub) (funcall 'neovm--re-concat (cdr sub) (cadddr rope)))))
           (t
            (let ((sub (funcall 'neovm--re-split (cadddr rope) (- pos left-len))))
              (cons (funcall 'neovm--re-concat (caddr rope) (car sub))
                    (cdr sub)))))))
       (t (cons rope nil)))))

  ;; Editor buffer: (rope cursor undo-stack)
  (fset 'neovm--re-buf-make
    (lambda ()
      (list nil 0 nil)))

  (fset 'neovm--re-buf-insert
    (lambda (buf text)
      "Insert TEXT at cursor, advance cursor, push old state to undo."
      (let* ((rope (car buf))
             (cursor (cadr buf))
             (undo (caddr buf))
             (halves (funcall 'neovm--re-split rope cursor))
             (new-rope (funcall 'neovm--re-concat
                                (funcall 'neovm--re-concat (car halves) (funcall 'neovm--re-leaf text))
                                (cdr halves))))
        (list new-rope (+ cursor (length text)) (cons (list rope cursor) undo)))))

  (fset 'neovm--re-buf-backspace
    (lambda (buf n)
      "Delete N chars before cursor."
      (let* ((rope (car buf))
             (cursor (cadr buf))
             (undo (caddr buf))
             (del-start (max 0 (- cursor n))))
        (if (= del-start cursor)
            buf
          (let* ((split1 (funcall 'neovm--re-split rope del-start))
                 (split2 (funcall 'neovm--re-split (cdr split1) (- cursor del-start)))
                 (new-rope (funcall 'neovm--re-concat (car split1) (cdr split2))))
            (list new-rope del-start (cons (list rope cursor) undo)))))))

  (fset 'neovm--re-buf-move
    (lambda (buf delta)
      "Move cursor by DELTA positions, clamped to [0, length]."
      (let* ((rope (car buf))
             (cursor (cadr buf))
             (undo (caddr buf))
             (new-cursor (max 0 (min (funcall 'neovm--re-length rope) (+ cursor delta)))))
        (list rope new-cursor undo))))

  (fset 'neovm--re-buf-undo
    (lambda (buf)
      "Restore previous state from undo stack."
      (let ((undo (caddr buf)))
        (if (null undo)
            buf
          (let ((prev (car undo)))
            (list (car prev) (cadr prev) (cdr undo)))))))

  (fset 'neovm--re-buf-text
    (lambda (buf) (funcall 'neovm--re-to-string (car buf))))

  (fset 'neovm--re-buf-cursor
    (lambda (buf) (cadr buf)))

  (unwind-protect
      (let ((buf (funcall 'neovm--re-buf-make)))
        ;; Type "Hello"
        (setq buf (funcall 'neovm--re-buf-insert buf "Hello"))
        (let ((s1 (funcall 'neovm--re-buf-text buf))
              (c1 (funcall 'neovm--re-buf-cursor buf)))
          ;; Type " World"
          (setq buf (funcall 'neovm--re-buf-insert buf " World"))
          (let ((s2 (funcall 'neovm--re-buf-text buf))
                (c2 (funcall 'neovm--re-buf-cursor buf)))
            ;; Move cursor back 6 positions (before " World")
            (setq buf (funcall 'neovm--re-buf-move buf -6))
            (let ((c3 (funcall 'neovm--re-buf-cursor buf)))
              ;; Insert " Beautiful" at cursor (position 5)
              (setq buf (funcall 'neovm--re-buf-insert buf " Beautiful"))
              (let ((s4 (funcall 'neovm--re-buf-text buf))
                    (c4 (funcall 'neovm--re-buf-cursor buf)))
                ;; Backspace 3 chars ("ful")
                (setq buf (funcall 'neovm--re-buf-backspace buf 3))
                (let ((s5 (funcall 'neovm--re-buf-text buf))
                      (c5 (funcall 'neovm--re-buf-cursor buf)))
                  ;; Undo the backspace
                  (setq buf (funcall 'neovm--re-buf-undo buf))
                  (let ((s6 (funcall 'neovm--re-buf-text buf))
                        (c6 (funcall 'neovm--re-buf-cursor buf)))
                    ;; Undo the insert
                    (setq buf (funcall 'neovm--re-buf-undo buf))
                    (let ((s7 (funcall 'neovm--re-buf-text buf))
                          (c7 (funcall 'neovm--re-buf-cursor buf)))
                      (list
                       s1 c1    ;; "Hello" cursor=5
                       s2 c2    ;; "Hello World" cursor=11
                       c3       ;; cursor=5
                       s4 c4    ;; "Hello Beautiful World" cursor=15
                       s5 c5    ;; "Hello Beauti World" cursor=12
                       s6 c6    ;; "Hello Beautiful World" cursor=15 (undo backspace)
                       s7 c7    ;; "Hello World" cursor=5 (undo insert)
                       )))))))))
    (fmakunbound 'neovm--re-leaf)
    (fmakunbound 'neovm--re-length)
    (fmakunbound 'neovm--re-concat)
    (fmakunbound 'neovm--re-to-string)
    (fmakunbound 'neovm--re-split)
    (fmakunbound 'neovm--re-buf-make)
    (fmakunbound 'neovm--re-buf-insert)
    (fmakunbound 'neovm--re-buf-backspace)
    (fmakunbound 'neovm--re-buf-move)
    (fmakunbound 'neovm--re-buf-undo)
    (fmakunbound 'neovm--re-buf-text)
    (fmakunbound 'neovm--re-buf-cursor)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello\" 5 \"Hello World\" 11 5 \"Hello Beautiful World\" 15 \"Hello Beauti World\" 12 \"Hello Beautiful World\" 15 \"Hello World\" 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Rope balancing: rebalance a degenerate rope
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rope_balance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a degenerate (left-linear) rope by repeated concat,
    // then rebalance it into a more balanced tree. Verify content
    // is preserved and depth is reduced.
    let form = r#"(progn
  (fset 'neovm--rb-leaf (lambda (s) (cons 'leaf s)))
  (fset 'neovm--rb-length
    (lambda (rope)
      (cond ((null rope) 0) ((eq (car rope) 'leaf) (length (cdr rope)))
            ((eq (car rope) 'concat) (cadr rope)) (t 0))))
  (fset 'neovm--rb-concat
    (lambda (left right)
      (cond ((null left) right) ((null right) left)
            (t (list 'concat (+ (funcall 'neovm--rb-length left)
                                (funcall 'neovm--rb-length right))
                     left right)))))
  (fset 'neovm--rb-to-string
    (lambda (rope)
      (cond ((null rope) "") ((eq (car rope) 'leaf) (cdr rope))
            ((eq (car rope) 'concat)
             (concat (funcall 'neovm--rb-to-string (caddr rope))
                     (funcall 'neovm--rb-to-string (cadddr rope))))
            (t ""))))
  (fset 'neovm--rb-depth
    (lambda (rope)
      (cond ((null rope) 0) ((eq (car rope) 'leaf) 1)
            ((eq (car rope) 'concat)
             (1+ (max (funcall 'neovm--rb-depth (caddr rope))
                      (funcall 'neovm--rb-depth (cadddr rope)))))
            (t 0))))
  (fset 'neovm--rb-collect-leaves
    (lambda (rope)
      "Collect all leaf strings in left-to-right order."
      (cond
       ((null rope) nil)
       ((eq (car rope) 'leaf) (list (cdr rope)))
       ((eq (car rope) 'concat)
        (append (funcall 'neovm--rb-collect-leaves (caddr rope))
                (funcall 'neovm--rb-collect-leaves (cadddr rope))))
       (t nil))))
  (fset 'neovm--rb-build-balanced
    (lambda (leaves)
      "Build a balanced rope from a list of leaf strings."
      (let ((n (length leaves)))
        (cond
         ((= n 0) nil)
         ((= n 1) (funcall 'neovm--rb-leaf (car leaves)))
         (t (let ((mid (/ n 2)))
              (let ((left-leaves nil) (right-leaves nil) (i 0))
                (dolist (l leaves)
                  (if (< i mid)
                      (setq left-leaves (cons l left-leaves))
                    (setq right-leaves (cons l right-leaves)))
                  (setq i (1+ i)))
                (funcall 'neovm--rb-concat
                         (funcall 'neovm--rb-build-balanced (nreverse left-leaves))
                         (funcall 'neovm--rb-build-balanced (nreverse right-leaves))))))))))
  (fset 'neovm--rb-rebalance
    (lambda (rope)
      (funcall 'neovm--rb-build-balanced (funcall 'neovm--rb-collect-leaves rope))))

  (unwind-protect
      (let ((degenerate nil))
        ;; Build left-linear: concat 8 single-char leaves
        (dolist (ch '("a" "b" "c" "d" "e" "f" "g" "h"))
          (setq degenerate
                (funcall 'neovm--rb-concat degenerate (funcall 'neovm--rb-leaf ch))))
        (let ((deg-str (funcall 'neovm--rb-to-string degenerate))
              (deg-depth (funcall 'neovm--rb-depth degenerate))
              (deg-len (funcall 'neovm--rb-length degenerate)))
          ;; Rebalance
          (let* ((balanced (funcall 'neovm--rb-rebalance degenerate))
                 (bal-str (funcall 'neovm--rb-to-string balanced))
                 (bal-depth (funcall 'neovm--rb-depth balanced))
                 (bal-len (funcall 'neovm--rb-length balanced)))
            (list
             deg-str deg-depth deg-len
             bal-str bal-depth bal-len
             ;; Content preserved
             (string= deg-str bal-str)
             ;; Length preserved
             (= deg-len bal-len)
             ;; Depth reduced (degenerate is 8, balanced should be 4)
             (< bal-depth deg-depth)))))
    (fmakunbound 'neovm--rb-leaf)
    (fmakunbound 'neovm--rb-length)
    (fmakunbound 'neovm--rb-concat)
    (fmakunbound 'neovm--rb-to-string)
    (fmakunbound 'neovm--rb-depth)
    (fmakunbound 'neovm--rb-collect-leaves)
    (fmakunbound 'neovm--rb-build-balanced)
    (fmakunbound 'neovm--rb-rebalance)))"#;
    let expect = expect_test::expect![[r#""OK (\"abcdefgh\" 8 8 \"abcdefgh\" 4 8 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
