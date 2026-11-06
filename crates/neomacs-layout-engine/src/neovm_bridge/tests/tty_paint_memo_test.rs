use super::*;
use std::sync::Arc;

fn memo_source() -> Arc<RootedFace> {
    let mut overlay = NeoFace::new("tty-paint-memo-overlay");
    overlay.foreground = Some(NeoColor::rgb(0x11, 0x22, 0x33));
    Arc::new(RootedFace::new(overlay.clone()).expect("installed mutator heap"))
}

fn memo_face(resolver: &FaceResolver, source: &Arc<RootedFace>) -> ResolvedFace {
    let mut face = resolver.realize_face(source.attributes());
    face.source_attributes = Some(Arc::clone(source));
    face.fg = 0x00ABCDEF;
    face.terminal_fg = Some(TerminalColor::Indexed(4));
    face.use_default_foreground = false;
    face.bg = 0x00112233;
    face.terminal_bg = None;
    face.use_default_background = true;
    face.terminal_inverse_video = true;
    face.terminal_underline_color = Some(TerminalColor::Indexed(7));
    face.underline_color = 0x00445566;
    face
}

#[test]
fn tty_paint_memo_answers_the_unrealized_entry_state() {
    let _context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let source = memo_source();
    let mut entry = memo_face(&resolver, &source);
    let frame = FrameId(9);

    // The realize flow records AFTER installing realized colors, but the
    // memo's key is the pre-install entry state the next probe arrives in
    // (GNU `lookup_face` keys on the merged lface, never the realized
    // `struct face`, src/xfaces.c:5064-5075).
    let probe_key = TtyPaintKey::of(&entry);
    let mut painted = entry.clone();
    painted.terminal_fg = Some(TerminalColor::Indexed(9));
    painted.underline_color = 0x00990099;
    resolver.record_tty_paint(probe_key, &painted, Arc::clone(&source), frame, 41);

    assert!(
        resolver
            .cached_tty_paint(&entry, &source, frame, 41)
            .is_some(),
        "a probe of the unrealized entry state must hit the memo"
    );
    assert!(
        resolver
            .cached_tty_paint(&painted, &source, frame, 41)
            .is_none(),
        "the realized state is not the state the memo answers for"
    );
}

#[test]
fn tty_paint_memo_installs_the_recorded_snapshot_verbatim() {
    let _context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let source = memo_source();
    let face = memo_face(&resolver, &source);
    let frame = FrameId(9);

    resolver.record_tty_paint(
        TtyPaintKey::of(&face),
        &face,
        Arc::clone(&source),
        frame,
        41,
    );

    // Only paint fields outside the key are disturbed; the snapshot must
    // restore every field install_realized_tty_colors writes.
    let mut consumed = face.clone();
    consumed.fg = 0;
    consumed.terminal_fg = None;
    consumed.use_default_foreground = true;
    consumed.bg = 0;
    consumed.terminal_bg = Some(TerminalColor::Indexed(1));
    consumed.use_default_background = false;
    consumed.terminal_inverse_video = false;
    consumed.terminal_underline_color = None;
    let Some(paint) = resolver.cached_tty_paint(&consumed, &source, frame, 41) else {
        panic!("same frame and binding generation must hit the memo");
    };
    paint.install(&mut consumed);
    assert_eq!(consumed.fg, 0x00ABCDEF);
    assert_eq!(consumed.terminal_fg, Some(TerminalColor::Indexed(4)));
    assert!(!consumed.use_default_foreground);
    assert_eq!(consumed.bg, 0x00112233);
    assert_eq!(consumed.terminal_bg, None);
    assert!(consumed.use_default_background);
    assert!(consumed.terminal_inverse_video);
    assert_eq!(
        consumed.terminal_underline_color,
        Some(TerminalColor::Indexed(7))
    );
    assert_eq!(consumed.underline_color, 0x00445566);
}

#[test]
fn tty_paint_memo_is_scoped_to_frame_and_binding_generation() {
    let _context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let source = memo_source();
    let face = memo_face(&resolver, &source);

    resolver.record_tty_paint(
        TtyPaintKey::of(&face),
        &face,
        Arc::clone(&source),
        FrameId(9),
        41,
    );

    // A different frame realizes against another frame's definitions, and a
    // bumped face_change_count retired them (GNU face_change,
    // src/xfaces.c:3098-3105).
    assert!(
        resolver
            .cached_tty_paint(&face, &source, FrameId(10), 41)
            .is_none()
    );
    assert!(
        resolver
            .cached_tty_paint(&face, &source, FrameId(9), 42)
            .is_none()
    );
    assert!(
        resolver
            .cached_tty_paint(&face, &source, FrameId(9), 41)
            .is_some()
    );
}

#[test]
fn tty_paint_memo_key_includes_entry_state_and_source_identity() {
    let _context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let source = memo_source();
    let face = memo_face(&resolver, &source);
    let frame = FrameId(9);

    resolver.record_tty_paint(
        TtyPaintKey::of(&face),
        &face,
        Arc::clone(&source),
        frame,
        41,
    );

    // The entry color state feeds the merge the realization consumed.
    let mut seeded = face.clone();
    seeded.color_state = FaceColorState::Seed;
    assert!(
        resolver
            .cached_tty_paint(&seeded, &source, frame, 41)
            .is_none()
    );

    // The preserved underline slot stays part of the key.
    let mut underlined = face.clone();
    underlined.underline_color = 0x0000FF00;
    assert!(
        resolver
            .cached_tty_paint(&underlined, &source, frame, 41)
            .is_none()
    );

    // Equal content under another allocation is another composition.
    let other = memo_source();
    assert_eq!(other.attributes(), source.attributes());
    assert!(
        resolver
            .cached_tty_paint(&face, &other, frame, 41)
            .is_none()
    );
}
