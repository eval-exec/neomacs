use expect_test::expect;

use super::ParityBatchCase;

fn downloads_unicode_media_updates_the_list_and_opens_the_file() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "ytdl-download"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (download-folder (expand-file-name "release archive" sandbox))
        (downloader (expand-file-name "youtube-dl-parity" sandbox))
        (player (expand-file-name "media-player-parity" sandbox))
        (calls-file (expand-file-name "downloader-calls.log" sandbox))
        (process-environment
         (cons (concat "NEOMACS_YTDL_CALLS=" calls-file)
               process-environment))
        (ytdl-command downloader)
        (ytdl-media-player player)
        (ytdl-download-types nil)
        (ytdl--download-list (make-hash-table :test 'equal))
        (ytdl--marked-items nil)
        (ytdl--last-downloaded-file-name nil)
        (ytdl--download-in-progress 0)
        (ytdl--mode-line-string "")
        (ytdl--mode-line-initialized? nil)
        (global-mode-string nil)
        (kill-ring '("https://fallback.invalid"))
        (kill-ring-yank-pointer kill-ring)
        (interprogram-paste-function nil)
        (interprogram-cut-function nil)
        (inputs
         '("https://videos.example/watch?v=release-λ&list=ops"
           "Release review λ"))
        prompts choices messages player-calls hook-events
        initial-state initial-mode-line buffer result)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory download-folder t)
         (neomacs-melpa-ytdl--write-executable
          downloader
          "printf 'CALL' >> \"$NEOMACS_YTDL_CALLS\"\nfor arg in \"$@\"; do printf ' <%s>' \"$arg\" >> \"$NEOMACS_YTDL_CALLS\"; done\nprintf '\\n' >> \"$NEOMACS_YTDL_CALLS\"\nout=''\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = '-o' ]; then out=$2; shift 2; else shift; fi\ndone\nbase=$(printf '%s' \"$out\" | sed 's/\\.%(ext)s$//')\nprintf 'release-media-λ\\n' > \"${base}.mp4\"\n")
         (neomacs-melpa-ytdl--write-executable player "exit 0\n")
         (ytdl-add-field-in-download-type-list
          "Release archive" "a" download-folder
          '("--write-info-json" "--no-mtime"))
         (let ((original-format-time-string
                (symbol-function 'format-time-string)))
           (cl-letf
               (((symbol-function 'read-from-minibuffer)
                 (lambda (prompt &optional initial &rest _)
                   (push
                    (list (substring-no-properties prompt) initial)
                    prompts)
                   (prog1 (car inputs)
                     (setq inputs (cdr inputs)))))
                ((symbol-function 'read-char-choice)
                 (lambda (prompt allowed &rest _)
                   (push
                    (list (substring-no-properties prompt) allowed)
                    choices)
                   ?a))
                ((symbol-function 'format-time-string)
                 (lambda (format-string &rest arguments)
                   (if (equal format-string "%Y-%m-%d-%T")
                       "2026-08-03-14:15:16"
                     (apply original-format-time-string
                            format-string arguments))))
                ((symbol-function 'start-process-shell-command)
                 (lambda (name process-buffer command)
                   (push (list name process-buffer command) player-calls)
                   'ytdl-media-player-process))
                ((symbol-function 'message)
                 (lambda (format-string &rest arguments)
                   (let ((text (apply #'format format-string arguments)))
                     (push text messages)
                     text))))
             (let ((ytdl-download-finished-hook
                    (list
                     (lambda ()
                       (push
                        (list 'normal ytdl--last-downloaded-file-name)
                        hook-events))))
                   (ytdl-download-finished-functions
                    (list
                     (lambda (filename uuid)
                        (push (list 'abnormal filename uuid) hook-events)))))
               (ytdl-download)
               (setq initial-state
                     (neomacs-melpa-ytdl--download-state)
                     initial-mode-line
                     (list ytdl--download-in-progress
                           ytdl--mode-line-string
                           (copy-tree global-mode-string)))
               (let* ((key (caar initial-state))
                      (item (gethash key ytdl--download-list))
                      (process (ytdl--list-entry-process-id item))
                      (attempts 0))
                 (while (and (equal (ytdl--list-entry-status item)
                                    "downloading")
                             (process-live-p process)
                             (< attempts 200))
                   (accept-process-output process 0.05)
                   (setq attempts (1+ attempts)))
                 (when (equal (ytdl--list-entry-status item)
                              "downloading")
                   (error "Timed out waiting for the real async download")))
               (setq buffer (get-buffer ytdl-dl-buffer-name))
               (with-current-buffer buffer
                 (goto-char (point-min))
                 (while (and (not (tabulated-list-get-id))
                             (not (eobp)))
                   (forward-line 1))
                 (call-interactively (key-binding (kbd "y")))
                 (call-interactively (key-binding (kbd "o"))))
               (ytdl-open-last-downloaded-file)
               (setq result
                     (list
                      :prompts (nreverse prompts)
                      :choices (nreverse choices)
                      :initial initial-state
                      :initial-mode-line initial-mode-line
                      :downloads (neomacs-melpa-ytdl--download-state)
                      :list
                      (with-current-buffer buffer
                        (list
                         (buffer-substring-no-properties
                          (point-min) (point-max))
                         (mapcar #'neomacs-melpa-ytdl--entry-state
                                 tabulated-list-entries)
                         major-mode buffer-read-only
                         revert-buffer-function
                         (lookup-key ytdl--dl-list-mode-map "y")
                         (lookup-key ytdl--dl-list-mode-map "o")))
                      :calls
                      (neomacs-melpa-ytdl--file-lines calls-file)
                      :file
                      (with-temp-buffer
                        (insert-file-contents
                         ytdl--last-downloaded-file-name)
                        (list ytdl--last-downloaded-file-name
                              (buffer-string)))
                      :hooks (nreverse hook-events)
                      :kill (car kill-ring)
                      :players (nreverse player-calls)
                      :mode-line
                      (list ytdl--download-in-progress
                            ytdl--mode-line-string
                            (copy-tree global-mode-string))
                      :messages (nreverse messages))))))
             result)
     (maphash
      (lambda (_ item)
        (let ((process (ytdl--list-entry-process-id item)))
          (when (and (processp process) (process-live-p process))
            (delete-process process))))
      ytdl--download-list)
     (when (buffer-live-p buffer)
       (kill-buffer buffer))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:prompts (("[ytdl] URL: " "https://fallback.invalid") ("[ytdl] Filename [no extension]: " nil)) :choices (("Destination folder: Release archive[a]" (97))) :initial (("https://videos.example/watch?v=release-λ&list=ops2026-08-03-14:15:16" "Release review λ" "downloading" "Release archive" nil "?" nil "https://videos.example/watch?v=release-λ&list=ops")) :initial-mode-line (1 "[ytdl 1]" ("" ytdl--mode-line-string)) :downloads (("https://videos.example/watch?v=release-λ&list=ops2026-08-03-14:15:16" "Release review λ" "downloaded" "Release archive" "[ORACLE-SANDBOX]/ytdl-download/release archive/Release review λ.mp4" "17" nil "https://videos.example/watch?v=release-λ&list=ops")) :list ("  Release review λ                    downloaded      17         Release archive\n" (("https://videos.example/watch?v=release-λ&list=ops2026-08-03-14:15:16" ("Release review λ" "downloaded" "17" "Release archive"))) ytdl--dl-list-mode t tabulated-list-revert ytdl--copy-item-path ytdl--open-item-at-point) :calls ("CALL <-o> <[ORACLE-SANDBOX]/ytdl-download/release archive/Release review λ.%(ext)s> <--write-info-json> <--no-mtime> <--> <https://videos.example/watch?v=release-λ&list=ops>") :file ("[ORACLE-SANDBOX]/ytdl-download/release archive/Release review λ.mp4" "release-media-λ\n") :hooks ((normal "[ORACLE-SANDBOX]/ytdl-download/release archive/Release review λ.mp4") (abnormal "[ORACLE-SANDBOX]/ytdl-download/release archive/Release review λ.mp4" "https://videos.example/watch?v=release-λ&list=ops2026-08-03-14:15:16")) :kill "[ORACLE-SANDBOX]/ytdl-download/release archive/Release review λ.mp4" :players (("[ORACLE-SANDBOX]/ytdl-download/media-player-parity" nil "[ORACLE-SANDBOX]/ytdl-download/media-player-parity [ORACLE-SANDBOX]/ytdl-download/release\\ archive/Release\\ review\\ \\λ.mp4") ("[ORACLE-SANDBOX]/ytdl-download/media-player-parity" nil "[ORACLE-SANDBOX]/ytdl-download/media-player-parity [ORACLE-SANDBOX]/ytdl-download/release\\ archive/Release\\ review\\ \\λ.mp4")) :mode-line (0 "" ("" ytdl--mode-line-string)) :messages ("[ytdl] Video downloaded: [ORACLE-SANDBOX]/ytdl-download/release archive/Release review λ.mp4" "[ytdl] File path is: [ORACLE-SANDBOX]/ytdl-download/release archive/Release review λ.mp4. Added to kill-ring." "[ytdl] Opening file"))"####
    ]];
    ParityBatchCase::value(
        "downloads_unicode_media_updates_the_list_and_opens_the_file",
        elisp_form,
        expect,
    )
}

fn fans_out_a_playlist_then_marks_opens_and_cleans_entries() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "ytdl-playlist"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (download-folder (expand-file-name "podcast queue" sandbox))
        (downloader (expand-file-name "youtube-dl-parity" sandbox))
        (player (expand-file-name "player-parity" sandbox))
        (calls-file (expand-file-name "calls.log" sandbox))
        (process-environment
         (cons (concat "NEOMACS_YTDL_CALLS=" calls-file)
               process-environment))
        (ytdl-command downloader)
        (ytdl-media-player player)
        (ytdl-download-types nil)
        (ytdl--download-list (make-hash-table :test 'equal))
        (ytdl--marked-items nil)
        (ytdl--last-downloaded-file-name nil)
        (ytdl--download-in-progress 0)
        (ytdl--mode-line-string "")
        (ytdl--mode-line-initialized? nil)
        (global-mode-string nil)
        (kill-ring '("https://fallback.invalid/playlist"))
        (kill-ring-yank-pointer kill-ring)
        (interprogram-paste-function nil)
        (interprogram-cut-function nil)
        (inputs
         '("https://videos.example/playlist?list=release-λ"
           "Incident"))
        async-jobs prompts confirmations messages player-calls hook-events
        initial-state initial-mode-line completed-state marked filtered-text all-marked
        before-cleanup after-delete files buffer result)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory download-folder t)
         (neomacs-melpa-ytdl--write-executable
          downloader
          "printf 'CALL' >> \"$NEOMACS_YTDL_CALLS\"\nfor arg in \"$@\"; do printf ' <%s>' \"$arg\" >> \"$NEOMACS_YTDL_CALLS\"; done\nprintf '\\n' >> \"$NEOMACS_YTDL_CALLS\"\ncase \" $* \" in\n  *' --dump-json '*)\n    printf '%s\\n' '{\"id\":\"video-a\",\"title\":\"Incident/Review.v2 λ\"}' '{\"id\":\"video-b\",\"title\":\"Audio.Deep/Dive\"}'\n    ;;\n  *)\n    out=''\n    while [ \"$#\" -gt 0 ]; do\n      if [ \"$1\" = '-o' ]; then out=$2; shift 2; else shift; fi\n    done\n    base=$(printf '%s' \"$out\" | sed 's/\\.%(ext)s$//')\n    printf 'playlist-media\\n' > \"${base}.mp4\"\n    ;;\nesac\n")
         (neomacs-melpa-ytdl--write-executable player "exit 0\n")
         (ytdl-add-field-in-download-type-list
          "Podcasts" "p" download-folder
          '("--extract-audio" "--audio-format" "opus"))
         (let ((original-format-time-string
                (symbol-function 'format-time-string)))
           (cl-letf
               (((symbol-function 'async-start)
                 (lambda (worker callback)
                   (let ((process-id
                          (intern
                           (format "ytdl-playlist-process-%d"
                                   (1+ (length async-jobs))))))
                     (push (cons worker callback) async-jobs)
                     process-id)))
                ((symbol-function 'read-from-minibuffer)
                 (lambda (prompt &optional initial &rest _)
                   (push (list (substring-no-properties prompt) initial)
                         prompts)
                   (prog1 (car inputs)
                     (setq inputs (cdr inputs)))))
                ((symbol-function 'read-char-choice)
                 (lambda (&rest _) ?p))
                ((symbol-function 'pop-to-buffer)
                 (lambda (buffer &rest _) buffer))
                ((symbol-function 'y-or-n-p)
                 (lambda (prompt)
                   (push prompt confirmations)
                   t))
                ((symbol-function 'format-time-string)
                 (lambda (format-string &rest arguments)
                   (if (equal format-string "%Y-%m-%d-%T")
                       "2026-08-03-15:16:17"
                     (apply original-format-time-string
                            format-string arguments))))
                ((symbol-function 'start-process-shell-command)
                 (lambda (name process-buffer command)
                   (push (list name process-buffer command) player-calls)
                   'ytdl-playlist-player-process))
                ((symbol-function 'message)
                 (lambda (format-string &rest arguments)
                   (let ((text (apply #'format format-string arguments)))
                     (push text messages)
                     text))))
             (let ((ytdl-download-finished-hook
                    (list
                     (lambda ()
                       (push
                        (list 'normal ytdl--last-downloaded-file-name)
                        hook-events))))
                   (ytdl-download-finished-functions
                    (list
                     (lambda (filename uuid)
                       (push (list 'abnormal filename uuid) hook-events)))))
               (ytdl-download-playlist)
               (setq initial-state
                     (neomacs-melpa-ytdl--download-state)
                     initial-mode-line
                     (list ytdl--download-in-progress
                           ytdl--mode-line-string))
               ;; Jobs are pushed, so this deliberately completes video-b first.
               (dolist (job async-jobs)
                 (funcall (cdr job) (funcall (car job))))
               (setq completed-state
                     (neomacs-melpa-ytdl--download-state))
               (setq buffer (get-buffer ytdl-dl-buffer-name))
               (with-current-buffer buffer
                 (setq tabulated-list-sort-key '("Title" . nil))
                 (call-interactively (key-binding (kbd "g")))
                 (call-interactively (key-binding (kbd "^")))
                 (setq marked
                       (sort (copy-sequence ytdl--marked-items) #'string<)
                       filtered-text
                       (buffer-substring-no-properties
                        (point-min) (point-max)))
                 (call-interactively (key-binding (kbd "O")))
                 (call-interactively (key-binding (kbd "M")))
                 (setq all-marked
                       (sort (copy-sequence ytdl--marked-items) #'string<))
                 (call-interactively (key-binding (kbd "U")))
                 (setq before-cleanup
                       (list
                        (buffer-substring-no-properties
                         (point-min) (point-max))
                        (mapcar #'neomacs-melpa-ytdl--entry-state
                                tabulated-list-entries)))
                 (neomacs-melpa-ytdl--goto-id
                  "video-b2026-08-03-15:16:17")
                 (call-interactively (key-binding (kbd "K")))
                 (setq after-delete
                       (neomacs-melpa-ytdl--download-state))
                 (call-interactively (key-binding (kbd "c"))))
               (setq files
                     (list
                      (file-exists-p
                       (expand-file-name
                        "Incident-Review-v2 λ.mp4" download-folder))
                      (file-exists-p
                       (expand-file-name
                        "Audio-Deep-Dive.mp4" download-folder))))
               (setq result
                     (list
                      :prompts (nreverse prompts)
                      :initial initial-state
                      :initial-mode-line initial-mode-line
                      :completed completed-state
                      :marked marked
                      :filtered filtered-text
                      :all-marked all-marked
                      :before-cleanup before-cleanup
                      :after-delete after-delete
                      :after-clear (neomacs-melpa-ytdl--download-state)
                      :files files
                      :last ytdl--last-downloaded-file-name
                      :calls
                      (neomacs-melpa-ytdl--file-lines calls-file)
                      :hooks (nreverse hook-events)
                      :players (nreverse player-calls)
                      :confirmations (nreverse confirmations)
                      :mode-line
                      (list ytdl--download-in-progress
                            ytdl--mode-line-string)
                      :messages (nreverse messages))))))
             result)
     (when (buffer-live-p buffer)
       (kill-buffer buffer))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:prompts (("[ytdl] URL: " "https://fallback.invalid/playlist") ("[ytdl] Regexp to match (titles and download types will be matched): " nil)) :initial (("video-a2026-08-03-15:16:17" "Incident-Review-v2 λ" "downloading" "Podcasts" nil "?" nil "video-a") ("video-b2026-08-03-15:16:17" "Audio-Deep-Dive" "downloading" "Podcasts" nil "?" nil "video-b")) :initial-mode-line (2 "[ytdl 2]") :completed (("video-a2026-08-03-15:16:17" "Incident-Review-v2 λ" "downloaded" "Podcasts" "[ORACLE-SANDBOX]/ytdl-playlist/podcast queue/Incident-Review-v2 λ.mp4" "15" nil "video-a") ("video-b2026-08-03-15:16:17" "Audio-Deep-Dive" "downloaded" "Podcasts" "[ORACLE-SANDBOX]/ytdl-playlist/podcast queue/Audio-Deep-Dive.mp4" "15" nil "video-b")) :marked ("video-a2026-08-03-15:16:17") :filtered "  Audio-Deep-Dive                     downloaded      15         Podcasts\n* Incident-Review-v2 λ                downloaded      15         Podcasts\n" :all-marked ("video-a2026-08-03-15:16:17" "video-b2026-08-03-15:16:17") :before-cleanup ("  Audio-Deep-Dive                     downloaded      15         Podcasts\n  Incident-Review-v2 λ                downloaded      15         Podcasts\n" (("video-b2026-08-03-15:16:17" ("Audio-Deep-Dive" "downloaded" "15" "Podcasts")) ("video-a2026-08-03-15:16:17" ("Incident-Review-v2 λ" "downloaded" "15" "Podcasts")))) :after-delete (("video-a2026-08-03-15:16:17" "Incident-Review-v2 λ" "downloaded" "Podcasts" "[ORACLE-SANDBOX]/ytdl-playlist/podcast queue/Incident-Review-v2 λ.mp4" "15" nil "video-a")) :after-clear nil :files (t nil) :last "[ORACLE-SANDBOX]/ytdl-playlist/podcast queue/Incident-Review-v2 λ.mp4" :calls ("CALL <--dump-json> <--flat-playlist> <--no-warnings> <https://videos.example/playlist?list=release-λ>" "CALL <-o> <[ORACLE-SANDBOX]/ytdl-playlist/podcast queue/Audio-Deep-Dive.%(ext)s> <--extract-audio> <--audio-format> <opus> <--> <video-b>" "CALL <-o> <[ORACLE-SANDBOX]/ytdl-playlist/podcast queue/Incident-Review-v2 λ.%(ext)s> <--extract-audio> <--audio-format> <opus> <--> <video-a>") :hooks ((normal "[ORACLE-SANDBOX]/ytdl-playlist/podcast queue/Audio-Deep-Dive.mp4") (abnormal "[ORACLE-SANDBOX]/ytdl-playlist/podcast queue/Audio-Deep-Dive.mp4" "video-b2026-08-03-15:16:17") (normal "[ORACLE-SANDBOX]/ytdl-playlist/podcast queue/Incident-Review-v2 λ.mp4") (abnormal "[ORACLE-SANDBOX]/ytdl-playlist/podcast queue/Incident-Review-v2 λ.mp4" "video-a2026-08-03-15:16:17")) :players (("[ORACLE-SANDBOX]/ytdl-playlist/player-parity" nil "[ORACLE-SANDBOX]/ytdl-playlist/player-parity [ORACLE-SANDBOX]/ytdl-playlist/podcast\\ queue/Incident-Review-v2\\ \\λ.mp4")) :confirmations ("[ytdl] Delete this item? The associated file will be deleted." "[ytdl] Clear the list of downloaded items?") :mode-line (0 "") :messages ("[ytdl] Video downloaded: [ORACLE-SANDBOX]/ytdl-playlist/podcast queue/Audio-Deep-Dive.mp4" "[ytdl] Video downloaded: [ORACLE-SANDBOX]/ytdl-playlist/podcast queue/Incident-Review-v2 λ.mp4" "[ytdl] Opening files"))"####
    ]];
    ParityBatchCase::value(
        "fans_out_a_playlist_then_marks_opens_and_cleans_entries",
        elisp_form,
        expect,
    )
}

fn selects_a_real_format_and_downloads_with_validated_custom_arguments() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "ytdl-format"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (download-folder (expand-file-name "review clips" sandbox))
        (downloader (expand-file-name "youtube-dl-parity" sandbox))
        (calls-file (expand-file-name "calls.log" sandbox))
        (process-environment
         (cons (concat "NEOMACS_YTDL_CALLS=" calls-file)
               process-environment))
        (ytdl-command downloader)
        (ytdl-format-entry-format "%i %q %e %r %b")
        (ytdl-download-types nil)
        (ytdl--download-list (make-hash-table :test 'equal))
        (ytdl--marked-items nil)
        (ytdl--last-downloaded-file-name nil)
        (ytdl--download-in-progress 0)
        (ytdl--mode-line-string "")
        (ytdl--mode-line-initialized? nil)
        (global-mode-string nil)
        (neomacs-melpa-ytdl--selected-format nil)
        (kill-ring '("https://fallback.invalid/format"))
        (kill-ring-yank-pointer kill-ring)
        (interprogram-paste-function nil)
        (inputs
         '("https://videos.example/watch?v=format-λ"
           "invalid/name"
           "Final cut λ"))
        async-jobs prompts completion-candidates confirmations
        minibuffer-messages selected-format initial-state buffer result)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory sandbox t)
         (neomacs-melpa-ytdl--write-executable
          downloader
          "printf 'CALL' >> \"$NEOMACS_YTDL_CALLS\"\nfor arg in \"$@\"; do printf ' <%s>' \"$arg\" >> \"$NEOMACS_YTDL_CALLS\"; done\nprintf '\\n' >> \"$NEOMACS_YTDL_CALLS\"\ncase \" $* \" in\n  *' --list-formats '*)\n    printf '%s\\n' 'format code  extension  resolution note' '249 webm audio only tiny 52k' '18 mp4 640x360 30fps 360p 500k' '18 mp4 640x360 30fps 360p 500k' '22 mp4 1280x720 30fps 720p 1500k'\n    ;;\n  *)\n    out=''\n    while [ \"$#\" -gt 0 ]; do\n      if [ \"$1\" = '-o' ]; then out=$2; shift 2; else shift; fi\n    done\n    base=$(printf '%s' \"$out\" | sed 's/\\.%(ext)s$//')\n    printf 'selected-format-media\\n' > \"${base}.mp4\"\n    ;;\nesac\n")
         (cl-letf
             (((symbol-function 'completing-read)
               (lambda (prompt collection &rest _)
                 (setq completion-candidates
                       (list prompt (all-completions "" collection)))
                 "18 360p mp4 640x360 30fps 500k")))
           (setq selected-format
                 (ytdl-select-format
                  "https://videos.example/watch?v=format-λ")
                 neomacs-melpa-ytdl--selected-format selected-format))
         (ytdl-add-field-in-download-type-list
          "Review clips" "r" download-folder
          (list "-f" 'neomacs-melpa-ytdl--selected-format
                "--write-thumbnail"))
         (let ((original-format-time-string
                (symbol-function 'format-time-string)))
           (cl-letf
               (((symbol-function 'async-start)
                 (lambda (worker callback)
                   (push (cons worker callback) async-jobs)
                   'ytdl-format-process))
                ((symbol-function 'read-from-minibuffer)
                 (lambda (prompt &optional initial &rest _)
                   (push (list (substring-no-properties prompt) initial)
                         prompts)
                   (prog1 (car inputs)
                     (setq inputs (cdr inputs)))))
                ((symbol-function 'read-char-choice)
                 (lambda (&rest _) ?r))
                ((symbol-function 'minibuffer-message)
                 (lambda (format-string &rest arguments)
                   (let ((text (apply #'format format-string arguments)))
                     (push text minibuffer-messages)
                     text)))
                ((symbol-function 'y-or-n-p)
                 (lambda (prompt)
                   (push prompt confirmations)
                   t))
                ((symbol-function 'format-time-string)
                 (lambda (format-string &rest arguments)
                   (if (equal format-string "%Y-%m-%d-%T")
                       "2026-08-03-16:17:18"
                     (apply original-format-time-string
                            format-string arguments)))))
             (ytdl-download)
             (setq initial-state
                   (neomacs-melpa-ytdl--download-state))
             (neomacs-melpa-ytdl--run-async-jobs async-jobs)
             (setq buffer (get-buffer ytdl-dl-buffer-name))
             (setq result
                   (list
                    :selected selected-format
                    :completion completion-candidates
                    :prompts (nreverse prompts)
                    :minibuffer (nreverse minibuffer-messages)
                    :confirmations (nreverse confirmations)
                    :initial initial-state
                    :downloads (neomacs-melpa-ytdl--download-state)
                    :directory (file-directory-p download-folder)
                    :file
                    (with-temp-buffer
                      (insert-file-contents
                       ytdl--last-downloaded-file-name)
                      (list ytdl--last-downloaded-file-name
                            (buffer-string)))
                    :calls
                    (neomacs-melpa-ytdl--file-lines calls-file)
                    :mode-line
                    (list ytdl--download-in-progress
                          ytdl--mode-line-string))))
             result))
     (when (buffer-live-p buffer)
       (kill-buffer buffer))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:selected "best[height<=360]" :completion ("Select format: " ("22 720p mp4 1280x720 30fps 1500k" "18 360p mp4 640x360 30fps 500k" "249 tiny webm audio only 52k")) :prompts (("[ytdl] URL: " "https://fallback.invalid/format") ("[ytdl] Filename [no extension]: " nil) ("[ytdl] Filename [no extension]: " nil)) :minibuffer ("[ytdl] Filename cannot contain '/'!") :confirmations ("Directory '[ORACLE-SANDBOX]/ytdl-format/review clips' does not exist. Create it?") :initial (("https://videos.example/watch?v=format-λ2026-08-03-16:17:18" "Final cut λ" "downloading" "Review clips" nil "?" nil "https://videos.example/watch?v=format-λ")) :downloads (("https://videos.example/watch?v=format-λ2026-08-03-16:17:18" "Final cut λ" "downloaded" "Review clips" "[ORACLE-SANDBOX]/ytdl-format/review clips/Final cut λ.mp4" "22" nil "https://videos.example/watch?v=format-λ")) :directory t :file ("[ORACLE-SANDBOX]/ytdl-format/review clips/Final cut λ.mp4" "selected-format-media\n") :calls ("CALL <--list-formats> <https://videos.example/watch?v=format-λ>" "CALL <-o> <[ORACLE-SANDBOX]/ytdl-format/review clips/Final cut λ.%(ext)s> <-f> <best[height<=360]> <--write-thumbnail> <--> <https://videos.example/watch?v=format-λ>") :mode-line (0 ""))"####
    ]];
    ParityBatchCase::value(
        "selects_a_real_format_and_downloads_with_validated_custom_arguments",
        elisp_form,
        expect,
    )
}

fn surfaces_download_errors_relaunches_and_preserves_explicit_deletion_semantics() -> ParityBatchCase
{
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "ytdl-relaunch"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (download-folder (expand-file-name "recovery queue" sandbox))
        (downloader (expand-file-name "youtube-dl-parity" sandbox))
        (calls-file (expand-file-name "calls.log" sandbox))
        (attempt-file (expand-file-name "attempt" sandbox))
        (process-environment
         (append
          (list (concat "NEOMACS_YTDL_CALLS=" calls-file)
                (concat "NEOMACS_YTDL_ATTEMPT=" attempt-file))
          process-environment))
        (ytdl-command downloader)
        (ytdl-download-types nil)
        (ytdl--download-list (make-hash-table :test 'equal))
        (ytdl--marked-items nil)
        (ytdl--last-downloaded-file-name nil)
        (ytdl--download-in-progress 0)
        (ytdl--mode-line-string "")
        (ytdl--mode-line-initialized? nil)
        (global-mode-string nil)
        (kill-ring '("https://fallback.invalid/recovery"))
        (kill-ring-yank-pointer kill-ring)
        (interprogram-paste-function nil)
        (inputs
         '("https://videos.example/watch?v=recovery-λ"
           "Recovered release λ"))
        (uuid-times '("2026-08-03-17:18:19"
                      "2026-08-03-17:18:20"))
        async-jobs prompts confirmations messages hook-events
        error-state error-mode relaunch-state recovered-state file-path
        missing-downloader no-player missing-player after-delete buffer result)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory download-folder t)
         (neomacs-melpa-ytdl--write-executable
          downloader
          "printf 'CALL' >> \"$NEOMACS_YTDL_CALLS\"\nfor arg in \"$@\"; do printf ' <%s>' \"$arg\" >> \"$NEOMACS_YTDL_CALLS\"; done\nprintf '\\n' >> \"$NEOMACS_YTDL_CALLS\"\nout=''\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = '-o' ]; then out=$2; shift 2; else shift; fi\ndone\nif [ ! -e \"$NEOMACS_YTDL_ATTEMPT\" ]; then\n  : > \"$NEOMACS_YTDL_ATTEMPT\"\n  printf '%s\\n' 'ERROR: transient upstream rejection λ'\nelse\n  base=$(printf '%s' \"$out\" | sed 's/\\.%(ext)s$//')\n  printf 'recovered-media\\n' > \"${base}.mp4\"\nfi\n")
         (ytdl-add-field-in-download-type-list
          "Recovery" "r" download-folder '("--retries" "2"))
         (let ((original-format-time-string
                (symbol-function 'format-time-string)))
           (cl-letf
               (((symbol-function 'async-start)
                 (lambda (worker callback)
                   (push (cons worker callback) async-jobs)
                   (intern
                    (format "ytdl-recovery-process-%d"
                            (length async-jobs)))))
                ((symbol-function 'read-from-minibuffer)
                 (lambda (prompt &optional initial &rest _)
                   (push (list (substring-no-properties prompt) initial)
                         prompts)
                   (prog1 (car inputs)
                     (setq inputs (cdr inputs)))))
                ((symbol-function 'read-char-choice)
                 (lambda (&rest _) ?r))
                ((symbol-function 'y-or-n-p)
                 (lambda (prompt)
                   (push prompt confirmations)
                   t))
                ((symbol-function 'format-time-string)
                 (lambda (format-string &rest arguments)
                   (if (equal format-string "%Y-%m-%d-%T")
                       (prog1 (car uuid-times)
                         (setq uuid-times (cdr uuid-times)))
                     (apply original-format-time-string
                            format-string arguments))))
                ((symbol-function 'message)
                 (lambda (format-string &rest arguments)
                   (let ((text (apply #'format format-string arguments)))
                     (push text messages)
                     text))))
             (let ((ytdl-download-finished-hook
                    (list
                     (lambda ()
                       (push
                        (list 'normal ytdl--last-downloaded-file-name)
                        hook-events))))
                   (ytdl-download-finished-functions
                    (list
                     (lambda (filename uuid)
                       (push (list 'abnormal filename uuid) hook-events)))))
               (ytdl-download)
               (neomacs-melpa-ytdl--run-async-jobs async-jobs)
               (setq async-jobs nil
                     error-state (neomacs-melpa-ytdl--download-state)
                     error-mode
                     (list ytdl--download-in-progress
                           ytdl--mode-line-string)
                     buffer (get-buffer ytdl-dl-buffer-name))
               (with-current-buffer buffer
                 (neomacs-melpa-ytdl--goto-id
                  "https://videos.example/watch?v=recovery-λ2026-08-03-17:18:19")
                 (call-interactively (key-binding (kbd "e")))
                 (call-interactively (key-binding (kbd "r"))))
               (setq relaunch-state
                     (neomacs-melpa-ytdl--download-state))
               (neomacs-melpa-ytdl--run-async-jobs async-jobs)
               (setq recovered-state
                     (neomacs-melpa-ytdl--download-state)
                     file-path ytdl--last-downloaded-file-name)
               (with-current-buffer buffer
                 (neomacs-melpa-ytdl--goto-id
                  "https://videos.example/watch?v=recovery-λ2026-08-03-17:18:20")
                 (call-interactively (key-binding (kbd "e")))
                 (call-interactively (key-binding (kbd "k"))))
               (setq after-delete
                     (list
                      (neomacs-melpa-ytdl--download-state)
                      (file-exists-p file-path)
                      (with-temp-buffer
                        (insert-file-contents file-path)
                        (buffer-string))))
               (setq missing-downloader
                     (let ((ytdl-command
                            (expand-file-name "missing-downloader" sandbox)))
                       (neomacs-melpa-ytdl--capture-error
                        #'ytdl-download))
                     no-player
                     (let ((ytdl-media-player nil))
                       (neomacs-melpa-ytdl--capture-error
                        #'ytdl-open-last-downloaded-file))
                     missing-player
                     (let ((ytdl-media-player
                            "missing-player-parity"))
                       (neomacs-melpa-ytdl--capture-error
                        #'ytdl-open-last-downloaded-file)))
               (setq result
                     (list
                      :prompts (nreverse prompts)
                      :error error-state
                      :error-mode error-mode
                      :relaunch relaunch-state
                      :recovered recovered-state
                      :after-delete after-delete
                      :missing-downloader missing-downloader
                      :no-player no-player
                      :missing-player missing-player
                      :calls
                      (neomacs-melpa-ytdl--file-lines calls-file)
                      :hooks (nreverse hook-events)
                      :confirmations (nreverse confirmations)
                      :mode-line
                      (list ytdl--download-in-progress
                            ytdl--mode-line-string)
                      :messages (nreverse messages))))))
             result)
     (when (buffer-live-p buffer)
       (kill-buffer buffer))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:prompts (("[ytdl] URL: " "https://fallback.invalid/recovery") ("[ytdl] Filename [no extension]: " nil)) :error (("https://videos.example/watch?v=recovery-λ2026-08-03-17:18:19" "Recovered release λ" "error" "Recovery" nil "?" "ERROR: transient upstream rejection λ" "https://videos.example/watch?v=recovery-λ")) :error-mode (0 "") :relaunch (("https://videos.example/watch?v=recovery-λ2026-08-03-17:18:20" "Recovered release λ" "downloading" "Recovery" nil "?" nil "https://videos.example/watch?v=recovery-λ")) :recovered (("https://videos.example/watch?v=recovery-λ2026-08-03-17:18:20" "Recovered release λ" "downloaded" "Recovery" "[ORACLE-SANDBOX]/ytdl-relaunch/recovery queue/Recovered release λ.mp4" "16" nil "https://videos.example/watch?v=recovery-λ")) :after-delete (nil t "recovered-media\n") :missing-downloader (error (error "youtube-dl is not installed.")) :no-player (error (error "No media player is set up. See ‘ytdl-media-player’.")) :missing-player (error (error "Program \"missing-player-parity\" cannot be found.")) :calls ("CALL <-o> <[ORACLE-SANDBOX]/ytdl-relaunch/recovery queue/Recovered release λ.%(ext)s> <--retries> <2> <--> <https://videos.example/watch?v=recovery-λ>" "CALL <-o> <[ORACLE-SANDBOX]/ytdl-relaunch/recovery queue/Recovered release λ.%(ext)s> <--retries> <2> <--> <https://videos.example/watch?v=recovery-λ>") :hooks ((normal "[ORACLE-SANDBOX]/ytdl-relaunch/recovery queue/Recovered release λ.mp4") (abnormal "[ORACLE-SANDBOX]/ytdl-relaunch/recovery queue/Recovered release λ.mp4" "https://videos.example/watch?v=recovery-λ2026-08-03-17:18:20")) :confirmations ("[ytdl] Delete this item?") :mode-line (0 "") :messages ("[ytdl] ERROR: transient upstream rejection λ" "[ytdl] ERROR: transient upstream rejection λ" "[ytdl] Video downloaded: [ORACLE-SANDBOX]/ytdl-relaunch/recovery queue/Recovered release λ.mp4" "[ytdl] Video is downloaded."))"####
    ]];
    ParityBatchCase::value(
        "surfaces_download_errors_relaunches_and_preserves_explicit_deletion_semantics",
        elisp_form,
        expect,
    )
}

fn relaunches_every_failed_download_from_the_list() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "ytdl-relaunch-all"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (download-folder (expand-file-name "retry queue" sandbox))
        (downloader (expand-file-name "youtube-dl-parity" sandbox))
        (calls-file (expand-file-name "calls.log" sandbox))
        (recovery-file (expand-file-name "upstream-recovered" sandbox))
        (process-environment
         (append
          (list (concat "NEOMACS_YTDL_CALLS=" calls-file)
                (concat "NEOMACS_YTDL_RECOVER=" recovery-file))
          process-environment))
        (ytdl-command downloader)
        (ytdl-download-types nil)
        (ytdl--download-list (make-hash-table :test 'equal))
        (ytdl--marked-items nil)
        (ytdl--last-downloaded-file-name nil)
        (ytdl--download-in-progress 0)
        (ytdl--mode-line-string "")
        (ytdl--mode-line-initialized? nil)
        (global-mode-string nil)
        (kill-ring '("https://fallback.invalid/retry-all"))
        (kill-ring-yank-pointer kill-ring)
        (interprogram-paste-function nil)
        (inputs
         '("https://videos.example/watch?v=retry-a" "Retry alpha λ"
           "https://videos.example/watch?v=retry-b" "Retry beta β"))
        (uuid-times
         '("2026-08-03-19:00:01" "2026-08-03-19:00:02"
           "2026-08-03-19:00:03" "2026-08-03-19:00:04"))
        async-jobs prompts messages failed queued recovered buffer result)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory download-folder t)
         (neomacs-melpa-ytdl--write-executable
          downloader
          "printf 'CALL' >> \"$NEOMACS_YTDL_CALLS\"\nfor arg in \"$@\"; do printf ' <%s>' \"$arg\" >> \"$NEOMACS_YTDL_CALLS\"; done\nprintf '\\n' >> \"$NEOMACS_YTDL_CALLS\"\nout=''\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = '-o' ]; then out=$2; shift 2; else shift; fi\ndone\nif [ ! -e \"$NEOMACS_YTDL_RECOVER\" ]; then\n  printf '%s\\n' 'ERROR: upstream maintenance λ'\nelse\n  base=$(printf '%s' \"$out\" | sed 's/\\.%(ext)s$//')\n  printf 'retried-media\\n' > \"${base}.mp4\"\nfi\n")
         (ytdl-add-field-in-download-type-list
          "Retry queue" "r" download-folder '("--retries" "3"))
         (let ((original-format-time-string
                (symbol-function 'format-time-string)))
           (cl-letf
               (((symbol-function 'async-start)
                 (lambda (worker callback)
                   (let ((process-id
                          (intern
                           (format "ytdl-relaunch-all-process-%d"
                                   (1+ (length async-jobs))))))
                     (push (cons worker callback) async-jobs)
                     process-id)))
                ((symbol-function 'read-from-minibuffer)
                 (lambda (prompt &optional initial &rest _)
                   (push (list (substring-no-properties prompt) initial)
                         prompts)
                   (prog1 (car inputs)
                     (setq inputs (cdr inputs)))))
                ((symbol-function 'read-char-choice)
                 (lambda (&rest _) ?r))
                ((symbol-function 'pop-to-buffer)
                 (lambda (buffer &rest _) buffer))
                ((symbol-function 'format-time-string)
                 (lambda (format-string &rest arguments)
                   (if (equal format-string "%Y-%m-%d-%T")
                       (prog1 (car uuid-times)
                         (setq uuid-times (cdr uuid-times)))
                     (apply original-format-time-string
                            format-string arguments))))
                ((symbol-function 'message)
                 (lambda (format-string &rest arguments)
                   (let ((text (apply #'format format-string arguments)))
                     (push text messages)
                     text))))
             (ytdl-download)
             (ytdl-download)
             (neomacs-melpa-ytdl--run-async-jobs async-jobs)
             (setq async-jobs nil
                   failed
                   (list
                    (neomacs-melpa-ytdl--download-state)
                    (list ytdl--download-in-progress
                          ytdl--mode-line-string))
                   buffer (get-buffer ytdl-dl-buffer-name))
             (with-temp-file recovery-file)
             (with-current-buffer buffer
               (call-interactively (key-binding (kbd "R"))))
             (setq queued
                   (list
                    (neomacs-melpa-ytdl--download-state)
                    (list ytdl--download-in-progress
                          ytdl--mode-line-string)))
             (neomacs-melpa-ytdl--run-async-jobs async-jobs)
             (setq recovered
                   (list
                    (neomacs-melpa-ytdl--download-state)
                    (list ytdl--download-in-progress
                          ytdl--mode-line-string)
                    (mapcar
                     (lambda (name)
                       (let ((path (expand-file-name name download-folder)))
                         (list
                          path
                          (and (file-exists-p path)
                               (with-temp-buffer
                                 (insert-file-contents path)
                                 (buffer-string))))))
                     '("Retry alpha λ.mp4" "Retry beta β.mp4"))))
             (setq result
                   (list
                    :prompts (nreverse prompts)
                    :failed failed
                    :queued queued
                    :recovered recovered
                    :calls
                    (neomacs-melpa-ytdl--file-lines calls-file)
                    :messages (nreverse messages)))
             result)))
     (when (buffer-live-p buffer)
       (kill-buffer buffer))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:prompts (("[ytdl] URL: " "https://fallback.invalid/retry-all") ("[ytdl] Filename [no extension]: " nil) ("[ytdl] URL: " "https://fallback.invalid/retry-all") ("[ytdl] Filename [no extension]: " nil)) :failed ((("https://videos.example/watch?v=retry-a2026-08-03-19:00:01" "Retry alpha λ" "error" "Retry queue" nil "?" "ERROR: upstream maintenance λ" "https://videos.example/watch?v=retry-a") ("https://videos.example/watch?v=retry-b2026-08-03-19:00:02" "Retry beta β" "error" "Retry queue" nil "?" "ERROR: upstream maintenance λ" "https://videos.example/watch?v=retry-b")) (0 "")) :queued ((("https://videos.example/watch?v=retry-a2026-08-03-19:00:03" "Retry alpha λ" "downloading" "Retry queue" nil "?" nil "https://videos.example/watch?v=retry-a") ("https://videos.example/watch?v=retry-b2026-08-03-19:00:04" "Retry beta β" "downloading" "Retry queue" nil "?" nil "https://videos.example/watch?v=retry-b")) (2 "[ytdl 2]")) :recovered ((("https://videos.example/watch?v=retry-a2026-08-03-19:00:03" "Retry alpha λ" "downloaded" "Retry queue" "[ORACLE-SANDBOX]/ytdl-relaunch-all/retry queue/Retry alpha λ.mp4" "14" nil "https://videos.example/watch?v=retry-a") ("https://videos.example/watch?v=retry-b2026-08-03-19:00:04" "Retry beta β" "downloaded" "Retry queue" "[ORACLE-SANDBOX]/ytdl-relaunch-all/retry queue/Retry beta β.mp4" "14" nil "https://videos.example/watch?v=retry-b")) (0 "") (("[ORACLE-SANDBOX]/ytdl-relaunch-all/retry queue/Retry alpha λ.mp4" "retried-media\n") ("[ORACLE-SANDBOX]/ytdl-relaunch-all/retry queue/Retry beta β.mp4" "retried-media\n"))) :calls ("CALL <-o> <[ORACLE-SANDBOX]/ytdl-relaunch-all/retry queue/Retry alpha λ.%(ext)s> <--retries> <3> <--> <https://videos.example/watch?v=retry-a>" "CALL <-o> <[ORACLE-SANDBOX]/ytdl-relaunch-all/retry queue/Retry beta β.%(ext)s> <--retries> <3> <--> <https://videos.example/watch?v=retry-b>" "CALL <-o> <[ORACLE-SANDBOX]/ytdl-relaunch-all/retry queue/Retry alpha λ.%(ext)s> <--retries> <3> <--> <https://videos.example/watch?v=retry-a>" "CALL <-o> <[ORACLE-SANDBOX]/ytdl-relaunch-all/retry queue/Retry beta β.%(ext)s> <--retries> <3> <--> <https://videos.example/watch?v=retry-b>") :messages ("[ytdl] ERROR: upstream maintenance λ" "[ytdl] ERROR: upstream maintenance λ" "[ytdl] Video downloaded: [ORACLE-SANDBOX]/ytdl-relaunch-all/retry queue/Retry alpha λ.mp4" "[ytdl] Video downloaded: [ORACLE-SANDBOX]/ytdl-relaunch-all/retry queue/Retry beta β.mp4"))"####
    ]];
    ParityBatchCase::value(
        "relaunches_every_failed_download_from_the_list",
        elisp_form,
        expect,
    )
}

fn bulk_removes_finished_downloads_and_interrupts_active_downloads() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "ytdl-bulk-list"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (download-folder (expand-file-name "team recordings" sandbox))
        (downloader (expand-file-name "youtube-dl-parity" sandbox))
        (calls-file (expand-file-name "calls.log" sandbox))
        (process-environment
         (cons (concat "NEOMACS_YTDL_CALLS=" calls-file)
               process-environment))
        (ytdl-command downloader)
        (ytdl-download-types nil)
        (ytdl--download-list (make-hash-table :test 'equal))
        (ytdl--marked-items nil)
        (ytdl--last-downloaded-file-name nil)
        (ytdl--download-in-progress 0)
        (ytdl--mode-line-string "")
        (ytdl--mode-line-initialized? nil)
        (global-mode-string nil)
        (kill-ring '("https://fallback.invalid/bulk"))
        (kill-ring-yank-pointer kill-ring)
        (interprogram-paste-function nil)
        (inputs
         '("https://videos.example/watch?v=keep-a" "Keep file λ"
           "https://videos.example/watch?v=keep-b" "Keep file β"
           "https://videos.example/watch?v=purge-a" "Purge file α"
           "https://videos.example/watch?v=purge-b" "Purge file β"
           "https://videos.example/watch?v=active-a" "Active file α"
           "https://videos.example/watch?v=active-b" "Active file β"))
        (uuid-times
         '("2026-08-03-20:00:01" "2026-08-03-20:00:02"
           "2026-08-03-20:00:03" "2026-08-03-20:00:04"
           "2026-08-03-20:00:05" "2026-08-03-20:00:06"))
        async-jobs prompts confirmations messages interrupts
        completed-preserved after-preserve completed-purged after-purge
        active-before-clear after-clear buffer result)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory download-folder t)
         (neomacs-melpa-ytdl--write-executable
          downloader
          "printf 'CALL' >> \"$NEOMACS_YTDL_CALLS\"\nfor arg in \"$@\"; do printf ' <%s>' \"$arg\" >> \"$NEOMACS_YTDL_CALLS\"; done\nprintf '\\n' >> \"$NEOMACS_YTDL_CALLS\"\nout=''\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = '-o' ]; then out=$2; shift 2; else shift; fi\ndone\nbase=$(printf '%s' \"$out\" | sed 's/\\.%(ext)s$//')\nprintf 'bulk-media\\n' > \"${base}.mp4\"\n")
         (ytdl-add-field-in-download-type-list
          "Team recordings" "t" download-folder '("--no-mtime"))
         (let ((original-format-time-string
                (symbol-function 'format-time-string)))
           (cl-letf
               (((symbol-function 'async-start)
                 (lambda (worker callback)
                   (let ((process-id
                          (intern
                           (format "ytdl-bulk-process-%d"
                                   (1+ (length async-jobs))))))
                     (push (cons worker callback) async-jobs)
                     process-id)))
                ((symbol-function 'read-from-minibuffer)
                 (lambda (prompt &optional initial &rest _)
                   (push (list (substring-no-properties prompt) initial)
                         prompts)
                   (prog1 (car inputs)
                     (setq inputs (cdr inputs)))))
                ((symbol-function 'read-char-choice)
                 (lambda (&rest _) ?t))
                ((symbol-function 'pop-to-buffer)
                 (lambda (buffer &rest _) buffer))
                ((symbol-function 'y-or-n-p)
                 (lambda (prompt)
                   (push prompt confirmations)
                   t))
                ((symbol-function 'interrupt-process)
                 (lambda (process &rest _)
                   (push process interrupts)))
                ((symbol-function 'format-time-string)
                 (lambda (format-string &rest arguments)
                   (if (equal format-string "%Y-%m-%d-%T")
                       (prog1 (car uuid-times)
                         (setq uuid-times (cdr uuid-times)))
                     (apply original-format-time-string
                            format-string arguments))))
                ((symbol-function 'message)
                 (lambda (format-string &rest arguments)
                   (let ((text (apply #'format format-string arguments)))
                     (push text messages)
                     text))))
             ;; Users first remove two completed rows while keeping their files.
             (ytdl-download)
             (ytdl-download)
             (neomacs-melpa-ytdl--run-async-jobs async-jobs)
             (setq async-jobs nil
                   buffer (get-buffer ytdl-dl-buffer-name)
                   completed-preserved
                   (neomacs-melpa-ytdl--download-state))
             (with-current-buffer buffer
               (call-interactively (key-binding (kbd "M")))
               (call-interactively (key-binding (kbd "d"))))
             (setq after-preserve
                   (list
                    (neomacs-melpa-ytdl--download-state)
                    (file-exists-p
                     (expand-file-name "Keep file λ.mp4" download-folder))
                    (file-exists-p
                     (expand-file-name "Keep file β.mp4" download-folder))
                    (copy-sequence ytdl--marked-items)
                    (list ytdl--download-in-progress
                          ytdl--mode-line-string)))
             ;; A second pair is removed together with the media files.
             (ytdl-download)
             (ytdl-download)
             (neomacs-melpa-ytdl--run-async-jobs async-jobs)
             (setq async-jobs nil
                   completed-purged
                   (neomacs-melpa-ytdl--download-state))
             (with-current-buffer buffer
               (call-interactively (key-binding (kbd "M")))
               (call-interactively (key-binding (kbd "D"))))
             (setq after-purge
                   (list
                    (neomacs-melpa-ytdl--download-state)
                    (file-exists-p
                     (expand-file-name "Purge file α.mp4" download-folder))
                    (file-exists-p
                     (expand-file-name "Purge file β.mp4" download-folder))
                    (copy-sequence ytdl--marked-items)
                    (list ytdl--download-in-progress
                          ytdl--mode-line-string)))
             ;; Finally, clearing the whole list interrupts each queued download.
             (ytdl-download)
             (ytdl-download)
             (setq active-before-clear
                   (list
                    (neomacs-melpa-ytdl--download-state)
                    (list ytdl--download-in-progress
                          ytdl--mode-line-string)))
             (with-current-buffer buffer
               (call-interactively (key-binding (kbd "C"))))
             (setq after-clear
                   (list
                    (neomacs-melpa-ytdl--download-state)
                    (nreverse interrupts)
                    (list ytdl--download-in-progress
                          ytdl--mode-line-string)
                    (with-current-buffer buffer
                      (buffer-substring-no-properties
                       (point-min) (point-max)))))
             (setq result
                   (list
                    :prompts (nreverse prompts)
                    :completed-preserved completed-preserved
                    :after-preserve after-preserve
                    :completed-purged completed-purged
                    :after-purge after-purge
                    :active-before-clear active-before-clear
                    :after-clear after-clear
                    :calls
                    (neomacs-melpa-ytdl--file-lines calls-file)
                    :confirmations (nreverse confirmations)
                    :messages (nreverse messages)))
             result)))
     (when (buffer-live-p buffer)
       (kill-buffer buffer))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:prompts (("[ytdl] URL: " "https://fallback.invalid/bulk") ("[ytdl] Filename [no extension]: " nil) ("[ytdl] URL: " "https://fallback.invalid/bulk") ("[ytdl] Filename [no extension]: " nil) ("[ytdl] URL: " "https://fallback.invalid/bulk") ("[ytdl] Filename [no extension]: " nil) ("[ytdl] URL: " "https://fallback.invalid/bulk") ("[ytdl] Filename [no extension]: " nil) ("[ytdl] URL: " "https://fallback.invalid/bulk") ("[ytdl] Filename [no extension]: " nil) ("[ytdl] URL: " "https://fallback.invalid/bulk") ("[ytdl] Filename [no extension]: " nil)) :completed-preserved (("https://videos.example/watch?v=keep-a2026-08-03-20:00:01" "Keep file λ" "downloaded" "Team recordings" "[ORACLE-SANDBOX]/ytdl-bulk-list/team recordings/Keep file λ.mp4" "11" nil "https://videos.example/watch?v=keep-a") ("https://videos.example/watch?v=keep-b2026-08-03-20:00:02" "Keep file β" "downloaded" "Team recordings" "[ORACLE-SANDBOX]/ytdl-bulk-list/team recordings/Keep file β.mp4" "11" nil "https://videos.example/watch?v=keep-b")) :after-preserve (nil t t nil (0 "")) :completed-purged (("https://videos.example/watch?v=purge-a2026-08-03-20:00:03" "Purge file α" "downloaded" "Team recordings" "[ORACLE-SANDBOX]/ytdl-bulk-list/team recordings/Purge file α.mp4" "11" nil "https://videos.example/watch?v=purge-a") ("https://videos.example/watch?v=purge-b2026-08-03-20:00:04" "Purge file β" "downloaded" "Team recordings" "[ORACLE-SANDBOX]/ytdl-bulk-list/team recordings/Purge file β.mp4" "11" nil "https://videos.example/watch?v=purge-b")) :after-purge (nil nil nil nil (0 "")) :active-before-clear ((("https://videos.example/watch?v=active-a2026-08-03-20:00:05" "Active file α" "downloading" "Team recordings" nil "?" nil "https://videos.example/watch?v=active-a") ("https://videos.example/watch?v=active-b2026-08-03-20:00:06" "Active file β" "downloading" "Team recordings" nil "?" nil "https://videos.example/watch?v=active-b")) (2 "[ytdl 2]")) :after-clear (nil (ytdl-bulk-process-2 ytdl-bulk-process-1) (0 "") "") :calls ("CALL <-o> <[ORACLE-SANDBOX]/ytdl-bulk-list/team recordings/Keep file λ.%(ext)s> <--no-mtime> <--> <https://videos.example/watch?v=keep-a>" "CALL <-o> <[ORACLE-SANDBOX]/ytdl-bulk-list/team recordings/Keep file β.%(ext)s> <--no-mtime> <--> <https://videos.example/watch?v=keep-b>" "CALL <-o> <[ORACLE-SANDBOX]/ytdl-bulk-list/team recordings/Purge file α.%(ext)s> <--no-mtime> <--> <https://videos.example/watch?v=purge-a>" "CALL <-o> <[ORACLE-SANDBOX]/ytdl-bulk-list/team recordings/Purge file β.%(ext)s> <--no-mtime> <--> <https://videos.example/watch?v=purge-b>") :confirmations ("[ytdl] Remove those 2 item(s)?" "[ytdl] Remove those 2 item(s)? The associated files will be deleted as well." "[ytdl] Stop current downloads and clear the whole list?") :messages ("[ytdl] Video downloaded: [ORACLE-SANDBOX]/ytdl-bulk-list/team recordings/Keep file λ.mp4" "[ytdl] Video downloaded: [ORACLE-SANDBOX]/ytdl-bulk-list/team recordings/Keep file β.mp4" "[ytdl] Video downloaded: [ORACLE-SANDBOX]/ytdl-bulk-list/team recordings/Purge file α.mp4" "[ytdl] Video downloaded: [ORACLE-SANDBOX]/ytdl-bulk-list/team recordings/Purge file β.mp4"))"####
    ]];
    ParityBatchCase::value(
        "bulk_removes_finished_downloads_and_interrupts_active_downloads",
        elisp_form,
        expect,
    )
}

fn downloads_and_opens_a_server_named_file_then_reports_metadata_failures() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "ytdl-default-name"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (download-folder (expand-file-name "named downloads" sandbox))
        (downloader (expand-file-name "youtube-dl-parity" sandbox))
        (player (expand-file-name "player-parity" sandbox))
        (calls-file (expand-file-name "calls.log" sandbox))
        (process-environment
         (cons (concat "NEOMACS_YTDL_CALLS=" calls-file)
               process-environment))
        (ytdl-command downloader)
        (ytdl-media-player player)
        (ytdl-always-query-default-filename 'yes)
        (ytdl-download-types nil)
        (ytdl--download-list (make-hash-table :test 'equal))
        (ytdl--marked-items nil)
        (ytdl--last-downloaded-file-name nil)
        (ytdl--download-in-progress 0)
        (ytdl--mode-line-string "")
        (ytdl--mode-line-initialized? nil)
        (global-mode-string nil)
        (kill-ring '("https://fallback.invalid/default"))
        (kill-ring-yank-pointer kill-ring)
        (interprogram-paste-function nil)
        (inputs '("https://videos.example/watch?v=server-name-λ"))
        async-jobs prompts player-calls messages metadata-failure
        buffer result)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory download-folder t)
         (neomacs-melpa-ytdl--write-executable
          downloader
          "printf 'CALL' >> \"$NEOMACS_YTDL_CALLS\"\nfor arg in \"$@\"; do printf ' <%s>' \"$arg\" >> \"$NEOMACS_YTDL_CALLS\"; done\nprintf '\\n' >> \"$NEOMACS_YTDL_CALLS\"\ncase \" $* \" in\n  *' --get-filename '*'metadata-fail'*)\n    printf '%s\\n' 'ERROR: metadata unavailable λ'\n    ;;\n  *' --get-filename '*)\n    printf '%s\\n' 'Release_Review.v2_lambda.mp4'\n    ;;\n  *)\n    out=''\n    while [ \"$#\" -gt 0 ]; do\n      if [ \"$1\" = '-o' ]; then out=$2; shift 2; else shift; fi\n    done\n    base=$(printf '%s' \"$out\" | sed 's/\\.%(ext)s$//')\n    printf 'server-named-media\\n' > \"${base}.mp4\"\n    ;;\nesac\n")
         (neomacs-melpa-ytdl--write-executable player "exit 0\n")
         (ytdl-add-field-in-download-type-list
          "Named" "n" download-folder '("--embed-metadata"))
         (let ((original-format-time-string
                (symbol-function 'format-time-string)))
           (cl-letf
               (((symbol-function 'async-start)
                 (lambda (worker callback)
                   (push (cons worker callback) async-jobs)
                   'ytdl-named-process))
                ((symbol-function 'read-from-minibuffer)
                 (lambda (prompt &optional initial &rest _)
                   (push (list (substring-no-properties prompt) initial)
                         prompts)
                   (prog1 (car inputs)
                     (setq inputs (cdr inputs)))))
                ((symbol-function 'read-char-choice)
                 (lambda (&rest _) ?n))
                ((symbol-function 'format-time-string)
                 (lambda (format-string &rest arguments)
                   (if (equal format-string "%Y-%m-%d-%T")
                       "2026-08-03-18:19:20"
                     (apply original-format-time-string
                            format-string arguments))))
                ((symbol-function 'start-process-shell-command)
                 (lambda (name process-buffer command)
                   (push (list name process-buffer command) player-calls)
                   'ytdl-named-player-process))
                ((symbol-function 'message)
                 (lambda (format-string &rest arguments)
                   (let ((text (apply #'format format-string arguments)))
                     (push text messages)
                     text))))
             (ytdl-download-open)
             (neomacs-melpa-ytdl--run-async-jobs async-jobs)
             (setq buffer (get-buffer ytdl-dl-buffer-name))
             (setq inputs '("https://videos.example/watch?v=metadata-fail"))
             (setq metadata-failure
                   (neomacs-melpa-ytdl--capture-error #'ytdl-download))
             (setq result
                   (list
                    :prompts (nreverse prompts)
                    :downloads (neomacs-melpa-ytdl--download-state)
                    :file
                    (with-temp-buffer
                      (insert-file-contents
                       ytdl--last-downloaded-file-name)
                      (list ytdl--last-downloaded-file-name
                            (buffer-string)))
                    :players (nreverse player-calls)
                    :metadata-failure metadata-failure
                    :calls
                    (neomacs-melpa-ytdl--file-lines calls-file)
                    :mode-line
                    (list ytdl--download-in-progress
                          ytdl--mode-line-string)
                    :messages (nreverse messages)))
             result)))
     (when (buffer-live-p buffer)
       (kill-buffer buffer))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:prompts (("[ytdl] URL: " "https://fallback.invalid/default") ("[ytdl] URL: " "https://fallback.invalid/default")) :downloads (("https://videos.example/watch?v=server-name-λ2026-08-03-18:19:20" "Release-Review" "downloaded" "Named" "[ORACLE-SANDBOX]/ytdl-default-name/named downloads/Release-Review.mp4" "19" nil "https://videos.example/watch?v=server-name-λ")) :file ("[ORACLE-SANDBOX]/ytdl-default-name/named downloads/Release-Review.mp4" "server-named-media\n") :players (("[ORACLE-SANDBOX]/ytdl-default-name/player-parity" nil "[ORACLE-SANDBOX]/ytdl-default-name/player-parity [ORACLE-SANDBOX]/ytdl-default-name/named\\ downloads/Release-Review.mp4")) :metadata-failure (error (error "ERROR: metadata unavailable λ")) :calls ("CALL <--get-filename> <--restrict-filenames> <--> <https://videos.example/watch?v=server-name-λ>" "CALL <-o> <[ORACLE-SANDBOX]/ytdl-default-name/named downloads/Release-Review.%(ext)s> <--embed-metadata> <--> <https://videos.example/watch?v=server-name-λ>" "CALL <--get-filename> <--restrict-filenames> <--> <https://videos.example/watch?v=metadata-fail>") :mode-line (0 "") :messages ("[ytdl] Video downloaded: [ORACLE-SANDBOX]/ytdl-default-name/named downloads/Release-Review.mp4"))"####
    ]];
    ParityBatchCase::value(
        "downloads_and_opens_a_server_named_file_then_reports_metadata_failures",
        elisp_form,
        expect,
    )
}

fn runs_the_documented_eshell_download_without_mutating_the_download_list() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "ytdl-eshell"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (download-folder (expand-file-name "eshell downloads" sandbox))
        (downloader (expand-file-name "youtube-dl-parity" sandbox))
        (calls-file (expand-file-name "calls.log" sandbox))
        (process-environment
         (cons (concat "NEOMACS_YTDL_CALLS=" calls-file)
               process-environment))
        (default-directory (file-name-as-directory sandbox))
        (ytdl-command downloader)
        (ytdl-download-types nil)
        (ytdl--download-list (make-hash-table :test 'equal))
        (ytdl--download-in-progress 0)
        (ytdl--mode-line-string "")
        (ytdl--mode-line-initialized? nil)
        (global-mode-string nil)
        (kill-ring '("https://fallback.invalid/eshell"))
        (kill-ring-yank-pointer kill-ring)
        (interprogram-paste-function nil)
        (inputs
         '("https://videos.example/watch?a=1&b=release-λ"
           "Eshell release λ"))
        (eshell-banner-message "")
        (eshell-prompt-function (lambda () "PARITY> "))
        (eshell-prompt-regexp "^PARITY> ")
        prompts buffer result)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory download-folder t)
         (neomacs-melpa-ytdl--write-executable
          downloader
          "printf 'PWD <%s>\\n' \"$PWD\" >> \"$NEOMACS_YTDL_CALLS\"\nprintf 'CALL' >> \"$NEOMACS_YTDL_CALLS\"\nfor arg in \"$@\"; do printf ' <%s>' \"$arg\" >> \"$NEOMACS_YTDL_CALLS\"; done\nprintf '\\n' >> \"$NEOMACS_YTDL_CALLS\"\nout=''\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = '-o' ]; then out=$2; shift 2; else shift; fi\ndone\nbase=$(printf '%s' \"$out\" | sed 's/\\.%(ext)s$//')\nprintf 'eshell-media\\n' > \"${base}.mp4\"\nprintf 'download-complete λ\\n'\n")
         (ytdl-add-field-in-download-type-list
          "Eshell" "e" download-folder
          '("--write-description" "--no-mtime"))
         (cl-letf
             (((symbol-function 'read-from-minibuffer)
               (lambda (prompt &optional initial &rest _)
                 (push (list (substring-no-properties prompt) initial)
                       prompts)
                 (prog1 (car inputs)
                   (setq inputs (cdr inputs)))))
              ((symbol-function 'read-char-choice)
               (lambda (&rest _) ?e)))
           (ytdl-download-eshell)
           (setq buffer (get-buffer "*ytdl*"))
           (let ((process (and buffer (get-buffer-process buffer)))
                 (attempts 0))
             (while (and process
                         (process-live-p process)
                         (< attempts 100))
               (accept-process-output process 0.05)
               (setq attempts (1+ attempts)))
             (when (and process (process-live-p process))
               (error "Timed out waiting for the Eshell download")))
           (setq result
                 (list
                  :prompts (nreverse prompts)
                  :calls
                  (neomacs-melpa-ytdl--file-lines calls-file)
                  :file
                  (let ((path
                         (expand-file-name
                          "Eshell release λ.mp4" download-folder))
                        (actual
                         (expand-file-name
                          "Eshell release λ.%ext)s.mp4" download-folder)))
                    (list
                     path
                     (file-exists-p path)
                     actual
                     (and (file-exists-p actual)
                          (with-temp-buffer
                            (insert-file-contents actual)
                            (buffer-string)))))
                  :eshell
                  (with-current-buffer buffer
                    (list
                     (buffer-substring-no-properties
                      (point-min) (point-max))
                     major-mode default-directory
                     (and (get-buffer-process buffer)
                          (process-status (get-buffer-process buffer)))))
                  :downloads (neomacs-melpa-ytdl--download-state)
                  :mode-line
                  (list ytdl--download-in-progress
                        ytdl--mode-line-string)))
           result))
     (when (buffer-live-p buffer)
       (let ((process (get-buffer-process buffer)))
         (when (and process (process-live-p process))
           (delete-process process)))
       (kill-buffer buffer))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:prompts (("[ytdl] URL: " "https://fallback.invalid/eshell") ("[ytdl] Filename [no extension]: " nil)) :calls ("PWD <[ORACLE-SANDBOX]/ytdl-eshell/eshell downloads>" "CALL <https://videos.example/watch?a=1&b=release-λ> <-o> <Eshell release λ.%ext)s> <--write-description> <--no-mtime>") :file ("[ORACLE-SANDBOX]/ytdl-eshell/eshell downloads/Eshell release λ.mp4" nil "[ORACLE-SANDBOX]/ytdl-eshell/eshell downloads/Eshell release λ.%ext)s.mp4" "eshell-media\n") :eshell ("cd [ORACLE-SANDBOX]/ytdl-eshell/eshell\\ downloads/ && [ORACLE-SANDBOX]/ytdl-eshell/youtube-dl-parity https\\://videos.example/watch\\?a\\=1\\&b\\=release-\\λ -o Eshell\\ release\\ \\λ.%(ext)s --write-description --no-mtime\ndownload-complete λ\n" eshell-mode "[ORACLE-SANDBOX]/ytdl-eshell/eshell downloads/" nil) :downloads nil :mode-line (0 ""))"####
    ]];
    ParityBatchCase::value(
        "runs_the_documented_eshell_download_without_mutating_the_download_list",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        downloads_unicode_media_updates_the_list_and_opens_the_file(),
        fans_out_a_playlist_then_marks_opens_and_cleans_entries(),
        selects_a_real_format_and_downloads_with_validated_custom_arguments(),
        surfaces_download_errors_relaunches_and_preserves_explicit_deletion_semantics(),
        relaunches_every_failed_download_from_the_list(),
        bulk_removes_finished_downloads_and_interrupts_active_downloads(),
        downloads_and_opens_a_server_named_file_then_reports_metadata_failures(),
        runs_the_documented_eshell_download_without_mutating_the_download_list(),
    ]
}
