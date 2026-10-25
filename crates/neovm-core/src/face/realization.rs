//! Frame-owned face realization, separate from authoritative Lisp definitions.
//!
//! | Knob | Default | Meaning |
//! | --- | --- | --- |
//! | `NEOVM_FACE_REALIZATION_LIFECYCLE` | on | GNU basic/demand realization through Lisp TTY color callbacks. Set to `0` for the previous path. |

use super::{Face, FaceTable};
use crate::emacs_core::value::Value;
use crate::gc_trace::GcTrace;
use crate::tagged::transport::SharedRoot;
use enum_map::EnumMap;
use rustc_hash::{FxHashMap, FxHasher};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

/// Process-wide immutable campaign selector. No mutable Lisp state is shared.
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    // Unset is on. An explicit value other than "1", including "0", stays off
    // so a measurement can still select the previous path.
    *ENABLED.get_or_init(|| match std::env::var("NEOVM_FACE_REALIZATION_LIFECYCLE") {
        Ok(value) => value == "1",
        Err(_) => true,
    })
}

/// GNU's forced basic face roles, in realization order. A role retains its
/// own result even when advised color resolution gives equal source attrs
/// different colors on successive calls (xfaces.c:5952-5983,6182-6219).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    enum_map::Enum,
    strum::EnumIter,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum BasicFace {
    Default,
    ModeLineActive,
    ModeLineInactive,
    ToolBar,
    Fringe,
    HeaderLineActive,
    HeaderLineInactive,
    ScrollBar,
    Border,
    Cursor,
    Mouse,
    Menu,
    VerticalBorder,
    WindowDivider,
    WindowDividerFirstPixel,
    WindowDividerLastPixel,
    InternalBorder,
    ChildFrameBorder,
    TabBar,
    TabLineActive,
    TabLineInactive,
    Margin,
}

impl BasicFace {
    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn from_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }
}

/// GNU classifies faces by frame output method (frame.h:932-933), and
/// is_tty_frame accepts termcap/MSDOS rather than the initial output kind
/// (dispnew.c:3491-3494). NeoVM's primary native record is redescribed in
/// place during display setup, so its current output kind is authoritative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameFaceClass {
    Initial,
    Termcap,
    WindowSystem,
}

pub(crate) fn frame_face_class(frame: &crate::window::Frame) -> Option<FrameFaceClass> {
    use crate::emacs_core::terminal::pure::{
        TerminalOutputMethod, live_terminal_output_method_for_id,
    };
    if frame.effective_window_system().is_some() {
        return Some(FrameFaceClass::WindowSystem);
    }
    live_terminal_output_method_for_id(frame.terminal_id).map(|method| match method {
        TerminalOutputMethod::Initial => FrameFaceClass::Initial,
        TerminalOutputMethod::Termcap => FrameFaceClass::Termcap,
        TerminalOutputMethod::WindowSystem => FrameFaceClass::WindowSystem,
    })
}

/// Whether this frame belongs to an actual termcap face target. Call on the
/// installed owning mutator. The immutable campaign knob is checked separately.
/// Bare frame fixtures and batch frames on output_initial retain dummy faces;
/// primary/secondary and suspended real TTYs retain termcap realization.
pub fn frame_is_tty_target(frame: &crate::window::Frame) -> bool {
    frame_face_class(frame) == Some(FrameFaceClass::Termcap)
}

/// Which existing frame definitions a mutation can affect. The display
/// freshness counter is global, while GNU realization invalidation is scoped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FaceChangeScope<'a> {
    AllFrames,
    Frames(&'a [crate::window::FrameId]),
    /// Definitions change, but GNU leaves existing realized combinations and
    /// forced basic roles intact until a real face-cache invalidation.
    DefinitionsOnly(&'a [crate::window::FrameId]),
}

impl FaceChangeScope<'_> {
    pub(crate) fn invalidates(self, frame: crate::window::FrameId) -> bool {
        match self {
            Self::AllFrames => true,
            Self::Frames(frames) => frames.contains(&frame),
            Self::DefinitionsOnly(_) => false,
        }
    }
}

