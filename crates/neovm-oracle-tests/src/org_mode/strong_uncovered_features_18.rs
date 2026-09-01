//! Strong uncovered-features-18 oracle tests — complex state capture.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:error (user-error \"No operator defined for property A\") \"* T\\n:PROPERTIES:\\n:A: 1\\n:END:\" \"* T\\n:PROPERTIES:\\n:A: 1\\n:END:\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:")
  (goto-char (point-min))
  (search-forward ":A:")
  (let ((before (buffer-string)))
    (condition-case err
        (let ((unread-command-events (list ?c)))
          (org-ctrl-c-ctrl-c)
          (list :ok before (buffer-string)))
      (error (list :error err before (buffer-string))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"Link\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text [[http://example.com][Link]] end")
  (search-forward "Link")
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on timestamp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_ts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T\\nSCHEDULED: <2026-01-15 Thu>\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15>")
  (goto-char (point-min))
  (search-forward "<2026")
  (backward-char 2)
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on footnote
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_foot() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Text[fn:1]\\n\\n[fn:1] Def\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1]\n\n[fn:1] Def")
  (goto-char (point-min))
  (search-forward "[fn:1]")
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on table
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| a | b |\\n|---+---|\\n| 1 | 2 |\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n|---+---|\n| 1 | 2 |")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on planning line
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_plan() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error #(\"‘C-c C-c’ can do nothing useful here\" 1 8 (font-lock-face help-key-binding face help-key-binding)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\nSCHEDULED: <2026-01-15>\nBody")
  (goto-char (point-min))
  (forward-line)
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on clock
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T\\nCLOCK: [2026-01-10 10:00]\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nCLOCK: [2026-01-10 10:00]")
  (goto-char (point-min))
  (search-forward "CLOCK:")
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on statistic cookie
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_stat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T [1/2]\\n- [X] a\\n- [ ] b\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T [1/2]\n- [X] a\n- [ ] b")
  (goto-char (point-min))
  (search-forward "[1/2]")
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on tag
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T :tag:")
  (goto-char (point-max))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on todo
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_todo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T")
  (goto-char (point-min))
  (search-forward "TODO")
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on priority
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_prio() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* [#A] T")
  (goto-char (point-min))
  (search-forward "[#A]")
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on drawer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error #(\"‘C-c C-c’ can do nothing useful here\" 1 8 (font-lock-face help-key-binding face help-key-binding)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:MYDRAWER:\nData\n:END:")
  (goto-char (point-min))
  (search-forward ":MYDRAWER:")
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on fixed-width
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error #(\"‘C-c C-c’ can do nothing useful here\" 1 8 (font-lock-face help-key-binding face help-key-binding)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert ": fixed\n: lines")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on comment
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error #(\"‘C-c C-c’ can do nothing useful here\" 1 8 (font-lock-face help-key-binding face help-key-binding)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "# Comment")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on keyword
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_keyword() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK #(\"#+TITLE: Test\" 0 13 (fontified nil))""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on quote block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_quote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error #(\"‘C-c C-c’ can do nothing useful here\" 1 8 (font-lock-face help-key-binding face help-key-binding)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_QUOTE\nQ\n#+END_QUOTE")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on center block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_center() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error #(\"‘C-c C-c’ can do nothing useful here\" 1 8 (font-lock-face help-key-binding face help-key-binding)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_CENTER\nC\n#+END_CENTER")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on export block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error #(\"‘C-c C-c’ can do nothing useful here\" 1 8 (font-lock-face help-key-binding face help-key-binding)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_EXPORT html\n<b>Bold</b>\n#+END_EXPORT")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on verse block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_verse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error #(\"‘C-c C-c’ can do nothing useful here\" 1 8 (font-lock-face help-key-binding face help-key-binding)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_VERSE\nLine\n#+END_VERSE")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on list item
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"- item\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- item")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on heading
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on paragraph
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_para() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error #(\"‘C-c C-c’ can do nothing useful here\" 1 8 (font-lock-face help-key-binding face help-key-binding)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Paragraph text")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on horizontal rule
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_hr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error #(\"‘C-c C-c’ can do nothing useful here\" 1 8 (font-lock-face help-key-binding face help-key-binding)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "-----\nText")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on latex fragment
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"$x\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text $x^2$ end")
  (search-forward "$x")
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on entity
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_entity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"\\\\alpha\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text \\alpha end")
  (search-forward "\\alpha")
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on macro
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"{{{m}}}\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: m Hi\nText {{{m}}} end")
  (search-forward "{{{m}}}")
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on radio target
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_radio() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"<<<target>>> and <<target>>\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<<<target>>> and <<target>>")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on diary sexp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_diary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error #(\"‘C-c C-c’ can do nothing useful here\" 1 8 (font-lock-face help-key-binding face help-key-binding)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "%%(diary-anniversary 1 1 2000)")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on inline task
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "*************** TODO Inline\nBody\n*************** END")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on snippet
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf18_ctrlc_snippet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"@@html:\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text @@html:<b>bold</b>@@ end")
  (search-forward "@@html:")
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}
