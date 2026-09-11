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

/// The realization identity of a face: every field that participates in what
/// the face LOOKS like, with enrichment stripped. Font realization may fill
/// in metrics, the exact font file, and the resolved font handle after row
/// construction (see [`FrameFaceAttempt::seal`]); two realizations that
/// differ only in those fields are the same face. This is the single source
/// of truth for that projection — both the content-addressed id map and
/// [`merge_compatible_realization`] are built on it.
pub(crate) fn face_realization_identity(face: &Face) -> Face {
    let mut identity = face.clone();
    identity.id = FaceId::new(0);
    identity.font_ascent = 0;
    identity.font_descent = 0;
    identity.font_file_path = None;
    identity.default_resolved_font_id = None;
    identity
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
    if let Some(fontset_base_family) = &identity.fontset_base_family {
        hasher.write(fontset_base_family.as_bytes());
    }
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
    hasher.write(face.fontset_base_family.as_bytes());
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
        .find_map(|(face, id)| (face == identity).then_some(*id))
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
    /// Persistent monotonic id allocator. Never rewinds within an arena
    /// lineage, so a stable id can never be re-minted for different content.
    next_face_id: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct FrameFaceAttempt {
    state: Rc<RefCell<FrameFaceAttemptState>>,
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
    /// Attempt-local ResolvedFace -> id fast path in front of the canonical
    /// identity map. Checkpoints re-resolve the same handful of faces
    /// hundreds of times per pass; without this memo each hit re-built the
    /// canonical protocol face just to look it up (+5% per keystroke,
    /// measured). Full-struct equality OVER-discriminates relative to the
    /// identity projection (face_id / metric fields differ), which is safe:
    /// a memo miss falls through to the canonical path and still returns the
    /// stable id — never a wrong one.
    resolved_memo: HashMap<u64, Vec<(crate::neovm_bridge::ResolvedFace, FaceId)>>,
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
            let identity = face_realization_identity(&face);
            let hash = face_identity_hash(&identity);
            let matched = realized_identity_lookup(&self.fresh_realized, hash, &identity)
                .or_else(|| realized_identity_lookup(&self.realized, hash, &identity));
            if matched != Some(face_id) {
                if let Some((bound, _)) = self
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
        }
        if let Some(existing) = self.faces.get(&face_id) {
            let mut merged = existing.clone();
            if !merge_compatible_realization(&mut merged, face) {
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
    ChangedRealization(FaceId),
    ChangedFontBinding(FaceId),
    FaceSetChanged {
        published: Vec<FaceId>,
        finalized: Vec<FaceId>,
    },
    MismatchedFaceId {
        table_id: FaceId,
        face_id: FaceId,
    },
}

impl Default for FrameFaceArena {
    fn default() -> Self {
        Self {
            owner: Arc::new(FrameFaceOwner),
            snapshot: Arc::new(FrameFaceSnapshot),
            generation: FrameFaceGeneration(1),
            faces: Arc::new(HashMap::default()),
            realized: Arc::new(HashMap::default()),
            next_face_id: BasicFaceId::SENTINEL,
        }
    }
}

impl FrameFaceArena {
    pub(crate) fn generation(&self) -> FrameFaceGeneration {
        self.generation
    }

    pub(crate) fn begin_attempt(&self) -> FrameFaceAttempt {
        FrameFaceAttempt {
            state: Rc::new(RefCell::new(FrameFaceAttemptState {
                owner: Arc::clone(&self.owner),
                base_snapshot: Arc::clone(&self.snapshot),
                generation: self.generation,
                next_face_id: self.next_face_id.max(BasicFaceId::SENTINEL),
                faces: HashMap::default(),
                realized: Arc::clone(&self.realized),
                fresh_realized: HashMap::default(),
                resolved_memo: HashMap::default(),
            })),
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
            next_face_id: BasicFaceId::SENTINEL,
        }
    }
}

impl FrameFaceAttempt {
    /// Checked admission of an existing resolver identity; no output is
    /// published. This is the sole constructor for a resolved binding.
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

    /// Validate a row's realization without publishing speculative metrics.
    /// Dropping this handle leaves the published face table unchanged.
    pub(crate) fn prepare_face(&self, face: Face) -> Result<RealizedFrameFace, FrameFaceConflict> {
        self.state.borrow().validate_face(&face)?;
        Ok(RealizedFrameFace {
            face,
            attempt: Rc::downgrade(&self.state),
        })
    }

    pub(crate) fn publish_face(
        &mut self,
        face: &RealizedFrameFace,
    ) -> Result<FaceId, FrameFacePublicationError> {
        self.use_face(face)
            .map_err(|_| FrameFacePublicationError::ForeignAttempt)?;
        self.publish(face.face.clone())
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
        self.publish(face.clone())?;
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
                resolved_memo: HashMap::default(),
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

    /// [`Self::stable_face_id`] with an attempt-local memo keyed by the
    /// resolved face, so repeated checkpoints of the same face skip building
    /// the canonical protocol face. `canonical` is invoked only on a memo
    /// miss and must produce the canonical identity projection of `face`.
    pub(crate) fn face_id_for_resolved(
        &mut self,
        face: &crate::neovm_bridge::ResolvedFace,
        canonical: impl FnOnce() -> Face,
    ) -> FaceId {
        let route = resolved_face_route_hash(face);
        {
            let state = self.state.borrow();
            if let Some(id) = state.resolved_memo.get(&route).and_then(|bucket| {
                bucket
                    .iter()
                    .find_map(|(memo_face, id)| (memo_face == face).then_some(*id))
            }) {
                return id;
            }
        }
        let face_id = self.stable_face_id(canonical());
        self.state
            .borrow_mut()
            .resolved_memo
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
        for face_id in face_ids {
            state.faces.insert(face_id, arena.faces[&face_id].clone());
        }
        Ok(())
    }

    fn publish(&mut self, face: Face) -> Result<FaceId, FrameFaceConflict> {
        let mut state = self.state.borrow_mut();
        let face_id = face.id;
        if state.faces.get(&face_id) == Some(&face) {
            return Ok(face_id);
        }
        state.validate_face(&face)?;
        match state.faces.entry(face_id) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(face);
            }
            std::collections::hash_map::Entry::Occupied(mut slot) if slot.get() != &face => {
                if !merge_compatible_realization(slot.get_mut(), &face) {
                    return Err(FrameFaceConflict {
                        face_id,
                        existing: Box::new(slot.get().clone()),
                        replacement: Box::new(face),
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
            next_face_id: state.next_face_id,
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

    /// Seal the exact renderer-facing table produced by the layout transaction.
    ///
    /// Font realization may enrich a published face with an exact font file or
    /// resolved-font handle after row construction. It may not add, remove, or
    /// re-key face identities.
    pub(crate) fn seal(
        &self,
        finalized_faces: FrameFaceMap,
    ) -> Result<FrameFaceArena, FrameFaceSealError> {
        let state = self.state.borrow();
        let mut published: Vec<FaceId> = state.faces.keys().copied().collect();
        let mut finalized: Vec<FaceId> = finalized_faces.keys().copied().collect();
        published.sort_unstable();
        finalized.sort_unstable();
        if published != finalized {
            return Err(FrameFaceSealError::FaceSetChanged {
                published,
                finalized,
            });
        }
        if let Some((table_id, face_id)) = finalized_faces
            .iter()
            .find_map(|(table_id, face)| (*table_id != face.id).then_some((*table_id, face.id)))
        {
            return Err(FrameFaceSealError::MismatchedFaceId { table_id, face_id });
        }
        for (id, finalized) in &finalized_faces {
            if face_realization_identity(&state.faces[id]) != face_realization_identity(finalized) {
                return Err(FrameFaceSealError::ChangedRealization(*id));
            }
            let published = &state.faces[id];
            if published
                .font_file_path
                .as_ref()
                .is_some_and(|path| finalized.font_file_path.as_ref() != Some(path))
                || published
                    .default_resolved_font_id
                    .as_ref()
                    .is_some_and(|font| finalized.default_resolved_font_id.as_ref() != Some(font))
            {
                return Err(FrameFaceSealError::ChangedFontBinding(*id));
            }
        }
        Ok(FrameFaceArena {
            owner: Arc::clone(&state.owner),
            snapshot: Arc::new(FrameFaceSnapshot),
            generation: state.generation.next(),
            faces: Arc::new(finalized_faces),
            realized: Self::fold_realized(&state),
            next_face_id: state.next_face_id,
        })
    }
}

fn merge_compatible_realization(existing: &mut Face, replacement: &Face) -> bool {
    let mut existing_identity = existing.clone();
    existing_identity.font_ascent = 0;
    existing_identity.font_descent = 0;
    existing_identity.font_file_path = None;
    existing_identity.default_resolved_font_id = None;
    let mut replacement_identity = replacement.clone();
    replacement_identity.font_ascent = 0;
    replacement_identity.font_descent = 0;
    replacement_identity.font_file_path = None;
    replacement_identity.default_resolved_font_id = None;
    if existing_identity != replacement_identity {
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
#[path = "frame_face_arena_test.rs"]
mod tests;
