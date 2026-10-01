//! Font selection observed through the frame published for rendering.
use super::*;

fn circle_fixture() -> (Context, neovm_core::window::FrameId, LayoutEngine) {
    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("org-circle-font", 640, 160, buffer_id);
    realize_test_gui_frame(&mut eval, frame_id);
    bind_minibuffer_buffer(&mut eval, frame_id);
    eval.eval_str(
        r#"(progn
          (internal-set-lisp-face-attribute 'default :family
                                          "JetBrainsMono Nerd Font" (selected-frame))
          (setq use-default-font-for-symbols nil)
          (setq font-encoding-alist '(("iso10646-1$" . (unicode-bmp . nil))))
          (set-fontset-font t #x25cb '(nil . "iso10646-1"))
          (insert "** Circle\n○ Plain\n")
          (set (make-local-variable 'face-remapping-alist)
               '((default (:family "Noto Sans CJK SC") default)))
          (put-text-property 2 3 'composition '(1 1 [9675]))
          (put-text-property 1 10 'face '(:weight bold)))"#,
    )
    .expect("Org Superstar's composed heading and buffer-face-mode remapping");
    (eval, frame_id, LayoutEngine::new())
}

fn assert_circle_fonts(
    eval: &mut Context,
    frame_id: neovm_core::window::FrameId,
    engine: &mut LayoutEngine,
    expected: &str,
) {
    engine.layout_frame_rust(eval, frame_id);
    let state = engine.last_frame_display_state.as_ref().unwrap();
    let circles: Vec<_> = state
        .window_matrices
        .iter()
        .flat_map(|window| &window.matrix.rows)
        .flat_map(|row| &row.glyphs[GlyphArea::Text.index()])
        .filter(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: '○' }))
        .collect();
    assert_eq!(
        circles.len(),
        2,
        "both the composed star and plain circle must paint"
    );
    for circle in circles {
        let binding = &state.char_fonts[&circle.face_id][&'○'];
        let font = &state.fonts[&binding.resolved_font_id];
        assert_eq!(
            font.family, expected,
            "the renderer must receive the GNU-selected circle font"
        );
    }
}

#[test]
fn composed_circle_uses_buffer_remapped_family_for_unspecified_fontset_rule() {
    let (mut eval, frame_id, mut engine) = circle_fixture();
    // GNU fontset.c:fontset_find_font passes the effective face's lface to
    // font.c:font_find_for_lface. A nil spec family inherits that face family.
    // Verified in GNU GUI with the same composition and remapping, 2026-09-20.
    assert_circle_fonts(&mut eval, frame_id, &mut engine, "Noto Sans CJK SC");
}

#[test]
fn circle_fonts_follow_live_remapping_and_explicit_fontset_overrides() {
    let (mut eval, frame_id, mut engine) = circle_fixture();
    assert_circle_fonts(&mut eval, frame_id, &mut engine, "Noto Sans CJK SC");
    // Reuse the engine: neither a retained row nor a per-character font cache
    // may preserve the old binding when face remapping changes.
    eval.eval_str("(setq face-remapping-alist nil)").unwrap();
    assert_circle_fonts(&mut eval, frame_id, &mut engine, "JetBrainsMono Nerd Font");
    eval.eval_str(
        r#"(setq face-remapping-alist '((default (:family "Noto Sans CJK SC") default)))"#,
    )
    .unwrap();
    assert_circle_fonts(&mut eval, frame_id, &mut engine, "Noto Sans CJK SC");

    // An explicit fontset family still wins over the remapped face family.
    eval.eval_str(r#"(set-fontset-font t #x25cb '("JetBrainsMono Nerd Font" . "iso10646-1"))"#)
        .unwrap();
    assert_circle_fonts(&mut eval, frame_id, &mut engine, "JetBrainsMono Nerd Font");
    eval.eval_str(r#"(set-fontset-font t #x25cb '(nil . "iso10646-1"))"#)
        .unwrap();
    assert_circle_fonts(&mut eval, frame_id, &mut engine, "Noto Sans CJK SC");
}

#[test]
fn circle_fonts_use_inline_family_when_fontset_family_is_unspecified() {
    let (mut eval, frame_id, mut engine) = circle_fixture();
    eval.eval_str(
        r#"(put-text-property 1 (point-max) 'face '(:family "JetBrainsMono Nerd Font"))"#,
    )
    .unwrap();
    assert_circle_fonts(&mut eval, frame_id, &mut engine, "JetBrainsMono Nerd Font");
}
