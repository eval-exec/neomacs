//! Gamma-strict combo tests for org-mode edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Gamma: org-element with all context edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn gamma_context_in_nested_markup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (underline underline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Some *text with _under<point>line_ text*")
      (goto-char (point-min))
      (search-forward "under")
      (list
       ;; In underline inside bold.
       (org-element-type (org-element-context))
       ;; Optional argument.
       (org-element-type (org-element-context (org-element-at-point)))))))"##,
        expect,
    );
}

#[test]
fn gamma_context_in_secondary_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK underline""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Headline _<point>with_ underlining")
      (goto-char (point-min))
      (search-forward "with")
      (org-element-type (org-element-context)))))"##,
        expect,
    );
}

#[test]
fn gamma_context_in_objects_in_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (table-cell table-cell)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Macro inside table cell.
     (with-temp-buffer (org-mode) (insert "| a | {<point>{{macro}}} |")
       (goto-char (point-min)) (search-forward "macro")
       (org-element-type (org-element-context)))
     ;; Table cell with macro.
     (with-temp-buffer (org-mode) (insert "| a | b<point> {{{macro}}} |")
       (goto-char (point-min)) (search-forward "b")
       (org-element-type (org-element-context))))))"##,
        expect,
    );
}

#[test]
fn gamma_context_in_item_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold paragraph)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Bold in item tag.
     (with-temp-buffer (org-mode) (insert "- *bo<point>ld* ::")
       (goto-char (point-min)) (search-forward "bo")
       (org-element-type (org-element-context)))
     ;; Not bold after tag.
     (with-temp-buffer (org-mode) (insert "- *bold* ::<point>")
       (goto-char (point-min)) (search-forward "::")
       (org-element-type (org-element-context))))))"##,
        expect,
    );
}

#[test]
fn gamma_context_in_table_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK table-row""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| a | b |\n|---<point>--|---|\n| c | d |")
      (goto-char (point-min)) (search-forward "---")
      (org-element-type (org-element-context)))))"##,
        expect,
    );
}

#[test]
fn gamma_context_in_affiliated_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (table bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Macro in affiliated keyword.
     (with-temp-buffer (org-mode) (insert "#+CAPTION: {<point>{{macro}}}\n| a | b |")
       (goto-char (point-min)) (search-forward "macro")
       (org-element-type (org-element-context)))
     ;; Bold in affiliated keyword.
     (with-temp-buffer (org-mode) (insert "#+caption: *<point>bold*\nParagraph")
       (goto-char (point-min)) (search-forward "bold")
       (org-element-type (org-element-context))))))"##,
        expect,
    );
}

#[test]
fn gamma_context_at_end_of_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "*bold*")
      (goto-char (point-max))
      (org-element-type (org-element-context)))))"##,
        expect,
    );
}

#[test]
fn gamma_context_parent_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "Some *bold<point>* text")
      (goto-char (point-min)) (search-forward "bold")
      (org-element-type
       (org-element-property :parent (org-element-context))))))"##,
        expect,
    );
}

#[test]
fn gamma_context_between_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK macro""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "<<target>><point>{{{test}}}")
      (goto-char (point-min)) (search-forward "}}")
      (backward-char 3)
      (org-element-type (org-element-context)))))"##,
        expect,
    );
}

#[test]
fn gamma_context_bold_at_headline_beginning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* *bo<point>ld*")
      (goto-char (point-min)) (search-forward "bo")
      (org-element-type (org-element-context)))))"##,
        expect,
    );
}

#[test]
fn gamma_context_incomplete_cell() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK table-cell""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "|a|b|c<point>")
      (goto-char (point-min)) (search-forward "c")
      (org-element-type (org-element-context)))))"##,
        expect,
    );
}

#[test]
fn gamma_context_in_inline_footnotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK link""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "[fn::[[<point>https://orgmode.org]]]")
      (goto-char (point-min)) (search-forward "https")
      (org-element-type (org-element-context)))))"##,
        expect,
    );
}

#[test]
fn gamma_context_tags_looking_like_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (headline headline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Not a link in tag position.
     (with-temp-buffer (org-mode) (insert "* Headline :file<point>:tags:")
       (goto-char (point-min)) (search-forward "file")
       (org-element-type (org-element-context)))
     ;; Link when not all tags.
     (with-temp-buffer (org-mode) (insert "* Headline :file<point>:tags: :real:tag:")
       (goto-char (point-min)) (search-forward "file")
       (org-element-type (org-element-context))))))"##,
        expect,
    );
}

