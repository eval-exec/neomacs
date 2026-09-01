use expect_test::expect;

use super::ParityBatchCase;

fn asdf_vm_plugin_menu_entries_merge_repository_and_installed_state_with_exact_faces()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_plugin_menu_entries_merge_repository_and_installed_state_with_exact_faces",
        r##"(cl-letf
               (((symbol-function
                  'asdf-vm-plugin--repository-alist)
                 (lambda ()
                   '(("ruby" .
                      "https://example/ruby.git")
                     ("nodejs" .
                      "https://example/node.git")
                     ("資料" .
                      "https://example/資料.git"))))
                ((symbol-function
                  'asdf-vm-plugin-list)
                 (lambda (&optional _)
                   '("nodejs"
                     "資料"))))
               (mapcar
                (lambda (entry)
                  (list
                   (car entry)
                   (mapcar
                    (lambda (cell)
                      (list
                       (substring-no-properties
                        cell)
                       (get-text-property
                        0
                        'font-lock-face
                        cell)))
                    (append
                     (cadr entry)
                     nil))))
                (asdf-vm-plugin-menu--list-entries)))"##,
        expect![[
            r#"OK (("https://example/ruby.git" (("available" asdf-vm-plugin-menu-status-available) ("ruby" asdf-vm-plugin-menu-status-available) ("https://example/ruby.git" asdf-vm-plugin-menu-status-available))) ("https://example/node.git" (("installed" asdf-vm-plugin-menu-status-installed) ("nodejs" asdf-vm-plugin-menu-status-installed) ("https://example/node.git" asdf-vm-plugin-menu-status-installed))) ("https://example/資料.git" (("installed" asdf-vm-plugin-menu-status-installed) ("資料" asdf-vm-plugin-menu-status-installed) ("https://example/資料.git" asdf-vm-plugin-menu-status-installed))))"#
        ]],
    )
}

fn asdf_vm_plugin_menu_mode_initializes_columns_padding_sort_refresh_imenu_and_keymap()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_plugin_menu_mode_initializes_columns_padding_sort_refresh_imenu_and_keymap",
        r##"(with-temp-buffer
               (let ((asdf-vm-plugin-menu-list-padding
                      4)
                     (asdf-vm-plugin-menu-status-column-width
                      12)
                     (asdf-vm-plugin-menu-name-column-width
                      31)
                     (asdf-vm-plugin-menu-url-column-width
                      42))
                 (asdf-vm-plugin-menu-mode)
                 (list
                  major-mode
                  mode-name
                  tabulated-list-format
                  tabulated-list-padding
                  tabulated-list-sort-key
                  revert-buffer-function
                  imenu-prev-index-position-function
                  imenu-extract-index-name-function
                  (mapcar
                   (lambda (key)
                     (list
                      key
                      (lookup-key
                       asdf-vm-plugin-menu-mode-map
                       (kbd key))))
                   '("u"
                     "DEL"
                     "d"
                     "i"
                     "r"
                     "w"
                     "x")))))"##,
        expect![[
            r#"OK (asdf-vm-plugin-menu-mode "ASDF-VM Plugin Menu" [("Status" 12 asdf-vm-plugin-menu--status-predicate) ("Plugin" 31 asdf-vm-plugin-menu--name-predicate) ("Repository Url" 42 asdf-vm-plugin-menu--url-predicate)] 4 ("Status") asdf-vm-plugin-menu--refresh asdf-vm-plugin-menu--imenu-prev-index-position-function tabulated-list-get-id (("u" asdf-vm-plugin-menu-mark-unmark) ("DEL" asdf-vm-plugin-menu-backup-unmark) ("d" asdf-vm-plugin-menu-mark-delete) ("i" asdf-vm-plugin-menu-mark-install) ("r" revert-buffer) ("w" asdf-vm-plugin-browse-url) ("x" asdf-vm-plugin-menu-execute)))"#
        ]],
    )
}

fn asdf_vm_plugin_menu_field_getters_return_selected_entry_columns_or_empty_strings()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_plugin_menu_field_getters_return_selected_entry_columns_or_empty_strings",
        r##"(with-temp-buffer
               (asdf-vm-plugin-menu-mode)
               (setq
                tabulated-list-entries
                '(("ruby-id"
                   ["installed"
                    "ruby"
                    "https://example/ruby.git"])
                  ("node-id"
                   ["available"
                    "nodejs"
                    "https://example/node.git"])))
               (tabulated-list-print t)
               (let ((read-fields
                      (lambda ()
                        (list
                         (tabulated-list-get-id)
                         (asdf-vm-plugin-menu--get-status)
                         (asdf-vm-plugin-menu--get-name)
                         (asdf-vm-plugin-menu--get-url)))))
                 (goto-char
                  (point-min))
                 (let ((header
                        (funcall read-fields))
                       (missing
                        (list
                         (asdf-vm-test-tabulated-list-goto-id
                          "missing")
                         (funcall read-fields))))
                   (asdf-vm-test-tabulated-list-goto-id
                    "ruby-id")
                   (let ((ruby
                          (funcall read-fields)))
                     (asdf-vm-test-tabulated-list-goto-id
                      "node-id")
                     (list
                      header
                      missing
                      ruby
                      (funcall read-fields))))))"##,
        expect![[
            r#"OK (("ruby-id" "installed" "ruby" "https://example/ruby.git") (nil (nil "" "" "")) ("ruby-id" "installed" "ruby" "https://example/ruby.git") ("node-id" "available" "nodejs" "https://example/node.git"))"#
        ]],
    )
}

