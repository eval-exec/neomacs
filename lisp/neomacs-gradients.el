;;; neomacs-gradients.el --- Gradient background faces for Neomacs  -*- lexical-binding: t; -*-
;;;
;;; This module demonstrates how to use GPU-rendered gradient backgrounds
;;; on Neomacs faces while maintaining 100% compatibility with GNU Emacs.
;;;
;;; Gradients are specified via the :background-gradient face attribute
;;; and are GPU-rendered with zero CPU overhead. GNU Emacs ignores the
;;; :background-gradient attribute and uses the solid :background color
;;; as a fallback.

;;; Basic examples:

;; 1. Mode-line with vertical linear gradient (red to blue)
(set-face-attribute 'mode-line nil
  :background "#1a1a1a"  ; GNU Emacs fallback
  :background-gradient '(
    :type linear
    :angle 90
    :stops ((0.0 . "#FF6B9D")
            (0.5 . "#C44569")
            (1.0 . "#4A0E4E"))))

;; 2. Header-line with horizontal gradient
(set-face-attribute 'header-line nil
  :background "#333333"
  :background-gradient '(
    :type linear
    :angle 0  ; Left to right
    :stops ((0.0 . "#FFD700")
            (1.0 . "#FF8C00"))))

;; 3. Highlight with radial gradient (bright center, dark edges)
(set-face-attribute 'highlight nil
  :background "#666666"
  :background-gradient '(
    :type radial
    :center-x 0.5
    :center-y 0.5
    :radius 0.8
    :stops ((0.0 . "#FFFFFF")
            (1.0 . "#0033CC"))))

;; 4. Region face with conic gradient (spinning wheel effect)
(set-face-attribute 'region nil
  :background "#444444"
  :background-gradient '(
    :type conic
    :center-x 0.5
    :center-y 0.5
    :angle-offset 0
    :stops ((0.0 . "#FF0000")
            (0.33 . "#00FF00")
            (0.66 . "#0000FF")
            (1.0 . "#FF0000"))))

;; 5. Success/error indicators with noise pattern
(defface neomacs-success '(
  (t :foreground "white"
     :background "#1a1a1a"
     :background-gradient (:type noise
                           :scale 3.0
                           :octaves 2
                           :color1 "#004400"
                           :color2 "#00CC00")))
  "Face for successful operations")

(defface neomacs-error '(
  (t :foreground "white"
     :background "#1a1a1a"
     :background-gradient (:type noise
                           :scale 2.0
                           :octaves 3
                           :color1 "#440000"
                           :color2 "#FF0000")))
  "Face for error states")

;;; Advanced examples:

;; 6. Custom theme with gradient faces
(defun neomacs-apply-gradient-theme ()
  "Apply a fancy gradient-based theme to Neomacs."
  (interactive)

  ;; Dark mode-line with wine-to-gold gradient
  (set-face-attribute 'mode-line nil
    :foreground "#E0E0E0"
    :background "#1a0f2e"
    :background-gradient '(
      :type linear
      :angle 45
      :stops ((0.0 . "#4A1A3D")
              (0.5 . "#7A2F4F")
              (1.0 . "#3D5A2E"))))

  ;; Active mode-line with brighter gradient
  (set-face-attribute 'mode-line-active nil
    :foreground "#FFFFFF"
    :background "#1a1a2e"
    :background-gradient '(
      :type radial
      :center-x 0.5
      :center-y 0.3
      :radius 1.0
      :stops ((0.0 . "#6366F1")
              (0.5 . "#3B82F6")
              (1.0 . "#1E40AF"))))

  ;; Inactive mode-line with subtle gradient
  (set-face-attribute 'mode-line-inactive nil
    :foreground "#808080"
    :background "#0f0f0f"
    :background-gradient '(
      :type linear
      :angle 180
      :stops ((0.0 . "#1a1a1a")
              (1.0 . "#0a0a0a"))))

  ;; Tab-bar with cyan-to-purple sweep
  (set-face-attribute 'tab-bar nil
    :background "#0a0e27"
    :background-gradient '(
      :type conic
      :center-x 0.5
      :center-y 0.5
      :angle-offset 0
      :stops ((0.0 . "#00CCFF")
              (0.5 . "#9933FF")
              (1.0 . "#00CCFF"))))

  ;; Fringe with organic noise pattern
  (set-face-attribute 'fringe nil
    :background "#1a1a1a"
    :background-gradient '(
      :type noise
      :scale 4.0
      :octaves 2
      :color1 "#0a0a0a"
      :color2 "#2a2a2a")))

;; 7. Rainbow gradient for fun
(defun neomacs-rainbow-mode-line ()
  "Make the mode-line a rainbow gradient!"
  (interactive)
  (set-face-attribute 'mode-line nil
    :background "#000000"
    :background-gradient '(
      :type linear
      :angle 0  ; Left to right
      :stops ((0.0   . "#FF0000")  ; Red
              (0.17  . "#FF7F00")  ; Orange
              (0.33  . "#FFFF00")  ; Yellow
              (0.5   . "#00FF00")  ; Green
              (0.67  . "#0000FF")  ; Blue
              (0.83  . "#4B0082")  ; Indigo
              (1.0   . "#9400D3")))))  ; Violet

;; 8. Animated gradient simulation (CPU-triggered via timer)
(defvar neomacs-gradient-animation-timer nil
  "Timer for gradient animation")

(defvar neomacs-gradient-animation-angle 0
  "Current angle for animated gradient")

(defun neomacs-update-gradient-angle ()
  "Update gradient angle for animation effect."
  (setq neomacs-gradient-animation-angle
    (mod (+ neomacs-gradient-animation-angle 5) 360))
  (set-face-attribute 'highlight nil
    :background "#333333"
    :background-gradient `(
      :type linear
      :angle ,neomacs-gradient-animation-angle
      :stops ((0.0 . "#FF6B9D")
              (0.5 . "#C44569")
              (1.0 . "#4A0E4E"))))
  (force-window-update))

(defun neomacs-start-gradient-animation ()
  "Start animating the highlight face gradient."
  (interactive)
  (when neomacs-gradient-animation-timer
    (cancel-timer neomacs-gradient-animation-timer))
  (setq neomacs-gradient-animation-timer
    (run-at-time 0 0.05 #'neomacs-update-gradient-angle)))

(defun neomacs-stop-gradient-animation ()
  "Stop animating the highlight face gradient."
  (interactive)
  (when neomacs-gradient-animation-timer
    (cancel-timer neomacs-gradient-animation-timer)
    (setq neomacs-gradient-animation-timer nil)))

;;; Integration guide:

;; To use these gradients in your Neomacs config:
;;
;; 1. Add to your init.el:
;;    (require 'neomacs-gradients)
;;    (neomacs-apply-gradient-theme)
;;
;; 2. Or enable specific gradients:
;;    (set-face-attribute 'mode-line nil
;;      :background-gradient '(:type linear :angle 90
;;                             :stops ((0.0 . "#FF0000")
;;                                     (1.0 . "#0000FF"))))
;;
;; 3. GNU Emacs users: Gradients are ignored, solid colors are used.
;;    The :background attribute is always a fallback.
;;
;; 4. Customize your own gradients by playing with:
;;    - :type (linear, radial, conic, noise)
;;    - :angle (0-360 degrees for linear/conic)
;;    - :center-x, :center-y, :radius (for radial)
;;    - :scale, :octaves (for noise)
;;    - :stops with color names or hex codes

(provide 'neomacs-gradients)
;;; neomacs-gradients.el ends here
