//! Process-global symbol registry backed by a separate string atom table.
//!
//! `SymId` is Lisp symbol identity and must stay stable across evaluator
//! creation/destruction so values can be formatted, compared, and moved
//! between contexts without keeping an old `Context` alive just for name
//! resolution. The runtime therefore uses a single append-only process symbol
//! registry.
//!
//! Name atoms are tracked separately via [`NameId`]. This mirrors GNU's model
//! more closely: a symbol is an object with a name, not just "slot N in the
//! string interner".

use hashbrown::HashMap;
use parking_lot::RwLock;
use rustc_hash::{FxBuildHasher, FxHashMap};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::hash::{BuildHasher, Hash, Hasher};
use std::mem::MaybeUninit;
use std::ptr::NonNull;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::heap_types::LispString;
use crate::tagged::value::TaggedValue;

/// A compact handle to a Lisp symbol object. Copy, 4 bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[repr(transparent)]
pub struct SymId(pub(crate) u32);

impl std::fmt::Debug for SymId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Resolve the symbol name so logs read `SymId(696 peculiar-error)`
        // instead of a bare id — otherwise a signal in a bug report is
        // undiagnosable. NEVER block: `Debug` can fire while this or another
        // thread holds the registry *write* lock (mid-intern), so a blocking
        // read could deadlock. `try_read` degrades to the id on contention.
        match global_symbol_registry().try_read() {
            Some(registry) => match registry.slot(*self) {
                Some(slot) => {
                    let name = registry.names.resolve_lisp_string(slot.name);
                    write!(
                        f,
                        "SymId({} {})",
                        self.0,
                        format_lisp_symbol_name_for_diagnostic(name)
                    )
                }
                None => write!(f, "SymId({})", self.0),
            },
            None => write!(f, "SymId({})", self.0),
        }
    }
}

/// Render a symbol name into a UTF-8 diagnostic without discarding Lisp
/// character identity.
///
/// GNU's symbol printer walks the name as a Lisp string instead of first
/// requiring UTF-8. Rust log sinks cannot carry Emacs byte8 or non-Unicode
/// characters directly, so preserve them with a fixed-width `\xNN` notation
/// that ordinary symbol escaping cannot produce. In particular, a unibyte
/// `C3 A9` name must stay `\xC3\xA9`; it is not the multibyte Unicode name `é`.
fn format_lisp_symbol_name_for_diagnostic(name: &LispString) -> String {
    fn push_char(out: &mut String, code: u32, confusing_prefix: bool) {
        if confusing_prefix {
            out.push('\\');
        }
        if crate::emacs_core::emacs_char::char_byte8_p(code) {
            let byte = crate::emacs_core::emacs_char::char_to_byte8(code);
            write!(out, "\\x{byte:02X}").expect("write to String");
            return;
        }

        let Some(ch) = char::from_u32(code) else {
            write!(out, "\\x{{{code:X}}}").expect("write to String");
            return;
        };
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => {
                write!(out, "\\x{{{code:X}}}").expect("write to String");
            }
            '"' | '\'' | ';' | '#' | '(' | ')' | ',' | '`' | '[' | ']' | ' ' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }

    if name.is_empty() {
        return "##".to_string();
    }

    let bytes = name.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut confusing_prefix = symbol_name_confusing(bytes);
    if !name.is_multibyte() {
        for &byte in bytes {
            push_char(
                &mut out,
                crate::emacs_core::emacs_char::unibyte_to_char(byte),
                confusing_prefix,
            );
            confusing_prefix = false;
        }
        return out;
    }

    let mut pos = 0;
    while pos < bytes.len() {
        let (code, len) = crate::emacs_core::emacs_char::string_char(&bytes[pos..]);
        push_char(&mut out, code, confusing_prefix);
        confusing_prefix = false;
        pos += len;
    }
    out
}

pub(crate) fn format_symbol_name_for_diagnostic(symbol: SymId) -> String {
    // The LIVE Lisp-visible name object where one exists, not the immutable
    // atom: GNU permits `aset` on a symbol's name string and a diagnostic must
    // show the mutation (`symbol_diagnostics_use_the_lisp_visible_name_object`).
    //
    // This used to call `resolve_sym_lisp_name`, which DIVERGENCES.md 167
    // deleted because it flattened both cases into one `&'static LispString` --
    // sound for the leaked atom, a lie for the GC-managed object. The typed
    // view is the replacement: hold it, and take the borrow at the point of
    // use, exactly as GNU reaches for `SDATA` only after `s1 = SYMBOL_NAME (s1)`
    // (src/fns.c:344-353).
    let name = resolve_lisp_visible_symbol_name(symbol);
    format_lisp_symbol_name_for_diagnostic(name.text())
}

/// Whether an unescaped symbol spelling would be read as something other than
/// that symbol. Shared by the Lisp printer and UTF-8 diagnostic renderer so
/// their escaping rules cannot drift.
pub(crate) fn symbol_name_confusing(bytes: &[u8]) -> bool {
    let Some(&first) = bytes.first() else {
        return false;
    };

    let signed = matches!(first, b'+' | b'-');
    let after_sign = usize::from(signed);
    let number_like_start = bytes
        .get(after_sign)
        .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.');

    (number_like_start && emacs_decimal_number_consumes_all(bytes))
        || first == b'?'
        || (first == b'.' && !bytes.get(1).is_some_and(|byte| byte.is_ascii_alphabetic()))
}

fn emacs_decimal_number_consumes_all(bytes: &[u8]) -> bool {
    let mut pos = 0usize;
    if matches!(bytes.get(pos), Some(b'+' | b'-')) {
        pos += 1;
    }

    let mut has_leading_digits = false;
    while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
        has_leading_digits = true;
        pos += 1;
    }

    if bytes.get(pos) == Some(&b'.') {
        pos += 1;
    }

    let mut has_trailing_digits = false;
    while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
        has_trailing_digits = true;
        pos += 1;
    }

    let mut has_exponent = false;
    if matches!(bytes.get(pos), Some(b'e' | b'E')) {
        let exponent_start = pos;
        pos += 1;
        let exponent_sign_is_plus = bytes.get(pos) == Some(&b'+');
        if matches!(bytes.get(pos), Some(b'+' | b'-')) {
            pos += 1;
        }

        let exponent_digits_start = pos;
        while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
            pos += 1;
        }

        if pos > exponent_digits_start {
            has_exponent = true;
        } else if exponent_sign_is_plus
            && bytes
                .get(pos..pos + 3)
                .is_some_and(|suffix| suffix == b"INF" || suffix == b"NaN")
        {
            pos += 3;
            has_exponent = true;
        } else {
            pos = exponent_start;
        }
    }

    let float_syntax = has_trailing_digits || (has_leading_digits && has_exponent);
    pos == bytes.len() && (has_leading_digits || float_syntax)
}

/// A compact handle to a deduplicated symbol-name atom. Runtime-local only.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(transparent)]
pub struct NameId(pub(crate) u32);

pub const NIL_SYM_ID: SymId = SymId(0);

/// Number of symbols `SymbolRegistry::new` seeds before anything interns:
/// nil, t, and the non-canonical `unbound` sentinel. The pdump restore maps
/// this prefix by POSITION (see `restore_dump_symbol_table`).
pub(crate) const SEED_SYMBOL_COUNT: usize = 3;
pub const T_SYM_ID: SymId = SymId(1);
pub const UNBOUND_SYM_ID: SymId = SymId(2);

/// Number of symbol-name atoms stored in each non-moving allocation.
const NAME_ATOM_CHUNK: usize = 4096;

/// Append-only, process-lifetime storage for interned symbol names.
///
/// The symbol registry exposes name atoms as `&'static LispString`, so the
/// backing allocations are intentionally leaked just as the former per-name
/// `Box::leak` allocations were. Chunking removes one allocator allocation and
/// one `Vec` pointer per distinct name while preserving stable addresses.
struct NameAtomStorage {
    chunks: Vec<NonNull<LispString>>,
    len: usize,
}

