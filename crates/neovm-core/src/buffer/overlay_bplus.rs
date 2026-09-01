//! Arena-backed high-fanout ordered storage for overlay indexes.
//!
//! This is deliberately a concrete implementation detail rather than a
//! caller-facing abstraction.  Endpoint and interval records share structural
//! balancing, lazy suffix shifts, stable leaf identities, and allocation-free
//! traversal while retaining their own typed keys and query semantics.

use std::fmt::Debug;
use std::hash::Hash;
use std::ops::{BitOr, Deref};

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

#[cfg(test)]
use rustc_hash::FxHashSet;

use super::position::{EmacsByteDelta, EmacsBytePos};

const MAX_ENTRIES: usize = 32;
const MIN_ENTRIES: usize = MAX_ENTRIES / 2;
// `smallvec`'s workspace feature set provides its standard 36-element inline
// array implementation (rather than arbitrary const-generic lengths).  Four
// spare slots also keep split insertion entirely inline.
const INLINE_ENTRIES: usize = 36;
// A non-root branch has at least 16 children and the root has at least two.
// With u32 arena identities, a ten-level tree would require more live nodes
// than can be represented; nine frames therefore cover every constructible
// tree while avoiding a 32-frame zero-fill on every query.
const MAX_TREE_DEPTH: usize = 9;
const ARENA_PAGE_NODES: usize = 64;

const fn minimum_nodes_for_depth(depth: usize) -> u64 {
    if depth <= 1 {
        return 1;
    }
    let mut total = 1_u64;
    let mut level_nodes = 2_u64;
    let mut level = 1;
    while level < depth {
        total += level_nodes;
        level_nodes *= MIN_ENTRIES as u64;
        level += 1;
    }
    total
}

const _: () = assert!(minimum_nodes_for_depth(MAX_TREE_DEPTH) <= u32::MAX as u64);
const _: () = assert!(minimum_nodes_for_depth(MAX_TREE_DEPTH + 1) > u32::MAX as u64);

/// Fixed-size conservative signature used to prune ordered subtrees without
/// coupling the tree to any caller's property vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OrderedFilterMask([u64; 4]);

impl OrderedFilterMask {
    pub(super) const EMPTY: Self = Self([0; 4]);
    pub(super) const ALL: Self = Self([u64::MAX; 4]);

    pub(super) fn with_bit(mut self, bit: usize) -> Self {
        self.0[bit / u64::BITS as usize] |= 1_u64 << (bit % u64::BITS as usize);
        self
    }

    pub(super) fn intersects(self, other: Self) -> bool {
        self.0
            .iter()
            .zip(other.0)
            .any(|(left, right)| left & right != 0)
    }
}

impl BitOr for OrderedFilterMask {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(std::array::from_fn(|index| self.0[index] | rhs.0[index]))
    }
}

pub(super) trait OrderedShiftRecord: Copy + Debug {
    type Identity: Copy + Debug + Eq + Hash;
    type Key: Copy + Debug + Ord;

