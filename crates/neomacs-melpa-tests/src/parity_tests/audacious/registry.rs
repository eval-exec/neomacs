use expect_test::expect;

use super::ParityBatchCase;

fn audacious_descriptor_and_archive_sources_pin_exact_melpa_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_descriptor_and_archive_sources_pin_exact_melpa_payload",
        r##"(let* ((descriptor
                (cadr
                 (assq
                  'audacious
                  package-alist)))
               (directory
                (package-desc-dir descriptor))
               (sources
                (mapcar
                 (lambda (name)
                   (expand-file-name
                    name
                    directory))
                 '("audacious-pkg.el"
                   "audacious.el"))))
         (list
          (list
           (package-desc-name descriptor)
           (package-version-join
            (package-desc-version descriptor))
           (package-desc-summary descriptor)
           (package-desc-reqs descriptor)
           (package-desc-extras descriptor))
          (mapcar
           (lambda (file)
             (list
              (file-name-nondirectory file)
              (file-attribute-size
               (file-attributes file))
              (with-temp-buffer
                (insert-file-contents-literally file)
                (secure-hash
                 'sha256
                 (current-buffer)))))
           sources)))"##,
        expect![[
            r#"OK ((audacious "20210917.51" "Emacs interface to control audacious." ((helm (3 6 2)) (emacs (24 4))) ((:maintainers ("Hitoshi Uchida" . "hitoshi.uchida@gmail.com")) (:authors ("Hitoshi Uchida" . "hitoshi.uchida@gmail.com")) (:revdesc . "65c37f12a5c7") (:commit . "65c37f12a5c774a0ae434beee27ff7737006dd2f") (:url . "https://github.com/shishimaru/audacious.el"))) (("audacious-pkg.el" 436 "450f8e945f2c93feefc07cd849b257ed885c1845104800b990e0f49756686583") ("audacious.el" 11019 "214cdcdbd41bba72c25fc53b4c1cb2d75116d39f91edb9394b411db543ee628f")))"#
        ]],
    )
}

fn audacious_complete_callable_command_interactive_and_arglist_surface_matches() -> ParityBatchCase
{
    ParityBatchCase::value(
        "audacious_complete_callable_command_interactive_and_arglist_surface_matches",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (fboundp symbol)
            (commandp symbol)
            (interactive-form symbol)
            (help-function-arglist
             symbol
             t)
            (file-name-nondirectory
             (symbol-file
              symbol
              'defun))))
         '(audacious-kill
           audacious-pause
           audacious-play
           audacious-playlist
           audacious-playlist--goto
           audacious-playlist-goto
           audacious-playlist-next
           audacious-playlist-prev
           audacious-playlist-show-current-info
           audacious-random-toggle
           audacious-repeat-toggle
           audacious-run
           audacious-song-goto
           audacious-song-goto-helm
           audacious-song-next
           audacious-song-prev
           audacious-song-seek
           audacious-song-seek-backward
           audacious-song-seek-forward
           audacious-song-show-current-info
           audacious-status
           audacious-stop
           audacious-string-integer-p
           audacious-volume
           audacious-volume-down
           audacious-volume-up))"##,
        expect![[
            r#"OK ((audacious-kill t t (interactive nil) nil "audacious.el") (audacious-pause t t (interactive nil) nil "audacious.el") (audacious-play t t (interactive nil) nil "audacious.el") (audacious-playlist t t (interactive nil) nil "audacious.el") (audacious-playlist--goto t nil nil (num) "audacious.el") (audacious-playlist-goto t t (interactive nil) nil "audacious.el") (audacious-playlist-next t t (interactive nil) nil "audacious.el") (audacious-playlist-prev t t (interactive nil) nil "audacious.el") (audacious-playlist-show-current-info t t (interactive nil) nil "audacious.el") (audacious-random-toggle t t (interactive nil) nil "audacious.el") (audacious-repeat-toggle t t (interactive nil) nil "audacious.el") (audacious-run t t (interactive nil) nil "audacious.el") (audacious-song-goto t t (interactive nil) nil "audacious.el") (audacious-song-goto-helm t t (interactive nil) nil "audacious.el") (audacious-song-next t t (interactive nil) nil "audacious.el") (audacious-song-prev t t (interactive nil) nil "audacious.el") (audacious-song-seek t t (interactive "MSeek +- sec: ") (time) "audacious.el") (audacious-song-seek-backward t t (interactive nil) nil "audacious.el") (audacious-song-seek-forward t t (interactive nil) nil "audacious.el") (audacious-song-show-current-info t t (interactive nil) nil "audacious.el") (audacious-status t t (interactive nil) nil "audacious.el") (audacious-stop t t (interactive nil) nil "audacious.el") (audacious-string-integer-p t nil nil (string) "audacious.el") (audacious-volume t t (interactive "M[+|-]percent: ") (vol) "audacious.el") (audacious-volume-down t t (interactive nil) nil "audacious.el") (audacious-volume-up t t (interactive nil) nil "audacious.el"))"#
        ]],
    )
}

