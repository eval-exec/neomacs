use super::*;

#[test]
fn oracle_prop_gde_overlay_nil_priority_identity_invariants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let (out)
(dolist (text (list (make-string 60 ?a) (make-string 60 ?ж)
(apply #'unibyte-string (make-list 60 255))
(string-as-multibyte (apply #'unibyte-string (make-list 60 255)))))
(with-temp-buffer (set-buffer-multibyte (multibyte-string-p text)) (insert text)
(dotimes (i 150) (let ((o (make-overlay 2 30))) (overlay-put o 'tag i)))
(let ((a (overlays-at 3 t)) (b (overlays-at 3 t))
(winner (cdr (get-char-property-and-overlay 3 'tag))))
(push (list (length a) (equal a b) (eq winner (car a))
(equal (sort (mapcar (lambda (o) (overlay-get o 'tag)) a) #'<)
(number-sequence 0 149))) out))))
(nreverse out))"#;
    let expected =
        expect_test::expect![[r#""OK ((150 t t t) (150 t t t) (150 t t t) (150 t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expected);
}
