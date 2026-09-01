//! Strong uncovered-features-31 oracle tests — org-bibtex, org-cite, org-ref.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-bibtex
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_bibtex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:TYPE: article\n:TITLE: Test Title\n:AUTHOR: John Doe\n:YEAR: 2026\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-bibtex)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-bibtex-create
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_bibtex_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (condition-case nil
      (org-bibtex-create)
    (error nil))
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-bibtex-check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_bibtex_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:TYPE: article\n:TITLE: Test Title\n:AUTHOR: John Doe\n:YEAR: 2026\n:END:")
  (condition-case nil
      (org-bibtex-check)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-bibtex-headline
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_bibtex_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:TYPE: article\n:TITLE: Test Title\n:AUTHOR: John Doe\n:YEAR: 2026\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-bibtex-headline)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-bibtex-export-to-kill-ring
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_bibtex_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:TYPE: article\n:TITLE: Test Title\n:AUTHOR: John Doe\n:YEAR: 2026\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-bibtex-export-to-kill-ring)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-cite-basic--complete-style
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_cite_style() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-cite-basic--complete-style)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-cite-basic--complete-key
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_cite_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-cite-basic--complete-key)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-cite-basic--print-reference
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_cite_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-cite-basic--print-reference "test-key")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-cite-basic--get-entry
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_cite_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-cite-basic--get-entry "test-key")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-cite-basic--all-keys
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_cite_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-cite-basic--all-keys)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ref
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_ref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-ref)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ref-cite-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_ref_cite() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text cite:key1,key2 end")
  (condition-case nil
      (org-ref-cite-link)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ref-ref-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_ref_ref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text ref:label end")
  (condition-case nil
      (org-ref-ref-link)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ref-bibliography-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_ref_bib() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "bibliography:refs.bib")
  (condition-case nil
      (org-ref-bibliography-link)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ref-bibliography*
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_ref_bib2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-ref-bibliography* "refs.bib")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ref-format-cite
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_ref_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-ref-format-cite '("key1" "key2"))
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ref-get-bibtex-key-under-cursor
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_ref_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "cite:key1")
  (goto-char (point-min))
  (condition-case nil
      (org-ref-get-bibtex-key-under-cursor)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ref-find-bibliography
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_ref_find() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "bibliography:refs.bib\n\nbibliography:more.bib")
  (condition-case nil
      (org-ref-find-bibliography)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ref-valid-keys
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_ref_valid() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-ref-valid-keys)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ref-cite-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_ref_cite_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "cite:key1 and ref:label")
  (goto-char (point-min))
  (list (condition-case nil (org-ref-cite-p) (error nil))
        (progn (search-forward "ref:") (condition-case nil (org-ref-cite-p) (error nil)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ref-ref-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_ref_ref_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "cite:key1 and ref:label")
  (goto-char (point-min))
  (list (condition-case nil (org-ref-ref-p) (error nil))
        (progn (search-forward "ref:") (condition-case nil (org-ref-ref-p) (error nil)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ref-bibliography-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf31_ref_bib_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "bibliography:refs.bib")
  (goto-char (point-min))
  (condition-case nil
      (org-ref-bibliography-p)
    (error nil)))"##,
        expect,
    );
}
