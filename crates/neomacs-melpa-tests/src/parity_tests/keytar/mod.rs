use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, KEYTAR_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const KEYTAR_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const KEYTAR_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'keytar)

(defvar keytar-test-pristine-path (getenv "PATH"))
(defvar keytar-test-pristine-exec-path (copy-sequence exec-path))

(defconst keytar-test-cli-script
  "#!/bin/sh
set -eu
log=${KEYTAR_TEST_LOG:?}
state=${KEYTAR_TEST_STATE:?}
{
  printf 'CALL'
  for argument in \"$@\"; do
    printf '\\t[%s]' \"$argument\"
  done
  printf '\\n'
} >> \"$log\"
command=${1-}
case \"$command\" in
  --version)
    printf '  9.7.3  \\n'
    ;;
  set-pass)
    if [ \"${3-}\" = 'fail-service' ]; then
      exit 9
    fi
    printf '%s\\n%s\\n%s\\n' \"${3-}\" \"${5-}\" \"${7-}\" > \"$state\"
    ;;
  get-pass)
    if [ \"${3-}\" = 'type-error' ]; then
      printf 'TypeError: unavailable backend\\n'
    elif [ \"${3-}\" = 'not-enough' ]; then
      printf 'Not enough arguments for get-pass\\n'
    elif [ \"${3-}\" = 'empty-output' ]; then
      :
    elif [ -f \"$state\" ] &&
         [ \"$(sed -n '1p' \"$state\")\" = \"${3-}\" ] &&
         [ \"$(sed -n '2p' \"$state\")\" = \"${5-}\" ]; then
      sed -n '3p' \"$state\"
    else
      printf 'null\\n'
    fi
    ;;
  delete-pass)
    if [ -f \"$state\" ] &&
       [ \"$(sed -n '1p' \"$state\")\" = \"${3-}\" ] &&
       [ \"$(sed -n '2p' \"$state\")\" = \"${5-}\" ]; then
      rm \"$state\"
    else
      exit 4
    fi
    ;;
  find-creds)
    if [ -f \"$state\" ] && [ \"$(sed -n '1p' \"$state\")\" = \"${3-}\" ]; then
      account=$(sed -n '2p' \"$state\")
      password=$(sed -n '3p' \"$state\")
      printf '[{\"account\":\"%s\",\"password\":\"%s\"}]\\n' \"$account\" \"$password\"
    else
      printf 'null\\n'
    fi
    ;;
  find-pass)
    if [ -f \"$state\" ] && [ \"$(sed -n '1p' \"$state\")\" = \"${3-}\" ]; then
      sed -n '3p' \"$state\"
    else
      printf 'null\\n'
    fi
    ;;
  *)
    printf 'Not enough arguments\\n'
    exit 2
    ;;
esac
")

(defconst keytar-test-npm-script
  "#!/bin/sh
set -eu
log=${KEYTAR_TEST_NPM_LOG:?}
{
  printf 'NPM'
  for argument in \"$@\"; do
    printf '\\t[%s]' \"$argument\"
  done
  printf '\\n'
} >> \"$log\"
if [ \"${KEYTAR_TEST_NPM_FAIL-}\" = '1' ]; then
  exit 9
fi
prefix=
while [ \"$#\" -gt 0 ]; do
  if [ \"$1\" = '--prefix' ]; then
    shift
    prefix=$1
  fi
  shift
done
mkdir -p \"$prefix/bin\"
cp \"${KEYTAR_TEST_CLI_TEMPLATE:?}\" \"$prefix/bin/keytar\"
chmod 755 \"$prefix/bin/keytar\"
")

(defun keytar-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun keytar-test-write-executable (path contents)
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (insert contents))
  (set-file-modes path #o755)
  path)

(defun keytar-test-file-string (path)
  (if (file-exists-p path)
      (with-temp-buffer
        (insert-file-contents path)
        (buffer-string))
    nil))

(defun keytar-test-log-string (path)
  (when-let ((contents (keytar-test-file-string path)))
    (string-replace "\t" "<TAB>" contents)))

(defun keytar-test-normalize-root (text)
  (and text
       (replace-regexp-in-string
        (regexp-quote (directory-file-name
                       (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        "<ROOT>" text t t)))

(defun keytar-test-reset ()
  (setenv "PATH" keytar-test-pristine-path)
  (setenv "KEYTAR_TEST_LOG" nil)
  (setenv "KEYTAR_TEST_STATE" nil)
  (setenv "KEYTAR_TEST_NPM_LOG" nil)
  (setenv "KEYTAR_TEST_NPM_FAIL" nil)
  (setenv "KEYTAR_TEST_CLI_TEMPLATE" nil)
  (setq exec-path (copy-sequence keytar-test-pristine-exec-path))
  (setq text-quoting-style 'curve)
  (dolist (name '("*Shell Command Output*" "*Shell Command Error*"))
    (when-let ((buffer (get-buffer name)))
      (kill-buffer buffer)))
  (let ((directory (keytar-test-path "keytar")))
    (when (file-directory-p directory)
      (delete-directory directory t))
    (make-directory directory t)
    (setq keytar-install-dir (expand-file-name "install" directory))
    (setenv "KEYTAR_TEST_LOG" (expand-file-name "cli.log" directory))
    (setenv "KEYTAR_TEST_STATE" (expand-file-name "credential.state" directory))
    directory))

(defun keytar-test-install-cli (&optional path)
  (let ((executable
         (or path (expand-file-name "bin/keytar" keytar-install-dir))))
    (keytar-test-write-executable executable keytar-test-cli-script)))

(defun keytar-test-install-fake-npm ()
  (let* ((directory (keytar-test-path "keytar"))
         (bin (expand-file-name "fake-bin" directory))
         (npm (expand-file-name "npm" bin))
         (template (expand-file-name "keytar-template" directory)))
    (keytar-test-write-executable template keytar-test-cli-script)
    (keytar-test-write-executable npm keytar-test-npm-script)
    (setenv "KEYTAR_TEST_NPM_LOG" (expand-file-name "npm.log" directory))
    (setenv "KEYTAR_TEST_CLI_TEMPLATE" template)
    (setenv "PATH" (concat bin path-separator keytar-test-pristine-path))
    (setq exec-path (cons bin (copy-sequence keytar-test-pristine-exec-path)))
    npm))
"##;

fn keytar_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(KEYTAR_MELPA_PIN, "keytar.el")
        .expect("prepare pinned keytar source below ./tmp")
        .with_prelude(KEYTAR_TEST_PRELUDE)
        .with_timeout(KEYTAR_TEST_TIMEOUT)
}

fn executable_discovery_and_preflight_cover_configured_and_path_installations() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (keytar-test-reset)
  (let ((missing (keytar-installed-p))
        (preflight
         (condition-case problem
             (progn (keytar-version) :unexpected-success)
           (user-error
            (list :type (car problem)
                  :message (error-message-string problem))))))
    (keytar-test-install-cli)
    (let* ((configured (keytar-installed-p))
           (configured-result
            (list :basename (file-name-nondirectory configured)
                  :absolute (file-name-absolute-p configured)
                  :executable (file-executable-p configured)
                  :version (keytar-version)))
           (path-bin (keytar-test-path "keytar/path-bin")))
      (keytar-test-install-cli (expand-file-name "keytar" path-bin))
      (let ((keytar-install-dir nil)
            (exec-path (cons path-bin exec-path)))
        (setenv "PATH" (concat path-bin path-separator (getenv "PATH")))
        (let ((from-path (keytar-installed-p)))
          (list :missing missing
                :preflight preflight
                :configured configured-result
                :path (list :basename (file-name-nondirectory from-path)
                            :absolute (file-name-absolute-p from-path)
                            :executable (file-executable-p from-path))))))))
"##;
    let expect = expect![[
        r##"OK (:missing nil :preflight (:type user-error :message "[WARNING] Make sure you have installed ‘@emacs-grammarly/keytar-cli‘ through ‘npm‘ or hit ‘M-x keytar-install‘") :configured (:basename "keytar" :absolute t :executable t :version "9.7.3") :path (:basename "keytar" :absolute t :executable t))"##
    ]];
    ParityBatchCase::value(
        "executable_discovery_and_preflight_cover_configured_and_path_installations",
        elisp_form,
        expect,
    )
}

fn credential_lifecycle_preserves_shell_metacharacters_and_reports_backend_results()
-> ParityBatchCase {
    let elisp_form = r##"
(progn
  (keytar-test-reset)
  (keytar-test-install-cli)
  (let* ((service "Deploy service; $HOME")
         (account "ops team's account")
         (password "r00t token; $(blocked) & safe")
         (created (keytar-set-password service account password))
         (fetched (keytar-get-password service account))
         (credentials (keytar-find-credentials service))
         (service-password (keytar-find-password service))
         (deleted (keytar-delete-password service account))
         (after-delete (keytar-get-password service account))
         (delete-missing (keytar-delete-password service account))
         (find-missing (keytar-find-password service)))
    (list :created created
          :fetched fetched
          :credentials credentials
          :service-password service-password
          :deleted deleted
          :after-delete after-delete
          :delete-missing delete-missing
          :find-missing find-missing
          :state-exists (file-exists-p (getenv "KEYTAR_TEST_STATE"))
          :calls (keytar-test-log-string (getenv "KEYTAR_TEST_LOG")))))
"##;
    let expect = expect![[
        r##"OK (:created t :fetched "r00t token; $(blocked) & safe" :credentials "[{\"account\":\"ops team's account\",\"password\":\"r00t token; $(blocked) & safe\"}]" :service-password "r00t token; $(blocked) & safe" :deleted t :after-delete nil :delete-missing nil :find-missing nil :state-exists nil :calls "CALL<TAB>[set-pass]<TAB>[-s]<TAB>[Deploy service; $HOME]<TAB>[-a]<TAB>[ops team's account]<TAB>[-p]<TAB>[r00t token; $(blocked) & safe]\nCALL<TAB>[get-pass]<TAB>[-s]<TAB>[Deploy service; $HOME]<TAB>[-a]<TAB>[ops team's account]\nCALL<TAB>[find-creds]<TAB>[-s]<TAB>[Deploy service; $HOME]\nCALL<TAB>[find-pass]<TAB>[-s]<TAB>[Deploy service; $HOME]\nCALL<TAB>[delete-pass]<TAB>[-s]<TAB>[Deploy service; $HOME]<TAB>[-a]<TAB>[ops team's account]\nCALL<TAB>[get-pass]<TAB>[-s]<TAB>[Deploy service; $HOME]<TAB>[-a]<TAB>[ops team's account]\nCALL<TAB>[delete-pass]<TAB>[-s]<TAB>[Deploy service; $HOME]<TAB>[-a]<TAB>[ops team's account]\nCALL<TAB>[find-pass]<TAB>[-s]<TAB>[Deploy service; $HOME]\n")"##
    ]];
    ParityBatchCase::value(
        "credential_lifecycle_preserves_shell_metacharacters_and_reports_backend_results",
        elisp_form,
        expect,
    )
}

fn backend_null_diagnostics_and_empty_output_follow_the_public_validity_contract() -> ParityBatchCase
{
    let elisp_form = r##"
(progn
  (keytar-test-reset)
  (keytar-test-install-cli)
  (list
   :missing (keytar-get-password "missing" "account")
   :type-error (keytar-get-password "type-error" "account")
   :not-enough (keytar-get-password "not-enough" "account")
   :empty (keytar-get-password "empty-output" "account")
   :direct
   (mapcar #'keytar--valid-return
           '("null" "TypeError: backend" "prefix TypeError: backend"
             "Not enough arguments" "" "  secret  "))
   :calls (keytar-test-log-string (getenv "KEYTAR_TEST_LOG"))))
"##;
    let expect = expect![[
        r##"OK (:missing nil :type-error nil :not-enough nil :empty "" :direct (nil nil nil nil "" "  secret  ") :calls "CALL<TAB>[get-pass]<TAB>[-s]<TAB>[missing]<TAB>[-a]<TAB>[account]\nCALL<TAB>[get-pass]<TAB>[-s]<TAB>[type-error]<TAB>[-a]<TAB>[account]\nCALL<TAB>[get-pass]<TAB>[-s]<TAB>[not-enough]<TAB>[-a]<TAB>[account]\nCALL<TAB>[get-pass]<TAB>[-s]<TAB>[empty-output]<TAB>[-a]<TAB>[account]\n")"##
    ]];
    ParityBatchCase::value(
        "backend_null_diagnostics_and_empty_output_follow_the_public_validity_contract",
        elisp_form,
        expect,
    )
}

fn npm_install_uses_the_configured_prefix_and_is_idempotent_once_the_cli_exists() -> ParityBatchCase
{
    let elisp_form = r##"
(progn
  (keytar-test-reset)
  (keytar-test-install-fake-npm)
  (let* ((first-message (keytar-install))
         (installed (keytar-installed-p))
         (version (keytar-version))
         (first-log (keytar-test-normalize-root
                     (keytar-test-log-string (getenv "KEYTAR_TEST_NPM_LOG"))))
         (second-message (keytar-install))
         (second-log (keytar-test-normalize-root
                      (keytar-test-log-string (getenv "KEYTAR_TEST_NPM_LOG")))))
    (list :first-message first-message
          :installed (list (file-name-nondirectory installed)
                           (file-executable-p installed))
          :version version
          :npm-first first-log
          :second-message second-message
          :npm-unchanged (equal first-log second-log))))
"##;
    let expect = expect![[
        r##"OK (:first-message "Successfully install ‘@emacs-grammarly/keytar-cli‘ through ‘npm‘!" :installed ("keytar" t) :version "9.7.3" :npm-first "NPM<TAB>[install]<TAB>[-g]<TAB>[@emacs-grammarly/keytar-cli]<TAB>[--prefix]<TAB>[<ROOT>/keytar/install]\n" :second-message "NPM package ‘@emacs-grammarly/keytar-cli‘ is already installed" :npm-unchanged t)"##
    ]];
    ParityBatchCase::value(
        "npm_install_uses_the_configured_prefix_and_is_idempotent_once_the_cli_exists",
        elisp_form,
        expect,
    )
}

fn npm_install_failure_surfaces_a_user_error_without_claiming_the_cli_is_available()
-> ParityBatchCase {
    let elisp_form = r##"
(progn
  (keytar-test-reset)
  (keytar-test-install-fake-npm)
  (setenv "KEYTAR_TEST_NPM_FAIL" "1")
  (let ((outcome
         (condition-case problem
             (progn (keytar-install) :unexpected-success)
           (user-error
            (list :type (car problem)
                  :message (error-message-string problem))))))
    (list :outcome outcome
          :installed (and (keytar-installed-p) t)
          :npm-log (keytar-test-normalize-root
                    (keytar-test-log-string (getenv "KEYTAR_TEST_NPM_LOG"))))))
"##;
    let expect = expect![[
        r##"OK (:outcome (:type user-error :message "Failed to install‘ @emacs-grammarly/keytar-cli‘ through ‘npm‘, make sure you have npm installed") :installed nil :npm-log "NPM<TAB>[install]<TAB>[-g]<TAB>[@emacs-grammarly/keytar-cli]<TAB>[--prefix]<TAB>[<ROOT>/keytar/install]\n")"##
    ]];
    ParityBatchCase::value(
        "npm_install_failure_surfaces_a_user_error_without_claiming_the_cli_is_available",
        elisp_form,
        expect,
    )
}

#[test]
fn keytar_package_batch() {
    let cases = vec![
        executable_discovery_and_preflight_cover_configured_and_path_installations(),
        credential_lifecycle_preserves_shell_metacharacters_and_reports_backend_results(),
        backend_null_diagnostics_and_empty_output_follow_the_public_validity_contract(),
        npm_install_uses_the_configured_prefix_and_is_idempotent_once_the_cli_exists(),
        npm_install_failure_surfaces_a_user_error_without_claiming_the_cli_is_available(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed keytar parity test");
    assert_oracle_batch_cases(keytar_oracle(), test_name, "keytar_parity", &cases);
}
