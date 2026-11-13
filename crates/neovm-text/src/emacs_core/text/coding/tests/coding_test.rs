use super::*;

use neovm_value::error::{FlowKind, FlowResultExt as _};

use neovm_value::intern::{intern, resolve_sym};

/// Invalid persisted policy values fall back to the built-in inherited policy.
#[test]
fn undecided_null_policy_invalid_storage_inherits_global() {
    crate::test_utils::init_test_tracing();
    for stored in [Value::fixnum(2), Value::fixnum(-2), Value::T] {
        let mut manager = mgr();
        manager
            .get_mut("undecided")
            .unwrap()
            .int_properties
            .insert(UNDECIDED_KEY_INHIBIT_NULL, stored);
        for global in [false, true] {
            assert_eq!(
                undecided_null_byte_inhibition(&manager, "undecided", global),
                Some(global),
                "stored={stored:?}, global={global}",
            );
        }
    }
}

// Model the manager image produced by a dump that predates the compiled NUL
// slot. Its original definition plist remains separate from public put overrides.
fn restored_null_policy_manager(
    definition: Option<Value>,
    compiled: Option<Value>,
) -> CodingSystemManager {
    let mut manager = mgr();
    let mut info =
        CodingSystemInfo::new("legacy-null-policy", "undecided", '-', EolType::Undecided);
    let plist = definition.map_or(Value::NIL, |flag| {
        Value::list(vec![Value::symbol(":inhibit-null-byte-detection"), flag])
    });
    info.int_properties.insert(PLIST_VERBATIM_KEY, plist);
    if let Some(compiled) = compiled {
        info.int_properties
            .insert(UNDECIDED_KEY_INHIBIT_NULL, compiled);
    }
    manager.register(info);
    manager.register(CodingSystemInfo::new(
        "legacy-null-policy-dos",
        "undecided",
        '-',
        EolType::Dos,
    ));
    manager.add_alias("legacy-null-alias", "legacy-null-policy");
    manager.add_alias("legacy-null-dos-alias", "legacy-null-policy-dos");
    CodingSystemManager::from_dump(
        manager.systems,
        manager.aliases,
        manager.alias_order,
        manager.priority,
        manager.keyboard_coding,
        manager.terminal_coding,
    )
}

#[test]
fn legacy_undecided_null_policy_recovers_definition_not_public_overrides() {
    crate::test_utils::init_test_tracing();
    for (definition, expected) in [
        (Value::NIL, [false, false]),
        (Value::T, [true, true]),
        (Value::symbol("inherit"), [false, true]),
    ] {
        for public in [Value::NIL, Value::T, Value::symbol("inherit")] {
            let mut manager = restored_null_policy_manager(Some(definition), None);
            builtin_coding_system_put(
                &mut manager,
                vec![
                    Value::symbol("legacy-null-alias"),
                    Value::symbol(":inhibit-null-byte-detection"),
                    public,
                ],
                SymbolMode::Bare,
            )
            .unwrap();
            for name in [
                "legacy-null-policy",
                "legacy-null-alias",
                "legacy-null-policy-dos",
                "legacy-null-dos-alias",
            ] {
                for (global, expected) in [false, true].into_iter().zip(expected) {
                    assert_eq!(
                        undecided_null_byte_inhibition(&manager, name, global),
                        Some(expected),
                        "definition={definition:?}, public={public:?}, name={name}, global={global}"
                    );
                }
            }
        }
    }
}

#[test]
fn legacy_omitted_null_attribute_differs_from_bootstrap_without_definition() {
    crate::test_utils::init_test_tracing();
    let manager = restored_null_policy_manager(None, None);
    for global in [false, true] {
        for name in [
            "legacy-null-policy",
            "legacy-null-alias",
            "legacy-null-policy-dos",
            "legacy-null-dos-alias",
        ] {
            assert_eq!(
                undecided_null_byte_inhibition(&manager, name, global),
                Some(false),
                "omitted definition flag, name={name}, global={global}"
            );
        }
        assert_eq!(
            undecided_null_byte_inhibition(&manager, "undecided", global),
            Some(global)
        );
    }
}

#[test]
fn compiled_null_policy_precedes_original_definition_and_subsidiary_fallback() {
    crate::test_utils::init_test_tracing();
    for (compiled, expected) in [
        (Value::fixnum(-1), [false, false]),
        (Value::fixnum(1), [true, true]),
        (Value::fixnum(0), [false, true]),
        (Value::fixnum(2), [false, true]),
    ] {
        for definition in [Value::NIL, Value::T] {
            let mut manager = restored_null_policy_manager(Some(definition), Some(compiled));
            builtin_coding_system_put(
                &mut manager,
                vec![
                    Value::symbol("legacy-null-policy"),
                    Value::symbol(":inhibit-null-byte-detection"),
                    if definition.is_nil() {
                        Value::T
                    } else {
                        Value::NIL
                    },
                ],
                SymbolMode::Bare,
            )
            .unwrap();
            for name in [
                "legacy-null-policy",
                "legacy-null-alias",
                "legacy-null-policy-dos",
                "legacy-null-dos-alias",
            ] {
                for (global, expected) in [false, true].into_iter().zip(expected) {
                    assert_eq!(
                        undecided_null_byte_inhibition(&manager, name, global),
                        Some(expected),
                        "compiled={compiled:?}, definition={definition:?}, name={name}, global={global}"
                    );
                }
            }
            // A descriptor with its own original definition must not borrow
            // a different base descriptor's compiled flag.
            let subsidiary = manager.get_mut("legacy-null-policy-dos").unwrap();
            subsidiary.int_properties.insert(
                PLIST_VERBATIM_KEY,
                Value::list(vec![
                    Value::symbol(":inhibit-null-byte-detection"),
                    Value::NIL,
                ]),
            );
            for global in [false, true] {
                assert_eq!(
                    undecided_null_byte_inhibition(&manager, "legacy-null-dos-alias", global),
                    Some(false)
                );
            }
        }
    }
}

#[test]
fn eol_type_domain_matches_gnu_symbols_and_codes() {
    assert_eq!(
        EolType::from_specified_symbol_name("unix"),
        Some(EolType::Unix)
    );
    assert_eq!(
        EolType::from_specified_symbol_name("dos"),
        Some(EolType::Dos)
    );
    assert_eq!(
        EolType::from_specified_symbol_name("mac"),
        Some(EolType::Mac)
    );
    assert_eq!(EolType::from_specified_symbol_name("undecided"), None);
    assert_eq!(EolType::from_specified_symbol_name("crlf"), None);

    assert_eq!(EolType::Unix.to_int(), 0);
    assert_eq!(EolType::Dos.to_int(), 1);
    assert_eq!(EolType::Mac.to_int(), 2);
    assert_eq!(i8::from(EolType::Unix), 0);
    assert_eq!(i8::from(EolType::Dos), 1);
    assert_eq!(i8::from(EolType::Mac), 2);
    assert_eq!(EolType::try_from(0_i8), Ok(EolType::Unix));
    assert_eq!(EolType::try_from(1_i8), Ok(EolType::Dos));
    assert_eq!(EolType::try_from(2_i8), Ok(EolType::Mac));
    assert!(EolType::try_from(3_i8).is_err());
    assert_eq!(EolType::Unix.name(), "unix");
}

#[test]
fn new_manager_has_standard_systems() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    assert!(m.is_known("utf-8"));
    assert!(m.is_known("utf-8-unix"));
    assert!(m.is_known("utf-8-dos"));
    assert!(m.is_known("utf-8-mac"));
    assert!(m.is_known("latin-1"));
    assert!(m.is_known("ascii"));
    assert!(m.is_known("binary"));
    assert!(m.is_known("raw-text"));
    assert!(m.is_known("undecided"));
    assert!(m.is_known("emacs-internal"));
    assert!(m.is_known("no-conversion"));
    assert!(m.is_known("iso-latin-5"));
    assert!(m.is_known("iso-latin-5-unix"));
    assert!(m.is_known("iso-8859-9"));
    assert!(m.is_known("latin-5"));
    assert!(m.is_known("iso-latin-9"));
    assert!(m.is_known("iso-latin-9-unix"));
    assert!(m.is_known("iso-8859-15"));
    assert!(m.is_known("latin-9"));
    assert!(m.is_known("chinese-big5"));
    assert!(m.is_known("chinese-big5-unix"));
    assert!(m.is_known("big5"));
    assert!(m.is_known("cp950"));
    assert!(m.is_known("chinese-iso-8bit"));
    assert!(m.is_known("chinese-iso-8bit-unix"));
    assert!(m.is_known("cn-gb-2312"));
    assert!(m.is_known("gb2312"));
    assert!(m.is_known_or_derived("big5-unix"));
    assert!(m.is_known_or_derived("cp950-dos"));
    assert!(m.is_known_or_derived("cn-gb-2312-unix"));
    assert!(m.is_known_or_derived("gb2312-dos"));
}

