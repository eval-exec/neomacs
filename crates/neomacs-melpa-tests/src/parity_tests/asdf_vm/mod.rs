use std::time::Duration;

use crate::{ASDF_VM_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod config;
mod core;
mod installer;
mod mode;
mod plugin;
mod plugin_menu;
mod process;
mod registry;
mod tool_versions;
mod util;
mod workflows;

const ASDF_VM_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ASDF_VM_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)

(defun asdf-vm-test-path (filename)
  (expand-file-name
   filename
   (getenv
    "NEOMACS_TEST_SANDBOX_ROOT")))

(defun asdf-vm-test-write-file (path content)
  (make-directory
   (file-name-directory path)
   t)
  (with-temp-file path
    (insert content))
  path)

(defun asdf-vm-test-read-file (path)
  (with-temp-buffer
    (insert-file-contents-literally path)
    (buffer-string)))

(defun asdf-vm-test-tabulated-list-goto-id
    (id)
  (goto-char
   (point-min))
  (let (found)
    (while
        (and
         (not found)
         (not
          (eobp)))
      (when
          (equal
           (tabulated-list-get-id)
           id)
        (setq found t))
      (unless found
        (forward-line 1)))
    found))

(defun asdf-vm-test-make-executable
    (name body)
  (let ((path
         (asdf-vm-test-path
          (concat
           "bin/"
           name))))
    (asdf-vm-test-write-file
     path
     (concat
      "#!/bin/sh\n"
      "set -eu\n"
      body
      "\n"))
    (set-file-modes path #o755)
    path))

(defun asdf-vm-test-error-data
    (thunk)
  (condition-case error-data
      (list :ok
            (funcall thunk))
    (error
     (list :error
           (car error-data)
           (cdr error-data)))))


;;; --- Real asdf 0.15.0 replay ------------------------------------------------

(defvar asdf-vm-test-records
  (file-name-as-directory
   (expand-file-name "asdf-records" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defvar asdf-vm-test-calls
  (expand-file-name "asdf-calls.log" (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defvar asdf-vm-test-misses
  (expand-file-name "asdf-misses.log" (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defconst asdf-vm-test-recordings
  '(
    (("current") 0 "" "No plugins installed\n")
    (("list-all" "nodejs") 1 "" "No such plugin: nodejs\n")
    (("list" "all" "nodejs") 1 "" "No such plugin: nodejs\n")
    (("plugin" "list") 0 "" "No plugins installed\n")
    (("set" "nodejs" "20.0.0") 1 "" "Unknown command: `asdf set nodejs 20.0.0`\nNo plugin named set\n")
    (("version") 0 "v0.15.0\n" "")))

(defun asdf-vm-test-key (arguments)
  "Return the record key for ARGUMENTS.  Must match the shell stand-in."
  (mapconcat
   (lambda (argument)
     (let ((base (if (string-match-p "/" argument)
                     (file-name-nondirectory (directory-file-name argument))
                   argument)))
       (concat "~" (replace-regexp-in-string "[^A-Za-z0-9._-]" "_" base))))
   arguments ""))

(defun asdf-vm-test-write-raw (path content)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert content)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defconst asdf-vm-test-stand-in
  (string-join
   (list
    "#!/bin/sh"
    "# Replay stand-in for asdf 0.15.0.  Every reply below was recorded from the"
    "# real binary; this script only looks one up and refuses to invent one."
    "key=\"\""
    "for a in \"$@\"; do"
    "  case \"$a\" in */) a=${a%/} ;; esac"
    "  case \"$a\" in */*) a=${a##*/} ;; esac"
    "  key=\"$key~$(printf '%s' \"$a\" | tr -c 'A-Za-z0-9._-' '_')\""
    "done"
    "printf '%s\\n' \"$*\" >> \"$ASDF_VM_TEST_CALLS\""
    "d=\"$ASDF_VM_TEST_RECORDS/$key\""
    "if [ ! -f \"$d/rc\" ]; then"
    "  printf '%s\\n' \"$*\" >> \"$ASDF_VM_TEST_MISSES\""
    "  printf 'UNRECORDED asdf invocation: %s\\n' \"$*\" >&2"
    "  exit 99"
    "fi"
    "cat \"$d/out\""
    "cat \"$d/err\" >&2"
    "exit \"$(cat \"$d/rc\")\""
    "")
   "\n"))

(defun asdf-vm-test-install ()
  "Install the recorded asdf 0.15.0 stand-in and point the package at it."
  (let ((installed nil)
        (bin (expand-file-name "bin" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
    (dolist (recording asdf-vm-test-recordings)
      (let* ((key (asdf-vm-test-key (nth 0 recording)))
             (path (expand-file-name key asdf-vm-test-records)))
        (when (member path installed)
          (error "Record key collision for %S" (nth 0 recording)))
        (push path installed)
        (asdf-vm-test-write-raw (expand-file-name "rc" path)
                                (format "%d\n" (nth 1 recording)))
        (asdf-vm-test-write-raw (expand-file-name "out" path) (nth 2 recording))
        (asdf-vm-test-write-raw (expand-file-name "err" path) (nth 3 recording))))
    (setenv "ASDF_VM_TEST_RECORDS" (directory-file-name asdf-vm-test-records))
    (setenv "ASDF_VM_TEST_CALLS" asdf-vm-test-calls)
    (setenv "ASDF_VM_TEST_MISSES" asdf-vm-test-misses)
    (let ((path (expand-file-name "asdf" bin)))
      (asdf-vm-test-write-raw path asdf-vm-test-stand-in)
      (set-file-modes path #o755)
      (setq asdf-vm-process-executable path))
    (length installed)))

(defun asdf-vm-test-settle (&optional seconds)
  "Wait for the package's asynchronous asdf process and its sentinel.

Waits on the process itself, not on a fixed duration: several of these calls
are expected to produce no stdout at all, and a polling budget spent proving
that is the marginal-timeout trap."
  (let ((deadline (+ (float-time) (or seconds 20.0))))
    (while (and (< (float-time) deadline)
                (seq-some (lambda (p)
                            (and (process-live-p p)
                                 (string-prefix-p "asdf" (process-name p))))
                          (process-list)))
      (accept-process-output nil 0.02))
    (accept-process-output nil 0.05)))

(defun asdf-vm-test-calls-made ()
  (if (not (file-exists-p asdf-vm-test-calls))
      'asdf-was-never-run
    (with-temp-buffer
      (insert-file-contents asdf-vm-test-calls)
      (split-string (buffer-string) "\n" t))))

(defun asdf-vm-test-unrecorded ()
  "Invocations the stand-in had no recording for; asserted empty everywhere.

asdf reports most failures on stderr with an empty stdout, so a stand-in
answering nothing looks exactly like a command that ran and printed nothing."
  (if (not (file-exists-p asdf-vm-test-misses))
      nil
    (with-temp-buffer
      (insert-file-contents asdf-vm-test-misses)
      (split-string (buffer-string) "\n" t))))

(defun asdf-vm-test-buffer (name)
  (let ((buffer (get-buffer name)))
    (if (not buffer)
        'no-such-buffer
      (with-current-buffer buffer
        (buffer-substring-no-properties (point-min) (point-max))))))
"##;

fn asdf_vm_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ASDF_VM_MELPA_PIN, source_file)
        .expect("prepare pinned asdf-vm source below ./tmp")
        .with_prelude(ASDF_VM_TEST_PRELUDE)
        .with_timeout(ASDF_VM_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed asdf-vm parity test")
        .into()
}

/// Multi-probe batch for `assert_asdf_vm_autoload_parity` cases (2a).
pub(crate) fn assert_asdf_vm_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        asdf_vm_oracle("asdf-vm-autoloads.el"),
        &name,
        "asdf_vm_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_asdf_vm_parity` cases (2a).
pub(crate) fn assert_asdf_vm_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(asdf_vm_oracle("asdf-vm.el"), &name, "asdf_vm_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn asdf_vm_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_asdf_vm_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_asdf_vm_autoload_batch(&cases);
}

#[test]
fn asdf_vm_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        config::config_public_surface_batch_cases(),
        core::core_public_surface_batch_cases(),
        installer::installer_public_surface_batch_cases(),
        mode::mode_public_surface_batch_cases(),
        plugin::plugin_public_surface_batch_cases(),
        plugin_menu::plugin_menu_public_surface_batch_cases(),
        process::process_public_surface_batch_cases(),
        registry::registry_asdf_vm_batch_cases(),
        tool_versions::tool_versions_public_surface_batch_cases(),
        util::util_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_asdf_vm_batch(&cases);
}

// END generated package batch tests
