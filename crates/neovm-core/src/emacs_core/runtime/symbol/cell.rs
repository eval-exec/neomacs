//! A symbol's value cell: the `redirect` tag and the word it selects, held
//! behind one private representation.
//!
//! GNU spells the cell as the `redirect` bit-field of `struct Lisp_Symbol`
//! plus its `val` union (`src/lisp.h:786-829`), and every reader is expected
//! to switch on the tag before it touches the union. Here both halves are
//! private to this module, so that discipline is a property of the types:
//!
//! - [`LispSymbol::value_cell`] reads the tag and then the word the tag
//!   selects, as one [`ValueCell`]. No reader can take the word without the
//!   tag, so none can read the wrong arm.
//! - [`CellWrite`] is the only writer of either half, and
//!   [`CellWrite::publish`] the one place a word or tag is stored. The moves
//!   it offers are typed by the arm the cell is in ([`ArmMut`]): a plain cell
//!   can be stored, aliased, localized or forwarded; an alias re-aliased or
//!   undone; a forwarded cell re-forwarded or localized; a localized cell
//!   never changes arm. Those are the transitions GNU makes
//!   (`Fdefvaralias`, `make_blv`, `defvar_*`, `Fmakunbound`).
//! - A `Forwarded` word is only ever a [`LispFwd`] header, which cannot be
//!   forged (see its invariant), and a `Localized` word only a [`BlvPtr`],
//!   which only `make_symbol_localized` and the obarray's deep copy create.
//!
//! # Threading
//!
//! One mutator writes a cell, through `&mut`; the concurrent GC marker reads
//! it from its own thread. Every store of the word is a Release store and
//! every store of the tag an atomic byte store, made by `publish`, and while a
//! mark runs a [`CellWrite`] brackets them with the chunk's seqlock, which
//! the marker's [`read_symbol_children`] honours. The mutator's own reads
//! ([`LispSymbol::value_cell`]) are plain loads: it is the only writer.
//! [`LispSymbol::value_cell_acquire`] is the reader for code that does not
//! own the writer side. Nothing here makes a symbol shareable between
//! mutators, and no `Send`/`Sync` is claimed for any type in this module.

use super::{
    LispBufferLocalValue, SYMBOL_NAME_SENTINEL, SymbolInterned, SymbolRedirect, SymbolTrappedWrite,
};
use crate::emacs_core::forward::LispFwd;
use crate::emacs_core::intern::{NameId, SymId, symbol_name_id};
use crate::emacs_core::value::Value;
use crate::tagged::header::load_value_atomic;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicUsize, Ordering};

// ===========================================================================
// The flags byte
// ===========================================================================

/// Packed flags byte for a [`LispSymbol`]. Mirrors the bit-packed first byte
/// of GNU `Lisp_Symbol::s` (`src/lisp.h:786-792`).
///
/// Bit layout:
/// ```text
///   bits 0..2 : SymbolRedirect
///   bits 2..4 : SymbolTrappedWrite
///   bits 4..6 : SymbolInterned
///   bit  6    : declared_special
///   bit  7    : runtime_projected (this port's own; see below)
/// ```
///
/// The redirect bits can only be changed from this module, together with the
/// word they select ([`CellWrite::publish`]); a symbol's flags byte is not
/// reachable mutably from outside it at all.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default)]
pub struct SymbolFlags(u8);

/// `SymbolFlags::is_plain_untrapped_unprojected` reads the redirect and
/// trapped-write fields as raw zero bits; that is only the (Plainval,
/// Untrapped) pair while both encodings are GNU's zero.
const _: () =
    assert!(SymbolRedirect::Plainval as u8 == 0 && SymbolTrappedWrite::Untrapped as u8 == 0);

impl SymbolFlags {
    pub(super) const REDIRECT_MASK: u8 = 0b0000_0011;
    const TRAPPED_WRITE_SHIFT: u8 = 2;
    pub(super) const TRAPPED_WRITE_MASK: u8 = 0b0000_1100;
    const INTERNED_SHIFT: u8 = 4;
    const INTERNED_MASK: u8 = 0b0011_0000;
    const DECLARED_SPECIAL_BIT: u8 = 0b0100_0000;
    /// Not a GNU field: the value cell of this symbol is mirrored by a host
    /// projection (`Context`'s cached `quit-flag`, `inhibit-quit`, … or
    /// `buffer-undo-list`'s shared undo state), so a write must go through
    /// the `Context` path that republishes it.  The bind/unbind fast tiers
    /// refuse such a symbol on this one bit.
    pub(super) const RUNTIME_PROJECTED_BIT: u8 = 0b1000_0000;

    #[inline(always)]
    pub fn redirect(self) -> SymbolRedirect {
        SymbolRedirect::try_from(self.0 & Self::REDIRECT_MASK)
            .expect("symbol redirect flag contains valid GNU symbol_redirect code")
    }

    /// Private: the redirect moves only with its word ([`CellWrite`]).
    #[inline]
    fn set_redirect(&mut self, r: SymbolRedirect) {
        self.store_byte((self.0 & !Self::REDIRECT_MASK) | r.gnu_code());
    }

    #[inline]
    pub fn trapped_write(self) -> SymbolTrappedWrite {
        let raw = (self.0 & Self::TRAPPED_WRITE_MASK) >> Self::TRAPPED_WRITE_SHIFT;
        SymbolTrappedWrite::try_from(raw)
            .expect("symbol trapped-write flag contains valid GNU symbol_trapped_write code")
    }

    #[inline]
    fn set_trapped_write(&mut self, t: SymbolTrappedWrite) {
        self.store_byte(
            (self.0 & !Self::TRAPPED_WRITE_MASK) | (t.gnu_code() << Self::TRAPPED_WRITE_SHIFT),
        );
    }

