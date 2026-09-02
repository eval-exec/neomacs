//! Lisp value representation and fundamental operations.
//!
//! After the tagged pointer migration, `Value` is a type alias for
//! `TaggedValue`.  This module provides:
//!
//! - The `Value` type alias and re-exports of `ValueKind`, `VecLikeType`
//! - Convenience constructors that allocate on the thread-local heap
//! - Data types: `LambdaData`, `LambdaParams`, `LispHashTable`, `HashKey`, etc.
//! - Equality functions: `eq_value`, `eql_value`, `equal_value`
//! - List helpers: `list_to_vec`, `list_length`
//! - Lexical environment helpers: `lexenv_*`
//! - String text property helpers

use malachite::integer::Integer;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::str::FromStr;
use std::sync::OnceLock;

use num_enum::{IntoPrimitive, TryFromPrimitive};
use rustc_hash::FxHashMap;
use strum::{EnumString, IntoStaticStr};

use super::error::{Flow, signal};
use super::intern::{SymId, intern};
use crate::buffer::text_props::{PropertyInterval, TextPropertyPlistRun, TextPropertyTable};
use crate::buffer::{CharPos0, CharRange, EmacsBytePos};
use crate::heap_types::LispString;
use crate::tagged::gc::{
    HeapWriteKind, MEMORY_USE_COUNT_LEN, MemoryUseCountSlot, note_heap_write, with_tagged_heap,
};
use crate::tagged::header::{
    BufferObj, ByteCodeObj, CHAR_TABLE_TOP_SLOTS, CharTableObj, ConsCell, FontObj, FontObjectData,
    FrameObj, HashTableObj, LambdaObj, LispValueSlice, MacroObj, MarkerObj, ObarrayObj, OverlayObj,
    ProcessObj, RecordObj, SubCharTableObj, SurfaceObj, TimerObj, VectorObj, WindowObj, XwidgetObj,
    XwidgetViewObj,
};
use crate::tagged::mutate;
use crate::tagged::value::{TAG_BITS, TAG_MASK, TaggedValue};

// ---------------------------------------------------------------------------
// The Value type — now a tagged pointer
// ---------------------------------------------------------------------------

/// Runtime Lisp value.
///
/// This is a type alias for `TaggedValue` — a single machine word (8 bytes on
/// 64-bit) encoding type and payload via tag bits.  Pattern matching uses
/// `value.kind()` → `ValueKind`.
pub type Value = TaggedValue;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum FunctionSourceIdentity {
    ByteCode(u64),
}

// Re-export tagged types for downstream use.
pub use crate::tagged::header::VecLikeType;
pub use crate::tagged::value::ValueKind;

// ---------------------------------------------------------------------------
// Data structures (unchanged — not part of the Value enum)
// ---------------------------------------------------------------------------

/// An insertion-order-preserving map from SymId to Value.
///
/// Used for lexical and dynamic environment frames where iteration order must
/// match the original binding order. This is critical for oclosure
/// compatibility: `oclosure--copy` reads the closure's env via `aref` and
/// pairs variables positionally with new arg values. A `HashMap` loses
/// insertion order and causes wrong variable-to-value bindings.
#[derive(Debug, Clone)]
pub struct OrderedSymMap {
    entries: Vec<(SymId, Value)>,
}

impl PartialEq for OrderedSymMap {
    fn eq(&self, other: &Self) -> bool {
        if self.entries.len() != other.entries.len() {
            return false;
        }
        self.entries
            .iter()
            .zip(other.entries.iter())
            .all(|((k1, v1), (k2, v2))| k1 == k2 && eq_value(v1, v2))
    }
}

impl Default for OrderedSymMap {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderedSymMap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            entries: Vec::with_capacity(cap),
        }
    }

    pub fn get(&self, key: &SymId) -> Option<&Value> {
        self.entries
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    pub fn insert(&mut self, key: SymId, value: Value) {
        if let Some(entry) = self.entries.iter_mut().rev().find(|(k, _)| *k == key) {
            entry.1 = value;
        } else {
            self.entries.push((key, value));
        }
    }

    pub fn contains_key(&self, key: &SymId) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.entries.iter().map(|(_, v)| v)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.entries.iter_mut().map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SymId, &Value)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Reconstruct from a vec of entries (for pdump load).
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn from_entries(entries: Vec<(SymId, Value)>) -> Self {
        Self { entries }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RuntimeBindingValue {
    Bound(Value),
    Void,
}

impl RuntimeBindingValue {
    pub fn bound(value: Value) -> Self {
        Self::Bound(value)
    }

    pub fn as_value(self) -> Option<Value> {
        match self {
            Self::Bound(value) => Some(value),
            Self::Void => None,
        }
    }

    pub fn as_ref(&self) -> Option<&Value> {
        match self {
            Self::Bound(value) => Some(value),
            Self::Void => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrderedRuntimeBindingMap {
    entries: Vec<(SymId, RuntimeBindingValue)>,
}

impl PartialEq for OrderedRuntimeBindingMap {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

impl Default for OrderedRuntimeBindingMap {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderedRuntimeBindingMap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            entries: Vec::with_capacity(cap),
        }
    }

    pub fn get(&self, key: &SymId) -> Option<&Value> {
        self.get_binding(key).and_then(RuntimeBindingValue::as_ref)
    }

    pub fn get_binding(&self, key: &SymId) -> Option<&RuntimeBindingValue> {
        self.entries
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    pub fn insert(&mut self, key: SymId, value: Value) {
        self.insert_binding(key, RuntimeBindingValue::Bound(value));
    }

    pub fn insert_binding(&mut self, key: SymId, value: RuntimeBindingValue) {
        if let Some(entry) = self.entries.iter_mut().rev().find(|(k, _)| *k == key) {
            entry.1 = value;
        } else {
            self.entries.push((key, value));
        }
    }

    pub fn set_void(&mut self, key: SymId) {
        self.insert_binding(key, RuntimeBindingValue::Void);
    }

    pub fn contains_key(&self, key: &SymId) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.entries.iter().filter_map(|(_, v)| v.as_ref())
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.entries.iter_mut().filter_map(|(_, v)| match v {
            RuntimeBindingValue::Bound(value) => Some(value),
            RuntimeBindingValue::Void => None,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SymId, &RuntimeBindingValue)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn from_entries(entries: Vec<(SymId, RuntimeBindingValue)>) -> Self {
        Self { entries }
    }
}

// ---------------------------------------------------------------------------
// Allocation statistics counters
// ---------------------------------------------------------------------------

thread_local! {
    static THREAD_LOCAL_ALLOCATION_COUNTS: Cell<[u64; MEMORY_USE_COUNT_LEN]> =
        const { Cell::new([0; MEMORY_USE_COUNT_LEN]) };

    #[cfg(test)]
    static BYTECODE_DATA_ACCESS_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_bytecode_data_access_count() {
    BYTECODE_DATA_ACCESS_COUNT.with(|count| count.set(0));
}

/// See [`TaggedValue::bytecode_interactive_probe`].
pub(crate) struct BytecodeInteractiveProbe {
    pub(crate) slot_count: usize,
    pub(crate) interactive: Option<Value>,
    pub(crate) doc_form: Option<Value>,
}

#[cfg(test)]
pub(crate) fn bytecode_data_access_count() -> usize {
    BYTECODE_DATA_ACCESS_COUNT.with(Cell::get)
}

#[inline]
fn add_wrapping(counter: MemoryUseCountSlot, delta: u64) {
    THREAD_LOCAL_ALLOCATION_COUNTS.with(|counts| {
        let mut values = counts.get();
        let slot = counter.index();
        values[slot] = values[slot].wrapping_add(delta);
        counts.set(values);
    });
}

fn as_neovm_int(value: u64) -> i64 {
    value as i64
}

/// A borrow was taken of a string object the collector has already reclaimed.
///
/// The check that reaches here is the string-side twin of `deadp`
/// (`src/alloc.c:425-429`) reading back `dead_object ()`: GNU nulls
/// `s->u.s.data` in `sweep_strings` "so that we know it's free"
/// (`src/alloc.c:1878-1882`), and `LispString::drop` now does the same. Its
/// job is DIVERGENCES.md 161 §6's: make the crime legible at the scene rather
/// than thirty frames downstream in the printer or the symbol resolver.
///
/// Deliberately loud. A defensive `None` here would trade a crash for a quiet
/// wrong answer over memory that is still corrupt.
#[cold]
#[inline(never)]
fn reclaimed_string_borrowed(ptr: *const crate::tagged::header::StringObj) -> ! {
    panic!(
        "use-after-free: borrowed a string object the collector has reclaimed \
         (StringObj at {ptr:?} has a null data pointer, GNU sweep_strings' \
         free marker). A `&LispString` outlived its object — see \
         `Value::as_lisp_string` and DIVERGENCES.md 163."
    )
}

// ---------------------------------------------------------------------------
// String text properties
// ---------------------------------------------------------------------------

fn string_text_props(value: Value) -> Option<&'static TextPropertyTable> {
    let ptr = value.as_string_ptr()?;
    Some(unsafe { (*ptr).data.intervals() })
}

/// String text properties now live on the string object itself.
///
/// Heap resets automatically discard them with the owning string, so there is
/// no side table to clear anymore.
pub fn reset_string_text_properties() {}

/// String text property GC roots are traced from `StringObj` during heap mark.
pub fn collect_string_text_prop_gc_roots(_roots: &mut Vec<Value>) {}

pub fn set_string_text_properties_table_for_value(value: Value, table: TextPropertyTable) {
    if table.is_empty() {
        // Normalize "no properties" to a NULL interval pointer instead of
        // Some(empty-table): every string property write-back funnels through
        // here (`save_string_props_for_value`), so a removal that empties the
        // table (remove-text-properties, set-text-properties nil over a
        // subrange, ...) nulls the field like GNU's interval-free state. This
        // keeps such strings eligible for the concurrent GC's interval-free
        // claim instead of deferring them to the STW drain forever; readers
        // already treat an empty table and no table identically
        // (`get_string_text_properties_table_for_value` returns None for both).
        clear_string_text_properties_for_value(value);
        return;
    }
    let _ = mutate::with_string_text_props_mut(value, |props| {
        *props = table;
    });
}

pub fn clear_string_text_properties_for_value(value: Value) {
    let _ = mutate::with_lisp_string_mut(value, |s| s.clear_intervals());
}

pub fn string_has_text_property_interval_tree(value: Value) -> bool {
    let Some(ptr) = value.as_string_ptr() else {
        return false;
    };
    unsafe { (*ptr).data.has_intervals() }
}

pub fn set_string_text_properties_for_value(value: Value, runs: Vec<StringTextPropertyRun>) {
    if let Some(table) = bulk_string_text_property_table(&runs) {
        set_string_text_properties_table_for_value(value, table);
        return;
    }

    let mut intervals = Vec::with_capacity(runs.len());
    for run in runs {
        if let Some(items) = list_to_vec(&run.plist) {
            let mut properties = HashMap::new();
            let mut key_order = Vec::new();
            for chunk in items.chunks(2) {
                if chunk.len() == 2 {
                    if !key_order.iter().any(|seen| eq_value(seen, &chunk[0])) {
                        key_order.push(chunk[0]);
                    }
                    properties.insert(chunk[0], chunk[1]);
                }
            }
            if !properties.is_empty() {
                intervals.push(PropertyInterval {
                    start: run.start,
                    end: run.end,
                    properties,
                    key_order,
                });
            }
        }
    }
    let table = TextPropertyTable::from_dump(intervals);
    set_string_text_properties_table_for_value(value, table);
}

fn bulk_string_text_property_table(runs: &[StringTextPropertyRun]) -> Option<TextPropertyTable> {
    let mut plist_runs = Vec::with_capacity(runs.len());
    for run in runs {
        if run.start >= run.end {
            continue;
        }
        let mut plist = Vec::new();
        if let Some(items) = list_to_vec(&run.plist) {
            for chunk in items.chunks(2) {
                if let [key, value] = chunk {
                    string_text_prop_plist_put(&mut plist, *key, *value);
                }
            }
        }
        plist_runs.push(TextPropertyPlistRun::new(
            CharRange::new(CharPos0::new(run.start), CharPos0::new(run.end)),
            plist,
        ));
    }

    if plist_runs.is_empty() {
        return Some(TextPropertyTable::new());
    }

    let mut sorted_bounds: Vec<CharRange> = plist_runs.iter().map(|run| run.range()).collect();
    sorted_bounds.sort_unstable_by_key(|range| (range.start(), range.end()));
    if sorted_bounds
        .windows(2)
        .any(|window| window[1].start() < window[0].end())
    {
        return None;
    }

    Some(TextPropertyTable::from_plist_runs(plist_runs))
}

fn string_text_prop_plist_put(plist: &mut Vec<(Value, Value)>, key: Value, value: Value) {
    for (name, existing) in plist.iter_mut() {
        if eq_value(name, &key) {
            *existing = value;
            return;
        }
    }
    // Preserve source order. The reader applies the whole property list at
    // once with `Fset_text_properties` semantics (GNU lread.c
    // `string_props_from_rev_list`), which keeps the plist in the written
    // order. Appending (not prepending) makes `text-properties-at` on a read
    // `#("..." S E (k1 v1 k2 v2 ...))` return `(k1 v1 k2 v2 ...)` like GNU.
    plist.push((key, value));
}

pub fn get_string_text_properties_for_value(value: Value) -> Option<Vec<StringTextPropertyRun>> {
    let table = string_text_props(value)?;
    if table.is_empty() {
        return None;
    }
    let mut runs = Vec::new();
    for interval in table.intervals_snapshot() {
        if interval.properties.is_empty() {
            continue;
        }
        let mut plist_items = Vec::new();
        for (key, val) in interval.ordered_properties() {
            plist_items.push(key);
            plist_items.push(*val);
        }
        runs.push(StringTextPropertyRun {
            start: interval.start,
            end: interval.end,
            plist: Value::list(plist_items),
        });
    }
    if runs.is_empty() { None } else { Some(runs) }
}

pub fn string_has_text_properties_for_value(value: Value) -> bool {
    string_text_props(value).is_some_and(|table| !table.is_empty())
}

pub fn get_string_text_properties_table_for_value(value: Value) -> Option<TextPropertyTable> {
    let table = string_text_props(value)?;
    if table.is_empty() {
        None
    } else {
        Some(table.clone())
    }
}

/// Borrow a string's live interval table for a read-only comparison, with the
/// same empty→None normalization as `get_string_text_properties_table_for_value`
/// but WITHOUT cloning the tree.  `equal-including-properties` only reads the
/// tables (no allocation / no GC in the walk), so the borrow stays valid for the
/// comparison; the clone was pure waste on every propertied-string compare.
fn string_text_props_nonempty(value: Value) -> Option<&'static TextPropertyTable> {
    let table = string_text_props(value)?;
    if table.is_empty() { None } else { Some(table) }
}

/// `equal-including-properties` string-interval comparison.  Two propertyless
/// strings are trivially property-equal, so skip the interval walk entirely in
/// that (dominant) case; otherwise compare the borrowed tables the way GNU's
/// `compare_string_intervals` does.  Semantically identical to calling
/// `TextPropertyTable::equal_including_property_values` with both tables — the
/// both-`None` walk already returns `true` — just cheaper and allocation-free.
fn string_intervals_equal_including_values(left: Value, right: Value, len: usize) -> bool {
    let left_props = string_text_props_nonempty(left);
    let right_props = string_text_props_nonempty(right);
    if left_props.is_none() && right_props.is_none() {
        return true;
    }
    TextPropertyTable::equal_including_property_values(left_props, right_props, len)
}

pub fn get_string_text_properties_interval_table_for_value(
    value: Value,
) -> Option<TextPropertyTable> {
    if !string_has_text_property_interval_tree(value) {
        return None;
    }
    Some(string_text_props(value)?.clone())
}

/// A string text property run used by printed propertized-string literals.
/// Bounds are 0-based character indices, as in GNU string intervals.
#[derive(Clone, Debug, PartialEq)]
pub struct StringTextPropertyRun {
    pub start: usize,
    pub end: usize,
    pub plist: Value,
}

/// Snapshot of a cons cell's car and cdr values (legacy compatibility).
pub struct ConsSnapshot {
    pub car: Value,
    pub cdr: Value,
}

/// Allocate a fresh float identity (stub — float identity is pointer-based).
pub fn next_float_id() -> u32 {
    0
}

// ---------------------------------------------------------------------------
// LambdaData, LambdaParams
// ---------------------------------------------------------------------------

/// Shared representation for lambda and macro bodies.
#[derive(Clone, Debug)]
pub struct LambdaData {
    pub params: LambdaParams,
    /// Body forms as a list of Values (Lisp forms to evaluate).
    pub body: Vec<Value>,
    /// For lexical closures: captured environment as a cons alist
    /// mirroring GNU Emacs's `Vinternal_interpreter_environment`.
    pub env: Option<Value>,
    pub docstring: Option<LispString>,
    /// Slot 4 in the closure vector: the `:documentation` form result.
    pub doc_form: Option<Value>,
    /// Slot 5 in GNU Emacs's closure vector: the interactive specification.
    pub interactive: Option<Value>,
}

/// Describes a lambda parameter list including &optional and &rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LambdaParams {
    pub required: Vec<SymId>,
    pub optional: Vec<SymId>,
    pub rest: Option<SymId>,
}

impl LambdaParams {
    pub fn simple(names: Vec<SymId>) -> Self {
        Self {
            required: names,
            optional: Vec::new(),
            rest: None,
        }
    }

    pub fn min_arity(&self) -> usize {
        self.required.len()
    }

    pub fn max_arity(&self) -> Option<usize> {
        if self.rest.is_some() {
            None
        } else {
            Some(self.required.len() + self.optional.len())
        }
    }
}

use crate::tagged::header::{
    CLOSURE_ARGLIST, CLOSURE_CODE, CLOSURE_CONSTANTS, CLOSURE_DOC_STRING, CLOSURE_INTERACTIVE,
};

impl LambdaData {
    /// Convert LambdaData to a GNU-compatible closure slot vector.
    ///
    /// Layout: [arglist, body, env, depth, docstring, interactive]
    /// All slots are GC-managed Values.
    pub fn to_closure_slots(&self) -> Vec<Value> {
        // Slot 0: arglist as Lisp list
        let arglist = crate::emacs_core::builtins::lambda_params_to_value(&self.params);

        // Slot 1: body as Lisp list of forms
        let body = Value::list(self.body.clone());

        // Slot 2: lexical environment (or nil for dynamic)
        let env = match self.env {
            Some(env_val) if env_val.is_nil() => Value::list(vec![Value::T]),
            Some(env_val) => env_val,
            None => Value::NIL,
        };

        // Slot 4: docstring
        let doc = self
            .doc_form
            .or_else(|| {
                self.docstring
                    .as_ref()
                    .map(|d| Value::heap_string(d.clone()))
            })
            .unwrap_or(Value::NIL);

        let mut slots = vec![arglist, body, env];
        if self.interactive.is_some() || !doc.is_nil() {
            // Slot 3: stack depth (nil for interpreted)
            slots.push(Value::NIL);
            slots.push(doc);
            if let Some(interactive) = self.interactive {
                // Slot 5: interactive spec.  Presence is significant even
                // when the value is nil.
                slots.push(interactive);
            }
        }
        slots
    }
}

// ---------------------------------------------------------------------------
// LispHashTable, HashKey
// ---------------------------------------------------------------------------

/// Hash table with configurable test function.
#[derive(Clone, Debug)]
pub struct LispHashTable {
    pub test: HashTableTest,
    pub test_name: Option<SymId>,
    pub user_cmp_function: Option<Value>,
    pub user_hash_function: Option<Value>,
    pub mutable: bool,
    pub size: i64,
    pub weakness: Option<HashTableWeakness>,
    pub rehash_size: f64,
    pub rehash_threshold: f64,
    pub data: HashTableStorage,
}

#[derive(Clone, Copy, Debug)]
pub struct HashTableEntry {
    pub key: Value,
    pub value: Value,
}

/// Compact backing store for a Lisp hash table.
///
/// The hash index owns each (potentially structural) [`HashKey`] exactly once.
/// Its value is a stable slot number; slot order provides GNU-compatible
/// iteration while storing only the original Lisp key and value. This keeps
/// indexing, key snapshots, insertion order, deletion holes, and slot reuse
/// behind one interface instead of mirroring every key across five containers.
#[derive(Clone, Debug, Default)]
pub struct HashTableStorage {
    index: FxHashMap<HashKey, usize>,
    slots: Vec<Option<HashTableEntry>>,
    free_slots: Vec<usize>,
    /// Dump entries not yet hydrated into `index`/`slots` (GNU pdumper's
    /// hash_rehash_needed, lazily: most loaded tables are never touched at
    /// startup, so the loader parks decoded entries here and the FIRST
    /// access through `Value::as_hash_table` / `with_hash_table_mut`
    /// hydrates - those two raw-pointer choke points are the ONLY ways the
    /// engine reaches a table, so no interior mutability is needed. The GC
    /// enumerations that bypass them branch on [`Self::pending_entries`].
    /// Weak tables are hydrated eagerly at load so the weak sweep never
    /// sees a pending table.
    pending: Option<Box<Vec<(HashKey, Value, Option<Value>)>>>,
}

pub struct HashTableIter<'a> {
    index: std::collections::hash_map::Iter<'a, HashKey, usize>,
    slots: &'a [Option<HashTableEntry>],
}

impl<'a> Iterator for HashTableIter<'a> {
    type Item = (&'a HashKey, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        for (key, &slot) in self.index.by_ref() {
            if let Some(entry) = self.slots.get(slot).and_then(Option::as_ref) {
                return Some((key, &entry.value));
            }
        }
        None
    }
}

impl<'a> IntoIterator for &'a HashTableStorage {
    type Item = (&'a HashKey, &'a Value);
    type IntoIter = HashTableIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl std::ops::Index<&HashKey> for HashTableStorage {
    type Output = Value;

    fn index(&self, key: &HashKey) -> &Self::Output {
        self.get(key).expect("no entry found for key")
    }
}

impl HashTableStorage {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            index: FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
            slots: Vec::with_capacity(capacity),
            free_slots: Vec::new(),
            pending: None,
        }
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    fn set_pending(&mut self, entries: Vec<(HashKey, Value, Option<Value>)>) {
        self.pending = Some(Box::new(entries));
    }

    fn take_pending(&mut self) -> Option<Vec<(HashKey, Value, Option<Value>)>> {
        self.pending.take().map(|b| *b)
    }

    /// Parked dump entries awaiting hydration, for the GC enumerations that
    /// reach storage without passing an accessor choke point. Each tuple is
    /// (hash key, value, key snapshot when the key object differs).
    #[inline]
    pub fn pending_entries(&self) -> Option<&[(HashKey, Value, Option<Value>)]> {
        self.pending.as_deref().map(|v| v.as_slice())
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.index.capacity()
    }

    pub fn contains_key(&self, key: &HashKey) -> bool {
        self.index.contains_key(key)
    }

    pub fn get(&self, key: &HashKey) -> Option<&Value> {
        let slot = *self.index.get(key)?;
        self.slots
            .get(slot)
            .and_then(Option::as_ref)
            .map(|entry| &entry.value)
    }

    pub fn get_mut(&mut self, key: &HashKey) -> Option<&mut Value> {
        let slot = *self.index.get(key)?;
        self.slots
            .get_mut(slot)
            .and_then(Option::as_mut)
            .map(|entry| &mut entry.value)
    }

    pub fn key_snapshot(&self, key: &HashKey) -> Option<&Value> {
        let slot = *self.index.get(key)?;
        self.slots
            .get(slot)
            .and_then(Option::as_ref)
            .map(|entry| &entry.key)
    }

    pub fn replace_key_snapshot(&mut self, key: &HashKey, key_value: Value) {
        let Some(&slot) = self.index.get(key) else {
            return;
        };
        self.slots[slot]
            .as_mut()
            .expect("hash index points to an empty entry slot")
            .key = key_value;
    }

    pub fn insert(&mut self, hash_key: HashKey, key: Value, value: Value) -> Option<Value> {
        if let Some(&slot) = self.index.get(&hash_key) {
            let entry = self.slots[slot]
                .as_mut()
                .expect("hash index points to an empty entry slot");
            return Some(std::mem::replace(&mut entry.value, value));
        }

        let slot = if let Some(slot) = self.free_slots.pop() {
            debug_assert!(self.slots[slot].is_none());
            self.slots[slot] = Some(HashTableEntry { key, value });
            slot
        } else {
            let slot = self.slots.len();
            self.slots.push(Some(HashTableEntry { key, value }));
            slot
        };
        self.index.insert(hash_key, slot);
        None
    }

    pub fn insert_replacing_key(
        &mut self,
        hash_key: HashKey,
        key: Value,
        value: Value,
    ) -> Option<Value> {
        if let Some(&slot) = self.index.get(&hash_key) {
            let entry = self.slots[slot]
                .as_mut()
                .expect("hash index points to an empty entry slot");
            entry.key = key;
            return Some(std::mem::replace(&mut entry.value, value));
        }
        self.insert(hash_key, key, value)
    }

    pub fn remove(&mut self, key: &HashKey) -> Option<Value> {
        let slot = self.index.remove(key)?;
        let entry = self.slots[slot]
            .take()
            .expect("hash index points to an empty entry slot");
        self.free_slots.push(slot);
        Some(entry.value)
    }

    pub fn clear(&mut self) {
        self.index.clear();
        self.slots.clear();
        self.free_slots.clear();
    }

    pub fn reserve(&mut self, additional: usize) {
        self.index.reserve(additional);
        self.slots.reserve(additional);
    }

    pub fn iter(&self) -> HashTableIter<'_> {
        HashTableIter {
            index: self.index.iter(),
            slots: &self.slots,
        }
    }

    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.slots
            .iter()
            .filter_map(Option::as_ref)
            .map(|entry| &entry.value)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.slots
            .iter_mut()
            .filter_map(Option::as_mut)
            .map(|entry| &mut entry.value)
    }

    pub fn key_snapshots(&self) -> impl Iterator<Item = &Value> {
        self.slots
            .iter()
            .filter_map(Option::as_ref)
            .map(|entry| &entry.key)
    }

    pub fn entries_in_slot_order(&self) -> impl Iterator<Item = &HashTableEntry> {
        self.slots.iter().filter_map(Option::as_ref)
    }

    pub fn entry_at(&self, slot: usize) -> Option<&HashTableEntry> {
        self.slots.get(slot).and_then(Option::as_ref)
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn live_hash_keys_in_slot_order(&self) -> Vec<&HashKey> {
        let mut keys = vec![None; self.slots.len()];
        for (key, &slot) in &self.index {
            if self.slots.get(slot).is_some_and(Option::is_some) {
                keys[slot] = Some(key);
            }
        }
        keys.into_iter().flatten().collect()
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&HashKey, &mut Value) -> bool) {
        let mut removed = Vec::new();
        for (key, &slot) in &self.index {
            let entry = self.slots[slot]
                .as_mut()
                .expect("hash index points to an empty entry slot");
            if !keep(key, &mut entry.value) {
                removed.push(key.clone());
            }
        }
        for key in removed {
            let _ = self.remove(&key);
        }
    }

    pub fn replace_pointer_key(&mut self, old_ptr: usize, new_ptr: usize, new_key: Value) {
        let old = HashKey::Ptr(old_ptr);
        let Some(slot) = self.index.remove(&old) else {
            return;
        };
        self.index.insert(HashKey::Ptr(new_ptr), slot);
        self.slots[slot]
            .as_mut()
            .expect("hash index points to an empty entry slot")
            .key = new_key;
    }

    pub(crate) fn known_storage_bytes(&self) -> usize {
        self.index
            .capacity()
            .saturating_mul(size_of::<(HashKey, usize)>())
            .saturating_add(
                self.pending
                    .as_deref()
                    .map_or(0, |p| p.capacity())
                    .saturating_mul(size_of::<(HashKey, Value, Option<Value>)>()),
            )
            .saturating_add(
                self.slots
                    .capacity()
                    .saturating_mul(size_of::<Option<HashTableEntry>>()),
            )
            .saturating_add(
                self.free_slots
                    .capacity()
                    .saturating_mul(size_of::<usize>()),
            )
    }
}

