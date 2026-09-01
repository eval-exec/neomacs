use super::super::intern::intern;
use super::*;
use crate::emacs_core::intern::resolve_sym;

#[test]
fn set_internal_bind_codes_match_gnu_set_internal_bind() {
    crate::test_utils::init_test_tracing();

    let cases = [
        (SetInternalBind::Set, 0),
        (SetInternalBind::Bind, 1),
        (SetInternalBind::Unbind, 2),
        (SetInternalBind::ThreadSwitch, 3),
    ];

    for (bind, code) in cases {
        assert_eq!(bind.gnu_code(), code);
        assert_eq!(SetInternalBind::from_gnu_code(code), Some(bind));
    }
    assert_eq!(SetInternalBind::from_gnu_code(4), None);
}

#[test]
fn symbol_redirect_codes_match_gnu_symbol_redirect() {
    crate::test_utils::init_test_tracing();

    let cases = [
        (SymbolRedirect::Plainval, 0),
        (SymbolRedirect::Varalias, 1),
        (SymbolRedirect::Localized, 2),
        (SymbolRedirect::Forwarded, 3),
    ];

    for (redirect, code) in cases {
        assert_eq!(redirect.gnu_code(), code);
        assert_eq!(SymbolRedirect::from_gnu_code(code), Some(redirect));
    }
    assert_eq!(SymbolRedirect::from_gnu_code(4), None);
}

#[test]
fn define_lisp_variable_encodes_specialness_and_locality_together() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();

    ob.define_lisp_variable(
        "defvar-global",
        Value::fixnum(41),
        LispVariableLocality::Global,
    );
    assert_eq!(
        ob.symbol_value("defvar-global").copied(),
        Some(Value::fixnum(41))
    );
    assert!(ob.is_special("defvar-global"));
    assert!(!ob.is_buffer_local("defvar-global"));

    ob.define_lisp_variable(
        "defvar-local-if-set",
        Value::fixnum(42),
        LispVariableLocality::BufferLocalIfSet,
    );
    let local_id = intern("defvar-local-if-set");
    assert_eq!(
        ob.symbol_value("defvar-local-if-set").copied(),
        Some(Value::fixnum(42))
    );
    assert!(ob.is_special("defvar-local-if-set"));
    assert!(ob.is_buffer_local("defvar-local-if-set"));
    assert!(ob.blv(local_id).is_some_and(|binding| binding.local_if_set));
}

#[test]
fn symbol_trapped_write_codes_match_gnu_symbol_trapped_write() {
    crate::test_utils::init_test_tracing();

    let cases = [
        (SymbolTrappedWrite::Untrapped, 0),
        (SymbolTrappedWrite::NoWrite, 1),
        (SymbolTrappedWrite::Trapped, 2),
    ];

    for (trapped_write, code) in cases {
        assert_eq!(trapped_write.gnu_code(), code);
        assert_eq!(SymbolTrappedWrite::from_gnu_code(code), Some(trapped_write));
    }
    assert_eq!(SymbolTrappedWrite::from_gnu_code(3), None);
}

#[test]
fn symbol_interned_codes_match_gnu_symbol_interned() {
    crate::test_utils::init_test_tracing();

    let cases = [
        (SymbolInterned::Uninterned, 0),
        (SymbolInterned::Interned, 1),
        (SymbolInterned::InternedInInitial, 2),
    ];

    for (interned, code) in cases {
        assert_eq!(interned.gnu_code(), code);
        assert_eq!(SymbolInterned::from_gnu_code(code), Some(interned));
    }
    assert_eq!(SymbolInterned::from_gnu_code(3), None);
}

#[test]
fn intern_creates_symbol() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    ob.intern("foo");
    assert!(ob.intern_soft("foo").is_some());
    assert!(ob.intern_soft("bar").is_none());
}

#[test]
fn symbol_value_cell() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    assert!(!ob.boundp("x"));
    ob.set_symbol_value("x", Value::fixnum(42));
    assert!(ob.boundp("x"));
    assert_eq!(ob.symbol_value("x").unwrap().as_int(), Some(42));
}

#[test]
fn symbol_value_id_or_nil_matches_value_cell_reads() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();

    let plain = intern("symbol-value-id-or-nil-plain");
    ob.set_symbol_value_id(plain, Value::fixnum(42));
    assert_eq!(ob.symbol_value_id_or_nil(plain), Value::fixnum(42));

    let alias = intern("symbol-value-id-or-nil-alias");
    ob.make_alias(alias, plain);
    assert_eq!(ob.symbol_value_id_or_nil(alias), Value::fixnum(42));

    let localized = intern("symbol-value-id-or-nil-localized");
    ob.make_symbol_localized(localized, Value::fixnum(7));
    assert_eq!(ob.symbol_value_id_or_nil(localized), Value::fixnum(7));

    let missing = intern("symbol-value-id-or-nil-missing");
    assert_eq!(ob.symbol_value_id_or_nil(missing), Value::NIL);
}

#[test]
fn symbol_function_cell() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    assert!(!ob.fboundp("f"));
    let start_epoch = ob.function_epoch();
    ob.set_symbol_function("f", Value::subr(intern("+")));
    assert!(ob.fboundp("f"));
    assert!(ob.function_epoch() > start_epoch);
    let after_set_epoch = ob.function_epoch();
    ob.fmakunbound("f");
    assert!(!ob.fboundp("f"));
    assert!(ob.function_epoch() > after_set_epoch);
}

