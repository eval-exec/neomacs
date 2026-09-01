use expect_test::expect;

use super::ParityBatchCase;

fn affe_exact_pin_dependency_features_and_custom_group_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "affe_exact_pin_dependency_features_and_custom_group_match",
        r##"(let* ((descriptor
                     (cadr (assq 'affe package-alist)))
                    (consult-descriptor
                     (cadr (assq 'consult package-alist))))
               (list
                (package-desc-name descriptor)
                (package-version-join
                 (package-desc-version descriptor))
                (package-desc-reqs descriptor)
                (and consult-descriptor
                     (package-desc-name consult-descriptor))
                (and consult-descriptor
                     (package-version-join
                      (package-desc-version
                       consult-descriptor)))
                (mapcar #'featurep
                        '(affe consult server))
                (get 'affe 'group-documentation)
                (get 'affe 'custom-prefix)
                (mapcar
                 (lambda (parent)
                   (assq 'affe
                         (get parent 'custom-group)))
                 '(files minibuffer))
                (get 'affe 'custom-links)))"##,
        expect![[
            r#"OK (affe "20260519.1026" ((emacs (29 1)) (consult (2 8))) consult "20260716.1105" (t t t) "Asynchronous Fuzzy Finder for Emacs." "affe-" ((affe custom-group) (affe custom-group)) ((emacs-library-link :tag "Library Source" "affe.el") (url-link :tag "Website" "https://github.com/minad/affe")))"#
        ]],
    )
}

fn affe_custom_variables_and_histories_preserve_defaults_metadata_and_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "affe_custom_variables_and_histories_preserve_defaults_metadata_and_values",
        r##"(list
               (mapcar
                (lambda (variable)
                  (list
                   variable
                   (symbol-value variable)
                   (get variable 'standard-value)
                   (get variable 'custom-type)
                   (assq variable
                         (get 'affe 'custom-group))
                   (get variable
                        'variable-documentation)))
                '(affe-count
                  affe-find-command
                  affe-grep-command
                  affe-regexp-compiler))
               (mapcar
                (lambda (variable)
                  (list
                   variable
                   (symbol-value variable)
                   (get variable 'standard-value)
                   (get variable 'custom-type)
                   (get variable
                        'variable-documentation)))
                '(affe--grep-history
                  affe--find-history)))"##,
        expect![[
            r#"OK (((affe-count 20 ((funcall #'#[nil (20) #1=(t)])) natnum (affe-count custom-variable) "Number of matches the backend should return.") (affe-find-command "rg --color=never --files" ((funcall #'#[nil ("rg --color=never --files") #1#])) string (affe-find-command custom-variable) "Find file command.") (affe-grep-command "rg --null --color=never --max-columns=1000 --no-heading --line-number -v ^$" ((funcall #'#[nil ("rg --null --color=never --max-columns=1000 --no-heading --line-number -v ^$") #1#])) string (affe-grep-command custom-variable) "Grep command.") (affe-regexp-compiler consult--default-regexp-compiler ((funcall #'#[nil (consult--regexp-compiler) #1#])) function (affe-regexp-compiler custom-variable) "Affe regular expression compiler.")) ((affe--grep-history nil nil nil nil) (affe--find-history nil nil nil nil)))"#
        ]],
    )
}

fn affe_function_surface_reports_arities_docs_and_interactive_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "affe_function_surface_reports_arities_docs_and_interactive_commands",
        r##"(mapcar
               (lambda (function)
                 (list
                  function
                  (help-function-arglist function t)
                  (documentation function)
                  (interactive-form function)
                  (file-name-nondirectory
                   (symbol-file function 'defun))))
               '(affe--connect
                 affe--send
                 affe--async
                 affe--command
                 affe-grep
                 affe-find))"##,
        expect![[
            r#"OK ((affe--connect (name callback) "Send EXPR to server NAME and call CALLBACK with result." nil "affe.el") (affe--send (proc expr) "Send EXPR to PROC." nil "affe.el") (affe--async (cmd &optional regexp) "Create asynchronous completion function.\nCMD is the backend command.\nREGEXP is the regexp which restricts the substring to match against." nil "affe.el") (affe--command (cmd paths) "Build command line argument list from CMD string and PATHS." nil "affe.el") (affe-grep (&optional dir initial) "Fuzzy grep in DIR with optional INITIAL input." (interactive "P") "affe.el") (affe-find (&optional dir initial) "Fuzzy find in DIR with optional INITIAL input." (interactive "P") "affe.el"))"#
        ]],
    )
}

fn affe_backend_surface_initializes_every_state_variable_hook_and_runtime_tuning() -> ParityBatchCase
{
    ParityBatchCase::value(
        "affe_backend_surface_initializes_every_state_variable_hook_and_runtime_tuning",
        r##"(list
               (featurep 'affe-backend)
               gc-cons-threshold
               gc-cons-percentage
               (memq #'affe-backend--setup
                     emacs-startup-hook)
               (eq affe-backend--search-head
                   affe-backend--search-tail)
               (eq affe-backend--producer-head
                   affe-backend--producer-tail)
               (list
                affe-backend--search-head
                affe-backend--search-found
                affe-backend--search-limit
                affe-backend--search-regexps
                affe-backend--producer-head
                affe-backend--producer-total
                affe-backend--producer-done
                affe-backend--producer-rest
                affe-backend--client-rest
                affe-backend--client
                affe-backend--restrict-regexp)
               (mapcar
                (lambda (function)
                  (list
                   function
                   (help-function-arglist function t)
                   (documentation function)))
                '(affe-backend--send
                  affe-backend--producer-filter
                  affe-backend--producer-sentinel
                  affe-backend--producer-start
                  affe-backend--server-filter
                  affe-backend--log
                  affe-backend--flush
                  affe-backend--producer-refresh
                  affe-backend--search-refresh
                  affe-backend--search-status
                  affe-backend--search-match-found
                  affe-backend--append-producer
                  affe-backend--search
                  affe-backend--setup)))"##,
        expect![[
            r#"OK (t 67108864 0.5 (affe-backend--setup) t t ((nil) 0 0 nil (nil) 0 nil "" "" nil nil) ((affe-backend--send (expr) "Send EXPR.") (affe-backend--producer-filter (_ out) "Process filter for the producer process receiving OUT string.") (affe-backend--producer-sentinel (_ status) "Sentinel for the producer process, receiving STATUS.") (affe-backend--producer-start (cmd) "Start backend CMD.") (affe-backend--server-filter (client out) "Server filter function receiving CLIENT and OUT string.") (affe-backend--log (&rest msg) "Send log message MSG.") (affe-backend--flush nil "Send a flush if no matching strings are found.") (affe-backend--producer-refresh nil "Refresh producer status.") (affe-backend--search-refresh nil "Refresh search.") (affe-backend--search-status nil "Send status to the CLIENT.") (affe-backend--search-match-found (match) "Called when matching string MATCH has been found.") (affe-backend--append-producer nil "Append producer list to search list.") (affe-backend--search nil "Search and send matching lines to client.") (affe-backend--setup nil "Setup backend server.")))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn surface_affe_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        affe_exact_pin_dependency_features_and_custom_group_match(),
        affe_custom_variables_and_histories_preserve_defaults_metadata_and_values(),
        affe_function_surface_reports_arities_docs_and_interactive_commands(),
    ]
}

pub(super) fn surface_affe_backend_batch_cases() -> Vec<ParityBatchCase> {
    vec![affe_backend_surface_initializes_every_state_variable_hook_and_runtime_tuning()]
}
