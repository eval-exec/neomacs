use expect_test::expect;

use super::ParityBatchCase;

fn project_file_uris_round_trip_spaces_unicode_and_reserved_filename_characters() -> ParityBatchCase
{
    let elisp_form = r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name "lsp-uri-project"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (files
        (list
         (cons "client α/reports/a#b%.el" "(message \"α report\")\n")
         (cons "客户 notes/launch plan.md" "# Launch 🚀\n")))
       (root-uri nil))
  (when (file-directory-p root)
    (delete-directory root t))
  (unwind-protect
      (progn
        (dolist (entry files)
          (let ((path (expand-file-name (car entry) root)))
            (make-directory (file-name-directory path) t)
            (write-region (cdr entry) nil path nil 'silent)))
        (setq root-uri (lsp--path-to-uri root))
        (mapcar
         (lambda (entry)
           (let* ((relative (car entry))
                  (path (expand-file-name relative root))
                  (uri (lsp--path-to-uri path))
                  (round-trip (lsp--uri-to-path uri)))
             (list
              :relative relative
              :uri-suffix (string-remove-prefix root-uri uri)
              :round-trip-relative (file-relative-name round-trip root)
              :exists (file-exists-p round-trip)
              :contents
              (with-temp-buffer
                (insert-file-contents round-trip)
                (buffer-string)))))
         files))
    (when (file-directory-p root)
      (delete-directory root t))))
"##;
    let expected = expect![[
        r##"OK ((:relative "client α/reports/a#b%.el" :uri-suffix "client%20%CE%B1/reports/a%23b%25.el" :round-trip-relative "client α/reports/a#b%.el" :exists t :contents "(message \"α report\")\n") (:relative "客户 notes/launch plan.md" :uri-suffix "%E5%AE%A2%E6%88%B7%20notes/launch%20plan.md" :round-trip-relative "客户 notes/launch plan.md" :exists t :contents "# Launch 🚀\n"))"##
    ]];
    ParityBatchCase::value(
        "project_file_uris_round_trip_spaces_unicode_and_reserved_filename_characters",
        elisp_form,
        expected,
    )
}

pub(super) fn uri_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![project_file_uris_round_trip_spaces_unicode_and_reserved_filename_characters()]
}
