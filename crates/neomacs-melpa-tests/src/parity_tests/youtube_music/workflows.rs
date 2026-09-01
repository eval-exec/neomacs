use expect_test::expect;

use super::ParityBatchCase;

fn renders_the_real_status_buffer_and_routes_playback_controls() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((youtube-music-buffer-name "*YouTube Music status parity*")
        (sandbox
         (expand-file-name
          "youtube-music-status"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (youtube-music-credentials-file
         (expand-file-name "credentials.eld" sandbox))
        (youtube-music-progress-bar-width 12)
        (youtube-music-seek-step 7)
        (youtube-music--state
         '(:title "mpv fallback" :pause nil :time-pos 102.7 :duration 248
           :playlist-pos 1
           :path "https://music.youtube.com/watch?v=vid-two"
           :loop-file "no" :loop-playlist "inf"))
        (youtube-music--playlist-cache
         '((:filename "https://music.youtube.com/watch?v=vid-one"
            :title "mpv one")
           (:filename "https://music.youtube.com/watch?v=vid-two"
            :title "mpv two")
           (:filename "https://youtu.be/vid-three"
            :title "mpv three")))
        (youtube-music--track-meta (make-hash-table :test 'equal))
        (youtube-music--liked-set (make-hash-table :test 'equal))
        (youtube-music--disliked-set (make-hash-table :test 'equal))
        (youtube-music--auth-state 'logged-in)
        (youtube-music--account-name "Parity Listener λ")
        (youtube-music--shuffled-p t)
        (youtube-music--mpv-process 'neomacs-melpa-youtube-music--mpv)
        (youtube-music--ipc-process 'neomacs-melpa-youtube-music--ipc)
        (youtube-music--request-counter 0)
        (youtube-music--pending-requests (make-hash-table :test 'eql))
        (youtube-music--ipc-buffer "")
        (neomacs-melpa-youtube-music--ipc-payloads nil)
        (neomacs-melpa-youtube-music--ipc-playlist-response
         youtube-music--playlist-cache)
        messages rendered rows mode-state)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory sandbox t)
         (puthash "vid-one" '(:title "First Light" :subtitle "Artist Alpha")
                  youtube-music--track-meta)
         (puthash "vid-two" '(:title "Release λ" :subtitle "Artist Beta")
                  youtube-music--track-meta)
         (puthash "vid-three" '(:title "Recovery" :subtitle "")
                  youtube-music--track-meta)
         (puthash "vid-two" t youtube-music--liked-set)
         (puthash "vid-three" t youtube-music--disliked-set)
         (cl-letf (((symbol-function 'process-live-p)
                    #'neomacs-melpa-youtube-music--process-live-p)
                   ((symbol-function 'process-send-string)
                    #'neomacs-melpa-youtube-music--process-send-string)
                   ((symbol-function 'char-displayable-p)
                    (lambda (&rest _) nil))
                   ((symbol-function 'message)
                    (lambda (format-string &rest arguments)
                      (let ((text (apply #'format format-string arguments)))
                        (push text messages)
                        text))))
           (youtube-music)
           (with-current-buffer youtube-music-buffer-name
             (setq rendered
                   (buffer-substring-no-properties (point-min) (point-max)))
             (dolist (index '(0 1 2))
               (let ((position
                      (text-property-any
                       (point-min) (point-max)
                       'youtube-music-playlist-index index)))
                 (push
                  (list index
                        (line-number-at-pos position)
                        (get-text-property position 'face)
                        (save-excursion
                          (goto-char position)
                          (buffer-substring-no-properties
                           (line-beginning-position)
                           (line-end-position))))
                  rows)))
             (let ((position
                    (text-property-any
                     (point-min) (point-max)
                     'youtube-music-playlist-index 1)))
               (goto-char position)
               (youtube-music-play-at-point)
               (youtube-music-remove-at-point))
             (setq mode-state
                   (list major-mode buffer-read-only truncate-lines
                         (lookup-key youtube-music-mode-map (kbd "SPC"))
                         (lookup-key youtube-music-mode-map (kbd "RET"))
                         (lookup-key youtube-music-mode-map (kbd "+")))))
           (youtube-music-play-pause)
           (youtube-music-stop)
           (youtube-music-next)
           (youtube-music-prev)
           (youtube-music-seek-forward)
           (youtube-music-seek-backward)
           (youtube-music-toggle-shuffle)
           (youtube-music-toggle-shuffle)
           (youtube-music-cycle-repeat)
           (list :rendered rendered
                 :rows (nreverse rows)
                 :mode mode-state
                 :ipc (nreverse neomacs-melpa-youtube-music--ipc-payloads)
                 :messages (nreverse messages)
                 :shuffle youtube-music--shuffled-p)))
     (when (get-buffer youtube-music-buffer-name)
       (kill-buffer youtube-music-buffer-name))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:rendered "── Now Playing ──\n  ▶ Release λ — Artist Beta  +  ↯ ↻\n  ████░░░░░░░░   1:42 / 4:08\n\n── Queue ──\n    1  First Light — Artist Alpha\n ▶  2  Release λ — Artist Beta  +\n    3  Recovery  -\n\n── Sources ──\n  s  Search (enqueue)    S  Search (replace)\n  u  Play URL            e  Enqueue URL\n  l  Library (liked, playlists, home)\n\n── Status ──\n  ● signed in as Parity Listener λ\n\n  ? menu   SPC pause  n next  p prev  x stop  f/b seek    g refresh  q bury  Q quit\n" :rows ((0 6 youtube-music-queue-past "    1  First Light — Artist Alpha") (1 7 youtube-music-queue-current " ▶  2  Release λ — Artist Beta  +") (2 8 default "    3  Recovery  -")) :mode (youtube-music-mode t t youtube-music-play-pause youtube-music-play-at-point youtube-music-like) :ipc ((1 ("get_property" "playlist")) (2 ("playlist-play-index" 1)) (3 ("playlist-remove" 1)) (4 ("cycle" "pause")) (5 ("seek" 0 "absolute")) (6 ("set" "pause" "yes")) (7 ("playlist-next" "weak")) (8 ("playlist-prev" "weak")) (9 ("seek" 7 "relative")) (10 ("seek" -7 "relative")) (11 ("playlist-unshuffle")) (12 ("playlist-shuffle")) (13 ("get_property" "playlist-pos")) (14 ("playlist-move" 1 0)) (15 ("set" "loop-playlist" "no")) (16 ("set" "loop-file" "inf"))) :messages ("youtube-music: shuffle off" "youtube-music: shuffle on" "youtube-music: repeat track") :shuffle t)"####
    ]];
    ParityBatchCase::value(
        "renders_the_real_status_buffer_and_routes_playback_controls",
        elisp_form,
        expect,
    )
}

fn searches_unicode_songs_and_enqueues_a_resolved_playlist() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((youtube-music--cookie "SAPISID=secret-λ; SID=session")
        (youtube-music--sapisid "secret-λ")
        (youtube-music--track-meta (make-hash-table :test 'equal))
        (youtube-music--shuffled-p nil)
        (youtube-music--mpv-process 'neomacs-melpa-youtube-music--mpv)
        (youtube-music--ipc-process 'neomacs-melpa-youtube-music--ipc)
        (youtube-music--request-counter 0)
        (youtube-music--pending-requests (make-hash-table :test 'eql))
        (youtube-music--ipc-buffer "")
        (neomacs-melpa-youtube-music--ipc-payloads nil)
        (neomacs-melpa-youtube-music--ipc-playlist-response nil)
        (youtube-music-search-max-results 4)
        (neomacs-melpa-youtube-music--response-buffers nil)
        (songs-json
         "{\"contents\":{\"tabbedSearchResultsRenderer\":{\"tabs\":[{\"tabRenderer\":{\"content\":{\"sectionListRenderer\":{\"contents\":[{\"musicShelfRenderer\":{\"contents\":[{\"musicResponsiveListItemRenderer\":{\"flexColumns\":[{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Résilience λ\"}]}}},{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Song • Artist α • 3:45\"}]}}}],\"playlistItemData\":{\"videoId\":\"song-resilience\"}}}]}}]}}}}]}}}")
        (playlists-json
         "{\"contents\":{\"tabbedSearchResultsRenderer\":{\"tabs\":[{\"tabRenderer\":{\"content\":{\"sectionListRenderer\":{\"contents\":[{\"musicShelfRenderer\":{\"contents\":[{\"musicResponsiveListItemRenderer\":{\"flexColumns\":[{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Ship Room Mix\"}]}}},{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Playlist • 12 songs\"}]}}}],\"navigationEndpoint\":{\"browseEndpoint\":{\"browseId\":\"VLPLSHIP\",\"browseEndpointContextSupportedConfigs\":{\"browseEndpointContextMusicConfig\":{\"pageType\":\"MUSIC_PAGE_TYPE_PLAYLIST\"}}}}}}]}}]}}}}]}}}")
        (browse-json
         "{\"contents\":{\"singleColumnBrowseResultsRenderer\":{\"tabs\":[{\"tabRenderer\":{\"content\":{\"sectionListRenderer\":{\"contents\":[{\"musicPlaylistShelfRenderer\":{\"contents\":[{\"musicResponsiveListItemRenderer\":{\"flexColumns\":[{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Rollback Song\"}]}}},{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Release Crew\"}]}}}],\"playlistItemData\":{\"videoId\":\"rollback-song\"}}},{\"musicResponsiveListItemRenderer\":{\"flexColumns\":[{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Recovery Song\"}]}}},{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Incident Band\"}]}}}],\"playlistItemData\":{\"videoId\":\"recovery-song\"}}}]}}]}}}}]}}}")
        requests request-headers prompts messages)
   (unwind-protect
       (cl-letf (((symbol-function 'float-time)
                  (lambda (&rest _) 1722643200.0))
                 ((symbol-function 'url-retrieve)
                  (lambda (url callback &rest _)
                    (let* ((body (decode-coding-string url-request-data 'utf-8))
                           (response
                            (cond
                             ((string-match-p
                               (regexp-quote youtube-music--search-songs-params)
                               body)
                              songs-json)
                             ((string-match-p
                               (regexp-quote youtube-music--search-playlists-params)
                               body)
                              playlists-json)
                             ((string-match-p "VLPLSHIP" body) browse-json)
                             (t (error "Unexpected YouTube Music request: %s" body))))
                           (headers
                            (mapcar
                             (lambda (header)
                               (cons (car header)
                                     (decode-coding-string (cdr header) 'utf-8)))
                             url-request-extra-headers))
                           (buffer
                            (neomacs-melpa-youtube-music--response-buffer
                             response)))
                      (unless request-headers
                        (setq request-headers headers))
                      (push (list url body) requests)
                      (with-current-buffer buffer
                        (funcall callback nil))
                      buffer)))
                 ((symbol-function 'completing-read)
                  (lambda (prompt collection &rest _)
                    (let ((candidates
                           (mapcar #'substring-no-properties
                                   (all-completions "" collection))))
                      (push (list prompt candidates) prompts)
                      (if (string= prompt "Play: ")
                          "Résilience λ — Artist α • 3:45"
                        "Ship Room Mix — 12 songs"))))
                 ((symbol-function 'process-live-p)
                  #'neomacs-melpa-youtube-music--process-live-p)
                 ((symbol-function 'process-send-string)
                  #'neomacs-melpa-youtube-music--process-send-string)
                 ((symbol-function 'message)
                  (lambda (format-string &rest arguments)
                    (let ((text (apply #'format format-string arguments)))
                      (push text messages)
                      text))))
         (youtube-music-search "café resilience λ")
         (youtube-music-search "release readiness" t)
         (list
          :requests (nreverse requests)
          :headers request-headers
          :prompts (nreverse prompts)
          :ipc (nreverse neomacs-melpa-youtube-music--ipc-payloads)
          :messages (nreverse messages)
          :metadata
          (mapcar
           (lambda (video-id)
             (cons video-id (gethash video-id youtube-music--track-meta)))
           '("song-resilience" "rollback-song" "recovery-song"))))
     (neomacs-melpa-youtube-music--cleanup-response-buffers))))
"####;
    let expect = expect![[
        r####"OK (:requests (("https://music.youtube.com/youtubei/v1/search?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}},\"query\":\"café resilience λ\",\"params\":\"EgWKAQIIAWoMEA4QChADEAQQCRAF\"}") ("https://music.youtube.com/youtubei/v1/search?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}},\"query\":\"café resilience λ\",\"params\":\"EgeKAQQoAEABagwQDhAKEAMQBBAJEAU%3D\"}") ("https://music.youtube.com/youtubei/v1/search?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}},\"query\":\"release readiness\",\"params\":\"EgWKAQIIAWoMEA4QChADEAQQCRAF\"}") ("https://music.youtube.com/youtubei/v1/search?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}},\"query\":\"release readiness\",\"params\":\"EgeKAQQoAEABagwQDhAKEAMQBBAJEAU%3D\"}") ("https://music.youtube.com/youtubei/v1/browse?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}},\"browseId\":\"VLPLSHIP\"}")) :headers (("User-Agent" . "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36") ("Cookie" . "SAPISID=secret-λ; SID=session") ("Authorization" . "SAPISIDHASH 1722643200_29a94be0088c24036473fc20b67cb13e6acabe67") ("X-Origin" . "https://music.youtube.com") ("X-Goog-AuthUser" . "0") ("Origin" . "https://music.youtube.com") ("Content-Type" . "application/json; charset=UTF-8") ("Accept" . "*/*")) :prompts (("Play: " ("Résilience λ — Artist α • 3:45" "Ship Room Mix — 12 songs")) ("Enqueue: " ("Résilience λ — Artist α • 3:45" "Ship Room Mix — 12 songs"))) :ipc ((1 ("loadfile" "https://music.youtube.com/watch?v=song-resilience" "replace")) (2 ("loadfile" "https://music.youtube.com/watch?v=rollback-song" "append-play")) (3 ("loadfile" "https://music.youtube.com/watch?v=recovery-song" "append-play"))) :messages ("youtube-music: searching for café resilience λ..." "youtube-music: searching for release readiness..." "youtube-music: loading playlist..." "youtube-music: appended 2 tracks") :metadata (("song-resilience" :title "Résilience λ" :subtitle "Artist α • 3:45") ("rollback-song" :title "Rollback Song" :subtitle "Release Crew") ("recovery-song" :title "Recovery Song" :subtitle "Incident Band")))"####
    ]];
    ParityBatchCase::value(
        "searches_unicode_songs_and_enqueues_a_resolved_playlist",
        elisp_form,
        expect,
    )
}

fn logs_in_from_a_pasted_cookie_and_logs_out_cleanly() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((youtube-music-buffer-name "*YouTube Music auth status parity*")
        (sandbox
         (expand-file-name
          "youtube-music-auth"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (youtube-music-credentials-file
         (expand-file-name "credentials.eld" sandbox))
        (youtube-music--cookie nil)
        (youtube-music--sapisid nil)
        (youtube-music--auth-state 'unknown)
        (youtube-music--account-name nil)
        (youtube-music--auto-refresh-attempted nil)
        (neomacs-melpa-youtube-music--response-buffers nil)
        (account-json
         "{\"responseContext\":{\"serviceTrackingParams\":[{\"service\":\"GFEEDBACK\",\"params\":[{\"key\":\"logged_in\",\"value\":\"1\"}]}]},\"actions\":[{\"openPopupAction\":{\"popup\":{\"multiPageMenuRenderer\":{\"header\":{\"activeAccountHeaderRenderer\":{\"accountName\":{\"runs\":[{\"text\":\"Parity Listener λ\"}]}}}}}}}]}")
        messages request login-buffer credentials saved-state final-state)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory sandbox t)
         (setq youtube-music--cookie nil
               youtube-music--sapisid nil
               youtube-music--auth-state 'unknown
               youtube-music--account-name nil)
         (cl-letf (((symbol-function 'float-time)
                    (lambda (&rest _) 1722643201.0))
                   ((symbol-function 'url-retrieve)
                    (lambda (url callback &rest _)
                      (setq request
                            (list url
                                  (decode-coding-string url-request-data 'utf-8)
                                  (mapcar
                                   (lambda (header)
                                     (cons
                                      (car header)
                                      (decode-coding-string
                                       (cdr header) 'utf-8)))
                                   url-request-extra-headers)))
                      (let ((buffer
                             (neomacs-melpa-youtube-music--response-buffer
                              account-json)))
                        (with-current-buffer buffer
                          (funcall callback nil))
                        buffer)))
                   ((symbol-function 'message)
                    (lambda (format-string &rest arguments)
                      (let* ((text (apply #'format format-string arguments))
                             (normalized
                              (replace-regexp-in-string
                               (regexp-quote youtube-music-credentials-file)
                               "<credentials>" text t t)))
                        (push normalized messages)
                        text))))
           (youtube-music-login-paste)
           (with-current-buffer "*youtube-music-login*"
             (setq login-buffer
                   (list
                    (buffer-substring-no-properties (point-min) (point-max))
                    (point) major-mode
                    (lookup-key youtube-music-login-mode-map (kbd "C-c C-c"))
                    (lookup-key youtube-music-login-mode-map (kbd "C-c C-k"))))
             (insert "SAPISID=pasted-secret; SID=session-42; PREF=hl=en")
             (youtube-music-login-finish))
           (setq credentials
                 (with-temp-buffer
                   (insert-file-contents youtube-music-credentials-file)
                   (list (buffer-string)
                         (file-modes youtube-music-credentials-file))))
           (setq saved-state
                 (list youtube-music--cookie youtube-music--sapisid
                       youtube-music--auth-state youtube-music--account-name
                       (get-buffer "*youtube-music-login*")))
           (youtube-music-auth-status)
           (youtube-music-logout)
           (setq final-state
                 (list (file-exists-p youtube-music-credentials-file)
                       youtube-music--cookie youtube-music--sapisid
                       youtube-music--auth-state youtube-music--account-name))
           (list :login-buffer login-buffer
                 :request request
                 :credentials credentials
                 :saved saved-state
                 :messages (nreverse messages)
                 :final final-state)))
     (when (get-buffer "*youtube-music-login*")
       (kill-buffer "*youtube-music-login*"))
     (when (get-buffer youtube-music-buffer-name)
       (kill-buffer youtube-music-buffer-name))
     (neomacs-melpa-youtube-music--cleanup-response-buffers)
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:login-buffer ("Paste your YouTube Music Cookie header below the line, then press\nC-c C-c to save, or C-c C-k to cancel.\n\nHow to obtain it:\n  1. Open https://music.youtube.com (signed in) in a browser.\n  2. Open DevTools (F12) and switch to the Network tab.\n  3. Refresh the page and click any music.youtube.com request.\n  4. Copy the entire 'Cookie' request header value.\n  5. Make sure it contains 'SAPISID=...'.\n----------------------------------------------------------------\n" 465 youtube-music-login-mode youtube-music-login-finish youtube-music-login-cancel) :request ("https://music.youtube.com/youtubei/v1/account/account_menu?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}}}" (("User-Agent" . "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36") ("Cookie" . "SAPISID=pasted-secret; SID=session-42; PREF=hl=en") ("Authorization" . "SAPISIDHASH 1722643201_9fa859384b6718e5389d74ffb58874c7dca146ee") ("X-Origin" . "https://music.youtube.com") ("X-Goog-AuthUser" . "0") ("Origin" . "https://music.youtube.com") ("Content-Type" . "application/json; charset=UTF-8") ("Accept" . "*/*"))) :credentials ("(:cookie \"SAPISID=pasted-secret; SID=session-42; PREF=hl=en\")" 384) :saved ("SAPISID=pasted-secret; SID=session-42; PREF=hl=en" "pasted-secret" logged-in "Parity Listener λ" nil) :messages ("youtube-music: logged in (cookie saved to <credentials>)" "youtube-music: logged in" "youtube-music: logged out") :final (nil nil nil logged-out nil))"####
    ]];
    ParityBatchCase::value(
        "logs_in_from_a_pasted_cookie_and_logs_out_cleanly",
        elisp_form,
        expect,
    )
}

fn browses_a_saved_playlist_and_home_recommendation() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((youtube-music--cookie "SAPISID=browse-secret; SID=session")
        (youtube-music--sapisid "browse-secret")
        (youtube-music--auth-state 'logged-in)
        (youtube-music--account-name "Browse Listener")
        (youtube-music--auto-refresh-attempted nil)
        (youtube-music--shuffled-p nil)
        (youtube-music--track-meta (make-hash-table :test 'equal))
        (youtube-music--mpv-process 'neomacs-melpa-youtube-music--mpv)
        (youtube-music--ipc-process 'neomacs-melpa-youtube-music--ipc)
        (youtube-music--request-counter 0)
        (youtube-music--pending-requests (make-hash-table :test 'eql))
        (youtube-music--ipc-buffer "")
        (neomacs-melpa-youtube-music--ipc-payloads nil)
        (neomacs-melpa-youtube-music--ipc-playlist-response nil)
        (neomacs-melpa-youtube-music--response-buffers nil)
        (playlists-json
         "{\"responseContext\":{\"serviceTrackingParams\":[{\"service\":\"GFEEDBACK\",\"params\":[{\"key\":\"logged_in\",\"value\":\"1\"}]}]},\"contents\":{\"singleColumnBrowseResultsRenderer\":{\"tabs\":[{\"tabRenderer\":{\"content\":{\"sectionListRenderer\":{\"contents\":[{\"gridRenderer\":{\"items\":[{\"musicTwoRowItemRenderer\":{\"title\":{\"runs\":[{\"text\":\"Incident Response Mix\"}]},\"navigationEndpoint\":{\"browseEndpoint\":{\"browseId\":\"VLLIBRARY\"}}}},{\"musicTwoRowItemRenderer\":{\"title\":{\"runs\":[{\"text\":\"Focus Queue\"}]},\"navigationEndpoint\":{\"browseEndpoint\":{\"browseId\":\"VLFOCUS\"}}}}]}}]}}}}]}}}")
        (playlist-tracks-json
         "{\"responseContext\":{\"serviceTrackingParams\":[{\"service\":\"GFEEDBACK\",\"params\":[{\"key\":\"logged_in\",\"value\":\"1\"}]}]},\"contents\":{\"singleColumnBrowseResultsRenderer\":{\"tabs\":[{\"tabRenderer\":{\"content\":{\"sectionListRenderer\":{\"contents\":[{\"musicPlaylistShelfRenderer\":{\"contents\":[{\"musicResponsiveListItemRenderer\":{\"flexColumns\":[{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Mitigation Song\"}]}}},{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Response Team\"}]}}}],\"playlistItemData\":{\"videoId\":\"mitigation-song\"}}},{\"musicResponsiveListItemRenderer\":{\"flexColumns\":[{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Postmortem λ\"}]}}},{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Learning Crew\"}]}}}],\"playlistItemData\":{\"videoId\":\"postmortem-song\"}}}]}}]}}}}]}}}")
        (home-json
         "{\"responseContext\":{\"serviceTrackingParams\":[{\"service\":\"GFEEDBACK\",\"params\":[{\"key\":\"logged_in\",\"value\":\"1\"}]}]},\"contents\":{\"singleColumnBrowseResultsRenderer\":{\"tabs\":[{\"tabRenderer\":{\"content\":{\"sectionListRenderer\":{\"contents\":[{\"musicCarouselShelfRenderer\":{\"header\":{\"musicCarouselShelfBasicHeaderRenderer\":{\"title\":{\"runs\":[{\"text\":\"Recovery Picks\"}]}}},\"contents\":[{\"musicTwoRowItemRenderer\":{\"title\":{\"runs\":[{\"text\":\"Home Resilience\"}]},\"subtitle\":{\"runs\":[{\"text\":\"Home Artist\"}]},\"navigationEndpoint\":{\"watchEndpoint\":{\"videoId\":\"home-resilience\"}}}},{\"musicTwoRowItemRenderer\":{\"title\":{\"runs\":[{\"text\":\"Home Rollback\"}]},\"subtitle\":{\"runs\":[{\"text\":\"Fallback Band\"}]},\"navigationEndpoint\":{\"watchEndpoint\":{\"videoId\":\"home-rollback\"}}}}]}}]}}}}]}}}")
        requests prompts messages)
   (unwind-protect
       (cl-letf (((symbol-function 'float-time)
                  (lambda (&rest _) 1722643203.0))
                 ((symbol-function 'url-retrieve)
                  (lambda (url callback &rest _)
                    (let* ((body (decode-coding-string url-request-data 'utf-8))
                           (response
                            (cond
                             ((string-match-p "FEmusic_liked_playlists" body)
                              playlists-json)
                             ((string-match-p "VLLIBRARY" body)
                              playlist-tracks-json)
                             ((string-match-p "FEmusic_home" body) home-json)
                             (t (error "Unexpected browse request: %s" body))))
                           (buffer
                            (neomacs-melpa-youtube-music--response-buffer
                             response)))
                      (push (list url body) requests)
                      (with-current-buffer buffer
                        (funcall callback nil))
                      buffer)))
                 ((symbol-function 'completing-read)
                  (lambda (prompt collection &rest _)
                    (let ((candidates
                           (mapcar #'substring-no-properties
                                   (all-completions "" collection))))
                      (push (list prompt candidates) prompts)
                      (cond
                       ((string= prompt "Playlist: ")
                        "Incident Response Mix")
                       ((string= prompt "Shelf: ") "Recovery Picks")
                       (t "Home Resilience — Home Artist")))))
                 ((symbol-function 'process-live-p)
                  #'neomacs-melpa-youtube-music--process-live-p)
                 ((symbol-function 'process-send-string)
                  #'neomacs-melpa-youtube-music--process-send-string)
                 ((symbol-function 'message)
                  (lambda (format-string &rest arguments)
                    (let ((text (apply #'format format-string arguments)))
                      (push text messages)
                      text))))
         (youtube-music-library-playlists)
         (youtube-music-home)
         (list :requests (nreverse requests)
               :prompts (nreverse prompts)
               :ipc (nreverse neomacs-melpa-youtube-music--ipc-payloads)
               :metadata
               (mapcar
                (lambda (video-id)
                  (cons video-id (gethash video-id youtube-music--track-meta)))
                '("mitigation-song" "postmortem-song" "home-resilience"))
               :messages (nreverse messages)))
     (neomacs-melpa-youtube-music--cleanup-response-buffers))))
"####;
    let expect = expect![[
        r####"OK (:requests (("https://music.youtube.com/youtubei/v1/browse?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}},\"browseId\":\"FEmusic_liked_playlists\"}") ("https://music.youtube.com/youtubei/v1/browse?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}},\"browseId\":\"VLLIBRARY\"}") ("https://music.youtube.com/youtubei/v1/browse?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}},\"browseId\":\"FEmusic_home\"}")) :prompts (("Playlist: " ("Incident Response Mix" "Focus Queue")) ("Shelf: " ("Recovery Picks")) ("Recovery Picks — pick: " ("Home Resilience — Home Artist" "Home Rollback — Fallback Band"))) :ipc ((1 ("loadfile" "https://music.youtube.com/watch?v=mitigation-song" "replace")) (2 ("loadfile" "https://music.youtube.com/watch?v=postmortem-song" "append")) (3 ("loadfile" "https://music.youtube.com/watch?v=home-resilience" "replace"))) :metadata (("mitigation-song" :title "Mitigation Song" :subtitle "Response Team") ("postmortem-song" :title "Postmortem λ" :subtitle "Learning Crew") ("home-resilience" :title "Home Resilience" :subtitle "Home Artist")) :messages ("youtube-music: fetching your playlists..." "youtube-music: loading playlist..." "youtube-music: queued 2 tracks" "youtube-music: fetching home..."))"####
    ]];
    ParityBatchCase::value(
        "browses_a_saved_playlist_and_home_recommendation",
        elisp_form,
        expect,
    )
}

fn reports_no_track_and_unavailable_mpv_without_mutating_the_queue() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "youtube-music-failure"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (socket (expand-file-name "mpv.sock" sandbox))
        (youtube-music-mpv-program "missing-mpv-parity")
        (youtube-music-mpv-mpris-search-paths nil)
        (youtube-music--socket-dir sandbox)
        (youtube-music--socket-path socket)
        (youtube-music--mpv-process nil)
        (youtube-music--ipc-process nil)
        (youtube-music--request-counter 0)
        (youtube-music--pending-requests (make-hash-table :test 'eql))
        (youtube-music--state
         '(:title nil :pause t :time-pos 0 :duration 0
           :playlist-pos -1 :path nil
           :loop-file "no" :loop-playlist "no"))
        (youtube-music--playlist-cache nil)
        (youtube-music--track-meta (make-hash-table :test 'equal))
        start-call no-track no-radio unavailable)
   (unwind-protect
       (progn
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory sandbox t)
         (cl-letf (((symbol-function 'process-live-p)
                    (lambda (&rest _) nil))
                   ((symbol-function 'start-process)
                    (lambda (name buffer program &rest arguments)
                      (setq start-call
                            (list name (buffer-name buffer) program arguments))
                      (signal
                       'file-missing
                       (list "Searching for program"
                             "No such file or directory"
                             program)))))
           (setq no-track
                 (condition-case error-data
                     (youtube-music-play-pause)
                   (error error-data)))
           (setq no-radio
                 (condition-case error-data
                     (youtube-music-radio)
                   (error error-data)))
           (setq unavailable
                 (condition-case error-data
                     (youtube-music-play-url
                      "https://music.youtube.com/watch?v=unavailable")
                   (error error-data)))
           (list :no-track no-track
                 :no-radio no-radio
                 :unavailable unavailable
                 :start start-call
                 :state youtube-music--state
                 :queue youtube-music--playlist-cache
                 :processes
                 (list youtube-music--mpv-process
                       youtube-music--ipc-process)
                 :request-counter youtube-music--request-counter
                 :pending (hash-table-count youtube-music--pending-requests)
                 :socket-exists (file-exists-p socket))))
     (when (get-buffer " *youtube-music-mpv*")
       (kill-buffer " *youtube-music-mpv*"))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:no-track (user-error "No track loaded — run ‘M-x youtube-music’ and search or pick from your library to start something") :no-radio (user-error "Cannot start radio: no track at point, and no playing track found (path=nil, playlist-pos=-1, queue-len=0)") :unavailable (file-missing "Searching for program" "No such file or directory" "missing-mpv-parity") :start ("youtube-music-mpv" " *youtube-music-mpv*" "missing-mpv-parity" ("--idle=yes" "--no-video" "--no-terminal" "--msg-level=all=warn" "--ytdl-format=bestaudio" "--input-ipc-server=[ORACLE-SANDBOX]/youtube-music-failure/mpv.sock")) :state (:title nil :pause t :time-pos 0 :duration 0 :playlist-pos -1 :path nil :loop-file "no" :loop-playlist "no") :queue nil :processes (nil nil) :request-counter 0 :pending 0 :socket-exists nil)"####
    ]];
    ParityBatchCase::value(
        "reports_no_track_and_unavailable_mpv_without_mutating_the_queue",
        elisp_form,
        expect,
    )
}

fn plays_the_paginated_library_and_rates_then_starts_radio() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((youtube-music--cookie "SAPISID=library-secret; SID=session")
        (youtube-music--sapisid "library-secret")
        (youtube-music--auth-state 'logged-in)
        (youtube-music--account-name "Library Listener")
        (youtube-music--auto-refresh-attempted nil)
        (youtube-music--shuffled-p nil)
        (youtube-music--mpv-process 'neomacs-melpa-youtube-music--mpv)
        (youtube-music--ipc-process 'neomacs-melpa-youtube-music--ipc)
        (youtube-music--request-counter 0)
        (youtube-music--pending-requests (make-hash-table :test 'eql))
        (youtube-music--ipc-buffer "")
        (neomacs-melpa-youtube-music--ipc-payloads nil)
        (neomacs-melpa-youtube-music--ipc-playlist-response nil)
        (youtube-music--state
         '(:title "Current" :pause nil :time-pos 0 :duration 180
           :playlist-pos 0
           :path "https://music.youtube.com/watch?v=liked-one"
           :loop-file "no" :loop-playlist "no"))
        (youtube-music--track-meta (make-hash-table :test 'equal))
        (youtube-music--liked-set nil)
        (youtube-music--disliked-set (make-hash-table :test 'equal))
        (neomacs-melpa-youtube-music--response-buffers nil)
        (liked-json
         "{\"responseContext\":{\"serviceTrackingParams\":[{\"service\":\"GFEEDBACK\",\"params\":[{\"key\":\"logged_in\",\"value\":\"1\"}]}]},\"contents\":{\"singleColumnBrowseResultsRenderer\":{\"tabs\":[{\"tabRenderer\":{\"content\":{\"sectionListRenderer\":{\"contents\":[{\"musicPlaylistShelfRenderer\":{\"contents\":[{\"musicResponsiveListItemRenderer\":{\"flexColumns\":[{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Liked One\"}]}}},{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Artist One\"}]}}}],\"playlistItemData\":{\"videoId\":\"liked-one\"}}},{\"musicResponsiveListItemRenderer\":{\"flexColumns\":[{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Liked Two λ\"}]}}},{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Artist Two\"}]}}}],\"playlistItemData\":{\"videoId\":\"liked-two\"}}}],\"continuations\":[{\"nextContinuationData\":{\"continuation\":\"liked-next λ\"}}]}}]}}}}]}}}")
        (continuation-json
         "{\"responseContext\":{\"serviceTrackingParams\":[{\"service\":\"GFEEDBACK\",\"params\":[{\"key\":\"logged_in\",\"value\":\"1\"}]}]},\"continuationContents\":{\"musicPlaylistShelfContinuation\":{\"contents\":[{\"musicResponsiveListItemRenderer\":{\"flexColumns\":[{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Liked Three\"}]}}},{\"musicResponsiveListItemFlexColumnRenderer\":{\"text\":{\"runs\":[{\"text\":\"Artist Three\"}]}}}],\"playlistItemData\":{\"videoId\":\"liked-three\"}}}]}}}")
        (radio-json
         "{\"responseContext\":{\"serviceTrackingParams\":[{\"service\":\"GFEEDBACK\",\"params\":[{\"key\":\"logged_in\",\"value\":\"1\"}]}]},\"contents\":{\"singleColumnMusicWatchNextResultsRenderer\":{\"tabbedRenderer\":{\"watchNextTabbedResultsRenderer\":{\"tabs\":[{\"tabRenderer\":{\"content\":{\"musicQueueRenderer\":{\"content\":{\"playlistPanelRenderer\":{\"contents\":[{\"playlistPanelVideoRenderer\":{\"title\":{\"runs\":[{\"text\":\"Radio Recovery\"}]},\"videoId\":\"radio-one\",\"longBylineText\":{\"runs\":[{\"text\":\"Resilience Band\"}]} }},{\"playlistPanelVideoWrapperRenderer\":{\"primaryRenderer\":{\"playlistPanelVideoRenderer\":{\"title\":{\"runs\":[{\"text\":\"Radio Rollback λ\"}]},\"videoId\":\"radio-two\",\"longBylineText\":{\"runs\":[{\"text\":\"Incident Crew\"}]}}}}}]}}}}}}]}}}}}")
        requests messages rate-states)
   (unwind-protect
       (cl-letf (((symbol-function 'float-time)
                  (lambda (&rest _) 1722643202.0))
                 ((symbol-function 'url-retrieve)
                  (lambda (url callback &rest _)
                    (let* ((body (decode-coding-string url-request-data 'utf-8))
                           (response
                            (cond
                             ((string-match-p "continuation=" url)
                              continuation-json)
                             ((string-match-p "FEmusic_liked_videos" body)
                              liked-json)
                             ((string-match-p "/next?" url) radio-json)
                             ((string-match-p "/like/" url)
                              "{\"responseContext\":{\"serviceTrackingParams\":[{\"service\":\"GFEEDBACK\",\"params\":[{\"key\":\"logged_in\",\"value\":\"1\"}]}]}}")
                             (t (error "Unexpected library request: %s" url))))
                           (buffer
                            (neomacs-melpa-youtube-music--response-buffer
                             response)))
                      (push (list url body) requests)
                      (with-current-buffer buffer
                        (funcall callback nil))
                      buffer)))
                 ((symbol-function 'process-live-p)
                  #'neomacs-melpa-youtube-music--process-live-p)
                 ((symbol-function 'process-send-string)
                  #'neomacs-melpa-youtube-music--process-send-string)
                 ((symbol-function 'message)
                  (lambda (format-string &rest arguments)
                    (let ((text (apply #'format format-string arguments)))
                      (push text messages)
                      text))))
         (youtube-music-liked)
         (push (list 'library
                     (neomacs-melpa-youtube-music--hash-keys
                      youtube-music--liked-set)
                     (neomacs-melpa-youtube-music--hash-keys
                      youtube-music--disliked-set))
               rate-states)
         (youtube-music-dislike)
         (push (list 'dislike
                     (neomacs-melpa-youtube-music--hash-keys
                      youtube-music--liked-set)
                     (neomacs-melpa-youtube-music--hash-keys
                      youtube-music--disliked-set))
               rate-states)
         (youtube-music-like)
         (push (list 'like
                     (neomacs-melpa-youtube-music--hash-keys
                      youtube-music--liked-set)
                     (neomacs-melpa-youtube-music--hash-keys
                      youtube-music--disliked-set))
               rate-states)
         (youtube-music-unrate)
         (push (list 'unrate
                     (neomacs-melpa-youtube-music--hash-keys
                      youtube-music--liked-set)
                     (neomacs-melpa-youtube-music--hash-keys
                      youtube-music--disliked-set))
               rate-states)
         (youtube-music-radio)
         (list :requests (nreverse requests)
               :ipc (nreverse neomacs-melpa-youtube-music--ipc-payloads)
               :ratings (nreverse rate-states)
               :metadata
               (mapcar
                (lambda (video-id)
                  (cons video-id (gethash video-id youtube-music--track-meta)))
                '("liked-one" "liked-two" "liked-three"
                  "radio-one" "radio-two"))
               :messages (nreverse messages)))
     (neomacs-melpa-youtube-music--cleanup-response-buffers))))
"####;
    let expect = expect![[
        r####"OK (:requests (("https://music.youtube.com/youtubei/v1/browse?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}},\"browseId\":\"FEmusic_liked_videos\"}") ("https://music.youtube.com/youtubei/v1/browse?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json&ctoken=liked-next%20%CE%BB&continuation=liked-next%20%CE%BB&type=next" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}},\"browseId\":\"FEmusic_liked_videos\"}") ("https://music.youtube.com/youtubei/v1/like/dislike?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}},\"target\":{\"videoId\":\"liked-one\"}}") ("https://music.youtube.com/youtubei/v1/like/like?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}},\"target\":{\"videoId\":\"liked-one\"}}") ("https://music.youtube.com/youtubei/v1/like/removelike?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}},\"target\":{\"videoId\":\"liked-one\"}}") ("https://music.youtube.com/youtubei/v1/next?key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30&prettyPrint=false&alt=json" "{\"context\":{\"client\":{\"clientName\":\"WEB_REMIX\",\"clientVersion\":\"1.20250101.01.00\",\"hl\":\"en\",\"gl\":\"US\"},\"user\":{}},\"videoId\":\"liked-one\",\"playlistId\":\"RDAMVMliked-one\"}")) :ipc ((1 ("loadfile" "https://music.youtube.com/watch?v=liked-one" "replace")) (2 ("loadfile" "https://music.youtube.com/watch?v=liked-two" "append")) (3 ("loadfile" "https://music.youtube.com/watch?v=liked-three" "append")) (4 ("loadfile" "https://music.youtube.com/watch?v=radio-one" "replace")) (5 ("loadfile" "https://music.youtube.com/watch?v=radio-two" "append"))) :ratings ((library ("liked-one" "liked-three" "liked-two") nil) (dislike ("liked-three" "liked-two") ("liked-one")) (like ("liked-one" "liked-three" "liked-two") nil) (unrate ("liked-three" "liked-two") nil)) :metadata (("liked-one" :title "Liked One" :subtitle "Artist One") ("liked-two" :title "Liked Two λ" :subtitle "Artist Two") ("liked-three" :title "Liked Three" :subtitle "Artist Three") ("radio-one" :title "Radio Recovery" :subtitle "Resilience Band") ("radio-two" :title "Radio Rollback λ" :subtitle "Incident Crew")) :messages ("youtube-music: fetching liked songs..." "youtube-music: queued 3 tracks" "youtube-music: disliked \"Liked One — Artist One\"" "youtube-music: liked \"Liked One — Artist One\"" "youtube-music: unrated \"Liked One — Artist One\"" "youtube-music: starting radio..." "youtube-music: radio queued (2 tracks)"))"####
    ]];
    ParityBatchCase::value(
        "plays_the_paginated_library_and_rates_then_starts_radio",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        renders_the_real_status_buffer_and_routes_playback_controls(),
        searches_unicode_songs_and_enqueues_a_resolved_playlist(),
        logs_in_from_a_pasted_cookie_and_logs_out_cleanly(),
        browses_a_saved_playlist_and_home_recommendation(),
        reports_no_track_and_unavailable_mpv_without_mutating_the_queue(),
        plays_the_paginated_library_and_rates_then_starts_radio(),
    ]
}