// SAFETY: chunks are appended and initialized only through `&mut self` while
// the enclosing `StringInterner` is write-locked. Published `LispString`s are
// immutable and their leaked backing allocations never move or disappear.
unsafe impl Send for NameAtomStorage {}
unsafe impl Sync for NameAtomStorage {}

impl NameAtomStorage {
    fn new() -> Self {
        Self {
            chunks: Vec::new(),
            len: 0,
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.len
    }

    fn reserve(&mut self, additional: usize) {
        let required_chunks = self
            .len
            .saturating_add(additional)
            .div_ceil(NAME_ATOM_CHUNK);
        self.chunks
            .reserve(required_chunks.saturating_sub(self.chunks.len()));
    }

    fn push(&mut self, value: LispString) -> &'static LispString {
        let chunk_index = self.len / NAME_ATOM_CHUNK;
        let slot_index = self.len % NAME_ATOM_CHUNK;
        if slot_index == 0 {
            let chunk = Box::<[LispString]>::new_uninit_slice(NAME_ATOM_CHUNK);
            let raw = Box::into_raw(chunk) as *mut MaybeUninit<LispString>;
            self.chunks.push(
                NonNull::new(raw.cast::<LispString>())
                    .expect("a non-empty Box allocation must not be null"),
            );
        }

        // SAFETY: `chunk_index` exists after the allocation above and
        // `slot_index` selects the next, as-yet-uninitialized slot. No slot is
        // ever written twice. The allocation is intentionally leaked and the
        // initialized value is never mutated, so the returned reference stays
        // valid for the remainder of the process.
        let slot = unsafe { self.chunks[chunk_index].as_ptr().add(slot_index) };
        unsafe { slot.write(value) };
        self.len += 1;
        unsafe { &*slot }
    }

    #[inline]
    fn get(&self, id: NameId) -> &'static LispString {
        let index = id.0 as usize;
        assert!(index < self.len, "invalid symbol name id {id:?}");
        let chunk_index = index / NAME_ATOM_CHUNK;
        let slot_index = index % NAME_ATOM_CHUNK;
        // SAFETY: the bounds check above proves this slot was initialized.
        // Chunks are leaked and initialized values never move or mutate.
        unsafe { &*self.chunks[chunk_index].as_ptr().add(slot_index) }
    }

    fn iter(&self) -> impl Iterator<Item = &'static LispString> + '_ {
        (0..self.len).map(|index| self.get(NameId(index as u32)))
    }
}

/// Append-only string interner used only for symbol names.
pub struct StringInterner {
    strings: NameAtomStorage,
    map: HashMap<&'static LispString, NameId, FxBuildHasher>,
}

impl Default for StringInterner {
    fn default() -> Self {
        Self::new()
    }
}

/// Fold a symbol name to the representation symbol IDENTITY is decided on.
///
/// GNU's `oblookup` compares a name's char count, byte count and bytes, never
/// its multibyte FLAG (lread.c), so an ascii-only multibyte spelling and its
/// unibyte spelling name the same symbol. Folding ascii-only multibyte names to
/// unibyte reproduces that on our flag-sensitive `LispString` comparison.
///
/// This is about identity ONLY. Which string object `symbol-name` returns is a
/// separate question, answered by the name object a symbol was created from --
/// GNU keeps that unfolded, multibyte flag and all.
pub(crate) fn normalize_symbol_name_lisp_string(s: &LispString) -> Cow<'_, LispString> {
    if s.is_ascii() && s.is_multibyte() {
        Cow::Owned(LispString::from_unibyte_slice(s.as_bytes()))
    } else {
        Cow::Borrowed(s)
    }
}

impl StringInterner {
    fn normalize_symbol_name_lisp_string<'a>(s: &'a LispString) -> Cow<'a, LispString> {
        normalize_symbol_name_lisp_string(s)
    }

    pub fn new() -> Self {
        Self {
            strings: NameAtomStorage::new(),
            map: HashMap::with_hasher(FxBuildHasher),
        }
    }

    fn reserve_additional_names(&mut self, additional: usize) {
        self.strings.reserve(additional);
        self.map.reserve(additional);
    }

    #[inline]
    fn hash_name_parts(&self, bytes: &[u8], multibyte: bool) -> u64 {
        // Keep this exactly aligned with `LispString::hash` so a borrowed
        // byte/representation query lands in the canonical map's bucket.
        let mut hasher = self.map.hasher().build_hasher();
        bytes.hash(&mut hasher);
        multibyte.hash(&mut hasher);
        hasher.finish()
    }

    #[inline]
    fn lookup_name_parts(&self, bytes: &[u8], multibyte: bool) -> Option<NameId> {
        let hash = self.hash_name_parts(bytes, multibyte);
        self.map
            .raw_entry()
            .from_hash(hash, |candidate| {
                candidate.is_multibyte() == multibyte && candidate.as_bytes() == bytes
            })
            .map(|(_, id)| *id)
    }

    fn name_atom_from_str(s: &str) -> LispString {
        if s.is_ascii() {
            LispString::from_unibyte_slice(s.as_bytes())
        } else {
            LispString::from_utf8(s)
        }
    }

    /// Intern a symbol-name atom, returning its unique id.
    pub fn intern(&mut self, s: &str) -> NameId {
        let multibyte = !s.is_ascii();
        if let Some(idx) = self.lookup_name_parts(s.as_bytes(), multibyte) {
            return idx;
        }
        let atom = Self::name_atom_from_str(s);
        self.intern_lisp_string(&atom)
    }

    /// Intern a symbol-name atom from an exact Lisp string representation.
    pub fn intern_lisp_string(&mut self, s: &LispString) -> NameId {
        let normalized = Self::normalize_symbol_name_lisp_string(s);
        if let Some(idx) = self.lookup_name_parts(normalized.as_bytes(), normalized.is_multibyte())
        {
            return idx;
        }
        let idx = NameId(self.strings.len() as u32);
        // `NameId(u32::MAX)` is reserved as the obarray empty-slot presence
        // sentinel (`symbol::SYMBOL_NAME_SENTINEL`): a `LispSymbol` slot is
        // "empty" iff its atomic `name` equals it. NameIds mint densely from 0,
        // so reaching `u32::MAX` (4.3B distinct symbol names) means the sentinel
        // would alias a real name and a live slot could read as empty.
        debug_assert_ne!(
            idx,
            crate::emacs_core::symbol::SYMBOL_NAME_SENTINEL,
            "NameId space exhausted: a real symbol name collided with the \
             obarray empty-slot presence sentinel (u32::MAX)",
        );
        let interned = self.strings.push(normalized.into_owned());
        self.map.insert(interned, idx);
        idx
    }

    /// Dump-table bulk intern: `name` was written by a valid interner, so it
    /// is already normalized (debug-checked), and a mapped name's bytes stay
    /// mapped for the process lifetime — so the general path's deep
    /// `LispString::clone` (an allocation + byte copy per name, ~17k names at
    /// startup) is replaced by a header-word [`LispString::borrowed_alias`].
    /// Owned or interval-carrying names fall back to the general path.
    pub fn intern_dump_lisp_string(&mut self, name: &LispString) -> NameId {
        debug_assert!(
            matches!(
                Self::normalize_symbol_name_lisp_string(name),
                Cow::Borrowed(_)
            ),
            "dump symbol name arrived unnormalized"
        );
        let Some(alias) = name.borrowed_alias() else {
            return self.intern_lisp_string(name);
        };
        if let Some(idx) = self.lookup_name_parts(name.as_bytes(), name.is_multibyte()) {
            return idx;
        }
        let idx = NameId(self.strings.len() as u32);
        debug_assert_ne!(
            idx,
            crate::emacs_core::symbol::SYMBOL_NAME_SENTINEL,
            "NameId space exhausted: a real symbol name collided with the \
             obarray empty-slot presence sentinel (u32::MAX)",
        );
        let interned = self.strings.push(alias);
        self.map.insert(interned, idx);
        idx
    }

    /// Look up a symbol-name atom without interning it.
    pub fn lookup(&self, s: &str) -> Option<NameId> {
        self.lookup_name_parts(s.as_bytes(), !s.is_ascii())
    }

    /// Look up a symbol-name atom without interning it.
    pub fn lookup_lisp_string(&self, s: &LispString) -> Option<NameId> {
        let normalized = Self::normalize_symbol_name_lisp_string(s);
        self.lookup_name_parts(normalized.as_bytes(), normalized.is_multibyte())
    }

    /// Resolve a name id back to its string. Panics if id is invalid.
    #[inline]
    pub fn resolve(&self, id: NameId) -> &'static str {
        self.resolve_lisp_string(id)
            .as_utf8_str()
            .unwrap_or_else(|| panic!("symbol name {:?} is not valid UTF-8", id))
    }

    /// Resolve a name id back to its exact Lisp-string storage.
    #[inline]
    pub fn resolve_lisp_string(&self, id: NameId) -> &'static LispString {
        self.strings.get(id)
    }
}

