;;; neomacs-image-tests.el --- Computed image playback tests -*- lexical-binding: t -*-

(require 'cl-lib)
(require 'cl-extra)
(require 'cl-seq)
(require 'cl-macs)
(require 'ert)
(require 'image)
(require 'neomacs-image)

(defun neomacs-image-tests--scheduled (frame metadata limit &optional speed)
  "Return the next frame and delay scheduled after FRAME, or nil."
  (let ((image (list 'image :type 'svg :index 0
                     :animation (and (plist-member metadata 'loop-start) t)
                     :speed (or speed 1)
                     :animate-buffer (current-buffer) :animate-tardiness 0))
        scheduled)
    (cl-letf (((symbol-function 'image-metadata)
               (lambda (&rest _) metadata))
              ((symbol-function 'image-show-frame)
               (lambda (spec frame &rest _)
                 (plist-put (cdr spec) :index frame)))
              ((symbol-function 'run-with-timer)
               (lambda (delay _repeat _function &rest args)
                 (setq scheduled (list (nth 1 args) delay))))
              ((symbol-function 'time-since) (lambda (&rest _) 0)))
      (image-animate-timeout image frame 5 0 limit (float-time)))
    scheduled))

(defun neomacs-image-tests--scheduled-frame (loop-start limit)
  "Return the frame scheduled after the last frame, or nil when finished."
  (car (neomacs-image-tests--scheduled
        4 (append '(count 5 delay 0.1)
                  (when loop-start (list 'loop-start loop-start))) limit)))

(ert-deftest neomacs-image-tests-loop-preserves-introduction ()
  (should (= 2 (neomacs-image-tests--scheduled-frame 2 t))))

(ert-deftest neomacs-image-tests-legacy-loop-starts-at-zero ()
  (should (= 0 (neomacs-image-tests--scheduled-frame nil t))))

(ert-deftest neomacs-image-tests-finite-playback-stops-after-last-frame ()
  (should-not (neomacs-image-tests--scheduled-frame 2 nil)))

(ert-deftest neomacs-image-tests-delay-belongs-to-displayed-frame ()
  ;; Metadata's ordinary delay can describe the previously selected frame.
  ;; The introduction/loop boundary must use the frame displayed now.
  (let ((metadata '(count 5 delay 0.25 loop-start 2
                         intro-delay 0.25 loop-delay 0.5)))
    (should (equal '(2 0.25) (neomacs-image-tests--scheduled 1 metadata t)))
    (should (equal '(3 0.5) (neomacs-image-tests--scheduled 2 metadata t)))
    (should (equal '(2 0.5) (neomacs-image-tests--scheduled 4 metadata t)))))

(ert-deftest neomacs-image-tests-reverse-loop-does-not-replay-introduction ()
  (should (equal '(4 0.5)
                 (neomacs-image-tests--scheduled
                  2 '(count 5 delay 0.25 loop-start 2
                            intro-delay 0.25 loop-delay 0.5)
                  t -1))))

(ert-deftest neomacs-image-tests-segment-delay-can-use-default ()
  (let ((metadata '(count 5 delay t loop-start 2 intro-delay t loop-delay t)))
    (should (equal (list 2 image-default-frame-delay)
                   (neomacs-image-tests--scheduled 1 metadata t)))
    (should (equal (list 3 image-default-frame-delay)
                   (neomacs-image-tests--scheduled 2 metadata t)))))

(ert-deftest neomacs-image-tests-svg-helper-mutates-displayed-spec-in-place ()
  (let ((neomacs-svg-animation 30)
        (image (list 'image :type 'svg :index 0))
        (opt-out (list 'image :type 'svg :animation nil :index 0))
        observed)
    (cl-letf (((symbol-function 'image-animate)
               (lambda (spec index limit)
                 (setq observed (list spec index limit))
                 (plist-put (cdr spec) :index 1)
                 'started)))
      (should (eq 'started (neomacs-image-animate-svg image t)))
      (should (eq image (car observed)))
      (should (equal '(nil t) (cdr observed)))
      (should (= 30 (plist-get (cdr image) :animation)))
      (should (= 1 (image-current-frame image)))
      (neomacs-image-animate-svg opt-out)
      (should (eq opt-out (car observed)))
      (should-not (plist-get (cdr opt-out) :animation)))))

(provide 'neomacs-image-tests)
;;; neomacs-image-tests.el ends here
