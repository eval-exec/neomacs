//! Ported upstream ERT tests from org-mode's test-org-lint.el (9.7.11).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ── Lint: add-checker ────────────────────────────────────────────────

#[test]
fn upstream_org_lint_add_checker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-lint)
  (list
   ;; Valid checker.
   (let ((org-lint--checkers nil))
     (org-lint-add-checker 'check "check" #'ignore)
     (length org-lint--checkers))
   ;; Duplicate name: not added twice.
   (let ((org-lint--checkers nil))
     (org-lint-add-checker 'check "check" #'ignore)
     (org-lint-add-checker 'check "other check" #'ignore)
     (length org-lint--checkers))))"##,
        expect,
    );
}

// ── Lint: duplicate-custom-id ────────────────────────────────────────

#[test]
fn upstream_org_lint_duplicate_custom_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((1 [#(\"3\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Duplicate CUSTOM_ID property \\\"foo\\\"\" #s(org-lint-checker duplicate-custom-id \"Report duplicate CUSTOM_ID properties\" org-lint-duplicate-custom-id nil (link))]) (2 [#(\"8\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Duplicate CUSTOM_ID property \\\"foo\\\"\" #s(org-lint-checker duplicate-custom-id \"Report duplicate CUSTOM_ID properties\" org-lint-duplicate-custom-id nil (link))])) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-lint)
  (let ((org-mode-hook nil))
    (list
     ;; Duplicate: detected.
     (with-temp-buffer (org-mode)
       (insert "* H1\n:PROPERTIES:\n:CUSTOM_ID: foo\n:END:\n\n* H2\n:PROPERTIES:\n:CUSTOM_ID: foo\n:END:")
       (goto-char (point-min))
       (org-lint '(duplicate-custom-id)))
     ;; No duplicate.
     (with-temp-buffer (org-mode)
       (insert "* H1\n:PROPERTIES:\n:CUSTOM_ID: foo\n:END:\n\n* H2\n:PROPERTIES:\n:CUSTOM_ID: bar\n:END:")
       (goto-char (point-min))
       (org-lint '(duplicate-custom-id))))))"##,
        expect,
    );
}

// ── Lint: duplicate-name ─────────────────────────────────────────────

#[test]
fn upstream_org_lint_duplicate_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((1 [#(\"1\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Duplicate NAME \\\"foo\\\"\" #s(org-lint-checker duplicate-name \"Report duplicate NAME values\" org-lint-duplicate-name nil (babel 'link))]) (2 [#(\"4\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Duplicate NAME \\\"foo\\\"\" #s(org-lint-checker duplicate-name \"Report duplicate NAME values\" org-lint-duplicate-name nil (babel 'link))])) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-lint)
  (let ((org-mode-hook nil))
    (list
     ;; Duplicate: detected.
     (with-temp-buffer (org-mode)
       (insert "#+name: foo\nParagraph1\n\n#+name: foo\nParagraph 2")
       (goto-char (point-min))
       (org-lint '(duplicate-name)))
     ;; No duplicate.
     (with-temp-buffer (org-mode)
       (insert "#+name: foo\nParagraph1\n\n#+name: bar\nParagraph 2")
       (goto-char (point-min))
       (org-lint '(duplicate-name))))))"##,
        expect,
    );
}

// ── Lint: duplicate-target ───────────────────────────────────────────

#[test]
fn upstream_org_lint_duplicate_target() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((1 [#(\"1\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Duplicate target <<foo>>\" #s(org-lint-checker duplicate-target \"Report duplicate targets\" org-lint-duplicate-target nil (link))]) (2 [#(\"1\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Duplicate target <<foo>>\" #s(org-lint-checker duplicate-target \"Report duplicate targets\" org-lint-duplicate-target nil (link))])) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-lint)
  (let ((org-mode-hook nil))
    (list
     ;; Duplicate: detected.
     (with-temp-buffer (org-mode)
       (insert "<<foo>> <<foo>>")
       (goto-char (point-min))
       (org-lint '(duplicate-target)))
     ;; No duplicate.
     (with-temp-buffer (org-mode)
       (insert "<<foo>> <<bar>>")
       (goto-char (point-min))
       (org-lint '(duplicate-target))))))"##,
        expect,
    );
}

// ── Lint: duplicate-footnote-definition ──────────────────────────────

#[test]
fn upstream_org_lint_duplicate_footnote_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((1 [#(\"1\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Duplicate footnote definition \\\"1\\\"\" #s(org-lint-checker duplicate-footnote-definition \"Report duplicate footnote definitions\" org-lint-duplicate-footnote-definition nil (footnote))]) (2 [#(\"3\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Duplicate footnote definition \\\"1\\\"\" #s(org-lint-checker duplicate-footnote-definition \"Report duplicate footnote definitions\" org-lint-duplicate-footnote-definition nil (footnote))])) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-lint)
  (let ((org-mode-hook nil))
    (list
     ;; Duplicate: detected.
     (with-temp-buffer (org-mode)
       (insert "[fn:1] Definition 1\n\n[fn:1] Definition 2")
       (goto-char (point-min))
       (org-lint '(duplicate-footnote-definition)))
     ;; No duplicate.
     (with-temp-buffer (org-mode)
       (insert "[fn:1] Definition 1\n\n[fn:2] Definition 2")
       (goto-char (point-min))
       (org-lint '(duplicate-footnote-definition))))))"##,
        expect,
    );
}

// ── Lint: orphaned-affiliated-keywords ───────────────────────────────

#[test]
fn upstream_org_lint_orphaned_affiliated_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 [#(\"1\" 0 1 (org-lint-marker #<marker in no buffer>)) \"low\" \"Orphaned affiliated keyword: \\\"NAME\\\"\" #s(org-lint-checker orphaned-affiliated-keywords \"Report orphaned affiliated keywords\" org-lint-orphaned-affiliated-keywords low nil)]))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-lint)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+name: foo")
      (goto-char (point-min))
      (org-lint '(orphaned-affiliated-keywords)))))"##,
        expect,
    );
}

// ── Lint: deprecated-export-blocks ───────────────────────────────────

#[test]
fn upstream_org_lint_deprecated_export_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 [#(\"1\" 0 1 (org-lint-marker #<marker in no buffer>)) \"low\" \"Deprecated syntax for export block.  Use \\\"BEGIN_EXPORT latex\\\" instead\" #s(org-lint-checker deprecated-export-blocks \"Report deprecated export block syntax\" org-lint-deprecated-export-blocks low (obsolete export))]))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-lint)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+begin_latex\n...\n#+end_latex")
      (goto-char (point-min))
      (org-lint '(deprecated-export-blocks)))))"##,
        expect,
    );
}

// ── Lint: deprecated-header-syntax ───────────────────────────────────

#[test]
fn upstream_org_lint_deprecated_header_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((1 [#(\"1\" 0 1 (org-lint-marker #<marker in no buffer>)) \"low\" \"Deprecated syntax for \\\"cache\\\".  Use header-args instead\" #s(org-lint-checker deprecated-header-syntax \"Report deprecated Babel header syntax\" org-lint-deprecated-header-syntax low (obsolete babel))])) ((1 [#(\"3\" 0 1 (org-lint-marker #<marker in no buffer>)) \"low\" \"Deprecated syntax for \\\"cache\\\".  Use :header-args: instead\" #s(org-lint-checker deprecated-header-syntax \"Report deprecated Babel header syntax\" org-lint-deprecated-header-syntax low (obsolete babel))])))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-lint)
  (let ((org-mode-hook nil))
    (list
     ;; Keyword.
     (with-temp-buffer (org-mode)
       (insert "#+property: cache yes")
       (goto-char (point-min))
       (org-lint '(deprecated-header-syntax)))
     ;; Property drawer.
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:cache: yes\n:END:")
       (goto-char (point-min))
       (org-lint '(deprecated-header-syntax))))))"##,
        expect,
    );
}

// ── Lint: missing-language-in-src-block ──────────────────────────────

#[test]
fn upstream_org_lint_missing_language_in_src_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 [#(\"1\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Missing language in source block\" #s(org-lint-checker missing-language-in-src-block \"Report missing language in source blocks\" org-lint-missing-language-in-src-block nil (babel))]))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-lint)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+begin_src\n...\n#+end_src")
      (goto-char (point-min))
      (org-lint '(missing-language-in-src-block)))))"##,
        expect,
    );
}

// ── Lint: missing-backend-in-export-block ────────────────────────────

#[test]
fn upstream_org_lint_missing_backend_in_export_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 [#(\"1\" 0 1 (org-lint-marker #<marker in no buffer>)) \"nil\" \"Missing backend in export block\" #s(org-lint-checker missing-backend-in-export-block \"Report missing backend in export blocks\" org-lint-missing-backend-in-export-block nil (export))]))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-lint)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+begin_export\n...\n#+end_export")
      (goto-char (point-min))
      (org-lint '(missing-backend-in-export-block)))))"##,
        expect,
    );
}

// ── Lint: special block with no name ─────────────────────────────────

#[test]
fn upstream_org_lint_special_block_no_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-lint)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_SPECIAL*\nContents\n#+END_SPECIAL*")
      (goto-char (point-min))
      (org-lint '(special-block-with-parameters)))))"##,
        expect,
    );
}

// ── Lint: obsolete-syntax ────────────────────────────────────────────

#[test]
fn upstream_org_lint_obsolete_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((1 [#(\"1\" 0 1 (org-lint-marker #<marker in no buffer>)) \"low\" \"Deprecated syntax for export block.  Use \\\"BEGIN_EXPORT HTML\\\" instead\" #s(org-lint-checker deprecated-export-blocks \"Report deprecated export block syntax\" org-lint-deprecated-export-blocks low (obsolete export))])) ((1 [#(\"1\" 0 1 (org-lint-marker #<marker in no buffer>)) \"low\" \"Deprecated syntax for export block.  Use \\\"BEGIN_EXPORT LaTeX\\\" instead\" #s(org-lint-checker deprecated-export-blocks \"Report deprecated export block syntax\" org-lint-deprecated-export-blocks low (obsolete export))])))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-lint)
  (let ((org-mode-hook nil))
    (list
     ;; Old export blocks.
     (with-temp-buffer (org-mode)
       (insert "#+BEGIN_HTML\n<p>Text</p>\n#+END_HTML")
       (goto-char (point-min))
       (org-lint '(deprecated-export-blocks)))
     ;; Old LaTeX block.
     (with-temp-buffer (org-mode)
       (insert "#+BEGIN_LaTeX\n\\textbf{Text}\n#+END_LaTeX")
       (goto-char (point-min))
        (org-lint '(deprecated-export-blocks))))))"##,
        expect,
    );
}

#[test]
fn org_lint_multi_checker_report_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil ((25 \"Missing language in source block\")) nil ((77 \"Deprecated syntax for export block.  Use \\\"BEGIN_EXPORT HTML\\\" instead\")) 0 1 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-lint)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+NAME: dup\n#+NAME: dup\n")
      (insert "#+begin_src\nmissing language\n#+end_src\n")
      (insert "[fn:missing]\n")
      (insert "#+BEGIN_HTML\n<p>raw</p>\n#+END_HTML\n")
      (insert "* TODO Task\nSCHEDULED: <2026-05-27 Wed>\nDEADLINE: <2026-05-26 Tue>\n")
      (let* ((ast (org-element-parse-buffer))
             (dup-name (org-lint-duplicate-name ast))
             (no-lang (org-lint-missing-language-in-src-block ast))
             (undef-fn (org-lint-undefined-footnote-reference ast))
             (deprecated (org-lint-deprecated-export-blocks ast))
             (sched-after-deadline
              (condition-case nil
                  (org-lint-scheduled-after-deadline ast)
                (error nil))))
        (list (mapcar (lambda (r) (list (car r) (nth 1 r))) dup-name)
              (mapcar (lambda (r) (list (car r) (nth 1 r))) no-lang)
              (mapcar (lambda (r) (list (car r) (nth 1 r))) undef-fn)
              (mapcar (lambda (r) (list (car r) (nth 1 r))) deprecated)
              (length dup-name)
              (length no-lang)
              (length undef-fn))))))"##,
        expect,
    );
}
