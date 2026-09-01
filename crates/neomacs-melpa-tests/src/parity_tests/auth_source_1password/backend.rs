use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_1password_backend_parser_accepts_only_the_exact_symbol() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_backend_parser_accepts_only_the_exact_symbol",
        r##"(mapcar
          (lambda (entry)
            (list
             entry
             (let ((backend
                    (auth-source-1password-backend-parse
                     entry)))
               (and
                backend
                (list
                 (eq
                  backend
                  auth-source-1password-backend)
                 (auth-source-1password-test-backend-shape
                  backend))))))
          '(1password
            "1password"
            :1password
            (1password)
            password-store
            nil
            t
            1))"##,
        expect![[
            r#"OK ((1password (t (t auth-source-backend password-store "." t t t nil ignore auth-source-1password-search))) ("1password" nil) (:1password nil) ((1password) nil) (password-store nil) (nil nil) (t nil) (1 nil))"#
        ]],
    )
}

fn auth_source_1password_parser_hook_is_idempotent_across_source_reloads() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_parser_hook_is_idempotent_across_source_reloads",
        r##"(let ((source
                (symbol-file
                 'auth-source-1password-backend-parse
                 'defun)))
          (load source nil t)
          (load source nil t)
          (list
           (let ((count 0))
             (dolist
                 (function
                  auth-source-backend-parser-functions
                  count)
               (when
                   (eq
                    function
                    #'auth-source-1password-backend-parse)
                 (setq count
                       (1+ count)))))
           (eq
            (run-hook-with-args-until-success
             'auth-source-backend-parser-functions
             '1password)
            auth-source-1password-backend)
           (run-hook-with-args-until-success
            'auth-source-backend-parser-functions
            'not-1password)))"##,
        expect![[r#"OK (1 t #s(auth-source-backend ignore "" t t t nil ignore ignore))"#]],
    )
}

fn auth_source_1password_enable_adds_only_when_absent_and_forgets_cache_each_call()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_enable_adds_only_when_absent_and_forgets_cache_each_call",
        r##"(let ((cases
                '(("~/.authinfo"
                   "secrets:session")
                  (1password
                   "~/.authinfo")
                  ("~/.authinfo"
                   1password
                   "secrets:session")
                  ("~/.authinfo"
                   1password
                   1password
                   "secrets:session"))))
          (mapcar
           (lambda (initial)
             (let ((auth-sources
                    (copy-tree initial))
                   forget-calls)
               (cl-letf
                   (((symbol-function
                      'auth-source-forget-all-cached)
                     (lambda ()
                       (push
                        (copy-tree auth-sources)
                        forget-calls)
                       :forgotten)))
                 (list
                  initial
                  (auth-source-1password-enable)
                  (copy-tree auth-sources)
                  (auth-source-1password-enable)
                  (copy-tree auth-sources)
                  (nreverse forget-calls)))))
           cases))"##,
        expect![[
            r#"OK ((("~/.authinfo" "secrets:session") :forgotten (1password "~/.authinfo" "secrets:session") :forgotten (1password "~/.authinfo" "secrets:session") ((1password "~/.authinfo" "secrets:session") (1password "~/.authinfo" "secrets:session"))) ((1password "~/.authinfo") :forgotten (1password "~/.authinfo") :forgotten (1password "~/.authinfo") ((1password "~/.authinfo") (1password "~/.authinfo"))) (("~/.authinfo" 1password "secrets:session") :forgotten ("~/.authinfo" 1password "secrets:session") :forgotten ("~/.authinfo" 1password "secrets:session") (("~/.authinfo" 1password "secrets:session") ("~/.authinfo" 1password "secrets:session"))) (("~/.authinfo" 1password 1password "secrets:session") :forgotten ("~/.authinfo" 1password 1password "secrets:session") :forgotten ("~/.authinfo" 1password 1password "secrets:session") (("~/.authinfo" 1password 1password "secrets:session") ("~/.authinfo" 1password 1password "secrets:session"))))"#
        ]],
    )
}

fn auth_source_1password_real_backend_discovery_uses_registered_parser_and_slots() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_1password_real_backend_discovery_uses_registered_parser_and_slots",
        r##"(let ((auth-sources
                '(1password)))
          (let ((backends
                 (auth-source-backends)))
            (list
             (length backends)
             (mapcar
              #'auth-source-1password-test-backend-shape
              backends)
             (eq
              (car backends)
              auth-source-1password-backend)
             (eq
              (slot-value
               (car backends)
               'search-function)
              #'auth-source-1password-search))))"##,
        expect![[
            r#"OK (1 ((t auth-source-backend password-store "." t t t nil ignore auth-source-1password-search)) t t)"#
        ]],
    )
}

fn auth_source_1password_real_auth_source_search_forwards_spec_and_returns_token() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_1password_real_auth_source_search_forwards_spec_and_returns_token",
        r##"(let ((auth-sources
                '(1password))
               (auth-source-do-cache nil)
               events)
          (cl-letf
              (((symbol-function 'executable-find)
                (lambda (program)
                  (push
                   (list :find program)
                   events)
                  t))
               ((symbol-function 'shell-command-to-string)
                (lambda (command)
                  (push
                   (list :shell command)
                   events)
                  " integration-secret\n")))
            (list
             (auth-source-search
              :type 'password-store
              :host "db.example"
              :user "reader"
              :port 5432
              :require '(:secret)
              :max 3
              :custom "forwarded")
             (nreverse events))))"##,
        expect![[
            r#"OK (((:user "reader" :secret "integration-secret")) ((:find "op") (:shell "op read op://Personal/db.example/reader")))"#
        ]],
    )
}

