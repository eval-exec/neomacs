use super::*;
use neovm_text::emacs_core::charset::{builtin_define_charset_internal, reset_charset_registry};
use neovm_value::intern::intern;

fn registry_spec(name: &str) -> FontSpecEntry {
    FontSpecEntry::Font(StoredFontSpec {
        family: None,
        registry: Some(intern(name)),
        lang: None,
        weight: None,
        slant: None,
        width: None,
        otf: None,
        definition: None,
    })
}

#[test]
fn family_only_cjk_rule_is_a_rendering_candidate_without_a_registry() {
    crate::test_utils::init_test_tracing();
    reset_fontset_registry();
    reset_charset_registry();
    set_fontset_font(
        &Value::T,
        &Value::cons(Value::fixnum(0x2E80), Value::fixnum(0x9FFF)),
        &Value::string("LXGWWenKai Nerd Font"),
        None,
        None,
        None,
        Some(&Value::NIL),
    )
    .expect("install the family-only fontset rule");
    let entries = matching_entries_for_fontset(DEFAULT_FONTSET_NAME, '中');
    assert!(
        entries
            .iter()
            .any(|entry| matches!(entry, FontSpecEntry::Font(spec)
            if spec.family == Some(intern("LXGWWenKai Nerd Font")))),
        "a missing registry must not discard the configured CJK rendering candidate"
    );
}

#[test]
fn unmatched_family_font_spec_defaults_to_ascii_repertory_like_gnu() {
    crate::test_utils::init_test_tracing();
    reset_charset_registry();
    let empty_encoding_alist = Value::NIL;

    let entry = parse_font_spec_entry(
        &Value::string("JetBrainsMono Nerd Font"),
        Some(&empty_encoding_alist),
    )
    .expect("font spec");
    let FontSpecEntry::Font(spec) = entry else {
        panic!("family name must produce a font spec");
    };

    assert_eq!(
        spec.definition,
        Some(FontDefinitionMetadata {
            encoding: intern("ascii"),
            repertory: Some(FontRepertory::Charset(intern("ascii"))),
        }),
        "GNU find_font_encoding falls back to Qascii when no alist pattern matches"
    );
    assert!(spec.matches_char('A' as u32));
    assert!(
        !spec.matches_char(0xE6AD),
        "ASCII query metadata does not describe private-use glyph coverage"
    );
}

#[test]
fn font_encoding_entry_distinguishes_nil_and_explicit_repertory() {
    crate::test_utils::init_test_tracing();
    reset_charset_registry();
    let spec = StoredFontSpec {
        family: Some(intern("Fixture")),
        registry: None,
        lang: None,
        weight: None,
        slant: None,
        width: None,
        otf: None,
        definition: None,
    };

    let nil_repertory_entry = Value::list(vec![
        Value::string("Fixture-$"),
        Value::symbol("unicode-bmp"),
    ]);
    let nil_repertory_alist = Value::list(vec![nil_repertory_entry]);
    assert_eq!(
        resolve_font_definition(&spec, Some(&nil_repertory_alist)),
        FontDefinitionMetadata {
            encoding: intern("unicode-bmp"),
            repertory: None,
        },
        "(ENCODING) has GNU's explicit nil repertory"
    );

    let explicit_repertory_entry = Value::cons(
        Value::string("Fixture-$"),
        Value::cons(Value::symbol("unicode"), Value::symbol("unicode-bmp")),
    );
    let explicit_repertory_alist = Value::list(vec![explicit_repertory_entry]);
    assert_eq!(
        resolve_font_definition(&spec, Some(&explicit_repertory_alist)),
        FontDefinitionMetadata {
            encoding: intern("unicode"),
            repertory: Some(FontRepertory::Charset(intern("unicode-bmp"))),
        }
    );
}

#[test]
fn fontset_add_mode_accepts_gnu_append_prepend_symbols_and_keywords() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        FontsetAddMode::from_lisp_value(Some(&Value::symbol("append"))),
        FontsetAddMode::Append
    );
    assert_eq!(
        FontsetAddMode::from_lisp_value(Some(&Value::symbol(":append"))),
        FontsetAddMode::Append
    );
    assert_eq!(
        FontsetAddMode::from_lisp_value(Some(&Value::symbol("prepend"))),
        FontsetAddMode::Prepend
    );
    assert_eq!(
        FontsetAddMode::from_lisp_value(Some(&Value::symbol(":prepend"))),
        FontsetAddMode::Prepend
    );
}

#[test]
fn fontset_add_mode_treats_other_values_as_overwrite() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        FontsetAddMode::from_lisp_value(None),
        FontsetAddMode::Overwrite
    );
    assert_eq!(
        FontsetAddMode::from_lisp_value(Some(&Value::NIL)),
        FontsetAddMode::Overwrite
    );
    assert_eq!(
        FontsetAddMode::from_lisp_value(Some(&Value::T)),
        FontsetAddMode::Overwrite
    );
    assert_eq!(
        FontsetAddMode::from_lisp_value(Some(&Value::string("append"))),
        FontsetAddMode::Overwrite
    );
}

#[test]
fn overlapping_ranges_follow_char_table_semantics() {
    crate::test_utils::init_test_tracing();
    let mut data = FontsetData::default();
    data.update_target(
        FontsetTarget::Range(0x80, 0x10FFFF),
        registry_spec("iso8859-1"),
        FontsetAddMode::Overwrite,
    );
    data.update_target(
        FontsetTarget::Range(0x4E00, 0x9FFF),
        registry_spec("gb2312.1980-0"),
        FontsetAddMode::Overwrite,
    );

    assert_eq!(
        data.specific_entries_for_char('好' as u32),
        vec![registry_spec("gb2312.1980-0")]
    );
}

