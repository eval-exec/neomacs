use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use expect_test::{Expect, expect};
use neomacs_tui_tests::{RawTerminalSnapshot, TuiSession};

use crate::{BEACON_MELPA_PIN, CachedMelpaOracle};

use super::support::{DisplayEnvOverride, PackageTuiPair};

const BEACON_TUI_PRELUDE: &str = r####"
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

fn wait_for(
    session: &mut TuiSession,
    timeout: Duration,
    description: &str,
    predicate: impl Fn(&[String]) -> bool,
) {
    session.read_until(timeout, |grid| predicate(grid));
    let grid = session.text_grid();
    assert!(
        predicate(&grid),
        "{} timed out waiting for {description}:\n{}",
        session.name,
        grid.join("\n")
    );
}

fn invoke(session: &mut TuiSession, command: &str, ready: &str) {
    session.send_keys("M-x");
    wait_for(session, Duration::from_secs(8), "M-x prompt", |grid| {
        grid.iter().any(|row| row.contains("M-x"))
    });
    session.send(command.as_bytes());
    session.send_keys("RET");
    wait_for(session, Duration::from_secs(20), ready, |grid| {
        grid.iter().any(|row| row.contains(ready))
    });
}

fn panic_text(payload: Box<dyn Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|value| (*value).to_owned())
        })
        .unwrap_or_else(|| "non-string panic payload".to_owned())
}

fn catch_phase<T>(label: &str, phase: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(phase))
        .map_err(|payload| format!("{label}: {}", panic_text(payload)))
}

