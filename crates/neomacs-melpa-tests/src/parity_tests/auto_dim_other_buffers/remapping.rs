use expect_test::expect;

use super::ParityBatchCase;

fn auto_dim_other_buffers_never_dim_hook_short_circuits_in_order_and_returns_hook_value()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_never_dim_hook_short_circuits_in_order_and_returns_hook_value",
        r##"(let ((buffer
                                (generate-new-buffer
                                 " *adob-hook-target*"))
                               events)
          (unwind-protect
              (progn
                (fset
                 'adob-test-never-first
                 (lambda (candidate)
                   (push
                    (list :first
                          (buffer-name candidate))
                    events)
                   nil))
                (fset
                 'adob-test-never-second
                 (lambda (candidate)
                   (push
                    (list :second
                          (buffer-name candidate))
                    events)
                   :keep-lit))
                (fset
                 'adob-test-never-third
                 (lambda (candidate)
                   (push
                    (list :third
                          (buffer-name candidate))
                    events)
                   :must-not-run))
                (let ((auto-dim-other-buffers-never-dim-buffer-functions
                       '(adob-test-never-first
                         adob-test-never-second
                         adob-test-never-third)))
                  (list
                   (adob--never-dim-p buffer)
                   (nreverse events))))
            (when (buffer-live-p buffer)
              (kill-buffer buffer))))"##,
        expect![[
            r#"OK (:keep-lit ((:first " *adob-hook-target*") (:second " *adob-hook-target*")))"#
        ]],
    )
}

fn auto_dim_other_buffers_fringe_detection_covers_legacy_pair_and_full_pair_shapes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_fringe_detection_covers_legacy_pair_and_full_pair_shapes",
        r##"(mapcar
          (lambda (faces)
            (let ((auto-dim-other-buffers-affected-faces
                   faces)
                  (adob--has-fringes
                   :before))
              (list
               faces
               (adob--has-fringes--refresh)
               adob--has-fringes)))
          '(nil
            ((default . auto-dim-other-buffers))
            ((fringe . auto-dim-other-buffers))
            ((fringe . nil))
            ((fringe . (nil . nil)))
            ((fringe . (auto-dim-other-buffers . nil)))
            ((fringe . (nil . mode-line-active)))))"##,
        expect![
            "OK ((nil nil nil) (((default . auto-dim-other-buffers)) nil nil) (((fringe . auto-dim-other-buffers)) t t) (((fringe)) nil nil) (((fringe nil)) nil nil) (((fringe auto-dim-other-buffers)) t t) (((fringe nil . mode-line-active)) t t))"
        ],
    )
}

fn auto_dim_other_buffers_positive_frame_parameter_predicate_rejects_missing_non_numeric_and_zero()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_positive_frame_parameter_predicate_rejects_missing_non_numeric_and_zero",
        r##"(mapcar
          (lambda (params)
            (list
             params
             (adob--positive-assqp
              'left-fringe
              params)
             (adob--positive-assqp
              'right-fringe
              params)))
          '(nil
            ((left-fringe . 0))
            ((left-fringe . -1))
            ((left-fringe . 1))
            ((left-fringe . 2.5)
             (right-fringe . 3))
            ((left-fringe . "4")
             (right-fringe . nil))))"##,
        expect![[
            r#"OK ((nil nil nil) (((left-fringe . 0)) nil nil) (((left-fringe . -1)) nil nil) (((left-fringe . 1)) t nil) (((left-fringe . 2.5) (right-fringe . 3)) t t) (((left-fringe . "4") (right-fringe)) nil nil))"#
        ]],
    )
}

fn auto_dim_other_buffers_remap_entry_compiler_forwards_exact_filtered_window_specs()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_remap_entry_compiler_forwards_exact_filtered_window_specs",
        r##"(let (calls)
          (cl-letf
              (((symbol-function
                 'face-remap-add-relative)
                (lambda (&rest arguments)
                  (push arguments calls)
                  (list
                   :cookie
                   (car arguments)))))
            (mapcar
             (lambda (entry)
               (list
                entry
                (adob--remap-add-relative-process-entry
                 entry)))
             '((default
                 . (auto-dim-other-buffers
                    . mode-line-active))
               (fringe
                 . auto-dim-other-buffers)
               (org-hide
                 . (nil
                    . auto-dim-other-buffers-hide))
               (mode-line
                 . (nil . nil))))
            (nreverse calls)))"##,
        expect![
            "OK ((default (:filtered (:window adob--dim nil) mode-line-active) (:filtered (:window adob--dim t) auto-dim-other-buffers)) (fringe (:filtered (:window adob--dim t) auto-dim-other-buffers)) (org-hide (:filtered (:window adob--dim nil) auto-dim-other-buffers-hide)))"
        ],
    )
}

