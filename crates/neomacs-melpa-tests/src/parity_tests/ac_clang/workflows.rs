use expect_test::expect;

use super::ParityBatchCase;

/// The setup ac-clang documents: `(ac-clang-initialize)' finds and version
/// checks the server binary, launches it once per Emacs, and `ac-clang-mode'
/// then opens a per buffer session.  The recorded packets are the contract the
/// package has with the server binary, so they are pinned byte for byte:
/// the launch argument vector, the CXTranslationUnit/CXCodeComplete parameter
/// packet, and the CREATE_SESSION packet carrying the buffer's CFLAGS and
/// source code.  Turning the mode off must delete the session again and give
/// the user their previous `ac-sources' back.
fn starts_the_clang_server_and_opens_a_session_for_a_cxx_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "starts_the_clang_server_and_opens_a_session_for_a_cxx_buffer",
        r##"
        (ac-clang-test-workflow
          (progn
            (ac-clang-test-install-server)
            (list
             :initialize (ac-clang-initialize)
             :process (get-process "Clang-Server")
             :activated
             (progn
               (ac-clang-test-open
                "src/widget.cpp"
                (concat "#include \"widget.hpp\"\n"
                        "\n"
                        "int main() {\n"
                        "    ui::Widget widget;\n"
                        "    return widget.area(2);\n"
                        "}\n"))
               (setq clang-server-cflags '("-I../include" "-std=c++17"))
               (auto-complete-mode 1)
               (ac-clang-mode 1)
               (ac-clang-test-wait-records 4)
               (list :major-mode major-mode
                     :ac-clang-mode ac-clang-mode
                     :lighter (assq 'ac-clang-mode minor-mode-alist)
                     :ac-sources ac-sources
                     :session-buffers
                     (mapcar #'buffer-name clang-server-session-establishing-buffers)))
             :deactivated
             (progn
               (ac-clang-mode 0)
               (ac-clang-test-wait-records 5)
               (list :ac-clang-mode ac-clang-mode
                     :ac-sources ac-sources
                     :session-buffers clang-server-session-establishing-buffers))
             :recorded (ac-clang-test-recorded))))
    "##,
        expect![[
            r##"OK (:initialize t :process (:process "Clang-Server" run) :activated (:major-mode c++-mode :ac-clang-mode t :lighter (ac-clang-mode " ClangAssist") :ac-sources (ac-source-clang-async) :session-buffers ("widget.cpp")) :deactivated (:ac-clang-mode nil :ac-sources (ac-source-words-in-same-mode-buffers) :session-buffers nil) :recorded (("01-VERSION" . "clang-server version 2.1.3\n") ("02-LAUNCH" . "--input-data\ns-expression\n--output-data\ns-expression\n") ("03-SET_CLANG_PARAMETERS" packet-size-matches-body "(:RequestId 0 :CommandType \"Server\" :CommandName \"SET_CLANG_PARAMETERS\" :TranslationUnitFlags \"CXTranslationUnit_DetailedPreprocessingRecord|CXTranslationUnit_Incomplete|CXTranslationUnit_PrecompiledPreamble|CXTranslationUnit_CacheCompletionResults|CXTranslationUnit_IncludeBriefCommentsInCodeCompletion|CXTranslationUnit_CreatePreambleOnFirstParse\" :CompleteAtFlags \"CXCodeComplete_IncludeMacros|CXCodeComplete_IncludeCodePatterns|CXCodeComplete_IncludeBriefComments\" :CompleteResultsLimit 0)") ("04-CREATE_SESSION" packet-size-matches-body "(:RequestId 1 :CommandType \"Server\" :CommandName \"CREATE_SESSION\" :SessionName \"[ORACLE-SANDBOX]/clang/src/widget.cpp\" :CFLAGS (\"-cc1\" \"-fsyntax-only\" \"-x\" \"c++\" \"-I../include\" \"-std=c++17\") :SourceCode \"#include \\\"widget.hpp\\\"\n\nint main() {\n    ui::Widget widget;\n    return widget.area(2);\n}\n\")") ("05-DELETE_SESSION" packet-size-matches-body "(:RequestId 2 :CommandType \"Server\" :CommandName \"DELETE_SESSION\" :SessionName \"[ORACLE-SANDBOX]/clang/src/widget.cpp\")")))"##
        ]],
    )
}

fn completes_widget_members_after_typing_a_dot() -> ParityBatchCase {
    ParityBatchCase::value(
        "completes_widget_members_after_typing_a_dot",
        r##"
        (ac-clang-test-workflow
          (progn
            (ac-clang-test-install-server)
            (ac-clang-test-reply
             "COMPLETION"
             (concat
              "(:RequestId @REQUESTID@ :Results\n"
              " [(:Name \"area\" :Prototype \"[#int#]area(<#int scale#>) const\""
              " :BriefComment \"Area in device pixels.\")\n"
              "  (:Name \"area\""
              " :Prototype \"[#int#]area(<#int scale#>, <#bool rounded#>) const\")\n"
              "  (:Name \"label\" :Prototype \"[#const std::string &#]label() const\""
              " :BriefComment \"Human readable label.\")\n"
              "  (:Name \"resize\""
              " :Prototype \"[#void#]resize(<#int width#>, <#int height#>)\")\n"
              "  (:Name \"operator=\""
              " :Prototype \"[#ui::Widget &#]operator=(<#const ui::Widget &#>)\")\n"
              "  (:Name \"~Widget\" :Prototype \"[#void#]~Widget()\")])"))
            (ac-clang-initialize)
            (ac-clang-test-open
             "src/widget.cpp"
             (concat "#include \"widget.hpp\"\n"
                     "\n"
                     "int main() {\n"
                     "    ui::Widget widget;\n"
                     "    widget\n"
                     "    return 0;\n"
                     "}\n"))
            (auto-complete-mode 1)
            (ac-clang-mode 1)
            (ac-clang-test-wait-records 4)
            (goto-char (point-min))
            (forward-line 4)
            (end-of-line)
            (execute-kbd-macro ".")
            (ac-clang-test-wait (lambda () ac-candidates))
            (list :line (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position))
                  :point (point)
                  :ac-point ac-point
                  :ac-prefix ac-prefix
                  :candidates (ac-clang-test-candidate-details ac-candidates)
                  :first-candidate-properties
                  (text-properties-at 0 (car ac-candidates))
                  :menu-live (ac-menu-live-p)
                  :menu (mapcar #'substring-no-properties (popup-list ac-menu))
                  :selected (substring-no-properties (ac-selected-candidate))
                  :recorded (last (ac-clang-test-recorded)))))
    "##,
        expect![[
            r##"OK (:line "    widget." :point 71 :ac-point 71 :ac-prefix "" :candidates (("area" "[#int#]area(<#int scale#>) const\n[#int#]area(<#int scale#>, <#bool rounded#>) const" #1=(0 1)) ("label" "[#const std::string &#]label() const" (2)) ("resize" "[#void#]resize(<#int width#>, <#int height#>)" (3)) ("~Widget" "[#void#]~Widget()" (5)) ("operator=" "[#ui::Widget &#]operator=(<#const ui::Widget &#>)" (4))) :first-candidate-properties (action ac-clang--action symbol "c" document ac-clang--document popup-face ac-clang-candidate-face selection-face ac-clang-selection-face :detail "[#int#]area(<#int scale#>) const\n[#int#]area(<#int scale#>, <#bool rounded#>) const" :indices #1#) :menu-live t :menu ("area" "label" "resize" "~Widget" "operator=") :selected "area" :recorded (("05-COMPLETION" packet-size-matches-body "(:RequestId 2 :CommandType \"Session\" :CommandName \"COMPLETION\" :SessionName \"[ORACLE-SANDBOX]/clang/src/widget.cpp\" :Line 5 :Column 12 :SourceCode \"#include \\\"widget.hpp\\\"\n\nint main() {\n    ui::Widget widget;\n    widget.\n    return 0;\n}\n\")")))"##
        ]],
    )
    .fresh_process()
}

fn expands_the_chosen_overload_into_a_yasnippet_argument_template() -> ParityBatchCase {
    ParityBatchCase::value(
        "expands_the_chosen_overload_into_a_yasnippet_argument_template",
        r##"
        (ac-clang-test-workflow
          (progn
            (ac-clang-test-install-server)
            (ac-clang-test-reply
             "COMPLETION"
             (concat
              "(:RequestId @REQUESTID@ :Results\n"
              " [(:Name \"area\" :Prototype \"[#int#]area(<#int scale#>) const\""
              " :BriefComment \"Area in device pixels.\")\n"
              "  (:Name \"area\""
              " :Prototype \"[#int#]area(<#int scale#>, <#bool rounded#>) const\")\n"
              "  (:Name \"label\" :Prototype \"[#const std::string &#]label() const\""
              " :BriefComment \"Human readable label.\")])"))
            (ac-clang-initialize)
            (ac-clang-test-open
             "src/widget.cpp"
             (concat "#include \"widget.hpp\"\n"
                     "\n"
                     "int main() {\n"
                     "    ui::Widget widget;\n"
                     "    widget\n"
                     "    return 0;\n"
                     "}\n"))
            (auto-complete-mode 1)
            (yas-minor-mode 1)
            (ac-clang-mode 1)
            (ac-clang-test-wait-records 4)
            (goto-char (point-min))
            (forward-line 4)
            (end-of-line)
            (execute-kbd-macro ".")
            (ac-clang-test-wait (lambda () ac-candidates))
            (let ((overloads (ac-clang-test-candidate-details ac-candidates))
                  templates)
              (ac-complete)
              (setq templates
                    (list :line (buffer-substring-no-properties
                                 (line-beginning-position) (line-end-position))
                          :point (point)
                          :candidates
                          (mapcar (lambda (candidate)
                                    (list (substring-no-properties candidate)
                                          (get-text-property 0 :detail candidate)
                                          (get-text-property 0 :args candidate)
                                          (get-text-property 0 :indices candidate)))
                                  ac-candidates)))
              (ac-next)
              (ac-complete)
              (list :overloads overloads
                    :templates templates
                    :expanded (list :buffer (buffer-substring-no-properties
                                             (point-min) (point-max))
                                    :point (point)
                                    :snippets (length (yas-active-snippets)))
                    :next-field (progn (yas-next-field) (point))
                    :exited (progn (yas-exit-all-snippets)
                                   (list :buffer (buffer-substring-no-properties
                                                  (point-min) (point-max))
                                         :point (point)
                                         :snippets (length (yas-active-snippets))))
                    :recorded (last (ac-clang-test-recorded))))))
    "##,
        expect![[
            r##"OK (:overloads (("area" "[#int#]area(<#int scale#>) const\n[#int#]area(<#int scale#>, <#bool rounded#>) const" (0 1)) ("label" "[#const std::string &#]label() const" (2))) :templates (:line "    widget.area(int scale" :point 85 :candidates (("(int scale)" "int" "(<#int scale#>)" (0)) ("(int scale, bool rounded)" "int" "(<#int scale#>, <#bool rounded#>)" (1)))) :expanded (:buffer "#include \"widget.hpp\"\n\nint main() {\n    ui::Widget widget;\n    widget.area(int scale, bool rounded)\n    return 0;\n}\n" :point 76 :snippets 1) :next-field 87 :exited (:buffer "#include \"widget.hpp\"\n\nint main() {\n    ui::Widget widget;\n    widget.area(int scale, bool rounded)\n    return 0;\n}\n" :point 100 :snippets 0) :recorded (("05-COMPLETION" packet-size-matches-body "(:RequestId 2 :CommandType \"Session\" :CommandName \"COMPLETION\" :SessionName \"[ORACLE-SANDBOX]/clang/src/widget.cpp\" :Line 5 :Column 12 :SourceCode \"#include \\\"widget.hpp\\\"\n\nint main() {\n    ui::Widget widget;\n    widget.\n    return 0;\n}\n\")")))"##
        ]],
    )
    .fresh_process()
}

fn manual_tab_trigger_completes_a_prefix_on_a_non_ascii_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "manual_tab_trigger_completes_a_prefix_on_a_non_ascii_line",
        r##"
        (ac-clang-test-workflow
          (progn
            (ac-clang-test-install-server)
            (ac-clang-test-reply
             "COMPLETION"
             (concat
              "(:RequestId @REQUESTID@ :Results\n"
              " [(:Name \"area\" :Prototype \"[#int#]area(<#int scale#>) const\")\n"
              "  (:Name \"label\" :Prototype \"[#const std::string &#]label() const\""
              " :BriefComment \"Human readable label.\")\n"
              "  (:Name \"setLabel\""
              " :Prototype \"[#void#]setLabel(<#std::string text#>)\")])"))
            (ac-clang-initialize)
            (ac-clang-test-open
             "src/widget.cpp"
             (concat "#include \"widget.hpp\"\n"
                     "\n"
                     "int main() {\n"
                     "    ui::Widget widget;\n"
                     "    widget.setLabel(\"Größe – 図形\"); widget.la\n"
                     "    return 0;\n"
                     "}\n"))
            (auto-complete-mode 1)
            (yas-minor-mode 1)
            (ac-clang-mode 1)
            (ac-clang-test-wait-records 4)
            (goto-char (point-min))
            (forward-line 4)
            (end-of-line)
            (execute-kbd-macro (kbd "<tab>"))
            (ac-clang-test-wait
             (lambda ()
               (string-suffix-p "label()"
                                (buffer-substring-no-properties
                                 (line-beginning-position) (line-end-position)))))
            (list :line (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position))
                  :point (point)
                  :column (current-column)
                  :menu-live (ac-menu-live-p)
                  :snippets (length (yas-active-snippets))
                  :messages (ac-clang-test-messages "label")
                  :recorded (last (ac-clang-test-recorded)))))
    "##,
        expect![[
            r##"OK (:line "    widget.setLabel(\"Größe – 図形\"); widget.label()" :point 109 :column 51 :menu-live nil :snippets 0 :messages ("const std::string & label() const") :recorded (("05-COMPLETION" packet-size-matches-body "(:RequestId 2 :CommandType \"Session\" :CommandName \"COMPLETION\" :SessionName \"[ORACLE-SANDBOX]/clang/src/widget.cpp\" :Line 5 :Column 51 :SourceCode \"#include \\\"widget.hpp\\\"\n\nint main() {\n    ui::Widget widget;\n    widget.setLabel(\\\"Gr\\303\\266\\303\\237e \\342\\200\\223 \\345\\233\\263\\345\\275\\242\\\"); widget.la\n    return 0;\n}\n\")")))"##
        ]],
    )
    .fresh_process()
}