fn both(
    pair: &mut PackageTuiPair,
    label: &str,
    operation: impl Fn(&mut TuiSession) + Copy,
) -> Result<(), String> {
    let gnu = catch_phase(&format!("GNU {label}"), || operation(&mut pair.gnu));
    let neo = catch_phase(&format!("Neo {label}"), || operation(&mut pair.neo));
    let errors = [gnu.err(), neo.err()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn exact_row(session: &TuiSession, marker: &str) -> String {
    session
        .text_grid()
        .into_iter()
        .find(|row| row.contains(marker))
        .unwrap_or_else(|| panic!("{} did not render {marker:?}", session.name))
        .trim()
        .to_owned()
}

fn push_rows(pair: &PackageTuiPair, marker: &str, gnu: &mut Vec<String>, neo: &mut Vec<String>) {
    gnu.push(exact_row(&pair.gnu, marker));
    neo.push(exact_row(&pair.neo, marker));
}

fn record_pair(
    pair: &PackageTuiPair,
    label: &str,
    marker: &str,
    expected: Expect,
    mismatches: &mut Vec<String>,
) {
    let gnu = exact_row(&pair.gnu, marker);
    let neo = exact_row(&pair.neo, marker);
    if neo != gnu {
        mismatches.push(format!("{label} differs\nGNU: {gnu}\nNeo: {neo}"));
    }
    expected.assert_eq(&gnu);
}

fn styled_span(session: &TuiSession, needle: &str, offset: usize, width: usize) -> String {
    let grid = session.text_grid();
    let row = grid
        .iter()
        .position(|contents| contents.contains(needle))
        .unwrap_or_else(|| {
            panic!(
                "{} did not render {needle:?}\n{}",
                session.name,
                grid.join("\n")
            )
        }) as u16;
    let column = grid[row as usize]
        .find(needle)
        .expect("located needle row must retain the needle")
        + offset;
    let mut snapshot = RawTerminalSnapshot::capture_rows(session.screen(), row..row + 1);
    snapshot.rows[0].cells = snapshot.rows[0].cells[column..column + width].to_vec();
    snapshot.ansi_grid()
}

fn repeated_styled_spans(session: &TuiSession, needle: &str, width: usize) -> String {
    let grid = session.text_grid();
    let row = grid
        .iter()
        .position(|contents| contents.matches(needle).count() == 2)
        .unwrap_or_else(|| {
            panic!(
                "{} did not render two occurrences of {needle:?}\n{}",
                session.name,
                grid.join("\n")
            )
        }) as u16;
    let columns = grid[row as usize]
        .match_indices(needle)
        .map(|(column, _)| column)
        .collect::<Vec<_>>();
    assert_eq!(
        columns.len(),
        2,
        "expected exactly two rendered occurrences"
    );
    let snapshot = RawTerminalSnapshot::capture_rows(session.screen(), row..row + 1);
    columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let mut occurrence = snapshot.clone();
            occurrence.rows[0].cells = occurrence.rows[0].cells[*column..*column + width].to_vec();
            format!(
                "B359-WINDOW-CELLS-{} {}",
                index + 1,
                occurrence.ansi_grid().trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn record_style(
    pair: &PackageTuiPair,
    label: &str,
    needle: &str,
    offset: usize,
    width: usize,
    expected: Expect,
    mismatches: &mut Vec<String>,
) {
    let gnu = styled_span(&pair.gnu, needle, offset, width);
    let neo = styled_span(&pair.neo, needle, offset, width);
    record_captured_style(label, gnu, neo, expected, mismatches);
}

fn record_captured_style(
    label: &str,
    gnu: String,
    neo: String,
    expected: Expect,
    mismatches: &mut Vec<String>,
) {
    if neo != gnu {
        mismatches.push(format!("{label} style differs\nGNU: {gnu:?}\nNeo: {neo:?}"));
    }
    expected.assert_eq(&gnu);
}

fn emitted_truecolor_cells(session: &TuiSession) -> Vec<(u16, u16, u16, char)> {
    let output = session.recent_output();
    let mut cells = Vec::new();
    let mut cursor = 0;
    while cursor + 2 < output.len() {
        let Some(relative_start) = output[cursor..]
            .windows(2)
            .position(|bytes| bytes == b"\x1b[")
        else {
            break;
        };
        let start = cursor + relative_start + 2;
        let Some(relative_end) = output[start..]
            .iter()
            .position(|byte| (0x40..=0x7e).contains(byte))
        else {
            break;
        };
        let end = start + relative_end;
        if output[end] != b'm' {
            cursor = end + 1;
            continue;
        }
        let parameters = String::from_utf8_lossy(&output[start..end])
            .split(';')
            .filter_map(|value| value.parse::<u16>().ok())
            .collect::<Vec<_>>();
        if let Some(index) = parameters
            .windows(2)
            .position(|parameters| parameters == [48, 2])
            && index + 4 < parameters.len()
            && end + 1 < output.len()
            && output[end + 1].is_ascii_graphic()
        {
            cells.push((
                parameters[index + 2],
                parameters[index + 3],
                parameters[index + 4],
                output[end + 1] as char,
            ));
        }
        cursor = end + 1;
    }
    cells
}

fn emitted_styled_span(session: &TuiSession, text: &str) -> String {
    let expected_chars = text.chars().collect::<Vec<_>>();
    let cells = emitted_truecolor_cells(session);
    let observed = cells
        .windows(expected_chars.len())
        .find(|window| {
            window
                .iter()
                .map(|(_, _, _, character)| *character)
                .eq(expected_chars.iter().copied())
        })
        .unwrap_or_else(|| {
            panic!(
                "{} did not emit a truecolor span for {text:?}: {cells:?}",
                session.name
            )
        });
    let mut rendered = String::new();
    for (red, green, blue, character) in observed {
        rendered.push_str(&format!("\x1b[0;48;2;{red};{green};{blue}m{character}"));
    }
    rendered.push_str("\x1b[0m\n");
    rendered
}

fn cell_span_background_state(
    session: &TuiSession,
    needle: &str,
    offset: usize,
    width: usize,
) -> Option<bool> {
    let grid = session.text_grid();
    let row = grid.iter().position(|contents| contents.contains(needle))?;
    let column = grid[row].find(needle).map(|start| start + offset)?;
    let control_background = session
        .screen()
        .cell(row as u16, (column + width + 1) as u16)
        .expect("Beacon styled-span control cell must exist")
        .bgcolor();
    Some((column..column + width).any(|col| {
        session
            .screen()
            .cell(row as u16, col as u16)
            .is_some_and(|cell| cell.bgcolor() != control_background)
    }))
}

fn wait_for_pair_background(
    pair: &mut PackageTuiPair,
    needle: &str,
    offset: usize,
    width: usize,
    present: bool,
) -> Option<(String, String)> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let target = Some(present);
    let mut gnu_seen = cell_span_background_state(&pair.gnu, needle, offset, width) == target;
    let mut neo_seen = cell_span_background_state(&pair.neo, needle, offset, width) == target;
    let mut gnu_style = gnu_seen.then(|| styled_span(&pair.gnu, needle, offset, width));
    let mut neo_style = neo_seen.then(|| styled_span(&pair.neo, needle, offset, width));
    while Instant::now() < deadline {
        if gnu_seen && neo_seen {
            return present.then(|| {
                (
                    gnu_style.expect("present GNU background must capture its raw style"),
                    neo_style.expect("present Neo background must capture its raw style"),
                )
            });
        }
        if !gnu_seen {
            pair.gnu.read(Duration::from_millis(60));
            gnu_seen = cell_span_background_state(&pair.gnu, needle, offset, width) == target;
            if present && gnu_seen {
                gnu_style = Some(styled_span(&pair.gnu, needle, offset, width));
            }
        }
        if !neo_seen {
            pair.neo.read(Duration::from_millis(60));
            neo_seen = cell_span_background_state(&pair.neo, needle, offset, width) == target;
            if present && neo_seen {
                neo_style = Some(styled_span(&pair.neo, needle, offset, width));
            }
        }
    }
    panic!(
        "Beacon peers never reached background present={present}; GNU={:?} Neo={:?}\nGNU grid:\n{}\nNeo grid:\n{}\nGNU output: {:?}\nNeo output: {:?}",
        cell_span_background_state(&pair.gnu, needle, offset, width),
        cell_span_background_state(&pair.neo, needle, offset, width),
        pair.gnu.text_grid().join("\n"),
        pair.neo.text_grid().join("\n"),
        String::from_utf8_lossy(pair.gnu.recent_output()),
        String::from_utf8_lossy(pair.neo.recent_output())
    );
}

fn wait_for_pair_marker(pair: &mut PackageTuiPair, marker: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        let gnu = pair.gnu.text_grid().iter().any(|row| row.contains(marker));
        let neo = pair.neo.text_grid().iter().any(|row| row.contains(marker));
        if gnu && neo {
            return;
        }
        pair.gnu.read(Duration::from_millis(60));
        pair.neo.read(Duration::from_millis(60));
    }
    panic!(
        "Beacon peers never rendered {marker:?}; GNU:\n{}\nNeo:\n{}",
        pair.gnu.text_grid().join("\n"),
        pair.neo.text_grid().join("\n")
    );
}

fn run_body(pair: &mut PackageTuiPair, mismatches: &mut Vec<String>) -> Result<(), String> {
    both(pair, "setup", |session| {
        invoke(session, "beacon359-tui-setup", "B359-SETUP")
    })?;
    record_pair(
        pair,
        "setup",
        "B359-SETUP",
        expect!["B359-SETUP cells=16777216 class=static-color defaults=t lighter=t focus=nil"],
        mismatches,
    );
    both(pair, "source provenance", |session| {
        invoke(session, "beacon359-tui-show-source", "B359-SOURCE")
    })?;
    record_pair(
        pair,
        "source provenance",
        "B359-SOURCE",
        expect![[r#"B359-SOURCE subject=beacon.el seq=t compile=compile.elc suffix=(".el")"#]],
        mismatches,
    );

    both(pair, "manual blink", |session| {
        invoke(session, "beacon359-tui-manual", "B359-MANUAL")
    })?;
    record_pair(
        pair,
        "manual",
        "B359-MANUAL",
        expect!["B359-MANUAL p=8 l=1 s=1 b=source o=7 t=t w=t n=1"],
        mismatches,
    );
    record_style(
        pair,
        "manual",
        "manual alpha",
        7,
        7,
        expect![[r#"
            [0;48;2;0;255;255ma[0;48;2;28;252;252ml[0;48;2;57;249;249mp[0;48;2;86;246;246mh[0;48;2;114;242;242ma[0;48;2;143;239;239m [0;48;2;172;236;236mb[0m
        "#]],
        mismatches,
    );

    both(pair, "EOL blink", |session| {
        invoke(session, "beacon359-tui-eol", "B359-EOL")
    })?;
    record_pair(
        pair,
        "EOL",
        "B359-EOL",
        expect!["B359-EOL p=41 l=2 s=1 b=source o=1 t=t w=t n=1"],
        mismatches,
    );
    record_style(
        pair,
        "EOL",
        "manual-eol",
        10,
        7,
        expect![[r#"
            [0;48;2;0;255;255m [0;48;2;28;252;252m [0;48;2;57;249;249m [0;48;2;86;246;246m [0;48;2;114;242;242m [0;48;2;143;239;239m [0;48;2;172;236;236m [0m
        "#]],
        mismatches,
    );

    both(pair, "natural timer start", |session| {
        invoke(session, "beacon359-tui-natural", "B359-NATURAL-START")
    })?;
    let _ = wait_for_pair_background(pair, "manual alpha", 7, 7, true);
    let _ = wait_for_pair_background(pair, "manual alpha", 7, 7, false);
    for session in [&mut pair.gnu, &mut pair.neo] {
        wait_for(
            session,
            Duration::from_secs(3),
            "natural timer terminal state",
            |grid| grid.iter().any(|row| row.contains("B359-NATURAL-DONE")),
        );
    }
    record_pair(
        pair,
        "natural timer",
        "B359-NATURAL-DONE",
        expect!["B359-NATURAL-DONE ovs=0 listed=nil timerp=t"],
        mismatches,
    );

    both(pair, "scroll setup", |session| {
        invoke(session, "beacon359-tui-prepare-scroll", "B359-SCROLL-READY")
    })?;
    both(pair, "real C-v", |session| {
        session.send_keys("C-v");
        wait_for(
            session,
            Duration::from_secs(8),
            "scroll redisplay observation",
            |grid| grid.iter().any(|row| row.contains("B359-SCROLL-AFTER ")),
        );
    })?;
    record_pair(
        pair,
        "scroll after redisplay",
        "B359-SCROLL-AFTER ",
        expect!["B359-SCROLL-AFTER p=761 l=20 s=20 b=scroll o=7 t=t w=t n=1"],
        mismatches,
    );
    record_style(
        pair,
        "scroll",
        "row 19 |",
        0,
        7,
        expect![[r#"
        [0;48;2;0;255;255mr[0;48;2;28;252;252mo[0;48;2;57;249;249mw[0;48;2;86;246;246m [0;48;2;114;242;242m1[0;48;2;143;239;239m9[0;48;2;172;236;236m [0m
    "#]],
        mismatches,
    );

    both(pair, "window setup", |session| {
        invoke(
            session,
            "beacon359-tui-prepare-windows",
            "B359-WINDOW-READY",
        )
    })?;
    both(pair, "real C-x o", |session| {
        session.send_keys("C-x o");
        wait_for(session, Duration::from_secs(8), "B359-WINDOW", |grid| {
            grid.iter().any(|row| row.contains("B359-WINDOW "))
        });
    })?;
    record_pair(
        pair,
        "window change",
        "B359-WINDOW ",
        expect!["B359-WINDOW p=1 l=1 s=1 b=source o=7 t=t w=t n=1"],
        mismatches,
    );
    let gnu_window = repeated_styled_spans(&pair.gnu, "manual alpha", 7);
    let neo_window = repeated_styled_spans(&pair.neo, "manual alpha", 7);
    if neo_window != gnu_window {
        mismatches.push(format!(
            "window-scoped rendering differs\nGNU:\n{gnu_window}\nNeo:\n{neo_window}"
        ));
    }
    expect![[r#"
        B359-WINDOW-CELLS-1 manual [0m
        B359-WINDOW-CELLS-2 [0;48;2;0;255;255mm[0;48;2;28;252;252ma[0;48;2;57;249;249mn[0;48;2;86;246;246mu[0;48;2;114;242;242ma[0;48;2;143;239;239ml[0;48;2;172;236;236m [0m"#]].assert_eq(&gnu_window);

    both(pair, "real buffer switch", |session| {
        session.send_keys("C-x b");
        wait_for(
            session,
            Duration::from_secs(8),
            "switch-to-buffer prompt",
            |grid| grid.iter().any(|row| row.contains("Switch to buffer")),
        );
        session.send_keys("C-q");
        session.send(b" ");
        session.send(b"*beacon359-other*");
        session.send_keys("RET");
        wait_for(session, Duration::from_secs(8), "B359-BUFFER", |grid| {
            grid.iter().any(|row| row.contains("B359-BUFFER "))
        });
    })?;
    record_pair(
        pair,
        "buffer switch",
        "B359-BUFFER ",
        expect!["B359-BUFFER p=37 l=2 s=1 b=other o=1 t=t w=t n=1"],
        mismatches,
    );

    both(pair, "next-line setup", |session| {
        invoke(session, "beacon359-tui-prepare-next", "B359-NEXT-READY")
    })?;
    record_pair(
        pair,
        "default command suppression prepared",
        "B359-NEXT-READY",
        expect!["B359-NEXT-READY p=861 l=24 s=4"],
        mismatches,
    );
    both(pair, "suppressed next line", |session| {
        session.send_keys("C-n");
        wait_for(
            session,
            Duration::from_secs(8),
            "suppressed next-line redisplay observation",
            |grid| grid.iter().any(|row| row.contains("B359-NEXT-AFTER ")),
        );
    })?;
    record_pair(
        pair,
        "default command suppression after redisplay",
        "B359-NEXT-AFTER ",
        expect!["B359-NEXT-AFTER p=901 l=25 before=4 after=15 delta=11 o=0 t=nil n=0"],
        mismatches,
    );

    let mut gnu_suppression = Vec::new();
    let mut neo_suppression = Vec::new();
    for (label, command) in [
        ("predicate", "beacon359-tui-configure-suppression"),
        ("major", "beacon359-tui-configure-major"),
        ("command", "beacon359-tui-configure-command"),
        ("local", "beacon359-tui-configure-local"),
        ("compilation", "beacon359-tui-configure-compilation"),
    ] {
        both(pair, label, |session| {
            invoke(session, command, "B359-SUPPRESS-READY")
        })?;
        push_rows(
            pair,
            "B359-SUPPRESS-READY",
            &mut gnu_suppression,
            &mut neo_suppression,
        );
        both(pair, &format!("{label} suppressed jump"), |session| {
            session.send_keys("C-c j");
            wait_for(session, Duration::from_secs(8), "B359-JUMP", |grid| {
                grid.iter().any(|row| row.contains("B359-JUMP "))
            });
        })?;
        push_rows(
            pair,
            "B359-JUMP ",
            &mut gnu_suppression,
            &mut neo_suppression,
        );
        both(pair, &format!("{label} recovery setup"), |session| {
            invoke(session, "beacon359-tui-recover", "B359-RECOVER-READY")
        })?;
        push_rows(
            pair,
            "B359-RECOVER-READY",
            &mut gnu_suppression,
            &mut neo_suppression,
        );
        both(pair, &format!("{label} recovery jump"), |session| {
            session.send_keys("C-c j");
            wait_for(session, Duration::from_secs(8), "B359-JUMP", |grid| {
                grid.iter().any(|row| row.contains("B359-JUMP "))
            });
        })?;
        push_rows(
            pair,
            "B359-JUMP ",
            &mut gnu_suppression,
            &mut neo_suppression,
        );
    }
    let gnu_suppression = gnu_suppression.join("\n");
    let neo_suppression = neo_suppression.join("\n");
    if neo_suppression != gnu_suppression {
        mismatches.push(format!(
            "suppression matrix differs\nGNU:\n{gnu_suppression}\nNeo:\n{neo_suppression}"
        ));
    }
    expect![[r#"
        B359-SUPPRESS-READY kind=predicate
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=0 t=nil n=0
        B359-RECOVER-READY mode=t local=nil
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=7 t=t n=1
        B359-SUPPRESS-READY kind=major
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=0 t=nil n=0
        B359-RECOVER-READY mode=t local=nil
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=7 t=t n=1
        B359-SUPPRESS-READY kind=command
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=0 t=nil n=0
        B359-RECOVER-READY mode=t local=nil
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=7 t=t n=1
        B359-SUPPRESS-READY kind=local mode=nil global-hook=1
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=0 t=nil n=0
        B359-RECOVER-READY mode=t local=nil
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=7 t=t n=1
        B359-SUPPRESS-READY kind=compilation mode=compilation-mode defaults=t
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=0 t=nil n=0
        B359-RECOVER-READY mode=t local=nil
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=7 t=t n=1"#]]
    .assert_eq(&gnu_suppression);

    both(pair, "mark setup", |session| {
        invoke(session, "beacon359-tui-prepare-mark", "B359-MARK-READY")
    })?;
    both(pair, "mark jump", |session| {
        session.send_keys("C-c j");
        wait_for(session, Duration::from_secs(8), "mark jump", |grid| {
            grid.iter().any(|row| row.contains("B359-JUMP "))
        });
    })?;
    record_pair(
        pair,
        "mark push",
        "B359-JUMP ",
        expect!["B359-JUMP p=141 l=6 m=1 a=nil r=(3) o=0 t=nil n=0"],
        mismatches,
    );

    both(pair, "mark/blink setup", |session| {
        invoke(
            session,
            "beacon359-tui-prepare-mark-blink",
            "B359-MARK-READY",
        )
    })?;
    both(pair, "mark/blink jump", |session| {
        session.send_keys("C-c j");
        wait_for(session, Duration::from_secs(8), "mark/blink jump", |grid| {
            grid.iter().any(|row| row.contains("B359-JUMP "))
        });
    })?;
    record_pair(
        pair,
        "mark/blink applied",
        "B359-JUMP ",
        expect!["B359-JUMP p=141 l=6 m=nil a=nil r=nil o=7 t=t n=1"],
        mismatches,
    );
    record_style(
        pair,
        "mark/blink applied",
        "row 02 |",
        0,
        7,
        expect![[r#"
            [0;48;2;255;0;255mr[0;48;2;252;28;252mo[0;48;2;249;57;249mw[0;48;2;246;86;246m [0;48;2;242;114;242m0[0;48;2;239;143;239m2[0;48;2;236;172;236m [0m
        "#]],
        mismatches,
    );

    both(pair, "active mark setup", |session| {
        invoke(
            session,
            "beacon359-tui-prepare-active-mark",
            "B359-MARK-READY",
        )
    })?;
    both(pair, "active mark jump", |session| {
        session.send_keys("C-c j");
        wait_for(
            session,
            Duration::from_secs(8),
            "active mark jump",
            |grid| grid.iter().any(|row| row.contains("B359-JUMP ")),
        );
    })?;
    record_pair(
        pair,
        "active mark preserved",
        "B359-JUMP ",
        expect!["B359-JUMP p=141 l=6 m=3 a=t r=nil o=0 t=nil n=0"],
        mismatches,
    );

    let mut gnu_horizontal = Vec::new();
    let mut neo_horizontal = Vec::new();
    for (label, command) in [
        ("horizontal alone", "beacon359-tui-prepare-horizontal-alone"),
        (
            "horizontal coupled",
            "beacon359-tui-prepare-horizontal-coupled",
        ),
    ] {
        both(pair, label, |session| {
            invoke(session, command, "B359-HORIZONTAL-READY")
        })?;
        push_rows(
            pair,
            "B359-HORIZONTAL-READY",
            &mut gnu_horizontal,
            &mut neo_horizontal,
        );
        both(pair, label, |session| {
            session.send_keys("C-c h");
            wait_for(session, Duration::from_secs(8), "B359-HORIZONTAL", |grid| {
                grid.iter().any(|row| row.contains("B359-HORIZONTAL "))
            });
        })?;
        push_rows(
            pair,
            "B359-HORIZONTAL ",
            &mut gnu_horizontal,
            &mut neo_horizontal,
        );
    }
    let gnu_horizontal = gnu_horizontal.join("\n");
    let neo_horizontal = neo_horizontal.join("\n");
    if neo_horizontal != gnu_horizontal {
        mismatches.push(format!(
            "horizontal coupling differs\nGNU:\n{gnu_horizontal}\nNeo:\n{neo_horizontal}"
        ));
    }
    expect![[r#"
        B359-HORIZONTAL-READY vertical=nil p=42 col=0
        B359-HORIZONTAL p=46 l=3 col=12 v=nil h=5 o=0 t=nil n=0
        B359-HORIZONTAL-READY vertical=1 p=42 col=0
        B359-HORIZONTAL p=46 l=3 col=12 v=1 h=5 o=2 t=t n=1"#]]
    .assert_eq(&gnu_horizontal);

    both(pair, "focus setup", |session| {
        invoke(session, "beacon359-tui-arm-focus", "B359-FOCUS-READY")
    })?;
    both(pair, "focus in input", |session| session.send(b"\x1b[I"))?;
    wait_for_pair_marker(pair, "B359-FOCUS-APPLIED n=1");
    record_pair(
        pair,
        "focus in state",
        "B359-FOCUS-APPLIED n=1",
        expect!["B359-FOCUS-APPLIED n=1 s=t r=1-2/2-3/3-4 b=t f=t w=t t=t"],
        mismatches,
    );
    let (gnu_focus_in, neo_focus_in) = wait_for_pair_background(pair, "manual alpha", 0, 3, true)
        .expect("focus-in must capture a real styled frame in both peers");
    record_captured_style(
        "focus in",
        gnu_focus_in,
        neo_focus_in,
        expect![[r#"
            [0;48;2;255;0;0mm[0;48;2;249;57;57ma[0;48;2;242;114;114mn[0m
        "#]],
        mismatches,
    );
    let _ = wait_for_pair_background(pair, "manual alpha", 0, 3, false);
    pair.gnu.clear_recent_output();
    pair.neo.clear_recent_output();
    both(pair, "focus out input", |session| session.send(b"\x1b[O"))?;
    wait_for_pair_marker(pair, "B359-FOCUS-APPLIED n=2");
    record_pair(
        pair,
        "focus out state",
        "B359-FOCUS-APPLIED n=2",
        expect!["B359-FOCUS-APPLIED n=2 s=nil r=1-2/2-3/3-4 b=t f=t w=t t=t"],
        mismatches,
    );
    let gnu_focus_out = emitted_styled_span(&pair.gnu, "man");
    let neo_focus_out = emitted_styled_span(&pair.neo, "man");
    record_captured_style(
        "focus out",
        gnu_focus_out,
        neo_focus_out,
        expect![[r#"
            [0;48;2;255;0;0mm[0;48;2;249;57;57ma[0;48;2;242;114;114mn[0m
        "#]],
        mismatches,
    );
    both(pair, "focus report", |session| {
        invoke(session, "beacon359-tui-focus-report", "B359-FOCUS-REPORT")
    })?;
    record_pair(
        pair,
        "focus",
        "B359-FOCUS-REPORT",
        expect!["B359-FOCUS-REPORT n=2 state=nil timerp=t listed=nil"],
        mismatches,
    );
    Ok(())
}

fn run_cleanup(pair: &mut PackageTuiPair, mismatches: &mut Vec<String>) -> Result<(), String> {
    let mut errors = Vec::new();
    if let Err(error) = both(pair, "real focus restore", |session| {
        session.send(b"\x1b[O");
        session.read(Duration::from_millis(300));
    }) {
        errors.push(error);
    }
    if let Err(error) = both(pair, "cleanup", |session| {
        invoke(session, "beacon359-tui-cleanup", "B359-CLEAN")
    }) {
        errors.push(error);
    } else {
        record_pair(
            pair,
            "cleanup",
            "B359-CLEAN",
            expect!["B359-CLEAN ok=t errors=nil resources=nil windows=t variables=t"],
            mismatches,
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

#[test]
fn beacon_real_truecolor_automated_display_and_timer_lifecycle_match_gnu() {
    let oracle = CachedMelpaOracle::new(BEACON_MELPA_PIN, "beacon.el")
        .expect("prepare exact shallow Beacon source below ./tmp")
        .with_prelude(BEACON_TUI_PRELUDE);
    let mut pair = PackageTuiPair::spawn_with_display_env(
        "beacon-real-display",
        oracle.prepared_packages(),
        &[DisplayEnvOverride::Set {
            key: "COLORTERM",
            value: "truecolor",
        }],
    )
    .expect("spawn real truecolor Beacon PTY pair");
    both(&mut pair, "real terminal profile prime", |session| {
        session.resize(24, 80);
        session.send(b"\x1b[O");
        session.read(Duration::from_millis(500));
    })
    .expect("establish the exact real 80x24 unfocused terminal baseline");

    let mut mismatches = Vec::new();
    let body = catch_phase("Beacon TUI body", || run_body(&mut pair, &mut mismatches))
        .and_then(|result| result);
    let cleanup = catch_phase("Beacon TUI cleanup", || {
        run_cleanup(&mut pair, &mut mismatches)
    })
    .and_then(|result| result);
    let mut errors = Vec::new();
    if let Err(error) = body {
        errors.push(error);
    }
    if let Err(error) = cleanup {
        errors.push(error);
    }
    errors.extend(mismatches);
    assert!(errors.is_empty(), "{}", errors.join("\n\n"));
}
