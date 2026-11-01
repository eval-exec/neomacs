;;; mouse-position-in-root-frame-gui-oracle.el --- GNU/Neomacs GUI oracle -*- lexical-binding: t -*-
;;
;; One result line written to NEOMACS_MOUSE_ROOT_ORACLE_RESULT.  The test runs
;; this fixture under both editors on one X11 display and compares the files.
;;
;; The contract pinned here, from GNU 31.1: Fmouse_position_in_root_frame
;; (src/frame.c:7010-7039) takes mouse-position (after mouse-position-function)
;; and adds the mouse frame's and each ancestor frame's left_pos/top_pos.  The
;; reference build reads nil coordinates as zero and answers nil for a
;; non-frame value.  mouse-drag-line uses this primitive on a tty
;; (lisp/mouse.el:862-871), so answering nil left tty window drags unable to
;; resize (issue #660).

(let ((frame (selected-frame))
      (path (getenv "NEOMACS_MOUSE_ROOT_ORACLE_RESULT")))
  (set-frame-position frame 3 4)
  (run-with-timer
   1.0 nil
   (lambda ()
     (let ((result
            (list :frame-position (frame-position frame)
                  :known (let ((mouse-position-function
                                (lambda (_pos) (cons frame (cons 19 45)))))
                           (mouse-position-in-root-frame))
                  :unknown (let ((mouse-position-function
                                  (lambda (_pos) (cons frame (cons nil nil)))))
                             (mouse-position-in-root-frame))
                  :non-frame (let ((mouse-position-function
                                    (lambda (_pos) (cons 42 (cons 1 2)))))
                               (mouse-position-in-root-frame)))))
       (with-temp-file path
         (let ((print-length nil) (print-level nil))
           (prin1 result (current-buffer))))
       (kill-emacs 0)))))
