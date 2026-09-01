//! Strict combo oracle probes, batch 238: dom.el DOM manipulation over a
//! parsed-XML-style tree. dom-tag/attributes/children/parent, dom-by-tag/
//! dom-by-class/dom-by-attribute, dom-strings text extraction, and
//! dom-remove-node.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_dom_tag_attr_children_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'dom)
(let ((node '(html nil (body nil (p ((class . "greeting")) "hello") (a ((href . "u")) "link")))))
  (list (dom-tag node)
        (dom-attributes node)
        (dom-attr node 'missing)
        (length (dom-children node))
        (dom-strings node)
        (dom-text node)))
"##;
    let expect = expect_test::expect![[r#""OK (html nil nil 1 (\"hello\" \"link\") \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_dom_by_tag_class_attribute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'dom)
(let ((node '(root nil (div ((class . "a")) "one") (span ((id . "s")) "two") (div ((class . "b")) "three"))))
  (list (length (dom-by-tag node 'div))
        (dom-by-tag node 'span)
        (dom-by-class node "a")
        (dom-by-attribute node 'id)
        (mapcar #'dom-tag (dom-children node))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function dom-by-attribute)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_dom_parent_remove_child_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'dom)
(let* ((child '(p nil "text"))
       (node `(root nil ,child (span nil "other"))))
  (list (eq (dom-parent node child) node)
        (dom-previous-sibling node child)
        (dom-next-sibling node child)
        (length (dom-children node))
        (progn (dom-remove-node node child) (length (dom-children node)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function dom-next-sibling)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
