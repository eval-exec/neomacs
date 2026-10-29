use super::*;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::eval::{Context, lookup_global_subr_entry};
use crate::emacs_core::intern::{intern, resolve_sym};
use crate::emacs_core::subr::leaf;
use crate::emacs_core::value::list_to_vec;
use std::collections::{BTreeMap, HashSet};

#[test]
fn pentry_catalog_covers_every_installed_subr_opcode_leaf_and_alias() {
    let mut ctx = Context::new();
    let primitive_names = INVENTORY
        .iter()
        .filter(|entry| entry.kind == CatalogKind::Primitive)
        .map(|entry| entry.name)
        .collect::<HashSet<_>>();
    let mut installed = HashSet::new();
    for (id, _) in ctx.obarray.iter_symbols() {
        if lookup_global_subr_entry(id).is_some() {
            let name = resolve_sym(id);
            assert!(
                primitive_names.contains(name),
                "unclassified installed subr: {name}"
            );
            installed.insert(name);
        }
    }
    assert!(!installed.is_empty());
    for batch in crate::emacs_core::builtins::localized_subr_catalog() {
        for spec in batch.specs() {
            assert!(installed.contains(spec.name()));
            let declaration = crate::emacs_core::subr::WorldSubr::new(*spec).declaration();
            assert_eq!(
                declaration.classification,
                super::super::effect_schema::Classification::Classified(Authority::World)
            );
        }
    }

    // Generated exhaustive matches are compiled against the real enums. Tables
    // have one source row per variant, including the trap opcode and opcode leaves.
    let opcode_names = OPCODES
        .iter()
        .map(|(name, _)| *name)
        .collect::<HashSet<_>>();
    assert_eq!(opcode_names.len(), OPCODES.len());
    assert!(opcode_names.contains("TrapOutOfRangeConstant"));
    assert_eq!(
        opcode_authority(&Op::TrapOutOfRangeConstant(0)),
        Authority::World
    );
    let tier_i_names = TIER_I_OPCODES
        .iter()
        .map(|(name, authority)| {
            assert_eq!(*authority, Authority::World);
            *name
        })
        .collect::<HashSet<_>>();
    assert!(!tier_i_names.is_empty());
    assert_eq!(tier_i_names.len(), TIER_I_OPCODES.len());
    let tier_i_rows = INVENTORY
        .iter()
        .filter(|row| row.kind == CatalogKind::Opcode && row.name.starts_with("TierI::"))
        .map(|row| {
            assert_eq!(row.authority, Authority::World);
            row.name
        })
        .collect::<HashSet<_>>();
    assert_eq!(tier_i_names, tier_i_rows);
    assert_eq!(LEAVES.len(), leaf::LEAVES.len());
    let leaf_names = LEAVES.iter().map(|(name, _)| *name).collect::<HashSet<_>>();
    assert_eq!(leaf_names.len(), leaf::LeafId::COUNT);
    for declaration in leaf::LEAVES {
        assert!(leaf_names.contains(format!("{:?}", declaration.id).as_str()));
        assert_eq!(leaf_authority(declaration.id), Authority::World);
    }

    // Runtime alias names cannot grant permanent authority. The same current
    // target resolver covers aliases created after the build inventory.
    ctx.obarray
        .set_symbol_function("pentry-alias-1", Value::symbol("null"));
    ctx.obarray
        .set_symbol_function("pentry-alias-2", Value::symbol("pentry-alias-1"));
    assert_eq!(
        alias_authority(&ctx, "pentry-alias-2"),
        Some(Authority::World)
    );
    assert_eq!(
        current_alias_target(&ctx, "pentry-alias-2"),
        AliasTarget::Native("null")
    );
    ctx.obarray
        .set_symbol_function("pentry-alias-1", Value::symbol("set-marker"));
    assert_eq!(
        alias_authority(&ctx, "pentry-alias-2"),
        Some(Authority::World)
    );
    assert_eq!(
        current_alias_target(&ctx, "pentry-alias-2"),
        AliasTarget::Native("set-marker")
    );
    ctx.obarray
        .set_symbol_function("pentry-alias-1", Value::symbol("pentry-alias-2"));
    assert_eq!(
        current_alias_target(&ctx, "pentry-alias-2"),
        AliasTarget::Cycle
    );
    assert_eq!(
        alias_authority(&ctx, "pentry-alias-2"),
        Some(Authority::World)
    );
    assert_eq!(
        alias_authority(&ctx, "pentry-unbound-alias"),
        Some(Authority::World)
    );
    assert_eq!(
        alias_target_authority(AliasTarget::Native("pentry-unknown-native")),
        None
    );
    assert_eq!(
        alias_target_authority(AliasTarget::Interpreted),
        Some(Authority::World)
    );
    assert!(
        INVENTORY
            .iter()
            .all(|entry| entry.authority == Authority::World)
    );
    eprintln!(
        "pentry inventory: installed_subrs={} source_primitives={} bytecode_opcodes={} tier_i_opcodes={} leaves={} aliases={}",
        installed.len(),
        primitive_names.len(),
        OPCODES.len(),
        TIER_I_OPCODES.len(),
        LEAVES.len(),
        INVENTORY
            .iter()
            .filter(|row| row.kind == CatalogKind::Alias)
            .count()
    );
}