    #[inline]
    pub fn interned(self) -> SymbolInterned {
        let raw = (self.0 & Self::INTERNED_MASK) >> Self::INTERNED_SHIFT;
        SymbolInterned::try_from(raw)
            .expect("symbol interned flag contains valid GNU symbol_interned code")
    }

    #[inline]
    fn set_interned(&mut self, i: SymbolInterned) {
        self.store_byte((self.0 & !Self::INTERNED_MASK) | (i.gnu_code() << Self::INTERNED_SHIFT));
    }

    #[inline]
    pub fn runtime_projected(self) -> bool {
        self.0 & Self::RUNTIME_PROJECTED_BIT != 0
    }

    #[inline]
    fn set_runtime_projected(&mut self, v: bool) {
        let byte = if v {
            self.0 | Self::RUNTIME_PROJECTED_BIT
        } else {
            self.0 & !Self::RUNTIME_PROJECTED_BIT
        };
        self.store_byte(byte);
    }

    /// The shape GNU's `do_one_unbind` restores with a bare `SET_SYMBOL_VAL`
    /// and `do_specbind` binds without `set_internal`: a plain value cell
    /// with no watcher and no constant refusal — plus, here, no host
    /// projection.  One byte test, since `Plainval` and `Untrapped` are both
    /// the zero encoding.
    #[inline(always)]
    pub fn is_plain_untrapped_unprojected(self) -> bool {
        self.0 & (Self::REDIRECT_MASK | Self::TRAPPED_WRITE_MASK | Self::RUNTIME_PROJECTED_BIT) == 0
    }

    #[inline]
    pub fn declared_special(self) -> bool {
        self.0 & Self::DECLARED_SPECIAL_BIT != 0
    }

    #[inline]
    fn set_declared_special(&mut self, v: bool) {
        let byte = if v {
            self.0 | Self::DECLARED_SPECIAL_BIT
        } else {
            self.0 & !Self::DECLARED_SPECIAL_BIT
        };
        self.store_byte(byte);
    }

    /// The raw byte, for a caller that packs it into a wider word (the
    /// symbol's write window).
    #[inline(always)]
    pub(super) fn bits(self) -> u8 {
        self.0
    }

    /// Atomic (relaxed) store of the whole flags byte so a concurrent GC reader
    /// (`load_redirect`) never observes a torn byte. Mirrors `ConsCell::set_car`:
    /// the field stays a plain `u8`, accessed atomically via a raw cast. There is
    /// a single mutator, so the caller's plain read of `self.0` to compute `byte`
    /// does not race (the GC thread only ever reads this byte).
    #[inline]
    fn store_byte(&mut self, byte: u8) {
        let p = std::ptr::from_mut(&mut self.0).cast::<AtomicU8>();
        // SAFETY: `AtomicU8` has the size and alignment of `u8`, and `&mut
        // self` makes this the only access from this thread.
        unsafe { (*p).store(byte, Ordering::Relaxed) };
    }

    /// Atomic (relaxed) read of the redirect tag, for the concurrent GC obarray
    /// scan. Pairs with the `store_byte` writes above so the scan never reads a
    /// torn flags byte while the mutator changes a redirect/flag bit.
    #[inline]
    fn load_redirect(&self) -> SymbolRedirect {
        let p = std::ptr::from_ref(&self.0).cast::<AtomicU8>();
        // SAFETY: as in `store_byte`; a shared reference permits an atomic
        // load.
        let byte = unsafe { (*p).load(Ordering::Relaxed) };
        SymbolRedirect::try_from(byte & Self::REDIRECT_MASK)
            .expect("symbol redirect flag contains valid GNU symbol_redirect code")
    }
}

// ===========================================================================
// The value cell
// ===========================================================================

/// A `Localized` cell's buffer-local record (GNU `struct
/// Lisp_Buffer_Local_Value *`): a box `make_symbol_localized` allocated and
/// leaked into the obarray's BLV pool, which frees it when the obarray drops.
///
/// Only [`BlvPtr::leak`] makes one, so a cell's `Localized` word is always a
/// record that pool owns. Dereferencing it is still the caller's `unsafe`:
/// the handle is a copyable address, and only the obarray that owns the
/// record can say it is alive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BlvPtr(NonNull<LispBufferLocalValue>);

impl BlvPtr {
    /// Leak RECORD for the obarray's BLV pool, which must free it with
    /// [`Self::free`].
    pub(super) fn leak(record: Box<LispBufferLocalValue>) -> Self {
        Self(NonNull::from(Box::leak(record)))
    }

    /// The record's address (for the GC root walk and compiled code).
    #[inline(always)]
    pub(crate) fn as_ptr(self) -> *mut LispBufferLocalValue {
        self.0.as_ptr()
    }

    /// Free a record [`Self::leak`] made.
    ///
    /// # Safety
    ///
    /// Called once per record, by the pool that owns it, when no symbol cell
    /// can be read again (the obarray is dropping).
    pub(super) unsafe fn free(self) {
        // SAFETY: the caller's contract; `leak` produced the box.
        drop(unsafe { Box::from_raw(self.0.as_ptr()) });
    }
}

