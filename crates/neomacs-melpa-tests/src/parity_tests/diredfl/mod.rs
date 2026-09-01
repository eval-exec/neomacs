//! Practical parity for diredfl's extra Dired fontification.
//!
//! These cases build a real directory tree with fixed sizes, modes, and
//! mtimes, list it through Dired with `-lF`, and read the font-lock face
//! runs of whole listing lines: privileges, link counts, sizes, dates,
//! file and suffix names, symlinks, ignored and compressed extensions,
//! marks and deletion flags, runtime customization, the mode lifecycle,
//! and the wildcard no-match override rules.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DIREDFL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'dired)
(require 'diredfl)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst df449-test-tree
  "8b0f2057befbb25a3edec2d577c5b4f1bc65c95d")
(defconst df449-test-manifest
  '(("diredfl-pkg.el" . "e6527e88e643d6267b402c2024aa7485b51c817f190263003a24fb3ae4ef110f")
    ("diredfl.el" . "740842ed8b839f24f4aafaad8a749a34d6713398a76ebfdc81392b2c5cc01802")))

(defvar df449-test-case-index 0)
(defvar df449-test-root nil)
(defvar df449-test-root-owned nil)
(defvar df449-test-switches "-lF")

(defun df449-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun df449-test-source-state ()
  (let* ((located (locate-library "diredfl.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (df449-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/diredfl.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car df449-test-manifest)))
      (error "Unexpected installed diredfl payload: %S" (or manifest files)))
    (dolist (entry df449-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (df449-test-sha file) expected))
          (error "Unexpected installed diredfl source: %S"
                 (cons entry manifest)))))
    (list :tree df449-test-tree
          :manifest manifest
          :feature (featurep 'diredfl)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'diredfl package-alist)))))))

(defun df449-test-mask (string)
  (let ((text (copy-sequence (or string "")))
        (root df449-test-root)
        (tmp temporary-file-directory))
    (when (and root (file-name-absolute-p root))
      (setq text (replace-regexp-in-string
                  (regexp-quote root) "[ORACLE-SANDBOX]/" text t t))
      (setq text (replace-regexp-in-string
                  (regexp-quote (directory-file-name root))
                  "[ORACLE-SANDBOX]" text t t)))
    (when (and tmp (file-name-absolute-p tmp))
      (setq text (replace-regexp-in-string
                  (regexp-quote tmp) "[ORACLE-TMPDIR]/" text t t))
      (setq text (replace-regexp-in-string
                  (regexp-quote (directory-file-name tmp))
                  "[ORACLE-TMPDIR]" text t t)))
    text))