/// Identity of one realized-face lifetime. Definitions-only changes retain
/// the token; full cache retirement replaces it. The exclusive frame mutator
/// shares this immutable identity locally, never across mutator threads.
#[derive(Clone, Debug)]
pub(crate) struct RealizationEpoch(std::rc::Rc<()>);

impl RealizationEpoch {
    fn new() -> Self {
        Self(std::rc::Rc::new(()))
    }

    fn same_as(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

static_assertions::assert_not_impl_any!(RealizationEpoch: Send, Sync, Copy);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DefaultLookupPosition {
    #[default]
    Shadowed,
    Winner,
}

/// Each frame's exclusive mutator owns this cache. Immutable FaceTable handles
/// remain copy-on-write; no cache entry or Lisp value is shared across mutators.
#[derive(Default)]
pub(crate) struct FrameFaceRealizationCache {
    generation: Option<u64>,
    epoch: Option<RealizationEpoch>,
    table: Option<FaceTable>,
    /// One independently retained root bundle per immutable raw snapshot.
    /// Layout may keep Arc clones after this cache retires its source table.
    root_lease: Option<std::sync::Arc<[SharedRoot]>>,
    entries: FxHashMap<u64, Vec<(Face, Face)>>,
    /// Allocated only by the lifecycle path, owned by the same frame mutator.
    /// The enum owns the role count and indexing; no numeric role mirrors exist.
    basics: Option<Box<EnumMap<BasicFace, Option<(Face, Face)>>>>,
    /// GNU's cached DEFAULT_FACE_ID retains its complete Lisp source vector.
    /// The typed display projection omits font/fontset and original plist slots.
    default_lface: Option<Value>,
    default_predecessor: Option<(Face, Face)>,
    default_lookup_position: DefaultLookupPosition,
    /// Face names whose frame-local Lisp vectors changed after `table` was
    /// derived. GNU stores definitions as the frame's face vectors and never
    /// re-derives an untouched one: a set writes one slot (xfaces.c:3767-3773)
    /// and realization stays content-keyed (lookup_face, xfaces.c:5064-5093).
    /// An unknown writer (no face name) sets `definitions_all_dirty` instead.
    dirty_definitions: rustc_hash::FxHashSet<Box<str>>,
    definitions_all_dirty: bool,
}

fn route_hash(face: &Face) -> u64 {
    let mut hash = FxHasher::default();
    face.color_specifications.foreground.hash(&mut hash);
    face.color_specifications.background.hash(&mut hash);
    face.color_specifications.underline.hash(&mut hash);
    face.color_specifications
        .underline_operations
        .hash(&mut hash);
    face.weight
        .map(|weight| weight.css_weight())
        .hash(&mut hash);
    hash.finish()
}

// Face derives equality through Value's structural `equal` comparison, as
// required by GNU face_attr_equal_p (xfaces.c:4481-4507).

impl std::fmt::Debug for FrameFaceRealizationCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FrameFaceRealizationCache")
            .field("generation", &self.generation)
            .field("faces", &self.table.as_ref().map(FaceTable::len))
            .field("attribute_buckets", &self.entries.len())
            .finish()
    }
}

static_assertions::assert_not_impl_any!(FrameFaceRealizationCache: Send, Sync);

impl FrameFaceRealizationCache {
    pub(crate) fn generation_matches(&self, generation: u64) -> bool {
        self.generation == Some(generation)
    }

    pub(crate) fn table(&self, generation: u64) -> Option<&FaceTable> {
        (self.generation == Some(generation)
            && self.dirty_definitions.is_empty()
            && !self.definitions_all_dirty)
            .then_some(self.table.as_ref())
            .flatten()
    }

    pub(crate) fn prepare(&mut self, generation: u64, table: FaceTable) {
        self.generation = Some(generation);
        self.epoch = Some(RealizationEpoch::new());
        self.table = Some(table);
        self.root_lease = None;
        self.dirty_definitions.clear();
        self.definitions_all_dirty = false;
        self.entries.clear();
        self.basics = None;
        self.default_lface = None;
        self.default_predecessor = None;
        self.default_lookup_position = DefaultLookupPosition::Shadowed;
    }

