use expect_test::expect;

use super::ParityBatchCase;

fn signs_and_submits_a_real_translation_request_with_history() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((youdao-dictionary-app-key "parity-app")
        (youdao-dictionary-secret-key "parity-secret")
        (youdao-dictionary-from "en")
        (youdao-dictionary-to "zh-CHS")
        (youdao-dictionary-buffer-name "*Youdao signed request parity*")
        (history-file (make-temp-file "youdao-parity-history-"))
        (youdao-dictionary-search-history-file history-file)
        (neomacs-melpa-youdao--response-buffers nil)
        (url-http-response-status 200)
        request history rendered)
   (unwind-protect
       (save-window-excursion
         (cl-letf (((symbol-function 'youdao-dictionary-get-salt)
                    (lambda () "314"))
                   ((symbol-function 'youdao-dictionary-get-curtime)
                    (lambda () "1722643200"))
                   ((symbol-function 'url-retrieve-synchronously)
                    (lambda (url &rest _)
                      (setq request
                            (list :url url
                                  :method url-request-method
                                  :data url-request-data
                                  :headers url-request-extra-headers))
                      (neomacs-melpa-youdao--response-buffer
                       "{\"errorCode\":\"0\",\"query\":\"reliability λ\",\"translation\":[\"可靠性\",\"可信度\"],\"basic\":{\"phonetic\":\"rɪˌlaɪəˈbɪləti\",\"explains\":[\"n. 可靠性\"]},\"web\":[{\"key\":\"site reliability\",\"value\":[\"站点可靠性\",\"SRE\"]}]}"))))
           (youdao-dictionary-search "reliability λ")
           (with-current-buffer youdao-dictionary-buffer-name
             (setq rendered
                   (list (buffer-substring-no-properties (point-min) (point-max))
                         (point) major-mode buffer-read-only
                         youdao-dictionary-current-buffer-word)))
           (setq history
                 (with-temp-buffer
                   (insert-file-contents history-file)
                   (buffer-string)))
           (list :request request :history history :rendered rendered)))
     (when (file-exists-p history-file)
       (delete-file history-file))
     (when (get-buffer youdao-dictionary-buffer-name)
       (kill-buffer youdao-dictionary-buffer-name))
     (neomacs-melpa-youdao--cleanup-response-buffers))))
"####;
    let expect = expect![[
        r####"OK (:request (:url "https://openapi.youdao.com/api" :method "POST" :data "q=reliability%20%CE%BB&from=en&to=zh-CHS&appKey=parity-app&salt=314&sign=f15b33f51a0c1250a5be90b9bbb3853d7154eaf875e41c468835030f7bdc4402&signType=v3&curtime=1722643200" :headers (("Content-Type" . "application/x-www-form-urlencoded"))) :history "reliability λ\n" :rendered ("reliability λ [rɪˌlaɪəˈbɪləti]\n\n* Basic Explains\n- n. 可靠性\n\n* Web References\n- site reliability :: 站点可靠性; SRE\n" 1 youdao-dictionary-mode t "reliability λ"))"####
    ]];
    ParityBatchCase::value(
        "signs_and_submits_a_real_translation_request_with_history",
        elisp_form,
        expect,
    )
}

