use expect_test::expect;

use super::ParityBatchCase;

fn local_mode_centers_a_real_window_and_owns_hooks_keys_and_lighter() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-ccm-test-with-window
 "local-lifecycle" 100
 (lambda (_buffer)
   (goto-char (point-min))
   (forward-line 59)
   (let ((ccm-vpos-init 5)
         (ccm-vpos-inverted 1)
         (ccm-step-delay 0)
         (ccm-step-size 100))
     (centered-cursor-mode 1)
     (redisplay t)
     (let ((enabled
            (list
             :mode centered-cursor-mode
             :vpos ccm-vpos
             :lighter (assq 'centered-cursor-mode minor-mode-alist)
             :post-command
             (neomacs-ccm-test-hook-member
              'ccm-position-cursor 'post-command-hook)
             :window-change
             (neomacs-ccm-test-hook-member
              'ccm-vpos-recenter 'window-configuration-change-hook)
             :recenter-key (key-binding (kbd "C-M-0"))
             :page-down-key (key-binding (kbd "C-v"))
             :window (neomacs-ccm-test-window-state))))
       (centered-cursor-mode -1)
       (list
        :enabled enabled
        :disabled
        (list :mode centered-cursor-mode
              :post-command
              (neomacs-ccm-test-hook-member
               'ccm-position-cursor 'post-command-hook)
              :window-change
              (neomacs-ccm-test-hook-member
               'ccm-vpos-recenter 'window-configuration-change-hook)
              :window (neomacs-ccm-test-window-state)))))))
"##;
    let expect = expect![[
        r#"OK (:enabled (:mode t :vpos 5 :lighter (centered-cursor-mode " ¢") :post-command (ccm-position-cursor) :window-change (ccm-vpos-recenter) :recenter-key ccm-vpos-recenter :page-down-key ccm-scroll-up :window (:point 2361 :point-line 60 :start 2161 :start-line 55 :end 4001 :end-line 101 :body-height 22 :text-height 22 :selected-visible-lines 22)) :disabled (:mode nil :post-command nil :window-change nil :window (:point 2361 :point-line 60 :start 2161 :start-line 55 :end 4001 :end-line 101 :body-height 22 :text-height 22 :selected-visible-lines 22)))"#
    ]];
    ParityBatchCase::value(
        "local_mode_centers_a_real_window_and_owns_hooks_keys_and_lighter",
        elisp_form,
        expect,
    )
}