    /// Refresh definitions through the two audited recorded realization paths.
    /// The opaque consumed permit is minted inline only in xfaces realization;
    /// it adds no state, checks, counter writes or allocation. This cannot
    /// revive an invalid generation: only prepare publishes one.
    pub(crate) fn replace_recorded_definitions(
        &mut self,
        table: FaceTable,
        _refresh: crate::emacs_core::xfaces::realization::RecordedDefinitionsRefresh,
    ) {
        self.table = Some(table);
        self.root_lease = None;
        self.dirty_definitions.clear();
        self.definitions_all_dirty = false;
    }

    /// Test-only raw replacement fault seam; unavailable in production.
    ///
    /// Existing lifecycle tests intentionally bypass the recorded-writer
    /// contract. A whole new table/palette must still reject a Rows receipt at
    /// equal count/epoch. This seam does NOT promise detection of an unrecorded
    /// faces-only replacement that preserves the palette and realization epoch.
    #[cfg(test)]
    pub(crate) fn replace_definitions(&mut self, table: FaceTable) {
        crate::emacs_core::xfaces::realization::replace_definitions_for_test_fault(self, table);
    }

    /// The last derived table plus the names that drifted from it. Callers
    /// rebuild only those entries; the returned table shares copy-on-write
    /// storage, so untouched entries keep their identity and content.
    pub(crate) fn reusable_definitions(
        &self,
    ) -> Option<(&FaceTable, &rustc_hash::FxHashSet<Box<str>>)> {
        self.table
            .as_ref()
            .map(|table| (table, &self.dirty_definitions))
    }

    pub(crate) fn mark_definition_dirty(&mut self, name: &str) {
        self.dirty_definitions.insert(Box::from(name));
    }

    pub(crate) fn mark_all_definitions_dirty(&mut self) {
        self.definitions_all_dirty = true;
    }

    pub(crate) fn definitions_all_dirty(&self) -> bool {
        self.definitions_all_dirty
    }

    pub(crate) fn root_lease(&self) -> Option<std::sync::Arc<[SharedRoot]>> {
        self.root_lease.clone()
    }

    pub(crate) fn set_root_lease(&mut self, lease: std::sync::Arc<[SharedRoot]>) {
        self.root_lease = Some(lease);
    }

    /// A no-purge mutation invalidates just the raw definition snapshot. The
    /// generation remains valid for realized combinations and fixed basic
    /// roles. The retired table stays available for incremental reuse; the
    /// dirty mark keeps `table` from serving stale entries. Passing no face
    /// name retires the whole snapshot: an unknown writer cannot be scoped.
    pub(crate) fn definitions_changed(&mut self, previous: u64, current: u64, face: Option<&str>) {
        match face {
            Some(name) => self.mark_definition_dirty(name),
            None => self.definitions_all_dirty = true,
        }
        if self.generation == Some(previous) {
            self.generation = Some(current);
            self.root_lease = None;
        }
    }

    pub(crate) fn basic(&self, role: BasicFace) -> Option<&(Face, Face)> {
        self.basics.as_ref()?[role].as_ref()
    }

    pub(crate) fn insert_basic(&mut self, role: BasicFace, source: Face, realized: Face) {
        if role == BasicFace::Default {
            self.retire_default();
            self.default_predecessor = self
                .lookup(&source)
                .cloned()
                .map(|paint| (source.clone(), paint));
        }
        let basics = self
            .basics
            .get_or_insert_with(|| Box::new(EnumMap::default()));
        basics[role] = Some((source.clone(), realized.clone()));
        self.insert(source, realized);
        if role == BasicFace::Default {
            self.default_lookup_position = DefaultLookupPosition::Winner;
        }
    }

