use expect_test::expect;

use super::ParityBatchCase;

fn repeated_elisp_command_cycles_a_real_deployment_call_and_returns_to_one_line() -> ParityBatchCase
{
    let elisp_form = r####"
(with-temp-buffer
  (neomacs-multi-line-test-prepare
   'emacs-lisp-mode
   "(configure-deployment \"payments-service\" :regions '(\"iad\" \"fra\") :timeout 30 :notify t)"
   "payments-service"
   52)
  (neomacs-multi-line-test-with-restored-cycle
    (let (states)
      (dotimes (_ 4)
        (execute-kbd-macro (kbd "C-c d"))
        (push (neomacs-multi-line-test-buffer-state) states))
      (nreverse states))))
"####;
    let expect = expect![[
        r####"OK ((:text "(configure-deployment \"payments-service\"\n                      :regions '(\"iad\" \"fra\")\n                      :timeout 30 :notify t)" :point 40 :line 1 :column 39 :modified t) (:text "(configure-deployment \"payments-service\" :regions '(\"iad\" \"fra\") :timeout 30 :notify t)" :point 40 :line 1 :column 39 :modified t) (:text "(configure-deployment \"payments-service\"\n                      :regions '(\"iad\" \"fra\")\n                      :timeout 30 :notify t)" :point 40 :line 1 :column 39 :modified t) (:text "(configure-deployment \"payments-service\" :regions '(\"iad\" \"fra\") :timeout 30 :notify t)" :point 40 :line 1 :column 39 :modified t))"####
    ]];
    ParityBatchCase::value(
        "repeated_elisp_command_cycles_a_real_deployment_call_and_returns_to_one_line",
        elisp_form,
        expect,
    )
}

fn python_cycle_manages_trailing_commas_and_prefix_single_lines_the_call() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (neomacs-multi-line-test-prepare
   'python-mode
   "deploy_service(\"payments\", regions=[\"iad\", \"fra\"], retries=3, notify=True)"
   "payments"
   44)
  (neomacs-multi-line-test-with-restored-cycle
    (let (states)
      (dotimes (_ 5)
        (execute-kbd-macro (kbd "C-c d"))
        (push (neomacs-multi-line-test-buffer-state) states))
      (execute-kbd-macro (kbd "C-u C-c d"))
      (let ((single-lined (neomacs-multi-line-test-buffer-state)))
        (goto-char (point-max))
        (insert "\ncleanup_service(\"reports\", regions=[\"iad\", \"fra\"], notify=False)")
        (search-backward "reports")
        (execute-kbd-macro (kbd "C-c d"))
        (list :cycled (nreverse states)
              :single-lined single-lined
              :new-expression (neomacs-multi-line-test-buffer-state))))))
"####;
    let expect = expect![[
        r####"OK (:cycled ((:text "deploy_service(\n    \"payments\", regions=[\"iad\", \"fra\"],\n    retries=3, notify=True,\n)" :point 30 :line 2 :column 13 :modified t) (:text "deploy_service(\n    \"payments\",\n    regions=[\"iad\", \"fra\"],\n    retries=3,\n    notify=True,\n)" :point 30 :line 2 :column 13 :modified t) (:text "deploy_service(\"payments\",\n               regions=[\"iad\", \"fra\"],\n               retries=3, notify=True)" :point 25 :line 1 :column 24 :modified t) (:text "deploy_service(\"payments\", regions=[\"iad\", \"fra\"], retries=3, notify=True)" :point 25 :line 1 :column 24 :modified t) (:text "deploy_service(\n    \"payments\", regions=[\"iad\", \"fra\"],\n    retries=3, notify=True,\n)" :point 30 :line 2 :column 13 :modified t)) :single-lined (:text "deploy_service(\"payments\", regions=[\"iad\", \"fra\"], retries=3, notify=True)" :point 25 :line 1 :column 24 :modified t) :new-expression (:text "deploy_service(\"payments\", regions=[\"iad\", \"fra\"], retries=3, notify=True)\ncleanup_service(\n    \"reports\", regions=[\"iad\", \"fra\"],\n    notify=False,\n)" :point 98 :line 3 :column 5 :modified t))"####
    ]];
    ParityBatchCase::value(
        "python_cycle_manages_trailing_commas_and_prefix_single_lines_the_call",
        elisp_form,
        expect,
    )
}

