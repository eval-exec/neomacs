use expect_test::expect;

use super::ParityBatchCase;

/// The workflow the back end exists for: open a C++ translation unit, type a
/// member access, and complete it against the rtags index.  The recorded
/// invocation is the package's whole contract with `rc', so the argument
/// vector is pinned in full — the `--code-complete-at' location is
/// `file:line:column:' built from point, so an off-by-one there is a failing
/// test, and because the user has been typing, the still-unsaved buffer has to
/// reach rc through `--unsaved-file'.  With two matches auto-complete expands
/// their common part inline; picking the second one runs `ac-rtags-action',
/// which parses the rtags signature and drops the user inside a freshly
/// inserted parameter list.
fn completes_a_member_function_and_expands_its_parameter_list() -> ParityBatchCase {
    ParityBatchCase::value(
        "completes_a_member_function_and_expands_its_parameter_list",
        r##"
        (progn
          (ac-rtags-test-write
           (expand-file-name "src/widget.h" ac-rtags-test-root)
           ac-rtags-test-widget-header)
          (ac-rtags-test-reply
           (ac-rtags-test-completions
            "widget.cpp:5:15:"
            '(("insert" "void insert(int idx, char ch)" "CXXMethod"
               "ui::Widget" "Insert one character.")
              ("insertAll" "void insertAll(const std::string &text, int idx)"
               "CXXMethod" "ui::Widget" "")
              ("label" "std::string label" "FieldDecl" "ui::Widget"
               "The visible caption."))))
          (ac-rtags-test-open
           "src/widget.cpp"
           (concat "#include \"widget.h\"\n"
                   "\n"
                   "int main() {\n"
                   "    ui::Widget widget;\n"
                   "    widget\n"
                   "    return 0;\n"
                   "}\n"))
          (goto-char (point-min))
          (forward-line 4)
          (end-of-line)
          (execute-kbd-macro ".ins")
          (list
           :typed (list :line (ac-rtags-test-line)
                        :point (point)
                        :modified (buffer-modified-p)
                        :ac-sources ac-sources
                        :invocations (ac-rtags-test-invocations))
           :offered (progn
                      (auto-complete)
                      (list :line (ac-rtags-test-line)
                            :point (point)
                            :ac-point ac-point
                            :ac-prefix (substring-no-properties ac-prefix)
                            :candidates
                            (ac-rtags-test-candidate-details ac-candidates)
                            :first-properties
                            (text-properties-at 0 (car ac-candidates))
                            :menu-live (ac-menu-live-p)
                            :menu (mapcar #'substring-no-properties
                                          (popup-list ac-menu))
                            :invocations (ac-rtags-test-invocations)))
           :completed (progn
                        (ac-next)
                        (ac-complete)
                        (list :line (ac-rtags-test-line)
                              :point (point)
                              :menu-live (ac-menu-live-p)
                              :last-completion (ac-rtags-test-last-completion)
                              :buffer (buffer-substring-no-properties
                                       (point-min) (point-max))))
           :recorded (ac-rtags-test-recorded)))
    "##,
        expect![[
            r##"OK (:typed (:line "    widget.ins" :point 72 :modified t :ac-sources (ac-source-rtags) :invocations 0) :offered (:line "    widget.insert" :point 75 :ac-point 69 :ac-prefix "insert" :candidates (("insert" "CXXMethod" "void insert(int idx, char ch)" "void insert(int idx, char ch)") ("insertAll" "CXXMethod" "void insertAll(const std::string &text, int idx)" "void insertAll(const std::string &text, int idx)")) :first-properties (action ac-rtags-action symbol "r" document ac-rtags-document ac-rtags-full "void insert(int idx, char ch)" ac-rtags-type "CXXMethod") :menu-live t :menu ("insert" "insertAll") :invocations 1) :completed (:line "    widget.insertAll(const std::string &text, int idx)" :point 79 :menu-live nil :last-completion ("insertAll" "CXXMethod" "void insertAll(const std::string &text, int idx)" 69) :buffer "#include \"widget.h\"\n\nint main() {\n    ui::Widget widget;\n    widget.insertAll(const std::string &text, int idx)\n    return 0;\n}\n") :recorded (("01-request" . "argv:\n  --current-file=[ORACLE-SANDBOX]/cpp/src/widget.cpp\n  --unsaved-file=[ORACLE-SANDBOX]/cpp/src/widget.cpp:<TEMPFILE>\n  -z\n  -t128\n  --code-complete-at\n  [ORACLE-SANDBOX]/cpp/src/widget.cpp:5:15:\n  --synchronous-completions\n  --elisp\ncwd: [ORACLE-SANDBOX]/cpp/src\nstdin: \nunsaved-file([ORACLE-SANDBOX]/cpp/src/widget.cpp):\n#include \"widget.h\"\n\nint main() {\n    ui::Widget widget;\n    widget.ins\n    return 0;\n}\n")))"##
        ]],
    )
}