fn asdf_vm_plugin_menu_mark_commands_enforce_mode_and_only_mark_eligible_statuses()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_plugin_menu_mark_commands_enforce_mode_and_only_mark_eligible_statuses",
        r##"(let ((wrong-mode
                    (with-temp-buffer
                      (asdf-vm-test-error-data
                       (lambda ()
                         (asdf-vm-plugin-menu-mark-install))))))
               (with-temp-buffer
                 (asdf-vm-plugin-menu-mode)
                 (setq
                  tabulated-list-entries
                  '(("ruby-id"
                     ["installed"
                      "ruby"
                      "https://example/ruby.git"])
                    ("node-id"
                     ["available"
                      "nodejs"
                      "https://example/node.git"])))
                 (tabulated-list-print t)
                 (asdf-vm-test-tabulated-list-goto-id
                  "ruby-id")
                 (asdf-vm-plugin-menu-mark-install)
                 (let ((after-ineligible-install
                        (tabulated-list-get-id)))
                   (asdf-vm-test-tabulated-list-goto-id
                    "node-id")
                   (asdf-vm-plugin-menu-mark-delete)
                   (let ((after-ineligible-delete
                          (tabulated-list-get-id)))
                     (asdf-vm-test-tabulated-list-goto-id
                      "ruby-id")
                     (asdf-vm-plugin-menu-mark-delete)
                     (asdf-vm-test-tabulated-list-goto-id
                      "node-id")
                     (asdf-vm-plugin-menu-mark-install)
                     (let ((marked
                            (mapcar
                             (lambda (id)
                               (asdf-vm-test-tabulated-list-goto-id
                                id)
                               (list
                                id
                                (char-after
                                 (line-beginning-position))))
                             '("ruby-id"
                               "node-id"))))
                       (asdf-vm-test-tabulated-list-goto-id
                        "node-id")
                       (asdf-vm-plugin-menu-mark-unmark)
                       (asdf-vm-test-tabulated-list-goto-id
                        "node-id")
                       (asdf-vm-plugin-menu-backup-unmark)
                       (list
                        wrong-mode
                        after-ineligible-install
                        after-ineligible-delete
                        marked
                        (mapcar
                         (lambda (id)
                           (asdf-vm-test-tabulated-list-goto-id
                            id)
                           (list
                            id
                            (char-after
                             (line-beginning-position))))
                         '("ruby-id"
                           "node-id"))))))))"##,
        expect![[
            r#"OK ((:error asdf-vm-incorrect-mode-error (fundamental-mode)) "node-id" nil (("ruby-id" 68) ("node-id" 73)) (("ruby-id" 32) ("node-id" 32)))"#
        ]],
    )
}

fn asdf_vm_plugin_browse_url_dispatches_primary_secondary_and_missing_url_error() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asdf_vm_plugin_browse_url_dispatches_primary_secondary_and_missing_url_error",
        r##"(let ((browse-url-secondary-browser-function
                    (lambda (url)
                      (list
                       :secondary
                       url)))
                   calls)
               (cl-letf
                   (((symbol-function
                      'browse-url)
                     (lambda (&rest arguments)
                       (push arguments calls)
                       :primary))
                    ((symbol-function
                      'asdf-vm-plugin-menu--get-name)
                     (lambda ()
                       "ruby")))
                 (list
                  (asdf-vm-plugin-browse-url
                   "https://example/ruby")
                  (asdf-vm-plugin-browse-url
                   "https://example/node"
                   t)
                  (asdf-vm-test-error-data
                   (lambda ()
                     (asdf-vm-plugin-browse-url
                      "")))
                  (nreverse calls))))"##,
        expect![[
            r#"OK (:primary (:secondary "https://example/node") (:error asdf-vm-plugin-menu-missing-url-error ("ruby")) (("https://example/ruby")))"#
        ]],
    )
}

