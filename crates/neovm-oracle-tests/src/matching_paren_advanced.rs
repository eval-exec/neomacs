//! Advanced oracle parity tests for `matching-paren`.
//!
//! Covers: all standard bracket types ( ) [ ] { }, syntax table interaction,
//! behavior after `modify-syntax-entry`, custom syntax tables, combined with
//! `char-syntax`, nil return for non-bracket characters, and behavior across
//! different buffer-local syntax tables.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// matching-paren for all standard bracket types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matching_paren_all_standard_brackets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test matching-paren for all six standard bracket characters
    // in the default syntax table, verify each returns its matching partner
    let form = r#"(with-temp-buffer
  (list
   ;; Parentheses
   (matching-paren ?\()      ;; should return ?\)
   (matching-paren ?\))      ;; should return ?\(
   ;; Square brackets
   (matching-paren ?\[)      ;; should return ?\]
   (matching-paren ?\])      ;; should return ?\[
   ;; Curly braces
   (matching-paren ?{)       ;; should return ?}
   (matching-paren ?})       ;; should return ?{
   ;; Verify returned values are characters
   (characterp (matching-paren ?\())
   (characterp (matching-paren ?\[))
   (characterp (matching-paren ?{))
   ;; Verify symmetry: matching-paren of matching-paren returns original
   (= ?\( (matching-paren (matching-paren ?\()))
   (= ?\[ (matching-paren (matching-paren ?\[)))
   (= ?{ (matching-paren (matching-paren ?{)))))"#;
    let expect = expect_test::expect![[r#""OK (41 40 93 91 125 123 t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// matching-paren returns nil for non-bracket characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matching_paren_nil_for_non_brackets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Various non-bracket characters should return nil from matching-paren:
    // letters, digits, whitespace, punctuation, string delimiters, etc.
    let form = r#"(with-temp-buffer
  (list
   ;; Word constituents
   (matching-paren ?a)
   (matching-paren ?Z)
   (matching-paren ?0)
   (matching-paren ?9)
   ;; Whitespace
   (matching-paren ?\s)
   (matching-paren ?\t)
   (matching-paren ?\n)
   ;; Punctuation
   (matching-paren ?.)
   (matching-paren ?,)
   (matching-paren ?!)
   (matching-paren ??)
   (matching-paren ?+)
   (matching-paren ?-)
   (matching-paren ?*)
   (matching-paren ?/)
   (matching-paren ?=)
   ;; String delimiters
   (matching-paren ?\")
   ;; Symbol constituents
   (matching-paren ?_)
   ;; Escape character
   (matching-paren ?\\)
   ;; Verify all are nil
   (null (matching-paren ?a))
   (null (matching-paren ?.))
   (null (matching-paren ?\"))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// matching-paren after modify-syntax-entry adds custom bracket pairs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matching_paren_custom_brackets_via_modify_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a custom syntax table, add angle brackets and guillemets
    // as paired delimiters, verify matching-paren reflects changes
    let form = r#"(with-temp-buffer
  (let ((st (make-syntax-table)))
    ;; Add angle brackets as open/close pair
    (modify-syntax-entry ?< "(>" st)
    (modify-syntax-entry ?> ")<" st)
    ;; Add pipe characters as open/close pair (unusual but valid)
    (modify-syntax-entry ?| "(}" st)
    (modify-syntax-entry ?} ")|" st)
    ;; Make @ a word constituent (verify it's NOT a bracket)
    (modify-syntax-entry ?@ "w" st)
    (set-syntax-table st)
    (list
     ;; Custom angle brackets work
     (matching-paren ?<)       ;; should return ?>
     (matching-paren ?>)       ;; should return ?<
     ;; Custom pipe/brace pair
     (matching-paren ?|)       ;; should return ?}
     ;; Standard brackets still work (inherited)
     (matching-paren ?\()
     (matching-paren ?\))
     (matching-paren ?\[)
     (matching-paren ?\])
     ;; Word constituent returns nil
     (matching-paren ?@)
     ;; Verify types
     (characterp (matching-paren ?<))
     (null (matching-paren ?@)))))"#;
    let expect = expect_test::expect![[r#""OK (62 60 125 41 40 93 91 nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// matching-paren combined with char-syntax cross-verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matching_paren_combined_with_char_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // For every character that matching-paren returns non-nil,
    // char-syntax should return either ?\( (open) or ?\) (close).
    // Build a comprehensive verification across many characters.
    let form = r#"(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?< "(>" st)
    (modify-syntax-entry ?> ")<" st)
    (set-syntax-table st)
    (let ((test-chars (list ?\( ?\) ?\[ ?\] ?{ ?} ?< ?>
                            ?a ?z ?0 ?\s ?. ?, ?+ ?- ?\"))
          (results nil))
      ;; For each char: (char matching-paren char-syntax is-bracket-syntax)
      (dolist (ch test-chars)
        (let* ((mp (matching-paren ch))
               (syn (char-syntax ch))
               ;; A char is a bracket iff char-syntax is ?\( or ?\)
               (is-open (= syn ?\())
               (is-close (= syn ?\)))
               (is-bracket (or is-open is-close)))
          (setq results
                (cons (list ch mp syn is-bracket
                            ;; matching-paren non-nil iff is-bracket
                            (eq (not (null mp)) is-bracket))
                      results))))
      (let ((verification (nreverse results)))
        (list
         ;; All consistency checks should be t
         (let ((all-ok t))
           (dolist (v verification)
             (unless (nth 4 v)
               (setq all-ok nil)))
           all-ok)
         ;; Open brackets have char-syntax ?\(
         (= (char-syntax ?\() ?\()
         (= (char-syntax ?\[) ?\()
         (= (char-syntax ?{) ?\()
         (= (char-syntax ?<) ?\()
         ;; Close brackets have char-syntax ?\)
         (= (char-syntax ?\)) ?\))
         (= (char-syntax ?\]) ?\))
         (= (char-syntax ?}) ?\))
         (= (char-syntax ?>) ?\))
         ;; Full result list for deterministic comparison
         verification)))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t t t t ((40 41 40 t t) (41 40 41 t t) (91 93 40 t t) (93 91 41 t t) (123 125 40 t t) (125 123 41 t t) (60 62 40 t t) (62 60 41 t t) (97 nil 119 nil t) (122 nil 119 nil t) (48 nil 119 nil t) (32 nil 32 nil t) (46 nil 46 nil t) (44 nil 46 nil t) (43 nil 95 nil t) (45 nil 95 nil t) (34 nil 34 nil t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// matching-paren across different buffer-local syntax tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matching_paren_buffer_local_syntax_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create two buffers with different syntax tables:
    // Buffer A: < > are brackets
    // Buffer B: < > are punctuation (default)
    // Verify matching-paren respects the current buffer's syntax table
    let form = r#"(let ((st-with-angles (make-syntax-table))
      (st-without-angles (make-syntax-table)))
  ;; Configure: angles are brackets in one table
  (modify-syntax-entry ?< "(>" st-with-angles)
  (modify-syntax-entry ?> ")<" st-with-angles)
  ;; In the other table, angles remain punctuation (default)
  (modify-syntax-entry ?< "." st-without-angles)
  (modify-syntax-entry ?> "." st-without-angles)
  (let (result-with result-without)
    (with-temp-buffer
      (set-syntax-table st-with-angles)
      (setq result-with
            (list (matching-paren ?<)
                  (matching-paren ?>)
                  (matching-paren ?\()
                  (char-syntax ?<)
                  (char-syntax ?>))))
    (with-temp-buffer
      (set-syntax-table st-without-angles)
      (setq result-without
            (list (matching-paren ?<)
                  (matching-paren ?>)
                  (matching-paren ?\()
                  (char-syntax ?<)
                  (char-syntax ?>))))
    (list
     result-with
     result-without
     ;; In with-angles: < and > have matches
     (characterp (nth 0 result-with))
     (characterp (nth 1 result-with))
     ;; In without-angles: < and > return nil
     (null (nth 0 result-without))
     (null (nth 1 result-without))
     ;; Standard parens work in both
     (characterp (nth 2 result-with))
     (characterp (nth 2 result-without)))))"#;
    let expect =
        expect_test::expect![[r#""OK ((62 60 41 40 41) (nil nil 41 46 46) t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// matching-paren with Unicode bracket characters via syntax modification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matching_paren_unicode_brackets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Register Unicode characters as bracket pairs and test matching-paren
    // Uses guillemets and CJK brackets
    let form = r#"(with-temp-buffer
  (let ((st (make-syntax-table)))
    ;; Left guillemet (171) matches right guillemet (187)
    (modify-syntax-entry 171 "(187" st)
    (modify-syntax-entry 187 ")171" st)
    ;; CJK left corner bracket (12300) matches right (12301)
    (modify-syntax-entry 12300 "(12301" st)
    (modify-syntax-entry 12301 ")12300" st)
    ;; Fullwidth left paren (65288) matches fullwidth right paren (65289)
    (modify-syntax-entry 65288 "(65289" st)
    (modify-syntax-entry 65289 ")65288" st)
    (set-syntax-table st)
    (list
     ;; Guillemets
     (matching-paren 171)        ;; left guillemet -> right guillemet (187)
     (matching-paren 187)        ;; right guillemet -> left guillemet (171)
     ;; CJK corner brackets
     (matching-paren 12300)
     (matching-paren 12301)
     ;; Fullwidth parens
     (matching-paren 65288)
     (matching-paren 65289)
     ;; Verify the return values match expected pairs
     (= (matching-paren 171) 187)
     (= (matching-paren 187) 171)
     (= (matching-paren 12300) 12301)
     (= (matching-paren 12301) 12300)
     (= (matching-paren 65288) 65289)
     (= (matching-paren 65289) 65288))))"#;
    let expect = expect_test::expect![[r#""OK (49 49 49 49 54 54 nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// matching-paren driven bracket-matching algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matching_paren_bracket_matcher() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use matching-paren to implement a bracket-matching validator
    // that checks if a string has properly balanced brackets
    let form = r#"(progn
  (fset 'neovm--test-brackets-balanced-p
    (lambda (str)
      "Check if brackets in STR are properly balanced using matching-paren."
      (with-temp-buffer
        (let ((st (make-syntax-table)))
          (modify-syntax-entry ?< "(>" st)
          (modify-syntax-entry ?> ")<" st)
          (set-syntax-table st)
          (let ((stack nil)
                (i 0)
                (len (length str))
                (valid t))
            (while (and valid (< i len))
              (let* ((ch (aref str i))
                     (syn (char-syntax ch))
                     (mp (matching-paren ch)))
                (cond
                 ;; Opening bracket: push onto stack
                 ((and mp (= syn ?\())
                  (setq stack (cons ch stack)))
                 ;; Closing bracket: check match against stack top
                 ((and mp (= syn ?\)))
                  (if (and stack (= (car stack) mp))
                      (setq stack (cdr stack))
                    (setq valid nil)))
                 ;; Non-bracket: skip
                 (t nil)))
              (setq i (1+ i)))
            ;; Balanced if valid and stack is empty
            (and valid (null stack)))))))

  (unwind-protect
      (list
       ;; Balanced cases
       (funcall 'neovm--test-brackets-balanced-p "()")
       (funcall 'neovm--test-brackets-balanced-p "[]")
       (funcall 'neovm--test-brackets-balanced-p "{}")
       (funcall 'neovm--test-brackets-balanced-p "<>")
       (funcall 'neovm--test-brackets-balanced-p "({[<>]})")
       (funcall 'neovm--test-brackets-balanced-p "a(b[c{d<e>f}g]h)i")
       (funcall 'neovm--test-brackets-balanced-p "")
       (funcall 'neovm--test-brackets-balanced-p "no brackets here")
       ;; Unbalanced cases
       (funcall 'neovm--test-brackets-balanced-p "(")
       (funcall 'neovm--test-brackets-balanced-p ")")
       (funcall 'neovm--test-brackets-balanced-p "([)]")
       (funcall 'neovm--test-brackets-balanced-p "((())")
       (funcall 'neovm--test-brackets-balanced-p "<{>}")
       (funcall 'neovm--test-brackets-balanced-p "}{"))
    (fmakunbound 'neovm--test-brackets-balanced-p)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