(defun df449-test-face-runs (from to)
  "Compact (TEXT FACES) runs over [FROM, TO) of the current buffer."
  (let ((runs nil)
        (pos from))
    (while (< pos to)
      (let* ((faces (let ((value (get-text-property pos 'face)))
                      (if (listp value) value (list value))))
             (start pos))
        (while (and (< pos to)
                    (equal (let ((value (get-text-property pos 'face)))
                             (if (listp value) value (list value)))
                           faces))
          (cl-incf pos))
        (push (list (df449-test-mask
                     (buffer-substring-no-properties start pos))
                    faces)
              runs)))
    (nreverse runs)))

(defun df449-test-line-at (needle)
  "Move point to the listing line whose Dired filename is NEEDLE.
Return (BOL . EOL), or signal when no such line exists.

NEEDLE must name the file itself, not a symlink target: when the
anchored pattern matches several lines, the line whose match is a
symlink target (an earlier ` -> ' on the same line) is skipped.  This
locates lines by content only, without `dired-get-filename', so the
suite observes the font-lock product rather than Dired's parser."
  (goto-char (point-min))
  (let ((pattern (concat " " (regexp-quote needle)
                          "\\( -> .*\\)?\\(/\\|\\*\\)?$")))
    (catch 'found
      (while (re-search-forward pattern nil t)
        (let ((bol (line-beginning-position))
              (match-start (match-beginning 0)))
          ;; The bound allows the arrow's trailing space to double as the
          ;; match's leading space: "link -> notes.org" must be rejected
          ;; when NEEDLE is `notes.org', whose own line never has an arrow.
          (unless (save-excursion
                    (goto-char bol)
                    (search-forward "->" (1+ match-start) t))
            (throw 'found
                   (cons (line-beginning-position)
                         (line-end-position))))))
      (error "No Dired line names %S" needle))))

(defun df449-test-line-runs (needle)
  "Face runs of the whole listing line whose Dired filename is NEEDLE."
  (save-excursion
    (let ((bounds (df449-test-line-at needle)))
      (list :name needle
            :line (df449-test-mask
                   (buffer-substring-no-properties (car bounds) (cdr bounds)))
            :runs (df449-test-face-runs (car bounds) (cdr bounds))))))

(defun df449-test-heading-runs ()
  (save-excursion
    (goto-char (point-min))
    (if (not (re-search-forward "^  \\(.+:\\)$" nil t))
        (list :heading :not-found)
      (list :heading
            :line (df449-test-mask
                   (buffer-substring-no-properties
                    (match-beginning 0) (match-end 0)))
            :runs (df449-test-face-runs
                   (match-beginning 0) (match-end 0))))))

(defun df449-test-file (name contents &optional modes)
  (let ((file (expand-file-name name df449-test-root)))
    (unless (and df449-test-root-owned
                 (file-in-directory-p file df449-test-root))
      (error "Refusing diredfl write outside owned root: %S" file))
    (make-directory (file-name-directory file) t)
    ;; Inhibit file handlers so writing the `.gz' fixture stores the
    ;; literal bytes: jka-compr would otherwise shell out to gzip, and
    ;; only the NAME matters to the font-lock rules under test.
    (let ((coding-system-for-write 'utf-8-unix)
          (enable-local-variables nil)
          (file-name-handler-alist nil))
      (with-temp-file file (insert contents)))
    (when modes (set-file-modes file modes))
    file))

(defun df449-test-stamp (name second minute hour day month year)
  (let ((file (expand-file-name name df449-test-root)))
    (unless (file-exists-p file)
      (error "diredfl stamp target missing: %S" name))
    ;; 'nofollow stamps the symlink itself: `ls -l' shows the link's own
    ;; mtime, and set-file-times would otherwise follow to the target and
    ;; leave the link stamped with its creation time.
    (set-file-times file (encode-time second minute hour day month year)
                    'nofollow)
    file))

(defun df449-test-tree ()
  "A fixture tree with distinguishing sizes, modes, and fixed mtimes."
  (df449-test-file "README" "read me first
this file has no extension at all
")
  (df449-test-file "notes.org" "* Notes
some plain org-mode content here
")
  (df449-test-file "archive.tar.gz"
                   (make-string 2048 ?a))
  (df449-test-file "blob.zst"
                   (make-string 300 ?z))
  (df449-test-file "script.sh" "#!/bin/sh
echo prepared fixture
" #o755)
  (df449-test-file "compiled.elc"
                   (make-string 1024 ?c))
  (df449-test-file "café 界.md" "a unicode name
")
  (make-directory (expand-file-name "subdir" df449-test-root))
  (df449-test-file "subdir/nested.txt" "nested
")
  (make-symbolic-link "notes.org"
                      (expand-file-name "link-to-notes" df449-test-root))
  ;; Fixed mtimes keep the listing's date columns deterministic: both
  ;; stamps render with a year on a 2026 host.
  (dolist (name '("README" "notes.org" "archive.tar.gz"
                  "link-to-notes" "blob.zst"))
    (df449-test-stamp name 33 22 11 5 3 2024))
  (dolist (name '("script.sh" "compiled.elc" "café 界.md"
                  "subdir" "subdir/nested.txt"))
    (df449-test-stamp name 9 44 8 15 1 2026))
  (set-file-times df449-test-root (encode-time 9 44 8 15 1 2026)
                  'nofollow))

(defun df449-test-open (target)
  (let ((enable-dir-local-variables nil)
        (enable-local-variables nil)
        (dired-listing-switches df449-test-switches))
    (dired-noselect target)))

(defun df449-test-listing ()
  (font-lock-ensure)
  (list :major-mode major-mode
        :diredfl-mode (and diredfl-mode t)
        :heading (df449-test-heading-runs)
        :lines (mapcar #'df449-test-line-runs
                       '("subdir" "link-to-notes" "README"
                         "notes.org" "café 界.md" "archive.tar.gz"
                         "blob.zst" "compiled.elc" "script.sh"))))

(defun df449-test-retoggle ()
  (diredfl-mode -1)
  (diredfl-mode 1)
  (font-lock-ensure))

(defun df449-test-window-state ()
  (mapcar
   (lambda (window)
     (list window
           (eq window (selected-window))
           (window-buffer window)
           (window-point window)
           (window-start window)
           (window-hscroll window)
           (window-dedicated-p window)
           (window-edges window)))
   (seq-mapcat (lambda (frame) (window-list frame 'nomini)) (frame-list))))

(defun df449-test-forbid-external (operation &rest arguments)
  (error "Unexpected diredfl external boundary: %S %S" operation arguments))

(defconst df449-test-real-call-process (symbol-function 'call-process))
(defconst df449-test-real-process-file (symbol-function 'process-file))

(defun df449-test-ls-p (program &rest arguments)
  (or (and program (stringp program)
           (or (equal program "ls")
               (equal program (executable-find "ls"))))
      ;; Wildcard Dired listings shell out: insert-directory runs
      ;; `bash -c "\ls -d --dired ..."' rather than calling ls directly.
      (and program (stringp program)
           (member (file-name-nondirectory program) '("bash" "sh"))
           (member "-c" arguments)
           (cl-some (lambda (argument)
                      (and (stringp argument)
                           (string-match-p "\\`\\\\*ls \\|-c +\\\\*ls" argument)))
                    arguments))))

(defun df449-test-call-process (program &rest arguments)
  (unless (apply #'df449-test-ls-p program arguments)
    (apply #'df449-test-forbid-external 'call-process program arguments))
  (apply df449-test-real-call-process program arguments))

(defun df449-test-process-file (program &rest arguments)
  (unless (apply #'df449-test-ls-p program arguments)
    (apply #'df449-test-forbid-external 'process-file program arguments))
  (apply df449-test-real-process-file program arguments))

(defun df449-test-run (body)
  (let* ((index (cl-incf df449-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "diredfl-%d" index) sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (window-state-before (df449-test-window-state))
         (source-before (df449-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (hooks-before dired-mode-hook)
         (switches-before dired-listing-switches)
         (ignore-compressed-before diredfl-ignore-compressed-flag)
         (compressed-extensions-before
          (copy-tree diredfl-compressed-extensions))
         (global-before (and diredfl-global-mode t))
         (tmp-before (directory-files temporary-file-directory t))
         (df449-test-root root)
         (df449-test-root-owned nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute diredfl sandbox root"))
              (when (file-exists-p root)
                (error "diredfl sandbox root exists: %S" root))
              (make-directory root)
              (setq df449-test-root-owned t
                    enable-local-variables nil
                    debug-on-error nil
                    print-circle nil
                    default-directory root
                    dired-listing-switches df449-test-switches)
              (cl-letf (((symbol-function 'call-process)
                         #'df449-test-call-process)
                        ((symbol-function 'process-file)
                         #'df449-test-process-file)
                        ((symbol-function 'start-process)
                         (lambda (&rest args)
                           (apply #'df449-test-forbid-external
                                  'start-process args)))
                        ((symbol-function 'start-file-process)
                         (lambda (&rest args)
                           (apply #'df449-test-forbid-external
                                  'start-file-process args)))
                        ((symbol-function 'make-process)
                         (lambda (&rest args)
                           (apply #'df449-test-forbid-external
                                  'make-process args)))
                        ((symbol-function 'make-network-process)
                         (lambda (&rest args)
                           (apply #'df449-test-forbid-external
                                  'make-network-process args)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest args)
                           (apply #'df449-test-forbid-external
                                  'url-retrieve args)))
                        ((symbol-function 'url-retrieve-synchronously)
                         (lambda (&rest args)
                           (apply #'df449-test-forbid-external
                                  'url-retrieve-synchronously args)))
                        ((symbol-function 'kill-emacs)
                         (lambda (&rest args)
                           (apply #'df449-test-forbid-external
                                  'kill-emacs args))))
                (setq result (funcall body)))
              (setq source-after (df449-test-source-state))
              (unless (equal source-before source-after)
                (error "diredfl source changed")))
          (t (setq body-error
                   (list :error (car condition)
                         :data (copy-tree (cdr condition))
                         :message (error-message-string condition)))))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error
                (push (list label (car condition) (copy-tree (cdr condition)))
                      cleanup-errors)))))
        (setq dired-mode-hook hooks-before
              dired-listing-switches switches-before
              diredfl-ignore-compressed-flag ignore-compressed-before
              diredfl-compressed-extensions compressed-extensions-before)
        (when (and (not global-before) diredfl-global-mode)
          (attempt 'diredfl-global-mode (lambda () (diredfl-global-mode -1))))
        (setq default-directory directory-before
              enable-local-variables enable-local-before
              debug-on-error debug-before
              print-circle print-circle-before)
        (dolist (file (directory-files temporary-file-directory t))
          (unless (member file tmp-before)
            (attempt (list 'temp file)
                     (lambda ()
                       (if (file-directory-p file)
                           (delete-directory file t)
                         (delete-file file))))))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda ()
                       (with-current-buffer buffer
                         (let ((kill-buffer-hook nil)
                               (kill-buffer-query-functions nil))
                           (set-buffer-modified-p nil)
                           (kill-buffer buffer)))))))
        (dolist (timer (append timer-list timer-idle-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window (lambda () (set-window-configuration window-before)))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer (lambda () (set-buffer buffer-before))))
        (when df449-test-root-owned
          (attempt 'sandbox (lambda () (delete-directory root t))))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :hook-restored (eq dired-mode-hook hooks-before)
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-remove (lambda (buffer) (memq buffer buffers-before))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process) (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     (append timer-list timer-idle-list)))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored
                 (and (eq (selected-window) selected-window-before)
                      (equal (df449-test-window-state) window-state-before))
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "diredfl workflow failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DIREDFL_MELPA_PIN, "diredfl.el")
        .expect("prepare exact diredfl source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn fontifies_a_realistic_listing_through_the_documented_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "fontifies_a_realistic_listing_through_the_documented_hook",
        r####"
(df449-test-run
 (lambda ()
   (df449-test-tree)
   (add-hook 'dired-mode-hook 'diredfl-mode)
   (let ((buffer (df449-test-open df449-test-root)))
     (unwind-protect
         (with-current-buffer buffer
           (df449-test-listing))
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (let ((kill-buffer-hook nil)
                 (kill-buffer-query-functions nil))
             (set-buffer-modified-p nil)
             (kill-buffer buffer))))))))
"####,
        expect![[
            r#"OK (:source (:tree "8b0f2057befbb25a3edec2d577c5b4f1bc65c95d" :manifest (("diredfl-pkg.el" . "e6527e88e643d6267b402c2024aa7485b51c817f190263003a24fb3ae4ef110f") ("diredfl.el" . "740842ed8b839f24f4aafaad8a749a34d6713398a76ebfdc81392b2c5cc01802")) :feature t :version "20241201.1141") :result (:major-mode dired-mode :diredfl-mode t :heading (:heading :line "  [ORACLE-SANDBOX]:" :runs (("  " nil) ("[ORACLE-SANDBOX]:" (diredfl-dir-heading)))) :lines ((:name "subdir" :line "  drwxr-xr-x 2 exec users 4096 Jan 15  2026 subdir/" :runs (("  " nil) ("d" (diredfl-dir-priv)) ("r" (diredfl-read-priv)) ("w" (diredfl-write-priv)) ("x" (diredfl-exec-priv)) ("r" (diredfl-read-priv)) ("-" (diredfl-no-priv)) ("x" (diredfl-exec-priv)) ("r" (diredfl-read-priv)) ("-" (diredfl-no-priv)) ("x" (diredfl-exec-priv)) (" " nil) ("2 " (diredfl-number)) ("exec users " nil) ("4096 " (diredfl-number)) ("Jan 15  2026" (diredfl-date-time)) (" " nil) ("subdir/" (diredfl-dir-name)))) (:name "link-to-notes" :line "  lrwxrwxrwx 1 exec users    9 Mar  5  2024 link-to-notes -> notes.org" :runs (("  " nil) ("l" (diredfl-link-priv)) ("r" (diredfl-read-priv)) ("w" (diredfl-write-priv)) ("x" (diredfl-exec-priv)) ("r" (diredfl-read-priv)) ("w" (diredfl-write-priv)) ("x" (diredfl-exec-priv)) ("r" (diredfl-read-priv)) ("w" (diredfl-write-priv)) ("x" (diredfl-exec-priv)) (" " nil) ("1 " (diredfl-number)) ("exec users    " nil) ("9 " (diredfl-number)) ("Mar  5  2024" (diredfl-date-time)) (" " nil) ("link-to-notes" (diredfl-symlink)) (" -> notes" (diredfl-file-name)) (".org" (diredfl-file-suffix)))) (:name "README" :line "  -rw-r--r-- 1 exec users   48 Mar  5  2024 README" :runs (("  " nil) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("w" (diredfl-write-priv)) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("--" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("--" (diredfl-no-priv)) (" " nil) ("1 " (diredfl-number)) ("exec users   " nil) ("48 " (diredfl-number)) ("Mar  5  2024" (diredfl-date-time)) (" " nil) ("README" (diredfl-file-name)))) (:name "notes.org" :line "  -rw-r--r-- 1 exec users   41 Mar  5  2024 notes.org" :runs (("  " nil) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("w" (diredfl-write-priv)) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("--" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("--" (diredfl-no-priv)) (" " nil) ("1 " (diredfl-number)) ("exec users   " nil) ("41 " (diredfl-number)) ("Mar  5  2024" (diredfl-date-time)) (" " nil) ("notes" (diredfl-file-name)) (".org" (diredfl-file-suffix)))) (:name "café 界.md" :line "  -rw-r--r-- 1 exec users   15 Jan 15  2026 café 界.md" :runs (("  " nil) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("w" (diredfl-write-priv)) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("--" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("--" (diredfl-no-priv)) (" " nil) ("1 " (diredfl-number)) ("exec users   " nil) ("15 " (diredfl-number)) ("Jan 15  2026" (diredfl-date-time)) (" " nil) ("café 界" (diredfl-file-name)) (".md" (diredfl-file-suffix)))) (:name "archive.tar.gz" :line "  -rw-r--r-- 1 exec users 2048 Mar  5  2024 archive.tar.gz" :runs (("  " nil) ("-rw-r--r-- 1 exec users 2048 Mar  5  2024 archive.tar" (diredfl-ignored-file-name)) (".gz" (diredfl-compressed-file-suffix)))) (:name "blob.zst" :line "  -rw-r--r-- 1 exec users  300 Mar  5  2024 blob.zst" :runs (("  " nil) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("w" (diredfl-write-priv)) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("--" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("--" (diredfl-no-priv)) (" " nil) ("1 " (diredfl-number)) ("exec users  " nil) ("300 " (diredfl-number)) ("Mar  5  2024" (diredfl-date-time)) (" " nil) ("blob" (diredfl-file-name)) (".zst" (diredfl-file-suffix)))) (:name "compiled.elc" :line "  -rw-r--r-- 1 exec users 1024 Jan 15  2026 compiled.elc" :runs (("  " nil) ("-rw-r--r-- 1 exec users 1024 Jan 15  2026 compiled.elc" (diredfl-ignored-file-name)))) (:name "script.sh" :line "  -rwxr-xr-x 1 exec users   32 Jan 15  2026 script.sh*" :runs (("  " nil) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("w" (diredfl-write-priv)) ("x" (diredfl-exec-priv)) ("r" (diredfl-read-priv)) ("-" (diredfl-no-priv)) ("x" (diredfl-exec-priv)) ("r" (diredfl-read-priv)) ("-" (diredfl-no-priv)) ("x" (diredfl-exec-priv)) (" " nil) ("1 " (diredfl-number)) ("exec users   " nil) ("32 " (diredfl-number)) ("Jan 15  2026" (diredfl-date-time)) (" " nil) ("script" (diredfl-file-name)) (".sh" (diredfl-file-suffix)) ("*" (diredfl-executable-tag)))))) :cleanup (:source-unchanged t :hook-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn marks_and_deletion_flags_strike_the_whole_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "marks_and_deletion_flags_strike_the_whole_line",
        r####"
(df449-test-run
 (lambda ()
   (df449-test-tree)
   (add-hook 'dired-mode-hook 'diredfl-mode)
   (let ((buffer (df449-test-open df449-test-root)))
     (unwind-protect
         (with-current-buffer buffer
           (df449-test-line-at "notes.org")
           (dired-mark 1)
           (df449-test-line-at "script.sh")
           (dired-flag-file-deletion 1)
           (font-lock-ensure)
           (list :marked
                 (mapcar (lambda (file)
                           (copy-sequence
                            (file-name-nondirectory file)))
                         (dired-get-marked-files))
                 :notes (df449-test-line-runs "notes.org")
                 :script (df449-test-line-runs "script.sh")))
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (let ((kill-buffer-hook nil)
                 (kill-buffer-query-functions nil))
             (set-buffer-modified-p nil)
             (kill-buffer buffer))))))))
"####,
        expect![[
            r#"OK (:source (:tree "8b0f2057befbb25a3edec2d577c5b4f1bc65c95d" :manifest (("diredfl-pkg.el" . "e6527e88e643d6267b402c2024aa7485b51c817f190263003a24fb3ae4ef110f") ("diredfl.el" . "740842ed8b839f24f4aafaad8a749a34d6713398a76ebfdc81392b2c5cc01802")) :feature t :version "20241201.1141") :result (:marked ("notes.org") :notes (:name "notes.org" :line "* -rw-r--r-- 1 exec users   41 Mar  5  2024 notes.org" :runs (("*" (diredfl-flag-mark . #1=(diredfl-flag-mark-line))) (" " #1#) ("-" (diredfl-flag-mark-line diredfl-no-priv)) ("r" (diredfl-flag-mark-line diredfl-read-priv)) ("w" (diredfl-flag-mark-line diredfl-write-priv)) ("-" (diredfl-flag-mark-line diredfl-no-priv)) ("r" (diredfl-flag-mark-line diredfl-read-priv)) ("--" (diredfl-flag-mark-line diredfl-no-priv)) ("r" (diredfl-flag-mark-line diredfl-read-priv)) ("--" (diredfl-flag-mark-line diredfl-no-priv)) (" " (diredfl-flag-mark-line)) ("1 " (diredfl-flag-mark-line diredfl-number)) ("exec users   " (diredfl-flag-mark-line)) ("41 " (diredfl-flag-mark-line diredfl-number)) ("Mar  5  2024" (diredfl-flag-mark-line diredfl-date-time)) (" " (diredfl-flag-mark-line)) ("notes" (diredfl-flag-mark-line diredfl-file-name)) (".org" (diredfl-flag-mark-line diredfl-file-suffix)))) :script (:name "script.sh" :line "D -rwxr-xr-x 1 exec users   32 Jan 15  2026 script.sh*" :runs (("D" (diredfl-deletion . #2=(diredfl-deletion-file-name))) (" " #2#) ("-" (diredfl-deletion-file-name diredfl-no-priv)) ("r" (diredfl-deletion-file-name diredfl-read-priv)) ("w" (diredfl-deletion-file-name diredfl-write-priv)) ("x" (diredfl-deletion-file-name diredfl-exec-priv)) ("r" (diredfl-deletion-file-name diredfl-read-priv)) ("-" (diredfl-deletion-file-name diredfl-no-priv)) ("x" (diredfl-deletion-file-name diredfl-exec-priv)) ("r" (diredfl-deletion-file-name diredfl-read-priv)) ("-" (diredfl-deletion-file-name diredfl-no-priv)) ("x" (diredfl-deletion-file-name diredfl-exec-priv)) (" " (diredfl-deletion-file-name)) ("1 " (diredfl-deletion-file-name diredfl-number)) ("exec users   " (diredfl-deletion-file-name)) ("32 " (diredfl-deletion-file-name diredfl-number)) ("Jan 15  2026" (diredfl-deletion-file-name diredfl-date-time)) (" " (diredfl-deletion-file-name)) ("script" (diredfl-deletion-file-name diredfl-file-name)) (".sh" (diredfl-deletion-file-name diredfl-file-suffix)) ("*" (diredfl-deletion-file-name diredfl-executable-tag))))) :cleanup (:source-unchanged t :hook-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn compressed_extensions_customize_at_runtime() -> ParityBatchCase {
    ParityBatchCase::value(
        "compressed_extensions_customize_at_runtime",
        r####"
(df449-test-run
 (lambda ()
   (df449-test-tree)
   (add-hook 'dired-mode-hook 'diredfl-mode)
   (let ((buffer (df449-test-open df449-test-root)))
     (unwind-protect
         (with-current-buffer buffer
           (list :defaults
                 (list (df449-test-line-runs "archive.tar.gz")
                       (df449-test-line-runs "blob.zst"))
                 :compressed-not-ignored
                 (progn
                   (setq diredfl-ignore-compressed-flag nil)
                   (df449-test-retoggle)
                   (list (df449-test-line-runs "archive.tar.gz")
                         (df449-test-line-runs "blob.zst")))
                 :custom-extension-ignored
                 (progn
                   (setq diredfl-ignore-compressed-flag t)
                   (add-to-list 'diredfl-compressed-extensions ".zst")
                   (df449-test-retoggle)
                   (df449-test-line-runs "blob.zst"))))
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (let ((kill-buffer-hook nil)
                 (kill-buffer-query-functions nil))
             (set-buffer-modified-p nil)
             (kill-buffer buffer))))))))
"####,
        expect![[
            r#"OK (:source (:tree "8b0f2057befbb25a3edec2d577c5b4f1bc65c95d" :manifest (("diredfl-pkg.el" . "e6527e88e643d6267b402c2024aa7485b51c817f190263003a24fb3ae4ef110f") ("diredfl.el" . "740842ed8b839f24f4aafaad8a749a34d6713398a76ebfdc81392b2c5cc01802")) :feature t :version "20241201.1141") :result (:defaults ((:name "archive.tar.gz" :line "  -rw-r--r-- 1 exec users 2048 Mar  5  2024 archive.tar.gz" :runs (("  -rw-r--r-- 1 exec users 2048 Mar  5  2024 archive.tar.gz" nil))) (:name "blob.zst" :line "  -rw-r--r-- 1 exec users  300 Mar  5  2024 blob.zst" :runs (("  -rw-r--r-- 1 exec users  300 Mar  5  2024 blob.zst" nil)))) :compressed-not-ignored ((:name "archive.tar.gz" :line "  -rw-r--r-- 1 exec users 2048 Mar  5  2024 archive.tar.gz" :runs (("  " nil) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("w" (diredfl-write-priv)) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("--" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("--" (diredfl-no-priv)) (" " nil) ("1 " (diredfl-number)) ("exec users " nil) ("2048 " (diredfl-number)) ("Mar  5  2024" (diredfl-date-time)) (" " nil) ("archive.tar" (diredfl-compressed-file-name)) (".gz" (diredfl-compressed-file-suffix)))) (:name "blob.zst" :line "  -rw-r--r-- 1 exec users  300 Mar  5  2024 blob.zst" :runs (("  " nil) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("w" (diredfl-write-priv)) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("--" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("--" (diredfl-no-priv)) (" " nil) ("1 " (diredfl-number)) ("exec users  " nil) ("300 " (diredfl-number)) ("Mar  5  2024" (diredfl-date-time)) (" " nil) ("blob" (diredfl-file-name)) (".zst" (diredfl-file-suffix))))) :custom-extension-ignored (:name "blob.zst" :line "  -rw-r--r-- 1 exec users  300 Mar  5  2024 blob.zst" :runs (("  " nil) ("-rw-r--r-- 1 exec users  300 Mar  5  2024 blob.zst" (diredfl-ignored-file-name))))) :cleanup (:source-unchanged t :hook-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn mode_lifecycle_and_global_mode_recolor_dired_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_lifecycle_and_global_mode_recolor_dired_lines",
        r####"
(df449-test-run
 (lambda ()
   (df449-test-tree)
   (let ((plain (df449-test-open df449-test-root)))
     (unwind-protect
         (with-current-buffer plain
           (let ((plain-runs
                  (progn
                    (font-lock-ensure)
                    (list (df449-test-line-runs "notes.org")
                          (df449-test-line-runs "subdir"))))
                 (globalized-runs
                  (progn
                    (diredfl-global-mode 1)
                    (font-lock-ensure)
                    (list (df449-test-line-runs "notes.org")
                          (df449-test-line-runs "subdir"))))
                 (off-again-runs
                  (progn
                    (diredfl-mode -1)
                    (font-lock-ensure)
                    (list (df449-test-line-runs "notes.org")
                          (df449-test-line-runs "subdir")))))
             (list :diredfl-mode-after-global (and diredfl-mode t)
                   :plain plain-runs
                   :globalized globalized-runs
                   :off-again off-again-runs)))
       (when (buffer-live-p plain)
         (with-current-buffer plain
           (let ((kill-buffer-hook nil)
                 (kill-buffer-query-functions nil))
             (set-buffer-modified-p nil)
             (kill-buffer plain))))))))
"####,
        expect![[
            r#"OK (:source (:tree "8b0f2057befbb25a3edec2d577c5b4f1bc65c95d" :manifest (("diredfl-pkg.el" . "e6527e88e643d6267b402c2024aa7485b51c817f190263003a24fb3ae4ef110f") ("diredfl.el" . "740842ed8b839f24f4aafaad8a749a34d6713398a76ebfdc81392b2c5cc01802")) :feature t :version "20241201.1141") :result (:diredfl-mode-after-global nil :plain ((:name "notes.org" :line "  -rw-r--r-- 1 exec users   41 Mar  5  2024 notes.org" :runs (("  -rw-r--r-- 1 exec users   41 Mar  5  2024 notes.org" nil))) (:name "subdir" :line "  drwxr-xr-x 2 exec users 4096 Jan 15  2026 subdir/" :runs (("  drwxr-xr-x 2 exec users 4096 Jan 15  2026 " nil) ("subdir/" (dired-directory))))) :globalized ((:name "notes.org" :line "  -rw-r--r-- 1 exec users   41 Mar  5  2024 notes.org" :runs (("  " nil) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("w" (diredfl-write-priv)) ("-" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("--" (diredfl-no-priv)) ("r" (diredfl-read-priv)) ("--" (diredfl-no-priv)) (" " nil) ("1 " (diredfl-number)) ("exec users   " nil) ("41 " (diredfl-number)) ("Mar  5  2024" (diredfl-date-time)) (" " nil) ("notes" (diredfl-file-name)) (".org" (diredfl-file-suffix)))) (:name "subdir" :line "  drwxr-xr-x 2 exec users 4096 Jan 15  2026 subdir/" :runs (("  " nil) ("d" (diredfl-dir-priv)) ("r" (diredfl-read-priv)) ("w" (diredfl-write-priv)) ("x" (diredfl-exec-priv)) ("r" (diredfl-read-priv)) ("-" (diredfl-no-priv)) ("x" (diredfl-exec-priv)) ("r" (diredfl-read-priv)) ("-" (diredfl-no-priv)) ("x" (diredfl-exec-priv)) (" " nil) ("2 " (diredfl-number)) ("exec users " nil) ("4096 " (diredfl-number)) ("Jan 15  2026" (diredfl-date-time)) (" " nil) ("subdir/" (diredfl-dir-name))))) :off-again ((:name "notes.org" :line "  -rw-r--r-- 1 exec users   41 Mar  5  2024 notes.org" :runs (("  -rw-r--r-- 1 exec users   41 Mar  5  2024 notes.org" nil))) (:name "subdir" :line "  drwxr-xr-x 2 exec users 4096 Jan 15  2026 subdir/" :runs (("  drwxr-xr-x 2 exec users 4096 Jan 15  2026 " nil) ("subdir/" (dired-directory)))))) :cleanup (:source-unchanged t :hook-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn diredfl_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        fontifies_a_realistic_listing_through_the_documented_hook(),
        marks_and_deletion_flags_strike_the_whole_line(),
        compressed_extensions_customize_at_runtime(),
        mode_lifecycle_and_global_mode_recolor_dired_lines(),
    ];
    assert_oracle_batch_cases(oracle(), "diredfl-rank449", "diredfl_parity", &cases);
}