    /// GNU removes only the former default node (xfaces.c:6189-6198).
    /// Preserve its prior generic winner, whether ordinary or another basic,
    /// because ASCII entries form a newest-first list (xfaces.c:4962-5000).
    pub(crate) fn retire_default(&mut self) {
        self.default_lface = None;
        let predecessor = self.default_predecessor.take();
        let was_winner = self.default_lookup_position == DefaultLookupPosition::Winner;
        self.default_lookup_position = DefaultLookupPosition::Shadowed;
        let Some((old, _)) = self
            .basics
            .as_mut()
            .and_then(|roles| roles[BasicFace::Default].take())
        else {
            return;
        };
        if !was_winner {
            return;
        }
        let hash = route_hash(&old);
        if let Some(bucket) = self.entries.get_mut(&hash) {
            bucket.retain(|(source, _)| *source != old);
            if bucket.is_empty() {
                self.entries.remove(&hash);
            }
        }
        if let Some((source, result)) = predecessor {
            self.insert(source, result);
        }
    }

    pub(crate) fn default_lface(&self) -> Option<Value> {
        self.default_lface
    }

    pub(crate) fn retain_default_lface(&mut self, source: Value) {
        self.default_lface = Some(source);
    }

    pub(crate) fn invalidate(&mut self) {
        self.generation = None;
        self.epoch = None;
        self.default_predecessor = None;
        self.default_lookup_position = DefaultLookupPosition::Shadowed;
        self.default_lface = None;
        self.root_lease = None;
        self.definitions_all_dirty = true;
    }

    pub(crate) fn realization_epoch(&self) -> Option<RealizationEpoch> {
        self.epoch.clone()
    }

    pub(crate) fn retains_epoch(&self, epoch: &RealizationEpoch) -> bool {
        self.epoch
            .as_ref()
            .is_some_and(|current| current.same_as(epoch))
    }

    /// Carry an unchanged source through a frame-local global-counter bump.
    /// An older or invalid cache must stay stale rather than being resurrected.
    pub(crate) fn preserve_across_change(&mut self, previous: u64, current: u64) {
        if self.generation == Some(previous) {
            self.generation = Some(current);
        }
    }

    pub(crate) fn lookup(&self, attributes: &Face) -> Option<&Face> {
        self.entries
            .get(&route_hash(attributes))?
            .iter()
            .find_map(|(source, realized)| (source == attributes).then_some(realized))
    }

    /// Project the existing generic cache winner, not its equivalent paint.
    /// Forced basic faces can shadow default with an identical attribute key.
    pub(crate) fn lookup_uses_default_slot(&self, attributes: &Face) -> Option<bool> {
        self.lookup(attributes)?;
        Some(
            self.default_lookup_position == DefaultLookupPosition::Winner
                && self
                    .basic(BasicFace::Default)
                    .is_some_and(|(source, _)| source == attributes),
        )
    }

    pub(crate) fn insert(&mut self, attributes: Face, realized: Face) {
        if self
            .basic(BasicFace::Default)
            .is_some_and(|(source, _)| *source == attributes)
        {
            self.default_lookup_position = DefaultLookupPosition::Shadowed;
        }
        let entries = self.entries.entry(route_hash(&attributes)).or_default();
        if let Some((_, previous)) = entries.iter_mut().find(|(source, _)| *source == attributes) {
            *previous = realized;
        } else {
            entries.push((attributes, realized));
        }
    }
}

impl GcTrace for FrameFaceRealizationCache {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        roots.extend(self.default_lface);
        if let Some((source, realized)) = &self.default_predecessor {
            source.trace_roots(roots);
            realized.trace_roots(roots);
        }
        if let Some(table) = &self.table {
            table.trace_roots(roots);
        }
        if let Some(basics) = &self.basics {
            for (source, realized) in basics.values().flatten() {
                source.trace_roots(roots);
                realized.trace_roots(roots);
            }
        }
        for entries in self.entries.values() {
            for (source, realized) in entries {
                source.trace_roots(roots);
                realized.trace_roots(roots);
            }
        }
    }
}

#[cfg(test)]
#[path = "realization/tests/cache_test.rs"]
mod tests;

/// GNU's basic face names in realization order, derived from its typed roles.
pub fn basic_face_names() -> impl Iterator<Item = &'static str> {
    crate::emacs_core::xfaces::realization::basic_face_names()
}
