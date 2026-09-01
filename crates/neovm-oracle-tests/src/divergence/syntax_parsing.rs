//! Syntax parsing divergence probes (calibration).
//!
//! Probes parse-partial-sexp state vectors (paren depth, in-string,
//! in-comment, quoted, comment-style) across various buffer contents,
//! scan-lists, scan-sexps, and forward-sexp/list/up-list/down-list navigation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_sp_parse_partial_basic_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 2 nil nil nil 0 nil nil (1) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(a (b) c)")
  (parse-partial-sexp 1 4))
"##,
        expect,
    );
}

#[test]
fn div_sp_parse_partial_into_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 2 34 nil nil 0 nil 9 (1) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(concat \"abc")
  (parse-partial-sexp 1 12))
"##,
        expect,
    );
}

#[test]
fn div_sp_parse_partial_into_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil nil nil t nil 0 nil 1 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "; abc comment")
  (parse-partial-sexp 1 8))
"##,
        expect,
    );
}

#[test]
fn div_sp_parse_partial_nested_parens() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3 4 nil nil nil 0 nil nil (1 2 3) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(((a)))")
  (parse-partial-sexp 1 5))
"##,
        expect,
    );
}

#[test]
fn div_sp_parse_partial_escaped_quote_in_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil nil 34 nil nil 0 nil 1 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "\"a\\\"b")
  (parse-partial-sexp 1 5))
"##,
        expect,
    );
}

#[test]
fn div_sp_parse_partial_semicolon_in_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil nil 34 nil nil 0 nil 1 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "\"a;b\"")
  (parse-partial-sexp 1 5))
"##,
        expect,
    );
}

#[test]
fn div_sp_scan_lists_forward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments scan-lists 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(a (b) c) x")
  (scan-lists 1 1))
"##,
        expect,
    );
}

#[test]
fn div_sp_scan_lists_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments scan-lists 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "x (a (b) c)")
  (scan-lists 12 -1))
"##,
        expect,
    );
}

#[test]
fn div_sp_scan_sexps_forward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "a b c d")
  (scan-sexps 1 2))
"##,
        expect,
    );
}

#[test]
fn div_sp_forward_sexp_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(a b) (c d)")
  (goto-char 1)
  (list (progn (forward-sexp) (point))
        (progn (forward-sexp) (point))))
"##,
        expect,
    );
}

#[test]
fn div_sp_forward_list_up_down() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (12 1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(a (b c) d)")
  (goto-char 1)
  (list (progn (forward-list) (point))
        (progn (backward-list) (point))
        (progn (down-list) (point))))
"##,
        expect,
    );
}

#[test]
fn div_sp_up_list_from_inner() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 9""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(a (b c) d)")
  (goto-char 5)
  (condition-case err (progn (up-list) (point)) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_sp_parse_partial_quoted_paren() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil nil nil nil t 0 nil nil nil 9)""#]];
    // "\(a\)": every paren is escaped (Sescape quotes the next char), so neither
    // "(" nor ")" is a delimiter.  The trailing escape skips to EOB and GNU's
    // scan_sexps_forward reaches `endquoted', which bypasses `symdone' so the
    // last-complete-sexp slot (element 2) stays nil; element 5 (quoted) is t and
    // element 10 holds the escape syntax code (Sescape = 9).
    // GNU: (0 nil nil nil nil t 0 nil nil nil 9)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "\\(a\\)")
  (parse-partial-sexp 1 5))
"##,
        expect,
    );
}

#[test]
fn div_sp_parse_partial_escaped_paren_mid_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil 1 nil nil nil 0 nil nil nil nil)""#]];
    // "(a \( b)": the escaped "(" in the middle is NOT an open paren, so the
    // outer list stays balanced and closes; element 2 (last complete sexp) is
    // the outer list start 1, element 5 nil, element 10 nil.
    // GNU: (0 nil 1 nil nil nil 0 nil nil nil nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(a \\( b)")
  (parse-partial-sexp 1 9))
"##,
        expect,
    );
}

#[test]
fn div_sp_parse_partial_char_literal_paren() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 nil nil nil nil 0 nil nil (2) nil)""#]];
    // "?(": parse-partial-sexp uses the SYNTAX TABLE, not reader semantics, so
    // "?" is an expression-prefix and "(" still opens a list -> depth 1, with
    // the open paren at 2 recorded in element 1 and the open-paren stack (2).
    // GNU: (1 2 nil nil nil nil 0 nil nil (2) nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "?(")
  (parse-partial-sexp 1 3))
