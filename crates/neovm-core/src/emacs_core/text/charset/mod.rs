//! Charset builtins for the Elisp interpreter.
//!
//! Charsets in Emacs define sets of characters with encoding properties.
//! For neovm we primarily support Unicode; other charsets are registered
//! for compatibility but map through to the Unicode code-point space.
//!
//! The `CharsetRegistry` stores known charset names, IDs, and plists.
//! It is initialized with the standard charsets: ascii, unicode,
//! unicode-bmp, latin-iso8859-1, emacs, and eight-bit.

use super::error::{EvalResult, Flow, signal};
use super::intern::{SymId, intern, lookup_interned, resolve_sym};
use super::value::*;
use crate::buffer::{EmacsBytePos, LispCharPos1};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

const RAW_BYTE_SENTINEL_MIN: u32 = 0xE080;
const RAW_BYTE_SENTINEL_MAX: u32 = 0xE0FF;
const UNIBYTE_BYTE_SENTINEL_MIN: u32 = 0xE300;
const UNIBYTE_BYTE_SENTINEL_MAX: u32 = 0xE3FF;

/// Cache key for a parsed charset `.map` file.  GNU's `load_charset_map`
/// converts every code point in the map to a *linear index* via
/// `CODE_POINT_TO_INDEX` (which depends on the owning charset's code-space and
/// minimum code), so the same `.map` file parsed for two charsets with
/// different code-spaces yields different code↔char tables.  The cache must
/// therefore key on the code-space identity, not just the file name.
#[derive(Clone, PartialEq, Eq, Hash)]
struct CharsetMapCacheKey {
    map_name: String,
    code_space: [i64; 8],
    min_code: i64,
}

static CHARSET_MAP_CACHE: OnceLock<
    RwLock<HashMap<CharsetMapCacheKey, Option<Arc<CharsetMapData>>>>,
> = OnceLock::new();

fn charset_map_cache() -> &'static RwLock<HashMap<CharsetMapCacheKey, Option<Arc<CharsetMapData>>>>
{
    CHARSET_MAP_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn charset_map_dir() -> PathBuf {
    // Resolve at RUNTIME, never via the compile-time `env!("CARGO_MANIFEST_DIR")`:
    // that path is the build machine's source tree, which is absent in an
    // installed release, so every charset-map load (e.g. `make-char
    // 'latin-jisx0201` from kinsoku.el during normal startup) would silently
    // fail -> `decode-char` returns nil -> "Invalid code(s)".
    // `charset_map_directory()` resolves under the install data dir
    // (`<runtime_root>/etc/charsets`), the neomacs equivalent of GNU's
    // `charset-map-path`. Memoized: the runtime root does not change mid-process.
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(super::load::charset_map_directory).clone()
}

fn parse_hex_i64(value: &str) -> Option<i64> {
    i64::from_str_radix(value.trim().trim_start_matches("0x"), 16).ok()
}

fn parse_hex_range(value: &str) -> Option<(i64, i64)> {
    let value = value.trim();
    if let Some((from, to)) = value.split_once('-') {
        Some((parse_hex_i64(from)?, parse_hex_i64(to)?))
    } else {
        let code = parse_hex_i64(value)?;
        Some((code, code))
    }
}

fn parse_charset_map_file(path: &Path, info: &CharsetInfo) -> Option<CharsetMapData> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut code_to_char = HashMap::new();
    let mut char_to_code = HashMap::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let source = fields.next()?;
        let target = fields.next()?;
        let (from, to) = parse_hex_range(source)?;
        let from_char = parse_hex_i64(target)?;
        // GNU `load_charset_map` (src/charset.c): a map entry "FROM-TO C" maps
        // the *code-point* range [FROM, TO] to consecutive characters starting
        // at C, but the stepping follows the charset's code-space — i.e. it is
        // linear in the CODE INDEX, not in the raw 32-bit code point.  For a
        // multi-dimensional code-space (e.g. the gb18030 4-byte charsets, whose
        // valid bytes are 0x30-0x39 / 0x81-0xFE) most raw integers between FROM
        // and TO are NOT valid code points.  So convert both ends to indices,
        // then walk index by index, materializing each real code point via
        // INDEX_TO_CODE_POINT.  (The old code iterated raw integers, which
        // produced bogus code↔char pairs for every 4-byte charset.)
        let from_index = charset_code_point_to_index(info, from)?;
        let to_index = if from == to {
            from_index
        } else {
            charset_code_point_to_index(info, to)?
        };
        if from_index < 0 || to_index < 0 || to_index < from_index {
            continue;
        }
        for offset in 0..=(to_index - from_index) {
            let index = from_index + offset;
            let Some(code) = charset_index_to_code_point(info, index) else {
                continue;
            };
            let ch = from_char + offset;
            code_to_char.insert(code, ch);
            char_to_code.entry(ch).or_insert(code);
        }
    }

    Some(CharsetMapData {
        code_to_char,
        char_to_code,
    })
}

/// Load (and cache) the code↔char tables of a charset `.map` file.  The owning
/// `info` supplies the code-space used to convert the map's code points to
/// linear indices (GNU `CODE_POINT_TO_INDEX`), so it is part of the cache key.
fn load_charset_map(map_name: &str, info: &CharsetInfo) -> Option<Arc<CharsetMapData>> {
    let key = CharsetMapCacheKey {
        map_name: map_name.to_string(),
        code_space: info.code_space,
        min_code: info.min_code,
    };
    if let Ok(cache) = charset_map_cache().read()
        && let Some(cached) = cache.get(&key)
    {
        return cached.clone();
    }

    let loaded = parse_charset_map_file(&charset_map_dir().join(format!("{map_name}.map")), info)
        .map(Arc::new);
    if let Ok(mut cache) = charset_map_cache().write() {
        cache.insert(key, loaded.clone());
    }
    loaded
}

// ---------------------------------------------------------------------------
// Charset data types
// ---------------------------------------------------------------------------

/// How a charset maps code points to characters.
#[derive(Clone, Debug)]
enum CharsetMethod {
    /// code → code + offset (most common, e.g. ASCII, latin-iso8859-1)
    Offset(i64),
    /// Explicit mapping table backed by an Emacs `.map` file.
    Map(String),
    /// Subset of another charset
    Subset(CharsetSubsetSpec),
    /// Superset of other charsets
    Superset(Vec<(SymId, i64)>),
}

#[derive(Clone, Debug)]
pub(crate) enum CharsetMethodSnapshot {
    Offset(i64),
    Map(String),
    Subset(CharsetSubsetSpecSnapshot),
    Superset(Vec<(SymId, i64)>),
}

#[derive(Clone, Debug)]
struct CharsetMapData {
    code_to_char: HashMap<i64, i64>,
    char_to_code: HashMap<i64, i64>,
}

