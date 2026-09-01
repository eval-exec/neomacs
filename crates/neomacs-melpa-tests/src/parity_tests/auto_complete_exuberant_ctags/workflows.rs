use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_exuberant_ctags_setup_installs_one_global_save_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_setup_installs_one_global_save_hook",
        r##"(let ((after-save-hook nil))
                           (ac-exuberant-ctags-setup)
                           (ac-exuberant-ctags-setup)
                           (list
                            after-save-hook
                            (memq
                             'ac-exuberant-ctags-build-index
                             after-save-hook)))"##,
        expect!["OK (#1=(ac-exuberant-ctags-build-index) #1#)"],
    )
}

fn auto_complete_exuberant_ctags_source_init_builds_only_when_index_is_nil() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_source_init_builds_only_when_index_is_nil",
        r##"(let ((calls 0)
                                (initializer
                                 (cdr
                                  (assq
                                   'init
                                   ac-source-exuberant-ctags))))
                           (cl-letf
                               (((symbol-function
                                  'ac-exuberant-ctags-build-index)
                                 (lambda ()
                                   (setq calls
                                         (1+ calls))
                                   (setq
                                    ac-exuberant-ctags-index
                                    '("built f C")))))
                             (setq ac-exuberant-ctags-index nil)
                             (funcall initializer)
                             (let ((first
                                    (list
                                     calls
                                     ac-exuberant-ctags-index)))
                               (funcall initializer)
                               (setq ac-exuberant-ctags-index nil)
                               (funcall initializer)
                               (list
                                first
                                calls
                                ac-exuberant-ctags-index))))"##,
        expect![[r#"OK ((1 #1=("built f C")) 2 #1#)"#]],
    )
}

fn auto_complete_exuberant_ctags_real_project_build_and_candidate_workflow_matches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_real_project_build_and_candidate_workflow_matches",
        r##"(let* ((root
                                 (auto-complete-exuberant-ctags-test-root
                                  "real-project"))
                                (source
                                 (expand-file-name "src/main.c" root))
                                (default-directory
                                 (file-name-directory source)))
                           (auto-complete-exuberant-ctags-test-write
                            source
                            "int main(void) { return render_frame(); }\n")
                           (auto-complete-exuberant-ctags-test-write
                            (expand-file-name "tags" root)
                            (concat
                             "render_frame\tsrc/ui.c\t/^void render_frame/;\"\tkind:f\tlanguage:C\n"
                             "render_model\tsrc/model.c\t/^void render_model/;\"\tkind:f\tlanguage:C\n"
                             "reset_state\tsrc/state.c\t/^void reset_state/;\"\tkind:f\tlanguage:C\n"))
                           (let ((index
                                  (ac-exuberant-ctags-build-index)))
                             (with-temp-buffer
                               (insert "    render")
                               (let ((ac-point (point))
                                     (ac-target "render")
                                     (candidates nil))
                                 (list
                                  index
                                  (auto-complete-exuberant-ctags-test-candidate
                                   candidates)
                                  (auto-complete-exuberant-ctags-test-relative
                                   ac-exuberant-ctags-tag-file-dir
                                   root))))))"##,
        expect![[
            r#"OK (("reset_state f C" "render_model f C" "render_frame f C") ("render_model" "render_frame") "./")"#
        ]],
    )
}

fn auto_complete_exuberant_ctags_after_save_hook_rebuilds_real_project_index() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_after_save_hook_rebuilds_real_project_index",
        r##"(let* ((root
                                 (auto-complete-exuberant-ctags-test-root
                                  "save-hook-project"))
                                (source
                                 (expand-file-name "src/app.c" root))
                                (tags
                                 (expand-file-name "tags" root))
                                (after-save-hook nil))
                           (auto-complete-exuberant-ctags-test-write
                            tags
                            "old_name\tsrc/app.c\t/^old/;\"\tkind:f\tlanguage:C\n")
                           (auto-complete-exuberant-ctags-test-write
                            source
                            "int old_name(void) { return 0; }\n")
                           (ac-exuberant-ctags-setup)
                           (with-current-buffer
                               (find-file-noselect source)
                             (unwind-protect
                                 (progn
                                   (let ((inhibit-read-only t))
                                     (erase-buffer)
                                     (insert
                                      "int new_name(void) { return 1; }\n"))
                                   (auto-complete-exuberant-ctags-test-write
                                    tags
                                    "new_name\tsrc/app.c\t/^new/;\"\tkind:f\tlanguage:C\n")
                                   (save-buffer)
                                   (list
                                    ac-exuberant-ctags-index
                                    (file-exists-p source)
                                    (buffer-modified-p)))
                               (set-buffer-modified-p nil)
                               (kill-buffer
                                (current-buffer)))))"##,
        expect![[r#"OK (("new_name f C") t nil)"#]],
    )
}