"##,
        expect,
    );
}

#[test]
fn div_sp_parse_partial_escaped_paren_in_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil nil 34 nil nil 0 nil 1 nil nil)""#]];
    // Escape inside a string: "\"a\\(b\"" is the 5 chars  " a \ ( b  with no
    // closing quote scanned, so element 3 reports the string terminator (34 =
    // ?\") and element 8 the string start (1); the escaped "(" is inert.
    // GNU: (0 nil nil 34 nil nil 0 nil 1 nil nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "\"a\\(b\"")
  (parse-partial-sexp 1 6))
"##,
        expect,
    );
}

#[test]
fn div_sp_parse_partial_lone_trailing_escape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil nil nil nil t 0 nil nil nil 9)""#]];
    // "a\": a symbol char then a trailing escape at EOB.  Like \(a\), the
    // trailing escape forces `endquoted', so the symbol is NOT registered as a
    // complete sexp (element 2 nil); element 5 (quoted) is t and element 10 = 9.
    // GNU: (0 nil nil nil nil t 0 nil nil nil 9)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "a\\")
  (parse-partial-sexp 1 3))
"##,
        expect,
    );
}

#[test]
fn div_sp_parse_partial_escaped_symbol_completes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil 1 nil nil nil 0 nil nil nil nil)""#]];
    // "a\(b\)c" scanned fully: the escapes are part of one symbol run; a normal
    // symbol char ends the run via `symdone', so element 2 (last complete sexp)
    // is the symbol start 1, with no trailing quote (element 5 nil, 10 nil).
    // GNU: (0 nil 1 nil nil nil 0 nil nil nil nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "a\\(b\\)c")
  (parse-partial-sexp 1 8))
"##,
        expect,
    );
}

#[test]
fn div_sp_parse_partial_box_quotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 2 nil nil nil 0 nil nil (1) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(comment \"text\")")
  (parse-partial-sexp 1 10))
"##,
        expect,
    );
}

#[test]
fn div_sp_parse_partial_oldstate_continue() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 1 2 34 nil nil 0 nil 4 (1) nil) (1 1 nil nil nil nil 1 nil nil (1) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(a \"str\" (b))")
  (let* ((s1 (parse-partial-sexp 1 5))
         (s2 (parse-partial-sexp 5 9 nil nil s1)))
    (list s1 s2)))
"##,
        expect,
    );
}

