use neomacs_display_protocol::font::FontBackendKind;

#[test]
fn default_backend_matches_the_build_target() {
    let backend = super::default_font_backend();
    #[cfg(target_os = "linux")]
    assert_eq!(backend.kind(), FontBackendKind::Fontconfig);
    #[cfg(target_os = "macos")]
    assert_eq!(backend.kind(), FontBackendKind::CoreText);
    #[cfg(windows)]
    assert_eq!(backend.kind(), FontBackendKind::DirectWrite);
}

#[cfg(target_os = "macos")]
#[test]
fn core_text_candidates_are_complete_scalable_native_answers() {
    use super::FontBackend as _;

    let backend = super::CoreTextBackend::default();
    let family = super::FontFamilyName::new(backend.resolve_family("monospace"))
        .expect("CoreText generic family");
    let query = super::FontCandidateQuery {
        scope: super::FontCandidateScope::Family(family.clone()),
        required: super::RequiredFontCoverage::Any,
        charset_ranges: Vec::new(),
        languages: Vec::new(),
        requested_weight: 400,
        requested_slant: neovm_core::face::FontSlant::Normal,
        requested_width: neovm_core::face::FontWidth::Normal,
        direction: super::TextDirection::LeftToRight,
    };
    let candidates = backend.list_candidates(&query);

    assert!(
        !candidates.is_empty(),
        "CoreText returned no face for resolved family {family:?}"
    );
    assert!(
        candidates.iter().all(|candidate| {
            candidate.matched.metadata.size == super::PlatformFontSize::Scalable
        })
    );

    let matched = candidates
        .into_iter()
        .find_map(|candidate| backend.finalize_match(candidate.matched))
        .expect("CoreText candidate has a replayable outline asset");
    let metrics = backend
        .design_metrics(&matched)
        .expect("CoreText exposes selected-face design metrics");
    assert!(metrics.units_per_em > 0);
    assert!(metrics.ascent > 0);
}

#[test]
fn native_design_metrics_scale_at_the_layout_boundary() {
    let metrics = super::PlatformFontDesignMetrics {
        units_per_em: 1_000,
        ascent: 800,
        descent: 200,
        line_gap: 100,
        max_advance: 700,
        space_advance: 500,
        average_advance: 600,
    }
    .at_pixel_size(20.0)
    .expect("valid design metrics");

    assert_eq!(metrics.ascent, 16);
    assert_eq!(metrics.descent, 4);
    assert_eq!(metrics.height, 22);
    assert_eq!(metrics.max_width, 14);
    assert_eq!(metrics.space_width, 10);
    assert_eq!(metrics.average_width, 12);
}

#[test]
fn native_fallback_direction_uses_unicode_bidi_class() {
    assert_eq!(
        super::TextDirection::for_char('א'),
        super::TextDirection::RightToLeft
    );
    assert_eq!(
        super::TextDirection::for_char('م'),
        super::TextDirection::RightToLeft
    );
    assert_eq!(
        super::TextDirection::for_char('好'),
        super::TextDirection::LeftToRight
    );
}

#[test]
fn catalog_coverage_requires_the_character_and_every_normalized_range() {
    let query = super::FontCandidateQuery {
        scope: super::FontCandidateScope::All,
        required: super::RequiredFontCoverage::Character('λ'),
        charset_ranges: vec![(0x43, 0x41)],
        languages: Vec::new(),
        requested_weight: 400,
        requested_slant: neovm_core::face::FontSlant::Normal,
        requested_width: neovm_core::face::FontWidth::Normal,
        direction: super::TextDirection::LeftToRight,
    };

    assert!(
        query.coverage_is_satisfied_by(|codepoint| { matches!(codepoint, 0x41..=0x43 | 0x3bb) })
    );
    assert!(
        !query.coverage_is_satisfied_by(|codepoint| { matches!(codepoint, 0x41 | 0x43 | 0x3bb) })
    );
    assert!(!query.coverage_is_satisfied_by(|codepoint| { matches!(codepoint, 0x41..=0x43) }));
}

#[test]
fn native_fallback_text_is_non_empty_and_requires_the_whole_cluster() {
    assert!(super::RequiredFontCoverage::for_text("").is_none());
    let required = super::RequiredFontCoverage::for_text("👩‍💻").unwrap();
    let query = super::FontCandidateQuery {
        scope: super::FontCandidateScope::All,
        required,
        charset_ranges: Vec::new(),
        languages: Vec::new(),
        requested_weight: 400,
        requested_slant: neovm_core::face::FontSlant::Normal,
        requested_width: neovm_core::face::FontWidth::Normal,
        direction: super::TextDirection::LeftToRight,
    };

    assert!(
        query.coverage_is_satisfied_by(|codepoint| {
            matches!(codepoint, 0x1f469 | 0x200d | 0x1f4bb)
        })
    );
    assert!(
        !query.coverage_is_satisfied_by(|codepoint| { matches!(codepoint, 0x1f469 | 0x1f4bb) })
    );
}

#[test]
fn catalog_language_hints_are_alternatives() {
    let query = super::FontCandidateQuery {
        scope: super::FontCandidateScope::All,
        required: super::RequiredFontCoverage::Any,
        charset_ranges: Vec::new(),
        languages: vec!["ja".into(), "zh-hans".into()],
        requested_weight: 400,
        requested_slant: neovm_core::face::FontSlant::Normal,
        requested_width: neovm_core::face::FontWidth::Normal,
        direction: super::TextDirection::LeftToRight,
    };

    assert!(query.languages_are_satisfied_by(|language| language == "zh-hans"));
    assert!(!query.languages_are_satisfied_by(|language| language == "ko"));
}
