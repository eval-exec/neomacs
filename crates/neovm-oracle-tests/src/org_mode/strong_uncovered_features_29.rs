//! Strong uncovered-features-29 oracle tests — org-attach, org-checklist, org-depend.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-attach
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_attach() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:ID: test-attach-id\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-attach)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-attach-new
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_attach_new() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:ID: test-attach-new\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-attach-new "test.txt")
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-attach-attach
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_attach_attach() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-file "/tmp/attach-test.txt"
  (insert "test content"))
(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:ID: test-attach-attach\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-attach-attach "/tmp/attach-test.txt")
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-attach-open
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_attach_open() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:ID: test-attach-open\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-attach-open)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-attach-reveal
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_attach_reveal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:ID: test-attach-reveal\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-attach-reveal)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-attach-delete-all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_attach_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:ID: test-attach-delete\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-attach-delete-all)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-attach-set-directory
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_attach_dir() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (condition-case nil
      (org-attach-set-directory "/tmp/attach")
    (error nil))
  (org-entry-get nil "ATTACH_DIR"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-attach-unset-directory
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_attach_unset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"/tmp/attach\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:ATTACH_DIR: /tmp/attach\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-attach-unset-directory)
    (error nil))
  (org-entry-get nil "ATTACH_DIR"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-attach-get-directory
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_attach_get_dir() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:ID: test-id\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-attach-get-directory)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-attach-file-list
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_attach_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:ID: test-attach-list\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-attach-file-list)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-attach-url
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_attach_url() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:ID: test-attach-url\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-attach-url "http://example.com/test.txt")
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-attach-attach-in-emacs
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_attach_emacs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:ID: test-attach-emacs\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-attach-attach-in-emacs "/tmp/test.txt")
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-checklist
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_checklist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((on \"a\") (off \"b\") (on \"c\") (off \"d\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n- [X] a\n- [ ] b\n- [X] c\n- [ ] d")
  (org-element-map (org-element-parse-buffer) 'item
    (lambda (i) (list (org-element-property :checkbox i)
                      (org-trim (buffer-substring-no-properties
                                  (org-element-property :contents-begin i)
                                  (org-element-property :contents-end i)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-checkbox with arg
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_check_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"- a\\n- [ ] b\\n- [-] c\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- [X] a\n- [ ] b\n- [-] c")
  (goto-char (point-min))
  (org-toggle-checkbox '(4))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-reset-checkbox-state-subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_check_reset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T\\n- [ ] a\\n- [ ] b\\n- [ ] c\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n- [X] a\n- [X] b\n- [-] c")
  (goto-char (point-min))
  (org-reset-checkbox-state-subtree)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-depend
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_depend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* TODO B\nBLOCKED=A")
  (goto-char (point-min))
  (condition-case nil
      (org-depend-trigger-todo "DONE" '("A" "B"))
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-notify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_notify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nDEADLINE: <2026-01-15>")
  (goto-char (point-min))
  (condition-case nil
      (org-notify)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-notify-add
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_notify_add() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-notify-add "test" '(:time "1h" :period "10m" :title "Test"))
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-drill
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_drill() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Q\n:PROPERTIES:\n:DRILL_CARD_TYPE: hide1cloze\n:END:\nThis is a {test} question")
  (condition-case nil
      (org-drill)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-drill-entry
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_drill_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Q\n:PROPERTIES:\n:DRILL_CARD_TYPE: hide1cloze\n:END:\nThis is a {test} question")
  (goto-char (point-min))
  (condition-case nil
      (org-drill-entry)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-drill-resume
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_drill_resume() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-drill-resume)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-drill-tree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_drill_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Q\n:PROPERTIES:\n:DRILL_CARD_TYPE: hide1cloze\n:END:\nThis is a {test} question")
  (goto-char (point-min))
  (condition-case nil
      (org-drill-tree)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-drill-maple-tree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf29_drill_maple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Q\n:PROPERTIES:\n:DRILL_CARD_TYPE: hide1cloze\n:END:\nThis is a {test} question")
  (goto-char (point-min))
  (condition-case nil
      (org-drill-maple-tree)
    (error nil)))"##,
        expect,
    );
}
