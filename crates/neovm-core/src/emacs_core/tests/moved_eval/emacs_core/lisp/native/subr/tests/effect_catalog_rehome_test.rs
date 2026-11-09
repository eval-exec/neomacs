//! The effect-catalog checks that need an upper crate.
//!
//! The inventory tables stay in eval. These checks observe them through the
//! plain-data fences and call this crate's localized subr catalog directly.
//! `pentry_catalog_covers_shipped_and_loaded_alias_cells` is the moved test.
//! `localized_subr_catalog_entries_are_installed_world_primitives` is the loop
//! that used to sit inside `pentry_catalog_covers_every_installed_subr_opcode_leaf_and_alias`.
//!
//! That parent test's constructor is `Context::new(unit_runtime())`. The
//! unit runtime is `cfg(test)` in eval, and the absent-layer guard rejects
//! compiling it under `test-support`, so this crate cannot call it. The
//! catalog this loop walks is the compiled batch list `new_context()`
//! installs. The installed-set construction, the spec iteration, and the
//! classification assert are the original ones. Every other assertion in
//! the parent test still runs on `unit_runtime()`.

use std::collections::HashSet;

use neovm_eval::test_support::authority::{
    alias_authority_is_world_for_test, lookup_global_subr_entry,
    primitive_inventory_names_for_test, shipped_alias_rows_for_test,
};
use neovm_value::intern::resolve_sym;

#[test]
fn localized_subr_catalog_entries_are_installed_world_primitives() {
    use crate::emacs_core::subr::WorldSubr;
    use crate::emacs_core::subr::effect_schema::{Authority, Classification};

    let ctx = crate::test_utils::new_context();
    let primitive_names = primitive_inventory_names_for_test()
        .into_iter()
        .collect::<HashSet<_>>();
    let mut installed = HashSet::new();
    for (id, _) in ctx.obarray().iter_symbols() {
        if lookup_global_subr_entry(id).is_some() {
            let name = resolve_sym(id);
            assert!(
                primitive_names.contains(name),
                "unclassified installed subr: {name}"
            );
            installed.insert(name);
        }
    }
    let mut localized = 0usize;
    for batch in crate::emacs_core::builtins::localized_subr_catalog() {
        for spec in batch.specs() {
            localized += 1;
            assert!(installed.contains(spec.name()));
            let declaration = WorldSubr::new(*spec).declaration();
            assert_eq!(
                declaration.classification,
                Classification::Classified(Authority::World)
            );
        }
    }
    eprintln!("localized_subr_catalog entries={localized}");
}

#[test]
fn pentry_catalog_covers_shipped_and_loaded_alias_cells() {
    let ctx = crate::test_utils::runtime_startup_context();
    let aliases = shipped_alias_rows_for_test(&ctx);
    assert!(!aliases.is_empty());
    for (_name, source, authority_is_world) in &aliases {
        assert_eq!(*source, "lisp/subr.el");
        assert!(*authority_is_world);
    }
    let mut loaded = 0;
    for (id, _) in ctx.obarray().iter_symbols() {
        if ctx
            .obarray()
            .symbol_function_id(id)
            .is_some_and(|value| value.as_bare_symbol_id().is_some())
        {
            loaded += 1;
            assert!(alias_authority_is_world_for_test(&ctx, resolve_sym(id)));
        }
    }
    assert!(
        loaded > 0,
        "loaded runtime must exercise actual alias cells"
    );
}
