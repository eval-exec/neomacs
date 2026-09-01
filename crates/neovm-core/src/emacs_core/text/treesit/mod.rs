use std::collections::{BTreeMap, HashMap};
use std::mem::MaybeUninit;
use std::ops::Range;
use std::ptr::NonNull;

use libloading::Library;
use tree_sitter::{InputEdit, Language, Parser, Point, Query, QueryError, Tree, ffi};

use super::intern::SymId;
use super::value::Value;
use crate::buffer::{Buffer, BufferId, EmacsBytePos, EmacsByteRange};
use crate::heap_types::LispString;

pub(crate) const TREESIT_PARSER_TAG: &str = "treesit-parser";
pub(crate) const TREESIT_NODE_TAG: &str = "treesit-node";
pub(crate) const TREESIT_COMPILED_QUERY_TAG: &str = "treesit-compiled-query";

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) const PARSER_SLOT_TYPE: usize = 0;
pub(crate) const PARSER_SLOT_ID: usize = 1;
pub(crate) const PARSER_SLOT_LANGUAGE: usize = 2;
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) const PARSER_SLOT_BUFFER: usize = 3;
pub(crate) const PARSER_SLOT_TAG: usize = 4;
pub(crate) const PARSER_SLOT_EMBED_LEVEL: usize = 5;
pub(crate) const PARSER_SLOT_NOTIFIERS: usize = 6;
pub(crate) const PARSER_SLOT_INCLUDED_RANGES: usize = 7;

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) const NODE_SLOT_TYPE: usize = 0;
pub(crate) const NODE_SLOT_ID: usize = 1;
pub(crate) const NODE_SLOT_PARSER: usize = 2;

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) const QUERY_SLOT_TYPE: usize = 0;
pub(crate) const QUERY_SLOT_ID: usize = 1;
pub(crate) const QUERY_SLOT_LANGUAGE: usize = 2;
pub(crate) const QUERY_SLOT_SOURCE: usize = 3;

pub(crate) struct LoadedLanguage {
    pub(crate) language: Language,
    pub(crate) filename: Option<String>,
    pub(crate) _library: Option<Library>,
}

#[derive(Clone, Copy)]
pub(crate) struct LineColCache {
    pub(crate) line: i64,
    pub(crate) col: i64,
    pub(crate) bytepos: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SourceByteRange {
    start: usize,
    end: usize,
}

/// GNU `TREESIT_TS_POINT_1_0` (`src/treesit.c`), deliberately distinct from
/// the real first-byte coordinate `(0, 0)`.
const TREESIT_TS_POINT_1_0: Point = Point { row: 1, column: 0 };

/// Version of a buffer's *text*, as a parser sees it.
///
/// Tree-sitter does not consume text properties, so GNU's `chars_modiff`
/// rather than the broader buffer modification tick is the correct content
/// version.
///
/// The buffer restriction is deliberately **not** part of this identity.  A
/// parser's own view of the buffer is tracked separately, on its tree
/// ([`ParsedTree::visible`]), exactly as GNU keeps `visible_beg`/`visible_end`
/// on the parser (`src/treesit.h:98-118`); narrowing therefore moves a tree's
/// window instead of invalidating the tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ParserInputRevision {
    chars_modified_tick: i64,
}

impl ParserInputRevision {
    pub(crate) fn for_buffer(buffer: &Buffer) -> Self {
        Self {
            chars_modified_tick: buffer.chars_modified_tick(),
        }
    }
}

/// A parse tree together with the buffer byte window its offsets are relative
/// to.
///
/// GNU keeps that window on the parser as `visible_beg`/`visible_end`
/// (`src/treesit.h:98-118`) and never lets it drift: every recorded change is
/// clipped into it (`treesit_record_change_1`, `src/treesit.c:1503-1560`) and
/// every reparse first reconciles it with the buffer's restriction by editing
/// the tree (`treesit_sync_visible_region`, `src/treesit.c:1626-1740`).
///
/// Pairing the window with the tree makes a tree whose frame is unknown
/// unrepresentable, which is what lets `treesit_ensure_parsed`
/// (`src/treesit.c:1901-1949`) hand the previous tree back to tree-sitter on
/// every reparse -- and therefore what keeps the whole-buffer affected range
/// (`treesit_get_affected_ranges`, `src/treesit.c:1857-1880`) to the one case
/// GNU uses it for: a parser that has never parsed.
pub(crate) struct ParsedTree {
    tree: Tree,
    visible: EmacsByteRange,
}

impl ParsedTree {
    pub(crate) const fn new(tree: Tree, visible: EmacsByteRange) -> Self {
        Self { tree, visible }
    }

    pub(crate) const fn tree(&self) -> &Tree {
        &self.tree
    }

    pub(crate) const fn visible(&self) -> EmacsByteRange {
        self.visible
    }

