//! Practical parity for Gptel's offline, user-visible workflows.
//!
//! The corpus drives real chat buffers, request construction, context, tools,
//! presets, Org integration, and response navigation while rejecting every
//! network and child-process boundary.

use std::time::Duration;

use expect_test::expect;

use crate::{COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, GPTEL_MELPA_PIN, TRANSIENT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'gptel)
(require 'gptel-org)

(defconst gptel402-test-source-manifest
  '(("gptel-anthropic.el" . "092b9afb132b83012c6c1f4918b2f4f76a75dbf1726766995bb6c860b2ced091")
    ("gptel-bedrock.el" . "bf308de02d37e035dc7ae53d5f601c74f85b59d47609bee09e102d859a8b3e38")
    ("gptel-context.el" . "9a30c26f9596639422821059f7b8adcd4ce0f6c8b0b5ac83986d878d22c0d2ff")
    ("gptel-gemini.el" . "39f12fbc5907256b6eeffe533a60e33dfd226fd74a949b4e426a451d17b4452d")
    ("gptel-gh.el" . "ef6f03b535b56f68066a7ab02deed769e7cca0ce2849d1c2b1525bde59eaa667")
    ("gptel-integrations.el" . "92040a52db21cd9ad80641e0ac13c8105e50e45e83ba3fe8d6b9bf7252709ac2")
    ("gptel-kagi.el" . "14d9da5527fb1b8666962e485eb6e500cb5730e22d7eef86bf27daba4f7867b9")
    ("gptel-oauth.el" . "d1ca33b6e8fd22caabdeabbb782d7dafee1d078f02cb23af667a8c6bb4cc1486")
    ("gptel-ollama.el" . "d848e914b54bcc6d64eb2ef808be76f5c7b141ba77d3fbedde53a183a5107b33")
    ("gptel-openai-extras.el" . "98b5dd587bf1e8bcdf3ea718223da4b5a8a39795b15d44151e0b5652b34071ef")
    ("gptel-openai-oauth.el" . "8195d620cb11f95824b7f9355109eb62bbe85eca7f2e955a810fe5aa06daebdc")
    ("gptel-openai-responses.el" . "e2a82f23e745b4025217c81b4d725e9d022c17103808a7ce3bbe78ca240fb159")
    ("gptel-openai.el" . "9f5481300aa8a4df173d747072580b09afc9ba4f9c826299327dba3c981dfa40")
    ("gptel-org.el" . "01cdfeb433c90813c8067de13242c35b06b85d2b9e3835c7060e530b506d4bbb")
    ("gptel-request.el" . "9b53dc8204fa683562643e3e45211dd9c7f7a105d677e77bea4ef3ef50a20df3")
    ("gptel-rewrite.el" . "81add49016517c4c907d7990bfc8e81d95aacb5fcf3a46e6aee92bd1043c91a8")
    ("gptel-transient.el" . "cff4e58e6f5280542f1e77207061b040dec01958b7c453dd0ccc7640e874fce1")
    ("gptel.el" . "4c15cdd05a219aa4232f4e78ca9576087fd5c94a7efaec02d6407e58bf7ac773")))

(defun gptel402-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let* ((main (symbol-file 'gptel 'defun))
       (directory (and main (file-name-directory main)))
       (payload
        (and directory
             (sort (seq-filter
                    (lambda (name)
                      (and (string-prefix-p "gptel" name)
                           (string-suffix-p ".el" name)
                           (not (string-suffix-p "-autoloads.el" name))
                           (not (string-suffix-p "-pkg.el" name))))
                    (directory-files directory nil nil t))
                   #'string<))))
  (unless (and (file-regular-p main)
               (equal payload (mapcar #'car gptel402-test-source-manifest))
               (cl-every
                (lambda (entry)
                  (let ((file (expand-file-name (car entry) directory)))
                    (and (file-regular-p file)
                         (not (file-symlink-p file))
                         (equal (gptel402-test-file-sha256 file) (cdr entry)))))
                gptel402-test-source-manifest))
    (error "Unexpected installed gptel payload: %S" (list main payload))))

(defun gptel402-test-normalize (value root)
  (cond
   ((stringp value)
    (if root
        (replace-regexp-in-string
         (regexp-quote (directory-file-name root)) "[ROOT]" value t t)
      (copy-sequence value)))
   ((markerp value)
    (list :marker (marker-position value)
          :buffer (and (marker-buffer value) (buffer-name (marker-buffer value)))))
   ((consp value)
    (cons (gptel402-test-normalize (car value) root)
          (gptel402-test-normalize (cdr value) root)))
   ((vectorp value)
    (apply #'vector (mapcar (lambda (item) (gptel402-test-normalize item root)) value)))
   (t value)))

(defun gptel402-test-condition (condition root)
  (list :error (car condition)
        :data (gptel402-test-normalize (copy-tree (cdr condition)) root)))

(defun gptel402-test-write-file (root relative content)
  (let ((file (expand-file-name relative root)))
    (unless (file-in-directory-p file root)
      (error "Refusing gptel fixture outside root: %s" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file file (insert content)))
    file))

(defun gptel402-test-manifest (root)
  (let (entries)
    (dolist (file (directory-files-recursively root "." nil nil t))
      (when (file-regular-p file)
        (push (cons (file-relative-name file root)
                    (gptel402-test-file-sha256 file)) entries)))
    (sort entries (lambda (left right) (string< (car left) (car right))))))

(defun gptel402-test-window-state ()
  (mapcar (lambda (window)
            (list (buffer-name (window-buffer window))
                  (window-point window) (window-start window)
                  (window-dedicated-p window)))
          (seq-mapcat (lambda (frame) (window-list frame 'nomini)) (frame-list))))

(defun gptel402-test-park-buffer (name)
  (when-let ((buffer (get-buffer name)))
    (let ((parked (generate-new-buffer-name (concat " *parked " name "*"))))
      (with-current-buffer buffer (rename-buffer parked t))
      (cons buffer name))))

(defun gptel402-test-forbid-external (kind &rest arguments)
  (error "Unexpected external gptel boundary: %S" (cons kind arguments)))

(defun gptel402-test-run (files body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox (file-name-as-directory (expand-file-name "gptel/" sandbox))))
         (window-before (current-window-configuration))
         (window-state-before (gptel402-test-window-state))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (gptel-context nil)
         (gptel-tools nil)
         (gptel--known-tools nil)
         (gptel--known-presets (copy-tree gptel--known-presets))
         (gptel-directives (copy-tree gptel-directives))
         (gptel-use-curl t)
         (gptel-stream nil)
         (gptel-track-media nil)
         (gptel-use-header-line nil)
         (gptel-default-mode 'text-mode)
         (gptel-display-buffer-action '(display-buffer-same-window))
         (transient-mark-mode transient-mark-mode)
         (message-log-max nil)
         (print-circle nil)
         (root-owned nil)
         (parked nil)
         fixture-before fixture-after result body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute gptel sandbox root"))
              (when (file-exists-p root)
                (error "gptel sandbox root already exists: %s" root))
              (dolist (name '("*gptel-rank402*" "*gptel-query*" "*gptel-context*"))
                (when-let ((entry (gptel402-test-park-buffer name))) (push entry parked)))
              (make-directory root t)
              (setq root-owned t)
              (dolist (entry files)
                (gptel402-test-write-file root (car entry) (cdr entry)))
              (setq fixture-before (gptel402-test-manifest root))
              (setq result
                    (cl-letf (((symbol-function 'call-process)
                               (lambda (&rest args) (apply #'gptel402-test-forbid-external 'call-process args)))
                              ((symbol-function 'call-process-region)
                               (lambda (&rest args) (apply #'gptel402-test-forbid-external 'call-process-region args)))
                              ((symbol-function 'process-file)
                               (lambda (&rest args) (apply #'gptel402-test-forbid-external 'process-file args)))
                              ((symbol-function 'start-process)
                               (lambda (&rest args) (apply #'gptel402-test-forbid-external 'start-process args)))
                              ((symbol-function 'start-file-process)
                               (lambda (&rest args) (apply #'gptel402-test-forbid-external 'start-file-process args)))
                              ((symbol-function 'make-process)
                               (lambda (&rest args) (apply #'gptel402-test-forbid-external 'make-process args)))
                              ((symbol-function 'make-network-process)
                               (lambda (&rest args) (apply #'gptel402-test-forbid-external 'make-network-process args)))
                              ((symbol-function 'open-network-stream)
                               (lambda (&rest args) (apply #'gptel402-test-forbid-external 'open-network-stream args)))
                              ((symbol-function 'url-retrieve)
                               (lambda (&rest args) (apply #'gptel402-test-forbid-external 'url-retrieve args)))
                              ((symbol-function 'url-retrieve-synchronously)
                               (lambda (&rest args) (apply #'gptel402-test-forbid-external 'url-retrieve-synchronously args))))
                      (funcall body root)))
              (setq fixture-after (gptel402-test-manifest root))
              (unless (equal fixture-before fixture-after)
                (error "gptel fixture changed: %S -> %S" fixture-before fixture-after)))
          (error (setq body-error (gptel402-test-condition condition root))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case condition (delete-process process)
            (error (push (gptel402-test-condition condition root) cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case condition
              (progn (with-current-buffer buffer (set-buffer-modified-p nil)) (kill-buffer buffer))
            (error (push (gptel402-test-condition condition root) cleanup-errors)))))
      (dolist (timer (copy-sequence timer-list))
        (unless (memq timer timers-before)
          (condition-case condition (cancel-timer timer)
            (error (push (gptel402-test-condition condition root) cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case condition (delete-frame frame t)
            (error (push (gptel402-test-condition condition root) cleanup-errors)))))
      (condition-case condition (set-window-configuration window-before)
        (error (push (gptel402-test-condition condition root) cleanup-errors)))
      (dolist (entry parked)
        (condition-case condition
            (if (buffer-live-p (car entry))
                (with-current-buffer (car entry) (rename-buffer (cdr entry) t))
              (error "Parked gptel buffer died: %S" entry))
          (error (push (gptel402-test-condition condition root) cleanup-errors))))
      (when (buffer-live-p buffer-before) (set-buffer buffer-before))
      (when root-owned
        (condition-case condition (delete-directory root t)
          (error (push (gptel402-test-condition condition root) cleanup-errors)))))
    (let ((cleanup
           (list :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter (lambda (buffer)
                                       (and (buffer-live-p buffer)
                                            (not (memq buffer buffers-before))))
                                     (buffer-list)))
                 :new-processes (length (seq-remove (lambda (p) (memq p processes-before)) (process-list)))
                 :new-timers (length (seq-remove (lambda (x) (memq x timers-before)) timer-list))
                 :new-frames (length (seq-remove (lambda (x) (memq x frames-before)) (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :fixture-restored (equal fixture-before fixture-after)
                 :window-restored (equal window-state-before (gptel402-test-window-state))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "gptel workflow failed: %S" (list result cleanup))
        (gptel402-test-normalize
         (list :source (copy-tree gptel402-test-source-manifest)
               :result result :cleanup cleanup) root)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GPTEL_MELPA_PIN, "gptel.el")
        .expect("prepare exact shallow gptel source below ./tmp")
        .with_melpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare exact Compat dependency below ./tmp")
        .with_melpa_dependency(TRANSIENT_MELPA_PIN)
        .expect("prepare exact Transient dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_chat_buffer_enables_mode_and_restores_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_chat_buffer_enables_mode_and_restores_lifecycle",
        r####"
(gptel402-test-run nil
 (lambda (_root)
   (let* ((buffer (gptel "*gptel-rank402*" nil "Question 界?" nil))
          enabled disabled)
     (with-current-buffer buffer
       (setq enabled
             (list :text (buffer-substring-no-properties (point-min) (point-max))
                   :mode major-mode :gptel-mode gptel-mode
                   :binding (lookup-key gptel-mode-map (kbd "C-c RET"))
                   :before-save (and (memq #'gptel--save-state before-save-hook) t)
                   :after-change (and (memq #'gptel--inherit-stickiness
                                            after-change-functions) t)
                   :status-kind (car-safe mode-line-process)
                   :status (and (eq (car-safe mode-line-process) :eval)
                                (substring-no-properties (eval (cadr mode-line-process) t)))))
       (gptel-mode -1)
       (setq disabled
             (list :gptel-mode gptel-mode
                   :before-save (and (memq #'gptel--save-state before-save-hook) t)
                   :after-change (and (memq #'gptel--inherit-stickiness
                                            after-change-functions) t)
                   :status mode-line-process)))
     (list :same-buffer (eq buffer (gptel "*gptel-rank402*" nil nil nil))
           :enabled enabled :disabled disabled))))
"####,
        expect![[
            r#"OK (:source (("gptel-anthropic.el" . "092b9afb132b83012c6c1f4918b2f4f76a75dbf1726766995bb6c860b2ced091") ("gptel-bedrock.el" . "bf308de02d37e035dc7ae53d5f601c74f85b59d47609bee09e102d859a8b3e38") ("gptel-context.el" . "9a30c26f9596639422821059f7b8adcd4ce0f6c8b0b5ac83986d878d22c0d2ff") ("gptel-gemini.el" . "39f12fbc5907256b6eeffe533a60e33dfd226fd74a949b4e426a451d17b4452d") ("gptel-gh.el" . "ef6f03b535b56f68066a7ab02deed769e7cca0ce2849d1c2b1525bde59eaa667") ("gptel-integrations.el" . "92040a52db21cd9ad80641e0ac13c8105e50e45e83ba3fe8d6b9bf7252709ac2") ("gptel-kagi.el" . "14d9da5527fb1b8666962e485eb6e500cb5730e22d7eef86bf27daba4f7867b9") ("gptel-oauth.el" . "d1ca33b6e8fd22caabdeabbb782d7dafee1d078f02cb23af667a8c6bb4cc1486") ("gptel-ollama.el" . "d848e914b54bcc6d64eb2ef808be76f5c7b141ba77d3fbedde53a183a5107b33") ("gptel-openai-extras.el" . "98b5dd587bf1e8bcdf3ea718223da4b5a8a39795b15d44151e0b5652b34071ef") ("gptel-openai-oauth.el" . "8195d620cb11f95824b7f9355109eb62bbe85eca7f2e955a810fe5aa06daebdc") ("gptel-openai-responses.el" . "e2a82f23e745b4025217c81b4d725e9d022c17103808a7ce3bbe78ca240fb159") ("gptel-openai.el" . "9f5481300aa8a4df173d747072580b09afc9ba4f9c826299327dba3c981dfa40") ("gptel-org.el" . "01cdfeb433c90813c8067de13242c35b06b85d2b9e3835c7060e530b506d4bbb") ("gptel-request.el" . "9b53dc8204fa683562643e3e45211dd9c7f7a105d677e77bea4ef3ef50a20df3") ("gptel-rewrite.el" . "81add49016517c4c907d7990bfc8e81d95aacb5fcf3a46e6aee92bd1043c91a8") ("gptel-transient.el" . "cff4e58e6f5280542f1e77207061b040dec01958b7c453dd0ccc7640e874fce1") ("gptel.el" . "4c15cdd05a219aa4232f4e78ca9576087fd5c94a7efaec02d6407e58bf7ac773")) :result (:same-buffer t :enabled (:text "Question 界?" :mode text-mode :gptel-mode t :binding gptel-send :before-save t :after-change t :status-kind :eval :status " gpt-5.4-mini") :disabled (:gptel-mode nil :before-save nil :after-change nil :status nil)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_send_builds_exact_request_and_inserts_recorded_response() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_send_builds_exact_request_and_inserts_recorded_response",
        r####"
(gptel402-test-run nil
 (lambda (_root)
   (let* ((buffer (gptel "*gptel-rank402*" nil "Prompt café 界" nil))
          (gptel-model 'gpt-4o-mini)
          (gptel-system-prompt "Answer exactly.")
          (transport-calls 0)
          captured)
     (with-current-buffer buffer
       (goto-char (point-max))
       (cl-letf (((symbol-function 'gptel-curl-get-response)
                  (lambda (fsm)
                    (let ((info (gptel-fsm-info fsm)))
                      (cl-incf transport-calls)
                      (unless (and (= transport-calls 1)
                                   (eq (gptel-fsm-state fsm) 'WAIT)
                                   (not (plist-member info :callback))
                                   (not (plist-get info :stream))
                                   (plist-get info :data))
                        (error "Unexpected gptel transport state: %S"
                               (list transport-calls (gptel-fsm-state fsm) info)))
                      (setq info (plist-put info :callback #'gptel--insert-response))
                      (plist-put info :http-status "200")
                      (plist-put info :status "HTTP/1.1 200 OK")
                      (setf (gptel-fsm-info fsm) info)
                      (setq captured
                            (list :backend (gptel-backend-name (plist-get info :backend))
                                  :model (plist-get info :model)
                                  :stream (plist-get info :stream)
                                  :position
                                  (let ((marker (plist-get info :position)))
                                    (list (marker-position marker)
                                          (buffer-name (marker-buffer marker))))
                                  :data (copy-tree (plist-get info :data))))
                      (gptel--fsm-transition fsm)
                      (funcall (plist-get info :callback) "Recorded reply α界" info)
                      (gptel--fsm-transition fsm)))))
         (call-interactively #'gptel-send))
       (let ((position (point-min)) runs)
         (while (< position (point-max))
           (let ((next (next-single-property-change position 'gptel nil (point-max))))
             (when (get-text-property position 'gptel)
               (push (list position next
                           (get-text-property position 'gptel)
                           (buffer-substring-no-properties position next)) runs))
             (setq position next)))
         (list :transport-calls transport-calls
               :request captured
               :text (buffer-substring-no-properties (point-min) (point-max))
               :response-runs (nreverse runs)
               :fsm-state (gptel-fsm-state gptel--fsm-last)))))))
"####,
        expect![[
            r#"OK (:source (("gptel-anthropic.el" . "092b9afb132b83012c6c1f4918b2f4f76a75dbf1726766995bb6c860b2ced091") ("gptel-bedrock.el" . "bf308de02d37e035dc7ae53d5f601c74f85b59d47609bee09e102d859a8b3e38") ("gptel-context.el" . "9a30c26f9596639422821059f7b8adcd4ce0f6c8b0b5ac83986d878d22c0d2ff") ("gptel-gemini.el" . "39f12fbc5907256b6eeffe533a60e33dfd226fd74a949b4e426a451d17b4452d") ("gptel-gh.el" . "ef6f03b535b56f68066a7ab02deed769e7cca0ce2849d1c2b1525bde59eaa667") ("gptel-integrations.el" . "92040a52db21cd9ad80641e0ac13c8105e50e45e83ba3fe8d6b9bf7252709ac2") ("gptel-kagi.el" . "14d9da5527fb1b8666962e485eb6e500cb5730e22d7eef86bf27daba4f7867b9") ("gptel-oauth.el" . "d1ca33b6e8fd22caabdeabbb782d7dafee1d078f02cb23af667a8c6bb4cc1486") ("gptel-ollama.el" . "d848e914b54bcc6d64eb2ef808be76f5c7b141ba77d3fbedde53a183a5107b33") ("gptel-openai-extras.el" . "98b5dd587bf1e8bcdf3ea718223da4b5a8a39795b15d44151e0b5652b34071ef") ("gptel-openai-oauth.el" . "8195d620cb11f95824b7f9355109eb62bbe85eca7f2e955a810fe5aa06daebdc") ("gptel-openai-responses.el" . "e2a82f23e745b4025217c81b4d725e9d022c17103808a7ce3bbe78ca240fb159") ("gptel-openai.el" . "9f5481300aa8a4df173d747072580b09afc9ba4f9c826299327dba3c981dfa40") ("gptel-org.el" . "01cdfeb433c90813c8067de13242c35b06b85d2b9e3835c7060e530b506d4bbb") ("gptel-request.el" . "9b53dc8204fa683562643e3e45211dd9c7f7a105d677e77bea4ef3ef50a20df3") ("gptel-rewrite.el" . "81add49016517c4c907d7990bfc8e81d95aacb5fcf3a46e6aee92bd1043c91a8") ("gptel-transient.el" . "cff4e58e6f5280542f1e77207061b040dec01958b7c453dd0ccc7640e874fce1") ("gptel.el" . "4c15cdd05a219aa4232f4e78ca9576087fd5c94a7efaec02d6407e58bf7ac773")) :result (:transport-calls 1 :request (:backend "ChatGPT" :model gpt-4o-mini :stream nil :position (14 "*gptel-rank402*") :data (:model "gpt-4o-mini" :input [(:role "user" :content "Prompt café 界")] :store :json-false :stream :json-false :instructions "Answer exactly.")) :text "Prompt café 界\n\nRecorded reply α界\n\n### " :response-runs ((16 33 response "Recorded reply α界")) :fsm-state DONE) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_context_commands_track_live_regions_files_and_removal() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_context_commands_track_live_regions_files_and_removal",
        r####"
(gptel402-test-run
 '(("notes/context.txt" . "File context 界.\n"))
 (lambda (root)
   (require 'gptel-context)
   (let* ((file (expand-file-name "notes/context.txt" root))
          (buffer (generate-new-buffer "gptel402-live-context"))
          added collected removed)
     (with-current-buffer buffer
       (insert "alpha café beta")
       (goto-char 7)
       (push-mark 11 t t)
       (setq transient-mark-mode t mark-active t)
       (call-interactively #'gptel-add)
       (gptel-add-file file)
       (setq added
             (list :context-count (length gptel-context)
                   :overlays
                   (mapcar (lambda (overlay)
                             (list (overlay-start overlay) (overlay-end overlay)
                                   (buffer-substring-no-properties
                                    (overlay-start overlay) (overlay-end overlay))))
                           (plist-get (alist-get buffer gptel-context) :overlays))))
       (with-temp-buffer
         (text-mode)
         (insert "Question with context.")
         (let* ((gptel-model 'gpt-4o-mini)
                (gptel-use-context 'system)
                (fsm (gptel-request nil :dry-run t :stream nil
                                    :transforms gptel-prompt-transform-functions)))
           (setq collected (copy-tree (plist-get (gptel-fsm-info fsm) :data)))))
       (gptel-context-remove-all nil)
       (setq removed
             (list :context gptel-context
                   :owned-overlays
                   (seq-count (lambda (overlay) (overlay-get overlay 'gptel-context))
                              (overlays-in (point-min) (point-max))))))
     (list :added added :collected collected :removed removed))))
"####,
        expect![[
            r#"OK (:source (("gptel-anthropic.el" . "092b9afb132b83012c6c1f4918b2f4f76a75dbf1726766995bb6c860b2ced091") ("gptel-bedrock.el" . "bf308de02d37e035dc7ae53d5f601c74f85b59d47609bee09e102d859a8b3e38") ("gptel-context.el" . "9a30c26f9596639422821059f7b8adcd4ce0f6c8b0b5ac83986d878d22c0d2ff") ("gptel-gemini.el" . "39f12fbc5907256b6eeffe533a60e33dfd226fd74a949b4e426a451d17b4452d") ("gptel-gh.el" . "ef6f03b535b56f68066a7ab02deed769e7cca0ce2849d1c2b1525bde59eaa667") ("gptel-integrations.el" . "92040a52db21cd9ad80641e0ac13c8105e50e45e83ba3fe8d6b9bf7252709ac2") ("gptel-kagi.el" . "14d9da5527fb1b8666962e485eb6e500cb5730e22d7eef86bf27daba4f7867b9") ("gptel-oauth.el" . "d1ca33b6e8fd22caabdeabbb782d7dafee1d078f02cb23af667a8c6bb4cc1486") ("gptel-ollama.el" . "d848e914b54bcc6d64eb2ef808be76f5c7b141ba77d3fbedde53a183a5107b33") ("gptel-openai-extras.el" . "98b5dd587bf1e8bcdf3ea718223da4b5a8a39795b15d44151e0b5652b34071ef") ("gptel-openai-oauth.el" . "8195d620cb11f95824b7f9355109eb62bbe85eca7f2e955a810fe5aa06daebdc") ("gptel-openai-responses.el" . "e2a82f23e745b4025217c81b4d725e9d022c17103808a7ce3bbe78ca240fb159") ("gptel-openai.el" . "9f5481300aa8a4df173d747072580b09afc9ba4f9c826299327dba3c981dfa40") ("gptel-org.el" . "01cdfeb433c90813c8067de13242c35b06b85d2b9e3835c7060e530b506d4bbb") ("gptel-request.el" . "9b53dc8204fa683562643e3e45211dd9c7f7a105d677e77bea4ef3ef50a20df3") ("gptel-rewrite.el" . "81add49016517c4c907d7990bfc8e81d95aacb5fcf3a46e6aee92bd1043c91a8") ("gptel-transient.el" . "cff4e58e6f5280542f1e77207061b040dec01958b7c453dd0ccc7640e874fce1") ("gptel.el" . "4c15cdd05a219aa4232f4e78ca9576087fd5c94a7efaec02d6407e58bf7ac773")) :result (:added (:context-count 2 :overlays ((7 11 "café"))) :collected (:model "gpt-4o-mini" :input [(:role "user" :content "Question with context.")] :store :json-false :stream :json-false :instructions "Request context:\n\nIn buffer `gptel402-live-context`:\n\n```\n ...café\n...\n```\n\nIn file `[ROOT]/notes/context.txt`:\n\n```\nFile context 界.\n\n```\n\nYou are a large language model living in Emacs and a helpful assistant. Respond concisely.") :removed (:context nil :owned-overlays 0)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_tool_and_preset_apis_build_schema_and_scope_options() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_tool_and_preset_apis_build_schema_and_scope_options",
        r####"
(gptel402-test-run nil
 (lambda (_root)
   (let* ((tool
           (gptel-make-tool
            :name "join_words"
            :function (lambda (left right) (concat left "界" right))
            :description "Join two words with Unicode."
            :args '((:name "left" :type string :description "Left word")
                    (:name "right" :type string :description "Right word" :optional t))
            :category "text" :confirm nil))
          (gptel-tools (list tool))
          (gptel-model 'gpt-4o-mini)
          (before (list gptel-system-prompt gptel-temperature gptel-stream))
          request-data inside after)
     (with-temp-buffer
       (text-mode)
       (insert "Join alpha and beta.")
       (setq request-data
             (plist-get (gptel-fsm-info (gptel-request nil :dry-run t :stream nil))
                        :data)))
     (gptel-make-preset 'rank402-child
       :system "Precise 界" :temperature 0.25 :stream nil)
     (setq inside
           (gptel-with-preset 'rank402-child
             (list gptel-system-prompt gptel-temperature gptel-stream
                   (funcall (gptel-tool-function (gptel-get-tool "join_words"))
                            "alpha" "beta"))))
     (setq after (list gptel-system-prompt gptel-temperature gptel-stream))
     (list :tool
           (list :name (gptel-tool-name tool)
                 :category (gptel-tool-category tool)
                 :description (gptel-tool-description tool)
                 :args (copy-tree (gptel-tool-args tool))
                 :lookup-same (eq tool (gptel-get-tool '("text" "join_words"))))
           :preset (copy-tree (gptel-get-preset 'rank402-child))
           :request-data request-data
           :before before :inside inside :after after))))
"####,
        expect![[
            r#"OK (:source (("gptel-anthropic.el" . "092b9afb132b83012c6c1f4918b2f4f76a75dbf1726766995bb6c860b2ced091") ("gptel-bedrock.el" . "bf308de02d37e035dc7ae53d5f601c74f85b59d47609bee09e102d859a8b3e38") ("gptel-context.el" . "9a30c26f9596639422821059f7b8adcd4ce0f6c8b0b5ac83986d878d22c0d2ff") ("gptel-gemini.el" . "39f12fbc5907256b6eeffe533a60e33dfd226fd74a949b4e426a451d17b4452d") ("gptel-gh.el" . "ef6f03b535b56f68066a7ab02deed769e7cca0ce2849d1c2b1525bde59eaa667") ("gptel-integrations.el" . "92040a52db21cd9ad80641e0ac13c8105e50e45e83ba3fe8d6b9bf7252709ac2") ("gptel-kagi.el" . "14d9da5527fb1b8666962e485eb6e500cb5730e22d7eef86bf27daba4f7867b9") ("gptel-oauth.el" . "d1ca33b6e8fd22caabdeabbb782d7dafee1d078f02cb23af667a8c6bb4cc1486") ("gptel-ollama.el" . "d848e914b54bcc6d64eb2ef808be76f5c7b141ba77d3fbedde53a183a5107b33") ("gptel-openai-extras.el" . "98b5dd587bf1e8bcdf3ea718223da4b5a8a39795b15d44151e0b5652b34071ef") ("gptel-openai-oauth.el" . "8195d620cb11f95824b7f9355109eb62bbe85eca7f2e955a810fe5aa06daebdc") ("gptel-openai-responses.el" . "e2a82f23e745b4025217c81b4d725e9d022c17103808a7ce3bbe78ca240fb159") ("gptel-openai.el" . "9f5481300aa8a4df173d747072580b09afc9ba4f9c826299327dba3c981dfa40") ("gptel-org.el" . "01cdfeb433c90813c8067de13242c35b06b85d2b9e3835c7060e530b506d4bbb") ("gptel-request.el" . "9b53dc8204fa683562643e3e45211dd9c7f7a105d677e77bea4ef3ef50a20df3") ("gptel-rewrite.el" . "81add49016517c4c907d7990bfc8e81d95aacb5fcf3a46e6aee92bd1043c91a8") ("gptel-transient.el" . "cff4e58e6f5280542f1e77207061b040dec01958b7c453dd0ccc7640e874fce1") ("gptel.el" . "4c15cdd05a219aa4232f4e78ca9576087fd5c94a7efaec02d6407e58bf7ac773")) :result (:tool (:name "join_words" :category "text" :description "Join two words with Unicode." :args ((:name "left" :type "string" :description "Left word") (:name "right" :type "string" :description "Right word" :optional t)) :lookup-same t) :preset (:system "Precise 界" :temperature 0.25 :stream nil) :request-data (:model "gpt-4o-mini" :input [(:role "user" :content "Join alpha and beta.")] :store :json-false :stream :json-false :instructions "You are a large language model living in Emacs and a helpful assistant. Respond concisely." :tools [(:type "function" :name "join_words" :description "Join two words with Unicode." :parameters (:type "object" :properties (:left (:type "string" :description "Left word") :right (:type "string" :description "Right word")) :required ["left"] :additionalProperties :json-false))]) :before ("You are a large language model living in Emacs and a helpful assistant. Respond concisely." nil nil) :inside ("Precise 界" 0.25 nil "alpha界beta") :after ("You are a large language model living in Emacs and a helpful assistant. Respond concisely." nil nil)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_org_and_response_navigation_preserve_semantic_bounds() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_org_and_response_navigation_preserve_semantic_bounds",
        r####"
(gptel402-test-run nil
 (lambda (_root)
   (require 'org)
   (let ((buffer (generate-new-buffer "gptel402-org"))
         (response-buffer (generate-new-buffer "gptel402-org-response"))
         (post-response-before (copy-sequence gptel-post-response-functions))
         (temp-buffers-before
          (seq-filter (lambda (candidate)
                        (string-prefix-p " *gptel-temp*" (buffer-name candidate)))
                      (buffer-list)))
         topic converted navigation transport transformer-created cleanup-state)
     (with-current-buffer buffer
       (org-mode)
       (insert "* Topic 界\nPrompt\n")
       (goto-char (point-min))
       (gptel-org-set-topic "topic-界")
       (setq topic (list :property (org-entry-get nil "GPTEL_TOPIC")
                         :text (buffer-substring-no-properties (point-min) (point-max))))
       (erase-buffer)
       (text-mode)
       (insert "User one\nAssistant α\nUser two\nAssistant 界")
       (add-text-properties 10 21 '(gptel response))
       (add-text-properties 31 42 '(gptel response))
       (goto-char (point-max))
       (call-interactively #'gptel-beginning-of-response)
       (let ((second-beg (point)))
         (call-interactively #'gptel-beginning-of-response)
         (let ((first-beg (point)))
           (call-interactively #'gptel-end-of-response)
           (setq navigation
                 (list :second-begin second-beg :first-begin first-beg
                       :first-end (point))))))
     (with-current-buffer response-buffer
       (org-mode)
       (insert "Convert this response.")
       (let ((gptel-model 'gpt-4o-mini)
             (gptel-system-prompt "Convert exactly.")
             (gptel-org-convert-response t))
         (cl-letf (((symbol-function 'gptel-curl-get-response)
                    (lambda (fsm)
                      (let ((info (gptel-fsm-info fsm)))
                        (unless (and (eq (gptel-fsm-state fsm) 'WAIT)
                                     (not (plist-member info :callback))
                                     (not (plist-get info :stream))
                                     (with-current-buffer (plist-get info :buffer)
                                       (derived-mode-p 'org-mode)))
                          (error "Unexpected Org transport state: %S"
                                 (list (gptel-fsm-state fsm) info major-mode)))
                        (setq transport
                              (list :model (plist-get info :model)
                                    :data (copy-tree (plist-get info :data))))
                        (plist-put info :transformer
                                   (gptel--stream-convert-markdown->org
                                    (plist-get info :position)))
                        (setq transformer-created
                              (and (functionp (plist-get info :transformer))
                                   (= (length gptel-post-response-functions)
                                      (1+ (length post-response-before)))))
                        (plist-put info :callback #'gptel--insert-response)
                        (setf (gptel-fsm-info fsm) info)
                        (gptel--fsm-transition fsm)
                        (funcall (plist-get info :callback)
                                 "# Heading\n\n**bold** and `code`\n\n```elisp\n(+ 1 2)\n```"
                                 info)
                        (gptel--fsm-transition fsm)))))
           (call-interactively #'gptel-send)))
       (setq converted (buffer-substring-no-properties (point-min) (point-max)))
       (setq cleanup-state
             (list :hook-restored
                   (equal gptel-post-response-functions post-response-before)
                   :temp-buffers-restored
                   (equal (seq-filter
                           (lambda (candidate)
                             (string-prefix-p " *gptel-temp*" (buffer-name candidate)))
                           (buffer-list))
                          temp-buffers-before))))
     (list :topic topic :transport transport
           :transformer-created transformer-created
           :converted converted :navigation navigation
           :transformer-cleanup cleanup-state))))
"####,
        expect![[
            r#"OK (:source (("gptel-anthropic.el" . "092b9afb132b83012c6c1f4918b2f4f76a75dbf1726766995bb6c860b2ced091") ("gptel-bedrock.el" . "bf308de02d37e035dc7ae53d5f601c74f85b59d47609bee09e102d859a8b3e38") ("gptel-context.el" . "9a30c26f9596639422821059f7b8adcd4ce0f6c8b0b5ac83986d878d22c0d2ff") ("gptel-gemini.el" . "39f12fbc5907256b6eeffe533a60e33dfd226fd74a949b4e426a451d17b4452d") ("gptel-gh.el" . "ef6f03b535b56f68066a7ab02deed769e7cca0ce2849d1c2b1525bde59eaa667") ("gptel-integrations.el" . "92040a52db21cd9ad80641e0ac13c8105e50e45e83ba3fe8d6b9bf7252709ac2") ("gptel-kagi.el" . "14d9da5527fb1b8666962e485eb6e500cb5730e22d7eef86bf27daba4f7867b9") ("gptel-oauth.el" . "d1ca33b6e8fd22caabdeabbb782d7dafee1d078f02cb23af667a8c6bb4cc1486") ("gptel-ollama.el" . "d848e914b54bcc6d64eb2ef808be76f5c7b141ba77d3fbedde53a183a5107b33") ("gptel-openai-extras.el" . "98b5dd587bf1e8bcdf3ea718223da4b5a8a39795b15d44151e0b5652b34071ef") ("gptel-openai-oauth.el" . "8195d620cb11f95824b7f9355109eb62bbe85eca7f2e955a810fe5aa06daebdc") ("gptel-openai-responses.el" . "e2a82f23e745b4025217c81b4d725e9d022c17103808a7ce3bbe78ca240fb159") ("gptel-openai.el" . "9f5481300aa8a4df173d747072580b09afc9ba4f9c826299327dba3c981dfa40") ("gptel-org.el" . "01cdfeb433c90813c8067de13242c35b06b85d2b9e3835c7060e530b506d4bbb") ("gptel-request.el" . "9b53dc8204fa683562643e3e45211dd9c7f7a105d677e77bea4ef3ef50a20df3") ("gptel-rewrite.el" . "81add49016517c4c907d7990bfc8e81d95aacb5fcf3a46e6aee92bd1043c91a8") ("gptel-transient.el" . "cff4e58e6f5280542f1e77207061b040dec01958b7c453dd0ccc7640e874fce1") ("gptel.el" . "4c15cdd05a219aa4232f4e78ca9576087fd5c94a7efaec02d6407e58bf7ac773")) :result (:topic (:property "topic-界" :text "* Topic 界\n:PROPERTIES:\n:GPTEL_TOPIC: topic-界\n:END:\nPrompt\n") :transport (:model gpt-4o-mini :data (:model "gpt-4o-mini" :input [(:role "user" :content "Convert this response.")] :store :json-false :stream :json-false :instructions "Convert exactly.")) :transformer-created t :converted "Convert this response.\n\n* Heading\n\n*bold* and =code=\n\n#+begin_src elisp\n(+ 1 2)\n#+end_src" :navigation (:second-begin 31 :first-begin 10 :first-end 21) :transformer-cleanup (:hook-restored t :temp-buffers-restored t)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn gptel_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_chat_buffer_enables_mode_and_restores_lifecycle(),
        public_send_builds_exact_request_and_inserts_recorded_response(),
        public_context_commands_track_live_regions_files_and_removal(),
        public_tool_and_preset_apis_build_schema_and_scope_options(),
        public_org_and_response_navigation_preserve_semantic_bounds(),
    ];
    assert_oracle_batch_cases(oracle(), "gptel-rank402", "gptel", &cases);
}
