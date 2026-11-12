use super::*;
use crate::font_backend::{FontCandidateScope, RequiredFontCoverage, TextDirection};
use neovm_core::face::{FontSlant, FontWidth};

#[test]
fn windows_generic_family_metadata_preserves_all_gnu_classes() {
    // GNU w32_enumfont_pattern_entity masks the native tmPitchAndFamily,
    // independently of fixed/variable pitch in its lower four bits.
    for (family, expected) in [
        (0x10, "serif"),
        (0x20, "sans"),
        (0x30, "mono"),
        (0x40, "script"),
        (0x50, "decorative"),
    ] {
        for pitch in 0..16 {
            assert_eq!(generic_adstyle(family | pitch), Some(expected));
        }
    }
    for family in [0, 0x60, 0xf0] {
        assert_eq!(generic_adstyle(family), None);
    }
}

#[test]
fn windows_generic_family_probe_requires_matching_style_and_unambiguous_physical_records() {
    let mut probe = GenericFamilyProbe::new(400, 0);
    probe.observe(700, 0, 0x30);
    probe.observe(400, 1, 0x30);
    assert_eq!(probe.adstyle(), None);
    probe.observe(400, 0, 0x31);
    assert_eq!(probe.adstyle(), Some("mono"));
    probe.observe(400, 0, 0x32);
    assert_eq!(probe.adstyle(), Some("mono"));
    probe.observe(400, 0, 0x20);
    assert_eq!(probe.adstyle(), None);
    probe.observe(400, 0, 0x30);
    assert_eq!(
        probe.adstyle(),
        None,
        "a later record cannot hide ambiguity"
    );
}

fn query() -> FontCandidateQuery {
    FontCandidateQuery {
        scalable: None,
        otf: None,
        script_coverage: None,
        scope: FontCandidateScope::All,
        required: RequiredFontCoverage::Any,
        charset_ranges: Vec::new(),
        languages: Vec::new(),
        explicit_language: None,
        requested_weight: 400,
        requested_slant: FontSlant::Normal,
        requested_width: FontWidth::Normal,
        direction: TextDirection::LeftToRight,
    }
}

fn os2(codepages: u32) -> Vec<u8> {
    let mut bytes = vec![0; 86];
    bytes[..2].copy_from_slice(&1u16.to_be_bytes());
    bytes[78..82].copy_from_slice(&codepages.to_be_bytes());
    bytes
}

#[test]
fn windows_query_any_still_requires_every_legacy_registry_character() {
    let mut query = query();
    query.charset_ranges = vec![(0x00a0, 0x00a0), (0x010e, 0x010e)];
    assert!(!query_is_satisfied_by(
        &query,
        |cps| cps.iter().all(|cp| *cp == 0x00a0),
        || None
    ));
    assert!(query_is_satisfied_by(
        &query,
        |cps| cps.iter().all(|cp| matches!(*cp, 0x00a0 | 0x010e)),
        || None
    ));
    query.required = RequiredFontCoverage::Character('λ');
    assert!(!query_is_satisfied_by(
        &query,
        |cps| cps.iter().all(|cp| matches!(*cp, 0x00a0 | 0x010e)),
        || None
    ));
    assert!(query_is_satisfied_by(
        &query,
        |cps| cps.iter().all(|cp| matches!(*cp, 0x00a0 | 0x010e | 0x03bb)),
        || None
    ));
}

#[test]
fn windows_query_rejects_missing_or_invalid_registry_scalars_before_language_lookup() {
    let mut query = query();
    query.charset_ranges = vec![(0x010e, 0x010e)];
    assert!(!query_is_satisfied_by(
        &query,
        |_| false,
        || panic!("no language request")
    ));
    query.charset_ranges = vec![(0xd800, 0xd800)];
    assert!(!query_is_satisfied_by(
        &query,
        |_| panic!("invalid scalar reached native API"),
        || None
    ));
}

#[test]
fn windows_query_explicit_cjk_languages_require_the_gnu_codepage_masks() {
    for (language, matching_bits) in [
        ("ja", 1 << 17),
        ("ko", (1 << 19) | (1 << 21)),
        ("zh", (1 << 18) | (1 << 20)),
    ] {
        let mut query = query();
        query.explicit_language = Some(language.to_owned());
        assert!(!query_is_satisfied_by(&query, |_| true, || Some(os2(1))));
        for bit in 0..32 {
            assert_eq!(
                query_is_satisfied_by(&query, |_| true, || Some(os2(1 << bit))),
                matching_bits & (1 << bit) != 0,
                "{language} bit {bit}"
            );
        }
    }
}