    fn apply_edit(&mut self, edit: &InputEdit, new_visible: EmacsByteRange) {
        self.tree.edit(edit);
        self.visible = new_visible;
    }

    /// Move this tree's window onto BUFFER's current restriction, GNU
    /// `treesit_sync_visible_region` (`src/treesit.c:1626-1740`).
    ///
    /// Returns whether the window moved, which is GNU's reason to set
    /// `need_reparse` (`src/treesit.c:1654-1656`).
    fn sync_visible_region(&mut self, buffer: &Buffer, tracking: ParserPointTracking) -> bool {
        let accessible = buffer.accessible_emacs_byte_range();
        if self.visible == accessible {
            return false;
        }
        let (begv, zv) = (accessible.start(), accessible.end());

        // 1. Make sure visible.start <= BEGV: tree-sitter sees an insertion at
        //    the beginning.
        if self.visible.start() > begv {
            let new_end = self.visible.start().get() - begv.get();
            let new_end_position = parser_point_at(buffer, begv, self.visible.start(), tracking);
            self.tree.edit(&InputEdit {
                start_byte: 0,
                old_end_byte: 0,
                new_end_byte: new_end,
                start_position: TREESIT_TS_POINT_1_0,
                old_end_position: TREESIT_TS_POINT_1_0,
                new_end_position,
            });
            self.visible = EmacsByteRange::new(begv, self.visible.end());
        }

        // 2. Make sure visible.end = ZV: an insertion or a deletion at the end.
        let visible_start = self.visible.start();
        if self.visible.end() < zv {
            let old_end = self.visible.end().get() - visible_start.get();
            let start_position =
                parser_point_at(buffer, visible_start, self.visible.end(), tracking);
            let new_end_position = parser_point_at(buffer, visible_start, zv, tracking);
            self.tree.edit(&InputEdit {
                start_byte: old_end,
                old_end_byte: old_end,
                new_end_byte: zv.get() - visible_start.get(),
                start_position,
                old_end_position: start_position,
                new_end_position,
            });
            self.visible = EmacsByteRange::new(visible_start, zv);
        } else if self.visible.end() > zv {
            let start = zv.get().saturating_sub(visible_start.get());
            let start_position = parser_point_at(buffer, visible_start, zv, tracking);
            let old_end_position =
                parser_point_at(buffer, visible_start, self.visible.end(), tracking);
            self.tree.edit(&InputEdit {
                start_byte: start,
                old_end_byte: self.visible.end().get() - visible_start.get(),
                new_end_byte: start,
                start_position,
                old_end_position,
                new_end_position: start_position,
            });
            self.visible = EmacsByteRange::new(visible_start, zv);
        }

        // 3. Make sure visible.start = BEGV: a deletion at the beginning.
        if self.visible.start() < begv {
            let old_end = begv.get() - self.visible.start().get();
            let old_end_position = parser_point_at(buffer, self.visible.start(), begv, tracking);
            self.tree.edit(&InputEdit {
                start_byte: 0,
                old_end_byte: old_end,
                new_end_byte: 0,
                start_position: TREESIT_TS_POINT_1_0,
                old_end_position,
                new_end_position: TREESIT_TS_POINT_1_0,
            });
            self.visible = EmacsByteRange::new(begv, self.visible.end());
        }

        debug_assert_eq!(self.visible, accessible);
        true
    }
}

/// Whether a parser's tree can be reused, incrementally reparsed, or must be
/// rebuilt.  Keeping this separate from `generation` prevents a buffer edit
/// revision from being confused with the generation used to invalidate Lisp
/// node handles.
///
/// GNU carries a single `need_reparse` bit (`src/treesit.h`) because every
/// text change reaches `treesit_record_change`.  `Clean`/`ReparsePending` are
/// that bit; the extra `FullReparseRequired` state exists only for a text
/// change that never reached [`TreeSitterManager::finish_buffer_edit`], where
/// the tree's window is no longer known to describe the buffer and reusing it
/// would corrupt the parse rather than merely widen a range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ParserFreshness {
    Unparsed,
    Clean(ParserInputRevision),
    /// The tree is in sync with the buffer (every change since the last parse
    /// was applied to it) and a reparse is due -- GNU `need_reparse`.
    ReparsePending(ParserInputRevision),
    FullReparseRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ParserReparseKind {
    Incremental,
    Full,
}

impl ParserFreshness {
    pub(crate) fn reparse_kind(
        &mut self,
        current: ParserInputRevision,
    ) -> Option<ParserReparseKind> {
        match *self {
            Self::Clean(parsed) if parsed == current => None,
            Self::ReparsePending(edited) if edited == current => {
                Some(ParserReparseKind::Incremental)
            }
            Self::Unparsed => Some(ParserReparseKind::Full),
            Self::Clean(_) | Self::ReparsePending(_) | Self::FullReparseRequired => {
                *self = Self::FullReparseRequired;
                Some(ParserReparseKind::Full)
            }
        }
    }

