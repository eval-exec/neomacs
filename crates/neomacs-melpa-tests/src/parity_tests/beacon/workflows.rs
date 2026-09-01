use expect_test::expect;

use super::ParityBatchCase;

fn public_midline_blink_fades_and_real_key_cancels() -> ParityBatchCase {
    let elisp_form = r##"(beacon359-test-run
 "manual-middle"
 (lambda ()
   (beacon359-test-own-buffer
    "middle" #'text-mode "alpha beta 界 gamma delta\nsecond line Ω\n")
   (put-text-property (point-min) (point-max)
                      'face '(:background "#000000"))
   (set-buffer-modified-p nil)
   (setq buffer-undo-list nil)
   (goto-char 7)
   (let ((beacon-size 5)
         (beacon-color "#ff0000")
         (beacon-blink-delay 100)
         (beacon-blink-duration 1.0)
         (beacon-before-blink-hook
          (list #'beacon359-test-record-blink))
         (focus-before after-focus-change-function)
         timers-before started finished timer initial after-one after-two after-key
         second-timer disable-active enabled-hooks disabled-hooks
         numeric-timer numeric)
     (beacon-mode 1)
     (beacon-mode 1)
     (setq enabled-hooks
           (list :mode beacon-mode
                 :record (beacon359-test-hook-count
                          #'beacon--record-vars 'pre-command-hook)
                 :vanish-pre (beacon359-test-hook-count
                              #'beacon--vanish 'pre-command-hook)
                 :post (beacon359-test-hook-count
                        #'beacon--post-command 'post-command-hook)
                 :vanish-change (beacon359-test-hook-count
                                 #'beacon--vanish 'before-change-functions)
                 :scroll (beacon359-test-hook-count
                          #'beacon--window-scroll-function
                          'window-scroll-functions)
                 :focus-changed (not (eq focus-before
                                         after-focus-change-function))))
     (setq timers-before (copy-sequence timer-list)
           started (current-time))
     (call-interactively #'beacon-blink)
     (setq finished (current-time)
           timer (beacon359-test-register-action
                  timers-before started finished 100 0.2)
           initial (beacon359-test-overlay-state))
     (beacon359-test-dispatch-owned-timer timer)
     (setq after-one (beacon359-test-overlay-state))
     (beacon359-test-dispatch-owned-timer timer)
     (setq after-two (beacon359-test-overlay-state)
           timers-before (copy-sequence timer-list)
           started (current-time))
     (call-interactively #'beacon-blink)
     (setq finished (current-time)
           timer (beacon359-test-register-action
                  timers-before started finished 100 0.2))
     (beacon359-test-command-loop
      (lambda () (execute-kbd-macro (kbd "C-f"))))
     (setq after-key
           (list :point (point) :overlays (beacon359-test-overlay-state)
                 :timer (beacon359-test-timer-state)
                 :owned-cancelled (not (memq timer timer-list))))
     (setq timers-before (copy-sequence timer-list)
           started (current-time))
     (call-interactively #'beacon-blink)
     (setq finished (current-time)
           second-timer (beacon359-test-register-action
                         timers-before started finished 100 0.2))
     (beacon-mode -1)
     (setq disabled-hooks
           (list :mode beacon-mode
                 :record (beacon359-test-hook-count
                          #'beacon--record-vars 'pre-command-hook)
                 :vanish-pre (beacon359-test-hook-count
                              #'beacon--vanish 'pre-command-hook)
                 :post (beacon359-test-hook-count
                        #'beacon--post-command 'post-command-hook)
                 :vanish-change (beacon359-test-hook-count
                                 #'beacon--vanish 'before-change-functions)
                 :scroll (beacon359-test-hook-count
                          #'beacon--window-scroll-function
                          'window-scroll-functions)
                 :focus-restored (eq focus-before
                                     after-focus-change-function)))
     (setq disable-active
           (list :overlays (beacon359-test-overlay-state)
                 :timer (beacon359-test-timer-state)
                 :same-timer (eq second-timer beacon--timer)))
     (setq beacon-color 0.5)
     (goto-char (point-min))
     (setq timers-before (copy-sequence timer-list)
           started (current-time))
     (call-interactively #'beacon-blink)
     (setq finished (current-time)
           numeric-timer (beacon359-test-register-action
                          timers-before started finished 100 0.2)
           numeric
           (list :color beacon-color :size beacon-size
                 :old-timer-cancelled (not (memq second-timer timer-list))
                 :overlays (beacon359-test-overlay-state)
                 :timer (and (eq numeric-timer beacon--timer)
                             (beacon359-test-timer-state))))
     (list
      :activation
      (list :feature (featurep 'beacon)
            :source (file-name-nondirectory
                     (symbol-file 'beacon-blink 'defun))
            :seq (package-built-in-p 'seq '(2 24))
            :suffixes load-suffixes
            :default-predicates beacon-dont-blink-predicates
            :default-modes beacon-dont-blink-major-modes
            :default-commands beacon-dont-blink-commands)
      :enabled enabled-hooks :events (nreverse beacon359-test-blinks)
      :initial initial :after-one after-one :after-two after-two
      :after-key after-key
      :disabled disabled-hooks :disable-active disable-active
      :numeric numeric
      :text (buffer-string) :modified (buffer-modified-p)
      :undo buffer-undo-list))))"##;
    ParityBatchCase::value(
        "public_midline_blink_fades_and_real_key_cancels",
        elisp_form,
        expect![[
            r##"OK (:result (:activation (:feature t :source "beacon.el" :seq t :suffixes (".el") :default-predicates (beacon--compilation-mode-p window-minibuffer-p) :default-modes (t magit-status-mode magit-popup-mode inf-ruby-mode mu4e-headers-mode gnus-summary-mode gnus-group-mode) :default-commands (next-line previous-line forward-line)) :enabled (:mode t :record 1 :vanish-pre 1 :post 1 :vanish-change 1 :scroll 1 :focus-changed t) :events ((:command nil :last nil :point 7 :line 1 :column 6 :window-start 1) (:command nil :last nil :point 7 :line 1 :column 6 :window-start 1) (:command nil :last forward-char :point 8 :line 1 :column 7 :window-start 1) (:command nil :last forward-char :point 1 :line 1 :column 0 :window-start 1)) :initial ((:range (7 . 8) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#ffff00000000") :colors nil :after nil) (:range (8 . 9) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#cccc00000000") :colors nil :after nil) (:range (9 . 10) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#999900000000") :colors nil :after nil) (:range (10 . 11) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#666600000000") :colors nil :after nil)) :after-one ((:range (7 . 8) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#cccc00000000") :colors nil :after nil) (:range (8 . 9) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#999900000000") :colors nil :after nil) (:range (9 . 10) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#666600000000") :colors nil :after nil)) :after-two ((:range (7 . 8) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#999900000000") :colors nil :after nil) (:range (8 . 9) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#666600000000") :colors nil :after nil)) :after-key (:point 8 :overlays nil :timer (:timer t :listed nil :function beacon--dec :repeat 0.2 :args nil) :owned-cancelled t) :disabled (:mode nil :record 0 :vanish-pre 0 :post 0 :vanish-change 0 :scroll 0 :focus-restored t) :disable-active (:overlays ((:range (8 . 9) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#ffff00000000") :colors nil :after nil) (:range (9 . 10) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#cccc00000000") :colors nil :after nil) (:range (10 . 11) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#999900000000") :colors nil :after nil) (:range (11 . 12) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#666600000000") :colors nil :after nil)) :timer (:timer t :listed t :function beacon--dec :repeat 0.2 :args nil) :same-timer t) :numeric (:color 0.5 :size 5 :old-timer-cancelled t :overlays ((:range (1 . 2) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#7fff7fff7fff") :colors nil :after nil) (:range (2 . 3) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#666666666666") :colors nil :after nil) (:range (3 . 4) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#4ccc4ccc4ccc") :colors nil :after nil) (:range (4 . 5) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#333333333333") :colors nil :after nil)) :timer (:timer t :listed t :function beacon--dec :repeat 0.2 :args nil)) :text #("alpha beta 界 gamma delta\nsecond line Ω\n" 0 39 (face (:background "#000000"))) :modified nil :undo nil) :cleanup (:new-buffers nil :new-processes nil :new-timers nil :owned-live (nil) :owned-overlays-live (nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil) :owned-timers-active (nil nil nil nil) :windows t :configuration t :buffer t :window t :variables t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn eol_after_string_truncates_fades_and_retains_deleted_edge_overlay() -> ParityBatchCase {
    let elisp_form = r##"(beacon359-test-run
 "eol"
 (lambda ()
   (delete-other-windows)
   (select-window (split-window-right -24))
   (beacon359-test-own-buffer
    "eol" #'text-mode
    "\twide 界 cedar birch m\nxxxxxxxxxxxxxxxxxxxxxxx\n")
   (put-text-property (point-min) (point-max)
                      'face '(:background "#000000"))
   (set-buffer-modified-p nil)
   (setq buffer-undo-list nil)
   (let ((beacon-size 5)
         (beacon-color "#ff0000")
         (beacon-blink-delay 100)
         (beacon-blink-duration 1.0)
         timers-before started finished timer first after-one old-cancelled
         edge-timer edge)
     (goto-char (point-min))
     (end-of-line)
     (setq timers-before (copy-sequence timer-list)
           started (current-time))
     (call-interactively #'beacon-blink)
     (setq finished (current-time)
           timer (beacon359-test-register-action
                  timers-before started finished 100 0.2)
           first
           (list :width (window-width) :body (window-body-width)
                 :point (point) :column (current-column)
                 :overlays (beacon359-test-overlay-state)
                 :timer (beacon359-test-timer-state)))
     (beacon359-test-dispatch-owned-timer timer)
     (setq after-one
           (list :overlays (beacon359-test-overlay-state)
                 :timer (beacon359-test-timer-state)))
     (forward-line 1)
     (end-of-line)
     (setq timers-before (copy-sequence timer-list)
           started (current-time))
     (call-interactively #'beacon-blink)
     (setq finished (current-time)
           old-cancelled (not (memq timer timer-list))
           edge-timer (beacon359-test-register-action
                       timers-before started finished 100 0.2)
           edge
           (list :width (window-width) :point (point)
                 :column (current-column)
                 :global-count (length beacon--ovs)
                 :buffers (mapcar #'overlay-buffer beacon--ovs)
                 :live (beacon359-test-overlay-state)
                 :old-timer-cancelled old-cancelled
                 :new-timer (and (eq edge-timer beacon--timer)
                                 (beacon359-test-timer-state))))
     (list :mode beacon-mode :first first :after-one after-one :edge edge
           :text (buffer-string) :modified (buffer-modified-p)
           :undo buffer-undo-list))))"##;
    ParityBatchCase::value(
        "eol_after_string_truncates_fades_and_retains_deleted_edge_overlay",
        elisp_form,
        expect![[
            r##"OK (:result (:mode nil :first (:width 24 :body 24 :point 22 :column 29 :overlays ((:range (22 . 22) :beacon t :priority 1152921504606846975 :selected-window t :face nil :colors ("#ffff00000000" "#cccc00000000") :after (:text "  " :cursor 1000 :faces ((:background "#ffff00000000") (:background "#cccc00000000"))))) :timer (:timer t :listed t :function beacon--dec :repeat 0.2 :args nil)) :after-one (:overlays ((:range (22 . 22) :beacon t :priority 1152921504606846975 :selected-window t :face nil :colors ("#cccc00000000") :after (:text " " :cursor 1000 :faces ((:background "#cccc00000000"))))) :timer (:timer t :listed t :function beacon--dec :repeat 0.2 :args nil)) :edge (:width 24 :point 46 :column 23 :global-count 1 :buffers (nil) :live nil :old-timer-cancelled t :new-timer (:timer t :listed t :function beacon--dec :repeat 0.2 :args nil)) :text #("\11wide 界 cedar birch m\nxxxxxxxxxxxxxxxxxxxxxxx\n" 0 46 (face (:background "#000000"))) :modified nil :undo nil) :cleanup (:new-buffers nil :new-processes nil :new-timers nil :owned-live (nil) :owned-overlays-live (nil nil) :owned-timers-active (nil nil) :windows t :configuration t :buffer t :window t :variables t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn invalid_sizes_and_color_fail_atomically_then_recover() -> ParityBatchCase {
    let elisp_form = r##"(beacon359-test-run
 "failure"
 (lambda ()
   (beacon359-test-own-buffer "failure" #'text-mode "alpha beta Ω\n")
   (put-text-property (point-min) (point-max)
                      'face '(:background "#000000"))
   (set-buffer-modified-p nil)
   (setq buffer-undo-list nil)
   (goto-char 3)
   (let ((beacon-size 0)
         (beacon-color "#ff0000")
         (beacon-blink-delay 100)
         (beacon-blink-duration 1.0)
         (beacon-before-blink-hook
          (list #'beacon359-test-record-blink))
         timers-before started finished zero invalid timer recovery before after)
     (setq before
           (list :point (point) :text (substring-no-properties (buffer-string))
                 :face (copy-tree (get-text-property (point) 'face))
                 :modified (buffer-modified-p) :undo buffer-undo-list))
     (setq timers-before (copy-sequence timer-list)
           zero (beacon359-test-condition
                 (lambda () (call-interactively #'beacon-blink))))
     (setq zero
           (list :outcome zero :events (length beacon359-test-blinks)
                 :overlays (beacon359-test-overlay-state)
                 :timer beacon--timer
                 :new-timers (length (seq-difference timer-list
                                                     timers-before #'eq))))
     (setq beacon-size 5 beacon-color "definitely-not-a-color-359"
           timers-before (copy-sequence timer-list))
     (setq invalid
           (beacon359-test-condition
            (lambda () (call-interactively #'beacon-blink))))
     (setq invalid
           (list :outcome invalid :events (length beacon359-test-blinks)
                 :overlays (beacon359-test-overlay-state)
                 :timer beacon--timer
                 :new-timers (length (seq-difference timer-list
                                                     timers-before #'eq))))
     (setq beacon-color "#ff0000"
           timers-before (copy-sequence timer-list)
           started (current-time))
     (call-interactively #'beacon-blink)
     (setq finished (current-time)
           timer (beacon359-test-register-action
                  timers-before started finished 100 0.2)
           recovery
           (list :events (length beacon359-test-blinks)
                 :overlays (beacon359-test-overlay-state)
                 :timer (and (eq timer beacon--timer)
                             (beacon359-test-timer-state))))
     (setq after
           (list :point (point)
                 :text (substring-no-properties (buffer-string))
                 :face (copy-tree (get-text-property (point) 'face))
                 :modified (buffer-modified-p) :undo buffer-undo-list))
     (list :before before :zero zero :invalid invalid :recovery recovery
           :after after :unchanged (equal after before)))))"##;
    ParityBatchCase::value(
        "invalid_sizes_and_color_fail_atomically_then_recover",
        elisp_form,
        expect![[
            r##"OK (:result (:before (:point 3 :text "alpha beta Ω\n" :face (:background "#000000") :modified nil :undo nil) :zero (:outcome (:signal arith-error :data nil :message "Arithmetic error") :events 1 :overlays nil :timer nil :new-timers 0) :invalid (:outcome (:signal wrong-type-argument :data (number-or-marker-p nil) :message "Wrong type argument: number-or-marker-p, nil") :events 2 :overlays nil :timer nil :new-timers 0) :recovery (:events 3 :overlays ((:range (3 . 4) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#ffff00000000") :colors nil :after nil) (:range (4 . 5) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#cccc00000000") :colors nil :after nil) (:range (5 . 6) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#999900000000") :colors nil :after nil) (:range (6 . 7) :beacon t :priority 1152921504606846975 :selected-window t :face (:background "#666600000000") :colors nil :after nil)) :timer (:timer t :listed t :function beacon--dec :repeat 0.2 :args nil)) :after (:point 3 :text "alpha beta Ω\n" :face (:background "#000000") :modified nil :undo nil) :unchanged t) :cleanup (:new-buffers nil :new-processes nil :new-timers nil :owned-live (nil) :owned-overlays-live (nil nil nil nil) :owned-timers-active (nil) :windows t :configuration t :buffer t :window t :variables t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

pub(super) fn beacon_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        public_midline_blink_fades_and_real_key_cancels(),
        eol_after_string_truncates_fades_and_retains_deleted_edge_overlay(),
        invalid_sizes_and_color_fail_atomically_then_recover(),
    ]
}
