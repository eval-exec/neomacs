//! `SpecSlot` v2 (p1-0-integration §3.1, S0.8): the per-site speculation
//! state compiled code and the call shims share, and the one statement of
//! what each of its words means for each kind of site.
//!
//! A slot is four words, baked into generated code by address (JIT) or
//! indexed off the sidecar (AOT, stride `size_of::<SpecSlot>()`, salted into
//! the ABI tag):
//!
//! | word | offset | [`SpecSlotKind::Bytecode`] | [`SpecSlotKind::Subr`] |
//! |---|---|---|---|
//! | `epoch` | 0 | clock at the last validation ([`SPEC_EPOCH_DISARMED`] never matches) | same |
//! | `leaf` | 8 | `*const CompiledLeaf`, or 0 | the expected subr's bits (immutable) |
//! | `direct_consts` | 16 | the shim's key: constant base with `KEY_*` flags, or 0 | the site's `SymId` (immutable) |
//! | `direct_entry` | 24 | the register-ABI entry, [`DirectEntryTag::Framed`], or 0 | 0 |
//!
//! Only a [`SpecSlotKind::Bytecode`] slot's `leaf`, `direct_consts` and
//! `direct_entry` ever change after the slot is built, so only those are
//! ever cleared: every slot walker goes through
//! [`CompiledLeaf::bytecode_spec_slots`]. Clearing a subr slot's words would
//! make its re-validation fail forever (a silent permanent fall to the
//! reference path), so `clear_leaf` refuses one in debug builds.

use super::*;

/// A non-code direct-entry word: framed memory entries are entered through
/// the contained trampoline rather than called with the register ABI.
/// Threading: immutable tags; each slot retains its existing atomic,
/// publish-last/clear-first protocol and belongs to its caller's mutator.
/// A Release store publishes the tag after leaf/key initialization; framed
/// generated loads are atomic with Acquire or stronger ordering. The live
/// or retired cache retains the leaf for the entire native-call extent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub(crate) enum DirectEntryTag {
    /// Not a native code address. Generated code tests this before any
    /// register-ABI indirect call when the framed shape is compile-enabled.
    Framed = 1,
}

/// What a spec slot's words hold (see the module table), decided by its
/// site's [`SpecCalleeKind`] when the leaf is built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpecSlotKind {
    /// A byte-code callee: the cached leaf, the shim key and the direct
    /// entry, armed and cleared as the callee's compiled state changes.
    Bytecode,
    /// A builtin callee (every subr [`SpecCalleeKind`]): immutable binding
    /// words (a `SubrGeneral` site's symbol and expected subr; zero for the
    /// other subr kinds, whose shims read neither).
    Subr,
    /// A closure source site (P2.1 C5, `compile::source_slots`):
    /// `direct_consts` the source's identity word (immutable); `leaf`,
    /// `direct_entry` and `epoch` a direct entry into the source's leaf
    /// (`NEOVM_JIT_DIRECT_CALL`), cleared by the leaf walk
    /// ([`CompiledLeaf::source_spec_slots`]).
    Source,
}

impl SpecCalleeKind {
    /// The kind of slot a site of this callee kind owns.
    pub(crate) fn slot_kind(self) -> SpecSlotKind {
        match self {
            SpecCalleeKind::Bytecode => SpecSlotKind::Bytecode,
            SpecCalleeKind::SubrGeneral
            | SpecCalleeKind::PredRecordp
            | SpecCalleeKind::PredSymbolWithPos
            | SpecCalleeKind::PredTypeOf
            | SpecCalleeKind::PredClTypeOf
            | SpecCalleeKind::PredFboundp
            | SpecCalleeKind::PredAutoloadDoLoad
            | SpecCalleeKind::EqInclProps
            | SpecCalleeKind::ArithIntrinsic { .. }
            // CallBuiltinSym sites own no slot; their kind never reaches a
            // slot, and would be a builtin's if it did.
            | SpecCalleeKind::CbsymTierA { .. }
            | SpecCalleeKind::CbsymTierB => SpecSlotKind::Subr,
            SpecCalleeKind::Source | SpecCalleeKind::Constant => SpecSlotKind::Source,
        }
    }
}

