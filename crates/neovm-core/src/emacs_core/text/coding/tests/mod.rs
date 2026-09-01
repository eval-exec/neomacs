use super::*;
use crate::emacs_core::intern::{intern, resolve_sym};

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

fn plist_get(value: &Value, key: &str) -> Option<Value> {
    let needle = key.trim_start_matches(':');
    let items = list_to_vec(value)?;
    let mut idx = 0;
    while idx + 1 < items.len() {
        if items[idx]
            .as_symbol_name()
            .is_some_and(|name| name.trim_start_matches(':') == needle)
        {
            return Some(items[idx + 1]);
        }
        idx += 2;
    }
    None
}

// ----- CodingSystemManager construction -----

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
fn coding_defvar_lisp_variables_are_special_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    assert_eq!(
        crate::emacs_core::format_eval_result(&eval.eval_str(
            "(list (special-variable-p 'coding-system-for-read)
                   (let ((coding-system-for-read 'chinese-big5-unix))
                     (symbol-value 'coding-system-for-read))
                   (special-variable-p 'coding-system-for-write)
                   (special-variable-p 'last-coding-system-used))"
        )),
        "OK (t chinese-big5-unix t t)"
    );
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

// ----- CodingSystemInfo -----

#[test]
fn base_name_strips_suffix() {
    crate::test_utils::init_test_tracing();
    let info = CodingSystemInfo::new("utf-8-unix", "utf-8", 'U', EolType::Unix);
    assert_eq!(info.base_name(), "utf-8");

    let info2 = CodingSystemInfo::new("utf-8", "utf-8", 'U', EolType::Undecided);
    assert_eq!(info2.base_name(), "utf-8");
}

// ----- coding-system-list -----

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
        if let Some(id) = item.as_symbol_id() {
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

// ----- coding-system-aliases -----

#[test]
fn coding_system_aliases_found() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_aliases(&m, vec![Value::symbol("utf-8")]).unwrap();
    let items = list_to_vec(&result).unwrap();
    // First element should be the canonical name
    assert!(items[0].is_symbol_named("utf-8"));
    // Should include aliases like mule-utf-8
    assert!(items.len() > 1);
}

#[test]
fn coding_system_aliases_derive_eol_suffixes() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_aliases(&m, vec![Value::symbol("latin-1-unix")]).unwrap();
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
    let result = builtin_coding_system_aliases(&m, vec![Value::symbol("nonexistent")]);
    assert!(result.is_err());
}

#[test]
fn coding_system_aliases_nil_maps_to_no_conversion_family() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_aliases(&m, vec![Value::NIL]).unwrap();
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
    let result = builtin_coding_system_aliases(&m, vec![Value::string("utf-8")]);
    assert!(result.is_err());
}

// ----- coding-system-get -----

#[test]
fn coding_system_get_name() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result =
        builtin_coding_system_get(&m, vec![Value::symbol("utf-8"), Value::symbol(":name")])
            .unwrap();
    assert!(result.is_symbol_named("utf-8"));
}

#[test]
fn coding_system_get_type() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_get(
        &m,
        vec![Value::symbol("latin-1"), Value::symbol(":coding-type")],
    )
    .unwrap();
    assert!(result.is_symbol_named("charset"));
}

#[test]
fn coding_system_get_mnemonic() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result =
        builtin_coding_system_get(&m, vec![Value::symbol("utf-8"), Value::symbol(":mnemonic")])
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
    )
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn coding_system_get_unknown_system() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result =
        builtin_coding_system_get(&m, vec![Value::symbol("bogus"), Value::symbol(":name")]);
    assert!(result.is_err());
}

// ----- coding-system-plist -----

#[test]
fn coding_system_plist_utf8_core_fields() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let plist = builtin_coding_system_plist(&m, vec![Value::symbol("utf-8")]).unwrap();
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
fn coding_system_plist_keyword_keys_work_with_builtin_plist_get() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let plist = builtin_coding_system_plist(&m, vec![Value::symbol("utf-8")]).unwrap();

    let name = crate::emacs_core::builtins::builtin_plist_get(vec![plist, Value::keyword(":name")])
        .unwrap();
    assert_eq!(name, Value::symbol("utf-8"));

    let mnemonic =
        crate::emacs_core::builtins::builtin_plist_get(vec![plist, Value::keyword(":mnemonic")])
            .unwrap();
    assert_eq!(mnemonic, Value::fixnum('U' as i64));
}

#[test]
fn coding_system_plist_normalizes_alias_and_eol_variant_name() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let latin = builtin_coding_system_plist(&m, vec![Value::symbol("latin-1")]).unwrap();
    assert_eq!(
        plist_get(&latin, ":name"),
        Some(Value::symbol("iso-latin-1"))
    );

    let utf8_unix = builtin_coding_system_plist(&m, vec![Value::symbol("utf-8-unix")]).unwrap();
    assert_eq!(plist_get(&utf8_unix, ":name"), Some(Value::symbol("utf-8")));
}

#[test]
fn coding_system_plist_nil_maps_to_no_conversion() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let plist = builtin_coding_system_plist(&m, vec![Value::NIL]).unwrap();
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
    let type_err = builtin_coding_system_plist(&m, vec![Value::string("utf-8")]);
    assert!(type_err.is_err());

    let unknown = builtin_coding_system_plist(&m, vec![Value::symbol("bogus")]);
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
    )
    .unwrap();

    let plist = builtin_coding_system_plist(&m, vec![Value::symbol("utf-8")]).unwrap();
    assert_eq!(plist_get(&plist, ":foo"), Some(Value::fixnum(42)));
}

// ----- coding-system-put -----

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
    )
    .unwrap();
    assert_eq!(result, Value::list(vec![Value::symbol("unicode")]));

    let info = m.get("utf-8").expect("utf-8 coding system should exist");
    assert!(info.properties.contains_key(&intern(":charset-list")));

    // Verify it was stored
    let get_result = builtin_coding_system_get(
        &m,
        vec![Value::symbol("utf-8"), Value::symbol(":charset-list")],
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
    )
    .unwrap();

    let result =
        builtin_coding_system_get(&m, vec![Value::symbol("utf-8"), Value::symbol(":mnemonic")])
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
        )
        .unwrap(),
        Value::NIL
    );
}

// ----- coding-system-base -----

#[test]
fn coding_system_base_with_suffix() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_base(&m, vec![Value::symbol("utf-8-unix")]).unwrap();
    assert!(result.is_symbol_named("utf-8"));
}

