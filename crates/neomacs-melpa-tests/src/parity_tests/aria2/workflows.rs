use expect_test::expect;

use super::ParityBatchCase;

fn aria2_opens_a_live_dashboard_with_active_paused_and_failed_transfers() -> ParityBatchCase {
    ParityBatchCase::value(
        "aria2_opens_a_live_dashboard_with_active_paused_and_failed_transfers",
        r##"(save-window-excursion
         (neomacs-aria2-test-cleanup)
         (let* ((aria2-executable
                 (executable-find "true"))
                (aria2-start-rpc-server nil)
                (aria2-kill-process-on-emacs-exit nil)
                (aria2-rcp-secret "dashboard-secret")
                (aria2-cc-file
                 (expand-file-name
                  "dashboard-controller.eieio"
                  temporary-file-directory))
                (aria2--cc nil)
                requests
                result)
           (unwind-protect
               (cl-letf
                   (((symbol-function
                      'url-retrieve-synchronously)
                     (lambda (url silent)
                       (let* ((request
                               (neomacs-aria2-test-rpc-request
                                url
                                silent))
                              (method
                               (nth 1 request))
                              (response
                               (cond
                                ((equal method "aria2.tellActive")
                                 [((gid . "active-ubuntu")
                                   (status . "active")
                                   (totalLength . "104857600")
                                   (completedLength . "52428800")
                                   (downloadSpeed . "2097152")
                                   (uploadSpeed . "0")
                                   (files .
                                    [((uris .
                                       [((uri .
                                          "https://downloads.example/ubuntu.iso"))]))])
                                   (dir . "/downloads")
                                   (bittorrent . nil)
                                   (errorCode . nil))])
                                ((equal method "aria2.tellWaiting")
                                 [((gid . "paused-course")
                                   (status . "paused")
                                   (totalLength . "2147483648")
                                   (completedLength . "536870912")
                                   (downloadSpeed . "0")
                                   (uploadSpeed . "0")
                                   (files .
                                    [((uris . []))])
                                   (dir . "/downloads")
                                   (bittorrent .
                                    ((info .
                                      ((name .
                                        "Rust Course Videos")))))
                                   (errorCode . nil))])
                                ((equal method "aria2.tellStopped")
                                 [((gid . "failed-manual")
                                   (status . "error")
                                   (totalLength . "8192")
                                   (completedLength . "0")
                                   (downloadSpeed . "0")
                                   (uploadSpeed . "0")
                                   (files .
                                    [((uris .
                                       [((uri .
                                          "ftp://mirror.example/manual.pdf"))]))])
                                   (dir . "/downloads")
                                   (bittorrent . nil)
                                   (errorCode . "3"))])
                                (t
                                 (error
                                  "Unexpected aria2 RPC method: %s"
                                  method)))))
                         (push
                          request
                          requests)
                         (neomacs-aria2-test-rpc-response
                          request
                          response)))))
                 (aria2-downloads-list)
                 (let ((list-buffer
                        (get-buffer
                         aria2-list-buffer-name)))
                   (setq result
                         (with-current-buffer list-buffer
                           (list
                            major-mode
                            mode-name
                            buffer-read-only
                            (key-binding "p")
                            (key-binding "=")
                            (key-binding "D")
                            (buffer-substring-no-properties
                             (point-min)
                             (point-max))
                            (nreverse requests))))))
             (neomacs-aria2-test-cleanup))
           result))"##,
        expect![[
            r#"OK (aria2-mode "Aria2" t aria2-toggle-pause aria2-move-up-in-list aria2-remove-download "manual.pdf                               error   ftp           0%   0.00 kB      0.00 kB      8.00 kB    A resource was not found\nRust Course Videos                       paused  bittorrent    25%  0.00 kB      0.00 kB      2.00 GB     - \nubuntu.iso                               active  https         50%  2048.00 kB   0.00 kB      100.00 MB   - \n" ((1 "aria2.tellActive" ("token:dashboard-secret" ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (2 "aria2.tellWaiting" ("token:dashboard-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (3 "aria2.tellStopped" ("token:dashboard-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"]))))"#
        ]],
    )
}

