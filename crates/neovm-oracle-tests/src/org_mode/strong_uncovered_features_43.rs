//! Strong uncovered-features-43 oracle tests — org-macro, org-entities, org-footnote.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-macro-replace-all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_macro_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Undefined Org macro: greeting; aborting\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: greeting Hello $1!\n{{{greeting(World)}}} and {{{greeting(Elisp)}}}")
  (let ((raw (buffer-string)))
    (org-macro-replace-all org-macro-templates)
    (list raw (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-macro-accumulate-arguments
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_macro_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-macro-accumulate-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-macro-accumulate-arguments "{{{macro(a,b,c)}}}" 0)"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-macro-expand-macro
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_macro_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-macro-expand-macro)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: greeting Hello $1!\n{{{greeting(World)}}}")
  (let ((org-macro-templates (org-macro--collect-macros)))
    (org-macro-expand-macro "{{{greeting(World)}}}" org-macro-templates)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-macro--collect-macros
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_macro_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"b\" . \"2\") (\"a\" . \"1\") (\"author\") (\"email\") (\"title\") (\"date\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: a 1\n#+MACRO: b 2\n{{{a}}} {{{b}}}")
  (org-macro--collect-macros))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entity-get
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_entity_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-entity-get)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-entity-get "alpha")
        (org-entity-get "beta")
        (org-entity-get "gamma")
        (org-entity-get "nonexistent"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entity-get-utf-8
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_entity_utf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-entity-get-utf-8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-entity-get-utf-8 "alpha")
        (org-entity-get-utf-8 "beta")
        (org-entity-get-utf-8 "gamma"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entity-get-latex
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_entity_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-entity-get-latex)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-entity-get-latex "alpha")
        (org-entity-get-latex "beta")
        (org-entity-get-latex "gamma"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entity-get-html
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_entity_html() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-entity-get-html)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-entity-get-html "alpha")
        (org-entity-get-html "beta")
        (org-entity-get-html "gamma"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entity-get-ascii
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_entity_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-entity-get-ascii)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-entity-get-ascii "alpha")
        (org-entity-get-ascii "beta")
        (org-entity-get-ascii "gamma"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-footnote-new
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_footnote_new() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Text[fn:1]\\n\\n* Footnotes\\n\\n[fn:1] \\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text")
  (goto-char (point-max))
  (condition-case nil
      (org-footnote-new)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-footnote-action
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_footnote_action() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (19 \"[fn:1] Def\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1]\n\n[fn:1] Def")
  (goto-char (point-min))
  (search-forward "[fn:1]")
  (goto-char (match-beginning 0))
  (condition-case nil
      (org-footnote-action)
    (error nil))
  (list (point)
        (buffer-substring-no-properties
         (line-beginning-position)
         (line-end-position))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-footnote-goto-definition
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_footnote_goto() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"[fn:1] Def\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1]\n\n[fn:1] Def")
  (goto-char (point-min))
  (condition-case nil
      (org-footnote-goto-definition "1")
    (error nil))
  (buffer-substring-no-properties (line-beginning-position) (line-end-position)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-footnote-goto-previous-reference
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_footnote_prev() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 16""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1] more[fn:1]\n\n[fn:1] Def")
  (goto-char (point-max))
  (condition-case nil
      (org-footnote-goto-previous-reference "1")
    (error nil))
  (point))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-footnote-delete
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_footnote_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Text\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1]\n\n[fn:1] Def")
  (goto-char (point-min))
  (condition-case nil
      (org-footnote-delete "1")
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-footnote-renumber-fn:A
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_footnote_renumber() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Text[fn:a] more[fn:b]\\n\\n[fn:a] DefA\\n[fn:b] DefB\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:a] more[fn:b]\n\n[fn:a] DefA\n[fn:b] DefB")
  (condition-case nil
      (org-footnote-renumber-fn:A)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-footnote-normalize
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_footnote_normalize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Text[fn:1]\\n\\n* Footnotes\\n\\n[fn:1] Def\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1]\n\n[fn:1] Def")
  (condition-case nil
      (org-footnote-normalize)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-footnote-all-notes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_footnote_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-footnote-all-notes)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1] more[fn:2]\n\n[fn:1] Def1\n[fn:2] Def2")
  (org-footnote-all-notes))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-footnote-at-reference-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_footnote_at_ref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ref nil) (:def nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1]\n\n[fn:1] Def")
  (let ((r '()))
    (goto-char (point-min))
    (search-forward "[fn:1]")
    (push (list :ref (org-footnote-at-reference-p)) r)
    (search-forward "[fn:1]")
    (push (list :def (org-footnote-at-reference-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-footnote-at-definition-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_footnote_at_def() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ref nil) (:def (\"1\" 13 23 \"Def\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1]\n\n[fn:1] Def")
  (let ((r '()))
    (goto-char (point-min))
    (search-forward "[fn:1]")
    (push (list :ref (org-footnote-at-definition-p)) r)
    (search-forward "[fn:1]")
    (push (list :def (org-footnote-at-definition-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-footnote-unique-label
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_footnote_unique() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"2\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1]\n\n[fn:1] Def")
  (org-footnote-unique-label))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-footnote-insert-definition
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf43_footnote_insert_def() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Text\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text")
  (goto-char (point-max))
  (condition-case nil
      (org-footnote-insert-definition "test" "Test definition")
    (error nil))
  (buffer-string))"##,
        expect,
    );
}
