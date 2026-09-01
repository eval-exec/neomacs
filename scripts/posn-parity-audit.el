;;; posn-parity-audit.el --- posn-at-point parity sweep -*- lexical-binding: t -*-

;; Ledger 201.  Focused successor to scripts/motion-parity-audit.el (ledger
;; 195), which found two `posn-at-point' rows while using it as a CONTROL and
;; handed them over rather than widening its scope.  This sweep asks only about
;; the layout query, and asks three questions per probe instead of one so that
;; the two rows can be told apart:
;;
;;   posn   (posn-at-point)                -- the divergent call
;;   pvw    (pos-visible-in-window-p P nil t) -- GNU BUILDS posn-at-point out
;;          of exactly this call (src/keyboard.c:13073).  If this answers and
;;          `posn' does not, the defect is in the composition, not in the
;;          geometry.
;;   pvwp   (pos-visible-in-window-p P)    -- the plain predicate.
;;
;; PROTOCOL, AND WHY IT CAN SEE THE DEFECT.  This port answers `posn-at-point'
;; on a TTY frame from a RETAINED REDISPLAY SNAPSHOT
;; (crates/neovm-core/src/emacs_core/display/xdisp/mod.rs:5633-5640). A protocol that redisplays
;; before the probe POPULATES that snapshot and therefore cannot see
;; "answered from an unpopulated matrix" at all -- it is a false green for the
;; very defect being measured (ledger 195 s6's rule, restated).  So:
;;
;;   L201_REDISPLAY=0  COLD  -- no redisplay anywhere.  Exposes row 1.
;;   L201_REDISPLAY=1  WARM  -- (redisplay t) before every probe.  Row 1 is
;;                             INVISIBLE here by construction; row 2 survives.
;;   L201_REDISPLAY=N  N redisplays before every probe.  A row that a SECOND
;;                     redisplay repairs is a CONVERGENCE lag, not a geometry
;;                     defect: the answer was published from the state before
;;                     the query's own scroll decision.
;;
;; Run BOTH.  GNU must answer identically under both, because GNU never reads a
;; glyph matrix for this query: `pos_visible_p' calls `start_display' on
;; `w->start' and `move_it_to' (src/xdisp.c:1772-1774), computing on demand.
;;
;;   emacs -nw -Q -l scripts/posn-parity-audit.el     (through the pty driver)
;;
;; Environment: L201_OUT (default ./tmp/l201/audit-out.txt), L201_REDISPLAY.

(defvar l201-out (or (getenv "L201_OUT") "./tmp/l201/audit-out.txt"))
(defvar l201-redisplays (string-to-number (or (getenv "L201_REDISPLAY") "0")))
(defun l201-redisplay ()
  (dotimes (_ l201-redisplays) (redisplay t)))

;; Byte-identical to ledger 195's `l195-text' so the numbers are comparable.
(defvar l201-text
  (concat
   "  alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron\n"
   "\twide 界 cedar birch maple spruce willow aspen oak\n"
   (make-string 100 ?x) " " (make-string 100 ?x) "\n"
   "short line\n"
   "one two three four five six seven eight nine ten eleven twelve thirteen fourteen\n"))

(defvar l201-positions '(1 5 20 43 48 60 83 90 109 133 160 200 240 260 300 330))

(defvar l201-configs
  (list
   (cons "full-wrap"
         (lambda () (setq-local truncate-lines nil) (setq-local word-wrap nil)))
   (cons "full-word-wrap"
         (lambda () (setq-local truncate-lines nil) (setq-local word-wrap t)))
   (cons "full-truncate"
         (lambda () (setq-local truncate-lines t) (setq-local word-wrap nil)))
   (cons "narrow-default-tpww"
         (lambda () (setq-local truncate-lines nil) (setq-local word-wrap nil)))
   (cons "narrow-tpww-nil"
         (lambda () (setq-local truncate-lines nil) (setq-local word-wrap nil)
           (setq-local truncate-partial-width-windows nil)))
   (cons "narrow-tpww-nil-word-wrap"
         (lambda () (setq-local truncate-lines nil) (setq-local word-wrap t)
           (setq-local truncate-partial-width-windows nil)))
   (cons "narrow-tpww-20"
         (lambda () (setq-local truncate-lines nil) (setq-local word-wrap nil)
           (setq-local truncate-partial-width-windows 20)))
   (cons "narrow-truncate"
         (lambda () (setq-local truncate-lines t) (setq-local word-wrap nil)
           (setq-local truncate-partial-width-windows nil)))
   (cons "narrow-visual-line-mode"
         (lambda () (setq-local word-wrap t) (visual-line-mode 1)))))

(defvar l201-narrow-configs
  '("narrow-default-tpww" "narrow-tpww-nil" "narrow-tpww-nil-word-wrap"
    "narrow-tpww-20" "narrow-truncate" "narrow-visual-line-mode"))

(defun l201-probe (fn)
  (condition-case err (format "%S" (funcall fn))
    (error (format "ERR:%S" (car err)))))

(defvar l201-queries
  (list
   (cons "posn"
         (lambda () (let ((p (posn-at-point)))
                      (and p (list (posn-point p) (posn-x-y p) (posn-col-row p))))))
   (cons "posn-actual"
         (lambda () (let ((p (posn-at-point))) (and p (posn-actual-col-row p)))))
   (cons "pvw"  (lambda () (pos-visible-in-window-p (point) nil t)))
   (cons "pvwp" (lambda () (pos-visible-in-window-p (point))))
   ;; The window state the three answers above were computed against.  A posn
   ;; divergence in a TRUNCATING window is only a posn divergence if both
   ;; editors hscrolled the same way; otherwise it is an automatic-hscroll
   ;; divergence wearing a posn costume.
   (cons "hscroll" (lambda () (window-hscroll)))))

(defun l201-run ()
  (let ((lines '())
        (buffer (generate-new-buffer " *l201-probe*")))
    (delete-other-windows)
    (dolist (config l201-configs)
      (let* ((name (car config))
             (setup (cdr config))
             (narrow (member name l201-narrow-configs)))
        (delete-other-windows)
        (switch-to-buffer buffer)
        (with-current-buffer buffer
          (kill-all-local-variables)
          (erase-buffer)
          (insert l201-text)
          (set-buffer-modified-p nil))
        (when narrow
          (select-window (split-window-right -24))
          (switch-to-buffer buffer))
        (with-current-buffer buffer (funcall setup))
        (push (format "CONFIG %s width=%s height=%s tl=%s ww=%s tpww=%s vlm=%s start=%s"
                      name (window-body-width) (window-body-height)
                      (buffer-local-value 'truncate-lines buffer)
                      (buffer-local-value 'word-wrap buffer)
                      (buffer-local-value 'truncate-partial-width-windows buffer)
                      (buffer-local-value 'visual-line-mode buffer)
                      (window-start))
              lines)
        (dolist (pos l201-positions)
          (dolist (query l201-queries)
            (with-current-buffer buffer
              (goto-char (min pos (point-max)))
              (l201-redisplay)
              (push (format "%s|%s|%s|%s" name pos (car query)
                            (l201-probe (cdr query)))
                    lines))))
        (delete-other-windows)))
    (with-temp-file l201-out
      (insert (mapconcat #'identity (nreverse lines) "\n") "\n"))))

(l201-run)
(kill-emacs)

;;; posn-parity-audit.el ends here