/// A symbol's value cell, read: the redirect tag together with the payload
/// it selects (GNU `SYMBOL_VAL` / `SYMBOL_ALIAS` / `SYMBOL_BLV` /
/// `SYMBOL_FWD`). [`LispSymbol::value_cell`] is the only producer.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ValueCell {
    /// GNU `SYMBOL_PLAINVAL`: the value itself, [`Value::UNBOUND`] when void.
    Plain(Value),
    /// GNU `SYMBOL_VARALIAS`: the symbol whose cell this one is.
    Alias(SymId),
    /// GNU `SYMBOL_LOCALIZED`: the buffer-local record.
    Localized(BlvPtr),
    /// GNU `SYMBOL_FORWARDED`: the descriptor whose slot holds the value.
    Forwarded(&'static LispFwd),
}

/// The one machine word behind a cell; what it means is the tag's business,
/// so only this module encodes and decodes it. Always fully initialised
/// (an alias id is zero-extended), so a raw read of it never sees
/// uninitialised bytes.
#[repr(transparent)]
#[derive(Clone, Copy)]
struct CellWord(usize);

impl CellWord {
    #[inline(always)]
    fn plain(value: Value) -> Self {
        Self(value.bits())
    }

    #[inline(always)]
    fn alias(target: SymId) -> Self {
        Self(target.0 as usize)
    }

    #[inline(always)]
    fn localized(blv: BlvPtr) -> Self {
        Self(blv.as_ptr() as usize)
    }

    #[inline(always)]
    fn forwarded(fwd: &'static LispFwd) -> Self {
        Self(std::ptr::from_ref(fwd) as usize)
    }

    /// Decode under the tag the word was stored with.
    #[inline(always)]
    fn decode(self, redirect: SymbolRedirect) -> ValueCell {
        match redirect {
            SymbolRedirect::Plainval => ValueCell::Plain(Value::from_bits(self.0)),
            // The word was zero-extended from a `u32` by `alias`.
            SymbolRedirect::Varalias => ValueCell::Alias(SymId(self.0 as u32)),
            SymbolRedirect::Localized => {
                // SAFETY: only `CellWord::localized` stores under this tag,
                // from a `BlvPtr`, which is never null.
                ValueCell::Localized(BlvPtr(unsafe {
                    NonNull::new_unchecked(self.0 as *mut LispBufferLocalValue)
                }))
            }
            // SAFETY: only `CellWord::forwarded` stores under this tag, from
            // a `&'static LispFwd`.
            SymbolRedirect::Forwarded => {
                ValueCell::Forwarded(unsafe { &*(self.0 as *const LispFwd) })
            }
        }
    }
}

/// Per-symbol metadata stored in the obarray. Mirrors GNU `struct
/// Lisp_Symbol` at `src/lisp.h:786-829`.
///
/// The value cell (`flags`' redirect bits and `val`) is private to this
/// module; see the module documentation for what that buys. The other
/// fields belong to the obarray module (`function`, `plist` and the
/// membership flags have no tag to stay consistent with).
pub struct LispSymbol {
    /// The symbol's name, held as the raw [`NameId`] `u32` in an atomic cell.
    /// This field is BOTH the real name AND the obarray slot's presence
    /// discriminant ([`SYMBOL_NAME_SENTINEL`] == empty), so that the concurrent
    /// GC obarray scan (`ObarrayScanSnapshot::scan`, the only cross-thread
    /// reader) can gate on presence with an `Acquire` load that pairs with the
    /// slot fill's terminal `Release` store (see [`LispSymbol::publish_fill`]):
    /// observing a non-sentinel name happens-after every arm write. Write-once
    /// (name is never reset to the sentinel on a live slot — presence is
    /// monotonic), so the single mutator reads it `Relaxed` ([`LispSymbol::name`]).
    pub(super) name: AtomicU32,
    /// Packed flags: redirect tag, trapped-write tag, interned tag,
    /// declared-special bit. Mirrors the first byte of GNU
    /// `Lisp_Symbol::s` (`lisp.h:786-792`).
    flags: SymbolFlags,
    /// The value cell's word, read under `flags`' redirect.
    val: CellWord,
    /// Function slot. `Value::NIL` is the unbound sentinel (GNU `Qnil` in
    /// `struct Lisp_Symbol::s.function`, `lisp.h:820`).
    pub function: Value,
    /// Property list as a Lisp cons list (NIL = empty). Matches GNU
    /// `struct Lisp_Symbol::s.plist` (`lisp.h:820`).
    pub plist: Value,
    /// Whether this symbol is interned in the global obarray.
    pub(super) interned_global: bool,
    /// Whether `fmakunbound` explicitly masked the symbol's fallback function.
    pub(super) function_unbound: bool,
}

/// Size of one obarray slot.
pub(crate) const LISP_SYMBOL_SIZE: usize = std::mem::size_of::<LispSymbol>();
/// Byte offset of a symbol's packed flags; its low bits are the redirect.
pub(crate) const LISP_SYMBOL_FLAGS_OFFSET: usize = std::mem::offset_of!(LispSymbol, flags);
/// Byte offset of a symbol's value-cell word (the value itself for
/// `Plainval`).
pub(crate) const LISP_SYMBOL_VAL_OFFSET: usize = std::mem::offset_of!(LispSymbol, val);
/// Byte offset of a symbol's `interned_global` flag: the byte right after the
/// flags byte (const-asserted in the obarray module), so compiled code and the
/// cached variable tiers read `flags | interned_global << 8` as one 16-bit
/// window.
pub(crate) const LISP_SYMBOL_INTERNED_GLOBAL_OFFSET: usize =
    std::mem::offset_of!(LispSymbol, interned_global);