#[test]
fn fmakunbound_masks_builtin_fallback_name() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let start_epoch = ob.function_epoch();
    ob.fmakunbound("car");
    assert!(ob.is_function_unbound("car"));
    assert!(!ob.fboundp("car"));
    assert!(ob.symbol_function("car").is_none());
    assert!(ob.function_epoch() > start_epoch);

    ob.set_symbol_function("car", Value::subr(intern("car")));
    assert!(!ob.is_function_unbound("car"));
    assert!(ob.fboundp("car"));
}

#[test]
fn symbol_properties() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    ob.put_property("foo", "doc", Value::string("A function."))
        .unwrap();
    assert_eq!(
        ob.get_property("foo", "doc").unwrap().as_utf8_str(),
        Some("A function.")
    );
}

#[test]
fn special_flag() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    assert!(!ob.is_special("x"));
    ob.make_special("x");
    assert!(ob.is_special("x"));
}

#[test]
fn indirect_function_follows_chain() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    ob.set_symbol_function("real-fn", Value::subr(intern("+")));
    // alias -> real-fn
    ob.set_symbol_function("alias", Value::symbol(intern("real-fn")));
    let resolved = ob.indirect_function("alias").unwrap();
    assert_eq!(resolved.as_subr_id().map(resolve_sym), Some("+"));
}

#[test]
fn t_and_nil_are_preinterned() {
    crate::test_utils::init_test_tracing();
    let ob = Obarray::new();
    assert!(ob.is_constant("t"));
    assert!(ob.is_constant("nil"));
    assert!(ob.is_constant(":keyword"));
    assert!(ob.is_special("t"));
    assert!(ob.is_special("nil"));
}

#[test]
fn interning_keyword_materializes_gnu_self_evaluating_symbol_state() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    ob.intern(":vm-keyword");
    assert!(ob.is_constant(":vm-keyword"));
    assert!(ob.is_special(":vm-keyword"));
    assert_eq!(
        ob.symbol_value(":vm-keyword"),
        Some(&Value::keyword(":vm-keyword"))
    );
}

#[test]
fn makunbound_doesnt_touch_constants() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    ob.makunbound("t");
    assert!(ob.boundp("t")); // t is constant, can't unbind
}

#[test]
fn canonical_id_mutators_keep_symbol_globally_interned() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let sym = intern("vm-ghost");

    ob.set_symbol_value_id(sym, Value::fixnum(1));
    assert!(ob.intern_soft("vm-ghost").is_some());
    assert!(ob.all_symbols().contains(&"vm-ghost"));

    ob.put_property_id(sym, intern("vm-prop"), Value::fixnum(2))
        .unwrap();
    assert_eq!(
        ob.get_property("vm-ghost", "vm-prop"),
        Some(Value::fixnum(2))
    );

    ob.set_symbol_function_id(sym, Value::subr(intern("+")));
    assert!(ob.fboundp("vm-ghost"));

    ob.make_special_id(sym);
    assert!(ob.is_special("vm-ghost"));
}

#[test]
fn replace_symbol_plist_id_overwrites_existing_entries() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let sym = intern("vm-plist");

    ob.put_property_id(sym, intern("stale"), Value::fixnum(1))
        .unwrap();
    ob.replace_symbol_plist_id(sym, [(intern("fresh"), Value::fixnum(2))]);

    assert_eq!(ob.get_property("vm-plist", "stale"), None);
    assert_eq!(ob.get_property("vm-plist", "fresh"), Some(Value::fixnum(2)));
}

#[test]
fn for_each_value_cell_mut_updates_plain_and_buffer_local_values() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();

    ob.set_symbol_value("plain", Value::fixnum(1));
    ob.set_symbol_value("buffer-local", Value::fixnum(2));
    ob.make_buffer_local("buffer-local", true);
    ob.set_symbol_function("callable", Value::fixnum(99));
    ob.put_property("plist-holder", "meta", Value::fixnum(77))
        .unwrap();

    ob.for_each_value_cell_mut(|value| {
        if let Some(n) = value.as_fixnum() {
            *value = Value::fixnum(n + 10);
        }
    });

    assert_eq!(ob.symbol_value("plain"), Some(&Value::fixnum(11)));
    assert_eq!(ob.symbol_value("buffer-local"), Some(&Value::fixnum(12)));
    assert_eq!(ob.symbol_function("callable"), Some(Value::fixnum(99)));
    assert_eq!(
        ob.get_property("plist-holder", "meta"),
        Some(Value::fixnum(77))
    );
}

// ===========================================================================
// Symbol-redirect refactor — Phase 1 sanity tests
// ===========================================================================
//
// These cover the new SymbolRedirect / SymbolFlags / SymbolVal machinery
// introduced in `drafts/symbol-redirect-plan.md` Step 1. They do NOT yet
// exercise LOCALIZED or FORWARDED dispatch — those land in later phases.

/// `LispSymbol::new` produces a fresh PLAINVAL symbol with NIL in its
/// value cell. Mirrors GNU `init_symbol` (`alloc.c:3659-3673`).
/// Phase H: fresh symbols start UNBOUND (not NIL), matching GNU's Qunbound sentinel.
#[test]
fn fresh_lisp_symbol_is_plainval_unbound() {
    crate::test_utils::init_test_tracing();
    let id = intern("phase1-fresh");
    let sym = LispSymbol::new(id);
    assert_eq!(sym.redirect(), SymbolRedirect::Plainval);
    assert_eq!(sym.flags.trapped_write(), SymbolTrappedWrite::Untrapped);
    assert_eq!(sym.flags.interned(), SymbolInterned::Uninterned);
    assert!(!sym.flags.declared_special());
    assert_eq!(sym.plain(), Value::UNBOUND);
}

