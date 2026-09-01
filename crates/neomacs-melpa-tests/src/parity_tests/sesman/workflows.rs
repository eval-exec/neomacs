use expect_test::expect;

use super::ParityBatchCase;

fn start_registers_and_lists_sessions_per_system() -> ParityBatchCase {
    ParityBatchCase::value(
        "start_registers_and_lists_sessions_per_system",
        r####"
(neomacs-sesman-test-with-empty
 (let ((sesman-system 'NeoParity))
   (sesman-start)
   (sesman-start)
   (list :count (length (sesman-sessions 'NeoParity))
         :names (neomacs-sesman-test-session-names 'NeoParity)
         :current (car (sesman-current-session 'NeoParity))
         :has (and (sesman-has-sessions-p 'NeoParity) t)
         :objects (mapcar #'cadr (sesman-sessions 'NeoParity)))))
"####,
        expect![[
            r#"OK (:count 2 :names ("neo-0" "neo-1") :current "neo-0" :has t :objects ("object-neo-1" "object-neo-0"))"#
        ]],
    )
}

fn quit_removes_the_current_session_and_marks_objects() -> ParityBatchCase {
    ParityBatchCase::value(
        "quit_removes_the_current_session_and_marks_objects",
        r####"
(neomacs-sesman-test-with-empty
 (let ((sesman-system 'NeoParity))
   (sesman-start)
   (let ((first (copy-sequence (sesman-current-session 'NeoParity))))
     (sesman-start)
     (sesman-quit)
     (list :before-name (car first)
           :remaining (neomacs-sesman-test-session-names 'NeoParity)
           :count (length (sesman-sessions 'NeoParity))
           :current (car (sesman-current-session 'NeoParity))))))
"####,
        expect![[r#"OK (:before-name "neo-0" :remaining ("neo-1") :count 1 :current "neo-1")"#]],
    )
}

fn link_with_buffer_and_directory_records_contexts() -> ParityBatchCase {
    ParityBatchCase::value(
        "link_with_buffer_and_directory_records_contexts",
        r####"
(neomacs-sesman-test-with-empty
 (let* ((sesman-system 'NeoParity)
        (root (file-name-as-directory
               (expand-file-name "sesman-links"
                                 (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
        (buf (get-buffer-create " *sesman-parity-buf*")))
   (when (file-exists-p root) (delete-directory root t))
   (make-directory root t)
   (unwind-protect
       (progn
         (sesman-start)
         (let ((session (sesman-current-session 'NeoParity)))
           (sesman-link-with-buffer buf session)
           (let ((default-directory root))
             (sesman-link-with-directory root session))
           (list :link-count (length (sesman-links 'NeoParity))
                 :types
                 (sort
                  (mapcar (lambda (lnk)
                            (symbol-name (sesman--lnk-context-type lnk)))
                          (sesman-links 'NeoParity))
                  #'string<)
                 :session (car session)
                 :has (and (sesman-has-sessions-p 'NeoParity) t))))
     (when (buffer-live-p buf) (kill-buffer buf))
     (when (file-exists-p root) (delete-directory root t)))))
"####,
        expect![[
            r#"OK (:link-count 3 :types ("buffer" "directory" "project") :session "neo-0" :has t)"#
        ]],
    )
}

fn unlink_clears_links_and_register_add_object_extend_sessions() -> ParityBatchCase {
    ParityBatchCase::value(
        "unlink_clears_links_and_register_add_object_extend_sessions",
        r####"
(neomacs-sesman-test-with-empty
 (let ((sesman-system 'NeoParity)
       (buf (get-buffer-create " *sesman-parity-unlink*")))
   (unwind-protect
       (progn
         (sesman-register 'NeoParity (list "manual" "seed"))
         (sesman-add-object 'NeoParity "manual" "extra")
         (let ((session (sesman-session 'NeoParity "manual")))
           (sesman-link-with-buffer buf session)
           (let ((linked (length (sesman-links 'NeoParity))))
             (sesman-unlink (sesman-links 'NeoParity))
             (list :session session
                   :linked linked
                   :after-unlink (length (sesman-links 'NeoParity))
                   :lookup (car (sesman-session 'NeoParity "manual"))))))
     (when (buffer-live-p buf) (kill-buffer buf)))))
"####,
        expect![[
            r#"OK (:session ("manual" "extra" "seed") :linked 2 :after-unlink 0 :lookup "manual")"#
        ]],
    )
}

fn ensure_session_requires_links_and_missing_system_errors() -> ParityBatchCase {
    ParityBatchCase::value(
        "ensure_session_requires_links_and_missing_system_errors",
        r####"
(list
 :empty
 (neomacs-sesman-test-with-empty
  (let ((sesman-system 'NeoParity))
    (condition-case err
        (sesman-ensure-session 'NeoParity)
      (error (list :signal (car err)
                   :message (error-message-string err))))))
 :linked
 (neomacs-sesman-test-with-empty
  (let ((sesman-system 'NeoParity)
        (buf (get-buffer-create " *sesman-ensure*")))
    (unwind-protect
        (progn
          (sesman-start)
          (sesman-link-with-buffer buf (sesman-session 'NeoParity
                                                      (car (neomacs-sesman-test-session-names
                                                            'NeoParity))))
          (let ((first (sesman-ensure-session 'NeoParity))
                (second (sesman-ensure-session 'NeoParity)))
            (list :name (car first)
                  :same (equal (car first) (car second))
                  :count (length (sesman-sessions 'NeoParity)))))
      (when (buffer-live-p buf) (kill-buffer buf)))))
 :missing-system
 (condition-case err
     (let ((sesman-system nil))
       (sesman-get-system))
   (error (list :signal (car err)
                :message (error-message-string err)))))
"####,
        expect![[
            r#"OK (:empty (:signal user-error :message "No linked NeoParity sessions") :linked (:name "neo-0" :same t :count 1) :missing-system (:signal error :message "No ‘sesman-system’ in buffer ‘*scratch*’"))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        start_registers_and_lists_sessions_per_system(),
        quit_removes_the_current_session_and_marks_objects(),
        link_with_buffer_and_directory_records_contexts(),
        unlink_clears_links_and_register_add_object_extend_sessions(),
        ensure_session_requires_links_and_missing_system_errors(),
    ]
}