fn ruby_hash_command_wraps_entries_and_single_line_command_restores_source() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (neomacs-multi-line-test-prepare
   'ruby-mode
   "deploy({region: \"iad\", service: \"payments\", retries: 3, notify: true})"
   "service"
   48)
  (neomacs-multi-line-test-with-restored-cycle
    (execute-kbd-macro (kbd "C-c d"))
    (let ((wrapped (neomacs-multi-line-test-buffer-state)))
      (call-interactively #'multi-line-single-line)
      (list :wrapped wrapped
            :single-lined (neomacs-multi-line-test-buffer-state)))))
"####;
    let expect = expect![[
        r####"OK (:wrapped (:text "deploy({\n         region: \"iad\", service: \"payments\",\n         retries: 3, notify: true,\n       })" :point 41 :line 2 :column 31 :modified t) :single-lined (:text "deploy({region: \"iad\", service: \"payments\", retries: 3, notify: true})" :point 31 :line 1 :column 30 :modified t))"####
    ]];
    ParityBatchCase::value(
        "ruby_hash_command_wraps_entries_and_single_line_command_restores_source",
        elisp_form,
        expect,
    )
}

fn nested_quoted_elisp_formats_only_the_selected_inner_expression() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (neomacs-multi-line-test-prepare
   'emacs-lisp-mode
   "(let ((plan '((service . \"api\")\n              (regions . (\"iad\" \"fra\"))\n              (retry . 3))))\n  (submit-plan plan :dry-run nil :notify t :deployment-window \"nightly\"))"
   "submit-plan"
   50)
  (neomacs-multi-line-test-with-restored-cycle
    (execute-kbd-macro (kbd "C-c d"))
    (neomacs-multi-line-test-buffer-state)))
"####;
    let expect = expect![[
        r####"OK (:text "(let ((plan '((service . \"api\")\n              (regions . (\"iad\" \"fra\"))\n              (retry . 3))))\n  (submit-plan plan :dry-run nil :notify t\n               :deployment-window \"nightly\"))" :point 116 :line 4 :column 14 :modified t)"####
    ]];
    ParityBatchCase::value(
        "nested_quoted_elisp_formats_only_the_selected_inner_expression",
        elisp_form,
        expect,
    )
}

fn clojure_keyword_pairs_and_go_trailing_commas_follow_their_real_mode_strategies()
-> ParityBatchCase {
    let elisp_form = r####"
(list
 :clojure
 (neomacs-multi-line-test-format-once
  'clojure-mode
  "(deploy-release :service \"api\" :regions [\"iad\" \"fra\"] :retry 3 :notify true)"
  "service"
  46)
 :go
 (neomacs-multi-line-test-format-once
  'go-mode
  "func deployRelease(service string, regions []string, retries int, notify bool) error { return nil }"
  "regions"
  52))
"####;
    let expect = expect![[
        r####"OK (:clojure (:text "(deploy-release :service \"api\"\n                :regions [\"iad\" \"fra\"] :retry 3\n                :notify true)" :point 25 :line 1 :column 24 :modified t) :go (:text "func deployRelease(\n        service string, regions []string,\n        retries int, notify bool,\n) error { return nil }" :point 52 :line 2 :column 31 :modified t))"####
    ]];
    ParityBatchCase::value(
        "clojure_keyword_pairs_and_go_trailing_commas_follow_their_real_mode_strategies",
        elisp_form,
        expect,
    )
}

fn haskell_leading_comma_strategy_formats_a_selected_region_list() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-multi-line-test-format-once
 'haskell-mode
 "regions = [\"us-east-1\", \"eu-west-1\", \"ap-southeast-1\"]"
 "eu-west-1"
 34)
"####;
    let expect = expect![[
        r####"OK (:text "regions = [\n  \"us-east-1\"\n  , \"eu-west-1\"\n  , \"ap-southeast-1\"\n  ]" :point 41 :line 3 :column 14 :modified t)"####
    ]];
    ParityBatchCase::value(
        "haskell_leading_comma_strategy_formats_a_selected_region_list",
        elisp_form,
        expect,
    )
}

fn advertised_c_cpp_java_and_rust_definition_routes_use_real_mode_indentation() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :c
 (neomacs-multi-line-test-format-once
  'c-mode
  "int deploy_release(const char *service, int retries, bool notify) { return retries; }"
  "retries"
  44)
 :cpp
 (neomacs-multi-line-test-format-once
  'c++-mode
  "std::array<const char *, 3> regions = {\"us-east-1\", \"eu-west-1\", \"ap-southeast-1\"};"
  "eu-west-1"
  44)
 :java
 (neomacs-multi-line-test-format-once
  'java-mode
  "void deployRelease(String service, int retries, boolean notify) { schedule(service); }"
  "retries"
  44)
 :rust
 (neomacs-multi-line-test-format-once
  'rust-mode
  "fn deploy_release(service: &str, retries: u32, notify: bool) -> Result<(), Error> { Ok(()) }"
  "retries"
  44))
