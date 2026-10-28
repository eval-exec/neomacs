use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::{intern, intern_uninterned};
use crate::emacs_core::symbol::ObarraySymbolCellSkipGuard;
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ObjectIdentity(usize);
#[derive(Clone, Copy, Debug)]
enum RootFamily {
    Plain,
    Projection,
    Function,
    Plist,
    Nonmember,
    Localized,
    OwnedForward,
    ParkedKeyboard,
}
#[derive(Clone, Copy, Debug)]
struct ExpectedRoot {
    family: RootFamily,
    identity: ObjectIdentity,
}
impl ExpectedRoot {
    fn new(family: RootFamily, value: TaggedValue) -> Self {
        Self {
            family,
            identity: ObjectIdentity(value.bits()),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootPresence {
    Present,
    Absent,
}
impl RootPresence {
    fn in_words(root: &ExpectedRoot, words: &[TaggedValue]) -> Self {
        if words.contains(&TaggedValue::from_bits(root.identity.0)) {
            Self::Present
        } else {
            Self::Absent
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootLiveness {
    Live,
    Reclaimed,
}
#[derive(Debug)]
struct RootObservation {
    family: RootFamily,
    selected: RootPresence,
    world: RootPresence,
}
struct CollectionReceipt {
    report: CollectionReport,
    census: Vec<RootObservation>,
    liveness: Vec<RootLiveness>,
}
fn finish(era: ParallelHeapEra<'_>) {
    let (collector, keeper) = era.finish().unwrap();
    drop(collector);
    drop(keeper);
}

/// No Context, setup collection, live owner or external graph root exists.
/// Two full jobs remove birth retention as a substitute for current reseeding.
fn root_families(backing: PlainSymbolBacking) {
    let expected = RefCell::new(Vec::<ExpectedRoot>::new());
    Context::with_test_symbol_functions_init_era(
        backing,
        |obarray, ids| {
            let mut roots = expected.borrow_mut();
            let global = TaggedValue::string("C6 current canonical global");
            obarray.seed_value_id(ids[0], global);
            roots.push(ExpectedRoot::new(
                match backing {
                    PlainSymbolBacking::Unprojected => RootFamily::Plain,
                    PlainSymbolBacking::Projected => RootFamily::Projection,
                    PlainSymbolBacking::VoidProjection => {
                        panic!("managed projection fixture must be issued")
                    }
                },
                global,
            ));
            roots.push(ExpectedRoot::new(
                RootFamily::Function,
                obarray.symbol_function_id(ids[0]).unwrap(),
            ));
            let plist = TaggedValue::cons(
                TaggedValue::from_sym_id(intern("c6-root-property")),
                TaggedValue::cons(TaggedValue::string("C6 plist child"), TaggedValue::NIL),
            );
            obarray.set_symbol_plist_id(ids[0], plist);
            roots.push(ExpectedRoot::new(RootFamily::Plist, plist));
            // Nonmember storage is present without deleting any canonical name.
            let nonmember = intern_uninterned("c6-root-nonmember");
            let value = TaggedValue::string("C6 present nonmember");
            obarray.seed_value_id(nonmember, value);
            assert!(!obarray.is_global_member(nonmember));
            roots.push(ExpectedRoot::new(RootFamily::Nonmember, value));
            // The owned descriptor remains a required pool root after localization;
            // neither the symbol redirect nor its BLV cached words name this value.
            let forwarded = intern("c6-root-owned-forwarder");
            let value = TaggedValue::string("C6 owned forwarder only");
            obarray.install_objfwd(forwarded, crate::emacs_core::forward::alloc_objfwd(value));
            roots.push(ExpectedRoot::new(RootFamily::OwnedForward, value));
            let blv = obarray
                .make_symbol_localized(forwarded, TaggedValue::string("C6 local default"))
                .unwrap();
            // SAFETY: sole serial setup owner, fresh BLV retained by this Obarray;
            // no era/backing, collector or overlapping record loan exists yet.
            let blv = unsafe { &*blv.as_ptr() };
            roots.push(ExpectedRoot::new(
                RootFamily::Localized,
                blv.defcell.load_atomic(),
            ));
            assert_eq!(blv.defcell.load_atomic(), blv.valcell.load_atomic());
            assert_eq!(blv.where_buf.load_atomic(), TaggedValue::NIL);
            // Only the parked keyboard owns this managed word. The active
            // slot is NIL, and the expected ledger below retains bits only.
            let keyboard_id = intern("c6-root-parked-keyboard");
            let keyboard = crate::emacs_core::forward::alloc_kboard_objfwd(TaggedValue::NIL);
            keyboard.use_selected_keyboard();
            let parked = TaggedValue::string("C6 parked keyboard only");
            keyboard.select_keyboard(1, parked);
            keyboard.select_keyboard(2, TaggedValue::NIL);
            assert_eq!(keyboard.get(), TaggedValue::NIL);
            obarray.install_kboard_objfwd(keyboard_id, keyboard);
            roots.push(ExpectedRoot::new(RootFamily::ParkedKeyboard, parked));
            // Prove that the existing serial flag really suppresses cells while
            // preserving residual pools. The parallel half must ignore this flag.
            use crate::gc_trace::GcTrace;
            let mut serial_full = Vec::new();
            obarray.trace_roots(&mut serial_full);
            let mut serial_residual = Vec::new();
            {
                let _skip = ObarraySymbolCellSkipGuard::new();
                obarray.trace_roots(&mut serial_residual);
            }
            let mut serial_restored = Vec::new();
            obarray.trace_roots(&mut serial_restored);
            assert_eq!(serial_full, serial_restored);
            eprintln!(
                "P3SYM_C6_KEEPER_WITNESS serial_cell_omission_detected_parallel_scan_must_reseed"
            );
            for root in roots.iter() {
                let value = TaggedValue::from_bits(root.identity.0);
                assert!(serial_full.contains(&value));
                match root.family {
                    RootFamily::Localized
                    | RootFamily::OwnedForward
                    | RootFamily::ParkedKeyboard => {
                        assert!(serial_residual.contains(&value))
                    }
                    RootFamily::Plain
                    | RootFamily::Projection
                    | RootFamily::Function
                    | RootFamily::Plist
                    | RootFamily::Nonmember => assert!(!serial_residual.contains(&value)),
                }
            }
        },
        |era, _| {
            let expected = std::mem::take(&mut *expected.borrow_mut());
            let mut receipts = Vec::new();
            for _ in 0..2 {
                let mut stopped = era.stop().unwrap();
                // Adversarial existing serial policy: the keeper must ignore it.
                // The guard restores the original policy even on an assertion.
                let skip = ObarraySymbolCellSkipGuard::new();
                let keeper = era.keeper.as_ref().unwrap();
                let mut cells = Vec::new();
                let mut projections = Vec::new();
                {
                    let symbols = stopped.symbols().unwrap();
                    symbols
                        .append_current_symbol_roots(keeper, &mut cells)
                        .unwrap();
                    symbols
                        .append_current_projection_roots(keeper, &mut projections)
                        .unwrap();
                }
                let mut all_world = Vec::new();
                keeper
                    .collect_parallel_world_roots(&mut stopped, &mut all_world)
                    .unwrap();
                let census: Vec<_> = expected
                    .iter()
                    .map(|root| {
                        let words = match root.family {
                            RootFamily::Projection => &projections,
                            RootFamily::Plain
                            | RootFamily::Function
                            | RootFamily::Plist
                            | RootFamily::Nonmember
                            | RootFamily::Localized
                            | RootFamily::OwnedForward
                            | RootFamily::ParkedKeyboard => &cells,
                        };
                        RootObservation {
                            family: root.family,
                            selected: RootPresence::in_words(root, words),
                            world: RootPresence::in_words(root, &all_world),
                        }
                    })
                    .collect();
                let report = era.collect_stopped(&mut stopped).unwrap();
                let live = era
                    .with_stopped_heap_for_test(&stopped, |heap| {
                        expected
                            .iter()
                            .map(|root| {
                                if heap.owns_heap_value_for_test(TaggedValue::from_bits(
                                    root.identity.0,
                                )) {
                                    RootLiveness::Live
                                } else {
                                    RootLiveness::Reclaimed
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap();
                drop(skip);
                stopped.complete().unwrap();
                receipts.push(CollectionReceipt {
                    report,
                    census,
                    liveness: live,
                });
            }
            finish(era);
            for CollectionReceipt {
                report,
                census,
                liveness: live,
            } in receipts
            {
                assert!(report.major);
                assert_eq!(report.frozen_owners, 0);
                assert!(
                    census
                        .iter()
                        .all(|row| row.selected == RootPresence::Present
                            && row.world == RootPresence::Present),
                    "C6 actual family census: {census:?}"
                );
                assert!(
                    live.iter().all(|present| *present == RootLiveness::Live),
                    "C6 live family roots: {live:?}"
                );
                eprintln!(
                    "P3SYM_C6_KEEPER_FAMILIES {backing:?} epoch={} major={} families={census:?} live={live:?}",
                    report.epoch, report.major
                );
            }
        },
    );
}
#[test]
fn p3sym_c6_keeper_plain_families_survive_two_full_stops_with_serial_skip() {
    root_families(PlainSymbolBacking::Unprojected);
}
#[test]
fn p3sym_c6_keeper_projected_families_survive_two_full_stops_with_serial_skip() {
    root_families(PlainSymbolBacking::Projected);
}

#[derive(Clone, Copy)]
enum AuthorityOccupancy {
    Reader,
    Writer,
}
#[must_use = "restore the synthetic negative-control authority before finishing"]
struct OccupiedWord<'a>(&'a AtomicU64);
impl<'a> OccupiedWord<'a> {
    fn new(word: &'a AtomicU64, mode: AuthorityOccupancy) -> Self {
        assert!(word::quiescent(word));
        let occupied = match mode {
            AuthorityOccupancy::Reader => 1,
            AuthorityOccupancy::Writer => {
                crate::tagged::gc::mutator_registration::authority_word::WRITER
            }
        };
        word.store(occupied, Ordering::Release);
        Self(word)
    }
}
impl Drop for OccupiedWord<'_> {
    fn drop(&mut self) {
        self.0.store(0, Ordering::Release);
    }
}
#[test]
fn p3sym_c6_keeper_nonzero_word_refuses_before_copy_and_recovers() {
    Context::with_test_symbol_era(|era, _| {
        let mut stopped = era.stop().unwrap();
        let mut observations = Vec::new();
        {
            let symbols = stopped.symbols().unwrap();
            let keeper = era.keeper.as_ref().unwrap();
            for occupancy in [AuthorityOccupancy::Reader, AuthorityOccupancy::Writer] {
                let guard = OccupiedWord::new(&era.registry().symbols.word, occupancy);
                let mut out = vec![TaggedValue::T];
                let cell_result = symbols.append_current_symbol_roots(keeper, &mut out);
                let projection_result = symbols.append_current_projection_roots(keeper, &mut out);
                drop(guard);
                observations.push((cell_result, projection_result, out));
            }
            symbols
                .append_current_symbol_roots(keeper, &mut Vec::new())
                .unwrap();
        }
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        finish(era);
        for (cells, projections, out) in observations {
            assert_eq!(cells, Err(RegistrationError::OutstandingLoan));
            assert_eq!(projections, Err(RegistrationError::OutstandingLoan));
            assert_eq!(out, [TaggedValue::T]);
        }
        eprintln!("P3SYM_C6_KEEPER_REFUSAL nonzero_word_before_copy recovered");
    });
}

#[test]
fn p3sym_c6_keeper_foreign_world_and_backing_refuse_before_copy() {
    Context::with_test_symbol_era(|outer, _| {
        let mut outer_stop = outer.stop().unwrap();
        let outer_symbols = outer_stop.symbols().unwrap();
        let observations = Context::with_test_symbol_era(|inner, _| {
            let mut inner_stop = inner.stop().unwrap();
            let inner_symbols = inner_stop.symbols().unwrap();
            let outer_keeper = outer.keeper.as_ref().unwrap();
            let inner_keeper = inner.keeper.as_ref().unwrap();
            let mut out = vec![TaggedValue::T];
            let cells = outer_symbols.append_current_symbol_roots(inner_keeper, &mut out);
            let projections = inner_symbols.append_current_projection_roots(outer_keeper, &mut out);
            let backing =
                outer_symbols.check_symbol_backing(&inner.registry().symbols.backing, outer_keeper);
            let designators =
                outer_keeper.check_symbol_root_designators(&inner.registry().symbols.backing);
            drop(inner_symbols);
            inner_stop.trace(|_, _| Ok(())).unwrap();
            inner_stop.complete().unwrap();
            finish(inner);
            (cells, projections, backing, designators, out)
        });
        drop(outer_symbols);
        outer_stop.trace(|_, _| Ok(())).unwrap();
        outer_stop.complete().unwrap();
        finish(outer);
        assert_eq!(observations.0, Err(RegistrationError::WrongHeap));
        assert_eq!(observations.1, Err(RegistrationError::WrongHeap));
        assert_eq!(observations.2, Err(RegistrationError::StaleRegistration));
        assert_eq!(observations.3, Err(RegistrationError::StaleRegistration));
        assert_eq!(observations.4, [TaggedValue::T]);
        eprintln!("P3SYM_C6_KEEPER_REFUSAL foreign_world_backing_before_copy");
    });
}

#[test]
fn p3sym_c6_keeper_root_source_guard_test() {
    let world = include_str!("../../../../../emacs_core/runtime/eval/world/parallel_roots.rs");
    let start = world
        .find("pub(crate) fn collect_parallel_world_roots(")
        .unwrap();
    let scanner = &world[start
        ..world[start..]
            .find("/// The full World root surface")
            .unwrap()
            + start];
    assert!(scanner.contains("stopped.symbols()?"));
    assert!(scanner.contains("symbols.append_current_symbol_roots(self, out)?"));
    assert!(scanner.contains("symbols.append_current_projection_roots(self, out)?"));
    assert!(!scanner.contains("world.obarray.get()"));
    assert!(!scanner.contains("projections.trace_roots"));
    for manager in [
        "owned_roots",
        "treesit",
        "watchers",
        "autoloads",
        "frames",
        "coding_systems",
        "buffers",
        "global.processes",
        "global.modes",
    ] {
        assert!(
            scanner.contains(manager),
            "C6 keeper preserves manager {manager}"
        );
    }
    let bridge = include_str!("../../../../../emacs_core/runtime/symbol/parallel/roots.rs");
    let receiver = include_str!("../roots.rs");
    for forbidden in [
        "thread_local!",
        "SEED_SKIP_OBARRAY_SYMBOL_CELLS",
        "ObarraySymbolCellSkipGuard",
        "current_context",
        "with_heap",
        "root::",
        "trace_roots_with",
    ] {
        assert!(
            !bridge.contains(forbidden),
            "C6 bridge must not reach {forbidden}"
        );
        assert!(
            !receiver.contains(forbidden),
            "C6 receiver must not reach {forbidden}"
        );
    }
    assert!(
        bridge
            .find("authority.check_symbol_backing(self, keeper)?")
            .unwrap()
            < bridge.find("self.obarray.as_ref()").unwrap()
    );
    assert!(
        bridge.find("self.check_root_generation(obarray)?").unwrap()
            < bridge.find("obarray.append_symbol_roots(").unwrap()
    );
    assert!(bridge.contains("SymbolRootSet::Complete"));
}