fn auth_source_1password_auth_source_type_requests_still_reach_custom_backend() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_auth_source_type_requests_still_reach_custom_backend",
        r##"(let ((auth-sources
                '(1password))
               (auth-source-do-cache nil)
               events)
          (cl-letf
              (((symbol-function 'executable-find)
                (lambda (program)
                  (push
                   (list :find program)
                   events)
                  t))
               ((symbol-function 'shell-command-to-string)
                (lambda (command)
                  (push
                   (list :shell command)
                   events)
                  "unexpected")))
            (list
             (auth-source-search
              :type 'netrc
              :host "db.example"
              :user "reader")
             (auth-source-search
              :type '(json plstore)
              :host "db.example"
              :user "reader")
             (nreverse events))))"##,
        expect![[
            r#"OK (((:user "reader" :secret "unexpected")) ((:user "reader" :secret "unexpected")) ((:find "op") (:shell "op read op://Personal/db.example/reader") (:find "op") (:shell "op read op://Personal/db.example/reader")))"#
        ]],
    )
}

fn auth_source_1password_pick_first_password_runs_complete_auth_source_flow() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_pick_first_password_runs_complete_auth_source_flow",
        r##"(let ((auth-sources
                '(1password))
               (auth-source-do-cache nil)
               events)
          (cl-letf
              (((symbol-function 'executable-find)
                (lambda (program)
                  (push
                   (list :find program)
                   events)
                  t))
               ((symbol-function 'shell-command-to-string)
                (lambda (command)
                  (push
                   (list :shell command)
                   events)
                  "\n first-picked-secret \n")))
            (list
             (auth-source-pick-first-password
              :type 'password-store
              :host "mail.example"
              :user "robot"
              :port "imap")
             (nreverse events))))"##,
        expect![[
            r#"OK ("first-picked-secret" ((:find "op") (:shell "op read op://Personal/mail.example/robot")))"#
        ]],
    )
}

fn auth_source_1password_auth_source_cache_reuses_token_until_enable_clears_it() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_1password_auth_source_cache_reuses_token_until_enable_clears_it",
        r##"(let ((auth-sources
                '(1password))
               (auth-source-do-cache t)
               (auth-source-cache-expiry nil)
               (outputs
                '("first-secret"
                  "after-forget-secret"))
               events)
          (auth-source-forget-all-cached)
          (cl-letf
              (((symbol-function 'executable-find)
                (lambda (program)
                  (push
                   (list :find program)
                   events)
                  t))
               ((symbol-function 'shell-command-to-string)
                (lambda (command)
                  (push
                   (list :shell command)
                   events)
                  (pop outputs))))
            (let ((spec
                   '(:type password-store
                     :host "cache.example"
                     :user "ci"
                     :port 443
                     :require (:secret))))
              (list
               (apply #'auth-source-search spec)
               (apply #'auth-source-search spec)
               (progn
                 (auth-source-1password-enable)
                 (apply #'auth-source-search spec))
               (nreverse events)
               outputs
               auth-sources))))"##,
        expect![[
            r#"OK (#1=((:user "ci" :secret "first-secret")) #1# ((:user "ci" :secret "after-forget-secret")) ((:find "op") (:shell "op read op://Personal/cache.example/ci") (:find "op") (:shell "op read op://Personal/cache.example/ci")) nil (1password))"#
        ]],
    )
}

fn auth_source_1password_legacy_advice_registration_path_parses_real_backend() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_1password_legacy_advice_registration_path_parses_real_backend",
        r##"(let ((saved-parsers
                auth-source-backend-parser-functions)
               (source
                (symbol-file
                 'auth-source-1password-backend-parse
                 'defun)))
          (unwind-protect
              (progn
                (advice-remove
                 'auth-source-backend-parse
                 #'auth-source-1password-backend-parse)
                (makunbound
                 'auth-source-backend-parser-functions)
                (load source nil t)
                (list
                 (boundp
                  'auth-source-backend-parser-functions)
                 (and
                  (advice-member-p
                   #'auth-source-1password-backend-parse
                   'auth-source-backend-parse)
                  t)
                 (eq
                  (auth-source-backend-parse
                   '1password)
                  auth-source-1password-backend)
                 (auth-source-1password-test-backend-shape
                  (auth-source-backend-parse
                   '1password))))
            (advice-remove
             'auth-source-backend-parse
             #'auth-source-1password-backend-parse)
            (setq
             auth-source-backend-parser-functions
             saved-parsers)))"##,
        expect![[
            r#"OK (nil t t (t auth-source-backend password-store "." t t t nil ignore auth-source-1password-search))"#
        ]],
    )
}

pub(super) fn backend_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_1password_backend_parser_accepts_only_the_exact_symbol(),
        auth_source_1password_parser_hook_is_idempotent_across_source_reloads(),
        auth_source_1password_enable_adds_only_when_absent_and_forgets_cache_each_call(),
        auth_source_1password_real_backend_discovery_uses_registered_parser_and_slots(),
        auth_source_1password_real_auth_source_search_forwards_spec_and_returns_token(),
        auth_source_1password_auth_source_type_requests_still_reach_custom_backend(),
        auth_source_1password_pick_first_password_runs_complete_auth_source_flow(),
        auth_source_1password_auth_source_cache_reuses_token_until_enable_clears_it(),
        auth_source_1password_legacy_advice_registration_path_parses_real_backend(),
    ]
}
