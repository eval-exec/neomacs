use super::*;
use neovm_core::emacs_core::fontset::FontSpecEntry;

fn frame_with_text(name: &str) -> (neovm_core::emacs_core::Context, neovm_core::window::FrameId) {
    let mut ctx = neovm_core::emacs_core::eval_hook_installers::new_context();
    let source = ctx.buffer_manager().current_buffer().unwrap().id();
    ctx.buffer_manager_mut()
        .get_mut(source)
        .unwrap()
        .insert("abc");
    let frame = ctx.frame_manager_mut().create_frame(name, 400, 160, source);
    ctx.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .set_window_system(None);
    (ctx, frame)
}

fn probe_key(frame: u64) -> crate::font::fallback_order::FallbackOrderKey {
    crate::font::fallback_order::FallbackOrderKey {
        fontset: crate::font::fallback_order::FaceFontsetId {
            frame,
            face: 1,
            face_epoch: 1,
        },
        family: "Probe".to_string(),
        weight: 400,
        slant: 0,
        width: 100,
        size: None,
        fontset_generation: 1,
    }
}

#[test]
fn realize_frame_fonts_stamps_the_face_epoch() {
    let (mut ctx, frame) = frame_with_text("fontset-epoch");
    *ctx.face_change_count_mut() = 6;
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    let inputs = engine
        .prepare_frame_font_inputs(&mut ctx, frame)
        .expect("font inputs");
    let prepared = engine.realize_frame_fonts(&mut ctx, frame, &inputs);
    let (got_frame, got_epoch) = engine
        .font_metrics
        .as_ref()
        .expect("font service")
        .realized_fontset_context();
    assert_eq!(got_frame, frame.0);
    assert_eq!(got_epoch, 6);
    assert_eq!(prepared.face_epoch.get(), 6);
}

#[test]
fn layout_keeps_the_arena_epoch_when_mode_line_bumps_the_counter() {
    let (mut ctx, frame, buffer, _window) = super::incr_editing_frame("abc\n", 800, 240);
    ctx.eval_str("(setq fontset-bumped nil)")
        .expect("arm the one-shot bump");
    // `if` and `setq` are special forms. `unless` is a macro this bare
    // context does not have, so a mode-line `:eval` would skip the bump.
    let format = Value::list(vec![Value::list(vec![
        Value::symbol(":eval"),
        Value::list(vec![
            Value::symbol("if"),
            Value::symbol("fontset-bumped"),
            Value::string("x"),
            Value::list(vec![
                Value::symbol("setq"),
                Value::symbol("fontset-bumped"),
                Value::symbol("t"),
            ]),
            Value::list(vec![Value::symbol("clear-face-cache")]),
            Value::string("x"),
        ]),
    ])]);
    ctx.buffer_manager_mut()
        .get_mut(buffer)
        .expect("buffer")
        .set_buffer_local("mode-line-format", format);
    ctx.eval_str("(setq header-line-format nil tab-line-format nil)")
        .expect("quiet chrome");
    let before = ctx.face_change_count();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    let attempt = engine.redisplay_frame_attempt(&mut ctx, frame);
    let after_attempt = ctx.face_change_count();
    let saw_bump = ctx
        .eval_str("fontset-bumped")
        .ok()
        .is_some_and(|value| !value.is_nil());
    assert!(
        matches!(attempt, FrameLayoutAttempt::Prepared(_)),
        "redisplay converges after one face bump (before {before} after {after_attempt} bumped {saw_bump})"
    );
    let after = ctx.face_change_count();
    assert!(
        after > before,
        "the mode line must bump face_change_count, before {before} after {after} bumped {saw_bump}"
    );
    let arena_epoch = engine
        .frame_face_arenas
        .get(&frame)
        .expect("sealed arena")
        .face_epoch()
        .get();
    let (_, context_epoch) = engine
        .font_metrics
        .as_ref()
        .expect("font service")
        .realized_fontset_context();
    assert_eq!(
        arena_epoch, before,
        "the arena stays on the epoch captured before the mode line"
    );
    assert_eq!(
        context_epoch, arena_epoch,
        "a retry must not re-key this arena's faces under the live counter {after}"
    );
}