fn audacious_all_function_documentation_contracts_are_exact_and_readable() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_all_function_documentation_contracts_are_exact_and_readable",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (documentation symbol t)))
         '(audacious-run
           audacious-kill
           audacious-volume
           audacious-volume-up
           audacious-volume-down
           audacious-play
           audacious-pause
           audacious-stop
           audacious-status
           audacious-song-next
           audacious-song-prev
           audacious-song-goto
           audacious-song-goto-helm
           audacious-song-seek
           audacious-song-seek-backward
           audacious-song-seek-forward
           audacious-song-show-current-info
           audacious-random-toggle
           audacious-repeat-toggle
           audacious-playlist
           audacious-playlist-show-current-info
           audacious-playlist--goto
           audacious-playlist-goto
           audacious-playlist-next
           audacious-playlist-prev
           audacious-string-integer-p))"##,
        expect![[
            r#"OK ((audacious-run "Launch audacious with headless mode as daemon.") (audacious-kill "Shutdown audacious process.") (audacious-volume "Manually increase / decrease the volume by the specified VOL percent.") (audacious-volume-up "Increase the volume by 10%.") (audacious-volume-down "Decrease the volume by 10%.") (audacious-play "Start to play.") (audacious-pause "Pause the playback.") (audacious-stop "Stop the playback.") (audacious-status "Show the current status of audacious.") (audacious-song-next "Play the next song in the current playlist.") (audacious-song-prev "Play the previous song in the current playlist.") (audacious-song-goto "Select a song with an inputted number.") (audacious-song-goto-helm "Select a song with helm interface.") (audacious-song-seek "Seek the song by TIME in seconds.") (audacious-song-seek-backward "Seek backward by 10 seconds.") (audacious-song-seek-forward "Seek forward by 10 seconds.") (audacious-song-show-current-info "Show information of the current song.") (audacious-random-toggle "Toggle the random playback.") (audacious-repeat-toggle "Toggle the repeat playback.") (audacious-playlist "Show the songs of the current playlist.") (audacious-playlist-show-current-info "Show the name of the current playlist.") (audacious-playlist--goto "Select a playlist by NUM.") (audacious-playlist-goto "Select a playlist with an inputted number.") (audacious-playlist-next "Select a next playlist.") (audacious-playlist-prev "Select a previous playlist.") (audacious-string-integer-p "Test the STRING is number or not."))"#
        ]],
    )
}

fn audacious_declared_variables_custom_schema_defaults_and_sources_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_declared_variables_custom_schema_defaults_and_sources_are_exact",
        r##"(list
         (get
          'audacious
          'custom-group)
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (and
              (boundp symbol)
              t)
             (symbol-value symbol)
             (special-variable-p symbol)
             (and
              (custom-variable-p symbol)
              t)
             (get symbol 'custom-type)
             (get symbol 'standard-value)
             (file-name-nondirectory
              (symbol-file
               symbol
               'defvar))))
          '(audacious-command
            audacious-msg
            audacious-playlist-position
            audacious-playlist-length
            audacious-playlist-name
            audacious-song-title
            audacious-song-position
            audacious-song-length)))"##,
        expect![[
            r#"OK (((audacious-command custom-variable)) ((audacious-command t "/fixture/bin/audtool" t t string ((executable-find "audtool")) "audacious.el") (audacious-msg t "" t nil nil nil "audacious.el") (audacious-playlist-position t nil t nil nil nil "audacious.el") (audacious-playlist-length t nil t nil nil nil "audacious.el") (audacious-playlist-name t nil t nil nil nil "audacious.el") (audacious-song-title t nil t nil nil nil "audacious.el") (audacious-song-position t nil t nil nil nil "audacious.el") (audacious-song-length t nil t nil nil nil "audacious.el")))"#
        ]],
    )
    .fresh_process()
}

