//! Oracle parity tests for buffer position operations:
//! `count-lines`, `line-number-at-pos`, `line-beginning-position`,
//! `line-end-position`, `pos-bol`, `pos-eol`, `column-number`,
//! `current-column`, `move-to-column`, `goto-line`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// count-lines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "line1\nline2\nline3\nline4\n")
                    (list (count-lines (point-min) (point-max))
                          (count-lines 1 6)
                          (count-lines 1 1)))"#;
    let expect = expect_test::expect![r#""OK (4 1 0)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_count_lines_no_trailing_newline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "line1\nline2\nline3")
                    (list (count-lines (point-min) (point-max))
                          (count-lines 1 (point-max))))"#;
    let expect = expect_test::expect![r#""OK (3 3)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_count_lines_accepts_marker_bounds_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs lisp/simple.el:count-lines delegates to narrow-to-region;
    // its START and END arguments are documented and declared as integer or
    // marker values.
    let form = r#"(with-temp-buffer
  (insert "line1\nline2\nline3\nline4")
  (let ((start (copy-marker 1))
        (end (copy-marker (point-max))))
    (list (count-lines start end)
          (count-lines end start))))"#;
    let expect = expect_test::expect![r#""OK (4 4)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// line-number-at-pos
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_line_number_at_pos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "aaa\nbbb\nccc\nddd\n")
                    (list
                     (line-number-at-pos 1)
                     (line-number-at-pos 4)
                     (line-number-at-pos 5)
                     (line-number-at-pos 8)
                     (line-number-at-pos 9)
                     (line-number-at-pos (point-max))))"#;
    let expect = expect_test::expect![r#""OK (1 1 2 2 3 5)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_line_number_at_pos_no_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Without argument, uses current point
    let form = r#"(with-temp-buffer
                    (insert "one\ntwo\nthree\n")
                    (goto-char (point-min))
                    (let ((at-start (line-number-at-pos)))
                      (forward-line 2)
                      (let ((at-third (line-number-at-pos)))
                        (list at-start at-third))))"#;
    let expect = expect_test::expect![r#""OK (1 3)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_line_number_at_pos_accepts_marker_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs fns.c:Fline_number_at_pos has an explicit marker branch and
    // uses the marker position when POSITION is a marker.
    let form = r#"(with-temp-buffer
  (insert "one\ntwo\nthree\n")
  (let ((m (copy-marker 6)))
    (line-number-at-pos m)))"#;
    let expect = expect_test::expect![r#""OK 2""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_line_number_at_pos_bignum_position_error_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs fns.c:Fline_number_at_pos validates a numeric POSITION with
    // CHECK_FIXNUM, unlike line-beginning-position/line-end-position which
    // accept bignum line offsets.
    let form = r#"(with-temp-buffer
  (insert "one\ntwo\n")
  (line-number-at-pos 1000000000000000000000000000000))"#;
    let expect = expect_test::expect![
        r#""ERR (wrong-type-argument fixnump 1000000000000000000000000000000)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// line-beginning-position / line-end-position
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_line_beginning_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world\nfoo bar\nbaz qux\n")
                    (goto-char 14)  ;; middle of "foo bar"
                    (list (line-beginning-position)
                          (line-end-position)
                          ;; N argument: lines ahead/behind
                          (line-beginning-position 0)
                          (line-beginning-position 2)
                          (line-end-position 0)
                          (line-end-position 2)))"#;
    let expect = expect_test::expect![r#""OK (13 20 1 21 12 28)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// current-column
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_current_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world\n  indented\n\ttabbed\n")
                    (let ((cols nil))
                      (goto-char (point-min))
                      (setq cols (cons (current-column) cols))
                      (goto-char 6)  ;; after "hello"
                      (setq cols (cons (current-column) cols))
                      (goto-char 13) ;; beginning of "  indented"
                      (setq cols (cons (current-column) cols))
                      (goto-char 15) ;; after "  " indent
                      (setq cols (cons (current-column) cols))
                      (nreverse cols)))"#;
    let expect = expect_test::expect![r#""OK (0 5 0 2)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// move-to-column
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_move_to_column_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world foo bar\n")
                    (goto-char (point-min))
                    (let ((result (move-to-column 6)))
                      (list result (current-column) (point))))"#;
    let expect = expect_test::expect![r#""OK (6 6 7)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_move_to_column_past_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Moving past end of line without FORCE
    let form = r#"(with-temp-buffer
                    (insert "short\nlong line here\n")
                    (goto-char (point-min))
                    (let ((result (move-to-column 100)))
                      (list result (current-column) (point))))"#;
    let expect = expect_test::expect![r#""OK (5 5 6)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_move_to_column_force_padding_inherits_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert (propertize "x" :time "STAMP"))
                    (move-to-column 5 t)
                    (list (buffer-string)
                          (mapcar (lambda (position)
                                    (get-text-property position :time))
                                  (number-sequence (point-min)
                                                   (1- (point-max))))))"#;
    let expect = expect_test::expect![
        r#""OK (#(\"x    \" 0 5 (:time \"STAMP\")) (\"STAMP\" \"STAMP\" \"STAMP\" \"STAMP\" \"STAMP\"))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_move_to_column_force_tab_split_inherits_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (let ((indent-tabs-mode nil))
                      (insert (propertize "x" :time "STAMP") "\tz")
                      (goto-char (point-min))
                      (move-to-column 5 t)
                      (list (buffer-string)
                            (mapcar (lambda (position)
                                      (get-text-property position :time))
                                    (number-sequence (point-min)
                                                     (1- (point-max)))))))"#;
    let expect = expect_test::expect![
        r#""OK (#(\"x       z\" 0 8 (:time \"STAMP\")) (\"STAMP\" \"STAMP\" \"STAMP\" \"STAMP\" \"STAMP\" \"STAMP\" \"STAMP\" \"STAMP\" nil))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: column-based text alignment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_position_align_columns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract column positions from a tabular text
    let form = r#"(with-temp-buffer
                    (insert "Name    Age City\n")
                    (insert "Alice   30  Boston\n")
                    (insert "Bob     25  NYC\n")
                    (insert "Carol   35  London\n")
                    ;; Extract data at fixed column offsets
                    (goto-char (point-min))
                    (forward-line 1) ;; skip header
                    (let ((records nil))
                      (while (not (eobp))
                        (let ((line-start (point)))
                          (let ((name (progn
                                        (move-to-column 0)
                                        (buffer-substring
                                         (point)
                                         (progn (skip-chars-forward "^ ")
                                                (point)))))
                                (age (progn
                                       (goto-char line-start)
                                       (move-to-column 8)
                                       (buffer-substring
                                        (point)
                                        (progn (skip-chars-forward "0-9")
                                               (point)))))
                                (city (progn
                                        (goto-char line-start)
                                        (move-to-column 12)
                                        (buffer-substring
                                         (point)
                                         (line-end-position)))))
                            (setq records
                                  (cons (list name
                                              (string-to-number age)
                                              city)
                                        records))))
                        (forward-line 1))
                      (nreverse records)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice\" 30 \"Boston\") (\"Bob\" 25 \"NYC\") (\"Carol\" 35 \"London\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: line-based operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_position_line_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute various per-line metrics
    let form = r#"(with-temp-buffer
                    (insert "short\n")
                    (insert "a medium length line\n")
                    (insert "\n")
                    (insert "the longest line in this buffer here\n")
                    (insert "end\n")
                    (goto-char (point-min))
                    (let ((metrics nil)
                          (line-num 1))
                      (while (not (eobp))
                        (let* ((bol (line-beginning-position))
                               (eol (line-end-position))
                               (len (- eol bol)))
                          (setq metrics
                                (cons (list line-num len
                                            (= bol eol))
                                      metrics)))
                        (setq line-num (1+ line-num))
                        (forward-line 1))
                      ;; Also find the longest line
                      (let ((max-len 0) (max-line 0))
                        (dolist (m (nreverse metrics))
                          (when (> (nth 1 m) max-len)
                            (setq max-len (nth 1 m)
                                  max-line (nth 0 m))))
                        (list (nreverse metrics)
                              (list 'longest max-line max-len)))))"#;
    let expect = expect_test::expect![r#""OK (((5 3 nil)) (longest 4 36))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