#[derive(Clone, Debug)]
pub(crate) struct CharsetSubsetSpec {
    pub parent: SymId,
    pub parent_min_code: i64,
    pub parent_max_code: i64,
    pub offset: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct CharsetSubsetSpecSnapshot {
    pub parent: SymId,
    pub parent_min_code: i64,
    pub parent_max_code: i64,
    pub offset: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct CharsetInfoSnapshot {
    pub id: i64,
    pub name: SymId,
    pub dimension: i64,
    pub code_space: [i64; 8],
    pub min_code: i64,
    pub max_code: i64,
    pub iso_final_char: Option<i64>,
    pub iso_revision: Option<i64>,
    pub emacs_mule_id: Option<i64>,
    pub ascii_compatible_p: bool,
    pub supplementary_p: bool,
    pub unified_p: bool,
    pub invalid_code: Option<i64>,
    pub unify_map: Value,
    pub method: CharsetMethodSnapshot,
    pub plist: Vec<(SymId, Value)>,
}

#[derive(Clone, Debug)]
pub(crate) struct CharsetRegistrySnapshot {
    pub charsets: Vec<CharsetInfoSnapshot>,
    pub priority: Vec<SymId>,
    pub next_id: i64,
    /// Index into `priority` of the first non-preferred charset, mirroring GNU's
    /// `Vcharset_non_preferred_head` (charset.c:85). `char_charset` returns the
    /// `unicode` parent charset once a Unicode character crosses this boundary
    /// without matching a preferred charset. `None` means the boundary is the
    /// end of the list (GNU's `Qnil`).
    pub non_preferred_head: Option<usize>,
}

/// Information about a single charset.
#[derive(Clone, Debug)]
struct CharsetInfo {
    id: i64,
    name: SymId,
    dimension: i64,
    code_space: [i64; 8],
    min_code: i64,
    max_code: i64,
    iso_final_char: Option<i64>,
    iso_revision: Option<i64>,
    emacs_mule_id: Option<i64>,
    ascii_compatible_p: bool,
    supplementary_p: bool,
    unified_p: bool,
    invalid_code: Option<i64>,
    unify_map: Value,
    method: CharsetMethod,
    plist: Vec<(SymId, Value)>,
}

/// Registry of known charsets, keyed by name.
pub(crate) struct CharsetRegistry {
    charsets: HashMap<SymId, CharsetInfo>,
    /// Aliases mapping an alias name to its canonical charset name. Aliases
    /// resolve to the target dynamically (like GNU `define-charset-alias`), so
    /// later mutations of the target's plist (e.g. characters.el setting
    /// `preferred-coding-system`) are visible through the alias.
    aliases: HashMap<SymId, SymId>,
    /// Priority-ordered list of charset names.
    priority: Vec<SymId>,
    /// Index into `priority` of the first *non-preferred* charset, mirroring
    /// GNU's `Vcharset_non_preferred_head` (charset.c:85). When `char_charset`
    /// walks `priority` for a Unicode character (`<= MAX_UNICODE_CHAR`) and
    /// crosses this boundary without a match, it returns the `unicode` parent
    /// charset (charset.c:1990-1993) instead of continuing into the
    /// non-preferred subset charsets (`unicode-bmp`, `latin-iso8859-1`, ...).
    /// `None` means the boundary is the end of the list (GNU's `Qnil`).
    non_preferred_head: Option<usize>,
    /// Next auto-assigned charset ID.
    next_id: i64,
}

impl CharsetRegistry {
    /// Create a new registry pre-populated with the standard charsets.
    pub fn new() -> Self {
        let mut reg = Self {
            charsets: HashMap::new(),
            aliases: HashMap::new(),
            priority: Vec::new(),
            non_preferred_head: None,
            next_id: 256, // start above the Emacs built-in range
        };
        reg.init_standard_charsets();
        // GNU's dumped Emacs has only `ascii` preferred by default (the locale
        // setup runs `set-charset-priority` with the ASCII-first list), so a
        // freshly started session classifies non-ASCII BMP characters as the
        // `unicode` parent charset, never the non-preferred `unicode-bmp`
        // subset (`(char-charset ?é)` => `unicode`). Seed the boundary to match:
        // index 1 == everything after `ascii` (priority[0]) is non-preferred.
        // `set-language-environment` / `set-charset-priority` override this at
        // runtime (e.g. the UTF-8 environment moves `unicode-bmp` to the front).
        reg.non_preferred_head = Some(1);
        reg
    }

    /// Resolve a charset name through any alias chain to its canonical name.
    fn resolve_name(&self, name: SymId) -> SymId {
        let mut current = name;
        // Aliases form a short chain in practice; cap iterations defensively.
        for _ in 0..8 {
            match self.aliases.get(&current) {
                Some(&target) if target != current => current = target,
                _ => break,
            }
        }
        current
    }

    fn make_default(id: i64, name: &str) -> CharsetInfo {
        CharsetInfo {
            id,
            name: intern(name),
            dimension: 1,
            code_space: [0, 127, 0, 0, 0, 0, 0, 0],
            min_code: 0,
            max_code: 127,
            iso_final_char: None,
            iso_revision: None,
            emacs_mule_id: None,
            ascii_compatible_p: false,
            supplementary_p: false,
            unified_p: false,
            invalid_code: None,
            unify_map: Value::NIL,
            method: CharsetMethod::Offset(0),
            plist: vec![],
        }
    }

    fn init_standard_charsets(&mut self) {
        // The five C-level charsets are defined by GNU's `init_charset_once`
        // (charset.c:2444-2468) in this exact order — ascii, iso-8859-1,
        // unicode, emacs, eight-bit — and each is appended to
        // `Vcharset_ordered_list`.  `emacs` and `eight-bit` are supplementary
        // (the 2nd-to-last argument to `define_charset_internal` is 1), so they
        // accrue at the tail of the ordered list.  Registering them here in the
        // same order seeds the priority list to match GNU before mule-conf.el
        // defines the remaining ~174 Lisp charsets.
        let mut ascii = Self::make_default(0, "ascii");
        ascii.ascii_compatible_p = true;
        ascii.iso_final_char = Some(66); // ISO final char 'B'
        ascii.emacs_mule_id = Some(0);
        self.register_builtin(ascii);

        // iso-8859-1 is a full 0-255 charset with identity mapping
        // (code_offset=0, min_code=0, max_code=255, ascii_compatible=true),
        // matching the built-in definition in GNU Emacs charset.c.
        // This is distinct from latin-iso8859-1 which only covers the
        // right-hand part (code points 32-127 mapping to characters 160-255).
        let mut iso_8859_1 = Self::make_default(1, "iso-8859-1");
        iso_8859_1.code_space = [0, 255, 0, 0, 0, 0, 0, 0];
        iso_8859_1.min_code = 0;
        iso_8859_1.max_code = 255;
        iso_8859_1.ascii_compatible_p = true;
        iso_8859_1.method = CharsetMethod::Offset(0);
        self.register_builtin(iso_8859_1);

        let mut unicode = Self::make_default(2, "unicode");
        unicode.dimension = 3;
        unicode.code_space = [0, 255, 0, 255, 0, 16, 0, 0];
        unicode.max_code = 0x10FFFF;
        unicode.ascii_compatible_p = true; // GNU charset-plist :ascii-compatible-p t
        self.register_builtin(unicode);

        let mut emacs = Self::make_default(3, "emacs");
        emacs.dimension = 3;
        emacs.code_space = [0, 255, 0, 255, 0, 63, 0, 0];
        emacs.max_code = 0x3FFF7F;
        emacs.ascii_compatible_p = true; // GNU charset-plist :ascii-compatible-p t
        emacs.supplementary_p = true; // GNU define_charset_internal: supplementary=1
        self.register_builtin(emacs);

        let mut eight_bit = Self::make_default(4, "eight-bit");
        eight_bit.code_space = [128, 255, 0, 0, 0, 0, 0, 0];
        eight_bit.min_code = 128;
        eight_bit.max_code = 255;
        eight_bit.supplementary_p = true;
        eight_bit.method = CharsetMethod::Offset(0x3FFF80);
        self.register_builtin(eight_bit);

        // unicode-bmp and latin-iso8859-1 are *Lisp* charsets in GNU
        // (mule-conf.el).  neomacs pre-seeds them so the runtime can use them
        // before loadup, but they must enter the ordered list only when
        // mule-conf.el's `define-charset` runs (at GNU's position), so register
        // them WITHOUT adding to the ordered list here.
        let mut bmp = Self::make_default(144, "unicode-bmp");
        bmp.dimension = 2;
        bmp.code_space = [0, 255, 0, 255, 0, 0, 0, 0];
        bmp.max_code = 0xFFFF;
        self.register_builtin_preseed(bmp);

        let mut latin1 = Self::make_default(5, "latin-iso8859-1");
        latin1.code_space = [32, 127, 0, 0, 0, 0, 0, 0];
        latin1.min_code = 32;
        latin1.method = CharsetMethod::Offset(160);
        latin1.iso_final_char = Some(65); // ISO final char 'A'
        latin1.emacs_mule_id = Some(129);
        self.register_builtin_preseed(latin1);

        self.define_alias(intern("ucs"), intern("unicode"));
    }

    /// Register a built-in (C-level) charset, giving it GNU's canonical
    /// charset plist before insertion.  GNU defines these charsets in C via
    /// `define_charset_internal` (charset.c:1268), which builds the plist as
    /// `:name :dimension :code-space :iso-final-char :emacs-mule-id
    /// :ascii-compatible-p :code-offset` in that order; mule-conf.el then
    /// appends `:docstring`/`:short-name`/`:long-name` via put-charset-property.
    /// Charsets created from Lisp `define-charset` keep the plist they were
    /// defined with, so they go through the plain `register`.
    fn register_builtin(&mut self, info: CharsetInfo) {
        self.register_builtin_impl(info, true);
    }

    /// Like `register_builtin`, but pre-seeds the charset without adding it to
    /// the ordered/priority list (it is added when mule-conf.el redefines it).
    fn register_builtin_preseed(&mut self, info: CharsetInfo) {
        self.register_builtin_impl(info, false);
    }

    fn register_builtin_impl(&mut self, mut info: CharsetInfo, add_ordered: bool) {
        // The canonical plist holds Lisp `Value`s (the `:code-space` vector
        // allocates on the tagged heap). `CharsetRegistry` is a thread-local
        // lazily built on whatever thread first needs it — including the GUI
        // render thread, which does font/char matching through the registry's
        // plain fields but has no VM heap. Only materialize the Lisp plist when
        // a heap is available (always so in tests via the fallback heap, and on
        // the Lisp thread); the render thread keeps the fields and an empty
        // plist (it never reads `charset-plist`). Without this guard,
        // `Value::vector` panics with "no TaggedHeap set for this thread".
        let heap_available =
            cfg!(test) || crate::tagged::gc::current_tagged_heap_identity().is_some();
        if info.plist.is_empty() && heap_available {
            let code_offset = match info.method {
                CharsetMethod::Offset(n) => n,
                _ => 0,
            };
            let code_space = info
                .code_space
                .iter()
                .map(|&n| Value::fixnum(n))
                .collect::<Vec<_>>();
            info.plist = vec![
                (intern(":name"), Value::from_sym_id(info.name)),
                (intern(":dimension"), Value::fixnum(info.dimension)),
                (intern(":code-space"), Value::vector(code_space)),
                (
                    intern(":iso-final-char"),
                    info.iso_final_char.map_or(Value::NIL, Value::fixnum),
                ),
                (
                    intern(":emacs-mule-id"),
                    info.emacs_mule_id.map_or(Value::NIL, Value::fixnum),
                ),
                (
                    intern(":ascii-compatible-p"),
                    if info.ascii_compatible_p {
                        Value::T
                    } else {
                        Value::NIL
                    },
                ),
                (intern(":code-offset"), Value::fixnum(code_offset)),
            ];
        }
        if add_ordered {
            self.register(info);
        } else {
            self.register_preseed(info);
        }
    }

    fn register(&mut self, info: CharsetInfo) {
        self.register_inner(info, true);
    }

    /// Insert a charset into the `charsets` map WITHOUT touching the ordered
    /// (priority) list.  Used to pre-seed charsets that the runtime needs
    /// before loadup but that mule-conf.el *redefines* (latin-iso8859-1,
    /// unicode-bmp): their ordered-list slot must come from that later Lisp
    /// `define-charset`, at GNU's position, not from the pre-seed.
    fn register_preseed(&mut self, info: CharsetInfo) {
        self.register_inner(info, false);
    }

    fn register_inner(&mut self, mut info: CharsetInfo, add_ordered: bool) {
        // Ensure the plist includes :dimension so that Elisp
        // charset-dimension (which reads the plist) returns a value.
        // GNU define-charset includes :dimension in the plist
        // (charset.c:1269-1273).  Builtin charsets start with an
        // empty plist; dynamic charsets from define-charset already
        // include it — only push if absent.
        let dim_sym = intern(":dimension");
        if !info.plist.iter().any(|(k, _)| *k == dim_sym) {
            info.plist.push((dim_sym, Value::fixnum(info.dimension)));
        }
        let name = info.name;
        let supplementary_p = info.supplementary_p;
        self.charsets.insert(name, info);
        // GNU's `Fdefine_charset_internal` appends every NEWLY-defined charset
        // to `Vcharset_ordered_list` (charset.c:1191-1219).  This is what makes
        // `charset-priority-list` return the full registered set in definition
        // order.  Only add a charset the first time it enters the ordered list:
        // some charsets (latin-iso8859-1, unicode-bmp) are pre-seeded in the
        // `charsets` map but are *redefined* from mule-conf.el — their first
        // appearance in the ordered list is at that (re)definition, matching
        // GNU where they are pure Lisp charsets.
        if add_ordered {
            let resolved = self.resolve_name(name);
            if !self.priority.contains(&resolved) {
                self.add_to_ordered(resolved, supplementary_p);
            }
        }
    }

    /// Insert NAME into the priority/ordered list following GNU's
    /// `Fdefine_charset_internal` rule (charset.c:1191-1219):
    /// - supplementary charsets are appended to the end;
    /// - non-supplementary charsets are inserted just before the first
    ///   supplementary charset (or at the end if there is no supplementary
    ///   charset yet).
    fn add_to_ordered(&mut self, name: SymId, supplementary_p: bool) {
        if supplementary_p {
            self.priority.push(name);
            return;
        }
        // Find the position of the first supplementary charset.
        let first_supp = self.priority.iter().position(|&id| {
            self.charsets
                .get(&id)
                .map(|cs| cs.supplementary_p)
                .unwrap_or(false)
        });
        match first_supp {
            Some(idx) => self.priority.insert(idx, name),
            None => self.priority.push(name),
        }
    }

    /// Allocate the next auto-incrementing charset ID.
    fn alloc_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Return true if a charset with the given name exists.
    pub fn contains(&self, name: &str) -> bool {
        lookup_interned(name).is_some_and(|id| self.contains_symbol(id))
    }

    pub fn contains_symbol(&self, name: SymId) -> bool {
        self.charsets.contains_key(&self.resolve_name(name))
    }

    /// Return the list of all charset names (unordered).
    #[cfg(test)]
    pub fn names(&self) -> Vec<String> {
        self.charsets
            .keys()
            .map(|name| resolve_sym(*name).to_string())
            .collect()
    }

    /// Return the priority-ordered list of charset names.
    pub fn priority_list(&self) -> &[SymId] {
        &self.priority
    }

    /// Classify a character into the highest-priority charset that contains it,
    /// mirroring GNU `char_charset` (charset.c:1968-1998) walking
    /// `Vcharset_ordered_list`.
    ///
    /// ASCII (`< 0x80`) short-circuits to `ascii` (GNU `CHAR_CHARSET`,
    /// charset.h:404). Otherwise the charsets are tried in priority order and
    /// the first whose `ENCODE_CHAR` succeeds wins. When a Unicode character
    /// (`<= MAX_UNICODE_CHAR`) crosses the `non_preferred_head` boundary
    /// without a match, GNU returns the dimension-3 `unicode` parent charset
    /// rather than descend into the non-preferred subsets such as
    /// `unicode-bmp`/`latin-iso8859-1` (charset.c:1990-1993). Characters past
    /// Unicode fall back to `emacs` (`<= MAX_5_BYTE_CHAR`) or `eight-bit`.
    ///
    /// This is what makes `(char-charset ?é)` follow the active charset
    /// priority: `unicode` under the default ASCII-only priority, but
    /// `unicode-bmp` once `set-language-environment "UTF-8"` moves the BMP
    /// subset to the front.
    pub(crate) fn classify_char(&self, ch: i64) -> SymId {
        const MAX_UNICODE_CHAR: i64 = 0x10_FFFF;
        const MAX_5_BYTE_CHAR: i64 = 0x3F_FF7F;

        if (0..=0x7F).contains(&ch) {
            return intern("ascii");
        }

        for (i, &charset) in self.priority.iter().enumerate() {
            if self.encode_char(charset, ch).is_some() {
                return charset;
            }
            // GNU advances past `priority[i]`; before examining the next
            // charset it returns `unicode` when that next slot begins the
            // non-preferred region and `ch` is within Unicode.
            if ch <= MAX_UNICODE_CHAR && self.non_preferred_head == Some(i + 1) {
                return intern("unicode");
            }
        }

        // End of the ordered list (GNU's `charset_list == Qnil`): a Unicode
        // char with a `None` (== `Qnil`) boundary still resolves to `unicode`.
        if ch <= MAX_UNICODE_CHAR {
            intern("unicode")
        } else if ch <= MAX_5_BYTE_CHAR {
            intern("emacs")
        } else {
            intern("eight-bit")
        }
    }

    /// Move the requested charset names to the front of the priority list
    /// (deduplicated, preserving relative order for remaining entries).
    pub fn set_priority(&mut self, requested: &[SymId]) {
        let mut seen = HashSet::with_capacity(self.priority.len() + requested.len());
        let mut reordered = Vec::with_capacity(self.priority.len() + requested.len());

        for &name in requested {
            if seen.insert(name) {
                reordered.push(name);
            }
        }

        // GNU `Fset_charset_priority` (charset.c:2184-2196) sets
        // `Vcharset_non_preferred_head` to the old ordered list with the
        // requested charsets removed: the boundary therefore sits right after
        // the prepended preferred charsets. That count is exactly the prefix
        // built by the loop above.
        self.non_preferred_head = Some(reordered.len());

        for &name in &self.priority {
            if seen.insert(name) {
                reordered.push(name);
            }
        }

        self.priority = reordered;
    }

    /// Return the plist for a charset, or None if not found.
    pub fn plist(&self, name: SymId) -> Option<&[(SymId, Value)]> {
        self.charsets
            .get(&self.resolve_name(name))
            .map(|info| info.plist.as_slice())
    }

    /// Return the internal ID for a charset, if known.
    pub fn id(&self, name: SymId) -> Option<i64> {
        self.charsets
            .get(&self.resolve_name(name))
            .map(|info| info.id)
    }

    /// Register ALIAS as another name for TARGET.
    ///
    /// Unlike a copy, the alias resolves to the target dynamically, so any
    /// later change to the target charset (plist, unification, …) is reflected
    /// through the alias — matching GNU `define-charset-alias`, where an alias
    /// is simply another name for the same charset.
    pub fn define_alias(&mut self, alias: SymId, target: SymId) {
        let canonical = self.resolve_name(target);
        if self.charsets.contains_key(&canonical) {
            self.aliases.insert(alias, canonical);
        }
    }

    fn snapshot(&self) -> CharsetRegistrySnapshot {
        // Materialize aliases as concrete charset entries so they survive the
        // pdump (which serializes only `charsets`).  Snapshots are taken after
        // loadup, so the resolved target's plist is already final (e.g. it
        // includes `preferred-coding-system`); a materialized clone therefore
        // carries the same data the dynamic alias would resolve to.
        let alias_clones = self.aliases.iter().filter_map(|(&alias, &target)| {
            let canonical = self.resolve_name(target);
            self.charsets.get(&canonical).map(|info| {
                let mut clone = info.clone();
                clone.name = alias;
                clone
            })
        });
        let mut charsets = self
            .charsets
            .values()
            .cloned()
            .chain(alias_clones)
            .map(|info| CharsetInfoSnapshot {
                id: info.id,
                name: info.name,
                dimension: info.dimension,
                code_space: info.code_space,
                min_code: info.min_code,
                max_code: info.max_code,
                iso_final_char: info.iso_final_char,
                iso_revision: info.iso_revision,
                emacs_mule_id: info.emacs_mule_id,
                ascii_compatible_p: info.ascii_compatible_p,
                supplementary_p: info.supplementary_p,
                unified_p: info.unified_p,
                invalid_code: info.invalid_code,
                unify_map: info.unify_map,
                method: match info.method {
                    CharsetMethod::Offset(offset) => CharsetMethodSnapshot::Offset(offset),
                    CharsetMethod::Map(ref map_name) => {
                        CharsetMethodSnapshot::Map(map_name.clone())
                    }
                    CharsetMethod::Subset(ref subset) => {
                        CharsetMethodSnapshot::Subset(CharsetSubsetSpecSnapshot {
                            parent: subset.parent,
                            parent_min_code: subset.parent_min_code,
                            parent_max_code: subset.parent_max_code,
                            offset: subset.offset,
                        })
                    }
                    CharsetMethod::Superset(ref members) => {
                        CharsetMethodSnapshot::Superset(members.clone())
                    }
                },
                plist: info.plist.clone(),
            })
            .collect::<Vec<_>>();
        charsets.sort_by(|left, right| resolve_sym(left.name).cmp(resolve_sym(right.name)));

        CharsetRegistrySnapshot {
            charsets,
            priority: self.priority.clone(),
            next_id: self.next_id,
            non_preferred_head: self.non_preferred_head,
        }
    }

    fn restore(snapshot: CharsetRegistrySnapshot) -> Self {
        let mut charsets = HashMap::with_capacity(snapshot.charsets.len());
        for info in snapshot.charsets {
            let name = info.name;
            charsets.insert(
                name,
                CharsetInfo {
                    id: info.id,
                    name,
                    dimension: info.dimension,
                    code_space: info.code_space,
                    min_code: info.min_code,
                    max_code: info.max_code,
                    iso_final_char: info.iso_final_char,
                    iso_revision: info.iso_revision,
                    emacs_mule_id: info.emacs_mule_id,
                    ascii_compatible_p: info.ascii_compatible_p,
                    supplementary_p: info.supplementary_p,
                    unified_p: info.unified_p,
                    invalid_code: info.invalid_code,
                    unify_map: info.unify_map,
                    method: match info.method {
                        CharsetMethodSnapshot::Offset(offset) => CharsetMethod::Offset(offset),
                        CharsetMethodSnapshot::Map(map_name) => CharsetMethod::Map(map_name),
                        CharsetMethodSnapshot::Subset(subset) => {
                            CharsetMethod::Subset(CharsetSubsetSpec {
                                parent: subset.parent,
                                parent_min_code: subset.parent_min_code,
                                parent_max_code: subset.parent_max_code,
                                offset: subset.offset,
                            })
                        }
                        CharsetMethodSnapshot::Superset(members) => {
                            CharsetMethod::Superset(members)
                        }
                    },
                    plist: info.plist,
                },
            );
        }

        Self {
            charsets,
            // Aliases were materialized into `charsets` at snapshot time, so
            // none remain to resolve dynamically after a restore.
            aliases: HashMap::new(),
            priority: snapshot.priority,
            non_preferred_head: snapshot.non_preferred_head,
            next_id: snapshot.next_id,
        }
    }

    /// Replace the plist for a charset.
    pub fn set_plist(&mut self, name: SymId, plist: Vec<(SymId, Value)>) {
        let name = self.resolve_name(name);
        if let Some(info) = self.charsets.get_mut(&name) {
            info.plist = plist;
        }
    }

    fn superset_members(info: &CharsetInfo) -> Vec<(SymId, i64)> {
        match &info.method {
            CharsetMethod::Superset(members) => members.clone(),
            _ => Vec::new(),
        }
    }

    /// Decode a code-point in the given charset to an Emacs internal
    /// character code.  Returns `None` when the code-point is outside
    /// the charset's valid range or the charset method cannot handle it.
    pub fn decode_char(&self, name: SymId, code_point: i64) -> Option<i64> {
        let info = self.charsets.get(&self.resolve_name(name))?;
        if info.ascii_compatible_p && (0..=0x7f).contains(&code_point) {
            return Some(code_point);
        }
        if code_point < info.min_code || code_point > info.max_code {
            return None;
        }
        if info.unified_p
            && let Some(unify_map) = charset_value_text(&info.unify_map)
            && let Some(decoded) = load_charset_map(&unify_map, info)
                .and_then(|map| map.code_to_char.get(&code_point).copied())
        {
            return Some(decoded);
        }
        match &info.method {
            CharsetMethod::Offset(offset) => {
                charset_code_point_to_index(info, code_point).map(|index| index + offset)
            }
            CharsetMethod::Map(map_name) => load_charset_map(map_name, info)
                .and_then(|map| map.code_to_char.get(&code_point).copied()),
            CharsetMethod::Subset(subset) => {
                let parent_code = code_point - subset.offset;
                if parent_code < subset.parent_min_code || parent_code > subset.parent_max_code {
                    None
                } else {
                    self.decode_char(subset.parent, parent_code)
                }
            }
            CharsetMethod::Superset(_) => {
                Self::superset_members(info)
                    .into_iter()
                    .find_map(|(parent_name, code_offset)| {
                        self.decode_char(parent_name, code_point - code_offset)
                    })
            }
        }
    }

    /// Encode an Emacs internal character code back to a code-point in
    /// the given charset.  Returns `None` when the character cannot be
    /// represented in the charset.
    pub fn encode_char(&self, name: SymId, ch: i64) -> Option<i64> {
        let info = self.charsets.get(&self.resolve_name(name))?;
        if info.unified_p
            && let Some(unify_map) = charset_value_text(&info.unify_map)
            && let Some(encoded) = load_charset_map(&unify_map, info)
                .and_then(|map| map.char_to_code.get(&ch).copied())
        {
            return Some(encoded);
        }
        match &info.method {
            CharsetMethod::Offset(offset) => {
                if info.ascii_compatible_p && (0..=0x7f).contains(&ch) {
                    return Some(ch);
                }
                let code_point = charset_index_to_code_point(info, ch.checked_sub(*offset)?)?;
                if code_point >= info.min_code && code_point <= info.max_code {
                    Some(code_point)
                } else {
                    None
                }
            }
            CharsetMethod::Map(map_name) => {
                load_charset_map(map_name, info).and_then(|map| map.char_to_code.get(&ch).copied())
            }
            CharsetMethod::Subset(subset) => {
                let parent_code = self.encode_char(subset.parent, ch)?;
                if parent_code < subset.parent_min_code || parent_code > subset.parent_max_code {
                    None
                } else {
                    Some(parent_code + subset.offset)
                }
            }
            CharsetMethod::Superset(_) => {
                Self::superset_members(info)
                    .into_iter()
                    .find_map(|(parent_name, code_offset)| {
                        self.encode_char(parent_name, ch)
                            .map(|code| code + code_offset)
                    })
            }
        }
    }

    /// Decode the next character of `bytes` in the charset, reading one byte
    /// per charset dimension as a big-endian code point. Returns the decoded
    /// Emacs character code and the number of bytes consumed, or `None` when
    /// there are too few bytes or the code point is not assigned in the
    /// charset (the caller then falls back to an eight-bit raw byte).
    pub fn decode_char_from_bytes(&self, name: SymId, bytes: &[u8]) -> Option<(i64, usize)> {
        let dimension = self
            .charsets
            .get(&self.resolve_name(name))
            .map(|info| info.dimension.clamp(1, 4) as usize)?;
        if bytes.len() < dimension {
            return None;
        }
        let code = bytes[..dimension]
            .iter()
            .fold(0i64, |acc, &byte| (acc << 8) | i64::from(byte));
        let ch = self.decode_char(name, code)?;
        Some((ch, dimension))
    }

    /// Encode `ch` in the charset and return the code point as its big-endian
    /// byte sequence (one byte per charset dimension) — the bytes a simple
    /// charset-based coding system emits for that character. Returns `None`
    /// when `ch` is not representable in the charset.
    pub fn encode_char_bytes(&self, name: SymId, ch: i64) -> Option<Vec<u8>> {
        let code = self.encode_char(name, ch)?;
        let dimension = self
            .charsets
            .get(&self.resolve_name(name))
            .map(|info| info.dimension.clamp(1, 4) as u32)?;
        let bytes = (0..dimension)
            .rev()
            .map(|i| ((code >> (8 * i)) & 0xFF) as u8)
            .collect();
        Some(bytes)
    }
}

// ---------------------------------------------------------------------------
// Singleton registry
// ---------------------------------------------------------------------------

use std::cell::RefCell;

thread_local! {
    static CHARSET_REGISTRY: RefCell<CharsetRegistry> = RefCell::new(CharsetRegistry::new());
}

/// Reset charset registry to default state (called from Context::new).
pub(crate) fn reset_charset_registry() {
    CHARSET_REGISTRY.with(|slot| *slot.borrow_mut() = CharsetRegistry::new());
    if let Ok(mut cache) = charset_map_cache().write() {
        cache.clear();
    }
}

/// Collect GC roots from charset runtime state.
///
/// GNU Emacs keeps charset Lisp attributes reachable from `Vcharset_hash_table`
/// and also marks `charset_table[i].attributes` in `mark_charset`.  Neomacs's
/// Rust-side charset registry stores the plist values directly, so those Lisp
/// values must be surfaced explicitly as GC roots.
pub(crate) fn collect_charset_gc_roots(roots: &mut Vec<Value>) {
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        for info in reg.charsets.values() {
            if !info.unify_map.is_nil() {
                roots.push(info.unify_map);
            }
            for (_, value) in &info.plist {
                roots.push(*value);
            }
        }
    });
}