#[test]
fn gamma_context_no_partial_export_snippets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "@@latex:\n\nparagraph\n\n@@")
      (goto-char (point-min))
      (org-element-type (org-element-context)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Gamma: org-element with all at-point edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn gamma_at_point_in_center_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "#+BEGIN_CENTER\nA\n#+END_CENTER")
      (search-forward "A")
      (org-element-type (org-element-at-point)))))"##,
        expect,
    );
}

#[test]
fn gamma_at_point_in_other_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "#+BEGIN_CENTER\nA\n#+END_CENTER")
      (search-forward "A")
      (let ((mk (point-marker)))
        (with-temp-buffer
          (org-element-type (org-element-at-point mk)))))))"##,
        expect,
    );
}

#[test]
fn gamma_at_point_parent_correctly_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "#+BEGIN_CENTER\nA\n#+END_CENTER")
      (search-forward "A")
      (org-element-type
       (org-element-property :parent (org-element-at-point))))))"##,
        expect,
    );
}

#[test]
fn gamma_at_point_blank_line_below_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"H2\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H1\n  \n* H2\n")
      (forward-line)
      (org-element-property :title (org-element-at-point)))))"##,
        expect,
    );
}

#[test]
fn gamma_at_point_beginning_of_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK table-row""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| a | b |")
      (org-element-type (org-element-at-point)))))"##,
        expect,
    );
}

#[test]
fn gamma_at_point_beginning_of_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "- item")
      (org-element-type (org-element-at-point)))))"##,
        expect,
    );
}

#[test]
fn gamma_at_point_closing_line_of_greater_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK center-block""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "#+BEGIN_CENTER\nParagraph\n#+END_CENTER")
      (forward-line 2)
      (org-element-type (org-element-at-point)))))"##,
        expect,
    );
}

#[test]
fn gamma_at_point_blank_line_between_items() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "- Para1\n\n- Para2")
      (forward-line)
      (org-element-type (org-element-at-point)))))"##,
        expect,
    );
}

#[test]
fn gamma_at_point_last_blank_line_in_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "- Para1\n- Para2\n\nPara3")
      (forward-line 2)
      (org-element-type (org-element-at-point)))))"##,
        expect,
    );
}

#[test]
fn gamma_at_point_last_blank_line_at_end_of_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK headline""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Headline\n- Para1\n- Para2\n\nPara3\n* Another headline")
      (forward-line 3)
      (org-element-type (org-element-at-point)))))"##,
        expect,
    );
}

#[test]
fn gamma_at_point_list_ends_at_eof() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "- a")
      (end-of-line)
      (org-element-type (org-element-at-point)))))"##,
        expect,
    );
}

#[test]
fn gamma_at_point_list_in_block_in_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- outer\n  #+begin_center\n  - inner\n  #+end_center")
      (goto-char (point-min))
      (search-forward "inner")
      (org-element-type (org-element-at-point)))))"##,
        expect,
    );
}

#[test]
fn gamma_at_point_eob_empty_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (headline (:standard-properties [1 1 nil nil 5 0 (:title) first-section element t nil nil nil 1 #<killed buffer> [org-element-deferred org-element--headline-deferred nil t] nil (org-data (:standard-properties [1 1 1 5 5 0 nil org-data nil t nil 3 5 nil #<killed buffer> [org-element-deferred org-element--get-global-node-properties nil t] nil nil] :pre-blank 0 :path nil))] :pre-blank 0 :raw-value [org-element-deferred org-element--headline-parse-title (t) t] :title [org-element-deferred org-element--headline-parse-title (t) t] :level [org-element-deferred org-element--headline-parse-title (t) t] :priority [org-element-deferred org-element--headline-parse-title (t) t] :tags [org-element-deferred org-element--headline-parse-title (t) t] :todo-keyword [org-element-deferred org-element--headline-parse-title (t) t] :todo-type [org-element-deferred org-element--headline-parse-title (t) t] :footnote-section-p [org-element-deferred org-element--headline-parse-title (t) t] :archivedp [org-element-deferred org-element--headline-parse-title (t) t] :commentedp [org-element-deferred org-element--headline-parse-title (t) t]))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "* H\n")
      (forward-line)
      (or (org-element-at-point) t))))"##,
        expect,
    );
}