/// Identity of the tagged heap that owns a Lisp-visible symbol name object.
///
/// Keeping this distinct from object identity makes it impossible to index the
/// per-heap root table with a raw object address (or vice versa).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
struct SymbolNameHeapId(usize);

/// Pointer identity of one Lisp-visible symbol name object.
///
/// `TaggedValue`'s `Eq`/`Hash` are structural, while GC roots are identities:
/// two equal strings must remain separate roots and one shared string must be
/// seeded only once.  This key makes that distinction explicit in the type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
struct SymbolNameObjectId(usize);

impl SymbolNameObjectId {
    fn of(value: TaggedValue) -> Self {
        Self(value.bits())
    }
}

#[derive(Clone, Copy, Debug)]
struct SymbolNameValue {
    value: TaggedValue,
    heap_id: SymbolNameHeapId,
}

/// Identity of a lazily materialized atom-backed symbol name.
///
/// A `SymId` is process-global while a Lisp string belongs to one tagged
/// heap.  Making both axes part of the key prevents a name object allocated by
/// one evaluator from being returned in another evaluator's heap.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct MaterializedSymbolNameKey {
    heap_id: SymbolNameHeapId,
    symbol: SymId,
}

/// The unique name objects that must be seeded for each live tagged heap.
///
/// Many uninterned symbols may deliberately share one exact name string (GNU
/// `make-symbol` preserves the argument object).  Root cardinality therefore
/// follows name-object identity, not symbol cardinality.
#[derive(Debug, Default)]
struct SymbolNameRootIndex {
    by_heap: FxHashMap<SymbolNameHeapId, FxHashMap<SymbolNameObjectId, TaggedValue>>,
}

impl SymbolNameRootIndex {
    fn insert(&mut self, name: SymbolNameValue) {
        let object_id = SymbolNameObjectId::of(name.value);
        let old = self
            .by_heap
            .entry(name.heap_id)
            .or_default()
            .insert(object_id, name.value);
        debug_assert!(
            old.is_none_or(|old| old.bits() == name.value.bits()),
            "one symbol-name object identity mapped to different values"
        );
    }

    fn extend_roots(&self, roots: &mut Vec<TaggedValue>, heap_id: SymbolNameHeapId) {
        if let Some(by_object) = self.by_heap.get(&heap_id) {
            roots.extend(by_object.values().copied());
        }
    }

    #[cfg(all(test, debug_assertions))]
    fn root_count(&self, heap_id: SymbolNameHeapId) -> usize {
        self.by_heap.get(&heap_id).map_or(0, FxHashMap::len)
    }
}

/// The name a freshly allocated symbol carries, stated at every construction
/// site so "no Lisp name object" is a claim rather than a forgotten argument.
///
/// GNU has only the second case: `intern_driver` and `Fmake_symbol` both store
/// THE STRING OBJECT they were handed (lread.c:4705-4708), so `symbol-name`
/// returns it with its text properties, its multibyteness and any later
/// mutation intact. We additionally construct symbols from Rust text -- the
/// reader, the dumper, bootstrap -- where no Lisp object exists to store; those
/// sites say `AtomOnly`; the first Lisp-visible `symbol-name` read materializes
/// one heap-local string from the atom and subsequent reads reuse it.
#[derive(Clone, Copy, Debug)]
enum NewSymbolName {
    /// No Lisp string object was involved; the interned name atom is the whole
    /// of the symbol's name.
    AtomOnly,
    /// GNU's case: this string object IS the symbol's name.
    LispObject(SymbolNameValue),
}

/// Storage selected for a symbol's first Lisp-visible name.
///
/// This discriminator lives in padding already present in [`SymbolSlot`].  It
/// avoids probing both sparse object tables for every symbol-name read while
/// keeping heap-local pointers out of the process-global dense symbol array.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum SymbolNameOrigin {
    AtomOnly,
    LispObject,
}

/// The Lisp-visible name of a symbol, keeping GNU's two ownership cases
/// distinct at the type boundary.
///
/// Lisp-created symbols retain the exact mutable string object supplied to
/// `intern` or `make-symbol`. Symbols synthesized by the reader, dumper, or
/// Rust bootstrap begin with only a process-lifetime immutable name atom.  A
/// Lisp-visible read can borrow that atom while filtering, then materialize one
/// cached heap object when it must return a value.  Callers must not silently
/// substitute the atom once an object exists: GNU `symbol-name` and every
/// primitive built on it observe later mutation of the object.
#[derive(Clone, Copy, Debug)]
pub(crate) enum LispVisibleSymbolName {
    LispObject(TaggedValue),
    Atom {
        symbol: SymId,
        string: &'static LispString,
    },
}

impl LispVisibleSymbolName {
    /// Borrow the name string, for no longer than this view lives.
    ///
    /// GNU spells the same coercion `s1 = SYMBOL_NAME (s1)` and only then
    /// reaches for `SDATA (s1)` (`src/fns.c:344-353`): what travels between
    /// statements is the Lisp OBJECT, and the byte pointer is taken at the
    /// point of use.  This is that, in types.  The object arm holds a
    /// `TaggedValue`, so the borrow is reborrowed from `&self` and cannot
    /// outlive the view; only the atom arm is genuinely `'static`, and it says
    /// so in its own field rather than in the return type of both.
    pub(crate) fn text(&self) -> &LispString {
        match self {
            Self::LispObject(value) => value
                .as_lisp_string()
                .expect("a registered symbol name object remains a string"),
            Self::Atom { string, .. } => string,
        }
    }
}

impl NewSymbolName {
    fn origin(self) -> SymbolNameOrigin {
        match self {
            Self::AtomOnly => SymbolNameOrigin::AtomOnly,
            Self::LispObject(_) => SymbolNameOrigin::LispObject,
        }
    }