fn smart_jump_follows_a_definition_into_a_header_and_returns() -> ParityBatchCase {
    ParityBatchCase::value(
        "smart_jump_follows_a_definition_into_a_header_and_returns",
        r##"
        (ac-clang-test-workflow
          (let ((header (ac-clang-test-write
                         (expand-file-name "src/widget.hpp" ac-clang-test-root)
                         ac-clang-test-widget-header)))
            (ac-clang-test-install-server)
            (ac-clang-test-reply
             "SMARTJUMP"
             (format "(:RequestId @REQUESTID@ :Results (:Path \"%s\" :Line 7 :Column 9))"
                     header))
            (ac-clang-initialize)
            (ac-clang-test-open
             "src/widget.cpp"
             (concat "#include \"widget.hpp\"\n"
                     "\n"
                     "int main() {\n"
                     "    ui::Widget widget;\n"
                     "    return widget.area(2);\n"
                     "}\n"))
            (setq clang-server-cflags '("-I." "-std=c++17"))
            (auto-complete-mode 1)
            (ac-clang-mode 1)
            (ac-clang-test-wait-records 4)
            (goto-char (point-min))
            (forward-line 4)
            (search-forward "area")
            (goto-char (match-beginning 0))
            (ac-clang-jump-smart)
            (ac-clang-test-wait (lambda () (get-file-buffer header)))
            (list :jumped
                  (append (ac-clang-test-here)
                          (with-current-buffer (window-buffer (selected-window))
                            (list :major-mode major-mode
                                  :cflags clang-server-cflags
                                  :ac-clang-mode ac-clang-mode)))
                  :returned (progn (set-buffer (window-buffer (selected-window)))
                                   (ac-clang-jump-back)
                                   (ac-clang-test-here))
                  :stack-exhausted (progn (set-buffer (window-buffer (selected-window)))
                                          (ac-clang-jump-back)
                                          (ac-clang-test-here))
                  :recorded (last (ac-clang-test-recorded)))))
    "##,
        expect![[
            r##"OK (:jumped (:window-buffer "widget.hpp" :line 7 :column 8 :text "    int area(int scale) const;" :major-mode c++-mode :cflags ("-I." "-std=c++17") :ac-clang-mode nil) :returned (:window-buffer "widget.cpp" :line 5 :column 18 :text "    return widget.area(2);") :stack-exhausted (:window-buffer "widget.cpp" :line 5 :column 18 :text "    return widget.area(2);") :recorded (("05-SMARTJUMP" packet-size-matches-body "(:RequestId 2 :CommandType \"Session\" :CommandName \"SMARTJUMP\" :SessionName \"[ORACLE-SANDBOX]/clang/src/widget.cpp\" :Line 5 :Column 19 :SourceCode \"#include \\\"widget.hpp\\\"\n\nint main() {\n    ui::Widget widget;\n    return widget.area(2);\n}\n\")")))"##
        ]],
    )
    .fresh_process()
}

fn refuses_to_start_when_the_server_binary_is_missing_or_too_old() -> ParityBatchCase {
    ParityBatchCase::value(
        "refuses_to_start_when_the_server_binary_is_missing_or_too_old",
        r##"
        (ac-clang-test-workflow
          (list
           :missing (progn
                      (ac-clang-test-use-bin "empty")
                      (list :found (executable-find "clang-server")
                            :initialize (ac-clang-initialize)
                            :process (get-process "Clang-Server")))
           :too-old (progn
                      (ac-clang-test-install-server "1.9.7" "bin-old")
                      (list :initialize (ac-clang-initialize)
                            :process (get-process "Clang-Server")))
           :current (progn
                      (ac-clang-test-install-server)
                      (list :initialize (ac-clang-initialize)
                            :process (get-process "Clang-Server")))
           :warnings (progn (ac-clang-test-wait-records 4)
                            (ac-clang-test-warnings))
           :recorded (ac-clang-test-recorded)))
    "##,
        expect![[
            r#"OK (:missing (:found nil :initialize nil :process nil) :too-old (:initialize nil :process nil) :current (:initialize t :process (:process "Clang-Server" run)) :warnings ("Warning (clang-server): clang-server binary not found." "Warning (clang-server): clang-server binary is old. please replace new binary. require version is (2 0 0) over.") :recorded (("01-VERSION" . "clang-server version 1.9.7\n") ("02-VERSION" . "clang-server version 2.1.3\n") ("03-LAUNCH" . "--input-data\ns-expression\n--output-data\ns-expression\n") ("04-SET_CLANG_PARAMETERS" packet-size-matches-body "(:RequestId 0 :CommandType \"Server\" :CommandName \"SET_CLANG_PARAMETERS\" :TranslationUnitFlags \"CXTranslationUnit_DetailedPreprocessingRecord|CXTranslationUnit_Incomplete|CXTranslationUnit_PrecompiledPreamble|CXTranslationUnit_CacheCompletionResults|CXTranslationUnit_IncludeBriefCommentsInCodeCompletion|CXTranslationUnit_CreatePreambleOnFirstParse\" :CompleteAtFlags \"CXCodeComplete_IncludeMacros|CXCodeComplete_IncludeCodePatterns|CXCodeComplete_IncludeBriefComments\" :CompleteResultsLimit 0)")))"#
        ]],
    )
    .fresh_process()
}

fn reports_a_server_command_error_and_completes_on_the_retry() -> ParityBatchCase {
    ParityBatchCase::value(
        "reports_a_server_command_error_and_completes_on_the_retry",
        r##"
        (ac-clang-test-workflow
          (progn
            (ac-clang-test-install-server)
            (ac-clang-test-reply
             "COMPLETION.1"
             "(:RequestId @REQUESTID@ :Error \"CREATE_SESSION is not completed yet.\")")
            (ac-clang-test-reply
             "COMPLETION.2"
             (concat
              "(:RequestId @REQUESTID@ :Results\n"
              " [(:Name \"area\" :Prototype \"[#int#]area(<#int scale#>) const\")\n"
              "  (:Name \"label\""
              " :Prototype \"[#const std::string &#]label() const\")])"))
            (ac-clang-initialize)
            (ac-clang-test-open
             "src/widget.cpp"
             (concat "#include \"widget.hpp\"\n"
                     "\n"
                     "int main() {\n"
                     "    ui::Widget widget;\n"
                     "    widget\n"
                     "    return 0;\n"
                     "}\n"))
            (auto-complete-mode 1)
            (ac-clang-mode 1)
            (ac-clang-test-wait-records 4)
            (goto-char (point-min))
            (forward-line 4)
            (end-of-line)
            (execute-kbd-macro ".")
            (ac-clang-test-wait (lambda () (ac-clang-test-messages "clang-server :")))
            (let ((failed (list :line (buffer-substring-no-properties
                                       (line-beginning-position) (line-end-position))
                                :point (point)
                                :menu-live (ac-menu-live-p)
                                :candidates ac-candidates)))
              (execute-kbd-macro (kbd "<tab>"))
              (ac-clang-test-wait (lambda () ac-candidates))
              (list :failed failed
                    :retried (list :line (buffer-substring-no-properties
                                          (line-beginning-position)
                                          (line-end-position))
                                   :point (point)
                                   :menu-live (ac-menu-live-p)
                                   :candidates
                                   (ac-clang-test-candidate-details ac-candidates))
                    :messages (ac-clang-test-messages "clang-server :")
                    :recorded (last (ac-clang-test-recorded) 2)))))
    "##,
        expect![[
            r##"OK (:failed (:line "    widget." :point 71 :menu-live nil :candidates nil) :retried (:line "    widget." :point 71 :menu-live t :candidates (("area" "[#int#]area(<#int scale#>) const" (0)) ("label" "[#const std::string &#]label() const" (1)))) :messages ("clang-server : server command error! : CREATE_SESSION is not completed yet.") :recorded (("05-COMPLETION" packet-size-matches-body "(:RequestId 2 :CommandType \"Session\" :CommandName \"COMPLETION\" :SessionName \"[ORACLE-SANDBOX]/clang/src/widget.cpp\" :Line 5 :Column 12 :SourceCode \"#include \\\"widget.hpp\\\"\n\nint main() {\n    ui::Widget widget;\n    widget.\n    return 0;\n}\n\")") ("06-COMPLETION" packet-size-matches-body "(:RequestId 3 :CommandType \"Session\" :CommandName \"COMPLETION\" :SessionName \"[ORACLE-SANDBOX]/clang/src/widget.cpp\" :Line 5 :Column 12 :SourceCode \"#include \\\"widget.hpp\\\"\n\nint main() {\n    ui::Widget widget;\n    widget.\n    return 0;\n}\n\")")))"##
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        starts_the_clang_server_and_opens_a_session_for_a_cxx_buffer(),
        completes_widget_members_after_typing_a_dot(),
        expands_the_chosen_overload_into_a_yasnippet_argument_template(),
        manual_tab_trigger_completes_a_prefix_on_a_non_ascii_line(),
        smart_jump_follows_a_definition_into_a_header_and_returns(),
        refuses_to_start_when_the_server_binary_is_missing_or_too_old(),
        reports_a_server_command_error_and_completes_on_the_retry(),
    ]
}
