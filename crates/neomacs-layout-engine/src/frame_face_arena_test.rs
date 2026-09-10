//! Caller-facing regressions for frame-face ownership and finalization.
use super::*;

#[test]
fn resolved_binding_preserves_identity_through_measurement_without_publishing() {
    let mut attempt = FrameFaceArena::default().begin_attempt();
    let resolved = crate::neovm_bridge::ResolvedFace::default();
    let id = crate::display_row::face_state::stable_face_id_for_resolved(&mut attempt, &resolved);
    let bound = attempt.bind_resolved_face(id, resolved.clone()).unwrap();
    assert_eq!(bound.face_id(), id);
    assert_eq!(bound.resolved().font_size, resolved.font_size);
    let measured = bound.realized(None);
    assert!(attempt.faces().is_empty());
    assert!(
        FrameFaceArena::default()
            .begin_attempt()
            .publish_face(&measured)
            .is_err()
    );
    attempt.publish_face(&measured).unwrap();
    let mut wrong = resolved;
    wrong.font_size *= 0.75;
    assert!(attempt.bind_resolved_face(id, wrong).is_err());
}

#[test]
fn discarded_row_preparation_does_not_publish_faces_or_replace_metrics() {
    let mut attempt = FrameFaceArena::default().begin_attempt();
    let mut face = Face::new(FaceId::new(0));
    face.font_ascent = 14;
    let prepared = attempt.prepare_face(face.clone()).unwrap();
    assert!(attempt.faces().is_empty());
    let mut other = FrameFaceArena::default().begin_attempt();
    assert!(other.publish_face(&prepared).is_err());
    assert!(other.faces().is_empty());
    attempt.publish_face(&prepared).unwrap();

    let mut measured = face.clone();
    measured.font_ascent = 20;
    let discarded = attempt.prepare_face(measured).unwrap();
    drop(discarded);
    assert_eq!(attempt.face(face.id), Some(face));
}

#[test]
fn sealing_can_complete_but_not_replace_or_erase_an_exact_font_binding() {
    let mut attempt = FrameFaceArena::default().begin_attempt();
    let mut face = Face::new(FaceId::new(0));
    face.font_file_path = Some("/fonts/primary.ttf".into());
    attempt.import_face(face.clone()).unwrap();
    for replacement in [None, Some("/fonts/unrelated.ttf".into())] {
        let mut finalized = attempt.faces();
        finalized.get_mut(&face.id).unwrap().font_file_path = replacement;
        assert!(attempt.seal(finalized).is_err());
    }
    let mut finalized = attempt.faces();
    finalized.get_mut(&face.id).unwrap().font_ascent = 14;
    finalized.get_mut(&face.id).unwrap().font_descent = 4;
    assert!(attempt.seal(finalized).is_ok());
}

#[test]
fn retained_faces_cannot_cross_sibling_speculative_presentations() {
    let arena = FrameFaceArena::default();
    let mut first = arena.begin_attempt();
    let mut sibling = arena.begin_attempt();
    let id = FaceId::new(0);
    let mut first_face = Face::new(id);
    first_face.font_size = 13.0;
    let mut sibling_face = first_face.clone();
    sibling_face.font_size = 20.0;
    first.import_face(first_face).unwrap();
    sibling.import_face(sibling_face).unwrap();
    let first = first.commit();
    let sibling = sibling.commit();
    assert_eq!(first.generation(), sibling.generation());
    let mut attempt = first.begin_attempt();
    assert!(
        attempt
            .admit_retained(sibling.generation(), [id], &sibling)
            .is_err()
    );
    assert!(attempt.faces().is_empty());
    assert!(
        attempt
            .admit_retained(first.generation(), [id], &first)
            .is_ok()
    );
    assert_eq!(attempt.face(id).unwrap().font_size, 13.0);
}

#[test]
fn checked_import_rejects_reserved_identity_mismatch_without_publication() {
    let mut attempt = FrameFaceArena::default().begin_attempt();
    let resolved = crate::neovm_bridge::ResolvedFace::default();
    let id = crate::display_row::face_state::stable_face_id_for_resolved(&mut attempt, &resolved);
    let mut wrong = crate::display_row::face_state::resolved_display_row_face(id, &resolved, None)
        .render_face();
    wrong.font_size *= 0.75;
    assert!(attempt.import_face(wrong).is_err());
    assert!(attempt.faces().is_empty());
    assert!(attempt.intern_resolved_face(&resolved).is_ok());
}

