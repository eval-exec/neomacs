use expect_test::expect;

use super::ParityBatchCase;

/// The package's headline story: label every window, press a digit, land
/// there.  Pins the label-to-window mapping itself and the whole frame after
/// each jump, and that jumping never touches any buffer's text.
fn jumping_between_labelled_windows_in_a_three_window_session() -> ParityBatchCase {
    ParityBatchCase::value(
        "jumping_between_labelled_windows_in_a_three_window_session",
        r####"
(unwind-protect
    (progn
      (aw-test-session)
      (global-set-key (kbd "C-c a") #'ace-window)
      (let ((start (list :layout (aw-test-layout)
                         :labels (aw-test-labels)
                         :keys (mapcar #'char-to-string aw-keys)
                         :scope aw-scope))
            second
            third
            back)
        (execute-kbd-macro (kbd "C-c a 2"))
        (setq second (list :selected (buffer-name (window-buffer (selected-window)))
                           :current-buffer (buffer-name)
                           :layout (aw-test-layout)))
        (execute-kbd-macro (kbd "C-c a 3"))
        (setq third (list :selected (buffer-name (window-buffer (selected-window)))
                          :layout (aw-test-layout)))
        (execute-kbd-macro (kbd "C-c a 1"))
        (setq back (list :selected (buffer-name (window-buffer (selected-window)))
                         :layout (aw-test-layout)))
        (list :start start
              :after-2 second
              :after-3 third
              :after-1 back
              :window-count (length (window-list nil 'no-minibuffer))
              :buffers-unmodified
              (mapcar (lambda (b) (list (buffer-name b) (buffer-modified-p b)))
                      (reverse aw-test-buffers)))))
  (aw-test-cleanup))
"####,
        expect![[
            r#"OK (:start (:layout ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected t) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 25) :buffer "notes.org" :point 1 :selected nil)) :labels ((:key "1" :edges (0 1 40 13) :buffer "ledger.el") (:key "2" :edges (0 13 40 25) :buffer "*build-log*") (:key "3" :edges (40 1 80 25) :buffer "notes.org")) :keys ("1" "2" "3" "4" "5" "6" "7" "8" "9") :scope global) :after-2 (:selected "*build-log*" :current-buffer "*build-log*" :layout ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected nil) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected t) (:edges (40 1 80 25) :buffer "notes.org" :point 1 :selected nil))) :after-3 (:selected "notes.org" :layout ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected nil) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 25) :buffer "notes.org" :point 1 :selected t))) :after-1 (:selected "ledger.el" :layout ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected t) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 25) :buffer "notes.org" :point 1 :selected nil))) :window-count 3 :buffers-unmodified (("ledger.el" nil) ("*build-log*" nil) ("notes.org" nil)))"#
        ]],
    )
}

fn one_and_two_window_layouts_switch_without_asking_for_a_label() -> ParityBatchCase {
    ParityBatchCase::value(
        "one_and_two_window_layouts_switch_without_asking_for_a_label",
        r####"
(unwind-protect
    (progn
      (aw-test-session)
      (global-set-key (kbd "C-c a") #'ace-window)
      (delete-other-windows)
      (let (one two)
        (execute-kbd-macro (kbd "C-c a X"))
        (setq one (list :windows (length (window-list nil 'no-minibuffer))
                        :selected (buffer-name (window-buffer (selected-window)))
                        :text (with-current-buffer (window-buffer (selected-window))
                                (buffer-string))
                        :labels (aw-test-labels)))
        (with-current-buffer (window-buffer (selected-window))
          (erase-buffer)
          (insert "(defun settle (invoice)\n  (message \"settled %s\" invoice))\n")
          (goto-char (point-min))
          (set-buffer-modified-p nil))
        (set-window-buffer (split-window-right) (nth 2 (reverse aw-test-buffers)))
        (execute-kbd-macro (kbd "C-c a Y"))
        (setq two (list :windows (length (window-list nil 'no-minibuffer))
                        :selected (buffer-name (window-buffer (selected-window)))
                        :text (with-current-buffer (window-buffer (selected-window))
                                (buffer-string))
                        :labels (aw-test-labels)
                        :layout (aw-test-layout)))
        (list :one-window one
              :two-windows two
              :dispatch-when-more-than aw-dispatch-when-more-than
              :dispatch-always aw-dispatch-always)))
  (aw-test-cleanup))
"####,
        expect![[
            r#"OK (:one-window (:windows 1 :selected "ledger.el" :text "X(defun settle (invoice)\n  (message \"settled %s\" invoice))\n" :labels ((:key "1" :edges (0 1 80 25) :buffer "ledger.el"))) :two-windows (:windows 2 :selected "notes.org" :text "Y* Release\n** TODO cut the branch\n" :labels ((:key "1" :edges (0 1 40 25) :buffer "ledger.el") (:key "2" :edges (40 1 80 25) :buffer "notes.org")) :layout ((:edges (0 1 40 25) :buffer "ledger.el" :point 1 :selected nil) (:edges (40 1 80 25) :buffer "notes.org" :point 2 :selected t))) :dispatch-when-more-than 2 :dispatch-always nil)"#
        ]],
    )
}

fn swapping_two_windows_and_flipping_back_to_the_previous_one() -> ParityBatchCase {
    ParityBatchCase::value(
        "swapping_two_windows_and_flipping_back_to_the_previous_one",
        r####"
(unwind-protect
    (progn
      (aw-test-session)
      (global-set-key (kbd "C-c a") #'ace-window)
      (let ((start (list :layout (aw-test-layout)
                         :labels (aw-test-labels)
                         :swap-invert aw-swap-invert
                         :dispatch (mapcar (lambda (entry)
                                             (list (char-to-string (car entry))
                                                   (nth 1 entry)
                                                   (nth 2 entry)))
                                           aw-dispatch-alist)))
            swapped
            flipped)
        (execute-kbd-macro (kbd "C-c a m 3"))
        (setq swapped (list :selected (buffer-name (window-buffer (selected-window)))
                            :layout (aw-test-layout)
                            :mode-line-tag ace-window-mode))
        (execute-kbd-macro (kbd "C-c a 2"))
        (execute-kbd-macro (kbd "C-c a n"))
        (setq flipped (list :selected (buffer-name (window-buffer (selected-window)))
                            :layout (aw-test-layout)))
        (list :start start :after-swap swapped :after-flip flipped
              :windows (length (window-list nil 'no-minibuffer)))))
  (aw-test-cleanup))
"####,
        expect![[
            r#"OK (:start (:layout ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected t) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 25) :buffer "notes.org" :point 1 :selected nil)) :labels ((:key "1" :edges (0 1 40 13) :buffer "ledger.el") (:key "2" :edges (0 13 40 25) :buffer "*build-log*") (:key "3" :edges (40 1 80 25) :buffer "notes.org")) :swap-invert nil :dispatch (("x" aw-delete-window "Delete Window") ("m" aw-swap-window "Swap Windows") ("M" aw-move-window "Move Window") ("c" aw-copy-window "Copy Window") ("j" aw-switch-buffer-in-window "Select Buffer") ("n" aw-flip-window nil) ("u" aw-switch-buffer-other-window "Switch Buffer Other Window") ("e" aw-execute-command-other-window "Execute Command Other Window") ("F" aw-split-window-fair "Split Fair Window") ("v" aw-split-window-vert "Split Vert Window") ("b" aw-split-window-horz "Split Horz Window") ("o" delete-other-windows "Delete Other Windows") ("T" aw-transpose-frame "Transpose Frame") ("?" aw-show-dispatch-help nil))) :after-swap (:selected "ledger.el" :layout ((:edges (0 1 40 13) :buffer "notes.org" :point 1 :selected nil) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 25) :buffer "ledger.el" :point 1 :selected t)) :mode-line-tag nil) :after-flip (:selected "ledger.el" :layout ((:edges (0 1 40 13) :buffer "notes.org" :point 1 :selected nil) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 25) :buffer "ledger.el" :point 1 :selected t))) :windows 3)"#
        ]],
    )
}

fn deleting_a_window_and_maximizing_another_through_dispatch() -> ParityBatchCase {
    ParityBatchCase::value(
        "deleting_a_window_and_maximizing_another_through_dispatch",
        r####"
(unwind-protect
    (progn
      (aw-test-session)
      (global-set-key (kbd "C-c a") #'ace-window)
      (let ((start (aw-test-layout))
            maximized
            rebuilt
            deleted)
        (execute-kbd-macro (kbd "C-c a o 3"))
        (setq maximized (list :windows (length (window-list nil 'no-minibuffer))
                              :selected (buffer-name (window-buffer (selected-window)))
                              :layout (aw-test-layout)
                              :labels (aw-test-labels)))
        (aw-test-session)
        (setq rebuilt (aw-test-layout))
        (execute-kbd-macro (kbd "C-c a x 2"))
        (setq deleted (list :windows (length (window-list nil 'no-minibuffer))
                            :selected (buffer-name (window-buffer (selected-window)))
                            :layout (aw-test-layout)
                            :labels (aw-test-labels)))
        (list :start start
              :after-delete-other-windows maximized
              :rebuilt rebuilt
              :after-delete deleted
              :buffers-still-live
              (mapcar (lambda (name) (list name (and (get-buffer name) t)))
                      '("ledger.el" "*build-log*" "notes.org"))
              :buffers-unmodified
              (mapcar (lambda (b) (list (buffer-name b) (buffer-modified-p b)))
                      (reverse aw-test-buffers)))))
  (aw-test-cleanup))
"####,
        expect![[
            r#"OK (:start ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected t) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 25) :buffer "notes.org" :point 1 :selected nil)) :after-delete-other-windows (:windows 1 :selected "notes.org" :layout ((:edges (0 1 80 25) :buffer "notes.org" :point 1 :selected t)) :labels ((:key "1" :edges (0 1 80 25) :buffer "notes.org"))) :rebuilt ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected t) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 25) :buffer "notes.org" :point 1 :selected nil)) :after-delete (:windows 2 :selected "ledger.el" :layout ((:edges (0 1 40 25) :buffer "ledger.el" :point 1 :selected t) (:edges (40 1 80 25) :buffer "notes.org" :point 1 :selected nil)) :labels ((:key "1" :edges (0 1 40 25) :buffer "ledger.el") (:key "2" :edges (40 1 80 25) :buffer "notes.org"))) :buffers-still-live (("ledger.el" t) ("*build-log*" t) ("notes.org" t)) :buffers-unmodified (("ledger.el" nil) ("*build-log*" nil) ("notes.org" nil)))"#
        ]],
    )
}

fn splitting_a_chosen_window_vertically_horizontally_and_fairly() -> ParityBatchCase {
    ParityBatchCase::value(
        "splitting_a_chosen_window_vertically_horizontally_and_fairly",
        r####"
(unwind-protect
    (progn
      (aw-test-session)
      (global-set-key (kbd "C-c a") #'ace-window)
      (let ((start (aw-test-layout))
            vert
            horz
            fair)
        (execute-kbd-macro (kbd "C-c a v 3"))
        (setq vert (list :windows (length (window-list nil 'no-minibuffer))
                         :selected (buffer-name (window-buffer (selected-window)))
                         :layout (aw-test-layout)))
        (execute-kbd-macro (kbd "C-c a b 1"))
        (setq horz (list :windows (length (window-list nil 'no-minibuffer))
                         :selected (buffer-name (window-buffer (selected-window)))
                         :layout (aw-test-layout)))
        (aw-test-session)
        (execute-kbd-macro (kbd "C-c a F 3"))
        (setq fair (list :windows (length (window-list nil 'no-minibuffer))
                         :selected (buffer-name (window-buffer (selected-window)))
                         :layout (aw-test-layout)
                         :aspect-ratio aw-fair-aspect-ratio))
        (list :start start :after-split-vert vert :after-split-horz horz
              :after-split-fair fair)))
  (aw-test-cleanup))
"####,
        expect![[
            r#"OK (:start ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected t) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 25) :buffer "notes.org" :point 1 :selected nil)) :after-split-vert (:windows 4 :selected "notes.org" :layout ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected nil) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 13) :buffer "notes.org" :point 1 :selected t) (:edges (40 13 80 25) :buffer "notes.org" :point 1 :selected nil))) :after-split-horz (:windows 5 :selected "ledger.el" :layout ((:edges (0 1 20 13) :buffer "ledger.el" :point 1 :selected t) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (20 1 40 13) :buffer "ledger.el" :point 1 :selected nil) (:edges (40 1 80 13) :buffer "notes.org" :point 1 :selected nil) (:edges (40 13 80 25) :buffer "notes.org" :point 1 :selected nil))) :after-split-fair (:windows 4 :selected "notes.org" :layout ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected nil) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 13) :buffer "notes.org" :point 1 :selected t) (:edges (40 13 80 25) :buffer "notes.org" :point 1 :selected nil)) :aspect-ratio 2))"#
        ]],
    )
}

fn ignoring_the_current_window_or_a_named_buffer_reshuffles_the_labels() -> ParityBatchCase {
    ParityBatchCase::value(
        "ignoring_the_current_window_or_a_named_buffer_reshuffles_the_labels",
        r####"
(unwind-protect
    (progn
      (aw-test-session)
      (global-set-key (kbd "C-c a") #'ace-window)
      ;; A fourth window, so one window can be filtered out and the label
      ;; prompt still appears.
      (select-window (nth 2 (sort (window-list nil 'no-minibuffer) #'aw-window<)))
      (split-window-below)
      (select-window (car (sort (window-list nil 'no-minibuffer) #'aw-window<)))
      (let ((start (list :layout (aw-test-layout)
                         :labels (aw-test-labels)
                         :ignore-on aw-ignore-on
                         :ignore-current aw-ignore-current
                         :ignored-buffers aw-ignored-buffers))
            ignore-current
            ignored-buffer
            ignore-off)
        (let ((aw-ignore-current t))
          (setq ignore-current (list :labels (aw-test-labels)))
          (execute-kbd-macro (kbd "C-c a 1"))
          (setq ignore-current
                (append ignore-current
                        (list :selected (buffer-name (window-buffer (selected-window)))
                              :layout (aw-test-layout)))))
        (select-window (car (sort (window-list nil 'no-minibuffer) #'aw-window<)))
        (let ((aw-ignored-buffers '("*build-log*")))
          (setq ignored-buffer (list :labels (aw-test-labels)))
          (execute-kbd-macro (kbd "C-c a 2"))
          (setq ignored-buffer
                (append ignored-buffer
                        (list :selected (buffer-name (window-buffer (selected-window)))
                              :layout (aw-test-layout))))
          (let ((aw-ignore-on nil))
            (setq ignore-off (list :labels (aw-test-labels)))))
        (list :start start
              :with-ignore-current ignore-current
              :with-ignored-buffer ignored-buffer
              :with-ignore-off ignore-off)))
  (aw-test-cleanup))
"####,
        expect![[
            r#"OK (:start (:layout ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected t) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 13) :buffer "notes.org" :point 1 :selected nil) (:edges (40 13 80 25) :buffer "notes.org" :point 1 :selected nil)) :labels ((:key "1" :edges (0 1 40 13) :buffer "ledger.el") (:key "2" :edges (0 13 40 25) :buffer "*build-log*") (:key "3" :edges (40 1 80 13) :buffer "notes.org") (:key "4" :edges (40 13 80 25) :buffer "notes.org")) :ignore-on t :ignore-current nil :ignored-buffers ("*Calc Trail*" " *LV*")) :with-ignore-current (:labels ((:key "1" :edges (0 13 40 25) :buffer "*build-log*") (:key "2" :edges (40 1 80 13) :buffer "notes.org") (:key "3" :edges (40 13 80 25) :buffer "notes.org")) :selected "*build-log*" :layout ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected nil) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected t) (:edges (40 1 80 13) :buffer "notes.org" :point 1 :selected nil) (:edges (40 13 80 25) :buffer "notes.org" :point 1 :selected nil))) :with-ignored-buffer (:labels ((:key "1" :edges (0 1 40 13) :buffer "ledger.el") (:key "2" :edges (40 1 80 13) :buffer "notes.org") (:key "3" :edges (40 13 80 25) :buffer "notes.org")) :selected "notes.org" :layout ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected nil) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 13) :buffer "notes.org" :point 1 :selected t) (:edges (40 13 80 25) :buffer "notes.org" :point 1 :selected nil))) :with-ignore-off (:labels ((:key "1" :edges (0 1 40 13) :buffer "ledger.el") (:key "2" :edges (0 13 40 25) :buffer "*build-log*") (:key "3" :edges (40 1 80 13) :buffer "notes.org") (:key "4" :edges (40 13 80 25) :buffer "notes.org"))))"#
        ]],
    )
}

fn quitting_or_pressing_an_unused_label_leaves_the_layout_untouched() -> ParityBatchCase {
    ParityBatchCase::value(
        "quitting_or_pressing_an_unused_label_leaves_the_layout_untouched",
        r####"
(unwind-protect
    (progn
      (aw-test-session)
      (global-set-key (kbd "C-c a") #'ace-window)
      (let ((start (aw-test-layout))
            quit
            bad-label
            recovered)
        (execute-kbd-macro (kbd "C-c a C-g"))
        (setq quit (list :windows (length (window-list nil 'no-minibuffer))
                         :selected (buffer-name (window-buffer (selected-window)))
                         :layout (aw-test-layout)
                         :ace-window-mode ace-window-mode
                         :aw-action aw-action))
        (execute-kbd-macro (kbd "C-c a 9 C-g"))
        (setq bad-label (list :windows (length (window-list nil 'no-minibuffer))
                              :selected (buffer-name (window-buffer (selected-window)))
                              :layout (aw-test-layout)))
        (execute-kbd-macro (kbd "C-c a 3"))
        (setq recovered (list :selected (buffer-name (window-buffer (selected-window)))
                              :layout (aw-test-layout)))
        (list :start start
              :after-quit quit
              :after-invalid-label bad-label
              :after-valid-label recovered
              :layout-survived-both
              (list (equal start (plist-get quit :layout))
                    (equal start (plist-get bad-label :layout)))
              :no-leftover-overlays
              (length (cl-remove-if-not
                       (lambda (o) (overlay-get o 'aw-overlay))
                       (apply #'append
                              (mapcar (lambda (b)
                                        (with-current-buffer b
                                          (overlays-in (point-min) (point-max))))
                                      aw-test-buffers)))))))
  (aw-test-cleanup))
"####,
        expect![[
            r#"OK (:start ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected t) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 25) :buffer "notes.org" :point 1 :selected nil)) :after-quit (:windows 3 :selected "ledger.el" :layout ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected t) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 25) :buffer "notes.org" :point 1 :selected nil)) :ace-window-mode nil :aw-action nil) :after-invalid-label (:windows 3 :selected "ledger.el" :layout ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected t) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 25) :buffer "notes.org" :point 1 :selected nil))) :after-valid-label (:selected "notes.org" :layout ((:edges (0 1 40 13) :buffer "ledger.el" :point 1 :selected nil) (:edges (0 13 40 25) :buffer "*build-log*" :point 1 :selected nil) (:edges (40 1 80 25) :buffer "notes.org" :point 1 :selected t))) :layout-survived-both (t t) :no-leftover-overlays 0)"#
        ]],
    )
}

fn display_mode_puts_each_windows_label_in_its_mode_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "display_mode_puts_each_windows_label_in_its_mode_line",
        r####"
(unwind-protect
    (progn
      (aw-test-session)
      (global-set-key (kbd "C-c a") #'ace-window)
      (cl-flet ((paths ()
                  (mapcar
                   (lambda (w)
                     (let ((path (window-parameter w 'ace-window-path)))
                       (list :buffer (buffer-name (window-buffer w))
                             :edges (window-edges w)
                             :path (and path (substring-no-properties path))
                             :face (and path (get-text-property 0 'face path)))))
                   (sort (window-list nil 'no-minibuffer) #'aw-window<))))
        (let ((before (list :enabled ace-window-display-mode
                            :paths (paths)
                            :mode-line-head (car (default-value 'mode-line-format))))
              enabled
              after-split
              disabled)
          (ace-window-display-mode 1)
          (setq enabled (list :enabled ace-window-display-mode
                              :paths (paths)
                              :mode-line-head (car (default-value 'mode-line-format))
                              :update-hooked
                              (and (memq 'aw-update
                                         window-configuration-change-hook)
                                   t)))
          (select-window (nth 2 (sort (window-list nil 'no-minibuffer) #'aw-window<)))
          (split-window-below)
          (select-window (car (sort (window-list nil 'no-minibuffer) #'aw-window<)))
          (setq after-split (list :windows (length (window-list nil 'no-minibuffer))
                                  :paths (paths)))
          (execute-kbd-macro (kbd "C-c a 4"))
          (setq after-split
                (append after-split
                        (list :selected-after-4
                              (buffer-name (window-buffer (selected-window)))
                              :selected-edges (window-edges (selected-window)))))
          (ace-window-display-mode -1)
          (setq disabled (list :enabled ace-window-display-mode
                               :mode-line-head (car (default-value 'mode-line-format))
                               :update-hooked
                               (and (memq 'aw-update
                                          window-configuration-change-hook)
                                    t)))
          (list :before before :enabled enabled :after-split after-split
                :disabled disabled
                :overlay-flag aw-display-mode-overlay))))
  (aw-test-cleanup))
"####,
        expect![[
            r#"OK (:before (:enabled nil :paths ((:buffer "ledger.el" :edges (0 1 40 13) :path nil :face nil) (:buffer "*build-log*" :edges (0 13 40 25) :path nil :face nil) (:buffer "notes.org" :edges (40 1 80 25) :path nil :face nil)) :mode-line-head "%e") :enabled (:enabled t :paths ((:buffer "ledger.el" :edges (0 1 40 13) :path "1" :face aw-mode-line-face) (:buffer "*build-log*" :edges (0 13 40 25) :path "2" :face aw-mode-line-face) (:buffer "notes.org" :edges (40 1 80 25) :path "3" :face aw-mode-line-face)) :mode-line-head (ace-window-display-mode (:eval (window-parameter (selected-window) 'ace-window-path))) :update-hooked t) :after-split (:windows 4 :paths ((:buffer "ledger.el" :edges (0 1 40 13) :path "1" :face aw-mode-line-face) (:buffer "*build-log*" :edges (0 13 40 25) :path "2" :face aw-mode-line-face) (:buffer "notes.org" :edges (40 1 80 13) :path "3" :face aw-mode-line-face) (:buffer "notes.org" :edges (40 13 80 25) :path nil :face nil)) :selected-after-4 "notes.org" :selected-edges (40 13 80 25)) :disabled (:enabled nil :mode-line-head "%e" :update-hooked nil) :overlay-flag t)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        jumping_between_labelled_windows_in_a_three_window_session(),
        one_and_two_window_layouts_switch_without_asking_for_a_label(),
        swapping_two_windows_and_flipping_back_to_the_previous_one(),
        deleting_a_window_and_maximizing_another_through_dispatch(),
        splitting_a_chosen_window_vertically_horizontally_and_fairly(),
        ignoring_the_current_window_or_a_named_buffer_reshuffles_the_labels(),
        quitting_or_pressing_an_unused_label_leaves_the_layout_untouched(),
        display_mode_puts_each_windows_label_in_its_mode_line(),
    ]
}