#[test]
fn aliases_resolve() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    assert!(m.is_known("iso-8859-1")); // alias for latin-1
    assert!(m.is_known("iso-8859-9")); // alias for latin-5
    assert!(m.is_known("iso-8859-15")); // alias for latin-9
    assert!(m.is_known("us-ascii")); // alias for ascii
    assert!(m.is_known("mule-utf-8")); // alias for utf-8
    assert!(m.is_known("cn-gb-2312")); // alias for chinese-iso-8bit
    assert!(m.is_known("gb2312")); // alias for chinese-iso-8bit
    assert!(m.is_known("big5")); // alias for chinese-big5
    assert!(m.is_known("cp950")); // alias for chinese-big5
    assert_eq!(
        m.resolve("iso-8859-1").map(resolve_sym),
        Some("iso-latin-1")
    );
    assert_eq!(
        m.resolve("iso-8859-9").map(resolve_sym),
        Some("iso-latin-5")
    );
    assert_eq!(
        m.resolve("iso-8859-15").map(resolve_sym),
        Some("iso-latin-9")
    );
    assert_eq!(m.resolve("ascii").map(resolve_sym), Some("us-ascii"));
    assert_eq!(
        m.resolve("cn-gb-2312").map(resolve_sym),
        Some("chinese-iso-8bit")
    );
    assert_eq!(
        m.resolve("gb2312").map(resolve_sym),
        Some("chinese-iso-8bit")
    );
    assert_eq!(m.resolve("big5").map(resolve_sym), Some("chinese-big5"));
    assert_eq!(m.resolve("cp950").map(resolve_sym), Some("chinese-big5"));
}

#[test]
fn canonical_name_for_detected_eol_matches_gnu_alias_resolution() {
    crate::test_utils::init_test_tracing();
    let m = mgr();

    assert_eq!(
        m.canonical_name_for_detected_eol("cn-gb-2312", "-dos")
            .as_deref(),
        Some("chinese-iso-8bit-dos")
    );
    assert_eq!(
        m.canonical_name_for_detected_eol("big5", "-mac").as_deref(),
        Some("chinese-big5-mac")
    );
    assert_eq!(
        m.canonical_name_for_detected_eol("no-conversion", "-dos")
            .as_deref(),
        Some("no-conversion")
    );
    assert_eq!(
        m.canonical_name_for_detected_eol("cn-gb-2312-unix", "-dos")
            .as_deref(),
        Some("chinese-iso-8bit-unix")
    );
}

#[test]
fn unknown_system_not_known() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    assert!(!m.is_known("martian-encoding"));
    assert_eq!(m.resolve("martian-encoding"), None);
}

#[test]
fn add_alias_works() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    m.add_alias("my-utf8", "utf-8");
    assert!(m.is_known("my-utf8"));
    assert_eq!(m.resolve("my-utf8").map(resolve_sym), Some("utf-8"));
}

#[test]
fn base_name_strips_suffix() {
    crate::test_utils::init_test_tracing();
    let info = CodingSystemInfo::new("utf-8-unix", "utf-8", 'U', EolType::Unix);
    assert_eq!(info.base_name(), "utf-8");

    let info2 = CodingSystemInfo::new("utf-8", "utf-8", 'U', EolType::Undecided);
    assert_eq!(info2.base_name(), "utf-8");
}

#[test]
fn coding_system_list_all() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_list(&m, vec![]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert!(items.len() >= 11); // at least the 11 pre-registered systems
}

#[test]
fn coding_system_list_base_only() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_list(&m, vec![Value::T]).unwrap();
    let items = list_to_vec(&result).unwrap();
    // Should not contain utf-8-unix, utf-8-dos, utf-8-mac
    for item in &items {
        if let Some(id) = item.as_bare_symbol_id() {
            let s = resolve_sym(id);
            assert!(
                !s.ends_with("-unix") && !s.ends_with("-dos") && !s.ends_with("-mac"),
                "base-only list should not contain: {}",
                s
            );
        }
    }
}