/// Standard hash-table tests. The numeric codes mirror GNU
/// `hash_table_std_test_t` (`src/lisp.h`): `eql=0`, `eq=1`, `equal=2`.
#[repr(u8)]
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr, IntoPrimitive, TryFromPrimitive,
)]
#[strum(serialize_all = "kebab-case")]
pub enum HashTableTest {
    Eq = 1,
    Eql = 0,
    Equal = 2,
}

impl HashTableTest {
    pub fn from_gnu_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }

    pub fn gnu_code(self) -> u8 {
        self.into()
    }

    pub fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn from_symbol_value(value: &Value) -> Option<Self> {
        Self::from_symbol_name(value.as_symbol_name()?)
    }

    pub fn name(self) -> &'static str {
        self.into()
    }
}

/// Weak hash-table modes. GNU `Weak_None=0` is represented by
/// `Option<HashTableWeakness>::None`; the enum covers the non-nil Lisp
/// weakness symbols.
#[repr(u8)]
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr, IntoPrimitive, TryFromPrimitive,
)]
#[strum(serialize_all = "kebab-case")]
pub enum HashTableWeakness {
    Key = 1,
    Value = 2,
    KeyOrValue = 3,
    KeyAndValue = 4,
}

impl HashTableWeakness {
    pub fn from_gnu_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }

    pub fn option_from_gnu_code(code: u8) -> Option<Option<Self>> {
        if code == 0 {
            Some(None)
        } else {
            Self::from_gnu_code(code).map(Some)
        }
    }

    pub fn gnu_code(self) -> u8 {
        self.into()
    }

    pub fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn from_symbol_value(value: &Value) -> Option<Self> {
        Self::from_symbol_name(value.as_symbol_name()?)
    }

    pub fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(prefix = ":", serialize_all = "kebab-case")]
pub(crate) enum HashTableMakeKeyword {
    Test,
    Size,
    Purecopy,
    RehashSize,
    RehashThreshold,
    Weakness,
}

impl HashTableMakeKeyword {
    pub(crate) fn from_symbol_name(name: &str) -> Option<Self> {
        name.strip_prefix(':')?.parse().ok()
    }

    pub(crate) fn from_symbol_value(value: &Value) -> Option<Self> {
        Self::from_symbol_name(value.as_symbol_name()?)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum HashTableLiteralKey {
    Test,
    Size,
    Purecopy,
    RehashSize,
    RehashThreshold,
    Weakness,
    Data,
}

impl HashTableLiteralKey {
    pub(crate) fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub(crate) fn from_symbol_value(value: &Value) -> Option<Self> {
        Self::from_symbol_name(value.as_symbol_name()?)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn name(self) -> &'static str {
        self.into()
    }
}

/// Key type that supports hashing for `eq`, `eql`, and `equal` tests.
#[derive(Clone, Debug)]
pub enum HashKey {
    Nil,
    True,
    Int(i64),
    /// Value-based key for bignums, holding the canonical two's-complement
    /// little-endian limbs. Two numerically-equal bignums (which are `eql`
    /// and `equal`) yield equal limb vectors, so they collide in the table
    /// instead of hashing by heap address.
    Bignum(Box<[u64]>),
    Float(u64),
    FloatEq(u64, u32),
    Symbol(SymId),
    Keyword(SymId),
    Char(char),
    Window(u64),
    Frame(u64),
    /// Pointer identity for eq hash tables (tagged pointer bits).
    Ptr(usize),
    /// Structural cons key for `equal`-test hash tables.
    EqualCons(Box<HashKey>, Box<HashKey>),
    /// Structural pseudovector key for `equal`-test hash tables.
    EqualVec(Box<[HashKey]>),
    /// Structural marker key for `equal`-test hash tables.
    Marker(Box<(Option<u64>, EmacsBytePos)>),
    /// Structural overlay key for `equal`-test hash tables.
    Overlay(Box<(Option<u64>, usize, usize, HashKey)>),
    /// Compact structural key for bool-vectors whose bits fit in one word.
    BoolVec(Box<(usize, u128)>),
    /// Structural key for symbol-with-pos objects when they are not transparent.
    SymbolWithPos(Box<HashKey>, Box<HashKey>),
    /// Back-reference marker used when structural objects recurse.
    Cycle(u32),
    /// Owned textual key used for structural hashing.
    Text(Box<str>),
}

impl std::hash::Hash for HashKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let tag: u8 = match self {
            HashKey::Nil => 0,
            HashKey::True => 1,
            HashKey::Int(_) => 2,
            HashKey::Bignum(_) => 21,
            HashKey::Float(_) => 3,
            HashKey::FloatEq(_, _) => 4,
            HashKey::Symbol(_) => 5,
            HashKey::Char(_) => 7,
            HashKey::Window(_) => 8,
            HashKey::Frame(_) => 9,
            HashKey::Ptr(_) => 10,
            HashKey::EqualCons(_, _) => 12,
            HashKey::EqualVec(_) => 13,
            HashKey::Keyword(_) => 14,
            HashKey::Cycle(_) => 15,
            HashKey::Text(_) => 16,
            HashKey::SymbolWithPos(_, _) => 17,
            HashKey::Marker(_) => 18,
            HashKey::Overlay(_) => 19,
            HashKey::BoolVec(_) => 20,
        };
        tag.hash(state);
        match self {
            HashKey::Nil | HashKey::True => {}
            HashKey::Int(n) => n.hash(state),
            HashKey::Bignum(limbs) => limbs.hash(state),
            HashKey::Float(bits) => bits.hash(state),
            HashKey::FloatEq(bits, id) => {
                bits.hash(state);
                id.hash(state);
            }
            HashKey::Symbol(id) | HashKey::Keyword(id) => id.hash(state),
            HashKey::Char(c) => c.hash(state),
            HashKey::Window(id) | HashKey::Frame(id) => id.hash(state),
            HashKey::Ptr(p) => p.hash(state),
            HashKey::EqualCons(car, cdr) => {
                car.hash(state);
                cdr.hash(state);
            }
            HashKey::EqualVec(items) => {
                items.len().hash(state);
                for item in items {
                    item.hash(state);
                }
            }
            HashKey::Marker(parts) => {
                parts.0.hash(state);
                parts.1.hash(state);
            }
            HashKey::Overlay(parts) => {
                parts.0.hash(state);
                parts.1.hash(state);
                parts.2.hash(state);
                parts.3.hash(state);
            }
            HashKey::BoolVec(parts) => {
                parts.0.hash(state);
                parts.1.hash(state);
            }
            HashKey::SymbolWithPos(sym, pos) => {
                sym.hash(state);
                pos.hash(state);
            }
            HashKey::Cycle(index) => index.hash(state),
            HashKey::Text(text) => text.hash(state),
        }
    }
}

