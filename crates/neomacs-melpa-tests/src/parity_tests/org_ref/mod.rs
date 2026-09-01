use std::time::Duration;

use crate::{CachedMelpaOracle, ORG_REF_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ORG_REF_TEST_TIMEOUT: Duration = Duration::from_secs(300);

const ORG_REF_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'org)
(require 'xref)
(require 'bibtex)
(require 'browse-url)
(require 'url)
(require 'network-stream)
;; `ffap' otherwise probes ambient C/C++ compilers the first time export/link
;; machinery loads it.  Reject only those owned initialization attempts and let
;; ffap's own `ignore-errors' take its normal deterministic fallback paths.
(defvar org-ref362-test-ffap-prime-calls nil)
(defun org-ref362-test-ffap-prime-reject (_original &rest arguments)
  (push (copy-tree arguments) org-ref362-test-ffap-prime-calls)
  (error "Blocked unrelated ffap compiler discovery"))
(let ((org-ref362-test-ffap-prime-calls nil))
  (advice-add 'call-process :around #'org-ref362-test-ffap-prime-reject)
  (unwind-protect
      (require 'ffap)
    (advice-remove 'call-process #'org-ref362-test-ffap-prime-reject))
  (unless (equal (nreverse org-ref362-test-ffap-prime-calls)
                 '(("gcc" nil t nil "--version")
                   ("gcc" nil (t nil) nil "-print-multiarch")
                   ("g++" nil t nil "-v")))
    (error "Unexpected ffap compiler discovery calls: %S"
           org-ref362-test-ffap-prime-calls))
  (when (advice-member-p #'org-ref362-test-ffap-prime-reject 'call-process)
    (error "ffap compiler guard survived priming")))
(require 'org-ref)

;; GNU reserves the menu-bar row lazily.  Establish that process baseline before
;; any shared case snapshots windows, buffers, or command-loop state.
(set-window-configuration (current-window-configuration))
(encode-coding-string "Org Ref 界" 'utf-8-unix)
;; The package's load-time Org hook installs its Easy Menu on the first real
;; Org buffer.  Make that irreversible registration part of the common prelude.
(with-temp-buffer (org-mode))

(defconst org-ref362-test-state-symbols
  '(bibtex-completion-bibliography bibtex-completion-library-path
    bibtex-completion-notes-path bibtex-completion-watch-bibliography
    bibtex-completion-display-formats
    bibtex-completion-display-formats-internal bibtex-completion-cache
    bibtex-completion-string-cache bibtex-completion-string-hash-table
    bibtex-completion-cached-notes-keys
    bibtex-completion-file-watch-descriptors
    org-ref-insert-link-function org-ref-insert-cite-function
    org-ref-insert-label-function org-ref-insert-ref-function
    org-ref-default-citation-link org-ref-cite-insert-version
    org-ref-default-ref-type org-ref-enable-multi-file-references
    org-ref-project-label-cache org-ref-file-timestamps
    org-ref-glossary-file-cache org-ref-acronym-file-cache
    org-ref-csl-default-style org-ref-csl-default-locale
    org-ref-footnote-counter org-ref-prefix-arg org-latex-prefer-user-labels
    org-link-elisp-confirm-function
    org-export-before-parsing-functions
    org-export-before-processing-functions
    org-export-filter-link-functions org-export-timestamp-file
    org-mark-ring-length org-mark-ring org-mark-ring-last-goto
    org-window-config-before-follow-link
    xref--history xref-history-storage
    browse-url-browser-function
    minibuffer-setup-hook minibuffer-exit-hook minibuffer-history
    extended-command-history command-history suggest-key-bindings
    execute-extended-command--binding-timer
    unread-command-events executing-kbd-macro this-command real-this-command
    last-command real-last-command last-command-event last-input-event
    current-prefix-arg prefix-arg deactivate-mark
    enable-local-variables enable-dir-local-variables create-lockfiles
    vc-handled-backends)
  "Global editor/package state restored after each Org Ref workflow.")

(defvar org-ref362-test-owned-buffers nil)
(defvar org-ref362-test-owned-watches nil)
(defvar org-ref362-test-stopped-watch-events nil)
(defvar org-ref362-test-browser-calls nil)
(defvar org-ref362-test-browser-error nil)
(defvar org-ref362-test-completions nil)
(defvar org-ref362-test-minibuffer-input nil)
(defvar org-ref362-test-message-events nil)
(defvar org-ref362-test-warning-events nil)
(defvar org-ref362-test-confirmation-events nil)
(defvar org-ref362-test-mark-return-events nil)
(defvar org-ref362-test-external-events nil)
(defvar org-ref362-test-external-advices nil)

(defconst org-ref362-test-forbidden-external-functions
  '(call-process call-process-region process-file start-process
    start-file-process make-process make-network-process
    open-network-stream url-retrieve url-retrieve-synchronously)
  "External process and network boundaries forbidden in Org Ref workflows.")

(defun org-ref362-test-reject-external (operation _original &rest arguments)
  "Record and reject external OPERATION with exact ARGUMENTS."
  (push (list :operation operation :arguments (copy-tree arguments))
        org-ref362-test-external-events)
  (error "Unexpected Org Ref external operation: %S %S"
         operation arguments))

(defun org-ref362-test-install-external-guards ()
  "Install exact fail-closed guards on external process/network boundaries."
  (setq org-ref362-test-external-advices nil)
  (dolist (operation org-ref362-test-forbidden-external-functions)
    (unless (fboundp operation)
      (error "Missing Org Ref external boundary function: %S" operation))
    (let ((advice (apply-partially #'org-ref362-test-reject-external operation)))
      (advice-add operation :around advice)
      (push (cons operation advice) org-ref362-test-external-advices))))

(defun org-ref362-test-remove-external-guards ()
  "Remove every installed external boundary guard."
  (let (errors)
    (dolist (entry org-ref362-test-external-advices)
      (condition-case condition
          (progn
            (advice-remove (car entry) (cdr entry))
            (when (advice-member-p (cdr entry) (car entry))
              (error "Org Ref external guard survived removal: %S"
                     (car entry))))
        (t (push (list (car entry)
                       (org-ref362-test-condition-state condition))
                 errors))))
    (when errors
      (error "Org Ref external guard cleanup failures: %S"
             (nreverse errors)))))

(defun org-ref362-test-variable-state (symbol)
  "Return SYMBOL's exact boundness and value identity."
  (if (boundp symbol)
      (list :bound t :value (symbol-value symbol))
    '(:bound nil)))

(defun org-ref362-test-restore-variable (symbol state)
  "Restore SYMBOL to exact STATE."
  (if (plist-get state :bound)
      (set symbol (plist-get state :value))
    (makunbound symbol)))

(defun org-ref362-test-variable-restored-p (symbol state)
  "Return non-nil when SYMBOL has exact STATE identity."
  (if (plist-get state :bound)
      (and (boundp symbol) (eq (symbol-value symbol) (plist-get state :value)))
    (not (boundp symbol))))

(defun org-ref362-test-buffer-content-state (name)
  "Return exact mutable state for existing buffer NAME."
  (when-let* ((buffer (get-buffer name)))
    (with-current-buffer buffer
      (let ((minimum (point-min)) (maximum (point-max)))
        (list :buffer buffer
              :text (save-restriction (widen) (buffer-string))
              :point (point) :modified (buffer-modified-p)
              :undo (copy-tree buffer-undo-list) :read-only buffer-read-only
              :mode major-mode :min minimum :max maximum)))))

(defun org-ref362-test-restore-buffer-content (state)
  "Restore an existing buffer to exact STATE."
  (when state
    (let ((buffer (plist-get state :buffer)))
      (unless (buffer-live-p buffer)
        (error "Org Ref baseline diagnostic buffer died: %S" buffer))
      (with-current-buffer buffer
        (when (and (symbolp (plist-get state :mode))
                   (fboundp (plist-get state :mode))
                   (not (eq major-mode (plist-get state :mode))))
          (funcall (plist-get state :mode)))
        (let ((inhibit-read-only t))
          (widen)
          (erase-buffer)
          (insert (plist-get state :text)))
        (goto-char (min (plist-get state :point) (point-max)))
        (setq buffer-undo-list (copy-tree (plist-get state :undo))
              buffer-read-only (plist-get state :read-only))
        (set-buffer-modified-p (plist-get state :modified))
        (narrow-to-region (plist-get state :min) (plist-get state :max))))))

(defun org-ref362-test-buffer-content-restored-p (state)
  "Return non-nil when existing diagnostic buffer STATE is restored."
  (or (null state)
      (let ((buffer (plist-get state :buffer)))
        (and (buffer-live-p buffer)
             (with-current-buffer buffer
               (and (equal (save-restriction (widen) (buffer-string))
                           (plist-get state :text))
                    (= (point) (plist-get state :point))
                    (eq (buffer-modified-p) (plist-get state :modified))
                    (equal buffer-undo-list (plist-get state :undo))
                    (eq buffer-read-only (plist-get state :read-only))
                    (eq major-mode (plist-get state :mode))
                    (= (point-min) (plist-get state :min))
                    (= (point-max) (plist-get state :max))))))))

(defun org-ref362-test-window-state ()
  "Return exact structural state for ordinary windows."
  (mapcar
   (lambda (window)
     (list :window window :buffer (window-buffer window)
           :edges (window-edges window) :point (window-point window)
           :start (window-start window) :hscroll (window-hscroll window)
           :vscroll (window-vscroll window t)
           :prev (copy-tree (window-prev-buffers window))
           :next (copy-tree (window-next-buffers window))))
   (window-list nil 'no-minibuf)))

(defun org-ref362-test-restore-windows (configuration state)
  "Restore window CONFIGURATION and exact semantic STATE."
  (set-window-configuration configuration)
  (dolist (entry state)
    (let ((window (plist-get entry :window)))
      (unless (window-live-p window)
        (error "Org Ref baseline window died: %S" window))
      (set-window-prev-buffers window (copy-tree (plist-get entry :prev)))
      (set-window-next-buffers window (copy-tree (plist-get entry :next)))
      (set-window-point window (plist-get entry :point))
      (set-window-start window (plist-get entry :start) 'noforce)
      (set-window-hscroll window (plist-get entry :hscroll))
      (set-window-vscroll window (plist-get entry :vscroll) t))))

(defun org-ref362-test-windows-restored-p (state)
  "Return non-nil when current windows exactly match STATE."
  (equal (org-ref362-test-window-state) state))

(defun org-ref362-test-write (file contents)
  "Write UTF-8 CONTENTS to owned FILE without visiting or locking it."
  (make-directory (file-name-directory file) t)
  (with-temp-buffer
    (set-buffer-multibyte t)
    (insert contents)
    (let ((coding-system-for-write 'utf-8-unix)
          (create-lockfiles nil))
      (write-region (point-min) (point-max) file nil 'silent))))

(defun org-ref362-test-owner-root ()
  "Return and validate the harness-provided sandbox owner."
  (let ((raw (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
    (unless (and (stringp raw) (not (string-empty-p raw))
                 (file-name-absolute-p raw) (file-directory-p raw))
      (error "Unsafe Org Ref sandbox owner: %S" raw))
    (file-name-as-directory (file-truename raw))))

(defun org-ref362-test-allocate-world (case-id)
  "Allocate and return an owned world for CASE-ID."
  (unless (and (stringp case-id)
               (string-match-p "\\`[a-z0-9-]+\\'" case-id))
    (error "Unsafe Org Ref case id: %S" case-id))
  (let* ((owner (org-ref362-test-owner-root))
         (root (file-name-as-directory
                (expand-file-name
                 (format "org-ref362-%s project 界" case-id) owner))))
    (unless (and (file-name-absolute-p root)
                 (file-in-directory-p root owner)
                 (not (equal (directory-file-name root)
                             (directory-file-name owner)))
                 (not (file-exists-p root)))
      (error "Unsafe or reused Org Ref root: %S" root))
    (condition-case condition
        (progn
          (make-directory root t)
          (list :owner owner :root root
                :main (expand-file-name "paper 界.org" root)
                :chapter (expand-file-name "chapter λ.org" root)
                :bib (expand-file-name "references.bib" root)
                :library (file-name-as-directory
                          (expand-file-name "library" root))))
      (t
       (when (and (file-directory-p root) (file-in-directory-p root owner))
         (delete-directory root t))
       (signal (car condition) (cdr condition))))))

(defconst org-ref362-test-bibliography
  "@article{ada2024deterministic,
  author = {Lovelace, Ada and Lei, 李},
  title = {Deterministic Café Workflow},
  journal = {Journal of Reproducible Examples},
  volume = {7},
  number = {2},
  pages = {11--19},
  year = {2024},
  url = {https://example.invalid/explicit?x=1},
  doi = {10.1000/alpha}
}

@article{alpha2024,
  author = {Ada Alpha and Bob Beta},
  title = {Deterministic Widgets in Practice},
  journal = {Journal of Reproducible Examples},
  volume = {7},
  number = {2},
  pages = {11--19},
  year = {2024},
  doi = {10.1000/alpha}
}

@book{gamma2020,
  author = {Gamma, Grace},
  title = {Structured Tools},
  publisher = {Example Press},
  address = {Test City},
  year = {2020},
  doi = {10.1000/fallback}
}

@misc{plain2019,
  author = {Plain, Pat},
  title = {No External Locator},
  year = {2019}
}
"
  "Exact realistic bibliography shared by Org Ref workflows.")

(defconst org-ref362-test-chapter
  "#+title: Included λ Chapter

#+name: included-target
Included target body.

\\begin{equation}
E = mc^2
\\label{eq:energy}
\\end{equation}

* Included custom
:PROPERTIES:
:CUSTOM_ID: custom-λ
:END:
Custom target body.

#+name: table-界
| n | value |
| 1 | λ     |
"
  "Exact included Org document.")

(defconst org-ref362-test-valid-main
  "#+title: Practical Org Ref Workflow
#+include: \"chapter λ.org\"

* Local target
:PROPERTIES:
:CUSTOM_ID: local-target
:END:

Evidence [[cite:&ada2024deterministic]] and [[cite:&gamma2020]].
See ref:local-target and ref:included-target.

bibliography:references.bib
"
  "Valid base Org document.")

(defun org-ref362-test-materialize-world (world)
  "Materialize exact files for WORLD."
  (make-directory (plist-get world :library) t)
  (org-ref362-test-write (plist-get world :bib)
                         org-ref362-test-bibliography)
  (org-ref362-test-write (plist-get world :chapter)
                         org-ref362-test-chapter)
  (org-ref362-test-write (plist-get world :main)
                         org-ref362-test-valid-main)
  world)

(defun org-ref362-test-write-main (world contents)
  "Replace WORLD's main document with CONTENTS."
  (let* ((file (plist-get world :main))
         (buffer (get-file-buffer file)))
    (when buffer
      (with-current-buffer buffer
        (let ((inhibit-read-only t))
          (erase-buffer)
          (insert contents)
          (let ((coding-system-for-write 'utf-8-unix)
                (create-lockfiles nil))
            (write-region (point-min) (point-max) file nil 'silent)))
        (set-buffer-modified-p nil)
        (goto-char (point-min))))
    (unless buffer (org-ref362-test-write file contents))))

(defun org-ref362-test-visit (file mode)
  "Visit owned FILE with MODE and inherited local configuration disabled."
  (let ((enable-local-variables nil)
        (enable-dir-local-variables nil)
        (create-lockfiles nil))
    (let ((buffer (find-file-noselect file)))
      (cl-pushnew buffer org-ref362-test-owned-buffers)
      (with-current-buffer buffer
        (funcall mode)
        (font-lock-ensure))
      buffer)))

(defun org-ref362-test-fresh-org-ring ()
  "Return an owned circular Org mark ring."
  (let (ring)
    (dotimes (_ org-mark-ring-length) (push (make-marker) ring))
    (setcdr (nthcdr (1- org-mark-ring-length) ring) ring)
    ring))

(defun org-ref362-test-configure-world (world)
  "Configure public Org Ref state for WORLD."
  (when (and (boundp 'execute-extended-command--binding-timer)
             execute-extended-command--binding-timer)
    (error "Ambient extended-command binding timer is active: %S"
           execute-extended-command--binding-timer))
  ;; Establish marker ownership first so the outer unwind can always detach
  ;; the fresh circular ring after any later configuration failure.
  (setq org-mark-ring-length 4
        org-mark-ring (org-ref362-test-fresh-org-ring)
        org-mark-ring-last-goto nil)
  (setq bibtex-completion-bibliography (list (plist-get world :bib))
        bibtex-completion-library-path (plist-get world :library)
        bibtex-completion-notes-path nil
        bibtex-completion-watch-bibliography t
        bibtex-completion-display-formats
        '((t . "${author:28} ${title:34} ${year:4} ${=type=:7}"))
        bibtex-completion-display-formats-internal nil
        bibtex-completion-cache nil
        bibtex-completion-string-cache nil
        bibtex-completion-string-hash-table nil
        bibtex-completion-cached-notes-keys nil
        bibtex-completion-file-watch-descriptors nil
        org-ref-project-label-cache (make-hash-table :test #'equal)
        org-ref-file-timestamps (make-hash-table :test #'equal)
        org-ref-glossary-file-cache (make-hash-table :test #'equal)
        org-ref-acronym-file-cache (make-hash-table :test #'equal)
        org-ref-enable-multi-file-references t
        org-ref-default-ref-type "ref"
        org-ref-default-citation-link "cite"
        org-ref-cite-insert-version 3
        org-ref-csl-default-style "chicago-author-date-16th-edition.csl"
        org-ref-csl-default-locale "en-US"
        org-ref-footnote-counter 0
        org-latex-prefer-user-labels t
        org-export-before-parsing-functions nil
        org-export-before-processing-functions nil
        org-export-filter-link-functions nil
        org-export-timestamp-file nil
        enable-local-variables nil
        enable-dir-local-variables nil
        create-lockfiles nil
        vc-handled-backends nil
        minibuffer-history nil
        extended-command-history nil
        command-history nil
        suggest-key-bindings nil
        execute-extended-command--binding-timer nil
        org-window-config-before-follow-link nil
        xref--history (cons nil nil)
        xref-history-storage #'xref-global-history
        org-ref362-test-owned-watches nil
        org-ref362-test-browser-calls nil
        org-ref362-test-browser-error nil
        org-ref362-test-completions nil
        org-ref362-test-minibuffer-input nil
        org-ref362-test-message-events nil
        org-ref362-test-warning-events nil
        org-ref362-test-confirmation-events nil
        org-ref362-test-mark-return-events nil)
  (define-key org-mode-map (kbd "C-c ]") #'org-ref-insert-link))

(defun org-ref362-test-run-command-loop (keys)
  "Drive KEYS through the real command loop in the selected owned buffer."
  (unless (eq (current-buffer) (window-buffer (selected-window)))
    (error "Org Ref command buffer is not selected"))
  (unless (zerop (minibuffer-depth))
    (error "Org Ref command began inside a minibuffer"))
  (unless (null unread-command-events)
    (error "Org Ref command inherited unread events: %S"
           unread-command-events))
  (execute-kbd-macro (kbd keys))
  (unless (and (null unread-command-events) (zerop (minibuffer-depth)))
    (error "Org Ref command leaked loop state: %S/%S"
           unread-command-events (minibuffer-depth)))
  (list :point (point) :mark (mark t) :active mark-active
        :unread unread-command-events :minibuffer-depth (minibuffer-depth)))

(defun org-ref362-test-mark-return-observer (original &rest arguments)
  "Observe exact state after the real public Org mark return ORIGINAL."
  (let ((value (apply original arguments)))
    (push (list :value value :point (org-ref362-test-point-state)
                :ring (org-ref362-test-org-ring-state))
          org-ref362-test-mark-return-events)
    value))

(defun org-ref362-test-drive-mark-returns (keys)
  "Drive contiguous public mark-return KEYS and observe every destination."
  (setq org-ref362-test-mark-return-events nil)
  (advice-add 'org-mark-ring-goto :around
              #'org-ref362-test-mark-return-observer)
  (unwind-protect
      (let ((loop (org-ref362-test-run-command-loop keys)))
        (list :loop loop
              :returns (nreverse org-ref362-test-mark-return-events)))
    (advice-remove 'org-mark-ring-goto
                   #'org-ref362-test-mark-return-observer)))

(defun org-ref362-test-completing-read-observer
    (original prompt collection &optional predicate require-match initial-input
              history default inherit-input-method)
  "Observe a real call to ORIGINAL `completing-read'."
  (let ((org-ref362-test-minibuffer-input :not-exited)
        (minibuffer-exit-hook
         (cons (lambda ()
                 (setq org-ref362-test-minibuffer-input
                       (minibuffer-contents-no-properties)))
               minibuffer-exit-hook)))
    (let ((result (funcall original prompt collection predicate require-match
                           initial-input history default inherit-input-method)))
      (push (list :prompt prompt
                  :collection (mapcar #'substring-no-properties
                                      (all-completions "" collection predicate))
                  :require-match require-match :initial initial-input
                  :history history :default default
                  :input org-ref362-test-minibuffer-input :selected result
                  :history-after (copy-tree minibuffer-history))
            org-ref362-test-completions)
      result)))

(defun org-ref362-test-drive-completion (keys)
  "Drive KEYS while narrowly observing real completion."
  (setq org-ref362-test-completions nil)
  (advice-add 'completing-read :around
              #'org-ref362-test-completing-read-observer)
  (unwind-protect
      (let ((loop (org-ref362-test-run-command-loop keys)))
        (list :loop loop :reads (nreverse org-ref362-test-completions)))
    (advice-remove 'completing-read
                   #'org-ref362-test-completing-read-observer)))

(defun org-ref362-test-message-observer (original format-string &rest args)
  "Observe real calls to ORIGINAL `message'."
  (let ((result (apply original format-string args)))
    (push (and format-string
               (substring-no-properties
                (apply #'format-message format-string args)))
          org-ref362-test-message-events)
    result))

(defun org-ref362-test-drive-with-messages (keys)
  "Drive KEYS and return only messages emitted by the action."
  (setq org-ref362-test-message-events nil)
  (advice-add 'message :around #'org-ref362-test-message-observer)
  (unwind-protect
      (let ((loop (org-ref362-test-run-command-loop keys)))
        (list :loop loop :messages (nreverse org-ref362-test-message-events)))
    (advice-remove 'message #'org-ref362-test-message-observer)))

(defun org-ref362-test-warning-observer
    (original type message &optional level buffer-name)
  "Observe real calls to ORIGINAL `display-warning'."
  (push (list :type type :message message :level level
              :buffer-name buffer-name)
        org-ref362-test-warning-events)
  (funcall original type message level buffer-name))

(defun org-ref362-test-confirm-elisp-link (link)
  "Record and approve the real user confirmation boundary for LINK."
  (push (substring-no-properties link) org-ref362-test-confirmation-events)
  t)

(defun org-ref362-test-reset-org-buffer (contents)
  "Reset the current owned Org buffer to exact CONTENTS."
  (let ((inhibit-read-only t))
    (erase-buffer)
    (insert contents))
  (org-mode)
  (goto-char (point-min))
  (setq buffer-undo-list nil)
  (set-buffer-modified-p nil)
  (font-lock-ensure)
  (buffer-string))

(defun org-ref362-test-browser-recorder (url &optional new-window)
  "Record exact external browser URL and NEW-WINDOW without launching it."
  (push (list :url url :new-window new-window) org-ref362-test-browser-calls)
  (when org-ref362-test-browser-error
    (error "%s" org-ref362-test-browser-error))
  nil)

(defun org-ref362-test-browser-state ()
  "Return browser calls in public invocation order."
  (nreverse (copy-tree org-ref362-test-browser-calls)))

(defun org-ref362-test-own-current-watches ()
  "Capture and validate the exact file watches created for the owned BibTeX file."
  (setq org-ref362-test-owned-watches
        (copy-sequence bibtex-completion-file-watch-descriptors))
  (list :bibliography
        (mapcar #'file-name-nondirectory bibtex-completion-bibliography)
        :count (length org-ref362-test-owned-watches)
        :valid (mapcar #'file-notify-valid-p org-ref362-test-owned-watches)))

(defun org-ref362-test-watch-observer (original file flags callback)
  "Call real file watch ORIGINAL and record the returned owned descriptor."
  (let ((descriptor (funcall original file flags callback)))
    (cl-pushnew descriptor org-ref362-test-owned-watches :test #'equal)
    descriptor))

(defun org-ref362-test-stopped-watch-observer (original object)
  "Observe the exact stopped watch OBJECT before calling real ORIGINAL."
  (when (file-notify-p object)
    (push (copy-tree (file-notify--event object))
          org-ref362-test-stopped-watch-events))
  (funcall original object))

(defun org-ref362-test-remove-owned-watch (descriptor)
  "Remove DESCRIPTOR and settle only its exact queued stopped event."
  (let ((org-ref362-test-stopped-watch-events nil)
        returned-event)
    (advice-add 'file-notify-handle-event :around
                #'org-ref362-test-stopped-watch-observer)
    (unwind-protect
        (progn
          (when (file-notify-valid-p descriptor)
            (file-notify-rm-watch descriptor))
          ;; GNU consumes this cleanup event immediately.  Neo queues it as a
          ;; special input event.  A zero-time real read dispatches only that
          ;; exact owned callback; it never pumps timers or ambient processes.
          (setq returned-event (read-event nil nil 0))
          (when returned-event
            (error "Unexpected input while settling Org Ref watch: %S"
                   returned-event))
          (unless (or
                   (null org-ref362-test-stopped-watch-events)
                   (equal org-ref362-test-stopped-watch-events
                          (list (list descriptor 'stopped
                                     (file-truename
                                       (car bibtex-completion-bibliography))))))
            (error "Unexpected Org Ref stopped-watch events: %S"
                   org-ref362-test-stopped-watch-events))
          (when (input-pending-p)
            (error "Input survived Org Ref stopped-watch settlement"))
          (when (file-notify-valid-p descriptor)
            (error "Org Ref owned file watch survived: %S" descriptor)))
      (advice-remove 'file-notify-handle-event
                     #'org-ref362-test-stopped-watch-observer)
      (when (advice-member-p #'org-ref362-test-stopped-watch-observer
                             'file-notify-handle-event)
        (error "Org Ref stopped-watch observer survived cleanup")))))

(defun org-ref362-test-point-state ()
  "Return exact selected-buffer point context."
  (list :file (and buffer-file-name (file-name-nondirectory buffer-file-name))
        :mode major-mode :line (line-number-at-pos) :point (point)
        :column (current-column)
        :context (buffer-substring-no-properties
                  (line-beginning-position) (line-end-position))
        :narrowed (buffer-narrowed-p)
        :selected (eq (current-buffer) (window-buffer (selected-window)))))

(defun org-ref362-test-marker-state (marker)
  "Return stable state for MARKER."
  (and (markerp marker) (marker-buffer marker)
       (list (file-name-nondirectory
              (or (buffer-file-name (marker-buffer marker)) ""))
             (marker-position marker))))

(defun org-ref362-test-org-ring-state ()
  "Return the finite stable view of the circular Org mark ring."
  (cl-loop for index below org-mark-ring-length
           for marker = (nth index org-mark-ring)
           when (marker-buffer marker)
           collect (org-ref362-test-marker-state marker)))

(defun org-ref362-test-xref-state ()
  "Return stable backward/forward xref histories."
  (let ((history (xref--get-history)))
    (list :back (mapcar #'org-ref362-test-marker-state (car history))
          :forward (mapcar #'org-ref362-test-marker-state (cdr history)))))

(defun org-ref362-test-link-state (needle)
  "Return exact Org link and text-property state at NEEDLE."
  (save-excursion
    (goto-char (point-min))
    (unless (search-forward needle nil t)
      (error "Missing Org Ref link needle: %S" needle))
    (goto-char (match-beginning 0))
    (let* ((element (org-element-context))
           (begin (org-element-property :begin element))
           (end (org-element-property :end element)))
      (list :needle needle :begin begin :end end
            :type (org-element-property :type element)
            :path (org-element-property :path element)
            :face-runs
            (let ((position begin) runs)
              (while (< position end)
                (let ((next (or (next-property-change position nil end) end)))
                  (push (list (- position begin) (- next begin)
                              (get-text-property position 'face)
                              (get-text-property position 'font-lock-face)
                              (get-text-property position 'cite-key)
                              (get-text-property position 'help-echo))
                        runs)
                  (setq position next)))
              (nreverse runs))))))

(defun org-ref362-test-document-state ()
  "Return exact mutable state of the current document."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point) :mark (mark t)
        :active mark-active :modified (buffer-modified-p)
        :undo (cond ((eq buffer-undo-list t) :disabled)
                    ((null buffer-undo-list) :empty)
                    (t :present))))

(defun org-ref362-test-output-state (name)
  "Return exact stable state of generated output buffer NAME."
  (when-let* ((buffer (get-buffer name)))
    (with-current-buffer buffer
      (list :name (buffer-name) :mode major-mode
            :text (buffer-substring-no-properties (point-min) (point-max))
            :point (point) :modified (buffer-modified-p)
            :read-only buffer-read-only :narrowed (buffer-narrowed-p)
            :min (point-min) :max (point-max)))))

(defun org-ref362-test-report-state ()
  "Return stable semantic sections from the public Org Ref report."
  (with-current-buffer (or (get-buffer "*org-ref*")
                           (error "Missing public Org Ref report buffer"))
    (cl-labels
        ((section (heading)
           (goto-char (point-min))
           (when (re-search-forward
                  (concat "^" (regexp-quote heading) "$") nil t)
             (buffer-substring-no-properties
              (line-beginning-position)
              (save-excursion
                (forward-line 1)
                (if (re-search-forward "^\\* " nil t)
                    (line-beginning-position)
                  (point-max)))))))
      (list :mode major-mode :read-only buffer-read-only
            :headings
            (org-element-map (org-element-parse-buffer) 'headline
              (lambda (headline)
                (org-element-property :raw-value headline)))
            :owned-source-title
            (save-excursion
              (goto-char (point-min))
              (and (search-forward "[paper 界.org]" nil t) t))
            :bad-citations (section "* Bad citations")
            :bad-refs (section "* Bad ref links")
            :bad-labels (section "* Multiply defined label links")
            :bad-files (section "* Bad files")))))

(defun org-ref362-test-bibtex-entry-state ()
  "Return exact location and representative parsed fields for current entry."
  (save-excursion
    (bibtex-beginning-of-entry)
    (let ((entry (bibtex-parse-entry)))
      (append (org-ref362-test-point-state)
              (list :key (cdr (assoc "=key=" entry))
                    :type (cdr (assoc "=type=" entry))
                    :author (cdr (assoc "author" entry))
                    :title (cdr (assoc "title" entry))
                    :year (cdr (assoc "year" entry)))))))

(defun org-ref362-test-condition-state (condition)
  "Return exact stable CONDITION state, normalizing only killed buffers."
  (list :symbol (car condition)
        :data (mapcar (lambda (datum)
                        (cond
                         ((bufferp datum) :killed-buffer)
                         ((markerp datum)
                          (list :marker
                                (and (marker-buffer datum)
                                     (file-name-nondirectory
                                      (or (buffer-file-name
                                           (marker-buffer datum)) "")))
                                (marker-position datum)))
                         (t datum)))
                      (cdr condition))
        :message (error-message-string condition)))

(defun org-ref362-test-attempt (phase thunk errors)
  "Run THUNK for cleanup PHASE and return updated ERRORS."
  (condition-case condition
      (progn (funcall thunk) errors)
    (t (cons (list phase (org-ref362-test-condition-state condition)) errors))))

(defun org-ref362-test-detach-markers ()
  "Detach every marker owned by the case's Org and xref histories."
  (dotimes (index org-mark-ring-length)
    (when-let* ((marker (nth index org-mark-ring)))
      (when (markerp marker) (set-marker marker nil nil))))
  (let ((history (xref--get-history)))
    (dolist (marker (append (car history) (cdr history)))
      (set-marker marker nil nil)))
  (setq org-mark-ring-last-goto nil))

(defun org-ref362-test-run (case-id thunk)
  "Run THUNK in an owned Org Ref world and clean every shared resource."
  (let* ((org-ref362-test-owned-buffers nil)
         (org-ref362-test-owned-watches nil)
         (org-ref362-test-browser-calls nil)
         (org-ref362-test-browser-error nil)
         (org-ref362-test-completions nil)
         (org-ref362-test-minibuffer-input nil)
         (org-ref362-test-message-events nil)
         (org-ref362-test-warning-events nil)
         (org-ref362-test-confirmation-events nil)
         (org-ref362-test-mark-return-events nil)
         (org-ref362-test-external-events nil)
         (org-ref362-test-external-advices nil)
         (world nil)
         (configured nil)
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (idle-timers-before (copy-sequence timer-idle-list))
         (buffer-before (current-buffer))
         (window-before (selected-window))
         (configuration-before (current-window-configuration))
         (windows-before (org-ref362-test-window-state))
         (warnings-before (org-ref362-test-buffer-content-state "*Warnings*"))
         (messages-before (org-ref362-test-buffer-content-state "*Messages*"))
         (report-baseline (get-buffer "*org-ref*"))
         (export-baseline (get-buffer "*org-ref ORG Export*"))
         (states-before
          (mapcar (lambda (symbol)
                    (cons symbol (org-ref362-test-variable-state symbol)))
                  org-ref362-test-state-symbols))
         (binding-before (lookup-key org-mode-map (kbd "C-c ]")))
         (body-value nil)
         (body-error nil)
         (cleanup-errors nil)
         (cleanup-state nil))
    (unwind-protect
        (condition-case condition
            (progn
              (org-ref362-test-install-external-guards)
              (advice-add 'file-notify-add-watch :around
                          #'org-ref362-test-watch-observer)
              (when report-baseline
                (with-current-buffer report-baseline
                  (rename-buffer
                   (generate-new-buffer-name
                    " *org-ref362 baseline report*"))))
              (when export-baseline
                (with-current-buffer export-baseline
                  (rename-buffer
                   (generate-new-buffer-name
                    " *org-ref362 baseline export*"))))
              (setq world (org-ref362-test-allocate-world case-id))
              (org-ref362-test-materialize-world world)
              (setq configured t)
              (org-ref362-test-configure-world world)
              (setq body-value (funcall thunk world)))
          (t (setq body-error (org-ref362-test-condition-state condition))))
      (setq cleanup-errors
            (org-ref362-test-attempt
             'remove-watch-observer
             (lambda ()
               (advice-remove 'file-notify-add-watch
                              #'org-ref362-test-watch-observer))
             cleanup-errors))
      (setq cleanup-errors
            (org-ref362-test-attempt
             'remove-external-guards
             #'org-ref362-test-remove-external-guards
             cleanup-errors))
      (let ((watch-index 0))
        (dolist (descriptor org-ref362-test-owned-watches)
          (setq cleanup-errors
                (org-ref362-test-attempt
                 (list 'remove-watch watch-index)
                 (lambda () (org-ref362-test-remove-owned-watch descriptor))
                 cleanup-errors))
          (setq watch-index (1+ watch-index))))
      (when configured
        (setq cleanup-errors
              (org-ref362-test-attempt
               'detach-markers #'org-ref362-test-detach-markers cleanup-errors)))
      ;; Quiesce asynchronous owners before restoring any ambient globals or
      ;; windows: a process sentinel or timer callback must not run afterward
      ;; and mutate the restored baseline.
      (let ((timer-index 0))
        (dolist (timer (append (copy-sequence timer-list)
                               (copy-sequence timer-idle-list)))
          (unless (or (memq timer timers-before)
                      (memq timer idle-timers-before))
            (setq cleanup-errors
                  (org-ref362-test-attempt
                   (list 'cancel-new-timer timer-index)
                   (lambda () (cancel-timer timer))
                   cleanup-errors))
            (setq timer-index (1+ timer-index)))))
      (let ((process-index 0))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (setq cleanup-errors
                  (org-ref362-test-attempt
                   (list 'reap-new-process process-index)
                   (lambda ()
                     (when (process-live-p process) (delete-process process))
                     (let ((deadline (+ (float-time) 1.0)))
                       (while (and (process-live-p process)
                                   (< (float-time) deadline))
                         (accept-process-output process 0.01)))
                     (when (process-live-p process)
                       (error "Org Ref process survived cleanup: %S"
                              process)))
                   cleanup-errors))
            (setq process-index (1+ process-index)))))
      (when world
        (let ((buffer-index 0))
          (dolist (buffer (buffer-list))
            (when (and (not (memq buffer buffers-before))
                       (buffer-live-p buffer))
              (setq cleanup-errors
                    (org-ref362-test-attempt
                     (list 'kill-new-buffer buffer-index (buffer-name buffer))
                     (lambda ()
                       (with-current-buffer buffer
                         (set-buffer-modified-p nil))
                       (kill-buffer buffer))
                     cleanup-errors))
              (setq buffer-index (1+ buffer-index))))))
      (dolist (entry states-before)
        (setq cleanup-errors
              (org-ref362-test-attempt
               (list 'restore-variable (car entry))
               (lambda ()
                 (org-ref362-test-restore-variable (car entry) (cdr entry)))
               cleanup-errors)))
      (setq cleanup-errors
            (org-ref362-test-attempt
             'restore-key
             (lambda ()
               (define-key org-mode-map (kbd "C-c ]")
                 (if (numberp binding-before) nil binding-before)))
             cleanup-errors))
      (setq cleanup-errors
            (org-ref362-test-attempt
             'restore-warnings
             (lambda () (org-ref362-test-restore-buffer-content warnings-before))
             cleanup-errors))
      (setq cleanup-errors
            (org-ref362-test-attempt
             'restore-messages
             (lambda () (org-ref362-test-restore-buffer-content messages-before))
             cleanup-errors))
      (setq cleanup-errors
            (org-ref362-test-attempt
             'restore-report-name
             (lambda ()
               (when report-baseline
                 (unless (buffer-live-p report-baseline)
                   (error "Org Ref baseline report buffer died"))
                 (with-current-buffer report-baseline
                   (rename-buffer "*org-ref*"))))
             cleanup-errors))
      (setq cleanup-errors
            (org-ref362-test-attempt
             'restore-export-name
             (lambda ()
               (when export-baseline
                 (unless (buffer-live-p export-baseline)
                   (error "Org Ref baseline export buffer died"))
                 (with-current-buffer export-baseline
                   (rename-buffer "*org-ref ORG Export*"))))
             cleanup-errors))
      (setq cleanup-errors
            (org-ref362-test-attempt
             'restore-windows
             (lambda ()
               (org-ref362-test-restore-windows
                configuration-before windows-before)
               (set-buffer buffer-before)
               (select-window window-before))
             cleanup-errors))
      (let ((process-index 0))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (setq cleanup-errors
                  (org-ref362-test-attempt
                   (list 'second-reap-process process-index)
                   (lambda ()
                     (when (process-live-p process) (delete-process process))
                     (let ((deadline (+ (float-time) 1.0)))
                       (while (and (process-live-p process)
                                   (< (float-time) deadline))
                         (accept-process-output process 0.01)))
                     (when (process-live-p process)
                       (error "Org Ref process survived second sweep: %S"
                              process)))
                   cleanup-errors))
            (setq process-index (1+ process-index)))))
      (let ((timer-index 0))
        (dolist (timer (append (copy-sequence timer-list)
                               (copy-sequence timer-idle-list)))
          (unless (or (memq timer timers-before)
                      (memq timer idle-timers-before))
            (setq cleanup-errors
                  (org-ref362-test-attempt
                   (list 'second-cancel-timer timer-index)
                   (lambda () (cancel-timer timer))
                   cleanup-errors))
            (setq timer-index (1+ timer-index)))))
      (let ((buffer-index 0))
        (dolist (buffer (buffer-list))
          (when (and (not (memq buffer buffers-before))
                     (buffer-live-p buffer))
            (setq cleanup-errors
                  (org-ref362-test-attempt
                   (list 'second-kill-buffer buffer-index
                         (buffer-name buffer))
                   (lambda ()
                     (with-current-buffer buffer (set-buffer-modified-p nil))
                     (kill-buffer buffer))
                   cleanup-errors))
            (setq buffer-index (1+ buffer-index)))))
      (when world
        (let* ((owner (plist-get world :owner))
               (root (plist-get world :root)))
          (setq cleanup-errors
                (org-ref362-test-attempt
                 'delete-root
                 (lambda ()
                   (unless (and (file-name-absolute-p root)
                                (file-directory-p root)
                                (file-in-directory-p root owner)
                                (not (equal (directory-file-name root)
                                            (directory-file-name owner))))
                     (error "Unsafe Org Ref cleanup root: %S" root))
                   (delete-directory root t))
                 cleanup-errors))))
      (setq cleanup-errors (nreverse cleanup-errors))
      (setq cleanup-state
            (list
             :new-buffers
             (mapcar #'buffer-name
                     (seq-remove (lambda (buffer)
                                   (or (memq buffer buffers-before)
                                       (not (buffer-live-p buffer))))
                                 (buffer-list)))
             :new-processes
             (seq-remove (lambda (process) (memq process processes-before))
                         (process-list))
             :new-timers
             (seq-remove (lambda (timer) (memq timer timers-before)) timer-list)
             :new-idle-timers
             (seq-remove (lambda (timer) (memq timer idle-timers-before))
                         timer-idle-list)
             :external-events (copy-tree org-ref362-test-external-events)
             :external-advices
             (seq-filter
              (lambda (entry)
                (advice-member-p (cdr entry) (car entry)))
              org-ref362-test-external-advices)
             :windows (org-ref362-test-windows-restored-p windows-before)
             :configuration (compare-window-configurations
                             (current-window-configuration)
                             configuration-before)
             :buffer (eq (current-buffer) buffer-before)
             :window (eq (selected-window) window-before)
             :variables
             (cl-every
              (lambda (entry)
                (org-ref362-test-variable-restored-p
                 (car entry) (cdr entry)))
              states-before)
             :key (equal (lookup-key org-mode-map (kbd "C-c ]"))
                         binding-before)
             :warnings
             (org-ref362-test-buffer-content-restored-p warnings-before)
             :messages
             (org-ref362-test-buffer-content-restored-p messages-before)
             :report
             (if report-baseline
                 (and (buffer-live-p report-baseline)
                      (equal (buffer-name report-baseline) "*org-ref*"))
               (null (get-buffer "*org-ref*")))
             :export
             (if export-baseline
                 (and (buffer-live-p export-baseline)
                      (equal (buffer-name export-baseline)
                             "*org-ref ORG Export*"))
               (null (get-buffer "*org-ref ORG Export*")))
             :root (and world
                        (not (file-exists-p (plist-get world :root))))
             :body-error body-error :cleanup-errors cleanup-errors)))
    (unless (and (null (plist-get cleanup-state :new-buffers))
                 (null (plist-get cleanup-state :new-processes))
                 (null (plist-get cleanup-state :new-timers))
                 (null (plist-get cleanup-state :new-idle-timers))
                 (null (plist-get cleanup-state :external-events))
                 (null (plist-get cleanup-state :external-advices))
                 (plist-get cleanup-state :windows)
                 (plist-get cleanup-state :configuration)
                 (plist-get cleanup-state :buffer)
                 (plist-get cleanup-state :window)
                 (plist-get cleanup-state :variables)
                 (plist-get cleanup-state :key)
                 (plist-get cleanup-state :warnings)
                 (plist-get cleanup-state :messages)
                 (plist-get cleanup-state :report)
                 (plist-get cleanup-state :export)
                 (plist-get cleanup-state :root)
                 (null body-error) (null cleanup-errors))
      (error "Org Ref workflow/cleanup failure: %S" cleanup-state))
    (list :result body-value :cleanup 'clean)))
"####;

fn org_ref_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ORG_REF_MELPA_PIN, "org-ref.el")
        .expect("prepare exact shallow Org Ref and recursive dependency sources below ./tmp")
        .with_prelude(ORG_REF_TEST_PRELUDE)
        .with_timeout(ORG_REF_TEST_TIMEOUT)
}

#[test]
fn org_ref_package_batch() {
    assert_oracle_batch_cases(
        org_ref_oracle(),
        "org-ref-package-batch",
        "Org Ref",
        &workflows::org_ref_batch_cases(),
    );
}