#[test]
fn gamma_at_point_drawer_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (paragraph paragraph paragraph)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; At drawer end.
     (with-temp-buffer (org-mode) (insert ":DRA<point>WER:\ntest\n:END:")
       (goto-char (point-min))
       (org-element-type (org-element-at-point)))
     ;; At drawer end line.
     (with-temp-buffer (org-mode) (insert ":DRAWER:\ntest\n:EN<point>D:")
       (goto-char (point-min))
       (forward-line 2)
       (org-element-type (org-element-at-point)))
     ;; At contents-end.
     (with-temp-buffer (org-mode) (insert ":DRAWER:\ntest\n<point>:END:")
       (goto-char (point-min))
       (forward-line 2)
       (org-element-type (org-element-at-point))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Gamma: org-element with all lineage edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn gamma_lineage_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (paragraph center-block section headline headline org-data)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n#+BEGIN_CENTER\n*bold*\n#+END_CENTER")
      (goto-char (point-min)) (search-forward "bold")
      (mapcar #'car (org-element-lineage (org-element-context))))))"##,
        expect,
    );
}

#[test]
fn gamma_lineage_from_parsed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (paragraph center-block section headline headline org-data)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n#+BEGIN_CENTER\n*bold*\n#+END_CENTER")
      (goto-char (point-min))
      (mapcar #'car
              (org-element-lineage
               (org-element-map (org-element-parse-buffer) 'bold
                 #'identity nil t))))))"##,
        expect,
    );
}

#[test]
fn gamma_lineage_types_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK center-block""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n#+BEGIN_CENTER\n*bold*\n#+END_CENTER")
      (goto-char (point-min)) (search-forward "bold")
      (org-element-type
       (org-element-lineage (org-element-context) 'center-block)))))"##,
        expect,
    );
}

#[test]
fn gamma_lineage_with_self() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (bold paragraph center-block section headline headline org-data)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n#+BEGIN_CENTER\n*bold*\n#+END_CENTER")
      (goto-char (point-min)) (search-forward "bold")
      (mapcar #'car (org-element-lineage (org-element-context) nil t)))))"##,
        expect,
    );
}