/// Phase F: `Obarray::set_symbol_value` writes ONLY to `flags + val`.
/// The legacy `value` field is no longer updated (Phase H will delete it).
/// Verify that the authoritative redirect fields are correct.
#[test]
fn plainval_redirect_mirrors_legacy_value_field() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    ob.set_symbol_value("phase1-mirror", Value::fixnum(7));
    let id = intern("phase1-mirror");
    let sym = ob.get_by_id(id).expect("symbol just installed");
    assert_eq!(sym.redirect(), SymbolRedirect::Plainval);
    assert_eq!(sym.plain(), Value::fixnum(7));
    // Phase F: legacy `value` field is intentionally not written anymore;
    // the authoritative source is `flags.redirect() + val`.
    // (Phase H deletes the field entirely.)
}

/// Phase F: `make_alias` flips the redirect tag to `Varalias` and sets
/// `val.alias`. The legacy `value` field is no longer updated.
#[test]
fn varalias_redirect_mirrors_legacy_alias_field() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let from_id = intern("phase1-alias-from");
    let to_id = intern("phase1-alias-to");
    ob.ensure_symbol_id(from_id);
    ob.ensure_symbol_id(to_id);
    ob.make_alias(from_id, to_id);
    let sym = ob.get_by_id(from_id).expect("symbol just installed");
    assert_eq!(sym.redirect(), SymbolRedirect::Varalias);
    assert_eq!(sym.alias_target(), to_id);
    // Phase F: legacy `value` field is intentionally not written anymore;
    // the authoritative source is `flags.redirect() + val.alias`.
    // (Phase H deletes the field entirely.)
}

/// Pre-interned `t` and `nil` carry their canonical values in both the
/// legacy and the new shape. Mirrors GNU's setup of `Qnil` / `Qt` in
/// `alloc.c::init_alloc_once`.
#[test]
fn t_and_nil_have_consistent_redirect_state() {
    crate::test_utils::init_test_tracing();
    let ob = Obarray::new();
    let t = ob.get_by_id(intern("t")).expect("t pre-interned");
    let nil = ob.get_by_id(intern("nil")).expect("nil pre-interned");
    assert_eq!(t.redirect(), SymbolRedirect::Plainval);
    assert_eq!(t.plain(), Value::T);
    assert!(t.flags.trapped_write() == SymbolTrappedWrite::NoWrite);
    assert_eq!(nil.redirect(), SymbolRedirect::Plainval);
    assert_eq!(nil.plain(), Value::NIL);
    assert!(nil.flags.trapped_write() == SymbolTrappedWrite::NoWrite);
}

/// SymbolFlags packs into a single byte (matches GNU's bit layout).
#[test]
fn symbol_flags_pack_into_one_byte() {
    crate::test_utils::init_test_tracing();
    assert_eq!(std::mem::size_of::<SymbolFlags>(), 1);
}

// Phase 3 — VARALIAS via the new redirect tag.

/// `indirect_variable_id` walks a single-hop alias chain to its
/// terminus. Mirrors GNU `indirect_variable` (`src/data.c:1284-1301`).
#[test]
fn indirect_variable_id_follows_chain() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let a = intern("phase3-alias-a");
    let b = intern("phase3-alias-b");
    let c = intern("phase3-alias-c");
    ob.ensure_symbol_id(a);
    ob.ensure_symbol_id(b);
    ob.ensure_symbol_id(c);
    // a → b → c
    ob.make_alias(a, b);
    ob.make_alias(b, c);
    assert_eq!(ob.indirect_variable_id(a), Some(c));
    assert_eq!(ob.indirect_variable_id(b), Some(c));
    assert_eq!(ob.indirect_variable_id(c), Some(c));
}

/// `indirect_variable_id` returns `None` on a cycle, detected via
/// Floyd's tortoise/hare. The cycle protection mirrors the cycle
/// guard in GNU's `find_symbol_value` `goto start` loop
/// (`src/data.c:1593-1595`).
#[test]
fn indirect_variable_id_detects_cycle() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let a = intern("phase3-cycle-a");
    let b = intern("phase3-cycle-b");
    ob.ensure_symbol_id(a);
    ob.ensure_symbol_id(b);
    // a → b → a (cycle)
    ob.make_alias(a, b);
    ob.make_alias(b, a);
    assert_eq!(ob.indirect_variable_id(a), None);
    assert_eq!(ob.indirect_variable_id(b), None);
}

/// `make_variable_alias` rejects an attempt that would create a cycle.
/// Mirrors GNU `Fdefvaralias`'s "base chain looking for new_alias"
/// guard (`src/eval.c:631-726`).
#[test]
fn make_variable_alias_rejects_cycle() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let a = intern("phase3-malias-a");
    let b = intern("phase3-malias-b");
    let c = intern("phase3-malias-c");
    ob.ensure_symbol_id(a);
    ob.ensure_symbol_id(b);
    ob.ensure_symbol_id(c);
    // a → b → c, then try to make c → a (cycle).
    ob.make_variable_alias(a, b).expect("a → b ok");
    ob.make_variable_alias(b, c).expect("b → c ok");
    let err = ob.make_variable_alias(c, a).unwrap_err();
    assert_eq!(err, MakeAliasError::Cycle);
}

