use std::time::Duration;

use crate::{AC_DCD_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_DCD_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// ac-dcd is the auto-complete source for D: it shells out to the `dcd-client`
/// and `dcd-server` programs of the D Completion Daemon, parses their output
/// into auto-complete candidates, and renders documentation, symbol search and
/// goto-definition results.
///
/// Only the external programs are replaced below -- `dcd-client`, `dcd-server`,
/// `dub` and `pidof`.  Each stand-in records the exact argument vector it was
/// given (and, for dcd-client, the stdin it received) and answers with
/// realistic canned DCD output.  auto-complete, the ac-dcd parser, the buffers
/// ac-dcd renders, the marker ring, the real D files in the sandbox and the
/// real flycheck-dmd-dub import discovery all run for real.  `d-mode` is a
/// separate package that ac-dcd does not depend on, so the prelude supplies the
/// mode symbol, its keymap and D's C-style comment/string syntax.
const AC_DCD_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'auto-complete)

(setq make-backup-files nil
      create-lockfiles nil)

(defvar ac-dcd-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defvar ac-dcd-test-bin
  (file-name-as-directory (expand-file-name "bin" ac-dcd-test-root)))

(defvar ac-dcd-test-replies
  (file-name-as-directory (expand-file-name "replies" ac-dcd-test-root)))

(defvar d-mode-syntax-table
  (let ((table (make-syntax-table)))
    (modify-syntax-entry ?_ "_" table)
    (modify-syntax-entry ?/ ". 124" table)
    (modify-syntax-entry ?* ". 23b" table)
    (modify-syntax-entry ?\n ">" table)
    (modify-syntax-entry ?\" "\"" table)
    table)
  "C-style syntax, as the real `d-mode' inherits from cc-mode.")

(define-derived-mode d-mode prog-mode "D"
  "Stand-in for the separately distributed `d-mode'."
  :syntax-table d-mode-syntax-table)

;; The real d-mode autoload claims *.d files.
(add-to-list 'auto-mode-alist '("\\.d\\'" . d-mode))

(defun ac-dcd-test-write-file (path contents)
  (make-directory (file-name-directory path) t)
  (with-temp-buffer
    (insert contents)
    (write-region (point-min) (point-max) path nil 'silent))
  path)

(defun ac-dcd-test-executable (name body)
  (let ((path (expand-file-name name ac-dcd-test-bin)))
    (ac-dcd-test-write-file path body)
    (set-file-modes path #o755)
    path))

(defun ac-dcd-test-install-tools ()
  "Install the recording dcd-client/dcd-server/dub/pidof stand-ins.
Returns the dcd-client path, which is also stored in `ac-dcd-executable'."
  (make-directory ac-dcd-test-replies t)
  (setq ac-dcd-executable
        (ac-dcd-test-executable
         "dcd-client"
         (concat
          "#!/bin/sh\n"
          "dir=\"$AC_DCD_TEST_DIR\"\n"
          "log=\"$dir/client.log\"\n"
          "n=0\n"
          "if [ -f \"$log\" ]; then n=$(wc -l < \"$log\" | tr -d ' '); fi\n"
          "cat > \"$dir/stdin-$n.txt\"\n"
          "mode=complete\n"
          "for a in \"$@\"; do\n"
          "  case \"$a\" in\n"
          "    -d) mode=doc ;;\n"
          "    -l) mode=location ;;\n"
          "    --search) mode=search ;;\n"
          "    --version) mode=version ;;\n"
          "    -I*) mode=imports ;;\n"
          "  esac\n"
          "done\n"
          "{ for a in \"$@\"; do printf '[%s]' \"$a\"; done; printf '\\n'; } >> \"$log\"\n"
          "if [ -f \"$dir/replies/reply-$mode\" ]; then cat \"$dir/replies/reply-$mode\"; fi\n"
          "if [ -f \"$dir/replies/status-$mode\" ]; then exit \"$(cat \"$dir/replies/status-$mode\")\"; fi\n"
          "exit 0\n")))
  (setq ac-dcd-server-executable
        (ac-dcd-test-executable
         "dcd-server"
         (concat
          "#!/bin/sh\n"
          "{ for a in \"$@\"; do printf '[%s]' \"$a\"; done; printf '\\n'; } "
          ">> \"$AC_DCD_TEST_DIR/server.log\"\n"
          "sleep 30\n")))
  (ac-dcd-test-executable
   "dub"
   (concat
    "#!/bin/sh\n"
    "{ for a in \"$@\"; do printf '[%s]' \"$a\"; done; printf '\\n'; } "
    ">> \"$AC_DCD_TEST_DIR/dub.log\"\n"
    "if [ -f \"$AC_DCD_TEST_DIR/replies/dub-describe.json\" ]; then\n"
    "  cat \"$AC_DCD_TEST_DIR/replies/dub-describe.json\"\n"
    "fi\n"))
  ;; Real pidof exits non-zero when the daemon is not running.
  (ac-dcd-test-executable "pidof" "#!/bin/sh\nexit 1\n")
  (setenv "AC_DCD_TEST_DIR" (directory-file-name ac-dcd-test-root))
  (setenv "PATH" (concat (directory-file-name ac-dcd-test-bin)
                         path-separator (getenv "PATH")))
  (push (directory-file-name ac-dcd-test-bin) exec-path)
  ac-dcd-executable)

(defun ac-dcd-test-reply (mode text &optional status)
  "Make the dcd-client stand-in answer TEXT for MODE, exiting with STATUS."
  (ac-dcd-test-write-file
   (expand-file-name (concat "reply-" mode) ac-dcd-test-replies) text)
  (when status
    (ac-dcd-test-write-file
     (expand-file-name (concat "status-" mode) ac-dcd-test-replies)
     (number-to-string status))))

(defun ac-dcd-test-log-lines (name)
  (let ((path (expand-file-name name ac-dcd-test-root)))
    (if (file-regular-p path)
        (with-temp-buffer
          (let ((coding-system-for-read 'utf-8))
            (insert-file-contents path))
          (split-string (buffer-string) "\n" t))
      nil)))

(defun ac-dcd-test-parse-argv (line)
  "Split a recorded \"[a][b]\" argument line back into a list."
  (let ((start 0) (result nil))
    (while (string-match "\\[\\([^]]*\\)\\]" line start)
      (push (match-string 1 line) result)
      (setq start (match-end 0)))
    (nreverse result)))

(defun ac-dcd-test-file-contents (path)
  (if (file-regular-p path)
      (with-temp-buffer
        (let ((coding-system-for-read 'utf-8))
          (insert-file-contents path))
        (buffer-string))
    'no-such-file))

(defun ac-dcd-test-client-calls ()
  "Every dcd-client invocation as (ARGV . STDIN), oldest first."
  (let ((lines (ac-dcd-test-log-lines "client.log"))
        (index -1))
    (mapcar (lambda (line)
              (setq index (1+ index))
              (cons (ac-dcd-test-parse-argv line)
                    (ac-dcd-test-file-contents
                     (expand-file-name (format "stdin-%d.txt" index)
                                       ac-dcd-test-root))))
            lines)))

(defun ac-dcd-test-server-calls ()
  (mapcar #'ac-dcd-test-parse-argv (ac-dcd-test-log-lines "server.log")))

(defun ac-dcd-test-dub-calls ()
  (mapcar #'ac-dcd-test-parse-argv (ac-dcd-test-log-lines "dub.log")))

(defun ac-dcd-test-buffer-text (name)
  (let ((buffer (get-buffer name)))
    (if buffer
        (with-current-buffer buffer
          (buffer-substring-no-properties (point-min) (point-max)))
      'no-such-buffer)))

(defun ac-dcd-test-displayed-buffers ()
  (sort (mapcar (lambda (window) (buffer-name (window-buffer window)))
                (window-list nil 'never))
        #'string<))

(defun ac-dcd-test-last-message ()
  (let ((buffer (get-buffer "*Messages*")))
    (and buffer
         (with-current-buffer buffer
           (car (last (split-string (buffer-substring-no-properties
                                     (point-min) (point-max))
                                    "\n" t)))))))

(defun ac-dcd-test-source (name text)
  "Write a real D source file below the sandbox and return its path."
  (ac-dcd-test-write-file (expand-file-name name ac-dcd-test-root) text))

(defun ac-dcd-test-cleanup ()
  (dolist (process (process-list))
    (set-process-query-on-exit-flag process nil)
    (delete-process process))
  (let ((kill-buffer-query-functions nil))
    (dolist (name (list ac-dcd-output-buffer-name
                        ac-dcd-error-buffer-name
                        ac-dcd-document-buffer-name
                        ac-dcd-search-symbol-buffer-name
                        " *dcd-server*"))
      (when (get-buffer name)
        (with-current-buffer name (set-buffer-modified-p nil))
        (kill-buffer name)))))

(defmacro ac-dcd-test-in-source (path &rest body)
  "Visit PATH in a window-displayed d-mode buffer with ac-dcd armed."
  `(let ((buffer (find-file-noselect ,path)))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (d-mode)
           (auto-complete-mode 1)
           (setq ac-sources '(ac-source-dcd))
           ,@body)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer (set-buffer-modified-p nil))
         (kill-buffer buffer))
       (ac-dcd-test-cleanup))))

(defun ac-dcd-test-complete-at (position)
  "Start a fresh ac-dcd completion at POSITION and return the candidate names."
  (ac-stop)
  (goto-char position)
  (ac-start :force-init t)
  (ac-update t)
  (mapcar #'substring-no-properties ac-candidates))
"##;

fn ac_dcd_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_DCD_MELPA_PIN, "ac-dcd.el")
        .expect("prepare pinned ac-dcd source below ./tmp")
        .with_prelude(AC_DCD_TEST_PRELUDE)
        .with_timeout(AC_DCD_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed ac-dcd parity test").into()
}

/// Multi-probe batch for `assert_ac_dcd_parity` cases (2a).
pub(crate) fn assert_ac_dcd_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_dcd_oracle(), &name, "ac_dcd_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_dcd_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_dcd_batch(&cases);
}

// END generated package batch tests
