//! Complex combo batch 426 — 19 probes targeting remaining edge areas:
//! format with float/integer conversions, string-replace, string-edit,
//! string-remove-prefix/suffix, string-truncate-left, window-min-delta,
//! window-max-delta, window-pixel-width, window-absolute-pixel-edges,
//! window-mode-line-height, window-header-line-height, line-number-at-pos
//! with display property, pos-bol/bolp/eolp with display, current-indentation,
//! indent-line-to, move-to-column with display, char-equal case-fold deep,
//! and compare-strings with multibyte case-fold.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// string-replace (Emacs 28+) / string-edit-distance.
#[test]
fn div_cx426_string_replace_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-length-argument 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-replace "foo" "bar" "foo foo foo")
      (string-replace "" "x" "abc"))
"##,
        expect,
    );
}

/// string-remove-prefix / string-remove-suffix with multibyte.
#[test]
fn div_cx426_string_remove_prefix_suffix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function string-remove-prefix)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-remove-prefix "caf" "café")
      (string-remove-suffix "tion" "position")
      (string-remove-prefix "αβ" "αβγ")
      (string-remove-prefix "xxx" "hello"))
"##,
        expect,
    );
}

/// string-truncate-left: truncating string from the left.
#[test]
fn div_cx426_string_truncate_left() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-truncate-left 5 "café世界")
      (string-truncate-left 3 "abcdef"))
"##,
        expect,
    );
}

/// window-min-delta / window-max-delta: window size constraints.
#[test]
fn div_cx426_window_min_max_delta() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (list (window-min-delta w)
        (window-max-delta w)))
"##,
        expect,
    );
}

/// window-pixel-width / window-pixel-height.
#[test]
fn div_cx426_window_pixel_dimensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (list (window-pixel-width w)
        (window-pixel-height w)))
"##,
        expect,
    );
}

/// window-absolute-pixel-edges: absolute pixel coordinates.
#[test]
fn div_cx426_window_absolute_pixel_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (window-absolute-pixel-edges w))
"##,
        expect,
    );
}

/// window-mode-line-height / window-header-line-height.
#[test]
fn div_cx426_window_mode_header_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (list (window-mode-line-height w)
        (window-header-line-height w)))
"##,
        expect,
    );
}

/// line-number-at-pos with display property and multibyte.
#[test]
fn div_cx426_line_number_at_pos_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\nline3")
  (put-text-property 3 4 'display "XXXX")
  (line-number-at-pos (point-max)))
"##,
        expect,
    );
}

/// bolp / eolp with display property that changes visual length.
#[test]
fn div_cx426_bolp_eolp_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc")
  (put-text-property 3 4 'display "XXXX")
  (list (bolp) (eolp)
        (progn (goto-char 4) (bolp) (eolp))))
"##,
        expect,
    );
}

/// current-indentation / indent-line-to / move-to-column.
#[test]
fn div_cx426_current_indentation_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "   some text")
  (list (current-indentation)
        (progn (indent-line-to 5) (current-indentation))))
"##,
        expect,
    );
}

/// char-equal with case-fold across the full Greek range.
#[test]
fn div_cx426_char_equal_greek_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (char-equal ?α ?Α) (char-equal ?Α ?α)
        (char-equal ?β ?Β) (char-equal ?γ ?Γ)
        (char-equal ?π ?Π) (char-equal ?Π ?π)
        (char-equal ?ρ ?Ρ) (char-equal ?Ρ ?ρ)
        (char-equal ?σ ?Σ) (char-equal ?Σ ?σ)
        (char-equal ?ω ?Ω) (char-equal ?Ω ?ω)))
"##,
        expect,
    );
}

/// compare-strings with multibyte and case-fold.
#[test]
fn div_cx426_compare_strings_multibyte_fold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 5 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (compare-strings "café" nil nil "CAFÉ" nil nil t)
        (compare-strings "straße" nil nil "STRASSE" nil nil t)
        (compare-strings "αβγ" nil nil "ΑΒΓ" nil nil t)))
"##,
        expect,
    );
}

/// format with mixed type conversions to string.
#[test]
fn div_cx426_format_mixed_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 1.0/3.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%s %d %f %c" "hello" 42 3.14 65)
      (format "%.10f" 1.0/3.0))
"##,
        expect,
    );
}

/// string-to-number with various radices and edge inputs.
#[test]
fn div_cx426_string_to_number_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3735928559 511 42 -42 31400000000.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-to-number "deadbeef" 16)
      (string-to-number "777" 8)
      (string-to-number "101010" 2)
      (string-to-number "  -42  ")
      (string-to-number "3.14e10"))
"##,
        expect,
    );
}

/// truncate-string-to-width with display property and ellipsis.
#[test]
fn div_cx426_truncate_string_to_width_ellipsis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"ab…\" #(\"abcdef\" 2 4 (display \"XXXX\")) #(\"abcde\" 2 4 (display \"XXXX\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "abcdef"))
  (put-text-property 2 4 'display "XXXX" s)
  (list (truncate-string-to-width s 3 nil nil ?.)
        (truncate-string-to-width s 6 nil nil t)
        (truncate-string-to-width s 5)))
"##,
        expect,
    );
}

/// format-prompt / format-message deeper with arguments.
#[test]
fn div_cx426_format_prompt_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Open file (default /tmp/test): \" \"Visit ‘the manual’ for details\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format-prompt "Open file" "/tmp/test")
      (format-message "Visit `%s' for details" "the manual"))
"##,
        expect,
    );
}

/// bool-vector count-consecutive with various patterns.
#[test]
fn div_cx426_bool_vector_count_consecutive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((bv (bool-vector t t nil nil t t t nil)))
  (list (bool-vector-count-consecutive bv t 0)
        (bool-vector-count-consecutive bv nil 2)
        (bool-vector-count-consecutive bv t 4)))
"##,
        expect,
    );
}

/// process-id / process-name.
#[test]
fn div_cx426_process_id_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"neo-cx426-pi\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "neo-cx426-pi"
                          :command '("echo" "test")
                          :connection-type 'pipe :buffer nil)))
  (accept-process-output proc 2)
  ;; Assert the testable invariant (a valid positive PID), not the raw PID:
  ;; GNU and neomacs spawn distinct OS processes with distinct PIDs, so a raw
  ;; (process-id) can never match across the two engines (or run-to-run).
  (prog1 (list (process-name proc)
               (integerp (process-id proc))
               (> (process-id proc) 0))
    (delete-process proc)))
"##,
        expect,
    );
}