#[test]
fn gamma_lineage_types_with_self() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (bold (:standard-properties [27 nil 28 32 33 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [27 27 27 34 34 0 nil nil element t nil nil nil nil #<killed buffer> nil nil (center-block (:standard-properties [12 12 27 34 46 0 nil planning element t nil 27 34 nil #<killed buffer> nil nil (section (:standard-properties [12 12 12 46 46 0 nil section element t nil 12 46 nil #<killed buffer> nil nil (headline (:standard-properties [6 6 12 46 46 0 (:title) section element t nil 14 46 2 #<killed buffer> [org-element-deferred org-element--headline-deferred nil t] nil (headline (:standard-properties [1 1 6 46 46 0 (:title) first-section element t nil 8 46 1 #<killed buffer> [org-element-deferred org-element--headline-deferred nil t] nil (org-data (:standard-properties [1 1 1 46 46 0 nil org-data nil t nil 3 46 nil #<killed buffer> [org-element-deferred org-element--get-global-node-properties nil t] nil nil] :pre-blank 0 :path nil))] :pre-blank 0 :raw-value [org-element-deferred org-element--headline-parse-title (t) t] :title [org-element-deferred org-element--headline-parse-title (t) t] :level [org-element-deferred org-element--headline-parse-title (t) t] :priority [org-element-deferred org-element--headline-parse-title (t) t] :tags [org-element-deferred org-element--headline-parse-title (t) t] :todo-keyword [org-element-deferred org-element--headline-parse-title (t) t] :todo-type [org-element-deferred org-element--headline-parse-title (t) t] :footnote-section-p [org-element-deferred org-element--headline-parse-title (t) t] :archivedp [org-element-deferred org-element--headline-parse-title (t) t] :commentedp [org-element-deferred org-element--headline-parse-title (t) t]))] :pre-blank 0 :raw-value [org-element-deferred org-element--headline-parse-title (t) t] :title [org-element-deferred org-element--headline-parse-title (t) t] :level [org-element-deferred org-element--headline-parse-title (t) t] :priority [org-element-deferred org-element--headline-parse-title (t) t] :tags [org-element-deferred org-element--headline-parse-title (t) t] :todo-keyword [org-element-deferred org-element--headline-parse-title (t) t] :todo-type [org-element-deferred org-element--headline-parse-title (t) t] :footnote-section-p [org-element-deferred org-element--headline-parse-title (t) t] :archivedp [org-element-deferred org-element--headline-parse-title (t) t] :commentedp [org-element-deferred org-element--headline-parse-title (t) t]))]))]))]))]))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n#+BEGIN_CENTER\n*bold*\n#+END_CENTER")
      (goto-char (point-min)) (search-forward "bold")
      (org-element-lineage (org-element-context) '(bold) t))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Gamma: org-element with all granularity edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn gamma_granularity_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Head 1\n** Head 2\n#+BEGIN_CENTER\nCentered paragraph.\n#+END_CENTER\nParagraph \\alpha.")
      (goto-char (point-min))
      (let ((tree (org-element-parse-buffer 'headline)))
        (list (length (org-element-map tree 'headline 'identity))
              (org-element-map tree 'paragraph 'identity))))))"##,
        expect,
    );
}

#[test]
fn gamma_granularity_greater_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Head 1\n** Head 2\n#+BEGIN_CENTER\nCentered paragraph.\n#+END_CENTER\nParagraph \\alpha.")
      (goto-char (point-min))
      (let ((tree (org-element-parse-buffer 'greater-element)))
        (list (length (org-element-map tree 'center-block 'identity))
              (length (org-element-map tree 'paragraph 'identity))
              (org-element-map tree 'entity 'identity)))))"##,
        expect,
    );
}

#[test]
fn gamma_granularity_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Head 1\n** Head 2\n#+BEGIN_CENTER\nCentered paragraph.\n#+END_CENTER\nParagraph \\alpha.")
      (goto-char (point-min))
      (let ((tree (org-element-parse-buffer 'element)))
        (list (length (org-element-map tree 'paragraph 'identity))
              (org-element-map tree 'entity 'identity)))))"##,
        expect,
    );
}

#[test]
fn gamma_granularity_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Head 1\n** Head 2\n#+BEGIN_CENTER\nCentered paragraph.\n#+END_CENTER\nParagraph \\alpha.")
      (goto-char (point-min))
      (let ((tree (org-element-parse-buffer 'object)))
        (length (org-element-map tree 'entity 'identity)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Gamma: org-element with secondary string parsing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn gamma_secondary_string_parsing_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; With headline granularity, title is a string.
     (with-temp-buffer (org-mode) (insert "* Headline")
       (goto-char (point-min))
       (stringp
        (org-element-property
         :title
         (org-element-map (org-element-parse-buffer 'headline) 'headline
                          #'identity nil t))))
     ;; With default granularity, title is a list.
     (with-temp-buffer (org-mode) (insert "* Headline")
       (goto-char (point-min))
       (listp
        (org-element-property
         :title
         (org-element-map (org-element-parse-buffer) 'headline
                          #'identity nil t))))
     ;; org-element-at-point never parses secondary strings.
     (with-temp-buffer (org-mode) (insert "* Headline")
       (goto-char (point-min))
       (listp (org-element-property :title (org-element-at-point)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Gamma: org-element with normalize-contents
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn gamma_normalize_contents_with_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((paragraph nil \"One \" (emphasis nil \"space\") \"\\n Two spaces\") (paragraph nil (verbatim nil \"V\") \"No space\\n  Two\\n   Three\") (paragraph nil \"Two spaces\\n\\n\\nTwo spaces\") (paragraph nil \"No space\\nTwo spaces\\n Three spaces\") (paragraph nil \"1 space\" (line-break) \" 2 spaces\") (verse-block nil \"line 1\\n\\nline 2\") (paragraph nil \" Two spaces \" (bold nil \" and\\nOne space\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; With objects.
   (org-element-normalize-contents
    '(paragraph nil " One " (emphasis nil "space") "\n  Two spaces"))
   ;; Object at start.
   (org-element-normalize-contents
    '(paragraph nil (verbatim nil "V") "No space\n  Two\n   Three"))
   ;; Blank lines.
   (org-element-normalize-contents
    '(paragraph nil "  Two spaces\n\n \n  Two spaces"))
   ;; With argument.
   (org-element-normalize-contents
    '(paragraph nil "No space\n  Two spaces\n   Three spaces") t)
   ;; Line break corner case.
   (org-element-normalize-contents
    '(paragraph nil " 1 space" (line-break) "  2 spaces"))
   ;; Verse block.
   (org-element-normalize-contents
    '(verse-block nil "  line 1\n\n  line 2"))
   ;; Recursive objects.
   (org-element-normalize-contents
    '(paragraph nil "  Two spaces " (bold nil " and\n One space")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Gamma: org-element with visible-only parsing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn gamma_parse_buffer_visible_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"H1\" \"H3\" \"H5\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n** H3 :visible:\n** H4\n** H5 :visible:")
      (goto-char (point-min))
      (org-occur ":visible:")
      (org-element-map (org-element-parse-buffer nil t) 'headline
        (lambda (hl) (org-element-property :raw-value hl))))))"##,
        expect,
    );
}
