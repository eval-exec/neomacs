//! Frame-scoped ownership for face identity across speculative layout attempts.
//!
//! Dynamic face ids are CONTENT-ADDRESSED and persistent: the arena keeps a
//! realization-identity -> id map across layout passes (the analogue of GNU's
//! per-frame face_cache: xfaces.c lookup_face hashes the attribute vector and
//! reuses the realized face's id on every redisplay). Without that memory,
//! ids were handed out in first-use order per pass, so one extra face
//! checkpoint early in a pass renumbered every face after it — the renderer
//! then saw dozens of "modified" faces per keystroke and mode-line composed
//! clusters missed their caches on every frame.

use neomacs_display_protocol::FrameFaceMap;
use neomacs_display_protocol::face::{BasicFaceId, Face};
use neomacs_display_protocol::types::FaceId;
use rustc_hash::FxHashMap as HashMap;
use rustc_hash::FxHasher;
use std::cell::RefCell;
use std::hash::Hasher;
use std::rc::{Rc, Weak};
use std::sync::Arc;

/// Entries above this drop the whole identity map at the next seal. Content
/// addressing means stale entries can never serve wrong data (changed face
/// definitions produce different content, hence different keys), so the only
/// risk is unbounded growth from pathological content churn; the reset costs
/// one frame of "added" faces, which the renderer absorbs without clearing
/// anything.
const REALIZED_IDENTITY_CAP: usize = 4096;

/// Entry budget for the persistent resolved-face memo, mirroring
/// [`REALIZED_IDENTITY_CAP`]: past it, sealing keeps only the fresh entries.
const RESOLVED_MEMO_CAP: usize = 4096;

/// Route-hash keyed memo from a resolved face to its stable id.
type ResolvedFaceMemo = HashMap<u64, Vec<(crate::neovm_bridge::ResolvedFace, FaceId)>>;

/// The realization identity of a face: every field that participates in what
/// the face LOOKS like, with enrichment stripped. Font realization may fill
/// in metrics, the exact font file, and the resolved font handle after row
/// construction (see [`FrameFaceAttempt::seal`]); two realizations that
/// differ only in those fields are the same face. Stored identity-map entries
/// own this projection; [`same_face_realization`] compares it while borrowing
/// complete faces, so validation need not clone discarded payloads.
pub(crate) fn face_realization_identity(face: &Face) -> Face {
    let mut identity = face.clone();
    identity.id = FaceId::new(0);
    identity.font_ascent = 0;
    identity.font_descent = 0;
    identity.font_file_path = None;
    identity.default_resolved_font_id = None;
    identity
}

/// Compare the complete realization projection without owning or cloning its
/// strings, gradients, or stipple payload. The exhaustive pattern deliberately
/// requires newly added protocol fields to choose their identity semantics.
fn same_face_realization(left: &Face, right: &Face) -> bool {
    let Face {
        id: _,
        tty_space_identity,
        foreground,
        background,
        terminal_foreground,
        terminal_background,
        use_default_foreground,
        use_default_background,
        underline_color,
        terminal_underline_color,
        overline_color,
        strike_through_color,
        box_color,
        font_family,
        font_size,
        font_weight,
        attributes,
        underline_style,
        box_type,
        box_line_width,
        box_corner_radius,
        box_border_style,
        box_border_speed,
        box_color2,
        font_file_path: _,
        font_ascent: _,
        font_descent: _,
        underline_position,
        underline_thickness,
        background_gradient,
        lisp_name,
        default_resolved_font_id: _,
        stipple,
        underline_placement,
    } = left;
    tty_space_identity == &right.tty_space_identity
        && foreground == &right.foreground
        && background == &right.background
        && terminal_foreground == &right.terminal_foreground
        && terminal_background == &right.terminal_background
        && use_default_foreground == &right.use_default_foreground
        && use_default_background == &right.use_default_background
        && underline_color == &right.underline_color
        && terminal_underline_color == &right.terminal_underline_color
        && overline_color == &right.overline_color
        && strike_through_color == &right.strike_through_color
        && box_color == &right.box_color
        && font_family == &right.font_family
        && font_size == &right.font_size
        && font_weight == &right.font_weight
        && attributes == &right.attributes
        && underline_style == &right.underline_style
        && box_type == &right.box_type
        && box_line_width == &right.box_line_width
        && box_corner_radius == &right.box_corner_radius
        && box_border_style == &right.box_border_style
        && box_border_speed == &right.box_border_speed
        && box_color2 == &right.box_color2
        && underline_position == &right.underline_position
        && underline_thickness == &right.underline_thickness
        && background_gradient == &right.background_gradient
        && lisp_name == &right.lisp_name
        && stipple == &right.stipple
        && underline_placement == &right.underline_placement
}

/// Routing hash for the identity buckets. Equality is decided by
/// `Face::eq` on the canonical projection, never by this hash, so hashing a
/// SUBSET of identity fields is safe (a missed field only costs bucket
/// collisions) — but hashing anything OUTSIDE the identity projection would
/// split equal identities across buckets and re-introduce id instability.
fn face_identity_hash(identity: &Face) -> u64 {
    let mut hasher = FxHasher::default();
    hasher.write_u32(identity.foreground.r.to_bits());
    hasher.write_u32(identity.foreground.g.to_bits());
    hasher.write_u32(identity.foreground.b.to_bits());
    hasher.write_u32(identity.background.r.to_bits());
    hasher.write_u32(identity.background.g.to_bits());
    hasher.write_u32(identity.background.b.to_bits());
    hasher.write(identity.font_family.as_bytes());
    hasher.write_u16(identity.font_weight);
    hasher.write_u32(identity.font_size.to_bits());
    hasher.write_u8(identity.underline_style as u8);
    if let Some(name) = &identity.lisp_name {
        hasher.write(name.as_bytes());
    }
    hasher.finish()
}

