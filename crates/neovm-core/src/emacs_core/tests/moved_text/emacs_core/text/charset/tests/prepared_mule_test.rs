use super::*;
use neovm_text::emacs_core::charset::prepared_mule::EmacsMuleEncoder;
use neovm_value::intern::SymId;
use neovm_value::value::Value;
use std::sync::Arc;

#[test]
fn prepared_mule_scalar_preserves_subset_and_superset_rules() {
    crate::test_utils::init_test_tracing();
    let _context = crate::test_utils::new_context();
    CHARSET_REGISTRY.with(|slot| {
        let mut registry = slot.borrow_mut();
        let mut parent = CharsetRegistry::make_default(900, "neovm-mule-parent");
        neovm_text::test_support::authority::set_charset_info_code_space(
            &mut parent,
            [33, 126, 0, 0, 0, 0, 0, 0],
        );
        neovm_text::test_support::authority::set_charset_info_min_code(&mut parent, 33);
        neovm_text::test_support::authority::set_charset_info_max_code(&mut parent, 126);
        neovm_text::test_support::authority::set_charset_info_method(
            &mut parent,
            CharsetMethod::Offset(0x1f300),
        );
        registry.register(parent);
        let mut subset = CharsetRegistry::make_default(901, "neovm-mule-subset");
        neovm_text::test_support::authority::set_charset_info_min_code(&mut subset, 40);
        neovm_text::test_support::authority::set_charset_info_max_code(&mut subset, 50);
        neovm_text::test_support::authority::set_charset_info_method(
            &mut subset,
            CharsetMethod::Subset(CharsetSubsetSpec {
                parent: intern("neovm-mule-parent"),
                parent_min_code: 33,
                parent_max_code: 43,
                offset: 7,
            }),
        );
        registry.register(subset);
        let mut superset = CharsetRegistry::make_default(902, "neovm-mule-superset");
        neovm_text::test_support::authority::set_charset_info_method(
            &mut superset,
            CharsetMethod::Superset(vec![(intern("neovm-mule-subset"), 100)]),
        );
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
    let _context = crate::test_utils::new_context();
    let mut mapped = CharsetRegistry::make_default(910, "neovm-mule-mapped");
    neovm_text::test_support::authority::set_charset_info_min_code(&mut mapped, 33);
    neovm_text::test_support::authority::set_charset_info_max_code(&mut mapped, 126);
    neovm_text::test_support::authority::set_charset_info_code_space(
        &mut mapped,
        [33, 126, 0, 0, 0, 0, 0, 0],
    );
    let map_name = "neovm-mule-synthetic-map";
    neovm_text::test_support::authority::set_charset_info_method(
        &mut mapped,
        CharsetMethod::Map(map_name.to_string()),
    );
    let map = Arc::new(neovm_text::test_support::authority::charset_map_data(
        [(105, 233)].into_iter().collect(),
        [(233, 105)].into_iter().collect(),
    ));
    charset_map_cache().write().expect("charset cache").insert(
        neovm_text::test_support::authority::charset_map_cache_key(
            map_name.to_string(),
            mapped.code_space(),
            mapped.min_code(),
        ),
        Some(map),
    );
    let mut unified = mapped.clone();
    neovm_text::test_support::authority::set_charset_info_name(
        &mut unified,
        intern("neovm-mule-unified"),
    );
    neovm_text::test_support::authority::set_charset_info_id(&mut unified, 911);
    neovm_text::test_support::authority::set_charset_info_method(
        &mut unified,
        CharsetMethod::Offset(0x1f300),
    );
    neovm_text::test_support::authority::set_charset_info_unified_p(&mut unified, true);
    neovm_text::test_support::authority::set_charset_info_unify_map(
        &mut unified,
        Value::string(map_name),
    );
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

#[test]
fn prepared_mule_order_preserves_definition_chronology_through_restore() {
    crate::test_utils::init_test_tracing();
    let _context = crate::test_utils::new_context();
    let older_name = intern("neovm-mule-older-supplementary");
    let newer_name = intern("neovm-mule-newer-ordinary");
    let ch = 0x1f300;
    CHARSET_REGISTRY.with(|slot| {
        let mut registry = slot.borrow_mut();
        let mut older = CharsetRegistry::make_default(920, "neovm-mule-older-supplementary");
        neovm_text::test_support::authority::set_charset_info_code_space(
            &mut older,
            [33, 126, 0, 0, 0, 0, 0, 0],
        );
        neovm_text::test_support::authority::set_charset_info_min_code(&mut older, 33);
        neovm_text::test_support::authority::set_charset_info_max_code(&mut older, 126);
        neovm_text::test_support::authority::set_charset_info_method(
            &mut older,
            CharsetMethod::Offset(ch),
        );
        neovm_text::test_support::authority::set_charset_info_emacs_mule_id(&mut older, Some(150));
        neovm_text::test_support::authority::set_charset_info_supplementary_p(&mut older, true);
        registry.register(older);
    });
    let expected = |name| {
        CHARSET_REGISTRY.with(|slot| {
            let registry = slot.borrow();
            let info = registry.charsets().get(&name).expect("registered fixture");
            (
                info.emacs_mule_id().expect("Mule fixture"),
                info.dimension(),
                registry.encode_char(name, ch).expect("overlapping fixture"),
            )
        })
    };
    let older_result = expected(older_name);
    assert_eq!(EmacsMuleEncoder::new().encode_char(ch), Some(older_result));
    CHARSET_REGISTRY.with(|slot| {
        let mut registry = slot.borrow_mut();
        let mut newer = CharsetRegistry::make_default(921, "neovm-mule-newer-ordinary");
        neovm_text::test_support::authority::set_charset_info_code_space(
            &mut newer,
            [34, 126, 0, 0, 0, 0, 0, 0],
        );
        neovm_text::test_support::authority::set_charset_info_min_code(&mut newer, 34);
        neovm_text::test_support::authority::set_charset_info_max_code(&mut newer, 126);
        neovm_text::test_support::authority::set_charset_info_method(
            &mut newer,
            CharsetMethod::Offset(ch),
        );
        neovm_text::test_support::authority::set_charset_info_emacs_mule_id(&mut newer, Some(151));
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

fn prepared_encode(name: SymId, ch: i64) -> Option<i64> {
    CharsetEncoder::new(name).encode_char(ch)
}
