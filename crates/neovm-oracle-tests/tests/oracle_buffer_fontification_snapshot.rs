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
    let expect = expect_test::expect![[r#""OK (\"abc\" done \"zbc\" ((1 4)))""#]];
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
    let expect = expect_test::expect![[r#""OK (\"abc\" done \"zbc\" ((1 4)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn divergence_buffer_string_properties_follow_post_hook_current_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#" (let ((source (generate-new-buffer "oracleb-source"))
       (target (generate-new-buffer "oracleb-target")))
   (unwind-protect
       (progn
         (with-current-buffer target
           (insert "xyz")
           (put-text-property 1 4 'oracle-font 'target))
         (with-current-buffer source
           (insert "abc")
           (let ((calls nil)
                 (buffer-access-fontified-property nil))
             (let ((buffer-access-fontify-functions
                    (list (lambda (beg end)
                            (setq calls (cons (list beg end) calls))
                            (set-buffer target)))))
               (let ((snapshot (buffer-string)))
                 (list (substring-no-properties snapshot)
                       (get-text-property 0 'oracle-font snapshot)
                       (eq (current-buffer) target)
                       (nreverse calls)))))))
     (kill-buffer source)
     (kill-buffer target)))"#;
    crate::common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK (\"abc\" target t ((1 4)))""#]],
    );
}

#[test]
fn divergence_buffer_string_validates_positions_after_hook_narrows() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#" (with-temp-buffer
   (insert "abc")
   (let ((buffer-access-fontified-property nil)
         (buffer-access-fontify-functions
          (list (lambda (beg end) (narrow-to-region 2 3)))))
     (condition-case err
         (list 'value (buffer-string))
       (error (list 'error-symbol (car err) 'error-data (cdr err))))))"#;
    crate::common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK (error-symbol args-out-of-range error-data (1 1))""#]],
    );
}

#[test]
fn divergence_buffer_substring_reversed_bounds_fontification() {
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
          (let ((snapshot (buffer-substring (point-max) (point-min))))
            (list (substring-no-properties snapshot)
                  (get-text-property 0 'oracle-font snapshot)
                  (let ((buffer-access-fontify-functions nil))
                    (buffer-substring-no-properties (point-min) (point-max)))
                  (nreverse calls))))))"#;
    let expect = expect_test::expect![[r#""OK (\"abc\" done \"zbc\" ((1 4)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
