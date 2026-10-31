use super::super::effect_parallel::{
    ADMITTED, CatalogRow, ParallelCatalogError, canonical_parallel_error,
};
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
    // Main added chrome controls and replaced subrp's macro registration.
    // Check both active installation and the cold World classification.
    for name in [
        "neomacs-set-scroll-indicators",
        "neomacs-set-extra-spacing",
        "subrp",
    ] {
        assert!(
            installed.contains(name),
            "missing rebased primitive: {name}"
        );
        let row = INVENTORY
            .iter()
            .find(|entry| entry.kind == CatalogKind::Primitive && entry.name == name)
            .expect("rebased primitive has a cold inventory row");
        assert_eq!(row.authority, Authority::World);
    }
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
        opcode_class(&Op::TrapOutOfRangeConstant(0)),
        ParallelClass::Scalar
    );
    for (name, class) in OPCODES {
        let row = INVENTORY
            .iter()
            .find(|row| row.kind == CatalogKind::Opcode && row.name == *name)
            .expect("opcode inventory row");
        assert_eq!(*class, row.class, "opcode class {name}");
    }
    let tier_i_names = TIER_I_OPCODES
        .iter()
        .map(|(name, class)| {
            assert_eq!(*class, ParallelClass::World);
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
        let expected = match declaration.id {
            leaf::LeafId::Gethash => ParallelClass::Sealed(SealedAuthority::HashRead),
            leaf::LeafId::Keywordp | leaf::LeafId::SymbolName => {
                ParallelClass::Sealed(SealedAuthority::CanonicalSymbol)
            }
            _ => ParallelClass::World,
        };
        assert_eq!(leaf_class(declaration.id), expected);
    }
    for (name, class) in LEAVES {
        let row = INVENTORY
            .iter()
            .find(|row| row.kind == CatalogKind::Leaf && row.name == *name)
            .expect("leaf inventory row");
        assert_eq!(*class, row.class, "leaf class {name}");
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

fn parallel_rows() -> Vec<CatalogRow<'static>> {
    INVENTORY
        .iter()
        .map(|entry| CatalogRow {
            kind: entry.kind,
            name: entry.name,
            class: entry.class,
            variant: entry.variant,
            alias_target: entry.target,
        })
        .collect()
}

fn parallel_class_of(name: &str) -> ParallelClass {
    INVENTORY
        .iter()
        .find(|entry| entry.kind == CatalogKind::Primitive && entry.name == name)
        .unwrap_or_else(|| panic!("missing primitive {name}"))
        .class
}

fn parallel_variant_of(kind: CatalogKind, name: &str) -> Option<ParallelVariant> {
    INVENTORY
        .iter()
        .find(|entry| entry.kind == kind && entry.name == name)
        .unwrap_or_else(|| panic!("missing {kind:?} {name}"))
        .variant
}

#[test]
fn pentry_catalog_parallel_tables_are_dense_and_canonical() {
    assert_eq!(PARALLEL_CLASSES.len(), INVENTORY.len());
    assert_eq!(PARALLEL_VARIANTS.len(), INVENTORY.len());
    for (index, entry) in INVENTORY.iter().enumerate() {
        assert_eq!(
            entry.class, PARALLEL_CLASSES[index],
            "class row {}",
            entry.name
        );
        assert_eq!(
            entry.variant, PARALLEL_VARIANTS[index],
            "variant row {}",
            entry.name
        );
    }
    let rows = parallel_rows();
    canonical_parallel_error(&rows, ADMITTED)
        .expect("checked-in table equals the canonical P3 classification");
    let (mut world, mut sealed, mut scalar) = (0, 0, 0);
    for entry in INVENTORY {
        match entry.class {
            ParallelClass::World => world += 1,
            ParallelClass::Sealed(_) => sealed += 1,
            ParallelClass::Scalar => scalar += 1,
        }
    }
    assert_eq!((world, sealed, scalar), (1_664, 41, 34));
    eprintln!(
        "parallel catalog: entries={} world={} sealed={} scalar={} typed_variants={} declarations={}",
        INVENTORY.len(),
        world,
        sealed,
        scalar,
        PARALLEL_VARIANTS
            .iter()
            .filter(|variant| variant.is_some())
            .count(),
        ADMITTED.len()
    );
}

#[test]
fn pentry_catalog_parallel_aliases_inherit_resolved_targets() {
    let alias_class = |name: &str| {
        INVENTORY
            .iter()
            .find(|entry| entry.kind == CatalogKind::Alias && entry.name == name)
            .unwrap_or_else(|| panic!("missing alias {name}"))
            .class
    };
    assert_eq!(alias_class("not"), ParallelClass::Scalar);
    assert_eq!(
        alias_class("error-type"),
        ParallelClass::Sealed(SealedAuthority::ConsFieldRead)
    );
    assert_eq!(
        alias_class("rplaca"),
        ParallelClass::Sealed(SealedAuthority::ConsFieldWrite)
    );
    assert_eq!(
        alias_class("rplacd"),
        ParallelClass::Sealed(SealedAuthority::ConsFieldWrite)
    );
    // World targets, a missing literal target and a target that is not a
    // primitive row all stay World, exactly as in P3.
    assert_eq!(alias_class("sxhash"), ParallelClass::World);
    assert_eq!(alias_class("ctl-x-4-prefix"), ParallelClass::World);
    assert_eq!(alias_class("error-slot-value"), ParallelClass::World);
    assert_eq!(
        alias_target_class(AliasTarget::Native("null")),
        Some(ParallelClass::Scalar)
    );
    assert_eq!(
        alias_target_class(AliasTarget::Native("car")),
        Some(ParallelClass::Sealed(SealedAuthority::ConsFieldRead))
    );
    assert_eq!(
        alias_target_class(AliasTarget::Native("pentry-unknown-native")),
        None
    );
    assert_eq!(
        alias_target_class(AliasTarget::Interpreted),
        Some(ParallelClass::World)
    );
}

#[test]
fn pentry_catalog_parallel_typed_variants_are_exactly_designated() {
    assert_eq!(
        parallel_variant_of(CatalogKind::Opcode, "Return"),
        Some(ParallelVariant::Return(ReturnDisposition::OwnerLocal))
    );
    for (name, access) in [
        ("VarRef", PlainVarAccess::Ref),
        ("VarSet", PlainVarAccess::Set),
        ("VarBind", PlainVarAccess::Bind),
        ("Unbind", PlainVarAccess::Unbind),
    ] {
        assert_eq!(
            parallel_variant_of(CatalogKind::Opcode, name),
            Some(ParallelVariant::PlainVar(access))
        );
    }
    for (kind, name) in [
        (CatalogKind::Opcode, "Add"),
        (CatalogKind::Opcode, "Add1"),
        (CatalogKind::Opcode, "Eqlsign"),
        (CatalogKind::Primitive, "+"),
        (CatalogKind::Primitive, "mod"),
        (CatalogKind::Primitive, "abs"),
    ] {
        assert_eq!(
            parallel_variant_of(kind, name),
            Some(ParallelVariant::Arith(ArithOperands::Numeric))
        );
    }
    let typed = INVENTORY
        .iter()
        .filter(|entry| entry.variant.is_some())
        .count();
    assert_eq!(typed, 37);
    assert_eq!(
        ADMITTED
            .iter()
            .filter(|declaration| declaration.variant.is_some())
            .count(),
        37
    );
    // E4 refines the executor split; the canonical P3 class stays World.
    for entry in INVENTORY.iter().filter(|entry| entry.variant.is_some()) {
        assert_eq!(entry.class, ParallelClass::World, "{}", entry.name);
    }
    assert_eq!(
        parallel_class_of("car"),
        ParallelClass::Sealed(SealedAuthority::ConsFieldRead)
    );
}

#[test]
fn pentry_catalog_parallel_gate_rejects_a_class_flip() {
    let rows = parallel_rows();

    // One Sealed declaration flipped to Scalar no longer agrees with the
    // generated table.
    let mut flipped_declarations = ADMITTED.to_vec();
    let car = flipped_declarations
        .iter_mut()
        .find(|declaration| declaration.kind == CatalogKind::Primitive && declaration.name == "car")
        .expect("car declaration");
    car.class = ParallelClass::Scalar;
    car.variant = None;
    assert!(matches!(
        canonical_parallel_error(&rows, &flipped_declarations),
        Err(ParallelCatalogError::DeclarationMismatch {
            kind: CatalogKind::Primitive,
            name: "car"
        })
    ));

    // If the generator applied that flip, car and its alias leave the
    // canonical Sealed total and the gate fails.
    let mut flipped_rows = rows.clone();
    for row in &mut flipped_rows {
        if row.name == "car" || row.name == "error-type" {
            row.class = ParallelClass::Scalar;
        }
    }
    assert!(matches!(
        canonical_parallel_error(&flipped_rows, &flipped_declarations),
        Err(ParallelCatalogError::CountMismatch {
            what: "canonical Sealed names",
            expected: 41,
            found: 39
        })
    ));

    // A new World registration defaults in but must be reviewed into the
    // canonical totals (POST_P3_WORLD_REGISTRATIONS) before the gate passes.
    let mut extended = rows.clone();
    extended.push(CatalogRow {
        kind: CatalogKind::Primitive,
        name: "pentry-new-registration",
        class: ParallelClass::World,
        variant: None,
        alias_target: None,
    });
    assert!(matches!(
        canonical_parallel_error(&extended, ADMITTED),
        Err(ParallelCatalogError::CountMismatch {
            what: "canonical names",
            ..
        })
    ));

    // A row claiming Sealed without an evidence-bearing declaration fails.
    let mut silent = rows.clone();
    let nconc = silent
        .iter_mut()
        .find(|row| row.kind == CatalogKind::Opcode && row.name == "Nconc")
        .expect("Nconc row");
    nconc.class = ParallelClass::Sealed(SealedAuthority::ConsFieldWrite);
    assert!(matches!(
        canonical_parallel_error(&silent, ADMITTED),
        Err(ParallelCatalogError::UndeclaredClass {
            kind: CatalogKind::Opcode,
            name: "Nconc"
        })
    ));
}
