use super::*;
use crate::font_backend::{NativeFontOtfFeature as Feature, NativeFontOtfRequirement};

fn otf_config() -> super::super::test_config::TestFontconfig {
    let files = [
        neomacs_test_fonts::hack_regular_3_003().to_path_buf(),
        neomacs_test_fonts::dejavu_2_37()
            .sans_regular()
            .to_path_buf(),
    ];
    let generic = crate::font_backend::FontFamilyName::new("Hack").unwrap();
    super::super::test_config::TestFontconfig::new(&files, &generic, &generic, &generic)
        .expect("pinned Hack and DejaVu native catalog")
}

fn requirement(script: [u8; 4]) -> NativeFontOtfRequirement {
    NativeFontOtfRequirement {
        script,
        language: None,
        gsub: vec![],
        gpos: vec![],
    }
}

#[test]
fn otf_listing_requires_the_script_in_both_gsub_and_gpos() {
    let config = otf_config();
    assert!(!config.list_otf_candidates(Some("Hack"), None).is_empty());
    assert!(
        config
            .list_otf_candidates(Some("Hack"), Some(&requirement(*b"latn")))
            .is_empty(),
        "pinned GNU rejects Hack's GSUB-only layout even without feature constraints"
    );
}

#[test]
fn otf_listing_rejects_missing_script_and_required_features() {
    let config = otf_config();
    let mut absent_feature = requirement(*b"latn");
    absent_feature.gsub.push(Feature::Required(*b"zzzz"));
    for request in [requirement(*b"zzzz"), absent_feature] {
        assert!(
            config
                .list_otf_candidates(Some("DejaVu Sans"), Some(&request))
                .is_empty()
        );
    }
}

#[test]
fn otf_listing_keeps_positive_features_and_missing_negative_features() {
    let config = otf_config();
    let mut request = requirement(*b"latn");
    request.gsub = vec![Feature::Required(*b"ccmp"), Feature::Forbidden(*b"zzzz")];
    request.gpos = vec![Feature::Required(*b"kern")];
    assert!(
        !config
            .list_otf_candidates(Some("DejaVu Sans"), Some(&request))
            .is_empty()
    );
}

#[test]
fn otf_listing_rejects_present_negative_features_and_wrong_table() {
    let config = otf_config();
    let mut forbidden = requirement(*b"latn");
    forbidden.gsub.push(Feature::Forbidden(*b"ccmp"));
    let mut wrong_table = requirement(*b"latn");
    wrong_table.gpos.push(Feature::Required(*b"ccmp"));
    for request in [forbidden, wrong_table] {
        assert!(
            config
                .list_otf_candidates(Some("DejaVu Sans"), Some(&request))
                .is_empty()
        );
    }
}

#[test]
fn otf_listing_preserves_observed_harfbuzz_unknown_language_no_feature_result() {
    let config = otf_config();
    for language in [None, Some(*b"ISM "), Some(*b"zzzz")] {
        let mut request = requirement(*b"latn");
        request.language = language;
        assert!(
            !config
                .list_otf_candidates(Some("DejaVu Sans"), Some(&request))
                .is_empty(),
            "strict GNU 31.1 HarfBuzz control: {language:?}"
        );
    }
}

#[test]
fn otf_listing_cache_separates_absence_script_features_and_polarity() {
    let config = otf_config();
    let count = || super::super::NATIVE_CANDIDATE_QUERIES.with(|count| count.get());
    let before = count();
    let all = config.list_otf_candidates(Some("DejaVu Sans"), None);
    assert!(!all.is_empty());
    let mut present = requirement(*b"latn");
    present.gsub.push(Feature::Required(*b"ccmp"));
    let mut absent = present.clone();
    absent.gsub = vec![Feature::Forbidden(*b"ccmp")];
    assert_eq!(
        config.list_otf_candidates(Some("DejaVu Sans"), Some(&present)),
        all
    );
    assert!(
        config
            .list_otf_candidates(Some("DejaVu Sans"), Some(&absent))
            .is_empty()
    );
    assert!(
        config
            .list_otf_candidates(Some("DejaVu Sans"), Some(&absent))
            .is_empty()
    );
    assert!(
        config
            .list_otf_candidates(Some("DejaVu Sans"), Some(&requirement(*b"zzzz")))
            .is_empty()
    );
    assert_eq!(
        count(),
        before + 4,
        "negative OTF answers are reusable but distinct"
    );
    assert_eq!(config.list_otf_candidates(Some("DejaVu Sans"), None), all);
    assert_eq!(count(), before + 4);
}

