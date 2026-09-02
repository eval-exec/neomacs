//! Tests for the tagged pointer value system.

use super::gc::{HeapWriteKind, HeapWriteRecord};
use super::header::*;
use super::value::*;
use crate::emacs_core::intern::{SymId, intern, intern_uninterned};

#[test]
fn nil_is_zero() {
    crate::test_utils::init_test_tracing();
    assert_eq!(TaggedValue::NIL.bits(), 0);
    assert!(TaggedValue::NIL.is_nil());
    assert!(TaggedValue::NIL.is_symbol());
    assert!(TaggedValue::NIL.is_list());
    assert!(!TaggedValue::NIL.is_cons());
    assert!(!TaggedValue::NIL.is_fixnum());
}

#[test]
fn t_is_symbol_1() {
    crate::test_utils::init_test_tracing();
    assert_eq!(TaggedValue::T.bits(), 8); // 1 << 3
    assert!(TaggedValue::T.is_t());
    assert!(TaggedValue::T.is_symbol());
    assert!(!TaggedValue::T.is_nil());
    assert_eq!(TaggedValue::T.as_symbol_id(), Some(SymId(1)));
}

#[test]
fn fixnum_encoding() {
    crate::test_utils::init_test_tracing();
    let zero = TaggedValue::fixnum(0);
    assert!(zero.is_fixnum());
    assert_eq!(zero.as_fixnum(), Some(0));
    assert!(!zero.is_nil()); // fixnum 0 != nil

    let one = TaggedValue::fixnum(1);
    assert!(one.is_fixnum());
    assert_eq!(one.as_fixnum(), Some(1));

    let neg = TaggedValue::fixnum(-42);
    assert!(neg.is_fixnum());
    assert_eq!(neg.as_fixnum(), Some(-42));

    let big = TaggedValue::fixnum(1_000_000_000);
    assert!(big.is_fixnum());
    assert_eq!(big.as_fixnum(), Some(1_000_000_000));

    // Max/min fixnum
    let max = TaggedValue::fixnum(TaggedValue::MOST_POSITIVE_FIXNUM);
    assert_eq!(max.as_fixnum(), Some(TaggedValue::MOST_POSITIVE_FIXNUM));

    let min = TaggedValue::fixnum(TaggedValue::MOST_NEGATIVE_FIXNUM);
    assert_eq!(min.as_fixnum(), Some(TaggedValue::MOST_NEGATIVE_FIXNUM));
}

#[test]
fn fixnum_not_nil() {
    crate::test_utils::init_test_tracing();
    // Fixnum 0 must NOT be nil (nil is Symbol(0) with tag 000)
    let zero = TaggedValue::fixnum(0);
    assert!(!zero.is_nil());
    assert!(!zero.is_symbol());
    assert!(zero.is_fixnum());
    // GNU low-tag layout: fixnum 0 is (0 << 2) | 2.
    assert_eq!(zero.bits(), 2);
}

#[test]
fn gnu_low_tag_layout() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();

    assert_eq!(TaggedValue::from_sym_id(SymId(42)).tag(), 0b000);
    assert_eq!(TaggedValue::fixnum(0).tag(), 0b010);
    assert_eq!(TaggedValue::fixnum(1).tag(), 0b110);
    assert_eq!(
        heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL).tag(),
        0b011
    );
    assert_eq!(
        heap.alloc_string(crate::heap_types::LispString::from_utf8("x"))
            .tag(),
        0b100
    );
    assert_eq!(heap.alloc_vector(Vec::new()).tag(), 0b101);
    assert_eq!(heap.alloc_float(1.0).tag(), 0b111);
}

