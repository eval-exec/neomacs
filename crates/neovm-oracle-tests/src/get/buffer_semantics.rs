//! Oracle parity tests for GNU `get-buffer`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_get_buffer_ignores_text_properties_in_name_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/buffer.c:get-buffer uses assoc_ignore_text_properties, so a
    // propertized string names the same buffer as its plain string contents.
    let form = r#"
(let* ((name " *oracle-get-buffer-props*")
       (buf (get-buffer-create name))
       (query (propertize (copy-sequence name) 'face 'bold 'category 'oracle)))
  (unwind-protect
      (list (eq (get-buffer query) buf)
            (eq (get-buffer buf) buf)
            (null (get-buffer (propertize " *oracle-get-buffer-missing*"
                                          'face 'bold)))
            (condition-case err
                (get-buffer 42)
              (error (list (car err) (cdr err)))))
    (when (buffer-live-p buf)
      (kill-buffer buf))))
"#;
    let expect = expect_test::expect![[r#""OK (t t t (wrong-type-argument (stringp 42)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
