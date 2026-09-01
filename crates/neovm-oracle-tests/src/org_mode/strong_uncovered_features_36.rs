//! Strong uncovered-features-36 oracle tests — org-indent, org-lint, org-ctags.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-indent-mode
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody\n** H2\nSub\n*** H3\nDeep")
  (org-indent-mode 1)
  (let ((r '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((indent (get-char-property (point) 'line-prefix)))
        (when indent (push (list (line-number-at-pos) indent) r)))
      (forward-line))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-indent-indent-buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_indent_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-indent-indent-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody\n** H2\nSub\n*** H3\nDeep")
  (org-indent-indent-buffer)
  (let ((r '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((indent (get-char-property (point) 'line-prefix)))
        (when indent (push (list (line-number-at-pos) indent) r)))
      (forward-line))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-indent-indent-region
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_indent_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-indent-indent-region)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody\n** H2\nSub\n*** H3\nDeep")
  (org-indent-indent-region (point-min) (point-max))
  (let ((r '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((indent (get-char-property (point) 'line-prefix)))
        (when indent (push (list (line-number-at-pos) indent) r)))
      (forward-line))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-indent-add-properties
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_indent_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-indent-add-properties)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody\n** H2\nSub")
  (org-indent-add-properties (point-min) (point-max))
  (let ((r '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((indent (get-char-property (point) 'line-prefix)))
        (when indent (push (list (line-number-at-pos) indent) r)))
      (forward-line))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-indent-remove-properties
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_indent_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-indent-add-properties)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody\n** H2\nSub")
  (org-indent-add-properties (point-min) (point-max))
  (org-indent-remove-properties (point-min) (point-max))
  (let ((r '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((indent (get-char-property (point) 'line-prefix)))
        (when indent (push (list (line-number-at-pos) indent) r)))
      (forward-line))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-indent-refresh-maybe
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_indent_refresh() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-indent-refresh-maybe)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody\n** H2\nSub")
  (org-indent-refresh-maybe (point-min) (point-max) nil)
  (let ((r '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((indent (get-char-property (point) 'line-prefix)))
        (when indent (push (list (line-number-at-pos) indent) r)))
      (forward-line))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-lint
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_lint() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nSCHEDULED: <invalid>\nBody [[broken]]")
  (length (org-lint)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-lint-report
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_lint_report() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nSCHEDULED: <invalid>\nBody [[broken]]")
  (condition-case nil
      (org-lint-report)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-lint-add-checker
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_lint_add() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-lint-add-checker 'test-checker
      :description "Test checker"
      :verify (lambda () nil))
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctags
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_ctags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-ctags)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctags-create-tags
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_ctags_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-ctags-create-tags)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctags-find-tag
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_ctags_find() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-ctags-find-tag "test-tag")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctags-generate-tags
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_ctags_gen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-ctags-generate-tags)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctags-update-tags
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_ctags_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-ctags-update-tags)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctags-visit-tags-table
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_ctags_visit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-ctags-visit-tags-table)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-checklist (org-checklist-create)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_checklist_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n- [X] a\n- [ ] b\n- [X] c")
  (goto-char (point-min))
  (let ((done (org-element-map (org-element-parse-buffer) 'item
                (lambda (i) (eq (org-element-property :checkbox i) 'on)))))
    (list (length done))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-cache status after modifications
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_cache_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-cache-status)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody")
  (let ((s1 (org-element-cache-status)))
    (insert "\nNew line")
    (let ((s2 (org-element-cache-status)))
      (list (plist-get s1 :size) (plist-get s2 :size)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-cache after heading level change
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_cache_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:after (2 2)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2")
  (goto-char (point-min))
  (org-metaright)
  (let ((r '()))
    (push (list :after (org-element-map (org-element-parse-buffer) 'headline
                          (lambda (h) (org-element-property :level h)))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-cache after todo change
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_cache_todo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:after \"TODO\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (goto-char (point-min))
  (org-todo)
  (let ((r '()))
    (push (list :after (org-element-property :todo-keyword (org-element-at-point))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-cache after tag change
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_cache_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:after (\"tag1\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (goto-char (point-min))
  (org-set-tags '("tag1"))
  (let ((r '()))
    (push (list :after (org-get-tags)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-cache after property change
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_cache_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:after \"1\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (goto-char (point-min))
  (org-set-property "A" "1")
  (let ((r '()))
    (push (list :after (org-entry-get nil "A")) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-cache after planning change
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf36_cache_plan() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:after ((timestamp (:standard-properties [16 nil nil nil 32 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2026-01-15 Thu>\" :year-start 2026 :month-start 1 :day-start 15 :hour-start nil :minute-start nil :year-end 2026 :month-end 1 :day-end 15 :hour-end nil :minute-end nil)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (goto-char (point-min))
  (org-schedule nil "<2026-01-15>")
  (let ((r '()))
    (push (list :after (org-element-map (org-element-parse-buffer) 'planning
                          (lambda (p) (org-element-property :scheduled p)))) r)
    (nreverse r)))"##,
        expect,
    );
}
