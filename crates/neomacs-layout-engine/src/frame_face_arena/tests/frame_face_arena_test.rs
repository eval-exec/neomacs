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
fn prepared_output_append_preserves_order_attempt_scope_and_speculative_metrics() {
    let arena = FrameFaceArena::default();
    let mut attempt = arena.begin_attempt();
    let mut first = Face::new(FaceId::new(0));
    first.font_family = "prepared output family".into();
    first.font_size = 13.0;
    first.id = attempt.stable_face_id(face_realization_identity(&first));
    attempt.import_face(first.clone()).unwrap();

    let mut measured = first.clone();
    measured.font_ascent = 18;
    measured.font_descent = 4;
    let mut second = Face::new(FaceId::new(1));
    second.foreground = Color::from_pixel(0x00112233);
    let mut output = Vec::with_capacity(3);
    for face in [&first, &measured, &second] {
        attempt
            .prepare_face_into_output(face.clone(), &mut output)
            .unwrap();
    }
    assert_eq!(output.len(), 3);
    for (prepared, expected) in output.iter().zip([&first, &measured, &second]) {
        assert_eq!(prepared.face(), expected);
        assert_eq!(attempt.use_face(prepared), Ok(expected.id));
        assert_eq!(
            arena.begin_attempt().use_face(prepared),
            Err(FrameFaceUseError::ForeignAttempt),
            "another attempt cannot consume a caller-owned prepared handle"
        );
    }
    assert_eq!(attempt.face(first.id), Some(first.clone()));
    assert!(attempt.face(second.id).is_none());
    assert_eq!(attempt.faces().len(), 1);
    drop(output);
    assert_eq!(
        attempt.face(first.id),
        Some(first),
        "discarding prepared output must not publish measured enrichment"
    );
}