    /// Adopt a Lisp string object as a new symbol's name, as GNU's
    /// `Fmake_symbol (string)` does.
    fn from_lisp_object(value: TaggedValue) -> Self {
        let heap_id = crate::tagged::gc::current_tagged_heap_identity()
            .expect("a Lisp symbol name value requires an installed tagged heap");
        Self::LispObject(SymbolNameValue {
            value,
            heap_id: SymbolNameHeapId(heap_id),
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct SymbolSlot {
    name: NameId,
    canonical: bool,
    name_origin: SymbolNameOrigin,
}

pub(crate) struct DumpedSymbolTable {
    pub names: Vec<LispString>,
    pub symbol_names: Vec<u32>,
    pub canonical: Vec<bool>,
}

#[derive(Debug)]
pub(crate) struct RestoredDumpSymbolTable {
    pub names: Vec<NameId>,
    pub symbols: Vec<SymId>,
}

/// Process-global append-only registry of Lisp symbols.
struct SymbolRegistry {
    names: StringInterner,
    symbols: Vec<SymbolSlot>,
    canonical_by_name: FxHashMap<NameId, SymId>,
    /// Exact heap string objects used as symbol names when Lisp supplies one
    /// directly. GNU stores that object in the symbol, so `(symbol-name
    /// (make-symbol NAME))` is `eq` to NAME and sees later string mutation.
    /// Keeping this rare case out of `SymbolSlot` makes every ordinary symbol
    /// substantially smaller.
    name_values: FxHashMap<SymId, SymbolNameValue>,
    /// Per-heap Lisp objects lazily created for symbols whose name originated
    /// as a Rust/process-lifetime atom.  GNU always stores one Lisp string in
    /// every symbol; this cache supplies the equivalent object without making
    /// every process-global [`SymbolSlot`] carry a heap-local pointer.
    materialized_name_values: FxHashMap<MaterializedSymbolNameKey, TaggedValue>,
    /// Per-heap set of exact Lisp name objects. This is deliberately indexed
    /// by object identity rather than symbol id: many uninterned symbols can
    /// share one name object, and seeding it once is sufficient.
    name_value_roots: SymbolNameRootIndex,
}

impl Default for SymbolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolRegistry {
    fn new() -> Self {
        let mut registry = Self {
            names: StringInterner::new(),
            symbols: Vec::new(),
            canonical_by_name: FxHashMap::default(),
            name_values: FxHashMap::default(),
            materialized_name_values: FxHashMap::default(),
            name_value_roots: SymbolNameRootIndex::default(),
        };
        let nil_name = registry.names.intern("nil");
        let nil_id = registry.alloc_symbol(nil_name, true, NewSymbolName::AtomOnly);
        debug_assert_eq!(nil_id, NIL_SYM_ID);

        let t_name = registry.names.intern("t");
        let t_id = registry.alloc_symbol(t_name, true, NewSymbolName::AtomOnly);
        debug_assert_eq!(t_id, T_SYM_ID);

        let unbound_name = registry.names.intern("unbound");
        let unbound_id = registry.alloc_symbol(unbound_name, false, NewSymbolName::AtomOnly);
        debug_assert_eq!(unbound_id, UNBOUND_SYM_ID);

        registry
    }

    fn alloc_symbol(&mut self, name: NameId, canonical: bool, name_value: NewSymbolName) -> SymId {
        let id = SymId(self.symbols.len() as u32);
        self.symbols.push(SymbolSlot {
            name,
            canonical,
            name_origin: name_value.origin(),
        });
        if let NewSymbolName::LispObject(name_value) = name_value {
            let old = self.name_values.insert(id, name_value);
            debug_assert!(old.is_none(), "new symbol id already had a name value");
            self.name_value_roots.insert(name_value);
        }
        if canonical {
            self.canonical_by_name.insert(name, id);
        }
        id
    }

    fn slot(&self, id: SymId) -> Option<&SymbolSlot> {
        self.symbols.get(id.0 as usize)
    }

    fn intern(&mut self, s: &str) -> SymId {
        let name = self.names.intern(s);
        if let Some(existing) = self.canonical_by_name.get(&name) {
            return *existing;
        }
        self.alloc_symbol(name, true, NewSymbolName::AtomOnly)
    }

    fn intern_lisp_string(&mut self, s: &LispString) -> SymId {
        let name = self.names.intern_lisp_string(s);
        if let Some(existing) = self.canonical_by_name.get(&name) {
            return *existing;
        }
        self.alloc_symbol(name, true, NewSymbolName::AtomOnly)
    }

    fn intern_uninterned(&mut self, s: &str) -> SymId {
        let name = self.names.intern(s);
        self.alloc_symbol(name, false, NewSymbolName::AtomOnly)
    }

    fn intern_uninterned_lisp_string(&mut self, s: &LispString) -> SymId {
        let name = self.names.intern_lisp_string(s);
        self.alloc_symbol(name, false, NewSymbolName::AtomOnly)
    }

    /// GNU `Fintern` on a Lisp string: return the canonical symbol of that
    /// name, and when this call is the one that CREATES it, adopt the argument
    /// as the symbol's name object (lread.c:4796-4805 -> `intern_driver` ->
    /// `Fmake_symbol (string)`). An already-interned symbol keeps the name it
    /// was created with, so the argument's text properties stay invisible --
    /// GNU reaches `intern_driver` only when `oblookup` found nothing.
    ///
    /// Name IDENTITY still runs through the normalized name atom, so a unibyte
    /// and an ascii-only multibyte spelling name the same symbol either way;
    /// only which string object `symbol-name` hands back is at stake here.
    fn intern_lisp_value(&mut self, name_value: TaggedValue) -> SymId {
        let name = self
            .names
            .intern_lisp_string(name_value.as_lisp_string().expect("string name"));
        if let Some(existing) = self.canonical_by_name.get(&name) {
            return *existing;
        }
        self.alloc_symbol(name, true, NewSymbolName::from_lisp_object(name_value))
    }

    fn make_uninterned_symbol_with_name_value(&mut self, name_value: TaggedValue) -> SymId {
        let name = self
            .names
            .intern_lisp_string(name_value.as_lisp_string().expect("string name"));
        self.alloc_symbol(name, false, NewSymbolName::from_lisp_object(name_value))
    }

    fn lookup(&self, s: &str) -> Option<SymId> {
        let name = self.names.lookup(s)?;
        self.canonical_by_name.get(&name).copied()
    }

    fn lookup_lisp_string(&self, s: &LispString) -> Option<SymId> {
        let name = self.names.lookup_lisp_string(s)?;
        self.canonical_by_name.get(&name).copied()
    }

    #[inline]
    fn is_canonical_id(&self, id: SymId) -> bool {
        self.slot(id).map(|slot| slot.canonical).unwrap_or(false)
    }

    #[inline]
    fn resolve(&self, id: SymId) -> &'static str {
        self.resolve_lisp_string(id)
            .as_utf8_str()
            .unwrap_or_else(|| panic!("symbol name {:?} is not valid UTF-8", id))
    }

    /// A symbol's NAME ATOM: process-lifetime, byte-exact, and the footing
    /// symbol identity is decided on. Never the Lisp name object.
    ///
    /// The two must not be conflated. The atom lives for the process, so
    /// callers may cache it as `&'static` (`thread_local_resolve` does); the
    /// name object is an ordinary GC-managed heap string belonging to one heap,
    /// and handing it out here would let a `&'static` outlive it. Lisp-visible
    /// name reads go through [`Self::resolve_name_value`] instead, which is
    /// what `symbol-name` prefers.
    #[inline]
    fn resolve_lisp_string(&self, id: SymId) -> &'static LispString {
        let slot = self
            .slot(id)
            .unwrap_or_else(|| panic!("invalid symbol id {:?}", id));
        self.names.resolve_lisp_string(slot.name)
    }

    #[inline]
    fn resolve_name_value_in_heap(
        &self,
        id: SymId,
        heap_id: SymbolNameHeapId,
    ) -> Option<TaggedValue> {
        let slot = self
            .slot(id)
            .unwrap_or_else(|| panic!("invalid symbol id {:?}", id));
        if slot.name_origin == SymbolNameOrigin::LispObject {
            note_exact_symbol_name_value_probe();
            if let Some(name_value) = self
                .name_values
                .get(&id)
                .copied()
                .filter(|name_value| name_value.heap_id == heap_id)
            {
                return Some(name_value.value);
            }
        }

        note_materialized_symbol_name_value_probe();
        self.materialized_name_values
            .get(&MaterializedSymbolNameKey {
                heap_id,
                symbol: id,
            })
            .copied()
    }

    #[inline]
    fn resolve_name_value(&self, id: SymId) -> Option<TaggedValue> {
        let heap_id = crate::tagged::gc::current_tagged_heap_identity().map(SymbolNameHeapId)?;
        self.resolve_name_value_in_heap(id, heap_id)
    }

    #[inline]
    fn resolve_lisp_visible_name(&self, id: SymId) -> LispVisibleSymbolName {
        if let Some(value) = self.resolve_name_value(id) {
            debug_assert!(
                value.is_string(),
                "a registered symbol name object remains a string"
            );
            LispVisibleSymbolName::LispObject(value)
        } else {
            LispVisibleSymbolName::Atom {
                symbol: id,
                string: self.resolve_lisp_string(id),
            }
        }
    }

    #[inline]
    fn name_id(&self, id: SymId) -> NameId {
        self.slot(id)
            .unwrap_or_else(|| panic!("invalid symbol id {:?}", id))
            .name
    }

    #[inline]
    fn resolve_name(&self, id: NameId) -> &'static str {
        self.names.resolve(id)
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn resolve_name_lisp_string(&self, id: NameId) -> &'static LispString {
        self.names.resolve_lisp_string(id)
    }

    fn dump_symbol_table(&self) -> DumpedSymbolTable {
        let names = self.names.strings.iter().map(LispString::clone).collect();
        let mut symbol_names = Vec::with_capacity(self.symbols.len());
        let mut canonical = Vec::with_capacity(self.symbols.len());
        for slot in &self.symbols {
            symbol_names.push(slot.name.0);
            canonical.push(slot.canonical);
        }
        DumpedSymbolTable {
            names,
            symbol_names,
            canonical,
        }
    }

    fn restore_dump_symbol_table(
        &mut self,
        names: &[LispString],
        symbol_names: &[u32],
        canonical: Option<&[bool]>,
    ) -> Result<RestoredDumpSymbolTable, String> {
        self.names.reserve_additional_names(names.len());
        self.symbols.reserve(symbol_names.len());
        let mut name_remap = Vec::with_capacity(names.len());
        for name in names {
            name_remap.push(self.names.intern_dump_lisp_string(name));
        }

        let derived_flags;
        let canonical = match canonical {
            Some(flags) if flags.len() == symbol_names.len() => flags,
            Some([]) => {
                derived_flags = derive_legacy_canonical_flags_from_names(names, symbol_names)?;
                &derived_flags
            }
            None => {
                derived_flags = derive_legacy_canonical_flags_from_names(names, symbol_names)?;
                &derived_flags
            }
            Some(flags) => {
                return Err(format!(
                    "pdump symbol metadata is inconsistent: {} symbols but {} canonical flags",
                    symbol_names.len(),
                    flags.len()
                ));
            }
        };

        if symbol_names.len() != canonical.len() {
            return Err(format!(
                "pdump symbol metadata is inconsistent: {} symbols but {} canonical flags",
                symbol_names.len(),
                canonical.len()
            ));
        }

        self.canonical_by_name
            .reserve(canonical.iter().filter(|&&flag| flag).count());

        let mut dump_canonical_slots: FxHashMap<NameId, usize> = FxHashMap::default();

        // Seed-prefix position map. A fresh registry holds exactly the
        // constructor's seeds (nil, t, unbound); the dump table is the full
        // dumper interner in id order, so its leading slots are the SAME
        // seeds. Interning them by name cannot reproduce that: `unbound` is
        // deliberately NON-canonical (an uninterned sentinel), so name-intern
        // allocates a fresh id and every later slot shifts by one — the
        // NEOVM_PDUMP_REMAP_AUDIT measured a uniform +1 over 18,390 slots.
        // Mapping a dump slot to the LIVE registry slot of the same position
        // when name and canonicality match (validated against the registry,
        // not a compile-time list, so a pre-load unintern of nil/t drops to
        // the lenient path) makes the whole remap identity by construction on
        // a fresh registry, which is what lets baked symbol words skip the
        // 127K-entry fixup walk. Non-matching prefixes (hand-built test
        // tables, legacy images) fall through per-slot to the ordinary path.
        let seed_prefix: Vec<(NameId, bool)> = self
            .symbols
            .iter()
            .take(SEED_SYMBOL_COUNT)
            .map(|live| (live.name, live.canonical))
            .collect();
        let seed_position_map =
            |slot: usize, runtime_name: NameId, is_canonical: bool| -> Option<SymId> {
                let (live_name, live_canonical) = *seed_prefix.get(slot)?;
                (live_name == runtime_name && live_canonical == is_canonical)
                    .then(|| SymId(slot as u32))
            };

        let symbol_remap = symbol_names
            .iter()
            .copied()
            .zip(canonical.iter().copied())
            .enumerate()
            .map(|(slot, (dump_name_id, is_canonical))| {
                let runtime_name = name_remap
                    .get(dump_name_id as usize)
                    .copied()
                    .ok_or_else(|| {
                        format!(
                            "pdump symbol metadata is inconsistent: symbol name id {} out of range for {} names",
                            dump_name_id,
                            names.len()
                        )
                    })?;
                if let Some(seed_id) = seed_position_map(slot, runtime_name, is_canonical) {
                    // Keep the duplicate-canonical rejection intact for the
                    // prefix: a malformed image with a second canonical
                    // nil/t must still hard-error, not last-wins clobber.
                    if is_canonical
                        && let Some(previous_slot) =
                            dump_canonical_slots.insert(runtime_name, slot)
                    {
                        return Err(format!(
                            "pdump symbol metadata is inconsistent: canonical symbol slots {} and {} both name {}",
                            previous_slot,
                            slot,
                            self.names.resolve(runtime_name)
                        ));
                    }
                    return Ok(seed_id);
                }
                if is_canonical {
                    if let Some(previous_slot) = dump_canonical_slots.insert(runtime_name, slot) {
                        return Err(format!(
                            "pdump symbol metadata is inconsistent: canonical symbol slots {} and {} both name {}",
                            previous_slot,
                            slot,
                            self.names.resolve(runtime_name)
                        ));
                    }
                    Ok::<SymId, String>(
                        self.canonical_by_name
                            .get(&runtime_name)
                            .copied()
                            .unwrap_or_else(|| {
                                self.alloc_symbol(runtime_name, true, NewSymbolName::AtomOnly)
                            }),
                    )
                } else {
                    Ok::<SymId, String>(self.alloc_symbol(
                        runtime_name,
                        false,
                        NewSymbolName::AtomOnly,
                    ))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(RestoredDumpSymbolTable {
            names: name_remap,
            symbols: symbol_remap,
        })
    }

    #[inline]
    fn canonical_symbol_for_name(&self, name: NameId) -> Option<SymId> {
        self.canonical_by_name.get(&name).copied()
    }

    fn unintern_canonical_symbol(&mut self, id: SymId) -> bool {
        let Some(slot) = self.symbols.get_mut(id.0 as usize) else {
            return false;
        };
        if !slot.canonical {
            return false;
        }
        if self.canonical_by_name.get(&slot.name).copied() != Some(id) {
            return false;
        }
        self.canonical_by_name.remove(&slot.name);
        slot.canonical = false;
        true
    }

    fn collect_name_value_roots(&self, roots: &mut Vec<TaggedValue>, heap_id: usize) {
        let heap_id = SymbolNameHeapId(heap_id);
        // Cross-check the identity set against the authoritative per-symbol
        // metadata in debug tests. The production root walk stays O(unique
        // name objects), independent of how many symbols share each object.
        #[cfg(all(test, debug_assertions))]
        {
            let mut unique = FxHashMap::default();
            for name_value in self
                .name_values
                .values()
                .filter(|name_value| name_value.heap_id == heap_id)
            {
                unique.insert(SymbolNameObjectId::of(name_value.value), name_value.value);
            }
            for value in self
                .materialized_name_values
                .iter()
                .filter_map(|(key, value)| (key.heap_id == heap_id).then_some(value))
            {
                unique.insert(SymbolNameObjectId::of(*value), *value);
            }
            assert_eq!(
                self.name_value_roots.root_count(heap_id),
                unique.len(),
                "per-heap name-object root index diverged"
            );
        }
        self.name_value_roots.extend_roots(roots, heap_id);
    }
}

// Dumped Lisp strings are immutable for this pass; their GC-aware interior
// mutability does not invalidate the temporary content-keyed map.
#[allow(clippy::mutable_key_type)]
fn derive_legacy_canonical_flags_from_names(
    names: &[LispString],
    symbol_names: &[u32],
) -> Result<Vec<bool>, String> {
    let mut seen = FxHashMap::default();
    symbol_names
        .iter()
        .copied()
        .map(|dump_name_id| {
            let name = names.get(dump_name_id as usize).ok_or_else(|| {
                format!(
                    "pdump symbol metadata is inconsistent: symbol name id {} out of range for {} names",
                    dump_name_id,
                    names.len()
                )
            })?;
            Ok(seen.insert(name.clone(), ()).is_none())
        })
        .collect()
}

fn global_symbol_registry() -> &'static RwLock<SymbolRegistry> {
    static GLOBAL_SYMBOL_REGISTRY: OnceLock<RwLock<SymbolRegistry>> = OnceLock::new();
    GLOBAL_SYMBOL_REGISTRY.get_or_init(|| RwLock::new(SymbolRegistry::new()))
}

fn symbol_registry_epoch() -> &'static AtomicU64 {
    static SYMBOL_REGISTRY_EPOCH: AtomicU64 = AtomicU64::new(0);
    &SYMBOL_REGISTRY_EPOCH
}

pub(crate) fn dump_runtime_interner() -> DumpedSymbolTable {
    let registry = global_symbol_registry().read();
    registry.dump_symbol_table()
}

pub(crate) fn restore_runtime_interner(
    names: &[LispString],
    symbol_names: &[u32],
    canonical: Option<&[bool]>,
) -> Result<RestoredDumpSymbolTable, String> {
    let mut registry = global_symbol_registry().write();
    registry.restore_dump_symbol_table(names, symbol_names, canonical)
}

/// Intern a string using the global runtime symbol registry.
#[inline]
pub fn intern(s: &str) -> SymId {
    #[cfg(test)]
    {
        INTERN_CALLS.set(INTERN_CALLS.get() + 1);
        INTERN_CALL_NAMES.with(|names| names.borrow_mut().push(s.to_owned()));
    }
    ensure_thread_local_cache_epoch_current();
    if let Some(sym_id) = thread_local_interned_str(s) {
        return sym_id;
    }
    let mut registry = global_symbol_registry().write();
    let sym_id = registry.intern(s);
    let canonical_name = registry.resolve(sym_id);
    drop(registry);
    thread_local_record_interned_str(canonical_name, sym_id);
    sym_id
}

/// Intern an exact Lisp-string name using the global runtime symbol registry.
#[inline]
pub fn intern_lisp_string(s: &LispString) -> SymId {
    let mut registry = global_symbol_registry().write();
    registry.intern_lisp_string(s)
}

/// Create an uninterned symbol using the global runtime symbol registry.
/// Always creates a new unique SymId, never reuses an existing one.
#[inline]
pub fn intern_uninterned(s: &str) -> SymId {
    let mut registry = global_symbol_registry().write();
    registry.intern_uninterned(s)
}

/// Create an uninterned symbol using an exact Lisp-string name.
#[inline]
pub fn intern_uninterned_lisp_string(s: &LispString) -> SymId {
    let mut registry = global_symbol_registry().write();
    registry.intern_uninterned_lisp_string(s)
}

/// Intern NAME_VALUE in the global obarray, adopting it as the new symbol's
/// exact name object when this call creates the symbol, matching GNU `intern`.
///
/// Prefer this over [`intern_lisp_string`] wherever the caller holds the Lisp
/// string OBJECT a symbol is being named from: `intern_lisp_string` keeps only
/// the name atom, which drops the string's text properties and its
/// multibyteness.
#[inline]
pub fn intern_lisp_value(name_value: TaggedValue) -> SymId {
    let mut registry = global_symbol_registry().write();
    registry.intern_lisp_value(name_value)
}

/// Create an uninterned symbol that stores NAME_VALUE as its exact name
/// object, matching GNU `make-symbol`.
#[inline]
pub fn make_uninterned_symbol_with_name_value(name_value: TaggedValue) -> SymId {
    let mut registry = global_symbol_registry().write();
    registry.make_uninterned_symbol_with_name_value(name_value)
}

/// Look up the canonical interned symbol id for a string without interning it.
#[inline]
pub fn lookup_interned(s: &str) -> Option<SymId> {
    let registry = global_symbol_registry().read();
    registry.lookup(s)
}

#[inline]
pub fn lookup_interned_lisp_string(s: &LispString) -> Option<SymId> {
    let registry = global_symbol_registry().read();
    registry.lookup_lisp_string(s)
}

#[inline]
pub fn is_canonical_id(id: SymId) -> bool {
    ensure_thread_local_cache_epoch_current();
    if let Some(is_canonical) = thread_local_is_canonical(id) {
        return is_canonical;
    }
    let registry = global_symbol_registry().read();
    let is_canonical = registry.is_canonical_id(id);
    drop(registry);
    thread_local_record_canonical(id, is_canonical);
    is_canonical
}

#[inline]
pub(crate) fn is_keyword_id(id: SymId) -> bool {
    if let Some(is_keyword) = thread_local_keyword(id) {
        return is_keyword;
    }
    let registry = global_symbol_registry().read();
    let is_keyword = registry
        .slot(id)
        .map(|slot| {
            slot.canonical
                && registry
                    .names
                    .resolve_lisp_string(slot.name)
                    .as_bytes()
                    .first()
                    .is_some_and(|byte| *byte == b':')
        })
        .unwrap_or(false);
    drop(registry);
    thread_local_record_keyword(id, is_keyword);
    is_keyword
}

#[inline]
pub fn resolve_sym_metadata(id: SymId) -> (&'static str, bool) {
    ensure_thread_local_cache_epoch_current();
    if let Some(is_canonical) = thread_local_is_canonical(id)
        && is_canonical
        && let Some(name) = thread_local_resolve(id)
    {
        return (name, true);
    }
    let registry = global_symbol_registry().read();
    let name_value = registry.resolve_lisp_string(id);
    let name = name_value
        .as_utf8_str()
        .unwrap_or_else(|| panic!("symbol name {:?} is not valid UTF-8", id));
    let is_canonical = registry.is_canonical_id(id);
    drop(registry);
    thread_local_record_canonical(id, is_canonical);
    if is_canonical {
        thread_local_record_name(id, name_value);
    }
    (name, is_canonical)
}

#[inline]
pub(crate) fn symbol_name_id(id: SymId) -> NameId {
    if let Some(name_id) = thread_local_name_id(id) {
        return name_id;
    }
    let registry = global_symbol_registry().read();
    let name_id = registry.name_id(id);
    drop(registry);
    thread_local_record_name_id(id, name_id);
    name_id
}

#[inline]
pub(crate) fn resolve_name(id: NameId) -> &'static str {
    let registry = global_symbol_registry().read();
    registry.resolve_name(id)
}

#[inline]
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn resolve_name_lisp_string(id: NameId) -> &'static LispString {
    let registry = global_symbol_registry().read();
    registry.resolve_name_lisp_string(id)
}

