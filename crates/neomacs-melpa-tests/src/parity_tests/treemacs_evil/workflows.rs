use expect_test::expect;

use super::ParityBatchCase;

fn treemacs_buffers_enter_the_dedicated_evil_state_with_motion_enabled() -> ParityBatchCase {
    ParityBatchCase::value(
        "treemacs_buffers_enter_the_dedicated_evil_state_with_motion_enabled",
        r##"
(let ((neomacs-treemacs-evil-workflow-buffer
       (neomacs-treemacs-evil-test-buffer)))
  (unwind-protect
      (with-current-buffer neomacs-treemacs-evil-workflow-buffer
        (list :major-mode major-mode
              :evil-local evil-local-mode
              :state evil-state
              :initial (evil-initial-state-for-buffer)
              :cursor (evil-state-property 'treemacs :cursor)
              :enable (evil-state-property 'treemacs :enable)
              :state-mode evil-treemacs-state-minor-mode
              :motion-mode evil-motion-state-minor-mode))
    (neomacs-treemacs-evil-test-kill
     neomacs-treemacs-evil-workflow-buffer)))
"##,
        expect![
            "OK (:major-mode treemacs-mode :evil-local t :state treemacs :initial treemacs :cursor evil-treemacs-state-cursor :enable (motion) :state-mode t :motion-mode t)"
        ],
    )
}

fn j_and_k_navigate_real_treemacs_buttons_in_state_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "j_and_k_navigate_real_treemacs_buttons_in_state_order",
        r##"
(let ((neomacs-treemacs-evil-workflow-buffer
       (neomacs-treemacs-evil-test-buffer)))
  (unwind-protect
      (with-current-buffer neomacs-treemacs-evil-workflow-buffer
        (let (states)
          (dolist (command
                   (list nil
                         (neomacs-treemacs-evil-test-state-binding "j")
                         (neomacs-treemacs-evil-test-state-binding "j")
                         (neomacs-treemacs-evil-test-state-binding "k")))
            (when command (call-interactively command))
            (push (list :line (line-number-at-pos)
                        :label (button-label (button-at (point)))
                        :state evil-state)
                  states))
          (nreverse states)))
    (neomacs-treemacs-evil-test-kill
     neomacs-treemacs-evil-workflow-buffer)))
"##,
        expect![[
            r#"OK ((:line 1 :label "Project" :state treemacs) (:line 2 :label "src" :state treemacs) (:line 3 :label "main.el" :state treemacs) (:line 2 :label "src" :state treemacs))"#
        ]],
    )
}