pub(crate) fn snapshot_charset_registry() -> CharsetRegistrySnapshot {
    CHARSET_REGISTRY.with(|slot| slot.borrow().snapshot())
}

pub(crate) fn restore_charset_registry(snapshot: CharsetRegistrySnapshot) {
    CHARSET_REGISTRY.with(|slot| *slot.borrow_mut() = CharsetRegistry::restore(snapshot));
}

/// Set the plist for a charset (used by `set-charset-plist` builtin).
pub(crate) fn set_charset_plist_registry(name: SymId, plist: Vec<(SymId, Value)>) {
    CHARSET_REGISTRY.with(|slot| slot.borrow_mut().set_plist(name, plist));
}

pub(crate) fn charset_target_ranges(name: &str) -> Option<Vec<(u32, u32)>> {
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        let name = lookup_interned(name)?;
        let info = reg.charsets.get(&name)?;
        match info.method {
            CharsetMethod::Offset(offset) => {
                offset_charset_char_ranges(info, offset, info.min_code, info.max_code)
            }
            CharsetMethod::Map(ref map_name) => {
                let values = load_charset_map(map_name, info)?
                    .code_to_char
                    .values()
                    .filter_map(|ch| u32::try_from(*ch).ok())
                    .collect();
                coalesce_u32_ranges(values)
            }
            CharsetMethod::Subset(ref subset) => {
                let values = (subset.parent_min_code..=subset.parent_max_code)
                    .filter_map(|parent_code| reg.decode_char(subset.parent, parent_code))
                    .filter_map(|ch| u32::try_from(ch).ok())
                    .collect();
                coalesce_u32_ranges(values)
            }
            CharsetMethod::Superset(_) => {
                let mut values = Vec::new();
                for (parent_name, _) in CharsetRegistry::superset_members(info) {
                    let ranges = charset_target_ranges(resolve_sym(parent_name))?;
                    for (from, to) in ranges {
                        values.extend(from..=to);
                    }
                }
                coalesce_u32_ranges(values)
            }
        }
    })
}

