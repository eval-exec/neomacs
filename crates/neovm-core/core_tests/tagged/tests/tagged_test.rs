//! Core-owned half of the historical `tagged::tests` module (Prep D C split, R7a).
//!
//! The tests that call the upper `Context`/bytecode/subr-factory boundary moved
//! here with unchanged assertions. The remaining lower-only tagged tests stay in
//! `neovm-value`. After the surface accessor relocation, `tagged/gc.rs` is a
//! 2990-line facade, so `gc_root_stays_a_facade_after_the_domain_split` keeps
//! its 3000-line ceiling and assertion; only the file path resolves to the
//! value crate.
use crate::emacs_core::eval::SubrValueExt;
use crate::emacs_core::intern::{intern, intern_uninterned};
use crate::tagged::header::{SubrDispatchKind, VecLikeType};
use crate::tagged::value::TaggedValue;
use neovm_eval::test_support::authority::{lookup_global_subr_entry, subr_entry_dispatch_kind};

#[test]
fn subr_dispatch_kind_from_global_table() {
    crate::test_utils::init_test_tracing();
    // Dispatch kinds are now stored in the global SubrEntry table,
    // not on heap SubrObj instances. This test verifies that after
    // a Context is created (which registers builtins), the dispatch
    // kinds can be looked up from the global table.
    let _ctx = crate::test_utils::new_context();

    let car_id = intern("car");
    let if_id = intern("if");
    let throw_id = intern("throw");

    let car_entry = lookup_global_subr_entry(car_id).expect("car registered");
    let if_entry = lookup_global_subr_entry(if_id).expect("if registered");
    let throw_entry = lookup_global_subr_entry(throw_id).expect("throw registered");

    assert_eq!(
        subr_entry_dispatch_kind(&car_entry),
        SubrDispatchKind::Builtin
    );
    assert_eq!(
        subr_entry_dispatch_kind(&if_entry),
        SubrDispatchKind::SpecialForm
    );
    assert_eq!(
        subr_entry_dispatch_kind(&throw_entry),
        SubrDispatchKind::ContextCallable
    );
}

#[test]
fn subr_is_gnu_shaped_vectorlike() {
    crate::test_utils::init_test_tracing();
    let sym = intern("tagged-subr-test");
    let subr = TaggedValue::subr(sym);
    assert!(subr.is_subr());
    assert_eq!(subr.tag(), 0b101);
    assert_eq!(subr.veclike_type(), Some(VecLikeType::Subr));
    assert_eq!(subr.as_subr_id(), Some(sym));
}

#[test]
fn subr_object_resolves_by_public_name() {
    crate::test_utils::init_test_tracing();

    let canonical = intern("car");
    let canonical_subr = TaggedValue::subr(canonical);

    assert_eq!(canonical_subr.as_subr_id(), Some(canonical));

    let uninterned = intern_uninterned("car");
    let uninterned_subr = TaggedValue::subr(uninterned);
    assert_eq!(uninterned_subr.as_subr_id(), Some(canonical));
    assert_eq!(canonical_subr.bits(), uninterned_subr.bits());
}

// ---------------------------------------------------------------------------
// Non-cons allocator cost probe (size-class arena design input)
// ---------------------------------------------------------------------------

/// PROFILING AID (not a pass/fail test): measure the CURRENT non-cons
/// allocator's end-to-end cost per size class — `Box` allocation + intrusive
/// link + `non_cons_object_addrs` insert on the alloc side, and clear-marks +
/// sweep-walk + addr-set remove + `Box` drop on the free side (an empty-roots
/// `collect_exact`, so mark work is nil and the collection is clear+sweep).
/// This is the "before" bound for the size-class arena-page redesign. Run:
///   cargo nextest run -p neovm-core --release --run-ignored ignored-only \
///     --no-capture -E 'test(alloc_roundtrip_cost_probe)'

