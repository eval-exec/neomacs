use super::*;

// =======================================================================
// BufferModiff
// =======================================================================

#[test]
fn equal_modiff_means_the_same_text_and_differing_modiff_does_not() {
    let before = BufferModiff::new(41);
    let same = BufferModiff::new(41);
    let after = BufferModiff::new(42);
    assert_eq!(before, same);
    assert_ne!(before, after);
    assert!(after > before, "the tick is monotonic");
}

#[test]
fn modiff_round_trips_its_tick() {
    assert_eq!(BufferModiff::new(u64::MAX).get(), u64::MAX);
    assert_eq!(BufferModiff::default().get(), 0);
}

// =======================================================================
// InputSerial
// =======================================================================

#[test]
fn input_serials_order_by_emission() {
    let first = InputSerial::FIRST;
    let second = first.next();
    let third = second.next();
    assert!(first < second && second < third);
    assert_eq!(third.get(), 2);
}

// =======================================================================
// InteractionSessionId
// =======================================================================

#[test]
fn session_ids_are_never_zero_so_the_option_is_free() {
    assert_eq!(InteractionSessionId::FIRST.get(), 1);
    assert_eq!(
        std::mem::size_of::<Option<InteractionSessionId>>(),
        std::mem::size_of::<InteractionSessionId>()
    );
}

#[test]
fn session_ids_advance_and_saturate_rather_than_reusing_one() {
    let first = InteractionSessionId::FIRST;
    let second = first.next();
    assert!(second > first);
    assert_eq!(second.get(), 2);

    // Reuse would make an ancient drag's commits look current, so the counter
    // saturates instead of wrapping back to 1.
    let last = InteractionSessionId::FIRST;
    let mut saturated = last;
    for _ in 0..3 {
        saturated = saturated.next();
    }
    assert_eq!(saturated.get(), 4);
}

// =======================================================================
// PresentationOrigin
// =======================================================================

#[test]
fn an_ordinary_presentation_animates_and_belongs_to_no_session() {
    let origin = PresentationOrigin::default();
    assert_eq!(origin, PresentationOrigin::Ordinary);
    assert!(!origin.suppresses_layout_motion());
    assert!(!origin.belongs_to(InteractionSessionId::FIRST));
}

#[test]
fn an_interactive_resize_presentation_suppresses_motion_for_its_own_session() {
    let session = InteractionSessionId::FIRST;
    let other = session.next();
    let origin = PresentationOrigin::InteractiveResize {
        session,
        through: InputSerial::new(17),
    };
    assert!(origin.suppresses_layout_motion());
    assert!(origin.belongs_to(session));
    assert!(
        !origin.belongs_to(other),
        "a different drag's commits are not this drag's"
    );
}

#[test]
fn origin_names_are_stable_for_diagnostics() {
    let name: &'static str = PresentationOrigin::Ordinary.into();
    assert_eq!(name, "ordinary");
    let name: &'static str = PresentationOrigin::InteractiveResize {
        session: InteractionSessionId::FIRST,
        through: InputSerial::FIRST,
    }
    .into();
    assert_eq!(name, "interactive-resize");
}

#[test]
fn origin_round_trips_through_json() {
    for origin in [
        PresentationOrigin::Ordinary,
        PresentationOrigin::InteractiveResize {
            session: InteractionSessionId::FIRST.next(),
            through: InputSerial::new(9),
        },
    ] {
        let json = serde_json::to_string(&origin).expect("serialize");
        let back: PresentationOrigin = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(origin, back);
    }
}

#[test]
fn a_zero_session_id_cannot_be_deserialized() {
    // NonZeroU64 rejects it, so "session zero" is unrepresentable on the wire
    // as well as in code.
    assert!(
        serde_json::from_str::<InteractionSessionId>("0").is_err(),
        "session ids start at one"
    );
    assert!(serde_json::from_str::<InteractionSessionId>("1").is_ok());
}