    /// GNU sets `need_reparse` when a parser's window had to move onto a new
    /// buffer restriction (`src/treesit.c:1654-1656`).  The tree itself was
    /// moved with it, so this is a pending reparse, never an invalidation.
    fn note_window_moved(&mut self, current: ParserInputRevision) {
        if let Self::Clean(revision) = *self
            && revision == current
        {
            *self = Self::ReparsePending(revision);
        }
    }

    fn accepts_incremental_edit(self, old: ParserInputRevision) -> bool {
        matches!(
            self,
            Self::Clean(revision) | Self::ReparsePending(revision)
                if revision == old
        )
    }
}

impl SourceByteRange {
    pub(crate) const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub(crate) const fn start(self) -> usize {
        self.start
    }

    pub(crate) const fn end(self) -> usize {
        self.end
    }
}

pub(crate) struct ParserEntry {
    pub(crate) value: Value,
    pub(crate) orig_buffer_id: BufferId,
    pub(crate) root_buffer_id: BufferId,
    pub(crate) language: SymId,
    pub(crate) tag: Value,
    pub(crate) parser: Parser,
    pub(crate) tree: Option<ParsedTree>,
    pub(crate) last_source: Option<LispString>,
    pub(crate) freshness: ParserFreshness,
    pub(crate) generation: u64,
    pub(crate) need_to_gc_buffer: bool,
    pub(crate) deleted: bool,
    pub(crate) tracking_linecol: bool,
    pub(crate) last_changed_ranges: Vec<SourceByteRange>,
}

pub(crate) struct NodeEntry {
    pub(crate) parser_id: u64,
    pub(crate) raw: tree_sitter::ffi::TSNode,
    pub(crate) generation: u64,
}

pub(crate) struct QueryEntry {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) language: SymId,
    pub(crate) compiled: Option<EmacsQuery>,
}

/// One argument in a tree-sitter query predicate.
///
/// GNU Emacs deliberately interprets these itself instead of adopting the
/// Rust binding's predicate rules.  Keeping the raw distinction lets the
/// builtin layer implement GNU's accepted argument orders for `eq?`,
/// `match?`, and `pred?`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum QueryPredicateArg {
    Capture(u32),
    String(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QueryPredicate {
    pub(crate) steps: Vec<QueryPredicateArg>,
}

#[derive(Clone, Copy)]
pub(crate) struct RawQueryCapture {
    pub(crate) index: u32,
    pub(crate) node: ffi::TSNode,
}

#[derive(Clone)]
pub(crate) struct RawQueryMatch {
    pub(crate) pattern_index: usize,
    pub(crate) captures: Vec<RawQueryCapture>,
}

/// An owning wrapper around `TSQuery` which intentionally bypasses the Rust
/// binding's built-in predicate validation and filtering.
///
/// GNU Emacs accepts predicate forms that the Rust wrapper rejects (notably
/// regex-first `#match?`) and evaluates them with Emacs Lisp semantics.  The
/// raw C query API preserves those predicate steps for the builtin layer.
pub(crate) struct EmacsQuery {
    raw: NonNull<ffi::TSQuery>,
    capture_names: Vec<String>,
    predicates: Vec<Vec<QueryPredicate>>,
}

impl EmacsQuery {
    pub(crate) fn new(language: &Language, source: &str) -> Result<Self, QueryError> {
        let raw = NonNull::new(Query::new_raw(language, source)?)
            .expect("tree-sitter returned a null query without an error");
        let mut query = Self {
            raw,
            capture_names: Vec::new(),
            predicates: Vec::new(),
        };
        query.capture_names = query.read_capture_names();
        query.predicates = query.read_predicates();
        Ok(query)
    }

    pub(crate) fn capture_names(&self) -> &[String] {
        &self.capture_names
    }

    pub(crate) fn predicates(&self) -> &[Vec<QueryPredicate>] {
        &self.predicates
    }

    pub(crate) fn matches(
        &self,
        node: ffi::TSNode,
        byte_range: Option<Range<usize>>,
    ) -> Result<Vec<RawQueryMatch>, &'static str> {
        let cursor = RawQueryCursor::new(self.raw, node, byte_range)?;
        let mut matches = Vec::new();
        loop {
            let mut raw_match = MaybeUninit::<ffi::TSQueryMatch>::uninit();
            if !unsafe {
                ffi::ts_query_cursor_next_match(cursor.raw.as_ptr(), raw_match.as_mut_ptr())
            } {
                break;
            }
            // SAFETY: tree-sitter returned true and initialized `raw_match`.
            matches.push(unsafe { copy_raw_query_match(raw_match.assume_init_ref()) });
        }
        Ok(matches)
    }

    fn read_capture_names(&self) -> Vec<String> {
        let count = unsafe { ffi::ts_query_capture_count(self.raw.as_ptr()) };
        (0..count)
            .map(|index| unsafe { query_text(self.raw, index, ffi::ts_query_capture_name_for_id) })
            .collect()
    }

    fn read_predicates(&self) -> Vec<Vec<QueryPredicate>> {
        let pattern_count = unsafe { ffi::ts_query_pattern_count(self.raw.as_ptr()) };
        (0..pattern_count)
            .map(|pattern_index| {
                let mut step_count = 0u32;
                let raw_steps = unsafe {
                    ffi::ts_query_predicates_for_pattern(
                        self.raw.as_ptr(),
                        pattern_index,
                        &mut step_count,
                    )
                };
                let steps = if step_count == 0 {
                    &[][..]
                } else {
                    // SAFETY: the query owns this array, which remains valid
                    // for the duration of this immutable borrow.
                    unsafe { std::slice::from_raw_parts(raw_steps, step_count as usize) }
                };
                steps
                    .split(|step| step.type_ == ffi::TSQueryPredicateStepTypeDone)
                    .filter(|predicate| !predicate.is_empty())
                    .map(|predicate| QueryPredicate {
                        steps: predicate
                            .iter()
                            .filter_map(|step| match step.type_ {
                                ffi::TSQueryPredicateStepTypeCapture => {
                                    Some(QueryPredicateArg::Capture(step.value_id))
                                }
                                ffi::TSQueryPredicateStepTypeString => {
                                    Some(QueryPredicateArg::String(unsafe {
                                        query_text(
                                            self.raw,
                                            step.value_id,
                                            ffi::ts_query_string_value_for_id,
                                        )
                                    }))
                                }
                                _ => None,
                            })
                            .collect(),
                    })
                    .collect()
            })
            .collect()
    }
}

