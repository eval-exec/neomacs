use super::*;
use std::sync::Arc;

fn identity_source() -> Arc<RootedFace> {
    let mut overlay = NeoFace::new("resolved-identity-overlay");
    overlay.foreground = Some(NeoColor::rgb(0x44, 0x55, 0x66));
    Arc::new(RootedFace::new(overlay).expect("installed mutator heap"))
}

fn identity_face(resolver: &FaceResolver, source: &Arc<RootedFace>) -> ResolvedFace {
    let mut face = resolver.realize_face(source.attributes());
    face.source_attributes = Some(Arc::clone(source));
    face.underline_color = 0x00112233;
    face.terminal_underline_color = Some(TerminalColor::Indexed(3));
    face
}

#[test]
fn shared_source_identity_ignores_enrichment_and_transient_ids() {
    let _context = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let source = identity_source();
    let face = identity_face(&resolver, &source);

    // Font realization may fill metrics after the checkpoint that interned
    // the source, and the assigned id field lags the arena's answer; neither
    // is part of the realization identity.
    let mut recheckpoint = face.clone();
    recheckpoint.font_char_width = 6.5;
    recheckpoint.font_ascent = 11.0;
    recheckpoint.font_line_height = 15.0;
    recheckpoint.face_id = 77;
    assert!(recheckpoint != face);
    assert!(face.shares_resolved_identity(&recheckpoint));
    assert!(recheckpoint.shares_resolved_identity(&face));

    // The same identity reaches the arena: the memo answers by source
    // without re-rendering the canonical projection (GNU `lookup_face`
    // keys the frame face cache by the merged lface, src/xfaces.c:5064).
    let mut arena = crate::frame_face_arena::FrameFaceArena::default();
    let mut attempt = arena.begin_attempt();
    let id = crate::display_row::face_state::stable_face_id_for_resolved(&mut attempt, &face);
    assert_eq!(
        crate::display_row::face_state::stable_face_id_for_resolved(&mut attempt, &recheckpoint),
        id
    );
}

#[test]
fn shared_source_identity_still_requires_the_escape_fields() {
    let _context = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let source = identity_source();
    let face = identity_face(&resolver, &source);

    // A relative :height folds against the base face's size.
    let mut resized = face.clone();
    resized.font_size *= 2.0;
    assert!(!face.shares_resolved_identity(&resized));

    // The named-face resolver assigns the name after interning.
    let mut renamed = face.clone();
    renamed.lisp_name = Some(std::sync::Arc::from("font-lock-comment-face"));
    assert!(!face.shares_resolved_identity(&renamed));

    // Entry color state feeds the realization the memo would replay.
    let mut seeded = face.clone();
    seeded.color_state = FaceColorState::Seed;
    assert!(!face.shares_resolved_identity(&seeded));

    // The preserved underline slots are part of the entry state.
    let mut recolored = face.clone();
    recolored.underline_color = 0x0000FF00;
    assert!(!face.shares_resolved_identity(&recolored));
    let mut reterm = face.clone();
    reterm.terminal_underline_color = None;
    assert!(!face.shares_resolved_identity(&reterm));

    // A forced basic role keeps distinct realizations apart.
    let mut roled = face.clone();
    roled.canonical_tty_role = Some(BasicFace::ModeLineInactive);
    assert!(!face.shares_resolved_identity(&roled));

    // Equal content under another allocation is another composition, and a
    // face without a retained source cannot claim identity at all.
    let other = identity_source();
    assert!(!face.shares_resolved_identity(&identity_face(&resolver, &other)));
    let mut unrooted = face.clone();
    unrooted.source_attributes = None;
    assert!(!face.shares_resolved_identity(&unrooted));
    assert!(!unrooted.shares_resolved_identity(&unrooted.clone()));
}
