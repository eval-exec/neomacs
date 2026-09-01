//! Strong uncovered-features-45 oracle tests — org-element-map complex, org-id, org-refile.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with predicate on headlines
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_map_pred() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" \"B\" \"C\" \"D\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C\n* DONE D")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (org-element-property :raw-value h))
    nil nil nil
    (lambda (h) (string= (org-element-property :todo-keyword h) "DONE"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with nested objects
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_map_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((bold \"bold /italic/ inside\") (italic \"italic\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold /italic/ inside* text")
  (org-element-map (org-element-parse-buffer) '(bold italic)
    (lambda (o) (list (org-element-type o)
                      (org-trim (buffer-substring-no-properties
                                  (org-element-property :contents-begin o)
                                  (org-element-property :contents-end o)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map full document element distribution
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_map_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: T\n* H1\nBody\n** H2\n- a\n- b\n| x |\n* H3\n:PROPERTIES:\n:A: 1\n:END:")
  (let ((types (org-element-map (org-element-parse-buffer) 'element 'org-element-type)))
    (list (length types)
          (sort (delete-dups (copy-sequence types)) 'string<))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-id-get-create on multiple headings
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_id_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"‘org-id-get’ expects a file-visiting buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n* H2\n* H3")
  (goto-char (point-min))
  (let ((r '()))
    (dotimes (_ 3)
      (org-id-get nil 'create)
      (push (org-entry-get nil "ID") r)
      (forward-line))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-refile-get-targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_refile() {
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
fn uf45_map_entries() {
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
// org-element-parent-chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_parent_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"bold\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold* text")
  (search-forward "bold")
  (let* ((obj (org-element-context))
         (chain '()))
    (let ((p obj))
      (while p
        (push (org-element-type p) chain)
        (setq p (org-element-property :parent p))))
    (nreverse chain)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-lineage
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_lineage() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"bold\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold* text")
  (search-forward "bold")
  (let* ((obj (org-element-context))
         (lineage (org-element-lineage obj '(headline paragraph bold) t)))
    (mapcar 'org-element-type lineage)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-contents
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 19)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody\n** H2\nSub")
  (goto-char (point-min))
  (let ((h1 (org-element-at-point)))
    (list (org-element-property :contents-begin h1)
          (org-element-property :contents-end h1))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-type-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_type_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"bold\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody *bold*")
  (list (org-element-type-p (org-element-at-point) 'headline)
        (org-element-type-p (org-element-at-point) 'paragraph)
        (progn (search-forward "bold")
               (org-element-type-p (org-element-context) 'bold))
        (org-element-type-p (org-element-context) 'italic)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-greater-element-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_greater() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-greater-element-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n- item\n:drawer:\n:END:")
  (list (org-element-greater-element-p (org-element-at-point))
        (progn (goto-char (point-min)) (search-forward "item")
               (org-element-greater-element-p (org-element-at-point)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-set-element
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp section)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (let ((h (org-element-at-point)))
    (org-element-set-element h 'section)
    (org-element-type h)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-swap-A-B
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_swap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"A\" \"B\" \"C\") (\"B\" \"A\" \"C\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n* B\n* C")
  (goto-char (point-min))
  (let ((d1 (org-element-map (org-element-parse-buffer) 'headline
              (lambda (h) (org-element-property :raw-value h)))))
    (org-element-swap-A-B (org-element-at-point) (progn (forward-line) (org-element-at-point)))
    (let ((d2 (org-element-map (org-element-parse-buffer) 'headline
                (lambda (h) (org-element-property :raw-value h)))))
      (list d1 d2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-property :robust-begin :robust-end
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_robust() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n|---+---|\n| 1 | 2 |")
  (let ((table (org-element-at-point)))
    (list (org-element-property :robust-begin table)
          (org-element-property :robust-end table))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-parent-element
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_parent_el() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"bold\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold* text")
  (search-forward "bold")
  (let* ((bold (org-element-context))
         (para (org-element-property :parent bold))
         (headline (org-element-property :parent para)))
    (list (org-element-type bold)
          (org-element-type para)
          (org-element-type headline))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-post-affiliated
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_affiliated() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+NAME: tbl\n#+CAPTION: My Table\n| a |")
  (let ((el (org-element-at-point)))
    (list (org-element-property :name el)
          (org-element-property :caption el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-begin/end/post-blank
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 10 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody\n")
  (let ((el (org-element-at-point)))
    (list (org-element-property :begin el)
          (org-element-property :end el)
          (org-element-property :post-blank el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-restriction
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_restriction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-restriction)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-element-restriction 'paragraph)
        (org-element-restriction 'headline)
        (org-element-restriction 'item))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-parse-secondary-string
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf45_secondary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-map)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-element-map (org-element-parse-secondary-string "*bold* /italic/ \\usepackage{a}" (org-element-restriction 'paragraph))
  'object
  (lambda (o) (org-element-type o)))"##,
        expect,
    );
}