#[test]
#[cfg(feature = "perf-probes")]
fn alloc_roundtrip_cost_probe() {
    use crate::heap_types::LispString;
    use crate::tagged::gc::TaggedHeap;
    use std::time::Instant;

    fn run_case(
        label: &str,
        m: usize,
        mut alloc_one: impl FnMut(&mut TaggedHeap),
        out: &mut String,
    ) {
        // Fresh heap per case isolates the population and the addr set.
        let mut heap = TaggedHeap::new();
        // Warm-up: page in allocator paths, then free everything.
        for _ in 0..1000 {
            alloc_one(&mut heap);
        }
        heap.collect_exact(std::iter::empty());

        let t0 = Instant::now();
        for _ in 0..m {
            alloc_one(&mut heap);
        }
        let alloc_ns = t0.elapsed().as_nanos() as f64 / m as f64;

        // Free side: empty-roots collection frees all M objects.
        let t1 = Instant::now();
        heap.collect_exact(std::iter::empty());
        let free_total = t1.elapsed();
        // Baseline: same collection on the now-empty heap (fixed overhead).
        let t2 = Instant::now();
        heap.collect_exact(std::iter::empty());
        let baseline = t2.elapsed();
        let free_ns = (free_total.saturating_sub(baseline)).as_nanos() as f64 / m as f64;

        out.push_str(&format!(
            "{label:<28} n={m:>7}  alloc={alloc_ns:>7.1} ns/obj  free={free_ns:>7.1} ns/obj  \
             (collect={:.2} ms, empty-heap baseline={:.3} ms)\n",
            free_total.as_secs_f64() * 1e3,
            baseline.as_secs_f64() * 1e3,
        ));
    }

    let m = 200_000;
    let mut out = String::new();
    out.push_str("CURRENT (Box + FxHashSet + intrusive list) alloc/free round-trip:\n");

    run_case(
        "float (24B fixed)",
        m,
        |h| {
            h.alloc_float(1.5);
        },
        &mut out,
    );
    for payload in [0usize, 48, 240, 1008, 4080] {
        run_case(
            &format!("string payload={payload}B"),
            m,
            move |h| {
                h.alloc_string(LispString::from_unibyte(vec![b'x'; payload]));
            },
            &mut out,
        );
    }
    for len in [0usize, 6, 30, 126, 1022] {
        run_case(
            &format!("vector len={len} ({}B back)", len * 8),
            m,
            move |h| {
                h.alloc_vector(vec![TaggedValue::fixnum(1); len]);
            },
            &mut out,
        );
    }
    run_case(
        "record len=4",
        m,
        |h| {
            h.alloc_record(vec![TaggedValue::fixnum(7); 4]);
        },
        &mut out,
    );
    run_case(
        "lambda 6 slots",
        m,
        |h| {
            h.alloc_lambda(vec![TaggedValue::NIL; 6]);
        },
        &mut out,
    );
    run_case(
        "macro 6 slots",
        m,
        |h| {
            h.alloc_macro(vec![TaggedValue::NIL; 6]);
        },
        &mut out,
    );
    run_case(
        "bytecode (~360B fixed)",
        m,
        |h| {
            h.alloc_bytecode(crate::emacs_core::bytecode::ByteCodeFunction::new(
                crate::emacs_core::value::LambdaParams::simple(vec![]),
            ));
        },
        &mut out,
    );
    run_case(
        "symbol-with-pos (40B fixed)",
        m,
        |h| {
            h.alloc_symbol_with_pos(TaggedValue::T, TaggedValue::fixnum(3));
        },
        &mut out,
    );
    for limbs in [1usize, 4, 64] {
        // `limbs == 1` is malachite `Small` (no limb allocation); otherwise
        // the Integer owns a limb Vec the sweep's drop_in_place frees.
        let value = if limbs == 1 {
            malachite::integer::Integer::from(u64::MAX)
        } else {
            malachite::integer::Integer::from(1) << (64 * limbs as u64 - 1)
        };
        run_case(
            &format!("bignum {limbs} limb(s)"),
            m,
            move |h| {
                h.alloc_bignum(value.clone());
            },
            &mut out,
        );
    }

    // Report via panic! so nextest surfaces the dump (profiling aid pattern).
    panic!("ALLOC ROUND-TRIP PROBE (profiling aid, not a failure)\n{out}");
}

/// `tagged/gc.rs` was split from 18,077 lines: 8,116 lines of inline test
/// modules moved to `gc/<name>.rs`, then allocation, mark-sweep, concurrent
/// marking, incremental marking, the cons block allocator, the arena pages,
/// and the background GC thread each went to a child module. What remains is
/// the `TaggedHeap` struct, the post-mark ownership verification gate, the
/// marker-chain and vector-link helpers, `Drop`, and the module declarations.
/// New collector work goes in the child module for its domain, or a new one;
/// this ceiling keeps the root from silently re-absorbing them.
#[test]
fn gc_root_stays_a_facade_after_the_domain_split() {
    const CEILING: usize = 3_000;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the core crate lives under crates/")
        .join("neovm-value/src/tagged/gc.rs");
    let lines = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .lines()
        .count();
    assert!(
        lines <= CEILING,
        "tagged/gc.rs is {lines} lines (ceiling {CEILING}); put the new code in the gc/ child \
         module for its domain instead of growing the root"
    );
}