#[test]
fn realized_handles_are_registered_atomically_and_scoped_to_one_attempt() {
    let arena = FrameFaceArena::default();
    let mut attempt = arena.begin_attempt();
    let mut resolved = crate::neovm_bridge::ResolvedFace::default();
    resolved.font_size = 13.0;
    let face = attempt.intern_resolved_face(&resolved).unwrap();
    let again = attempt.intern_resolved_face(&resolved).unwrap();
    let id = attempt.use_face(&face).unwrap();
    assert_eq!(attempt.use_face(&again).unwrap(), id);
    assert_eq!(attempt.face(id).unwrap().font_size, 13.0);
    assert_eq!(attempt.clone().use_face(&face).unwrap(), id);

    let other_frame = FrameFaceArena::default().begin_attempt();
    assert!(other_frame.use_face(&face).is_err());
    let other_attempt = arena.begin_attempt();
    assert!(other_attempt.use_face(&face).is_err());
    let next = attempt.commit().begin_attempt();
    assert!(next.use_face(&face).is_err());
}

#[test]
fn an_older_attempt_cannot_admit_a_later_presentation() {
    let mut old = FrameFaceArena::default().begin_attempt();
    let face = Face::new(FaceId::new(0));
    old.import_face(face.clone()).unwrap();
    let committed = old.commit();
    assert!(
        old.admit_retained(committed.generation(), [face.id], &committed)
            .is_err(),
        "retained admission must also validate the destination attempt's generation"
    );
}

#[test]
fn retained_faces_cannot_cross_frame_arenas_with_equal_generations() {
    let first = FrameFaceArena::default().begin_attempt().commit();
    let mut other_attempt = FrameFaceArena::default().begin_attempt();
    let face = Face::new(FaceId::new(0));
    other_attempt.import_face(face.clone()).unwrap();
    let other = other_attempt.commit();
    assert_eq!(first.generation(), other.generation());

    let mut attempt = first.begin_attempt();
    assert!(
        attempt
            .admit_retained(other.generation(), [face.id], &other)
            .is_err(),
        "equal presentation counters do not establish frame ownership"
    );
    assert!(attempt.faces().is_empty());
}

#[test]
fn sealing_rejects_changed_styling_without_losing_the_published_face() {
    let arena = FrameFaceArena::default();
    let mut attempt = arena.begin_attempt();
    let mut face = Face::new(FaceId::new(0));
    face.font_size = 13.0;
    attempt.import_face(face.clone()).unwrap();

    let mut finalized = attempt.faces();
    finalized.get_mut(&face.id).unwrap().font_size = 9.889436;
    assert!(
        attempt.seal(finalized).is_err(),
        "font finalization must not change a published face's styling identity"
    );
    assert_eq!(attempt.face(face.id), Some(face));
    assert!(attempt.seal(attempt.faces()).is_ok());
}

use neomacs_display_protocol::types::Color;

fn identity_with_fg(pixel: u32) -> Face {
    let mut face = Face::new(FaceId::new(0));
    face.foreground = Color::from_pixel(pixel);
    face_realization_identity(&face)
}

#[test]
fn stable_ids_survive_realization_order_across_attempts() {
    // The GNU face_cache property: the same realization identity keeps
    // its id across layout passes even when the passes encounter faces
    // in a different order. Without it, one extra early checkpoint
    // renumbered every later face and the renderer diffed dozens of
    // "modified" faces per keystroke.
    let red = identity_with_fg(0x00FF0000);
    let blue = identity_with_fg(0x000000FF);

    let arena = FrameFaceArena::default();
    let mut first = arena.begin_attempt();
    let red_id = first.stable_face_id(red.clone());
    let blue_id = first.stable_face_id(blue.clone());
    assert_ne!(red_id, blue_id);
    let mut red_face = red.clone();
    red_face.id = red_id;
    first.import_face(red_face).expect("publish red");
    let mut blue_face = blue.clone();
    blue_face.id = blue_id;
    first.import_face(blue_face).expect("publish blue");
    let sealed = first.commit();

    // Opposite realization order, same ids.
    let mut second = sealed.begin_attempt();
    assert_eq!(second.stable_face_id(blue.clone()), blue_id);
    assert_eq!(second.stable_face_id(red.clone()), red_id);

    // A never-seen identity gets a fresh id above every previous one.
    let green = identity_with_fg(0x0000FF00);
    let green_id = second.stable_face_id(green);
    assert!(green_id.get() > red_id.get().max(blue_id.get()));
}

