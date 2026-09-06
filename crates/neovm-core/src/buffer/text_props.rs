//! Text-property interval storage for buffers and strings.
//!
//! GNU Emacs represents text properties as an interval tree rooted from the
//! owning string or buffer.  This module keeps the existing Rust API name
//! (`TextPropertyTable`) for callers while preserving GNU's mutation shape:
//! split at the edit range, change the affected interval plists, and preserve
//! raw interval boundaries.  Higher-level property-change queries decide
//! whether adjacent interval plists are semantically equal.

use crate::emacs_core::intern::SymId;
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

use rustc_hash::FxHashSet;

use super::{CharLen, CharPos0, CharRange};
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::value::{Value, eq_value, equal_value};
use crate::gc_trace::GcTrace;

#[cfg(test)]
thread_local! {
    static INTERVAL_BALANCE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_interval_balance_calls_for_test() {
    INTERVAL_BALANCE_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
pub(crate) fn interval_balance_calls_for_test() -> usize {
    INTERVAL_BALANCE_CALLS.with(std::cell::Cell::get)
}

// ---------------------------------------------------------------------------
// PropertyInterval
// ---------------------------------------------------------------------------

/// Public snapshot of one text-property interval.
///
/// Runtime storage uses the same start/end/plist shape.  This type remains the
/// serialization and inspection shape used by pdump/tests.  Bounds are character
/// positions, matching GNU intervals; buffer owners convert byte positions at
/// the boundary.
#[derive(Clone, Debug)]
pub struct PropertyInterval {
    /// Character position where this interval starts (inclusive).
    pub start: usize,
    /// Character position where this interval ends (exclusive).
    pub end: usize,
    /// Snapshot map for the interval plist.
    pub properties: HashMap<Value, Value>,
    /// Property keys in GNU plist order, newest first.
    pub(crate) key_order: Vec<Value>,
}

/// GNU `object-intervals` shaped run for the complete object partition.
///
/// Bounds are character positions.  Unlike [`PropertyInterval`], this also
/// represents nil-property gaps once an interval tree exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectIntervalRun {
    start: CharPos0,
    end: CharPos0,
    properties: Vec<(Value, Value)>,
}

impl ObjectIntervalRun {
    pub fn new(start: CharPos0, end: CharPos0, properties: Vec<(Value, Value)>) -> Self {
        Self {
            start,
            end,
            properties,
        }
    }

    pub fn start(&self) -> CharPos0 {
        self.start
    }

    pub fn end(&self) -> CharPos0 {
        self.end
    }

    pub fn properties(&self) -> &[(Value, Value)] {
        &self.properties
    }

    pub fn into_parts(self) -> (CharPos0, CharPos0, Vec<(Value, Value)>) {
        (self.start, self.end, self.properties)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectIntervalPlistRun {
    start: CharPos0,
    end: CharPos0,
    plist: Value,
}

impl ObjectIntervalPlistRun {
    pub fn new(start: CharPos0, end: CharPos0, plist: Value) -> Self {
        debug_assert!(start.get() <= end.get());
        Self { start, end, plist }
    }

    pub fn start(&self) -> CharPos0 {
        self.start
    }

    pub fn end(&self) -> CharPos0 {
        self.end
    }

    pub fn plist(&self) -> Value {
        self.plist
    }
}

impl PartialEq<(usize, usize, Vec<(Value, Value)>)> for ObjectIntervalRun {
    fn eq(&self, other: &(usize, usize, Vec<(Value, Value)>)) -> bool {
        self.start.get() == other.0 && self.end.get() == other.1 && self.properties == other.2
    }
}

impl PropertyInterval {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn new(range: CharRange) -> Self {
        Self {
            start: range.start().get(),
            end: range.end().get(),
            properties: HashMap::new(),
            key_order: Vec::new(),
        }
    }

    pub fn with_properties(range: CharRange, properties: HashMap<Value, Value>) -> Self {
        let key_order: Vec<Value> = properties.keys().copied().collect();
        Self {
            start: range.start().get(),
            end: range.end().get(),
            properties,
            key_order,
        }
    }

    fn from_plist(range: CharRange, plist: &[(Value, Value)]) -> Self {
        let mut properties = HashMap::new();
        for (key, value) in plist.iter().rev() {
            properties.insert(*key, *value);
        }
        let mut key_order = Vec::new();
        for (key, _) in plist {
            if !key_order.iter().any(|seen| eq_value(seen, key)) {
                key_order.push(*key);
            }
        }
        Self {
            start: range.start().get(),
            end: range.end().get(),
            properties,
            key_order,
        }
    }

    fn into_plist(self) -> Vec<(Value, Value)> {
        let mut plist = Vec::new();
        for key in &self.key_order {
            if let Some(value) = self.properties.get(key)
                && !plist.iter().any(|(seen, _)| eq_value(seen, key))
            {
                plist.push((*key, *value));
            }
        }
        for (key, value) in self.properties {
            if !plist.iter().any(|(seen, _)| eq_value(seen, &key)) {
                plist.push((key, value));
            }
        }
        plist
    }

    /// Iterate properties in GNU plist order.
    pub fn ordered_properties(&self) -> impl Iterator<Item = (Value, &Value)> {
        self.key_order
            .iter()
            .filter_map(move |key| self.properties.get(key).map(|value| (*key, value)))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TextPropertyPlistRun {
    range: CharRange,
    plist: Vec<(Value, Value)>,
}

impl TextPropertyPlistRun {
    pub(crate) fn new(range: CharRange, plist: Vec<(Value, Value)>) -> Self {
        Self { range, plist }
    }

    pub(crate) fn range(&self) -> CharRange {
        self.range
    }

    fn into_interval_run(self) -> Option<IntervalRun> {
        (!self.range.is_empty()).then(|| {
            IntervalRun::new_in_char_range(self.range, plist_value_from_pairs(&self.plist))
        })
    }
}

type IntervalPlist = Value;

fn plist_value_from_pairs(plist: &[(Value, Value)]) -> Value {
    let mut items = Vec::with_capacity(plist.len() * 2);
    for (key, value) in plist {
        items.push(*key);
        items.push(*value);
    }
    Value::list(items)
}

/// Build a fresh plist with new cons cells holding the same keys/values, like
/// GNU `Fcopy_sequence` of an interval plist.  The (atom-or-not) values are
/// shared; only the spine cons cells are new, so an in-place value-cell
/// rewrite of the copy cannot reach the original plist that may have been
/// handed to Lisp by `text-properties-at`.
fn copy_plist_value(plist: Value) -> Value {
    if plist.is_nil() {
        return Value::NIL;
    }
    plist_value_from_pairs(&plist_pairs(plist))
}

/// The property symbols `IntervalNode::extract_cached` looks for, interned
/// once (these are core symbols interned at bootstrap, so their ids are stable
/// across a pdump load).
struct PlistFlagSymbols {
    front_sticky: SymId,
    rear_nonsticky: SymId,
    read_only: SymId,
    invisible: SymId,
    syntax_table: SymId,
    category: SymId,
}

fn cached_plist_flag_symbols() -> &'static PlistFlagSymbols {
    static SYMS: std::sync::OnceLock<PlistFlagSymbols> = std::sync::OnceLock::new();
    SYMS.get_or_init(|| PlistFlagSymbols {
        front_sticky: crate::emacs_core::intern::intern("front-sticky"),
        rear_nonsticky: crate::emacs_core::intern::intern("rear-nonsticky"),
        read_only: crate::emacs_core::intern::intern("read-only"),
        invisible: crate::emacs_core::intern::intern("invisible"),
        syntax_table: crate::emacs_core::intern::intern("syntax-table"),
        category: crate::emacs_core::intern::intern("category"),
    })
}

fn plist_pairs(plist: Value) -> Vec<(Value, Value)> {
    let mut pairs = Vec::new();
    let mut tail = plist;
    while tail.is_cons() {
        let key = tail.cons_car();
        let rest = tail.cons_cdr();
        if !rest.is_cons() {
            break;
        }
        pairs.push((key, rest.cons_car()));
        tail = rest.cons_cdr();
    }
    pairs
}

fn plist_value_prepend_pair(plist: Value, key: Value, value: Value) -> Value {
    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(plist);
    push_scratch_gc_root(key);
    push_scratch_gc_root(value);

    let value_cell = Value::cons(value, plist);
    push_scratch_gc_root(value_cell);
    let result = Value::cons(key, value_cell);

    restore_scratch_gc_roots(saved);
    result
}

fn plist_value_get(plist: Value, key: Value) -> Option<Value> {
    let mut tail = plist;
    while tail.is_cons() {
        let name = tail.cons_car();
        let rest = tail.cons_cdr();
        if !rest.is_cons() {
            return None;
        }
        if eq_value(&name, &key) {
            return Some(rest.cons_car());
        }
        tail = rest.cons_cdr();
    }
    None
}

fn plist_value_put_replace(plist: &mut Value, key: Value, value: Value) -> bool {
    let mut tail = *plist;
    while tail.is_cons() {
        let name = tail.cons_car();
        let rest = tail.cons_cdr();
        if !rest.is_cons() {
            break;
        }
        if eq_value(&name, &key) {
            let existing = rest.cons_car();
            if eq_value(&existing, &value) {
                return false;
            }
            rest.set_car(value);
            return true;
        }
        tail = rest.cons_cdr();
    }

    // Missing key: prepend the pair onto the EXISTING plist — two new cons
    // cells sharing the old spine, exactly GNU `add_properties`
    // (`i->plist = Fcons (sym, Fcons (val, i->plist))`). The interval owns its
    // plist (splits copy it), so sharing the tail is private. The old code
    // materialized every pair into a Vec and rebuilt the whole plist — n new
    // conses + a Vec per added property, 1,269 times in the type-sim window.
    *plist = plist_value_prepend_pair(*plist, key, value);
    true
}

/// Remove KEY from a property list IN PLACE, mirroring GNU `remove_properties`
/// (src/textprop.c). `text-properties-at` returns the live interval plist, so a
/// captured snapshot aliases these cons cells; rebuilding the plist (the old
/// behavior) left that snapshot stale. Head matches advance the plist pointer
/// without mutating the old head cell; body matches are spliced out via
/// `set_cdr`. Allocates nothing.
fn plist_value_remove(plist: &mut Value, key: Value) -> bool {
    let mut changed = false;

    // Phase 1: strip matching pairs at the head (GNU advances
    // current_plist = XCDR(XCDR(current_plist)) without mutating the old head).
    let mut current = *plist;
    while current.is_cons() {
        let rest = current.cons_cdr();
        if !rest.is_cons() {
            break;
        }
        if eq_value(&current.cons_car(), &key) {
            current = rest.cons_cdr();
            changed = true;
        } else {
            break;
        }
    }

    // Phase 2: splice out body matches in place
    // (GNU `Fsetcdr (XCDR (tail2), XCDR (XCDR (this)))`).
    let mut tail = current;
    while tail.is_cons() {
        let val_cell = tail.cons_cdr(); // XCDR(tail2)
        if !val_cell.is_cons() {
            break;
        }
        let this = val_cell.cons_cdr(); // next key cell
        if this.is_cons() && eq_value(&this.cons_car(), &key) {
            let this_val = this.cons_cdr();
            if this_val.is_cons() {
                val_cell.set_cdr(this_val.cons_cdr());
                changed = true;
                continue; // stay on the same tail; another match may follow
            }
        }
        tail = this;
    }

    if changed {
        *plist = current;
    }
    changed
}

// ---------------------------------------------------------------------------
// IntervalNode — GNU-shaped internal interval tree node
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IntervalId(usize);

/// Internal interval node.
///
/// This mirrors GNU `struct interval`: each node stores subtree
/// `total_length`, a cached `position`, parent/left/right links, cached bits,
/// and the Lisp plist.  A node's own length is derived from `total_length`
/// minus the left and right subtree lengths, just like GNU's `LENGTH` macro.
#[derive(Clone, Debug)]
struct IntervalNode {
    total_length: CharLen,
    position: CharPos0,
    left: Option<IntervalId>,
    right: Option<IntervalId>,
    parent: Option<IntervalId>,
    front_sticky: bool,
    rear_sticky: bool,
    write_protect: bool,
    visible: bool,
    /// Plist contains a `syntax-table` or `category` key (cached like the
    /// bits above): the syntax scanner's run coalescing races through
    /// bit-clear intervals with one bool compare instead of plist walks.
    has_syntax_prop: bool,
    plist: IntervalPlist,
}

impl IntervalNode {
    fn new(length: CharLen, position: CharPos0, plist: IntervalPlist) -> Self {
        let (front_sticky, rear_sticky, write_protect, visible, has_syntax_prop) =
            Self::extract_cached(plist);
        Self {
            total_length: length,
            position,
            left: None,
            right: None,
            parent: None,
            front_sticky,
            rear_sticky,
            write_protect,
            visible,
            has_syntax_prop,
            plist,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn with_cached(
        length: CharLen,
        position: CharPos0,
        front_sticky: bool,
        rear_sticky: bool,
        write_protect: bool,
        visible: bool,
        has_syntax_prop: bool,
        plist: IntervalPlist,
    ) -> Self {
        Self {
            total_length: length,
            position,
            left: None,
            right: None,
            parent: None,
            front_sticky,
            rear_sticky,
            write_protect,
            visible,
            has_syntax_prop,
            plist,
        }
    }

    fn is_empty_plist(&self) -> bool {
        self.plist.is_nil()
    }

    /// Extract cached booleans from a plist (mirrors GNU's cache bits).
    fn extract_cached(plist: Value) -> (bool, bool, bool, bool, bool) {
        // A direct cons walk comparing interned ids: this runs on every
        // interval plist refresh (1,674 times in the type-sim window), and the
        // previous Vec-of-pairs + symbol-NAME string matches were a measurable
        // allocation + compare tax for five boolean flags.
        let ids = cached_plist_flag_symbols();
        let mut front_sticky = false;
        let mut rear_sticky = false;
        let mut write_protect = false;
        let mut visible = true;
        let mut has_syntax_prop = false;
        let mut tail = plist;
        while tail.is_cons() {
            let key = tail.cons_car();
            let rest = tail.cons_cdr();
            if !rest.is_cons() {
                break;
            }
            let value = rest.cons_car();
            if let Some(id) = key.as_symbol_id() {
                if id == ids.front_sticky {
                    front_sticky = value.is_truthy();
                } else if id == ids.rear_nonsticky {
                    rear_sticky = !value.is_truthy();
                } else if id == ids.read_only {
                    write_protect = value.is_truthy();
                } else if id == ids.invisible {
                    visible = !value.is_truthy();
                } else if id == ids.syntax_table || id == ids.category {
                    has_syntax_prop = true;
                }
            }
            tail = rest.cons_cdr();
        }
        (
            front_sticky,
            rear_sticky,
            write_protect,
            visible,
            has_syntax_prop,
        )
    }

    /// Re-extract and update cached booleans from current plist.
    fn refresh_cache(&mut self) {
        let (fs, rs, wp, vis, hsp) = Self::extract_cached(self.plist);
        self.has_syntax_prop = hsp;
        self.front_sticky = fs;
        self.rear_sticky = rs;
        self.write_protect = wp;
        self.visible = vis;
    }
}

#[derive(Clone, Debug)]
struct IntervalRun {
    range: CharRange,
    front_sticky: bool,
    rear_sticky: bool,
    write_protect: bool,
    visible: bool,
    has_syntax_prop: bool,
    plist: IntervalPlist,
}

impl IntervalRun {
    fn default_in_char_range(range: CharRange) -> Self {
        Self::new_in_char_range(range, Value::NIL)
    }

    fn new_in_char_range(range: CharRange, plist: IntervalPlist) -> Self {
        let (front_sticky, rear_sticky, write_protect, visible, has_syntax_prop) =
            IntervalNode::extract_cached(plist);
        Self {
            range,
            front_sticky,
            rear_sticky,
            write_protect,
            visible,
            has_syntax_prop,
            plist,
        }
    }

    fn from_node(start: CharPos0, node: &IntervalNode, length: CharLen) -> Self {
        Self {
            range: CharRange::from_start_len(start, length),
            front_sticky: node.front_sticky,
            rear_sticky: node.rear_sticky,
            write_protect: node.write_protect,
            visible: node.visible,
            has_syntax_prop: node.has_syntax_prop,
            plist: node.plist,
        }
    }

    fn start(&self) -> CharPos0 {
        self.range.start()
    }

    fn end(&self) -> CharPos0 {
        self.range.end()
    }

    fn set_start(&mut self, start: CharPos0) {
        self.range = CharRange::new(start, self.end());
    }

    fn set_end(&mut self, end: CharPos0) {
        self.range = CharRange::new(self.start(), end);
    }

    fn shift_by(&mut self, offset: CharLen) {
        self.range = CharRange::new(self.start().add_len(offset), self.end().add_len(offset));
    }

    fn len(&self) -> CharLen {
        self.range.len()
    }

    fn is_empty_plist(&self) -> bool {
        self.plist.is_nil()
    }

    fn refresh_cache(&mut self) {
        let (fs, rs, wp, vis, hsp) = IntervalNode::extract_cached(self.plist);
        self.has_syntax_prop = hsp;
        self.front_sticky = fs;
        self.rear_sticky = rs;
        self.write_protect = wp;
        self.visible = vis;
    }

    fn to_node(&self) -> IntervalNode {
        IntervalNode::with_cached(
            self.len(),
            self.range.start(),
            self.front_sticky,
            self.rear_sticky,
            self.write_protect,
            self.visible,
            self.has_syntax_prop,
            self.plist,
        )
    }
}

#[derive(Debug, Default)]
struct IntervalTree {
    root: Option<IntervalId>,
    nodes: Vec<IntervalNode>,
    /// Positional last-descent memo for `find_id`: `(start, end, id)` of the most
    /// recently located interval, tagged with the tree version it was valid for.
    /// A lookup that lands inside `[start, end)` returns in O(1) instead of
    /// re-descending the order-statistic tree O(log n). font-lock reads text
    /// properties at adjacent positions, so consecutive lookups overwhelmingly hit
    /// the same interval. GNU's `find_interval` re-descends from the root on every
    /// call, so this beats it.
    ///
    /// Stored as plain atomics (not a `Cell`) because `TextPropertyTable` must be
    /// `Sync` -- it backs a shared `OnceLock` sentinel. `version` is bumped by the
    /// three in-place structural/positional mutators (`push_node`,
    /// `add_length_to_ancestors`, `delete_node`); a lookup trusts the memo only
    /// when `cache_gen == gen`, so any such mutation invalidates it. Plist-value
    /// changes (`set_node_plist`) deliberately do NOT bump `version`: they move no
    /// interval boundary, the `pos -> interval` mapping is unchanged, and the
    /// caller reads the node's plist fresh via the returned id. Access is
    /// cooperatively single-threaded, so `Relaxed` ordering suffices and the memo
    /// cannot tear. `find_id_uncached` (test-only) plus a randomized differential
    /// test prove the memo never disagrees with a fresh descent.
    version: AtomicU64,
    cache_gen: MemoGeneration,
    cache_start: AtomicUsize,
    cache_end: AtomicUsize,
    cache_id: AtomicUsize,
    /// In-order-last node memo for `prune_trailing_empty_intervals`, keyed by
    /// `version` exactly like the find memo (see `rightmost_id_memo`).
    rightmost_gen: MemoGeneration,
    rightmost_id_cache: AtomicUsize,
}

/// Generation tag pairing a memo with the tree version it was computed for.
///
/// A newtype with its own `Default` so that "no memo yet" is the value a
/// container gets for free. `IntervalTree` previously derived `Default`, which
/// produced `cache_gen == 0` -- equal to the starting `version` -- so a
/// brand-new tree reported its memo VALID while `cache_id` still pointed at a
/// node that did not exist. The containment check masked it (`0 <= pos && pos
/// < 0` never holds) until the sequential finger's `pos == cache_end` arm,
/// which IS satisfied at position 0, indexed an empty node vec.
///
/// Encoding the empty state here rather than in a hand-written `Default` means
/// the derive is correct again: adding a field cannot silently reintroduce the
/// bug, and every construction path gets the sentinel without having to know
/// about it.
#[derive(Debug)]
struct MemoGeneration(AtomicU64);

impl MemoGeneration {
    /// No memo has been stored. Distinct from every real version, which start
    /// at 0 and only increment.
    const EMPTY: u64 = u64::MAX;

    #[inline]
    fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    #[inline]
    fn set(&self, version: u64) {
        self.0.store(version, Ordering::Relaxed);
    }
}

impl Default for MemoGeneration {
    fn default() -> Self {
        Self(AtomicU64::new(Self::EMPTY))
    }
}

impl Clone for IntervalTree {
    fn clone(&self) -> Self {
        // A clone starts with a fresh (empty) memo; it will populate its own.
        Self {
            root: self.root,
            nodes: self.nodes.clone(),
            version: AtomicU64::new(0),
            cache_gen: MemoGeneration::default(),
            cache_start: AtomicUsize::new(0),
            cache_end: AtomicUsize::new(0),
            cache_id: AtomicUsize::new(0),
            rightmost_gen: MemoGeneration::default(),
            rightmost_id_cache: AtomicUsize::new(0),
        }
    }
}

/// Forward in-order iterator over a tree's intervals (see [`IntervalTree::cursor_at`]).
/// Yields `(start, end, &node)` per interval; `start` is carried via `interval_end`
/// so no `find_id` re-descent (or stale per-node position) is ever needed after the
/// initial seat.
struct IntervalCursor<'a> {
    tree: &'a IntervalTree,
    next: Option<(CharPos0, IntervalId)>,
}

impl<'a> Iterator for IntervalCursor<'a> {
    type Item = (CharPos0, CharPos0, &'a IntervalNode);

    fn next(&mut self) -> Option<Self::Item> {
        let (start, id) = self.next?;
        let end = self.tree.interval_end(start, id);
        let node = &self.tree.nodes[id.0];
        self.next = self.tree.next_id(id).map(|next_id| (end, next_id));
        Some((start, end, node))
    }
}

/// Backward in-order iterator over a tree's intervals (see
/// [`IntervalTree::reverse_cursor_at`]). Yields `(start, end, &node)` for the
/// interval containing the seed position, then each preceding interval; the
/// predecessor's start is derived as `start - node_len(prev)`, never from the
/// dead per-node position.
struct ReverseIntervalCursor<'a> {
    tree: &'a IntervalTree,
    next: Option<(CharPos0, IntervalId)>,
}

impl<'a> Iterator for ReverseIntervalCursor<'a> {
    type Item = (CharPos0, CharPos0, &'a IntervalNode);

    fn next(&mut self) -> Option<Self::Item> {
        let (start, id) = self.next?;
        let end = self.tree.interval_end(start, id);
        let node = &self.tree.nodes[id.0];
        self.next = self.tree.prev_id(id).map(|prev_id| {
            let prev_start = start.saturating_sub_len(self.tree.node_len(prev_id));
            (prev_start, prev_id)
        });
        Some((start, end, node))
    }
}

impl IntervalTree {
    fn new() -> Self {
        Self::default()
    }

    fn from_runs(runs: Vec<IntervalRun>) -> Self {
        let runs = Self::normalize_runs(runs);
        Self::from_normalized_runs(runs)
    }

    fn from_runs_preserving_shape(runs: Vec<IntervalRun>) -> Self {
        let runs = Self::normalize_runs_preserving_shape(runs);
        Self::from_normalized_runs(runs)
    }

    fn from_normalized_runs(runs: Vec<IntervalRun>) -> Self {
        let mut tree = Self {
            root: None,
            nodes: Vec::with_capacity(runs.len()),
            ..Self::default()
        };
        tree.root = tree.build_balanced(&runs, 0, runs.len(), None);
        tree
    }

    fn normalize_runs(mut runs: Vec<IntervalRun>) -> Vec<IntervalRun> {
        Self::normalize_runs_impl(&mut runs, false)
    }

    fn normalize_runs_preserving_shape(mut runs: Vec<IntervalRun>) -> Vec<IntervalRun> {
        Self::normalize_runs_impl(&mut runs, true)
    }

    fn normalize_runs_impl(runs: &mut Vec<IntervalRun>, preserve_empty: bool) -> Vec<IntervalRun> {
        runs.retain(|run| run.start() < run.end());
        runs.sort_by_key(|run| run.start());

        let mut normalized = Vec::new();
        let mut cursor = CharPos0::ZERO;
        for mut run in runs.drain(..) {
            if run.end() <= cursor {
                continue;
            }
            if run.start() < cursor {
                run.set_start(cursor);
            }
            if cursor < run.start() {
                normalized.push(IntervalRun::default_in_char_range(CharRange::new(
                    cursor,
                    run.start(),
                )));
            }
            cursor = run.end();
            normalized.push(run);
        }

        if normalized.iter().all(IntervalRun::is_empty_plist) {
            return Vec::new();
        }
        if !preserve_empty {
            while normalized.last().is_some_and(IntervalRun::is_empty_plist) {
                normalized.pop();
            }
        }
        normalized
    }

    fn build_balanced(
        &mut self,
        runs: &[IntervalRun],
        start: usize,
        end: usize,
        parent: Option<IntervalId>,
    ) -> Option<IntervalId> {
        if start >= end {
            return None;
        }

        let mid = start + (end - start) / 2;
        let id = IntervalId(self.nodes.len());
        self.nodes.push(runs[mid].to_node());

        let left = self.build_balanced(runs, start, mid, Some(id));
        let right = self.build_balanced(runs, mid + 1, end, Some(id));

        let left_len = self.subtree_len(left);
        let right_len = self.subtree_len(right);
        let node_len = runs[mid].len().get();
        let node = &mut self.nodes[id.0];
        node.left = left;
        node.right = right;
        node.parent = parent;
        node.total_length = CharLen::new(left_len.get() + node_len + right_len.get());
        node.position = runs[mid].start();

        Some(id)
    }

    fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    fn len(&self) -> CharLen {
        self.root
            .map(|root| self.nodes[root.0].total_length)
            .unwrap_or(CharLen::ZERO)
    }

    fn subtree_len(&self, id: Option<IntervalId>) -> CharLen {
        id.map(|id| self.nodes[id.0].total_length)
            .unwrap_or(CharLen::ZERO)
    }

    fn node_len(&self, id: IntervalId) -> CharLen {
        let node = &self.nodes[id.0];
        CharLen::new(
            node.total_length.get()
                - self.subtree_len(node.left).get()
                - self.subtree_len(node.right).get(),
        )
    }

    fn push_node(&mut self, node: IntervalNode) -> IntervalId {
        self.invalidate_find_cache();
        let id = IntervalId(self.nodes.len());
        self.nodes.push(node);
        id
    }

    fn is_left_child(&self, id: IntervalId) -> bool {
        self.nodes[id.0]
            .parent
            .and_then(|parent| self.nodes[parent.0].left)
            == Some(id)
    }

    fn leftmost_id(&self, mut id: IntervalId) -> IntervalId {
        while let Some(left) = self.nodes[id.0].left {
            id = left;
        }
        id
    }

    /// In-order predecessor of `id` -- the mirror of [`IntervalTree::next_id`]
    /// (rightmost of the left subtree, else climb to the first right-child
    /// ancestor). O(1) amortized over a full reverse walk.
    fn prev_id(&self, id: IntervalId) -> Option<IntervalId> {
        if let Some(left) = self.nodes[id.0].left {
            return Some(self.rightmost_id(left));
        }

        let mut child = id;
        while let Some(parent) = self.nodes[child.0].parent {
            if self.nodes[parent.0].right == Some(child) {
                return Some(parent);
            }
            child = parent;
        }
        None
    }

    fn rightmost_id(&self, mut id: IntervalId) -> IntervalId {
        while let Some(right) = self.nodes[id.0].right {
            id = right;
        }
        id
    }

    fn next_id(&self, id: IntervalId) -> Option<IntervalId> {
        if let Some(right) = self.nodes[id.0].right {
            return Some(self.leftmost_id(right));
        }

        let mut child = id;
        while let Some(parent) = self.nodes[child.0].parent {
            if self.nodes[parent.0].left == Some(child) {
                return Some(parent);
            }
            child = parent;
        }
        None
    }

    /// A forward in-order walk over intervals, seated at the interval containing
    /// `pos` (or empty when `pos` is past the last interval). Mirrors GNU's
    /// `find_interval` + `next_interval` access pattern: ONE `find_id` descent to
    /// seat, then O(1) amortized `next_id` steps, with each interval's start
    /// carried forward via `interval_end` (contiguous partition). Property scans
    /// use this instead of re-descending the tree (`find_id`) per boundary, which
    /// was the O(N log n) hotspot. Encapsulating the walk in an iterator is also
    /// the misuse guard: callers get correct `(start, end)` for free and cannot
    /// accidentally re-search or read the stale per-node position.
    fn cursor_at(&self, pos: CharPos0) -> IntervalCursor<'_> {
        IntervalCursor {
            tree: self,
            next: self.find_id(pos),
        }
    }

    /// Seat a [`ReverseIntervalCursor`] at an explicit `(start, id)` so callers
    /// can begin the backward walk at a chosen interval (e.g. the one *before* a
    /// boundary, or the last interval when a position is past the tree end).
    fn reverse_cursor_from(
        &self,
        seat: Option<(CharPos0, IntervalId)>,
    ) -> ReverseIntervalCursor<'_> {
        ReverseIntervalCursor {
            tree: self,
            next: seat,
        }
    }

    /// `(start, id)` of the last interval, or None when the tree is empty.
    fn last_interval(&self) -> Option<(CharPos0, IntervalId)> {
        let root = self.root?;
        let id = self.rightmost_id(root);
        let end = CharPos0::ZERO.add_len(self.len());
        Some((end.saturating_sub_len(self.node_len(id)), id))
    }

    fn interval_end(&self, start: CharPos0, id: IntervalId) -> CharPos0 {
        start.add_len(self.node_len(id))
    }

    fn add_length_to_ancestors(&mut self, mut id: Option<IntervalId>, delta: isize) {
        self.invalidate_find_cache();
        while let Some(current) = id {
            let total = &mut self.nodes[current.0].total_length;
            if delta >= 0 {
                *total = CharLen::new(total.get() + delta as usize);
            } else {
                *total = CharLen::new(total.get() - (-delta) as usize);
            }
            id = self.nodes[current.0].parent;
        }
    }

    fn rotate_right(&mut self, a: IntervalId) -> IntervalId {
        let b = self.nodes[a.0]
            .left
            .expect("rotate_right requires a left child");
        let c = self.nodes[b.0].right;
        let a_parent = self.nodes[a.0].parent;
        let a_was_left = self.is_left_child(a);
        let old_total = self.nodes[a.0].total_length;
        let b_old_total = self.nodes[b.0].total_length.get();
        let c_total = self.subtree_len(c).get();

        if let Some(parent) = a_parent {
            if a_was_left {
                self.nodes[parent.0].left = Some(b);
            } else {
                self.nodes[parent.0].right = Some(b);
            }
        } else {
            self.root = Some(b);
        }
        self.nodes[b.0].parent = a_parent;

        self.nodes[b.0].right = Some(a);
        self.nodes[a.0].parent = Some(b);

        self.nodes[a.0].left = c;
        if let Some(c) = c {
            self.nodes[c.0].parent = Some(a);
        }

        self.nodes[a.0].total_length =
            CharLen::new(self.nodes[a.0].total_length.get() - (b_old_total - c_total));
        self.nodes[b.0].total_length = old_total;
        b
    }

    fn rotate_left(&mut self, a: IntervalId) -> IntervalId {
        let b = self.nodes[a.0]
            .right
            .expect("rotate_left requires a right child");
        let c = self.nodes[b.0].left;
        let a_parent = self.nodes[a.0].parent;
        let a_was_left = self.is_left_child(a);
        let old_total = self.nodes[a.0].total_length;
        let b_old_total = self.nodes[b.0].total_length.get();
        let c_total = self.subtree_len(c).get();

        if let Some(parent) = a_parent {
            if a_was_left {
                self.nodes[parent.0].left = Some(b);
            } else {
                self.nodes[parent.0].right = Some(b);
            }
        } else {
            self.root = Some(b);
        }
        self.nodes[b.0].parent = a_parent;

        self.nodes[b.0].left = Some(a);
        self.nodes[a.0].parent = Some(b);

        self.nodes[a.0].right = c;
        if let Some(c) = c {
            self.nodes[c.0].parent = Some(a);
        }

        self.nodes[a.0].total_length =
            CharLen::new(self.nodes[a.0].total_length.get() - (b_old_total - c_total));
        self.nodes[b.0].total_length = old_total;
        b
    }

    fn balance_an_interval(&mut self, mut id: IntervalId) -> IntervalId {
        #[cfg(test)]
        INTERVAL_BALANCE_CALLS.with(|calls| calls.set(calls.get() + 1));
        loop {
            let left_len = self.subtree_len(self.nodes[id.0].left).get() as isize;
            let right_len = self.subtree_len(self.nodes[id.0].right).get() as isize;
            let old_diff = left_len - right_len;
            if old_diff > 0 {
                let left = self.nodes[id.0]
                    .left
                    .expect("positive left/right diff requires left child");
                let new_diff = self.nodes[id.0].total_length.get() as isize
                    - self.nodes[left.0].total_length.get() as isize
                    + self.subtree_len(self.nodes[left.0].right).get() as isize
                    - self.subtree_len(self.nodes[left.0].left).get() as isize;
                if new_diff.abs() >= old_diff {
                    break;
                }
                id = self.rotate_right(id);
                if let Some(right) = self.nodes[id.0].right {
                    self.balance_an_interval(right);
                }
            } else if old_diff < 0 {
                let right = self.nodes[id.0]
                    .right
                    .expect("negative left/right diff requires right child");
                let new_diff = self.nodes[id.0].total_length.get() as isize
                    - self.nodes[right.0].total_length.get() as isize
                    + self.subtree_len(self.nodes[right.0].left).get() as isize
                    - self.subtree_len(self.nodes[right.0].right).get() as isize;
                if new_diff.abs() >= -old_diff {
                    break;
                }
                id = self.rotate_left(id);
                if let Some(left) = self.nodes[id.0].left {
                    self.balance_an_interval(left);
                }
            } else {
                break;
            }
        }
        id
    }

    fn balance_upwards(&mut self, mut id: Option<IntervalId>) {
        while let Some(current) = id {
            let balanced = self.balance_an_interval(current);
            id = self.nodes[balanced.0].parent;
            if id.is_none() {
                self.root = Some(balanced);
            }
        }
    }

    fn append_default_interval(&mut self, range: CharRange) {
        if range.is_empty() {
            return;
        }

        if self.root.is_none() {
            let root = self.push_node(IntervalNode::new(range.len(), range.start(), Value::NIL));
            self.root = Some(root);
            return;
        }

        let rightmost = self.rightmost_id(self.root.expect("root checked above"));
        let id = self.push_node(IntervalNode::new(range.len(), range.start(), Value::NIL));
        self.nodes[id.0].parent = Some(rightmost);
        self.nodes[rightmost.0].right = Some(id);
        self.add_length_to_ancestors(Some(rightmost), range.len().get() as isize);
        self.balance_upwards(Some(rightmost));
    }

    fn ensure_cover(&mut self, end: CharPos0) {
        let current_end = CharPos0::ZERO.add_len(self.len());
        if current_end < end {
            self.append_default_interval(CharRange::new(current_end, end));
        }
    }

    fn insert_default_at(&mut self, pos: CharPos0, len: CharLen) {
        if len.is_empty() || self.root.is_none() {
            return;
        }
        let tree_len = CharPos0::ZERO.add_len(self.len());
        // A gap before the insertion (pos past the tree) is rare and not on the
        // buffer-insert hot path; keep the simple rebuild for it.
        if pos > tree_len {
            return self.insert_default_at_slow(pos, len);
        }
        if pos == tree_len {
            // Append a fresh property-free interval at the very end.
            self.insert_default_node_before(None, len);
            return;
        }
        let Some((start, id)) = self.find_id(pos) else {
            return;
        };
        if pos == start {
            // At an interval boundary: splice a default interval in front of it.
            self.insert_default_node_before(Some(id), len);
        } else if self.nodes[id.0].is_empty_plist() {
            // Inside an interval with no properties: stretch it (and shift
            // everything after) by `len` -- O(log n), no new node.
            self.add_length_to_ancestors(Some(id), len.get() as isize);
        } else {
            // Inside a property-bearing interval: split it at `pos` and splice a
            // default interval between the halves, so the inserted text does not
            // inherit the surrounding properties (mirrors GNU
            // `adjust_intervals_for_insertion`).
            let Some(right) = self.split_at(pos) else {
                return;
            };
            self.insert_default_node_before(Some(right), len);
        }
    }

    /// Splice a fresh, property-free interval of `len` chars into the tree as
    /// the in-order predecessor of `before` (or at the very end when `None`),
    /// shifting every later interval by `len`.  Local: O(log n), no rebuild.
    fn insert_default_node_before(&mut self, before: Option<IntervalId>, len: CharLen) {
        let d_id = self.push_node(IntervalNode::new(len, CharPos0::ZERO, Value::NIL));
        match before {
            Some(n) => {
                if let Some(left) = self.nodes[n.0].left {
                    let mut cur = left;
                    while let Some(right) = self.nodes[cur.0].right {
                        cur = right;
                    }
                    self.nodes[cur.0].right = Some(d_id);
                    self.nodes[d_id.0].parent = Some(cur);
                } else {
                    self.nodes[n.0].left = Some(d_id);
                    self.nodes[d_id.0].parent = Some(n);
                }
            }
            None => {
                let mut cur = self.root.expect("non-empty tree");
                while let Some(right) = self.nodes[cur.0].right {
                    cur = right;
                }
                self.nodes[cur.0].right = Some(d_id);
                self.nodes[d_id.0].parent = Some(cur);
            }
        }
        let parent = self.nodes[d_id.0].parent;
        self.add_length_to_ancestors(parent, len.get() as isize);
        self.balance_upwards(parent);
    }

    fn insert_default_at_slow(&mut self, pos: CharPos0, len: CharLen) {
        if len.is_empty() || self.root.is_none() {
            return;
        }

        let mut runs = self.runs();
        let tree_len = CharPos0::ZERO.add_len(self.len());
        if pos >= tree_len {
            if tree_len < pos {
                runs.push(IntervalRun::default_in_char_range(CharRange::new(
                    tree_len, pos,
                )));
            }
            runs.push(IntervalRun::default_in_char_range(
                CharRange::from_start_len(pos, len),
            ));
            *self = Self::from_runs_preserving_shape(runs);
            return;
        }

        let mut adjusted = Vec::with_capacity(runs.len() + 2);
        let mut inserted = false;

        for run in runs {
            if run.end() <= pos {
                adjusted.push(run);
                continue;
            }

            if run.start() >= pos {
                if !inserted {
                    adjusted.push(IntervalRun::default_in_char_range(
                        CharRange::from_start_len(pos, len),
                    ));
                    let mut shifted = run;
                    shifted.shift_by(len);
                    adjusted.push(shifted);
                    inserted = true;
                } else {
                    let mut shifted = run;
                    shifted.shift_by(len);
                    adjusted.push(shifted);
                }
                continue;
            }

            if run.is_empty_plist() {
                let mut extended = run;
                extended.set_end(extended.end().add_len(len));
                adjusted.push(extended);
            } else {
                let right_plist = plist_value_from_pairs(&plist_pairs(run.plist));
                let mut left = run.clone();
                left.set_end(pos);
                let inserted_run =
                    IntervalRun::default_in_char_range(CharRange::from_start_len(pos, len));
                let mut right = run;
                right.set_start(pos.add_len(len));
                right.set_end(right.end().add_len(len));
                right.plist = right_plist;
                right.refresh_cache();
                adjusted.push(left);
                adjusted.push(inserted_run);
                adjusted.push(right);
            }
            inserted = true;
        }

        *self = Self::from_runs_preserving_shape(adjusted);
    }

    fn split_at(&mut self, pos: CharPos0) -> Option<IntervalId> {
        if pos == CharPos0::ZERO {
            return self.find_id(CharPos0::ZERO).map(|(_, id)| id);
        }

        let (start, id) = self.find_id(pos)?;
        let len = self.node_len(id);
        if pos == start {
            return Some(id);
        }
        if pos >= start.add_len(len) {
            return self.next_id(id);
        }

        let offset = pos.saturating_offset_from(start);
        let new_len = len.saturating_sub(offset);
        let old_right = self.nodes[id.0].right;
        let plist = plist_value_from_pairs(&plist_pairs(self.nodes[id.0].plist));
        let mut new = IntervalNode::with_cached(
            new_len,
            pos,
            self.nodes[id.0].front_sticky,
            self.nodes[id.0].rear_sticky,
            self.nodes[id.0].write_protect,
            self.nodes[id.0].visible,
            self.nodes[id.0].has_syntax_prop,
            plist,
        );
        new.parent = Some(id);
        new.right = old_right;
        new.total_length = CharLen::new(new.total_length.get() + self.subtree_len(old_right).get());
        let new_id = self.push_node(new);

        if let Some(old_right) = old_right {
            self.nodes[old_right.0].parent = Some(new_id);
        }
        self.nodes[id.0].right = Some(new_id);
        self.balance_upwards(Some(id));
        self.find_id(pos).map(|(_, id)| id)
    }

    /// Split the interval `id` (which starts at `node_start`) at `pos`, strictly
    /// inside it, WITHOUT rebalancing: the new right part hangs as `id`'s right
    /// child above `id`'s former right subtree, so `id`'s `total_length` and
    /// every ancestor's are unchanged and no upward walk is needed.  GNU
    /// `split_interval_left`, which likewise leaves balancing to the caller.
    /// `right` supplies the new node's plist and cached flags; its length and
    /// links are overwritten here.  Returns the new right node.
    fn split_node_right_raw(
        &mut self,
        id: IntervalId,
        node_start: CharPos0,
        pos: CharPos0,
        mut right: IntervalNode,
    ) -> IntervalId {
        let len = self.node_len(id);
        let offset = pos.saturating_offset_from(node_start);
        debug_assert!(!offset.is_empty() && offset < len);
        let new_len = len.saturating_sub(offset);
        let old_right = self.nodes[id.0].right;
        right.position = pos;
        right.parent = Some(id);
        right.left = None;
        right.right = old_right;
        right.total_length = CharLen::new(new_len.get() + self.subtree_len(old_right).get());
        let new_id = self.push_node(right);
        if let Some(old_right) = old_right {
            self.nodes[old_right.0].parent = Some(new_id);
        }
        self.nodes[id.0].right = Some(new_id);
        new_id
    }

    /// A detached node carrying `id`'s cached flags and `plist` (the shape a
    /// raw split hands to the remainder that keeps its plist object).
    fn node_shaped_like(&self, id: IntervalId, plist: IntervalPlist) -> IntervalNode {
        let node = &self.nodes[id.0];
        IntervalNode::with_cached(
            CharLen::ZERO,
            CharPos0::ZERO,
            node.front_sticky,
            node.rear_sticky,
            node.write_protect,
            node.visible,
            node.has_syntax_prop,
            plist,
        )
    }

    /// `copy_properties (over, this)`: node `id` takes `plist` (already a fresh
    /// copy of `run`'s) and `run`'s cached flags, which depend only on the
    /// plist's content.
    fn set_node_plist_from_run(&mut self, id: IntervalId, run: &IntervalRun, plist: IntervalPlist) {
        let node = &mut self.nodes[id.0];
        node.plist = plist;
        node.front_sticky = run.front_sticky;
        node.rear_sticky = run.rear_sticky;
        node.write_protect = run.write_protect;
        node.visible = run.visible;
        node.has_syntax_prop = run.has_syntax_prop;
    }

    /// GNU `graft_intervals_into_buffer` (src/intervals.c) over the contiguous
    /// source `runs` shifted by `offset`; see
    /// `TextPropertyTable::append_shifted_raw` for the contract.
    ///
    /// One descent finds `under`, the target interval at the graft start; if
    /// the graft begins inside it the unchanged left part is split off and
    /// re-homed onto a fresh plist (`copy_properties (under, end_unchanged)`)
    /// while the right part keeps the original plist object.  The runs are then
    /// walked against the target with the current interval tracked by id: a run
    /// ending inside the target interval splits it (`split_interval_left`,
    /// remainder keeps the plist object), a target interval ending at or before
    /// the run's end takes the run whole (its boundary survives) and the walk
    /// advances with `next_id`.  Every covered target interval receives a fresh
    /// copy of the run's plist.  Balancing happens once, climbing from the
    /// deepest node the splits created.
    fn graft_shifted_runs(&mut self, runs: &[IntervalRun], offset: CharLen) {
        let runs: Vec<&IntervalRun> = runs.iter().filter(|run| run.start() < run.end()).collect();
        let (Some(first), Some(last)) = (runs.first(), runs.last()) else {
            return;
        };
        let start = first.start().add_len(offset);
        let end = last.end().add_len(offset);
        self.ensure_cover(end);
        let Some((seat_start, mut id)) = self.find_id(start) else {
            return;
        };
        let mut node_start = seat_start;
        // Each node a raw split creates is the right child of the node split
        // -- the previous split's node while the walk stays on fresh nodes --
        // so consecutive splits form a right-spine hanging off the first
        // split node; it is relinked balanced whenever the walk moves on to an
        // existing node and at the end.
        let mut spine: Vec<IntervalId> = Vec::new();
        if seat_start < start {
            let original = self.nodes[id.0].plist;
            let right = self.node_shaped_like(id, original);
            let right_id = self.split_node_right_raw(id, node_start, start, right);
            if !original.is_nil() {
                // Same content, so the cached flags stay valid.
                self.nodes[id.0].plist = copy_plist_value(original);
            }
            id = right_id;
            node_start = start;
            spine.push(right_id);
        } else if start > CharPos0::ZERO {
            // Insertion at an interval boundary.  GNU's
            // `adjust_intervals_for_insertion` would have STRETCHED the
            // preceding (rear-sticky) interval over the inserted text, so its
            // graft splits that interval and `copy_properties (under,
            // end_unchanged)` re-homes the remainder onto a fresh plist; our
            // insert-adjust splices a default interval instead, so the same
            // re-home must happen here.  Without it a plist cons already
            // returned to Lisp by `text-properties-at' stayed attached to the
            // predecessor and a later in-place `put-text-property' (undo's
            // property-change entry) mutated the value Lisp held: oracle
            // cases div_cx19/40/43/46 read `(face nil)' where GNU keeps
            // `(face bold)'.
            self.rehome_predecessor_plist(start);
        }
        let mut cursor = start;
        'runs: for run in runs {
            let run_end = run.end().add_len(offset);
            debug_assert_eq!(run.start().add_len(offset), cursor);
            while cursor < run_end {
                let node_end = self.interval_end(node_start, id);
                let fresh = copy_plist_value(run.plist);
                if node_end > run_end {
                    let original = self.nodes[id.0].plist;
                    let right = self.node_shaped_like(id, original);
                    let right_id = self.split_node_right_raw(id, node_start, run_end, right);
                    self.set_node_plist_from_run(id, run, fresh);
                    id = right_id;
                    node_start = run_end;
                    cursor = run_end;
                    spine.push(right_id);
                } else {
                    self.set_node_plist_from_run(id, run, fresh);
                    cursor = node_end;
                    node_start = node_end;
                    if cursor < end {
                        // The next split will hang off an existing node, so
                        // the chain built so far ends here.
                        self.rebalance_spine(&spine);
                        spine.clear();
                        let Some(next) = self.next_id(id) else {
                            break 'runs;
                        };
                        id = next;
                    }
                }
            }
        }
        self.invalidate_find_cache();
        self.rebalance_spine(&spine);
    }

    /// Relink the right-spine `spine` (each node the right child of the one
    /// before it; the first hangs off its split parent, the last carries the
    /// original right subtree) into a balanced subtree in O(k), then climb
    /// from the anchor once.  Balancing the chain incrementally instead costs
    /// ~10 `balance_an_interval` calls per node (rotations cascade down a
    /// degenerate chain).
    fn rebalance_spine(&mut self, spine: &[IntervalId]) {
        let (Some(&first), Some(&last)) = (spine.first(), spine.last()) else {
            return;
        };
        let Some(anchor) = self.nodes[first.0].parent else {
            return;
        };
        debug_assert_eq!(self.nodes[anchor.0].right, Some(first));
        let tail = self.nodes[last.0].right;
        // Own lengths are derived from `total_length` minus the children's, so
        // capture them before any link changes.
        let lens: Vec<usize> = spine.iter().map(|&id| self.node_len(id).get()).collect();

        fn build(
            tree: &mut IntervalTree,
            spine: &[IntervalId],
            lens: &[usize],
            tail: Option<IntervalId>,
            lo: usize,
            hi: usize,
            parent: Option<IntervalId>,
        ) -> (Option<IntervalId>, usize) {
            if lo >= hi {
                return (None, 0);
            }
            let mid = lo + (hi - lo) / 2;
            let id = spine[mid];
            let (left, left_total) = build(tree, spine, lens, tail, lo, mid, Some(id));
            let (right, right_total) = if mid + 1 == spine.len() {
                if let Some(tail) = tail {
                    tree.nodes[tail.0].parent = Some(id);
                }
                (tail, tree.subtree_len(tail).get())
            } else {
                build(tree, spine, lens, tail, mid + 1, hi, Some(id))
            };
            let node = &mut tree.nodes[id.0];
            node.left = left;
            node.right = right;
            node.parent = parent;
            node.total_length = CharLen::new(lens[mid] + left_total + right_total);
            (Some(id), lens[mid] + left_total + right_total)
        }

        let (root, total) = build(self, spine, &lens, tail, 0, spine.len(), Some(anchor));
        debug_assert_eq!(
            total,
            lens.iter().sum::<usize>() + self.subtree_len(tail).get()
        );
        self.nodes[anchor.0].right = root;
        self.invalidate_find_cache();
        self.balance_upwards(Some(anchor));
    }

    #[cfg(test)]
    fn assert_invariants_for_test(&self) {
        fn walk(tree: &IntervalTree, id: IntervalId, parent: Option<IntervalId>) -> (usize, usize) {
            let node = &tree.nodes[id.0];
            assert_eq!(node.parent, parent, "parent link of node {}", id.0);
            let (left_total, left_depth) = node
                .left
                .map(|left| walk(tree, left, Some(id)))
                .unwrap_or((0, 0));
            let (right_total, right_depth) = node
                .right
                .map(|right| walk(tree, right, Some(id)))
                .unwrap_or((0, 0));
            assert!(
                node.total_length.get() >= left_total + right_total,
                "total_length of node {} smaller than its subtrees",
                id.0
            );
            (node.total_length.get(), 1 + left_depth.max(right_depth))
        }
        if let Some(root) = self.root {
            walk(self, root, None);
        }
    }

    #[cfg(test)]
    fn max_depth_for_test(&self) -> usize {
        fn depth(tree: &IntervalTree, id: Option<IntervalId>) -> usize {
            match id {
                None => 0,
                Some(id) => {
                    1 + depth(tree, tree.nodes[id.0].left).max(depth(tree, tree.nodes[id.0].right))
                }
            }
        }
        depth(self, self.root)
    }

    fn intervals_overlapping_after_splits(
        &mut self,
        range: CharRange,
    ) -> Vec<(CharPos0, IntervalId)> {
        if range.is_empty() {
            return Vec::new();
        }

        self.ensure_cover(range.end());
        self.split_at(range.start());
        self.split_at(range.end());

        self.ids_overlapping(range)
    }

    fn existing_intervals_overlapping_after_splits(
        &mut self,
        range: CharRange,
    ) -> Vec<(CharPos0, IntervalId)> {
        if range.is_empty() || self.root.is_none() {
            return Vec::new();
        }

        self.split_at(range.start());
        self.split_at(range.end());

        self.ids_overlapping(range)
    }

    fn set_node_plist(&mut self, id: IntervalId, plist: Value) {
        let node = &mut self.nodes[id.0];
        node.plist = plist;
        node.refresh_cache();
    }

    /// Re-home the interval ending exactly at `boundary` (the in-order
    /// predecessor of the interval starting at `boundary`) onto a fresh
    /// `copy-sequence` of its plist, mirroring GNU
    /// `graft_intervals_into_buffer`'s `copy_properties (under, end_unchanged)`.
    ///
    /// The boundary must already be a clean interval edge (the caller has split
    /// there).  This detaches the original plist cons -- which may alias a value
    /// already returned by `text-properties-at` -- from the buffer so a later
    /// in-place `put-text-property` cannot mutate the value held in Lisp.  Empty
    /// plists are left alone (GNU's `copy_properties` short-circuits on
    /// DEFAULT_INTERVAL_P).
    fn rehome_predecessor_plist(&mut self, boundary: CharPos0) {
        if boundary == CharPos0::ZERO {
            return;
        }
        let prev = CharPos0::new(boundary.get() - 1);
        let Some((_, id)) = self.find_id(prev) else {
            return;
        };
        let node = &self.nodes[id.0];
        if node.is_empty_plist() {
            return;
        }
        let fresh = copy_plist_value(node.plist);
        self.set_node_plist(id, fresh);
    }

    fn delete_node(&mut self, id: IntervalId) -> Option<IntervalId> {
        self.invalidate_find_cache();
        let left = self.nodes[id.0].left;
        let right = self.nodes[id.0].right;

        match (left, right) {
            (None, None) => None,
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (Some(migrate), Some(right)) => {
                let migrate_len = self.nodes[migrate.0].total_length;
                let mut cursor = right;
                self.nodes[cursor.0].total_length =
                    self.nodes[cursor.0].total_length.add_len(migrate_len);
                while let Some(left) = self.nodes[cursor.0].left {
                    cursor = left;
                    self.nodes[cursor.0].total_length =
                        self.nodes[cursor.0].total_length.add_len(migrate_len);
                }
                self.nodes[cursor.0].left = Some(migrate);
                self.nodes[migrate.0].parent = Some(cursor);
                Some(right)
            }
        }
    }

    /// Fuse every adjacent pair whose shared boundary lies in
    /// `[range.start, range.end]` and whose plists are `eq`-equal and non-nil,
    /// each right interval absorbed into its predecessor in place (GNU
    /// `merge_interval_left`).  Returns whether anything merged.
    ///
    /// Only the intervals whose boundary is inside `range` are visited: the
    /// one containing `range.start` (or the last interval when `range.start`
    /// is the tree end), its predecessor when `range.start` is itself a
    /// boundary, then successors while their boundary is at or before
    /// `range.end`.  A merge widens the left interval, so its new boundary is
    /// re-examined before moving on — the cascade the whole-buffer restart
    /// loop used to produce.
    fn merge_equal_neighbors_in(&mut self, range: CharRange) -> bool {
        if self.root.is_none() || self.len().is_empty() {
            return false;
        }
        let start = range.start();
        let end = range.end();
        let tree_end = CharPos0::ZERO.add_len(self.len());
        let located = if start < tree_end {
            self.find_id(start)
        } else {
            self.last_interval()
        };
        let Some((mut left_start, mut left)) = located else {
            return false;
        };
        let mut merged_any = false;
        // `start` on a boundary: the pair straddling it qualifies too.
        if left_start == start
            && let Some(predecessor) = self.prev_id(left)
            && self.plists_mergeable(predecessor, left)
        {
            let predecessor_start = left_start.saturating_sub_len(self.node_len(predecessor));
            self.merge_into_predecessor(predecessor, left);
            left = predecessor;
            left_start = predecessor_start;
            merged_any = true;
        }
        loop {
            let boundary = left_start.add_len(self.node_len(left));
            if boundary > end {
                break;
            }
            let Some(right) = self.next_id(left) else {
                break;
            };
            if self.plists_mergeable(left, right) {
                self.merge_into_predecessor(left, right);
                merged_any = true;
            } else {
                left_start = boundary;
                left = right;
            }
        }
        merged_any
    }

    /// The merge condition: a non-nil left plist `eq`-equal to the right one.
    /// Two property-free neighbours are deliberately left apart (the
    /// partition shape is preserved, as `normalize_runs_preserving_shape`
    /// preserves it).
    fn plists_mergeable(&self, left: IntervalId, right: IntervalId) -> bool {
        let left_plist = self.nodes[left.0].plist;
        if left_plist.is_nil() {
            return false;
        }
        plists_equal_eq(
            &plist_pairs(left_plist),
            &plist_pairs(self.nodes[right.0].plist),
        )
    }

    /// Absorb `right` into its in-order predecessor `left`: `left` grows by
    /// `right`'s length, `right` shrinks to zero and is unlinked.  Subtree
    /// totals stay consistent because both adjustments walk the ancestor
    /// chain; the unlink may shorten a spine, so balance is restored from the
    /// unlinked node's former parent.
    fn merge_into_predecessor(&mut self, left: IntervalId, right: IntervalId) {
        debug_assert_eq!(self.next_id(left), Some(right));
        let absorbed = self.node_len(right);
        let parent = self.nodes[right.0].parent;
        self.add_length_to_ancestors(Some(left), absorbed.get() as isize);
        self.add_length_to_ancestors(Some(right), -(absorbed.get() as isize));
        self.delete_zero_length_interval(right);
        self.balance_upwards(parent.or(self.root));
    }

    fn delete_zero_length_interval(&mut self, id: IntervalId) {
        debug_assert_eq!(self.node_len(id), CharLen::ZERO);
        let parent = self.nodes[id.0].parent;
        let replacement = self.delete_node(id);

        if let Some(parent) = parent {
            if self.nodes[parent.0].left == Some(id) {
                self.nodes[parent.0].left = replacement;
            } else {
                debug_assert_eq!(self.nodes[parent.0].right, Some(id));
                self.nodes[parent.0].right = replacement;
            }
            if let Some(replacement) = replacement {
                self.nodes[replacement.0].parent = Some(parent);
            }
        } else {
            self.root = replacement;
            if let Some(replacement) = replacement {
                self.nodes[replacement.0].parent = None;
            }
        }

        let node = &mut self.nodes[id.0];
        node.left = None;
        node.right = None;
        node.parent = None;
        node.total_length = CharLen::ZERO;
        node.plist = Value::NIL;
        node.refresh_cache();
    }

    fn interval_deletion_adjustment(
        &mut self,
        id: IntervalId,
        from: CharLen,
        amount: CharLen,
    ) -> CharLen {
        if amount.is_empty() {
            return CharLen::ZERO;
        }

        let left_len = self.subtree_len(self.nodes[id.0].left);
        if from < left_len {
            let left = self.nodes[id.0]
                .left
                .expect("left branch length requires a left child");
            let subtract = self.interval_deletion_adjustment(left, from, amount);
            self.nodes[id.0].total_length = self.nodes[id.0].total_length.saturating_sub(subtract);
            return subtract;
        }

        let total_len = self.nodes[id.0].total_length;
        let right_len = self.subtree_len(self.nodes[id.0].right);
        let right_start = total_len.saturating_sub(right_len);
        if from >= right_start {
            let right = self.nodes[id.0]
                .right
                .expect("right branch length requires a right child");
            let subtract =
                self.interval_deletion_adjustment(right, from.saturating_sub(right_start), amount);
            self.nodes[id.0].total_length = self.nodes[id.0].total_length.saturating_sub(subtract);
            return subtract;
        }

        let subtract = right_start.saturating_sub(from).min(amount);

        self.nodes[id.0].total_length = self.nodes[id.0].total_length.saturating_sub(subtract);
        if self.node_len(id).is_empty() {
            self.delete_zero_length_interval(id);
        }
        subtract
    }

    fn delete_range(&mut self, range: CharRange) {
        self.invalidate_find_cache();
        if range.is_empty() || self.root.is_none() {
            return;
        }
        let start = range.start().saturating_offset_from(CharPos0::ZERO);
        let end = range.end().saturating_offset_from(CharPos0::ZERO);

        let tree_len = self.len();
        if start >= tree_len {
            return;
        }

        let mut left_to_delete = end.min(tree_len).saturating_sub(start);
        if left_to_delete == tree_len {
            self.root = None;
            return;
        }

        while !left_to_delete.is_empty() {
            let Some(root) = self.root else {
                return;
            };
            if left_to_delete == self.nodes[root.0].total_length {
                self.root = None;
                return;
            }
            let deleted = self.interval_deletion_adjustment(root, start, left_to_delete);
            if deleted.is_empty() {
                break;
            }
            left_to_delete = left_to_delete.saturating_sub(deleted);
        }
    }

    fn merge_interval_left(&mut self, id: IntervalId) -> Option<IntervalId> {
        self.invalidate_find_cache();
        let absorb = self.node_len(id);
        if absorb.is_empty() {
            return None;
        }

        if let Some(left) = self.nodes[id.0].left {
            let mut predecessor = left;
            while let Some(right) = self.nodes[predecessor.0].right {
                self.nodes[predecessor.0].total_length =
                    self.nodes[predecessor.0].total_length.add_len(absorb);
                predecessor = right;
            }
            self.nodes[predecessor.0].total_length =
                self.nodes[predecessor.0].total_length.add_len(absorb);
            self.delete_zero_length_interval(id);
            return Some(predecessor);
        }

        self.nodes[id.0].total_length = self.nodes[id.0].total_length.saturating_sub(absorb);
        let mut predecessor = id;
        while let Some(parent) = self.nodes[predecessor.0].parent {
            if self.nodes[parent.0].right == Some(predecessor) {
                self.delete_zero_length_interval(id);
                return Some(parent);
            }
            predecessor = parent;
            self.nodes[predecessor.0].total_length = self.nodes[predecessor.0]
                .total_length
                .saturating_sub(absorb);
        }

        None
    }

    fn remove_rightmost_interval(&mut self) -> Option<CharLen> {
        let root = self.root?;
        let id = self.rightmost_id(root);
        let removed_len = self.node_len(id);
        let left = self.nodes[id.0].left;
        let parent = self.nodes[id.0].parent;

        if let Some(parent) = parent {
            debug_assert_eq!(self.nodes[parent.0].right, Some(id));
            self.nodes[parent.0].right = left;
            if let Some(left) = left {
                self.nodes[left.0].parent = Some(parent);
            }
            self.add_length_to_ancestors(Some(parent), -(removed_len.get() as isize));
            self.balance_upwards(Some(parent));
        } else {
            self.root = left;
            if let Some(left) = left {
                self.nodes[left.0].parent = None;
            }
            // The other branch bumps `version` through `add_length_to_ancestors`;
            // removing the root itself is just as structural.
            self.invalidate_find_cache();
        }

        Some(removed_len)
    }

    /// The in-order-last interval, memoized against the structural `version`.
    /// Rotations preserve in-order and node ids are arena-stable, so only an
    /// insertion or deletion — each of which bumps `version` — can change which
    /// node is rightmost. The per-put trailing prune (an insert can leave a
    /// default tail that a later put must drop) thus costs one node read in the
    /// steady state instead of a right-spine walk from the root on every put.
    fn rightmost_id_memo(&self, root: IntervalId) -> IntervalId {
        let version = self.version.load(Ordering::Relaxed);
        if self.rightmost_gen.get() == version {
            return IntervalId(self.rightmost_id_cache.load(Ordering::Relaxed));
        }
        let id = self.rightmost_id(root);
        self.rightmost_id_cache.store(id.0, Ordering::Relaxed);
        self.rightmost_gen.set(version);
        id
    }

    fn prune_trailing_empty_intervals(&mut self) {
        while let Some(root) = self.root {
            let rightmost = self.rightmost_id_memo(root);
            if !self.nodes[rightmost.0].is_empty_plist() {
                break;
            }
            self.remove_rightmost_interval();
        }
    }

    fn live_plists(&self, id: Option<IntervalId>, f: &mut impl FnMut(Value)) {
        let Some(id) = id else {
            return;
        };
        let node = &self.nodes[id.0];
        self.live_plists(node.left, f);
        f(node.plist);
        self.live_plists(node.right, f);
    }

    fn runs(&self) -> Vec<IntervalRun> {
        let mut runs = Vec::new();
        self.push_runs(self.root, CharPos0::ZERO, &mut runs);
        runs
    }

    fn push_runs(
        &self,
        id: Option<IntervalId>,
        base: CharPos0,
        runs: &mut Vec<IntervalRun>,
    ) -> CharPos0 {
        let Some(id) = id else {
            return base;
        };
        let node = &self.nodes[id.0];
        let after_left = self.push_runs(node.left, base, runs);
        let length = self.node_len(id);
        runs.push(IntervalRun::from_node(after_left, node, length));
        self.push_runs(node.right, after_left.add_len(length), runs)
    }

    fn find(&self, pos: CharPos0) -> Option<(CharPos0, &IntervalNode)> {
        let (start, id) = self.find_id(pos)?;
        Some((start, &self.nodes[id.0]))
    }

    fn find_id(&self, pos: CharPos0) -> Option<(CharPos0, IntervalId)> {
        // Positional last-descent memo (see the `version`/`cache_*` fields): a lookup
        // that lands inside the previously located interval, at the same tree
        // version, returns without re-descending.
        let version = self.version.load(Ordering::Relaxed);
        if self.cache_gen.get() == version {
            let start = CharPos0::new(self.cache_start.load(Ordering::Relaxed));
            let end = CharPos0::new(self.cache_end.load(Ordering::Relaxed));
            if start <= pos && pos < end {
                return Some((start, IntervalId(self.cache_id.load(Ordering::Relaxed))));
            }

            // Sequential-scan finger. The containment memo above cannot serve a
            // forward property scan: `SyntaxPropRange` caches whole RUNS, so it
            // only calls back when it has consumed one and wants the position
            // one PAST it -- never a position inside the cached interval. That
            // pattern lands here every time, which is why a font-lock scroll
            // profiled as almost pure root-descent (find_id 8.29% of self time,
            // its hot instructions all parent/child pointer chasing).
            //
            // Such a lookup is asking for the tree-order successor, reachable in
            // O(1) amortized via `next_id` instead of an O(log n) descent.
            // Intervals tile the text contiguously (the invariant the forward
            // walker in `intervals_from` already relies on), so the successor
            // begins exactly where the cached interval ended. Zero-length nodes
            // contain no position and are stepped over.
            if pos == end {
                let mut id = IntervalId(self.cache_id.load(Ordering::Relaxed));
                let mut start = end;
                while let Some(next) = self.next_id(id) {
                    let next_end = start.add_len(self.node_len(next));
                    if pos < next_end {
                        self.cache_start.store(start.get(), Ordering::Relaxed);
                        self.cache_end.store(next_end.get(), Ordering::Relaxed);
                        self.cache_id.store(next.0, Ordering::Relaxed);
                        self.cache_gen.set(version);
                        return Some((start, next));
                    }
                    id = next;
                    start = next_end;
                }
            }
        }
        let mut id = self.root?;
        let mut base = CharPos0::ZERO;
        loop {
            let node = &self.nodes[id.0];
            let left_len = self.subtree_len(node.left);
            let node_start = base.add_len(left_len);
            let node_len = self.node_len(id);
            if pos < node_start {
                id = node.left?;
            } else if pos < node_start.add_len(node_len) {
                let node_end = node_start.add_len(node_len);
                self.cache_start.store(node_start.get(), Ordering::Relaxed);
                self.cache_end.store(node_end.get(), Ordering::Relaxed);
                self.cache_id.store(id.0, Ordering::Relaxed);
                self.cache_gen.set(version);
                return Some((node_start, id));
            } else {
                base = node_start.add_len(node_len);
                id = node.right?;
            }
        }
    }

    /// Invalidate the `find_id` memo by advancing the tree version. Called by the
    /// three in-place structural/positional mutators.
    #[inline]
    fn invalidate_find_cache(&self) {
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    /// `find_id` without consulting or updating the memo -- the ground truth used
    /// by the differential test to prove the cache never goes stale.
    #[cfg(test)]
    fn find_id_uncached(&self, pos: CharPos0) -> Option<(CharPos0, IntervalId)> {
        let mut id = self.root?;
        let mut base = CharPos0::ZERO;
        loop {
            let node = &self.nodes[id.0];
            let left_len = self.subtree_len(node.left);
            let node_start = base.add_len(left_len);
            let node_len = self.node_len(id);
            if pos < node_start {
                id = node.left?;
            } else if pos < node_start.add_len(node_len) {
                return Some((node_start, id));
            } else {
                base = node_start.add_len(node_len);
                id = node.right?;
            }
        }
    }

    fn first_id_after(&self, current: CharPos0) -> Option<(CharPos0, IntervalId)> {
        let mut best = None;
        self.find_first_id_after(self.root, CharPos0::ZERO, current, &mut best);
        best
    }

    fn first_start_after(&self, current: CharPos0) -> Option<CharPos0> {
        self.first_id_after(current).map(|(start, _)| start)
    }

    fn first_id_overlapping(&self, range: CharRange) -> Option<(CharPos0, IntervalId)> {
        if range.is_empty() {
            return None;
        }
        if let Some((interval_start, id)) = self.find_id(range.start())
            && self.interval_end(interval_start, id) > range.start()
        {
            return Some((interval_start, id));
        }
        self.first_id_after(range.start())
            .filter(|(next_start, _)| *next_start < range.end())
    }

    fn ids_overlapping(&self, range: CharRange) -> Vec<(CharPos0, IntervalId)> {
        let mut result = Vec::new();
        let Some((mut node_start, mut id)) = self.first_id_overlapping(range) else {
            return result;
        };
        while node_start < range.end() {
            let node_end = self.interval_end(node_start, id);
            if node_end > range.start() {
                result.push((node_start, id));
            }
            let Some(next_id) = self.next_id(id) else {
                break;
            };
            node_start = node_end;
            id = next_id;
        }
        result
    }

    fn find_first_id_after(
        &self,
        id: Option<IntervalId>,
        base: CharPos0,
        current: CharPos0,
        best: &mut Option<(CharPos0, IntervalId)>,
    ) {
        let Some(id) = id else {
            return;
        };
        let node = &self.nodes[id.0];
        let left_len = self.subtree_len(node.left);
        let node_start = base.add_len(left_len);
        let node_len = self.node_len(id);
        if node_start > current {
            let replace = best.is_none_or(|(old_start, _)| node_start < old_start);
            if replace {
                *best = Some((node_start, id));
            }
            self.find_first_id_after(node.left, base, current, best);
        } else {
            self.find_first_id_after(node.right, node_start.add_len(node_len), current, best);
        }
    }
}

// ---------------------------------------------------------------------------
// Plist helpers
// ---------------------------------------------------------------------------

/// True when `left` and `right` agree (by `eq`) on every property in `keys`,
/// reading each plist straight off its interval-node `Value` spine (no `Vec`
/// materialization). An absent key reads as nil, so two plists differing only in
/// e.g. `face` compare equal for the watched display keys, and passing
/// `Value::NIL` as `right` tests whether `left`'s watched keys are all nil.
fn watched_keys_equal_eq_plist(left: Value, right: Value, keys: &[Value]) -> bool {
    keys.iter().all(|k| {
        let a = plist_value_get(left, *k).unwrap_or(Value::NIL);
        let b = plist_value_get(right, *k).unwrap_or(Value::NIL);
        eq_value(&a, &b)
    })
}

fn plists_equal_eq(left: &[(Value, Value)], right: &[(Value, Value)]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter().all(|(left_key, left_value)| {
        right.iter().any(|(right_key, right_value)| {
            eq_value(left_key, right_key) && eq_value(left_value, right_value)
        })
    })
}

fn plists_equal_values_equal(left: &[(Value, Value)], right: &[(Value, Value)]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter().all(|(left_key, left_value)| {
        right.iter().any(|(right_key, right_value)| {
            eq_value(left_key, right_key) && equal_value(left_value, right_value, 0)
        })
    })
}

// ---------------------------------------------------------------------------
// TextPropertyTable
// ---------------------------------------------------------------------------

/// How an ordered property plist is applied to an interval.
///
/// This makes GNU's two distinct ordering policies explicit at call sites,
/// instead of encoding them as an easy-to-reverse iterator convention.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PropertyPlistApplication {
    /// Apply pairs in plist order, as GNU `Fadd_text_properties` does.
    ///
    /// Each newly added pair is prepended, so a fresh destination observes the
    /// reverse of the supplied plist order.
    AddProperties,
    /// Apply pairs in reverse so the destination retains their supplied order.
    ///
    /// GNU `propertize` obtains this effect by reversing its plist before
    /// calling `add-text-properties`.
    PreserveSuppliedOrder,
}

/// One property-bearing string participating in a concatenation.
///
/// The offset is a character length, not an untyped byte/character integer:
/// interval storage and GNU's string-property positions are character based
/// even when the destination string is multibyte.
pub(crate) struct ShiftedTextPropertySource {
    table: TextPropertyTable,
    char_offset: CharLen,
}

impl ShiftedTextPropertySource {
    pub(crate) fn new(table: TextPropertyTable, char_offset: CharLen) -> Self {
        Self { table, char_offset }
    }
}

/// Conservative answer to whether a text-property name can occur in a table.
///
/// `DefinitelyAbsent` is proof and may be used to skip an interval-tree walk.
/// `PossiblyPresent` is not proof of presence: removals deliberately need not
/// recount the whole tree, so it can remain positive after the last occurrence
/// is removed.  Making the asymmetry an enum prevents a plain `bool` from
/// inviting callers to infer more than the index guarantees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum PropertyNamePresence {
    DefinitelyAbsent,
    PossiblyPresent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PropertyNameIdentity(usize);

impl From<Value> for PropertyNameIdentity {
    fn from(value: Value) -> Self {
        Self(value.bits())
    }
}

/// Property names known to have been assigned since the last exact rebuild.
///
/// Identities are raw tagged bits rather than `Value`s so a stale conservative
/// entry is not a GC root.  While a name is live in an interval its plist is the
/// real root; after removal, identity reuse can only cause a harmless false
/// positive (`PossiblyPresent`), never a false `DefinitelyAbsent`.
#[derive(Clone, Debug, Default)]
enum ConservativePropertyNames {
    /// The common no-properties case has no heap allocation.
    #[default]
    Empty,
    /// Snapshots share the summary in O(1); the next mutation detaches it.
    Assigned(Arc<FxHashSet<PropertyNameIdentity>>),
}

impl ConservativePropertyNames {
    fn from_runs(runs: &[IntervalRun]) -> Self {
        let mut names = Self::default();
        for run in runs {
            names.observe_plist(run.plist);
        }
        names
    }

    fn observe(&mut self, name: Value) {
        let identity = name.into();
        match self {
            Self::Empty => {
                let mut assigned = FxHashSet::default();
                assigned.insert(identity);
                *self = Self::Assigned(Arc::new(assigned));
            }
            Self::Assigned(assigned) => {
                Arc::make_mut(assigned).insert(identity);
            }
        }
    }

    fn observe_plist(&mut self, mut plist: Value) {
        while plist.is_cons() {
            let name = plist.cons_car();
            let rest = plist.cons_cdr();
            if !rest.is_cons() {
                break;
            }
            self.observe(name);
            plist = rest.cons_cdr();
        }
    }

    fn include(&mut self, other: &Self) {
        let Self::Assigned(other) = other else {
            return;
        };
        match self {
            Self::Empty => *self = Self::Assigned(Arc::clone(other)),
            Self::Assigned(assigned) if !Arc::ptr_eq(assigned, other) => {
                Arc::make_mut(assigned).extend(other.iter().copied());
            }
            Self::Assigned(_) => {}
        }
    }

    fn presence(&self, name: Value) -> PropertyNamePresence {
        let is_known = match self {
            Self::Empty => false,
            Self::Assigned(assigned) => assigned.contains(&name.into()),
        };
        if is_known {
            PropertyNamePresence::PossiblyPresent
        } else {
            PropertyNamePresence::DefinitelyAbsent
        }
    }
}

/// Effect a wholesale interval-tree replacement can have on property names.
///
/// Requiring this at the replacement boundary makes a mutation that can add
/// names declare its source.  Pure structural rewrites retain the shared
/// summary without an O(intervals) recount.
enum ReplacementPropertyNames<'a> {
    Preserve,
    Include(&'a ConservativePropertyNames),
}

/// Text-property interval storage.
///
/// The backing store is a GNU-shaped augmented interval tree.  Callers still
/// use absolute character positions; internally node starts are derived from
/// subtree lengths.
#[derive(Debug)]
pub struct TextPropertyTable {
    intervals: IntervalTree,
    property_names: ConservativePropertyNames,
    /// Bumped by every mutating entry point; run-caching scanners key their
    /// memos on it (together with the buffer content epoch) so a property
    /// change invalidates them. Monotonic, wraps never (u64).
    mutation_tick: u64,
    /// Bumped only by mutations that can change `syntax-table`/`category`
    /// presence or values: exact for by-name put/remove, conservative for
    /// bulk plist replacement, and NOT bumped by pure position adjusts
    /// (those always ride a content-epoch change). The syntax-run memo keys
    /// on this so font-lock's per-token `face` writes stop invalidating it.
    syntax_prop_tick: u64,
    /// Lazy "does ANY interval carry a syntax-relevant key?" memo, packed as
    /// `((syntax_prop_tick + 1) << 1) | any` — the +1 makes the default `0`
    /// never match, and the answer self-recomputes on first query after a
    /// syntax-relevant mutation. One full-tree bit scan per such mutation,
    /// zero per-mutation bookkeeping. Atomic (not Cell) only so the table
    /// stays `Sync` for the shared EMPTY static; ordering is Relaxed — a
    /// racing recompute is just repeated work.
    syntax_prop_any: std::sync::atomic::AtomicU64,
    /// Lazy sorted list of the bit-set intervals' `[start, end)` bounds,
    /// tagged with the `syntax_prop_tick + 1` it was built at (0 = never).
    /// Sparse in practice (elisp-mode marks ~21 docstring delimiters in a
    /// 50k-interval fontified buffer), so queries binary-search this instead
    /// of walking thousands of face intervals. Mutex only for `Sync` (the
    /// shared EMPTY static); the heap is per-thread, so it is uncontended —
    /// and a failed try_lock just falls back to the cursor walk.
    syntax_prop_ranges: std::sync::Mutex<(u64, Vec<(CharPos0, CharPos0)>)>,
}

/// Clip a cached-range endpoint for a deletion of `range` (length
/// `del_len`): positions past the deletion shift down, positions inside
/// clamp to its start.
fn clip_char_pos_for_delete(pos: CharPos0, range: CharRange, del_len: CharLen) -> CharPos0 {
    if pos >= range.end() {
        CharPos0::new(pos.get() - del_len.get())
    } else if pos > range.start() {
        range.start()
    } else {
        pos
    }
}

impl Clone for TextPropertyTable {
    fn clone(&self) -> Self {
        use std::sync::atomic::Ordering;
        Self {
            intervals: self.intervals.clone(),
            property_names: self.property_names.clone(),
            mutation_tick: self.mutation_tick,
            syntax_prop_tick: self.syntax_prop_tick,
            syntax_prop_any: std::sync::atomic::AtomicU64::new(
                self.syntax_prop_any.load(Ordering::Relaxed),
            ),
            syntax_prop_ranges: std::sync::Mutex::new(
                self.syntax_prop_ranges
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or((0, Vec::new())),
            ),
        }
    }
}

impl TextPropertyTable {
    pub fn new() -> Self {
        Self {
            intervals: IntervalTree::new(),
            property_names: ConservativePropertyNames::default(),
            mutation_tick: 0,
            syntax_prop_tick: 0,
            syntax_prop_any: std::sync::atomic::AtomicU64::new(0),
            syntax_prop_ranges: std::sync::Mutex::new((0, Vec::new())),
        }
    }

    /// Current mutation tick — see the field doc.
    pub fn mutation_tick(&self) -> u64 {
        self.mutation_tick
    }

    /// Current syntax-relevant mutation tick — see the field doc.
    pub fn syntax_prop_tick(&self) -> u64 {
        self.syntax_prop_tick
    }

    /// Whether a property NAME can influence syntax resolution.
    fn name_is_syntax_relevant(name: Value) -> bool {
        name.is_symbol_named("syntax-table") || name.is_symbol_named("category")
    }

    /// Return the conservative presence state for `name` without descending the
    /// interval tree.
    pub fn property_name_presence(&self, name: Value) -> PropertyNamePresence {
        self.property_names.presence(name)
    }

    /// Copy interval nodes and plist cons spines, while preserving plist
    /// values by reference.  GNU `copy_intervals` copies intervals and
    /// `copy_properties` copies `interval->plist` with `copy-sequence`.
    pub fn copy_interval_plist_spines(&self) -> Self {
        let mut runs = self.intervals.runs();
        for run in &mut runs {
            run.plist = plist_value_from_pairs(&plist_pairs(run.plist));
            run.refresh_cache();
        }
        Self::from_interval_runs_preserving_shape(runs)
    }

    // -- Query helpers -------------------------------------------------------

    /// Find the interval containing `pos`, returning its (start, node) pair.
    fn find_interval(&self, pos: CharPos0) -> Option<(CharPos0, &IntervalNode)> {
        self.intervals.find(pos)
    }

    /// Find the first stored interval that overlaps `[start, end)`.
    ///
    /// GNU interval users first locate the interval at START and then advance
    /// with `next_interval`; they don't scan from the beginning of the object.
    /// Keep that traversal shape centralized so callers follow GNU's
    /// interval-access path instead of depending on the tree representation.
    fn first_interval_overlapping(&self, range: CharRange) -> Option<(CharPos0, IntervalId)> {
        self.intervals.first_id_overlapping(range)
    }

    fn for_each_interval_overlapping(
        &self,
        range: CharRange,
        mut f: impl FnMut(usize, usize, &IntervalNode),
    ) {
        let Some((mut node_start, mut id)) = self.first_interval_overlapping(range) else {
            return;
        };
        loop {
            if node_start >= range.end() {
                break;
            }
            let node_end = self.intervals.interval_end(node_start, id);
            if node_end > range.start() {
                let node = &self.intervals.nodes[id.0];
                f(node_start.get(), node_end.get(), node);
            }
            let Some(next_id) = self.intervals.next_id(id) else {
                break;
            };
            node_start = node_end;
            id = next_id;
        }
    }

    fn any_interval_overlapping(
        &self,
        range: CharRange,
        mut predicate: impl FnMut(usize, &IntervalNode) -> bool,
    ) -> bool {
        let Some((mut node_start, mut id)) = self.first_interval_overlapping(range) else {
            return false;
        };
        loop {
            if node_start >= range.end() {
                return false;
            }
            let node_end = self.intervals.interval_end(node_start, id);
            let node = &self.intervals.nodes[id.0];
            if node_end > range.start() && predicate(node_start.get(), node) {
                return true;
            }
            let Some(next_id) = self.intervals.next_id(id) else {
                return false;
            };
            node_start = node_end;
            id = next_id;
        }
    }

    fn try_for_each_interval_overlapping<E>(
        &self,
        range: CharRange,
        mut f: impl FnMut(CharRange, &IntervalNode) -> Result<(), E>,
    ) -> Result<(), E> {
        let Some((mut node_start, mut id)) = self.first_interval_overlapping(range) else {
            return Ok(());
        };
        loop {
            if node_start >= range.end() {
                break;
            }
            let node_end = self.intervals.interval_end(node_start, id);
            if node_end > range.start() {
                let node = &self.intervals.nodes[id.0];
                f(CharRange::new(node_start, node_end), node)?;
            }
            let Some(next_id) = self.intervals.next_id(id) else {
                break;
            };
            node_start = node_end;
            id = next_id;
        }
        Ok(())
    }

    fn from_interval_runs(runs: Vec<IntervalRun>) -> Self {
        let property_names = ConservativePropertyNames::from_runs(&runs);
        Self {
            intervals: IntervalTree::from_runs(runs),
            property_names,
            mutation_tick: 0,
            syntax_prop_tick: 0,
            syntax_prop_any: std::sync::atomic::AtomicU64::new(0),
            syntax_prop_ranges: std::sync::Mutex::new((0, Vec::new())),
        }
    }

    fn from_interval_runs_preserving_shape(runs: Vec<IntervalRun>) -> Self {
        let property_names = ConservativePropertyNames::from_runs(&runs);
        Self::with_runs_and_names(runs, property_names)
    }

    /// A fresh table over `runs` (shape preserved) with an already-known
    /// conservative name summary.
    fn with_runs_and_names(
        runs: Vec<IntervalRun>,
        property_names: ConservativePropertyNames,
    ) -> Self {
        Self {
            intervals: IntervalTree::from_runs_preserving_shape(runs),
            property_names,
            mutation_tick: 0,
            syntax_prop_tick: 0,
            syntax_prop_any: std::sync::atomic::AtomicU64::new(0),
            syntax_prop_ranges: std::sync::Mutex::new((0, Vec::new())),
        }
    }

    /// Compose disjoint, shifted source tables into one balanced interval tree.
    ///
    /// GNU `concat_to_string` first owns the complete result and then applies
    /// each source interval through `add-text-properties`.  Replaying those
    /// writes against a growing tree is unnecessarily quadratic in NeoMacs:
    /// every property can split, search, and rebalance the interval tree.  This
    /// constructor performs the same ordered-plist transformation in memory,
    /// then builds the destination tree once from all known runs.
    pub(crate) fn from_shifted_sources(
        sources: Vec<ShiftedTextPropertySource>,
        application: PropertyPlistApplication,
    ) -> Self {
        fn applied_to_empty_destination(
            properties: Vec<(Value, Value)>,
            application: PropertyPlistApplication,
        ) -> Vec<(Value, Value)> {
            let mut destination = Vec::with_capacity(properties.len());
            let mut apply = |(name, value): (Value, Value)| {
                if let Some((_, existing)) = destination
                    .iter_mut()
                    .find(|(existing_name, _)| eq_value(existing_name, &name))
                {
                    *existing = value;
                } else {
                    destination.insert(0, (name, value));
                }
            };

            match application {
                PropertyPlistApplication::AddProperties => {
                    properties.into_iter().for_each(&mut apply);
                }
                PropertyPlistApplication::PreserveSuppliedOrder => {
                    properties.into_iter().rev().for_each(&mut apply);
                }
            }
            destination
        }

        let saved_roots = save_scratch_gc_roots();
        let mut runs = Vec::new();
        for source in sources {
            for run in source.table.intervals.runs() {
                if run.is_empty_plist() {
                    continue;
                }
                let properties = applied_to_empty_destination(plist_pairs(run.plist), application);
                let plist = plist_value_from_pairs(&properties);
                // Later plist construction may collect.  Keep every fresh
                // destination spine alive until the finished tree owns it.
                push_scratch_gc_root(plist);
                runs.push(IntervalRun::new_in_char_range(
                    CharRange::new(
                        run.start().add_len(source.char_offset),
                        run.end().add_len(source.char_offset),
                    ),
                    plist,
                ));
            }
        }
        let table = Self::from_interval_runs_preserving_shape(runs);
        restore_scratch_gc_roots(saved_roots);
        table
    }

    fn replace_runs_preserving_shape(
        &mut self,
        runs: Vec<IntervalRun>,
        property_names: ReplacementPropertyNames<'_>,
    ) {
        self.mutation_tick += 1;
        match property_names {
            ReplacementPropertyNames::Preserve => {}
            ReplacementPropertyNames::Include(names) => self.property_names.include(names),
        }
        self.intervals = IntervalTree::from_runs_preserving_shape(runs);
    }

    fn split_runs_at(runs: &mut Vec<IntervalRun>, pos: CharPos0) {
        if pos == CharPos0::ZERO {
            return;
        }
        let Some(index) = runs
            .iter()
            .position(|run| run.start() < pos && pos < run.end())
        else {
            return;
        };

        let right_plist = plist_value_from_pairs(&plist_pairs(runs[index].plist));
        let mut right = runs[index].clone();
        right.set_start(pos);
        right.plist = right_plist;
        right.refresh_cache();
        runs[index].set_end(pos);
        runs.insert(index + 1, right);
    }

    fn ensure_runs_cover(runs: &mut Vec<IntervalRun>, end: CharPos0) {
        let current_end = runs.last().map(IntervalRun::end).unwrap_or(CharPos0::ZERO);
        if current_end < end {
            runs.push(IntervalRun::default_in_char_range(CharRange::new(
                current_end,
                end,
            )));
        }
    }

    // -- Public API ----------------------------------------------------------

    pub fn put_property_in_char_range(
        &mut self,
        range: CharRange,
        name: Value,
        value: Value,
    ) -> bool {
        self.put_property_raw(range, name, value)
    }

    fn put_property_raw(&mut self, range: CharRange, name: Value, value: Value) -> bool {
        self.mutation_tick += 1;
        if Self::name_is_syntax_relevant(name) {
            self.syntax_prop_tick += 1;
            self.syntax_ranges_note_put(range);
        }
        if range.is_empty() {
            return false;
        }
        self.property_names.observe(name);
        let mut changed = false;

        let affected = self.intervals.intervals_overlapping_after_splits(range);
        for (_, id) in affected {
            if plist_value_put_replace(&mut self.intervals.nodes[id.0].plist, name, value) {
                self.intervals.nodes[id.0].refresh_cache();
                changed = true;
            }
        }
        self.intervals.prune_trailing_empty_intervals();
        changed
    }

    pub fn put_property_for_object_char_len(
        &mut self,
        range: CharRange,
        object_len: CharLen,
        name: Value,
        value: Value,
    ) -> bool {
        self.put_property_for_object_len_raw(range, object_len, name, value)
    }

    pub(crate) fn apply_property_plist_for_object_char_len(
        &mut self,
        range: CharRange,
        object_len: CharLen,
        properties: &[(Value, Value)],
        application: PropertyPlistApplication,
    ) -> bool {
        let mut changed = false;
        let mut apply = |&(name, value): &(Value, Value)| {
            changed |= self.put_property_for_object_len_raw(range, object_len, name, value);
        };
        match application {
            PropertyPlistApplication::AddProperties => properties.iter().for_each(&mut apply),
            PropertyPlistApplication::PreserveSuppliedOrder => {
                properties.iter().rev().for_each(&mut apply);
            }
        }
        changed
    }

    fn put_property_for_object_len_raw(
        &mut self,
        range: CharRange,
        object_len: CharLen,
        name: Value,
        value: Value,
    ) -> bool {
        self.mutation_tick += 1;
        if Self::name_is_syntax_relevant(name) {
            self.syntax_prop_tick += 1;
        }
        if range.is_empty() {
            return false;
        }
        self.property_names.observe(name);

        self.intervals
            .ensure_cover(CharPos0::ZERO.add_len(object_len).max(range.end()));
        self.intervals.split_at(range.start());
        self.intervals.split_at(range.end());

        // Walk the covered intervals in place: the loop body only replaces
        // plist values (never splits or merges), so `(start, id)` cursor
        // state stays valid and the `ids_overlapping` Vec — a per-put
        // allocation that grows to the whole cover set — is not needed.
        // On a 20k-interval whole-buffer put the collect + realloc traffic
        // was ~19% of the workload.
        let mut changed = false;
        let mut cursor = self.intervals.first_id_overlapping(range);
        while let Some((node_start, id)) = cursor {
            if node_start >= range.end() {
                break;
            }
            let node_end = self.intervals.interval_end(node_start, id);
            if node_end > range.start()
                && plist_value_put_replace(&mut self.intervals.nodes[id.0].plist, name, value)
            {
                self.intervals.nodes[id.0].refresh_cache();
                changed = true;
            }
            cursor = self.intervals.next_id(id).map(|next| (node_end, next));
        }
        changed
    }

    pub(crate) fn from_plist_runs(runs: Vec<TextPropertyPlistRun>) -> Self {
        Self::from_interval_runs_preserving_shape(
            runs.into_iter()
                .filter_map(TextPropertyPlistRun::into_interval_run)
                .collect(),
        )
    }

    pub fn get_property_at_char_pos(&self, pos: CharPos0, name: Value) -> Option<Value> {
        // A name never written to this table cannot be on any interval:
        // answer without descending (one set probe instead of a tree walk).
        // `field`, `invisible`, `display`, `composition` probes from the
        // motion and column scanners are overwhelmingly this case.
        if self.property_name_presence(name) == PropertyNamePresence::DefinitelyAbsent {
            return None;
        }
        self.get_property_raw(pos, name)
    }

    /// The plist of the interval covering `pos`, or `None` when no interval
    /// does -- GNU `interval_of` (src/textprop.c), whose NULL result means the
    /// character has no properties at all.
    ///
    /// Distinct from [`Self::interval_plist_run_at_char_pos`]: a caller that
    /// only needs the character's properties must not pay for the run bounds,
    /// which cost an interval-end computation and, in a gap, a walk to the next
    /// property change. That walk ran per character on the byte-addressed
    /// scanners (regexp matching, forward-comment) until this existed.
    pub fn interval_plist_at_char_pos(&self, pos: CharPos0) -> Option<Value> {
        self.find_interval(pos).map(|(_, node)| node.plist)
    }

    fn get_property_raw(&self, pos: CharPos0, name: Value) -> Option<Value> {
        if self.property_name_presence(name) == PropertyNamePresence::DefinitelyAbsent {
            return None;
        }
        let (_, node) = self.find_interval(pos)?;
        plist_value_get(node.plist, name)
    }

    /// Returns property `name` at `pos` together with the `[start, end)` char
    /// run over which it stays constant (the interval containing `pos`, or the
    /// gap up to the next property boundary when no interval covers it).
    ///
    /// Mirrors GNU `syntax.c` `update_syntax_table`, which loads
    /// `gl_state.b_property` / `gl_state.e_property` from the interval at the
    /// position so a scan range-checks per char and only refetches when it
    /// leaves the run.  `total` bounds the gap run at the buffer end.
    pub fn get_property_run_at_char_pos(
        &self,
        pos: CharPos0,
        name: Value,
        total: usize,
    ) -> (Option<Value>, CharPos0, CharPos0) {
        match self.intervals.find_id(pos) {
            Some((start, id)) => {
                let node = &self.intervals.nodes[id.0];
                let end = self.intervals.interval_end(start, id);
                (plist_value_get(node.plist, name), start, end)
            }
            None => {
                let end = self
                    .next_property_change_raw(pos)
                    .unwrap_or_else(|| CharPos0::new(total));
                (None, pos, end)
            }
        }
    }

    /// Returns the plist of the interval covering `pos` together with its
    /// `[start, end)` char run, or `None` for the plist when no interval covers
    /// `pos` (the run then spans up to the next property boundary).
    ///
    /// This is the shape GNU's `update_syntax_table` (src/syntax.c) works in:
    /// it takes `interval_of (charpos)`, keeps `i->position` /
    /// `INTERVAL_LAST_POS (i)` as the run, and resolves the property from
    /// `i->plist` with `textget` -- and returns early with NO property when
    /// there is no interval, which is why the missing case is distinguished
    /// from an interval whose plist lacks the property.
    pub fn interval_plist_run_at_char_pos(
        &self,
        pos: CharPos0,
        total: usize,
    ) -> (Option<Value>, CharPos0, CharPos0) {
        match self.intervals.find_id(pos) {
            Some((start, id)) => {
                let node = &self.intervals.nodes[id.0];
                let end = self.intervals.interval_end(start, id);
                (Some(node.plist), start, end)
            }
            None => {
                let end = self
                    .next_property_change_raw(pos)
                    .unwrap_or_else(|| CharPos0::new(total));
                (None, pos, end)
            }
        }
    }

    /// The next char position in `(pos, cap)` where any property in `keys`
    /// changes VALUE from its value at `pos`, or `cap` if none change first.
    ///
    /// Mirrors GNU `compute_stop_pos` (src/xdisp.c): record the watched keys'
    /// values at `pos`, then step interval boundaries comparing ONLY those keys,
    /// so a run that is constant in `keys` but split by other properties (e.g.
    /// dense `face` intervals from font-lock) is coalesced in one walk instead of
    /// stopping at every boundary. Gaps (no interval) read as all-nil, matching
    /// `next_property_change_raw`. This lets a per-char display scanner learn the
    /// span over which invisible/display/composition cannot change and skip the
    /// expensive per-char property probes across it.
    pub fn next_watched_property_change(
        &self,
        pos: CharPos0,
        cap: CharPos0,
        keys: &[Value],
    ) -> CharPos0 {
        if cap <= pos {
            return cap;
        }
        // Same locate-once-then-step-siblings walk as
        // `next_single_property_change_after_char_pos`, comparing the watched keys
        // directly off each node's plist spine and bounded by `cap`.
        let mut cursor = self.intervals.cursor_at(pos);
        let Some((_, mut boundary, first_node)) = cursor.next() else {
            return cap;
        };
        let current = first_node.plist;
        loop {
            if boundary >= cap {
                return cap;
            }
            match cursor.next() {
                Some((_, end, node)) => {
                    if !watched_keys_equal_eq_plist(current, node.plist, keys) {
                        return boundary;
                    }
                    boundary = end;
                }
                None => {
                    // Trailing implicit-nil region at the tree end.
                    if !watched_keys_equal_eq_plist(current, Value::NIL, keys) {
                        return boundary;
                    }
                    return cap;
                }
            }
        }
    }

    /// The end of the run from `pos` (capped at `cap`) over which NO interval
    /// carries a `syntax-table`/`category` plist key, using the cached
    /// per-node bit — one bool compare per boundary instead of plist walks.
    ///
    /// Only meaningful when the interval AT `pos` itself lacks the bit: a
    /// bit-set interval's value can differ from its bit-set neighbor's, so
    /// prop-bearing intervals are never merged — the walk stops at the first
    /// one. Callers with a bit-set current interval must use the precise
    /// `next_watched_property_change` instead.
    pub fn syntax_prop_free_run_end(&self, pos: CharPos0, cap: CharPos0) -> CharPos0 {
        if cap <= pos {
            return cap;
        }
        // Whole-table fast path: no interval anywhere carries a
        // syntax-relevant key (the overwhelmingly common buffer), so every
        // run extends to the cap without walking anything.
        if !self.has_any_syntax_prop_interval() {
            return cap;
        }
        // Sparse fast path: binary-search the lazily built list of bit-set
        // intervals instead of stepping every face interval up to the cap.
        if let Ok(mut guard) = self.syntax_prop_ranges.try_lock() {
            if guard.0 != self.syntax_prop_tick + 1 {
                let mut ranges = Vec::new();
                for (start, end, node) in self.intervals.cursor_at(CharPos0::ZERO) {
                    if node.has_syntax_prop {
                        ranges.push((start, end));
                    }
                }
                *guard = (self.syntax_prop_tick + 1, ranges);
            }
            let ranges = &guard.1;
            // First range whose end is beyond `pos`.
            let idx = ranges.partition_point(|&(_, end)| end <= pos);
            return match ranges.get(idx) {
                Some(&(start, end)) if start <= pos => end.min(cap), // inside a prop interval
                Some(&(start, _)) => start.min(cap),                 // prop-free until the next one
                None => cap,
            };
        }
        let mut cursor = self.intervals.cursor_at(pos);
        let Some((_, mut boundary, first)) = cursor.next() else {
            return cap; // no intervals at all: implicit-nil to the cap
        };
        if first.has_syntax_prop {
            return boundary.min(cap); // prop-bearing: just its own extent
        }
        loop {
            if boundary >= cap {
                return cap;
            }
            match cursor.next() {
                Some((_, end, node)) => {
                    if node.has_syntax_prop {
                        return boundary;
                    }
                    boundary = end;
                }
                None => return cap, // trailing implicit-nil region
            }
        }
    }

    /// `put` of a syntax-relevant property over `range`: keep the cached
    /// range list truthful incrementally (drop overlapped entries, insert the
    /// put range as one merged entry — the put wrote ONE value across it, so
    /// treating it as a single constant run is sound), then revalidate the
    /// guard for the just-bumped tick. Invalidating instead would cost a
    /// full-tree rebuild scan on the next query — per syntax-propertize
    /// write, which typing triggers constantly.
    fn syntax_ranges_note_put(&mut self, range: CharRange) {
        let Ok(mut guard) = self.syntax_prop_ranges.lock() else {
            return;
        };
        // Valid for the PRE-bump tick (we just incremented)?
        if guard.0 != self.syntax_prop_tick {
            *guard = (0, Vec::new());
            return;
        }
        if !range.is_empty() {
            guard
                .1
                .retain(|&(s, e)| e <= range.start() || s >= range.end());
            let idx = guard.1.partition_point(|&(s, _)| s < range.start());
            guard.1.insert(idx, (range.start(), range.end()));
        }
        guard.0 = self.syntax_prop_tick + 1;
        // The any-flag is monotone under put; refresh it too.
        self.syntax_prop_any.store(
            ((self.syntax_prop_tick + 1) << 1) | 1,
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    /// Revalidate the cached ranges for the just-bumped tick without
    /// touching their contents (see `remove_property_raw`).
    fn syntax_ranges_revalidate(&mut self) {
        let Ok(mut guard) = self.syntax_prop_ranges.lock() else {
            return;
        };
        if guard.0 == self.syntax_prop_tick {
            guard.0 = self.syntax_prop_tick + 1;
            let packed = self
                .syntax_prop_any
                .load(std::sync::atomic::Ordering::Relaxed);
            if packed >> 1 == self.syntax_prop_tick {
                self.syntax_prop_any.store(
                    ((self.syntax_prop_tick + 1) << 1) | (packed & 1),
                    std::sync::atomic::Ordering::Relaxed,
                );
            }
        } else {
            *guard = (0, Vec::new());
        }
    }

    /// Whether ANY interval in the table carries a syntax-relevant plist key
    /// (see the `syntax_prop_any` field doc for the lazy memo contract).
    fn has_any_syntax_prop_interval(&self) -> bool {
        use std::sync::atomic::Ordering;
        let packed = self.syntax_prop_any.load(Ordering::Relaxed);
        if packed >> 1 == self.syntax_prop_tick + 1 {
            return packed & 1 == 1;
        }
        let any = self
            .intervals
            .cursor_at(CharPos0::ZERO)
            .any(|(_, _, node)| node.has_syntax_prop);
        self.syntax_prop_any.store(
            ((self.syntax_prop_tick + 1) << 1) | u64::from(any),
            Ordering::Relaxed,
        );
        any
    }

    /// Whether any property in `keys` has a non-nil value on the half-open
    /// character `range`.
    ///
    /// The [`CharRange`] argument makes the scan limit part of the operation's
    /// type instead of an optional caller convention.  The interval tree is
    /// located once at `range.start()` and then traversed in order only until
    /// `range.end()`, matching GNU redisplay's bounded, multi-property walk in
    /// `compute_stop_pos`.
    pub fn has_any_non_nil_property_in_char_range(&self, range: CharRange, keys: &[Value]) -> bool {
        if range.is_empty() || keys.is_empty() {
            return false;
        }
        // Same conservative-name short-circuit as
        // `range_has_any_property_named_raw`: a never-assigned key is nil in
        // every interval, so no interval can hold a non-nil value for it.
        if keys
            .iter()
            .all(|key| self.property_names.presence(*key) == PropertyNamePresence::DefinitelyAbsent)
        {
            return false;
        }

        self.intervals
            .cursor_at(range.start())
            .take_while(|(start, _, _)| *start < range.end())
            .any(|(_, _, node)| !watched_keys_equal_eq_plist(node.plist, Value::NIL, keys))
    }

    pub fn get_properties_at_char_pos(&self, pos: CharPos0) -> HashMap<Value, Value> {
        self.get_properties_raw(pos)
    }

    fn get_properties_raw(&self, pos: CharPos0) -> HashMap<Value, Value> {
        let Some((start, id)) = self.intervals.find_id(pos) else {
            return HashMap::new();
        };
        let node = &self.intervals.nodes[id.0];
        let end = self.intervals.interval_end(start, id);
        PropertyInterval::from_plist(CharRange::new(start, end), &plist_pairs(node.plist))
            .properties
    }

    pub fn get_properties_ordered_at_char_pos(&self, pos: CharPos0) -> Vec<(Value, Value)> {
        self.get_properties_ordered_raw(pos)
    }

    fn get_properties_ordered_raw(&self, pos: CharPos0) -> Vec<(Value, Value)> {
        let Some((_, node)) = self.find_interval(pos) else {
            return Vec::new();
        };
        plist_pairs(node.plist)
    }

    pub fn get_properties_plist_value_at_char_pos(&self, pos: CharPos0) -> Value {
        self.get_properties_plist_value_raw(pos)
    }

    fn get_properties_plist_value_raw(&self, pos: CharPos0) -> Value {
        let Some((_, node)) = self.find_interval(pos) else {
            return Value::NIL;
        };
        node.plist
    }

    pub fn range_has_all_properties_in_char_range(
        &self,
        range: CharRange,
        properties: &[(Value, Value)],
    ) -> bool {
        self.range_has_all_properties_raw(range, properties)
    }

    fn range_has_all_properties_raw(
        &self,
        range: CharRange,
        properties: &[(Value, Value)],
    ) -> bool {
        if range.is_empty() || properties.is_empty() {
            return true;
        }

        let Some((mut interval_start, mut id)) = self.intervals.find_id(range.start()) else {
            return false;
        };
        let mut cursor = range.start();
        while cursor < range.end() {
            let node = &self.intervals.nodes[id.0];
            for (name, value) in properties {
                let Some(existing) = plist_value_get(node.plist, *name) else {
                    return false;
                };
                if !eq_value(&existing, value) {
                    return false;
                }
            }
            let node_end = self.intervals.interval_end(interval_start, id);
            if node_end <= cursor {
                return false;
            }
            cursor = node_end.min(range.end());
            if cursor < range.end() {
                let Some(next_id) = self.intervals.next_id(id) else {
                    return false;
                };
                interval_start = node_end;
                id = next_id;
            }
        }

        true
    }

    pub fn range_has_any_property_named_in_char_range(
        &self,
        range: CharRange,
        names: &[Value],
    ) -> bool {
        self.range_has_any_property_named_raw(range, names)
    }

    fn range_has_any_property_named_raw(&self, range: CharRange, names: &[Value]) -> bool {
        if range.is_empty() || names.is_empty() {
            return false;
        }
        // Conservative name summary first: a name never assigned since the
        // last exact rebuild cannot sit in any interval, so the tree walk is
        // provably fruitless. This is `syntax-propertize`'s flush probing
        // `(syntax-table syntax-multiline)` over a buffer holding only
        // font-lock `face` intervals.
        if names.iter().all(|name| {
            self.property_names.presence(*name) == PropertyNamePresence::DefinitelyAbsent
        }) {
            return false;
        }

        self.any_interval_overlapping(range, |_, node| {
            names
                .iter()
                .any(|name| plist_value_get(node.plist, *name).is_some())
        })
    }

    pub fn range_has_any_interval_in_char_range(&self, range: CharRange) -> bool {
        self.range_has_any_interval_raw(range)
    }

    fn range_has_any_interval_raw(&self, range: CharRange) -> bool {
        if range.is_empty() {
            return false;
        }

        self.any_interval_overlapping(range, |_, node| !node.is_empty_plist())
    }

    pub fn remove_property_in_char_range(&mut self, range: CharRange, name: Value) -> bool {
        self.remove_property_raw(range, name)
    }

    fn remove_property_raw(&mut self, range: CharRange, name: Value) -> bool {
        self.mutation_tick += 1;
        if Self::name_is_syntax_relevant(name) {
            self.syntax_prop_tick += 1;
            // Removal keeps existing entries: an entry only BOUNDS a run —
            // the value always resolves fresh from the tree, and a stale
            // presence entry just yields a shorter-than-optimal run. What
            // must not happen is the full-rebuild scan per propertize flush.
            self.syntax_ranges_revalidate();
        }
        if range.is_empty() {
            return false;
        }
        let mut changed = false;

        let affected = self
            .intervals
            .existing_intervals_overlapping_after_splits(range);
        for (_, id) in affected {
            if plist_value_remove(&mut self.intervals.nodes[id.0].plist, name) {
                self.intervals.nodes[id.0].refresh_cache();
                changed = true;
            }
        }
        changed
    }

    /// Strip every property in `names` over `range` in ONE split+collect
    /// interval walk. The per-name variant repeats that walk (and its
    /// callers repeat their undo-run walk) per property; font-lock
    /// unfontify removes several properties per edit.
    pub fn remove_properties_in_char_range(&mut self, range: CharRange, names: &[Value]) -> bool {
        self.mutation_tick += 1;
        if names
            .iter()
            .any(|name| Self::name_is_syntax_relevant(*name))
        {
            self.syntax_prop_tick += 1;
            // Same policy as the single-name removal: presence entries only
            // bound runs, so keep them and revalidate the guard.
            self.syntax_ranges_revalidate();
        }
        if range.is_empty() || names.is_empty() {
            return false;
        }
        let mut changed = false;
        let affected = self
            .intervals
            .existing_intervals_overlapping_after_splits(range);
        for (_, id) in affected {
            let mut node_changed = false;
            for name in names {
                if plist_value_remove(&mut self.intervals.nodes[id.0].plist, *name) {
                    node_changed = true;
                }
            }
            if node_changed {
                self.intervals.nodes[id.0].refresh_cache();
                changed = true;
            }
        }
        changed
    }

    pub fn remove_all_properties_in_char_range(&mut self, range: CharRange) {
        self.remove_all_properties_raw(range);
    }

    fn remove_all_properties_raw(&mut self, range: CharRange) {
        self.mutation_tick += 1;
        self.syntax_prop_tick += 1;
        // Pure removal: stale-presence entries only shorten runs (values
        // always resolve fresh) — keep them and revalidate the guard.
        self.syntax_ranges_revalidate();
        if range.is_empty() {
            return;
        }

        let affected = self
            .intervals
            .existing_intervals_overlapping_after_splits(range);
        for (_, id) in affected {
            self.intervals.set_node_plist(id, Value::NIL);
        }
        self.intervals.prune_trailing_empty_intervals();
    }

    pub fn set_properties_in_char_range(&mut self, range: CharRange, plist: Vec<(Value, Value)>) {
        self.set_properties_raw(range, plist);
    }

    fn set_properties_raw(&mut self, range: CharRange, plist: Vec<(Value, Value)>) {
        self.mutation_tick += 1;
        self.syntax_prop_tick += 1;
        if plist
            .iter()
            .any(|&(name, _)| Self::name_is_syntax_relevant(name))
        {
            self.syntax_ranges_note_put(range);
        } else {
            self.syntax_ranges_revalidate();
        }
        if range.is_empty() {
            return;
        }
        if plist.is_empty() && self.intervals.is_empty() {
            return;
        }
        for (name, _) in &plist {
            self.property_names.observe(*name);
        }

        self.intervals.ensure_cover(range.end());
        self.intervals.split_at(range.start());
        self.intervals.split_at(range.end());

        let mut cursor = range.start();
        let mut first = true;
        while cursor < range.end() {
            let Some((node_start, id)) = self.intervals.find_id(cursor) else {
                break;
            };
            let node_end = self.intervals.interval_end(node_start, id);
            self.intervals
                .set_node_plist(id, plist_value_from_pairs(&plist));
            if first {
                first = false;
            } else {
                self.intervals.merge_interval_left(id);
            }
            if node_end <= cursor {
                break;
            }
            cursor = node_end;
        }

        if plist.is_empty() {
        } else {
            self.intervals.prune_trailing_empty_intervals();
        }
    }

    pub fn set_properties_for_object_char_len(
        &mut self,
        range: CharRange,
        object_len: CharLen,
        plist: Vec<(Value, Value)>,
    ) {
        self.set_properties_for_object_len_raw(range, object_len, plist);
    }

    fn set_properties_for_object_len_raw(
        &mut self,
        range: CharRange,
        object_len: CharLen,
        plist: Vec<(Value, Value)>,
    ) {
        self.mutation_tick += 1;
        self.syntax_prop_tick += 1;
        if plist
            .iter()
            .any(|&(name, _)| Self::name_is_syntax_relevant(name))
        {
            self.syntax_ranges_note_put(range);
        } else {
            self.syntax_ranges_revalidate();
        }
        if range.is_empty() {
            return;
        }
        if plist.is_empty() && self.intervals.is_empty() {
            return;
        }
        for (name, _) in &plist {
            self.property_names.observe(*name);
        }

        self.intervals
            .ensure_cover(CharPos0::ZERO.add_len(object_len).max(range.end()));
        self.intervals.split_at(range.start());
        self.intervals.split_at(range.end());

        let new_plist = plist_value_from_pairs(&plist);
        let mut cursor = range.start();
        let mut first = true;
        while cursor < range.end() {
            let Some((node_start, id)) = self.intervals.find_id(cursor) else {
                break;
            };
            let node_end = self.intervals.interval_end(node_start, id);
            self.intervals.set_node_plist(id, new_plist);
            if first {
                first = false;
            } else {
                self.intervals.merge_interval_left(id);
            }
            if node_end <= cursor {
                break;
            }
            cursor = node_end;
        }
    }

    pub fn next_property_change_after_char_pos(&self, pos: CharPos0) -> Option<CharPos0> {
        self.next_property_change_raw(pos)
    }

    /// Like `next_property_change_after_char_pos`, but only reports a change of
    /// the single property `name` (compared by `eq`), ignoring changes to any
    /// other property.  Mirrors GNU `next_single_char_property_change`'s
    /// text-property half: walk interval boundaries and stop when the value of
    /// `name` differs from its value at `pos`.
    pub fn next_single_property_change_after_char_pos(
        &self,
        pos: CharPos0,
        name: Value,
    ) -> Option<CharPos0> {
        if self.property_name_presence(name) == PropertyNamePresence::DefinitelyAbsent {
            return None;
        }
        self.next_single_property_change_after_char_pos_limited(pos, name, None)
    }

    /// Display-engine variant of [`Self::next_single_property_change_after_char_pos`]
    /// that stops scanning at `limit` and reports `Some(limit)` as a soft
    /// boundary when `name` has not changed by then. Mirrors GNU's
    /// `TEXT_PROP_DISTANCE_LIMIT` cap in `compute_stop_pos`: the display code
    /// only needs the next boundary within the visible window, and a boundary
    /// that is *too small* merely makes the caller re-check sooner -- it never
    /// changes what is rendered, because the invisible/display status at each
    /// position is looked up exactly and independently. This turns the layout's
    /// per-redisplay invisible/display scan from O(buffer) into O(window).
    ///
    /// Lisp `next-single-property-change` MUST return the exact boundary, so it
    /// keeps calling the unbounded method above (the `None` limit path).
    pub fn next_single_property_change_after_char_pos_bounded(
        &self,
        pos: CharPos0,
        name: Value,
        limit: CharPos0,
    ) -> Option<CharPos0> {
        self.next_single_property_change_after_char_pos_limited(pos, name, Some(limit))
    }

    fn next_single_property_change_after_char_pos_limited(
        &self,
        pos: CharPos0,
        name: Value,
        limit: Option<CharPos0>,
    ) -> Option<CharPos0> {
        // GNU's Fnext_single_property_change shape: locate the interval at `pos`
        // once, capture `name`'s value there, then step siblings comparing only
        // `name` and return the first boundary where it differs -- O(log n + N)
        // instead of two find_id descents per boundary.
        let mut cursor = self.intervals.cursor_at(pos);
        let (_, first_end, first_node) = cursor.next()?;
        let current = plist_value_get(first_node.plist, name).unwrap_or(Value::NIL);
        let mut boundary = first_end;
        for (start, end, node) in cursor {
            // Soft cap: once the next interval begins at or beyond `limit`, stop
            // and report `limit` -- `name` has held `current` continuously across
            // it, so this is a valid (merely early) boundary. Checked on `start`
            // only: the natural tail below still handles running out of intervals
            // within `[pos, limit]`, so a `limit` past the tree end reduces to the
            // exact unbounded answer (never a spurious stop at buffer end).
            if limit.is_some_and(|limit| start >= limit) {
                return limit;
            }
            let value = plist_value_get(node.plist, name).unwrap_or(Value::NIL);
            if !eq_value(&current, &value) {
                return Some(start);
            }
            boundary = end;
        }
        // Past the last interval the buffer is implicitly nil: report the change
        // to nil at the tree end (matching the old walk, which read nil there).
        if !eq_value(&current, &Value::NIL) {
            return Some(boundary);
        }
        None
    }

    fn next_property_change_raw(&self, pos: CharPos0) -> Option<CharPos0> {
        let current = self.plist_at(pos).unwrap_or_default();
        let mut cursor = pos;
        while let Some(next) = self.next_interval_boundary_raw(cursor) {
            let next_plist = self.plist_at(next).unwrap_or_default();
            if !plists_equal_eq(&current, &next_plist) {
                return Some(next);
            }
            if next.get() <= cursor.get() {
                return None;
            }
            cursor = next;
        }
        None
    }

    pub fn previous_property_change_before_char_pos(&self, pos: CharPos0) -> Option<CharPos0> {
        self.previous_property_change_raw(pos)
    }

    /// Previous boundary where the single property `name` changes, ignoring
    /// unrelated plist changes. The active value is the one at `pos`.
    pub fn previous_single_property_change_before_char_pos(
        &self,
        pos: CharPos0,
        name: Value,
    ) -> Option<CharPos0> {
        if self.property_name_presence(name) == PropertyNamePresence::DefinitelyAbsent {
            return None;
        }
        // Backward mirror of next_single_property_change_after_char_pos. `current`
        // is `name`'s value at pos (nil past the tree end); `boundary` is the start
        // of the run of `current` reached so far. We step the intervals STRICTLY
        // BEFORE `boundary` via prev_id, comparing `current` to each one's value
        // (the value just before `boundary`) and returning `boundary` on the first
        // difference -- the same positions the old two-find_id-per-boundary walk
        // reported, with one seat + O(1) steps.
        let (current, mut boundary, seat) = match self.intervals.find_id(pos) {
            Some((start, id)) => {
                let current = plist_value_get(self.intervals.nodes[id.0].plist, name);
                let seat = self.intervals.prev_id(id).map(|prev| {
                    let prev_start = start.saturating_sub_len(self.intervals.node_len(prev));
                    (prev_start, prev)
                });
                (current, start, seat)
            }
            None => {
                // pos is past the last interval: the value there is nil, and the
                // nearest change back is at the tree end (last interval's value vs
                // the implicit trailing nil).
                let last = self.intervals.last_interval()?;
                let tree_len = CharPos0::ZERO.add_len(self.intervals.len());
                (None, tree_len, Some(last))
            }
        };
        let current_norm = current.unwrap_or(Value::NIL);
        let mut cursor = self.intervals.reverse_cursor_from(seat);
        loop {
            if boundary == CharPos0::ZERO {
                return current.is_some().then_some(CharPos0::ZERO);
            }
            match cursor.next() {
                Some((start, _, node)) => {
                    let value_before = plist_value_get(node.plist, name).unwrap_or(Value::NIL);
                    if !eq_value(&current_norm, &value_before) {
                        return Some(boundary);
                    }
                    boundary = start;
                }
                None => return current.is_some().then_some(CharPos0::ZERO),
            }
        }
    }

    fn previous_property_change_raw(&self, pos: CharPos0) -> Option<CharPos0> {
        if pos == CharPos0::ZERO {
            return None;
        }

        let current = self
            .plist_at(pos.saturating_sub_len(CharLen::new(1)))
            .unwrap_or_default();
        let mut cursor = pos;
        while let Some(prev) = self.previous_interval_boundary_raw(cursor) {
            if prev == CharPos0::ZERO {
                return None;
            }
            let previous_plist = self
                .plist_at(prev.saturating_sub_len(CharLen::new(1)))
                .unwrap_or_default();
            if !plists_equal_eq(&current, &previous_plist) {
                return Some(prev);
            }
            if prev.get() >= cursor.get() {
                return None;
            }
            cursor = prev;
        }

        None
    }

    /// Return the next raw interval boundary after `pos`, even when adjacent
    /// interval plists are equal.  This matches GNU's `next_interval` path used
    /// by `(next-property-change POS OBJECT t)`.
    /// Walk the intervals forward from the one containing `pos`, in tree
    /// order, calling `f(start, end, plist)` for each until it returns
    /// `false`.  O(1) amortized per step (`next_id`), no per-step descent or
    /// position conversion -- the shape of GNU's `next_interval` loops.
    /// Positions past the tree's cover yield nothing: the trailing
    /// property-free text has no interval.
    pub fn for_each_interval_from_char_pos<F>(&self, pos: CharPos0, mut f: F)
    where
        F: FnMut(CharPos0, CharPos0, Value) -> bool,
    {
        for (start, end, node) in self.intervals.cursor_at(pos) {
            if !f(start, end, node.plist) {
                return;
            }
        }
    }

    pub fn next_interval_boundary_after_char_pos(&self, pos: CharPos0) -> Option<CharPos0> {
        self.next_interval_boundary_raw(pos)
    }

    fn next_interval_boundary_raw(&self, pos: CharPos0) -> Option<CharPos0> {
        if let Some((start, id)) = self.intervals.find_id(pos) {
            return Some(self.intervals.interval_end(start, id));
        }

        self.intervals.first_start_after(pos)
    }

    /// Return the previous raw interval boundary before `pos`.
    pub fn previous_interval_boundary_before_char_pos(&self, pos: CharPos0) -> Option<CharPos0> {
        self.previous_interval_boundary_raw(pos)
    }

    fn previous_interval_boundary_raw(&self, pos: CharPos0) -> Option<CharPos0> {
        if pos == CharPos0::ZERO {
            return None;
        }

        let scan_pos = pos.saturating_sub_len(CharLen::new(1));
        if let Some((start, _)) = self.intervals.find_id(scan_pos) {
            return Some(start);
        }

        let tree_len = CharPos0::ZERO.add_len(self.intervals.len());
        (tree_len > CharPos0::ZERO && tree_len < pos).then_some(tree_len)
    }

    pub fn adjust_for_insert_at_char_pos(&mut self, pos: CharPos0, len: CharLen) {
        self.adjust_for_insert_raw(pos, len);
    }

    fn adjust_for_insert_raw(&mut self, pos: CharPos0, len: CharLen) {
        self.mutation_tick += 1;
        if len.is_empty() {
            return;
        }
        // Position shifts don't bump syntax_prop_tick (values are unchanged),
        // but the cached bit-set-interval RANGES hold positions — shift them
        // in place so they stay truthful. (Serving stale positions here was a
        // real bug: the interactive sim's typing phase doubled and could
        // misclassify syntax runs near shifted properties.)
        if let Ok(mut guard) = self.syntax_prop_ranges.lock()
            && guard.0 == self.syntax_prop_tick + 1
        {
            for (start, end) in guard.1.iter_mut() {
                if *start >= pos {
                    *start = start.add_len(len);
                    *end = end.add_len(len);
                } else if *end > pos {
                    *end = end.add_len(len);
                }
            }
        }

        self.intervals.insert_default_at(pos, len);
    }

    pub fn adjust_for_delete_char_range(&mut self, range: CharRange) {
        self.adjust_for_delete_raw(range);
    }

    fn adjust_for_delete_raw(&mut self, range: CharRange) {
        self.mutation_tick += 1;
        if range.is_empty() {
            return;
        }
        // See adjust_for_insert_raw: keep the cached ranges positionally
        // truthful. Deletion clips; an entry emptied by the clip is removed.
        if let Ok(mut guard) = self.syntax_prop_ranges.lock()
            && guard.0 == self.syntax_prop_tick + 1
        {
            let del_len = range.len();
            for (start, end) in guard.1.iter_mut() {
                *start = clip_char_pos_for_delete(*start, range, del_len);
                *end = clip_char_pos_for_delete(*end, range, del_len);
            }
            guard.1.retain(|(start, end)| end > start);
        }

        self.intervals.delete_range(range);
    }

    pub fn adjust_for_replace_at_char_pos(
        &mut self,
        start: CharPos0,
        old_len: CharLen,
        new_len: CharLen,
    ) {
        self.adjust_for_replace_raw(start, old_len, new_len);
    }

    fn adjust_for_replace_raw(&mut self, start: CharPos0, old_len: CharLen, new_len: CharLen) {
        self.mutation_tick += 1;
        // A replace both clips and shifts; just drop the cached ranges (rare
        // relative to plain inserts/deletes) — the guard tick 0 never matches.
        if let Ok(mut guard) = self.syntax_prop_ranges.lock() {
            *guard = (0, Vec::new());
        }
        match new_len.cmp(&old_len) {
            std::cmp::Ordering::Greater => {
                self.adjust_for_insert_raw(start, CharLen::new(new_len.get() - old_len.get()));
            }
            std::cmp::Ordering::Less => {
                self.adjust_for_delete_raw(CharRange::from_start_len(
                    start,
                    CharLen::new(old_len.get() - new_len.get()),
                ));
            }
            std::cmp::Ordering::Equal => {}
        }
    }

    pub fn intervals_snapshot(&self) -> Vec<PropertyInterval> {
        self.intervals
            .runs()
            .into_iter()
            .filter(|run| !run.is_empty_plist())
            .map(|run| PropertyInterval::from_plist(run.range, &plist_pairs(run.plist)))
            .collect()
    }

    /// Return GNU `object-intervals' shaped runs for a string of `len' chars.
    ///
    /// Unlike `intervals_snapshot', this keeps nil-property gaps once an
    /// interval tree exists.  GNU reports those gaps so callers can observe the
    /// complete interval partition of the string.
    pub fn object_interval_runs_for_char_len(&self, len: CharLen) -> Vec<ObjectIntervalRun> {
        self.object_interval_runs_raw(len)
    }

    fn object_interval_runs_raw(&self, len: CharLen) -> Vec<ObjectIntervalRun> {
        self.object_interval_plist_runs_raw(len)
            .into_iter()
            .map(|run| ObjectIntervalRun::new(run.start(), run.end(), plist_pairs(run.plist())))
            .collect()
    }

    /// Return GNU `object-intervals' shaped runs with the live interval plist.
    ///
    /// GNU `collect_interval' stores `interval->plist' directly in the
    /// returned structure, so later property replacement can be visible through
    /// that plist object.
    pub fn object_interval_plist_runs_for_char_len(
        &self,
        len: CharLen,
    ) -> Vec<ObjectIntervalPlistRun> {
        self.object_interval_plist_runs_raw(len)
    }

    fn object_interval_plist_runs_raw(&self, len: CharLen) -> Vec<ObjectIntervalPlistRun> {
        if self.intervals.is_empty() {
            return Vec::new();
        }

        let len_pos = CharPos0::ZERO.add_len(len);
        let mut runs = Vec::new();
        let mut cursor = CharPos0::ZERO;
        for run in self.intervals.runs() {
            let start = run.start().min(len_pos);
            let end = run.end().min(len_pos);
            if cursor < start {
                runs.push(ObjectIntervalPlistRun::new(cursor, start, Value::NIL));
            }
            if start < end {
                runs.push(ObjectIntervalPlistRun::new(start, end, run.plist));
                cursor = end;
            }
        }
        if cursor < len_pos {
            runs.push(ObjectIntervalPlistRun::new(cursor, len_pos, Value::NIL));
        }
        runs
    }

    pub fn first_interval_pos_with_property_eq_in_char_range(
        &self,
        range: CharRange,
        name: Value,
        value: Value,
    ) -> Option<CharPos0> {
        self.first_interval_pos_with_property_eq_raw(range, name, value)
    }

    fn first_interval_pos_with_property_eq_raw(
        &self,
        range: CharRange,
        name: Value,
        value: Value,
    ) -> Option<CharPos0> {
        if range.is_empty() {
            return None;
        }
        let (mut key, mut id) = self.first_interval_overlapping(range)?;
        loop {
            if key >= range.end() {
                return None;
            }
            let node = &self.intervals.nodes[id.0];
            if plist_value_get(node.plist, name).is_some_and(|found| eq_value(&found, &value)) {
                return Some(key.max(range.start()));
            }
            let next_key = self.intervals.interval_end(key, id);
            let next_id = self.intervals.next_id(id)?;
            key = next_key;
            id = next_id;
        }
    }

    pub(crate) fn try_for_each_interval_in_char_range<E>(
        &self,
        range: CharRange,
        f: impl FnMut(CharRange, &[(Value, Value)]) -> Result<(), E>,
    ) -> Result<(), E> {
        self.try_for_each_interval_in_range_raw(range, f)
    }

    /// Like [`Self::try_for_each_interval_in_char_range`] but hands the
    /// closure each interval's plist VALUE instead of materializing a
    /// `Vec<(Value, Value)>` pair slice per interval — that per-interval
    /// allocation dominated read-only verification over large ranges.
    pub(crate) fn try_for_each_interval_plist_in_char_range<E>(
        &self,
        range: CharRange,
        mut f: impl FnMut(CharRange, Value) -> Result<(), E>,
    ) -> Result<(), E> {
        if range.is_empty() {
            return Ok(());
        }
        self.try_for_each_interval_overlapping(range, |interval_range, node| {
            f(interval_range, node.plist)
        })
    }

    fn try_for_each_interval_in_range_raw<E>(
        &self,
        range: CharRange,
        mut f: impl FnMut(CharRange, &[(Value, Value)]) -> Result<(), E>,
    ) -> Result<(), E> {
        if range.is_empty() {
            return Ok(());
        }
        self.try_for_each_interval_overlapping(range, |interval_range, node| {
            let pairs = plist_pairs(node.plist);
            f(interval_range, &pairs)
        })
    }

    pub fn is_empty(&self) -> bool {
        self.intervals.is_empty()
    }

    fn plist_at(&self, pos: CharPos0) -> Option<Vec<(Value, Value)>> {
        let (_, node) = self.find_interval(pos)?;
        Some(plist_pairs(node.plist))
    }

    fn next_interval_boundary_after(&self, pos: CharPos0, end: CharPos0) -> CharPos0 {
        let containing_end = self
            .intervals
            .find_id(pos)
            .map(|(start, id)| self.intervals.interval_end(start, id))
            .or_else(|| self.intervals.first_start_after(pos))
            .unwrap_or(end);
        let next_start = self.intervals.first_start_after(pos).unwrap_or(end);
        containing_end.min(next_start).min(end)
    }

    /// Compare string text-property intervals the way GNU
    /// `compare_string_intervals` does for `equal-including-properties`.
    ///
    /// Missing intervals are default intervals.  Property names compare by
    /// `eq`, while property values compare by ordinary `equal`.
    pub fn equal_including_property_values(
        left: Option<&TextPropertyTable>,
        right: Option<&TextPropertyTable>,
        len: usize,
    ) -> bool {
        let end = CharPos0::new(len);
        let mut pos = CharPos0::ZERO;
        while pos.get() < len {
            let left_plist = left
                .and_then(|table| table.plist_at(pos))
                .unwrap_or_default();
            let right_plist = right
                .and_then(|table| table.plist_at(pos))
                .unwrap_or_default();
            if !plists_equal_values_equal(&left_plist, &right_plist) {
                return false;
            }

            let left_next = left
                .map(|table| table.next_interval_boundary_after(pos, end))
                .unwrap_or(end);
            let right_next = right
                .map(|table| table.next_interval_boundary_after(pos, end))
                .unwrap_or(end);
            let next = left_next.min(right_next).max(pos.add_len(CharLen::new(1)));
            pos = next;
        }
        true
    }

    pub fn slice_char_range(&self, range: CharRange) -> TextPropertyTable {
        self.slice_raw(range)
    }

    fn slice_raw(&self, range: CharRange) -> TextPropertyTable {
        if range.is_empty() {
            return TextPropertyTable::new();
        }
        let start = range.start().get();
        let end = range.end().get();
        // GNU `copy_intervals` (src/intervals.c): one interval per source
        // interval overlapping the range, each carrying `copy_properties` -- a
        // `Fcopy_sequence` of the plist -- with the cached flags carried over
        // (they depend only on the plist).  The intervals arrive in order and
        // tile the range, so the tree is built straight from them.
        //
        // History: this went through plist pairs, `TextPropertyPlistRun`, a
        // second cons walk, a flag re-extraction and a name-summary walk per
        // interval (~500 Ir each; a 50-line kill slices ~170 of them twice).
        let mut runs: Vec<IntervalRun> = Vec::new();
        self.for_each_interval_overlapping(range, |interval_start, node_end, node| {
            let new_start = interval_start.max(start) - start;
            let new_end = node_end.min(end) - start;
            if new_start < new_end {
                let mut run = IntervalRun::from_node(
                    CharPos0::new(new_start),
                    node,
                    CharLen::new(new_end - new_start),
                );
                run.plist = copy_plist_value(node.plist);
                runs.push(run);
            }
        });
        if runs.is_empty() {
            return TextPropertyTable::new();
        }
        // The slice's names are a subset of the source's: the conservative
        // summary (false positives allowed) is shared in O(1).
        Self::with_runs_and_names(runs, self.property_names.clone())
    }

    pub fn slice_copy_text_properties_char_range(&self, range: CharRange) -> TextPropertyTable {
        self.slice_copy_text_properties_raw(range)
    }

    fn slice_copy_text_properties_raw(&self, range: CharRange) -> TextPropertyTable {
        if range.is_empty() {
            return TextPropertyTable::new();
        }
        let start = range.start().get();
        let end = range.end().get();

        let mut table = TextPropertyTable::new();
        self.for_each_interval_overlapping(range, |interval_start, node_end, node| {
            let new_start = interval_start.max(start) - start;
            let new_end = node_end.min(end) - start;
            if new_start >= new_end {
                return;
            }
            for (name, value) in plist_pairs(node.plist) {
                table.put_property_in_char_range(
                    CharRange::new(CharPos0::new(new_start), CharPos0::new(new_end)),
                    name,
                    value,
                );
            }
        });
        table
    }

    pub fn append_shifted_at_char_offset(&mut self, other: &TextPropertyTable, offset: CharLen) {
        self.append_shifted_raw(other, offset);
    }

    pub fn append_shifted_at_char_pos(&mut self, other: &TextPropertyTable, pos: CharPos0) {
        self.append_shifted_raw(other, CharLen::new(pos.get()));
    }

    fn append_shifted_raw(&mut self, other: &TextPropertyTable, offset: CharLen) {
        self.mutation_tick += 1;
        self.syntax_prop_tick += 1;
        if other.has_any_syntax_prop_interval() {
            if let Ok(mut guard) = self.syntax_prop_ranges.lock() {
                *guard = (0, Vec::new());
            }
        } else {
            self.syntax_ranges_revalidate();
        }
        // GNU `graft_intervals_into_buffer` (src/intervals.c): the source's
        // intervals are walked ONCE against the target interval under the
        // insertion.  Each source run edge splits the target interval it falls
        // inside (`split_interval_left`, no rebalancing), every covered target
        // interval takes `copy_properties` -- a `Fcopy_sequence` -- of the
        // run's plist, and target boundaries that already lie inside a run
        // survive.  The tree is balanced once afterwards
        // (`buffer_balance_intervals`).
        //
        // This REPLACES the range's properties with the run's plist, preserving
        // the plist ORDER (set, not per-property put which prepends) and NOT
        // merging adjacent intervals, so the insert boundaries that
        // text-property stickiness relies on survive.
        //
        // The grafted buffer intervals never alias the source string's plist
        // cons, and the target interval split at the graft's start re-homes
        // its preceding remainder onto a fresh plist (`copy_properties (under,
        // end_unchanged)`), so a cons already returned to Lisp by
        // `text-properties-at' is detached from the buffer and never mutated
        // in place by a later `put-text-property'.
        //
        // History: this used to split, re-find and re-home PER RUN, each split
        // with a full rebalance climb -- ~3.4K Ir per face run, a third of a
        // 50-line yank in the kill/yank sim window (GNU: ~0.4K per interval).
        let runs = other.intervals.runs();
        for run in &runs {
            self.property_names.observe_plist(run.plist);
        }
        self.intervals.graft_shifted_runs(&runs, offset);
    }

    /// The pre-graft algorithm (split, re-find and re-home per run), kept as
    /// the reference oracle for `graft_shifted_runs`: the two must leave the
    /// same interval partition and plist values.
    #[cfg(test)]
    pub(crate) fn append_shifted_reference_for_test(
        &mut self,
        other: &TextPropertyTable,
        offset: CharLen,
    ) {
        self.mutation_tick += 1;
        self.syntax_prop_tick += 1;
        for run in other.intervals.runs() {
            self.property_names.observe_plist(run.plist);
            let start = run.start().add_len(offset);
            let end = run.end().add_len(offset);
            if start >= end {
                continue;
            }
            self.intervals.ensure_cover(end);
            self.intervals.split_at(start);
            self.intervals.split_at(end);
            if !run.is_empty_plist() {
                self.intervals.rehome_predecessor_plist(start);
            }
            let mut cursor = start;
            while cursor < end {
                let Some((node_start, id)) = self.intervals.find_id(cursor) else {
                    break;
                };
                let node_end = self.intervals.interval_end(node_start, id);
                let fresh = copy_plist_value(run.plist);
                self.intervals.set_node_plist(id, fresh);
                if node_end <= cursor {
                    break;
                }
                cursor = node_end;
            }
        }
    }

    /// The raw plist object stored on the interval containing `pos` (identity
    /// matters for the `copy_properties` aliasing rules).
    #[cfg(test)]
    pub(crate) fn raw_plist_at_for_test(&self, pos: CharPos0) -> Option<Value> {
        self.intervals
            .find_id(pos)
            .map(|(_, id)| self.intervals.nodes[id.0].plist)
    }

    #[cfg(test)]
    pub(crate) fn assert_tree_invariants_for_test(&self) {
        self.intervals.assert_invariants_for_test();
    }

    #[cfg(test)]
    pub(crate) fn tree_max_depth_for_test(&self) -> usize {
        self.intervals.max_depth_for_test()
    }

    /// `(start, end, plist pairs)` of every interval, empty ones included.
    #[cfg(test)]
    pub(crate) fn interval_plist_runs_for_test(&self) -> Vec<(usize, usize, Vec<(Value, Value)>)> {
        self.intervals
            .runs()
            .into_iter()
            .map(|run| (run.start().get(), run.end().get(), plist_pairs(run.plist)))
            .collect()
    }

    pub fn merge_missing_shifted_at_char_offset(
        &mut self,
        other: &TextPropertyTable,
        offset: CharLen,
    ) {
        self.merge_missing_shifted_raw(other, offset);
    }

    fn merge_missing_shifted_raw(&mut self, other: &TextPropertyTable, offset: CharLen) {
        self.mutation_tick += 1;
        self.syntax_prop_tick += 1;
        if other.has_any_syntax_prop_interval() {
            if let Ok(mut guard) = self.syntax_prop_ranges.lock() {
                *guard = (0, Vec::new());
            }
        } else {
            self.syntax_ranges_revalidate();
        }
        let mut target_runs = self.intervals.runs();
        for source in other.intervals.runs() {
            if source.is_empty_plist() {
                continue;
            }
            let shifted_start = source.start().add_len(offset);
            let shifted_end = source.end().add_len(offset);

            Self::ensure_runs_cover(&mut target_runs, shifted_end);
            Self::split_runs_at(&mut target_runs, shifted_start);
            Self::split_runs_at(&mut target_runs, shifted_end);

            for target in &mut target_runs {
                if target.start() < shifted_end && target.end() > shifted_start {
                    for (name, value) in plist_pairs(source.plist) {
                        if plist_value_get(target.plist, name).is_none() {
                            target.plist = plist_value_prepend_pair(target.plist, name, value);
                        }
                    }
                    target.refresh_cache();
                }
            }
        }
        self.replace_runs_preserving_shape(
            target_runs,
            ReplacementPropertyNames::Include(&other.property_names),
        );
    }

    pub fn merge_adjacent_equal_properties_around_char_range(&mut self, range: CharRange) {
        self.merge_adjacent_equal_properties_around(range);
    }

    /// Fuse adjacent intervals with `eq`-equal, non-nil plists whose shared
    /// boundary lies in `[range.start, range.end]` — GNU `merge_interval_left`
    /// applied at the boundaries an edit just created.
    ///
    /// Runs after every inherited-property insert, so it is on the
    /// `self-insert-command` path.  It works on the tree in place, touching
    /// only the intervals whose boundary is inside `range`: O(k log n) for
    /// the k intervals examined.  The algorithm it replaces
    /// ([`Self::merge_adjacent_equal_properties_around_reference_for_test`])
    /// collected every run in the buffer, compared every adjacent pair with
    /// freshly allocated plist vectors, and rebuilt the balanced tree — a
    /// per-keystroke cost proportional to the number of property runs in the
    /// buffer (8% of the Lisp thread on the GUI typing profile of a 258 KB
    /// fontified file).
    fn merge_adjacent_equal_properties_around(&mut self, range: CharRange) {
        self.mutation_tick += 1;
        if range.start() > range.end() {
            return;
        }
        self.intervals.merge_equal_neighbors_in(range);
    }

    /// The pre-local algorithm, kept as the reference oracle for
    /// [`Self::merge_adjacent_equal_properties_around`]: the two must leave
    /// the same interval partition and plist values.
    #[cfg(test)]
    pub(crate) fn merge_adjacent_equal_properties_around_reference_for_test(
        &mut self,
        range: CharRange,
    ) {
        self.mutation_tick += 1;
        let mut runs = self.intervals.runs();
        let start = range.start();
        let end = range.end();
        if start > end || runs.len() < 2 {
            return;
        }

        loop {
            let mut merged = false;
            let mut idx = 0;
            while idx + 1 < runs.len() {
                let left_end = runs[idx].end();
                let right_start = runs[idx + 1].start();
                if left_end != right_start
                    || runs[idx].plist.is_nil()
                    || !plists_equal_eq(
                        &plist_pairs(runs[idx].plist),
                        &plist_pairs(runs[idx + 1].plist),
                    )
                    || left_end < start
                    || right_start > end
                {
                    idx += 1;
                    continue;
                }

                let right_end = runs[idx + 1].end();
                runs[idx].set_end(right_end);
                runs[idx].refresh_cache();
                runs.remove(idx + 1);
                merged = true;
                break;
            }

            if !merged {
                break;
            }
        }
        self.replace_runs_preserving_shape(runs, ReplacementPropertyNames::Preserve);
    }

    pub(crate) fn dump_intervals(&self) -> Vec<PropertyInterval> {
        self.intervals_snapshot()
    }

    #[cfg(test)]
    pub(crate) fn debug_interval_bounds(&self) -> Vec<(usize, usize, bool)> {
        self.intervals
            .runs()
            .into_iter()
            .map(|run| (run.start().get(), run.end().get(), run.is_empty_plist()))
            .collect()
    }

    pub(crate) fn from_dump(intervals: Vec<PropertyInterval>) -> Self {
        Self::from_interval_runs(
            intervals
                .into_iter()
                .filter(|interval| interval.start < interval.end)
                .map(|interval| {
                    IntervalRun::new_in_char_range(
                        CharRange::new(CharPos0::new(interval.start), CharPos0::new(interval.end)),
                        plist_value_from_pairs(&interval.into_plist()),
                    )
                })
                .collect(),
        )
    }

    pub(crate) fn for_each_root(&self, mut f: impl FnMut(Value)) {
        self.intervals.live_plists(self.intervals.root, &mut f);
    }
}

impl Default for TextPropertyTable {
    fn default() -> Self {
        Self::new()
    }
}

impl GcTrace for TextPropertyTable {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        self.for_each_root(|value| roots.push(value));
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[path = "text_props_test.rs"]
mod tests;
