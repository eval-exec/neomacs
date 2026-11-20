use super::{CoverageRequirement, GnuFontPolicy, coalesce_ranges};
use neovm_core::emacs_core::fontset::StoredFontSpec;
use neovm_core::emacs_core::intern::intern;

#[test]
fn registry_constraints_are_platform_neutral_and_typed() {
    let spec = StoredFontSpec {
        family: None,
        registry: Some(intern("gb2312.1980-0")),
        lang: None,
        weight: None,
        slant: None,
        width: None,
        otf: None,
        definition: None,
    };

    let constraints = GnuFontPolicy::constraints_for_spec(&spec, '好');

    assert_eq!(constraints.representative_char(), '专');
    assert_eq!(constraints.languages()[0].as_str(), "zh-cn");
    assert_eq!(
        constraints.coverage(),
        &CoverageRequirement::Ranges(vec![(0x4e13, 0x4e13), ('好' as u32, '好' as u32),])
    );
}

#[test]
fn generic_unicode_registry_does_not_invent_a_charset_filter() {
    let spec = StoredFontSpec {
        family: None,
        registry: Some(intern("iso10646-1")),
        lang: None,
        weight: None,
        slant: None,
        width: None,
        otf: None,
        definition: None,
    };

    let constraints = GnuFontPolicy::constraints_for_spec(&spec, 'λ');

    assert_eq!(constraints.coverage(), &CoverageRequirement::Any);
}

#[test]
fn coverage_normalization_preserves_reversed_range_endpoints() {
    assert_eq!(
        coalesce_ranges(vec![(0x9fff, 0x4e00), (0x3400, 0x4dbf)]),
        vec![(0x3400, 0x4dbf), (0x4e00, 0x9fff)]
    );
}

// --- GNU `font_check_otf_features` / `font_check_otf` (src/font.c:2358-2425) ---

use super::{otf_features_match, otf_spec_matches};
use crate::font::probe::{OtfCapability, OtfLangSys, OtfScript};
use neovm_core::emacs_core::fontset::{StoredOtfFeatures, StoredOtfLangsys, StoredOtfSpec};

fn features(required: &[&str], forbidden: &[&str]) -> StoredOtfFeatures {
    StoredOtfFeatures {
        required: required.iter().map(|tag| (*tag).to_owned()).collect(),
        forbidden: forbidden.iter().map(|tag| (*tag).to_owned()).collect(),
    }
}

fn langsys(tag: Option<&str>, feature_tags: &[&str]) -> OtfLangSys {
    OtfLangSys {
        tag: tag.map(str::to_owned),
        features: feature_tags.iter().map(|tag| (*tag).to_owned()).collect(),
    }
}

fn script(tag: &str, lang_syses: Vec<OtfLangSys>) -> OtfScript {
    OtfScript {
        tag: tag.to_owned(),
        lang_syses,
    }
}

fn spec(
    script_tag: &str,
    langsys: StoredOtfLangsys,
    gsub: Option<StoredOtfFeatures>,
    gpos: Option<StoredOtfFeatures>,
) -> StoredOtfSpec {
    StoredOtfSpec {
        script: script_tag.to_owned(),
        langsys,
        gsub,
        gpos,
    }
}

#[test]
fn otf_features_match_prefers_default_langsys_and_falls_back_to_the_first() {
    let table = vec![script(
        "arab",
        vec![
            langsys(Some("URD "), &["fina"]),
            langsys(None, &["init", "medi"]),
        ],
    )];
    assert!(otf_features_match(
        "arab",
        &StoredOtfLangsys::Default,
        &features(&["init"], &[]),
        &table
    ));
    // The default langsys is chosen, so a feature only URD carries is absent.
    assert!(!otf_features_match(
        "arab",
        &StoredOtfLangsys::Default,
        &features(&["fina"], &[]),
        &table
    ));

    let no_default = vec![script("arab", vec![langsys(Some("URD "), &["fina"])])];
    // With no default entry GNU takes XCAR of the langsys alist, the first one.
    assert!(otf_features_match(
        "arab",
        &StoredOtfLangsys::Default,
        &features(&["fina"], &[]),
        &no_default
    ));
}