fn auto_dim_other_buffers_real_face_remaps_add_remove_and_local_cookie_lifecycle_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_real_face_remaps_add_remove_and_local_cookie_lifecycle_match",
        r##"(with-temp-buffer
          (let ((auto-dim-other-buffers-affected-faces
                 '((default
                     . (auto-dim-other-buffers
                        . bold))
                   (mode-line
                     . (nil
                        . auto-dim-other-buffers))
                   (fringe
                     . (nil . nil)))))
            (let ((added
                   (adob--remap-add-relative)))
              (let ((active
                     (list
                      (length added)
                      (adob-test-remap-summary
                       (current-buffer)))))
                (adob--remap-remove-relative)
                (list
                 active
                 (adob-test-remap-summary
                  (current-buffer)))))))"##,
        expect![
            "OK ((2 (t 2 (default mode-line) ((mode-line ((:filtered (:window adob--dim nil) auto-dim-other-buffers))) (default nil)))) (nil 0 nil nil))"
        ],
    )
}

fn auto_dim_other_buffers_remap_faces_transitions_between_dimmed_never_dim_and_dimmed_again()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_remap_faces_transitions_between_dimmed_never_dim_and_dimmed_again",
        r##"(let ((buffer
                                (generate-new-buffer
                                 " *adob-transition*"))
                               (auto-dim-other-buffers-affected-faces
                                '((default
                                   . auto-dim-other-buffers)))
                               (auto-dim-other-buffers-never-dim-buffer-functions
                                '(adob-test-never-dim-by-name))
                               (adob-test-never-dim-names nil)
                               updates)
          (unwind-protect
              (cl-letf
                  (((symbol-function
                     'adob--force-window-update)
                    (lambda (object)
                      (push
                       (if
                           (bufferp object)
                           (buffer-name object)
                         object)
                       updates))))
                (let ((first
                       (adob--remap-faces
                        buffer
                        buffer))
                      first-state)
                  (setq first-state
                        (adob-test-remap-summary
                         buffer)
                        adob-test-never-dim-names
                        (list
                         (buffer-name buffer)))
                  (let ((second
                         (adob--remap-faces
                          buffer
                          buffer))
                        second-state)
                    (setq second-state
                          (adob-test-remap-summary
                           buffer)
                          adob-test-never-dim-names
                          nil)
                    (let ((third
                           (adob--remap-faces
                            buffer
                            buffer)))
                      (list
                       first
                       first-state
                       second
                       second-state
                       third
                       (adob-test-remap-summary
                        buffer)
                       (nreverse updates))))))
            (when (buffer-live-p buffer)
              (kill-buffer buffer))))"##,
        expect![[
            r#"OK (t (t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))) nil (nil 0 nil nil) t (t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))) (" *adob-transition*" " *adob-transition*" " *adob-transition*"))"#
        ]],
    )
}

fn auto_dim_other_buffers_remap_cycle_rebuilds_every_owned_buffer_and_skips_newly_exempt_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_remap_cycle_rebuilds_every_owned_buffer_and_skips_newly_exempt_buffer",
        r##"(let ((first
                                (generate-new-buffer
                                 " *adob-cycle-first*"))
                               (second
                                (generate-new-buffer
                                 " *adob-cycle-second*"))
                               (auto-dim-other-buffers-affected-faces
                                '((default
                                   . auto-dim-other-buffers)))
                               (auto-dim-other-buffers-never-dim-buffer-functions
                                '(adob-test-never-dim-by-name))
                               (adob-test-never-dim-names nil))
          (unwind-protect
              (progn
                (with-current-buffer first
                  (adob--remap-add-relative))
                (with-current-buffer second
                  (adob--remap-add-relative))
                (setq
                 auto-dim-other-buffers-affected-faces
                 '((default
                    . (auto-dim-other-buffers
                       . bold)))
                 adob-test-never-dim-names
                 (list
                  (buffer-name second)))
                (adob--remap-cycle-all t)
                (let ((rebuilt
                       (list
                        (adob-test-remap-summary
                         first)
                        (adob-test-remap-summary
                         second))))
                  (adob--remap-cycle-all nil)
                  (list
                   rebuilt
                   (adob-test-remap-summary
                    first)
                   (adob-test-remap-summary
                    second))))
            (when (buffer-live-p first)
              (kill-buffer first))
            (when (buffer-live-p second)
              (kill-buffer second))))"##,
        expect![
            "OK (((t 1 (default) ((default nil))) (nil 0 nil nil)) (nil 0 nil nil) (nil 0 nil nil))"
        ],
    )
}

