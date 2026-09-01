//! Combo-strict-8 oracle tests — strict contract verification for
//! untested and thinly-tested org APIs: org-beginning-of-subtree,
//! org-complex-heading-regexp, org-reduced-level, org-2ft,
//! narrowing + at-point, property special chars, table formula
//! errors, babel :results types, multi-temp-buffer isolation,
//! and timestamp format round-trips.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// org-beginning-of-subtree + org-end-of-subtree consistency
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_beginning_end_subtree_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-beginning-of-subtree)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* A\nBody A.\n** A1\nBody A1.\n** A2\nBody A2.\n* B\nBody B.\n")
      (let ((r '()))
        ;; on A1: beginning → end should be same subtree
        (goto-char (point-min))
        (search-forward "** A1") (beginning-of-line)
        (let ((beg (progn (org-beginning-of-subtree) (point)))
              (end (progn (org-end-of-subtree) (point))))
          (push (list :a1-end (point)) r)
          ;; beginning again: should go back to same place
          (org-beginning-of-subtree)
          (push (list :a1-beg-again (point)) r))
        ;; on A: end should encompass A1 + A2
        (goto-char (point-min))
        (org-end-of-subtree)
        (push (list :a-end (point)) r)
        ;; beginning again
        (org-beginning-of-subtree)
        (push (list :a-beg (point)) r)
        ;; on B: end should be at EOF or next heading
        (search-forward "* B") (beginning-of-line)
        (org-end-of-subtree)
        (push (list :b-end (point)) r)
        (org-beginning-of-subtree)
        (push (list :b-beg (point)) r)
        ;; repeated begin/end should be stable
        (org-end-of-subtree) (org-beginning-of-subtree)
        (push (list :b-beg-stable (equal (point) (plist-get (nth 1 r) :b-beg))) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-complex-heading-regexp matching on various heading types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_complex_heading_regexp_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 19 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO [#A] Clean room :chore:\n")
      (insert "** DONE Review :work:\n")
      (insert "*** WAIT [#B] Think :urgent:personal:\n")
      (insert "* Simple heading\n")
      (insert "** [ ] Checkbox item\n")
      (goto-char (point-min))
      (let ((r '()))
        (while (re-search-forward org-complex-heading-regexp nil t)
          (push (list :todo (match-string 2)
                      :priority (match-string 3)
                      :title (match-string 4)
                      :tags (match-string 5))
                r))
        (push (list :count (length r)) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-reduced-level with org-odd-levels-only
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_reduced_level_odd_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 15 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-odd-levels-only t))
    (with-temp-buffer (org-mode)
      (insert "* L1\n** L2\n*** L3\n**** L4\n***** L5\n****** L6\n")
      (goto-char (point-min))
      (let ((r '()))
        (while (re-search-forward org-heading-regexp nil t)
          (push (list :raw (org-element-property :raw-value (org-element-at-point))
                      :level (org-element-property :level (org-element-at-point))
                      :reduced (org-reduced-level (org-element-property :level (org-element-at-point))))
                r))
        (push (list :count (length r)) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-2ft date formatting
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_org_2ft_formatting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 19 22)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((test-times '("<2024-01-15 Mon>"
                      "<2024-06-01 Sat 10:30>"
                      "[2024-12-25 Wed]"
                      "<2024-02-29 Thu +1w>"
                      "<2024-01-15 Mon>--<2024-01-20 Sat>")))
    (let ((r '()))
      (dolist (ts-string test-times)
        (condition-case err
            (let* ((ts (org-timestamp-from-string ts-string))
                   (encoded (org-2ft ts))
                   (decoded (org-timestamp-format ts "%Y-%m-%d")))
              (push (list :input ts-string
                          :encoded encoded
                          :decoded decoded)
                    r))
          (error (push (list :input ts-string :error (error-message-string err)) r))))
      (nreverse r)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point under narrowing and widening
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_at_point_under_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 39 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* A\nBody A.\n** A1\nBody A1.\n* B\nBody B.\n* C\nBody C.\n")
      (let ((r '()))
        ;; full buffer: at-point on A1
        (goto-char (point-min))
        (search-forward "** A1") (beginning-of-line)
        (push (list :full-a1-at (org-element-type (org-element-at-point))) r)
        (push (list :full-a1-level (org-element-property :level (org-element-at-point))) r)
        ;; narrow to A's subtree
        (goto-char (point-min))
        (org-narrow-to-subtree)
        ;; now A1 is still headline level 2
        (goto-char (point-min))
        (search-forward "** A1") (beginning-of-line)
        (push (list :narrow-a1-at (org-element-type (org-element-at-point))) r)
        (push (list :narrow-parse-headlines
                    (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
        ;; narrow to just A1
        (goto-char (point-min))
        (search-forward "** A1") (beginning-of-line)
        (let ((start (point)))
          (org-end-of-subtree)
          (narrow-to-region start (point)))
        (goto-char (point-min))
        (push (list :super-narrow-at (org-element-type (org-element-at-point))) r)
        (push (list :super-narrow-parse-headlines
                    (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
        ;; widen
        (widen)
        (goto-char (point-min))
        (push (list :after-widen-headlines
                    (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
        ;; at-point on B after widening
        (search-forward "* B") (beginning-of-line)
        (push (list :widen-b-at (org-element-type (org-element-at-point))) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Property API: org-entry-put with special characters
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_property_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 35 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Task\n")
      (goto-char (point-min))
      (let ((r '()))
        ;; value with colon
        (org-entry-put nil "URL" "https://example.com:8080/path")
        (push (list :colon-val (org-entry-get nil "URL")) r)
        ;; value with spaces and quotes
        (org-entry-put nil "DESC" "Some \"quoted\" text here")
        (push (list :quoted-val (org-entry-get nil "DESC")) r)
        ;; value with newlines (should work)
        (org-entry-put nil "NOTES" "Line one\nLine two\nLine three")
        (push (list :multiline-val (org-entry-get nil "NOTES")) r)
        ;; key with special chars
        (org-entry-put nil "MY_KEY" "value")
        (push (list :underscore-key (org-entry-get nil "MY_KEY")) r)
        ;; numeric value
        (org-entry-put nil "COUNT" "42")
        (push (list :numeric-val (org-entry-get nil "COUNT")) r)
        ;; empty value
        (org-entry-put nil "EMPTY" "")
        (push (list :empty-val (org-entry-get nil "EMPTY")) r)
        ;; value that looks like a property drawer
        (org-entry-put nil "META" ":PROPERTIES:")
        (push (list :meta-val (org-entry-get nil "META")) r)
        ;; delete and re-read
        (org-entry-delete nil "URL")
        (push (list :deleted-url (org-entry-get nil "URL")) r)
        ;; replace
        (org-entry-put nil "COUNT" "99")
        (push (list :replaced-count (org-entry-get nil "COUNT")) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Table formula: error conditions (non-numeric, div by zero)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_table_formula_error_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 30 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| a | b | c |\n|---+---+---|\n| 1 | 2 |   |\n| x | y |   |\n| 0 | 10|   |\n")
      (insert "#+TBLFM: $3=$1+$2\n")
      (goto-char (point-min))
      (let ((r '()))
        ;; recalc: row 1 (numeric) works, row 2 (strings) may produce error
        (push (list :before (buffer-substring-no-properties (point-min) (point-max))) r)
        (condition-case err
            (progn (org-table-recalculate t)
                   (org-table-align)
                   (push (list :after-recalc (buffer-substring-no-properties (point-min) (point-max))) r))
          (error (push (list :recalc-error (error-message-string err)) r)))
        ;; try division formula (row 3: 10/0 should error)
        (goto-char (point-max))
        ;; replace formula with div
        (search-backward "#+TBLFM:")
        (kill-line)
        (insert "#+TBLFM: $3=$2/$1\n")
        (condition-case err
            (progn (org-table-recalculate t)
                   (org-table-align)
                   (push (list :after-div (buffer-substring-no-properties (point-min) (point-max))) r))
          (error (push (list :div-error (error-message-string err)) r)))
        ;; to-lisp after all mutations
        (goto-char (point-min))
        (push (list :to-lisp (org-table-to-lisp)) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel with :results silent and :results replace
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_babel_results_silent_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 29 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (with-temp-buffer (org-mode)
      (insert "#+begin_src emacs-lisp :results silent\n(+ 1 2)\n#+end_src\n\n")
      (insert "#+begin_src emacs-lisp :results replace\n'(1 2 3 4 5)\n#+end_src\n")
      (let ((r '()))
        ;; execute silent block (should produce no output in buffer)
        (goto-char (point-min))
        (search-forward "#+begin_src emacs-lisp")
        (push (org-babel-execute-src-block) r)
        (push (list :after-silent-buffer (buffer-substring-no-properties (point-min) (point-max))) r)
        ;; execute replace block twice; second should replace first result
        (search-forward "#+begin_src emacs-lisp")
        (push (org-babel-execute-src-block) r)
        (push (list :after-first-replace (buffer-substring-no-properties (point-min) (point-max))) r)
        ;; modify the replace block content
        (goto-char (point-min))
        (search-forward "'(1 2 3 4 5)")
        (replace-match "'(10 20 30 40 50 60 70)")
        ;; re-execute
        (goto-char (point-min))
        (search-forward "#+begin_src emacs-lisp :results replace")
        (push (org-babel-execute-src-block) r)
        (push (list :after-second-replace (buffer-substring-no-properties (point-min) (point-max))) r)
        ;; count result blocks
        (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multiple temp-buffer isolation: create 3, parse all, verify no leakage
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_multibuffer_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 39 19)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((r '()))
    ;; buffer 1: headlines
    (with-temp-buffer
      (org-mode)
      (insert "* H1\n** H2\n* H3\n")
      (push (list :b1-headlines
                  (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                          (org-element-map (org-element-parse-buffer) 'headline #'identity)))
            r))
    ;; buffer 2: table
    (with-temp-buffer
      (org-mode)
      (insert "| a | b |\n| 1 | 2 |\n")
      (push (list :b2-cells
                  (mapcar (lambda (c) (substring-no-properties
                                       (org-element-interpret-data (org-element-contents c))))
                          (org-element-map (org-element-parse-buffer) 'table-cell #'identity)))
            r))
    ;; buffer 3: mixed
    (with-temp-buffer
      (org-mode)
      (insert "* X\n- item1\n- item2\n")
      (push (list :b3-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
      (push (list :b3-items (length (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
      ;; modify buffer 3: add content
      (goto-char (point-max))
      (insert "\n| x | y |\n| 3 | 4 |\n")
      (push (list :b3-after-mod-tables (length (org-element-map (org-element-parse-buffer) 'table #'identity))) r))
    ;; re-verify buffer 1 still unchanged by subsequent buffers
    (with-temp-buffer
      (org-mode)
      (insert "* H1\n** H2\n* H3\n")
      (push (list :b1-again-headlines
                  (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                          (org-element-map (org-element-parse-buffer) 'headline #'identity)))
            r))
    (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Timestamp format roundtrip: string → timestamp → format → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_timestamp_format_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; active timestamp
     (let ((ts (org-timestamp-from-string "<2024-06-15 Sat 14:30>")))
       (list :active-props
             (list (org-element-property :year-start ts)
                   (org-element-property :month-start ts)
                   (org-element-property :day-start ts)
                   (org-element-property :hour-start ts)
                   (org-element-property :minute-start ts))
             :active-format (org-timestamp-format ts "%Y-%m-%d %H:%M")
             :active-type (org-element-property :type ts)))
     ;; inactive timestamp
     (let ((ts (org-timestamp-from-string "[2024-12-25 Wed]")))
       (list :inactive-props
             (list (org-element-property :year-start ts)
                   (org-element-property :month-start ts)
                   (org-element-property :day-start ts))
             :inactive-format (org-timestamp-format ts "%B %d, %Y")
             :inactive-type (org-element-property :type ts)))
     ;; range timestamp
     (let ((ts (org-timestamp-from-string "<2024-01-01 Mon>--<2024-01-07 Sun>")))
       (list :range-props
             (list (org-element-property :year-start ts)
                   (org-element-property :month-start ts)
                   (org-element-property :day-start ts)
                   (org-element-property :year-end ts)
                   (org-element-property :month-end ts)
                   (org-element-property :day-end ts))
             :range-format (org-timestamp-format ts "%Y-%m-%d")
             :range-type (org-element-property :type ts)))
     ;; with repeater (+1w)
     (let ((ts (org-timestamp-from-string "<2024-03-01 Fri +1w>")))
       (list :repeater-type (org-element-property :repeater-type ts)
             :repeater-value (org-element-property :repeater-value ts)
             :repeater-unit (org-element-property :repeater-unit ts))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-link-unescape with various edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_link_unescape_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:spaces \"hello world\") (:brackets \"a[b]c\") (:slashes \"a/b/c\") (:colons \"key:value:extra\") (:percent \"50% off\") (:ampersand \"a&b&c\") (:tilde \"user~home\") (:at-sign \"user@host\") (:empty \"\") (:url \"https://example.com/path?q=hello world&x=1\") (:double-escaped \"test\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; Basic escape/unescape roundtrip
   (list :spaces (org-link-unescape (org-link-escape "hello world")))
   (list :brackets (org-link-unescape (org-link-escape "a[b]c")))
   (list :slashes (org-link-unescape (org-link-escape "a/b/c")))
   (list :colons (org-link-unescape (org-link-escape "key:value:extra")))
   (list :percent (org-link-unescape (org-link-escape "50% off")))
   (list :ampersand (org-link-unescape (org-link-escape "a&b&c")))
   (list :tilde (org-link-unescape (org-link-escape "user~home")))
   (list :at-sign (org-link-unescape (org-link-escape "user@host")))
   ;; Empty string
   (list :empty (org-link-unescape (org-link-escape "")))
   ;; URL-like
   (list :url (org-link-unescape (org-link-escape "https://example.com/path?q=hello world&x=1")))
   ;; Double escaping
   (list :double-escaped (org-link-unescape (org-link-escape (org-link-escape "test"))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-remove-indentation on various strings
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_remove_indentation_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"two spaces\\n three spaces\\ntwo spaces\" \"no indent\\n  two spaces\\n    four spaces\\n  two spaces\" \"single line\" \"line1\\n\\nline2\" \"top level\\n  nested\\n  nested2\\ntop again\" \"one tab\\n\ttwo tabs\" \"\" \"plain text\\nno indent at all\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; Simple indentation
   (org-remove-indentation "  two spaces\n   three spaces\n  two spaces")
   ;; Mixed indentation
   (org-remove-indentation "no indent\n  two spaces\n    four spaces\n  two spaces")
   ;; Single line
   (org-remove-indentation "  single line")
   ;; Blank line in middle
   (org-remove-indentation "  line1\n\n  line2")
   ;; Preserve relative indent within block
   (org-remove-indentation "  top level\n    nested\n    nested2\n  top again")
   ;; Tab indentation
   (org-remove-indentation "\tone tab\n\t\ttwo tabs")
   ;; Empty string
   (org-remove-indentation "")
   ;; No indentation
   (org-remove-indentation "plain text\nno indent at all")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-get-valid-level edge inputs
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_get_valid_level_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 18 31)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; Valid levels
   (list :min-level (org-get-valid-level 1 1))
   (list :max-level (org-get-valid-level 10 1))
   (list :mid-level (org-get-valid-level 5 1))
   ;; Clamp to max
   (cond ((fboundp 'org-get-valid-level)
          (list :clamp-high (org-get-valid-level 100 1)))
         (t :not-available))
   ;; Change relative
   (list :change-plus (org-get-valid-level 3 1))
   (list :change-minus (org-get-valid-level 3 -1))
   ;; Zero (should clamp to min)
   (cond ((fboundp 'org-get-valid-level)
          (list :zero (org-get-valid-level 0 1)))
         (t :not-available)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-fold-show-children visibility + element counts
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_fold_show_children_visibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 25 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* A\n** A1\nBody A1.\n** A2\nBody A2.\n* B\n** B1\nBody B1.\n** B2\nBody B2.\n")
      (let ((r '()))
        ;; fold all to level 1
        (goto-char (point-min))
        (org-overview)
        ;; show children of A (reveals A1 and A2 headlines but not bodies)
        (org-fold-show-children)
        (push (list :visibility-a (get-char-property (point) 'invisible)) r)
        ;; parse visible only after showing children
        (push (list :vis-headlines (length (org-element-map (org-element-parse-buffer nil t) 'headline #'identity))) r)
        ;; show all
        (org-show-all)
        (push (list :after-showall-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
        ;; now fold A subtree and show children of B
        (goto-char (point-min))
        (org-fold-hide-subtree)
        (goto-char (point-min))
        (search-forward "* B") (beginning-of-line)
        (org-fold-show-children)
        (push (list :b-vis-headlines (length (org-element-map (org-element-parse-buffer nil t) 'headline #'identity))) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-parse-buffer with various granularity levels
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_parse_granularity_all_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 29 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n*BOLD* and /ITALIC/ text.\n** H2\n| a | b |\n| 1 | 2 |\n")
      (let ((r '()))
        ;; granularity: headline (default)
        (let ((tree-hl (org-element-parse-buffer 'headline)))
          (push (list :hl-types (delete-dups (mapcar #'org-element-type
                                                     (org-element-map tree-hl t #'identity)))) r)
          (push (list :hl-bolds (length (org-element-map tree-hl 'bold #'identity))) r))
        ;; granularity: greater-element
        (let ((tree-ge (org-element-parse-buffer 'greater-element)))
          (push (list :ge-types (delete-dups (mapcar #'org-element-type
                                                     (org-element-map tree-ge t #'identity)))) r)
          (push (list :ge-sections (length (org-element-map tree-ge 'section #'identity))) r))
        ;; granularity: element
        (let ((tree-el (org-element-parse-buffer 'element)))
          (push (list :el-types (delete-dups (mapcar #'org-element-type
                                                     (org-element-map tree-el t #'identity)))) r)
          (push (list :el-paragraphs (length (org-element-map tree-el 'paragraph #'identity))) r))
        ;; granularity: object (finest)
        (let ((tree-ob (org-element-parse-buffer 'object)))
          (push (list :ob-types (delete-dups (mapcar #'org-element-type
                                                     (org-element-map tree-ob t #'identity)))) r)
          (push (list :ob-bolds (length (org-element-map tree-ob 'bold #'identity))) r)
          (push (list :ob-italics (length (org-element-map tree-ob 'italic #'identity))) r))
        (nreverse r))))))"##,
        expect,
    );
}
