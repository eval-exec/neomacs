use super::ParityBatchCase;
use expect_test::expect;

fn request_id_creates_a_normalized_zero_initialized_message_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "request_id_creates_a_normalized_zero_initialized_message_file",
        r##"(let ((id (async-status-req-id "compile")))
  (unwind-protect
      (async-status-test-id-summary id)
    (async-status-clean-up id)))"##,
        expect!["OK (t t t \"0\")"],
    )
}

fn request_ids_are_unique_and_keep_independent_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "request_ids_are_unique_and_keep_independent_values",
        r##"(let ((first (async-status-req-id "build"))
      (second (async-status-req-id "build")))
  (unwind-protect
      (progn
        (async-status-set-msg-val first 0.25)
        (async-status-set-msg-val second 0.75)
        (list
         (not (equal first second))
         (mapcar #'async-status--get-msg-val
                 (list first second))))
    (async-status-clean-up first)
    (async-status-clean-up second)))"##,
        expect!["OK (t (\"0.25\" \"0.75\"))"],
    )
}

fn unicode_and_space_names_remain_usable_file_identifiers() -> ParityBatchCase {
    ParityBatchCase::value(
        "unicode_and_space_names_remain_usable_file_identifiers",
        r##"(let ((id (async-status-req-id "build 雪 λ")))
  (unwind-protect
      (list
       (and (string-match-p "build 雪 λ" id) t)
       (async-status--get-msg-val id)
       (string-prefix-p
        "async-status--build 雪 λ-"
        (file-name-nondirectory
         (async-status--get-absolute-path-by-id id))))
    (async-status-clean-up id)))"##,
        expect!["OK (t \"0\" t)"],
    )
}

fn absolute_path_resolution_uses_the_sandbox_temporary_directory() -> ParityBatchCase {
    ParityBatchCase::value(
        "absolute_path_resolution_uses_the_sandbox_temporary_directory",
        r##"(let* ((id "async-status-manual-id")
       (path (async-status--get-absolute-path-by-id id)))
  (list
   (equal path
          (expand-file-name id temporary-file-directory))
   (equal (file-name-directory path)
          (file-name-as-directory temporary-file-directory))
   (file-name-nondirectory path)))"##,
        expect!["OK (t t \"async-status-manual-id\")"],
    )
}

fn get_message_value_trims_surrounding_whitespace_but_preserves_content() -> ParityBatchCase {
    ParityBatchCase::value(
        "get_message_value_trims_surrounding_whitespace_but_preserves_content",
        r##"(let ((id (async-status-req-id "trim")))
  (unwind-protect
      (let ((path (async-status--get-absolute-path-by-id id)))
        (with-temp-file path
          (insert " \n\t0.375 \r\n"))
        (list
         (async-status--get-msg-val id)
         (with-temp-buffer
           (insert-file-contents-literally path)
           (buffer-string))))
    (async-status-clean-up id)))"##,
        expect![[r#"OK ("0.375" " \n\0110.375 \15\n")"#]],
    )
}

fn direct_set_serializes_representative_floating_point_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "direct_set_serializes_representative_floating_point_values",
        r##"(let ((id (async-status-req-id "values")))
  (unwind-protect
      (mapcar
       (lambda (value)
         (async-status-set-msg-val id value)
         (list value
               (async-status--get-msg-val id)
               (string-to-number
                (async-status--get-msg-val id))))
       '(0.0 0.125 1.0 -2.5 1250.75))
    (async-status-clean-up id)))"##,
        expect![[
            r#"OK ((0.0 "0.0" 0.0) (0.125 "0.125" 0.125) (1.0 "1.0" 1.0) (-2.5 "-2.5" -2.5) (1250.75 "1250.75" 1250.75))"#
        ]],
    )
}

fn safe_set_uses_a_strict_default_threshold_boundary() -> ParityBatchCase {
    ParityBatchCase::value(
        "safe_set_uses_a_strict_default_threshold_boundary",
        r##"(let ((id (async-status-req-id "threshold")))
  (unwind-protect
      (let (trace)
        (dolist (value '(0.005 0.01 0.010001 0.020001 0.020002 0.030002))
          (async-status-safely-set-msg-val id value)
          (push
           (list value (async-status--get-msg-val id))
           trace))
        (nreverse trace))
    (async-status-clean-up id)))"##,
        expect![[
            r#"OK ((0.005 "0") (0.01 "0") (0.010001 "0.010001") (0.020001 "0.020001") (0.020002 "0.020001") (0.030002 "0.030002"))"#
        ]],
    )
}

