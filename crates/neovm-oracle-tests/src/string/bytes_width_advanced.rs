//! Oracle parity tests for `string-bytes`, `string-width`, and `char-width`
//! with complex patterns.
//!
//! Tests string-bytes for ASCII vs multibyte, empty strings, string-width for
//! CJK and combining chars, string-width vs length comparisons, char-width for
//! various character classes, fixed-width column building, and boundary-aware
//! truncation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// string-bytes for ASCII vs multibyte strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_bytes_width_ascii_vs_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; ASCII: string-bytes = length
  (string-bytes "hello")
  (length "hello")
  (= (string-bytes "hello") (length "hello"))
  ;; Latin-1 supplement (2 bytes in UTF-8)
  (string-bytes "\u00e9")
  (length "\u00e9")
  ;; CJK ideographs (3 bytes in UTF-8)
  (string-bytes "\u4e16")
  (length "\u4e16")
  (string-bytes "\u4e16\u754c")
  (length "\u4e16\u754c")
  ;; Mixed ASCII and multibyte
  (string-bytes "hello\u4e16\u754c")
  (length "hello\u4e16\u754c")
  ;; 4-byte UTF-8 characters (supplementary plane)
  (string-bytes (string #x1f600))
  (length (string #x1f600))
  ;; Comparison: bytes >= length for all strings
  (let ((strings (list "" "abc" "\u00e9" "\u4e16\u754c" "A\u4e2dB"
                       (string #x1f600))))
    (mapcar (lambda (s)
              (list (length s) (string-bytes s)
                    (>= (string-bytes s) (length s))))
            strings)))"#;
    let expect = expect_test::expect![[
        r#""OK (5 5 t 2 1 3 1 6 2 11 7 4 1 ((0 0 t) (3 3 t) (1 2 t) (2 6 t) (3 5 t) (1 4 t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-bytes for empty string and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_bytes_width_empty_and_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Empty string
  (string-bytes "")
  (length "")
  (string-width "")
  (= (string-bytes "") 0)
  (= (length "") 0)
  (= (string-width "") 0)
  ;; Single space
  (string-bytes " ")
  ;; String of spaces
  (string-bytes "     ")
  (= (string-bytes "     ") 5)
  ;; NUL character (single byte in Emacs internal)
  (string-bytes (string 0))
  (length (string 0))
  ;; String made of repeated multibyte char
  (let ((s (make-string 10 #x4e16)))
    (list (length s) (string-bytes s)))
  ;; Concatenation of different byte-length chars
  (let ((s (concat "A" "\u00e9" "\u4e16" (string #x1f600))))
    (list (length s) (string-bytes s)
          ;; Each char contributes different byte count
          (string-bytes "A")
          (string-bytes "\u00e9")
          (string-bytes "\u4e16")
          (string-bytes (string #x1f600))))
  ;; make-string with ASCII
  (string-bytes (make-string 100 ?A))
  ;; make-string with multibyte
  (string-bytes (make-string 50 #x4e16)))"#;
    let expect =
        expect_test::expect![[r#""OK (0 0 0 t t t 1 5 t 1 1 (10 30) (4 10 1 2 3 4) 100 150)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-width for CJK (width 2) and combining chars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_bytes_width_cjk_combining() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; CJK ideographs: each is display width 2
  (string-width "\u4e16")
  (string-width "\u4e16\u754c")
  (string-width "\u4e16\u754c\u4f60\u597d")
  ;; Japanese Hiragana (width 2)
  (string-width "\u3042\u3044\u3046")
  ;; Korean Hangul (width 2)
  (string-width "\uc548\ub155")
  ;; Fullwidth Latin (width 2)
  (string-width "\uff21\uff22\uff23")
  ;; Combining characters (width 0)
  (string-width (string ?e #x0301))
  (string-width (string ?a #x0300 #x0301 #x0302))
  ;; Pre-composed vs decomposed
  (list (string-width "\u00e9")
        (string-width (string ?e #x0301))
        (= (string-width "\u00e9") (string-width (string ?e #x0301))))
  ;; CJK + combining
  (string-width (string #x4e16 #x0301))
  ;; Halfwidth Katakana (width 1)
  (string-width "\uff71\uff72\uff73")
  ;; Mixed: ASCII(1) + CJK(2) + combining(0) + fullwidth(2)
  (let ((s (concat "A" "\u4e16" (string ?e #x0301) "\uff21")))
    (list (length s) (string-width s) (string-bytes s))))"#;
    let expect = expect_test::expect![[r#""OK (2 4 8 6 4 6 1 1 (1 1 t) 2 3 (5 6 10))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-width vs length comparison across character types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_bytes_width_vs_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((test-cases
        (list
          ;; (string expected-length expected-width description)
          (list "" 0 0)
          (list "abc" 3 3)
          (list "\u4e16\u754c" 2 4)
          (list "\u00e9\u00f1" 2 2)
          (list "\uff21\uff22" 2 4)
          (list (string ?a #x0301) 2 1)
          (list "A\u4e16B" 3 4)
          (list "\uff71\uff72" 2 2))))
  (mapcar (lambda (tc)
            (let ((s (car tc))
                  (exp-len (cadr tc))
                  (exp-wid (caddr tc)))
              (list
                (= (length s) exp-len)
                (= (string-width s) exp-wid)
                ;; Relationships
                (>= (string-width s) 0)
                ;; For pure ASCII: width = length
                ;; For CJK: width > length
                ;; For combining: width < length
                (cond
                  ((= (string-width s) (length s)) 'equal)
                  ((> (string-width s) (length s)) 'wider)
                  ((< (string-width s) (length s)) 'narrower)))))
          test-cases))"#;
    let expect = expect_test::expect![[
        r#""OK ((t t t equal) (t t t equal) (t t t wider) (t t t equal) (t t t wider) (t t t narrower) (t t t wider) (t t t equal))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-width for various character classes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_bytes_width_char_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; ASCII printable (width 1)
  (char-width ?A)
  (char-width ?z)
  (char-width ?0)
  (char-width ?\s)
  (char-width ?!)
  (char-width ?~)
  ;; CJK ideographs (width 2)
  (char-width #x4e16)
  (char-width #x4e2d)
  (char-width #x6587)
  ;; Hiragana (width 2)
  (char-width #x3042)
  ;; Katakana (width 2)
  (char-width #x30a2)
  ;; Hangul (width 2)
  (char-width #xac00)
  ;; Fullwidth Latin (width 2)
  (char-width #xff21)
  ;; Halfwidth Katakana (width 1)
  (char-width #xff71)
  ;; Latin-1 supplement (width 1)
  (char-width #x00e9)
  (char-width #x00f1)
  ;; Combining marks (width 0)
  (char-width #x0300)
  (char-width #x0301)
  (char-width #x0308)
  ;; Control characters
  (char-width 0)
  (char-width 7)
  (char-width 8)
  (char-width ?\t)
  (char-width ?\n)
  (char-width 27)
  (char-width 127)
  ;; Emoji
  (char-width #x2764)
  (char-width #x1f600)
  ;; Variation selectors (width 0)
  (char-width #xfe0e)
  (char-width #xfe0f)
  ;; Batch: classify width for a range of chars
  (let ((chars (list ?A #x4e16 #x0301 ?\t #xff21 #xff71 #x00e9 #xfe0f)))
    (mapcar (lambda (ch) (list ch (char-width ch))) chars)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 1 1 1 1 1 2 2 2 2 2 2 2 1 1 1 0 0 0 2 2 2 8 0 2 2 1 2 0 0 ((65 1) (19990 2) (769 0) (9 8) (65313 2) (65393 1) (233 1) (65039 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: building fixed-width columns with variable-width chars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_bytes_width_fixed_columns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Pad or truncate to exact display width
  (fset 'neovm--sbw-pad-to
    (lambda (str width)
      "Pad STR with spaces to exactly WIDTH display columns."
      (let ((w (string-width str)))
        (cond
          ((< w width) (concat str (make-string (- width w) ?\s)))
          ((> w width)
           ;; Truncate character by character
           (let ((i 0)
                 (accum 0)
                 (len (length str)))
             (while (and (< i len)
                         (<= (+ accum (char-width (aref str i))) width))
               (setq accum (+ accum (char-width (aref str i))))
               (setq i (1+ i)))
             (concat (substring str 0 i)
                     (make-string (- width accum) ?\s))))
          (t str)))))

  ;; Format a row with fixed column widths
  (fset 'neovm--sbw-format-row
    (lambda (fields widths sep)
      (let ((parts nil)
            (f fields)
            (w widths))
        (while (and f w)
          (setq parts (cons (funcall 'neovm--sbw-pad-to (car f) (car w))
                            parts))
          (setq f (cdr f))
          (setq w (cdr w)))
        (mapconcat #'identity (nreverse parts) sep))))

  ;; Format a full table
  (fset 'neovm--sbw-format-table
    (lambda (header rows widths)
      (let ((hdr (funcall 'neovm--sbw-format-row header widths " | "))
            (sep (make-string (+ (apply #'+ widths)
                                 (* (1- (length widths)) 3))
                              ?-)))
        (concat hdr "\n" sep "\n"
                (mapconcat
                 (lambda (row)
                   (funcall 'neovm--sbw-format-row row widths " | "))
                 rows "\n")))))

  (unwind-protect
      (let ((widths '(10 8 6)))
        (list
          ;; All ASCII
          (funcall 'neovm--sbw-format-row '("Alice" "Boston" "100") widths " | ")
          ;; CJK names (width 2 each)
          (funcall 'neovm--sbw-format-row
                   '("\u5f20\u4e09" "\u4e1c\u4eac" "88") widths " | ")
          ;; Mixed
          (funcall 'neovm--sbw-format-row
                   '("A\u4e2d\u6587" "Mix\u6d4b" "42") widths " | ")
          ;; Verify all rows have same display width
          (let* ((r1 (funcall 'neovm--sbw-format-row '("Test" "Data" "1") widths " | "))
                 (r2 (funcall 'neovm--sbw-format-row '("\u4e16\u754c" "\u4f60\u597d" "2") widths " | ")))
            (list (string-width r1)
                  (string-width r2)
                  (= (string-width r1) (string-width r2))))
          ;; Full table
          (funcall 'neovm--sbw-format-table
                   '("Name" "City" "Score")
                   '(("Alice" "NYC" "95")
                     ("\u5f20\u4e09" "\u5317\u4eac" "88")
                     ("Bob" "LA" "72"))
                   widths)
          ;; Truncation case: long string gets cut
          (funcall 'neovm--sbw-pad-to "VeryLongNameHere" 8)
          ;; CJK truncation: can't split a wide char
          (funcall 'neovm--sbw-pad-to "\u4e16\u754c\u4f60\u597d\u4e2d" 7)))
    (fmakunbound 'neovm--sbw-pad-to)
    (fmakunbound 'neovm--sbw-format-row)
    (fmakunbound 'neovm--sbw-format-table)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice      | Boston   | 100   \" \"张三       | 东京     | 88    \" \"A中文      | Mix测    | 42    \" (30 30 t) \"Name       | City     | Score \\n------------------------------\\nAlice      | NYC      | 95    \\n张三       | 北京     | 88    \\nBob        | LA       | 72    \" \"VeryLong\" \"世界你 \")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: truncation respecting character boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_bytes_width_boundary_truncation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Truncate to a maximum number of bytes, respecting char boundaries
  (fset 'neovm--sbw-truncate-bytes
    (lambda (str max-bytes suffix)
      "Truncate STR to at most MAX-BYTES, appending SUFFIX if truncated.
       Never breaks a multibyte character."
      (if (<= (string-bytes str) max-bytes)
          (cons str nil)
        (let* ((suffix-bytes (string-bytes suffix))
               (budget (- max-bytes suffix-bytes))
               (i 0)
               (accum 0)
               (len (length str)))
          (while (and (< i len)
                      (<= (+ accum (string-bytes (string (aref str i)))) budget))
            (setq accum (+ accum (string-bytes (string (aref str i)))))
            (setq i (1+ i)))
          (cons (concat (substring str 0 i) suffix) t)))))

  ;; Truncate to a maximum display width, respecting char boundaries
  (fset 'neovm--sbw-truncate-width
    (lambda (str max-width suffix)
      "Truncate STR to MAX-WIDTH display columns, appending SUFFIX."
      (if (<= (string-width str) max-width)
          (cons str nil)
        (let* ((suffix-w (string-width suffix))
               (budget (- max-width suffix-w))
               (i 0)
               (accum 0)
               (len (length str)))
          (while (and (< i len)
                      (<= (+ accum (char-width (aref str i))) budget))
            (setq accum (+ accum (char-width (aref str i))))
            (setq i (1+ i)))
          (cons (concat (substring str 0 i) suffix) t)))))

  (unwind-protect
      (list
        ;; Byte truncation: ASCII (1 byte each)
        (funcall 'neovm--sbw-truncate-bytes "hello world" 8 "..")
        ;; Byte truncation: CJK (3 bytes each)
        (funcall 'neovm--sbw-truncate-bytes "\u4e16\u754c\u4f60\u597d" 8 "..")
        ;; Byte truncation: mixed
        (funcall 'neovm--sbw-truncate-bytes "Hi\u4e16\u754c" 6 "..")
        ;; Byte truncation: fits
        (funcall 'neovm--sbw-truncate-bytes "short" 20 "..")
        ;; Width truncation: ASCII
        (funcall 'neovm--sbw-truncate-width "hello world" 8 "..")
        ;; Width truncation: CJK (width 2 each)
        (funcall 'neovm--sbw-truncate-width "\u4e16\u754c\u4f60\u597d" 6 "..")
        ;; Width truncation: odd budget with wide chars
        (funcall 'neovm--sbw-truncate-width "\u4e16\u754c\u4f60\u597d" 5 "..")
        ;; Width truncation: mixed
        (funcall 'neovm--sbw-truncate-width "A\u4e16B\u754cC" 6 "..")
        ;; Width truncation: fits
        (funcall 'neovm--sbw-truncate-width "abc" 10 "..")
        ;; Verify truncated results don't exceed limits
        (let ((result (car (funcall 'neovm--sbw-truncate-bytes
                                    "\u4e16\u754c\u4f60\u597d\u4e2d\u6587" 10 ".."))))
          (list result (<= (string-bytes result) 10)))
        (let ((result (car (funcall 'neovm--sbw-truncate-width
                                    "\u4e16\u754c\u4f60\u597d\u4e2d\u6587" 8 ".."))))
          (list result (<= (string-width result) 8)))
        ;; Compare byte-based and width-based truncation
        (let* ((s "Hello\u4e16\u754cWorld")
               (tb (funcall 'neovm--sbw-truncate-bytes s 10 ".."))
               (tw (funcall 'neovm--sbw-truncate-width s 10 "..")))
          (list (car tb) (car tw)
                (string-bytes (car tb))
                (string-width (car tw)))))
    (fmakunbound 'neovm--sbw-truncate-bytes)
    (fmakunbound 'neovm--sbw-truncate-width)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello ..\" . t) (\"世界..\" . t) (\"Hi..\" . t) (\"short\") (\"hello ..\" . t) (\"世界..\" . t) (\"世..\" . t) (\"A世B..\" . t) (\"abc\") (\"世界..\" t) (\"世界你..\" t) (\"Hello世..\" \"Hello世..\" 10 9))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