#[test]
fn gnu_pvec_type_layout_for_shared_vectorlikes() {
    crate::test_utils::init_test_tracing();

    let shared = [
        (VecLikeType::Vector, 0),
        (VecLikeType::Bignum, 2),
        (VecLikeType::Marker, 3),
        (VecLikeType::Overlay, 4),
        (VecLikeType::SymbolWithPos, 6),
        (VecLikeType::UserPtr, 8),
        (VecLikeType::Frame, 10),
        (VecLikeType::Window, 11),
        (VecLikeType::Buffer, 13),
        (VecLikeType::HashTable, 14),
        (VecLikeType::Obarray, 15),
        (VecLikeType::Terminal, 16),
        (VecLikeType::WindowConfiguration, 17),
        (VecLikeType::Subr, 18),
        (VecLikeType::Xwidget, 20),
        (VecLikeType::XwidgetView, 21),
        (VecLikeType::ModuleFunction, 25),
        (VecLikeType::Sqlite, 30),
        (VecLikeType::Lambda, 31),
        (VecLikeType::CharTable, 32),
        (VecLikeType::SubCharTable, 33),
        (VecLikeType::Record, 34),
        (VecLikeType::Font, 35),
    ];

    for (kind, gnu_code) in shared {
        assert_eq!(u8::from(kind), gnu_code);
        assert_eq!(VecLikeType::try_from(gnu_code), Ok(kind));
        assert_eq!(kind.gnu_pvec_code(), Some(gnu_code));
    }

    assert!(VecLikeType::try_from(1).is_err());
    assert_eq!(GnuPvecType::from_gnu_code(1), Some(GnuPvecType::Free));
    assert_eq!(VecLikeType::Macro.gnu_pvec_code(), None);
    assert_eq!(VecLikeType::ByteCode.gnu_pvec_code(), None);
    assert_eq!(VecLikeType::Timer.gnu_pvec_code(), None);
    assert_eq!(VecLikeType::SurfaceHandle.gnu_pvec_code(), None);
    assert_eq!(VecLikeType::VideoHandle.gnu_pvec_code(), None);
}

#[test]
fn gnu_pvec_type_layout_matches_gnu_lisp_h() {
    crate::test_utils::init_test_tracing();

    let cases = [
        (GnuPvecType::NormalVector, 0),
        (GnuPvecType::Free, 1),
        (GnuPvecType::Bignum, 2),
        (GnuPvecType::Marker, 3),
        (GnuPvecType::Overlay, 4),
        (GnuPvecType::Finalizer, 5),
        (GnuPvecType::SymbolWithPos, 6),
        (GnuPvecType::MiscPtr, 7),
        (GnuPvecType::UserPtr, 8),
        (GnuPvecType::Process, 9),
        (GnuPvecType::Frame, 10),
        (GnuPvecType::Window, 11),
        (GnuPvecType::BoolVector, 12),
        (GnuPvecType::Buffer, 13),
        (GnuPvecType::HashTable, 14),
        (GnuPvecType::Obarray, 15),
        (GnuPvecType::Terminal, 16),
        (GnuPvecType::WindowConfiguration, 17),
        (GnuPvecType::Subr, 18),
        (GnuPvecType::Other, 19),
        (GnuPvecType::Xwidget, 20),
        (GnuPvecType::XwidgetView, 21),
        (GnuPvecType::Thread, 22),
        (GnuPvecType::Mutex, 23),
        (GnuPvecType::Condvar, 24),
        (GnuPvecType::ModuleFunction, 25),
        (GnuPvecType::NativeCompUnit, 26),
        (GnuPvecType::TsParser, 27),
        (GnuPvecType::TsNode, 28),
        (GnuPvecType::TsCompiledQuery, 29),
        (GnuPvecType::Sqlite, 30),
        (GnuPvecType::Closure, 31),
        (GnuPvecType::CharTable, 32),
        (GnuPvecType::SubCharTable, 33),
        (GnuPvecType::Record, 34),
        (GnuPvecType::Font, 35),
    ];

    for (kind, code) in cases {
        assert_eq!(kind.gnu_code(), code);
        assert_eq!(GnuPvecType::from_gnu_code(code), Some(kind));
    }
    assert_eq!(GnuPvecType::from_gnu_code(36), None);
}

#[test]
fn q_unbound_is_not_user_interned_unbound_symbol() {
    crate::test_utils::init_test_tracing();

    let user_unbound = TaggedValue::from_sym_id(intern("unbound"));

    assert!(TaggedValue::UNBOUND.is_unbound());
    assert!(!user_unbound.is_unbound());
    assert_ne!(TaggedValue::UNBOUND.bits(), user_unbound.bits());
}