/// Routing hash for the attempt-local resolved-face memo. Same subset rule
/// as [`face_identity_hash`]: equality is full `ResolvedFace::eq`.
fn resolved_face_route_hash(face: &crate::neovm_bridge::ResolvedFace) -> u64 {
    let mut hasher = FxHasher::default();
    hasher.write_u32(face.fg);
    hasher.write_u32(face.bg);
    hasher.write(face.font_family.as_bytes());
    hasher.write_u16(face.font_weight);
    hasher.write_u32(face.font_size.to_bits());
    hasher.write_u8(face.underline_style);
    if let Some(name) = &face.lisp_name {
        hasher.write(name.as_bytes());
    }
    hasher.finish()
}

type RealizedIdentityMap = HashMap<u64, Vec<(Face, FaceId)>>;

fn realized_identity_lookup(
    map: &RealizedIdentityMap,
    hash: u64,
    identity: &Face,
) -> Option<FaceId> {
    map.get(&hash)?
        .iter()
        .find_map(|(face, id)| same_face_realization(face, identity).then_some(*id))
}

/// The evaluator's `face_change_count` a frame face table was realized under.
///
/// A distinct type (not a raw `u64`) so a seat count can never stand in for a
/// seal count or a font-catalog generation at a call site: the only way to
/// compare epochs is through this type, and the engine's frame boundary is the
/// single place that turns a mismatched epoch into a whole-cache retirement
/// (GNU `face_change`, src/xfaces.c:3098-3105).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FaceEpoch(u64);

impl FaceEpoch {
    pub(crate) fn new(count: u64) -> Self {
        Self(count)
    }