impl Drop for EmacsQuery {
    fn drop(&mut self) {
        unsafe { ffi::ts_query_delete(self.raw.as_ptr()) }
    }
}

// The underlying tree-sitter query is immutable after construction, matching
// the guarantees of the crate's high-level `Query` wrapper.
unsafe impl Send for EmacsQuery {}
unsafe impl Sync for EmacsQuery {}

struct RawQueryCursor {
    raw: NonNull<ffi::TSQueryCursor>,
}

impl RawQueryCursor {
    fn new(
        query: NonNull<ffi::TSQuery>,
        node: ffi::TSNode,
        byte_range: Option<Range<usize>>,
    ) -> Result<Self, &'static str> {
        let raw = NonNull::new(unsafe { ffi::ts_query_cursor_new() })
            .expect("tree-sitter failed to allocate a query cursor");
        let cursor = Self { raw };
        if let Some(range) = byte_range {
            let start = u32::try_from(range.start)
                .map_err(|_| "tree-sitter query range exceeds 32-bit byte offsets")?;
            let end = u32::try_from(range.end)
                .map_err(|_| "tree-sitter query range exceeds 32-bit byte offsets")?;
            if !unsafe { ffi::ts_query_cursor_set_byte_range(cursor.raw.as_ptr(), start, end) } {
                return Err("invalid tree-sitter query byte range");
            }
        }
        unsafe { ffi::ts_query_cursor_exec(cursor.raw.as_ptr(), query.as_ptr(), node) };
        Ok(cursor)
    }
}

impl Drop for RawQueryCursor {
    fn drop(&mut self) {
        unsafe { ffi::ts_query_cursor_delete(self.raw.as_ptr()) }
    }
}

unsafe fn query_text(
    query: NonNull<ffi::TSQuery>,
    index: u32,
    getter: unsafe extern "C" fn(*const ffi::TSQuery, u32, *mut u32) -> *const std::ffi::c_char,
) -> String {
    let mut length = 0u32;
    let ptr = unsafe { getter(query.as_ptr(), index, &mut length) }.cast::<u8>();
    if length == 0 {
        return String::new();
    }
    // SAFETY: tree-sitter returns `length` bytes owned by the live query.
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, length as usize) })
        .into_owned()
}

unsafe fn copy_raw_query_match(raw: &ffi::TSQueryMatch) -> RawQueryMatch {
    let captures = if raw.capture_count == 0 {
        Vec::new()
    } else {
        // SAFETY: this is copied before advancing the query cursor, while the
        // match's capture array is still valid.
        unsafe { std::slice::from_raw_parts(raw.captures, raw.capture_count as usize) }
            .iter()
            .map(|capture| RawQueryCapture {
                index: capture.index,
                node: capture.node,
            })
            .collect()
    };
    RawQueryMatch {
        pattern_index: raw.pattern_index as usize,
        captures,
    }
}

