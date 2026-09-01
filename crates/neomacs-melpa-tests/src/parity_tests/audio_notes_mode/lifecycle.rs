use expect_test::expect;

use super::ParityBatchCase;

fn audio_notes_mode_org_mobile_advice_runs_only_when_hook_and_real_queue_are_both_present()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_org_mobile_advice_runs_only_when_hook_and_real_queue_are_both_present",
        r##"(let (events
                               queue
                               anm/hook-into-org-pull)
                           (fset
                            'org-mobile-pull
                            (lambda (&rest arguments)
                              (push
                               (list :base arguments)
                               events)
                              :pulled))
                           (ad-activate
                            'org-mobile-pull)
                           (cl-letf
                               (((symbol-function
                                  'anm/list-files)
                                 (lambda ()
                                   (push
                                    (list :list queue)
                                    events)
                                   queue))
                                ((symbol-function
                                  'audio-notes-mode)
                                 (lambda (&optional argument)
                                   (push
                                    (list :mode argument)
                                    events)
                                   :activated)))
                             (setq anm/hook-into-org-pull t
                                   queue '("/notes/one.wav"))
                             (let ((both
                                    (org-mobile-pull
                                     :first)))
                               (setq anm/hook-into-org-pull nil
                                     queue '("/notes/two.wav"))
                               (let ((disabled
                                      (org-mobile-pull
                                       :second)))
                                 (setq anm/hook-into-org-pull t
                                       queue nil)
                                 (let ((empty
                                        (org-mobile-pull
                                         :third)))
                                   (list
                                    both
                                    disabled
                                    empty
                                    (nreverse events)))))))"##,
        expect![[
            r#"OK (:pulled :pulled :pulled ((:base (:first)) (:list ("/notes/one.wav")) (:mode 1) (:base (:second)) (:base (:third)) (:list nil)))"#
        ]],
    )
}

fn audio_notes_mode_org_mobile_advice_registration_name_kind_and_docstring_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "audio_notes_mode_org_mobile_advice_registration_name_kind_and_docstring_match",
        r##"(let ((advice
                                (ad-find-advice
                                 'org-mobile-pull
                                 'after
                                 'anm/after-org-mobile-pull-advice)))
                           (list
                            (car advice)
                            (nth 1 advice)
                            (nth 2 advice)
                            (car
                             (nth 3 advice))
                            (nth
                             1
                             (nth 3 advice))
                            (nth
                             2
                             (nth 3 advice))
                            (nth
                             3
                             (nth 3 advice))
                            (nthcdr
                             4
                             (nth 3 advice))
                            (ad-is-active
                             'org-mobile-pull)
                            (consp
                             (get
                              'org-mobile-pull
                              'ad-advice-info))))"##,
        expect![[
            r#"OK (anm/after-org-mobile-pull-advice nil t advice lambda nil "Check for audio notes after every org-pull." ((when (and anm/hook-into-org-pull (anm/list-files)) (audio-notes-mode 1))) nil t)"#
        ]],
    )
    .fresh_process()
}

fn audio_notes_mode_greeting_resolves_live_command_bindings_from_mode_map() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_greeting_resolves_live_command_bindings_from_mode_map",
        r##"(let ((default
                                (substitute-command-keys
                                 anm/greeting)))
                           (define-key
                            audio-notes-mode-map
                            (kbd "C-c C-j")
                            nil)
                           (define-key
                            audio-notes-mode-map
                            (kbd "C-c C-x")
                            #'anm/play-next)
                           (let ((remapped
                                  (substitute-command-keys
                                   anm/greeting)))
                             (list
                              default
                              remapped)))"##,
        expect![[
            r#"OK (#("You’re in ‘audio-notes-mode’. This mode will deactivate after you go through your notes, to quit manually use M-x audio-notes-mode.\nM-x anm/play-next: DELETES this audio note and moves to the next one.\nM-x anm/play-current: Replays this audio note.\nTo disable this message, edit ‘anm/display-greeting’." 110 130 (font-lock-face help-key-binding face help-key-binding) 132 136 (font-lock-face help-key-binding face help-key-binding) 136 149 (font-lock-face help-key-binding face help-key-binding) 202 206 (font-lock-face help-key-binding face help-key-binding) 206 222 (font-lock-face help-key-binding face help-key-binding)) #("You’re in ‘audio-notes-mode’. This mode will deactivate after you go through your notes, to quit manually use M-x audio-notes-mode.\nM-x anm/play-next: DELETES this audio note and moves to the next one.\nM-x anm/play-current: Replays this audio note.\nTo disable this message, edit ‘anm/display-greeting’." 110 130 (font-lock-face help-key-binding face help-key-binding) 132 136 (font-lock-face help-key-binding face help-key-binding) 136 149 (font-lock-face help-key-binding face help-key-binding) 202 206 (font-lock-face help-key-binding face help-key-binding) 206 222 (font-lock-face help-key-binding face help-key-binding)))"#
        ]],
    )
}

fn audio_notes_mode_activation_with_empty_queue_returns_to_disabled_state_and_reports_directory()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_activation_with_empty_queue_returns_to_disabled_state_and_reports_directory",
        r##"(let ((anm/notes-directory
                                "/fixed/empty-notes/")
                               (anm/player-command
                                'internal)
                               (anm/found-files nil)
                               events)
                           (cl-letf
                               (((symbol-function
                                  'anm/list-files)
                                 (lambda ()
                                   (push :list events)
                                   nil))
                                ((symbol-function
                                  'message)
                                 (lambda
                                     (format-string &rest arguments)
                                   (let ((text
                                          (apply
                                           #'format
                                           format-string
                                           arguments)))
                                     (push
                                      (list :message text)
                                      events)
                                     text))))
                             (let ((result
                                    (audio-notes-mode 1)))
                               (list
                                result
                                audio-notes-mode
                                anm/found-files
                                anm/current
                                (nreverse events)))))"##,
        expect![[
            r#"OK (nil nil nil nil (:list (:message "[OAN]:No audio notes found in \"/fixed/empty-notes/\".")))"#
        ]],
    )
}

