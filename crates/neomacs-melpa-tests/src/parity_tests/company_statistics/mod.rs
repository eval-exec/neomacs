use crate::{COMPANY_STATISTICS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const COMPANY_STATISTICS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'company)
(require 'company-statistics)

;; Establish GNU's reserved menu-bar row and the compiled C mode dependency
;; before any case captures its mutable editor baseline.
(set-window-configuration (current-window-configuration))
(let ((outer-suffixes load-suffixes))
  (let ((load-suffixes '(".elc" ".el")))
    (require 'cc-mode))
  (unless (equal load-suffixes outer-suffixes)
    (error "Company Statistics leaked dependency load suffixes: %S" load-suffixes)))

(defconst company-statistics361-test-candidate-names
  '("cache-alpha" "cache-beta" "cache-界"))

(defconst company-statistics361-test-state-symbols
  '(company-statistics-mode company-statistics-size company-statistics-file
    company-statistics-auto-save company-statistics-auto-restore
    company-statistics-capture-context company-statistics-score-change
    company-statistics-score-calc company-statistics--scores
    company-statistics--log company-statistics--index
    company-statistics--context company-transformers
    company-completion-started-hook company-completion-finished-hook
    company-completion-cancelled-hook company-after-completion-hook
    company-timer company-tooltip-timer company-echo-timer company--cache
    company--disabled-backends
    kill-emacs-hook enable-local-variables enable-dir-local-variables
    unread-command-events executing-kbd-macro this-command real-this-command
    last-command real-last-command last-command-event last-input-event
    current-prefix-arg prefix-arg deactivate-mark
    emulation-mode-map-alists)
  "Global editor and package state restored after each shared workflow.")

(defvar company-statistics361-test-owned-buffers nil)
(defvar company-statistics361-test-current-world nil)
(defvar company-statistics361-test-events nil)
(defvar company-statistics361-test-backend-events nil)
(defvar company-statistics361-test-backend-calls nil)

(defun company-statistics361-test-variable-state (symbol)
  "Return SYMBOL's exact boundness and value identity."
  (if (boundp symbol)
      (list :bound t :value (symbol-value symbol))
    '(:bound nil)))

(defun company-statistics361-test-restore-variable (symbol state)
  "Restore SYMBOL to exact STATE."
  (if (plist-get state :bound)
      (set symbol (plist-get state :value))
    (makunbound symbol)))

(defun company-statistics361-test-variable-restored-p (symbol state)
  "Return non-nil when SYMBOL has the exact STATE identity."
  (if (plist-get state :bound)
      (and (boundp symbol) (eq (symbol-value symbol) (plist-get state :value)))
    (not (boundp symbol))))

(defun company-statistics361-test-cache-state ()
  "Return process-global Company cache contents in stable key order."
  (let (rows)
    (maphash (lambda (key value)
               (push (cons (copy-tree key) (copy-tree value)) rows))
             company--cache)
    (sort rows
          (lambda (left right)
            (string< (prin1-to-string (car left))
                     (prin1-to-string (car right)))))))

(defun company-statistics361-test-window-state ()
  "Return stable ownership state for all ordinary windows."
  (mapcar
   (lambda (window)
     (list :window window :buffer (window-buffer window)
           :edges (window-edges window) :point (window-point window)
           :start (window-start window) :hscroll (window-hscroll window)
           :vscroll (window-vscroll window t)
           :prev (copy-tree (window-prev-buffers window))
           :next (copy-tree (window-next-buffers window))))
   (window-list nil 'no-minibuf)))

(defun company-statistics361-test-buffer-content-state (name)
  "Return exact mutable content state for existing buffer NAME."
  (let ((buffer (get-buffer name)))
    (when buffer
      (with-current-buffer buffer
        (list :buffer buffer :text (buffer-string) :point (point)
              :modified (buffer-modified-p) :undo buffer-undo-list
              :read-only buffer-read-only)))))

(defun company-statistics361-test-restore-buffer-content (state)
  "Restore an existing buffer to exact content STATE."
  (when state
    (let ((buffer (plist-get state :buffer)))
      (unless (buffer-live-p buffer)
        (error "Company Statistics baseline log buffer died: %S" buffer))
      (with-current-buffer buffer
        (let ((inhibit-read-only t))
          (widen)
          (erase-buffer)
          (insert (plist-get state :text)))
        (goto-char (min (plist-get state :point) (point-max)))
        (setq buffer-undo-list (plist-get state :undo)
              buffer-read-only (plist-get state :read-only))
        (set-buffer-modified-p (plist-get state :modified))))))

(defun company-statistics361-test-buffer-content-restored-p (state)
  "Return non-nil when existing buffer STATE is exactly restored."
  (or (null state)
      (let ((buffer (plist-get state :buffer)))
        (and (buffer-live-p buffer)
             (with-current-buffer buffer
               (and (equal (buffer-string) (plist-get state :text))
                    (= (point) (plist-get state :point))
                    (eq (buffer-modified-p) (plist-get state :modified))
                    (eq buffer-undo-list (plist-get state :undo))
                    (eq buffer-read-only (plist-get state :read-only))))))))

(defun company-statistics361-test-restore-windows (configuration state)
  "Restore window CONFIGURATION and exact semantic STATE."
  (set-window-configuration configuration)
  (dolist (entry state)
    (let ((window (plist-get entry :window)))
      (unless (window-live-p window)
        (error "Company Statistics baseline window died: %S" window))
      (set-window-prev-buffers window (copy-tree (plist-get entry :prev)))
      (set-window-next-buffers window (copy-tree (plist-get entry :next)))
      (set-window-point window (plist-get entry :point))
      (set-window-start window (plist-get entry :start) 'noforce)
      (set-window-hscroll window (plist-get entry :hscroll))
      (set-window-vscroll window (plist-get entry :vscroll) t))))

(defun company-statistics361-test-allocate-world (case-name)
  "Return an unmaterialized owned world for CASE-NAME."
  (let ((raw-owner (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
    (unless (and raw-owner (not (string-empty-p raw-owner))
                 (file-name-absolute-p raw-owner)
                 (file-directory-p raw-owner))
      (error "Company Statistics sandbox root is unsafe: %S" raw-owner))
    (let* ((owner (file-name-as-directory (file-truename raw-owner)))
           (root (expand-file-name
                  (format "company-statistics361-%s/" case-name) owner)))
      (unless (and (file-name-absolute-p root)
                   (not (equal owner root))
                   (string-prefix-p owner root)
                   (not (file-exists-p root)))
        (error "Company Statistics refuses owned root: %S" (list owner root)))
      (list :owner owner :root root
            :history (expand-file-name "state/history.el" root)))))

(defun company-statistics361-test-materialize-world (world)
  "Create the already-owned WORLD and its ordinary directories."
  (let ((root (plist-get world :root)))
    (make-directory root)
    (make-directory (file-name-directory (plist-get world :history)) t)
    (unless (file-directory-p root)
      (error "Company Statistics world was not created: %S" root))))

(defun company-statistics361-test-path (relative)
  "Return RELATIVE below the current owned world."
  (unless company-statistics361-test-current-world
    (error "Company Statistics has no active owned world"))
  (expand-file-name relative
                    (plist-get company-statistics361-test-current-world :root)))

(defun company-statistics361-test-normalize-string (string)
  "Replace the exact current owned root in STRING."
  (if (not (stringp string)) string
    (let ((root (and company-statistics361-test-current-world
                     (plist-get company-statistics361-test-current-world
                                :root))))
      (if root
          (replace-regexp-in-string (regexp-quote root) "[ROOT]/" string t t)
        string))))

(defun company-statistics361-test-write (relative text)
  "Write UTF-8 TEXT to the owned RELATIVE file."
  (let ((path (company-statistics361-test-path relative))
        (coding-system-for-write 'utf-8-unix))
    (make-directory (file-name-directory path) t)
    (with-temp-file path (insert text))
    path))

(defun company-statistics361-test-own-buffer (buffer)
  "Register BUFFER as case-owned and return it."
  (unless (buffer-live-p buffer)
    (error "Company Statistics cannot own dead buffer: %S" buffer))
  (cl-pushnew buffer company-statistics361-test-owned-buffers :test #'eq)
  buffer)

(defun company-statistics361-test-backend (command &optional argument &rest _)
  "Provide deterministic property-bearing candidates to real Company."
  (push (list command
              (if (stringp argument)
                  (substring-no-properties argument)
                argument))
        company-statistics361-test-backend-calls)
  (pcase command
    ('prefix
     (let ((end (point))
           (start (save-excursion
                    (skip-chars-backward "[:word:]-")
                    (point))))
       (buffer-substring-no-properties start end)))
    ('candidates
     (mapcar
      (lambda (entry)
        (let ((candidate (copy-sequence (car entry))))
          (put-text-property 0 (length candidate)
                             'company-statistics361-source-index
                             (cdr entry) candidate)
          candidate))
      (seq-filter
       (lambda (entry) (string-prefix-p argument (car entry)))
       (cl-mapcar #'cons company-statistics361-test-candidate-names '(0 1 2)))))
    ('sorted t)
    ('duplicates nil)
    ('ignore-case nil)
    ('post-completion
     (push (list :post (substring-no-properties argument)
                 :properties
                 (and (> (length argument) 0)
                      (copy-tree (text-properties-at 0 argument))))
           company-statistics361-test-backend-events))))

(defun company-statistics361-test-plain-candidates ()
  "Return active Company candidate text without display properties."
  (mapcar #'substring-no-properties company-candidates))

(defun company-statistics361-test-normalize-context-entry (entry)
  "Normalize one subject context ENTRY without changing its meaning."
  (if (and (consp entry) (eq (car entry) :file))
      (list :file (company-statistics361-test-normalize-string (cadr entry)))
    (copy-tree entry)))

(defun company-statistics361-test-context ()
  "Return the exact current heavy context with owned roots normalized."
  (mapcar #'company-statistics361-test-normalize-context-entry
          company-statistics--context))

(defun company-statistics361-test-context-key (key)
  "Normalize score context KEY."
  (cond ((null key) :global)
        ((and (consp key) (eq (car key) :file))
         (company-statistics361-test-normalize-context-entry key))
        (t (copy-tree key))))

(defun company-statistics361-test-updates (updates)
  "Return exact UPDATES with semantic context keys."
  (mapcar (lambda (entry)
            (cons (company-statistics361-test-context-key (car entry))
                  (cdr entry)))
          updates))

(defun company-statistics361-test-scores ()
  "Return score hash contents in stable candidate order."
  (let (rows)
    (when (hash-table-p company-statistics--scores)
      (maphash
       (lambda (candidate updates)
         (push (list (substring-no-properties candidate)
                     :key-properties
                     (and (> (length candidate) 0)
                          (text-properties-at 0 candidate))
                     :updates (company-statistics361-test-updates updates))
               rows))
       company-statistics--scores))
    (sort rows (lambda (left right) (string< (car left) (car right))))))

(defun company-statistics361-test-log ()
  "Return the exact ring vector with normalized entries."
  (and (vectorp company-statistics--log)
       (apply
        #'vector
        (mapcar
         (lambda (entry)
           (and entry
                (cons (substring-no-properties (car entry))
                      (company-statistics361-test-updates (cdr entry)))))
         (append company-statistics--log nil)))))

(defun company-statistics361-test-ledger (&optional alias-candidate alias-slot)
  "Return canonical store state and optional cons alias observation."
  (list :size company-statistics-size
        :scores (company-statistics361-test-scores)
        :log (company-statistics361-test-log)
        :index company-statistics--index
        :alias
        (and alias-candidate alias-slot
             (let ((scores (gethash alias-candidate
                                    company-statistics--scores))
                   (entry (and (vectorp company-statistics--log)
                               (aref company-statistics--log alias-slot))))
               (and entry
                    (list :global
                          (eq (assoc nil scores) (assoc nil (cdr entry)))
                          :mode
                          (eq (assoc major-mode scores)
                              (assoc major-mode (cdr entry)))))))))

(defun company-statistics361-test-observe-start (manual)
  "Observe one real Company start after the subject captured context."
  (push (list :started :manual (and manual t)
              :prefix company-prefix
              :candidates (company-statistics361-test-plain-candidates)
              :selection company-selection
              :subject-active
              (and (memq 'company-statistics--start
                         company-completion-started-hook) t)
              :context
              (and (memq 'company-statistics--start
                         company-completion-started-hook)
                   (company-statistics361-test-context)))
        company-statistics361-test-events))

(defun company-statistics361-test-observe-finish (result)
  "Observe one real Company finish after the subject updated history."
  (push (list :finished (substring-no-properties result)
              :result-properties
              (and (> (length result) 0)
                   (copy-tree (text-properties-at 0 result)))
              :index company-statistics--index)
        company-statistics361-test-events))

(defun company-statistics361-test-command-loop (keys)
  "Execute one contiguous Company KEYS macro with bounded loop state."
  (when unread-command-events
    (error "Company Statistics began with unread events: %S"
           unread-command-events))
  (execute-kbd-macro keys)
  (when unread-command-events
    (error "Company Statistics left unread events: %S"
           unread-command-events))
  (when (active-minibuffer-window)
    (error "Company Statistics left an active minibuffer")))

(defun company-statistics361-test-session-buffer (request)
  "Create or rearm the real Company buffer described by REQUEST."
  (let* ((relative (plist-get request :file))
         (path (and relative (company-statistics361-test-path relative)))
         (buffer
          (if path
              (progn
                (unless (file-exists-p path)
                  (company-statistics361-test-write relative ""))
                (let ((create-lockfiles nil)
                      (enable-local-variables nil)
                      (enable-dir-local-variables nil))
                  (find-file-noselect path)))
            (or (get-buffer " *company-statistics361-neutral*")
                (generate-new-buffer " *company-statistics361-neutral*")))))
    (company-statistics361-test-own-buffer buffer)
    (switch-to-buffer buffer)
    (setq-local create-lockfiles nil)
    (when company-candidates (company-abort))
    (let ((inhibit-read-only t)) (erase-buffer))
    (let ((mode (or (plist-get request :mode) #'fundamental-mode)))
      (unless (eq major-mode mode) (funcall mode)))
    (let ((keyword (plist-get request :keyword))
          (parent (plist-get request :parent))
          keyword-start)
      ;; Elisp keywords are fontified only in form position.  The opening
      ;; parenthesis is real fixture syntax; C and other modes retain their
      ;; ordinary top-level spelling.
      (when (and keyword (derived-mode-p 'emacs-lisp-mode))
        (insert "("))
      (setq keyword-start (point))
      (cond ((and keyword parent) (insert keyword " " parent ".ca"))
            (parent (insert parent ".ca"))
            (t (insert "ca")))
      ;; Company Statistics reads the real `face' spans installed by the
      ;; major mode's font-lock engine.  Batch buffers do not inherit the
      ;; interactive global font-lock toggle, so enable that public minor
      ;; mode before forcing the complete owned buffer to be fontified.
      (font-lock-mode 1)
      (font-lock-flush (point-min) (point-max))
      (font-lock-ensure (point-min) (point-max))
      (font-lock-fontify-region (point-min) (point-max))
      (when (plist-get request :keyword-face)
        (unless (eq (get-text-property keyword-start 'face)
                    'font-lock-keyword-face)
          (error "Company Statistics keyword is not really fontified: %S"
                 (list major-mode keyword
                       (get-text-property keyword-start 'face))))))
    (setq-local company-backends '(company-statistics361-test-backend)
                company-frontends nil
                company-idle-delay nil
                company-abort-on-unique-match nil)
    (unless company-mode (company-mode 1))
    (local-set-key (kbd "C-c c") #'company-complete)
    (goto-char (point-max))
    (set-buffer-modified-p nil)
    (setq buffer-undo-list nil)
    buffer))

(defun company-statistics361-test-session (request)
  "Run one real Company command-loop session described by REQUEST."
  (setq company-statistics361-test-events nil
        company-statistics361-test-backend-events nil
        company-statistics361-test-backend-calls nil)
  (let* ((buffer (company-statistics361-test-session-buffer request))
         (setup-calls (nreverse company-statistics361-test-backend-calls))
         (before (buffer-substring-no-properties (point-min) (point-max)))
         (steps (or (plist-get request :steps) 0))
         (finish (plist-get request :finish))
         (keys (vconcat (kbd "C-c c")
                        (apply #'vconcat
                               (make-list steps (kbd "C-n")))
                        (kbd (if finish "RET" "C-g")))))
    (setq company-statistics361-test-backend-calls nil)
    (company-statistics361-test-command-loop keys)
    ;; Keep the real modified buffer while releasing only its owned file lock;
    ;; package persistence assertions must not mistake editor lockfiles for
    ;; Company Statistics output.
    (when (buffer-modified-p) (unlock-buffer))
    (list :before before :keys (key-description keys)
          :setup-calls setup-calls
          :events (nreverse company-statistics361-test-events)
          :calls (nreverse company-statistics361-test-backend-calls)
          :backend (nreverse company-statistics361-test-backend-events)
          :after (buffer-substring-no-properties (point-min) (point-max))
          :point (point) :modified (buffer-modified-p)
          :active (and company-candidates t)
          :tooltip (and (company-tooltip-visible-p) t)
          :runtime
          (list :timer company-timer :tooltip-timer company-tooltip-timer
                :echo-timer company-echo-timer
                :cache-count (hash-table-count company--cache))
          :unread unread-command-events)))

(defun company-statistics361-test-hook-count (function hook)
  "Count FUNCTION in HOOK by identity."
  (cl-count function (default-value hook) :test #'eq))

(defun company-statistics361-test-mode-state ()
  "Return the exact public integration state."
  (list :mode company-statistics-mode
        :transformers
        (cl-count 'company-sort-by-statistics company-transformers :test #'eq)
        :transformer-last (eq (car (last company-transformers))
                              'company-sort-by-statistics)
        :started
        (company-statistics361-test-hook-count
         'company-statistics--start 'company-completion-started-hook)
        :finished
        (company-statistics361-test-hook-count
         'company-statistics--finished 'company-completion-finished-hook)))

(defun company-statistics361-test-condition (thunk)
  "Return THUNK's value or exact normalized nonlocal condition."
  (condition-case condition
      (list :value (funcall thunk))
    (t
     (list :signal (car condition)
           :data
           (mapcar (lambda (item)
                     (cond ((bufferp item) :killed-buffer)
                           ((stringp item)
                            (company-statistics361-test-normalize-string item))
                           (t item)))
                   (cdr condition))
           :message
           (company-statistics361-test-normalize-string
            (error-message-string condition))))))

(defun company-statistics361-test-read-history ()
  "Return exact semantic and byte observations for the owned history file."
  (let ((path company-statistics-file))
    (when (file-regular-p path)
      (with-temp-buffer
        (set-buffer-multibyte nil)
        (insert-file-contents-literally path)
        (let ((bytes (buffer-string)))
          (list :bytes (string-bytes bytes)
                :sha256 (secure-hash 'sha256 bytes)
                :text (decode-coding-string bytes 'utf-8)))))))

(defun company-statistics361-test-exit-save ()
  "Dispatch only the exact registered package exit callback."
  (let ((callback #'company-statistics--maybe-save))
    (unless (and (= (company-statistics361-test-hook-count
                     callback 'kill-emacs-hook) 1)
                 (string-suffix-p
                  "/company-statistics.el"
                  (symbol-file callback 'defun)))
      (error "Company Statistics exit callback is not exact: %S"
             (list kill-emacs-hook (symbol-file callback 'defun))))
    (let ((kill-emacs-hook (list callback)))
      (run-hooks 'kill-emacs-hook))))

(defun company-statistics361-test-warning-delta (before)
  "Return normalized new warning text after BEFORE."
  (let ((buffer (get-buffer "*Warnings*")))
    (when buffer
      (company-statistics361-test-own-buffer buffer)
      (with-current-buffer buffer
        (company-statistics361-test-normalize-string
         (buffer-substring-no-properties
          (min before (point-max)) (point-max)))))))

(defun company-statistics361-test-tree ()
  "Return the exact relative tree below the current owned root."
  (let ((root (plist-get company-statistics361-test-current-world :root))
        rows)
    (cl-labels
        ((walk (directory)
           (dolist (path (directory-files
                          directory t directory-files-no-dot-files-regexp))
             (let ((relative (file-relative-name path root)))
               (if (file-directory-p path)
                   (progn (push (concat relative "/") rows) (walk path))
                 (push relative rows))))))
      (walk root))
    (sort rows #'string<)))

(defun company-statistics361-test-configure
    (size profile auto-save auto-restore history)
  "Configure and publicly enable an owned SIZE/PROFILE session."
  (setq company-statistics-size size
        company-statistics-file history
        company-statistics-auto-save auto-save
        company-statistics-auto-restore auto-restore
        company-statistics-capture-context
        #'company-statistics-capture-context-heavy
        company-statistics-score-change
        (if (eq profile 'light)
            #'company-statistics-score-change-light
          #'company-statistics-score-change-heavy)
        company-statistics-score-calc
        (if (eq profile 'light)
            #'company-statistics-score-calc-light
          #'company-statistics-score-calc-heavy))
  (let ((value (company-statistics-mode 1)))
    (add-hook 'company-completion-started-hook
              #'company-statistics361-test-observe-start t)
    (add-hook 'company-completion-finished-hook
              #'company-statistics361-test-observe-finish t)
    value))

(defun company-statistics361-test-fixture-reset-store ()
  "Clear only the in-memory store to model a new editor session."
  (setq company-statistics--scores nil
        company-statistics--log nil
        company-statistics--index nil
        company-statistics--context nil))

(defun company-statistics361-test-run (case-name thunk)
  "Run THUNK in one owned and fully reversible CASE-NAME world."
  (unless (string-match-p "\\`[a-z0-9-]+\\'" case-name)
    (error "Company Statistics invalid case name: %S" case-name))
  (let ((statistics-source (symbol-file 'company-statistics-mode 'defun))
        (company-source (symbol-file 'company-complete 'defun))
        (c-source (symbol-file 'c-mode 'defun)))
    (unless (and (featurep 'company-statistics) (featurep 'company)
                 statistics-source company-source c-source
                 (string-suffix-p "/company-statistics.el" statistics-source)
                 (string-suffix-p "/company.el" company-source)
                 (string-suffix-p ".elc" c-source)
                 (package-built-in-p 'cl-lib '(0 4))
                 (equal load-suffixes '(".el")))
      (error "Company Statistics activation boundary failed: %S"
             (list statistics-source company-source c-source
                   (featurep 'company-statistics) (featurep 'company)
                   (package-built-in-p 'cl-lib '(0 4)) load-suffixes))))
  (unless (and (null company-timer) (null company-tooltip-timer)
               (null company-echo-timer) (hash-table-p company--cache))
    (error "Company Statistics Company runtime baseline is unsafe: %S"
           (list company-timer company-tooltip-timer company-echo-timer
                 company--cache)))
  (let* ((buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (idle-timers-before (copy-sequence timer-idle-list))
         (current-buffer-before (current-buffer))
         (selected-window-before (selected-window))
         (configuration-before (current-window-configuration))
         (windows-before (company-statistics361-test-window-state))
         (warnings-before
          (company-statistics361-test-buffer-content-state "*Warnings*"))
         (messages-before
          (company-statistics361-test-buffer-content-state "*Messages*"))
         (states-before
          (mapcar (lambda (symbol)
                    (cons symbol
                          (company-statistics361-test-variable-state symbol)))
                  company-statistics361-test-state-symbols))
         (cache-value-before (company-statistics361-test-cache-state))
         (backend-plist-before
          (copy-tree (symbol-plist 'company-statistics361-test-backend)))
         (size-plist-before (copy-tree (symbol-plist 'company-statistics-size)))
         (company-statistics361-test-owned-buffers nil)
         (company-statistics361-test-events nil)
         (company-statistics361-test-backend-events nil)
         (company-statistics361-test-backend-calls nil)
         company-statistics361-test-current-world
         body-value body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (setq company-statistics361-test-current-world
                    (company-statistics361-test-allocate-world case-name))
              (company-statistics361-test-materialize-world
               company-statistics361-test-current-world)
              ;; Fork all mutable registration lists before the public mode
              ;; touches them, and start each case from an owned empty store.
              (setq company-transformers (copy-sequence company-transformers)
                    company-completion-started-hook
                    (copy-sequence company-completion-started-hook)
                    company-completion-finished-hook
                    (copy-sequence company-completion-finished-hook)
                    company-completion-cancelled-hook
                    (copy-sequence company-completion-cancelled-hook)
                    company-after-completion-hook
                    (copy-sequence company-after-completion-hook)
                    company--cache (copy-hash-table company--cache)
                    company--disabled-backends
                    (copy-sequence company--disabled-backends)
                    kill-emacs-hook (copy-sequence kill-emacs-hook)
                    company-statistics-mode nil)
              (company-statistics361-test-fixture-reset-store)
              (setq body-value
                    ;; The outer oracle uses circular printing only when it
                    ;; serializes the completed observation.  Real users run
                    ;; the package callback under GNU's ordinary printer
                    ;; defaults; otherwise `company-statistics--save' writes
                    ;; transport-induced #n= aliases into its cache.
                    (let ((print-circle nil)
                          (print-escape-newlines nil)
                          (print-escape-control-characters nil))
                      (funcall thunk
                               company-statistics361-test-current-world))))
          (t (setq body-error condition)))
      (cl-labels
          ((attempt (phase function)
             (condition-case condition
                 (funcall function)
               (t (push (list phase condition) cleanup-errors))))
           (sweep (number)
             (dolist (timer
                      (delete-dups
                       (append (seq-difference timer-list timers-before #'eq)
                               (seq-difference timer-idle-list
                                               idle-timers-before #'eq))))
               (attempt (list 'timer number) (lambda () (cancel-timer timer))))
             (dolist (process
                      (seq-difference (process-list) processes-before #'eq))
               (attempt
                (list 'process number)
                (lambda ()
                  (set-process-query-on-exit-flag process nil)
                  (when (process-live-p process) (delete-process process))
                  (when (process-live-p process)
                    (error "Company Statistics process survived: %S" process)))))
             (dolist (buffer
                      (seq-difference (buffer-list) buffers-before #'eq))
               (attempt
                (list 'buffer number (buffer-name buffer))
                (lambda ()
                  (when (buffer-live-p buffer)
                    (with-current-buffer buffer
                      (when company-candidates (company-abort))
                      (set-buffer-modified-p nil))
                    (kill-buffer buffer)))))))
        (dolist (buffer (copy-sequence company-statistics361-test-owned-buffers))
          (when (buffer-live-p buffer)
            (attempt
             (list 'abort (buffer-name buffer))
             (lambda ()
               (with-current-buffer buffer
                 (when company-candidates (company-abort)))))))
        (attempt 'disable-mode
                 (lambda ()
                   (when (bound-and-true-p company-statistics-mode)
                     (company-statistics-mode -1))))
        (attempt 'window-first
                 (lambda ()
                   (company-statistics361-test-restore-windows
                    configuration-before windows-before)))
        (dotimes (number 2) (sweep number))
        (dolist (entry states-before)
          (attempt
           (list 'variable (car entry))
           (lambda ()
             (company-statistics361-test-restore-variable
              (car entry) (cdr entry)))))
        (attempt 'custom-plist
                 (lambda ()
                   (setplist 'company-statistics-size
                             (copy-tree size-plist-before))))
        (attempt 'backend-plist
                 (lambda ()
                   (setplist 'company-statistics361-test-backend
                             (copy-tree backend-plist-before))))
        (attempt 'warnings
                 (lambda ()
                   (company-statistics361-test-restore-buffer-content
                    warnings-before)))
        (attempt 'messages
                 (lambda ()
                   (company-statistics361-test-restore-buffer-content
                    messages-before)))
        (attempt 'window-final
                 (lambda ()
                   (company-statistics361-test-restore-windows
                    configuration-before windows-before)))
        (attempt
         'select-baseline
         (lambda ()
           (unless (and (buffer-live-p current-buffer-before)
                        (window-live-p selected-window-before))
             (error "Company Statistics baseline selection died"))
           (select-window selected-window-before)
           (set-buffer current-buffer-before)))
        (when company-statistics361-test-current-world
          (attempt
           'delete-root
           (lambda ()
             (let* ((root
                     (plist-get company-statistics361-test-current-world :root))
                    (owner
                     (plist-get company-statistics361-test-current-world :owner))
                    (true-root
                     (and (file-exists-p root)
                          (file-name-as-directory (file-truename root)))))
               (when true-root
                 (unless (and (file-name-absolute-p root)
                              (not (equal true-root owner))
                              (string-prefix-p owner true-root))
                   (error "Company Statistics refuses root deletion: %S"
                          (list owner root)))
                 (delete-directory root t)))))
        ;; Unicode path deletion can create GNU's internal coding buffer.
        (sweep 'after-root))))
    (setq cleanup-errors (nreverse cleanup-errors))
    (let ((cleanup-state
           (list
            :new-buffers (seq-difference (buffer-list) buffers-before #'eq)
            :new-processes (seq-difference (process-list) processes-before #'eq)
            :new-timers
            (delete-dups
             (append (seq-difference timer-list timers-before #'eq)
                     (seq-difference timer-idle-list idle-timers-before #'eq)))
            :windows
            (equal (company-statistics361-test-window-state) windows-before)
            :configuration
            (compare-window-configurations
             (current-window-configuration) configuration-before)
            :buffer (eq (current-buffer) current-buffer-before)
            :window (eq (selected-window) selected-window-before)
            :variables
            (cl-every
             (lambda (entry)
               (company-statistics361-test-variable-restored-p
                (car entry) (cdr entry)))
             states-before)
            :cache-content
            (equal (company-statistics361-test-cache-state) cache-value-before)
            :custom-plist
            (equal (symbol-plist 'company-statistics-size) size-plist-before)
            :backend-plist
            (equal (symbol-plist 'company-statistics361-test-backend)
                   backend-plist-before)
            :warnings
            (company-statistics361-test-buffer-content-restored-p
             warnings-before)
            :messages
            (company-statistics361-test-buffer-content-restored-p
             messages-before)
            :root
            (and company-statistics361-test-current-world
                 (not (file-exists-p
                       (plist-get company-statistics361-test-current-world
                                  :root))))
            :body-error body-error :cleanup-errors cleanup-errors)))
      (unless (and (null (plist-get cleanup-state :new-buffers))
                   (null (plist-get cleanup-state :new-processes))
                   (null (plist-get cleanup-state :new-timers))
                   (plist-get cleanup-state :windows)
                   (plist-get cleanup-state :configuration)
                   (plist-get cleanup-state :buffer)
                   (plist-get cleanup-state :window)
                   (plist-get cleanup-state :variables)
                   (plist-get cleanup-state :cache-content)
                   (plist-get cleanup-state :custom-plist)
                   (plist-get cleanup-state :backend-plist)
                   (plist-get cleanup-state :warnings)
                   (plist-get cleanup-state :messages)
                   (plist-get cleanup-state :root)
                   (null body-error) (null cleanup-errors))
        (error "Company Statistics workflow/cleanup failure: %S"
               cleanup-state))
      (list :result body-value :cleanup 'clean))))
"####;

fn company_statistics_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COMPANY_STATISTICS_MELPA_PIN, "company-statistics.el")
        .expect("prepare exact shallow Company Statistics and Company sources below ./tmp")
        .with_prelude(COMPANY_STATISTICS_TEST_PRELUDE)
}

#[test]
fn company_statistics_package_batch() {
    assert_oracle_batch_cases(
        company_statistics_oracle(),
        "company-statistics-package-batch",
        "Company Statistics",
        &workflows::company_statistics_batch_cases(),
    );
}