// The slot layout compiled code and the AOT cache bake (`jit_layout`), pinned
// to its numbers: moving any of these is an ABI change that needs the lead's
// ABI number, so it must fail the build rather than slip through as a
// different `offset_of!`. The word is a full aligned machine word, so it can
// become an atomic word in place.
const _: () = {
    use std::mem::{align_of, offset_of, size_of};
    assert!(LISP_SYMBOL_SIZE == 32);
    assert!(LISP_SYMBOL_VAL_OFFSET == 0);
    assert!(offset_of!(LispSymbol, function) == 8);
    assert!(offset_of!(LispSymbol, plist) == 16);
    assert!(offset_of!(LispSymbol, name) == 24);
    assert!(LISP_SYMBOL_FLAGS_OFFSET == 28);
    assert!(LISP_SYMBOL_INTERNED_GLOBAL_OFFSET == 29);
    assert!(offset_of!(LispSymbol, function_unbound) == 30);
    assert!(size_of::<CellWord>() == size_of::<AtomicUsize>());
    assert!(align_of::<CellWord>() == align_of::<AtomicUsize>());
    assert!(LISP_SYMBOL_VAL_OFFSET % align_of::<AtomicUsize>() == 0);
    assert!(size_of::<SymbolFlags>() == 1);
    // A cell read as a `Value` word (the GC scan, compiled code) is a
    // `Value`'s size.
    assert!(size_of::<Value>() == size_of::<usize>());
    assert!(align_of::<Value>() >= align_of::<usize>());
};

impl std::fmt::Debug for LispSymbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LispSymbol")
            .field("name", &self.name())
            .field("flags", &self.flags)
            .field("cell", &self.value_cell())
            .field("function", &self.function)
            .field("plist", &self.plist)
            .field("interned_global", &self.interned_global)
            .field("function_unbound", &self.function_unbound)
            .finish()
    }
}

// Hand-written because `name: AtomicU32` is not `Clone`-derivable. A cloned
// obarray is never concurrently scanned, so a `Relaxed` load of the source name
// is sufficient. The clone's cell names the SAME BLV record or descriptor as
// the source; `Obarray::clone` re-homes both before the copy is used
// ([`LocalizedArm::rehome`], [`ForwardedArm::forward_to`]).
impl Clone for LispSymbol {
    fn clone(&self) -> Self {
        Self {
            name: AtomicU32::new(self.name.load(Ordering::Relaxed)),
            flags: self.flags,
            val: self.val,
            function: self.function,
            plist: self.plist,
            interned_global: self.interned_global,
            function_unbound: self.function_unbound,
        }
    }
}

/// The non-redirect flags a portable dump records for a symbol.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DumpedSymbolFlags {
    pub(crate) trapped_write: SymbolTrappedWrite,
    pub(crate) interned: SymbolInterned,
    pub(crate) declared_special: bool,
}

/// The cell a portable dump can restore directly. A `Localized` or
/// `Forwarded` cell names process memory (a BLV record, a leaked
/// descriptor), so the image carries what to rebuild instead and the loader
/// makes those transitions on the live obarray; until then such a symbol
/// holds `Plain` with the value the loader will rebuild from.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DumpedCell {
    Plain(Value),
    Alias(SymId),
}

impl LispSymbol {
    /// A fully-initialized EMPTY obarray slot: `name == `[`SYMBOL_NAME_SENTINEL`]
    /// with the arm defaults GNU gives a freshly-interned symbol (Plainval /
    /// UNBOUND / NIL / NIL). Chunks are built heap-direct from this value in
    /// `SymbolChunks::grow_for`; a slot is later published by
    /// [`Self::publish_fill`], which flips the name off the sentinel LAST.
    pub(super) fn empty() -> Self {
        Self {
            name: AtomicU32::new(SYMBOL_NAME_SENTINEL.0),
            flags: SymbolFlags::default(),
            val: CellWord::plain(Value::UNBOUND),
            function: Value::NIL,
            plist: Value::NIL,
            interned_global: false,
            function_unbound: false,
        }
    }

    /// A detached symbol for ID: Plainval / UNBOUND, as GNU's `init_symbol`
    /// leaves a fresh one (`src/alloc.c`).
    pub fn new(id: SymId) -> Self {
        let sym = Self::empty();
        // Fresh single-threaded construction. The cross-thread PUBLISH (a
        // `Release` store) happens at the obarray slot fill (`publish_fill`),
        // not here, so `Relaxed` is correct for building a detached symbol.
        sym.name.store(symbol_name_id(id).0, Ordering::Relaxed);
        sym
    }

    /// A detached symbol rebuilt from a portable dump: the redirect and its
    /// word come in as one [`DumpedCell`], so the loader cannot pair a tag
    /// with another arm's word.
    pub(crate) fn from_dump(
        id: SymId,
        flags: DumpedSymbolFlags,
        cell: DumpedCell,
        function: Value,
        plist: Value,
    ) -> Self {
        let mut sym = Self::new(id);
        sym.flags.set_trapped_write(flags.trapped_write);
        sym.flags.set_interned(flags.interned);
        sym.flags.set_declared_special(flags.declared_special);
        // A detached symbol: no other thread can see it, so the cell is
        // written without the seqlock or a pre-image note.
        let (redirect, word) = match cell {
            DumpedCell::Plain(value) => (SymbolRedirect::Plainval, CellWord::plain(value)),
            DumpedCell::Alias(target) => (SymbolRedirect::Varalias, CellWord::alias(target)),
        };
        sym.flags.set_redirect(redirect);
        sym.val = word;
        sym.function = function;
        sym.plist = plist;
        sym
    }

    /// The symbol's name. Write-once, and read here only by the single mutator
    /// thread, so `Relaxed` is correct (program order orders the construction /
    /// publish store before any same-thread read). The concurrent GC obarray
    /// scan is the ONLY cross-thread reader and loads the name atom with
    /// `Acquire` itself — it does not go through this accessor.
    #[inline]
    pub fn name(&self) -> NameId {
        NameId(self.name.load(Ordering::Relaxed))
    }