/// `make_variable_alias` rejects an attempt to alias a constant.
#[test]
fn make_variable_alias_rejects_constant() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let target = intern("phase3-malias-target");
    let nil_id = intern("nil"); // pre-interned constant
    ob.ensure_symbol_id(target);
    let err = ob.make_variable_alias(nil_id, target).unwrap_err();
    assert_eq!(err, MakeAliasError::Constant);
}

/// After `make_variable_alias`, both symbols are marked
/// `declared_special` (special).
#[test]
fn make_variable_alias_marks_both_special() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let a = intern("phase3-malias-special-a");
    let b = intern("phase3-malias-special-b");
    ob.ensure_symbol_id(a);
    ob.ensure_symbol_id(b);
    ob.make_variable_alias(a, b).expect("a → b ok");
    assert!(ob.is_special_id(a));
    assert!(ob.is_special_id(b));
    assert!(ob.is_alias_id(a));
    assert!(!ob.is_alias_id(b));
}

// Phase 4 — LOCALIZED read path with BLV cache.

/// `make_symbol_localized` allocates a BLV with `defcell == valcell`,
/// flips the redirect to LOCALIZED, and stores the BLV pointer in
/// `val.blv`. Mirrors GNU `make_blv` (`src/data.c:2112-2140`).
#[test]
fn make_symbol_localized_allocates_blv() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let id = intern("phase4-localized-x");
    ob.make_symbol_localized(id, Value::fixnum(42));
    let sym = ob.get_by_id(id).expect("symbol installed");
    assert_eq!(sym.redirect(), SymbolRedirect::Localized);
    let blv = ob.blv(id).expect("BLV pointer");
    // defcell == valcell initially.
    assert_eq!(blv.defcell, blv.valcell);
    // (sym . default)
    assert_eq!(blv.defcell.cons_cdr(), Value::fixnum(42));
    assert!(blv.where_buf.is_nil());
    assert!(!blv.found);
    assert!(!blv.local_if_set);
}

/// `Obarray::trace_roots` walks the BLV pool and pushes each BLV's
/// `defcell`, `valcell`, and `where_buf` fields as GC roots. Regression
/// test for a bug where a LOCALIZED symbol's cons-typed default value
/// was unrooted and got reclaimed by GC, causing reads to return
/// random heap garbage (see the `make_buffer_local` + LOCALIZED
/// interaction in Phase 10 cleanup).
#[test]
fn trace_roots_walks_blv_pool() {
    use crate::gc_trace::GcTrace;
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let id = intern("blv-trace-regression-x");
    // Cons-typed default. If the BLV isn't traced, this cons is
    // unreachable the moment the BLV swallows it.
    let default = Value::cons(Value::fixnum(1), Value::fixnum(2));
    ob.make_symbol_localized(id, default);
    let mut roots = Vec::new();
    ob.trace_roots(&mut roots);
    // The BLV's defcell is `(sym . default)`, a cons. valcell starts
    // equal to defcell. Both must be present in the root set.
    let blv = ob.blv(id).expect("BLV");
    assert!(
        roots.iter().any(|r| r.bits() == blv.defcell.bits()),
        "BLV defcell should be traced as a root"
    );
    assert!(
        roots.iter().any(|r| r.bits() == blv.valcell.bits()),
        "BLV valcell should be traced as a root"
    );
}

/// `find_symbol_value_in_buffer` for a LOCALIZED symbol with no
/// per-buffer binding returns the default. Mirrors GNU
/// `find_symbol_value` LOCALIZED arm.
#[test]
fn localized_returns_default_when_no_buffer_local() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let id = intern("phase4-default-x");
    ob.make_symbol_localized(id, Value::fixnum(7));
    let buf_value = Value::NIL; // pretend no current buffer
    let alist = Value::NIL; // empty alist
    let v = ob.find_symbol_value_in_buffer(id, None, buf_value, alist, None, 0, None);
    assert_eq!(v, Some(Value::fixnum(7)));
}

/// `find_symbol_value_in_buffer` swaps the BLV cache to the buffer's
/// `local_var_alist` entry when one exists. Mirrors GNU
/// `swap_in_symval_forwarding` (`src/data.c:1539-1571`).
#[test]
fn localized_swap_in_reads_buffer_local_value() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let id = intern("phase4-buflocal-x");
    ob.make_symbol_localized(id, Value::fixnum(0));
    // Build a fake buffer alist `((phase4-buflocal-x . 99))` and a
    // fake buffer value (we use a fixnum as a sentinel for "buffer A"
    // since the test doesn't need a real BufferManager).
    let cell = Value::cons(Value::from_sym_id(id), Value::fixnum(99));
    let alist = Value::cons(cell, Value::NIL);
    let buf_a = Value::fixnum(1);
    let v = ob.find_symbol_value_in_buffer(id, None, buf_a, alist, None, 0, None);
    assert_eq!(v, Some(Value::fixnum(99)));
    // The cache now records `where_buf == buf_a` and `found == true`.
    let blv = ob.blv(id).expect("BLV");
    assert_eq!(blv.where_buf, buf_a);
    assert!(blv.found);
}

