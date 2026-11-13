//! Runtime side-table GC roots, enumerated once for both the collector
//! and a parking Green owner (kept out of `eval/mod.rs`, a facade).

use super::Value;

/// Collect GC roots from runtime-global side tables that hold Values.
///
/// These side tables are invisible to the normal GC root scan (which only
/// walks the Evaluator struct and its sub-managers).  This function calls each
/// module's `collect_*_gc_roots` helper to ensure those Values are marked as
/// live during garbage collection.
#[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
pub(crate) fn collect_thread_local_gc_roots(
    roots: &mut Vec<(Value, &'static str)>,
    heap_id: usize,
    collection_epoch: usize,
    scan: neovm_value::tagged::gc::CacheRootScan,
    stats: &mut Vec<neovm_value::tagged::gc::RootGroup>,
) {
    fn collect_group(
        roots: &mut Vec<(Value, &'static str)>,
        origin: &'static str,
        stats: &mut Vec<neovm_value::tagged::gc::RootGroup>,
        collect: impl FnOnce(&mut Vec<Value>),
    ) {
        // GC handshake instrumentation: per-side-table build cost + volume
        // (the JIT reloc walk in particular scales with the COMPILED cache).
        let t0 = std::time::Instant::now();
        let mut group = Vec::new();
        collect(&mut group);
        stats.push((origin, t0.elapsed().as_micros() as u64, group.len()));
        roots.extend(group.into_iter().map(|root| (root, origin)));
    }

    for_each_thread_local_root_source(heap_id, collection_epoch, scan, |origin, collect| {
        collect_group(roots, origin, stats, collect);
    });
}

/// One enumeration for collection and parked owners. Each sink chooses its
/// destination and instrumentation; the source callback only appends Values.
/// Parking supplies its retained Vec and never needs per-group scratch.
#[inline(always)]
#[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
pub(crate) fn for_each_thread_local_root_source(
    heap_id: usize,
    collection_epoch: usize,
    scan: neovm_value::tagged::gc::CacheRootScan,
    mut sink: impl FnMut(&'static str, &mut dyn FnMut(&mut Vec<Value>)),
) {
    // R1a: heap-object constants loaded by JIT-compiled leaves through their reloc
    // vectors — generated code holds only indices, so these must be rooted here.
    #[cfg(feature = "jit")]
    sink("jit-reloc-thread-local", &mut |group| {
        super::super::jit::cache::collect_jit_reloc_gc_roots_for_heap(group, heap_id)
    });
    sink("syntax-thread-local", &mut |group| {
        neovm_text::emacs_core::syntax::collect_syntax_gc_roots(group, heap_id)
    });
    sink("casetab-thread-local", &mut |group| {
        neovm_text::emacs_core::casetab::collect_casetab_gc_roots(group, heap_id)
    });
    sink("string-pos-cache-thread-local", &mut |group| {
        neovm_value::string_pos_cache::collect_string_pos_cache_gc_roots(
            group,
            heap_id,
            collection_epoch,
            scan,
        )
    });
    sink("category-thread-local", &mut |group| {
        neovm_text::emacs_core::category::collect_category_gc_roots(group, heap_id)
    });
    // Terminal Lisp state is traced from Context, including after migration.
    sink("dynamic-module-thread-local", &mut |group| {
        super::lifecycle::collect_dynamic_module_active_gc_roots(group, heap_id)
    });
    sink("regex-thread-local", &mut |group| {
        neovm_text::emacs_core::regex::collect_regex_gc_roots(
            group,
            heap_id,
            collection_epoch,
            scan,
        )
    });
    sink("symbol-name-thread-local", &mut |group| {
        neovm_value::intern::collect_symbol_name_gc_roots(group, heap_id)
    });
    sink("scratch-thread-local", &mut |roots| {
        super::SCRATCH_GC_ROOTS.with(|scratch| {
            for root in scratch.borrow().iter() {
                // Without an installed heap only immediate values can be
                // shared. Never trace an unattributed object pointer.
                if root.heap_identity == heap_id
                    || (root.heap_identity == 0 && !root.value.is_heap_object())
                {
                    roots.push(root.value);
                }
            }
        });
    });
}