    fn identity(self) -> Self::Identity;
    fn key(self) -> Self::Key;
    fn key_position(self) -> EmacsBytePos;
    fn end_position(self) -> EmacsBytePos;
    /// Conservative record classes used by callers to prune whole subtrees.
    /// The default keeps existing indexes unfiltered.
    fn filter_mask(self) -> OrderedFilterMask {
        OrderedFilterMask::ALL
    }
    fn shifted(self, delta: EmacsByteDelta) -> Self;
    fn shifted_key(key: Self::Key, delta: EmacsByteDelta) -> Self::Key;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct OrderedNodeId(u32);

impl OrderedNodeId {
    fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect("overlay B+ arena exceeds u32::MAX nodes"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OrderedSummary<K> {
    first_key: K,
    last_key: K,
    min_position: EmacsBytePos,
    max_position: EmacsBytePos,
    max_end: EmacsBytePos,
    filter_mask: OrderedFilterMask,
    count: usize,
    non_overlapping: bool,
}

impl<K: Copy> OrderedSummary<K> {
    fn shifted<R: OrderedShiftRecord<Key = K>>(self, delta: EmacsByteDelta) -> Self {
        #[cfg(test)]
        super::overlay::record_endpoint_search_summary_shift();
        Self {
            first_key: R::shifted_key(self.first_key, delta),
            last_key: R::shifted_key(self.last_key, delta),
            min_position: delta.apply_to_pos(self.min_position),
            max_position: delta.apply_to_pos(self.max_position),
            max_end: delta.apply_to_pos(self.max_end),
            filter_mask: self.filter_mask,
            count: self.count,
            non_overlapping: self.non_overlapping,
        }
    }
}

#[derive(Clone, Debug)]
enum OrderedNodeKind<R: OrderedShiftRecord> {
    Leaf(SmallVec<[R; INLINE_ENTRIES]>),
    Branch(SmallVec<[OrderedNodeId; INLINE_ENTRIES]>),
}

#[derive(Clone, Debug)]
struct OrderedNode<R: OrderedShiftRecord> {
    parent: Option<OrderedNodeId>,
    pending_shift: EmacsByteDelta,
    summary: Option<OrderedSummary<R::Key>>,
    /// Monotone maximum of interval ends through each child/record.
    ///
    /// A high-fanout node cannot prune a point query efficiently from each
    /// child's independent `max_end`: those values are not ordered.  Prefix
    /// maxima turn the lower edge of a query into one binary search while the
    /// ordinary start ordering supplies the upper edge.
    prefix_max_end: SmallVec<[EmacsBytePos; INLINE_ENTRIES]>,
    /// First start position for each branch child, stored beside the branch.
    /// B+ separator searches must stay contiguous instead of dereferencing an
    /// arena node for every binary-search comparison.  Leaves keep this empty.
    branch_min_positions: SmallVec<[EmacsBytePos; INLINE_ENTRIES]>,
    kind: OrderedNodeKind<R>,
}

impl<R: OrderedShiftRecord> OrderedNode<R> {
    fn leaf(records: SmallVec<[R; INLINE_ENTRIES]>, parent: Option<OrderedNodeId>) -> Self {
        Self {
            parent,
            pending_shift: EmacsByteDelta::ZERO,
            summary: None,
            prefix_max_end: SmallVec::new(),
            branch_min_positions: SmallVec::new(),
            kind: OrderedNodeKind::Leaf(records),
        }
    }

    fn branch(
        children: SmallVec<[OrderedNodeId; INLINE_ENTRIES]>,
        parent: Option<OrderedNodeId>,
    ) -> Self {
        Self {
            parent,
            pending_shift: EmacsByteDelta::ZERO,
            summary: None,
            prefix_max_end: SmallVec::new(),
            branch_min_positions: SmallVec::new(),
            kind: OrderedNodeKind::Branch(children),
        }
    }
}

/// A cache-local B+ tree with typed records and stable arena leaf handles.
#[derive(Clone, Debug)]
pub(super) struct OrderedShiftTree<R: OrderedShiftRecord> {
    root: Option<OrderedNodeId>,
    rightmost_leaf: Option<OrderedNodeId>,
    pages: Vec<Box<[Option<OrderedNode<R>>]>>,
    next_slot: usize,
    free: Vec<OrderedNodeId>,
    by_identity: FxHashMap<R::Identity, OrderedNodeId>,
}

impl<R: OrderedShiftRecord> OrderedShiftTree<R> {
    pub(super) fn new() -> Self {
        Self {
            root: None,
            rightmost_leaf: None,
            pages: Vec::new(),
            next_slot: 0,
            free: Vec::new(),
            by_identity: FxHashMap::default(),
        }
    }

    /// Construct occupied leaves and branch levels directly in linear time.
    pub(super) fn from_records(mut records: Vec<R>) -> Self {
        if !records.is_sorted_by_key(|record| record.key()) {
            records.sort_unstable_by_key(|record| record.key());
        }
        let mut tree = Self::new();
        if records.is_empty() {
            return tree;
        }

        let leaf_count = records.len().div_ceil(MAX_ENTRIES);
        let base = records.len() / leaf_count;
        let extra = records.len() % leaf_count;
        let mut leaves = Vec::with_capacity(leaf_count);
        let mut consumed = 0;
        for leaf_index in 0..leaf_count {
            let len = base + usize::from(leaf_index < extra);
            let mut leaf_records = SmallVec::new();
            leaf_records.extend_from_slice(&records[consumed..consumed + len]);
            consumed += len;
            let id = tree.allocate(OrderedNode::leaf(leaf_records, None));
            tree.refresh(id);
            let identities: SmallVec<[_; INLINE_ENTRIES]> = tree
                .leaf_records(id)
                .iter()
                .map(|record| record.identity())
                .collect();
            for identity in identities {
                assert!(tree.by_identity.insert(identity, id).is_none());
            }
            leaves.push(id);
        }

        let mut level = leaves;
        tree.rightmost_leaf = level.last().copied();
        while level.len() > 1 {
            let parent_count = level.len().div_ceil(MAX_ENTRIES);
            let base = level.len() / parent_count;
            let extra = level.len() % parent_count;
            let mut parents = Vec::with_capacity(parent_count);
            let mut consumed = 0;
            for parent_index in 0..parent_count {
                let len = base + usize::from(parent_index < extra);
                let mut children = SmallVec::new();
                children.extend_from_slice(&level[consumed..consumed + len]);
                consumed += len;
                let id = tree.allocate(OrderedNode::branch(children, None));
                let children = tree.branch_children(id).clone();
                for child in children {
                    tree.node_mut(child).parent = Some(id);
                }
                tree.refresh(id);
                parents.push(id);
            }
            level = parents;
        }
        tree.root = level.first().copied();
        tree
    }

    pub(super) fn len(&self) -> usize {
        self.by_identity.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.by_identity.is_empty()
    }

    pub(super) fn contains(&self, identity: R::Identity) -> bool {
        self.by_identity.contains_key(&identity)
    }

    pub(super) fn insert(&mut self, record: R) -> bool {
        if self.by_identity.contains_key(&record.identity()) {
            return false;
        }
        let Some(root) = self.root else {
            let mut records = SmallVec::new();
            records.push(record);
            let root = self.allocate(OrderedNode::leaf(records, None));
            self.refresh(root);
            self.root = Some(root);
            self.rightmost_leaf = Some(root);
            self.by_identity.insert(record.identity(), root);
            return true;
        };

        if record.key() > self.summary(root).last_key {
            self.insert_at_right_edge(record);
            return true;
        }

        let split = self.insert_node(root, record);
        if let Some(right) = split {
            let mut children = SmallVec::new();
            children.push(root);
            children.push(right);
            let new_root = self.allocate(OrderedNode::branch(children, None));
            self.node_mut(root).parent = Some(new_root);
            self.node_mut(right).parent = Some(new_root);
            self.refresh(new_root);
            self.root = Some(new_root);
        }
        true
    }

    /// Insert a new greatest key through the cached right edge.
    ///
    /// Fontification and Dired normally attach overlays in buffer order.  A
    /// B+ tree should make that workload an append plus one scalar summary
    /// update per level, rather than repeating binary searches at every node.
    fn insert_at_right_edge(&mut self, record: R) {
        let leaf = self
            .rightmost_leaf
            .expect("nonempty overlay B+ tree has a rightmost leaf");
        self.normalize_path(leaf);
        let insertion = self.leaf_records(leaf).len();
        self.leaf_records_mut(leaf).push(record);
        self.by_identity.insert(record.identity(), leaf);
        self.refresh_after_insert(leaf, insertion);

        let mut current = leaf;
        loop {
            let split = (self.node_len(current) > MAX_ENTRIES).then(|| self.split(current));
            let Some(parent) = self.node(current).parent else {
                if let Some(right) = split {
                    let mut children = SmallVec::new();
                    children.push(current);
                    children.push(right);
                    let new_root = self.allocate(OrderedNode::branch(children, None));
                    self.node_mut(current).parent = Some(new_root);
                    self.node_mut(right).parent = Some(new_root);
                    self.refresh(new_root);
                    self.root = Some(new_root);
                }
                return;
            };

            let changed = self.branch_children(parent).len() - 1;
            debug_assert_eq!(self.branch_children(parent)[changed], current);
            if let Some(right) = split {
                self.branch_children_mut(parent).push(right);
                self.node_mut(right).parent = Some(parent);
            }
            self.refresh_after_insert(parent, changed);
            current = parent;
        }
    }

    fn insert_node(&mut self, id: OrderedNodeId, record: R) -> Option<OrderedNodeId> {
        self.push(id);
        match &self.node(id).kind {
            OrderedNodeKind::Leaf(_) => {
                let insertion = self
                    .leaf_records(id)
                    .binary_search_by_key(&record.key(), |candidate| candidate.key())
                    .unwrap_err();
                self.leaf_records_mut(id).insert(insertion, record);
                self.by_identity.insert(record.identity(), id);
                self.refresh_after_insert(id, insertion);
            }
            OrderedNodeKind::Branch(_) => {
                let child_index = self.child_index_for_key(id, record.key());
                let child = self.branch_children(id)[child_index];
                if let Some(right) = self.insert_node(child, record) {
                    self.branch_children_mut(id).insert(child_index + 1, right);
                    self.node_mut(right).parent = Some(id);
                }
                self.refresh_after_insert(id, child_index);
            }
        }
        (self.node_len(id) > MAX_ENTRIES).then(|| self.split(id))
    }

    fn split(&mut self, id: OrderedNodeId) -> OrderedNodeId {
        let parent = self.node(id).parent;
        let right_kind = match &mut self.node_mut(id).kind {
            OrderedNodeKind::Leaf(records) => {
                let middle = records.len() / 2;
                OrderedNodeKind::Leaf(records.drain(middle..).collect())
            }
            OrderedNodeKind::Branch(children) => {
                let middle = children.len() / 2;
                OrderedNodeKind::Branch(children.drain(middle..).collect())
            }
        };
        let right = self.allocate(OrderedNode {
            parent,
            pending_shift: EmacsByteDelta::ZERO,
            summary: None,
            prefix_max_end: SmallVec::new(),
            branch_min_positions: SmallVec::new(),
            kind: right_kind,
        });
        match &self.node(right).kind {
            OrderedNodeKind::Leaf(records) => {
                let identities: SmallVec<[_; INLINE_ENTRIES]> =
                    records.iter().map(|record| record.identity()).collect();
                for identity in identities {
                    self.by_identity.insert(identity, right);
                }
            }
            OrderedNodeKind::Branch(children) => {
                let children = children.clone();
                for child in children {
                    self.node_mut(child).parent = Some(right);
                }
            }
        }
        self.refresh(id);
        self.refresh(right);
        if self.rightmost_leaf == Some(id)
            && matches!(self.node(right).kind, OrderedNodeKind::Leaf(_))
        {
            self.rightmost_leaf = Some(right);
        }
        right
    }

    pub(super) fn remove(&mut self, identity: R::Identity) -> Option<R> {
        let leaf = *self.by_identity.get(&identity)?;
        self.normalize_path(leaf);
        let index = self
            .leaf_records(leaf)
            .iter()
            .position(|record| record.identity() == identity)
            .expect("overlay B+ identity map pointed to the wrong leaf");
        let removed = self.leaf_records_mut(leaf).remove(index);
        self.by_identity.remove(&identity);
        self.refresh(leaf);
        self.rebalance_after_remove(leaf);
        Some(removed)
    }

    /// Replace a record whose ordered key is unchanged.
    ///
    /// This is the mutation analogue of GNU itree's end-only region update:
    /// interval augmentation changes, but membership and equal-start order do
    /// not.  Requiring the complete replacement record lets the record and key
    /// types enforce identity/key preservation at this single seam.
    pub(super) fn replace_same_key(&mut self, replacement: R) -> Option<R> {
        let identity = replacement.identity();
        let leaf = *self.by_identity.get(&identity)?;
        self.normalize_path(leaf);
        let index = self
            .leaf_records(leaf)
            .iter()
            .position(|record| record.identity() == identity)
            .expect("overlay B+ identity map pointed to the wrong leaf");
        let previous = self.leaf_records(leaf)[index];
        assert_eq!(
            previous.key(),
            replacement.key(),
            "same-key replacement changed ordered position"
        );
        self.leaf_records_mut(leaf)[index] = replacement;
        self.refresh(leaf);
        self.refresh_ancestors(self.node(leaf).parent);
        Some(previous)
    }

    pub(super) fn record(&self, identity: R::Identity) -> Option<R> {
        let leaf = *self.by_identity.get(&identity)?;
        let record = self
            .leaf_records(leaf)
            .iter()
            .find(|record| record.identity() == identity)
            .copied()?;
        let mut delta = EmacsByteDelta::ZERO;
        let mut current = Some(leaf);
        while let Some(id) = current {
            let node = self.node(id);
            delta = delta.combine(node.pending_shift);
            current = node.parent;
        }
        Some(record.shifted(delta))
    }

    pub(super) fn shift_at_or_after(
        &mut self,
        position: EmacsBytePos,
        inclusive: bool,
        delta: EmacsByteDelta,
    ) {
        if delta.is_zero() {
            return;
        }
        if let Some(root) = self.root {
            self.shift_node(root, position, inclusive, delta);
        }
    }

    fn shift_node(
        &mut self,
        id: OrderedNodeId,
        position: EmacsBytePos,
        inclusive: bool,
        delta: EmacsByteDelta,
    ) {
        #[cfg(test)]
        super::overlay::record_overlay_shift_node_visit();
        let summary = self.summary(id);
        let wholly_affected =
            summary.min_position > position || (inclusive && summary.min_position == position);
        if wholly_affected {
            self.apply_shift(id, delta);
            return;
        }
        let wholly_before =
            summary.max_position < position || (!inclusive && summary.max_position == position);
        if wholly_before {
            return;
        }

        self.push(id);
        match &self.node(id).kind {
            OrderedNodeKind::Leaf(_) => {
                for record in self.leaf_records_mut(id) {
                    let affected = record.key_position() > position
                        || (inclusive && record.key_position() == position);
                    if affected {
                        *record = record.shifted(delta);
                    }
                }
            }
            OrderedNodeKind::Branch(_) => {
                let children = self.branch_children(id).clone();
                for child in children {
                    self.shift_node(child, position, inclusive, delta);
                }
            }
        }
        self.refresh(id);
    }

    pub(super) fn matches<Q: OrderedTreeQuery<R>>(
        &self,
        query: Q,
    ) -> OrderedTreeMatches<&Self, R, Q> {
        OrderedTreeMatches::new(self, query)
    }

    pub(super) fn matches_reverse<Q: OrderedTreeQuery<R>>(
        &self,
        query: Q,
    ) -> OrderedTreeMatches<&Self, R, Q> {
        OrderedTreeMatches::new_reverse(self, query)
    }

    #[inline(always)]
    pub(super) fn matches_owned<T, Q>(tree: T, query: Q) -> OrderedTreeMatches<T, R, Q>
    where
        T: Deref<Target = Self>,
        Q: OrderedTreeQuery<R>,
    {
        OrderedTreeMatches::new(tree, query)
    }

    /// Visit every matching record without constructing resumable iterator
    /// state.  Materialized Lisp queries consume the complete result in one
    /// call, so a bounded recursive walk avoids copying the fixed frame stack
    /// while streaming layout consumers retain `matches_owned`.
    #[inline(always)]
    pub(super) fn for_each_match<Q, F>(&self, query: Q, mut visit: F)
    where
        Q: OrderedTreeQuery<R>,
        F: FnMut(R),
    {
        if let Some(root) = self.root {
            if query.can_use_non_overlapping_single_path() && self.summary(root).non_overlapping {
                if let Some(record) = self.match_in_non_overlapping_tree(root, query) {
                    visit(record);
                }
                return;
            }
            self.for_each_match_in_node(root, EmacsByteDelta::ZERO, query, &mut visit);
        }
    }

    fn match_in_non_overlapping_tree<Q>(&self, mut id: OrderedNodeId, query: Q) -> Option<R>
    where
        Q: OrderedTreeQuery<R>,
    {
        let mut inherited = EmacsByteDelta::ZERO;
        loop {
            #[cfg(test)]
            query.record_subtree_visit();
            let node = self.node(id);
            let record_delta = inherited.combine(node.pending_shift);
            match &node.kind {
                OrderedNodeKind::Leaf(records) => {
                    let past_candidate = records.partition_point(|record| {
                        !query.minimum_start_is_too_large(
                            record_delta.apply_to_pos(record.key_position()),
                        )
                    });
                    let record = *records.get(past_candidate.checked_sub(1)?)?;
                    let record = if record_delta.is_zero() {
                        record
                    } else {
                        record.shifted(record_delta)
                    };
                    return query.record_matches(record).then_some(record);
                }
                OrderedNodeKind::Branch(children) => {
                    let past_candidate = node.branch_min_positions.partition_point(|minimum| {
                        !query.minimum_start_is_too_large(record_delta.apply_to_pos(*minimum))
                    });
                    id = *children.get(past_candidate.checked_sub(1)?)?;
                    inherited = record_delta;
                }
            }
        }
    }

    fn for_each_match_in_node<Q, F>(
        &self,
        id: OrderedNodeId,
        inherited: EmacsByteDelta,
        query: Q,
        visit: &mut F,
    ) where
        Q: OrderedTreeQuery<R>,
        F: FnMut(R),
    {
        #[cfg(test)]
        query.record_subtree_visit();
        let summary = self.summary(id);
        if !query.subtree_may_match(
            inherited.apply_to_pos(summary.min_position),
            inherited.apply_to_pos(summary.max_position),
            inherited.apply_to_pos(summary.max_end),
        ) || !query.subtree_filter_may_match(summary.filter_mask)
        {
            return;
        }

        let node = self.node(id);
        let record_delta = inherited.combine(node.pending_shift);
        let cursor = node.prefix_max_end.partition_point(|maximum_end| {
            query.maximum_end_is_too_small(inherited.apply_to_pos(*maximum_end))
        });
        match &node.kind {
            OrderedNodeKind::Leaf(records) => {
                let end = records.partition_point(|record| {
                    !query.minimum_start_is_too_large(
                        record_delta.apply_to_pos(record.key_position()),
                    )
                });
                for record in &records[cursor.min(end)..end] {
                    let record = if record_delta.is_zero() {
                        *record
                    } else {
                        record.shifted(record_delta)
                    };
                    if query.record_matches(record) {
                        visit(record);
                    }
                }
            }
            OrderedNodeKind::Branch(children) => {
                let end = node.branch_min_positions.partition_point(|minimum| {
                    !query.minimum_start_is_too_large(record_delta.apply_to_pos(*minimum))
                });
                for child in &children[cursor.min(end)..end] {
                    self.for_each_match_in_node(*child, record_delta, query, visit);
                }
            }
        }
    }

    pub(super) fn next_position_after(
        &self,
        position: EmacsBytePos,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        let root = self.root?;
        self.next_in_node(root, position, limit, EmacsByteDelta::ZERO)
    }

    fn next_in_node(
        &self,
        mut id: OrderedNodeId,
        position: EmacsBytePos,
        limit: EmacsBytePos,
        mut inherited: EmacsByteDelta,
    ) -> Option<EmacsBytePos> {
        loop {
            #[cfg(test)]
            super::overlay::record_endpoint_search_node_visit();
            let node = self.node(id);
            let record_delta = inherited.combine(node.pending_shift);
            match &node.kind {
                OrderedNodeKind::Leaf(records) => {
                    let candidate_index = records.partition_point(|record| {
                        record_delta.apply_to_pos(record.key_position()) <= position
                    });
                    return records.get(candidate_index).and_then(|record| {
                        let candidate = record_delta.apply_to_pos(record.key_position());
                        (candidate <= limit).then_some(candidate)
                    });
                }
                OrderedNodeKind::Branch(children) => {
                    // The prefix maxima are contiguous branch separators.  A
                    // full child-summary lookup here defeats the cache-local
                    // B+ shape and shifts fields the endpoint search does not
                    // need.  This node's pending shift is already reflected
                    // in the prefix cache, so only the inherited delta applies.
                    let first_candidate = node
                        .prefix_max_end
                        .partition_point(|maximum| inherited.apply_to_pos(*maximum) <= position);
                    let child = *children.get(first_candidate)?;
                    id = child;
                    inherited = record_delta;
                }
            }
        }
    }

    pub(super) fn previous_position_before(
        &self,
        position: EmacsBytePos,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        let root = self.root?;
        self.previous_in_node(root, position, limit, EmacsByteDelta::ZERO)
    }

    fn previous_in_node(
        &self,
        mut id: OrderedNodeId,
        position: EmacsBytePos,
        limit: EmacsBytePos,
        mut inherited: EmacsByteDelta,
    ) -> Option<EmacsBytePos> {
        loop {
            #[cfg(test)]
            super::overlay::record_endpoint_search_node_visit();
            let node = self.node(id);
            let record_delta = inherited.combine(node.pending_shift);
            match &node.kind {
                OrderedNodeKind::Leaf(records) => {
                    let past_last_candidate = records.partition_point(|record| {
                        record_delta.apply_to_pos(record.key_position()) < position
                    });
                    return past_last_candidate.checked_sub(1).and_then(|index| {
                        let candidate = record_delta.apply_to_pos(records[index].key_position());
                        (candidate >= limit).then_some(candidate)
                    });
                }
                OrderedNodeKind::Branch(children) => {
                    let past_last_candidate = children.partition_point(|child| {
                        record_delta.apply_to_pos(self.summary(*child).min_position) < position
                    });
                    let child = *children.get(past_last_candidate.checked_sub(1)?)?;
                    if record_delta.apply_to_pos(self.summary(child).max_position) < limit {
                        return None;
                    }
                    id = child;
                    inherited = record_delta;
                }
            }
        }
    }

    fn normalize_path(&mut self, id: OrderedNodeId) {
        let mut path = SmallVec::<[OrderedNodeId; 16]>::new();
        let mut current = Some(id);
        while let Some(node) = current {
            path.push(node);
            current = self.node(node).parent;
        }
        for node in path.into_iter().rev() {
            self.push(node);
        }
    }

    fn rebalance_after_remove(&mut self, mut id: OrderedNodeId) {
        loop {
            if Some(id) == self.root {
                self.finish_root_removal(id);
                return;
            }
            if self.node_len(id) >= MIN_ENTRIES {
                self.refresh(id);
                self.refresh_ancestors(self.node(id).parent);
                return;
            }

            let parent = self.node(id).parent.expect("non-root B+ node has a parent");
            self.push(parent);
            self.push(id);
            let child_index = self
                .branch_children(parent)
                .iter()
                .position(|child| *child == id)
                .expect("B+ parent lost its child");
            let left = child_index
                .checked_sub(1)
                .map(|index| self.branch_children(parent)[index]);
            let right = self.branch_children(parent).get(child_index + 1).copied();

            if let Some(left) = left.filter(|left| self.node_len(*left) > MIN_ENTRIES) {
                self.push(left);
                self.borrow_from_left(left, id);
                self.refresh(left);
                self.refresh(id);
                self.refresh_ancestors(Some(parent));
                return;
            }
            if let Some(right) = right.filter(|right| self.node_len(*right) > MIN_ENTRIES) {
                self.push(right);
                self.borrow_from_right(id, right);
                self.refresh(id);
                self.refresh(right);
                self.refresh_ancestors(Some(parent));
                return;
            }

            if let Some(left) = left {
                self.push(left);
                self.merge_nodes(left, id);
                if self.rightmost_leaf == Some(id) {
                    self.rightmost_leaf = Some(left);
                }
                self.branch_children_mut(parent).remove(child_index);
                self.release(id);
                self.refresh(left);
            } else if let Some(right) = right {
                self.push(right);
                self.merge_nodes(id, right);
                if self.rightmost_leaf == Some(right) {
                    self.rightmost_leaf = Some(id);
                }
                self.branch_children_mut(parent).remove(child_index + 1);
                self.release(right);
                self.refresh(id);
            }
            self.refresh(parent);
            id = parent;
        }
    }

    fn finish_root_removal(&mut self, root: OrderedNodeId) {
        self.push(root);
        match &self.node(root).kind {
            OrderedNodeKind::Leaf(records) if records.is_empty() => {
                self.release(root);
                self.root = None;
                self.rightmost_leaf = None;
            }
            OrderedNodeKind::Branch(children) if children.len() == 1 => {
                let child = children[0];
                self.node_mut(child).parent = None;
                self.release(root);
                self.root = Some(child);
            }
            _ => self.refresh(root),
        }
    }

    fn borrow_from_left(&mut self, left: OrderedNodeId, destination: OrderedNodeId) {
        match (&self.node(left).kind, &self.node(destination).kind) {
            (OrderedNodeKind::Leaf(_), OrderedNodeKind::Leaf(_)) => {
                let record = self.leaf_records_mut(left).pop().expect("nonempty leaf");
                self.leaf_records_mut(destination).insert(0, record);
                self.by_identity.insert(record.identity(), destination);
            }
            (OrderedNodeKind::Branch(_), OrderedNodeKind::Branch(_)) => {
                let child = self
                    .branch_children_mut(left)
                    .pop()
                    .expect("nonempty branch");
                self.branch_children_mut(destination).insert(0, child);
                self.node_mut(child).parent = Some(destination);
            }
            _ => unreachable!("B+ siblings at one level have the same node kind"),
        }
    }

    fn borrow_from_right(&mut self, destination: OrderedNodeId, right: OrderedNodeId) {
        match (&self.node(destination).kind, &self.node(right).kind) {
            (OrderedNodeKind::Leaf(_), OrderedNodeKind::Leaf(_)) => {
                let record = self.leaf_records_mut(right).remove(0);
                self.leaf_records_mut(destination).push(record);
                self.by_identity.insert(record.identity(), destination);
            }
            (OrderedNodeKind::Branch(_), OrderedNodeKind::Branch(_)) => {
                let child = self.branch_children_mut(right).remove(0);
                self.branch_children_mut(destination).push(child);
                self.node_mut(child).parent = Some(destination);
            }
            _ => unreachable!("B+ siblings at one level have the same node kind"),
        }
    }

    fn merge_nodes(&mut self, destination: OrderedNodeId, source: OrderedNodeId) {
        match (&self.node(destination).kind, &self.node(source).kind) {
            (OrderedNodeKind::Leaf(_), OrderedNodeKind::Leaf(records)) => {
                let records = records.clone();
                for record in &records {
                    self.by_identity.insert(record.identity(), destination);
                }
                self.leaf_records_mut(destination).extend(records);
            }
            (OrderedNodeKind::Branch(_), OrderedNodeKind::Branch(children)) => {
                let children = children.clone();
                for child in &children {
                    self.node_mut(*child).parent = Some(destination);
                }
                self.branch_children_mut(destination).extend(children);
            }
            _ => unreachable!("B+ siblings at one level have the same node kind"),
        }
    }

    fn refresh_ancestors(&mut self, mut current: Option<OrderedNodeId>) {
        while let Some(id) = current {
            self.refresh(id);
            current = self.node(id).parent;
        }
    }

    fn child_index_for_key(&self, id: OrderedNodeId, key: R::Key) -> usize {
        let children = self.branch_children(id);
        children
            .partition_point(|child| self.summary(*child).last_key < key)
            .min(children.len() - 1)
    }

    fn apply_shift(&mut self, id: OrderedNodeId, delta: EmacsByteDelta) {
        let node = self.node_mut(id);
        node.summary = node.summary.map(|summary| summary.shifted::<R>(delta));
        for maximum_end in &mut node.prefix_max_end {
            *maximum_end = delta.apply_to_pos(*maximum_end);
        }
        node.pending_shift = node.pending_shift.combine(delta);
    }

    fn push(&mut self, id: OrderedNodeId) {
        let shift = self.node(id).pending_shift;
        if shift.is_zero() {
            return;
        }
        match &self.node(id).kind {
            OrderedNodeKind::Leaf(_) => {
                for record in self.leaf_records_mut(id) {
                    *record = record.shifted(shift);
                }
            }
            OrderedNodeKind::Branch(children) => {
                let children = children.clone();
                for child in children {
                    self.apply_shift(child, shift);
                }
                for minimum in &mut self.node_mut(id).branch_min_positions {
                    *minimum = shift.apply_to_pos(*minimum);
                }
            }
        }
        self.node_mut(id).pending_shift = EmacsByteDelta::ZERO;
    }

    fn refresh(&mut self, id: OrderedNodeId) {
        debug_assert!(self.node(id).pending_shift.is_zero());
        let (summary, prefix_max_end, branch_min_positions) = match &self.node(id).kind {
            OrderedNodeKind::Leaf(records) => {
                let mut prefix = SmallVec::with_capacity(records.len());
                let mut maximum: Option<EmacsBytePos> = None;
                let mut non_overlapping = true;
                for record in records {
                    if maximum.is_some_and(|previous_end| previous_end > record.key_position()) {
                        non_overlapping = false;
                    }
                    maximum = Some(maximum.map_or(record.end_position(), |current| {
                        current.max(record.end_position())
                    }));
                    prefix.push(maximum.expect("record just established a maximum"));
                }
                let summary = records.first().map(|first| OrderedSummary {
                    first_key: first.key(),
                    last_key: records.last().expect("first record existed").key(),
                    min_position: first.key_position(),
                    max_position: records.last().expect("first record existed").key_position(),
                    max_end: *prefix.last().expect("first record existed"),
                    filter_mask: records
                        .iter()
                        .fold(OrderedFilterMask::EMPTY, |mask, record| {
                            mask | record.filter_mask()
                        }),
                    count: records.len(),
                    non_overlapping,
                });
                (summary, prefix, SmallVec::new())
            }
            OrderedNodeKind::Branch(children) => {
                let mut prefix = SmallVec::with_capacity(children.len());
                let mut minimums = SmallVec::with_capacity(children.len());
                let mut maximum: Option<EmacsBytePos> = None;
                let mut non_overlapping = true;
                for child in children {
                    let child_summary = self.summary(*child);
                    if !child_summary.non_overlapping
                        || maximum
                            .is_some_and(|previous_end| previous_end > child_summary.min_position)
                    {
                        non_overlapping = false;
                    }
                    minimums.push(child_summary.min_position);
                    let child_end = child_summary.max_end;
                    maximum = Some(maximum.map_or(child_end, |current| current.max(child_end)));
                    prefix.push(maximum.expect("child just established a maximum"));
                }
                let summary = children.first().map(|first| OrderedSummary {
                    first_key: self.summary(*first).first_key,
                    last_key: self
                        .summary(*children.last().expect("first child existed"))
                        .last_key,
                    min_position: self.summary(*first).min_position,
                    max_position: self
                        .summary(*children.last().expect("first child existed"))
                        .max_position,
                    max_end: *prefix.last().expect("first child existed"),
                    filter_mask: children
                        .iter()
                        .fold(OrderedFilterMask::EMPTY, |mask, child| {
                            mask | self.summary(*child).filter_mask
                        }),
                    count: children
                        .iter()
                        .map(|child| self.summary(*child).count)
                        .sum(),
                    non_overlapping,
                });
                (summary, prefix, minimums)
            }
        };
        let node = self.node_mut(id);
        node.summary = summary;
        node.prefix_max_end = prefix_max_end;
        node.branch_min_positions = branch_min_positions;
    }

    /// Update the summary after one record was inserted below `changed`.
    ///
    /// Recomputing all entries in every ancestor turns high fanout into extra
    /// work.  Prefix maxima before the changed slot remain valid, so the
    /// monotonic append workload used by Dired/fontification touches one slot
    /// per level except when a node actually splits.
    fn refresh_after_insert(&mut self, id: OrderedNodeId, changed: usize) {
        debug_assert!(self.node(id).pending_shift.is_zero());
        if matches!(self.node(id).kind, OrderedNodeKind::Leaf(_)) {
            let node = self.node_mut(id);
            let old_summary = node.summary.expect("live B+ leaf has a summary");
            let OrderedNodeKind::Leaf(records) = &node.kind else {
                unreachable!("node kind was checked above")
            };
            node.prefix_max_end.truncate(changed);
            let mut maximum = node.prefix_max_end.last().copied();
            let mut non_overlapping = old_summary.non_overlapping;
            for record in &records[changed..] {
                if maximum.is_some_and(|previous_end| previous_end > record.key_position()) {
                    non_overlapping = false;
                }
                let end = record.end_position();
                maximum = Some(maximum.map_or(end, |current| current.max(end)));
                node.prefix_max_end
                    .push(maximum.expect("record just established a maximum"));
            }
            let first = records.first().expect("inserted B+ leaf is nonempty");
            let last = records.last().expect("inserted B+ leaf is nonempty");
            node.summary = Some(OrderedSummary {
                first_key: first.key(),
                last_key: last.key(),
                min_position: first.key_position(),
                max_position: last.key_position(),
                max_end: *node
                    .prefix_max_end
                    .last()
                    .expect("inserted B+ leaf is nonempty"),
                filter_mask: old_summary.filter_mask | records[changed].filter_mask(),
                count: old_summary.count + 1,
                non_overlapping,
            });
            return;
        }

        // Branch summaries live in separate arena slots, so copy only the
        // changed suffix's scalar maxima before mutably borrowing this node.
        let old_summary = self.summary(id);
        let children = self.branch_children(id);
        let first = self.summary(*children.first().expect("B+ branch is nonempty"));
        let last = self.summary(*children.last().expect("B+ branch is nonempty"));
        let changed_summaries = children[changed..]
            .iter()
            .map(|child| self.summary(*child))
            .collect::<SmallVec<[_; INLINE_ENTRIES]>>();
        let changed_filter_mask = changed_summaries
            .iter()
            .fold(OrderedFilterMask::EMPTY, |mask, summary| {
                mask | summary.filter_mask
            });
        let node = self.node_mut(id);
        node.prefix_max_end.truncate(changed);
        node.branch_min_positions.truncate(changed);
        let mut maximum = node.prefix_max_end.last().copied();
        let mut non_overlapping = old_summary.non_overlapping;
        for summary in changed_summaries {
            if !summary.non_overlapping
                || maximum.is_some_and(|previous_end| previous_end > summary.min_position)
            {
                non_overlapping = false;
            }
            node.branch_min_positions.push(summary.min_position);
            let end = summary.max_end;
            maximum = Some(maximum.map_or(end, |current| current.max(end)));
            node.prefix_max_end
                .push(maximum.expect("child just established a maximum"));
        }
        node.summary = Some(OrderedSummary {
            first_key: first.first_key,
            last_key: last.last_key,
            min_position: first.min_position,
            max_position: last.max_position,
            max_end: *node
                .prefix_max_end
                .last()
                .expect("inserted B+ branch is nonempty"),
            filter_mask: old_summary.filter_mask | changed_filter_mask,
            count: old_summary.count + 1,
            non_overlapping,
        });
    }

    #[inline(always)]
    fn summary(&self, id: OrderedNodeId) -> OrderedSummary<R::Key> {
        self.node(id)
            .summary
            .expect("nonempty overlay B+ node has a summary")
    }

    fn node_len(&self, id: OrderedNodeId) -> usize {
        match &self.node(id).kind {
            OrderedNodeKind::Leaf(records) => records.len(),
            OrderedNodeKind::Branch(children) => children.len(),
        }
    }

    fn leaf_records(&self, id: OrderedNodeId) -> &SmallVec<[R; INLINE_ENTRIES]> {
        match &self.node(id).kind {
            OrderedNodeKind::Leaf(records) => records,
            OrderedNodeKind::Branch(_) => panic!("overlay B+ node is not a leaf"),
        }
    }

    fn leaf_records_mut(&mut self, id: OrderedNodeId) -> &mut SmallVec<[R; INLINE_ENTRIES]> {
        match &mut self.node_mut(id).kind {
            OrderedNodeKind::Leaf(records) => records,
            OrderedNodeKind::Branch(_) => panic!("overlay B+ node is not a leaf"),
        }
    }

    fn branch_children(&self, id: OrderedNodeId) -> &SmallVec<[OrderedNodeId; INLINE_ENTRIES]> {
        match &self.node(id).kind {
            OrderedNodeKind::Branch(children) => children,
            OrderedNodeKind::Leaf(_) => panic!("overlay B+ node is not a branch"),
        }
    }

    fn branch_children_mut(
        &mut self,
        id: OrderedNodeId,
    ) -> &mut SmallVec<[OrderedNodeId; INLINE_ENTRIES]> {
        match &mut self.node_mut(id).kind {
            OrderedNodeKind::Branch(children) => children,
            OrderedNodeKind::Leaf(_) => panic!("overlay B+ node is not a branch"),
        }
    }

    fn allocate(&mut self, node: OrderedNode<R>) -> OrderedNodeId {
        if let Some(id) = self.free.pop() {
            debug_assert!(self.slot(id).is_none());
            *self.slot_mut(id) = Some(node);
            id
        } else {
            let id = OrderedNodeId::from_index(self.next_slot);
            if self.next_slot.is_multiple_of(ARENA_PAGE_NODES) {
                self.pages.push(
                    std::iter::repeat_with(|| None)
                        .take(ARENA_PAGE_NODES)
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                );
            }
            self.next_slot += 1;
            *self.slot_mut(id) = Some(node);
            id
        }
    }

    fn release(&mut self, id: OrderedNodeId) {
        assert!(self.slot_mut(id).take().is_some());
        self.free.push(id);
    }

    fn slot(&self, id: OrderedNodeId) -> &Option<OrderedNode<R>> {
        let index = id.index();
        &self.pages[index / ARENA_PAGE_NODES][index % ARENA_PAGE_NODES]
    }

    fn slot_mut(&mut self, id: OrderedNodeId) -> &mut Option<OrderedNode<R>> {
        let index = id.index();
        &mut self.pages[index / ARENA_PAGE_NODES][index % ARENA_PAGE_NODES]
    }

    #[inline(always)]
    fn node(&self, id: OrderedNodeId) -> &OrderedNode<R> {
        self.slot(id)
            .as_ref()
            .expect("overlay B+ node handle points to a free slot")
    }

    fn node_mut(&mut self, id: OrderedNodeId) -> &mut OrderedNode<R> {
        self.slot_mut(id)
            .as_mut()
            .expect("overlay B+ node handle points to a free slot")
    }

    #[cfg(test)]
    pub(super) fn height(&self) -> usize {
        let mut height = 0;
        let mut current = self.root;
        while let Some(id) = current {
            height += 1;
            current = match &self.node(id).kind {
                OrderedNodeKind::Leaf(_) => None,
                OrderedNodeKind::Branch(children) => children.first().copied(),
            };
        }
        height
    }

    #[cfg(test)]
    pub(super) fn assert_invariants(&self) {
        let Some(root) = self.root else {
            assert!(self.by_identity.is_empty());
            assert_eq!(self.rightmost_leaf, None);
            let free: FxHashSet<_> = self.free.iter().copied().collect();
            assert_eq!(free.len(), self.free.len(), "B+ free list has duplicates");
            assert_eq!(free.len(), self.next_slot);
            for index in 0..self.next_slot {
                let id = OrderedNodeId::from_index(index);
                assert!(self.slot(id).is_none());
                assert!(free.contains(&id));
            }
            return;
        };
        assert_eq!(self.node(root).parent, None);
        let mut expected_rightmost = root;
        while let OrderedNodeKind::Branch(children) = &self.node(expected_rightmost).kind {
            expected_rightmost = *children.last().expect("live B+ branch is nonempty");
        }
        assert_eq!(self.rightmost_leaf, Some(expected_rightmost));
        let mut records = Vec::new();
        let mut reachable_nodes = FxHashSet::default();
        let mut leaf_depth = None;
        let summary = self.assert_node_invariants(
            root,
            None,
            1,
            EmacsByteDelta::ZERO,
            &mut leaf_depth,
            &mut records,
            &mut reachable_nodes,
        );
        assert_eq!(summary.count, self.by_identity.len());
        assert_eq!(records.len(), self.by_identity.len());
        for pair in records.windows(2) {
            assert!(pair[0].key() < pair[1].key(), "B+ record ordering drifted");
        }
        for record in records {
            let leaf = self.by_identity[&record.identity()];
            assert!(
                self.leaf_records(leaf)
                    .iter()
                    .any(|candidate| candidate.identity() == record.identity())
            );
            assert_eq!(
                self.record(record.identity()).map(R::key),
                Some(record.key())
            );
        }
        let free: FxHashSet<_> = self.free.iter().copied().collect();
        assert_eq!(free.len(), self.free.len(), "B+ free list has duplicates");
        for index in 0..self.next_slot {
            let id = OrderedNodeId::from_index(index);
            assert_eq!(
                self.slot(id).is_some(),
                reachable_nodes.contains(&id),
                "B+ arena contains an unreachable live node"
            );
            assert_eq!(
                self.slot(id).is_none(),
                free.contains(&id),
                "B+ free-list membership disagrees with arena occupancy"
            );
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    fn assert_node_invariants(
        &self,
        id: OrderedNodeId,
        parent: Option<OrderedNodeId>,
        depth: usize,
        inherited: EmacsByteDelta,
        leaf_depth: &mut Option<usize>,
        records: &mut Vec<R>,
        reachable_nodes: &mut FxHashSet<OrderedNodeId>,
    ) -> OrderedSummary<R::Key> {
        let node = self.node(id);
        assert!(reachable_nodes.insert(id), "B+ node is reachable twice");
        assert_eq!(node.parent, parent);
        let record_delta = inherited.combine(node.pending_shift);
        let (expected, expected_prefix, expected_branch_minimums) = match &node.kind {
            OrderedNodeKind::Leaf(raw) => {
                if Some(id) != self.root {
                    assert!((MIN_ENTRIES..=MAX_ENTRIES).contains(&raw.len()));
                }
                if let Some(expected_depth) = *leaf_depth {
                    assert_eq!(depth, expected_depth, "B+ leaves have unequal depth");
                } else {
                    *leaf_depth = Some(depth);
                }
                let logical: SmallVec<[_; INLINE_ENTRIES]> = raw
                    .iter()
                    .map(|record| record.shifted(record_delta))
                    .collect();
                records.extend(logical.iter().copied());
                let first = logical.first().expect("live B+ leaf is nonempty");
                let mut maximum: Option<EmacsBytePos> = None;
                let mut non_overlapping = true;
                let prefix = logical
                    .iter()
                    .map(|record| {
                        if maximum.is_some_and(|previous_end| previous_end > record.key_position())
                        {
                            non_overlapping = false;
                        }
                        let end = record.end_position();
                        maximum = Some(maximum.map_or(end, |current| current.max(end)));
                        maximum.expect("record just established a maximum")
                    })
                    .collect::<SmallVec<[_; INLINE_ENTRIES]>>();
                (
                    OrderedSummary {
                        first_key: first.key(),
                        last_key: logical.last().expect("first existed").key(),
                        min_position: first.key_position(),
                        max_position: logical.last().expect("first existed").key_position(),
                        max_end: *prefix.last().expect("first existed"),
                        filter_mask: logical
                            .iter()
                            .fold(OrderedFilterMask::EMPTY, |mask, record| {
                                mask | record.filter_mask()
                            }),
                        count: logical.len(),
                        non_overlapping,
                    },
                    prefix,
                    SmallVec::new(),
                )
            }
            OrderedNodeKind::Branch(children) => {
                if Some(id) != self.root {
                    assert!((MIN_ENTRIES..=MAX_ENTRIES).contains(&children.len()));
                } else {
                    assert!(children.len() >= 2);
                }
                let child_summaries: SmallVec<[_; INLINE_ENTRIES]> = children
                    .iter()
                    .map(|child| {
                        self.assert_node_invariants(
                            *child,
                            Some(id),
                            depth + 1,
                            record_delta,
                            leaf_depth,
                            records,
                            reachable_nodes,
                        )
                    })
                    .collect();
                let first = child_summaries.first().expect("branch has children");
                let mut maximum: Option<EmacsBytePos> = None;
                let mut non_overlapping = true;
                let prefix = child_summaries
                    .iter()
                    .map(|summary| {
                        if !summary.non_overlapping
                            || maximum
                                .is_some_and(|previous_end| previous_end > summary.min_position)
                        {
                            non_overlapping = false;
                        }
                        maximum = Some(
                            maximum.map_or(summary.max_end, |current| current.max(summary.max_end)),
                        );
                        maximum.expect("child just established a maximum")
                    })
                    .collect::<SmallVec<[_; INLINE_ENTRIES]>>();
                let minimums = child_summaries
                    .iter()
                    .map(|summary| summary.min_position)
                    .collect::<SmallVec<[_; INLINE_ENTRIES]>>();
                (
                    OrderedSummary {
                        first_key: first.first_key,
                        last_key: child_summaries.last().expect("first existed").last_key,
                        min_position: first.min_position,
                        max_position: child_summaries.last().expect("first existed").max_position,
                        max_end: *prefix.last().expect("first existed"),
                        filter_mask: child_summaries
                            .iter()
                            .fold(OrderedFilterMask::EMPTY, |mask, summary| {
                                mask | summary.filter_mask
                            }),
                        count: child_summaries.iter().map(|summary| summary.count).sum(),
                        non_overlapping,
                    },
                    prefix,
                    minimums,
                )
            }
        };
        assert_eq!(
            node.summary.map(|summary| summary.shifted::<R>(inherited)),
            Some(expected),
            "B+ subtree summary drifted"
        );
        let stored_prefix = node
            .prefix_max_end
            .iter()
            .map(|maximum_end| inherited.apply_to_pos(*maximum_end))
            .collect::<SmallVec<[_; INLINE_ENTRIES]>>();
        assert_eq!(
            stored_prefix, expected_prefix,
            "B+ prefix maximum-end summary drifted"
        );
        let stored_branch_minimums = node
            .branch_min_positions
            .iter()
            .map(|minimum| record_delta.apply_to_pos(*minimum))
            .collect::<SmallVec<[_; INLINE_ENTRIES]>>();
        assert_eq!(
            stored_branch_minimums, expected_branch_minimums,
            "B+ branch separator positions drifted"
        );
        expected
    }
}

pub(super) trait OrderedTreeQuery<R: OrderedShiftRecord>: Copy {
    fn subtree_may_match(
        self,
        minimum: EmacsBytePos,
        maximum: EmacsBytePos,
        maximum_end: EmacsBytePos,
    ) -> bool;
    fn record_matches(self, record: R) -> bool;

    /// Whether a subtree's conservative record-class union can contain a
    /// match. False skips the subtree without reading its records.
    fn subtree_filter_may_match(self, _filter_mask: OrderedFilterMask) -> bool {
        true
    }

    /// A point query over a globally non-overlapping tree has at most one
    /// result and may descend through one start separator per level.
    fn can_use_non_overlapping_single_path(self) -> bool {
        false
    }

    /// Whether a prefix whose greatest interval end is `maximum_end` cannot
    /// contain a match.  Implementations must be monotone: once this becomes
    /// false for an ordered prefix, it remains false.
    fn maximum_end_is_too_small(self, _maximum_end: EmacsBytePos) -> bool {
        false
    }

    /// Whether an ordered record/child starting at `minimum_start` and every
    /// following record/child cannot match.  Implementations must be monotone.
    fn minimum_start_is_too_large(self, _minimum_start: EmacsBytePos) -> bool {
        false
    }

    #[cfg(test)]
    fn record_subtree_visit(self) {}
}

#[derive(Clone, Copy)]
struct OrderedTraversalFrame {
    id: OrderedNodeId,
    cursor: usize,
    end: usize,
    inherited: EmacsByteDelta,
}

const EMPTY_TRAVERSAL_FRAME: OrderedTraversalFrame = OrderedTraversalFrame {
    id: OrderedNodeId(0),
    cursor: 0,
    end: 0,
    inherited: EmacsByteDelta::ZERO,
};

#[derive(Clone, Copy)]
enum OrderedTraversalDirection {
    Forward,
    Reverse,
}

pub(super) struct OrderedTreeMatches<T, R, Q>
where
    T: Deref<Target = OrderedShiftTree<R>>,
    R: OrderedShiftRecord,
    Q: OrderedTreeQuery<R>,
{
    tree: T,
    query: Q,
    direction: OrderedTraversalDirection,
    frames: [OrderedTraversalFrame; MAX_TREE_DEPTH],
    len: usize,
}

impl<T, R, Q> OrderedTreeMatches<T, R, Q>
where
    T: Deref<Target = OrderedShiftTree<R>>,
    R: OrderedShiftRecord,
    Q: OrderedTreeQuery<R>,
{
    #[inline(always)]
    fn new(tree: T, query: Q) -> Self {
        Self::in_direction(tree, query, OrderedTraversalDirection::Forward)
    }

    fn new_reverse(tree: T, query: Q) -> Self {
        Self::in_direction(tree, query, OrderedTraversalDirection::Reverse)
    }

    fn in_direction(tree: T, query: Q, direction: OrderedTraversalDirection) -> Self {
        let root = tree.root;
        let mut matches = Self {
            tree,
            query,
            direction,
            frames: [EMPTY_TRAVERSAL_FRAME; MAX_TREE_DEPTH],
            len: 0,
        };
        if let Some(root) = root {
            matches.push_if_relevant(root, EmacsByteDelta::ZERO);
        }
        matches
    }

    #[inline(always)]
    fn push_if_relevant(&mut self, id: OrderedNodeId, inherited: EmacsByteDelta) {
        #[cfg(test)]
        super::overlay::record_overlay_iterator_frame_push();
        #[cfg(test)]
        self.query.record_subtree_visit();
        let summary = self.tree.summary(id);
        if !self.query.subtree_may_match(
            inherited.apply_to_pos(summary.min_position),
            inherited.apply_to_pos(summary.max_position),
            inherited.apply_to_pos(summary.max_end),
        ) || !self.query.subtree_filter_may_match(summary.filter_mask)
        {
            return;
        }
        assert!(
            self.len < self.frames.len(),
            "overlay B+ tree exceeded depth bound"
        );
        let node = self.tree.node(id);
        let record_delta = inherited.combine(node.pending_shift);
        // `prefix_max_end` already includes this node's pending shift (it is
        // updated with the summary), so only the shift inherited from parents
        // is applied to it here.
        let cursor = node.prefix_max_end.partition_point(|maximum_end| {
            self.query
                .maximum_end_is_too_small(inherited.apply_to_pos(*maximum_end))
        });
        let end = match &node.kind {
            OrderedNodeKind::Leaf(records) => records.partition_point(|record| {
                !self
                    .query
                    .minimum_start_is_too_large(record_delta.apply_to_pos(record.key_position()))
            }),
            OrderedNodeKind::Branch(_) => node.branch_min_positions.partition_point(|minimum| {
                !self
                    .query
                    .minimum_start_is_too_large(record_delta.apply_to_pos(*minimum))
            }),
        };
        if cursor >= end {
            return;
        }
        self.frames[self.len] = OrderedTraversalFrame {
            id,
            cursor,
            end,
            inherited,
        };
        self.len += 1;
    }
}

impl<T, R, Q> Iterator for OrderedTreeMatches<T, R, Q>
where
    T: Deref<Target = OrderedShiftTree<R>>,
    R: OrderedShiftRecord,
    Q: OrderedTreeQuery<R>,
{
    type Item = R;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        while self.len > 0 {
            let frame_index = self.len - 1;
            let mut frame = self.frames[frame_index];
            let node = self.tree.node(frame.id);
            let record_delta = frame.inherited.combine(node.pending_shift);
            match &node.kind {
                OrderedNodeKind::Leaf(records) => {
                    if frame.cursor == frame.end {
                        self.len -= 1;
                        continue;
                    }
                    let record_index = match self.direction {
                        OrderedTraversalDirection::Forward => {
                            let index = frame.cursor;
                            frame.cursor += 1;
                            index
                        }
                        OrderedTraversalDirection::Reverse => {
                            frame.end -= 1;
                            frame.end
                        }
                    };
                    let record = if record_delta.is_zero() {
                        records[record_index]
                    } else {
                        records[record_index].shifted(record_delta)
                    };
                    self.frames[frame_index] = frame;
                    if self.query.record_matches(record) {
                        return Some(record);
                    }
                }
                OrderedNodeKind::Branch(children) => {
                    if frame.cursor == frame.end {
                        self.len -= 1;
                        continue;
                    }
                    let child_index = match self.direction {
                        OrderedTraversalDirection::Forward => {
                            let index = frame.cursor;
                            frame.cursor += 1;
                            index
                        }
                        OrderedTraversalDirection::Reverse => {
                            frame.end -= 1;
                            frame.end
                        }
                    };
                    let child = children[child_index];
                    self.frames[frame_index] = frame;
                    self.push_if_relevant(child, record_delta);
                }
            }
        }
        None
    }
}
