use expect_test::expect;

use super::ParityBatchCase;

/// The primary completion workflow: after `os.get' the server reply is
/// stored in `jedi:complete-reply', converted into auto-complete popup
/// items by `jedi:ac-direct-matches', and `jedi:ac-setup' registers the
/// source and enables auto-complete-mode.
fn completion_after_os_dot_get_flows_into_the_ac_source() -> ParityBatchCase {
    ParityBatchCase::value(
        "completion_after_os_dot_get_flows_into_the_ac_source",
        r####"(unwind-protect
    (progn
      (jedi--test-open "app.py" (jedi--test-app-c))
      (with-current-buffer "app.py"
        (jedi-mode 1)
        (jedi:start-server)
        (jedi--test-at 20 16) ; after "os.get"
        (deferred:sync! (jedi:complete-request))
        (let ((items (jedi:ac-direct-matches))
              (ac-sources-before ac-sources))
          (jedi:ac-setup)
          (jedi--test-result
           :source (jedi--test-source-state)
           :reply-words (mapcar (lambda (x) (plist-get x :word))
                                jedi:complete-reply)
           :reply-count (length jedi:complete-reply)
           :first-item (list :word (popup-item-property (car items) 'popup-item-word)
                             :symbol (popup-item-property (car items) 'symbol)
                             :summary (popup-item-property (car items) 'summary))
           :last-item-word (popup-item-property (car (last items))
                                                'popup-item-word)
           :ac-source-added (memq 'ac-source-jedi-direct ac-sources)
           :ac-source-not-duplicated
           (equal ac-sources-before
                  (delete 'ac-source-jedi-direct ac-sources))
           :auto-complete-mode auto-complete-mode))))
  (jedi--test-reset))"####,
        expect![[
            r#"OK (:source (:upstream-tree "bc79acd486975a713095f2de438777906d001350" :feature t :version "20250602.2107") :reply-words ("get_blocking" "get_exec_path" "get_handle_inheritable" "get_inheritable" "get_terminal_size" "getcwd" "getcwdb" "getegid" "getenv" "getenvb" "geteuid" "getgid" "getgrouplist" "getgroups" "getloadavg" "getlogin" "getpgid" "getpgrp" "getpid" "getppid" "getpriority" "getrandom" "getresgid" "getresuid" "getsid" "getuid" "getxattr") :reply-count 27 :first-item (:word nil :symbol "function" :summary "def get_blocking") :last-item-word nil :ac-source-added (ac-source-jedi-direct ac-source-words-in-same-mode-buffers) :ac-source-not-duplicated t :auto-complete-mode t :messages nil :calls "(call 4 complete (\"import os\\12\\12def greet(name):\\12    \\\"\\\"\\\"Return a friendly greeting.\\\"\\\"\\\"\\12    return \\\"hello \\\" + name\\12\\12class Counter(object):\\12    \\\"\\\"\\\"A simple counter.\\\"\\\"\\\"\\12    def __init__(self, start=0):\\12        self.value = start\\12\\12    def increment(self):\\12        self.value += 1\\12        return self.value\\12\\12\\12def main():\\12    print(greet(\\\"world\\\"))\\12    c = Counter()\\12    c.increment()\\12    print(os.getcwd())\\12\" 21 16 \"@@ROOT@@/jedi-fixtures/app.py\"))\n\n")"#
        ]],
    )
}

/// Completion after `c.' resolves to the project-local method through the
/// same public path.
fn completion_after_c_dot_resolves_the_local_method() -> ParityBatchCase {
    ParityBatchCase::value(
        "completion_after_c_dot_resolves_the_local_method",
        r####"(unwind-protect
    (progn
      (jedi--test-open "app.py" (jedi--test-app-c))
      (with-current-buffer "app.py"
        (jedi-mode 1)
        (jedi:start-server)
        (jedi--test-at 19 7) ; after "c."
        (deferred:sync! (jedi:complete-request))
        (jedi--test-result
         :reply jedi:complete-reply
         :request-point jedi:complete-request-point
         :point (point))))
  (jedi--test-reset))"####,
        expect![[
            r#"OK (:reply ((:word "increment" :doc "increment()" :description "def increment" :symbol "function")) :request-point 347 :point 347 :messages nil :calls "(call 7 complete (\"import os\\12\\12def greet(name):\\12    \\\"\\\"\\\"Return a friendly greeting.\\\"\\\"\\\"\\12    return \\\"hello \\\" + name\\12\\12class Counter(object):\\12    \\\"\\\"\\\"A simple counter.\\\"\\\"\\\"\\12    def __init__(self, start=0):\\12        self.value = start\\12\\12    def increment(self):\\12        self.value += 1\\12        return self.value\\12\\12\\12def main():\\12    print(greet(\\\"world\\\"))\\12    c = Counter()\\12    c.increment()\\12    print(os.getcwd())\\12\" 20 7 \"@@ROOT@@/jedi-fixtures/app.py\"))\n\n")"#
        ]],
    )
}

/// `jedi:goto-definition' jumps to the recorded definition location and
/// `jedi:goto-definition-pop-marker' returns to the call site.
fn goto_definition_jumps_to_the_function_and_pops_back() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_definition_jumps_to_the_function_and_pops_back",
        r####"(unwind-protect
    (progn
      (jedi--test-open "app.py" (jedi--test-app-c))
      (with-current-buffer "app.py"
        (jedi-mode 1)
        (jedi:start-server)
        (jedi--test-at 17 11) ; on greet
        (let ((call-site (point)))
          (jedi:goto-definition-push-marker)
          (jedi:goto-definition)
          (jedi--test-pump)
          (let ((definition-site
                 (list :line (line-number-at-pos)
                       :column (- (point) (line-beginning-position))
                       :text (buffer-substring-no-properties
                              (line-beginning-position)
                              (line-end-position)))))
            (jedi:goto-definition-pop-marker)
            (jedi--test-result
             :definition-site definition-site
             :returned (and (= (point) call-site)
                            (eq (current-buffer) (get-buffer "app.py"))))))))
  (jedi--test-reset))"####,
        expect![[
            r#"OK (:definition-site (:line 3 :column 4 :text "def greet(name):") :returned t :messages nil :calls "(call 10 goto (\"import os\\12\\12def greet(name):\\12    \\\"\\\"\\\"Return a friendly greeting.\\\"\\\"\\\"\\12    return \\\"hello \\\" + name\\12\\12class Counter(object):\\12    \\\"\\\"\\\"A simple counter.\\\"\\\"\\\"\\12    def __init__(self, start=0):\\12        self.value = start\\12\\12    def increment(self):\\12        self.value += 1\\12        return self.value\\12\\12\\12def main():\\12    print(greet(\\\"world\\\"))\\12    c = Counter()\\12    c.increment()\\12    print(os.getcwd())\\12\" 18 11 \"@@ROOT@@/jedi-fixtures/app.py\"))\n\n")"#
        ]],
    )
}

/// `jedi:show-doc' renders the recorded docstring in *jedi:doc*.
fn show_doc_renders_the_recorded_docstring() -> ParityBatchCase {
    ParityBatchCase::value(
        "show_doc_renders_the_recorded_docstring",
        r####"(unwind-protect
    (progn
      (jedi--test-open "app.py" (jedi--test-app-c))
      (with-current-buffer "app.py"
        (jedi-mode 1)
        (jedi:start-server)
        (jedi--test-at 18 9) ; on Counter
        (jedi:show-doc)
        (jedi--test-pump)
        (jedi--test-result
         :doc-buffer
         (with-current-buffer (get-buffer jedi:doc-buffer-name)
           (buffer-substring-no-properties (point-min) (point-max))))))
  (jedi--test-reset))"####,
        expect![[
            r#"OK (:doc-buffer "Docstring for __main__.Counter\n\nCounter(start=0)\n\nA simple counter." :messages nil :calls "(call 13 get_definition (\"import os\\12\\12def greet(name):\\12    \\\"\\\"\\\"Return a friendly greeting.\\\"\\\"\\\"\\12    return \\\"hello \\\" + name\\12\\12class Counter(object):\\12    \\\"\\\"\\\"A simple counter.\\\"\\\"\\\"\\12    def __init__(self, start=0):\\12        self.value = start\\12\\12    def increment(self):\\12        self.value += 1\\12        return self.value\\12\\12\\12def main():\\12    print(greet(\\\"world\\\"))\\12    c = Counter()\\12    c.increment()\\12    print(os.getcwd())\\12\" 19 9 \"@@ROOT@@/jedi-fixtures/app.py\"))\n\n")"#
        ]],
    )
}

/// `jedi:get-in-function-call' shows the recorded call signature with the
/// current argument highlighted.
/// `jedi:get-in-function-call' shows the recorded call signature with the
/// current argument highlighted.
fn get_in_function_call_shows_the_signature_with_the_argument_highlight() -> ParityBatchCase {
    ParityBatchCase::value(
        "get_in_function_call_shows_the_signature_with_the_argument_highlight",
        r####"(unwind-protect
    (progn
      (jedi--test-open "app.py" (jedi--test-app-c))
      (with-current-buffer "app.py"
        (jedi-mode 1)
        (jedi:start-server)
        (jedi--test-at 17 16) ; inside greet(
        (setq jedi:tooltip-method nil) ; echo-area fallback, documented
        (jedi--test-with-message-capture
         (jedi:get-in-function-call)
         (jedi--test-pump)
         (let ((msg (car jedi--test-messages)))
           (jedi--test-result
            :signature (substring-no-properties msg)
            :argument-face (get-text-property 6 'face msg))))))
  (jedi--test-reset))"####,
        expect![[
            r#"OK (:signature "greet(param name)" :argument-face jedi:highlight-function-argument :messages (#("greet(param name)" 6 16 (face jedi:highlight-function-argument))) :calls "(call 16 get_in_function_call (\"import os\\12\\12def greet(name):\\12    \\\"\\\"\\\"Return a friendly greeting.\\\"\\\"\\\"\\12    return \\\"hello \\\" + name\\12\\12class Counter(object):\\12    \\\"\\\"\\\"A simple counter.\\\"\\\"\\\"\\12    def __init__(self, start=0):\\12        self.value = start\\12\\12    def increment(self):\\12        self.value += 1\\12        return self.value\\12\\12\\12def main():\\12    print(greet(\\\"world\\\"))\\12    c = Counter()\\12    c.increment()\\12    print(os.getcwd())\\12\" 18 16 \"@@ROOT@@/jedi-fixtures/app.py\"))\n\n")"#
        ]],
    )
}

/// The defined-names reply becomes the nested imenu index through the
/// public conversion.
fn defined_names_build_the_nested_imenu_index() -> ParityBatchCase {
    ParityBatchCase::value(
        "defined_names_build_the_nested_imenu_index",
        r####"(unwind-protect
    (progn
      (jedi--test-open "names.py" (jedi--test-names-py))
      (with-current-buffer "names.py"
        (jedi-mode 1)
        (jedi:start-server)
        (deferred:sync! (jedi:defined-names-deferred))
        (let ((index (jedi:create-nested-imenu-index)))
          (jedi--test-result
           :cache-names
           (mapcar (lambda (x) (plist-get (car x) :local_name))
                   jedi:defined-names--cache)
           :index-shape
           (mapcar
            (lambda (entry)
              (if (consp (cdr entry))
                  (list :name (car entry)
                        :children (mapcar #'car (cddr entry))
                        :line (line-number-at-pos (cdr (cadr entry))))
                (list :name (car entry)
                      :line (line-number-at-pos (cdr entry)))))
            index)))))
  (jedi--test-reset))"####,
        expect![[
            r#"OK (:cache-names ("greet" "Counter") :index-shape ((:name "greet" :line 1) (:name "Counter" :children ("__init__" "increment") :line 5)) :messages nil :calls "(call 19 defined_names (\"def greet(name):\\12    \\\"\\\"\\\"Return a friendly greeting.\\\"\\\"\\\"\\12    return \\\"hello \\\" + name\\12\\12class Counter(object):\\12    \\\"\\\"\\\"A simple counter.\\\"\\\"\\\"\\12    def __init__(self, start=0):\\12        self.value = start\\12\\12    def increment(self):\\12        self.value += 1\\12        return self.value\\12\" \"@@ROOT@@/jedi-fixtures/names.py\"))\n\n")"#
        ]],
    )
}

/// The server lifecycle: stop without a server reports it, start brings a
/// live connection, stop kills it, and a broken server command degrades
/// with the documented warning, disables the mode, and -- through the
/// package's own failure path -- signals on the nil manager.
fn the_server_lifecycle_and_the_broken_command_warning() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_server_lifecycle_and_the_broken_command_warning",
        r####"(unwind-protect
    (progn
      (jedi--test-open "app.py" (jedi--test-app-c))
      (with-current-buffer "app.py"
        (jedi-mode 1)
        (jedi--test-with-message-capture
         (jedi:stop-server)
         (let ((stopped-message (car jedi--test-messages)))
           (setq jedi--test-messages nil)
           (jedi:start-server)
           (let ((live (jedi:epc--live-p jedi:epc)))
             (jedi:stop-server)
             (setq jedi:server-command
                   (list (expand-file-name "no-such-jedi-python"
                                           jedi--test-root)
                         "jediepcserver.py"))
             (let* ((warnings-before
                     (with-current-buffer
                         (get-buffer-create "*Warnings*")
                       (buffer-string)))
                    (bad-start-error nil))
               (condition-case err
                   (jedi:start-server)
                 (error (setq bad-start-error
                              (list (car err) (cadr err)))))
               (jedi--test-result
                :stopped-message stopped-message
                :live-after-start live
                :live-after-stop (and jedi:epc
                                      (jedi:epc--live-p jedi:epc))
                :bad-start-error bad-start-error
                :warnings
                (let* ((text
                        (with-current-buffer
                            (get-buffer-create "*Warnings*")
                          (buffer-substring-no-properties
                           (point-min) (point-max))))
                       (tail (substring text (length warnings-before))))
                  (jedi--test-normalize-editor tail))
                :mode jedi-mode)))))))
  (jedi--test-reset))"####,
        expect![[
            r#"OK (:stopped-message "Jedi server is already killed." :live-after-start (open listen connect stop) :live-after-stop nil :bad-start-error (wrong-type-argument epc:manager) :warnings "Error (jedi): \n================================\nFailed to start Jedi EPC server.\n================================\n\n*** EPC Error ***\nServer may raise an error. Use \"M-x epc:pop-to-last-server-process-buffer RET\" to see full traceback:\n@@EMACS@@: [ORACLE-SANDBOX]/no-such-jedi-python: No such file or directory\n\nProcess epc:server:22 exited abnormally with code 127\n\n\n*** EPC Server Output (last 10 lines) ***\n@@EMACS@@: [ORACLE-SANDBOX]/no-such-jedi-python: No such file or directory\n\nProcess epc:server:22 exited abnormally with code 127\n\n\n*** EPC Server Config ***\nServer arguments: (\"[ORACLE-SANDBOX]/no-such-jedi-python\" \"jediepcserver.py\")\nActual command: nil (\"[ORACLE-SANDBOX]/no-such-jedi-python\" not found in exec-path)\nVIRTUAL_ENV envvar: nil\n\n*** jedi-mode is disabled in #<buffer app.py> ***\nFix the problem and re-enable it.\n\n*** You may need to run \"M-x jedi:install-server\". ***\nThis could solve the problem especially if you haven't run the command yet\nsince Jedi.el installation or update and if the server complains about\nPython module imports.\n" :mode nil :messages ("Error (jedi): \n================================\nFailed to start Jedi EPC server.\n================================\n\n*** EPC Error ***\nServer may raise an error. Use \"M-x epc:pop-to-last-server-process-buffer RET\" to see full traceback:\n@@EMACS@@: [ORACLE-SANDBOX]/no-such-jedi-python: No such file or directory\n\nProcess epc:server:22 exited abnormally with code 127\n\n\n*** EPC Server Output (last 10 lines) ***\n@@EMACS@@: [ORACLE-SANDBOX]/no-such-jedi-python: No such file or directory\n\nProcess epc:server:22 exited abnormally with code 127\n\n\n*** EPC Server Config ***\nServer arguments: (\"[ORACLE-SANDBOX]/no-such-jedi-python\" \"jediepcserver.py\")\nActual command: nil (\"[ORACLE-SANDBOX]/no-such-jedi-python\" not found in exec-path)\nVIRTUAL_ENV envvar: nil\n\n*** jedi-mode is disabled in #<buffer app.py> ***\nFix the problem and re-enable it.\n\n*** You may need to run \"M-x jedi:install-server\". ***\nThis could solve the problem especially if you haven't run the command yet\nsince Jedi.el installation or update and if the server complains about\nPython module imports.") :calls "")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        completion_after_os_dot_get_flows_into_the_ac_source(),
        completion_after_c_dot_resolves_the_local_method(),
        goto_definition_jumps_to_the_function_and_pops_back(),
        show_doc_renders_the_recorded_docstring(),
        get_in_function_call_shows_the_signature_with_the_argument_highlight(),
        defined_names_build_the_nested_imenu_index(),
        the_server_lifecycle_and_the_broken_command_warning(),
    ]
}
