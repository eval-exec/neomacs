use std::time::Duration;

use crate::{AANGIT_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AANGIT_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Sandbox helpers shared by the workflows.  aangit is a Transient front end
/// for the `ng` and `npm` command line tools, so the tests install local stand
/// ins for those two executables: they record their exact argument vector and
/// produce a realistic Angular workspace, while aangit keeps running its real
/// menu, argument, prompt and `shell-command` path.
const AANGIT_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar aangit-test-root
  (file-name-as-directory
   (expand-file-name "workspace" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defvar aangit-test-log
  (expand-file-name "commands.log" aangit-test-root))

(defun aangit-test-write-executable (name body)
  (let ((path (expand-file-name name (expand-file-name "bin" aangit-test-root))))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert body)
      (write-region (point-min) (point-max) path nil 'silent))
    (set-file-modes path #o755)
    path))

(defun aangit-test-setup-cli ()
  "Install recording `ng' and `npm' stand-ins and enter the workspace."
  (when (file-directory-p aangit-test-root)
    (delete-directory aangit-test-root t))
  (make-directory aangit-test-root t)
  (aangit-test-write-executable
   "ng"
   (concat "#!/bin/sh\n"
           "printf '%s\\n' \"ng $*\" >> \"$AANGIT_LOG\"\n"
           "if [ \"$1\" = new ]; then\n"
           "  mkdir -p \"$3/src/app\"\n"
           "  printf '{\"name\":\"%s\"}\\n' \"$3\" > \"$3/angular.json\"\n"
           "  printf 'bootstrapApplication(%s);\\n' \"$3\" > \"$3/src/main.ts\"\n"
           "  printf 'export class AppComponent {}\\n' > \"$3/src/app/app.component.ts\"\n"
           "fi\n"
           "if [ \"$1\" = generate ] && [ \"$2\" = component ]; then\n"
           "  mkdir -p \"src/app/$3\"\n"
           "  printf 'export class %sComponent {}\\n' \"$3\" > \"src/app/$3/$3.component.ts\"\n"
           "fi\n"
           "exit 0\n"))
  (aangit-test-write-executable
   "npm"
   (concat "#!/bin/sh\n"
           "printf '%s\\n' \"npm $*\" >> \"$AANGIT_LOG\"\n"
           "exit 0\n"))
  (setq default-directory aangit-test-root)
  (setenv "AANGIT_LOG" aangit-test-log)
  (setenv "PATH"
          (concat (expand-file-name "bin" aangit-test-root)
                  path-separator
                  (getenv "PATH")))
  aangit-test-root)

(defun aangit-test-commands ()
  "Return the exact command lines the stand-in executables recorded."
  (if (file-exists-p aangit-test-log)
      (with-temp-buffer
        (insert-file-contents aangit-test-log)
        (split-string (buffer-string) "\n" t))
    'no-command-ran))

(defun aangit-test-last-message ()
  (with-current-buffer (get-buffer-create "*Messages*")
    (car (last (split-string (buffer-string) "\n" t)))))

(defun aangit-test-relative-files (directory)
  (let ((directory
         (file-name-as-directory
          (expand-file-name directory aangit-test-root))))
    (sort (mapcar (lambda (path) (file-relative-name path directory))
                  (directory-files-recursively directory ".*"))
          #'string<)))

(defun aangit-test-active-prefix ()
  (and (boundp 'transient--prefix)
       transient--prefix
       (oref transient--prefix command)))
"##;

fn aangit_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AANGIT_MELPA_PIN, "aangit.el")
        .expect("prepare pinned aangit source below ./tmp")
        .with_prelude(AANGIT_TEST_PRELUDE)
        .with_timeout(AANGIT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed aangit parity test").into()
}

/// Multi-probe batch for `assert_aangit_parity` cases (2a).
pub(crate) fn assert_aangit_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(aangit_oracle(), &name, "aangit_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn aangit_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_aangit_batch(&cases);
}

// END generated package batch tests
