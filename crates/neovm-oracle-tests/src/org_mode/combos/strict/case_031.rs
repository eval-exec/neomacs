use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn strict_org_special_block_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer (org-mode)
    (insert "#+BEGIN_ABSTRACT\nAbstract content.\n#+END_ABSTRACT\n")
    (let* ((t (org-element-parse-buffer))
           (sb (car (org-element-map t 'special-block #'identity))))
      (list :type (when sb (org-element-property :type sb))
            :exists (and sb t)))))"##,
        expect,
    );
}
#[test]
fn strict_org_description_list_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer (org-mode)
    (insert "- Term1 :: Description one.\n- Term2 :: Description two.\n")
    (let* ((t (org-element-parse-buffer))
           (items (org-element-map t 'item #'identity)))
      (list :count (length items)
            :tags (mapcar (lambda (it) (org-element-property :tag it)) items)))))"##,
        expect,
    );
}
#[test]
fn strict_org_empty_heading_with_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer (org-mode)
    (insert "*  :tag1:tag2:\n")
    (let* ((t (org-element-parse-buffer))
           (h (car (org-element-map t 'headline #'identity))))
      (list :raw (substring-no-properties (or (org-element-property :raw-value h) ""))
            :tags (org-element-property :tags h)
            :level (org-element-property :level h)))))"##,
        expect,
    );
}
#[test]
fn strict_org_heading_with_just_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer (org-mode)
    (insert "* [#A]\n")
    (let* ((t (org-element-parse-buffer))
           (h (car (org-element-map t 'headline #'identity))))
      (list :priority (org-element-property :priority h)
            :raw (substring-no-properties (or (org-element-property :raw-value h) ""))
            :level (org-element-property :level h)))))"##,
        expect,
    );
}
#[test]
fn strict_org_table_zero_cells() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:to-lisp ((#(\"0\" 0 1 (face org-table))) (#(\"0\" 0 1 (face org-table :org-untouchable t)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer (org-mode)
    (insert "| 0 |\n| 0 |\n") (insert "#+TBLFM: @2=vsum(@1..@-1)\n")
    (goto-char (point-min)) (condition-case nil (org-table-recalculate t) (error :err))
    (list :to-lisp (org-table-to-lisp))))"##,
        expect,
    );
}
#[test]
fn strict_org_babel_empty_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-emacs-lisp)
  (with-temp-buffer (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      (insert "#+begin_src emacs-lisp :results value\n\n#+end_src\n")
      (goto-char (point-min)) (search-forward "#+begin_src")
      (condition-case nil (org-babel-execute-src-block) (error :empty-body)))))"##,
        expect,
    );
}
#[test]
fn strict_org_link_abbreviation_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:expand \"https://github.com/foo/bar\" :no-expand \"no:such\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ol)
  (let ((org-link-abbrev-alist '(("gh" . "https://github.com/%s"))))
    (list :expand (condition-case nil (org-link-expand-abbrev "gh:foo/bar") (error :no-expand))
          :no-expand (condition-case nil (org-link-expand-abbrev "no:such") (error :no-expand)))))"##,
        expect,
    );
}
#[test]
fn strict_org_timestamp_iso_8601() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (let ((ts (org-timestamp-from-string "<2024-06-15 Sat 14:30:00>")))
    (list :type (org-element-property :type ts)
          :hour (org-element-property :hour-start ts)
          :minute (org-element-property :minute-start ts)
          :second (numberp (or (org-element-property :second-start ts) 0)))))"##,
        expect,
    );
}
#[test]
fn strict_org_paragraph_with_trailing_spaces() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer (org-mode)
    (insert "Trailing spaces   \n")
    (let* ((t (org-element-parse-buffer))
           (p (car (org-element-map t 'paragraph #'identity))))
      (list :post-blank (when p (org-element-property :post-blank p))
            :contents (when p (buffer-substring-no-properties
                               (org-element-property :contents-begin p)
                               (org-element-property :contents-end p)))))))"##,
        expect,
    );
}
#[test]
fn strict_org_tag_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:tag-groups-fbound t :tag-sort-fbound nil :tag-column-bound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (list :tag-groups-fbound (fboundp 'org-tag-alist-to-groups)
        :tag-sort-fbound (fboundp 'org-tags-sort-function)
        :tag-column-bound (boundp 'org-tags-column)))"##,
        expect,
    );
}