fn offset_raw_range(
    info: &CharsetInfo,
    offset: i64,
    from_code: i64,
    to_code: i64,
) -> Option<(u32, u32)> {
    let from_idx = charset_code_point_to_index(info, from_code)?;
    let to_idx = charset_code_point_to_index(info, to_code)?;
    let from = u32::try_from(from_idx.checked_add(offset)?).ok()?;
    let to = u32::try_from(to_idx.checked_add(offset)?).ok()?;
    Some((from.min(to), from.max(to)))
}

fn offset_unified_ranges(info: &CharsetInfo, from_code: i64, to_code: i64) -> Vec<(u32, u32)> {
    if !info.unified_p {
        return Vec::new();
    }
    let Some(unify_map) = charset_value_text(&info.unify_map) else {
        return Vec::new();
    };
    let Some(map) = load_charset_map(&unify_map, info) else {
        return Vec::new();
    };

    let partial = from_code > info.min_code || to_code < info.max_code;
    let values = if partial {
        map.char_to_code
            .iter()
            .filter_map(|(ch, code)| {
                if *code >= from_code && *code <= to_code {
                    u32::try_from(*ch).ok()
                } else {
                    None
                }
            })
            .collect()
    } else {
        map.char_to_code
            .keys()
            .filter_map(|ch| u32::try_from(*ch).ok())
            .collect()
    };
    coalesce_u32_ranges(values).unwrap_or_default()
}

fn offset_charset_char_ranges(
    info: &CharsetInfo,
    offset: i64,
    from_code: i64,
    to_code: i64,
) -> Option<Vec<(u32, u32)>> {
    let mut ranges = offset_unified_ranges(info, from_code, to_code);
    if let Some(raw) = offset_raw_range(info, offset, from_code, to_code) {
        ranges.push(raw);
    }
    if ranges.is_empty() {
        None
    } else {
        Some(ranges)
    }
}

pub(crate) fn map_charset_char_ranges(
    name: &str,
    from_code: Option<i64>,
    to_code: Option<i64>,
) -> Option<Vec<(u32, u32)>> {
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        let name = lookup_interned(name)?;
        let info = reg.charsets.get(&name)?;
        let from = from_code
            .map(|code| code.max(info.min_code))
            .unwrap_or(info.min_code);
        let to = to_code
            .map(|code| code.min(info.max_code))
            .unwrap_or(info.max_code);
        if from > to {
            return Some(Vec::new());
        }

        match &info.method {
            CharsetMethod::Offset(offset) => offset_charset_char_ranges(info, *offset, from, to),
            CharsetMethod::Map(map_name) => {
                let values = load_charset_map(map_name, info)?
                    .code_to_char
                    .iter()
                    .filter_map(|(code, ch)| {
                        if *code >= from && *code <= to {
                            u32::try_from(*ch).ok()
                        } else {
                            None
                        }
                    })
                    .collect();
                Some(coalesce_u32_ranges(values).unwrap_or_default())
            }
            _ => {
                let values: Vec<u32> = (from..=to)
                    .filter_map(|code| reg.decode_char(name, code))
                    .filter_map(|ch| u32::try_from(ch).ok())
                    .collect();
                Some(coalesce_u32_ranges(values).unwrap_or_default())
            }
        }
    })
}

pub(crate) fn charset_exists(name: &str) -> bool {
    CHARSET_REGISTRY.with(|slot| slot.borrow().contains(name))
}

pub(crate) fn charset_contains_char(name: &str, ch: u32) -> Option<bool> {
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        let name = lookup_interned(name)?;
        reg.charsets.get(&name)?;
        Some(reg.encode_char(name, i64::from(ch)).is_some())
    })
}

/// Encode `ch` in the charset named by `charset` and return the code point's
/// big-endian bytes (one per dimension), or `None` if it is not encodable.
pub(crate) fn charset_encode_char_bytes(charset: SymId, ch: i64) -> Option<Vec<u8>> {
    CHARSET_REGISTRY.with(|slot| slot.borrow().encode_char_bytes(charset, ch))
}

/// Decode the next character of `bytes` in `charset`, returning the Emacs
/// character code and how many bytes were consumed, or `None` when the leading
/// bytes are not an assigned code point of the charset.
pub(crate) fn charset_decode_char_from_bytes(charset: SymId, bytes: &[u8]) -> Option<(i64, usize)> {
    CHARSET_REGISTRY.with(|slot| slot.borrow().decode_char_from_bytes(charset, bytes))
}

/// Encode `ch` in `charset` to its raw code point (not split into bytes), or
/// `None` if unrepresentable. Used by codecs that transform the code point
/// (e.g. Shift-JIS).
pub(crate) fn charset_encode_char(charset: SymId, ch: i64) -> Option<i64> {
    CHARSET_REGISTRY.with(|slot| slot.borrow().encode_char(charset, ch))
}

