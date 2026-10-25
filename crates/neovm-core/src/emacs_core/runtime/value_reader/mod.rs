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

mod old_unicode_names;

use super::eval::push_scratch_gc_root;
use super::intern::intern;
use malachite::base::num::conversion::traits::FromStringBase;
use malachite::integer::Integer;
// bytes_to_unibyte_storage_string and encode_nonunicode_char_for_storage
// imports removed — using emacs_char + Vec<u8> directly
use super::emacs_char;
use crate::buffer::{
    Buffer, CharLen, CharPos0, CharRange, EmacsByteLen, EmacsBytePos, EmacsByteRange,
};
use crate::tagged::SymbolMode;
use smallvec::SmallVec;
use std::cell::Cell;

use super::builtins::collections::lookup_hash_table_test_alias;
use super::value::{
    HashTableLiteralKey, HashTableTest, HashTableWeakness, Value, build_hash_table_literal_value,
    eq_value, get_string_text_properties_for_value, list_to_vec,
    set_string_text_properties_for_value,
};

const UNICODE_CHARACTER_NAME_LENGTH_BOUND: usize = 200;
const CHAR_CODE_MASK: u32 = 0x3F_FFFF;
fn apply_control_modifier(value: DecodedCharacter) -> DecodedCharacter {
    let (code, mut modifiers) = value.parts();
    let code = if code == b'?' as u32 {
        0x7F
    } else if (b'@' as u32..=b'_' as u32).contains(&code)
        || (b'a' as u32..=b'z' as u32).contains(&code)
    {
        code & 0x1F
    } else {
        modifiers.insert(CharacterModifiers::CONTROL);
        code
    };
    DecodedCharacter(code | modifiers.bits())
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
pub fn read_all(
    input: &str,
    vars: crate::emacs_core::eval::VariableView<'_>,
) -> Result<Vec<Value>, ReadError> {
    read_all_with_source_multibyte(input, true, vars)
}

/// Read all top-level forms from `input`, preserving the source string's
/// multibyte/unibyte distinction where it affects reader results.
pub fn read_all_with_source_multibyte(
    input: &str,
    source_multibyte: bool,
    vars: crate::emacs_core::eval::VariableView<'_>,
) -> Result<Vec<Value>, ReadError> {
    let mut reader = Reader::new(
        input,
        ReaderSourceSemantics::for_lisp_object(source_multibyte),
        vars,
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
    vars: crate::emacs_core::eval::VariableView<'_>,
) -> Result<Vec<Value>, ReadError> {
    let range = CharRange::from_start_len(CharPos0::new(0), CharLen::new(input.schars()));
    let range = CharacterReadRange::for_source(input, range, EmacsBytePos::ZERO)?;
    let mut reader = Reader::new_lisp_string(input, LispReaderRange::Characters(range), vars);
    let mut forms = Vec::new();
    while reader.skip_ws_and_comments() {
        // GNU resets read_objects_map for every read; labels never cross forms.
        reader.read_labels = ReadLabels::for_source(&reader.source);
        reader.dot_syntax.set(None);
        reader.step_cache.set(None);
        forms.push(reader.read_form()?);
    }
    Ok(forms)
}

/// Read a single form from `input` starting at byte offset `start`.
/// Returns `None` if there is nothing to read (only whitespace/comments remain).
/// On success returns `(value, end_position)`.
pub fn read_one(
    input: &str,
    start: usize,
    vars: crate::emacs_core::eval::VariableView<'_>,
) -> Result<Option<(Value, usize)>, ReadError> {
    read_one_with_source_multibyte(input, true, start, vars)
}

/// Read a single form from `input`, preserving whether the original source was
/// multibyte or unibyte.
pub fn read_one_with_source_multibyte(
    input: &str,
    source_multibyte: bool,
    start: usize,
    vars: crate::emacs_core::eval::VariableView<'_>,
) -> Result<Option<(Value, usize)>, ReadError> {
    let mut reader = Reader::new(
        input,
        ReaderSourceSemantics::for_lisp_object(source_multibyte),
        vars,
    );
    reader.advance_raw_to(EmacsBytePos::new(start));
    if !reader.skip_ws_and_comments() {
        return Ok(None);
    }
    let value = reader.read_form()?;
    Ok(Some((value, reader.pos)))
}

/// Read one form from encoded compiled-file bytes, retaining byte offsets.
///
/// This is intentionally separate from the unibyte string API above.  GNU
/// decodes file input but preserves each byte of unibyte Lisp objects.
pub(crate) fn read_one_from_encoded_file_bytes<'a>(
    input: &'a [u8],
    start: usize,
    origin: crate::emacs_core::doc::DocBytePosition,
    saved_doc_blocks: &'a mut SavedDocBlocks,
    vars: crate::emacs_core::eval::VariableView<'a>,
    shorthands: Option<&ReadSymbolShorthands>,
) -> Result<Option<(Value, usize)>, ReadError> {
    let mut reader = Reader::new_file_bytes(input, origin, saved_doc_blocks, vars);
    reader.shorthands = shorthands.into();
    reader.advance_raw_to(EmacsBytePos::new(start));
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
    vars: crate::emacs_core::eval::VariableView<'_>,
) -> Result<Option<(Value, usize)>, ReadError> {
    read_one_with_locate_syms_with_symbols(
        input,
        source_multibyte,
        start,
        locate_syms,
        vars,
        SymbolMode::Bare,
    )
}

pub fn read_one_with_locate_syms_with_symbols(
    input: &str,
    source_multibyte: bool,
    start: usize,
    locate_syms: bool,
    vars: crate::emacs_core::eval::VariableView<'_>,
    mode: SymbolMode,
) -> Result<Option<(Value, usize)>, ReadError> {
    let mut reader = Reader::new(
        input,
        ReaderSourceSemantics::for_lisp_object(source_multibyte),
        vars,
    );
    reader.advance_raw_to(EmacsBytePos::new(start));
    reader.readchar_offset_origin = ReadcharOffsetOrigin::RelativeToSourceByte(start);
    reader.symbol_position_mode = SymbolPositionMode::from(locate_syms);
    reader.symbol_mode = mode;
    if !reader.skip_ws_and_comments() {
        return Ok(None);
    }
    let value = reader.read_form()?;
    Ok(Some((value, reader.pos)))
}

#[path = "callable.rs"]
mod callable;

pub(crate) use callable::read_from_callable;

mod lazy_doc;

pub(crate) use lazy_doc::SavedDocBlocks;
use lazy_doc::{LazyDocPolicy, LazyDocRef, LiveLoadFileName};

/// Read a single form directly from a buffer byte range.
///
/// GNU Emacs' reader installs buffer get/unget callbacks for buffer streams;
/// it does not copy the accessible region into a temporary string on each
/// `(read (current-buffer))`.  Keep the same model here so repeated buffer
/// reads, such as `unidata-gen.el`, advance through the original buffer.
pub(crate) fn read_one_from_buffer_with_locate_syms_with_symbols(
    buffer: &Buffer,
    range: EmacsByteRange,
    readchar_offset_origin: BufferReadcharOffsetOrigin,
    symbol_position_mode: SymbolPositionMode,
    vars: crate::emacs_core::eval::VariableView<'_>,
    shorthands: Option<&ReadSymbolShorthands>,
    mode: SymbolMode,
) -> Result<(Option<Value>, EmacsBytePos), ReadError> {
    let mut reader = Reader::new_buffer(buffer, range, readchar_offset_origin, vars);
    reader.symbol_position_mode = symbol_position_mode;
    reader.symbol_mode = mode;
    reader.shorthands = shorthands.into();
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
        // A preview is a validated BYTE span, even when the source's GNU
        // declaration maps its endpoints to negative character coordinates.
        // It does not construct a substring or carry text properties.
        let slice = &self.input.as_bytes()[start..end];
        // Issue #131: this slice is only used for human-readable load/trace form
        // previews, so a lossy UTF-8 rendering (real PUA preserved, eight-bit ->
        // U+FFFD) is correct and avoids the buggy storage-string sentinels.
        crate::emacs_core::emacs_char::to_utf8_lossy(slice)
    }

    pub fn read_one(
        &self,
        start: usize,
        vars: crate::emacs_core::eval::VariableView<'_>,
    ) -> Result<Option<(Value, usize)>, ReadError> {
        self.read_one_range(start, self.logical_len(), vars)
    }

    pub(crate) fn read_one_with_shorthands(
        &self,
        start: usize,
        vars: crate::emacs_core::eval::VariableView<'_>,
        shorthands: Option<&ReadSymbolShorthands>,
    ) -> Result<Option<(Value, usize)>, ReadError> {
        let mut reader = Reader::new_lisp_string(
            self.input,
            LispReaderRange::Bytes(EmacsByteRange::new(
                EmacsBytePos::new(start),
                EmacsBytePos::new(self.logical_len()),
            )),
            vars,
        );
        reader.shorthands = shorthands.into();
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
        vars: crate::emacs_core::eval::VariableView<'_>,
    ) -> Result<Option<(Value, usize)>, ReadError> {
        let mut reader = Reader::new_lisp_string(
            self.input,
            LispReaderRange::Bytes(EmacsByteRange::new(
                EmacsBytePos::new(start),
                EmacsBytePos::new(end),
            )),
            vars,
        );
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
        vars: crate::emacs_core::eval::VariableView<'_>,
    ) -> Result<Option<(Value, usize)>, ReadError> {
        self.read_one_with_locate_syms_with_symbols(start, locate_syms, vars, SymbolMode::Bare)
    }

    pub fn read_one_with_locate_syms_with_symbols(
        &self,
        start: usize,
        locate_syms: bool,
        vars: crate::emacs_core::eval::VariableView<'_>,
        mode: SymbolMode,
    ) -> Result<Option<(Value, usize)>, ReadError> {
        self.read_one_range_with_locate_syms_with_symbols(
            start,
            self.logical_len(),
            SymbolPositionMode::from(locate_syms),
            vars,
            None,
            mode,
        )
    }

    pub(crate) fn read_one_character_range_with_locate_syms(
        &self,
        range: CharacterReadRange<'a>,
        symbol_position_mode: SymbolPositionMode,
        vars: crate::emacs_core::eval::VariableView<'_>,
        shorthands: ReadSymbolShorthandBinding<'_>,
    ) -> Result<Option<(Value, CharPos0)>, ReadError> {
        self.read_one_character_range_with_locate_syms_with_symbols(
            range,
            symbol_position_mode,
            vars,
            shorthands,
            SymbolMode::Bare,
        )
    }

    /// Lisp START/END are logical character coordinates, not byte bounds.
    pub(crate) fn read_one_character_range_with_locate_syms_with_symbols(
        &self,
        range: CharacterReadRange<'a>,
        symbol_position_mode: SymbolPositionMode,
        vars: crate::emacs_core::eval::VariableView<'_>,
        shorthands: ReadSymbolShorthandBinding<'_>,
        mode: SymbolMode,
    ) -> Result<Option<(Value, CharPos0)>, ReadError> {
        let mut reader =
            Reader::new_lisp_string(self.input, LispReaderRange::Characters(range), vars);
        reader.symbol_position_mode = symbol_position_mode;
        reader.symbol_mode = mode;
        reader.shorthands = match shorthands {
            ReadSymbolShorthandBinding::Unbound => ReaderShorthands::Unbound,
            ReadSymbolShorthandBinding::Snapshot(snapshot) => ReaderShorthands::Snapshot(snapshot),
        };
        if !reader.skip_ws_and_comments() {
            return Ok(None);
        }
        let value = reader.read_form()?;
        let position = reader
            .character_cursor
            .position()
            .ok_or_else(|| reader.ordinary_error("Character cursor unavailable for byte source"))?;
        Ok(Some((value, position)))
    }

    pub(crate) fn read_one_range_with_locate_syms_with_symbols(
        &self,
        start: usize,
        end: usize,
        symbol_position_mode: SymbolPositionMode,
        vars: crate::emacs_core::eval::VariableView<'_>,
        shorthands: Option<&ReadSymbolShorthands>,
        mode: SymbolMode,
    ) -> Result<Option<(Value, usize)>, ReadError> {
        let mut reader = Reader::new_lisp_string(
            self.input,
            LispReaderRange::Bytes(EmacsByteRange::new(
                EmacsBytePos::new(start),
                EmacsBytePos::new(end),
            )),
            vars,
        );
        reader.symbol_position_mode = symbol_position_mode;
        reader.symbol_mode = mode;
        reader.shorthands = shorthands.into();
        if !reader.skip_ws_and_comments() {
            return Ok(None);
        }
        let value = reader.read_form()?;
        Ok(Some((value, reader.pos)))
    }
}

/// Symbol materialization mode for one reader. This value contains no Lisp
/// state and can be copied between mutators; a reader's source remains local.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SymbolPositionMode {
    Bare,
    Positioned,
}

/// Keep the inherited public boolean API compatible at its boundary.
impl From<bool> for SymbolPositionMode {
    #[inline]
    fn from(position_symbols: bool) -> Self {
        match position_symbols {
            false => Self::Bare,
            true => Self::Positioned,
        }
    }
}

static_assertions::assert_impl_all!(SymbolPositionMode: Send, Sync, Copy);

#[derive(Clone, Debug, Default)]
pub(crate) struct ReadSymbolShorthands {
    pairs: Vec<(crate::heap_types::LispString, crate::heap_types::LispString)>,
}

impl ReadSymbolShorthands {
    pub(crate) fn from_lisp_value(value: Value) -> Option<Self> {
        let pairs = crate::emacs_core::value::list_iter(value)
            .filter_map(|entry| {
                if !entry.is_cons() {
                    return None;
                }
                let shorthand = entry.cons_car().as_lisp_string()?.clone();
                let longhand = entry.cons_cdr().as_lisp_string()?.clone();
                Some((shorthand, longhand))
            })
            .collect::<Vec<_>>();
        Some(Self { pairs })
    }

    /// `read-symbol-shorthands` from a file's Local Variables section.
    ///
    /// GNU `hack-local-variables--find-variables` sorts that value by the
    /// length of each shorthand, longest first, with the stable `sort`
    /// (lisp/files.el:4462-4467), so the first-match scan in
    /// `oblookup_considering_shorthand` (lread.c:4966-5010) prefers the
    /// longest prefix. A value bound any other way keeps its own order.
    pub(crate) fn from_file_local_value(value: Value) -> Option<Self> {
        let mut shorthands = Self::from_lisp_value(value)?;
        shorthands
            .pairs
            .sort_by_key(|(short, _)| std::cmp::Reverse(short.schars()));
        Some(shorthands)
    }