#[test]
fn prepared_output_rejects_identity_and_font_conflicts_without_partial_append() {
    use neomacs_display_protocol::font::ResolvedFontId;

    let mut attempt = FrameFaceArena::default().begin_attempt();
    let mut face = Face::new(FaceId::new(0));
    face.font_family = "retained prepared payload".into();
    face.font_file_path = Some("/fonts/exact.ttf".into());
    face.default_resolved_font_id = Some(ResolvedFontId(7));
    face.id = attempt.stable_face_id(face_realization_identity(&face));
    attempt.import_face(face.clone()).unwrap();
    let mut output = Vec::with_capacity(4);
    attempt
        .prepare_face_into_output(face.clone(), &mut output)
        .unwrap();
    let storage = output.as_ptr();
    let capacity = output.capacity();
    let mut different_identity = face.clone();
    different_identity.font_size *= 1.5;
    let mut different_path = face.clone();
    different_path.font_file_path = Some("/fonts/conflicting.ttf".into());
    let mut different_font = face.clone();
    different_font.default_resolved_font_id = Some(ResolvedFontId(8));
    for rejected in [different_identity, different_path, different_font] {
        let expected = attempt.prepare_face(rejected.clone()).unwrap_err();
        assert_eq!(
            attempt.prepare_face_into_output(rejected, &mut output),
            Err(expected),
            "output append must use the same checked admission as owned preparation"
        );
        assert_eq!(output.len(), 1);
        assert_eq!(output.as_ptr(), storage);
        assert_eq!(output.capacity(), capacity);
        assert_eq!(output[0].face(), &face);
        assert_eq!(attempt.face(face.id), Some(face.clone()));
    }
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

#[test]
fn borrowed_realization_comparison_matches_canonical_projection_for_every_field() {
    use neomacs_display_protocol::face::{
        BoxBorderStyle, BoxLineWidth, BoxType, FaceAttributes, UnderlinePosition, UnderlineStyle,
    };
    use neomacs_display_protocol::font::ResolvedFontId;
    use neomacs_display_protocol::frame_glyphs::StipplePattern;
    use neomacs_display_protocol::gradient::{ColorStop, Gradient};
    use neomacs_display_protocol::terminal_color::TerminalColor;

    let base = Face::new(FaceId::new(0));
    let changes: &[(&str, fn(&mut Face), bool)] = &[
        ("id", |face| face.id = FaceId::new(37), true),
        ("foreground", |face| face.foreground = Color::BLACK, false),
        ("background", |face| face.background = Color::WHITE, false),
        (
            "terminal_foreground",
            |face| face.terminal_foreground = Some(TerminalColor::Indexed(2)),
            false,
        ),
        (
            "terminal_background",
            |face| face.terminal_background = Some(TerminalColor::Indexed(3)),
            false,
        ),
        (
            "use_default_foreground",
            |face| face.use_default_foreground = !face.use_default_foreground,
            false,
        ),
        (
            "use_default_background",
            |face| face.use_default_background = !face.use_default_background,
            false,
        ),
        (
            "underline_color",
            |face| face.underline_color = Some(Color::WHITE),
            false,
        ),
        (
            "terminal_underline_color",
            |face| face.terminal_underline_color = Some(TerminalColor::Indexed(4)),
            false,
        ),
        (
            "overline_color",
            |face| face.overline_color = Some(Color::WHITE),
            false,
        ),
        (
            "strike_through_color",
            |face| face.strike_through_color = Some(Color::WHITE),
            false,
        ),
        (
            "box_color",
            |face| face.box_color = Some(Color::WHITE),
            false,
        ),
        (
            "font_family",
            |face| face.font_family = "different family".into(),
            false,
        ),
        ("font_size", |face| face.font_size *= 1.5, false),
        ("font_weight", |face| face.font_weight = 700, false),
        (
            "attributes",
            |face| face.attributes = FaceAttributes::BOLD,
            false,
        ),
        (
            "underline_style",
            |face| face.underline_style = UnderlineStyle::Wave,
            false,
        ),
        ("box_type", |face| face.box_type = BoxType::Raised3D, false),
        (
            "box_line_width",
            |face| face.box_line_width = BoxLineWidth::from_gnu(3),
            false,
        ),
        (
            "box_corner_radius",
            |face| face.box_corner_radius = 4,
            false,
        ),
        (
            "box_border_style",
            |face| face.box_border_style = BoxBorderStyle::Neon,
            false,
        ),
        (
            "box_border_speed",
            |face| face.box_border_speed = 2.0,
            false,
        ),
        (
            "box_color2",
            |face| face.box_color2 = Some(Color::WHITE),
            false,
        ),
        (
            "font_file_path",
            |face| face.font_file_path = Some("/fonts/enriched.ttf".into()),
            true,
        ),
        ("font_ascent", |face| face.font_ascent = 17, true),
        ("font_descent", |face| face.font_descent = 5, true),
        (
            "underline_position",
            |face| face.underline_position = 9,
            false,
        ),
        (
            "underline_thickness",
            |face| face.underline_thickness = 3,
            false,
        ),
        (
            "background_gradient",
            |face| {
                face.background_gradient = Some(Box::new(Gradient::Linear {
                    angle: 90.0,
                    stops: vec![
                        ColorStop::new(0.0, Color::BLACK),
                        ColorStop::new(1.0, Color::WHITE),
                    ],
                }))
            },
            false,
        ),
        (
            "lisp_name",
            |face| face.lisp_name = Some("borrowed identity".into()),
            false,
        ),
        (
            "default_resolved_font_id",
            |face| face.default_resolved_font_id = Some(ResolvedFontId(7)),
            true,
        ),
        (
            "stipple",
            |face| {
                face.stipple = Some(Box::new(StipplePattern {
                    width: 8,
                    height: 2,
                    bits: vec![0x55, 0xaa],
                }))
            },
            false,
        ),
        (
            "underline_placement",
            |face| face.underline_placement = UnderlinePosition::DescentLine { pixels_above: 2 },
            false,
        ),
    ];
    for (field, change, expected_same) in changes {
        let mut changed = base.clone();
        change(&mut changed);
        let expected = face_realization_identity(&base) == face_realization_identity(&changed);
        assert_eq!(expected, *expected_same, "fixture must change {field}");
        assert_eq!(same_face_realization(&base, &changed), expected, "{field}");
        assert_eq!(
            same_face_realization(&changed, &base),
            expected,
            "{field}, reversed"
        );
    }

    // Preserve PartialEq rather than replacing floating comparisons with bit
    // equality or making NaN faces reflexive as a pointer fast path might do.
    let mut positive_zero = base.clone();
    positive_zero.font_size = 0.0;
    let mut negative_zero = positive_zero.clone();
    negative_zero.font_size = -0.0;
    assert!(same_face_realization(&positive_zero, &negative_zero));
    let mut nan = base;
    nan.font_size = f32::NAN;
    assert!(!same_face_realization(&nan, &nan));
}

#[test]
fn borrowed_realization_compares_nested_payload_contents_and_float_semantics() {
    use neomacs_display_protocol::frame_glyphs::StipplePattern;
    use neomacs_display_protocol::gradient::{ColorStop, Gradient};

    let mut face = Face::new(FaceId::new(0));
    face.lisp_name = Some("nested payload face".into());
    face.background_gradient = Some(Box::new(Gradient::Linear {
        angle: 90.0,
        stops: vec![
            ColorStop::new(0.0, Color::BLACK),
            ColorStop::new(1.0, Color::WHITE),
        ],
    }));
    face.stipple = Some(Box::new(StipplePattern {
        width: 8,
        height: 2,
        bits: vec![0x55, 0xaa],
    }));
    let mut changed = face.clone();
    assert!(
        same_face_realization(&face, &changed),
        "equal independently owned payloads match"
    );
    changed.stipple.as_mut().unwrap().bits[1] ^= 1;
    assert!(!same_face_realization(&face, &changed));
    assert_eq!(
        same_face_realization(&face, &changed),
        face_realization_identity(&face) == face_realization_identity(&changed)
    );
    changed = face.clone();
    let Gradient::Linear { stops, .. } = changed.background_gradient.as_deref_mut().unwrap() else {
        unreachable!()
    };
    stops[1].position = 0.75;
    assert!(!same_face_realization(&face, &changed));
    assert_eq!(
        same_face_realization(&face, &changed),
        face_realization_identity(&face) == face_realization_identity(&changed)
    );
    let Gradient::Linear { stops, .. } = changed.background_gradient.as_deref_mut().unwrap() else {
        unreachable!()
    };
    stops[1].position = f32::NAN;
    assert!(
        !same_face_realization(&changed, &changed),
        "nested NaN remains nonreflexive"
    );
}

#[test]
fn borrowed_validation_preserves_enrichment_conflicts_without_mutating_published_faces() {
    use neomacs_display_protocol::font::ResolvedFontId;

    let mut base = Face::new(FaceId::new(0));
    base.font_family = "complete face identity".into();
    base.font_file_path = Some("/fonts/exact.ttf".into());
    base.default_resolved_font_id = Some(ResolvedFontId(7));
    base.font_ascent = 12;
    base.font_descent = 4;
    let mut attempt = FrameFaceArena::default().begin_attempt();
    attempt.import_face(base.clone()).unwrap();
    for path in [None, Some("/fonts/exact.ttf"), Some("/fonts/different.ttf")] {
        for font_id in [None, Some(ResolvedFontId(7)), Some(ResolvedFontId(8))] {
            let mut replacement = base.clone();
            replacement.font_file_path = path.map(str::to_owned);
            replacement.default_resolved_font_id = font_id;
            replacement.font_ascent = 18;
            replacement.font_descent = 0;
            let compatible =
                path != Some("/fonts/different.ttf") && font_id != Some(ResolvedFontId(8));
            assert_eq!(compatible_realization(&base, &replacement), compatible);
            assert_eq!(
                attempt.prepare_face(replacement.clone()).is_ok(),
                compatible
            );
            assert_eq!(
                attempt.face(base.id),
                Some(base.clone()),
                "preparation is speculative"
            );

            let mut merged = base.clone();
            assert_eq!(
                merge_compatible_realization(&mut merged, &replacement),
                compatible
            );
            if compatible {
                assert_eq!(merged.font_ascent, 18);
                assert_eq!(
                    merged.font_descent, 4,
                    "zero replacement metrics preserve enrichment"
                );
                assert_eq!(merged.font_file_path, base.font_file_path);
                assert_eq!(
                    merged.default_resolved_font_id,
                    base.default_resolved_font_id
                );
            } else {
                assert_eq!(merged, base, "a conflicting merge is atomic");
            }
        }
    }
    let mut wrong_id = base.clone();
    wrong_id.id = FaceId::new(1);
    assert!(same_face_realization(&base, &wrong_id));
    assert!(
        !compatible_realization(&base, &wrong_id),
        "merging must also preserve the slot ID"
    );
}

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

    let sealed = attempt
        .seal(HashMap::default())
        .expect("seal empty attempt");

    assert_ne!(
        sealed.generation(),
        arena.generation(),
        "each accepted presentation needs a distinct retained-face generation"
    );
}