/// A buffer change in progress, recorded in **absolute** buffer bytes.
///
/// GNU's `treesit_record_change_1` (`src/treesit.c:1503-1560`) receives
/// absolute byte positions and clips them into each parser's own window, so
/// the buffer's restriction at the moment of the edit never enters the
/// arithmetic.  That is what makes an edit inside `comment-with-narrowing`
/// (`lisp/newcomment.el:1094-1112`) an ordinary incremental edit.
struct PendingBufferEdit {
    content_revision: ParserInputRevision,
    point_tracking: ParserPointTracking,
    start_byte: EmacsBytePos,
    old_end_byte: EmacsBytePos,
    /// Pre-edit tree coordinates of the clipped endpoints, one entry per
    /// parser that holds a tree, because each parser clips into its own
    /// window.  GNU derives the same points from per-parser
    /// `visi_beg_linecol` (`src/treesit.c:1543-1560`); measuring them here,
    /// while the old text is still present, is the same value without the
    /// incremental line/column bookkeeping.
    points: Vec<PendingEditPoints>,
}

struct PendingEditPoints {
    parser_id: u64,
    start: Point,
    old_end: Point,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParserPointTracking {
    BytesOnly,
    LineAndColumn,
}

/// The edit as one parser's tree sees it: tree-relative offsets plus the
/// window the tree covers afterwards.  GNU computes both in the same block
/// (`src/treesit.c:1420-1457`).
struct TreeRelativeEdit {
    edit: InputEdit,
    new_visible: EmacsByteRange,
}

impl PendingBufferEdit {
    fn for_buffer(
        buffer: &Buffer,
        byte_range: EmacsByteRange,
        point_tracking: ParserPointTracking,
        windows: impl Iterator<Item = (u64, EmacsByteRange)>,
    ) -> Self {
        let points = if point_tracking == ParserPointTracking::LineAndColumn {
            windows
                .map(|(parser_id, visible)| PendingEditPoints {
                    parser_id,
                    start: parser_point_at(
                        buffer,
                        visible.start(),
                        clamp_into(byte_range.start(), visible),
                        point_tracking,
                    ),
                    old_end: parser_point_at(
                        buffer,
                        visible.start(),
                        clamp_into(byte_range.end(), visible),
                        point_tracking,
                    ),
                })
                .collect()
        } else {
            Vec::new()
        };
        Self {
            content_revision: ParserInputRevision::for_buffer(buffer),
            point_tracking,
            start_byte: byte_range.start(),
            old_end_byte: byte_range.end(),
            points,
        }
    }

    /// Express this change in one parser's tree coordinates, GNU
    /// `treesit_record_change_1` (`src/treesit.c:1420-1457`).
    fn for_window(
        &self,
        buffer: &Buffer,
        parser_id: u64,
        visible: EmacsByteRange,
        new_end: EmacsBytePos,
    ) -> TreeRelativeEdit {
        let base = visible.start().get();
        let start_offset = clamp_into(self.start_byte, visible).get() - base;
        let old_end_offset = clamp_into(self.old_end_byte, visible).get() - base;
        // GNU deliberately leaves NEW_END unclipped above the window: inserting
        // into a narrowed region grows that region (Bug#61369).
        let new_end_offset = new_end.max(visible.start()).get() - base;

        // Move the window with the text that shifted it (GNU
        // `visi_beg_delta`, `src/treesit.c:1439-1453`).
        let (old_end, new_end_abs) = (self.old_end_byte.get(), new_end.get());
        let visi_beg_delta: i64 = if old_end > new_end_abs {
            base.min(new_end_abs) as i64 - base.min(old_end) as i64
        } else if old_end < base {
            new_end_abs as i64 - old_end as i64
        } else {
            0
        };
        let new_base = (base as i64 + visi_beg_delta).max(0) as usize;
        let new_visible_end = (visible.end().get() as i64 + visi_beg_delta + new_end_offset as i64
            - old_end_offset as i64)
            .max(new_base as i64) as usize;

        let (start_position, old_end_position) = self
            .points
            .iter()
            .find(|points| points.parser_id == parser_id)
            .map_or((TREESIT_TS_POINT_1_0, TREESIT_TS_POINT_1_0), |points| {
                (points.start, points.old_end)
            });

        TreeRelativeEdit {
            edit: InputEdit {
                start_byte: start_offset,
                old_end_byte: old_end_offset,
                new_end_byte: new_end_offset,
                start_position,
                old_end_position,
                new_end_position: parser_point_at(
                    buffer,
                    EmacsBytePos::new(new_base),
                    new_end,
                    self.point_tracking,
                ),
            },
            new_visible: EmacsByteRange::from_usize(new_base, new_visible_end),
        }
    }
}

fn clamp_into(pos: EmacsBytePos, range: EmacsByteRange) -> EmacsBytePos {
    pos.max(range.start()).min(range.end())
}

#[derive(Default)]
pub(crate) struct TreeSitterManager {
    next_parser_id: u64,
    next_node_id: u64,
    next_query_id: u64,
    loaded_languages: HashMap<SymId, LoadedLanguage>,
    parsers: BTreeMap<u64, ParserEntry>,
    nodes: BTreeMap<u64, NodeEntry>,
    queries: BTreeMap<u64, QueryEntry>,
    linecol_caches: HashMap<BufferId, LineColCache>,
    pending_edits: HashMap<BufferId, PendingBufferEdit>,
}

impl TreeSitterManager {
    pub(crate) fn new() -> Self {
        Self {
            next_parser_id: 1,
            next_node_id: 1,
            next_query_id: 1,
            loaded_languages: HashMap::new(),
            parsers: BTreeMap::new(),
            nodes: BTreeMap::new(),
            queries: BTreeMap::new(),
            linecol_caches: HashMap::new(),
            pending_edits: HashMap::new(),
        }
    }

