//! Fresh names must be reseeded from the live, heap-keyed process index.
//! The suspended shell/input holders must not mask a missing stopped census.
use super::*;
use crate::tagged::gc::mutator_registration::image::PublishedGcWork;
use crate::tagged::gc::{AllStopped, ProofEra as ParallelHeapEra};

#[derive(Default, Debug)]
struct Ledger {
    names: Vec<usize>,
    world: Vec<usize>,
    external: Vec<usize>,
    owners: Vec<usize>,
    journal: Vec<usize>,
    other_work: Vec<String>,
}
impl Ledger {
    fn without_names(&self) -> impl Iterator<Item = &usize> {
        self.world
            .iter()
            .chain(&self.external)
            .chain(&self.owners)
            .chain(&self.journal)
    }
}

fn ledger(era: &ParallelHeapEra<'_>, stopped: &mut AllStopped<'_>) -> Ledger {
    let mut result = Ledger::default();
    let mut names = Vec::new();
    stopped
        .symbols()
        .unwrap()
        .append_name_roots(&mut names)
        .unwrap();
    result.names = names.into_iter().map(Value::bits).collect();
    let mut world = Vec::new();
    era.core
        .keeper
        .as_ref()
        .unwrap()
        .collect_parallel_world_roots(stopped, &mut world)
        .unwrap();
    result.world = world.into_iter().map(Value::bits).collect();
    let mut external = Vec::new();
    era.core
        .legacy_roots
        .as_ref()
        .unwrap()
        .append_words(&mut external);
    era.with_stopped_heap_for_test(stopped, |heap| {
        crate::tagged::transport::collect_shared_root_gc_roots(heap, &mut external);
    })
    .unwrap();
    result.external = external.into_iter().map(Value::bits).collect();
    let keys: Vec<_> = {
        let gate = era.registry().lock().unwrap();
        gate.entries.iter().map(|entry| entry.key).collect()
    };
    for key in keys {
        stopped
            .with_owner_image(key, |image| {
                image.visit_work(|work| match work {
                    PublishedGcWork::Root(word) => result.owners.push(word.value().bits()),
                    other => result.other_work.push(format!("{other:?}")),
                });
            })
            .unwrap();
    }
    let mut journal = Vec::new();
    stopped
        .symbols()
        .unwrap()
        .append_function_roots(&mut journal)
        .unwrap();
    result.journal = journal.into_iter().map(Value::bits).collect();
    result
}

/// No assertion about the observed graph runs while the owner is Native.
/// Complete the stop and resume before checking the facts. This fixture never
/// detaches a cons catalog; keep its collector installed throughout both jobs.
fn round(
    era: &ParallelHeapEra<'_>,
    owner: &mut Mutator<'_>,
    words: [usize; 2],
) -> (Ledger, [bool; 2], Option<Vec<u8>>, u64) {
    // This synchronous keeper fixture needs AllStopped heap access inside
    // the interval; with_native_extent is for non-keeper blocking owners and
    // forbids that access. Keep this explicit pair and suppress unrelated
    // native-return probes while it drives both completed collections.
    let _probes = crate::tagged::gc::mutator_registration::a8_probes::suppress();
    owner.enter_native(&[]).unwrap();
    let mut stopped = match era.stop() {
        Ok(stopped) => stopped,
        Err(error) => {
            owner.leave_native().unwrap();
            panic!("fresh-name stop refused before collection: {error:?}");
        }
    };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let before = ledger(era, &mut stopped);
        let report = era.collect_stopped(&mut stopped);
        let observed = report.as_ref().ok().map(|_| {
            era.with_stopped_heap_for_test(&stopped, |heap| {
                let live = words.map(|word| heap.owns_heap_value_for_test(Value::from_bits(word)));
                // Allocation membership precedes every payload read. A dead
                // word stays an inert integer: never read its string payload.
                let text = live[0]
                    .then(|| {
                        Value::from_bits(words[0])
                            .as_lisp_string()
                            .map(|name| name.as_bytes().to_vec())
                    })
                    .flatten();
                (live, text)
            })
        });
        (before, report, observed)
    }));
    let completed = stopped.complete();
    let returned = owner.leave_native();
    completed.unwrap();
    returned.unwrap();
    let (before, report, observed) =
        outcome.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    let report = report.expect("fresh-name stopped job after normal owner recovery");
    assert!(
        report.major,
        "C5 qualifies full stop-all collection, not minors"
    );
    assert_eq!(report.frozen_owners, 1);
    let (live, text) = observed.unwrap().unwrap();
    eprintln!(
        "P3SYM_C5_NAME_GC_ROUND epoch={} cycle={} roots={} names={} live={live:?}",
        report.epoch,
        report.cycle,
        report.roots,
        before.names.len()
    );
    (before, live, text, report.epoch)
}