    pub(crate) fn rewrite(
        &self,
        name: &crate::heap_types::LispString,
    ) -> Result<Option<crate::heap_types::LispString>, ReadShorthandError> {
        let input = name.as_bytes();
        for (short, long) in &self.pairs {
            let short_bytes = short.as_bytes();
            if let Some(suffix) = input.strip_prefix(short_bytes) {
                let capacity = long
                    .sbytes()
                    .checked_add(suffix.len())
                    .filter(|&capacity| capacity <= isize::MAX as usize)
                    .ok_or(crate::heap_types::GnuDeclaredCharacterCountError::TooLarge)?;
                let mut rewritten = Vec::with_capacity(capacity);
                rewritten.extend_from_slice(long.as_bytes());
                rewritten.extend_from_slice(suffix);
                if !name.is_multibyte() {
                    // Keep the inherited UB representation. GNU contradictory
                    // UB headers/padding require a separate registry/source
                    // contract and are outside this bounded metadata repair.
                    return Ok(Some(crate::heap_types::LispString::from_unibyte(rewritten)));
                }
                // GNU lread.c:5004 preserves this signed arithmetic when the
                // reader calls make_specified_string (alloc.c:2273-2290).
                let declared =
                    long.schars() as i128 - short.schars() as i128 + name.schars() as i128;
                let chars = match crate::heap_types::GnuDeclaredCharacterCount::try_from(declared) {
                    Ok(chars) => chars,
                    Err(crate::heap_types::GnuDeclaredCharacterCountError::Negative) => {
                        recount_specified_multibyte_name(&rewritten)?
                    }
                    Err(error @ crate::heap_types::GnuDeclaredCharacterCountError::TooLarge) => {
                        return Err(error.into());
                    }
                };
                return Ok(Some(
                    crate::heap_types::LispString::from_emacs_bytes_with_declared_chars(
                        rewritten, chars,
                    ),
                ));
            }
        }
        Ok(None)
    }
}

/// Invalid GNU specified-name construction metadata.
/// Threading: immutable error data without Lisp references or callbacks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum ReadShorthandError {
    #[error(transparent)]
    CharacterCount(#[from] crate::heap_types::GnuDeclaredCharacterCountError),
    #[error("Invalid multibyte symbol-name encoding during character recount")]
    InvalidMultibyteRecount,
}

static_assertions::assert_impl_all!(ReadShorthandError: Send, Sync);

/// GNU make_specified_string uses negative counts as a recount sentinel.
/// Its strict scanner aborts on invalid sequences (character.c:519-535).
/// This bounded scanner reports typed failure instead of a Rust panic.
#[cold]
fn recount_specified_multibyte_name(
    bytes: &[u8],
) -> Result<crate::heap_types::GnuDeclaredCharacterCount, ReadShorthandError> {
    let mut remaining = bytes;
    let mut count = 0usize;
    while !remaining.is_empty() {
        let width = emacs_char::multibyte_length(remaining, true)
            .filter(|&width| width > 0 && width <= remaining.len())
            .ok_or(ReadShorthandError::InvalidMultibyteRecount)?;
        remaining = &remaining[width..];
        count += 1;
    }
    Ok(crate::heap_types::GnuDeclaredCharacterCount::try_from(
        count as i128,
    )?)
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ReadError {
    pub message: String,
    pub position: usize,
    pub kind: ReadErrorKind,
    pub signal_symbol: Option<super::intern::SymId>,
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
    /// A loaded `.elc` file's raw bytes.  GNU reads a byte-compiled file
    /// straight from the file (`readbyte_from_file`); this port used to widen
    /// every byte into a Latin-1 `String` first, which copied the whole file
    /// and made a byte >= 0x80 two bytes wide for the rest of the read.
    FileBytes(&'a [u8]),
    LispString(&'a crate::heap_types::LispString),
    Buffer(&'a Buffer),
    Callable(&'a callable::CallableSource<'a>),
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
        match self {
            Self::MultibyteCharacters | Self::EncodedFileBytes => true,
            Self::UnibyteCharacters => false,
        }
    }
}

/// Reader environments belong to the current mutator. Callable source-buffer
/// borrows never span callbacks; its Context is borrowed for each funcall.
#[derive(Clone, Copy, Debug)]
enum ReaderEnvironment<'a> {
    Variables(crate::emacs_core::eval::VariableView<'a>),
    Callable(&'a callable::CallableSource<'a>),
}

impl ReaderEnvironment<'_> {
    fn value(self, name: &str) -> Option<Value> {
        match self {
            Self::Variables(vars) => vars.value_by_name(name),
            Self::Callable(source) => source.variable(name),
        }
    }
}

/// Snapshot shorthand binding at Lisp string entry. A snapshot borrows
/// mutator-confined LispString data; it never crosses owner threads and has no
/// evaluator, callback, GC handle or TLS state. Optionality is converted
/// only at inherited API boundaries.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ReadSymbolShorthandBinding<'a> {
    Unbound,
    Snapshot(&'a ReadSymbolShorthands),
}

impl<'a> From<Option<&'a ReadSymbolShorthands>> for ReadSymbolShorthandBinding<'a> {
    fn from(value: Option<&'a ReadSymbolShorthands>) -> Self {
        match value {
            None => Self::Unbound,
            Some(snapshot) => Self::Snapshot(snapshot),
        }
    }
}

/// Shorthand binding lifetime: byte-backed input snapshots its binding; a
/// callable may change it at every get/unget boundary in the current mutator.
#[derive(Clone, Copy, Debug)]
enum ReaderShorthands<'a> {
    Unbound,
    Snapshot(&'a ReadSymbolShorthands),
    LiveCallable(&'a callable::CallableSource<'a>),
}

impl<'a> From<Option<&'a ReadSymbolShorthands>> for ReaderShorthands<'a> {
    fn from(value: Option<&'a ReadSymbolShorthands>) -> Self {
        match value {
            None => Self::Unbound,
            Some(shorthands) => Self::Snapshot(shorthands),
        }
    }
}

impl ReaderShorthands<'_> {
    fn rewrite(
        self,
        name: &crate::heap_types::LispString,
    ) -> Result<Option<crate::heap_types::LispString>, ReadShorthandError> {
        match self {
            Self::Unbound => Ok(None),
            Self::Snapshot(shorthands) => shorthands.rewrite(name),
            Self::LiveCallable(source) => match source
                .variable("read-symbol-shorthands")
                .and_then(ReadSymbolShorthands::from_lisp_value)
            {
                Some(shorthands) => shorthands.rewrite(name),
                None => Ok(None),
            },
        }
    }
}

/// GNU keeps read_objects_map rooted (lread.c:5898-5901). Byte-backed
/// parsers make no Lisp calls; callable input can collect at every character,
/// so every label entry retains its value beyond temporary parser scopes.
#[derive(Debug)]
enum ReadLabels {
    ByteBacked(std::collections::HashMap<ReadLabel, Value>),
    Callable(std::collections::HashMap<ReadLabel, crate::emacs_core::error::RetainedLispValue>),
}

impl ReadLabels {
    fn for_source(source: &ReaderSource<'_>) -> Self {
        match source {
            ReaderSource::Callable(_) => Self::Callable(std::collections::HashMap::new()),
            ReaderSource::Runtime(_)
            | ReaderSource::FileBytes(_)
            | ReaderSource::LispString(_)
            | ReaderSource::Buffer(_) => Self::ByteBacked(std::collections::HashMap::new()),
        }
    }

    fn insert(&mut self, label: ReadLabel, value: Value) {
        match self {
            Self::ByteBacked(labels) => {
                labels.insert(label, value);
            }
            Self::Callable(labels) => {
                labels.insert(label, value.into());
            }
        }
    }

    fn get(&self, label: &ReadLabel) -> Option<&Value> {
        match self {
            Self::ByteBacked(labels) => labels.get(label),
            Self::Callable(labels) => labels.get(label).map(AsRef::as_ref),
        }
    }
}

/// A source range keeps byte-oriented load callers distinct from Lisp bounds.
#[derive(Clone, Copy, Debug)]
enum LispReaderRange<'a> {
    Bytes(EmacsByteRange),
    Characters(CharacterReadRange<'a>),
}

/// The original Lisp string and its payload belong to one mutator. The
/// parser may own a snapshot, but GNU's position cache must retain this actual
/// object identity and actual payload pointer (lread.c:433-437).
#[derive(Clone, Copy, Debug)]
struct OriginalStringReadSource<'a> {
    identity: Value,
    payload: &'a crate::heap_types::LispString,
    _mutator: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[derive(Debug, thiserror::Error)]
enum OriginalStringReadSourceError {
    #[error("Original character reader source is not a Lisp string")]
    NotString,
}
static_assertions::assert_impl_all!(OriginalStringReadSourceError: Send, Sync);

impl<'a> TryFrom<&'a Value> for OriginalStringReadSource<'a> {
    type Error = OriginalStringReadSourceError;

    fn try_from(identity: &'a Value) -> Result<Self, Self::Error> {
        let payload = identity
            .as_lisp_string()
            .ok_or(OriginalStringReadSourceError::NotString)?;
        Ok(Self {
            identity: *identity,
            payload,
            _mutator: std::marker::PhantomData,
        })
    }
}

/// Detached byte-backed snapshots have no Lisp object identity. Native string
/// character reads carry their original heap payload alongside the snapshot.
/// Threading: original Lisp values/borrows cannot leave their current mutator;
/// the existing context-owned position cache is reused, with no new TLS.
#[derive(Clone, Copy, Debug)]
enum OriginalStringCacheBinding<'a> {
    Uncached,
    Original(OriginalStringReadSource<'a>),
}

/// Logical bounds paired with START's byte coordinate chosen at the entry
/// boundary. An original binding retains the identity used by GNU unget.
/// Threading: may borrow mutator-local heap data; never Send or Sync.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CharacterReadRange<'a> {
    characters: CharRange,
    start_byte: EmacsBytePos,
    original: OriginalStringCacheBinding<'a>,
}

impl<'a> CharacterReadRange<'a> {
    pub(crate) fn for_source(
        source: &crate::heap_types::LispString,
        characters: CharRange,
        start_byte: EmacsBytePos,
    ) -> Result<Self, ReadError> {
        if characters.start().get() > characters.end().get()
            || characters.end().get() > source.schars()
            || start_byte.get() > source.sbytes()
        {
            return Err(ReadError {
                message: "Character reader range exceeds source".into(),
                position: start_byte.get(),
                kind: ReadErrorKind::Error,
                signal_symbol: None,
                signal_data: Vec::new(),
            });
        }
        Ok(Self {
            characters,
            start_byte,
            original: OriginalStringCacheBinding::Uncached,
        })
    }

    pub(crate) fn for_original(
        identity: &'a Value,
        characters: CharRange,
        start_byte: EmacsBytePos,
    ) -> Result<Self, ReadError> {
        let original = OriginalStringReadSource::try_from(identity).map_err(|error| ReadError {
            message: error.to_string(),
            position: start_byte.get(),
            kind: ReadErrorKind::Error,
            signal_symbol: None,
            signal_data: Vec::new(),
        })?;
        let mut range = Self::for_source(original.payload, characters, start_byte)?;
        range.original = OriginalStringCacheBinding::Original(original);
        Ok(range)
    }
}

/// Logical source progress without rescanning a normal source at creation.
/// PhysicalEnd is a proof that the requested character count is at least the
/// remaining byte length: every bounded decode consumes at least one byte.
/// Threading: immutable numeric units except for reader-local cursor updates;
/// no Lisp state, heap reference, callback or TLS cache.
#[derive(Clone, Copy, Debug)]
enum SourceCharacterCursor {
    Bytes,
    PhysicalEnd { position: CharPos0, end: CharPos0 },
    CharacterEnd { position: CharPos0, end: CharPos0 },
}

impl SourceCharacterCursor {
    fn for_character_range(range: CharRange, available_bytes: EmacsByteLen) -> Self {
        if range.len().get() >= available_bytes.get() {
            Self::PhysicalEnd {
                position: range.start(),
                end: range.end(),
            }
        } else {
            Self::CharacterEnd {
                position: range.start(),
                end: range.end(),
            }
        }
    }

    #[inline(always)]
    fn permits_offset(self, offset: CharLen) -> bool {
        match self {
            Self::Bytes | Self::PhysicalEnd { .. } => true,
            Self::CharacterEnd { position, end } => {
                offset.get() < end.get().saturating_sub(position.get())
            }
        }
    }

    #[inline]
    fn ascii_limit(self, byte: EmacsBytePos, limit: EmacsBytePos) -> EmacsBytePos {
        match self {
            Self::Bytes | Self::PhysicalEnd { .. } => limit,
            Self::CharacterEnd { position, end } => EmacsBytePos::new(
                limit.get().min(
                    byte.get()
                        .saturating_add(end.get().saturating_sub(position.get())),
                ),
            ),
        }
    }

    #[inline(always)]
    fn advance(&mut self, chars: CharLen) {
        match self {
            Self::Bytes => {}
            Self::PhysicalEnd { position, .. } => *position = position.add_len(chars),
            Self::CharacterEnd { position, end } => {
                *position =
                    CharPos0::new(position.get().saturating_add(chars.get()).min(end.get()));
            }
        }
    }

    /// `permits_offset(0)` and then `advance(1)` in one dispatch: the range
    /// check and the step of one consumed character. Leaves the cursor
    /// unchanged when the range is exhausted.
    #[inline(always)]
    fn take_one(&mut self) -> bool {
        match self {
            Self::Bytes => true,
            Self::PhysicalEnd { position, .. } => {
                *position = position.add_len(CharLen::new(1));
                true
            }
            Self::CharacterEnd { position, end } => {
                let admitted = position.get() < end.get();
                if admitted {
                    *position = CharPos0::new(position.get() + 1);
                }
                admitted
            }
        }
    }

    /// Rare physical-EOF check: admit only initialized byte SBYTES, and
    /// only while the requested logical interval still contains this step.
    fn permits_initialized_terminator(self, offset: CharLen) -> bool {
        match self {
            Self::Bytes => false,
            Self::PhysicalEnd { position, end } | Self::CharacterEnd { position, end } => {
                offset.get() < end.get().saturating_sub(position.get())
            }
        }
    }

    fn after_unread_at(self, byte: EmacsBytePos, limit: EmacsBytePos) -> Self {
        match self {
            Self::Bytes => Self::Bytes,
            Self::PhysicalEnd { position, end } | Self::CharacterEnd { position, end } => {
                Self::for_character_range(
                    CharRange::from_start_len(
                        position,
                        CharLen::new(end.get().saturating_sub(position.get())),
                    ),
                    EmacsByteLen::new(limit.get().saturating_sub(byte.get())),
                )
            }
        }
    }

    #[inline]
    fn position(self) -> Option<CharPos0> {
        match self {
            Self::Bytes => None,
            Self::PhysicalEnd { position, .. } | Self::CharacterEnd { position, .. } => {
                Some(position)
            }
        }
    }
}

/// Validated internal source spans. InitializedTerminator carries only the
/// physical payload interval; rendering appends exactly GNU's initialized NUL.
/// Threading: numeric positions only, with no Lisp value or callback state.
#[derive(Clone, Copy, Debug)]
enum ReaderSourceSpan {
    Payload(EmacsByteRange),
    InitializedTerminator(EmacsByteRange),
}
static_assertions::assert_impl_all!(ReaderSourceSpan: Send, Sync);

