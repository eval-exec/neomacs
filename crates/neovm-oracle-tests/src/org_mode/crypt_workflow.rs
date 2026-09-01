use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_crypt_stubbed_encrypt_decrypt_reuse_hook_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-crypt)
  (with-temp-buffer
    (let ((org-crypt-key "default@example.org")
          (org-crypt-tag-matcher "crypt")
          (org-crypt-disable-auto-save nil)
          (cipher-table nil)
          (calls nil))
      (cl-letf (((symbol-function 'epg-make-context)
                 (lambda (&rest args)
                   (push (cons 'context args) calls)
                   'mock-context))
                ((symbol-function 'epg-list-keys)
                 (lambda (_context name &optional mode)
                   (push (list 'keys name mode) calls)
                   (and (not (string= name ""))
                        (list (concat "KEY:" name)))))
                ((symbol-function 'epg-encrypt-string)
                 (lambda (_context plain recipients &optional sign trust)
                   (let ((cipher
                          (format "-----BEGIN PGP MESSAGE-----\nkey=%S sign=%S trust=%S\nsha=%s\n-----END PGP MESSAGE-----\n"
                                  recipients sign trust (sha1 plain))))
                     (push (list 'encrypt recipients plain) calls)
                     (push (cons (org-crypt--encrypted-text
                                  1 (with-temp-buffer
                                      (insert cipher)
                                      (point-max)))
                                 plain)
                           cipher-table)
                     cipher)))
                ((symbol-function 'epg-decrypt-string)
                 (lambda (_context cipher)
                   (push (list 'decrypt cipher) calls)
                   (or (cdr (assoc cipher cipher-table))
                       (error "missing cipher")))))
        (org-mode)
        (insert "* Secrets :crypt:\n")
        (insert ":PROPERTIES:\n:CRYPTKEY: alice@example.org\n:END:\n")
        (insert "Plain alpha\n")
        (insert "** Nested raw\nBody nested\n")
        (insert "* Symmetric :crypt:\n")
        (insert ":PROPERTIES:\n:CRYPTKEY: nil\n:END:\n")
        (insert "Plain beta\n")
        (let ((initial (buffer-substring-no-properties
                        (point-min) (point-max))))
          (goto-char (point-min))
          (org-encrypt-entries)
          (let ((after-encrypt
                 (buffer-substring-no-properties (point-min) (point-max)))
                (encrypted-regions
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (search-forward needle)
                      (beginning-of-line)
                      (org-at-encrypted-entry-p)))
                  '("Secrets" "Symmetric"))))
            (goto-char (point-min))
            (org-decrypt-entry)
            (let ((after-first-decrypt
                   (buffer-substring-no-properties
                    (point-min) (point-max))))
              (org-encrypt-entry)
              (let ((after-reencrypt
                     (buffer-substring-no-properties
                      (point-min) (point-max))))
                (goto-char (point-min))
                (search-forward "Symmetric")
                (beginning-of-line)
                (org-decrypt-entry)
                (goto-char (point-min))
                (org-crypt-use-before-save-magic)
                (let ((hooks (mapcar (lambda (fn)
                                       (cond ((eq fn 'org-encrypt-entries)
                                              'org-encrypt-entries)
                                             ((functionp fn) 'function)
                                             (t fn)))
                                     org-mode-hook))
                      (encrypt-calls
                       (mapcar (lambda (call)
                                 (and (eq (car-safe call) 'encrypt)
                                      (list (nth 1 call)
                                            (string-match-p
                                             "Plain alpha\\|Plain beta"
                                             (nth 2 call)))))
                               (reverse calls))))
                  (list initial
                        after-encrypt
                        encrypted-regions
                        after-first-decrypt
                        after-reencrypt
                        (buffer-substring-no-properties
                         (point-min) (point-max))
                        encrypt-calls
                        hooks
                        (mapcar (lambda (call)
                                  (if (eq (car-safe call) 'decrypt)
                                      (list 'decrypt
                                            (string-match-p
                                             "BEGIN PGP MESSAGE"
                                             (nth 1 call)))
                                    call))
                                (reverse calls)))))))))))"##,
        expect,
    );
}

#[test]
fn org_crypt_decrypt_nested_headings_autosave_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-crypt)
  (require 'org-fold)
  (with-temp-buffer
    (let ((org-crypt-key "fallback@example.org")
          (org-crypt-tag-matcher "crypt")
          (org-crypt-disable-auto-save 'encrypt)
          (cipher-table nil)
          (calls nil))
      (cl-labels
          ((cipher-for
            (name plain)
            (let ((cipher
                   (format "-----BEGIN PGP MESSAGE-----\n%s\n-----END PGP MESSAGE-----\n"
                           name)))
              (push (cons (org-crypt--encrypted-text
                           1 (with-temp-buffer
                               (insert cipher)
                               (point-max)))
                          plain)
                    cipher-table)
              cipher))
           (snapshot
            (label)
            (list label
                  (buffer-substring-no-properties
                   (point-min) (point-max))
                  (mapcar
                   (lambda (needle)
                     (save-excursion
                       (goto-char (point-min))
                       (search-forward needle nil t)
                       (and (match-beginning 0)
                            (list needle
                                  (line-number-at-pos)
                                  (org-current-level)
                                  (org-at-heading-p)
                                  (not (null
                                        (org-invisible-p
                                         (line-beginning-position))))))))
                   '("Vault" "Child one" "Grand child" "Peer one"
                     "Symmetric" "Sym child" "Plain" "Not encrypted"))
                  (mapcar
                   (lambda (fn)
                     (cond ((eq fn 'org-encrypt-entries)
                            'org-encrypt-entries)
                           ((functionp fn) 'function)
                           (t fn)))
                   auto-save-hook))))
        (let* ((plain-a "* Child one\nchild body\n** Grand child\ngrand body\n* Peer one\npeer body\n")
               (plain-b "** Sym child\nsym body\n"))
          (cl-letf (((symbol-function 'epg-make-context)
                     (lambda (&rest args)
                       (push (cons 'context args) calls)
                       'mock-context))
                    ((symbol-function 'epg-list-keys)
                     (lambda (_context name &optional mode)
                       (push (list 'keys name mode) calls)
                       (and name
                            (not (string= name ""))
                            (list (concat "KEY:" name)))))
                    ((symbol-function 'epg-encrypt-string)
                     (lambda (_context plain recipients &optional sign trust)
                       (let ((cipher
                              (format "-----BEGIN PGP MESSAGE-----\nre:%s\n-----END PGP MESSAGE-----\n"
                                      (sha1 plain))))
                         (push (list 'encrypt recipients plain) calls)
                         (push (cons (org-crypt--encrypted-text
                                      1 (with-temp-buffer
                                          (insert cipher)
                                          (point-max)))
                                     plain)
                               cipher-table)
                         cipher)))
                    ((symbol-function 'epg-decrypt-string)
                     (lambda (_context cipher)
                       (push (list 'decrypt cipher) calls)
                       (or (cdr (assoc cipher cipher-table))
                           (error "missing cipher")))))
            (org-mode)
            (auto-save-mode 1)
            (insert "* Vault :crypt:\n")
            (insert ":PROPERTIES:\n:CRYPTKEY: bob@example.org\n:END:\n")
            (insert (cipher-for "cipher-a" plain-a))
            (insert "* Symmetric :crypt:\n")
            (insert ":PROPERTIES:\n:CRYPTKEY: nil\n:END:\n")
            (insert (cipher-for "cipher-b" plain-b))
            (insert "* Plain\nNot encrypted\n")
            (goto-char (point-min))
            (org-fold-hide-subtree)
            (let ((initial (snapshot 'initial)))
              (org-decrypt-entries)
              (let ((after-decrypt (snapshot 'after-decrypt))
                    (encrypted-after-decrypt
                     (mapcar
                      (lambda (needle)
                        (save-excursion
                          (goto-char (point-min))
                          (search-forward needle)
                          (beginning-of-line)
                          (org-at-encrypted-entry-p)))
                      '("Vault" "Symmetric" "Plain"))))
                (run-hooks 'auto-save-hook)
                (let ((after-autosave (snapshot 'after-autosave))
                      (encrypted-after-autosave
                       (mapcar
                        (lambda (needle)
                          (save-excursion
                            (goto-char (point-min))
                            (search-forward needle)
                            (beginning-of-line)
                            (not (null (org-at-encrypted-entry-p)))))
                        '("Vault" "Symmetric" "Plain"))))
                  (list initial
                        after-decrypt
                        encrypted-after-decrypt
                        after-autosave
                        encrypted-after-autosave
                        (mapcar
                         (lambda (call)
                           (pcase (car-safe call)
                             ('encrypt
                              (list 'encrypt
                                    (nth 1 call)
                                    (string-match-p
                                     "Child one\\|Sym child"
                                     (nth 2 call))
                                    (string-match-p
                                     "^\\*\\* Child one"
                                     (nth 2 call))))
                             ('decrypt
                              (list 'decrypt
                                    (string-match-p
                                     "BEGIN PGP MESSAGE"
                                     (nth 1 call))))
                             (_ call)))
                         (reverse calls)))))))))))"##,
        expect,
    );
}

#[test]
fn org_crypt_matcher_key_error_autosave_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-crypt)
  (with-temp-buffer
    (let ((org-crypt-key "global@example.org")
          (org-crypt-tag-matcher "crypt+LEVEL=1")
          (cipher-table nil)
          (calls nil)
          (ask-answers '(t nil)))
      (cl-labels
          ((compact-call
            (call)
            (pcase (car-safe call)
              ('encrypt
               (list 'encrypt
                     (nth 1 call)
                     (string-match-p "top secret\\|nested secret\\|broken secret"
                                     (nth 2 call))
                     (string-match-p "SHOULD-FAIL" (nth 2 call))))
              ('decrypt
               (list 'decrypt
                     (string-match-p "BEGIN PGP MESSAGE" (nth 1 call))))
              (_ call)))
           (at-heading
            (title fn)
            (save-excursion
              (goto-char (point-min))
              (search-forward title)
              (beginning-of-line)
              (funcall fn)))
           (policy
            (setting answer)
            (let ((org-crypt-disable-auto-save setting)
                  (ask-answers (list answer)))
              (with-temp-buffer
                (org-mode)
                (setq buffer-file-name
                      (expand-file-name "crypt-policy.org" temporary-file-directory))
                (auto-save-mode 1)
                (let ((before (list buffer-auto-save-file-name
                                    (local-variable-p 'auto-save-hook))))
                  (org-crypt-check-auto-save)
                  (list setting
                        answer
                        before
                        buffer-auto-save-file-name
                        (mapcar (lambda (fn)
                                  (cond ((eq fn 'org-encrypt-entries)
                                         'org-encrypt-entries)
                                        ((functionp fn) 'function)
                                        (t fn)))
                                auto-save-hook)))))))
        (cl-letf (((symbol-function 'epg-make-context)
                   (lambda (&rest args)
                     (push (cons 'context args) calls)
                     'mock-context))
                  ((symbol-function 'epg-list-keys)
                   (lambda (_context name &optional mode)
                     (push (list 'keys name mode) calls)
                     (cond ((or (null name) (string= name "")
                                (string= name "missing@example.org"))
                            nil)
                           (t (list (concat "KEY:" name))))))
                  ((symbol-function 'epg-encrypt-string)
                   (lambda (_context plain recipients &optional sign trust)
                     (when (string-match-p "SHOULD-FAIL" plain)
                       (error "mock encrypt refused payload"))
                     (let ((cipher
                            (format "-----BEGIN PGP MESSAGE-----\nrecipients=%S sign=%S trust=%S sha=%s\n-----END PGP MESSAGE-----\n"
                                    recipients sign trust (sha1 plain))))
                       (push (list 'encrypt recipients plain) calls)
                       (push (cons (org-crypt--encrypted-text
                                    1 (with-temp-buffer
                                        (insert cipher)
                                        (point-max)))
                                   plain)
                             cipher-table)
                       cipher)))
                  ((symbol-function 'epg-decrypt-string)
                   (lambda (_context cipher)
                     (push (list 'decrypt cipher) calls)
                     (or (cdr (assoc cipher cipher-table))
                         (error "mock missing cipher"))))
                  ((symbol-function 'y-or-n-p)
                   (lambda (prompt)
                     (push (list 'ask prompt) calls)
                     (pop ask-answers))))
          (org-mode)
          (insert "* Top Crypt :crypt:\n")
          (insert ":PROPERTIES:\n:CRYPTKEY: alice@example.org\n:END:\n")
          (insert "top secret\n")
          (insert "** Nested Crypt :crypt:\n")
          (insert ":PROPERTIES:\n:CRYPTKEY: bob@example.org\n:END:\n")
          (insert "nested secret\n")
          (insert "* Symmetric Top :crypt:\n")
          (insert ":PROPERTIES:\n:CRYPTKEY: nil\n:END:\n")
          (insert "symmetric secret\n")
          (insert "* Missing Key Top :crypt:\n")
          (insert ":PROPERTIES:\n:CRYPTKEY: missing@example.org\n:END:\n")
          (insert "missing secret\n")
          (insert "* Plain Tagged Child\n")
          (insert "** Child Crypt :crypt:\n")
          (insert "child secret\n")
          (insert "* Broken Top :crypt:\n")
          (insert "SHOULD-FAIL broken secret\n")
          (let* ((keys-before
                  (list
                   (at-heading "Top Crypt" #'org-crypt-key-for-heading)
                   (at-heading "Nested Crypt" #'org-crypt-key-for-heading)
                   (at-heading "Symmetric Top" #'org-crypt-key-for-heading)
                   (at-heading "Missing Key Top" #'org-crypt-key-for-heading)
                   (at-heading "Child Crypt" #'org-crypt-key-for-heading)))
                 (bulk-error
                  (condition-case err
                      (progn (goto-char (point-min))
                             (org-encrypt-entries)
                             nil)
                    (error (error-message-string err))))
                 (encrypted-flags
                  (mapcar (lambda (title)
                            (at-heading title
                              (lambda ()
                                (not (null (org-at-encrypted-entry-p))))))
                          '("Top Crypt" "Nested Crypt" "Symmetric Top"
                            "Missing Key Top" "Child Crypt" "Broken Top")))
                 (after-bulk
                  (buffer-substring-no-properties (point-min) (point-max)))
                 (decrypt-error
                  (progn
                    (goto-char (point-max))
                    (insert "* Corrupt :crypt:\n")
                    (insert "-----BEGIN PGP MESSAGE-----\nunknown\n-----END PGP MESSAGE-----\n")
                    (beginning-of-line 0)
                    (condition-case err
                        (org-decrypt-entry)
                      (error (error-message-string err)))))
                 (corrupt-still-encrypted
                  (at-heading "Corrupt"
                    (lambda () (not (null (org-at-encrypted-entry-p))))))
                 (policy-results
                  (list (policy t t)
                        (policy nil nil)
                        (policy 'ask t)
                        (policy 'ask nil)
                        (policy 'encrypt nil)
                        (policy 'unknown nil))))
            (list keys-before
                  bulk-error
                  encrypted-flags
                  (string-match-p "SHOULD-FAIL broken secret" after-bulk)
                  decrypt-error
                  corrupt-still-encrypted
                   policy-results
                   (mapcar #'compact-call (reverse calls))))))))"##,
        expect,
    );
}

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