/// Per-site speculation state (see the module docs for each word).
///
/// `epoch` is the obarray `function_epoch` at which this site's callee
/// binding was last validated. `leaf` lazily caches a `*const CompiledLeaf`
/// (as `usize` bits; 0 = none) for the armed callee, so repeat calls skip
/// the compiled-cache hash lookup (the V3 fast path). The leaf pointer is
/// cleared whenever revalidation fails (the binding changed), and is sound
/// while set because the tagged-heap identity is stable during native
/// execution (so `cache::clear()` cannot fire mid-call to free the leaf —
/// see `resolve_compiled_leaf_ptr`; NOT "the cache never evicts", audit #1).
/// `repr(C)` pins the field order the baked pointer arithmetic relies on.
///
/// `direct_consts` is the shim's own fast-path key: the cached leaf's
/// constant base when that leaf takes this site's call -- as the generated
/// code laid it out, or normalized into a frame buffer of at most
/// [`FAST_PATH_MAX_ARITY`] words (missing `&optional` slots nil-filled, a
/// `&rest` tail consed into the last slot) -- 0 otherwise. With it set the
/// shim enters the leaf without re-classifying the callee, the leaf or the
/// call; it is armed together with `leaf` and cleared with it.
///
/// `direct_entry` is the key of a site that calls the leaf itself (a direct
/// call, `NEOVM_JIT_DIRECT_CALL`): the leaf's register-ABI entry when the
/// site may enter it with its arguments in registers and `direct_consts`'s
/// constant base as its `aux` word, or [`DirectEntryTag::Framed`] for an
/// exact-arity framed JIT leaf under `NEOVM_JIT_DIRECT_SHAPES=framed`, else
/// 0. The framed trampoline retains the leaf's memory ABI. Armed LAST and cleared FIRST, so
/// a site that sees it set sees the leaf and key it goes with. Publication
/// uses Release, paired with an atomic Acquire-or-stronger generated load
/// when framed sites are enabled; existing off-mode loads are unchanged.
#[repr(C)]
pub(crate) struct SpecSlot {
    pub(super) epoch: AtomicU64,
    pub(super) leaf: AtomicU64,
    pub(super) direct_consts: AtomicU64,
    pub(super) direct_entry: AtomicU64,
}

/// Byte offsets of the four words, for generated code.
pub(crate) const SPEC_SLOT_EPOCH_OFFSET: usize = core::mem::offset_of!(SpecSlot, epoch);
pub(crate) const SPEC_SLOT_LEAF_OFFSET: usize = core::mem::offset_of!(SpecSlot, leaf);
pub(crate) const SPEC_SLOT_KEY_OFFSET: usize = core::mem::offset_of!(SpecSlot, direct_consts);
pub(crate) const SPEC_SLOT_DIRECT_ENTRY_OFFSET: usize =
    core::mem::offset_of!(SpecSlot, direct_entry);

const _: () = {
    assert!(core::mem::size_of::<SpecSlot>() == 32);
    assert!(SPEC_SLOT_EPOCH_OFFSET == 0);
    assert!(SPEC_SLOT_LEAF_OFFSET == 8);
    assert!(SPEC_SLOT_KEY_OFFSET == 16);
    assert!(SPEC_SLOT_DIRECT_ENTRY_OFFSET == 24);
};

/// Largest callee arity the spec shim's fast path frames itself (a short
/// call to a callee with `&optional` slots is nil-filled, and a `&rest`
/// callee's tail consed, into a buffer of this many words on the shim's
/// stack); wider callees keep the slow half.
pub(crate) const FAST_PATH_MAX_ARITY: usize = 16;

