//! Oracle parity tests for `delete-char` with ALL parameter combinations:
//! positive N (delete forward), negative N (delete backward), N=0 (no-op),
//! KILLFLAG parameter (nil vs t), boundary errors, narrowing interactions,
//! and complex character-by-character processing.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// delete-char with positive N: delete forward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_char_positive_n_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test delete-char with various positive N values from different positions
    let form = r#"(let ((results nil))
  ;; Delete 1 char from beginning
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char (point-min))
          (delete-char 1)
          (list (buffer-string) (point))) results)
  ;; Delete 3 chars from beginning
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char (point-min))
          (delete-char 3)
          (list (buffer-string) (point))) results)
  ;; Delete all chars
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char (point-min))
          (delete-char 6)
          (list (buffer-string) (point))) results)
  ;; Delete from middle position
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char 3)
          (delete-char 2)
          (list (buffer-string) (point))) results)
  ;; Delete 1 char at a time in a loop
  (push (with-temp-buffer
          (insert "hello")
          (goto-char (point-min))
          (let ((removed nil))
            (dotimes (_ 5)
              (push (char-after (point)) removed)
              (delete-char 1))
            (list (buffer-string) (nreverse removed)))) results)
  ;; Delete from end minus one
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char 6) ;; before 'f'
          (delete-char 1)
          (list (buffer-string) (point))) results)
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"bcdef\" 1) (\"def\" 1) (\"\" 1) (\"abef\" 3) (\"\" (104 101 108 108 111)) (\"abcde\" 6))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-char with negative N: delete backward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_char_negative_n_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
  ;; Delete 1 char backward from end
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char (point-max))
          (delete-char -1)
          (list (buffer-string) (point))) results)
  ;; Delete 3 chars backward from end
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char (point-max))
          (delete-char -3)
          (list (buffer-string) (point))) results)
  ;; Delete all chars backward
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char (point-max))
          (delete-char -6)
          (list (buffer-string) (point))) results)
  ;; Delete backward from middle
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char 4) ;; after 'c'
          (delete-char -2)
          (list (buffer-string) (point))) results)
  ;; Delete 1 char backward at a time in a loop
  (push (with-temp-buffer
          (insert "world")
          (goto-char (point-max))
          (let ((removed nil))
            (dotimes (_ 5)
              (push (char-before (point)) removed)
              (delete-char -1))
            (list (buffer-string) removed))) results)
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"abcde\" 6) (\"abc\" 4) (\"\" 1) (\"adef\" 2) (\"\" (119 111 114 108 100)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-char with N = 0: no-op
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_char_zero_noop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
  ;; N=0 at beginning of buffer
  (push (with-temp-buffer
          (insert "hello")
          (goto-char (point-min))
          (delete-char 0)
          (list (buffer-string) (point) (buffer-size))) results)
  ;; N=0 at end of buffer
  (push (with-temp-buffer
          (insert "hello")
          (goto-char (point-max))
          (delete-char 0)
          (list (buffer-string) (point) (buffer-size))) results)
  ;; N=0 at middle of buffer
  (push (with-temp-buffer
          (insert "hello")
          (goto-char 3)
          (delete-char 0)
          (list (buffer-string) (point) (buffer-size))) results)
  ;; N=0 in empty buffer
  (push (with-temp-buffer
          (delete-char 0)
          (list (buffer-string) (point) (buffer-size))) results)
  ;; N=0 with killflag t (still no-op)
  (push (with-temp-buffer
          (insert "hello")
          (goto-char 3)
          (delete-char 0 t)
          (list (buffer-string) (point) (buffer-size))) results)
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" 1 5) (\"hello\" 6 5) (\"hello\" 3 5) (\"\" 1 0) (\"hello\" 3 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// KILLFLAG parameter: nil vs t
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_char_killflag_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test killflag nil and t with various N values.
    // When killflag is t, deleted text is saved to kill ring.
    let form = r#"(let ((results nil))
  ;; killflag nil, positive N
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char (point-min))
          (delete-char 3 nil)
          (list (buffer-string) (point))) results)
  ;; killflag t, positive N (same deletion, but text goes to kill ring)
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char (point-min))
          (delete-char 3 t)
          (list (buffer-string) (point))) results)
  ;; killflag nil, negative N
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char (point-max))
          (delete-char -2 nil)
          (list (buffer-string) (point))) results)
  ;; killflag t, negative N
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char (point-max))
          (delete-char -2 t)
          (list (buffer-string) (point))) results)
  ;; killflag with N=0 (no deletion either way)
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char 3)
          (delete-char 0 nil)
          (delete-char 0 t)
          (list (buffer-string) (point))) results)
  ;; killflag with various truthy values (any non-nil is truthy)
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char (point-min))
          (delete-char 1 'yes)
          (list (buffer-string) (point))) results)
  (push (with-temp-buffer
          (insert "abcdef")
          (goto-char (point-min))
          (delete-char 1 42)
          (list (buffer-string) (point))) results)
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"def\" 1) (\"def\" 1) (\"abcd\" 5) (\"abcd\" 5) (\"abcdef\" 3) (\"bcdef\" 1) (\"bcdef\" 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-char at beginning/end of buffer: error handling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_char_boundary_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Trying to delete past beginning should signal an error
    let form1 = r#"(condition-case err
    (with-temp-buffer
      (insert "hello")
      (goto-char (point-min))
      (delete-char -1)
      'no-error)
  (error (list 'got-error (car err))))"#;
    let expect = expect_test::expect![[r#""OK (got-error beginning-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(form1, expect);

    // Trying to delete past end should signal an error
    let form2 = r#"(condition-case err
    (with-temp-buffer
      (insert "hello")
      (goto-char (point-max))
      (delete-char 1)
      'no-error)
  (error (list 'got-error (car err))))"#;
    let expect = expect_test::expect![[r#""OK (got-error end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // Trying to delete more chars than available (forward)
    let form3 = r#"(condition-case err
    (with-temp-buffer
      (insert "hi")
      (goto-char (point-min))
      (delete-char 10)
      'no-error)
  (error (list 'got-error (car err))))"#;
    let expect = expect_test::expect![[r#""OK (got-error end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);

    // Trying to delete more chars than available (backward)
    let form4 = r#"(condition-case err
    (with-temp-buffer
      (insert "hi")
      (goto-char (point-max))
      (delete-char -10)
      'no-error)
  (error (list 'got-error (car err))))"#;
    let expect = expect_test::expect![[r#""OK (got-error beginning-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(form4, expect);

    // Delete in empty buffer (should error for any N != 0)
    let form5 = r#"(condition-case err
    (with-temp-buffer
      (delete-char 1)
      'no-error)
  (error (list 'got-error (car err))))"#;
    let expect = expect_test::expect![[r#""OK (got-error end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(form5, expect);

    // Delete exact remaining chars (should succeed, not error)
    let form6 = r#"(with-temp-buffer
  (insert "abc")
  (goto-char 2)
  (delete-char 2)
  (list (buffer-string) (point)))"#;
    let expect = expect_test::expect![[r#""OK (\"a\" 2)""#]];
    crate::common::assert_oracle_parity_expect(form6, expect);
}

// ---------------------------------------------------------------------------
// delete-char with narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_char_with_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
  ;; Narrow to middle, delete within narrowed region
  (push (with-temp-buffer
          (insert "0123456789")
          (narrow-to-region 4 8)
          (goto-char (point-min))
          (delete-char 2)
          (let ((narrowed (buffer-string)))
            (widen)
            (list narrowed (buffer-string)))) results)
  ;; Delete backward within narrowed region
  (push (with-temp-buffer
          (insert "0123456789")
          (narrow-to-region 4 8)
          (goto-char (point-max))
          (delete-char -2)
          (let ((narrowed (buffer-string)))
            (widen)
            (list narrowed (buffer-string)))) results)
  ;; Error when trying to delete past narrowed boundary (forward)
  (push (condition-case err
            (with-temp-buffer
              (insert "0123456789")
              (narrow-to-region 4 8)
              (goto-char (point-min))
              (delete-char 10) ;; more than narrowed region
              'no-error)
          (error (list 'got-error (car err)))) results)
  ;; Error when trying to delete past narrowed boundary (backward)
  (push (condition-case err
            (with-temp-buffer
              (insert "0123456789")
              (narrow-to-region 4 8)
              (goto-char (point-max))
              (delete-char -10)
              'no-error)
          (error (list 'got-error (car err)))) results)
  ;; Delete all within narrowed region, then widen to see rest
  (push (with-temp-buffer
          (insert "ABCDEFGHIJ")
          (narrow-to-region 3 8)
          (goto-char (point-min))
          (delete-char (- (point-max) (point-min)))
          (let ((narrowed-str (buffer-string))
                (narrowed-size (buffer-size)))
            (widen)
            (list narrowed-str narrowed-size (buffer-string)))) results)
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"56\" \"01256789\") (\"34\" \"01234789\") (got-error end-of-buffer) (got-error beginning-of-buffer) (\"\" 5 \"ABHIJ\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: character-by-character buffer processing with delete-char
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_char_processing_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Process buffer char by char: keep only alphabetic chars, delete digits and punctuation
    let form = r#"(with-temp-buffer
  (insert "h3ll0 w0rld! t35t-c4s3.")
  (goto-char (point-min))
  (let ((kept 0) (deleted 0))
    (while (not (eobp))
      (let ((ch (char-after (point))))
        (if (or (and (>= ch ?a) (<= ch ?z))
                (and (>= ch ?A) (<= ch ?Z))
                (= ch ?\s))
            (progn
              (setq kept (1+ kept))
              (forward-char 1))
          (setq deleted (1+ deleted))
          (delete-char 1))))
    (list (buffer-string) kept deleted (point) (buffer-size))))"#;
    let expect = expect_test::expect![[r#""OK (\"hll wrld ttcs\" 13 10 14 13)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: delete-char in loops with position tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_char_loop_with_position_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Delete every other character, tracking positions throughout
    let form = r#"(with-temp-buffer
  (insert "abcdefghij")
  (goto-char (point-min))
  (let ((positions nil)
        (deleted-chars nil)
        (toggle nil))
    ;; Walk through, alternating between keep and delete
    (while (not (eobp))
      (if toggle
          (progn
            (push (list 'del (char-after (point)) (point)) positions)
            (push (char-after (point)) deleted-chars)
            (delete-char 1))
        (push (list 'keep (char-after (point)) (point)) positions)
        (forward-char 1))
      (setq toggle (not toggle)))
    (list (buffer-string)
          (nreverse deleted-chars)
          (length (nreverse positions))
          (buffer-size))))"#;
    let expect = expect_test::expect![[r#""OK (\"acegi\" (98 100 102 104 106) 10 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: delete-char with multibyte characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_char_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
  ;; Delete forward through multibyte string
  (push (with-temp-buffer
          (insert "abc")
          (goto-char (point-min))
          (delete-char 1)
          (list (buffer-string) (point))) results)
  ;; Delete backward through string with accented chars
  (push (with-temp-buffer
          (insert "cafe\u0301")
          (goto-char (point-max))
          (delete-char -1)
          (list (buffer-string) (point) (buffer-size))) results)
  ;; Mixed ASCII and non-ASCII: delete from middle
  (push (with-temp-buffer
          (insert "a-b-c-d-e")
          (goto-char 4)
          (delete-char 3) ;; delete "-c-"
          (list (buffer-string) (point))) results)
  ;; Delete chars and rebuild
  (push (with-temp-buffer
          (insert "hello")
          (goto-char 2)
          (delete-char 3) ;; delete "ell"
          (insert "ELL")
          (list (buffer-string) (point))) results)
  (nreverse results))"#;
    let expect =
        expect_test::expect![[r#""OK ((\"bc\" 1) (\"cafe\" 5 4) (\"a-bd-e\" 4) (\"hELLo\" 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: interleaved insert and delete-char operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_char_interleaved_with_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "ABCDE")
  ;; Transform by deleting and inserting at various points
  (goto-char 2) ;; after A
  (delete-char 1) ;; delete B
  (insert "b")   ;; insert lowercase b
  (goto-char 4) ;; after b, C
  (delete-char 1) ;; delete D
  (insert "d")   ;; insert lowercase d
  ;; Now buffer should be "AbCdE"
  (let ((step1 (buffer-string)))
    ;; Delete from both ends
    (goto-char (point-min))
    (delete-char 1) ;; delete A
    (goto-char (point-max))
    (delete-char -1) ;; delete E
    (let ((step2 (buffer-string)))
      ;; Insert at boundaries
      (goto-char (point-min))
      (insert "[")
      (goto-char (point-max))
      (insert "]")
      (list step1 step2 (buffer-string) (buffer-size) (point)))))"#;
    let expect = expect_test::expect![[r#""OK (\"AbCdE\" \"bCd\" \"[bCd]\" 5 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
