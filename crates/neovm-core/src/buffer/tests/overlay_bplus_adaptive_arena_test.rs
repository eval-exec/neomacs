//! Arena capacity, slot identity, query and reuse regression tests.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TestRecord {
    identity: usize,
    start: EmacsBytePos,
}

impl TestRecord {
    fn new(identity: usize) -> Self {
        Self {
            identity,
            start: EmacsBytePos::new(identity * 2),
        }
    }
}

impl OrderedShiftRecord for TestRecord {
    type Identity = usize;
    type Key = (EmacsBytePos, usize);

    fn identity(self) -> Self::Identity {
        self.identity
    }
    fn key(self) -> Self::Key {
        (self.start, self.identity)
    }
    fn key_position(self) -> EmacsBytePos {
        self.start
    }
    fn end_position(self) -> EmacsBytePos {
        EmacsBytePos::new(self.start.get() + 1)
    }
    fn shifted(self, delta: EmacsByteDelta) -> Self {
        Self {
            start: delta.apply_to_pos(self.start),
            ..self
        }
    }
    fn shifted_key(key: Self::Key, delta: EmacsByteDelta) -> Self::Key {
        (delta.apply_to_pos(key.0), key.1)
    }
}

#[derive(Clone, Copy)]
struct AllRecords;
impl OrderedTreeQuery<TestRecord> for AllRecords {
    fn subtree_may_match(self, _: EmacsBytePos, _: EmacsBytePos, _: EmacsBytePos) -> bool {
        true
    }
    fn record_matches(self, _: TestRecord) -> bool {
        true
    }
}

#[derive(Clone, Copy)]
struct At(EmacsBytePos);
impl OrderedTreeQuery<TestRecord> for At {
    fn subtree_may_match(
        self,
        minimum: EmacsBytePos,
        maximum: EmacsBytePos,
        maximum_end: EmacsBytePos,
    ) -> bool {
        minimum <= self.0 && self.0 <= maximum && self.0 < maximum_end
    }
    fn record_matches(self, record: TestRecord) -> bool {
        record.key_position() <= self.0 && self.0 < record.end_position()
    }
}

fn records(count: usize) -> Vec<TestRecord> {
    (0..count).map(TestRecord::new).collect()
}

fn assert_storage_domain(tree: &OrderedShiftTree<TestRecord>) {
    if tree.next_slot == 0 {
        assert!(tree.pages.is_empty());
        return;
    }
    assert_eq!(tree.pages.len(), tree.next_slot.div_ceil(ARENA_PAGE_NODES));
    assert!((1..=ARENA_PAGE_NODES).contains(&tree.pages[0].len()));
    for page in &tree.pages[1..] {
        assert_eq!(page.len(), ARENA_PAGE_NODES);
    }
    for index in 0..tree.next_slot {
        let page = index / ARENA_PAGE_NODES;
        let offset = index % ARENA_PAGE_NODES;
        assert!(offset < tree.pages[page].len(), "minted ID has no slot");
    }
    let mut index = 0;
    for page in &tree.pages {
        for slot in page.iter() {
            if index >= tree.next_slot {
                assert!(slot.is_none(), "spare slot occupied");
            }
            index += 1;
        }
    }
    assert!(tree.free.iter().all(|id| id.index() < tree.next_slot));
}

fn assert_records_and_queries(tree: &OrderedShiftTree<TestRecord>, count: usize) {
    let expected = records(count);
    assert_eq!(tree.matches(AllRecords).collect::<Vec<_>>(), expected);
    assert_eq!(
        tree.matches_reverse(AllRecords).collect::<Vec<_>>(),
        expected.iter().copied().rev().collect::<Vec<_>>()
    );
    for identity in (0..count).step_by(97) {
        let record = TestRecord::new(identity);
        assert_eq!(
            tree.matches(At(record.start)).collect::<Vec<_>>(),
            vec![record]
        );
        assert!(tree.matches(At(record.end_position())).next().is_none());
    }
    tree.assert_invariants();
    assert_storage_domain(tree);
}

#[test]
fn empty_construction_allocates_no_pages() {
    let incremental = OrderedShiftTree::<TestRecord>::new();
    let bulk = OrderedShiftTree::<TestRecord>::from_records(Vec::new());
    for tree in [&incremental, &bulk] {
        assert!(tree.pages.is_empty());
        assert_eq!(tree.next_slot, 0);
        assert_records_and_queries(tree, 0);
    }
}

#[test]
fn first_incremental_node_reserves_exactly_one_slot() {
    let mut tree = OrderedShiftTree::new();
    assert!(tree.insert(TestRecord::new(0)));
    assert_eq!(tree.next_slot, 1);
    assert_eq!(
        tree.pages.iter().map(|page| page.len()).collect::<Vec<_>>(),
        vec![1]
    );
    assert_records_and_queries(&tree, 1);
}

