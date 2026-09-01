//! Strong uncovered-features-4 oracle tests — test features not yet tested.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-shifttab cycling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_shifttab_cycling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:after-shifttab org-fold-outline org-fold-outline))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody")
  (goto-char (point-min))
  (let ((s '()))
    (org-shifttab)
    (push (list :after-shifttab
                (get-char-property (search-forward "H2") 'invisible)
                (progn (forward-line) (get-char-property (point) 'invisible)))
          s)
    (nreverse s)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-meta-return at different positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_meta_return_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"H1\" \"New\" \"H2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n* H2")
  (goto-char (point-min))
  (end-of-line)
  (org-meta-return)
  (insert "New")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (org-element-property :raw-value h))))"##,
        expect,
    );
}

#[test]
fn uf4_meta_return_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" \"New\" \"B\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B")
  (goto-char (point-min))
  (end-of-line)
  (org-meta-return)
  (insert "New")
  (org-element-map (org-element-parse-buffer) 'item
    (lambda (i) (org-trim (buffer-substring-no-properties
                            (org-element-property :contents-begin i)
                            (org-element-property :contents-end i))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-shiftmetaright / org-shiftmetaleft
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_shiftmeta_right_left() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((1 \"H1\") (2 \"H2\") (1 \"H3\")) ((1 \"H1\") (1 \"H2\") (1 \"H3\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n* H2\n* H3")
  (goto-char (point-min))
  (forward-line 1)
  (org-shiftmetaright)
  (let ((d1 (org-element-map (org-element-parse-buffer) 'headline
              (lambda (h) (list (org-element-property :level h)
                                (org-element-property :raw-value h))))))
    (org-shiftmetaleft)
    (let ((d2 (org-element-map (org-element-parse-buffer) 'headline
                (lambda (h) (list (org-element-property :level h)
                                  (org-element-property :raw-value h))))))
      (list d1 d2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-shiftmetaup / org-shiftmetadown
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_shiftmeta_up_down() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"A\" \"B\" \"C\") (\"A\" \"C\" \"B\") (\"A\" \"B\" \"C\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n* B\n* C")
  (goto-char (point-min))
  (forward-line 1)
  (let ((d1 (org-element-map (org-element-parse-buffer) 'headline
              (lambda (h) (org-element-property :raw-value h)))))
    (org-shiftmetadown)
    (let ((d2 (org-element-map (org-element-parse-buffer) 'headline
                (lambda (h) (org-element-property :raw-value h)))))
      (org-shiftmetaup)
      (let ((d3 (org-element-map (org-element-parse-buffer) 'headline
                  (lambda (h) (org-element-property :raw-value h)))))
        (list d1 d2 d3)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-metaright / org-metaleft on list
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_meta_right_left_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((\"- \" nil) (\"- \" nil) (\"- \" nil)) ((\"- \" nil) (\"- \" nil) (\"- \" nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C")
  (goto-char (point-min))
  (forward-line 1)
  (org-metaright)
  (let ((d1 (org-element-map (org-element-parse-buffer) 'item
              (lambda (i) (list (org-element-property :bullet i)
                                (org-element-property :level i))))))
    (org-metaleft)
    (let ((d2 (org-element-map (org-element-parse-buffer) 'item
                (lambda (i) (list (org-element-property :bullet i)
                                  (org-element-property :level i))))))
      (list d1 d2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-metaup / org-metadown on list
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_meta_up_down_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"A\" \"B\" \"C\") (\"A\" \"C\" \"B\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C")
  (goto-char (point-min))
  (forward-line 1)
  (let ((d1 (org-element-map (org-element-parse-buffer) 'item
              (lambda (i) (org-trim (buffer-substring-no-properties
                                      (org-element-property :contents-begin i)
                                      (org-element-property :contents-end i)))))))
    (org-metadown)
    (let ((d2 (org-element-map (org-element-parse-buffer) 'item
                (lambda (i) (org-trim (buffer-substring-no-properties
                                        (org-element-property :contents-begin i)
                                        (org-element-property :contents-end i)))))))
      (list d1 d2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-return-at-point
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_return_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"item\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- item")
  (goto-char (point-max))
  (org-return)
  (insert "new")
  (org-element-map (org-element-parse-buffer) 'item
    (lambda (i) (org-trim (buffer-substring-no-properties
                            (org-element-property :contents-begin i)
                            (org-element-property :contents-end i))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-context at different positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_element_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:h headline) (:bold bold))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody *bold* text")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :h (org-element-type (org-element-context))) r)
    (search-forward "bold")
    (push (list :bold (org-element-type (org-element-context))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-get-category
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_get_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:h1 \"custom\") (:h2 \"custom\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+CATEGORY: default\n* H1\n:PROPERTIES:\n:CATEGORY: custom\n:END:\n** H2")
  (let ((r '()))
    (goto-char (point-min))
    (search-forward "H1")
    (push (list :h1 (org-get-category)) r)
    (search-forward "H2")
    (push (list :h2 (org-get-category)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-get with ITEM
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_entry_get_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Heading\" \"TODO\" \"A\" \":tag:\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Heading :tag:")
  (goto-char (point-min))
  (list (org-entry-get nil "ITEM")
        (org-entry-get nil "TODO")
        (org-entry-get nil "PRIORITY")
        (org-entry-get nil "TAGS")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-get with SCHEDULED/DEADLINE/CLOSED
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_entry_get_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"<2026-01-15>\" nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\nCLOSED: [2026-01-10]")
  (goto-char (point-min))
  (list (org-entry-get nil "SCHEDULED")
        (org-entry-get nil "DEADLINE")
        (org-entry-get nil "CLOSED")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-get with custom properties
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_entry_get_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"myid\" \"2h\" \"test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:CUSTOM_ID: myid\n:EFFORT: 2h\n:VAR: test\n:END:")
  (goto-char (point-min))
  (list (org-entry-get nil "CUSTOM_ID")
        (org-entry-get nil "EFFORT")
        (org-entry-get nil "VAR")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-properties with 'standard flag
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_entry_properties_standard() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"CATEGORY\" . \"???\") (\"B\" . \"2\") (\"A\" . \"1\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:B: 2\n:END:")
  (goto-char (point-min))
  (org-entry-properties nil 'standard))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-put / org-entry-get / org-entry-delete
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_entry_put_get_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1\" \"2\" nil \"2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (org-entry-put nil "A" "1")
  (org-entry-put nil "B" "2")
  (let ((v1 (org-entry-get nil "A"))
        (v2 (org-entry-get nil "B")))
    (org-entry-delete nil "A")
    (list v1 v2 (org-entry-get nil "A") (org-entry-get nil "B"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-get-repeat
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_get_repeat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"+1w\" \"+1m\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO W\nSCHEDULED: <2026-01-15 +1w>\n* TODO M\nDEADLINE: <2026-01-20 +1m>\n* TODO N")
  (goto-char (point-min))
  (let ((r1 (org-get-repeat)))
    (forward-line 2)
    (let ((r2 (org-get-repeat)))
      (forward-line 2)
      (list r1 r2 (org-get-repeat)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clock-sum-current-entry
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_clock_sum_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-sum-current-entry)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\n:END:")
  (goto-char (point-min))
  (org-clock-sum-current-entry))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-columns-get-format
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_columns_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-columns-get-format)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+COLUMNS: %25ITEM %TODO %3PRIORITY %TAGS %V\n* TODO [#A] T :tag:\n:PROPERTIES:\n:V: val\n:END:")
  (goto-char (point-min))
  (org-columns-get-format))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-refile-get-targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_refile_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"P1\" \"P2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* P1\n** T1\n*** S1\n* P2\n** T2")
  (mapcar 'car (org-refile-get-targets nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-map-entries with match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_map_entries_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C")
  (list (org-map-entries (lambda () (org-get-heading t t t t)) "TODO" 'file)
        (org-map-entries (lambda () (org-get-heading t t t t)) "DONE" 'file)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-id-get-create
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_id_get_create() {
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
// org-store-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_store_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading\nBody")
  (goto-char (point-min))
  (org-store-link nil)
  (list (car org-stored-links)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-struct
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_list_struct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 (0 \"- \" nil nil nil 19)) (5 (2 \"- \" nil nil nil 12)) (12 (2 \"- \" nil nil nil 19)) (19 (0 \"- \" nil nil nil 29)) (23 (2 \"- \" nil nil nil 29)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- a\n  - a1\n  - a2\n- b\n  - b1")
  (let ((struct (org-list-struct)))
    (mapcar (lambda (s) (list (car s) (cdr s))) struct)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-lineage
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_element_lineage() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (section headline headline headline org-data)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody")
  (goto-char (point-min))
  (search-forward "Body")
  (let* ((para (org-element-at-point))
         (lineage (org-element-lineage para)))
    (mapcar 'org-element-type lineage)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-contents
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_element_contents() {
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
fn uf4_element_type_p() {
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
// org-element-map no-recursion
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_element_map_no_recurse() {
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
// org-element-map first-match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_element_map_first_match() {
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
// org-element-parent-chain deep
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf4_element_parent_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (paragraph section headline headline headline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody")
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
