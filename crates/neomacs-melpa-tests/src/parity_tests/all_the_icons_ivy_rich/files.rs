use expect_test::expect;

use super::ParityBatchCase;

fn real_local_file_reports_exact_size_time_modes_and_per_character_privilege_faces()
-> ParityBatchCase {
    ParityBatchCase::value(
        "real_local_file_reports_exact_size_time_modes_and_per_character_privilege_faces",
        r##"(let* ((root
                     (expand-file-name
                      "all-the-icons-ivy-rich-metadata"
                      (getenv "TMPDIR")))
                    (file
                     (expand-file-name "script.el" root)))
               (unwind-protect
                   (progn
                     (when (file-exists-p root)
                       (delete-directory root t))
                     (make-directory root t)
                     (with-temp-file file
                       (insert "0123456789"))
                     (set-file-modes file #o754)
                     (set-file-times
                      file
                      (encode-time 0 34 12 2 1 2020 t))
                     (setenv "TZ" "UTC")
                     (set-time-zone-rule t)
                     (let ((modes
                            (all-the-icons-ivy-rich--file-modes file)))
                       (list
                        (all-the-icons-ivy-rich--file-size file)
                        (all-the-icons-ivy-rich--file-modification-time
                         file)
                        (all-the-icons-ivy-rich--file-id file)
                        (substring-no-properties modes)
                        (mapcar
                         (lambda (index)
                           (get-text-property index 'face modes))
                         (number-sequence
                          0
                          (1- (length modes)))))))
                 (when (file-exists-p root)
                   (delete-directory root t))))"##,
        expect![[
            r#"OK ("10" "Jan 02 12:34" "" "-rwxr-xr--" (all-the-icons-ivy-rich-file-priv-no all-the-icons-ivy-rich-file-priv-read all-the-icons-ivy-rich-file-priv-write all-the-icons-ivy-rich-file-priv-exec all-the-icons-ivy-rich-file-priv-read all-the-icons-ivy-rich-file-priv-no all-the-icons-ivy-rich-file-priv-exec all-the-icons-ivy-rich-file-priv-read all-the-icons-ivy-rich-file-priv-no all-the-icons-ivy-rich-file-priv-no))"#
        ]],
    )
}

fn missing_and_remote_files_short_circuit_every_metadata_lookup_without_io() -> ParityBatchCase {
    ParityBatchCase::value(
        "missing_and_remote_files_short_circuit_every_metadata_lookup_without_io",
        r##"(let ((missing
                    (expand-file-name
                     "does-not-exist"
                     (getenv "TMPDIR")))
                   (remote
                    "/ssh:neomacs-parity.invalid:/never-visited"))
               (list
                (mapcar
                 (lambda (path)
                   (list
                    (file-remote-p path)
                    (all-the-icons-ivy-rich--file-modes path)
                    (all-the-icons-ivy-rich--file-id path)
                    (all-the-icons-ivy-rich--file-size path)
                    (all-the-icons-ivy-rich--file-modification-time
                     path)))
                 (list missing remote))
                (file-exists-p missing)))"##,
        expect![[r#"OK (((nil "" "" "" "") ("/ssh:neomacs-parity.invalid:" "" "" "" "")) nil)"#]],
    )
}