/// Switching buffers reloads the BLV cache. A symbol with a binding
/// in buffer A returns A's value when current; switching to buffer B
/// (with no binding) returns the default.
#[test]
fn localized_blv_cache_invalidated_on_buffer_switch() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let id = intern("phase4-switch-x");
    ob.make_symbol_localized(id, Value::fixnum(0));

    // Buffer A has a binding (sym . 42).
    let buf_a = Value::fixnum(1);
    let alist_a = Value::cons(
        Value::cons(Value::from_sym_id(id), Value::fixnum(42)),
        Value::NIL,
    );
    let v_a = ob.find_symbol_value_in_buffer(id, None, buf_a, alist_a, None, 0, None);
    assert_eq!(v_a, Some(Value::fixnum(42)));

    // Buffer B has no binding for this symbol → default.
    let buf_b = Value::fixnum(2);
    let alist_b = Value::NIL;
    let v_b = ob.find_symbol_value_in_buffer(id, None, buf_b, alist_b, None, 0, None);
    assert_eq!(v_b, Some(Value::fixnum(0)));
    let blv = ob.blv(id).expect("BLV");
    assert_eq!(blv.where_buf, buf_b);
    assert!(!blv.found);
}

// Phase 5 — LOCALIZED write path.

/// `set_internal_localized` with `local_if_set = true` and
/// `bindflag = Set` auto-creates a per-buffer binding when none
/// exists. Mirrors GNU set_internal lines 1687-1763 (`src/data.c`).
#[test]
fn set_localized_creates_buffer_local_when_local_if_set() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let id = intern("phase5-autolocal-x");
    ob.make_symbol_localized(id, Value::fixnum(0));
    ob.set_blv_local_if_set(id, true);

    let buf = Value::fixnum(1);
    let mut alist = Value::NIL;
    alist = ob
        .set_internal_localized(
            id,
            Value::fixnum(42),
            buf,
            alist,
            SetInternalBind::Set,
            false, // let_shadows: false
        )
        .into_value();
    // The alist now has one entry: (sym . 42).
    assert!(alist.is_cons());
    let head = alist.cons_car();
    assert!(head.is_cons());
    assert_eq!(head.cons_car(), Value::from_sym_id(id));
    assert_eq!(head.cons_cdr(), Value::fixnum(42));
    // Read it back via the buffer-aware path.
    let v = ob.find_symbol_value_in_buffer(id, None, buf, alist, None, 0, None);
    assert_eq!(v, Some(Value::fixnum(42)));
}

/// When `local_if_set` is false, `set_internal_localized` writes the
/// default cell instead of auto-creating a per-buffer binding.
#[test]
fn set_localized_writes_default_when_no_local_if_set() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let id = intern("phase5-noautolocal-x");
    ob.make_symbol_localized(id, Value::fixnum(0));
    // local_if_set stays false (default).

    let buf = Value::fixnum(1);
    let alist = Value::NIL;
    let new_alist = ob.set_internal_localized(
        id,
        Value::fixnum(99),
        buf,
        alist,
        SetInternalBind::Set,
        false,
    );
    // Alist unchanged (no per-buffer binding created).
    assert_eq!(new_alist.into_value(), Value::NIL);
    // The default value was updated to 99.
    let blv = ob.blv(id).expect("BLV");
    assert_eq!(blv.defcell.cons_cdr(), Value::fixnum(99));
}

/// When `let_shadows == true`, the auto-create branch is suppressed.
/// Mirrors GNU's `let_shadows_buffer_binding_p` guard.
#[test]
fn set_localized_does_not_create_when_let_shadows() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let id = intern("phase5-letshadow-x");
    ob.make_symbol_localized(id, Value::fixnum(0));
    ob.set_blv_local_if_set(id, true);

    let buf = Value::fixnum(1);
    let alist = Value::NIL;
    let new_alist = ob.set_internal_localized(
        id,
        Value::fixnum(13),
        buf,
        alist,
        SetInternalBind::Set,
        true, // let_shadows: true
    );
    // No per-buffer binding created; defcell got the write.
    assert_eq!(new_alist.into_value(), Value::NIL);
    let blv = ob.blv(id).expect("BLV");
    assert_eq!(blv.defcell.cons_cdr(), Value::fixnum(13));
}

/// `set_internal_localized` with `bindflag = Bind` (let-binding's
/// initial assignment) never auto-creates a per-buffer binding,
/// even when `local_if_set` is true. The let unwind machinery in
/// Phase 7 handles restoration.
#[test]
fn set_localized_bind_never_auto_creates() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let id = intern("phase5-bind-x");
    ob.make_symbol_localized(id, Value::fixnum(0));
    ob.set_blv_local_if_set(id, true);

    let buf = Value::fixnum(1);
    let alist = Value::NIL;
    let new_alist = ob.set_internal_localized(
        id,
        Value::fixnum(7),
        buf,
        alist,
        SetInternalBind::Bind, // let-binding initial assignment
        false,
    );
    assert_eq!(new_alist.into_value(), Value::NIL);
    let blv = ob.blv(id).expect("BLV");
    assert_eq!(blv.defcell.cons_cdr(), Value::fixnum(7));
}

