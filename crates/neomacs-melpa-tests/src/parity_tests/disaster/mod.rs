//! Practical parity for disaster.  The package compiles the C/C++/Fortran
//! file under point -- via make, a compile_commands.json command, or the
//! default compiler -- and disassembles the object with objdump, jumping
//! to and highlighting the line matching the current source line.
//!
//! The compiler, make, and objdump are environmental: the prelude installs
//! recording shell stand-ins ahead of PATH (the documented disaster-cc /
//! disaster-objdump customizations point at them) that replay output
//! captured from a real objdump run against exactly the fixture source the
//! suite authors, and log every argument vector so the cases assert the
//! exact commands the package built.

use std::time::Duration;

use crate::{CachedMelpaOracle, DISASTER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"(require 'cl-lib)
(require 'package)

(setq make-backup-files nil create-lockfiles nil)

(defvar disaster--test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
(defvar disaster--test-bin
  (file-name-as-directory (expand-file-name "bin" disaster--test-root)))
(defvar disaster--test-fixtures
  (file-name-as-directory (expand-file-name "disaster-fixtures"
                                            disaster--test-root)))

;; Provenance: pinned upstream 0299c129d4153e3a794358159737c3ff9d155654.
(defconst disaster--test-upstream-tree
  "99fc80bd6c76227721f176644bbd4b5d76b2a22f"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst disaster--test-manifest
  '(("disaster.el"
     . "7f2b25882f0a042fcc215582ef028ac5072a1d729e6e2c3e291775a78c20d6c9"))
  "Per-file sha256 of the package-built sources the suite verifies.
package-build replaces the upstream `Version:' header with
`Package-Version:'/`Package-Revision:', so the hash covers the built form.")

(defconst disaster--test-cc-b64
  "IyEvYmluL3NoCmRpcj0iJERJU0FTVEVSX1RFU1RfRElSIgp7IGZvciBhIGluICIkQCI7IGRvIHByaW50ZiAnWyVzXScgIiRhIjsgZG9uZTsgcHJpbnRmICdcbic7IH0gPj4gIiRkaXIvY2MubG9nIgppZiBbICIkRElTQVNURVJfVEVTVF9GQUlMIiA9IDEgXTsgdGhlbgogIGNhdCAiJGRpci9yZWNvcmRlZC1jb21waWxlLWVycm9yLnR4dCIgPiYyCiAgZXhpdCAxCmZpCm91dD0KcHJldj0KZm9yIGEgaW4gIiRAIjsgZG8KICBpZiBbICIkcHJldiIgPSAtbyBdOyB0aGVuIG91dD0iJGEiOyBmaQogIHByZXY9IiRhIgpkb25lCm1rZGlyIC1wICIkKGRpcm5hbWUgIiRvdXQiKSIKOiA+ICIkb3V0IgpleGl0IDAK"
  "Base64 of the recording `cc' stand-in.")

(defconst disaster--test-objdump-b64
  "IyEvYmluL3NoCmRpcj0iJERJU0FTVEVSX1RFU1RfRElSIgp7IGZvciBhIGluICIkQCI7IGRvIHByaW50ZiAnWyVzXScgIiRhIjsgZG9uZTsgcHJpbnRmICdcbic7IH0gPj4gIiRkaXIvb2JqZHVtcC5sb2ciCmlmIFsgIiRESVNBU1RFUl9URVNUX09CSkRVTVBfTUlTUyIgPSAxIF07IHRoZW4KICBwcmludGYgJyVzXG4nICIkKGJhc2VuYW1lICIkMSIpOiAgICAgZmlsZSBmb3JtYXQgZWxmNjQteDg2LTY0IiAiIiAiIiAiRGlzYXNzZW1ibHkgb2Ygc2VjdGlvbiAudGV4dDoiICIiICIwMDAwMDAwMDAwMDAwMDAwIDxhZGQ+OiIgIiAgIDA6CWxlYSAgICAoJXJkaSwlcnNpLDEpLCVlYXgiICIgICAzOglyZXQiCiAgZXhpdCAwCmZpCnNlZCAic3xAQERJU0FTVEVSLU9CSkBAfCQoYmFzZW5hbWUgIiQxIil8IiAiJGRpci9yZWNvcmRlZC1vYmpkdW1wLnR4dCIKZXhpdCAwCg=="
  "Base64 of the recording `objdump' stand-in.")

(defconst disaster--test-make-b64
  "IyEvYmluL3NoCmRpcj0iJERJU0FTVEVSX1RFU1RfRElSIgp7IGZvciBhIGluICIkQCI7IGRvIHByaW50ZiAnWyVzXScgIiRhIjsgZG9uZTsgcHJpbnRmICdcbic7IH0gPj4gIiRkaXIvbWFrZS5sb2ciCnRhcmdldD0KZm9yIGEgaW4gIiRAIjsgZG8KICBjYXNlICIkYSIgaW4KICAgIC0qKSA7OwogICAgKikgdGFyZ2V0PSIkYSIgOzsKICBlc2FjCmRvbmUKOiA+ICIkdGFyZ2V0IgpleGl0IDAK"
  "Base64 of the recording `make' stand-in.")

(defconst disaster--test-recorded-objdump-b64
  "CkBARElTQVNURVItT0JKQEA6ICAgICBmaWxlIGZvcm1hdCBlbGY2NC14ODYtNjQKCgpEaXNhc3NlbWJseSBvZiBzZWN0aW9uIC50ZXh0OgoKMDAwMDAwMDAwMDAwMDAwMCA8YWRkPjoKYWRkKCk6CkBARElTQVNURVItUkVDT1JEQEAvYXBwLmM6MgppbnQgYWRkKGludCBhLCBpbnQgYikgewogIHJldHVybiBhICsgYjsKICAgMDoJbGVhICAgICglcmRpLCVyc2ksMSksJWVheApAQERJU0FTVEVSLVJFQ09SREBAL2FwcC5jOjMKfQogICAzOgl4b3IgICAgJWVzaSwlZXNpCiAgIDU6CXhvciAgICAlZWRpLCVlZGkKICAgNzoJcmV0CgpEaXNhc3NlbWJseSBvZiBzZWN0aW9uIC50ZXh0LnN0YXJ0dXA6CgowMDAwMDAwMDAwMDAwMDAwIDxtYWluPjoKbWFpbigpOgpAQERJU0FTVEVSLVJFQ09SREBAL2FwcC5jOjYKCmludCBtYWluKHZvaWQpIHsKICByZXR1cm4gYWRkKDIsIDMpOwogICAwOgltb3YgICAgJDB4MywlZXNpCiAgIDU6CW1vdiAgICAkMHgyLCVlZGkKICAgYToJam1wICAgIGYgPG1haW4rMHhmPgo="
  "Base64 of the recorded objdump output.  Captured from a real
`objdump -d -M att -Sl --no-show-raw-insn' run against the compiled
fixture source, then stripped of machine-specific paths.")

(defconst disaster--test-compile-error-b64
  "YXBwLmM6Mjo1OiBlcnJvcjogZXhwZWN0ZWQgJzsnIGFmdGVyIGV4cHJlc3Npb24KICByZXR1cm4gYSArIGIKICAgIF4KMSBlcnJvciBnZW5lcmF0ZWQuCg=="
  "Base64 of the recorded compiler failure output.")

(defconst disaster--test-app-c
  "int add(int a, int b) {\n  return a + b;\n}\n\nint main(void) {\n  return add(2, 3);\n}\n"
  "The exact source the recorded objdump output was generated from.")

(defun disaster--test-write (path text)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun disaster--test-read (path)
  (with-temp-buffer
    (let ((coding-system-for-read 'utf-8-unix))
      (insert-file-contents path)
      (buffer-string))))

(defun disaster--test-normalize (text)
  (replace-regexp-in-string
   (regexp-quote (directory-file-name disaster--test-root))
   "@@ROOT@@" text t t))

(defun disaster--test-install-standins ()
  "Install the recording `cc'/`objdump'/`make' stand-ins ahead of PATH."
  (dolist (entry (list (cons "cc" disaster--test-cc-b64)
                       (cons "objdump" disaster--test-objdump-b64)
                       (cons "make" disaster--test-make-b64)))
    (let ((program (expand-file-name (car entry) disaster--test-bin)))
      (disaster--test-write
       program
       (decode-coding-string (base64-decode-string (cdr entry))
                             'utf-8-unix))
      (set-file-modes program #o755)))
  (disaster--test-write
   (expand-file-name "recorded-objdump.txt" disaster--test-fixtures)
   (decode-coding-string (base64-decode-string
                          disaster--test-recorded-objdump-b64)
                         'utf-8-unix))
  (disaster--test-write
   (expand-file-name "recorded-compile-error.txt" disaster--test-fixtures)
   (decode-coding-string (base64-decode-string
                          disaster--test-compile-error-b64)
                         'utf-8-unix))
  (setq disaster-cc (expand-file-name "cc" disaster--test-bin)
        disaster-objdump (expand-file-name "objdump" disaster--test-bin))
  (setenv "DISASTER_TEST_DIR" (directory-file-name disaster--test-fixtures))
  (setenv "PATH" (concat disaster--test-bin
                          path-separator (getenv "PATH")))
  (setq exec-path (cons disaster--test-bin exec-path)))

(defun disaster--test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "disaster.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/disaster.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed disaster location: %S" located))
    (dolist (entry disaster--test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed disaster source: %S"
                   (car entry))))))
    (list :upstream-tree disaster--test-upstream-tree
          :feature (featurep 'disaster)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'disaster package-alist)))))))

(defun disaster--test-open (relpath content)
  "Write CONTENT into FIXTURES/RELPATH, visiting it in a fresh buffer."
  (let* ((path (expand-file-name relpath disaster--test-fixtures))
         (name (file-name-nondirectory relpath)))
    (when (get-buffer name)
      (with-current-buffer (get-buffer name)
        (set-buffer-modified-p nil)
        (kill-buffer)))
    (disaster--test-write path content)
    (find-file path)))

(defun disaster--test-reset-log (name)
  (let ((log (expand-file-name (concat name ".log")
                               disaster--test-fixtures)))
    (when (file-exists-p log)
      (delete-file log))))

(defvar disaster--test-messages nil)

(defmacro disaster--test-with-message-capture (&rest body)
  "Run BODY with `message' captured."
  `(let ((disaster--test-messages nil))
     (cl-letf (((symbol-function 'message)
                (lambda (fmt &rest args)
                  (push (apply #'format-message fmt args)
                        disaster--test-messages))))
       ,@body)))

(defun disaster--test-result (&rest plist)
  (append
   plist
   (list :messages (nreverse disaster--test-messages)
         :cc-calls (disaster--test-normalize
                    (if (file-exists-p
                         (expand-file-name "cc.log" disaster--test-fixtures))
                        (disaster--test-read
                         (expand-file-name "cc.log" disaster--test-fixtures))
                      ""))
         :objdump-calls
         (disaster--test-normalize
          (if (file-exists-p
               (expand-file-name "objdump.log" disaster--test-fixtures))
              (disaster--test-read
               (expand-file-name "objdump.log" disaster--test-fixtures))
            ""))
         :make-calls (disaster--test-normalize
                      (if (file-exists-p
                           (expand-file-name "make.log"
                                             disaster--test-fixtures))
                          (disaster--test-read
                           (expand-file-name "make.log"
                                             disaster--test-fixtures))
                        "")))))

(defun disaster--test-reset ()
  "Restore editor state mutated by the workflows."
  (dolist (buf (list disaster-buffer-assembly disaster-buffer-compiler
                     "app.c"))
    (when (get-buffer buf)
      (with-current-buffer (get-buffer buf)
        (set-buffer-modified-p nil)
        (kill-buffer))))
  (setenv "DISASTER_TEST_FAIL" nil)
  (setenv "DISASTER_TEST_OBJDUMP_MISS" nil)
  (disaster--test-reset-log "cc")
  (disaster--test-reset-log "objdump")
  (disaster--test-reset-log "make"))

(disaster--test-install-standins)

;; The sandbox sits inside the git workspace, whose root also carries a
;; Makefile: vc-root-dir and the default marker scan would both resolve
;; the workspace root, and every object path would embed the
;; per-run-random sandbox directory.  The suite therefore runs under a
;; documented editor configuration -- vc disabled, and a marker list
;; without the workspace-wide "Makefile" entry -- so the package's own
;; marker scan and default-compiler fallback are what the workflows
;; exercise.  Case directories that need a project root carry a
;; .projectile marker.
(setq vc-handled-backends nil)
(setq disaster-project-root-files
      '((".projectile") ("CMakeLists.txt")))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DISASTER_MELPA_PIN, "disaster.el")
        .expect("prepare pinned disaster source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn disaster_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(
        oracle(),
        "disaster_package_batch",
        "disaster_parity",
        &cases,
    );
}