/// Decode a raw code point in `charset` to an Emacs character code, or `None`.
pub(crate) fn charset_decode_char(charset: SymId, code: i64) -> Option<i64> {
    CHARSET_REGISTRY.with(|slot| slot.borrow().decode_char(charset, code))
}

/// Whether `charset` is ASCII-compatible (`CHARSET_ASCII_COMPATIBLE_P`). Used
/// by the coding-system plist to compute `:ascii-compatible-p` like GNU.
pub(crate) fn charset_is_ascii_compatible(charset: SymId) -> bool {
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        reg.charsets
            .get(&reg.resolve_name(charset))
            .is_some_and(|info| info.ascii_compatible_p)
    })
}

/// Which bytes can BEGIN a character of a charset, and how many bytes that
/// character has.
///
/// This is the data GNU builds `coding_attr_charset_valids` out of.  When a
/// `charset`-type coding system is defined, GNU walks each charset in
/// `:charset-list` and marks every byte from `code_space[(dim - 1) * 4]`
/// through `code_space[(dim - 1) * 4 + 1]` as one that charset can start
/// (src/coding.c:11122-11165); `decode_coding_charset` then consults the
/// resulting 256-entry vector BEFORE it reads a second byte (:5526-5528).
/// This port's `code_space` is GNU's table without the size and multiplier
/// columns, so the same range is at `[(dim - 1) * 2]` and `[+ 1]`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CharsetLeadingByte {
    /// `CHARSET_DIMENSION`: how many bytes one character occupies.
    pub dimension: usize,
    first: u8,
    last: u8,
}

impl CharsetLeadingByte {
    /// GNU's `AREF (valids, c)` naming this charset.
    pub(crate) fn accepts(self, byte: u8) -> bool {
        (self.first..=self.last).contains(&byte)
    }
}

/// [`CharsetLeadingByte`] for `charset`, or `None` when the charset is not
/// registered or its leading byte range is not a byte range (which is the same
/// thing GNU's vector says by leaving every element nil for it: no byte can
/// start it, so it decodes nothing).
pub(crate) fn charset_leading_byte(charset: SymId) -> Option<CharsetLeadingByte> {
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        let info = reg.charsets.get(&reg.resolve_name(charset))?;
        let dimension = charset_dimension(info);
        let first = u8::try_from(charset_byte_min(info, dimension - 1)).ok()?;
        let last = u8::try_from(charset_byte_max(info, dimension - 1)).ok()?;
        (first <= last).then_some(CharsetLeadingByte {
            dimension,
            first,
            last,
        })
    })
}

/// The dimension of `charset` (1 or 2), or `None` if unknown. Used by the
/// ISO-2022 category computation.
pub(crate) fn charset_dimension_by_sym(charset: SymId) -> Option<i64> {
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        reg.charsets
            .get(&reg.resolve_name(charset))
            .map(|info| info.dimension)
    })
}

/// Highest-priority charset that has an emacs-mule id and can encode `ch`,
/// returned as (emacs-mule-id, dimension, code point). The emacs-mule codec
/// selects charsets through the same priority order GNU uses.
pub(crate) fn emacs_mule_encode_char(ch: i64) -> Option<(i64, i64, i64)> {
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        // GNU selects from `Vemacs_mule_charset_list` in charset-priority order.
        // The emacs-mule ids are assigned to roughly follow that priority
        // (latin < CJK < CNS), so iterate every emacs-mule charset by ascending
        // id and take the first that can represent the character.
        let jisx0208_1978 = lookup_interned("japanese-jisx0208-1978");
        let mut candidates: Vec<(i64, SymId, i64)> = reg
            .charsets
            .iter()
            // GNU's charset priority ranks the deprecated 1978 JIS after gb2312
            // and the 1990 JIS, but its emacs-mule id (144) sorts ahead of
            // gb2312's (145). Its repertoire is covered by those, so drop it
            // from the candidates to keep the priority order GNU uses.
            .filter(|(name, _)| Some(**name) != jisx0208_1978)
            .filter_map(|(name, info)| info.emacs_mule_id.map(|id| (id, *name, info.dimension)))
            .collect();
        candidates.sort_by_key(|(id, _, _)| *id);
        for (id, name, dimension) in candidates {
            if let Some(code) = reg.encode_char(name, ch) {
                return Some((id, dimension, code));
            }
        }
        None
    })
}

/// All charsets that can be designated in ISO-2022 (they have an iso-final
/// char), in ascending charset-id order, excluding the deprecated
/// japanese-jisx0208-1978. Used as the candidate set for FULL_SUPPORT ISO-2022
/// coding systems (those whose `:charset-list` is the symbol `iso-2022`).
pub(crate) fn iso2022_full_charset_candidates() -> Vec<SymId> {
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        let jisx0208_1978 = lookup_interned("japanese-jisx0208-1978");
        let mut out: Vec<(i64, SymId)> = reg
            .charsets
            .iter()
            .filter(|(name, info)| info.iso_final_char.is_some() && Some(**name) != jisx0208_1978)
            .map(|(name, info)| (info.id, *name))
            .collect();
        out.sort_by_key(|(id, _)| *id);
        out.into_iter().map(|(_, name)| name).collect()
    })
}

/// ISO-2022 designation properties of a charset: `(iso-final-char, dimension,
/// chars_96)`. `chars_96` is true for a 96-character set (its code space starts
/// at 0x20), false for a 94-character set. Returns `None` if the charset has no
/// ISO-2022 final char (cannot be designated).
pub(crate) fn charset_iso2022_designation(charset: SymId) -> Option<(i64, i64, bool)> {
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        let info = reg.charsets.get(&reg.resolve_name(charset))?;
        let final_char = info.iso_final_char?;
        Some((final_char, info.dimension, info.code_space[0] == 0x20))
    })
}

/// The charset (and its dimension) matching an ISO-2022 designation — final
/// char, dimension and set size — or `None`. Used when decoding an ESC
/// designation sequence. Prefers the lowest charset id, so the modern
/// japanese-jisx0208 (final `B`) wins over duplicates.
pub(crate) fn charset_by_iso_final(
    final_char: i64,
    dimension: i64,
    chars_96: bool,
) -> Option<(SymId, i64)> {
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        reg.charsets
            .iter()
            .filter(|(_, info)| {
                info.iso_final_char == Some(final_char)
                    && info.dimension == dimension
                    && (info.code_space[0] == 0x20) == chars_96
            })
            .min_by_key(|(_, info)| info.id)
            .map(|(name, info)| (*name, info.dimension))
    })
}

/// The charset (name, dimension) carrying a given emacs-mule id, or `None`.
pub(crate) fn charset_by_emacs_mule_id(id: i64) -> Option<(SymId, i64)> {
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        reg.charsets.iter().find_map(|(name, info)| {
            (info.emacs_mule_id == Some(id)).then_some((*name, info.dimension))
        })
    })
}

/// `emacs_mule_bytes[c]`: the number of source bytes an emacs-mule sequence
/// led by byte `c` consumes (charset.c:1183).  Returns `None` when `c` is not
/// an emacs-mule leading code (the caller defaults to 1).
pub(crate) fn emacs_mule_leading_code_bytes(c: u8) -> Option<i32> {
    let (_, dimension) = charset_by_emacs_mule_id(i64::from(c))?;
    let extra = if c < 0xA0 { 1 } else { 2 };
    Some(dimension as i32 + extra)
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_int_or_marker(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

fn optional_charset_lisp_pos_to_byte(
    buf: &crate::buffer::Buffer,
    pos: Option<&Value>,
) -> Result<Option<EmacsBytePos>, Flow> {
    let Some(pos) = pos else {
        return Ok(Some(buf.point_emacs_byte_pos()));
    };
    let pos = LispCharPos1::new(expect_int_or_marker(pos)?);
    let point_min = buf.point_min_lisp_char_pos();
    let point_max = buf.point_max_lisp_char_pos();
    if pos < point_min || pos > point_max {
        return Ok(None);
    }
    Ok(Some(buf.lisp_pos_to_accessible_emacs_byte_pos(pos)))
}

fn require_known_charset(value: &Value) -> Result<SymId, Flow> {
    let name = match value.kind() {
        ValueKind::Symbol(id) => id,
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("charsetp"), *value],
            ));
        }
    };
    let known = CHARSET_REGISTRY.with(|slot| slot.borrow().contains_symbol(name));
    if known {
        Ok(name)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("charsetp"), Value::from_sym_id(name)],
        ))
    }
}

fn decode_char_codepoint_arg(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) if n >= 0 => Ok(n),
        ValueKind::Float => {
            let f = value.as_float().unwrap();
            if f.is_finite() && f >= 0.0 && f.fract() == 0.0 && f <= i64::MAX as f64 {
                Ok(f as i64)
            } else {
                Err(signal(
                    "error",
                    vec![Value::string(
                        "Not an in-range integer, integral float, or cons of integers",
                    )],
                ))
            }
        }
        _ => Err(signal(
            "error",
            vec![Value::string(
                "Not an in-range integer, integral float, or cons of integers",
            )],
        )),
    }
}

fn expect_wholenump(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) if n >= 0 => Ok(n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("wholenump"), *value],
        )),
    }
}

fn expect_fixnump(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("fixnump"), *value],
        )),
    }
}

fn encode_char_input(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(c) if (0..=0x3F_FFFF).contains(&c) => Ok(c),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), *value],
        )),
    }
}

fn charset_value_text(value: &Value) -> Option<String> {
    match value.kind() {
        ValueKind::String => value
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes())),
        ValueKind::Symbol(id) => Some(resolve_sym(id).to_string()),
        _ => None,
    }
}

fn parse_map_name(value: &Value) -> Option<String> {
    charset_value_text(value)
}

fn parse_subset_spec(value: &Value) -> Option<CharsetSubsetSpec> {
    let items = list_to_vec(value)?;
    if items.len() != 4 {
        return None;
    }
    Some(CharsetSubsetSpec {
        parent: items[0].as_symbol_id()?,
        parent_min_code: decode_code_arg(&items[1]),
        parent_max_code: decode_code_arg(&items[2]),
        offset: int_or_zero(&items[3]),
    })
}