fn audio_notes_mode_nil_player_rolls_back_activation_before_signaling_configuration_error()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_nil_player_rolls_back_activation_before_signaling_configuration_error",
        r##"(let ((anm/notes-directory
                                "/fixed/notes/")
                               (anm/player-command nil)
                               (anm/found-files nil)
                               events)
                           (cl-letf
                               (((symbol-function
                                  'anm/list-files)
                                 (lambda ()
                                   (push :unexpected-list events)
                                   '("/fixed/notes/one.wav")))
                                ((symbol-function
                                  'message)
                                 (lambda
                                     (format-string &rest arguments)
                                   (let ((text
                                          (apply
                                           #'format
                                           format-string
                                           arguments)))
                                     (push
                                      (list :message text)
                                      events)
                                     text))))
                             (list
                              (audio-notes-test-error
                               (lambda ()
                                 (audio-notes-mode 1)))
                              audio-notes-mode
                              anm/found-files
                              (nreverse events))))"##,
        expect![[
            r#"OK ((:signal error ("‘anm/player-command’ can’t be nil.")) nil nil ((:message "[OAN]:No audio notes found in \"/fixed/notes/\".")))"#
        ]],
    )
}

fn audio_notes_mode_full_ui_lifecycle_visits_target_builds_player_layout_and_cleans_everything()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_full_ui_lifecycle_visits_target_builds_player_layout_and_cleans_everything",
        r##"(let* ((notes-directory
                                 (audio-notes-test-directory
                                  "full-lifecycle-notes"))
                                (first
                                 (audio-notes-test-write
                                  notes-directory
                                  "01-first.wav"
                                  "audio"))
                                (goto-path
                                 (expand-file-name
                                  "inbox.org"
                                  default-directory))
                                (goto-buffer
                                 (generate-new-buffer
                                  " *audio-notes-goto*"))
                                (dired-buffer
                                 (generate-new-buffer
                                  " *audio-notes-directory*"))
                                (anm/notes-directory
                                 notes-directory)
                                (anm/goto-file goto-path)
                                (anm/player-command
                                 '("mock-player" file))
                                (anm/display-greeting nil)
                                (anm/found-files nil)
                                (anm/did-visit nil)
                                (anm/current nil)
                                events
                                process-buffer)
                           (unwind-protect
                               (progn
                                 (with-current-buffer dired-buffer
                                   (insert
                                    "header\n01-first.wav\n"))
                                 (cl-letf
                                     (((symbol-function
                                        'anm/list-files)
                                       (lambda ()
                                         (push :list-files events)
                                         (list first)))
                                      ((symbol-function
                                        'window-configuration-to-register)
                                       (lambda (register)
                                         (push
                                          (list :save-layout register)
                                          events)
                                         :saved))
                                      ((symbol-function
                                        'delete-other-windows)
                                       (lambda (&optional window)
                                         (push
                                          (list :delete-other window)
                                          events)
                                         :deleted))
                                      ((symbol-function
                                        'selected-window)
                                       (lambda ()
                                         (push :selected-window events)
                                         'focus-window))
                                      ((symbol-function
                                        'split-window-right)
                                       (lambda (&rest arguments)
                                         (push
                                          (cons :split-right arguments)
                                          events)
                                         'directory-window))
                                      ((symbol-function
                                        'split-window-below)
                                       (lambda (&optional size)
                                         (push
                                          (list :split-below size)
                                          events)
                                         'process-window))
                                      ((symbol-function
                                        'select-window)
                                       (lambda
                                           (window &optional norecord)
                                         (push
                                          (list
                                           :select
                                           window
                                           norecord)
                                          events)
                                         window))
                                      ((symbol-function
                                        'find-file)
                                       (lambda
                                           (filename &optional wildcards)
                                         (push
                                          (list
                                           :find-file
                                           filename
                                           wildcards)
                                          events)
                                         (let ((buffer
                                                (if
                                                    (equal filename goto-path)
                                                    goto-buffer
                                                  dired-buffer)))
                                           (set-buffer buffer)
                                           buffer)))
                                      ((symbol-function
                                        'hl-line-mode)
                                       (lambda (&optional argument)
                                         (push
                                          (list :hl-line argument)
                                          events)
                                         argument))
                                      ((symbol-function
                                        'revert-buffer)
                                       (lambda (&rest arguments)
                                         (push
                                          (cons :revert arguments)
                                          events)
                                         :reverted))
                                      ((symbol-function
                                        'line-number-at-pos)
                                       (lambda (&optional position absolute)
                                         (push
                                          (list
                                           :line-number
                                           position
                                           absolute)
                                          events)
                                         3))
                                      ((symbol-function
                                        'switch-to-buffer)
                                       (lambda
                                           (buffer-or-name &optional norecord force-same-window)
                                         (push
                                          (list
                                           :switch
                                           (buffer-name buffer-or-name)
                                           norecord
                                           force-same-window)
                                          events)
                                         buffer-or-name))
                                      ((symbol-function
                                        'anm/play-current)
                                       (lambda ()
                                         (push
                                          (list :play anm/current)
                                          events)
                                         (setq anm/current first)
                                         :playing))
                                      ((symbol-function
                                        'jump-to-register)
                                       (lambda (register &optional delete)
                                         (push
                                          (list
                                           :restore-layout
                                           register
                                           delete)
                                          events)
                                         :restored))
                                      ((symbol-function
                                        'get-buffer-window)
                                       (lambda
                                           (buffer-or-name &optional frame)
                                         (push
                                          (list
                                           :buffer-window
                                           (buffer-name buffer-or-name)
                                           frame)
                                          events)
                                         'directory-window))
                                      ((symbol-function
                                        'delete-window)
                                       (lambda (&optional window)
                                         (push
                                          (list :delete-window window)
                                          events)
                                         :window-deleted))
                                      ((symbol-function
                                        'bury-buffer)
                                       (lambda (&optional buffer-or-name)
                                         (push
                                          (list
                                           :bury
                                           (buffer-name
                                            (or
                                             buffer-or-name
                                             (current-buffer))))
                                          events)
                                         :buried)))
                                   (let ((enabled-result
                                          (audio-notes-mode 1)))
                                     (setq process-buffer
                                           anm/process-buffer)
                                     (let ((enabled-state
                                            (list
                                             enabled-result
                                             audio-notes-mode
                                             anm/found-files
                                             anm/did-visit
                                             (eq
                                              anm/goto-file-buffer
                                              goto-buffer)
                                             (eq
                                              anm/dired-buffer
                                              dired-buffer)
                                             (buffer-name
                                              anm/process-buffer)
                                             (file-name-nondirectory
                                              anm/current))))
                                       (let ((disabled-result
                                              (audio-notes-mode -1)))
                                         (list
                                          enabled-state
                                          (list
                                           disabled-result
                                           audio-notes-mode
                                           anm/found-files
                                           anm/did-visit
                                           anm/current
                                           (buffer-live-p
                                            process-buffer))
                                          (nreverse events)))))))
                             (when
                                 (buffer-live-p goto-buffer)
                               (kill-buffer goto-buffer))
                             (when
                                 (buffer-live-p dired-buffer)
                               (kill-buffer dired-buffer))
                             (when
                                 (buffer-live-p process-buffer)
                               (kill-buffer process-buffer))))"##,
        expect![[
            r#"OK ((t t t t t t "*Audio notes player*" "01-first.wav") (nil nil nil nil nil nil) (:list-files (:save-layout :anm/before-anm-configuration) (:delete-other nil) (:find-file "[ORACLE-SANDBOX]/inbox.org" nil) :selected-window (:split-right) (:select directory-window nil) (:find-file "[ORACLE-SANDBOX]/full-lifecycle-notes/" nil) (:hl-line 1) (:revert) (:line-number 21 nil) (:split-below 2) (:select process-window nil) (:switch "*Audio notes player*" nil nil) (:select focus-window nil) (:play nil) (:restore-layout :anm/before-anm-configuration nil) (:bury " *audio-notes-goto*") (:buffer-window " *audio-notes-directory*" nil) (:buffer-window " *audio-notes-directory*" nil) (:delete-window directory-window) (:bury " *audio-notes-directory*")))"#
        ]],
    )
}

fn audio_notes_mode_mplayer_activation_installs_seek_bindings_even_when_queue_is_empty()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_mplayer_activation_installs_seek_bindings_even_when_queue_is_empty",
        r##"(let ((anm/notes-directory
                                "/fixed/no-notes/")
                               (anm/player-command
                                '("mplayer" "-quiet" file))
                               (anm/found-files nil))
                           (define-key
                            audio-notes-mode-map
                            (kbd "C-c C-f")
                            nil)
                           (define-key
                            audio-notes-mode-map
                            (kbd "C-c C-b")
                            nil)
                           (cl-letf
                               (((symbol-function
                                  'anm/list-files)
                                 (lambda () nil))
                                ((symbol-function
                                  'message)
                                 (lambda (&rest _arguments)
                                   nil)))
                             (let ((result
                                    (audio-notes-mode 1)))
                               (list
                                result
                                audio-notes-mode
                                (lookup-key
                                 audio-notes-mode-map
                                 (kbd "C-c C-f"))
                                (lookup-key
                                 audio-notes-mode-map
                                 (kbd "C-c C-b"))))))"##,
        expect!["OK (nil nil anm/mplayer-seek-forward anm/mplayer-seek-backward)"],
    )
}

fn audio_notes_mode_switching_away_from_mplayer_keeps_previously_installed_seek_bindings()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_switching_away_from_mplayer_keeps_previously_installed_seek_bindings",
        r##"(let ((anm/notes-directory
                                "/fixed/no-notes/")
                               (anm/found-files nil))
                           (define-key
                            audio-notes-mode-map
                            (kbd "C-c C-f")
                            nil)
                           (define-key
                            audio-notes-mode-map
                            (kbd "C-c C-b")
                            nil)
                           (cl-letf
                               (((symbol-function
                                  'anm/list-files)
                                 (lambda () nil))
                                ((symbol-function
                                  'message)
                                 (lambda (&rest _arguments)
                                   nil)))
                             (setq anm/player-command
                                   '("mplayer" file))
                             (audio-notes-mode 1)
                             (let ((after-mplayer
                                    (list
                                     (lookup-key
                                      audio-notes-mode-map
                                      (kbd "C-c C-f"))
                                     (lookup-key
                                      audio-notes-mode-map
                                      (kbd "C-c C-b")))))
                               (setq anm/player-command
                                     '("vlc" file))
                               (audio-notes-mode 1)
                               (list
                                after-mplayer
                                (lookup-key
                                 audio-notes-mode-map
                                 (kbd "C-c C-f"))
                                (lookup-key
                                 audio-notes-mode-map
                                 (kbd "C-c C-b"))
                                audio-notes-mode))))"##,
        expect![
            "OK ((anm/mplayer-seek-forward anm/mplayer-seek-backward) anm/mplayer-seek-forward anm/mplayer-seek-backward nil)"
        ],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        audio_notes_mode_org_mobile_advice_runs_only_when_hook_and_real_queue_are_both_present(),
        audio_notes_mode_org_mobile_advice_registration_name_kind_and_docstring_match(),
        audio_notes_mode_greeting_resolves_live_command_bindings_from_mode_map(),
        audio_notes_mode_activation_with_empty_queue_returns_to_disabled_state_and_reports_directory(),
        audio_notes_mode_nil_player_rolls_back_activation_before_signaling_configuration_error(),
        audio_notes_mode_full_ui_lifecycle_visits_target_builds_player_layout_and_cleans_everything(),
        audio_notes_mode_mplayer_activation_installs_seek_bindings_even_when_queue_is_empty(),
        audio_notes_mode_switching_away_from_mplayer_keeps_previously_installed_seek_bindings(),
    ]
}
