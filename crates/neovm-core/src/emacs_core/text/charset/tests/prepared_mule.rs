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
    charset_map_cache().write().expect("charset cache").insert(
        CharsetMapCacheKey {
            map_name: map_name.to_string(),
            code_space: mapped.code_space,
            min_code: mapped.min_code,
        },
        Some(map),
    );
    let mut unified = mapped.clone();
    unified.name = intern("neovm-mule-unified");
    unified.id = 911;
    unified.method = CharsetMethod::Offset(0x1f300);
    unified.unified_p = true;
    unified.unify_map = Value::string(map_name);
    CHARSET_REGISTRY.with(|slot| {
        let mut registry = slot.borrow_mut();
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