impl SpecSlot {
    /// A slot armed at `epoch` with no cached leaf.
    pub(crate) const fn at_epoch(epoch: u64) -> Self {
        Self {
            epoch: AtomicU64::new(epoch),
            leaf: AtomicU64::new(0),
            direct_consts: AtomicU64::new(0),
            direct_entry: AtomicU64::new(0),
        }
    }

    /// The slot a site of `kind` starts with, armed at `epoch`: a
    /// `SubrGeneral` site's carries its immutable binding words
    /// ([`Self::bind_subr`]), every other starts empty.
    pub(crate) fn for_site(kind: SpecCalleeKind, epoch: u64, sym: u32, expected: u64) -> Self {
        let slot = Self::at_epoch(epoch);
        if kind == SpecCalleeKind::SubrGeneral {
            slot.bind_subr(sym, expected);
        }
        slot
    }

    /// Arm a slot built at an older epoch for `epoch`, when the caller has
    /// proved the site's binding unchanged at `epoch` (a background compile's
    /// install, `jit::bg`: the only epoch write after a leaf's front). A
    /// loader-disarmed slot never re-arms. Whether the slot moved.
    pub(crate) fn restamp(&self, epoch: u64) -> bool {
        let armed = self.epoch.load(Ordering::Relaxed);
        if armed == epoch || armed == super::SPEC_EPOCH_DISARMED {
            return false;
        }
        self.epoch.store(epoch, Ordering::Relaxed);
        true
    }

    /// The cached callee leaf, null when none.
    #[inline(always)]
    pub(crate) fn leaf_ptr(&self) -> *const CompiledLeaf {
        self.leaf.load(Ordering::Relaxed) as *const CompiledLeaf
    }

    /// The armed direct entry, null when the site may not call directly
    /// (generated code reads the word itself; tests read it here).
    #[cfg(test)]
    pub(crate) fn direct_entry(&self) -> *const u8 {
        self.direct_entry.load(Ordering::Acquire) as usize as *const u8
    }

    /// Cache `leaf` for the armed callee; `direct_consts` is the callee's
    /// constant base when the leaf takes the site's call (the shim's
    /// fast-path key), null otherwise, with [`Self::KEY_SHORT_CALL`] set
    /// when the fast path builds the leaf's frame (a short call, nil-filled,
    /// or a `&rest` leaf, its tail consed), [`Self::KEY_FRAMED`] when the
    /// leaf runs under its own native frame and [`Self::KEY_REGISTER`] when
    /// its raw entry has the register ABI. The base is 8-byte aligned, so
    /// the low bits are free; folding the facts into the word the fast path
    /// loads anyway keeps its pure, handler-free, memory-ABI case at one
    /// test each instead of the leaf's arity load, its three eligibility
    /// loads and its ABI. Any direct entry the slot held goes first.
    #[inline(always)]
    pub(crate) fn arm_leaf(
        &self,
        leaf: *const CompiledLeaf,
        direct_consts: *const Value,
        short_call: bool,
        framed: bool,
        register: bool,
    ) {
        debug_assert!(!self.holds_subr_binding(), "arm_leaf on a subr site's slot");
        // The runtime arms a slot only after a clear (the entry is 0), but
        // a direct entry must never outlive the leaf it was armed for, so a
        // re-arm over a live leaf drops it first.
        self.direct_entry.store(0, Ordering::Relaxed);
        self.leaf.store(leaf as usize as u64, Ordering::Relaxed);
        let key = if direct_consts.is_null() {
            0
        } else {
            debug_assert_eq!(direct_consts as usize & Self::KEY_FLAGS as usize, 0);
            direct_consts as usize as u64
                | if short_call { Self::KEY_SHORT_CALL } else { 0 }
                | if framed { Self::KEY_FRAMED } else { 0 }
                | if register { Self::KEY_REGISTER } else { 0 }
        };
        self.direct_consts.store(key, Ordering::Relaxed);
    }

