;;; motion-parity-audit.el --- screen-line motion parity sweep -*- lexical-binding: t -*-

;; Ledger 195.  9 window/wrap/truncation configs x 16 positions x 23 motions =
;; 3312 probes, written one per line as CONFIG|POSITION|MOTION|VALUE.  Load it
;; in GNU Emacs and in this port with the SAME geometry and diff the outputs
;; with scripts/motion-parity-compare.py.
;;
;;   emacs --batch -l scripts/motion-parity-audit.el          # batch engine
;;   scripts/motion-parity-pty.py emacs -nw -Q -l ...         # display engine
;;
;; Environment:
;;   L195_OUT               output file (default ./tmp/l195/audit-out.txt)
;;   L195_REDISPLAY=0       COLD protocol: never redisplay between probes
;;   L195_FORCE_INTERACTIVE=1   bind `noninteractive' to nil for the whole sweep.
;;                          DO NOT USE IT FOR A PARITY SWEEP (ledger 210).  The
;;                          text that stood here said it "selects GNU's
;;                          DISPLAY-ITERATOR engine for every motion"; measured,
;;                          it selects nothing: GNU's --batch answers are
;;                          byte-identical with and without it.  GNU's Lisp
;;                          `noninteractive' is a COPY -- DEFVAR_BOOL
;;                          ("noninteractive", noninteractive1) at
;;                          src/emacs.c:3535, assigned once from the C flag at
;;                          src/emacs.c:1953 -- and `Fvertical_motion'
;;                          (src/indent.c:2280) branches on the C flag, which
;;                          Lisp cannot write.  This port has ONE variable, so
;;                          the binding DOES change behaviour here, which makes
;;                          this mode put DIFFERENT questions to the two
;;                          editors.  Use the pty driver instead.
;;   L195_COLS / L195_ROWS  read by scripts/motion-parity-pty.py, default 160x50
;;
;; THE GEOMETRY IS PART OF THE ANSWER, so this sweep records it (ledger 210).
;; Every probe here is a question about a window of a particular width and
;; height: the same tree answers COLD 130 / WARM 352 at 160 columns and COLD 160
;; / WARM 444 at 80, because only at 80 does this text's longest line reach the
;; window edge where the truncation marker lives.  Ledger 205 published the
;; first pair and ledger 209 the second, and the difference was read as a
;; 30-cold / 92-warm motion regression that never existed.  The first line of
;; the output is therefore a GEOMETRY stamp and every CONFIG line carries its
;; window's height as well as its width; scripts/motion-parity-compare.py
;; REFUSES to diff two files that disagree about either.
;;
;; TWO PROTOCOLS, AND THE CHOICE IS NOT COSMETIC.  WARM (the default) redisplays
;; before every probe, which is closest to a real command loop; COLD never does.
;; They do not measure the same thing in this port, because `vertical-motion'
;; answers from a retained redisplay snapshot when one is fresh and from a
;; buffer-text scanner when it is not.  A defect in the scanner is INVISIBLE
;; under WARM.  Ledger 195 §6: WARM reported 826 divergences where COLD reported
;; 1784, and WARM could not see ledger 191's own headline defect at all.  Run
;; both, and run the sensitivity check against the protocol as well as the tree.

(defvar l195-out (or (getenv "L195_OUT") "./tmp/l195/audit-out.txt"))
;; WARM (default): redisplay before every probe, so no probe inherits the
;; redisplay state the previous one left behind.  COLD: never redisplay --
;; which is the protocol that exercises the FALLBACK screen-line scanner,
;; because a live snapshot is what masks it.
(defvar l195-warm (not (equal (getenv "L195_REDISPLAY") "0")))
(defun l195-redisplay () (when l195-warm (redisplay t)))

(defvar l195-text
  (concat
   "  alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron\n"
   "\twide 界 cedar birch maple spruce willow aspen oak\n"
   (make-string 100 ?x) " " (make-string 100 ?x) "\n"
   "short line\n"
   "one two three four five six seven eight nine ten eleven twelve thirteen fourteen\n"))

