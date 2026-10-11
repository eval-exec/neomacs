use super::*;

#[test]
fn oracle_prop_gde_overlay_compiled_property_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((f (byte-compile
(lambda (pos) (list (get-char-property pos 'probe)
                   (car (get-char-property-and-overlay pos 'probe)))))))
(with-temp-buffer (insert "abcdefghijkl")
(let ((a (make-overlay 1 10)) (b (make-overlay 2 8)) (c (make-overlay 3 11)))
(dolist (item (list (list a 'a '(0 . 2)) (list b 'b '(0 . 0)) (list c 'c '(0 . 1))))
(overlay-put (car item) 'probe (cadr item))
(overlay-put (car item) 'priority (nth 2 item)))
(dotimes (_ 40) (funcall f 5))
(list (funcall f 5)
(progn (overlay-put b 'probe nil) (funcall f 5))
(let ((default-text-properties '(probe fallback))) (funcall f (point-max)))))))"#;
    let expected = expect_test::expect![[r#""OK ((c c) (a a) (fallback fallback))""#]];
    crate::common::assert_oracle_parity_expect(form, expected);
}