#[test]
fn prepared_faces_reject_foreign_or_conflicting_namespaces() {
    let arena = FrameFaceArena::default();
    let mut first = arena.begin_attempt();
    let mut sibling = arena.begin_attempt();
    let id = FaceId::new(1);
    let mut face = Face::new(id);
    first.import_face(face.clone()).unwrap();
    face.font_size += 4.0;
    sibling.import_face(face).unwrap();
    let first = first.commit();
    let sibling = sibling.commit();
    let mut next = first.begin_attempt();
    assert!(
        next.admit_prepared([id], &sibling.prepared_snapshot(), &first)
            .is_err()
    );
    assert!(next.faces().is_empty());
    let foreign = FrameFaceArena::default();
    assert!(
        next.admit_prepared([id], &foreign.prepared_snapshot(), &first)
            .is_err()
    );
    assert!(next.faces().is_empty());
    next.admit_prepared([id], &first.prepared_snapshot(), &first)
        .unwrap();
    assert_eq!(next.face(id), first.faces.get(&id).cloned());
}

#[test]
fn prepared_dynamic_face_survives_a_page_that_does_not_use_it() {
    let arena = FrameFaceArena::default();
    let mut first = arena.begin_attempt();
    let mut face = Face::new(FaceId::new(0));
    face.foreground = Color::from_pixel(0x00112233);
    face.id = first.stable_face_id(face_realization_identity(&face));
    first.import_face(face.clone()).unwrap();
    let first = first.commit();
    fn require_send_sync<T: Send + Sync + 'static>() {}
    require_send_sync::<PreparedFaceSnapshot>();
    let prepared = first.prepared_snapshot();
    let second = first.begin_attempt().commit();
    drop(first);
    let prepared = std::thread::spawn(move || prepared)
        .join()
        .expect("prepared identities need no thread-local arena");
    assert!(second.faces.is_empty());
    let mut third = second.begin_attempt();
    third.admit_prepared([face.id], &prepared, &second).unwrap();
    assert_eq!(third.face(face.id), Some(face));
}

