use super::*;

fn preferred_size_catalog() -> FontResolver {
    let mut fixed = candidate("Preference Fixture", 400, FontSlant::Normal, 100);
    fixed.matched.metadata.size = PlatformFontSize::Fixed {
        device_ppem_26_6: 13 * 64,
    };
    let scalable = candidate("Preference Fixture", 700, FontSlant::Normal, 100);
    FontResolver::new(Box::new(ScopedListingBackend {
        candidates: vec![fixed, scalable],
    }))
}

#[test]
fn preferred_average_width_mismatch_uses_gnu_size_priority_before_weight() {
    let resolver = preferred_size_catalog();
    let query = FontEntityQuery::new(FontFamilyName::new("Preference Fixture"));
    let prefer = query.clone().with_weight(400);
    for average_width in [0, 120] {
        let constraints = FontEntityListConstraints {
            pixel_size: Some(13),
            average_width: Some(average_width),
            ..Default::default()
        };
        let listed = resolver.list_entities(
            &query,
            &Default::default(),
            Some((&prefer, &constraints)),
            Some(1),
        );
        assert_eq!(
            listed[0].candidate.metadata.size,
            PlatformFontSize::Scalable,
            "fixed entities have absent average width; GNU's size mismatch outranks weight"
        );
    }
    for pixel_size in [None, Some(13)] {
        let constraints = FontEntityListConstraints {
            pixel_size,
            ..Default::default()
        };
        let listed = resolver.list_entities(
            &query,
            &Default::default(),
            Some((&prefer, &constraints)),
            Some(1),
        );
        assert_eq!(
            listed[0].candidate.metadata.size,
            PlatformFontSize::Fixed {
                device_ppem_26_6: 13 * 64
            }
        );
    }
    let constraints = FontEntityListConstraints {
        average_width: Some(0),
        ..Default::default()
    };
    let listed = resolver.list_entities(
        &query,
        &Default::default(),
        Some((&prefer, &constraints)),
        Some(1),
    );
    assert_eq!(
        listed[0].candidate.metadata.size,
        PlatformFontSize::Fixed {
            device_ppem_26_6: 13 * 64
        },
        "GNU only scores numeric size metadata when a preferred size is present"
    );
}

#[test]
fn preferred_dpi_mismatch_shares_gnu_size_metadata_bit() {
    let resolver = preferred_size_catalog();
    let query = FontEntityQuery::new(FontFamilyName::new("Preference Fixture"));
    let prefer = query.clone().with_weight(400);
    for (pixel_size, expected) in [
        (Some(13), PlatformFontSize::Scalable),
        (
            None,
            PlatformFontSize::Fixed {
                device_ppem_26_6: 13 * 64,
            },
        ),
    ] {
        let constraints = FontEntityListConstraints {
            pixel_size,
            dpi: Some(96),
            ..Default::default()
        };
        let listed = resolver.list_entities(
            &query,
            &Default::default(),
            Some((&prefer, &constraints)),
            Some(1),
        );
        assert_eq!(listed[0].candidate.metadata.size, expected);
    }
}
