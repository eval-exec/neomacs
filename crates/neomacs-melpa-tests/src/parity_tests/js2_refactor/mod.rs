use std::time::Duration;

use expect_test::expect;

use crate::{
    CachedMelpaOracle, DASH_MELPA_PIN, JS2_MODE_MELPA_PIN, JS2_REFACTOR_MELPA_PIN,
    MULTIPLE_CURSORS_MELPA_PIN, S_MELPA_PIN, YASNIPPET_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const JS2_REFACTOR_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const JS2_REFACTOR_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'js2-mode)
(require 'js2-refactor)

(defvar js2r-test-replacement nil)

(defun js2r-test-replace-active-name ()
  (interactive)
  (when (mark t)
    (delete-region (point) (mark t)))
  (insert js2r-test-replacement))

(defun js2r-test-cancel-parse-timer ()
  (when (and (boundp 'js2-mode-parse-timer)
             (timerp js2-mode-parse-timer))
    (cancel-timer js2-mode-parse-timer)
    (setq js2-mode-parse-timer nil)))

(defun js2r-test-reset (source)
  (js2r-test-cancel-parse-timer)
  (fundamental-mode)
  (erase-buffer)
  (insert source)
  (goto-char (point-min))
  (js2-mode)
  (setq-local indent-tabs-mode nil
              js-indent-level 2
              js2-basic-offset 2
              js2-idle-timer-delay 3600)
  (js2r-test-cancel-parse-timer)
  (js2-reparse 'force)
  (let ((yas-dont-activate-functions nil))
    (js2-refactor-mode 1))
  (js2r-test-cancel-parse-timer))

(defun js2r-test-source ()
  (js2r-test-cancel-parse-timer)
  (buffer-substring-no-properties (point-min) (point-max)))

(defun js2r-test-reparse-summary ()
  (js2-reparse 'force)
  (js2r-test-cancel-parse-timer)
  (list :dirty js2-mode-buffer-dirty-p
        :errors (mapcar #'js2-get-msg
                        (mapcar #'car (js2-errors)))
        :warnings (mapcar #'js2-get-msg
                          (mapcar #'car (js2-warnings)))))
"##;

fn js2_refactor_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(JS2_REFACTOR_MELPA_PIN, "js2-refactor.el")
        .expect("prepare pinned js2-refactor source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned Dash dependency below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s dependency below ./tmp")
        .with_melpa_dependency(MULTIPLE_CURSORS_MELPA_PIN)
        .expect("prepare pinned multiple-cursors dependency below ./tmp")
        .with_melpa_dependency(YASNIPPET_MELPA_PIN)
        .expect("prepare pinned YASnippet dependency below ./tmp")
        .with_melpa_dependency(JS2_MODE_MELPA_PIN)
        .expect("prepare pinned js2-mode dependency below ./tmp")
        .with_prelude(JS2_REFACTOR_TEST_PRELUDE)
        .with_timeout(JS2_REFACTOR_TEST_TIMEOUT)
}

fn signature_migration_rewrites_definition_body_and_every_local_callsite() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (js2r-test-reset
   (concat
    "function schedule(service, retries, options) {\n"
    "  return queue(service, retries, options.region);\n"
    "}\n"
    "schedule(\"api\", 3, {region: \"east\"});\n"
    "schedule(workerName, retryBudget, productionOptions);\n"))
  (goto-char (point-min))
  (search-forward "schedule")
  (js2r-arguments-to-object)
  (list :mode (list js2-refactor-mode yas-minor-mode)
        :source (js2r-test-source)
        :parse (js2r-test-reparse-summary)))
"##;
    let expect = expect![[
        r##"OK (:mode (t t) :source "function schedule(params) {\n  return queue(params.service, params.retries, params.options.region);\n}\nschedule({\n  service: \"api\",\n  retries: 3,\n  options: {region: \"east\"}\n});\nschedule({\n  service: workerName,\n  retries: retryBudget,\n  options: productionOptions\n});\n" :parse (:dirty nil :errors nil :warnings ("Undeclared variable or function 'queue'" "Undeclared variable or function 'workerName'" "Undeclared variable or function 'retryBudget'" "Undeclared variable or function 'productionOptions'")))"##
    ]];
    ParityBatchCase::value(
        "signature_migration_rewrites_definition_body_and_every_local_callsite",
        elisp_form,
        expect,
    )
}

fn scope_aware_rename_updates_outer_bindings_without_touching_keys_properties_or_shadowing()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((delete-selection-was-enabled delete-selection-mode)
        source cursors)
    (unwind-protect
        (progn
          (delete-selection-mode 1)
          (js2r-test-reset
           (concat
            "const release = {id: 417, status: \"queued\"};\n"
            "function deploy(release) { return release.id; }\n"
            "function audit() { return release.id; }\n"
            "const payload = {release: release, status: release.status};\n"))
          (goto-char (point-min))
          (search-forward "release")
          (backward-word)
          (js2r-rename-var)
          (setq cursors (mc/num-cursors))
          (let ((js2r-test-replacement "deployment"))
            (mc/execute-command-for-all-cursors
             #'js2r-test-replace-active-name))
          (setq source (js2r-test-source)))
      (when (bound-and-true-p multiple-cursors-mode)
        (multiple-cursors-mode 0))
      (unless delete-selection-was-enabled
        (delete-selection-mode -1)))
    (list :cursors cursors :source source)))
"##;
    let expect = expect![[
        r##"OK (:cursors 4 :source "const deployment = {id: 417, status: \"queued\"};\nfunction deploy(release) { return release.id; }\nfunction audit() { return deployment.id; }\nconst payload = {release: deployment, status: deployment.status};\n")"##
    ]];
    ParityBatchCase::value(
        "scope_aware_rename_updates_outer_bindings_without_touching_keys_properties_or_shadowing",
        elisp_form,
        expect,
    )
}

fn inline_and_instance_rewrites_preserve_scope_and_remove_obsolete_declarations() -> ParityBatchCase
{
    let elisp_form = r##"
(with-temp-buffer
  (let (inlined instance)
    (js2r-test-reset
     (concat
      "const baseUrl = \"https://deploy.example/\";\n"
      "function requestRelease(service) {\n"
      "  const endpoint = baseUrl + service;\n"
      "  return request(endpoint).then(() => audit(endpoint));\n"
      "}\n"))
    (goto-char (point-min))
    (search-forward "const endpoint")
    (search-backward "endpoint")
    (js2r-inline-var)
    (setq inlined
          (list :source (js2r-test-source)
                :parse (js2r-test-reparse-summary)))

    (js2r-test-reset
     (concat
      "function Deployment(retryBudget) {\n"
      "  let attempts = retryBudget + 1;\n"
      "  this.run = function () {\n"
      "    return attempts > 1 ? attempts : retryBudget;\n"
      "  };\n"
      "}\n"))
    (goto-char (point-min))
    (search-forward "let attempts")
    (js2r-var-to-this)
    (setq instance
          (list :source (js2r-test-source)
                :parse (js2r-test-reparse-summary)))
    (list :inline inlined :instance instance)))
"##;
    let expect = expect![[
        r##"OK (:inline (:source "const baseUrl = \"https://deploy.example/\";\nfunction requestRelease(service) {\n  return request(baseUrl + service).then(() => audit(baseUrl + service));\n}\n" :parse (:dirty nil :errors nil :warnings ("Undeclared variable or function 'request'" "Undeclared variable or function 'audit'"))) :instance (:source "function Deployment(retryBudget) {\n  this.attempts = retryBudget + 1;\n  this.run = function () {\n    return this.attempts > 1 ? this.attempts : retryBudget;\n  };\n}\n" :parse (:dirty nil :errors nil :warnings nil)))"##
    ]];
    ParityBatchCase::value(
        "inline_and_instance_rewrites_preserve_scope_and_remove_obsolete_declarations",
        elisp_form,
        expect,
    )
}

fn extraction_and_conditional_rewrites_create_reusable_control_flow() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let (extracted conditional)
    (js2r-test-reset
     (concat
      "function calculate(order, tax) {\n"
      "  const subtotal = order.items.reduce((sum, item) => sum + item.price, 0);\n"
      "  const total = subtotal * (1 + tax);\n"
      "  return formatCurrency(total);\n"
      "}\n"))
    (goto-char (point-min))
    (search-forward "const subtotal")
    (beginning-of-line)
    (let ((start (point)))
      (search-forward "const total")
      (end-of-line)
      (set-mark start)
      (activate-mark)
      (js2r-extract-function "computeTotal"))
    (setq extracted
          (list :source (js2r-test-source)
                :parse (js2r-test-reparse-summary)))

    (js2r-test-reset
     (concat
      "function route(environment, release) {\n"
      "  return publish(environment === \"production\" ? release : preview(release));\n"
      "}\n"))
    (goto-char (point-min))
    (search-forward "?")
    (js2r-ternary-to-if)
    (setq conditional
          (list :source (js2r-test-source)
                :parse (js2r-test-reparse-summary)))
    (list :extracted extracted :conditional conditional)))
"##;
    let expect = expect![[
        r##"OK (:extracted (:source "function computeTotal(tax, order) {\n  const subtotal = order.items.reduce((sum, item) => sum + item.price, 0);\n  const total = subtotal * (1 + tax);\n  return total;\n}\n\nfunction calculate(order, tax) {\n  var total = computeTotal(tax, order);\n  return formatCurrency(total);\n}\n" :parse (:dirty nil :errors nil :warnings ("Undeclared variable or function 'formatCurrency'"))) :conditional (:source "function route(environment, release) {\n  if (environment === \"production\") {\n    return publish(release);\n  } else {\n    return publish(preview(release));\n  }\n}\n" :parse (:dirty nil :errors nil :warnings ("Undeclared variable or function 'publish'" "Undeclared variable or function 'publish'" "Undeclared variable or function 'preview'"))))"##
    ]];
    ParityBatchCase::value(
        "extraction_and_conditional_rewrites_create_reusable_control_flow",
        elisp_form,
        expect,
    )
}

fn iife_workflow_wraps_strict_code_injects_global_alias_and_unwraps() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (js2r-test-reset
   (concat
    "const client = window.releaseClient;\n"
    "client.deploy(window.releaseId);\n"))
  (let ((js2r-iife-style 'lambda)
        (js2r-use-strict t)
        (js2r-prefered-quote-type 2)
        wrapped injected unwrapped)
    (js2r-wrap-buffer-in-iife)
    (goto-char (point-min))
    (search-forward "window.releaseClient")
    (setq wrapped (js2r-test-source))
    (js2r-add-global-to-iife "window" "root")
    (setq injected (js2r-test-source))
    (goto-char (point-min))
    (js2r-unwrap-iife)
    (setq unwrapped (js2r-test-source))
    (list :wrapped wrapped :injected injected :unwrapped unwrapped)))
"##;
    let expect = expect![[
        r##"OK (:wrapped "(() => {\n  'use strict';\n  const client = window.releaseClient;\n  client.deploy(window.releaseId);\n})();\n" :injected "((root) => {\n  'use strict';\n  const client = root.releaseClient;\n  client.deploy(root.releaseId);\n})(window);\n" :unwrapped "'use strict';\nconst client = root.releaseClient;\nclient.deploy(root.releaseId);\n")"##
    ]];
    ParityBatchCase::value(
        "iife_workflow_wraps_strict_code_injects_global_alias_and_unwraps",
        elisp_form,
        expect,
    )
}

fn structural_editing_expands_nested_data_and_moves_statements_across_function_boundary()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let (expanded contracted slurped barfed)
    (js2r-test-reset
     "const release = {service: \"api\", options: {retries: 3, region: \"east\"}};\n")
    (search-forward "service")
    (js2r-expand-node-at-point)
    (setq expanded (js2r-test-source))
    (search-backward "service")
    (js2r-contract-node-at-point)
    (setq contracted (js2r-test-source))

    (js2r-test-reset
     (concat
      "function deploy() {\n"
      "  validate();\n"
      "}\n"
      "publish();\n"
      "audit();\n"
      "notify();\n"))
    (search-forward "validate")
    (js2r-forward-slurp 2)
    (setq slurped (js2r-test-source))
    (js2r-test-reparse-summary)
    (goto-char (point-min))
    (search-forward "validate")
    (js2r-forward-barf 1)
    (setq barfed (js2r-test-source))
    (list :expanded expanded
          :contracted contracted
          :slurped slurped
          :barfed barfed
          :parse (js2r-test-reparse-summary))))
"##;
    let expect = expect![[
        r##"OK (:expanded "const release = {\n  service: \"api\",\n  options: {retries: 3, region: \"east\"}\n};\n" :contracted "const release = { service: \"api\", options: {retries: 3, region: \"east\"} };\n" :slurped "function deploy() {\n  validate();\n  publish();\n  audit();\n}\nnotify();\n" :barfed "function deploy() {\n  validate();\n  publish();\n}\naudit();\nnotify();\n" :parse (:dirty nil :errors nil :warnings ("Undeclared variable or function 'validate'" "Undeclared variable or function 'publish'" "Undeclared variable or function 'audit'" "Undeclared variable or function 'notify'")))"##
    ]];
    ParityBatchCase::value(
        "structural_editing_expands_nested_data_and_moves_statements_across_function_boundary",
        elisp_form,
        expect,
    )
}

#[test]
fn js2_refactor_package_batch() {
    let cases = vec![
        signature_migration_rewrites_definition_body_and_every_local_callsite(),
        scope_aware_rename_updates_outer_bindings_without_touching_keys_properties_or_shadowing(),
        inline_and_instance_rewrites_preserve_scope_and_remove_obsolete_declarations(),
        extraction_and_conditional_rewrites_create_reusable_control_flow(),
        iife_workflow_wraps_strict_code_injects_global_alias_and_unwraps(),
        structural_editing_expands_nested_data_and_moves_statements_across_function_boundary(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed js2-refactor parity test");
    assert_oracle_batch_cases(
        js2_refactor_oracle(),
        test_name,
        "js2_refactor_parity",
        &cases,
    );
}