fn state_and_mode_maps_route_navigation_actions_toggles_and_paths() -> ParityBatchCase {
    ParityBatchCase::value(
        "state_and_mode_maps_route_navigation_actions_toggles_and_paths",
        r##"
(list
 :state
 (mapcar (lambda (key)
           (cons key (neomacs-treemacs-evil-test-state-binding key)))
         '("j" "k" "M-j" "M-k" "th" "tf" "ta" "w"
           ">" "<" "RET" "TAB" "H" "!" "=" "W"))
 :mode
 (mapcar (lambda (key)
           (cons key (neomacs-treemacs-evil-test-mode-binding key)))
         '("yp" "ya" "yr" "yn" "yf" "yv" "gr" "h" "l" "RET"))
 :mouse
 (list
  (let ((map (evil-get-auxiliary-keymap treemacs-mode-map 'treemacs)))
    (lookup-key map [down-mouse-1]))
  (let ((map (evil-get-auxiliary-keymap treemacs-mode-map 'treemacs)))
    (lookup-key map [drag-mouse-1]))))
"##,
        expect![[
            r#"OK (:state (("j" . treemacs-next-line) ("k" . treemacs-previous-line) ("M-j" . treemacs-next-neighbour) ("M-k" . treemacs-previous-neighbour) ("th" . treemacs-toggle-show-dotfiles) ("tf" . treemacs-follow-mode) ("ta" . treemacs-filewatch-mode) ("w" . treemacs-set-width) (">" . treemacs-increase-width) ("<" . treemacs-decrease-width) ("RET" . treemacs-RET-action) ("TAB" . treemacs-TAB-action) ("H" . treemacs-collapse-parent-node) ("!" . treemacs-run-shell-command-for-current-node) ("=" . treemacs-fit-window-width) ("W" . treemacs-extra-wide-toggle)) :mode (("yp" . treemacs-copy-project-path-at-point) ("ya" . treemacs-copy-absolute-path-at-point) ("yr" . treemacs-copy-relative-path-at-point) ("yn" . treemacs-copy-filename-at-point) ("yf" . treemacs-copy-file) ("yv" . treemacs-paste-dir-at-point-to-minibuffer) ("gr" . treemacs-refresh) ("h" . treemacs-COLLAPSE-action) ("l" . treemacs-RET-action) ("RET" . treemacs-RET-action)) :mouse (treemacs-leftclick-action treemacs-dragleftclick-action))"#
        ]],
    )
}

fn click_recovery_returns_the_local_buffer_from_visual_to_treemacs_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "click_recovery_returns_the_local_buffer_from_visual_to_treemacs_state",
        r##"
(let ((neomacs-treemacs-evil-workflow-buffer
       (neomacs-treemacs-evil-test-buffer)))
  (unwind-protect
      (with-current-buffer neomacs-treemacs-evil-workflow-buffer
        (evil-visual-state)
        (let ((before evil-state))
          (cl-letf (((symbol-function 'treemacs-get-local-buffer)
                     (lambda ()
                       neomacs-treemacs-evil-workflow-buffer)))
            (treemacs-evil---turn-off-visual-state-after-click))
          (list :before before
                :after evil-state
                :treemacs-mode evil-treemacs-state-minor-mode
                :visual-mode evil-visual-state-minor-mode)))
    (neomacs-treemacs-evil-test-kill
     neomacs-treemacs-evil-workflow-buffer)))
"##,
        expect!["OK (:before visual :after treemacs :treemacs-mode t :visual-mode nil)"],
    )
}

fn window_move_compatibility_closes_calls_original_and_reopens_active_treemacs() -> ParityBatchCase
{
    ParityBatchCase::value(
        "window_move_compatibility_closes_calls_original_and_reopens_active_treemacs",
        r##"
(let ((window (selected-window))
      events)
  (cl-letf (((symbol-function 'treemacs-get-local-window)
             (lambda () window))
            ((symbol-function 'treemacs)
             (lambda () (push :treemacs events))))
    (let ((result
           (treemacs-evil--window-move-compatibility-advice
            (lambda (&rest args)
              (push (cons :original args) events)
              :moved)
            'left 3)))
      (list :result result
            :events (nreverse events)
            :window-live (window-live-p window)))))
"##,
        expect![
            "OK (:result #1=(:treemacs) :events (:treemacs (:original left 3) . #1#) :window-live t)"
        ],
    )
}

fn integration_advices_are_registered_on_window_moves_and_mouse_actions() -> ParityBatchCase {
    ParityBatchCase::value(
        "integration_advices_are_registered_on_window_moves_and_mouse_actions",
        r##"
(list
 :window-moves
 (mapcar
  (lambda (function)
    (cons function
          (and
           (advice-member-p
            #'treemacs-evil--window-move-compatibility-advice function)
           t)))
  '(evil-window-move-far-left
    evil-window-move-far-right
    evil-window-move-very-top
    evil-window-move-very-bottom))
 :mouse
 (list
  (and
   (advice-member-p
    #'treemacs-evil---turn-off-visual-state-after-click
    'treemacs-leftclick-action)
   t)
  (and
   (advice-member-p
    #'treemacs-evil---turn-off-visual-state-after-click
    'treemacs-doubleclick-action)
   t)))
"##,
        expect![
            "OK (:window-moves ((evil-window-move-far-left . t) (evil-window-move-far-right . t) (evil-window-move-very-top . t) (evil-window-move-very-bottom . t)) :mouse (t t))"
        ],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        treemacs_buffers_enter_the_dedicated_evil_state_with_motion_enabled(),
        j_and_k_navigate_real_treemacs_buttons_in_state_order(),
        state_and_mode_maps_route_navigation_actions_toggles_and_paths(),
        click_recovery_returns_the_local_buffer_from_visual_to_treemacs_state(),
        window_move_compatibility_closes_calls_original_and_reopens_active_treemacs(),
        integration_advices_are_registered_on_window_moves_and_mouse_actions(),
    ]
}