fn post_command_recenters_normal_motion_but_respects_ignored_mouse_commands() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-ccm-test-with-window
 "post-command" 100
 (lambda (_buffer)
   (let ((ccm-vpos-init 4)
         (ccm-vpos-inverted 1)
         (ccm-step-delay 0)
         (ccm-step-size 100))
     (goto-char (point-min))
     (forward-line 19)
     (centered-cursor-mode 1)
     (redisplay t)
     (let ((initial (neomacs-ccm-test-window-state)))
       (forward-line 17)
       (let ((this-command 'next-line)
             (last-command 'next-line)
             (last-command-event ?n))
         (run-hooks 'post-command-hook))
       (redisplay t)
       (let ((normal-motion (neomacs-ccm-test-window-state)))
         (forward-line 2)
         (let ((this-command 'mouse-set-point)
               (last-command 'mouse-set-point)
               (last-command-event nil))
           (run-hooks 'post-command-hook))
         (let ((ignored-motion (neomacs-ccm-test-window-state)))
           (centered-cursor-mode -1)
           (list :initial initial
                 :normal-motion normal-motion
                 :ignored-motion ignored-motion)))))))
"##;
    let expect = expect![
        "OK (:initial (:point 761 :point-line 20 :start 601 :start-line 16 :end 4001 :end-line 101 :body-height 22 :text-height 22 :selected-visible-lines 22) :normal-motion (:point 1441 :point-line 37 :start 1281 :start-line 33 :end 4001 :end-line 101 :body-height 22 :text-height 22 :selected-visible-lines 22) :ignored-motion (:point 1521 :point-line 39 :start 1281 :start-line 33 :end 4001 :end-line 101 :body-height 22 :text-height 22 :selected-visible-lines 22))"
    ];
    ParityBatchCase::value(
        "post_command_recenters_normal_motion_but_respects_ignored_mouse_commands",
        elisp_form,
        expect,
    )
}

fn end_of_file_policy_switches_between_bottom_alignment_and_fixed_cursor_position()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-ccm-test-with-window
 "end-of-file" 40
 (lambda (_buffer)
   (goto-char (point-max))
   (forward-line -2)
   (let ((ccm-vpos 4)
         (ccm-vpos-inverted 1)
         (ccm-step-delay 0)
         (ccm-step-size 100)
         (ccm-inhibit-centering-when nil)
         (ccm-recenter-at-end-of-file nil))
     (setq ccm-recenter-at-end-of-file nil)
     (ccm-position-cursor)
     (redisplay t)
     (let ((bottom-aligned (neomacs-ccm-test-window-state)))
       (set-window-start (selected-window) (point-min))
       (setq ccm-recenter-at-end-of-file t)
       (ccm-position-cursor)
       (redisplay t)
       (list :bottom-aligned bottom-aligned
             :fixed-position (neomacs-ccm-test-window-state))))))
"##;
    let expect = expect![
        "OK (:bottom-aligned (:point 1521 :point-line 39 :start 761 :start-line 20 :end 1601 :end-line 41 :body-height 22 :text-height 22 :selected-visible-lines 22) :fixed-position (:point 1521 :point-line 39 :start 1361 :start-line 35 :end 1601 :end-line 41 :body-height 22 :text-height 22 :selected-visible-lines 22))"
    ];
    ParityBatchCase::value(
        "end_of_file_policy_switches_between_bottom_alignment_and_fixed_cursor_position",
        elisp_form,
        expect,
    )
}

fn vertical_position_adjustments_clamp_top_and_bottom_anchors_to_live_viewport() -> ParityBatchCase
{
    let elisp_form = r##"
(neomacs-ccm-test-with-window
 "position-adjustments" 30
 (lambda (_buffer)
   (let ((visible (ccm-visible-text-lines))
         positive-max
         positive-step-back
         negative-max
         negative-step-back)
     (setq ccm-vpos 0)
     (ccm-vpos-down 1000)
     (setq positive-max ccm-vpos)
     (ccm-vpos-up 3)
     (setq positive-step-back ccm-vpos)
     (setq ccm-vpos -1)
     (ccm-vpos-down 1000)
     (setq negative-max ccm-vpos)
     (ccm-vpos-up 3)
     (setq negative-step-back ccm-vpos)
     (setq ccm-vpos-init 6
           ccm-vpos-inverted -1)
     (ccm-vpos-recenter)
     (list :visible visible
           :positive-max positive-max
           :positive-step-back positive-step-back
           :negative-max negative-max
           :negative-step-back negative-step-back
           :inverted-center ccm-vpos))))
"##;
    let expect = expect![
        "OK (:visible 22 :positive-max 21 :positive-step-back 18 :negative-max -22 :negative-step-back -19 :inverted-center -6)"
    ];
    ParityBatchCase::value(
        "vertical_position_adjustments_clamp_top_and_bottom_anchors_to_live_viewport",
        elisp_form,
        expect,
    )
}

fn paging_commands_move_the_cursor_and_mode_keymap_routes_standard_page_keys() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-ccm-test-with-window
 "paging" 120
 (lambda (_buffer)
   (goto-char (point-min))
   (forward-line 49)
   (let ((ccm-vpos-init 5)
         (ccm-vpos-inverted 1)
         (ccm-step-delay 0)
         (ccm-step-size 100))
     (centered-cursor-mode 1)
     (redisplay t)
     (let ((initial (neomacs-ccm-test-window-state))
           (bindings
            (mapcar (lambda (key) (list key (key-binding (kbd key))))
                    '("C-v" "M-v" "<next>" "<prior>"
                      "C-M--" "C-M-=" "C-M-0"))))
       (ccm-scroll-up 8)
       (ccm-position-cursor)
       (redisplay t)
       (let ((paged-down (neomacs-ccm-test-window-state)))
         (ccm-scroll-down 3)
         (ccm-position-cursor)
         (redisplay t)
         (let ((paged-up (neomacs-ccm-test-window-state)))
           (centered-cursor-mode -1)
           (list :bindings bindings
                 :initial initial
                 :paged-down paged-down
                 :paged-up paged-up)))))))
"##;
    let expect = expect![[
        r#"OK (:bindings (("C-v" ccm-scroll-up) ("M-v" ccm-scroll-down) ("<next>" ccm-scroll-up) ("<prior>" ccm-scroll-down) ("C-M--" ccm-vpos-up) ("C-M-=" ccm-vpos-down) ("C-M-0" ccm-vpos-recenter)) :initial (:point 1961 :point-line 50 :start 1761 :start-line 45 :end 4801 :end-line 121 :body-height 22 :text-height 22 :selected-visible-lines 22) :paged-down (:point 2281 :point-line 58 :start 2081 :start-line 53 :end 4801 :end-line 121 :body-height 22 :text-height 22 :selected-visible-lines 22) :paged-up (:point 2161 :point-line 55 :start 1961 :start-line 50 :end 4801 :end-line 121 :body-height 22 :text-height 22 :selected-visible-lines 22))"#
    ]];
    ParityBatchCase::value(
        "paging_commands_move_the_cursor_and_mode_keymap_routes_standard_page_keys",
        elisp_form,
        expect,
    )
}

fn two_windows_showing_one_buffer_recenter_independently_when_selected() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-ccm-test-with-window
 "two-windows" 120
 (lambda (buffer)
   (let* ((top (selected-window))
          (bottom (split-window-below))
          (ccm-vpos-init 3)
          (ccm-vpos-inverted 1)
          (ccm-step-delay 0)
          (ccm-step-size 100))
     (set-window-buffer bottom buffer)
     (with-current-buffer buffer
       (goto-char (point-min))
       (forward-line 24)
       (set-window-point top (point))
       (goto-char (point-min))
       (forward-line 84)
       (set-window-point bottom (point)))
     (set-window-start top (point-min))
     (with-current-buffer buffer
       (goto-char (point-min))
       (forward-line 60)
       (set-window-start bottom (point)))
     (select-window top)
     (centered-cursor-mode 1)
     (ccm-position-cursor)
     (redisplay t)
     (let ((top-centered (neomacs-ccm-test-window-state top))
           (bottom-before (neomacs-ccm-test-window-state bottom)))
       (select-window bottom)
       (ccm-position-cursor)
       (redisplay t)
       (let ((top-after (neomacs-ccm-test-window-state top))
             (bottom-centered (neomacs-ccm-test-window-state bottom)))
         (centered-cursor-mode -1)
         (list :top-centered top-centered
               :bottom-before bottom-before
               :top-after top-after
               :bottom-centered bottom-centered))))))