    pub(crate) fn roots(&self) -> Vec<Value> {
        self.parsers.values().map(|entry| entry.value).collect()
    }

    pub(crate) fn loaded_language(&self, key: SymId) -> Option<(Language, Option<String>)> {
        self.loaded_languages
            .get(&key)
            .map(|loaded| (loaded.language.clone(), loaded.filename.clone()))
    }

    pub(crate) fn cache_loaded_language(&mut self, key: SymId, loaded: LoadedLanguage) {
        self.loaded_languages.entry(key).or_insert(loaded);
    }

    pub(crate) fn find_reusable_parser(
        &self,
        orig_buffer_id: BufferId,
        language: SymId,
        tag: Value,
    ) -> Option<Value> {
        self.parsers
            .values()
            .find(|entry| {
                !entry.deleted
                    && entry.orig_buffer_id == orig_buffer_id
                    && entry.language == language
                    && entry.tag == tag
            })
            .map(|entry| entry.value)
    }

    #[allow(clippy::too_many_arguments)] // parser registry stores the full GNU-visible parser identity
    pub(crate) fn insert_parser(
        &mut self,
        value: Value,
        orig_buffer_id: BufferId,
        root_buffer_id: BufferId,
        language: SymId,
        tag: Value,
        parser: Parser,
        tracking_linecol: bool,
    ) -> u64 {
        let id = self.next_parser_id;
        self.next_parser_id += 1;
        self.parsers.insert(
            id,
            ParserEntry {
                value,
                orig_buffer_id,
                root_buffer_id,
                language,
                tag,
                parser,
                tree: None,
                last_source: None,
                freshness: ParserFreshness::Unparsed,
                generation: 0,
                need_to_gc_buffer: false,
                deleted: false,
                tracking_linecol,
                last_changed_ranges: Vec::new(),
            },
        );
        id
    }

    pub(crate) fn parser(&self, id: u64) -> Option<&ParserEntry> {
        self.parsers.get(&id)
    }

    pub(crate) fn parser_mut(&mut self, id: u64) -> Option<&mut ParserEntry> {
        self.parsers.get_mut(&id)
    }

    pub(crate) fn parser_values_for(
        &self,
        root_buffer_id: BufferId,
        orig_buffer_id: BufferId,
        language: Option<SymId>,
        tag_filter: ParserTagFilter,
    ) -> Vec<Value> {
        let mut items = self
            .parsers
            .iter()
            .rev()
            .filter_map(|(_, entry)| {
                if entry.root_buffer_id != root_buffer_id || entry.orig_buffer_id != orig_buffer_id
                {
                    return None;
                }
                if entry.deleted {
                    return None;
                }
                if let Some(language) = language
                    && entry.language != language
                {
                    return None;
                }
                if !tag_filter.matches(entry.tag) {
                    return None;
                }
                Some(entry.value)
            })
            .collect::<Vec<_>>();
        items.shrink_to_fit();
        items
    }

    pub(crate) fn mark_parser_deleted(&mut self, id: u64) -> bool {
        let Some(entry) = self.parsers.get_mut(&id) else {
            return false;
        };
        entry.deleted = true;
        true
    }

    pub(crate) fn insert_node(
        &mut self,
        parser_id: u64,
        raw: tree_sitter::ffi::TSNode,
        generation: u64,
    ) -> u64 {
        let id = self.next_node_id;
        self.next_node_id += 1;
        self.nodes.insert(
            id,
            NodeEntry {
                parser_id,
                raw,
                generation,
            },
        );
        id
    }

    pub(crate) fn node(&self, id: u64) -> Option<&NodeEntry> {
        self.nodes.get(&id)
    }