#[inline]
pub(crate) fn canonical_symbol_for_name(id: NameId) -> Option<SymId> {
    ensure_thread_local_cache_epoch_current();
    if let Some(sym_id) = thread_local_canonical_symbol_for_name(id) {
        return Some(sym_id);
    }
    let registry = global_symbol_registry().read();
    let sym_id = registry.canonical_symbol_for_name(id)?;
    drop(registry);
    thread_local_record_canonical_symbol_for_name(id, sym_id);
    Some(sym_id)
}

/// Resolve a SymId to its string using the global runtime symbol registry.
#[inline]
pub fn resolve_sym(id: SymId) -> &'static str {
    ensure_thread_local_cache_epoch_current();
    if let Some(is_canonical) = thread_local_is_canonical(id)
        && is_canonical
        && let Some(s) = thread_local_resolve(id)
    {
        return s;
    }
    let registry = global_symbol_registry().read();
    let name_value = registry.resolve_lisp_string(id);
    let s = name_value
        .as_utf8_str()
        .unwrap_or_else(|| panic!("symbol name {:?} is not valid UTF-8", id));
    let is_canonical = registry.is_canonical_id(id);
    drop(registry);
    thread_local_record_canonical(id, is_canonical);
    if is_canonical {
        thread_local_record_name(id, name_value);
    }
    s
}