impl PartialEq for HashKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (HashKey::Nil, HashKey::Nil) | (HashKey::True, HashKey::True) => true,
            (HashKey::Int(a), HashKey::Int(b)) => a == b,
            (HashKey::Bignum(a), HashKey::Bignum(b)) => a == b,
            (HashKey::Float(a), HashKey::Float(b)) => a == b,
            (HashKey::FloatEq(a, id_a), HashKey::FloatEq(b, id_b)) => a == b && id_a == id_b,
            (HashKey::Symbol(a), HashKey::Symbol(b)) => a == b,
            (HashKey::Keyword(a), HashKey::Keyword(b)) => a == b,
            (HashKey::Char(a), HashKey::Char(b)) => a == b,
            (HashKey::Window(a), HashKey::Window(b)) | (HashKey::Frame(a), HashKey::Frame(b)) => {
                a == b
            }
            (HashKey::Ptr(a), HashKey::Ptr(b)) => a == b,
            (HashKey::EqualCons(a_car, a_cdr), HashKey::EqualCons(b_car, b_cdr)) => {
                a_car == b_car && a_cdr == b_cdr
            }
            (HashKey::EqualVec(a), HashKey::EqualVec(b)) => a == b,
            (HashKey::Marker(a), HashKey::Marker(b)) => a == b,
            (HashKey::Overlay(a), HashKey::Overlay(b)) => a == b,
            (HashKey::BoolVec(a), HashKey::BoolVec(b)) => a == b,
            (HashKey::SymbolWithPos(a_sym, a_pos), HashKey::SymbolWithPos(b_sym, b_pos)) => {
                a_sym == b_sym && a_pos == b_pos
            }
            (HashKey::Cycle(a), HashKey::Cycle(b)) => a == b,
            (HashKey::Text(a), HashKey::Text(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for HashKey {}

impl HashKey {
    /// Create a string hash key by allocating on the heap.
    // This constructor accepts owned or borrowed text, unlike `FromStr`, and
    // is an established public helper used by table clients.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: impl Into<String>) -> Self {
        // For `equal` hash tables, use text content directly
        HashKey::Text(s.into().into_boxed_str())
    }
}

impl LispHashTable {
    pub fn new(test: HashTableTest) -> Self {
        Self::new_with_options(test, 0, None, 1.5, 0.8125)
    }

    pub(crate) fn new_unpopulated_with_options(
        test: HashTableTest,
        size: i64,
        weakness: Option<HashTableWeakness>,
        rehash_size: f64,
        rehash_threshold: f64,
    ) -> Self {
        Self {
            test,
            test_name: None,
            user_cmp_function: None,
            user_hash_function: None,
            mutable: true,
            size,
            weakness,
            rehash_size,
            rehash_threshold,
            data: HashTableStorage::default(),
        }
    }

    pub fn new_with_options(
        test: HashTableTest,
        size: i64,
        weakness: Option<HashTableWeakness>,
        rehash_size: f64,
        rehash_threshold: f64,
    ) -> Self {
        Self {
            test,
            test_name: None,
            user_cmp_function: None,
            user_hash_function: None,
            mutable: true,
            size,
            weakness,
            rehash_size,
            rehash_threshold,
            data: HashTableStorage::with_capacity(size.max(0) as usize),
        }
    }

    pub fn insert(&mut self, hash_key: HashKey, key: Value, value: Value) -> Option<Value> {
        self.data.insert(hash_key, key, value)
    }

    /// Insert or update an entry while replacing its original-key snapshot.
    /// Used by internal caches whose key value is already canonical.
    pub fn upsert_iterable(&mut self, hash_key: HashKey, key_value: Value, value: Value) {
        self.data.insert_replacing_key(hash_key, key_value, value);
    }

    pub fn key_snapshot(&self, key: &HashKey) -> Option<&Value> {
        self.data.key_snapshot(key)
    }

    pub fn replace_key_snapshot(&mut self, key: &HashKey, key_value: Value) {
        self.data.replace_key_snapshot(key, key_value);
    }

    pub fn key_snapshots(&self) -> impl Iterator<Item = &Value> {
        self.data.key_snapshots()
    }

    pub fn entries_in_slot_order(&self) -> impl Iterator<Item = &HashTableEntry> {
        self.data.entries_in_slot_order()
    }

    pub fn entry_at(&self, slot: usize) -> Option<&HashTableEntry> {
        self.data.entry_at(slot)
    }

    pub fn entry_slot_count(&self) -> usize {
        self.data.slot_count()
    }

    pub fn replace_pointer_key(&mut self, old_ptr: usize, new_ptr: usize, new_key: Value) {
        self.data.replace_pointer_key(old_ptr, new_ptr, new_key);
    }

    /// Rebuild from insertion-ordered `(hash_key, value, key_snapshot)`
    /// triples: one insert (one hash) per entry, no temporary maps. The
    /// pdump loader's path — its dump format stores entries pre-sorted.
    pub fn rebuild_from_ordered_entries(&mut self, entries: Vec<(HashKey, Value, Option<Value>)>) {
        self.data.clear();
        self.data.reserve(entries.len());
        for (hash_key, value, snapshot) in entries {
            let key = snapshot.unwrap_or(value);
            self.insert(hash_key, key, value);
        }
    }

    /// Park decoded dump entries for lazy hydration (see
    /// `HashTableStorage::pending`). The table must be otherwise empty.
    pub fn set_pending_dump_entries(&mut self, entries: Vec<(HashKey, Value, Option<Value>)>) {
        debug_assert!(self.data.is_empty() && self.data.pending_entries().is_none());
        self.data.set_pending(entries);
    }

    /// True when parked dump entries have not been hydrated yet.
    #[inline]
    pub fn needs_hydration(&self) -> bool {
        self.data.pending_entries().is_some()
    }

    /// Build `index`/`slots` from parked dump entries. Idempotent.
    #[cold]
    pub fn hydrate_pending(&mut self) {
        if let Some(entries) = self.data.take_pending() {
            self.rebuild_from_ordered_entries(entries);
        }
    }

    pub fn rebuild_from_parts(
        &mut self,
        values: FxHashMap<HashKey, Value>,
        key_snapshots: FxHashMap<HashKey, Value>,
        insertion_order: Vec<HashKey>,
    ) {
        self.data.clear();
        self.data.reserve(values.len());
        for hash_key in insertion_order {
            let Some(value) = values.get(&hash_key).copied() else {
                continue;
            };
            let key = key_snapshots.get(&hash_key).copied().unwrap_or(value);
            self.insert(hash_key, key, value);
        }
        for (hash_key, value) in values {
            if self.data.contains_key(&hash_key) {
                continue;
            }
            let key = key_snapshots.get(&hash_key).copied().unwrap_or(value);
            self.insert(hash_key, key, value);
        }
    }

    pub fn live_hash_keys_in_slot_order(&self) -> Vec<&HashKey> {
        self.data.live_hash_keys_in_slot_order()
    }
}

pub(crate) fn build_hash_table_literal_value(
    test: HashTableTest,
    test_name: Option<SymId>,
    size: i64,
    weakness: Option<HashTableWeakness>,
    rehash_size: f64,
    rehash_threshold: f64,
    entries: Vec<(Value, Value)>,
) -> Value {
    let table_value =
        Value::hash_table_with_options(test, size, weakness, rehash_size, rehash_threshold);
    let _ = table_value.with_hash_table_mut(|table| {
        table.test_name = test_name;
        table.user_cmp_function = None;
        table.user_hash_function = None;
        for (key_value, val_value) in entries {
            let key = key_value.to_hash_key(&table.test);
            table.insert(key, key_value, val_value);
        }
    });
    table_value
}

// ---------------------------------------------------------------------------
// Conversion traits for flexible constructors
// ---------------------------------------------------------------------------

/// Trait for types that can be converted to a symbol Value.
/// Implemented by `&str`, `String`, `SymId`.
pub trait IntoSymbol {
    fn into_symbol(self) -> Value;
}

impl IntoSymbol for SymId {
    fn into_symbol(self) -> Value {
        TaggedValue::from_sym_id(self)
    }
}

impl IntoSymbol for &str {
    fn into_symbol(self) -> Value {
        if self == "nil" {
            Value::NIL
        } else if self == "t" {
            Value::T
        } else if self.starts_with(':') {
            add_wrapping(MemoryUseCountSlot::Symbols, 1);
            TaggedValue::from_kw_id(intern(self))
        } else {
            add_wrapping(MemoryUseCountSlot::Symbols, 1);
            TaggedValue::from_sym_id(intern(self))
        }
    }
}

impl IntoSymbol for String {
    fn into_symbol(self) -> Value {
        self.as_str().into_symbol()
    }
}

impl IntoSymbol for &String {
    fn into_symbol(self) -> Value {
        self.as_str().into_symbol()
    }
}

impl IntoSymbol for &&str {
    fn into_symbol(self) -> Value {
        (*self).into_symbol()
    }
}

impl IntoSymbol for &&String {
    fn into_symbol(self) -> Value {
        self.as_str().into_symbol()
    }
}

fn canonical_keyword_name(name: &str) -> String {
    if name.starts_with(':') {
        name.to_owned()
    } else {
        format!(":{name}")
    }
}

// ---------------------------------------------------------------------------
// Convenience constructors on TaggedValue
// ---------------------------------------------------------------------------

impl TaggedValue {
    /// Create a symbol from a string (with nil/t/keyword canonicalization) or SymId.
    pub fn symbol(s: impl IntoSymbol) -> Self {
        s.into_symbol()
    }

    /// Create a symbol by interning a name string, with nil/t/keyword canonicalization.
    pub fn make_symbol(s: impl AsRef<str>) -> Self {
        s.as_ref().into_symbol()
    }

    /// Create a keyword by interning a canonical `:name` symbol.
    pub fn keyword(s: impl AsRef<str>) -> Self {
        add_wrapping(MemoryUseCountSlot::Symbols, 1);
        TaggedValue::from_kw_id(intern(&canonical_keyword_name(s.as_ref())))
    }

    /// Wrap an existing interned keyword symbol id.
    ///
    /// Callers must only pass SymIds whose canonical names already start with `:`.
    pub fn keyword_id(id: SymId) -> Self {
        TaggedValue::from_kw_id(id)
    }

    /// Create a keyword by interning a name string.
    pub fn make_keyword(s: impl AsRef<str>) -> Self {
        Self::keyword(s)
    }

    /// Convert bool to Value (T or NIL).
    #[inline]
    pub fn bool(b: bool) -> Self {
        if b { Value::T } else { Value::NIL }
    }

    // -- Heap-allocating constructors --

    /// Allocate a string on the heap (old API name).
    pub fn string(s: impl Into<String>) -> Self {
        Self::make_string(s)
    }

    /// Allocate a string on the heap.
    /// ASCII-only strings are created as unibyte (matching GNU Emacs
    /// where make_string with pure ASCII is effectively unibyte).
    /// Non-ASCII strings are created as multibyte.
    pub fn make_string(s: impl Into<String>) -> Self {
        let s = s.into();
        let multibyte = !s.is_ascii();
        with_tagged_heap(|h| h.alloc_string(LispString::new(s, multibyte)))
    }

    /// Allocate a string from a pre-built LispString.
    pub fn heap_string(s: LispString) -> Self {
        with_tagged_heap(|h| h.alloc_string(s))
    }

    /// Allocate a multibyte string.
    pub fn multibyte_string(s: impl Into<String>) -> Self {
        let s = s.into();
        with_tagged_heap(|h| h.alloc_string(LispString::new(s, true)))
    }

    /// Allocate a unibyte string.
    pub fn unibyte_string(s: impl Into<String>) -> Self {
        let s = s.into();
        with_tagged_heap(|h| h.alloc_string(LispString::new(s, false)))
    }

    /// Allocate a string with text properties.
    pub fn string_with_text_properties(
        s: impl Into<String>,
        runs: Vec<StringTextPropertyRun>,
    ) -> Self {
        let value = Self::make_string(s);
        set_string_text_properties_for_value(value, runs);
        value
    }

    /// Allocate a multibyte string with text properties.
    pub fn multibyte_string_with_text_properties(
        s: impl Into<String>,
        runs: Vec<StringTextPropertyRun>,
    ) -> Self {
        let value = Self::multibyte_string(s);
        set_string_text_properties_for_value(value, runs);
        value
    }

    /// Allocate a float on the heap.
    pub fn make_float(f: f64) -> Self {
        with_tagged_heap(|h| h.alloc_float(f))
    }

    /// Allocate a bignum on the heap. Caller is responsible for ensuring
    /// the value is outside fixnum range — internal callers should
    /// almost always use [`Value::make_integer`] instead, which mirrors
    /// GNU `make_integer_mpz` (`src/bignum.c:146`) by returning a
    /// fixnum when the value fits and only allocating a bignum on
    /// promotion.
    pub fn bignum(value: Integer) -> Self {
        with_tagged_heap(|h| h.alloc_bignum(value))
    }

    /// Canonical "make a Lisp integer from this machine integer" fast
    /// path. Mirrors GNU `make_int` (`src/lisp.h:3041`): return an
    /// immediate fixnum when possible and allocate a bignum only when
    /// the value is outside the fixnum range.
    #[inline]
    pub fn make_int(value: i64) -> Self {
        if (Self::MOST_NEGATIVE_FIXNUM..=Self::MOST_POSITIVE_FIXNUM).contains(&value) {
            Self::fixnum(value)
        } else {
            Self::bignum(Integer::from(value))
        }
    }

    /// Canonical "make a Lisp integer from this malachite::Integer" entry
    /// point. Mirrors GNU `make_integer_mpz` (`src/bignum.c:146`):
    /// returns a fixnum if the value fits in fixnum range, otherwise
    /// allocates a bignum object.
    pub fn make_integer(value: Integer) -> Self {
        if let Ok(small) = i64::try_from(&value)
            && (TaggedValue::MOST_NEGATIVE_FIXNUM..=TaggedValue::MOST_POSITIVE_FIXNUM)
                .contains(&small)
        {
            return Self::fixnum(small);
        }
        Self::bignum(value)
    }

    /// Convenience used by the dump loader to materialize a bignum from
    /// its decimal representation. If parsing fails (which would
    /// indicate a corrupt dump) it falls back to 0 rather than
    /// panicking — the dump format guarantees a valid base-10 string.
    pub fn make_integer_from_str_or_zero(text: &str) -> Self {
        match Integer::from_str(text) {
            Ok(value) => Self::make_integer(value),
            Err(_) => Self::fixnum(0),
        }
    }

    /// Allocate a cons cell (old API name).
    #[inline]
    pub fn cons(car: Value, cdr: Value) -> Self {
        Self::make_cons(car, cdr)
    }

    /// Allocate a cons cell.
    #[inline]
    pub fn make_cons(car: Value, cdr: Value) -> Self {
        // Keep the expensive corruption diagnostic out of release allocation hot paths.
        #[cfg(debug_assertions)]
        if car.is_string() {
            let ptr = car.as_string_ptr().unwrap();
            let hdr = unsafe { &(*ptr).header };
            if !matches!(hdr.kind, crate::tagged::header::HeapObjectKind::String) {
                // Check if the address is actually a VecLike — dump its type_tag
                let vlh = unsafe { &*(ptr as *const crate::tagged::header::VecLikeHeader) };
                let expected_tagged = ptr as usize | 0b101; // what the VecLike tag would be
                panic!(
                    "CONS CAR BUG: car={:#x} (ptr {:?}, kind={:?}) is corrupt string.\n\
                     VecLikeHeader.type_tag={:?}\n\
                     If this were tagged as VecLike it would be {:#x}\n\
                     car XOR veclike_tagged = {:#x}",
                    car.0,
                    ptr,
                    hdr.kind,
                    vlh.type_tag,
                    expected_tagged,
                    car.0 ^ expected_tagged,
                );
            }
        }
        with_tagged_heap(|h| h.alloc_cons(car, cdr))
    }

    /// Build a proper list from a Vec.
    pub fn list(mut values: Vec<Value>) -> Self {
        // Root the elements in one go and keep the growing list rooted through
        // a single slot re-pointed per cons: two thread-local pushes per
        // element made an 11-element `parse-partial-sexp' state cost ~1.1K Ir
        // (GNU conses it for ~200).
        let saved_roots = super::eval::save_scratch_gc_roots();
        super::eval::push_scratch_gc_roots(&values);
        let acc_slot = super::eval::push_scratch_gc_root_slot(Value::NIL);
        let mut acc = Value::NIL;
        while let Some(item) = values.pop() {
            acc = Value::cons(item, acc);
            super::eval::set_scratch_gc_root(acc_slot, acc);
        }
        super::eval::restore_scratch_gc_roots(saved_roots);
        acc
    }

    /// Build a proper list from a slice without first cloning into a `Vec`.
    pub fn list_from_slice(values: &[Value]) -> Self {
        let saved_roots = super::eval::save_scratch_gc_roots();
        for value in values.iter().copied() {
            super::eval::push_scratch_gc_root(value);
        }
        let mut acc = Value::NIL;
        let mut idx = values.len();
        while idx > 0 {
            idx -= 1;
            acc = Value::cons(values[idx], acc);
            super::eval::push_scratch_gc_root(acc);
        }
        super::eval::restore_scratch_gc_roots(saved_roots);
        acc
    }

    /// Allocate a vector (old API name).
    pub fn vector(values: Vec<Value>) -> Self {
        Self::make_vector(values)
    }

    /// Allocate a vector.
    pub fn make_vector(values: Vec<Value>) -> Self {
        with_tagged_heap(|h| h.alloc_vector(values))
    }

    /// Allocate a GNU-shaped char-table.
    pub fn make_char_table(purpose: Value, init: Value, n_extras: usize) -> Self {
        with_tagged_heap(|h| h.alloc_char_table(purpose, init, n_extras))
    }

    /// Allocate a GNU-shaped sub-char-table.
    pub fn make_sub_char_table(depth: i32, min_char: i32, contents: Vec<Value>) -> Self {
        with_tagged_heap(|h| h.alloc_sub_char_table(depth, min_char, contents))
    }

    /// Allocate a record.
    pub fn make_record(values: Vec<Value>) -> Self {
        with_tagged_heap(|h| h.alloc_record(values))
    }

    /// Allocate an opaque opened-font pseudovector (`PVEC_FONT`).
    pub(crate) fn make_font(data: FontObjectData) -> Self {
        with_tagged_heap(|h| h.alloc_font(data))
    }

    /// Allocate a window-configuration pseudovector (distinct type tag, same
    /// `{header, data}` storage as a record).
    pub fn make_window_configuration(values: Vec<Value>) -> Self {
        with_tagged_heap(|h| h.alloc_window_configuration(values))
    }

    /// Allocate a lambda. Converts LambdaData to a Value vector for GC safety.
    pub fn make_lambda(data: LambdaData) -> Self {
        with_tagged_heap(|h| h.alloc_lambda_from_data(data))
    }

    /// Allocate a lambda from already-validated GNU closure slots.
    pub fn make_lambda_with_slots(slots: Vec<Value>) -> Self {
        with_tagged_heap(|h| h.alloc_lambda(slots))
    }

    /// Allocate a macro. Converts LambdaData to a Value vector for GC safety.
    pub fn make_macro(data: LambdaData) -> Self {
        with_tagged_heap(|h| h.alloc_macro_from_data(data))
    }

    /// Allocate a macro from already-validated GNU closure slots.
    pub fn make_macro_with_slots(slots: Vec<Value>) -> Self {
        with_tagged_heap(|h| h.alloc_macro(slots))
    }

    /// Allocate a bytecode function.
    pub fn make_bytecode(bc: super::bytecode::ByteCodeFunction) -> Self {
        // Test builds hand-assemble instruction vectors all over the suite;
        // route them through the real sealing normalizer so they may enter
        // the unchecked-fetch driver. Production sealing happens exclusively
        // in the decode installers — this branch must never widen to release
        // builds, or the `ops_sealed` marker would stop proving anything.
        #[cfg(test)]
        let bc = {
            let mut bc = bc;
            bc.seal_hand_assembled_ops_for_test();
            bc
        };
        with_tagged_heap(|h| h.alloc_bytecode(bc))
    }

    /// Allocate a hash table.
    pub fn hash_table(test: HashTableTest) -> Self {
        with_tagged_heap(|h| h.alloc_hash_table(LispHashTable::new(test)))
    }

    /// Allocate a hash table with options.
    pub fn hash_table_with_options(
        test: HashTableTest,
        size: i64,
        weakness: Option<HashTableWeakness>,
        rehash_size: f64,
        rehash_threshold: f64,
    ) -> Self {
        with_tagged_heap(|h| {
            h.alloc_hash_table(LispHashTable::new_with_options(
                test,
                size,
                weakness,
                rehash_size,
                rehash_threshold,
            ))
        })
    }

    /// Allocate a GNU-shaped obarray object.
    pub fn obarray(size: usize) -> Self {
        with_tagged_heap(|h| h.alloc_obarray(vec![Value::NIL; size]))
    }

    /// Allocate a marker.
    pub fn make_marker(data: crate::heap_types::LispMarker) -> Self {
        with_tagged_heap(|h| h.alloc_marker(data))
    }

    /// Allocate an overlay.
    pub fn make_overlay(data: impl Into<crate::heap_types::OverlayData>) -> Self {
        let mut data = data.into();
        if data.serial == 0 {
            data.serial = crate::heap_types::next_overlay_serial();
        } else {
            crate::heap_types::observe_overlay_serial(data.serial);
        }
        with_tagged_heap(|h| h.alloc_overlay(data))
    }

    /// Allocate a buffer reference.
    pub fn make_buffer(id: crate::buffer::BufferId) -> Self {
        with_tagged_heap(|h| {
            if let Some(value) = h.buffer_value(id) {
                value
            } else {
                let value = h.alloc_buffer(id);
                h.register_buffer_value(id, value);
                value
            }
        })
    }

    /// Allocate a window reference.
    pub fn make_window(id: u64) -> Self {
        with_tagged_heap(|h| {
            if let Some(value) = h.window_value(id) {
                value
            } else {
                let value = h.alloc_window(id);
                h.register_window_value(id, value);
                value
            }
        })
    }

    /// Allocate a frame reference.
    pub fn make_frame(id: u64) -> Self {
        with_tagged_heap(|h| {
            if let Some(value) = h.frame_value(id) {
                value
            } else {
                let value = h.alloc_frame(id);
                h.register_frame_value(id, value);
                value
            }
        })
    }

    /// Allocate a timer reference.
    pub fn make_timer(id: u64) -> Self {
        with_tagged_heap(|h| {
            if let Some(value) = h.timer_value(id) {
                value
            } else {
                let value = h.alloc_timer(id);
                h.register_timer_value(id, value);
                value
            }
        })
    }

    /// Allocate a process reference.
    ///
    /// Returns the same value for the same id (eq-ness) via the process value
    /// cache, exactly like `make_buffer`/`make_timer`.  A process that has
    /// exited is still a process object in GNU (status `exit`/`signal`), so the
    /// cached value is never evicted on `delete-process`.
    pub fn make_process(id: crate::emacs_core::process::ProcessId) -> Self {
        with_tagged_heap(|h| {
            if let Some(value) = h.process_value(id) {
                value
            } else {
                let value = h.alloc_process(id);
                h.register_process_value(id, value);
                value
            }
        })
    }

    /// Allocate a GNU-shaped terminal object.
    pub fn make_terminal(id: u64) -> Self {
        with_tagged_heap(|h| h.alloc_terminal(id))
    }

    /// Allocate a GNU-shaped xwidget model object.
    pub fn make_xwidget(
        type_: Value,
        title: Value,
        buffer: Value,
        width: i32,
        height: i32,
        xwidget_id: u32,
        webview_id: neomacs_display_protocol::WebViewId,
    ) -> Self {
        with_tagged_heap(|h| {
            h.alloc_xwidget(type_, title, buffer, width, height, xwidget_id, webview_id)
        })
    }

    /// Allocate a GNU-shaped xwidget view object.
    pub fn make_xwidget_view(model: Value, window: Value) -> Self {
        with_tagged_heap(|h| h.alloc_xwidget_view(model, window))
    }

    /// Allocate a GC-managed shader-surface handle wrapping a host surface
    /// id (`neomacs-surface-create`). When the handle becomes unreachable,
    /// the GC sweep queues the id for a best-effort
    /// `DisplayHost::destroy_shader_surface`, so a handle Lisp drops without
    /// an explicit `neomacs-surface-destroy` still frees its GPU objects.
    pub fn make_surface_handle(surface_id: u32) -> Self {
        with_tagged_heap(|h| h.alloc_surface_handle(surface_id))
    }

    /// Allocate an opaque SQLite database or statement object.
    pub(crate) fn make_sqlite(is_statement: bool, id: i64) -> Self {
        with_tagged_heap(|h| h.alloc_sqlite(is_statement, id))
    }

    // -- Veclike accessor helpers --

    /// Check if this is a lambda.
    #[inline]
    pub fn is_lambda(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Lambda)
    }

    /// Check if this is a macro.
    #[inline]
    pub fn is_macro(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Macro)
    }

    /// Check if this is a bytecode function.
    #[inline]
    pub fn is_bytecode(self) -> bool {
        self.veclike_type() == Some(VecLikeType::ByteCode)
    }

    /// Check if this is a buffer.
    #[inline]
    pub fn is_buffer(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Buffer)
    }

    /// Check if this is a window.
    #[inline]
    pub fn is_window(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Window)
    }

    /// Check if this is a frame.
    #[inline]
    pub fn is_frame(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Frame)
    }

    /// Check if this is a timer.
    #[inline]
    pub fn is_timer(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Timer)
    }

    /// Check if this is a process.
    #[inline]
    pub fn is_process(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Process)
    }

    /// Check if this is a marker.
    #[inline]
    pub fn is_marker(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Marker)
    }

    /// Check if this is an overlay.
    #[inline]
    pub fn is_overlay(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Overlay)
    }

    /// Check if this is an xwidget.
    #[inline]
    pub fn is_xwidget(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Xwidget)
    }

    /// Check if this is an xwidget view.
    #[inline]
    pub fn is_xwidget_view(self) -> bool {
        self.veclike_type() == Some(VecLikeType::XwidgetView)
    }

    /// Check if this is a GC-managed shader-surface handle.
    #[inline]
    pub fn is_surface_handle(self) -> bool {
        self.veclike_type() == Some(VecLikeType::SurfaceHandle)
    }

    // -- Data accessors for heap types --

    /// Get an owned copy of the string contents.
    pub fn as_str_owned(self) -> Option<String> {
        self.as_utf8_str().map(|s| s.to_owned())
    }

    /// Get an owned, lossily-decoded `String` view of a Lisp string: valid
    /// Unicode (including real Private-Use glyphs) is preserved exactly, while
    /// raw eight-bit bytes become U+FFFD. Test-only convenience; production code
    /// uses `as_lisp_string` for byte-faithful access.
    pub fn as_runtime_string_owned(self) -> Option<String> {
        self.as_lisp_string()
            .map(|string| crate::emacs_core::emacs_char::to_utf8_lossy(string.as_bytes()))
    }

    /// Access the heap string via a closure.
    pub fn with_str<R>(self, f: impl FnOnce(&str) -> R) -> Option<R> {
        self.as_utf8_str().map(f)
    }

    /// Borrow the `LispString` for a string value, for no longer than the
    /// `Value` place it was read from.
    ///
    /// # What the lifetime does and does not say (DIVERGENCES.md 163, 167)
    ///
    /// The referent lives in a mark-sweep, partly-concurrent heap. When the
    /// collector reclaims the object, `sweep_range` runs `drop_in_place`,
    /// which frees the byte buffer. Until 167 this function returned
    /// `&'static LispString` at ~500 production sites, which told the borrow
    /// checker the referent outlives the process and so opted every one of
    /// them out of the check it would otherwise run. It is `&self` now, so a
    /// borrow that ESCAPES its `Value` -- returned from a closure that owns
    /// the value, parked in a `'static` field, handed out of the function that
    /// read it -- is a compile error. That cost 20 of them, and every one was
    /// either a temporary or a genuine escape.
    ///
    /// It is NOT the safepoint property: a `Value` local can outlive a
    /// safepoint and still be unrooted, so keeping the place alive is not
    /// keeping the object alive. Two rules are what make the surviving borrows
    /// sound, and they are the ones to check before adding another:
    ///
    /// 1. **Rooting.** A value need only be rooted before the next SAFEPOINT,
    ///    not the next allocation (`tagged/CONCURRENT_GC.md`, "precise-rooting
    ///    precondition"). A subr's own arguments are rooted for the whole
    ///    call by the backtrace frame `apply_internal` pushes — GNU roots the
    ///    same thing by name in `mark_specpdl`'s `SPECPDL_BACKTRACE` arm
    ///    (`src/eval.c`) — so `args[i].as_lisp_string()` cannot be collected
    ///    out from under the borrow.
    /// 2. **Relocation.** `LispString::mutate_bytes` can move the payload, so
    ///    a live borrow is also invalidated by `aset` on the same string with
    ///    no collection involved. GNU has the identical hazard and is
    ///    explicit about it: `compact_small_strings` relocates small string
    ///    data on every GC, which is what `pin_string` exists for.
    ///
    /// Prefer [`Value::lisp_string_in`] / `Context::lisp_string` when either
    /// rule is in doubt: the borrow they return is tied to a shared borrow of
    /// the heap, and every safepoint in this engine needs `&mut Context`, so
    /// holding one across a safepoint is a BORROW ERROR instead of a review
    /// question.
    ///
    /// The escape property is a compile-time one, so this is the only place it
    /// can be pinned. Restoring the `&'static` return type makes this doctest
    /// stop failing, which is the red measurement for 167:
    ///
    /// ```compile_fail,E0515
    /// use neovm_core::Value;
    /// use neovm_core::heap_types::LispString;
    ///
    /// // A borrow of a heap string cannot outlive the `Value` it was read
    /// // from: the collector is free to take the object the moment nothing
    /// // roots it, and a `Value` on the Rust stack roots nothing.
    /// fn escapes(value: Value) -> Option<&'static LispString> {
    ///     value.as_lisp_string()
    /// }
    /// ```
    pub fn as_lisp_string(&self) -> Option<&LispString> {
        // SAFETY: the anchor is `self` -- the returned borrow is tied to the
        // `Value` place it was read from, which is the strongest claim this
        // function is entitled to make.
        unsafe { self.as_lisp_string_reanchored() }
    }

    /// Borrow the `LispString` with an anchor this function cannot check.
    ///
    /// [`Value::as_lisp_string`] ties its result to the `Value` PLACE it was
    /// read from. That is the honest thing to say about a `Copy` word pointing
    /// into a mark-sweep heap, and it is what turns "is this borrow held
    /// across a safepoint" from a review question into a compile question,
    /// because every safepoint in this engine needs `&mut Context`.
    ///
    /// Three callers need to anchor the borrow somewhere ELSE. Each is listed
    /// here, and the list is the point of the function:
    ///
    /// * [`Value::as_lisp_string`] itself, anchored to `&self`.
    /// * [`Value::lisp_string_in`], anchored to a shared borrow of the heap --
    ///   a STRONGER anchor than the place, and the one that makes holding the
    ///   borrow across a safepoint a borrow error.
    /// * [`Value::closure_docstring`], anchored to the closure the docstring
    ///   slot belongs to.
    ///
    /// # Safety
    ///
    /// The caller must name the anchor the returned borrow is tied to, and
    /// that anchor must keep the string object alive for all of it. This is
    /// the only remaining launderer of a heap borrow's lifetime in `Value`
    /// (DIVERGENCES.md 167); a fourth call site is an argument to be had, not
    /// an edit to be made.
    #[inline]
    unsafe fn as_lisp_string_reanchored<'a>(self) -> Option<&'a LispString> {
        self.as_string_ptr().map(|p| {
            let string = unsafe { &(*p).data };
            if string.is_reclaimed() {
                reclaimed_string_borrowed(p);
            }
            string
        })
    }

    /// Borrow the `LispString` for as long as the collector provably cannot
    /// run — the honest sibling of [`Value::as_lisp_string`].
    ///
    /// The returned borrow is tied to `heap`. Every GC safepoint in this
    /// engine (`Context::gc_safe_point`, `eval_sub`, `apply_internal`, the
    /// bytecode branch poll, and the `garbage-collect` subr) reaches
    /// collection through a `&mut Context` that owns the heap, so the borrow
    /// checker rejects any attempt to hold this reference across one. That is
    /// the whole point: the type system already models "the collector may run
    /// here", and it spells it `&mut`.
    ///
    /// This does NOT cover a `Value` parked in a `Vec`/`HashMap`/struct field
    /// — a `Copy` word with no borrow to track. Those need an explicit root
    /// (DIVERGENCES.md 161/162's `InFlightRoots`), not a lifetime.
    #[inline]
    pub fn lisp_string_in<'a>(
        self,
        heap: &'a crate::tagged::gc::TaggedHeap,
    ) -> Option<&'a LispString> {
        let _ = heap;
        // SAFETY: the anchor is `heap`. Reaching a collection needs `&mut
        // Context`, which owns the heap, so this borrow cannot coexist with
        // one -- a stronger anchor than `as_lisp_string`'s place, which is the
        // whole reason this function exists.
        unsafe { self.as_lisp_string_reanchored() }
    }

    /// [`Value::lisp_string_in`] with GNU's `CHECK_STRING` signal — the
    /// field-precise sibling of `Context::expect_lisp_string`, and the answer
    /// to the one false positive that accessor is measured to have.
    ///
    /// `Context::expect_lisp_string` takes `&self` on the WHOLE `Context`, so
    /// its borrow blocks a safepoint (which is the entire point — every
    /// safepoint in this engine is a `&mut Context` method) but it also blocks
    /// `&mut ctx.buffers`, `&mut ctx.treesit` and every other disjoint field,
    /// none of which can reach a safepoint. Naming the heap directly is
    /// strictly more precise and gives up nothing:
    ///
    /// * `&mut *ctx` — still `E0502`, because a borrow of `ctx.tagged_heap`
    ///   conflicts with a mutable borrow of the whole struct. The collector
    ///   still cannot run while this borrow lives.
    /// * `&mut ctx.buffers` — now allowed, because the borrow checker knows
    ///   the two fields are disjoint.
    ///
    /// Use this where a site must mutate one field while reading a string;
    /// use `Context::expect_lisp_string` everywhere else, since it reads
    /// better and the extra strictness costs nothing at a site that does not
    /// need it. DIVERGENCES.md 175 §5 measured the false positive
    /// (`builtins/treesit.rs`, `E0502` ×3 on `eval.buffers`), and 185 §2 is
    /// this.
    #[inline]
    pub fn expect_lisp_string_in<'a>(
        self,
        heap: &'a crate::tagged::gc::TaggedHeap,
    ) -> Result<&'a LispString, Flow> {
        self.lisp_string_in(heap).ok_or_else(|| {
            signal(
                crate::emacs_core::error::LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), self],
            )
        })
    }

    /// Check if a string is multibyte.
    pub fn string_is_multibyte(self) -> bool {
        self.as_lisp_string().is_some_and(|s| s.is_multibyte())
    }

    /// Get the closure slot vector for a Lambda or Macro.
    pub fn closure_slots(self) -> Option<&'static LispValueSlice> {
        match self.veclike_type()? {
            VecLikeType::Lambda => {
                let ptr = self.as_veclike_ptr().unwrap() as *const LambdaObj;
                Some(unsafe { LispValueSlice::from_slice((*ptr).data.as_slice()) })
            }
            VecLikeType::Macro => {
                let ptr = self.as_veclike_ptr().unwrap() as *const MacroObj;
                Some(unsafe { LispValueSlice::from_slice((*ptr).data.as_slice()) })
            }
            _ => None,
        }
    }

    /// Mutate closure slots through the centralized tagged-runtime write path.
    pub fn with_closure_slots_mut<R>(self, f: impl FnOnce(&mut Vec<Value>) -> R) -> Option<R> {
        mutate::with_closure_slots_mut(self, f)
    }

    /// Replace the entire closure slot vector through the centralized write path.
    pub fn replace_closure_slots(self, slots: Vec<Value>) -> bool {
        mutate::replace_closure_slots(self, slots)
    }

    /// Update a single closure slot through the centralized write path.
    pub fn set_closure_slot(self, index: usize, value: Value) -> bool {
        mutate::set_closure_slot(self, index, value)
    }

    fn closure_parsed_params_cell(self) -> Option<&'static OnceLock<LambdaParams>> {
        match self.veclike_type()? {
            VecLikeType::Lambda => {
                let ptr = self.as_veclike_ptr().unwrap() as *const LambdaObj;
                Some(unsafe { &(*ptr).parsed_params })
            }
            VecLikeType::Macro => {
                let ptr = self.as_veclike_ptr().unwrap() as *const MacroObj;
                Some(unsafe { &(*ptr).parsed_params })
            }
            _ => None,
        }
    }

    pub fn closure_slot(self, index: usize) -> Option<Value> {
        self.closure_slots()
            .and_then(|slots| slots.get(index).copied())
    }

    pub fn closure_params(self) -> Option<&'static LambdaParams> {
        let cell = self.closure_parsed_params_cell()?;
        Some(cell.get_or_init(|| {
            let arglist = self.closure_slot(CLOSURE_ARGLIST).unwrap_or(Value::NIL);
            crate::emacs_core::builtins::parse_lambda_params_from_value(&arglist)
                .unwrap_or_else(|_| LambdaParams::simple(vec![]))
        }))
    }

    pub fn closure_body_value(self) -> Option<Value> {
        self.closure_slot(CLOSURE_CODE)
    }

    pub(crate) fn function_source_identity(self) -> Option<FunctionSourceIdentity> {
        // GNU `function-equal` (Ffunction_equal, profiler.c) only treats two
        // distinct objects as equal when both are COMPILEDP and share their
        // bytecode object. Interpreted-function closures (the Emacs 30+ `#[args
        // body env]` records) fall through to the EQ-only case there, so they
        // must NOT advertise a shared source identity here -- two instances of
        // the same lambda expression with different captured environments are
        // not `function-equal`.
        self.get_bytecode_data()
            .map(|function| FunctionSourceIdentity::ByteCode(function.source_id))
    }

    /// GNU `function-equal`: two compiled closures are equal when they share
    /// their bytecode object; every other function compares by identity (EQ).
    pub(crate) fn function_equal(self, other: Value) -> bool {
        if self.bits() == other.bits() {
            return true;
        }
        matches!(
            (self.function_source_identity(), other.function_source_identity()),
            (Some(left), Some(right)) if left == right
        )
    }

    pub fn closure_env(self) -> Option<Option<Value>> {
        self.closure_slot(CLOSURE_CONSTANTS)
            .map(|env| (!env.is_nil()).then_some(env))
    }

    pub fn closure_doc_value(self) -> Option<Value> {
        self.closure_slot(CLOSURE_DOC_STRING)
    }

    pub fn closure_doc_form(self) -> Option<Option<Value>> {
        self.closure_doc_value().map(|doc| {
            if doc.is_nil() || doc.is_string() {
                None
            } else {
                Some(doc)
            }
        })
    }

    pub fn closure_docstring(&self) -> Option<Option<&LispString>> {
        self.closure_doc_value().map(|doc| {
            if doc.is_string() {
                // SAFETY: the anchor is `self`. `doc` is this closure's own
                // CLOSURE_DOC_STRING slot, so the string is reachable from the
                // closure and outlives it only if the closure does; tying the
                // borrow to `&self` says exactly that. The `doc` local is a
                // copy of the slot and anchors nothing, which is why the safe
                // accessor cannot express it.
                unsafe { doc.as_lisp_string_reanchored() }
            } else {
                None
            }
        })
    }

    /// Return GNU's `CLOSURE_INTERACTIVE` slot when it is present.
    ///
    /// Presence, rather than the Lisp truth of the stored value, determines
    /// whether a closure is a command: `(interactive)` stores `nil` in this
    /// slot and is still interactive.  `Option<Value>` represents those two
    /// states without permitting callers to accidentally flatten away a
    /// present `nil` slot.
    pub fn closure_interactive(self) -> Option<Value> {
        self.closure_slot(CLOSURE_INTERACTIVE)
    }

    /// Borrow the ByteCodeFunction from a ByteCode value.
    ///
    /// THE materialization seam: a lazy pdump stub is built from its mapped
    /// extras here, once, on the mutator thread, before any caller sees the
    /// data. Everything that reads bytecode data routes through this (the
    /// architecture test pins it), so a stub can never leak empty vectors.
    pub fn get_bytecode_data(self) -> Option<&'static super::bytecode::ByteCodeFunction> {
        #[cfg(test)]
        BYTECODE_DATA_ACCESS_COUNT.with(|count| count.set(count.get() + 1));
        if self.veclike_type()? == VecLikeType::ByteCode {
            let ptr = self.as_veclike_ptr().unwrap() as *const ByteCodeObj;
            let data = unsafe { &(*ptr).data };
            if data.is_pdump_stub() {
                crate::emacs_core::pdump::materialize_and_publish_stub(self);
            }
            Some(unsafe { &(*ptr).data })
        } else {
            None
        }
    }

    /// [`Self::get_bytecode_data`] for callers that ALREADY proved the
    /// veclike type — the VM's resolved-callee token proves it at mint, and
    /// re-checking on every `code()` projection measured +7.3 Ir/call on the
    /// tier-0 differential. Lives here, at the chokepoint file, because this
    /// is exactly where the lazy pdump stub check (two loads + a predictable
    /// branch to a cold materializer) lands: the caller's type proof does not
    /// exempt it from materialization, only from re-classification.
    ///
    /// SAFETY contract (checked in debug): `self` must be a ByteCode value.
    #[inline(always)]
    pub(crate) fn bytecode_data_typechecked_by_caller(
        self,
    ) -> &'static super::bytecode::ByteCodeFunction {
        debug_assert_eq!(self.veclike_type(), Some(VecLikeType::ByteCode));
        #[cfg(test)]
        BYTECODE_DATA_ACCESS_COUNT.with(|count| count.set(count.get() + 1));
        let ptr = (self.bits() & !TAG_MASK) as *const ByteCodeObj;
        // SAFETY: the caller's type proof (debug-asserted above) establishes
        // a live ByteCodeObj; bytecode arena/mapped objects are immovable.
        let data = unsafe { &(*ptr).data };
        if data.is_pdump_stub() {
            crate::emacs_core::pdump::materialize_and_publish_stub(self);
            return unsafe { &(*ptr).data };
        }
        data
    }

    /// [`Self::get_bytecode_data`] that promises NOT to materialize a lazy
    /// pdump stub (once stubs exist): the peek for scanners that only care
    /// about already-live functions — AOT post-insert marking, PGO drains.
    /// Today (pre-stub) it is the same borrow.
    pub(crate) fn bytecode_data_if_materialized(
        self,
    ) -> Option<&'static super::bytecode::ByteCodeFunction> {
        if self.veclike_type()? != VecLikeType::ByteCode {
            return None;
        }
        let ptr = self.as_veclike_ptr().unwrap() as *const ByteCodeObj;
        let data = unsafe { &(*ptr).data };
        (!data.is_pdump_stub()).then_some(data)
    }

    /// The command-classification facts of a bytecode value, WITHOUT forcing
    /// full materialization (once stubs exist, this reads the raw mapped
    /// extras header): closure slot count plus the interactive/doc-form
    /// slots. Serving `commandp`/`interactive-form`/`command-modes` through
    /// this probe keeps the first obarray-wide M-x sweep from materializing
    /// every dumped function in one burst.
    /// Raw-header required-only check for a still-stub function; `None` when
    /// the value is not bytecode, `Some(materialized-or-raw answer)` else —
    /// without materializing a stub.
    pub(crate) fn bytecode_params_required_only_probe(self) -> Option<bool> {
        if self.veclike_type()? != VecLikeType::ByteCode {
            return None;
        }
        let ptr = self.as_veclike_ptr().unwrap() as *const ByteCodeObj;
        let data = unsafe { &(*ptr).data };
        if data.is_pdump_stub() {
            return Some(unsafe {
                crate::emacs_core::pdump::stub_params_required_only(ptr, data.closure_slot_count)
            });
        }
        Some(data.params.optional.is_empty() && data.params.rest.is_none())
    }

    pub(crate) fn bytecode_interactive_probe(self) -> Option<BytecodeInteractiveProbe> {
        if self.veclike_type()? != VecLikeType::ByteCode {
            return None;
        }
        let ptr = self.as_veclike_ptr().unwrap() as *const ByteCodeObj;
        let data = unsafe { &(*ptr).data };
        if data.is_pdump_stub() {
            // Read the raw mapped extras header — the whole point of this
            // probe is that an obarray-wide commandp/interactive sweep must
            // not materialize 6,779 functions in one burst.
            return Some(unsafe {
                crate::emacs_core::pdump::stub_interactive_probe(ptr, data.closure_slot_count)
            });
        }
        Some(BytecodeInteractiveProbe {
            slot_count: data.observable_closure_slot_count(),
            interactive: data.interactive,
            doc_form: data.doc_form,
        })
    }

    /// Get the pointer address as a unique identity for a string value.
    /// Used for text property operations.
    pub fn str_ptr_key(self) -> Option<usize> {
        self.as_string_ptr().map(|p| p as usize)
    }

    /// Get the buffer ID from a buffer value.
    pub fn as_buffer_id(self) -> Option<crate::buffer::BufferId> {
        if self.is_buffer() {
            let ptr = self.as_veclike_ptr().unwrap() as *const BufferObj;
            Some(unsafe { (*ptr).id })
        } else {
            None
        }
    }

    /// Get the window ID from a window value.
    pub fn as_window_id(self) -> Option<u64> {
        if self.is_window() {
            let ptr = self.as_veclike_ptr().unwrap() as *const WindowObj;
            Some(unsafe { (*ptr).id })
        } else {
            None
        }
    }

    /// Get the frame ID from a frame value.
    pub fn as_frame_id(self) -> Option<u64> {
        if self.is_frame() {
            let ptr = self.as_veclike_ptr().unwrap() as *const FrameObj;
            Some(unsafe { (*ptr).id })
        } else {
            None
        }
    }

    /// Get the timer ID from a timer value.
    pub fn as_timer_id(self) -> Option<u64> {
        if self.is_timer() {
            let ptr = self.as_veclike_ptr().unwrap() as *const TimerObj;
            Some(unsafe { (*ptr).id })
        } else {
            None
        }
    }

    /// Get the process ID from a process value.
    pub fn as_process_id(self) -> Option<crate::emacs_core::process::ProcessId> {
        if self.is_process() {
            let ptr = self.as_veclike_ptr().unwrap() as *const ProcessObj;
            Some(unsafe { (*ptr).id })
        } else {
            None
        }
    }

    /// Get the host surface id from a shader-surface handle value.
    pub fn as_surface_handle(self) -> Option<u32> {
        if self.is_surface_handle() {
            let ptr = self.as_veclike_ptr().unwrap() as *const SurfaceObj;
            Some(unsafe { (*ptr).surface_id })
        } else {
            None
        }
    }

    /// Get the xwidget payload from an xwidget value.
    pub fn as_xwidget(self) -> Option<&'static XwidgetObj> {
        if self.is_xwidget() {
            let ptr = self.as_veclike_ptr().unwrap() as *const XwidgetObj;
            Some(unsafe { &*ptr })
        } else {
            None
        }
    }

    /// Mutate xwidget data through the centralized tagged-runtime write path.
    pub fn with_xwidget_mut<R>(self, f: impl FnOnce(&mut XwidgetObj) -> R) -> Option<R> {
        mutate::with_xwidget_mut(self, f)
    }

    /// Get the xwidget-view payload from an xwidget-view value.
    pub fn as_xwidget_view(self) -> Option<&'static XwidgetViewObj> {
        if self.is_xwidget_view() {
            let ptr = self.as_veclike_ptr().unwrap() as *const XwidgetViewObj;
            Some(unsafe { &*ptr })
        } else {
            None
        }
    }

    /// Mutate xwidget-view data through the centralized tagged-runtime write path.
    pub fn with_xwidget_view_mut<R>(self, f: impl FnOnce(&mut XwidgetViewObj) -> R) -> Option<R> {
        mutate::with_xwidget_view_mut(self, f)
    }

    /// Get the marker data from a marker value.
    pub fn as_marker_data(self) -> Option<&'static crate::heap_types::LispMarker> {
        if self.is_marker() {
            let ptr = self.as_veclike_ptr().unwrap() as *const MarkerObj;
            Some(unsafe { &(*ptr).data })
        } else {
            None
        }
    }

    /// Mutate marker data through the centralized tagged-runtime write path.
    pub fn with_marker_data_mut<R>(
        self,
        f: impl FnOnce(&mut crate::heap_types::LispMarker) -> R,
    ) -> Option<R> {
        mutate::with_marker_data_mut(self, f)
    }

    /// Get the overlay data from an overlay value.
    pub fn as_overlay_data(self) -> Option<&'static crate::heap_types::OverlayData> {
        if self.is_overlay() {
            let ptr = self.as_veclike_ptr().unwrap() as *const OverlayObj;
            Some(unsafe { &(*ptr).data })
        } else {
            None
        }
    }

    /// Mutate overlay data through the centralized tagged-runtime write path.
    pub fn with_overlay_data_mut<R>(
        self,
        f: impl FnOnce(&mut crate::heap_types::OverlayData) -> R,
    ) -> Option<R> {
        mutate::with_overlay_data_mut(self, f)
    }

    /// Get vector elements.
    pub fn as_vector_data(self) -> Option<&'static LispValueSlice> {
        if self.is_vector() {
            let ptr = self.as_veclike_ptr().unwrap() as *const VectorObj;
            Some(unsafe { LispValueSlice::from_slice((*ptr).data.as_slice()) })
        } else {
            None
        }
    }

    /// Mutate vector elements through the centralized tagged-runtime write path.
    pub fn with_vector_data_mut<R>(self, f: impl FnOnce(&mut Vec<Value>) -> R) -> Option<R> {
        mutate::with_vector_data_mut(self, f)
    }

    /// Replace the entire contents of a vector value.
    pub fn replace_vector_data(self, values: Vec<Value>) -> bool {
        mutate::replace_vector_data(self, values)
    }

    /// Update a single vector slot through the centralized write path.
    pub fn set_vector_slot(self, index: usize, value: Value) -> bool {
        mutate::set_vector_slot(self, index, value)
    }

    /// Borrow a GNU-shaped char-table object.
    pub fn as_char_table_obj(self) -> Option<&'static CharTableObj> {
        if self.is_char_table() {
            let ptr = self.as_veclike_ptr().unwrap() as *const CharTableObj;
            Some(unsafe { &*ptr })
        } else {
            None
        }
    }

    /// Mutate a GNU-shaped char-table object.
    pub fn with_char_table_mut<R>(self, f: impl FnOnce(&mut CharTableObj) -> R) -> Option<R> {
        if !self.is_char_table() {
            return None;
        }
        // Write barrier: char-tables are dumped (syntax/category/case tables)
        // and mutated in place, so the GC remembered set must learn about
        // dumped char-table → heap edges through this single mutation
        // chokepoint. Fired before `f` (conservative: any `_mut` borrow may
        // store a heap pointer). No-op unless write tracking is enabled.
        note_heap_write(self, HeapWriteKind::CharTableData);
        let ptr = self.as_veclike_ptr().unwrap() as *mut CharTableObj;
        Some(f(unsafe { &mut *ptr }))
    }

    /// Borrow a GNU-shaped sub-char-table object.
    pub fn as_sub_char_table_obj(self) -> Option<&'static SubCharTableObj> {
        if self.is_sub_char_table() {
            let ptr = self.as_veclike_ptr().unwrap() as *const SubCharTableObj;
            Some(unsafe { &*ptr })
        } else {
            None
        }
    }

    /// Mutate a GNU-shaped sub-char-table object.
    pub fn with_sub_char_table_mut<R>(
        self,
        f: impl FnOnce(&mut SubCharTableObj) -> R,
    ) -> Option<R> {
        if !self.is_sub_char_table() {
            return None;
        }
        // Write barrier — see `with_char_table_mut`. Sub-char-tables are the
        // dumped char-table interior nodes and are mutated the same way.
        note_heap_write(self, HeapWriteKind::SubCharTableData);
        let ptr = self.as_veclike_ptr().unwrap() as *mut SubCharTableObj;
        Some(f(unsafe { &mut *ptr }))
    }

    /// Expose GNU's readable char-table slots:
    /// DEFAULT PARENT PURPOSE ASCII CONTENTS[64] EXTRAS...
    pub fn char_table_external_slots(self) -> Option<Vec<Value>> {
        let table = self.as_char_table_obj()?;
        let mut slots = Vec::with_capacity(4 + CHAR_TABLE_TOP_SLOTS + table.extras.len());
        slots.push(table.defalt);
        slots.push(table.parent);
        slots.push(table.purpose);
        slots.push(table.ascii);
        slots.extend_from_slice(&table.contents);
        slots.extend_from_slice(table.extras.as_slice());
        Some(slots)
    }

    /// Get record elements.
    pub fn as_record_data(self) -> Option<&'static LispValueSlice> {
        if self.is_record() {
            let ptr = self.as_veclike_ptr().unwrap() as *const RecordObj;
            Some(unsafe { LispValueSlice::from_slice((*ptr).data.as_slice()) })
        } else {
            None
        }
    }

    /// Borrow the typed native payload of an opened-font pseudovector.
    pub(crate) fn as_font_data(self) -> Option<&'static FontObjectData> {
        if self.is_font_object() {
            let ptr = self.as_veclike_ptr().unwrap() as *const FontObj;
            Some(unsafe { &(*ptr).data })
        } else {
            None
        }
    }

    /// Borrow the data slots of a window-configuration pseudovector. Distinct
    /// from `as_vector_data`/`as_record_data` so that a window-configuration is
    /// never mistaken for a vector or a record by the type predicates.
    pub fn as_window_configuration_data(self) -> Option<&'static LispValueSlice> {
        if self.is_window_configuration() {
            let ptr = self.as_veclike_ptr().unwrap() as *const RecordObj;
            Some(unsafe { LispValueSlice::from_slice((*ptr).data.as_slice()) })
        } else {
            None
        }
    }

    /// Mutate record elements through the centralized tagged-runtime write path.
    pub fn with_record_data_mut<R>(self, f: impl FnOnce(&mut Vec<Value>) -> R) -> Option<R> {
        mutate::with_record_data_mut(self, f)
    }

    /// Replace the entire contents of a record value.
    pub fn replace_record_data(self, values: Vec<Value>) -> bool {
        mutate::replace_record_data(self, values)
    }

    /// Update a single record slot through the centralized write path.
    pub fn set_record_slot(self, index: usize, value: Value) -> bool {
        mutate::set_record_slot(self, index, value)
    }

    /// Replace the contents of either a vector or record.
    pub fn replace_vectorlike_sequence_data(self, values: Vec<Value>) -> bool {
        match self.veclike_type() {
            Some(VecLikeType::Vector) => self.replace_vector_data(values),
            Some(VecLikeType::Record) => self.replace_record_data(values),
            _ => false,
        }
    }

    /// Get hash table reference.
    pub fn as_hash_table(self) -> Option<&'static LispHashTable> {
        if self.is_hash_table() {
            let ptr = self.as_veclike_ptr().unwrap() as *mut HashTableObj;
            // Lazy dump hydration happens through the raw pointer BEFORE the
            // shared reference exists; this and `with_hash_table_mut` are the
            // only engine paths to a table, so a pending table can never be
            // observed unhydrated by Lisp. Single predictable branch when
            // already hydrated (the permanent state).
            unsafe {
                if (*ptr).table.needs_hydration() {
                    (*ptr).table.hydrate_pending();
                }
                Some(&(*ptr).table)
            }
        } else {
            None
        }
    }

    /// Mutate a hash table through the centralized tagged-runtime write path.
    pub fn with_hash_table_mut<R>(self, f: impl FnOnce(&mut LispHashTable) -> R) -> Option<R> {
        mutate::with_hash_table_mut(self, f)
    }

    /// Replace the entire contents of a hash table value.
    pub fn replace_hash_table(self, table: LispHashTable) -> bool {
        self.with_hash_table_mut(|current| *current = table)
            .is_some()
    }

    /// Borrow a GNU-shaped obarray object.
    pub fn as_obarray_obj(self) -> Option<&'static ObarrayObj> {
        if self.is_obarray() {
            let ptr = self.as_veclike_ptr().unwrap() as *const ObarrayObj;
            Some(unsafe { &*ptr })
        } else {
            None
        }
    }

    /// Mutate a GNU-shaped obarray object.
    pub fn with_obarray_mut<R>(self, f: impl FnOnce(&mut ObarrayObj) -> R) -> Option<R> {
        if !self.is_obarray() {
            return None;
        }
        // Write barrier — see `with_char_table_mut`. Obarrays are dumped and
        // mutated by `intern`; this chokepoint feeds the GC remembered set.
        note_heap_write(self, HeapWriteKind::ObarrayData);
        let ptr = self.as_veclike_ptr().unwrap() as *mut ObarrayObj;
        Some(f(unsafe { &mut *ptr }))
    }

    /// TEST-ONLY: mutate bytecode data through the centralized
    /// tagged-runtime write path. Post-publish bytecode is IMMUTABLE in
    /// production — see `mutate::with_bytecode_data_mut_for_test` for the
    /// invariant this gate enforces.
    #[cfg(test)]
    pub fn with_bytecode_data_mut_for_test<R>(
        self,
        f: impl FnOnce(&mut super::bytecode::ByteCodeFunction) -> R,
    ) -> Option<R> {
        mutate::with_bytecode_data_mut_for_test(self, f)
    }

    /// Mutate string data through the centralized tagged-runtime write path.
    pub fn with_lisp_string_mut<R>(self, f: impl FnOnce(&mut LispString) -> R) -> Option<R> {
        mutate::with_lisp_string_mut(self, f)
    }

    /// Convert to hash key based on the hash table test.
    pub fn to_hash_key(&self, test: &HashTableTest) -> HashKey {
        match test {
            HashTableTest::Eq => self.to_eq_key(),
            HashTableTest::Eql => self.to_eql_key(),
            HashTableTest::Equal => self.to_equal_key(),
        }
    }

    /// Convert to hash key with optional symbol-with-pos transparency.
    pub fn to_hash_key_swp(&self, test: &HashTableTest, symbols_with_pos_enabled: bool) -> HashKey {
        match test {
            HashTableTest::Eq => self.to_eq_key_swp(symbols_with_pos_enabled),
            HashTableTest::Eql => self.to_eql_key_swp(symbols_with_pos_enabled),
            HashTableTest::Equal => self.to_equal_key_swp(symbols_with_pos_enabled),
        }
    }

    /// EQ hash key with optional symbol-with-pos transparency.
    pub fn to_eq_key_swp(&self, symbols_with_pos_enabled: bool) -> HashKey {
        if symbols_with_pos_enabled && self.is_symbol_with_pos() {
            let sym = self.as_symbol_with_pos_sym().unwrap();
            return sym.to_eq_key();
        }
        self.to_eq_key()
    }

    /// EQL hash key with optional symbol-with-pos transparency.
    pub fn to_eql_key_swp(&self, symbols_with_pos_enabled: bool) -> HashKey {
        if symbols_with_pos_enabled && self.is_symbol_with_pos() {
            let sym = self.as_symbol_with_pos_sym().unwrap();
            return sym.to_eql_key();
        }
        self.to_eql_key()
    }

    fn to_eq_key(self) -> HashKey {
        match self.kind() {
            ValueKind::Nil => HashKey::Nil,
            ValueKind::T => HashKey::True,
            ValueKind::Fixnum(n) => HashKey::Int(n),
            ValueKind::Float => {
                // For eq, each float allocation is unique (pointer identity)
                HashKey::Ptr(self.bits())
            }
            ValueKind::Symbol(id) => HashKey::Symbol(id),
            // Static subrs: use bit pattern identity (each SymId
            // encodes to a unique immediate value).
            ValueKind::Subr(_) => HashKey::Ptr(self.bits()),
            // All heap types: use pointer identity
            ValueKind::Cons | ValueKind::String | ValueKind::Veclike(_) => {
                HashKey::Ptr(self.bits())
            }
            // `Qunbound` collapses to its unique bit pattern — two
            // UNBOUND values are `eq`. Ordinary Lisp code should
            // never stash `Qunbound` in a hash table; this arm
            // exists only so the match stays exhaustive.
            ValueKind::Unbound | ValueKind::Unknown => HashKey::Ptr(self.bits()),
        }
    }

    fn to_eql_key(self) -> HashKey {
        match self.kind() {
            ValueKind::Fixnum(n) => HashKey::Int(n),
            ValueKind::Float => HashKey::Float(self.xfloat().to_bits()),
            // Bignums are `eql` (and `equal`) by value, not by heap address.
            ValueKind::Veclike(VecLikeType::Bignum) => self.bignum_hash_key(),
            _ => self.to_eq_key(),
        }
    }

    /// Build a value-based [`HashKey`] for a bignum from its canonical
    /// two's-complement little-endian limbs. Falls back to pointer identity
    /// only if the value is somehow not a bignum.
    fn bignum_hash_key(&self) -> HashKey {
        match self.as_bignum() {
            Some(bignum) => {
                HashKey::Bignum(bignum.to_twos_complement_limbs_asc().into_boxed_slice())
            }
            None => self.to_eq_key(),
        }
    }

    fn to_equal_key(self) -> HashKey {
        let mut seen = Vec::new();
        self.to_equal_key_depth_swp(0, &mut seen, false)
    }

    fn to_equal_key_swp(self, symbols_with_pos_enabled: bool) -> HashKey {
        let mut seen = Vec::new();
        self.to_equal_key_depth_swp(0, &mut seen, symbols_with_pos_enabled)
    }

    fn to_equal_key_depth(self, depth: usize, seen: &mut Vec<usize>) -> HashKey {
        self.to_equal_key_depth_swp(depth, seen, false)
    }

    fn to_equal_key_depth_swp(
        self,
        depth: usize,
        seen: &mut Vec<usize>,
        symbols_with_pos_enabled: bool,
    ) -> HashKey {
        if depth > 200 {
            return self.to_eq_key();
        }
        match self.kind() {
            ValueKind::Nil => HashKey::Nil,
            ValueKind::T => HashKey::True,
            ValueKind::Fixnum(n) => HashKey::Int(n),
            ValueKind::Float => HashKey::Float(self.xfloat().to_bits()),
            // Bignums are `equal` by value, not by heap address.
            ValueKind::Veclike(VecLikeType::Bignum) => self.bignum_hash_key(),
            ValueKind::Symbol(id) => HashKey::Symbol(id),
            ValueKind::Veclike(VecLikeType::SymbolWithPos) => {
                let swp = self.as_symbol_with_pos().unwrap();
                if symbols_with_pos_enabled {
                    return swp.sym.to_equal_key_depth_swp(
                        depth + 1,
                        seen,
                        symbols_with_pos_enabled,
                    );
                }
                HashKey::SymbolWithPos(
                    Box::new(swp.sym.to_eq_key()),
                    Box::new(swp.pos.to_equal_key_depth_swp(
                        depth + 1,
                        seen,
                        symbols_with_pos_enabled,
                    )),
                )
            }
            ValueKind::String => {
                // Use content for equal hashing
                if let Some(s) = self.as_utf8_str() {
                    HashKey::Text(s.into())
                } else {
                    self.to_eq_key()
                }
            }
            ValueKind::Cons => {
                let ptr = self.bits();
                if let Some(index) = seen.iter().position(|&p| p == ptr) {
                    return HashKey::Cycle(index as u32);
                }
                seen.push(ptr);
                let car = self.cons_car();
                let cdr = self.cons_cdr();
                let car_key = car.to_equal_key_depth_swp(depth + 1, seen, symbols_with_pos_enabled);
                let cdr_key = cdr.to_equal_key_depth_swp(depth + 1, seen, symbols_with_pos_enabled);
                seen.pop();
                HashKey::EqualCons(Box::new(car_key), Box::new(cdr_key))
            }
            ValueKind::Veclike(kind)
                if matches!(
                    kind,
                    VecLikeType::Vector
                        | VecLikeType::Record
                        | VecLikeType::CharTable
                        | VecLikeType::SubCharTable
                ) =>
            {
                if self.is_vector()
                    && let Some(key) = bool_vector_equal_hash_key(&self)
                {
                    return key;
                }
                let ptr = self.bits();
                if let Some(index) = seen.iter().position(|&p| p == ptr) {
                    return HashKey::Cycle(index as u32);
                }
                seen.push(ptr);
                let view = StructuralPseudovectorView::from_value(self, kind)
                    .expect("structural pseudovector kind must expose its storage");
                let mut keys = Vec::with_capacity(view.len() + 3);
                keys.push(HashKey::Text(view.hash_tag().into()));
                view.append_shape_hash_keys(&mut keys);
                for index in 0..view.len() {
                    keys.push(view.slot(index).to_equal_key_depth_swp(
                        depth + 1,
                        seen,
                        symbols_with_pos_enabled,
                    ));
                }
                seen.pop();
                HashKey::EqualVec(keys.into_boxed_slice())
            }
            ValueKind::Veclike(VecLikeType::Marker) => {
                super::marker::marker_equal_hash_key_value(&self)
            }
            ValueKind::Veclike(VecLikeType::Overlay) => {
                if let Some(overlay) = self.as_overlay_data() {
                    let (start, end) = overlay.current_range();
                    HashKey::Overlay(Box::new((
                        overlay.buffer.map(|buffer| buffer.0),
                        start,
                        end,
                        overlay.plist.to_equal_key_depth_swp(
                            depth + 1,
                            seen,
                            symbols_with_pos_enabled,
                        ),
                    )))
                } else {
                    self.to_eq_key()
                }
            }
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                let ptr = self.bits();
                if let Some(index) = seen.iter().position(|&p| p == ptr) {
                    return HashKey::Cycle(index as u32);
                }
                seen.push(ptr);
                let key = bytecode_to_equal_key(self, depth + 1, seen, symbols_with_pos_enabled);
                seen.pop();
                key
            }
            ValueKind::Veclike(VecLikeType::Lambda) => {
                let ptr = self.bits();
                if let Some(index) = seen.iter().position(|&p| p == ptr) {
                    return HashKey::Cycle(index as u32);
                }
                seen.push(ptr);
                let key = closure_to_equal_key(self, depth + 1, seen);
                seen.pop();
                key
            }
            _ => self.to_eq_key(),
        }
    }

    pub(crate) fn memory_use_counts_snapshot() -> [i64; 7] {
        let mut counts = with_tagged_heap(|heap| heap.memory_use_counts_snapshot());
        let thread_local_counts = THREAD_LOCAL_ALLOCATION_COUNTS.with(|counts| counts.get());
        for (count, extra) in counts.iter_mut().zip(thread_local_counts) {
            *count = count.wrapping_add(extra);
        }
        [
            as_neovm_int(counts[MemoryUseCountSlot::ConsCells.index()]),
            as_neovm_int(counts[MemoryUseCountSlot::Floats.index()]),
            as_neovm_int(counts[MemoryUseCountSlot::VectorCells.index()]),
            as_neovm_int(counts[MemoryUseCountSlot::Symbols.index()]),
            as_neovm_int(counts[MemoryUseCountSlot::StringChars.index()]),
            as_neovm_int(counts[MemoryUseCountSlot::Intervals.index()]),
            as_neovm_int(counts[MemoryUseCountSlot::Strings.index()]),
        ]
    }
}