#[test]
fn stable_ids_ignore_enrichment_but_not_content() {
    // Metrics, the exact font file, and the resolved font handle are
    // filled in after row construction; they must not fork identity.
    let base = identity_with_fg(0x00123456);
    let mut enriched = base.clone();
    enriched.font_ascent = 12;
    enriched.font_descent = 3;
    enriched.font_file_path = Some("/tmp/font.ttf".to_owned());

    let arena = FrameFaceArena::default();
    let mut attempt = arena.begin_attempt();
    let id = attempt.stable_face_id(base.clone());
    assert_eq!(
        attempt.stable_face_id(face_realization_identity(&enriched)),
        id
    );

    // A genuinely different rendering is a different face.
    let mut bold = base.clone();
    bold.font_weight = 700;
    assert_ne!(attempt.stable_face_id(bold), id);
}

#[test]
fn publishing_enriched_faces_under_stable_ids_merges_cleanly() {
    // The id key is computed pre-enrichment; the published face carries
    // metrics. publish() must accept that (merge_compatible_realization
    // treats enrichment as compatible) and the debug verification must
    // compare identities, not raw faces.
    let identity = identity_with_fg(0x00ABCDEF);
    let arena = FrameFaceArena::default();
    let mut attempt = arena.begin_attempt();
    let id = attempt.stable_face_id(identity.clone());

    let mut published = identity;
    published.id = id;
    published.font_ascent = 14;
    published.font_descent = 4;
    published.default_resolved_font_id = Some(neomacs_display_protocol::font::ResolvedFontId(7));
    attempt.import_face(published).expect("enriched publish");
}

#[test]
fn one_attempt_cannot_rebind_a_face_id_to_different_rendering() {
    let arena = FrameFaceArena::default();
    let mut attempt = arena.begin_attempt();
    let face_id = attempt.reserve_dynamic_face();

    let mut original = Face::new(face_id);
    original.foreground = Color::from_pixel(0x00112233);
    attempt
        .import_face(original.clone())
        .expect("first publication");

    let mut replacement = Face::new(face_id);
    replacement.foreground = Color::from_pixel(0x00445566);
    assert!(
        attempt.import_face(replacement).is_err(),
        "a frame face id is immutable once published"
    );
    assert_eq!(
        attempt.faces().get(&face_id),
        Some(&original),
        "rejected publication must preserve the original face"
    );
}

#[test]
fn one_attempt_can_complete_missing_metrics_for_the_same_face() {
    let arena = FrameFaceArena::default();
    let mut attempt = arena.begin_attempt();
    let face_id = attempt.reserve_dynamic_face();
    let incomplete = Face::new(face_id);
    attempt
        .import_face(incomplete)
        .expect("publish semantic face before measurement");

    let mut measured = Face::new(face_id);
    measured.font_ascent = 13;
    measured.font_descent = 5;
    attempt
        .import_face(measured.clone())
        .expect("measurement may complete missing metrics");
    assert_eq!(attempt.face(face_id), Some(measured));
}

#[test]
fn later_realization_replaces_metrics_without_clearing_exact_font_identity() {
    let arena = FrameFaceArena::default();
    let mut attempt = arena.begin_attempt();
    let face_id = attempt.reserve_dynamic_face();
    let mut earlier = Face::new(face_id);
    earlier.font_ascent = 7;
    earlier.font_descent = 3;
    earlier.font_file_path = Some("/fonts/exact.ttf".to_owned());
    attempt
        .import_face(earlier)
        .expect("publish earlier realization");

    let mut later = Face::new(face_id);
    later.font_ascent = 4;
    later.font_descent = 2;
    attempt
        .import_face(later)
        .expect("publish later realization of the same face");

    let realized = attempt.face(face_id).expect("realized face");
    assert_eq!((realized.font_ascent, realized.font_descent), (4, 2));
    assert_eq!(realized.font_file_path.as_deref(), Some("/fonts/exact.ttf"));
}

