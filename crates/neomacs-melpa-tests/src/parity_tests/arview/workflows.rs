use expect_test::expect;

use super::ParityBatchCase;

fn installed_autoload_opens_a_renamed_release_archive_and_preserves_exported_bytes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "installed_autoload_opens_a_renamed_release_archive_and_preserves_exported_bytes",
        r##"(save-window-excursion
  (let* ((archive
          (arview-test-create-project-tar
           "widget release.download"
           "config/release.conf"))
         (export-directory
          (arview-test-path "exported"))
         (exported-file
          (expand-file-name
           "widget.bin"
           export-directory))
         (before
          (list
           (featurep 'arview)
           (autoloadp
            (symbol-function 'arview))
           (commandp 'arview)))
         view-buffer
         view-directory
         observation)
    (make-directory export-directory t)
    (unwind-protect
        (progn
          (arview nil archive)
          (setq view-buffer
                (current-buffer)
                view-directory
                default-directory)
          (copy-file
           (expand-file-name
            "build/widget.bin"
            view-directory)
           exported-file)
          (setq observation
                (list
                 before
                 (featurep 'arview)
                 major-mode
                 arview-buffer-p
                 (string-prefix-p
                  (concat
                   temporary-file-directory
                   "arview-widget release.download.")
                  view-directory)
                 (arview-test-manifest
                  view-directory)
                 (file-attribute-size
                  (file-attributes exported-file))
                 (arview-test-file-sha256
                  exported-file)
                 (file-exists-p archive)))
          (kill-buffer view-buffer)
          (list
           observation
           (buffer-live-p view-buffer)
           (file-exists-p view-directory)
           (file-exists-p exported-file)
           (file-exists-p archive)))
      (when (buffer-live-p view-buffer)
        (kill-buffer view-buffer)))))"##,
        expect![[
            r#"OK (((nil t t) t dired-mode t t (("README.md" 48 "b77f148882de4ba1e7070d6654dcb6e52f24513d5e7fc027306ebcd5179b16b8") ("build/widget.bin" 14 "78b23e4f97b8c75fecf06fddfb5c83b14e802f6918a71c27c244c8531782cf05") ("config/release.conf" 47 "72330968a1047b1c2b1fd7ca30824c996829290e710f2b1cdf9d2b05d35c9c8f")) 14 "78b23e4f97b8c75fecf06fddfb5c83b14e802f6918a71c27c244c8531782cf05" t) nil nil t t)"#
        ]],
    )
}

fn dired_key_binding_opens_a_release_tree_and_cleans_it_when_the_view_is_closed() -> ParityBatchCase
{
    ParityBatchCase::value(
        "dired_key_binding_opens_a_release_tree_and_cleans_it_when_the_view_is_closed",
        r##"(save-window-excursion
  (let* ((archive
          (arview-test-create-project-tar
           "nightly build.tar"
           "config/release.conf"))
         (temporary-file-directory
          (file-name-as-directory
           (arview-test-path
            "dired-extract-root")))
         source-buffer
         view-buffer
         view-directory
         observation)
    (make-directory temporary-file-directory t)
    (unwind-protect
        (progn
          (setq source-buffer
                (dired-noselect
                 (file-name-directory archive)))
          (switch-to-buffer source-buffer)
          (dired-goto-file archive)
          (let ((command
                 (key-binding [C-return])))
            (call-interactively command)
            (setq view-buffer
                  (current-buffer)
                  view-directory
                  default-directory
                  observation
                  (list
                   command
                   major-mode
                   arview-buffer-p
                   (string-prefix-p
                    (concat
                     temporary-file-directory
                     "arview-nightly build.tar.")
                    view-directory)
                   (arview-test-manifest
                    view-directory)
                   (file-exists-p archive))))
          (kill-buffer view-buffer)
          (list
           observation
           (buffer-live-p source-buffer)
           (buffer-live-p view-buffer)
           (file-exists-p view-directory)
           (file-exists-p archive)))
      (when (buffer-live-p view-buffer)
        (kill-buffer view-buffer))
      (when (buffer-live-p source-buffer)
        (kill-buffer source-buffer)))))"##,
        expect![[
            r#"OK ((arview-dired dired-mode t t (("README.md" 48 "b77f148882de4ba1e7070d6654dcb6e52f24513d5e7fc027306ebcd5179b16b8") ("build/widget.bin" 14 "78b23e4f97b8c75fecf06fddfb5c83b14e802f6918a71c27c244c8531782cf05") ("config/release.conf" 47 "72330968a1047b1c2b1fd7ca30824c996829290e710f2b1cdf9d2b05d35c9c8f")) t) t nil nil t)"#
        ]],
    )
}

fn single_prefix_prompt_places_the_extracted_project_in_the_chosen_workspace() -> ParityBatchCase {
    ParityBatchCase::value(
        "single_prefix_prompt_places_the_extracted_project_in_the_chosen_workspace",
        r##"(save-window-excursion
  (let* ((archive
          (arview-test-create-project-tar
           "custom destination.tar"
           "config/release.conf"))
         (chosen-root
          (file-name-as-directory
           (arview-test-path
            "chosen-workspace")))
         prompt-call
         view-buffer
         view-directory
         observation)
    (make-directory chosen-root t)
    (unwind-protect
        (progn
          (cl-letf
              (((symbol-function
                 'read-directory-name)
                (lambda
                    (prompt directory
                            &optional default
                            mustmatch initial
                            predicate)
                  (setq prompt-call
                        (list
                         prompt
                         directory
                         default
                         mustmatch
                         initial
                         predicate))
                  chosen-root)))
            (arview '(4) archive))
          (setq view-buffer
                (current-buffer)
                view-directory
                default-directory
                observation
                (list
                 prompt-call
                 major-mode
                 arview-buffer-p
                 (string-prefix-p
                  (concat
                   chosen-root
                   "arview-custom destination.tar.")
                  view-directory)
                 (arview-test-manifest
                  view-directory)
                 (file-exists-p archive)))
          (kill-buffer view-buffer)
          (list
           observation
           (file-exists-p view-directory)
           (file-exists-p archive)))
      (when (buffer-live-p view-buffer)
        (kill-buffer view-buffer)))))"##,
        expect![[
            r#"OK ((("Temporary directory: " "[ORACLE-TMPDIR]/" nil t nil nil) dired-mode t t (("README.md" 48 "b77f148882de4ba1e7070d6654dcb6e52f24513d5e7fc027306ebcd5179b16b8") ("build/widget.bin" 14 "78b23e4f97b8c75fecf06fddfb5c83b14e802f6918a71c27c244c8531782cf05") ("config/release.conf" 47 "72330968a1047b1c2b1fd7ca30824c996829290e710f2b1cdf9d2b05d35c9c8f")) t) nil t)"#
        ]],
    )
}

fn remote_release_is_copied_locally_then_both_copy_and_view_are_removed_on_close() -> ParityBatchCase
{
    ParityBatchCase::value(
        "remote_release_is_copied_locally_then_both_copy_and_view_are_removed_on_close",
        r##"(save-window-excursion
  (let* ((local-fixture
          (arview-test-create-project-tar
           "remote release.tar"
           "config/release.conf"))
         (remote-archive
          "/ssh:release@build.example:/incoming/remote release.tar")
         (temporary-file-directory
          (file-name-as-directory
           (arview-test-path
            "remote-extract-root")))
         (copied-archive
          (expand-file-name
           "remote release.tar"
           temporary-file-directory))
         (arview-archive-type-functions
          '(arview-file-extension))
         (real-copy-file
          (symbol-function 'copy-file))
         copy-calls
         view-buffer
         view-directory
         observation)
    (make-directory temporary-file-directory t)
    (unwind-protect
        (cl-letf
            (((symbol-function 'copy-file)
              (lambda
                  (source destination
                          &rest arguments)
                (push
                 (list
                  source
                  destination
                  arguments)
                 copy-calls)
                (unless
                    (string=
                     source
                     remote-archive)
                  (error
                   "Unexpected remote copy source: %s"
                   source))
                (apply
                 real-copy-file
                 local-fixture
                 destination
                 arguments))))
          (arview nil remote-archive)
          (setq view-buffer
                (current-buffer)
                view-directory
                default-directory
                observation
                (list
                 (nreverse copy-calls)
                 major-mode
                 (string=
                  arview-buffer-p
                  copied-archive)
                 (file-exists-p copied-archive)
                 (file-attribute-size
                  (file-attributes copied-archive))
                 (string=
                  (arview-test-file-sha256
                   copied-archive)
                  (arview-test-file-sha256
                   local-fixture))
                 (arview-test-manifest
                  view-directory)
                 (file-exists-p local-fixture)))
          (kill-buffer view-buffer)
          (list
           observation
           (buffer-live-p view-buffer)
           (file-exists-p view-directory)
           (file-exists-p copied-archive)
           (file-exists-p local-fixture)))
      (when (buffer-live-p view-buffer)
        (kill-buffer view-buffer)))))"##,
        expect![[
            r#"OK (((("/ssh:release@build.example:/incoming/remote release.tar" "[ORACLE-SANDBOX]/remote-extract-root/" nil)) dired-mode t t 10240 t (("README.md" 48 "b77f148882de4ba1e7070d6654dcb6e52f24513d5e7fc027306ebcd5179b16b8") ("build/widget.bin" 14 "78b23e4f97b8c75fecf06fddfb5c83b14e802f6918a71c27c244c8531782cf05") ("config/release.conf" 47 "72330968a1047b1c2b1fd7ca30824c996829290e710f2b1cdf9d2b05d35c9c8f")) t) nil nil nil t)"#
        ]],
    )
}

fn unicode_archive_and_member_names_survive_extract_inspect_and_cleanup() -> ParityBatchCase {
    ParityBatchCase::value(
        "unicode_archive_and_member_names_survive_extract_inspect_and_cleanup",
        r##"(save-window-excursion
  (let* ((archive
          (arview-test-create-project-tar
           "資料 release λ.tar"
           "config/βeta λ.conf"))
         (temporary-file-directory
          (file-name-as-directory
           (arview-test-path
            "unicode-extract-root")))
         view-buffer
         view-directory
         observation)
    (make-directory temporary-file-directory t)
    (unwind-protect
        (progn
          (arview nil archive)
          (setq view-buffer
                (current-buffer)
                view-directory
                default-directory
                observation
                (list
                 (file-name-nondirectory archive)
                 major-mode
                 arview-buffer-p
                 (string-prefix-p
                  (concat
                   temporary-file-directory
                   "arview-資料 release λ.tar.")
                  view-directory)
                 (arview-test-manifest
                  view-directory)
                 (arview-test-file-sha256
                  (expand-file-name
                   "config/βeta λ.conf"
                   view-directory))))
          (kill-buffer view-buffer)
          (list
           observation
           (file-exists-p view-directory)
           (file-exists-p archive)))
      (when (buffer-live-p view-buffer)
        (kill-buffer view-buffer)))))"##,
        expect![[
            r#"OK (("資料 release λ.tar" dired-mode t t (("README.md" 48 "b77f148882de4ba1e7070d6654dcb6e52f24513d5e7fc027306ebcd5179b16b8") ("build/widget.bin" 14 "78b23e4f97b8c75fecf06fddfb5c83b14e802f6918a71c27c244c8531782cf05") ("config/βeta λ.conf" 47 "72330968a1047b1c2b1fd7ca30824c996829290e710f2b1cdf9d2b05d35c9c8f")) "72330968a1047b1c2b1fd7ca30824c996829290e710f2b1cdf9d2b05d35c9c8f") nil t)"#
        ]],
    )
    .fresh_process()
}

fn corrupt_download_opens_an_empty_view_with_actionable_tar_diagnostics_then_cleans_up()
-> ParityBatchCase {
    ParityBatchCase::value(
        "corrupt_download_opens_an_empty_view_with_actionable_tar_diagnostics_then_cleans_up",
        r##"(save-window-excursion
  (let* ((archive
          (arview-test-write-bytes
           (arview-test-path
            "truncated download.tar")
           (encode-coding-string
            "this transfer stopped before the archive arrived\n"
            'utf-8-unix
            t)))
         (temporary-file-directory
          (file-name-as-directory
           (arview-test-path
            "diagnostic-extract-root")))
         view-buffer
         view-directory
         log-text
         observation)
    (make-directory temporary-file-directory t)
    (unwind-protect
        (progn
          (arview nil archive)
          (setq view-buffer
                (current-buffer)
                view-directory
                default-directory
                log-text
                (with-current-buffer
                    (get-buffer
                     arview-log-buffer-name)
                  (buffer-string))
                observation
                (list
                 major-mode
                 arview-buffer-p
                 (arview-test-manifest
                  view-directory)
                 (and
                  (string-match-p
                   "\\(?:This does not look like a tar archive\\|not a tar archive\\)"
                   log-text)
                  t)
                 (and
                  (string-match-p
                   "\\(?:failure status\\|not recoverable\\)"
                   log-text)
                  t)
                 (> (length log-text) 60)
                 (file-exists-p archive)))
          (kill-buffer view-buffer)
          (list
           observation
           (file-exists-p view-directory)
           (file-exists-p archive)))
      (when (buffer-live-p view-buffer)
        (kill-buffer view-buffer)))))"##,
        expect!["OK ((dired-mode t nil t t t t) nil t)"],
    )
}

pub(super) fn workflows_arview_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![installed_autoload_opens_a_renamed_release_archive_and_preserves_exported_bytes()]
}

pub(super) fn workflows_arview_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        dired_key_binding_opens_a_release_tree_and_cleans_it_when_the_view_is_closed(),
        single_prefix_prompt_places_the_extracted_project_in_the_chosen_workspace(),
        remote_release_is_copied_locally_then_both_copy_and_view_are_removed_on_close(),
        unicode_archive_and_member_names_survive_extract_inspect_and_cleanup(),
        corrupt_download_opens_an_empty_view_with_actionable_tar_diagnostics_then_cleans_up(),
    ]
}