pub(crate) fn bool_vector_equal_hash_key(value: &Value) -> Option<HashKey> {
    let vec = value.as_vector_data()?;
    if vec.len() < 2 || vec[0].as_symbol_name()? != "--bool-vector--" {
        return None;
    }
    let len = match vec[1].kind() {
        ValueKind::Fixnum(n) if (0..=128).contains(&n) => n as usize,
        _ => return None,
    };
    if vec.len() != len + 2 {
        return None;
    }

    let mut bits = 0_u128;
    for index in 0..len {
        match vec[2 + index].as_fixnum() {
            Some(0) => {}
            Some(1) => bits |= 1_u128 << index,
            _ => return None,
        }
    }
    Some(HashKey::BoolVec(Box::new((len, bits))))
}

// ---------------------------------------------------------------------------
// Equality
// ---------------------------------------------------------------------------

/// `eq` — identity comparison (pointer equality for heap types).
/// Characters are fixnums, so `(eq ?A 65)` is `t` (same bit pattern).
pub fn eq_value(left: &Value, right: &Value) -> bool {
    left.bits() == right.bits()
}

/// EQ with optional symbol-with-pos transparency.
pub fn eq_value_swp(left: &Value, right: &Value, symbols_with_pos_enabled: bool) -> bool {
    if left.bits() == right.bits() {
        return true;
    }
    if !symbols_with_pos_enabled {
        return false;
    }
    let l = if left.is_symbol_with_pos() {
        left.as_symbol_with_pos_sym().unwrap()
    } else {
        *left
    };
    let r = if right.is_symbol_with_pos() {
        right.as_symbol_with_pos_sym().unwrap()
    } else {
        *right
    };
    l.bits() == r.bits()
}

