use super::*;

fn scalability_config() -> super::super::test_config::TestFontconfig {
    let files = [
        neomacs_test_fonts::hack_regular_3_003().to_path_buf(),
        neomacs_test_fonts::spleen_2_2_0().bdf(),
        neomacs_test_fonts::noto_color_emoji_2_051().to_path_buf(),
    ];
    let generic = crate::font_backend::FontFamilyName::new("Hack").unwrap();
    super::super::test_config::TestFontconfig::new(&files, &generic, &generic, &generic)
        .expect("owned native outline, BDF and color-bitmap catalog")
}

fn reference_scalability_pattern(family: &str, scalable: Option<bool>) -> FcPatternGuard {
    // GNU ftfont_spec_pattern preserves an absent FC_SCALABLE property and
    // uses its exact native Boolean for an explicit :scalable extra.
    // SAFETY: the new pattern is immediately guarded and owns copied values.
    let pattern = FcPatternGuard(unsafe { fontconfig_sys::FcPatternCreate() });
    assert!(!pattern.0.is_null());
    let family = CString::new(family).unwrap();
    // SAFETY: pattern and NUL-terminated family remain live for the call.
    assert_ne!(
        unsafe {
            fontconfig_sys::FcPatternAddString(
                pattern.0,
                fontconfig::FC_FAMILY.as_ptr(),
                family.as_ptr().cast(),
            )
        },
        0
    );
    if let Some(scalable) = scalable {
        // SAFETY: the pattern owns the copied native Boolean.
        assert_ne!(
            unsafe {
                fontconfig_sys::FcPatternAddBool(
                    pattern.0,
                    fontconfig::FC_SCALABLE.as_ptr(),
                    i32::from(scalable),
                )
            },
            0
        );
    }
    pattern
}

#[test]
fn scalability_listing_matches_exact_native_boolean_for_outline_and_color_bitmap() {
    for family in ["Hack", "Noto Color Emoji", "Spleen"] {
        let config = scalability_config();
        let unconstrained = reference_scalability_pattern(family, None);
        assert!(
            !native_reference_candidates(&config, &unconstrained, ReferenceSelection::AllMetadata,)
                .is_empty(),
            "pinned family must be present: {family}"
        );
        for scalable in [None, Some(true), Some(false)] {
            let pattern = reference_scalability_pattern(family, scalable);
            let expected =
                native_reference_candidates(&config, &pattern, ReferenceSelection::AllMetadata);
            assert_eq!(
                config
                    .fresh_cache_view()
                    .list_scalable_candidates(Some(family), scalable),
                expected,
                "native FC_SCALABLE, not outline/strike-size inference: {family} {scalable:?}"
            );
        }
    }
}

#[test]
fn scalability_listing_keeps_native_scalable_color_bitmap_with_average_width_zero() {
    use crate::font::resolver::{FontEntityListConstraints, FontEntityQuery};
    let catalog = crate::font::metrics::test_catalog::TestFontCatalog::from_files(&[
        neomacs_test_fonts::hack_regular_3_003(),
        neomacs_test_fonts::noto_color_emoji_2_051(),
    ])
    .unwrap()
    .to_native_fontconfig();
    let service = catalog.metrics_service();
    let query = FontEntityQuery::new(crate::font_backend::FontFamilyName::new("Noto Color Emoji"));
    let constraints = FontEntityListConstraints {
        scalable: Some(true),
        average_width: Some(0),
        ..Default::default()
    };
    assert_eq!(
        service
            .list_font_entities(&query, &constraints, None, Some(1))
            .len(),
        1
    );
}

#[test]
fn scalability_cache_keeps_absent_true_and_false_queries_distinct() {
    let config = native_fixture();
    let count = || super::super::NATIVE_CANDIDATE_QUERIES.with(|count| count.get());
    let before = count();
    let all = config.list_scalable_candidates(Some("Hack"), None);
    assert!(!all.is_empty());
    assert_eq!(count(), before + 1);
    assert!(
        config
            .list_scalable_candidates(Some("Hack"), Some(false))
            .is_empty()
    );
    assert_eq!(
        count(),
        before + 2,
        "false is not the unconstrained cache entry"
    );
    assert!(
        config
            .list_scalable_candidates(Some("Hack"), Some(false))
            .is_empty()
    );
    assert_eq!(count(), before + 2, "negative false answer is reusable");
    assert_eq!(
        config.list_scalable_candidates(Some("Hack"), Some(true)),
        all
    );
    assert_eq!(
        count(),
        before + 3,
        "true retains its own native query identity"
    );
    assert_eq!(config.list_scalable_candidates(Some("Hack"), None), all);
    assert_eq!(
        count(),
        before + 3,
        "original unconstrained entry remains reusable"
    );
}
