pub(super) const MWIM_VISUAL_TUI_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'timer)

(defvar mwim358-tui-baseline nil)
(defvar mwim358-tui-owned-buffers nil)
(defvar mwim358-tui-epoch 0)
(defvar mwim358-tui-wide-origin nil)

(defun mwim358-tui-window-state ()
  (mapcar
   (lambda (window)
     (list :window window
           :buffer (window-buffer window)
           :edges (window-edges window)
           :point (window-point window)
           :start (window-start window)
           :hscroll (window-hscroll window)
           :vscroll (window-vscroll window t)
           :dedicated (window-dedicated-p window)
           :parameters
           (sort (copy-tree (seq-filter #'cdr (window-parameters window)))
                 (lambda (left right)
                   (string< (symbol-name (car left))
                            (symbol-name (car right)))))
           :prev-buffers (copy-tree (window-prev-buffers window))
           :next-buffers (copy-tree (window-next-buffers window))
           :margins (window-margins window)
           :fringes (window-fringes window)
           :scroll-bars (window-scroll-bars window)))
   (window-list nil 'no-minibuf)))

(defun mwim358-tui-restore-windows ()
  (let ((configuration (plist-get mwim358-tui-baseline :configuration))
        (structure (plist-get mwim358-tui-baseline :windows)))
    (set-window-configuration configuration)
    (dolist (entry structure)
      (let ((window (plist-get entry :window)))
        (unless (window-live-p window)
          (error "MWIM TUI baseline window died: %S" window))
        (dolist (parameter (window-parameters window))
          (set-window-parameter window (car parameter) nil))
        (dolist (parameter (plist-get entry :parameters))
          (set-window-parameter window (car parameter) (cdr parameter)))
        (set-window-prev-buffers
         window (copy-tree (plist-get entry :prev-buffers)))
        (set-window-next-buffers
         window (copy-tree (plist-get entry :next-buffers)))
        (set-window-point window (plist-get entry :point))
        (set-window-start window (plist-get entry :start) 'noforce)
        (set-window-hscroll window (plist-get entry :hscroll))
        (set-window-vscroll window (plist-get entry :vscroll) t)))))

(defun mwim358-tui-snapshot-baseline ()
  ;; This runs as the first interactive test command, after terminal startup.
  (when mwim358-tui-baseline
    (error "MWIM TUI baseline was already captured"))
  (setq mwim358-tui-baseline
        (list :buffers (buffer-list)
              :processes (process-list)
              :timers (copy-sequence timer-list)
              :idle-timers (copy-sequence timer-idle-list)
              :buffer (current-buffer)
              :window (selected-window)
              :configuration (current-window-configuration)
              :windows (mwim358-tui-window-state))))

(defun mwim358-tui-observe-command ()
  (when (memq this-command
              '(mwim-beginning-of-line-or-code mwim-end-of-line-or-code))
    (cl-incf mwim358-tui-epoch)
    (message
     "MWIM-MOVE e=%d p=%d line=%d col=%d begin=%S end=%S mod=%S undo=%S"
     mwim358-tui-epoch (point) (line-number-at-pos) (current-column)
     mwim-beginning-of-line-function mwim-end-of-line-function
     (buffer-modified-p) buffer-undo-list)))

(defun mwim358-tui-setup ()
  (interactive)
  (let ((source (symbol-file 'mwim 'defun)))
    (unless (and (featurep 'mwim)
                 (package-built-in-p 'seq '(2 24))
                 source
                 (string-suffix-p "/mwim.el" source)
                 (equal load-suffixes '(".el")))
      (error "MWIM TUI activation boundary failed: mwim=%S seq=%S source=%S suffixes=%S"
             (featurep 'mwim) (package-built-in-p 'seq '(2 24))
             source load-suffixes)))
  (mwim358-tui-snapshot-baseline)
  (delete-other-windows)
  (select-window (split-window-right -24))
  (unless (and (= (window-width) 24) (= (window-body-width) 24))
    (error "MWIM TUI geometry mismatch: width=%S body=%S"
           (window-width) (window-body-width)))
  (let ((buffer (generate-new-buffer " *mwim358-visual*")))
    (push buffer mwim358-tui-owned-buffers)
    (switch-to-buffer buffer)
    (text-mode)
    (insert
     "  alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron\n\twide 界 cedar birch maple spruce willow aspen oak\n")
    (set-buffer-modified-p nil)
    (setq buffer-undo-list nil)
    (setq-local word-wrap t)
    (visual-line-mode 1)
    (setq-local mwim-beginning-of-line-function
                #'beginning-of-visual-line)
    (setq-local mwim-end-of-line-function #'end-of-visual-line)
    (let ((map (make-sparse-keymap)))
      (set-keymap-parent map (current-local-map))
      (define-key map (kbd "C-a") #'mwim-beginning-of-line-or-code)
      (define-key map (kbd "C-e") #'mwim-end-of-line-or-code)
      (use-local-map map))
    (add-hook 'post-command-hook #'mwim358-tui-observe-command nil t)
    (setq mwim358-tui-wide-origin
          (save-excursion
            (goto-char (point-min))
            (forward-line 1)
            (search-forward "maple")
            (point)))
    (goto-char 48)
    (set-window-point (selected-window) (point))
    (redisplay t)
    (message
     "MWIM-VISUAL-SETUP w=%d b=%d p=%d chars=%d begin=%S end=%S visual=%S wrap=%S"
     (window-width) (window-body-width) (point) (buffer-size)
     mwim-beginning-of-line-function mwim-end-of-line-function
     visual-line-mode word-wrap)))

(defun mwim358-tui-reset-middle ()
  (interactive)
  (goto-char 48)
  (set-window-point (selected-window) (point))
  (redisplay t)
  (message "MWIM-VISUAL-RESET-MIDDLE p=%d" (point)))

(defun mwim358-tui-reset-final ()
  (interactive)
  (goto-char 80)
  (set-window-point (selected-window) (point))
  (redisplay t)
  (message "MWIM-VISUAL-RESET-FINAL p=%d" (point)))

(defun mwim358-tui-use-wide-visual ()
  (interactive)
  (setq-local mwim-beginning-of-line-function
              #'beginning-of-visual-line)
  (setq-local mwim-end-of-line-function #'end-of-visual-line)
  (goto-char mwim358-tui-wide-origin)
  (set-window-point (selected-window) (point))
  (redisplay t)
  (message
   "MWIM-WIDE-VISUAL-RESET p=%d line=%d col=%d begin=%S end=%S"
   (point) (line-number-at-pos) (current-column)
   mwim-beginning-of-line-function mwim-end-of-line-function))

(defun mwim358-tui-reset-wide-visual ()
  (interactive)
  (goto-char mwim358-tui-wide-origin)
  (set-window-point (selected-window) (point))
  (redisplay t)
  (message "MWIM-WIDE-VISUAL-RESET-AGAIN p=%d line=%d col=%d"
           (point) (line-number-at-pos) (current-column)))

(defun mwim358-tui-use-logical ()
  (interactive)
  (setq-local mwim-beginning-of-line-function #'beginning-of-line)
  (setq-local mwim-end-of-line-function #'end-of-line)
  (goto-char mwim358-tui-wide-origin)
  (set-window-point (selected-window) (point))
  (redisplay t)
  (message
   "MWIM-LOGICAL-RESET p=%d line=%d col=%d begin=%S end=%S"
   (point) (line-number-at-pos) (current-column)
   mwim-beginning-of-line-function mwim-end-of-line-function))

(defun mwim358-tui-reset-logical ()
  (interactive)
  (goto-char mwim358-tui-wide-origin)
  (set-window-point (selected-window) (point))
  (redisplay t)
  (message "MWIM-LOGICAL-RESET-AGAIN p=%d line=%d col=%d"
           (point) (line-number-at-pos) (current-column)))

(defun mwim358-tui-cleanup ()
  (interactive)
  (let (errors state)
    (cl-labels
        ((attempt
          (phase function)
          (condition-case condition
              (funcall function)
            (t (push (list phase condition) errors))))
         (sweep
          (number)
          (dolist
              (process
               (seq-difference
                (process-list) (plist-get mwim358-tui-baseline :processes)
                #'eq))
            (attempt
             (list 'process number)
             (lambda ()
               (set-process-query-on-exit-flag process nil)
               (when (process-live-p process) (delete-process process)))))
          (dolist
              (timer
               (delete-dups
                (append
                 (seq-difference
                  timer-list (plist-get mwim358-tui-baseline :timers) #'eq)
                 (seq-difference
                  timer-idle-list
                  (plist-get mwim358-tui-baseline :idle-timers) #'eq))))
            (attempt (list 'timer number) (lambda () (cancel-timer timer))))
          (dolist
              (buffer
               (seq-difference
                (buffer-list) (plist-get mwim358-tui-baseline :buffers) #'eq))
            (attempt
             (list 'buffer number)
             (lambda ()
               (when (buffer-live-p buffer)
                 (set-buffer-modified-p nil)
                 (kill-buffer buffer)))))))
      (if (not mwim358-tui-baseline)
          (push '(baseline missing) errors)
        (attempt 'window-first #'mwim358-tui-restore-windows)
        (dotimes (number 2) (sweep number))
        (attempt 'window-final #'mwim358-tui-restore-windows)
        (attempt
         'select-baseline
         (lambda ()
           (let ((buffer (plist-get mwim358-tui-baseline :buffer))
                 (window (plist-get mwim358-tui-baseline :window)))
             (unless (and (buffer-live-p buffer) (window-live-p window))
               (error "MWIM TUI selected baseline state died"))
             (select-window window)
             (set-buffer buffer)))))
      (setq errors (nreverse errors))
      (setq state
            (list
             :new-buffers
             (seq-difference
              (buffer-list) (plist-get mwim358-tui-baseline :buffers) #'eq)
             :new-processes
             (seq-difference
              (process-list) (plist-get mwim358-tui-baseline :processes) #'eq)
             :new-timers
             (delete-dups
              (append
               (seq-difference
                timer-list (plist-get mwim358-tui-baseline :timers) #'eq)
               (seq-difference
                timer-idle-list
                (plist-get mwim358-tui-baseline :idle-timers) #'eq)))
             :owned-live (mapcar #'buffer-live-p mwim358-tui-owned-buffers)
             :windows (equal (mwim358-tui-window-state)
                             (plist-get mwim358-tui-baseline :windows))
             :configuration
             (compare-window-configurations
              (current-window-configuration)
              (plist-get mwim358-tui-baseline :configuration))
             :buffer (eq (current-buffer)
                         (plist-get mwim358-tui-baseline :buffer))
             :window (eq (selected-window)
                         (plist-get mwim358-tui-baseline :window))))
      (unless (and (null errors)
                   (null (plist-get state :new-buffers))
                   (null (plist-get state :new-processes))
                   (null (plist-get state :new-timers))
                   (not (memq t (plist-get state :owned-live)))
                   (plist-get state :windows)
                   (plist-get state :configuration)
                   (plist-get state :buffer)
                   (plist-get state :window))
        (error "MWIM TUI cleanup failure: errors=%S state=%S" errors state))
      (message "MWIM-VISUAL-CLEAN ok=t errors=nil resources=nil windows=t"))))
"####;