fn current_alias_target(ctx: &Context, name: &str) -> AliasTarget<'static> {
    let mut id = intern(name);
    let mut visited = HashSet::new();
    while visited.insert(id) {
        let Some(value) = ctx.obarray.symbol_function_id(id) else {
            return AliasTarget::Void;
        };
        if let Some(next) = value.as_bare_symbol_id() {
            id = next;
        } else if value.is_subr() {
            return AliasTarget::Native(resolve_sym(value.as_subr_id().expect("subr identity")));
        } else {
            return AliasTarget::Interpreted;
        }
    }
    AliasTarget::Cycle
}

fn alias_authority(ctx: &Context, name: &str) -> Option<Authority> {
    alias_target_authority(current_alias_target(ctx, name))
}

/// Inspect reader output, independently of the generator's line-oriented
/// extraction. These are source diagnostics; runtime authority follows cells.
fn quoted_symbol(value: Value) -> Option<&'static str> {
    let expression = list_to_vec(&value)?;
    if expression.len() == 2
        && matches!(
            expression[0].as_bare_symbol_name(),
            Some("quote" | "function")
        )
    {
        expression[1].as_bare_symbol_name()
    } else {
        None
    }
}

#[test]
fn pentry_catalog_shipped_alias_rows_match_lisp_reader_forms() {
    let ctx = Context::new();
    let path = neomacs_infra::crate_root!().join("../../lisp/subr.el");
    let source = std::fs::read_to_string(path).expect("shipped subr.el source");
    // This only reads repository Lisp; it evaluates no forms.
    let forms = crate::emacs_core::value_reader::read_all(&source, ctx.variables())
        .expect("read shipped subr.el forms");
    let mut declared = BTreeMap::new();
    for form in forms {
        let Some(expression) = list_to_vec(&form) else {
            continue;
        };
        if expression
            .first()
            .and_then(|head| head.as_bare_symbol_name())
            != Some("defalias")
        {
            continue;
        }
        assert!(expression.len() >= 3, "complete top-level defalias form");
        let name = quoted_symbol(expression[1]).expect("literal top-level alias name");
        let target = quoted_symbol(expression[2]);
        assert!(
            declared.insert(name, target).is_none(),
            "duplicate alias {name}"
        );
    }
    let mut generated = BTreeMap::new();
    for row in INVENTORY
        .iter()
        .filter(|row| row.kind == CatalogKind::Alias)
    {
        assert_eq!(row.source, "lisp/subr.el");
        assert!(generated.insert(row.name, row.target).is_none());
    }
    assert!(!declared.is_empty());
    assert_eq!(
        generated, declared,
        "complete shipped alias source registry"
    );
}

#[test]
fn pentry_catalog_covers_shipped_and_loaded_alias_cells() {
    let ctx = crate::test_utils::runtime_startup_context();
    let aliases = INVENTORY
        .iter()
        .filter(|row| row.kind == CatalogKind::Alias)
        .collect::<Vec<_>>();
    assert!(!aliases.is_empty());
    for row in &aliases {
        assert_eq!(row.source, "lisp/subr.el");
        assert_eq!(alias_authority(&ctx, row.name), Some(Authority::World));
    }
    let mut loaded = 0;
    for (id, _) in ctx.obarray.iter_symbols() {
        if ctx
            .obarray
            .symbol_function_id(id)
            .is_some_and(|value| value.as_bare_symbol_id().is_some())
        {
            loaded += 1;
            assert_eq!(
                alias_authority(&ctx, resolve_sym(id)),
                Some(Authority::World)
            );
        }
    }
    assert!(
        loaded > 0,
        "loaded runtime must exercise actual alias cells"
    );
}