    pub(crate) fn clear_nodes_for_parser(&mut self, parser_id: u64) {
        self.nodes.retain(|_, entry| entry.parser_id != parser_id);
    }

    pub(crate) fn insert_query(&mut self, language: SymId) -> u64 {
        let id = self.next_query_id;
        self.next_query_id += 1;
        self.queries.insert(
            id,
            QueryEntry {
                language,
                compiled: None,
            },
        );
        id
    }

    pub(crate) fn query(&self, id: u64) -> Option<&QueryEntry> {
        self.queries.get(&id)
    }

    pub(crate) fn query_mut(&mut self, id: u64) -> Option<&mut QueryEntry> {
        self.queries.get_mut(&id)
    }

    pub(crate) fn linecol_cache(&self, buffer_id: BufferId) -> Option<LineColCache> {
        self.linecol_caches.get(&buffer_id).copied()
    }

    pub(crate) fn set_linecol_cache(&mut self, buffer_id: BufferId, cache: LineColCache) {
        self.linecol_caches.insert(buffer_id, cache);
        for parser in self.parsers.values_mut() {
            if parser.orig_buffer_id == buffer_id && !parser.deleted {
                parser.tracking_linecol = true;
            }
        }
    }

    pub(crate) fn enable_linecol_tracking(&mut self, buffer_id: BufferId) {
        self.linecol_caches
            .entry(buffer_id)
            .or_insert(LineColCache {
                line: 1,
                col: 1,
                bytepos: 0,
            });
        for parser in self.parsers.values_mut() {
            if parser.orig_buffer_id == buffer_id && !parser.deleted {
                parser.tracking_linecol = true;
            }
        }
    }

    pub(crate) fn note_buffer_change(&mut self, buffer_id: BufferId, start_byte: EmacsBytePos) {
        let start_byte = start_byte.get();
        if let Some(cache) = self.linecol_caches.get_mut(&buffer_id)
            && cache.bytepos > start_byte
        {
            *cache = LineColCache {
                line: 1,
                col: 1,
                bytepos: 0,
            };
        }
    }

    /// Catch one parser up with the buffer's restriction before it is asked to
    /// parse, GNU `treesit_sync_visible_region` called from
    /// `treesit_ensure_parsed` (`src/treesit.c:1908-1914`) and from
    /// `Ftreesit_parser_set_included_ranges` (`src/treesit.c:2745-2746`).
    ///
    /// The tree is moved onto the new window rather than discarded, so a
    /// narrowing is a reason to reparse, never a reason to lose the tree.
    pub(crate) fn sync_visible_region(&mut self, parser_id: u64, buffer: &Buffer) {
        let Some(parser) = self.parsers.get_mut(&parser_id) else {
            return;
        };
        let tracking = if parser.tracking_linecol {
            ParserPointTracking::LineAndColumn
        } else {
            ParserPointTracking::BytesOnly
        };
        let Some(tree) = parser.tree.as_mut() else {
            return;
        };
        if tree.sync_visible_region(buffer, tracking) {
            parser
                .freshness
                .note_window_moved(ParserInputRevision::for_buffer(buffer));
        }
    }

    pub(crate) fn has_editable_tree(&self, buffer_id: BufferId) -> bool {
        self.parsers.values().any(|parser| {
            !parser.deleted && parser.orig_buffer_id == buffer_id && parser.tree.is_some()
        })
    }

    pub(crate) fn has_pending_edit(&self, buffer_id: BufferId) -> bool {
        self.pending_edits.contains_key(&buffer_id)
    }

    pub(crate) fn begin_buffer_edit(
        &mut self,
        buffer_id: BufferId,
        buffer: &Buffer,
        byte_range: EmacsByteRange,
    ) {
        if !self.has_editable_tree(buffer_id) {
            return;
        }
        let point_tracking = if self.linecol_caches.contains_key(&buffer_id) {
            ParserPointTracking::LineAndColumn
        } else {
            // GNU passes its `(1, 0)` dummy TSPoint when this buffer did not opt
            // into line and column tracking; byte offsets are sufficient for
            // the common parser path and make preparing a keystroke O(1).
            ParserPointTracking::BytesOnly
        };
        let windows = self.parser_windows(buffer_id);
        self.pending_edits.insert(
            buffer_id,
            PendingBufferEdit::for_buffer(buffer, byte_range, point_tracking, windows.into_iter()),
        );
    }

    fn parser_windows(&self, buffer_id: BufferId) -> Vec<(u64, EmacsByteRange)> {
        self.parsers
            .iter()
            .filter(|(_, parser)| !parser.deleted && parser.orig_buffer_id == buffer_id)
            .filter_map(|(id, parser)| parser.tree.as_ref().map(|tree| (*id, tree.visible())))
            .collect()
    }

