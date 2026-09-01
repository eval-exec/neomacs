use std::time::Duration;

use crate::{CachedMelpaOracle, XCSCOPE_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const XCSCOPE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const XCSCOPE_TEST_PRELUDE: &str = r####"
(require 'subr-x)
(require 'xcscope)

(defvar xcscope-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
(defvar xcscope-test-bin
  (file-name-as-directory (expand-file-name "xcscope-bin" xcscope-test-root)))
(defvar xcscope-test-calls-log-path
  (expand-file-name "xcscope-calls.log" xcscope-test-root))
(defvar xcscope-test-misses-log-path
  (expand-file-name "xcscope-misses.log" xcscope-test-root))

(defun xcscope-test-write (path content &optional executable)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-file path
      (insert content)))
  (when executable
    (set-file-modes path #o755))
  path)

(defun xcscope-test-read (path)
  (with-temp-buffer
    (insert-file-contents-literally path)
    (buffer-string)))

(defconst xcscope-test-cscope-stand-in
  (string-join
   (list
    "#!/bin/sh"
    "set -eu"
    "printf 'cwd=%s|' \"$PWD\" >> \"$XCSCOPE_TEST_CALLS\""
    "for arg in \"$@\"; do printf '<%s>' \"$arg\" >> \"$XCSCOPE_TEST_CALLS\"; done"
    "printf '\\n' >> \"$XCSCOPE_TEST_CALLS\""
    "case \" $* \" in"
    "  *' -0 release_count '*)"
    "    printf '%s\\n' 'src/release.c publish_release 7   release_count += 1;'"
    "    printf '%s\\n' 'src/main.c validate_release 4   if (release_count > 0) publish_release();'"
    "    ;;"
    "  *' -1 publish_release '*)"
    "    printf '%s\\n' 'src/release.c publish_release 8 void publish_release(void) {'"
    "    ;;"
    "  *' -4 absent_release_marker '*)"
    "    ;;"
    "  *' -4 backend_unavailable '*)"
    "    printf 'cscope: cannot read database\\n' >&2"
    "    exit 42"
    "    ;;"
    "  *' -b -i cscope.files -f cscope.out '*)"
    "    printf 'fixture database\\n' > cscope.out"
    "    printf 'Indexed %s source files.\\n' \"$(wc -l < cscope.files | tr -d ' ')\""
    "    ;;"
    "  *)"
    "    printf 'UNRECORDED cscope invocation: %s\\n' \"$*\" >> \"$XCSCOPE_TEST_MISSES\""
    "    printf 'UNRECORDED cscope invocation: %s\\n' \"$*\" >&2"
    "    exit 99"
    "    ;;"
    "esac"
    "")
   "\n"))

(defun xcscope-test-install-cscope ()
  (make-directory xcscope-test-bin t)
  (setenv "XCSCOPE_TEST_CALLS" xcscope-test-calls-log-path)
  (setenv "XCSCOPE_TEST_MISSES" xcscope-test-misses-log-path)
  (xcscope-test-write
   (expand-file-name "cscope" xcscope-test-bin)
   xcscope-test-cscope-stand-in
   t))

(defun xcscope-test-reset ()
  (dolist (buffer-name
           (list cscope-output-buffer-name cscope-info-buffer-name
                 "*cscope-indexing-buffer*"))
    (when-let ((buffer (get-buffer buffer-name)))
      (when-let ((process (get-buffer-process buffer)))
        (ignore-errors (delete-process process)))
      (kill-buffer buffer)))
  (dolist (path (list xcscope-test-calls-log-path
                      xcscope-test-misses-log-path))
    (when (file-exists-p path)
      (delete-file path))))

(defun xcscope-test-project (name)
  (let* ((root (file-name-as-directory
                (expand-file-name name xcscope-test-root)))
         (release (expand-file-name "src/release.c" root))
         (main (expand-file-name "src/main.c" root))
         (header (expand-file-name "include/release.h" root)))
    (when (file-directory-p root)
      (delete-directory root t))
    (xcscope-test-write
     header
     "extern int release_count;\nvoid publish_release(void);\n")
    (xcscope-test-write
     release
     (concat
      "#include \"release.h\"\n"
      "\n"
      "static const char *release_name = \"v3\";\n"
      "\n"
      "/* Generated state follows. */\n"
      "int release_count = 3;\n"
      "\n"
      "void publish_release(void) {\n"
      "  release_count += 1;\n"
      "}\n"))
    (xcscope-test-write
     main
     (concat
      "#include \"release.h\"\n"
      "\n"
      "void validate_release(void) {\n"
      "  if (release_count > 0) publish_release();\n"
      "}\n"))
    (xcscope-test-write (expand-file-name "cscope.out" root) "fixture database\n")
    root))

(defun xcscope-test-kill-project-buffers (project)
  (dolist (buffer (buffer-list))
    (let ((file (buffer-file-name buffer)))
      (when (and file (file-in-directory-p file project))
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer)))))

(defun xcscope-test-cleanup (project)
  (when (and cscope-unix-index-process
             (process-live-p cscope-unix-index-process))
    (ignore-errors (delete-process cscope-unix-index-process)))
  (when-let ((buffer (get-buffer cscope-output-buffer-name)))
    (with-current-buffer buffer
      (when (and cscope-process (process-live-p cscope-process))
        (ignore-errors (delete-process cscope-process)))))
  (xcscope-test-kill-project-buffers project)
  (dolist (buffer-name
           (list cscope-output-buffer-name cscope-info-buffer-name
                 "*cscope-indexing-buffer*"))
    (when-let ((buffer (get-buffer buffer-name)))
      (kill-buffer buffer)))
  (when (file-directory-p project)
    (delete-directory project t))
  (dolist (log (list xcscope-test-calls-log-path
                     xcscope-test-misses-log-path))
    (when (file-exists-p log)
      (delete-file log))))

(defun xcscope-test-await-search ()
  (let ((deadline (+ (float-time) 10.0)))
    (while (and (with-current-buffer cscope-output-buffer-name cscope-process)
                (< (float-time) deadline))
      (accept-process-output nil 0.01))
    (when (with-current-buffer cscope-output-buffer-name cscope-process)
      (error "cscope search did not finish"))))

(defun xcscope-test-await-index ()
  (let ((deadline (+ (float-time) 10.0)))
    (while (and cscope-unix-index-process (< (float-time) deadline))
      (accept-process-output nil 0.01))
    (when cscope-unix-index-process
      (error "cscope index did not finish"))))

(defun xcscope-test-normalize (text project)
  (replace-regexp-in-string
   (regexp-quote (directory-file-name project))
   "[PROJECT]" text t t))

(defun xcscope-test-calls (project)
  (if (file-exists-p xcscope-test-calls-log-path)
      (split-string
       (xcscope-test-normalize
        (xcscope-test-read xcscope-test-calls-log-path) project)
       "\n" t)
    nil))

(defun xcscope-test-misses ()
  (and (file-exists-p xcscope-test-misses-log-path)
       (xcscope-test-read xcscope-test-misses-log-path)))

(defun xcscope-test-result-entries (project)
  (with-current-buffer cscope-output-buffer-name
    (let (entries)
      (save-excursion
        (goto-char (point-min))
        (while (< (point) (point-max))
          (let* ((start (line-beginning-position))
                 (file (get-text-property start 'cscope-file))
                 (line (get-text-property start 'cscope-line-number)))
            (when line
              (push
               (list (file-relative-name file project)
                     line
                     (get-text-property start 'cscope-fuzzy-search-text-regexp)
                     (get-text-property start 'face)
                     (buffer-substring-no-properties
                      (line-beginning-position) (line-end-position)))
               entries)))
          (forward-line 1)))
      (nreverse entries))))

(xcscope-test-install-cscope)
"####;

fn xcscope_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(XCSCOPE_MELPA_PIN, "xcscope.el")
        .expect("prepare pinned xcscope source below ./tmp")
        .with_prelude(XCSCOPE_TEST_PRELUDE)
        .with_timeout(XCSCOPE_TEST_TIMEOUT)
}

#[test]
fn xcscope_package_batch() {
    assert_oracle_batch_cases(
        xcscope_oracle(),
        "xcscope_package_batch",
        "xcscope_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