#[test]
fn coding_system_base_without_suffix() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_base(&m, vec![Value::symbol("utf-8")]).unwrap();
    assert!(result.is_symbol_named("utf-8"));
}

#[test]
fn coding_system_base_unknown_still_strips() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_base(&m, vec![Value::symbol("foo-bar-unix")]);
    assert!(result.is_err());
}

// ----- coding-system-eol-type -----

#[test]
fn eol_type_unix() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_eol_type(&m, vec![Value::symbol("utf-8-unix")]).unwrap();
    assert!(eq_value(&result, &Value::fixnum(0)));
}

#[test]
fn eol_type_dos() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_eol_type(&m, vec![Value::symbol("utf-8-dos")]).unwrap();
    assert!(eq_value(&result, &Value::fixnum(1)));
}

#[test]
fn eol_type_mac() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_eol_type(&m, vec![Value::symbol("utf-8-mac")]).unwrap();
    assert!(eq_value(&result, &Value::fixnum(2)));
}

#[test]
fn eol_type_undecided_returns_vector() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_eol_type(&m, vec![Value::symbol("utf-8")]).unwrap();
    // Should be a vector of [utf-8-unix utf-8-dos utf-8-mac]
    if result.is_vector() {
        let locked = result.as_vector_data().unwrap().clone();
        assert_eq!(locked.len(), 3);
        assert!(locked[0].is_symbol_named("utf-8-unix"));
        assert!(locked[1].is_symbol_named("utf-8-dos"));
        assert!(locked[2].is_symbol_named("utf-8-mac"));
    } else {
        panic!("expected vector for undecided eol-type");
    }
}

#[test]
fn eol_type_latin_alias_uses_iso_latin_display_variants() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_eol_type(&m, vec![Value::symbol("latin-1")]).unwrap();
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
    let result = builtin_coding_system_eol_type(&m, vec![Value::symbol("gbk")]).unwrap();
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
    let result = builtin_coding_system_eol_type(&m, vec![Value::NIL]).unwrap();
    assert_eq!(result, Value::fixnum(0));
}

#[test]
fn eol_type_non_symbol_designator_returns_nil() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    assert!(
        builtin_coding_system_eol_type(&m, vec![Value::string("utf-8")])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_coding_system_eol_type(&m, vec![Value::fixnum(1)])
            .unwrap()
            .is_nil()
    );
}

#[test]
fn eol_type_unknown_returns_nil() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_eol_type(&m, vec![Value::symbol("nonexistent")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn find_coding_systems_region_internal_accepts_raw_unibyte_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let result = builtin_find_coding_systems_region_internal(&mut eval, vec![raw, Value::NIL]);
    assert_eq!(result.unwrap(), Value::T);
}

#[test]
fn find_coding_systems_region_internal_ignores_bad_exclude_for_ascii() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result = builtin_find_coding_systems_region_internal(
        &mut eval,
        vec![Value::string("abc"), Value::NIL, Value::symbol("utf-8")],
    );
    assert_eq!(result.unwrap(), Value::T);
}

#[test]
fn find_coding_systems_region_internal_rejects_non_list_exclude_for_non_ascii_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result = builtin_find_coding_systems_region_internal(
        &mut eval,
        vec![Value::string("汉"), Value::NIL, Value::symbol("utf-8")],
    );
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("listp"), Value::symbol("utf-8")]
            );
        }
        other => panic!("expected wrong-type-argument listp, got {other:?}"),
    }
}

#[test]
fn find_coding_systems_region_internal_rejects_non_list_exclude_for_non_ascii_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    eval.eval_str("(insert \"汉\")").expect("insert text");
    let result = builtin_find_coding_systems_region_internal(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(2), Value::symbol("utf-8")],
    );
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("listp"), Value::symbol("utf-8")]
            );
        }
        other => panic!("expected wrong-type-argument listp, got {other:?}"),
    }
}

// ----- coding-system-type -----

#[test]
fn coding_system_type_utf8() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_type(&m, vec![Value::symbol("utf-8")]).unwrap();
    assert!(result.is_symbol_named("utf-8"));
}

#[test]
fn coding_system_type_raw_text() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_type(&m, vec![Value::symbol("raw-text")]).unwrap();
    assert!(result.is_symbol_named("raw-text"));
}

#[test]
fn coding_system_type_unknown() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_type(&m, vec![Value::symbol("bogus")]);
    assert!(result.is_err());
}

// ----- coding-system-change-eol-conversion -----

#[test]
fn change_eol_by_int() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("utf-8"), Value::fixnum(1)],
    )
    .unwrap();
    assert!(result.is_symbol_named("utf-8-dos"));
}

#[test]
fn change_eol_by_symbol() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("utf-8-unix"), Value::symbol("mac")],
    )
    .unwrap();
    assert!(result.is_symbol_named("utf-8-mac"));
}

#[test]
fn change_eol_strips_existing_suffix() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("utf-8-dos"), Value::fixnum(0)],
    )
    .unwrap();
    assert!(result.is_symbol_named("utf-8-unix"));
}

#[test]
fn change_eol_gbk_alias_returns_canonical_chinese_gbk_variant() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("gbk"), Value::symbol("unix")],
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
    )
    .unwrap();
    assert_eq!(result, Value::symbol("utf-8-unix"));

    let result = builtin_coding_system_change_eol_conversion(
        &m,
        vec![Value::symbol("utf-8-dos"), Value::make_float(1.0)],
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
    ) {
        Err(Flow::Signal(sig)) => {
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
    ) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data.len(), 2);
            assert_eq!(sig.data[1], Value::fixnum(3));
        }
        other => panic!("expected args-out-of-range signal, got {other:?}"),
    }
}

// ----- coding-system-change-text-conversion -----

#[test]
fn change_text_conversion_preserves_eol() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_text_conversion(
        &m,
        vec![Value::symbol("utf-8-unix"), Value::symbol("latin-1")],
    )
    .unwrap();
    assert!(result.is_symbol_named("iso-latin-1-unix"));
}

#[test]
fn change_text_conversion_undecided_eol() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_change_text_conversion(
        &m,
        vec![Value::symbol("utf-8"), Value::symbol("latin-1")],
    )
    .unwrap();
    // utf-8 has undecided eol -> no suffix
    assert!(result.is_symbol_named("latin-1"));
}

// ----- detect-coding-string -----

#[test]
fn detect_coding_string_highest() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_detect_coding_string(&m, vec![Value::string("hello"), Value::T]).unwrap();
    assert!(result.is_symbol_named("undecided"));
}

