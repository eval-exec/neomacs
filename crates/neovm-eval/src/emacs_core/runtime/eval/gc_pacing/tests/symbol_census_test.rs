use super::*;

fn collect_symbol_row(ctx: &mut Context) -> usize {
    ctx.tagged_heap.request_gc_object_counts();
    ctx.gc_collect_exact();
    ctx.tagged_heap.gc_object_counts().symbols_used
}

#[test]
fn gc_rows_symbols_count_neo_seed_and_runtime_births_after_sweep() {
    // Admission lazily interns this native cache identity on the first GC.
    // Initialize it before activation so the controlled births below are
    // exactly the seven interned and five uninterned runtime symbols.
    neovm_text::emacs_core::casetab::standard_case_table_object_symbol_id();
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let seed = neovm_value::intern::FIRST_WELL_KNOWN_SYM_ID as usize
        + neovm_value::intern::WellKnown::COUNT;
    assert_eq!(ctx.gc_symbols_used(), seed);
    assert_eq!(collect_symbol_row(&mut ctx), seed);
    let before = collect_symbol_row(&mut ctx);
    let name_prefix = neovm_value::intern::symbol_span_total();
    let mut held = Vec::new();
    for index in 0..7 {
        let name = format!("gc-row-runtime-{name_prefix}-{index}");
        held.push(Value::from_sym_id(ctx.obarray.intern_member(&name)));
    }
    for _ in 0..5 {
        held.push(Value::from_sym_id(
            neovm_value::intern::make_uninterned_symbol("gc-row-uninterned"),
        ));
    }
    assert_eq!(
        ctx.gc_symbols_used(),
        before,
        "last-sweep snapshot is stable"
    );
    let saved = save_scratch_gc_roots();
    push_scratch_gc_roots(&held);
    assert_eq!(collect_symbol_row(&mut ctx) - before, 12);
    let after = ctx.gc_symbols_used();
    assert_eq!(collect_symbol_row(&mut ctx), after, "no repeated addition");
    restore_scratch_gc_roots(saved);
}

#[test]
fn gc_rows_symbols_rebaseline_each_world_after_activation() {
    {
        let mut first = Context::new(crate::test_utils::unit_runtime());
        for _ in 0..9 {
            neovm_value::intern::make_uninterned_symbol("gc-row-prior-world");
        }
        assert!(collect_symbol_row(&mut first) >= SEED_SYMBOL_COUNT + 9);
    }
    let mut second = Context::new(crate::test_utils::unit_runtime());
    assert_eq!(collect_symbol_row(&mut second), SEED_SYMBOL_COUNT);
}

#[test]
fn gc_rows_symbols_exclude_image_births_after_pdump_reconstruction() {
    let mut source = Context::new(crate::test_utils::unit_runtime());
    for _ in 0..11 {
        neovm_value::intern::make_uninterned_symbol("gc-row-image-symbol");
    }
    collect_symbol_row(&mut source);
    let mut restored = crate::emacs_core::pdump::clone_active_evaluator(&mut source)
        .expect("symbol-row snapshot reconstruction");
    assert_eq!(collect_symbol_row(&mut restored), SEED_SYMBOL_COUNT);
    let before = collect_symbol_row(&mut restored);
    for _ in 0..3 {
        neovm_value::intern::make_uninterned_symbol("gc-row-after-image");
    }
    assert_eq!(collect_symbol_row(&mut restored) - before, 3);
}
