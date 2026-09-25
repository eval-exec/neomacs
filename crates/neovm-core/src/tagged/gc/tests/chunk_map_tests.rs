//! The chunk map (`chunk_map.rs`): the radix itself, its entries for every
//! block and page class through creation, release and re-indexing, the
//! ownership oracles answering exactly as the per-class registries do, and
//! concurrent cycles with it on (a TSan surface: the mutator adds pages
//! under a running marker).

use super::fake_image::FakeImage;
use super::knobs::set_chunk_map_for_test;
use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::value::{HashTableTest, LambdaParams, LispHashTable};
use crate::heap_types::LispMarker;

/// A heap with the chunk map on, installed on this thread.
fn chunk_heap() -> Box<TaggedHeap> {
    set_chunk_map_for_test(Some(true));
    let mut heap = Box::new(TaggedHeap::new());
    set_chunk_map_for_test(None);
    set_tagged_heap(&mut heap);
    assert!(heap.chunk_map.is_some());
    heap
}

fn map(heap: &TaggedHeap) -> &ChunkMap {
    heap.chunk_map.as_deref().expect("the chunk map is on")
}

fn addr_of(value: TaggedValue) -> usize {
    TaggedHeap::value_heap_addr(value).expect("a heap object")
}

fn marker(heap: &mut TaggedHeap, id: u64) -> TaggedValue {
    heap.alloc_marker(LispMarker {
        buffer: None,
        insertion_type: false,
        marker_id: Some(id),
        bytepos: 0,
        charpos: 0,
        last_position_valid: true,
        next_marker: std::ptr::null_mut(),
        chained: false,
    })
}

/// One object of every paged class, plus a boxed hash table.
fn one_of_each(heap: &mut TaggedHeap) -> Vec<(ChunkClass, TaggedValue)> {
    vec![
        (
            ChunkClass::Cons,
            heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL),
        ),
        (ChunkClass::Float, heap.alloc_float(2.5)),
        (
            ChunkClass::String,
            heap.alloc_string(crate::heap_types::LispString::from_utf8("s")),
        ),
        (
            ChunkClass::Vector,
            heap.alloc_vector(vec![TaggedValue::NIL; 2]),
        ),
        (
            ChunkClass::ByteCode,
            heap.alloc_bytecode(ByteCodeFunction::new(LambdaParams::simple(vec![]))),
        ),
        (
            ChunkClass::Lambda,
            heap.alloc_lambda(vec![TaggedValue::NIL; 3]),
        ),
        (
            ChunkClass::Macro,
            heap.alloc_macro(vec![TaggedValue::NIL; 3]),
        ),
        (
            ChunkClass::Record,
            heap.alloc_record(vec![TaggedValue::T; 2]),
        ),
        (
            ChunkClass::SymbolWithPos,
            heap.alloc_symbol_with_pos(TaggedValue::T, TaggedValue::fixnum(3)),
        ),
        (ChunkClass::Marker, marker(heap, 1)),
        (
            ChunkClass::Bignum,
            heap.alloc_bignum(Integer::from(1u64 << 62)),
        ),
        (
            ChunkClass::None,
            heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq)),
        ),
    ]
}

/// The heap's answer for `value` through the registries: the chunk map
/// taken out, so every oracle falls back to its `FxHashMap` path.
fn registry_owns(heap: &mut TaggedHeap, value: TaggedValue) -> bool {
    let saved = heap.chunk_map.take();
    let owns = if value.is_cons() {
        let base = ConsBlock::block_base_for_ptr(value.xcons_ptr());
        heap.cons_block_index_by_base.contains_key(&base)
    } else {
        heap.owns_heap_value_object(value, addr_of(value))
    };
    heap.chunk_map = saved;
    owns
}

fn chunk_owns(heap: &TaggedHeap, value: TaggedValue) -> bool {
    if value.is_cons() {
        map(heap).get(addr_of(value)).is(ChunkClass::Cons)
    } else {
        heap.owns_heap_value_object(value, addr_of(value))
    }
}

#[test]
fn the_radix_answers_none_until_set_and_after_clear() {
    let map = ChunkMap::new();
    for addr in [0usize, 0x1000, 0x7fff_ffff_0000, usize::MAX, 1 << 47] {
        assert_eq!(map.get(addr), ChunkEntry::NONE, "{addr:#x}");
    }
    let base = 0x5555_0001_0000usize;
    let entry = ChunkEntry::new(ChunkClass::Record, 12345);
    map.set(base, entry);
    // Every address in the granule answers; its neighbours do not.
    assert_eq!(map.get(base), entry);
    assert_eq!(map.get(base + 0xffff), entry);
    assert_eq!(map.get(base + 0x1_0000), ChunkEntry::NONE);
    assert_eq!(map.get(base - 1), ChunkEntry::NONE);
    // Another 4 GiB region gets its own leaf.
    let far = base + (1usize << 33);
    map.set(far, ChunkEntry::new(ChunkClass::Cons, 0));
    assert!(map.get(far).is(ChunkClass::Cons));
    assert_eq!(map.get(base), entry);
    map.set(base, ChunkEntry::NONE);
    assert_eq!(map.get(base), ChunkEntry::NONE);
    // Clearing an unset granule of a region with no leaf allocates nothing.
    map.set(0x10_0000_0000, ChunkEntry::NONE);
    assert_eq!(map.get(0x10_0000_0000), ChunkEntry::NONE);
}

