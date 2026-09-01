//! Ported upstream ERT tests from org-mode's test-org-element.el (9.7.11) - batch 3.
//!
//! Covers remaining tests: lineage-map, property-inherited, cache tests.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ── org-element-lineage-map ──────────────────────────────────────────

#[test]
fn upstream_org_element_lineage_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((paragraph center-block section headline headline org-data) (bold paragraph center-block section headline headline org-data) (\"H2\" \"H1\") bold)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "* H1\n** H2\n#+BEGIN_CENTER\n*bold*\n#+END_CENTER")
      (goto-char (point-min))
      (search-forward "bold")
      (list
       ;; Full lineage map.
       (org-element-lineage-map (org-element-context) #'org-element-type)
       ;; With self.
       (org-element-lineage-map (org-element-context) #'org-element-type nil t)
       ;; FUN as form.
       (org-element-lineage-map
        (org-element-context)
        '(org-element-property :raw-value node))
       ;; FIRST-MATCH.
       (org-element-lineage-map
        (org-element-context) #'org-element-type nil t t)))))"##,
        expect,
    );
}

// ── org-element-property-inherited ───────────────────────────────────

#[test]
fn upstream_org_element_property_inherited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bar baz (bar baz) nil \"nil\" (bar value))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Without self.
   (org-element-property-inherited
    :foo
    (car (org-element-contents
          (org-element-create
           'parent '(:foo bar)
           (org-element-create 'child '(:foo baz))))))
   ;; With self.
   (org-element-property-inherited
    :foo
    (car (org-element-contents
          (org-element-create
           'parent '(:foo bar)
           (org-element-create 'child '(:foo baz)))))
    'with-self)
   ;; Accumulate.
   (org-element-property-inherited
    :foo
    (car (org-element-contents
          (org-element-create
           'parent '(:foo bar)
           (org-element-create 'child '(:foo baz)))))
    'with-self 'accumulate)
   ;; LITERAL-NIL.
   (org-element-property-inherited
    :foo
    (org-element-create 'child '(:foo "nil"))
    'with-self nil t)
   ;; Without LITERAL-NIL.
   (org-element-property-inherited
    :foo
    (org-element-create 'child '(:foo "nil"))
    'with-self)
   ;; PROPERTY as list.
   (org-element-property-inherited
    '(:foo :extra)
    (car (org-element-contents
          (org-element-create
           'parent '(:foo bar :extra value)
           (org-element-create 'child '(:foo baz)))))
    nil 'accumulate)))"##,
        expect,
    );
}

// ── Cache: cache-map ─────────────────────────────────────────────────

#[test]
fn upstream_org_element_cache_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((org-data headline section drawer paragraph headline) (org-data headline section drawer paragraph))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    (list
     ;; Cache map with element granularity.
     (with-temp-buffer
       (org-mode)
       (insert "* headline\n:DRAWER:\nparagraph\n:END:\n* headline 2")
       (goto-char (point-min))
       (org-element-cache-map #'car :granularity 'element))
     ;; Cache map on shorter buffer.
     (with-temp-buffer
       (org-mode)
       (insert "* headline\n:DRAWER:\nparagraph\n:END:")
       (goto-char (point-min))
       (org-element-cache-map #'car :granularity 'element)))))"##,
        expect,
    );
}

// ── Cache: basic expectations ────────────────────────────────────────

#[test]
fn upstream_org_element_cache_shift_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (18 . 23)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Shift positions after insert.
    (with-temp-buffer
      (org-mode)
      (insert "para1\n\npara2\n\npara3")
      (goto-char (point-min))
      (save-excursion (goto-char (point-max)) (org-element-at-point))
      (insert "add")
      (forward-line 4)
      (let ((element (org-element-at-point)))
        (cons (org-element-property :begin element)
              (org-element-property :end element))))))"##,
        expect,
    );
}

#[test]
fn upstream_org_element_cache_reparent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK item""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Re-parent shifted elements.
    (with-temp-buffer
      (org-mode)
      (insert "- item\n\n\n  para1\n  para2")
      (goto-char (point-min))
      (end-of-line)
      (org-element-at-point)
      (save-excursion (goto-char (point-max)) (org-element-at-point))
      (forward-line)
      (delete-char 1)
      (goto-char (point-max))
      (org-element-type
       (org-element-property :parent (org-element-at-point))))))"##,
        expect,
    );
}

#[test]
fn upstream_org_element_cache_sensitive_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"Para2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    (list
     ;; Adding END_EXAMPLE alters structure.
     (with-temp-buffer
       (org-mode)
       (insert "#+BEGIN_EXAMPLE\nPara1\n\nPara2\n")
       (goto-char (point-max))
       (org-element-at-point)
       (insert "#+END_EXAMPLE")
       (search-backward "Para1")
       (org-element-type (org-element-at-point)))
     ;; Adding BEGIN_EXAMPLE alters structure.
     (with-temp-buffer
       (org-mode)
       (insert "Para1\n\nPara2\n#+END_EXAMPLE")
       (goto-char (point-max))
       (org-element-at-point)
       (insert "#+BEGIN_EXAMPLE\n")
       (search-forward "Para2")
       (org-element-type (org-element-at-point))))))"##,
        expect,
    );
}

