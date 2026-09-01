use expect_test::expect;

use super::ParityBatchCase;

fn audio_notes_mode_exact_package_descriptor_origin_and_dependency_contract_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_exact_package_descriptor_origin_and_dependency_contract_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'audio-notes-mode
                                   package-alist)))
                                (extras
                                 (package-desc-extras descriptor))
                                ;; Mask the installed package's own
                                ;; directory.  Spelling it out pinned the
                                ;; harness's acquisition layout, so this
                                ;; expectation broke when the cache moved
                                ;; from package-cache/ to the
                                ;; revision-pinned source-install-cache/ --
                                ;; a harness change wearing the shape of a
                                ;; package regression.
                                (installed
                                 (directory-file-name
                                  (file-name-directory
                                   (getenv
                                    "NEOMACS_PACKAGE_SOURCE")))))
                           (list
                            (package-desc-name descriptor)
                            (package-version-join
                             (package-desc-version descriptor))
                            (package-desc-summary descriptor)
                            (package-desc-reqs descriptor)
                            (package-desc-kind descriptor)
                            (package-desc-archive descriptor)
                            (replace-regexp-in-string
                             (regexp-quote installed)
                             "[PACKAGE]"
                             (package-desc-dir descriptor)
                             t t)
                            (alist-get :commit extras)
                            (alist-get :revdesc extras)
                            (alist-get :url extras)
                            (alist-get :keywords extras)
                            (alist-get :authors extras)
                            (alist-get :maintainers extras)))"##,
        expect![[
            r#"OK (audio-notes-mode "20170611.2159" "Play audio notes synced from somewhere else." nil nil nil "[PACKAGE]" "fa38350829c7e97257efc746a010471d33748a68" "fa38350829c7" "https://github.com/Bruce-Connor/audio-notes-mode" ("hypermedia" "convenience") (("Artur Malabarba" . "bruce.connor.am@gmail.com")) (("Artur Malabarba" . "bruce.connor.am@gmail.com")))"#
        ]],
    )
}

fn audio_notes_mode_installed_payload_inventory_and_exact_archive_hashes_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "audio_notes_mode_installed_payload_inventory_and_exact_archive_hashes_match",
        r##"(let* ((directory
                                 (file-name-directory
                                  (getenv
                                   "NEOMACS_PACKAGE_SOURCE")))
                                (archive-files
                                 '("audio-notes-mode-pkg.el"
                                   "audio-notes-mode.el")))
                           (mapcar
                            (lambda (file)
                              (let ((path
                                     (expand-file-name
                                      file
                                      directory)))
                                (if
                                    (member file archive-files)
                                    (list
                                     file
                                     :archive
                                     (file-attribute-size
                                      (file-attributes path))
                                     (with-temp-buffer
                                       (insert-file-contents-literally path)
                                       (secure-hash
                                        'sha256
                                        (current-buffer))))
                                  (list
                                   file
                                   :generated
                                   (file-readable-p path)))))
                            (sort
                             (seq-filter
                              (lambda (file)
                                (file-regular-p
                                 (expand-file-name
                                  file
                                  directory)))
                              (directory-files
                               directory
                               nil
                               "\\`[^.]"))
                             #'string<)))"##,
        expect![[
            r#"OK (("audio-notes-mode-autoloads.el" :generated t) ("audio-notes-mode-pkg.el" :archive 469 "2a1e422c77fd0c59101523248bd0cef98a0f14d424618d7f874363553180ab65") ("audio-notes-mode.el" :archive 20079 "83c5bf06158a0cce041afaeb552284a52d4a49156b4857fc3a942bfe3fbfb7ad") ("audio-notes-mode.elc" :generated t))"#
        ]],
    )
}

