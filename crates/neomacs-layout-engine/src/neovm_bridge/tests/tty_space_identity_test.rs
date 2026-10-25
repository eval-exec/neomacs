use super::*;
use neovm_core::emacs_core::terminal::pure;

struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        pure::reset_terminal_host();
        pure::reset_terminal_runtime();
    }
}

fn fixture() -> (Context, FrameId, FaceResolver, RestoreTerminal) {
    let mut context = Context::new();
    let restore = RestoreTerminal;
    context.set_gc_threshold(usize::MAX);
    pure::configure_terminal_runtime(pure::TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_owned()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
    let buffer = context.buffer_manager().current_buffer().unwrap().id();
    let frame = context
        .frame_manager_mut()
        .create_frame("tty-space-identity", 80, 25, buffer);
    context.sync_runtime_faces_for_frame(frame);
    let mut resolver = FaceResolver::new(context.face_table(), 0x00ffffff, 0, 14.0, None);
    resolver.prepare_tty_realization(frame, &mut context, None);
    assert!(resolver.has_tty_realization());
    (context, frame, resolver, restore)
}

fn buffer() -> Buffer {
    let mut buffer = Buffer::new_standalone(
        neovm_core::buffer::BufferId(891),
        Value::string("*tty-space-identity*"),
    );
    buffer.insert("x");
    buffer
}

#[test]
fn overlay_string_unrelated_remapping_uses_generic_default_slot_identity() {
    let (mut context, frame, resolver, _restore) = fixture();
    let mut buffer = buffer();
    let mut next = 0;
    let mut canonical = resolver.face_for_overlay_string(&buffer, 0, &mut next);
    resolver.realize_for_display(&mut canonical, &mut context);
    assert_eq!(
        canonical.tty_space_identity,
        Some(TtySpaceIdentity::CanonicalDefault)
    );
    buffer.set_buffer_local(
        "face-remapping-alist",
        Value::list(vec![Value::list(vec![
            Value::symbol("mode-line-inactive"),
            Value::symbol("mode-line"),
        ])]),
    );
    let mut generic = resolver.face_for_overlay_string(&buffer, 0, &mut next);
    assert_eq!(generic.canonical_tty_role, None);
    resolver.realize_for_display(&mut generic, &mut context);
    let attributes = generic.source_attributes.as_ref().unwrap().attributes();
    assert_eq!(
        context.cached_display_face_uses_default_slot_for_frame(frame, attributes),
        Some(false)
    );
    assert_eq!(
        generic.tty_space_identity,
        Some(TtySpaceIdentity::OtherRealized)
    );
    assert_eq!(canonical.terminal_fg, generic.terminal_fg);
    assert_eq!(canonical.terminal_bg, generic.terminal_bg);
    assert!(!crate::display_source_resolver::same_resolved_face(
        &canonical, &generic
    ));
}

#[test]
fn ordinary_buffer_only_bypasses_generic_lookup_without_raw_overlays() {
    assert!(OrderedFaceSources::from_text_and_overlays(Some(Value::NIL), Vec::new()).is_empty());
    let (mut context, _, resolver, _restore) = fixture();
    let mut buffer = buffer();
    let mut next = 0;
    let mut canonical = resolver.face_at_pos(&buffer, 0, &mut next);
    resolver.realize_for_display(&mut canonical, &mut context);
    assert_eq!(
        canonical.tty_space_identity,
        Some(TtySpaceIdentity::CanonicalDefault)
    );
    let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
        serial: 1,
        plist: Value::NIL,
        buffer: Some(buffer.id()),
        start: 0,
        end: 1,
        front_advance: false,
        rear_advance: false,
    });
    buffer.overlays_mut().insert_overlay(overlay);
    // No face property: raw overlay presence alone bypasses GNU's fastpath.
    let mut generic = resolver.face_at_pos(&buffer, 0, &mut next);
    resolver.realize_for_display(&mut generic, &mut context);
    assert_eq!(
        generic.tty_space_identity,
        Some(TtySpaceIdentity::OtherRealized)
    );
    let sources = overlay_faces_at(&buffer, EmacsBytePos::new(0), None).into_sources(None);
    assert!(!sources.is_empty());
    assert!(sources.values().next().is_none());
}

#[test]
fn paint_memo_hits_requery_logical_identity_in_mutable_and_cached_paths() {
    let (mut context, frame, resolver, _restore) = fixture();
    let mut entry = resolver.default_face().clone();
    entry.require_generic_tty_lookup();
    let attributes = entry.source_attributes.as_ref().unwrap().clone();
    assert_eq!(
        context.cached_display_face_uses_default_slot_for_frame(frame, attributes.attributes()),
        Some(false)
    );
    resolver.record_tty_paint(
        TtyPaintKey::of(&entry),
        &entry,
        attributes.clone(),
        frame,
        context.face_change_count(),
    );
    // A paint memo must not preserve even an explicitly stale identity bit.
    let mut cached = entry.clone();
    cached.tty_space_identity = Some(TtySpaceIdentity::CanonicalDefault);
    assert!(resolver.realize_cached_for_display(&mut cached, &context));
    assert_eq!(
        cached.tty_space_identity,
        Some(TtySpaceIdentity::OtherRealized)
    );
    let mut mutable = entry;
    mutable.tty_space_identity = Some(TtySpaceIdentity::CanonicalDefault);
    resolver.realize_for_display(&mut mutable, &mut context);
    assert_eq!(
        mutable.tty_space_identity,
        Some(TtySpaceIdentity::OtherRealized)
    );
}

#[test]
fn equal_paint_default_and_other_slots_publish_distinct_arena_identities() {
    let _context = Context::new();
    let mut canonical = ResolvedFace::default();
    canonical.tty_space_identity = Some(TtySpaceIdentity::CanonicalDefault);
    let mut other = canonical.clone();
    other.tty_space_identity = Some(TtySpaceIdentity::OtherRealized);
    let mut attempt = crate::frame_face_arena::FrameFaceArena::default().begin_attempt();
    let left =
        crate::display_row::face_state::stable_face_id_for_resolved(&mut attempt, &canonical);
    let right = crate::display_row::face_state::stable_face_id_for_resolved(&mut attempt, &other);
    assert_ne!(left, right);
    let left_face =
        crate::display_row::face_state::resolved_display_row_face(left, &canonical, None)
            .render_face();
    let right_face = crate::display_row::face_state::resolved_display_row_face(right, &other, None)
        .render_face();
    assert_eq!(left_face.foreground, right_face.foreground);
    assert_eq!(
        left_face.tty_space_identity,
        Some(TtySpaceIdentity::CanonicalDefault)
    );
    assert_eq!(
        right_face.tty_space_identity,
        Some(TtySpaceIdentity::OtherRealized)
    );
    attempt.import_face(left_face).unwrap();
    attempt.import_face(right_face).unwrap();
}