fn auto_dim_other_buffers_force_window_update_calls_basic_refresh_then_optional_fringe_refresh()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_force_window_update_calls_basic_refresh_then_optional_fringe_refresh",
        r##"(let ((buffer
                 (generate-new-buffer
                  " *adob-force-target*"))
                events)
          (unwind-protect
              (cl-letf
                  (((symbol-function
                     'force-window-update)
                    (lambda (object)
                      (push
                       (list
                        :force
                        (if (eq object buffer)
                            :fixture-buffer
                          object))
                       events)))
                   ((symbol-function
                     'adob--force-fringes-refresh)
                    (lambda (windows)
                      (push
                       (list :fringes windows)
                       events)))
                   ((symbol-function
                     'get-buffer-window-list)
                    (lambda (&rest arguments)
                      (push
                       (list
                        :lookup
                        (cons
                         (if (eq (car arguments)
                                 buffer)
                             :fixture-buffer
                           (car arguments))
                         (cdr arguments)))
                       events)
                      '(:window-a :window-b))))
                (let ((adob--has-fringes nil))
                  (adob--force-window-update
                   :plain-object))
                (let ((adob--has-fringes t))
                  (adob--force-window-update
                   :window-object)
                  (adob--force-window-update
                   buffer))
                (nreverse events))
            (when (buffer-live-p buffer)
              (kill-buffer buffer))))"##,
        expect![[
            r#"OK ((:force :plain-object) (:force :window-object) (:lookup (:window-object nil t)) (:fringes #1=(:window-a :window-b)) (:force :fixture-buffer) (:lookup (:fixture-buffer nil t)) (:fringes #1#))"#
        ]],
    )
}

fn auto_dim_other_buffers_fringe_refresh_deduplicates_frames_and_toggles_only_positive_fringe_frames()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_fringe_refresh_deduplicates_frames_and_toggles_only_positive_fringe_frames",
        r##"(let (events)
          (cl-letf
              (((symbol-function 'window-frame)
                (lambda (window)
                  (alist-get
                   window
                   '((:window-a . :frame-a)
                     (:window-b . :frame-a)
                     (:window-c . :frame-b)
                     (:window-d . :frame-c)))))
               ((symbol-function
                 'frame-parameters)
                (lambda (frame)
                  (alist-get
                   frame
                   '((:frame-a
                      (left-fringe . 8))
                     (:frame-b
                      (right-fringe . 0))
                     (:frame-c
                      (right-fringe . 4))))))
               ((symbol-function
                 'face-attribute)
                (lambda (face attribute frame inherit)
                  (push
                   (list
                    :read
                    face
                    attribute
                    frame
                    inherit)
                   events)
                  (eq frame :frame-c)))
               ((symbol-function
                 'internal-set-lisp-face-attribute)
                (lambda (face attribute value frame)
                  (push
                   (list
                    :write
                    face
                    attribute
                    value
                    frame)
                   events))))
            (list
             (adob--force-fringes-refresh
              '(:window-a
                :window-b
                :window-c
                :window-d))
             (nreverse events))))"##,
        expect![
            "OK (nil ((:read adob--hack :inverse-video :frame-a nil) (:write adob--hack :inverse-video t :frame-a) (:read adob--hack :inverse-video :frame-c nil) (:write adob--hack :inverse-video nil :frame-c)))"
        ],
    )
}

fn auto_dim_other_buffers_kill_all_local_variables_advice_restores_real_remaps_and_kills_other_locals()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_kill_all_local_variables_advice_restores_real_remaps_and_kills_other_locals",
        r##"(with-temp-buffer
          (let ((auto-dim-other-buffers-affected-faces
                 '((default
                    . auto-dim-other-buffers))))
            (setq-local
             adob-test-unrelated-local
             :before)
            (adob--remap-add-relative)
            (let ((before
                   (adob-test-remap-summary
                    (current-buffer))))
              (adob--kill-all-local-variables-advice
               #'kill-all-local-variables)
              (list
               before
               (boundp
                'adob-test-unrelated-local)
               (local-variable-p
                'adob-test-unrelated-local)
               (adob-test-remap-summary
                (current-buffer))))))"##,
        expect![
            "OK ((t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))) nil nil (t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))))"
        ],
    )
}

pub(super) fn remapping_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dim_other_buffers_never_dim_hook_short_circuits_in_order_and_returns_hook_value(),
        auto_dim_other_buffers_fringe_detection_covers_legacy_pair_and_full_pair_shapes(),
        auto_dim_other_buffers_positive_frame_parameter_predicate_rejects_missing_non_numeric_and_zero(),
        auto_dim_other_buffers_remap_entry_compiler_forwards_exact_filtered_window_specs(),
        auto_dim_other_buffers_real_face_remaps_add_remove_and_local_cookie_lifecycle_match(),
        auto_dim_other_buffers_remap_faces_transitions_between_dimmed_never_dim_and_dimmed_again(),
        auto_dim_other_buffers_remap_cycle_rebuilds_every_owned_buffer_and_skips_newly_exempt_buffer(),
        auto_dim_other_buffers_force_window_update_calls_basic_refresh_then_optional_fringe_refresh(),
        auto_dim_other_buffers_fringe_refresh_deduplicates_frames_and_toggles_only_positive_fringe_frames(),
        auto_dim_other_buffers_kill_all_local_variables_advice_restores_real_remaps_and_kills_other_locals(),
    ]
}
