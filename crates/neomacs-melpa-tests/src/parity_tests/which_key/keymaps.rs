use expect_test::expect;

use super::ParityBatchCase;

fn which_key_keymap_binding_extraction_covers_binding_definition_shapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_keymap_binding_extraction_covers_binding_definition_shapes",
        r##"(let ((map (make-sparse-keymap)))
               (define-key map "a" #'forward-char)
               (define-key map "b" "literal")
               (define-key map "c" [123 45 6])
               (define-key map "d" (lambda () (interactive)))
               (define-key map "e" '("Named" . backward-char))
               (define-key map "f" '("Group" . (keymap)))
               (define-key map "g" '(menu-item "Menu" next-line))
               (define-key map "h" #'self-insert-command)
               (define-key map "i" #'ignore)
               (sort
                (which-key--get-keymap-bindings map)
                (lambda (a b) (string-lessp (car a) (car b)))))"##,
        expect![[
            r#"OK (("a" . "forward-char") ("b" . "literal") ("c" . "{ - C-f") ("d" . "function") ("e" . "Named") ("f" . "group:Group") ("g" . "next-line"))"#
        ]],
    )
}

fn which_key_recursive_binding_extraction_distinguishes_top_level_and_all() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_recursive_binding_extraction_distinguishes_top_level_and_all",
        r##"(let ((map (make-sparse-keymap)))
               (define-key map "c" "c")
               (define-key map "dd" "dd")
               (define-key map "eee" "eee")
               (define-key map (kbd "M-g g") "M-gg")
               (list
                (sort
                 (which-key--get-keymap-bindings map)
                 (lambda (a b) (string-lessp (car a) (car b))))
                (sort
                 (which-key--get-keymap-bindings map nil nil nil t)
                 (lambda (a b) (string-lessp (car a) (car b))))))"##,
        expect![[
            r#"OK ((("M-g" . "prefix") ("c" . "c") ("d" . "prefix") ("e" . "prefix")) (("M-g g" . "M-gg") ("c" . "c") ("d d" . "dd") ("e e e" . "eee")))"#
        ]],
    )
}

fn which_key_keymap_binding_extraction_applies_prefix_start_and_filter_arguments() -> ParityBatchCase
{
    ParityBatchCase::value(
        "which_key_keymap_binding_extraction_applies_prefix_start_and_filter_arguments",
        r##"(let ((map (make-sparse-keymap)))
               (define-key map (kbd "C-c a") #'forward-char)
               (define-key map (kbd "C-c b") #'backward-char)
               (define-key map (kbd "C-c c") #'next-line)
               (list
                (sort
                 (which-key--get-keymap-bindings
                  map
                  '(("existing" . "value"))
                  (kbd "C-c")
                  (lambda (binding)
                    (string-match-p "ward" (cdr binding)))
                  t)
                 (lambda (a b) (string-lessp (car a) (car b))))
                (which-key--get-keymap-bindings
                 map nil (kbd "C-x") nil t)))"##,
        expect![[
            r#"OK ((("C-c a" . "forward-char") ("C-c b" . "backward-char") ("existing" . "value")) nil)"#
        ]],
    )
}

fn which_key_recursive_definition_reaches_every_nested_map_and_optional_root() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_recursive_definition_reaches_every_nested_map_and_optional_root",
        r##"(let ((root (make-sparse-keymap))
                    (first (make-sparse-keymap))
                    (second (make-sparse-keymap)))
               (define-key first "n" second)
               (define-key root "p" first)
               (which-key-define-key-recursively root (kbd "x") #'forward-char)
               (let ((without-root
                      (list
                       (lookup-key root (kbd "x"))
                       (lookup-key first (kbd "x"))
                       (lookup-key second (kbd "x")))))
                 (which-key-define-key-recursively
                  root (kbd "y") #'backward-char t)
                 (list
                  without-root
                  (lookup-key root (kbd "y"))
                  (lookup-key first (kbd "y"))
                  (lookup-key second (kbd "y")))))"##,
        expect!["OK ((nil forward-char forward-char) backward-char backward-char backward-char)"],
    )
}