#[test]
fn partial_overlap_append_splits_ranges() {
    crate::test_utils::init_test_tracing();
    let mut data = FontsetData::default();
    data.update_target(
        FontsetTarget::Range(0x1000, 0x1005),
        registry_spec("base"),
        FontsetAddMode::Overwrite,
    );
    data.update_target(
        FontsetTarget::Range(0x1002, 0x1003),
        registry_spec("extra"),
        FontsetAddMode::Append,
    );

    assert_eq!(
        data.specific_entries_for_char(0x1001),
        vec![registry_spec("base")]
    );
    assert_eq!(
        data.specific_entries_for_char(0x1002),
        vec![registry_spec("base"), registry_spec("extra")]
    );
    assert_eq!(
        data.specific_entries_for_char(0x1004),
        vec![registry_spec("base")]
    );
}

#[test]
fn update_targets_batch_matches_single_target_updates() {
    crate::test_utils::init_test_tracing();
    let mut data1 = FontsetData::default();
    let mut data2 = FontsetData::default();

    // Initial base ranges
    let base_targets = vec![
        FontsetTarget::Range(0x1000, 0x1050),
        FontsetTarget::Range(0x2000, 0x2050),
        FontsetTarget::Range(0x3000, 0x3050),
    ];
    for t in &base_targets {
        data1.update_target(t.clone(), registry_spec("base"), FontsetAddMode::Overwrite);
    }
    data2.update_targets(
        base_targets.clone(),
        registry_spec("base"),
        FontsetAddMode::Overwrite,
    );

    // Apply multiple overlapping and new ranges with Append
    let update_targets = vec![
        FontsetTarget::Range(0x1020, 0x1040), // partial overlap
        FontsetTarget::Range(0x1045, 0x1060), // partial overlap extending past end
        FontsetTarget::Range(0x2000, 0x2050), // exact overlap
        FontsetTarget::Range(0x2500, 0x2550), // disjoint new range
        FontsetTarget::Range(0x3010, 0x3020), // sub-range
    ];

    for t in &update_targets {
        data1.update_target(t.clone(), registry_spec("extra"), FontsetAddMode::Append);
    }
    data2.update_targets(
        update_targets,
        registry_spec("extra"),
        FontsetAddMode::Append,
    );

    // Verify both produce identical entries across sampled points
    let probe_points = [
        0x1000, 0x1019, 0x1020, 0x1030, 0x1040, 0x1041, 0x1045, 0x1050, 0x1055, 0x1060, 0x1061,
        0x2000, 0x2025, 0x2050, 0x2051, 0x2500, 0x2525, 0x2550, 0x2551, 0x3000, 0x3009, 0x3010,
        0x3015, 0x3020, 0x3021, 0x3050,
    ];

    for &cp in &probe_points {
        assert_eq!(
            data1.specific_entries_for_char(cp),
            data2.specific_entries_for_char(cp),
            "Mismatch at codepoint {cp:#x}"
        );
    }
}

#[test]
fn update_targets_batch_large_fragmented_ranges() {
    crate::test_utils::init_test_tracing();
    let mut data = FontsetData::default();
    // Simulate 4,000 non-contiguous ranges (like japanese-jisx0208)
    let targets: Vec<FontsetTarget> = (0..4000)
        .map(|i| FontsetTarget::Range(i * 10, i * 10 + 5))
        .collect();

    data.update_targets(targets, registry_spec("cjk"), FontsetAddMode::Overwrite);

    assert_eq!(
        data.specific_entries_for_char(25 * 10 + 2),
        vec![registry_spec("cjk")]
    );
    assert_eq!(data.specific_entries_for_char(25 * 10 + 7), Vec::new());
}

/// Both fontsets give every code point in `0..=last` the same entries, and
/// they share one fallback chain.
fn assert_same_fontset_lookup(sequential: &FontsetData, batched: &FontsetData, last: u32) {
    for code in 0..=last {
        assert_eq!(
            sequential.specific_entries_for_char(code),
            batched.specific_entries_for_char(code),
            "specific entries differ at {code:#x}"
        );
    }
    assert_eq!(
        sequential
            .fallback
            .as_ref()
            .map(|entries| entries.to_original_entries()),
        batched
            .fallback
            .as_ref()
            .map(|entries| entries.to_original_entries()),
        "fallback entries differ"
    );
}

fn apply_one_target_at_a_time(
    data: &mut FontsetData,
    targets: &[FontsetTarget],
    entry: &FontSpecEntry,
    add: FontsetAddMode,
) {
    for target in targets {
        data.update_target(target.clone(), entry.clone(), add);
    }
}