    /// Arm the register entry or framed tag of the leaf [`Self::arm_leaf`]
    /// just cached: published with Release last, after the leaf/key/epoch.
    #[inline]
    pub(crate) fn arm_direct_entry(&self, entry: *const u8) {
        debug_assert!(
            !self.holds_subr_binding(),
            "a subr site never calls directly"
        );
        debug_assert!(!self.leaf_ptr().is_null(), "a direct entry needs its leaf");
        let flags = self.direct_consts.load(Ordering::Relaxed) & Self::KEY_FLAGS;
        debug_assert!(
            if entry as usize as u64 == DirectEntryTag::Framed as u64 {
                flags == Self::KEY_FRAMED
            } else {
                flags & !Self::KEY_SHORT_CALL == Self::KEY_REGISTER
            },
            "the direct entry's ABI must match its slot key"
        );
        self.direct_entry
            .store(entry as usize as u64, Ordering::Release);
    }

    /// `direct_consts` flag: the site's call is not the leaf's frame as laid
    /// out -- short of its arity (nil-filled `&optional` slots) or to a
    /// `&rest` leaf (the tail consed into its last slot) -- so the fast path
    /// builds the frame in a buffer of [`FAST_PATH_MAX_ARITY`] words.
    pub(crate) const KEY_SHORT_CALL: u64 = 1;
    /// `direct_consts` flag: the leaf runs under its own native frame.
    pub(crate) const KEY_FRAMED: u64 = 2;
    /// `direct_consts` flag: the leaf is frameless with the register ABI
    /// (`EntryShape::RawRegister`), so its raw entry is
    /// `CompiledLeaf::entry_call_raw_register`. Never set with
    /// [`Self::KEY_FRAMED`]: a framed body keeps the memory ABI.
    pub(crate) const KEY_REGISTER: u64 = 4;
    /// The flag bits of the key; the rest is the constant base.
    pub(crate) const KEY_FLAGS: u64 = 7;

    /// Drop the cached leaf, and with it the direct entry and the fast-path
    /// key: the direct entry FIRST, so no site enters a leaf whose key is
    /// already gone (the arming order, reversed).
    #[inline(always)]
    pub(crate) fn clear_leaf(&self) {
        debug_assert!(
            !self.holds_subr_binding(),
            "clear_leaf on a subr site's slot would erase its binding words"
        );
        self.direct_entry.store(0, Ordering::Relaxed);
        self.direct_consts.store(0, Ordering::Relaxed);
        self.leaf.store(0, Ordering::Relaxed);
    }

    /// Give a `SubrGeneral` site's slot its IMMUTABLE binding words, written
    /// once when the slot is built: `leaf` holds the expected subr's bits and
    /// `direct_consts` the site's symbol (p1-0-integration §3.1). A leaf
    /// builtin's Bcall trampoline then gets the symbol, the expected binding
    /// and the armed epoch from the one slot pointer, keeping the argument
    /// registers for the call's own arguments. The subr shims never read
    /// these two words; nothing may clear them (`clear_leaf` asserts): a
    /// cleared slot would make every re-validation fail -- a permanent,
    /// silent fall to the reference path.
    pub(crate) fn bind_subr(&self, sym: u32, expected: u64) {
        debug_assert!(
            Value::from_bits(expected as usize).is_veclike(),
            "a subr binding is a tagged subr object"
        );
        self.leaf.store(expected, Ordering::Relaxed);
        self.direct_consts.store(u64::from(sym), Ordering::Relaxed);
    }

    /// Whether this slot carries a subr site's binding words
    /// ([`Self::bind_subr`]): the in-band witness of [`SpecSlotKind::Subr`]
    /// for a bound site, which `arm_leaf`/`clear_leaf` assert against. A
    /// bytecode site's `leaf` is null or an aligned `CompiledLeaf` pointer
    /// (tag bits 0), a subr binding is a tagged veclike. Walkers decide by
    /// the leaf's [`SpecSlotKind`]s, never by this.
    #[inline(always)]
    pub(crate) fn holds_subr_binding(&self) -> bool {
        self.leaf.load(Ordering::Relaxed) & TAG_MASK as u64 != 0
    }

