use expect_test::expect;

use super::ParityBatchCase;

fn auto_read_only_find_file_hook_checks_selected_buffer_then_project_then_delegates()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_find_file_hook_checks_selected_buffer_then_project_then_delegates",
        r##"(let (events)
         (cl-letf
             (((symbol-function 'window-buffer)
               (lambda (&optional _window)
                 (push :window-buffer events)
                 (current-buffer)))
              ((symbol-function 'project-current)
               (lambda (&optional _maybe-prompt
                                  _directory)
                 (push :project-current events)
                 nil))
              ((symbol-function 'auto-read-only)
               (lambda ()
                 (push :auto-read-only events)
                 :protected)))
           (list
            (auto-read-only--hook-find-file)
            (nreverse events))))"##,
        expect!["OK (:protected (:window-buffer :project-current :auto-read-only))"],
    )
}

fn auto_read_only_find_file_hook_short_circuits_before_project_for_nonselected_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_find_file_hook_short_circuits_before_project_for_nonselected_buffer",
        r##"(let ((selected
                (generate-new-buffer
                 " *auto-read-only-selected*"))
               events)
         (unwind-protect
             (cl-letf
                 (((symbol-function 'window-buffer)
                   (lambda (&optional _window)
                     (push :window-buffer events)
                     selected))
                  ((symbol-function 'project-current)
                   (lambda (&rest _arguments)
                     (push :unexpected-project
                           events)
                     nil))
                  ((symbol-function 'auto-read-only)
                   (lambda ()
                     (push :unexpected-action
                           events))))
               (list
                (auto-read-only--hook-find-file)
                (nreverse events)))
           (when (buffer-live-p selected)
             (kill-buffer selected))))"##,
        expect!["OK (nil (:window-buffer))"],
    )
}