#[test]
fn detect_coding_string_list() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_detect_coding_string(&m, vec![Value::string("hello")]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].is_symbol_named("undecided"));
}

#[test]
fn detect_coding_string_wrong_type() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_detect_coding_string(&m, vec![Value::fixnum(42)]);
    assert!(result.is_err());
}

#[test]
fn detect_coding_string_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_detect_coding_string(&m, vec![Value::string("x"), Value::NIL, Value::NIL]);
    assert!(result.is_err());
}

// ----- detect-coding-region -----

#[test]
fn detect_coding_region_highest() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.buffers.current_buffer_mut().unwrap().insert("abc");
    let result = builtin_detect_coding_region(
        &eval.coding_systems,
        &eval.buffers,
        vec![Value::fixnum(1), Value::fixnum(4), Value::T],
    )
    .unwrap();
    assert!(result.is_symbol_named("undecided"));
}

#[test]
fn detect_coding_region_list() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.buffers.current_buffer_mut().unwrap().insert("abc");
    let result = builtin_detect_coding_region(
        &eval.coding_systems,
        &eval.buffers,
        vec![Value::fixnum(1), Value::fixnum(4)],
    )
    .unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].is_symbol_named("undecided"));
}

#[test]
fn detect_coding_region_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let eval = crate::emacs_core::eval::Context::new();
    let result = builtin_detect_coding_region(
        &eval.coding_systems,
        &eval.buffers,
        vec![Value::fixnum(1), Value::fixnum(100), Value::NIL, Value::NIL],
    );
    assert!(result.is_err());
}

#[test]
fn detect_coding_region_rejects_non_integer_or_marker_bounds() {
    crate::test_utils::init_test_tracing();
    let eval = crate::emacs_core::eval::Context::new();
    assert!(
        builtin_detect_coding_region(
            &eval.coding_systems,
            &eval.buffers,
            vec![Value::string("a"), Value::fixnum(1)]
        )
        .is_err()
    );
    assert!(
        builtin_detect_coding_region(
            &eval.coding_systems,
            &eval.buffers,
            vec![Value::fixnum(1), Value::string("b")]
        )
        .is_err()
    );
    assert!(
        builtin_detect_coding_region(
            &eval.coding_systems,
            &eval.buffers,
            vec![Value::NIL, Value::fixnum(1)]
        )
        .is_err()
    );
    assert!(
        builtin_detect_coding_region(
            &eval.coding_systems,
            &eval.buffers,
            vec![Value::fixnum(1), Value::NIL]
        )
        .is_err()
    );
}

#[test]
fn detect_coding_region_validates_accessible_region_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.buffers.current_buffer_mut().unwrap().insert("abc");

    let err = builtin_detect_coding_region(
        &eval.coding_systems,
        &eval.buffers,
        vec![Value::fixnum(0), Value::fixnum(2)],
    )
    .expect_err("GNU validate_region rejects positions before point-min");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            let items = sig.data;
            assert_eq!(items.len(), 3);
            assert!(items[0].is_buffer());
            assert_eq!(items[1].as_fixnum(), Some(0));
            assert_eq!(items[2].as_fixnum(), Some(2));
        }
        other => panic!("expected args-out-of-range signal, got {other:?}"),
    }
}

// ----- keyboard/terminal coding system -----

#[test]
fn keyboard_coding_system_default() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_keyboard_coding_system(&m, vec![]).unwrap();
    assert!(result.is_symbol_named("utf-8-unix"));
}

#[test]
fn terminal_coding_system_default() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_terminal_coding_system(&m, vec![]).unwrap();
    assert!(result.is_symbol_named("utf-8-unix"));
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
    let set = builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("latin-1")]).unwrap();
    assert!(set.is_symbol_named("iso-latin-1-unix"));
    let get = builtin_keyboard_coding_system(&m, vec![]).unwrap();
    assert!(get.is_symbol_named("iso-latin-1-unix"));
}

#[test]
fn set_keyboard_coding_system_canonicalizes_non_unix_alias_suffixes() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();

    let latin_dos =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("latin-1-dos")]).unwrap();
    assert_eq!(latin_dos, Value::symbol("iso-latin-1-unix"));

    let latin_mac =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("latin-1-mac")]).unwrap();
    assert_eq!(latin_mac, Value::symbol("iso-latin-1-unix"));

    let iso_dos =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("iso-8859-1-dos")]).unwrap();
    assert_eq!(iso_dos, Value::symbol("iso-latin-1-unix"));

    let ascii_dos =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("ascii-dos")]).unwrap();
    assert_eq!(ascii_dos, Value::symbol("us-ascii-unix"));

    let ascii_mac =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("ascii-mac")]).unwrap();
    assert_eq!(ascii_mac, Value::symbol("us-ascii-unix"));
}

#[test]
fn set_keyboard_coding_system_preserves_explicit_unix_spelling() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();

    let latin_unix =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("latin-1-unix")]).unwrap();
    assert_eq!(latin_unix, Value::symbol("latin-1-unix"));

    let iso_unix =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("iso-8859-1-unix")]).unwrap();
    assert_eq!(iso_unix, Value::symbol("iso-8859-1-unix"));

    let ascii_unix =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("ascii-unix")]).unwrap();
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
        )
        .unwrap(),
        Value::symbol("iso-latin-1-unix")
    );
    assert_eq!(
        builtin_coding_system_change_eol_conversion(
            &m,
            vec![Value::symbol("latin-1-unix"), Value::NIL],
        )
        .unwrap(),
        Value::symbol("iso-latin-1")
    );
    assert_eq!(
        builtin_coding_system_change_eol_conversion(
            &m,
            vec![Value::symbol("latin-1-unix"), Value::fixnum(1)],
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
        )
        .unwrap(),
        Value::symbol("iso-latin-9-unix")
    );
    assert_eq!(
        builtin_coding_system_change_eol_conversion(
            &m,
            vec![Value::symbol("iso-8859-15-unix"), Value::NIL],
        )
        .unwrap(),
        Value::symbol("iso-latin-9")
    );
    assert_eq!(
        builtin_coding_system_base(&m, vec![Value::symbol("iso-8859-15-unix")]).unwrap(),
        Value::symbol("iso-latin-9")
    );
}

#[test]
fn set_keyboard_coding_system_normalizes_latin9_alias_family() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr_with_latin9();

    let set =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("iso-8859-15")]).unwrap();
    assert_eq!(set, Value::symbol("iso-latin-9-unix"));

    let get = builtin_keyboard_coding_system(&m, vec![]).unwrap();
    assert_eq!(get, Value::symbol("iso-latin-9-unix"));
}