/// Resolve a SymId to its exact Lisp-string name using the global runtime
/// symbol registry.
#[inline]
pub fn resolve_sym_lisp_string(id: SymId) -> &'static LispString {
    ensure_thread_local_cache_epoch_current();
    if let Some(name) = thread_local_resolve_lisp_string(id) {
        return name;
    }
    #[cfg(test)]
    note_resolve_sym_lisp_string_registry_read();
    let registry = global_symbol_registry().read();
    let name = registry.resolve_lisp_string(id);
    drop(registry);
    thread_local_record_name(id, name);
    name
}

/// Remove ID from the process-global canonical name map.
///
/// GNU `unintern` unlinks a symbol object from the obarray and marks that
/// object uninterned; a later `intern` of the same name allocates a distinct
/// symbol.  Neomacs keeps symbol objects in a process-global registry, so the
/// obarray layer must explicitly remove the registry's canonical name mapping
/// when the initial obarray uninterns a canonical symbol.
pub(crate) fn unintern_canonical_id(id: SymId) -> bool {
    let changed = {
        let mut registry = global_symbol_registry().write();
        registry.unintern_canonical_symbol(id)
    };
    if changed {
        symbol_registry_epoch().fetch_add(1, Ordering::AcqRel);
        ensure_thread_local_cache_epoch_current();
    }
    changed
}