fn renders_sync_and_async_lookups_in_the_dictionary_buffer() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((youdao-dictionary-app-key "parity-app")
        (youdao-dictionary-secret-key "parity-secret")
        (youdao-dictionary-buffer-name "*Youdao parity result*")
        (neomacs-melpa-youdao--response-buffers nil)
        (url-http-response-status 200)
        (responses
         (list
          "{\"errorCode\":\"0\",\"query\":\"resilience\",\"translation\":[\"韧性\"],\"basic\":{\"phonetic\":\"rɪˈzɪliəns\",\"explains\":[\"n. 韧性\",\"n. 恢复力\"]},\"web\":[{\"key\":\"system resilience\",\"value\":[\"系统韧性\",\"系统恢复力\"]}]}"
          "{\"errorCode\":\"0\",\"query\":\"发布检查清单\",\"translation\":[\"release checklist\",\"deployment checklist\"]}"))
        requests sync-state async-state)
   (unwind-protect
       (save-window-excursion
         (cl-letf (((symbol-function 'youdao-dictionary-get-salt)
                    (lambda () "271"))
                   ((symbol-function 'youdao-dictionary-get-curtime)
                    (lambda () "1722643201"))
                   ((symbol-function 'url-retrieve-synchronously)
                    (lambda (url &rest _)
                      (push (list :sync url url-request-data) requests)
                      (neomacs-melpa-youdao--response-buffer (pop responses))))
                   ((symbol-function 'url-retrieve)
                    (lambda (url callback &rest _)
                      (push (list :async url url-request-data) requests)
                      (let ((buffer
                             (neomacs-melpa-youdao--response-buffer
                              (pop responses))))
                        (with-current-buffer buffer
                          (funcall callback nil))
                        buffer))))
           (youdao-dictionary-search "resilience")
           (with-current-buffer youdao-dictionary-buffer-name
             (setq sync-state
                   (list
                    :text (buffer-substring-no-properties (point-min) (point-max))
                    :point (point)
                    :mode major-mode
                    :read-only buffer-read-only
                    :word youdao-dictionary-current-buffer-word
                    :keys
                    (list (lookup-key youdao-dictionary-mode-map (kbd "q"))
                          (lookup-key youdao-dictionary-mode-map (kbd "p"))
                          (lookup-key youdao-dictionary-mode-map (kbd "y"))))))
           (youdao-dictionary-search-async "发布检查清单")
           (with-current-buffer youdao-dictionary-buffer-name
             (setq async-state
                   (list
                    :text (buffer-substring-no-properties (point-min) (point-max))
                    :point (point)
                    :mode major-mode
                    :read-only buffer-read-only
                    :word youdao-dictionary-current-buffer-word)))
           (list :sync sync-state
                 :async async-state
                 :requests (nreverse requests))))
     (when (get-buffer youdao-dictionary-buffer-name)
       (kill-buffer youdao-dictionary-buffer-name))
     (neomacs-melpa-youdao--cleanup-response-buffers))))
"####;
    let expect = expect![[
        r####"OK (:sync (:text "resilience [rɪˈzɪliəns]\n\n* Basic Explains\n- n. 韧性\n- n. 恢复力\n\n* Web References\n- system resilience :: 系统韧性; 系统恢复力\n" :point 1 :mode youdao-dictionary-mode :read-only t :word "resilience" :keys (quit-window youdao-dictionary-play-voice-of-current-word youdao-dictionary-play-voice-at-point)) :async (:text "发布检查清单\n\n* Translation\n- release checklist\n- deployment checklist\n" :point 1 :mode youdao-dictionary-mode :read-only t :word "发布检查清单") :requests ((:sync "https://openapi.youdao.com/api" "q=resilience&from=auto&to=auto&appKey=parity-app&salt=271&sign=8ac86547de06ae75f8485bb2c0aa9fcd5ff7250dfac43216475f64fc204f8695&signType=v3&curtime=1722643201") (:async "https://openapi.youdao.com/api" "q=%E5%8F%91%E5%B8%83%E6%A3%80%E6%9F%A5%E6%B8%85%E5%8D%95&from=auto&to=auto&appKey=parity-app&salt=271&sign=f1a6cf2659bc4e4bef73268da17c4541aa81183ecdb9ff407fe77f38d21a6598&signType=v3&curtime=1722643201")))"####
    ]];
    ParityBatchCase::value(
        "renders_sync_and_async_lookups_in_the_dictionary_buffer",
        elisp_form,
        expect,
    )
}

fn replaces_a_region_and_word_from_real_dictionary_choices() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let ((youdao-dictionary-app-key "parity-app")
       (youdao-dictionary-secret-key "parity-secret")
       (url-http-response-status 200)
       (response
        "{\"errorCode\":\"0\",\"query\":\"reliable\",\"translation\":[\"可靠的\"],\"basic\":{\"phonetic\":\"rɪˈlaɪəbəl\",\"explains\":[\"[计] dependable\",\"adj. reliable\",\"n. resilience\"]}}")
       (choice-number 0)
       (kill-ring nil)
       (kill-ring-yank-pointer nil)
       (neomacs-melpa-youdao--response-buffers nil)
       menus requests region-state word-state)
   (unwind-protect
       (cl-letf (((symbol-function 'youdao-dictionary-get-salt)
              (lambda () "161"))
             ((symbol-function 'youdao-dictionary-get-curtime)
              (lambda () "1722643202"))
             ((symbol-function 'url-retrieve-synchronously)
              (lambda (url &rest _)
                (push (list url url-request-data) requests)
                (neomacs-melpa-youdao--response-buffer response)))
             ((symbol-function 'popup-menu*)
              (lambda (items &rest _)
                (push items menus)
                (setq choice-number (1+ choice-number))
                (if (= choice-number 1)
                    (car items)
                  (nth 2 items)))))
     (with-temp-buffer
       (insert "The release is reliable for production.")
       (goto-char (point-min))
       (search-forward "reliable")
       (set-mark (match-beginning 0))
       (goto-char (match-end 0))
       (let ((transient-mark-mode t)
             (mark-active t))
         (youdao-dictionary-search-and-replace))
       (setq region-state
             (list (buffer-string) (point) (mark) mark-active)))
     (with-temp-buffer
       (insert "Ship reliable builds every day.")
       (goto-char (point-min))
       (search-forward "reliable")
       (backward-char 3)
       (youdao-dictionary-search-and-replace)
       (setq word-state (list (buffer-string) (point))))
     (list :region region-state
           :word word-state
           :menus (nreverse menus)
           :requests (nreverse requests)))
     (neomacs-melpa-youdao--cleanup-response-buffers))))
"####;
    let expect = expect![[
        r####"OK (:region ("The release is dependable for production." 26 16 t) :word ("Ship resilienceble builds every day." 16) :menus (("dependable" "adj. reliable" "n. resilience") ("dependable" "adj. reliable" "n. resilience")) :requests (("https://openapi.youdao.com/api" "q=reliable&from=auto&to=auto&appKey=parity-app&salt=161&sign=de9eed61d605caec6624f194d1ee2d5db6494deb7776da09b4eea98c6c451b14&signType=v3&curtime=1722643202") ("https://openapi.youdao.com/api" "q=reliable&from=auto&to=auto&appKey=parity-app&salt=161&sign=de9eed61d605caec6624f194d1ee2d5db6494deb7776da09b4eea98c6c451b14&signType=v3&curtime=1722643202")))"####
    ]];
    ParityBatchCase::value(
        "replaces_a_region_and_word_from_real_dictionary_choices",
        elisp_form,
        expect,
    )
}

fn presents_a_lookup_through_popup_tooltip_and_posframe() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((youdao-dictionary-app-key "parity-app")
        (youdao-dictionary-secret-key "parity-secret")
        (youdao-dictionary-buffer-name "*Youdao presentation parity*")
        (neomacs-melpa-youdao--response-buffers nil)
        (response
         "{\"errorCode\":\"0\",\"query\":\"reliability\",\"translation\":[\"可靠性\"],\"basic\":{\"phonetic\":\"rɪˌlaɪəˈbɪləti\",\"explains\":[\"n. 可靠性\",\"n. 可信度\"]},\"web\":[]}")
        (events (list ?t ?p))
        (unread-command-events nil)
        (had-posframe (featurep 'posframe))
        popup-state tooltip-show tooltip-hide
        posframe-show posframe-delete other-frame-calls requests)
   (unwind-protect
       (progn
         (provide 'posframe)
         (cl-letf (((symbol-function 'youdao-dictionary-get-salt)
                    (lambda () "919"))
                   ((symbol-function 'youdao-dictionary-get-curtime)
                    (lambda () "1722643204"))
                   ((symbol-function 'url-retrieve-synchronously)
                    (lambda (url &rest _)
                      (push (list url url-request-data) requests)
                      (neomacs-melpa-youdao--response-buffer response)))
                   ((symbol-function 'popup-tip)
                    (lambda (string &rest arguments)
                      (setq popup-state (list string arguments))
                      'popup-shown))
                   ((symbol-function 'pos-tip-show)
                    (lambda (&rest arguments)
                      (setq tooltip-show arguments)
                      'tooltip-shown))
                   ((symbol-function 'pos-tip-hide)
                    (lambda ()
                      (setq tooltip-hide (1+ (or tooltip-hide 0)))))
                   ((symbol-function 'read-event)
                    (lambda (&rest _) (pop events)))
                   ((symbol-function 'posframe-workable-p)
                    (lambda () t))
                   ((symbol-function 'face-foreground)
                    (lambda (&rest _) "#112233"))
                   ((symbol-function 'posframe-show)
                    (lambda (&rest arguments)
                      (setq posframe-show
                            (list
                             :arguments arguments
                             :buffer
                             (with-current-buffer (car arguments)
                               (list
                                (buffer-substring-no-properties
                                 (point-min) (point-max))
                                (point) major-mode buffer-read-only
                                youdao-dictionary-current-buffer-word))))
                      'posframe-shown))
                   ((symbol-function 'posframe-delete)
                    (lambda (&rest arguments)
                      (setq posframe-delete arguments)))
                   ((symbol-function 'other-frame)
                    (lambda (&rest arguments)
                      (push arguments other-frame-calls))))
           (with-temp-buffer
             (insert "Check reliability before deployment.")
             (goto-char (point-min))
             (search-forward "reliability")
             (backward-char 4)
             (list
              :popup-return (youdao-dictionary-search-at-point+)
              :tooltip-return (youdao-dictionary-search-at-point-tooltip)
              :posframe-return (youdao-dictionary-search-at-point-posframe))))
         (list
          :popup popup-state
          :tooltip (list tooltip-show tooltip-hide)
          :posframe posframe-show
          :posframe-delete posframe-delete
          :other-frame (nreverse other-frame-calls)
          :unread-events unread-command-events
          :requests (nreverse requests)))
     (when (get-buffer youdao-dictionary-buffer-name)
       (kill-buffer youdao-dictionary-buffer-name))
     (neomacs-melpa-youdao--cleanup-response-buffers)
     (unless had-posframe
       (setq features (delq 'posframe features))))))
"####;
    let expect = expect![[
        r####"OK (:popup ("reliability [rɪˌlaɪəˈbɪləti]\n\n* Basic Explains\n- n. 可靠性\n- n. 可信度\n\n* Web References\n\n" nil) :tooltip (("reliability [rɪˌlaɪəˈbɪləti]\n\n* Basic Explains\n- n. 可靠性\n- n. 可信度\n\n* Web References\n\n" nil nil nil 0) 1) :posframe (:arguments ("*Youdao presentation parity*" :left-fringe 8 :right-fringe 8 :internal-border-color "#112233" :internal-border-width 1) :buffer ("reliability [rɪˌlaɪəˈbɪləti]\n\n* Basic Explains\n- n. 可靠性\n- n. 可信度\n\n* Web References\n\n" 1 youdao-dictionary-mode t "reliability")) :posframe-delete ("*Youdao presentation parity*") :other-frame ((0)) :unread-events (112 116) :requests (("https://openapi.youdao.com/api" "q=reliability&from=auto&to=auto&appKey=parity-app&salt=919&sign=d0a02c4b88827484812796e5313534ec76ef08de628358d7db794a87e7a0e01f&signType=v3&curtime=1722643204") ("https://openapi.youdao.com/api" "q=reliability&from=auto&to=auto&appKey=parity-app&salt=919&sign=d0a02c4b88827484812796e5313534ec76ef08de628358d7db794a87e7a0e01f&signType=v3&curtime=1722643204") ("https://openapi.youdao.com/api" "q=reliability&from=auto&to=auto&appKey=parity-app&salt=919&sign=d0a02c4b88827484812796e5313534ec76ef08de628358d7db794a87e7a0e01f&signType=v3&curtime=1722643204")))"####
    ]];
    ParityBatchCase::value(
        "presents_a_lookup_through_popup_tooltip_and_posframe",
        elisp_form,
        expect,
    )
}

fn reports_missing_credentials_and_http_failures_without_a_result() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let ((youdao-dictionary-buffer-name "*Youdao failure parity*")
       (neomacs-melpa-youdao--response-buffers nil)
       (network-calls 0)
       missing-credentials http-failure response-buffer)
   (unwind-protect
       (progn
         (setq missing-credentials
               (condition-case error-data
                   (let ((youdao-dictionary-app-key nil)
                         (youdao-dictionary-secret-key nil))
                     (cl-letf (((symbol-function 'auth-source-search)
                                (lambda (&rest _) nil))
                               ((symbol-function 'url-retrieve-synchronously)
                                (lambda (&rest _)
                                  (setq network-calls (1+ network-calls)))))
                       (youdao-dictionary-search "offline")))
                 (user-error error-data)))
         (setq http-failure
               (condition-case error-data
                   (let ((youdao-dictionary-app-key "parity-app")
                         (youdao-dictionary-secret-key "parity-secret"))
                     (cl-letf (((symbol-function 'youdao-dictionary-get-salt)
                                (lambda () "707"))
                               ((symbol-function 'youdao-dictionary-get-curtime)
                                (lambda () "1722643205"))
                               ((symbol-function 'url-retrieve-synchronously)
                                (lambda (&rest _)
                                  (setq network-calls (1+ network-calls))
                                  (setq response-buffer
                                        (neomacs-melpa-youdao--response-buffer
                                         "{\"errorCode\":\"503\"}"))
                                  (with-current-buffer response-buffer
                                    (setq-local url-http-response-status 503))
                                  response-buffer)))
                       (youdao-dictionary-search "service unavailable")))
                 (error error-data)))
         (list
          :missing-credentials missing-credentials
          :http-failure http-failure
          :network-calls network-calls
          :response-buffer-live (buffer-live-p response-buffer)
          :result-buffer-live
          (and (get-buffer youdao-dictionary-buffer-name) t)))
     (when (get-buffer youdao-dictionary-buffer-name)
       (kill-buffer youdao-dictionary-buffer-name))
     (neomacs-melpa-youdao--cleanup-response-buffers))))
"####;
    let expect = expect![[
        r####"OK (:missing-credentials (user-error "You have not set the API key/secret.  See also URL ‘https://github.com/xuchunyang/youdao-dictionary.el#usage’.") :http-failure (error "Problem connecting to the server") :network-calls 1 :response-buffer-live t :result-buffer-live nil)"####
    ]];
    ParityBatchCase::value(
        "reports_missing_credentials_and_http_failures_without_a_result",
        elisp_form,
        expect,
    )
}

fn uses_auth_source_and_plays_region_and_result_buffer_audio() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let ((youdao-dictionary-app-key nil)
       (youdao-dictionary-secret-key nil)
       (youdao-dictionary-buffer-name "*Youdao auth parity*")
       (neomacs-melpa-youdao--response-buffers nil)
       auth-calls request rendered processes
       region-state result-state missing-player)
   (unwind-protect
       (save-window-excursion
         (cl-letf (((symbol-function 'auth-source-search)
                    (lambda (&rest arguments)
                      (push arguments auth-calls)
                      (list (list :user "auth-app"
                                  :secret (lambda () "auth-secret")))))
                   ((symbol-function 'youdao-dictionary-get-salt)
                    (lambda () "808"))
                   ((symbol-function 'youdao-dictionary-get-curtime)
                    (lambda () "1722643203"))
                   ((symbol-function 'url-retrieve-synchronously)
                    (lambda (url &rest _)
                      (setq request (list url url-request-data))
                      (neomacs-melpa-youdao--response-buffer
                       "{\"errorCode\":\"0\",\"query\":\"authenticated lookup\",\"translation\":[\"认证查询\"]}")))
                   ((symbol-function 'executable-find)
                    (lambda (program)
                      (and (equal program "mpv") "/usr/bin/mpv")))
                   ((symbol-function 'start-process)
                    (lambda (&rest arguments)
                      (push arguments processes)
                      'parity-process)))
           (youdao-dictionary-search "authenticated lookup")
           (with-current-buffer youdao-dictionary-buffer-name
             (setq rendered
                   (list (buffer-substring-no-properties (point-min) (point-max))
                         youdao-dictionary-current-buffer-word)))
           (with-temp-buffer
             (insert "Read the release note aloud.")
             (goto-char (point-min))
             (search-forward "release note")
             (set-mark (match-beginning 0))
             (goto-char (match-end 0))
             (let ((transient-mark-mode t)
                   (mark-active t))
               (setq region-state
                     (youdao-dictionary-play-voice-at-point))))
           (with-temp-buffer
             (setq-local youdao-dictionary-current-buffer-word "可靠性 λ")
             (setq result-state
                   (youdao-dictionary-play-voice-of-current-word)))
           (setq missing-player
                 (condition-case error-data
                     (cl-letf (((symbol-function 'executable-find)
                                (lambda (_program) nil))
                               ((symbol-function 'read-string)
                                (lambda (&rest _) "offline word")))
                       (with-temp-buffer
                         (youdao-dictionary-play-voice-from-input)))
                   (user-error error-data)))
           (list :auth-source-calls (nreverse auth-calls)
                 :request request
                 :rendered rendered
                 :region region-state
                 :result result-state
                 :processes (nreverse processes)
                 :missing-player missing-player)))
     (when (get-buffer youdao-dictionary-buffer-name)
       (kill-buffer youdao-dictionary-buffer-name))
     (neomacs-melpa-youdao--cleanup-response-buffers))))
"####;
    let expect = expect![[
        r####"OK (:auth-source-calls ((:host "openapi.youdao.com" :max 1) (:host "openapi.youdao.com" :max 1) (:host "openapi.youdao.com" :max 1) (:host "openapi.youdao.com" :max 1) (:host "openapi.youdao.com" :max 1) (:host "openapi.youdao.com" :max 1) (:host "openapi.youdao.com" :max 1) (:host "openapi.youdao.com" :max 1) (:host "openapi.youdao.com" :max 1) (:host "openapi.youdao.com" :max 1) (:host "openapi.youdao.com" :max 1)) :request ("https://openapi.youdao.com/api" "q=authenticated%20lookup&from=auto&to=auto&appKey=auth-app&salt=808&sign=3db903455f87ed2f3f2e473527604261f088f6866fc285d7b05397a7abf9cb83&signType=v3&curtime=1722643203") :rendered ("authenticated lookup\n\n* Translation\n- 认证查询\n" "authenticated lookup") :region parity-process :result parity-process :processes (("/usr/bin/mpv" nil "/usr/bin/mpv" "http://dict.youdao.com/dictvoice?type=2&audio=release%20note") ("/usr/bin/mpv" nil "/usr/bin/mpv" "http://dict.youdao.com/dictvoice?type=2&audio=%E5%8F%AF%E9%9D%A0%E6%80%A7%20%CE%BB")) :missing-player (user-error "mplayer or mpg123 is needed to play word voice"))"####
    ]];
    ParityBatchCase::value(
        "uses_auth_source_and_plays_region_and_result_buffer_audio",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        signs_and_submits_a_real_translation_request_with_history(),
        renders_sync_and_async_lookups_in_the_dictionary_buffer(),
        replaces_a_region_and_word_from_real_dictionary_choices(),
        presents_a_lookup_through_popup_tooltip_and_posframe(),
        reports_missing_credentials_and_http_failures_without_a_result(),
        uses_auth_source_and_plays_region_and_result_buffer_audio(),
    ]
}
