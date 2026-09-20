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

(defun dotspacemacs/user ()
  "User customization happens here once the layers are loaded."
  ;; The home buffer's footer prints wall-clock load times that differ
  ;; between the two editors on every run.  display-startup-time only
  ;; computes them when unset (`unless'), and this file loads before it
  ;; runs -- so pin both numbers, and the footer becomes identical text
  ;; in every editor mounting this fixture.
  ;; :before so the values are pinned before display-startup-time's
  ;; `unless' guards read them.
  (advice-add 'configuration-layer/display-startup-time :before
              (lambda (&rest _)
                (setq configuration-layer--spacemacs-startup-time 0.500
                      dotspacemacs--user-config-elapsed-time 0.500))))

(defun dotspacemacs/emacs-custom-settings ()
  "Emacs custom settings are applied once everything is loaded.")
