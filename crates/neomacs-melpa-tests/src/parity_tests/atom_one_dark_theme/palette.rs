use expect_test::expect;

use super::ParityBatchCase;

fn atom_one_dark_theme_complete_palette_key_order_values_and_uniqueness_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_complete_palette_key_order_values_and_uniqueness_match",
        r##"(list
         (display-color-cells
          (selected-frame))
         (length
          atom-one-dark-colors-alist)
         atom-one-dark-colors-alist
         (length
          (delete-dups
           (mapcar #'car
                   (copy-tree
                    atom-one-dark-colors-alist))))
         (seq-every-p #'stringp
                      (mapcar #'car
                              atom-one-dark-colors-alist))
         (seq-every-p #'stringp
                      (mapcar #'cdr
                              atom-one-dark-colors-alist)))"##,
        expect![[
            r##"OK (0 30 (("atom-one-dark-accent" . "#528BFF") ("atom-one-dark-fg" if nil "color-248" "#ABB2BF") ("atom-one-dark-bg" if nil "color-235" "#282C34") ("atom-one-dark-bg-1" if nil "color-234" "#121417") ("atom-one-dark-bg-hl" if nil "color-236" "#2C323C") ("atom-one-dark-gutter" if nil "color-239" "#4B5363") ("atom-one-dark-insert" . "#43D08A") ("atom-one-dark-change" . "#E0C285") ("atom-one-dark-delete" . "#E05252") ("atom-one-dark-info" . "#6494ED") ("atom-one-dark-success" . "#73C900") ("atom-one-dark-warning" . "#E2C08D") ("atom-one-dark-error" . "#FF6347") ("atom-one-dark-mono-1" if nil "color-248" "#ABB2BF") ("atom-one-dark-mono-2" if nil "color-244" "#828997") ("atom-one-dark-mono-3" if nil "color-240" "#5C6370") ("atom-one-dark-cyan" . "#56B6C2") ("atom-one-dark-blue" . "#61AFEF") ("atom-one-dark-purple" . "#C678DD") ("atom-one-dark-green" . "#98C379") ("atom-one-dark-red-1" . "#E06C75") ("atom-one-dark-red-2" . "#BE5046") ("atom-one-dark-orange-1" . "#D19A66") ("atom-one-dark-orange-2" . "#E5C07B") ("atom-one-dark-gray" if nil "color-237" "#3E4451") ("atom-one-dark-silver" if nil "color-247" "#9DA5B4") ("atom-one-dark-black" if nil "color-233" "#21252B") ("atom-one-dark-ui-fg" if nil "color-247" "#9DA5B4") ("atom-one-dark-level-3-color" if nil "color-233" "#21252B") ("atom-one-dark-border" if nil "color-232" "#181A1F")) 30 t nil)"##
        ]],
    )
}

fn atom_one_dark_theme_color_macro_expansion_arglist_documentation_and_indent_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_color_macro_expansion_arglist_documentation_and_indent_match",
        r##"(list
         (macrop
          'atom-one-dark-with-color-variables)
         (prin1-to-string
          (help-function-arglist
           'atom-one-dark-with-color-variables
           t))
         (documentation
          'atom-one-dark-with-color-variables
          t)
         (get
          'atom-one-dark-with-color-variables
          'lisp-indent-function)
         (macroexpand
          '(atom-one-dark-with-color-variables
             (list
              class
              atom-one-dark-fg
              atom-one-dark-bg
              atom-one-dark-accent))))"##,
        expect![[
            r##"OK (t "(&rest body)" "Bind the colors list around BODY." 0 (let ((class '((class color) (min-colors 89))) (atom-one-dark-accent "#528BFF") (atom-one-dark-fg (if nil "color-248" "#ABB2BF")) (atom-one-dark-bg (if nil "color-235" "#282C34")) (atom-one-dark-bg-1 (if nil "color-234" "#121417")) (atom-one-dark-bg-hl (if nil "color-236" "#2C323C")) (atom-one-dark-gutter (if nil "color-239" "#4B5363")) (atom-one-dark-insert "#43D08A") (atom-one-dark-change "#E0C285") (atom-one-dark-delete "#E05252") (atom-one-dark-info "#6494ED") (atom-one-dark-success "#73C900") (atom-one-dark-warning "#E2C08D") (atom-one-dark-error "#FF6347") (atom-one-dark-mono-1 (if nil "color-248" "#ABB2BF")) (atom-one-dark-mono-2 (if nil "color-244" "#828997")) (atom-one-dark-mono-3 (if nil "color-240" "#5C6370")) (atom-one-dark-cyan "#56B6C2") (atom-one-dark-blue "#61AFEF") (atom-one-dark-purple "#C678DD") (atom-one-dark-green "#98C379") (atom-one-dark-red-1 "#E06C75") (atom-one-dark-red-2 "#BE5046") (atom-one-dark-orange-1 "#D19A66") (atom-one-dark-orange-2 "#E5C07B") (atom-one-dark-gray (if nil "color-237" "#3E4451")) (atom-one-dark-silver (if nil "color-247" "#9DA5B4")) (atom-one-dark-black (if nil "color-233" "#21252B")) (atom-one-dark-ui-fg (if nil "color-247" "#9DA5B4")) (atom-one-dark-level-3-color (if nil "color-233" "#21252B")) (atom-one-dark-border (if nil "color-232" "#181A1F"))) (list class atom-one-dark-fg atom-one-dark-bg atom-one-dark-accent)))"##
        ]],
    )
}