#[test]
fn update_targets_batch_merges_unsorted_overlapping_and_adjacent_overwrites() {
    crate::test_utils::init_test_tracing();
    let mut sequential = FontsetData::default();
    sequential.update_target(
        FontsetTarget::Range(0x20, 0x60),
        registry_spec("base"),
        FontsetAddMode::Overwrite,
    );
    let mut batched = sequential.clone();
    // Unsorted; 0x40..=0x4F touches 0x50..=0x58 and 0x45..=0x55 overlaps both.
    let targets = vec![
        FontsetTarget::Range(0x50, 0x58),
        FontsetTarget::Range(0x10, 0x12),
        FontsetTarget::Range(0x40, 0x4F),
        FontsetTarget::Range(0x45, 0x55),
    ];
    apply_one_target_at_a_time(
        &mut sequential,
        &targets,
        &registry_spec("new"),
        FontsetAddMode::Overwrite,
    );
    batched.update_targets(targets, registry_spec("new"), FontsetAddMode::Overwrite);

    let new = vec![registry_spec("new")];
    let base = vec![registry_spec("base")];
    for (code, expected) in [
        (0x0F, Vec::new()),
        (0x10, new.clone()),
        (0x12, new.clone()),
        (0x13, Vec::new()),
        (0x20, base.clone()),
        (0x3F, base.clone()),
        (0x40, new.clone()),
        (0x4F, new.clone()),
        (0x50, new.clone()),
        (0x58, new.clone()),
        (0x59, base.clone()),
        (0x60, base),
        (0x61, Vec::new()),
    ] {
        assert_eq!(
            batched.specific_entries_for_char(code),
            expected,
            "{code:#x}"
        );
    }
    assert_same_fontset_lookup(&sequential, &batched, 0x80);
}

/// GNU `set-fontset-font` calls `fontset_add` once per target range, so a
/// character two targets cover gets an `append`ed or `prepend`ed font twice.
#[test]
fn update_targets_batch_repeats_append_and_prepend_for_overlapping_targets() {
    crate::test_utils::init_test_tracing();
    let base = registry_spec("base");
    let extra = registry_spec("extra");
    for (add, twice) in [
        (
            FontsetAddMode::Append,
            vec![base.clone(), extra.clone(), extra.clone()],
        ),
        (
            FontsetAddMode::Prepend,
            vec![extra.clone(), extra.clone(), base.clone()],
        ),
    ] {
        let once = match add {
            FontsetAddMode::Append => vec![base.clone(), extra.clone()],
            FontsetAddMode::Prepend => vec![extra.clone(), base.clone()],
            FontsetAddMode::Overwrite => unreachable!("only the accumulating modes"),
        };
        let mut sequential = FontsetData::default();
        sequential.update_target(
            FontsetTarget::Range(0x40, 0x60),
            base.clone(),
            FontsetAddMode::Overwrite,
        );
        let mut batched = sequential.clone();
        let targets = vec![
            FontsetTarget::Range(0x4C, 0x58),
            FontsetTarget::Range(0x48, 0x50),
        ];
        apply_one_target_at_a_time(&mut sequential, &targets, &extra, add);
        batched.update_targets(targets, extra.clone(), add);

        for (code, expected) in [
            (0x47, vec![base.clone()]),
            (0x48, once.clone()),
            (0x4B, once.clone()),
            (0x4C, twice.clone()),
            (0x50, twice.clone()),
            (0x51, once.clone()),
            (0x58, once.clone()),
            (0x59, vec![base.clone()]),
        ] {
            assert_eq!(
                batched.specific_entries_for_char(code),
                expected,
                "{add:?} at {code:#x}"
            );
        }
        assert_same_fontset_lookup(&sequential, &batched, 0x80);
    }
}

/// A fixed linear congruential sequence keeps every generated case reproducible.
struct CaseSequence(u64);

impl CaseSequence {
    fn below(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % bound
    }

    fn pick<T: Clone>(&mut self, items: &[T]) -> T {
        items[self.below(items.len() as u64) as usize].clone()
    }

    /// `count` targets over `0..=last`, mostly short ranges that often
    /// overlap or touch, now and then the fallback.
    fn targets(&mut self, count: u64, last: u32) -> Vec<FontsetTarget> {
        (0..count)
            .map(|_| {
                if self.below(10) == 0 {
                    FontsetTarget::Fallback
                } else {
                    let from = self.below(u64::from(last) + 1) as u32;
                    let to = (from + self.below(24) as u32).min(last);
                    FontsetTarget::Range(from, to)
                }
            })
            .collect()
    }
}

/// The batch is only a faster spelling of one `update_target` per target,
/// whatever the targets' order, overlap, adjacency, or mode.
#[test]
fn update_targets_batch_matches_one_target_at_a_time_for_generated_targets() {
    crate::test_utils::init_test_tracing();
    let modes = [
        FontsetAddMode::Overwrite,
        FontsetAddMode::Append,
        FontsetAddMode::Prepend,
    ];
    let entries = [
        registry_spec("a"),
        registry_spec("b"),
        registry_spec("c"),
        FontSpecEntry::ExplicitNone,
    ];
    let last = 0x7F;
    let mut sequence = CaseSequence(0x808);
    for case in 0..300 {
        let mut sequential = FontsetData::default();
        let mut batched = FontsetData::default();
        for _ in 0..sequence.below(5) {
            let count = 1 + sequence.below(4);
            let targets = sequence.targets(count, last);
            let entry = sequence.pick(&entries);
            let add = sequence.pick(&modes);
            apply_one_target_at_a_time(&mut sequential, &targets, &entry, add);
            apply_one_target_at_a_time(&mut batched, &targets, &entry, add);
        }
        let count = 1 + sequence.below(8);
        let targets = sequence.targets(count, last);
        let entry = sequence.pick(&entries);
        let add = sequence.pick(&modes);
        apply_one_target_at_a_time(&mut sequential, &targets, &entry, add);
        batched.update_targets(targets.clone(), entry, add);
        for code in 0..=last {
            assert_eq!(
                sequential.specific_entries_for_char(code),
                batched.specific_entries_for_char(code),
                "case {case}: {add:?} {targets:?} differs at {code:#x}"
            );
        }
        assert_same_fontset_lookup(&sequential, &batched, last);
    }
}

