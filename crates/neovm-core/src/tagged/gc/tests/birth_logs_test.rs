//! Black-birth construction and used-prefix logging.
//! These isolate mutator logging; the coordinator owns real-cycle P-all tests.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::value::{HashTableTest, LambdaParams, LispHashTable};
use crate::heap_types::{LispMarker, LispString};
use std::ops::{Deref, DerefMut};

struct Roots(usize);

impl Roots {
    fn new() -> Self {
        Self(save_scratch_gc_roots())
    }

    fn keep(&self, value: TaggedValue) -> TaggedValue {
        push_scratch_gc_root(value);
        value
    }
}

impl Drop for Roots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn heap(on: bool) -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    heap.generational.enabled = on;
    heap.publish_barrier_window();
    heap
}

/// An isolated phase fixture: no worker is launched. RAII clears the synthetic
/// running flag even if an assertion panics, so heap Drop never tries to join
/// a nonexistent job. The ordinary begin entry closes regions/flips parity.
struct BirthPhase<'a>(&'a mut TaggedHeap);

impl<'a> BirthPhase<'a> {
    fn new(heap: &'a mut TaggedHeap, concurrent: bool) -> Self {
        heap.concurrent_begin();
        heap.generational.major_in_progress = heap.generational.enabled;
        heap.concurrent_mark_running = concurrent;
        Self(heap)
    }

    fn stop(&mut self) {
        self.0.close_alloc_regions();
        self.0.concurrent_mark_running = false;
    }
}

impl Deref for BirthPhase<'_> {
    type Target = TaggedHeap;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl DerefMut for BirthPhase<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl Drop for BirthPhase<'_> {
    fn drop(&mut self) {
        self.stop();
        self.0.mark_in_progress = false;
        self.0.generational.major_in_progress = false;
    }
}

fn header_addr(value: TaggedValue) -> usize {
    TaggedHeap::value_heap_addr(value).expect("a heap object")
}

fn cons_births(heap: &TaggedHeap) -> Vec<TaggedValue> {
    heap.mutators()
        .flat_map(|mutator| mutator.black_born_regions.iter())
        .flat_map(|region| region.cons_owners())
        .collect()
}

fn float_births(heap: &TaggedHeap) -> Vec<usize> {
    heap.mutators()
        .flat_map(|mutator| mutator.black_born_regions.iter())
        .flat_map(|region| region.float_headers())
        .map(|header| header as usize)
        .collect()
}

fn bytecode() -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    function.ops = vec![Op::Nil];
    function
}

fn marker() -> LispMarker {
    LispMarker {
        buffer: None,
        insertion_type: false,
        marker_id: None,
        bytepos: 0,
        charpos: 0,
        last_position_valid: false,
        next_marker: std::ptr::null_mut(),
        chained: false,
    }
}

#[test]
fn black_birth_logging_is_empty_in_legacy_concurrent_marking() {
    let mut heap = heap(false);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let mut phase = BirthPhase::new(&mut heap, true);
    roots.keep(phase.alloc_vector(vec![TaggedValue::NIL]));
    roots.keep(phase.alloc_cons(TaggedValue::T, TaggedValue::NIL));
    roots.keep(phase.alloc_float(1.25));
    phase.stop();
    let mutator = phase.current_mutator_gc();
    assert!(!phase.generational.major_in_progress);
    assert!(mutator.black_born.is_empty());
    assert!(mutator.black_born_regions.is_empty());
    assert!(mutator.black_cons_region_start.is_none());
    assert!(mutator.black_float_region_start.is_none());
}

#[test]
fn stop_the_world_major_allocations_need_no_black_birth_log() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let mut phase = BirthPhase::new(&mut heap, false);
    roots.keep(phase.alloc_vector(vec![TaggedValue::NIL]));
    roots.keep(phase.alloc_cons(TaggedValue::T, TaggedValue::NIL));
    roots.keep(phase.alloc_float(1.25));
    phase.stop();
    assert!(phase.current_mutator_gc().black_born.is_empty());
    assert!(phase.current_mutator_gc().black_born_regions.is_empty());
}

#[test]
fn black_deferred_sweep_allocations_need_no_major_birth_log() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    heap.set_sweep_in_progress_for_test(true);
    roots.keep(heap.alloc_vector(vec![TaggedValue::NIL]));
    roots.keep(heap.alloc_cons(TaggedValue::T, TaggedValue::NIL));
    roots.keep(heap.alloc_float(1.25));
    heap.set_sweep_in_progress_for_test(false);
    assert!(heap.current_mutator_gc().black_born.is_empty());
    assert!(heap.current_mutator_gc().black_born_regions.is_empty());
}