#[test]
fn deleting_a_frame_drops_its_fallback_group() {
    let (mut ctx, live) = frame_with_text("fontset-live");
    let source = ctx.buffer_manager().current_buffer().unwrap().id();
    let dead = ctx
        .frame_manager_mut()
        .create_frame("fontset-dead", 80, 40, source);
    let entry = FontSpecEntry::ExplicitNone;
    crate::font::fallback_order::promote_fallback(&probe_key(live.0), &entry);
    crate::font::fallback_order::promote_fallback(&probe_key(dead.0), &entry);
    assert_eq!(
        crate::font::fallback_order::fallback_order_groups_for_frame(dead.0),
        1
    );
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    let locks_before = crate::font::fallback_order::fallback_order_lock_count();
    engine.retire_removed_frame_owners(&ctx);
    assert_eq!(
        crate::font::fallback_order::fallback_order_lock_count(),
        locks_before,
        "redisplay does not walk the order map while every keyed frame is alive"
    );
    ctx.frame_manager_mut().delete_frame(dead);
    engine.retire_removed_frame_owners(&ctx);
    assert_eq!(
        crate::font::fallback_order::fallback_order_groups_for_frame(dead.0),
        0,
        "a deleted frame's fallback group is dropped"
    );
    assert_eq!(
        crate::font::fallback_order::fallback_order_groups_for_frame(live.0),
        1,
        "another frame's group stays"
    );
}

#[test]
fn font_at_lookup_uses_the_redisplayed_face_id() {
    let (mut ctx, frame) = frame_with_text("fontset-font-at");
    let window_id = ctx
        .frame_manager()
        .get(frame)
        .and_then(|frame| frame.selected_window())
        .expect("selected window")
        .id();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    let attempt = engine.redisplay_frame_attempt(&mut ctx, frame);
    let FrameLayoutAttempt::Prepared(state) = attempt else {
        panic!("redisplay must seal a frame before font-at can see its face");
    };
    let drawn = state
        .window_matrices
        .iter()
        .flat_map(|entry| &entry.matrix.rows)
        .flat_map(|row| &row.glyphs[neomacs_display_protocol::GlyphArea::Text.index()])
        .find(|glyph| {
            matches!(
                glyph.provenance,
                neomacs_display_protocol::GlyphProvenance::Buffer { charpos: 0 }
            )
        })
        .expect("the first buffer character is drawn")
        .face_id;
    let (face, epoch) = engine
        .realized_fontset_for_lisp_pos(&mut ctx, frame, window_id, 1)
        .expect("font-at lookup sees the sealed face");
    let arena_epoch = engine
        .frame_face_arenas
        .get(&frame)
        .expect("sealed arena")
        .face_epoch()
        .get();
    assert_eq!(
        face,
        drawn.get(),
        "font-at must name the face redisplay drew"
    );
    assert_eq!(epoch, arena_epoch, "the order key uses the arena epoch");

    let implicit = crate::font::metrics::selection_for_realized_fontset(
        "Probe", 400, false, 16.0, frame.0, None, None,
    );
    let named = crate::font::metrics::selection_for_realized_fontset(
        "Probe",
        400,
        false,
        16.0,
        frame.0,
        Some(face),
        Some(epoch),
    );
    assert_eq!(
        implicit.fontset_id(),
        crate::font::fallback_order::FaceFontsetId::default(),
        "a missing realized id stays on the implicit fontset"
    );
    assert_eq!(
        named.fontset_id(),
        crate::font::fallback_order::FaceFontsetId {
            frame: frame.0,
            face,
            face_epoch: epoch,
        },
        "both realized ids name the fontset redisplay moved"
    );
}