"####;
    let expect = expect![[
        r####"OK (:c (:text "int deploy_release(\n                   const char *service,\n                   int retries, bool notify\n                  ) { return retries; }" :point 91 :line 3 :column 30 :modified t) :cpp (:text "std::array<const char *, 3> regions = {\n  \"us-east-1\", \"eu-west-1\", \"ap-southeast-1\"\n};" :point 66 :line 2 :column 25 :modified t) :java (:text "void deployRelease(\n                   String service,\n                   int retries,\n                   boolean notify\n                  ) { schedule(service); }" :point 86 :line 3 :column 30 :modified t) :rust (:text "fn deploy_release(\n    service: &str, retries: u32, notify: bool\n) -> Result<(), Error> { Ok(()) }" :point 46 :line 2 :column 26 :modified t))"####
    ]];
    ParityBatchCase::value(
        "advertised_c_cpp_java_and_rust_definition_routes_use_real_mode_indentation",
        elisp_form,
        expect,
    )
}

fn javascript_array_and_scala_definition_routes_format_selected_constructs() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :javascript
 (neomacs-multi-line-test-format-once
  'js-mode
  "const regions = [\"us-east-1\", \"eu-west-1\", \"ap-southeast-1\"];"
  "eu-west-1"
  38)
 :scala
 (neomacs-multi-line-test-format-once
  'scala-mode
  "def deployRelease(service: String, retries: Int, notify: Boolean): Unit = schedule(service)"
  "retries"
  46))
"####;
    let expect = expect![[
        r####"OK (:javascript (:text "const regions = [\n    \"us-east-1\", \"eu-west-1\",\n    \"ap-southeast-1\"\n];" :point 46 :line 2 :column 27 :modified t) :scala (:text "def deployRelease(\n  service: String, retries: Int,\n  notify: Boolean\n): Unit = schedule(service)" :point 46 :line 2 :column 26 :modified t))"####
    ]];
    ParityBatchCase::value(
        "javascript_array_and_scala_definition_routes_format_selected_constructs",
        elisp_form,
        expect,
    )
}