#[test]
fn entries_pack_class_and_index() {
    for class in [
        ChunkClass::Cons,
        ChunkClass::Float,
        ChunkClass::String,
        ChunkClass::Vector,
        ChunkClass::ByteCode,
        ChunkClass::Lambda,
        ChunkClass::Macro,
        ChunkClass::Record,
        ChunkClass::SymbolWithPos,
        ChunkClass::Marker,
        ChunkClass::Bignum,
    ] {
        for index in [0usize, 1, 4096, ChunkEntry::MAX_INDEX] {
            let entry = ChunkEntry::new(class, index);
            assert_eq!(entry.class(), class);
            assert!(entry.is(class));
            assert_eq!(entry.index(), index);
            assert_ne!(entry, ChunkEntry::NONE);
        }
    }
    assert_eq!(ChunkEntry::NONE.class(), ChunkClass::None);
}

/// Every block and page class is registered with its class and its index
/// in the owning collection; boxed, image, static and stack addresses are
/// not.
#[test]
fn every_block_and_page_is_registered_with_its_index() {
    let mut heap = chunk_heap();
    let objects = one_of_each(&mut heap);
    for &(class, value) in &objects {
        let entry = map(&heap).get(addr_of(value));
        assert_eq!(entry.class(), class, "{class:?}");
        if class == ChunkClass::Cons {
            let base = ConsBlock::block_base_for_ptr(value.xcons_ptr());
            assert_eq!(heap.cons_blocks[entry.index()].base_addr(), base);
            assert_eq!(heap.cons_block_index_by_base[&base], entry.index());
        }
    }
    let image = FakeImage::leak(false);
    let image_vector = image.register_vector(&mut heap);
    let image_cons = image.register_cons(&mut heap);
    assert_eq!(map(&heap).get(addr_of(image_vector)), ChunkEntry::NONE);
    assert_eq!(map(&heap).get(addr_of(image_cons)), ChunkEntry::NONE);
    let on_stack = 0u64;
    assert_eq!(
        map(&heap).get(&on_stack as *const u64 as usize),
        ChunkEntry::NONE
    );
    static STATIC_WORD: u64 = 7;
    assert_eq!(
        map(&heap).get(&STATIC_WORD as *const u64 as usize),
        ChunkEntry::NONE
    );
}

/// The oracles answer exactly as the registries, for live objects of every
/// class, for freed slots, and for image objects.
#[test]
fn ownership_through_the_map_equals_the_registries() {
    let mut heap = chunk_heap();
    let keep = one_of_each(&mut heap);
    let dead = one_of_each(&mut heap);
    let mut root = TaggedValue::NIL;
    for &(_, value) in &keep {
        root = heap.alloc_cons(value, root);
    }
    // Remember the dead objects' addresses before the sweep frees them.
    let dead_values: Vec<TaggedValue> = dead.iter().map(|&(_, v)| v).collect();
    for &(class, value) in keep.iter().chain(dead.iter()) {
        assert_eq!(
            chunk_owns(&heap, value),
            registry_owns(&mut heap, value),
            "live {class:?}"
        );
    }
    heap.collect_exact(std::iter::once(root));
    for &(class, value) in &keep {
        assert!(chunk_owns(&heap, value), "kept {class:?}");
        assert_eq!(chunk_owns(&heap, value), registry_owns(&mut heap, value));
    }
    for value in dead_values.iter().copied().filter(|v| !v.is_cons()) {
        assert!(!chunk_owns(&heap, value), "freed {value:?}");
        assert_eq!(chunk_owns(&heap, value), registry_owns(&mut heap, value));
    }
    let image = FakeImage::leak(false);
    let image_vector = image.register_vector(&mut heap);
    assert!(!chunk_owns(&heap, image_vector));
    assert_eq!(
        chunk_owns(&heap, image_vector),
        registry_owns(&mut heap, image_vector)
    );
}