/// If the target buffer's `local_var_alist` entry was replaced after the BLV
/// cache was loaded, writes must update the current alist entry, not the stale
/// cached cons.  GNU keeps the cache coherent through `swap_in_symval_forwarding`;
/// Neomacs announces every structural alist change through the BLV alist
/// epoch (`set_local_var_alist_entry` / `remove_local_var_alist_entry` bump it),
/// after which reads AND writes trust an epoch-valid cache and reselect from
/// the authoritative alist otherwise.  The hand-built replacement below stands
/// in for those APIs, so it announces itself the same way.
#[test]
fn set_localized_reselects_replaced_alist_cell() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let id = intern("phase5-stale-blv-x");
    ob.make_symbol_localized(id, Value::fixnum(0));

    let buf = Value::fixnum(1);
    let old_cell = Value::cons(Value::from_sym_id(id), Value::fixnum(10));
    let old_alist = Value::cons(old_cell, Value::NIL);
    assert_eq!(
        ob.find_symbol_value_in_buffer(id, None, buf, old_alist, None, 0, None),
        Some(Value::fixnum(10))
    );

    let new_cell = Value::cons(Value::from_sym_id(id), Value::fixnum(20));
    let new_alist = Value::cons(new_cell, Value::NIL);
    note_blv_alist_structural_mutation();
    let returned_alist = ob.set_internal_localized(
        id,
        Value::fixnum(99),
        buf,
        new_alist,
        SetInternalBind::Bind,
        false,
    );

    assert_eq!(returned_alist.into_value(), new_alist);
    assert_eq!(old_cell.cons_cdr(), Value::fixnum(10));
    assert_eq!(new_cell.cons_cdr(), Value::fixnum(99));
}

// Phase 8a — FORWARDED via BUFFER_OBJFWD slot.

/// `install_buffer_objfwd` flips the redirect to `Forwarded` and
/// stores the descriptor pointer in `val.fwd`. Mirrors GNU
/// `defvar_per_buffer` (`buffer.c:4990-5012`).
#[test]
fn install_buffer_objfwd_flips_redirect() {
    crate::test_utils::init_test_tracing();
    use crate::buffer::buffer::BufferSlotPredicate;
    use crate::emacs_core::forward::alloc_buffer_objfwd;
    let mut ob = Obarray::new();
    let id = intern("phase8-fwd-x");
    let fwd = alloc_buffer_objfwd(0, -1, BufferSlotPredicate::String, Value::fixnum(42));
    ob.install_buffer_objfwd(id, fwd);
    let sym = ob.get_by_id(id).expect("symbol installed");
    assert_eq!(sym.redirect(), SymbolRedirect::Forwarded);
    assert!(sym.flags.declared_special());
    assert!(sym.flags.declared_special());
}

/// `find_symbol_value_in_buffer` for a FORWARDED `BUFFER_OBJFWD`
/// reads from `current_buffer.slots[offset]`. Mirrors GNU
/// `do_symval_forwarding` (`data.c:1330-1352`) for the
/// `Lisp_Buffer_Objfwd` arm.
#[test]
fn find_symbol_value_forwarded_reads_buffer_slot() {
    crate::test_utils::init_test_tracing();
    use crate::buffer::buffer::BufferSlotPredicate;
    use crate::emacs_core::forward::alloc_buffer_objfwd;
    let mut ob = Obarray::new();
    let id = intern("phase8-fwd-slot-x");
    let fwd = alloc_buffer_objfwd(3, -1, BufferSlotPredicate::String, Value::fixnum(0));
    ob.install_buffer_objfwd(id, fwd);

    // Synthetic buffer slot table.
    let mut slots = vec![Value::NIL; 10];
    slots[3] = Value::fixnum(99);
    let v = ob.find_symbol_value_in_buffer(id, None, Value::NIL, Value::NIL, Some(&slots), 0, None);
    assert_eq!(v, Some(Value::fixnum(99)));
}

/// When no current-buffer slot table is provided (e.g. during
/// startup before any buffer exists), the FORWARDED arm returns
/// the forwarder's default.
#[test]
fn find_symbol_value_forwarded_returns_default_without_buffer() {
    crate::test_utils::init_test_tracing();
    use crate::buffer::buffer::BufferSlotPredicate;
    use crate::emacs_core::forward::alloc_buffer_objfwd;
    let mut ob = Obarray::new();
    let id = intern("phase8-fwd-default-x");
    let fwd = alloc_buffer_objfwd(5, -1, BufferSlotPredicate::String, Value::fixnum(7));
    ob.install_buffer_objfwd(id, fwd);
    let v = ob.find_symbol_value_in_buffer(id, None, Value::NIL, Value::NIL, None, 0, None);
    assert_eq!(v, Some(Value::fixnum(7)));
}

/// `Obarray::clone` deep-copies the BLV pool and remaps symbol
/// pointers, so a cloned obarray reads independently from the
/// original.
#[test]
fn clone_obarray_deep_copies_blvs() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    let id = intern("phase4-clone-x");
    ob.make_symbol_localized(id, Value::fixnum(11));
    let cloned = ob.clone();
    // Both obarrays read the same default initially.
    let v1 = ob.find_symbol_value(id);
    let v2 = cloned.find_symbol_value(id);
    assert_eq!(v1, Some(Value::fixnum(11)));
    assert_eq!(v2, Some(Value::fixnum(11)));
    // The cloned obarray's BLV pointer is a fresh allocation.
    let blv1 = ob.blv(id).expect("blv1");
    let blv2 = cloned.blv(id).expect("blv2");
    assert!(
        std::ptr::addr_of!(*blv1) != std::ptr::addr_of!(*blv2),
        "cloned BLV must be a distinct allocation"
    );
}

#[test]
fn uninterned_keyword_and_nil_names_are_not_canonical_constants() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let nil_id = crate::emacs_core::intern::intern_uninterned("nil");
    let kw_id = crate::emacs_core::intern::intern_uninterned(":vm-k");

    assert!(!eval.obarray().is_constant_id(nil_id));
    assert!(!eval.obarray().is_constant_id(kw_id));

    eval.obarray_mut()
        .set_symbol_function_id(nil_id, Value::subr(intern("+")));
    assert!(eval.obarray().symbol_function_id(nil_id).is_some());
    assert!(eval.obarray().intern_soft("nil").is_some());
    assert!(eval.obarray().intern_soft(":vm-k").is_none());
}

