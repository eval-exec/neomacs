//! Value-native Lisp reader.
//!
//! A mechanical translation of `parser.rs` that produces `Value` (tagged heap
//! pointers) directly instead of intermediate `Expr` AST nodes.
//!
//! Supports: integers, floats, strings (with escapes), symbols, keywords,
//! uninterned symbols (`#:foo`), character literals (?a), lists, dotted pairs,
//! vectors, quote ('), function (#'), backquote (`), unquote (,), splice (,@),
//! line comments (;), block comments (#|..|#), hash-table literals, records,
//! bool-vector literals, byte-code literals, char-table literals (#^[...] /
//! #^^[...]), read labels (#N= / #N#), radix integers (#x, #o, #b),
//! propertized strings, reader skip (#@N).

use super::eval::{push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots};
use super::intern::{intern, intern_lisp_string, intern_uninterned_lisp_string};
use malachite::base::num::conversion::traits::FromStringBase;
use malachite::integer::Integer;
// bytes_to_unibyte_storage_string and encode_nonunicode_char_for_storage
// imports removed — using emacs_char + Vec<u8> directly
use super::emacs_char;
use crate::buffer::{Buffer, EmacsByteLen, EmacsBytePos, EmacsByteRange};
use smallvec::SmallVec;
use std::cell::Cell;

use super::builtins::collections::lookup_hash_table_test_alias;
use super::value::{
    HashTableLiteralKey, HashTableTest, HashTableWeakness, StringTextPropertyRun, Value,
    build_hash_table_literal_value, eq_value, get_string_text_properties_for_value, list_to_vec,
    set_string_text_properties_for_value,
};

const UNICODE_CHARACTER_NAME_LENGTH_BOUND: usize = 200;
const CHAR_CODE_MASK: u32 = 0x3F_FFFF;
const CHAR_CTRL_MODIFIER: u32 = 1 << 26;
const CHAR_META_MODIFIER: u32 = 1 << 27;
const CHAR_SHIFT_MODIFIER: u32 = 1 << 25;

fn apply_control_modifier(value: u32) -> u32 {
    let code = value & CHAR_CODE_MASK;
    let modifiers = value & !CHAR_CODE_MASK;

    if code == b'?' as u32 {
        0x7F | modifiers
    } else if (b'@' as u32..=b'_' as u32).contains(&code)
        || (b'a' as u32..=b'z' as u32).contains(&code)
    {
        (code & 0x1F) | modifiers
    } else {
        code | modifiers | CHAR_CTRL_MODIFIER
    }
}

fn plist_get(plist: &[Value], key: HashTableLiteralKey) -> Option<Value> {
    let mut i = 0;
    while i + 1 < plist.len() {
        if HashTableLiteralKey::from_symbol_value(&plist[i]) == Some(key) {
            return Some(plist[i + 1]);
        }
        i += 2;
    }
    None
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Read all top-level forms from `input`, returning them as `Value`.
pub fn read_all(input: &str, obarray: &super::symbol::Obarray) -> Result<Vec<Value>, ReadError> {
    read_all_with_source_multibyte(input, true, obarray)
}

/// Read all top-level forms from `input`, preserving the source string's
/// multibyte/unibyte distinction where it affects reader results.
pub fn read_all_with_source_multibyte(
    input: &str,
    source_multibyte: bool,
    obarray: &super::symbol::Obarray,
) -> Result<Vec<Value>, ReadError> {
    let mut reader = Reader::new(
        input,
        ReaderSourceSemantics::for_lisp_object(source_multibyte),
        obarray,
    );
    let mut forms = Vec::new();
    while reader.skip_ws_and_comments() {
        forms.push(reader.read_form()?);
    }
    Ok(forms)
}

/// Read all top-level forms from a faithful Emacs-bytes `LispString`, returning
/// them as `Value`. Uses the LispString reader path (the same one the streaming
/// `.el` load loop uses), so non-Unicode source character literals keep their
/// real codes instead of round-tripping through the in-Unicode storage-string
/// form (issue #131).
pub fn read_all_lisp_source(
    input: &crate::heap_types::LispString,
    obarray: &super::symbol::Obarray,
) -> Result<Vec<Value>, ReadError> {
    let source = LispReadSource::new(input);
    let mut forms = Vec::new();
    let mut pos = 0;
    while let Some((form, next_pos)) = source.read_one(pos, obarray)? {
        forms.push(form);
        pos = next_pos;
    }
    Ok(forms)
}

/// Read a single form from `input` starting at byte offset `start`.
/// Returns `None` if there is nothing to read (only whitespace/comments remain).
/// On success returns `(value, end_position)`.
pub fn read_one(
    input: &str,
    start: usize,
    obarray: &super::symbol::Obarray,
) -> Result<Option<(Value, usize)>, ReadError> {
    read_one_with_source_multibyte(input, true, start, obarray)
}

/// Read a single form from `input`, preserving whether the original source was
/// multibyte or unibyte.
pub fn read_one_with_source_multibyte(
    input: &str,
    source_multibyte: bool,
    start: usize,
    obarray: &super::symbol::Obarray,
) -> Result<Option<(Value, usize)>, ReadError> {
    let mut reader = Reader::new(
        input,
        ReaderSourceSemantics::for_lisp_object(source_multibyte),
        obarray,
    );
    reader.pos = start;
    if !reader.skip_ws_and_comments() {
        return Ok(None);
    }
    let value = reader.read_form()?;
    Ok(Some((value, reader.pos)))
}

/// Read one form from the Latin-1 envelope used for compiled-file bytes.
///
/// This is intentionally separate from the unibyte string API above.  GNU
/// decodes file input but preserves each byte of unibyte Lisp objects.
pub(crate) fn read_one_from_encoded_file_bytes(
    input: &str,
    start: usize,
    obarray: &super::symbol::Obarray,
    shorthands: Option<&ReadSymbolShorthands>,
) -> Result<Option<(Value, usize)>, ReadError> {
    let mut reader = Reader::new(input, ReaderSourceSemantics::EncodedFileBytes, obarray);
    reader.shorthands = shorthands;
    reader.pos = start;
    if !reader.skip_ws_and_comments() {
        return Ok(None);
    }
    let value = reader.read_form()?;
    Ok(Some((value, reader.pos)))
}

/// Read a single form from `input`, optionally wrapping interned symbols in
/// `symbol-with-pos` objects that record GNU's character offset from the start
/// of this read.  Used by `read-positioning-symbols`.
pub fn read_one_with_locate_syms(
    input: &str,
    source_multibyte: bool,
    start: usize,
    locate_syms: bool,
    obarray: &super::symbol::Obarray,
) -> Result<Option<(Value, usize)>, ReadError> {
    let mut reader = Reader::new(
        input,
        ReaderSourceSemantics::for_lisp_object(source_multibyte),
        obarray,
    );
    reader.pos = start;
    reader.readchar_offset_origin = ReadcharOffsetOrigin::RelativeToSourceByte(start);
    reader.locate_syms = locate_syms;
    if !reader.skip_ws_and_comments() {
        return Ok(None);
    }
    let value = reader.read_form()?;
    Ok(Some((value, reader.pos)))
}

/// Read a single form directly from a buffer byte range.
///
/// GNU Emacs' reader installs buffer get/unget callbacks for buffer streams;
/// it does not copy the accessible region into a temporary string on each
/// `(read (current-buffer))`.  Keep the same model here so repeated buffer
/// reads, such as `unidata-gen.el`, advance through the original buffer.
pub(crate) fn read_one_from_buffer_with_locate_syms(
    buffer: &Buffer,
    range: EmacsByteRange,
    readchar_offset_origin: BufferReadcharOffsetOrigin,
    locate_syms: bool,
    obarray: &super::symbol::Obarray,
    shorthands: Option<&ReadSymbolShorthands>,
) -> Result<(Option<Value>, EmacsBytePos), ReadError> {
    let mut reader = Reader::new_buffer(buffer, range, readchar_offset_origin, obarray);
    reader.locate_syms = locate_syms;
    reader.shorthands = shorthands;
    if !reader.skip_ws_and_comments() {
        return Ok((None, EmacsBytePos::new(reader.pos)));
    }
    let value = reader.read_form()?;
    Ok((Some(value), EmacsBytePos::new(reader.pos)))
}

/// Initial value of GNU `lread.c`'s `readchar_offset` for a buffer-backed
/// reader.  A buffer stream starts at its absolute Lisp point, whereas a
/// marker stream starts at zero even though it reads the marker's buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BufferReadcharOffsetOrigin {
    BufferPoint,
    Zero,
}

/// Reader source wrapper for Lisp strings.
///
/// This keeps the runtime-storage adapter inside the reader boundary so callers
/// can work in logical Emacs-byte offsets instead of storage-string byte math.
pub struct LispReadSource<'a> {
    input: &'a crate::heap_types::LispString,
}

impl<'a> LispReadSource<'a> {
    pub fn new(input: &'a crate::heap_types::LispString) -> Self {
        Self { input }
    }

    pub fn is_multibyte(&self) -> bool {
        self.input.is_multibyte()
    }

    pub fn logical_len(&self) -> usize {
        self.input.sbytes()
    }

    pub fn storage_slice_range(&self, start: usize, end: usize) -> String {
        assert!(start <= end, "invalid LispReadSource range: {start}..{end}");
        assert!(
            end <= self.logical_len(),
            "LispReadSource end {end} exceeds logical length {}",
            self.logical_len()
        );
        let slice = self
            .input
            .slice(start, end)
            .expect("LispReadSource range should stay within source");
        // Issue #131: this slice is only used for human-readable load/trace form
        // previews, so a lossy UTF-8 rendering (real PUA preserved, eight-bit ->
        // U+FFFD) is correct and avoids the buggy storage-string sentinels.
        crate::emacs_core::emacs_char::to_utf8_lossy(slice.as_bytes())
    }

    pub fn read_one(
        &self,
        start: usize,
        obarray: &super::symbol::Obarray,
    ) -> Result<Option<(Value, usize)>, ReadError> {
        self.read_one_range(start, self.logical_len(), obarray)
    }

    pub(crate) fn read_one_with_shorthands(
        &self,
        start: usize,
        obarray: &super::symbol::Obarray,
        shorthands: Option<&ReadSymbolShorthands>,
    ) -> Result<Option<(Value, usize)>, ReadError> {
        let mut reader = Reader::new_lisp_string(self.input, start, self.logical_len(), obarray);
        reader.shorthands = shorthands;
        if !reader.skip_ws_and_comments() {
            return Ok(None);
        }
        let value = reader.read_form()?;
        Ok(Some((value, reader.pos)))
    }

    pub fn read_one_range(
        &self,
        start: usize,
        end: usize,
        obarray: &super::symbol::Obarray,
    ) -> Result<Option<(Value, usize)>, ReadError> {
        let mut reader = Reader::new_lisp_string(self.input, start, end, obarray);
        if !reader.skip_ws_and_comments() {
            return Ok(None);
        }
        let value = reader.read_form()?;
        Ok(Some((value, reader.pos)))
    }

    pub fn read_one_with_locate_syms(
        &self,
        start: usize,
        locate_syms: bool,
        obarray: &super::symbol::Obarray,
    ) -> Result<Option<(Value, usize)>, ReadError> {
        self.read_one_range_with_locate_syms(start, self.logical_len(), locate_syms, obarray, None)
    }

