//! Comprehensive oracle parity tests for buffer search operations:
//! `search-forward`/`search-backward` with BOUND, NOERROR, COUNT,
//! `re-search-forward`/`re-search-backward` with complex regexps,
//! `looking-at` at various positions, `skip-chars-forward`/`skip-chars-backward`
//! with character ranges, search within narrowed regions,
//! `case-fold-search` interaction.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// search-forward: all parameters, edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_forward_comprehensive_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "alpha beta alpha gamma alpha delta alpha epsilon alpha")
  (goto-char (point-min))
  (list
    ;; Basic: find first occurrence, returns position after match
    (let ((r (search-forward "alpha" nil t)))
      (list r (point)))
    ;; COUNT=2: find second occurrence from current position
    (let ((r (search-forward "alpha" nil t 2)))
      (list r (point)))
    ;; COUNT with BOUND: bounded search for Nth occurrence
    (progn
      (goto-char (point-min))
      (search-forward "alpha" 30 t 3))
    ;; COUNT=0: no-op, returns point
    (progn
      (goto-char (point-min))
      (let ((before (point)))
        (list (search-forward "alpha" nil t 0) before (point))))
    ;; Negative COUNT: search backward from point
    (progn
      (goto-char (point-max))
      (let ((r (search-forward "alpha" nil t -2)))
        (list r (point))))
    ;; NOERROR=t: returns nil when not found, point unchanged
    (progn
      (goto-char (point-min))
      (let ((before (point))
            (r (search-forward "NONEXISTENT" nil t)))
        (list r (= before (point)))))
    ;; BOUND too small: cannot find even though string exists
    (progn
      (goto-char (point-min))
      (search-forward "beta" 3 t))
    ;; Search for empty string
    (progn
      (goto-char 5)
      (let ((r (search-forward "" nil t)))
        (list r (point))))))"#;
    let expect =
        expect_test::expect![r#""OK ((6 6) (29 29) 29 (1 1 1) (36 36) (nil t) nil (5 5))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// search-backward: all parameters, edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_backward_comprehensive_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "one two three one two three one two three")
  (list
    ;; Basic from end
    (progn
      (goto-char (point-max))
      (let ((r (search-backward "two" nil t)))
        (list r (point) (buffer-substring (point) (+ (point) 3)))))
    ;; COUNT=3: find third-from-last occurrence
    (progn
      (goto-char (point-max))
      (let ((r (search-backward "two" nil t 3)))
        (list r (point))))
    ;; BOUND limits how far back to search
    (progn
      (goto-char (point-max))
      (let ((r (search-backward "one" 20 t)))
        (list r (point))))
    ;; BOUND excludes all occurrences
    (progn
      (goto-char (point-max))
      (search-backward "one" 40 t))
    ;; Negative COUNT in search-backward searches forward
    (progn
      (goto-char (point-min))
      (let ((r (search-backward "three" nil t -2)))
        (list r (point))))
    ;; NOERROR=t on failure
    (progn
      (goto-char (point-max))
      (let ((before (point))
            (r (search-backward "MISSING" nil t)))
        (list r (= before (point)))))
    ;; Successive backward searches
    (progn
      (goto-char (point-max))
      (let ((positions nil))
        (while (search-backward "one" nil t)
          (push (point) positions))
        positions))))"#;
    let expect = expect_test::expect![[
        r#""OK ((33 33 \"two\") (5 5) (29 29) nil (28 28) (nil t) (1 15 29))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// re-search-forward with complex regexps and groups
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_forward_complex_regexps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "2025-01-15 error: disk full\n")
  (insert "2025-02-28 warn: memory high\n")
  (insert "2025-03-01 error: cpu overload\n")
  (insert "2025-03-02 info: all clear\n")
  (insert "2025-03-02 error: network down\n")
  (goto-char (point-min))
  (list
    ;; Capture date and level with groups
    (progn
      (goto-char (point-min))
      (let ((matches nil))
        (while (re-search-forward
                "\\([0-9]\\{4\\}-[0-9]\\{2\\}-[0-9]\\{2\\}\\) \\(error\\|warn\\|info\\): \\(.+\\)$"
                nil t)
          (push (list (match-string 1) (match-string 2) (match-string 3))
                matches))
        (nreverse matches)))
    ;; Count errors only
    (progn
      (goto-char (point-min))
      (let ((count 0))
        (while (re-search-forward "^[0-9-]+ error:" nil t)
          (setq count (1+ count)))
        count))
    ;; Extract all dates using shy groups
    (progn
      (goto-char (point-min))
      (let ((dates nil))
        (while (re-search-forward "\\([0-9]\\{4\\}\\)-\\([0-9]\\{2\\}\\)-\\([0-9]\\{2\\}\\)" nil t)
          (push (list (match-string 1) (match-string 2) (match-string 3))
                dates))
        (nreverse dates)))
    ;; BOUND parameter with regex
    (progn
      (goto-char (point-min))
      (re-search-forward "error" 20 t))
    ;; COUNT parameter with regex
    (progn
      (goto-char (point-min))
      (let ((r (re-search-forward "20[0-9]\\{2\\}" nil t 3)))
        (list r (point))))
    ;; Alternation with backreference-like groups
    (progn
      (goto-char (point-min))
      (let ((results nil))
        (while (re-search-forward "\\(error\\|warn\\)" nil t)
          (push (match-string 1) results))
        (nreverse results)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"2025-01-15\" \"error\" \"disk full\") (\"2025-02-28\" \"warn\" \"memory high\") (\"2025-03-01\" \"error\" \"cpu overload\") (\"2025-03-02\" \"info\" \"all clear\") (\"2025-03-02\" \"error\" \"network down\")) 3 ((\"2025\" \"01\" \"15\") (\"2025\" \"02\" \"28\") (\"2025\" \"03\" \"01\") (\"2025\" \"03\" \"02\") (\"2025\" \"03\" \"02\")) 17 (62 62) (\"error\" \"warn\" \"error\" \"error\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// re-search-backward with complex patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_backward_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "(defun foo (x y)\n  \"Docstring for foo.\"\n  (+ x y))\n\n")
  (insert "(defun bar (a &optional b)\n  \"Docstring for bar.\"\n  (list a b))\n\n")
  (insert "(defvar baz 42\n  \"Variable baz.\")\n")
  (list
    ;; Find last defun from end
    (progn
      (goto-char (point-max))
      (when (re-search-backward "(defun \\([a-z]+\\)" nil t)
        (list (match-string 0) (match-string 1) (point))))
    ;; Collect all defun names backward
    (progn
      (goto-char (point-max))
      (let ((names nil))
        (while (re-search-backward "(defun \\([a-z]+\\)" nil t)
          (push (match-string 1) names))
        names))
    ;; Find defun or defvar backward
    (progn
      (goto-char (point-max))
      (let ((defs nil))
        (while (re-search-backward "(def\\(un\\|var\\) \\([a-z]+\\)" nil t)
          (push (list (match-string 1) (match-string 2)) defs))
        defs))
    ;; Bounded backward regex search
    (progn
      (goto-char (point-max))
      (let ((r (re-search-backward "(defun" 50 t)))
        (list r (when r (point)))))
    ;; Find docstring closest to end
    (progn
      (goto-char (point-max))
      (when (re-search-backward "\"\\([^\"]+\\)\"" nil t)
        (match-string 1)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"(defun bar\" \"bar\" 53) (\"foo\" \"bar\") ((\"un\" \"foo\") (\"un\" \"bar\") (\"var\" \"baz\")) (53 53) \"Variable baz.\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// looking-at at various positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_looking_at_various_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "  (defun my-func (x)\n    (1+ x))\n\n;; comment\n42\n")
  (list
    ;; At beginning: whitespace
    (progn (goto-char (point-min))
           (list (looking-at "[ \t]+")
                 (match-end 0)))
    ;; At the paren
    (progn (goto-char 3)
           (looking-at "(defun \\([^ ]+\\)"))
    ;; Extract function name via looking-at groups
    (progn (goto-char 3)
           (when (looking-at "(defun \\([a-z-]+\\)")
             (match-string 1)))
    ;; At newline
    (progn (goto-char (progn (goto-char (point-min))
                              (end-of-line) (point)))
           (looking-at "\n"))
    ;; At comment
    (progn (goto-char (point-min))
           (search-forward ";;" nil t)
           (goto-char (match-beginning 0))
           (looking-at ";;.*$"))
    ;; looking-at with end-of-buffer
    (progn (goto-char (point-max))
           (looking-at "."))
    ;; looking-at with character classes
    (progn (goto-char (point-min))
           (search-forward "42" nil t)
           (goto-char (match-beginning 0))
           (list (looking-at "[0-9]+")
                 (match-string 0)))
    ;; looking-at anchored patterns
    (progn (goto-char (point-min))
           (list (looking-at "^")         ;; always true at bol
                 (looking-at "^ +")       ;; leading spaces
                 (looking-at "^(")))))"#;
    let expect =
        expect_test::expect![[r#""OK ((t 3) t \"my-func\" t t nil (t \"42\") (t t nil))""#]];
    // not at ( because spaces first
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// skip-chars-forward / skip-chars-backward with ranges
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "   hello123world   456   ")
  (list
    ;; skip-chars-forward over whitespace from beginning
    (progn (goto-char (point-min))
           (let ((n (skip-chars-forward " \t\n")))
             (list n (point))))
    ;; skip-chars-forward over alphanumeric
    (progn (goto-char 4)
           (let ((n (skip-chars-forward "a-z0-9")))
             (list n (point) (buffer-substring 4 (point)))))
    ;; skip-chars-forward with range and negation
    (progn (goto-char (point-min))
           (let ((n (skip-chars-forward "^ ")))  ;; skip non-space
             (list n (point))))
    ;; skip-chars-forward with LIMIT
    (progn (goto-char 4)
           (let ((n (skip-chars-forward "a-z" 8)))
             (list n (point))))
    ;; skip-chars-backward over whitespace from end
    (progn (goto-char (point-max))
           (let ((n (skip-chars-backward " \t")))
             (list n (point))))
    ;; skip-chars-backward over digits
    (progn (goto-char (point-max))
           (skip-chars-backward " ")
           (let ((end (point))
                 (n (skip-chars-backward "0-9")))
             (list n (point) (buffer-substring (point) end))))
    ;; skip-chars-backward with LIMIT
    (progn (goto-char (point-max))
           (let ((n (skip-chars-backward " 0-9" 15)))
             (list n (point))))
    ;; Combining forward and backward to extract a word at point
    (progn (goto-char 8)  ;; middle of "hello123world"
           (let ((start (progn (skip-chars-backward "a-z0-9") (point)))
                 (end (progn (skip-chars-forward "a-z0-9") (point))))
             (list start end (buffer-substring start end))))
    ;; skip with special chars in range: hyphen, caret
    (progn
      (erase-buffer)
      (insert "foo-bar--baz")
      (goto-char (point-min))
      (let ((n (skip-chars-forward "a-z-")))  ;; letters and hyphens
        (list n (point) (buffer-substring (point-min) (point)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 4) (13 17 \"hello123world\") (0 1) (4 8) (-3 23) (-3 20 \"456\") (-9 17) (4 17 \"hello123world\") (12 13 \"foo-bar--baz\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Search within narrowed regions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_in_narrowed_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "HEADER: important\nline 1: alpha\nline 2: beta\nline 3: alpha\nline 4: gamma\nFOOTER: end\n")
  (list
    ;; Narrow to middle lines (exclude header and footer)
    (save-restriction
      (goto-char (point-min))
      (forward-line 1)
      (let ((start (point)))
        (goto-char (point-max))
        (forward-line -1)
        (narrow-to-region start (point))
        (list
          ;; search-forward within narrowed region
          (progn (goto-char (point-min))
                 (let ((r (search-forward "alpha" nil t)))
                   (list r (point))))
          ;; search-forward cannot see header
          (progn (goto-char (point-min))
                 (search-forward "HEADER" nil t))
          ;; search-forward cannot see footer
          (progn (goto-char (point-min))
                 (search-forward "FOOTER" nil t))
          ;; re-search-forward in narrowed region
          (progn (goto-char (point-min))
                 (let ((matches nil))
                   (while (re-search-forward "line \\([0-9]+\\)" nil t)
                     (push (match-string 1) matches))
                   (nreverse matches)))
          ;; search-backward in narrowed region
          (progn (goto-char (point-max))
                 (let ((r (search-backward "alpha" nil t)))
                   (list r (point))))
          ;; Count occurrences in narrowed region
          (progn (goto-char (point-min))
                 (let ((n 0))
                   (while (search-forward ":" nil t) (setq n (1+ n)))
                   n))
          ;; point-min and point-max reflect narrowing
          (list (point-min) (point-max)))))
    ;; After save-restriction, full buffer visible again
    (progn (goto-char (point-min))
           (search-forward "HEADER" nil t))))"#;
    let expect = expect_test::expect![[
        r#""OK (((32 32) nil nil (\"1\" \"2\" \"3\" \"4\") (54 54) 4 (19 74)) 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// case-fold-search interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_case_fold() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "Hello World HELLO world hElLo WoRlD")
  (list
    ;; Default case-fold-search=t: case-insensitive
    (progn (goto-char (point-min))
           (let ((matches nil))
             (while (search-forward "hello" nil t)
               (push (list (match-beginning 0)
                           (buffer-substring (match-beginning 0) (point)))
                     matches))
             (nreverse matches)))
    ;; With case-fold-search=nil: case-sensitive
    (let ((case-fold-search nil))
      (goto-char (point-min))
      (let ((matches nil))
        (while (search-forward "hello" nil t)
          (push (buffer-substring (match-beginning 0) (point)) matches))
        (nreverse matches)))
    ;; re-search with case-fold-search=t
    (progn (goto-char (point-min))
           (let ((case-fold-search t)
                 (matches nil))
             (while (re-search-forward "hello" nil t)
               (push (buffer-substring (match-beginning 0) (match-end 0)) matches))
             (nreverse matches)))
    ;; re-search with case-fold-search=nil
    (let ((case-fold-search nil))
      (goto-char (point-min))
      (let ((matches nil))
        (while (re-search-forward "hello" nil t)
          (push (buffer-substring (match-beginning 0) (match-end 0)) matches))
        (nreverse matches)))
    ;; search-backward with case-fold
    (progn
      (goto-char (point-max))
      (let ((case-fold-search t))
        (let ((r (search-backward "WORLD" nil t)))
          (list r (when r (buffer-substring r (+ r 5)))))))
    ;; Case-sensitive search-backward
    (progn
      (goto-char (point-max))
      (let ((case-fold-search nil))
        (let ((positions nil))
          (while (search-backward "World" nil t)
            (push (point) positions))
          positions)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 \"Hello\") (13 \"HELLO\") (25 \"hElLo\")) nil (\"Hello\" \"HELLO\" \"hElLo\") nil (31 \"WoRlD\") (7))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multi-strategy search pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_complex_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "/* Block comment */\n")
  (insert "int x = 10;   // line comment\n")
  (insert "int y = 20;\n")
  (insert "/* Another\n   block comment */\n")
  (insert "int z = x + y; // sum\n")
  (insert "return z;      // result\n")
  (list
    ;; Extract all single-line comments using re-search
    (progn (goto-char (point-min))
           (let ((comments nil))
             (while (re-search-forward "//\\s-*\\(.+\\)$" nil t)
               (push (match-string 1) comments))
             (nreverse comments)))
    ;; Extract all variable assignments using search + regex
    (progn (goto-char (point-min))
           (let ((assignments nil))
             (while (re-search-forward "int \\([a-z]+\\) = \\([^;]+\\);" nil t)
               (push (cons (match-string 1) (match-string 2)) assignments))
             (nreverse assignments)))
    ;; Find block comments: search for /* then find matching */
    (progn (goto-char (point-min))
           (let ((blocks nil))
             (while (search-forward "/*" nil t)
               (let ((start (- (point) 2)))
                 (when (search-forward "*/" nil t)
                   (push (buffer-substring start (point)) blocks))))
             (nreverse blocks)))
    ;; Use skip-chars to extract identifiers after "int "
    (progn (goto-char (point-min))
           (let ((ids nil))
             (while (search-forward "int " nil t)
               (let ((start (point)))
                 (skip-chars-forward "a-z_")
                 (push (buffer-substring start (point)) ids)))
             (nreverse ids)))
    ;; Backward: find last return statement
    (progn (goto-char (point-max))
           (when (re-search-backward "return \\(.+\\);" nil t)
             (match-string 1)))
    ;; Combined: find lines that have both assignment and comment
    (progn (goto-char (point-min))
           (let ((results nil))
             (while (not (eobp))
               (let ((line-start (point))
                     (line-end (progn (end-of-line) (point))))
                 (save-excursion
                   (goto-char line-start)
                   (when (and (re-search-forward "=" line-end t)
                              (progn (goto-char line-start)
                                     (search-forward "//" line-end t)))
                     (push (buffer-substring line-start line-end) results))))
               (forward-line 1))
             (nreverse results)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"line comment\" \"sum\" \"result\") ((\"x\" . \"10\") (\"y\" . \"20\") (\"z\" . \"x + y\")) (\"/* Block comment */\" \"/* Another\\n   block comment */\") (\"x\" \"y\" \"z\") \"z\" (\"int x = 10;   // line comment\" \"int z = x + y; // sum\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
