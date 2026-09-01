use expect_test::expect;

use super::ParityBatchCase;

fn enabling_vault_mode_edits_and_saves_a_real_encrypted_group_vars_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_vault_mode_edits_and_saves_a_real_encrypted_group_vars_file",
        r##"
(let* ((fixture (neomacs-ansible-vault-fixture))
       (root (plist-get fixture :root))
       (vault-file
        (plist-get fixture :legacy-vault))
       (vault-command
        (plist-get fixture :vault-command))
       (vault-log
        (plist-get fixture :vault-log))
       (default-directory root)
       (ansible-vault-command vault-command)
       (ansible-vault-password-file nil)
       (auto-mode-alist
        (cons
         '("\\.yml\\'" . text-mode)
         auto-mode-alist))
       buffer)
  (setenv "ANSIBLE_VAULT_PASSWORD_FILE" nil)
  (unwind-protect
      (progn
        (let ((magic-mode-alist
               (cl-remove-if
                (lambda (entry)
                  (eq
                   (cdr entry)
                   #'ansible-vault-mode))
                magic-mode-alist)))
          (setq buffer
                (find-file-noselect vault-file)))
        (with-current-buffer buffer
          (text-mode)
          (ansible-vault-mode 1)
          (let ((opened-state
                 (list
                  major-mode
                  (buffer-substring-no-properties
                   (point-min) (point-max))
                  (buffer-modified-p)
                  backup-inhibited
                  (and
                   (memq
                    'ansible-vault--before-save-hook
                    before-save-hook)
                   t)
                  (and
                   (memq
                    'ansible-vault--after-save-hook
                    after-save-hook)
                   t)
                  ansible-vault--header-version
                  ansible-vault--header-cipher-algorithm
                  ansible-vault--password-file)))
            (goto-char (point-min))
            (search-forward "stable")
            (replace-match "canary" t t)
            (let ((point-before-save (point)))
              (save-buffer)
              (list
               opened-state
               (buffer-substring-no-properties
                (point-min) (point-max))
               (buffer-modified-p)
               (= point-before-save (point))
               ansible-vault--point
               (neomacs-ansible-vault-read-file
                vault-file)
               (neomacs-ansible-vault-read-file
                vault-log))))))
    (when (buffer-live-p buffer)
      (set-buffer-modified-p nil)
      (kill-buffer buffer))))
"##,
        expect![[
            r#"OK ((text-mode "api_token: initial-secret\nrelease_channel: stable\n" nil t t t "1.1" "AES256" "[ORACLE-SANDBOX]/infrastructure/credentials/team.pass") "api_token: initial-secret\nrelease_channel: canary\n" nil t 0 "$ANSIBLE_VAULT;1.1;AES256\nENC:api_token: initial-secret\nENC:release_channel: canary\n" "decrypt|team-secret||\nencrypt|team-secret||\ndecrypt|team-secret||\n")"#
        ]],
    )
}