fn auto_read_only_find_file_hook_suppresses_files_inside_real_shaped_project_value()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_find_file_hook_suppresses_files_inside_real_shaped_project_value",
        r##"(let (events)
         (cl-letf
             (((symbol-function 'window-buffer)
               (lambda (&optional _window)
                 (current-buffer)))
              ((symbol-function 'project-current)
               (lambda (&optional _maybe-prompt
                                  directory)
                 (push
                  (list :project directory)
                  events)
                 '(vc . "/workspace/project/")))
              ((symbol-function 'auto-read-only)
               (lambda ()
                 (push :unexpected-action
                       events))))
           (list
            (auto-read-only--hook-find-file)
            (nreverse events))))"##,
        expect!["OK (nil ((:project nil)))"],
    )
}

fn auto_read_only_find_file_hook_treats_nil_and_singleton_project_values_as_unowned()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_find_file_hook_treats_nil_and_singleton_project_values_as_unowned",
        r##"(mapcar
         (lambda (project)
           (let (calls)
             (cl-letf
                 (((symbol-function 'window-buffer)
                   (lambda (&optional _window)
                     (current-buffer)))
                  ((symbol-function 'project-current)
                   (lambda (&rest _arguments)
                     project))
                  ((symbol-function 'auto-read-only)
                   (lambda ()
                     (push project calls)
                     :applied)))
               (list
                project
                (auto-read-only--hook-find-file)
                (nreverse calls)))))
         '(nil
           (transient)
           (vc . nil)
           (vc . "/workspace/project/")))"##,
        expect![[
            r#"OK ((nil :applied (nil)) (#1=(transient) :applied (#1#)) (#2=(vc) :applied (#2#)) ((vc . "/workspace/project/") nil nil))"#
        ]],
    )
}

fn auto_read_only_find_file_hook_propagates_errors_from_each_reached_stage() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_find_file_hook_propagates_errors_from_each_reached_stage",
        r##"(list
         (cl-letf
             (((symbol-function 'window-buffer)
               (lambda (&optional _window)
                 (error "window failure"))))
           (auto-read-only-test-error-data
            #'auto-read-only--hook-find-file))
         (cl-letf
             (((symbol-function 'window-buffer)
               (lambda (&optional _window)
                 (current-buffer)))
              ((symbol-function 'project-current)
               (lambda (&rest _arguments)
                 (error "project failure"))))
           (auto-read-only-test-error-data
            #'auto-read-only--hook-find-file))
         (cl-letf
             (((symbol-function 'window-buffer)
               (lambda (&optional _window)
                 (current-buffer)))
              ((symbol-function 'project-current)
               (lambda (&rest _arguments)
                 nil))
              ((symbol-function 'auto-read-only)
               (lambda ()
                 (error "action failure"))))
           (auto-read-only-test-error-data
            #'auto-read-only--hook-find-file)))"##,
        expect![[
            r#"OK ((:error error ("window failure")) (:error error ("project failure")) (:error error ("action failure")))"#
        ]],
    )
}

fn auto_read_only_find_file_hook_uses_current_default_directory_for_project_lookup()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_read_only_find_file_hook_uses_current_default_directory_for_project_lookup",
        r##"(with-temp-buffer
         (setq default-directory
               "/workspace/project/subdir/")
         (let (arguments)
           (cl-letf
               (((symbol-function 'window-buffer)
                 (lambda (&optional _window)
                   (current-buffer)))
                ((symbol-function 'project-current)
                 (lambda (&rest values)
                   (setq arguments
                         (list
                          values
                          default-directory))
                   nil))
                ((symbol-function 'auto-read-only)
                 (lambda ()
                   :applied)))
             (list
              (auto-read-only--hook-find-file)
              arguments
              default-directory))))"##,
        expect![[
            r#"OK (:applied (nil "/workspace/project/subdir/") "/workspace/project/subdir/")"#
        ]],
    )
}

fn auto_read_only_find_file_hook_tracks_real_window_selection_across_two_buffers() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_read_only_find_file_hook_tracks_real_window_selection_across_two_buffers",
        r##"(save-window-excursion
         (let ((first
                (generate-new-buffer
                 " *auto-read-only-window-first*"))
               (second
                (generate-new-buffer
                 " *auto-read-only-window-second*"))
               events)
           (unwind-protect
               (progn
                 (delete-other-windows)
                 (set-window-buffer
                  (selected-window)
                  first)
                 (let ((other
                        (split-window-below)))
                   (set-window-buffer other second)
                   (cl-letf
                       (((symbol-function
                          'project-current)
                         (lambda (&rest _arguments)
                           nil))
                        ((symbol-function
                          'auto-read-only)
                         (lambda ()
                           (push
                            (buffer-name)
                            events)
                           :applied)))
                     (let ((nonselected
                            (with-current-buffer second
                              (auto-read-only--hook-find-file))))
                       (select-window other)
                       (let ((selected
                              (with-current-buffer second
                                (auto-read-only--hook-find-file))))
                         (list
                          nonselected
                          selected
                          (nreverse events)))))))
             (when (buffer-live-p first)
               (kill-buffer first))
             (when (buffer-live-p second)
               (kill-buffer second)))))"##,
        expect![[r#"OK (nil :applied (" *auto-read-only-window-second*"))"#]],
    )
}

pub(super) fn hook_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_read_only_find_file_hook_checks_selected_buffer_then_project_then_delegates(),
        auto_read_only_find_file_hook_short_circuits_before_project_for_nonselected_buffer(),
        auto_read_only_find_file_hook_suppresses_files_inside_real_shaped_project_value(),
        auto_read_only_find_file_hook_treats_nil_and_singleton_project_values_as_unowned(),
        auto_read_only_find_file_hook_propagates_errors_from_each_reached_stage(),
        auto_read_only_find_file_hook_uses_current_default_directory_for_project_lookup(),
        auto_read_only_find_file_hook_tracks_real_window_selection_across_two_buffers(),
    ]
}
