use expect_test::expect;

use super::ParityBatchCase;

fn editing_a_nested_production_playbook_finds_the_project_and_adds_ansible_semantics()
-> ParityBatchCase {
    ParityBatchCase::value(
        "editing_a_nested_production_playbook_finds_the_project_and_adds_ansible_semantics",
        r##"
(let* ((fixture (neomacs-ansible-fixture))
       (root (nth 0 fixture))
       (deploy (nth 2 fixture))
       (default-directory
        (file-name-directory deploy))
       (ansible-root-path nil)
       (ansible-dir-search-limit 8)
       (ac-user-dictionary-files nil)
       buffer)
  (unwind-protect
      (progn
        (setq buffer (find-file-noselect deploy))
        (with-current-buffer buffer
          (text-mode)
          (font-lock-mode 1)
          (ansible-mode 1)
          (font-lock-ensure)
          (let ((enabled-state
                 (list
                  ansible-mode
                  (file-relative-name
                   (ansible-find-root-path)
                   root)
                  (sort
                   (ansible-list-playbooks)
                   #'string-lessp)
                  compile-command
                  (mapcar
                   (lambda (token)
                     (cons
                      token
                      (neomacs-ansible-face-at token)))
                   '("hosts"
                     "tasks"
                     "name"
                     "Publish release"
                     "copy"
                     "{{"
                     "artifact_path"
                     "}}"
                     "when"))
                  (and
                   (member
                    (expand-file-name
                     "dict/ansible"
                     ansible-dir)
                    ac-user-dictionary-files)
                   t))))
            (ansible-mode -1)
            (font-lock-flush)
            (font-lock-ensure)
            (list
             enabled-state
             ansible-mode
             (mapcar
              #'neomacs-ansible-face-at
              '("tasks"
                "Publish release"
                "copy"
                "artifact_path"
                "when"))
             (buffer-substring-no-properties
              (point-min) (point-max))))))
    (when (buffer-live-p buffer)
      (set-buffer-modified-p nil)
      (kill-buffer buffer))))
"##,
        expect![[
            r#"OK ((t "." ("group_vars/production/vault.yml" "playbooks/production/deploy.yml" "playbooks/production/rollback.yml" "site.yml") "LANG=C.UTF-8 ansible-lint [ORACLE-SANDBOX]/ansible-project/playbooks/production/deploy.yml" (("hosts" . ansible-section-face) ("tasks" . ansible-section-face) ("name" . font-lock-builtin-face) ("Publish release" . ansible-task-label-face) ("copy" . font-lock-keyword-face) ("{{" . font-lock-builtin-face) ("artifact_path" . font-lock-function-name-face) ("}}" . font-lock-builtin-face) ("when" . font-lock-builtin-face)) nil) nil (nil nil nil nil nil) "---\n- hosts: production\n  tasks:\n    - name: Publish release\n      copy:\n        src: \"{{ artifact_path }}\"\n        dest: /srv/storefront/app.tar\n      when: release_ready\n")"#
        ]],
    )
}