fn safe_set_supports_zero_positive_and_negative_custom_thresholds() -> ParityBatchCase {
    ParityBatchCase::value(
        "safe_set_supports_zero_positive_and_negative_custom_thresholds",
        r##"(let ((id (async-status-req-id "custom-threshold")))
  (unwind-protect
      (let (trace)
        (dolist (step '((0.0 0.0)
                        (0.1 0.0)
                        (0.1 -0.001)
                        (0.15 0.1)
                        (0.21 0.1)
                        (-1.0 -2.0)))
          (async-status-safely-set-msg-val
           id (car step) (cadr step))
          (push (async-status--get-msg-val id) trace))
        (nreverse trace))
    (async-status-clean-up id)))"##,
        expect!["OK (\"0\" \"0.1\" \"0.1\" \"0.1\" \"0.21\" \"-1.0\")"],
    )
}

fn safe_set_rejects_every_non_float_value_without_modifying_the_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "safe_set_rejects_every_non_float_value_without_modifying_the_file",
        r##"(let ((id (async-status-req-id "types")))
  (unwind-protect
      (let ((outcomes
             (mapcar
              (lambda (value)
                (async-status-test-error
                 (lambda ()
                   (async-status-safely-set-msg-val id value))))
              '(0 1 nil t "0.5" (0.5) 1/2))))
        (list
         (mapcar
          (lambda (outcome)
            (list
             (car outcome)
             (cadr outcome)
             (and
              (string-match-p
               "not float"
               (format "%S" outcome))
              t)))
          outcomes)
         (async-status--get-msg-val id)))
    (async-status-clean-up id)))"##,
        expect![
            "OK (((:error error t) (:error error t) (:error error t) (:error error t) (:error error t) (:error error t) (:error error t)) \"0\")"
        ],
    )
}

fn direct_set_can_create_a_manual_message_file_and_cleanup_removes_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "direct_set_can_create_a_manual_message_file_and_cleanup_removes_it",
        r##"(let* ((id "async-status-manual-create")
       (path (async-status--get-absolute-path-by-id id)))
  (when (file-exists-p path)
    (delete-file path))
  (async-status-set-msg-val id 0.625)
  (let ((before
         (list
          (file-exists-p path)
          (async-status--get-msg-val id))))
    (async-status-clean-up id)
    (list before (file-exists-p path))))"##,
        expect!["OK ((t \"0.625\") nil)"],
    )
}

fn missing_message_files_surface_file_errors_for_reads_and_cleanup() -> ParityBatchCase {
    ParityBatchCase::value(
        "missing_message_files_surface_file_errors_for_reads_and_cleanup",
        r##"(let* ((id "async-status-definitely-missing")
       (path (async-status--get-absolute-path-by-id id)))
  (when (file-exists-p path)
    (delete-file path))
  (mapcar
   (lambda (thunk)
     (let ((outcome (async-status-test-error thunk)))
       (list (car outcome) (cadr outcome))))
   (list
    (lambda () (async-status--get-msg-val id))
    (lambda () (async-status-clean-up id)))))"##,
        expect!["OK ((:error file-missing) (:ok nil))"],
    )
}

pub(super) fn files_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        request_id_creates_a_normalized_zero_initialized_message_file(),
        request_ids_are_unique_and_keep_independent_values(),
        unicode_and_space_names_remain_usable_file_identifiers(),
        absolute_path_resolution_uses_the_sandbox_temporary_directory(),
        get_message_value_trims_surrounding_whitespace_but_preserves_content(),
        direct_set_serializes_representative_floating_point_values(),
        safe_set_uses_a_strict_default_threshold_boundary(),
        safe_set_supports_zero_positive_and_negative_custom_thresholds(),
        safe_set_rejects_every_non_float_value_without_modifying_the_file(),
        direct_set_can_create_a_manual_message_file_and_cleanup_removes_it(),
        missing_message_files_surface_file_errors_for_reads_and_cleanup(),
    ]
}
