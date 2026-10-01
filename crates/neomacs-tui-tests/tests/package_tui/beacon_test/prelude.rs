pub(super) const BEACON_TUI_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'timer)
(let ((load-suffixes '(".elc" ".el")))
  (require 'compile))
(unless (equal load-suffixes '(".el"))
  (error "Beacon TUI dependency boundary leaked suffixes: %S" load-suffixes))

(defvar beacon359-tui-baseline nil)
(defvar beacon359-tui-owned-buffers nil)
(defvar beacon359-tui-owned-overlays nil)
(defvar beacon359-tui-owned-timers nil)
(defvar beacon359-tui-observer-timers nil)
(defvar beacon359-tui-blinks 0)
(defvar beacon359-tui-focus-armed nil)
(defvar beacon359-tui-pending-arm nil)
(defvar beacon359-tui-package-defaults nil)
(defvar beacon359-tui-next-start nil)
(defvar beacon359-tui-buffer-observed nil)

(defconst beacon359-tui-state-symbols
  '(beacon-mode beacon-push-mark
    beacon-blink-when-point-moves-vertically
    beacon-blink-when-point-moves-horizontally
    beacon-blink-when-buffer-changes beacon-blink-when-window-scrolls
    beacon-blink-when-window-changes beacon-blink-when-focused
    beacon-blink-duration beacon-blink-delay beacon-size beacon-color
    beacon-dont-blink-predicates beacon-dont-blink-major-modes
    beacon-dont-blink-commands beacon-before-blink-hook beacon-lighter
    beacon--timer beacon--ovs beacon--window-scrolled beacon--previous-place
    beacon--previous-mark-head beacon--previous-window
    beacon--previous-window-start pre-command-hook post-command-hook
    before-change-functions window-scroll-functions
    after-focus-change-function transient-mark-mode global-mark-ring
    unread-command-events))

(defun beacon359-tui-copy (value)
  (cond ((consp value)
         (cons (beacon359-tui-copy (car value))
               (beacon359-tui-copy (cdr value))))
        ((vectorp value)
         (apply #'vector (mapcar #'beacon359-tui-copy (append value nil))))
        ((stringp value) (copy-sequence value))
        (t value)))

(defun beacon359-tui-variable-state (symbol)
  (if (boundp symbol)
      (list :bound t :value (beacon359-tui-copy (symbol-value symbol)))
    '(:bound nil)))

(defun beacon359-tui-restore-variable (symbol state)
  (if (plist-get state :bound)
      (set symbol (beacon359-tui-copy (plist-get state :value)))
    (makunbound symbol)))

(defun beacon359-tui-window-parameters (window)
  (sort (copy-tree (seq-filter #'cdr (window-parameters window)))
        (lambda (left right)
          (string< (symbol-name (car left)) (symbol-name (car right))))))

(defun beacon359-tui-window-state ()
  (mapcar
   (lambda (window)
     (list :window window :buffer (window-buffer window)
           :edges (window-edges window) :point (window-point window)
           :start (window-start window) :hscroll (window-hscroll window)
           :vscroll (window-vscroll window t)
           :dedicated (window-dedicated-p window)
           :parameters (beacon359-tui-window-parameters window)
           :prev-buffers (copy-tree (window-prev-buffers window))
           :next-buffers (copy-tree (window-next-buffers window))
           :margins (window-margins window)
           :fringes (window-fringes window)
           :scroll-bars (window-scroll-bars window)))
   (window-list nil 'no-minibuf)))

(defun beacon359-tui-restore-windows ()
  (let ((configuration (plist-get beacon359-tui-baseline :configuration))
        (structure (plist-get beacon359-tui-baseline :windows)))
    (set-window-configuration configuration)
    (dolist (entry structure)
      (let ((window (plist-get entry :window)))
        (unless (window-live-p window)
          (error "Beacon TUI baseline window died: %S" window))
        (dolist (parameter (window-parameters window))
          (set-window-parameter window (car parameter) nil))
        (dolist (parameter (plist-get entry :parameters))
          (set-window-parameter window (car parameter) (cdr parameter)))
        (set-window-prev-buffers window
                                 (copy-tree (plist-get entry :prev-buffers)))
        (set-window-next-buffers window
                                 (copy-tree (plist-get entry :next-buffers)))
        (set-window-point window (plist-get entry :point))
        (set-window-start window (plist-get entry :start) 'noforce)
        (set-window-hscroll window (plist-get entry :hscroll))
        (set-window-vscroll window (plist-get entry :vscroll) t)))))

(defun beacon359-tui-capture-baseline ()
  (when beacon359-tui-baseline
    (error "Beacon TUI baseline already captured"))
  (setq beacon359-tui-baseline
        (list :buffers (buffer-list) :processes (process-list)
              :timers (copy-sequence timer-list)
              :idle-timers (copy-sequence timer-idle-list)
              :buffer (current-buffer) :window (selected-window)
              :focus (frame-focus-state)
              :configuration (current-window-configuration)
              :windows (beacon359-tui-window-state)
              :states
              (mapcar (lambda (symbol)
                        (cons symbol (beacon359-tui-variable-state symbol)))
                      beacon359-tui-state-symbols))))

(defun beacon359-tui-own-buffer (name text)
  (when (get-buffer name)
    (error "Beacon TUI refuses preexisting buffer: %S" name))
  (let ((buffer (generate-new-buffer name)))
    (push buffer beacon359-tui-owned-buffers)
    (with-current-buffer buffer
      (text-mode)
      (insert text)
      (set-buffer-modified-p nil)
      (setq buffer-undo-list nil)
      (local-set-key (kbd "C-c j") #'beacon359-tui-jump-five)
      (local-set-key (kbd "C-c h") #'beacon359-tui-jump-column))
    buffer))

(defun beacon359-tui-live-overlays ()
  (seq-filter #'overlay-buffer (copy-sequence beacon--ovs)))

(defun beacon359-tui-own-action ()
  (when (timerp beacon--timer)
    (cl-pushnew beacon--timer beacon359-tui-owned-timers :test #'eq))
  (dolist (overlay beacon--ovs)
    (cl-pushnew overlay beacon359-tui-owned-overlays :test #'eq)))

(defun beacon359-tui-state (tag)
  (beacon359-tui-own-action)
  (message
   "B359-%s p=%d l=%d s=%d b=%S o=%d t=%S w=%S n=%d"
   tag (point) (line-number-at-pos)
   (line-number-at-pos (window-start))
   (cond ((equal (buffer-name) " *beacon359-source*") 'source)
         ((equal (buffer-name) " *beacon359-scroll*") 'scroll)
         ((equal (buffer-name) " *beacon359-other*") 'other)
         (t major-mode))
   (length (beacon359-tui-live-overlays))
   (and (timerp beacon--timer) (memq beacon--timer timer-list) t)
   (cl-every (lambda (overlay)
               (eq (overlay-get overlay 'window) (selected-window)))
             (beacon359-tui-live-overlays))
   beacon359-tui-blinks))

(defun beacon359-tui-before-blink ()
  (cl-incf beacon359-tui-blinks)
  (when beacon359-tui-focus-armed
    (message "B359-FOCUS-HOOK n=%d state=%S"
             beacon359-tui-blinks (frame-focus-state))
    (let ((timer (run-at-time 0.05 nil #'beacon359-tui-focus-applied)))
      (push timer beacon359-tui-observer-timers))))

(defun beacon359-tui-focus-applied ()
  (beacon359-tui-own-action)
  (let* ((overlays
          (sort (beacon359-tui-live-overlays)
                (lambda (left right)
                  (< (overlay-start left) (overlay-start right)))))
         (ranges (mapcar (lambda (overlay)
                           (cons (overlay-start overlay) (overlay-end overlay)))
                         overlays))
         (beacons (cl-every (lambda (overlay) (overlay-get overlay 'beacon))
                            overlays))
         (faces (mapcar (lambda (overlay)
                          (plist-get (overlay-get overlay 'face) :background))
                        overlays))
         (windows (cl-every (lambda (overlay)
                              (eq (overlay-get overlay 'window)
                                  (selected-window)))
                            overlays))
         (listed (and (timerp beacon--timer)
                      (memq beacon--timer timer-list) t)))
    (unless (and (equal ranges '((1 . 2) (2 . 3) (3 . 4)))
                 beacons
                 (equal faces '("#ffff00000000" "#f97939793979"
                                "#f2f372f272f2"))
                 windows listed)
      (error "Beacon focus overlay state drifted: %S"
             (list ranges beacons faces windows listed)))
    (message "B359-FOCUS-APPLIED n=%d s=%S r=1-2/2-3/3-4 b=t f=t w=t t=t"
             beacon359-tui-blinks (frame-focus-state))))

(defun beacon359-tui-post-command ()
  (when (and (not beacon359-tui-buffer-observed)
             (equal (buffer-name) " *beacon359-other*"))
    (setq beacon359-tui-buffer-observed t)
    (let ((timer (run-at-time 0.15 nil #'beacon359-tui-buffer-state)))
      (push timer beacon359-tui-observer-timers)))
  (pcase this-command
    ('scroll-up-command
     (beacon359-tui-state "SCROLL")
     (beacon359-tui-schedule-state "SCROLL-AFTER"))
    ('other-window
     (beacon359-tui-state "WINDOW")
     (beacon359-tui-arm-next
      'switch-to-buffer '((beacon-blink-when-buffer-changes . t))))
    ('next-line
     (beacon359-tui-state "NEXT")
     (let ((timer (run-at-time 0.15 nil #'beacon359-tui-next-state)))
       (push timer beacon359-tui-observer-timers)))
    ('beacon359-tui-jump-five (beacon359-tui-jump-state))
    ('beacon359-tui-jump-column (beacon359-tui-horizontal-state))))

(defun beacon359-tui-schedule-state (tag)
  (let ((timer (run-at-time 0.15 nil #'beacon359-tui-state tag)))
    (push timer beacon359-tui-observer-timers)))

(defun beacon359-tui-buffer-state ()
  (unless (and (equal (buffer-name) " *beacon359-other*")
               (eq major-mode 'text-mode)
               (equal (buffer-string)
                      "other target alpha beta gamma delta\n"))
    (error "Beacon TUI selected the wrong existing buffer: %S"
           (list (buffer-name) major-mode (buffer-string))))
  (beacon359-tui-state "BUFFER"))

(defun beacon359-tui-next-state ()
  (let* ((after (line-number-at-pos (window-start)))
         (delta (- after beacon359-tui-next-start))
         (overlays (length (beacon359-tui-live-overlays)))
         (listed (and (timerp beacon--timer)
                      (memq beacon--timer timer-list) t)))
    (beacon359-tui-own-action)
    (unless (and (> delta 0) (= overlays 0) (not listed)
                 (= beacon359-tui-blinks 0))
      (error "Beacon default command suppression failed: %S"
             (list beacon359-tui-next-start after delta overlays listed
                   beacon359-tui-blinks)))
    (message
     "B359-NEXT-AFTER p=%d l=%d before=%d after=%d delta=%d o=%d t=%S n=%d"
     (point) (line-number-at-pos) beacon359-tui-next-start after delta
     overlays listed beacon359-tui-blinks)))

(defun beacon359-tui-arm-next-command ()
  (when (and beacon359-tui-pending-arm
             (eq this-command (car beacon359-tui-pending-arm)))
    (dolist (entry (cdr beacon359-tui-pending-arm))
      (set (car entry) (cdr entry)))
    (setq beacon359-tui-pending-arm nil
          beacon359-tui-blinks 0)))

(defun beacon359-tui-arm-next (command settings)
  (setq beacon359-tui-pending-arm (cons command settings)))

(defun beacon359-tui-jump-five ()
  (interactive)
  (forward-line 5))

(defun beacon359-tui-jump-column ()
  (interactive)
  (move-to-column 12))

(defun beacon359-tui-jump-state ()
  (beacon359-tui-own-action)
  (message
   "B359-JUMP p=%d l=%d m=%S a=%S r=%S o=%d t=%S n=%d"
   (point) (line-number-at-pos) (mark t) mark-active
   (mapcar #'marker-position mark-ring)
   (length (beacon359-tui-live-overlays))
   (and (timerp beacon--timer) (memq beacon--timer timer-list) t)
   beacon359-tui-blinks))

(defun beacon359-tui-setup ()
  (interactive)
  (let ((source (symbol-file 'beacon-blink 'defun)))
    (unless (and (featurep 'beacon) source
                 (string-suffix-p "/beacon.el" source)
                 (package-built-in-p 'seq '(2 24))
                 (featurep 'compile)
                 (string-suffix-p ".elc"
                                  (or (symbol-file 'compilation-mode 'defun) ""))
                 (equal load-suffixes '(".el"))
                 (= (display-color-cells) 16777216)
                 (eq (display-visual-class) 'static-color))
      (error "Beacon TUI activation failed: %S"
             (list source load-suffixes (display-color-cells)
                   (display-visual-class)
                   (symbol-file 'compilation-mode 'defun)))))
  (setq beacon359-tui-package-defaults
        (list :predicates (copy-sequence beacon-dont-blink-predicates)
              :modes (copy-sequence beacon-dont-blink-major-modes)
              :commands (copy-sequence beacon-dont-blink-commands)))
  (unless (equal beacon359-tui-package-defaults
                 '(:predicates
                   (beacon--compilation-mode-p window-minibuffer-p)
                   :modes
                   (t magit-status-mode magit-popup-mode inf-ruby-mode
                      mu4e-headers-mode gnus-summary-mode gnus-group-mode)
                   :commands (next-line previous-line forward-line)))
    (error "Beacon TUI package defaults changed: %S"
           beacon359-tui-package-defaults))
  (beacon359-tui-capture-baseline)
  (delete-other-windows)
  (let ((source
         (beacon359-tui-own-buffer
          " *beacon359-source*"
          (concat
           "manual alpha beta gamma delta\n"
           "manual-eol\n"
           "\t界abcdefghijklmnop\n"
           (mapconcat
            (lambda (number)
              (format "row %02d | alpha beta gamma delta epsilon" number))
            (number-sequence 0 69) "\n") "\n")))
        (other (beacon359-tui-own-buffer
                " *beacon359-other*"
                "other target alpha beta gamma delta\n"))
        (scroll
         (beacon359-tui-own-buffer
          " *beacon359-scroll*"
          (concat
           (mapconcat
            (lambda (number)
              (format "row %02d | alpha beta gamma delta epsilon" number))
            (number-sequence 0 69) "\n") "\n"))))
    (switch-to-buffer source)
    (goto-char (point-min))
    (beacon359-tui-reset-automatic)
    (setq beacon-before-blink-hook '(beacon359-tui-before-blink))
    (beacon-mode 1)
    (add-hook 'pre-command-hook #'beacon359-tui-arm-next-command t)
    (add-hook 'post-command-hook #'beacon359-tui-post-command t)
    (set-window-start (selected-window) (point-min))
    (redisplay t)
    (unless (string-match-p (regexp-quote "(*)")
                            (format-mode-line mode-line-format))
      (error "Beacon TUI enabled lighter is not visible: %S"
             (format-mode-line mode-line-format)))
    (message
     "B359-SETUP cells=%d class=%S defaults=t lighter=t focus=%S"
     (display-color-cells) (display-visual-class)
     (frame-focus-state))))

(defun beacon359-tui-show-source ()
  (interactive)
  (message "B359-SOURCE subject=%s seq=%S compile=%s suffix=%S"
           (file-name-nondirectory (symbol-file 'beacon-blink 'defun))
           (package-built-in-p 'seq '(2 24))
           (file-name-nondirectory (symbol-file 'compilation-mode 'defun))
           load-suffixes))

(defun beacon359-tui-manual ()
  (interactive)
  (switch-to-buffer " *beacon359-source*")
  (goto-char (point-min))
  (search-forward "manual ")
  (setq beacon359-tui-blinks 0)
  (call-interactively #'beacon-blink)
  (beacon359-tui-state "MANUAL"))

(defun beacon359-tui-eol ()
  (interactive)
  (switch-to-buffer " *beacon359-source*")
  (goto-char (point-min))
  (forward-line 1)
  (end-of-line)
  (setq beacon359-tui-blinks 0)
  (call-interactively #'beacon-blink)
  (beacon359-tui-state "EOL"))

(defun beacon359-tui-natural-finished ()
  (message "B359-NATURAL-DONE ovs=%d listed=%S timerp=%S"
           (length (beacon359-tui-live-overlays))
           (and (timerp beacon--timer) (memq beacon--timer timer-list) t)
           (timerp beacon--timer)))

(defun beacon359-tui-natural ()
  (interactive)
  (switch-to-buffer " *beacon359-source*")
  (goto-char (point-min))
  (search-forward "manual ")
  (setq beacon-size 8 beacon-color "#00ff00"
        beacon-blink-delay 0.8 beacon-blink-duration 1.4
        beacon359-tui-blinks 0)
  (call-interactively #'beacon-blink)
  (beacon359-tui-own-action)
  (let ((observer (run-at-time 2.8 nil #'beacon359-tui-natural-finished)))
    (push observer beacon359-tui-observer-timers))
  (beacon359-tui-state "NATURAL-START"))

(defun beacon359-tui-reset-automatic ()
  (setq beacon-size 8 beacon-color "#00ffff"
        beacon-blink-delay 30 beacon-blink-duration 1.0
        beacon-push-mark nil
        beacon-blink-when-point-moves-vertically nil
        beacon-blink-when-point-moves-horizontally nil
        beacon-blink-when-buffer-changes nil
        beacon-blink-when-window-changes nil
        beacon-blink-when-window-scrolls nil
        beacon-blink-when-focused nil
        beacon-dont-blink-predicates
        (copy-sequence
         (plist-get beacon359-tui-package-defaults :predicates))
        beacon-dont-blink-major-modes
        (copy-sequence (plist-get beacon359-tui-package-defaults :modes))
        beacon-dont-blink-commands
        (copy-sequence (plist-get beacon359-tui-package-defaults :commands))
        beacon359-tui-pending-arm nil
        beacon359-tui-blinks 0))

(defun beacon359-tui-prepare-scroll ()
  (interactive)
  (beacon359-tui-reset-automatic)
  (delete-other-windows)
  (switch-to-buffer " *beacon359-scroll*")
  (goto-char (point-min))
  (set-window-start (selected-window) (point))
  (goto-char (window-start))
  (beacon359-tui-arm-next
   'scroll-up-command
   '((beacon-blink-when-window-scrolls . t)))
  (redisplay t)
  (message "B359-SCROLL-READY p=%d l=%d s=%d"
           (point) (line-number-at-pos)
           (line-number-at-pos (window-start))))

(defun beacon359-tui-prepare-windows ()
  (interactive)
  (beacon359-tui-reset-automatic)
  (setq beacon359-tui-buffer-observed nil)
  (delete-other-windows)
  (switch-to-buffer " *beacon359-source*")
  (goto-char (point-min))
  (let ((left (selected-window))
        (right (split-window-right)))
    (set-window-buffer right (current-buffer))
    (set-window-point right (point))
    (select-window left)
    (beacon359-tui-arm-next
     'other-window '((beacon-blink-when-window-changes . t)))
    (redisplay t)
    (message "B359-WINDOW-READY count=%d same=%S selected-left=%S"
             (length (window-list nil 'no-minibuf))
             (eq (window-buffer left) (window-buffer right))
             (eq (selected-window) left))))

(defun beacon359-tui-prepare-next ()
  (interactive)
  (beacon359-tui-reset-automatic)
  (setq beacon-dont-blink-commands '(next-line previous-line forward-line))
  (delete-other-windows)
  (switch-to-buffer " *beacon359-source*")
  (goto-char (point-min))
  (forward-line 3)
  (set-window-start (selected-window) (point))
  (goto-char (window-end nil t))
  (forward-line -1)
  (setq beacon359-tui-next-start
        (line-number-at-pos (window-start)))
  (beacon359-tui-arm-next
   'next-line '((beacon-blink-when-window-scrolls . t)))
  (redisplay t)
  (message "B359-NEXT-READY p=%d l=%d s=%d"
           (point) (line-number-at-pos)
           (line-number-at-pos (window-start))))

(defun beacon359-tui-block-p () t)

(defun beacon359-tui-configure-suppression ()
  (interactive)
  (beacon359-tui-reset-automatic)
  (switch-to-buffer " *beacon359-source*")
  (goto-char (point-min))
  (beacon359-tui-arm-next
   'beacon359-tui-jump-five
   '((beacon-blink-when-point-moves-vertically . 1)
     (beacon-dont-blink-predicates . (beacon359-tui-block-p))))
  (message "B359-SUPPRESS-READY kind=predicate"))

(defun beacon359-tui-configure-major ()
  (interactive)
  (beacon359-tui-reset-automatic)
  (switch-to-buffer " *beacon359-source*")
  (goto-char (point-min))
  (beacon359-tui-arm-next
   'beacon359-tui-jump-five
   '((beacon-blink-when-point-moves-vertically . 1)
     (beacon-dont-blink-major-modes . (text-mode))))
  (message "B359-SUPPRESS-READY kind=major"))

(defun beacon359-tui-configure-command ()
  (interactive)
  (beacon359-tui-reset-automatic)
  (switch-to-buffer " *beacon359-source*")
  (goto-char (point-min))
  (beacon359-tui-arm-next
   'beacon359-tui-jump-five
   '((beacon-blink-when-point-moves-vertically . 1)
     (beacon-dont-blink-commands . (beacon359-tui-jump-five))))
  (message "B359-SUPPRESS-READY kind=command"))

(defun beacon359-tui-configure-local ()
  (interactive)
  (beacon359-tui-reset-automatic)
  (switch-to-buffer " *beacon359-source*")
  (goto-char (point-min))
  (setq-local beacon-mode nil)
  (beacon359-tui-arm-next
   'beacon359-tui-jump-five
   '((beacon-blink-when-point-moves-vertically . 1)))
  (let ((global-count
         (cl-count #'beacon--post-command
                   (default-value 'post-command-hook) :test #'eq)))
    (unless (= global-count 1)
      (error "Beacon TUI global lifecycle hook drifted: %S" global-count))
    (message "B359-SUPPRESS-READY kind=local mode=%S global-hook=%d"
             beacon-mode global-count)))

(defun beacon359-tui-configure-compilation ()
  (interactive)
  (beacon359-tui-reset-automatic)
  (switch-to-buffer " *beacon359-source*")
  (compilation-mode)
  (local-set-key (kbd "C-c j") #'beacon359-tui-jump-five)
  (goto-char (point-min))
  (beacon359-tui-arm-next
   'beacon359-tui-jump-five
     `((beacon-blink-when-point-moves-vertically . 1)
     (beacon-dont-blink-predicates
      . ,(copy-sequence
          (plist-get beacon359-tui-package-defaults :predicates)))))
  (message "B359-SUPPRESS-READY kind=compilation mode=%S defaults=t"
           major-mode))

(defun beacon359-tui-recover ()
  (interactive)
  (beacon359-tui-reset-automatic)
  (switch-to-buffer " *beacon359-source*")
  (unless (eq major-mode 'text-mode) (text-mode))
  (kill-local-variable 'beacon-mode)
  (local-set-key (kbd "C-c j") #'beacon359-tui-jump-five)
  (local-set-key (kbd "C-c h") #'beacon359-tui-jump-column)
  (goto-char (point-min))
  (beacon359-tui-arm-next
   'beacon359-tui-jump-five
   '((beacon-blink-when-point-moves-vertically . 1)))
  (message "B359-RECOVER-READY mode=%S local=%S"
           beacon-mode (local-variable-p 'beacon-mode)))

(defun beacon359-tui-prepare-mark-world (ring-head)
  (beacon359-tui-reset-automatic)
  (switch-to-buffer " *beacon359-source*")
  (text-mode)
  (local-set-key (kbd "C-c j") #'beacon359-tui-jump-five)
  (goto-char (point-min))
  (setq-local mark-ring
              (and ring-head (list (copy-marker ring-head))))
  (set-marker (mark-marker) nil)
  (setq transient-mark-mode t
        beacon-blink-when-buffer-changes nil
        beacon-blink-when-window-changes nil
        beacon-blink-when-window-scrolls nil)
  (beacon359-tui-arm-next
   'beacon359-tui-jump-five '((beacon-push-mark . 1))))

(defun beacon359-tui-prepare-mark ()
  (interactive)
  (beacon359-tui-prepare-mark-world 3)
  (message "B359-MARK-READY blink=nil head=%d"
           (marker-position (car mark-ring))))

(defun beacon359-tui-prepare-mark-blink ()
  (interactive)
  (beacon359-tui-prepare-mark-world nil)
  (beacon359-tui-arm-next
   'beacon359-tui-jump-five
   '((beacon-push-mark . 1)
     (beacon-blink-when-point-moves-vertically . 1)
     (beacon-color . "#ff00ff")))
  (message "B359-MARK-READY blink=vertical"))

(defun beacon359-tui-prepare-active-mark ()
  (interactive)
  (beacon359-tui-reset-automatic)
  (switch-to-buffer " *beacon359-source*")
  (text-mode)
  (local-set-key (kbd "C-c j") #'beacon359-tui-jump-five)
  (goto-char (point-min))
  (setq-local mark-ring nil)
  (set-mark 3)
  (setq mark-active t transient-mark-mode t)
  (beacon359-tui-arm-next
   'beacon359-tui-jump-five '((beacon-push-mark . 1)))
  (message "B359-MARK-READY blink=nil active=t mark=%d" (mark)))

(defun beacon359-tui-horizontal-state ()
  (beacon359-tui-own-action)
  (message
   "B359-HORIZONTAL p=%d l=%d col=%d v=%S h=%S o=%d t=%S n=%d"
   (point) (line-number-at-pos) (current-column)
   beacon-blink-when-point-moves-vertically
   beacon-blink-when-point-moves-horizontally
   (length (beacon359-tui-live-overlays))
   (and (timerp beacon--timer) (memq beacon--timer timer-list) t)
   beacon359-tui-blinks))

(defun beacon359-tui-prepare-horizontal (vertical)
  (beacon359-tui-reset-automatic)
  (switch-to-buffer " *beacon359-source*")
  (text-mode)
  (local-set-key (kbd "C-c h") #'beacon359-tui-jump-column)
  (goto-char (point-min))
  (forward-line 2)
  (setq beacon-size 3 beacon-color "#00ffff")
  (beacon359-tui-arm-next
   'beacon359-tui-jump-column
   `((beacon-blink-when-point-moves-vertically . ,vertical)
     (beacon-blink-when-point-moves-horizontally . 5)))
  (message "B359-HORIZONTAL-READY vertical=%S p=%d col=%d"
           vertical (point) (current-column)))

(defun beacon359-tui-prepare-horizontal-alone ()
  (interactive) (beacon359-tui-prepare-horizontal nil))

(defun beacon359-tui-prepare-horizontal-coupled ()
  (interactive) (beacon359-tui-prepare-horizontal 1))

(defun beacon359-tui-arm-focus ()
  (interactive)
  (beacon359-tui-reset-automatic)
  (switch-to-buffer " *beacon359-source*")
  (text-mode)
  (goto-char (point-min))
  (setq-local mark-ring nil)
  (set-marker (mark-marker) nil)
  (setq mark-active nil)
  (setq beacon-color "#ff0000" beacon-size 4
        beacon-blink-delay 0.6 beacon-blink-duration 1.8
        beacon-blink-when-focused t
        beacon359-tui-focus-armed t beacon359-tui-blinks 0)
  (message "B359-FOCUS-READY state=%S" (frame-focus-state)))

(defun beacon359-tui-focus-report ()
  (interactive)
  (setq beacon359-tui-focus-armed nil)
  (message "B359-FOCUS-REPORT n=%d state=%S timerp=%S listed=%S"
           beacon359-tui-blinks (frame-focus-state) (timerp beacon--timer)
           (and (timerp beacon--timer) (memq beacon--timer timer-list) t)))

(defun beacon359-tui-cleanup ()
  (interactive)
  (let (errors state owned-overlays owned-timers)
    (cl-labels
        ((attempt
          (phase function)
          (condition-case condition
              (funcall function)
            (t (push (list phase condition) errors))))
         (sweep
          (number)
          (dolist
              (timer
               (delete-dups
                (append
                 (seq-difference timer-list
                                 (plist-get beacon359-tui-baseline :timers) #'eq)
                 (seq-difference timer-idle-list
                                 (plist-get beacon359-tui-baseline :idle-timers)
                                 #'eq))))
            (attempt (list 'timer number) (lambda () (cancel-timer timer))))
          (dolist
              (process
               (seq-difference (process-list)
                               (plist-get beacon359-tui-baseline :processes)
                               #'eq))
            (attempt (list 'process number)
                     (lambda ()
                       (set-process-query-on-exit-flag process nil)
                       (when (process-live-p process) (delete-process process)))))
          (dolist
              (buffer
               (seq-difference (buffer-list)
                               (plist-get beacon359-tui-baseline :buffers) #'eq))
            (attempt (list 'buffer number)
                     (lambda ()
                       (when (buffer-live-p buffer)
                         (set-buffer-modified-p nil)
                         (kill-buffer buffer)))))))
      (if (not beacon359-tui-baseline)
          (push '(baseline missing) errors)
        (setq owned-overlays
              (delete-dups
               (append beacon359-tui-owned-overlays
                       (copy-sequence beacon--ovs))))
        (dolist (buffer beacon359-tui-owned-buffers)
          (when (buffer-live-p buffer)
            (dolist (overlay
                     (with-current-buffer buffer
                       (overlays-in (point-min) (point-max))))
              (when (overlay-get overlay 'beacon)
                (cl-pushnew overlay owned-overlays :test #'eq)))))
        (setq owned-timers
              (delete-dups
               (append beacon359-tui-owned-timers
                       beacon359-tui-observer-timers
                       (and (timerp beacon--timer) (list beacon--timer)))))
        (attempt 'disable-mode
                 (lambda () (when (bound-and-true-p beacon-mode)
                              (beacon-mode -1))))
        (dolist (timer owned-timers)
          (attempt 'owned-timer (lambda () (cancel-timer timer))))
        (dolist (overlay owned-overlays)
          (attempt 'owned-overlay
                   (lambda () (when (overlayp overlay)
                                (delete-overlay overlay)))))
        (attempt 'window-first #'beacon359-tui-restore-windows)
        (dotimes (number 2) (sweep number))
        (dolist (entry (plist-get beacon359-tui-baseline :states))
          (attempt (list 'variable (car entry))
                   (lambda ()
                     (beacon359-tui-restore-variable (car entry) (cdr entry)))))
        (attempt 'window-final #'beacon359-tui-restore-windows)
        (attempt
         'select-baseline
         (lambda ()
           (let ((buffer (plist-get beacon359-tui-baseline :buffer))
                 (window (plist-get beacon359-tui-baseline :window)))
             (unless (and (buffer-live-p buffer) (window-live-p window))
               (error "Beacon TUI baseline selection died"))
             (select-window window)
             (set-buffer buffer)))))
      (setq errors (nreverse errors))
      (setq state
            (list
             :new-buffers
             (seq-difference (buffer-list)
                             (plist-get beacon359-tui-baseline :buffers) #'eq)
             :new-processes
             (seq-difference (process-list)
                             (plist-get beacon359-tui-baseline :processes) #'eq)
             :new-timers
             (delete-dups
              (append
               (seq-difference timer-list
                               (plist-get beacon359-tui-baseline :timers) #'eq)
               (seq-difference timer-idle-list
                               (plist-get beacon359-tui-baseline :idle-timers)
                               #'eq)))
             :owned-buffers
             (mapcar #'buffer-live-p beacon359-tui-owned-buffers)
             :owned-overlays (mapcar #'overlay-buffer owned-overlays)
             :owned-timers
             (mapcar (lambda (timer)
                       (or (memq timer timer-list)
                           (memq timer timer-idle-list)))
                     owned-timers)
             :windows (equal (beacon359-tui-window-state)
                             (plist-get beacon359-tui-baseline :windows))
             :configuration
             (compare-window-configurations
              (current-window-configuration)
              (plist-get beacon359-tui-baseline :configuration))
             :buffer (eq (current-buffer)
                         (plist-get beacon359-tui-baseline :buffer))
             :window (eq (selected-window)
                         (plist-get beacon359-tui-baseline :window))
             :focus (eq (frame-focus-state)
                        (plist-get beacon359-tui-baseline :focus))
             :variables
             (cl-every
              (lambda (entry)
                (equal (beacon359-tui-variable-state (car entry)) (cdr entry)))
              (plist-get beacon359-tui-baseline :states))
             :unread (null unread-command-events)
             :minibuffer (null (active-minibuffer-window))))
      (unless (and (null errors)
                   (null (plist-get state :new-buffers))
                   (null (plist-get state :new-processes))
                   (null (plist-get state :new-timers))
                   (not (memq t (plist-get state :owned-buffers)))
                   (not (seq-some #'identity
                                  (plist-get state :owned-overlays)))
                   (not (seq-some #'identity
                                  (plist-get state :owned-timers)))
                   (plist-get state :windows)
                   (plist-get state :configuration)
                   (plist-get state :buffer) (plist-get state :window)
                   (plist-get state :focus)
                   (plist-get state :variables) (plist-get state :unread)
                   (plist-get state :minibuffer))
        (error "Beacon TUI cleanup failure: errors=%S state=%S" errors state))
      (message "B359-CLEAN ok=t errors=nil resources=nil windows=t variables=t"))))
"####;
