//! GNU-compatible structural ordering for overlay mutations.
//!
//! The authoritative interval and endpoint queries live in high-fanout B+
//! trees.  GNU Emacs nevertheless exposes one detail of its red-black
//! interval tree during insertion: front-advancing nodes are collected in
//! tree pre-order, removed, and reinserted in reverse collection order.  This
//! compact topology mirror preserves exactly that ordering state without
//! duplicating interval positions or query augmentation.

#![deny(clippy::wildcard_enum_match_arm)]

use std::cmp::Ordering;
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;
use std::mem::size_of;
use std::rc::Rc;

use rustc_hash::FxHashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Color {
    Red,
    Black,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Descent {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
struct OrderNodeId(u32);

impl OrderNodeId {
    fn index(self) -> usize {
        self.0 as usize
    }
}

/// A slot removed from the linked topology but still owned by its arena.
///
/// Only `detach_node` constructs this token. Consuming it is required to
/// recycle the slot or restore its retained node; a linked `OrderNodeId`
/// cannot be passed to either operation. The token is call-local under the
/// mirror's exclusive borrow and cannot be copied to finish a detach twice.
/// It carries authority over a mutator-owned arena slot, so its zero-sized
/// `Rc` marker also prevents sending or sharing it with another mutator.
#[derive(Debug)]
#[repr(transparent)]
#[must_use = "a detached GNU order node must be recycled or reinserted"]
struct DetachedOrderNode {
    node: OrderNodeId,
    _mutator: PhantomData<Rc<()>>,
}

static_assertions::assert_impl_all!(OrderNodeId: Copy, Send, Sync);
static_assertions::assert_not_impl_any!(DetachedOrderNode: Copy, Clone, Send, Sync);

const _: () = assert!(size_of::<OrderNodeId>() == size_of::<u32>());
const _: () = assert!(size_of::<DetachedOrderNode>() == size_of::<OrderNodeId>());

/// Membership failures leave the mirror unchanged.
///
/// These are internal index contracts, not Lisp conditions. Callers that have
/// already validated membership treat a failure as invariant corruption.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum GnuOverlayOrderError {
    #[error("overlay identity is already present in the GNU order mirror")]
    AlreadyPresent,
    #[error("overlay identity is missing from the GNU order mirror")]
    MissingIdentity,
}

// Typed mutation results retain the old bool's one-byte representation.
const _: () = assert!(size_of::<Result<(), GnuOverlayOrderError>>() == size_of::<bool>());

#[derive(Clone, Copy, Debug)]
struct OrderNode<I> {
    identity: I,
    color: Color,
    parent: Option<OrderNodeId>,
    left: Option<OrderNodeId>,
    right: Option<OrderNodeId>,
}

/// A topology-only mirror of GNU's overlay red-black tree.
///
/// Starts are deliberately not stored here.  Text edits can lazily shift a
/// whole B+ subtree, so copying positions into this mirror would either make
/// edits linear or create two position authorities.  Callers instead provide
/// the comparison with current authoritative positions when inserting.
#[derive(Clone, Debug)]
pub(super) struct GnuOverlayOrder<I>
where
    I: Copy + Debug + Eq + Hash,
{
    root: Option<OrderNodeId>,
    nodes: Vec<Option<OrderNode<I>>>,
    free: Vec<OrderNodeId>,
    by_identity: FxHashMap<I, OrderNodeId>,
}

impl<I> GnuOverlayOrder<I>
where
    I: Copy + Debug + Eq + Hash,
{
    pub(super) fn new() -> Self {
        Self {
            root: None,
            nodes: Vec::new(),
            free: Vec::new(),
            by_identity: FxHashMap::default(),
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.by_identity.len()
    }

    /// Insert using GNU's `new_start <= existing_start` left descent.
    ///
    /// # Errors
    /// Returns `AlreadyPresent` before comparing or mutating a duplicate.
    pub(super) fn insert_by(
        &mut self,
        identity: I,
        mut compare_with_existing: impl FnMut(I) -> Ordering,
    ) -> Result<(), GnuOverlayOrderError> {
        if self.by_identity.contains_key(&identity) {
            return Err(GnuOverlayOrderError::AlreadyPresent);
        }

        let mut parent = None;
        let mut child = self.root;
        let mut descent = Descent::Left;
        while let Some(id) = child {
            parent = Some(id);
            descent = if compare_with_existing(self.node(id).identity) != Ordering::Greater {
                Descent::Left
            } else {
                Descent::Right
            };
            child = match descent {
                Descent::Left => self.node(id).left,
                Descent::Right => self.node(id).right,
            };
        }

        let id = self.allocate(OrderNode {
            identity,
            color: if parent.is_some() {
                Color::Red
            } else {
                Color::Black
            },
            parent,
            left: None,
            right: None,
        });
        self.by_identity.insert(identity, id);
        match parent {
            None => self.root = Some(id),
            Some(parent) => match descent {
                Descent::Left => self.node_mut(parent).left = Some(id),
                Descent::Right => self.node_mut(parent).right = Some(id),
            },
        }
        if parent.is_some() {
            self.insert_fix(id);
        }
        Ok(())
    }

    /// Return a member's predecessor in GNU's structural in-order traversal.
    ///
    /// Coordinate contraction can make this order differ from attachment
    /// serials at equal starts. This pure query needs only buffer-owned tree
    /// links under the owner's existing borrow, with no shared Lisp state or
    /// cached positions. It walks at most one tree-height path.
    #[inline]
    pub(super) fn predecessor_identity(&self, identity: I) -> Option<I> {
        let mut current = *self
            .by_identity
            .get(&identity)
            .expect("GNU order predecessor target is missing from the mirror");
        if let Some(mut previous) = self.node(current).left {
            while let Some(right) = self.node(previous).right {
                previous = right;
            }
            return Some(self.node(previous).identity);
        }
        while let Some(parent) = self.node(current).parent {
            if self.node(parent).right == Some(current) {
                return Some(self.node(parent).identity);
            }
            current = parent;
        }
        None
    }

    /// Insert immediately before a validated structural successor.
    ///
    /// The caller proves the gap matches GNU's `new_start <= existing_start`
    /// descent: its predecessor has a strictly smaller start and its successor
    /// has a greater or equal start. Attachment order alone does not prove
    /// this after a deletion collapses starts. The existing red-black fixup
    /// then preserves GNU's exact topology and front-advancing pre-order.
    /// Mutations require the owning buffer's exclusive access to this mirror;
    /// the successor is call-local state and is not shared across mutators.
    ///
    /// # Errors
    /// Returns `AlreadyPresent` without mutating a duplicate. A supplied
    /// successor must belong to the mirror, as validated by the owning index.
    pub(super) fn insert_before(
        &mut self,
        identity: I,
        successor: Option<I>,
    ) -> Result<(), GnuOverlayOrderError> {
        if self.by_identity.contains_key(&identity) {
            return Err(GnuOverlayOrderError::AlreadyPresent);
        }

        let (parent, descent) = match successor {
            Some(successor) => {
                let successor = *self
                    .by_identity
                    .get(&successor)
                    .expect("GNU order successor is missing from the mirror");
                match self.node(successor).left {
                    None => (Some(successor), Descent::Left),
                    Some(mut predecessor) => {
                        while let Some(right) = self.node(predecessor).right {
                            predecessor = right;
                        }
                        (Some(predecessor), Descent::Right)
                    }
                }
            }
            None => match self.root {
                None => (None, Descent::Left),
                Some(mut last) => {
                    while let Some(right) = self.node(last).right {
                        last = right;
                    }
                    (Some(last), Descent::Right)
                }
            },
        };
        let id = self.allocate(OrderNode {
            identity,
            color: if parent.is_some() {
                Color::Red
            } else {
                Color::Black
            },
            parent,
            left: None,
            right: None,
        });
        self.by_identity.insert(identity, id);
        match parent {
            None => self.root = Some(id),
            Some(parent) => match descent {
                Descent::Left => self.node_mut(parent).left = Some(id),
                Descent::Right => self.node_mut(parent).right = Some(id),
            },
        }
        if parent.is_some() {
            self.insert_fix(id);
        }
        Ok(())
    }

    /// Remove one identity using the same successor-splice algorithm and
    /// fix-up cases as GNU's `itree_remove`.
    ///
    /// # Errors
    /// Returns `MissingIdentity` without mutation when the identity is absent.
    pub(super) fn remove(&mut self, identity: I) -> Result<(), GnuOverlayOrderError> {
        let Some(node) = self.by_identity.remove(&identity) else {
            return Err(GnuOverlayOrderError::MissingIdentity);
        };
        let detached = self.detach_node(node);
        self.recycle_detached_node(detached, identity);
        Ok(())
    }

    #[inline]
    fn recycle_detached_node(&mut self, detached: DetachedOrderNode, identity: I) {
        let node = detached.node;
        let removed = self.nodes[node.index()]
            .take()
            .expect("GNU order identity map referenced a vacant node");
        debug_assert_eq!(removed.identity, identity);
        self.free.push(node);
    }

    /// Reinsert an existing node with GNU's remove/fixup/insert/fixup order.
    ///
    /// `itree_node_set_region` (itree.c:750-755) retains the node itself while
    /// changing its numeric begin. Keep this mirror's identity entry and arena
    /// slot too: the ordinary remove/insert pair would free and immediately
    /// reacquire this same slot. No identity lookup or arena mutation is needed
    /// during the detached interval. The owner holds exclusive access; only
    /// the internal comparison runs before this method restores a valid tree.
    ///
    /// # Errors
    /// Returns `MissingIdentity` before comparing or mutating an absent member.
    pub(super) fn reinsert_by(
        &mut self,
        identity: I,
        compare_with_existing: impl FnMut(I) -> Ordering,
    ) -> Result<(), GnuOverlayOrderError> {
        let Some(node) = self.by_identity.get(&identity).copied() else {
            return Err(GnuOverlayOrderError::MissingIdentity);
        };
        let detached = self.detach_node(node);
        self.reinsert_detached_by(detached, identity, compare_with_existing);
        Ok(())
    }

    #[inline]
    fn reinsert_detached_by(
        &mut self,
        detached: DetachedOrderNode,
        identity: I,
        mut compare_with_existing: impl FnMut(I) -> Ordering,
    ) {
        let node = detached.node;
        let mut parent = None;
        let mut child = self.root;
        let mut descent = Descent::Left;
        while let Some(id) = child {
            parent = Some(id);
            descent = if compare_with_existing(self.node(id).identity) != Ordering::Greater {
                Descent::Left
            } else {
                Descent::Right
            };
            child = match descent {
                Descent::Left => self.node(id).left,
                Descent::Right => self.node(id).right,
            };
        }

        self.nodes[node.index()] = Some(OrderNode {
            identity,
            color: if parent.is_some() {
                Color::Red
            } else {
                Color::Black
            },
            parent,
            left: None,
            right: None,
        });
        match parent {
            None => self.root = Some(node),
            Some(parent) => match descent {
                Descent::Left => self.node_mut(parent).left = Some(node),
                Descent::Right => self.node_mut(parent).right = Some(node),
            },
        }
        if parent.is_some() {
            self.insert_fix(node);
        }
    }

    /// Detach a node using GNU's successor splice and deletion fixup.
    /// The caller retains responsibility for its identity entry and slot.
    #[inline]
    fn detach_node(&mut self, node: OrderNodeId) -> DetachedOrderNode {
        let splice = if self.node(node).left.is_none() || self.node(node).right.is_none() {
            node
        } else {
            self.subtree_min(
                self.node(node)
                    .right
                    .expect("two-child node has a right subtree"),
            )
        };
        let subtree = self.node(splice).left.or(self.node(splice).right);
        let splice_parent = self.node(splice).parent;
        let subtree_parent = if splice_parent != Some(node) {
            splice_parent
        } else {
            Some(splice)
        };
        let removed_black = self.node(splice).color == Color::Black;

        self.replace_child(subtree, splice);
        if splice != node {
            self.transplant(splice, node);
        }
        if removed_black {
            self.remove_fix(subtree, subtree_parent);
        }
        DetachedOrderNode {
            node,
            _mutator: PhantomData,
        }
    }

    /// Visit structural in-order without allocating a root path per node.
    /// Parent/child links belong to the exclusively borrowed owning buffer;
    /// the cursor is call-local and stores no shared Lisp state.
    pub(super) fn for_each_inorder(&self, mut visit: impl FnMut(I)) {
        let mut current = self.root;
        while let Some(id) = current {
            if let Some(left) = self.node(id).left {
                current = Some(left);
            } else {
                break;
            }
        }
        while let Some(id) = current {
            visit(self.node(id).identity);
            if let Some(mut next) = self.node(id).right {
                while let Some(left) = self.node(next).left {
                    next = left;
                }
                current = Some(next);
            } else {
                let mut child = id;
                current = None;
                while let Some(parent) = self.node(child).parent {
                    if self.node(parent).left == Some(child) {
                        current = Some(parent);
                        break;
                    }
                    child = parent;
                }
            }
        }
    }

    /// Return `identities` in the current GNU tree's pre-order.
    ///
    /// Comparing root-to-node paths avoids walking unrelated overlays: the
    /// cost is proportional to the selected boundary set and tree height.
    pub(super) fn subset_in_preorder(&self, identities: &[I]) -> Vec<I> {
        let mut paths: Vec<_> = identities
            .iter()
            .copied()
            .map(|identity| {
                let id = *self
                    .by_identity
                    .get(&identity)
                    .expect("indexed overlay missing from GNU order mirror");
                (self.path_from_root(id), identity)
            })
            .collect();
        paths.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        paths.into_iter().map(|(_, identity)| identity).collect()
    }

    fn path_from_root(&self, mut node: OrderNodeId) -> Vec<Descent> {
        let mut reversed = Vec::new();
        while let Some(parent) = self.node(node).parent {
            reversed.push(if self.node(parent).left == Some(node) {
                Descent::Left
            } else {
                debug_assert_eq!(self.node(parent).right, Some(node));
                Descent::Right
            });
            node = parent;
        }
        reversed.reverse();
        reversed
    }

    fn allocate(&mut self, node: OrderNode<I>) -> OrderNodeId {
        if let Some(id) = self.free.pop() {
            debug_assert!(self.nodes[id.index()].is_none());
            self.nodes[id.index()] = Some(node);
            id
        } else {
            let index = self.nodes.len();
            let id = OrderNodeId(u32::try_from(index).expect("GNU overlay order arena exhausted"));
            self.nodes.push(Some(node));
            id
        }
    }

    fn node(&self, id: OrderNodeId) -> &OrderNode<I> {
        self.nodes[id.index()]
            .as_ref()
            .expect("GNU overlay order node is vacant")
    }

    fn node_mut(&mut self, id: OrderNodeId) -> &mut OrderNode<I> {
        self.nodes[id.index()]
            .as_mut()
            .expect("GNU overlay order node is vacant")
    }

    fn color(&self, node: Option<OrderNodeId>) -> Color {
        node.map_or(Color::Black, |id| self.node(id).color)
    }

    fn set_color(&mut self, node: Option<OrderNodeId>, color: Color) {
        if let Some(id) = node {
            self.node_mut(id).color = color;
        }
    }

    fn subtree_min(&self, mut node: OrderNodeId) -> OrderNodeId {
        while let Some(left) = self.node(node).left {
            node = left;
        }
        node
    }

    fn replace_child(&mut self, source: Option<OrderNodeId>, dest: OrderNodeId) {
        let parent = self.node(dest).parent;
        match parent {
            None => self.root = source,
            Some(parent) if self.node(parent).left == Some(dest) => {
                self.node_mut(parent).left = source;
            }
            Some(parent) => {
                debug_assert_eq!(self.node(parent).right, Some(dest));
                self.node_mut(parent).right = source;
            }
        }
        if let Some(source) = source {
            self.node_mut(source).parent = parent;
        }
    }

    fn transplant(&mut self, source: OrderNodeId, dest: OrderNodeId) {
        self.replace_child(Some(source), dest);
        let left = self.node(dest).left;
        let right = self.node(dest).right;
        let color = self.node(dest).color;
        {
            let source_node = self.node_mut(source);
            source_node.left = left;
            source_node.right = right;
            source_node.color = color;
        }
        if let Some(left) = left {
            self.node_mut(left).parent = Some(source);
        }
        if let Some(right) = right {
            self.node_mut(right).parent = Some(source);
        }
    }

    fn rotate_left(&mut self, node: OrderNodeId) {
        let right = self
            .node(node)
            .right
            .expect("left rotation needs right child");
        let right_left = self.node(right).left;
        self.node_mut(node).right = right_left;
        if let Some(right_left) = right_left {
            self.node_mut(right_left).parent = Some(node);
        }

        let parent = self.node(node).parent;
        self.node_mut(right).parent = parent;
        match parent {
            None => self.root = Some(right),
            Some(parent) if self.node(parent).left == Some(node) => {
                self.node_mut(parent).left = Some(right);
            }
            Some(parent) => {
                debug_assert_eq!(self.node(parent).right, Some(node));
                self.node_mut(parent).right = Some(right);
            }
        }
        self.node_mut(right).left = Some(node);
        self.node_mut(node).parent = Some(right);
    }

    fn rotate_right(&mut self, node: OrderNodeId) {
        let left = self
            .node(node)
            .left
            .expect("right rotation needs left child");
        let left_right = self.node(left).right;
        self.node_mut(node).left = left_right;
        if let Some(left_right) = left_right {
            self.node_mut(left_right).parent = Some(node);
        }

        let parent = self.node(node).parent;
        self.node_mut(left).parent = parent;
        match parent {
            None => self.root = Some(left),
            Some(parent) if self.node(parent).right == Some(node) => {
                self.node_mut(parent).right = Some(left);
            }
            Some(parent) => {
                debug_assert_eq!(self.node(parent).left, Some(node));
                self.node_mut(parent).left = Some(left);
            }
        }
        self.node_mut(left).right = Some(node);
        self.node_mut(node).parent = Some(left);
    }

    fn insert_fix(&mut self, mut node: OrderNodeId) {
        while self.color(self.node(node).parent) == Color::Red {
            let parent = self.node(node).parent.expect("red node has a parent");
            let grandparent = self
                .node(parent)
                .parent
                .expect("red parent has a grandparent");
            if self.node(grandparent).left == Some(parent) {
                let uncle = self.node(grandparent).right;
                if self.color(uncle) == Color::Red {
                    self.set_color(Some(parent), Color::Black);
                    self.set_color(uncle, Color::Black);
                    self.set_color(Some(grandparent), Color::Red);
                    node = grandparent;
                } else {
                    if self.node(parent).right == Some(node) {
                        node = parent;
                        self.rotate_left(node);
                    }
                    let parent = self.node(node).parent.expect("rotated node has a parent");
                    let grandparent = self
                        .node(parent)
                        .parent
                        .expect("rotated parent has a grandparent");
                    self.set_color(Some(parent), Color::Black);
                    self.set_color(Some(grandparent), Color::Red);
                    self.rotate_right(grandparent);
                }
            } else {
                debug_assert_eq!(self.node(grandparent).right, Some(parent));
                let uncle = self.node(grandparent).left;
                if self.color(uncle) == Color::Red {
                    self.set_color(Some(parent), Color::Black);
                    self.set_color(uncle, Color::Black);
                    self.set_color(Some(grandparent), Color::Red);
                    node = grandparent;
                } else {
                    if self.node(parent).left == Some(node) {
                        node = parent;
                        self.rotate_right(node);
                    }
                    let parent = self.node(node).parent.expect("rotated node has a parent");
                    let grandparent = self
                        .node(parent)
                        .parent
                        .expect("rotated parent has a grandparent");
                    self.set_color(Some(parent), Color::Black);
                    self.set_color(Some(grandparent), Color::Red);
                    self.rotate_left(grandparent);
                }
            }
        }
        self.set_color(self.root, Color::Black);
    }

    fn remove_fix(&mut self, mut node: Option<OrderNodeId>, mut parent: Option<OrderNodeId>) {
        while let Some(parent_id) = parent.filter(|_| self.color(node) == Color::Black) {
            if self.node(parent_id).left == node {
                let mut other = self
                    .node(parent_id)
                    .right
                    .expect("black-height deficit has a right sibling");
                if self.color(Some(other)) == Color::Red {
                    self.set_color(Some(other), Color::Black);
                    self.set_color(Some(parent_id), Color::Red);
                    self.rotate_left(parent_id);
                    other = self
                        .node(parent_id)
                        .right
                        .expect("rotation provides a right sibling");
                }
                if self.color(self.node(other).left) == Color::Black
                    && self.color(self.node(other).right) == Color::Black
                {
                    self.set_color(Some(other), Color::Red);
                    node = Some(parent_id);
                    parent = self.node(parent_id).parent;
                } else {
                    if self.color(self.node(other).right) == Color::Black {
                        self.set_color(self.node(other).left, Color::Black);
                        self.set_color(Some(other), Color::Red);
                        self.rotate_right(other);
                        other = self
                            .node(parent_id)
                            .right
                            .expect("rotation provides a right sibling");
                    }
                    let parent_color = self.node(parent_id).color;
                    self.set_color(Some(other), parent_color);
                    self.set_color(Some(parent_id), Color::Black);
                    self.set_color(self.node(other).right, Color::Black);
                    self.rotate_left(parent_id);
                    node = self.root;
                    parent = None;
                }
            } else {
                debug_assert_eq!(self.node(parent_id).right, node);
                let mut other = self
                    .node(parent_id)
                    .left
                    .expect("black-height deficit has a left sibling");
                if self.color(Some(other)) == Color::Red {
                    self.set_color(Some(other), Color::Black);
                    self.set_color(Some(parent_id), Color::Red);
                    self.rotate_right(parent_id);
                    other = self
                        .node(parent_id)
                        .left
                        .expect("rotation provides a left sibling");
                }
                if self.color(self.node(other).right) == Color::Black
                    && self.color(self.node(other).left) == Color::Black
                {
                    self.set_color(Some(other), Color::Red);
                    node = Some(parent_id);
                    parent = self.node(parent_id).parent;
                } else {
                    if self.color(self.node(other).left) == Color::Black {
                        self.set_color(self.node(other).right, Color::Black);
                        self.set_color(Some(other), Color::Red);
                        self.rotate_left(other);
                        other = self
                            .node(parent_id)
                            .left
                            .expect("rotation provides a left sibling");
                    }
                    let parent_color = self.node(parent_id).color;
                    self.set_color(Some(other), parent_color);
                    self.set_color(Some(parent_id), Color::Black);
                    self.set_color(self.node(other).left, Color::Black);
                    self.rotate_right(parent_id);
                    node = self.root;
                    parent = None;
                }
            }
        }
        self.set_color(node, Color::Black);
    }

    #[cfg(test)]
    pub(super) fn assert_invariants(&self) {
        assert_eq!(self.by_identity.len(), self.nodes.iter().flatten().count());
        if let Some(root) = self.root {
            assert_eq!(self.node(root).parent, None);
            assert_eq!(self.node(root).color, Color::Black);
            self.assert_subtree_invariants(root);
        } else {
            assert!(self.by_identity.is_empty());
        }
    }

    #[cfg(test)]
    fn assert_subtree_invariants(&self, node: OrderNodeId) -> usize {
        let record = self.node(node);
        assert_eq!(self.by_identity.get(&record.identity), Some(&node));
        for child in [record.left, record.right].into_iter().flatten() {
            assert_eq!(self.node(child).parent, Some(node));
            if record.color == Color::Red {
                assert_eq!(self.node(child).color, Color::Black);
            }
        }
        let left_height = record
            .left
            .map_or(1, |left| self.assert_subtree_invariants(left));
        let right_height = record
            .right
            .map_or(1, |right| self.assert_subtree_invariants(right));
        assert_eq!(left_height, right_height);
        left_height + usize::from(record.color == Color::Black)
    }
}

#[cfg(test)]
#[path = "overlay_order/tests/overlay_order_test.rs"]
mod tests;
