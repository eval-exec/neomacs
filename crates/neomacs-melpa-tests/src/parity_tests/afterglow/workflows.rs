use expect_test::expect;

use super::ParityBatchCase;

/// The package in one sentence: press the key, the line you landed on glows,
/// and a moment later the glow is gone.  The overlay's bounds are the line's,
/// it carries the default `hl-line' face at priority 100 so it wins over a
/// region, it lives in the buffer the command ran in, and the timer the package
/// scheduled for the configured duration is what removes it.
fn a_trigger_glows_the_line_the_command_moved_to_until_its_timer_fires() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_trigger_glows_the_line_the_command_moved_to_until_its_timer_fires",
        r##"(unwind-protect
    (let ((buffer (afterglow-test-buffer)))
      (global-set-key (kbd "C-c m") #'afterglow-test-move)
      (afterglow-mode 1)
      (afterglow-add-trigger 'afterglow-test-move :thing 'line :duration 0.2)
      (let ((armed (afterglow-test-state 'afterglow-test-move))
            (known (copy-sequence timer-list))
            (start (float-time)))
        (execute-kbd-macro (kbd "C-c m"))
        (let ((glowing (list :point (point)
                             :line (line-number-at-pos)
                             :overlays (afterglow-test-overlays)
                             :delays (afterglow-test-delays known start)
                             :timer-count (length (afterglow-test-new-timers known)))))
          (let ((fired (afterglow-test-run-new-timers known)))
            (list :armed armed
                  :glowing glowing
                  :fired fired
                  :after (list :overlays (afterglow-test-overlays)
                               :overlay-live (and afterglow--temp-overlay
                                                  (overlay-buffer afterglow--temp-overlay)
                                                  t)
                               :pending (length (afterglow-test-new-timers known))
                               :buffer (buffer-name buffer)
                               :point (point)))))))
  (afterglow-test-cleanup))"##,
        expect![[
            r#"OK (:armed (:mode t :triggers 1 :advised (afterglow-test-move) :armed ((afterglow-test-move . t))) :glowing (:point 18 :line 2 :overlays ((18 36 hl-line 100 "*afterglow-workflow*")) :delays (2) :timer-count 1) :fired 1 :after (:overlays nil :overlay-live nil :pending 0 :buffer "*afterglow-workflow*" :point 18))"#
        ]],
    )
}

fn trigger_properties_choose_what_the_glow_covers() -> ParityBatchCase {
    ParityBatchCase::value(
        "trigger_properties_choose_what_the_glow_covers",
        r##"(unwind-protect
    (progn
      (afterglow-test-buffer)
      (global-set-key (kbd "C-c n") #'afterglow-test-command)
      (afterglow-mode 1)
      (transient-mark-mode 1)
      (let (results)
        (dolist (spec '((word (:thing word :duration 0.3) 7)
                        (line-width (:thing line :width 5 :duration 0.3) 7)
                        (custom-function
                         (:thing afterglow-test-bounds :duration 0.3 :face highlight)
                         7)
                        (window (:thing window :duration 0.3) 7)
                        (empty-line (:thing line :duration 0.3) 52)))
          (let ((known (copy-sequence timer-list)))
            (apply #'afterglow-add-trigger 'afterglow-test-command (nth 1 spec))
            (goto-char (nth 2 spec))
            (execute-kbd-macro (kbd "C-c n"))
            (push (list (car spec)
                        :line (line-number-at-pos)
                        :overlays (afterglow-test-overlays)
                        :timers (length (afterglow-test-new-timers known)))
                  results)
            (afterglow-test-run-new-timers known)))
        (let ((known (copy-sequence timer-list)))
          (afterglow-add-trigger 'afterglow-test-command :thing 'region :duration 0.3)
          (goto-char 1)
          (set-mark 1)
          (goto-char 6)
          (activate-mark)
          (execute-kbd-macro (kbd "C-c n"))
          (push (list 'region
                      :region-active (region-active-p)
                      :bounds (region-bounds)
                      :overlays (afterglow-test-overlays))
                results)
          (afterglow-test-run-new-timers known)
          (deactivate-mark))
        (list :window-bounds (cons (window-start) (window-end nil t))
              :results (nreverse results))))
  (afterglow-test-cleanup))"##,
        expect![[
            r#"OK (:window-bounds (1 . 69) :results ((word :line 1 :overlays ((7 11 hl-line 100 "*afterglow-workflow*")) :timers 1) (line-width :line 1 :overlays ((1 6 hl-line 100 "*afterglow-workflow*")) :timers 1) (custom-function :line 1 :overlays ((7 11 highlight 100 "*afterglow-workflow*")) :timers 1) (window :line 1 :overlays ((1 69 hl-line 100 "*afterglow-workflow*")) :timers 1) (empty-line :line 4 :overlays nil :timers 0) (region :region-active t :bounds ((1 . 6)) :overlays ((1 6 hl-line 100 "*afterglow-workflow*")))))"#
        ]],
    )
    .fresh_process()
}

