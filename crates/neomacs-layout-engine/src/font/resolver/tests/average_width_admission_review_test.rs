use super::*;

#[test]
fn listed_average_width_filters_known_nonzero_values_even_for_requested_zero() {
    for actual in [None, Some(0), Some(80)] {
        let mut font = candidate("Width Fixture", 400, FontSlant::Normal, 100);
        font.matched.metadata.size = PlatformFontSize::Fixed {
            device_ppem_26_6: 16 * 64,
        };
        font.matched.metadata.average_width = actual;
        let resolver = FontResolver::new(Box::new(ScopedListingBackend {
            candidates: vec![font],
        }));
        let query = FontEntityQuery::new(FontFamilyName::new("Width Fixture"));
        for requested in [0, 80, 120] {
            let constraints = FontEntityListConstraints {
                average_width: Some(requested),
                ..Default::default()
            };
            assert_eq!(
                !resolver
                    .list_entities(&query, &constraints, None, Some(1))
                    .is_empty(),
                actual.is_none_or(|width| width == 0 || width == requested),
                "known {actual:?}, requested {requested}"
            );
        }
    }
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn windows_generic_adstyle_admits_aliases_and_ignores_unknown_names() {
    let mut mono = candidate("Windows Generic Fixture", 400, FontSlant::Normal, 100);
    mono.matched.metadata.adstyle = Some("mono".to_owned());
    let resolver = FontResolver::new(Box::new(FileSizeListingBackend {
        candidates: vec![mono],
    }));
    let query = FontEntityQuery::new(FontFamilyName::new("Windows Generic Fixture"));
    for (requested, expected) in [
        ("mono", true),
        ("monospace", true),
        ("sans", false),
        ("sans-serif", false),
        ("serif", false),
        ("script", false),
        ("decorative", false),
        ("unrecognized", true),
        ("Mono", true),
    ] {
        let constraints = FontEntityListConstraints {
            adstyle: Some(requested.to_owned()),
            ..Default::default()
        };
        assert_eq!(
            !resolver
                .list_entities(&query, &constraints, None, Some(1))
                .is_empty(),
            expected,
            "{requested}"
        );
    }
}
