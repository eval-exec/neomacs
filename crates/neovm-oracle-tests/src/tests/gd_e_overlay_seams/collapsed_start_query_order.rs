use super::*;

#[test]
fn oracle_prop_gde_overlay_collapsed_start_query_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let (out)
(dolist (text (list (make-string 60 ?a) (make-string 60 ?ж)
(apply #'unibyte-string (make-list 60 255))
(string-as-multibyte (apply #'unibyte-string (make-list 60 255)))))
(with-temp-buffer (set-buffer-multibyte (multibyte-string-p text)) (insert text)
(let ((a (make-overlay 10 30)) (b (make-overlay 20 30)) (c (make-overlay 5 30)))
(dolist (item (list (cons a 'a) (cons b 'b) (cons c 'c)))
(overlay-put (car item) 'tag (cdr item)))
(delete-region 5 25)
(push (list (mapcar (lambda (o) (overlay-get o 'tag)) (overlays-at 5))
(mapcar (lambda (o) (overlay-get o 'tag)) (overlays-in 5 6))
(mapcar (lambda (o) (overlay-get o 'tag)) (car (overlay-lists)))) out))))
(nreverse out))"#;
    let expected = expect_test::expect![[
        r#""OK (((c a b) (c a b) (c a b)) ((c a b) (c a b) (c a b)) ((c a b) (c a b) (c a b)) ((c a b) (c a b) (c a b)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expected);
}
