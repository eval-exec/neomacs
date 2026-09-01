use expect_test::expect;

use super::ParityBatchCase;

fn searches_unicode_results_renders_rows_and_starts_external_playback() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "ytdious-search"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (curl (expand-file-name "curl" sandbox))
        (calls-file (expand-file-name "calls.log" sandbox))
        (exec-path (cons sandbox exec-path))
        (ytdious-invidious-api-url "https://invidious.example")
        (ytdious-player-external t)
        (ytdious-player-external-command "mpv-parity")
        (ytdious-player-external-options "--profile=release-parity")
        (ytdious-published-date-time-string "%Y-%m-%d")
        thumbnail-urls process-calls buffer result)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory sandbox t)
         (neomacs-melpa-ytdious--write-executable
          curl
          "printf '%s\\n' \"$*\" >> \"$NEOMACS_TEST_SANDBOX_ROOT/ytdious-search/calls.log\"\nprintf '%s\\n' '[{\"author\":\"Release Engineering λ\",\"lengthSeconds\":3723,\"title\":\"Resilient deploy — café\",\"videoId\":\"video-λ-1\",\"authorId\":\"channel-release\",\"viewCount\":120034,\"published\":1722513600},{\"author\":\"Incident Review\",\"lengthSeconds\":95,\"title\":\"Rollback in practice\",\"videoId\":\"video-2\",\"authorId\":\"channel-incident\",\"viewCount\":808,\"published\":1719835200}]'\n")
         (cl-letf (((symbol-function 'url-retrieve)
                    (lambda (url &rest _)
                      (push url thumbnail-urls)
                      nil))
                   ((symbol-function 'start-process)
                    (lambda (name buffer-name program &rest arguments)
                      (push
                       (list name buffer-name program arguments)
                       process-calls)
                      'ytdious-player-process)))
           (switch-to-buffer (ytdious-buffer))
           (ytdious-mode)
           (ytdious-search "resilience λ date:week")
           (setq buffer (current-buffer))
           (goto-char (point-min))
           (let ((current (ytdious-get-current-video)))
             (ytdious-play)
             (setq result
                   (list
                    :buffer (buffer-name)
                    :text (buffer-substring-no-properties
                           (point-min) (point-max))
                    :mode
                    (list major-mode buffer-read-only
                          revert-buffer-function
                          (lookup-key ytdious-mode-map (kbd "RET"))
                          (lookup-key ytdious-mode-map (kbd ">"))
                          (lookup-key ytdious-mode-map (kbd "C-<return>")))
                    :criteria
                    (list ytdious-search-term ytdious-date-criterion
                          ytdious-sort-criterion ytdious-current-page
                          ytdious-channel)
                    :columns
                    (mapcar #'car (append tabulated-list-format nil))
                    :entries
                    (mapcar #'neomacs-melpa-ytdious--entry-state
                            tabulated-list-entries)
                    :current current
                    :mode-line
                    (neomacs-melpa-ytdious--mode-line-state)
                    :curl
                    (neomacs-melpa-ytdious--file-lines calls-file)
                    :thumbnails (nreverse thumbnail-urls)
                    :player (nreverse process-calls)))))
         result)
     (when (buffer-live-p buffer)
       (kill-buffer buffer))
     (when (get-buffer "*ytdious*")
       (kill-buffer "*ytdious*"))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:buffer #("ytdious [SRCH: resilience λ]" 8 28 (face ytdious-video-published-face)) :text "2024-08-01 Release Engineering λ 01:02:03 Resilient deploy — café                                                                                  120034\n2024-07-01 Incident Review      00:01:35 Rollback in practice                                                                                        808\n" :mode (ytdious-mode t ytdious--draw-buffer ytdious-play ytdious-search-next-page ytdious-play-continious) :criteria ("resilience λ" week relevance 1 nil) :columns ("Date" "Author" "Length" "Title" "Views") :entries (("video-λ-1" ("2024-08-01" "Release Engineering λ" "01:02:03" "Resilient deploy — café" "120034") (ytdious-video-published-face ytdious-channel-name-face ytdious-video-length-face nil ytdious-video-view-face)) ("video-2" ("2024-07-01" "Incident Review" "00:01:35" "Rollback in practice" "808") (ytdious-video-published-face ytdious-channel-name-face ytdious-video-length-face nil ytdious-video-view-face))) :current ((author . "Release Engineering λ") (lengthSeconds . 3723) (title . "Resilient deploy — café") (videoId . "video-λ-1") (authorId . "channel-release") (viewCount . 120034) (published . 1722513600)) :mode-line ((("page:" nil) ("1" ytdious-video-published-face)) ((" date:" nil) ("week" ytdious-video-published-face)) ((" sort:" nil) ("relevance" ytdious-video-published-face))) :curl ("--silent -X GET https://invidious.example/api/v1/search?q=resilience%20%CE%BB&date=week&sort_by=relevance&page=1&fields=author,lengthSeconds,title,videoId,authorId,viewCount,published") :thumbnails ("https://invidious.example/vi/video-λ-1/mqdefault.jpg") :player (("ytdious player" "ytdious player" "mpv-parity" ("--profile=release-parity" "https://invidious.example/watch?v=video-λ-1"))))"####
    ]];
    ParityBatchCase::value(
        "searches_unicode_results_renders_rows_and_starts_external_playback",
        elisp_form,
        expect,
    )
}

fn searches_a_region_then_pages_sorts_and_opens_the_result_channel() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "ytdious-navigation"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (curl (expand-file-name "curl" sandbox))
        (calls-file (expand-file-name "calls.log" sandbox))
        (exec-path (cons sandbox exec-path))
        (ytdious-invidious-api-url "https://invidious.example")
        (ytdious-published-date-time-string "%Y-%m-%d")
        thumbnail-urls minibuffer-prompts source-buffer
        result-buffer states)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory sandbox t)
         (neomacs-melpa-ytdious--write-executable
          curl
          "printf '%s\\n' \"$*\" >> \"$NEOMACS_TEST_SANDBOX_ROOT/ytdious-navigation/calls.log\"\ncase \"$*\" in\n  *'/channels/videos/'*)\n    printf '%s\\n' '[{\"author\":\"Release Engineering λ\",\"lengthSeconds\":600,\"title\":\"Channel postmortem\",\"videoId\":\"channel-video\",\"authorId\":\"channel-release\",\"viewCount\":33,\"published\":1722513600}]'\n    ;;\n  *)\n    printf '%s\\n' '[{\"author\":\"Release Engineering λ\",\"lengthSeconds\":3723,\"title\":\"Resilient deploy\",\"videoId\":\"video-1\",\"authorId\":\"channel-release\",\"viewCount\":120034,\"published\":1722513600},{\"author\":\"Incident Review\",\"lengthSeconds\":95,\"title\":\"Rollback practice\",\"videoId\":\"video-2\",\"authorId\":\"channel-incident\",\"viewCount\":808,\"published\":1719835200}]'\n    ;;\nesac\n")
         (setq source-buffer (generate-new-buffer "*ytdious region source*"))
         (with-current-buffer source-buffer
           (insert "incident response λ date:month\nunselected notes")
           (goto-char (point-min))
           (set-mark (line-end-position))
           (activate-mark))
         (cl-letf (((symbol-function 'url-retrieve)
                    (lambda (url &rest _)
                      (push url thumbnail-urls)
                      nil))
                   ((symbol-function 'read-from-minibuffer)
                    (lambda (prompt initial &rest _)
                      (push (list prompt initial) minibuffer-prompts)
                      initial)))
           (switch-to-buffer source-buffer)
           (ytdious-region-search)
           (setq result-buffer (current-buffer))
           (push
            (list 'region
                  ytdious-search-term ytdious-date-criterion
                  ytdious-sort-criterion ytdious-current-page
                  ytdious-channel
                  (mapcar #'car tabulated-list-entries))
            states)
           (ytdious-search "incident response λ date:month")
           (push
            (list 'search-reset
                  ytdious-search-term ytdious-date-criterion
                  ytdious-sort-criterion ytdious-current-page
                  ytdious-channel
                  (mapcar #'car tabulated-list-entries))
            states)
           (ytdious-search-next-page)
           (push
            (list 'next ytdious-current-page
                  (mapcar #'car tabulated-list-entries))
            states)
           (ytdious-search-previous-page)
           (push (list 'previous ytdious-current-page) states)
           (ytdious-rotate-sort)
           (push (list 'sort-forward ytdious-sort-criterion) states)
           (ytdious-rotate-sort-backwards)
           (push (list 'sort-backward ytdious-sort-criterion) states)
           (ytdious-rotate-date)
           (push (list 'date-forward ytdious-date-criterion) states)
           (ytdious-rotate-date-backwards)
           (push (list 'date-backward ytdious-date-criterion) states)
           (ytdious-toggle-sort-direction)
           (push
            (list 'reverse ytdious-sort-reverse
                  (mapcar #'car tabulated-list-entries))
            states)
           (goto-char (point-min))
           (ytdious-view-channel-at-point)
           (push
            (list 'channel ytdious-channel
                  (buffer-name)
                  (mapcar #'car tabulated-list-entries))
            states)
           (ytdious-search-recent)
           (push
            (list 'recent ytdious-search-term ytdious-channel
                  ytdious-current-page (buffer-name))
            states)
           (list
            :states (nreverse states)
            :minibuffer (nreverse minibuffer-prompts)
            :curl (neomacs-melpa-ytdious--file-lines calls-file)
            :thumbnails (nreverse thumbnail-urls)
            :source
            (with-current-buffer source-buffer
              (list (buffer-string) (region-beginning) (region-end))))))
     (when (buffer-live-p source-buffer)
       (kill-buffer source-buffer))
     (when (buffer-live-p result-buffer)
       (kill-buffer result-buffer))
     (when (get-buffer "*ytdious*")
       (kill-buffer "*ytdious*"))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:states ((region "incident response λ" month relevance 1 nil ("video-1" "video-2")) (search-reset "incident response λ" month relevance 1 nil ("video-1" "video-2")) (next 2 ("video-1" "video-2")) (previous 1) (sort-forward rating) (sort-backward relevance) (date-forward year) (date-backward month) (reverse t ("video-2" "video-1")) (channel "channel-incident" #("ytdious [CHAN: Release Engineering λ]" 8 37 (face ytdious-video-published-face)) ("channel-video")) (recent "incident response λ" nil 1 #("ytdious [SRCH: incident response λ]" 8 35 (face ytdious-video-published-face)))) :minibuffer (("Search terms: " "incident response λ")) :curl ("--silent -X GET https://invidious.example/api/v1/search?q=incident%20response%20%CE%BB&date=month&sort_by=relevance&page=1&fields=author,lengthSeconds,title,videoId,authorId,viewCount,published" "--silent -X GET https://invidious.example/api/v1/search?q=incident%20response%20%CE%BB&date=month&sort_by=relevance&page=1&fields=author,lengthSeconds,title,videoId,authorId,viewCount,published" "--silent -X GET https://invidious.example/api/v1/search?q=incident%20response%20%CE%BB&date=month&sort_by=relevance&page=2&fields=author,lengthSeconds,title,videoId,authorId,viewCount,published" "--silent -X GET https://invidious.example/api/v1/search?q=incident%20response%20%CE%BB&date=month&sort_by=relevance&page=1&fields=author,lengthSeconds,title,videoId,authorId,viewCount,published" "--silent -X GET https://invidious.example/api/v1/search?q=incident%20response%20%CE%BB&date=month&sort_by=rating&page=1&fields=author,lengthSeconds,title,videoId,authorId,viewCount,published" "--silent -X GET https://invidious.example/api/v1/search?q=incident%20response%20%CE%BB&date=month&sort_by=relevance&page=1&fields=author,lengthSeconds,title,videoId,authorId,viewCount,published" "--silent -X GET https://invidious.example/api/v1/search?q=incident%20response%20%CE%BB&date=year&sort_by=relevance&page=1&fields=author,lengthSeconds,title,videoId,authorId,viewCount,published" "--silent -X GET https://invidious.example/api/v1/search?q=incident%20response%20%CE%BB&date=month&sort_by=relevance&page=1&fields=author,lengthSeconds,title,videoId,authorId,viewCount,published" "--silent -X GET https://invidious.example/api/v1/channels/videos/channel-incident?sort_by=newest" "--silent -X GET https://invidious.example/api/v1/search?q=incident%20response%20%CE%BB&date=all&sort_by=relevance&page=1&fields=author,lengthSeconds,title,videoId,authorId,viewCount,published") :thumbnails ("https://invidious.example/vi/video-1/mqdefault.jpg" "https://invidious.example/vi/video-1/mqdefault.jpg" "https://invidious.example/vi/video-1/mqdefault.jpg" "https://invidious.example/vi/video-1/mqdefault.jpg" "https://invidious.example/vi/video-1/mqdefault.jpg" "https://invidious.example/vi/video-1/mqdefault.jpg" "https://invidious.example/vi/video-1/mqdefault.jpg" "https://invidious.example/vi/video-1/mqdefault.jpg" "https://invidious.example/vi/video-2/mqdefault.jpg" "https://invidious.example/vi/channel-video/mqdefault.jpg" "https://invidious.example/vi/video-2/mqdefault.jpg") :source ("incident response λ date:month\nunselected notes" 1 31))"####
    ]];
    ParityBatchCase::value(
        "searches_a_region_then_pages_sorts_and_opens_the_result_channel",
        elisp_form,
        expect,
    )
}

fn renders_a_thumbnail_detail_popup_and_cleans_the_response_buffer() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "ytdious-thumbnail"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (curl (expand-file-name "curl" sandbox))
        (exec-path (cons sandbox exec-path))
        (ytdious-invidious-api-url "https://invidious.example")
        (ytdious-published-date-time-string "%Y-%m-%d")
        (kill-ring nil)
        (kill-ring-yank-pointer nil)
        (interprogram-cut-function nil)
        response-buffer popup-buffer search-buffer
        image-calls insert-calls display-calls)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory sandbox t)
         (neomacs-melpa-ytdious--write-executable
          curl
          "printf '%s\\n' '[{\"author\":\"Visual Ops\",\"lengthSeconds\":125,\"title\":\"Visual incident λ\",\"videoId\":\"visual-video\",\"authorId\":\"visual-channel\",\"viewCount\":42,\"published\":1722513600}]'\n")
         (cl-letf (((symbol-function 'url-retrieve)
                    (lambda (_url callback callback-arguments &rest _)
                      (setq response-buffer
                            (generate-new-buffer
                             " *ytdious thumbnail response*"))
                      (with-current-buffer response-buffer
                        (insert
                         "HTTP/1.1 200 OK\nContent-Type: image/jpeg\n\nJPEG-PARITY-λ")
                        (goto-char (point-min))
                        (apply callback nil callback-arguments))
                      response-buffer))
                   ((symbol-function 'create-image)
                    (lambda (data type data-p &rest properties)
                      (push (list data type data-p properties) image-calls)
                      '(:type parity-thumbnail)))
                   ((symbol-function 'insert-image)
                    (lambda (image &rest _)
                      (push image insert-calls)
                      (insert "[thumbnail λ]")))
                   ((symbol-function 'display-buffer-pop-up-window)
                    (lambda (buffer alist)
                      (push (list (buffer-name buffer) alist) display-calls)
                      nil)))
           (setq search-buffer
                 (get-buffer-create "*ytdious thumbnail search*"))
           (with-current-buffer search-buffer
             (ytdious-mode)
             (ytdious-search "visual incident"))
           (setq popup-buffer (get-buffer "ytdious: Video Details"))
           (list
            :popup
            (with-current-buffer popup-buffer
              (list (buffer-string) major-mode buffer-read-only))
            :image (nreverse image-calls)
            :insert (nreverse insert-calls)
            :display (nreverse display-calls)
            :response-live (buffer-live-p response-buffer)
            :search-buffer (buffer-name search-buffer))))
     (when (buffer-live-p response-buffer)
       (kill-buffer response-buffer))
     (when (buffer-live-p popup-buffer)
       (kill-buffer popup-buffer))
     (when (buffer-live-p search-buffer)
       (kill-buffer search-buffer))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:popup ("\nVisual incident λ\n\n[thumbnail λ]" help-mode t) :image (("JPEG-PARITY-λ" nil t nil)) :insert ((:type parity-thumbnail)) :display (("ytdious: Video Details" nil)) :response-live nil :search-buffer #("ytdious [SRCH: visual incident]" 8 31 (face ytdious-video-published-face)))"####
    ]];
    ParityBatchCase::value(
        "renders_a_thumbnail_detail_popup_and_cleans_the_response_buffer",
        elisp_form,
        expect,
    )
}

fn surfaces_transport_and_malformed_json_failures_without_result_rows() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "ytdious-failures"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (curl (expand-file-name "curl" sandbox))
        (calls-file (expand-file-name "calls.log" sandbox))
        (exec-path (cons sandbox exec-path))
        (ytdious-invidious-api-url "https://invidious.example")
        transport malformed buffers)
   (unwind-protect
       (save-window-excursion
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory sandbox t)
         (neomacs-melpa-ytdious--write-executable
          curl
          "printf '%s\\n' \"$*\" >> \"$NEOMACS_TEST_SANDBOX_ROOT/ytdious-failures/calls.log\"\ncase \"$*\" in\n  *transport-failure*) exit 7 ;;\n  *malformed-response*) printf '%s\\n' '{broken-json' ;;\n  *) printf '%s\\n' '[]' ;;\nesac\n")
         (cl-letf (((symbol-function 'url-retrieve)
                    (lambda (&rest _)
                      (error
                       "Failed searches must not request thumbnails"))))
           (let ((buffer
                  (generate-new-buffer "*ytdious transport failure*")))
             (push buffer buffers)
             (with-current-buffer buffer
               (ytdious-mode)
               (setq transport
                     (condition-case error-data
                         (ytdious-search "transport-failure")
                       (error error-data)))))
           (let ((buffer
                  (generate-new-buffer "*ytdious malformed failure*")))
             (push buffer buffers)
             (with-current-buffer buffer
               (ytdious-mode)
               (setq malformed
                     (condition-case error-data
                         (ytdious-search "malformed-response")
                       (error error-data)))))
           (list
            :transport transport
            :malformed malformed
            :buffers
            (mapcar
             (lambda (buffer)
               (with-current-buffer buffer
                 (list
                  (buffer-name)
                  (buffer-substring-no-properties
                   (point-min) (point-max))
                  tabulated-list-entries)))
             (reverse buffers))
            :curl (neomacs-melpa-ytdious--file-lines calls-file))))
     (dolist (buffer buffers)
       (when (buffer-live-p buffer)
         (kill-buffer buffer)))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r####"OK (:transport (error "Curl had problems connecting to Invidious") :malformed (json-string-format 10) :buffers (("*ytdious transport failure*" "" nil) ("*ytdious malformed failure*" "" nil)) :curl ("--silent -X GET https://invidious.example/api/v1/search?q=transport-failure&date=all&sort_by=relevance&page=1&fields=author,lengthSeconds,title,videoId,authorId,viewCount,published" "--silent -X GET https://invidious.example/api/v1/search?q=malformed-response&date=all&sort_by=relevance&page=1&fields=author,lengthSeconds,title,videoId,authorId,viewCount,published"))"####
    ]];
    ParityBatchCase::value(
        "surfaces_transport_and_malformed_json_failures_without_result_rows",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        searches_unicode_results_renders_rows_and_starts_external_playback(),
        searches_a_region_then_pages_sorts_and_opens_the_result_channel(),
        renders_a_thumbnail_detail_popup_and_cleans_the_response_buffer(),
        surfaces_transport_and_malformed_json_failures_without_result_rows(),
    ]
}