fn a_production_vault_id_selects_its_password_and_survives_the_edit_save_cycle() -> ParityBatchCase
{
    ParityBatchCase::value(
        "a_production_vault_id_selects_its_password_and_survives_the_edit_save_cycle",
        r##"
(let* ((fixture (neomacs-ansible-vault-fixture))
       (root (plist-get fixture :root))
       (password-file
        (plist-get fixture :password-file))
       (vault-command
        (plist-get fixture :vault-command))
       (vault-log
        (plist-get fixture :vault-log))
       (production-vault
        (plist-get fixture :production-vault))
       (default-directory root)
       (ansible-vault-command vault-command)
       (ansible-vault-vault-id-alist
        (list
         (cons "prod" password-file)))
       (auto-mode-alist
        (cons
         '("\\.yml\\'" . text-mode)
         auto-mode-alist))
       buffer)
  (setenv "ANSIBLE_VAULT_PASSWORD_FILE" nil)
  (unwind-protect
      (progn
        (let ((magic-mode-alist
               (cl-remove-if
                (lambda (entry)
                  (eq
                   (cdr entry)
                   #'ansible-vault-mode))
                magic-mode-alist)))
          (setq buffer
                (find-file-noselect
                 production-vault)))
        (with-current-buffer buffer
          (ansible-vault-mode 1)
          (let ((opened-state
                 (list
                  major-mode
                  (buffer-substring-no-properties
                   (point-min) (point-max))
                  ansible-vault--header-version
                  ansible-vault--header-cipher-algorithm
                  ansible-vault--header-vault-id
                  ansible-vault--vault-id
                  (file-relative-name
                   ansible-vault--password-file
                   root))))
            (goto-char (point-min))
            (search-forward "blue")
            (replace-match "green" t t)
            (save-buffer)
            (list
             opened-state
             (buffer-substring-no-properties
              (point-min) (point-max))
             (buffer-modified-p)
             (neomacs-ansible-vault-read-file
              production-vault)
             (neomacs-ansible-vault-read-file
              vault-log)))))
    (when (buffer-live-p buffer)
      (set-buffer-modified-p nil)
      (kill-buffer buffer))))
"##,
        expect![[
            r#"OK ((text-mode "deployment_ring: blue\napproval_required: true\n" "1.2" "AES256" "prod" "prod" "credentials/team.pass") "deployment_ring: green\napproval_required: true\n" nil "$ANSIBLE_VAULT;1.2;AES256;prod\nENC:deployment_ring: green\nENC:approval_required: true\n" "decrypt|team-secret|prod@[ORACLE-SANDBOX]/infrastructure/credentials/team.pass|\nencrypt|team-secret|prod@[ORACLE-SANDBOX]/infrastructure/credentials/team.pass|prod\ndecrypt|team-secret|prod@[ORACLE-SANDBOX]/infrastructure/credentials/team.pass|\n")"#
        ]],
    )
    .fresh_process()
}

fn encrypting_and_decrypting_one_yaml_secret_preserves_the_surrounding_playbook() -> ParityBatchCase
{
    ParityBatchCase::value(
        "encrypting_and_decrypting_one_yaml_secret_preserves_the_surrounding_playbook",
        r##"
(let* ((fixture (neomacs-ansible-vault-fixture))
       (root (plist-get fixture :root))
       (password-file
        (plist-get fixture :password-file))
       (vault-command
        (plist-get fixture :vault-command))
       (vault-log
        (plist-get fixture :vault-log))
       (default-directory root)
       (ansible-vault-command vault-command)
       (ansible-vault-password-file password-file))
  (setenv "ANSIBLE_VAULT_PASSWORD_FILE" nil)
  (with-temp-buffer
    (setq buffer-file-name
          (expand-file-name
           "group_vars/production/services.yml"
           root))
    (insert
     "api:\n"
     "  endpoint: https://api.example.test\n"
     "  token: checkout-secret\n"
     "  timeout: 30\n"
     "deploy:\n"
     "  strategy: rolling\n")
    (goto-char (point-min))
    (search-forward "checkout-secret")
    (let ((secret-start
           (- (point) (length "checkout-secret")))
          (secret-end (point)))
      (ansible-vault-encrypt-region
       secret-start secret-end)
      (let ((encrypted-state
             (buffer-substring-no-properties
              (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "  token: !vault |")
        (let ((vault-start
               (line-beginning-position))
              (vault-end
               (progn
                 (forward-line 3)
                 (point))))
          (ansible-vault-decrypt-region
           vault-start vault-end)
          (list
           encrypted-state
           (buffer-substring-no-properties
            (point-min) (point-max))
           (buffer-narrowed-p)
           (line-number-at-pos)
           (neomacs-ansible-vault-read-file
            vault-log)))))))
"##,
        expect![[
            r#"OK ("api:\n  endpoint: https://api.example.test\n  token: !vault |\n          $ANSIBLE_VAULT;1.1;AES256\n          ENC:checkout-secret\n\n  timeout: 30\ndeploy:\n  strategy: rolling\n" "api:\n  endpoint: https://api.example.test\n  token: checkout-secret\n\n  timeout: 30\ndeploy:\n  strategy: rolling\n" nil 3 "encrypt_string|team-secret||\ndecrypt|team-secret||\n")"#
        ]],
    )
    .fresh_process()
}

fn entering_the_correct_password_recovers_the_same_buffer_after_an_actionable_error()
-> ParityBatchCase {
    ParityBatchCase::value(
        "entering_the_correct_password_recovers_the_same_buffer_after_an_actionable_error",
        r##"
(let* ((fixture (neomacs-ansible-vault-fixture))
       (root (plist-get fixture :root))
       (wrong-password-file
        (plist-get fixture :wrong-password-file))
       (vault-file
        (plist-get fixture :legacy-vault))
       (vault-command
        (plist-get fixture :vault-command))
       (vault-log
        (plist-get fixture :vault-log))
       (config-file
        (expand-file-name "ansible.cfg" root))
       (default-directory root)
       (ansible-vault-command vault-command)
       (ansible-vault-password-file nil)
       (vault-buffer
        (generate-new-buffer
         " *production-vault-recovery*"))
       generated-password-file
       first-state)
  (setenv "ANSIBLE_VAULT_PASSWORD_FILE" nil)
  (unwind-protect
      (progn
        (with-temp-file config-file
          (insert
           (format
            "[defaults]\nvault_password_file = %s\n"
            wrong-password-file)))
        (with-current-buffer vault-buffer
          (setq buffer-file-name vault-file)
          (insert
           "$ANSIBLE_VAULT;1.1;AES256\n"
           "ENC:api_token: initial-secret\n"
           "ENC:release_channel: stable")
          (ansible-vault-decrypt-current-buffer))
        (setq
         first-state
         (list
          (with-current-buffer vault-buffer
            (list
             (buffer-substring-no-properties
              (point-min) (point-max))
             (file-relative-name
              ansible-vault--password-file
              root)))
          (let ((error-buffer
                 (get-buffer
                  "*ansible-vault-error*")))
            (and
             error-buffer
             (with-current-buffer error-buffer
               (list
                buffer-read-only
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))
        (with-current-buffer vault-buffer
          (cl-letf
              (((symbol-function 'read-passwd)
                (lambda (&rest _arguments)
                  "team-secret")))
            (call-interactively
             #'ansible-vault--request-password))
          (setq generated-password-file
                ansible-vault--password-file)
          (ansible-vault-decrypt-current-buffer))
        (list
         first-state
         (with-current-buffer vault-buffer
           (buffer-substring-no-properties
            (point-min) (point-max)))
         (file-exists-p generated-password-file)
         (file-modes generated-password-file)
         (neomacs-ansible-vault-read-file
          generated-password-file)
         (neomacs-ansible-vault-read-file
          vault-log)))
    (when (buffer-live-p vault-buffer)
      (set-buffer-modified-p nil)
      (kill-buffer vault-buffer))
    (let ((error-buffer
           (get-buffer "*ansible-vault-error*")))
      (when (buffer-live-p error-buffer)
        (kill-buffer error-buffer)))))
"##,
        expect![[
            r#"OK ((("$ANSIBLE_VAULT;1.1;AES256\nENC:api_token: initial-secret\nENC:release_channel: stable" "credentials/wrong.pass") (t "$ [ORACLE-SANDBOX]/bin/ansible-vault decrypt --output=- --vault-password-file=\"[ORACLE-SANDBOX]/infrastructure/credentials/wrong.pass\"\nERROR! Decryption failed: invalid vault password\n\n")) "api_token: initial-secret\nrelease_channel: stable\n" t 256 "team-secret" "decrypt|wrong-secret||\ndecrypt|team-secret||\n")"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        enabling_vault_mode_edits_and_saves_a_real_encrypted_group_vars_file(),
        a_production_vault_id_selects_its_password_and_survives_the_edit_save_cycle(),
        encrypting_and_decrypting_one_yaml_secret_preserves_the_surrounding_playbook(),
        entering_the_correct_password_recovers_the_same_buffer_after_an_actionable_error(),
    ]
}