#[test]
fn div_sp_unbalanced_paren_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-number-of-arguments""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(a (b")
  (condition-case err (scan-lists 1 1) (scan-error (list 'scan-error)) (error (car err))))
"##,
        expect,
    );
}

// ---------------------------------------------------------------------------
// parse-sexp-ignore-comments sexp scanning (PR #134 / #118)
//
// forward-sexp/backward-sexp/scan-sexps/scan-lists skip whole comment bodies
// when `parse-sexp-ignore-comments' is non-nil, matching GNU's
// scan_sexps_forward / scan_lists comment-skipping (src/syntax.c).  These lock
// in that a stray/unbalanced paren inside a comment body is ignored, and that
// the non-ignore path (parse-sexp-ignore-comments nil) is unaffected.
// ---------------------------------------------------------------------------

#[test]
fn div_sp_ignore_comments_forward_sexp_line_comment_stray_paren() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 22""#]];
    // emacs-lisp-mode has parse-sexp-ignore-comments t by default; the stray
    // "(" inside the ";; oops (" comment must not be treated as an open paren,
    // so forward-sexp skips the comment and the "(real sexp)" list -> 22.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert ";; oops (\n(real sexp)\n")
  (goto-char (point-min))
  (forward-sexp)
  (point))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_forward_sexp_c_block_comment_parens() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 16""#]];
    // The C block comment "/* ) ( */" holds an unbalanced close+open; with
    // parse-sexp-ignore-comments t forward-sexp skips the comment body and the
    // whole list "(a /* ) ( */ b)" -> 16.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (c-mode)
  (setq parse-sexp-ignore-comments t)
  (insert "(a /* ) ( */ b)")
  (goto-char (point-min))
  (forward-sexp)
  (point))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_backward_sexp_line_comment_stray_paren() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    // backward-sexp 2 from end skips "(bar)" then the "; c (" comment with its
    // stray "(" and lands at the start of "(foo)" -> 1.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(foo) ; c (\n(bar)")
  (goto-char (point-max))
  (backward-sexp 2)
  (point))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_backward_sexp_nested_c_block_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 29""#]];
    // c-mode treats /* */ as nestable here; backward-sexp from end skips the
    // "(b)" then must skip the whole nested "/* outer /* inner */ */" comment,
    // landing at the start of "(b)" line -> 29.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (c-mode)
  (setq parse-sexp-ignore-comments t)
  (insert "(a) /* outer /* inner */ */\n(b)")
  (goto-char (point-max))
  (backward-sexp)
  (point))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_scan_sexps_unbalanced_paren_in_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 21""#]];
    // The #118 case: the unmatched "(" lives inside the ";  unmatched (" line
    // comment, so scan-sexps over the "(a ... b)" list finds the real closing
    // paren and returns 21 instead of signaling.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(a ; unmatched (\n b)")
  (scan-sexps 1 1))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_forward_sexp_two_block_comments_in_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 24""#]];
    // Two separate block comments inside a single list; forward-sexp must skip
    // both and the whole list "(a /* c */ b /* d */ e)" -> 24.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (c-mode)
  (setq parse-sexp-ignore-comments t)
  (insert "(a /* c */ b /* d */ e)")
  (goto-char (point-min))
  (forward-sexp)
  (point))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_scan_lists_backward_over_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 13""#]];
    // scan-lists backward over a comment containing a stray close paren must
    // skip the comment body and land before "(b)" -> 13.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (c-mode)
  (setq parse-sexp-ignore-comments t)
  (insert "(a) /* ) */ (b)")
  (scan-lists (point-max) -1 0))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_eof_unterminated_c_block_signals() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (scan-error)""#]];
    // An unterminated C block comment runs to EOF; GNU signals scan-error
    // because the enclosing list never closes.  Lock the signal class.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (c-mode)
  (setq parse-sexp-ignore-comments t)
  (insert "(a /* unterminated")
  (goto-char (point-min))
  (condition-case err
      (progn (forward-sexp) (point))
    (scan-error (list 'scan-error))
    (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_eof_unterminated_line_signals() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (scan-error)""#]];
    // An unterminated line comment swallows the rest of the buffer; the
    // enclosing list "(a ; ..." never closes -> scan-error.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(a ; unterminated")
  (goto-char (point-min))
  (condition-case err
      (progn (forward-sexp) (point))
    (scan-error (list 'scan-error))
    (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_disabled_line_comment_stops_in_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 8""#]];
    // With parse-sexp-ignore-comments nil the comment is NOT skipped: ";" is a
    // comment-starter but not a sexp boundary, so forward-sexp over ";; oops "
    // stops just before the stray "(" at 8.  Locks the non-ignore path.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (setq parse-sexp-ignore-comments nil)
  (insert ";; oops (\n(real sexp)\n")
  (goto-char (point-min))
  (condition-case err
      (progn (forward-sexp) (point))
    (scan-error (list 'scan-error))
    (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_disabled_scan_sexps_sees_comment_paren() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (scan-error)""#]];
    // With parse-sexp-ignore-comments nil the stray "(" inside the comment is
    // counted as a real open paren, leaving the list unbalanced -> scan-error.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (setq parse-sexp-ignore-comments nil)
  (insert "(a ; unmatched (\n b)")
  (condition-case err
      (scan-sexps 1 1)
    (scan-error (list 'scan-error))
    (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_string_beats_block_comment_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 41""#]];
    // The first "/* ... */" appears inside a string and must not be treated as
    // a comment opener.  Only the later real C block comment is skipped, so the
    // whole outer list is one sexp.
    // GNU: 41
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (c-mode)
  (setq parse-sexp-ignore-comments t)
  (insert "(a \"/* not comment ) */\" /* real ) */ b)")
  (goto-char (point-min))
  (forward-sexp)
  (point))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_respects_narrowing_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 22 21 21)""#]];
    // Comment skipping must operate inside the accessible region only and
    // preserve GNU's absolute point values while narrowed.
    // GNU: (8 22 21 21)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (c-mode)
  (setq parse-sexp-ignore-comments t)
  (insert "prefix (a /* ) */ b) suffix")
  (narrow-to-region 8 22)
  (goto-char (point-min))
  (list (point-min) (point-max)
        (progn (forward-sexp) (point))
        (scan-sexps (point-min) 1)))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_forward_and_backward_multicount_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (34 1)""#]];
    // Multi-count sexp motion repeats the skip logic across line comments in
    // both directions.  The stray open paren and close paren in comments are
    // both ignored while counting foo, (bar), and baz as three sexps.
    // GNU: (34 1)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "foo ; stray (\n(bar) ; stray )\nbaz")
  (goto-char (point-min))
  (let ((fwd (progn (forward-sexp 3) (point))))
    (goto-char (point-max))
    (let ((back (progn (backward-sexp 3) (point))))
      (list fwd back))))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_motion_from_inside_line_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (14 22)""#]];
    // Starting inside a comment body is a distinct path from starting before
    // the comment opener.  GNU skips to the next real sexp when moving forward
    // and back to the previous real sexp when moving backward.
    // GNU: (14 22)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(a) ; comment with ( and )\n(b)")
  (goto-char 8)
  (let ((fwd (progn (forward-sexp) (point))))
    (goto-char 23)
    (let ((back (progn (backward-sexp) (point))))
      (list fwd back))))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_forward_list_down_up_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (33 1 2 32 33)""#]];
    // Exercise list wrappers around scan-lists in one path: forward-list,
    // backward-list, down-list, forward-sexp with a comment before the nested
    // list, and up-list from inside the outer list.
    // GNU: (33 1 2 32 33)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (c-mode)
  (setq parse-sexp-ignore-comments t)
  (insert "(/* ) */ (inner /* ( */ x) tail)")
  (goto-char (point-min))
  (list (progn (forward-list) (point))
        (progn (backward-list) (point))
        (progn (down-list) (point))
        (progn (forward-sexp 2) (point))
        (progn (up-list) (point))))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_scan_lists_depth_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 11 48)""#]];
    // scan-lists with a nonzero depth takes different exits than scan-sexps.
    // This locks forward depth exit, backward depth exit, and the unbalanced
    // depth error payload in the presence of a skipped block comment.
    // GNU: (20 1 (scan-error "Unbalanced parentheses" 4 22))
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (c-mode)
  (setq parse-sexp-ignore-comments t)
  (insert "(a /* ) */ (b c) d) z")
  (list (scan-lists 4 1 1)
        (scan-lists 18 -1 1)
        (condition-case err
            (scan-lists 4 1 2)
          (scan-error (cons 'scan-error (cdr err)))
          (error (cons (car err) (cdr err)))))))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_comment_fence_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 31 31)""#]];
    // Scomment_fence is another comment representation used by syntax tables.
    // Treat "!" as a fence comment delimiter; the unmatched parens inside the
    // fenced comment must be ignored while scanning the outer list.
    // GNU: (nil 31 31)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((st (make-syntax-table)))
    (with-syntax-table st
      (modify-syntax-entry ?! "!" st)
      (setq parse-sexp-ignore-comments t)
      (insert "(a ! comment with ) and ( ! b)")
      (goto-char (point-min))
      (list (forward-sexp) (point) (scan-sexps 1 1)))))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_syntax_property_comment_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 19 (1 1 6 nil nil nil 0 nil nil (1) nil))""#]];
    // The syntax-table text property can dynamically turn a span into a
    // comment.  This covers the honor-properties path combined with
    // parse-sexp-ignore-comments and syntax-ppss state reporting.
    // GNU: (nil 19 (1 1 6 nil nil nil 0 nil nil (1) nil))
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (setq parse-sexp-ignore-comments t)
  (insert "(a x not-comment ) b)")
  (put-text-property 4 17 'syntax-table (string-to-syntax "<"))
  (put-text-property 17 18 'syntax-table (string-to-syntax ">"))
  (goto-char (point-min))
  (condition-case err
      (list (forward-sexp) (point) (syntax-ppss 12))
    (scan-error (cons 'scan-error (cdr err)))
    (error (cons (car err) (cdr err)))))
"##,
        expect,
    );
}

#[test]
fn div_sp_ignore_comments_backward_over_strings_and_comments_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42 1 1)""#]];
    // Mixed strings and comments in both directions: delimiters embedded in
    // strings are inert, the real block comment is skipped, and backward sexp
    // scanning returns to the outer list start.
    // GNU: (42 1 1)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (c-mode)
  (setq parse-sexp-ignore-comments t)
  (insert "(a \"/* not ) */\" /* ) */ (b \"; not (\" c))")
  (list (scan-sexps 1 1)
        (scan-sexps (point-max) -1)
        (progn (goto-char (point-max)) (backward-sexp) (point))))
"##,
        expect,
    );
}
