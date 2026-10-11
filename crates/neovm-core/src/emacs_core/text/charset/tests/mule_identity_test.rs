use super::*;

#[test]
fn prepared_mule_preserves_existing_member_identity_when_name_becomes_alias() {
    crate::test_utils::init_test_tracing();
    let _context = Context::new();
    let original = intern("neovm-mule-collision-original");
    let target = intern("neovm-mule-collision-target");
    let ch = 0x1f300;
    let original_only = 0x1f350;
    CHARSET_REGISTRY.with(|slot| {
        let mut registry = slot.borrow_mut();
        for (name, id, mule_id, min_code, max_code) in
            [(original, 930, 150, 33, 126), (target, 931, 151, 34, 50)]
        {
            let mut info = CharsetRegistry::make_default(id, resolve_sym(name));
            info.code_space = [min_code, max_code, 0, 0, 0, 0, 0, 0];
            info.min_code = min_code;
            info.max_code = max_code;
            info.method = CharsetMethod::Offset(ch);
            info.emacs_mule_id = Some(mule_id);
            info.plist = vec![(intern(":name"), Value::from_sym_id(name))];
            registry.register(info);
        }
    });
    let result = |name, character| {
        CHARSET_REGISTRY.with(|slot| {
            let registry = slot.borrow();
            let info = registry.charsets.get(&name).expect("registered fixture");
            (
                info.emacs_mule_id.expect("Mule fixture"),
                info.dimension,
                registry
                    .encode_char(name, character)
                    .expect("fixture character"),
            )
        })
    };
    let original_result = result(original, ch);
    let target_result = result(target, ch);
    let original_only_result = result(original, original_only);
    assert_eq!(
        EmacsMuleEncoder::new().encode_char(ch),
        Some(original_result)
    );
    CHARSET_REGISTRY.with(|slot| slot.borrow_mut().define_alias(original, target));
    assert_eq!(charset_encode_char(original, ch), Some(target_result.2));
    // GNU's Mule list stores charset identities, independently of an alias
    // replacing a symbol's lookup entry. Its old member still encodes here.
    assert_eq!(
        EmacsMuleEncoder::new().encode_char(ch),
        Some(original_result)
    );
    assert_eq!(
        EmacsMuleEncoder::new().encode_char(original_only),
        Some(original_only_result)
    );

    builtin_set_charset_priority(vec![Value::from_sym_id(original)])
        .expect("the colliding alias now names the target");
    assert_eq!(EmacsMuleEncoder::new().encode_char(ch), Some(target_result));
    builtin_set_charset_priority(vec![Value::symbol("ascii")])
        .expect("unrelated priority change preserves target precedence");
    assert_eq!(EmacsMuleEncoder::new().encode_char(ch), Some(target_result));
    assert_eq!(
        EmacsMuleEncoder::new().encode_char(original_only),
        Some(original_only_result)
    );
    let snapshot = snapshot_charset_registry();
    reset_charset_registry();
    restore_charset_registry(snapshot);
    assert_eq!(EmacsMuleEncoder::new().encode_char(ch), Some(target_result));
    assert_eq!(
        EmacsMuleEncoder::new().encode_char(original_only),
        Some(original_only_result)
    );
}

#[test]
fn prepared_mule_preserves_legacy_alias_redefinition_fallback() {
    crate::test_utils::init_test_tracing();
    let _context = Context::new();
    let base = intern("neovm-mule-redefinition-base");
    let alias = intern("neovm-mule-redefinition-alias");
    CHARSET_REGISTRY.with(|slot| {
        let mut registry = slot.borrow_mut();
        let mut info = CharsetRegistry::make_default(150, resolve_sym(base));
        info.code_space = [33, 126, 0, 0, 0, 0, 0, 0];
        info.min_code = 33;
        info.max_code = 126;
        info.emacs_mule_id = Some(150);
        info.method = CharsetMethod::Offset(0x1f300);
        info.plist = vec![(intern(":name"), Value::from_sym_id(base))];
        registry.register(info.clone());
        registry.define_alias(alias, base);
        // Existing Neo definition-through-alias behavior changes the raw
        // alias entry rather than GNU's underlying charset identity. Preserve
        // that fallback here; the alias entry must not become a Mule member.
        info.name = alias;
        info.method = CharsetMethod::Offset(0x1f400);
        info.plist = vec![(intern(":name"), Value::from_sym_id(alias))];
        registry.register(info);
    });
    for character in [0x1f300, 0x1f350, 0x1f400] {
        let legacy = emacs_mule_encode_char(character);
        assert_eq!(EmacsMuleEncoder::new().encode_char(character), legacy);
    }
    let snapshot = snapshot_charset_registry();
    reset_charset_registry();
    restore_charset_registry(snapshot);
    for character in [0x1f300, 0x1f350, 0x1f400] {
        let legacy = emacs_mule_encode_char(character);
        assert_eq!(EmacsMuleEncoder::new().encode_char(character), legacy);
    }
}

#[test]
fn prepared_mule_restores_legacy_materialized_alias_members_once() {
    crate::test_utils::init_test_tracing();
    let _context = Context::new();
    let base = intern("neovm-mule-legacy-base");
    let alias = intern("neovm-mule-legacy-alias");
    CHARSET_REGISTRY.with(|slot| {
        let mut registry = slot.borrow_mut();
        let mut info = CharsetRegistry::make_default(152, resolve_sym(base));
        info.code_space = [33, 126, 0, 0, 0, 0, 0, 0];
        info.min_code = 33;
        info.max_code = 126;
        info.emacs_mule_id = Some(152);
        info.method = CharsetMethod::Offset(0x1f300);
        info.plist = vec![(intern(":name"), Value::from_sym_id(base))];
        registry.register(info);
        registry.define_alias(alias, base);
    });
    let expected = EmacsMuleEncoder::new().encode_char(0x1f300);
    let mut snapshot = snapshot_charset_registry();
    assert!(snapshot.charsets.iter().any(|info| info.name == alias));
    snapshot.priority_identities = None;
    snapshot.aliases = None;
    snapshot.priority.insert(0, alias);
    snapshot.emacs_mule_order.insert(0, alias);
    reset_charset_registry();
    restore_charset_registry(snapshot);
    assert_eq!(EmacsMuleEncoder::new().encode_char(0x1f300), expected);
    let snapshot = snapshot_charset_registry();
    assert_eq!(
        snapshot
            .emacs_mule_order
            .iter()
            .filter(|&&name| name == base)
            .count(),
        1
    );
    assert!(!snapshot.emacs_mule_order.contains(&alias));

    // Explicit modern metadata is authoritative even if a stale materialized
    // alias entry remains in the dump mirror. It cannot create a new member.
    let mut snapshot = snapshot;
    snapshot.aliases = Some(Vec::new());
    snapshot.emacs_mule_order.clear();
    reset_charset_registry();
    restore_charset_registry(snapshot);
    assert_eq!(EmacsMuleEncoder::new().encode_char(0x1f300), None);
}
