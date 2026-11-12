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
    let hack = FontEntityQuery::new(crate::font_backend::FontFamilyName::new("Hack"));
    assert!(
        service
            .list_font_entities(&hack, &constraints, None, Some(1))
            .is_empty()
    );
    let dejavu = FontEntityQuery::new(crate::font_backend::FontFamilyName::new("DejaVu Sans"));
    assert_eq!(
        service
            .list_font_entities(&dejavu, &constraints, None, Some(1))
            .len(),
        1
    );
}
