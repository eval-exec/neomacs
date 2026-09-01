use expect_test::expect;

use super::ParityBatchCase;

fn extracts_prompted_subtitles_and_uses_timestamp_links() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "youtube-sub-extractor-prompted"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (temporary-file-directory (file-name-as-directory sandbox))
        (executable (expand-file-name "yt-dlp-parity" sandbox))
        (calls-file (expand-file-name "calls.log" sandbox))
        (youtube-sub-extractor-executable-path executable)
        (youtube-sub-extractor-language-choice t)
        (youtube-sub-extractor-timestamps 'right-margin)
        (youtube-sub-extractor-min-chunk-size 5)
        (video-url
         "https://www.youtube.com/watch?v=incident-parity&list=release&t=99")
        (kill-ring nil)
        (kill-ring-yank-pointer nil)
        (interprogram-cut-function nil)
        prompts messages browsed result buffer)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory sandbox t)
         (neomacs-melpa-youtube-sub-extractor--write-executable
          executable
          "printf '%s\\n' \"$*\" >> calls.log\ncase \" $* \" in\n  *\" --list-subs \"*)\n    printf '%s\\n' '[info] Available subtitles for incident-parity:' 'Language Name Formats' 'en English vtt, srv3' 'es Spanish vtt, srv3'\n    ;;\n  *)\n    printf '%s\\n' 'WEBVTT' '' '00:00:01.250 --> 00:00:03.000' 'Incident <b>detected</b>' 'service degraded' '' '00:00:03.000 --> 00:00:05.000' 'service degraded' 'Mitigation λ applied' '' '00:00:05.000 --> 00:00:09.000' 'Monitoring stable' > 'Incident response λ [incident-parity].en.vtt'\n    printf '%s\\n' '[info] Writing video subtitles' '[download] Destination: Incident response λ [incident-parity].en.vtt' '[download] 100% of 1.00KiB'\n    ;;\nesac\n")
         (cl-letf (((symbol-function 'completing-read)
                    (lambda (prompt collection &rest _)
                      (push
                       (list prompt
                             (mapcar #'substring-no-properties
                                     (all-completions "" collection)))
                       prompts)
                      "en"))
                   ((symbol-function 'browse-url)
                    (lambda (url &rest _)
                      (push url browsed)
                      url))
                   ((symbol-function 'message)
                    (lambda (format-string &rest arguments)
                      (let ((text (apply #'format format-string arguments)))
                        (push text messages)
                        text))))
           (youtube-sub-extractor-extract-subs video-url)
           (setq buffer (current-buffer))
           (goto-char (point-min))
           (search-forward "Incident detected")
           (let* ((timestamp
                   (get-text-property (line-beginning-position) 'timestamp))
                  (copied (youtube-sub-extractor-copy-ts-link))
                  (visited (youtube-sub-extractor-browse-ts-link)))
             (setq result
                   (list
                    :buffer (buffer-name)
                    :text (buffer-substring-no-properties
                           (point-min) (point-max))
                    :timestamp timestamp
                    :overlays
                    (neomacs-melpa-youtube-sub-extractor--overlay-state)
                    :mode
                    (list youtube-sub-extractor-subtitles-mode
                          buffer-read-only
                          (lookup-key youtube-sub-extractor-subtitles-mode-map
                                      (kbd "RET"))
                          (lookup-key youtube-sub-extractor-subtitles-mode-map
                                      (kbd "C-c C-o")))
                    :margins (window-margins (get-buffer-window buffer))
                    :video-url video-url
                    :copied copied
                    :kill (car kill-ring)
                    :visited visited
                    :browsed (nreverse browsed)
                    :prompts (nreverse prompts)
                    :calls
                    (split-string
                     (neomacs-melpa-youtube-sub-extractor--file-string
                      calls-file)
                     "\n" t)
                    :download-removed
                    (not
                     (file-exists-p
                      (expand-file-name
                       "Incident response λ [incident-parity].en.vtt"
                       sandbox)))
                    :messages (nreverse messages)))))
         result)
     (when (buffer-live-p buffer)
       (kill-buffer buffer))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:buffer "Incident response λ [incident-parity].en" :text "Incident response λ [incident-parity].en\n\nIncident detected service degraded Mitigation λ applied\nMonitoring stable\n" :timestamp "00:00:01.250 --> 00:00:05.000" :overlays ((44 99 "00:01" ((margin right-margin) "00:01")) (100 117 "00:05" ((margin right-margin) "00:05"))) :mode (t t youtube-sub-extractor-copy-ts-link youtube-sub-extractor-browse-ts-link) :margins (nil . 9) :video-url "https://www.youtube.com/watch?v=incident-parity&list=release&t=99" :copied "https://www.youtube.com/watch?t=1&list=release&v=incident-parity" :kill "https://www.youtube.com/watch?t=1&list=release&v=incident-parity" :visited "https://www.youtube.com/watch?t=1&list=release&v=incident-parity" :browsed ("https://www.youtube.com/watch?t=1&list=release&v=incident-parity") :prompts (("Choose the language: " ("en" "es"))) :calls ("--list-subs --no-simulate --skip-download --no-playlist https://www.youtube.com/watch?v=incident-parity&list=release&t=99" "--skip-download --no-playlist --write-subs --sub-langs en https://www.youtube.com/watch?v=incident-parity&list=release&t=99") :download-removed t :messages ("sending request --list-subs --no-simulate --skip-download --no-playlist \"https://www.youtube.com/watch?v=incident-parity&list=release&t=99\"" "sending request --skip-download --no-playlist --write-subs --sub-langs \"en\" \"https://www.youtube.com/watch?v=incident-parity&list=release&t=99\"" "https://www.youtube.com/watch?t=1&list=release&v=incident-parity" "https://www.youtube.com/watch?t=1&list=release&v=incident-parity"))"####
    ]];
    ParityBatchCase::value(
        "extracts_prompted_subtitles_and_uses_timestamp_links",
        elisp_form,
        expect,
    )
}

fn honors_a_configured_language_and_renders_copyable_timestamps() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "youtube-sub-extractor-configured"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (temporary-file-directory (file-name-as-directory sandbox))
        (executable (expand-file-name "yt-dlp-parity" sandbox))
        (calls-file (expand-file-name "calls.log" sandbox))
        (youtube-sub-extractor-executable-path executable)
        (youtube-sub-extractor-language-choice "es")
        (youtube-sub-extractor-timestamps 'left-side-text)
        (youtube-sub-extractor-min-chunk-size 1)
        (video-url "https://youtu.be/configured-parity")
        buffer result)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory sandbox t)
         (neomacs-melpa-youtube-sub-extractor--write-executable
          executable
          "printf '%s\\n' \"$*\" >> calls.log\ncase \" $* \" in\n  *\" --list-subs \"*)\n    printf '%s\\n' '[info] Available subtitles for configured-parity:' 'Language Name Formats' 'en English vtt, srv3' 'es Spanish vtt, srv3'\n    ;;\n  *)\n    printf '%s\\n' 'WEBVTT' '' '00:01:02.500 --> 00:01:05.500' 'Mitigación lista' '' '00:01:05.500 --> 00:01:08.750' 'Servicio estable λ' > 'Informe de incidente [configured-parity].es.vtt'\n    printf '%s\\n' '[download] Destination: Informe de incidente [configured-parity].es.vtt' '[download] 100% of 512B'\n    ;;\nesac\n")
         (cl-letf (((symbol-function 'completing-read)
                    (lambda (&rest _)
                      (error "Configured language must not prompt"))))
           (youtube-sub-extractor-extract-subs video-url)
           (setq buffer (current-buffer))
           (setq result
                 (list
                  :buffer (buffer-name)
                  :text (buffer-substring-no-properties
                         (point-min) (point-max))
                  :line-properties
                  (save-excursion
                    (goto-char (point-min))
                    (search-forward "Mitigación")
                    (list
                     (get-text-property (line-beginning-position) 'timestamp)
                     (get-text-property (line-beginning-position) 'help-echo)))
                  :overlays
                  (neomacs-melpa-youtube-sub-extractor--overlay-state)
                  :margins (window-margins (get-buffer-window buffer))
                  :calls
                  (split-string
                   (neomacs-melpa-youtube-sub-extractor--file-string calls-file)
                   "\n" t)
                  :download-removed
                  (not
                   (file-exists-p
                    (expand-file-name
                     "Informe de incidente [configured-parity].es.vtt"
                     sandbox))))))
         result)
     (when (buffer-live-p buffer)
       (kill-buffer buffer))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:buffer "Informe de incidente [configured-parity].es" :text "Informe de incidente [configured-parity].es\n\n01:02\11Mitigación lista\n01:05\11Servicio estable λ\n" :line-properties ("00:01:02.500 --> 00:01:05.500" "01:02") :overlays ((47 69 "01:02" ((margin left-margin) "01:02")) (70 94 "01:05" ((margin left-margin) "01:05"))) :margins (nil) :calls ("--list-subs --no-simulate --skip-download --no-playlist https://youtu.be/configured-parity" "--skip-download --no-playlist --write-subs --sub-langs es https://youtu.be/configured-parity") :download-removed t)"####
    ]];
    ParityBatchCase::value(
        "honors_a_configured_language_and_renders_copyable_timestamps",
        elisp_form,
        expect,
    )
}

fn falls_back_to_auto_captions_without_visible_timestamps() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "youtube-sub-extractor-auto"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (temporary-file-directory (file-name-as-directory sandbox))
        (executable (expand-file-name "yt-dlp-parity" sandbox))
        (calls-file (expand-file-name "calls.log" sandbox))
        (youtube-sub-extractor-executable-path executable)
        (youtube-sub-extractor-language-choice t)
        (youtube-sub-extractor-timestamps nil)
        (youtube-sub-extractor-min-chunk-size 2)
        (video-url "https://youtu.be/auto-parity")
        buffer result)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory sandbox t)
         (neomacs-melpa-youtube-sub-extractor--write-executable
          executable
          "printf '%s\\n' \"$*\" >> calls.log\ncase \" $* \" in\n  *\" --list-subs \"*)\n    printf '%s\\n' '[info] Available automatic captions for auto-parity:' 'Language Name Formats' 'en English vtt, srv3'\n    ;;\n  *)\n    printf '%s\\n' 'WEBVTT' '' '00:00:00.000 --> 00:00:03.250 align:start position:0%' '<00:00:00.000><c>Auto</c> recovery' '' '00:00:03.250 --> 00:00:06.000' 'captions stable' > 'Auto recovery [auto-parity].en.vtt'\n    printf '%s\\n' '[download] Destination: Auto recovery [auto-parity].en.vtt' '[download] 100% of 768B'\n    ;;\nesac\n")
         (cl-letf (((symbol-function 'completing-read)
                    (lambda (&rest _)
                      (error "Auto-caption fallback must not prompt"))))
           (youtube-sub-extractor-extract-subs video-url)
           (setq buffer (current-buffer))
           (setq result
                 (list
                  :buffer (buffer-name)
                  :text (buffer-substring-no-properties
                         (point-min) (point-max))
                  :overlays
                  (neomacs-melpa-youtube-sub-extractor--overlay-state)
                  :margins (window-margins (get-buffer-window buffer))
                  :calls
                  (split-string
                   (neomacs-melpa-youtube-sub-extractor--file-string calls-file)
                   "\n" t)
                  :download-removed
                  (not
                   (file-exists-p
                    (expand-file-name
                     "Auto recovery [auto-parity].en.vtt"
                     sandbox))))))
         result)
     (when (buffer-live-p buffer)
       (kill-buffer buffer))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:buffer "Auto recovery [auto-parity].en" :text "Auto recovery [auto-parity].en\n\nAuto recovery\ncaptions stable\n" :overlays ((34 47 "00:00" ((margin left-margin) "00:00")) (48 63 "00:03" ((margin left-margin) "00:03"))) :margins (nil) :calls ("--list-subs --no-simulate --skip-download --no-playlist https://youtu.be/auto-parity" "--skip-download --no-playlist --write-auto-subs https://youtu.be/auto-parity") :download-removed t)"####
    ]];
    ParityBatchCase::value(
        "falls_back_to_auto_captions_without_visible_timestamps",
        elisp_form,
        expect,
    )
}

