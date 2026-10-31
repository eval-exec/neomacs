;;; face-background-mode-gui-oracle.el --- GNU/Neomacs GUI face oracle -*- lexical-binding: t -*-
;;
;; One `(NAME . STATE)` line per case, written to the path named by
;; NEOMACS_FACE_BACKGROUND_MODE_ORACLE_RESULT. The test runs this fixture
;; under both editors on one X11 display and compares the files line by line.
;;
;; STATE is
;;   (background-mode foreground-color background-color PROBE-DUAL)
;;
;; The default face's own attributes are deliberately not recorded: on the
;; untouched opening frame Neomacs leaves them `unspecified-fg'/`unspecified-bg'
;; where GNU reports the frame colors. That is a separate lazy-realization
;; divergence, not this contract.
;;
;; The contract pinned here, from GNU 31.1:
;;
;; * When a frame is created, `face-set-after-frame-default' merges the
;;   global default face into the new frame; `internal_merge_in_global_face'
;;   publishes the default face's string foreground/background colors back
;;   through `Fmodify_frame_parameters' (src/xfaces.c:4398-4412). A frame
;;   created after the default face is customized therefore inherits those
;;   colors instead of keeping the toolkit's white/black seed.
;;
;; * Assigning `background-color' calls `frame-set-background-mode'
;;   (update_face_from_frame_parameter, src/xfaces.c:3896-3906), so defface
;;   display conditions such as `(background dark)' re-evaluate for the new
;;   frame. PROBE-DUAL records which branch won.
;;
;; * Frame parameters named explicitly at frame creation are applied after
;;   the face merge (the `face-params' loop in face-set-after-frame-default,
;;   lisp/faces.el:2293-2305), so they win over the default face's color.

(defface probe-dual
  '((((background dark)) (:foreground "yellow"))
    (t (:foreground "blue")))
  "Probe face with dark/light defface branches.")

(defvar neo-face-background-mode-oracle-results nil)

(defun neo-face-background-mode-oracle--state (frame)
  (list (frame-parameter frame 'background-mode)
        (frame-parameter frame 'foreground-color)
        (frame-parameter frame 'background-color)
        (face-attribute 'probe-dual :foreground frame t)))

(defun neo-face-background-mode-oracle--record (name thunk)
  ;; Errors propagate: a fixture that cannot reach the state under test must
  ;; fail the oracle instead of recording two identical error values that
  ;; would compare equal.
  (push (cons name (funcall thunk))
        neo-face-background-mode-oracle-results))

;; Control: the opening GUI frame uses the toolkit default (light).
(neo-face-background-mode-oracle--record
 'opening
 (lambda () (neo-face-background-mode-oracle--state (selected-frame))))

;; Changing the default face on the existing frame updates its colors and
;; background mode immediately.
(neo-face-background-mode-oracle--record
 'current-after-dark
 (lambda ()
   (set-face-attribute 'default nil :foreground "white" :background "black")
   (neo-face-background-mode-oracle--state (selected-frame))))

;; A frame created after the dark default face inherits it.
(neo-face-background-mode-oracle--record
 'new-after-dark
 (lambda ()
   (let ((frame (make-frame '((name . "probe-dark-default")))))
     (unwind-protect
         (neo-face-background-mode-oracle--state frame)
       (delete-frame frame)))))

;; The opposite direction: a light default face yields a light frame.
(neo-face-background-mode-oracle--record
 'new-after-light
 (lambda ()
   (set-face-attribute 'default nil :foreground "black" :background "white")
   (let ((frame (make-frame '((name . "probe-light-default")))))
     (unwind-protect
         (neo-face-background-mode-oracle--state frame)
       (delete-frame frame)))))

;; An explicit frame parameter is applied after the face merge and wins.
(neo-face-background-mode-oracle--record
 'new-explicit-white
 (lambda ()
   (set-face-attribute 'default nil :foreground "white" :background "black")
   (let ((frame (make-frame '((name . "probe-explicit-bg")
                              (background-color . "white")))))
     (unwind-protect
         (neo-face-background-mode-oracle--state frame)
       (delete-frame frame)))))

(let ((path (getenv "NEOMACS_FACE_BACKGROUND_MODE_ORACLE_RESULT")))
  (with-temp-file path
    (dolist (result (reverse neo-face-background-mode-oracle-results))
      (prin1 result (current-buffer))
      (terpri (current-buffer)))))

(kill-emacs 0)