fn parse_superset_spec(value: &Value) -> Option<Vec<(SymId, i64)>> {
    let items = list_to_vec(value)?;
    let members = items
        .into_iter()
        .map(|item| match item.kind() {
            ValueKind::Symbol(id) => Some((id, 0)),
            ValueKind::Cons => {
                let car = item.cons_car();
                let cdr = item.cons_cdr();
                let name = car.as_symbol_id()?;
                Some((name, int_or_zero(&cdr)))
            }
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;

    Some(members)
}

// ---------------------------------------------------------------------------
// Pure builtins (Vec<Value> -> EvalResult)
// ---------------------------------------------------------------------------

/// `(charsetp OBJECT)` -- return t if OBJECT names a known charset.
pub(crate) fn builtin_charsetp(args: Vec<Value>) -> EvalResult {
    expect_args("charsetp", &args, 1)?;
    let name = match args[0].kind() {
        ValueKind::Symbol(id) => id,
        _ => return Ok(Value::NIL),
    };
    let found = CHARSET_REGISTRY.with(|slot| slot.borrow().contains_symbol(name));
    Ok(Value::bool_val(found))
}

/// `(charset-list)` -- return charset symbols in priority order.
#[cfg(test)]
pub(crate) fn builtin_charset_list(args: Vec<Value>) -> EvalResult {
    expect_args("charset-list", &args, 0)?;
    let names: Vec<Value> = CHARSET_REGISTRY.with(|slot| {
        slot.borrow()
            .priority_list()
            .iter()
            .map(|name| Value::from_sym_id(*name))
            .collect()
    });
    Ok(Value::list(names))
}

/// `(unibyte-charset)` -- return the charset used for unibyte strings.
#[cfg(test)]
pub(crate) fn builtin_unibyte_charset(args: Vec<Value>) -> EvalResult {
    expect_args("unibyte-charset", &args, 0)?;
    Ok(Value::symbol("eight-bit"))
}

/// `(charset-priority-list &optional HIGHESTP)` -- return list of charsets
/// in priority order.  If HIGHESTP is non-nil, return only the highest
/// priority charset.
pub(crate) fn builtin_charset_priority_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("charset-priority-list", &args, 1)?;
    let highestp = args.first().map(|v| v.is_truthy()).unwrap_or(false);
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        let priority = reg.priority_list();
        if highestp {
            // GNU Fcharset_priority_list (charset.c:2163): HIGHESTP returns
            // the highest-priority charset's NAME SYMBOL itself, not a list.
            if let Some(first) = priority.first() {
                Ok(Value::from_sym_id(*first))
            } else {
                Ok(Value::NIL)
            }
        } else {
            let syms: Vec<Value> = priority.iter().map(|s| Value::from_sym_id(*s)).collect();
            Ok(Value::list(syms))
        }
    })
}

/// `(set-charset-priority &rest CHARSETS)` -- set charset detection priority.
pub(crate) fn builtin_set_charset_priority(args: Vec<Value>) -> EvalResult {
    expect_min_args("set-charset-priority", &args, 1)?;

    let mut requested = Vec::with_capacity(args.len());
    for arg in &args {
        let name = match arg.kind() {
            ValueKind::Symbol(id) => id,
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("charsetp"), *arg],
                ));
            }
        };
        let known = CHARSET_REGISTRY.with(|slot| slot.borrow().contains_symbol(name));
        if !known {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("charsetp"), *arg],
            ));
        }
        requested.push(name);
    }
    CHARSET_REGISTRY.with(|slot| slot.borrow_mut().set_priority(&requested));
    Ok(Value::NIL)
}

/// Classify a character code into the name of the highest-priority charset
/// that contains it, mirroring GNU's `CHAR_CHARSET` for the default priority
/// order.  Used by both `char-charset` and `split-char`.
///
/// GNU `char_charset` (charset.c) classifies the high code ranges by the
/// internal char boundaries: codes above MAX_5_BYTE_CHAR (0x3FFF7F) are raw
/// bytes in the `eight-bit` charset, codes above the Unicode maximum
/// (0x10FFFF) but within MAX_5_BYTE_CHAR are in the internal `emacs` charset,
/// and the rest are Unicode.
///
/// Whether GNU surfaces the `unicode-bmp` subset depends on the active charset
/// priority: `char_charset` walks `Vcharset_ordered_list`, so once
/// `set-language-environment "UTF-8"` moves `unicode-bmp` ahead of the
/// `non_preferred_head` boundary, BMP characters classify as `unicode-bmp`;
/// under the default ASCII-only priority they fall through to the `unicode`
/// parent. The live ordered list and boundary are consulted via
/// `CharsetRegistry::classify_char` (a faithful port of GNU `char_charset`).
pub(crate) fn char_charset_name(ch: i64) -> &'static str {
    let sym = CHARSET_REGISTRY.with(|slot| slot.borrow().classify_char(ch));
    resolve_sym(sym)
}

/// `(char-charset CH &optional RESTRICTION)` -- return charset for character.
///
/// Mirrors GNU `Fchar_charset` (src/charset.c:2032).  Without RESTRICTION the
/// char is classified by `CHAR_CHARSET` (see `char_charset_name`): ASCII chars
/// map to `ascii`, all other Unicode chars to the dimension-3 `unicode`
/// charset.
///
/// When RESTRICTION is a non-nil list of charsets, GNU walks it in order and
/// returns the first charset that contains CH (`ENCODE_CHAR` !=
/// `CHARSET_INVALID_CODE`), or nil if none does.  Each element must be a real
/// charset or a `wrong-type-argument`/`charsetp` error is signalled, exactly
/// as `CHECK_CHARSET_GET_CHARSET` does.  The literal list element is returned,
/// not its alias target.
pub(crate) fn builtin_char_charset(args: Vec<Value>) -> EvalResult {
    expect_min_args("char-charset", &args, 1)?;
    expect_max_args("char-charset", &args, 2)?;
    let ch = encode_char_input(&args[0])?;

    let restriction = args.get(1).copied().unwrap_or(Value::NIL);
    if restriction.is_nil() {
        return Ok(Value::symbol(char_charset_name(ch)));
    }

    // GNU only special-cases CONSP restrictions; a non-cons (coding-system)
    // restriction goes through `coding_system_charset_list`, which neomacs does
    // not yet model, so fall back to the unrestricted classification.
    if !matches!(restriction.kind(), ValueKind::Cons) {
        return Ok(Value::symbol(char_charset_name(ch)));
    }

    let mut tail = restriction;
    while matches!(tail.kind(), ValueKind::Cons) {
        let elem = tail.cons_car();
        // CHECK_CHARSET_GET_CHARSET: each element must be a known charset.
        let name = require_known_charset(&elem)?;
        let contains = CHARSET_REGISTRY.with(|slot| slot.borrow().encode_char(name, ch).is_some());
        if contains {
            return Ok(elem);
        }
        tail = tail.cons_cdr();
    }
    Ok(Value::NIL)
}

/// `(split-char CH)` -- return a list of the charset symbol and the one to
/// four position-codes of CH in that charset.
///
/// GNU `Fsplit_char` (src/charset.c): validates CHARACTER, classifies the
/// char into its highest-priority charset (`CHAR_CHARSET`), encodes it
/// (`ENCODE_CHAR`) to a code point, then splits that code into
/// `CHARSET_DIMENSION` bytes big-endian and conses the charset name onto the
/// front.  Since `CHAR_CHARSET` canonicalizes every Unicode char to the
/// dimension-3 `unicode` charset (see `char_charset_name`), non-ASCII chars
/// yield four-element lists.  Examples (UTF-8 language environment):
/// `(split-char ?A)` => `(ascii 65)`, `(split-char ?中)` => `(unicode 0 78 45)`.
pub(crate) fn builtin_split_char(args: Vec<Value>) -> EvalResult {
    expect_args("split-char", &args, 1)?;
    // CHECK_CHARACTER: reject non-characters like GNU (encode_char_input
    // enforces the 0..=0x3FFFFF character range and signals otherwise).
    let ch = encode_char_input(&args[0])?;
    let charset_name = char_charset_name(ch);
    let charset_sym = intern(charset_name);
    // ENCODE_CHAR: the code point of CH within its charset. For the
    // unicode/ascii/eight-bit/emacs charsets this is an identity-ish map, but
    // we go through the registry so any code-offset is honored like GNU.
    let code = charset_encode_char(charset_sym, ch).unwrap_or(ch);
    let dimension = charset_dimension_by_sym(charset_sym).unwrap_or(1).max(1);
    // Split the code into `dimension` bytes, big-endian, as in GNU.
    let mut codes = Vec::with_capacity(dimension as usize);
    for shift in (0..dimension).rev() {
        let byte = (code >> (8 * shift)) & 0xFF;
        codes.push(Value::fixnum(byte));
    }
    let mut elems = Vec::with_capacity(dimension as usize + 1);
    elems.push(Value::from_sym_id(charset_sym));
    elems.extend(codes);
    Ok(Value::list(elems))
}

/// `(charset-plist CHARSET)` -- return property list for CHARSET.
pub(crate) fn builtin_charset_plist(args: Vec<Value>) -> EvalResult {
    expect_args("charset-plist", &args, 1)?;
    let name = require_known_charset(&args[0])?;
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        if let Some(pairs) = reg.plist(name) {
            let mut elems = Vec::with_capacity(pairs.len() * 2);
            for (key, val) in pairs {
                elems.push(Value::from_sym_id(*key));
                elems.push(*val);
            }
            Ok(Value::list(elems))
        } else {
            Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("charsetp"), Value::from_sym_id(name)],
            ))
        }
    })
}

/// `(charset-id-internal &optional CHARSET)` -- return internal charset id.
pub(crate) fn builtin_charset_id_internal(args: Vec<Value>) -> EvalResult {
    expect_max_args("charset-id-internal", &args, 1)?;
    let arg = args.first().cloned().unwrap_or(Value::NIL);
    let name = match arg.kind() {
        ValueKind::Symbol(id) => id,
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("charsetp"), arg],
            ));
        }
    };

    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        if let Some(id) = reg.id(name) {
            Ok(Value::fixnum(id))
        } else {
            Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("charsetp"), Value::from_sym_id(name)],
            ))
        }
    })
}

/// Extract an integer from a Value, or return 0 for nil.
fn int_or_zero(val: &Value) -> i64 {
    match val.kind() {
        ValueKind::Fixnum(n) => n,
        _ => 0,
    }
}

/// Extract an optional integer from a Value (nil → None).
fn opt_int(val: &Value) -> Option<i64> {
    match val.kind() {
        ValueKind::Fixnum(n) => Some(n),
        ValueKind::Nil => None,
        _ => None,
    }
}

/// Decode a code point argument that may be a plain int or a cons (HI . LO).
fn decode_code_arg(val: &Value) -> i64 {
    match val.kind() {
        ValueKind::Fixnum(n) => n,
        ValueKind::Cons => {
            let pair_car = val.cons_car();
            let pair_cdr = val.cons_cdr();
            let hi = int_or_zero(&pair_car);
            let lo = int_or_zero(&pair_cdr);
            (hi << 16) | lo
        }
        _ => 0,
    }
}

/// Parse a plist Value into a Vec of (key, value) pairs.
fn parse_plist(val: &Value) -> Vec<(SymId, Value)> {
    let mut result = Vec::new();
    let Some(items) = list_to_vec(val) else {
        return result;
    };
    let mut i = 0;
    while i + 1 < items.len() {
        if let Some(key) = items[i].as_symbol_id() {
            result.push((key, items[i + 1]));
        }
        i += 2;
    }
    result
}