/// The string a symbol's name READS as from Lisp, with the object-vs-atom
/// distinction kept in the type: the name object the symbol was created from
/// when it has one on the current heap, else its process-lifetime name atom.
///
/// GNU has only the first case -- a symbol's name is the string object it was
/// created from -- so everything Lisp can observe follows the object:
/// `symbol-name`, printing, and obarray lookup, including through a later
/// `aset` on that string.
///
/// Do NOT cache the borrow. Unlike the process-lifetime atom from
/// [`resolve_sym_lisp_string`], a name object is an ordinary GC-managed heap
/// string owned by one heap.  DIVERGENCES.md 167: this function used to have a
/// flattened sibling, `resolve_sym_lisp_name`, that answered
/// `&'static LispString` for BOTH cases -- so the sentence directly above was
/// advice a caller could ignore silently.  It is a lifetime now: hold the
/// [`LispVisibleSymbolName`] and take [`LispVisibleSymbolName::text`] at the
/// point of use, exactly as GNU takes `SDATA` only after `s1 = SYMBOL_NAME
/// (s1)`.
#[inline]
pub(crate) fn resolve_lisp_visible_symbol_name(id: SymId) -> LispVisibleSymbolName {
    global_symbol_registry()
        .read()
        .resolve_lisp_visible_name(id)
}

/// Return the one Lisp name object for `id` in the current tagged heap.
///
/// GNU stores this object directly in `struct Lisp_Symbol`.  Neomacs keeps
/// process-global symbol identity separate from evaluator-local heaps, so an
/// atom-backed symbol acquires its object lazily and caches it under the typed
/// `(heap, symbol)` identity.  The allocation happens outside the registry
/// lock; the second lookup makes concurrent first reads converge on one
/// published object.
pub(crate) fn materialize_symbol_name_value(id: SymId) -> TaggedValue {
    let heap_id = SymbolNameHeapId(
        crate::tagged::gc::current_tagged_heap_identity()
            .expect("materializing a Lisp symbol name requires an installed tagged heap"),
    );
    let atom = {
        let registry = global_symbol_registry().read();
        if let Some(value) = registry.resolve_name_value_in_heap(id, heap_id) {
            return value;
        }
        registry.resolve_lisp_string(id)
    };

    let materialized = TaggedValue::heap_string(atom.clone());
    let mut registry = global_symbol_registry().write();
    if let Some(value) = registry.resolve_name_value_in_heap(id, heap_id) {
        return value;
    }

    let key = MaterializedSymbolNameKey {
        heap_id,
        symbol: id,
    };
    let old = registry.materialized_name_values.insert(key, materialized);
    debug_assert!(
        old.is_none(),
        "materialized symbol name replaced after lookup"
    );
    registry.name_value_roots.insert(SymbolNameValue {
        value: materialized,
        heap_id,
    });
    materialized
}

/// Map a set of Lisp-visible symbol names under one registry read lock.
///
/// GNU stores the name directly in each symbol, so an obarray scan performs no
/// global lock acquisition per candidate. Keeping the lock inside this batch
/// boundary gives Neomacs the same O(1)-lock scan shape.  The mapper writes
/// directly into the caller's final collection, so no parallel full-obarray
/// name collection is required. `LispVisibleSymbolName` carries either a
/// process-lifetime atom or the exact rooted Lisp value, never a reference into
/// the registry lock.
pub(crate) fn map_lisp_visible_symbol_names<T>(
    ids: &[SymId],
    mut map: impl FnMut(SymId, LispVisibleSymbolName) -> T,
) -> Vec<T> {
    let registry = global_symbol_registry().read();
    let mut mapped = Vec::with_capacity(ids.len());
    for &id in ids {
        mapped.push(map(id, registry.resolve_lisp_visible_name(id)));
    }
    mapped
}

#[inline]
pub fn resolve_sym_name_value(id: SymId) -> Option<TaggedValue> {
    let registry = global_symbol_registry().read();
    registry.resolve_name_value(id)
}

pub(crate) fn collect_symbol_name_gc_roots(roots: &mut Vec<TaggedValue>, heap_id: usize) {
    let registry = global_symbol_registry().read();
    registry.collect_name_value_roots(roots, heap_id);
}

// ---------------------------------------------------------------------------
// Thread-local lockless cache for SymId -> &'static str
// ---------------------------------------------------------------------------
//
// `resolve_sym` is called from many bytecode hot paths (e.g. `is_keyword`,
// debug formatting) and acquiring the global RwLock — even with parking_lot
// — is many extra atomic ops per call. Once a SymId is interned, the
// underlying interned `&'static LispString` is permanently valid, so the
// (id -> name) mapping is monotonic and stable for the lifetime of the process.

#[derive(Clone, Copy, Debug)]
struct SymbolCacheEntry {
    /// A thin pointer is enough here. Converting the interned `LispString` to
    /// `&str` on a hit avoids paying for a 16-byte fat pointer in every dense
    /// cache entry.
    name: Option<&'static LispString>,
    /// `NameId(u32::MAX)` is reserved globally and doubles as the cache-miss
    /// sentinel, avoiding the padding of `Option<NameId>`.
    name_id: u32,
    canonical: Option<bool>,
    keyword: Option<bool>,
}

impl Default for SymbolCacheEntry {
    fn default() -> Self {
        Self {
            name: None,
            name_id: SYMBOL_NAME_CACHE_MISSING,
            canonical: None,
            keyword: None,
        }
    }
}

const SYMBOL_NAME_CACHE_MISSING: u32 = u32::MAX;
// Canonical `nil` has SymId 0. Leaving its name-cache slot at zero merely
// makes that one lookup miss the cache; every other canonical SymId can be
// stored directly, which keeps this dense NameId-indexed table at four bytes
// per entry instead of eight for `Option<SymId>`.
const NAME_CANONICAL_CACHE_MISSING: u32 = NIL_SYM_ID.0;

