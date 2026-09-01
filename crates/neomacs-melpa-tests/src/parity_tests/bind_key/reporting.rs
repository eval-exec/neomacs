use expect_test::expect;

use super::ParityBatchCase;

fn bind_key_descriptions_cover_symbols_lambdas_closures_keymaps_and_byte_code() -> ParityBatchCase {
    ParityBatchCase::value(
        "bind_key_descriptions_cover_symbols_lambdas_closures_keymaps_and_byte_code",
        r##"(let ((map-symbol
                    (make-symbol "neomacs-described-map")))
               (set map-symbol (make-sparse-keymap))
               (fset map-symbol (symbol-value map-symbol))
               (put map-symbol 'variable-documentation
                    "A documented keymap")
               (list
                (let ((bind-key-describe-special-forms nil))
                  (list
                   (get-binding-description 'forward-char)
                   (get-binding-description
                    '(lambda () "Lambda documentation" (interactive)))
                   (get-binding-description
                    '(closure nil nil "Closure documentation"
                              (interactive)))
                   (get-binding-description '(keymap))
                   (get-binding-description
                    (byte-compile
                     (lambda () (interactive))))))
                (let ((bind-key-describe-special-forms t))
                  (list
                   (get-binding-description
                    '(lambda () "Lambda documentation" (interactive)))
                   (get-binding-description
                    '(closure nil nil "Closure documentation"
                              (interactive)))
                   (get-binding-description map-symbol)))))"##,
        expect![[
            r##"OK ((forward-char "#<lambda>" "#<closure>" "#<keymap>" "#<byte-compiled lambda>") ("Lambda documentation" "Closure documentation" "A documented keymap"))"##
        ]],
    )
}

fn compare_keybindings_reports_order_and_group_boundaries_for_maps_and_prefixes() -> ParityBatchCase
{
    ParityBatchCase::value(
        "compare_keybindings_reports_order_and_group_boundaries_for_maps_and_prefixes",
        r##"(let ((bind-key-segregation-regexp
                    "\\`\\(?:C-c \\|M-g \\)"))
               (mapcar
                (lambda (pair)
                  (compare-keybindings (car pair) (cadr pair)))
                '(((("a") ignore nil)
                   (("C-c a") ignore nil))
                  ((("C-c a") ignore nil)
                   (("M-g a") ignore nil))
                  ((("C-c a" . map-z) ignore nil)
                   (("C-c a" . map-a) ignore nil))
                  ((("C-c a" . map-a) ignore nil)
                   (("C-c b" . map-a) ignore nil))
                  ((("C-c b" . map-a) ignore nil)
                   (("C-c a" . map-a) ignore nil)))))"##,
        expect![[r#"OK ((t . t) (t . t) (nil . t) (t) (nil))"#]],
    )
}

fn describe_personal_keybindings_reports_original_current_and_rebound_commands() -> ParityBatchCase
{
    ParityBatchCase::value(
        "describe_personal_keybindings_reports_original_current_and_rebound_commands",
        r##"(progn
               (defvar neomacs-bind-key-report-map
                 (make-sparse-keymap))
               (define-key neomacs-bind-key-report-map "a"
                           #'beginning-of-line)
               (let ((personal-keybindings nil)
                     (bind-key-column-widths '(12 . 28)))
                 (bind-key "a" #'forward-char
                           'neomacs-bind-key-report-map)
                 (bind-key "b" #'backward-char
                           'neomacs-bind-key-report-map)
                 (define-key neomacs-bind-key-report-map "b"
                             #'end-of-line)
                 (describe-personal-keybindings)
                 (with-current-buffer "*Personal Keybindings*"
                   (buffer-substring-no-properties
                    (point-min) (point-max)))))"##,
        expect![[
            r#"OK "Key name    Command                     Comments\n----------- --------------------------- ---------------------\n\n\nneomacs-bind-key-report-map: a\n-------------------------------------------------------------\n\na           `forward-char'              was `beginning-of-line'\nb           `backward-char'             [now: `end-of-line']\n""#
        ]],
    )
}

fn bind_key_registry_distinguishes_global_symbol_and_direct_map_descriptors() -> ParityBatchCase {
    ParityBatchCase::value(
        "bind_key_registry_distinguishes_global_symbol_and_direct_map_descriptors",
        r##"(progn
               (defvar neomacs-bind-key-symbol-map
                 (make-sparse-keymap))
               (let ((personal-keybindings nil)
                     (direct-map (make-sparse-keymap)))
                 (bind-key "C-c g" #'forward-line)
                 (bind-key "x" #'forward-char
                           'neomacs-bind-key-symbol-map)
                 (bind-key "y" #'backward-char direct-map)
                 (mapcar
                  (lambda (entry)
                    (list
                     (caar entry)
                     (let ((descriptor (cdar entry)))
                       (cond
                        ((null descriptor) 'global)
                        ((symbolp descriptor) descriptor)
                        ((keymapp descriptor) 'direct-keymap)
                        (t descriptor)))
                     (nth 1 entry)
                     (nth 2 entry)))
                  (reverse personal-keybindings))))"##,
        expect![[
            r#"OK (("C-c g" global forward-line nil) ("x" neomacs-bind-key-symbol-map forward-char nil) ("y" direct-map backward-char nil))"#
        ]],
    )
}

pub(super) fn reporting_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        bind_key_descriptions_cover_symbols_lambdas_closures_keymaps_and_byte_code(),
        compare_keybindings_reports_order_and_group_boundaries_for_maps_and_prefixes(),
        describe_personal_keybindings_reports_original_current_and_rebound_commands(),
        bind_key_registry_distinguishes_global_symbol_and_direct_map_descriptors(),
    ]
}