fn appends_the_scope_operator_for_a_namespace_but_not_for_a_field() -> ParityBatchCase {
    ParityBatchCase::value(
        "appends_the_scope_operator_for_a_namespace_but_not_for_a_field",
        r##"
        (progn
          (ac-rtags-test-write
           (expand-file-name "src/widget.h" ac-rtags-test-root)
           ac-rtags-test-widget-header)
          (ac-rtags-test-reply
           (ac-rtags-test-completions
            "widget.cpp:5:15:"
            '(("label" "std::string label" "FieldDecl" "ui::Widget"
               "The visible caption.")))
           1)
          (ac-rtags-test-reply
           (ac-rtags-test-completions
            "widget.cpp:6:6:"
            '(("ui" "namespace ui" "Namespace" "" "")))
           2)
          (ac-rtags-test-open
           "src/widget.cpp"
           (concat "#include \"widget.h\"\n"
                   "\n"
                   "int main() {\n"
                   "    ui::Widget widget;\n"
                   "    widget.lab\n"
                   "    u\n"
                   "    return 0;\n"
                   "}\n"))
          (list
           :field (progn
                    (goto-char (point-min))
                    (forward-line 4)
                    (end-of-line)
                    (auto-complete)
                    (list :line (ac-rtags-test-line)
                          :point (point)
                          :last-completion (ac-rtags-test-last-completion)))
           :namespace (progn
                        (goto-char (point-min))
                        (forward-line 5)
                        (end-of-line)
                        (auto-complete)
                        (list :line (ac-rtags-test-line)
                              :point (point)
                              :last-completion (ac-rtags-test-last-completion)))
           :buffer (buffer-substring-no-properties (point-min) (point-max))
           :invocations (ac-rtags-test-invocations)
           :recorded (ac-rtags-test-recorded-argv)))
    "##,
        expect![[
            r##"OK (:field (:line "    widget.label" :point 74 :last-completion ("label" "FieldDecl" "std::string label" 69)) :namespace (:line "    ui::" :point 83 :last-completion ("ui" "Namespace" "namespace ui" 79)) :buffer "#include \"widget.h\"\n\nint main() {\n    ui::Widget widget;\n    widget.label\n    ui::\n    return 0;\n}\n" :invocations 2 :recorded (("01-request" "--current-file=[ORACLE-SANDBOX]/cpp/src/widget.cpp" "-z" "-t128" "--code-complete-at" "[ORACLE-SANDBOX]/cpp/src/widget.cpp:5:15:" "--synchronous-completions" "--elisp") ("02-request" "--current-file=[ORACLE-SANDBOX]/cpp/src/widget.cpp" "--unsaved-file=[ORACLE-SANDBOX]/cpp/src/widget.cpp:<TEMPFILE>" "-z" "-t128" "--code-complete-at" "[ORACLE-SANDBOX]/cpp/src/widget.cpp:6:6:" "--synchronous-completions" "--elisp")))"##
        ]],
    )
    .fresh_process()
}

