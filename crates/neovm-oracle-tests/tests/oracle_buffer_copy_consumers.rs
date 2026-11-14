//! Shared buffer-copy consumers retain bytes, multibyteness, and properties.
//!
//! Source contract: GNU Emacs 31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9,
//! editfns.c `Finsert_buffer_substring` / `Freplace_region_contents`,
//! insdel.c `insert_from_buffer_1` / `replace_range`, and buffer.c
//! `Fset_buffer_multibyte`. Empty expectations await an attested-GNU refresh.

use crate::common::assert_oracle_parity_expect;

#[test]
fn insert_buffer_substring_copies_bytes_and_properties_without_access_hooks() {
    // GNU runs source access hooks here. Bind them nil to isolate the shared
    // copy helper; this case does not pin the separate hook-dispatch debt.
    assert_oracle_parity_expect(
        r#"(let ((source (generate-new-buffer "neovm--copy-source")))
              (unwind-protect
                  (progn
                    (with-current-buffer source
                      (insert (string ?A #x3bb ?Z))
                      (put-text-property 1 4 'oracle-copy 'source)
                      (put-text-property 2 3 'face 'bold)
                      (setq-local buffer-access-fontify-functions nil))
                    (with-temp-buffer
                      (setq-local buffer-access-fontify-functions nil)
                      (let ((destination (current-buffer)))
                        (insert-buffer-substring source 1 4)
                        (let ((text (buffer-substring-no-properties 1 (point-max))))
                          (list (string-to-list text)
                                (string-to-list (string-as-unibyte text))
                                (multibyte-string-p text)
                                (mapcar (lambda (pos) (text-properties-at pos)) '(1 2 3))
                                (eq destination (current-buffer))
                                (with-current-buffer source
                                  (string-to-list (buffer-substring-no-properties 1 4))))))))
                (kill-buffer source)))"#,
        expect_test::expect![[
            r#""OK ((65 955 90) (65 206 187 90) t ((oracle-copy source) (face bold oracle-copy source) (oracle-copy source)) t (65 955 90))""#
        ]],
    );
}

#[test]
fn replace_region_contents_buffer_slice_copies_bytes_and_properties_without_access_hooks() {
    assert_oracle_parity_expect(
        r#"(let ((calls 0)
                  (source (generate-new-buffer "neovm--replace-copy-source")))
              (unwind-protect
                  (progn
                    (with-current-buffer source
                      (insert (string ?x ?A #x3bb ?Z ?y))
                      (put-text-property 2 5 'oracle-copy 'source)
                      (put-text-property 3 4 'face 'bold)
                      (setq-local buffer-access-fontify-functions
                                  (list (lambda (start end) (setq calls (1+ calls))))))
                    (with-temp-buffer
                      (insert "old")
                      (setq-local buffer-access-fontify-functions
                                  (list (lambda (start end) (setq calls (1+ calls)))))
                      (replace-region-contents 1 4 (vector source 2 5) nil 0)
                      (let ((text (buffer-substring-no-properties 1 (point-max))))
                        (list (string-to-list text)
                              (string-to-list (string-as-unibyte text))
                              (multibyte-string-p text)
                              (mapcar (lambda (pos) (text-properties-at pos)) '(1 2 3))
                              calls
                              (with-current-buffer source
                                (string-to-list (buffer-substring-no-properties 1 6)))))))
                (kill-buffer source)))"#,
        expect_test::expect![[
            r#""OK ((65 955 90) (65 206 187 90) t ((oracle-copy source) (face bold oracle-copy source) (oracle-copy source)) 0 (120 65 955 90 121))""#
        ]],
    );
}

#[test]
fn set_buffer_multibyte_roundtrip_preserves_bytes_and_remaps_properties_without_access_hooks() {
    assert_oracle_parity_expect(
        r#"(let ((calls 0))
              (with-temp-buffer
                (insert (string ?A #x3bb ?Z))
                (put-text-property 1 4 'oracle-copy 'source)
                (put-text-property 2 3 'face 'bold)
                (setq-local buffer-access-fontify-functions
                            (list (lambda (start end) (setq calls (1+ calls)))))
                (set-buffer-multibyte nil)
                (let* ((text (buffer-substring-no-properties 1 (point-max)))
                       (unibyte-state
                        (list (string-to-list text)
                              (string-to-list (string-as-unibyte text))
                              (multibyte-string-p text)
                              (mapcar (lambda (pos) (text-properties-at pos)) '(1 2 3 4)))))
                  (set-buffer-multibyte t)
                  (let ((text (buffer-substring-no-properties 1 (point-max))))
                    (list unibyte-state
                          (list (string-to-list text)
                                (string-to-list (string-as-unibyte text))
                                (multibyte-string-p text)
                                (mapcar (lambda (pos) (text-properties-at pos)) '(1 2 3)))
                          calls)))))"#,
        expect_test::expect![[
            r#""OK (((65 206 187 90) (65 206 187 90) nil ((oracle-copy source) (face bold oracle-copy source) (face bold oracle-copy source) (oracle-copy source))) ((65 955 90) (65 206 187 90) t ((oracle-copy source) (face bold oracle-copy source) (oracle-copy source))) 0)""#
        ]],
    );
}