#[test]
fn every_page_header_and_box_link_records_a_fully_constructed_birth() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let mut phase = BirthPhase::new(&mut heap, true);
    let mut addresses = Vec::new();
    macro_rules! keep {
        ($allocation:expr) => {{
            let value = roots.keep($allocation);
            addresses.push(header_addr(value));
            value
        }};
    }
    keep!(phase.alloc_string(LispString::from_utf8("black birth")));
    keep!(phase.alloc_vector(vec![TaggedValue::fixnum(11)]));
    keep!(phase.alloc_lambda(vec![TaggedValue::NIL; 6]));
    keep!(phase.alloc_macro(vec![TaggedValue::NIL; 6]));
    keep!(phase.alloc_bytecode(bytecode()));
    let prototype = bytecode();
    keep!(phase.alloc_bytecode_instance(
        &prototype,
        vec![TaggedValue::fixnum(12)].into(),
        TaggedValue::NIL,
    ));
    keep!(phase.alloc_record(vec![TaggedValue::fixnum(13)]));
    keep!(phase.alloc_window_configuration(vec![TaggedValue::fixnum(14)]));
    keep!(phase.alloc_marker(marker()));
    keep!(phase.alloc_bignum(Integer::from(1u64) << 80u64));
    keep!(phase.alloc_symbol_with_pos(TaggedValue::T, TaggedValue::fixnum(15)));
    keep!(phase.alloc_hash_table(LispHashTable::new(HashTableTest::Eq)));
    let logged: Vec<usize> = phase
        .current_mutator_gc()
        .black_born
        .iter()
        .map(|header| *header as usize)
        .collect();
    assert_eq!(
        logged, addresses,
        "including the coordinator's Box link seam"
    );
    for header in &phase.current_mutator_gc().black_born {
        let header = unsafe { &**header };
        assert!(header.is_marked_at(phase.mark_parity));
        assert!(!header.tenured);
    }
    assert!(phase.current_mutator_gc().black_born_regions.is_empty());
}

#[test]
fn canonical_empty_string_reuse_is_not_a_new_birth() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let mut phase = BirthPhase::new(&mut heap, true);
    let first = roots.keep(phase.alloc_string(LispString::from_utf8("")));
    let second = roots.keep(phase.alloc_string(LispString::from_utf8("")));
    assert_eq!(first, second);
    assert_eq!(
        phase.current_mutator_gc().black_born,
        [header_addr(first) as *mut GcHeader]
    );
}

#[test]
fn partial_regions_log_only_used_cells_and_float_headers() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let mut phase = BirthPhase::new(&mut heap, true);
    let mut conses = Vec::new();
    let mut floats = Vec::new();
    for i in 0..3 {
        conses.push(roots.keep(phase.alloc_cons(TaggedValue::fixnum(i), TaggedValue::NIL)));
        floats.push(header_addr(roots.keep(phase.alloc_float(i as f64 + 0.25))));
    }
    assert!(phase.current_mutator_gc().black_born_regions.is_empty());
    assert!(phase.cons_region_for_test().1 > phase.cons_region_for_test().0);
    assert!(phase.float_region_for_test().1 > phase.float_region_for_test().0);
    phase.stop();
    assert_eq!(cons_births(&phase), conses);
    assert_eq!(float_births(&phase), floats);
    assert_eq!(phase.current_mutator_gc().black_born_regions.len(), 2);
    assert!(
        phase.current_mutator_gc().black_born.is_empty(),
        "floats log by region"
    );
    assert_eq!(
        phase.float_arena.pages[0].allocated, 3,
        "unused tail refunded"
    );
}

#[test]
fn fully_used_regions_log_before_the_zero_unused_early_return() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let mut phase = BirthPhase::new(&mut heap, true);
    let mut conses = Vec::new();
    let mut floats = Vec::new();
    for i in 0..CONS_REGION_MAX_CELLS {
        conses.push(roots.keep(phase.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::NIL)));
    }
    for i in 0..FLOAT_REGION_MAX_SLOTS {
        floats.push(header_addr(roots.keep(phase.alloc_float(i as f64))));
    }
    let (cons_cur, cons_lim) = phase.cons_region_for_test();
    let (float_cur, float_lim) = phase.float_region_for_test();
    assert_eq!(cons_cur, cons_lim);
    assert_eq!(float_cur, float_lim);
    phase.stop();
    assert_eq!(cons_births(&phase), conses);
    assert_eq!(float_births(&phase), floats);
    assert_eq!(phase.region_stats.cons_tail_closes, 0);
    assert_eq!(phase.region_stats.float_tail_closes, 0);
}