#[test]
fn upstream_org_element_cache_intersecting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; No intersecting elements.
    (with-temp-buffer
      (org-mode)
      (insert ":DRAWER:\nP1\n\n:END:\n#+END_EXAMPLE")
      (goto-char (point-min))
      (org-element-at-point (point-max))
      (org-element-at-point)
      (insert "#+BEGIN_EXAMPLE")
      (org-element-type (org-element-at-point)))))"##,
        expect,
    );
}

#[test]
fn upstream_org_element_cache_merge_paragraphs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 . 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Modifying last line merges paragraphs.
    (with-temp-buffer
      (org-mode)
      (insert "para1\n\npara2")
      (goto-char (point-max))
      (org-element-at-point)
      (forward-line -1)
      (insert "merge")
      (let ((element (org-element-at-point)))
        (cons (org-element-property :begin element)
              (org-element-property :end element))))))"##,
        expect,
    );
}

#[test]
fn upstream_org_element_cache_fixed_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 . 32)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Modifying first line alters element above.
    (with-temp-buffer
      (org-mode)
      (insert ": fixed-width\n:not-fixed-width")
      (goto-char (point-max))
      (org-element-at-point)
      (search-backward ":")
      (forward-char)
      (insert " ")
      (let ((element (org-element-at-point)))
        (cons (org-element-property :begin element)
              (org-element-property :end element))))))"##,
        expect,
    );
}

// ── Cache: post-blank preservation ───────────────────────────────────

#[test]
fn upstream_org_element_cache_post_blank() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (drawer 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    (with-temp-buffer
      (org-mode)
      (insert ":DRAWER:\ntest\n:END:\n #\nParagraph")
      (goto-char (point-min))
      (org-element-cache-map #'ignore :granularity 'element)
      (list
       (org-element-type (org-element-at-point))
       (org-element-property :post-blank (org-element-at-point (point-min)))))))"##,
        expect,
    );
}

// ── Cache: edits near end ────────────────────────────────────────────

#[test]
fn upstream_org_element_cache_edit_near_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Edits near :end of element.
    (with-temp-buffer
      (org-mode)
      (insert "* H1\nP1\n*H2\n")
      (goto-char (point-min))
      (org-element-cache-map #'ignore :granularity 'element)
      (search-forward "*H2")
      (backward-char 3)
      (insert "Blah")
      (org-element-type (org-element-at-point)))))"##,
        expect,
    );
}

// ── Cache: partial shifting ──────────────────────────────────────────

#[test]
fn upstream_org_element_cache_partial_shift() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Partial shifting: only shift ending positions.
    (with-temp-buffer
      (org-mode)
      (insert "#+BEGIN_CENTER\nPara1\n\nPara2\n\nPara3\n#+END_CENTER")
      (goto-char (point-min))
      (save-excursion (search-forward "3") (org-element-at-point))
      (search-forward "Para2")
      (insert " ")
      (let ((element (org-element-property :parent (org-element-at-point))))
        (equal (cons (org-element-property :begin element)
                     (org-element-property :end element))
               (cons (point-min) (point-max)))))))"##,
        expect,
    );
}

// ── Cache: preserve local structures ─────────────────────────────────

#[test]
fn upstream_org_element_cache_preserve_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK table""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Preserve local structures when re-parenting.
    (with-temp-buffer
      (org-mode)
      (insert "#+begin_center\nP0\n\n\n\n  P1\n  | a | b |\n  | c | d |\n#+end_center")
      (goto-char (point-min))
      (save-excursion (search-forward "| c |") (org-element-at-point))
      (insert "- item")
      (search-forward "| c |")
      (beginning-of-line)
      (org-element-type
       (org-element-property :parent (org-element-at-point))))))"##,
        expect,
    );
}

// ── Cache: propagate list structures ─────────────────────────────────

#[test]
fn upstream_org_element_cache_list_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; When re-parenting, propagate changes to list structures.
    (with-temp-buffer
      (org-mode)
      (insert "\n  Para\n  - item")
      (goto-char (point-max))
      (org-element-at-point)
      (goto-char (point-min))
      (insert "- Top\n")
      (search-forward "- item")
      (beginning-of-line)
      (length (org-element-property :structure (org-element-at-point))))))"##,
        expect,
    );
}