/// `eql` — like `eq` but also value-equality for numbers of same type.
pub fn eql_value(left: &Value, right: &Value) -> bool {
    if left.bits() == right.bits() {
        return true;
    }
    match (left.kind(), right.kind()) {
        (ValueKind::Float, ValueKind::Float) => left.xfloat().to_bits() == right.xfloat().to_bits(),
        (ValueKind::Veclike(VecLikeType::Bignum), ValueKind::Veclike(VecLikeType::Bignum)) => {
            left.as_bignum().expect("left bignum") == right.as_bignum().expect("right bignum")
        }
        _ => false,
    }
}

/// EQL with optional symbol-with-pos transparency.
pub fn eql_value_swp(left: &Value, right: &Value, symbols_with_pos_enabled: bool) -> bool {
    if eq_value_swp(left, right, symbols_with_pos_enabled) {
        return true;
    }
    match (left.kind(), right.kind()) {
        (ValueKind::Float, ValueKind::Float) => left.xfloat().to_bits() == right.xfloat().to_bits(),
        (ValueKind::Veclike(VecLikeType::Bignum), ValueKind::Veclike(VecLikeType::Bignum)) => {
            left.as_bignum().expect("left bignum") == right.as_bignum().expect("right bignum")
        }
        _ => false,
    }
}