fn which_key_map_binding_predicate_matches_commands_prefixes_and_absences() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_map_binding_predicate_matches_commands_prefixes_and_absences",
        r##"(let ((map (make-sparse-keymap))
                    (which-key--pages-obj
                     (make-which-key--pages :prefix [])))
               (define-key map "a" #'forward-char)
               (define-key map "p" (make-sparse-keymap))
               (list
                (which-key--map-binding-p map '("a" . "forward-char"))
                (which-key--map-binding-p map '("a" . "backward-char"))
                (which-key--map-binding-p map '("p" . "Prefix Command"))
                (which-key--map-binding-p map '("z" . "nil"))))"##,
        expect!["OK (t nil t t)"],
    )
}

fn which_key_get_bindings_rejects_a_non_keymap_object() -> ParityBatchCase {
    ParityBatchCase::signal(
        "which_key_get_bindings_rejects_a_non_keymap_object",
        r##"(which-key--get-bindings nil 'not-a-keymap)"##,
        expect![[r#"ERR (error "not-a-keymap is not a keymap")"#]],
    )
}

fn which_key_public_show_commands_forward_exact_arguments_to_the_display_boundary()
-> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_public_show_commands_forward_exact_arguments_to_the_display_boundary",
        r##"(progn
               (defvar neomacs-which-key-map)
               (setq neomacs-which-key-map (make-sparse-keymap))
               (let (calls)
               (define-key neomacs-which-key-map "a" #'forward-char)
               (cl-letf (((symbol-function 'which-key--show-keymap)
                          (lambda (&rest args)
                            (push args calls))))
                 (which-key-show-keymap 'neomacs-which-key-map t)
                 (which-key-show-full-keymap 'neomacs-which-key-map)
                 (list
                  (nreverse calls)
                  (keymapp neomacs-which-key-map)))))"##,
        expect![[
            r#"OK ((("neomacs-which-key-map" #1=(keymap (97 . forward-char)) nil nil t) ("neomacs-which-key-map" #1# nil t)) t)"#
        ]],
    )
}

fn which_key_major_mode_show_handles_present_and_missing_maps() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_major_mode_show_handles_present_and_missing_maps",
        r##"(let ((neomacs-which-key-mode-map
                     (make-sparse-keymap))
                    (major-mode 'neomacs-which-key-mode)
                    calls messages)
               (cl-letf (((symbol-function 'which-key--show-keymap)
                          (lambda (&rest args)
                            (push args calls)))
                         ((symbol-function 'message)
                          (lambda (format-string &rest args)
                            (push (apply #'format format-string args)
                                  messages))))
                 (which-key-show-major-mode)
                 (which-key-show-major-mode t)
                 (setq major-mode 'neomacs-which-key-missing-mode)
                 (which-key-show-major-mode)
                 (list (nreverse calls) (nreverse messages))))"##,
        expect![[
            r#"OK (nil ("which-key: No map named neomacs-which-key-mode-map" "which-key: No map named neomacs-which-key-mode-map" "which-key: No map named neomacs-which-key-missing-mode-map"))"#
        ]],
    )
}

pub(super) fn keymaps_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        which_key_keymap_binding_extraction_covers_binding_definition_shapes(),
        which_key_recursive_binding_extraction_distinguishes_top_level_and_all(),
        which_key_keymap_binding_extraction_applies_prefix_start_and_filter_arguments(),
        which_key_recursive_definition_reaches_every_nested_map_and_optional_root(),
        which_key_map_binding_predicate_matches_commands_prefixes_and_absences(),
        which_key_get_bindings_rejects_a_non_keymap_object(),
        which_key_public_show_commands_forward_exact_arguments_to_the_display_boundary(),
        which_key_major_mode_show_handles_present_and_missing_maps(),
    ]
}
