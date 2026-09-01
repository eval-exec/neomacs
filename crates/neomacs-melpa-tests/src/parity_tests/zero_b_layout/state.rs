use expect_test::expect;

use super::ParityBatchCase;

fn zero_b_layout_exposes_its_documented_defaults() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_b_layout_exposes_its_documented_defaults",
        r##"(list
               0blayout-alist
               0blayout-default
               0blayout-keys-map
               (keymapp 0blayout-mode-map)
               0blayout-mode
               (mapcar
                (lambda (key)
                  (lookup-key 0blayout-mode-map (kbd key)))
                '("C-c C-l C-c"
                  "C-c C-l C-k"
                  "C-c C-l C-b")))"##,
        expect![[
            r#"OK (nil "default" (("C-c" . 0blayout-new) ("C-k" . 0blayout-kill) ("C-b" . 0blayout-switch)) t nil (0blayout-new 0blayout-kill 0blayout-switch))"#
        ]],
    )
    .fresh_process()
}

fn zero_b_layout_get_current_name_falls_back_to_the_custom_default() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_b_layout_get_current_name_falls_back_to_the_custom_default",
        r##"(let ((0blayout-default "fallback"))
               (set-frame-parameter nil '0blayout-current nil)
               (list
                (frame-parameter nil '0blayout-current)
                (0blayout-get-current-name)))"##,
        expect![[r#"OK (nil "fallback")"#]],
    )
}

fn zero_b_layout_set_current_name_round_trips_frame_local_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_b_layout_set_current_name_round_trips_frame_local_values",
        r##"(progn
               (set-frame-parameter nil '0blayout-current nil)
               (let ((first (0blayout-set-current-name "work"))
                     after-first
                     second)
                 (setq after-first
                       (list
                        (frame-parameter nil '0blayout-current)
                        (0blayout-get-current-name)))
                 (setq second
                       (0blayout-set-current-name "review"))
                 (list
                  first
                  after-first
                  second
                  (0blayout-get-current-name))))"##,
        expect![[r#"OK (nil ("work" "work") nil "review")"#]],
    )
}

fn zero_b_layout_mode_toggles_its_global_binding_map() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_b_layout_mode_toggles_its_global_binding_map",
        r##"(unwind-protect
               (progn
                 (0blayout-mode -1)
                 (let ((disabled
                        (list
                         0blayout-mode
                         (key-binding (kbd "C-c C-l C-c"))))
                       enabled
                       disabled-again)
                   (0blayout-mode 1)
                   (setq enabled
                         (list
                          0blayout-mode
                          (key-binding (kbd "C-c C-l C-c"))))
                   (0blayout-mode -1)
                   (setq disabled-again
                         (list
                          0blayout-mode
                          (key-binding (kbd "C-c C-l C-c"))))
                   (list disabled enabled disabled-again)))
             (0blayout-mode -1))"##,
        expect!["OK ((nil nil) (t 0blayout-new) (nil nil))"],
    )
    .fresh_process()
}

fn zero_b_layout_custom_metadata_matches_the_package_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_b_layout_custom_metadata_matches_the_package_contract",
        r##"(list
               (not
                (null
                 (custom-variable-p '0blayout-default)))
               (get '0blayout-default 'custom-type)
               (copy-tree
                (get '0blayout-default 'standard-value))
               (get '0blayout-mode 'custom-type)
               (copy-tree
                (get '0blayout-mode 'standard-value)))"##,
        expect![[r#"OK (t string ("default") boolean (nil))"#]],
    )
}

pub(super) fn state_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        zero_b_layout_exposes_its_documented_defaults(),
        zero_b_layout_get_current_name_falls_back_to_the_custom_default(),
        zero_b_layout_set_current_name_round_trips_frame_local_values(),
        zero_b_layout_mode_toggles_its_global_binding_map(),
        zero_b_layout_custom_metadata_matches_the_package_contract(),
    ]
}
