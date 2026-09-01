//! Strong uncovered-features-51 oracle tests — org-list deep, org-item, org-checkbox.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-list-struct
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_list_struct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 0 \"- \" nil nil nil 17) (5 2 \"- \" nil nil nil 11) (11 2 \"- \" nil nil nil 17) (17 0 \"- \" nil nil nil 20))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D")
  (goto-char (point-min))
  (org-list-struct))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-prevs-alist
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_list_prevs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1) (5) (11 . 5) (17 . 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D")
  (let ((struct (org-list-struct)))
    (org-list-prevs-alist struct)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-parents-alist
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_list_parents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1) (5 . 1) (11 . 1) (17))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D")
  (let ((struct (org-list-struct)))
    (org-list-parents-alist struct)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-get-nth
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_list_nth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (3 . 3) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D")
  (let ((struct (org-list-struct)))
    (list (org-list-get-nth 0 struct)
          (org-list-get-nth 1 struct))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-get-item-end
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_list_item_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (17 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D")
  (let ((struct (org-list-struct)))
    (list (org-list-get-item-end 1 struct)
          (org-list-get-item-end 2 struct))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-get-item-begin
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_list_item_begin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 0) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D")
  (let ((struct (org-list-struct)))
    (list (org-list-get-item-begin 1 struct)
          (org-list-get-item-begin 2 struct))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-get-bullet
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_list_bullet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"- \" nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  1. B\n  2. C\n+ D")
  (let ((struct (org-list-struct)))
    (list (org-list-get-bullet 1 struct)
          (org-list-get-bullet 2 struct)
          (org-list-get-bullet 3 struct))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-get-checkbox
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_list_checkbox() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"[X]\" nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- [X] A\n- [ ] B\n- [-] C")
  (let ((struct (org-list-struct)))
    (list (org-list-get-checkbox 1 struct)
          (org-list-get-checkbox 2 struct)
          (org-list-get-checkbox 3 struct))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-get-depth
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_list_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-list-get-depth)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n    - C\n- D")
  (let ((struct (org-list-struct)))
    (list (org-list-get-depth 1 struct)
          (org-list-get-depth 2 struct)
          (org-list-get-depth 3 struct))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-get-parent
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_list_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (3 . 3) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D")
  (let ((struct (org-list-struct)))
    (list (org-list-get-parent 1 struct)
          (org-list-get-parent 2 struct)
          (org-list-get-parent 3 struct))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-get-children
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_list_children() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (3 . 3) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D")
  (let ((struct (org-list-struct)))
    (list (org-list-get-children 1 struct)
          (org-list-get-children 2 struct))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-get-siblings
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_list_siblings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-list-get-siblings)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D")
  (let ((struct (org-list-struct)))
    (list (org-list-get-siblings 1 struct)
          (org-list-get-siblings 2 struct))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-get-top-point
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_list_top() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Before\n- A\n  - B\n  - C\n- D\nAfter")
  (org-list-get-top-point))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-get-bottom-point
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_list_bottom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Before\n- A\n  - B\n  - C\n- D\nAfter")
  (org-list-get-bottom-point))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-item-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_at_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:before nil) (:item t) (:cont nil) (:after nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Before\n- item\n  continued\nAfter")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :before (org-at-item-p)) r)
    (forward-line)
    (push (list :item (org-at-item-p)) r)
    (forward-line)
    (push (list :cont (org-at-item-p)) r)
    (forward-line)
    (push (list :after (org-at-item-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-item-checkbox-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_at_checkbox() {
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
// org-toggle-checkbox
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_toggle_checkbox() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"- [ ] a\\n- [ ] b\\n- [-] c\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- [X] a\n- [ ] b\n- [-] c")
  (goto-char (point-min))
  (org-toggle-checkbox)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-checkbox with universal arg
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_toggle_checkbox_univ() {
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
fn uf51_reset_checkbox() {
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
// org-update-checkbox-count
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_update_count() {
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
fn uf51_update_parent() {
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
// org-list-struct-fix-box
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_fix_box() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (3 . 4) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- [X] A\n- [ ] B\n- [ ] C")
  (let ((struct (org-list-struct)))
    (org-list-struct-fix-box struct (org-list-parents-alist struct))
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-set-checkbox
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf51_set_checkbox() {
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
fn uf51_list_toggle() {
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