fn audio_notes_mode_complete_callable_command_and_alias_surface_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_complete_callable_command_and_alias_surface_matches",
        r##"(let ((symbols
                                '(anm/bug-report
                                  anm/customize
                                  anm/-mplayer-send
                                  anm/-mplayer-parse-seconds
                                  anm/mplayer-seek-forward
                                  anm/mplayer-seek-backward
                                  anm/-is-mplayer-p
                                  anm/-is-alive-p
                                  anm/display-on-modeline
                                  anm/global-mode-string
                                  anm/play-next
                                  anm/play-current
                                  anm/play-pause-current
                                  anm/stop
                                  anm/play-file
                                  anm/list-files
                                  audio-notes-mode)))
                           (list
                            (mapcar
                             (lambda (symbol)
                               (list
                                symbol
                                (fboundp symbol)
                                (commandp symbol)
                                (help-function-arglist
                                 symbol
                                 t)
                                (interactive-form symbol)
                                (secure-hash
                                 'sha256
                                 (or
                                  (documentation
                                   symbol
                                   t)
                                  ""))))
                             symbols)
                            (eq
                             (indirect-function
                              'anm/play-current)
                             (indirect-function
                              'anm/play-pause-current))
                            (symbol-function
                             'anm/play-current)))"##,
        expect![[
            r#"OK (((anm/bug-report t t nil (interactive nil) "ec2c9351c6fc6f5b92ad84dd996e531f704cef755c6dc8888b5a4a712e73b083") (anm/customize t t nil (interactive nil) "14bb4b48d57ae76f19b07c0f2141a816d12f0d9b409ddcb194998542f850056a") (anm/-mplayer-send t nil (cmd) nil "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855") (anm/-mplayer-parse-seconds t nil (seconds) nil "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855") (anm/mplayer-seek-forward t t (N) (interactive "P") "23bb629ca80683b3425da217dcd29c93efc108fca91cb3c040e33f197006a807") (anm/mplayer-seek-backward t t (N) (interactive "P") "00977819a3d753a0160fe33a4ceba3386cabad251834232e8ccc2b3e18cd0ec0") (anm/-is-mplayer-p t nil nil nil "98ce167714f751b75c84d379c6a17b2f37cc85fb992fb8d6e3dbc4ab69538812") (anm/-is-alive-p t nil nil nil "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855") (anm/display-on-modeline t t (t-or-nil-or-color) (interactive "i") "382acf786d691a5f8fff7aae2343c45c20bd7340cb977124ee6faaa752d42ce3") (anm/global-mode-string t nil nil nil "76c2feb1834a8d1f412b3b7c9083a626a9b1bb0904730704a114325ecd65de69") (anm/play-next t t nil (interactive nil) "b69ac48a79b11a2f6d47e77d63d002027ea0a49046c4381e78186923137fc63e") (anm/play-current t t nil (interactive nil) "21b2f23aa6918a45a911ecbc38a779fb4669f234a45625461fff181ead3643f0") (anm/play-pause-current t t nil (interactive nil) "21b2f23aa6918a45a911ecbc38a779fb4669f234a45625461fff181ead3643f0") (anm/stop t t nil (interactive nil) "3a04bbb18cd25b62ee8478be99e10f4d2e25f29a79fab29abc76eaee873d485b") (anm/play-file t nil (file) nil "33e0b99c879cdfd24a72e65085c6dd0924f56c58a33ed087b379a0e7b87fa246") (anm/list-files t nil nil nil "c99b9ca3481b53cd7ea49dc687ee51314fe660f772e33c5bd263f5243ad6e05f") (audio-notes-mode t t (&optional arg) (interactive (list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle))) "ff1fa79d4d3ee5386d590f87aaa5778bdf5fed101ce5719a616b03af6286b329")) t anm/play-pause-current)"#
        ]],
    )
}

fn audio_notes_mode_complete_customization_defaults_and_metadata_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_complete_customization_defaults_and_metadata_match",
        r##"(let ((symbols
                                '(anm/display-greeting
                                  anm/notes-directory
                                  anm/goto-file
                                  anm/file-regexp
                                  anm/lighter
                                  anm/hook-into-org-pull
                                  anm/after-play-hook
                                  anm/before-play-hook
                                  anm/process-buffer-name
                                  anm/player-command
                                  anm/default-seek-step
                                  anm/delete-command)))
                           (mapcar
                            (lambda (symbol)
                              (list
                               symbol
                               (symbol-value symbol)
                               (get symbol 'custom-type)
                               (get symbol 'standard-value)
                               (get symbol 'custom-version)
                               (get symbol 'custom-package-version)
                               (get symbol 'custom-requests)))
                            symbols))"##,
        expect![[
            r#"OK ((anm/display-greeting t boolean (t) nil (audio-notes-mode . "0.1") nil) (anm/notes-directory "~/Dropbox/AudioNotes/" string ((concat (if (boundp 'org-directory) org-directory "~/Dropbox/") "AudioNotes/")) nil (audio-notes-mode . "0.7") nil) (anm/goto-file nil (choice string nil) (nil) nil nil nil) (anm/file-regexp "^[^\\.].*.\\(mp[34]\\|wav\\|3ga\\|3gpp\\|m4a\\)$" regexp ("^[^\\.].*.\\(mp[34]\\|wav\\|3ga\\|3gpp\\|m4a\\)$") nil nil nil) (anm/lighter " ▶" string ((if (char-displayable-p 9654) " ▶" " anm")) nil (audio-notes-mode . "0.1") nil) (anm/hook-into-org-pull nil (choice (const :tag "Always, activate on org-pull." t) (const :tag "Don't activate on org-pull." nil)) (nil) nil (audio-notes-mode . "0.1") nil) (anm/after-play-hook nil hook ('nil) nil (audio-notes-mode . "0.1") nil) (anm/before-play-hook nil hook ('nil) nil (audio-notes-mode . "0.1") nil) (anm/process-buffer-name "*Audio notes player*" string ("*Audio notes player*") nil (audio-notes-mode . "0.1") nil) (anm/player-command internal (choice (const :tag "Emacs internal player" internal) (cons (string :tag "Executable name") (repeat (choice (const :tag "File Name" file) (string :tag "Other Arguments"))))) ((cond ((executable-find "mplayer") anm/default-mplayer) ((executable-find "smplayer") anm/default-smplayer) ((executable-find "vlc") anm/default-vlc) (t 'internal))) nil (audio-notes-mode . "0.1") nil) (anm/default-seek-step 5 integer (5) (audio-notes-mode . "1.0") nil nil) (anm/delete-command #1=(delete-file file t) sexp ('#1#) nil (audio-notes-mode . "0.7") nil))"#
        ]],
    )
    .fresh_process()
}

fn audio_notes_mode_constants_runtime_state_and_documentation_contract_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_constants_runtime_state_and_documentation_contract_match",
        r##"(list
                          anm/version
                          anm/version-int
                          anm/default-mplayer
                          anm/default-vlc
                          anm/player-command-documentation
                          anm/greeting
                          (mapcar
                           (lambda (symbol)
                             (list
                              symbol
                              (boundp symbol)
                              (symbol-value symbol)
                              (documentation-property
                               symbol
                               'variable-documentation
                               t)))
                           '(anm/dired-buffer
                             anm/goto-file-buffer
                             anm/process-buffer
                             anm/process
                             anm/mode-line-color
                             anm/current
                             anm/did-visit
                             anm/found-files))
                          (featurep 'audio-notes-mode)
                          (get
                           'audio-notes-mode
                           'custom-group)
                          (get
                           'audio-notes-mode
                           'group-documentation))"##,
        expect![[
            r#"OK ("1.1.1" 7 ("mplayer" "-quiet" file) ("vlc" file) "Which media player to use for the audio files, must be a symbol or a list.\n\nIf it's the symbol 'internal: uses emacs' internal player.\n\nIf it's a list: the first element is the executable name (like\n\"mplayer\") and all following elements are arguments to be\npassed to it. All arguments must either be strings or the symbol\n'file, which will be replaced by the filename (you probably\nshould include 'file at least once). For example, the default\nvalue (if you have mplayer installed) is\n\n    %S\n\nEmacs internal player should be able to play wav files, but not\nmp4, so your decision on which to use should be based on this." "You're in `audio-notes-mode'. This mode will deactivate after you go through your notes, to quit manually use \\[audio-notes-mode].\n\\[anm/play-next]: DELETES this audio note and moves to the next one.\n\\[anm/play-current]: Replays this audio note.\nTo disable this message, edit `anm/display-greeting'." ((anm/dired-buffer t nil "The buffer displaying the notes.") (anm/goto-file-buffer t nil "The buffer the user asked to open.") (anm/process-buffer t nil "Process buffer.") (anm/process t nil "Process.") (anm/mode-line-color t "ForestGreen" "") (anm/current t nil "Currently played file.") (anm/did-visit t nil "Did we visit a file and mess up the configuration.") (anm/found-files t nil "")) t ((anm/display-greeting custom-variable) (anm/notes-directory custom-variable) (anm/goto-file custom-variable) (anm/file-regexp custom-variable) (anm/lighter custom-variable) (anm/hook-into-org-pull custom-variable) (anm/after-play-hook custom-variable) (anm/before-play-hook custom-variable) (anm/process-buffer-name custom-variable) (anm/player-command custom-variable) (anm/default-seek-step custom-variable) (anm/delete-command custom-variable) (audio-notes-mode custom-variable)) nil)"#
        ]],
    )
    .fresh_process()
}

fn audio_notes_mode_global_minor_mode_metadata_and_initial_keymap_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_global_minor_mode_metadata_and_initial_keymap_match",
        r##"(list
                          audio-notes-mode
                          (get
                           'audio-notes-mode
                           'function-documentation)
                          (get
                           'audio-notes-mode
                           'custom-type)
                          (get
                           'audio-notes-mode
                           'standard-value)
                          (get
                           'audio-notes-mode
                           'globalized-minor-mode)
                          (get
                           'audio-notes-mode
                           'minor-mode-function)
                          (assq
                           'audio-notes-mode
                           minor-mode-alist)
                          (assq
                           'audio-notes-mode
                           minor-mode-map-alist)
                          (mapcar
                           (lambda (key)
                             (list
                              key
                              (lookup-key
                               audio-notes-mode-map
                               (kbd key))))
                           '("C-c C-j"
                             "C-c C-k"
                             "C-c C-n"
                             "C-c C-p"
                             "C-c C-s"
                             "C-c C-q"
                             "C-c C-f"
                             "C-c C-b")))"##,
        expect![[
            r#"OK (nil nil boolean (nil) nil nil (audio-notes-mode anm/lighter) (audio-notes-mode keymap (3 keymap (17 . audio-notes-mode) (19 . anm/stop) (16 . anm/play-pause-current) (14 . anm/play-next) (11 . anm/play-pause-current) (10 . anm/play-next))) (("C-c C-j" anm/play-next) ("C-c C-k" anm/play-pause-current) ("C-c C-n" anm/play-next) ("C-c C-p" anm/play-pause-current) ("C-c C-s" anm/stop) ("C-c C-q" audio-notes-mode) ("C-c C-f" nil) ("C-c C-b" nil)))"#
        ]],
    )
    .fresh_process()
}

fn audio_notes_mode_source_reload_preserves_user_values_alias_and_advice_identity()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_source_reload_preserves_user_values_alias_and_advice_identity",
        r##"(let* ((source
                                 (getenv
                                  "NEOMACS_PACKAGE_SOURCE"))
                                (advice-before
                                 (ad-find-advice
                                  'org-mobile-pull
                                  'after
                                  'anm/after-org-mobile-pull-advice))
                                (function-before
                                 (symbol-function
                                  'anm/play-pause-current)))
                           (setq
                            anm/display-greeting :user-greeting
                            anm/notes-directory "/user/notes/"
                            anm/player-command '("custom-player" file)
                            anm/current "/user/current.wav")
                           (load source nil t t)
                           (list
                            anm/display-greeting
                            anm/notes-directory
                            anm/player-command
                            anm/current
                            (eq
                             (indirect-function
                              'anm/play-current)
                             (indirect-function
                              'anm/play-pause-current))
                            (eq
                             function-before
                             (symbol-function
                              'anm/play-pause-current))
                            (equal
                             advice-before
                             (ad-find-advice
                              'org-mobile-pull
                              'after
                              'anm/after-org-mobile-pull-advice))
                            (featurep
                             'audio-notes-mode)))"##,
        expect![[
            r#"OK (:user-greeting "/user/notes/" ("custom-player" file) "/user/current.wav" t nil t t)"#
        ]],
    )
}

fn audio_notes_mode_generated_autoloads_register_exact_commands_paths_and_feature()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_generated_autoloads_register_exact_commands_paths_and_feature",
        r##"(list
                          (featurep
                           'audio-notes-mode-autoloads)
                          (featurep
                           'audio-notes-mode)
                          (mapcar
                           (lambda (symbol)
                             (list
                              symbol
                              (autoloadp
                               (symbol-function symbol))
                              (nth
                               1
                               (symbol-function symbol))
                              (commandp symbol)))
                           '(anm/display-on-modeline
                             audio-notes-mode))
                          ;; Mask the installed package's own directory;
                          ;; see the note in the descriptor case above.
                          (mapcar
                           (lambda (entry)
                             (replace-regexp-in-string
                              (regexp-quote
                               (directory-file-name
                                (file-name-directory
                                 (getenv
                                  "NEOMACS_PACKAGE_SOURCE"))))
                              "[PACKAGE]"
                              entry
                              t t))
                           (seq-filter
                            (lambda (entry)
                              (string-match-p
                               "audio-notes-mode"
                               entry))
                            load-path))
                          (get
                           'audio-notes-mode
                           'definition-prefixes))"##,
        expect![[
            r#"OK (t nil ((anm/display-on-modeline t "audio-notes-mode" t) (audio-notes-mode t "audio-notes-mode" t)) ("[PACKAGE]") nil)"#
        ]],
    )
}

pub(super) fn registry_audio_notes_mode_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        audio_notes_mode_exact_package_descriptor_origin_and_dependency_contract_match(),
        audio_notes_mode_installed_payload_inventory_and_exact_archive_hashes_match(),
        audio_notes_mode_complete_callable_command_and_alias_surface_matches(),
        audio_notes_mode_complete_customization_defaults_and_metadata_match(),
        audio_notes_mode_constants_runtime_state_and_documentation_contract_match(),
        audio_notes_mode_global_minor_mode_metadata_and_initial_keymap_match(),
        audio_notes_mode_source_reload_preserves_user_values_alias_and_advice_identity(),
    ]
}

pub(super) fn registry_audio_notes_mode_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![audio_notes_mode_generated_autoloads_register_exact_commands_paths_and_feature()]
}