#[test]
fn exhausted_refills_preserve_each_preceding_used_prefix() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let mut phase = BirthPhase::new(&mut heap, true);
    for i in 0..(2 * CONS_REGION_MAX_CELLS + 1) {
        roots.keep(phase.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::NIL));
    }
    for i in 0..(2 * FLOAT_REGION_MAX_SLOTS + 1) {
        roots.keep(phase.alloc_float(i as f64));
    }
    assert_eq!(cons_births(&phase).len(), 2 * CONS_REGION_MAX_CELLS);
    assert_eq!(float_births(&phase).len(), 2 * FLOAT_REGION_MAX_SLOTS);
    phase.stop();
    assert_eq!(cons_births(&phase).len(), 2 * CONS_REGION_MAX_CELLS + 1);
    assert_eq!(float_births(&phase).len(), 2 * FLOAT_REGION_MAX_SLOTS + 1);
    let logged = phase.current_mutator_gc().black_born_regions.len();
    phase.close_alloc_regions();
    assert_eq!(phase.current_mutator_gc().black_born_regions.len(), logged);
}

#[test]
fn born_black_children_trace_without_consuming_promotion_logs() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let cons_child = roots.keep(heap.alloc_cons(TaggedValue::fixnum(17), TaggedValue::NIL));
    let vector_child = roots.keep(heap.alloc_string(LispString::from_utf8("constructor child")));
    let mut phase = BirthPhase::new(&mut heap, true);
    // Isolate this trace from ordinary root work; the scratch roots still
    // protect the test's Values throughout all allocation entry points.
    phase.gray_queue.clear();
    assert!(!phase.is_value_marked(cons_child));
    assert!(!phase.is_value_marked(vector_child));
    let born_cons = roots.keep(phase.alloc_cons(cons_child, TaggedValue::NIL));
    let born_vector = roots.keep(phase.alloc_vector(vec![vector_child]));
    assert!(phase.is_value_marked(born_cons));
    assert!(phase.is_value_marked(born_vector));
    phase.stop();
    let headers_before = phase.current_mutator_gc().black_born.clone();
    let regions_before = phase.current_mutator_gc().black_born_regions.len();
    phase.trace_black_born_children_world_stopped();
    phase.incremental_drain_all();
    assert!(phase.is_value_marked(cons_child));
    assert!(phase.is_value_marked(vector_child));
    assert_eq!(phase.current_mutator_gc().black_born, headers_before);
    assert_eq!(
        phase.current_mutator_gc().black_born_regions.len(),
        regions_before
    );
    assert_eq!(cons_births(&phase), [born_cons]);
    phase.trace_black_born_children_world_stopped();
    phase.incremental_drain_all();
    assert_eq!(phase.current_mutator_gc().black_born, headers_before);
    assert_eq!(
        phase.current_mutator_gc().black_born_regions.len(),
        regions_before
    );
}

#[test]
fn born_black_weak_table_registers_before_weak_decisions() {
    let mut heap = heap(true);
    set_tagged_heap(&mut heap);
    let roots = Roots::new();
    let strong = roots.keep(heap.alloc_string(LispString::from_utf8("custom weak test")));
    let key = roots.keep(heap.alloc_string(LispString::from_utf8("weak key")));
    let value = roots.keep(heap.alloc_cons(TaggedValue::fixnum(19), TaggedValue::NIL));
    let mut phase = BirthPhase::new(&mut heap, true);
    phase.gray_queue.clear();
    let mut table = LispHashTable::new_with_options(
        HashTableTest::Eq,
        4,
        Some(HashTableWeakness::Key),
        1.5,
        0.8,
    );
    table.user_cmp_function = Some(strong);
    table.insert(key.to_hash_key(&HashTableTest::Eq), key, value);
    let owner = roots.keep(phase.alloc_hash_table(table));
    phase.stop();
    phase.trace_black_born_children_world_stopped();
    phase.incremental_drain_all();
    assert!(
        phase
            .weak_hash_tables_set
            .contains(&(header_addr(owner) as *mut HashTableObj))
    );
    assert!(
        phase.is_value_marked(strong),
        "non-weak closure stays strong"
    );
    assert!(
        !phase.is_value_marked(key),
        "weak keys are not constructor roots"
    );
    assert!(
        !phase.is_value_marked(value),
        "weak entries await the weak pass"
    );
    assert_eq!(
        phase.current_mutator_gc().black_born,
        [header_addr(owner) as *mut GcHeader]
    );
}