#[test]
fn set_keyboard_coding_system_accepts_alias_derived_variants() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();

    let latin_unix =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("latin-1-unix")]).unwrap();
    assert_eq!(latin_unix, Value::symbol("latin-1-unix"));

    let latin_dos =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("latin-1-dos")]).unwrap();
    assert_eq!(latin_dos, Value::symbol("iso-latin-1-unix"));
}

#[test]
fn set_terminal_coding_system_accepts_alias_derived_variants() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();

    assert!(
        builtin_set_terminal_coding_system(&mut m, vec![Value::symbol("latin-1-unix")]).is_ok()
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
    let set = builtin_set_terminal_coding_system(&mut m, vec![Value::symbol("ascii")]).unwrap();
    assert!(set.is_nil());
    let get = builtin_terminal_coding_system(&m, vec![]).unwrap();
    assert!(get.is_symbol_named("ascii"));
}

#[test]
fn set_keyboard_coding_nil_resets_to_no_conversion() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("latin-1")]).unwrap();
    builtin_set_keyboard_coding_system(&mut m, vec![Value::NIL]).unwrap();
    let result = builtin_keyboard_coding_system(&m, vec![]).unwrap();
    assert!(result.is_symbol_named("no-conversion"));
}

#[test]
fn set_terminal_coding_nil_sets_nil_symbol() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    builtin_set_terminal_coding_system(&mut m, vec![Value::symbol("utf-8")]).unwrap();
    builtin_set_terminal_coding_system(&mut m, vec![Value::NIL]).unwrap();
    let result = builtin_terminal_coding_system(&m, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn coding_system_setters_validate_symbol_and_known_names() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    assert!(builtin_set_keyboard_coding_system(&mut m, vec![Value::string("utf-8")]).is_err());
    assert!(builtin_set_terminal_coding_system(&mut m, vec![Value::string("utf-8")]).is_err());
    assert!(
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("no-such-coding")]).is_err()
    );
    assert!(
        builtin_set_terminal_coding_system(&mut m, vec![Value::symbol("no-such-coding")]).is_err()
    );
}

#[test]
fn coding_system_setters_treat_keywords_as_symbol_designators() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let keyword = Value::keyword(":utf-8");
    let kb = builtin_set_keyboard_coding_system(&mut m, vec![keyword]);
    let term = builtin_set_terminal_coding_system(&mut m, vec![keyword]);

    match kb {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "coding-system-error"),
        other => panic!("expected coding-system-error for keyword keyboard set, got {other:?}"),
    }
    match term {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "coding-system-error"),
        other => panic!("expected coding-system-error for keyword terminal set, got {other:?}"),
    }
}

#[test]
fn coding_system_setters_validate_arity_edges() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    assert!(builtin_set_keyboard_coding_system(&mut m, vec![Value::NIL, Value::NIL]).is_ok());
    assert!(
        builtin_set_keyboard_coding_system(&mut m, vec![Value::NIL, Value::NIL, Value::NIL])
            .is_err()
    );

    assert!(builtin_set_terminal_coding_system(&mut m, vec![Value::NIL, Value::NIL]).is_ok());
    assert!(
        builtin_set_terminal_coding_system(&mut m, vec![Value::NIL, Value::NIL, Value::NIL])
            .is_ok()
    );
    assert!(
        builtin_set_terminal_coding_system(
            &mut m,
            vec![Value::NIL, Value::NIL, Value::NIL, Value::NIL]
        )
        .is_err()
    );
}

// ----- coding-system-priority-list -----

#[test]
fn priority_list_full() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_priority_list(&m, vec![]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert!(!items.is_empty());
    // First should be utf-8
    assert!(items[0].is_symbol_named("utf-8"));
}

#[test]
fn priority_list_highest() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    // GNU `(coding-system-priority-list t)` returns a BARE symbol (the base name
    // of the highest-priority category), not a one-element list.
    let result = builtin_coding_system_priority_list(&m, vec![Value::T]).unwrap();
    assert!(result.is_symbol_named("utf-8"));
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

// ----- EolType -----

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

// ----- strip_eol_suffix -----

#[test]
fn strip_eol_suffix_works() {
    crate::test_utils::init_test_tracing();
    assert_eq!(strip_eol_suffix("utf-8-unix"), "utf-8");
    assert_eq!(strip_eol_suffix("utf-8-dos"), "utf-8");
    assert_eq!(strip_eol_suffix("utf-8-mac"), "utf-8");
    assert_eq!(strip_eol_suffix("utf-8"), "utf-8");
    assert_eq!(strip_eol_suffix("latin-1"), "latin-1");
}

// ----- argument validation -----

#[test]
fn coding_system_get_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_get(&m, vec![Value::symbol("utf-8")]);
    assert!(result.is_err());
}

#[test]
fn coding_system_base_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_base(&m, vec![]);
    assert!(result.is_err());
}

#[test]
fn coding_system_aliases_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_coding_system_aliases(&m, vec![]);
    assert!(result.is_err());
}

#[test]
fn coding_system_p_reads_runtime_aliases() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let before = builtin_coding_system_p(&m, vec![Value::symbol("vm-utf8")]).unwrap();
    assert!(before.is_nil());

    builtin_define_coding_system_alias(
        &mut m,
        vec![Value::symbol("vm-utf8"), Value::symbol("utf-8")],
    )
    .unwrap();
    let after = builtin_coding_system_p(&m, vec![Value::symbol("vm-utf8")]).unwrap();
    assert!(after.is_truthy());
}

#[test]
fn coding_system_p_accepts_nil_and_supported_derived_variants() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    assert!(
        builtin_coding_system_p(&m, vec![Value::NIL])
            .unwrap()
            .is_truthy()
    );
    assert!(
        builtin_coding_system_p(&m, vec![Value::symbol("ascii-dos")])
            .unwrap()
            .is_truthy()
    );
}