fn audacious_exact_runtime_dependency_pin_is_activated_without_loading_real_helm() -> ParityBatchCase
{
    ParityBatchCase::value(
        "audacious_exact_runtime_dependency_pin_is_activated_without_loading_real_helm",
        r##"(let ((audacious-descriptor
                (package--get-activatable-pkg
                 'audacious))
               (helm-descriptor
                (package--get-activatable-pkg
                 'helm)))
         (list
          package-load-list
          (mapcar
           (lambda (entry)
             (pcase-let
                 ((`(,name
                     ,descriptor
                     ,expected-version)
                   entry))
               (list
                name
                (package-version-join
                 (package-desc-version
                  descriptor))
                (and
                 (memq
                  name
                  package-activated-list)
                 t)
                (equal
                 (package-version-join
                  (package-desc-version
                   descriptor))
                 expected-version)
                (file-name-nondirectory
                 (directory-file-name
                  (package-desc-dir
                   descriptor))))))
           `((audacious
              ,audacious-descriptor
              "20210917.51")
             (helm
              ,helm-descriptor
              "20260728.709")))
          (featurep 'helm)
          (fboundp 'helm)
          (fboundp
           'helm-build-sync-source)))"##,
        expect![[
            r#"OK ((all (audacious "20210917.51") (helm "20260728.709")) ((audacious "20210917.51" t t "audacious-20210917.51") (helm "20260728.709" t t "helm-20260728.709")) t t nil)"#
        ]],
    )
}

fn audacious_source_reload_preserves_runtime_state_and_customized_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_source_reload_preserves_runtime_state_and_customized_command",
        r##"(let ((source
                (getenv
                 "NEOMACS_PACKAGE_SOURCE")))
         (setq audacious-command
               "/custom/bin/audtool"
               audacious-msg
               "queued rows\n"
               audacious-playlist-position
               :playlist-position
               audacious-playlist-length
               :playlist-length
               audacious-playlist-name
               :playlist-name
               audacious-song-title
               :song-title
               audacious-song-position
               :song-position
               audacious-song-length
               :song-length)
         (load source nil t)
         (list
          audacious-command
          audacious-msg
          audacious-playlist-position
          audacious-playlist-length
          audacious-playlist-name
          audacious-song-title
          audacious-song-position
          audacious-song-length
          (featurep 'audacious)))"##,
        expect![[
            r#"OK ("/custom/bin/audtool" "queued rows\n" :playlist-position :playlist-length :playlist-name :song-title :song-position :song-length t)"#
        ]],
    )
}

fn audacious_generated_autoload_registers_only_prefix_and_feature() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_generated_autoload_registers_only_prefix_and_feature",
        r##"(let* ((file
                 (locate-library
                  "audacious-autoloads"))
                (history
                 (assoc file load-history))
                (prefix-files
                 (if
                     (hash-table-p
                      definition-prefixes)
                     (gethash
                      "audacious-"
                      definition-prefixes)
                   (cdr
                    (assoc
                     "audacious-"
                     definition-prefixes)))))
         (list
          (featurep
           'audacious-autoloads)
          (featurep
           'audacious)
          (mapcar
           (lambda (event)
             (list
              (car event)
              (cdr event)))
           (seq-filter
            (lambda (event)
              (memq
               (car-safe event)
               '(defun provide)))
            (cdr history)))
          (sort
           (delete-dups
            (copy-sequence
             prefix-files))
           #'string<)
          (mapcar
           (lambda (symbol)
             (list
              symbol
              (fboundp symbol)
              (boundp symbol)))
           '(audacious-run
             audacious-command
             audacious-song-goto-helm))))"##,
        expect![[
            r#"OK (t nil ((provide audacious-autoloads)) ("audacious") ((audacious-run nil nil) (audacious-command nil nil) (audacious-song-goto-helm nil nil)))"#
        ]],
    )
}

pub(super) fn registry_audacious_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        audacious_descriptor_and_archive_sources_pin_exact_melpa_payload(),
        audacious_complete_callable_command_interactive_and_arglist_surface_matches(),
        audacious_all_function_documentation_contracts_are_exact_and_readable(),
        audacious_declared_variables_custom_schema_defaults_and_sources_are_exact(),
        audacious_exact_runtime_dependency_pin_is_activated_without_loading_real_helm(),
        audacious_source_reload_preserves_runtime_state_and_customized_command(),
    ]
}

pub(super) fn registry_audacious_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![audacious_generated_autoload_registers_only_prefix_and_feature()]
}
