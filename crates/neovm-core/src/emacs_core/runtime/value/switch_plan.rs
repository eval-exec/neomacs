//! Jump-table dispatch for GNU `Bswitch` (src/bytecode.c) through a plan
//! compiled from the table's keys.
//!
//! Both `switch` tiers used to answer every dispatch with the general
//! hash-table probe. For an `equal` table keyed by conses -- what `pcase`
//! backquote patterns compile to -- that walked the dispatch value three
//! times (admit, hash, compare against a boxed key tree): ~87 cycles a
//! dispatch on elb-pcase, 82% of the row.
//!
//! A jump table is a constant with a handful of keys, dispatched over and
//! over. So the table carries a [`SwitchPlanCache`]: on its second dispatch
//! the keys are compiled into a POD plan (sorted by the value tag they can
//! match) and every later dispatch answers from it:
//!
//! - a dense fixnum index, when every key is a fixnum in a short span;
//! - a bit-identity scan, when every key is one value's bit pattern (nil, t,
//!   fixnums, symbols; heap identity under `eq`/`eql`);
//! - pre-order "key programs" walked in lockstep with the value, for
//!   `equal` structure (conses of those leaves, floats by bits, strings by
//!   content);
//! - for larger tables, only a prefilter on the value's tag before the
//!   hashed lookup.
//!
//! The answer is, by construction, exactly [`HashTableStorage::lookup`]'s:
//! the plan is compiled from the index's own [`HashKey`] snapshots (what the
//! lookup compares against) and declines, to the hashed lookup, every key
//! shape and table it cannot reproduce. Debug builds check that on every
//! dispatch.
//!
//! The plan holds no traced `Value`: targets are fixnums, keys are raw bits
//! that are only compared (a heap key's object stays alive through the
//! table's own slot while the plan exists), strings are copied bytes. It is
//! dropped by every mutation of the table (`with_hash_table_mut` and the
//! storage's bulk mutators), so it never goes stale.
//!
//! [`HashTableStorage::lookup`]: super::HashTableStorage::lookup
use std::fmt;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{HashKey, HashTableTest, LispHashTable, Value, ValueKind};
use crate::tagged::value::{FIXNUM_SHIFT, TAG_CONS, TAG_FLOAT, TAG_MASK, TAG_STRING};

/// Keys a bit-identity scan may hold; larger all-immediate tables prefilter
/// and hash. Matches `small_identity_scan`'s 32.
const LINEAR_MAX_BITS: usize = 32;
/// Keys a key-program scan may hold; larger structural tables prefilter and
/// hash.
const LINEAR_MAX_NODE_KEYS: usize = 16;
/// Nodes one key program may hold.
const MAX_KEY_NODES: usize = 64;
/// Bound on the pending-cdr stack of [`match_key`]: how deeply conses may nest
/// in CAR position (a list's spine is cdr-linked and does not count).
const MAX_CAR_DEPTH: usize = 8;
/// Widest `max - min` a dense fixnum index may cover.
const DENSE_MAX_SPAN: u64 = 256;
/// Built plans a table's mutations may drop before it stops being planned: a
/// jump table mutated between dispatches is not what the byte compiler
/// emits, and rebuilding for it forever would cost more than it saves.
const MAX_REBUILDS: u8 = 4;
/// A dense fixnum slot no key occupies.
const NO_ENTRY: u16 = u16::MAX;

/// The byte offset of a table's [`super::HashTableStorage::switch_epoch`]
/// from the start of its `HashTableObj` (the untagged pointer of a hash table
/// value): JIT code reads it to check that a jump table is still the one it
/// was compiled against.
pub(crate) const SWITCH_EPOCH_OFFSET: usize =
    core::mem::offset_of!(crate::tagged::header::HashTableObj, table.data.switch_epoch);

/// The low-3-bit value tags some key can match: bit `t` is set when a key can
/// equal a value whose tag is `t`. A value outside the set misses without
/// looking at any key (elb-pcase's fixnum inputs against a table of conses).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct TagSet(u8);

impl TagSet {
    #[inline(always)]
    fn admits(self, v: Value) -> bool {
        self.0 & (1u8 << (v.bits() & TAG_MASK)) != 0
    }

    fn insert(&mut self, tag: usize) {
        self.0 |= 1u8 << tag;
    }
}

