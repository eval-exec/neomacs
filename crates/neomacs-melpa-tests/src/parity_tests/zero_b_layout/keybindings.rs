use expect_test::expect;

use super::ParityBatchCase;

fn zero_b_layout_rebinding_replaces_the_previous_prefix() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_b_layout_rebinding_replaces_the_previous_prefix",
        r##"(progn
               (0blayout-add-keybindings-with-prefix "C-x C-z")
               (list
                (mapcar
                 (lambda (key)
                   (lookup-key 0blayout-mode-map (kbd key)))
                 '("C-c C-l C-c"
                   "C-c C-l C-k"
                   "C-c C-l C-b"))
                (mapcar
                 (lambda (key)
                   (lookup-key 0blayout-mode-map (kbd key)))
                 '("C-x C-z C-c"
                   "C-x C-z C-k"
                   "C-x C-z C-b"))))"##,
        expect!["OK ((1 1 1) (0blayout-new 0blayout-kill 0blayout-switch))"],
    )
}

fn zero_b_layout_rebinding_honors_a_replaced_key_specification() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_b_layout_rebinding_honors_a_replaced_key_specification",
        r##"(let ((0blayout-keys-map
                      '(("n" . next-line)
                        ("p" . previous-line)
                        ("RET" . newline))))
               (0blayout-add-keybindings-with-prefix "C-c z")
               (list
                (lookup-key 0blayout-mode-map (kbd "C-c z n"))
                (lookup-key 0blayout-mode-map (kbd "C-c z p"))
                (lookup-key 0blayout-mode-map (kbd "C-c z RET"))
                (lookup-key
                 0blayout-mode-map
                 (kbd "C-c C-l C-c"))))"##,
        expect!["OK (next-line previous-line newline 2)"],
    )
}

fn zero_b_layout_accepts_an_empty_binding_specification() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_b_layout_accepts_an_empty_binding_specification",
        r##"(let ((0blayout-keys-map nil))
               (0blayout-add-keybindings-with-prefix "C-c z")
               (list
                (keymapp 0blayout-mode-map)
                (cdr 0blayout-mode-map)
                (lookup-key
                 0blayout-mode-map
                 (kbd "C-c C-l C-c"))))"##,
        expect!["OK (t nil 1)"],
    )
}

fn zero_b_layout_rebinding_accepts_an_unterminated_event_name() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_b_layout_rebinding_accepts_an_unterminated_event_name",
        r##"(0blayout-add-keybindings-with-prefix "<definitely-not-a-key")"##,
        expect!["OK nil"],
    )
}

pub(super) fn keybindings_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        zero_b_layout_rebinding_replaces_the_previous_prefix(),
        zero_b_layout_rebinding_honors_a_replaced_key_specification(),
        zero_b_layout_accepts_an_empty_binding_specification(),
        zero_b_layout_rebinding_accepts_an_unterminated_event_name(),
    ]
}