#[test]
fn symbol_encoding() {
    crate::test_utils::init_test_tracing();
    let sym = TaggedValue::from_sym_id(SymId(42));
    assert!(sym.is_symbol());
    assert_eq!(sym.as_symbol_id(), Some(SymId(42)));
    assert!(!sym.is_fixnum());
    assert!(!sym.is_nil());
    assert!(!sym.is_cons());
}

#[test]
fn char_is_fixnum() {
    crate::test_utils::init_test_tracing();
    // In GNU Emacs, characters ARE integers. ?A is just 65.
    let ch = TaggedValue::char('A');
    assert!(ch.is_fixnum()); // chars are fixnums
    assert!(ch.is_char()); // characterp checks range
    assert_eq!(ch.as_fixnum(), Some(65)); // ?A = 65
    assert_eq!(ch.as_char(), Some('A'));
    // (eq ?A 65) must be t
    assert_eq!(ch.bits(), TaggedValue::fixnum(65).bits());

    // Unicode
    let emoji = TaggedValue::char('🦀');
    assert_eq!(emoji.as_char(), Some('🦀'));
    assert!(emoji.is_fixnum());
}

#[test]
fn keyword_is_symbol() {
    crate::test_utils::init_test_tracing();
    // In GNU Emacs, keywords are ordinary symbols with : prefix
    let kw = TaggedValue::from_kw_id(SymId(99));
    assert!(kw.is_symbol()); // keywords are symbols
    assert_eq!(kw.as_symbol_id(), Some(SymId(99)));
    // as_keyword_id delegates to as_symbol_id for keyword-named symbols
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
fn subr_dispatch_kind_from_global_table() {
    crate::test_utils::init_test_tracing();
    // Dispatch kinds are now stored in the global SubrEntry table,
    // not on heap SubrObj instances. This test verifies that after
    // a Context is created (which registers builtins), the dispatch
    // kinds can be looked up from the global table.
    let _ctx = crate::emacs_core::eval::Context::new();

    let car_id = intern("car");
    let if_id = intern("if");
    let throw_id = intern("throw");

    let car_entry =
        crate::emacs_core::eval::lookup_global_subr_entry(car_id).expect("car registered");
    let if_entry = crate::emacs_core::eval::lookup_global_subr_entry(if_id).expect("if registered");
    let throw_entry =
        crate::emacs_core::eval::lookup_global_subr_entry(throw_id).expect("throw registered");

    assert_eq!(car_entry.dispatch_kind, SubrDispatchKind::Builtin);
    assert_eq!(if_entry.dispatch_kind, SubrDispatchKind::SpecialForm);
    assert_eq!(throw_entry.dispatch_kind, SubrDispatchKind::ContextCallable);
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

#[test]
fn cons_allocation_and_access() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();

    let car = TaggedValue::fixnum(1);
    let cdr = TaggedValue::fixnum(2);
    let cons = heap.alloc_cons(car, cdr);

    assert!(cons.is_cons());
    assert!(cons.is_list());
    assert!(!cons.is_nil());
    assert_eq!(cons.cons_car().as_fixnum(), Some(1));
    assert_eq!(cons.cons_cdr().as_fixnum(), Some(2));
}

#[test]
fn cons_set_car_cdr() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();

    let cons = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    assert_eq!(cons.cons_car().as_fixnum(), Some(1));
    assert!(cons.cons_cdr().is_nil());

    cons.set_car(TaggedValue::fixnum(99));
    cons.set_cdr(TaggedValue::fixnum(100));
    assert_eq!(cons.cons_car().as_fixnum(), Some(99));
    assert_eq!(cons.cons_cdr().as_fixnum(), Some(100));
}

