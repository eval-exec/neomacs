//! GNU file-save parity for non-Unicode characters in legacy encodings.
#[path = "../src/common.rs"]
mod common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn shift_jis_save_preserves_charset_only_character_and_original_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((file (make-temp-file "cp932-save-" nil ".txt"))
                  (make-backup-files nil)
                  (coding-system-for-read nil)
                  (coding-system-for-write nil))
             (unwind-protect
                 (progn
                   (let ((coding-system-for-write 'no-conversion))
                     (write-region (unibyte-string #x8a #xbf #x8e #x9a #x87 #x40 #x0a) nil file))
                   (let ((coding (with-current-buffer (find-file-noselect file)
                                   (goto-char (point-max)) (insert "abc\n")
                                   (save-buffer) buffer-file-coding-system)))
                     (with-temp-buffer
                       (set-buffer-multibyte nil)
                       (insert-file-contents-literally file)
                       (list coding (string-to-list (buffer-string))))))
               (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
               (delete-file file)))"#,
        expect_test::expect![[
            r#""OK (japanese-shift-jis-unix (138 191 142 154 135 64 10 97 98 99 10))""#
        ]],
    );
}