    /// A subr site's `(symbol, expected subr bits)` ([`Self::bind_subr`]).
    #[inline(always)]
    pub(crate) fn subr_binding(&self) -> (SymId, u64) {
        (
            SymId(self.direct_consts.load(Ordering::Relaxed) as u32),
            self.leaf.load(Ordering::Relaxed),
        )
    }
}

/// Direct entries armed, process-wide (the `direct-call:` census entry;
/// engagement evidence). Counted once per arming, off every hot path.
pub(crate) static DIRECT_ENTRIES_ARMED: AtomicU64 = AtomicU64::new(0);

/// Arm `slot`'s direct entry for `leaf`, which [`SpecSlot::arm_leaf`] just
/// cached for a call of `nargs` arguments, when a compiled site may enter
/// the leaf itself (S2.1b, `NEOVM_JIT_DIRECT_CALL`): the leaf has the
/// register ABI for exactly `nargs` words (no `&optional` padding, no
/// `&rest` list to build), runs frameless (no bindings, no handler frames,
/// no AOT sidecar: what the shim enters raw), the key the shim uses is the
/// constant base with no flag but [`SpecSlot::KEY_REGISTER`] (the site
/// passes the base as `aux`), and the lean backtrace frame the site pushes
/// has a probed layout. Anything else leaves the entry 0 and the site on
/// the shim. Under the framed shape knob, an exact required-only JIT memory
/// leaf with bindings or handlers publishes the framed trampoline tag
/// instead. AOT sidecars remain excluded. Once per arming, so cold.
#[cold]
#[inline(never)]
pub(crate) fn arm_direct_entry_if_eligible(slot: &SpecSlot, leaf: &CompiledLeaf, nargs: usize) {
    let key = slot.direct_consts.load(Ordering::Relaxed);
    let eligible = leaf.abi
        == (LeafAbi::Register {
            arity: nargs.min(u8::MAX as usize) as u8,
        })
        && leaf.arity == nargs
        && !leaf.has_rest
        && leaf.direct_call_eligible()
        && key != 0
        && key & SpecSlot::KEY_FLAGS == SpecSlot::KEY_REGISTER
        && super::jit_layout::backtrace_layout().is_some();
    if eligible {
        slot.arm_direct_entry(leaf.entry);
        DIRECT_ENTRIES_ARMED.fetch_add(1, Ordering::Relaxed);
    } else if super::knobs::jit_direct_shapes().framed
        && framed_direct_eligible(leaf, nargs)
        && key != 0
        && key & SpecSlot::KEY_FLAGS == SpecSlot::KEY_FRAMED
        && super::jit_layout::backtrace_layout().is_some()
    {
        slot.arm_direct_entry(DirectEntryTag::Framed as u64 as usize as *const u8);
        DIRECT_ENTRIES_ARMED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Initial framed reach is exact required-only calls of JIT memory leaves;
/// AOT sidecars and argument normalization remain on the reference path.
/// Threading: reads immutable facts of a live, mutator-owned cache leaf.
#[inline]
pub(crate) fn framed_direct_eligible(leaf: &CompiledLeaf, nargs: usize) -> bool {
    leaf.abi == LeafAbi::Memory
        && leaf.entry_shape == super::leaf::EntryShape::Framed
        && leaf.sidecar.is_none()
        && (leaf.has_binds || leaf.has_handlers)
        && leaf.required == leaf.arity
        && !leaf.has_rest
        && leaf.arity == nargs
        && nargs <= super::direct_call::MAX_REST_CALL_ARGS
}

/// Arm `slot`'s direct entry for the leaf the spec shim cached in it, for a
/// direct site whose call of `nargs` arguments is not the leaf's frame as
/// laid out (`NEOVM_JIT_DIRECT_SHAPES`): the site passes its arguments as
/// the register words of `shape` -- nil for each `&optional` slot it lacks,
/// a fresh list for a `&rest` one -- so the leaf must take exactly those:
/// the register ABI for `shape`'s words, that lambda list, a call of
/// `nargs` it accepts, frameless, and the key the shim armed for it (the
/// site masks the flags off for `aux`). The site's shape, not the shim's,
/// is what makes this check possible: an object at the expected bits may
/// not be the one the site was compiled against (a collected definition's
/// slot reused), so the leaf is matched against the shape the site's code
/// passes. Once per arming, so cold.
#[cold]
#[inline(never)]
pub(crate) fn arm_shaped_direct_entry(
    slot: &SpecSlot,
    shape: super::direct_call::CalleeShape,
    nargs: usize,
) {
    let ptr = slot.leaf_ptr();
    if ptr.is_null() {
        return;
    }
    // SAFETY: a slot's leaf names a live or retired cache leaf, which stays
    // allocated (`resolve_compiled_leaf_ptr`).
    let leaf = unsafe { &*ptr };
    let key = slot.direct_consts.load(Ordering::Relaxed);
    let eligible = leaf.abi
        == (LeafAbi::Register {
            arity: shape.arity().min(u8::MAX as usize) as u8,
        })
        && leaf.arity == shape.arity()
        && leaf.has_rest == shape.rest
        && leaf.accepts(nargs)
        && leaf.direct_call_eligible()
        && key != 0
        && key & SpecSlot::KEY_FRAMED == 0
        && key & SpecSlot::KEY_REGISTER != 0
        && super::jit_layout::backtrace_layout().is_some();
    if eligible {
        slot.arm_direct_entry(leaf.entry);
        DIRECT_ENTRIES_ARMED.fetch_add(1, Ordering::Relaxed);
    }
}

/// The per-slot kinds of a leaf's spec slots (`CompiledLeaf::spec_slot_kinds`),
/// from the site map whose `slot` fields number `0..n` densely.
pub(crate) fn spec_slot_kinds_of(
    sites: &HashMap<usize, SpecSite>,
    n: usize,
) -> Box<[SpecSlotKind]> {
    let mut kinds: Vec<Option<SpecSlotKind>> = vec![None; n];
    for site in sites.values() {
        debug_assert!(kinds[site.slot].is_none(), "one site per slot");
        kinds[site.slot] = Some(site.kind.slot_kind());
    }
    kinds
        .into_iter()
        .map(|k| k.expect("spec slots are numbered densely"))
        .collect()
}

impl CompiledLeaf {
    /// This leaf's [`SpecSlotKind::Source`] slots (closure source sites,
    /// whose `leaf` word may hold a direct entry's leaf).
    pub(crate) fn source_spec_slots(&self) -> impl Iterator<Item = &SpecSlot> {
        self.spec_slots
            .iter()
            .zip(self.spec_slot_kinds.iter())
            .filter(|(_, kind)| **kind == SpecSlotKind::Source)
            .map(|(slot, _)| slot)
    }

    /// This leaf's [`SpecSlotKind::Bytecode`] slots: the only ones whose
    /// words name a callee leaf, and so the only ones a walker (retiring a
    /// leaf, a redefinition firing) may clear.
    pub(crate) fn bytecode_spec_slots(&self) -> impl Iterator<Item = &SpecSlot> {
        debug_assert_eq!(self.spec_slots.len(), self.spec_slot_kinds.len());
        self.spec_slots
            .iter()
            .zip(self.spec_slot_kinds.iter())
            .filter(|(_, kind)| **kind == SpecSlotKind::Bytecode)
            .map(|(slot, _)| slot)
    }
}

#[cfg(test)]
#[path = "spec_slot/tests/spec_slot_test.rs"]
mod tests;