#[test]
fn fallback_entries_append_after_specific_entries() {
    crate::test_utils::init_test_tracing();
    let mut data = FontsetData::default();
    data.update_target(
        FontsetTarget::Range(0x4E00, 0x9FFF),
        registry_spec("gb2312.1980-0"),
        FontsetAddMode::Overwrite,
    );
    data.update_target(
        FontsetTarget::Fallback,
        registry_spec("iso10646-1"),
        FontsetAddMode::Append,
    );

    assert_eq!(
        data.entries_for_char('好' as u32, FontsetLookupPurpose::Patterns),
        vec![registry_spec("gb2312.1980-0"), registry_spec("iso10646-1")]
    );
}

#[test]
fn encoding_charset_filters_informational_patterns() {
    crate::test_utils::init_test_tracing();
    let mut data = FontsetData::default();
    data.update_target(
        FontsetTarget::Range(0x80, 0x10FFFF),
        FontSpecEntry::Font(StoredFontSpec {
            family: None,
            registry: Some(intern("iso8859-1")),
            lang: None,
            weight: None,
            slant: None,
            width: None,
            otf: None,
            definition: Some(FontDefinitionMetadata {
                encoding: intern("iso-8859-1"),
                repertory: Some(FontRepertory::Charset(intern("iso-8859-1"))),
            }),
        }),
        FontsetAddMode::Append,
    );
    data.update_target(
        FontsetTarget::Range(0x80, 0x10FFFF),
        FontSpecEntry::Font(StoredFontSpec {
            family: None,
            registry: Some(intern("iso10646-1")),
            lang: None,
            weight: None,
            slant: None,
            width: None,
            otf: None,
            definition: Some(FontDefinitionMetadata {
                encoding: intern("unicode-bmp"),
                repertory: Some(FontRepertory::Charset(intern("unicode-bmp"))),
            }),
        }),
        FontsetAddMode::Append,
    );

    let registries: Vec<_> = data
        .entries_for_char('好' as u32, FontsetLookupPurpose::Patterns)
        .into_iter()
        .filter_map(|entry| match entry {
            FontSpecEntry::Font(spec) => spec.registry.map(|sym| resolve_sym(sym).to_string()),
            FontSpecEntry::ExplicitNone => None,
        })
        .collect();

    assert_eq!(registries, vec!["iso10646-1".to_string()]);
}

#[test]
fn encoding_subset_charset_filters_informational_patterns() {
    crate::test_utils::init_test_tracing();
    reset_charset_registry();

    let mut parent_args = vec![Value::NIL; 17];
    parent_args[0] = Value::symbol("latin-iso8859-2-test");
    parent_args[1] = Value::fixnum(1);
    parent_args[2] = Value::vector(vec![Value::fixnum(0), Value::fixnum(255)]);
    parent_args[8] = Value::T;
    parent_args[12] = Value::string("8859-2");
    builtin_define_charset_internal(parent_args).unwrap();

    let mut subset_args = vec![Value::NIL; 17];
    subset_args[0] = Value::symbol("iso-8859-2-test");
    subset_args[1] = Value::fixnum(1);
    subset_args[2] = Value::vector(vec![Value::fixnum(32), Value::fixnum(127)]);
    subset_args[13] = Value::list(vec![
        Value::symbol("latin-iso8859-2-test"),
        Value::fixnum(160),
        Value::fixnum(255),
        Value::fixnum(-128),
    ]);
    builtin_define_charset_internal(subset_args).unwrap();

    let mut data = FontsetData::default();
    data.update_target(
        FontsetTarget::Range(0x80, 0x10FFFF),
        FontSpecEntry::Font(StoredFontSpec {
            family: None,
            registry: Some(intern("iso8859-2")),
            lang: None,
            weight: None,
            slant: None,
            width: None,
            otf: None,
            definition: Some(FontDefinitionMetadata {
                encoding: intern("iso-8859-2-test"),
                repertory: Some(FontRepertory::Charset(intern("iso-8859-2-test"))),
            }),
        }),
        FontsetAddMode::Append,
    );
    data.update_target(
        FontsetTarget::Range(0x80, 0x10FFFF),
        FontSpecEntry::Font(StoredFontSpec {
            family: None,
            registry: Some(intern("iso10646-1")),
            lang: None,
            weight: None,
            slant: None,
            width: None,
            otf: None,
            definition: Some(FontDefinitionMetadata {
                encoding: intern("unicode-bmp"),
                repertory: Some(FontRepertory::Charset(intern("unicode-bmp"))),
            }),
        }),
        FontsetAddMode::Append,
    );

    let registries: Vec<_> = data
        .entries_for_char('好' as u32, FontsetLookupPurpose::Patterns)
        .into_iter()
        .filter_map(|entry| match entry {
            FontSpecEntry::Font(spec) => spec.registry.map(|sym| resolve_sym(sym).to_string()),
            FontSpecEntry::ExplicitNone => None,
        })
        .collect();

    assert_eq!(registries, vec!["iso10646-1".to_string()]);
}