/// Both coordinate domains must be restored when GNU unreads an error char.
#[derive(Clone, Copy, Debug)]
struct ReaderCursorCheckpoint {
    byte: EmacsBytePos,
    characters: SourceCharacterCursor,
}

static_assertions::assert_impl_all!(SourceCharacterCursor: Send, Sync);
static_assertions::assert_impl_all!(ReaderCursorCheckpoint: Send, Sync);

/// GNU's dot probe unreads the character after '.', before the grammar
/// consumes the period. A changed byte mapping is resumed when that period is
/// consumed; both domains remain paired. Pure numeric reader-local metadata.
#[derive(Clone, Copy, Debug)]
struct DotUnreadCursor {
    period: EmacsBytePos,
    after_period: ReaderCursorCheckpoint,
}
static_assertions::assert_impl_all!(DotUnreadCursor: Send, Sync);

struct Reader<'a> {
    source: ReaderSource<'a>,
    source_semantics: ReaderSourceSemantics,
    pos: usize,
    limit: usize,
    /// The source's bytes up to `limit` when the source is byte-backed (a
    /// loaded file's text, or a Lisp string); None for a buffer or callable
    /// source, which has no contiguous bytes and keeps the per-character
    /// step. Neither the source nor `limit` changes during a read, so the
    /// slice is taken once, at construction. Every storage-string sentinel
    /// is a code point at or above U+E080 and every multibyte lead byte is
    /// at or above 0x80, so an ASCII byte of either source is that character
    /// and one position wide: the byte-run fast paths read such runs
    /// directly, the way GNU `read0` walks `readchar` bytes.
    source_bytes: Option<&'a [u8]>,
    character_cursor: SourceCharacterCursor,
    original_string: OriginalStringCacheBinding<'a>,
    dot_unread: Cell<Option<DotUnreadCursor>>,
    /// `#N=EXPR` / `#N#` read labels for shared structure in `.elc` files.
    read_labels: ReadLabels,
    /// Materialization mode for reader symbols and their source positions.
    symbol_position_mode: SymbolPositionMode,
    /// Dynamic SYMBOLP/EQ policy, supplied by the user-facing reader boundary.
    symbol_mode: SymbolMode,
    /// Coordinate origin for `symbol-with-pos-pos`.  GNU initializes its
    /// character counter to the absolute point for buffer streams and zero
    /// for strings, markers, functions, and files.
    readchar_offset_origin: ReadcharOffsetOrigin,
    /// The variables the reader sees, for resolving `#$` to the current value of
    /// `load-file-name` (matching GNU's `Vload_file_name`).
    environment: ReaderEnvironment<'a>,
    /// GNU `load-force-doc-strings`, memoized for a byte-backed source on first
    /// consultation; a callable source is consulted live at each completion.
    lazy_doc_policy: Cell<LazyDocPolicy>,
    /// The absolute file position of `source[0]` for a compiled-file byte
    /// slice; `None` for every other source.
    file_origin: Option<crate::emacs_core::doc::DocBytePosition>,
    /// GNU's saved `#@` blocks, owned by the compiled-file read loop and
    /// shared by every top-level form it reads.
    saved_doc_blocks: Option<&'a mut SavedDocBlocks>,
    /// Active `read-symbol-shorthands`, if any.  GNU's reader rewrites symbol
    /// names before interning when this dynamic variable is non-nil.
    shorthands: ReaderShorthands<'a>,
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
    /// The GNU dot lookahead belongs to one source character. A list and its
    /// form dispatcher must not invoke READCHARFUN twice to classify that dot.
    /// Reader-local state is never shared across mutators.
    dot_syntax: Cell<Option<(EmacsBytePos, DotSyntax)>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadcharOffsetOrigin {
    RelativeToSourceByte(usize),
    AbsoluteBufferPosition,
}

/// A circular-reader label that GNU can represent as a nonnegative fixnum.
/// Labels belong to one reader, with no shared state between mutators.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(transparent)]
struct ReadLabel(i64);

impl ReadLabel {
    fn new(value: i64) -> Option<Self> {
        (0..=Value::MOST_POSITIVE_FIXNUM)
            .contains(&value)
            .then_some(Self(value))
    }
}

/// Only ordinary cons lists admit a dotted tail; string-property payloads do not.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ListSyntax {
    Cons,
    StringProperties,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DotSyntax {
    TailMarker,
    SymbolConstituent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CharTableSyntax {
    Table,
    SubTable,
}

/// Whether quoting suppresses numeric interpretation of a symbol token.
/// Owned by one reader; it carries no shared Lisp state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SymbolQuoting {
    Plain,
    Escaped,
}

#[derive(Debug)]
struct ReaderToken {
    /// Raw token bytes, kept inline (SmallVec) so the common short-symbol case
    /// never touches the heap. The `LispString` name is materialized lazily by
    /// `to_lisp_string()` only on the cold paths (uninterned `#:` symbols,
    /// non-ASCII / escaped tokens, shorthand rewrite); ASCII symbols intern
    /// straight from the `&str` view via the interner's no-alloc fast path.
    bytes: ReaderTokenBytes,
    /// Source character domain decides the symbol name's storage encoding.
    /// The token is local to its reader and never shared between mutators.
    source_semantics: ReaderSourceSemantics,
    quoting: SymbolQuoting,
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
        if self.source_semantics.is_multibyte() {
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

    if subtree.is_bare_symbol() || subtree.is_number() {
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
    /// Byte-backed readers retain the boundary's policy snapshot. Callable
    /// input can change it while fetching a character, so consult its context
    /// only at a policy-sensitive semantic boundary.
    fn symbol_mode(&self) -> SymbolMode {
        match self.environment {
            ReaderEnvironment::Variables(_) => self.symbol_mode,
            ReaderEnvironment::Callable(source) => source.symbol_mode(),
        }
    }

    fn new(
        input: &'a str,
        source_semantics: ReaderSourceSemantics,
        vars: crate::emacs_core::eval::VariableView<'a>,
    ) -> Self {
        Self {
            source: ReaderSource::Runtime(input),
            source_semantics,
            pos: 0,
            limit: input.len(),
            source_bytes: Some(input.as_bytes()),
            character_cursor: SourceCharacterCursor::Bytes,
            original_string: OriginalStringCacheBinding::Uncached,
            dot_unread: Cell::new(None),
            read_labels: ReadLabels::for_source(&ReaderSource::Runtime(input)),
            symbol_position_mode: SymbolPositionMode::Bare,
            symbol_mode: SymbolMode::Bare,
            readchar_offset_origin: ReadcharOffsetOrigin::RelativeToSourceByte(0),
            environment: ReaderEnvironment::Variables(vars),
            lazy_doc_policy: Cell::new(LazyDocPolicy::Unknown),
            file_origin: None,
            saved_doc_blocks: None,
            shorthands: ReaderShorthands::Unbound,
            step_cache: Cell::new(None),
            dot_syntax: Cell::new(None),
        }
    }

    /// Read a `.elc` file's bytes directly, the way GNU's `readbyte_from_file`
    /// does.  Each byte is its own character, which is exactly what the
    /// Latin-1 envelope used to deliver, so every token, string and escape
    /// path sees the identical code sequence.
    fn new_file_bytes(
        input: &'a [u8],
        origin: crate::emacs_core::doc::DocBytePosition,
        saved_doc_blocks: &'a mut SavedDocBlocks,
        vars: crate::emacs_core::eval::VariableView<'a>,
    ) -> Self {
        Self {
            source: ReaderSource::FileBytes(input),
            source_semantics: ReaderSourceSemantics::EncodedFileBytes,
            pos: 0,
            limit: input.len(),
            source_bytes: Some(input),
            character_cursor: SourceCharacterCursor::Bytes,
            original_string: OriginalStringCacheBinding::Uncached,
            dot_unread: Cell::new(None),
            read_labels: ReadLabels::for_source(&ReaderSource::FileBytes(input)),
            symbol_position_mode: SymbolPositionMode::Bare,
            symbol_mode: SymbolMode::Bare,
            readchar_offset_origin: ReadcharOffsetOrigin::RelativeToSourceByte(0),
            environment: ReaderEnvironment::Variables(vars),
            lazy_doc_policy: Cell::new(LazyDocPolicy::Unknown),
            file_origin: Some(origin),
            saved_doc_blocks: Some(saved_doc_blocks),
            shorthands: ReaderShorthands::Unbound,
            step_cache: Cell::new(None),
            dot_syntax: Cell::new(None),
        }
    }

    fn new_lisp_string(
        input: &'a crate::heap_types::LispString,
        range: LispReaderRange<'a>,
        vars: crate::emacs_core::eval::VariableView<'a>,
    ) -> Self {
        let (start, end, character_cursor, original_string) = match range {
            LispReaderRange::Bytes(bytes) => (
                bytes.start().get(),
                bytes.end().get(),
                SourceCharacterCursor::Bytes,
                OriginalStringCacheBinding::Uncached,
            ),
            LispReaderRange::Characters(range) => {
                // GNU read_internal_start computes cached START's byte position
                // but retains END as characters (lread.c:2554-2560).
                let start = range.start_byte.get();
                let end = input.sbytes();
                let cursor = SourceCharacterCursor::for_character_range(
                    range.characters,
                    EmacsByteLen::new(end.saturating_sub(start)),
                );
                (start, end, cursor, range.original)
            }
        };
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
            source_bytes: Some(&input.as_bytes()[..end]),
            character_cursor,
            original_string,
            dot_unread: Cell::new(None),
            read_labels: ReadLabels::for_source(&ReaderSource::LispString(input)),
            symbol_position_mode: SymbolPositionMode::Bare,
            symbol_mode: SymbolMode::Bare,
            readchar_offset_origin: ReadcharOffsetOrigin::RelativeToSourceByte(start),
            environment: ReaderEnvironment::Variables(vars),
            lazy_doc_policy: Cell::new(LazyDocPolicy::Unknown),
            file_origin: None,
            saved_doc_blocks: None,
            shorthands: ReaderShorthands::Unbound,
            step_cache: Cell::new(None),
            dot_syntax: Cell::new(None),
        }
    }

    fn new_buffer(
        input: &'a Buffer,
        range: EmacsByteRange,
        readchar_offset_origin: BufferReadcharOffsetOrigin,
        vars: crate::emacs_core::eval::VariableView<'a>,
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
            source_bytes: None,
            character_cursor: SourceCharacterCursor::Bytes,
            original_string: OriginalStringCacheBinding::Uncached,
            dot_unread: Cell::new(None),
            read_labels: ReadLabels::for_source(&ReaderSource::Buffer(input)),
            symbol_position_mode: SymbolPositionMode::Bare,
            symbol_mode: SymbolMode::Bare,
            readchar_offset_origin: match readchar_offset_origin {
                BufferReadcharOffsetOrigin::BufferPoint => {
                    ReadcharOffsetOrigin::AbsoluteBufferPosition
                }
                BufferReadcharOffsetOrigin::Zero => {
                    ReadcharOffsetOrigin::RelativeToSourceByte(start)
                }
            },
            environment: ReaderEnvironment::Variables(vars),
            lazy_doc_policy: Cell::new(LazyDocPolicy::Unknown),
            file_origin: None,
            saved_doc_blocks: None,
            shorthands: ReaderShorthands::Unbound,
            step_cache: Cell::new(None),
            dot_syntax: Cell::new(None),
        }
    }

    // -- Whitespace & comments -----------------------------------------------