fn counsel_file_candidate_resolves_a_real_symlink_and_preserves_ivy_faces() -> ParityBatchCase {
    ParityBatchCase::value(
        "counsel_file_candidate_resolves_a_real_symlink_and_preserves_ivy_faces",
        r##"(let* ((root
                     (file-name-as-directory
                      (expand-file-name
                       "all-the-icons-ivy-rich-symlink"
                       (getenv "TMPDIR"))))
                    (target
                     (expand-file-name "target.el" root))
                    (link
                     (expand-file-name "link.el" root))
                    (ivy--directory root)
                    (ivy-last
                     (make-ivy-state
                      :caller 'counsel-find-file)))
               (unwind-protect
                   (progn
                     (when (file-exists-p root)
                       (delete-directory root t))
                     (make-directory root t)
                     (with-temp-file target
                       (insert "(message \"fixture\")\n"))
                     (make-symbolic-link "target.el" link)
                     (let ((rendered
                            (all-the-icons-ivy-rich-file-name
                             "link.el")))
                       (list
                        (substring-no-properties rendered)
                        (get-text-property 0 'face rendered)
                        (get-text-property
                         (string-match " -> " rendered)
                         'face rendered)
                        (all-the-icons-ivy-rich-file-size
                         "link.el")
                        (substring-no-properties
                         (all-the-icons-ivy-rich-file-modes
                          "link.el")))))
                 (when (file-exists-p root)
                   (delete-directory root t))))"##,
        expect![[
            r#"OK ("link.el -> target.el" nil all-the-icons-ivy-rich-doc-face "9" "lrwxrwxrwx")"#
        ]],
    )
}

fn project_root_respects_disabled_remote_and_builtin_project_boundaries() -> ParityBatchCase {
    ParityBatchCase::value(
        "project_root_respects_disabled_remote_and_builtin_project_boundaries",
        r##"(let* ((root
                     (file-name-as-directory
                      (expand-file-name
                       "all-the-icons-ivy-rich-project"
                       (getenv "TMPDIR"))))
                    (default-directory root)
                    (project-find-functions
                     (list
                      (lambda (_directory)
                        (cons 'transient root)))))
               (make-directory root t)
               (unwind-protect
                   (let ((all-the-icons-ivy-rich-project t))
                     (list
                      (file-relative-name
                       (all-the-icons-ivy-rich--project-root)
                       (file-name-directory
                        (directory-file-name root)))
                      (let ((all-the-icons-ivy-rich-project nil))
                        (all-the-icons-ivy-rich--project-root))
                      (let ((default-directory
                              "/ssh:neomacs-parity.invalid:/workspace/"))
                        (all-the-icons-ivy-rich--project-root))))
                 (when (file-exists-p root)
                   (delete-directory root t))))"##,
        expect![[
            r#"OK ("all-the-icons-ivy-rich-project/" nil "/ssh:neomacs-parity.invalid:/workspace/")"#
        ]],
    )
}

fn project_columns_read_real_candidate_metadata_from_the_detected_root() -> ParityBatchCase {
    ParityBatchCase::value(
        "project_columns_read_real_candidate_metadata_from_the_detected_root",
        r##"(let* ((root
                     (file-name-as-directory
                      (expand-file-name
                       "all-the-icons-ivy-rich-project-columns"
                       (getenv "TMPDIR"))))
                    (file
                     (expand-file-name "lib/main.rs" root))
                    (default-directory root)
                    (project-find-functions
                     (list
                      (lambda (_directory)
                        (cons 'transient root)))))
               (unwind-protect
                   (progn
                     (when (file-exists-p root)
                       (delete-directory root t))
                     (make-directory
                      (file-name-directory file)
                      t)
                     (with-temp-file file
                       (insert "fn main() {}\n"))
                     (set-file-modes file #o640)
                     (list
                      (all-the-icons-ivy-rich-project-name
                       "lib/")
                      (all-the-icons-ivy-rich-project-name
                       "lib/main.rs")
                      (substring-no-properties
                       (all-the-icons-ivy-rich-project-file-modes
                        "lib/main.rs"))
                      (all-the-icons-ivy-rich-project-file-size
                       "lib/main.rs")
                      (all-the-icons-ivy-rich-project-file-id
                       "lib/main.rs")))
                 (when (file-exists-p root)
                   (delete-directory root t))))"##,
        expect![[r#"OK (#("lib/" 0 4 (face ivy-subdir)) "lib/main.rs" "-rw-r-----" "13" "")"#]],
    )
}

fn project_transformer_distinguishes_directory_unvisited_and_visited_real_files() -> ParityBatchCase
{
    ParityBatchCase::value(
        "project_transformer_distinguishes_directory_unvisited_and_visited_real_files",
        r##"(let* ((root
                     (file-name-as-directory
                      (expand-file-name
                       "all-the-icons-ivy-rich-project-transformer"
                       (getenv "TMPDIR"))))
                    (file
                     (expand-file-name "src/core.el" root))
                    (default-directory root)
                    (ivy--directory root)
                    (ivy-last
                     (make-ivy-state
                      :caller 'project-find-file))
                    (project-find-functions
                     (list
                      (lambda (_directory)
                        (cons 'transient root))))
                    buffer)
               (unwind-protect
                   (progn
                     (when (file-exists-p root)
                       (delete-directory root t))
                     (make-directory
                      (file-name-directory file)
                      t)
                     (with-temp-file file
                       (insert "(provide 'core)\n"))
                     (let ((directory
                            (all-the-icons-ivy-rich-project-find-file-transformer
                             "src/"))
                           (unvisited
                            (all-the-icons-ivy-rich-project-find-file-transformer
                             "src/core.el")))
                       (setq buffer
                             (find-file-noselect file))
                       (let ((visited
                              (all-the-icons-ivy-rich-project-find-file-transformer
                               "src/core.el")))
                         (list
                          (list
                           (substring-no-properties directory)
                           (get-text-property
                            0 'face directory))
                          (list
                           (substring-no-properties unvisited)
                           (get-text-property
                            0 'face unvisited))
                          (list
                           (substring-no-properties visited)
                           (get-text-property
                            0 'face visited))
                          (eq
                           (get-file-buffer file)
                           buffer)))))
                 (when (buffer-live-p buffer)
                   (kill-buffer buffer))
                 (when (file-exists-p root)
                   (delete-directory root t))))"##,
        expect![[r#"OK (("src/" ivy-subdir) ("src/core.el" ivy-virtual) ("src/core.el" nil) t)"#]],
    )
}

fn file_mode_cache_reuses_one_propertized_mode_string_across_matching_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "file_mode_cache_reuses_one_propertized_mode_string_across_matching_files",
        r##"(let* ((root
                     (expand-file-name
                      "all-the-icons-ivy-rich-mode-cache"
                      (getenv "TMPDIR")))
                    (first-file
                     (expand-file-name "first" root))
                    (second-file
                     (expand-file-name "second" root))
                    (all-the-icons-ivy-rich--file-modes-cache nil))
               (unwind-protect
                   (progn
                     (when (file-exists-p root)
                       (delete-directory root t))
                     (make-directory root t)
                     (dolist (file (list first-file second-file))
                       (with-temp-file file
                         (insert "fixture"))
                       (set-file-modes file #o600))
                     (let ((first
                            (all-the-icons-ivy-rich--file-modes
                             first-file))
                           (again
                            (all-the-icons-ivy-rich--file-modes
                             first-file))
                           (second
                            (all-the-icons-ivy-rich--file-modes
                             second-file)))
                       (list
                        (substring-no-properties first)
                        (eq first again)
                        (eq first second)
                        (length
                         all-the-icons-ivy-rich--file-modes-cache)
                        (mapcar
                         (lambda (index)
                           (get-text-property index 'face first))
                         (number-sequence
                          0
                          (1- (length first)))))))
                 (when (file-exists-p root)
                   (delete-directory root t))))"##,
        expect![[
            r#"OK ("-rw-------" t t 1 (all-the-icons-ivy-rich-file-priv-no all-the-icons-ivy-rich-file-priv-read all-the-icons-ivy-rich-file-priv-write all-the-icons-ivy-rich-file-priv-no all-the-icons-ivy-rich-file-priv-no all-the-icons-ivy-rich-file-priv-no all-the-icons-ivy-rich-file-priv-no all-the-icons-ivy-rich-file-priv-no all-the-icons-ivy-rich-file-priv-no all-the-icons-ivy-rich-file-priv-no))"#
        ]],
    )
}

pub(super) fn files_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        real_local_file_reports_exact_size_time_modes_and_per_character_privilege_faces(),
        missing_and_remote_files_short_circuit_every_metadata_lookup_without_io(),
        counsel_file_candidate_resolves_a_real_symlink_and_preserves_ivy_faces(),
        project_root_respects_disabled_remote_and_builtin_project_boundaries(),
        project_columns_read_real_candidate_metadata_from_the_detected_root(),
        project_transformer_distinguishes_directory_unvisited_and_visited_real_files(),
        file_mode_cache_reuses_one_propertized_mode_string_across_matching_files(),
    ]
}