#[test]
fn repertory_target_ranges_support_subset_charsets() {
    crate::test_utils::init_test_tracing();
    reset_charset_registry();

    let mut parent_args = vec![Value::NIL; 17];
    parent_args[0] = Value::symbol("latin-iso8859-2-test");
    parent_args[1] = Value::fixnum(1);
    parent_args[2] = Value::vector(vec![Value::fixnum(0), Value::fixnum(255)]);
    parent_args[8] = Value::T;
    parent_args[12] = Value::string("8859-2");
    builtin_define_charset_internal(parent_args).unwrap();

    let mut subset_args = vec![Value::NIL; 17];
    subset_args[0] = Value::symbol("iso-8859-2-test");
    subset_args[1] = Value::fixnum(1);
    subset_args[2] = Value::vector(vec![Value::fixnum(32), Value::fixnum(127)]);
    subset_args[13] = Value::list(vec![
        Value::symbol("latin-iso8859-2-test"),
        Value::fixnum(160),
        Value::fixnum(255),
        Value::fixnum(-128),
    ]);
    builtin_define_charset_internal(subset_args).unwrap();

    let ranges = neovm_text::emacs_core::charset::charset_target_ranges("iso-8859-2-test")
        .expect("subset charset ranges");
    assert!(
        ranges
            .iter()
            .any(|(from, to)| *from <= 0x00A0 && 0x00A0 <= *to)
    );
    assert!(
        ranges
            .iter()
            .any(|(from, to)| *from <= 0x017D && 0x017D <= *to)
    );
}

#[test]
fn parse_font_spec_entry_preserves_raw_unibyte_string_names() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
        0xFF,
    ]));
    let entry = parse_font_spec_entry(&raw, None).expect("parse raw font spec");
    match entry {
        FontSpecEntry::Font(spec) => {
            // Issue #131: the family is interned faithfully, so its symbol name
            // keeps the raw byte (0xFF) instead of the PUA-sentinel storage form.
            let family = spec.family.expect("family symbol");
            assert_eq!(
                neovm_value::intern::resolve_sym_lisp_string(family).as_bytes(),
                &[0xFF]
            );
            assert_eq!(spec.registry, None);
        }
        FontSpecEntry::ExplicitNone => panic!("expected font entry"),
    }
}

#[test]
fn registry_storage_uses_property_free_spellings_for_names_and_aliases() {
    crate::test_utils::init_test_tracing();
    let mut registry = FontsetRegistry::with_defaults();
    let name = fontset_name_spelling("-*-fixed-medium-r-normal-*-16-*-*-*-*-*-fontset-unit");
    let alias = fontset_name_spelling("fontset-unit");
    let registered = registry.register_fontset(name.clone(), Some(alias.clone()));

    assert!(registry.fontsets.contains_key(&name));
    assert_eq!(registry.alias_to_name.get(&alias), Some(&name));
    assert!(
        registry
            .ordered_names
            .iter()
            .any(|candidate| candidate == &name)
    );
    assert_eq!(registered, name);

    let listed = list_to_vec(&registry.list_value());
    assert!(listed.contains(&Value::heap_string(name.to_lisp_string())));

    let alias_alist = list_to_vec(&registry.alias_alist_value());
    assert!(alias_alist.contains(&Value::cons(
        Value::heap_string(name.to_lisp_string()),
        Value::heap_string(alias.to_lisp_string())
    )));
}

#[test]
fn registry_spellings_preserve_non_ascii_and_string_byte_modes() {
    crate::test_utils::init_test_tracing();
    let mut registry = FontsetRegistry::with_defaults();
    let unicode = fontset_name_spelling("fontset-字-é");
    let raw = FontNameSpelling::from(&LispString::from_unibyte(vec![0xff, b'A']));
    let multibyte_ascii = fontset_name_spelling("fontset-ascii");
    let unibyte_ascii =
        FontNameSpelling::from(&LispString::from_unibyte(b"fontset-ascii".to_vec()));

    registry.register_fontset(unicode.clone(), Some(raw.clone()));
    registry.register_fontset(multibyte_ascii.clone(), None);
    registry.register_fontset(unibyte_ascii.clone(), None);
    assert_eq!(registry.alias_to_name.get(&raw), Some(&unicode));
    assert_ne!(multibyte_ascii, unibyte_ascii);
    assert!(registry.fontsets.contains_key(&multibyte_ascii));
    assert!(registry.fontsets.contains_key(&unibyte_ascii));

    let names = list_to_vec(&registry.list_value());
    for spelling in [&unicode, &multibyte_ascii, &unibyte_ascii] {
        let expected = spelling.to_lisp_string();
        assert!(
            names
                .iter()
                .any(|name| name.as_lisp_string() == Some(&expected))
        );
    }
    let aliases = list_to_vec(&registry.alias_alist_value());
    let alias = aliases
        .iter()
        .find(|pair| pair.cons_car().as_lisp_string() == Some(&unicode.to_lisp_string()))
        .unwrap()
        .cons_cdr();
    let alias = alias.as_lisp_string().unwrap();
    assert_eq!(alias.as_bytes(), &[0xff, b'A']);
    assert!(!alias.is_multibyte());
    assert!(!alias.has_intervals());
}

#[test]
fn snapshot_fontset_registry_preserves_lisp_string_names() {
    crate::test_utils::init_test_tracing();
    reset_fontset_registry();

    let custom_name = fontset_name_lisp_string("fontset-snapshot");
    let custom_alias = fontset_name_lisp_string("fontset-snapshot-alias");
    restore_fontset_registry(FontsetRegistrySnapshot {
        ordered_names: vec![
            fontset_name_lisp_string(DEFAULT_FONTSET_NAME),
            custom_name.clone(),
        ],
        alias_to_name: vec![
            (
                fontset_name_lisp_string(DEFAULT_FONTSET_ALIAS),
                fontset_name_lisp_string(DEFAULT_FONTSET_NAME),
            ),
            (custom_alias.clone(), custom_name.clone()),
        ],
        fontsets: vec![
            (
                fontset_name_lisp_string(DEFAULT_FONTSET_NAME),
                FontsetDataSnapshot::default(),
            ),
            (custom_name.clone(), FontsetDataSnapshot::default()),
        ],
        generation: 7,
    });

    let snapshot = snapshot_fontset_registry();
    assert_eq!(
        snapshot.ordered_names,
        vec![
            fontset_name_lisp_string(DEFAULT_FONTSET_NAME),
            custom_name.clone(),
        ]
    );
    assert!(
        snapshot
            .alias_to_name
            .contains(&(custom_alias.clone(), custom_name.clone()))
    );
    assert!(
        snapshot
            .fontsets
            .iter()
            .any(|(name, _)| name == &custom_name)
    );
}

