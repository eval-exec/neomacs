use expect_test::expect;

use super::ParityBatchCase;

/// Turning the mode on is what makes achievements watch you: it registers the
/// repeating idle timer built from `achievements-idle-time', installs the
/// `post-command-hook' checker and collects the achievements that need it (the
/// arrow-key one).  Turning it off has to undo all of that, while the
/// `kill-emacs-hook' that saves the file - installed by `achievements-init'
/// when the package was loaded - stays.
fn achievements_mode_installs_and_removes_its_hook_and_idle_timer() -> ParityBatchCase {
    ParityBatchCase::value(
        "achievements_mode_installs_and_removes_its_hook_and_idle_timer",
        r##"(ach-test-with-live-buffer
 (let ((achievements-file (ach-test-path "achievements.eld"))
       (achievements-idle-time 3))
   (let ((before (list (and (memq #'achievements-post-command-function post-command-hook) t)
                       achievements-timer
                       achievements-post-command-list
                       (assq 'achievements-mode minor-mode-alist))))
     (achievements-mode 1)
     (let ((enabled
            (list achievements-mode
                  (and (memq #'achievements-post-command-function post-command-hook) t)
                  (and (memq achievements-timer timer-idle-list) t)
                  (timer--time achievements-timer)
                  (timer--repeat-delay achievements-timer)
                  (timer--function achievements-timer)
                  (mapcar #'emacs-achievement-name achievements-post-command-list))))
       (achievements-mode -1)
       (list before
             enabled
             (list achievements-mode
                   (and (memq #'achievements-post-command-function post-command-hook) t)
                   achievements-timer
                   (cl-count-if (lambda (timer)
                                  (eq (timer--function timer) #'achievements-update-score))
                                timer-idle-list))
             (assq 'achievements-mode minor-mode-alist)
             (and (memq #'achievements-save-achievements kill-emacs-hook) t))))))"##,
        expect![[
            r#"OK ((nil nil nil #1=(achievements-mode " Achieve")) (t t t (0 3 0 0) t achievements-update-score ("No arrows")) (nil nil nil 0) #1# t)"#
        ]],
    )
}

fn running_commands_unlocks_the_matching_achievements_and_logs_them() -> ParityBatchCase {
    ParityBatchCase::value(
        "running_commands_unlocks_the_matching_achievements_and_logs_them",
        r##"(ach-test-with-live-buffer
 (let ((achievements-file (ach-test-path "achievements.eld")))
   (keyfreq-mode 1)
   (achievements-mode 1)
   (let ((before (list (ach-test-record "Buffer, buffers, everywhere")
                       (ach-test-record "Log Auditor")
                       (ach-test-record "What did I just do?"))))
     (execute-kbd-macro (kbd "C-x C-b C-h e C-h l C-x o"))
     (achievements-update-score)
     (list before
           (ach-test-record "Buffer, buffers, everywhere")
           (ach-test-record "Log Auditor")
           (ach-test-record "What did I just do?")
           (ach-test-record "Top o' the morning")
           achievements-score
           achievements-total
           (ach-test-earned)
           (last (ach-test-unlock-messages) 3)
           (car (split-string (ach-test-log) "\n"))))))"##,
        expect![[
            r#"OK ((("Buffer, buffers, everywhere" "You've seen all the buffers that can be seen." :pending 5 nil nil) ("Log Auditor" "You learned new things by using `view-echo-area-messages'." :pending 5 nil nil) ("What did I just do?" "You answered a question by using `(command-history view-lossage)'." :pending 5 nil nil)) ("Buffer, buffers, everywhere" "You've seen all the buffers that can be seen." t 5 nil t) ("Log Auditor" "You learned new things by using `view-echo-area-messages'." t 5 nil t) ("What did I just do?" "You answered a question by using `(command-history view-lossage)'." :pending 5 nil nil) ("Top o' the morning" "You've used Emacs as a replacement for top." :pending 5 nil nil) 70 590.5 ("Achiever" "Buffer, buffers, everywhere" "Clean Desk" "Green Glowing faces" "Log Auditor" "Loyalist" "Modernist" "Package Neophyte" "Post Modernist" "Purest Vanilla" "Streamlined" "Tainted Love" "Traditionalist" "Tux's Friend" "Unlocker") ("ACHIEVEMENT UNLOCKED: You’ve earned the ‘Package Neophyte’ achievement!" "ACHIEVEMENT UNLOCKED: You’ve earned the ‘Clean Desk’ achievement!" "ACHIEVEMENT UNLOCKED: You’ve earned the ‘Buffer, buffers, everywhere’ achievement!") "You've earned the `Buffer, buffers, everywhere' achievement! [You've seen all the buffers that can be seen.]")"#
        ]],
    )
}

fn an_achievement_needing_several_commands_stays_locked_until_all_have_run() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_achievement_needing_several_commands_stays_locked_until_all_have_run",
        r##"(ach-test-with-live-buffer
 (let ((achievements-file (ach-test-path "achievements.eld"))
       ;; Help commands change the selected buffer.  A dynamically scoped copy
       ;; keeps the real global lookup behavior without leaking test bindings.
       (global-map (copy-keymap global-map))
       ;; `describe-gnu-project' intentionally opens the GNU Project page.
       ;; Exercise that real command and GNU Emacs's real `browse-url' routing,
       ;; but replace its configurable external-browser boundary so repeated
       ;; batch and isolation runs cannot leave desktop processes behind.
       (browse-url-browser-function #'ach-test-capture-browser-launch))
   (global-set-key (kbd "C-c 1") 'about-emacs)
   (global-set-key (kbd "C-c 2") 'describe-copying)
   (global-set-key (kbd "C-c 3") 'describe-distribution)
   (global-set-key (kbd "C-c 4") 'describe-gnu-project)
   (global-set-key (kbd "C-c 5") 'describe-no-warranty)
   (keyfreq-mode 1)
   (achievements-mode 1)
   (execute-kbd-macro (kbd "h e l l o SPC w o r l d C-c 1 C-c 2 C-c 3 C-c 4 C-x o"))
   (achievements-update-score)
   (let ((four-of-five (list (ach-test-record "Free Software Zealot")
                             (ach-test-record "Short Story")
                             (ach-test-record "Top o' the morning")
                             achievements-score
                             (ach-test-earned)
                             (nreverse ach-test-opened-urls)
                             (with-current-buffer "*achievements-workflow*" (buffer-string)))))
     (execute-kbd-macro (kbd "C-c 5 C-x o"))
     (achievements-update-score)
     (list four-of-five
           (ach-test-record "Free Software Zealot")
           (ach-test-record "Short Story")
           (ach-test-record "Top o' the morning")
           achievements-score
           (sort (cl-set-difference (ach-test-earned) (nth 4 four-of-five) :test #'equal)
                 #'string<)
           (car (last (ach-test-unlock-messages)))))))"##,
        expect![[
            r#"OK ((("Free Software Zealot" "You've read the sales pitch." :pending 5 nil nil) ("Short Story" "You've written the equivalent of a short story." :pending 5 nil nil) ("Top o' the morning" "You've used Emacs as a replacement for top." :pending 5 nil nil) 60 ("Achiever" "Clean Desk" "Green Glowing faces" "Loyalist" "Modernist" "Package Neophyte" "Post Modernist" "Purest Vanilla" "Streamlined" "Tainted Love" "Traditionalist" "Tux's Friend" "Unlocker") ("https://www.gnu.org/gnu/thegnuproject.html") "hello world") ("Free Software Zealot" "You've read the sales pitch." t 5 nil t) ("Short Story" "You've written the equivalent of a short story." :pending 5 nil nil) ("Top o' the morning" "You've used Emacs as a replacement for top." :pending 5 nil nil) 70 ("Free Software Zealot") "ACHIEVEMENT UNLOCKED: You’ve earned the ‘Free Software Zealot’ achievement!")"#
        ]],
    )
}

fn the_achievements_list_buffer_renders_rows_and_grows_when_refreshed() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_achievements_list_buffer_renders_rows_and_grows_when_refreshed",
        r##"(ach-test-with-live-buffer
 (let ((achievements-file (ach-test-path "achievements.eld")))
   (keyfreq-mode 1)
   (achievements-mode 1)
   (execute-kbd-macro (kbd "C-x C-b C-h e C-x o"))
   (achievements-list-achievements)
   (let ((first-view
          (list (buffer-name (window-buffer (selected-window)))
                major-mode
                (line-number-at-pos (point-max))
                achievements-score
                achievements-total
                (ach-test-rows "Achiever" "Buffer, buffers, everywhere"
                               "Top o' the morning" "Twenty Five" "Narrow minded"))))
     (execute-kbd-macro (kbd "g"))
     (execute-kbd-macro (kbd "g"))
     (list first-view
           (line-number-at-pos (point-max))
           achievements-score
           achievements-total
           (ach-test-rows "Achiever" "Buffer, buffers, everywhere"
                          "Top o' the morning" "Twenty Five" "Narrow minded")
           (list tabulated-list-padding
                 (and (memq #'achievements-update-score tabulated-list-revert-hook) t))))))"##,
        expect![[
            r#"OK (("*Achievements*" achievements-list-mode 98 70 590.5 (("Achiever" . " ✓    5 Achiever                       You used the achievements package.") ("Buffer, buffers, everywhere" . " ✓    5 Buffer, buffers, everywhere    You've seen all the buffers that can be seen.") ("Top o' the morning" . "      5 Top o' the morning             ") ("Twenty Five" . "      5 Twenty Five                    ") ("Narrow minded"))) 116 75 680.5 (("Achiever" . " ✓    5 Achiever                       You used the achievements package.") ("Buffer, buffers, everywhere" . " ✓    5 Buffer, buffers, everywhere    You've seen all the buffers that can be seen.") ("Top o' the morning" . "      5 Top o' the morning             ") ("Twenty Five" . "      5 Twenty Five                    ") ("Narrow minded" . "      5 Narrow minded                  ")) (1 t))"#
        ]],
    )
}

fn achievements_are_saved_to_the_achievements_file_and_restored_from_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "achievements_are_saved_to_the_achievements_file_and_restored_from_it",
        r##"(ach-test-with-live-buffer
 (let ((achievements-file (ach-test-path "state/achievements.eld")))
   (make-directory (file-name-directory achievements-file) t)
   (keyfreq-mode 1)
   (achievements-mode 1)
   (execute-kbd-macro (kbd "C-x C-b C-h e C-x o"))
   (achievements-update-score)
   (let* ((saved (with-temp-buffer
                   (insert-file-contents achievements-file)
                   (buffer-string)))
          (records (car (read-from-string saved)))
          (earned (ach-test-earned)))
     (setq achievements-list nil)
     (achievements-load-achievements)
     (let ((restored (list (length achievements-list) (ach-test-earned))))
       (delete-file achievements-file)
       (achievements-load-achievements)
       (list (file-name-nondirectory achievements-file)
             (file-exists-p (ach-test-path "state/achievements.eld"))
             (length records)
             (substring saved 0 2)
             (prin1-to-string
              (cl-find "Buffer, buffers, everywhere" records
                       :key #'emacs-achievement-name :test #'equal))
             (prin1-to-string
              (cl-find "Top o' the morning" records
                       :key #'emacs-achievement-name :test #'equal))
             (length earned)
             restored
             (equal earned (cadr restored))
             achievements-list)))))"##,
        expect![[
            r##"OK ("achievements.eld" nil 101 "(#" "#s(emacs-achievement \"Buffer, buffers, everywhere\" \"You've seen all the buffers that can be seen.\" t nil nil 5 0 nil)" "#s(emacs-achievement \"Top o' the morning\" \"You've used Emacs as a replacement for top.\" (lambda nil (and (achievements-command-was-run 'proced))) nil nil 5 0 nil)" 15 (101 ("Achiever" "Buffer, buffers, everywhere" "Clean Desk" "Green Glowing faces" "Log Auditor" "Loyalist" "Modernist" "Package Neophyte" "Post Modernist" "Purest Vanilla" "Streamlined" "Tainted Love" "Traditionalist" "Tux's Friend" "Unlocker")) t nil)"##
        ]],
    )
}

fn display_when_earned_nil_unlocks_achievements_silently() -> ParityBatchCase {
    ParityBatchCase::value(
        "display_when_earned_nil_unlocks_achievements_silently",
        r##"(ach-test-with-live-buffer
 (let ((achievements-file (ach-test-path "achievements.eld")))
   (keyfreq-mode 1)
   (achievements-mode 1)
   (let ((achievements-display-when-earned nil))
     (execute-kbd-macro (kbd "C-x C-b C-h e C-x o"))
     (achievements-update-score))
   (let ((silent (list (ach-test-record "Buffer, buffers, everywhere")
                       (ach-test-record "Log Auditor")
                       achievements-score
                       (ach-test-unlock-messages)
                       (ach-test-log))))
     (execute-kbd-macro (kbd "C-h l C-x o"))
     (achievements-update-score)
     (list silent
           (ach-test-record "What did I just do?")
           (ach-test-record "Log Auditor")
           achievements-score
           (ach-test-unlock-messages)
           (ach-test-log)))))"##,
        expect![[
            r#"OK ((("Buffer, buffers, everywhere" "You've seen all the buffers that can be seen." t 5 nil t) ("Log Auditor" "You learned new things by using `view-echo-area-messages'." t 5 nil t) 70 nil no-log-buffer) ("What did I just do?" "You answered a question by using `(command-history view-lossage)'." :pending 5 nil nil) ("Log Auditor" "You learned new things by using `view-echo-area-messages'." t 5 nil t) 75 ("ACHIEVEMENT UNLOCKED: You’ve earned the ‘Unlocker’ achievement!") "You've earned the `Unlocker' achievement! [You have earned over 50 points in Emacs achievements.  Not bad.]")"#
        ]],
    )
}

fn disabling_an_achievement_removes_it_from_the_list_for_good() -> ParityBatchCase {
    ParityBatchCase::value(
        "disabling_an_achievement_removes_it_from_the_list_for_good",
        r##"(ach-test-with-live-buffer
 (let ((achievements-file (ach-test-path "achievements.eld")))
   (achievements-mode 1)
   (achievements-list-achievements)
   (goto-char (point-min))
   (re-search-forward "^.*Top o' the morning")
   (goto-char (line-beginning-position))
   (let ((before (list (tabulated-list-get-id)
                       (line-number-at-pos (point-max))
                       (key-binding (kbd "d"))
                       (lookup-key achievements-list-mode-map "d")
                       (eq (current-local-map) achievements-list-mode-map)
                       (commandp 'achievements-disable))))
     (cl-letf (((symbol-function 'y-or-n-p) (lambda (_prompt) t)))
       (call-interactively 'achievements-disable))
     (list before
           (ach-test-record "Top o' the morning")
           (line-number-at-pos (point-max))
           (ach-test-rows "Top o' the morning" "Achiever")
           achievements-score
           achievements-total
           (tabulated-list-get-id)))))"##,
        expect![[
            r#"OK (("Top o' the morning" 98 undefined nil t t) ("Top o' the morning" "You've used Emacs as a replacement for top." nil 5 nil nil) 115 (("Top o' the morning") ("Achiever" . " ✓    5 Achiever                       You used the achievements package.")) 65 590.5 "Achiever")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        achievements_mode_installs_and_removes_its_hook_and_idle_timer(),
        running_commands_unlocks_the_matching_achievements_and_logs_them(),
        an_achievement_needing_several_commands_stays_locked_until_all_have_run(),
        the_achievements_list_buffer_renders_rows_and_grows_when_refreshed(),
        achievements_are_saved_to_the_achievements_file_and_restored_from_it(),
        display_when_earned_nil_unlocks_achievements_silently(),
        disabling_an_achievement_removes_it_from_the_list_for_good(),
    ]
}