    fn skip_ws_and_comments(&mut self) -> bool {
        loop {
            if let Some(src) = self.byte_backed_source() {
                let mut p = self.pos;
                let run = &src[..self.ascii_limit(EmacsBytePos::new(src.len()))];
                loop {
                    while p < run.len() && run[p] <= 32 {
                        p += 1;
                    }
                    if p < src.len()
                        && src[p] == b';'
                        && matches!(self.character_cursor, SourceCharacterCursor::Bytes)
                    {
                        match memchr::memchr(b'\n', &src[p..]) {
                            Some(off) => {
                                p += off + 1;
                                continue;
                            }
                            None => p = src.len(),
                        }
                    }
                    break;
                }
                self.advance_ascii_to(EmacsBytePos::new(p));
            }
            let Some(ch) = self.current_code() else {
                return false;
            };
            if is_reader_whitespace_code(ch) {
                self.bump();
                continue;
            }
            if ch == b';' as u32 {
                // Line comment
                loop {
                    // Past the payload (after an admitted terminator) only
                    // the per-character step can answer the end.
                    if let Some(src) = self.byte_backed_source()
                        && self.pos < src.len()
                    {
                        let end = self.ascii_limit(EmacsBytePos::new(src.len()));
                        let span = &src[self.pos..end];
                        let newline = memchr::memchr(b'\n', span);
                        let candidate = newline.map_or(end, |offset| self.pos + offset + 1);
                        let prefix = &src[self.pos..candidate];
                        if !prefix.is_empty() && prefix.is_ascii() {
                            self.advance_ascii_to(EmacsBytePos::new(candidate));
                            if newline.is_some() {
                                break;
                            }
                            continue;
                        }
                        let ascii = prefix
                            .iter()
                            .position(|byte| *byte >= 0x80)
                            .unwrap_or(prefix.len());
                        if ascii > 0 {
                            self.advance_ascii_to(EmacsBytePos::new(self.pos + ascii));
                            continue;
                        }
                    }
                    match self.take_code() {
                        Some(c) if c != b'\n' as u32 => {}
                        _ => break,
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
        // Record the byte position before reading — used by symbol positioning
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
            x if x == b']' as u32 => {
                self.bump();
                Err(self.error("]"))
            }
            x if x == b'.' as u32 && self.is_dot_separator() => {
                self.bump();
                Err(self.error("."))
            }
            x if x == b'[' as u32 => self.read_vector(),
            x if x == b'\'' as u32 => {
                self.bump();
                let _roots = callable::RootScope::new();
                let quoted = self.read_form()?;
                push_scratch_gc_root(quoted);
                let result = Value::list(vec![Value::symbol("quote"), quoted]);
                Ok(result)
            }
            x if x == b'`' as u32 => {
                self.bump();
                let _roots = callable::RootScope::new();
                let quoted = self.read_form()?;
                push_scratch_gc_root(quoted);
                let result = Value::list(vec![Value::symbol(intern("`")), quoted]);
                Ok(result)
            }
            x if x == b',' as u32 => {
                self.bump();
                if self.current_code() == Some(b'@' as u32) {
                    self.bump();
                    let _roots = callable::RootScope::new();
                    let expr = self.read_form()?;
                    push_scratch_gc_root(expr);
                    let result = Value::list(vec![Value::symbol(intern(",@")), expr]);
                    Ok(result)
                } else {
                    self.unread_source_lookahead()?;
                    let _roots = callable::RootScope::new();
                    let expr = self.read_form()?;
                    push_scratch_gc_root(expr);
                    let result = Value::list(vec![Value::symbol(intern(",")), expr]);
                    Ok(result)
                }
            }
            x if x == b'"' as u32 => self.read_string(),
            x if x == b'?' as u32 => self.read_char_literal(),
            x if x == b'#' as u32 => self.read_hash_syntax(),
            _ => self.read_atom(),
        }?;

        // GNU read0 wraps SYMBOLP(val) && !NILP(val) in positioning mode.
        match self.symbol_position_mode {
            SymbolPositionMode::Bare => Ok(value),
            SymbolPositionMode::Positioned => {
                if value.is_bare_symbol() && !value.is_nil() {
                    let pos_val = Value::fixnum(self.symbol_position(form_start) as i64);
                    Ok(crate::tagged::gc::with_tagged_heap(|heap| {
                        heap.alloc_symbol_with_pos(value, pos_val)
                    }))
                } else {
                    Ok(value)
                }
            }
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
            ReaderSource::Callable(source) => {
                source.character_count(EmacsBytePos::new(start), EmacsBytePos::new(end))
            }
            // One byte, one character.
            ReaderSource::FileBytes(_) => end - start,
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
        self.read_list(ListSyntax::Cons)
    }

    fn read_list(&mut self, syntax: ListSyntax) -> Result<Value, ReadError> {
        self.expect('(')?;
        let _roots = callable::RootScope::new();
        let mut items = Vec::new();
        loop {
            self.skip_ws_and_comments();
            match self.current_code() {
                Some(code) if code == b')' as u32 => {
                    self.bump();
                    let result = Value::list(items);
                    return Ok(result);
                }
                Some(code) if code == b'.' as u32 && self.is_dot_separator() => {
                    match syntax {
                        ListSyntax::StringProperties => {
                            self.bump();
                            return Err(self.error("."));
                        }
                        ListSyntax::Cons => {
                            if items.is_empty() {
                                self.bump();
                                return Err(self.error("."));
                            }
                        }
                    }
                    // Dotted pair
                    self.bump(); // consume '.'
                    let cdr = self.read_form()?;
                    push_scratch_gc_root(cdr);
                    self.skip_ws_and_comments();
                    self.unread_source_lookahead()?;
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
                            // GNU `lread.c:4294-4301`: a completed dotted list
                            // whose car is the exact live `load-file-name`
                            // object and whose cdr is a fixnum is a lazy doc
                            // reference when `load-force-doc-strings` is
                            // non-nil. The admission is typed (identity, never
                            // string equality) and applies to the whole
                            // object's car/cdr, so `(x #$ . N)` is ordinary
                            // data. `acc` is rooted above; the converted string
                            // is rooted here for the enclosing read.
                            let completed = acc;
                            if let Some(reference) =
                                LazyDocRef::admit(completed, self.lazy_doc_policy())
                            {
                                let converted = self.lazy_doc_string(reference)?;
                                push_scratch_gc_root(converted);
                                return Ok(converted);
                            }
                            return Ok(completed);
                        }
                        _ => {
                            return Err(self.error_after_current("expected )"));
                        }
                    }
                }
                Some(_) => {
                    let item = self.read_form()?;
                    push_scratch_gc_root(item);
                    items.push(item);
                }
                None => {
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
    fn is_dot_separator(&mut self) -> bool {
        let position = EmacsBytePos::new(self.pos);
        if let Some((cached_position, syntax)) = self.dot_syntax.get()
            && cached_position == position
        {
            return matches!(syntax, DotSyntax::TailMarker);
        }
        let following = self.peek_code_at(1);
        if let ReaderSource::Callable(source) = self.source {
            let next = source
                .code_and_next(EmacsBytePos::new(self.pos))
                .map(|(_, next)| next)
                .unwrap_or(EmacsBytePos::new(self.pos));
            let _ = source.unread(next);
            self.step_cache.set(None);
        } else if matches!(self.original_string,
                OriginalStringCacheBinding::Original(original)
                    if original.payload.schars() != original.payload.sbytes())
            && following.is_some()
            && let Some(position) = self.character_cursor.position()
            && let Some(mapped) = self.original_unread_byte(position.add_len(CharLen::new(1)))
            && self.next_pos(self.pos) != Some(mapped.get())
        {
            let mut characters = self.character_cursor;
            characters.advance(CharLen::new(1));
            characters = characters.after_unread_at(mapped, EmacsBytePos::new(self.limit));
            self.dot_unread.set(Some(DotUnreadCursor {
                period: EmacsBytePos::new(self.pos),
                after_period: ReaderCursorCheckpoint {
                    byte: mapped,
                    characters,
                },
            }));
        }
        let syntax = match following {
            None => DotSyntax::TailMarker,
            Some(c) => {
                if is_reader_whitespace_code(c)
                    || c == b'"' as u32
                    || c == b'\'' as u32
                    || c == b'(' as u32
                    || c == b';' as u32
                    || c == b'[' as u32
                    || c == b'#' as u32
                    || c == b'?' as u32
                    || c == b'`' as u32
                    || c == b',' as u32
                {
                    DotSyntax::TailMarker
                } else {
                    DotSyntax::SymbolConstituent
                }
            }
        };
        self.dot_syntax.set(Some((position, syntax)));
        matches!(syntax, DotSyntax::TailMarker)
    }

    // -- Vectors [1 2 3] ----------------------------------------------------

    /// Read `[...]` and return items as a Vec<Value>.
    fn read_vector_items(&mut self) -> Result<Vec<Value>, ReadError> {
        self.expect('[')?;
        let _roots = callable::RootScope::new();
        let mut items = Vec::new();
        loop {
            self.skip_ws_and_comments();
            match self.current_code() {
                Some(code) if code == b']' as u32 => {
                    self.bump();
                    return Ok(items);
                }
                Some(_) => {
                    let item = self.read_form()?;
                    push_scratch_gc_root(item);
                    items.push(item);
                }
                None => {
                    return Err(self.end_of_file_error());
                }
            }
        }
    }

    fn read_vector(&mut self) -> Result<Value, ReadError> {
        let _roots = callable::RootScope::new();
        let items = self.read_vector_items()?;
        for item in &items {
            push_scratch_gc_root(*item);
        }
        let result = Value::make_vector(items);
        Ok(result)
    }

    // -- Strings "..." -------------------------------------------------------

    fn read_string(&mut self) -> Result<Value, ReadError> {
        self.expect('"')?;
        let mut buf = Vec::new();
        // GNU `read_string_literal` (`src/lread.c:3041-3170`) accumulates ONE
        // buffer of internal-encoding bytes plus two flags, and decides the
        // literal's multibyteness once at the end: a raw-byte escape forces
        // single-byte, a real multibyte character forces multibyte, and an
        // all-ASCII literal comes out unibyte because `nbytes == nchars`.
        // Carrying a parallel unibyte buffer instead cost a second allocation
        // and a second copy of every string literal the reader saw.
        let mut flags = StringLiteralFlags::default();
        // The source is fixed for the whole literal; only `pos` moves.
        let byte_source = self.byte_backed_source();
        loop {
            // The ASCII run up to the next quote, backslash or non-ASCII
            // character in one copy.
            if let Some(src) = byte_source {
                let start = self.pos;
                let mut p = start;
                // Bounding the slice, not the loop, keeps the per-byte
                // bounds check out of the run.
                let run = &src[..self.ascii_limit(EmacsBytePos::new(src.len()))];
                while p < run.len() {
                    let b = run[p];
                    if b >= 0x80 || b == b'"' || b == b'\\' {
                        break;
                    }
                    p += 1;
                }
                if p > start {
                    buf.extend_from_slice(&src[start..p]);
                    self.advance_ascii_to(EmacsBytePos::new(p));
                }
            }
            let Some(ch) = self.take_code() else {
                return Err(self.end_of_file_error());
            };
            match ch {
                x if x == b'"' as u32 => {
                    return Ok(Value::heap_string(flags.finish(buf)));
                }
                x if x == b'\\' as u32 => {
                    let Some(esc) = self.take_code() else {
                        return Err(self.end_of_file_error());
                    };
                    // GNU lread.c:3060-3077 applies only these string
                    // rules before using the common character decoder.
                    match esc {
                        x if x == b's' as u32 => buf.push(b' '),
                        x if x == b'\n' as u32 || x == b' ' as u32 => {}
                        _ => match self_contained_escape_code(esc) {
                            // No modifier: the code goes in as it is.
                            Some(code) => Self::push_string_char(&mut buf, &mut flags, code),
                            None => {
                                let value = self.read_char_escape(esc)?;
                                self.push_string_escape_value(&mut buf, &mut flags, value)?;
                            }
                        },
                    }
                }
                other => {
                    self.push_source_string_code(&mut buf, &mut flags, other);
                }
            }
        }
    }

    /// Add one character to a string literal, in GNU's shape.
    ///
    /// This is `add_char_to_buffer (&rb, ch, true)` plus the flag update that
    /// follows it in `read_string_literal` (`src/lread.c:3120-3130`): the
    /// buffer always receives the character's internal encoding, and which
    /// kind of character it was is remembered in a flag rather than in a
    /// second buffer.
    fn push_string_char(buf: &mut Vec<u8>, flags: &mut StringLiteralFlags, code: u32) {
        if code < 0x80 {
            // ASCII reads the same in both representations.
            buf.push(code as u8);
            return;
        }

        if emacs_char::char_byte8_p(code) && !flags.multibyte {
            // A raw byte, in a literal that is still only raw bytes: store
            // the byte itself, so the buffer stays the finished string.
            buf.push(emacs_char::char_to_byte8(code));
            return;
        }

        flags.promote_to_multibyte(buf);
        let mut tmp = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = emacs_char::char_string(code, &mut tmp);
        buf.extend_from_slice(&tmp[..len]);
    }

    #[inline]
    fn push_string_escape_value(
        &self,
        buf: &mut Vec<u8>,
        flags: &mut StringLiteralFlags,
        val: DecodedCharacter,
    ) -> Result<(), ReadError> {
        let (mut code, mut modifiers) = val.parts();

        if !emacs_char::char_byte8_p(code) && code < 0x80 {
            if modifiers == CharacterModifiers::CONTROL && code == ' ' as u32 {
                code = 0;
                modifiers = CharacterModifiers::empty();
            }

            if modifiers.contains(CharacterModifiers::SHIFT) {
                if ('A' as u32..='Z' as u32).contains(&code) {
                    modifiers.remove(CharacterModifiers::SHIFT);
                } else if ('a' as u32..='z' as u32).contains(&code) {
                    code -= 'a' as u32 - 'A' as u32;
                    modifiers.remove(CharacterModifiers::SHIFT);
                }
            }

            if modifiers.contains(CharacterModifiers::META) {
                modifiers.remove(CharacterModifiers::META);
                code = emacs_char::unibyte_to_char((code as u8) | 0x80);
            }
        }

        if !modifiers.is_empty() {
            return Err(self.escape_error(CharacterEscapeError::InvalidStringModifier));
        }

        Self::push_string_char(buf, flags, code);
        Ok(())
    }

    fn push_source_string_code(
        &mut self,
        buf: &mut Vec<u8>,
        flags: &mut StringLiteralFlags,
        code: u32,
    ) {
        if self.source_semantics == ReaderSourceSemantics::UnibyteCharacters
            && (0x80..=0xFF).contains(&code)
        {
            // GNU source_string_get/source_buffer_get expose high bytes as
            // BYTE8 characters.  Feeding that domain value into the shared
            // string builder preserves the raw byte and keeps the result
            // unibyte, even when adjacent bytes happen to form valid UTF-8.
            Self::push_string_char(buf, flags, emacs_char::byte8_to_char(code as u8));
            return;
        }

        Self::push_string_char(buf, flags, code);
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
            if value > (CharacterModifiers::META.bits() | (CharacterModifiers::META.bits() - 1)) {
                return Err(self.escape_error(CharacterEscapeError::HexOutOfRange { value }));
            }
            digits += 1;
        }

        self.unread_source_lookahead()?;
        if digits == 0 {
            return Err(self.escape_error(CharacterEscapeError::MissingHexDigit));
        }

        if digits < 3 && (0x80..0x100).contains(&value) {
            return Ok(emacs_char::unibyte_to_char(value as u8));
        }

        Ok(value)
    }

    fn read_char_unicode_escape(&mut self, kind: UnicodeEscapeKind) -> Result<u32, ReadError> {
        let mut value = 0u32;
        for _ in 0..kind.digit_count() {
            let Some(c) = self.current_code() else {
                return Err(
                    self.escape_error(CharacterEscapeError::MalformedUnicode { kind, value })
                );
            };
            self.bump();
            let Some(digit) = ascii_hex_digit_value(c) else {
                return Err(self.escape_error(CharacterEscapeError::NonHexUnicode {
                    character: source_code_for_error(c),
                    code: c,
                }));
            };
            value = (value << 4) + digit;
        }

        if value > 0x10FFFF {
            return Err(self.escape_error(CharacterEscapeError::NonUnicode { value }));
        }

        Ok(value)
    }

    /// Only GNU-shaped error data crosses the reader boundary. Every new
    /// escape failure originates in a typed error with explicit GNU text.
    #[cold]
    #[deny(clippy::wildcard_enum_match_arm)]
    fn escape_error(&self, error: CharacterEscapeError) -> ReadError {
        let message = error.to_string();
        match error {
            CharacterEscapeError::InvalidStringModifier
            | CharacterEscapeError::MissingNameOpeningBrace
            | CharacterEscapeError::InvalidNameCharacter { .. }
            | CharacterEscapeError::NameTooLong
            | CharacterEscapeError::EmptyName
            | CharacterEscapeError::UnknownName { .. } => self.error(&message),
            CharacterEscapeError::HexOutOfRange { .. }
            | CharacterEscapeError::MissingHexDigit
            | CharacterEscapeError::MalformedUnicode { .. }
            | CharacterEscapeError::NonHexUnicode { .. }
            | CharacterEscapeError::NonUnicode { .. }
            | CharacterEscapeError::ModifierWithoutHyphen { .. }
            | CharacterEscapeError::EscapedNewline => self.ordinary_error(&message),
        }
    }

    fn read_unicode_name_escape(&mut self) -> Result<u32, ReadError> {
        let opening_brace = self.current_code();
        if opening_brace.is_some() {
            self.bump();
        }
        if opening_brace != Some(b'{' as u32) {
            return Err(self.escape_error(CharacterEscapeError::MissingNameOpeningBrace));
        }
        let mut name = String::new();
        let mut whitespace = false;

        loop {
            let Some(ch) = self.current_code() else {
                return Err(self.end_of_file_error());
            };
            self.bump();

            if ch == b'}' as u32 {
                break;
            }

            if ch >= 0x80 {
                return Err(
                    self.escape_error(CharacterEscapeError::InvalidNameCharacter { code: ch })
                );
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
                return Err(self.escape_error(CharacterEscapeError::NameTooLong));
            }
            name.push(char::from(ch as u8));
        }

        if name.is_empty() {
            return Err(self.escape_error(CharacterEscapeError::EmptyName));
        }

        character_name_to_code(&name)
            .ok_or_else(|| self.escape_error(CharacterEscapeError::UnknownName { name }))
    }

    // -- Character literals ?a -----------------------------------------------

    fn read_char_literal(&mut self) -> Result<Value, ReadError> {
        self.expect('?')?;
        if let Some(ch) = self.current_code()
            && (ch == b' ' as u32 || ch == b'\t' as u32)
        {
            self.bump();
            return Ok(Value::fixnum(ch as i64));
        }

        let value = self.parse_char_value()?;
        let following = self.current_code();
        self.unread_source_lookahead()?;
        if matches!(following, Some(ch) if !is_char_literal_delimiter_code(ch)) {
            return Err(self.error("?"));
        }
        let (mut code, modifiers) = value.parts();
        if emacs_char::char_byte8_p(code) {
            code = emacs_char::char_to_byte8(code) as u32;
        }

        // Character literals with modifier bits produce values beyond Unicode
        // range.  Emit them as fixnums, matching GNU Emacs where characters
        // are integers.
        Ok(Value::fixnum(i64::from(code | modifiers.bits())))
    }

    /// Character and string literals share GNU's escape decoder. Only the
    /// caller decides how decoded modifier bits project into its domain.
    fn parse_char_value(&mut self) -> Result<DecodedCharacter, ReadError> {
        let Some(ch) = self.current_code() else {
            return Err(self.end_of_file_error());
        };
        self.bump();
        if ch != b'\\' as u32 {
            return Ok(DecodedCharacter(ch));
        }
        let Some(esc) = self.current_code() else {
            return Err(self.end_of_file_error());
        };
        self.bump();
        self.read_char_escape(esc)
    }

    /// GNU `read_char_escape` (lread.c:2604-2864) for the character ESC after
    /// a backslash. The escapes that take no modifier (the named controls,
    /// a character standing for itself, and octal) are answered inline: they
    /// are most of the escapes in docstrings and in a compiled file's
    /// byte-code strings. Every other escape takes
    /// [`Self::read_modified_char_escape`]. Always inlined: as an out-of-line
    /// call its frame alone cost more than the escape.
    #[inline(always)]
    fn read_char_escape(&mut self, esc: u32) -> Result<DecodedCharacter, ReadError> {
        if let Some(code) = self_contained_escape_code(esc) {
            return Ok(DecodedCharacter(code));
        }
        if (u32::from(b'0')..=u32::from(b'7')).contains(&esc) {
            return self.read_octal_escape(esc).map(DecodedCharacter);
        }
        self.read_modified_char_escape(esc)
    }

    /// The octal escape whose first digit is FIRST (lread.c:2697-2717): up to
    /// two more digits, with GNU's unread of the lookahead that ends a
    /// shorter escape. Codes 0x80-0xFF are raw bytes.
    #[inline(always)]
    fn read_octal_escape(&mut self, first: u32) -> Result<u32, ReadError> {
        let mut val = first - u32::from(b'0');
        for _ in 0..2 {
            match self.current_code() {
                Some(c) if (u32::from(b'0')..=u32::from(b'7')).contains(&c) => {
                    self.bump();
                    val = val * 8 + (c - u32::from(b'0'));
                }
                _ => {
                    self.unread_source_lookahead()?;
                    break;
                }
            }
        }
        Ok(if (0x80..0x100).contains(&val) {
            emacs_char::byte8_to_char(val as u8)
        } else {
            val
        })
    }

    /// [`Self::read_char_escape`] for modifier prefixes and the escapes that
    /// read further syntax (`\x`, `\u`, `\U`, `\N{}`, `\s`). Modifier
    /// chains are iterative, so their source length cannot become Rust stack
    /// depth.
    #[inline(never)]
    fn read_modified_char_escape(&mut self, mut esc: u32) -> Result<DecodedCharacter, ReadError> {
        let mut modifiers = CharacterModifiers::empty();
        let mut controls = 0usize;
        let mut value = DecodedCharacter(loop {
            let modifier = match esc {
                x if x == b'M' as u32 => Some(CharacterModifiers::META),
                x if x == b'S' as u32 => Some(CharacterModifiers::SHIFT),
                x if x == b'A' as u32 => Some(CharacterModifiers::ALT),
                x if x == b'H' as u32 => Some(CharacterModifiers::HYPER),
                x if x == b's' as u32 && self.current_code() == Some(b'-' as u32) => {
                    Some(CharacterModifiers::SUPER)
                }
                _ => None,
            };
            let control = esc == b'C' as u32 || esc == b'^' as u32;
            if modifier.is_some() || control {
                if esc != b'^' as u32 {
                    let separator = self.current_code();
                    if separator.is_some() {
                        self.bump();
                    }
                    if separator != Some(b'-' as u32) {
                        return Err(self.escape_error(
                            CharacterEscapeError::ModifierWithoutHyphen {
                                prefix: char::from(esc as u8),
                            },
                        ));
                    }
                }
                if let Some(modifier) = modifier {
                    modifiers |= modifier;
                }
                if control {
                    controls += 1;
                }
                let Some(ch) = self.current_code() else {
                    return Err(self.end_of_file_error());
                };
                self.bump();
                if ch != b'\\' as u32 {
                    break ch;
                }
                let Some(next) = self.current_code() else {
                    return Err(self.end_of_file_error());
                };
                self.bump();
                esc = next;
                continue;
            }
            if let Some(code) = self_contained_escape_code(esc) {
                break code;
            }
            break match esc {
                x if x == b's' as u32 => {
                    self.unread_source_lookahead()?;
                    b' ' as u32
                }
                x if x == b'\n' as u32 => {
                    return Err(self.escape_error(CharacterEscapeError::EscapedNewline));
                }
                x if x == b'x' as u32 => self.read_char_hex_escape()?,
                x if x == b'u' as u32 => self.read_char_unicode_escape(UnicodeEscapeKind::Short)?,
                x if x == b'U' as u32 => self.read_char_unicode_escape(UnicodeEscapeKind::Long)?,
                x if x == b'N' as u32 => self.read_unicode_name_escape()?,
                x if (b'0' as u32..=b'7' as u32).contains(&x) => self.read_octal_escape(x)?,
                other => other,
            };
        });
        for _ in 0..controls {
            value = apply_control_modifier(value);
        }
        Ok(DecodedCharacter(u32::from(value) | modifiers.bits()))
    }

    // -- Hash syntax #' #( etc -----------------------------------------------

    fn read_hash_syntax(&mut self) -> Result<Value, ReadError> {
        let syntax_start = self.pos;
        self.expect('#')?;
        let Some(ch) = self.current_code() else {
            return Err(self.error("#"));
        };

        match ch {
            x if x == b'\'' as u32 => {
                // #'function
                self.bump();
                let _roots = callable::RootScope::new();
                let expr = self.read_form()?;
                push_scratch_gc_root(expr);
                let result = Value::list(vec![Value::symbol("function"), expr]);
                Ok(result)
            }
            x if x == b'_' as u32 => {
                // #_NAME reads NAME as a literal symbol, bypassing
                // `read-symbol-shorthands` and number/t/nil interpretation
                // (GNU lread.c). So `#_foo` -> foo, `#_2` -> the symbol named "2".
                self.bump();
                let token = self.read_symbol_token()?;
                if token.bytes.is_empty() {
                    return self.intern_empty_symbol();
                }
                let name = token.to_lisp_string();
                Ok(Value::from_sym_id(super::intern::intern_reader_name(name)))
            }
            x if x == b'(' as u32 => {
                // GNU src/lread.c:3347-3368 applies each triple using
                // set-text-properties, including replacement and Lisp signals.
                let _roots = callable::RootScope::new();
                let result = (|| {
                    let list = self.read_list(ListSyntax::StringProperties)?;
                    push_scratch_gc_root(list);
                    let items = list_to_vec(&list).ok_or_else(|| self.error("#"))?;
                    let Some((&string, rest)) = items.split_first() else {
                        return Err(self.error("#"));
                    };
                    let target =
                        crate::emacs_core::textprop::StringTextPropertyTarget::try_from(string)
                            .map_err(|_| self.error("#"))?;
                    for chunk in rest.chunks(3) {
                        if chunk.len() != 3 {
                            return Err(self.error("Invalid string property list"));
                        }
                        target
                            .set_properties(chunk[0], chunk[1], chunk[2])
                            .map_err(|flow| {
                                self.flow_signal_error(flow, "Invalid text property argument")
                            })?;
                    }
                    Ok(string)
                })();
                result
            }
            x if x == b'[' as u32 => {
                // #[...] — compiled-function literal in .elc.
                // Produce a closure object directly, matching GNU Emacs's
                // lread.c `bytecode_from_rev_list`.
                let _roots = callable::RootScope::new();
                let items = self.read_vector_items()?;
                for item in &items {
                    push_scratch_gc_root(*item);
                }
                let result = crate::emacs_core::builtins::closure_from_reader_literal_slots(&items)
                    .map_err(|e| {
                        let msg = match e.kind() {
                            crate::emacs_core::error::FlowRef::Signal(sig) => sig
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
                let syntax = if self.current_code() == Some(b'^' as u32) {
                    self.bump();
                    CharTableSyntax::SubTable
                } else {
                    CharTableSyntax::Table
                };
                if self.current_code() != Some(b'[' as u32) {
                    // GNU buffers the offending character, then unreads it
                    // before signalling (src/lread.c:3889-3906).
                    let offending_position = self.cursor_checkpoint();
                    if self.current_code().is_some() {
                        self.bump();
                    }
                    let message = self.source_syntax_string(syntax_start, self.pos);
                    self.restore_cursor(offending_position);
                    self.unread_source_lookahead()?;
                    return Err(self.error(&message));
                }

                let _roots = callable::RootScope::new();
                let items = self.read_vector_items()?;
                for item in &items {
                    push_scratch_gc_root(*item);
                }
                let result = match syntax {
                    CharTableSyntax::Table => {
                        crate::emacs_core::chartable::make_char_table_from_external_slots_with_symbols(
                            &items,
                            self.symbol_mode(),
                        )
                            .map_err(|msg| self.error(&msg))
                    }
                    CharTableSyntax::SubTable => {
                        crate::emacs_core::chartable::make_sub_char_table_from_external_slots(
                            &items,
                        )
                        .map_err(|error| self.ordinary_error(&error.to_string()))
                    }
                };
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
                let token = self.read_symbol_token()?;
                Ok(Value::from_sym_id(
                    super::intern::make_uninterned_reader_symbol(if token.bytes.is_empty() {
                        crate::heap_types::LispString::from_unibyte_slice(b"")
                    } else {
                        token.to_lisp_string()
                    }),
                ))
            }
            x if x == b'$' as u32 => {
                // #$ — expands to the current load file name during read.
                // Matches GNU lread.c: returns Vload_file_name (the actual
                // file path string), not the symbol `load-file-name`.
                self.bump();
                Ok(self
                    .environment
                    .value("load-file-name")
                    .unwrap_or(Value::NIL))
            }
            x if x == b'#' as u32 => {
                // ## — symbol with empty name.
                self.bump();
                self.intern_empty_symbol()
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
                    // GNU `read1` case 's' buffers then unreads the next
                    // character (lread.c:3859), so the signal's data includes
                    // that character while the stream leaves it unconsumed
                    // (e.g. "#s[").  Mirror that: append the consumed character
                    // to the "#s" prefix, or report just "#s" at end of input.
                    let offending_position = self.cursor_checkpoint();
                    let mut message = String::from("#s");
                    if let Some(code) = self.current_code() {
                        self.bump();
                        if let Some(ch) = char::from_u32(code) {
                            message.push(ch);
                        }
                    }
                    self.restore_cursor(offending_position);
                    self.unread_source_lookahead()?;
                    if let Some(nul) = message.find('\0') {
                        message.truncate(nul);
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
                let mut n = i64::from(ch as u8 - b'0');
                self.bump();
                while let Some(d) = self.current_code() {
                    if is_ascii_digit_code(d) {
                        self.bump();
                        n = n
                            .checked_mul(10)
                            .and_then(|value| value.checked_add(i64::from(d as u8 - b'0')))
                            .ok_or_else(|| {
                                self.error(&self.source_syntax_string(syntax_start, self.pos))
                            })?;
                    } else {
                        break;
                    }
                }
                let label = ReadLabel::new(n);
                let suffix = self.current_code();
                if suffix.is_some() {
                    self.bump();
                }
                match (suffix, label) {
                    (Some(code), _) if code == b'r' as u32 || code == b'R' as u32 => {
                        // #NrDIGITS -- radix-N integer.  GNU lread.c checks
                        // this before #N=/#N# read-label syntax.
                        if !(2..=36).contains(&n) {
                            return Err(self.error(&format!("integer, radix {}", n)));
                        }
                        self.read_radix_number(n as u32)
                    }
                    (Some(code), Some(label))
                        if code == b'=' as u32 && self.read_circle_enabled() =>
                    {
                        // #N=EXPR -- GNU lread.c installs a placeholder
                        // before reading EXPR so #N# can refer to recursive
                        // structures being read.
                        let placeholder = Value::cons(Value::NIL, Value::NIL);
                        self.read_labels.insert(label, placeholder);
                        let _roots = callable::RootScope::new();
                        push_scratch_gc_root(placeholder);
                        let expr = match self.read_form() {
                            Ok(expr) => expr,
                            Err(err) => {
                                return Err(err);
                            }
                        };

                        let result = if expr.is_cons() {
                            if eq_value(&expr, &placeholder) {
                                return Err(self.error("nonsensical self-reference"));
                            }
                            placeholder.set_car(expr.cons_car());
                            placeholder.set_cdr(expr.cons_cdr());
                            placeholder
                        } else {
                            substitute_read_placeholder(expr, placeholder);
                            self.read_labels.insert(label, expr);
                            expr
                        };
                        Ok(result)
                    }
                    (Some(code), Some(label))
                        if code == b'#' as u32 && self.read_circle_enabled() =>
                    {
                        // #N# — reference previously defined label N
                        self.read_labels.get(&label).copied().ok_or_else(|| {
                            self.error(&self.source_syntax_string(syntax_start, self.pos))
                        })
                    }
                    _ => Err(self.error(&self.source_syntax_string(syntax_start, self.pos))),
                }
            }
            _ => Err(self.error_after_current(&format!("#{}", source_code_for_error(ch)))),
        }
    }

    /// A Lisp `Flow` from a reader-side native call as a typed read error.
    /// `fallback_message` names the operation when the flow carries no signal
    /// datum (a throw into the reader cannot happen on these paths).
    fn flow_signal_error(
        &self,
        flow: crate::emacs_core::error::Flow,
        fallback_message: &str,
    ) -> ReadError {
        use crate::emacs_core::error::FlowRef;
        match flow.kind() {
            FlowRef::Signal(signal) => {
                let symbol = signal.symbol;
                let message = signal
                    .data
                    .first()
                    .and_then(|value| value.as_str_owned())
                    .unwrap_or_else(|| fallback_message.to_string());
                self.signal_error(
                    crate::emacs_core::error::UserCondition::from(symbol),
                    signal.data.clone(),
                    &message,
                )
            }
            FlowRef::Throw(_) | FlowRef::ThreadBlocked(_) | FlowRef::Shutdown(_) => {
                self.ordinary_error(fallback_message)
            }
        }
    }

    /// GNU `load_force_doc_strings` and the live `load-file-name` object at a
    /// dotted-list completion. A byte-backed source cannot run Lisp during the
    /// read, so its answer is memoized on first use (a read with no dotted list
    /// pays nothing); a callable source can change either binding at every
    /// character, so it is consulted live.
    fn lazy_doc_policy(&self) -> LazyDocPolicy {
        // The live `load-file-name` lookup is made only when the flag is on,
        // so an ordinary read (and every normal `.elc` load) pays one symbol
        // lookup at most.
        fn observe(
            forced: Option<Value>,
            load_file_name: impl FnOnce() -> Option<Value>,
        ) -> LazyDocPolicy {
            if !forced.as_ref().is_some_and(|value| value.is_truthy()) {
                return LazyDocPolicy::Disabled;
            }
            LazyDocPolicy::Forced {
                witness: LiveLoadFileName::observe(load_file_name()),
            }
        }
        match self.environment {
            ReaderEnvironment::Variables(vars) => {
                let cached = self.lazy_doc_policy.get();
                if !matches!(cached, LazyDocPolicy::Unknown) {
                    return cached;
                }
                let policy = observe(vars.value_by_name("load-force-doc-strings"), || {
                    vars.value_by_name("load-file-name")
                });
                self.lazy_doc_policy.set(policy);
                policy
            }
            ReaderEnvironment::Callable(source) => {
                observe(source.variable("load-force-doc-strings"), || {
                    source.variable("load-file-name")
                })
            }
        }
    }

    /// GNU `get_lazy_string` (`src/lread.c:3495-3550`): the saved `#@` block
    /// first, else `get_doc_string (val, 1)`. The result is unibyte; GNU's
    /// non-string-file and unresolved-record answers are nil, a missing file is
    /// its `Cannot open doc string file` sentence, and malformed data signals.
    fn lazy_doc_string(&self, reference: LazyDocRef) -> Result<Value, ReadError> {
        let position = reference.position();
        if let Some(saved) = self.saved_doc_blocks.as_deref()
            && let Some(unquoted) = saved.unquoted(position)
        {
            let bytes = unquoted.map_err(|error| {
                self.flow_signal_error(
                    crate::emacs_core::doc::doc_unquote_error_flow(error),
                    "Invalid data in documentation file",
                )
            })?;
            return Ok(Value::heap_string(
                crate::heap_types::LispString::from_unibyte(bytes),
            ));
        }

        let file_value = reference.file_value();
        let Some(file) = file_value.as_lisp_string() else {
            // GNU `get_doc_string`: `!STRINGP (file)` answers Qnil.
            return Ok(Value::NIL);
        };
        let file = crate::emacs_core::emacs_char::to_utf8_lossy(file.as_bytes());
        let lisp_directory =
            crate::emacs_core::doc::lisp_directory_string(self.environment.value("lisp-directory"));
        match crate::emacs_core::doc::load_lazy_doc_string(
            lisp_directory.as_deref(),
            &file,
            position,
        )
        .map_err(|flow| self.flow_signal_error(flow, "Read error in documentation file"))?
        {
            crate::emacs_core::doc::LazyDocRead::Resolved(string) => Ok(Value::heap_string(string)),
            crate::emacs_core::doc::LazyDocRead::Unresolved => Ok(Value::NIL),
            crate::emacs_core::doc::LazyDocRead::CannotOpen(string) => {
                Ok(Value::heap_string(string))
            }
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

        // GNU `skip_lazy_string` records this block when
        // `load-force-doc-strings` is non-nil and the source is a file
        // (`src/lread.c:3445-3485`). The recorded position is absolute in the
        // file, so a pair read from a later top-level form can still resolve
        // it; the reader's own position is relative to its byte slice.
        let forced = self.lazy_doc_policy().is_forced();
        let block_start = self.pos;
        match self.source {
            ReaderSource::Runtime(_) | ReaderSource::FileBytes(_) => {
                self.skip_exact_source_bytes(len)?
            }
            ReaderSource::LispString(_) | ReaderSource::Buffer(_) | ReaderSource::Callable(_) => {
                self.skip_dynamic_doc_string_in_non_file_source();
            }
        }
        let mut block_error = None;
        if forced
            && let (Some(origin), Some(saved)) =
                (self.file_origin, self.saved_doc_blocks.as_deref_mut())
            && let ReaderSource::FileBytes(bytes) = self.source
        {
            match bytes.get(block_start..self.pos) {
                Some(block) => match origin.checked_advance(block_start) {
                    Some(position) => saved.record(position, block),
                    None => block_error = Some("#@ position overflow"),
                },
                None => block_error = Some("#@ block out of range"),
            }
        }
        if let Some(message) = block_error {
            return Err(self.error(message));
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

        // GNU `lread.c`: the bytes in order, bits past SIZE masked off.
        Ok(crate::emacs_core::boolvec::bool_vector_from_bytes(
            size,
            data_string.as_bytes(),
        ))
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
            if is_ascii_digit_for_radix_code(c, radix) {
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

        let digits_end = self.pos;
        self.unread_source_lookahead()?;
        let digits_source = self.source_slice_string(start, digits_end);
        let digits: String = digits_source
            .chars()
            .filter(|c| *c != '-' && *c != '+')
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
        let _roots = callable::RootScope::new();
        let list = self.read_list_or_dotted()?;
        push_scratch_gc_root(list);

        let mut items: Vec<Value> = Vec::new();
        let mut cursor = list;
        while cursor.is_cons() {
            items.push(cursor.cons_car());
            cursor = cursor.cons_cdr();
        }
        if !cursor.is_nil() {
            return Err(self.error("."));
        }

        if items.is_empty() {
            return Err(self.error("#s"));
        }

        // Check if first element is the symbol `hash-table`
        let is_hash_table = items
            .first()
            .is_some_and(|v| v.is_bare_symbol_named("hash-table"));

        if is_hash_table {
            let result = self.read_hash_table_literal_from_plist(&items[1..]);
            return result;
        }

        // Not a hash-table — it's a record #s(type field1 field2 ...)
        let record_value = Value::make_record(items);
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
            let Some(name) = test_value.as_bare_symbol_name() else {
                return Err(self.signal_error(
                    crate::emacs_core::error::LispCondition::WrongTypeArgument,
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
                            crate::emacs_core::error::LispCondition::Error,
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
                let Some(name) = value.as_bare_symbol_name() else {
                    return Err(self.signal_error(
                        crate::emacs_core::error::LispCondition::Error,
                        vec![Value::string("Invalid hash table weakness"), value],
                        "Invalid hash table weakness",
                    ));
                };
                match HashTableWeakness::from_symbol_name(name) {
                    Some(weakness) => Some(weakness),
                    None => {
                        return Err(self.signal_error(
                            crate::emacs_core::error::LispCondition::Error,
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
                    crate::emacs_core::error::LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), {
                        let mut tail = data_value;
                        while tail.is_cons() {
                            tail = tail.cons_cdr();
                        }
                        tail
                    }],
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
        let token = self.read_symbol_token()?;

        if token.bytes.is_empty() {
            return Err(self.error("expected atom"));
        }

        // GNU resolves even nil/t through the obarray after shorthand
        // expansion (lread.c:4222-4255); core symbols can be uninterned
        // (lread.c:4887-4922). Only bypass spelling lookup when this binding
        // cannot rewrite and the original core identity remains canonical.
        if matches!(self.shorthands, ReaderShorthands::Unbound)
            && let Some(id) = match token.bytes.as_slice() {
                b"nil" => Some(super::intern::NIL_SYM_ID),
                b"t" => Some(super::intern::T_SYM_ID),
                _ => None,
            }
            && super::intern::is_canonical_id(id)
        {
            return Ok(Value::from_sym_id(id));
        }

        // Validate a plain token once; both number parsing and the ordinary
        // symbol path use the same immutable byte spelling.
        let token_text = (token.quoting == SymbolQuoting::Plain)
            .then(|| token.as_utf8())
            .flatten();
        // GNU lread.c:4189-4191 only considers these initial bytes for numbers.
        if matches!(token.bytes.first(), Some(b'0'..=b'9' | b'.' | b'+' | b'-'))
            && let Some(token_text) = token_text
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
        }

        // Emacs reader shorthand: bare ## reads as the symbol with empty name.
        if token.quoting == SymbolQuoting::Plain && token.bytes.as_slice() == b"##" {
            return self.intern_empty_symbol();
        }

        // A computed ASCII spelling has encoding-independent canonical hit
        // identity. Retain this token's exact source encoding only if the
        // shared hit path must create its first canonical symbol.
        if matches!(self.shorthands, ReaderShorthands::Unbound)
            && token.quoting == SymbolQuoting::Plain
            && let Some(text) = token_text
            && let Ok(name) = super::intern::ValidatedAsciiName::try_from(text)
        {
            return Ok(Value::from_sym_id(super::intern::intern_ascii_reader_name(
                name,
                || token.to_lisp_string(),
            )));
        }

        // Cold path: non-ASCII / escaped / shorthand-rewritten symbols keep the
        // exact LispString encoding.
        let name = self.to_lisp_string_for_token(&token)?;
        // The creator performs the name lookup under its write lock and
        // preserves the first exact reader spelling on a miss. A preliminary
        // read-lock lookup would hash every cold token a second time.
        let id = super::intern::intern_reader_name(name);
        Ok(Value::from_sym_id(id))
    }

    /// Empty #_ and ## use Fintern, including empty-prefix shorthands
    /// (GNU lread.c:3851, 3999). Rewritten builtin names are multibyte
    /// (lread.c:4793-4796), independently of the reader source encoding.
    #[cold]
    fn intern_empty_symbol(&self) -> Result<Value, ReadError> {
        let empty = crate::heap_types::LispString::from_emacs_bytes(Vec::new());
        let rewritten = self
            .shorthands
            .rewrite(&empty)
            .map_err(|error| self.ordinary_error(&error.to_string()))?;
        Ok(match rewritten {
            Some(name) => Value::from_sym_id(super::intern::intern_reader_name(name)),
            None => Value::from_sym_id(intern("")),
        })
    }

    /// Build the token's exact `LispString` name, applying any active reader
    /// shorthand rewrite (`read-symbol-shorthands`).
    fn to_lisp_string_for_token(
        &self,
        token: &ReaderToken,
    ) -> Result<crate::heap_types::LispString, ReadError> {
        let name = token.to_lisp_string();
        // GNU exempts only names made entirely of core operator constituents
        // (lread.c:3545-3552, 4222-4229), including escaped spellings.
        if token.bytes.iter().all(|byte| {
            matches!(
                byte,
                b'^' | b'*' | b'+' | b'-' | b'/' | b'<' | b'=' | b'>' | b'_' | b'|'
            )
        }) {
            return Ok(name);
        }
        Ok(self
            .shorthands
            .rewrite(&name)
            .map_err(|error| self.ordinary_error(&error.to_string()))?
            .unwrap_or(name))
    }

    fn read_symbol_token(&mut self) -> Result<ReaderToken, ReadError> {
        let mut bytes = ReaderTokenBytes::new();
        let mut quoting = SymbolQuoting::Plain;
        let byte_source = self.byte_backed_source();
        if self
            .dot_unread
            .get()
            .is_some_and(|pending| pending.period.get() == self.pos)
        {
            // GNU already buffered this period before probing/unreading the
            // next character. Consume it once before the ordinary token run.
            self.push_symbol_token_code(&mut bytes, u32::from(b'.'));
            self.bump();
        }
        loop {
            // The ASCII run of the token in one copy; an escape, a non-ASCII
            // character or the delimiter is left to the per-character step.
            if let Some(src) = byte_source {
                let start = self.pos;
                let mut p = start;
                let run = &src[..self.ascii_limit(EmacsBytePos::new(src.len()))];
                while p < run.len() {
                    let b = run[p];
                    if b >= 0x80 || b == b'\\' || is_symbol_delimiter_code(u32::from(b)) {
                        break;
                    }
                    p += 1;
                }
                if p > start {
                    bytes.extend_from_slice(&src[start..p]);
                    self.advance_ascii_to(EmacsBytePos::new(p));
                }
            }
            let Some(ch) = self.current_code() else {
                break;
            };
            if is_symbol_delimiter_code(ch) {
                break;
            }
            if ch == b'\\' as u32 {
                quoting = SymbolQuoting::Escaped;
                self.bump();
                match self.current_code() {
                    Some(escaped) => {
                        self.push_symbol_token_code(&mut bytes, escaped);
                        self.bump();
                    }
                    None => return Err(self.end_of_file_error()),
                }
                continue;
            }
            self.push_symbol_token_code(&mut bytes, ch);
            self.bump();
        }
        self.unread_source_lookahead()?;
        // Keep `bytes` inline (no into_vec / LispString here). read_atom interns
        // ASCII symbols directly from the &str view (interner no-alloc fast
        // path); only the cold paths materialize a LispString via to_lisp_string.
        Ok(ReaderToken {
            bytes,
            source_semantics: self.source_semantics,
            quoting,
        })
    }

    fn push_symbol_token_code(&self, bytes: &mut ReaderTokenBytes, code: u32) {
        if !self.source_semantics.is_multibyte() {
            // Unibyte object sources expose high bytes as BYTE8 characters,
            // then add_char_to_buffer stores their original bytes (lread.c).
            bytes.push(if emacs_char::char_byte8_p(code) {
                emacs_char::char_to_byte8(code)
            } else {
                code as u8
            });
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

    /// Match GNU unreadchar for actual grammar lookahead. Callable input
    /// invokes Lisp unget; original string input remaps through its position
    /// cache. Detached byte-backed sources retain their byte cursor.
    /// GNU unreadchar after a lookahead. Only a callable stream and a string
    /// whose characters are not its bytes have state to restore; every other
    /// source just keeps its byte position.
    #[inline]
    fn unread_source_lookahead(&mut self) -> Result<(), ReadError> {
        if matches!(self.source, ReaderSource::Callable(_))
            || matches!(self.original_string,
                OriginalStringCacheBinding::Original(original)
                    if original.payload.schars() != original.payload.sbytes())
        {
            return self.unread_stateful_source_lookahead();
        }
        Ok(())
    }

    #[cold]
    #[inline(never)]
    fn unread_stateful_source_lookahead(&mut self) -> Result<(), ReadError> {
        if let ReaderSource::Callable(source) = self.source {
            let outcome = source.unread(EmacsBytePos::new(self.pos));
            self.step_cache.set(None);
            // The source retains the original Flow. Stop immediately before
            // any Lisp object is constructed; finish returns that Flow.
            outcome.map_err(|_| self.end_of_file_error())?;
        } else if matches!(self.original_string,
                OriginalStringCacheBinding::Original(original)
                    if original.payload.schars() != original.payload.sbytes())
            && self.current_code().is_some()
            && let Some(position) = self.character_cursor.position()
            && let Some(byte) = self.original_unread_byte(position)
        {
            // GNU readchar has consumed the lookahead, then unreadchar
            // decrements to this same logical position and remaps its byte
            // coordinate through the ORIGINAL source's shared cache.
            self.pos = byte.get();
            self.character_cursor = self
                .character_cursor
                .after_unread_at(byte, EmacsBytePos::new(self.limit));
            self.step_cache.set(None);
            self.dot_syntax.set(None);
            self.dot_unread.set(None);
        }
        Ok(())
    }

    /// Called only for an actual lookahead: GNU unreadchar(-1) does not
    /// update the source/cache. Equal-count byte strings need no cache walk.
    #[cold]
    #[inline(never)]
    fn original_unread_byte(&self, position: CharPos0) -> Option<EmacsBytePos> {
        match self.original_string {
            OriginalStringCacheBinding::Uncached => None,
            OriginalStringCacheBinding::Original(original) => {
                if original.payload.schars() == original.payload.sbytes() {
                    return None;
                }
                Some(EmacsBytePos::new(
                    crate::emacs_core::string_pos_cache::string_char_to_byte(
                        original.identity,
                        original.payload,
                        position.get(),
                    ),
                ))
            }
        }
    }

    fn cursor_checkpoint(&self) -> ReaderCursorCheckpoint {
        ReaderCursorCheckpoint {
            byte: EmacsBytePos::new(self.pos),
            characters: self.character_cursor,
        }
    }

    fn restore_cursor(&mut self, checkpoint: ReaderCursorCheckpoint) {
        self.pos = checkpoint.byte.get();
        self.character_cursor = checkpoint.characters;
        self.dot_syntax.set(None);
        self.dot_unread.set(None);
        self.step_cache.set(None);
    }

    #[inline]
    fn advance_ascii_to(&mut self, next: EmacsBytePos) {
        self.character_cursor
            .advance(CharLen::new(next.get().saturating_sub(self.pos)));
        self.pos = next.get();
    }

    #[inline(always)]
    fn advance_decoded_to(&mut self, next: EmacsBytePos) {
        self.character_cursor.advance(CharLen::new(1));
        self.pos = next.get();
    }

    /// Raw positioning is used only by explicitly byte-domain sources.
    #[inline]
    fn advance_raw_to(&mut self, next: EmacsBytePos) {
        self.pos = next.get();
    }

    #[inline]
    fn ascii_limit(&self, end: EmacsBytePos) -> usize {
        self.character_cursor
            .ascii_limit(EmacsBytePos::new(self.pos), end)
            .get()
    }

    /// The character at the cursor, read once per source character: always
    /// inlined, with the bounded-range end and the payload terminator out of
    /// line.
    #[inline(always)]
    fn current_code(&self) -> Option<u32> {
        if !self.character_cursor.permits_offset(CharLen::ZERO) {
            return None;
        }
        self.source_code_at(self.pos, CharLen::ZERO)
    }

    fn peek_code_at(&self, offset: usize) -> Option<u32> {
        if !self.character_cursor.permits_offset(CharLen::new(offset)) {
            return None;
        }
        let mut pos = self.pos;
        for step in 0..offset {
            pos = self.next_pos(pos).or_else(|| {
                self.initialized_terminator_next(EmacsBytePos::new(pos), CharLen::new(step))
                    .map(EmacsBytePos::get)
            })?;
        }
        self.source_code_at(pos, CharLen::new(offset))
    }

    /// The character at the cursor, consumed: GNU `READCHAR`. The same as
    /// [`Self::current_code`] followed by [`Self::bump`] when it is a
    /// character, but the common step decodes the character and checks the
    /// range once.
    #[inline(always)]
    fn take_code(&mut self) -> Option<u32> {
        if self.dot_unread.get().is_none()
            && let Some((code, next)) = self.code_and_next(self.pos)
            && self.character_cursor.take_one()
        {
            self.pos = next;
            return Some(code);
        }
        self.take_code_slow()
    }

    /// [`Self::take_code`] at a pending dot resume, the end of a bounded
    /// range or the end of the source.
    #[cold]
    #[inline(never)]
    fn take_code_slow(&mut self) -> Option<u32> {
        let code = self.current_code()?;
        self.bump();
        Some(code)
    }

    /// Step past the character at the cursor. The common step (no pending dot
    /// resume, a character inside the range) is inlined; every other case
    /// takes [`Self::bump_slow`].
    #[inline(always)]
    fn bump(&mut self) {
        if self.dot_unread.get().is_none()
            && let Some((_, next)) = self.code_and_next(self.pos)
            && self.character_cursor.take_one()
        {
            self.pos = next;
            return;
        }
        self.bump_slow();
    }

    /// [`Self::bump`] at a pending dot resume, the end of a bounded range or
    /// the end of the source.
    #[cold]
    #[inline(never)]
    fn bump_slow(&mut self) {
        if !self.character_cursor.permits_offset(CharLen::new(0)) {
            return;
        }
        if let Some(pending) = self.dot_unread.take()
            && pending.period.get() == self.pos
        {
            self.restore_cursor(pending.after_period);
            return;
        }
        let next = self.next_pos(self.pos).or_else(|| {
            self.initialized_terminator_next(EmacsBytePos::new(self.pos), CharLen::ZERO)
                .map(EmacsBytePos::get)
        });
        match next {
            Some(next) => self.advance_decoded_to(EmacsBytePos::new(next)),
            None => match self.character_cursor {
                SourceCharacterCursor::Bytes => self.advance_raw_to(EmacsBytePos::new(self.limit)),
                SourceCharacterCursor::PhysicalEnd { .. }
                | SourceCharacterCursor::CharacterEnd { .. } => {}
            },
        }
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

    fn signal_error(
        &self,
        symbol: impl crate::emacs_core::error::IntoConditionSym,
        data: Vec<Value>,
        message: &str,
    ) -> ReadError {
        ReadError {
            position: self.pos,
            message: message.to_string(),
            kind: ReadErrorKind::Signal,
            signal_symbol: Some(symbol.condition_sym()),
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
        self.environment
            .value("read-circle")
            .is_none_or(|value| !value.is_nil())
    }

    fn skip_to_limit(&mut self) {
        match self.source {
            ReaderSource::Callable(source) => {
                while let Some((_, next)) = source.code_and_next(EmacsBytePos::new(self.pos)) {
                    self.advance_decoded_to(next);
                }
            }
            ReaderSource::Runtime(_) | ReaderSource::FileBytes(_) | ReaderSource::Buffer(_) => {
                self.advance_raw_to(EmacsBytePos::new(self.limit))
            }
            ReaderSource::LispString(_) => match self.character_cursor {
                SourceCharacterCursor::Bytes => self.advance_raw_to(EmacsBytePos::new(self.limit)),
                SourceCharacterCursor::PhysicalEnd { .. }
                | SourceCharacterCursor::CharacterEnd { .. } => {
                    while self.current_code().is_some() {
                        if let Some(src) = self.byte_backed_source() {
                            let end = self.ascii_limit(EmacsBytePos::new(src.len()));
                            let span = &src[self.pos..end];
                            let ascii = if span.is_ascii() {
                                span.len()
                            } else {
                                span.iter()
                                    .position(|byte| *byte >= 0x80)
                                    .unwrap_or(span.len())
                            };
                            if ascii > 0 {
                                self.advance_ascii_to(EmacsBytePos::new(self.pos + ascii));
                                continue;
                            }
                        }
                        self.bump();
                    }
                }
            },
        }
    }

    fn skip_dynamic_doc_string_in_non_file_source(&mut self) {
        while let Some(c) = self.current_code() {
            self.bump();
            if c == 0x1F {
                break;
            }
        }
    }

    /// Skip raw file bytes without passing them through character decoding.
    /// The legacy runtime-string source retains its Latin-1 envelope offsets.
    fn skip_exact_source_bytes(&mut self, len: usize) -> Result<(), ReadError> {
        match self.source {
            ReaderSource::Callable(source) => {
                for _ in 0..len {
                    let Some((_, next)) = source.code_and_next(EmacsBytePos::new(self.pos)) else {
                        return Err(self.end_of_file_error());
                    };
                    self.advance_decoded_to(next);
                }
                Ok(())
            }
            // GNU `skip_dyn_bytes` skips BYTES; with the file's own bytes as
            // the source that is what a byte count means again.
            ReaderSource::FileBytes(_) => {
                let end = self
                    .pos
                    .checked_add(len)
                    .ok_or_else(|| self.error("byte skip past end of input"))?;
                if end > self.limit {
                    return Err(self.error("byte skip past end of input"));
                }
                self.advance_raw_to(EmacsBytePos::new(end));
                Ok(())
            }
            ReaderSource::Runtime(input) => {
                let mut chars = input[self.pos..self.limit].chars();
                let mut bytes_advanced = 0usize;
                for _ in 0..len {
                    match chars.next() {
                        Some(c) => bytes_advanced += c.len_utf8(),
                        None => return Err(self.error("byte skip past end of input")),
                    }
                }
                self.advance_raw_to(EmacsBytePos::new(self.pos + bytes_advanced));
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
                while self.pos < end {
                    if self.current_code().is_none() {
                        return Err(self.end_of_file_error());
                    }
                    self.bump();
                }
                if self.pos != end {
                    return Err(self.error("byte skip is not a character boundary"));
                }
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
                self.advance_raw_to(EmacsBytePos::new(end));
                Ok(())
            }
        }
    }

    #[inline(always)]
    fn source_code_at(&self, pos: usize, offset: CharLen) -> Option<u32> {
        match self.code_and_next(pos) {
            Some((code, _)) => Some(code),
            None => self.initialized_terminator_code(pos, offset),
        }
    }

    #[cold]
    #[inline(never)]
    fn initialized_terminator_code(&self, pos: usize, offset: CharLen) -> Option<u32> {
        self.initialized_terminator_next(EmacsBytePos::new(pos), offset)
            .map(|_| 0)
    }

    /// The only modeled byte outside the payload is its initialized NUL
    /// (GNU alloc.c:1812). Byte ranges/files/buffers/callables never use it.
    /// The logical range gates admission, and equality prevents a second
    /// virtual step or any access to unspecified GNU padding beyond it.
    #[cold]
    fn initialized_terminator_next(
        &self,
        pos: EmacsBytePos,
        offset: CharLen,
    ) -> Option<EmacsBytePos> {
        match self.source {
            ReaderSource::LispString(_) => (pos.get() == self.limit
                && self.character_cursor.permits_initialized_terminator(offset))
            .then(|| pos.add_len(EmacsByteLen::new(1))),
            ReaderSource::Runtime(_)
            | ReaderSource::FileBytes(_)
            | ReaderSource::Buffer(_)
            | ReaderSource::Callable(_) => None,
        }
    }

    #[inline(always)]
    fn next_pos(&self, pos: usize) -> Option<usize> {
        self.code_and_next(pos).map(|(_, next)| next)
    }

    /// Decode the char at `pos`, returning `(code, next_pos)`, through the
    /// one-entry `step_cache`.  The read hot path peeks then advances over the
    /// same position, so caching makes it decode each char once.
    /// One character of source at `pos`.  GNU `readchar` reads a byte and
    /// takes the multibyte path only for a lead byte; the ASCII byte of a
    /// string source is answered here before the memo and the source
    /// dispatch (this ran once per character of every loaded file, and its
    /// call overhead alone was 40% of its cost).
    #[inline(always)]
    fn code_and_next(&self, pos: usize) -> Option<(u32, usize)> {
        if let Some(src) = self.byte_backed_source()
            && let Some(&byte) = src.get(pos)
            && byte < 0x80
        {
            return Some((u32::from(byte), pos + 1));
        }
        self.code_and_next_memo(pos)
    }

    /// The source's bytes up to `limit` when the source is byte-backed:
    /// [`Self::source_bytes`].
    #[inline(always)]
    fn byte_backed_source(&self) -> Option<&'a [u8]> {
        self.source_bytes
    }

    fn code_and_next_memo(&self, pos: usize) -> Option<(u32, usize)> {
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
            ReaderSource::Callable(source) => source
                .code_and_next(EmacsBytePos::new(pos))
                .map(|(code, next)| (code, next.get())),
            ReaderSource::FileBytes(input) => {
                // GNU source_file_get (lread.c): decode before tokenization,
                // including symbols and character literals. Keep positions in
                // file bytes so #@ docstring skips remain byte-exact.
                let bytes = &input[pos..self.limit];
                let width = emacs_char::bytes_by_char_head(bytes[0]);
                if bytes.len() >= width && bytes[1..width].iter().all(|b| b & 0xC0 == 0x80) {
                    let (code, _) = emacs_char::string_char_unchecked(&bytes[..width]);
                    Some((code.code(), pos + width))
                } else {
                    // A malformed/truncated sequence consumes only its lead
                    // byte; subsequent bytes must still reach the parser.
                    Some((emacs_char::byte8_to_char(bytes[0]), pos + 1))
                }
            }
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
            return Some((emacs_char::unibyte_to_char(byte), 1));
        }

        let (code, width) = emacs_char::string_char(&bytes[pos..]);
        if pos + width > self.limit {
            return None;
        }
        Some((code.code(), width))
    }

    fn buffer_code_step(&self, input: &Buffer, pos: usize) -> Option<(u32, usize)> {
        if pos >= self.limit || pos >= input.total_emacs_byte_len().get() {
            return None;
        }

        if !self.source_semantics.is_multibyte() {
            let byte = input.emacs_byte_at_pos(EmacsBytePos::new(pos))?;
            return Some((emacs_char::unibyte_to_char(byte), 1));
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
            Some((c, n)) => (c.code(), n),
            None => {
                let mut tmp = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
                for (idx, slot) in tmp[..available].iter_mut().enumerate() {
                    *slot = input.emacs_byte_at_pos(range_start.add_len(EmacsByteLen::new(idx)))?;
                }
                let (c, n) = emacs_char::string_char(&tmp[..available]);
                (c.code(), n)
            }
        };
        if pos + width > self.limit {
            return None;
        }
        Some((code, width))
    }

    /// Validate grammar-produced cursor spans before any physical slicing.
    /// A consumed logical terminator is explicit, never a clipped payload.
    fn source_span(&self, range: EmacsByteRange) -> ReaderSourceSpan {
        assert!(range.start() <= range.end(), "invalid reader source span");
        if range.end().get() <= self.limit {
            return ReaderSourceSpan::Payload(range);
        }
        assert!(
            matches!(self.source, ReaderSource::LispString(_))
                && !matches!(self.character_cursor, SourceCharacterCursor::Bytes)
                && range.start().get() <= self.limit
                && self.limit.checked_add(1) == Some(range.end().get())
                && range.end().get() <= self.pos,
            "reader source span exceeds initialized data"
        );
        ReaderSourceSpan::InitializedTerminator(EmacsByteRange::new(
            range.start(),
            EmacsBytePos::new(self.limit),
        ))
    }

    /// GNU invalid_syntax_with_buffer ends at the first C NUL (lread.c:3713-3716,
    /// 657-659). Numeric source extraction separately preserves that byte.
    fn source_syntax_string(&self, start: usize, end: usize) -> String {
        let mut message = self.source_slice_string(start, end);
        if let Some(nul) = message.find('\0') {
            message.truncate(nul);
        }
        message
    }

    fn source_slice_string(&self, start: usize, end: usize) -> String {
        let range = EmacsByteRange::new(EmacsBytePos::new(start), EmacsBytePos::new(end));
        let range = match self.source_span(range) {
            ReaderSourceSpan::Payload(range) => range,
            ReaderSourceSpan::InitializedTerminator(payload) => {
                let ReaderSource::LispString(input) = self.source else {
                    unreachable!("validated logical-string source span");
                };
                let mut text = crate::emacs_core::emacs_char::to_utf8_lossy(
                    &input.as_bytes()[payload.start().get()..payload.end().get()],
                );
                text.push('\0');
                return text;
            }
        };
        let start = range.start().get();
        let end = range.end().get();
        match self.source {
            ReaderSource::Callable(source) => {
                source.slice(EmacsBytePos::new(start), EmacsBytePos::new(end))
            }
            ReaderSource::Runtime(input) => input[start..end].to_string(),
            // Only ASCII numeric sub-parsers read this slice.
            ReaderSource::FileBytes(input) => {
                String::from_utf8_lossy(&input[start..end]).into_owned()
            }
            ReaderSource::LispString(input) => {
                // Issue #131: `source_slice_string` only feeds ASCII numeric
                // sub-parsers (hex/radix-digit escapes), so a lossy UTF-8
                // rendering is byte-exact here and avoids the storage-string
                // sentinel scheme.
                //
                // Read those bytes out of the source directly.  Going through
                // `LispString::slice` built a whole substring first, and that
                // is not free: it converts both ends from byte to character
                // positions, and each conversion counts lead bytes across the
                // entire prefix of the source string.  So extracting the two
                // or three ASCII digits of `#x41` cost two scans of everything
                // read so far, plus a copy of the digits and a slice of the
                // text-property table that nothing here looks at.
                let bytes = input.as_bytes();
                assert!(
                    end <= bytes.len(),
                    "reader slice end {end} exceeds source length {}",
                    bytes.len()
                );
                crate::emacs_core::emacs_char::to_utf8_lossy(&bytes[start..end])
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

/// GNU read_char_escape (lread.c:2604-2864) for an ASCII character after a
/// backslash, when the escape reads nothing further: the named control
/// escapes `\a \b \d \e \f \n \r \t \v` (lread.c:2624-2632), and every
/// character without escape syntax, which stands for itself
/// (`default: chr = c;`, lread.c:2832-2834), such as `\\` and `\"`. None
/// for the escapes that read more source or signal: the modifier prefixes,
/// octal, hex, Unicode and named-character escapes, and an escaped newline.
const fn ascii_self_contained_escape(esc: u8) -> Option<u8> {
    match esc {
        b'\n'
        | b'M'
        | b'S'
        | b'H'
        | b'A'
        | b's'
        | b'C'
        | b'^'
        | b'0'..=b'7'
        | b'x'
        | b'u'
        | b'U'
        | b'N' => None,
        b'a' => Some(0x07),
        b'b' => Some(0x08),
        b'd' => Some(0x7F),
        b'e' => Some(0x1B),
        b'f' => Some(0x0C),
        b'n' => Some(b'\n'),
        b'r' => Some(b'\r'),
        b't' => Some(b'\t'),
        b'v' => Some(0x0B),
        other => Some(other),
    }
}

/// [`ascii_self_contained_escape`] for every ASCII character, so answering
/// an escape is one indexed load.
const ASCII_SELF_CONTAINED_ESCAPES: [Option<u8>; 128] = {
    let mut table = [None; 128];
    let mut esc = 0;
    while esc < table.len() {
        table[esc] = ascii_self_contained_escape(esc as u8);
        esc += 1;
    }
    table
};

/// [`ascii_self_contained_escape`] for any character code: an escaped
/// non-ASCII character stands for itself.
#[inline(always)]
fn self_contained_escape_code(esc: u32) -> Option<u32> {
    match ASCII_SELF_CONTAINED_ESCAPES.get(esc as usize) {
        Some(entry) => entry.map(u32::from),
        None => Some(esc),
    }
}

fn is_char_literal_delimiter_code(code: u32) -> bool {
    is_reader_whitespace_code(code)
        || matches!(
            code,
            34 | 39 | 59 | 40 | 41 | 91 | 93 | 35 | 63 | 96 | 44 | 46
        )
}

fn is_symbol_delimiter_code(code: u32) -> bool {
    is_reader_whitespace_code(code)
        || matches!(code, 40 | 41 | 91 | 93 | 39 | 96 | 44 | 34 | 59 | 35)
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
    // GNU c_isspace includes vertical tab; Rust's ASCII predicate does not.
    // This predicate is used only by Unicode character-name normalization.
    matches!(code, 0x09..=0x0D | 0x20)
}

fn is_ascii_digit_code(code: u32) -> bool {
    code <= 0x7F && (code as u8).is_ascii_digit()
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

/// Unicode escape spelling and width travel together, so callers cannot
/// request eight digits while identifying the escape as `\u`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display)]
enum UnicodeEscapeKind {
    #[strum(serialize = "u")]
    Short,
    #[strum(serialize = "U")]
    Long,
}

impl UnicodeEscapeKind {
    const fn digit_count(self) -> usize {
        match self {
            Self::Short => 4,
            Self::Long => 8,
        }
    }
}

/// GNU read_char_escape failures. Variant selection determines the Lisp
/// condition; Display contains only the exact GNU error-data message.
/// Error payloads own scalar/text data and contain no mutator-owned Lisp state.
#[derive(Debug, Clone, thiserror::Error)]
enum CharacterEscapeError {
    #[error("Invalid modifier in string")]
    InvalidStringModifier,
    #[error("Hex character out of range: \\x{value:x}...")]
    HexOutOfRange { value: u32 },
    #[error("Invalid escape char syntax: \\x not followed by hex digit")]
    MissingHexDigit,
    #[error("Malformed Unicode escape: \\{kind}{value:x}")]
    MalformedUnicode { kind: UnicodeEscapeKind, value: u32 },
    #[error("Non-hex character used for Unicode escape: {character} ({code})")]
    NonHexUnicode { character: String, code: u32 },
    #[error("Non-Unicode character: 0x{value:x}")]
    NonUnicode { value: u32 },
    #[error("Expected opening brace after \\N")]
    MissingNameOpeningBrace,
    #[error("Invalid character U+{code:04X} in character name")]
    InvalidNameCharacter { code: u32 },
    #[error("Character name too long")]
    NameTooLong,
    #[error("Empty character name")]
    EmptyName,
    #[error("\\N{{{name}}}")]
    UnknownName { name: String },
    #[error("Invalid escape char syntax: \\{prefix} not followed by -")]
    ModifierWithoutHyphen { prefix: char },
    #[error("Invalid escape char syntax: \\<newline>")]
    EscapedNewline,
}

bitflags::bitflags! {
    /// GNU character modifier flags. This scalar has no mutator-owned state.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct CharacterModifiers: u32 {
        const ALT = 1 << 22;
        const SUPER = 1 << 23;
        const HYPER = 1 << 24;
        const SHIFT = 1 << 25;
        const CONTROL = 1 << 26;
        const META = 1 << 27;
    }
}

/// A character after escape decoding, retaining GNU modifier bits and the
/// BYTE8 domain. String projection cannot accidentally accept an undecoded
/// escape prefix. It contains no Lisp state and may cross mutator threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use = "a decoded character must be projected into a literal's value domain"]
struct DecodedCharacter(u32);

static_assertions::assert_impl_all!(DecodedCharacter: Send, Sync);
static_assertions::assert_impl_all!(CharacterModifiers: Send, Sync);

impl DecodedCharacter {
    fn parts(self) -> (u32, CharacterModifiers) {
        (
            self.0 & CHAR_CODE_MASK,
            CharacterModifiers::from_bits_retain(self.0 & !CHAR_CODE_MASK),
        )
    }
}

impl From<DecodedCharacter> for u32 {
    fn from(value: DecodedCharacter) -> Self {
        value.0
    }
}

fn character_name_to_code(name: &str) -> Option<u32> {
    if let Some(hex) = name.strip_prefix("U+") {
        return parse_unicode_codepoint(hex);
    }

    lookup_algorithmic_unicode_name(name)
        .or_else(|| lookup_primary_unicode_name(name))
        .or_else(|| lookup_gnu_cached_name_alias(name))
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

fn strip_ascii_case_prefix<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    name.get(..prefix.len())?
        .eq_ignore_ascii_case(prefix)
        .then(|| &name[prefix.len()..])
}

/// GNU char-from-name (mule-cmds.el:3188-3207) validates algorithmic
/// names against the name property, including its exact hex spelling.
fn lookup_algorithmic_unicode_name(name: &str) -> Option<u32> {
    if let Some(hex) = strip_ascii_case_prefix(name, "CJK IDEOGRAPH-") {
        let code = parse_unicode_codepoint(hex)?;
        let ch = char::from_u32(code)?;
        let primary = unicode_names2::name(ch)?.to_string();
        return (primary == format!("CJK UNIFIED IDEOGRAPH-{code:X}")
            && name.eq_ignore_ascii_case(&format!("CJK IDEOGRAPH-{code:X}")))
        .then_some(code);
    }
    if let Some(hex) = strip_ascii_case_prefix(name, "TANGUT IDEOGRAPH-") {
        let code = parse_unicode_codepoint(hex)?;
        // GNU UnicodeData.txt:30502-30503 and 31743-31744 (Unicode 17).
        return (((0x17000..=0x187FF).contains(&code) || (0x18D00..=0x18D1E).contains(&code))
            && name.eq_ignore_ascii_case(&format!("TANGUT IDEOGRAPH-{code:X}")))
        .then_some(code);
    }
    None
}

fn lookup_primary_unicode_name(name: &str) -> Option<u32> {
    // GNU's name property spells these "CJK IDEOGRAPH-", not Unicode's
    // "CJK UNIFIED IDEOGRAPH-". Its char-from-name rejects the latter.
    if strip_ascii_case_prefix(name, "CJK UNIFIED IDEOGRAPH-").is_some() {
        return None;
    }
    let ch = unicode_names2::character(name)?;
    let primary = unicode_names2::name(ch)?;
    let canonical = primary.to_string();
    if !canonical.eq_ignore_ascii_case(name) {
        return None;
    }
    let code = ch as u32;
    // The table deliberately omits algorithmically named ranges. Outside
    // its ranges char-from-name's NAME-HEX fallback must round-trip the
    // suffix through the name property (mule-cmds.el:3195-3207). In
    // particular, TANGUT COMPONENT-001 is not a character named U+0001.
    if !gnu_ucs_names_cached_code(code) {
        let (_, suffix) = canonical.rsplit_once('-')?;
        if parse_unicode_codepoint(suffix)? != code {
            return None;
        }
    }
    Some(code)
}

/// The ranges selected by GNU `ucs-names`, mule-cmds.el:3096-3127.
fn gnu_ucs_names_cached_code(code: u32) -> bool {
    matches!(code,
        0x0000..=0x33FF | 0x4DC0..=0x4DFF | 0xA000..=0xD7FF
        | 0xFB00..=0x143FA | 0x14400..=0x14646 | 0x16100..=0x16139
        | 0x16800..=0x16F9F | 0x16FE0..=0x16FF6 | 0x1AFF0..=0x1B122
        | 0x1B132 | 0x1B150..=0x1B16F | 0x1B170..=0x1B2FF
        | 0x1BC00..=0x1BCAF | 0x1CC00..=0x1FFFF | 0xE0000..=0xE01FF)
}

fn lookup_gnu_cached_name_alias(name: &str) -> Option<u32> {
    old_unicode_names::lookup(name)
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
/// Which representation a string literal's characters have forced so far.
///
/// GNU reads a literal into one buffer of internal-encoding bytes and two
/// flags, then converts the whole buffer back with `str_as_unibyte` if it
/// turned out to hold only raw bytes (`src/lread.c:3050-3168`).  That final
/// pass is measurable here: loading one large `.elc` eight times spent 3.5M
/// instructions inside `str_as_unibyte` and 2.2M more in the `memchr` it
/// scans with, because a compiled file is full of raw-byte literals.
///
/// So the buffer is kept in the representation the literal has earned
/// instead.  While no real multibyte character has appeared, raw bytes are
/// stored raw and the buffer already IS the unibyte string.  The first
/// multibyte character promotes what has accumulated -- once, over a prefix,
/// and only for literals that end up multibyte anyway -- after which every
/// character is appended in internal encoding, as GNU does throughout.
/// Either way the closing quote hands the buffer over untouched.
#[derive(Default)]
struct StringLiteralFlags {
    /// The buffer holds internal-encoding bytes rather than raw bytes, and
    /// the literal will be a multibyte string.  This is GNU's
    /// `force_multibyte`; its `force_singlebyte` needs no counterpart,
    /// because a raw byte simply stays a raw byte.
    multibyte: bool,
}

impl StringLiteralFlags {
    /// Close the literal, in whichever representation it earned.
    fn finish(self, buf: Vec<u8>) -> crate::heap_types::LispString {
        if self.multibyte {
            maybe_recombine_latin1_emacs(buf)
        } else {
            crate::heap_types::LispString::from_unibyte(buf)
        }
    }

    /// Promote the bytes accumulated so far to internal encoding, so that a
    /// character that cannot be a raw byte can be appended after them.
    fn promote_to_multibyte(&mut self, buf: &mut Vec<u8>) {
        if !self.multibyte {
            *buf = emacs_char::str_to_multibyte(buf);
            self.multibyte = true;
        }
    }
}

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
#[path = "tests/value_reader_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/lazy_doc_test.rs"]
mod lazy_doc_tests;

#[cfg(test)]
#[path = "tests/declared_shorthand_counts.rs"]
mod declared_shorthand_count_tests;

#[cfg(test)]
#[path = "tests/logical_source_cursor.rs"]
mod logical_source_cursor_tests;

#[cfg(test)]
#[path = "tests/original_string_unread.rs"]
mod original_string_unread_tests;
