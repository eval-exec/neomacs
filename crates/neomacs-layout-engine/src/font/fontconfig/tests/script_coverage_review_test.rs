use super::*;
use crate::font_backend::NativeFontScriptCoverage;

#[test]
fn script_all_requires_every_representative_in_pinned_hack_and_dejavu() {
    for family in ["Hack", "DejaVu Sans Mono"] {
        let config = native_fixture();
        assert!(
            !config
                .list_script_candidates(
                    Some(family),
                    &[],
                    &[],
                    &NativeFontScriptCoverage::All(vec![0x41, 0x03bb])
                )
                .is_empty()
        );
        assert!(
            config
                .list_script_candidates(
                    Some(family),
                    &[],
                    &[],
                    &NativeFontScriptCoverage::All(vec![0x41, 0x4e00])
                )
                .is_empty(),
            "ALL includes unsupported CJK: {family}"
        );
        assert!(
            config
                .list_script_candidates(
                    Some(family),
                    &[],
                    &[],
                    &NativeFontScriptCoverage::All(vec![0x3fffff])
                )
                .is_empty(),
            "Emacs characters outside Unicode still constrain native coverage"
        );
    }
}

#[test]
fn script_any_accepts_one_representative_and_rejects_empty_or_unsupported_vectors() {
    for family in ["Hack", "DejaVu Sans Mono"] {
        let config = native_fixture();
        assert!(
            !config
                .list_script_candidates(
                    Some(family),
                    &[],
                    &[],
                    &NativeFontScriptCoverage::Any(vec![0x4e00, 0x41])
                )
                .is_empty(),
            "ANY permits a supported alternative"
        );
        assert!(
            config
                .list_script_candidates(
                    Some(family),
                    &[],
                    &[],
                    &NativeFontScriptCoverage::Any(vec![0x4e00, u32::MAX])
                )
                .is_empty()
        );
        assert!(
            config
                .list_script_candidates(
                    Some(family),
                    &[],
                    &[],
                    &NativeFontScriptCoverage::Any(Vec::new())
                )
                .is_empty(),
            "an empty vector matches no font"
        );
    }
}

#[test]
fn existing_registry_charset_overrides_list_and_vector_script_coverage() {
    let config = native_fixture();
    let baseline = config.list_candidates(Some("Hack"), &[(0x41, 0x41)], None, &[]);
    assert!(!baseline.is_empty());
    for script in [
        NativeFontScriptCoverage::All(vec![0x4e00]),
        NativeFontScriptCoverage::Any(Vec::new()),
        NativeFontScriptCoverage::Any(vec![0x4e00]),
    ] {
        assert_eq!(
            config.list_script_candidates(Some("Hack"), &[(0x41, 0x41)], &[], &script),
            baseline,
            "a preexisting registry charset takes precedence over script representatives"
        );
    }
}

#[test]
fn script_payload_and_mode_partition_the_existing_native_query_cache() {
    let config = native_fixture();
    let unrestricted = config.list_candidates(Some("Hack"), &[], None, &[]);
    assert!(!unrestricted.is_empty());
    assert_eq!(
        config.list_script_candidates(
            Some("Hack"),
            &[],
            &[],
            &NativeFontScriptCoverage::All(Vec::new())
        ),
        unrestricted
    );
    assert!(
        config
            .list_script_candidates(
                Some("Hack"),
                &[],
                &[],
                &NativeFontScriptCoverage::Any(Vec::new())
            )
            .is_empty(),
        "ALL empty and ANY empty cannot share cached results"
    );
    assert!(
        config
            .list_script_candidates(
                Some("Hack"),
                &[],
                &[],
                &NativeFontScriptCoverage::All(vec![0x4e00])
            )
            .is_empty()
    );
    // GNU adds FC_CHARSET to a vector query's object projection. Native
    // enumeration order can differ from metadata-only listing even when all
    // fonts cover A, so compare against that exact independent projection.
    // SAFETY: the returned pattern is immediately owned by its guard.
    let pattern = FcPatternGuard(unsafe { fontconfig_sys::FcPatternCreate() });
    assert!(!pattern.0.is_null());
    // SAFETY: the pattern is live and copies this static family string.
    assert_ne!(
        unsafe {
            fontconfig_sys::FcPatternAddString(
                pattern.0,
                fontconfig::FC_FAMILY.as_ptr(),
                c"Hack".as_ptr().cast(),
            )
        },
        0
    );
    let expected_any =
        native_reference_candidates(&config, &pattern, ReferenceSelection::Covering('A'));
    assert_eq!(
        config.list_script_candidates(
            Some("Hack"),
            &[],
            &[],
            &NativeFontScriptCoverage::Any(vec![0x41])
        ),
        expected_any
    );
}

#[test]
fn resolver_forwards_script_before_max_and_unicode_shortcuts_do_not_override_it() {
    use crate::font::resolver::{FontEntityListConstraints, FontEntityQuery};
    let authority = crate::font::metrics::test_catalog::standard_catalog().to_native_fontconfig();
    let service = authority.metrics_service();
    for registry in [
        None,
        Some("iso10646-1"),
        Some("ascii-0"),
        Some("unicode-bmp"),
    ] {
        let mut query = FontEntityQuery::new(crate::font_backend::FontFamilyName::new("Hack"));
        if let Some(registry) = registry {
            query = query.with_registry(registry);
        }
        for script in [
            NativeFontScriptCoverage::All(vec![0x4e00]),
            NativeFontScriptCoverage::Any(vec![0x4e00]),
            NativeFontScriptCoverage::Any(Vec::new()),
        ] {
            let constraints = FontEntityListConstraints {
                script_coverage: Some(script),
                ..Default::default()
            };
            assert!(
                service
                    .list_font_entities(&query, &constraints, None, Some(1))
                    .is_empty(),
                "script selection precedes MAX for {registry:?}"
            );
        }
    }
    let query = FontEntityQuery::new(crate::font_backend::FontFamilyName::new("Hack"))
        .with_registry("iso8859-1");
    let constraints = FontEntityListConstraints {
        script_coverage: Some(NativeFontScriptCoverage::Any(Vec::new())),
        ..Default::default()
    };
    assert_eq!(
        service
            .list_font_entities(&query, &constraints, None, Some(1))
            .len(),
        1,
        "legacy registry charset overrides an empty script vector"
    );
}
