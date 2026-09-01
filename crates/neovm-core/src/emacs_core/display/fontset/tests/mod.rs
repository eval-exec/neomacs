use super::*;
use crate::emacs_core::charset::{builtin_define_charset_internal, reset_charset_registry};
use crate::emacs_core::intern::intern;

fn registry_spec(name: &str) -> FontSpecEntry {
    FontSpecEntry::Font(StoredFontSpec {
        family: None,
        registry: Some(intern(name)),
        lang: None,
        weight: None,
        slant: None,
        width: None,
        repertory: None,
    })
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
        spec.repertory,
        Some(FontRepertory::Charset(intern("ascii"))),
        "GNU find_font_encoding falls back to Qascii when no alist pattern matches"
    );
    assert!(spec.matches_char('A' as u32));
    assert!(
        !spec.matches_char(0xE6AD),
        "an unmatched family-only spec must not capture a private-use icon"
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
        repertory: None,
    };

    let nil_repertory_entry = Value::list(vec![
        Value::string("Fixture-$"),
        Value::symbol("unicode-bmp"),
    ]);
    let nil_repertory_alist = Value::list(vec![nil_repertory_entry]);
    assert_eq!(
        resolve_font_repertory(&spec, Some(&nil_repertory_alist)),
        FontRepertoryConstraint::Unrestricted,
        "(ENCODING) has GNU's explicit nil repertory"
    );

    let explicit_repertory_entry = Value::cons(
        Value::string("Fixture-$"),
        Value::cons(Value::symbol("unicode"), Value::symbol("unicode-bmp")),
    );
    let explicit_repertory_alist = Value::list(vec![explicit_repertory_entry]);
    assert_eq!(
        resolve_font_repertory(&spec, Some(&explicit_repertory_alist)),
        FontRepertoryConstraint::Restricted(FontRepertory::Charset(intern("unicode-bmp")))
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
        data.matching_entries_for_char('好' as u32),
        vec![registry_spec("gb2312.1980-0"), registry_spec("iso10646-1")]
    );
}

#[test]
fn repertory_charset_filters_non_matching_entries() {
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
            repertory: Some(FontRepertory::Charset(intern("iso-8859-1"))),
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
            repertory: Some(FontRepertory::Charset(intern("unicode-bmp"))),
        }),
        FontsetAddMode::Append,
    );

    let registries: Vec<_> = data
        .matching_entries_for_char('好' as u32)
        .into_iter()
        .filter_map(|entry| match entry {
            FontSpecEntry::Font(spec) => spec.registry.map(|sym| resolve_sym(sym).to_string()),
            FontSpecEntry::ExplicitNone => None,
        })
        .collect();

    assert_eq!(registries, vec!["iso10646-1".to_string()]);
}

#[test]
fn repertory_subset_charset_filters_non_matching_entries() {
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
            repertory: Some(FontRepertory::Charset(intern("iso-8859-2-test"))),
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
            repertory: Some(FontRepertory::Charset(intern("unicode-bmp"))),
        }),
        FontsetAddMode::Append,
    );

    let registries: Vec<_> = data
        .matching_entries_for_char('好' as u32)
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

    let ranges = crate::emacs_core::charset::charset_target_ranges("iso-8859-2-test")
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
fn repertory_target_ranges_support_symbol_backed_charsets() {
    crate::test_utils::init_test_tracing();
    let ranges = repertory_target_ranges(&FontRepertory::Charset(intern("unicode-bmp")))
        .expect("symbol-backed repertory ranges");
    assert!(
        ranges
            .iter()
            .any(|(from, to)| *from <= ('好' as u32) && ('好' as u32) <= *to)
    );
}

#[test]
fn parse_font_spec_entry_preserves_raw_unibyte_string_names() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let entry = parse_font_spec_entry(&raw, None).expect("parse raw font spec");
    match entry {
        FontSpecEntry::Font(spec) => {
            // Issue #131: the family is interned faithfully, so its symbol name
            // keeps the raw byte (0xFF) instead of the PUA-sentinel storage form.
            let family = spec.family.expect("family symbol");
            assert_eq!(
                crate::emacs_core::intern::resolve_sym_lisp_string(family).as_bytes(),
                &[0xFF]
            );
            assert_eq!(spec.registry, None);
        }
        FontSpecEntry::ExplicitNone => panic!("expected font entry"),
    }
}

#[test]
fn registry_storage_uses_lisp_strings_for_names_and_aliases() {
    crate::test_utils::init_test_tracing();
    let mut registry = FontsetRegistry::with_defaults();
    let name = fontset_name_lisp_string("-*-fixed-medium-r-normal-*-16-*-*-*-*-*-fontset-unit");
    let alias = fontset_name_lisp_string("fontset-unit");
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
    assert!(listed.contains(&Value::heap_string(name.clone())));

    let alias_alist = list_to_vec(&registry.alias_alist_value());
    assert!(alias_alist.contains(&Value::cons(
        Value::heap_string(name),
        Value::heap_string(alias)
    )));
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

    let dumped = crate::emacs_core::pdump::convert::dump_fontset_registry();
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
    let legacy_dump = crate::emacs_core::pdump::types::DumpFontsetRegistry {
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
                crate::emacs_core::pdump::types::DumpFontsetData {
                    ranges: Vec::new(),
                    fallback: None,
                },
            ),
            (
                legacy_name.clone(),
                crate::emacs_core::pdump::types::DumpFontsetData {
                    ranges: Vec::new(),
                    fallback: None,
                },
            ),
        ],
        generation: 11,
    };
    crate::emacs_core::pdump::convert::load_fontset_registry(&legacy_dump);

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
                        repertory: Some(FontRepertory::Charset(repertory_sym)),
                    })],
                }],
                fallback: None,
            },
        )],
        generation: 13,
    });

    let dumped = crate::emacs_core::pdump::convert::dump_fontset_registry();
    let spec = dumped
        .fontsets_lisp
        .iter()
        .find(|(name, _)| name.data == DEFAULT_FONTSET_NAME.as_bytes())
        .and_then(|(_, data)| data.ranges.first())
        .and_then(|range| range.entries.first())
        .and_then(|entry| match entry {
            crate::emacs_core::pdump::types::DumpFontSpecEntry::Font(spec) => Some(spec),
            crate::emacs_core::pdump::types::DumpFontSpecEntry::ExplicitNone => None,
        })
        .expect("dumped font spec");

    assert!(matches!(
        spec.repertory.as_ref().expect("dumped repertory"),
        crate::emacs_core::pdump::types::DumpFontRepertory::CharsetSym(sym)
            if sym.0 == repertory_sym.0
    ));
    assert_eq!(
        spec.family_sym,
        Some(crate::emacs_core::pdump::types::DumpSymId(family_sym.0))
    );
    assert_eq!(
        spec.registry_sym,
        Some(crate::emacs_core::pdump::types::DumpSymId(registry_sym.0))
    );
    assert_eq!(
        spec.lang_sym,
        Some(crate::emacs_core::pdump::types::DumpSymId(lang_sym.0))
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
