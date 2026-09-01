use expect_test::expect;

use super::ParityBatchCase;

fn age_encrypts_and_decrypts_a_recipient_message_through_a_cli() -> ParityBatchCase {
    ParityBatchCase::value(
        "age_encrypts_and_decrypts_a_recipient_message_through_a_cli",
        r##"(let* ((root
                                 (expand-file-name
                                  "round-trip"
                                  (getenv
                                   "NEOMACS_TEST_SANDBOX_ROOT")))
                                (program
                                 (expand-file-name
                                  "fake-age"
                                  root))
                                (log-file
                                 (concat
                                  program
                                  ".log"))
                                (identity
                                 (expand-file-name
                                  "identity.txt"
                                  root)))
                           (make-directory root t)
                           (with-temp-file program
                             (insert
                              "#!/bin/sh\n"
                              "if [ \"$1\" = \"--version\" ]; then echo 1.2.3; exit 0; fi\n"
                              "printf '%s\\n' \"$*\" >> \"${0}.log\"\n"
                              "output=''; mode=''; input=''\n"
                              "while [ \"$#\" -gt 0 ]; do\n"
                              "  case \"$1\" in\n"
                              "    --output) output=$2; shift 2 ;;\n"
                              "    --encrypt) mode=encrypt; shift ;;\n"
                              "    --decrypt) mode=decrypt; shift ;;\n"
                              "    -r|-R|-i) shift 2 ;;\n"
                              "    --armor|-p) shift ;;\n"
                              "    --) shift; input=$1; shift ;;\n"
                              "    *) input=$1; shift ;;\n"
                              "  esac\n"
                              "done\n"
                              "if [ \"$mode\" = encrypt ]; then\n"
                              "  { printf 'age-encryption.org/v1\\n'; cat \"$input\"; } > \"$output\"\n"
                              "elif [ \"$mode\" = decrypt ]; then\n"
                              "  first=$(sed -n '1p' \"$input\")\n"
                              "  if [ \"$first\" != 'age-encryption.org/v1' ]; then\n"
                              "    echo 'age: error: malformed payload' >&2\n"
                              "    exit 1\n"
                              "  fi\n"
                              "  sed '1d' \"$input\" > \"$output\"\n"
                              "fi\n"))
                           (set-file-modes
                            program
                            #o700)
                           (with-temp-file identity
                             (insert
                              "AGE-SECRET-KEY-TEST"))
                           (let* ((age-program
                                   program)
                                  (age-default-recipient
                                   "age1recipient")
                                  (age-default-identity
                                   identity)
                                  (age-always-use-default-keys
                                   t)
                                  (age--configurations
                                   (list
                                    (cons
                                     'Age
                                     (age-config--make-age-configuration
                                      program))))
                                  (recipient-cipher
                                   (age-encrypt-string
                                    (age-make-context
                                     'Age
                                     t)
                                    "deploy-token-42\n"
                                    '("age1recipient")))
                                  (recipient-plain
                                   (age-decrypt-string
                                    (age-make-context)
                                    recipient-cipher))
                                  (log
                                   (with-temp-buffer
                                     (insert-file-contents
                                      log-file)
                                     (buffer-string))))
                             (list
                              recipient-cipher
                              recipient-plain
                              (and
                               (string-match-p
                                "--armor.*--encrypt.*-r age1recipient"
                                log)
                               t)
                              (and
                               (string-match-p
                                "--decrypt.*-i "
                                log)
                               t))))"##,
        expect![[r#"OK ("age-encryption.org/v1\ndeploy-token-42\n" "deploy-token-42\n" t t)"#]],
    )
}

fn age_opens_edits_and_saves_an_encrypted_org_file_transparently() -> ParityBatchCase {
    ParityBatchCase::value(
        "age_opens_edits_and_saves_an_encrypted_org_file_transparently",
        r##"(let* ((root
                                 (expand-file-name
                                  "org-vault"
                                  (getenv
                                   "NEOMACS_TEST_SANDBOX_ROOT")))
                                (program
                                 (expand-file-name
                                  "fake-age"
                                  root))
                                (identity
                                 (expand-file-name
                                  "identity.txt"
                                  root))
                                (file
                                 (expand-file-name
                                  "plans.org.age"
                                  root))
                                buffer)
                           (make-directory root t)
                           (with-temp-file program
                             (insert
                              "#!/bin/sh\n"
                              "if [ \"$1\" = \"--version\" ]; then echo 1.2.3; exit 0; fi\n"
                              "output=''; mode=''; input=''\n"
                              "while [ \"$#\" -gt 0 ]; do\n"
                              "  case \"$1\" in\n"
                              "    --output) output=$2; shift 2 ;;\n"
                              "    --encrypt) mode=encrypt; shift ;;\n"
                              "    --decrypt) mode=decrypt; shift ;;\n"
                              "    -r|-R|-i) shift 2 ;;\n"
                              "    --armor|-p) shift ;;\n"
                              "    --) shift; input=$1; shift ;;\n"
                              "    *) input=$1; shift ;;\n"
                              "  esac\n"
                              "done\n"
                              "if [ \"$mode\" = encrypt ]; then\n"
                              "  { printf 'age-encryption.org/v1\\n'; cat \"$input\"; } > \"$output\"\n"
                              "elif [ \"$mode\" = decrypt ]; then\n"
                              "  first=$(sed -n '1p' \"$input\")\n"
                              "  if [ \"$first\" != 'age-encryption.org/v1' ]; then\n"
                              "    echo 'age: error: malformed payload' >&2\n"
                              "    exit 1\n"
                              "  fi\n"
                              "  sed '1d' \"$input\" > \"$output\"\n"
                              "fi\n"))
                           (set-file-modes
                            program
                            #o700)
                           (with-temp-file identity
                             (insert
                              "AGE-SECRET-KEY-TEST"))
                           (let* ((age-program
                                   program)
                                  (age-default-recipient
                                   "age1recipient")
                                  (age-default-identity
                                   identity)
                                  (age-always-use-default-keys
                                   t)
                                  (age--configurations
                                   (list
                                    (cons
                                     'Age
                                     (age-config--make-age-configuration
                                      program))))
                                  (cipher
                                   (age-encrypt-string
                                    (age-make-context)
                                    "* Project Phoenix\n** TODO rotate deploy token\n"
                                    '("age1recipient"))))
                             (let ((age-inhibit t))
                               (with-temp-file file
                                 (set-buffer-multibyte nil)
                                 (insert cipher)))
                             (unwind-protect
                                 (progn
                                   (age-file-enable)
                                   (setq buffer
                                         (find-file-noselect file))
                                   (let ((opened
                                          (with-current-buffer buffer
                                            (goto-char
                                             (point-max))
                                            (insert
                                             "** DONE publish runbook\n")
                                            (save-buffer)
                                            (list
                                             major-mode
                                             (buffer-substring-no-properties
                                              (point-min)
                                              (point-max))
                                             (local-variable-p
                                              'age-file-encrypt-to)
                                             buffer-auto-save-file-name))))
                                     (when
                                         (buffer-live-p buffer)
                                       (with-current-buffer buffer
                                         (set-buffer-modified-p nil))
                                       (kill-buffer buffer)
                                       (setq buffer nil))
                                     (let* ((raw
                                             (let ((age-inhibit t))
                                               (with-temp-buffer
                                                 (set-buffer-multibyte nil)
                                                 (insert-file-contents-literally
                                                  file)
                                                 (buffer-string))))
                                            (decrypted
                                             (age-decrypt-string
                                              (age-make-context)
                                              raw)))
                                       (list
                                        opened
                                        raw
                                        decrypted))))
                               (when
                                   (buffer-live-p buffer)
                                 (with-current-buffer buffer
                                   (set-buffer-modified-p nil))
                                 (kill-buffer buffer))
                               (age-file-disable))))"##,
        expect![[
            r#"OK ((org-mode "* Project Phoenix\n** TODO rotate deploy token\n** DONE publish runbook\n" t nil) "age-encryption.org/v1\n* Project Phoenix\n** TODO rotate deploy token\n** DONE publish runbook\n" "* Project Phoenix\n** TODO rotate deploy token\n** DONE publish runbook\n")"#
        ]],
    )
}

fn age_supplies_credentials_from_an_encrypted_authinfo_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "age_supplies_credentials_from_an_encrypted_authinfo_file",
        r##"(let* ((root
                                 (expand-file-name
                                  "auth-source"
                                  (getenv
                                   "NEOMACS_TEST_SANDBOX_ROOT")))
                                (program
                                 (expand-file-name
                                  "fake-age"
                                  root))
                                (identity
                                 (expand-file-name
                                  "identity.txt"
                                  root))
                                (file
                                 (expand-file-name
                                  ".authinfo.age"
                                  root)))
                           (make-directory root t)
                           (with-temp-file program
                             (insert
                              "#!/bin/sh\n"
                              "if [ \"$1\" = \"--version\" ]; then echo 1.2.3; exit 0; fi\n"
                              "output=''; mode=''; input=''\n"
                              "while [ \"$#\" -gt 0 ]; do\n"
                              "  case \"$1\" in\n"
                              "    --output) output=$2; shift 2 ;;\n"
                              "    --encrypt) mode=encrypt; shift ;;\n"
                              "    --decrypt) mode=decrypt; shift ;;\n"
                              "    -r|-R|-i) shift 2 ;;\n"
                              "    --armor|-p) shift ;;\n"
                              "    --) shift; input=$1; shift ;;\n"
                              "    *) input=$1; shift ;;\n"
                              "  esac\n"
                              "done\n"
                              "if [ \"$mode\" = encrypt ]; then\n"
                              "  { printf 'age-encryption.org/v1\\n'; cat \"$input\"; } > \"$output\"\n"
                              "elif [ \"$mode\" = decrypt ]; then\n"
                              "  first=$(sed -n '1p' \"$input\")\n"
                              "  if [ \"$first\" != 'age-encryption.org/v1' ]; then\n"
                              "    echo 'age: error: malformed payload' >&2\n"
                              "    exit 1\n"
                              "  fi\n"
                              "  sed '1d' \"$input\" > \"$output\"\n"
                              "fi\n"))
                           (set-file-modes
                            program
                            #o700)
                           (with-temp-file identity
                             (insert
                              "AGE-SECRET-KEY-TEST"))
                           (let* ((age-program
                                   program)
                                  (age-default-recipient
                                   "age1recipient")
                                  (age-default-identity
                                   identity)
                                  (age-always-use-default-keys
                                   t)
                                  (age--configurations
                                   (list
                                    (cons
                                     'Age
                                     (age-config--make-age-configuration
                                      program))))
                                  (cipher
                                   (age-encrypt-string
                                    (age-make-context)
                                    "machine api.example.test login alice password swordfish port 443\n"
                                    '("age1recipient"))))
                             (let ((age-inhibit t))
                               (with-temp-file file
                                 (set-buffer-multibyte nil)
                                 (insert cipher)))
                             (unwind-protect
                                 (progn
                                   (age-file-enable)
                                   (require
                                    'auth-source)
                                   (auth-source-forget-all-cached)
                                   (let* ((auth-sources
                                           (list file))
                                          (auth-source-do-cache
                                           nil)
                                          (entry
                                           (car
                                            (auth-source-search
                                             :host
                                             "api.example.test"
                                             :user
                                             "alice"
                                             :port
                                             "443"
                                             :require
                                             '(:secret)
                                             :max
                                             1)))
                                          (secret
                                           (plist-get
                                            entry
                                            :secret)))
                                     (list
                                      (plist-get
                                       entry
                                       :host)
                                      (plist-get
                                       entry
                                       :user)
                                      (plist-get
                                       entry
                                       :port)
                                      (if
                                          (functionp secret)
                                          (funcall secret)
                                        secret))))
                               (auth-source-forget-all-cached)
                               (age-file-disable))))"##,
        expect![[r#"OK ("api.example.test" "alice" "443" "swordfish")"#]],
    )
}

fn age_reports_a_corrupt_cipher_as_a_user_visible_decryption_error() -> ParityBatchCase {
    ParityBatchCase::value(
        "age_reports_a_corrupt_cipher_as_a_user_visible_decryption_error",
        r##"(let* ((root
                                 (expand-file-name
                                  "corrupt"
                                  (getenv
                                   "NEOMACS_TEST_SANDBOX_ROOT")))
                                (program
                                 (expand-file-name
                                  "fake-age"
                                  root))
                                (identity
                                 (expand-file-name
                                  "identity.txt"
                                  root)))
                           (make-directory root t)
                           (with-temp-file program
                             (insert
                              "#!/bin/sh\n"
                              "if [ \"$1\" = \"--version\" ]; then echo 1.2.3; exit 0; fi\n"
                              "output=''; mode=''; input=''\n"
                              "while [ \"$#\" -gt 0 ]; do\n"
                              "  case \"$1\" in\n"
                              "    --output) output=$2; shift 2 ;;\n"
                              "    --encrypt) mode=encrypt; shift ;;\n"
                              "    --decrypt) mode=decrypt; shift ;;\n"
                              "    -r|-R|-i) shift 2 ;;\n"
                              "    --armor|-p) shift ;;\n"
                              "    --) shift; input=$1; shift ;;\n"
                              "    *) input=$1; shift ;;\n"
                              "  esac\n"
                              "done\n"
                              "if [ \"$mode\" = decrypt ]; then\n"
                              "  first=$(sed -n '1p' \"$input\")\n"
                              "  if [ \"$first\" != 'age-encryption.org/v1' ]; then\n"
                              "    echo 'age: error: malformed payload' >&2\n"
                              "    exit 1\n"
                              "  fi\n"
                              "  sed '1d' \"$input\" > \"$output\"\n"
                              "fi\n"))
                           (set-file-modes
                            program
                            #o700)
                           (with-temp-file identity
                             (insert
                              "AGE-SECRET-KEY-TEST"))
                           (let* ((age-program
                                   program)
                                  (age-default-identity
                                   identity)
                                  (age-always-use-default-keys
                                   t)
                                  (age--configurations
                                   (list
                                    (cons
                                     'Age
                                     (age-config--make-age-configuration
                                      program)))))
                             (condition-case problem
                                 (age-decrypt-string
                                  (age-make-context)
                                  "this is not an age payload\n")
                               (error
                                (list
                                 (car problem)
                                 (error-message-string
                                  problem))))))"##,
        expect![[
            r#"OK (age-error "Age error: \"Age failed with error\", \"malformed payload\"")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        age_encrypts_and_decrypts_a_recipient_message_through_a_cli(),
        age_opens_edits_and_saves_an_encrypted_org_file_transparently(),
        age_supplies_credentials_from_an_encrypted_authinfo_file(),
        age_reports_a_corrupt_cipher_as_a_user_visible_decryption_error(),
    ]
}
