//! Peta-strict combo tests for org-mode extreme edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-parse-buffer granularity combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_parse_buffer_granularity_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\nPara *bold* /italic/.\n** H2\nMore text.\n")
      (goto-char (point-min))
      (list
       ;; Default granularity.
       (length (org-element-map (org-element-parse-buffer) t #'identity))
       ;; Element granularity.
       (length (org-element-map (org-element-parse-buffer 'element) t #'identity))
       ;; Greater-element granularity.
       (length (org-element-map (org-element-parse-buffer 'greater-element) t #'identity))
       ;; Object granularity.
       (length (org-element-map (org-element-parse-buffer 'object) t #'identity))
       ;; Headline granularity.
       (length (org-element-map (org-element-parse-buffer 'headline) t #'identity)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache-map combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_map_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    (with-temp-buffer (org-mode)
      (insert "* headline\n:DRAWER:\nparagraph\n:END:\n* headline 2")
      (goto-char (point-min))
      (list
       ;; Cache map with element granularity.
       (org-element-cache-map #'car :granularity 'element))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache shift combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_shift_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (18 . 23)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Shift positions after insert.
    (with-temp-buffer (org-mode)
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

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache reparent combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_reparent_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK item""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Re-parent shifted elements.
    (with-temp-buffer (org-mode)
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

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache sensitive change combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_sensitive_change_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"Para2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    (list
     ;; Adding END_EXAMPLE alters structure.
     (with-temp-buffer (org-mode)
       (insert "#+BEGIN_EXAMPLE\nPara1\n\nPara2\n")
       (goto-char (point-max))
       (org-element-at-point)
       (insert "#+END_EXAMPLE")
       (search-backward "Para1")
       (org-element-type (org-element-at-point)))
     ;; Adding BEGIN_EXAMPLE alters structure.
     (with-temp-buffer (org-mode)
       (insert "Para1\n\nPara2\n#+END_EXAMPLE")
       (goto-char (point-max))
       (org-element-at-point)
       (insert "#+BEGIN_EXAMPLE\n")
       (search-forward "Para2")
       (org-element-type (org-element-at-point))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache intersecting combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_intersecting_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; No intersecting elements.
    (with-temp-buffer (org-mode)
      (insert ":DRAWER:\nP1\n\n:END:\n#+END_EXAMPLE")
      (goto-char (point-min))
      (org-element-at-point (point-max))
      (org-element-at-point)
      (insert "#+BEGIN_EXAMPLE")
      (org-element-type (org-element-at-point)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache merge combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_merge_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 . 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Modifying last line merges paragraphs.
    (with-temp-buffer (org-mode)
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

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache fixed-width combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_fixed_width_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 . 32)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Modifying first line alters element above.
    (with-temp-buffer (org-mode)
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

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache post-blank combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_post_blank_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (drawer 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    (with-temp-buffer (org-mode)
      (insert ":DRAWER:\ntest\n:END:\n #\nParagraph")
      (goto-char (point-min))
      (org-element-cache-map #'ignore :granularity 'element)
      (list
       (org-element-type (org-element-at-point))
       (org-element-property :post-blank (org-element-at-point (point-min)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache edit-near-end combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_edit_near_end_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Edits near :end of element.
    (with-temp-buffer (org-mode)
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

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache partial-shift combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_partial_shift_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Partial shifting: only shift ending positions.
    (with-temp-buffer (org-mode)
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

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache preserve-structures combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_preserve_structures_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK table""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Preserve local structures when re-parenting.
    (with-temp-buffer (org-mode)
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

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache list-structures combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_list_structures_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; When re-parenting, propagate changes to list structures.
    (with-temp-buffer (org-mode)
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

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache remove-sensitive-line combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_remove_sensitive_line_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Removing a line alters structure.
    (with-temp-buffer (org-mode)
      (insert "# +BEGIN_EXAMPLE\nPara1\n\nPara2\n#+END_EXAMPLE")
      (goto-char (point-max))
      (org-element-at-point)
      (forward-char)
      (delete-char 1)
      (search-forward "Para2")
      (org-element-type (org-element-at-point)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache slurp combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_slurp_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Correctly slurp obsolete elements inside a new element.
    (with-temp-buffer (org-mode)
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

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache empty-line-end combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_empty_line_end_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (headline (:standard-properties [1 1 nil nil 5 0 (:title) first-section element t nil nil nil 1 #<killed buffer> [org-element-deferred org-element--headline-deferred nil t] nil (org-data (:standard-properties [1 1 1 5 5 0 nil org-data nil t nil 3 5 nil #<killed buffer> [org-element-deferred org-element--get-global-node-properties nil t] nil nil] :pre-blank 0 :path nil))] :pre-blank 0 :raw-value [org-element-deferred org-element--headline-parse-title (t) t] :title [org-element-deferred org-element--headline-parse-title (t) t] :level [org-element-deferred org-element--headline-parse-title (t) t] :priority [org-element-deferred org-element--headline-parse-title (t) t] :tags [org-element-deferred org-element--headline-parse-title (t) t] :todo-keyword [org-element-deferred org-element--headline-parse-title (t) t] :todo-type [org-element-deferred org-element--headline-parse-title (t) t] :footnote-section-p [org-element-deferred org-element--headline-parse-title (t) t] :archivedp [org-element-deferred org-element--headline-parse-title (t) t] :commentedp [org-element-deferred org-element--headline-parse-title (t) t]))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    ;; Do not error at eob on an empty line.
    (with-temp-buffer (org-mode)
      (insert "* H\n")
      (goto-char (point-min))
      (forward-line)
      (or (org-element-at-point) t))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Peta: org-element with all org-element-cache drawer-end combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn peta_all_cache_drawer_end_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (drawer drawer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-element-use-cache t))
    (list
     ;; Return greater element when outside contents.
     (with-temp-buffer (org-mode)
       (insert ":DRAWER:\ntest\n:END:")
       (goto-char (point-max))
       (org-element-type (org-element-at-point)))
     ;; Return greater element at :contents-end.
     (with-temp-buffer (org-mode)
       (insert ":DRAWER:\ntest\n:END:")
       (goto-char (point-min))
       (forward-line 2)
       (org-element-type (org-element-at-point))))))"##,
        expect,
    );
}