#[test]
fn fontset_registry_pdump_uses_lisp_string_names_and_loads_legacy_strings() {
    crate::test_utils::init_test_tracing();
    let _unit = crate::test_utils::unit_context();
    reset_fontset_registry();

    let fresh_name = fontset_name_lisp_string("fontset-pdump");
    let fresh_alias = fontset_name_lisp_string("fontset-pdump-alias");
    restore_fontset_registry(FontsetRegistrySnapshot {
        ordered_names: vec![
            fontset_name_lisp_string(DEFAULT_FONTSET_NAME),
            fresh_name.clone(),
        ],
        alias_to_name: vec![
            (
                fontset_name_lisp_string(DEFAULT_FONTSET_ALIAS),
                fontset_name_lisp_string(DEFAULT_FONTSET_NAME),
            ),
            (fresh_alias.clone(), fresh_name.clone()),
        ],
        fontsets: vec![
            (
                fontset_name_lisp_string(DEFAULT_FONTSET_NAME),
                FontsetDataSnapshot::default(),
            ),
            (fresh_name.clone(), FontsetDataSnapshot::default()),
        ],
        generation: 9,
    });

    let dumped = neovm_eval::emacs_core::pdump::convert::dump_fontset_registry();
    assert!(dumped.ordered_names.is_empty());
    assert!(dumped.alias_to_name.is_empty());
    assert!(dumped.fontsets.is_empty());
    assert!(
        dumped
            .ordered_names_lisp
            .iter()
            .any(|name| { name.data == fresh_name.as_bytes() && name.size == fresh_name.schars() })
    );
    assert!(dumped.alias_to_name_lisp.iter().any(|(alias, name)| {
        alias.data == fresh_alias.as_bytes() && name.data == fresh_name.as_bytes()
    }));
    assert!(dumped.fontsets_lisp.iter().any(|(name, _)| {
        name.data == fresh_name.as_bytes() && name.size == fresh_name.schars()
    }));

    reset_fontset_registry();
    let legacy_name = "fontset-legacy".to_string();
    let legacy_alias = "fontset-legacy-alias".to_string();
    let legacy_dump = neovm_eval::emacs_core::pdump::types::DumpFontsetRegistry {
        ordered_names_lisp: Vec::new(),
        alias_to_name_lisp: Vec::new(),
        fontsets_lisp: Vec::new(),
        ordered_names: vec![DEFAULT_FONTSET_NAME.to_string(), legacy_name.clone()],
        alias_to_name: vec![
            (
                DEFAULT_FONTSET_ALIAS.to_string(),
                DEFAULT_FONTSET_NAME.to_string(),
            ),
            (legacy_alias.clone(), legacy_name.clone()),
        ],
        fontsets: vec![
            (
                DEFAULT_FONTSET_NAME.to_string(),
                neovm_eval::emacs_core::pdump::types::DumpFontsetData {
                    ranges: Vec::new(),
                    fallback: None,
                },
            ),
            (
                legacy_name.clone(),
                neovm_eval::emacs_core::pdump::types::DumpFontsetData {
                    ranges: Vec::new(),
                    fallback: None,
                },
            ),
        ],
        generation: 11,
    };
    neovm_eval::test_support::authority::load_fontset_registry_for_test(&legacy_dump);

    let restored = snapshot_fontset_registry();
    let legacy_name_lisp = fontset_name_lisp_string(&legacy_name);
    let legacy_alias_lisp = fontset_name_lisp_string(&legacy_alias);
    assert!(restored.ordered_names.contains(&legacy_name_lisp));
    assert!(
        restored
            .alias_to_name
            .contains(&(legacy_alias_lisp, legacy_name_lisp.clone()))
    );
    assert!(
        restored
            .fontsets
            .iter()
            .any(|(name, _)| name == &legacy_name_lisp)
    );
}