fn keeps_the_bare_name_when_parameter_expansion_is_disabled() -> ParityBatchCase {
    ParityBatchCase::value(
        "keeps_the_bare_name_when_parameter_expansion_is_disabled",
        r##"
        (progn
          (ac-rtags-test-write
           (expand-file-name "src/widget.h" ac-rtags-test-root)
           ac-rtags-test-widget-header)
          (setq ac-rtags-expand-functions nil)
          (ac-rtags-test-reply
           (ac-rtags-test-completions
            "widget.cpp:5:15:"
            '(("insert" "void insert(int idx, char ch)" "CXXMethod"
               "ui::Widget" "Insert one character.")))
           1)
          (ac-rtags-test-reply
           (ac-rtags-test-completions
            "widget.cpp:6:6:"
            '(("ui" "namespace ui" "Namespace" "" "")))
           2)
          (ac-rtags-test-open
           "src/widget.cpp"
           (concat "#include \"widget.h\"\n"
                   "\n"
                   "int main() {\n"
                   "    ui::Widget widget;\n"
                   "    widget.ins\n"
                   "    u\n"
                   "    return 0;\n"
                   "}\n"))
          (list
           :option ac-rtags-expand-functions
           :method (progn
                     (goto-char (point-min))
                     (forward-line 4)
                     (end-of-line)
                     (auto-complete)
                     (list :line (ac-rtags-test-line)
                           :point (point)
                           :last-completion (ac-rtags-test-last-completion)))
           :namespace (progn
                        (goto-char (point-min))
                        (forward-line 5)
                        (end-of-line)
                        (auto-complete)
                        (list :line (ac-rtags-test-line)
                              :point (point)))
           :buffer (buffer-substring-no-properties (point-min) (point-max))
           :recorded (ac-rtags-test-recorded-argv)))
    "##,
        expect![[
            r##"OK (:option nil :method (:line "    widget.insert" :point 75 :last-completion ("insert" "CXXMethod" "void insert(int idx, char ch)" 69)) :namespace (:line "    ui::" :point 84) :buffer "#include \"widget.h\"\n\nint main() {\n    ui::Widget widget;\n    widget.insert\n    ui::\n    return 0;\n}\n" :recorded (("01-request" "--current-file=[ORACLE-SANDBOX]/cpp/src/widget.cpp" "-z" "-t128" "--code-complete-at" "[ORACLE-SANDBOX]/cpp/src/widget.cpp:5:15:" "--synchronous-completions" "--elisp") ("02-request" "--current-file=[ORACLE-SANDBOX]/cpp/src/widget.cpp" "--unsaved-file=[ORACLE-SANDBOX]/cpp/src/widget.cpp:<TEMPFILE>" "-z" "-t128" "--code-complete-at" "[ORACLE-SANDBOX]/cpp/src/widget.cpp:6:6:" "--synchronous-completions" "--elisp")))"##
        ]],
    )
    .fresh_process()
}

fn offers_nothing_when_rc_is_unindexed_silent_or_unparsable() -> ParityBatchCase {
    ParityBatchCase::value(
        "offers_nothing_when_rc_is_unindexed_silent_or_unparsable",
        r##"
        (progn
          (ac-rtags-test-reply "" 1 35)
          (ac-rtags-test-reply "" 2)
          (ac-rtags-test-reply
           "(list 'completions (list \"widget.cpp:5:15:\" (rtags-no-such-helper)))"
           3)
          (ac-rtags-test-open
           "src/widget.cpp"
           (concat "#include \"widget.h\"\n"
                   "\n"
                   "int main() {\n"
                   "    ui::Widget widget;\n"
                   "    widget.ins\n"
                   "    return 0;\n"
                   "}\n"))
          (goto-char (point-min))
          (forward-line 4)
          (end-of-line)
          (list :not-indexed (ac-rtags-test-attempt)
                :silent (ac-rtags-test-attempt)
                :unparsable (ac-rtags-test-attempt)
                :messages (ac-rtags-test-messages
                           "\\(not indexed\\|Completion Error\\)")
                :buffer (buffer-substring-no-properties (point-min) (point-max))
                :recorded (ac-rtags-test-recorded-argv)))
    "##,
        expect![[
            r##"OK (:not-indexed (:error completed :line "    widget.ins" :point 72 :candidates nil :not-indexed t :not-connected nil :invocations 1) :silent (:error completed :line "    widget.ins" :point 72 :candidates nil :not-indexed nil :not-connected nil :invocations 2) :unparsable (:error completed :line "    widget.ins" :point 72 :candidates nil :not-indexed nil :not-connected nil :invocations 3) :messages ("RTags: [ORACLE-SANDBOX]/cpp/src/widget.cpp is not indexed" "****** Got Completion Error ******") :buffer "#include \"widget.h\"\n\nint main() {\n    ui::Widget widget;\n    widget.ins\n    return 0;\n}\n" :recorded (("01-request" "--current-file=[ORACLE-SANDBOX]/cpp/src/widget.cpp" "-z" "-t128" "--code-complete-at" "[ORACLE-SANDBOX]/cpp/src/widget.cpp:5:15:" "--synchronous-completions" "--elisp") ("02-request" "--current-file=[ORACLE-SANDBOX]/cpp/src/widget.cpp" "-z" "-t128" "--code-complete-at" "[ORACLE-SANDBOX]/cpp/src/widget.cpp:5:15:" "--synchronous-completions" "--elisp") ("03-request" "--current-file=[ORACLE-SANDBOX]/cpp/src/widget.cpp" "-z" "-t128" "--code-complete-at" "[ORACLE-SANDBOX]/cpp/src/widget.cpp:5:15:" "--synchronous-completions" "--elisp")))"##
        ]],
    )
    .fresh_process()
}