#[test]
fn nested_cons_list() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();

    // Build list (1 2 3)
    let c3 = heap.alloc_cons(TaggedValue::fixnum(3), TaggedValue::NIL);
    let c2 = heap.alloc_cons(TaggedValue::fixnum(2), c3);
    let c1 = heap.alloc_cons(TaggedValue::fixnum(1), c2);

    assert_eq!(c1.cons_car().as_fixnum(), Some(1));
    assert_eq!(c1.cons_cdr().cons_car().as_fixnum(), Some(2));
    assert_eq!(c1.cons_cdr().cons_cdr().cons_car().as_fixnum(), Some(3));
    assert!(c1.cons_cdr().cons_cdr().cons_cdr().is_nil());
}

#[test]
fn float_allocation() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();

    let f = heap.alloc_float(3.125);
    assert!(f.is_float());
    assert!((f.xfloat() - 3.125).abs() < f64::EPSILON);
}

#[test]
fn vector_allocation() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();

    let items = vec![TaggedValue::fixnum(10), TaggedValue::fixnum(20)];
    let vec = heap.alloc_vector(items);
    assert!(vec.is_veclike());
    assert_eq!(vec.veclike_type(), Some(VecLikeType::Vector));
}

#[test]
fn vector_mutation_helper_updates_elements() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();
    super::gc::set_tagged_heap(&mut heap);

    let vec = heap.alloc_vector(vec![TaggedValue::fixnum(10), TaggedValue::fixnum(20)]);
    let _ = super::mutate::with_vector_data_mut(vec, |items| {
        items[1] = TaggedValue::fixnum(99);
    });

    let items = unsafe { &(*(vec.as_veclike_ptr().unwrap() as *const VectorObj)).data };
    assert_eq!(items[0].as_fixnum(), Some(10));
    assert_eq!(items[1].as_fixnum(), Some(99));
}

#[test]
fn heap_write_tracking_records_unique_mutated_owners_and_slot_events() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();
    heap.set_write_tracking_mode(super::gc::WriteTrackingMode::OwnersAndRecords);
    super::gc::set_tagged_heap(&mut heap);

    let cons = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    let vec = heap.alloc_vector(vec![TaggedValue::fixnum(10), TaggedValue::fixnum(20)]);

    cons.set_car(TaggedValue::fixnum(2));
    cons.set_cdr(vec);
    assert_eq!(heap.dirty_owner_count(), 1);
    assert!(heap.is_dirty_owner(cons));
    assert_eq!(heap.dirty_write_count(), 2);
    assert_eq!(
        heap.dirty_writes(),
        &[
            HeapWriteRecord::slot(cons, HeapWriteKind::ConsCar, 0, TaggedValue::fixnum(2)),
            HeapWriteRecord::slot(cons, HeapWriteKind::ConsCdr, 1, vec),
        ]
    );

    assert!(super::mutate::set_vector_slot(
        vec,
        1,
        TaggedValue::fixnum(99)
    ));
    assert_eq!(heap.dirty_owner_count(), 2);
    assert!(heap.is_dirty_owner(vec));
    assert_eq!(heap.dirty_write_count(), 3);
    assert_eq!(
        heap.dirty_writes()[2],
        HeapWriteRecord::slot(vec, HeapWriteKind::VectorSlot, 1, TaggedValue::fixnum(99))
    );
}

#[test]
fn bulk_mutation_helpers_record_bulk_write_kinds() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();
    heap.set_write_tracking_mode(super::gc::WriteTrackingMode::OwnersAndRecords);
    super::gc::set_tagged_heap(&mut heap);

    let vec = heap.alloc_vector(vec![TaggedValue::fixnum(10), TaggedValue::fixnum(20)]);
    let _ = super::mutate::with_vector_data_mut(vec, |items| {
        items[1] = TaggedValue::fixnum(99);
    });

    assert_eq!(heap.dirty_owner_count(), 1);
    assert_eq!(heap.dirty_write_count(), 1);
    assert_eq!(
        heap.dirty_writes(),
        &[HeapWriteRecord::bulk(vec, HeapWriteKind::VectorBulk)]
    );
}

