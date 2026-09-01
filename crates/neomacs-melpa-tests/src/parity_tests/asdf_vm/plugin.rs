use expect_test::expect;

use super::ParityBatchCase;

fn asdf_vm_plugin_index_parser_reads_real_repository_files_and_signals_exact_failures()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_plugin_index_parser_reads_real_repository_files_and_signals_exact_failures",
        r##"(let* ((valid
                     (asdf-vm-test-path
                      "plugin-index/ruby"))
                    (unicode
                     (asdf-vm-test-path
                      "plugin-index/資料"))
                    (missing
                     (asdf-vm-test-path
                      "plugin-index/missing"))
                    (malformed
                     (asdf-vm-test-path
                      "plugin-index/malformed")))
               (asdf-vm-test-write-file
                valid
                (concat
                 "name = ruby\n"
                 "repository = https://github.com/asdf-vm/asdf-ruby.git\n"))
               (asdf-vm-test-write-file
                unicode
                "repository = ssh://example/資料 λ.git\n")
               (asdf-vm-test-write-file
                malformed
                "name = no-repository\n")
               (mapcar
                (lambda (path)
                  (asdf-vm-test-error-data
                   (lambda ()
                     (asdf-vm-plugin--parse-index-file
                      path))))
                (list
                 valid
                 unicode
                 missing
                 malformed)))"##,
        expect![[
            r#"OK ((:ok ("ruby" . "https://github.com/asdf-vm/asdf-ruby.git")) (:ok ("資料" . "ssh://example/資料 λ.git")) (:error asdf-vm-plugin-unreadable-repository-file ("[ORACLE-SANDBOX]/plugin-index/missing")) (:error search-failed ("repository = ")))"#
        ]],
    )
}

fn asdf_vm_plugin_repository_alist_loads_real_disk_index_memoizes_then_refreshes_when_cleared()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_plugin_repository_alist_loads_real_disk_index_memoizes_then_refreshes_when_cleared",
        r##"(let* ((root
                     (asdf-vm-test-path
                      "repository"))
                    (plugins
                     (expand-file-name
                      "plugins"
                      root))
                    (ruby
                     (expand-file-name
                      "ruby"
                      plugins))
                    (node
                     (expand-file-name
                      "nodejs"
                      plugins))
                    (asdf-vm-plugin-repository-path
                     root)
                    (asdf-vm-plugin--repository-alist
                     nil))
               (asdf-vm-test-write-file
                ruby
                "repository = https://example/ruby-v1.git\n")
               (asdf-vm-test-write-file
                node
                "repository = https://example/node.git\n")
               (let ((first
                      (asdf-vm-plugin--repository-alist)))
                 (asdf-vm-test-write-file
                  ruby
                  "repository = https://example/ruby-v2.git\n")
                 (let ((memoized
                        (asdf-vm-plugin--repository-alist)))
                   (setq
                    asdf-vm-plugin--repository-alist
                    nil)
                   (list
                    first
                    memoized
                    (asdf-vm-plugin--repository-alist)))))"##,
        expect![[
            r#"OK (#1=(("nodejs" . "https://example/node.git") ("ruby" . "https://example/ruby-v1.git")) #1# (("nodejs" . "https://example/node.git") ("ruby" . "https://example/ruby-v2.git")))"#
        ]],
    )
}

fn asdf_vm_plugin_name_and_url_readers_forward_completion_options_and_repository_default()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_plugin_name_and_url_readers_forward_completion_options_and_repository_default",
        r##"(let ((asdf-vm-plugin--repository-alist
                    '(("ruby" .
                       "https://example/ruby.git")
                      ("nodejs" .
                       "https://example/node.git")))
                   calls)
               (cl-letf
                   (((symbol-function
                      'completing-read)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :complete arguments)
                        calls)
                       "ruby"))
                    ((symbol-function
                      'read-string)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :read arguments)
                        calls)
                       "https://chosen/repository.git")))
                 (list
                  (asdf-vm-plugin--plugin-completing-read
                   'predicate
                   t
                   "ru"
                   'history
                   "ruby"
                   t)
                  (asdf-vm-plugin--git-url-read-string
                   "ruby")
                  (asdf-vm-plugin--git-url-read-string
                   "missing"
                   "https://initial/value.git"
                   'history
                   "default"
                   t)
                  (nreverse calls))))"##,
        expect![[
            r#"OK ("ruby" "https://chosen/repository.git" "https://chosen/repository.git" ((:complete "Plugin name: " ("ruby" "nodejs") predicate t "ru" history "ruby" t) (:read "Plugin git url: " "https://example/ruby.git" nil nil nil) (:read "Plugin git url: " "https://initial/value.git" history "default" t)))"#
        ]],
    )
}

