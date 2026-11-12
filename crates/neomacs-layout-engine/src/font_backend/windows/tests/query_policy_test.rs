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

#[test]
fn windows_generic_family_query_reuses_native_resources_and_all_styles_per_family() {
    let initialized = std::cell::Cell::new(0);
    let enumerated = std::cell::Cell::new(0);
    let mut query = GenericFamilyQuery::new();
    for (family, weight, italic, expected) in [
        (1, 400, 0, Some("mono")),
        (1, 700, 0, Some("sans")),
        (1, 400, 1, None),
        (2, 400, 0, Some("serif")),
        (1, 400, 0, Some("mono")),
    ] {
        let mut name = [0; 32];
        name[0] = family;
        let actual = query.classify(
            || {
                initialized.set(initialized.get() + 1);
                Some(())
            },
            |_| {
                Some(GenericFamilyRequest {
                    family: name,
                    weight,
                    italic,
                })
            },
            |_, name| {
                enumerated.set(enumerated.get() + 1);
                Some(if name[0] == 1 {
                    vec![
                        GenericFamilyRecord {
                            weight: 400,
                            italic: 0,
                            pitch_and_family: 0x31,
                        },
                        // A second charset repeats the same physical class.
                        GenericFamilyRecord {
                            weight: 400,
                            italic: 0,
                            pitch_and_family: 0x32,
                        },
                        GenericFamilyRecord {
                            weight: 700,
                            italic: 0,
                            pitch_and_family: 0x20,
                        },
                    ]
                } else {
                    vec![GenericFamilyRecord {
                        weight: 400,
                        italic: 0,
                        pitch_and_family: 0x10,
                    }]
                })
            },
        );
        assert_eq!(
            actual, expected,
            "family {family}, weight {weight}, italic {italic}"
        );
    }
    assert_eq!(initialized.get(), 1, "one native resource set per query");
    assert_eq!(enumerated.get(), 2, "one complete enumeration per family");
}

#[test]
fn windows_generic_family_query_preserves_ambiguity_and_retries_only_in_a_new_scope() {
    struct Resources<'a>(&'a std::cell::Cell<usize>);
    impl Drop for Resources<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let initialized = std::cell::Cell::new(0);
    let enumerated = std::cell::Cell::new(0);
    let dropped = std::cell::Cell::new(0);
    {
        let mut query = GenericFamilyQuery::new();
        for (family, weight, expected) in [
            (1, 400, None),
            (1, 400, None),
            (1, 700, Some("serif")),
            (2, 400, None),
            (2, 400, None),
            (3, 400, None),
            (3, 400, None),
        ] {
            let mut name = [0; 32];
            name[0] = family;
            assert_eq!(
                query.classify(
                    || {
                        initialized.set(initialized.get() + 1);
                        Some(Resources(&dropped))
                    },
                    |_| Some(GenericFamilyRequest {
                        family: name,
                        weight,
                        italic: 0
                    }),
                    |_, name| {
                        enumerated.set(enumerated.get() + 1);
                        match name[0] {
                            1 => Some(vec![
                                GenericFamilyRecord {
                                    weight: 400,
                                    italic: 0,
                                    pitch_and_family: 0x30,
                                },
                                GenericFamilyRecord {
                                    weight: 400,
                                    italic: 0,
                                    pitch_and_family: 0x20,
                                },
                                GenericFamilyRecord {
                                    weight: 700,
                                    italic: 0,
                                    pitch_and_family: 0x10,
                                },
                            ]),
                            // A failed or incomplete native enumeration is unknown.
                            2 => None,
                            _ => Some(Vec::new()),
                        }
                    },
                ),
                expected
            );
        }
        assert_eq!(initialized.get(), 1);
        assert_eq!(enumerated.get(), 3);
        assert_eq!(
            dropped.get(),
            0,
            "resources stay live through the native query"
        );
    }
    assert_eq!(dropped.get(), 1);
    let mut next_query = GenericFamilyQuery::new();
    let mut same_family = [0; 32];
    same_family[0] = 2;
    assert_eq!(
        next_query.classify(
            || {
                initialized.set(initialized.get() + 1);
                Some(Resources(&dropped))
            },
            |_| Some(GenericFamilyRequest {
                family: same_family,
                weight: 400,
                italic: 0
            }),
            |_, _| {
                enumerated.set(enumerated.get() + 1);
                Some(vec![GenericFamilyRecord {
                    weight: 400,
                    italic: 0,
                    pitch_and_family: 0x30,
                }])
            },
        ),
        Some("mono")
    );
    assert_eq!(initialized.get(), 2);
    assert_eq!(enumerated.get(), 4);
    drop(next_query);
    assert_eq!(dropped.get(), 2);
}

#[test]
fn windows_generic_family_query_initialization_failure_is_lazy_and_not_retried() {
    let initialized = std::cell::Cell::new(0);
    let mut query = GenericFamilyQuery::<()>::new();
    assert_eq!(initialized.get(), 0);
    for _ in 0..3 {
        assert_eq!(
            query.classify(
                || {
                    initialized.set(initialized.get() + 1);
                    None
                },
                |_| panic!("failed resources cannot describe a font"),
                |_, _| panic!("failed resources cannot enumerate a family"),
            ),
            None
        );
    }
    assert_eq!(initialized.get(), 1);
}