#[test]
fn fontset_registry_pdump_uses_symbol_identity_for_charset_repertories() {
    crate::test_utils::init_test_tracing();
    let _unit = crate::test_utils::unit_context();
    reset_fontset_registry();

    let repertory_sym = intern("unicode-bmp");
    let family_sym = intern("fixed");
    let registry_sym = intern("iso10646-1");
    let lang_sym = intern("ja");
    restore_fontset_registry(FontsetRegistrySnapshot {
        ordered_names: vec![fontset_name_lisp_string(DEFAULT_FONTSET_NAME)],
        alias_to_name: vec![(
            fontset_name_lisp_string(DEFAULT_FONTSET_ALIAS),
            fontset_name_lisp_string(DEFAULT_FONTSET_NAME),
        )],
        fontsets: vec![(
            fontset_name_lisp_string(DEFAULT_FONTSET_NAME),
            FontsetDataSnapshot {
                ranges: vec![FontsetRangeEntrySnapshot {
                    from: 0x80,
                    to: 0x10FFFF,
                    entries: vec![FontSpecEntry::Font(StoredFontSpec {
                        family: Some(family_sym),
                        registry: Some(registry_sym),
                        lang: Some(lang_sym),
                        weight: None,
                        slant: None,
                        width: None,
                        otf: None,
                        definition: Some(FontDefinitionMetadata {
                            encoding: repertory_sym,
                            repertory: Some(FontRepertory::Charset(repertory_sym)),
                        }),
                    })],
                }],
                fallback: None,
            },
        )],
        generation: 13,
    });

    let dumped = neovm_eval::emacs_core::pdump::convert::dump_fontset_registry();
    let spec = dumped
        .fontsets_lisp
        .iter()
        .find(|(name, _)| name.data == DEFAULT_FONTSET_NAME.as_bytes())
        .and_then(|(_, data)| data.ranges.first())
        .and_then(|range| range.entries.first())
        .and_then(|entry| match entry {
            neovm_eval::emacs_core::pdump::types::DumpFontSpecEntry::Font(spec) => Some(spec),
            neovm_eval::emacs_core::pdump::types::DumpFontSpecEntry::ExplicitNone => None,
        })
        .expect("dumped font spec");

    assert!(matches!(
        spec.definition.as_ref().expect("dumped definition").repertory.as_ref().expect("dumped repertory"),
        neovm_eval::emacs_core::pdump::types::DumpFontRepertory::CharsetSym(sym)
            if sym.0 == repertory_sym.to_raw()
    ));
    assert_eq!(
        spec.family_sym,
        Some(neovm_eval::emacs_core::pdump::types::DumpSymId(
            family_sym.to_raw()
        ))
    );
    assert_eq!(
        spec.registry_sym,
        Some(neovm_eval::emacs_core::pdump::types::DumpSymId(
            registry_sym.to_raw()
        ))
    );
    assert_eq!(
        spec.lang_sym,
        Some(neovm_eval::emacs_core::pdump::types::DumpSymId(
            lang_sym.to_raw()
        ))
    );
    assert!(spec.family.is_none());
    assert!(spec.registry.is_none());
    assert!(spec.lang.is_none());
}

#[test]
fn resolve_fontset_name_maps_a_font_name_to_the_default_fontset() {
    crate::test_utils::init_test_tracing();
    // t / nil resolve to the default fontset.
    assert_eq!(
        resolve_fontset_name_arg(&Value::T).unwrap(),
        DEFAULT_FONTSET_NAME
    );
    assert_eq!(
        resolve_fontset_name_arg(&Value::NIL).unwrap(),
        DEFAULT_FONTSET_NAME
    );
    // A plain font name (the `(frame-parameter nil 'font)` idiom, issue #177)
    // resolves to the default fontset -- the one neomacs renders from -- rather
    // than a phantom fontset nothing consults.
    let font_name = Value::string("-*-hack-regular-normal-*-*-27-*-*-*-*-*-*-*");
    assert_eq!(
        resolve_fontset_name_arg(&font_name).unwrap(),
        DEFAULT_FONTSET_NAME
    );
    // A genuine `-...-fontset-NAME` XLFD keeps its own name.
    let fontset_name = "-*-*-*-*-*-*-*-*-*-*-*-*-fontset-mine";
    assert_eq!(
        resolve_fontset_name_arg(&Value::string(fontset_name)).unwrap(),
        fontset_name
    );
}

fn registry_names(entries: &[FontSpecEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| match entry {
            FontSpecEntry::Font(spec) => spec.registry.map_or_else(String::new, |sym| {
                neovm_value::intern::resolve_sym(sym).to_owned()
            }),
            FontSpecEntry::ExplicitNone => "none".to_owned(),
        })
        .collect()
}

/// GNU's `reorder_font_vector`: rendering tries the entry whose encoding
/// comes first among the preferred charsets, whatever the definition order,
/// and follows each `set-charset-priority`.  `fontset-font` still reports the
/// definition order.
#[test]
fn rendering_tries_fontset_entries_in_charset_priority_order() {
    crate::test_utils::init_test_tracing();
    reset_fontset_registry();
    reset_charset_registry();
    let encodings = Value::list(vec![
        Value::cons(Value::string("iso8859-1"), Value::symbol("iso-8859-1")),
        Value::cons(Value::string("iso10646-1"), Value::symbol("unicode-bmp")),
    ]);
    for registry in ["iso8859-1", "iso10646-1"] {
        set_fontset_font(
            &Value::T,
            &Value::fixnum('é' as i64),
            &Value::cons(Value::NIL, Value::string(registry)),
            Some(&Value::symbol("append")),
            None,
            None,
            Some(&encodings),
        )
        .expect("install a registry entry");
    }
    let set_priority = |charset: &'static str| {
        neovm_text::test_support::authority::set_charset_priority_for_test(vec![Value::symbol(
            charset,
        )])
        .expect("set-charset-priority");
    };
    let rendering = || registry_names(&matching_entries_for_char('é'));

    let before = fontset_generation();
    set_priority("unicode-bmp");
    assert_ne!(
        fontset_generation(),
        before,
        "cached lookups must be redone"
    );
    assert_eq!(rendering(), ["iso10646-1", "iso8859-1"]);

    set_priority("iso-8859-1");
    assert_eq!(rendering(), ["iso8859-1", "iso10646-1"]);

    set_priority("unicode-bmp");
    let patterns = registry().read().unwrap().matching_entries_for_char(
        &fontset_name_spelling(DEFAULT_FONTSET_NAME),
        'é',
        FontsetLookupPurpose::Patterns,
    );
    assert_eq!(registry_names(&patterns), ["iso8859-1", "iso10646-1"]);
}

