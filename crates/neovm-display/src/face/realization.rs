//! Frame-owned face realization, separate from authoritative Lisp definitions.
//!
//! | Knob | Default | Meaning |
//! | --- | --- | --- |
//! | `NEOVM_FACE_REALIZATION_LIFECYCLE` | on | GNU basic/demand realization through Lisp TTY color callbacks. Set to `0` for the previous path. |

use super::{Face, FaceTable};
use crate::emacs_core::xfaces::realization::RecordedDefinitionsRefresh;
use crate::window::DisplayFrameExt;
use enum_map::EnumMap;
use neovm_value::gc_trace::GcTrace;
use neovm_value::tagged::transport::SharedRoot;
use neovm_value::value::Value;
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub use neovm_eval::emacs_core::eval::data::face::BasicFace;
pub use neovm_eval::emacs_core::eval::data::face::enabled;

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

pub use neovm_eval::emacs_core::eval::data::face::CachedFaceSlot;
pub(crate) use neovm_eval::emacs_core::eval::data::face::{
    DefaultLookupPosition, FrameFaceRealizationCache, RealizationEpoch,
};

static_assertions::assert_not_impl_any!(RealizationEpoch: Send, Sync, Copy);

static_assertions::assert_impl_all!(CachedFaceSlot: Copy, Send, Sync);
const _: () = assert!(std::mem::size_of::<CachedFaceSlot>() == 1);

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

static_assertions::assert_not_impl_any!(FrameFaceRealizationCache: Send, Sync);

/// Stage-4 W5/E13 upper extension trait over the eval-owned type.
#[doc(hidden)]
pub trait FrameFaceRealizationCacheExt {
    fn generation_matches(&self, generation: u64) -> bool;

    fn prepare(&mut self, generation: u64, table: FaceTable);

    fn replace_recorded_definitions(
        &mut self,
        table: FaceTable,
        _refresh: RecordedDefinitionsRefresh,
    );

    #[cfg(test)]
    fn replace_definitions(&mut self, table: FaceTable);

    fn reusable_definitions(&self) -> Option<(&FaceTable, &rustc_hash::FxHashSet<Box<str>>)>;

    fn mark_definition_dirty(&mut self, name: &str);

    fn mark_all_definitions_dirty(&mut self);

    fn definitions_all_dirty(&self) -> bool;

    fn root_lease(&self) -> Option<std::sync::Arc<[SharedRoot]>>;

    fn set_root_lease(&mut self, lease: std::sync::Arc<[SharedRoot]>);

    fn definitions_changed(&mut self, previous: u64, current: u64, face: Option<&str>);

    fn insert_basic(&mut self, role: BasicFace, source: Face, realized: Face);

    fn retire_default(&mut self);

    fn default_lface(&self) -> Option<Value>;

    fn retain_default_lface(&mut self, source: Value);

    fn invalidate(&mut self);

    fn preserve_across_change(&mut self, previous: u64, current: u64);

    fn lookup(&self, attributes: &Face) -> Option<&Face>;

    fn lookup_slot(&self, attributes: &Face) -> CachedFaceSlot;

    fn insert(&mut self, attributes: Face, realized: Face);
}

impl FrameFaceRealizationCacheExt for FrameFaceRealizationCache {
    fn generation_matches(&self, generation: u64) -> bool {
        self.generation == Some(generation)
    }

    fn prepare(&mut self, generation: u64, table: FaceTable) {
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
    fn replace_recorded_definitions(
        &mut self,
        table: FaceTable,
        _refresh: RecordedDefinitionsRefresh,
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
    fn replace_definitions(&mut self, table: FaceTable) {
        crate::emacs_core::xfaces::realization::replace_definitions_for_test_fault(self, table);
    }

    /// The last derived table plus the names that drifted from it. Callers
    /// rebuild only those entries; the returned table shares copy-on-write
    /// storage, so untouched entries keep their identity and content.
    fn reusable_definitions(&self) -> Option<(&FaceTable, &rustc_hash::FxHashSet<Box<str>>)> {
        self.table
            .as_ref()
            .map(|table| (table, &self.dirty_definitions))
    }

    fn mark_definition_dirty(&mut self, name: &str) {
        self.dirty_definitions.insert(Box::from(name));
    }

    fn mark_all_definitions_dirty(&mut self) {
        self.definitions_all_dirty = true;
    }

    fn definitions_all_dirty(&self) -> bool {
        self.definitions_all_dirty
    }

    fn root_lease(&self) -> Option<std::sync::Arc<[SharedRoot]>> {
        self.root_lease.clone()
    }

    fn set_root_lease(&mut self, lease: std::sync::Arc<[SharedRoot]>) {
        self.root_lease = Some(lease);
    }

    /// A no-purge mutation invalidates just the raw definition snapshot. The
    /// generation remains valid for realized combinations and fixed basic
    /// roles. The retired table stays available for incremental reuse; the
    /// dirty mark keeps `table` from serving stale entries. Passing no face
    /// name retires the whole snapshot: an unknown writer cannot be scoped.
    fn definitions_changed(&mut self, previous: u64, current: u64, face: Option<&str>) {
        match face {
            Some(name) => self.mark_definition_dirty(name),
            None => self.definitions_all_dirty = true,
        }
        if self.generation == Some(previous) {
            self.generation = Some(current);
            self.root_lease = None;
        }
    }

    fn insert_basic(&mut self, role: BasicFace, source: Face, realized: Face) {
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
    fn retire_default(&mut self) {
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

    fn default_lface(&self) -> Option<Value> {
        self.default_lface
    }

    fn retain_default_lface(&mut self, source: Value) {
        self.default_lface = Some(source);
    }

    fn invalidate(&mut self) {
        self.generation = None;
        self.epoch = None;
        self.default_predecessor = None;
        self.default_lookup_position = DefaultLookupPosition::Shadowed;
        self.default_lface = None;
        self.root_lease = None;
        self.definitions_all_dirty = true;
    }

    /// Carry an unchanged source through a frame-local global-counter bump.
    /// An older or invalid cache must stay stale rather than being resurrected.
    fn preserve_across_change(&mut self, previous: u64, current: u64) {
        if self.generation == Some(previous) {
            self.generation = Some(current);
        }
    }

    fn lookup(&self, attributes: &Face) -> Option<&Face> {
        self.entries
            .get(&route_hash(attributes))?
            .iter()
            .find_map(|(source, realized)| (source == attributes).then_some(realized))
    }

    /// Project the existing generic cache winner, not its equivalent paint.
    /// Forced basic faces can shadow default with an identical attribute key.
    fn lookup_slot(&self, attributes: &Face) -> CachedFaceSlot {
        if self.lookup(attributes).is_none() {
            return CachedFaceSlot::Unavailable;
        }
        if self.default_lookup_position == DefaultLookupPosition::Winner
            && self
                .basic(BasicFace::Default)
                .is_some_and(|(source, _)| source == attributes)
        {
            CachedFaceSlot::Default
        } else {
            CachedFaceSlot::NonDefault
        }
    }

    fn insert(&mut self, attributes: Face, realized: Face) {
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

// The default bodies above are the implementation; this binds them to the
// upper-layer type that owns the state.

#[cfg(test)]
#[path = "realization/tests/cache_test.rs"]
mod tests;

/// GNU's basic face names in realization order, derived from its typed roles.
pub fn basic_face_names() -> impl Iterator<Item = &'static str> {
    crate::emacs_core::xfaces::realization::basic_face_names()
}
