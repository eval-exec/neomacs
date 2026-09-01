use expect_test::expect;

use super::ParityBatchCase;

fn auth_source_keytar_enable_adds_keytar_to_front_and_forgets_cached_credentials() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_keytar_enable_adds_keytar_to_front_and_forgets_cached_credentials",
        r##"(let ((auth-sources
                                '("~/.authinfo"
                                  "~/.netrc"))
                               calls)
          (cl-letf
              (((symbol-function
                 'auth-source-forget-all-cached)
                (lambda ()
                  (push
                   (copy-tree auth-sources)
                   calls)
                  :cache-cleared)))
            (list
             (auth-source-keytar-enable)
             auth-sources
             (nreverse calls))))"##,
        expect![[
            r#"OK (:cache-cleared (keytar "~/.authinfo" "~/.netrc") ((keytar "~/.authinfo" "~/.netrc")))"#
        ]],
    )
}

fn auth_source_keytar_enable_is_idempotent_for_source_membership_but_clears_cache_each_time()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_enable_is_idempotent_for_source_membership_but_clears_cache_each_time",
        r##"(let ((auth-sources
                                '("first-source"))
                               calls)
          (cl-letf
              (((symbol-function
                 'auth-source-forget-all-cached)
                (lambda ()
                  (push
                   (copy-tree auth-sources)
                   calls)
                  (length calls))))
            (list
             (auth-source-keytar-enable)
             (auth-source-keytar-enable)
             (auth-source-keytar-enable)
             auth-sources
             (nreverse calls))))"##,
        expect![[
            r#"OK (1 2 3 (keytar "first-source") ((keytar "first-source") (keytar "first-source") (keytar "first-source")))"#
        ]],
    )
}

fn auth_source_keytar_enable_preserves_existing_keytar_position_in_auth_sources() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auth_source_keytar_enable_preserves_existing_keytar_position_in_auth_sources",
        r##"(let ((auth-sources
                                '("first"
                                  keytar
                                  "last"))
                               calls)
          (cl-letf
              (((symbol-function
                 'auth-source-forget-all-cached)
                (lambda ()
                  (setq calls
                        (1+ (or calls 0)))
                  :cleared)))
            (list
             (auth-source-keytar-enable)
             auth-sources
             calls)))"##,
        expect![[r#"OK (:cleared ("first" keytar "last") 1)"#]],
    )
}

fn auth_source_keytar_enable_distinguishes_symbolic_source_from_similarly_named_entries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_enable_distinguishes_symbolic_source_from_similarly_named_entries",
        r##"(let ((auth-sources
                                '("keytar"
                                  (keytar)
                                  keytar-config
                                  (:source keytar))))
          (cl-letf
              (((symbol-function
                 'auth-source-forget-all-cached)
                (lambda ()
                  :cleared)))
            (list
             (auth-source-keytar-enable)
             auth-sources)))"##,
        expect![[r#"OK (:cleared (keytar "keytar" (keytar) keytar-config (:source keytar)))"#]],
    )
}

fn auth_source_keytar_enable_propagates_cache_clear_failure_after_registering_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_enable_propagates_cache_clear_failure_after_registering_source",
        r##"(let ((auth-sources
                                '("existing")))
          (cl-letf
              (((symbol-function
                 'auth-source-forget-all-cached)
                (lambda ()
                  (error
                   "fixture cache failure"))))
            (list
             (auth-source-keytar-test-error-data
              #'auth-source-keytar-enable)
             auth-sources)))"##,
        expect![[r#"OK ((:error error ("fixture cache failure")) (keytar "existing"))"#]],
    )
}

fn auth_source_keytar_enable_returns_exact_cache_clear_result() -> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_enable_returns_exact_cache_clear_result",
        r##"(mapcar
          (lambda (result)
            (let ((auth-sources nil))
              (cl-letf
                  (((symbol-function
                     'auth-source-forget-all-cached)
                    (lambda ()
                      result)))
                (list
                 result
                 (auth-source-keytar-enable)
                 auth-sources))))
          '(nil
            t
            :cleared
            17
            "done"
            (:cache "result")))"##,
        expect![[
            r#"OK ((nil nil (keytar)) (t t (keytar)) (:cleared :cleared (keytar)) (17 17 (keytar)) ("done" "done" (keytar)) (#1=(:cache "result") #1# (keytar)))"#
        ]],
    )
}

fn auth_source_keytar_enable_respects_dynamic_auth_sources_binding_without_mutating_global_default()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_enable_respects_dynamic_auth_sources_binding_without_mutating_global_default",
        r##"(let ((global-before
                                (copy-tree
                                 (default-value
                                  'auth-sources)))
                               dynamic-result)
          (setq dynamic-result
                (let ((auth-sources
                       '("sandbox-authinfo")))
                  (cl-letf
                      (((symbol-function
                         'auth-source-forget-all-cached)
                        (lambda ()
                          :cleared)))
                    (list
                     (auth-source-keytar-enable)
                     auth-sources
                     (default-value
                      'auth-sources)))))
          (list
           dynamic-result
           (default-value
            'auth-sources)
           (equal
            global-before
            (default-value
             'auth-sources))))"##,
        expect![[
            r#"OK ((:cleared #1=(keytar "sandbox-authinfo") #1#) ("~/.authinfo" "~/.authinfo.gpg" "~/.netrc") t)"#
        ]],
    )
}

fn auth_source_keytar_enable_uses_structural_membership_for_preexisting_keytar_symbol()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auth_source_keytar_enable_uses_structural_membership_for_preexisting_keytar_symbol",
        r##"(let ((auth-sources
                                (list
                                 (copy-sequence "first")
                                 (intern
                                  (concat
                                   "key"
                                   "tar"))
                                 (copy-sequence "last"))))
          (cl-letf
              (((symbol-function
                 'auth-source-forget-all-cached)
                (lambda ()
                  :cleared)))
            (list
             (auth-source-keytar-enable)
             auth-sources
             (length
              (seq-filter
               (lambda (entry)
                 (eq entry 'keytar))
               auth-sources)))))"##,
        expect![[r#"OK (:cleared ("first" keytar "last") 1)"#]],
    )
}

pub(super) fn enable_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auth_source_keytar_enable_adds_keytar_to_front_and_forgets_cached_credentials(),
        auth_source_keytar_enable_is_idempotent_for_source_membership_but_clears_cache_each_time(),
        auth_source_keytar_enable_preserves_existing_keytar_position_in_auth_sources(),
        auth_source_keytar_enable_distinguishes_symbolic_source_from_similarly_named_entries(),
        auth_source_keytar_enable_propagates_cache_clear_failure_after_registering_source(),
        auth_source_keytar_enable_returns_exact_cache_clear_result(),
        auth_source_keytar_enable_respects_dynamic_auth_sources_binding_without_mutating_global_default(),
        auth_source_keytar_enable_uses_structural_membership_for_preexisting_keytar_symbol(),
    ]
}
