use expect_test::expect;

use super::ParityBatchCase;

fn repository_workflow_handles_mixed_values_missing_keys_invalidation_and_clear() -> ParityBatchCase
{
    let elisp_form = r##"
(let* ((root (neomacs-pcache-test-root "pcache-basic"))
       (pcache-directory root)
       (*pcache-repositories* (make-hash-table :test 'equal))
       repository)
  (when (file-directory-p root)
    (delete-directory root t))
  (unwind-protect
      (progn
        (setq repository (pcache-repository "deploy/releases"))
        (pcache-put repository 'artifact
                    '(:name "neomacs-λ" :targets (linux windows) :ready t))
        (pcache-put repository 7 [queued 3 "retry"])
        (pcache-put repository 'nullable nil)
        (let ((loaded (neomacs-pcache-test-canonical-entries repository))
              (queries
               (list
                :artifact (pcache-get repository 'artifact :missing)
                :nullable (pcache-get repository 'nullable :missing)
                :has-nullable (pcache-has repository 'nullable)
                :missing (pcache-get repository 'absent :fallback)
                :has-missing (pcache-has repository 'absent))))
          (pcache-invalidate repository 7)
          (let ((invalidated
                 (neomacs-pcache-test-canonical-entries repository)))
            (pcache-clear repository)
            (list
             :name (object-name-string repository)
             :directory-created (file-directory-p
                                 (expand-file-name "deploy" root))
             :loaded loaded
             :queries queries
             :invalidated invalidated
             :cleared (neomacs-pcache-test-canonical-entries repository)
             :file-before-forced-save
             (file-exists-p (expand-file-name "deploy/releases" root))))))
    (when repository
      (pcache-destroy-repository "deploy/releases"))
    (when (file-directory-p root)
      (delete-directory root t))))
"##;
    let expect = expect![[
        r#"OK (:name "deploy/releases" :directory-created t :loaded ((7 [queued 3 "retry"]) (artifact #1=(:name "neomacs-λ" :targets (linux windows) :ready t)) (nullable nil)) :queries (:artifact #1# :nullable nil :has-nullable t :missing :fallback :has-missing nil) :invalidated ((artifact #1#) (nullable nil)) :cleared nil :file-before-forced-save nil)"#
    ]];
    ParityBatchCase::value(
        "repository_workflow_handles_mixed_values_missing_keys_invalidation_and_clear",
        elisp_form,
        expect,
    )
}

fn forced_save_round_trips_unicode_properties_nested_data_and_an_eieio_value() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-pcache-test-root "pcache-round-trip"))
       (pcache-directory root)
       (*pcache-repositories* (make-hash-table :test 'equal))
       (repository-name "builds/production")
       (file (expand-file-name repository-name root))
       repository original-artifact styled-label)
  (when (file-directory-p root)
    (delete-directory root t))
  (unwind-protect
      (cl-letf (((symbol-function 'current-time)
                 (lambda () (seconds-to-time 1700000000))))
        (setq repository (pcache-repository repository-name)
              original-artifact
              (make-instance
               'neomacs-pcache-test-artifact
               :name "nightly-λ"
               :digest "sha256:abc123"
               :labels '("signed" "release"))
              styled-label (propertize "ready ✓" 'face 'success))
        (pcache-put repository 'artifact original-artifact)
        (pcache-put repository 'metadata
                    '(:targets [linux windows]
                      :retries (0 2 5)
                      :owners ("release" "infra")))
        (pcache-put repository 'status styled-label)
        (pcache-save repository t)
        (let ((file-text (neomacs-pcache-test-file-string file))
              (before-properties (text-properties-at 0 styled-label)))
          (setq *pcache-repositories* (make-hash-table :test 'equal)
                repository (pcache-repository repository-name))
          (let ((restored-artifact (pcache-get repository 'artifact))
                (restored-status (pcache-get repository 'status)))
            (list
             :file
             (list
              :exists (file-exists-p file)
              :header (car (split-string file-text "\n" t))
              :repository-constructor
              (and (string-match-p "(pcache-repository" file-text) t)
              :entry-constructor
              (and (string-match-p "(pcache-entry" file-text) t)
              :internal-version
              (string-remove-prefix
               (concat emacs-version "/") pcache-version-constant))
             :valid (pcache-validate-repo repository)
             :artifact
             (list
              :class (eieio-object-class-name restored-artifact)
              :name (oref restored-artifact name)
              :digest (oref restored-artifact digest)
              :labels (oref restored-artifact labels)
              :new-instance (not (eq original-artifact restored-artifact)))
             :metadata (pcache-get repository 'metadata)
             :status
             (list
              :text restored-status
              :before-properties before-properties
              :restored-properties (text-properties-at 0 restored-status))))))
    (when repository
      (pcache-destroy-repository repository-name))
    (when (file-directory-p root)
      (delete-directory root t))))
"##;
    let expect = expect![[
        r#"OK (:file (:exists t :header ";; Object builds/production" :repository-constructor t :entry-constructor t :internal-version "0.5") :valid t :artifact (:class neomacs-pcache-test-artifact :name "nightly-λ" :digest "sha256:abc123" :labels ("signed" "release") :new-instance t) :metadata (:targets [linux windows] :retries (0 2 5) :owners ("release" "infra")) :status (:text "ready ✓" :before-properties (face success) :restored-properties nil))"#
    ]];
    ParityBatchCase::value(
        "forced_save_round_trips_unicode_properties_nested_data_and_an_eieio_value",
        elisp_form,
        expect,
    )
}

fn expiration_boundary_and_purge_remove_only_ttl_entries_at_deterministic_times() -> ParityBatchCase
{
    let elisp_form = r##"
(let* ((root (neomacs-pcache-test-root "pcache-expiration"))
       (pcache-directory root)
       (*pcache-repositories* (make-hash-table :test 'equal))
       repository before boundary after-purge)
  (when (file-directory-p root)
    (delete-directory root t))
  (unwind-protect
      (progn
        (cl-letf (((symbol-function 'current-time)
                   (lambda () (seconds-to-time 1000))))
          (setq repository (pcache-repository "sessions/cache"))
          (pcache-put repository 'short '(:token "one-use") 2)
          (pcache-put repository 'long '(:token "release") 10)
          (pcache-put repository 'permanent '(:token "manual")))
        (cl-letf (((symbol-function 'current-time)
                   (lambda () (seconds-to-time 1001))))
          (setq before
                (list
                 :short (pcache-get repository 'short :expired)
                 :long (pcache-get repository 'long :expired)
                 :permanent (pcache-get repository 'permanent :expired)
                 :has-short (pcache-has repository 'short))))
        (cl-letf (((symbol-function 'current-time)
                   (lambda () (seconds-to-time 1002))))
          (setq boundary
                (list
                 :has-before-get (pcache-has repository 'short)
                 :short (pcache-get repository 'short :expired)
                 :has-after-get (pcache-has repository 'short)
                 :remaining (neomacs-pcache-test-canonical-entries repository))))
        (cl-letf (((symbol-function 'current-time)
                   (lambda () (seconds-to-time 1010))))
          (pcache-purge-invalid repository)
          (setq after-purge
                (list
                 :long (pcache-get repository 'long :expired)
                 :permanent (pcache-get repository 'permanent :expired)
                 :entries (neomacs-pcache-test-canonical-entries repository))))
        (list
         :before before
         :at-expiration-boundary boundary
         :after-purge after-purge
         :file-written (file-exists-p
                        (expand-file-name "sessions/cache" root))))
    (when repository
      (pcache-destroy-repository "sessions/cache"))
    (when (file-directory-p root)
      (delete-directory root t))))
"##;
    let expect = expect![[
        r#"OK (:before (:short (:token "one-use") :long #1=(:token "release") :permanent #2=(:token "manual") :has-short t) :at-expiration-boundary (:has-before-get nil :short :expired :has-after-get nil :remaining ((long #1#) (permanent #2#))) :after-purge (:long :expired :permanent #2# :entries ((permanent #2#))) :file-written nil)"#
    ]];
    ParityBatchCase::value(
        "expiration_boundary_and_purge_remove_only_ttl_entries_at_deterministic_times",
        elisp_form,
        expect,
    )
}

fn delayed_save_threshold_and_force_control_which_updates_survive_a_reload() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-pcache-test-root "pcache-save-policy"))
       (pcache-directory root)
       (*pcache-repositories* (make-hash-table :test 'equal))
       (repository-name "jobs/deploy")
       (file (expand-file-name repository-name root))
       repository at-creation at-threshold after-delay lost-unforced forced)
  (when (file-directory-p root)
    (delete-directory root t))
  (unwind-protect
      (progn
        (cl-letf (((symbol-function 'current-time)
                   (lambda () (seconds-to-time 1000))))
          (setq repository (pcache-repository repository-name))
          (pcache-put repository 'queued '(build-1))
          (setq at-creation (file-exists-p file)))
        (cl-letf (((symbol-function 'current-time)
                   (lambda () (seconds-to-time 1300))))
          (pcache-put repository 'running '(build-2))
          (setq at-threshold (file-exists-p file)))
        (cl-letf (((symbol-function 'current-time)
                   (lambda () (seconds-to-time 1301))))
          (pcache-put repository 'completed '(build-3))
          (setq after-delay (file-exists-p file)))
        (setq *pcache-repositories* (make-hash-table :test 'equal)
              repository (pcache-repository repository-name))
        (cl-letf (((symbol-function 'current-time)
                   (lambda () (seconds-to-time 1302))))
          (pcache-put repository 'unforced '(build-4))
          (setq *pcache-repositories* (make-hash-table :test 'equal)
                repository (pcache-repository repository-name)
                lost-unforced (pcache-get repository 'unforced :not-persisted))
          (pcache-put repository 'forced '(build-5))
          (pcache-save repository t)
          (setq *pcache-repositories* (make-hash-table :test 'equal)
                repository (pcache-repository repository-name)
                forced (pcache-get repository 'forced :missing)))
        (list
         :file-exists (list at-creation at-threshold after-delay)
         :auto-saved
         (list
          (pcache-get repository 'queued :missing)
          (pcache-get repository 'running :missing)
          (pcache-get repository 'completed :missing))
         :lost-unforced lost-unforced
         :forced forced
         :entries (neomacs-pcache-test-canonical-entries repository)))
    (when repository
      (pcache-destroy-repository repository-name))
    (when (file-directory-p root)
      (delete-directory root t))))
"##;
    let expect = expect![
        "OK (:file-exists (nil nil t) :auto-saved (#3=(build-1) #4=(build-2) #1=(build-3)) :lost-unforced :not-persisted :forced #2=(build-5) :entries ((completed #1#) (forced #2#) (queued #3#) (running #4#)))"
    ];
    ParityBatchCase::value(
        "delayed_save_threshold_and_force_control_which_updates_survive_a_reload",
        elisp_form,
        expect,
    )
}

fn malformed_files_recover_while_stale_versions_are_accepted_after_constructor_rewrite()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-pcache-test-root "pcache-recovery"))
       (pcache-directory root)
       (*pcache-repositories* (make-hash-table :test 'equal))
       (repository-name "api/responses")
       (file (expand-file-name repository-name root))
       repository malformed stale-file stale final)
  (when (file-directory-p root)
    (delete-directory root t))
  (make-directory (file-name-directory file) t)
  (unwind-protect
      (progn
        (with-temp-file file
          (insert "this is not a persistent object\n"))
        (setq repository (pcache-repository repository-name)
              malformed
              (list
               :value (pcache-get repository 'old :fresh)
               :valid (pcache-validate-repo repository)
               :registry (eq repository
                             (gethash repository-name
                                      *pcache-repositories*))))
        (pcache-put repository 'recovered '(:status 200 :body "fresh"))
        (pcache-save repository t)
        (let ((text (neomacs-pcache-test-file-string file)))
          (with-temp-file file
            (insert
             (replace-regexp-in-string
              (regexp-quote pcache-version-constant)
              "stale-editor/0.5"
              text t t))))
        (let ((text (neomacs-pcache-test-file-string file)))
          (setq stale-file
                (list
                 :stale-version-present
                 (and (string-match-p
                       (regexp-quote "stale-editor/0.5") text)
                      t)
                 :current-version-absent
                 (not (string-match-p
                       (regexp-quote pcache-version-constant) text)))))
        (setq *pcache-repositories* (make-hash-table :test 'equal)
              repository (pcache-repository repository-name)
              stale
              (list
               :old-value (pcache-get repository 'recovered :discarded)
               :version-rewritten-to-current
               (equal (oref repository version) pcache-version-constant)
               :valid (pcache-validate-repo repository)))
        (pcache-put repository 'current '(:status 201 :body "rebuilt λ"))
        (pcache-save repository t)
        (setq *pcache-repositories* (make-hash-table :test 'equal)
              repository (pcache-repository repository-name)
              final
              (list
               :current (pcache-get repository 'current :missing)
               :old (pcache-get repository 'recovered :discarded)
               :valid (pcache-validate-repo repository)))
        (list :malformed malformed
              :stale-file stale-file
              :stale-version-load stale
              :rebuilt final))
    (when repository
      (pcache-destroy-repository repository-name))
    (when (file-directory-p root)
      (delete-directory root t))))
"##;
    let expect = expect![[
        r#"OK (:malformed (:value :fresh :valid t :registry t) :stale-file (:stale-version-present t :current-version-absent t) :stale-version-load (:old-value (:status 200 :body "fresh") :version-rewritten-to-current t :valid t) :rebuilt (:current (:status 201 :body "rebuilt λ") :old (:status 200 :body "fresh") :valid t))"#
    ]];
    ParityBatchCase::value(
        "malformed_files_recover_while_stale_versions_are_accepted_after_constructor_rewrite",
        elisp_form,
        expect,
    )
}

fn registry_identity_eql_keys_mapping_destroy_and_recreate_match_documented_contract()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-pcache-test-root "pcache-lifecycle"))
       (pcache-directory root)
       (*pcache-repositories* (make-hash-table :test 'equal))
       (repository-name "clients/github")
       (file (expand-file-name repository-name root))
       first same
       (string-key (copy-sequence "release"))
       (equal-string-key (copy-sequence "release"))
       before after-invalidate file-before-destroy registry-before-destroy
       file-after-destroy registry-after-destroy recreated)
  (when (file-directory-p root)
    (delete-directory root t))
  (unwind-protect
      (progn
        (setq first (pcache-repository repository-name)
              same (pcache-repository repository-name))
        (pcache-put first string-key '(:id 41 :state open))
        (pcache-put first 'symbol-key '(:id 42 :state merged))
        (pcache-put first 9 '(:id 43 :state queued))
        (setq before
              (list
               :same-instance (eq first same)
               :same-string-object (pcache-get first string-key :missing)
               :equal-distinct-string (pcache-get first equal-string-key :missing)
               :entries (neomacs-pcache-test-canonical-entries first)))
        (pcache-invalidate first 'symbol-key)
        (setq after-invalidate
              (neomacs-pcache-test-canonical-entries first))
        (pcache-save first t)
        (setq file-before-destroy (file-exists-p file)
              registry-before-destroy
              (eq first (gethash repository-name *pcache-repositories*)))
        (pcache-destroy-repository repository-name)
        (setq file-after-destroy (file-exists-p file)
              registry-after-destroy
              (gethash repository-name *pcache-repositories*))
        (pcache-destroy-repository repository-name)
        (setq recreated (pcache-repository repository-name))
        (list
         :before before
         :after-invalidate after-invalidate
         :destroy
         (list file-before-destroy registry-before-destroy
               file-after-destroy registry-after-destroy)
         :recreated
         (list
          :new-instance (not (eq first recreated))
          :entries (neomacs-pcache-test-canonical-entries recreated)
          :valid (pcache-validate-repo recreated))))
    (pcache-destroy-repository repository-name)
    (when (file-directory-p root)
      (delete-directory root t))))
"##;
    let expect = expect![[
        r#"OK (:before (:same-instance t :same-string-object #1=(:id 41 :state open) :equal-distinct-string :missing :entries (("release" #1#) (9 #2=(:id 43 :state queued)) (symbol-key (:id 42 :state merged)))) :after-invalidate (("release" #1#) (9 #2#)) :destroy (t t nil nil) :recreated (:new-instance t :entries nil :valid t))"#
    ]];
    ParityBatchCase::value(
        "registry_identity_eql_keys_mapping_destroy_and_recreate_match_documented_contract",
        elisp_form,
        expect,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        repository_workflow_handles_mixed_values_missing_keys_invalidation_and_clear(),
        forced_save_round_trips_unicode_properties_nested_data_and_an_eieio_value(),
        expiration_boundary_and_purge_remove_only_ttl_entries_at_deterministic_times(),
        delayed_save_threshold_and_force_control_which_updates_survive_a_reload(),
        malformed_files_recover_while_stale_versions_are_accepted_after_constructor_rewrite(),
        registry_identity_eql_keys_mapping_destroy_and_recreate_match_documented_contract(),
    ]
}
