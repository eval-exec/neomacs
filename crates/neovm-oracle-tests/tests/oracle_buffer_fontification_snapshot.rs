use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// GNU editfns.c:make_buffer_string_both copies bytes before fontification,
// then copies the properties produced by buffer-access-fontify-functions.
#[test]
fn divergence_buffer_string_copies_bytes_before_fontification() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
      (insert "abc")
      (let ((calls nil)
            (buffer-access-fontified-property nil))
        (let ((buffer-access-fontify-functions
               (list (lambda (beg end)
                       (setq calls (cons (list beg end) calls))
                       (let ((buffer-access-fontify-functions nil))
                         (subst-char-in-region beg end ?a ?z)
                         (put-text-property beg end 'oracle-font 'done))))))
          (let ((snapshot (buffer-string)))
            (list (substring-no-properties snapshot)
                  (get-text-property 0 'oracle-font snapshot)
                  (let ((buffer-access-fontify-functions nil))
                    (buffer-substring-no-properties (point-min) (point-max)))
                  (nreverse calls))))))"#;
    let expect = expect_test::expect![[""]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// GNU editfns.c:make_buffer_string_both copies bytes before fontification,
// then copies the properties produced by buffer-access-fontify-functions.
#[test]
fn divergence_buffer_substring_copies_bytes_before_fontification() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
      (insert "abc")
      (let ((calls nil)
            (buffer-access-fontified-property nil))
        (let ((buffer-access-fontify-functions
               (list (lambda (beg end)
                       (setq calls (cons (list beg end) calls))
                       (let ((buffer-access-fontify-functions nil))
                         (subst-char-in-region beg end ?a ?z)
                         (put-text-property beg end 'oracle-font 'done))))))
          (let ((snapshot (buffer-substring (point-min) (point-max))))
            (list (substring-no-properties snapshot)
                  (get-text-property 0 'oracle-font snapshot)
                  (let ((buffer-access-fontify-functions nil))
                    (buffer-substring-no-properties (point-min) (point-max)))
                  (nreverse calls))))))"#;
    let expect = expect_test::expect![[""]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