fn atom_one_dark_theme_color_macro_binds_all_thirty_symbols_without_leaking_them() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_one_dark_theme_color_macro_binds_all_thirty_symbols_without_leaking_them",
        r##"(let ((symbols
                (mapcar
                 (lambda (entry)
                   (intern
                    (car entry)))
                 atom-one-dark-colors-alist)))
         (list
          (mapcar
           (lambda (symbol)
             (list
              symbol
              (boundp symbol)))
           symbols)
          (atom-one-dark-with-color-variables
            (cl-mapcar
             (lambda (symbol value)
               (list symbol value))
             symbols
             (list
              atom-one-dark-accent
              atom-one-dark-fg
              atom-one-dark-bg
              atom-one-dark-bg-1
              atom-one-dark-bg-hl
              atom-one-dark-gutter
              atom-one-dark-insert
              atom-one-dark-change
              atom-one-dark-delete
              atom-one-dark-info
              atom-one-dark-success
              atom-one-dark-warning
              atom-one-dark-error
              atom-one-dark-mono-1
              atom-one-dark-mono-2
              atom-one-dark-mono-3
              atom-one-dark-cyan
              atom-one-dark-blue
              atom-one-dark-purple
              atom-one-dark-green
              atom-one-dark-red-1
              atom-one-dark-red-2
              atom-one-dark-orange-1
              atom-one-dark-orange-2
              atom-one-dark-gray
              atom-one-dark-silver
              atom-one-dark-black
              atom-one-dark-ui-fg
              atom-one-dark-level-3-color
              atom-one-dark-border)))
          (mapcar
           (lambda (symbol)
             (list
              symbol
              (boundp symbol)))
           symbols)))"##,
        expect![[
            r##"OK (((atom-one-dark-accent nil) (atom-one-dark-fg nil) (atom-one-dark-bg nil) (atom-one-dark-bg-1 nil) (atom-one-dark-bg-hl nil) (atom-one-dark-gutter nil) (atom-one-dark-insert nil) (atom-one-dark-change nil) (atom-one-dark-delete nil) (atom-one-dark-info nil) (atom-one-dark-success nil) (atom-one-dark-warning nil) (atom-one-dark-error nil) (atom-one-dark-mono-1 nil) (atom-one-dark-mono-2 nil) (atom-one-dark-mono-3 nil) (atom-one-dark-cyan nil) (atom-one-dark-blue nil) (atom-one-dark-purple nil) (atom-one-dark-green nil) (atom-one-dark-red-1 nil) (atom-one-dark-red-2 nil) (atom-one-dark-orange-1 nil) (atom-one-dark-orange-2 nil) (atom-one-dark-gray nil) (atom-one-dark-silver nil) (atom-one-dark-black nil) (atom-one-dark-ui-fg nil) (atom-one-dark-level-3-color nil) (atom-one-dark-border nil)) ((atom-one-dark-accent "#528BFF") (atom-one-dark-fg "#ABB2BF") (atom-one-dark-bg "#282C34") (atom-one-dark-bg-1 "#121417") (atom-one-dark-bg-hl "#2C323C") (atom-one-dark-gutter "#4B5363") (atom-one-dark-insert "#43D08A") (atom-one-dark-change "#E0C285") (atom-one-dark-delete "#E05252") (atom-one-dark-info "#6494ED") (atom-one-dark-success "#73C900") (atom-one-dark-warning "#E2C08D") (atom-one-dark-error "#FF6347") (atom-one-dark-mono-1 "#ABB2BF") (atom-one-dark-mono-2 "#828997") (atom-one-dark-mono-3 "#5C6370") (atom-one-dark-cyan "#56B6C2") (atom-one-dark-blue "#61AFEF") (atom-one-dark-purple "#C678DD") (atom-one-dark-green "#98C379") (atom-one-dark-red-1 "#E06C75") (atom-one-dark-red-2 "#BE5046") (atom-one-dark-orange-1 "#D19A66") (atom-one-dark-orange-2 "#E5C07B") (atom-one-dark-gray "#3E4451") (atom-one-dark-silver "#9DA5B4") (atom-one-dark-black "#21252B") (atom-one-dark-ui-fg "#9DA5B4") (atom-one-dark-level-3-color "#21252B") (atom-one-dark-border "#181A1F")) ((atom-one-dark-accent nil) (atom-one-dark-fg nil) (atom-one-dark-bg nil) (atom-one-dark-bg-1 nil) (atom-one-dark-bg-hl nil) (atom-one-dark-gutter nil) (atom-one-dark-insert nil) (atom-one-dark-change nil) (atom-one-dark-delete nil) (atom-one-dark-info nil) (atom-one-dark-success nil) (atom-one-dark-warning nil) (atom-one-dark-error nil) (atom-one-dark-mono-1 nil) (atom-one-dark-mono-2 nil) (atom-one-dark-mono-3 nil) (atom-one-dark-cyan nil) (atom-one-dark-blue nil) (atom-one-dark-purple nil) (atom-one-dark-green nil) (atom-one-dark-red-1 nil) (atom-one-dark-red-2 nil) (atom-one-dark-orange-1 nil) (atom-one-dark-orange-2 nil) (atom-one-dark-gray nil) (atom-one-dark-silver nil) (atom-one-dark-black nil) (atom-one-dark-ui-fg nil) (atom-one-dark-level-3-color nil) (atom-one-dark-border nil)))"##
        ]],
    )
}

