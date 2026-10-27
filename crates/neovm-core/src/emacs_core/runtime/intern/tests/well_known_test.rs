//! The DEFSYM table: fixed ids, names, and canonical registry identities.

use super::super::{
    FIRST_WELL_KNOWN_SYM_ID, WELL_KNOWN_NAMES, WellKnown, intern, is_canonical_id, lookup_interned,
    resolve_sym,
};

#[test]
fn table_ids_names_and_registry_identities_agree() {
    let _ctx = crate::emacs_core::Context::new();
    for (index, variant) in WellKnown::VARIANTS.iter().copied().enumerate() {
        assert_eq!(variant.name(), WELL_KNOWN_NAMES[index]);
        assert_eq!(variant.id().0, FIRST_WELL_KNOWN_SYM_ID + index as u32);
        assert_eq!(resolve_sym(variant.id()), variant.name());
        assert!(is_canonical_id(variant.id()));
        assert_eq!(lookup_interned(variant.name()), Some(variant.id()));
        assert_eq!(intern(variant.name()), variant.id());
    }
}

#[test]
fn table_identity_matches_the_string_constructor() {
    let _ctx = crate::emacs_core::Context::new();
    for variant in WellKnown::VARIANTS.iter().copied() {
        let fixed = crate::emacs_core::Value::symbol(variant);
        let text = crate::emacs_core::Value::symbol(variant.name());
        assert_eq!(fixed.bits(), text.bits(), "{}", variant.name());
        assert_eq!(
            fixed.is_bare_keyword(),
            text.is_bare_keyword(),
            "{}",
            variant.name()
        );
    }
}

/// GNU's DEFSYM model in one probe: `unintern` removes only the name ->
/// symbol mapping (`Funintern`); every C-side static keeps the ORIGINAL
/// symbol object, name and value cell, while Lisp interning the spelling
/// again produces a fresh, unbound symbol.
///
/// The registry is process-global, so the destructive half runs in a child
/// process. nextest already isolates tests, but a plain `cargo test` shares
/// one process and must not inherit the uninterned name.
#[test]
fn unintern_keeps_the_well_known_identity_while_lisp_reinterns() {
    const CHILD_ENV: &str = "NEOVM_WELL_KNOWN_UNINTERN_CHILD";
    if std::env::var_os(CHILD_ENV).is_none() {
        let exe = std::env::current_exe().expect("test executable path");
        let module = module_path!();
        let test_path = format!(
            "{}::unintern_keeps_the_well_known_identity_while_lisp_reinterns",
            module.strip_prefix("neovm_core::").unwrap_or(module)
        );
        let status = std::process::Command::new(exe)
            .args(["--exact", &test_path, "--nocapture"])
            .env(CHILD_ENV, "1")
            .status()
            .expect("spawn isolated unintern child");
        assert!(status.success(), "isolated unintern child failed");
        return;
    }

    use crate::emacs_core::Value;
    let mut ctx = crate::emacs_core::Context::new();
    let symbol = WellKnown::ModeLine;
    let original = symbol.id();
    ctx.assign_by_id(original, Value::fixnum(42));
    assert_eq!(
        ctx.obarray.symbol_value_id_copied(original),
        Some(Value::fixnum(42))
    );

    assert!(super::super::unintern_canonical_id(original));

    // C side: the static still denotes the original symbol object.
    assert_eq!(resolve_sym(original), symbol.name());
    assert!(!is_canonical_id(original));
    assert_eq!(
        ctx.obarray.symbol_value_id_copied(original),
        Some(Value::fixnum(42)),
        "the old symbol keeps its value cell after unintern"
    );
    assert_eq!(
        Value::symbol(symbol).bits(),
        Value::from_sym_id(original).bits(),
        "the fixed constructor keeps pointing at the original symbol"
    );

    // Lisp side: `(intern "mode-line")` builds a new, unbound symbol.
    let fresh = intern(symbol.name());
    assert_ne!(fresh, original);
    assert_eq!(lookup_interned(symbol.name()), Some(fresh));
    assert_eq!(resolve_sym(fresh), symbol.name());
    assert_eq!(ctx.obarray.symbol_value_id_copied(fresh), None);

    // A by-name read resolves to the fresh symbol, never re-resolving the
    // fixed identity (the retired EpochCachedSymbol behaviour).
    assert_eq!(intern(symbol.name()), fresh);
}
