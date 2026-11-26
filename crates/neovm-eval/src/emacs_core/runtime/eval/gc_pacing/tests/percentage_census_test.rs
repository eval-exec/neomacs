use super::*;
use neovm_value::hashtab::HashTestGcInhibitAccounting;
const SYMBOL_BYTES: usize = neovm_value::symbol::LISP_SYMBOL_SIZE;

fn cached_symbol_bytes(ctx: &Context) -> usize {
    ctx.gc_cons_percentage_live_bytes() - ctx.tagged_heap.live_bytes()
}

#[test]
fn gc_percentage_symbol_census_counts_neo_seeds_and_runtime_births_once_per_gc() {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    assert_eq!(
        cached_symbol_bytes(&ctx),
        (neovm_value::intern::FIRST_WELL_KNOWN_SYM_ID as usize
            + neovm_value::intern::WellKnown::COUNT)
            * std::mem::size_of::<neovm_value::symbol::LispSymbol>(),
        "activation interns, including native subrs, do not join the fixed seed array"
    );
    ctx.gc_collect_exact();
    let before = cached_symbol_bytes(&ctx);
    let name_prefix = neovm_value::intern::symbol_span_total();
    let mut held = Vec::new();
    for index in 0..7 {
        let name = format!("gc-census-runtime-{name_prefix}-{index}");
        held.push(Value::from_sym_id(ctx.obarray.intern_member(&name)));
    }
    for _ in 0..5 {
        held.push(Value::from_sym_id(
            neovm_value::intern::make_uninterned_symbol("gc-census-uninterned"),
        ));
    }
    assert_eq!(
        cached_symbol_bytes(&ctx),
        before,
        "the percentage census changes at completed collection, not birth"
    );
    let saved = save_scratch_gc_roots();
    push_scratch_gc_roots(&held);
    ctx.gc_collect_exact();
    assert_eq!(
        cached_symbol_bytes(&ctx) - before,
        12 * std::mem::size_of::<neovm_value::symbol::LispSymbol>()
    );
    let after = cached_symbol_bytes(&ctx);
    ctx.gc_collect_exact();
    assert_eq!(cached_symbol_bytes(&ctx), after, "no repeated addition");
    restore_scratch_gc_roots(saved);
}

#[test]
fn gc_percentage_symbol_census_is_shared_by_normal_and_inhibited_budgets() {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    ctx.gc_collect_exact();
    ctx.gc_runtime_settings_cache.gc_cons_threshold_bytes = GC_THRESHOLD_FLOOR_BYTES;
    ctx.gc_runtime_settings_cache.gc_cons_percentage = Some(4.0);
    ctx.gc_runtime_settings_cache.memory_full = GcMemoryPressure::Normal;
    ctx.gc_runtime_settings_cache.syms = None;
    let charged = ctx.tagged_heap.bytes_since_gc();
    let accounting = HashTestGcInhibitAccounting::new(
        ctx.tagged_heap.bytes_since_gc_exact(),
        charged,
        ctx.tagged_heap.gc_threshold(),
        None,
        GC_THRESHOLD_FLOOR_BYTES,
        Some(4.0),
        false,
        false,
        false,
    );
    let expected = super::super::consing_threshold(
        GC_THRESHOLD_FLOOR_BYTES,
        Some(4.0),
        ctx.tagged_heap.live_bytes() + ctx.gc_symbols_used() * SYMBOL_BYTES,
        charged,
    );
    let expected = gc_threshold_cap_from_env().map_or(expected, |cap| expected.min(cap));
    assert_eq!(ctx.effective_gc_threshold_bytes(), expected);
    assert_eq!(ctx.user_test_gc_entry_threshold(&accounting), expected);
}
