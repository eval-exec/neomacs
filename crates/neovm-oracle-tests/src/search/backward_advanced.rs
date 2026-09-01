//! Advanced oracle parity tests for `search-backward`:
//! BOUND, NOERROR, COUNT parameters, literal vs regex comparison,
//! multiple consecutive backward searches, and reverse key=value parsing.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic backward literal search
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_backward_basic_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "the quick brown fox jumps over the lazy dog")
  (goto-char (point-max))
  ;; search-backward finds last occurrence of "the"
  (let ((pos (search-backward "the" nil t)))
    (list pos (point)
          (buffer-substring pos (+ pos 3))
          ;; Search again to find earlier "the"
          (let ((pos2 (search-backward "the" nil t)))
            (list pos2 (buffer-substring pos2 (+ pos2 3)))))))"#;
    let expect = expect_test::expect![[r#""OK (32 32 \"the\" (1 \"the\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// search-backward with BOUND parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_backward_with_bound() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "AAA marker BBB marker CCC marker DDD")
  (goto-char (point-max))
  ;; BOUND restricts how far back we search
  (let* ((pos-max (point-max))
         ;; Find last "marker" first to get a reference position
         (last-marker (search-backward "marker" nil t))
         (_ (goto-char (point-max)))
         ;; Now bound at position 20 -- should skip the first "marker" at ~5
         (bounded (search-backward "marker" 20 t))
         (_ (goto-char (point-max)))
         ;; Bound at position 30 -- should only find the last marker
         (tight-bound (search-backward "marker" 30 t))
         (_ (goto-char (point-max)))
         ;; Bound past everything -- should return nil
         (too-tight (search-backward "marker" (1- pos-max) t)))
    (list last-marker bounded tight-bound too-tight)))"#;
    let expect = expect_test::expect![[r#""OK (27 27 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// search-backward with NOERROR parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_backward_noerror_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "some text without the target pattern")
  (goto-char (point-max))
  (let ((pos-before (point)))
    ;; NOERROR=t: return nil, point unchanged
    (let ((r1 (search-backward "NONEXISTENT" nil t)))
      (let ((pos-after-t (point)))
        ;; NOERROR=nil would signal error, so we test with condition-case
        (goto-char (point-max))
        (let ((r2 (condition-case err
                      (search-backward "NONEXISTENT" nil nil)
                    (search-failed 'caught-error))))
          ;; Test that successful search does move point
          (goto-char (point-max))
          (let ((r3 (search-backward "text" nil t)))
            (list r1
                  (= pos-before pos-after-t)  ;; point unchanged on failure with t
                  r2                           ;; error caught
                  r3                           ;; successful search position
                  (point))))))))"#;
    let expect = expect_test::expect![[r#""OK (nil t caught-error 6 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// search-backward with COUNT parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_backward_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "xa xb xc xd xe xf xg xh")
  (goto-char (point-max))
  ;; COUNT=1 finds last "x"
  (let ((c1 (progn (goto-char (point-max))
                    (search-backward "x" nil t 1)
                    (list (point) (buffer-substring (point) (+ (point) 2)))))
        ;; COUNT=4 finds the 4th "x" from end
        (c4 (progn (goto-char (point-max))
                    (search-backward "x" nil t 4)
                    (list (point) (buffer-substring (point) (+ (point) 2)))))
        ;; COUNT larger than available: returns nil with NOERROR=t
        (c-big (progn (goto-char (point-max))
                       (search-backward "x" nil t 100))))
    (list c1 c4 c-big)))"#;
    let expect = expect_test::expect![[r#""OK ((22 \"xh\") (13 \"xe\") nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// search-backward vs re-search-backward (literal vs regex)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_backward_vs_re_search_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  ;; Insert text with regex-special characters
  (insert "price is $5.00 or $10.00 (see note [1])")
  (goto-char (point-max))
  ;; search-backward treats "$5.00" literally
  (let ((lit-pos (search-backward "$5.00" nil t)))
    (goto-char (point-max))
    ;; re-search-backward with the same string needs escaping for regex
    ;; Without escaping, "$5.00" means end-of-line + "5" + any-char + "00"
    (let ((regex-pos (re-search-backward "\\$5\\.00" nil t)))
      (goto-char (point-max))
      ;; Literal search for "[1]" works directly
      (let ((lit-bracket (search-backward "[1]" nil t)))
        (goto-char (point-max))
        ;; Regex search for "[1]" means character class containing "1"
        ;; which will match the last "1" in the buffer differently
        (let ((regex-bracket (re-search-backward "\\[1\\]" nil t)))
          (list
           (list 'literal-dollar lit-pos)
           (list 'regex-dollar regex-pos)
           (= lit-pos regex-pos)  ;; should be same position for properly escaped regex
           (list 'literal-bracket lit-bracket)
           (list 'regex-bracket regex-bracket)
           (= lit-bracket regex-bracket)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((literal-dollar 10) (regex-dollar 10) t (literal-bracket 36) (regex-bracket 36) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple consecutive backward searches
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_backward_multiple_consecutive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert "Section: INTRO\nLine: first paragraph\nLine: second paragraph\n")
  (insert "Section: BODY\nLine: main content\nLine: more content\nLine: conclusion\n")
  (insert "Section: END\nLine: final note\n")
  (goto-char (point-max))
  ;; Walk backward collecting sections and their line counts
  (let ((sections nil)
        (current-section nil)
        (current-lines 0))
    ;; First pass: find each Section header backward and count Lines between
    (while (search-backward "Section: " nil t)
      (let ((section-start (point)))
        ;; Extract section name
        (let ((name (buffer-substring (+ section-start 9)
                                       (progn (end-of-line) (point)))))
          ;; Count "Line:" occurrences from here to end-of-section
          (let ((lines 0)
                (limit (if sections
                           (car (cdr (car sections)))  ;; previous section start
                         (point-max))))
            (save-excursion
              (goto-char section-start)
              (while (search-forward "Line: " limit t)
                (setq lines (1+ lines))))
            (setq sections (cons (list name section-start lines) sections)))
          (goto-char section-start))))
    sections))"#;
    let expect =
        expect_test::expect![[r#""OK ((\"INTRO\" 1 2) (\"BODY\" 61 3) (\"END\" 130 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: reverse-parse key=value pairs from end of config buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_search_backward_reverse_parse_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-parse-config-backward
    (lambda (text)
      "Parse a config file backward, collecting key=value pairs grouped by [section] headers.
       Returns an alist of (section . ((key . value) ...)) with sections in file order."
      (with-temp-buffer
        (insert text)
        (goto-char (point-max))
        (let ((sections nil)
              (current-pairs nil)
              (current-section nil))
          ;; Scan backward line by line
          (while (not (bobp))
            (let ((line-end (point))
                  (line-start (progn (beginning-of-line) (point))))
              (let ((line (buffer-substring line-start line-end)))
                (cond
                 ;; Section header [name]
                 ((and (> (length line) 2)
                       (= (aref line 0) ?\[)
                       (= (aref line (1- (length line))) ?\]))
                  (let ((name (substring line 1 (1- (length line)))))
                    (setq sections (cons (cons name current-pairs) sections)
                          current-pairs nil
                          current-section name)))
                 ;; Key=value line (skip comments and empty lines)
                 ((let ((eq-pos nil) (i 0))
                    (while (and (< i (length line)) (null eq-pos))
                      (when (= (aref line i) ?=)
                        (setq eq-pos i))
                      (setq i (1+ i)))
                    (when (and eq-pos (> eq-pos 0)
                               (not (= (aref line 0) ?#))
                               (not (= (aref line 0) ?\;)))
                      (let ((key (string-trim (substring line 0 eq-pos)))
                            (val (string-trim (substring line (1+ eq-pos)))))
                        (setq current-pairs (cons (cons key val) current-pairs)))
                      t)))))
              (when (> line-start 1)
                (goto-char (1- line-start)))))
          ;; If there were pairs before first section header, add under "global"
          (when current-pairs
            (setq sections (cons (cons "global" current-pairs) sections)))
          sections))))

  (unwind-protect
      (let ((config "[database]
host=localhost
port=5432
name=myapp_db
user=admin
password=secret123

[server]
host=0.0.0.0
port=8080
workers=4
debug=false

[logging]
level=info
file=/var/log/app.log
max_size=10M
rotate=true"))
        (let ((parsed (funcall 'neovm--test-parse-config-backward config)))
          (list
           ;; Number of sections
           (length parsed)
           ;; Section names in order
           (mapcar #'car parsed)
           ;; Key count per section
           (mapcar (lambda (s) (cons (car s) (length (cdr s)))) parsed)
           ;; Specific value lookups
           (cdr (assoc "port" (cdr (assoc "database" parsed))))
           (cdr (assoc "workers" (cdr (assoc "server" parsed))))
           (cdr (assoc "level" (cdr (assoc "logging" parsed)))))))
    (fmakunbound 'neovm--test-parse-config-backward)))"#;
    let expect = expect_test::expect![[
        r#""OK (3 (\"database\" \"server\" \"logging\") ((\"database\" . 5) (\"server\" . 4) (\"logging\" . 4)) \"5432\" \"4\" \"info\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
