//! Collection-coverage stamps across the runtime's deferred minor path.

use super::gc_collection_epoch_tests::UncoveredRegexCaches;
use super::gc_collection_epoch_tests::WarmRegexCaches;
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[derive(Clone, Copy)]
struct MinorCounts {
    completed: usize,
    context: u64,
}

fn generational_context() -> Context {
    // Nextest isolates each test. Select before construction, rather than
    // relying on a caller's environment or weakening phase assertions.
    // SAFETY: nextest isolates this test process; no runtime workers exist
    // before constructor configuration, so no thread reads the environment.
    unsafe {
        std::env::set_var("NEOVM_GC_GENERATIONAL", "1");
        std::env::set_var("NEOVM_GC_MAJOR_MAX_MINORS", usize::MAX.to_string());
        std::env::set_var("NEOVM_GC_MAJOR_GROWTH_PERCENT", usize::MAX.to_string());
        std::env::set_var("NEOVM_GC_STRESS_MAJOR_EVERY", usize::MAX.to_string());
    }
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    ctx.gc_stress = false;
    ctx.with_gc_inhibited(|ctx| {
        ctx.eval_str(
            "(setq gc-cons-threshold most-positive-fixnum
                   gc-cons-percentage 1000 memory-full nil)",
        )
        .unwrap();
    });
    // Bootstrap is a major; every collection below is selected as a minor.
    ctx.gc_collect_exact();
    ctx.setup_thread_locals();
    ctx.set_gc_threshold(usize::MAX);
    assert!(ctx.tagged_heap.generational_enabled());
    assert!(ctx.tagged_heap.should_run_minor(false, false));
    ctx
}

fn start_public_minor(ctx: &mut Context) -> MinorCounts {
    assert!(ctx.tagged_heap.generational_enabled());
    assert!(!ctx.gc_stress);
    assert!(ctx.tagged_heap.should_run_minor(false, false));
    assert!(!ctx.tagged_heap.mark_in_progress());
    assert!(!ctx.tagged_heap.sweep_in_progress());
    let before = MinorCounts {
        completed: ctx.tagged_heap.gc_collections(),
        context: ctx.gc_count,
    };
    ctx.tagged_heap.alloc_cons(Value::fixnum(313), Value::NIL);
    ctx.set_gc_threshold(1);
    ctx.gc_safe_point();
    // The minor's stopped marking and promotion finish in this call, but
    // its sweep is deferred. Neither counter publishes completion yet.
    assert!(ctx.tagged_heap.sweep_in_progress());
    assert!(!ctx.tagged_heap.mark_in_progress());
    assert!(!ctx.tagged_heap.concurrent_mark_running());
    assert_eq!(ctx.tagged_heap.gc_collections(), before.completed);
    assert_eq!(ctx.gc_count, before.context);
    ctx.set_gc_threshold(usize::MAX);
    before
}

fn finish_public_minor(ctx: &mut Context, before: MinorCounts) {
    assert!(ctx.tagged_heap.sweep_in_progress());
    for _ in 0..10_000 {
        ctx.gc_safe_point();
        if !ctx.tagged_heap.sweep_in_progress() {
            assert!(!ctx.tagged_heap.mark_in_progress());
            assert!(!ctx.tagged_heap.concurrent_mark_running());
            assert_eq!(ctx.tagged_heap.gc_collections(), before.completed + 1);
            assert_eq!(ctx.gc_count, before.context + 1);
            return;
        }
    }
    panic!("public safe points did not complete the deferred minor sweep");
}

fn detach_literal_case_table(ctx: &mut Context, warm: &WarmRegexCaches) {
    let cached_canon = warm.cache_tables()[2];
    assert!(ctx.tagged_heap.owns_heap_value_for_test(cached_canon));
    let before =
        buffer_search_translation_table(ctx.buffers.current_buffer().unwrap(), true).unwrap();
    assert_eq!(before.bits(), cached_canon.bits());
    ctx.with_gc_inhibited(|ctx| {
        ctx.eval_str("(set-case-table (standard-case-table))")
            .unwrap();
    });
    // A standard table can still yield Some(canon) when the Context's
    // explicit standard-table TLS cache is unset. Detachment is an identity
    // condition: the buffer must stop referring to our cached custom canon.
    let after = buffer_search_translation_table(ctx.buffers.current_buffer().unwrap(), true);
    assert!(after.is_none_or(|table| table.bits() != cached_canon.bits()));
}

fn assert_table_generation(ctx: &Context, warm: &WarmRegexCaches, old: bool) {
    for table in warm.cache_tables() {
        // The allocation registry is safe to inspect even if a missing root
        // caused reclamation. Only inspect generation after proving ownership.
        assert!(ctx.tagged_heap.owns_heap_value_for_test(table));
        assert_eq!(ctx.tagged_heap.value_is_old_for_test(table), old);
    }
}

fn assert_all_regex_caches_empty() {
    assert!(SEARCH_PATTERN_CACHE.with(|cache| cache.borrow().is_empty()));
    assert!(LISP_REGEX_PATTERN_CACHE.with(|cache| cache.borrow().is_empty()));
    assert!(LITERAL_TRT_CACHE.with(|cache| cache.borrow().is_none()));
}

