use expect_test::expect;

use super::ParityBatchCase;

fn session_registration_updates_existing_work_and_merges_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "session_registration_updates_existing_work_and_merges_metadata",
        r##"
(let ((ai-code-session--sessions (make-hash-table :test 'equal))
      (ai-code-session--next-id 0)
      (buffer (generate-new-buffer "*ai-code-session-lifecycle*")))
  (unwind-protect
      (let* ((first
              (ai-code-session-register
               :buffer buffer :backend 'codex :repo-root "./"
               :task-file "tasks/issue.org"
               :metadata '(:branch "main" :dirty-count 1)))
             (second
              (ai-code-session-register
               :buffer buffer :backend "gemini"
               :metadata '(:dirty-count 3 :status "running"))))
        (list
         (eq first second)
         (ai-code-session-id second)
         (ai-code-session-backend second)
         (file-name-absolute-p (ai-code-session-repo-root second))
         (file-name-nondirectory (ai-code-session-task-file second))
         (ai-code-session-metadata second)
         (eq second (ai-code-session-get buffer))
         (eq second (ai-code-session-get "S1"))))
    (kill-buffer buffer)))
"##,
        expect![[
            r#"OK (t "S1" "gemini" t "issue.org" (:branch "main" :dirty-count 3 :status "running") t t)"#
        ]],
    )
}

fn session_registry_orders_activity_and_unregistration_accepts_both_keys() -> ParityBatchCase {
    ParityBatchCase::value(
        "session_registry_orders_activity_and_unregistration_accepts_both_keys",
        r##"
(let ((ai-code-session--sessions (make-hash-table :test 'equal))
      (ai-code-session--next-id 0)
      (clock 0)
      (a (generate-new-buffer "*ai-code-A*"))
      (b (generate-new-buffer "*ai-code-B*")))
  (unwind-protect
      (cl-letf (((symbol-function 'current-time)
                 (lambda () (seconds-to-time (cl-incf clock)))))
        (let ((first (ai-code-session-register :buffer a :backend 'codex))
              (second (ai-code-session-register :buffer b :backend 'gemini)))
          (ai-code-session-update-metadata first '(:phase "review"))
          (let ((ordered
                 (mapcar (lambda (session)
                           (list (ai-code-session-id session)
                                 (ai-code-session-backend session)
                                 (ai-code-session-metadata session)))
                         (ai-code-session-list))))
            (ai-code-session-unregister b)
            (let ((after-buffer (mapcar #'ai-code-session-id
                                        (ai-code-session-list))))
              (ai-code-session-unregister (ai-code-session-id first))
              (list ordered after-buffer (ai-code-session-list))))))
    (kill-buffer a)
    (kill-buffer b)))
"##,
        expect![[r#"OK ((("S2" "gemini" nil) ("S1" "codex" nil)) ("S1") nil)"#]],
    )
}

fn session_refresh_prunes_dead_buffers_and_preserves_live_runtime_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "session_refresh_prunes_dead_buffers_and_preserves_live_runtime_metadata",
        r##"
(let ((ai-code-session--sessions (make-hash-table :test 'equal))
      (ai-code-session--next-id 0)
      (live (generate-new-buffer "*ai-code-live*"))
      (dead (generate-new-buffer "*ai-code-dead*")))
  (unwind-protect
      (let* ((live-session
              (ai-code-session-register
               :buffer live :backend 'codex
               :metadata '(:branch "feature/one" :dirty-count 4)))
             (dead-session
              (ai-code-session-register :buffer dead :backend 'gemini)))
        (kill-buffer dead)
        (cl-letf (((symbol-function 'ai-code-session--branch)
                   (lambda (_root) nil))
                  ((symbol-function 'ai-code-session--dirty-count)
                   (lambda (_root) nil))
                  ((symbol-function 'get-buffer-process)
                   (lambda (_buffer) nil)))
          (let ((refreshed (ai-code-session-refresh)))
            (list
             (mapcar #'ai-code-session-id refreshed)
             (ai-code-session-metadata live-session)
             (ai-code-session-get (ai-code-session-id dead-session))))))
    (when (buffer-live-p live) (kill-buffer live))
    (when (buffer-live-p dead) (kill-buffer dead))))
"##,
        expect![[r#"OK (("S1") (:branch "feature/one" :dirty-count 4 :status "stopped") nil)"#]],
    )
}

fn session_buffer_names_roundtrip_project_and_instance_identity() -> ParityBatchCase {
    ParityBatchCase::value(
        "session_buffer_names_roundtrip_project_and_instance_identity",
        r##"
(let* ((root (make-temp-file "ai-code-session-name-" t))
       (prefix "codex")
       (plain (ai-code-backends-infra--session-buffer-name prefix root))
       (instance
        (ai-code-backends-infra--sanitize-instance-name
         "feature]oauth"))
       (named (ai-code-backends-infra--session-buffer-name
               prefix root instance))
       (plain-parsed
        (ai-code-backends-infra--parse-session-buffer-name plain prefix))
       (named-parsed
        (ai-code-backends-infra--parse-session-buffer-name named prefix)))
  (unwind-protect
      (list
       (string-prefix-p "*codex[" plain)
       (string-suffix-p "]*" plain)
       (and (string-match-p "feature-oauth" named) t)
       (mapcar
        (lambda (parsed)
          (list
           (equal
            (car parsed)
            (file-name-nondirectory
             (directory-file-name root)))
           (cdr parsed)))
        (list plain-parsed named-parsed))
       (ai-code-backends-infra--session-instance-name named prefix)
       (cdr (ai-code-backends-infra--session-key root instance)))
    (delete-directory root t)))
"##,
        expect![[r#"OK (t t t ((t nil) (t "feature-oauth")) "feature-oauth" "feature-oauth")"#]],
    )
}

fn session_dashboard_entry_formats_repository_task_backend_and_dirty_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "session_dashboard_entry_formats_repository_task_backend_and_dirty_state",
        r##"
(let* ((buffer (generate-new-buffer "*ai-code-dashboard-entry*"))
       (session
        (make-ai-code-session
         :id "S42" :buffer buffer :backend "github-copilot-cli"
         :repo-root "/workspace/payment-service/"
         :task-file "/workspace/payment-service/tasks/fix-race.org"
         :metadata '(:branch "feature/atomic-ledger"
                     :status "running" :dirty-count 7))))
  (unwind-protect
      (let ((entry (ai-code-session-dashboard--entry session)))
        (list (car entry) (append (cadr entry) nil)
              (ai-code-session-dashboard--backend-label 'open-interpreter)))
    (kill-buffer buffer)))
"##,
        expect![[
            r#"OK ("S42" ("S42" "payment-service" "fix-race.org" "Github Copilot Cli" "feature/atomic-ledger" "running" "7") "Open Interpreter")"#
        ]],
    )
}

pub(super) fn sessions_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        session_registration_updates_existing_work_and_merges_metadata(),
        session_registry_orders_activity_and_unregistration_accepts_both_keys(),
        session_refresh_prunes_dead_buffers_and_preserves_live_runtime_metadata(),
        session_buffer_names_roundtrip_project_and_instance_identity(),
        session_dashboard_entry_formats_repository_task_backend_and_dirty_state(),
    ]
}