    pub(crate) fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FrameFaceGeneration(u64);

impl Default for FrameFaceGeneration {
    fn default() -> Self {
        Self(1)
    }
}

impl FrameFaceGeneration {
    fn next(self) -> Self {
        Self(
            self.0
                .checked_add(1)
                .expect("frame face generation exhausted"),
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FrameFaceArena {
    owner: Arc<FrameFaceOwner>,
    snapshot: Arc<FrameFaceSnapshot>,
    generation: FrameFaceGeneration,
    faces: Arc<FrameFaceMap>,
    /// Persistent realization-identity -> stable id map (GNU face_cache
    /// analogue). Survives seals so the same content keeps the same id
    /// across frames regardless of realization order.
    realized: Arc<RealizedIdentityMap>,
    /// Persistent resolved-face -> stable id memo (the resolved counterpart
    /// of `realized`). Survives seals so rendering probes for the same
    /// interned source skip the canonical projection and identity-map
    /// lookup in every later attempt of this lineage -- GNU serves repeated
    /// merged-lface lookups from the frame face cache the same way
    /// (`lookup_face`, src/xfaces.c:5064-5093).
    resolved_memo: Arc<ResolvedFaceMemo>,
    /// Persistent monotonic id allocator. Never rewinds within an arena
    /// lineage, so a stable id can never be re-minted for different content.
    next_face_id: u32,
    /// GNU `face_change_count` this table was realized under. A different
    /// epoch at the next frame retires the whole cache (GNU `face_change`
    /// frees realized faces, src/xfaces.c:3098-3105, :4889-4954), so retained
    /// rows minted under the old definitions can never be admitted.
    face_epoch: FaceEpoch,
}

#[derive(Clone, Debug)]
pub(crate) struct FrameFaceAttempt {
    state: Rc<RefCell<FrameFaceAttemptState>>,
}

/// Exclusive owner of one speculative attempt's face table.
///
/// The output owner holds this value for the whole layout transaction; layout
/// producers only ever receive [`FrameFaceAttempt`] handles.  Consuming the
/// owner with [`FrameFaceTableAttempt::into_publication`] is the only way to
/// obtain the finished table, so a table that was handed to the presented
/// frame state cannot be observed through a live attempt: there is no `take`
/// through `&self`.
pub(crate) struct FrameFaceTableAttempt {
    state: Rc<RefCell<FrameFaceAttemptState>>,
}

/// Why an attempt's table could not be handed off: a layout handle is still
/// alive, so consuming the owner would leave that handle observing a state
/// whose table has moved.  The table is left untouched and the frame is
/// rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameFaceStillShared {
    /// Strong attempt/owner handles at the failed hand-off.
    pub(crate) handles: usize,
}

/// The publication step's exclusive claim on a finished attempt.
///
/// Built only by [`FrameFaceTableAttempt::into_publication`], which consumes
/// the owner and proves no layout handle survives.  The attempt state is owned
/// by value (no `Rc` remains), so no reader can observe the table after
/// [`Self::install`] moves it into the presented frame state.
pub(crate) struct FrameFacePublication {
    state: FrameFaceAttemptState,
    faces: FrameFaceMap,
}

impl FrameFacePublication {
    /// Move the published table out and return the seal authority.
    ///
    /// The caller places the table in the presented frame state; one
    /// transaction publishes it, so this is a move, never a copy.
    pub(crate) fn install(self) -> (FrameFaceMap, FrameFaceSeal) {
        let mut published: Vec<FaceId> = self.faces.keys().copied().collect();
        published.sort_unstable();
        (
            self.faces,
            FrameFaceSeal {
                state: self.state,
                published,
            },
        )
    }
}

/// The seal authority for a table installed into the presented frame.
///
/// It carries the attempt's non-table state and the sorted published ids;
/// `seal` consumes it, so one attempt can seal at most once.
pub(crate) struct FrameFaceSeal {
    state: FrameFaceAttemptState,
    published: Vec<FaceId>,
}

impl FrameFaceSeal {
    /// Commit the presented table after font realization.
    ///
    /// The table is the one this attempt published: it was moved into the
    /// presented state, not copied, so the published namespace is exactly the
    /// ids the layout transaction produced.  `seal` checks that no identity
    /// was added, removed or re-keyed after the hand-off.  Styling cannot
    /// diverge from a second copy because there is no second copy: the only
    /// writer between `install` and `seal` is `realize_frame_fonts`, whose
    /// enrichments mutate these same `Face` values.
    pub(crate) fn seal(
        self,
        presented: &FrameFaceMap,
        face_epoch: FaceEpoch,
    ) -> Result<FrameFaceArena, FrameFaceSealError> {
        let mut finalized: Vec<FaceId> = presented.keys().copied().collect();
        finalized.sort_unstable();
        if self.published != finalized {
            return Err(FrameFaceSealError::FaceSetChanged {
                published: self.published,
                finalized,
            });
        }
        if let Some((table_id, face_id)) = presented
            .iter()
            .find_map(|(table_id, face)| (*table_id != face.id).then_some((*table_id, face.id)))
        {
            return Err(FrameFaceSealError::MismatchedFaceId { table_id, face_id });
        }
        Ok(FrameFaceArena {
            owner: Arc::clone(&self.state.owner),
            snapshot: Arc::new(FrameFaceSnapshot),
            generation: self.state.generation.next(),
            // The presented state and the committed arena are two owners; the
            // remaining whole-table copy is this shared commit.
            faces: Arc::new(presented.clone()),
            realized: FrameFaceAttempt::fold_realized(&self.state),
            resolved_memo: FrameFaceAttempt::fold_resolved_memo(&self.state),
            next_face_id: self.state.next_face_id,
            face_epoch,
        })
    }
}

impl FrameFaceTableAttempt {
    /// A cloneable layout handle for this attempt.  It can publish, bind and
    /// read faces, but cannot yield the table.
    pub(crate) fn handle(&self) -> FrameFaceAttempt {
        FrameFaceAttempt {
            state: Rc::clone(&self.state),
        }
    }

    /// Consume the owner and move its table to the publication step.
    ///
    /// `Rc::try_unwrap` is the proof that no layout handle survives: the
    /// attempt state is then owned by value, so no `FrameFaceAttempt` can
    /// read the emptied table.  On failure nothing is moved and the frame is
    /// rejected instead of exposing a handed-off-but-empty attempt.
    pub(crate) fn into_publication(self) -> Result<FrameFacePublication, FrameFaceStillShared> {
        // The owner is one of the strong handles; report only the layout
        // handles that are keeping it shared.
        let handles = Rc::strong_count(&self.state).saturating_sub(1);
        let state = Rc::try_unwrap(self.state).map_err(|_| FrameFaceStillShared { handles })?;
        let mut state = state.into_inner();
        let faces = std::mem::take(&mut state.faces);
        Ok(FrameFacePublication { state, faces })
    }
}

/// Immutable face identities accompanying prepared rows.
///
/// Unlike an arena or attempt this grants no identity-allocation or publication
/// capability. It can cross a worker boundary; admission still validates its
/// namespace and every referenced identity against the current arena. The
/// caller must separately validate the rows' complete layout-input key.
#[derive(Clone, Debug)]
pub(crate) struct PreparedFaceSnapshot {
    owner: Arc<FrameFaceOwner>,
    faces: Arc<FrameFaceMap>,
}

/// An immutable realization owned by exactly one speculative attempt.
/// Only the arena can construct this value. Raw IDs are extracted after
/// checking the destination attempt, never paired with replacement styling.
#[derive(Clone, Debug)]
pub(crate) struct RealizedFrameFace {
    face: Face,
    attempt: Weak<RefCell<FrameFaceAttemptState>>,
}

impl RealizedFrameFace {
    pub(crate) fn face(&self) -> &Face {
        &self.face
    }
}

/// Resolved source attributes bound to a validated rendering identity.
/// Measurement may enrich metrics, but cannot select a different face ID.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedFrameFace {
    resolved: crate::neovm_bridge::ResolvedFace,
    realized: RealizedFrameFace,
}

impl ResolvedFrameFace {
    pub(crate) fn into_resolved(self) -> crate::neovm_bridge::ResolvedFace {
        self.resolved
    }
    pub(crate) fn face_id(&self) -> FaceId {
        self.realized.face.id
    }
    pub(crate) fn resolved(&self) -> &crate::neovm_bridge::ResolvedFace {
        &self.resolved
    }
    pub(crate) fn realized(
        &self,
        metrics: Option<crate::font::metrics::FontMetrics>,
    ) -> RealizedFrameFace {
        let mut face = self.realized.clone();
        if let Some(metrics) = metrics {
            face.face.font_ascent = metrics.ascent as i32;
            face.face.font_descent = metrics.descent.max(0.0).ceil() as i32;
        }
        face
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameFaceUseError {
    ForeignAttempt,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FrameFacePublicationError {
    ForeignAttempt,
    Conflict(FrameFaceConflict),
}

#[derive(Debug)]
struct FrameFaceAttemptState {
    owner: Arc<FrameFaceOwner>,
    base_snapshot: Arc<FrameFaceSnapshot>,
    generation: FrameFaceGeneration,
    next_face_id: u32,
    faces: FrameFaceMap,
    /// Read-only view of the arena's persistent identity map.
    realized: Arc<RealizedIdentityMap>,
    /// Identities first realized in this attempt; folded into the arena at
    /// seal. Also serves publish-time verification: an id handed out for an
    /// identity must only ever be published with that identity.
    fresh_realized: RealizedIdentityMap,
    /// ResolvedFace -> id fast path in front of the canonical identity map.
    /// Checkpoints re-resolve the same handful of faces hundreds of times
    /// per pass; without this memo each hit re-built the canonical protocol
    /// face just to look it up (+5% per keystroke, measured). The arena's
    /// persistent memo answers probes for identities sealed in earlier
    /// attempts; `fresh_resolved_memo` caches this attempt's own entries
    /// until they fold at seal. Full-struct equality OVER-discriminates
    /// relative to the identity projection (face_id / metric fields differ),
    /// which is safe: a memo miss falls through to the canonical path and
    /// still returns the stable id — never a wrong one.
    resolved_memo: Arc<ResolvedFaceMemo>,
    fresh_resolved_memo: ResolvedFaceMemo,
}

/// Allocation identity, deliberately independent of the presentation counter.
#[derive(Debug)]
struct FrameFaceOwner;

/// Distinguishes sibling attempts sealed from the same predecessor.
#[derive(Debug)]
struct FrameFaceSnapshot;

impl FrameFaceAttemptState {
    fn validate_face(&self, face: &Face) -> Result<(), FrameFaceConflict> {
        let face_id = face.id;
        if self.faces.get(&face_id) == Some(face) {
            return Ok(());
        }
        // Dynamic IDs are content-bound even before first publication.
        // Use the content index on the hot path. Only a mismatched/imported ID
        // needs a reverse search to produce a useful conflict diagnostic.
        if face_id.get() >= BasicFaceId::SENTINEL {
            let hash = face_identity_hash(face);
            let matched = realized_identity_lookup(&self.fresh_realized, hash, face)
                .or_else(|| realized_identity_lookup(&self.realized, hash, face));
            if matched != Some(face_id)
                && let Some((bound, _)) = self
                    .fresh_realized
                    .values()
                    .flatten()
                    .chain(self.realized.values().flatten())
                    .find(|(_, id)| *id == face_id)
            {
                let mut existing = bound.clone();
                existing.id = face_id;
                return Err(FrameFaceConflict {
                    face_id,
                    existing: Box::new(existing),
                    replacement: Box::new(face.clone()),
                });
            }
        }
        if let Some(existing) = self.faces.get(&face_id) {
            if !compatible_realization(existing, face) {
                return Err(FrameFaceConflict {
                    face_id,
                    existing: Box::new(existing.clone()),
                    replacement: Box::new(face.clone()),
                });
            }
        }
        Ok(())
    }

    fn reserve_dynamic_face(&mut self) -> FaceId {
        while self.faces.contains_key(&FaceId::new(self.next_face_id)) {
            self.next_face_id = self.next_face_id.saturating_add(1);
        }
        let face_id = FaceId::new(self.next_face_id);
        self.next_face_id = self.next_face_id.saturating_add(1);
        face_id
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FrameFaceConflict {
    pub(crate) face_id: FaceId,
    pub(crate) existing: Box<Face>,
    pub(crate) replacement: Box<Face>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameFaceReuseError {
    ForeignArena,
    ForeignSnapshot,
    AttemptGenerationMismatch {
        attempt: FrameFaceGeneration,
        source: FrameFaceGeneration,
    },
    StaleGeneration {
        retained: FrameFaceGeneration,
        current: FrameFaceGeneration,
    },
    MissingFace(FaceId),
    ConflictingFace(FaceId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FrameFaceSealError {
    /// The presented table no longer holds exactly the identities the layout
    /// transaction published.
    FaceSetChanged {
        published: Vec<FaceId>,
        finalized: Vec<FaceId>,
    },
    /// A presented entry is stored under a key that is not its own face id.
    MismatchedFaceId { table_id: FaceId, face_id: FaceId },
}

impl Default for FrameFaceArena {
    fn default() -> Self {
        Self {
            owner: Arc::new(FrameFaceOwner),
            snapshot: Arc::new(FrameFaceSnapshot),
            generation: FrameFaceGeneration(1),
            faces: Arc::new(HashMap::default()),
            realized: Arc::new(HashMap::default()),
            resolved_memo: Arc::new(HashMap::default()),
            next_face_id: BasicFaceId::SENTINEL,
            face_epoch: FaceEpoch::new(0),
        }
    }
}

impl FrameFaceArena {
    /// Reserve identities for evaluator-resolved off-screen work without
    /// publishing its speculative metrics into the current presentation.
    pub(crate) fn reserve_prepared(
        &mut self,
        attempt: &FrameFaceAttempt,
    ) -> Result<PreparedFaceSnapshot, FrameFaceReuseError> {
        let state = attempt.state.borrow();
        if !Arc::ptr_eq(&self.owner, &state.owner) {
            return Err(FrameFaceReuseError::ForeignArena);
        }
        if !Arc::ptr_eq(&self.snapshot, &state.base_snapshot) {
            return Err(FrameFaceReuseError::ForeignSnapshot);
        }
        self.realized = FrameFaceAttempt::fold_realized(&state);
        self.next_face_id = self.next_face_id.max(state.next_face_id);
        // A second attempt based on the old allocator may have minted the
        // same IDs for different faces. Reservations serialize that lineage
        // without changing the displayed face table or its generation.
        self.snapshot = Arc::new(FrameFaceSnapshot);
        Ok(PreparedFaceSnapshot {
            owner: Arc::clone(&self.owner),
            faces: Arc::new(state.faces.clone()),
        })
    }

    /// Resume private row acquisition after an unrelated presentation was
    /// sealed. Every previously reserved ID must still name the same face.
    pub(crate) fn resume_prepared(
        &self,
        prepared: &PreparedFaceSnapshot,
    ) -> Result<FrameFaceAttempt, FrameFaceReuseError> {
        let mut attempt = self.begin_attempt();
        attempt.admit_prepared(prepared.faces.keys().copied(), prepared, self)?;
        Ok(attempt)
    }

    pub(crate) fn prepared_snapshot(&self) -> PreparedFaceSnapshot {
        PreparedFaceSnapshot {
            owner: Arc::clone(&self.owner),
            faces: Arc::clone(&self.faces),
        }
    }

    /// Join current retained rows and private computed rows without changing
    /// either namespace. The normal attempt admission still validates every
    /// referenced identity before this combined snapshot can be replayed.
    pub(crate) fn prepared_with_retained(
        &self,
        prepared: &PreparedFaceSnapshot,
    ) -> Result<PreparedFaceSnapshot, FrameFaceReuseError> {
        if !Arc::ptr_eq(&self.owner, &prepared.owner) {
            return Err(FrameFaceReuseError::ForeignArena);
        }
        let mut faces = self.faces.as_ref().clone();
        for (id, face) in prepared.faces.iter() {
            if faces.get(id).is_some_and(|current| current != face) {
                return Err(FrameFaceReuseError::ConflictingFace(*id));
            }
            faces.insert(*id, face.clone());
        }
        Ok(PreparedFaceSnapshot {
            owner: Arc::clone(&self.owner),
            faces: Arc::new(faces),
        })
    }

    pub(crate) fn generation(&self) -> FrameFaceGeneration {
        self.generation
    }

    /// GNU `face_change_count` the committed table was realized under. The
    /// layout engine replaces the arena when this differs from the evaluator's
    /// current count, which is exactly GNU's `face_change` retirement
    /// (src/xfaces.c:3098-3105, `free_realized_faces` :4889-4954).
    pub(crate) fn face_epoch(&self) -> FaceEpoch {
        self.face_epoch
    }

    /// Retire every realized face and memo in this lineage, keeping the owner
    /// and advancing the generation so no retained row or prepared snapshot
    /// from the old definitions can be admitted (`StaleGeneration` /
    /// `ForeignSnapshot` typed errors, never a content conflict).
    pub(crate) fn invalidate_for_face_definition_change(&self, face_epoch: FaceEpoch) -> Self {
        Self {
            owner: Arc::clone(&self.owner),
            snapshot: Arc::new(FrameFaceSnapshot),
            generation: self.generation.next(),
            faces: Arc::new(HashMap::default()),
            realized: Arc::new(HashMap::default()),
            resolved_memo: Arc::new(HashMap::default()),
            next_face_id: BasicFaceId::SENTINEL,
            face_epoch,
        }
    }

    fn new_attempt_state(&self) -> FrameFaceAttemptState {
        FrameFaceAttemptState {
            owner: Arc::clone(&self.owner),
            base_snapshot: Arc::clone(&self.snapshot),
            generation: self.generation,
            next_face_id: self.next_face_id.max(BasicFaceId::SENTINEL),
            faces: HashMap::default(),
            realized: Arc::clone(&self.realized),
            fresh_realized: HashMap::default(),
            resolved_memo: Arc::clone(&self.resolved_memo),
            fresh_resolved_memo: HashMap::default(),
        }
    }

    pub(crate) fn begin_attempt(&self) -> FrameFaceAttempt {
        FrameFaceAttempt {
            state: Rc::new(RefCell::new(self.new_attempt_state())),
        }
    }

    /// Begin an attempt whose table an output owner can hand off by value.
    pub(crate) fn begin_table_attempt(&self) -> FrameFaceTableAttempt {
        FrameFaceTableAttempt {
            state: Rc::new(RefCell::new(self.new_attempt_state())),
        }
    }

    #[cfg(test)]
    pub(crate) fn invalidate(&self) -> Self {
        Self {
            owner: Arc::clone(&self.owner),
            snapshot: Arc::new(FrameFaceSnapshot),
            generation: self.generation.next(),
            faces: Arc::new(HashMap::default()),
            realized: Arc::new(HashMap::default()),
            resolved_memo: Arc::new(HashMap::default()),
            next_face_id: BasicFaceId::SENTINEL,
            face_epoch: self.face_epoch,
        }
    }
}

impl FrameFaceAttempt {
    /// Checked admission of an existing resolver identity; no output is
    /// published. Resolved bindings are constructed only by the arena.
    pub(crate) fn bind_resolved_face(
        &self,
        id: FaceId,
        resolved: crate::neovm_bridge::ResolvedFace,
    ) -> Result<ResolvedFrameFace, FrameFaceConflict> {
        let face = crate::display_row::face_state::resolved_display_row_face(id, &resolved, None)
            .render_face();
        let realized = self.prepare_face(face)?;
        Ok(ResolvedFrameFace { resolved, realized })
    }

    /// Bind directly into caller-owned storage without transporting a large
    /// success result. A failed identity check leaves the prior output intact.
    pub(crate) fn bind_resolved_face_into(
        &self,
        id: FaceId,
        resolved: crate::neovm_bridge::ResolvedFace,
        output: &mut Option<ResolvedFrameFace>,
    ) -> Result<(), FrameFaceConflict> {
        let face = crate::display_row::face_state::resolved_display_row_face(id, &resolved, None)
            .render_face();
        self.state.borrow().validate_face(&face)?;
        *output = Some(ResolvedFrameFace {
            resolved,
            realized: RealizedFrameFace {
                face,
                attempt: Rc::downgrade(&self.state),
            },
        });
        Ok(())
    }

    /// Validate a row's realization without publishing speculative metrics.
    /// Dropping this handle leaves the published face table unchanged.
    pub(crate) fn prepare_face(&self, face: Face) -> Result<RealizedFrameFace, FrameFaceConflict> {
        self.state.borrow().validate_face(&face)?;
        Ok(RealizedFrameFace {
            face,
            attempt: Rc::downgrade(&self.state),
        })
    }

    /// Append a checked realization directly to a row's prepared output. A
    /// rejected face leaves both that output and the published table unchanged.
    /// The caller keeps encounter order without transporting a large success
    /// payload through a separate `Result<RealizedFrameFace, _>`.
    #[inline]
    pub(crate) fn prepare_face_into_output(
        &self,
        face: Face,
        output: &mut Vec<RealizedFrameFace>,
    ) -> Result<(), FrameFaceConflict> {
        self.state.borrow().validate_face(&face)?;
        output.push(RealizedFrameFace {
            face,
            attempt: Rc::downgrade(&self.state),
        });
        Ok(())
    }

    pub(crate) fn publish_face(
        &mut self,
        face: &RealizedFrameFace,
    ) -> Result<FaceId, FrameFacePublicationError> {
        self.use_face(face)
            .map_err(|_| FrameFacePublicationError::ForeignAttempt)?;
        self.publish_ref(&face.face)
            .map_err(FrameFacePublicationError::Conflict)
    }

    /// Resolve identity and register rendering in the same operation.
    pub(crate) fn intern_resolved_face(
        &mut self,
        resolved: &crate::neovm_bridge::ResolvedFace,
    ) -> Result<RealizedFrameFace, FrameFaceConflict> {
        let id = crate::display_row::face_state::stable_face_id_for_resolved(self, resolved);
        let face = crate::display_row::face_state::resolved_display_row_face(id, resolved, None)
            .render_face();
        self.import_face(face)
    }

    /// Checked import for row realizations and protocol fixtures that already
    /// carry IDs. New dynamic resolution should use `intern_resolved_face`.
    pub(crate) fn import_face(
        &mut self,
        face: Face,
    ) -> Result<RealizedFrameFace, FrameFaceConflict> {
        // Validate through the value that is about to be retained: an
        // identity hit in the frame face table must not copy the face at all.
        self.publish_ref(&face)?;
        Ok(RealizedFrameFace {
            face,
            attempt: Rc::downgrade(&self.state),
        })
    }

    pub(crate) fn use_face(&self, face: &RealizedFrameFace) -> Result<FaceId, FrameFaceUseError> {
        if !Weak::ptr_eq(&face.attempt, &Rc::downgrade(&self.state)) {
            return Err(FrameFaceUseError::ForeignAttempt);
        }
        Ok(face.face.id)
    }

    #[cfg(test)]
    pub(crate) fn for_test_with_next_id(next_face_id: u32) -> Self {
        Self {
            state: Rc::new(RefCell::new(FrameFaceAttemptState {
                owner: Arc::new(FrameFaceOwner),
                base_snapshot: Arc::new(FrameFaceSnapshot),
                generation: FrameFaceGeneration(1),
                next_face_id: next_face_id.max(BasicFaceId::SENTINEL),
                faces: HashMap::default(),
                realized: Arc::new(HashMap::default()),
                fresh_realized: HashMap::default(),
                resolved_memo: Arc::new(HashMap::default()),
                fresh_resolved_memo: HashMap::default(),
            })),
        }
    }

    #[cfg(test)]
    pub(crate) fn reserve_dynamic_face(&mut self) -> FaceId {
        let mut state = self.state.borrow_mut();
        state.reserve_dynamic_face()
    }

    /// Content-addressed dynamic face id: the same realization identity gets
    /// the same id in every attempt of this arena lineage, regardless of the
    /// order faces are encountered in. `identity` must already be the
    /// canonical projection (see [`face_realization_identity`]).
    pub(crate) fn stable_face_id(&mut self, identity: Face) -> FaceId {
        debug_assert_eq!(
            identity,
            face_realization_identity(&identity),
            "stable_face_id takes the canonical realization projection"
        );
        let mut state = self.state.borrow_mut();
        let hash = face_identity_hash(&identity);
        if let Some(id) = realized_identity_lookup(&state.fresh_realized, hash, &identity)
            .or_else(|| realized_identity_lookup(&state.realized, hash, &identity))
        {
            return id;
        }
        let face_id = state.reserve_dynamic_face();
        state
            .fresh_realized
            .entry(hash)
            .or_default()
            .push((identity, face_id));
        face_id
    }

    /// [`Self::stable_face_id`] with a memo keyed by the resolved face —
    /// this attempt's entries plus the arena's persistent memo folded at
    /// seal — so repeated checkpoints of the same face skip building the
    /// canonical protocol face. A memo entry is matched first by interned
    /// source identity — the same key GNU's `lookup_face` uses
    /// (src/xfaces.c:5064-5075) — falling back to full equality for
    /// content-equal sources from distinct allocations. `canonical` is
    /// invoked only on a memo miss and must produce the canonical identity
    /// projection of `face`.
    pub(crate) fn face_id_for_resolved(
        &mut self,
        face: &crate::neovm_bridge::ResolvedFace,
        canonical: impl FnOnce() -> Face,
    ) -> FaceId {
        let route = resolved_face_route_hash(face);
        {
            let state = self.state.borrow();
            let hit = |memo: &ResolvedFaceMemo| {
                memo.get(&route).and_then(|bucket| {
                    bucket.iter().find_map(|(memo_face, id)| {
                        (face.shares_resolved_identity(memo_face) || memo_face == face)
                            .then_some(*id)
                    })
                })
            };
            if let Some(id) = hit(&state.resolved_memo).or_else(|| hit(&state.fresh_resolved_memo))
            {
                return id;
            }
        }
        let face_id = self.stable_face_id(canonical());
        self.state
            .borrow_mut()
            .fresh_resolved_memo
            .entry(route)
            .or_default()
            .push((face.clone(), face_id));
        face_id
    }

    pub(crate) fn reserve_after(&mut self, face_id: FaceId) {
        let mut state = self.state.borrow_mut();
        state.next_face_id = state.next_face_id.max(face_id.get().saturating_add(1));
    }

    #[cfg(test)]
    pub(crate) fn next_face_id_for_test(&self) -> u32 {
        self.state.borrow().next_face_id
    }

    pub(crate) fn admit_retained(
        &mut self,
        generation: FrameFaceGeneration,
        face_ids: impl IntoIterator<Item = FaceId>,
        arena: &FrameFaceArena,
    ) -> Result<(), FrameFaceReuseError> {
        if !Arc::ptr_eq(&self.state.borrow().owner, &arena.owner) {
            return Err(FrameFaceReuseError::ForeignArena);
        }
        let attempt_generation = self.state.borrow().generation;
        if attempt_generation != arena.generation {
            return Err(FrameFaceReuseError::AttemptGenerationMismatch {
                attempt: attempt_generation,
                source: arena.generation,
            });
        }
        if !Arc::ptr_eq(&self.state.borrow().base_snapshot, &arena.snapshot) {
            return Err(FrameFaceReuseError::ForeignSnapshot);
        }
        if generation != arena.generation {
            return Err(FrameFaceReuseError::StaleGeneration {
                retained: generation,
                current: arena.generation,
            });
        }
        let face_ids: Vec<FaceId> = face_ids.into_iter().collect();
        for face_id in &face_ids {
            if !arena.faces.contains_key(face_id) {
                return Err(FrameFaceReuseError::MissingFace(*face_id));
            }
        }
        {
            let state = self.state.borrow();
            for face_id in &face_ids {
                if state
                    .faces
                    .get(face_id)
                    .is_some_and(|existing| existing != &arena.faces[face_id])
                {
                    return Err(FrameFaceReuseError::ConflictingFace(*face_id));
                }
            }
        }
        let mut state = self.state.borrow_mut();
        // The admitted set is known here; reserve it once instead of walking
        // the table's growth ladder on this per-frame path.
        state.faces.reserve(face_ids.len());
        for face_id in face_ids {
            state.faces.insert(face_id, arena.faces[&face_id].clone());
        }
        Ok(())
    }

    /// Import older prepared content only when its IDs still name the same
    /// realizations in the current committed namespace. The caller separately
    /// checks the full layout key (including font-selection invalidation).
    pub(crate) fn admit_prepared(
        &mut self,
        face_ids: impl IntoIterator<Item = FaceId>,
        source: &PreparedFaceSnapshot,
        current: &FrameFaceArena,
    ) -> Result<(), FrameFaceReuseError> {
        let mut state = self.state.borrow_mut();
        if !Arc::ptr_eq(&state.owner, &source.owner) || !Arc::ptr_eq(&state.owner, &current.owner) {
            return Err(FrameFaceReuseError::ForeignArena);
        }
        if !Arc::ptr_eq(&state.base_snapshot, &current.snapshot) {
            return Err(FrameFaceReuseError::ForeignSnapshot);
        }
        // Rows repeat face IDs for every glyph. Validate each distinct ID
        // once, retaining encounter order and the all-or-nothing admission.
        let mut seen = rustc_hash::FxHashSet::default();
        let ids: Vec<_> = face_ids.into_iter().filter(|id| seen.insert(*id)).collect();
        for id in &ids {
            let face = source
                .faces
                .get(id)
                .ok_or(FrameFaceReuseError::MissingFace(*id))?;
            if let Some(now) = current.faces.get(id) {
                if now != face {
                    return Err(FrameFaceReuseError::ConflictingFace(*id));
                }
            } else {
                if id.get() < BasicFaceId::SENTINEL
                    || realized_identity_lookup(&current.realized, face_identity_hash(face), face)
                        != Some(*id)
                {
                    return Err(FrameFaceReuseError::MissingFace(*id));
                }
            }
            if state.faces.get(id).is_some_and(|now| now != face) {
                return Err(FrameFaceReuseError::ConflictingFace(*id));
            }
        }
        for id in ids {
            state
                .faces
                .entry(id)
                .or_insert_with(|| source.faces[&id].clone());
        }
        Ok(())
    }

    /// Validate a publication through the already-realized face instead of a
    /// speculative copy: the frame face cache is content-addressed, so a face
    /// the table already holds is an identity hit, not a new value.  GNU's
    /// `realize_face` (src/xfaces.c) likewise looks an equal realization up
    /// and returns its id without copying the face.
    fn publish_ref(&mut self, face: &Face) -> Result<FaceId, FrameFaceConflict> {
        let mut state = self.state.borrow_mut();
        let face_id = face.id;
        if state.faces.get(&face_id) == Some(face) {
            return Ok(face_id);
        }
        state.validate_face(face)?;
        match state.faces.entry(face_id) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(face.clone());
            }
            std::collections::hash_map::Entry::Occupied(mut slot) if slot.get() != face => {
                if !merge_compatible_realization(slot.get_mut(), face) {
                    return Err(FrameFaceConflict {
                        face_id,
                        existing: Box::new(slot.get().clone()),
                        replacement: Box::new(face.clone()),
                    });
                }
            }
            std::collections::hash_map::Entry::Occupied(_) => {}
        }
        Ok(face_id)
    }

    pub(crate) fn faces(&self) -> FrameFaceMap {
        self.state.borrow().faces.clone()
    }

    pub(crate) fn face(&self, face_id: FaceId) -> Option<Face> {
        self.state.borrow().faces.get(&face_id).cloned()
    }

    pub(crate) fn face_vertical_metrics(&self, face_id: FaceId) -> Option<(f32, f32)> {
        self.state.borrow().faces.get(&face_id).and_then(|face| {
            let ascent = face.font_ascent.max(0) as f32;
            let height = ascent + face.font_descent.max(0) as f32;
            (height > 0.0).then_some((height, ascent))
        })
    }

    #[cfg(test)]
    pub(crate) fn commit(&self) -> FrameFaceArena {
        let state = self.state.borrow();
        FrameFaceArena {
            owner: Arc::clone(&state.owner),
            snapshot: Arc::new(FrameFaceSnapshot),
            generation: state.generation.next(),
            faces: Arc::new(state.faces.clone()),
            realized: Self::fold_realized(&state),
            resolved_memo: Self::fold_resolved_memo(&state),
            next_face_id: state.next_face_id,
            face_epoch: FaceEpoch::new(0),
        }
    }

    /// The arena's persistent identity map plus this attempt's fresh
    /// realizations. Steady state (no new identities) shares the existing
    /// Arc without copying.
    fn fold_realized(state: &FrameFaceAttemptState) -> Arc<RealizedIdentityMap> {
        if state.fresh_realized.is_empty() {
            return Arc::clone(&state.realized);
        }
        let mut folded: RealizedIdentityMap = (*state.realized).clone();
        for (hash, entries) in &state.fresh_realized {
            folded.entry(*hash).or_default().extend(entries.clone());
        }
        if folded.values().map(Vec::len).sum::<usize>() > REALIZED_IDENTITY_CAP {
            folded = state.fresh_realized.clone();
        }
        Arc::new(folded)
    }

    /// The persistent resolved memo plus this attempt's fresh entries,
    /// mirroring `fold_realized`. Steady state shares the existing Arc.
    fn fold_resolved_memo(state: &FrameFaceAttemptState) -> Arc<ResolvedFaceMemo> {
        if state.fresh_resolved_memo.is_empty() {
            return Arc::clone(&state.resolved_memo);
        }
        let mut folded: ResolvedFaceMemo = (*state.resolved_memo).clone();
        for (route, entries) in &state.fresh_resolved_memo {
            folded.entry(*route).or_default().extend(entries.clone());
        }
        if folded.values().map(Vec::len).sum::<usize>() > RESOLVED_MEMO_CAP {
            folded = state.fresh_resolved_memo.clone();
        }
        Arc::new(folded)
    }
}

fn compatible_realization(existing: &Face, replacement: &Face) -> bool {
    if existing.id != replacement.id || !same_face_realization(existing, replacement) {
        return false;
    }

    if existing
        .font_file_path
        .as_ref()
        .zip(replacement.font_file_path.as_ref())
        .is_some_and(|(existing, replacement)| existing != replacement)
        || existing
            .default_resolved_font_id
            .as_ref()
            .zip(replacement.default_resolved_font_id.as_ref())
            .is_some_and(|(existing, replacement)| existing != replacement)
    {
        return false;
    }

    true
}

fn merge_compatible_realization(existing: &mut Face, replacement: &Face) -> bool {
    if !compatible_realization(existing, replacement) {
        return false;
    }

    if replacement.font_ascent != 0 {
        existing.font_ascent = replacement.font_ascent;
    }
    if replacement.font_descent != 0 {
        existing.font_descent = replacement.font_descent;
    }
    if replacement.font_file_path.is_some() {
        existing
            .font_file_path
            .clone_from(&replacement.font_file_path);
    }
    if replacement.default_resolved_font_id.is_some() {
        existing
            .default_resolved_font_id
            .clone_from(&replacement.default_resolved_font_id);
    }
    true
}

#[cfg(test)]
#[path = "frame_face_arena/tests/frame_face_arena_test.rs"]
mod tests;
