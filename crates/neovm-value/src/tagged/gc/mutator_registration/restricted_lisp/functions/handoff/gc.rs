//! Actual stopped root families, not substitutes for the collector census.
use super::*;
use crate::tagged::gc::ProofEra as ParallelHeapEra;
use crate::tagged::gc::mutator_registration::image::PublishedGcWork;

#[derive(Default)]
pub(super) struct Ledger {
    pub(super) world: Vec<usize>,
    pub(super) external: Vec<usize>,
    pub(super) owners: Vec<(u64, Vec<usize>)>,
    pub(super) journal: Vec<usize>,
    pub(super) other_work: usize,
    pub(super) work_details: Vec<String>,
}
impl Ledger {
    pub(super) fn ordinary_has(&self, graph: &[usize]) -> bool {
        self.world
            .iter()
            .chain(&self.external)
            .chain(self.owners.iter().flat_map(|(_, roots)| roots))
            .any(|word| graph.contains(word))
    }
}
pub(super) fn ledger(era: &ParallelHeapEra<'_>, stopped: &mut AllStopped<'_>) -> Ledger {
    let mut result = Ledger::default();
    let mut roots = Vec::new();
    era.core
        .keeper
        .as_ref()
        .unwrap()
        .collect_parallel_world_roots(stopped, &mut roots)
        .unwrap();
    result.world = roots.into_iter().map(TaggedValue::bits).collect();
    let mut roots = Vec::new();
    era.core
        .legacy_roots
        .as_ref()
        .unwrap()
        .append_words(&mut roots);
    era.with_stopped_heap_for_test(stopped, |heap| {
        crate::tagged::transport::collect_shared_root_gc_roots(heap, &mut roots);
    })
    .unwrap();
    result.external = roots.into_iter().map(TaggedValue::bits).collect();
    let keys: Vec<_> = {
        let gate = era.registry().lock().unwrap();
        gate.entries.iter().map(|entry| entry.key).collect()
    };
    for key in keys {
        let mut roots = Vec::new();
        stopped
            .with_owner_image(key, |image| {
                image.visit_work(|work| match work {
                    PublishedGcWork::Root(word) => roots.push(word.value().bits()),
                    work => {
                        result.other_work += 1;
                        result.work_details.push(format!("{work:?}"));
                    }
                });
            })
            .unwrap();
        result.owners.push((key.owner(), roots));
    }
    let mut roots = Vec::new();
    stopped
        .symbols()
        .unwrap()
        .append_function_roots(&mut roots)
        .unwrap();
    result.journal = roots.into_iter().map(TaggedValue::bits).collect();
    result
}
pub(super) fn graph(
    era: &ParallelHeapEra<'_>,
    stopped: &AllStopped<'_>,
    head: usize,
) -> Vec<usize> {
    era.with_stopped_heap_for_test(stopped, |heap| {
        let mut graph = Vec::new();
        let mut value = TaggedValue::from_bits(head);
        // The serially prepared closed lambda spine has exactly three nodes.
        for _ in 0..3 {
            if !value.is_cons() || !heap.owns_heap_value_for_test(value) {
                break;
            }
            graph.push(value.bits());
            value = value.cons_cdr_unobserved();
        }
        graph
    })
    .unwrap()
}
pub(super) fn live(
    era: &ParallelHeapEra<'_>,
    stopped: &AllStopped<'_>,
    graphs: &[Vec<usize>],
) -> Vec<Vec<bool>> {
    era.with_stopped_heap_for_test(stopped, |heap| {
        graphs
            .iter()
            .map(|graph| {
                graph
                    .iter()
                    .map(|&word| heap.owns_heap_value_for_test(TaggedValue::from_bits(word)))
                    .collect()
            })
            .collect()
    })
    .unwrap()
}