#[test]
fn worker_face_reservation_preserves_publication_and_serializes_identity_allocation() {
    let mut arena = FrameFaceArena::default();
    let generation = arena.generation();
    let mut attempt = arena.begin_attempt();
    let sibling = arena.begin_attempt();
    let resolved = crate::neovm_bridge::ResolvedFace::default();
    let id = crate::display_row::face_state::stable_face_id_for_resolved(&mut attempt, &resolved);
    let rendered = crate::display_row::face_state::resolved_display_row_face(id, &resolved, None)
        .render_face();
    attempt.import_face(rendered.clone()).unwrap();
    let prepared = arena.reserve_prepared(&attempt).unwrap();
    assert_eq!(arena.generation(), generation);
    assert!(
        arena.faces.is_empty(),
        "reservation must not publish speculative faces"
    );
    assert!(matches!(
        arena.reserve_prepared(&sibling),
        Err(FrameFaceReuseError::ForeignSnapshot)
    ));
    let mut fresh = arena.begin_attempt();
    fresh.admit_prepared([id], &prepared, &arena).unwrap();
    assert_eq!(fresh.face(id), Some(rendered));
    let mut other = resolved.clone();
    other.font_size *= 2.0;
    let other_id = crate::display_row::face_state::stable_face_id_for_resolved(&mut fresh, &other);
    assert_ne!(id, other_id);
    let foreign = FrameFaceArena::default().begin_attempt();
    assert!(matches!(
        arena.reserve_prepared(&foreign),
        Err(FrameFaceReuseError::ForeignArena)
    ));
    let invalidated = arena.invalidate();
    assert!(
        invalidated
            .begin_attempt()
            .admit_prepared([id], &prepared, &invalidated)
            .is_err()
    );
}

#[test]
fn admitting_repeated_prepared_glyph_faces_preserves_existing_storage() {
    let arena = FrameFaceArena::default();
    let mut source = arena.begin_attempt();
    let id = FaceId::new(1);
    let mut face = Face::new(id);
    face.font_family = "prepared face with owned family storage".to_owned();
    source.import_face(face).unwrap();
    let arena = source.commit();
    let prepared = arena.prepared_snapshot();
    let mut attempt = arena.begin_attempt();
    attempt.admit_prepared([id], &prepared, &arena).unwrap();
    let storage = attempt.state.borrow().faces[&id].font_family.as_ptr();
    // A second row references the same face. It must be checked against its
    // source namespace without replacing the identical, already owned face.
    attempt.admit_prepared([id], &prepared, &arena).unwrap();
    assert_eq!(
        attempt.state.borrow().faces[&id].font_family.as_ptr(),
        storage
    );
    attempt
        .admit_prepared(std::iter::repeat_n(id, 4096), &prepared, &arena)
        .unwrap();
    assert_eq!(
        attempt.state.borrow().faces[&id].font_family.as_ptr(),
        storage
    );
    assert_eq!(attempt.faces(), *arena.faces);
}

