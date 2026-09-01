use expect_test::expect;

use super::ParityBatchCase;

fn ansible_doc_opens_completed_module_documentation_from_a_real_playbook() -> ParityBatchCase {
    ParityBatchCase::value(
        "ansible_doc_opens_completed_module_documentation_from_a_real_playbook",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "ansible-doc-playbook-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (default-directory root)
       (playbook
        (expand-file-name "playbooks/deploy.yml" root))
       playbook-buffer
       result)
  (unwind-protect
      (progn
        (neomacs-ansible-doc-test-cleanup root)
        (make-directory (file-name-directory playbook) t)
        (with-temp-file playbook
          (insert
           "---\n"
           "- name: Deploy application configuration\n"
           "  hosts: app_servers\n"
           "  become: true\n"
           "  tasks:\n"
           "    - name: Copy the configuration\n"
           "      copy:\n"
           "        src: files/app.conf\n"
           "        dest: /etc/myapp/app.conf\n"
           "        backup: true\n"))
        (let ((tool
               (neomacs-ansible-doc-test-install-tool root)))
          (neomacs-ansible-doc-test-use-tool root tool)
          (setq ansible-doc--modules nil
                playbook-buffer (find-file-noselect playbook))
          (add-hook 'yaml-mode-hook #'ansible-doc-mode)
          (switch-to-buffer playbook-buffer)
          (yaml-mode)
          (goto-char (point-min))
          (search-forward "copy:")
          (backward-char 2)
          (let ((playbook-state
                 (list
                  (file-relative-name buffer-file-name root)
                  major-mode
                  ansible-doc-mode
                  (key-binding (kbd "C-c ?"))
                  (line-number-at-pos)
                  (current-column)
                  (buffer-substring-no-properties
                   (line-beginning-position)
                   (line-end-position))
                  (buffer-modified-p)))
                completion-state)
            (cl-letf
                (((symbol-function 'completing-read)
                  (lambda
                    (prompt collection predicate require-match
                            initial-input history default
                            &rest _arguments)
                    (setq completion-state
                          (list
                           prompt
                           (all-completions "" collection predicate)
                           require-match
                           initial-input
                           history
                           default))
                    "")))
              (call-interactively
               (key-binding (kbd "C-c ?"))))
            (font-lock-ensure)
            (jit-lock-fontify-now
             (point-min)
             (point-max))
            (let* ((documentation
                    (substring-no-properties (buffer-string)))
                   (face-at
                    (lambda (text)
                      (save-excursion
                        (goto-char (point-min))
                        (search-forward text)
                        (get-char-property
                         (match-beginning 0)
                         'face))))
                   (xref
                    (save-excursion
                      (goto-char (point-min))
                      (search-forward "[file]")
                      (button-at (match-beginning 0))))
                   (options
                    (mapcar
                     (lambda (entry)
                       (let ((position (cdr entry)))
                         (list
                          (car entry)
                          (if (markerp position)
                              (marker-position position)
                            position))))
                     (cdr
                      (assoc
                       "Options"
                       (imenu--make-index-alist t))))))
              (setq result
                    (list
                     playbook-state
                     completion-state
                     (list
                      (buffer-name)
                      major-mode
                      mode-name
                      (ansible-doc-current-module)
                      buffer-read-only
                      truncate-lines
                      (point)
                      documentation
                      (mapcar
                       (lambda (text)
                         (list text (funcall face-at text)))
                       '("> COPY" "Options" "= src" "- dest"
                         "Default:" "Choices:" "Notes:"
                         "Requirements:"))
                      options
                      (and
                       xref
                       (list
                        (substring-no-properties
                         (button-label xref))
                        (substring-no-properties
                        (button-get xref 'ansible-module))
                        (button-get xref 'action)
                        (button-get xref 'help-echo))))
                     (neomacs-ansible-doc-test-file-string
                      (plist-get tool :trace))))))))
    (neomacs-ansible-doc-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (("playbooks/deploy.yml" yaml-mode t ansible-doc 7 9 "      copy:" nil) ("Documentation for Ansible Module (default copy): " ("copy" "file" "user" "ansible.builtin.template") t nil nil "copy") ("*ansible-doc copy*" ansible-doc-module-mode "ADoc Module" "copy" t t 1 "> COPY\n\nCopy application configuration to managed hosts.\n\nOptions (= is mandatory):\n= src\n    Local path of the configuration file.\n- dest\n    Absolute path on the managed host.\n    [Default: /etc/myapp/app.conf]\n- backup\n    Create a backup before replacing the file.\n    (Choices: yes, no)\n    See [file] for ownership and mode management.\nNotes:  The source is read from the control machine.\nRequirements:  none\n\n# - name: Deploy the application configuration\n  copy:\n    src: files/app.conf\n    dest: /etc/myapp/app.conf\n    backup: yes\n" (("> COPY" ansible-doc-header) ("Options" ansible-doc-section) ("= src" ansible-doc-mandatory-option) ("- dest" ansible-doc-option) ("Default:" ansible-doc-label) ("Choices:" ansible-doc-label) ("Notes:" ansible-doc-section) ("Requirements:" ansible-doc-section)) (("src" 85) ("dest" 133) ("backup" 214)) ("[file]" "file" ansible-doc-follow-module-xref "mouse-2, RET: visit module")) "ansible-doc cwd=[ORACLE-SANDBOX]/ansible-doc-playbook-workflow/playbooks <--list>\nansible-doc cwd=[ORACLE-SANDBOX]/ansible-doc-playbook-workflow/playbooks <copy>\n")"#
        ]],
    )
}

fn ansible_doc_follows_a_module_reference_and_bookmarks_the_destination() -> ParityBatchCase {
    ParityBatchCase::value(
        "ansible_doc_follows_a_module_reference_and_bookmarks_the_destination",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "ansible-doc-navigation-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (default-directory root)
       (playbook
        (expand-file-name "roles/myapp/tasks/main.yml" root))
       result)
  (unwind-protect
      (progn
        (neomacs-ansible-doc-test-cleanup root)
        (make-directory (file-name-directory playbook) t)
        (with-temp-file playbook
          (insert
           "---\n"
           "- name: Ensure configuration directory exists\n"
           "  file:\n"
           "    path: /etc/myapp\n"
           "    state: directory\n"
           "    mode: 0750\n"
           "- name: Install application configuration\n"
           "  copy:\n"
           "    src: app.conf\n"
           "    dest: /etc/myapp/app.conf\n"))
        (let ((tool
               (neomacs-ansible-doc-test-install-tool root)))
          (neomacs-ansible-doc-test-use-tool root tool)
          (require 'bookmark)
          (setq bookmark-alist nil
                bookmark-save-flag nil)
          (find-file playbook)
          (add-hook 'yaml-mode-hook #'ansible-doc-mode)
          (yaml-mode)
          (ansible-doc "copy")
          (font-lock-ensure)
          (jit-lock-fontify-now
           (point-min)
           (point-max))
          (let ((copy-state
                 (list
                  (buffer-name)
                  (substring-no-properties
                   (ansible-doc-current-module))
                  (point)
                  (buffer-substring-no-properties
                   (line-beginning-position)
                   (line-end-position)))))
            (goto-char (point-min))
            (search-forward "[file]")
            (push-button (match-beginning 0))
            (font-lock-ensure)
            (jit-lock-fontify-now
             (point-min)
             (point-max))
            (let ((file-state
                   (list
                    (buffer-name)
                    (substring-no-properties
                     (ansible-doc-current-module))
                    major-mode
                    (point)
                    (buffer-substring-no-properties
                     (point-min)
                     (point-max)))))
              (bookmark-set "file-module-docs")
              (let* ((record
                      (bookmark-get-bookmark-record
                       "file-module-docs"))
                     (bookmark-state
                      (list
                       (substring-no-properties
                        (bookmark-prop-get
                         record
                         'ansible-module))
                       (bookmark-prop-get record 'handler))))
                (switch-to-buffer
                 (find-buffer-visiting playbook))
                (goto-char (point-min))
                (search-forward "state: directory")
                (bookmark-jump "file-module-docs")
                (setq result
                      (list
                       copy-state
                       file-state
                       bookmark-state
                       (list
                        (buffer-name)
                        (substring-no-properties
                         (ansible-doc-current-module))
                        major-mode
                        (point)
                        (buffer-substring-no-properties
                         (line-beginning-position)
                         (line-end-position)))
                       (sort
                        (mapcar #'buffer-name
                                (cl-remove-if-not
                                 (lambda (buffer)
                                   (string-prefix-p
                                    "*ansible-doc "
                                    (buffer-name buffer)))
                                 (buffer-list)))
                       #'string<)
                       (neomacs-ansible-doc-test-file-string
                        (plist-get tool :trace)))))))))
    (neomacs-ansible-doc-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (("*ansible-doc copy*" "copy" 1 "> COPY") ("*ansible-doc file*" "file" ansible-doc-module-mode 1 "> FILE\n\nManage ownership, permissions, and state of remote paths.\n\nOptions (= is mandatory):\n= path\n    Path to manage.\n- state\n    Desired path state.\n    (Choices: file, directory, absent)\n- mode\n    Filesystem permissions.\n\n# - name: Create the configuration directory\n  file:\n    path: /etc/myapp\n    state: directory\n    mode: 0750\n") ("file" ansible-doc-jump-module-bookmark) ("*ansible-doc file*" "file" ansible-doc-module-mode 1 "> FILE") ("*ansible-doc copy*" "*ansible-doc file*") "ansible-doc cwd=[ORACLE-SANDBOX]/ansible-doc-navigation-workflow/roles/myapp/tasks <copy>\nansible-doc cwd=[ORACLE-SANDBOX]/ansible-doc-navigation-workflow/roles/myapp/tasks <file>\n")"#
        ]],
    )
}

fn ansible_doc_reuses_completion_after_editing_then_reloads_changed_documentation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ansible_doc_reuses_completion_after_editing_then_reloads_changed_documentation",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "ansible-doc-edit-reload-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (default-directory root)
       (playbook
        (expand-file-name "playbooks/accounts.yml" root))
       playbook-buffer
       completion-calls
       result)
  (unwind-protect
      (progn
        (neomacs-ansible-doc-test-cleanup root)
        (make-directory (file-name-directory playbook) t)
        (with-temp-file playbook
          (insert
           "---\n"
           "- name: Provision application resources\n"
           "  hosts: app_servers\n"
           "  tasks:\n"
           "    - name: Deploy the configuration\n"
           "      copy:\n"
           "        src: app.conf\n"
           "        dest: /etc/myapp/app.conf\n"))
        (let ((tool
               (neomacs-ansible-doc-test-install-tool root)))
          (neomacs-ansible-doc-test-use-tool root tool)
          (setq ansible-doc--modules nil
                playbook-buffer (find-file-noselect playbook))
          (add-hook 'yaml-mode-hook #'ansible-doc-mode)
          (cl-letf
              (((symbol-function 'completing-read)
                (lambda
                  (prompt collection predicate require-match
                          _initial-input _history default
                          &rest _arguments)
                  (push
                   (list
                    prompt
                    (all-completions
                     ""
                     collection
                     predicate)
                    require-match
                    default)
                   completion-calls)
                  "")))
            (switch-to-buffer playbook-buffer)
            (yaml-mode)
            (goto-char (point-min))
            (search-forward "copy:")
            (backward-char 2)
            (call-interactively #'ansible-doc)
            (let ((copy-buffer (current-buffer)))
              (switch-to-buffer playbook-buffer)
              (goto-char (point-min))
              (search-forward
               "Deploy the configuration")
              (replace-match
               "Create the application service account"
               t
               t)
              (goto-char (point-min))
              (search-forward "copy:")
              (replace-match "user:" t t)
              (forward-line 1)
              (let ((fields-begin (point)))
                (forward-line 2)
                (delete-region
                 fields-begin
                 (point)))
              (insert
               "        name: myapp\n"
               "        system: true\n")
              (save-buffer)
              (goto-char (point-min))
              (search-forward "user:")
              (backward-char 2)
              (call-interactively #'ansible-doc)
              (font-lock-ensure)
              (jit-lock-fontify-now
               (point-min)
               (point-max))
              (goto-char (point-min))
              (search-forward "= name")
              (let ((old-point (point))
                    (before-reload
                     (substring-no-properties
                      (buffer-string))))
                (with-temp-file
                    (expand-file-name
                     "user-doc-updated"
                     root)
                  (insert "ready\n"))
                (revert-buffer nil t)
                (font-lock-ensure)
                (jit-lock-fontify-now
                 (point-min)
                 (point-max))
                (setq result
                      (list
                       (nreverse completion-calls)
                       (list
                        (buffer-name copy-buffer)
                        (buffer-live-p copy-buffer)
                        (with-current-buffer copy-buffer
                          (ansible-doc-current-module)))
                       (list
                        (buffer-name)
                        (ansible-doc-current-module)
                        old-point
                        (point)
                        before-reload
                        (substring-no-properties
                         (buffer-string))
                        (get-char-property
                         (save-excursion
                           (goto-char (point-min))
                           (search-forward
                            "/usr/sbin/nologin")
                           (match-beginning 0))
                         'face))
                       (with-current-buffer playbook-buffer
                         (list
                          (buffer-substring-no-properties
                           (point-min)
                           (point-max))
                          (buffer-modified-p)
                          (line-number-at-pos)
                          (current-column)))
                       (neomacs-ansible-doc-test-file-string
                        (plist-get tool :trace)))))))))
    (neomacs-ansible-doc-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK ((("Documentation for Ansible Module (default copy): " ("copy" "file" "user" "ansible.builtin.template") t "copy") ("Documentation for Ansible Module (default user): " ("copy" "file" "user" "ansible.builtin.template") t "user")) ("*ansible-doc copy*" t "copy") ("*ansible-doc user*" "user" 79 79 "> USER\n\nManage application service accounts.\n\nOptions (= is mandatory):\n= name\n    Account name.\n- shell\n    Login shell.\n    [Default: /bin/sh]\n- system\n    Create a system account.\n    (Choices: yes, no)\n\n# - name: Create the application account\n  user:\n    name: myapp\n    system: yes\n" "> USER\n\nManage application service accounts and login policy.\n\nOptions (= is mandatory):\n= name\n    Account name.\n- shell\n    Login shell.\n    [Default: /usr/sbin/nologin]\n- system\n    Create a system account.\n    (Choices: yes, no)\n\n# - name: Create the application account\n  user:\n    name: myapp\n    system: yes\n" ansible-doc-default) ("---\n- name: Provision application resources\n  hosts: app_servers\n  tasks:\n    - name: Create the application service account\n      user:\n        name: myapp\n        system: true\n" nil 6 9) "ansible-doc cwd=[ORACLE-SANDBOX]/ansible-doc-edit-reload-workflow/playbooks <--list>\nansible-doc cwd=[ORACLE-SANDBOX]/ansible-doc-edit-reload-workflow/playbooks <copy>\nansible-doc cwd=[ORACLE-SANDBOX]/ansible-doc-edit-reload-workflow/playbooks <user>\nansible-doc cwd=[ORACLE-SANDBOX]/ansible-doc-edit-reload-workflow/playbooks <user>\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ansible_doc_opens_completed_module_documentation_from_a_real_playbook(),
        ansible_doc_follows_a_module_reference_and_bookmarks_the_destination(),
        ansible_doc_reuses_completion_after_editing_then_reloads_changed_documentation(),
    ]
}