;; 16 positions spread over the text (1-based, clamped at use).
(defvar l195-positions '(1 5 20 43 48 60 83 90 109 133 160 200 240 260 300 330))

;; ---------------------------------------------------------------------------
;; Configs.  Each entry is (NAME . SETUP-FUNCTION).  SETUP runs with the probe
;; buffer current and the probe window selected.
;; ---------------------------------------------------------------------------
(defvar l195-configs
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

(defvar l195-narrow-configs
  '("narrow-default-tpww" "narrow-tpww-nil" "narrow-tpww-nil-word-wrap"
    "narrow-tpww-20" "narrow-truncate" "narrow-visual-line-mode"))

;; ---------------------------------------------------------------------------
;; Motions.  Each entry is (NAME . FUNCTION); point is set before each call.
;; The last two are CONTROLS: `posn-at-point' asks the LAYOUT engine, never
;; `vertical-motion', so a control divergence means the rows themselves differ
;; (ledger 184's rule; 191 chose this control for exactly that reason).
;; ---------------------------------------------------------------------------
(defvar l195-motions
  (list
   (cons "vm0"        (lambda () (list (vertical-motion 0) (point))))
   (cons "vm1"        (lambda () (list (vertical-motion 1) (point))))
   (cons "vm2"        (lambda () (list (vertical-motion 2) (point))))
   (cons "vm3"        (lambda () (list (vertical-motion 3) (point))))
   (cons "vm-1"       (lambda () (list (vertical-motion -1) (point))))
   (cons "vm-2"       (lambda () (list (vertical-motion -2) (point))))
   (cons "vm-3"       (lambda () (list (vertical-motion -3) (point))))
   (cons "vm-big"     (lambda () (list (vertical-motion 40) (point))))
   (cons "vmc-0.0"    (lambda () (list (vertical-motion '(0 . 0)) (point))))
   (cons "vmc-5.0"    (lambda () (list (vertical-motion '(5 . 0)) (point))))
   (cons "vmc-12.0"   (lambda () (list (vertical-motion '(12 . 0)) (point))))
   (cons "vmc-40.0"   (lambda () (list (vertical-motion '(40 . 0)) (point))))
   (cons "vmc-5.1"    (lambda () (list (vertical-motion '(5 . 1)) (point))))
   (cons "vmc-5.-1"   (lambda () (list (vertical-motion '(5 . -1)) (point))))
   (cons "bovl"       (lambda () (beginning-of-visual-line) (point)))
   (cons "eovl"       (lambda () (end-of-visual-line) (point)))
   (cons "csl-min"    (lambda () (count-screen-lines (point-min) (point))))
   (cons "csl-all"    (lambda () (count-screen-lines (point-min) (point-max))))
   (cons "mtwl-0"     (lambda () (list (move-to-window-line 0) (point))))
   (cons "mtwl-1"     (lambda () (list (move-to-window-line 1) (point))))
   (cons "mtwl-nil"   (lambda () (list (move-to-window-line nil) (point))))
   ;; CONTROLS -- layout engine, not motion.
   (cons "posn-col"   (lambda () (l195-redisplay) (let ((p (posn-at-point))) (and p (posn-col-row p)))))
   (cons "posn-actual"(lambda () (l195-redisplay) (let ((p (posn-at-point))) (and p (posn-actual-col-row p)))))))

(defun l195-probe (fn)
  (condition-case err
      (format "%S" (funcall fn))
    (error (format "ERR:%S" (car err)))))

(defun l195-run ()
  (let ((lines '())
        (buffer (generate-new-buffer " *l195-probe*")))
    (delete-other-windows)
    ;; Stamp the frame first: a count published without it is not comparable
    ;; with any other run of this script (ledger 210).
    ;; `probes' is what makes a TRUNCATED or EMPTY output self-detecting: the
    ;; comparator refuses a file that does not carry the number of probes it
    ;; says it has (ledger 210).
    (push (format "GEOMETRY frame-width=%s frame-height=%s probes=%s"
                  (frame-width) (frame-height)
                  (* (length l195-configs) (length l195-positions)
                     (length l195-motions)))
          lines)
    (dolist (config l195-configs)
      (let* ((name (car config))
             (setup (cdr config))
             (narrow (member name l195-narrow-configs)))
        (delete-other-windows)
        (switch-to-buffer buffer)
        (with-current-buffer buffer
          (kill-all-local-variables)
          (erase-buffer)
          (insert l195-text)
          (set-buffer-modified-p nil))
        (when narrow
          (select-window (split-window-right -24))
          (switch-to-buffer buffer))
        (with-current-buffer buffer (funcall setup))
        ;; `height' is here because `move-to-window-line' with nil asks for the
        ;; MIDDLE row: a taller window is a different question (ledger 210).
        (push (format "CONFIG %s width=%s height=%s tl=%s ww=%s tpww=%s vlm=%s"
                      name (window-body-width) (window-body-height)
                      (buffer-local-value 'truncate-lines buffer)
                      (buffer-local-value 'word-wrap buffer)
                      (buffer-local-value 'truncate-partial-width-windows buffer)
                      (buffer-local-value 'visual-line-mode buffer))
              lines)
        (dolist (pos l195-positions)
          (dolist (motion l195-motions)
            (with-current-buffer buffer
              (goto-char (min pos (point-max)))
              ;; Uniform protocol: every probe sees a freshly displayed window
              ;; in BOTH editors, so no probe inherits the redisplay state left
              ;; behind by the previous one.
              (l195-redisplay)
              (push (format "%s|%s|%s|%s" name pos (car motion)
                            (l195-probe (cdr motion)))
                    lines))))
        (delete-other-windows)))
    (with-temp-file l195-out
      (insert (mapconcat #'identity (nreverse lines) "\n") "\n"))))

;; L195_FORCE_INTERACTIVE=1 runs the whole sweep with `noninteractive' bound to
;; nil.  Kept because ledger 191's code did exactly this, and because the
;; asymmetry it exposes is worth being able to reproduce -- but see the header:
;; in GNU the binding is INERT and in this port it is not, so it is not a way to
;; reach GNU's display-iterator engine under --batch.
(if (equal (getenv "L195_FORCE_INTERACTIVE") "1")
    (let ((noninteractive nil)) (l195-run))
  (l195-run))
(kill-emacs)

;;; motion-parity-audit.el ends here