/// Releasing pages clears their granules and re-indexes the survivors: a
/// page's entry always names its current index.
#[test]
fn released_pages_leave_the_map_and_survivors_are_reindexed() {
    let mut heap = chunk_heap();
    // Three vector pages: the first and third hold survivors.
    let per_page = ObjectPage::<VectorObj>::SLOTS;
    let mut all = Vec::new();
    for i in 0..3 * per_page {
        all.push(heap.alloc_vector(vec![TaggedValue::fixnum(i as i64)]));
    }
    assert_eq!(heap.vector_arena.pages.len(), 3);
    let keep_first = all[0];
    let keep_third = all[2 * per_page];
    let middle_base = addr_of(all[per_page]) & !(OBJECT_PAGE_ALIGN - 1);
    assert!(map(&heap).get(middle_base).is(ChunkClass::Vector));
    let root = heap.alloc_cons(keep_first, keep_third);
    drop(all);
    heap.collect_exact(std::iter::once(root));
    assert_eq!(
        heap.vector_arena.pages.len(),
        2,
        "the middle page is released"
    );
    assert_eq!(map(&heap).get(middle_base), ChunkEntry::NONE);
    for (index, page) in heap.vector_arena.pages.iter().enumerate() {
        let entry = map(&heap).get(page.base_addr());
        assert!(entry.is(ChunkClass::Vector));
        assert_eq!(entry.index(), index, "re-indexed");
    }
    assert!(heap.owns_veclike_object(addr_of(keep_third) as *const u8));
    // A new page may reuse the released storage: its entry is the new one.
    let mut fresh = Vec::new();
    while heap.vector_arena.pages.len() < 3 {
        fresh.push(heap.alloc_vector(vec![TaggedValue::NIL]));
    }
    let last = heap.vector_arena.pages.len() - 1;
    let entry = map(&heap).get(heap.vector_arena.pages[last].base_addr());
    assert_eq!(entry, ChunkEntry::new(ChunkClass::Vector, last));
}

/// Released cons blocks leave the map, and the survivors are re-indexed on
/// both release paths (the eager sweep and the deferred sweep's end).
#[test]
fn released_cons_blocks_leave_the_map() {
    let mut heap = chunk_heap();
    let mut lists = Vec::new();
    for _ in 0..3 {
        let mut l = TaggedValue::NIL;
        for i in 0..CONS_BLOCK_SIZE {
            l = heap.alloc_cons(TaggedValue::fixnum(i as i64), l);
        }
        lists.push(l);
    }
    assert!(heap.cons_blocks.len() >= 3);
    let bases: Vec<usize> = heap.cons_blocks.iter().map(ConsBlock::base_addr).collect();
    // Keep only the last list: the blocks wholly holding the others empty.
    let root = lists[2];
    heap.collect_exact(std::iter::once(root));
    let live: FxHashSet<usize> = heap.cons_blocks.iter().map(ConsBlock::base_addr).collect();
    assert!(
        live.len() < bases.len(),
        "some block must have been released"
    );
    for base in bases {
        let entry = map(&heap).get(base);
        if live.contains(&base) {
            assert!(entry.is(ChunkClass::Cons));
            assert_eq!(heap.cons_blocks[entry.index()].base_addr(), base);
        } else {
            assert_eq!(entry, ChunkEntry::NONE, "released block {base:#x}");
        }
    }
    assert!(heap.is_value_marked(root));
}

fn run_concurrent_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.concurrent_begin();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    for &root in roots {
        heap.seed_root(root);
    }
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(!heap.sweep_in_progress());
}

/// Concurrent cycles with the map on keep every live object of every class
/// and free the garbage, while the mutator adds blocks and pages under the
/// running marker.
#[test]
fn chunk_map_concurrent_cycles_keep_the_live_set_while_pages_grow() {
    let mut heap = chunk_heap();
    let mut root = TaggedValue::NIL;
    for &(_, value) in &one_of_each(&mut heap) {
        root = heap.alloc_cons(value, root);
    }
    let mut spine = TaggedValue::NIL;
    for i in 0..20_000 {
        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8(&format!("{i}")));
        let v = heap.alloc_vector(vec![s, TaggedValue::fixnum(i)]);
        spine = heap.alloc_cons(v, spine);
    }
    root = heap.alloc_cons(spine, root);
    heap.collect_exact(std::iter::once(root));
    assert!(heap.should_run_concurrent());
    for round in 0..3 {
        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();
        // New blocks and pages while the GC thread reads the map.
        let mut fresh = TaggedValue::NIL;
        for i in 0..5_000 {
            let f = heap.alloc_float(i as f64);
            let v = heap.alloc_vector(vec![f]);
            fresh = heap.alloc_cons(v, fresh);
        }
        let _garbage: Vec<TaggedValue> = (0..2_000)
            .map(|i| heap.alloc_string(crate::heap_types::LispString::from_utf8(&format!("g{i}"))))
            .collect();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        heap.seed_root(fresh);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        root = heap.alloc_cons(fresh, root);
        assert!(
            heap.owns_veclike_object(
                addr_of(unsafe { (*fresh.xcons_ptr()).load_car() }) as *const u8
            )
        );
        let _ = round;
    }
    run_concurrent_cycle(&mut heap, &[root]);
    // Every string on the spine is still intact.
    let mut cell = spine;
    let mut n = 0;
    while cell.is_cons() {
        let v = unsafe { (*cell.xcons_ptr()).load_car() };
        let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const VectorObj) };
        let s = obj.data.load_atomic(0);
        assert!(heap.owns_string_object(s.as_string_ptr().unwrap() as *const u8));
        n += 1;
        cell = unsafe { (*cell.xcons_ptr()).load_cdr() };
    }
    assert_eq!(n, 20_000);
    heap.assert_object_arenas_coherent();
}
