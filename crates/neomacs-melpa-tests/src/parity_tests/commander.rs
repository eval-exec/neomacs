use std::time::Duration;

use expect_test::expect;

use crate::{COMMANDER_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN, F_MELPA_PIN, S_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const COMMANDER_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const COMMANDER_TEST_PRELUDE: &str = r##"
(require 'commander)

(defvar neomacs-commander-test-events nil)

(defun neomacs-commander-test-verbose ()
  (push '(:option verbose) neomacs-commander-test-events))

(defun neomacs-commander-test-config (file)
  (push (list :option 'config :file file) neomacs-commander-test-events))

(defun neomacs-commander-test-deploy (&rest targets)
  (push (list :command 'deploy :targets targets)
        neomacs-commander-test-events))

(defun neomacs-commander-test-port (&rest ports)
  (push (list :option 'port :values ports) neomacs-commander-test-events))

(defun neomacs-commander-test-fallback (&rest arguments)
  (push (list :fallback arguments) neomacs-commander-test-events))
"##;

fn commander_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COMMANDER_MELPA_PIN, "commander.el")
        .expect("prepare revision-pinned Commander source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare revision-pinned Dash dependency below ./tmp")
        .with_melpa_dependency(F_MELPA_PIN)
        .expect("prepare revision-pinned f dependency below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare revision-pinned s dependency below ./tmp")
        .with_prelude(COMMANDER_TEST_PRELUDE)
        .with_timeout(COMMANDER_TEST_TIMEOUT)
}

fn release_cli_parses_options_command_arguments_and_generates_usage() -> ParityBatchCase {
    let elisp_form = r##"
(let (neomacs-commander-test-events)
  (commander
   (name "neomacs-release")
   (description "Build and publish a reproducible Neomacs release.")
   (option "-v, --verbose" "Show each release phase."
           neomacs-commander-test-verbose)
   (option "-c, --config <FILE>" "Read release settings from FILE."
           neomacs-commander-test-config)
   (command "deploy <TARGET>"
            ("Deploy TARGET and any following artifacts."
             "Options may appear after command arguments.")
            neomacs-commander-test-deploy)
   (parse ("deploy" "production" "neomacs.tar.zst"
           "--config" "release.toml" "--verbose")))
  (list :events (nreverse neomacs-commander-test-events)
        :usage (commander-usage)
        :deploy-help (commander-usage-for "deploy")))
"##;
    let expected = expect![[
        r####"OK (:events ((:option config :file "release.toml") (:option verbose) (:command deploy :targets ("production" "neomacs.tar.zst"))) :usage "USAGE: neomacs-release [COMMAND] [OPTIONS]\n\nBuild and publish a reproducible Neomacs release.\n\nCOMMANDS:\n\n deploy <TARGET>              Deploy TARGET and any following artifacts.\n                              Options may appear after command arguments.\n\nOPTIONS:\n\n -v, --verbose                Show each release phase.\n -c, --config <FILE>          Read release settings from FILE." :deploy-help ("Deploy TARGET and any following artifacts." "Options may appear after command arguments."))"####
    ]];
    ParityBatchCase::value(
        "release_cli_parses_options_command_arguments_and_generates_usage",
        elisp_form,
        expected,
    )
}

fn optional_values_defaults_and_fallback_command_form_a_complete_cli() -> ParityBatchCase {
    let elisp_form = r##"
(list
 :default-port
 (let (neomacs-commander-test-events)
   (commander
    (option "-p, --port [PORT]" "Listen on PORT."
            neomacs-commander-test-port "8080")
    (default neomacs-commander-test-fallback "status")
    (parse ("--port")))
   (nreverse neomacs-commander-test-events))
 :explicit-port-and-input
 (let (neomacs-commander-test-events)
   (commander
    (option "-p, --port [PORT]" "Listen on PORT."
            neomacs-commander-test-port "8080")
    (default neomacs-commander-test-fallback "status")
    (parse ("artifact.tar" "--port" "9090")))
   (nreverse neomacs-commander-test-events))
 :default-command
 (let (neomacs-commander-test-events)
   (commander
    (command "deploy [TARGET]" "Deploy TARGET."
             neomacs-commander-test-deploy "staging")
    (default "deploy" "production")
    (parse nil))
   (nreverse neomacs-commander-test-events)))
"##;
    let expected = expect![[
        r####"OK (:default-port ((:option port :values ("8080")) (:fallback ("status"))) :explicit-port-and-input ((:option port :values ("9090")) (:fallback ("artifact.tar"))) :default-command ((:command deploy :targets ("production"))))"####
    ]];
    ParityBatchCase::value(
        "optional_values_defaults_and_fallback_command_form_a_complete_cli",
        elisp_form,
        expected,
    )
}

fn config_file_supplies_defaults_before_explicit_command_line_options() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((directory (make-temp-file "commander-release-" t))
       (default-directory (file-name-as-directory directory))
       neomacs-commander-test-events)
  (unwind-protect
      (progn
        (with-temp-file "release.opts"
          (insert "--config baseline.toml\n--port 7000\n"))
        (commander
         (config "release.opts")
         (option "--config <FILE>" "Configuration file."
                 neomacs-commander-test-config)
         (option "--port <PORT>" "Release service port."
                 neomacs-commander-test-port)
         (command "deploy <TARGET>" "Deploy TARGET."
                  neomacs-commander-test-deploy)
         (parse ("deploy" "production" "--port" "9000")))
        (nreverse neomacs-commander-test-events))
    (delete-directory directory t)))
"##;
    let expected = expect![[
        r####"OK ((:option config :file "baseline.toml") (:option port :values ("7000")) (:option port :values ("9000")) (:command deploy :targets ("production")))"####
    ]];
    ParityBatchCase::value(
        "config_file_supplies_defaults_before_explicit_command_line_options",
        elisp_form,
        expected,
    )
}

fn malformed_invocations_report_exact_actionable_errors() -> ParityBatchCase {
    let elisp_form = r##"
(list
 (condition-case err
     (progn
       (commander
        (option "--config <FILE>" "Configuration file."
                neomacs-commander-test-config)
        (command "deploy <TARGET>" "Deploy TARGET."
                 neomacs-commander-test-deploy)
        (parse ("--unknown")))
       :unexpected-success)
   (error (list (car err) (error-message-string err))))
 (condition-case err
     (progn
       (commander
        (option "--config <FILE>" "Configuration file."
                neomacs-commander-test-config)
        (command "deploy <TARGET>" "Deploy TARGET."
                 neomacs-commander-test-deploy)
        (parse ("--config")))
       :unexpected-success)
   (error (list (car err) (error-message-string err))))
 (condition-case err
     (progn
       (commander
        (option "--config <FILE>" "Configuration file."
                neomacs-commander-test-config)
        (command "deploy <TARGET>" "Deploy TARGET."
                 neomacs-commander-test-deploy)
        (parse ("deploy")))
       :unexpected-success)
   (error (list (car err) (error-message-string err))))
 (condition-case err
     (progn
       (commander
        (option "--config <FILE>" "Configuration file."
                neomacs-commander-test-config)
        (command "deploy <TARGET>" "Deploy TARGET."
                 neomacs-commander-test-deploy)
        (parse ("missing-command")))
       :unexpected-success)
   (error (list (car err) (error-message-string err)))))
"##;
    let expected = expect![[
        r####"OK ((error "Option ‘--unknown‘ not available") (error "Option ‘--config‘ requires argument") (error "Command ‘deploy‘ requires argument") (error "Command ‘missing-command‘ not available"))"####
    ]];
    ParityBatchCase::value(
        "malformed_invocations_report_exact_actionable_errors",
        elisp_form,
        expected,
    )
}

#[test]
fn commander_package_batch() {
    assert_oracle_batch_cases(
        commander_oracle(),
        "commander-package-batch",
        "Commander",
        &[
            release_cli_parses_options_command_arguments_and_generates_usage(),
            optional_values_defaults_and_fallback_command_form_a_complete_cli(),
            config_file_supplies_defaults_before_explicit_command_line_options(),
            malformed_invocations_report_exact_actionable_errors(),
        ],
    );
}
