use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, POSFRAME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const POSFRAME_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const POSFRAME_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'posframe)

(when (timerp posframe-hidehandler-timer)
  (cancel-timer posframe-hidehandler-timer))
(setq posframe-hidehandler-timer nil)

(defvar posframe-test-handler-count 0)
(defvar posframe-test-last-handler-info nil)
(defvar posframe-test-initialize-count 0)

(defun posframe-test-counting-handler (info)
  (setq posframe-test-handler-count
        (1+ posframe-test-handler-count)
        posframe-test-last-handler-info info)
  (plist-get info :raw-position))

(defun posframe-test-initialize ()
  (setq posframe-test-initialize-count
        (1+ posframe-test-initialize-count)))

(defun posframe-test-properties (string property)
  (let ((position 0)
        runs)
    (while (< position (length string))
      (let* ((value (get-text-property position property string))
             (next
              (or
               (next-single-property-change
                position property string)
               (length string))))
        (when value
          (push (list position next value) runs))
        (setq position next)))
    (nreverse runs)))

(defun posframe-test-parameter-summary (parameters)
  (let ((buffer-info
         (cdr (assq 'posframe-buffer parameters))))
    (list
     :keys
     (mapcar #'car
             (delq nil (copy-sequence parameters)))
     :title (cdr (assq 'title parameters))
     :parent (cdr (assq 'parent-frame parameters))
     :buffer
     (and buffer-info
          (list (car buffer-info)
                (buffer-name (cdr buffer-info))))
     :keep-ratio (cadr (assq 'keep-ratio parameters))
     :focus (cdr (assq 'no-accept-focus parameters))
     :border
     (list
      (cdr (assq 'internal-border-width parameters))
      (cdr (assq 'child-frame-border-width parameters)))
     :fringes
     (list
      (cdr (assq 'left-fringe parameters))
      (cdr (assq 'right-fringe parameters)))
     :position
     (cons
      (cdr (assq 'left parameters))
      (cdr (assq 'top parameters)))
     :alpha (cdr (assq 'alpha parameters))
     :undecorated (cdr (assq 'undecorated parameters)))))
"##;

fn posframe_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(POSFRAME_MELPA_PIN, "posframe.el")
        .expect("prepare pinned posframe source below ./tmp")
        .with_prelude(POSFRAME_TEST_PRELUDE)
        .with_timeout(POSFRAME_TEST_TIMEOUT)
}

fn popup_placement_catalog_maps_frame_and_window_anchors_to_pixels() -> ParityBatchCase {
    let elisp_form = r##"
(let ((info
       '(:parent-frame-width 1000
         :parent-frame-height 800
         :parent-window-left 100
         :parent-window-top 50
         :parent-window-width 600
         :parent-window-height 400
         :posframe-width 240
         :posframe-height 100
         :mode-line-height 24
         :minibuffer-height 30
         :position (70 . 90)
         :x-pixel-offset 5
         :y-pixel-offset -10)))
  (mapcar
   (lambda (entry)
     (list
      (car entry)
      (funcall (cdr entry) info)))
   '((frame-center . posframe-poshandler-frame-center)
     (frame-top-center . posframe-poshandler-frame-top-center)
     (frame-bottom-center . posframe-poshandler-frame-bottom-center)
     (frame-bottom-left . posframe-poshandler-frame-bottom-left-corner)
     (frame-bottom-right . posframe-poshandler-frame-bottom-right-corner)
     (frame-other-corner
      . posframe-poshandler-frame-top-left-or-right-other-corner)
     (window-center . posframe-poshandler-window-center)
     (window-top-left . posframe-poshandler-window-top-left-corner)
     (window-top-right . posframe-poshandler-window-top-right-corner)
     (window-bottom-center . posframe-poshandler-window-bottom-center)
     (window-bottom-right . posframe-poshandler-window-bottom-right-corner)
     (absolute . posframe-poshandler-absolute-x-y))))
"##;
    let expect = expect![[
        r####"OK ((frame-center (380 . 350)) (frame-top-center (380 . 0)) (frame-bottom-center (380 . 646)) (frame-bottom-left (0 . -54)) (frame-bottom-right (-1 . -54)) (frame-other-corner (-1 . 0)) (window-center (280 . 200)) (window-top-left (100 . 50)) (window-top-right (460 . 50)) (window-bottom-center (280 . 326)) (window-bottom-right (460 . 326)) (absolute (75 . 80)))"####
    ]];
    ParityBatchCase::value(
        "popup_placement_catalog_maps_frame_and_window_anchors_to_pixels",
        elisp_form,
        expect,
    )
}

fn point_handlers_respect_glyph_offsets_viewport_clamping_and_upward_layout() -> ParityBatchCase {
    let elisp_form = r##"
(cl-letf
    (((symbol-function 'posn-at-point)
      (lambda (&rest _) 'point-position))
     ((symbol-function 'posn-x-y)
      (lambda (_position) '(900 . 100)))
     ((symbol-function 'posn-object-x-y)
      (lambda (_position) '(10 . 5)))
     ((symbol-function 'window-inside-pixel-edges)
      (lambda (_window) '(20 0 620 400)))
     ((symbol-function 'window-pixel-edges)
      (lambda (_window) '(0 30 640 430))))
  (let ((info
         '(:position 42
           :parent-window release-window
           :parent-frame-width 1000
           :parent-frame-height 800
           :parent-window-left 100
           :parent-window-top 50
           :parent-window-width 600
           :parent-window-height 400
           :posframe-width 200
           :posframe-height 120
           :font-height 18
           :header-line-height 10
           :tab-line-height 4
           :x-pixel-offset 5
           :y-pixel-offset 6)))
    (list
     :below
     (posframe-poshandler-point-bottom-left-corner info)
     :upward
     (posframe-poshandler-point-bottom-left-corner-upward info)
     :top
     (posframe-poshandler-point-top-left-corner info)
     :window-center
     (posframe-poshandler-point-window-center info)
     :frame-center
     (posframe-poshandler-point-frame-center info))))
"##;
    let expect = expect![[
        r####"OK (:below (800 . 163) :upward (800 . 25) :top (800 . 145) :window-center (300 . 163) :frame-center (400 . 163))"####
    ]];
    ParityBatchCase::value(
        "point_handlers_respect_glyph_offsets_viewport_clamping_and_upward_layout",
        elisp_form,
        expect,
    )
}

fn handler_dispatch_caches_equal_info_and_translates_reference_coordinates() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (setq posframe-test-handler-count 0
        posframe-test-last-handler-info nil
        posframe--last-poshandler-info nil
        posframe--last-posframe-pixel-position nil)
  (let* ((info
          '(:position custom
            :poshandler posframe-test-counting-handler
            :raw-position (-10 . -20)
            :ref-position (100 . 200)
            :parent-frame-width 800
            :parent-frame-height 600
         :posframe-width 100
         :posframe-height 50))
         (first (posframe-run-poshandler info))
         (_cached
          (setq posframe--last-posframe-pixel-position first))
         (second (posframe-run-poshandler (copy-tree info)))
         (changed
          (posframe-run-poshandler
           (plist-put
            (copy-tree info)
            :raw-position '(30 . 40)))))
    (list
     :positions
     (list
      (copy-tree first)
      (copy-tree second)
      (copy-tree changed))
     :handler-calls posframe-test-handler-count
     :selected
     (list
      :integer
      (eq
       (posframe--get-valid-poshandler '(:position 7))
       #'posframe-poshandler-point-bottom-left-corner)
      :coordinates
      (eq
       (posframe--get-valid-poshandler
        '(:position (8 . 9)))
       #'posframe-poshandler-absolute-x-y)
      :invalid
      (condition-case error-data
          (posframe--get-valid-poshandler
           '(:position unsupported))
        (error error-data)))
     :direct-reference
     (posframe--calculate-new-position
      '(:parent-frame-width 800
        :parent-frame-height 600
        :posframe-width 100
        :posframe-height 50)
      '(-10 . -20)
      '(100 . 200)))))
"##;
    let expect = expect![[
        r####"OK (:positions ((790 . 730) (790 . 730) (130 . 240)) :handler-calls 2 :selected (:integer t :coordinates t :invalid (error "Posframe: have no valid poshandler")) :direct-reference (790 . 730))"####
    ]];
    ParityBatchCase::value(
        "handler_dispatch_caches_equal_info_and_translates_reference_coordinates",
        elisp_form,
        expect,
    )
}

fn child_frame_creation_builds_parameters_reuses_compatible_frames_and_recreates_changed_ones()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((buffer (get-buffer-create " *posframe-create-test*"))
      made
      frame-parameters-set
      window-parameters-set
      deleted
      border-colors
      scales
      (make-count 0))
  (unwind-protect
      (cl-letf
          (((symbol-function 'display-graphic-p)
            (lambda (&optional _display) t))
           ((symbol-function 'facep)
            (lambda (face &optional _frame)
              (eq face 'child-frame-border)))
           ((symbol-function 'posframe--find-existing-posframe)
            (lambda (&rest _) nil))
           ((symbol-function 'posframe-delete-frame)
            (lambda (_buffer)
              (setq deleted (1+ (or deleted 0)))))
           ((symbol-function 'make-frame)
            (lambda (parameters)
              (setq make-count (1+ make-count))
              (push
               (posframe-test-parameter-summary parameters)
               made)
              (intern
               (format "posframe-frame-%d" make-count))))
           ((symbol-function 'frame-live-p)
            (lambda (_frame) t))
           ((symbol-function 'set-frame-parameter)
            (lambda (_frame parameter value)
              (push (list parameter
                          (if (bufferp value)
                              (buffer-name value)
                            value))
                    frame-parameters-set)
              value))
           ((symbol-function 'frame-parameter)
            (lambda (_frame parameter)
              (and (eq parameter 'tab-bar-lines) 1)))
           ((symbol-function 'frame-root-window)
            (lambda (_frame) 'posframe-root-window))
           ((symbol-function 'set-window-parameter)
            (lambda (_window parameter value)
              (push (list parameter value)
                    window-parameters-set)))
           ((symbol-function 'set-window-buffer)
            (lambda (&rest _) nil))
           ((symbol-function 'set-window-dedicated-p)
            (lambda (&rest _) nil))
           ((symbol-function 'minibuffer-window)
            (lambda (&optional _frame) 'minibuffer-window))
           ((symbol-function 'face-attribute)
            (lambda (_face attribute &rest _)
              (if (eq attribute :font)
                  "Parent Mono"
                "parent-bg")))
           ((symbol-function 'face-background)
            (lambda (&rest _) "old-border"))
           ((symbol-function 'set-face-background)
            (lambda (face color frame)
              (push (list face color frame) border-colors)))
           ((symbol-function 'text-scale-set)
            (lambda (factor)
              (push factor scales))))
        (let (first second changed)
          (with-current-buffer buffer
            (setq posframe--frame nil
                  posframe--last-args nil)
            (setq first
                  (posframe--create-posframe
                   buffer
                   :position '(25 . 40)
                   :parent-frame 'parent-frame
                   :foreground-color "ivory"
                   :background-color "navy"
                   :left-fringe 3
                   :right-fringe 5
                   :border-width 4
                   :border-color "gold"
                   :font "Operator Mono"
                   :cursor 'box
                   :tty-non-selected-cursor 'hollow
                   :keep-ratio t
                   :lines-truncate t
                   :override-parameters '((alpha . 95))
                   :accept-focus t
                   :parent-text-scale-mode-amount 2))
            (setq second
                  (posframe--create-posframe
                   buffer
                   :position '(25 . 40)
                   :parent-frame 'parent-frame
                   :foreground-color "ivory"
                   :background-color "navy"
                   :left-fringe 3
                   :right-fringe 5
                   :border-width 4
                   :border-color "gold"
                   :font "Operator Mono"
                   :cursor 'box
                   :tty-non-selected-cursor 'hollow
                   :keep-ratio t
                   :lines-truncate t
                   :override-parameters '((alpha . 95))
                   :accept-focus t
                   :parent-text-scale-mode-amount 2))
            (setq changed
                  (posframe--create-posframe
                   buffer
                   :position '(25 . 40)
                   :parent-frame 'parent-frame
                   :foreground-color "ivory"
                   :background-color "maroon"
                   :left-fringe 3
                   :right-fringe 5
                   :border-width 4
                   :border-color "gold"
                   :font "Operator Mono"
                   :cursor 'box
                   :tty-non-selected-cursor 'hollow
                   :keep-ratio t
                   :lines-truncate t
                   :override-parameters '((alpha . 95))
                   :accept-focus t
                   :parent-text-scale-mode-amount 2))
            (list
             :frames (list first second changed)
             :make-count make-count
             :delete-count deleted
             :made (nreverse made)
             :buffer-state
             (list
              :truncate truncate-lines
              :cursor cursor-type
              :non-selected-cursor
              cursor-in-non-selected-windows
              :mode-line mode-line-format
              :header-line header-line-format
              :accept-focus posframe--accept-focus)
             :frame-parameter-names
             (mapcar #'car
                     (nreverse frame-parameters-set))
             :window-parameters
             (nreverse window-parameters-set)
             :border-colors
             (nreverse border-colors)
             :scales (nreverse scales)))))
    (when (buffer-live-p buffer)
      (kill-buffer buffer))))
"##;
    let expect = expect![[
        r####"OK (:frames (posframe-frame-1 posframe-frame-1 posframe-frame-2) :make-count 2 :delete-count 2 :made ((:keys (alpha foreground-color background-color title parent-frame keep-ratio posframe-buffer fullscreen no-accept-focus min-width min-height border-width internal-border-width child-frame-border-width vertical-scroll-bars horizontal-scroll-bars left-fringe right-fringe menu-bar-lines tool-bar-lines tab-bar-lines line-spacing unsplittable no-other-frame undecorated visibility cursor-type tty-non-selected-cursor minibuffer left top width height no-special-glyphs skip-taskbar inhibit-double-buffering desktop-dont-save) :title "posframe" :parent parent-frame :buffer (" *posframe-create-test*" " *posframe-create-test*") :keep-ratio t :focus nil :border (4 4) :fringes (3 5) :position (25 . 40) :alpha 95 :undecorated t) (:keys (alpha foreground-color background-color title parent-frame keep-ratio posframe-buffer fullscreen no-accept-focus min-width min-height border-width internal-border-width child-frame-border-width vertical-scroll-bars horizontal-scroll-bars left-fringe right-fringe menu-bar-lines tool-bar-lines tab-bar-lines line-spacing unsplittable no-other-frame undecorated visibility cursor-type tty-non-selected-cursor minibuffer left top width height no-special-glyphs skip-taskbar inhibit-double-buffering desktop-dont-save) :title "posframe" :parent parent-frame :buffer (" *posframe-create-test*" " *posframe-create-test*") :keep-ratio t :focus nil :border (4 4) :fringes (3 5) :position (25 . 40) :alpha 95 :undecorated t)) :buffer-state (:truncate t :cursor box :non-selected-cursor box :mode-line nil :header-line nil :accept-focus t) :frame-parameter-names (last-args font tab-bar-lines parent-frame tab-bar-lines parent-frame last-args font tab-bar-lines parent-frame) :window-parameters ((mode-line-format none) (header-line-format none) (mode-line-format none) (header-line-format none)) :border-colors ((child-frame-border "gold" posframe-frame-1) (child-frame-border "gold" posframe-frame-1) (child-frame-border "gold" posframe-frame-2)) :scales (2 2 2))"####
    ]];
    ParityBatchCase::value(
        "child_frame_creation_builds_parameters_reuses_compatible_frames_and_recreates_changed_ones",
        elisp_form,
        expect,
    )
}

fn show_orchestrates_initialization_content_sizing_position_timers_and_mouse_banishment()
-> ParityBatchCase {
    let elisp_form = r##"
(funcall
 (lambda ()
   (let ((buffer
          (get-buffer-create " *posframe-show-test*"))
         (source
          (propertize
           "Deploy staging"
           'face 'font-lock-function-name-face
           'read-only t))
         created
         sized
         moved
         refreshes
         timeouts
         frame-settings
         mouse-events)
     (unwind-protect
         (progn
           (setq posframe-test-handler-count 0
                 posframe-test-last-handler-info nil
                 posframe-test-initialize-count 0)
           (cl-letf
               (((symbol-function 'frame-width)
                 (lambda (&optional _frame) 80))
                ((symbol-function 'frame-height)
                 (lambda (&optional _frame) 30))
                ((symbol-function 'window-pixel-top)
                 (lambda (_window) 10))
                ((symbol-function 'window-pixel-left)
                 (lambda (_window) 20))
                ((symbol-function 'window-pixel-width)
                 (lambda (_window) 600))
                ((symbol-function 'window-pixel-height)
                 (lambda (window)
                   (if (eq window (selected-window))
                       400
                     30)))
                ((symbol-function 'frame-pixel-width)
                 (lambda (frame)
                   (if (eq frame 'posframe-frame)
                       400
                     1000)))
                ((symbol-function 'frame-pixel-height)
                 (lambda (frame)
                   (if (eq frame 'posframe-frame)
                       72
                     800)))
                ((symbol-function 'default-font-width)
                 (lambda () 8))
                ((symbol-function 'default-line-height)
                 (lambda () 18))
                ((symbol-function 'posframe--get-font-height)
                 (lambda (_position) 18))
                ((symbol-function 'window-mode-line-height)
                 (lambda (&optional _window) 24))
                ((symbol-function 'window-minibuffer-p)
                 (lambda (&optional _window) nil))
                ((symbol-function 'window-header-line-height)
                 (lambda (&optional _window) 10))
                ((symbol-function 'window-tab-line-height)
                 (lambda (&optional _window) 4))
                ((symbol-function 'mouse-pixel-position)
                 (lambda ()
                   (cons (selected-frame) '(300 . 200))))
                ((symbol-function 'posframe--create-posframe)
                 (lambda (created-buffer &rest arguments)
                   (push
                    (list
                     :buffer (buffer-name created-buffer)
                     :position (plist-get arguments :position)
                     :foreground
                     (plist-get arguments :foreground-color)
                     :background
                     (plist-get arguments :background-color)
                     :truncate
                     (plist-get arguments :lines-truncate)
                     :accept-focus
                     (plist-get arguments :accept-focus))
                    created)
                   (setq-local posframe--frame 'posframe-frame)
                   'posframe-frame))
                ((symbol-function 'posframe--set-frame-size)
                 (lambda (size-info)
                   (push
                    (list
                     :optimized nil
                     :width (plist-get size-info :width)
                     :height (plist-get size-info :height)
                     :maximum
                     (cons
                      (plist-get size-info :max-width)
                      (plist-get size-info :max-height))
                     :minimum
                     (cons
                      (plist-get size-info :min-width)
                      (plist-get size-info :min-height))
                     :text (buffer-string)
                     :faces
                     (posframe-test-properties
                      (buffer-string) 'face))
                    sized)))
                ((symbol-function
                  'posframe--set-frame-size-and-position)
                 (lambda
                   (size-info position
                              parent-frame-width
                              parent-frame-height)
                   (push
                    (list
                     :optimized t
                     :width (plist-get size-info :width)
                     :height (plist-get size-info :height)
                     :maximum
                     (cons
                      (plist-get size-info :max-width)
                      (plist-get size-info :max-height))
                     :minimum
                     (cons
                      (plist-get size-info :min-width)
                      (plist-get size-info :min-height))
                     :text (buffer-string)
                     :faces
                     (posframe-test-properties
                      (buffer-string) 'face))
                    sized)
                   (push position moved)
                   (setq-local
                    posframe--last-posframe-pixel-position
                    position
                    posframe--last-parent-frame-size
                    (cons
                     parent-frame-width
                     parent-frame-height)
                    posframe--last-posframe-displayed-size
                    (cons
                     (frame-pixel-width
                      (plist-get size-info :posframe))
                     (frame-pixel-height
                      (plist-get size-info :posframe))))))
                ((symbol-function 'set-frame-position)
                 (lambda (_frame x y)
                   (push (cons x y) moved)))
                ((symbol-function 'frame-visible-p)
                 (lambda (_frame) t))
                ((symbol-function 'posframe--run-refresh-timer)
                 (lambda (repeat size-info)
                   (push
                    (list repeat
                          (plist-get size-info :width)
                          (plist-get size-info :height))
                    refreshes)))
                ((symbol-function 'posframe--run-timeout-timer)
                 (lambda (_frame seconds)
                   (push seconds timeouts)))
                ((symbol-function 'frame-root-window)
                 (lambda (_frame) nil))
                ((symbol-function 'window-live-p)
                 (lambda (_window) nil))
                ((symbol-function 'set-frame-parameter)
                 (lambda (_frame parameter value)
                   (push
                    (list
                     parameter
                     (if (eq parameter
                             'posframe-parent-buffer)
                         (list
                          (car value)
                          (buffer-name (cdr value)))
                       value))
                    frame-settings))))
             (let ((posframe-mouse-banish-function
                    (lambda (info)
                      (push
                       (list
                        :mouse
                        (cons
                         (plist-get info :mouse-x)
                         (plist-get info :mouse-y))
                        :position
                        (cons
                         (plist-get info :posframe-x)
                         (plist-get info :posframe-y))
                        :size
                        (cons
                         (plist-get info :posframe-width)
                         (plist-get info :posframe-height)))
                       mouse-events))))
               (let ((first
                      (posframe-show
                       buffer
                       :string source
                       :position 7
                       :poshandler
                       #'posframe-test-counting-handler
                       :poshandler-extra-info
                       '(:raw-position (15 . 25)
                         :parent-frame-width 1200
                         :workflow release)
                       :width 100
                       :height 2
                       :max-width 50
                       :max-height 10
                       :min-width 60
                       :min-height 4
                       :foreground-color "ivory"
                       :background-color "navy"
                       :initialize #'posframe-test-initialize
                       :lines-truncate t
                       :refresh 0.25
                       :timeout 5
                       :accept-focus t
                       :hidehandler
                       #'posframe-hidehandler-when-buffer-switch))
                     (second
                      (posframe-show
                       buffer
                       :string
                       (propertize
                        "Rollback"
                        'face 'font-lock-warning-face)
                       :no-properties t
                       :position 7
                       :poshandler
                       #'posframe-test-counting-handler
                       :poshandler-extra-info
                       '(:raw-position (15 . 25)
                         :parent-frame-width 1200
                         :workflow release)
                       :width 100
                       :height 2
                       :max-width 50
                       :max-height 10
                       :min-width 60
                       :min-height 4
                       :foreground-color "ivory"
                       :background-color "navy"
                       :initialize #'posframe-test-initialize
                       :lines-truncate t
                       :refresh 0.25
                       :timeout 5
                       :accept-focus t
                       :hidehandler
                       #'posframe-hidehandler-when-buffer-switch)))
                 (list
                  :returns (list first second)
                  :initializations
                  posframe-test-initialize-count
                  :handler-calls
                  posframe-test-handler-count
                  :handler-overrides
                  (list
                   (plist-get
                    posframe-test-last-handler-info
                    :parent-frame-width)
                   (plist-get
                    posframe-test-last-handler-info
                    :workflow))
                  :source-read-only
                  (get-text-property 0 'read-only source)
                  :buffer
                  (with-current-buffer buffer
                    (list
                     :text (buffer-string)
                     :faces
                     (posframe-test-properties
                      (buffer-string) 'face)
                     :initialized
                     posframe--initialized-p))
                  :created (nreverse created)
                  :sized (nreverse sized)
                  :moved (nreverse moved)
                  :refreshes (nreverse refreshes)
                  :timeouts (nreverse timeouts)
                  :frame-settings
                  (nreverse frame-settings)
                  :mouse-events
                  (nreverse mouse-events))))))
       (when (buffer-live-p buffer)
         (kill-buffer buffer))))))
"##;
    let expect = expect![[
        r####"OK (:returns (posframe-frame posframe-frame) :initializations 1 :handler-calls 1 :handler-overrides (1200 release) :source-read-only nil :buffer (:text "Rollback" :faces nil :initialized t) :created ((:buffer " *posframe-show-test*" :position 7 :foreground "ivory" :background "navy" :truncate t :accept-focus t) (:buffer " *posframe-show-test*" :position 7 :foreground "ivory" :background "navy" :truncate t :accept-focus t)) :sized ((:optimized t :width 50 :height 4 :maximum (50 . 10) :minimum (50 . 4) :text #("Deploy staging" 0 14 (face font-lock-function-name-face)) :faces ((0 14 font-lock-function-name-face))) (:optimized nil :width 50 :height 4 :maximum (50 . 10) :minimum (50 . 4) :text "Rollback" :faces nil)) :moved ((15 . 25)) :refreshes ((0.25 50 4) (0.25 50 4)) :timeouts (5 5) :frame-settings ((posframe-hidehandler posframe-hidehandler-when-buffer-switch) (posframe-parent-buffer ("*scratch*" "*scratch*")) (posframe-hidehandler posframe-hidehandler-when-buffer-switch) (posframe-parent-buffer ("*scratch*" "*scratch*"))) :mouse-events ((:mouse (300 . 200) :position (15 . 25) :size (400 . 72)) (:mouse (300 . 200) :position (15 . 25) :size (400 . 72))))"####
    ]];
    ParityBatchCase::value(
        "show_orchestrates_initialization_content_sizing_position_timers_and_mouse_banishment",
        elisp_form,
        expect,
    )
}

fn frame_sizing_and_movement_avoid_redundant_backend_operations_but_track_parent_changes()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let (operations
        (frame-width 120)
        (frame-height 60))
    (cl-letf
        (((symbol-function 'default-font-width)
          (lambda () 8))
         ((symbol-function 'default-line-height)
          (lambda () 18))
         ((symbol-function 'set-frame-size)
          (lambda (_frame width height pixelwise)
            (push
             (list 'size width height pixelwise)
             operations)))
         ((symbol-function 'set-frame-height)
          (lambda (_frame height)
            (push (list 'height height) operations)))
         ((symbol-function 'set-frame-width)
          (lambda (_frame width)
            (push (list 'width width) operations)))
         ((symbol-function 'posframe--fit-frame-to-buffer)
          (lambda (_frame max-height min-height
                          max-width min-width only)
            (push
             (list 'fit
                   max-height min-height
                   max-width min-width only)
             operations)))
         ((symbol-function 'set-frame-position)
          (lambda (_frame x y)
            (push (list 'move x y) operations)))
         ((symbol-function 'frame-pixel-width)
          (lambda (_frame) frame-width))
         ((symbol-function 'frame-pixel-height)
          (lambda (_frame) frame-height))
         ((symbol-function 'frame-visible-p)
          (lambda (_frame) t)))
      (setq posframe--last-posframe-pixel-position nil
            posframe--last-posframe-displayed-size nil
            posframe--last-parent-frame-size nil)
      (dolist
          (size-info
           '((:posframe popup :width 10 :height 3
              :max-width 50 :max-height 20
              :min-width 2 :min-height 1)
             (:posframe popup :width nil :height 4
              :max-width 50 :max-height 20
              :min-width 2 :min-height 1)
             (:posframe popup :width 12 :height nil
              :max-width 50 :max-height 20
              :min-width 2 :min-height 1)
             (:posframe popup :width nil :height nil
              :max-width 50 :max-height 20
              :min-width 2 :min-height 1)))
        (posframe--set-frame-size size-info))
      (posframe--set-frame-position
       'popup '(30 . 40) 1000 800)
      (posframe--set-frame-position
       'popup '(30 . 40) 1000 800)
      (posframe--set-frame-position
       'popup '(30 . 40) 1100 800)
      (posframe--set-frame-position
       'popup '(-10 . -20) 1100 800)
      (posframe--set-frame-position
       'popup '(-10 . -20) 1100 800)
      (setq frame-width 130)
      (posframe--set-frame-position
       'popup '(-10 . -20) 1100 800)
      (list
       :operations (nreverse operations)
       :position-cache
       posframe--last-posframe-pixel-position
       :parent-cache
       posframe--last-parent-frame-size
       :displayed-size
       posframe--last-posframe-displayed-size
       :last-size
       (list
        :width
        (plist-get posframe--last-posframe-size :width)
        :height
        (plist-get posframe--last-posframe-size :height))))))
"##;
    let expect = expect![[
        r####"OK (:operations ((size 80 54 t) (height 4) (fit 20 1 50 2 horizontally) (width 12) (fit 20 1 50 2 vertically) (fit 20 1 50 2 nil) (move 30 40) (move 30 40) (move -10 -20) (move -10 -20)) :position-cache (-10 . -20) :parent-cache (1100 . 800) :displayed-size (130 . 60) :last-size (:width nil :height nil))"####
    ]];
    ParityBatchCase::value(
        "frame_sizing_and_movement_avoid_redundant_backend_operations_but_track_parent_changes",
        elisp_form,
        expect,
    )
}

fn mouse_banishment_moves_only_when_needed_and_respects_parent_bounds() -> ParityBatchCase {
    let elisp_form = r##"
(let (moves)
  (cl-letf (((symbol-function 'set-mouse-pixel-position)
             (lambda (_frame x y)
               (push (cons x y) moves))))
    (let ((base
           '(:parent-frame parent
             :posframe-x 100
             :posframe-y 80
             :posframe-width 240
             :posframe-height 120
             :parent-frame-width 1000
             :parent-frame-height 800)))
      (posframe-mouse-banish-default
       (append
        '(:mouse-x 150 :mouse-y 100)
        base))
      (posframe-mouse-banish-default
       (append
        '(:mouse-x 20 :mouse-y 30)
        base))
      (posframe-mouse-banish-simple base)
      (posframe-mouse-banish-simple
       '(:parent-frame parent
         :posframe-x 0
         :posframe-y 0
         :posframe-width 995
         :posframe-height 795
         :parent-frame-width 1000
         :parent-frame-height 800))
      (list :moves (nreverse moves)))))
"##;
    let expect = expect![[r####"OK (:moves ((95 . 70) (95 . 70) (1000 . 800)))"####]];
    ParityBatchCase::value(
        "mouse_banishment_moves_only_when_needed_and_respects_parent_bounds",
        elisp_form,
        expect,
    )
}

fn refresh_and_timeout_timers_replace_prior_work_and_execute_safe_callbacks() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((posframe--refresh-timer '(timer . old-refresh))
        (posframe--timeout-timer '(timer . old-timeout))
        scheduled
        cancelled
        resized
        hidden
        (timer-sequence 0))
    (cl-letf
        (((symbol-function 'timerp)
          (lambda (value)
            (and (consp value)
                 (eq (car value) 'timer))))
         ((symbol-function 'cancel-timer)
          (lambda (timer)
            (push timer cancelled)))
         ((symbol-function 'run-with-timer)
          (lambda (seconds repeat function &rest arguments)
            (setq timer-sequence (1+ timer-sequence))
            (let ((timer
                   (list
                    'timer timer-sequence
                    seconds repeat function arguments)))
              (push timer scheduled)
              timer)))
         ((symbol-function 'frame-live-p)
          (lambda (_frame) t))
         ((symbol-function 'frame-visible-p)
          (lambda (_frame) t))
         ((symbol-function 'posframe--set-frame-size)
          (lambda (size-info)
            (push
             (list
              (plist-get size-info :posframe)
              (plist-get size-info :max-width)
              (plist-get size-info :max-height))
             resized)))
         ((symbol-function 'make-frame-invisible)
          (lambda (frame &optional _force)
            (push frame hidden))))
      (let ((dynamic-size
             '(:posframe popup
               :width nil :height nil
               :max-width 60 :max-height 20
               :min-width 2 :min-height 1))
            (fixed-size
             '(:posframe popup
               :width 30 :height 5
               :max-width 60 :max-height 20
               :min-width 2 :min-height 1)))
        (posframe--run-refresh-timer
         0.25 dynamic-size)
        (posframe--run-refresh-timer
         0.5 fixed-size)
        (let ((refresh-timer posframe--refresh-timer))
          (apply
           (nth 4 refresh-timer)
           (nth 5 refresh-timer)))
        (posframe--run-timeout-timer 'popup 5)
        (let ((timeout-timer posframe--timeout-timer))
          (apply
           (nth 4 timeout-timer)
           (nth 5 timeout-timer)))
        (list
         :scheduled
         (mapcar
          (lambda (timer)
            (list
             :id (nth 1 timer)
             :seconds (nth 2 timer)
             :repeat (nth 3 timer)
             :callback
             (if (symbolp (nth 4 timer))
                 (nth 4 timer)
               'lambda)
             :arguments
             (mapcar
              (lambda (argument)
                (if (and (listp argument)
                         (plist-member argument :posframe))
                    (list
                     :posframe
                     (plist-get argument :posframe)
                     :width
                     (plist-get argument :width)
                     :height
                     (plist-get argument :height))
                  argument))
              (nth 5 timer))))
          (nreverse scheduled))
         :cancelled (nreverse cancelled)
         :resized (nreverse resized)
         :hidden (nreverse hidden)
         :refresh-token
         (list
          (car posframe--refresh-timer)
          (nth 1 posframe--refresh-timer))
         :timeout-token
         (list
          (car posframe--timeout-timer)
          (nth 1 posframe--timeout-timer)))))))
"##;
    let expect = expect![[
        r####"OK (:scheduled ((:id 1 :seconds nil :repeat 0.25 :callback lambda :arguments ((:posframe popup :width nil :height nil))) (:id 2 :seconds 5 :repeat nil :callback posframe--make-frame-invisible :arguments (popup))) :cancelled ((timer . old-refresh) (timer . old-timeout)) :resized ((popup 60 20)) :hidden (popup) :refresh-token (timer 1) :timeout-token (timer 2))"####
    ]];
    ParityBatchCase::value(
        "refresh_and_timeout_timers_replace_prior_work_and_execute_safe_callbacks",
        elisp_form,
        expect,
    )
}

fn hide_and_delete_lifecycle_targets_matching_buffers_and_switch_handlers() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((buffer-a
        (get-buffer-create " *posframe-lifecycle-a*"))
       (buffer-b
        (get-buffer-create " *posframe-lifecycle-b*"))
       (parent
        (get-buffer-create " *posframe-lifecycle-parent*"))
       (name-a (buffer-name buffer-a))
       (name-b (buffer-name buffer-b))
       hidden
       deleted
       cancelled
       marked-existing)
  (unwind-protect
      (progn
        (with-current-buffer buffer-a
          (setq posframe--frame 'frame-a
                posframe--refresh-timer
                '(timer . refresh-a)
                posframe--timeout-timer
                '(timer . timeout-a)))
        (with-current-buffer buffer-b
          (setq posframe--frame 'frame-b
                posframe--refresh-timer
                '(timer . refresh-b)
                posframe--timeout-timer
                '(timer . timeout-b)))
        (cl-letf
            (((symbol-function 'frame-list)
              (lambda ()
                '(frame-a frame-b unrelated)))
             ((symbol-function 'frame-parameter)
              (lambda (frame parameter)
                (cond
                 ((eq parameter 'posframe-buffer)
                  (cond
                   ((eq frame 'frame-a)
                    (cons name-a buffer-a))
                   ((eq frame 'frame-b)
                    (cons name-b buffer-b))))
                 ((eq parameter 'posframe-hidehandler)
                  (and
                   (eq frame 'frame-b)
                   #'posframe-hidehandler-when-buffer-switch))
                 ((eq parameter 'posframe-parent-buffer)
                  (and
                   (eq frame 'frame-b)
                   (cons (buffer-name parent) parent))))))
             ((symbol-function 'set-frame-parameter)
              (lambda (frame parameter value)
                (when (eq parameter 'existing-posframe)
                  (push (list frame value)
                        marked-existing))))
             ((symbol-function 'frame-live-p)
              (lambda (_frame) t))
             ((symbol-function 'frame-visible-p)
              (lambda (_frame) t))
             ((symbol-function 'make-frame-invisible)
              (lambda (frame &optional _force)
                (push frame hidden)))
             ((symbol-function 'delete-frame)
              (lambda (frame &optional _force)
                (push frame deleted)))
             ((symbol-function 'timerp)
              (lambda (value)
                (and (consp value)
                     (eq (car value) 'timer))))
             ((symbol-function 'cancel-timer)
              (lambda (timer)
                (push timer cancelled))))
          (posframe-hide name-a)
          (posframe-hide buffer-b)
          (posframe-hide-all)
          (posframe-hidehandler-daemon-function)
          (posframe-delete-frame buffer-a)
          (posframe-delete buffer-b)
          (list
           :hidden (nreverse hidden)
           :deleted (nreverse deleted)
           :cancelled (nreverse cancelled)
           :marked-existing
           (nreverse marked-existing)
           :buffer-a-live (buffer-live-p buffer-a)
           :buffer-b-live (buffer-live-p buffer-b))))
    (when (buffer-live-p buffer-a)
      (kill-buffer buffer-a))
    (when (buffer-live-p buffer-b)
      (kill-buffer buffer-b))
    (when (buffer-live-p parent)
      (kill-buffer parent))))
"##;
    let expect = expect![[
        r####"OK (:hidden (frame-a frame-b frame-a frame-b frame-b) :deleted (frame-a frame-b) :cancelled nil :marked-existing ((frame-a t) (frame-b t)) :buffer-a-live t :buffer-b-live nil)"####
    ]];
    ParityBatchCase::value(
        "hide_and_delete_lifecycle_targets_matching_buffers_and_switch_handlers",
        elisp_form,
        expect,
    )
}

#[test]
fn posframe_package_batch() {
    let cases = vec![
        popup_placement_catalog_maps_frame_and_window_anchors_to_pixels(),
        point_handlers_respect_glyph_offsets_viewport_clamping_and_upward_layout(),
        handler_dispatch_caches_equal_info_and_translates_reference_coordinates(),
        child_frame_creation_builds_parameters_reuses_compatible_frames_and_recreates_changed_ones(
        ),
        show_orchestrates_initialization_content_sizing_position_timers_and_mouse_banishment(),
        frame_sizing_and_movement_avoid_redundant_backend_operations_but_track_parent_changes(),
        mouse_banishment_moves_only_when_needed_and_respects_parent_bounds(),
        refresh_and_timeout_timers_replace_prior_work_and_execute_safe_callbacks(),
        hide_and_delete_lifecycle_targets_matching_buffers_and_switch_handlers(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed posframe parity test");
    assert_oracle_batch_cases(posframe_oracle(), test_name, "posframe_parity", &cases);
}