    pub(crate) fn read_one_range_with_locate_syms(
        &self,
        start: usize,
        end: usize,
        locate_syms: bool,
        obarray: &super::symbol::Obarray,
        shorthands: Option<&ReadSymbolShorthands>,
    ) -> Result<Option<(Value, usize)>, ReadError> {
        let mut reader = Reader::new_lisp_string(self.input, start, end, obarray);
        reader.locate_syms = locate_syms;
        reader.shorthands = shorthands;
        if !reader.skip_ws_and_comments() {
            return Ok(None);
        }
        let value = reader.read_form()?;
        Ok(Some((value, reader.pos)))
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ReadSymbolShorthands {
    pairs: Vec<(crate::heap_types::LispString, crate::heap_types::LispString)>,
}

impl ReadSymbolShorthands {
    pub(crate) fn from_lisp_value(value: Value) -> Option<Self> {
        let mut pairs = crate::emacs_core::value::list_to_vec(&value)?
            .into_iter()
            .filter_map(|entry| {
                if !entry.is_cons() {
                    return None;
                }
                let shorthand = entry.cons_car().as_lisp_string()?.clone();
                let longhand = entry.cons_cdr().as_lisp_string()?.clone();
                Some((shorthand, longhand))
            })
            .collect::<Vec<_>>();
        pairs.sort_by_key(|pair| std::cmp::Reverse(pair.0.sbytes()));
        Some(Self { pairs })
    }

    fn rewrite(
        &self,
        name: &crate::heap_types::LispString,
    ) -> Option<crate::heap_types::LispString> {
        let input = name.as_bytes();
        for (short, long) in &self.pairs {
            let short_bytes = short.as_bytes();
            if input.starts_with(short_bytes) {
                let mut rewritten = long.as_bytes().to_vec();
                rewritten.extend_from_slice(&input[short_bytes.len()..]);
                return Some(if name.is_multibyte() || long.is_multibyte() {
                    crate::heap_types::LispString::from_emacs_bytes(rewritten)
                } else {
                    crate::heap_types::LispString::from_unibyte(rewritten)
                });
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ReadError {
    pub message: String,
    pub position: usize,
    pub kind: ReadErrorKind,
    pub signal_symbol: Option<String>,
    pub signal_data: Vec<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadErrorKind {
    InvalidReadSyntax,
    EndOfFile,
    Error,
    Signal,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "read error at {}: {}", self.position, self.message)
    }
}

impl std::error::Error for ReadError {}

// ---------------------------------------------------------------------------
// Reader struct
// ---------------------------------------------------------------------------

enum ReaderSource<'a> {
    Runtime(&'a str),
    LispString(&'a crate::heap_types::LispString),
    Buffer(&'a Buffer),
}

/// How a reader source exposes non-ASCII input to the Lisp parser.
///
/// GNU `lread.c` has separate `source_*_get` functions, so these states cannot
/// be confused there:
///
/// - multibyte strings/buffers yield decoded Emacs characters;
/// - unibyte strings/buffers yield one BYTE8 character per high byte;
/// - encoded file input decodes valid multibyte byte sequences.
///
/// Neomacs used to represent all three with one `source_multibyte` boolean.
/// The compiled-file compatibility path then decoded valid UTF-8-looking byte
/// runs for unibyte Lisp strings and buffers too.  Keep the third state
/// explicit so file decoding cannot silently leak into object readers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReaderSourceSemantics {
    MultibyteCharacters,
    UnibyteCharacters,
    EncodedFileBytes,
}

impl ReaderSourceSemantics {
    const fn for_lisp_object(multibyte: bool) -> Self {
        if multibyte {
            Self::MultibyteCharacters
        } else {
            Self::UnibyteCharacters
        }
    }

    const fn is_multibyte(self) -> bool {
        matches!(self, Self::MultibyteCharacters)
    }
}

struct Reader<'a> {
    source: ReaderSource<'a>,
    source_semantics: ReaderSourceSemantics,
    pos: usize,
    limit: usize,
    /// `#N=EXPR` / `#N#` read labels for shared structure in `.elc` files.
    read_labels: std::collections::HashMap<usize, Value>,
    /// When true, wrap interned symbols in symbol-with-pos objects.
    locate_syms: bool,
    /// Coordinate origin for `symbol-with-pos-pos`.  GNU initializes its
    /// character counter to the absolute point for buffer streams and zero
    /// for strings, markers, functions, and files.
    readchar_offset_origin: ReadcharOffsetOrigin,
    /// The active obarray, for resolving `#$` to the current value of
    /// `load-file-name` (matching GNU's `Vload_file_name`).
    obarray: &'a super::symbol::Obarray,
    /// Active `read-symbol-shorthands`, if any.  GNU's reader rewrites symbol
    /// names before interning when this dynamic variable is non-nil.
    shorthands: Option<&'a ReadSymbolShorthands>,
    /// One-entry decode cache: `(pos, code, next_pos)` for the most recently
    /// decoded position.  neomacs's reader peeks the current char
    /// (`source_code_at`) then advances (`next_pos`) as separate steps, which
    /// otherwise decodes the same multibyte char twice (each decode reads
    /// bytes one at a time via `emacs_byte_at_pos`).  This cache makes the
    /// peek+advance pair decode each position once.  GNU avoids the double
    /// decode differently -- `readchar` reads-and-advances in one step -- but
    /// GNU's `unreadchar` still re-decodes pushed-back chars (it only backs the
    /// position up); this pos cache does not.  Safe because the source bytes at
    /// a given position are immutable for the reader's lifetime.
    step_cache: Cell<Option<(usize, u32, usize)>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadcharOffsetOrigin {
    RelativeToSourceByte(usize),
    AbsoluteBufferPosition,
}

struct ReaderToken {
    /// Raw token bytes, kept inline (SmallVec) so the common short-symbol case
    /// never touches the heap. The `LispString` name is materialized lazily by
    /// `to_lisp_string()` only on the cold paths (uninterned `#:` symbols,
    /// non-ASCII / escaped tokens, shorthand rewrite); ASCII symbols intern
    /// straight from the `&str` view via the interner's no-alloc fast path.
    bytes: ReaderTokenBytes,
    /// Whether the source string was multibyte when the token was read; decides
    /// how `to_lisp_string` reconstructs the exact Lisp-string encoding.
    multibyte: bool,
    had_escape: bool,
}

impl ReaderToken {
    /// UTF-8 view of the token bytes if valid. Always true for ASCII; for a
    /// multibyte source the bytes are Emacs-internal encoding, so non-ASCII
    /// returns `None` and the caller falls back to the `LispString` path.
    fn as_utf8(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes).ok()
    }

    /// Materialize the exact Lisp-string name (heap). Reconstructs the same
    /// encoding `read_symbol_token` used to before this was made lazy.
    fn to_lisp_string(&self) -> crate::heap_types::LispString {
        if self.multibyte {
            crate::heap_types::LispString::from_emacs_bytes(self.bytes.to_vec())
        } else {
            crate::heap_types::LispString::from_unibyte(self.bytes.to_vec())
        }
    }
}

type ReaderTokenBytes = SmallVec<[u8; 64]>;

fn substitute_read_placeholder(object: Value, placeholder: Value) {
    let mut seen = Vec::new();
    let result = substitute_read_placeholder_recurse(object, placeholder, object, &mut seen);
    debug_assert!(eq_value(&result, &object));
}

fn substitute_read_placeholder_recurse(
    object: Value,
    placeholder: Value,
    subtree: Value,
    seen: &mut Vec<Value>,
) -> Value {
    if eq_value(&subtree, &placeholder) {
        return object;
    }

    if subtree.is_symbol() || subtree.is_number() {
        return subtree;
    }

    if seen.iter().any(|seen| eq_value(seen, &subtree)) {
        return subtree;
    }
    seen.push(subtree);

    if subtree.is_string() {
        if let Some(runs) = get_string_text_properties_for_value(subtree) {
            let remapped = runs
                .into_iter()
                .map(|mut run| {
                    run.plist =
                        substitute_read_placeholder_recurse(object, placeholder, run.plist, seen);
                    run
                })
                .collect();
            set_string_text_properties_for_value(subtree, remapped);
        }
        return subtree;
    }

    if subtree.is_cons() {
        let car =
            substitute_read_placeholder_recurse(object, placeholder, subtree.cons_car(), seen);
        let cdr =
            substitute_read_placeholder_recurse(object, placeholder, subtree.cons_cdr(), seen);
        subtree.set_car(car);
        subtree.set_cdr(cdr);
    } else if subtree.is_vector() {
        if let Some(items) = subtree.as_vector_data() {
            let mut items = items.clone();
            for item in &mut items {
                *item = substitute_read_placeholder_recurse(object, placeholder, *item, seen);
            }
            let _ = subtree.replace_vector_data(items);
        }
    } else if subtree.is_record()
        && let Some(items) = subtree.as_record_data()
    {
        let mut items = items.clone();
        for item in &mut items {
            *item = substitute_read_placeholder_recurse(object, placeholder, *item, seen);
        }
        let _ = subtree.replace_record_data(items);
    }

    subtree
}

impl<'a> Reader<'a> {
    fn new(
        input: &'a str,
        source_semantics: ReaderSourceSemantics,
        obarray: &'a super::symbol::Obarray,
    ) -> Self {
        Self {
            source: ReaderSource::Runtime(input),
            source_semantics,
            pos: 0,
            limit: input.len(),
            read_labels: std::collections::HashMap::new(),
            locate_syms: false,
            readchar_offset_origin: ReadcharOffsetOrigin::RelativeToSourceByte(0),
            obarray,
            shorthands: None,
            step_cache: Cell::new(None),
        }
    }

    fn new_lisp_string(
        input: &'a crate::heap_types::LispString,
        start: usize,
        end: usize,
        obarray: &'a super::symbol::Obarray,
    ) -> Self {
        assert!(start <= end, "invalid Lisp reader range: {start}..{end}");
        assert!(
            end <= input.sbytes(),
            "Lisp reader end {end} exceeds logical length {}",
            input.sbytes()
        );
        Self {
            source: ReaderSource::LispString(input),
            source_semantics: ReaderSourceSemantics::for_lisp_object(input.is_multibyte()),
            pos: start,
            limit: end,
            read_labels: std::collections::HashMap::new(),
            locate_syms: false,
            readchar_offset_origin: ReadcharOffsetOrigin::RelativeToSourceByte(start),
            obarray,
            shorthands: None,
            step_cache: Cell::new(None),
        }
    }

    fn new_buffer(
        input: &'a Buffer,
        range: EmacsByteRange,
        readchar_offset_origin: BufferReadcharOffsetOrigin,
        obarray: &'a super::symbol::Obarray,
    ) -> Self {
        let start = range.start().get();
        let end = range.end().get();
        assert!(start <= end, "invalid buffer reader range: {start}..{end}");
        let input_len = input.total_emacs_byte_len().get();
        assert!(
            end <= input_len,
            "buffer reader end {end} exceeds logical length {input_len}"
        );
        Self {
            source: ReaderSource::Buffer(input),
            source_semantics: ReaderSourceSemantics::for_lisp_object(input.get_multibyte()),
            pos: start,
            limit: end,
            read_labels: std::collections::HashMap::new(),
            locate_syms: false,
            readchar_offset_origin: match readchar_offset_origin {
                BufferReadcharOffsetOrigin::BufferPoint => {
                    ReadcharOffsetOrigin::AbsoluteBufferPosition
                }
                BufferReadcharOffsetOrigin::Zero => {
                    ReadcharOffsetOrigin::RelativeToSourceByte(start)
                }
            },
            obarray,
            shorthands: None,
            step_cache: Cell::new(None),
        }
    }

    // -- Whitespace & comments -----------------------------------------------

    fn skip_ws_and_comments(&mut self) -> bool {
        loop {
            let Some(ch) = self.current_code() else {
                return false;
            };
            if is_reader_whitespace_code(ch) {
                self.bump();
                continue;
            }
            if ch == b';' as u32 {
                // Line comment
                while let Some(c) = self.current_code() {
                    self.bump();
                    if c == b'\n' as u32 {
                        break;
                    }
                }
                continue;
            }
            if ch == b'#' as u32 && self.peek_code_at(1) == Some(b'|' as u32) {
                // Block comment #| ... |#
                self.bump(); // #
                self.bump(); // |
                let mut depth = 1;
                while depth > 0 {
                    match self.current_code() {
                        None => return false,
                        Some(code)
                            if code == b'#' as u32 && self.peek_code_at(1) == Some(b'|' as u32) =>
                        {
                            self.bump();
                            self.bump();
                            depth += 1;
                        }
                        Some(code)
                            if code == b'|' as u32 && self.peek_code_at(1) == Some(b'#' as u32) =>
                        {
                            self.bump();
                            self.bump();
                            depth -= 1;
                        }
                        _ => self.bump(),
                    }
                }
                continue;
            }
            return true;
        }
    }

    // -- Main read dispatch --------------------------------------------------

    fn read_form(&mut self) -> Result<Value, ReadError> {
        self.skip_ws_and_comments();
        // Record the byte position before reading — used by locate_syms
        // to tag symbols with their source offset (mirrors GNU read0).
        let form_start = self.pos;
        let Some(ch) = self.current_code() else {
            return Err(self.end_of_file_error());
        };

        let value = match ch {
            x if x == b'(' as u32 => self.read_list_or_dotted(),
            x if x == b')' as u32 => {
                self.bump();
                Err(self.error(")"))
            }
            x if x == b'[' as u32 => self.read_vector(),
            x if x == b'\'' as u32 => {
                self.bump();
                let saved = save_scratch_gc_roots();
                let quoted = self.read_form()?;
                push_scratch_gc_root(quoted);
                let result = Value::list(vec![Value::symbol("quote"), quoted]);
                restore_scratch_gc_roots(saved);
                Ok(result)
            }
            x if x == b'`' as u32 => {
                self.bump();
                let saved = save_scratch_gc_roots();
                let quoted = self.read_form()?;
                push_scratch_gc_root(quoted);
                let result = Value::list(vec![Value::symbol(intern("`")), quoted]);
                restore_scratch_gc_roots(saved);
                Ok(result)
            }
            x if x == b',' as u32 => {
                self.bump();
                if self.current_code() == Some(b'@' as u32) {
                    self.bump();
                    let saved = save_scratch_gc_roots();
                    let expr = self.read_form()?;
                    push_scratch_gc_root(expr);
                    let result = Value::list(vec![Value::symbol(intern(",@")), expr]);
                    restore_scratch_gc_roots(saved);
                    Ok(result)
                } else {
                    let saved = save_scratch_gc_roots();
                    let expr = self.read_form()?;
                    push_scratch_gc_root(expr);
                    let result = Value::list(vec![Value::symbol(intern(",")), expr]);
                    restore_scratch_gc_roots(saved);
                    Ok(result)
                }
            }
            x if x == b'"' as u32 => self.read_string(),
            x if x == b'?' as u32 => self.read_char_literal(),
            x if x == b'#' as u32 => self.read_hash_syntax(),
            _ => self.read_atom(),
        }?;

        // Wrap symbols with their source position when locate_syms is active.
        // Matches GNU read0: SYMBOLP(val) && !NILP(val).
        if self.locate_syms && value.is_symbol() && !value.is_nil() {
            let pos_val = Value::fixnum(self.symbol_position(form_start) as i64);
            Ok(crate::tagged::gc::with_tagged_heap(|heap| {
                heap.alloc_symbol_with_pos(value, pos_val)
            }))
        } else {
            Ok(value)
        }
    }

    /// Translate the reader's internal byte cursor into GNU's source-specific
    /// character coordinate for a symbol-with-position object.
    fn symbol_position(&self, source_byte: usize) -> usize {
        match self.readchar_offset_origin {
            ReadcharOffsetOrigin::RelativeToSourceByte(origin) => {
                self.source_character_distance(origin, source_byte)
            }
            ReadcharOffsetOrigin::AbsoluteBufferPosition => match self.source {
                ReaderSource::Buffer(buffer) => buffer
                    .emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(source_byte))
                    .as_i64() as usize,
                _ => unreachable!("absolute readchar offsets require a buffer source"),
            },
        }
    }

    fn source_character_distance(&self, start: usize, end: usize) -> usize {
        debug_assert!(start <= end);
        match self.source {
            ReaderSource::Runtime(_) => {
                let mut byte = start;
                let mut chars = 0;
                while byte < end {
                    byte = self
                        .next_pos(byte)
                        .expect("symbol position must be on a source character boundary");
                    chars += 1;
                }
                debug_assert_eq!(byte, end);
                chars
            }
            ReaderSource::LispString(input) => {
                if self.source_semantics.is_multibyte() {
                    emacs_char::byte_to_char_pos(input.as_bytes(), end)
                        - emacs_char::byte_to_char_pos(input.as_bytes(), start)
                } else {
                    end - start
                }
            }
            ReaderSource::Buffer(buffer) => {
                buffer
                    .emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(end))
                    .as_i64() as usize
                    - buffer
                        .emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(start))
                        .as_i64() as usize
            }
        }
    }

    // -- Lists and dotted pairs ----------------------------------------------

    fn read_list_or_dotted(&mut self) -> Result<Value, ReadError> {
        self.expect('(')?;
        let saved = save_scratch_gc_roots();
        let mut items = Vec::new();
        loop {
            self.skip_ws_and_comments();
            match self.current_code() {
                Some(code) if code == b')' as u32 => {
                    self.bump();
                    let result = Value::list(items);
                    restore_scratch_gc_roots(saved);
                    return Ok(result);
                }
                Some(code) if code == b'.' as u32 && self.is_dot_separator() => {
                    if items.is_empty() {
                        self.bump();
                        restore_scratch_gc_roots(saved);
                        return Err(self.error("."));
                    }
                    // Dotted pair
                    self.bump(); // consume '.'
                    let cdr = self.read_form()?;
                    push_scratch_gc_root(cdr);
                    self.skip_ws_and_comments();
                    match self.current_code() {
                        Some(code) if code == b')' as u32 => {
                            self.bump();
                            // Build cons chain: (a b c . d)
                            // items = [a, b, c], cdr = d
                            let mut acc = cdr;
                            for item in items.into_iter().rev() {
                                acc = Value::cons(item, acc);
                                push_scratch_gc_root(acc);
                            }
                            restore_scratch_gc_roots(saved);
                            return Ok(acc);
                        }
                        _ => {
                            restore_scratch_gc_roots(saved);
                            return Err(self.error("expected ')' after dotted pair"));
                        }
                    }
                }
                Some(_) => {
                    let item = self.read_form()?;
                    push_scratch_gc_root(item);
                    items.push(item);
                }
                None => {
                    restore_scratch_gc_roots(saved);
                    return Err(self.end_of_file_error());
                }
            }
        }
    }

    /// Check if current '.' is a dot separator (not part of a number like 1.5).
    ///
    /// GNU lread.c treats dot as a dotted-tail marker when the following
    /// character starts a new reader token, including reader-prefix characters
    /// such as comma and backquote.
    fn is_dot_separator(&self) -> bool {
        match self.peek_code_at(1) {
            None => true,
            Some(c) => {
                is_reader_whitespace_code(c)
                    || c == b'"' as u32
                    || c == b'\'' as u32
                    || c == b'(' as u32
                    || c == b';' as u32
                    || c == b'[' as u32
                    || c == b'#' as u32
                    || c == b'?' as u32
                    || c == b'`' as u32
                    || c == b',' as u32
            }
        }
    }

    // -- Vectors [1 2 3] ----------------------------------------------------

    /// Read `[...]` and return items as a Vec<Value>.
    fn read_vector_items(&mut self) -> Result<Vec<Value>, ReadError> {
        self.expect('[')?;
        let saved = save_scratch_gc_roots();
        let mut items = Vec::new();
        loop {
            self.skip_ws_and_comments();
            match self.current_code() {
                Some(code) if code == b']' as u32 => {
                    self.bump();
                    restore_scratch_gc_roots(saved);
                    return Ok(items);
                }
                Some(_) => {
                    let item = self.read_form()?;
                    push_scratch_gc_root(item);
                    items.push(item);
                }
                None => {
                    restore_scratch_gc_roots(saved);
                    return Err(self.end_of_file_error());
                }
            }
        }
    }

    fn read_vector(&mut self) -> Result<Value, ReadError> {
        let saved = save_scratch_gc_roots();
        let items = self.read_vector_items()?;
        for item in &items {
            push_scratch_gc_root(*item);
        }
        let result = Value::make_vector(items);
        restore_scratch_gc_roots(saved);
        Ok(result)
    }

    // -- Strings "..." -------------------------------------------------------

    fn read_string(&mut self) -> Result<Value, ReadError> {
        self.expect('"')?;
        let mut buf = Vec::new();
        // GNU `lread.c:3043-3142` keeps ASCII-only and raw-byte string literals
        // unibyte unless a real multibyte character is forced while reading.
        let mut unibyte_buf = Some(Vec::new());
        loop {
            let Some(ch) = self.current_code() else {
                return Err(self.end_of_file_error());
            };
            self.bump();
            match ch {
                x if x == b'"' as u32 => {
                    let string = if let Some(bytes) = unibyte_buf {
                        crate::heap_types::LispString::from_unibyte(bytes)
                    } else {
                        maybe_recombine_latin1_emacs(buf)
                    };
                    return Ok(Value::heap_string(string));
                }
                x if x == b'\\' as u32 => {
                    let Some(esc) = self.current_code() else {
                        return Err(self.end_of_file_error());
                    };
                    self.bump();
                    match esc {
                        x if x == b'n' as u32 => {
                            buf.push(b'\n');
                            if let Some(bytes) = unibyte_buf.as_mut() {
                                bytes.push(b'\n');
                            }
                        }
                        x if x == b'r' as u32 => {
                            buf.push(b'\r');
                            if let Some(bytes) = unibyte_buf.as_mut() {
                                bytes.push(b'\r');
                            }
                        }
                        x if x == b't' as u32 => {
                            buf.push(b'\t');
                            if let Some(bytes) = unibyte_buf.as_mut() {
                                bytes.push(b'\t');
                            }
                        }
                        x if x == b'\\' as u32 => {
                            buf.push(b'\\');
                            if let Some(bytes) = unibyte_buf.as_mut() {
                                bytes.push(b'\\');
                            }
                        }
                        x if x == b'"' as u32 => {
                            buf.push(b'"');
                            if let Some(bytes) = unibyte_buf.as_mut() {
                                bytes.push(b'"');
                            }
                        }
                        x if x == b'a' as u32 => {
                            buf.push(0x07);
                            if let Some(bytes) = unibyte_buf.as_mut() {
                                bytes.push(0x07);
                            }
                        }
                        x if x == b'b' as u32 => {
                            buf.push(0x08);
                            if let Some(bytes) = unibyte_buf.as_mut() {
                                bytes.push(0x08);
                            }
                        }
                        x if x == b'f' as u32 => {
                            buf.push(0x0C);
                            if let Some(bytes) = unibyte_buf.as_mut() {
                                bytes.push(0x0C);
                            }
                        }
                        x if x == b'e' as u32 => {
                            buf.push(0x1B);
                            if let Some(bytes) = unibyte_buf.as_mut() {
                                bytes.push(0x1B);
                            }
                        }
                        x if x == b'v' as u32 => {
                            buf.push(0x0B);
                            if let Some(bytes) = unibyte_buf.as_mut() {
                                bytes.push(0x0B);
                            }
                        }
                        x if x == b's' as u32 => {
                            buf.push(b' ');
                            if let Some(bytes) = unibyte_buf.as_mut() {
                                bytes.push(b' ');
                            }
                        }
                        // Modifier escapes in strings
                        x if x == b'C' as u32 && self.current_code() == Some(b'-' as u32) => {
                            self.bump(); // consume '-'
                            let base = self.parse_string_char_value(0)?;
                            self.push_string_escape_value(
                                &mut buf,
                                &mut unibyte_buf,
                                apply_control_modifier(base),
                            )?;
                        }
                        x if x == b'^' as u32 => {
                            let base = self.parse_string_char_value(0)?;
                            self.push_string_escape_value(
                                &mut buf,
                                &mut unibyte_buf,
                                apply_control_modifier(base),
                            )?;
                        }
                        x if x == b'M' as u32 && self.current_code() == Some(b'-' as u32) => {
                            self.bump(); // consume '-'
                            let val = self.parse_string_char_value(CHAR_META_MODIFIER)?;
                            self.push_string_escape_value(&mut buf, &mut unibyte_buf, val)?;
                        }
                        x if x == b'S' as u32 && self.current_code() == Some(b'-' as u32) => {
                            self.bump(); // consume '-'
                            let val = self.parse_string_char_value(CHAR_SHIFT_MODIFIER)?;
                            self.push_string_escape_value(&mut buf, &mut unibyte_buf, val)?;
                        }
                        x if x == b'A' as u32 && self.current_code() == Some(b'-' as u32) => {
                            self.bump(); // consume '-'
                            let val = self.parse_string_char_value(1 << 22)?;
                            self.push_string_escape_value(&mut buf, &mut unibyte_buf, val)?;
                        }
                        x if x == b'H' as u32 && self.current_code() == Some(b'-' as u32) => {
                            self.bump(); // consume '-'
                            let val = self.parse_string_char_value(1 << 24)?;
                            self.push_string_escape_value(&mut buf, &mut unibyte_buf, val)?;
                        }
                        x if x == b'd' as u32 => {
                            buf.push(0x7F);
                            if let Some(bytes) = unibyte_buf.as_mut() {
                                bytes.push(0x7F);
                            }
                        }
                        x if x == b'x' as u32 => {
                            let (mut hex, digit_count) = self.read_hex_digits()?;
                            if hex <= emacs_char::MAX_CHAR {
                                if digit_count < 3 && (0x80..0x100).contains(&hex) {
                                    hex = emacs_char::byte8_to_char(hex as u8);
                                }
                                let mut tmp = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
                                let len = emacs_char::char_string(hex, &mut tmp);
                                buf.extend_from_slice(&tmp[..len]);
                                if let Some(bytes) = unibyte_buf.as_mut() {
                                    if let Some(byte) = emacs_char::char_to_byte_safe(hex) {
                                        bytes.push(byte);
                                    } else {
                                        unibyte_buf = None;
                                    }
                                }
                            } else {
                                return Err(self.error(
                                    "invalid codepoint in \\x escape (exceeds Emacs 22-bit limit)",
                                ));
                            }
                        }
                        x if x == b'u' as u32 => {
                            let hex = self.read_fixed_hex(4)?;
                            let mut tmp = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
                            let len = emacs_char::char_string(hex, &mut tmp);
                            buf.extend_from_slice(&tmp[..len]);
                            if let Some(bytes) = unibyte_buf.as_mut() {
                                if hex < 0x80 {
                                    bytes.push(hex as u8);
                                } else {
                                    unibyte_buf = None;
                                }
                            }
                        }
                        x if x == b'U' as u32 => {
                            let hex = self.read_fixed_hex(8)?;
                            if hex <= emacs_char::MAX_CHAR {
                                let mut tmp = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
                                let len = emacs_char::char_string(hex, &mut tmp);
                                buf.extend_from_slice(&tmp[..len]);
                                if let Some(bytes) = unibyte_buf.as_mut() {
                                    if hex < 0x80 {
                                        bytes.push(hex as u8);
                                    } else {
                                        unibyte_buf = None;
                                    }
                                }
                            } else {
                                return Err(self.error("invalid unicode codepoint in \\U escape"));
                            }
                        }
                        x if x == b'N' as u32 && self.current_code() == Some(b'{' as u32) => {
                            let value = self.read_unicode_name_escape()?;
                            if let Some(c) = char::from_u32(value) {
                                let mut tmp = [0u8; 4];
                                buf.extend_from_slice(c.encode_utf8(&mut tmp).as_bytes());
                                if let Some(bytes) = unibyte_buf.as_mut() {
                                    if value < 0x80 {
                                        bytes.push(value as u8);
                                    } else {
                                        unibyte_buf = None;
                                    }
                                }
                            } else {
                                return Err(self.error("invalid unicode codepoint in \\N{...}"));
                            }
                        }
                        x if (b'0' as u32..=b'7' as u32).contains(&x) => {
                            // Octal escape
                            let mut val = esc - b'0' as u32;
                            for _ in 0..2 {
                                match self.current_code() {
                                    Some(c) if (b'0' as u32..=b'7' as u32).contains(&c) => {
                                        self.bump();
                                        val = val * 8 + (c - b'0' as u32);
                                    }
                                    _ => break,
                                }
                            }
                            if val <= emacs_char::MAX_CHAR {
                                if (0x80..0x100).contains(&val) {
                                    val = emacs_char::byte8_to_char(val as u8);
                                }
                                let mut tmp = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
                                let len = emacs_char::char_string(val, &mut tmp);
                                buf.extend_from_slice(&tmp[..len]);
                                if let Some(bytes) = unibyte_buf.as_mut() {
                                    if let Some(byte) = emacs_char::char_to_byte_safe(val) {
                                        bytes.push(byte);
                                    } else {
                                        unibyte_buf = None;
                                    }
                                }
                            }
                        }
                        x if x == b'\n' as u32 || x == b' ' as u32 => {
                            // `\<LF>` and `\<SPC>` generate no characters at
                            // all (line/whitespace continuation). Matches GNU
                            // `read_string_literal` in `src/lread.c`:
                            //   case ' ': case '\n': ... continue;
                            // Note: `\<TAB>`/`\<CR>` are NOT dropped by GNU;
                            // they fall through to `read_char_escape` and keep
                            // the literal char, so they stay in `other`.
                        }
                        other => {
                            // Unknown escape — keep the escaped source character.
                            Self::push_string_char(&mut buf, &mut unibyte_buf, other);
                        }
                    }
                }
                other => {
                    self.push_source_string_code(&mut buf, &mut unibyte_buf, other);
                }
            }
        }
    }

    /// Parse the next character in a string, applying accumulated modifiers.
    /// Handles recursive modifiers (e.g. `\M-\C-x`) and escape sequences.
    fn parse_string_char_value(&mut self, modifiers: u32) -> Result<u32, ReadError> {
        let Some(ch) = self.current_code() else {
            return Err(self.error("expected character after modifier escape in string"));
        };
        self.bump();
        if ch == b'\\' as u32 {
            let Some(esc) = self.current_code() else {
                return Err(self.error("unterminated escape in string modifier"));
            };
            self.bump();
            match esc {
                x if x == b'C' as u32 && self.current_code() == Some(b'-' as u32) => {
                    self.bump();
                    let base = self.parse_string_char_value(modifiers)?;
                    Ok(apply_control_modifier(base))
                }
                x if x == b'M' as u32 && self.current_code() == Some(b'-' as u32) => {
                    self.bump();
                    self.parse_string_char_value(modifiers | CHAR_META_MODIFIER)
                }
                x if x == b'S' as u32 && self.current_code() == Some(b'-' as u32) => {
                    self.bump();
                    self.parse_string_char_value(modifiers | CHAR_SHIFT_MODIFIER)
                }
                x if x == b's' as u32 && self.current_code() == Some(b'-' as u32) => {
                    self.bump();
                    self.parse_string_char_value(modifiers | (1 << 23))
                }
                x if x == b'A' as u32 && self.current_code() == Some(b'-' as u32) => {
                    self.bump();
                    self.parse_string_char_value(modifiers | (1 << 22))
                }
                x if x == b'H' as u32 && self.current_code() == Some(b'-' as u32) => {
                    self.bump();
                    self.parse_string_char_value(modifiers | (1 << 24))
                }
                x if x == b'n' as u32 => Ok(b'\n' as u32 | modifiers),
                x if x == b'r' as u32 => Ok(b'\r' as u32 | modifiers),
                x if x == b't' as u32 => Ok(b'\t' as u32 | modifiers),
                x if x == b'a' as u32 => Ok(0x07 | modifiers),
                x if x == b'b' as u32 => Ok(0x08 | modifiers),
                x if x == b'f' as u32 => Ok(0x0C | modifiers),
                x if x == b'v' as u32 => Ok(0x0B | modifiers),
                x if x == b'e' as u32 => Ok(0x1B | modifiers),
                x if x == b's' as u32 => Ok(b' ' as u32 | modifiers),
                x if x == b'd' as u32 => Ok(0x7F | modifiers),
                x if x == b'\\' as u32 => Ok(b'\\' as u32 | modifiers),
                x if x == b'"' as u32 => Ok(b'"' as u32 | modifiers),
                x if x == b'N' as u32 && self.current_code() == Some(b'{' as u32) => {
                    Ok(self.read_unicode_name_escape()? | modifiers)
                }
                x if x == b'^' as u32 => {
                    let base = self.parse_string_char_value(modifiers)?;
                    Ok(apply_control_modifier(base))
                }
                other => Ok(other | modifiers),
            }
        } else {
            Ok(ch | modifiers)
        }
    }

    fn push_string_char(buf: &mut Vec<u8>, unibyte_buf: &mut Option<Vec<u8>>, code: u32) {
        if emacs_char::char_byte8_p(code) {
            let byte = emacs_char::char_to_byte8(code);
            let mut tmp = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
            let len = emacs_char::char_string(code, &mut tmp);
            buf.extend_from_slice(&tmp[..len]);
            if let Some(bytes) = unibyte_buf.as_mut() {
                bytes.push(byte);
            }
            return;
        }

        if code < 0x80 {
            buf.push(code as u8);
            if let Some(bytes) = unibyte_buf.as_mut() {
                bytes.push(code as u8);
            }
            return;
        }

        let mut tmp = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = emacs_char::char_string(code, &mut tmp);
        buf.extend_from_slice(&tmp[..len]);
        *unibyte_buf = None;
    }

    fn push_string_escape_value(
        &self,
        buf: &mut Vec<u8>,
        unibyte_buf: &mut Option<Vec<u8>>,
        val: u32,
    ) -> Result<(), ReadError> {
        let mut modifiers = val & !CHAR_CODE_MASK;
        let mut code = val & CHAR_CODE_MASK;

        if !emacs_char::char_byte8_p(code) && code < 0x80 {
            if modifiers == CHAR_CTRL_MODIFIER && code == ' ' as u32 {
                code = 0;
                modifiers = 0;
            }

            if (modifiers & CHAR_SHIFT_MODIFIER) != 0 {
                if ('A' as u32..='Z' as u32).contains(&code) {
                    modifiers &= !CHAR_SHIFT_MODIFIER;
                } else if ('a' as u32..='z' as u32).contains(&code) {
                    code -= 'a' as u32 - 'A' as u32;
                    modifiers &= !CHAR_SHIFT_MODIFIER;
                }
            }

            if (modifiers & CHAR_META_MODIFIER) != 0 {
                modifiers &= !CHAR_META_MODIFIER;
                code = emacs_char::unibyte_to_char((code as u8) | 0x80);
            }
        }

        if modifiers != 0 {
            return Err(self.error("Invalid modifier in string"));
        }

        Self::push_string_char(buf, unibyte_buf, code);
        Ok(())
    }

    fn push_source_string_code(
        &mut self,
        buf: &mut Vec<u8>,
        unibyte_buf: &mut Option<Vec<u8>>,
        code: u32,
    ) {
        if self.source_semantics == ReaderSourceSemantics::UnibyteCharacters
            && (0x80..=0xFF).contains(&code)
        {
            // GNU source_string_get/source_buffer_get expose high bytes as
            // BYTE8 characters.  Feeding that domain value into the shared
            // string builder preserves the raw byte and keeps the result
            // unibyte, even when adjacent bytes happen to form valid UTF-8.
            Self::push_string_char(buf, unibyte_buf, emacs_char::byte8_to_char(code as u8));
            return;
        }

        if self.source_semantics == ReaderSourceSemantics::EncodedFileBytes
            && (0x80..=0xFF).contains(&code)
        {
            // Compiled-file bytes arrive through a Latin-1 `&str` envelope.
            // GNU's file source decodes valid multibyte byte runs before the
            // string parser, so reconstruct those runs only in this state.
            let byte0 = code as u8;
            let decoded = if byte0 >= 0xC0 {
                let expected_len = if byte0 < 0xE0 {
                    2
                } else if byte0 < 0xF0 {
                    3
                } else if byte0 < 0xF8 {
                    4
                } else {
                    0
                };
                if expected_len >= 2 {
                    let save_pos = self.pos;
                    let mut utf8_bytes = vec![byte0];
                    let mut ok = true;
                    for _ in 1..expected_len {
                        match self.current_code() {
                            Some(c) if (0x80..=0xBF).contains(&c) => {
                                utf8_bytes.push(c as u8);
                                self.bump();
                            }
                            _ => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        if let Ok(s) = std::str::from_utf8(&utf8_bytes) {
                            s.chars().next().map(|ch| ch as u32)
                        } else {
                            self.pos = save_pos;
                            None
                        }
                    } else {
                        self.pos = save_pos;
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(decoded_code) = decoded {
                Self::push_string_char(buf, unibyte_buf, decoded_code);
            } else {
                buf.push(byte0);
                if let Some(bytes) = unibyte_buf.as_mut() {
                    bytes.push(byte0);
                }
            }
            return;
        }

        Self::push_string_char(buf, unibyte_buf, code);
    }

    fn read_hex_digits(&mut self) -> Result<(u32, usize), ReadError> {
        let start = self.pos;
        while let Some(c) = self.current_code() {
            if is_ascii_hexdigit_code(c) {
                self.bump();
            } else {
                if c == b';' as u32 {
                    self.bump(); // consume terminating semicolon
                }
                break;
            }
        }
        let hex_storage = self.source_slice_string(start, self.pos);
        let hex_str = hex_storage.trim_end_matches(';');
        if hex_str.is_empty() {
            return Err(self.error("expected hex digits after \\x"));
        }
        let digits = hex_str.len();
        let value =
            u32::from_str_radix(hex_str, 16).map_err(|_| self.error("invalid hex escape"))?;
        Ok((value, digits))
    }

    fn read_fixed_hex(&mut self, count: usize) -> Result<u32, ReadError> {
        let start = self.pos;
        for _ in 0..count {
            match self.current_code() {
                Some(c) if is_ascii_hexdigit_code(c) => self.bump(),
                _ => return Err(self.error(&format!("expected {} hex digits", count))),
            }
        }
        let hex_storage = self.source_slice_string(start, self.pos);
        u32::from_str_radix(&hex_storage, 16).map_err(|_| self.error("invalid hex escape"))
    }

    fn read_char_hex_escape(&mut self) -> Result<u32, ReadError> {
        let mut value = 0u32;
        let mut digits = 0usize;
        while let Some(c) = self.current_code() {
            let Some(digit) = ascii_hex_digit_value(c) else {
                break;
            };
            self.bump();
            value = (value << 4) + digit;
            if value > (CHAR_META_MODIFIER | (CHAR_META_MODIFIER - 1)) {
                return Err(
                    self.ordinary_error(&format!("Hex character out of range: \\x{value:x}..."))
                );
            }
            digits += 1;
        }

        if digits == 0 {
            return Err(
                self.ordinary_error("Invalid escape char syntax: \\x not followed by hex digit")
            );
        }

        if digits < 3 && (0x80..0x100).contains(&value) {
            return Ok(emacs_char::unibyte_to_char(value as u8));
        }

        Ok(value)
    }

    fn read_char_unicode_escape(&mut self, count: usize, prefix: char) -> Result<u32, ReadError> {
        let mut value = 0u32;
        for _ in 0..count {
            let Some(c) = self.current_code() else {
                return Err(
                    self.ordinary_error(&format!("Malformed Unicode escape: \\{prefix}{value:x}"))
                );
            };
            self.bump();
            let Some(digit) = ascii_hex_digit_value(c) else {
                return Err(self.ordinary_error(&format!(
                    "Non-hex character used for Unicode escape: {} ({})",
                    source_code_for_error(c),
                    c
                )));
            };
            value = (value << 4) + digit;
        }

        if value > 0x10FFFF {
            return Err(self.ordinary_error(&format!("Non-Unicode character: 0x{value:x}")));
        }

        Ok(value)
    }

    fn read_unicode_name_escape(&mut self) -> Result<u32, ReadError> {
        if self.current_code() != Some(b'{' as u32) {
            return Err(self.error("Expected opening brace after \\N"));
        }
        self.bump();
        let mut name = String::new();
        let mut whitespace = false;

        loop {
            let Some(ch) = self.current_code() else {
                return Err(self.error("unterminated \\N{...} escape"));
            };
            self.bump();

            if ch == b'}' as u32 {
                break;
            }

            if ch >= 0x80 {
                return Err(self.error(&format!("Invalid character U+{ch:04X} in character name")));
            }

            let ch = if is_ascii_whitespace_code(ch) {
                if whitespace {
                    continue;
                }
                whitespace = true;
                b' ' as u32
            } else {
                whitespace = false;
                ch
            };

            if name.len() >= UNICODE_CHARACTER_NAME_LENGTH_BOUND {
                return Err(self.error("Character name too long"));
            }
            name.push(char::from_u32(ch).expect("ASCII character name byte"));
        }

        if name.is_empty() {
            return Err(self.error("Empty character name"));
        }

        character_name_to_code(&name).ok_or_else(|| self.error(&format!("\\N{{{name}}}")))
    }

    // -- Character literals ?a -----------------------------------------------

    fn read_char_literal(&mut self) -> Result<Value, ReadError> {
        self.expect('?')?;
        if matches!(self.current_code(), Some(code) if code == b' ' as u32 || code == b'\t' as u32)
        {
            let ch = self
                .current_code()
                .expect("matched whitespace char literal");
            self.bump();
            return Ok(Value::fixnum(ch as i64));
        }

        let val = self.parse_char_value(0)?;
        if matches!(self.current_code(), Some(ch) if !is_char_literal_delimiter_code(ch)) {
            return Err(self.error("?"));
        }
        let modifiers = val & !CHAR_CODE_MASK;
        let mut code = val & CHAR_CODE_MASK;
        if emacs_char::char_byte8_p(code) {
            code = emacs_char::char_to_byte8(code) as u32;
        }

        // Character literals with modifier bits produce values beyond Unicode
        // range.  Emit them as fixnums, matching GNU Emacs where characters
        // are integers.
        Ok(Value::fixnum((code | modifiers) as i64))
    }

    /// Parse the value part of a character literal, accumulating modifier bits.
    fn parse_char_value(&mut self, modifiers: u32) -> Result<u32, ReadError> {
        let Some(ch) = self.current_code() else {
            return Err(self.end_of_file_error());
        };
        self.bump();

        if ch == b'\\' as u32 {
            let Some(esc) = self.current_code() else {
                return Err(self.end_of_file_error());
            };
            self.bump();
            let val = match esc {
                x if x == b'n' as u32 => b'\n' as u32,
                x if x == b'r' as u32 => b'\r' as u32,
                x if x == b't' as u32 => b'\t' as u32,
                x if x == b'\\' as u32 => b'\\' as u32,
                x if x == b'\'' as u32 => b'\'' as u32,
                x if x == b'"' as u32 => b'"' as u32,
                x if x == b'a' as u32 => 0x07, // BEL
                x if x == b'b' as u32 => 0x08, // BS
                x if x == b'f' as u32 => 0x0C, // FF
                x if x == b'v' as u32 => 0x0B, // VT
                x if x == b'e' as u32 => 0x1B, // ESC
                x if x == b'd' as u32 => 0x7F, // DEL
                x if x == b's' as u32 && self.current_code() == Some(b'-' as u32) => {
                    self.bump();
                    return self.parse_char_value(modifiers | (1 << 23)); // super bit
                }
                x if x == b's' as u32 => b' ' as u32,
                x if x == b'x' as u32 => self.read_char_hex_escape()?,
                x if x == b'u' as u32 => self.read_char_unicode_escape(4, 'u')?,
                x if x == b'U' as u32 => self.read_char_unicode_escape(8, 'U')?,
                x if x == b'N' as u32 => self.read_unicode_name_escape()?,
                x if (b'0' as u32..=b'7' as u32).contains(&x) => {
                    let mut val = esc - b'0' as u32;
                    for _ in 0..2 {
                        match self.current_code() {
                            Some(c) if (b'0' as u32..=b'7' as u32).contains(&c) => {
                                self.bump();
                                val = val * 8 + (c - b'0' as u32);
                            }
                            _ => break,
                        }
                    }
                    val
                }
                x if x == b'C' as u32 => {
                    if self.current_code() != Some(b'-' as u32) {
                        return Err(self
                            .ordinary_error("Invalid escape char syntax: \\C not followed by -"));
                    }
                    self.bump(); // consume '-'
                    let base = self.parse_char_value(modifiers)?;
                    return Ok(apply_control_modifier(base));
                }
                x if x == b'M' as u32 => {
                    if self.current_code() != Some(b'-' as u32) {
                        return Err(self
                            .ordinary_error("Invalid escape char syntax: \\M not followed by -"));
                    }
                    self.bump();
                    return self.parse_char_value(modifiers | CHAR_META_MODIFIER);
                }
                x if x == b'S' as u32 => {
                    if self.current_code() != Some(b'-' as u32) {
                        return Err(self
                            .ordinary_error("Invalid escape char syntax: \\S not followed by -"));
                    }
                    self.bump();
                    return self.parse_char_value(modifiers | CHAR_SHIFT_MODIFIER);
                }
                x if x == b'A' as u32 => {
                    if self.current_code() != Some(b'-' as u32) {
                        return Err(self
                            .ordinary_error("Invalid escape char syntax: \\A not followed by -"));
                    }
                    self.bump();
                    return self.parse_char_value(modifiers | (1 << 22)); // alt bit
                }
                x if x == b'H' as u32 => {
                    if self.current_code() != Some(b'-' as u32) {
                        return Err(self
                            .ordinary_error("Invalid escape char syntax: \\H not followed by -"));
                    }
                    self.bump();
                    return self.parse_char_value(modifiers | (1 << 24)); // hyper bit
                }
                x if x == b'^' as u32 => {
                    let base = self.parse_char_value(modifiers)?;
                    return Ok(apply_control_modifier(base));
                }
                other => other,
            };
            Ok(val | modifiers)
        } else {
            Ok(ch | modifiers)
        }
    }

    // -- Hash syntax #' #( etc -----------------------------------------------

    fn read_hash_syntax(&mut self) -> Result<Value, ReadError> {
        self.expect('#')?;
        let Some(ch) = self.current_code() else {
            return Err(self.error("#"));
        };

        match ch {
            x if x == b'\'' as u32 => {
                // #'function
                self.bump();
                let saved = save_scratch_gc_roots();
                let expr = self.read_form()?;
                push_scratch_gc_root(expr);
                let result = Value::list(vec![Value::symbol("function"), expr]);
                restore_scratch_gc_roots(saved);
                Ok(result)
            }
            x if x == b'_' as u32 => {
                // #_NAME reads NAME as a literal symbol, bypassing
                // `read-symbol-shorthands` and number/t/nil interpretation
                // (GNU lread.c). So `#_foo` -> foo, `#_2` -> the symbol named "2".
                self.bump();
                let token = self.read_symbol_token();
                if token.bytes.is_empty() {
                    return Err(self.error("#_"));
                }
                let name = token.to_lisp_string();
                Ok(Value::from_sym_id(intern_lisp_string(&name)))
            }
            x if x == b'(' as u32 => {
                // #("string" START END (PROPS...) ...) — propertized string.
                //
                // GNU `src/lread.c:string_props_from_rev_list' reads a string
                // followed by START END PLIST triplets and applies each triplet
                // with `Fset_text_properties'.  Keep the same representation as
                // ordinary string intervals so redisplay sees properties like
                // `(display (space :align-to ...))' on package string literals.
                let saved = save_scratch_gc_roots();
                let list = self.read_list_or_dotted()?;
                push_scratch_gc_root(list);
                let Some(items) = list_to_vec(&list) else {
                    restore_scratch_gc_roots(saved);
                    return Err(self.error("#"));
                };
                let Some((&string, rest)) = items.split_first() else {
                    restore_scratch_gc_roots(saved);
                    return Err(self.error("#"));
                };
                if !string.is_string() {
                    restore_scratch_gc_roots(saved);
                    return Err(self.error("#"));
                }
                if rest.len() % 3 != 0 {
                    restore_scratch_gc_roots(saved);
                    return Err(self.error("#(: invalid string property list"));
                }
                let string_len = string
                    .as_lisp_string()
                    .map(|s| s.schars() as i64)
                    .unwrap_or(0);
                let mut runs = Vec::with_capacity(rest.len() / 3);
                for chunk in rest.chunks(3) {
                    let Some(start) = chunk[0].as_fixnum() else {
                        restore_scratch_gc_roots(saved);
                        return Err(self.error("#(: invalid string property start"));
                    };
                    let Some(end) = chunk[1].as_fixnum() else {
                        restore_scratch_gc_roots(saved);
                        return Err(self.error("#(: invalid string property end"));
                    };
                    if !(0 <= start && start <= end && end <= string_len) {
                        restore_scratch_gc_roots(saved);
                        // GNU read1 signals `args-out-of-range' with the START
                        // and END of the offending interval (not an
                        // invalid-read-syntax).
                        return Err(self.signal_error(
                            "args-out-of-range",
                            vec![Value::fixnum(start), Value::fixnum(end)],
                            "args-out-of-range",
                        ));
                    }
                    let Some(plist_items) = list_to_vec(&chunk[2]) else {
                        restore_scratch_gc_roots(saved);
                        return Err(self.error("Invalid string property list"));
                    };
                    if plist_items.len() % 2 != 0 {
                        restore_scratch_gc_roots(saved);
                        return Err(self.error("Invalid string property list"));
                    }
                    runs.push(StringTextPropertyRun {
                        start: start as usize,
                        end: end as usize,
                        plist: chunk[2],
                    });
                }
                if !runs.is_empty() {
                    set_string_text_properties_for_value(string, runs);
                }
                restore_scratch_gc_roots(saved);
                Ok(string)
            }
            x if x == b'[' as u32 => {
                // #[...] — compiled-function literal in .elc.
                // Produce a closure object directly, matching GNU Emacs's
                // lread.c `bytecode_from_rev_list`.
                let saved = save_scratch_gc_roots();
                let items = self.read_vector_items()?;
                for item in &items {
                    push_scratch_gc_root(*item);
                }
                let result = crate::emacs_core::builtins::closure_from_reader_literal_slots(&items)
                    .map_err(|e| {
                        let msg = match &e {
                            crate::emacs_core::error::Flow::Signal(sig) => sig
                                .data
                                .first()
                                .and_then(|v| {
                                    v.as_lisp_string().map(|ls| {
                                        crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes())
                                    })
                                })
                                .unwrap_or_else(|| format!("{:?}", sig.data)),
                            other => format!("{:?}", other),
                        };
                        self.error(&msg)
                    });
                restore_scratch_gc_roots(saved);
                result
            }
            x if x == b'^' as u32 => {
                // #^[...]  — char-table literal.
                // #^^[...] — sub-char-table literal.
                //
                // GNU Emacs marks vectors read through this syntax as
                // PVEC_CHAR_TABLE / PVEC_SUB_CHAR_TABLE in lread.c.  NeoVM
                // represents char-tables as tagged Values, so delegate the
                // slot conversion to chartable.rs after reading the vector
                // payload.
                self.bump();
                let is_sub_char_table = if self.current_code() == Some(b'^' as u32) {
                    self.bump();
                    true
                } else {
                    false
                };
                if self.current_code() != Some(b'[' as u32) {
                    return Err(self.error("#^"));
                }

                let saved = save_scratch_gc_roots();
                let items = self.read_vector_items()?;
                for item in &items {
                    push_scratch_gc_root(*item);
                }
                let result = if is_sub_char_table {
                    crate::emacs_core::chartable::make_sub_char_table_from_external_slots(&items)
                } else {
                    crate::emacs_core::chartable::make_char_table_from_external_slots(&items)
                }
                .map_err(|msg| self.error(&msg));
                restore_scratch_gc_roots(saved);
                result
            }
            x if x == b'@' as u32 => {
                // #@N<bytes> — reader skip used by .elc for inline data blocks.
                self.read_hash_skip_bytes()
            }
            x if x == b'!' as u32 => {
                // `#!shebang line` — GNU `lread.c` treats `#!` as a
                // comment to end-of-line, so a script-style shebang
                // (`#!/usr/bin/env emacs --script`) loads cleanly.
                // Skip to the next newline (or EOF) and read the next
                // form.
                self.bump();
                while let Some(c) = self.current_code() {
                    self.bump();
                    if c == b'\n' as u32 {
                        break;
                    }
                }
                self.read_form()
            }
            x if x == b':' as u32 => {
                // #:X — uninterned symbol.
                self.bump();
                let token = self.read_symbol_token();
                Ok(Value::from_sym_id(intern_uninterned_lisp_string(
                    &token.to_lisp_string(),
                )))
            }
            x if x == b'$' as u32 => {
                // #$ — expands to the current load file name during read.
                // Matches GNU lread.c: returns Vload_file_name (the actual
                // file path string), not the symbol `load-file-name`.
                self.bump();
                let sym_id = super::intern::intern("load-file-name");
                Ok(self
                    .obarray
                    .symbol_value_id(sym_id)
                    .copied()
                    .unwrap_or(Value::NIL))
            }
            x if x == b'#' as u32 => {
                // ## — symbol with empty name.
                self.bump();
                Ok(Value::from_sym_id(intern("")))
            }
            x if x == b'b' as u32 || x == b'B' as u32 => {
                // #b... binary integer
                self.bump();
                self.read_radix_number(2)
            }
            x if x == b'o' as u32 || x == b'O' as u32 => {
                // #o... octal integer
                self.bump();
                self.read_radix_number(8)
            }
            x if x == b'x' as u32 || x == b'X' as u32 => {
                // #x... hex integer
                self.bump();
                self.read_radix_number(16)
            }
            x if x == b's' as u32 => {
                // #s(hash-table ...) or #s(record-type ...)
                self.bump();
                if self.current_code() == Some(b'(' as u32) {
                    self.read_hash_table_or_record_literal()
                } else {
                    // GNU `read1` case 's' consumes the next character with
                    // `read_and_buffer` before checking it, so the
                    // `invalid-read-syntax` text includes the offending char
                    // (e.g. "#s[").  Mirror that: append the consumed character
                    // to the "#s" prefix, or report just "#s" at end of input.
                    let mut message = String::from("#s");
                    if let Some(code) = self.current_code() {
                        self.bump();
                        if let Some(ch) = char::from_u32(code) {
                            message.push(ch);
                        }
                    }
                    Err(self.error(&message))
                }
            }
            x if x == b'&' as u32 => {
                // #&SIZE"DATA" — bool-vector literal.
                self.bump();
                self.read_bool_vector_literal()
            }
            x if is_ascii_digit_code(x) => {
                // #N=EXPR defines read label N, #N# references it.
                let mut n: usize = (ch as u8 - b'0') as usize;
                self.bump();
                while let Some(d) = self.current_code() {
                    if is_ascii_digit_code(d) {
                        n = n * 10 + (d as u8 - b'0') as usize;
                        self.bump();
                    } else {
                        break;
                    }
                }
                match self.current_code() {
                    Some(code) if code == b'r' as u32 || code == b'R' as u32 => {
                        // #NrDIGITS -- radix-N integer.  GNU lread.c checks
                        // this before #N=/#N# read-label syntax.
                        self.bump();
                        if !(2..=36).contains(&n) {
                            return Err(self.error(&format!("integer, radix {}", n)));
                        }
                        self.read_radix_number(n as u32)
                    }
                    Some(code) if code == b'=' as u32 && self.read_circle_enabled() => {
                        // #N=EXPR -- GNU lread.c installs a placeholder
                        // before reading EXPR so #N# can refer to recursive
                        // structures being read.
                        self.bump();
                        let placeholder = Value::cons(Value::NIL, Value::NIL);
                        self.read_labels.insert(n, placeholder);
                        let saved = save_scratch_gc_roots();
                        push_scratch_gc_root(placeholder);
                        let expr = match self.read_form() {
                            Ok(expr) => expr,
                            Err(err) => {
                                restore_scratch_gc_roots(saved);
                                return Err(err);
                            }
                        };

                        let result = if expr.is_cons() {
                            if eq_value(&expr, &placeholder) {
                                restore_scratch_gc_roots(saved);
                                return Err(self.error("nonsensical self-reference"));
                            }
                            placeholder.set_car(expr.cons_car());
                            placeholder.set_cdr(expr.cons_cdr());
                            placeholder
                        } else {
                            substitute_read_placeholder(expr, placeholder);
                            self.read_labels.insert(n, expr);
                            expr
                        };
                        restore_scratch_gc_roots(saved);
                        Ok(result)
                    }
                    Some(code) if code == b'#' as u32 && self.read_circle_enabled() => {
                        // #N# — reference previously defined label N
                        self.bump();
                        self.read_labels
                            .get(&n)
                            .copied()
                            .ok_or_else(|| self.error(&format!("#{n}#")))
                    }
                    Some(code) if code == b'=' as u32 => {
                        self.bump();
                        Err(self.error(&format!("#{n}=")))
                    }
                    Some(code) if code == b'#' as u32 => {
                        self.bump();
                        Err(self.error(&format!("#{n}#")))
                    }
                    _ => Err(self.error(&format!("#{n}"))),
                }
            }
            _ => Err(self.error_after_current(&format!("#{}", source_code_for_error(ch)))),
        }
    }

    fn read_hash_skip_bytes(&mut self) -> Result<Value, ReadError> {
        self.expect('@')?;
        let mut len = 0usize;
        let mut digits = 0usize;
        loop {
            match self.current_code() {
                Some(c) if is_ascii_digit_code(c) => {
                    self.bump();
                    len = len
                        .checked_mul(10)
                        .and_then(|n| n.checked_add((c as u8 - b'0') as usize))
                        .ok_or_else(|| self.error("#@"))?;
                    digits += 1;
                    if digits == 2 && len == 0 {
                        self.skip_to_limit();
                        return Ok(Value::NIL);
                    }
                }
                Some(_) if len > 0 => {
                    self.bump();
                    len -= 1;
                    break;
                }
                Some(_) => return Err(self.end_of_file_error()),
                None => return Err(self.end_of_file_error()),
            }
        }

        match self.source {
            ReaderSource::Runtime(_) => self.skip_exact_source_bytes(len)?,
            ReaderSource::LispString(_) | ReaderSource::Buffer(_) => {
                self.skip_dynamic_doc_string_in_non_file_source();
            }
        }
        self.read_form()
    }

    fn read_bool_vector_literal(&mut self) -> Result<Value, ReadError> {
        let mut size = 0usize;
        let mut saw_digit = false;
        while let Some(c) = self.current_code() {
            if !is_ascii_digit_code(c) {
                break;
            }
            saw_digit = true;
            self.bump();
            let digit = (c as u8 - b'0') as usize;
            size = size
                .checked_mul(10)
                .and_then(|n| n.checked_add(digit))
                .ok_or_else(|| self.error("#&"))?;
        }
        if !saw_digit || self.current_code() != Some(b'"' as u32) {
            return Err(self.error("#&"));
        }
        let data = self.read_string()?;
        let data_string = data.as_lisp_string().ok_or_else(|| self.error("#&..."))?;
        let size_in_bytes = size.div_ceil(8);
        let data_chars = data_string.schars();
        let old_emacs_19_extra_byte = data_chars > 0 && size == (data_chars - 1) * 8;
        if data_string.is_multibyte() || !(data_chars == size_in_bytes || old_emacs_19_extra_byte) {
            return Err(self.error("#&..."));
        }

        let mut bits = Vec::with_capacity(size);
        for byte_val in data_string.as_bytes() {
            for bit_idx in 0..8 {
                if bits.len() >= size {
                    break;
                }
                bits.push((byte_val >> bit_idx) & 1 != 0);
            }
        }
        Ok(super::chartable::bool_vector_from_bits(&bits))
    }

    fn read_radix_number(&mut self, radix: u32) -> Result<Value, ReadError> {
        let start = self.pos;
        let negative = if self.current_code() == Some(b'-' as u32) {
            self.bump();
            true
        } else if self.current_code() == Some(b'+' as u32) {
            self.bump();
            false
        } else {
            false
        };

        // GNU `read_integer` (src/lread.c:2944) keeps consuming *all*
        // alphanumeric characters via `digit_to_number`, which distinguishes a
        // non-alphanumeric terminator (return < -1, stop) from an alphanumeric
        // character that is not a valid digit for this radix (return -1, e.g.
        // `g' in `#x1g' or `8' in `#o18').  The latter does NOT stop reading:
        // it is consumed and flags the whole token invalid, so the integer is
        // rejected with "integer, radix N" rather than silently truncated and
        // leaving the stray letter to be misread as the next form.
        let mut saw_invalid_digit = false;
        while let Some(c) = self.current_code() {
            if is_ascii_digit_for_radix_code(c, radix) || c == b'_' as u32 {
                self.bump();
            } else if is_ascii_alphanumeric_code(c) {
                // Alphanumeric but not a digit for this radix: consume and
                // poison the token (matches GNU `digit == -1` path).
                saw_invalid_digit = true;
                self.bump();
            } else {
                break;
            }
        }

        let digits_source = self.source_slice_string(start, self.pos);
        let digits: String = digits_source
            .chars()
            .filter(|c| *c != '_' && *c != '-' && *c != '+')
            .collect();
        if digits.is_empty() || saw_invalid_digit {
            return Err(self.error(&format!("integer, radix {}", radix)));
        }

        // Try i64 first; on overflow promote to a malachite::Integer with the
        // requested radix. Mirrors GNU `string_to_number` (`src/lread.c`)
        // which falls through to the bignum path on overflow.
        let value = match i64::from_str_radix(&digits, radix) {
            Ok(val) => Value::make_integer(Integer::from(if negative { -val } else { val })),
            Err(_) => {
                let mut signed = String::with_capacity(digits.len() + 1);
                if negative {
                    signed.push('-');
                }
                signed.push_str(&digits);
                let parsed = Integer::from_string_base(radix as u8, &signed)
                    .ok_or_else(|| self.error("invalid radix number"))?;
                Value::make_integer(parsed)
            }
        };
        Ok(value)
    }

    fn read_hash_table_or_record_literal(&mut self) -> Result<Value, ReadError> {
        // #s(hash-table size N test T data (k1 v1 k2 v2 ...))
        // or #s(record-type field1 field2 ...)
        let saved = save_scratch_gc_roots();
        let list = self.read_list_or_dotted()?;
        push_scratch_gc_root(list);

        let mut items: Vec<Value> = Vec::new();
        let mut cursor = list;
        while cursor.is_cons() {
            items.push(cursor.cons_car());
            cursor = cursor.cons_cdr();
        }
        if !cursor.is_nil() {
            restore_scratch_gc_roots(saved);
            return Err(self.error("."));
        }

        if items.is_empty() {
            restore_scratch_gc_roots(saved);
            return Err(self.error("#s"));
        }

        // Check if first element is the symbol `hash-table`
        let is_hash_table = items
            .first()
            .is_some_and(|v| v.is_symbol_named("hash-table"));

        if is_hash_table {
            let result = self.read_hash_table_literal_from_plist(&items[1..]);
            restore_scratch_gc_roots(saved);
            return result;
        }

        // Not a hash-table — it's a record #s(type field1 field2 ...)
        let record_value = Value::make_record(items);
        restore_scratch_gc_roots(saved);
        Ok(record_value)
    }

    fn read_hash_table_literal_from_plist(&self, plist: &[Value]) -> Result<Value, ReadError> {
        let mut test = HashTableTest::Eql;
        let mut test_name = None;
        let mut user_cmp_function = None;
        let mut user_hash_function = None;

        if let Some(test_value) = plist_get(plist, HashTableLiteralKey::Test)
            && !test_value.is_nil()
        {
            let Some(name) = test_value.as_symbol_name() else {
                return Err(self.signal_error(
                    "wrong-type-argument",
                    vec![Value::symbol("symbolp"), test_value],
                    "symbolp",
                ));
            };
            test_name = Some(intern(name));
            test = match HashTableTest::from_symbol_name(name) {
                Some(test) => test,
                None => {
                    if let Some(alias) = lookup_hash_table_test_alias(name) {
                        user_cmp_function = alias.user_cmp_function;
                        user_hash_function = alias.user_hash_function;
                        alias.standard_test.unwrap_or(HashTableTest::Equal)
                    } else {
                        return Err(self.signal_error(
                            "error",
                            vec![Value::string("Invalid hash table test"), test_value],
                            "Invalid hash table test",
                        ));
                    }
                }
            };
        }

        let weakness = match plist_get(plist, HashTableLiteralKey::Weakness) {
            None => None,
            Some(value) if value.is_nil() => None,
            Some(value) if value.is_t() => Some(HashTableWeakness::KeyAndValue),
            Some(value) => {
                let Some(name) = value.as_symbol_name() else {
                    return Err(self.signal_error(
                        "error",
                        vec![Value::string("Invalid hash table weakness"), value],
                        "Invalid hash table weakness",
                    ));
                };
                match HashTableWeakness::from_symbol_name(name) {
                    Some(weakness) => Some(weakness),
                    None => {
                        return Err(self.signal_error(
                            "error",
                            vec![Value::string("Invalid hash table weakness"), value],
                            "Invalid hash table weakness",
                        ));
                    }
                }
            }
        };

        let data_value = plist_get(plist, HashTableLiteralKey::Data).unwrap_or(Value::NIL);
        let data = if data_value.is_nil() {
            Vec::new()
        } else if !data_value.is_cons() {
            return Err(self.ordinary_error("Hash table data is not a list"));
        } else {
            list_to_vec(&data_value).ok_or_else(|| {
                self.signal_error(
                    "wrong-type-argument",
                    vec![Value::symbol("listp"), data_value],
                    "listp",
                )
            })?
        };
        if data.len() & 1 != 0 {
            return Err(self.ordinary_error("Hash table data length is odd"));
        }

        let entries = data
            .chunks_exact(2)
            .map(|pair| (pair[0], pair[1]))
            .collect::<Vec<_>>();
        let table = build_hash_table_literal_value(
            test,
            test_name,
            (data.len() / 2) as i64,
            weakness,
            1.5,
            0.8125,
            entries,
        );
        let _ = table.with_hash_table_mut(|hash_table| {
            hash_table.user_cmp_function = user_cmp_function;
            hash_table.user_hash_function = user_hash_function;
        });
        Ok(table)
    }

    // -- Atoms (numbers, symbols) --------------------------------------------

    fn read_atom(&mut self) -> Result<Value, ReadError> {
        let token = self.read_symbol_token();

        if token.bytes.is_empty() {
            return Err(self.error("expected atom"));
        }

        // Keywords (:foo) — including bare `:` which is a keyword in Emacs
        if token.bytes.first() == Some(&b':') {
            return Ok(Value::keyword_id(intern_lisp_string(
                &token.to_lisp_string(),
            )));
        }

        if !token.had_escape
            && let Some(token_text) = token.as_utf8()
        {
            // Try integer. Funnel through Value::make_integer so a value
            // that fits in i64 but not in fixnum (62-bit) is promoted to
            // a bignum, matching GNU `string_to_number` behavior. On i64
            // overflow, fall through to a malachite::Integer parse so true
            // bignum literals work.
            if looks_like_integer(token_text) {
                // A single trailing "." with no fractional digits and no
                // exponent is an integer terminator in GNU's reader (e.g.
                // "5." reads as the integer 5, not the float 5.0). Strip it
                // before parsing the magnitude. See `string_to_number` in
                // GNU `src/lread.c`: float_syntax requires TRAIL_INT or an
                // exponent, so "1." is lexed as an integer.
                let digits = token_text.strip_suffix('.').unwrap_or(token_text);
                if let Ok(n) = digits.parse::<i64>() {
                    return Ok(Value::make_integer(Integer::from(n)));
                }
                if let Ok(parsed) = digits.parse::<Integer>() {
                    return Ok(Value::make_integer(parsed));
                }
            }

            // Try float — handles 1.5, 1e10, .5, 1.5e-3, etc.
            if looks_like_float(token_text) {
                if let Ok(f) = token_text.parse::<f64>() {
                    return Ok(Value::make_float(f));
                }
                if let Some(f) = parse_emacs_special_float(token_text) {
                    return Ok(Value::make_float(f));
                }
            }

            // t and nil
            if token_text == "t" {
                return Ok(Value::T);
            }
            if token_text == "nil" {
                return Ok(Value::NIL);
            }

            // A bare `.` is the symbol `.` only when it directly precedes a
            // closing delimiter (e.g. `(a .)` => `(a \.)`, matching GNU
            // lread.c which does not treat `)`/`]` as dot terminators).
            // Otherwise — at top level (`(read ".")`) or as a dotted
            // separator with no following element — GNU signals
            // invalid-read-syntax instead of reading the symbol `\.`.
            if token_text == "." {
                match self.current_code() {
                    Some(c) if c == b')' as u32 || c == b']' as u32 => {}
                    _ => return Err(self.error(".")),
                }
            }

            // Emacs reader shorthand: bare ## reads as the symbol with empty name.
            if token_text == "##" && !token.had_escape {
                return Ok(Value::from_sym_id(intern("")));
            }
        }

        // Fast path: a plain ASCII symbol with no shorthand rewrite and no
        // escapes interns straight from its &str view. `intern(&str)` hits the
        // interner's utf8_map without allocating, and on miss builds exactly the
        // same atom the LispString path would (name_atom_from_str for ASCII ==
        // LispString::from_unibyte). This skips the per-token heap LispString
        // for the overwhelming majority of symbols read while loading.
        if self.shorthands.is_none()
            && !token.had_escape
            && let Some(text) = token.as_utf8()
            && text.is_ascii()
        {
            return Ok(Value::from_sym_id(intern(text)));
        }

        // Cold path: non-ASCII / escaped / shorthand-rewritten symbols keep the
        // exact LispString encoding.
        let name = self.to_lisp_string_for_token(&token);
        Ok(Value::from_sym_id(intern_lisp_string(&name)))
    }

    /// Build the token's exact `LispString` name, applying any active reader
    /// shorthand rewrite (`read-symbol-shorthands`).
    fn to_lisp_string_for_token(&self, token: &ReaderToken) -> crate::heap_types::LispString {
        let name = token.to_lisp_string();
        self.shorthands
            .and_then(|shorthands| shorthands.rewrite(&name))
            .unwrap_or(name)
    }

    fn read_symbol_token(&mut self) -> ReaderToken {
        let mut bytes = ReaderTokenBytes::new();
        let mut had_escape = false;
        while let Some(ch) = self.current_code() {
            if is_symbol_delimiter_code(ch) {
                break;
            }
            if ch == b'\\' as u32 {
                had_escape = true;
                self.bump();
                match self.current_code() {
                    Some(escaped) => {
                        self.push_symbol_token_code(&mut bytes, escaped);
                        self.bump();
                    }
                    None => bytes.push(b'\\'),
                }
                continue;
            }
            self.push_symbol_token_code(&mut bytes, ch);
            self.bump();
        }
        // Keep `bytes` inline (no into_vec / LispString here). read_atom interns
        // ASCII symbols directly from the &str view (interner no-alloc fast
        // path); only the cold paths materialize a LispString via to_lisp_string.
        ReaderToken {
            bytes,
            multibyte: self.source_semantics.is_multibyte(),
            had_escape,
        }
    }

    fn push_symbol_token_code(&self, bytes: &mut ReaderTokenBytes, code: u32) {
        if !self.source_semantics.is_multibyte() && code <= 0xFF {
            bytes.push(code as u8);
            return;
        }

        let mut tmp = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = emacs_char::char_string(code, &mut tmp);
        bytes.extend_from_slice(&tmp[..len]);
    }

    // -- Helpers -------------------------------------------------------------

    fn expect(&mut self, expected: char) -> Result<(), ReadError> {
        let expected_code = expected as u32;
        match self.current_code() {
            Some(code) if code == expected_code => {
                self.bump();
                Ok(())
            }
            _ => Err(self.error(&format!("expected '{}'", expected))),
        }
    }

    fn current_code(&self) -> Option<u32> {
        self.source_code_at(self.pos)
    }

    fn peek_code_at(&self, offset: usize) -> Option<u32> {
        let mut pos = self.pos;
        for _ in 0..offset {
            pos = self.next_pos(pos)?;
        }
        self.source_code_at(pos)
    }

    fn bump(&mut self) {
        self.pos = self.next_pos(self.pos).unwrap_or(self.limit);
    }

    fn error(&self, message: &str) -> ReadError {
        ReadError {
            position: self.pos,
            message: message.to_string(),
            kind: ReadErrorKind::InvalidReadSyntax,
            signal_symbol: None,
            signal_data: Vec::new(),
        }
    }

    fn ordinary_error(&self, message: &str) -> ReadError {
        ReadError {
            position: self.pos,
            message: message.to_string(),
            kind: ReadErrorKind::Error,
            signal_symbol: None,
            signal_data: Vec::new(),
        }
    }

    fn signal_error(&self, symbol: &str, data: Vec<Value>, message: &str) -> ReadError {
        ReadError {
            position: self.pos,
            message: message.to_string(),
            kind: ReadErrorKind::Signal,
            signal_symbol: Some(symbol.to_string()),
            signal_data: data,
        }
    }

    fn end_of_file_error(&self) -> ReadError {
        ReadError {
            position: self.pos,
            message: "end-of-file".to_string(),
            kind: ReadErrorKind::EndOfFile,
            signal_symbol: None,
            signal_data: Vec::new(),
        }
    }

    fn error_after_current(&mut self, message: &str) -> ReadError {
        if self.current_code().is_some() {
            self.bump();
        }
        self.error(message)
    }

    fn read_circle_enabled(&self) -> bool {
        self.obarray
            .symbol_value("read-circle")
            .is_none_or(|value| !value.is_nil())
    }

    fn skip_to_limit(&mut self) {
        self.pos = self.limit;
    }

    fn skip_dynamic_doc_string_in_non_file_source(&mut self) {
        while let Some(c) = self.current_code() {
            self.bump();
            if c == 0x1F {
                break;
            }
        }
    }

    /// Advance `pos` past `len` source bytes from a `.elc` file.
    ///
    /// `.elc` bytes are Latin-1-decoded into a Rust `String` so that every
    /// source byte (including raw 0x80..=0xFF) becomes exactly one `char`.
    /// `#@LEN` skips count source bytes, not UTF-8 bytes, so we advance by
    /// `len` chars and let each char contribute its actual UTF-8 width to
    /// `pos`. A naive byte-wise advance would under-skip by 1 for every
    /// 0x80..=0xFF source byte (which becomes a 2-byte UTF-8 sequence in
    /// our `String`) and land mid-docstring on files like `window.elc`,
    /// where docstrings contain U+2019 (`'`) stored as `0xe2 0x80 0x99`.
    fn skip_exact_source_bytes(&mut self, len: usize) -> Result<(), ReadError> {
        match self.source {
            ReaderSource::Runtime(input) => {
                let mut chars = input[self.pos..self.limit].chars();
                let mut bytes_advanced = 0usize;
                for _ in 0..len {
                    match chars.next() {
                        Some(c) => bytes_advanced += c.len_utf8(),
                        None => return Err(self.error("byte skip past end of input")),
                    }
                }
                self.pos += bytes_advanced;
                Ok(())
            }
            ReaderSource::LispString(_) => {
                let end = self
                    .pos
                    .checked_add(len)
                    .ok_or_else(|| self.error("byte skip past end of input"))?;
                if end > self.limit {
                    return Err(self.error("byte skip past end of input"));
                }
                self.pos = end;
                Ok(())
            }
            ReaderSource::Buffer(_) => {
                let end = self
                    .pos
                    .checked_add(len)
                    .ok_or_else(|| self.error("byte skip past end of input"))?;
                if end > self.limit {
                    return Err(self.error("byte skip past end of input"));
                }
                self.pos = end;
                Ok(())
            }
        }
    }

    fn source_code_at(&self, pos: usize) -> Option<u32> {
        self.code_and_next(pos).map(|(code, _)| code)
    }

    fn next_pos(&self, pos: usize) -> Option<usize> {
        self.code_and_next(pos).map(|(_, next)| next)
    }

    /// Decode the char at `pos`, returning `(code, next_pos)`, through the
    /// one-entry `step_cache`.  The read hot path peeks then advances over the
    /// same position, so caching makes it decode each char once.
    fn code_and_next(&self, pos: usize) -> Option<(u32, usize)> {
        if let Some((cached_pos, code, next)) = self.step_cache.get()
            && cached_pos == pos
        {
            return Some((code, next));
        }
        let decoded = self.decode_step(pos)?;
        self.step_cache.set(Some((pos, decoded.0, decoded.1)));
        Some(decoded)
    }

    /// Raw single-char decode at `pos` -> `(code, next_pos)`, bypassing the
    /// cache.  Preserves the exact per-source limit semantics of the previous
    /// `source_code_at`/`next_pos`.
    fn decode_step(&self, pos: usize) -> Option<(u32, usize)> {
        if pos >= self.limit {
            return None;
        }
        match self.source {
            ReaderSource::Runtime(input) => {
                crate::emacs_core::string_escape::storage_code_step(input, pos, true)
                    .filter(|(_, next)| *next <= self.limit)
            }
            ReaderSource::LispString(input) => self
                .lisp_string_code_step(input, pos)
                .map(|(code, width)| (code, pos + width)),
            ReaderSource::Buffer(input) => self
                .buffer_code_step(input, pos)
                .map(|(code, width)| (code, pos + width)),
        }
    }

    fn lisp_string_code_step(
        &self,
        input: &crate::heap_types::LispString,
        pos: usize,
    ) -> Option<(u32, usize)> {
        let bytes = input.as_bytes();
        if pos >= self.limit || pos >= bytes.len() {
            return None;
        }

        if !self.source_semantics.is_multibyte() {
            let byte = *bytes.get(pos)?;
            return Some((byte as u32, 1));
        }

        let (code, width) = emacs_char::string_char(&bytes[pos..]);
        if pos + width > self.limit {
            return None;
        }
        Some((code, width))
    }

    fn buffer_code_step(&self, input: &Buffer, pos: usize) -> Option<(u32, usize)> {
        if pos >= self.limit || pos >= input.total_emacs_byte_len().get() {
            return None;
        }

        if !self.source_semantics.is_multibyte() {
            let byte = input.emacs_byte_at_pos(EmacsBytePos::new(pos))?;
            return Some((byte as u32, 1));
        }

        let available = (self.limit - pos).min(emacs_char::MAX_MULTIBYTE_LENGTH);

        // Fast path: decode straight from a contiguous byte slice, like GNU's
        // `string_char_and_length (BUF_BYTE_ADDRESS (...))`.  Only when the
        // char's bytes straddle a gap / piece boundary does this yield None,
        // and we fall back to assembling them one byte at a time.  `available`
        // never exceeds the buffer length (it is clamped to `limit <= total`),
        // so both paths read identical bytes.
        let range =
            EmacsByteRange::from_start_len(EmacsBytePos::new(pos), EmacsByteLen::new(available));
        let range_start = range.start();
        let (code, width) = match input
            .with_contiguous_emacs_byte_range(range, emacs_char::string_char)
        {
            Some(decoded) => decoded,
            None => {
                let mut tmp = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
                for (idx, slot) in tmp[..available].iter_mut().enumerate() {
                    *slot = input.emacs_byte_at_pos(range_start.add_len(EmacsByteLen::new(idx)))?;
                }
                emacs_char::string_char(&tmp[..available])
            }
        };
        if pos + width > self.limit {
            return None;
        }
        Some((code, width))
    }

    fn source_slice_string(&self, start: usize, end: usize) -> String {
        assert!(start <= end, "invalid reader slice: {start}..{end}");
        assert!(
            end <= self.limit,
            "reader slice end {end} exceeds limit {}",
            self.limit
        );
        match self.source {
            ReaderSource::Runtime(input) => input[start..end].to_string(),
            ReaderSource::LispString(input) => {
                let slice = input
                    .slice(start, end)
                    .expect("reader slice should stay within source");
                // Issue #131: `source_slice_string` only feeds ASCII numeric
                // sub-parsers (hex/radix-digit escapes), so a lossy UTF-8
                // rendering is byte-exact here and avoids the storage-string
                // sentinel scheme.
                crate::emacs_core::emacs_char::to_utf8_lossy(slice.as_bytes())
            }
            ReaderSource::Buffer(input) => {
                let mut bytes = Vec::with_capacity(end - start);
                input.copy_emacs_byte_range_to(
                    EmacsByteRange::from_start_len(
                        EmacsBytePos::new(start),
                        EmacsByteLen::new(end - start),
                    ),
                    &mut bytes,
                );
                let slice = if input.get_multibyte() {
                    crate::heap_types::LispString::from_emacs_bytes(bytes)
                } else {
                    crate::heap_types::LispString::from_unibyte(bytes)
                };
                // Issue #131: ASCII-only numeric sub-parser input (see above);
                // lossy rendering is byte-exact and drops the storage scheme.
                crate::emacs_core::emacs_char::to_utf8_lossy(slice.as_bytes())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Free functions (copied from parser.rs)
// ---------------------------------------------------------------------------

fn is_char_literal_delimiter_code(code: u32) -> bool {
    is_reader_whitespace_code(code)
        || matches!(
            code,
            34 | 39 | 59 | 40 | 41 | 91 | 93 | 35 | 63 | 96 | 44 | 46
        )
}

fn is_symbol_delimiter_code(code: u32) -> bool {
    is_reader_whitespace_code(code) || matches!(code, 40 | 41 | 91 | 93 | 39 | 96 | 44 | 34 | 59)
}

/// GNU `read0`'s lexical whitespace predicate (`src/lread.c`) is deliberately
/// broader than the host language's ASCII-whitespace predicate: all C0
/// controls (character codes 0 through 32) and NO-BREAK SPACE terminate a
/// token and are skipped between forms.  Keep that rule at the reader boundary
/// instead of borrowing Rust's narrower `is_ascii_whitespace` semantics.
fn is_reader_whitespace_code(code: u32) -> bool {
    code <= 32 || code == 0xA0
}

fn is_ascii_whitespace_code(code: u32) -> bool {
    code <= 0x7F && (code as u8).is_ascii_whitespace()
}

fn is_ascii_digit_code(code: u32) -> bool {
    code <= 0x7F && (code as u8).is_ascii_digit()
}

fn is_ascii_hexdigit_code(code: u32) -> bool {
    code <= 0x7F && (code as u8).is_ascii_hexdigit()
}

fn ascii_hex_digit_value(code: u32) -> Option<u32> {
    if code > 0x7F {
        return None;
    }
    match code as u8 {
        b'0'..=b'9' => Some((code as u8 - b'0') as u32),
        b'a'..=b'f' => Some((code as u8 - b'a' + 10) as u32),
        b'A'..=b'F' => Some((code as u8 - b'A' + 10) as u32),
        _ => None,
    }
}

fn is_ascii_digit_for_radix_code(code: u32, radix: u32) -> bool {
    code <= 0x7F && (code as u8 as char).is_digit(radix)
}

/// True for ASCII [0-9A-Za-z], i.e. characters that GNU `digit_to_number`
/// maps to a non-negative *or* `-1` value (it returns `-2` only for
/// non-alphanumerics).  Used by the radix reader to keep consuming a token
/// like `1g` so a letter that is not a valid digit poisons the integer
/// instead of silently terminating it.
fn is_ascii_alphanumeric_code(code: u32) -> bool {
    code <= 0x7F && (code as u8 as char).is_ascii_alphanumeric()
}

fn character_name_to_code(name: &str) -> Option<u32> {
    if let Some(hex) = name.strip_prefix("U+") {
        return parse_unicode_codepoint(hex);
    }

    lookup_primary_unicode_name(name)
        .or_else(|| lookup_lambda_spelling_alias(name))
        .or_else(|| lookup_gnu_old_unicode_name(name))
}

fn parse_unicode_codepoint(hex: &str) -> Option<u32> {
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    valid_unicode_scalar(value).then_some(value)
}

fn valid_unicode_scalar(value: u32) -> bool {
    value <= 0x10FFFF && !(0xD800..=0xDFFF).contains(&value)
}

fn lookup_primary_unicode_name(name: &str) -> Option<u32> {
    let ch = unicode_names2::character(name)?;
    let primary = unicode_names2::name(ch)?;
    primary
        .to_string()
        .eq_ignore_ascii_case(name)
        .then_some(ch as u32)
}

fn lookup_lambda_spelling_alias(name: &str) -> Option<u32> {
    let upper = name.to_ascii_uppercase();
    if !upper.contains("LAMBDA") {
        return None;
    }
    let lamda = replace_ascii_word(&upper, "LAMBDA", "LAMDA")?;
    lookup_primary_unicode_name(&lamda)
}

fn replace_ascii_word(input: &str, needle: &str, replacement: &str) -> Option<String> {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let needle_bytes = needle.as_bytes();
    let mut cursor = 0;
    let mut changed = false;

    while cursor < bytes.len() {
        if bytes[cursor..].starts_with(needle_bytes)
            && cursor
                .checked_sub(1)
                .and_then(|idx| bytes.get(idx))
                .is_none_or(|b| !b.is_ascii_alphanumeric())
            && bytes
                .get(cursor + needle_bytes.len())
                .is_none_or(|b| !b.is_ascii_alphanumeric())
        {
            out.push_str(replacement);
            cursor += needle_bytes.len();
            changed = true;
        } else {
            out.push(bytes[cursor] as char);
            cursor += 1;
        }
    }

    changed.then_some(out)
}

fn lookup_gnu_old_unicode_name(name: &str) -> Option<u32> {
    match name.to_ascii_uppercase().as_str() {
        "NULL" => Some(0x00),
        "START OF HEADING" => Some(0x01),
        "START OF TEXT" => Some(0x02),
        "END OF TEXT" => Some(0x03),
        "END OF TRANSMISSION" => Some(0x04),
        "ENQUIRY" => Some(0x05),
        "ACKNOWLEDGE" => Some(0x06),
        "BELL (BEL)" => Some(0x07),
        "BACKSPACE" => Some(0x08),
        "CHARACTER TABULATION" => Some(0x09),
        "LINE FEED (LF)" => Some(0x0A),
        "LINE TABULATION" => Some(0x0B),
        "FORM FEED (FF)" => Some(0x0C),
        "CARRIAGE RETURN (CR)" => Some(0x0D),
        "SHIFT OUT" => Some(0x0E),
        "SHIFT IN" => Some(0x0F),
        "DATA LINK ESCAPE" => Some(0x10),
        "DEVICE CONTROL ONE" => Some(0x11),
        "DEVICE CONTROL TWO" => Some(0x12),
        "DEVICE CONTROL THREE" => Some(0x13),
        "DEVICE CONTROL FOUR" => Some(0x14),
        "NEGATIVE ACKNOWLEDGE" => Some(0x15),
        "SYNCHRONOUS IDLE" => Some(0x16),
        "END OF TRANSMISSION BLOCK" => Some(0x17),
        "CANCEL" => Some(0x18),
        "END OF MEDIUM" => Some(0x19),
        "SUBSTITUTE" => Some(0x1A),
        "ESCAPE" => Some(0x1B),
        "INFORMATION SEPARATOR FOUR" => Some(0x1C),
        "INFORMATION SEPARATOR THREE" => Some(0x1D),
        "INFORMATION SEPARATOR TWO" => Some(0x1E),
        "INFORMATION SEPARATOR ONE" => Some(0x1F),
        "DELETE" => Some(0x7F),
        "BREAK PERMITTED HERE" => Some(0x82),
        "NO BREAK HERE" => Some(0x83),
        "NEXT LINE (NEL)" => Some(0x85),
        "START OF SELECTED AREA" => Some(0x86),
        "END OF SELECTED AREA" => Some(0x87),
        "CHARACTER TABULATION SET" => Some(0x88),
        "CHARACTER TABULATION WITH JUSTIFICATION" => Some(0x89),
        "LINE TABULATION SET" => Some(0x8A),
        "PARTIAL LINE FORWARD" => Some(0x8B),
        "PARTIAL LINE BACKWARD" => Some(0x8C),
        "REVERSE LINE FEED" => Some(0x8D),
        "SINGLE SHIFT TWO" => Some(0x8E),
        "SINGLE SHIFT THREE" => Some(0x8F),
        "DEVICE CONTROL STRING" => Some(0x90),
        "PRIVATE USE ONE" => Some(0x91),
        "PRIVATE USE TWO" => Some(0x92),
        "SET TRANSMIT STATE" => Some(0x93),
        "CANCEL CHARACTER" => Some(0x94),
        "MESSAGE WAITING" => Some(0x95),
        "START OF GUARDED AREA" => Some(0x96),
        "END OF GUARDED AREA" => Some(0x97),
        "START OF STRING" => Some(0x98),
        "SINGLE CHARACTER INTRODUCER" => Some(0x9A),
        "CONTROL SEQUENCE INTRODUCER" => Some(0x9B),
        "STRING TERMINATOR" => Some(0x9C),
        "OPERATING SYSTEM COMMAND" => Some(0x9D),
        "PRIVACY MESSAGE" => Some(0x9E),
        "APPLICATION PROGRAM COMMAND" => Some(0x9F),
        "NON-BREAKING SPACE" => Some(0x00A0),
        "BYTE ORDER MARK" => Some(0xFEFF),
        _ => None,
    }
}

fn source_code_for_error(code: u32) -> String {
    char::from_u32(code)
        .map(|ch| ch.to_string())
        .unwrap_or_else(|| format!("\\U{code:08X}"))
}

fn looks_like_float(s: &str) -> bool {
    let s = if s.starts_with('+') || s.starts_with('-') {
        &s[1..]
    } else {
        s
    };
    if s.is_empty() {
        return false;
    }
    let first = s.as_bytes()[0];
    if !first.is_ascii_digit() && first != b'.' {
        return false;
    }
    // Mirror GNU `string_to_number` (`src/lread.c`): float syntax requires
    // either a fractional digit after the dot (TRAIL_INT) or an exponent
    // (E_EXP). A bare trailing dot like "5." has no fractional digits and no
    // exponent, so it is an integer terminator, not a float.
    if s.contains('e') || s.contains('E') {
        return true;
    }
    match s.split_once('.') {
        // Has a dot: float only if there are digits after it.
        Some((_, frac)) => frac.bytes().any(|b| b.is_ascii_digit()),
        None => false,
    }
}

/// True if `s` is a plain decimal integer literal (with optional sign)
/// — i.e. would parse as either an i64 or a bignum but never as a
/// float. We use this to gate the bignum-fallback path so we don't
/// trip on tokens that contain `e`/`E`/`.` (those are floats) or that
/// aren't numeric at all (those are symbols).
fn looks_like_integer(s: &str) -> bool {
    let s = if s.starts_with('+') || s.starts_with('-') {
        &s[1..]
    } else {
        s
    };
    // Accept a single optional trailing "." as an integer terminator (GNU
    // `string_to_number` lexes "5." / "100." as integers, not floats). The
    // magnitude before the dot must be a non-empty run of digits.
    let digits = s.strip_suffix('.').unwrap_or(s);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

pub(crate) fn parse_emacs_special_float(token: &str) -> Option<f64> {
    const NAN_QUIET_BIT: u64 = 1u64 << 51;
    const NAN_PAYLOAD_MASK: u64 = (1u64 << 51) - 1;
    const NAN_LEADING_DOT_PAYLOAD: u64 = 2_251_799_813_685_246;

    let exp_idx = token.find(['e', 'E'])?;
    let (mantissa, exponent_suffix) = token.split_at(exp_idx);
    let suffix = &exponent_suffix[1..];
    match suffix {
        "+INF" => {
            let mantissa = mantissa.parse::<f64>().ok()?;
            if !mantissa.is_finite() {
                return None;
            }
            Some(if mantissa.is_sign_negative() {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            })
        }
        "+NaN" => {
            let mantissa_value = mantissa.parse::<f64>().ok()?;
            if !mantissa_value.is_finite() {
                return None;
            }

            let body = mantissa
                .strip_prefix('+')
                .or_else(|| mantissa.strip_prefix('-'))
                .unwrap_or(mantissa);

            let mut payload = 0u64;
            if body.starts_with('.') {
                payload = NAN_LEADING_DOT_PAYLOAD;
            } else {
                let integer_part = body
                    .split_once('.')
                    .map(|(int_part, _)| int_part)
                    .unwrap_or(body);
                let mut any_nonzero = false;
                for digit in integer_part.bytes() {
                    if !digit.is_ascii_digit() {
                        return None;
                    }
                    let value = (digit - b'0') as u64;
                    any_nonzero |= value != 0;
                    payload = ((payload * 10) + value) & NAN_PAYLOAD_MASK;
                }
                if !any_nonzero {
                    payload = 0;
                }
            }

            if payload == 0 {
                return Some(if mantissa_value.is_sign_negative() {
                    -f64::NAN
                } else {
                    f64::NAN
                });
            }

            let sign = if mantissa_value.is_sign_negative() {
                1u64 << 63
            } else {
                0
            };
            let bits = sign | (0x7ffu64 << 52) | NAN_QUIET_BIT | (payload & NAN_PAYLOAD_MASK);
            Some(f64::from_bits(bits))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Latin-1 → UTF-8 recombination for .elc string constants
// ---------------------------------------------------------------------------

/// Re-decode a string that may contain Latin-1 codepoints (0x80–0xFF)
/// which are actually UTF-8 byte sequences decomposed by the `.elc`
/// loader's `b as char` mapping.
///
/// `.elc` files are loaded as Latin-1 (`load.rs:1418`) because bytecode
/// instruction strings contain raw bytes 0x00–0xFF that aren't valid
/// UTF-8. However, this also decomposes multibyte string *constants*
/// — e.g., the 3-byte UTF-8 for U+2018 (LEFT SINGLE QUOTATION MARK)
/// becomes three Latin-1 codepoints U+00E2 U+0080 U+0098.
///
/// This function detects strings whose chars ≤ U+00FF form valid UTF-8
/// when treated as raw bytes, and recombines them into proper Unicode
/// codepoints. Strings that are pure ASCII or contain chars > U+00FF
/// are returned unchanged. Strings whose bytes don't form valid UTF-8
/// (genuine unibyte/bytecode data) are also returned unchanged.
///
/// This mirrors GNU Emacs `lread.c` which reads `.elc` strings in
/// unibyte mode and then re-encodes multibyte strings via
/// `string_to_multibyte`.
/// Build a LispString from reader-produced bytes (Emacs internal encoding).
///
/// GNU reads ordinary source string literals as unibyte when the contents are
/// pure ASCII, even though the same bytes could be represented as multibyte
/// UTF-8. Keep that canonicalization here so `(intern "foo")` and
/// `(intern (string-to-multibyte "foo"))` name the same symbol.
///
/// Non-ASCII reader bytes stay multibyte and go through `from_emacs_bytes`
/// so Emacs internal encoding still counts characters correctly.
fn maybe_recombine_latin1_emacs(data: Vec<u8>) -> crate::heap_types::LispString {
    if data.is_empty() || data.iter().all(|&b| b < 0x80) {
        return crate::heap_types::LispString::from_unibyte(data);
    }
    // The bytes are in Emacs internal encoding (may contain C0/C1 overlong
    // for raw bytes, or standard multi-byte UTF-8 for Unicode).
    crate::heap_types::LispString::from_emacs_bytes(data)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