fn atom_one_dark_theme_color_macro_uses_palette_present_when_form_is_expanded() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_color_macro_uses_palette_present_when_form_is_expanded",
        r##"(let ((original
                (copy-tree
                 atom-one-dark-colors-alist)))
         (unwind-protect
             (progn
               (setcdr
                (assoc
                 "atom-one-dark-accent"
                 atom-one-dark-colors-alist)
                "#010203")
               (setcdr
                (assoc
                 "atom-one-dark-fg"
                 atom-one-dark-colors-alist)
                "#111213")
               (list
                (eval
                 '(atom-one-dark-with-color-variables
                    (list
                     atom-one-dark-accent
                     atom-one-dark-fg
                     atom-one-dark-bg)))
                (cdr
                 (assoc
                  "atom-one-dark-accent"
                  atom-one-dark-colors-alist))))
           (setq atom-one-dark-colors-alist
                 original)))"##,
        expect![[r##"OK (("#010203" "#111213" "#282C34") "#010203")"##]],
    )
}

fn atom_one_dark_theme_forced_256_color_reload_recomputes_palette_and_new_face_specs()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_forced_256_color_reload_recomputes_palette_and_new_face_specs",
        r##"(let ((source
                (getenv "NEOMACS_PACKAGE_SOURCE")))
         (makunbound
          'atom-one-dark-colors-alist)
         (cl-letf
             (((symbol-function
                'display-color-cells)
               (lambda (&optional _display)
                 256)))
           (load source nil t t))
         (list
          atom-one-dark-colors-alist
          (car
           (last
            (atom-one-dark-test-face-specs
             'default)))
          (car
           (last
            (atom-one-dark-test-face-specs
             'font-lock-comment-face)))
          (car
           (last
            (atom-one-dark-test-face-specs
             'mode-line)))
          (car
           (last
            (atom-one-dark-test-face-specs
             'line-number)))))"##,
        expect![[
            r##"OK ((("atom-one-dark-accent" . "#528BFF") ("atom-one-dark-fg" if t "color-248" "#ABB2BF") ("atom-one-dark-bg" if t "color-235" "#282C34") ("atom-one-dark-bg-1" if t "color-234" "#121417") ("atom-one-dark-bg-hl" if t "color-236" "#2C323C") ("atom-one-dark-gutter" if t "color-239" "#4B5363") ("atom-one-dark-insert" . "#43D08A") ("atom-one-dark-change" . "#E0C285") ("atom-one-dark-delete" . "#E05252") ("atom-one-dark-info" . "#6494ED") ("atom-one-dark-success" . "#73C900") ("atom-one-dark-warning" . "#E2C08D") ("atom-one-dark-error" . "#FF6347") ("atom-one-dark-mono-1" if t "color-248" "#ABB2BF") ("atom-one-dark-mono-2" if t "color-244" "#828997") ("atom-one-dark-mono-3" if t "color-240" "#5C6370") ("atom-one-dark-cyan" . "#56B6C2") ("atom-one-dark-blue" . "#61AFEF") ("atom-one-dark-purple" . "#C678DD") ("atom-one-dark-green" . "#98C379") ("atom-one-dark-red-1" . "#E06C75") ("atom-one-dark-red-2" . "#BE5046") ("atom-one-dark-orange-1" . "#D19A66") ("atom-one-dark-orange-2" . "#E5C07B") ("atom-one-dark-gray" if t "color-237" "#3E4451") ("atom-one-dark-silver" if t "color-247" "#9DA5B4") ("atom-one-dark-black" if t "color-233" "#21252B") ("atom-one-dark-ui-fg" if t "color-247" "#9DA5B4") ("atom-one-dark-level-3-color" if t "color-233" "#21252B") ("atom-one-dark-border" if t "color-232" "#181A1F")) ((t (:foreground "color-248" :background "color-235"))) ((t (:foreground "color-240" :slant italic))) ((t (:background "color-233" :foreground "color-247" :box (:color "color-232" :line-width 1)))) ((t (:foreground "color-239" :background "color-235"))))"##
        ]],
    )
}

fn atom_one_dark_theme_color_macro_duplicate_palette_keys_follow_let_binding_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_color_macro_duplicate_palette_keys_follow_let_binding_order",
        r##"(let ((atom-one-dark-colors-alist
                '(("atom-one-dark-fg" . "first")
                  ("atom-one-dark-fg" . "second"))))
         (list
          (macroexpand
           '(atom-one-dark-with-color-variables
              atom-one-dark-fg))
          (eval
           '(atom-one-dark-with-color-variables
              atom-one-dark-fg))))"##,
        expect![[
            r#"OK ((let ((class '((class color) (min-colors 89))) (atom-one-dark-fg "first") (atom-one-dark-fg "second")) atom-one-dark-fg) "second")"#
        ]],
    )
}

fn atom_one_dark_theme_color_macro_malformed_palette_entries_signal_exact_errors() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_one_dark_theme_color_macro_malformed_palette_entries_signal_exact_errors",
        r##"(mapcar
         (lambda (palette)
           (let ((atom-one-dark-colors-alist
                  palette))
             (atom-one-dark-test-error
              (lambda ()
                (macroexpand
                 '(atom-one-dark-with-color-variables
                    atom-one-dark-fg))))))
         '(((42 . "#fff"))
           ((nil . "#fff"))
           (not-a-cons)
           nil))"##,
        expect![
            "OK ((:signal wrong-type-argument (stringp 42)) (:signal wrong-type-argument (stringp nil)) (:signal wrong-type-argument (listp not-a-cons)) (:ok (let ((class '((class color) (min-colors 89)))) atom-one-dark-fg)))"
        ],
    )
}

pub(super) fn palette_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atom_one_dark_theme_complete_palette_key_order_values_and_uniqueness_match(),
        atom_one_dark_theme_color_macro_expansion_arglist_documentation_and_indent_match(),
        atom_one_dark_theme_color_macro_binds_all_thirty_symbols_without_leaking_them(),
        atom_one_dark_theme_color_macro_uses_palette_present_when_form_is_expanded(),
        atom_one_dark_theme_forced_256_color_reload_recomputes_palette_and_new_face_specs(),
        atom_one_dark_theme_color_macro_duplicate_palette_keys_follow_let_binding_order(),
        atom_one_dark_theme_color_macro_malformed_palette_entries_signal_exact_errors(),
    ]
}