// ===========================================================================
// Stage 1b seqlock symbol-read protocol: torn-arm-read defense
// ===========================================================================
//
// These two tests prove that `read_symbol_children_consistent` (the GC-thread
// read side of the per-chunk seqlock) never returns a value read from the WRONG
// union arm under a concurrent writer that flips a symbol between two redirect
// states. The positive test asserts zero torn reads under the real protocol; the
// negative control holds the writer inside the torn window and proves that the
// same read without the seqlock retry accepts an inconsistent arm/value pair.

/// Two distinct "heap-looking" `Value`s minted from raw tagged bits.
///
/// `TAG_CONS == 0b011`. A word `(fake_ptr | TAG_CONS)` with `fake_ptr`
/// 8-aligned has `tag() == TAG_CONS`, so `Value::is_heap_object()` returns
/// `true` (it matches `TAG_CONS | TAG_STRING | TAG_FLOAT | TAG_VECLIKE`).
/// The pointer is NEVER dereferenced by the test — only its bits are compared
/// and its heap-object-ness exercised — so a fake address is sound here. The
/// two values differ in the high bits, so a torn read that swaps one for the
/// other is detectable by value comparison.
fn heap_a() -> Value {
    // 0x1_0000 | 0b011 = 0x1_0003. 8-aligned base, cons tag.
    Value::from_bits(0x1_0000 | crate::tagged::value::TAG_CONS)
}
fn heap_b() -> Value {
    // 0x2_0000 | 0b011 = 0x2_0003. Distinct 8-aligned base, cons tag.
    Value::from_bits(0x2_0000 | crate::tagged::value::TAG_CONS)
}

/// Raw-pointer bundle to share the symbol + seqlock across threads. The only
/// cross-thread accesses are the atomic word/flag stores on the writer side and
/// the seqlock-protocol atomic loads on the reader side, mirroring the
/// production `ConsCell` / per-chunk-seqlock pattern (single mutator, single GC
/// reader). Hence `Send` is sound.
struct Shared(*mut LispSymbol, *const std::sync::atomic::AtomicU32);
unsafe impl Send for Shared {}

/// Number of writer arm-flips in the protected concurrent stress test.
const SEQLOCK_WRITER_ITERS: u64 = 4_000_000;

/// Drive the shared writer loop: flip the symbol between
///   State P: redirect=Plainval, val word = HEAP_A
///   State V: redirect=Varalias, val word = HEAP_B (deliberately staged as a
///            heap-looking word so a TORN (Plainval, HEAP_B) read is detectable;
///            a real SymId alias word would be non-heap and silently invisible)
/// EXACTLY mirroring `SeqlockWriteGuard`: bump seq to ODD (Release), do the two
/// writes (redirect first, then the val word — so a non-retrying reader that
/// samples redirect=Plainval then the still-stale/just-updated word can tear),
/// bump seq back to EVEN (Release).
fn run_seqlock_writer(shared: Shared, done: &std::sync::atomic::AtomicBool) {
    use std::sync::atomic::Ordering;
    let sym: &mut LispSymbol = unsafe { &mut *shared.0 };
    let seq: &std::sync::atomic::AtomicU32 = unsafe { &*shared.1 };
    let a = heap_a();
    let b = heap_b();
    for _ in 0..SEQLOCK_WRITER_ITERS {
        // --- State V: Varalias arm, word staged as HEAP_B ---
        seq.fetch_add(1, Ordering::Release); // -> odd: arm change in flight
        sym.flags.set_redirect(SymbolRedirect::Varalias);
        crate::tagged::header::store_value_atomic(unsafe { &mut sym.val.plain }, b);
        seq.fetch_add(1, Ordering::Release); // -> even

        // --- State P: Plainval arm, word = HEAP_A ---
        seq.fetch_add(1, Ordering::Release); // -> odd
        sym.flags.set_redirect(SymbolRedirect::Plainval);
        crate::tagged::header::store_value_atomic(unsafe { &mut sym.val.plain }, a);
        seq.fetch_add(1, Ordering::Release); // -> even
    }
    done.store(true, Ordering::Release);
}

fn run_paused_seqlock_writer(
    shared: Shared,
    arm_published: &std::sync::Barrier,
    reader_sampled: &std::sync::Barrier,
) {
    use std::sync::atomic::Ordering;
    let sym: &mut LispSymbol = unsafe { &mut *shared.0 };
    let seq: &std::sync::atomic::AtomicU32 = unsafe { &*shared.1 };

    seq.fetch_add(1, Ordering::Release); // odd: arm change in flight
    sym.flags.set_redirect(SymbolRedirect::Plainval);
    arm_published.wait();
    reader_sampled.wait();
    crate::tagged::header::store_value_atomic(unsafe { &mut sym.val.plain }, heap_a());
    seq.fetch_add(1, Ordering::Release); // even: stable State P
}