fn the_duration_and_face_customizations_change_the_overlay_itself() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_duration_and_face_customizations_change_the_overlay_itself",
        r##"(unwind-protect
    (progn
      (afterglow-test-buffer)
      (global-set-key (kbd "C-c n") #'afterglow-test-command)
      (afterglow-mode 1)
      (let (results)
        (dolist (spec '((stock-defaults 1 hl-line (:thing line))
                        (custom-defaults 2 error (:thing line))
                        (trigger-overrides 2 error (:thing line :duration 0.5
                                                   :face success))))
          (setq afterglow-default-duration (nth 1 spec)
                afterglow-default-face (nth 2 spec))
          (apply #'afterglow-add-trigger 'afterglow-test-command (nth 3 spec))
          (goto-char 7)
          (let ((known (copy-sequence timer-list))
                (start (float-time)))
            (execute-kbd-macro (kbd "C-c n"))
            (push (list (car spec)
                        :default-duration afterglow-default-duration
                        :default-face afterglow-default-face
                        :overlays (afterglow-test-overlays)
                        :delays (afterglow-test-delays known start))
                  results)
            (afterglow-test-run-new-timers known)))
        (nreverse results)))
  (afterglow-test-cleanup))"##,
        expect![[
            r#"OK ((stock-defaults :default-duration 1 :default-face hl-line :overlays ((1 17 hl-line 100 "*afterglow-workflow*")) :delays (10)) (custom-defaults :default-duration 2 :default-face error :overlays ((1 17 error 100 "*afterglow-workflow*")) :delays (20)) (trigger-overrides :default-duration 2 :default-face error :overlays ((1 17 success 100 "*afterglow-workflow*")) :delays (5)))"#
        ]],
    )
    .fresh_process()
}