thread_local! {
    // Cell, not RefCell: read on EVERY cached symbol query (epoch check),
    // and a u64 needs no borrow flag.
    static THREAD_CACHE_EPOCH: Cell<u64> = const { Cell::new(0) };
    static INTERN_STR_CACHE: RefCell<FxHashMap<&'static str, SymId>> = RefCell::new(FxHashMap::default());
    static SYMBOL_CACHE: RefCell<Vec<SymbolCacheEntry>> = const { RefCell::new(Vec::new()) };
    static NAME_CANONICAL_SYMBOL_CACHE: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
thread_local! {
    static RESOLVE_SYM_LISP_STRING_REGISTRY_READS: RefCell<usize> = const { RefCell::new(0) };
    static INTERN_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static INTERN_CALL_NAMES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static EXACT_SYMBOL_NAME_VALUE_PROBES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static MATERIALIZED_SYMBOL_NAME_VALUE_PROBES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn note_exact_symbol_name_value_probe() {
    EXACT_SYMBOL_NAME_VALUE_PROBES.set(EXACT_SYMBOL_NAME_VALUE_PROBES.get() + 1);
}

#[cfg(not(test))]
fn note_exact_symbol_name_value_probe() {}

#[cfg(test)]
fn note_materialized_symbol_name_value_probe() {
    MATERIALIZED_SYMBOL_NAME_VALUE_PROBES.set(MATERIALIZED_SYMBOL_NAME_VALUE_PROBES.get() + 1);
}

#[cfg(not(test))]
fn note_materialized_symbol_name_value_probe() {}

#[cfg(test)]
fn reset_symbol_name_value_probes() {
    EXACT_SYMBOL_NAME_VALUE_PROBES.set(0);
    MATERIALIZED_SYMBOL_NAME_VALUE_PROBES.set(0);
}

#[cfg(test)]
fn symbol_name_value_probes() -> (usize, usize) {
    (
        EXACT_SYMBOL_NAME_VALUE_PROBES.get(),
        MATERIALIZED_SYMBOL_NAME_VALUE_PROBES.get(),
    )
}

#[cfg(test)]
fn note_resolve_sym_lisp_string_registry_read() {
    RESOLVE_SYM_LISP_STRING_REGISTRY_READS.with(|reads| *reads.borrow_mut() += 1);
}

#[cfg(test)]
pub(crate) fn reset_resolve_sym_lisp_string_registry_reads() {
    RESOLVE_SYM_LISP_STRING_REGISTRY_READS.with(|reads| *reads.borrow_mut() = 0);
}

#[cfg(test)]
pub(crate) fn resolve_sym_lisp_string_registry_reads() -> usize {
    RESOLVE_SYM_LISP_STRING_REGISTRY_READS.with(|reads| *reads.borrow())
}

#[cfg(test)]
pub(crate) fn reset_intern_calls() {
    INTERN_CALLS.set(0);
    INTERN_CALL_NAMES.with(|names| names.borrow_mut().clear());
}

#[cfg(test)]
pub(crate) fn intern_calls() -> usize {
    INTERN_CALLS.get()
}

#[cfg(test)]
pub(crate) fn intern_call_names() -> Vec<String> {
    INTERN_CALL_NAMES.with(|names| names.borrow().clone())
}

fn ensure_thread_local_cache_epoch_current() {
    let current = symbol_registry_epoch().load(Ordering::Acquire);
    THREAD_CACHE_EPOCH.with(|epoch| {
        if epoch.get() == current {
            return;
        }
        epoch.set(current);
        INTERN_STR_CACHE.with(|cache| cache.borrow_mut().clear());
        SYMBOL_CACHE.with(|cache| cache.borrow_mut().clear());
        NAME_CANONICAL_SYMBOL_CACHE.with(|cache| cache.borrow_mut().clear());
    });
}

#[inline]
fn thread_local_interned_str(s: &str) -> Option<SymId> {
    INTERN_STR_CACHE.with(|cache| cache.borrow().get(s).copied())
}

#[inline]
fn thread_local_record_interned_str(s: &'static str, id: SymId) {
    INTERN_STR_CACHE.with(|cache| {
        cache.borrow_mut().insert(s, id);
    });
}

#[inline]
fn thread_local_resolve(id: SymId) -> Option<&'static str> {
    thread_local_resolve_lisp_string(id).map(|name| {
        // `resolve_sym` validates a name via `as_utf8_str` BEFORE
        // `thread_local_record_name` admits it, and symbol names are
        // immutable — re-running full UTF-8 validation on every cache hit
        // was the hottest single cost of symbol resolution (275M Ir of
        // `from_utf8` on a 300k-iteration string workload).
        debug_assert!(name.as_utf8_str().is_some());
        unsafe { std::str::from_utf8_unchecked(name.as_bytes()) }
    })
}

#[inline]
fn thread_local_resolve_lisp_string(id: SymId) -> Option<&'static LispString> {
    SYMBOL_CACHE.with(|cache| {
        let cache = cache.borrow();
        cache.get(id.0 as usize).and_then(|entry| entry.name)
    })
}

#[inline]
fn thread_local_record_name(id: SymId, name: &'static LispString) {
    SYMBOL_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let idx = id.0 as usize;
        if cache.len() <= idx {
            cache.resize(idx + 1, SymbolCacheEntry::default());
        }
        cache[idx].name = Some(name);
    });
}

#[inline]
fn thread_local_name_id(id: SymId) -> Option<NameId> {
    SYMBOL_CACHE.with(|cache| {
        let cache = cache.borrow();
        cache
            .get(id.0 as usize)
            .map(|entry| entry.name_id)
            .filter(|&name_id| name_id != SYMBOL_NAME_CACHE_MISSING)
            .map(NameId)
    })
}

#[inline]
fn thread_local_record_name_id(id: SymId, name_id: NameId) {
    debug_assert_ne!(name_id.0, SYMBOL_NAME_CACHE_MISSING);
    SYMBOL_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let idx = id.0 as usize;
        if cache.len() <= idx {
            cache.resize(idx + 1, SymbolCacheEntry::default());
        }
        cache[idx].name_id = name_id.0;
    });
}

#[inline]
fn thread_local_is_canonical(id: SymId) -> Option<bool> {
    SYMBOL_CACHE.with(|cache| {
        let cache = cache.borrow();
        cache.get(id.0 as usize).and_then(|entry| entry.canonical)
    })
}

#[inline]
fn thread_local_record_canonical(id: SymId, is_canonical: bool) {
    SYMBOL_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let idx = id.0 as usize;
        if cache.len() <= idx {
            cache.resize(idx + 1, SymbolCacheEntry::default());
        }
        cache[idx].canonical = Some(is_canonical);
    });
}

#[inline]
fn thread_local_canonical_symbol_for_name(id: NameId) -> Option<SymId> {
    NAME_CANONICAL_SYMBOL_CACHE.with(|cache| {
        let cache = cache.borrow();
        cache
            .get(id.0 as usize)
            .copied()
            .filter(|&sym_id| sym_id != NAME_CANONICAL_CACHE_MISSING)
            .map(SymId)
    })
}

#[inline]
fn thread_local_record_canonical_symbol_for_name(id: NameId, sym_id: SymId) {
    NAME_CANONICAL_SYMBOL_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let idx = id.0 as usize;
        if cache.len() <= idx {
            cache.resize(idx + 1, NAME_CANONICAL_CACHE_MISSING);
        }
        cache[idx] = sym_id.0;
    });
}

#[inline]
fn thread_local_keyword(id: SymId) -> Option<bool> {
    SYMBOL_CACHE.with(|cache| {
        let cache = cache.borrow();
        cache.get(id.0 as usize).and_then(|entry| entry.keyword)
    })
}

#[inline]
fn thread_local_record_keyword(id: SymId, is_keyword: bool) {
    SYMBOL_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let idx = id.0 as usize;
        if cache.len() <= idx {
            cache.resize(idx + 1, SymbolCacheEntry::default());
        }
        cache[idx].keyword = Some(is_keyword);
    });
}

/// Resolve a SymId to its string using the global runtime symbol registry.
///
/// Returns `None` if the id is outside the current symbol range instead of
/// panicking. This is useful at serialization boundaries where we want a
/// structured error instead of aborting the process on malformed runtime data.
#[inline]
pub fn try_resolve_sym(id: SymId) -> Option<&'static str> {
    let registry = global_symbol_registry().read();
    registry
        .slot(id)
        .map(|slot| registry.names.resolve(slot.name))
}

#[inline]
pub fn try_resolve_sym_lisp_string(id: SymId) -> Option<&'static LispString> {
    let registry = global_symbol_registry().read();
    registry
        .slot(id)
        .map(|slot| registry.names.resolve_lisp_string(slot.name))
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/seed_prefix.rs"]
mod seed_prefix_tests;