/// `equal` — structural comparison.
pub fn equal_value(left: &Value, right: &Value, depth: usize) -> bool {
    let mut seen = None;
    equal_value_inner(left, right, depth, &mut seen, false, EqualKind::Plain)
}

/// `equal-including-properties` — structural comparison that also compares
/// string text-property intervals.  This mirrors GNU Emacs `internal_equal`
/// carrying its comparison mode recursively into compound objects.
pub fn equal_value_including_properties(left: &Value, right: &Value, depth: usize) -> bool {
    let mut seen = None;
    equal_value_inner(
        left,
        right,
        depth,
        &mut seen,
        false,
        EqualKind::IncludingProperties,
    )
}

/// `equal` — structural comparison with optional symbol-with-pos transparency.
pub fn equal_value_swp(
    left: &Value,
    right: &Value,
    depth: usize,
    symbols_with_pos_enabled: bool,
) -> bool {
    let mut seen = None;
    equal_value_inner(
        left,
        right,
        depth,
        &mut seen,
        symbols_with_pos_enabled,
        EqualKind::Plain,
    )
}

pub fn try_equal_value_swp(
    left: &Value,
    right: &Value,
    depth: usize,
    symbols_with_pos_enabled: bool,
) -> Result<bool, Flow> {
    let mut seen = None;
    try_equal_value_inner(
        left,
        right,
        depth,
        &mut seen,
        symbols_with_pos_enabled,
        EqualKind::Plain,
    )
}

pub fn try_equal_value_including_properties(
    left: &Value,
    right: &Value,
    depth: usize,
) -> Result<bool, Flow> {
    try_equal_value_including_properties_swp(left, right, depth, false)
}