fn opening_editing_and_saving_a_vault_file_keeps_plaintext_in_emacs_and_ciphertext_on_disk()
-> ParityBatchCase {
    ParityBatchCase::value(
        "opening_editing_and_saving_a_vault_file_keeps_plaintext_in_emacs_and_ciphertext_on_disk",
        r##"
(let* ((fixture (neomacs-ansible-fixture))
       (root (nth 0 fixture))
       (vault-file (nth 4 fixture))
       (vault-log (nth 6 fixture))
       (default-directory root)
       (ansible-vault-password-environment-variable
        "NEOMACS_RELEASE_VAULT_PASSWORD")
       (ansible-vault-password
        #'ansible-vault-password-from-environment)
       (ansible-hook
        (append
         ansible-hook
         '(ansible-auto-decrypt-encrypt)))
       buffer)
  (setenv
   "NEOMACS_RELEASE_VAULT_PASSWORD"
   "release-secret")
  (unwind-protect
      (progn
        (setq buffer (find-file-noselect vault-file))
        (with-current-buffer buffer
          (text-mode)
          (ansible-mode 1)
          (let ((opened
                 (list
                  (buffer-substring-no-properties
                   (point-min) (point-max))
                  (buffer-modified-p)
                  (and
                   (memq
                    'ansible-encrypt-buffer
                    before-save-hook)
                   t)
                  (and
                   (memq
                    'ansible-decrypt-buffer
                    after-save-hook)
                   t))))
            (goto-char (point-min))
            (search-forward "stable")
            (replace-match "canary" t t)
            (save-buffer)
            (list
             opened
             (buffer-substring-no-properties
              (point-min) (point-max))
             (buffer-modified-p)
             (neomacs-ansible-read-file vault-file)
             (neomacs-ansible-read-file vault-log)
             ansible-vault-store-cleanup-file
             (cl-remove-if-not
              (lambda (hook)
                (memq
                 hook
                 '(ansible-encrypt-buffer
                   ansible-decrypt-buffer)))
              (append
               before-save-hook
               after-save-hook))))))
    (when (buffer-live-p buffer)
      (set-buffer-modified-p nil)
      (kill-buffer buffer))))
"##,
        expect![[
            r#"OK (("api_token: initial-secret\nrelease_channel: stable" nil t t) "api_token: initial-secret\nrelease_channel: canary" nil "$ANSIBLE_VAULT;1.1;AES256\nENC:api_token: initial-secret\nENC:release_channel: canary" "decrypt|release-secret\nencrypt|release-secret\ndecrypt|release-secret\n" nil (ansible-encrypt-buffer ansible-decrypt-buffer))"#
        ]],
    )
}

fn encrypting_and_decrypting_an_indented_vars_region_preserves_the_following_play()
-> ParityBatchCase {
    ParityBatchCase::value(
        "encrypting_and_decrypting_an_indented_vars_region_preserves_the_following_play",
        r##"
(let* ((fixture (neomacs-ansible-fixture))
       (root (nth 0 fixture))
       (password-file (nth 5 fixture))
       (vault-log (nth 6 fixture))
       (default-directory root)
       (ansible-vault-password 'file)
       (ansible-vault-password-file password-file))
  (with-temp-buffer
    (text-mode)
    (insert
     "- hosts: production\n"
     "  vars:\n"
     "    api_token: checkout-secret\n"
     "    deploy_key: ssh-ed25519-demo\n"
     "  tasks:\n"
     "    - name: Publish release\n"
     "      copy:\n"
     "        src: \"{{ artifact_path }}\"\n"
     "        dest: /srv/storefront/app.tar\n")
    (goto-char (point-min))
    (search-forward "    api_token")
    (goto-char (match-beginning 0))
    (let ((region-start (point)))
      (ansible-encrypt-region
       region-start
       (point-max))
      (let ((encrypted
             (buffer-substring-no-properties
              (point-min) (point-max))))
        (goto-char region-start)
        (ansible-decrypt-region
         region-start
         (point-max))
        (list
         encrypted
         (buffer-substring-no-properties
          (point-min) (point-max))
         (buffer-modified-p)
         (neomacs-ansible-read-file vault-log)
         ansible-vault-store-cleanup-file)))))
"##,
        expect![[
            r#"OK ("- hosts: production\n  vars:\n    $ANSIBLE_VAULT;1.1;AES256\n    ENC:api_token: checkout-secret\n    ENC:deploy_key: ssh-ed25519-demo\n  tasks:\n    - name: Publish release\n      copy:\n        src: \"{{ artifact_path }}\"\n        dest: /srv/storefront/app.tar\n" "- hosts: production\n  vars:\n    api_token: checkout-secret\n    deploy_key: ssh-ed25519-demo\n  tasks:\n    - name: Publish release\n      copy:\n        src: \"{{ artifact_path }}\"\n        dest: /srv/storefront/app.tar\n" nil "encrypt|team-secret\ndecrypt|team-secret\n" nil)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        editing_a_nested_production_playbook_finds_the_project_and_adds_ansible_semantics(),
        opening_editing_and_saving_a_vault_file_keeps_plaintext_in_emacs_and_ciphertext_on_disk(),
        encrypting_and_decrypting_an_indented_vars_region_preserves_the_following_play(),
    ]
}