fn signals_when_rdm_is_down_or_rc_is_not_installed() -> ParityBatchCase {
    ParityBatchCase::value(
        "signals_when_rdm_is_down_or_rc_is_not_installed",
        r##"
        (progn
          (ac-rtags-test-reply "" 1 36)
          (ac-rtags-test-reply
           (ac-rtags-test-completions
            "widget.cpp:5:15:"
            '(("insert" "void insert(int idx, char ch)" "CXXMethod"
               "ui::Widget" "Insert one character.")))
           2)
          (ac-rtags-test-open
           "src/widget.cpp"
           (concat "#include \"widget.h\"\n"
                   "\n"
                   "int main() {\n"
                   "    ui::Widget widget;\n"
                   "    widget.ins\n"
                   "    return 0;\n"
                   "}\n"))
          (goto-char (point-min))
          (forward-line 4)
          (end-of-line)
          (list :rdm-down (ac-rtags-test-attempt)
                :rc-missing (progn (ac-rtags-test-uninstall-rc)
                                   (ac-rtags-test-attempt))
                :recovered (progn (ac-rtags-test-install-rc)
                                  (ac-rtags-test-attempt))
                :line (ac-rtags-test-line)
                :buffer (buffer-substring-no-properties (point-min) (point-max))
                :recorded (ac-rtags-test-recorded-argv)))
    "##,
        expect![[
            r##"OK (:rdm-down (:error (error "RTags: Can’t seem to connect to server. Is rdm running?") :line "    widget.ins" :point 72 :candidates nil :not-indexed nil :not-connected t :invocations 1) :rc-missing (:error (error "RTags: Can’t find rc") :line "    widget.ins" :point 72 :candidates nil :not-indexed nil :not-connected t :invocations 1) :recovered (:error completed :line "    widget.insert(int idx, char ch)" :point 76 :candidates nil :not-indexed nil :not-connected nil :invocations 2) :line "    widget.insert(int idx, char ch)" :buffer "#include \"widget.h\"\n\nint main() {\n    ui::Widget widget;\n    widget.insert(int idx, char ch)\n    return 0;\n}\n" :recorded (("01-request" "--current-file=[ORACLE-SANDBOX]/cpp/src/widget.cpp" "-z" "-t128" "--code-complete-at" "[ORACLE-SANDBOX]/cpp/src/widget.cpp:5:15:" "--synchronous-completions" "--elisp") ("02-request" "--current-file=[ORACLE-SANDBOX]/cpp/src/widget.cpp" "-z" "-t128" "--code-complete-at" "[ORACLE-SANDBOX]/cpp/src/widget.cpp:5:15:" "--synchronous-completions" "--elisp")))"##
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        completes_a_member_function_and_expands_its_parameter_list(),
        appends_the_scope_operator_for_a_namespace_but_not_for_a_field(),
        keeps_the_bare_name_when_parameter_expansion_is_disabled(),
        offers_nothing_when_rc_is_unindexed_silent_or_unparsable(),
        signals_when_rdm_is_down_or_rc_is_not_installed(),
    ]
}
