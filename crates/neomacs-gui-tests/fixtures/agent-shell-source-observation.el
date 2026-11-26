;;; agent-shell-source-observation.el --- MELPA #832 regression -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'map)
(require 'json)
(require 'subr-x)

(defvar dispgh832-source-root
  (file-name-directory (or load-file-name buffer-file-name)))
(defvar dispgh832-root (getenv "NEOMACS_832_ROOT"))
(defvar dispgh832-package-root
  (or (getenv "NEOMACS_832_PACKAGE_ROOT")
      (expand-file-name "upstream/" dispgh832-source-root)))
(defvar dispgh832-buffer nil)
(defvar dispgh832-timers nil)
(defvar dispgh832-processes nil)
(defvar dispgh832-finishing nil)
(defvar dispgh832-opening nil)
(defvar dispgh832-samples nil)

(unless (and dispgh832-root (file-name-absolute-p dispgh832-root))
  (error "Exec must provide absolute owned NEOMACS_832_ROOT"))
(setq dispgh832-root (file-name-as-directory dispgh832-root))
(make-directory dispgh832-root t)
(dolist (name '("HOME" "XDG_CACHE_HOME" "TMPDIR"))
  (unless (and (getenv name)
               (file-in-directory-p (getenv name) dispgh832-root))
    (error "%s must be exec-pinned inside NEOMACS_832_ROOT" name)))
(setq user-emacs-directory (expand-file-name "emacs/" dispgh832-root)
      default-directory dispgh832-root)
(make-directory user-emacs-directory t)

(defun dispgh832-log (event &rest fields)
  "Append a bounded observation without forcing window layout."
  (with-temp-buffer
    (let ((print-length 80) (print-level 7))
      (prin1 (append (list :event event :time (float-time)) fields) (current-buffer)))
    (insert "\n")
    (write-region (point-min) (point-max)
                  (expand-file-name "receipt.el" dispgh832-root) t 'silent)))

(defun dispgh832-owned-processes ()
  "Remember only this shell's dummy and ACP process objects."
  (when (buffer-live-p dispgh832-buffer)
    (with-current-buffer dispgh832-buffer
      (dolist (process (list (get-buffer-process (current-buffer))
                            (map-elt (map-elt agent-shell--state :client) :process)))
        (when (processp process)
          (cl-pushnew process dispgh832-processes :test #'eq))))))

(defun dispgh832-observe-process-start (original &rest arguments)
  "Remember processes actually started by this opener, without replacing them."
  (let ((process (apply original arguments)))
    (when (and dispgh832-opening (processp process))
      (cl-pushnew process dispgh832-processes :test #'eq))
    process))

(defun dispgh832-adopt-partial-shell ()
  "Recover this config's shell if package startup signaled before returning."
  (unless (buffer-live-p dispgh832-buffer)
    (dolist (buffer (buffer-list))
      (with-current-buffer buffer
        (when (and (derived-mode-p 'agent-shell-mode)
                   (boundp 'agent-shell--state)
                   (eq (map-nested-elt agent-shell--state '(:agent-config :identifier))
                       'dispgh832-local))
          (setq dispgh832-buffer buffer))))))

(defun dispgh832-sample (label)
  (dispgh832-owned-processes)
  (when (buffer-live-p dispgh832-buffer)
    (with-current-buffer dispgh832-buffer
      (let ((window (get-buffer-window (current-buffer) t))
            (text (buffer-substring-no-properties (point-min) (point-max))))
        (push `((label . ,(format "%s" label))
                (initialized . ,(and (map-elt agent-shell--state :init-finished) t))
                (session . ,(map-nested-elt agent-shell--state '(:session :id)))
                (buffer . ,(buffer-name))
                (window . ,(and window (prin1-to-string window)))
                (body_seen . ,(and (string-match-p "Local resumed session" text) t))
                (body_hash . ,(secure-hash 'sha256 text))
                (end . ,(and window (window-end window nil))))
              dispgh832-samples)))))

(defun dispgh832-finish (&optional exit-code)
  (unless dispgh832-finishing
    (setq dispgh832-finishing t)
    (dispgh832-adopt-partial-shell)
    (dispgh832-sample 'before-cleanup)
    (when (and (eq window-system 'x)
               (fboundp 'x-export-frames)
               (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG"))
      (let ((coding-system-for-write 'no-conversion))
        (with-temp-file (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")
          (set-buffer-multibyte nil)
          (insert (x-export-frames nil 'png)))))
    (mapc #'cancel-timer dispgh832-timers)
    (when (buffer-live-p dispgh832-buffer)
      (with-current-buffer dispgh832-buffer
        (when (and (boundp 'agent-shell--realign-timer)
                   (timerp agent-shell--realign-timer))
          (cancel-timer agent-shell--realign-timer))
        ;; Keep the package's real kill hooks/ACP shutdown/heartbeat cleanup.
        (let ((kill-buffer-query-functions nil))
          (condition-case error
              (kill-buffer (current-buffer))
            (error (dispgh832-log 'buffer-cleanup-error :error error))))))
    (dolist (process dispgh832-processes)
      ;; These objects were obtained only from our own shell, never a PID scan.
      (when (process-live-p process)
        (delete-process process))
      (accept-process-output process 0.05)
      (dispgh832-log 'owned-process-cleanup
        :name (process-name process) :pid (process-id process)
        :live (process-live-p process) :status (process-status process)
        :exit-status (process-exit-status process)))
    (advice-remove 'start-process #'dispgh832-observe-process-start)
    (advice-remove 'make-process #'dispgh832-observe-process-start)
    (with-temp-file (expand-file-name "samples.json" dispgh832-root)
      (insert (json-encode (vconcat (nreverse dispgh832-samples)))))
    (dispgh832-log 'finished :exit-code (or exit-code 0))
    (kill-emacs (or exit-code 0))))

(condition-case error
    (progn
      (dolist (name '("agent-shell" "shell-maker" "acp.el"))
        (let ((directory (expand-file-name name dispgh832-package-root)))
          (unless (file-directory-p directory)
            (error "Pinned package directory missing: %s" directory))
          (add-to-list 'load-path directory)))
      (require 'agent-shell)
      ;; Observe ownership without replacing package behavior.
      (advice-add 'start-process :around #'dispgh832-observe-process-start)
      (advice-add 'make-process :around #'dispgh832-observe-process-start)
      (setq agent-shell-transcript-file-path-function nil
            agent-shell-file-completion-enabled nil)
      (when-let* ((style (getenv "NEOMACS_832_HEADER_STYLE")))
        (setq agent-shell-header-style (intern style)))
      (let* ((python (or (executable-find "python3") (error "python3 required")))
             (backend (expand-file-name "agent-shell-fake-acp.py" dispgh832-source-root))
             (backend-receipt (expand-file-name "backend.jsonl" dispgh832-root))
             (config
              (agent-shell-make-agent-config
               :identifier 'dispgh832-local :mode-line-name "832 Local"
               :buffer-name "832 Local" :shell-prompt "Local> "
               :shell-prompt-regexp "Local> "
               ;; No :icon-name: use the real local fallback-SVG renderer.
               :welcome-function (lambda (_) "Owned local ACP reproduction.\n")
               :client-maker
               (lambda (buffer)
                 (agent-shell--make-acp-client
                  :command python :command-params (list backend "--receipt" backend-receipt)
                  :context-buffer buffer)))))
        (switch-to-buffer (get-buffer-create "832-Probe.java"))
        (insert "class Probe { int count = 1; }\n")
        (java-mode)
        (let ((window (split-window-right)))
          (setq dispgh832-buffer
                (let ((dispgh832-opening t))
                  (agent-shell--start :config config :no-focus t :new-session t
                    :session-id "832-local-session" :session-strategy 'new
                    :cwd dispgh832-root)))
          (set-window-buffer window dispgh832-buffer)
          (select-window window)))
      (dispgh832-log 'opened :version emacs-version
        :features system-configuration-features
        :package-root dispgh832-package-root
        :agent-shell-source (locate-library "agent-shell")
        :agent-shell-version (and (boundp 'agent-shell-version) agent-shell-version)
        :shell-maker-version shell-maker-version :acp-version acp-package-version)
      (dispgh832-sample 'initial)
      (dolist (delay '(0.2 0.7 1.5 3.0 5.0))
        (push (run-at-time delay nil #'dispgh832-sample delay) dispgh832-timers))
      (push (run-at-time (string-to-number (or (getenv "NEOMACS_832_SECONDS") "8"))
                         nil #'dispgh832-finish 0)
            dispgh832-timers))
  (error
   (dispgh832-log 'setup-error :error error)
   (dispgh832-finish 1)))
