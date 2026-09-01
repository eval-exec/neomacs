//! Strong uncovered-features-2 oracle tests — test features not yet tested.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-at-heading-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_at_heading_at_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:h1 t) (:body nil) (:h2 t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody\n** H2\nSub")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :h1 (org-at-heading-p)) r)
    (forward-line 1)
    (push (list :body (org-at-heading-p)) r)
    (forward-line 1)
    (push (list :h2 (org-at-heading-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-table-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_at_table_at_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:row1 t) (:row2 t) (:body nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |\n\nBody")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :row1 (org-at-table-p)) r)
    (forward-line 1)
    (push (list :row2 (org-at-table-p)) r)
    (forward-line 2)
    (push (list :body (org-at-table-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-item-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_at_item_at_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:item1 t) (:item2 t) (:body nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- item 1\n- item 2\nBody")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :item1 (org-at-item-p)) r)
    (forward-line 1)
    (push (list :item2 (org-at-item-p)) r)
    (forward-line 1)
    (push (list :body (org-at-item-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-planning-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_at_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:h nil) (:sched t) (:body nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nSCHEDULED: <2026-01-15>\nBody")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :h (org-at-planning-p)) r)
    (forward-line 1)
    (push (list :sched (org-at-planning-p)) r)
    (forward-line 1)
    (push (list :body (org-at-planning-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-property-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_at_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:h nil) (:prop t) (:body nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:\nBody")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :h (org-at-property-p)) r)
    (forward-line 2)
    (push (list :prop (org-at-property-p)) r)
    (forward-line 2)
    (push (list :body (org-at-property-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-comment-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_at_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:c1 t) (:normal nil) (:c2 t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "# comment\nNormal\n# another")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :c1 (org-at-comment-p)) r)
    (forward-line 1)
    (push (list :normal (org-at-comment-p)) r)
    (forward-line 1)
    (push (list :c2 (org-at-comment-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-timestamp-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_at_timestamp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:before nil) (:active year) (:inactive year))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text <2026-01-15> and [2026-01-20] here")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :before (org-at-timestamp-p 'lax)) r)
    (search-forward "<")
    (push (list :active (org-at-timestamp-p 'lax)) r)
    (search-forward "[")
    (push (list :inactive (org-at-timestamp-p 'lax)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-block-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_at_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:begin t) (:inside nil) (:body nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC\n(+ 1)\n#+END_SRC\nBody")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :begin (org-at-block-p)) r)
    (forward-line 1)
    (push (list :inside (org-at-block-p)) r)
    (forward-line 2)
    (push (list :body (org-at-block-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-drawer-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_at_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:h nil) (:drawer t) (:body nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:\nBody")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :h (org-at-drawer-p)) r)
    (forward-line 1)
    (push (list :drawer (org-at-drawer-p)) r)
    (forward-line 3)
    (push (list :body (org-at-drawer-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-radio-target-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_at_radio_target() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:target (1 . 4)) (:text nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<<<target>>>\nSee target here")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :target (org-in-regexp "<<<")) r)
    (forward-line 1)
    (push (list :text (org-in-regexp "<<<")) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-footnote-definition-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_at_footnote_def() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function footnote-at-reference-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1]\n\n[fn:1] Definition\nBody")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :text (footnote-at-reference-p)) r)
    (search-forward "[fn:1]")
    (push (list :ref (footnote-at-reference-p)) r)
    (forward-line 1)
    (push (list :def (footnote-at-definition-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-clock-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_at_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:clock t) (:body nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:00] =>  1:00\n:END:\nBody")
  (let ((r '()))
    (goto-char (point-min))
    (search-forward "CLOCK:")
    (push (list :clock (org-at-clock-log-p)) r)
    (forward-line 2)
    (push (list :body (org-at-clock-log-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-struct
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_list_struct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 (0 \"- \" nil nil nil 30)) (10 (2 \"- \" nil nil nil 20)) (20 (2 \"- \" nil nil nil 30)) (30 (0 \"- \" nil nil nil 38)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- item 1\n  - sub 1\n  - sub 2\n- item 2")
  (let ((struct (org-list-struct)))
    (mapcar (lambda (s) (list (car s) (cdr s))) struct)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-prevs-alist
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_list_prevs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 nil) (5 nil) (12 5) (19 1) (23 nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- a\n  - a1\n  - a2\n- b\n  - b1")
  (let ((prevs (org-list-prevs-alist (org-list-struct))))
    (mapcar (lambda (p) (list (car p) (cdr p))) prevs)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-parents-alist
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_list_parents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 nil) (5 1) (12 1) (19 nil) (23 19))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- a\n  - a1\n  - a2\n- b\n  - b1")
  (let ((parents (org-list-parents-alist (org-list-struct))))
    (mapcar (lambda (p) (list (car p) (cdr p))) parents)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-parent-chain deep
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_element_parent_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (paragraph section headline headline headline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody text")
  (goto-char (point-min))
  (search-forward "Body")
  (let* ((para (org-element-at-point))
         (h3 (org-element-property :parent para))
         (h2 (org-element-property :parent h3))
         (h1 (org-element-property :parent h2))
         (tree (org-element-property :parent h1)))
    (list (org-element-type para)
          (org-element-type h3)
          (org-element-type h2)
          (org-element-type h1)
          (org-element-type tree))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-lineage
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_element_lineage() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (section headline headline headline org-data)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody text")
  (goto-char (point-min))
  (search-forward "Body")
  (let* ((para (org-element-at-point))
         (lineage (org-element-lineage para)))
    (mapcar 'org-element-type lineage)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-contents with different parent types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_element_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody\n- item\n| tbl |")
  (let* ((tree (org-element-parse-buffer))
         (h (car (org-element-map tree 'headline (lambda (h) h))))
         (children (mapcar 'org-element-type (org-element-contents h))))
    children)"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-type-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_element_type_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody\n- item\n| tbl |")
  (let* ((tree (org-element-parse-buffer))
         (h (car (org-element-map tree 'headline (lambda (h) h))))
         (p (car (org-element-map tree 'paragraph (lambda (p) p))))
         (pl (car (org-element-map tree 'plain-list (lambda (l) l)))))
    (list (org-element-type-p h 'headline)
          (org-element-type-p h 'paragraph)
          (org-element-type-p p 'paragraph)
          (org-element-type-p p 'headline)
          (org-element-type-p pl 'plain-list)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with :no-recursion
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_element_map_no_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"H2a\" \"H3\" \"H2b\") (\"H2a\" \"H3\" \"H2b\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2a\n*** H3\n** H2b")
  (let* ((tree (org-element-parse-buffer))
         (h1 (car (org-element-map tree 'headline (lambda (h) h))))
         (direct (org-element-map (org-element-contents h1) 'headline
                   (lambda (h) (org-element-property :raw-value h))
                   nil nil nil t))
         (recursive (org-element-map (org-element-contents h1) 'headline
                      (lambda (h) (org-element-property :raw-value h)))))
    (list direct recursive)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with first-match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_element_map_first_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"H1\" \"H2\" \"H3\") \"H1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n* H2\n* H3")
  (let* ((tree (org-element-parse-buffer))
         (all (org-element-map tree 'headline
                (lambda (h) (org-element-property :raw-value h))))
         (first (org-element-map tree 'headline
                  (lambda (h) (org-element-property :raw-value h))
                  nil 'first-match)))
    (list all first)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with info
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_element_map_with_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n* H2\n* H3")
  (let* ((tree (org-element-parse-buffer))
         (result (org-element-map tree 'headline
                   (lambda (h info)
                     (list (org-element-property :raw-value h)
                           (plist-get info :first-match)))
                   nil 'first-match)))
    result)"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-id-get-create
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_id_get_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"‘org-id-get’ expects a file-visiting buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n* H2")
  (goto-char (point-min))
  (let ((id1 (org-id-get nil 'create)))
    (forward-line)
    (let ((id2 (org-id-get nil 'create)))
      (list (stringp id1) (stringp id2) (string= id1 id2)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clock-sum with scope
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_clock_sum_scope() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 150""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:00] =>  1:00\n:END:\n* B\n:LOGBOOK:\nCLOCK: [2026-01-11 14:00]--[2026-01-11 15:30] =>  1:30\n:END:")
  (org-clock-sum))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-store-link and org-insert-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_store_insert_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading\nBody")
  (goto-char (point-min))
  (org-store-link nil)
  (goto-char (point-max))
  (let ((stored (car org-stored-links)))
    (org-insert-link nil stored "click")
    (buffer-substring-no-properties (point-min) (point-max))))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-sort-entries
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_sort_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Zebra\n* Apple\n* Mango\n* Banana")
  (org-sort-entries nil ?a)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (org-element-property :raw-value h))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clone-subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_clone_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clone-subtree)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task\n** Sub1\n** Sub2")
  (goto-char (point-min))
  (org-clone-subtree 2)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :level h)
                      (org-element-property :raw-value h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-copy-subtree / org-paste-subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_copy_paste_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"H1\") (2 \"Sub1\") (1 \"H2\") (2 \"Sub2\") (1 \"H1\") (2 \"Sub1\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** Sub1\n* H2\n** Sub2")
  (goto-char (point-min))
  (org-copy-subtree)
  (goto-char (point-max))
  (org-paste-subtree 1)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :level h)
                      (org-element-property :raw-value h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-mark-subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_mark_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 1 21)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody\n** H2\nSub\n* H1b")
  (goto-char (point-min))
  (org-mark-subtree)
  (let ((m (mark))
        (p (point)))
    (list (< p m) p m)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-narrow-to-subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_narrow_to_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* H1\\nBody 1\\n** H2\\nSub\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody 1\n** H2\nSub\n* H2b\nBody 2")
  (goto-char (point-min))
  (org-narrow-to-subtree)
  (let ((narrowed (buffer-string)))
    (widen)
    (list narrowed)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-end-of-subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_end_of_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (31 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2a\n*** H3\nBody\n** H2b\n* H1b")
  (goto-char (point-min))
  (let ((p1 (progn (org-end-of-subtree) (point))))
    (goto-char (point-min))
    (search-forward "H2a")
    (beginning-of-line)
    (let ((p2 (progn (org-end-of-subtree) (point))))
      (list p1 p2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-cycle-hide-drawers
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_cycle_hide_drawers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n:PROPERTIES:\n:A: 1\n:END:\nBody\n* H2")
  (goto-char (point-min))
  (org-cycle-hide-drawers 'all)
  (let ((hidden1 (get-char-property (search-forward "A") 'invisible)))
    (goto-char (point-max))
    (org-cycle-hide-drawers nil)
    (let ((hidden2 (get-char-property (search-forward "A") 'invisible)))
      (list hidden1 hidden2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-heading
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_toggle_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* Plain text\" \"Existing heading\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Plain text\n* Existing heading")
  (goto-char (point-min))
  (org-toggle-heading)
  (let ((s1 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
    (forward-line)
    (org-toggle-heading)
    (let ((s2 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
      (list s1 s2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-move-item-down
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_move_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"A\" \"B\" \"C\") (\"B\" \"A\" \"C\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C")
  (goto-char (point-min))
  (let ((d1 (org-element-map (org-element-parse-buffer) 'item
              (lambda (i) (org-trim (buffer-substring-no-properties
                                      (org-element-property :contents-begin i)
                                      (org-element-property :contents-end i)))))))
    (org-move-item-down)
    (let ((d2 (org-element-map (org-element-parse-buffer) 'item
                (lambda (i) (org-trim (buffer-substring-no-properties
                                        (org-element-property :contents-begin i)
                                        (org-element-property :contents-end i)))))))
      (list d1 d2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-insert-todo-heading
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_insert_todo_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Existing\" nil) (\"New task\" \"TODO\") (\"Right task\" \"TODO\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Existing")
  (goto-char (point-max))
  (org-insert-todo-heading nil)
  (insert "New task")
  (org-insert-todo-heading 'right)
  (insert "Right task")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :raw-value h)
                      (org-element-property :todo-keyword h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-move-subtree-up / org-move-subtree-down
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_move_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"A\" \"B\" \"C\" \"D\") (\"A\" \"C\" \"B\" \"D\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n* B\n* C\n* D")
  (goto-char (point-min))
  (let ((o1 (org-element-map (org-element-parse-buffer) 'headline
              (lambda (h) (org-element-property :raw-value h)))))
    (forward-line 1)
    (org-move-subtree-down)
    (let ((o2 (org-element-map (org-element-parse-buffer) 'headline
                (lambda (h) (org-element-property :raw-value h)))))
      (list o1 o2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-dblock-update
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_dblock_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK #(\"#+BEGIN: clocktable :maxlevel 2\\n#+CAPTION: Clock summary at [2026-06-15 Mon 12:00]\\n| Headline     | Time   |\\n|--------------+--------|\\n| *Total time* | *0:00* |\\n#+END:\" 83 84 (face org-table) 84 85 (face org-table rear-nonsticky t display (space :relative-width 1)) 85 93 (face org-table) 93 97 (face org-table) 97 98 (face org-table display (space :relative-width 1.001)) 98 99 (face org-table) 99 100 (face org-table rear-nonsticky t display (space :relative-width 1)) 100 104 (face org-table) 104 106 (face org-table) 106 107 (face org-table display (space :relative-width 1.001)) 107 108 (face org-table) 108 109 (face org-table-row) 109 110 (face org-table) 110 134 (face org-table) 134 135 (face org-table-row) 135 136 (face org-table) 136 137 (face org-table rear-nonsticky t display (space :relative-width 1)) 137 149 (org-emphasis t font-lock-multiline t face (bold org-table)) 149 150 (face org-table display (space :relative-width 1.001)) 150 151 (face org-table) 151 152 (face org-table rear-nonsticky t display (space :relative-width 1)) 152 158 (org-emphasis t font-lock-multiline t face (bold org-table)) 158 159 (face org-table display (space :relative-width 1.001)) 159 160 (face org-table) 160 161 (face org-table-row))""##
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN: clocktable :maxlevel 2\n#+END:")
  (goto-char (point-min))
  (org-dblock-update)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-macro-replace-all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_macro_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Undefined Org macro: g; aborting\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: g Hello $1!\n{{{g(World)}}}")
  (let ((raw (buffer-string)))
    (org-macro-replace-all org-macro-templates)
    (list raw (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-try-structure-completion
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_try_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-try-structure-completion)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<s")
  (org-try-structure-completion)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-update-statistics-cookies
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_update_statistics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T [66%]\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T [%]\n- [X] a\n- [ ] b\n- [X] c")
  (goto-char (point-min))
  (org-update-statistics-cookies t)
  (buffer-substring-no-properties (line-beginning-position) (line-end-position)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-match-sparse-tree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf2_match_sparse_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"A\" \"B\" \"C\") nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C")
  (goto-char (point-min))
  (org-match-sparse-tree nil "TODO")
  (let ((v '()) (h '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((hd (org-get-heading t t t t)))
        (when hd
          (if (get-char-property (point) 'invisible) (push hd h) (push hd v))))
      (forward-line))
    (list (nreverse v) (nreverse h))))"##,
        expect,
    );
}