fn reports_missing_tools_and_failed_downloads_without_leaking_state() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "youtube-sub-extractor-failures"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (temporary-file-directory (file-name-as-directory sandbox))
        (executable (expand-file-name "yt-dlp-failure" sandbox))
        (calls-file (expand-file-name "calls.log" sandbox))
        (youtube-sub-extractor-language-choice "en")
        (youtube-sub-extractor-timestamps 'left-margin)
        missing failed buffers files)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory sandbox t)
         (let ((youtube-sub-extractor-executable-path "missing-yt-dlp-parity")
               (exec-path (list sandbox)))
           (setq missing
                 (condition-case error-data
                     (youtube-sub-extractor-extract-subs
                      "https://youtu.be/missing-tool")
                   (error error-data))))
         (neomacs-melpa-youtube-sub-extractor--write-executable
          executable
          "printf '%s\\n' \"$*\" >> calls.log\ncase \" $* \" in\n  *\" --list-subs \"*)\n    printf '%s\\n' '[info] Available subtitles for failed-parity:' 'Language Name Formats' 'en English vtt, srv3'\n    ;;\n  *)\n    printf '%s\\n' 'partial subtitle payload' > 'failed-download.en.vtt.part'\n    printf '%s\\n' '[youtube] ERROR: requested subtitles are unavailable' '[download] 63% of 1.00KiB'\n    ;;\nesac\n")
         (let ((youtube-sub-extractor-executable-path executable))
           (setq failed
                 (condition-case error-data
                     (youtube-sub-extractor-extract-subs
                      "https://youtu.be/failed-download")
                   (error error-data))))
         (setq buffers
               (mapcar #'buffer-name
                       (seq-filter
                        (lambda (buffer)
                          (with-current-buffer buffer
                            (bound-and-true-p
                             youtube-sub-extractor-subtitles-mode)))
                        (buffer-list))))
         (setq files
               (sort
                (directory-files sandbox nil nil t)
                #'string<))
         (list
          :missing missing
          :failed failed
          :calls
          (split-string
           (neomacs-melpa-youtube-sub-extractor--file-string calls-file)
           "\n" t)
          :subtitle-buffers buffers
          :files files))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:missing (error "ERROR: I couldn’t locate yt-dlp or youtube-dl!") :failed (error "Failed to extract subtitles, output log:\n\n[youtube] ERROR: requested subtitles are unavailable\n[download] 63% of 1.00KiB\n") :calls ("--list-subs --no-simulate --skip-download --no-playlist https://youtu.be/failed-download" "--skip-download --no-playlist --write-subs --sub-langs en https://youtu.be/failed-download") :subtitle-buffers nil :files ("." ".." "calls.log" "failed-download.en.vtt.part" "yt-dlp-failure"))"####
    ]];
    ParityBatchCase::value(
        "reports_missing_tools_and_failed_downloads_without_leaking_state",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        extracts_prompted_subtitles_and_uses_timestamp_links(),
        honors_a_configured_language_and_renders_copyable_timestamps(),
        falls_back_to_auto_captions_without_visible_timestamps(),
        reports_missing_tools_and_failed_downloads_without_leaking_state(),
    ]
}