    /// Presence predicate for the single mutator and stop-the-world callers
    /// (get/get_mut/iter/from_dump/trace/clone). `Relaxed`: the concurrent GC
    /// scan is the only reader that needs `Acquire`, and it loads the atom
    /// directly at its presence gate.
    #[inline]
    pub(super) fn is_present(&self) -> bool {
        self.name.load(Ordering::Relaxed) != SYMBOL_NAME_SENTINEL.0
    }

    /// Publish `src` into this (currently EMPTY) slot: write ALL arm fields
    /// FIRST, THEN `Release`-store the name LAST. The terminal `Release`
    /// publishes the whole fill; the concurrent GC obarray scan's `Acquire`
    /// load of the name is the pairing entry gate, so once the scan observes
    /// the published (non-sentinel) name every arm write above happens-before
    /// its arm reads. A plain struct memcpy would NOT establish that ordering —
    /// the name store MUST be a separate `Release` after the arm writes. Called
    /// only on a pristine empty slot (presence is monotonic: None -> Some only).
    #[inline]
    pub(super) fn publish_fill(&mut self, src: LispSymbol) {
        let published_name = src.name.load(Ordering::Relaxed);
        self.flags = src.flags;
        self.val = src.val;
        self.function = src.function;
        self.plist = src.plist;
        self.interned_global = src.interned_global;
        self.function_unbound = src.function_unbound;
        // Terminal Release: publishes the arm writes above to the GC scan's
        // Acquire load of `name`.
        self.name.store(published_name, Ordering::Release);
    }

    /// The packed flags, by value.
    #[inline(always)]
    pub fn flags(&self) -> SymbolFlags {
        self.flags
    }

    /// Read the redirect tag.
    #[inline(always)]
    pub fn redirect(&self) -> SymbolRedirect {
        self.flags.redirect()
    }

    /// GNU `SYMBOL_TRAPPED_WRITE_P`: the symbol's `trapped_write` field.
    #[inline]
    pub fn trapped_write(&self) -> SymbolTrappedWrite {
        self.flags.trapped_write()
    }

    /// GNU `set_symbol_trapped_write`.
    #[inline]
    pub(crate) fn set_trapped_write(&mut self, t: SymbolTrappedWrite) {
        self.flags.set_trapped_write(t);
    }

    /// GNU `s.interned`.
    #[inline]
    pub(crate) fn set_interned(&mut self, i: SymbolInterned) {
        self.flags.set_interned(i);
    }

    /// GNU `s.declared_special`.
    #[inline]
    pub(crate) fn set_declared_special(&mut self, v: bool) {
        self.flags.set_declared_special(v);
    }

    /// See `SymbolFlags::RUNTIME_PROJECTED_BIT`.
    #[inline]
    pub(super) fn set_runtime_projected(&mut self, v: bool) {
        self.flags.set_runtime_projected(v);
    }

    /// The symbol's 16-bit write window: the flags byte with
    /// `interned_global` above it, the two bytes at
    /// [`LISP_SYMBOL_FLAGS_OFFSET`] that an inline or cached symbol-cell
    /// write tests under `SYMCELL_INLINE_WRITE_MASK`.
    #[inline(always)]
    pub(crate) fn write_window(&self) -> u16 {
        u16::from(self.flags.bits()) | (u16::from(self.interned_global) << 8)
    }

    #[inline]
    pub fn is_interned_global(&self) -> bool {
        self.interned_global
    }

    /// The value cell: the tag, then the word it selects (GNU's `switch
    /// (sym->u.s.redirect)`). The single mutator's reader; see the module's
    /// threading note.
    #[inline(always)]
    pub(crate) fn value_cell(&self) -> ValueCell {
        self.val.decode(self.flags.redirect())
    }

    /// [`Self::value_cell`] with atomic loads (Acquire for the word), for a
    /// reader that does not own the writer side: the stop-the-world root
    /// walk, and what concurrent mutators will need. Consistent with a
    /// concurrent writer only under the chunk seqlock
    /// ([`read_symbol_children`]).
    #[inline]
    pub(crate) fn value_cell_acquire(&self) -> ValueCell {
        let redirect = self.flags.load_redirect();
        CellWord(self.load_word_acquire()).decode(redirect)
    }

    #[inline(always)]
    fn load_word_acquire(&self) -> usize {
        let p = std::ptr::from_ref(&self.val.0).cast::<AtomicUsize>();
        // SAFETY: `CellWord` is a `usize`, which `AtomicUsize` matches in
        // size and alignment (asserted above); every store to it is atomic
        // (`CellWrite::publish`) or happens before the slot is published.
        unsafe { (*p).load(Ordering::Acquire) }
    }

    /// The value of a `Plainval` cell ([`Value::UNBOUND`] when void);
    /// `None` for every other redirect.
    #[inline(always)]
    pub(crate) fn plain_value(&self) -> Option<Value> {
        match self.value_cell() {
            ValueCell::Plain(value) => Some(value),
            ValueCell::Alias(_) | ValueCell::Localized(_) | ValueCell::Forwarded(_) => None,
        }
    }

