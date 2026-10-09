use super::*;

#[test]
fn oracle_prop_gde_overlay_nontransitive_sorted_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer (insert "abcdefghijkl")
(let ((a (make-overlay 1 10)) (b (make-overlay 2 8)) (c (make-overlay 3 11)))
(dolist (item (list (list a 'a '(0 . 2)) (list b 'b '(0 . 0)) (list c 'c '(0 . 1))))
(overlay-put (car item) 'tag (cadr item)) (overlay-put (car item) 'priority (nth 2 item)))
(list (mapcar (lambda (o) (overlay-get o 'tag)) (overlays-at 5 t))
(mapcar (lambda (o) (overlay-get o 'tag)) (overlays-at 5)))))"#;
    let expected = expect_test::expect![[r#""OK ((c b a) (a b c))""#]];
    crate::common::assert_oracle_parity_expect(form, expected);
}
