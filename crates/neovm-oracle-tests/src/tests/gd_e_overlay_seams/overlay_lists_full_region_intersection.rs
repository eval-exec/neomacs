use super::*;

#[test]
fn oracle_prop_gde_review_overlay_lists_full_region_intersection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list
(with-temp-buffer
(insert "ab")
(dolist (spec '((3 3 end-empty) (1 1 begin-empty) (2 3 nonempty)))
(overlay-put (make-overlay (car spec) (cadr spec)) 'tag (nth 2 spec)))
(list (mapcar (lambda (o) (overlay-get o 'tag)) (car (overlay-lists)))
(save-restriction (narrow-to-region 2 2)
(mapcar (lambda (o) (overlay-get o 'tag)) (car (overlay-lists))))))
(with-temp-buffer (overlay-put (make-overlay 1 1) 'tag 'empty-buffer)
(mapcar (lambda (o) (overlay-get o 'tag)) (car (overlay-lists))))
(with-temp-buffer (insert "abcdef")
(overlay-put (make-overlay 4 7) 'tag 'collapsed-end)
(overlay-put (make-overlay 2 3) 'tag 'prefix)
(delete-region 4 7)
(mapcar (lambda (o) (overlay-get o 'tag)) (car (overlay-lists)))))"#;
    let expected = expect_test::expect![[
        r#""OK (((begin-empty nonempty) (begin-empty nonempty)) (empty-buffer) (prefix))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expected);
}