fn query() -> FontCandidateQuery {
    FontCandidateQuery {
        signature_requirement: None,
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

fn signature_table(version: u16, unicode: [u32; 4], codepages: u32) -> Vec<u8> {
    let mut bytes = vec![0; if version == 0 { 78 } else { 86 }];
    bytes[..2].copy_from_slice(&version.to_be_bytes());
    for (word, value) in unicode.into_iter().enumerate() {
        bytes[42 + word * 4..46 + word * 4].copy_from_slice(&value.to_be_bytes());
    }
    if version != 0 {
        bytes[78..82].copy_from_slice(&codepages.to_be_bytes());
    }
    bytes
}

#[test]
fn windows_signature_sip_requires_surrogate_and_cjk_signature_bits() {
    use crate::font_backend::FontSignatureRequirement;
    let mut query = query();
    query.signature_requirement = Some(FontSignatureRequirement::Sip);
    for (version, usb1, csb, expected) in [
        (1, 0, 0, false),
        (1, 0x02000000, 0, false),
        (1, 0x28000000, 0, false),
        (1, 0, 0x003e0000, false),
        (0, 0x0a000000, 0, true),
        (0, 0x22000000, 0, true),
        (0, 0x02000000, 0x003e0000, false),
        (1, 0x02000000, 1 << 17, true),
        (1, 0x02000000, 1 << 18, true),
        (1, 0x02000000, 1 << 19, true),
        (1, 0x02000000, 1 << 20, true),
        (1, 0x02000000, 1 << 21, true),
        (1, 0x02000000, 1 << 16, false),
    ] {
        assert_eq!(
            query_is_satisfied_by(
                &query,
                |_| panic!("no invented representative glyph"),
                || Some(signature_table(version, [0, usb1, 0, 0], csb))
            ),
            expected,
            "v{version} USB1={usb1:x} CSB={csb:x}"
        );
    }
}

#[test]
fn windows_signature_unicode_and_bmp_follow_each_native_range_bit() {
    use crate::font_backend::FontSignatureRequirement;
    for (requirement, masks) in [
        (
            FontSignatureRequirement::Unicode,
            [0x3fffffffu32, u32::MAX, u32::MAX, u32::MAX],
        ),
        (
            FontSignatureRequirement::Bmp,
            [0x007f001f, 0xe81fffff, 0xffffff9e, u32::MAX],
        ),
    ] {
        let mut query = query();
        query.signature_requirement = Some(requirement);
        for word in 0..4 {
            for bit in 0..32 {
                let mut ranges = [0; 4];
                ranges[word] = 1 << bit;
                assert_eq!(
                    query_is_satisfied_by(
                        &query,
                        |_| panic!("signature is not glyph coverage"),
                        || Some(signature_table(0, ranges, 0))
                    ),
                    masks[word] & (1 << bit) != 0,
                    "{requirement:?} word={word} bit={bit}"
                );
            }
        }
    }
}

#[test]
fn windows_signature_rejects_missing_and_truncated_metadata() {
    use crate::font_backend::FontSignatureRequirement;
    for requirement in [
        FontSignatureRequirement::Unicode,
        FontSignatureRequirement::Bmp,
        FontSignatureRequirement::Sip,
    ] {
        let mut query = query();
        query.signature_requirement = Some(requirement);
        assert!(!query_is_satisfied_by(&query, |_| true, || None));
        for table in [
            Vec::new(),
            vec![0; 42],
            signature_table(0, [1, 0x0a000000, 0, 0], 0)[..57].to_vec(),
        ] {
            assert!(
                !query_is_satisfied_by(&query, |_| true, || Some(table.clone())),
                "{requirement:?}"
            );
        }
    }
}

#[test]
fn windows_signature_composes_with_glyph_and_language_admission() {
    use crate::font_backend::FontSignatureRequirement;
    let mut query = query();
    query.signature_requirement = Some(FontSignatureRequirement::Sip);
    query.explicit_language = Some("ja".to_owned());
    query.required = RequiredFontCoverage::Character('好');
    let calls = std::cell::Cell::new(0);
    assert!(query_is_satisfied_by(
        &query,
        |cps| cps == [u32::from('好')],
        || {
            calls.set(calls.get() + 1);
            Some(signature_table(1, [0, 0x02000000, 0, 0], 1 << 17))
        }
    ));
    assert_eq!(calls.get(), 1);
    assert!(!query_is_satisfied_by(
        &query,
        |_| false,
        || panic!("failed glyph coverage must stop before table probing")
    ));
    assert!(
        !query_is_satisfied_by(
            &query,
            |_| true,
            || Some(signature_table(1, [0, 0x02000000, 0, 0], 1 << 18))
        ),
        "SIP may pass through Chinese coverage while explicit Japanese still fails"
    );
}

#[test]
fn windows_signature_native_fallback_preserves_partial_text_coverage() {
    use crate::font_backend::FontSignatureRequirement;
    let mut query = query();
    query.required = RequiredFontCoverage::for_text("好a").unwrap();
    query.charset_ranges = vec![(0x0400, 0x04ff)];
    query.signature_requirement = Some(FontSignatureRequirement::Sip);
    query.explicit_language = Some("ja".to_owned());
    assert!(
        signature_and_language_are_satisfied_by(&query, || Some(signature_table(
            1,
            [0, 0x02000000, 0, 0],
            1 << 17
        ))),
        "mapped fallback admits its signature/language without requiring the complete requested text"
    );
    assert!(
        !signature_and_language_are_satisfied_by(&query, || Some(signature_table(
            1,
            [1, 0, 0, 0],
            1 << 17
        ))),
        "matching language alone cannot erase a requested SIP signature"
    );
    query.signature_requirement = None;
    query.explicit_language = None;
    assert!(signature_and_language_are_satisfied_by(&query, || panic!(
        "unconstrained native fallback must not probe OS/2"
    )));
}