fn asdf_vm_plugin_installed_and_available_lists_reflect_real_filesystem_and_messages()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_plugin_installed_and_available_lists_reflect_real_filesystem_and_messages",
        r##"(let* ((asdf-vm--plugins-directory
                     (file-name-as-directory
                      (asdf-vm-test-path
                       "installed-plugins")))
                    (asdf-vm-plugin--repository-alist
                     '(("ruby" .
                        "https://example/ruby.git")
                       ("nodejs" .
                        "https://example/node.git")
                       ("資料" .
                        "https://example/資料.git"))))
               (make-directory
                (expand-file-name
                 "ruby"
                 asdf-vm--plugins-directory)
                t)
               (make-directory
                (expand-file-name
                 "資料"
                 asdf-vm--plugins-directory)
                t)
               (asdf-vm-test-write-file
                (expand-file-name
                 "standalone.plugin"
                 asdf-vm--plugins-directory)
                "fixture")
               (asdf-vm-test-write-file
                (expand-file-name
                 ".hidden"
                 asdf-vm--plugins-directory)
                "ignored")
               (list
                (asdf-vm-plugin-list)
                (asdf-vm-plugin-list-all)))"##,
        expect![[
            r#"OK (("ruby" "standalone" "資料") (("ruby" "https://example/ruby.git") ("nodejs" "https://example/node.git") ("資料" "https://example/資料.git")))"#
        ]],
    )
}

fn asdf_vm_plugin_installed_completion_forwards_all_arguments_and_disk_candidates()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_plugin_installed_completion_forwards_all_arguments_and_disk_candidates",
        r##"(let (calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-plugin--installed-plugins)
                     (lambda ()
                       '("ruby"
                         "nodejs"
                         "資料")))
                    ((symbol-function
                      'completing-read)
                     (lambda (&rest arguments)
                       (push arguments calls)
                       "資料")))
                 (list
                  (asdf-vm-plugin--installed-plugin-completing-read
                   'predicate
                   t
                   "資"
                   'history
                   "ruby"
                   t)
                  (nreverse calls))))"##,
        expect![[
            r#"OK ("資料" (("Installed plugin name: " ("ruby" "nodejs" "資料") predicate t "資" history "ruby" t)))"#
        ]],
    )
}

fn asdf_vm_plugin_mutating_commands_construct_add_remove_update_and_update_all_calls_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_plugin_mutating_commands_construct_add_remove_update_and_update_all_calls_exactly",
        r##"(let (calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-call)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :call arguments)
                        calls)
                       :queued)))
                 (list
                  (asdf-vm-plugin-add
                   "ruby"
                   "https://example/ruby.git"
                   1)
                  (asdf-vm-plugin-remove
                   "nodejs")
                  (asdf-vm-plugin-update
                   "python")
                  (asdf-vm-plugin-update
                   "python"
                   "feature/λ"
                   4)
                  (asdf-vm-plugin-update-all
                   1)
                  (nreverse calls))))"##,
        expect![[
            r#"OK (:queued :queued :queued :queued :queued ((:call :command (plugin add) :command-arguments ("ruby" "https://example/ruby.git") :blocking 1) (:call :command (plugin remove) :command-arguments ("nodejs") :blocking nil) (:call :command #1=(plugin update) :command-arguments ("python") :blocking nil) (:call :command #1# :command-arguments ("python" "feature/λ") :blocking 4) (:call :command (plugin update) :command-arguments ("--all") :blocking 1)))"#
        ]],
    )
}

pub(super) fn plugin_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asdf_vm_plugin_index_parser_reads_real_repository_files_and_signals_exact_failures(),
        asdf_vm_plugin_repository_alist_loads_real_disk_index_memoizes_then_refreshes_when_cleared(
        ),
        asdf_vm_plugin_name_and_url_readers_forward_completion_options_and_repository_default(),
        asdf_vm_plugin_installed_and_available_lists_reflect_real_filesystem_and_messages(),
        asdf_vm_plugin_installed_completion_forwards_all_arguments_and_disk_candidates(),
        asdf_vm_plugin_mutating_commands_construct_add_remove_update_and_update_all_calls_exactly(),
    ]
}
