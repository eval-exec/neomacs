use super::*;

fn fixed_first_catalog(pixels: u32) -> FontResolver {
    let mut fixed = candidate("Zero Size Fixture", 400, FontSlant::Normal, 100);
    fixed.matched.metadata.size = PlatformFontSize::Fixed {
        device_ppem_26_6: pixels * 64,
    };
    let scalable = candidate("Zero Size Fixture", 400, FontSlant::Normal, 100);
    FontResolver::new(Box::new(ScopedListingBackend {
        candidates: vec![fixed, scalable],
    }))
}

#[test]
fn preferred_zero_size_ranks_scalable_before_equal_style_positive_fixed() {
    let resolver = fixed_first_catalog(1);
    let query = FontEntityQuery::new(FontFamilyName::new("Zero Size Fixture"));
    for (pixel_size, expected) in [
        (
            None,
            PlatformFontSize::Fixed {
                device_ppem_26_6: 64,
            },
        ),
        (Some(0), PlatformFontSize::Scalable),
        (
            Some(1),
            PlatformFontSize::Fixed {
                device_ppem_26_6: 64,
            },
        ),
    ] {
        let preferred = FontEntityListConstraints {
            pixel_size,
            ..Default::default()
        };
        let listed = resolver.list_entities(
            &query,
            &Default::default(),
            Some((&query, &preferred)),
            None,
        );
        assert_eq!(
            listed.len(),
            2,
            "PREFER changes order without dropping fixed fonts"
        );
        assert_eq!(
            listed[0].candidate.metadata.size, expected,
            "{pixel_size:?}"
        );
    }
}

#[test]
fn primary_zero_size_does_not_reject_positive_fixed_entities() {
    let resolver = fixed_first_catalog(16);
    let query = FontEntityQuery::new(FontFamilyName::new("Zero Size Fixture"));
    let primary = FontEntityListConstraints {
        pixel_size: Some(0),
        ..Default::default()
    };
    let listed = resolver.list_entities(&query, &primary, None, None);
    assert_eq!(listed.len(), 2);
    assert_eq!(
        listed[0].candidate.metadata.size,
        PlatformFontSize::Fixed {
            device_ppem_26_6: 16 * 64
        }
    );
}