#[test]
fn check_coding_system_signals_unknown_symbols() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let result = builtin_check_coding_system(&m, vec![Value::symbol("vm-no-such")]);
    match result {
        Err(Flow::Signal(sig)) => {
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
        builtin_check_coding_system(&m, vec![Value::symbol("latin-1-unix")]).unwrap(),
        Value::symbol("latin-1-unix")
    );
    assert_eq!(
        builtin_check_coding_system(&m, vec![Value::symbol("ascii-unix")]).unwrap(),
        Value::symbol("ascii-unix")
    );
    assert_eq!(
        builtin_check_coding_system(&m, vec![Value::symbol("undecided-unix")]).unwrap(),
        Value::symbol("undecided-unix")
    );
    assert_eq!(
        builtin_check_coding_system(&m, vec![Value::symbol("utf-8-auto-unix")]).unwrap(),
        Value::symbol("utf-8-auto-unix")
    );
    assert_eq!(
        builtin_check_coding_system(&m, vec![Value::symbol("prefer-utf-8-unix")]).unwrap(),
        Value::symbol("prefer-utf-8-unix")
    );
    assert_eq!(
        builtin_check_coding_system(&m, vec![Value::symbol("gbk-unix")]).unwrap(),
        Value::symbol("gbk-unix")
    );
}

#[test]
fn check_coding_system_rejects_unsupported_derived_variants() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    assert!(builtin_check_coding_system(&m, vec![Value::symbol("no-conversion-unix")]).is_err());
    assert!(builtin_check_coding_system(&m, vec![Value::symbol("binary-unix")]).is_err());
    // emacs-internal is an alias for utf-8-emacs-unix, so
    // emacs-internal-unix correctly resolves to utf-8-emacs-unix
    // (the base is utf-8-emacs-unix, strip suffix → utf-8-emacs, derive
    // unix → utf-8-emacs-unix).
}

#[test]
fn check_coding_systems_region_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    assert!(
        builtin_check_coding_systems_region(
            &mut eval,
            vec![
                Value::fixnum(1),
                Value::fixnum(1),
                Value::list(vec![Value::symbol("utf-8")])
            ]
        )
        .unwrap()
        .is_nil()
    );
    assert!(
        builtin_check_coding_systems_region(
            &mut eval,
            vec![Value::string("x"), Value::fixnum(1), Value::symbol("utf-8")]
        )
        .unwrap()
        .is_nil()
    );

    let start_type_err = builtin_check_coding_systems_region(
        &mut eval,
        vec![Value::symbol("x"), Value::fixnum(1), Value::symbol("utf-8")],
    )
    .unwrap_err();
    match start_type_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("integer-or-marker-p"), Value::symbol("x")]
            );
        }
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }

    let type_err = builtin_check_coding_systems_region(
        &mut eval,
        vec![Value::fixnum(1), Value::string("x"), Value::symbol("utf-8")],
    )
    .unwrap_err();
    match type_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("integer-or-marker-p"), Value::string("x")]
            );
        }
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }

    assert!(builtin_check_coding_systems_region(&mut eval, vec![]).is_err());
    assert!(
        builtin_check_coding_systems_region(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)])
            .is_err()
    );
}

#[test]
fn check_coding_systems_region_matches_gnu_validation_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.eval_str("(insert \"abc\")").expect("insert ascii");

    let range_err = builtin_check_coding_systems_region(
        &mut eval,
        vec![
            Value::fixnum(9),
            Value::fixnum(10),
            Value::list(vec![Value::symbol("no-such-coding")]),
        ],
    )
    .expect_err("GNU validates bad buffer ranges before coding systems");
    match range_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(9), Value::fixnum(10)]);
        }
        other => panic!("expected args-out-of-range, got {other:?}"),
    }

    let reversed_err = builtin_check_coding_systems_region(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(2),
            Value::list(vec![Value::symbol("no-such-coding")]),
        ],
    )
    .expect_err("GNU does not swap check-coding-systems-region endpoints");
    match reversed_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(3), Value::fixnum(2)]);
        }
        other => panic!("expected args-out-of-range, got {other:?}"),
    }

    assert!(
        builtin_check_coding_systems_region(
            &mut eval,
            vec![
                Value::fixnum(1),
                Value::fixnum(2),
                Value::list(vec![Value::symbol("no-such-coding")]),
            ],
        )
        .expect("GNU ignores coding list for ASCII buffer text")
        .is_nil()
    );

    let coding_err = builtin_check_coding_systems_region(
        &mut eval,
        vec![
            Value::string("é"),
            Value::NIL,
            Value::list(vec![Value::symbol("no-such-coding")]),
        ],
    )
    .expect_err("GNU validates coding list for non-ASCII string text");
    match coding_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "coding-system-error");
            assert_eq!(sig.data, vec![Value::symbol("no-such-coding")]);
        }
        other => panic!("expected coding-system-error, got {other:?}"),
    }

    assert!(
        builtin_check_coding_systems_region(
            &mut eval,
            vec![
                Value::string("é"),
                Value::NIL,
                Value::cons(Value::symbol("utf-8"), Value::symbol("ignored-tail")),
            ],
        )
        .expect("GNU ignores dotted tail after valid coding cons cars")
        .is_nil()
    );
}

#[test]
fn set_keyboard_coding_system_rejects_unsuitable_variants() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let auto = builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("utf-8-auto")]);
    let auto_derived =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("utf-8-auto-unix")]);
    let prefer = builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("prefer-utf-8")]);
    let prefer_derived =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("prefer-utf-8-unix")]);
    let undecided = builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("undecided")]);
    let undecided_derived =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("undecided-unix")]);

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
    let set =
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("emacs-internal")]).unwrap();
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
    )
    .unwrap();

    let list = builtin_coding_system_priority_list(&m, vec![]).unwrap();
    let items = list_to_vec(&list).unwrap();
    assert!(items[0].is_symbol_named("raw-text"));
    assert!(items[1].is_symbol_named("utf-8"));
}

#[test]
fn set_coding_system_priority_rejects_nil_payload() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let result = builtin_set_coding_system_priority(&mut m, vec![Value::NIL]);
    match result {
        Err(Flow::Signal(sig)) => {
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
    let result = builtin_set_coding_system_priority(&mut m, vec![Value::keyword(":utf-8")]);
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "coding-system-error"),
        other => panic!("expected coding-system-error signal, got {other:?}"),
    }
}

