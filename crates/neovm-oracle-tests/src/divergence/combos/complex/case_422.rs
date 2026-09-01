//! Complex combo batch 422 — 20 probes into remaining edge areas:
//! marker insertion-type with edit patterns, overlay priority face
//! merging, text-property front/rear sticky, keymap precedence
//! ordering, hash-table all weakness types, syntax pps nested
//! comments/strings, regex symbol boundaries, char syntax classes,
//! process filter modifying its buffer, bidi paragraph-start/end,
//! and multi-level keymap inheritance.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// marker insertion-type: before vs after insert at marker.
#[test]
fn div_cx422_marker_insertion_type_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcde")
  (let ((m-before (make-marker))
        (m-after (make-marker)))
    (set-marker m-before 3)
    (set-marker m-after 3)
    (set-marker-insertion-type m-before nil)
    (set-marker-insertion-type m-after t)
    (goto-char 3)
    (insert "XY")
    (list (marker-position m-before)
          (marker-position m-after))))
"##,
        expect,
    );
}

/// overlay priority: multiple overlays with same priority.
#[test]
fn div_cx422_overlay_priority_same() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK italic""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((o1 (make-overlay 2 5)) (o2 (make-overlay 2 5)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic)
    (overlay-put o1 'priority 5)
    (overlay-put o2 'priority 5)
    (get-char-property 3 'face)))
"##,
        expect,
    );
}

/// text-property front-sticky / rear-sticky across insert.
#[test]
fn div_cx422_text_prop_sticky_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil italic)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcde")
  (put-text-property 1 3 'face 'bold)
  (put-text-property 3 5 'face 'italic)
  (put-text-property 1 5 'front-sticky '(face))
  (put-text-property 1 5 'rear-nonsticky '(face))
  (goto-char 3)
  (insert "X")
  (list (get-text-property 3 'face)
        (get-text-property 4 'face)))
"##,
        expect,
    );
}

/// keymap precedence: minor-mode, local, global ordering.
#[test]
fn div_cx422_keymap_precedence_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK forward-char""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((minor (make-sparse-keymap))
        (local (make-sparse-keymap)))
    (define-key minor "a" 'backward-char)
    (define-key local "a" 'forward-char)
    (use-local-map local)
    (let ((minor-mode-map-alist (list (cons (make-symbol "test") minor))))
      (key-binding "a"))))
"##,
        expect,
    );
}

/// hash-table weakness: all 4 weakness types.
#[test]
fn div_cx422_hash_weakness_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((k1 (cons 1 nil))
      (k2 (cons 2 nil))
      (v1 (cons 'a nil))
      (v2 (cons 'b nil)))
  (let ((ht-key (make-hash-table :weakness 'key :test 'eq))
        (ht-val (make-hash-table :weakness 'value :test 'eq))
        (ht-ko (make-hash-table :weakness 'key-or-value :test 'eq))
        (ht-ka (make-hash-table :weakness 'key-and-value :test 'eq)))
    (puthash k1 v1 ht-key)
    (puthash k2 v2 ht-val)
    (puthash k1 v1 ht-ko)
    (puthash k1 v1 ht-ka)
    (list (hash-table-count ht-key)
          (hash-table-count ht-val)
          (hash-table-count ht-ko)
          (hash-table-count ht-ka))))
"##,
        expect,
    );
}

/// syntax pps with nested comments and strings.
#[test]
fn div_cx422_syntax_pps_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 1 2 nil nil nil 0 nil nil (1) nil) (1 1 10 34 nil nil 0 nil 13 (1) nil) (1 1 10 34 nil nil 0 nil 13 (1) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(defun f () \"hello /* comment */ world\" ; (not a comment)
          42)")
  (list (parse-partial-sexp 1 8)
        (parse-partial-sexp 1 18)
        (parse-partial-sexp 1 35)))
"##,
        expect,
    );
}

/// regex with symbol boundaries \\_< \\_>.
#[test]
fn div_cx422_regex_symbol_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"foo_bar\" \"foo_bar\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "foo_bar foo-bar foo_bar")
  (let (results)
    (goto-char 1)
    (while (re-search-forward "\\_<foo_bar\\_>" nil t)
      (push (match-string 0) results))
    (nreverse results)))
"##,
        expect,
    );
}

/// char syntax classes for various syntax types.
#[test]
fn div_cx422_char_syntax_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 9 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-syntax ?a)
      (char-syntax ?\()
      (char-syntax ?\))
      (char-syntax ?\")
      (char-syntax ?\;)
      (char-syntax ?\ )
      (char-syntax ?_)
      (char-syntax ?\()))
"##,
        expect,
    );
}