#[test]
fn otf_features_match_named_langsys_must_exist() {
    let table = vec![script(
        "arab",
        vec![langsys(None, &["init"]), langsys(Some("URD "), &["fina"])],
    )];
    assert!(otf_features_match(
        "arab",
        &StoredOtfLangsys::Named("URD ".to_owned()),
        &features(&["fina"], &[]),
        &table
    ));
    assert!(!otf_features_match(
        "arab",
        &StoredOtfLangsys::Named("URD ".to_owned()),
        &features(&["init"], &[]),
        &table
    ));
    // A named language system the font does not advertise rejects the font.
    assert!(!otf_features_match(
        "arab",
        &StoredOtfLangsys::Named("KUR ".to_owned()),
        &features(&["init"], &[]),
        &table
    ));
}

#[test]
fn otf_features_match_applies_required_then_nil_separated_forbidden_tags() {
    let table = vec![script(
        "arab",
        vec![langsys(None, &["init", "medi", "liga"])],
    )];
    let default = StoredOtfLangsys::Default;
    assert!(otf_features_match(
        "arab",
        &default,
        &features(&["init", "medi"], &["mark"]),
        &table
    ));
    assert!(!otf_features_match(
        "arab",
        &default,
        &features(&["init"], &["liga"]),
        &table
    ));
    assert!(!otf_features_match(
        "arab",
        &default,
        &features(&["rlig"], &[]),
        &table
    ));
    // An empty feature table still satisfies a forbidden-only requirement.
    let empty = vec![script("arab", vec![])];
    assert!(otf_features_match(
        "arab",
        &default,
        &features(&[], &["liga"]),
        &empty
    ));
}

#[test]
fn otf_features_match_missing_script_rejects_even_a_forbidden_only_list() {
    let table = vec![script("latn", vec![langsys(None, &["liga"])])];
    assert!(!otf_features_match(
        "arab",
        &StoredOtfLangsys::Default,
        &features(&[], &["liga"]),
        &table
    ));
}

#[test]
fn otf_features_match_unmatchable_langsys_rejects_every_font() {
    let table = vec![script("arab", vec![langsys(None, &["init"])])];
    assert!(!otf_features_match(
        "arab",
        &StoredOtfLangsys::Unmatchable,
        &features(&["init"], &[]),
        &table
    ));
}

#[test]
fn otf_spec_matches_checks_gsub_and_gpos_independently() {
    let capability = OtfCapability {
        gsub: vec![script("arab", vec![langsys(None, &["init", "fina"])])],
        gpos: vec![script("arab", vec![langsys(None, &["kern"])])],
    };
    assert!(otf_spec_matches(
        &spec(
            "arab",
            StoredOtfLangsys::Default,
            Some(features(&["init"], &[])),
            Some(features(&["kern"], &[])),
        ),
        &capability
    ));
    assert!(!otf_spec_matches(
        &spec(
            "arab",
            StoredOtfLangsys::Default,
            Some(features(&["init"], &[])),
            Some(features(&["mark"], &[])),
        ),
        &capability
    ));
    // GNU skips a side whose feature list is nil, so the script is not checked.
    assert!(otf_spec_matches(
        &spec("zzzz", StoredOtfLangsys::Default, None, None),
        &capability
    ));
    assert!(!otf_spec_matches(
        &spec(
            "zzzz",
            StoredOtfLangsys::Unmatchable,
            None,
            Some(features(&[], &["kern"])),
        ),
        &capability
    ));
}

// --- GNU ftfont list-level `:otf` filter (src/ftfont.c:1026-1041) ---

use super::{otf_listing_side_matches, otf_representative_ranges, otf_spec_matches_for_listing};