    /// The value of a `Plainval` cell as a reference into the cell, for the
    /// legacy `&Value`-returning readers (`Obarray::symbol_value_id`,
    /// `default_value_id`); `None` for every other redirect. Readers that
    /// copy ([`Self::plain_value`]) are the ones to use: a reference pins the
    /// word to plain loads.
    #[inline]
    pub(super) fn plain_value_ref(&self) -> Option<&Value> {
        match self.flags.redirect() {
            // SAFETY: `Value` is a `repr(transparent)` machine word, and
            // under this tag the word is a `Value` (`CellWord::plain`).
            SymbolRedirect::Plainval => {
                Some(unsafe { &*std::ptr::from_ref(&self.val.0).cast::<Value>() })
            }
            SymbolRedirect::Varalias | SymbolRedirect::Localized | SymbolRedirect::Forwarded => {
                None
            }
        }
    }

    /// The target of a `Varalias` cell; `None` for every other redirect.
    #[inline(always)]
    pub(crate) fn alias_target(&self) -> Option<SymId> {
        match self.value_cell() {
            ValueCell::Alias(target) => Some(target),
            ValueCell::Plain(_) | ValueCell::Localized(_) | ValueCell::Forwarded(_) => None,
        }
    }

    /// The record of a `Localized` cell; `None` for every other redirect.
    #[inline(always)]
    pub(crate) fn localized_blv(&self) -> Option<BlvPtr> {
        match self.value_cell() {
            ValueCell::Localized(blv) => Some(blv),
            ValueCell::Plain(_) | ValueCell::Alias(_) | ValueCell::Forwarded(_) => None,
        }
    }

    /// The descriptor of a `Forwarded` cell; `None` for every other redirect.
    #[inline(always)]
    pub(crate) fn forwarded_descriptor(&self) -> Option<&'static LispFwd> {
        match self.value_cell() {
            ValueCell::Forwarded(fwd) => Some(fwd),
            ValueCell::Plain(_) | ValueCell::Alias(_) | ValueCell::Localized(_) => None,
        }
    }
}

/// Compile-time contracts of the value cell: what code outside it cannot do.
///
/// The cell word is private, so nothing outside can store a payload without
/// its tag:
///
/// ```compile_fail,E0616
/// use neovm_core::emacs_core::intern::intern;
/// use neovm_core::emacs_core::symbol::LispSymbol;
///
/// let symbol = LispSymbol::new(intern("p61-word"));
/// let _word = &symbol.val;
/// ```
///
/// nor set the tag apart from its payload:
///
/// ```compile_fail,E0624
/// use neovm_core::emacs_core::intern::intern;
/// use neovm_core::emacs_core::symbol::{LispSymbol, SymbolRedirect};
///
/// let symbol = LispSymbol::new(intern("p61-tag"));
/// let mut flags = symbol.flags();
/// flags.set_redirect(SymbolRedirect::Localized);
/// ```
///
/// A forwarder descriptor cannot be fabricated, so a `Forwarded` cell always
/// names one a `defvar_*` registered:
///
/// ```compile_fail,E0451
/// use neovm_core::emacs_core::forward::{LispFwd, LispFwdType};
///
/// let _forged = LispFwd { ty: LispFwdType::Int };
/// ```
///
/// nor copied out of the storage it describes:
///
/// ```compile_fail,E0507
/// use neovm_core::emacs_core::forward::LispFwd;
///
/// fn copy(fwd: &'static LispFwd) -> LispFwd {
///     *fwd
/// }
/// ```
///
/// and its slot is not lent out as a reference:
///
/// ```compile_fail,E0624
/// use neovm_core::emacs_core::forward::LispFwd;
///
/// fn peek(fwd: &'static LispFwd) {
///     let _ = fwd.load_ref();
/// }
/// ```
#[cfg(doctest)]
pub struct ValueCellCompileContract;

// ===========================================================================
// Writing a cell
// ===========================================================================

/// One read of the concurrent-mark gate, taken by a writer before it begins
/// a [`CellWrite`]. Only [`Self::read`] makes one, so a writer cannot claim
/// "no mark is running" without having asked.
///
/// The answer stays true for the write: a mark starts only at a
/// world-stopped handshake, and a cell write reaches no safe point. It is
/// neither `Copy` nor `Clone`, so one read serves one write and cannot be
/// carried across a safe point to a later one, where a mark that began in
/// between would miss that write's pre-image. (P7.10's mutator token makes
/// the safe point itself unrepresentable here.)
#[derive(Debug)]
pub(super) struct MarkGate {
    marking: bool,
}

impl MarkGate {
    #[inline(always)]
    pub(super) fn read() -> Self {
        Self {
            marking: crate::tagged::gc::concurrent_mark_active(),
        }
    }

    #[inline(always)]
    pub(super) fn is_marking(&self) -> bool {
        self.marking
    }
}

/// Open a chunk seqlock's write window: the odd count, then a Release fence,
/// so no store inside the window can become visible before the count that
/// announces it. A Release increment alone orders only what came before it;
/// this is Boehm's fence-based writer ("Can Seqlocks Get Along With
/// Programming Language Memory Models?", MSPC 2012).
#[inline(always)]
fn seqlock_enter(seq: &AtomicU32) {
    seq.fetch_add(1, Ordering::Relaxed);
    std::sync::atomic::fence(Ordering::Release);
}

/// Close a chunk seqlock's write window: the even count, Release, after
/// every store inside it.
#[inline(always)]
fn seqlock_exit(seq: &AtomicU32) {
    seq.fetch_add(1, Ordering::Release);
}

/// Exclusive write access to one symbol's value cell: the only way to change
/// a cell's tag or word.
///
/// While a concurrent mark runs it holds the cell's chunk seqlock odd from
/// [`Self::begin`] to its drop, so the marker never pairs a tag with a word
/// from another arm, and [`Self::publish`] notes the pre-image of every plain
/// word it overwrites (snapshot-at-the-beginning). Off the mark it costs
/// nothing beyond the stores.
#[must_use = "a cell write publishes nothing until one of its transitions runs"]
pub(super) struct CellWrite<'a> {
    sym: &'a mut LispSymbol,
    /// The chunk seqlock, held odd for this write; `None` off the mark.
    seq: Option<&'a AtomicU32>,
}

