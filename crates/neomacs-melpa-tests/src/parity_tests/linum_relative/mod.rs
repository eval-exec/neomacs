use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, LINUM_RELATIVE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const LINUM_RELATIVE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const LINUM_RELATIVE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'linum-relative)

(defun neomacs-linum-relative-test-rendered-lines ()
  "Describe the actual legacy Linum gutter overlays in this buffer."
  (save-excursion
    (goto-char (point-min))
    (let ((line 1)
          rendered)
      (while (not (eobp))
        (let* ((overlay
                (cl-find-if
                 (lambda (candidate)
                   (overlay-get candidate 'linum-str))
                 (overlays-in (point) (point))))
               (text (and overlay (overlay-get overlay 'linum-str)))
               (before (and overlay (overlay-get overlay 'before-string)))
               (display (and before (get-text-property 0 'display before))))
          (push
           (list
            :line line
            :position (point)
            :text (and text (substring-no-properties text))
            :face (and text (get-text-property 0 'face text))
            :margin-display
            (and display
                 (equal (car display) '(margin left-margin)))
            :same-display-text
            (and display text
                 (equal-including-properties (cadr display) text)))
           rendered))
        (forward-line 1)
        (setq line (1+ line)))
      (nreverse rendered))))

(defun neomacs-linum-relative-test-legacy-state (window)
  "Describe package, Linum, hook, overlay, and WINDOW margin state."
  (list
   :relative-mode linum-relative-mode
   :lighter
   (let ((entry (assq 'linum-relative-mode minor-mode-alist)))
     (list
      :entry (copy-tree (cdr entry))
      :value
      (and linum-relative-mode
           (symbol-value (cadr entry)))))
   :linum-mode linum-mode
   :format linum-format
   :saved-format linum-relative-user-format
   :last-position linum-relative-last-pos
   :rendered (neomacs-linum-relative-test-rendered-lines)
   :left-margin (car (window-margins window))
   :owned-left-margin
   (car-safe (window-parameter window 'linum--set-margins))
   :hooks
   (list
    :post-command
    (cl-count #'linum-update-current post-command-hook :test #'eq)
    :window-scroll
    (cl-count #'linum-after-scroll window-scroll-functions :test #'eq)
    :window-configuration
    (cl-count
     #'linum-update-current
     window-configuration-change-hook
     :test #'eq)
    :change-major-mode
    (cl-count #'linum-delete-overlays change-major-mode-hook :test #'eq))))

(defun neomacs-linum-relative-test-native-state ()
  "Describe package and native line-number backend state."
  (list
   :relative-mode linum-relative-mode
   :native-mode display-line-numbers-mode
   :type display-line-numbers-type
   :display display-line-numbers
   :saved-type linum-relative-user-type
   :legacy-mode linum-mode
   :current-symbol linum-relative-current-symbol
   :relative-format linum-relative-format
   :offset linum-relative-plusp-offset))
"##;

fn linum_relative_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LINUM_RELATIVE_MELPA_PIN, "linum-relative.el")
        .expect("prepare exact linum-relative source below ./tmp")
        .with_prelude(LINUM_RELATIVE_TEST_PRELUDE)
        .with_timeout(LINUM_RELATIVE_TEST_TIMEOUT)
}

fn legacy_backend_renders_and_recenters_a_real_relative_gutter() -> ParityBatchCase {
    let elisp_form = r##"
(save-window-excursion
  (let ((buffer (generate-new-buffer " *linum-relative-gutter*"))
        (window (selected-window))
        (linum-relative-backend 'linum-mode)
        (linum-relative-current-symbol "▶")
        (linum-relative-plusp-offset 0)
        (linum-relative-format " %2s│")
        (linum-relative-last-pos 0)
        (linum-relative-user-format 'dynamic)
        (linum-format "%04d")
        initial moved disabled result)
    (unwind-protect
        (progn
          (delete-other-windows)
          (set-window-buffer window buffer)
          (with-current-buffer buffer
            (insert "alpha\nβeta\n\ncurrent\nfifth\nsixth\n")
            (set-buffer-modified-p nil)
            (goto-char (point-min))
            (forward-line 3)
            (linum-relative-mode 1)
            (setq initial
                  (list
                   :point (list (point) (line-number-at-pos))
                   :state
                   (neomacs-linum-relative-test-legacy-state window)))
            (goto-char (point-min))
            (forward-line 1)
            (linum-update-current)
            (setq moved
                  (list
                   :point (list (point) (line-number-at-pos))
                   :state
                   (neomacs-linum-relative-test-legacy-state window)))
            (linum-relative-mode -1)
            (setq disabled
                  (neomacs-linum-relative-test-legacy-state window))
            (setq result
                  (list
                   :initial initial
                   :after-move moved
                   :disabled disabled
                   :buffer
                   (list
                    :text (buffer-string)
                    :point (point)
                    :modified (buffer-modified-p))))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (when linum-relative-mode
            (linum-relative-mode -1))
          (when linum-mode
            (linum-mode -1)))
        (kill-buffer buffer)))
    result))
"##;
    let expect = expect![[
        r#"OK (:initial (:point (13 4) :state (:relative-mode t :lighter (:entry (linum-relative-lighter) :value " LR") :linum-mode t :format linum-relative :saved-format "%04d" :last-position 4 :rendered ((:line 1 :position 1 :text "  3│" :face linum :margin-display t :same-display-text t) (:line 2 :position 7 :text "  2│" :face linum :margin-display t :same-display-text t) (:line 3 :position 12 :text "  1│" :face linum :margin-display t :same-display-text t) (:line 4 :position 13 :text "  ▶│" :face linum-relative-current-face :margin-display t :same-display-text t) (:line 5 :position 21 :text "  1│" :face linum :margin-display t :same-display-text t) (:line 6 :position 27 :text "  2│" :face linum :margin-display t :same-display-text t)) :left-margin 4 :owned-left-margin 4 :hooks (:post-command 1 :window-scroll 1 :window-configuration 1 :change-major-mode 1))) :after-move (:point (7 2) :state (:relative-mode t :lighter (:entry (linum-relative-lighter) :value " LR") :linum-mode t :format linum-relative :saved-format "%04d" :last-position 2 :rendered ((:line 1 :position 1 :text "  1│" :face linum :margin-display t :same-display-text t) (:line 2 :position 7 :text "  ▶│" :face linum-relative-current-face :margin-display t :same-display-text t) (:line 3 :position 12 :text "  1│" :face linum :margin-display t :same-display-text t) (:line 4 :position 13 :text "  2│" :face linum :margin-display t :same-display-text t) (:line 5 :position 21 :text "  3│" :face linum :margin-display t :same-display-text t) (:line 6 :position 27 :text "  4│" :face linum :margin-display t :same-display-text t)) :left-margin 4 :owned-left-margin 4 :hooks (:post-command 1 :window-scroll 1 :window-configuration 1 :change-major-mode 1))) :disabled (:relative-mode nil :lighter (:entry (linum-relative-lighter) :value nil) :linum-mode nil :format "%04d" :saved-format "%04d" :last-position 2 :rendered ((:line 1 :position 1 :text nil :face nil :margin-display nil :same-display-text nil) (:line 2 :position 7 :text nil :face nil :margin-display nil :same-display-text nil) (:line 3 :position 12 :text nil :face nil :margin-display nil :same-display-text nil) (:line 4 :position 13 :text nil :face nil :margin-display nil :same-display-text nil) (:line 5 :position 21 :text nil :face nil :margin-display nil :same-display-text nil) (:line 6 :position 27 :text nil :face nil :margin-display nil :same-display-text nil)) :left-margin nil :owned-left-margin nil :hooks (:post-command 0 :window-scroll 0 :window-configuration 0 :change-major-mode 0)) :buffer (:text "alpha\nβeta\n\ncurrent\nfifth\nsixth\n" :point 7 :modified nil))"#
    ]];
    ParityBatchCase::value(
        "legacy_backend_renders_and_recenters_a_real_relative_gutter",
        elisp_form,
        expect,
    )
}

fn legacy_toggle_exposes_ownership_of_an_existing_absolute_gutter() -> ParityBatchCase {
    let elisp_form = r##"
(save-window-excursion
  (let ((buffer (generate-new-buffer " *linum-relative-existing*"))
        (window (selected-window))
        (linum-relative-backend 'linum-mode)
        (linum-relative-current-symbol "0")
        (linum-relative-plusp-offset 0)
        (linum-relative-format "%2s")
        (linum-relative-last-pos 0)
        (linum-relative-user-format 'dynamic)
        (linum-format "%04d")
        absolute relative toggled-off reenabled external-takeover result)
    (unwind-protect
        (progn
          (delete-other-windows)
          (set-window-buffer window buffer)
          (set-window-margins window 1 nil)
          (with-current-buffer buffer
            (insert "plan\nbuild\nverify\ndeploy\n")
            (set-buffer-modified-p nil)
            (goto-char (point-min))
            (forward-line 2)
            (linum-mode 1)
            (setq absolute
                  (neomacs-linum-relative-test-legacy-state window))
            (linum-relative-toggle)
            (setq relative
                  (neomacs-linum-relative-test-legacy-state window))
            (linum-relative-toggle)
            (setq toggled-off
                  (neomacs-linum-relative-test-legacy-state window))
            (set-window-margins window 1 nil)
            (linum-relative-toggle)
            (setq reenabled
                  (neomacs-linum-relative-test-legacy-state window))
            (set-window-margins window 7 nil)
            (linum-relative-toggle)
            (setq external-takeover
                  (neomacs-linum-relative-test-legacy-state window))
            (setq result
                  (list
                   :absolute absolute
                   :relative relative
                   :toggled-off toggled-off
                   :reenabled reenabled
                   :external-margin-takeover external-takeover
                   :buffer
                   (list
                    :text (buffer-string)
                    :point (point)
                    :line (line-number-at-pos)
                    :modified (buffer-modified-p))))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (when linum-relative-mode
            (linum-relative-mode -1))
          (when linum-mode
            (linum-mode -1)))
        (set-window-margins window nil nil)
        (set-window-parameter window 'linum--set-margins nil)
        (kill-buffer buffer)))
    result))
"##;
    let expect = expect![[
        r#"OK (:absolute (:relative-mode nil :lighter (:entry (linum-relative-lighter) :value nil) :linum-mode t :format "%04d" :saved-format dynamic :last-position 3 :rendered ((:line 1 :position 1 :text "0001" :face linum :margin-display t :same-display-text t) (:line 2 :position 6 :text "0002" :face linum :margin-display t :same-display-text t) (:line 3 :position 12 :text "0003" :face linum :margin-display t :same-display-text t) (:line 4 :position 19 :text "0004" :face linum :margin-display t :same-display-text t)) :left-margin 4 :owned-left-margin 4 :hooks (:post-command 1 :window-scroll 1 :window-configuration 1 :change-major-mode 1)) :relative (:relative-mode nil :lighter (:entry (linum-relative-lighter) :value nil) :linum-mode t :format linum-relative :saved-format "%04d" :last-position 3 :rendered ((:line 1 :position 1 :text " 2" :face linum :margin-display t :same-display-text t) (:line 2 :position 6 :text " 1" :face linum :margin-display t :same-display-text t) (:line 3 :position 12 :text " 0" :face linum-relative-current-face :margin-display t :same-display-text t) (:line 4 :position 19 :text " 1" :face linum :margin-display t :same-display-text t)) :left-margin 4 :owned-left-margin 4 :hooks (:post-command 1 :window-scroll 1 :window-configuration 1 :change-major-mode 1)) :toggled-off (:relative-mode nil :lighter (:entry (linum-relative-lighter) :value nil) :linum-mode nil :format "%04d" :saved-format "%04d" :last-position 3 :rendered ((:line 1 :position 1 :text nil :face nil :margin-display nil :same-display-text nil) (:line 2 :position 6 :text nil :face nil :margin-display nil :same-display-text nil) (:line 3 :position 12 :text nil :face nil :margin-display nil :same-display-text nil) (:line 4 :position 19 :text nil :face nil :margin-display nil :same-display-text nil)) :left-margin nil :owned-left-margin nil :hooks (:post-command 0 :window-scroll 0 :window-configuration 0 :change-major-mode 0)) :reenabled (:relative-mode nil :lighter (:entry (linum-relative-lighter) :value nil) :linum-mode t :format linum-relative :saved-format "%04d" :last-position 3 :rendered ((:line 1 :position 1 :text " 2" :face linum :margin-display t :same-display-text t) (:line 2 :position 6 :text " 1" :face linum :margin-display t :same-display-text t) (:line 3 :position 12 :text " 0" :face linum-relative-current-face :margin-display t :same-display-text t) (:line 4 :position 19 :text " 1" :face linum :margin-display t :same-display-text t)) :left-margin 2 :owned-left-margin 2 :hooks (:post-command 1 :window-scroll 1 :window-configuration 1 :change-major-mode 1)) :external-margin-takeover (:relative-mode nil :lighter (:entry (linum-relative-lighter) :value nil) :linum-mode nil :format "%04d" :saved-format "%04d" :last-position 3 :rendered ((:line 1 :position 1 :text nil :face nil :margin-display nil :same-display-text nil) (:line 2 :position 6 :text nil :face nil :margin-display nil :same-display-text nil) (:line 3 :position 12 :text nil :face nil :margin-display nil :same-display-text nil) (:line 4 :position 19 :text nil :face nil :margin-display nil :same-display-text nil)) :left-margin 7 :owned-left-margin 2 :hooks (:post-command 0 :window-scroll 0 :window-configuration 0 :change-major-mode 0)) :buffer (:text "plan\nbuild\nverify\ndeploy\n" :point 12 :line 3 :modified nil))"#
    ]];
    ParityBatchCase::value(
        "legacy_toggle_exposes_ownership_of_an_existing_absolute_gutter",
        elisp_form,
        expect,
    )
}

fn native_backend_toggles_relative_and_restores_the_visual_policy() -> ParityBatchCase {
    let elisp_form = r##"
(let ((linum-relative-backend 'display-line-numbers-mode)
      (linum-relative-current-symbol "IGNORED")
      (linum-relative-format "ignored:%s")
      (linum-relative-plusp-offset 37)
      (linum-relative-user-type t)
      (display-line-numbers-type t)
      stages)
  (with-temp-buffer
    (insert "one\ntwo\nthree\nfour\n")
    (set-buffer-modified-p nil)
    (setq-local display-line-numbers-type 'visual)
    (display-line-numbers-mode 1)
    (push (cons :preexisting (neomacs-linum-relative-test-native-state)) stages)
    (unwind-protect
        (progn
          (linum-relative-mode 1)
          (push (cons :enabled (neomacs-linum-relative-test-native-state)) stages)
          (linum-relative-toggle)
          (push (cons :toggle-off (neomacs-linum-relative-test-native-state)) stages)
          (linum-relative-toggle)
          (push (cons :toggle-on (neomacs-linum-relative-test-native-state)) stages)
          (linum-relative-mode -1)
          (push (cons :disabled (neomacs-linum-relative-test-native-state)) stages)
          (list
           :stages (nreverse stages)
           :buffer
           (list
            :text (buffer-string)
            :point (point)
            :modified (buffer-modified-p))))
      (when linum-relative-mode
        (linum-relative-mode -1))
      (when display-line-numbers-mode
        (display-line-numbers-mode -1)))))
"##;
    let expect = expect![[
        r#"OK (:stages ((:preexisting :relative-mode nil :native-mode t :type visual :display visual :saved-type t :legacy-mode nil :current-symbol "IGNORED" :relative-format "ignored:%s" :offset 37) (:enabled :relative-mode t :native-mode t :type relative :display relative :saved-type visual :legacy-mode nil :current-symbol "IGNORED" :relative-format "ignored:%s" :offset 37) (:toggle-off :relative-mode t :native-mode nil :type visual :display nil :saved-type visual :legacy-mode nil :current-symbol "IGNORED" :relative-format "ignored:%s" :offset 37) (:toggle-on :relative-mode t :native-mode t :type relative :display relative :saved-type visual :legacy-mode nil :current-symbol "IGNORED" :relative-format "ignored:%s" :offset 37) (:disabled :relative-mode nil :native-mode nil :type visual :display nil :saved-type visual :legacy-mode nil :current-symbol "IGNORED" :relative-format "ignored:%s" :offset 37)) :buffer (:text "one\ntwo\nthree\nfour\n" :point 20 :modified nil))"#
    ]];
    ParityBatchCase::value(
        "native_backend_toggles_relative_and_restores_the_visual_policy",
        elisp_form,
        expect,
    )
}

fn native_backend_two_buffer_workflow_surfaces_shared_saved_policy() -> ParityBatchCase {
    let elisp_form = r##"
(let ((first (generate-new-buffer " *linum-relative-native-a*"))
      (second (generate-new-buffer " *linum-relative-native-b*"))
      (linum-relative-backend 'display-line-numbers-mode)
      (linum-relative-user-type t)
      (display-line-numbers-type t)
      stages result)
  (unwind-protect
      (progn
        (push
         (list :initial
               :global-type display-line-numbers-type
               :saved-type linum-relative-user-type)
         stages)
        (with-current-buffer first
          (insert "alpha\nbeta\n")
          (linum-relative-mode 1))
        (push
         (list
          :first-enabled
          :global-type display-line-numbers-type
          :saved-type linum-relative-user-type
          :first
          (with-current-buffer first
            (neomacs-linum-relative-test-native-state)))
         stages)
        (with-current-buffer second
          (insert "uno\ndos\n")
          (linum-relative-mode 1))
        (push
         (list
          :second-enabled
          :global-type display-line-numbers-type
          :saved-type linum-relative-user-type
          :buffers
          (list
           (with-current-buffer first
             (neomacs-linum-relative-test-native-state))
           (with-current-buffer second
             (neomacs-linum-relative-test-native-state))))
         stages)
        (with-current-buffer first
          (linum-relative-mode -1))
        (push
         (list
          :first-disabled
          :global-type display-line-numbers-type
          :saved-type linum-relative-user-type
          :first
          (with-current-buffer first
            (neomacs-linum-relative-test-native-state))
          :second
          (with-current-buffer second
            (neomacs-linum-relative-test-native-state)))
         stages)
        (with-current-buffer second
          (linum-relative-mode -1))
        (push
         (list
          :both-disabled
          :global-type display-line-numbers-type
          :saved-type linum-relative-user-type
          :buffers
          (list
           (with-current-buffer first
             (neomacs-linum-relative-test-native-state))
           (with-current-buffer second
             (neomacs-linum-relative-test-native-state))))
         stages)
        (setq result (nreverse stages)))
    (dolist (buffer (list first second))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (when linum-relative-mode
            (linum-relative-mode -1))
          (when display-line-numbers-mode
            (display-line-numbers-mode -1)))
        (kill-buffer buffer))))
  result)
"##;
    let expect = expect![[
        r#"OK ((:initial :global-type t :saved-type t) (:first-enabled :global-type relative :saved-type t :first (:relative-mode t :native-mode t :type relative :display relative :saved-type t :legacy-mode nil :current-symbol "0" :relative-format "%3s" :offset 0)) (:second-enabled :global-type relative :saved-type relative :buffers ((:relative-mode t :native-mode t :type relative :display relative :saved-type relative :legacy-mode nil :current-symbol "0" :relative-format "%3s" :offset 0) (:relative-mode t :native-mode t :type relative :display relative :saved-type relative :legacy-mode nil :current-symbol "0" :relative-format "%3s" :offset 0))) (:first-disabled :global-type relative :saved-type relative :first (:relative-mode nil :native-mode nil :type relative :display nil :saved-type relative :legacy-mode nil :current-symbol "0" :relative-format "%3s" :offset 0) :second (:relative-mode t :native-mode t :type relative :display relative :saved-type relative :legacy-mode nil :current-symbol "0" :relative-format "%3s" :offset 0)) (:both-disabled :global-type relative :saved-type relative :buffers ((:relative-mode nil :native-mode nil :type relative :display nil :saved-type relative :legacy-mode nil :current-symbol "0" :relative-format "%3s" :offset 0) (:relative-mode nil :native-mode nil :type relative :display nil :saved-type relative :legacy-mode nil :current-symbol "0" :relative-format "%3s" :offset 0))))"#
    ]];
    ParityBatchCase::value(
        "native_backend_two_buffer_workflow_surfaces_shared_saved_policy",
        elisp_form,
        expect,
    )
}

fn legacy_backend_two_buffer_workflow_surfaces_shared_saved_format() -> ParityBatchCase {
    let elisp_form = r##"
(save-window-excursion
  (let* ((first (generate-new-buffer " *linum-relative-legacy-a*"))
         (second (generate-new-buffer " *linum-relative-legacy-b*"))
         (first-window (selected-window))
         (second-window (progn
                          (delete-other-windows)
                          (split-window first-window nil 'right)))
         (linum-relative-backend 'linum-mode)
         (linum-relative-current-symbol "0")
         (linum-relative-format "%2s")
         (linum-relative-last-pos 0)
         (linum-relative-user-format 'dynamic)
         (linum-format 'dynamic)
         stages result)
    (unwind-protect
        (progn
          (set-window-buffer first-window first)
          (set-window-buffer second-window second)
          (with-current-buffer first
            (insert "alpha\nbeta\ngamma\n")
            (set-buffer-modified-p nil)
            (setq-local linum-format "A%02d")
            (goto-char (point-min))
            (forward-line 1)
            (linum-relative-mode 1))
          (push
           (list
            :first-enabled
            :saved-format linum-relative-user-format
            :first
            (with-current-buffer first
              (neomacs-linum-relative-test-legacy-state first-window)))
           stages)
          (with-current-buffer second
            (insert "uno\ndos\ntres\n")
            (set-buffer-modified-p nil)
            (setq-local linum-format "B[%d]")
            (goto-char (point-min))
            (forward-line 2)
            (linum-relative-mode 1))
          (push
           (list
            :second-enabled
            :saved-format linum-relative-user-format
            :buffers
            (list
             (with-current-buffer first
               (neomacs-linum-relative-test-legacy-state first-window))
             (with-current-buffer second
               (neomacs-linum-relative-test-legacy-state second-window))))
           stages)
          (with-current-buffer first
            (linum-relative-mode -1))
          (push
           (list
            :first-disabled
            :saved-format linum-relative-user-format
            :first-format (with-current-buffer first linum-format)
            :first
            (with-current-buffer first
              (neomacs-linum-relative-test-legacy-state first-window))
            :second
            (with-current-buffer second
              (neomacs-linum-relative-test-legacy-state second-window)))
           stages)
          (with-current-buffer second
            (linum-relative-mode -1))
          (push
           (list
            :both-disabled
            :saved-format linum-relative-user-format
            :formats
            (list
             (with-current-buffer first linum-format)
             (with-current-buffer second linum-format))
            :states
            (list
             (with-current-buffer first
               (neomacs-linum-relative-test-legacy-state first-window))
             (with-current-buffer second
               (neomacs-linum-relative-test-legacy-state second-window))))
           stages)
          (setq result (nreverse stages)))
      (dolist (buffer (list first second))
        (when (buffer-live-p buffer)
          (with-current-buffer buffer
            (when linum-relative-mode
              (linum-relative-mode -1))
            (when linum-mode
              (linum-mode -1)))
          (kill-buffer buffer))))
    result))
"##;
    let expect = expect![[
        r#"OK ((:first-enabled :saved-format "A%02d" :first (:relative-mode t :lighter (:entry (linum-relative-lighter) :value " LR") :linum-mode t :format linum-relative :saved-format "A%02d" :last-position 2 :rendered ((:line 1 :position 1 :text " 1" :face linum :margin-display t :same-display-text t) (:line 2 :position 7 :text " 0" :face linum-relative-current-face :margin-display t :same-display-text t) (:line 3 :position 12 :text " 1" :face linum :margin-display t :same-display-text t)) :left-margin 2 :owned-left-margin 2 :hooks (:post-command 1 :window-scroll 1 :window-configuration 1 :change-major-mode 1))) (:second-enabled :saved-format "B[%d]" :buffers ((:relative-mode t :lighter (:entry (linum-relative-lighter) :value " LR") :linum-mode t :format linum-relative :saved-format "B[%d]" :last-position 3 :rendered ((:line 1 :position 1 :text " 1" :face linum :margin-display t :same-display-text t) (:line 2 :position 7 :text " 0" :face linum-relative-current-face :margin-display t :same-display-text t) (:line 3 :position 12 :text " 1" :face linum :margin-display t :same-display-text t)) :left-margin 2 :owned-left-margin 2 :hooks (:post-command 1 :window-scroll 1 :window-configuration 1 :change-major-mode 1)) (:relative-mode t :lighter (:entry (linum-relative-lighter) :value " LR") :linum-mode t :format linum-relative :saved-format "B[%d]" :last-position 3 :rendered ((:line 1 :position 1 :text " 2" :face linum :margin-display t :same-display-text t) (:line 2 :position 5 :text " 1" :face linum :margin-display t :same-display-text t) (:line 3 :position 9 :text " 0" :face linum-relative-current-face :margin-display t :same-display-text t)) :left-margin 2 :owned-left-margin 2 :hooks (:post-command 1 :window-scroll 1 :window-configuration 1 :change-major-mode 1)))) (:first-disabled :saved-format "B[%d]" :first-format "B[%d]" :first (:relative-mode nil :lighter (:entry (linum-relative-lighter) :value nil) :linum-mode nil :format "B[%d]" :saved-format "B[%d]" :last-position 3 :rendered ((:line 1 :position 1 :text nil :face nil :margin-display nil :same-display-text nil) (:line 2 :position 7 :text nil :face nil :margin-display nil :same-display-text nil) (:line 3 :position 12 :text nil :face nil :margin-display nil :same-display-text nil)) :left-margin nil :owned-left-margin nil :hooks (:post-command 0 :window-scroll 0 :window-configuration 0 :change-major-mode 0)) :second (:relative-mode t :lighter (:entry (linum-relative-lighter) :value " LR") :linum-mode t :format linum-relative :saved-format "B[%d]" :last-position 3 :rendered ((:line 1 :position 1 :text " 2" :face linum :margin-display t :same-display-text t) (:line 2 :position 5 :text " 1" :face linum :margin-display t :same-display-text t) (:line 3 :position 9 :text " 0" :face linum-relative-current-face :margin-display t :same-display-text t)) :left-margin 2 :owned-left-margin 2 :hooks (:post-command 1 :window-scroll 1 :window-configuration 1 :change-major-mode 1))) (:both-disabled :saved-format "B[%d]" :formats ("B[%d]" "B[%d]") :states ((:relative-mode nil :lighter (:entry (linum-relative-lighter) :value nil) :linum-mode nil :format "B[%d]" :saved-format "B[%d]" :last-position 3 :rendered ((:line 1 :position 1 :text nil :face nil :margin-display nil :same-display-text nil) (:line 2 :position 7 :text nil :face nil :margin-display nil :same-display-text nil) (:line 3 :position 12 :text nil :face nil :margin-display nil :same-display-text nil)) :left-margin nil :owned-left-margin nil :hooks (:post-command 0 :window-scroll 0 :window-configuration 0 :change-major-mode 0)) (:relative-mode nil :lighter (:entry (linum-relative-lighter) :value nil) :linum-mode nil :format "B[%d]" :saved-format "B[%d]" :last-position 3 :rendered ((:line 1 :position 1 :text nil :face nil :margin-display nil :same-display-text nil) (:line 2 :position 5 :text nil :face nil :margin-display nil :same-display-text nil) (:line 3 :position 9 :text nil :face nil :margin-display nil :same-display-text nil)) :left-margin nil :owned-left-margin nil :hooks (:post-command 0 :window-scroll 0 :window-configuration 0 :change-major-mode 0)))))"#
    ]];
    ParityBatchCase::value(
        "legacy_backend_two_buffer_workflow_surfaces_shared_saved_format",
        elisp_form,
        expect,
    )
}

fn cross_window_refresh_temporarily_numbers_the_target_from_the_callers_line() -> ParityBatchCase {
    let elisp_form = r##"
(save-window-excursion
  (let ((caller (generate-new-buffer " *linum-relative-advice-caller*"))
        (target (generate-new-buffer " *linum-relative-advice-target*"))
        (target-window (selected-window))
        caller-window
        (linum-relative-backend 'linum-mode)
        (linum-relative-current-symbol "0")
        (linum-relative-plusp-offset 0)
        (linum-relative-format "%2s")
        (linum-relative-last-pos 99)
        (linum-relative-user-format 'dynamic)
        (linum-format "%04d")
        caller-driven target-driven result)
    (unwind-protect
        (progn
          (delete-other-windows)
          (setq caller-window (split-window-right))
          (set-window-buffer target-window target)
          (set-window-buffer caller-window caller)
          (with-current-buffer target
            (insert "alpha\nbeta\ngamma\ndelta\nepsilon\n")
            (set-buffer-modified-p nil)
            (goto-char (point-min))
            (forward-line 1)
            (linum-relative-mode 1))
          (with-current-buffer caller
            (insert "one\ntwo\nthree\nfour\n")
            (set-buffer-modified-p nil))
          (select-window caller-window)
          ;; `linum-update' promises to update TARGET, but the package's
          ;; load-time advice samples the current caller before GNU Linum
          ;; enters TARGET.  The visible target gutter therefore jumps from
          ;; its own line 2 to the caller's line 4.
          (with-current-buffer caller
            (goto-char (point-min))
            (forward-line 3)
            (linum-update target))
          (setq caller-driven
                (list
                 :selected-buffer (eq (window-buffer (selected-window)) caller)
                 :caller
                 (with-current-buffer caller
                   (list
                    :point (point)
                    :line (line-number-at-pos)
                    :text (buffer-string)
                    :modified (buffer-modified-p)))
                 :target
                 (with-current-buffer target
                   (list
                    :point (point)
                    :line (line-number-at-pos)
                    :text (buffer-string)
                    :modified (buffer-modified-p)
                    :state
                    (neomacs-linum-relative-test-legacy-state target-window)))))
          ;; A normal target-buffer refresh samples the target's own point
          ;; and restores the expected user-visible relative gutter.
          (with-current-buffer target
            (linum-update-current))
          (setq target-driven
                (with-current-buffer target
                  (list
                   :point (point)
                   :line (line-number-at-pos)
                   :state
                   (neomacs-linum-relative-test-legacy-state target-window))))
          (with-current-buffer target
            (linum-relative-mode -1))
          (setq result
                (list
                 :caller-driven caller-driven
                 :target-refresh target-driven
                 :disabled
                 (with-current-buffer target
                   (neomacs-linum-relative-test-legacy-state target-window)))))
      (dolist (buffer (list caller target))
        (when (buffer-live-p buffer)
          (with-current-buffer buffer
            (when linum-relative-mode
              (linum-relative-mode -1))
            (when linum-mode
              (linum-mode -1)))
          (kill-buffer buffer))))
    result))
"##;
    let expect = expect![[
        r#"OK (:caller-driven (:selected-buffer t :caller (:point 15 :line 4 :text "one\ntwo\nthree\nfour\n" :modified nil) :target (:point 7 :line 2 :text "alpha\nbeta\ngamma\ndelta\nepsilon\n" :modified nil :state (:relative-mode t :lighter (:entry (linum-relative-lighter) :value " LR") :linum-mode t :format linum-relative :saved-format "%04d" :last-position 4 :rendered ((:line 1 :position 1 :text " 3" :face linum :margin-display t :same-display-text t) (:line 2 :position 7 :text " 2" :face linum :margin-display t :same-display-text t) (:line 3 :position 12 :text " 1" :face linum :margin-display t :same-display-text t) (:line 4 :position 18 :text " 0" :face linum-relative-current-face :margin-display t :same-display-text t) (:line 5 :position 24 :text " 1" :face linum :margin-display t :same-display-text t)) :left-margin 2 :owned-left-margin 2 :hooks (:post-command 1 :window-scroll 1 :window-configuration 1 :change-major-mode 1)))) :target-refresh (:point 7 :line 2 :state (:relative-mode t :lighter (:entry (linum-relative-lighter) :value " LR") :linum-mode t :format linum-relative :saved-format "%04d" :last-position 2 :rendered ((:line 1 :position 1 :text " 1" :face linum :margin-display t :same-display-text t) (:line 2 :position 7 :text " 0" :face linum-relative-current-face :margin-display t :same-display-text t) (:line 3 :position 12 :text " 1" :face linum :margin-display t :same-display-text t) (:line 4 :position 18 :text " 2" :face linum :margin-display t :same-display-text t) (:line 5 :position 24 :text " 3" :face linum :margin-display t :same-display-text t)) :left-margin 2 :owned-left-margin 2 :hooks (:post-command 1 :window-scroll 1 :window-configuration 1 :change-major-mode 1))) :disabled (:relative-mode nil :lighter (:entry (linum-relative-lighter) :value nil) :linum-mode nil :format "%04d" :saved-format "%04d" :last-position 2 :rendered ((:line 1 :position 1 :text nil :face nil :margin-display nil :same-display-text nil) (:line 2 :position 7 :text nil :face nil :margin-display nil :same-display-text nil) (:line 3 :position 12 :text nil :face nil :margin-display nil :same-display-text nil) (:line 4 :position 18 :text nil :face nil :margin-display nil :same-display-text nil) (:line 5 :position 24 :text nil :face nil :margin-display nil :same-display-text nil)) :left-margin nil :owned-left-margin nil :hooks (:post-command 0 :window-scroll 0 :window-configuration 0 :change-major-mode 0)))"#
    ]];
    ParityBatchCase::value(
        "cross_window_refresh_temporarily_numbers_the_target_from_the_callers_line",
        elisp_form,
        expect,
    )
}

fn global_mode_covers_normal_buffers_and_skips_a_live_helm_buffer() -> ParityBatchCase {
    let elisp_form = r##"
(let ((normal (generate-new-buffer " *linum-relative-global-normal*"))
      (helm (generate-new-buffer " *linum-relative-global-helm*"))
      future
      (linum-relative-backend 'display-line-numbers-mode)
      (linum-relative-user-type t)
      (display-line-numbers-type t)
      enabled disabled result)
  (unwind-protect
      (progn
        (linum-relative-global-mode -1)
        (with-current-buffer normal
          (fundamental-mode)
          (insert "normal\n"))
        (with-current-buffer helm
          (fundamental-mode)
          (setq-local helm-alive-p t)
          (insert "helm candidate\n"))
        (linum-relative-global-mode 1)
        (setq future (generate-new-buffer " *linum-relative-global-future*"))
        (with-current-buffer future
          (text-mode)
          (insert "future\n"))
        (setq enabled
              (list
               :global linum-relative-global-mode
               :listed
               (and (memq 'linum-relative-global-mode global-minor-modes) t)
               :major-mode-hook-count
               (cl-count
                #'linum-relative-global-mode-enable-in-buffer
                after-change-major-mode-hook
                :test #'eq)
               :normal
               (with-current-buffer normal
                 (neomacs-linum-relative-test-native-state))
               :helm
               (with-current-buffer helm
                 (list
                  :helm-live helm-alive-p
                  :state (neomacs-linum-relative-test-native-state)))
               :future
               (with-current-buffer future
                 (list
                  :mode major-mode
                  :state (neomacs-linum-relative-test-native-state)))))
        (linum-relative-global-mode -1)
        (setq disabled
              (list
               :global linum-relative-global-mode
               :listed
               (and (memq 'linum-relative-global-mode global-minor-modes) t)
               :major-mode-hook-count
               (cl-count
                #'linum-relative-global-mode-enable-in-buffer
                after-change-major-mode-hook
                :test #'eq)
               :local-modes
               (mapcar
                (lambda (buffer)
                  (with-current-buffer buffer
                    (list
                     linum-relative-mode
                     display-line-numbers-mode
                     display-line-numbers)))
                (list normal helm future))))
        (setq result (list :enabled enabled :disabled disabled)))
    (when linum-relative-global-mode
      (linum-relative-global-mode -1))
    (dolist (buffer (list normal helm future))
      (when (buffer-live-p buffer)
        (kill-buffer buffer))))
  result)
"##;
    let expect = expect![[
        r#"OK (:enabled (:global t :listed t :major-mode-hook-count 1 :normal (:relative-mode t :native-mode t :type relative :display relative :saved-type relative :legacy-mode nil :current-symbol "0" :relative-format "%3s" :offset 0) :helm (:helm-live t :state (:relative-mode nil :native-mode nil :type relative :display nil :saved-type relative :legacy-mode nil :current-symbol "0" :relative-format "%3s" :offset 0)) :future (:mode text-mode :state (:relative-mode t :native-mode t :type relative :display relative :saved-type relative :legacy-mode nil :current-symbol "0" :relative-format "%3s" :offset 0))) :disabled (:global nil :listed nil :major-mode-hook-count 0 :local-modes ((nil nil nil) (nil nil nil) (nil nil nil))))"#
    ]];
    ParityBatchCase::value(
        "global_mode_covers_normal_buffers_and_skips_a_live_helm_buffer",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn helm_integration_mode_preserves_user_hooks_across_its_lifecycle() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((hook-symbols
        '(helm-move-selection-after-hook
          helm-after-initialize-hook
          helm-after-preselection-hook))
       (package-hooks
        '((helm-move-selection-after-hook . linum-relative-for-helm)
          (helm-after-initialize-hook . helm--turn-on-linum-relative)
          (helm-after-preselection-hook . linum-relative-for-helm)))
       (old-hooks
        (mapcar
         (lambda (hook)
           (list
            hook
            (boundp hook)
            (and (boundp hook) (copy-tree (symbol-value hook)))))
         hook-symbols))
       snapshots mode-hook-events result)
  (unwind-protect
      (progn
        ;; A user may already have Helm navigation hooks before enabling the
        ;; package's public integration mode.  Package teardown must leave
        ;; that unrelated configuration intact.
        (dolist (hook hook-symbols)
          (set hook (list #'ignore)))
        (with-temp-buffer
          (let ((helm-linum-relative-mode-hook
                 (list
                  (lambda ()
                    (push
                     (list
                      :mode helm-linum-relative-mode
                      :listed
                      (and
                       (memq 'helm-linum-relative-mode local-minor-modes)
                       t))
                     mode-hook-events)))))
            (cl-labels
                ((snapshot
                  (phase)
                  (list
                   phase
                   :mode helm-linum-relative-mode
                   :listed
                   (and
                    (memq 'helm-linum-relative-mode local-minor-modes)
                    t)
                   :hooks
                   (mapcar
                    (lambda (entry)
                      (list
                       (car entry)
                       :package
                       (cl-count
                        (cdr entry)
                        (symbol-value (car entry))
                        :test #'eq)
                       :user
                       (cl-count
                        #'ignore
                        (symbol-value (car entry))
                        :test #'eq)))
                    package-hooks))))
              (push (snapshot :initial) snapshots)
              (helm-linum-relative-mode 1)
              (push (snapshot :enabled) snapshots)
              (helm-linum-relative-mode 1)
              (push (snapshot :reenabled) snapshots)
              (helm-linum-relative-mode -1)
              (push (snapshot :disabled) snapshots)
              (setq result
                    (list
                     :snapshots (nreverse snapshots)
                     :mode-hook-events (nreverse mode-hook-events)))))))
    (dolist (entry old-hooks)
      (if (cadr entry)
          (set (car entry) (caddr entry))
        (makunbound (car entry)))))
  result)
"##;
    let expect = expect![
        "OK (:snapshots ((:initial :mode nil :listed nil :hooks ((helm-move-selection-after-hook :package 0 :user 1) (helm-after-initialize-hook :package 0 :user 1) (helm-after-preselection-hook :package 0 :user 1))) (:enabled :mode t :listed t :hooks ((helm-move-selection-after-hook :package 1 :user 1) (helm-after-initialize-hook :package 1 :user 1) (helm-after-preselection-hook :package 1 :user 1))) (:reenabled :mode t :listed t :hooks ((helm-move-selection-after-hook :package 1 :user 1) (helm-after-initialize-hook :package 1 :user 1) (helm-after-preselection-hook :package 1 :user 1))) (:disabled :mode nil :listed nil :hooks ((helm-move-selection-after-hook :package 0 :user 1) (helm-after-initialize-hook :package 0 :user 1) (helm-after-preselection-hook :package 0 :user 1)))) :mode-hook-events ((:mode t :listed t) (:mode t :listed t) (:mode nil :listed nil)))"
    ];
    ParityBatchCase::value(
        "helm_integration_mode_preserves_user_hooks_across_its_lifecycle",
        elisp_form,
        expect,
    )
}

fn invalid_legacy_format_reports_the_error_and_remains_cleanable() -> ParityBatchCase {
    let elisp_form = r##"
(save-window-excursion
  (let ((buffer (generate-new-buffer " *linum-relative-invalid-format*"))
        (window (selected-window))
        (linum-relative-backend 'linum-mode)
        (linum-relative-current-symbol "0")
        (linum-relative-plusp-offset 0)
        (linum-relative-format "%q")
        (linum-relative-last-pos 0)
        (linum-relative-user-format 'dynamic)
        (linum-format "%04d")
        failure partial disabled result)
    (unwind-protect
        (progn
          (delete-other-windows)
          (set-window-buffer window buffer)
          (with-current-buffer buffer
            (insert "first\nsecond\n")
            (set-buffer-modified-p nil)
            (goto-char (point-min))
            (setq failure
                  (condition-case error-data
                      (progn
                        (linum-relative-mode 1)
                        :unexpected-success)
                    (error error-data)))
            (setq partial
                  (neomacs-linum-relative-test-legacy-state window))
            ;; The public disable command must remain usable after enablement
            ;; aborts in the formatter, so users can recover without manually
            ;; repairing Linum hooks or package state.
            (linum-relative-mode -1)
            (setq disabled
                  (neomacs-linum-relative-test-legacy-state window))
            (setq result
                  (list
                   :failure failure
                   :partial-state partial
                   :disabled disabled
                   :buffer
                   (list
                    :text (buffer-string)
                    :point (point)
                    :line (line-number-at-pos)
                    :modified (buffer-modified-p))))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (when linum-relative-mode
            (linum-relative-mode -1))
          (when linum-mode
            (linum-mode -1)))
        (kill-buffer buffer)))
    result))
"##;
    let expect = expect![[
        r#"OK (:failure (error "Invalid format operation %q") :partial-state (:relative-mode t :lighter (:entry (linum-relative-lighter) :value " LR") :linum-mode t :format linum-relative :saved-format "%04d" :last-position 1 :rendered ((:line 1 :position 1 :text nil :face nil :margin-display nil :same-display-text nil) (:line 2 :position 7 :text nil :face nil :margin-display nil :same-display-text nil)) :left-margin nil :owned-left-margin nil :hooks (:post-command 1 :window-scroll 1 :window-configuration 1 :change-major-mode 1)) :disabled (:relative-mode nil :lighter (:entry (linum-relative-lighter) :value nil) :linum-mode nil :format "%04d" :saved-format "%04d" :last-position 1 :rendered ((:line 1 :position 1 :text nil :face nil :margin-display nil :same-display-text nil) (:line 2 :position 7 :text nil :face nil :margin-display nil :same-display-text nil)) :left-margin nil :owned-left-margin nil :hooks (:post-command 0 :window-scroll 0 :window-configuration 0 :change-major-mode 0)) :buffer (:text "first\nsecond\n" :point 1 :line 1 :modified nil))"#
    ]];
    ParityBatchCase::value(
        "invalid_legacy_format_reports_the_error_and_remains_cleanable",
        elisp_form,
        expect,
    )
}

fn documented_legacy_customization_controls_numbers_faces_and_lighter() -> ParityBatchCase {
    let elisp_form = r##"
(save-window-excursion
  (let ((buffer (generate-new-buffer " *linum-relative-customization*"))
        (window (selected-window))
        (linum-relative-backend 'linum-mode)
        (linum-relative-current-symbol "")
        (linum-relative-plusp-offset 2)
        (linum-relative-format "[%s]")
        (linum-relative-lighter " REL")
        (linum-relative-last-pos 0)
        (linum-relative-user-format 'dynamic)
        (linum-format "%04d")
        absolute-current negative-offset disabled result)
    (unwind-protect
        (progn
          (delete-other-windows)
          (set-window-buffer window buffer)
          (with-current-buffer buffer
            (insert "prepare\nbuild\nvalidate\nrelease\nmonitor\n")
            (set-buffer-modified-p nil)
            (goto-char (point-min))
            (forward-line 2)
            (linum-relative-mode 1)
            (setq absolute-current
                  (neomacs-linum-relative-test-legacy-state window))
            (setq linum-relative-current-symbol nil
                  linum-relative-plusp-offset -2)
            (linum-update-current)
            (setq negative-offset
                  (neomacs-linum-relative-test-legacy-state window))
            (linum-relative-mode -1)
            (setq disabled
                  (neomacs-linum-relative-test-legacy-state window))
            (setq result
                  (list
                   :empty-symbol-and-positive-offset absolute-current
                   :nil-symbol-and-negative-offset negative-offset
                   :disabled disabled
                   :buffer
                   (list
                    :text (buffer-string)
                    :point (point)
                    :line (line-number-at-pos)
                    :modified (buffer-modified-p))))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (when linum-relative-mode
            (linum-relative-mode -1))
          (when linum-mode
            (linum-mode -1)))
        (kill-buffer buffer)))
    result))
"##;
    let expect = expect![[
        r#"OK (:empty-symbol-and-positive-offset (:relative-mode t :lighter (:entry (linum-relative-lighter) :value " REL") :linum-mode t :format linum-relative :saved-format "%04d" :last-position 3 :rendered ((:line 1 :position 1 :text "[4]" :face linum :margin-display t :same-display-text t) (:line 2 :position 9 :text "[3]" :face linum :margin-display t :same-display-text t) (:line 3 :position 15 :text "[3]" :face linum-relative-current-face :margin-display t :same-display-text t) (:line 4 :position 24 :text "[3]" :face linum :margin-display t :same-display-text t) (:line 5 :position 32 :text "[4]" :face linum :margin-display t :same-display-text t)) :left-margin 3 :owned-left-margin 3 :hooks (:post-command 1 :window-scroll 1 :window-configuration 1 :change-major-mode 1)) :nil-symbol-and-negative-offset (:relative-mode t :lighter (:entry (linum-relative-lighter) :value " REL") :linum-mode t :format linum-relative :saved-format "%04d" :last-position 3 :rendered ((:line 1 :position 1 :text "[0]" :face linum :margin-display t :same-display-text t) (:line 2 :position 9 :text "[-1]" :face linum :margin-display t :same-display-text t) (:line 3 :position 15 :text "[-2]" :face linum-relative-current-face :margin-display t :same-display-text t) (:line 4 :position 24 :text "[-1]" :face linum :margin-display t :same-display-text t) (:line 5 :position 32 :text "[0]" :face linum :margin-display t :same-display-text t)) :left-margin 4 :owned-left-margin 4 :hooks (:post-command 1 :window-scroll 1 :window-configuration 1 :change-major-mode 1)) :disabled (:relative-mode nil :lighter (:entry (linum-relative-lighter) :value nil) :linum-mode nil :format "%04d" :saved-format "%04d" :last-position 3 :rendered ((:line 1 :position 1 :text nil :face nil :margin-display nil :same-display-text nil) (:line 2 :position 9 :text nil :face nil :margin-display nil :same-display-text nil) (:line 3 :position 15 :text nil :face nil :margin-display nil :same-display-text nil) (:line 4 :position 24 :text nil :face nil :margin-display nil :same-display-text nil) (:line 5 :position 32 :text nil :face nil :margin-display nil :same-display-text nil)) :left-margin nil :owned-left-margin nil :hooks (:post-command 0 :window-scroll 0 :window-configuration 0 :change-major-mode 0)) :buffer (:text "prepare\nbuild\nvalidate\nrelease\nmonitor\n" :point 15 :line 3 :modified nil))"#
    ]];
    ParityBatchCase::value(
        "documented_legacy_customization_controls_numbers_faces_and_lighter",
        elisp_form,
        expect,
    )
}

#[test]
fn linum_relative_package_batch() {
    let cases = vec![
        legacy_backend_renders_and_recenters_a_real_relative_gutter(),
        legacy_toggle_exposes_ownership_of_an_existing_absolute_gutter(),
        native_backend_toggles_relative_and_restores_the_visual_policy(),
        native_backend_two_buffer_workflow_surfaces_shared_saved_policy(),
        legacy_backend_two_buffer_workflow_surfaces_shared_saved_format(),
        cross_window_refresh_temporarily_numbers_the_target_from_the_callers_line(),
        global_mode_covers_normal_buffers_and_skips_a_live_helm_buffer(),
        helm_integration_mode_preserves_user_hooks_across_its_lifecycle(),
        invalid_legacy_format_reports_the_error_and_remains_cleanable(),
        documented_legacy_customization_controls_numbers_faces_and_lighter(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("unnamed linum-relative parity test");
    assert_oracle_batch_cases(
        linum_relative_oracle(),
        test_name,
        "linum_relative_parity",
        &cases,
    );
}
