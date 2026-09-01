//! Complex combo batch 104 — treesit / treesitter API availability,
//! language grammar availability, parsing basic buffers with embedded
//! languages.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx104_treesit_available_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (treesit-available-p)
          (fboundp 'treesit-parser-create)
          (fboundp 'treesit-node-root)
          (fboundp 'treesit-query-capture))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx104_treesit_language_availability_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((python nil) (c nil) (c++ nil) (javascript nil) (typescript nil) (rust nil) (go nil) (bash nil) (html nil) (css nil) (json nil) (yaml nil) (toml nil) (dockerfile nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (mapcar (lambda (lang)
              (list lang (treesit-language-available-p lang)))
            '(python c c++ javascript typescript rust go bash
              html css json yaml toml dockerfile))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx104_treesit_parser_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "def hello():\n    return 42\n")
      (when (treesit-language-available-p 'python)
        (let ((parser (treesit-parser-create 'python)))
          (treesit-parser-buffer parser (current-buffer))
          (let ((root (treesit-buffer-root-node 'python)))
            (list (treesit-node-p root)
                  (treesit-node-type root))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx104_treesit_node_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "print('hello')\n")
      (when (treesit-language-available-p 'python)
        (let ((parser (treesit-parser-create 'python)))
          (goto-char 3)
          (let ((node (treesit-node-at (point))))
            (list (treesit-node-p node)
                  (treesit-node-type node))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx104_treesit_query_language_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'treesit-query-compile)
          (fboundp 'treesit-query-capture)
          (fboundp 'treesit-query-string)
          (fboundp 'treesit-induce-sparse-tree)
          (boundp 'treesit-font-lock-defaults))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx104_treesit_thing_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "function foo() { return 42; }\n")
      (when (treesit-language-available-p 'javascript)
        (let ((parser (treesit-parser-create 'javascript)))
          (goto-char 10)
          (let ((bounds (treesit-thing-at-point-1 'sexp)))
            (list (consp bounds))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx104_treesit_search_forward_gop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'treesit-search-subtree)
          (fboundp 'treesit-search-forward)
          (fboundp 'treesit-search-forward-goto)
          (fboundp 'treesit-parent-until)
          (fboundp 'treesit-node-beginning)
          (fboundp 'treesit-node-end))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx104_treesit_explore_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'treesit)
      (list (fboundp 'treesit-explore-mode)
            (fboundp 'treesit-inspect-node)
            (boundp 'treesit-font-lock-feature-list)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx104_treesit_parser_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'treesit-parser-list)
          (fboundp 'treesit-parser-p)
          (fboundp 'treesit-parser-buffer)
          (fboundp 'treesit-parser-language)
          (fboundp 'treesit-parser-delete))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx104_treesit_node_field_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'treesit-node-field-name)
          (fboundp 'treesit-node-field-name-for-child)
          (fboundp 'treesit-node-child-by-field-name)
          (fboundp 'treesit-node-named-child)
          (fboundp 'treesit-node-child-count))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx104_treesit_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ts-avail (treesit-available-p)))
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Treesit mega test buffer content")
        (put-text-property 1 8 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list ts-avail
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx104_treesit_node_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'treesit-node-p)
          (fboundp 'treesit-node-eq)
          (fboundp 'treesit-node-null)
          (fboundp 'treesit-node-extra-p)
          (fboundp 'treesit-node-named-p)
          (fboundp 'treesit-node-leaf-p))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx104_treesit_install_language() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'treesit-install-language-grammar)
          (fboundp 'treesit-language-grammar-set)
          (boundp 'treesit-language-source-alist))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
