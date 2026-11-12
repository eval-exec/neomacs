use super::*;
use crate::font_backend::{FontCandidateScope, RequiredFontCoverage, TextDirection};
use neovm_core::face::{FontSlant, FontWidth};

fn query() -> FontCandidateQuery {
    FontCandidateQuery {
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
    assert!(!query_is_satisfied_by(&query, |cp| cp == 0x00a0, || None));
    assert!(query_is_satisfied_by(
        &query,
        |cp| matches!(cp, 0x00a0 | 0x010e),
        || None
    ));
    query.required = RequiredFontCoverage::Character('λ');
    assert!(!query_is_satisfied_by(
        &query,
        |cp| matches!(cp, 0x00a0 | 0x010e),
        || None
    ));
    assert!(query_is_satisfied_by(
        &query,
        |cp| matches!(cp, 0x00a0 | 0x010e | 0x03bb),
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