#[test]
fn retained_faces_occupy_their_slots_before_fresh_allocation() {
    let arena = FrameFaceArena::default();
    let mut first = arena.begin_attempt();
    let retained_id = first.reserve_dynamic_face();
    let mut retained_face = Face::new(retained_id);
    retained_face.foreground = Color::from_pixel(0x00112233);
    first
        .import_face(retained_face.clone())
        .expect("publish retained face");
    let committed = first.commit();

    let mut next = committed.begin_attempt();
    next.admit_retained(committed.generation, [retained_id], &committed)
        .expect("admit retained face");

    let fresh_id = next.reserve_dynamic_face();
    assert_ne!(
        fresh_id, retained_id,
        "fresh allocation must not alias an admitted retained face"
    );
    assert_eq!(next.faces().get(&retained_id), Some(&retained_face));
}

#[test]
fn invalidated_arena_rejects_stale_retained_handles_before_admission() {
    let arena = FrameFaceArena::default();
    let mut first = arena.begin_attempt();
    let retained_id = first.reserve_dynamic_face();
    first
        .import_face(Face::new(retained_id))
        .expect("publish retained face");
    let committed = first.commit();
    let stale_generation = committed.generation();
    let invalidated = committed.invalidate();
    let mut next = invalidated.begin_attempt();

    assert_eq!(
        next.admit_retained(stale_generation, [retained_id], &invalidated),
        Err(FrameFaceReuseError::StaleGeneration {
            retained: stale_generation,
            current: invalidated.generation(),
        })
    );
    assert!(
        next.faces().is_empty(),
        "failed admission must not partially publish retained faces"
    );
}

#[test]
fn retained_admission_cannot_overwrite_an_attempt_publication() {
    let arena = FrameFaceArena::default();
    let mut first = arena.begin_attempt();
    let face_id = first.reserve_dynamic_face();
    let mut retained = Face::new(face_id);
    retained.foreground = Color::from_pixel(0x00112233);
    first.import_face(retained).expect("publish retained face");
    let committed = first.commit();

    let mut next = committed.begin_attempt();
    let mut fresh = Face::new(face_id);
    fresh.foreground = Color::from_pixel(0x00445566);
    next.import_face(fresh.clone()).expect("publish fresh face");
    assert_eq!(
        next.admit_retained(committed.generation(), [face_id], &committed),
        Err(FrameFaceReuseError::ConflictingFace(face_id))
    );
    assert_eq!(
        next.face(face_id),
        Some(fresh),
        "failed retained admission must preserve the attempt publication"
    );
}

#[test]
fn sealing_commits_the_finalized_face_table_for_future_replay() {
    let arena = FrameFaceArena::default();
    let mut attempt = arena.begin_attempt();
    let face_id = attempt.reserve_dynamic_face();
    attempt
        .import_face(Face::new(face_id))
        .expect("publish semantic face");

    let mut finalized_faces = attempt.faces();
    finalized_faces
        .get_mut(&face_id)
        .expect("published face")
        .font_file_path = Some("/fonts/exact.ttf".to_owned());
    let sealed = attempt
        .seal(finalized_faces)
        .expect("sealing may enrich a published face");

    let mut replay = sealed.begin_attempt();
    replay
        .admit_retained(sealed.generation(), [face_id], &sealed)
        .expect("admit face from sealed arena");
    assert_eq!(
        replay.face(face_id).and_then(|face| face.font_file_path),
        Some("/fonts/exact.ttf".to_owned())
    );
}

#[test]
fn sealing_advances_the_generation() {
    let arena = FrameFaceArena::default();
    let attempt = arena.begin_attempt();

    let sealed = attempt.seal(HashMap::new()).expect("seal empty attempt");

    assert_ne!(
        sealed.generation(),
        arena.generation(),
        "each accepted presentation needs a distinct retained-face generation"
    );
}
