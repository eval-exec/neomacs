//! Strong uncovered-features-34 oracle tests — org-pcomplete, org-list, org-table.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-pcomplete
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf34_pcomplete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#+BEGIN_\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEG")
  (condition-case nil
      (pcomplete)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-pcomplete-initial
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf34_pcomplete_init() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#+\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+")
  (condition-case nil
      (org-pcomplete-initial)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-pcomplete-thing-at-point
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf34_pcomplete_thing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test")
  (goto-char (point-min))
  (condition-case nil
      (org-pcomplete-thing-at-point)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-struct
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf34_list_struct() {
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
fn uf34_list_prevs() {
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
fn uf34_list_parents() {
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
fn uf34_list_nth() {
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
fn uf34_list_item_end() {
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
fn uf34_list_item_begin() {
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
fn uf34_list_bullet() {
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
fn uf34_list_checkbox() {
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
fn uf34_list_depth() {
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
fn uf34_list_parent() {
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
fn uf34_list_children() {
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
fn uf34_list_siblings() {
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
fn uf34_list_top() {
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
fn uf34_list_bottom() {
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
// org-list-struct-apply
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf34_list_apply() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-list-struct-apply)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D")
  (let* ((struct (org-list-struct))
         (new-struct (copy-sequence struct)))
    (org-list-struct-apply new-struct)
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-send-item
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf34_list_send() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"- A\\n  - B\\n  - C\\n- D\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D")
  (goto-char (point-min))
  (let ((struct (org-list-struct)))
    (condition-case nil
        (org-list-send-item 'down struct)
      (error nil))
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-exchange-items
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf34_list_exchange() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"- A\\n- B\\n- C\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C")
  (let ((struct (org-list-struct)))
    (condition-case nil
        (org-list-exchange-items 1 2 struct)
      (error nil))
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-write-struct
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf34_list_write() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 3) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D")
  (let ((struct (org-list-struct)))
    (org-list-write-struct struct)
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-indent-item-generic
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf34_list_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"- A\\n  - B\\n- C\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C")
  (goto-char (point-min))
  (forward-line 1)
  (let ((struct (org-list-struct)))
    (org-list-indent-item-generic 1 t struct)
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-fix-item-bullet
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf34_list_fix_bullet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-list-fix-item-bullet)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  1. B\n  2. C\n- D")
  (let ((struct (org-list-struct)))
    (org-list-fix-item-bullet 2 struct)
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-fix-bullet
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf34_list_fix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-list-fix-bullet)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  1. B\n  2. C\n- D")
  (let ((struct (org-list-struct)))
    (org-list-fix-bullet struct)
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-list-struct-fix-box
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf34_list_fix_box() {
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
// org-list-struct-apply-struct
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf34_list_apply_struct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D")
  (let ((struct (org-list-struct)))
    (org-list-struct-apply-struct struct)
    (buffer-string)))"##,
        expect,
    );
}