#[test]
fn seqlock_symbol_read_never_tears_arm() {
    crate::test_utils::init_test_tracing();
    use std::sync::atomic::{AtomicBool, Ordering};

    // Start in State P so the very first reads (before the writer runs) are
    // already a consistent Plainval/HEAP_A pair.
    let mut sym = LispSymbol::new(intern("vm-seqlock-test-sym"));
    sym.flags.set_redirect(SymbolRedirect::Plainval);
    sym.val = SymbolVal { plain: heap_a() };
    // Only the val arm may produce a child: function/plist are NIL (non-heap).
    sym.function = Value::NIL;
    sym.plist = Value::NIL;

    let seq = std::sync::atomic::AtomicU32::new(0); // even = stable
    let done = AtomicBool::new(false);

    let a = heap_a();
    let b = heap_b();

    std::thread::scope(|scope| {
        let shared = Shared(&mut sym as *mut LispSymbol, &seq as *const _);
        let writer = scope.spawn(|| run_seqlock_writer(shared, &done));

        // Reader (this thread): hammer the real protocol until the writer is done.
        // Every pushed child MUST be HEAP_A — the only value legally reachable
        // through the Plainval arm. If HEAP_B (the Varalias-arm word) is ever
        // pushed, the seqlock failed to prevent a torn-arm read.
        let mut reads: u64 = 0;
        while !done.load(Ordering::Acquire) {
            for _ in 0..1024 {
                read_symbol_children_consistent(&seq, &sym, |child| {
                    assert_eq!(
                        child.bits(),
                        a.bits(),
                        "TORN ARM READ: protocol pushed {:#x}, expected HEAP_A {:#x} \
                         (HEAP_B is {:#x} — pushing it means redirect=Plainval was \
                         paired with the Varalias-arm word)",
                        child.bits(),
                        a.bits(),
                        b.bits(),
                    );
                });
                reads += 1;
            }
        }
        writer.join().unwrap();
        // Sanity: the reader actually ran many times against the live race.
        assert!(reads > 1000, "reader barely ran ({reads} iterations)");
    });
}

#[test]
fn seqlock_negative_control_tears_without_protocol() {
    crate::test_utils::init_test_tracing();
    use std::sync::Barrier;

    // Begin in State V. The writer will publish the Plainval redirect, then
    // pause before replacing HEAP_B with HEAP_A so the reader deterministically
    // samples the exact torn window that the seqlock protects.
    let mut sym = LispSymbol::new(intern("vm-seqlock-test-sym"));
    sym.flags.set_redirect(SymbolRedirect::Varalias);
    sym.val = SymbolVal { plain: heap_b() };
    sym.function = Value::NIL;
    sym.plist = Value::NIL;

    let seq = std::sync::atomic::AtomicU32::new(0);
    let arm_published = Barrier::new(2);
    let reader_sampled = Barrier::new(2);
    let b = heap_b();

    let (redirect, value) = std::thread::scope(|scope| {
        let shared = Shared(&mut sym as *mut LispSymbol, &seq as *const _);
        let writer =
            scope.spawn(|| run_paused_seqlock_writer(shared, &arm_published, &reader_sampled));

        // BROKEN reader: read the redirect tag, then read the val word, with NO
        // seqlock retry (no odd-check, no re-read of seq). This is exactly the
        // bug the real protocol defends against. The barriers hold the writer
        // mid-flip V->P: redirect is Plainval while the word is still HEAP_B.
        arm_published.wait();
        let redirect = sym.flags.load_redirect();
        let value = crate::tagged::header::load_value_atomic(unsafe { &sym.val.plain });
        reader_sampled.wait();
        writer.join().unwrap();
        (redirect, value)
    });

    assert_eq!(redirect, SymbolRedirect::Plainval);
    assert!(value.is_heap_object());
    assert_eq!(value.bits(), b.bits(), "broken reader must accept HEAP_B");
}

/// The memoized `debug-on-next-call` cell (ledger 172's per-`Op::Call` read,
/// cached because GNU's is one load of `globals.f_debug_on_next_call`) must
/// not leak across `Obarray::clone`: clone duplicates every stateful
/// forwarder, so a cloned obarray owns a DIFFERENT `LispBoolFwd` cell and the
/// cache has to re-resolve rather than alias the source's.
#[test]
fn cloned_obarray_resolves_its_own_debug_on_next_call_cell() {
    use crate::emacs_core::defvar_bool::ByteBooleanVars;

    let mut ob = Obarray::new();
    let id = intern("debug-on-next-call");
    ob.define_bool_variable("debug-on-next-call", false, ByteBooleanVars::Listed);

    let source = ob
        .debug_on_next_call_bool_fwd(id)
        .expect("DEFVAR_BOOL just installed the descriptor");
    source.set(true);

    // Cache is primed in the source; the clone must start unresolved and find
    // its own duplicated cell (same value, different address).
    let cloned_ob = ob.clone();
    let cloned = cloned_ob
        .debug_on_next_call_bool_fwd(id)
        .expect("clone duplicated the stateful forwarder");
    assert!(
        !std::ptr::eq(source, cloned),
        "clone must not share the source's cell"
    );
    assert!(cloned.get(), "clone_stateful copies the armed value");

    // Desync-proofing both ways: each obarray reads its own cell.
    source.set(false);
    assert!(
        cloned.get(),
        "disarming the source must not disarm the clone"
    );
    cloned.set(false);
    source.set(true);
    assert!(
        !cloned_ob.debug_on_next_call_bool_fwd(id).unwrap().get(),
        "re-arming the source must not re-arm the clone"
    );

    // The memoized read and the uncached slot walk agree on the same cell.
    assert!(std::ptr::eq(
        ob.debug_on_next_call_bool_fwd(id).unwrap(),
        ob.bool_forwarder(id).unwrap()
    ));
}
