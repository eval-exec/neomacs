use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_xoauth2_enable_registers_source_and_smtp_mechanism() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_enable_registers_source_and_smtp_mechanism",
        r##"(let ((auth-sources
                '("~/.authinfo"))
               (smtpmail-auth-supported
                '(plain login cram-md5)))
         (auth-source-xoauth2-enable)
         (list
          auth-sources
          smtpmail-auth-supported
          (and
           (advice-member-p
            #'auth-source-xoauth2-backend-parse
            'auth-source-backend-parse)
           t)))"##,
        expect![[r#"OK ((xoauth2 "~/.authinfo") (xoauth2 plain login cram-md5) t)"#]],
    )
}

fn auth_source_xoauth2_nnimap_advice_sends_exact_sasl_initial_response() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_nnimap_advice_sends_exact_sasl_initial_response",
        r##"(let (calls)
         (setq nnimap-authenticator
               'xoauth2)
         (fset
          'nnimap-login
          (lambda (user password)
            (push (list :fallback user password) calls)
            :fallback))
         (cl-letf
             (((symbol-function 'nnimap-capability)
               (lambda (capability)
                 (push (list :capability capability) calls)
                 t))
              ((symbol-function 'nnimap-command)
               (lambda (command)
                 (push (list :command command) calls)
                 :authenticated)))
           (auth-source-xoauth2-enable)
           (list
            (nnimap-login
             "alice@example"
             "access-token")
            (nreverse calls))))"##,
        expect![[
            r#"OK (:authenticated ((:capability "AUTH=XOAUTH2") (:capability "SASL-IR") (:command "AUTHENTICATE XOAUTH2 dXNlcj1hbGljZUBleGFtcGxlAWF1dGg9QmVhcmVyIGFjY2Vzcy10b2tlbgEB")))"#
        ]],
    )
}

fn auth_source_xoauth2_nnimap_advice_falls_back_for_each_unsatisfied_condition() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_xoauth2_nnimap_advice_falls_back_for_each_unsatisfied_condition",
        r##"(let (calls)
         (setq nnimap-authenticator
               'plain
               auth-source-xoauth2-test-capabilities
               'all)
         (fset
          'nnimap-login
          (lambda (user password)
            (push (list :fallback user password) calls)
            (list :fallback user password)))
         (auth-source-xoauth2-enable)
         (cl-letf
             (((symbol-function 'nnimap-capability)
               (lambda (capability)
                 (pcase auth-source-xoauth2-test-capabilities
                   ('none nil)
                   ('xoauth2-only
                    (equal capability "AUTH=XOAUTH2"))
                   (_ t))))
              ((symbol-function 'nnimap-command)
               (lambda (command)
                 (push (list :unexpected command) calls)
                 :unexpected)))
           (list
            (progn
              (setq nnimap-authenticator
                    'plain
                    auth-source-xoauth2-test-capabilities
                    'all)
              (nnimap-login "plain-user" "plain-password"))
            (progn
              (setq nnimap-authenticator
                    'xoauth2
                    auth-source-xoauth2-test-capabilities
                    'none)
              (nnimap-login "no-xoauth2" "password"))
            (progn
              (setq nnimap-authenticator
                    'xoauth2
                    auth-source-xoauth2-test-capabilities
                    'xoauth2-only)
              (nnimap-login "no-sasl-ir" "password"))
            (nreverse calls))))"##,
        expect![[
            r#"OK ((:fallback "plain-user" "plain-password") (:fallback "no-xoauth2" "password") (:fallback "no-sasl-ir" "password") ((:fallback "plain-user" "plain-password") (:fallback "no-xoauth2" "password") (:fallback "no-sasl-ir" "password")))"#
        ]],
    )
}

fn auth_source_xoauth2_smtp_helper_sends_exact_command_and_code() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_smtp_helper_sends_exact_command_and_code",
        r##"(let (calls)
         (cl-letf
             (((symbol-function 'smtpmail-command-or-throw)
               (lambda (&rest arguments)
                 (push arguments calls)
                 :accepted)))
           (list
            (auth-source-xoauth2--smtpmail-auth-method
             'fixture-process
             "alice@example"
             "access-token")
            (nreverse calls))))"##,
        expect![[
            r#"OK (:accepted ((fixture-process "AUTH XOAUTH2 dXNlcj1hbGljZUBleGFtcGxlAWF1dGg9QmVhcmVyIGFjY2Vzcy10b2tlbgEB" 235)))"#
        ]],
    )
}

fn auth_source_xoauth2_enable_installs_smtp_generic_method() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_enable_installs_smtp_generic_method",
        r##"(let (calls)
         (cl-letf
             (((symbol-function
                'auth-source-xoauth2--smtpmail-auth-method)
               (lambda (&rest arguments)
                 (push arguments calls)
                 :xoauth2-authenticated)))
           (auth-source-xoauth2-enable)
           (list
            (smtpmail-try-auth-method
             'fixture-process
             'xoauth2
             "alice"
             "token")
            (nreverse calls))))"##,
        expect![[r#"OK (:xoauth2-authenticated ((fixture-process "alice" "token")))"#]],
    )
}

fn auth_source_xoauth2_repeated_enable_deduplicates_lists_and_preserves_one_fallback_call()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_xoauth2_repeated_enable_deduplicates_lists_and_preserves_one_fallback_call",
        r##"(let ((auth-sources nil)
               (smtpmail-auth-supported nil)
               calls)
         (fset
          'nnimap-login
          (lambda (user password)
            (push (list :fallback user password) calls)
            :fallback))
         (auth-source-xoauth2-enable)
         (auth-source-xoauth2-enable)
         (auth-source-xoauth2-enable)
         (setq nnimap-authenticator
               'plain)
         (cl-letf
             (((symbol-function 'nnimap-capability)
               (lambda (_capability)
                 nil)))
           (list
            (nnimap-login "alice" "password")
            auth-sources
            smtpmail-auth-supported
            (nreverse calls))))"##,
        expect![[r#"OK (:fallback (xoauth2) (xoauth2) ((:fallback "alice" "password")))"#]],
    )
}

pub(super) fn enable_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_xoauth2_enable_registers_source_and_smtp_mechanism(),
        auth_source_xoauth2_nnimap_advice_sends_exact_sasl_initial_response(),
        auth_source_xoauth2_nnimap_advice_falls_back_for_each_unsatisfied_condition(),
        auth_source_xoauth2_smtp_helper_sends_exact_command_and_code(),
        auth_source_xoauth2_enable_installs_smtp_generic_method(),
        auth_source_xoauth2_repeated_enable_deduplicates_lists_and_preserves_one_fallback_call(),
    ]
}
