//! Org-mode divergence probes (calibration).
//!
//! Probes deterministic, structural org operations: org-element-parse-buffer,
//! org-element-at-point headline properties (raw-value/todo-keyword/level/tags/
//! priority), org-element-link, org-element-src-block, org-table-align,
//! org-mode fontification. (Both engines (require 'org) successfully.)

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

fn org(form: &str) {
    let _ = form;
}

#[test]
fn div_org_element_parse_root_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    org("");
    let expect = expect_test::expect![[r#""OK (org-data 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'org)
  (with-temp-buffer
    (insert "* Heading 1\nSome text.\n** Sub\n")
    (org-mode)
    (let ((tree (org-element-parse-buffer)))
      (list (org-element-type tree)
            (length (org-element-contents tree))))))
"##,
        expect,
    );
}

#[test]
fn div_org_headline_raw_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    org("");
    let expect = expect_test::expect![[r#""OK \"Buy milk\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'org)
  (with-temp-buffer
    (insert "* TODO Buy milk")
    (org-mode)
    (let ((hl (org-element-at-point)))
      (org-element-property :raw-value hl))))
"##,
        expect,
    );
}

#[test]
fn div_org_headline_todo_keyword_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    org("");
    let expect = expect_test::expect![[r#""OK (\"DONE\" done 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'org)
  (with-temp-buffer
    (insert "** DONE Task")
    (org-mode)
    (let ((hl (org-element-at-point)))
      (list (org-element-property :todo-keyword hl)
            (org-element-property :todo-type hl)
            (org-element-property :level hl)))))
"##,
        expect,
    );
}

#[test]
fn div_org_headline_tags_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    org("");
    let expect = expect_test::expect![[r#""OK (65 (\"tag1\" \"tag2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'org)
  (with-temp-buffer
    (insert "* TODO [#A] Project :tag1:tag2:")
    (org-mode)
    (let ((hl (org-element-at-point)))
      (list (org-element-property :priority hl)
            (org-element-property :tags hl)))))
"##,
        expect,
    );
}

#[test]
fn div_org_link_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    org("");
    let expect =
        expect_test::expect![[r#""OK (\"https\" \"//example.com\" \"https://example.com\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'org)
  (with-temp-buffer
    (insert "[[https://example.com][Example]]")
    (org-mode)
    (let* ((tree (org-element-parse-buffer))
           (link (org-element-map tree 'link #'identity nil t)))
      (list (org-element-property :type link)
            (org-element-property :path link)
            (org-element-property :raw-link link)))))
"##,
        expect,
    );
}

#[test]
fn div_org_src_block_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    org("");
    let expect = expect_test::expect![[r#""OK (\"emacs-lisp\" \"(+ 1 2)\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'org)
  (with-temp-buffer
    (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n")
    (org-mode)
    (let ((tree (org-element-parse-buffer)))
      (org-element-map tree 'src-block
        (lambda (b) (list (org-element-property :language b)
                          (org-element-property :value b))) nil t))))
"##,
        expect,
    );
}

#[test]
fn div_org_table_align() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    org("");
    let expect = expect_test::expect![[r#""OK \"| a | b |\\n| 1 | 2 |\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'org)
  (with-temp-buffer
    (insert "| a | b |\n| 1 | 2 |\n")
    (org-mode)
    (org-table-align)
    (buffer-substring-no-properties (point-min) (point-max))))
"##,
        expect,
    );
}

#[test]
fn div_org_fontify_bold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    org("");
    let expect = expect_test::expect![[r#""OK (bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'org)
  (with-temp-buffer
    (insert "*bold*")
    (org-mode)
    (font-lock-fontify-buffer)
    (get-text-property 1 'face)))
"##,
        expect,
    );
}

#[test]
fn div_org_property_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    org("");
    let expect = expect_test::expect![[r#""OK ((\"KEY\" \"value\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'org)
  (with-temp-buffer
    (insert "* H\n:PROPERTIES:\n:KEY: value\n:END:\n")
    (org-mode)
    (org-element-map (org-element-parse-buffer) 'node-property
      (lambda (p) (list (org-element-property :key p)
                        (org-element-property :value p))) nil)))
"##,
        expect,
    );
}

#[test]
fn div_org_itemize_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    org("");
    let expect = expect_test::expect![[r#""OK (\"- \" \"- \" \"- \")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'org)
  (with-temp-buffer
    (insert "- a\n- b\n- c\n")
    (org-mode)
    (let ((tree (org-element-parse-buffer)))
      (org-element-map tree 'item
        (lambda (i) (org-element-property :bullet i)) nil))))
"##,
        expect,
    );
}

#[test]
fn div_org_map_entries_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    org("");
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'org)
  (with-temp-buffer
    (insert "* A\n* B\n** B1\n")
    (org-mode)
    (let (n) (org-map-entries (lambda () (setq n (1+ n))) nil 'buffer) n)))
"##,
        expect,
    );
}

#[test]
fn div_org_timestamp_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    org("");
    let expect = expect_test::expect![[r#""OK (2024 1 15)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'org)
  (with-temp-buffer
    (insert "[2024-01-15 Mon]")
    (org-mode)
    (let* ((tree (org-element-parse-buffer))
           (ts (org-element-map tree 'timestamp #'identity nil t)))
      (list (org-element-property :year-start ts)
            (org-element-property :month-start ts)
            (org-element-property :day-start ts)))))
"##,
        expect,
    );
}

#[test]
fn div_org_todo_keywords_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    org("");
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'org)
  org-todo-keywords-1)
"##,
        expect,
    );
}

#[test]
fn div_org_section_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    org("");
    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'org)
  (with-temp-buffer
    (insert "* H\nparagraph text\n")
    (org-mode)
    (let* ((tree (org-element-parse-buffer))
           (sec (org-element-map tree 'section #'identity nil t)))
      (org-element-property :begin sec))))
"##,
        expect,
    );
}
