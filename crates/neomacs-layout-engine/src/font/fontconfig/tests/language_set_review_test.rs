use super::super::FcQueryKind;
use super::*;

fn reference_language_pattern(
    family: Option<&str>,
    languages: &[String],
) -> (FcPatternGuard, Option<FcLangSetGuard>) {
    // The reference is GNU ftfont_spec_pattern: one pattern and one language
    // set populated by every requested language, not one query per language.
    // SAFETY: the new native pattern is immediately owned by its guard.
    let pattern = FcPatternGuard(unsafe { fontconfig_sys::FcPatternCreate() });
    assert!(!pattern.0.is_null());
    if let Some(family) = family {
        let family = CString::new(family).unwrap();
        // SAFETY: the guard and C string are live; the pattern copies the name.
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
    }
    let langset = if languages.is_empty() {
        None
    } else {
        // SAFETY: the new native set is immediately owned by its guard.
        let langset = FcLangSetGuard(unsafe { fontconfig_sys::FcLangSetCreate() });
        assert!(!langset.0.is_null());
        for language in languages {
            let language = CString::new(language.as_str()).unwrap();
            // SAFETY: the set copies the live C string into its owned data.
            assert_ne!(
                unsafe { fontconfig_sys::FcLangSetAdd(langset.0, language.as_ptr().cast()) },
                0
            );
        }
        // SAFETY: both guarded objects remain live through the reference query.
        assert_ne!(
            unsafe {
                fontconfig_sys::FcPatternAddLangSet(
                    pattern.0,
                    fontconfig::FC_LANG.as_ptr(),
                    langset.0,
                )
            },
            0
        );
        Some(langset)
    };
    (pattern, langset)
}

#[test]
fn multiple_languages_list_one_native_set_without_duplicate_entities() {
    let config = native_fixture();
    let languages = vec!["en".to_owned(), "ru".to_owned()];
    let (pattern, _langset) = reference_language_pattern(Some("DejaVu Sans Mono"), &languages);
    let expected = native_reference_candidates(&config, &pattern, ReferenceSelection::AllMetadata);
    assert!(
        !expected.is_empty(),
        "pinned multilingual family must be present"
    );
    let actual = config.list_candidates(Some("DejaVu Sans Mono"), &[], None, &languages);
    assert_eq!(
        actual, expected,
        "a shared native language set preserves native uniquification and order"
    );
}

#[test]
fn multiple_languages_require_the_same_native_language_set() {
    let config = native_fixture();
    let languages = vec!["en".to_owned(), "ja".to_owned()];
    assert!(
        config
            .list_candidates(Some("DejaVu Sans Mono"), &[], None, &["ja".to_owned()])
            .is_empty(),
        "fixture lacks Japanese language coverage"
    );
    let (pattern, _langset) = reference_language_pattern(Some("DejaVu Sans Mono"), &languages);
    let expected = native_reference_candidates(&config, &pattern, ReferenceSelection::AllMetadata);
    assert!(
        expected.is_empty(),
        "a font without Japanese coverage does not satisfy the combined native language set"
    );
    assert_eq!(
        config.list_candidates(Some("DejaVu Sans Mono"), &[], None, &languages),
        expected
    );
}

#[test]
fn empty_language_query_preserves_unconstrained_native_listing() {
    let config = native_fixture();
    let (pattern, _langset) = reference_language_pattern(None, &[]);
    let expected = native_reference_candidates(&config, &pattern, ReferenceSelection::AllMetadata);
    assert!(
        expected
            .iter()
            .any(|font| font.matched.family == "Noto Color Emoji"),
        "fixture contains non-Latin coverage"
    );
    assert_eq!(
        config.list_candidates(None, &[], None, &[]),
        expected,
        "no explicit query language means no explicit FC_LANG restriction"
    );
}

#[test]
fn multiple_languages_match_one_native_pattern_and_preserve_empty_query_defaults() {
    let config = native_fixture();
    for languages in [Vec::new(), vec!["en".to_owned(), "ru".to_owned()]] {
        let (pattern, _langset) = reference_language_pattern(Some("DejaVu Sans Mono"), &languages);
        let expected = config.with_reference_config(|owner| {
            owner
                .apply_defaults(&pattern)
                .expect("isolated native defaults");
            let native = owner.native_ptr().expect("live isolated configuration");
            // SAFETY: the owner locks the live configuration and the pattern is owned.
            assert_ne!(
                unsafe {
                    fontconfig_sys::FcConfigSubstitute(
                        native,
                        pattern.0,
                        fontconfig_sys::FcMatchPattern,
                    )
                },
                0
            );
            // SAFETY: this exclusive guarded pattern remains live.
            unsafe { fontconfig_sys::FcDefaultSubstitute(pattern.0) };
            let mut result = fontconfig_sys::FcResultNoMatch;
            // SAFETY: owner and pattern are live; the returned pattern is guarded.
            let matched = FcPatternGuard(unsafe {
                fontconfig_sys::FcFontMatch(native, pattern.0, &mut result)
            });
            listed_font_from_raw_pattern(matched.0).expect("pinned family matches")
        });
        let actual = config.query_uncached(
            Some("DejaVu Sans Mono"),
            &[],
            None,
            &languages,
            FcQueryKind::Match,
        );
        assert_eq!(
            actual.len(),
            1,
            "one GNU driver match returns one native entity"
        );
        assert_eq!(actual, vec![expected]);
    }
}