#[test]
fn set_coding_system_priority_string_is_type_error() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    let result = builtin_set_coding_system_priority(&mut m, vec![Value::string("utf-8")]);
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn internal_coding_system_setters_match_surface_validation() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    assert_eq!(
        builtin_set_keyboard_coding_system_internal(&mut m, vec![Value::symbol("utf-8")]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_set_terminal_coding_system_internal(&mut m, vec![Value::symbol("utf-8")]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_set_safe_terminal_coding_system_internal(&mut m, vec![Value::symbol("utf-8")])
            .unwrap(),
        Value::NIL
    );
    assert!(
        builtin_set_keyboard_coding_system_internal(&mut m, vec![Value::symbol("foo")]).is_err()
    );
    assert!(
        builtin_set_terminal_coding_system_internal(&mut m, vec![Value::symbol("foo")]).is_err()
    );
    assert!(
        builtin_set_safe_terminal_coding_system_internal(&mut m, vec![Value::symbol("foo")])
            .is_err()
    );
}

#[test]
fn text_quoting_and_conversion_style_basics() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        builtin_text_quoting_style(&eval, vec![]).expect("text-quoting-style"),
        Value::symbol("curve")
    );
    for style in ["grave", "straight", "curve"] {
        eval.obarray
            .set_symbol_value("text-quoting-style", Value::symbol(style));
        assert_eq!(
            builtin_text_quoting_style(&eval, vec![]).expect("text-quoting-style explicit style"),
            Value::symbol(style)
        );
    }
    for style in ["nil", "foo", "Curve"] {
        eval.obarray
            .set_symbol_value("text-quoting-style", Value::symbol(style));
        assert_eq!(
            builtin_text_quoting_style(&eval, vec![]).expect("text-quoting-style fallback"),
            Value::symbol("curve")
        );
    }
    assert!(builtin_text_quoting_style(&eval, vec![Value::NIL]).is_err());
    assert_eq!(
        builtin_set_text_conversion_style(vec![Value::symbol("latin-1")])
            .expect("set-text-conversion-style"),
        Value::NIL
    );
    assert_eq!(
        builtin_set_text_conversion_style(vec![Value::symbol("foo"), Value::symbol("bar")])
            .expect("set-text-conversion-style 2 args"),
        Value::NIL
    );
    assert!(builtin_set_text_conversion_style(vec![]).is_err());
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
            TextQuotingStyle::from_symbol_value(Value::symbol(name)),
            Some(style)
        );
        assert_eq!(style.to_symbol(), Value::symbol(name));
    }
    assert_eq!(
        TextQuotingStyle::from_symbol_value(Value::symbol("Curve")),
        None
    );
    assert_eq!(
        TextQuotingStyle::from_symbol_value(Value::symbol("nil")),
        None
    );
    assert_eq!(TextQuotingStyle::from_symbol_value(Value::NIL), None);
}

#[test]
fn text_quoting_style_variable_defaults_to_nil() {
    crate::test_utils::init_test_tracing();
    let eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        eval.obarray.symbol_value("text-quoting-style"),
        Some(&Value::NIL)
    );
}

