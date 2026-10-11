use super::*;

#[test]
fn oracle_prop_gde_overlay_sorted_category_and_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(save-window-excursion
(with-temp-buffer (insert "abcdef")
(let ((w1 (selected-window)) (w2 (split-window))
(a (make-overlay 1 6)) (b (make-overlay 1 6)) (c (make-overlay 1 6)))
(set-window-buffer w1 (current-buffer)) (set-window-buffer w2 (current-buffer))
(unwind-protect
(progn (put 'gde-sorted-category 'priority 5) (put 'gde-sorted-category 'window w1)
(overlay-put a 'category 'gde-sorted-category) (overlay-put a 'tag 'category)
(overlay-put b 'priority 9) (overlay-put b 'window w2) (overlay-put b 'tag 'window2)
(overlay-put c 'priority 1) (overlay-put c 'tag 'plain)
(mapcar (lambda (sorted) (mapcar (lambda (o) (overlay-get o 'tag)) (overlays-at 3 sorted)))
(list t w1 w2)))
(setplist 'gde-sorted-category nil)))))"#;
    let expected = expect_test::expect![[
        r#""OK ((window2 category plain) (category plain) (window2 plain))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expected);
}