#[test]
fn gc_collection_epoch_regex_same_thread_minor_keeps_warm_entries() {
    let mut ctx = generational_context();
    let warm = WarmRegexCaches::new(&mut ctx);
    let completed = ctx.tagged_heap.gc_collections();
    for _ in 0..3 {
        let before = start_public_minor(&mut ctx);
        finish_public_minor(&mut ctx, before);
        ctx.setup_thread_locals();
        warm.assert_present_and_hit(&ctx);
        warm.assert_cached_search_results(&ctx);
    }
    assert_eq!(ctx.tagged_heap.gc_collections(), completed + 3);
}

#[test]
fn gc_collection_epoch_regex_cache_only_young_tables_survive_minor_and_major() {
    let mut ctx = generational_context();
    let warm = WarmRegexCaches::new(&mut ctx);
    // The fixture already detached its custom syntax table and independent
    // compiled translation. Detach the literal case table too. None of these
    // three tables is in Context, symbols, buffers, or scratch-root slots.
    detach_literal_case_table(&mut ctx, &warm);
    assert_table_generation(&ctx, &warm, false);
    let before = start_public_minor(&mut ctx);
    // Minor survivors are promoted before the mutator can resume sweeping.
    assert_table_generation(&ctx, &warm, true);
    finish_public_minor(&mut ctx, before);
    ctx.setup_thread_locals();
    warm.assert_present_and_compiled_hits(&ctx);
    warm.assert_cached_search_results(&ctx);
    assert_table_generation(&ctx, &warm, true);

    ctx.gc_collect_exact();
    ctx.setup_thread_locals();
    warm.assert_present_and_compiled_hits(&ctx);
    warm.assert_cached_search_results(&ctx);
    assert_table_generation(&ctx, &warm, true);

    // Negative control: Rust-held Values/Rcs and Context aliases must not be
    // additional GC roots. Removing only the regexp caches frees every table.
    let tables = warm.cache_tables();
    clear_regex_caches();
    ctx.gc_collect_exact();
    for table in tables {
        assert!(
            !ctx.tagged_heap.owns_heap_value_for_test(table),
            "detached table had a root outside the regexp caches"
        );
    }
}

#[test]
fn gc_collection_epoch_regex_uncovered_minor_clears_saved_cache_on_activation() {
    let mut ctx = generational_context();
    let warm = WarmRegexCaches::new(&mut ctx);
    detach_literal_case_table(&mut ctx, &warm);
    assert_table_generation(&ctx, &warm, false);
    let tables = warm.cache_tables();
    let completed = ctx.tagged_heap.gc_collections();
    let stale = UncoveredRegexCaches::take();
    let before = start_public_minor(&mut ctx);
    finish_public_minor(&mut ctx, before);
    for table in tables {
        assert!(
            !ctx.tagged_heap.owns_heap_value_for_test(table),
            "uncovered minor unexpectedly rooted the saved cache"
        );
    }
    stale.restore();
    assert_eq!(ctx.tagged_heap.gc_collections(), completed + 1);
    // Saved Rc identities remain local without reading their reclaimed Lisp
    // tables. Activation must clear every cache before publishing any root.
    ctx.setup_thread_locals();
    assert_all_regex_caches_empty();
    ctx.gc_collect_exact();
}

#[test]
fn gc_collection_epoch_regex_owning_minor_sweep_activation_keeps_covered_entries() {
    let mut ctx = generational_context();
    let warm = WarmRegexCaches::new(&mut ctx);
    detach_literal_case_table(&mut ctx, &warm);
    let before = start_public_minor(&mut ctx);
    ctx.setup_thread_locals();
    assert_eq!(ctx.tagged_heap.gc_collections(), before.completed);
    assert!(ctx.tagged_heap.sweep_in_progress());
    warm.assert_present_and_compiled_hits(&ctx);
    warm.assert_cached_search_results(&ctx);
    assert_table_generation(&ctx, &warm, true);
    finish_public_minor(&mut ctx, before);
}

#[test]
fn gc_collection_epoch_regex_foreign_minor_sweep_activation_clears_uncovered_entries() {
    let mut ctx = generational_context();
    let warm = WarmRegexCaches::new(&mut ctx);
    detach_literal_case_table(&mut ctx, &warm);
    assert_table_generation(&ctx, &warm, false);
    let completed = ctx.tagged_heap.gc_collections();
    let stale = UncoveredRegexCaches::take();
    let before = start_public_minor(&mut ctx);
    stale.restore();
    assert_eq!(ctx.tagged_heap.gc_collections(), completed);
    assert!(ctx.tagged_heap.sweep_in_progress());
    // Restore an uncovered cache after the owner finished marking but before
    // the public safe point finishes its deferred sweep.
    ctx.setup_thread_locals();
    assert_all_regex_caches_empty();
    finish_public_minor(&mut ctx, before);
    for table in warm.cache_tables() {
        assert!(!ctx.tagged_heap.owns_heap_value_for_test(table));
    }
    ctx.gc_collect_exact();
}