#[test]
fn find_operation_coding_system_validates_operation_target_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    assert_eq!(
        eval.eval_str(r#"(find-operation-coding-system 'insert-file-contents "x")"#)
            .expect("valid file target returns nil when file-coding-system-alist is nil"),
        Value::NIL
    );

    let invalid = eval
        .eval_str(r#"(find-operation-coding-system 'insert-file-contents (list "x"))"#)
        .expect_err("GNU rejects non-string insert-file-contents targets");
    match invalid {
        crate::emacs_core::error::EvalError::Signal { symbol, data, .. } => {
            assert_eq!(resolve_sym(symbol), "error");
            assert_eq!(
                data,
                vec![Value::string(
                    "Invalid argument 1 of operation ‘insert-file-contents’"
                )]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }

    let invalid_operation = eval
        .eval_str(r#"(find-operation-coding-system 'not-an-operation "x")"#)
        .expect_err("GNU rejects operations without target-idx");
    match invalid_operation {
        crate::emacs_core::error::EvalError::Signal { symbol, data, .. } => {
            assert_eq!(resolve_sym(symbol), "error");
            assert_eq!(data, vec![Value::string("Invalid first argument")]);
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

// ===========================================================================
// detect-coding-string algorithm verification against GNU Emacs 31 outputs.
//
// `detect_categories` is the pure detection core; here we drive it with the
// exact category->coding-system bindings and priority order GNU reports via
// `coding-system-priority-list`, then assert byte-exact result lists captured
// from the GNU binary.
// ===========================================================================

/// Build (priorities, cat_system) matching GNU's runtime state (UTF-8/English
/// language environment, the default).
fn gnu_detect_state() -> (Vec<usize>, [Option<SymId>; CODING_CAT_MAX]) {
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
    (priorities, cat_system)
}

fn detect_list(bytes: &[u8]) -> Vec<String> {
    detect_list_mb(bytes, bytes.len(), false)
}

fn detect_list_mb(bytes: &[u8], src_chars: usize, multibytep: bool) -> Vec<String> {
    let (priorities, cat_system) = gnu_detect_state();
    let v = detect_categories(
        &priorities,
        &cat_system,
        bytes,
        src_chars,
        multibytep,
        false,
        SourceBlock::Last,
    );
    list_to_vec(&v)
        .unwrap()
        .iter()
        .map(|s| resolve_sym(s.as_symbol_id().unwrap()).to_string())
        .collect()
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

// NOTE: `(detect-coding-string (unibyte-string 228 184 173 230 151 182))`
// (3-byte UTF-8 for 中时) returns `(utf-8 utf-8-auto japanese-shift-jis raw-text)`
// in GNU.  The exclusion of emacs-mule depends on `emacs_mule_bytes[0x97] == 3`,
// which comes from the charset `japanese-jisx0213-1` (emacs-mule-id 151,
// dimension 2).  That charset is defined by lisp at startup and is *not* present
// in the bare unit-test charset registry, so this case can only be verified
// against the booted runtime, not here.

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

// ----- multibyte string inputs (`(string ...)` / "literal") -----
// These produce *multibyte* strings; detection runs on their Emacs-internal
// byte representation with `multibytep = t`, so high chars decode to single
// (negated) codes rather than raw bytes.

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

// ===========================================================================
// Coding-system priority list (Group B): category-based reordering.
// ===========================================================================

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
    builtin_set_coding_system_priority(&mut m, vec![Value::symbol("utf-8")]).unwrap();
    let once = list_to_vec(&builtin_coding_system_priority_list(&m, vec![]).unwrap()).unwrap();
    builtin_set_coding_system_priority(&mut m, vec![Value::symbol("utf-8")]).unwrap();
    let twice = list_to_vec(&builtin_coding_system_priority_list(&m, vec![]).unwrap()).unwrap();
    assert_eq!(
        once.len(),
        twice.len(),
        "re-preferring must not grow the list"
    );
    assert_eq!(
        resolve_sym(once[0].as_symbol_id().unwrap()),
        "utf-8",
        "preferred system moves to the head"
    );
    assert_eq!(resolve_sym(twice[0].as_symbol_id().unwrap()), "utf-8");
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
    builtin_set_coding_system_priority(&mut m, vec![Value::symbol("iso-latin-1")]).unwrap();
    // HIGHESTP returns a bare symbol (GNU `CODING_ATTR_BASE_NAME`).
    let after_latin = builtin_coding_system_priority_list(&m, vec![Value::T]).unwrap();
    assert_eq!(
        resolve_sym(after_latin.as_symbol_id().unwrap()),
        "iso-latin-1"
    );
    builtin_set_coding_system_priority(&mut m, vec![Value::symbol("utf-8")]).unwrap();
    let after_utf8 =
        list_to_vec(&builtin_coding_system_priority_list(&m, vec![]).unwrap()).unwrap();
    assert_eq!(resolve_sym(after_utf8[0].as_symbol_id().unwrap()), "utf-8");
    // Fronting two distinct categories one at a time never grows the list.
    assert!(after_utf8.len() <= base);
}

// ===========================================================================
// utf-8-with-signature BOM on encode (Group C).
// ===========================================================================

#[test]
fn encode_lisp_string_prepends_bom_for_signature() {
    crate::test_utils::init_test_tracing();
    let s = crate::heap_types::LispString::from_unibyte(b"x".to_vec());
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

// ===========================================================================
// Regression: chinese-iso-8bit / chinese-big5 must NOT be dropped from the
// detection priority list during the loadup `reset-language-environment`
// reorder.  GNU classifies them under distinct detection *categories*
// (`coding-category-iso-8-2` and `coding-category-big5`), so fronting the
// charset category (`iso-latin-1`) must leave them in place.  Mapping them to
// `coding-category-charset` (the old bug) made them collide with iso-latin-1
// and get deduped out, shrinking the list from GNU's 20 down to 18.
// ===========================================================================

#[test]
fn priority_list_keeps_chinese_iso_8bit_and_big5_after_charset_front() {
    crate::test_utils::init_test_tracing();
    let mut m = mgr();
    assert_eq!(m.priority.len(), 20, "fresh new() should have 20 entries");

    // The three charset-family entries must each have GNU's category: only
    // iso-latin-1 is `coding-category-charset`; the two Chinese systems are
    // distinct (`iso-8-2` and `big5`).
    assert_eq!(
        coding_category_of(&m, "iso-latin-1"),
        Some("coding-category-charset")
    );
    assert_eq!(
        coding_category_of(&m, "chinese-iso-8bit"),
        Some("coding-category-iso-8-2")
    );
    assert_eq!(
        coding_category_of(&m, "chinese-big5"),
        Some("coding-category-big5")
    );

    // Fronting the charset category (as `reset-language-environment` does via
    // `iso-latin-1`) must not evict the distinct-category Chinese systems.
    builtin_set_coding_system_priority(&mut m, vec![Value::symbol("iso-latin-1")]).unwrap();
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

// ===========================================================================
// `coding-category-list`: all 21 detection categories in priority order, like
// GNU's `Vcoding_category_list`.  Verifies the value the dispatcher writes.
// ===========================================================================

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
    let cats: Vec<&str> = coding_category_priority_list(&m)
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

// Verifies the unbound-category insertion against the exact booted post-
// `reset-language-environment` category order (the 20 bound categories in
// priority order, by `enum coding_category` index), and that it reproduces
// GNU's `coding-category-list` byte-for-byte (ccl lands before undecided).
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

// ===========================================================================
// detect-coding-string iso-2022 escape-designation category mapping (bug 14).
//
// `\033$B$3\033(B` designates JISX0208 (a 7-bit ISO-2022 stream).  GNU's
// `detect_coding_iso_2022` records this as found for the iso-7, iso-7-tight,
// iso-7-else and iso-8-else categories, so the highest-priority *found*
// category wins.  Default priority -> iso-2022-7bit (the iso-7 category);
// with a priority override, the fronted category's coding system wins.
// ===========================================================================

/// Register the `japanese-jisx0208` charset (dim-2, ISO final 'B') in the bare
/// test registry so the iso-2022 escape detector recognizes the `$B`
/// designation, mirroring the booted runtime.
fn register_jisx0208_for_detect() {
    let mut args = vec![Value::NIL; 17];
    args[0] = Value::symbol("japanese-jisx0208");
    args[1] = Value::fixnum(2); // dimension
    args[2] = Value::vector(vec![
        Value::fixnum(33),
        Value::fixnum(126),
        Value::fixnum(33),
        Value::fixnum(126),
    ]); // 94x94 code-space
    args[5] = Value::fixnum(66); // iso-final-char 'B'
    crate::emacs_core::charset::builtin_define_charset_internal(args).unwrap();
}

const ISO7_DESIGNATION: &[u8] = b"\x1b$B$3\x1b(B";

fn detect_highest_with_front(front: Option<CodingCat>) -> String {
    let (mut priorities, cat_system) = gnu_detect_state();
    if let Some(cat) = front {
        let c = cat as usize;
        priorities.retain(|&x| x != c);
        priorities.insert(0, c);
    }
    let v = detect_categories(
        &priorities,
        &cat_system,
        ISO7_DESIGNATION,
        6,
        false,
        true,
        SourceBlock::Last,
    );
    resolve_sym(v.as_symbol_id().unwrap()).to_string()
}

#[test]
fn detect_iso7_designation_default_priority_is_iso_2022_7bit() {
    crate::test_utils::init_test_tracing();
    register_jisx0208_for_detect();
    // (detect-coding-string (string-to-unibyte "\033$B$3\033(B") t) => iso-2022-7bit
    assert_eq!(detect_highest_with_front(None), "iso-2022-7bit");
    // The list form: (iso-2022-7bit iso-2022-7bit-lock iso-2022-8bit-ss2 iso-2022-jp)
    assert_eq!(
        detect_list(ISO7_DESIGNATION),
        vec![
            "iso-2022-7bit",
            "iso-2022-7bit-lock",
            "iso-2022-8bit-ss2",
            "iso-2022-jp",
        ]
    );
}

#[test]
fn detect_iso7_designation_respects_priority_override() {
    crate::test_utils::init_test_tracing();
    register_jisx0208_for_detect();
    // (with-coding-priority '(iso-2022-jp) ...) => iso-2022-jp (iso-7-tight).
    assert_eq!(
        detect_highest_with_front(Some(CodingCat::Iso7Tight)),
        "iso-2022-jp"
    );
    // (with-coding-priority '(iso-2022-7bit-lock) ...) => iso-2022-7bit-lock.
    assert_eq!(
        detect_highest_with_front(Some(CodingCat::Iso7Else)),
        "iso-2022-7bit-lock"
    );
    // (with-coding-priority '(iso-2022-8bit-ss2) ...) => iso-2022-8bit-ss2.
    assert_eq!(
        detect_highest_with_front(Some(CodingCat::Iso8Else)),
        "iso-2022-8bit-ss2"
    );
}

// ---------------------------------------------------------------------------
// fix8 GROUP=coding bug (3): check-coding-systems-region +
// unencodable-char-position used to always return nil. They now scan each
// candidate coding system's :charset-list for unencodable characters, matching
// GNU's Fcheck_coding_systems_region / Funencodable_char_position (coding.c).
// ---------------------------------------------------------------------------

fn fmt(eval: &mut crate::emacs_core::eval::Context, src: &str) -> String {
    crate::emacs_core::format_eval_result(&eval.eval_str(src))
}

/// `Context::new()` is bare (no `with-temp-buffer` macro), so seed the current
/// buffer directly: erase it, insert `seed`, then evaluate `expr`.
fn fmt_buf(eval: &mut crate::emacs_core::eval::Context, seed: &str, expr: &str) -> String {
    let src = format!("(progn (erase-buffer) (insert {seed}) {expr})");
    crate::emacs_core::format_eval_result(&eval.eval_str(&src))
}

#[test]
fn check_coding_systems_region_reports_unencodable_positions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // (us-ascii 2): "é" at buffer position 2 is not us-ascii-encodable.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?a ?\u{e9} ?b)",
            "(check-coding-systems-region (point-min) (point-max) '(us-ascii))",
        ),
        "OK ((us-ascii 2))"
    );
}

#[test]
fn check_coding_systems_region_accepts_string_first_arg() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // String START => 0-based char indices, END ignored: "é" is index 1.
    assert_eq!(
        fmt(
            &mut eval,
            "(check-coding-systems-region (string ?a ?\u{e9} ?b) nil '(us-ascii))",
        ),
        "OK ((us-ascii 1))"
    );
}

#[test]
fn check_coding_systems_region_utf8_encodes_everything() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // utf-8 (charset `unicode`) encodes every character, so it never appears.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?a ?\u{e9} ?b ?\u{3042})",
            "(check-coding-systems-region (point-min) (point-max) '(utf-8 us-ascii))",
        ),
        "OK ((us-ascii 2 4))"
    );
}

#[test]
fn check_coding_systems_region_preserves_input_order_and_charset_membership() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // latin-1 encodes "é" but not "あ" (pos 2); us-ascii encodes neither (1 2).
    // Output keeps the input list order.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?\u{e9} ?\u{3042})",
            "(check-coding-systems-region (point-min) (point-max) '(latin-1 us-ascii))",
        ),
        "OK ((latin-1 2) (us-ascii 1 2))"
    );
    // latin-1 encodes both "é" and "ü", so the region is fully encodable => nil.
    // (euc-jp/shift_jis are only registered by elisp at runtime, not by the bare
    // `CodingSystemManager::new()`, so they are verified on the release binary.)
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?\u{e9} ?\u{fc})",
            "(check-coding-systems-region (point-min) (point-max) '(latin-1))",
        ),
        "OK nil"
    );
}