impl fmt::Debug for TagSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TagSet({:#010b})", self.0)
    }
}

/// The per-table plan cache, a field of [`super::HashTableStorage`].
#[derive(Default)]
pub(crate) struct SwitchPlanCache {
    slot: OnceLock<PlanSlot>,
    /// Set by the table's first dispatch. The plan is built on the SECOND,
    /// so a jump table dispatched once (a top-level `byte-code` form run at
    /// load time) never pays for a build.
    seen: AtomicBool,
    /// How many built plans a mutation has dropped; at [`MAX_REBUILDS`] the
    /// table is planned as [`PlanSlot::Generic`].
    rebuilds: u8,
}

/// `copy-hash-table` starts cold: the copy is a different object, mutated
/// independently, and builds its own plan if it is ever dispatched on.
impl Clone for SwitchPlanCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl fmt::Debug for SwitchPlanCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SwitchPlanCache")
            .field("shape", &self.shape())
            .field("rebuilds", &self.rebuilds)
            .finish()
    }
}

impl SwitchPlanCache {
    /// Drop the plan: the table is about to change. Called before every
    /// mutation, so it costs one load when there is no plan (every table
    /// that is not a jump table).
    #[inline]
    pub(crate) fn invalidate(&mut self) {
        if self.slot.take().is_some() {
            self.rebuilds = self.rebuilds.saturating_add(1);
        }
    }

    /// The built plan's shape, or `None` before the table has been planned.
    pub(crate) fn shape(&self) -> Option<PlanShape> {
        self.slot.get().map(PlanSlot::shape)
    }
}

/// How a planned table answers; for tracing and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PlanShape {
    /// The hashed lookup, unfiltered.
    Generic,
    /// A tag prefilter, then the hashed lookup.
    Hashed,
    /// `value - base` indexes a dense slot array.
    DenseFixnum,
    /// A bit-identity scan of the keys sharing the value's tag.
    Bits,
    /// A lockstep walk of the value against each key program sharing its tag.
    Nodes,
}

enum PlanSlot {
    /// Answer with the hashed lookup: weak or user-test tables, keys the plan
    /// cannot reproduce, targets that are not byte offsets, or a table that
    /// hit the rebuild cap.
    Generic,
    /// Too many keys to scan, but every key's tag is known: prefilter, then
    /// the hashed lookup.
    Hashed {
        tags: TagSet,
    },
    Plan {
        tags: TagSet,
        body: Box<PlanBody>,
    },
}

struct PlanBody {
    /// Keys sorted by tag: `ranges[t]` is the `[lo, hi)` run of keys whose
    /// tag is `t` (unused by `DenseFixnum`).
    ranges: [(u16, u16); 8],
    /// Each key's jump target, a non-negative fixnum (a byte offset).
    targets: Box<[Value]>,
    keys: PlanKeys,
}

enum PlanKeys {
    /// Every key is one value's bit pattern.
    Bits(Box<[usize]>),
    /// Every key is a fixnum within [`DENSE_MAX_SPAN`] of `base`: slot
    /// `n - base` holds the key's index into `targets`, or [`NO_ENTRY`].
    DenseFixnum { base: i64, slots: Box<[u16]> },
    /// Key `k`'s program is `nodes[starts[k]..starts[k + 1]]`.
    Nodes {
        starts: Box<[u32]>,
        nodes: Box<[KeyNode]>,
        strings: Box<[StringKey]>,
    },
}

/// One node of a key program: a pre-order walk of the key's `HashKey`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyNode {
    /// `v.bits() == b`: nil, t, a fixnum or a bare symbol under any test, or
    /// a heap object's identity under `eq`/`eql`.
    Bits(usize),
    /// A float whose IEEE bits are these (GNU `same_float`: 0.0 is not
    /// -0.0, and a NaN matches only its own payload).
    Float(u64),
    /// A string equal by content: an index into the plan's strings.
    String(u32),
    /// A cons: the car's program follows, then the cdr's.
    Cons,
}

impl KeyNode {
    /// The tag of every value this node's key can match.
    fn tag(self) -> usize {
        match self {
            KeyNode::Bits(bits) => bits & TAG_MASK,
            KeyNode::Float(_) => TAG_FLOAT,
            KeyNode::String(_) => TAG_STRING,
            KeyNode::Cons => TAG_CONS,
        }
    }
}