#[test]
fn repeated_prepared_faces_still_validate_each_source_namespace() {
    let mut source = FrameFaceArena::default().begin_attempt();
    let id = FaceId::new(1);
    source.import_face(Face::new(id)).unwrap();
    let arena = source.commit();
    let prepared = arena.prepared_snapshot();
    let mut attempt = arena.begin_attempt();
    attempt.admit_prepared([id], &prepared, &arena).unwrap();
    let before = attempt.faces();
    let mut conflict = prepared.clone();
    Arc::make_mut(&mut conflict.faces)
        .get_mut(&id)
        .unwrap()
        .font_size += 1.0;
    assert!(matches!(
        attempt.admit_prepared([id, id], &conflict, &arena),
        Err(FrameFaceReuseError::ConflictingFace(bad)) if bad == id
    ));
    assert_eq!(attempt.faces(), before);
    let missing = FaceId::new(99);
    assert!(matches!(
        attempt.admit_prepared([id, id, missing], &prepared, &arena),
        Err(FrameFaceReuseError::MissingFace(bad)) if bad == missing
    ));
    assert_eq!(attempt.faces(), before);
}

#[test]
fn resolved_binding_slot_matches_owned_binding_without_publishing() {
    let mut attempt = FrameFaceArena::default().begin_attempt();
    let mut resolved = crate::neovm_bridge::ResolvedFace::default();
    resolved.font_family = "slot-family".into();
    resolved.font_size = 19.0;
    resolved.font_ascent = 14.0;
    resolved.font_line_height = 19.0;
    resolved.italic = true;
    resolved.font_weight = 700;
    let id = crate::display_row::face_state::stable_face_id_for_resolved(&mut attempt, &resolved);
    let expected = attempt.bind_resolved_face(id, resolved.clone()).unwrap();
    let mut output = None;
    attempt
        .bind_resolved_face_into(id, resolved.clone(), &mut output)
        .unwrap();
    let bound = output.as_ref().unwrap();
    assert_eq!(bound.face_id(), expected.face_id());
    assert_eq!(bound.resolved(), expected.resolved());
    assert_eq!(bound.realized(None).face(), expected.realized(None).face());
    assert!(attempt.faces().is_empty());
    let measured_metrics = crate::font::metrics::FontMetrics {
        ascent: 20.0,
        descent: 6.0,
        line_height: 26.0,
        char_width: 11.0,
        space_width: 10.0,
    };
    let realized = bound.realized(Some(measured_metrics));
    assert_eq!(realized.face().font_ascent, 20);
    assert_eq!(realized.face().font_descent, 6);
    assert_eq!(bound.resolved(), &resolved);
    assert!(attempt.faces().is_empty());
    assert_eq!(attempt.use_face(&realized).unwrap(), id);
    assert!(
        FrameFaceArena::default()
            .begin_attempt()
            .publish_face(&realized)
            .is_err()
    );
    attempt.publish_face(&realized).unwrap();
    assert_eq!(attempt.face(id).as_ref(), Some(realized.face()));
}

#[test]
fn conflicting_resolved_binding_leaves_caller_slot_and_publication_unchanged() {
    let mut attempt = FrameFaceArena::default().begin_attempt();
    let resolved = crate::neovm_bridge::ResolvedFace::default();
    let id = crate::display_row::face_state::stable_face_id_for_resolved(&mut attempt, &resolved);
    let mut output = None;
    attempt
        .bind_resolved_face_into(id, resolved.clone(), &mut output)
        .unwrap();
    let retained = output.as_ref().unwrap().realized(None);
    attempt.publish_face(&retained).unwrap();
    let before = attempt.faces();
    let mut conflicting = resolved.clone();
    conflicting.font_size *= 0.75;
    assert!(
        attempt
            .bind_resolved_face_into(id, conflicting, &mut output)
            .is_err()
    );
    let unchanged = output.as_ref().unwrap();
    assert_eq!(unchanged.resolved(), &resolved);
    assert_eq!(unchanged.realized(None).face(), retained.face());
    assert_eq!(attempt.faces(), before);
    assert_eq!(attempt.use_face(&unchanged.realized(None)).unwrap(), id);
}