#[test]
fn unencodable_char_position_returns_first_position() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // No COUNT => the first unencodable position (1-based) as a bare integer.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?a ?\u{e9})",
            "(unencodable-char-position (point-min) (point-max) 'us-ascii)",
        ),
        "OK 2"
    );
}

#[test]
fn unencodable_char_position_with_count_returns_list() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // COUNT non-nil => a list of up to COUNT positions in ascending order.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?a ?\u{e9} ?b ?\u{fc})",
            "(unencodable-char-position (point-min) (point-max) 'us-ascii 5)",
        ),
        "OK (2 4)"
    );
}

#[test]
fn unencodable_char_position_string_arg_uses_zero_based_indices() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // STRING arg: START/END index the string; positions are 0-based char indices.
    assert_eq!(
        fmt(
            &mut eval,
            "(unencodable-char-position 0 3 'us-ascii nil (string ?a ?\u{e9} ?b))",
        ),
        "OK 1"
    );
    assert_eq!(
        fmt(
            &mut eval,
            "(unencodable-char-position 0 5 'us-ascii 5 (string ?a ?\u{e9} ?b ?\u{fc} ?c))",
        ),
        "OK (1 3)"
    );
}

#[test]
fn unencodable_char_position_raw_text_and_encodable_return_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // raw-text encodes every byte => nil.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?\u{e9})",
            "(unencodable-char-position (point-min) (point-max) 'raw-text)",
        ),
        "OK nil"
    );
    // utf-8 encodes everything => nil.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?\u{3042})",
            "(unencodable-char-position (point-min) (point-max) 'utf-8)",
        ),
        "OK nil"
    );
}

#[test]
fn unencodable_char_position_swaps_reversed_region() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    // GNU validate_region swaps START/END; check-coding-systems-region does not.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?a ?\u{e9} ?b)",
            "(unencodable-char-position 4 1 'us-ascii)",
        ),
        "OK 2"
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

/// DIVERGENCES.md entry 143: `Context::eol_conversion` reads the variable the
/// way GNU's C code sees a `DEFVAR_BOOL` -- the dynamic value, which no lexical
/// binding of the name can shadow, and nil when unbound (src/coding.c:12027).
#[test]
fn context_eol_conversion_reads_the_defvar_bool_dynamically() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::coding::EolConversion;

    let mut eval = crate::emacs_core::eval::Context::new();
    assert_eq!(eval.eol_conversion(), EolConversion::Enabled);
    let _ = eval.eval_str("(setq inhibit-eol-conversion t)");
    assert_eq!(eval.eol_conversion(), EolConversion::Inhibited);
    let _ = eval.eval_str("(setq inhibit-eol-conversion nil)");
    assert_eq!(eval.eol_conversion(), EolConversion::Enabled);
    // A non-nil, non-`t` value inhibits too: GNU tests the C bool.
    let _ = eval.eval_str("(setq inhibit-eol-conversion 'anything)");
    assert_eq!(eval.eol_conversion(), EolConversion::Inhibited);
}