/// A copy of `HashKey::StringContent`: the bytes and SCHARS, which is what
/// `equal` compares (text properties are ignored).
struct StringKey {
    schars: usize,
    bytes: Box<[u8]>,
}

#[cfg(test)]
thread_local! {
    static PLAN_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Test-only: how many plans this thread has built.
#[cfg(test)]
pub(crate) fn plan_builds_for_test() -> usize {
    PLAN_BUILDS.with(std::cell::Cell::get)
}

impl LispHashTable {
    /// The target GNU `Bswitch` (src/bytecode.c) jumps to for `value`,
    /// answered through the table's switch plan. Always exactly
    /// `self.data.lookup(value, self.test, swp).copied()`.
    #[inline(always)]
    pub(crate) fn switch_target(&self, value: Value, swp: bool) -> Option<Value> {
        let answer = match self.data.switch_plan.slot.get() {
            Some(slot) => slot.lookup(self, value, swp),
            None => self.switch_target_unplanned(value, swp),
        };
        #[cfg(debug_assertions)]
        self.check_switch_target(value, swp, answer);
        answer
    }

    #[cfg(debug_assertions)]
    #[cold]
    #[inline(never)]
    fn check_switch_target(&self, value: Value, swp: bool, answer: Option<Value>) {
        assert_eq!(
            answer.map(Value::bits),
            self.data
                .lookup(value, self.test, swp)
                .map(|target| target.bits()),
            "switch plan ({:?}) diverged from the hashed lookup for a {:?} dispatch value \
             ({:#x}) under {:?}, symbols-with-pos-enabled {swp}",
            self.data.switch_plan.shape(),
            value.kind(),
            value.bits(),
            self.test,
        );
    }

    /// The table has no plan yet: the first dispatch only notes that it
    /// happened, the second builds the plan.
    #[cold]
    #[inline(never)]
    fn switch_target_unplanned(&self, value: Value, swp: bool) -> Option<Value> {
        let cache = &self.data.switch_plan;
        // A weak table is mutated by the GC's sweep, which does not drop a
        // plan; never cache one for it.
        if self.weakness.is_none() && cache.seen.load(Ordering::Relaxed) {
            let slot = cache
                .slot
                .get_or_init(|| PlanSlot::build(self, cache.rebuilds));
            return slot.lookup(self, value, swp);
        }
        cache.seen.store(true, Ordering::Relaxed);
        self.data.lookup(value, self.test, swp).copied()
    }
}

impl PlanSlot {
    #[inline(always)]
    fn lookup(&self, table: &LispHashTable, v: Value, swp: bool) -> Option<Value> {
        match self {
            PlanSlot::Plan { tags, body } => {
                if swp {
                    return body.lookup_swp(*tags, v);
                }
                if !tags.admits(v) {
                    return None;
                }
                body.lookup_plain(v)
            }
            PlanSlot::Hashed { tags } => {
                // A positioned symbol's own tag says nothing about the bare
                // symbol it stands for, so the prefilter needs `swp` off.
                if !swp && !tags.admits(v) {
                    return None;
                }
                table.data.lookup(v, table.test, swp).copied()
            }
            PlanSlot::Generic => table.data.lookup(v, table.test, swp).copied(),
        }
    }

    fn shape(&self) -> PlanShape {
        match self {
            PlanSlot::Generic => PlanShape::Generic,
            PlanSlot::Hashed { .. } => PlanShape::Hashed,
            PlanSlot::Plan { body, .. } => match body.keys {
                PlanKeys::Bits(_) => PlanShape::Bits,
                PlanKeys::DenseFixnum { .. } => PlanShape::DenseFixnum,
                PlanKeys::Nodes { .. } => PlanShape::Nodes,
            },
        }
    }

    #[cold]
    #[inline(never)]
    fn build(table: &LispHashTable, rebuilds: u8) -> PlanSlot {
        #[cfg(test)]
        PLAN_BUILDS.with(|builds| builds.set(builds.get() + 1));
        let slot = Self::compile(table, rebuilds);
        tracing::debug!(
            target: "neovm::switch_plan",
            keys = table.data.len(),
            test = ?table.test,
            shape = ?slot.shape(),
            rebuilds,
            "built jump-table plan"
        );
        slot
    }

    fn compile(table: &LispHashTable, rebuilds: u8) -> PlanSlot {
        debug_assert!(
            !table.needs_hydration(),
            "a table reaches dispatch through as_hash_table, which hydrates it"
        );
        if rebuilds >= MAX_REBUILDS
            || table.weakness.is_some()
            || table.user_cmp_function.is_some()
            || table.user_hash_function.is_some()
        {
            return PlanSlot::Generic;
        }
        let test = table.test;
        let data = &table.data;
        let mut keys: Vec<CompiledKey> = Vec::with_capacity(data.len());
        let mut nodes: Vec<KeyNode> = Vec::new();
        let mut strings: Vec<StringKey> = Vec::new();
        // Compile the INDEX's key snapshots, not the key objects: the
        // snapshots are what the hashed lookup compares against.
        for (hash_key, &slot) in &data.index {
            let Some(entry) = data.slots.get(slot).and_then(Option::as_ref) else {
                return PlanSlot::Generic;
            };
            // Anything but a byte offset keeps each tier's own handling of
            // it (tier-0 signals, the JIT reports a stale table).
            if !entry.value.as_fixnum().is_some_and(|target| target >= 0) {
                return PlanSlot::Generic;
            }
            // Small `eq`/`eql` tables answer a fixnum or symbol by comparing
            // the stored key OBJECT's bits (`small_identity_scan`), not the
            // index key. The two agree only while the object is the one the
            // index key was made from; a table whose key objects were swapped
            // or positioned under `symbols-with-pos-enabled` is not planned.
            if test != HashTableTest::Equal && entry.key.to_hash_key(&test) != *hash_key {
                return PlanSlot::Generic;
            }
            let start = nodes.len();
            let limit = start + MAX_KEY_NODES;
            if flatten(hash_key, test, &mut nodes, &mut strings, 0, limit).is_none() {
                return PlanSlot::Generic;
            }
            keys.push(CompiledKey {
                start,
                end: nodes.len(),
                target: entry.value,
            });
        }

        let mut tags = TagSet::default();
        for key in &keys {
            tags.insert(nodes[key.start].tag());
        }
        let all_bits = keys
            .iter()
            .all(|key| key.end - key.start == 1 && matches!(nodes[key.start], KeyNode::Bits(_)));
        if all_bits
            && !keys.is_empty()
            && let Some(dense) = dense_fixnums(&keys, &nodes)
        {
            return PlanSlot::Plan {
                tags,
                body: Box::new(dense),
            };
        }
        let scan_limit = if all_bits {
            LINEAR_MAX_BITS
        } else {
            LINEAR_MAX_NODE_KEYS
        };
        if keys.len() > scan_limit {
            return PlanSlot::Hashed { tags };
        }

        // Sort by tag so a dispatch scans only the keys its tag can match.
        keys.sort_by_key(|key| nodes[key.start].tag());
        let mut ranges = [(0u16, 0u16); 8];
        for (tag, range) in ranges.iter_mut().enumerate() {
            let lo = keys.partition_point(|key| nodes[key.start].tag() < tag);
            let hi = keys.partition_point(|key| nodes[key.start].tag() <= tag);
            *range = (lo as u16, hi as u16);
        }
        let targets: Box<[Value]> = keys.iter().map(|key| key.target).collect();
        let plan_keys = if all_bits {
            PlanKeys::Bits(
                keys.iter()
                    .map(|key| match nodes[key.start] {
                        KeyNode::Bits(bits) => bits,
                        _ => unreachable!("all_bits"),
                    })
                    .collect(),
            )
        } else {
            let mut starts = Vec::with_capacity(keys.len() + 1);
            let mut sorted = Vec::with_capacity(nodes.len());
            for key in &keys {
                starts.push(sorted.len() as u32);
                sorted.extend_from_slice(&nodes[key.start..key.end]);
            }
            starts.push(sorted.len() as u32);
            PlanKeys::Nodes {
                starts: starts.into_boxed_slice(),
                nodes: sorted.into_boxed_slice(),
                strings: strings.into_boxed_slice(),
            }
        };
        PlanSlot::Plan {
            tags,
            body: Box::new(PlanBody {
                ranges,
                targets,
                keys: plan_keys,
            }),
        }
    }
}

/// One key while a plan is compiled: its program's span in the node buffer
/// and its target.
struct CompiledKey {
    start: usize,
    end: usize,
    target: Value,
}

/// A dense index over all-fixnum keys spanning at most [`DENSE_MAX_SPAN`].
fn dense_fixnums(keys: &[CompiledKey], nodes: &[KeyNode]) -> Option<PlanBody> {
    let fixnums: Vec<i64> = keys
        .iter()
        .map(|key| match nodes[key.start] {
            KeyNode::Bits(bits) => Value::from_bits(bits).as_fixnum(),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let base = *fixnums.iter().min()?;
    let max = *fixnums.iter().max()?;
    let span = max.abs_diff(base);
    if span > DENSE_MAX_SPAN {
        return None;
    }
    let mut slots = vec![NO_ENTRY; span as usize + 1];
    for (index, n) in fixnums.iter().enumerate() {
        slots[n.abs_diff(base) as usize] = index as u16;
    }
    Some(PlanBody {
        ranges: [(0, 0); 8],
        targets: keys.iter().map(|key| key.target).collect(),
        keys: PlanKeys::DenseFixnum {
            base,
            slots: slots.into_boxed_slice(),
        },
    })
}

/// Append `hash_key`'s program to `out`, or `None` for a key the plan cannot
/// reproduce exactly under `test` (the table then answers with the hashed
/// lookup) or whose program would grow `out` past `limit` nodes.
/// `car_depth` counts the conses whose CAR encloses this key.
fn flatten(
    hash_key: &HashKey,
    test: HashTableTest,
    out: &mut Vec<KeyNode>,
    strings: &mut Vec<StringKey>,
    car_depth: usize,
    limit: usize,
) -> Option<()> {
    match (hash_key, test) {
        (HashKey::Nil, _) => out.push(KeyNode::Bits(Value::NIL.bits())),
        (HashKey::True, _) => out.push(KeyNode::Bits(Value::T.bits())),
        (HashKey::Int(n), _) => {
            let v = Value::fixnum(*n);
            if v.as_fixnum() != Some(*n) {
                return None;
            }
            out.push(KeyNode::Bits(v.bits()));
        }
        (HashKey::Symbol(id), _) => {
            // nil and t key as `Nil`/`True`; a `Symbol` naming either (or
            // the unbound marker) is not a key any value materializes to.
            let v = Value::from_sym_id(*id);
            if !matches!(v.kind(), ValueKind::Symbol(_)) {
                return None;
            }
            out.push(KeyNode::Bits(v.bits()));
        }
        // Identity keys only under `eq`/`eql`, where `compile` has checked
        // that the slot's object keys this way. Under `equal` a `Ptr` may
        // name an object `equal` keys structurally (a table whose test
        // changed after insertion): bit identity would then hit where the
        // hashed lookup misses.
        (HashKey::Ptr(bits), HashTableTest::Eq | HashTableTest::Eql) => {
            out.push(KeyNode::Bits(*bits))
        }
        (HashKey::Float(bits), HashTableTest::Eql | HashTableTest::Equal) => {
            out.push(KeyNode::Float(*bits))
        }
        (HashKey::StringContent(content), HashTableTest::Equal) => {
            let index = u32::try_from(strings.len()).ok()?;
            strings.push(StringKey {
                schars: content.1,
                bytes: content.0.clone(),
            });
            out.push(KeyNode::String(index));
        }
        (HashKey::EqualCons(car, cdr), HashTableTest::Equal) => {
            if car_depth + 1 >= MAX_CAR_DEPTH {
                return None;
            }
            out.push(KeyNode::Cons);
            flatten(car, test, out, strings, car_depth + 1, limit)?;
            // A cdr takes its enclosing cons's place on the pending stack.
            flatten(cdr, test, out, strings, car_depth, limit)?;
        }
        // Bignum, EqualVec, Marker, Overlay, BoolVec, SymbolWithPos,
        // ByteCode, Cycle, Text, Keyword, Char, Window, Frame, FloatEq, and
        // an identity key under `equal`.
        _ => return None,
    }
    (out.len() <= limit).then_some(())
}

impl PlanBody {
    #[inline(always)]
    fn lookup_plain(&self, v: Value) -> Option<Value> {
        match &self.keys {
            PlanKeys::DenseFixnum { base, slots } => {
                // The tag prefilter admitted only fixnum tags.
                let index = ((v.bits() as i64) >> FIXNUM_SHIFT).wrapping_sub(*base) as u64 as usize;
                match slots.get(index) {
                    Some(&entry) if entry != NO_ENTRY => Some(self.targets[entry as usize]),
                    _ => None,
                }
            }
            PlanKeys::Bits(keys) => {
                let (lo, hi) = self.ranges[v.bits() & TAG_MASK];
                let (lo, hi) = (lo as usize, hi as usize);
                let bits = v.bits();
                keys[lo..hi]
                    .iter()
                    .position(|&key| key == bits)
                    .map(|i| self.targets[lo + i])
            }
            PlanKeys::Nodes { .. } => self.match_nodes::<false>(v),
        }
    }

    /// `symbols-with-pos-enabled`: `to_eq_key_swp`/`to_eql_key_swp` strip a
    /// positioned symbol at the top level only; `to_equal_key_depth_swp`
    /// strips at every level, which [`match_key`] does at each leaf.
    #[cold]
    #[inline(never)]
    fn lookup_swp(&self, tags: TagSet, v: Value) -> Option<Value> {
        let v = strip_symbol_position(v);
        if !tags.admits(v) {
            return None;
        }
        match &self.keys {
            PlanKeys::Nodes { .. } => self.match_nodes::<true>(v),
            _ => self.lookup_plain(v),
        }
    }

    /// Scan the key programs sharing `v`'s tag; keys are unique under the
    /// table's test, so the first match is the only one.
    #[inline(never)]
    fn match_nodes<const SWP: bool>(&self, v: Value) -> Option<Value> {
        let PlanKeys::Nodes {
            starts,
            nodes,
            strings,
        } = &self.keys
        else {
            unreachable!("match_nodes on a plan without key programs")
        };
        let (lo, hi) = self.ranges[v.bits() & TAG_MASK];
        (lo as usize..hi as usize)
            .find(|&k| {
                match_key::<SWP>(
                    &nodes[starts[k] as usize..starts[k + 1] as usize],
                    strings,
                    v,
                )
            })
            .map(|k| self.targets[k])
    }
}

/// A positioned symbol's bare symbol; any other value unchanged.
#[inline]
fn strip_symbol_position(v: Value) -> Value {
    if v.is_symbol_with_pos() {
        v.as_symbol_with_pos_sym().unwrap_or(v)
    } else {
        v
    }
}

/// Walk `v` in lockstep with one key program. The walk is bounded by the KEY
/// (at most [`MAX_KEY_NODES`] nodes, at most [`MAX_CAR_DEPTH`] pending
/// cdrs), never by `v`, so a cyclic or arbitrarily deep value terminates.
///
/// A match means `v` has the key's exact shape with no cycle inside it (two
/// distinct finite subtrees of the key would otherwise match one subtree of
/// `v`), so `to_equal_key_depth_swp(v)` -- which emits `Cycle` only for a
/// true ancestor -- materializes to precisely this key.
#[inline(always)]
fn match_key<const SWP: bool>(prog: &[KeyNode], strings: &[StringKey], mut v: Value) -> bool {
    let mut pending = [Value::NIL; MAX_CAR_DEPTH];
    let mut depth = 0usize;
    let mut pc = 0usize;
    loop {
        match prog[pc] {
            KeyNode::Cons => {
                if !v.is_cons() {
                    return false;
                }
                // `flatten` bounds the car nesting below MAX_CAR_DEPTH.
                pending[depth] = v.cons_cdr();
                depth += 1;
                v = v.cons_car();
                pc += 1;
                continue;
            }
            KeyNode::Bits(bits) => {
                let leaf = if SWP { strip_symbol_position(v) } else { v };
                if leaf.bits() != bits {
                    return false;
                }
            }
            KeyNode::Float(bits) => {
                if !(v.is_float() && v.xfloat().to_bits() == bits) {
                    return false;
                }
            }
            KeyNode::String(index) => {
                let key = &strings[index as usize];
                // `value_matches`: SCHARS and bytes; properties are ignored.
                if !(v.is_string()
                    && v.as_lisp_string().is_some_and(|string| {
                        string.schars() == key.schars && string.as_bytes() == &*key.bytes
                    }))
                {
                    return false;
                }
            }
        }
        pc += 1;
        if depth == 0 {
            return true;
        }
        depth -= 1;
        v = pending[depth];
    }
}

#[cfg(test)]
#[path = "switch_plan/tests/switch_plan_test.rs"]
mod tests;