    pub(crate) fn finish_buffer_edit(
        &mut self,
        buffer_id: BufferId,
        buffer: &Buffer,
        new_end_byte: EmacsBytePos,
    ) {
        let Some(edit) = self.pending_edits.remove(&buffer_id) else {
            return;
        };
        let new_revision = ParserInputRevision::for_buffer(buffer);

        let mut edited_parser_ids = Vec::new();
        for (parser_id, parser) in &mut self.parsers {
            if parser.deleted || parser.orig_buffer_id != buffer_id {
                continue;
            }
            if let Some(tree) = parser.tree.as_mut() {
                if parser
                    .freshness
                    .accepts_incremental_edit(edit.content_revision)
                {
                    let relative =
                        edit.for_window(buffer, *parser_id, tree.visible(), new_end_byte);
                    tree.apply_edit(&relative.edit, relative.new_visible);
                    parser.freshness = ParserFreshness::ReparsePending(new_revision);
                } else {
                    // A text change that never reached this hook left the tree's
                    // window describing text that is no longer there, so unlike
                    // GNU -- which sees every change -- the tree cannot be
                    // reused.
                    parser.freshness = ParserFreshness::FullReparseRequired;
                }
                parser.generation = parser.generation.saturating_add(1);
                parser.last_changed_ranges.clear();
                edited_parser_ids.push(*parser_id);
            }
        }

        for parser_id in edited_parser_ids {
            self.clear_nodes_for_parser(parser_id);
        }
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;

fn parser_point_at(
    buffer: &Buffer,
    visible_start: EmacsBytePos,
    target: EmacsBytePos,
    tracking: ParserPointTracking,
) -> Point {
    if tracking == ParserPointTracking::BytesOnly {
        // GNU `TREESIT_TS_POINT_1_0` (`treesit.c`) is deliberately distinct
        // from the real first-byte coordinate `(0, 0)`.
        return Point::new(1, 0);
    }
    let target = target.max(visible_start);
    let row = buffer.count_newlines_emacs_byte(visible_start, target);
    let line_start = buffer
        .prev_newline_emacs_byte(target, visible_start)
        .map_or(visible_start.get(), |newline| newline.get() + 1);
    Point {
        row,
        column: target.get().saturating_sub(line_start),
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ParserTagFilter {
    Any,
    Exact(Value),
}

impl ParserTagFilter {
    fn matches(self, candidate: Value) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(expected) => candidate == expected,
        }
    }
}

pub(crate) fn make_parser_value(
    id: u64,
    language_symbol: Value,
    buffer: Value,
    tag: Value,
) -> Value {
    Value::make_record(vec![
        Value::symbol(TREESIT_PARSER_TAG),
        Value::fixnum(id as i64),
        language_symbol,
        buffer,
        tag,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ])
}

pub(crate) fn make_node_value(id: u64, parser: Value) -> Value {
    Value::make_record(vec![
        Value::symbol(TREESIT_NODE_TAG),
        Value::fixnum(id as i64),
        parser,
    ])
}

pub(crate) fn make_query_value(id: u64, language_symbol: Value, source: Value) -> Value {
    Value::make_record(vec![
        Value::symbol(TREESIT_COMPILED_QUERY_TAG),
        Value::fixnum(id as i64),
        language_symbol,
        source,
    ])
}

pub(crate) fn parser_id(value: Value) -> Option<u64> {
    record_id_with_tag(value, TREESIT_PARSER_TAG, PARSER_SLOT_ID)
}

pub(crate) fn node_id(value: Value) -> Option<u64> {
    record_id_with_tag(value, TREESIT_NODE_TAG, NODE_SLOT_ID)
}

pub(crate) fn query_id(value: Value) -> Option<u64> {
    record_id_with_tag(value, TREESIT_COMPILED_QUERY_TAG, QUERY_SLOT_ID)
}

pub(crate) fn is_parser(value: Value) -> bool {
    record_tag_is(value, TREESIT_PARSER_TAG)
}

pub(crate) fn is_node(value: Value) -> bool {
    record_tag_is(value, TREESIT_NODE_TAG)
}

pub(crate) fn is_compiled_query(value: Value) -> bool {
    record_tag_is(value, TREESIT_COMPILED_QUERY_TAG)
}

fn record_id_with_tag(value: Value, expected_tag: &str, id_slot: usize) -> Option<u64> {
    let items = value.as_record_data()?;
    let tag = items.first()?.as_symbol_name()?;
    if tag != expected_tag {
        return None;
    }
    let id = items.get(id_slot)?.as_fixnum()?;
    (id >= 0).then_some(id as u64)
}

fn record_tag_is(value: Value, expected_tag: &str) -> bool {
    value
        .as_record_data()
        .and_then(|items| items.first().copied())
        .and_then(|tag| tag.as_symbol_name())
        == Some(expected_tag)
}