#[test]
fn otf_listing_public_pipeline_filters_before_maximum() {
    use crate::font::resolver::{FontEntityListConstraints, FontEntityQuery};
    let catalog = crate::font::metrics::test_catalog::TestFontCatalog::from_files(&[
        neomacs_test_fonts::hack_regular_3_003(),
        neomacs_test_fonts::dejavu_2_37().sans_regular(),
    ])
    .unwrap()
    .to_native_fontconfig();
    let service = catalog.metrics_service();
    let constraints = FontEntityListConstraints {
        otf: Some(requirement(*b"latn")),
        ..Default::default()
    };
    let query = FontEntityQuery::new(None);
    let unfiltered = service.list_font_entities(&query, &Default::default(), None, None);
    assert!(
        unfiltered
            .iter()
            .any(|font| font.candidate.metadata.family == "Hack")
    );
    assert!(
        unfiltered
            .iter()
            .any(|font| font.candidate.metadata.family == "DejaVu Sans")
    );
    // FcFontList does not define order. The backend receives the OTF query
    // without a result limit; MAX applies to its accepted results below.
    let listed = service.list_font_entities(&query, &constraints, None, Some(1));
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].candidate.metadata.family, "DejaVu Sans");
}

#[test]
fn otf_listing_checks_each_variable_font_file_once_per_native_enumeration() {
    let file = neomacs_test_fonts::noto_sans_mono_variable();
    let family = crate::font_backend::FontFamilyName::new("Noto Sans Mono").unwrap();
    let config = super::super::test_config::TestFontconfig::new(
        &[file.to_path_buf()],
        &family,
        &family,
        &family,
    )
    .unwrap();
    let unfiltered = config.list_otf_candidates(Some("Noto Sans Mono"), None);
    assert!(
        unfiltered.len() > 1,
        "native named instances must share this variable file"
    );
    assert!(
        unfiltered
            .iter()
            .all(|font| font.matched.file.as_deref() == file.to_str())
    );
    let count = || super::super::otf_requirement::NATIVE_FILE_CHECKS.with(std::cell::Cell::get);
    let before = count();
    let positive = requirement(*b"latn");
    assert_eq!(
        config.list_otf_candidates(Some("Noto Sans Mono"), Some(&positive)),
        unfiltered
    );
    assert_eq!(
        count(),
        before + 1,
        "all named instances reuse one face-zero check"
    );
    assert_eq!(
        config.list_otf_candidates(Some("Noto Sans Mono"), Some(&positive)),
        unfiltered
    );
    assert_eq!(
        count(),
        before + 1,
        "the completed native query cache avoids all file checks"
    );
    let mut negative = positive.clone();
    negative.gsub.push(Feature::Required(*b"zzzz"));
    assert!(
        config
            .list_otf_candidates(Some("Noto Sans Mono"), Some(&negative))
            .is_empty()
    );
    assert_eq!(
        count(),
        before + 2,
        "a negative requirement is also checked once per file"
    );
    assert!(
        config
            .list_otf_candidates(Some("Noto Sans Mono"), Some(&negative))
            .is_empty()
    );
    assert_eq!(count(), before + 2);
    assert_eq!(
        config
            .fresh_cache_view()
            .list_otf_candidates(Some("Noto Sans Mono"), Some(&positive)),
        unfiltered
    );
    assert_eq!(
        count(),
        before + 3,
        "per-enumeration results do not leak into another query owner"
    );
}
