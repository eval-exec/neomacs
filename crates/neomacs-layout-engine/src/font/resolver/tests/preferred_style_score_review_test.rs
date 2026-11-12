use super::*;

fn preferred_first(
    candidates: Vec<FontCandidate>,
    preference: &FontEntityQuery,
    pixel_size: Option<u32>,
) -> PlatformFontMetadata {
    preferred_with_constraints(
        candidates,
        preference,
        FontEntityListConstraints {
            pixel_size,
            ..Default::default()
        },
    )
}

fn preferred_with_constraints(
    candidates: Vec<FontCandidate>,
    preference: &FontEntityQuery,
    preferred: FontEntityListConstraints,
) -> PlatformFontMetadata {
    let resolver = FontResolver::new(Box::new(ScopedListingBackend { candidates }));
    let listed = resolver.list_entities(
        &FontEntityQuery::new(FontFamilyName::new("Style Score Fixture")),
        &Default::default(),
        Some((preference, &preferred)),
        Some(1),
    );
    assert_eq!(listed.len(), 1);
    listed.into_iter().next().unwrap().candidate.metadata
}

#[test]
fn preferred_listing_distinguishes_numeric_italic_and_oblique_slants() {
    for pixel_size in [None, Some(13)] {
        for (first, requested) in [
            (FontSlant::Italic, FontSlant::Oblique),
            (FontSlant::Oblique, FontSlant::Italic),
            (FontSlant::ReverseItalic, FontSlant::ReverseOblique),
            (FontSlant::ReverseOblique, FontSlant::ReverseItalic),
        ] {
            let preference = FontEntityQuery::new(FontFamilyName::new("Style Score Fixture"))
                .with_slant(requested);
            let selected = preferred_first(
                vec![
                    candidate("Style Score Fixture", 400, first, 100),
                    candidate("Style Score Fixture", 400, requested, 100),
                ],
                &preference,
                pixel_size,
            );
            assert_eq!(
                selected.slant, requested,
                "size {pixel_size:?}, first {first:?}"
            );
        }
    }
}

#[test]
fn preferred_listing_does_not_invent_a_weight_for_missing_metadata() {
    for pixel_size in [None, Some(13)] {
        let mut missing = candidate("Style Score Fixture", 400, FontSlant::Normal, 100);
        missing.matched.metadata.weight = None;
        let preference =
            FontEntityQuery::new(FontFamilyName::new("Style Score Fixture")).with_weight(700);
        let selected = preferred_first(
            vec![
                missing,
                candidate("Style Score Fixture", 600, FontSlant::Normal, 100),
            ],
            &preference,
            pixel_size,
        );
        assert_eq!(selected.weight, None, "size {pixel_size:?}");
    }
}

#[test]
fn preferred_listing_does_not_invent_a_width_for_missing_metadata() {
    for pixel_size in [None, Some(13)] {
        let mut missing = candidate("Style Score Fixture", 400, FontSlant::Normal, 100);
        missing.matched.metadata.width = None;
        let mut nearer = candidate("Style Score Fixture", 400, FontSlant::Normal, 100);
        nearer.matched.metadata.width = Some(FontWidth::SemiExpanded);
        let preference = FontEntityQuery::new(FontFamilyName::new("Style Score Fixture"))
            .with_width(FontWidth::Expanded);
        let selected = preferred_first(vec![missing, nearer], &preference, pixel_size);
        assert_eq!(selected.width, None, "size {pixel_size:?}");
    }
}

fn fixed_average_width_first(
    pixel_size: Option<u32>,
    average_width: Option<i32>,
    first: Option<i32>,
) -> Option<i32> {
    let fixed = |average_width| {
        let mut candidate = candidate("Style Score Fixture", 400, FontSlant::Normal, 100);
        candidate.matched.metadata.size = PlatformFontSize::Fixed {
            device_ppem_26_6: 13 * 64,
        };
        candidate.matched.metadata.average_width = average_width;
        candidate
    };
    let preference = FontEntityQuery::new(FontFamilyName::new("Style Score Fixture"));
    preferred_with_constraints(
        vec![fixed(first), fixed(Some(120))],
        &preference,
        FontEntityListConstraints {
            pixel_size,
            average_width,
            ..Default::default()
        },
    )
    .average_width
}

#[test]
fn preferred_listing_average_width_matching_fixed_entity_has_no_lowbit_penalty() {
    assert_eq!(
        fixed_average_width_first(Some(13), Some(120), Some(130)),
        Some(120)
    );
}

#[test]
fn preferred_listing_average_width_absent_entity_mismatches_a_present_preference() {
    assert_eq!(
        fixed_average_width_first(Some(13), Some(120), None),
        Some(120)
    );
}

#[test]
fn preferred_listing_average_width_is_unscored_without_a_preferred_size() {
    assert_eq!(
        fixed_average_width_first(None, Some(120), Some(130)),
        Some(130)
    );
}

#[test]
fn preferred_listing_average_width_is_unscored_without_a_preferred_average_width() {
    assert_eq!(
        fixed_average_width_first(Some(13), None, Some(130)),
        Some(130)
    );
}