fn documented_defhook_installs_a_buffer_local_custom_strategy_for_a_derived_mode() -> ParityBatchCase
{
    let elisp_form = r####"
(progn
  (define-derived-mode neomacs-multi-line-test-mode
    emacs-lisp-mode "Multi-Line-Test")
  (multi-line-defhook neomacs-multi-line-test
    (make-instance
     'multi-line-strategy
     :find multi-line-lisp-find-strategy
     :enter (make-instance 'multi-line-up-list-enter-strategy
                           :skip-chars "`',@")
     :respace
     (multi-line-default-respacers
      (multi-line-clearing-reindenting-respacer
       (make-instance 'multi-line-always-newline)))))
  (list
   :custom
   (neomacs-multi-line-test-format-once
    'neomacs-multi-line-test-mode
    "(deploy-release service regions retries notify)"
    "regions"
    200)
   :ordinary-elisp
   (neomacs-multi-line-test-format-once
    'emacs-lisp-mode
    "(deploy-release service regions retries notify)"
    "regions"
    200)))
"####;
    let expect = expect![[
        r####"OK (:custom (:text "(\n deploy-release\n service\n regions\n retries\n notify\n )" :point 36 :line 4 :column 8 :modified t) :ordinary-elisp (:text "(deploy-release service regions retries notify)" :point 32 :line 1 :column 31 :modified t))"####
    ]];
    ParityBatchCase::value(
        "documented_defhook_installs_a_buffer_local_custom_strategy_for_a_derived_mode",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn public_highlight_commands_show_every_split_candidate_then_clear_overlays() -> ParityBatchCase {
    let elisp_form = r####"
(let ((multi-line-overlays-to-remove nil))
  (with-temp-buffer
    (neomacs-multi-line-test-prepare
     'emacs-lisp-mode
     "(release-build :service \"api\" :region \"iad\" :retry 3)"
     "service"
     42)
    (call-interactively #'multi-line-highlight-current-candidates)
    (let ((highlighted
           (mapcar
            (lambda (overlay)
              (list :start (overlay-start overlay)
                    :end (overlay-end overlay)
                    :text (buffer-substring-no-properties
                           (overlay-start overlay) (overlay-end overlay))
                    :face (overlay-get overlay 'face)
                    :buffer (eq (overlay-buffer overlay) (current-buffer))))
            (sort (copy-sequence multi-line-overlays-to-remove)
                  (lambda (left right)
                    (< (overlay-start left) (overlay-start right)))))))
      (call-interactively #'multi-line-clear-highlights)
      (list :source (buffer-substring-no-properties (point-min) (point-max))
            :highlighted highlighted
            :remaining (length (overlays-in (point-min) (point-max)))))))
"####;
    let expect = expect![[
        r####"OK (:source "(release-build :service \"api\" :region \"iad\" :retry 3)" :highlighted ((:start 1 :end 2 :text "(" :face highlight :buffer t) (:start 14 :end 15 :text "d" :face highlight :buffer t) (:start 29 :end 30 :text "\"" :face highlight :buffer t) (:start 43 :end 44 :text "\"" :face highlight :buffer t) (:start 52 :end 53 :text "3" :face highlight :buffer t)) :remaining 0)"####
    ]];
    ParityBatchCase::value(
        "public_highlight_commands_show_every_split_candidate_then_clear_overlays",
        elisp_form,
        expect,
    )
}

fn disabling_and_reenabling_mode_hooks_changes_new_python_buffers() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (call-interactively #'multi-line-disable-mode-hooks)
  (let ((without-language-strategy
         (with-temp-buffer
           (neomacs-multi-line-test-prepare
            'python-mode
            "deploy_service(\"payments\", region=\"iad\", retry=3)"
            "payments"
            38)
           (neomacs-multi-line-test-with-restored-cycle
             (execute-kbd-macro (kbd "C-c d"))
             (neomacs-multi-line-test-buffer-state)))))
    (call-interactively #'multi-line-enable-mode-hooks)
    (let ((with-language-strategy
           (with-temp-buffer
             (neomacs-multi-line-test-prepare
              'python-mode
              "deploy_service(\"payments\", region=\"iad\", retry=3)"
              "payments"
              38)
             (neomacs-multi-line-test-with-restored-cycle
               (execute-kbd-macro (kbd "C-c d"))
               (neomacs-multi-line-test-buffer-state)))))
      (list :disabled without-language-strategy
            :reenabled with-language-strategy))))
"####;
    let expect = expect![[
        r####"OK (:disabled (:text "deploy_service(\n    \"payments\", region=\"iad\", retry=3\n)" :point 30 :line 2 :column 13 :modified t) :reenabled (:text "deploy_service(\n    \"payments\", region=\"iad\", retry=3,\n)" :point 30 :line 2 :column 13 :modified t))"####
    ]];
    ParityBatchCase::value(
        "disabling_and_reenabling_mode_hooks_changes_new_python_buffers",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn malformed_expression_surfaces_the_real_structural_scan_error() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (neomacs-multi-line-test-prepare
   'emacs-lisp-mode
   "(deploy-release :service \"api\" :regions '(\"iad\" \"fra\")"
   "service"
   48)
  (set-buffer-modified-p nil)
  (condition-case error-data
      (call-interactively #'multi-line)
    (error
     (list :error error-data
           :source (buffer-substring-no-properties (point-min) (point-max))
           :point (point)
           :modified (buffer-modified-p)))))
"####;
    let expect = expect![[
        r####"OK (:error (scan-error "Unbalanced parentheses" 25 55) :source "(deploy-release :service \"api\" :regions '(\"iad\" \"fra\")" :point 25 :modified nil)"####
    ]];
    ParityBatchCase::value(
        "malformed_expression_surfaces_the_real_structural_scan_error",
        elisp_form,
        expect,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        repeated_elisp_command_cycles_a_real_deployment_call_and_returns_to_one_line(),
        python_cycle_manages_trailing_commas_and_prefix_single_lines_the_call(),
        ruby_hash_command_wraps_entries_and_single_line_command_restores_source(),
        nested_quoted_elisp_formats_only_the_selected_inner_expression(),
        clojure_keyword_pairs_and_go_trailing_commas_follow_their_real_mode_strategies(),
        haskell_leading_comma_strategy_formats_a_selected_region_list(),
        advertised_c_cpp_java_and_rust_definition_routes_use_real_mode_indentation(),
        javascript_array_and_scala_definition_routes_format_selected_constructs(),
        documented_defhook_installs_a_buffer_local_custom_strategy_for_a_derived_mode(),
        public_highlight_commands_show_every_split_candidate_then_clear_overlays(),
        disabling_and_reenabling_mode_hooks_changes_new_python_buffers(),
        malformed_expression_surfaces_the_real_structural_scan_error(),
    ]
}