fn coalesce_u32_ranges(mut values: Vec<u32>) -> Option<Vec<(u32, u32)>> {
    if values.is_empty() {
        return None;
    }

    values.sort_unstable();
    values.dedup();

    let mut ranges = Vec::new();
    let mut start = values[0];
    let mut end = values[0];

    for value in values.into_iter().skip(1) {
        if value == end.saturating_add(1) {
            end = value;
        } else {
            ranges.push((start, end));
            start = value;
            end = value;
        }
    }

    ranges.push((start, end));
    Some(ranges)
}

fn charset_dimension(info: &CharsetInfo) -> usize {
    usize::try_from(info.dimension).unwrap_or(1).clamp(1, 4)
}

fn charset_byte_min(info: &CharsetInfo, byte_index: usize) -> i64 {
    info.code_space.get(byte_index * 2).copied().unwrap_or(0)
}

fn charset_byte_max(info: &CharsetInfo, byte_index: usize) -> i64 {
    info.code_space
        .get(byte_index * 2 + 1)
        .copied()
        .unwrap_or(0)
}

fn charset_byte_size(info: &CharsetInfo, byte_index: usize) -> Option<i64> {
    let min = charset_byte_min(info, byte_index);
    let max = charset_byte_max(info, byte_index);
    if max < min { None } else { Some(max - min + 1) }
}

fn charset_code_linear_p(info: &CharsetInfo) -> bool {
    let dimension = charset_dimension(info);
    dimension == 1
        || (0..dimension.saturating_sub(1)).all(|index| charset_byte_size(info, index) == Some(256))
}

fn charset_raw_code_index(info: &CharsetInfo, code_point: i64) -> Option<i64> {
    let mut index = 0i64;
    let mut stride = 1i64;
    for byte_index in 0..4 {
        let byte = (code_point >> (byte_index * 8)) & 0xff;
        let min = charset_byte_min(info, byte_index);
        let max = charset_byte_max(info, byte_index);
        if byte < min || byte > max {
            return None;
        }
        index = index.checked_add(byte.checked_sub(min)?.checked_mul(stride)?)?;
        stride = stride.checked_mul(charset_byte_size(info, byte_index)?)?;
    }
    Some(index)
}

fn charset_code_point_to_index(info: &CharsetInfo, code_point: i64) -> Option<i64> {
    if charset_code_linear_p(info) {
        return code_point.checked_sub(info.min_code);
    }
    let raw_index = charset_raw_code_index(info, code_point)?;
    let min_index = charset_raw_code_index(info, info.min_code)?;
    raw_index.checked_sub(min_index)
}

fn charset_index_to_code_point(info: &CharsetInfo, index: i64) -> Option<i64> {
    if index < 0 {
        return None;
    }
    if charset_code_linear_p(info) {
        return info.min_code.checked_add(index);
    }

    let mut index = index.checked_add(charset_raw_code_index(info, info.min_code)?)?;
    let mut code_point = 0i64;
    for byte_index in 0..4 {
        let size = charset_byte_size(info, byte_index)?;
        let min = charset_byte_min(info, byte_index);
        let byte = min.checked_add(index % size)?;
        if byte > charset_byte_max(info, byte_index) {
            return None;
        }
        code_point |= byte << (byte_index * 8);
        index /= size;
    }
    if index == 0 { Some(code_point) } else { None }
}

fn make_char_position_code(info: &CharsetInfo, args: &[Value]) -> Result<i64, Flow> {
    let Some(code1) = args.get(1).filter(|value| !value.is_nil()) else {
        return Ok(if info.ascii_compatible_p {
            0
        } else {
            info.min_code
        });
    };

    let dimension = charset_dimension(info);
    let mut code = expect_make_char_code_byte(code1)?;
    for low_dimension in (0..dimension.saturating_sub(1)).rev() {
        code <<= 8;
        let code_arg_index = dimension - low_dimension;
        let next = match args.get(code_arg_index) {
            Some(value) if !value.is_nil() => expect_make_char_code_byte(value)?,
            _ => charset_byte_min(info, low_dimension),
        };
        code |= next;
    }

    if info.iso_final_char.is_some() {
        code &= 0x7f7f7f7f;
    }
    Ok(code)
}

fn expect_make_char_code_byte(value: &Value) -> Result<i64, Flow> {
    let code = expect_wholenump(value)?;
    if code >= 0x100 {
        Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::fixnum(0xff), *value],
        ))
    } else {
        Ok(code)
    }
}

/// `(make-char CHARSET &optional CODE1 CODE2 CODE3 CODE4)` -- return a
/// character at the charset position codes.
pub(crate) fn builtin_make_char(args: Vec<Value>) -> EvalResult {
    if args.is_empty() || args.len() > 5 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("make-char"), Value::fixnum(args.len() as i64)],
        ));
    }

    let name = require_known_charset(&args[0])?;
    let decoded = CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        let info = reg.charsets.get(&name).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("charsetp"), args[0]],
            )
        })?;
        let code = make_char_position_code(info, &args)?;
        Ok(reg.decode_char(name, code))
    })?;

    match decoded {
        Some(ch) => Ok(Value::fixnum(ch)),
        None => Err(signal("error", vec![Value::string("Invalid code(s)")])),
    }
}

/// `(define-charset-internal NAME DIM CODE-SPACE MIN-CODE MAX-CODE
///    ISO-FINAL ISO-REVISION EMACS-MULE-ID ASCII-COMPAT-P SUPPLEMENTARY-P
///    INVALID-CODE CODE-OFFSET MAP SUBSET SUPERSET UNIFY-MAP PLIST)`
///
/// Internal charset initializer — registers a charset in the registry.
/// Accepts exactly 17 arguments matching the Emacs C function.
pub(crate) fn builtin_define_charset_internal(args: Vec<Value>) -> EvalResult {
    expect_args("define-charset-internal", &args, 17)?;

    // arg[0]: name (symbol)
    let name = match args[0].kind() {
        ValueKind::Symbol(id) => id,
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), args[0]],
            ));
        }
    };

    // arg[1]: dimension (vector or integer — the define-charset macro passes
    //         a vector of the form [dim ...], but we also accept a plain int)
    let dimension = match args[1].kind() {
        ValueKind::Fixnum(n) => n,
        ValueKind::Veclike(VecLikeType::Vector) => {
            let vec = args[1].as_vector_data().unwrap().clone();
            if vec.is_empty() {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![args[1], Value::fixnum(0)],
                ));
            }
            int_or_zero(&vec[0])
        }
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("arrayp"), args[1]],
            ));
        }
    };

    // arg[2]: code-space (vector of 8 integers — byte ranges per dimension)
    let code_space = match args[2].kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let vec = args[2].as_vector_data().unwrap().clone();
            if vec.len() < 2 {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![args[2], Value::fixnum(vec.len() as i64)],
                ));
            }
            let mut cs = [0i64; 8];
            for (i, v) in vec.iter().enumerate().take(8) {
                cs[i] = int_or_zero(v);
            }
            cs
        }
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("arrayp"), args[2]],
            ));
        }
    };

    // Compute default min/max code from code-space, matching official Emacs
    // charset.c: min = cs[0] | cs[2]<<8 | cs[4]<<16 | cs[6]<<24
    let cs_min =
        code_space[0] | (code_space[2] << 8) | (code_space[4] << 16) | (code_space[6] << 24);
    let cs_max =
        code_space[1] | (code_space[3] << 8) | (code_space[5] << 16) | (code_space[7] << 24);

    // arg[3]: min-code, arg[4]: max-code (override from code-space if given)
    let min_code = if args[3].is_nil() {
        cs_min
    } else {
        decode_code_arg(&args[3])
    };
    let max_code = if args[4].is_nil() {
        cs_max
    } else {
        decode_code_arg(&args[4])
    };

    // arg[5]: iso-final-char (char or nil)
    let iso_final_char = opt_int(&args[5]);

    // arg[6]: iso-revision (int or nil)
    let iso_revision = opt_int(&args[6]);

    // arg[7]: emacs-mule-id (int or nil)
    let emacs_mule_id = opt_int(&args[7]);

    // arg[8]: ascii-compatible-p
    let ascii_compatible_p = args[8].is_truthy();

    // arg[9]: supplementary-p
    let supplementary_p = args[9].is_truthy();

    // arg[10]: invalid-code (int or nil)
    let invalid_code = opt_int(&args[10]);

    // arg[11]: code-offset  → CHARSET_METHOD_OFFSET
    // arg[12]: map           → CHARSET_METHOD_MAP
    // arg[13]: subset        → CHARSET_METHOD_SUBSET
    // arg[14]: superset      → CHARSET_METHOD_SUPERSET
    let method = if !args[11].is_nil() {
        CharsetMethod::Offset(int_or_zero(&args[11]))
    } else if !args[12].is_nil() {
        CharsetMethod::Map(parse_map_name(&args[12]).unwrap_or_default())
    } else if !args[13].is_nil() {
        CharsetMethod::Subset(parse_subset_spec(&args[13]).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), args[13]],
            )
        })?)
    } else if !args[14].is_nil() {
        CharsetMethod::Superset(parse_superset_spec(&args[14]).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), args[14]],
            )
        })?)
    } else {
        // Default to offset 0 if nothing specified
        CharsetMethod::Offset(0)
    };

    // arg[15]: unify-map
    // arg[16]: plist
    let unify_map = args[15];
    let plist = parse_plist(&args[16]);

    CHARSET_REGISTRY.with(|slot| {
        let mut reg = slot.borrow_mut();
        // Use emacs-mule-id as the charset ID if provided and no collision,
        // otherwise auto-allocate.
        let id = if let Some(mule_id) = emacs_mule_id {
            mule_id
        } else {
            reg.alloc_id()
        };

        let info = CharsetInfo {
            id,
            name,
            dimension,
            code_space,
            min_code,
            max_code,
            iso_final_char,
            iso_revision,
            emacs_mule_id,
            ascii_compatible_p,
            supplementary_p,
            unified_p: false,
            invalid_code,
            unify_map,
            method,
            plist,
        };
        reg.register(info);
    });

    Ok(Value::NIL)
}

/// Context-aware variant of `(find-charset-region BEG END &optional TABLE)`.
///
/// Returns charset symbols present in the region `[BEG, END)` where BEG/END are
/// Emacs 1-based character positions inside the accessible region.
pub(crate) fn builtin_find_charset_region(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("find-charset-region", &args, 2)?;
    expect_max_args("find-charset-region", &args, 3)?;
    let region = super::position::LispRegionArgs::from_values(&ctx.buffers, args[0], args[1])?;

    let buf = ctx
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    let byte_range = region.accessible_byte_range(buf)?;
    if byte_range.is_empty() {
        return Ok(Value::list(vec![Value::symbol("ascii")]));
    }

    let string = buf.buffer_substring_lisp_string_range(byte_range);
    let charsets = classify_string_charsets(&string);
    if charsets.is_empty() {
        return Ok(Value::list(vec![Value::symbol("ascii")]));
    }
    Ok(Value::list(
        charsets.into_iter().map(Value::symbol).collect::<Vec<_>>(),
    ))
}

