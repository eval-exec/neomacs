//! Display font-face oracles re-homed from neovm-display (LEAD 21:1xZ
//! display lib-test assignment): these tests need core boot
//! (`runtime_startup_eval_all`) and therefore live in core at TEST
//! granularity (S1-S4). Bodies are verbatim from the display source.

fn bootstrap_eval_all(src: &str) -> Vec<String> {
    crate::test_utils::runtime_startup_eval_all(src)
}

#[test]
fn set_face_attribute_accepts_only_gnu_underline_style_symbols() {
    crate::test_utils::init_test_tracing();
    let rendered = bootstrap_eval_all(
        r#"(list
             (condition-case nil
                 (progn
                   (set-face-attribute 'default nil :underline '(:style dots))
                   'ok)
               (error 'error))
             (condition-case nil
                 (progn
                   (set-face-attribute 'default nil :underline '(:style dash))
                   'ok)
               (error 'error)))"#,
    );

    assert_eq!(rendered, vec!["OK (ok error)".to_string()]);
}

#[test]
fn set_face_attribute_accepts_only_gnu_box_style_symbols() {
    crate::test_utils::init_test_tracing();
    let rendered = bootstrap_eval_all(
        r#"(list
             (condition-case nil
                 (progn
                   (set-face-attribute 'default nil :box '(:style flat-button))
                   'ok)
               (error 'error))
             (condition-case nil
                 (progn
                   (set-face-attribute 'default nil :box '(:style flat))
                   'ok)
               (error 'error))
             (condition-case nil
                 (progn
                   (set-face-attribute 'default nil :box 0)
                   'ok)
               (error 'error)))"#,
    );

    assert_eq!(rendered, vec!["OK (ok error error)".to_string()]);
}

#[test]
fn tty_default_face_color_realization_normalizes_font_attributes_like_gnu() {
    crate::test_utils::init_test_tracing();
    let rendered = bootstrap_eval_all(
        r##"(progn
             (face-spec-reset-face 'default (selected-frame))
             (set-face-attribute 'default (selected-frame)
                                 :foreground "#ffffff"
                                 :background nil
                                 :family "Terminus"
                                 :foundry "xos4"
                                 :slant 'normal
                                 :weight 'normal
                                 :height 130
                                 :width 'normal)
             (list (face-attribute 'default :family nil 'default)
                   (face-attribute 'default :foundry nil 'default)
                   (face-attribute 'default :height nil 'default)))"##,
    );

    // GNU xfaces.c routes a default-face color change through the frame
    // parameter path.  `update_face_from_frame_parameter' then calls
    // `realize_basic_faces', whose TTY branch canonicalizes the live frame's
    // family/foundry/width/height before the remaining attributes are applied.
    assert_eq!(
        rendered,
        vec![r#"OK ("default" "default" 130)"#.to_string()]
    );
}

#[test]
fn bootstrap_set_face_attribute_updates_live_mode_line_face() {
    crate::test_utils::init_test_tracing();
    let rendered = bootstrap_eval_all(
        r#"(list
             (assq :background face-x-resources)
             (progn
               (set-face-attribute 'mode-line (selected-frame)
                                   :background "grey75"
                                   :foreground "black")
               (face-background 'mode-line nil t))
             (let* ((table (frame--face-hash-table (selected-frame)))
                    (face (gethash 'mode-line table)))
               (list (aref face 9) (aref face 10))))"#,
    );

    assert_eq!(
        rendered,
        vec!["OK ((:background (\".attributeBackground\" . \"Face.AttributeBackground\")) \"grey75\" (\"black\" \"grey75\"))".to_string()]
    );
}

#[test]
fn bootstrap_frame_face_hash_table_is_frame_owned_object() {
    crate::test_utils::init_test_tracing();
    let rendered = bootstrap_eval_all(
        r#"(let ((a (frame--face-hash-table (selected-frame)))
                 (b (frame--face-hash-table (selected-frame))))
             (eq a b))"#,
    );

    assert_eq!(rendered, vec!["OK t".to_string()]);
}

// -----------------------------------------------------------------------
// Arity checks
// -----------------------------------------------------------------------