#[test]
fn bulk_first_leaf_reserves_exactly_one_slot() {
    for count in [1, 2, MAX_ENTRIES] {
        let tree = OrderedShiftTree::from_records(records(count));
        assert_eq!(tree.next_slot, 1);
        assert_eq!(
            tree.pages.iter().map(|page| page.len()).collect::<Vec<_>>(),
            vec![1]
        );
        assert_records_and_queries(&tree, count);
    }
}

#[test]
fn bulk_sizes_first_page_from_all_leaf_and_branch_nodes() {
    for (count, nodes) in [(33, 3), (1024, 33), (1025, 36), (1984, 65)] {
        let tree = OrderedShiftTree::from_records(records(count));
        assert_eq!(tree.next_slot, nodes);
        assert_eq!(tree.pages[0].len(), nodes.min(ARENA_PAGE_NODES));
        assert_eq!(tree.pages.len(), nodes.div_ceil(ARENA_PAGE_NODES));
        for page in &tree.pages[1..] {
            assert_eq!(page.len(), ARENA_PAGE_NODES);
        }
        assert_records_and_queries(&tree, count);
    }
}

#[test]
fn arena_growth_preserves_integer_slot_ids_across_page64_boundary() {
    let mut tree = OrderedShiftTree::new();
    let mut ids = Vec::new();
    // Directly exercise the arena without creating invalid tree topology.
    // The arena-only phase has no root, identity index or query assertions.
    for identity in 0..70 {
        let mut values = SmallVec::new();
        values.push(TestRecord::new(identity));
        let id = tree.allocate(OrderedNode::leaf(values, None));
        assert_eq!(id.index(), identity);
        ids.push(id);
        for (previous, &saved) in ids.iter().enumerate() {
            assert_eq!(
                tree.leaf_records(saved).as_slice(),
                &[TestRecord::new(previous)]
            );
        }
        assert_storage_domain(&tree);
    }
    assert!(tree.pages.len() >= 2);
    assert_eq!(tree.pages[0].len(), ARENA_PAGE_NODES);
    assert_eq!(tree.pages[1].len(), ARENA_PAGE_NODES);
    for id in ids {
        tree.release(id);
    }
    tree.assert_invariants();
}

#[test]
fn incremental_tree_queries_and_leftmost_id_survive_first_page_growth() {
    let mut tree = OrderedShiftTree::new();
    assert!(tree.insert(TestRecord::new(0)));
    let first_leaf = tree.by_identity[&0];
    for identity in 1..3000 {
        assert!(tree.insert(TestRecord::new(identity)));
        // Right-edge splitting retains record0 in its original leftmost leaf.
        assert_eq!(tree.by_identity[&0], first_leaf);
        assert_eq!(tree.leaf_records(first_leaf)[0], TestRecord::new(0));
        if tree.next_slot <= 65 || identity % 97 == 0 {
            assert_records_and_queries(&tree, identity + 1);
        }
    }
    assert!(tree.next_slot > ARENA_PAGE_NODES);
    assert_records_and_queries(&tree, 3000);
}

#[test]
fn growing_partial_bulk_page_preserves_all_records() {
    for initial in [33, 1024] {
        let mut tree = OrderedShiftTree::from_records(records(initial));
        for identity in initial..(initial + 1200) {
            assert!(tree.insert(TestRecord::new(identity)));
            if identity % 97 == 0 {
                assert_records_and_queries(&tree, identity + 1);
            }
        }
        assert_records_and_queries(&tree, initial + 1200);
    }
}

#[test]
fn drain_and_refill_reuses_existing_pages_and_high_water_mark() {
    // Positive control: this retention behavior already exists in baseline A.
    let count = 3000;
    let mut tree = OrderedShiftTree::new();
    for record in records(count) {
        assert!(tree.insert(record));
    }
    let pages: Vec<_> = tree.pages.iter().map(|page| page.as_ptr()).collect();
    let lengths: Vec<_> = tree.pages.iter().map(|page| page.len()).collect();
    let high_water = tree.next_slot;
    for identity in 0..count {
        assert_eq!(tree.remove(identity), Some(TestRecord::new(identity)));
        if identity % 97 == 0 {
            tree.assert_invariants();
        }
    }
    assert!(tree.is_empty());
    tree.assert_invariants();
    assert_eq!(tree.free.len(), high_water);
    assert_eq!(tree.next_slot, high_water);
    for record in records(count) {
        assert!(tree.insert(record));
    }
    assert_eq!(tree.next_slot, high_water);
    assert_eq!(
        tree.pages
            .iter()
            .map(|page| page.as_ptr())
            .collect::<Vec<_>>(),
        pages
    );
    assert_eq!(
        tree.pages.iter().map(|page| page.len()).collect::<Vec<_>>(),
        lengths
    );
    assert_records_and_queries(&tree, count);
}