fn adding_and_removing_triggers_arms_and_disarms_the_advice() -> ParityBatchCase {
    ParityBatchCase::value(
        "adding_and_removing_triggers_arms_and_disarms_the_advice",
        r##"(unwind-protect
    (progn
      (afterglow-test-buffer)
      (global-set-key (kbd "C-c n") #'afterglow-test-command)
      (global-set-key (kbd "C-c m") #'afterglow-test-move)
      (let ((before (afterglow-test-state 'afterglow-test-command
                                          'afterglow-test-move)))
        (afterglow-add-triggers
         '((afterglow-test-command :thing line :duration 0.2)
           (afterglow-test-move :thing word :duration 0.2)))
        (let ((added (afterglow-test-state 'afterglow-test-command
                                           'afterglow-test-move)))
          (afterglow-add-trigger 'afterglow-test-command :thing 'word
                                 :duration 0.2)
          (let ((replaced (afterglow-test-state 'afterglow-test-command
                                                'afterglow-test-move))
                (known (copy-sequence timer-list)))
            (goto-char 7)
            (execute-kbd-macro (kbd "C-c n"))
            (let ((glowing (afterglow-test-overlays)))
              (afterglow-test-run-new-timers known)
              (afterglow-remove-triggers '(afterglow-test-command
                                           afterglow-test-move))
              (let ((removed (afterglow-test-state 'afterglow-test-command
                                                   'afterglow-test-move))
                    (known2 (copy-sequence timer-list)))
                (goto-char 7)
                (execute-kbd-macro (kbd "C-c n"))
                (list :before before
                      :added added
                      :replaced replaced
                      :glowing glowing
                      :removed removed
                      :after-removal
                      (list :overlays (afterglow-test-overlays)
                            :timers (length (afterglow-test-new-timers known2))))))))))
  (afterglow-test-cleanup))"##,
        expect![[
            r#"OK (:before (:mode nil :triggers 0 :advised nil :armed ((afterglow-test-command) (afterglow-test-move))) :added (:mode nil :triggers 2 :advised (afterglow-test-command afterglow-test-move) :armed ((afterglow-test-command . t) (afterglow-test-move . t))) :replaced (:mode nil :triggers 2 :advised (afterglow-test-command afterglow-test-move) :armed ((afterglow-test-command . t) (afterglow-test-move . t))) :glowing ((7 11 hl-line 100 "*afterglow-workflow*")) :removed (:mode nil :triggers 0 :advised nil :armed ((afterglow-test-command) (afterglow-test-move))) :after-removal (:overlays nil :timers 0))"#
        ]],
    )
    .fresh_process()
}

fn switching_the_mode_off_stops_new_glows_but_leaves_the_last_one_on_screen() -> ParityBatchCase {
    ParityBatchCase::value(
        "switching_the_mode_off_stops_new_glows_but_leaves_the_last_one_on_screen",
        r##"(unwind-protect
    (progn
      (afterglow-test-buffer)
      (global-set-key (kbd "C-c n") #'afterglow-test-command)
      (afterglow-mode 1)
      (afterglow-add-trigger 'afterglow-test-command :thing 'line :duration 0.2)
      (goto-char 7)
      (let ((known (copy-sequence timer-list)))
        (execute-kbd-macro (kbd "C-c n"))
        (let ((glowing (list :overlays (afterglow-test-overlays)
                             :properties (overlay-properties afterglow--temp-overlay)
                             :state (afterglow-test-state 'afterglow-test-command))))
          (afterglow-mode 0)
          (let ((switched-off
                 (list :overlays (afterglow-test-overlays)
                       :state (afterglow-test-state 'afterglow-test-command)
                       :pending (length (afterglow-test-new-timers known)))))
            (goto-char 25)
            (execute-kbd-macro (kbd "C-c n"))
            (let ((no-new-glow (list :point (point)
                                     :overlays (afterglow-test-overlays)
                                     :timers (length (afterglow-test-new-timers known)))))
              (afterglow-test-run-new-timers known)
              (list :glowing glowing
                    :switched-off switched-off
                    :no-new-glow no-new-glow
                    :after-timer (afterglow-test-overlays)))))))
  (afterglow-test-cleanup))"##,
        expect![[
            r#"OK (:glowing (:overlays ((1 17 hl-line 100 "*afterglow-workflow*")) :properties (priority 100 face hl-line) :state (:mode t :triggers 1 :advised (afterglow-test-command) :armed ((afterglow-test-command . t)))) :switched-off (:overlays ((1 17 hl-line 100 "*afterglow-workflow*")) :state (:mode nil :triggers 1 :advised nil :armed ((afterglow-test-command))) :pending 1) :no-new-glow (:point 25 :overlays ((1 17 hl-line 100 "*afterglow-workflow*")) :timers 1) :after-timer nil)"#
        ]],
    )
    .fresh_process()
}

fn a_second_glow_makes_the_first_timer_cancel_it_early() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_second_glow_makes_the_first_timer_cancel_it_early",
        r##"(unwind-protect
    (progn
      (afterglow-test-buffer)
      (global-set-key (kbd "C-c m") #'afterglow-test-move)
      (afterglow-mode 1)
      (afterglow-add-trigger 'afterglow-test-move :thing 'line :duration 0.2)
      (let ((known (copy-sequence timer-list)))
        (execute-kbd-macro (kbd "C-c m"))
        (let ((first-glow (afterglow-test-overlays))
              (first-timer (car (afterglow-test-new-timers known))))
          (execute-kbd-macro (kbd "C-c m"))
          (let ((second-glow (list :overlays (afterglow-test-overlays)
                                   :pending (length (afterglow-test-new-timers known)))))
            (timer-event-handler first-timer)
            (let ((after-first-timer
                   (list :overlays (afterglow-test-overlays)
                         :pending (length (afterglow-test-new-timers known)))))
              (afterglow-test-run-new-timers known)
              (list :first-glow first-glow
                    :second-glow second-glow
                    :after-first-timer after-first-timer
                    :after-both (afterglow-test-overlays)))))))
  (afterglow-test-cleanup))"##,
        expect![[
            r#"OK (:first-glow ((18 36 hl-line 100 "*afterglow-workflow*")) :second-glow (:overlays ((37 51 hl-line 100 "*afterglow-workflow*")) :pending 2) :after-first-timer (:overlays nil :pending 1) :after-both nil)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        a_trigger_glows_the_line_the_command_moved_to_until_its_timer_fires(),
        trigger_properties_choose_what_the_glow_covers(),
        the_duration_and_face_customizations_change_the_overlay_itself(),
        adding_and_removing_triggers_arms_and_disarms_the_advice(),
        switching_the_mode_off_stops_new_glows_but_leaves_the_last_one_on_screen(),
        a_second_glow_makes_the_first_timer_cancel_it_early(),
    ]
}