"##;
    let expect = expect![
        "OK (:top-centered (:point 2401 :point-line 61 :start 2281 :start-line 58 :end 4801 :end-line 121 :body-height 11 :text-height 11 :selected-visible-lines 11) :bottom-before (:point 3361 :point-line 85 :start 2401 :start-line 61 :end 4801 :end-line 121 :body-height 10 :text-height 10) :top-after (:point 2401 :point-line 61 :start 2281 :start-line 58 :end 4801 :end-line 121 :body-height 11 :text-height 11) :bottom-centered (:point 3361 :point-line 85 :start 3241 :start-line 82 :end 4801 :end-line 121 :body-height 10 :text-height 10 :selected-visible-lines 10))"
    ];
    ParityBatchCase::value(
        "two_windows_showing_one_buffer_recenter_independently_when_selected",
        elisp_form,
        expect,
    )
}

fn global_mode_tracks_existing_and_future_buffers_then_removes_local_hooks() -> ParityBatchCase {
    let elisp_form = r##"
(let ((first (generate-new-buffer "ccm-global-first.txt"))
      (second (generate-new-buffer "ccm-global-second.txt"))
      future)
  (unwind-protect
      (progn
        (when global-centered-cursor-mode
          (global-centered-cursor-mode -1))
        (dolist (buffer (list first second))
          (with-current-buffer buffer
            (text-mode)
            (insert "one\ntwo\nthree\n")))
        (global-centered-cursor-mode 1)
        (setq future (generate-new-buffer "ccm-global-future.txt"))
        (with-current-buffer future
          (text-mode)
          (insert "future\nbuffer\n"))
        (let ((enabled
               (mapcar
                (lambda (buffer)
                  (with-current-buffer buffer
                    (list (buffer-name)
                          major-mode
                          centered-cursor-mode
                          (neomacs-ccm-test-hook-member
                           'ccm-position-cursor 'post-command-hook))))
                (list first second future))))
          (global-centered-cursor-mode -1)
          (list
           :global global-centered-cursor-mode
           :enabled enabled
           :disabled
           (mapcar
            (lambda (buffer)
              (with-current-buffer buffer
                (list (buffer-name)
                      centered-cursor-mode
                      (neomacs-ccm-test-hook-member
                       'ccm-position-cursor 'post-command-hook))))
            (list first second future)))))
    (when global-centered-cursor-mode
      (global-centered-cursor-mode -1))
    (dolist (buffer (list first second future))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))
"##;
    let expect = expect![[
        r#"OK (:global nil :enabled (("ccm-global-first.txt" text-mode t (ccm-position-cursor)) ("ccm-global-second.txt" text-mode t (ccm-position-cursor)) ("ccm-global-future.txt" text-mode t (ccm-position-cursor))) :disabled (("ccm-global-first.txt" nil nil) ("ccm-global-second.txt" nil nil) ("ccm-global-future.txt" nil nil)))"#
    ]];
    ParityBatchCase::value(
        "global_mode_tracks_existing_and_future_buffers_then_removes_local_hooks",
        elisp_form,
        expect,
    )
    .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        local_mode_centers_a_real_window_and_owns_hooks_keys_and_lighter(),
        post_command_recenters_normal_motion_but_respects_ignored_mouse_commands(),
        end_of_file_policy_switches_between_bottom_alignment_and_fixed_cursor_position(),
        vertical_position_adjustments_clamp_top_and_bottom_anchors_to_live_viewport(),
        paging_commands_move_the_cursor_and_mode_keymap_routes_standard_page_keys(),
        two_windows_showing_one_buffer_recenter_independently_when_selected(),
        global_mode_tracks_existing_and_future_buffers_then_removes_local_hooks(),
    ]
}
