use expect_test::expect;

use super::ParityBatchCase;

fn aurel_download_clones_missing_repository_and_returns_destination() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_download_clones_missing_repository_and_returns_destination",
        r##"(let (events)
         (cl-letf
             (((symbol-function
                'file-exists-p)
               (lambda (file)
                 (push
                  (list :exists file)
                  events)
                 nil))
              ((symbol-function
                'call-process)
               (lambda (&rest arguments)
                 (push
                  (cons
                   :call
                   (cons
                    default-directory
                    arguments))
                  events)
                 0))
              ((symbol-function 'message)
               (lambda (format-string &rest arguments)
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
            (aurel-download
             "https://aur.example/demo.git"
             "/fixture/downloads/")
            (nreverse events))))"##,
        expect![[
            r#"OK ("/fixture/downloads/demo" ((:message "Cloning https://aur.example/demo.git") (:exists "/fixture/downloads/demo") (:call "/fixture/downloads/" "git" nil (:buffer "*aurel debug*") nil "clone" "https://aur.example/demo.git")))"#
        ]],
    )
}

fn aurel_download_existing_repository_skips_clone_and_reports_destination() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_download_existing_repository_skips_clone_and_reports_destination",
        r##"(let (events)
         (cl-letf
             (((symbol-function
                'file-exists-p)
               (lambda (file)
                 (push
                  (list :exists file)
                  events)
                 t))
              ((symbol-function
                'call-process)
               (lambda (&rest arguments)
                 (push
                  (cons :unexpected-call arguments)
                  events)
                 :unexpected))
              ((symbol-function 'message)
               (lambda (format-string &rest arguments)
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
            (aurel-download
             "ssh://aur.example/existing.git"
             "/fixture/downloads")
            (nreverse events))))"##,
        expect![[
            r#"OK ("/fixture/downloads/existing" ((:message "Cloning ssh://aur.example/existing.git") (:exists "/fixture/downloads/existing") (:message "Package directory already exists: /fixture/downloads/existing")))"#
        ]],
    )
}

fn aurel_download_adapters_open_dired_pkgbuild_and_eshell_destinations() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_download_adapters_open_dired_pkgbuild_and_eshell_destinations",
        r##"(let (events)
         (cl-letf
             (((symbol-function
                'aurel-download)
               (lambda (url dir)
                 (push
                  (list :download url dir)
                  events)
                 (concat
                  (file-name-as-directory
                   dir)
                  "demo")))
              ((symbol-function 'dired)
               (lambda (directory)
                 (push
                  (list :dired directory)
                  events)
                 :dired))
              ((symbol-function
                'file-exists-p)
               (lambda (file)
                 (push
                  (list :exists file)
                  events)
                 t))
              ((symbol-function 'find-file)
               (lambda (file)
                 (push
                  (list :find-file file)
                  events)
                 :file))
              ((symbol-function 'eshell)
               (lambda ()
                 (push :eshell events)
                 :shell))
              ((symbol-function 'eshell/cd)
               (lambda (directory)
                 (push
                  (list :cd directory)
                  events)
                 :cd)))
           (list
            (aurel-download-dired
             "fixture:demo"
             "/one")
            (aurel-download-pkgbuild
             "fixture:demo"
             "/two/")
            (aurel-download-eshell
             "fixture:demo"
             "/three")
            (nreverse events))))"##,
        expect![[
            r#"OK (:dired :file :cd ((:download "fixture:demo" "/one") (:dired "/one/demo") (:download "fixture:demo" "/two/") (:exists "/two/demo/PKGBUILD") (:find-file "/two/demo/PKGBUILD") (:download "fixture:demo" "/three") :eshell (:cd "/three/demo")))"#
        ]],
    )
}

fn aurel_pkgbuild_adapter_reports_exact_missing_file_after_download() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_pkgbuild_adapter_reports_exact_missing_file_after_download",
        r##"(let (events)
         (cl-letf
             (((symbol-function
                'aurel-download)
               (lambda (url dir)
                 (push
                  (list :download url dir)
                  events)
                 "/fixture/demo"))
              ((symbol-function
                'file-exists-p)
               (lambda (file)
                 (push
                  (list :exists file)
                  events)
                 nil))
              ((symbol-function 'find-file)
               (lambda (file)
                 (push
                  (list :unexpected file)
                  events))))
           (list
            (aurel-test-error-data
             (lambda ()
               (aurel-download-pkgbuild
                "fixture:demo"
                "/fixture")))
            (nreverse events))))"##,
        expect![[
            r#"OK ((:error error ("File ‘/fixture/demo/PKGBUILD’ does not exist")) ((:download "fixture:demo" "/fixture") (:exists "/fixture/demo/PKGBUILD")))"#
        ]],
    )
}

fn aurel_download_directory_reader_prompts_only_with_prefix() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_download_directory_reader_prompts_only_with_prefix",
        r##"(let ((aurel-download-directory
                "/configured/")
               (aurel-directory-prompt
                "Destination: ")
               calls)
         (cl-letf
             (((symbol-function
                'read-directory-name)
               (lambda (&rest arguments)
                 (push arguments calls)
                 "/chosen/")))
           (list
            (let ((current-prefix-arg
                   nil))
              (aurel-read-download-directory))
            (let ((current-prefix-arg
                   '(4)))
              (aurel-read-download-directory))
            (nreverse calls))))"##,
        expect![[r#"OK ("/configured/" "/chosen/" (("Destination: " "/configured/")))"#]],
    )
}

fn aurel_list_download_handles_single_cancelled_and_confirmed_multi_selection() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_list_download_handles_single_cancelled_and_confirmed_multi_selection",
        r##"(let ((entries
                '((1
                   (git-url
                    . "fixture:one"))
                  (2
                   (git-url
                    . "fixture:two"))
                  (3
                   (git-url
                    . "fixture:three"))))
               (marked-cases
                '(nil
                  (1 2)
                  (2 3)))
               (confirm-cases
                '(nil))
               events)
         (cl-letf
             (((symbol-function
                'aurel-read-download-directory)
               (lambda ()
                 "/fixture/out/"))
              ((symbol-function
                'bui-list-get-marked-id-list)
               (lambda ()
                 (pop marked-cases)))
              ((symbol-function
                'bui-list-current-id)
               (lambda ()
                 1))
              ((symbol-function
                'bui-current-entries)
               (lambda ()
                 entries))
              ((symbol-function
                'bui-entry-by-id)
               (lambda (all id)
                 (cdr
                  (assq id all))))
              ((symbol-function
                'bui-entries-by-ids)
               (lambda (all ids)
                 (mapcar
                  (lambda (id)
                    (cdr
                     (assq id all)))
                  ids)))
              ((symbol-function
                'bui-entry-value)
               (lambda (entry parameter)
                 (alist-get
                  parameter
                  entry)))
              ((symbol-function 'y-or-n-p)
               (lambda (prompt)
                 (push
                  (list :confirm prompt)
                  events)
                 (pop confirm-cases)))
              ((symbol-function
                'aurel-download-dired)
               (lambda (url dir)
                 (push
                  (list :single url dir)
                  events)
                 :single))
              ((symbol-function
                'aurel-download)
               (lambda (url dir)
                 (push
                  (list :multi url dir)
                  events)
                 (list :multi url))))
           (let ((aurel-list-download-function
                  'aurel-download-dired)
                 (aurel-list-multi-download-function
                  'aurel-download)
                 (aurel-list-multi-download-no-confirm
                  nil))
             (list
              (aurel-list-download-package)
              (aurel-list-download-package)
              (let ((aurel-list-multi-download-no-confirm
                     t))
                (aurel-list-download-package))
              (nreverse events)))))"##,
        expect![[
            r#"OK (:single nil ((:multi "fixture:two") (:multi "fixture:three")) ((:single "fixture:one" "/fixture/out/") (:confirm "Download 2 marked packages? ") (:multi "fixture:two" "/fixture/out/") (:multi "fixture:three" "/fixture/out/")))"#
        ]],
    )
}

fn aurel_user_action_honors_confirmation_then_posts_cookie_token() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_user_action_honors_confirmation_then_posts_cookie_token",
        r##"(let ((answers
                '(nil t))
               events)
         (cl-letf
             (((symbol-function 'y-or-n-p)
               (lambda (prompt)
                 (push
                  (list :confirm prompt)
                  events)
                 (pop answers)))
              ((symbol-function
                'aurel-aur-login-maybe)
               (lambda (&rest arguments)
                 (push
                  (cons :login arguments)
                  events)
                 t))
              ((symbol-function
                'aurel-get-aur-cookie)
               (lambda ()
                 (url-cookie-create
                  :name "AURSID"
                  :value "TOKEN-42")))
              ((symbol-function
                'aurel-url-post)
               (lambda (url fields &optional inhibit)
                 (push
                  (list
                   :post
                   url
                   fields
                   inhibit)
                  events)
                 :posted)))
           (list
            (aurel-aur-user-action
             'vote
             "demo-base")
            (aurel-aur-user-action
             'subscribe
             "demo-base")
            (nreverse events))))"##,
        expect![[
            r#"OK (nil t ((:confirm "Vote for `demo-base' package?") (:confirm "Enable notifications for `demo-base' package?") (:login) (:post "https://aur.archlinux.org/pkgbase/demo-base/notify" (("token" . "TOKEN-42") ("do_Notify" . "")) nil)))"#
        ]],
    )
}

fn aurel_login_maybe_prefers_cookie_then_auth_secret_then_forced_prompts() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_login_maybe_prefers_cookie_then_auth_secret_then_forced_prompts",
        r##"(let ((cookie-cases
                '(:cookie nil nil))
               events)
         (cl-letf
             (((symbol-function
                'aurel-get-aur-cookie)
               (lambda ()
                 (pop cookie-cases)))
              ((symbol-function
                'auth-source-search)
               (lambda (&rest arguments)
                 (push
                  (cons :auth arguments)
                  events)
                 (list
                  (list
                   :user
                   "auth-user"
                   :secret
                   (lambda ()
                     "auth-secret")))))
              ((symbol-function 'read-string)
               (lambda (prompt initial)
                 (push
                  (list
                   :read-user
                   prompt
                   initial)
                  events)
                 "prompt-user"))
              ((symbol-function 'read-passwd)
               (lambda (prompt)
                 (push
                  (list :read-password prompt)
                  events)
                 "prompt-secret"))
              ((symbol-function
                'aurel-aur-login)
               (lambda (&rest arguments)
                 (push
                  (cons :login arguments)
                  events)
                 :logged-in)))
           (list
            (aurel-aur-login-maybe)
            (aurel-aur-login-maybe
             nil
             :noerror)
            (aurel-aur-login-maybe
             :force
             :forced-noerror)
            (nreverse events))))"##,
        expect![[
            r#"OK (t :logged-in :logged-in ((:auth :host "aur.archlinux.org") (:login "auth-user" "auth-secret" t :noerror) (:auth :host "aur.archlinux.org") (:read-user "AUR user name: " "") (:read-password "Password: ") (:login "prompt-user" "prompt-secret" t :forced-noerror)))"#
        ]],
    )
    .fresh_process()
}

fn aurel_user_package_info_fetches_html_and_adds_nested_account_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_user_package_info_fetches_html_and_adds_nested_account_state",
        r##"(let (events)
         (cl-letf
             (((symbol-function
                'aurel-aur-login-maybe)
               (lambda (&rest arguments)
                 (push
                  (cons :login arguments)
                  events)
                 t))
              ((symbol-function
                'url-retrieve-synchronously)
               (lambda (url)
                 (push
                  (list :retrieve url)
                  events)
                 (let ((buffer
                        (generate-new-buffer
                         " *aurel-user-html*")))
                   (with-current-buffer buffer
                     (insert
                      "<form name=\"do_UnVote\">"
                      "<form name=\"do_Notify\">"))
                   buffer))))
           (let ((info
                  '((name . "demo")
                    (id . 42))))
             (list
              (aurel-get-aur-user-package-info
               "fixture:demo")
              (aurel-add-aur-user-package-info
               info)
              (nreverse events)))))"##,
        expect![[
            r#"OK (((voted . t) (subscribed)) ((user-info (voted . t) (subscribed)) (name . "demo") (id . 42)) ((:login nil t) (:retrieve "fixture:demo") (:login nil t) (:retrieve "https://aur.archlinux.org/packages/demo")))"#
        ]],
    )
}

fn aurel_info_user_action_reverts_only_after_success_without_norevert() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_info_user_action_reverts_only_after_success_without_norevert",
        r##"(let ((results
                '(nil t t))
               events)
         (cl-letf
             (((symbol-function
                'aurel-aur-user-action)
               (lambda (action package-base)
                 (push
                  (list
                   :action
                   action
                   package-base)
                  events)
                 (pop results)))
              ((symbol-function
                'revert-buffer)
               (lambda (&rest arguments)
                 (push
                  (cons :revert arguments)
                  events)
                 :reverted)))
           (list
            (aurel-info-aur-user-action
             'vote
             "demo")
            (aurel-info-aur-user-action
             'subscribe
             "demo")
            (aurel-info-aur-user-action
             'unsubscribe
             "demo"
             :norevert)
            (nreverse events))))"##,
        expect![[
            r#"OK (nil :reverted nil ((:action vote "demo") (:action subscribe "demo") (:revert nil t) (:action unsubscribe "demo")))"#
        ]],
    )
}

fn aurel_debug_writes_only_enabled_levels_with_deterministic_timestamp() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_debug_writes_only_enabled_levels_with_deterministic_timestamp",
        r##"(let ((aurel-debug-buffer
                "*aurel-test-debug*")
               (aurel-debug-level
                3))
         (cl-letf
             (((symbol-function 'current-time)
               (lambda ()
                 :fixture-time))
              ((symbol-function
                'format-time-string)
               (lambda (format time)
                 (list
                  format
                  time)
                 "12:34:56.789")))
           (list
            (aurel-debug
             1
             "received %s packages"
             2)
            (aurel-debug
             3
             "url=%s"
             "fixture")
            (aurel-debug
             4
             "hidden")
            (with-current-buffer
                aurel-debug-buffer
              (buffer-string)))))"##,
        expect![[
            r#"OK (nil nil nil "12:34:56.789 received 2 packages\n12:34:56.789 url=fixture\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aurel_download_clones_missing_repository_and_returns_destination(),
        aurel_download_existing_repository_skips_clone_and_reports_destination(),
        aurel_download_adapters_open_dired_pkgbuild_and_eshell_destinations(),
        aurel_pkgbuild_adapter_reports_exact_missing_file_after_download(),
        aurel_download_directory_reader_prompts_only_with_prefix(),
        aurel_list_download_handles_single_cancelled_and_confirmed_multi_selection(),
        aurel_user_action_honors_confirmation_then_posts_cookie_token(),
        aurel_login_maybe_prefers_cookie_then_auth_secret_then_forced_prompts(),
        aurel_user_package_info_fetches_html_and_adds_nested_account_state(),
        aurel_info_user_action_reverts_only_after_success_without_norevert(),
        aurel_debug_writes_only_enabled_levels_with_deterministic_timestamp(),
    ]
}