// ── Cache: add/remove sensitive lines ────────────────────────────────

#[test]
fn upstream_org_element_cache_remove_sensitive_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Removing a line alters structure.
    (with-temp-buffer
      (org-mode)
      (insert "# +BEGIN_EXAMPLE\nPara1\n\nPara2\n#+END_EXAMPLE")
      (goto-char (point-max))
      (org-element-at-point)
      (forward-char)
      (delete-char 1)
      (search-forward "Para2")
      (org-element-type (org-element-at-point))))))"##,
        expect,
    );
}

// ── Cache: slurp obsolete elements ───────────────────────────────────

#[test]
fn upstream_org_element_cache_slurp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Correctly slurp obsolete elements inside a new element.
    (with-temp-buffer
      (org-mode)
      (insert ":DRAWER:\nP1\n\nP2\n#+END_EXAMPLE\n:END:")
      (goto-char (point-min))
      (org-element-at-point (point-max))
      (save-excursion
        (re-search-forward "P2")
        (list (org-element-type (org-element-at-point))))
      (insert "#+BEGIN_EXAMPLE")
      (re-search-forward "P2")
      (org-element-type (org-element-at-point)))))"##,
        expect,
    );
}

// ── Cache: empty line at end ─────────────────────────────────────────

#[test]
fn upstream_org_element_cache_empty_line_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (headline (:standard-properties [1 1 nil nil 5 0 (:title) first-section element t nil nil nil 1 #<killed buffer> [org-element-deferred org-element--headline-deferred nil t] nil (org-data (:standard-properties [1 1 1 5 5 0 nil org-data nil t nil 3 5 nil #<killed buffer> [org-element-deferred org-element--get-global-node-properties nil t] nil nil] :pre-blank 0 :path nil))] :pre-blank 0 :raw-value [org-element-deferred org-element--headline-parse-title (t) t] :title [org-element-deferred org-element--headline-parse-title (t) t] :level [org-element-deferred org-element--headline-parse-title (t) t] :priority [org-element-deferred org-element--headline-parse-title (t) t] :tags [org-element-deferred org-element--headline-parse-title (t) t] :todo-keyword [org-element-deferred org-element--headline-parse-title (t) t] :todo-type [org-element-deferred org-element--headline-parse-title (t) t] :footnote-section-p [org-element-deferred org-element--headline-parse-title (t) t] :archivedp [org-element-deferred org-element--headline-parse-title (t) t] :commentedp [org-element-deferred org-element--headline-parse-title (t) t]))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Do not error at eob on an empty line.
    (with-temp-buffer
      (org-mode)
      (insert "* H\n")
      (goto-char (point-min))
      (forward-line)
      (or (org-element-at-point) t))))"##,
        expect,
    );
}

// ── Cache: drawer at end ─────────────────────────────────────────────

#[test]
fn upstream_org_element_cache_drawer_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (drawer drawer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    (list
     ;; Return greater element when outside contents.
     (with-temp-buffer
       (org-mode)
       (insert ":DRAWER:\ntest\n:END:")
       (goto-char (point-max))
       (org-element-type (org-element-at-point)))
     ;; Return greater element at :contents-end.
      (with-temp-buffer
        (org-mode)
        (insert ":DRAWER:\ntest\n:END:")
        (goto-char (point-min))
        (forward-line 2)
        (org-element-type (org-element-at-point))))))"##,
        expect,
    );
}

#[test]
fn org_element_parse_cache_edit_reparse_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 35 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (with-temp-buffer
    (org-mode)
    (insert "* Alpha :work:\nAlpha body.\n** Beta\nBeta body.\n*** Gamma\nGamma body.\n")
    (let* ((tree-1 (org-element-parse-buffer))
           (headlines-1
            (org-element-map tree-1 'headline
              (lambda (h)
                (list (org-element-property :raw-value h)
                      (org-element-property :level h)
                      (org-element-property :tags h))))))
      (goto-char (point-max))
      (insert "\n** Delta\nDelta body.\n")
      (let* ((tree-2 (org-element-parse-buffer))
             (headlines-2
              (org-element-map tree-2 'headline
                (lambda (h)
                  (list (org-element-property :raw-value h)
                        (org-element-property :level h)
                        (org-element-property :tags h))))))
        (goto-char (point-min))
        (search-forward "Alpha")
        (beginning-of-line)
        (org-set-tags '("work" "urgent"))
        (let* ((tree-3 (org-element-parse-buffer))
               (headlines-3
                (org-element-map tree-3 'headline
                  (lambda (h)
                    (list (org-element-property :raw-value h)
                          (org-element-property :tags h))))))
          (list headlines-1 headlines-2 headlines-3
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))))"##,
        expect,
    );
}
