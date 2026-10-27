use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_crypt_tag_match_encrypt_decrypt_visibility_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-crypt--matcher-tags)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-crypt)
  (with-temp-buffer
    (let ((org-crypt-tag-matcher "crypt")
          (org-crypt-key nil)
          (org-crypt-disable-auto-save t))
      (org-mode)
      (insert "* Secret :crypt:\n")
      (insert ":PROPERTIES:\n:CRYPTKEY: test-key\n:END:\n")
      (insert "Secret body text.\n")
      (insert "** Nested secret\n")
      (insert "Nested secret body.\n")
      (insert "* Plain\n")
      (insert "Plain body.\n")
      (insert "* Also secret :crypt:\n")
      (insert "Another secret.\n")
      (let ((snap (lambda ()
                    (mapcar
                     (lambda (needle)
                       (save-excursion
                         (goto-char (point-min))
                         (search-forward needle)
                         (list needle
                               (line-number-at-pos)
                               (not (null (org-at-encrypted-entry-p)))
                               (invisible-p (point)))))
                     '("Secret" "Nested secret" "Plain" "Also secret")))))
        ;; Check tag matcher
        (let* ((matcher (org-crypt--matcher-tags))
               (initial (funcall snap)))
          ;; Encrypt entries
          (condition-case err
              (org-encrypt-entries)
            (error nil))
          (let ((after-encrypt (funcall snap))
                (encrypted-buf (buffer-substring-no-properties
                                (point-min) (point-max))))
            ;; Decrypt entries
            (condition-case err
                (org-decrypt-entries)
              (error nil))
            (let ((after-decrypt (funcall snap))
                  (decrypted-buf (buffer-substring-no-properties
                                  (point-min) (point-max))))
              (list matcher
                    initial
                    after-encrypt
                    after-decrypt
                    (not (string= encrypted-buf decrypted-buf))
                    (string-match-p "Secret body" decrypted-buf)))))))))"##,
        expect,
    );
}