fn asdf_vm_plugin_menu_execute_scans_marks_runs_delete_and_install_batches_then_refreshes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_plugin_menu_execute_scans_marks_runs_delete_and_install_batches_then_refreshes",
        r##"(with-temp-buffer
               (setq major-mode
                     'asdf-vm-plugin-menu-mode)
               (insert
                (concat
                 "D delete-ruby\n"
                 "I install-node\n"
                 "I install-資料\n"
                 "  untouched\n"))
               (goto-char
                (point-min))
               (let (calls)
                 (cl-letf
                     (((symbol-function
                        'asdf-vm-plugin-menu--get-name)
                       (lambda ()
                         (pcase
                             (line-number-at-pos)
                           (1 "ruby")
                           (2 "nodejs")
                           (3 "資料")
                           (_ "untouched"))))
                      ((symbol-function
                        'asdf-vm-plugin-menu--get-url)
                       (lambda ()
                         (format
                          "https://example/%s.git"
                          (asdf-vm-plugin-menu--get-name))))
                      ((symbol-function
                        'asdf-vm-plugin-remove)
                       (lambda (&rest arguments)
                         (push
                          (cons
                           :remove arguments)
                          calls)
                         :removed))
                      ((symbol-function
                        'asdf-vm-plugin-add)
                       (lambda (&rest arguments)
                         (push
                          (cons
                           :add arguments)
                          calls)
                         :added))
                      ((symbol-function
                        'asdf-vm-plugin-menu--refresh)
                       (lambda (&rest arguments)
                         (push
                          (cons
                           :refresh arguments)
                          calls)
                         :refreshed)))
                   (list
                    (asdf-vm-plugin-menu-execute)
                    (nreverse calls)))))"##,
        expect![[
            r#"OK (:refreshed ((:remove "ruby" t) (:add "資料" "https://example/資料.git" t) (:add "nodejs" "https://example/nodejs.git" t) (:refresh)))"#
        ]],
    )
}

fn asdf_vm_plugin_menu_sort_predicates_apply_status_name_and_repository_rules() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_plugin_menu_sort_predicates_apply_status_name_and_repository_rules",
        r##"(let ((available-z
                    '("z"
                      ["available"
                       "zeta"
                       "https://z.example"]))
                   (available-a
                    '("a"
                      ["available"
                       "alpha"
                       "https://a.example"]))
                   (installed-a
                    '("i"
                      ["installed"
                       "alpha"
                       "https://i.example"])))
               (list
                (asdf-vm-plugin-menu--status-predicate
                 installed-a
                 available-a)
                (asdf-vm-plugin-menu--status-predicate
                 available-z
                 available-a)
                (asdf-vm-plugin-menu--name-predicate
                 available-a
                 available-z)
                (asdf-vm-plugin-menu--url-predicate
                 available-a
                 available-z)
                (asdf-vm-plugin-menu--url-predicate
                 available-z
                 available-a)))"##,
        expect!["OK (t nil t t nil)"],
    )
}

fn asdf_vm_plugin_menu_command_creates_utf8_buffer_refreshes_and_displays_same_window()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_plugin_menu_command_creates_utf8_buffer_refreshes_and_displays_same_window",
        r##"(let ((asdf-vm-plugin-menu-buffer-name
                    "*fixture-plugin-menu*")
                   calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-plugin-menu--refresh)
                     (lambda (&rest arguments)
                       (push
                        (list
                         :refresh
                         (buffer-name
                          (current-buffer))
                         arguments)
                        calls)
                       :refreshed))
                    ((symbol-function
                      'pop-to-buffer-same-window)
                     (lambda (buffer &rest arguments)
                       (push
                        (list
                         :display
                         (buffer-name buffer)
                         arguments)
                        calls)
                       buffer)))
                 (let ((result
                        (asdf-vm-plugin-menu)))
                   (with-current-buffer
                       asdf-vm-plugin-menu-buffer-name
                     (list
                      (buffer-name result)
                      major-mode
                      buffer-file-coding-system
                      (nreverse calls))))))"##,
        expect![[
            r#"OK ("*fixture-plugin-menu*" asdf-vm-plugin-menu-mode utf-8 ((:refresh "*fixture-plugin-menu*" nil) (:display "*fixture-plugin-menu*" nil)))"#
        ]],
    )
}

pub(super) fn plugin_menu_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asdf_vm_plugin_menu_entries_merge_repository_and_installed_state_with_exact_faces(),
        asdf_vm_plugin_menu_mode_initializes_columns_padding_sort_refresh_imenu_and_keymap(),
        asdf_vm_plugin_menu_field_getters_return_selected_entry_columns_or_empty_strings(),
        asdf_vm_plugin_menu_mark_commands_enforce_mode_and_only_mark_eligible_statuses(),
        asdf_vm_plugin_browse_url_dispatches_primary_secondary_and_missing_url_error(),
        asdf_vm_plugin_menu_execute_scans_marks_runs_delete_and_install_batches_then_refreshes(),
        asdf_vm_plugin_menu_sort_predicates_apply_status_name_and_repository_rules(),
        asdf_vm_plugin_menu_command_creates_utf8_buffer_refreshes_and_displays_same_window(),
    ]
}