/// `(encode-big5-char CH)` -- encode character CH in BIG5 space.
pub(crate) fn builtin_encode_big5_char(args: Vec<Value>) -> EvalResult {
    expect_args("encode-big5-char", &args, 1)?;
    let ch = encode_char_input(&args[0])?;
    Ok(Value::fixnum(ch))
}

/// `(decode-big5-char CODE)` -- decode BIG5 code to Emacs character code.
pub(crate) fn builtin_decode_big5_char(args: Vec<Value>) -> EvalResult {
    expect_args("decode-big5-char", &args, 1)?;
    let code = expect_wholenump(&args[0])?;
    Ok(Value::fixnum(code))
}

/// `(encode-sjis-char CH)` -- encode character CH in Shift-JIS space.
pub(crate) fn builtin_encode_sjis_char(args: Vec<Value>) -> EvalResult {
    expect_args("encode-sjis-char", &args, 1)?;
    let ch = encode_char_input(&args[0])?;
    Ok(Value::fixnum(ch))
}

/// `(decode-sjis-char CODE)` -- decode Shift-JIS code to Emacs character code.
pub(crate) fn builtin_decode_sjis_char(args: Vec<Value>) -> EvalResult {
    expect_args("decode-sjis-char", &args, 1)?;
    let code = expect_wholenump(&args[0])?;
    Ok(Value::fixnum(code))
}

/// `(get-unused-iso-final-char DIMENSION CHARS)` -- return an available ISO
/// final-char code for the requested DIMENSION/CHARS class.
pub(crate) fn builtin_get_unused_iso_final_char(args: Vec<Value>) -> EvalResult {
    expect_args("get-unused-iso-final-char", &args, 2)?;
    let dimension = expect_fixnump(&args[0])?;
    let chars = expect_fixnump(&args[1])?;
    if !matches!(dimension, 1..=3) {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Invalid DIMENSION {dimension}, it should be 1, 2, or 3"
            ))],
        ));
    }
    if !matches!(chars, 94 | 96) {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Invalid CHARS {chars}, it should be 94 or 96"
            ))],
        ));
    }
    let final_char = match (dimension, chars) {
        (1, 94) => 54,
        (1, 96) => 51,
        (2, 94) => 50,
        (2, 96) | (3, 94) | (3, 96) => 48,
        _ => 48,
    };
    Ok(Value::fixnum(final_char))
}

/// `(declare-equiv-charset DIMENSION CHARS CH CHARSET)` -- declare an
/// equivalent charset mapping tuple.
pub(crate) fn builtin_declare_equiv_charset(args: Vec<Value>) -> EvalResult {
    expect_args("declare-equiv-charset", &args, 4)?;
    let _charset = require_known_charset(&args[3])?;
    let dimension = expect_fixnump(&args[0])?;
    let chars = expect_fixnump(&args[1])?;
    let _ch = encode_char_input(&args[2])?;
    if !matches!(dimension, 1..=3) {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Invalid DIMENSION {dimension}, it should be 1, 2, or 3"
            ))],
        ));
    }
    if !matches!(chars, 94 | 96) {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Invalid CHARS {chars}, it should be 94 or 96"
            ))],
        ));
    }
    Ok(Value::NIL)
}

/// `(define-charset-alias ALIAS CHARSET)` -- add ALIAS for CHARSET.
pub(crate) fn builtin_define_charset_alias(args: Vec<Value>) -> EvalResult {
    expect_args("define-charset-alias", &args, 2)?;
    let target = require_known_charset(&args[1])?;
    if let Some(id) = args[0].as_symbol_id() {
        CHARSET_REGISTRY.with(|slot| slot.borrow_mut().define_alias(id, target));
    }
    Ok(Value::NIL)
}

/// `(find-charset-string STR &optional TABLE)` -- returns a list of charsets
/// present in STR.
pub(crate) fn builtin_find_charset_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("find-charset-string", &args, 1)?;
    expect_max_args("find-charset-string", &args, 2)?;
    if !args[0].is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        ));
    }
    let string = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;

    let charsets = classify_string_charsets(string);
    if charsets.is_empty() {
        Ok(Value::NIL)
    } else {
        Ok(Value::list(
            charsets.into_iter().map(Value::symbol).collect::<Vec<_>>(),
        ))
    }
}

/// `(decode-char CHARSET CODE-POINT)` -- decode code-point in CHARSET space.
///
/// Uses the charset's registered method (Offset, Map, etc.) to convert
/// a charset-specific code-point to an Emacs internal character code.
pub(crate) fn builtin_decode_char(args: Vec<Value>) -> EvalResult {
    expect_args("decode-char", &args, 2)?;
    let name = require_known_charset(&args[0])?;
    let code_point = decode_char_codepoint_arg(&args[1])?;

    let decoded = CHARSET_REGISTRY.with(|slot| slot.borrow().decode_char(name, code_point));

    Ok(decoded.map_or(Value::NIL, Value::fixnum))
}

/// `(encode-char CH CHARSET)` -- encode CH in CHARSET space.
///
/// Uses the charset's registered method to convert an Emacs internal
/// character code back to a charset-specific code-point.
pub(crate) fn builtin_encode_char(args: Vec<Value>) -> EvalResult {
    expect_args("encode-char", &args, 2)?;
    let ch = encode_char_input(&args[0])?;
    let name = require_known_charset(&args[1])?;

    let encoded = CHARSET_REGISTRY.with(|slot| slot.borrow().encode_char(name, ch));

    Ok(encoded.map_or(Value::NIL, Value::fixnum))
}

/// `(unify-charset CHARSET &optional UNIFY-MAP DEUNIFY)` -- toggle Unicode
/// unification for an offset charset.
pub(crate) fn builtin_unify_charset(args: Vec<Value>) -> EvalResult {
    expect_min_args("unify-charset", &args, 1)?;
    expect_max_args("unify-charset", &args, 3)?;
    let name = require_known_charset(&args[0])?;

    CHARSET_REGISTRY.with(|slot| {
        let mut reg = slot.borrow_mut();
        let name = reg.resolve_name(name);
        let info = reg.charsets.get_mut(&name).expect("known charset");

        if args.get(2).is_some_and(|value| value.is_truthy()) {
            info.unified_p = false;
            return Ok(Value::NIL);
        }

        if let Some(unify_map) = args.get(1).filter(|value| !value.is_nil()) {
            match unify_map.kind() {
                ValueKind::String | ValueKind::Veclike(VecLikeType::Vector) => {
                    info.unify_map = *unify_map;
                }
                _ => {
                    return Err(signal("error", vec![Value::string("Bad unify-map")]));
                }
            }
        }

        match info.method {
            CharsetMethod::Offset(offset) if offset >= 0x110000 => {
                info.unified_p = true;
                Ok(Value::NIL)
            }
            _ => Err(signal(
                "error",
                vec![Value::string(format!(
                    "Can't unify charset: {}",
                    resolve_sym(name)
                ))],
            )),
        }
    })
}

/// `(clear-charset-maps)` -- clear charset-related caches and return nil.
pub(crate) fn builtin_clear_charset_maps(args: Vec<Value>) -> EvalResult {
    expect_max_args("clear-charset-maps", &args, 0)?;
    if let Ok(mut cache) = charset_map_cache().write() {
        cache.clear();
    }
    Ok(Value::NIL)
}

/// Context-aware variant of `(charset-after &optional POS)`.
///
/// Returns the charset of the character at POS (1-based), or the character
/// after point when POS is omitted. Returns nil at end-of-buffer or for
/// out-of-range numeric positions.
pub(crate) fn builtin_charset_after(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("charset-after", &args, 1)?;
    let buf = ctx
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    let Some(target_byte) = optional_charset_lisp_pos_to_byte(buf, args.first())? else {
        return Ok(Value::NIL);
    };

    let point_max_byte = buf.accessible_emacs_byte_region().end();
    if target_byte >= point_max_byte {
        return Ok(Value::NIL);
    }

    let Some(ch) = buf.char_after_emacs_byte_pos(target_byte) else {
        return Ok(Value::NIL);
    };
    let cp = ch as u32;
    // GNU `Fcharset_after` returns `CHAR_CHARSET (ch)`, which canonicalizes
    // every Unicode char to the dimension-3 `unicode` charset and never
    // surfaces the internal `unicode-bmp` subset (see `char_charset_name`).
    let charset = if (RAW_BYTE_SENTINEL_MIN..=RAW_BYTE_SENTINEL_MAX).contains(&cp) {
        "eight-bit"
    } else if (UNIBYTE_BYTE_SENTINEL_MIN..=UNIBYTE_BYTE_SENTINEL_MAX).contains(&cp) {
        let byte = cp - UNIBYTE_BYTE_SENTINEL_MIN;
        if byte <= 0x7F { "ascii" } else { "eight-bit" }
    } else {
        char_charset_name(cp as i64)
    };
    Ok(Value::symbol(charset))
}

fn classify_string_charsets(ls: &crate::heap_types::LispString) -> Vec<&'static str> {
    use crate::emacs_core::emacs_char;
    let bytes = ls.as_bytes();
    if bytes.is_empty() {
        return Vec::new();
    }

    // GNU `find_charsets_in_text` (charset.c:1487) records, per character, the
    // charset that `CHAR_CHARSET` resolves it to, then `find-charset-string`
    // (charset.c:1577) returns those charsets ordered by ascending charset id
    // (it iterates `charset_table` high -> low, consing each set entry, which
    // reverses to low -> high). We mirror that: classify each character through
    // the live ordered list (so `unicode-bmp` appears under a UTF-8 priority),
    // collect the distinct charsets, and sort by registry id. Issue #131: an
    // eight-bit raw byte is a byte8 char; a multibyte char is decoded from its
    // extended encoding.
    CHARSET_REGISTRY.with(|slot| {
        let reg = slot.borrow();
        let mut per_char: Vec<SymId> = Vec::new();
        if ls.is_multibyte() {
            let mut pos = 0usize;
            while pos < bytes.len() {
                let (cp, len) = emacs_char::string_char(&bytes[pos..]);
                pos += len;
                per_char.push(if emacs_char::char_byte8_p(cp) {
                    intern("eight-bit")
                } else {
                    reg.classify_char(cp as i64)
                });
            }
        } else {
            for &b in bytes {
                per_char.push(if b <= 0x7F {
                    intern("ascii")
                } else {
                    intern("eight-bit")
                });
            }
        }

        // Distinct charsets, ordered by ascending registry id (GNU's
        // `charset_table` iteration order).
        let mut found: Vec<(i64, SymId)> = Vec::new();
        for sym in per_char {
            if !found.iter().any(|&(_, existing)| existing == sym) {
                found.push((reg.id(sym).unwrap_or(i64::MAX), sym));
            }
        }
        found.sort_by_key(|&(id, _)| id);
        found.into_iter().map(|(_, sym)| resolve_sym(sym)).collect()
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
