;;; neomacs-effects.el --- Customization for Neomacs visual effects  -*- lexical-binding: t; -*-

;; This file is part of Neomacs.

;;; Commentary:

;; Every renderer effect is configured through `defcustom', the way
;; `blink-cursor-interval' and friends are: one option per property, with a
;; type, a docstring and a group.  There is no public setter function.  Setting
;; an option with `setq' or through \\[customize] is the whole interface.
;;
;; The properties themselves are defined in Rust and published by
;; `neomacs-effect-schema', which reports each property's kind, its bounds and,
;; for a symbol-valued property, the names it accepts.  The options below are
;; written against that schema, and `neomacs-effects--check-schema' verifies at
;; load time that they still match it — so a property renamed in Rust is a
;; warning here rather than a value that is silently ignored.

;;; Code:

(declare-function neomacs-effect-schema "neomacsterm.c" (effect))
(declare-function neomacs-effect-names "neomacsterm.c" (&optional scope))
(declare-function neomacs--effect-set "neomacsterm.c" (effect &rest properties))

(defgroup neomacs nil
  "The Neomacs renderer."
  :group 'frames
  :prefix "neomacs-")

(defgroup neomacs-window-animation nil
  "How a window layout change is animated.

A `split-window' or `delete-window' does not appear all at once: the panes
travel to their new rectangles, the picture that was on screen holds the ground
it has not yet given up, and a window that is appearing fades in.  Four slots
shape that, named as niri names them — one for a window appearing, one for a
window leaving, one for a pane whose size changes, one for a pane that only
moves.

Geometry uses ONE curve for the whole frame, whichever slot supplies it.  Emacs
windows abut, so two panes animating on separate clocks would disagree about
where their shared edge is, and a one-pixel gap on a moving seam is far more
visible than the motion itself."
  :group 'neomacs
  :prefix "neomacs-window-")

(defgroup neomacs-cursor nil
  "Cursor blinking, motion and size transitions."
  :group 'neomacs
  :prefix "neomacs-cursor-")

(defgroup neomacs-transitions nil
  "Animated transitions when a window's contents change."
  :group 'neomacs
  :prefix "neomacs-")

