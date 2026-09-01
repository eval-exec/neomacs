use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, JS2_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const JS2_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const JS2_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'js2-mode)

(defun js2-test-cancel-parse-timer ()
  (when (and (boundp 'js2-mode-parse-timer)
             (timerp js2-mode-parse-timer))
    (cancel-timer js2-mode-parse-timer)
    (setq js2-mode-parse-timer nil)))

(defun js2-test-activate-and-parse ()
  (let ((js2-idle-timer-delay 3600))
    (js2-mode))
  (js2-test-cancel-parse-timer)
  (js2-reparse 'force)
  (js2-test-cancel-parse-timer))

(defun js2-test-print-tree (ast)
  (with-temp-buffer
    (js2-print-tree ast)
    (string-trim-right (buffer-string))))

(defun js2-test-ast-node-count (ast)
  (let ((count 0))
    (js2-visit-ast
     ast
     (lambda (_node end-p)
       (unless end-p (setq count (1+ count)))
       t))
    count))

(defun js2-test-diagnostics ()
  (mapcar
   (lambda (diagnostic)
     (let ((position (nth 1 diagnostic))
           (length (nth 2 diagnostic)))
       (list (js2-get-msg (car diagnostic))
             :line (line-number-at-pos position)
             :column (save-excursion
                       (goto-char position)
                       (current-column))
             :text
             (buffer-substring-no-properties
              position (min (point-max) (+ position length)))
             :face (nth 3 diagnostic))))
   (js2-errors-and-warnings)))

(defun js2-test-location ()
  (list :line (line-number-at-pos)
        :column (current-column)
        :symbol (thing-at-point 'symbol t)))

(defun js2-test-normalize-imenu (entries)
  (mapcar
   (lambda (entry)
     (let ((name (car entry))
           (value (cdr entry)))
       (cond
        ((number-or-marker-p value)
         (save-excursion
           (goto-char value)
           (list name :line (line-number-at-pos) :column (current-column))))
        ((listp value)
         (cons name (js2-test-normalize-imenu value)))
        (t (list name :value value)))))
   entries))

(defun js2-test-face-at (needle &optional offset)
  (goto-char (point-min))
  (search-forward needle)
  (let ((position (+ (- (point) (length needle)) (or offset 0))))
    (list needle
          (get-text-property position 'font-lock-face)
          (get-text-property position 'face))))
"##;

fn js2_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(JS2_MODE_MELPA_PIN, "js2-mode.el")
        .expect("prepare pinned js2-mode source below ./tmp")
        .with_prelude(JS2_MODE_TEST_PRELUDE)
        .with_timeout(JS2_MODE_TEST_TIMEOUT)
}

fn production_module_parses_round_trips_and_reports_broken_release_syntax() -> ParityBatchCase {
    let elisp_form = r##"
(let ((valid
       (concat
        "export async function deploy({service, retries = 3}, logger = console) {\n"
        "  const failures = [];\n"
        "  for (let attempt = 1; attempt <= retries; attempt += 1) {\n"
        "    try {\n"
        "      const result = await service.run?.();\n"
        "      return {...result, attempt};\n"
        "    } catch (error) {\n"
        "      failures.push(error.message ?? \"unknown\");\n"
        "      logger.warn(`retry ${attempt}: ${error.message}`);\n"
        "    }\n"
        "  }\n"
        "  throw new AggregateError(failures, \"deployment failed\");\n"
        "}\n"))
      (invalid
       "const release = { environment: \"production\", steps: [deploy(), };\n"))
  (list
   :valid
   (with-temp-buffer
     (insert valid)
     (js2-test-activate-and-parse)
     (list
      :mode major-mode
      :indent-function (eq indent-line-function #'js2-indent-line)
      :forward-function (eq forward-sexp-function #'js2-mode-forward-sexp)
      :dirty js2-mode-buffer-dirty-p
      :node-count (js2-test-ast-node-count js2-mode-ast)
      :diagnostics (js2-test-diagnostics)
      :printed (js2-test-print-tree js2-mode-ast)))
   :invalid
   (with-temp-buffer
     (insert invalid)
     (js2-test-activate-and-parse)
     (js2-test-diagnostics))))
"##;
    let expect = expect![[
        r##"OK (:valid (:mode js2-mode :indent-function t :forward-function t :dirty nil :node-count 80 :diagnostics (("Undeclared variable or function 'AggregateError'" :line 12 :column 12 :text "AggregateError" :face js2-external-variable)) :printed "export async function deploy({service, retries = 3}, logger = console) {\n    const failures = [];\n    for (let attempt = 1; attempt <= retries; attempt += 1) {\n        try {\n            const result = await service.run();\n            return {...result, attempt};\n        } catch (error) {\n            failures.push(error.message ?? \"unknown\");\n            logger.warn(`retry ${attempt}: ${error.message}`);\n        }\n    }\n    throw new AggregateError(failures, \"deployment failed\");\n}") :invalid (("missing } after property list" :line 1 :column 65 :text "\n" :face nil) ("missing ] after element list" :line 1 :column 52 :text "[" :face nil) ("syntax error" :line 1 :column 64 :text ";" :face nil) ("missing ] after element list" :line 1 :column 64 :text ";" :face nil) ("syntax error" :line 1 :column 63 :text "}" :face nil) ("Undeclared variable or function 'deploy'" :line 1 :column 53 :text "deploy" :face js2-external-variable) ("missing ; after statement" :line 2 :column 0 :text "" :face nil)))"##
    ]];
    ParityBatchCase::value(
        "production_module_parses_round_trips_and_reports_broken_release_syntax",
        elisp_form,
        expect,
    )
}

fn whole_file_indentation_formats_nested_async_control_flow_and_data() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert
   (concat
    "class Deployment {\n"
    "constructor(environment) {\n"
    "this.environment = environment;\n"
    "}\n"
    "async run(services) {\n"
    "for (const service of services) {\n"
    "if (service.enabled) {\n"
    "await deploy(service, {\n"
    "retries: 3,\n"
    "region: \"east\"\n"
    "});\n"
    "}\n"
    "}\n"
    "}\n"
    "}\n"
    "const plan = [\n"
    "{name: \"api\", enabled: true},\n"
    "{name: \"worker\", enabled: false}\n"
    "];\n"))
  (let ((js-indent-level 2)
        (js2-basic-offset 2)
        (js2-indent-switch-body t)
        (indent-tabs-mode nil))
    (js2-test-activate-and-parse)
    (indent-region (point-min) (point-max))
    (js2-test-cancel-parse-timer)
    (list :text (buffer-substring-no-properties (point-min) (point-max))
          :dirty-after-edit js2-mode-buffer-dirty-p
          :reparsed
          (progn
            (js2-reparse 'force)
            (js2-test-cancel-parse-timer)
            (list :dirty js2-mode-buffer-dirty-p
                  :errors (js2-errors))))))
"##;
    let expect = expect![[
        r##"OK (:text "class Deployment {\n  constructor(environment) {\n    this.environment = environment;\n  }\n  async run(services) {\n    for (const service of services) {\n      if (service.enabled) {\n        await deploy(service, {\n          retries: 3,\n          region: \"east\"\n        });\n      }\n    }\n  }\n}\nconst plan = [\n  {name: \"api\", enabled: true},\n  {name: \"worker\", enabled: false}\n];\n" :dirty-after-edit t :reparsed (:dirty nil :errors nil))"##
    ]];
    ParityBatchCase::value(
        "whole_file_indentation_formats_nested_async_control_flow_and_data",
        elisp_form,
        expect,
    )
}

fn ast_navigation_finds_scoped_definitions_and_marks_the_enclosing_method() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert
   (concat
    "const config = { production: { retries: 3 } };\n"
    "function deploy(service, options) {\n"
    "  return `${service}:${options.retries}`;\n"
    "}\n"
    "class Runner {\n"
    "  async run(service) {\n"
    "    return deploy(service, config.production);\n"
    "  }\n"
    "}\n"
    "const runner = new Runner();\n"
    "runner.run(\"api\");\n"))
  (js2-test-activate-and-parse)
  (let (function-jump parameter-jump object-jump defun-bounds sexp)
    (goto-char (point-min))
    (search-forward "return deploy")
    (search-backward "deploy")
    (js2-jump-to-definition)
    (setq function-jump (js2-test-location))

    (goto-char (point-min))
    (search-forward "`${service}")
    (search-backward "service")
    (js2-jump-to-definition)
    (setq parameter-jump (js2-test-location))

    (goto-char (point-min))
    (search-forward "config.production")
    (search-backward "production")
    (js2-jump-to-definition)
    (setq object-jump (js2-test-location))

    (goto-char (point-min))
    (search-forward "return deploy")
    (js2-mark-defun)
    (setq defun-bounds
          (list :start-line (line-number-at-pos (region-beginning))
                :end-line (line-number-at-pos (region-end))
                :text
                (buffer-substring-no-properties
                 (region-beginning) (region-end))))

    (deactivate-mark)
    (goto-char (point-min))
    (search-forward "const runner")
    (beginning-of-line)
    (let ((start (point)))
      (forward-sexp)
      (setq sexp
            (list :text (buffer-substring-no-properties start (point))
                  :end-line (line-number-at-pos))))
    (list :jumps (list function-jump parameter-jump object-jump)
          :defun defun-bounds
          :sexp sexp)))
"##;
    let expect = expect![[
        r##"OK (:jumps ((:line 2 :column 0 :symbol "function") (:line 2 :column 16 :symbol "service") (:line 1 :column 17 :symbol "production")) :defun (:start-line 6 :end-line 8 :text "async run(service) {\n    return deploy(service, config.production);\n  }") :sexp (:text "const runner = new Runner();" :end-line 10))"##
    ]];
    ParityBatchCase::value(
        "ast_navigation_finds_scoped_definitions_and_marks_the_enclosing_method",
        elisp_form,
        expect,
    )
}

fn json_inspector_reports_nested_paths_with_actual_and_hardcoded_array_indexes() -> ParityBatchCase
{
    let elisp_form = r##"
(with-temp-buffer
  (insert
   (concat
    "const deployment = {\n"
    "  environments: [\n"
    "    {name: \"staging\", services: []},\n"
    "    {name: \"production\", services: [\n"
    "      {name: \"api\", checks: [\"health\", \"latency\"]},\n"
    "      {name: \"worker\", checks: [\"queue-depth\"]}\n"
    "    ]}\n"
    "  ]\n"
    "};\n"
    "console.log(deployment);\n"))
  (js2-test-activate-and-parse)
  (let (latency worker object-use)
    (goto-char (point-min))
    (search-forward "latency")
    (setq latency
          (list :actual (js2-print-json-path)
                :zeroed (js2-print-json-path 0)
                :wildcard (js2-print-json-path "INDEX")))
    (goto-char (point-min))
    (search-forward "queue-depth")
    (setq worker (js2-print-json-path))
    (goto-char (point-min))
    (search-forward "console.log")
    (setq object-use (js2-print-json-path))
    (list :latency latency :worker worker :non-literal object-use)))
"##;
    let expect = expect![[
        r##"OK (:latency (:actual "environments[1].services[0].checks[1]" :zeroed "environments[0].services[0].checks[0]" :wildcard "environments[INDEX].services[INDEX].checks[INDEX]") :worker "environments[1].services[1].checks[0]" :non-literal nil)"##
    ]];
    ParityBatchCase::value(
        "json_inspector_reports_nested_paths_with_actual_and_hardcoded_array_indexes",
        elisp_form,
        expect,
    )
}

fn imenu_builds_a_navigable_index_for_functions_objects_and_classes() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert
   (concat
    "function validateRelease(release) { return release.id; }\n"
    "const deploy = async function deployRelease(release) {\n"
    "  return validateRelease(release);\n"
    "};\n"
    "const pipeline = {\n"
    "  prepare() { return true; },\n"
    "  execute: async function execute(release) { return deploy(release); },\n"
    "  recovery: { rollback() { return \"rolled-back\"; } }\n"
    "};\n"
    "class Coordinator {\n"
    "  start(release) { return pipeline.execute(release); }\n"
    "}\n"))
  (js2-test-activate-and-parse)
  (let ((index (js2-mode-create-imenu-index)))
    (list :function (eq imenu-create-index-function
                        #'js2-mode-create-imenu-index)
          :index (js2-test-normalize-imenu index))))
"##;
    let expect = expect![[
        r##"OK (:function t :index (("validateRelease" :line 1 :column 0) ("deploy" :line 2 :column 15) ("pipeline" ("prepare" :line 6 :column 2) ("execute" :line 7 :column 2) ("recovery" ("rollback" :line 8 :column 14))) ("Coordinator" ("start" :line 11 :column 2))))"##
    ]];
    ParityBatchCase::value(
        "imenu_builds_a_navigable_index_for_functions_objects_and_classes",
        elisp_form,
        expect,
    )
}

fn jsdoc_comment_continuation_and_code_folding_support_review_workflow() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert
   (concat
    "/**\n"
    " * Deploy a service release.\n"
    " * @param {string} service service identifier\n"
    " * @returns {Promise<string>} deployment identifier\n"
    " */\n"
    "async function deployService(service) {\n"
    "  // Validate the release before deployment\n"
    "  await validate(service);\n"
    "  return `deployed:${service}`;\n"
    "}\n"))
  (js2-test-activate-and-parse)
  (font-lock-ensure (point-min) (point-max))
  (let ((faces
         (mapcar
          (lambda (spec) (js2-test-face-at (car spec) (cdr spec)))
          '(("@param" . 0) ("{string}" . 1) ("service identifier" . 0)
            ("deployService" . 0) ("await" . 0)
            ("`deployed:${service}`" . 0))))
        continuation folded shown)
    (goto-char (point-min))
    (search-forward "Validate the release")
    (end-of-line)
    (js2-line-break)
    (insert "and record the audit event")
    (setq continuation
          (buffer-substring-no-properties
           (line-beginning-position 0) (line-end-position)))
    (js2-reparse 'force)
    (js2-test-cancel-parse-timer)
    (goto-char (point-min))
    (search-forward "await validate")
    (js2-mode-hide-element)
    (setq folded
          (let ((bounds (js2-mode-invisible-overlay-bounds)))
            (and bounds
                 (list :lines
                       (list (line-number-at-pos (car bounds))
                             (line-number-at-pos (cdr bounds)))
                       :invisible
                       (get-char-property (car bounds) 'invisible)))))
    (js2-mode-show-element)
    (setq shown (js2-mode-invisible-overlay-bounds))
    (list :faces faces
          :comment continuation
          :folded folded
          :shown shown)))
"##;
    let expect = expect![[
        r##"OK (:faces (("@param" js2-jsdoc-tag nil) ("{string}" js2-jsdoc-type nil) ("service identifier" font-lock-doc-face nil) ("deployService" font-lock-function-name-face nil) ("await" font-lock-keyword-face nil) ("`deployed:${service}`" font-lock-string-face nil)) :comment "  // Validate the release before deployment\n    // and record the audit event" :folded (:lines (6 11) :invisible js2-outline) :shown nil)"##
    ]];
    ParityBatchCase::value(
        "jsdoc_comment_continuation_and_code_folding_support_review_workflow",
        elisp_form,
        expect,
    )
}

#[test]
fn js2_mode_package_batch() {
    let cases = vec![
        production_module_parses_round_trips_and_reports_broken_release_syntax(),
        whole_file_indentation_formats_nested_async_control_flow_and_data(),
        ast_navigation_finds_scoped_definitions_and_marks_the_enclosing_method(),
        json_inspector_reports_nested_paths_with_actual_and_hardcoded_array_indexes(),
        imenu_builds_a_navigable_index_for_functions_objects_and_classes(),
        jsdoc_comment_continuation_and_code_folding_support_review_workflow(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed js2-mode parity test");
    assert_oracle_batch_cases(js2_mode_oracle(), test_name, "js2_mode_parity", &cases);
}