#[test]
fn otf_listing_requires_the_script_in_both_tables() {
    let capability = OtfCapability {
        gsub: vec![script("arab", vec![langsys(None, &["init"])])],
        gpos: vec![script("latn", vec![langsys(None, &["kern"])])],
    };
    assert!(!otf_spec_matches_for_listing(
        &spec(
            "arab",
            StoredOtfLangsys::Default,
            Some(features(&["init"], &[])),
            None,
        ),
        &capability
    ));
    // Even a spec with no feature lists must find the script in both tables:
    // hbotf_check_features is called for GSUB and GPOS unconditionally.
    assert!(!otf_spec_matches_for_listing(
        &spec("arab", StoredOtfLangsys::Default, None, None),
        &capability
    ));
    let capability = OtfCapability {
        gsub: vec![script("arab", vec![langsys(None, &["init"])])],
        gpos: vec![script("arab", vec![langsys(None, &["kern"])])],
    };
    assert!(otf_spec_matches_for_listing(
        &spec("arab", StoredOtfLangsys::Default, None, None),
        &capability
    ));
}

#[test]
fn otf_listing_named_langsys_must_exist_in_each_table() {
    let capability = OtfCapability {
        gsub: vec![script(
            "arab",
            vec![langsys(None, &["init"]), langsys(Some("URD "), &["fina"])],
        )],
        gpos: vec![script("arab", vec![langsys(None, &["kern"])])],
    };
    let named = StoredOtfLangsys::Named("URD ".to_owned());
    assert!(otf_listing_side_matches(
        "arab",
        &named,
        Some(&features(&["fina"], &[])),
        &capability.gsub
    ));
    // The GPOS side has no URD langsys, so the font does not list.
    assert!(!otf_spec_matches_for_listing(
        &spec("arab", named, Some(features(&["fina"], &[])), None),
        &capability
    ));
}

#[test]
fn otf_listing_default_langsys_does_not_fall_back_to_the_first() {
    let table = vec![script("arab", vec![langsys(Some("URD "), &["init"])])];
    assert!(!otf_listing_side_matches(
        "arab",
        &StoredOtfLangsys::Default,
        Some(&features(&["init"], &[])),
        &table
    ));
    // With no features the script alone satisfies the side.
    assert!(otf_listing_side_matches(
        "arab",
        &StoredOtfLangsys::Default,
        None,
        &table
    ));
    // A forbidden-only list on an absent default langsys sees no features.
    assert!(otf_listing_side_matches(
        "arab",
        &StoredOtfLangsys::Default,
        Some(&features(&[], &["init"])),
        &table
    ));
}

#[test]
fn otf_listing_rejects_an_unmatchable_langsys() {
    let table = vec![script("arab", vec![langsys(None, &["init"])])];
    assert!(!otf_listing_side_matches(
        "arab",
        &StoredOtfLangsys::Unmatchable,
        None,
        &table
    ));
}

#[test]
fn otf_representative_ranges_follow_the_gnu_script_tables() {
    let arab = spec("arab", StoredOtfLangsys::Default, None, None);
    assert_eq!(
        otf_representative_ranges(&arab, None),
        vec![(0x628, 0x628), (0x6c1, 0x6c1)]
    );
    // GNU's ftfont adds the representative charset only when Fontconfig has no
    // registry charset of its own: the generic registries keep it, a concrete
    // registry replaces it.
    assert_eq!(
        otf_representative_ranges(&arab, Some("iso10646-1")),
        vec![(0x628, 0x628), (0x6c1, 0x6c1)]
    );
    assert_eq!(
        otf_representative_ranges(&arab, Some(" UNICODE-BMP ")),
        vec![(0x628, 0x628), (0x6c1, 0x6c1)]
    );
    assert!(otf_representative_ranges(&arab, Some("iso8859-1")).is_empty());
    assert!(
        otf_representative_ranges(&spec("zzzz", StoredOtfLangsys::Default, None, None), None)
            .is_empty()
    );
}
