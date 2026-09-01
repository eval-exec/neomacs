//! Practical EIN parity against the exact locked notebook client.
//!
//! The workflows use EIN's real notebook, worksheet, cell, Polymode, content
//! API, deferred, and widget code. Only `request`, the unavailable HTTP server
//! boundary, is replayed with exact URLs, settings, response order, and errors.

use std::time::Duration;

use expect_test::expect;

use crate::{
    ANAPHORA_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN, DEFERRED_MELPA_PIN, EIN_MELPA_PIN,
    POLYMODE_MELPA_PIN, REQUEST_MELPA_PIN, WEBSOCKET_MELPA_PIN, WITH_EDITOR_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'json)
(require 'ein-notebook)
(require 'ein-notebooklist)
(require 'ein-ipynb-mode)

(defconst ein376-test-notebook-data
  '(:metadata (:language_info (:name "python")
               :kernelspec (:name "python3"
                            :display_name "Python 3"
                            :language "python"))
    :nbformat 4
    :nbformat_minor 5
    :cells
    ((:cell_type "markdown"
      :id "intro-界"
      :metadata (:tags ["documentation" "unicode"])
      :source "# Café notebook\n\nRésumé with λ and 界.")
     (:cell_type "code"
      :id "code-total"
      :metadata (:collapsed nil :tags ["calculation"])
      :execution_count 7
      :source "values = [2, 5, 8]\nsum(values)"
      :outputs
      ((:output_type "stream" :name "stdout" :text "loading…\n")
       (:output_type "execute_result" :execution_count 7
        :metadata nil :data (:text/plain "15"))))
     (:cell_type "raw"
      :id "raw-notes"
      :metadata nil
      :source "RAW Ω / keep exactly"))))

(defun ein376-test-make-notebook (name data)
  (let* ((path (concat "project space/" name))
         (events (ein:events-new))
         (kernelspec
          (make-ein:$kernelspec
           :name "python3"
           :display-name "Python 3"
           :language "python"
           :spec '(:argv ["python" "-m" "ipykernel_launcher"]
                   :display_name "Python 3"
                   :language "python")))
         (notebook (ein:notebook-new 37676 path kernelspec)))
    (setf (ein:$notebook-notebook-name notebook) name
          (ein:$notebook-notebook-id notebook) path
          (ein:$notebook-api-version notebook) 5)
    (ein:notebook-bind-events notebook events)
    (setf (ein:$notebook-kernel notebook)
          (ein:kernel-new 37676 path kernelspec "/api/kernels" events "5"))
    (ein:notebook-from-json notebook data)
    notebook))

(defun ein376-test-cell-state (cell)
  (list :type (ein:cell-type cell)
        :class (eieio-object-class-name cell)
        :id (let ((id (ein:cell-id cell)))
              (if (member id '("intro-界" "code-total" "raw-notes" "error-界"))
                  id
                (list :generated t
                      :uuid
                      (and (stringp id)
                           (not
                            (null
                             (string-match-p
                              "\\`[[:xdigit:]]\\{8\\}-[[:xdigit:]]\\{4\\}-4[[:xdigit:]]\\{3\\}-[89ab][[:xdigit:]]\\{3\\}-[[:xdigit:]]\\{12\\}\\'"
                              id)))))))
        :text (substring-no-properties (ein:cell-get-text cell))
        :outputs
        (mapcar
         (lambda (output)
           (list :type (plist-get output :output_type)
                 :name (plist-get output :name)
                 :text (plist-get output :text)
                 :count (plist-get output :execution_count)
                 :plain (plist-get (plist-get output :data) :text/plain)
                 :ename (plist-get output :ename)
                 :evalue (plist-get output :evalue)
                 :traceback (and (plist-get output :traceback)
                                 (append (plist-get output :traceback) nil))))
         (slot-value cell 'outputs))
        :prompt (and (ein:codecell-p cell)
                     (ein:oref-safe cell 'input-prompt-number))
        :collapsed (and (ein:codecell-p cell)
                        (slot-value cell 'collapsed))
        :input-start (ein:cell-input-pos-min cell)
        :input-end (ein:cell-input-pos-max cell)))

(defun ein376-test-buffer-state ()
  (let ((cells (ein:worksheet-get-cells ein:%worksheet%)))
    (list :buffer (buffer-name)
          :mode major-mode
          :notebook-mode ein:notebook-mode
          :polymode poly-ein-mode
          :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :modified (buffer-modified-p)
          :dirty (ein:worksheet-modified-p ein:%worksheet%)
          :cells (mapcar #'ein376-test-cell-state cells)
          :keys
          (mapcar (lambda (key)
                    (list key (key-binding (kbd key))))
                  '("C-c C-a" "C-c C-b" "C-c C-k" "C-c C-y"
                    "C-c <up>" "C-c <down>" "C-c C-t" "C-c C-l")))))

(defun ein376-test-edit-state ()
  (let ((current (ein:worksheet-get-current-cell :noerror t)))
    (list :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :current (and current (ein376-test-cell-state current))
          :cells (mapcar #'ein376-test-cell-state
                         (ein:worksheet-get-cells ein:%worksheet%))
          :kill-ring
          (mapcar
           (lambda (entry)
             (mapcar #'ein376-test-cell-state entry))
           ein:kill-ring)
          :modified (buffer-modified-p)
          :dirty (ein:worksheet-modified-p ein:%worksheet%))))

(defun ein376-test-call-key (key)
  (call-interactively (key-binding (kbd key))))

(defun ein376-test-visible-text ()
  (let ((position (point-min)) pieces)
    (while (< position (point-max))
      (unless (get-char-property position 'invisible)
        (push (char-to-string (char-after position)) pieces))
      (setq position (1+ position)))
    (apply #'concat (nreverse pieces))))

(defun ein376-test-output-state ()
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :visible (ein376-test-visible-text)
        :point (point)
        :cells (mapcar #'ein376-test-cell-state
                       (ein:worksheet-get-cells ein:%worksheet%))
        :modified (buffer-modified-p)
        :dirty (ein:worksheet-modified-p ein:%worksheet%)))

(defvar ein376-test-http-ledger nil)
(defvar ein376-test-http-router nil)

(defun ein376-test-utf8-text (text)
  (if (multibyte-string-p text)
      text
    (decode-coding-string text 'utf-8)))

(defun ein376-test-http-data-state (data)
  (cond
   ((null data) nil)
   ((stringp data) (ein376-test-utf8-text data))
   (t (copy-tree data))))

(defun ein376-test-replay-request (_original url &rest settings)
  (let* ((routed (and ein376-test-http-router
                      (funcall ein376-test-http-router url settings)))
         (failure (and (consp routed)
                       (eq (car routed) :ein376-failure)))
         (status (if failure (nth 1 routed) 200))
         (data (if failure (nth 2 routed) routed))
         (error-thrown (and failure (nth 3 routed)))
         (response (make-request-response
                    :status-code status
                    :data data
                    :error-thrown error-thrown
                    :symbol-status (if failure 'error 'success)
                    :url url
                    :done-p t))
         (arguments (list :data data
                          :symbol-status (if failure 'error 'success)
                          :error-thrown error-thrown
                          :response response)))
    (push (append
           (list :url url
                 :type (plist-get settings :type)
                 :headers (copy-tree (plist-get settings :headers))
                 :data (ein376-test-http-data-state (plist-get settings :data))
                 :encoding (plist-get settings :encoding)
                 :sync (plist-get settings :sync)
                 :timeout (plist-get settings :timeout)
                 :success (functionp (plist-get settings :success))
                 :error (functionp (plist-get settings :error)))
           (and (stringp (plist-get settings :data))
                (list :data-bytes (string-bytes (plist-get settings :data))
                      :data-multibyte
                      (multibyte-string-p (plist-get settings :data))
                      :data-sha256
                      (secure-hash 'sha256 (plist-get settings :data))))
           (and failure
                (list :response
                      (list :status status
                            :data (copy-tree data)
                            :error (copy-tree error-thrown)))))
          ein376-test-http-ledger)
    (when-let* ((callback (plist-get settings
                                     (if failure :error :success))))
      (apply callback arguments))
    (when-let* ((complete (plist-get settings :complete)))
      (apply complete arguments))
    response))

(defun ein376-test-with-http-replay (thunk)
  (let (ein376-test-http-ledger)
    (advice-add 'request :around #'ein376-test-replay-request)
    (unwind-protect
        (list :value (funcall thunk)
              :requests (nreverse ein376-test-http-ledger))
      (advice-remove 'request #'ein376-test-replay-request))))

(defun ein376-test-notebooklist-response (url _settings)
  (cond
   ((string-suffix-p "/api/spec.yaml" url)
    "5")
   ((string-suffix-p "/api/kernelspecs" url)
    '(:default "python3"
      :kernelspecs
      (:python3
       (:name "python3"
        :spec (:argv ["python" "-m" "ipykernel_launcher"]
               :display_name "Python 3"
               :language "python")
        :resources nil))))
   ((string-suffix-p "/api/sessions" url)
    [(:id "session-café"
      :path "Café analysis 界.ipynb"
      :name "Café analysis 界.ipynb"
      :type "notebook"
      :kernel (:id "kernel-界" :name "python3")
      :notebook (:path "Café analysis 界.ipynb"
                 :name "Café analysis 界.ipynb"))])
   ((string-match-p "/api/contents/?\\'" url)
    '(:name ""
      :path ""
      :type "directory"
      :format "json"
      :writable t
      :created "2026-08-10T09:00:00.000000Z"
      :last_modified "2026-08-12T15:45:00.000000Z"
      :content
      [(:name "Café analysis 界.ipynb"
        :path "Café analysis 界.ipynb"
        :type "notebook"
        :format "json"
        :writable t
        :created "2026-08-10T09:00:00.000000Z"
        :last_modified "2026-08-12T15:45:00.000000Z")
       (:name "notes Ω.txt"
        :path "notes Ω.txt"
        :type "file"
        :format "text"
        :mimetype "text/plain"
        :writable t
        :created "2026-08-09T08:00:00.000000Z"
        :last_modified "2026-08-11T12:30:00.000000Z")]))
   (t (error "ein376 replay rejected unexpected URL: %s" url))))

(defun ein376-test-notebooklist-missing-response (url settings)
  (if (string-match-p "/api/contents/missing%20folder/?\\'" url)
      '(:ein376-failure
        404
        (:message "No such notebook directory: missing folder")
        (error "HTTP 404: missing folder"))
    (ein376-test-notebooklist-response url settings)))

(defun ein376-test-widget-state ()
  (let ((position (point-min)) seen widgets)
    (while (< position (point-max))
      (when-let* ((widget (widget-at position)))
        (unless (memq widget seen)
          (push widget seen)
          (let* ((from0 (widget-get widget :from))
                 (to0 (widget-get widget :to))
                 (from (if (markerp from0) (marker-position from0) from0))
                 (to (if (markerp to0) (marker-position to0) to0)))
            (unless (and (integerp from) (integerp to) (<= from to))
              (error "widget has invalid bounds: %S" (list from0 to0)))
            (push (list :span (list from to)
                        :text (buffer-substring-no-properties from to)
                        :type (widget-type widget)
                        :value (let ((value (widget-value widget)))
                                 (if (or (null value)
                                         (stringp value)
                                         (symbolp value)
                                         (numberp value))
                                     value
                                   :structured))
                        :tag (widget-get widget :tag)
                        :notify (functionp (widget-get widget :notify)))
                  widgets))))
      (setq position (1+ position)))
    (nreverse widgets)))

(defun ein376-test-notebooklist-state ()
  (list :buffer (buffer-name)
        :mode major-mode
        :readonly buffer-read-only
        :url (ein:$notebooklist-url-or-port ein:%notebooklist%)
        :path (ein:$notebooklist-path ein:%notebooklist%)
        :api-version (ein:$notebooklist-api-version ein:%notebooklist%)
        :kernel (and ein:%notebooklist-new-kernel%
                     (list :name
                           (ein:$kernelspec-name ein:%notebooklist-new-kernel%)
                           :display
                           (ein:$kernelspec-display-name
                            ein:%notebooklist-new-kernel%)
                           :language
                           (ein:$kernelspec-language
                            ein:%notebooklist-new-kernel%)))
        :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :line (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))
        :widgets (ein376-test-widget-state)
        :keys (mapcar (lambda (key) (list key (key-binding (kbd key))))
                      '("n" "p" "TAB" "RET" "C-c C-r"))))

(defun ein376-test-notebooklist-locus ()
  (let ((widget (widget-at)))
    (list :point (point)
          :line (buffer-substring-no-properties
                 (line-beginning-position) (line-end-position))
          :widget (and widget
                       (list :type (widget-type widget)
                             :text (buffer-substring-no-properties
                                    (widget-get widget :from)
                                    (widget-get widget :to))
                             :value (widget-value widget))))))

(defun ein376-test-run (thunk)
  (let ((buffers-before (buffer-list))
        (processes-before (process-list))
        (timers-before (append timer-list nil))
        (idle-timers-before (append timer-idle-list nil))
        (kill-ring-before ein:kill-ring)
        (kill-pointer-before ein:kill-ring-yank-pointer)
        result cleanup-errors)
    (unwind-protect
        (setq result (save-window-excursion (funcall thunk)))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case error
              (when (process-live-p process)
                (delete-process process))
            (error (push (list :process (process-name process) error)
                         cleanup-errors)))))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (or (memq timer timers-before)
                    (memq timer idle-timers-before))
          (condition-case error
              (cancel-timer timer)
            (error (push (list :timer error) cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case error
              (when (buffer-live-p buffer)
                (with-current-buffer buffer
                  (setq kill-buffer-query-functions nil)
                  (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error (push (list :buffer (buffer-name buffer) error)
                         cleanup-errors)))))
      (condition-case error
          (setq ein:kill-ring kill-ring-before
                ein:kill-ring-yank-pointer kill-pointer-before)
        (error (push (list :kill-ring error) cleanup-errors)))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (push (list :remaining-process (process-name process)) cleanup-errors)))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (or (memq timer timers-before)
                    (memq timer idle-timers-before))
          (push (list :remaining-timer timer) cleanup-errors)))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (push (list :remaining-buffer (buffer-name buffer)) cleanup-errors))))
    (if cleanup-errors
        (error "EIN cleanup failed: %S" (nreverse cleanup-errors))
      result)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EIN_MELPA_PIN, "ein.el")
        .expect("prepare pinned EIN source below ./tmp")
        .with_melpa_dependency(ANAPHORA_MELPA_PIN)
        .expect("prepare pinned Anaphora dependency below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned Dash dependency below ./tmp")
        .with_melpa_dependency(DEFERRED_MELPA_PIN)
        .expect("prepare pinned Deferred dependency below ./tmp")
        .with_melpa_dependency(POLYMODE_MELPA_PIN)
        .expect("prepare pinned Polymode dependency below ./tmp")
        .with_melpa_dependency(REQUEST_MELPA_PIN)
        .expect("prepare pinned Request dependency below ./tmp")
        .with_melpa_dependency(WEBSOCKET_MELPA_PIN)
        .expect("prepare pinned Websocket dependency below ./tmp")
        .with_melpa_dependency(WITH_EDITOR_MELPA_PIN)
        .expect("prepare pinned With-Editor dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn parses_and_renders_a_real_nbformat4_notebook() -> ParityBatchCase {
    ParityBatchCase::value(
        "parses_and_renders_a_real_nbformat4_notebook",
        r####"(ein376-test-run
 (lambda ()
   (let* ((notebook
           (ein376-test-make-notebook "Café analysis 界.ipynb"
                                      (copy-tree ein376-test-notebook-data)))
          (buffer (ein:notebook-buffer notebook)))
     (with-current-buffer buffer
       (goto-char (point-min))
       (ein:worksheet-focus-cell)
       (ein376-test-buffer-state)))))"####,
        expect![[
            r##"OK (:buffer "*ein: 37676/project space/Café analysis 界.ipynb*" :mode fundamental-mode :notebook-mode t :polymode t :text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [7]:\nvalues = [2, 5, 8]\nsum(values)\nloading…\n\nOut [7]:\n15\n\nraw:\nRAW Ω / keep exactly\n\n" :point 12 :modified nil :dirty nil :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "code" :class ein:codecell :id "code-total" :text "values = [2, 5, 8]\nsum(values)" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt 7 :collapsed nil :input-start 59 :input-end 89) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 118 :input-end 138)) :keys (("C-c C-a" ein:worksheet-insert-cell-above-km) ("C-c C-b" ein:worksheet-insert-cell-below-km) ("C-c C-k" ein:worksheet-kill-cell-km) ("C-c C-y" ein:worksheet-yank-cell-km) ("C-c <up>" ein:worksheet-move-cell-up-km) ("C-c <down>" ein:worksheet-move-cell-down-km) ("C-c C-t" ein:worksheet-toggle-cell-type-km) ("C-c C-l" ein:worksheet-clear-output-km)))"##
        ]],
    )
}

fn public_cell_editing_preserves_content_order_and_cell_kill_ring() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_cell_editing_preserves_content_order_and_cell_kill_ring",
        r####"(ein376-test-run
 (lambda ()
   (let* ((notebook
           (ein376-test-make-notebook "Cell editing 界.ipynb"
                                      (copy-tree ein376-test-notebook-data)))
          (buffer (ein:notebook-buffer notebook)))
     (with-current-buffer buffer
       (setq ein:kill-ring nil
             ein:kill-ring-yank-pointer nil)
       (ein:cell-goto (nth 1 (ein:worksheet-get-cells ein:%worksheet%)))
       (let ((before (ein376-test-edit-state)))
         (ein376-test-call-key "C-c <down>")
         (let ((moved-down (ein376-test-edit-state)))
           (ein376-test-call-key "C-c <up>")
           (let ((moved-back (ein376-test-edit-state)))
             (goto-char (+ (ein:cell-input-pos-min
                            (ein:worksheet-get-current-cell))
                           (length "values = [2, 5, 8]")))
             (ein376-test-call-key "C-c C-s")
             (let ((split (ein376-test-edit-state)))
               (ein376-test-call-key "C-c C-m")
               (let ((merged (ein376-test-edit-state)) toggled)
                 (dotimes (_ 3)
                   (ein376-test-call-key "C-c C-t")
                   (push (ein376-test-edit-state) toggled))
                 (ein376-test-call-key "C-c C-w")
                 (let ((copied (ein376-test-edit-state)))
                   (ein376-test-call-key "C-c C-k")
                   (let ((killed (ein376-test-edit-state)))
                     (ein376-test-call-key "C-c C-y")
                     (list :before before
                           :moved-down moved-down
                           :moved-back moved-back
                           :split split
                           :merged merged
                           :toggled (nreverse toggled)
                           :copied copied
                           :killed killed
                           :yanked (ein376-test-edit-state)))))))))))))"####,
        expect![[
            r##"OK (:before (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [7]:\nvalues = [2, 5, 8]\nsum(values)\nloading…\n\nOut [7]:\n15\n\nraw:\nRAW Ω / keep exactly\n\n" :point 59 :current (:type "code" :class ein:codecell :id "code-total" :text "values = [2, 5, 8]\nsum(values)" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt 7 :collapsed nil :input-start 59 :input-end 89) :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "code" :class ein:codecell :id "code-total" :text "values = [2, 5, 8]\nsum(values)" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt 7 :collapsed nil :input-start 59 :input-end 89) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 118 :input-end 138)) :kill-ring nil :modified nil :dirty nil) :moved-down (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nraw:\nRAW Ω / keep exactly\n\nIn [ ]:\nvalues = [2, 5, 8]\nsum(values)\nloading…\n\nOut [7]:\n15\n\n" :point 86 :current (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt nil :collapsed nil :input-start 86 :input-end 116) :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 56 :input-end 76) (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt nil :collapsed nil :input-start 86 :input-end 116)) :kill-ring nil :modified t :dirty t) :moved-back (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [ ]:\nvalues = [2, 5, 8]\nsum(values)\nloading…\n\nOut [7]:\n15\n\nraw:\nRAW Ω / keep exactly\n\n" :point 59 :current (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt nil :collapsed nil :input-start 59 :input-end 89) :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt nil :collapsed nil :input-start 59 :input-end 89) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 118 :input-end 138)) :kill-ring nil :modified t :dirty t) :split (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [ ]:\nvalues = [2, 5, 8]\n\nIn [ ]:\nsum(values)\nloading…\n\nOut [7]:\n15\n\nraw:\nRAW Ω / keep exactly\n\n" :point 87 :current (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "sum(values)" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt nil :collapsed nil :input-start 87 :input-end 98) :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]" :outputs nil :prompt nil :collapsed nil :input-start 59 :input-end 77) (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "sum(values)" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt nil :collapsed nil :input-start 87 :input-end 98) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 127 :input-end 147)) :kill-ring nil :modified t :dirty t) :merged (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [ ]:\nvalues = [2, 5, 8]\nsum(values)\nloading…\n\nOut [7]:\n15\n\nraw:\nRAW Ω / keep exactly\n\n" :point 59 :current (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt nil :collapsed nil :input-start 59 :input-end 89) :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt nil :collapsed nil :input-start 59 :input-end 89) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 118 :input-end 138)) :kill-ring nil :modified t :dirty t) :toggled ((:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nmarkdown:\nvalues = [2, 5, 8]\nsum(values)\n\nraw:\nRAW Ω / keep exactly\n\n" :point 61 :current (:type "markdown" :class ein:markdowncell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start 61 :input-end 91) :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "markdown" :class ein:markdowncell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start 61 :input-end 91) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 98 :input-end 118)) :kill-ring nil :modified t :dirty t) (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nraw:\nvalues = [2, 5, 8]\nsum(values)\n\nraw:\nRAW Ω / keep exactly\n\n" :point 56 :current (:type "raw" :class ein:rawcell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start 56 :input-end 86) :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "raw" :class ein:rawcell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start 56 :input-end 86) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 93 :input-end 113)) :kill-ring nil :modified t :dirty t) (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [ ]:\nvalues = [2, 5, 8]\nsum(values)\n\nraw:\nRAW Ω / keep exactly\n\n" :point 59 :current (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start 59 :input-end 89) :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start 59 :input-end 89) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 96 :input-end 116)) :kill-ring nil :modified t :dirty t)) :copied (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [ ]:\nvalues = [2, 5, 8]\nsum(values)\n\nraw:\nRAW Ω / keep exactly\n\n" :point 59 :current (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start 59 :input-end 89) :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start 59 :input-end 89) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 96 :input-end 116)) :kill-ring (((:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start nil :input-end nil))) :modified t :dirty t) :killed (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nraw:\nRAW Ω / keep exactly\n\n" :point 56 :current (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 56 :input-end 76) :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 56 :input-end 76)) :kill-ring (((:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start nil :input-end nil)) ((:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start nil :input-end nil))) :modified t :dirty t) :yanked (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nraw:\nRAW Ω / keep exactly\n\nIn [ ]:\nvalues = [2, 5, 8]\nsum(values)\n\n" :point 86 :current (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start 86 :input-end 116) :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 56 :input-end 76) (:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start 86 :input-end 116)) :kill-ring (((:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start nil :input-end nil)) ((:type "code" :class ein:codecell :id (:generated t :uuid t) :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start nil :input-end nil))) :modified t :dirty t))"##
        ]],
    )
}

fn output_visibility_and_clearing_preserve_prompts_and_exact_error_data() -> ParityBatchCase {
    ParityBatchCase::value(
        "output_visibility_and_clearing_preserve_prompts_and_exact_error_data",
        r####"(ein376-test-run
 (lambda ()
   (let* ((data (copy-tree ein376-test-notebook-data))
          (cells
           (append
            (plist-get data :cells)
            '((:cell_type "code"
               :id "error-界"
               :metadata (:collapsed nil :tags ["failure"])
               :execution_count 8
               :source "raise ValueError('bad 界')"
               :outputs
               ((:output_type "error"
                 :ename "ValueError"
                 :evalue "bad 界"
                 :traceback ["Traceback (most recent call last):"
                             "ValueError: bad 界"]))))))
          (_ (plist-put data :cells cells))
          (notebook (ein376-test-make-notebook "Output lifecycle.ipynb" data))
          (buffer (ein:notebook-buffer notebook)))
     (with-current-buffer buffer
       (ein:cell-goto (nth 1 (ein:worksheet-get-cells ein:%worksheet%)))
       (let ((before (ein376-test-output-state)))
         (ein376-test-call-key "C-c C-e")
         (let ((hidden (ein376-test-output-state)))
           (ein376-test-call-key "C-c C-e")
           (let ((shown (ein376-test-output-state)))
             (let ((current-prefix-arg '(4)))
               (ein376-test-call-key "C-c C-l"))
             (let ((preserved (ein376-test-output-state)))
               (ein376-test-call-key "C-c C-S-l")
               (list :before before
                     :hidden hidden
                     :shown shown
                     :current-cleared-preserving-prompt preserved
                     :all-cleared (ein376-test-output-state))))))))))"####,
        expect![[
            r##"OK (:before (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [7]:\nvalues = [2, 5, 8]\nsum(values)\nloading…\n\nOut [7]:\n15\n\nraw:\nRAW Ω / keep exactly\n\nIn [8]:\nraise ValueError('bad 界')\nTraceback (most recent call last):\nValueError: bad 界\n\n\n" :visible "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [7]:\nvalues = [2, 5, 8]\nsum(values)\nloading…\n\nOut [7]:\n15\n\nraw:\nRAW Ω / keep exactly\n\nIn [8]:\nraise ValueError('bad 界')\nTraceback (most recent call last):\nValueError: bad 界\n\n\n" :point 59 :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "code" :class ein:codecell :id "code-total" :text "values = [2, 5, 8]\nsum(values)" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt 7 :collapsed nil :input-start 59 :input-end 89) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 118 :input-end 138) (:type "code" :class ein:codecell :id "error-界" :text "raise ValueError('bad 界')" :outputs ((:type "error" :name nil :text nil :count nil :plain nil :ename "ValueError" :evalue "bad 界" :traceback ("Traceback (most recent call last):" "ValueError: bad 界"))) :prompt 8 :collapsed nil :input-start 148 :input-end 173)) :modified nil :dirty nil) :hidden (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [7]:\nvalues = [2, 5, 8]\nsum(values)\n..\n\nraw:\nRAW Ω / keep exactly\n\nIn [8]:\nraise ValueError('bad 界')\nTraceback (most recent call last):\nValueError: bad 界\n\n\n" :visible "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [7]:\nvalues = [2, 5, 8]\nsum(values)\n..\n\nraw:\nRAW Ω / keep exactly\n\nIn [8]:\nraise ValueError('bad 界')\nTraceback (most recent call last):\nValueError: bad 界\n\n\n" :point 59 :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "code" :class ein:codecell :id "code-total" :text "values = [2, 5, 8]\nsum(values)" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt 7 :collapsed t :input-start 59 :input-end 89) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 99 :input-end 119) (:type "code" :class ein:codecell :id "error-界" :text "raise ValueError('bad 界')" :outputs ((:type "error" :name nil :text nil :count nil :plain nil :ename "ValueError" :evalue "bad 界" :traceback ("Traceback (most recent call last):" "ValueError: bad 界"))) :prompt 8 :collapsed nil :input-start 129 :input-end 154)) :modified t :dirty t) :shown (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [7]:\nvalues = [2, 5, 8]\nsum(values)\nloading…\n\nOut [7]:\n15\n\nraw:\nRAW Ω / keep exactly\n\nIn [8]:\nraise ValueError('bad 界')\nTraceback (most recent call last):\nValueError: bad 界\n\n\n" :visible "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [7]:\nvalues = [2, 5, 8]\nsum(values)\nloading…\n\nOut [7]:\n15\n\nraw:\nRAW Ω / keep exactly\n\nIn [8]:\nraise ValueError('bad 界')\nTraceback (most recent call last):\nValueError: bad 界\n\n\n" :point 59 :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "code" :class ein:codecell :id "code-total" :text "values = [2, 5, 8]\nsum(values)" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt 7 :collapsed nil :input-start 59 :input-end 89) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 118 :input-end 138) (:type "code" :class ein:codecell :id "error-界" :text "raise ValueError('bad 界')" :outputs ((:type "error" :name nil :text nil :count nil :plain nil :ename "ValueError" :evalue "bad 界" :traceback ("Traceback (most recent call last):" "ValueError: bad 界"))) :prompt 8 :collapsed nil :input-start 148 :input-end 173)) :modified t :dirty t) :current-cleared-preserving-prompt (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [7]:\nvalues = [2, 5, 8]\nsum(values)\n\nraw:\nRAW Ω / keep exactly\n\nIn [8]:\nraise ValueError('bad 界')\nTraceback (most recent call last):\nValueError: bad 界\n\n\n" :visible "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [7]:\nvalues = [2, 5, 8]\nsum(values)\n\nraw:\nRAW Ω / keep exactly\n\nIn [8]:\nraise ValueError('bad 界')\nTraceback (most recent call last):\nValueError: bad 界\n\n\n" :point 59 :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "code" :class ein:codecell :id "code-total" :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt 7 :collapsed nil :input-start 59 :input-end 89) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 96 :input-end 116) (:type "code" :class ein:codecell :id "error-界" :text "raise ValueError('bad 界')" :outputs ((:type "error" :name nil :text nil :count nil :plain nil :ename "ValueError" :evalue "bad 界" :traceback ("Traceback (most recent call last):" "ValueError: bad 界"))) :prompt 8 :collapsed nil :input-start 126 :input-end 151)) :modified t :dirty t) :all-cleared (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [ ]:\nvalues = [2, 5, 8]\nsum(values)\n\nraw:\nRAW Ω / keep exactly\n\nIn [ ]:\nraise ValueError('bad 界')\n\n" :visible "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nIn [ ]:\nvalues = [2, 5, 8]\nsum(values)\n\nraw:\nRAW Ω / keep exactly\n\nIn [ ]:\nraise ValueError('bad 界')\n\n" :point 59 :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 49) (:type "code" :class ein:codecell :id "code-total" :text "values = [2, 5, 8]\nsum(values)" :outputs nil :prompt nil :collapsed nil :input-start 59 :input-end 89) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 96 :input-end 116) (:type "code" :class ein:codecell :id "error-界" :text "raise ValueError('bad 界')" :outputs nil :prompt nil :collapsed nil :input-start 126 :input-end 151)) :modified t :dirty t))"##
        ]],
    )
}

fn public_save_serializes_exact_nbformat4_content_through_http_boundary() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_save_serializes_exact_nbformat4_content_through_http_boundary",
        r####"(ein376-test-run
 (lambda ()
   (let* ((notebook
           (ein376-test-make-notebook "Saved café 界.ipynb"
                                      (copy-tree ein376-test-notebook-data)))
          (buffer (ein:notebook-buffer notebook)))
     (with-current-buffer buffer
       (let ((save-hook-count 0))
         (add-hook 'before-save-hook
                   (lambda () (setq save-hook-count (1+ save-hook-count)))
                   nil t)
         (let ((markdown (nth 0 (ein:worksheet-get-cells ein:%worksheet%)))
               (code (nth 1 (ein:worksheet-get-cells ein:%worksheet%))))
           (ein:cell-goto markdown 0 :after-input)
           (insert "\n\nEdited locally: naïve → exact.")
           (ein:cell-goto code 0 :after-input)
           (insert " + 1")
           (let ((before (ein376-test-edit-state)))
             (ein376-test-with-http-replay
              (lambda ()
                (ein376-test-call-key "C-x C-s")
                (list :before before
                      :after (ein376-test-edit-state)
                      :save-hook-count save-hook-count
                      :notebook-json
                      (ein376-test-utf8-text
                       (ein:json-encode
                        (ein:notebook-to-json notebook)))))))))))))"####,
        expect![[
            r##"OK (:value (:before (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nEdited locally: naïve → exact.\n\nIn [7]:\nvalues = [2, 5, 8]\nsum(values) + 1\nloading…\n\nOut [7]:\n15\n\nraw:\nRAW Ω / keep exactly\n\n" :point 125 :current (:type "code" :class ein:codecell :id "code-total" :text "values = [2, 5, 8]\nsum(values) + 1" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt 7 :collapsed nil :input-start 91 :input-end 125) :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界.\n\nEdited locally: naïve → exact." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 81) (:type "code" :class ein:codecell :id "code-total" :text "values = [2, 5, 8]\nsum(values) + 1" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt 7 :collapsed nil :input-start 91 :input-end 125) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 154 :input-end 174)) :kill-ring nil :modified t :dirty t) :after (:text "\nmarkdown:\n# Café notebook\n\nRésumé with λ and 界.\n\nEdited locally: naïve → exact.\n\nIn [7]:\nvalues = [2, 5, 8]\nsum(values) + 1\nloading…\n\nOut [7]:\n15\n\nraw:\nRAW Ω / keep exactly\n\n" :point 125 :current (:type "code" :class ein:codecell :id "code-total" :text "values = [2, 5, 8]\nsum(values) + 1" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt 7 :collapsed nil :input-start 91 :input-end 125) :cells ((:type "markdown" :class ein:markdowncell :id "intro-界" :text "# Café notebook\n\nRésumé with λ and 界.\n\nEdited locally: naïve → exact." :outputs nil :prompt nil :collapsed nil :input-start 12 :input-end 81) (:type "code" :class ein:codecell :id "code-total" :text "values = [2, 5, 8]\nsum(values) + 1" :outputs ((:type "stream" :name "stdout" :text "loading…\n" :count nil :plain nil :ename nil :evalue nil :traceback nil) (:type "execute_result" :name nil :text nil :count 7 :plain "15" :ename nil :evalue nil :traceback nil)) :prompt 7 :collapsed nil :input-start 91 :input-end 125) (:type "raw" :class ein:rawcell :id "raw-notes" :text "RAW Ω / keep exactly" :outputs nil :prompt nil :collapsed nil :input-start 154 :input-end 174)) :kill-ring nil :modified nil :dirty nil) :save-hook-count 1 :notebook-json "{\"nbformat\":4,\"nbformat_minor\":5,\"metadata\":{\"language_info\":{\"name\":\"python\"},\"kernelspec\":{\"argv\":[\"python\",\"-m\",\"ipykernel_launcher\"],\"display_name\":\"Python 3\",\"language\":\"python\",\"name\":\"python3\"},\"name\":\"Saved café 界.ipynb\"},\"cells\":[{\"cell_type\":\"markdown\",\"source\":\"# Café notebook\\n\\nRésumé with λ and 界.\\n\\nEdited locally: naïve → exact.\",\"metadata\":{\"tags\":[\"documentation\",\"unicode\"],\"collapsed\":false},\"id\":\"intro-界\"},{\"source\":\"values = [2, 5, 8]\\nsum(values) + 1\",\"cell_type\":\"code\",\"execution_count\":7,\"outputs\":[{\"output_type\":\"stream\",\"name\":\"stdout\",\"text\":\"loading…\\n\"},{\"output_type\":\"execute_result\",\"execution_count\":7,\"metadata\":{},\"data\":{\"text/plain\":\"15\"}}],\"metadata\":{\"collapsed\":false,\"tags\":[\"calculation\"]},\"id\":\"code-total\"},{\"cell_type\":\"raw\",\"source\":\"RAW Ω / keep exactly\",\"metadata\":{\"collapsed\":false},\"id\":\"raw-notes\"}]}") :requests ((:url "http://127.0.0.1:37676/api/contents/project%20space/Saved%20caf%C3%A9%20%E7%95%8C.ipynb" :type "PUT" :headers (("Content-Type" . "application/json") ("User-Agent" . "Mozilla/5.0")) :data "{\"type\":\"notebook\",\"name\":\"Saved café 界.ipynb\",\"path\":\"project space/Saved café 界.ipynb\",\"format\":\"json\",\"content\":{\"nbformat\":4,\"nbformat_minor\":5,\"metadata\":{\"language_info\":{\"name\":\"python\"},\"kernelspec\":{\"argv\":[\"python\",\"-m\",\"ipykernel_launcher\"],\"display_name\":\"Python 3\",\"language\":\"python\",\"name\":\"python3\"},\"name\":\"Saved café 界.ipynb\"},\"cells\":[{\"cell_type\":\"markdown\",\"source\":\"# Café notebook\\n\\nRésumé with λ and 界.\\n\\nEdited locally: naïve → exact.\",\"metadata\":{\"tags\":[\"documentation\",\"unicode\"],\"collapsed\":false},\"id\":\"intro-界\"},{\"source\":\"values = [2, 5, 8]\\nsum(values) + 1\",\"cell_type\":\"code\",\"execution_count\":7,\"outputs\":[{\"output_type\":\"stream\",\"name\":\"stdout\",\"text\":\"loading…\\n\"},{\"output_type\":\"execute_result\",\"execution_count\":7,\"metadata\":{},\"data\":{\"text/plain\":\"15\"}}],\"metadata\":{\"collapsed\":false,\"tags\":[\"calculation\"]},\"id\":\"code-total\"},{\"cell_type\":\"raw\",\"source\":\"RAW Ω / keep exactly\",\"metadata\":{\"collapsed\":false},\"id\":\"raw-notes\"}]}}" :encoding binary :sync nil :timeout 10.0 :success t :error t :data-bytes 997 :data-multibyte nil :data-sha256 "c1dc5f35fb0bfcbe7d8c85dc79f86d1330b36422d9585c0ede0678c4b50bcd25")))"##
        ]],
    )
}

fn public_notebook_list_queries_and_renders_real_widget_ui() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_notebook_list_queries_and_renders_real_widget_ui",
        r####"(ein376-test-run
 (lambda ()
   (let ((ein:notebooklist-map (make-hash-table :test 'equal))
         (*ein:notebook-api-version* (make-hash-table :test 'equal))
         (*ein:kernelspecs* (make-hash-table :test 'equal))
         (*ein:content-hierarchy* (make-hash-table :test 'equal))
         (ein:query-xsrf-cache (make-hash-table :test 'equal))
         (ein:query-authorization-tokens (make-hash-table :test 'equal))
         (ein:notebooklist-sort-field :name)
         (ein:notebooklist-sort-order :ascending)
         (ein:notebooklist-date-format "%F")
         (ein:force-sync t)
         (deferred:queue nil)
         (ein376-test-http-router #'ein376-test-notebooklist-response)
         callback-state
         error-state)
     (ein376-test-with-http-replay
      (lambda ()
        (condition-case error
            (ein:notebooklist-open*
             "http://127.0.0.1:37676" "" t
             (lambda (buffer url-or-port)
               (setq callback-state
                     (list :buffer (buffer-name buffer) :url url-or-port)))
             (lambda (&rest failure)
               (setq error-state (copy-tree failure))))
          (error (error "notebook-list open failed: %S" error)))
        (deferred:flush-queue!)
        (let ((buffer (ein:notebooklist-get-buffer
                       "http://127.0.0.1:37676")))
          (unless (and callback-state (buffer-live-p buffer))
            (error "notebook-list callback did not produce a live buffer"))
          (with-current-buffer buffer
            (goto-char (point-min))
            (let ((initial (ein376-test-notebooklist-state))
                  navigation)
              (ein376-test-call-key "n")
              (push (list :next (ein376-test-notebooklist-locus)) navigation)
              (ein376-test-call-key "p")
              (push (list :previous (ein376-test-notebooklist-locus)) navigation)
              (goto-char (point-min))
              (ein376-test-call-key "TAB")
              (push (list :tab (ein376-test-notebooklist-locus)) navigation)
              (ein376-test-call-key "RET")
              (push (list :activated-home (ein376-test-notebooklist-locus))
                    navigation)
              (list :callback callback-state
                    :error error-state
                    :initial initial
                    :navigation (nreverse navigation)
                    :after-home (ein376-test-notebooklist-state)
                    :registered (ein:notebooklist-keys)
                    :api-version
                    (gethash "http://127.0.0.1:37676"
                             *ein:notebook-api-version*)
                    :hierarchy
                    (mapcar (lambda (content)
                              (let ((session
                                     (ein:$content-session-p content)))
                                (list :name (ein:$content-name content)
                                      :path (ein:$content-path content)
                                      :type (ein:$content-type content)
                                      :session
                                      (and session
                                           (list :id (car session)
                                                 :kernel-id
                                                 (plist-get (cdr session) :id)
                                                 :kernel-name
                                                 (plist-get (cdr session)
                                                            :name))))))
                            (ein:content-need-hierarchy
                             "http://127.0.0.1:37676")))))))))))"####,
        expect![[
            r#"OK (:value (:callback (:buffer "*ein:notebooklist http://127.0.0.1:37676*" :url "http://127.0.0.1:37676") :error nil :initial (:buffer "*ein:notebooklist http://127.0.0.1:37676*" :mode ein:notebooklist-mode :readonly t :url "http://127.0.0.1:37676" :path "" :api-version 5 :kernel (:name "python3" :display "Python 3" :language "python") :text "Contents API 5 (http://127.0.0.1:37676)\n\n | [Home] |\n\n[New Notebook] [Resync] [Open In Browser]\n\nCreate New Notebooks Using Kernel:\n(*) Python 3\n\n[Open] [Stop] [Delete] : Café analysis 界.ipynb                            2026-08-12\n[Open]                 : notes Ω.txt                                       2026-08-11\n" :point 1 :line "Contents API 5 (http://127.0.0.1:37676)" :widgets ((:span (45 51) :text "[Home]" :type link :value "Home" :tag nil :notify t) (:span (55 69) :text "[New Notebook]" :type link :value "New Notebook" :tag nil :notify t) (:span (70 78) :text "[Resync]" :type link :value "Resync" :tag nil :notify t) (:span (79 96) :text "[Open In Browser]" :type link :value "Open In Browser" :tag nil :notify t) (:span (133 136) :text "(*)" :type radio-button :value t :tag nil :notify t) (:span (147 153) :text "[Open]" :type link :value "Open" :tag nil :notify t) (:span (154 160) :text "[Stop]" :type link :value "Stop" :tag nil :notify t) (:span (161 169) :text "[Delete]" :type link :value "Delete" :tag nil :notify t) (:span (232 238) :text "[Open]" :type link :value "Open" :tag nil :notify t)) :keys (("n" ein:notebooklist-next-item) ("p" ein:notebooklist-prev-item) ("TAB" widget-forward) ("RET" widget-button-press) ("C-c C-r" ein:notebooklist-reload))) :navigation ((:next (:point 41 :line "" :widget nil)) (:previous (:point 1 :line "Contents API 5 (http://127.0.0.1:37676)" :widget nil)) (:tab (:point 45 :line " | [Home] |" :widget (:type link :text "[Home]" :value "Home"))) (:activated-home (:point 1 :line "Contents API 5 (http://127.0.0.1:37676)" :widget nil))) :after-home (:buffer "*ein:notebooklist http://127.0.0.1:37676*" :mode ein:notebooklist-mode :readonly t :url "http://127.0.0.1:37676" :path "" :api-version 5 :kernel (:name "python3" :display "Python 3" :language "python") :text "Contents API 5 (http://127.0.0.1:37676)\n\n | [Home] |\n\n[New Notebook] [Resync] [Open In Browser]\n\nCreate New Notebooks Using Kernel:\n(*) Python 3\n\n[Open] [Stop] [Delete] : Café analysis 界.ipynb                            2026-08-12\n[Open]                 : notes Ω.txt                                       2026-08-11\n" :point 1 :line "Contents API 5 (http://127.0.0.1:37676)" :widgets ((:span (45 51) :text "[Home]" :type link :value "Home" :tag nil :notify t) (:span (55 69) :text "[New Notebook]" :type link :value "New Notebook" :tag nil :notify t) (:span (70 78) :text "[Resync]" :type link :value "Resync" :tag nil :notify t) (:span (79 96) :text "[Open In Browser]" :type link :value "Open In Browser" :tag nil :notify t) (:span (133 136) :text "(*)" :type radio-button :value t :tag nil :notify t) (:span (147 153) :text "[Open]" :type link :value "Open" :tag nil :notify t) (:span (154 160) :text "[Stop]" :type link :value "Stop" :tag nil :notify t) (:span (161 169) :text "[Delete]" :type link :value "Delete" :tag nil :notify t) (:span (232 238) :text "[Open]" :type link :value "Open" :tag nil :notify t)) :keys (("n" ein:notebooklist-next-item) ("p" ein:notebooklist-prev-item) ("TAB" widget-forward) ("RET" widget-button-press) ("C-c C-r" ein:notebooklist-reload))) :registered ("http://127.0.0.1:37676") :api-version "5" :hierarchy ((:name "Café analysis 界.ipynb" :path "Café analysis 界.ipynb" :type "notebook" :session (:id "session-café" :kernel-id "kernel-界" :kernel-name "python3")) (:name "notes Ω.txt" :path "notes Ω.txt" :type "file" :session nil))) :requests ((:url "http://127.0.0.1:37676/api/spec.yaml" :type nil :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success nil :error nil) (:url "http://127.0.0.1:37676/api/kernelspecs" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t) (:url "http://127.0.0.1:37676/api/contents" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t) (:url "http://127.0.0.1:37676/api/sessions" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t) (:url "http://127.0.0.1:37676/api/sessions" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t) (:url "http://127.0.0.1:37676/api/contents" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t) (:url "http://127.0.0.1:37676/api/contents" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t) (:url "http://127.0.0.1:37676/api/sessions" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t)))"#
        ]],
    )
}

fn missing_notebook_directory_reports_404_without_ui_then_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "missing_notebook_directory_reports_404_without_ui_then_recovers",
        r####"(ein376-test-run
 (lambda ()
   (let ((ein:notebooklist-map (make-hash-table :test 'equal))
         (*ein:notebook-api-version* (make-hash-table :test 'equal))
         (*ein:kernelspecs* (make-hash-table :test 'equal))
         (*ein:content-hierarchy* (make-hash-table :test 'equal))
         (ein:query-xsrf-cache (make-hash-table :test 'equal))
         (ein:query-authorization-tokens (make-hash-table :test 'equal))
         (ein:notebooklist-sort-field :name)
         (ein:notebooklist-sort-order :ascending)
         (ein:notebooklist-date-format "%F")
         (ein:force-sync t)
         (deferred:queue nil)
         (ein376-test-http-router
          #'ein376-test-notebooklist-missing-response)
         callback-state
         error-state)
     (ein376-test-with-http-replay
      (lambda ()
        (ein:notebooklist-open*
         "http://127.0.0.1:37676" "missing folder" t
         (lambda (buffer url-or-port)
           (setq callback-state
                 (list :buffer (buffer-name buffer) :url url-or-port)))
         (lambda (&rest failure)
           (setq error-state (copy-tree failure))))
        (deferred:flush-queue!)
        (let ((failed
               (list :callback callback-state
                     :error error-state
                     :registered (ein:notebooklist-keys)
                     :buffer
                     (and (get-buffer
                           "*ein:notebooklist http://127.0.0.1:37676*")
                          t)
                     :request-count (length ein376-test-http-ledger)
                     :hierarchy-count
                     (length (ein:content-need-hierarchy
                              "http://127.0.0.1:37676")))))
          (setq callback-state nil
                error-state nil
                ein376-test-http-router
                #'ein376-test-notebooklist-response)
          (ein:notebooklist-open*
           "http://127.0.0.1:37676" "" t
           (lambda (buffer url-or-port)
             (setq callback-state
                   (list :buffer (buffer-name buffer) :url url-or-port)))
           (lambda (&rest failure)
             (setq error-state (copy-tree failure))))
          (deferred:flush-queue!)
          (let ((buffer (ein:notebooklist-get-buffer
                         "http://127.0.0.1:37676")))
            (unless (and callback-state (buffer-live-p buffer))
              (error "notebook-list recovery did not produce a live buffer"))
            (with-current-buffer buffer
              (list :failed failed
                    :recovered-callback callback-state
                    :recovered-error error-state
                    :recovered (ein376-test-notebooklist-state)
                    :request-count (length ein376-test-http-ledger))))))))))"####,
        expect![[
            r#"OK (:value (:failed (:callback nil :error ("http://127.0.0.1:37676" 404) :registered nil :buffer nil :request-count 5 :hierarchy-count 2) :recovered-callback (:buffer "*ein:notebooklist http://127.0.0.1:37676*" :url "http://127.0.0.1:37676") :recovered-error nil :recovered (:buffer "*ein:notebooklist http://127.0.0.1:37676*" :mode ein:notebooklist-mode :readonly t :url "http://127.0.0.1:37676" :path "" :api-version 5 :kernel (:name "python3" :display "Python 3" :language "python") :text "Contents API 5 (http://127.0.0.1:37676)\n\n | [Home] |\n\n[New Notebook] [Resync] [Open In Browser]\n\nCreate New Notebooks Using Kernel:\n(*) Python 3\n\n[Open] [Stop] [Delete] : Café analysis 界.ipynb                            2026-08-12\n[Open]                 : notes Ω.txt                                       2026-08-11\n" :point 318 :line "" :widgets ((:span (45 51) :text "[Home]" :type link :value "Home" :tag nil :notify t) (:span (55 69) :text "[New Notebook]" :type link :value "New Notebook" :tag nil :notify t) (:span (70 78) :text "[Resync]" :type link :value "Resync" :tag nil :notify t) (:span (79 96) :text "[Open In Browser]" :type link :value "Open In Browser" :tag nil :notify t) (:span (133 136) :text "(*)" :type radio-button :value t :tag nil :notify t) (:span (147 153) :text "[Open]" :type link :value "Open" :tag nil :notify t) (:span (154 160) :text "[Stop]" :type link :value "Stop" :tag nil :notify t) (:span (161 169) :text "[Delete]" :type link :value "Delete" :tag nil :notify t) (:span (232 238) :text "[Open]" :type link :value "Open" :tag nil :notify t)) :keys (("n" ein:notebooklist-next-item) ("p" ein:notebooklist-prev-item) ("TAB" widget-forward) ("RET" widget-button-press) ("C-c C-r" ein:notebooklist-reload))) :request-count 11) :requests ((:url "http://127.0.0.1:37676/api/spec.yaml" :type nil :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success nil :error nil) (:url "http://127.0.0.1:37676/api/kernelspecs" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t) (:url "http://127.0.0.1:37676/api/contents/missing%20folder" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t :response (:status 404 :data (:message "No such notebook directory: missing folder") :error (error "HTTP 404: missing folder"))) (:url "http://127.0.0.1:37676/api/sessions" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t) (:url "http://127.0.0.1:37676/api/contents" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t) (:url "http://127.0.0.1:37676/api/spec.yaml" :type nil :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success nil :error nil) (:url "http://127.0.0.1:37676/api/kernelspecs" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t) (:url "http://127.0.0.1:37676/api/contents" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t) (:url "http://127.0.0.1:37676/api/sessions" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t) (:url "http://127.0.0.1:37676/api/sessions" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t) (:url "http://127.0.0.1:37676/api/contents" :type "GET" :headers (("User-Agent" . "Mozilla/5.0")) :data nil :encoding binary :sync t :timeout 10.0 :success t :error t)))"#
        ]],
    )
}

#[test]
fn ein_practical_workflows_batch() {
    let cases = vec![
        parses_and_renders_a_real_nbformat4_notebook(),
        public_cell_editing_preserves_content_order_and_cell_kill_ring(),
        output_visibility_and_clearing_preserve_prompts_and_exact_error_data(),
        public_save_serializes_exact_nbformat4_content_through_http_boundary(),
        public_notebook_list_queries_and_renders_real_widget_ui(),
        missing_notebook_directory_reports_404_without_ui_then_recovers(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "ein_practical_workflows_batch",
        "ein_parity",
        &cases,
    );
}