/// GNU compares charset ids, so an encoding written as a charset alias in
/// `font-encoding-alist` ranks like the charset it names.
#[test]
fn an_encoding_spelled_as_a_charset_alias_ranks_like_its_charset() {
    crate::test_utils::init_test_tracing();
    reset_fontset_registry();
    reset_charset_registry();
    neovm_text::test_support::authority::define_charset_alias_for_test(vec![
        Value::symbol("neomacs-test-bmp"),
        Value::symbol("unicode-bmp"),
    ])
    .expect("define-charset-alias");
    let encodings = Value::list(vec![
        Value::cons(Value::string("iso8859-1"), Value::symbol("iso-8859-1")),
        Value::cons(
            Value::string("iso10646-1"),
            Value::symbol("neomacs-test-bmp"),
        ),
    ]);
    for registry in ["iso8859-1", "iso10646-1"] {
        set_fontset_font(
            &Value::T,
            &Value::fixnum('é' as i64),
            &Value::cons(Value::NIL, Value::string(registry)),
            Some(&Value::symbol("append")),
            None,
            None,
            Some(&encodings),
        )
        .expect("install a registry entry");
    }
    neovm_text::test_support::authority::set_charset_priority_for_test(vec![Value::symbol(
        "unicode-bmp",
    )])
    .expect("set-charset-priority");
    assert_eq!(
        registry_names(&matching_entries_for_char('é')),
        ["iso10646-1", "iso8859-1"]
    );

    // Re-pointing the alias later does not change the defined entry.
    neovm_text::test_support::authority::define_charset_alias_for_test(vec![
        Value::symbol("neomacs-test-bmp"),
        Value::symbol("iso-8859-1"),
    ])
    .expect("re-point the alias");
    // With the alias resolved at lookup time, the entry would now be
    // `iso-8859-1`, as non-preferred as the other, and keep definition order.
    neovm_text::test_support::authority::set_charset_priority_for_test(vec![Value::symbol(
        "unicode-bmp",
    )])
    .expect("set-charset-priority");
    assert_eq!(
        registry_names(&matching_entries_for_char('é')),
        ["iso10646-1", "iso8859-1"]
    );
}

/// A context that is created (or a registry that is reset) does not replace
/// the order published by `set-charset-priority`; only a priority change does.
#[test]
fn resetting_a_charset_registry_keeps_the_published_order() {
    crate::test_utils::init_test_tracing();
    reset_fontset_registry();
    reset_charset_registry();
    let encodings = Value::list(vec![
        Value::cons(Value::string("iso8859-1"), Value::symbol("iso-8859-1")),
        Value::cons(Value::string("iso10646-1"), Value::symbol("unicode-bmp")),
    ]);
    for registry in ["iso8859-1", "iso10646-1"] {
        set_fontset_font(
            &Value::T,
            &Value::fixnum('é' as i64),
            &Value::cons(Value::NIL, Value::string(registry)),
            Some(&Value::symbol("append")),
            None,
            None,
            Some(&encodings),
        )
        .expect("install a registry entry");
    }
    neovm_text::test_support::authority::set_charset_priority_for_test(vec![Value::symbol(
        "unicode-bmp",
    )])
    .expect("set-charset-priority");
    let generation = fontset_generation();

    reset_charset_registry();

    assert_eq!(fontset_generation(), generation);
    assert_eq!(
        registry_names(&matching_entries_for_char('é')),
        ["iso10646-1", "iso8859-1"]
    );
}

#[test]
fn stored_otf_spec_preserves_gnu_langsys_and_feature_shape() {
    crate::test_utils::init_test_tracing();
    let spec = parse_stored_otf_spec(&Value::list(vec![
        Value::symbol("arab"),
        Value::symbol("URD "),
        Value::list(vec![
            Value::symbol("init"),
            Value::NIL,
            Value::symbol("liga"),
        ]),
        Value::NIL,
    ]))
    .expect("valid :otf shape");
    assert_eq!(spec.script, "arab");
    assert_eq!(spec.langsys, StoredOtfLangsys::Named("URD ".to_owned()));
    assert_eq!(
        spec.gsub,
        Some(StoredOtfFeatures {
            required: vec!["init".to_owned()],
            forbidden: vec!["liga".to_owned()],
        })
    );
    assert!(spec.gpos.is_none());
}

#[test]
fn stored_otf_spec_marks_non_symbol_langsys_unmatchable() {
    crate::test_utils::init_test_tracing();
    // GNU's validator re-tests XCAR (val) for the second slot, so a string
    // reaches font_check_otf; `assq_no_quit` can never `eq` it to a tag
    // symbol, so every feature requirement rejects the font.
    let spec = parse_stored_otf_spec(&Value::list(vec![
        Value::symbol("arab"),
        Value::string("URD "),
        Value::list(vec![Value::symbol("init")]),
    ]))
    .expect("the validator quirk reaches the store");
    assert_eq!(spec.langsys, StoredOtfLangsys::Unmatchable);
    assert!(spec.gsub.is_some());

    let nil_langsys = parse_stored_otf_spec(&Value::list(vec![
        Value::symbol("arab"),
        Value::NIL,
        Value::list(vec![Value::symbol("init")]),
    ]))
    .expect("nil LANGSYS means the default langsys");
    assert_eq!(nil_langsys.langsys, StoredOtfLangsys::Default);
}
