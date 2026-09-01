//! Oracle parity tests for GNU `subr.el' `make-composed-keymap'.

use crate::common::assert_oracle_parity;

#[test]
fn oracle_make_composed_keymap_lookup_order_and_nil_bindings() {
    let form = r#"
(let ((map1 (make-sparse-keymap))
      (map2 (make-sparse-keymap))
      (parent (make-sparse-keymap)))
  (define-key map1 [?a] nil)
  (define-key map1 [?b] 'map1-b)
  (define-key map2 [?a] 'map2-a)
  (define-key map2 [?b] 'map2-b)
  (define-key map2 [?c] nil)
  (define-key parent [?a] 'parent-a)
  (define-key parent [?c] 'parent-c)
  (let ((composed (make-composed-keymap (list map1 map2) parent))
        (parent-only (make-composed-keymap (list map1) parent))
        (empty-maps (make-composed-keymap nil parent)))
    (list
     ;; MAPS are searched in order.  A nil binding in one MAPS entry does not
     ;; hide later MAPS entries, but it does hide PARENT.
     (lookup-key composed [?a])
     (lookup-key composed [?b])
     (lookup-key composed [?c])
     (lookup-key parent-only [?a])
     (lookup-key empty-maps [?a])
     (lookup-key empty-maps [?c])
     (keymapp composed))))"#;
    let expect = expect_test::expect![[r#""OK (map2-a map1-b nil nil parent-a parent-c t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_make_composed_keymap_single_map_and_parent_shape() {
    let form = r#"
(let ((map (make-sparse-keymap))
      (parent (make-sparse-keymap)))
  (define-key map [?x] 'map-x)
  (define-key parent [?y] 'parent-y)
  (let ((composed (make-composed-keymap map parent)))
    (list
     (lookup-key composed [?x])
     (lookup-key composed [?y])
     ;; GNU `subr.el' splices both the map and parent into the returned keymap.
     (car composed)
     (keymapp (cadr composed))
     (eq (caddr composed) 'keymap)
     (equal (cdddr composed) (cdr parent))))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 14 46)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