fn aria2_resumes_reprioritizes_and_removes_a_transfer_from_the_live_list() -> ParityBatchCase {
    ParityBatchCase::value(
        "aria2_resumes_reprioritizes_and_removes_a_transfer_from_the_live_list",
        r##"(save-window-excursion
         (neomacs-aria2-test-cleanup)
         (let* ((aria2-executable
                 (executable-find "true"))
                (aria2-start-rpc-server nil)
                (aria2-kill-process-on-emacs-exit nil)
                (aria2-rcp-secret "control-secret")
                (aria2-cc-file
                 (expand-file-name
                  "control-controller.eieio"
                  temporary-file-directory))
                (aria2--cc nil)
                (transfer-state 'paused)
                requests
                result)
           (unwind-protect
               (cl-letf
                   (((symbol-function
                      'url-retrieve-synchronously)
                     (lambda (url silent)
                       (let* ((request
                               (neomacs-aria2-test-rpc-request
                                url
                                silent))
                              (method
                               (nth 1 request))
                              (row
                               '((gid . "release-image")
                                 (status . "paused")
                                 (totalLength . "734003200")
                                 (completedLength . "183500800")
                                 (downloadSpeed . "0")
                                 (uploadSpeed . "0")
                                 (files .
                                  [((uris .
                                     [((uri .
                                        "https://cdn.example/release.iso"))]))])
                                 (dir . "/downloads")
                                 (bittorrent . nil)
                                 (errorCode . nil)))
                              response)
                         (cond
                          ((equal method "aria2.tellActive")
                           (setq response
                                 (if
                                     (eq transfer-state 'active)
                                     (vector
                                      (cons
                                       '(status . "active")
                                       (assq-delete-all
                                        'status
                                        (copy-tree row))))
                                   [])))
                          ((equal method "aria2.tellWaiting")
                           (setq response
                                 (if
                                     (eq transfer-state 'paused)
                                     (vector row)
                                   [])))
                          ((equal method "aria2.tellStopped")
                           (setq response []))
                          ((equal method "aria2.unpause")
                           (setq transfer-state 'active
                                 response "release-image"))
                          ((equal method "aria2.changePosition")
                           (setq response 0))
                          ((equal method "aria2.remove")
                           (setq transfer-state 'removed
                                 response "release-image"))
                          (t
                           (error
                            "Unexpected aria2 RPC method: %s"
                            method)))
                         (push
                          request
                          requests)
                         (neomacs-aria2-test-rpc-response
                          request
                          response)))))
                 (aria2-downloads-list)
                 (let ((list-buffer
                        (get-buffer
                         aria2-list-buffer-name)))
                   (switch-to-buffer list-buffer)
                   (goto-char (point-min))
                   (search-forward "release.iso")
                   (beginning-of-line)
                   (let ((resume-command
                          (key-binding "p")))
                     (call-interactively resume-command)
                     (goto-char (point-min))
                     (search-forward "release.iso")
                     (beginning-of-line)
                     (let ((priority-command
                            (key-binding "="))
                           (current-prefix-arg '(4)))
                       (call-interactively priority-command))
                     (goto-char (point-min))
                     (search-forward "release.iso")
                     (beginning-of-line)
                     (let ((remove-command
                            (key-binding "D"))
                           (executing-kbd-macro noninteractive)
                           (unread-command-events '(?y)))
                       (call-interactively remove-command))
                     (setq result
                           (list
                            resume-command
                            (key-binding "=")
                            (key-binding "D")
                            transfer-state
                            (buffer-substring-no-properties
                             (point-min)
                             (point-max))
                            (nreverse requests))))))
             (neomacs-aria2-test-cleanup))
           result))"##,
        expect![[
            r#"OK (aria2-toggle-pause aria2-move-up-in-list aria2-remove-download removed "" ((1 "aria2.tellActive" ("token:control-secret" ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (2 "aria2.tellWaiting" ("token:control-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (3 "aria2.tellStopped" ("token:control-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (4 "aria2.unpause" ("token:control-secret" "release-image")) (5 "aria2.tellActive" ("token:control-secret" ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (6 "aria2.tellWaiting" ("token:control-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (7 "aria2.tellStopped" ("token:control-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (8 "aria2.changePosition" ("token:control-secret" "release-image" 0 "POS_SET")) (9 "aria2.tellActive" ("token:control-secret" ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (10 "aria2.tellWaiting" ("token:control-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (11 "aria2.tellStopped" ("token:control-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (12 "aria2.remove" ("token:control-secret" "release-image"))))"#
        ]],
    )
}

fn aria2_submits_a_magnet_link_through_the_widget_dialog_and_refreshes_the_dashboard()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aria2_submits_a_magnet_link_through_the_widget_dialog_and_refreshes_the_dashboard",
        r##"(save-window-excursion
         (neomacs-aria2-test-cleanup)
         (let* ((aria2-executable
                 (executable-find "true"))
                (aria2-start-rpc-server nil)
                (aria2-kill-process-on-emacs-exit nil)
                (aria2-rcp-secret "magnet-secret")
                (aria2-cc-file
                 (expand-file-name
                  "magnet-controller.eieio"
                  temporary-file-directory))
                (aria2--cc nil)
                (magnet
                 "magnet:?xt=urn:btih:0123456789abcdef&dn=Release")
                added
                requests
                result)
           (unwind-protect
               (cl-letf
                   (((symbol-function
                      'url-retrieve-synchronously)
                     (lambda (url silent)
                       (let* ((request
                               (neomacs-aria2-test-rpc-request
                                url
                                silent))
                              (method
                               (nth 1 request))
                              response)
                         (cond
                          ((equal method "aria2.tellActive")
                           (setq response
                                 (if added
                                     [((gid . "magnet-release")
                                       (status . "active")
                                       (totalLength . "4096")
                                       (completedLength . "1024")
                                       (downloadSpeed . "512")
                                       (uploadSpeed . "64")
                                       (files .
                                        [((uris .
                                           [((uri .
                                              "magnet:?xt=urn:btih:0123456789abcdef&dn=Release"))]))])
                                       (dir . "/downloads")
                                       (bittorrent .
                                        ((info .
                                          ((name .
                                            "Release Sources")))))
                                       (errorCode . nil))]
                                   [])))
                          ((member method
                                   '("aria2.tellWaiting"
                                     "aria2.tellStopped"))
                           (setq response []))
                          ((equal method "aria2.addUri")
                           (setq added t
                                 response "magnet-release"))
                          (t
                           (error
                            "Unexpected aria2 RPC method: %s"
                            method)))
                         (push
                          request
                          requests)
                         (neomacs-aria2-test-rpc-response
                          request
                          response)))))
                 (aria2-downloads-list)
                 (switch-to-buffer
                  (get-buffer
                   aria2-list-buffer-name))
                 (let ((dialog-command
                        (key-binding "u")))
                   (call-interactively dialog-command)
                   (let ((dialog-mode major-mode)
                         (dialog-header
                          (substring-no-properties
                           (format "%s"
                                   header-line-format)))
                         (submit-command
                          (lookup-key
                           aria2-dialog-mode-map
                           (kbd "C-c C-c"))))
                     (execute-kbd-macro
                      magnet)
                     (call-interactively submit-command)
                     (call-interactively
                      (key-binding "g"))
                     (setq result
                           (list
                            dialog-command
                            dialog-mode
                            dialog-header
                            submit-command
                            (get-buffer
                             aria2-url-list-buffer-name)
                            (buffer-substring-no-properties
                             (point-min)
                             (point-max))
                            (nreverse requests))))))
             (neomacs-aria2-test-cleanup))
           result))"##,
        expect![[
            r#"OK (aria2-add-uris aria2-dialog-mode "Add urls, then download with ‘C-c C-c’, or cancel with ‘C-c C-k’" aria2-dialog-submit nil "Release Sources                          active  bittorrent    25%  0.00 kB      0.00 kB      4.00 kB     - \n" ((1 "aria2.tellActive" ("token:magnet-secret" ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (2 "aria2.tellWaiting" ("token:magnet-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (3 "aria2.tellStopped" ("token:magnet-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (4 "aria2.addUri" ("token:magnet-secret" ["magnet:?xt=urn:btih:0123456789abcdef&dn=Release"])) (5 "aria2.tellActive" ("token:magnet-secret" ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (6 "aria2.tellWaiting" ("token:magnet-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (7 "aria2.tellStopped" ("token:magnet-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"]))))"#
        ]],
    )
}

fn aria2_selects_a_real_torrent_and_imports_its_exact_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "aria2_selects_a_real_torrent_and_imports_its_exact_payload",
        r##"(save-window-excursion
         (neomacs-aria2-test-cleanup)
         (let* ((torrent
                 (expand-file-name
                  "release.torrent"
                  temporary-file-directory))
                (aria2-executable
                 (executable-find "true"))
                (aria2-start-rpc-server nil)
                (aria2-kill-process-on-emacs-exit nil)
                (aria2-rcp-secret "torrent-secret")
                (aria2-cc-file
                 (expand-file-name
                  "torrent-controller.eieio"
                  temporary-file-directory))
                (aria2--cc nil)
                added
                requests
                result)
           (with-temp-file torrent
             (set-buffer-multibyte nil)
             (insert
              "d8:announce32:https://tracker.example/announce"
              "4:infod6:lengthi1073741824e"
              "4:name11:release.iso"
              "12:piece lengthi262144e"
              "6:pieces20:01234567890123456789ee"))
           (unwind-protect
               (cl-letf
                   (((symbol-function
                      'url-retrieve-synchronously)
                     (lambda (url silent)
                       (let* ((request
                               (neomacs-aria2-test-rpc-request
                                url
                                silent))
                              (method
                               (nth 1 request))
                              response)
                         (cond
                          ((equal method "aria2.tellActive")
                           (setq response
                                 (if added
                                     [((gid . "torrent-release")
                                       (status . "active")
                                       (totalLength . "1073741824")
                                       (completedLength . "0")
                                       (downloadSpeed . "0")
                                       (uploadSpeed . "0")
                                       (files .
                                        [((uris . []))])
                                       (dir . "/downloads")
                                       (bittorrent .
                                        ((info .
                                          ((name .
                                            "release.iso")))))
                                       (errorCode . nil))]
                                   [])))
                          ((member method
                                   '("aria2.tellWaiting"
                                     "aria2.tellStopped"))
                           (setq response []))
                          ((equal method "aria2.addTorrent")
                           (setq added t
                                 response "torrent-release"))
                          (t
                           (error
                            "Unexpected aria2 RPC method: %s"
                            method)))
                         (push
                          request
                          requests)
                         (neomacs-aria2-test-rpc-response
                          request
                          response)))))
                 (aria2-downloads-list)
                 (switch-to-buffer
                  (get-buffer
                   aria2-list-buffer-name))
                 (let ((import-command
                        (key-binding "f"))
                       (executing-kbd-macro noninteractive)
                       (unread-command-events
                        (append
                         torrent
                         '(return))))
                   (call-interactively import-command)
                   (setq result
                         (list
                          import-command
                          (buffer-substring-no-properties
                           (point-min)
                           (point-max))
                          (with-temp-buffer
                            (set-buffer-multibyte nil)
                            (insert-file-contents-literally
                             torrent)
                            (buffer-string))
                          (nreverse requests)))))
             (neomacs-aria2-test-cleanup)
             (when (file-exists-p torrent)
               (delete-file torrent)))
           result))"##,
        expect![[
            r#"OK (aria2-add-file "release.iso                              active  bittorrent    0%   0.00 kB      0.00 kB      1.00 GB     - \n" "d8:announce32:https://tracker.example/announce4:infod6:lengthi1073741824e4:name11:release.iso12:piece lengthi262144e6:pieces20:01234567890123456789ee" ((1 "aria2.tellActive" ("token:torrent-secret" ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (2 "aria2.tellWaiting" ("token:torrent-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (3 "aria2.tellStopped" ("token:torrent-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (4 "aria2.addTorrent" ("token:torrent-secret" "ZDg6YW5ub3VuY2UzMjpodHRwczovL3RyYWNrZXIuZXhhbXBsZS9hbm5vdW5jZTQ6aW5mb2Q2Omxlbmd0aGkxMDczNzQxODI0ZTQ6bmFtZTExOnJlbGVhc2UuaXNvMTI6cGllY2UgbGVuZ3RoaTI2MjE0NGU2OnBpZWNlczIwOjAxMjM0NTY3ODkwMTIzNDU2Nzg5ZWU=" [])) (5 "aria2.tellActive" ("token:torrent-secret" ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (6 "aria2.tellWaiting" ("token:torrent-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"])) (7 "aria2.tellStopped" ("token:torrent-secret" 0 2305843009213693951 ["gid" "status" "totalLength" "completedLength" "downloadSpeed" "uploadSpeed" "files" "dir" "bittorrent" "errorCode"]))))"#
        ]],
    )
}

fn aria2_reports_the_public_setup_failure_when_aria2c_is_not_installed() -> ParityBatchCase {
    ParityBatchCase::value(
        "aria2_reports_the_public_setup_failure_when_aria2c_is_not_installed",
        r##"(save-window-excursion
         (neomacs-aria2-test-cleanup)
         (let* ((aria2-executable
                 (expand-file-name
                  "missing-bin/aria2c"
                  temporary-file-directory))
                (aria2--cc nil)
                outcome)
           (unwind-protect
               (setq outcome
                     (condition-case error-data
                         (list
                          :unexpected-success
                          (aria2-downloads-list))
                       (error
                        (let ((buffer
                               (get-buffer
                                aria2-list-buffer-name)))
                          (list
                           :error
                           (car error-data)
                           (cdr error-data)
                           (error-message-string
                            error-data)
                           (and buffer
                                (buffer-live-p buffer))
                           (and buffer
                                (with-current-buffer buffer
                                  major-mode)))))))
             (neomacs-aria2-test-cleanup))
           outcome))"##,
        expect![[
            r#"OK (:error aria2-err-no-executable nil "Couldn’t find ‘aria2c’ executable, aborting" t aria2-mode)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aria2_opens_a_live_dashboard_with_active_paused_and_failed_transfers(),
        aria2_resumes_reprioritizes_and_removes_a_transfer_from_the_live_list(),
        aria2_submits_a_magnet_link_through_the_widget_dialog_and_refreshes_the_dashboard(),
        aria2_selects_a_real_torrent_and_imports_its_exact_payload(),
        aria2_reports_the_public_setup_failure_when_aria2c_is_not_installed(),
    ]
}
