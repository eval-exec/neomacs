use std::time::Duration;

use crate::{AC_C_HEADERS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_C_HEADERS_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// A real include tree plus the auto-complete driver the package documents:
/// "require this script (and auto-complete) then add to `ac-sources'".  The
/// workflows complete through `ac-start` / `ac-update` / `ac-complete` in a
/// window-displayed buffer, with `cc-search-directories` pointing at the
/// sandbox headers.
const AC_C_HEADERS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'auto-complete)

(defun ac-c-headers-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ac-c-headers-test-write (name text)
  (let ((path (ac-c-headers-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun ac-c-headers-test-include-tree ()
  "Create a small but realistic include tree and return its directory."
  (ac-c-headers-test-write
   "include/stdio.h"
   "int printf(const char *fmt, ...);\nint puts(const char *s);\n")
  (ac-c-headers-test-write
   "include/string.h"
   (concat "/* strlen_only_in_block_comment */\n"
           "size_t strlen(const char *s);\n"
           "// strdup_only_in_line_comment\n"
           "char *strchr(const char *s, int c);\n"))
  (ac-c-headers-test-write "include/sys/types.h" "typedef long ssize_t;\n")
  (ac-c-headers-test-write "include/sys/stat.h" "struct stat;\n")
  (ac-c-headers-test-write "include/notaheader.txt" "not a header\n")
  (ac-c-headers-test-path "include"))

(defmacro ac-c-headers-test-in-buffer (&rest body)
  "Run BODY in a window-displayed buffer with auto-complete armed."
  `(let ((buffer (generate-new-buffer "*ac-c-headers-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (setq ac-sources '(ac-source-c-headers))
           (auto-complete-mode 1)
           ,@body)
       (kill-buffer buffer))))

(defun ac-c-headers-test-candidates ()
  "Start completion at point and return the plain candidate strings."
  (ac-start :force-init t)
  (ac-update t)
  (mapcar #'substring-no-properties ac-candidates))
"##;

fn ac_c_headers_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_C_HEADERS_MELPA_PIN, "ac-c-headers.el")
        .expect("prepare pinned ac-c-headers source below ./tmp")
        .with_prelude(AC_C_HEADERS_TEST_PRELUDE)
        .with_timeout(AC_C_HEADERS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-c-headers parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_c_headers_parity` cases (2a).
pub(crate) fn assert_ac_c_headers_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_c_headers_oracle(), &name, "ac_c_headers_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_c_headers_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_c_headers_batch(&cases);
}

// END generated package batch tests