#[test]
fn windows_query_explicit_language_does_not_normalize_or_guess_unsupported_tags() {
    for language in ["JA", "ja-JP", "zh-cn", "ru", "en", ""] {
        let mut query = query();
        query.explicit_language = Some(language.to_owned());
        assert!(
            !query_is_satisfied_by(&query, |_| true, || Some(os2(u32::MAX))),
            "{language}"
        );
    }
}

#[test]
fn windows_query_rejects_absent_old_or_truncated_language_metadata() {
    let mut query = query();
    query.explicit_language = Some("ja".to_owned());
    assert!(!query_is_satisfied_by(&query, |_| true, || None));
    for table in [Vec::new(), vec![0; 78], os2(1 << 17)[..81].to_vec(), {
        let mut old = os2(1 << 17);
        old[..2].fill(0);
        old
    }] {
        assert!(!query_is_satisfied_by(&query, |_| true, || Some(table)));
    }
}

#[test]
fn windows_query_registry_language_hints_do_not_become_explicit_constraints() {
    for hints in [Vec::new(), vec!["ru".to_owned()], vec!["zh-cn".to_owned()]] {
        let mut query = query();
        query.languages = hints;
        assert!(query_is_satisfied_by(
            &query,
            |_| panic!("no coverage constraint"),
            || panic!("no explicit language")
        ));
    }
}

#[test]
fn windows_query_batches_deduplicated_required_text_and_overlapping_ranges() {
    let mut query = query();
    query.required = RequiredFontCoverage::for_text("AAЖλ").unwrap();
    query.charset_ranges = vec![(0x0400, 0x07e7), (0x0600, 0x0580), (0x0400, 0x0400)];
    let mut batches = Vec::new();
    assert!(query_is_satisfied_by(
        &query,
        |codepoints| {
            batches.push(codepoints.to_vec());
            true
        },
        || panic!("no explicit language")
    ));
    assert_eq!(batches.len(), 1, "all coverage uses one native query");
    let expected: Vec<u32> = [0x0041, 0x03bb]
        .into_iter()
        .chain(0x0400..=0x07e7)
        .collect();
    assert_eq!(
        batches[0], expected,
        "overlap and required duplicates disappear"
    );
}

#[test]
fn windows_query_batch_validation_precedes_provider_and_language() {
    let mut query = query();
    query.charset_ranges = vec![(0xd7ff, 0xd800)];
    query.explicit_language = Some("ja".to_owned());
    assert!(!query_is_satisfied_by(
        &query,
        |_| panic!("invalid query reached native provider"),
        || panic!("coverage must be checked first")
    ));
}

#[test]
fn windows_query_failed_batch_rejects_before_language_lookup() {
    let mut query = query();
    query.required = RequiredFontCoverage::Character('A');
    query.explicit_language = Some("ja".to_owned());
    assert!(!query_is_satisfied_by(
        &query,
        |_| false,
        || panic!("failed coverage reached OS/2")
    ));
}

#[test]
fn windows_fallback_rejects_missing_or_foreign_explicit_language_codepages() {
    for language in ["ja", "ko", "zh"] {
        assert!(!explicit_language_is_satisfied_by(Some(language), || Some(
            os2(1)
        )));
        assert!(!explicit_language_is_satisfied_by(Some(language), || None));
    }
    for language in ["JA", "ja-JP", "ru"] {
        assert!(!explicit_language_is_satisfied_by(
            Some(language),
            || panic!("unsupported tag needs no native metadata")
        ));
    }
}

#[test]
fn windows_fallback_language_admission_preserves_partial_text_coverage() {
    let mut query = query();
    query.required = RequiredFontCoverage::for_text("AЖ").unwrap();
    query.explicit_language = Some("ja".to_owned());
    // MapCharacters may return a font for just the initial Latin prefix.
    assert!(!query_is_satisfied_by(
        &query,
        |codepoints| codepoints.iter().all(|cp| *cp == u32::from('A')),
        || Some(os2(1 << 17))
    ));
    assert!(explicit_language_is_satisfied_by(
        query.explicit_language.as_deref(),
        || Some(os2(1 << 17))
    ));
}

#[test]
fn windows_fallback_without_explicit_language_needs_no_native_metadata() {
    assert!(explicit_language_is_satisfied_by(None, || panic!(
        "unconstrained fallback needs no OS/2"
    )));
}
