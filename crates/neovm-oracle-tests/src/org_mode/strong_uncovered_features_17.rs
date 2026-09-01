//! Strong uncovered-features-17 oracle tests — complex state capture.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-cycle at various visibility states
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:overview \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\") (:content \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\") (:all \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody\n* H1b")
  (goto-char (point-min))
  (let ((r '()))
    (org-overview)
    (push (list :overview (buffer-substring-no-properties (point-min) (point-max))) r)
    (org-content)
    (push (list :content (buffer-substring-no-properties (point-min) (point-max))) r)
    (org-show-all)
    (push (list :all (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-cycle on single heading
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_cycle_single() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"H2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n** H2\nBody\n* H1b")
  (goto-char (point-min))
  (let ((r '()))
    (org-cycle 'children)
    (push (list :children (get-char-property (search-forward "H2") 'invisible)) r)
    (org-cycle 'subtree)
    (push (list :subtree (get-char-property (search-forward "H2") 'invisible)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-global-cycle
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_global() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:first \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\") (:second \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\") (:third \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody\n* H1b\n** H2b")
  (let ((r '()))
    (org-global-cycle nil)
    (push (list :first (buffer-substring-no-properties (point-min) (point-max))) r)
    (org-global-cycle nil)
    (push (list :second (buffer-substring-no-properties (point-min) (point-max))) r)
    (org-global-cycle nil)
    (push (list :third (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-set-startup-visibility
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_startup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK \"#+STARTUP: overview\\n* H1\\n** H2\\n*** H3\\nBody\\n* H1b\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+STARTUP: overview\n* H1\n** H2\n*** H3\nBody\n* H1b")
  (org-set-startup-visibility)
  (buffer-substring-no-properties (point-min) (point-max)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-show-context
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody\n* H1b")
  (goto-char (point-min))
  (search-forward "Body")
  (beginning-of-line)
  (org-show-context 'agenda)
  (buffer-substring-no-properties (point-min) (point-max)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-show-set-visibility
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_set_vis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody\n* H1b")
  (goto-char (point-min))
  (search-forward "H2")
  (beginning-of-line)
  (org-show-set-visibility 'canonical)
  (buffer-substring-no-properties (point-min) (point-max)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-flag-region
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:hidden org-hide-block) (:shown nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Line1\nLine2\nLine3\nLine4")
  (let ((r '()))
    (org-flag-region (point-min) (+ (point-min) 10) t 'org-hide-block)
    (push (list :hidden (get-char-property (point-min) 'invisible)) r)
    (org-flag-region (point-min) (+ (point-min) 10) nil 'org-hide-block)
    (push (list :shown (get-char-property (point-min) 'invisible)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-hide-block-toggle
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_block_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:hidden nil) (:shown nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC")
  (goto-char (point-min))
  (let ((r '()))
    (org-hide-block-toggle)
    (push (list :hidden (get-char-property (+ (point-min) 20) 'invisible)) r)
    (org-hide-block-toggle)
    (push (list :shown (get-char-property (+ (point-min) 20) 'invisible)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-hide-drawer-toggle
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_drawer_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"nil@1: Not at a drawer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:")
  (goto-char (point-min))
  (let ((r '()))
    (org-hide-drawer-toggle)
    (push (list :hidden (get-char-property (search-forward ":A:") 'invisible)) r)
    (org-hide-drawer-toggle)
    (push (list :shown (get-char-property (search-forward ":A:") 'invisible)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-heading on list
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_heading_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 \"item1\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- item1\n- item2\n- item3")
  (goto-char (point-min))
  (org-toggle-heading)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :level h)
                      (org-element-property :raw-value h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-heading on heading
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_heading_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (headline headline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n* H2\n* H3")
  (goto-char (point-min))
  (org-toggle-heading)
  (let ((s1 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
    (org-element-map (org-element-parse-buffer) '(headline plain-list item)
      (lambda (e) (org-element-type e)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-item on heading
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_item_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n* H2\n* H3")
  (goto-char (point-min))
  (org-toggle-item)
  (org-element-map (org-element-parse-buffer) '(headline plain-list item)
      (lambda (e) (org-element-type e))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-checkbox
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_checkbox() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:1 \"- [ ] a\") (:2 \"- [X] b\") (:3 \"- [X] c\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- [X] a\n- [ ] b\n- [-] c")
  (goto-char (point-min))
  (let ((r '()))
    (org-toggle-checkbox)
    (push (list :1 (buffer-substring-no-properties (line-beginning-position) (line-end-position))) r)
    (forward-line)
    (org-toggle-checkbox)
    (push (list :2 (buffer-substring-no-properties (line-beginning-position) (line-end-position))) r)
    (forward-line)
    (org-toggle-checkbox)
    (push (list :3 (buffer-substring-no-properties (line-beginning-position) (line-end-position))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-radio-button
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_radio() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error \"Cannot toggle checkbox outside of a list\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "( ) opt1\n( ) opt2\n( ) opt3")
  (goto-char (point-min))
  (org-toggle-radio-button)
  (buffer-substring-no-properties (point-min) (point-max)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-insert-todo-heading with arg
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_todo_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"H1\" \"TODO\") (\"H2\" \"DONE\") (\"New\" \"TODO\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO H1\n* DONE H2")
  (goto-char (point-max))
  (org-insert-todo-heading 1)
  (insert "New")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :raw-value h)
                      (org-element-property :todo-keyword h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-insert-todo-heading-respect-content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_todo_respect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"H1\" \"TODO\") (\"New\" \"TODO\") (\"H2\" \"DONE\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO H1\nBody\n* DONE H2")
  (goto-char (point-min))
  (end-of-line)
  (org-insert-todo-heading-respect-content)
  (insert "New")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :raw-value h)
                      (org-element-property :todo-keyword h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-insert-heading-respect-content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_heading_respect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"H1\" \"New\" \"H2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody\n* H2")
  (goto-char (point-min))
  (end-of-line)
  (org-insert-heading-respect-content)
  (insert "New")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (org-element-property :raw-value h))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-insert-property-drawer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_prop_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* H\\n:PROPERTIES:\\n:END:\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (goto-char (point-min))
  (org-insert-property-drawer)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-insert-drawer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_drawer_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* H\\n:MYDRAWER:\\n\\n:END:\\n\\nBody\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody")
  (goto-char (point-min))
  (end-of-line)
  (org-insert-drawer nil "MYDRAWER")
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-insert-comment
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_comment_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"# \\n* H\\nBody\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody")
  (goto-char (point-min))
  (end-of-line)
  (org-insert-comment)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-insert-src-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_src_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* H\\n#+begin_src \\n#+end_src\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (goto-char (point-max))
  (org-insert-structure-template "src")
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-fill-paragraph
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_fill() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"This is a very long paragraph that\\nshould be wrapped at some point because\\nit exceeds the fill column width\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq fill-column 40)
  (insert "This is a very long paragraph that should be wrapped at some point because it exceeds the fill column width")
  (goto-char (point-min))
  (org-fill-paragraph)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-fill-item
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_fill_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-fill-item)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq fill-column 40)
  (insert "- This is a very long list item that should be wrapped at some point because it exceeds the fill column width")
  (goto-char (point-min))
  (org-fill-item)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-fill-heading
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_fill_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-fill-heading)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq fill-column 40)
  (insert "* This is a very long heading that should be wrapped at some point")
  (goto-char (point-min))
  (org-fill-heading)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-auto-fill-function
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_auto_fill() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"This is a very long paragraph that\\nshould be wrapped at some point because\\nit exceeds the fill column width\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq fill-column 40)
  (insert "This is a very long paragraph that should be wrapped at some point because it exceeds the fill column width")
  (goto-char (point-min))
  (end-of-line)
  (org-auto-fill-function)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on src-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_ctrlc_src() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n#+RESULTS:")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ctrl-c-ctrl-c on checkbox
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf17_ctrlc_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"- [ ] a\\n- [ ] b\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- [X] a\n- [ ] b")
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
fn uf17_ctrlc_keyword() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r##""OK #(\"#+STARTUP: overview\\n* H\" 0 23 (fontified nil))""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+STARTUP: overview\n* H")
  (goto-char (point-min))
  (org-ctrl-c-ctrl-c)
  (buffer-string))"##,
        expect,
    );
}