#[test]
fn collection_clears_dirty_owner_tracking_at_begin() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();
    heap.set_write_tracking_mode(super::gc::WriteTrackingMode::OwnersAndRecords);
    super::gc::set_tagged_heap(&mut heap);

    let reachable = heap.alloc_vector(vec![TaggedValue::fixnum(10), TaggedValue::fixnum(20)]);
    assert!(super::mutate::set_vector_slot(
        reachable,
        0,
        TaggedValue::fixnum(42),
    ));
    assert_eq!(heap.dirty_owner_count(), 1);
    assert_eq!(heap.dirty_write_count(), 1);

    // Clear-at-BEGIN (the dirty_owners ABA fix): the owner-tracking tables are
    // reset when a cycle STARTS — on the same per-cycle lifecycle as the SATB
    // dedup sets — NOT at end-of-collection. A pre-cycle entry must not survive
    // into this cycle's sweep, where a freed owner's slot can be reused under the
    // same owner address bits (the ABA). So the reset is observable right after
    // `begin_collection`, before the sweep runs.
    heap.begin_collection();
    assert_eq!(
        heap.dirty_owner_count(),
        0,
        "begin_collection must clear dirty-owner tracking"
    );
    assert_eq!(
        heap.dirty_write_count(),
        0,
        "begin_collection must clear the write records too"
    );

    // Completing the cycle leaves them clear (marking traces but writes no heap
    // slots, so the barrier records nothing).
    heap.seed_root(reachable);
    heap.complete_collection();
    assert_eq!(heap.dirty_owner_count(), 0);
    assert_eq!(heap.dirty_write_count(), 0);
}

#[test]
fn value_size_is_one_word() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        std::mem::size_of::<TaggedValue>(),
        std::mem::size_of::<usize>()
    );
    assert_eq!(std::mem::size_of::<TaggedValue>(), 8); // 64-bit
}

#[test]
fn cons_cell_is_two_words() {
    crate::test_utils::init_test_tracing();
    assert_eq!(std::mem::size_of::<ConsCell>(), 16);
}

#[test]
fn value_kind_dispatch() {
    crate::test_utils::init_test_tracing();
    let nil = TaggedValue::NIL;
    assert!(matches!(nil.kind(), ValueKind::Nil));

    let t = TaggedValue::T;
    assert!(matches!(t.kind(), ValueKind::T));

    let n = TaggedValue::fixnum(42);
    assert!(matches!(n.kind(), ValueKind::Fixnum(42)));

    let sym = TaggedValue::from_sym_id(SymId(5));
    assert!(matches!(sym.kind(), ValueKind::Symbol(SymId(5))));

    let ch = TaggedValue::char('x');
    assert!(matches!(ch.kind(), ValueKind::Fixnum(n) if n == 'x' as i64));

    let kw = TaggedValue::from_kw_id(SymId(3));
    assert!(matches!(kw.kind(), ValueKind::Symbol(SymId(3))));
}

#[test]
fn gc_basic_collection() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();

    // Allocate some cons cells
    let _unreachable = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    let reachable = heap.alloc_cons(TaggedValue::fixnum(2), TaggedValue::NIL);

    assert_eq!(heap.allocated_count, 2);

    // Collect with only `reachable` as a root
    heap.collect(std::iter::once(reachable));

    // The unreachable cons should be freed
    assert_eq!(heap.allocated_count, 1);

    // The reachable cons should still be accessible
    assert_eq!(reachable.cons_car().as_fixnum(), Some(2));
}

#[test]
fn gc_transitive_reachability() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();

    // Build a chain: root -> c1 -> c2 -> c3
    let c3 = heap.alloc_cons(TaggedValue::fixnum(3), TaggedValue::NIL);
    let c2 = heap.alloc_cons(TaggedValue::fixnum(2), c3);
    let c1 = heap.alloc_cons(TaggedValue::fixnum(1), c2);

    // Also allocate an unreachable cons
    let _garbage = heap.alloc_cons(TaggedValue::fixnum(999), TaggedValue::NIL);

    assert_eq!(heap.allocated_count, 4);

    // Collect with c1 as root — c2 and c3 should survive transitively
    heap.collect(std::iter::once(c1));

    assert_eq!(heap.allocated_count, 3); // c1, c2, c3 survive; _garbage freed

    // Verify the chain is intact
    assert_eq!(c1.cons_car().as_fixnum(), Some(1));
    assert_eq!(c1.cons_cdr().cons_car().as_fixnum(), Some(2));
    assert_eq!(c1.cons_cdr().cons_cdr().cons_car().as_fixnum(), Some(3));
}

