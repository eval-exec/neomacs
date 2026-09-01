//! Oracle parity tests for `indent-to`.

use crate::common::assert_oracle_parity;

#[test]
fn oracle_indent_to_respects_tab_width_and_indent_tabs_mode() {
    let form = r#"(list
                    (let ((tab-width 4) (indent-tabs-mode t))
                      (with-temp-buffer
                        (list (indent-to 6 1)
                              (current-column)
                              (append (buffer-string) nil))))
                    (let ((tab-width 4) (indent-tabs-mode nil))
                      (with-temp-buffer
                        (list (indent-to 6 1)
                              (current-column)
                              (append (buffer-string) nil))))
                    (with-temp-buffer
                      (setq tab-width 4)
                      (insert "ab")
                      (list (indent-to 6 2)
                            (current-column)
                            (append (buffer-string) nil))))"#;
    let expect = expect_test::expect![[
        r#""OK ((6 6 (9 32 32)) (6 6 (32 32 32 32 32 32)) (6 6 (97 98 9 32 32)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_indent_to_honors_inhibit_read_only_binding() {
    let form = r#"(with-temp-buffer
                    (setq buffer-read-only t)
                    (let ((inhibit-read-only t))
                      (list (indent-to 6 1)
                            (current-column)
                            (buffer-string))))"#;
    let expect = expect_test::expect![[r#""OK (6 6 \"      \")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