#[test]
fn coding_system_list_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_list(&m, vec![Value::NIL, Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn coding_system_aliases_found() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_aliases(
        &m,
        vec![Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    let items = list_to_vec(&result).unwrap();
    // First element should be the canonical name
    assert!(items[0].is_bare_symbol_named("utf-8"));
    // Should include aliases like mule-utf-8
    assert!(items.len() > 1);
}

#[test]
fn coding_system_aliases_derive_eol_suffixes() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_aliases(
        &m,
        vec![Value::symbol("latin-1-unix")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(
        result,
        Value::list(vec![
            Value::symbol("iso-latin-1-unix"),
            Value::symbol("iso-8859-1-unix"),
            Value::symbol("latin-1-unix"),
        ])
    );
}

#[test]
fn coding_system_aliases_unknown() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_aliases(
        &m,
        vec![Value::symbol("nonexistent")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    assert!(result.is_err());
}

#[test]
fn coding_system_aliases_nil_maps_to_no_conversion_family() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_aliases(
        &m,
        vec![Value::NIL],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(
        result,
        Value::list(vec![
            Value::symbol("no-conversion"),
            Value::symbol("binary")
        ])
    );
}

#[test]
fn coding_system_aliases_string_is_type_error() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_aliases(
        &m,
        vec![Value::string("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    assert!(result.is_err());
}

#[test]
fn coding_system_get_name() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_get(
        &m,
        vec![Value::symbol("utf-8"), Value::symbol(":name")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_bare_symbol_named("utf-8"));
}

#[test]
fn coding_system_get_type() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_get(
        &m,
        vec![Value::symbol("latin-1"), Value::symbol(":coding-type")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_bare_symbol_named("charset"));
}

#[test]
fn coding_system_get_mnemonic() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_get(
        &m,
        vec![Value::symbol("utf-8"), Value::symbol(":mnemonic")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(eq_value(&result, &Value::fixnum('U' as i64)));
}

#[test]
fn coding_system_get_eol_type() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_get(
        &m,
        vec![Value::symbol("utf-8-unix"), Value::symbol(":eol-type")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn coding_system_get_unknown_prop() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_get(
        &m,
        vec![Value::symbol("utf-8"), Value::symbol(":nonexistent")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn coding_system_get_unknown_system() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_get(
        &m,
        vec![Value::symbol("bogus"), Value::symbol(":name")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    assert!(result.is_err());
}

#[test]
fn coding_system_plist_utf8_core_fields() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let plist = builtin_coding_system_plist(
        &m,
        vec![Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(plist_get(&plist, ":name"), Some(Value::symbol("utf-8")));
    assert_eq!(
        plist_get(&plist, ":coding-type"),
        Some(Value::symbol("utf-8"))
    );
    assert_eq!(
        plist_get(&plist, ":mnemonic"),
        Some(Value::fixnum('U' as i64))
    );
}

#[test]
fn coding_system_plist_normalizes_alias_and_eol_variant_name() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let latin = builtin_coding_system_plist(
        &m,
        vec![Value::symbol("latin-1")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(
        plist_get(&latin, ":name"),
        Some(Value::symbol("iso-latin-1"))
    );

    let utf8_unix = builtin_coding_system_plist(
        &m,
        vec![Value::symbol("utf-8-unix")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(plist_get(&utf8_unix, ":name"), Some(Value::symbol("utf-8")));
}

#[test]
fn coding_system_plist_nil_maps_to_no_conversion() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let plist = builtin_coding_system_plist(
        &m,
        vec![Value::NIL],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(
        plist_get(&plist, ":name"),
        Some(Value::symbol("no-conversion"))
    );
    assert_eq!(
        plist_get(&plist, ":coding-type"),
        Some(Value::symbol("raw-text"))
    );
}

#[test]
fn coding_system_plist_type_and_unknown_errors() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let type_err = builtin_coding_system_plist(
        &m,
        vec![Value::string("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    assert!(type_err.is_err());

    let unknown = builtin_coding_system_plist(
        &m,
        vec![Value::symbol("bogus")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    assert!(unknown.is_err());
}

#[test]
fn coding_system_plist_includes_custom_properties_from_put() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    builtin_coding_system_put(
        &mut m,
        vec![
            Value::symbol("utf-8"),
            Value::symbol(":foo"),
            Value::fixnum(42),
        ],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();

    let plist = builtin_coding_system_plist(
        &m,
        vec![Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(plist_get(&plist, ":foo"), Some(Value::fixnum(42)));
}

#[test]
fn coding_system_put_custom_prop() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let result = builtin_coding_system_put(
        &mut m,
        vec![
            Value::symbol("utf-8"),
            Value::symbol(":charset-list"),
            Value::list(vec![Value::symbol("unicode")]),
        ],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(result, Value::list(vec![Value::symbol("unicode")]));

    let info = m.get("utf-8").expect("utf-8 coding system should exist");
    assert!(info.properties.contains_key(&intern(":charset-list")));

    // Verify it was stored
    let get_result = builtin_coding_system_get(
        &m,
        vec![Value::symbol("utf-8"), Value::symbol(":charset-list")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(!get_result.is_nil());
}

#[test]
fn coding_system_put_mnemonic() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    builtin_coding_system_put(
        &mut m,
        vec![
            Value::symbol("utf-8"),
            Value::symbol(":mnemonic"),
            Value::char('X'),
        ],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();

    let result = builtin_coding_system_get(
        &m,
        vec![Value::symbol("utf-8"), Value::symbol(":mnemonic")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(eq_value(&result, &Value::fixnum('X' as i64)));
}

#[test]
fn coding_system_put_unknown_system_errors() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let result = builtin_coding_system_put(
        &mut m,
        vec![
            Value::symbol("bogus"),
            Value::symbol(":foo"),
            Value::fixnum(1),
        ],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    assert!(result.is_err());
}

#[test]
fn define_coding_system_internal_keeps_symbol_metadata() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    builtin_define_coding_system_internal(
        &mut m,
        vec![
            Value::symbol("vm-charset-coding"),
            Value::char('V'),
            Value::symbol("charset"),
            Value::list(vec![Value::symbol("ascii"), Value::symbol("unicode")]),
            Value::T,
            Value::NIL,
            Value::NIL,
            Value::symbol("post-read-fn"),
            Value::symbol("pre-write-fn"),
            Value::fixnum('?' as i64),
            Value::NIL,
            Value::list(vec![Value::keyword(":foo"), Value::fixnum(7)]),
            Value::symbol("unix"),
        ],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();

    let info = m
        .get("vm-charset-coding")
        .expect("defined coding system should exist");
    assert_eq!(resolve_sym(info.coding_type), "charset");
    assert_eq!(
        info.charset_list
            .iter()
            .map(|id| resolve_sym(*id))
            .collect::<Vec<_>>(),
        vec!["ascii", "unicode"]
    );
    assert_eq!(
        info.post_read_conversion.map(resolve_sym),
        Some("post-read-fn")
    );
    assert_eq!(
        info.pre_write_conversion.map(resolve_sym),
        Some("pre-write-fn")
    );
    assert!(info.properties.contains_key(&intern(":foo")));
}

#[test]
fn define_coding_system_internal_nil_conversion_slots_match_gnu() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    builtin_define_coding_system_internal(
        &mut m,
        vec![
            Value::symbol("vm-nil-conversion-coding"),
            Value::char('N'),
            Value::symbol("charset"),
            Value::list(vec![Value::symbol("ascii")]),
            Value::T,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum('?' as i64),
            Value::NIL,
            Value::NIL,
            Value::symbol("unix"),
        ],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();

    let info = m
        .get("vm-nil-conversion-coding")
        .expect("defined coding system should exist");
    assert_eq!(info.post_read_conversion, None);
    assert_eq!(info.pre_write_conversion, None);
    assert_eq!(
        builtin_coding_system_get(
            &m,
            vec![
                Value::symbol("vm-nil-conversion-coding"),
                Value::keyword(":post-read-conversion"),
            ],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_coding_system_get(
            &m,
            vec![
                Value::symbol("vm-nil-conversion-coding"),
                Value::keyword(":pre-write-conversion"),
            ],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::NIL
    );
}

#[test]
fn coding_system_base_with_suffix() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_base(
        &m,
        vec![Value::symbol("utf-8-unix")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_bare_symbol_named("utf-8"));
}

#[test]
fn coding_system_base_without_suffix() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_base(
        &m,
        vec![Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_bare_symbol_named("utf-8"));
}

#[test]
fn coding_system_base_unknown_still_strips() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_base(
        &m,
        vec![Value::symbol("foo-bar-unix")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    assert!(result.is_err());
}

#[test]
fn eol_type_unix() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_eol_type(
        &m,
        &vec![Value::symbol("utf-8-unix")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(eq_value(&result, &Value::fixnum(0)));
}

#[test]
fn eol_type_dos() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_eol_type(
        &m,
        &vec![Value::symbol("utf-8-dos")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(eq_value(&result, &Value::fixnum(1)));
}

#[test]
fn eol_type_mac() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_eol_type(
        &m,
        &vec![Value::symbol("utf-8-mac")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(eq_value(&result, &Value::fixnum(2)));
}

#[test]
fn eol_type_undecided_returns_vector() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_eol_type(
        &m,
        &vec![Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    // Should be a vector of [utf-8-unix utf-8-dos utf-8-mac]
    if result.is_vector() {
        let locked = result.as_vector_data().unwrap().clone();
        assert_eq!(locked.len(), 3);
        assert!(locked[0].is_bare_symbol_named("utf-8-unix"));
        assert!(locked[1].is_bare_symbol_named("utf-8-dos"));
        assert!(locked[2].is_bare_symbol_named("utf-8-mac"));
    } else {
        panic!("expected vector for undecided eol-type");
    }
}

#[test]
fn eol_type_latin_alias_uses_iso_latin_display_variants() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_eol_type(
        &m,
        &vec![Value::symbol("latin-1")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    if result.is_vector() {
        let locked = result.as_vector_data().unwrap().clone();
        assert_eq!(locked.len(), 3);
        assert_eq!(locked[0], Value::symbol("iso-latin-1-unix"));
        assert_eq!(locked[1], Value::symbol("iso-latin-1-dos"));
        assert_eq!(locked[2], Value::symbol("iso-latin-1-mac"));
    } else {
        panic!("expected vector for undecided latin-1 eol-type");
    }
}

#[test]
fn eol_type_gbk_alias_uses_chinese_gbk_display_variants() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_eol_type(
        &m,
        &vec![Value::symbol("gbk")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    if result.is_vector() {
        let locked = result.as_vector_data().unwrap().clone();
        assert_eq!(locked.len(), 3);
        assert_eq!(locked[0], Value::symbol("chinese-gbk-unix"));
        assert_eq!(locked[1], Value::symbol("chinese-gbk-dos"));
        assert_eq!(locked[2], Value::symbol("chinese-gbk-mac"));
    } else {
        panic!("expected vector for undecided gbk eol-type");
    }
}

#[test]
fn eol_type_nil_maps_to_no_conversion() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_eol_type(
        &m,
        &vec![Value::NIL],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(result, Value::fixnum(0));
}

#[test]
fn eol_type_non_symbol_designator_returns_nil() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    assert!(
        builtin_coding_system_eol_type(
            &m,
            &vec![Value::string("utf-8")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap()
        .is_nil()
    );
    assert!(
        builtin_coding_system_eol_type(
            &m,
            &vec![Value::fixnum(1)],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap()
        .is_nil()
    );
}

#[test]
fn eol_type_unknown_returns_nil() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_eol_type(
        &m,
        &vec![Value::symbol("nonexistent")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn write_coding_class_distinguishes_raw_text_types() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    for name in ["raw-text", "raw-text-unix", "no-conversion", "binary"] {
        assert_eq!(
            classify_write_coding(&m, name),
            WriteCodingClass::RawText,
            "{name} must be byte-faithful for the write path"
        );
    }
    for name in ["utf-8", "utf-8-unix", "latin-1", "undecided"] {
        assert_eq!(
            classify_write_coding(&m, name),
            WriteCodingClass::Selectable,
            "{name} must go through the safety selector"
        );
    }
}

#[test]
fn coding_system_type_utf8() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_type(
        &m,
        vec![Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_bare_symbol_named("utf-8"));
}

#[test]
fn coding_system_type_raw_text() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_type(
        &m,
        vec![Value::symbol("raw-text")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_bare_symbol_named("raw-text"));
}

#[test]
fn coding_system_type_unknown() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_type(
        &m,
        vec![Value::symbol("bogus")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    assert!(result.is_err());
}

#[test]
fn change_eol_by_int() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("utf-8"), Value::fixnum(1)],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_bare_symbol_named("utf-8-dos"));
}

#[test]
fn change_eol_by_symbol() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("utf-8-unix"), Value::symbol("mac")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_bare_symbol_named("utf-8-mac"));
}

#[test]
fn change_eol_strips_existing_suffix() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("utf-8-dos"), Value::fixnum(0)],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_bare_symbol_named("utf-8-unix"));
}

#[test]
fn change_eol_gbk_alias_returns_canonical_chinese_gbk_variant() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("gbk"), Value::symbol("unix")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(result, Value::symbol("chinese-gbk-unix"));
}

#[test]
fn change_eol_gbk_alias_variant_to_nil_returns_canonical_base() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("gbk-unix"), Value::NIL],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(result, Value::symbol("chinese-gbk"));
}

#[test]
fn change_eol_same_fixed_eol_returns_original_designator_like_gnu() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("latin-1-unix"), Value::fixnum(0)],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(result, Value::symbol("latin-1-unix"));
}

#[test]
fn change_eol_fixed_eol_accepts_equal_float_like_gnu() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("utf-8-unix"), Value::make_float(0.0)],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(result, Value::symbol("utf-8-unix"));

    let result = builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("utf-8-dos"), Value::make_float(1.0)],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(result, Value::symbol("utf-8-dos"));
}

#[test]
fn change_eol_nonmatching_float_uses_gnu_aref_error() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    match builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("utf-8-unix"), Value::make_float(1.0)],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .kinded()
    {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("fixnump"), Value::make_float(1.0)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn change_eol_out_of_range_uses_gnu_aref_error() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    match builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("utf-8"), Value::fixnum(3)],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .kinded()
    {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data.len(), 2);
            assert_eq!(sig.data[1], Value::fixnum(3));
        }
        other => panic!("expected args-out-of-range signal, got {other:?}"),
    }
}

#[test]
fn change_text_conversion_preserves_eol() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_text_conversion(
        &m,
        vec![Value::symbol("utf-8-unix"), Value::symbol("latin-1")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_bare_symbol_named("iso-latin-1-unix"));
}

#[test]
fn change_text_conversion_undecided_eol() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_text_conversion(
        &m,
        vec![Value::symbol("utf-8"), Value::symbol("latin-1")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    // utf-8 has undecided eol -> no suffix
    assert!(result.is_bare_symbol_named("latin-1"));
}

#[test]
fn detect_coding_string_highest() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_detect_coding_string(
        &m,
        vec![Value::string("hello"), Value::T],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_bare_symbol_named("undecided"));
}

#[test]
fn detect_coding_string_list() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_detect_coding_string(
        &m,
        vec![Value::string("hello")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].is_bare_symbol_named("undecided"));
}

#[test]
fn detect_coding_string_wrong_type() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_detect_coding_string(
        &m,
        vec![Value::fixnum(42)],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    assert!(result.is_err());
}

#[test]
fn detect_coding_string_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_detect_coding_string(
        &m,
        vec![Value::string("x"), Value::NIL, Value::NIL],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    assert!(result.is_err());
}

#[test]
fn keyboard_coding_system_default() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_keyboard_coding_system(&m, vec![]).unwrap();
    assert!(result.is_bare_symbol_named("no-conversion"));
}

#[test]
fn terminal_coding_system_default() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_terminal_coding_system(&m, vec![]).unwrap();
    assert!(result.is_bare_symbol_named("undecided"));
}

#[test]
fn coding_system_getters_validate_max_arity() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    assert!(builtin_keyboard_coding_system(&m, vec![Value::NIL]).is_ok());
    assert!(builtin_terminal_coding_system(&m, vec![Value::NIL]).is_ok());
    assert!(builtin_keyboard_coding_system(&m, vec![Value::NIL, Value::NIL]).is_err());
    assert!(builtin_terminal_coding_system(&m, vec![Value::NIL, Value::NIL]).is_err());
}

#[test]
fn set_keyboard_coding_system() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let set = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("latin-1")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(set.is_bare_symbol_named("iso-latin-1-unix"));
    let get = builtin_keyboard_coding_system(&m, vec![]).unwrap();
    assert!(get.is_bare_symbol_named("iso-latin-1-unix"));
}

#[test]
fn set_keyboard_coding_system_canonicalizes_non_unix_alias_suffixes() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();

    let latin_dos = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("latin-1-dos")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(latin_dos, Value::symbol("iso-latin-1-unix"));

    let latin_mac = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("latin-1-mac")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(latin_mac, Value::symbol("iso-latin-1-unix"));

    let iso_dos = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("iso-8859-1-dos")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(iso_dos, Value::symbol("iso-latin-1-unix"));

    let ascii_dos = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("ascii-dos")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(ascii_dos, Value::symbol("us-ascii-unix"));

    let ascii_mac = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("ascii-mac")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(ascii_mac, Value::symbol("us-ascii-unix"));
}

#[test]
fn set_keyboard_coding_system_preserves_explicit_unix_spelling() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();

    let latin_unix = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("latin-1-unix")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(latin_unix, Value::symbol("latin-1-unix"));

    let iso_unix = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("iso-8859-1-unix")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(iso_unix, Value::symbol("iso-8859-1-unix"));

    let ascii_unix = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("ascii-unix")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(ascii_unix, Value::symbol("ascii-unix"));
}

#[test]
fn coding_system_change_eol_conversion_canonicalizes_alias_families() {
    crate::test_utils::init_test_tracing();
    let m = mgr();

    assert_eq!(
        builtin_coding_system_change_eol_conversion(
            &m,
            vec![Value::symbol("latin-1"), Value::fixnum(0)],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::symbol("iso-latin-1-unix")
    );
    assert_eq!(
        builtin_coding_system_change_eol_conversion(
            &m,
            vec![Value::symbol("latin-1-unix"), Value::NIL],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::symbol("iso-latin-1")
    );
    assert_eq!(
        builtin_coding_system_change_eol_conversion(
            &m,
            vec![Value::symbol("latin-1-unix"), Value::fixnum(1)],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::symbol("iso-latin-1-dos")
    );
}

#[test]
fn coding_system_change_eol_conversion_canonicalizes_latin9_alias_family() {
    crate::test_utils::init_test_tracing();
    let m = mgr_with_latin9();

    assert_eq!(
        builtin_coding_system_change_eol_conversion(
            &m,
            vec![Value::symbol("iso-8859-15"), Value::fixnum(0)],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::symbol("iso-latin-9-unix")
    );
    assert_eq!(
        builtin_coding_system_change_eol_conversion(
            &m,
            vec![Value::symbol("iso-8859-15-unix"), Value::NIL],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::symbol("iso-latin-9")
    );
    assert_eq!(
        builtin_coding_system_base(
            &m,
            vec![Value::symbol("iso-8859-15-unix")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::symbol("iso-latin-9")
    );
}

#[test]
fn set_keyboard_coding_system_normalizes_latin9_alias_family() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr_with_latin9();

    let set = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("iso-8859-15")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(set, Value::symbol("iso-latin-9-unix"));

    let get = builtin_keyboard_coding_system(&m, vec![]).unwrap();
    assert_eq!(get, Value::symbol("iso-latin-9-unix"));
}

#[test]
fn set_keyboard_coding_system_accepts_alias_derived_variants() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();

    let latin_unix = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("latin-1-unix")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(latin_unix, Value::symbol("latin-1-unix"));

    let latin_dos = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("latin-1-dos")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(latin_dos, Value::symbol("iso-latin-1-unix"));
}

#[test]
fn set_terminal_coding_system_accepts_alias_derived_variants() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();

    assert!(
        builtin_set_terminal_coding_system(
            &mut m,
            vec![Value::symbol("latin-1-unix")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_ok()
    );
    assert_eq!(
        builtin_terminal_coding_system(&m, vec![]).unwrap(),
        Value::symbol("latin-1-unix")
    );
}

#[test]
fn set_terminal_coding_system() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let set = builtin_set_terminal_coding_system(
        &mut m,
        vec![Value::symbol("ascii")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(set.is_nil());
    let get = builtin_terminal_coding_system(&m, vec![]).unwrap();
    assert!(get.is_bare_symbol_named("ascii"));
}

#[test]
fn set_keyboard_coding_nil_resets_to_no_conversion() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("latin-1")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::NIL],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    let result = builtin_keyboard_coding_system(&m, vec![]).unwrap();
    assert!(result.is_bare_symbol_named("no-conversion"));
}

#[test]
fn set_terminal_coding_nil_sets_nil_symbol() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    builtin_set_terminal_coding_system(
        &mut m,
        vec![Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    builtin_set_terminal_coding_system(
        &mut m,
        vec![Value::NIL],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    let result = builtin_terminal_coding_system(&m, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn coding_system_setters_validate_symbol_and_known_names() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    assert!(
        builtin_set_keyboard_coding_system(
            &mut m,
            vec![Value::string("utf-8")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );
    assert!(
        builtin_set_terminal_coding_system(
            &mut m,
            vec![Value::string("utf-8")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );
    assert!(
        builtin_set_keyboard_coding_system(
            &mut m,
            vec![Value::symbol("no-such-coding")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );
    assert!(
        builtin_set_terminal_coding_system(
            &mut m,
            vec![Value::symbol("no-such-coding")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );
}

#[test]
fn coding_system_setters_treat_keywords_as_symbol_designators() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let keyword = Value::keyword(":utf-8");
    let kb = builtin_set_keyboard_coding_system(
        &mut m,
        vec![keyword],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    let term = builtin_set_terminal_coding_system(
        &mut m,
        vec![keyword],
        neovm_value::tagged::value::SymbolMode::Bare,
    );

    match kb.kinded() {
        Err(FlowKind::Signal(sig)) => assert_eq!(sig.symbol_name(), "coding-system-error"),
        other => panic!("expected coding-system-error for keyword keyboard set, got {other:?}"),
    }
    match term.kinded() {
        Err(FlowKind::Signal(sig)) => assert_eq!(sig.symbol_name(), "coding-system-error"),
        other => panic!("expected coding-system-error for keyword terminal set, got {other:?}"),
    }
}

#[test]
fn coding_system_setters_validate_arity_edges() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    assert!(
        builtin_set_keyboard_coding_system(
            &mut m,
            vec![Value::NIL, Value::NIL],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_ok()
    );
    assert!(
        builtin_set_keyboard_coding_system(
            &mut m,
            vec![Value::NIL, Value::NIL, Value::NIL],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );

    assert!(
        builtin_set_terminal_coding_system(
            &mut m,
            vec![Value::NIL, Value::NIL],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_ok()
    );
    assert!(
        builtin_set_terminal_coding_system(
            &mut m,
            vec![Value::NIL, Value::NIL, Value::NIL],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_ok()
    );
    assert!(
        builtin_set_terminal_coding_system(
            &mut m,
            vec![Value::NIL, Value::NIL, Value::NIL, Value::NIL],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );
}

#[test]
fn priority_list_full() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_priority_list(&m, vec![]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert!(!items.is_empty());
    // First should be utf-8
    assert!(items[0].is_bare_symbol_named("utf-8"));
}

#[test]
fn priority_list_highest() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    // GNU `(coding-system-priority-list t)` returns a BARE symbol (the base name
    // of the highest-priority category), not a one-element list.
    let result = builtin_coding_system_priority_list(&m, vec![Value::T]).unwrap();
    assert!(result.is_bare_symbol_named("utf-8"));
    assert!(
        list_to_vec(&result).is_none(),
        "HIGHESTP result must be a bare symbol, not a list"
    );
}

#[test]
fn priority_list_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_priority_list(&m, vec![Value::NIL, Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn eol_type_to_int() {
    crate::test_utils::init_test_tracing();
    assert_eq!(EolType::Unix.to_int(), 0);
    assert_eq!(EolType::Dos.to_int(), 1);
    assert_eq!(EolType::Mac.to_int(), 2);
    assert_eq!(EolType::Undecided.to_int(), 0);
}

#[test]
fn eol_type_from_suffix() {
    crate::test_utils::init_test_tracing();
    assert_eq!(EolType::from_suffix("utf-8-unix"), Some(EolType::Unix));
    assert_eq!(EolType::from_suffix("utf-8-dos"), Some(EolType::Dos));
    assert_eq!(EolType::from_suffix("utf-8-mac"), Some(EolType::Mac));
    assert_eq!(EolType::from_suffix("utf-8"), None);
}

#[test]
fn strip_eol_suffix_works() {
    crate::test_utils::init_test_tracing();
    assert_eq!(strip_eol_suffix("utf-8-unix"), "utf-8");
    assert_eq!(strip_eol_suffix("utf-8-dos"), "utf-8");
    assert_eq!(strip_eol_suffix("utf-8-mac"), "utf-8");
    assert_eq!(strip_eol_suffix("utf-8"), "utf-8");
    assert_eq!(strip_eol_suffix("latin-1"), "latin-1");
}

#[test]
fn coding_system_get_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_get(
        &m,
        vec![Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    assert!(result.is_err());
}

#[test]
fn coding_system_base_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result =
        builtin_coding_system_base(&m, vec![], neovm_value::tagged::value::SymbolMode::Bare);
    assert!(result.is_err());
}

#[test]
fn coding_system_aliases_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result =
        builtin_coding_system_aliases(&m, vec![], neovm_value::tagged::value::SymbolMode::Bare);
    assert!(result.is_err());
}

#[test]
fn coding_system_p_reads_runtime_aliases() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let before = builtin_coding_system_p(
        &m,
        vec![Value::symbol("vm-utf8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(before.is_nil());

    builtin_define_coding_system_alias(
        &mut m,
        vec![Value::symbol("vm-utf8"), Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    let after = builtin_coding_system_p(
        &m,
        vec![Value::symbol("vm-utf8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(after.is_truthy());
}

#[test]
fn coding_system_p_accepts_nil_and_supported_derived_variants() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    assert!(
        builtin_coding_system_p(
            &m,
            vec![Value::NIL],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap()
        .is_truthy()
    );
    assert!(
        builtin_coding_system_p(
            &m,
            vec![Value::symbol("ascii-dos")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap()
        .is_truthy()
    );
}

#[test]
fn check_coding_system_signals_unknown_symbols() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_check_coding_system(
        &m,
        vec![Value::symbol("vm-no-such")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    match result.kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "coding-system-error");
            assert_eq!(sig.data, vec![Value::symbol("vm-no-such")]);
        }
        other => panic!("expected coding-system-error signal, got {other:?}"),
    }
}

#[test]
fn check_coding_system_accepts_supported_derived_variants() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    assert_eq!(
        builtin_check_coding_system(
            &m,
            vec![Value::symbol("latin-1-unix")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::symbol("latin-1-unix")
    );
    assert_eq!(
        builtin_check_coding_system(
            &m,
            vec![Value::symbol("ascii-unix")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::symbol("ascii-unix")
    );
    assert_eq!(
        builtin_check_coding_system(
            &m,
            vec![Value::symbol("undecided-unix")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::symbol("undecided-unix")
    );
    assert_eq!(
        builtin_check_coding_system(
            &m,
            vec![Value::symbol("utf-8-auto-unix")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::symbol("utf-8-auto-unix")
    );
    assert_eq!(
        builtin_check_coding_system(
            &m,
            vec![Value::symbol("prefer-utf-8-unix")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::symbol("prefer-utf-8-unix")
    );
    assert_eq!(
        builtin_check_coding_system(
            &m,
            vec![Value::symbol("gbk-unix")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::symbol("gbk-unix")
    );
}

#[test]
fn check_coding_system_rejects_unsupported_derived_variants() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    assert!(
        builtin_check_coding_system(
            &m,
            vec![Value::symbol("no-conversion-unix")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );
    assert!(
        builtin_check_coding_system(
            &m,
            vec![Value::symbol("binary-unix")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );
    // emacs-internal is an alias for utf-8-emacs-unix, so
    // emacs-internal-unix correctly resolves to utf-8-emacs-unix
    // (the base is utf-8-emacs-unix, strip suffix → utf-8-emacs, derive
    // unix → utf-8-emacs-unix).
}

#[test]
fn set_keyboard_coding_system_rejects_unsuitable_variants() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let auto = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("utf-8-auto")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    let auto_derived = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("utf-8-auto-unix")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    let prefer = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("prefer-utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    let prefer_derived = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("prefer-utf-8-unix")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    let undecided = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("undecided")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    let undecided_derived = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("undecided-unix")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );

    assert!(auto.is_err());
    assert!(auto_derived.is_err());
    assert!(prefer.is_err());
    assert!(prefer_derived.is_err());
    assert!(undecided.is_err());
    assert!(undecided_derived.is_err());
}

#[test]
fn set_keyboard_coding_system_preserves_emacs_internal() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let set = builtin_set_keyboard_coding_system(
        &mut m,
        vec![Value::symbol("emacs-internal")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert_eq!(set, Value::symbol("emacs-internal"));

    let get = builtin_keyboard_coding_system(&m, vec![]).unwrap();
    assert_eq!(get, Value::symbol("emacs-internal"));
}

#[test]
fn find_coding_system_known_and_unknown() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let known = builtin_find_coding_system(&m, vec![Value::symbol("utf-8")]).unwrap();
    assert_eq!(known, Value::symbol("utf-8"));

    let unknown = builtin_find_coding_system(&m, vec![Value::symbol("vm-no-such-coding")]).unwrap();
    assert_eq!(unknown, Value::NIL);
}

#[test]
fn set_coding_system_priority_reorders_front_in_arg_order() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    builtin_set_coding_system_priority(
        &mut m,
        vec![Value::symbol("raw-text"), Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();

    let list = builtin_coding_system_priority_list(&m, vec![]).unwrap();
    let items = list_to_vec(&list).unwrap();
    assert!(items[0].is_bare_symbol_named("raw-text"));
    assert!(items[1].is_bare_symbol_named("utf-8"));
}

#[test]
fn set_coding_system_priority_rejects_nil_payload() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let result = builtin_set_coding_system_priority(
        &mut m,
        vec![Value::NIL],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    match result.kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("coding-system-p"), Value::NIL]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn set_coding_system_priority_keyword_signals_coding_system_error() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let result = builtin_set_coding_system_priority(
        &mut m,
        vec![Value::keyword(":utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    match result.kinded() {
        Err(FlowKind::Signal(sig)) => assert_eq!(sig.symbol_name(), "coding-system-error"),
        other => panic!("expected coding-system-error signal, got {other:?}"),
    }
}

#[test]
fn set_coding_system_priority_string_is_type_error() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let result = builtin_set_coding_system_priority(
        &mut m,
        vec![Value::string("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    match result.kinded() {
        Err(FlowKind::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn internal_coding_system_setters_match_surface_validation() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    assert_eq!(
        builtin_set_keyboard_coding_system_internal(
            &mut m,
            vec![Value::symbol("utf-8")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_set_terminal_coding_system_internal(
            &mut m,
            vec![Value::symbol("utf-8")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_set_safe_terminal_coding_system_internal(
            &mut m,
            vec![Value::symbol("utf-8")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .unwrap(),
        Value::NIL
    );
    assert!(
        builtin_set_keyboard_coding_system_internal(
            &mut m,
            vec![Value::symbol("foo")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );
    assert!(
        builtin_set_terminal_coding_system_internal(
            &mut m,
            vec![Value::symbol("foo")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );
    assert!(
        builtin_set_safe_terminal_coding_system_internal(
            &mut m,
            vec![Value::symbol("foo")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );
}

#[test]
fn text_quoting_style_domain_matches_gnu_symbols() {
    for style in [
        TextQuotingStyle::Grave,
        TextQuotingStyle::Straight,
        TextQuotingStyle::Curve,
    ] {
        let name = style.symbol_name();
        assert_eq!(
            TextQuotingStyle::from_symbol_value(
                Value::symbol(name),
                neovm_value::tagged::value::SymbolMode::Bare
            ),
            Some(style)
        );
        assert_eq!(style.to_symbol(), Value::symbol(name));
    }
    assert_eq!(
        TextQuotingStyle::from_symbol_value(
            Value::symbol("Curve"),
            neovm_value::tagged::value::SymbolMode::Bare
        ),
        None
    );
    assert_eq!(
        TextQuotingStyle::from_symbol_value(
            Value::symbol("nil"),
            neovm_value::tagged::value::SymbolMode::Bare
        ),
        None
    );
    assert_eq!(
        TextQuotingStyle::from_symbol_value(
            Value::NIL,
            neovm_value::tagged::value::SymbolMode::Bare
        ),
        None
    );
}

#[test]
fn detect_matches_gnu_ascii() {
    crate::test_utils::init_test_tracing();
    // (detect-coding-string (unibyte-string 65 66 67)) => (undecided)
    assert_eq!(detect_list(&[65, 66, 67]), vec!["undecided"]);
}

#[test]
fn detect_matches_gnu_utf8_bom() {
    crate::test_utils::init_test_tracing();
    // (detect-coding-string (unibyte-string 239 187 191 97))
    assert_eq!(
        detect_list(&[239, 187, 191, 97]),
        vec![
            "utf-8",
            "iso-latin-1",
            "emacs-mule",
            "in-is13194-devanagari",
            "utf-8-auto",
            "utf-8-with-signature",
            "japanese-shift-jis",
            "chinese-big5",
            "iso-2022-8bit-ss2",
        ]
    );
}

#[test]
fn detect_matches_gnu_utf16be_bom_null() {
    crate::test_utils::init_test_tracing();
    // (detect-coding-string (unibyte-string 254 255 0 65)) => (no-conversion)
    assert_eq!(detect_list(&[254, 255, 0, 65]), vec!["no-conversion"]);
}

/// NUL may precede or follow UTF-8; suppressing its binary classification
/// preserves the character detector's ordinary priority in either position.
#[test]
fn detect_null_byte_inhibition_preserves_utf8() {
    crate::test_utils::init_test_tracing();
    let bindings = gnu_detect_state();
    for bytes in [
        b"\0\xe7\xa1\xac\n".as_slice(),
        b"\xe7\xa1\xac\0\n".as_slice(),
    ] {
        for (inhibit_null, expected) in [(false, "no-conversion"), (true, "utf-8")] {
            let detected = detect_categories(
                &bindings,
                bytes,
                bytes.len(),
                false,
                true,
                SourceBlock::Last,
                inhibit_null,
            );
            assert!(detected.is_bare_symbol_named(expected));
        }
    }
}

#[test]
fn detect_matches_gnu_lone_high_byte() {
    crate::test_utils::init_test_tracing();
    // (detect-coding-string (unibyte-string 255))
    assert_eq!(
        detect_list(&[255]),
        vec![
            "iso-latin-1",
            "emacs-mule",
            "in-is13194-devanagari",
            "chinese-iso-8bit",
            "iso-2022-8bit-ss2",
        ]
    );
}

#[test]
fn detect_matches_gnu_valid_utf8() {
    crate::test_utils::init_test_tracing();
    // (detect-coding-string (unibyte-string 99 97 102 195 169))  "café"
    assert_eq!(
        detect_list(&[99, 97, 102, 195, 169]),
        vec![
            "utf-8",
            "iso-latin-1",
            "emacs-mule",
            "in-is13194-devanagari",
            "chinese-iso-8bit",
            "utf-8-auto",
            "japanese-shift-jis",
            "chinese-big5",
            "iso-2022-8bit-ss2",
        ]
    );
}

#[test]
fn detect_matches_gnu_latin1_high() {
    crate::test_utils::init_test_tracing();
    // (detect-coding-string (unibyte-string 99 97 102 233))
    assert_eq!(
        detect_list(&[99, 97, 102, 233]),
        vec![
            "iso-latin-1",
            "emacs-mule",
            "in-is13194-devanagari",
            "chinese-iso-8bit",
            "iso-2022-8bit-ss2",
        ]
    );
}

#[test]
fn detect_matches_gnu_binary_null() {
    crate::test_utils::init_test_tracing();
    // (detect-coding-string (unibyte-string 255 254 0 0)) => (no-conversion)
    assert_eq!(detect_list(&[255, 254, 0, 0]), vec!["no-conversion"]);
}

#[test]
fn detect_matches_gnu_utf8_bom_then_ascii() {
    crate::test_utils::init_test_tracing();
    // (detect-coding-string (unibyte-string 239 187 191 65 66 67))
    assert_eq!(
        detect_list(&[239, 187, 191, 65, 66, 67]),
        vec![
            "utf-8",
            "iso-latin-1",
            "emacs-mule",
            "in-is13194-devanagari",
            "utf-8-auto",
            "utf-8-with-signature",
            "japanese-shift-jis",
            "chinese-big5",
            "iso-2022-8bit-ss2",
        ]
    );
}

#[test]
fn detect_matches_gnu_mb_string_with_bom_chars() {
    crate::test_utils::init_test_tracing();
    // (detect-coding-string (string #xef #xbb #xbf 65))
    // multibyte bytes = [195 175 194 187 194 191 65], chars = 4.
    assert_eq!(
        detect_list_mb(&[195, 175, 194, 187, 194, 191, 65], 4, true),
        vec!["utf-8", "utf-8-auto", "iso-2022-7bit"]
    );
}

#[test]
fn detect_matches_gnu_mb_cafe() {
    crate::test_utils::init_test_tracing();
    // (detect-coding-string "café")  multibyte bytes = [99 97 102 195 169], chars = 4.
    assert_eq!(
        detect_list_mb(&[99, 97, 102, 195, 169], 4, true),
        vec!["utf-8", "utf-8-auto", "iso-2022-7bit"]
    );
}

#[test]
fn detect_matches_gnu_mb_utf16_chars_with_null() {
    crate::test_utils::init_test_tracing();
    // (detect-coding-string (string #xff #xfe 65 0))
    // multibyte bytes = [195 191 195 190 65 0], chars = 4 -> (no-conversion).
    assert_eq!(
        detect_list_mb(&[195, 191, 195, 190, 65, 0], 4, true),
        vec!["no-conversion"]
    );
    // (string #xff #xfe #x00) -> [195 191 195 190 0], chars = 3.
    assert_eq!(
        detect_list_mb(&[195, 191, 195, 190, 0], 3, true),
        vec!["no-conversion"]
    );
}

#[test]
fn priority_list_has_twenty_entries_like_gnu() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let v = builtin_coding_system_priority_list(&m, vec![]).unwrap();
    assert_eq!(list_to_vec(&v).unwrap().len(), 20);
}

#[test]
fn set_priority_is_idempotent_across_repeated_calls() {
    crate::test_utils::init_test_tracing();
    // NOTE: in the bare unit-test coding manager, the runtime-defined systems
    // (utf-8-auto, utf-16, iso-2022-7bit, ...) carry no verbatim plist, so
    // `coding_category_of` falls back to the per-base mapping and several
    // collapse onto the same category.  The absolute length therefore differs
    // from the booted runtime's 20.  What we *can* verify here is the algorithm
    // itself: preferring a coding system, then preferring it again, leaves the
    // priority list unchanged (no growth, idempotent) and moves it to the head.
    let mut m = mgr();
    builtin_set_coding_system_priority(
        &mut m,
        vec![Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    let once = list_to_vec(&builtin_coding_system_priority_list(&m, vec![]).unwrap()).unwrap();
    builtin_set_coding_system_priority(
        &mut m,
        vec![Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    let twice = list_to_vec(&builtin_coding_system_priority_list(&m, vec![]).unwrap()).unwrap();
    assert_eq!(
        once.len(),
        twice.len(),
        "re-preferring must not grow the list"
    );
    assert_eq!(
        resolve_sym(once[0].as_bare_symbol_id().unwrap()),
        "utf-8",
        "preferred system moves to the head"
    );
    assert_eq!(resolve_sym(twice[0].as_bare_symbol_id().unwrap()), "utf-8");
}

#[test]
fn set_priority_moves_charset_category_to_front() {
    crate::test_utils::init_test_tracing();
    // iso-latin-1 is registered in the bare manager (charset category); fronting
    // it then fronting utf-8 must keep utf-8 first and not grow the list.
    let mut m = mgr();
    let base = list_to_vec(&builtin_coding_system_priority_list(&m, vec![]).unwrap())
        .unwrap()
        .len();
    builtin_set_coding_system_priority(
        &mut m,
        vec![Value::symbol("iso-latin-1")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    // HIGHESTP returns a bare symbol (GNU `CODING_ATTR_BASE_NAME`).
    let after_latin = builtin_coding_system_priority_list(&m, vec![Value::T]).unwrap();
    assert_eq!(
        resolve_sym(after_latin.as_bare_symbol_id().unwrap()),
        "iso-latin-1"
    );
    builtin_set_coding_system_priority(
        &mut m,
        vec![Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    let after_utf8 =
        list_to_vec(&builtin_coding_system_priority_list(&m, vec![]).unwrap()).unwrap();
    assert_eq!(
        resolve_sym(after_utf8[0].as_bare_symbol_id().unwrap()),
        "utf-8"
    );
    // Fronting two distinct categories one at a time never grows the list.
    assert!(after_utf8.len() <= base);
}

#[test]
fn encode_lisp_string_prepends_bom_for_signature() {
    crate::test_utils::init_test_tracing();
    let s = neovm_value::heap_types::LispString::from_unibyte(b"x".to_vec());
    let bytes = crate::encoding::encode_lisp_string(
        &s,
        "utf-8-with-signature",
        crate::emacs_core::coding::EolConversion::Enabled,
    );
    assert_eq!(bytes, vec![0xEF, 0xBB, 0xBF, b'x']);
    // Plain utf-8 must NOT prepend a BOM.
    let plain = crate::encoding::encode_lisp_string(
        &s,
        "utf-8",
        crate::emacs_core::coding::EolConversion::Enabled,
    );
    assert_eq!(plain, vec![b'x']);
}

#[test]
fn priority_list_keeps_chinese_iso_8bit_and_big5_after_charset_front() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    assert_eq!(m.priority.len(), 20, "fresh new() should have 20 entries");

    // The three charset-family entries must each have GNU's category: only
    // iso-latin-1 is `coding-category-charset`; the two Chinese systems are
    // distinct (`iso-8-2` and `big5`).
    assert_eq!(
        coding_category_of(
            &m,
            "iso-latin-1",
            neovm_value::tagged::value::SymbolMode::Bare
        ),
        Some("coding-category-charset")
    );
    assert_eq!(
        coding_category_of(
            &m,
            "chinese-iso-8bit",
            neovm_value::tagged::value::SymbolMode::Bare
        ),
        Some("coding-category-iso-8-2")
    );
    assert_eq!(
        coding_category_of(
            &m,
            "chinese-big5",
            neovm_value::tagged::value::SymbolMode::Bare
        ),
        Some("coding-category-big5")
    );

    // Fronting the charset category (as `reset-language-environment` does via
    // `iso-latin-1`) must not evict the distinct-category Chinese systems.
    builtin_set_coding_system_priority(
        &mut m,
        vec![Value::symbol("iso-latin-1")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    let names: Vec<&str> = m.priority.iter().map(|&s| resolve_sym(s)).collect();
    assert!(
        names.contains(&"chinese-iso-8bit"),
        "chinese-iso-8bit dropped! names={names:?}"
    );
    assert!(
        names.contains(&"chinese-big5"),
        "chinese-big5 dropped! names={names:?}"
    );
    assert_eq!(m.priority.len(), 20, "list must still have 20 entries");
    assert_eq!(names[0], "iso-latin-1", "fronted charset system first");
}

#[test]
fn coding_category_priority_list_has_all_21_categories_in_order() {
    crate::test_utils::init_test_tracing();
    // NOTE: this exercises `coding_category_priority_list` on a *fresh*
    // `CodingSystemManager`, where the iso-2022/utf-16 systems are not yet
    // fully specified (their full :coding-type specs come from
    // `define-coding-system-internal` during loadup), so some entries resolve
    // to other categories than the booted runtime.  We therefore assert only
    // the structural invariants that hold regardless: the result always covers
    // all 21 categories exactly once, including the unbound `coding-category-ccl`
    // and the now-distinct `iso-8-2`/`big5` categories.  The exact GNU order is
    // verified end-to-end against the booted binary's `coding-category-list`.
    let m = mgr();
    let cats: Vec<&str> =
        coding_category_priority_list(&m, neovm_value::tagged::value::SymbolMode::Bare)
            .iter()
            .map(|&s| resolve_sym(s))
            .collect();
    assert_eq!(cats.len(), 21, "expected 21 categories, got {cats:?}");
    assert!(cats.contains(&"coding-category-ccl"), "ccl must be present");
    assert!(
        cats.contains(&"coding-category-iso-8-2"),
        "iso-8-2 (chinese-iso-8bit) present"
    );
    assert!(
        cats.contains(&"coding-category-big5"),
        "big5 (chinese-big5) present"
    );
    // No duplicates: every category appears exactly once.
    let mut sorted = cats.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 21, "categories must be unique: {cats:?}");
}

#[test]
fn insert_unbound_categories_matches_gnu_booted_order() {
    // Indices for the booted priority order: utf-8(7) iso-7(0) charset(14)
    // iso-7-else(4) iso-8-else(5) emacs-mule(18) raw-text(19) iso-7-tight(1)
    // iso-8-1(2) iso-8-2(3) utf-8-auto(6) utf-8-sig(8) utf-16-auto(9)
    // utf-16-be(10) utf-16-le(11) utf-16-be-nosig(12) utf-16-le-nosig(13)
    // sjis(15) big5(16) undecided(20).  Only ccl(17) is unbound/missing.
    let mut order = vec![
        7, 0, 14, 4, 5, 18, 19, 1, 2, 3, 6, 8, 9, 10, 11, 12, 13, 15, 16, 20,
    ];
    insert_unbound_categories(&mut order);
    // GNU inserts ccl(17) between big5(16) and undecided(20).
    assert_eq!(
        order,
        vec![
            7, 0, 14, 4, 5, 18, 19, 1, 2, 3, 6, 8, 9, 10, 11, 12, 13, 15, 16, 17, 20
        ],
        "ccl must be inserted just before undecided, matching GNU"
    );
}

/// DIVERGENCES.md entry 143: the two resolvers cannot be called without being
/// told what `inhibit-eol-conversion` holds, and `Inhibited` collapses BOTH
/// halves of GNU's `adjust_coding_eol_type` -- the eol type the conversion runs
/// with AND the name rewrite -- for every `EolType`, concrete or vector.
///
/// The point of the enum rather than a `bool` is the middle assertion: an
/// inhibited resolution is `DecodeEolResolution::Inhibited`, a state of its own
/// and not `NotSeen`, because `NotSeen` is a property of the TEXT (GNU's
/// `EOL_SEEN_NONE`) while this one is a property of the SESSION.
#[test]
fn eol_resolution_requires_being_told_about_inhibit_eol_conversion() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::coding::{DecodeEolResolution, EolConversion, EolType, ResolvedEol};

    let crlf = b"a\r\nb\r\n";
    let cr = b"a\rb\r";
    let none = b"abc";

    // ENABLED: GNU's three states, unchanged from entry 139.
    assert_eq!(
        EolType::Dos.resolve_for_decode(crlf, EolConversion::Enabled),
        DecodeEolResolution::Specified(ResolvedEol::Dos)
    );
    assert_eq!(
        EolType::Undecided.resolve_for_decode(crlf, EolConversion::Enabled),
        DecodeEolResolution::Adjusted(ResolvedEol::Dos)
    );
    assert_eq!(
        EolType::Undecided.resolve_for_decode(cr, EolConversion::Enabled),
        DecodeEolResolution::Adjusted(ResolvedEol::Mac)
    );
    assert_eq!(
        EolType::Undecided.resolve_for_decode(none, EolConversion::Enabled),
        DecodeEolResolution::NotSeen
    );

    // INHIBITED: `decode_eol` returns on its first line (src/coding.c:6767),
    // so every one of them answers the same thing, and it is neither
    // `Specified(Unix)` nor `NotSeen`.
    for eol in [
        EolType::Unix,
        EolType::Dos,
        EolType::Mac,
        EolType::Undecided,
    ] {
        for text in [&crlf[..], &cr[..], &none[..]] {
            let resolution = eol.resolve_for_decode(text, EolConversion::Inhibited);
            assert_eq!(resolution, DecodeEolResolution::Inhibited, "{eol:?}");
            // Converts nothing...
            assert_eq!(resolution.eol(), ResolvedEol::Unix, "{eol:?}");
            // ...and moves no name.
            assert_eq!(resolution.adjusted(), None, "{eol:?}");
        }
    }

    // The encode side is GNU's `inhibit_eol_conversion ? Qunix : ...`
    // (src/coding.c:7625): it never detected, so `Inhibited` only forces unix.
    assert_eq!(
        EolType::Dos.for_encode(EolConversion::Enabled),
        ResolvedEol::Dos
    );
    assert_eq!(
        EolType::Mac.for_encode(EolConversion::Enabled),
        ResolvedEol::Mac
    );
    for eol in [
        EolType::Unix,
        EolType::Dos,
        EolType::Mac,
        EolType::Undecided,
    ] {
        assert_eq!(
            eol.for_encode(EolConversion::Inhibited),
            ResolvedEol::Unix,
            "{eol:?}"
        );
    }
}

/// The accessors memoize per coding-system symbol, and every registry
/// writer drops the memo: an alias re-pointed at another system answers for
/// the new target, and a property written through `get_mut` shows up in the
/// next plist. Memoized plists are the same object call to call, as GNU's
/// `CODING_ATTR_PLIST` is.
#[test]
fn accessor_memo_follows_every_registry_change() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let utf8 = Value::symbol("utf-8");
    let first =
        builtin_coding_system_plist(&m, vec![utf8], neovm_value::tagged::value::SymbolMode::Bare)
            .expect("plist");
    let second =
        builtin_coding_system_plist(&m, vec![utf8], neovm_value::tagged::value::SymbolMode::Bare)
            .expect("plist");
    assert_eq!(first.bits(), second.bits(), "memoized: one object");
    assert_eq!(
        builtin_coding_system_base(
            &m,
            vec![Value::symbol("utf-8-unix")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .expect("base")
        .as_bare_symbol_name(),
        Some("utf-8")
    );

    m.add_alias("memo-test-alias", "utf-8");
    let alias = Value::symbol("memo-test-alias");
    assert_eq!(
        builtin_coding_system_base(
            &m,
            vec![alias],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .expect("base")
        .as_bare_symbol_name(),
        Some("utf-8")
    );
    m.add_alias("memo-test-alias", "iso-latin-1");
    assert_eq!(
        builtin_coding_system_base(
            &m,
            vec![alias],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .expect("base")
        .as_bare_symbol_name(),
        Some("iso-latin-1"),
        "re-pointed alias: the memo was dropped"
    );

    // `coding-system-put` is the Lisp-visible writer (it reaches the info
    // through `get_mut`, which drops the memo).
    let key = intern(":memo-test-prop");
    builtin_coding_system_put(
        &mut m,
        vec![
            utf8,
            Value::from_sym_id(key),
            Value::symbol("memo-test-value"),
        ],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .expect("put");
    let third =
        builtin_coding_system_plist(&m, vec![utf8], neovm_value::tagged::value::SymbolMode::Bare)
            .expect("plist");
    assert_ne!(first.bits(), third.bits(), "a new plist after the write");
    let items = neovm_value::value::list_to_vec(&third).expect("list");
    let at = items
        .iter()
        .position(|v| v.as_bare_symbol_id() == Some(key))
        .expect("the written property is in the plist");
    assert_eq!(items[at + 1].as_bare_symbol_name(), Some("memo-test-value"));
}

/// `coding-system-eol-type` of an undecided system is a vector, and GNU
/// hands back a fresh copy each time (`Fcopy_sequence`): the memo must not
/// share one, or an `aset` on the answer would leak into the next.
#[test]
fn eol_type_vector_answers_are_fresh_each_call() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let a = builtin_coding_system_eol_type(
        &m,
        &vec![Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .expect("eol");
    let b = builtin_coding_system_eol_type(
        &m,
        &vec![Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .expect("eol");
    assert!(a.is_vector(), "undecided: a vector, got {a:?}");
    assert_ne!(a.bits(), b.bits(), "two calls, two vectors");
    let fixnum = builtin_coding_system_eol_type(
        &m,
        &vec![Value::symbol("utf-8-unix")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .expect("eol");
    assert_eq!(fixnum.as_fixnum(), Some(0));
}

fn mgr() -> CodingSystemManager {
    CodingSystemManager::new()
}

fn mgr_with_latin9() -> CodingSystemManager {
    let mut m = mgr();
    m.register(CodingSystemInfo::new(
        "iso-latin-9",
        "charset",
        '0',
        EolType::Undecided,
    ));
    m.register(CodingSystemInfo::new(
        "iso-latin-9-unix",
        "charset",
        '0',
        EolType::Unix,
    ));
    m.register(CodingSystemInfo::new(
        "iso-latin-9-dos",
        "charset",
        '0',
        EolType::Dos,
    ));
    m.register(CodingSystemInfo::new(
        "iso-latin-9-mac",
        "charset",
        '0',
        EolType::Mac,
    ));
    m.add_alias("iso-8859-15", "iso-latin-9");
    m.add_alias("latin-9", "iso-latin-9");
    m.add_alias("latin-0", "iso-latin-9");
    m
}

fn plist_get(value: &Value, key: &str) -> Option<Value> {
    let needle = key.trim_start_matches(':');
    let items = list_to_vec(value)?;
    let mut idx = 0;
    while idx + 1 < items.len() {
        if items[idx]
            .as_bare_symbol_name()
            .is_some_and(|name| name.trim_start_matches(':') == needle)
        {
            return Some(items[idx + 1]);
        }
        idx += 2;
    }
    None
}

/// Build (priorities, cat_system) matching GNU's runtime state (UTF-8/English
/// language environment, the default).
fn gnu_detect_state() -> CodingDetectionBindings {
    // (category enum index, bound coding-system base name), in GNU priority order.
    let order: [(CodingCat, &str); 20] = [
        (CodingCat::Utf8Nosig, "utf-8"),
        (CodingCat::Iso7, "iso-2022-7bit"),
        (CodingCat::Charset, "iso-latin-1"),
        (CodingCat::Iso7Else, "iso-2022-7bit-lock"),
        (CodingCat::Iso8Else, "iso-2022-8bit-ss2"),
        (CodingCat::EmacsMule, "emacs-mule"),
        (CodingCat::RawText, "raw-text"),
        (CodingCat::Iso7Tight, "iso-2022-jp"),
        (CodingCat::Iso81, "in-is13194-devanagari"),
        (CodingCat::Iso82, "chinese-iso-8bit"),
        (CodingCat::Utf8Auto, "utf-8-auto"),
        (CodingCat::Utf8Sig, "utf-8-with-signature"),
        (CodingCat::Utf16Auto, "utf-16"),
        (CodingCat::Utf16Be, "utf-16be-with-signature"),
        (CodingCat::Utf16Le, "utf-16le-with-signature"),
        (CodingCat::Utf16BeNosig, "utf-16be"),
        (CodingCat::Utf16LeNosig, "utf-16le"),
        (CodingCat::Sjis, "japanese-shift-jis"),
        (CodingCat::Big5, "chinese-big5"),
        (CodingCat::Undecided, "undecided"),
    ];
    let mut cat_system: [Option<SymId>; CODING_CAT_MAX] = [None; CODING_CAT_MAX];
    let mut priorities = Vec::new();
    for (cat, name) in order {
        cat_system[cat as usize] = Some(intern(name));
        priorities.push(cat as usize);
    }
    // ccl has no bound coding system; append it (and any other) in enum order.
    for cat in 0..CODING_CAT_MAX {
        if !priorities.contains(&cat) {
            priorities.push(cat);
        }
    }
    let mgr = CodingSystemManager::new();
    CodingDetectionBindings {
        priorities,
        systems: cat_system,
        charset: CharsetDetectionPlan::for_coding_system(
            &mgr,
            intern("iso-latin-1"),
            neovm_value::tagged::value::SymbolMode::Bare,
        ),
    }
}

fn detect_list(bytes: &[u8]) -> Vec<String> {
    detect_list_mb(bytes, bytes.len(), false)
}

fn detect_list_mb(bytes: &[u8], src_chars: usize, multibytep: bool) -> Vec<String> {
    let bindings = gnu_detect_state();
    let v = detect_categories(
        &bindings,
        bytes,
        src_chars,
        multibytep,
        false,
        SourceBlock::Last,
        false,
    );
    list_to_vec(&v)
        .unwrap()
        .iter()
        .map(|s| resolve_sym(s.as_bare_symbol_id().unwrap()).to_string())
        .collect()
}
