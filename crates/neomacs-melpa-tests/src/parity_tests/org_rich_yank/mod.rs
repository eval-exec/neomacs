//! Practical parity for Org Rich Yank's public rich-paste workflows.
//!
//! The cases exercise real kill advice, Org source-language discovery,
//! source links, clipboard-link fallback, image delegation, indentation,
//! custom formatting, empty-kill failure, and public advice disable/re-enable
//! lifecycle without accessing a real GUI or external process.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ORG_RICH_YANK_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'org)
(require 'org-rich-yank)

(defconst ory406-test-source-sha256
  "ad9f24124b01d47941b2edb3ad7f2c63f94125a5886831bdfe65008c2efae019")

(defun ory406-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let* ((loaded (symbol-file 'org-rich-yank 'defun))
       (source (and loaded
                    (if (string-suffix-p ".elc" loaded)
                        (concat (file-name-sans-extension loaded) ".el")
                      loaded)))
       (directory (and source (file-name-directory source)))
       (payload
        (and directory
             (sort (seq-filter
                    (lambda (name)
                      (and (string-suffix-p ".el" name)
                           (not (string-suffix-p "-autoloads.el" name))
                           (not (string-suffix-p "-pkg.el" name))))
                    (directory-files directory nil nil t))
                   #'string<))))
  (unless (and (file-regular-p source)
               (not (file-symlink-p source))
               (equal (file-name-nondirectory source) "org-rich-yank.el")
               (equal payload '("org-rich-yank.el"))
               (equal (ory406-test-file-sha256 source)
                      ory406-test-source-sha256))
    (error "Unexpected installed Org Rich Yank source: %S %S" source payload)))

(unless (and (advice-member-p #'org-rich-yank--store #'kill-new)
             (advice-member-p #'org-rich-yank--store #'kill-append))
  (error "Org Rich Yank did not install its kill advice"))

(defvar ory406-test-root nil)
(defvar ory406-test-selection-plan nil)
(defvar ory406-test-selection-calls nil)

(defun ory406-test-normalize (value)
  (cond
   ((stringp value)
    (if ory406-test-root
        (replace-regexp-in-string
         (regexp-quote (directory-file-name ory406-test-root))
         "[ROOT]" value t t)
      (copy-sequence value)))
   ((consp value)
    (cons (ory406-test-normalize (car value))
          (ory406-test-normalize (cdr value))))
   ((vectorp value)
    (apply #'vector (mapcar #'ory406-test-normalize value)))
   (t value)))

(defun ory406-test-condition (condition)
  (list :type (car condition)
        :data (ory406-test-normalize (copy-tree (cdr condition)))
        :message (ory406-test-normalize (error-message-string condition))))

(defun ory406-test-write-file (root relative contents)
  (let ((file (expand-file-name relative root)))
    (unless (and (file-in-directory-p file root)
                 (not (equal file (directory-file-name root))))
      (error "Unsafe Org Rich Yank fixture path: %s" file))
    (make-directory (file-name-directory file) t)
    (with-temp-buffer
      (insert contents)
      (let ((coding-system-for-write 'utf-8-unix))
        (write-region (point-min) (point-max) file nil 'silent)))
    file))

(defun ory406-test-manifest (root)
  (mapcar
   (lambda (file)
     (unless (and (file-in-directory-p file root)
                  (file-regular-p file)
                  (not (file-symlink-p file)))
       (error "Unsafe Org Rich Yank fixture: %s" file))
     (list (file-relative-name file root)
           (ory406-test-file-sha256 file)))
   (sort (directory-files-recursively root "." nil nil nil) #'string-lessp)))

(defun ory406-test-window-state ()
  (mapcar (lambda (window)
            (list (window-buffer window) (window-point window)
                  (window-start window) (window-dedicated-p window)))
          (seq-mapcat (lambda (frame) (window-list frame 'nomini))
                      (frame-list))))

(defun ory406-test-park-buffer (name)
  (when-let* ((buffer (get-buffer name)))
    (let ((old-name (buffer-name buffer)))
      (with-current-buffer buffer
        (rename-buffer (format " *ory406-parked-%s*" (sxhash-eq buffer)) t))
      (cons buffer old-name))))

(defun ory406-test-gui-get-selection (selection target)
  (unless (eq selection 'CLIPBOARD)
    (error "Unexpected Org Rich Yank selection: %S" (list selection target)))
  (let ((entry (assq target ory406-test-selection-plan)))
    (unless entry
      (error "Unexpected Org Rich Yank clipboard target: %S" target))
    (push (list selection target) ory406-test-selection-calls)
    (copy-tree (cdr entry))))

(defun ory406-test-run (files selection-plan expected-selection-calls body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name "org-rich-yank/" sandbox))))
         (window-before (current-window-configuration))
         (window-state-before (ory406-test-window-state))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (advice-new-before
          (and (advice-member-p #'org-rich-yank--store #'kill-new) t))
         (advice-append-before
          (and (advice-member-p #'org-rich-yank--store #'kill-append) t))
         (kill-ring nil)
         (kill-ring-yank-pointer nil)
         (interprogram-cut-function nil)
         (interprogram-paste-function nil)
         (org-rich-yank--buffer nil)
         (org-rich-yank--lang nil)
         (org-rich-yank-add-target-indent t)
         (org-rich-yank-format-paste #'org-rich-yank--format-paste-default)
         (org-rich-yank-download-image nil)
         (org-rich-yank--clipboard-link-mime-types
          (copy-sequence org-rich-yank--clipboard-link-mime-types))
         (org-stored-links nil)
         (org-store-link-plist nil)
         (org-link-context-for-files 'line)
         (enable-dir-local-variables nil)
         (ory406-test-root root)
         (ory406-test-selection-plan selection-plan)
         (ory406-test-selection-calls nil)
         (default-directory default-directory)
         (message-log-max nil)
         (print-circle nil)
         (parked nil)
         (root-owned nil)
         fixture-before fixture-after result body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute Org Rich Yank sandbox root"))
              (when (file-exists-p root)
                (error "Org Rich Yank sandbox root already exists: %s" root))
              (dolist (name '(" *ory406-source*" " *ory406-target*"
                              " *ory406-other*"))
                (when-let* ((entry (ory406-test-park-buffer name)))
                  (push entry parked)))
              (make-directory root t)
              (setq root-owned t)
              (dolist (file files)
                (ory406-test-write-file root (car file) (cdr file)))
              (setq fixture-before (ory406-test-manifest root)
                    default-directory root)
              (setq result
                    (cl-letf (((symbol-function 'gui-get-selection)
                               #'ory406-test-gui-get-selection)
                              ((symbol-function 'gui-backend-get-selection)
                               (lambda (&rest arguments)
                                 (error "Unexpected Org Rich Yank image boundary: %S"
                                        arguments)))
                              ((symbol-function 'call-process)
                               (lambda (&rest arguments)
                                 (error "Unexpected Org Rich Yank process: %S"
                                        arguments)))
                              ((symbol-function 'process-file)
                               (lambda (&rest arguments)
                                 (error "Unexpected Org Rich Yank process-file: %S"
                                        arguments)))
                              ((symbol-function 'make-process)
                               (lambda (&rest arguments)
                                 (error "Unexpected Org Rich Yank process creation: %S"
                                        arguments)))
                              ((symbol-function 'make-network-process)
                               (lambda (&rest arguments)
                                 (error "Unexpected Org Rich Yank network: %S"
                                        arguments)))
                              ((symbol-function 'open-network-stream)
                               (lambda (&rest arguments)
                                 (error "Unexpected Org Rich Yank stream: %S"
                                        arguments))))
                      (funcall body root)))
              (unless (equal (nreverse ory406-test-selection-calls)
                             expected-selection-calls)
                (error "Unexpected Org Rich Yank clipboard calls: %S"
                       ory406-test-selection-calls))
              (setq fixture-after (ory406-test-manifest root))
              (unless (equal fixture-before fixture-after)
                (error "Org Rich Yank fixture changed: %S %S"
                       fixture-before fixture-after)))
          (error (setq body-error condition)))
      (unless (advice-member-p #'org-rich-yank--store #'kill-new)
        (condition-case condition
            (org-rich-yank-enable)
          (error (push (list :restore-advice condition) cleanup-errors))))
      (unless (advice-member-p #'org-rich-yank--store #'kill-append)
        (condition-case condition
            (org-rich-yank-enable)
          (error (push (list :restore-append-advice condition) cleanup-errors))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case condition
              (progn
                (set-process-query-on-exit-flag process nil)
                (delete-process process))
            (error (push (list :delete-process condition) cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case condition
              (progn
                (with-current-buffer buffer (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error (push (list :kill-buffer (buffer-name buffer) condition)
                         cleanup-errors)))))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (condition-case condition
              (cancel-timer timer)
            (error (push (list :cancel-timer condition) cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case condition
              (delete-frame frame t)
            (error (push (list :delete-frame condition) cleanup-errors)))))
      (condition-case condition
          (progn
            (when (buffer-live-p buffer-before) (set-buffer buffer-before))
            (set-window-configuration window-before))
        (error (push (list :restore-window condition) cleanup-errors)))
      (dolist (entry parked)
        (condition-case condition
            (let ((buffer (car entry)) (name (cdr entry)))
              (unless (buffer-live-p buffer)
                (error "Parked Org Rich Yank buffer died: %s" name))
              (with-current-buffer buffer (rename-buffer name t)))
          (error (push (list :restore-buffer condition) cleanup-errors))))
      (condition-case condition
          (when root-owned
            (when (file-exists-p root) (delete-directory root t)))
        (error (push (list :delete-root condition) cleanup-errors)))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case condition
              (progn
                (with-current-buffer buffer (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error (push (list :kill-reaction-buffer
                               (buffer-name buffer) condition)
                         cleanup-errors))))))
    (let ((cleanup
           (list :new-buffers
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
                 :window-restored (equal window-state-before
                                         (ory406-test-window-state))
                 :buffer-restored (eq buffer-before (current-buffer))
                 :advice-restored
                 (and (eq advice-new-before
                          (and (advice-member-p #'org-rich-yank--store #'kill-new) t))
                      (eq advice-append-before
                          (and (advice-member-p #'org-rich-yank--store
                                                #'kill-append) t)))
                 :body-error (and body-error (ory406-test-condition body-error))
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Org Rich Yank workflow failed: %S" cleanup)
        (list :result (ory406-test-normalize result) :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ORG_RICH_YANK_MELPA_PIN, "org-rich-yank.el")
        .expect("prepare exact shallow Org Rich Yank source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_yank_formats_a_file_source_with_language_link_and_indent() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_yank_formats_a_file_source_with_language_link_and_indent",
        r####"(ory406-test-run
 '(("source.js" . "const café = '界';\n#+end_src\n"))
 '((TARGETS))
 '((CLIPBOARD TARGETS))
 (lambda (root)
   (let* ((source (find-file-noselect (expand-file-name "source.js" root)))
          (target (generate-new-buffer " *ory406-target*")))
     (with-current-buffer source
       (js-mode)
       (goto-char (point-min))
       (push-mark (point-max) t t)
       (call-interactively #'kill-ring-save))
     (with-current-buffer target
       (org-mode)
       (insert "* Paste\n  ")
       (call-interactively #'org-rich-yank)
       (list :text (buffer-substring-no-properties (point-min) (point-max))
             :locus (list (= (point) (point-max))
                          (line-number-at-pos) (current-column))
             :source-buffer (eq org-rich-yank--buffer source)
             :language org-rich-yank--lang
             :kill (current-kill 0)
             :stored-links (copy-tree org-stored-links))))))"####,
        expect![[
            r#"OK (:result (:text "* Paste\n  #+begin_src js\n  const café = '界';\n  ,#+end_src\n  #+end_src\n  [[file:[ROOT]/source.js::const café = '界';]]\n  " :locus (t 7 2) :source-buffer t :language "js" :kill "const café = '界';\n#+end_src\n" :stored-links nil) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :advice-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_kill_append_inside_an_org_source_block_keeps_its_language() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_kill_append_inside_an_org_source_block_keeps_its_language",
        r####"(ory406-test-run
 '(("snippet.org" . "* Snippet\n#+begin_src python\nprint('seed')\n#+end_src\n"))
 '((TARGETS))
 '((CLIPBOARD TARGETS))
 (lambda (root)
   (let* ((source (find-file-noselect (expand-file-name "snippet.org" root)))
          (target (generate-new-buffer " *ory406-target*")))
     (with-current-buffer source
       (org-mode)
       (goto-char (point-min))
       (search-forward "print")
       (kill-new "print('café')")
       (kill-append "\nprint('界')" nil))
     (with-current-buffer target
       (org-mode)
       (call-interactively #'org-rich-yank)
       (list :text (buffer-substring-no-properties (point-min) (point-max))
             :source-buffer (eq org-rich-yank--buffer source)
             :language org-rich-yank--lang
             :kill (current-kill 0)
             :stored-links (copy-tree org-stored-links))))))"####,
        expect![[
            r##"OK (:result (:text "#+begin_src python\nprint('café')\nprint('界')\n#+end_src\n[[file:[ROOT]/snippet.org::*Snippet][Snippet]]\n" :source-buffer t :language "python" :kill "print('café')\nprint('界')" :stored-links nil) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :advice-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn public_clipboard_url_yanks_a_quote_and_clears_source_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_clipboard_url_yanks_a_quote_and_clears_source_metadata",
        r####"(ory406-test-run
 nil
 '((TARGETS . [text/uri-list])
   (text/uri-list . "https://example.test/release/界\n"))
 '((CLIPBOARD TARGETS) (CLIPBOARD text/uri-list))
 (lambda (_root)
   (let ((source (generate-new-buffer " *ory406-source*"))
         (target (generate-new-buffer " *ory406-target*")))
     (with-current-buffer source
       (fundamental-mode)
       (kill-new "Quoted café\n界\n"))
     (with-current-buffer target
       (org-mode)
       (call-interactively #'org-rich-yank)
       (list :text (buffer-substring-no-properties (point-min) (point-max))
             :source-buffer org-rich-yank--buffer
             :language org-rich-yank--lang
             :kill (current-kill 0))))))"####,
        expect![[
            r##"OK (:result (:text "#+begin_quote\nQuoted café\n界\n#+end_quote\nhttps://example.test/release/界\n" :source-buffer nil :language nil :kill "Quoted café\n界\n") :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :advice-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn public_advice_lifecycle_and_custom_formatter_recover() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_advice_lifecycle_and_custom_formatter_recover",
        r####"(ory406-test-run
 '(("custom.el" . ";;; custom.el --- fixture\n"))
 '((TARGETS))
 '((CLIPBOARD TARGETS))
 (lambda (root)
   (let* ((source (find-file-noselect (expand-file-name "custom.el" root)))
          (other (generate-new-buffer " *ory406-other*"))
          (target (generate-new-buffer " *ory406-target*"))
          formatter-calls disabled-state)
     (with-current-buffer source
       (emacs-lisp-mode)
       (kill-new "(message \"before\")"))
     (org-rich-yank-disable)
     (with-current-buffer other
       (fundamental-mode)
       (kill-new "disabled"))
     (setq disabled-state
           (list (eq org-rich-yank--buffer source)
                 org-rich-yank--lang
                 (and (advice-member-p #'org-rich-yank--store #'kill-new) t)
                 (and (advice-member-p #'org-rich-yank--store #'kill-append) t)))
     (org-rich-yank-enable)
     (with-current-buffer source
       (goto-char (point-min))
       (kill-new "(message \"recovered café\")\n"))
     (setq org-rich-yank-add-target-indent nil
           org-rich-yank-format-paste
           (lambda (language contents link)
             (let ((plain-link (substring-no-properties link)))
               (push (list language contents plain-link) formatter-calls)
               (format "<%s>\n%s\nSOURCE=%s" language contents plain-link))))
     (with-current-buffer target
       (org-mode)
       (insert "Existing text")
       (call-interactively #'org-rich-yank)
       (list :disabled disabled-state
             :enabled
             (list (and (advice-member-p #'org-rich-yank--store #'kill-new) t)
                   (and (advice-member-p #'org-rich-yank--store #'kill-append) t))
             :text (buffer-substring-no-properties (point-min) (point-max))
             :formatter-calls (nreverse formatter-calls)
             :source-buffer (eq org-rich-yank--buffer source)
             :language org-rich-yank--lang)))))"####,
        expect![[
            r#"OK (:result (:disabled (t "emacs-lisp" nil nil) :enabled (t t) :text "Existing text\n<emacs-lisp>\n(message \"recovered café\")\n\nSOURCE=[[file:[ROOT]/custom.el::;;; custom.el --- fixture]]\n" :formatter-calls (("emacs-lisp" "(message \"recovered café\")\n" "[[file:[ROOT]/custom.el::;;; custom.el --- fixture]]\n")) :source-buffer t :language "emacs-lisp") :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :advice-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_image_clipboard_delegates_without_inserting_text() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_image_clipboard_delegates_without_inserting_text",
        r####"(ory406-test-run
 nil
 nil
 nil
 (lambda (_root)
   (let ((target (generate-new-buffer " *ory406-target*"))
         (original-require (symbol-function 'require))
         image-calls download-calls)
     (cl-letf (((symbol-function 'require)
                (lambda (feature &optional filename noerror)
                  (if (eq feature 'org-download)
                      (progn
                        (unless (and (null filename) (eq noerror 'noerror))
                          (error "Unexpected org-download require: %S"
                                 (list feature filename noerror)))
                        t)
                    (funcall original-require feature filename noerror))))
               ((symbol-function 'gui-backend-get-selection)
                (lambda (selection target)
                  (unless (equal (list selection target)
                                 '(CLIPBOARD image/png))
                    (error "Unexpected image selection: %S"
                           (list selection target)))
                  (push (list selection target) image-calls)
                  "recorded-png-界"))
               ((symbol-function 'org-download-clipboard)
                (lambda ()
                  (push :clipboard download-calls)
                  :downloaded)))
       (with-current-buffer target
         (org-mode)
         (insert "Before image")
         (let ((before (list (buffer-substring-no-properties
                              (point-min) (point-max))
                             (point) (buffer-modified-p))))
           (setq org-rich-yank-download-image t)
           (call-interactively #'org-rich-yank)
           (list :before before
                 :after (list (buffer-substring-no-properties
                               (point-min) (point-max))
                              (point) (buffer-modified-p))
                 :image-calls (nreverse image-calls)
                 :download-calls (nreverse download-calls))))))))"####,
        expect![[
            r#"OK (:result (:before ("Before image" 13 t) :after ("Before image" 13 t) :image-calls ((CLIPBOARD image/png)) :download-calls (:clipboard)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :advice-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_empty_kill_ring_failure_is_atomic() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_empty_kill_ring_failure_is_atomic",
        r####"(ory406-test-run
 nil
 nil
 nil
 (lambda (_root)
   (let ((target (generate-new-buffer " *ory406-target*")))
     (with-current-buffer target
       (org-mode)
       (insert "Before failure")
       (goto-char 7)
       (set-buffer-modified-p nil)
       (let ((before (list (buffer-substring-no-properties
                            (point-min) (point-max))
                           (point) (buffer-modified-p)))
             failure)
         (condition-case condition
             (call-interactively #'org-rich-yank)
           (error (setq failure (ory406-test-condition condition))))
         (list :failure failure
               :before before
               :after (list (buffer-substring-no-properties
                             (point-min) (point-max))
                            (point) (buffer-modified-p))))))))"####,
        expect![[
            r#"OK (:result (:failure (:type error :data ("Kill ring is empty") :message "Kill ring is empty") :before ("Before failure" 7 nil) :after ("Before failure" 7 nil)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :advice-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn org_rich_yank_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_yank_formats_a_file_source_with_language_link_and_indent(),
        public_kill_append_inside_an_org_source_block_keeps_its_language(),
        public_clipboard_url_yanks_a_quote_and_clears_source_metadata(),
        public_advice_lifecycle_and_custom_formatter_recover(),
        public_image_clipboard_delegates_without_inserting_text(),
        public_empty_kill_ring_failure_is_atomic(),
    ];
    assert_oracle_batch_cases(oracle(), "org-rich-yank-rank406", "org-rich-yank", &cases);
}
