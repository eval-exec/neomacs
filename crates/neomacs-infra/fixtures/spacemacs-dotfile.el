;;; -*- lexical-binding: t; -*-
;; Minimal deterministic Spacemacs dotfile for the fixture home.
;;
;; A missing dotfile makes Spacemacs run its first-run wizard
;; interactively (editing style, distribution...) which no harness may
;; answer; providing one skips it.  This file selects the defaults the
;; wizard would write, nothing more: the vanilla distribution, the
;; default editing style, and no extra layers — package.el installs
;; exactly the core layers' packages during the bootstrap run.
;;
;; The lexical-binding cookie on line 1 is load-bearing: without it GNU
;; raises a lexical-binding warning at startup (and pops a *Warnings*
;; window that no paired comparison could ever match).

(defun dotspacemacs/layers ()
  (setq-default
   dotspacemacs-distribution 'spacemacs
   dotspacemacs-configuration-layer-path '()
   dotspacemacs-configuration-layers '()))

(defun dotspacemacs/init ()
  (setq-default
   dotspacemacs-editing-style 'vim
   dotspacemacs-startup-banner 'official
   dotspacemacs-check-for-update nil
   dotspacemacs-elpa-https t))

(defun dotspacemacs/user-config ()
  "User customization happens here once the layers are loaded."
  ;; This must be `dotspacemacs/user-config': the pinned Spacemacs revision
  ;; calls that name (core-spacemacs.el:296) and never `dotspacemacs/user',
  ;; so an advice installed under the latter never runs.
  ;;
  ;; The home buffer's footer prints wall-clock load times that differ
  ;; between the two editors on every run.  `configuration-layer/display-summary'
  ;; computes `configuration-layer--spacemacs-startup-time' under an `unless'
  ;; guard and prints `dotspacemacs--user-config-elapsed-time' directly; this
  ;; :before advice pins both before it runs, so the footer is identical text
  ;; in every editor mounting this fixture.  (An earlier revision advised
  ;; `configuration-layer/display-startup-time' -- a function the pinned
  ;; revision does not define, which left both numbers to the clock.)
  (advice-add 'configuration-layer/display-summary :before
              (lambda (&rest _)
                (setq configuration-layer--spacemacs-startup-time 0.500
                      dotspacemacs--user-config-elapsed-time 0.500))))

(defun dotspacemacs/emacs-custom-settings ()
  "Emacs custom settings are applied once everything is loaded.")