#[test]
fn gc_float_collection() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();

    let f1 = heap.alloc_float(1.0);
    let _f2 = heap.alloc_float(2.0); // unreachable

    assert_eq!(heap.allocated_count, 2);

    heap.collect(std::iter::once(f1));

    assert_eq!(heap.allocated_count, 1);
    assert!((f1.xfloat() - 1.0).abs() < f64::EPSILON);
}

/// The collector is precise: a value held only in a Rust local is not a root.
/// There is no machine-stack scan and no API to configure one
/// (`tagged/CONCURRENT_GC.md`, "precise-rooting precondition").
#[test]
fn gc_collect_exact_does_not_scan_the_machine_stack() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();

    let stack_only = heap.alloc_cons(TaggedValue::fixnum(9), TaggedValue::NIL);
    let keep_visible = [stack_only];
    std::hint::black_box(&keep_visible);

    heap.collect_exact(std::iter::empty());

    assert_eq!(heap.allocated_count, 0);
}

#[test]
fn gc_collect_exact_preserves_roots_across_multiple_cons_blocks() {
    crate::test_utils::init_test_tracing();
    let mut heap = super::gc::TaggedHeap::new();

    let mut first_block_root = TaggedValue::NIL;
    let mut later_block_root = TaggedValue::NIL;
    for i in 0..10_000 {
        let cell = heap.alloc_cons(TaggedValue::fixnum(i), TaggedValue::NIL);
        if i == 10 {
            first_block_root = cell;
        }
        if i == 9_500 {
            later_block_root = cell;
        }
    }

    heap.collect_exact([first_block_root, later_block_root].into_iter());

    assert_eq!(first_block_root.cons_car().as_fixnum(), Some(10));
    assert_eq!(later_block_root.cons_car().as_fixnum(), Some(9_500));
}

#[test]
fn equality_identity() {
    crate::test_utils::init_test_tracing();
    // Same tagged value = equal
    let a = TaggedValue::fixnum(42);
    let b = TaggedValue::fixnum(42);
    assert_eq!(a, b);

    // Different values = not equal
    let c = TaggedValue::fixnum(43);
    assert_ne!(a, c);

    // nil == nil
    assert_eq!(TaggedValue::NIL, TaggedValue::NIL);

    // Symbol identity
    let s1 = TaggedValue::from_sym_id(SymId(5));
    let s2 = TaggedValue::from_sym_id(SymId(5));
    assert_eq!(s1, s2);
}

#[test]
fn fixnum_range_follows_the_target_word_width() {
    crate::test_utils::init_test_tracing();
    let (min_for_32_bits, max_for_32_bits) = super::value::fixnum_bounds_for_word_bits(32);
    assert_eq!(min_for_32_bits, -(1_i64 << 29));
    assert_eq!(max_for_32_bits, (1_i64 << 29) - 1);

    let max = TaggedValue::MOST_POSITIVE_FIXNUM;
    let min = TaggedValue::MOST_NEGATIVE_FIXNUM;
    assert_eq!(
        (min, max),
        super::value::fixnum_bounds_for_word_bits(usize::BITS)
    );

    let v_max = TaggedValue::fixnum(max);
    assert_eq!(v_max.as_fixnum(), Some(max));

    let v_min = TaggedValue::fixnum(min);
    assert_eq!(v_min.as_fixnum(), Some(min));
}

#[test]
fn debug_format() {
    crate::test_utils::init_test_tracing();
    assert_eq!(format!("{:?}", TaggedValue::NIL), "nil");
    assert_eq!(format!("{:?}", TaggedValue::T), "t");
    assert_eq!(format!("{:?}", TaggedValue::fixnum(42)), "42");
    assert_eq!(format!("{:?}", TaggedValue::char('A')), "65");
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
#[ignore = "profiling aid; run explicitly in release with --no-capture"]
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
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/tagged/gc.rs");
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