/// process filter modifying its own buffer.
#[test]
fn div_cx422_process_filter_modify_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"initial: hello\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *cx422-pfmb*")))
  (with-current-buffer buf (insert "initial: "))
  (let ((proc (make-process :name "neo-cx422-pf"
                            :command '("echo" "hello")
                            :connection-type 'pipe :buffer buf)))
    (set-process-sentinel proc #'ignore)
    (set-process-query-on-exit-flag proc nil)
    (accept-process-output proc 2)
    (prog1 (with-current-buffer buf
             (string-trim-right (buffer-string)))
      (kill-buffer buf))))
"##,
        expect,
    );
}

/// bidi paragraph-start / paragraph-end.
#[test]
fn div_cx422_bidi_paragraph_direction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (left-to-right 17)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc العربية def\n")
  (list (current-bidi-paragraph-direction)
        (progn (goto-char 1)
               (forward-paragraph 1)
               (point))))
"##,
        expect,
    );
}

/// multi-level keymap inheritance chain.
#[test]
fn div_cx422_keymap_multi_level_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (self-insert-command self-insert-command self-insert-command)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((base (make-sparse-keymap))
      (mid (make-sparse-keymap))
      (top (make-sparse-keymap)))
  (define-key base "a" 'base-fn)
  (define-key mid "b" 'mid-fn)
  (define-key top "c" 'top-fn)
  (set-keymap-parent mid base)
  (set-keymap-parent top mid)
  (list (key-binding "a" nil nil top)
        (key-binding "b" nil nil top)
        (key-binding "c" nil nil top)))
"##,
        expect,
    );
}

/// text-property with multiple overlapping properties.
#[test]
fn div_cx422_text_prop_overlap_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold underline underline bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghij")
  (put-text-property 1 10 'face 'bold)
  (put-text-property 3 7 'face 'italic)
  (put-text-property 4 6 'face 'underline)
  (put-text-property 5 8 'mouse-face 'highlight)
  (list (get-text-property 2 'face)
        (get-text-property 4 'face)
        (get-text-property 5 'face)
        (get-text-property 7 'face)))
"##,
        expect,
    );
}

/// overlay with before-string and after-string simultaneously.
#[test]
fn div_cx422_overlay_both_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"abc\" 2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc")
  (let ((ov (make-overlay 2 3)))
    (overlay-put ov 'before-string ">>")
    (overlay-put ov 'after-string "<<")
    (list (buffer-string)
          (overlay-start ov)
          (overlay-end ov))))
"##,
        expect,
    );
}

/// regex with optional matching: \? after various constructs.
#[test]
fn div_cx422_regex_optional_quantifier() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"ac\" 0 \"abc\" 0 \"ad\" 0 \"abcd\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match "ab?c" "ac")
      (match-string 0 "ac")
      (string-match "ab?c" "abc")
      (match-string 0 "abc")
      (string-match "a\\(bc\\)?d" "ad")
      (match-string 0 "ad")
      (string-match "a\\(bc\\)?d" "abcd")
      (match-string 0 "abcd"))
"##,
        expect,
    );
}

/// float arithmetic with NaN and Inf.
#[test]
fn div_cx422_float_nan_inf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (-0.0e+NaN 1.0e+INF -1.0e+INF)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (/ 0.0 0) (error (car e)))
      (condition-case e (/ 1.0 0) (error (car e)))
      (condition-case e (/ -1.0 0) (error (car e))))
"##,
        expect,
    );
}

/// format with %e %f %g scientific notation.
#[test]
fn div_cx422_format_scientific() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"3.141593e+00\" \"3.141593\" \"3.14159\" \"1.00e+06\" \"3.14\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((pi 3.141592653589793))
  (list (format "%e" pi)
        (format "%f" pi)
        (format "%g" pi)
        (format "%.2e" 1000000.0)
        (format "%.2f" pi)))
"##,
        expect,
    );
}

/// process-coding-system for stdin/stdout/stderr.
#[test]
fn div_cx422_process_coding_std() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (utf-8-unix latin-1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "neo-cx422-pcs"
                          :command '("cat")
                          :connection-type 'pipe :buffer nil
                          :coding '(utf-8-unix . latin-1))))
  (process-send-string proc "test\n")
  (accept-process-output proc 1)
  (let ((coding (process-coding-system proc)))
    (delete-process proc)
    (list (car coding) (cdr coding))))
"##,
        expect,
    );
}

/// char-after/char-before with display property substitution.
#[test]
fn div_cx422_char_after_before_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (97 98 99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcde")
  (put-text-property 2 3 'display "XYZ")
  (list (char-after 1)
        (char-after 2)
        (progn (goto-char 4) (char-before))))
"##,
        expect,
    );
}

/// list notation with dotted pair printing.
#[test]
fn div_cx422_dotted_pair_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"(a . b)\" \"(a b . c)\" \"(a b c)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (prin1-to-string '(a . b))
      (prin1-to-string '(a b . c))
      (prin1-to-string '(a b c)))
"##,
        expect,
    );
}

/// vector operations: vconcat, vector, aref, aset edge.
#[test]
fn div_cx422_vector_ops_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([1 99 3] [1 99 3 4 5] 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v (vector 1 2 3)))
  (aset v 1 99)
  (list v
        (vconcat v [4 5])
        (aref (vconcat [1] [2] [3]) 1)))
"##,
        expect,
    );
}
