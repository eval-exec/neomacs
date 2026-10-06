use super::*;

// Expected results come from the full registry rules; the encoder owns only
// prepared numeric/map data and falls back for other charset methods.
fn prepared_encode(name: SymId, ch: i64) -> Option<i64> {
    CharsetEncoder::new(name).encode_char(ch)
}

#[test]
fn prepared_mule_scalar_preserves_subset_and_superset_rules() {
    crate::test_utils::init_test_tracing();
    let _context = Context::new();
    CHARSET_REGISTRY.with(|slot| {
        let mut registry = slot.borrow_mut();
        let mut parent = CharsetRegistry::make_default(900, "neovm-mule-parent");
        parent.code_space = [33, 126, 0, 0, 0, 0, 0, 0];
        parent.min_code = 33;
        parent.max_code = 126;
        parent.method = CharsetMethod::Offset(0x1f300);
        registry.register(parent);
        let mut subset = CharsetRegistry::make_default(901, "neovm-mule-subset");
        subset.min_code = 40;
        subset.max_code = 50;
        subset.method = CharsetMethod::Subset(CharsetSubsetSpec {
            parent: intern("neovm-mule-parent"),
            parent_min_code: 33,
            parent_max_code: 43,
            offset: 7,
        });
        registry.register(subset);
        let mut superset = CharsetRegistry::make_default(902, "neovm-mule-superset");
        superset.method = CharsetMethod::Superset(vec![(intern("neovm-mule-subset"), 100)]);
        registry.register(superset);
        registry.define_alias(
            intern("neovm-mule-subset-alias"),
            intern("neovm-mule-subset"),
        );
    });
    for name in [
        "neovm-mule-parent",
        "neovm-mule-subset",
        "neovm-mule-subset-alias",
        "neovm-mule-superset",
    ] {
        let name = intern(name);
        for ch in [0, 65, 0x1f2ff, 0x1f300, 0x1f30a, 0x1f30b, 0x1f35d, 0x1f35e] {
            let expected = CHARSET_REGISTRY.with(|slot| slot.borrow().encode_char(name, ch));
            assert_eq!(
                prepared_encode(name, ch),
                expected,
                "charset {name:?}, character {ch}"
            );
        }
    }
}

#[test]
fn prepared_mule_scalar_preserves_map_alias_and_unification() {
    crate::test_utils::init_test_tracing();
    let _context = Context::new();
    let mut mapped = CharsetRegistry::make_default(910, "neovm-mule-mapped");
    mapped.min_code = 33;
    mapped.max_code = 126;
    mapped.code_space = [33, 126, 0, 0, 0, 0, 0, 0];
    let map_name = "neovm-mule-synthetic-map";
    mapped.method = CharsetMethod::Map(map_name.to_string());
    let map = Arc::new(CharsetMapData {
        code_to_char: [(105, 233)].into_iter().collect(),
        char_to_code: [(233, 105)].into_iter().collect(),
    });
    let seed_key = CharsetMapCacheKey {
        map_name: map_name.to_string(),
        code_space: mapped.code_space,
        min_code: mapped.min_code,
    };
    let mut unified = mapped.clone();
    unified.name = intern("neovm-mule-unified");
    unified.id = 911;
    unified.method = CharsetMethod::Offset(0x1f300);
    unified.unified_p = true;
    unified.unify_map = Value::string(map_name);
    CHARSET_REGISTRY.with(|slot| {
        let mut registry = slot.borrow_mut();
        registry.seed_charset_map(seed_key, Some(map));
        registry.register(mapped);
        registry.register(unified);
        registry.define_alias(
            intern("neovm-mule-mapped-alias"),
            intern("neovm-mule-mapped"),
        );
    });
    for name in [
        "neovm-mule-mapped",
        "neovm-mule-mapped-alias",
        "neovm-mule-unified",
    ] {
        let name = intern(name);
        for ch in [0, 65, 233, 234, 0x1f300, 0x1f35d, 0x1f35e] {
            let expected = CHARSET_REGISTRY.with(|slot| slot.borrow().encode_char(name, ch));
            assert_eq!(
                prepared_encode(name, ch),
                expected,
                "charset {name:?}, character {ch}"
            );
        }
    }
}

#[test]
fn prepared_mule_order_preserves_definition_chronology_through_restore() {
    crate::test_utils::init_test_tracing();
    let _context = Context::new();
    let older_name = intern("neovm-mule-older-supplementary");
    let newer_name = intern("neovm-mule-newer-ordinary");
    let ch = 0x1f300;
    CHARSET_REGISTRY.with(|slot| {
        let mut registry = slot.borrow_mut();
        let mut older = CharsetRegistry::make_default(920, "neovm-mule-older-supplementary");
        older.code_space = [33, 126, 0, 0, 0, 0, 0, 0];
        older.min_code = 33;
        older.max_code = 126;
        older.method = CharsetMethod::Offset(ch);
        older.emacs_mule_id = Some(150);
        older.supplementary_p = true;
        registry.register(older);
    });
    let expected = |name| {
        CHARSET_REGISTRY.with(|slot| {
            let registry = slot.borrow();
            let info = registry.charsets.get(&name).expect("registered fixture");
            (
                info.emacs_mule_id.expect("Mule fixture"),
                info.dimension,
                registry.encode_char(name, ch).expect("overlapping fixture"),
            )
        })
    };
    let older_result = expected(older_name);
    assert_eq!(EmacsMuleEncoder::new().encode_char(ch), Some(older_result));
    CHARSET_REGISTRY.with(|slot| {
        let mut registry = slot.borrow_mut();
        let mut newer = CharsetRegistry::make_default(921, "neovm-mule-newer-ordinary");
        newer.code_space = [34, 126, 0, 0, 0, 0, 0, 0];
        newer.min_code = 34;
        newer.max_code = 126;
        newer.method = CharsetMethod::Offset(ch);
        newer.emacs_mule_id = Some(151);
        registry.register(newer);
    });
    let newer_result = expected(newer_name);
    // GNU's Mule list appends the new member, even though the global priority
    // list inserts this ordinary charset before supplementary charsets.
    assert_eq!(EmacsMuleEncoder::new().encode_char(ch), Some(older_result));
    let snapshot = snapshot_charset_registry();
    reset_charset_registry();
    restore_charset_registry(snapshot);
    assert_eq!(EmacsMuleEncoder::new().encode_char(ch), Some(older_result));
    builtin_set_charset_priority(vec![Value::from_sym_id(newer_name)])
        .expect("explicitly prefer the newer fixture");
    assert_eq!(EmacsMuleEncoder::new().encode_char(ch), Some(newer_result));
    let reordered = snapshot_charset_registry();
    reset_charset_registry();
    restore_charset_registry(reordered);
    assert_eq!(EmacsMuleEncoder::new().encode_char(ch), Some(newer_result));
    let alias = intern("neovm-mule-older-alias");
    CHARSET_REGISTRY.with(|slot| slot.borrow_mut().define_alias(alias, older_name));
    builtin_set_charset_priority(vec![
        Value::from_sym_id(alias),
        Value::from_sym_id(older_name),
    ])
    .expect("prefer canonical older member through alias");
    assert_eq!(EmacsMuleEncoder::new().encode_char(ch), Some(older_result));
    let snapshot = snapshot_charset_registry();
    assert_eq!(
        snapshot
            .emacs_mule_order
            .iter()
            .filter(|&&name| name == older_name)
            .count(),
        1
    );
}