impl<'a> CellWrite<'a> {
    /// Begin a write of SYM's cell. SEQ must be the seqlock of the chunk that
    /// holds SYM (the obarray passes its own); GATE is the caller's read of
    /// the mark gate.
    #[inline(always)]
    pub(super) fn begin(sym: &'a mut LispSymbol, seq: &'a AtomicU32, gate: MarkGate) -> Self {
        debug_assert!(
            gate.is_marking() || !crate::tagged::gc::concurrent_mark_active(),
            "a mark began between this write's gate read and the write"
        );
        let seq = if gate.is_marking() {
            seqlock_enter(seq);
            Some(seq)
        } else {
            None
        };
        Self { sym, seq }
    }

    /// The symbol being written, for the checks a writer makes first.
    #[inline(always)]
    pub(super) fn symbol(&self) -> &LispSymbol {
        self.sym
    }

    /// See [`LispSymbol::set_declared_special`]; `defvar_*` sets it with the
    /// forwarder.
    #[inline]
    pub(super) fn set_declared_special(&mut self, v: bool) {
        self.sym.set_declared_special(v);
    }

    /// The cell's current arm, as the transitions that arm allows.
    #[inline(always)]
    pub(super) fn arm(&mut self) -> ArmMut<'_, 'a> {
        match self.sym.value_cell() {
            ValueCell::Plain(value) => ArmMut::Plain(PlainArm { write: self, value }),
            ValueCell::Alias(_) => ArmMut::Alias(AliasArm { write: self }),
            ValueCell::Localized(blv) => ArmMut::Localized(LocalizedArm { write: self, blv }),
            ValueCell::Forwarded(fwd) => ArmMut::Forwarded(ForwardedArm { write: self, fwd }),
        }
    }

    /// THE store seam of a value cell: every change of a tag or a word is
    /// this call.
    ///
    /// Order: while a mark runs, the pre-image of a plain word is noted
    /// first (the only arm whose word is a heap reference); then the word is
    /// published with a Release store; then the tag, if it changed, with an
    /// atomic byte store. Both stores sit inside the seqlock window
    /// [`Self::begin`] opened, so the marker's consistent read sees the old
    /// pair or the new one.
    #[inline(always)]
    fn publish(&mut self, redirect: SymbolRedirect, word: CellWord) {
        if self.seq.is_some()
            && let ValueCell::Plain(old) = self.sym.value_cell()
        {
            crate::tagged::gc::note_root_overwrite_while_marking(old);
        }
        let p = std::ptr::from_mut(&mut self.sym.val.0).cast::<AtomicUsize>();
        // SAFETY: as in `LispSymbol::load_word_acquire`; `&mut` makes this
        // the only writer.
        unsafe { (*p).store(word.0, Ordering::Release) };
        if self.sym.flags.redirect() != redirect {
            self.sym.flags.set_redirect(redirect);
        }
    }
}

impl Drop for CellWrite<'_> {
    #[inline(always)]
    fn drop(&mut self) {
        if let Some(seq) = self.seq {
            seqlock_exit(seq);
        }
    }
}

/// A cell's arm, with the transitions GNU allows out of it.
pub(super) enum ArmMut<'w, 'a> {
    Plain(PlainArm<'w, 'a>),
    Alias(AliasArm<'w, 'a>),
    Localized(LocalizedArm<'w, 'a>),
    Forwarded(ForwardedArm<'w, 'a>),
}

/// A `Plainval` cell being written.
pub(super) struct PlainArm<'w, 'a> {
    write: &'w mut CellWrite<'a>,
    value: Value,
}

impl PlainArm<'_, '_> {
    /// The value the cell holds ([`Value::UNBOUND`] when void).
    #[inline(always)]
    pub(super) fn value(&self) -> Value {
        self.value
    }

    /// GNU `SET_SYMBOL_VAL`: store VALUE ([`Value::UNBOUND`] voids the cell)
    /// and hand back the value it replaced.
    #[inline(always)]
    pub(super) fn store(self, value: Value) -> Value {
        self.write
            .publish(SymbolRedirect::Plainval, CellWord::plain(value));
        self.value
    }

    /// GNU `Fdefvaralias` on a plain NEW-ALIAS (`src/eval.c:688-698`).
    pub(super) fn alias_to(self, target: SymId) {
        self.write
            .publish(SymbolRedirect::Varalias, CellWord::alias(target));
    }

    /// GNU `make_blv` on a plain variable (`src/data.c:2112-2140`).
    pub(super) fn localize(self, blv: BlvPtr) {
        self.write
            .publish(SymbolRedirect::Localized, CellWord::localized(blv));
    }

    /// GNU `defvar_int` / `defvar_bool` / `defvar_lisp` /
    /// `defvar_per_buffer` / `defvar_kboard` (`src/lread.c`,
    /// `src/buffer.c`).
    pub(super) fn forward_to(self, fwd: &'static LispFwd) {
        self.write
            .publish(SymbolRedirect::Forwarded, CellWord::forwarded(fwd));
    }
}

/// A `Varalias` cell being written.
pub(super) struct AliasArm<'w, 'a> {
    write: &'w mut CellWrite<'a>,
}

impl AliasArm<'_, '_> {
    /// GNU `Fdefvaralias` on an alias NEW-ALIAS: re-point it.
    pub(super) fn alias_to(self, target: SymId) {
        self.write
            .publish(SymbolRedirect::Varalias, CellWord::alias(target));
    }

