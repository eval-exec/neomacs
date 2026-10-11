//! Runtime charset-category detection agrees with GNU and preserves file bytes.
#[path = "../src/common.rs"]
mod common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn chinese_gb_file_detection_and_save_preserve_extension_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((file (make-temp-file "gbk-detect-" nil ".txt"))
                  (make-backup-files nil)
                  (coding-system-for-read nil) (coding-system-for-write nil))
             (set-language-environment 'Chinese-GB)
             (unwind-protect
                 (progn
                   (let ((coding-system-for-write 'no-conversion))
                     (write-region (unibyte-string #x86 #xaa #xe0 #xc2 #x0a) nil file))
                   (let ((read-state
                          (with-current-buffer (find-file-noselect file)
                            (prog1 (list buffer-file-coding-system (string-to-list (buffer-string)))
                              (goto-char (point-max)) (insert "abc\n") (save-buffer)))))
                     (with-temp-buffer
                       (set-buffer-multibyte nil) (insert-file-contents-literally file)
                       (list read-state (string-to-list (buffer-string))))))
               (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
               (delete-file file)))"#,
        expect_test::expect![[
            r#""OK ((chinese-gbk-unix (21872 21990 10)) (134 170 224 194 10 97 98 99 10))""#
        ]],
    );
}

#[test]
fn preferred_charset_detection_covers_gbk_boundary_and_windows_1252() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(progn
             (set-language-environment 'Chinese-GB)
             (let ((gbk (mapcar
                         (lambda (codes)
                           (and (memq 'chinese-gbk
                                      (detect-coding-string (apply #'unibyte-string codes))) t))
                         '((#x9f #x40) (#x81 #x80) (#x81 #x40) (#x86 #xb4)
                           (#x88 #xd2) (#xa0 #x40) (#xaa #x40) (#xfe #x40)
                           (#xe9 #x46) (#x81 #x7f) (#x81) (#x81 #x3f)
                           (#x81 #xff) (#xff #x40) (#x86 #xaa #x81)))))
               (prefer-coding-system 'windows-1252)
               (list gbk (mapcar (lambda (code)
                                  (detect-coding-string (unibyte-string code #x20 #x31 #x30 #x0a)))
                                '(#x80 #x81 #x8d #x9f)))))"#,
        expect_test::expect![[
            r#""OK ((t t t t t t t t t t nil nil nil nil nil) ((windows-1252-unix raw-text-unix) (windows-1252-unix raw-text-unix) (windows-1252-unix raw-text-unix) (windows-1252-unix emacs-mule-unix raw-text-unix)))""#
        ]],
    );
}

#[test]
fn multibyte_string_and_region_detection_preserve_raw_byte_interpretation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(progn
             (set-language-environment 'Chinese-GB)
             (let ((text (concat "漢" (string-to-multibyte
                                      (unibyte-string #x86 #xaa #xe0 #xc2 #x0a)))))
               (list (detect-coding-string text t)
                     (with-temp-buffer
                       (insert text)
                       (detect-coding-region (point-min) (point-max) t)))))"#,
        expect_test::expect![[r#""OK (chinese-gbk-unix chinese-gbk-unix)""#]],
    );
}
