//! Strong uncovered-features-40 oracle tests — org-attach, org-checklist, org-crypt.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-attach
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_attach() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:ID: test-id\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-attach)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-attach-set-directory
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_attach_dir() {
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
// org-crypt
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_crypt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:CRYPTKEY: test-key\n:END:\nSecret text")
  (goto-char (point-min))
  (condition-case nil
      (org-crypt)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-encrypt-entry
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_encrypt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"* T\\n:PROPERTIES:\\n:CRYPTKEY: test-key\\n:END:\\nSecret text\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:CRYPTKEY: test-key\n:END:\nSecret text")
  (goto-char (point-min))
  (condition-case nil
      (org-encrypt-entry)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-decrypt-entry
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_decrypt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"* T\\n:PROPERTIES:\\n:CRYPTKEY: test-key\\n:END:\\nSecret text\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:CRYPTKEY: test-key\n:END:\nSecret text")
  (goto-char (point-min))
  (condition-case nil
      (progn (org-encrypt-entry) (org-decrypt-entry))
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-encrypt-entries
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_encrypt_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"* T1\\n:PROPERTIES:\\n:CRYPTKEY: k1\\n:END:\\nSecret1\\n* T2\\n:PROPERTIES:\\n:CRYPTKEY: k2\\n:END:\\nSecret2\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1\n:PROPERTIES:\n:CRYPTKEY: k1\n:END:\nSecret1\n* T2\n:PROPERTIES:\n:CRYPTKEY: k2\n:END:\nSecret2")
  (condition-case nil
      (org-encrypt-entries)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-decrypt-entries
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_decrypt_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"* T1\\n:PROPERTIES:\\n:CRYPTKEY: k1\\n:END:\\nSecret1\\n* T2\\n:PROPERTIES:\\n:CRYPTKEY: k2\\n:END:\\nSecret2\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1\n:PROPERTIES:\n:CRYPTKEY: k1\n:END:\nSecret1\n* T2\n:PROPERTIES:\n:CRYPTKEY: k2\n:END:\nSecret2")
  (condition-case nil
      (progn (org-encrypt-entries) (org-decrypt-entries))
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-checklist (reset-checkbox-state-subtree)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_checklist_reset() {
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
// org-toggle-checkbox with universal arg
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_check_toggle_univ() {
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
// org-update-checkbox-count
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_check_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T [1/2]\\n- [X] a\\n- [ ] b\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T [1/2]\n- [X] a\n- [ ] b")
  (goto-char (point-min))
  (org-update-checkbox-count)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-update-parent-checkboxes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_check_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-update-parent-checkboxes)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n- [X] a\n- [X] b\n- [ ] c")
  (goto-char (point-min))
  (search-forward "[ ] c")
  (org-update-parent-checkboxes)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-item-checkbox-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_at_checkbox() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:1 t) (:2 t) (:3 nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- [X] a\n- [ ] b\n- no box")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :1 (org-at-item-checkbox-p)) r)
    (forward-line)
    (push (list :2 (org-at-item-checkbox-p)) r)
    (forward-line)
    (push (list :3 (org-at-item-checkbox-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-at-regexp-after-bullet-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_list_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:1 t) (:2 t) (:3 nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- TODO task\n- DONE done\n- no match")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :1 (org-list-at-regexp-after-bullet-p "TODO")) r)
    (forward-line)
    (push (list :2 (org-list-at-regexp-after-bullet-p "DONE")) r)
    (forward-line)
    (push (list :3 (org-list-at-regexp-after-bullet-p "TODO")) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-set-checkbox
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_list_set_box() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-list-struct-apply)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- [X] a\n- [ ] b")
  (goto-char (point-min))
  (let ((struct (org-list-struct)))
    (org-list-set-checkbox 1 struct "[ ]")
    (org-list-struct-apply struct)
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-toggle-checkbox
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_list_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-list-toggle-checkbox)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- [X] a\n- [ ] b")
  (goto-char (point-min))
  (let ((struct (org-list-struct)))
    (org-list-toggle-checkbox nil struct)
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-checkbox-overlay
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_list_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- [X] a\n- [ ] b")
  (goto-char (point-min))
  (condition-case nil
      (org-list-checkbox-overlay)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-struct-fix-bullet
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_list_fix_bullet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-list-struct-fix-bullet)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  1. B\n  2. C\n- D")
  (let ((struct (org-list-struct)))
    (org-list-struct-fix-bullet struct)
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-struct-fix-ind
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_list_fix_ind() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 3) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n    - C\n- D")
  (let ((struct (org-list-struct)))
    (org-list-struct-fix-ind struct)
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-struct-fix-struct
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_list_fix_struct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-list-struct-fix-struct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D")
  (let ((struct (org-list-struct)))
    (org-list-struct-fix-struct struct)
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-struct-indent-item
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_list_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-list-struct-indent-item)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C")
  (goto-char (point-min))
  (forward-line 1)
  (let ((struct (org-list-struct)))
    (org-list-struct-indent-item 1 struct)
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-struct-outdent-item
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_list_outdent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-list-struct-outdent-item)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C")
  (goto-char (point-min))
  (forward-line 1)
  (let ((struct (org-list-struct)))
    (org-list-struct-outdent-item 1 struct)
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-struct-move-item
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_list_move() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-list-struct-move-item)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C")
  (goto-char (point-min))
  (forward-line 1)
  (let ((struct (org-list-struct)))
    (org-list-struct-move-item 1 struct)
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-struct-move-item-down
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_list_move_down() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-list-struct-move-item-down)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C")
  (goto-char (point-min))
  (let ((struct (org-list-struct)))
    (org-list-struct-move-item-down 1 struct)
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-struct-move-item-up
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf40_list_move_up() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-list-struct-move-item-up)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C")
  (goto-char (point-min))
  (forward-line 2)
  (let ((struct (org-list-struct)))
    (org-list-struct-move-item-up 3 struct)
    (buffer-string)))"##,
        expect,
    );
}