fn clear_holders<'era>(run: &mut RunAccess<'_, 'era>, homes: &mut Homes<'era>) {
    for holder in [homes.old_handle, homes.equal_holder] {
        contention::retry(
            Deadline::contention(),
            || run.sym_makunbound_into(&holder, &mut homes.dst),
            contention::global,
        )
        .expect("clear the actual serial input holder");
    }
    // Drop every ordinary name home from the canonical bank. The retained
    // transport roots are only NIL/NIL/23, and this minimal shell has NO
    // captured Context/TLS roots. Homes left in the fixture are stale handles.
    run.clear_root_slots().unwrap();
}

fn sole_name_root(kind: PlainSymbolBacking) {
    fixture_owner(kind, Query::Fresh, |era, owner, homes| {
        let (words, spelling, id) = {
            let mut run = owner.run_access().unwrap();
            let words = [homes.name.word_for_test(&run), homes.equal_name.word_for_test(&run)];
            let spelling = Value::from_bits(words[0]).as_lisp_string().unwrap().as_bytes().to_vec();
            intern_retry(&mut run, &homes.name, &mut homes.dst).unwrap();
            let id = Value::from_bits(homes.dst.word_for_test(&run)).as_bare_symbol_id().unwrap();
            assert_eq!(crate::emacs_core::intern::parallel::name_publication_for_test(era.heap_identity(), id, words[1]), (Some(words[0]), true, false, true));
            clear_holders(&mut run, homes);
            (words, spelling, id)
        };
        let (_, first_live, _, first_epoch) = round(era, owner, words);
        assert!(first_live[0], "fresh winning name survived the birth/history drain");
        let (roots, live, text, epoch) = round(era, owner, words);
        assert!(epoch > first_epoch);
        assert_eq!(live, [true, false], "winner survives solely through registry; equal losing object is reclaimed");
        assert_eq!(text, Some(spelling));
        assert!(roots.other_work.is_empty(), "birth/address/history work cannot mask missing name roots: {roots:?}");
        assert_eq!(roots.names.iter().filter(|&&word| word == words[0]).count(), 1);
        assert!(!roots.names.contains(&words[1]), "losing object must never enter the exact root index");
        assert!(roots.without_names().all(|word| !words.contains(word)), "holder globals, owner homes, frozen shell, transport and function journal must retain neither name: {roots:?}");
        // Counterfactual on the copied REAL ledger only. Never collect a live
        // registered name with its required family deliberately omitted.
        assert!(!roots.without_names().any(|&word| word == words[0]));
        eprintln!("P3SYM_C5_NAME_GC_WITNESS omitted_live_name_index_loses_only_winner {kind:?}");
        assert_eq!(crate::emacs_core::intern::parallel::name_publication_for_test(era.heap_identity(), id, words[1]), (Some(words[0]), true, false, true));
    }).expect("fresh-name GC with independently successful fixture teardown");
}

#[test]
fn p3sym_c5_plain_fresh_name_is_the_only_root_across_two_stops() {
    sole_name_root(PlainSymbolBacking::Unprojected);
}

#[test]
fn p3sym_c5_projected_fresh_name_is_the_only_root_across_two_stops() {
    sole_name_root(PlainSymbolBacking::VoidProjection);
}