fn auto_complete_exuberant_ctags_switching_projects_replaces_index_and_directory() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_switching_projects_replaces_index_and_directory",
        r##"(let* ((sandbox
                                 (auto-complete-exuberant-ctags-test-root
                                  "switch-projects"))
                                (first
                                 (expand-file-name "first/src/" sandbox))
                                (second
                                 (expand-file-name "second/lib/" sandbox)))
                           (make-directory first t)
                           (make-directory second t)
                           (auto-complete-exuberant-ctags-test-write
                            (expand-file-name "../tags" first)
                            "first_api\tx\tkind:f\tlanguage:C\n")
                           (auto-complete-exuberant-ctags-test-write
                            (expand-file-name "../tags" second)
                            "second_api\tx\tkind:m\tlanguage:Ruby\n")
                           (let ((default-directory first))
                             (ac-exuberant-ctags-build-index))
                           (let ((first-state
                                  (list
                                   ac-exuberant-ctags-index
                                   (auto-complete-exuberant-ctags-test-relative
                                    ac-exuberant-ctags-tag-file-dir
                                    sandbox))))
                             (let ((default-directory second))
                               (ac-exuberant-ctags-build-index))
                             (list
                              first-state
                              ac-exuberant-ctags-index
                              (auto-complete-exuberant-ctags-test-relative
                               ac-exuberant-ctags-tag-file-dir
                               sandbox))))"##,
        expect![[r#"OK ((("first_api f C") "first/") ("second_api m Ruby") "second/")"#]],
    )
}

fn auto_complete_exuberant_ctags_real_auto_complete_source_requires_three_characters()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_real_auto_complete_source_requires_three_characters",
        r##"(save-window-excursion
                           (let ((ac-exuberant-ctags-index
                                  '("render f C"
                                    "rename m Rust"))
                                 (real-candidate
                                  (symbol-function
                                   'ac-exuberant-ctags-candidate)))
                             (mapcar
                              (lambda (text)
                                (with-temp-buffer
                                  (switch-to-buffer
                                   (current-buffer))
                                  (let ((ac-use-comphist nil)
                                        (ac-use-quick-help nil)
                                        (ac-auto-show-menu t)
                                        (ac-expand-on-auto-complete nil)
                                        (ac-ignore-case nil)
                                        (ac-sources
                                         '(ac-source-exuberant-ctags))
                                        calls)
                                    (cl-letf
                                        (((symbol-function
                                           'ac-exuberant-ctags-candidate)
                                          (lambda ()
                                            (push ac-target calls)
                                            (funcall real-candidate))))
                                      (unwind-protect
                                          (progn
                                            (auto-complete-mode 1)
                                            (insert text)
                                            (let ((started
                                                   (auto-complete)))
                                              (list
                                               text
                                               started
                                               (nreverse calls)
                                               ac-prefix
                                               ac-target
                                               (mapcar
                                                #'substring-no-properties
                                                ac-candidates)
                                               (popup-live-p ac-menu)
                                               ac-completing)))
                                        (auto-complete-mode -1))))))
                              '("x = re" "x = ren"))))"##,
        expect![[
            r#"OK (("x = re" nil nil nil nil nil nil nil) ("x = ren" t ("ren") "ren" "ren" ("render" "rename") t t))"#
        ]],
    )
}

fn auto_complete_exuberant_ctags_real_auto_complete_project_session_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_real_auto_complete_project_session_matches",
        r##"(save-window-excursion
                           (let* ((root
                                   (auto-complete-exuberant-ctags-test-root
                                    "auto-complete-session"))
                                  (source-directory
                                   (expand-file-name "src/" root)))
                             (make-directory source-directory t)
                             (auto-complete-exuberant-ctags-test-write
                              (expand-file-name "tags" root)
                              (concat
                               "render_frame\tx\tkind:f\tlanguage:C\n"
                               "render_model\tx\tkind:f\tlanguage:C\n"
                               "reset_state\tx\tkind:f\tlanguage:C\n"))
                             (with-temp-buffer
                               (switch-to-buffer
                                (current-buffer))
                               (setq default-directory
                                     source-directory)
                               (let ((ac-use-comphist nil)
                                     (ac-use-quick-help nil)
                                     (ac-auto-show-menu t)
                                     (ac-expand-on-auto-complete nil)
                                     (ac-ignore-case nil)
                                     (ac-sources
                                      '(ac-source-exuberant-ctags))
                                     (ac-exuberant-ctags-index nil))
                                 (unwind-protect
                                     (progn
                                       (auto-complete-mode 1)
                                       (insert "result = render")
                                       (auto-complete)
                                       (let ((initial
                                              (list
                                               ac-exuberant-ctags-index
                                               ac-prefix
                                               ac-target
                                               (mapcar
                                                #'substring-no-properties
                                                ac-candidates)
                                               (popup-live-p ac-menu)
                                               (substring-no-properties
                                                (ac-selected-candidate)))))
                                         (ac-next)
                                         (let ((selected
                                                (substring-no-properties
                                                 (ac-selected-candidate))))
                                           (ac-complete)
                                           (list
                                            initial
                                            selected
                                            (buffer-string)
                                            ac-menu
                                            ac-completing
                                            (auto-complete-exuberant-ctags-test-relative
                                             ac-exuberant-ctags-tag-file-dir
                                             root)))))
                                   (auto-complete-mode -1))))))"##,
        expect![[
            r#"OK ((("reset_state f C" "render_model f C" "render_frame f C") "render" "render" ("render_model" "render_frame") t "render_model") "render_frame" "result = render_frame" nil nil "./")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_exuberant_ctags_setup_installs_one_global_save_hook(),
        auto_complete_exuberant_ctags_source_init_builds_only_when_index_is_nil(),
        auto_complete_exuberant_ctags_real_project_build_and_candidate_workflow_matches(),
        auto_complete_exuberant_ctags_after_save_hook_rebuilds_real_project_index(),
        auto_complete_exuberant_ctags_switching_projects_replaces_index_and_directory(),
        auto_complete_exuberant_ctags_real_auto_complete_source_requires_three_characters(),
        auto_complete_exuberant_ctags_real_auto_complete_project_session_matches(),
    ]
}