/// `equal-including-properties` honoring `symbols-with-pos-enabled`.  GNU's
/// `Fequal_including_properties` funnels through the same `internal_equal`
/// that reads the `symbols_with_pos_enabled` global, so a position-symbol is
/// compared to its bare symbol (and to another position of the same symbol)
/// when the flag is set — exactly like plain `equal`.  The builtin passes the
/// live `Context` flag here; a hardcoded `false` silently diverged from GNU
/// inside the byte-compiler, which binds the flag to `t`.
pub fn try_equal_value_including_properties_swp(
    left: &Value,
    right: &Value,
    depth: usize,
    symbols_with_pos_enabled: bool,
) -> Result<bool, Flow> {
    let mut seen = None;
    try_equal_value_inner(
        left,
        right,
        depth,
        &mut seen,
        symbols_with_pos_enabled,
        EqualKind::IncludingProperties,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EqualKind {
    Plain,
    IncludingProperties,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct EqualSeenPair {
    left_bits: usize,
    right_bits: usize,
}

impl EqualSeenPair {
    fn new(left: Value, right: Value) -> Self {
        Self {
            left_bits: left.bits(),
            right_bits: right.bits(),
        }
    }
}

/// Borrowed GNU-visible storage for vectorlikes compared element-by-element.
///
/// GNU `internal_equal_1` treats normal vectors, records, char tables, and
/// sub-char-tables as structural pseudovectors.  Their Rust layouts differ,
/// so exposing one typed view keeps the two equality engines on the same
/// exhaustive layout mapping without materializing temporary vectors.
#[derive(Clone, Copy)]
pub(crate) enum StructuralPseudovectorView<'a> {
    Vector(&'a [Value]),
    Record(&'a [Value]),
    CharTable(&'a CharTableObj),
    SubCharTable(&'a SubCharTableObj),
}

impl StructuralPseudovectorView<'_> {
    pub(crate) fn from_value(value: Value, kind: VecLikeType) -> Option<Self> {
        match kind {
            VecLikeType::Vector => value
                .as_vector_data()
                .map(|slots| Self::Vector(slots.as_slice())),
            VecLikeType::Record => value
                .as_record_data()
                .map(|slots| Self::Record(slots.as_slice())),
            VecLikeType::CharTable => value.as_char_table_obj().map(Self::CharTable),
            VecLikeType::SubCharTable => value.as_sub_char_table_obj().map(Self::SubCharTable),
            _ => None,
        }
    }

    fn same_shape(self, other: Self) -> bool {
        match (self, other) {
            (Self::Vector(left), Self::Vector(right))
            | (Self::Record(left), Self::Record(right)) => left.len() == right.len(),
            (Self::CharTable(left), Self::CharTable(right)) => {
                left.extras.len() == right.extras.len()
            }
            (Self::SubCharTable(left), Self::SubCharTable(right)) => {
                left.depth == right.depth
                    && left.min_char == right.min_char
                    && left.contents.len() == right.contents.len()
            }
            _ => false,
        }
    }

    pub(crate) fn len(self) -> usize {
        match self {
            Self::Vector(slots) | Self::Record(slots) => slots.len(),
            Self::CharTable(table) => 4 + CHAR_TABLE_TOP_SLOTS + table.extras.len(),
            Self::SubCharTable(table) => table.contents.len(),
        }
    }

    pub(crate) fn slot(self, index: usize) -> Value {
        match self {
            Self::Vector(slots) | Self::Record(slots) => slots[index],
            Self::CharTable(table) => match index {
                0 => table.defalt,
                1 => table.parent,
                2 => table.purpose,
                3 => table.ascii,
                index if index < 4 + CHAR_TABLE_TOP_SLOTS => table.contents[index - 4],
                _ => table.extras.as_slice()[index - 4 - CHAR_TABLE_TOP_SLOTS],
            },
            // GNU stores DEPTH and MIN_CHAR in the packed, non-Lisp prefix of
            // a sub-char-table. `same_shape` compares those typed fields;
            // only CONTENTS are recursively interpreted as Lisp values.
            Self::SubCharTable(table) => table.contents.as_slice()[index],
        }
    }

    fn hash_tag(self) -> &'static str {
        match self {
            Self::Vector(_) => "#<vector>",
            Self::Record(_) => "#<record>",
            Self::CharTable(_) => "#<char-table>",
            Self::SubCharTable(_) => "#<sub-char-table>",
        }
    }

    fn append_shape_hash_keys(self, keys: &mut Vec<HashKey>) {
        if let Self::SubCharTable(table) = self {
            keys.push(HashKey::Int(i64::from(table.depth)));
            keys.push(HashKey::Int(i64::from(table.min_char)));
        }
    }
}

fn equal_value_inner(
    left: &Value,
    right: &Value,
    depth: usize,
    seen: &mut Option<HashSet<EqualSeenPair>>,
    symbols_with_pos_enabled: bool,
    kind: EqualKind,
) -> bool {
    if depth > 200 {
        return false;
    }

    let mut left = *left;
    let mut right = *right;
    if symbols_with_pos_enabled {
        if left.is_symbol_with_pos() {
            left = left.as_symbol_with_pos_sym().unwrap();
        }
        if right.is_symbol_with_pos() {
            right = right.as_symbol_with_pos_sym().unwrap();
        }
    }

    if left.bits() == right.bits() {
        return true;
    }

    if left.is_fixnum() || right.is_fixnum() || left.is_symbol() || right.is_symbol() {
        return false;
    }

    if left.is_float() {
        return right.is_float() && left.xfloat().to_bits() == right.xfloat().to_bits();
    }
    if left.is_string() {
        return if right.is_string() {
            match (left.as_lisp_string(), right.as_lisp_string()) {
                (Some(left_string), Some(right_string)) => {
                    left_string.schars() == right_string.schars()
                        && left_string.sbytes() == right_string.sbytes()
                        && left_string.as_bytes() == right_string.as_bytes()
                        && (kind == EqualKind::Plain
                            || string_intervals_equal_including_values(
                                left,
                                right,
                                left_string.schars(),
                            ))
                }
                _ => false,
            }
        } else {
            false
        };
    }
    if left.is_cons() {
        if !right.is_cons() {
            return false;
        }
        if depth > 10 {
            let pair = EqualSeenPair::new(left, right);
            if !seen.get_or_insert_with(HashSet::new).insert(pair) {
                return true;
            }
        }
        let a_car = left.cons_car();
        let a_cdr = left.cons_cdr();
        let b_car = right.cons_car();
        let b_cdr = right.cons_cdr();
        return equal_value_inner(
            &a_car,
            &b_car,
            depth + 1,
            seen,
            symbols_with_pos_enabled,
            kind,
        ) && equal_value_inner(
            &a_cdr,
            &b_cdr,
            depth + 1,
            seen,
            symbols_with_pos_enabled,
            kind,
        );
    }

    if !left.is_veclike() || !right.is_veclike() {
        return false;
    }

    let Some(left_type) = left.veclike_type() else {
        return false;
    };
    let Some(right_type) = right.veclike_type() else {
        return false;
    };
    if left_type != right_type {
        return false;
    }

    match left_type {
        VecLikeType::Marker => {
            super::marker::marker_equal_logical_fields(&left)
                == super::marker::marker_equal_logical_fields(&right)
        }
        VecLikeType::Bignum => {
            left.as_bignum().expect("left bignum") == right.as_bignum().expect("right bignum")
        }
        VecLikeType::Overlay => {
            let Some(left_overlay) = left.as_overlay_data() else {
                return false;
            };
            let Some(right_overlay) = right.as_overlay_data() else {
                return false;
            };
            let left_range = left_overlay.current_range();
            let right_range = right_overlay.current_range();
            left_overlay.buffer == right_overlay.buffer
                && left_range == right_range
                && equal_value_inner(
                    &left_overlay.plist,
                    &right_overlay.plist,
                    depth + 1,
                    seen,
                    symbols_with_pos_enabled,
                    kind,
                )
        }
        VecLikeType::Vector
        | VecLikeType::Record
        | VecLikeType::CharTable
        | VecLikeType::SubCharTable => {
            let (Some(left_view), Some(right_view)) = (
                StructuralPseudovectorView::from_value(left, left_type),
                StructuralPseudovectorView::from_value(right, right_type),
            ) else {
                return false;
            };
            if !left_view.same_shape(right_view) {
                return false;
            }
            if depth > 10 {
                let pair = EqualSeenPair::new(left, right);
                if !seen.get_or_insert_with(HashSet::new).insert(pair) {
                    return true;
                }
            }
            for index in 0..left_view.len() {
                if !equal_value_inner(
                    &left_view.slot(index),
                    &right_view.slot(index),
                    depth + 1,
                    seen,
                    symbols_with_pos_enabled,
                    kind,
                ) {
                    return false;
                }
            }
            true
        }
        VecLikeType::HashTable => false,
        VecLikeType::ByteCode => {
            bytecode_equal(&left, &right, depth, seen, symbols_with_pos_enabled, kind)
        }
        VecLikeType::Lambda => {
            if depth > 10 {
                let pair = EqualSeenPair::new(left, right);
                if !seen.get_or_insert_with(HashSet::new).insert(pair) {
                    return true;
                }
            }
            closure_equal(
                &left,
                &right,
                depth + 1,
                seen,
                symbols_with_pos_enabled,
                kind,
            )
        }
        VecLikeType::SymbolWithPos => {
            if symbols_with_pos_enabled {
                unreachable!("symbol-with-pos values are unwrapped before equality dispatch")
            } else {
                let l = left.as_symbol_with_pos().unwrap();
                let r = right.as_symbol_with_pos().unwrap();
                l.sym.bits() == r.sym.bits() && l.pos.bits() == r.pos.bits()
            }
        }
        _ => false,
    }
}

struct EqualTailGuard {
    tortoise: Value,
    power: usize,
    distance: usize,
}

impl EqualTailGuard {
    fn new(tail: Value) -> Self {
        Self {
            tortoise: tail,
            power: 1,
            distance: 0,
        }
    }

    fn found_cycle_after_advance(&mut self, tail: Value) -> bool {
        if !tail.is_cons() {
            return false;
        }
        self.distance = self.distance.saturating_add(1);
        if tail.bits() == self.tortoise.bits() {
            return true;
        }
        if self.distance == self.power {
            self.tortoise = tail;
            self.power = self.power.saturating_mul(2).max(1);
            self.distance = 0;
        }
        false
    }
}

/// Port of GNU `internal_equal_cycle` (fns.c): the slow path taken once the left
/// list has been found circular.  `o1` is the right list, `o2` the left.  Returns
/// `true` iff the right list is also circular with matching elements (cars equal
/// up to its own cycle); `false` if the right list terminates first.  This mirrors
/// GNU's rule that two circular lists with no element differences are `equal`,
/// regardless of differing cycle structure — a cycle is not an error.
fn equal_value_cycle(
    o1: Value,
    o2: Value,
    depth: usize,
    seen: &mut Option<HashSet<EqualSeenPair>>,
    symbols_with_pos_enabled: bool,
    kind: EqualKind,
) -> Result<bool, Flow> {
    let mut o1_tail = o1;
    let mut o2_tail = o2;
    let mut guard = EqualTailGuard::new(o1_tail);
    while o1_tail.is_cons() {
        if !o2_tail.is_cons() {
            return Ok(false);
        }
        let o1_car = o1_tail.cons_car();
        let o2_car = o2_tail.cons_car();
        if !try_equal_value_inner(
            &o1_car,
            &o2_car,
            depth + 1,
            seen,
            symbols_with_pos_enabled,
            kind,
        )? {
            return Ok(false);
        }
        let o1_cdr = o1_tail.cons_cdr();
        o2_tail = o2_tail.cons_cdr();
        if o1_cdr.bits() == o2_tail.bits() {
            return Ok(true);
        }
        o1_tail = o1_cdr;
        if guard.found_cycle_after_advance(o1_tail) {
            // Cycle in o1 (right) detected.  Since o2 (left) is circular too and
            // no differences were found, the lists are `equal'.
            return Ok(true);
        }
    }
    // o1 (right) terminated but o2 (left) is circular: not equal.
    Ok(false)
}

fn try_equal_value_inner(
    left: &Value,
    right: &Value,
    depth: usize,
    seen: &mut Option<HashSet<EqualSeenPair>>,
    symbols_with_pos_enabled: bool,
    kind: EqualKind,
) -> Result<bool, Flow> {
    if depth > 200 {
        return Err(signal(
            "error",
            vec![Value::string("Stack overflow in equal")],
        ));
    }

    let mut left = *left;
    let mut right = *right;
    if symbols_with_pos_enabled {
        if left.is_symbol_with_pos() {
            left = left.as_symbol_with_pos_sym().unwrap();
        }
        if right.is_symbol_with_pos() {
            right = right.as_symbol_with_pos_sym().unwrap();
        }
    }

    if left.bits() == right.bits() {
        return Ok(true);
    }

    if left.is_fixnum() || right.is_fixnum() || left.is_symbol() || right.is_symbol() {
        return Ok(false);
    }

    if left.is_float() {
        return Ok(right.is_float() && left.xfloat().to_bits() == right.xfloat().to_bits());
    }
    if left.is_string() {
        return Ok(if right.is_string() {
            match (left.as_lisp_string(), right.as_lisp_string()) {
                (Some(left_string), Some(right_string)) => {
                    left_string.schars() == right_string.schars()
                        && left_string.sbytes() == right_string.sbytes()
                        && left_string.as_bytes() == right_string.as_bytes()
                        && (kind == EqualKind::Plain
                            || string_intervals_equal_including_values(
                                left,
                                right,
                                left_string.schars(),
                            ))
                }
                _ => false,
            }
        } else {
            false
        });
    }
    if left.is_cons() {
        if !right.is_cons() {
            return Ok(false);
        }
        let mut left_tail = left;
        let mut right_tail = right;
        let mut tail_guard = EqualTailGuard::new(left_tail);
        while left_tail.is_cons() {
            if !right_tail.is_cons() {
                return Ok(false);
            }
            let left_car = left_tail.cons_car();
            let right_car = right_tail.cons_car();
            if !try_equal_value_inner(
                &left_car,
                &right_car,
                depth + 1,
                seen,
                symbols_with_pos_enabled,
                kind,
            )? {
                return Ok(false);
            }
            let left_cdr = left_tail.cons_cdr();
            right_tail = right_tail.cons_cdr();
            if left_cdr.bits() == right_tail.bits() {
                return Ok(true);
            }
            left_tail = left_cdr;
            if tail_guard.found_cycle_after_advance(left_tail) {
                // GNU internal_equal_1: a cycle in the left list is not an error.
                // The lists are `equal' iff the right list is also circular with
                // matching elements; a finite right list terminates first → not
                // equal.  (internal_equal_cycle, called with the right list.)
                return if right_tail.is_cons() {
                    equal_value_cycle(
                        right_tail,
                        left_tail,
                        depth,
                        seen,
                        symbols_with_pos_enabled,
                        kind,
                    )
                } else {
                    Ok(false)
                };
            }
        }
        return try_equal_value_inner(
            &left_tail,
            &right_tail,
            depth + 1,
            seen,
            symbols_with_pos_enabled,
            kind,
        );
    }

    if !left.is_veclike() || !right.is_veclike() {
        return Ok(false);
    }

    let Some(left_type) = left.veclike_type() else {
        return Ok(false);
    };
    let Some(right_type) = right.veclike_type() else {
        return Ok(false);
    };
    if left_type != right_type {
        return Ok(false);
    }

    match left_type {
        VecLikeType::Marker => Ok(super::marker::marker_equal_logical_fields(&left)
            == super::marker::marker_equal_logical_fields(&right)),
        VecLikeType::Bignum => {
            Ok(left.as_bignum().expect("left bignum") == right.as_bignum().expect("right bignum"))
        }
        VecLikeType::Overlay => {
            let Some(left_overlay) = left.as_overlay_data() else {
                return Ok(false);
            };
            let Some(right_overlay) = right.as_overlay_data() else {
                return Ok(false);
            };
            let left_range = left_overlay.current_range();
            let right_range = right_overlay.current_range();
            Ok(left_overlay.buffer == right_overlay.buffer
                && left_range == right_range
                && try_equal_value_inner(
                    &left_overlay.plist,
                    &right_overlay.plist,
                    depth + 1,
                    seen,
                    symbols_with_pos_enabled,
                    kind,
                )?)
        }
        VecLikeType::Vector
        | VecLikeType::Record
        | VecLikeType::CharTable
        | VecLikeType::SubCharTable => {
            let (Some(left_view), Some(right_view)) = (
                StructuralPseudovectorView::from_value(left, left_type),
                StructuralPseudovectorView::from_value(right, right_type),
            ) else {
                return Ok(false);
            };
            if !left_view.same_shape(right_view) {
                return Ok(false);
            }
            if depth > 10 {
                let pair = EqualSeenPair::new(left, right);
                if !seen.get_or_insert_with(HashSet::new).insert(pair) {
                    return Ok(true);
                }
            }
            for index in 0..left_view.len() {
                if !try_equal_value_inner(
                    &left_view.slot(index),
                    &right_view.slot(index),
                    depth + 1,
                    seen,
                    symbols_with_pos_enabled,
                    kind,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        VecLikeType::HashTable => Ok(false),
        VecLikeType::ByteCode => {
            try_bytecode_equal(&left, &right, depth, seen, symbols_with_pos_enabled, kind)
        }
        VecLikeType::Lambda => {
            if depth > 10 {
                let pair = EqualSeenPair::new(left, right);
                if !seen.get_or_insert_with(HashSet::new).insert(pair) {
                    return Ok(true);
                }
            }
            try_closure_equal(
                &left,
                &right,
                depth + 1,
                seen,
                symbols_with_pos_enabled,
                kind,
            )
        }
        VecLikeType::SymbolWithPos => {
            if symbols_with_pos_enabled {
                unreachable!("symbol-with-pos values are unwrapped before equality dispatch")
            } else {
                let l = left.as_symbol_with_pos().unwrap();
                let r = right.as_symbol_with_pos().unwrap();
                Ok(l.sym.bits() == r.sym.bits() && l.pos.bits() == r.pos.bits())
            }
        }
        _ => Ok(false),
    }
}

fn closure_params_to_equal_key(params: &LambdaParams) -> HashKey {
    let mut values = Vec::with_capacity(params.required.len() + params.optional.len() + 3);
    values.push(HashKey::Text("params".into()));
    for sym in &params.required {
        values.push(HashKey::Symbol(*sym));
    }
    if !params.optional.is_empty() {
        values.push(HashKey::Text("&optional".into()));
        for sym in &params.optional {
            values.push(HashKey::Symbol(*sym));
        }
    }
    if let Some(rest) = params.rest {
        values.push(HashKey::Text("&rest".into()));
        values.push(HashKey::Symbol(rest));
    }
    HashKey::EqualVec(values.into_boxed_slice())
}

/// `equal`-table key for a byte-code object, derived from
/// [`ByteCodeFunction::structural_slots`] so it agrees with [`bytecode_equal`]
/// by construction: equal objects produce equal keys, and every difference
/// `equal` sees -- including an absent versus a present-but-`nil` captured
/// environment, and a docstring's character count -- reaches the key.  GNU's
/// `sxhash_obj` -> `sxhash_vector` gives equal closures one bucket
/// (src/fns.c:5525-5536); this is the table's side of that promise.
fn bytecode_to_equal_key(
    value: Value,
    depth: usize,
    seen: &mut Vec<usize>,
    symbols_with_pos_enabled: bool,
) -> HashKey {
    use super::bytecode::ByteCodeSlot;
    if depth > 200 {
        return HashKey::Text("#<byte-code-depth-limit>".into());
    }
    let Some(bc) = value.get_bytecode_data() else {
        return value.to_eq_key();
    };
    let mut key_of =
        |slot: &Value| slot.to_equal_key_depth_swp(depth + 1, seen, symbols_with_pos_enabled);
    fn byte_keys(bytes: &[u8]) -> impl Iterator<Item = HashKey> + '_ {
        bytes.iter().map(|byte| HashKey::Int(i64::from(*byte)))
    }

    let slots = bc.structural_slots();
    let mut keys = Vec::with_capacity(slots.len() + 2);
    keys.push(HashKey::Text("#<byte-code>".into()));
    keys.push(HashKey::Int(bc.observable_closure_slot_count() as i64));
    for slot in slots {
        keys.push(match slot {
            ByteCodeSlot::Value(value) => key_of(&value),
            ByteCodeSlot::Bytes(bytes) => HashKey::EqualVec(byte_keys(bytes).collect()),
            ByteCodeSlot::Ops(ops) => HashKey::Text(format!("#<ops {ops:?}>").into()),
            ByteCodeSlot::Values(values) => {
                HashKey::EqualVec(values.iter().map(&mut key_of).collect())
            }
            // GNU string equality is characters + bytes + contents; the
            // representation flag is not part of it, and the character count
            // is (two raw bytes are not one multibyte character).
            ByteCodeSlot::Text(text) => HashKey::EqualVec(
                std::iter::once(HashKey::Int(text.schars() as i64))
                    .chain(byte_keys(text.as_bytes()))
                    .collect(),
            ),
            ByteCodeSlot::Absent => HashKey::Text("#<absent>".into()),
        });
    }
    HashKey::EqualVec(keys.into_boxed_slice())
}

fn closure_to_equal_key(value: Value, depth: usize, seen: &mut Vec<usize>) -> HashKey {
    if depth > 200 {
        return HashKey::Text("#<lambda-depth-limit>".into());
    }

    let Some(params) = value.closure_params() else {
        return HashKey::Text("#<invalid-lambda>".into());
    };

    let mut slots = vec![
        HashKey::Text("lambda".into()),
        closure_params_to_equal_key(params),
        value
            .closure_body_value()
            .map_or(HashKey::Nil, |body| body.to_equal_key_depth(0, seen)),
        match value.closure_env().unwrap_or(None) {
            Some(env) => env.to_equal_key_depth(0, seen),
            None => HashKey::Text("dynamic".into()),
        },
    ];

    if let Some(doc_value) = value.closure_doc_value()
        && !doc_value.is_nil()
    {
        slots.push(HashKey::Nil);
        // Issue #131: key the docstring by content the same way as any other
        // string value (valid UTF-8 -> text key, eight-bit -> eq key) instead of
        // the retired storage form, whose in-Unicode sentinels would collide a
        // real Private-Use glyph with a raw byte in the equal hash key.
        let doc = doc_value.to_equal_key_depth(0, seen);
        slots.push(doc);
    }

    HashKey::EqualVec(slots.into_boxed_slice())
}

fn closure_equal(
    left: &Value,
    right: &Value,
    depth: usize,
    seen: &mut Option<HashSet<EqualSeenPair>>,
    symbols_with_pos_enabled: bool,
    kind: EqualKind,
) -> bool {
    let (Some(left_params), Some(right_params)) = (left.closure_params(), right.closure_params())
    else {
        return false;
    };
    if left_params != right_params {
        return false;
    }

    let body_equal = match (left.closure_body_value(), right.closure_body_value()) {
        (Some(left_body), Some(right_body)) => equal_value_inner(
            &left_body,
            &right_body,
            depth + 1,
            seen,
            symbols_with_pos_enabled,
            kind,
        ),
        (None, None) => true,
        _ => false,
    };
    if !body_equal {
        return false;
    }

    let env_equal = match (
        left.closure_env().unwrap_or(None),
        right.closure_env().unwrap_or(None),
    ) {
        (None, None) => true,
        (Some(l), Some(r)) => {
            equal_value_inner(&l, &r, depth + 1, seen, symbols_with_pos_enabled, kind)
        }
        _ => false,
    };
    if !env_equal || left.closure_docstring().flatten() != right.closure_docstring().flatten() {
        return false;
    }

    match (
        left.closure_doc_form().flatten(),
        right.closure_doc_form().flatten(),
    ) {
        (None, None) => true,
        (Some(l), Some(r)) => {
            equal_value_inner(&l, &r, depth + 1, seen, symbols_with_pos_enabled, kind)
        }
        _ => false,
    }
}

fn try_closure_equal(
    left: &Value,
    right: &Value,
    depth: usize,
    seen: &mut Option<HashSet<EqualSeenPair>>,
    symbols_with_pos_enabled: bool,
    kind: EqualKind,
) -> Result<bool, Flow> {
    let (Some(left_params), Some(right_params)) = (left.closure_params(), right.closure_params())
    else {
        return Ok(false);
    };
    if left_params != right_params {
        return Ok(false);
    }

    let body_equal = match (left.closure_body_value(), right.closure_body_value()) {
        (Some(left_body), Some(right_body)) => try_equal_value_inner(
            &left_body,
            &right_body,
            depth + 1,
            seen,
            symbols_with_pos_enabled,
            kind,
        )?,
        (None, None) => true,
        _ => false,
    };
    if !body_equal {
        return Ok(false);
    }

    let env_equal = match (
        left.closure_env().unwrap_or(None),
        right.closure_env().unwrap_or(None),
    ) {
        (None, None) => true,
        (Some(l), Some(r)) => {
            try_equal_value_inner(&l, &r, depth + 1, seen, symbols_with_pos_enabled, kind)?
        }
        _ => false,
    };
    if !env_equal || left.closure_docstring().flatten() != right.closure_docstring().flatten() {
        return Ok(false);
    }

    match (
        left.closure_doc_form().flatten(),
        right.closure_doc_form().flatten(),
    ) {
        (None, None) => Ok(true),
        (Some(l), Some(r)) => {
            try_equal_value_inner(&l, &r, depth + 1, seen, symbols_with_pos_enabled, kind)
        }
        _ => Ok(false),
    }
}

/// GNU string equality (src/fns.c `internal_equal_1`, `Lisp_String`):
/// character count, byte count and contents.  The unibyte/multibyte
/// representation flag `LispString::eq` also compares is not part of it: an
/// ASCII string stored unibyte equals the same text stored multibyte, while
/// two raw bytes never equal the one multibyte character with those bytes.
fn gnu_string_contents_equal(left: &LispString, right: &LispString) -> bool {
    left.schars() == right.schars()
        && left.sbytes() == right.sbytes()
        && left.as_bytes() == right.as_bytes()
}

/// The slot walk behind both [`bytecode_equal`] and [`try_bytecode_equal`]:
/// GNU's `ASIZE` check first, then [`ByteCodeFunction::structural_slots`]
/// pairwise, with `value_equal` deciding the Lisp values (constants at one
/// extra level of depth, as elements of the vector they sit in).
fn bytecode_slots_equal(
    left: &super::bytecode::ByteCodeFunction,
    right: &super::bytecode::ByteCodeFunction,
    depth: usize,
    value_equal: &mut dyn FnMut(&Value, &Value, usize) -> Result<bool, Flow>,
) -> Result<bool, Flow> {
    use super::bytecode::ByteCodeSlot;
    if left.observable_closure_slot_count() != right.observable_closure_slot_count() {
        return Ok(false);
    }
    let (left_slots, right_slots) = (left.structural_slots(), right.structural_slots());
    if left_slots.len() != right_slots.len() {
        return Ok(false);
    }
    for (left, right) in left_slots.into_iter().zip(right_slots) {
        let equal = match (left, right) {
            (ByteCodeSlot::Value(a), ByteCodeSlot::Value(b)) => value_equal(&a, &b, depth + 1)?,
            (ByteCodeSlot::Bytes(a), ByteCodeSlot::Bytes(b)) => a == b,
            (ByteCodeSlot::Ops(a), ByteCodeSlot::Ops(b)) => a == b,
            (ByteCodeSlot::Values(a), ByteCodeSlot::Values(b)) => {
                if a.len() != b.len() {
                    return Ok(false);
                }
                for (a, b) in a.iter().zip(b) {
                    if !value_equal(a, b, depth + 2)? {
                        return Ok(false);
                    }
                }
                true
            }
            (ByteCodeSlot::Text(a), ByteCodeSlot::Text(b)) => gnu_string_contents_equal(a, b),
            (ByteCodeSlot::Absent, ByteCodeSlot::Absent) => true,
            _ => false,
        };
        if !equal {
            return Ok(false);
        }
    }
    Ok(true)
}

/// GNU `internal_equal_1` compares a `PVEC_CLOSURE` exactly like a vector
/// (src/fns.c:2984-2998 in emacs-31.1, :2987-3001 on master): the `ASIZE`
/// test first -- which is also the type test, so a five-slot closure never
/// equals a four-slot one -- then every slot element-wise.  This port keeps
/// a byte-code function's GNU slots in typed fields, so the walk reads
/// [`ByteCodeFunction::structural_slots`], the one place that spells the
/// schema; `make-closure` (bytecode.c `Fmake_closure`) copies the prototype
/// and replaces the leading constants, so two instances of one prototype
/// with `equal` captures are `equal` -- the property `remove-hook` needs to
/// find a closure it was handed a freshly rebuilt twin of.
fn bytecode_equal(
    left: &Value,
    right: &Value,
    depth: usize,
    seen: &mut Option<HashSet<EqualSeenPair>>,
    symbols_with_pos_enabled: bool,
    kind: EqualKind,
) -> bool {
    let (Some(l), Some(r)) = (left.get_bytecode_data(), right.get_bytecode_data()) else {
        return false;
    };
    if depth > 10 {
        let pair = EqualSeenPair::new(*left, *right);
        if !seen.get_or_insert_with(HashSet::new).insert(pair) {
            return true;
        }
    }
    bytecode_slots_equal(l, r, depth, &mut |a, b, depth| {
        Ok(equal_value_inner(
            a,
            b,
            depth,
            seen,
            symbols_with_pos_enabled,
            kind,
        ))
    })
    .unwrap_or(false)
}

/// [`bytecode_equal`] for the error-propagating walk.
fn try_bytecode_equal(
    left: &Value,
    right: &Value,
    depth: usize,
    seen: &mut Option<HashSet<EqualSeenPair>>,
    symbols_with_pos_enabled: bool,
    kind: EqualKind,
) -> Result<bool, Flow> {
    let (Some(l), Some(r)) = (left.get_bytecode_data(), right.get_bytecode_data()) else {
        return Ok(false);
    };
    if depth > 10 {
        let pair = EqualSeenPair::new(*left, *right);
        if !seen.get_or_insert_with(HashSet::new).insert(pair) {
            return Ok(true);
        }
    }
    bytecode_slots_equal(l, r, depth, &mut |a, b, depth| {
        try_equal_value_inner(a, b, depth, seen, symbols_with_pos_enabled, kind)
    })
}

// ---------------------------------------------------------------------------
// List iteration helpers
// ---------------------------------------------------------------------------

/// Collect a proper list into a Vec.
pub fn list_to_vec(value: &Value) -> Option<Vec<Value>> {
    // Argument lists and parse states are short; one allocation instead of
    // the 0->4->8->16 growth chain (three reallocations for an 11-element
    // `parse-partial-sexp' state, ~600 Ir of a 1K call).
    let mut result = Vec::with_capacity(16);
    let mut tortoise = *value;
    let mut hare = *value;
    let mut step = 0u64;
    loop {
        if hare.is_nil() {
            return Some(result);
        } else if hare.is_cons() {
            result.push(hare.cons_car());
            hare = hare.cons_cdr();
            step += 1;
            if step.is_multiple_of(2) {
                if tortoise.is_cons() {
                    tortoise = tortoise.cons_cdr();
                }
                if tortoise.bits() == hare.bits() {
                    return None; // cycle
                }
            }
        } else {
            return None;
        }
    }
}

/// Length of a list (counts cons cells).
pub fn list_length(value: &Value) -> Option<usize> {
    let mut len = 0;
    let mut tortoise = *value;
    let mut hare = *value;
    loop {
        if hare.is_nil() {
            return Some(len);
        } else if hare.is_cons() {
            len += 1;
            hare = hare.cons_cdr();
            if hare.is_nil() {
                return Some(len);
            } else if hare.is_cons() {
                len += 1;
                hare = hare.cons_cdr();
            } else {
                return None; // improper
            }
            if tortoise.is_cons() {
                tortoise = tortoise.cons_cdr();
            }
            if tortoise.bits() == hare.bits() {
                return None; // cycle
            }
        } else {
            return None;
        }
    }
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

impl fmt::Display for TaggedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", super::print::print_value(self))
    }
}

// ---------------------------------------------------------------------------
// Flat cons-alist lexical environment helpers
// ---------------------------------------------------------------------------

/// Walk a cons-alist lexenv for a symbol. Returns the cons cell Value
/// of the `(sym . val)` binding, or `None` if not found.
pub fn lexenv_assq(lexenv: Value, sym_id: SymId) -> Option<Value> {
    let mut cursor = lexenv;
    while cursor.is_cons() {
        // Mirrors GNU `Fassq`: after CONSP succeeds, use direct XCAR/XCDR
        // style loads instead of re-running generic accessor checks.
        let cursor_ptr = cons_ptr_unchecked(cursor);
        let car = unsafe { cons_car_unchecked(cursor_ptr) };
        if car.is_cons() {
            let car_ptr = cons_ptr_unchecked(car);
            let binding_sym = unsafe { cons_car_unchecked(car_ptr) };
            if lexenv_binding_symbol_matches(binding_sym, sym_id) {
                return Some(car);
            }
        }
        cursor = unsafe { cons_cdr_unchecked(cursor_ptr) };
    }
    None
}

#[inline(always)]
fn lexenv_binding_symbol_matches(value: Value, sym_id: SymId) -> bool {
    let bits = value.bits();
    bits != Value::UNBOUND.bits() && bits == (sym_id.0 as usize) << TAG_BITS
}

pub(crate) fn lexenv_binding_symbol_value(sym_id: SymId) -> Value {
    TaggedValue::from_sym_id(sym_id)
}

/// Look up symbol value in a cons-alist lexenv.
pub fn lexenv_lookup(lexenv: Value, sym_id: SymId) -> Option<Value> {
    let cell = lexenv_assq(lexenv, sym_id)?;
    Some(unsafe { cons_cdr_unchecked(cons_ptr_unchecked(cell)) })
}

/// Return true if the lexical environment contains a bare-symbol declaration
/// marking SYM_ID as locally special/dynamic.
pub fn lexenv_declares_special(lexenv: Value, sym_id: SymId) -> bool {
    let mut cursor = lexenv;
    let target_bits = TaggedValue::from_sym_id(sym_id).bits();
    while cursor.is_cons() {
        let cursor_ptr = cons_ptr_unchecked(cursor);
        let car = unsafe { cons_car_unchecked(cursor_ptr) };
        if car.bits() == target_bits {
            return true;
        }
        cursor = unsafe { cons_cdr_unchecked(cursor_ptr) };
    }
    false
}

#[inline(always)]
fn cons_ptr_unchecked(value: Value) -> *const ConsCell {
    (value.bits() & !TAG_MASK) as *const ConsCell
}

#[inline(always)]
unsafe fn cons_car_unchecked(ptr: *const ConsCell) -> Value {
    unsafe { std::ptr::addr_of!((*ptr).car).read() }
}

#[inline(always)]
unsafe fn cons_cdr_unchecked(ptr: *const ConsCell) -> Value {
    unsafe { std::ptr::addr_of!((*ptr).cdr_or_next.cdr).read() }
}

/// Mutate a binding in place: set cdr of the `(sym . val)` cons cell.
pub fn lexenv_set(cell: Value, value: Value) {
    cell.set_cdr(value);
}

/// Prepend a `(sym . val)` binding onto a lexenv alist. Returns the new head.
pub fn lexenv_prepend(lexenv: Value, sym_id: SymId, val: Value) -> Value {
    let binding = Value::make_cons(lexenv_binding_symbol_value(sym_id), val);
    Value::make_cons(binding, lexenv)
}

// ---------------------------------------------------------------------------
// Test assertion helpers
// ---------------------------------------------------------------------------

/// Structural equality assertion for Values.
///
/// `PartialEq` on `TaggedValue` is bitwise (pointer identity for heap types),
/// matching GNU Emacs `eq` semantics. Tests that compare VALUES structurally
/// (like `assert_eq!(eval("(cons 1 2)"), Value::cons(...))`) must use this
/// macro instead of `assert_eq!`.
#[cfg(test)]
#[macro_export]
macro_rules! assert_val_eq {
    ($left:expr, $right:expr) => {{
        let left_val = &$left;
        let right_val = &$right;
        if !$crate::emacs_core::value::equal_value(left_val, right_val, 0) {
            panic!(
                "assertion `left == right` failed (structural)\n  left: {}\n right: {}",
                $crate::emacs_core::print::print_value(left_val),
                $crate::emacs_core::print::print_value(right_val),
            );
        }
    }};
    ($left:expr, $right:expr, $($msg:tt)+) => {{
        let left_val = &$left;
        let right_val = &$right;
        if !$crate::emacs_core::value::equal_value(left_val, right_val, 0) {
            panic!(
                "assertion `left == right` failed (structural): {}\n  left: {}\n right: {}",
                format_args!($($msg)+),
                $crate::emacs_core::print::print_value(left_val),
                $crate::emacs_core::print::print_value(right_val),
            );
        }
    }};
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