(defun neomacs-effects--set (effect property value)
  "Push VALUE for PROPERTY of EFFECT to the renderer, if it is running.
Silently does nothing in batch mode or before the renderer exists, so that
loading a configuration file cannot fail for want of a display."
  (when (fboundp 'neomacs--effect-set)
    (ignore-errors (neomacs--effect-set effect property value))))

(defmacro neomacs-effects--defslot (slot summary occasion &rest defaults)
  "Define the ten options of window-animation SLOT.

SUMMARY names what the slot animates and OCCASION when it happens; both are
woven into each option's docstring.  DEFAULTS is a plist of the properties
whose default differs from the shared one.

A macro rather than forty written-out options because the four slots really are
parallel — they take the same ten properties, meaning the same things — and
forty hand-copied docstrings would drift apart at the first edit."
  (let* ((name (symbol-name slot))
         (sym (lambda (property) (intern (format "neomacs-%s-%s" name property))))
         (get (lambda (property fallback)
                (if (plist-member defaults property)
                    (plist-get defaults property)
                  fallback))))
    `(progn
       (defcustom ,(funcall sym "enabled") ,(funcall get :enabled t)
         ,(format "Whether %s is animated.
When nil, %s happens immediately.

Turning every window-animation slot off also stops the compositor from keeping
the previous frame in a texture, which it must do to animate from it — see
`neomacs-window-animations-off' for the cheaper way to say that." summary occasion)
         :type 'boolean
         :group 'neomacs-window-animation
         :set (lambda (symbol value)
                (set-default symbol value)
                (neomacs-effects--set ',slot :enabled value)))

       (defcustom ,(funcall sym "kind") ,(funcall get :kind ''spring)
         ,(format "Which family of curve animates %s.

`easing' is a fixed-duration curve: it takes `neomacs-%s-duration' seconds and
follows `neomacs-%s-easing'.  `spring' is a second-order spring with no
duration at all — it is described by its stiffness and damping ratio, and
settles when it arrives." summary name name)
         :type '(choice (const :tag "Fixed-duration curve" easing)
                        (const :tag "Second-order spring" spring))
         :group 'neomacs-window-animation
         :set (lambda (symbol value)
                (set-default symbol value)
                (neomacs-effects--set ',slot :kind value)))

       (defcustom ,(funcall sym "duration") ,(funcall get :duration 0.15)
         ,(format "Seconds %s takes, when `neomacs-%s-kind' is `easing'.
Zero disables this slot, exactly as setting `neomacs-%s-enabled' to nil does.
Ignored for a spring, which has no duration." summary name name)
         :type 'number
         :group 'neomacs-window-animation
         :set (lambda (symbol value)
                (set-default symbol value)
                (neomacs-effects--set ',slot :duration value)))

       (defcustom ,(funcall sym "easing") ,(funcall get :easing ''ease-out-quad)
         ,(format "Curve shape for %s, when `neomacs-%s-kind' is `easing'.

`cubic-bezier' takes its control points from the four `neomacs-%s-bezier-*'
options, which lets a curve be transcribed directly from a niri configuration.
Note that `spring' here is a fixed curve *shaped* like a spring, and is not the
same thing as setting `neomacs-%s-kind' to `spring'." summary name name name)
         :type '(choice (const linear)
                        (const ease-out-quad)
                        (const ease-out-cubic)
                        (const ease-out-expo)
                        (const ease-in-out-cubic)
                        (const :tag "Spring-shaped curve" spring)
                        (const :tag "Custom cubic Bezier" cubic-bezier))
         :group 'neomacs-window-animation
         :set (lambda (symbol value)
                (set-default symbol value)
                (neomacs-effects--set ',slot :easing value)))

       ,@(let ((axes '(("bezier-x1" :bezier-x1 0.0 "first control point's time")
                       ("bezier-y1" :bezier-y1 0.0 "first control point's value")
                       ("bezier-x2" :bezier-x2 1.0 "second control point's time")
                       ("bezier-y2" :bezier-y2 1.0 "second control point's value"))))
           (mapcar
            (lambda (axis)
              (let ((property (nth 0 axis)) (keyword (nth 1 axis))
                    (default (nth 2 axis)) (what (nth 3 axis)))
                `(defcustom ,(funcall sym property) ,default
                   ,(format "The %s of the Bezier curve for %s.
Used only when `neomacs-%s-easing' is `cubic-bezier'.  These are the same four
numbers a niri configuration writes as `cubic-bezier(x1, y1, x2, y2)'.

Time is clamped to 0.0-1.0 so the curve stays solvable; value is deliberately
not, because overshooting past 1.0 is the point of a curve like
`cubic-bezier(0.34, 1.56, 0.64, 1.0)'." what summary name)
                   :type 'number
                   :group 'neomacs-window-animation
                   :set (lambda (symbol value)
                          (set-default symbol value)
                          (neomacs-effects--set ',slot ,keyword value)))))
            axes))

       (defcustom ,(funcall sym "damping-ratio") ,(funcall get :damping-ratio 1.0)
         ,(format "How %s settles, when `neomacs-%s-kind' is `spring'.

1.0 is critically damped: the fastest approach that never overshoots.  Below
1.0 the motion bounces past its destination before returning, and above 1.0 it
creeps in more slowly than it needs to.  Clamped to 0.1-10.0." summary name)
         :type 'number
         :group 'neomacs-window-animation
         :set (lambda (symbol value)
                (set-default symbol value)
                (neomacs-effects--set ',slot :damping-ratio value)))

       (defcustom ,(funcall sym "stiffness") ,(funcall get :stiffness 800)
         ,(format "How hard the spring pulls %s toward its destination.

Higher is faster.  With the default damping ratio, 800 settles in about a third
of a second.  This is the same number a niri configuration gives as
`stiffness', and it means the same thing." summary)
         :type 'integer
         :group 'neomacs-window-animation
         :set (lambda (symbol value)
                (set-default symbol value)
                (neomacs-effects--set ',slot :stiffness value))))))

(neomacs-effects--defslot window-open
  "a window appearing"
  "a new window is simply there on the next frame"
  :kind 'easing :duration 0.15 :easing 'ease-out-expo)

(neomacs-effects--defslot window-close
  "a window going away"
  "a deleted window vanishes on the next frame"
  ;; Off by default.  A departing pane's ground is being taken by the neighbour
  ;; growing across it, and the old picture is drawn over the new one, so fading
  ;; it shows the deleted window and its replacement half-visible at once.
  ;; Uncovering it as the neighbour arrives reads better, and is what happens
  ;; when this is nil.
  :enabled nil :kind 'easing :duration 0.15 :easing 'ease-out-quad)

(neomacs-effects--defslot window-resize
  "a window whose size changes"
  "a resized window snaps to its new size")

(neomacs-effects--defslot window-movement
  "a window that moves without changing size"
  "a moved window snaps to its new position")


;;;; Global controls

(defcustom neomacs-window-animations-off nil
  "Whether to disable every window-animation slot at once.

This is the master switch, and it is cheaper than turning the four slots off
individually.  Animating a layout change requires the frame as it was *before*
the change, so while any slot is on, every frame is composed through an
offscreen ring — two full-frame textures and one extra blit per frame, whether
or not anything is moving.  Setting this reclaims that; clearing an individual
slot's `enabled' does not, because the others still need the previous picture."
  :type 'boolean
  :group 'neomacs-window-animation
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'window-animations :off value)))

(defcustom neomacs-window-animations-slowdown 1.0
  "Multiplier on the length of every window animation.

1.0 is normal speed; 20.0 makes a third-of-a-second motion take about seven
seconds.  Clamped to 0.05-20.0.

This exists mainly so an animation can be watched.  A screenshot cannot sample
a 337ms motion, and two of the four slots are springs with no duration to
lengthen by hand, so before this the only way to see one was to rebuild with a
probe."
  :type 'number
  :group 'neomacs-window-animation
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'window-animations :slowdown value)))

;;;; Cursor

(defcustom neomacs-cursor-blink-enabled t
  "Whether the cursor blinks.

Normally you want `blink-cursor-mode' instead: Neomacs mirrors that variable
into the renderer, so this follows it.  Setting this directly overrides the
mirror until `blink-cursor-mode' next changes."
  :type 'boolean
  :group 'neomacs-cursor
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'cursor-blink :enabled value)))

(defcustom neomacs-cursor-blink-interval 0.5
  "Seconds between cursor blinks.
Mirrors `blink-cursor-interval'; see `neomacs-cursor-blink-enabled'."
  :type 'number
  :group 'neomacs-cursor
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'cursor-blink :interval value)))

(defcustom neomacs-cursor-motion-enabled t
  "Whether the cursor slides between positions instead of jumping.
When nil the cursor is drawn at its new position on the next frame."
  :type 'boolean
  :group 'neomacs-cursor
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'cursor-motion :enabled value)))

(defcustom neomacs-cursor-motion-speed 1.0
  "How fast the cursor travels, as a multiplier.
Higher is faster.  Interacts with `neomacs-cursor-motion-style': a style with
its own duration uses `neomacs-cursor-motion-duration' instead."
  :type 'number
  :group 'neomacs-cursor
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'cursor-motion :speed value)))

(defcustom neomacs-cursor-motion-style 'neovide
  "The curve the cursor follows between positions.

`neovide' uses per-corner exponential easing, distance-adjusted timing,
and `neomacs-cursor-motion-trail-size', matching Neovide's cursor motion.
`exponential' eases toward the target without a fixed arrival time.
`critically-damped-spring' arrives as fast as possible without overshooting.
The remaining choices are fixed-duration curves and use
`neomacs-cursor-motion-duration'."
  :type '(choice (const neovide)
                 (const exponential)
                 (const critically-damped-spring)
                 (const linear)
                 (const ease-out-quad)
                 (const ease-out-cubic)
                 (const ease-out-expo)
                 (const ease-in-out-cubic))
  :group 'neomacs-cursor
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'cursor-motion :style value)))

(defcustom neomacs-cursor-motion-duration 0.06
  "Seconds the cursor takes to reach its new position.
For `neovide', this is the base duration, scaled by travel distance and
corner alignment.  For `critically-damped-spring', it sets spring timing.
Used directly by fixed-duration styles; ignored by `exponential'."
  :type 'number
  :group 'neomacs-cursor
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'cursor-motion :duration value)))

(defcustom neomacs-cursor-motion-trail-size 0.7
  "How much of a trail the moving cursor leaves, from 0.0 to 1.0.
Zero draws no trail."
  :type 'number
  :group 'neomacs-cursor
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'cursor-motion :trail-size value)))

(defcustom neomacs-cursor-motion-distance-length-adjust t
  "Whether longer cursor jumps take longer in the `neovide' motion style.
Scale the base duration by the logarithm of the travel distance."
  :type 'boolean
  :group 'neomacs-cursor
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'cursor-motion :distance-length-adjust value)))

(defcustom neomacs-cursor-size-transition-enabled nil
  "Whether the cursor animates when it changes shape or size.
This is the change from a box to a bar when entering `overwrite-mode', or
between characters of different widths."
  :type 'boolean
  :group 'neomacs-cursor
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'cursor-size-transition :enabled value)))

(defcustom neomacs-cursor-size-transition-duration 0.1
  "Seconds a cursor size or shape change takes."
  :type 'number
  :group 'neomacs-cursor
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'cursor-size-transition :duration value)))

;;;; Content transitions

(defcustom neomacs-buffer-transition-enabled nil
  "Whether switching the buffer shown in a window is animated.
When nil the new buffer is drawn on the next frame."
  :type 'boolean
  :group 'neomacs-transitions
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'buffer-transition :enabled value)))

(defcustom neomacs-buffer-transition-duration 0.2
  "Seconds a buffer switch takes."
  :type 'number
  :group 'neomacs-transitions
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'buffer-transition :duration value)))

(defcustom neomacs-buffer-transition-effect 'crossfade
  "How the outgoing buffer gives way to the incoming one.
`crossfade' blends them; `slide' moves one across the other; the remainder are
decorative."
  :type '(choice (const crossfade) (const slide) (const scale-zoom)
                 (const fade-edges) (const cascade) (const parallax)
                 (const tilt) (const page-curl) (const card-flip)
                 (const cylinder-roll) (const wobbly) (const wave)
                 (const per-line-spring) (const liquid) (const motion-blur)
                 (const chromatic-aberration) (const ghost-trails)
                 (const color-temperature) (const crt-scanlines)
                 (const depth-of-field) (const typewriter-reveal))
  :group 'neomacs-transitions
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'buffer-transition :effect value)))

(defcustom neomacs-buffer-transition-easing 'ease-out-quad
  "Curve shape for a buffer switch."
  :type '(choice (const linear) (const ease-out-quad) (const ease-out-cubic)
                 (const ease-out-expo) (const ease-in-out-cubic)
                 (const spring) (const cubic-bezier))
  :group 'neomacs-transitions
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'buffer-transition :easing value)))

(defcustom neomacs-buffer-transition-axis 'auto
  "Which way a directional buffer transition moves.
`auto' lets the effect choose, usually horizontally."
  :type '(choice (const auto) (const horizontal) (const vertical))
  :group 'neomacs-transitions
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'buffer-transition :axis value)))

(defcustom neomacs-buffer-transition-direction 'forward
  "Which sense a directional buffer transition runs in.
Used when nothing about the switch implies a direction of its own."
  :type '(choice (const forward) (const backward))
  :group 'neomacs-transitions
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'buffer-transition :direction value)))

(defcustom neomacs-scroll-transition-enabled nil
  "Whether scrolling a window is animated.
When nil the viewport jumps to its new position on the next frame."
  :type 'boolean
  :group 'neomacs-transitions
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'scroll-transition :enabled value)))

(defcustom neomacs-scroll-transition-duration 0.15
  "Seconds a scroll takes."
  :type 'number
  :group 'neomacs-transitions
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'scroll-transition :duration value)))

(defcustom neomacs-scroll-transition-effect 'slide
  "How the viewport moves when scrolling."
  :type '(choice (const slide) (const crossfade) (const scale-zoom)
                 (const fade-edges) (const cascade) (const parallax)
                 (const tilt) (const page-curl) (const card-flip)
                 (const cylinder-roll) (const wobbly) (const wave)
                 (const per-line-spring) (const liquid) (const motion-blur)
                 (const chromatic-aberration) (const ghost-trails)
                 (const color-temperature) (const crt-scanlines)
                 (const depth-of-field) (const typewriter-reveal))
  :group 'neomacs-transitions
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'scroll-transition :effect value)))

(defcustom neomacs-scroll-transition-easing 'ease-out-quad
  "Curve shape for a scroll."
  :type '(choice (const linear) (const ease-out-quad) (const ease-out-cubic)
                 (const ease-out-expo) (const ease-in-out-cubic)
                 (const spring) (const cubic-bezier))
  :group 'neomacs-transitions
  :set (lambda (symbol value)
         (set-default symbol value)
         (neomacs-effects--set 'scroll-transition :easing value)))


;;;; The gallery

;; The ~150 decorative effects are generated rather than written out. One
;; option per effect, holding its properties as a plist, with a `:type' built
;; from the Rust schema so each property still gets a real widget.
;;
;; Per-effect rather than per-property, unlike the behavioural options above,
;; and that difference is deliberate. Nobody completes on
;; `neomacs-effect-argyle-pattern-diamond-size'; what you do with a gallery is
;; browse it, turn one on, and nudge a number. Generating 800 more symbols
;; would bury the 61 above in `M-x customize-apropos', and each would carry a
;; docstring nobody wrote.

(defun neomacs-effects--property-type (entry)
  "Build a customization type for one `neomacs-effect-schema' ENTRY."
  (pcase (cadr entry)
    ('boolean 'boolean)
    ('integer 'integer)
    ('seconds '(number :tag "Seconds"))
    ('unit-interval '(number :tag "0.0 to 1.0"))
    ('percentage '(number :tag "0 to 100"))
    ('color '(choice (string :tag "#RRGGBB")
                     (repeat :tag "Channels" number)))
    ('color-list '(repeat (repeat :tag "Channels" number)))
    ('symbol `(choice ,@(mapcar (lambda (name) (list 'const name))
                                (cddr entry))))
    (_ 'sexp)))

(defun neomacs-effects--plist-type (schema)
  "Build a plist customization type from SCHEMA."
  `(plist :options
          ,(mapcar (lambda (entry)
                     (list (car entry) (neomacs-effects--property-type entry)))
                   schema)))

(defun neomacs-effects--define-gallery ()
  "Define one option per decorative effect, from the Rust schema."
  (dolist (effect (neomacs-effect-names 'shader))
    (let* ((schema (neomacs-effect-schema effect))
           (symbol (intern (format "neomacs-effect-%s" effect))))
      (custom-declare-variable
       symbol
       `',(neomacs-effect-get effect)
       (format "Properties of the `%s' renderer effect.

A plist of the properties this effect accepts; see the widget for the type of
each.  Decorative effects are configured one effect at a time rather than one
property at a time — see the commentary in neomacs-effects.el."
               effect)
       :type (neomacs-effects--plist-type schema)
       :group 'neomacs-effects-gallery
       :set (lambda (symbol value)
              (set-default symbol value)
              (when (fboundp 'neomacs--effect-set)
                (ignore-errors (apply #'neomacs--effect-set effect value))))))))

(defgroup neomacs-effects-gallery nil
  "Decorative renderer effects.

Roughly a hundred and fifty shader effects — cursor comets, auroras, patterned
backgrounds.  Each is one option holding that effect's properties."
  :group 'neomacs
  :prefix "neomacs-effect-")

;;;; Keeping the options honest

(defun neomacs-effects--check-schema ()
  "Warn if the options above no longer match what the renderer accepts.

The behavioural options are written by hand, so a property renamed or removed
in Rust would leave an option that silently does nothing.  This compares the
two and reports the difference rather than letting it rot."
  (dolist (effect (list 'window-open 'window-close 'window-resize
                        'window-movement 'window-animations 'cursor-blink
                        'cursor-motion 'cursor-size-transition
                        'buffer-transition 'scroll-transition))
    (dolist (entry (neomacs-effect-schema effect))
      (let* ((property (substring (symbol-name (car entry)) 1))
             (symbol (intern (format "neomacs-%s-%s" effect property))))
        (unless (boundp symbol)
          (display-warning
           'neomacs
           (format "%s accepts :%s but there is no `%s' to set it"
                   effect property symbol)
           :warning))))))

(when (fboundp 'neomacs-effect-schema)
  (neomacs-effects--define-gallery)
  (neomacs-effects--check-schema))

(provide 'neomacs-effects)

;;; neomacs-effects.el ends here