    /// GNU `Fmakunbound` / `internal-delete-indirect-variable` on an alias:
    /// `redirect = SYMBOL_PLAINVAL; SET_SYMBOL_VAL (sym, VALUE)`
    /// (`src/data.c:781-784`, `src/eval.c:740-742`).
    pub(super) fn unalias(self, value: Value) {
        self.write
            .publish(SymbolRedirect::Plainval, CellWord::plain(value));
    }
}

/// A `Localized` cell being written. GNU never moves a localized variable
/// to another arm; its value lives in the record.
pub(super) struct LocalizedArm<'w, 'a> {
    write: &'w mut CellWrite<'a>,
    blv: BlvPtr,
}

impl LocalizedArm<'_, '_> {
    #[inline]
    pub(super) fn blv(&self) -> BlvPtr {
        self.blv
    }

    /// Point the cell at BLV, the deep copy of its record. Only for
    /// `Obarray::clone`, whose copied cells still name the source's records.
    pub(super) fn rehome(self, blv: BlvPtr) {
        self.write
            .publish(SymbolRedirect::Localized, CellWord::localized(blv));
    }
}

/// A `Forwarded` cell being written.
pub(super) struct ForwardedArm<'w, 'a> {
    write: &'w mut CellWrite<'a>,
    fwd: &'static LispFwd,
}

impl ForwardedArm<'_, '_> {
    #[inline]
    pub(super) fn descriptor(&self) -> &'static LispFwd {
        self.fwd
    }

    /// Install FWD in place of the current descriptor (a re-registration,
    /// or `Obarray::clone` re-homing a stateful descriptor's copy).
    pub(super) fn forward_to(self, fwd: &'static LispFwd) {
        self.write
            .publish(SymbolRedirect::Forwarded, CellWord::forwarded(fwd));
    }

    /// GNU `make_blv` on a forwarded variable: the record keeps the
    /// descriptor (`blv->fwd`), the cell moves to it.
    pub(super) fn localize(self, blv: BlvPtr) {
        self.write
            .publish(SymbolRedirect::Localized, CellWord::localized(blv));
    }
}

// ===========================================================================
// The GC's reader
// ===========================================================================

/// Read a symbol's traceable heap children CONSISTENTLY with concurrent mutator
/// arm changes, for the Stage 1b concurrent obarray scan (the GC-thread read
/// side; pairs with [`CellWrite`] on the write side).
///
/// `seq` is the symbol's per-chunk seqlock; `sym` the symbol in that chunk. The
/// standard seqlock read protocol (retry while the counter is odd or changes
/// across the read) guarantees the `(redirect, val)` pair is observed from a
/// single epoch — never torn — so `val` is interpreted only as the arm the
/// consistently-observed `redirect` names. Only `Plainval` holds a heap value
/// cell: alias = a non-heap `SymId`, localized = a BLV record, forwarded = a
/// descriptor — none is a heap `Value` to trace here (BLV interiors are reached
/// via the BLV-pool root, owned descriptor values via the forwarder roots).
/// `function`/`plist` are single-word atomic `Value`s with no discriminant, so
/// they are always consistent. `push` is called for each heap-object child to
/// enqueue onto the GC gray set.
///
/// Caller must hold the start-of-cycle chunk snapshot so `sym`/`seq` address live,
/// non-moving memory. Bounded in practice: with a single mutator the odd window
/// is ~4 stores, so the retry loop converges immediately.
#[cfg(test)]
pub(crate) fn read_symbol_children_consistent(
    seq: &AtomicU32,
    sym: &LispSymbol,
    push: impl FnMut(Value),
) {
    read_symbol_children::<false>(seq, sym, push);
}

pub(super) fn read_symbol_children<const MAJOR: bool>(
    seq: &AtomicU32,
    sym: &LispSymbol,
    mut push: impl FnMut(Value),
) {
    loop {
        let s1 = seq.load(Ordering::Acquire);
        if s1 & 1 != 0 {
            // A `(flags, val)` arm change is in flight in this chunk — wait it out.
            std::hint::spin_loop();
            continue;
        }
        let redirect = sym.flags.load_redirect();
        // Read the word regardless of arm; it is only INTERPRETED below when
        // the consistently-observed redirect is `Plainval`.
        let word = sym.load_word_acquire();
        let function = load_value_atomic(&sym.function);
        let plist = load_value_atomic(&sym.plist);
        // Boehm's reader half of `seqlock_enter`: a load above that saw a
        // store from a write window also sees, after this fence, the odd
        // count that opened it.
        std::sync::atomic::fence(Ordering::Acquire);
        if seq.load(Ordering::Relaxed) != s1 {
            // An arm change landed during the read — the quadruple may be torn.
            continue;
        }
        // Consistent snapshot. `is_heap_object()` excludes fixnums, nil, symbol
        // ids and UNBOUND, so the Plainval gate never traces a non-heap word.
        if let ValueCell::Plain(plain) = CellWord(word).decode(redirect)
            && (plain.is_heap_object()
                || (MAJOR && matches!(plain.kind(), crate::tagged::value::ValueKind::Symbol(_))))
        {
            push(plain);
        }
        if function.is_heap_object()
            || (MAJOR && matches!(function.kind(), crate::tagged::value::ValueKind::Symbol(_)))
        {
            push(function);
        }
        if plist.is_heap_object()
            || (MAJOR && matches!(plist.kind(), crate::tagged::value::ValueKind::Symbol(_)))
        {
            push(plist);
        }
        return;
    }
}

#[cfg(test)]
#[path = "tests/cell.rs"]
mod tests;
