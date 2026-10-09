use super::*;

#[test]
fn oracle_prop_gde_overlay_multibyte_unibyte_raw_byte_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let (out)
  (dolist (text (list "aж😀中z" (unibyte-string 65 128 255 66)
                      (concat "a" (string-as-multibyte (unibyte-string 128 255)) "ж😀z")))
    (with-temp-buffer
      (set-buffer-multibyte (multibyte-string-p text))
      (insert text)
      (let ((a (make-overlay 2 4)) (b (make-overlay 3 5)))
        (overlay-put a 'probe 'a)
        (overlay-put a 'priority '(0 . 2))
        (overlay-put b 'probe 'b)
        (overlay-put b 'priority '(0 . 1))
        (push (list (multibyte-string-p text) (string-to-list text)
                    (mapcar (lambda (pos) (get-char-property pos 'probe))
                            (list 1 2 3 4 (point-max)))
                    (get-char-property (copy-marker 3) 'probe)
                    (save-restriction (narrow-to-region 2 4)
                      (list (get-char-property 2 'probe) (get-char-property 3 'probe)
                            (get-char-property 4 'probe)))) out))))
  (nreverse out))"#;
    let expected = expect_test::expect![[
        r#""OK ((t (97 1078 128512 20013 122) (nil a a b nil) a (a a b)) (nil (65 128 255 66) (nil a a b nil) a (a a b)) (t (97 4194176 4194303 1078 128512 122) (nil a a b nil) a (a a b)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expected);
}
