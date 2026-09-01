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

use neomacs_display_protocol::face::{BasicFaceId, Face};
use neomacs_display_protocol::types::FaceId;
use rustc_hash::FxHasher;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hasher;
use std::rc::Rc;
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
    generation: FrameFaceGeneration,
    faces: Arc<HashMap<FaceId, Face>>,
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

#[derive(Debug)]
struct FrameFaceAttemptState {
    generation: FrameFaceGeneration,
    next_face_id: u32,
    faces: HashMap<FaceId, Face>,
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

impl FrameFaceAttemptState {
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
    StaleGeneration {
        retained: FrameFaceGeneration,
        current: FrameFaceGeneration,
    },
    MissingFace(FaceId),
    ConflictingFace(FaceId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FrameFaceSealError {
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
            generation: FrameFaceGeneration(1),
            faces: Arc::new(HashMap::new()),
            realized: Arc::new(HashMap::new()),
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
                generation: self.generation,
                next_face_id: self.next_face_id.max(BasicFaceId::SENTINEL),
                faces: HashMap::new(),
                realized: Arc::clone(&self.realized),
                fresh_realized: HashMap::new(),
                resolved_memo: HashMap::new(),
            })),
        }
    }

    #[cfg(test)]
    pub(crate) fn invalidate(&self) -> Self {
        Self {
            generation: self.generation.next(),
            faces: Arc::new(HashMap::new()),
            realized: Arc::new(HashMap::new()),
            next_face_id: BasicFaceId::SENTINEL,
        }
    }
}

impl FrameFaceAttempt {
    #[cfg(test)]
    pub(crate) fn for_test_with_next_id(next_face_id: u32) -> Self {
        Self {
            state: Rc::new(RefCell::new(FrameFaceAttemptState {
                generation: FrameFaceGeneration(1),
                next_face_id: next_face_id.max(BasicFaceId::SENTINEL),
                faces: HashMap::new(),
                realized: Arc::new(HashMap::new()),
                fresh_realized: HashMap::new(),
                resolved_memo: HashMap::new(),
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

    pub(crate) fn publish(&mut self, face: Face) -> Result<FaceId, FrameFaceConflict> {
        let mut state = self.state.borrow_mut();
        let face_id = face.id;
        // An id handed out by stable_face_id is bound to its realization
        // identity for the arena's lifetime; publishing different content
        // under it would silently corrupt the content-addressed map.
        #[cfg(debug_assertions)]
        {
            let identity = face_realization_identity(&face);
            for map in [&state.fresh_realized, &*state.realized] {
                for (bound_identity, bound_id) in
                    map.values().flatten().filter(|(_, id)| *id == face_id)
                {
                    debug_assert_eq!(
                        *bound_identity, identity,
                        "face id {bound_id:?} is content-bound; published face diverges from its realization identity"
                    );
                }
            }
        }
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

    pub(crate) fn faces(&self) -> HashMap<FaceId, Face> {
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
        finalized_faces: HashMap<FaceId, Face>,
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
        Ok(FrameFaceArena {
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
mod tests {
    use super::*;
    use neomacs_display_protocol::types::Color;

    fn identity_with_fg(pixel: u32) -> Face {
        let mut face = Face::new(FaceId::new(0));
        face.foreground = Color::from_pixel(pixel);
        face_realization_identity(&face)
    }

    #[test]
    fn stable_ids_survive_realization_order_across_attempts() {
        // The GNU face_cache property: the same realization identity keeps
        // its id across layout passes even when the passes encounter faces
        // in a different order. Without it, one extra early checkpoint
        // renumbered every later face and the renderer diffed dozens of
        // "modified" faces per keystroke.
        let red = identity_with_fg(0x00FF0000);
        let blue = identity_with_fg(0x000000FF);

        let arena = FrameFaceArena::default();
        let mut first = arena.begin_attempt();
        let red_id = first.stable_face_id(red.clone());
        let blue_id = first.stable_face_id(blue.clone());
        assert_ne!(red_id, blue_id);
        let mut red_face = red.clone();
        red_face.id = red_id;
        first.publish(red_face).expect("publish red");
        let mut blue_face = blue.clone();
        blue_face.id = blue_id;
        first.publish(blue_face).expect("publish blue");
        let sealed = first.commit();

        // Opposite realization order, same ids.
        let mut second = sealed.begin_attempt();
        assert_eq!(second.stable_face_id(blue.clone()), blue_id);
        assert_eq!(second.stable_face_id(red.clone()), red_id);

        // A never-seen identity gets a fresh id above every previous one.
        let green = identity_with_fg(0x0000FF00);
        let green_id = second.stable_face_id(green);
        assert!(green_id.get() > red_id.get().max(blue_id.get()));
    }

    #[test]
    fn stable_ids_ignore_enrichment_but_not_content() {
        // Metrics, the exact font file, and the resolved font handle are
        // filled in after row construction; they must not fork identity.
        let base = identity_with_fg(0x00123456);
        let mut enriched = base.clone();
        enriched.font_ascent = 12;
        enriched.font_descent = 3;
        enriched.font_file_path = Some("/tmp/font.ttf".to_owned());

        let arena = FrameFaceArena::default();
        let mut attempt = arena.begin_attempt();
        let id = attempt.stable_face_id(base.clone());
        assert_eq!(
            attempt.stable_face_id(face_realization_identity(&enriched)),
            id
        );

        // A genuinely different rendering is a different face.
        let mut bold = base.clone();
        bold.font_weight = 700;
        assert_ne!(attempt.stable_face_id(bold), id);
    }

    #[test]
    fn publishing_enriched_faces_under_stable_ids_merges_cleanly() {
        // The id key is computed pre-enrichment; the published face carries
        // metrics. publish() must accept that (merge_compatible_realization
        // treats enrichment as compatible) and the debug verification must
        // compare identities, not raw faces.
        let identity = identity_with_fg(0x00ABCDEF);
        let arena = FrameFaceArena::default();
        let mut attempt = arena.begin_attempt();
        let id = attempt.stable_face_id(identity.clone());

        let mut published = identity;
        published.id = id;
        published.font_ascent = 14;
        published.font_descent = 4;
        published.default_resolved_font_id =
            Some(neomacs_display_protocol::font::ResolvedFontId(7));
        attempt.publish(published).expect("enriched publish");
    }

    #[test]
    fn one_attempt_cannot_rebind_a_face_id_to_different_rendering() {
        let arena = FrameFaceArena::default();
        let mut attempt = arena.begin_attempt();
        let face_id = attempt.reserve_dynamic_face();

        let mut original = Face::new(face_id);
        original.foreground = Color::from_pixel(0x00112233);
        attempt
            .publish(original.clone())
            .expect("first publication");

        let mut replacement = Face::new(face_id);
        replacement.foreground = Color::from_pixel(0x00445566);
        assert!(
            attempt.publish(replacement).is_err(),
            "a frame face id is immutable once published"
        );
        assert_eq!(
            attempt.faces().get(&face_id),
            Some(&original),
            "rejected publication must preserve the original face"
        );
    }

    #[test]
    fn one_attempt_can_complete_missing_metrics_for_the_same_face() {
        let arena = FrameFaceArena::default();
        let mut attempt = arena.begin_attempt();
        let face_id = attempt.reserve_dynamic_face();
        let incomplete = Face::new(face_id);
        attempt
            .publish(incomplete)
            .expect("publish semantic face before measurement");

        let mut measured = Face::new(face_id);
        measured.font_ascent = 13;
        measured.font_descent = 5;
        attempt
            .publish(measured.clone())
            .expect("measurement may complete missing metrics");
        assert_eq!(attempt.face(face_id), Some(measured));
    }

    #[test]
    fn later_realization_replaces_metrics_without_clearing_exact_font_identity() {
        let arena = FrameFaceArena::default();
        let mut attempt = arena.begin_attempt();
        let face_id = attempt.reserve_dynamic_face();
        let mut earlier = Face::new(face_id);
        earlier.font_ascent = 7;
        earlier.font_descent = 3;
        earlier.font_file_path = Some("/fonts/exact.ttf".to_owned());
        attempt
            .publish(earlier)
            .expect("publish earlier realization");

        let mut later = Face::new(face_id);
        later.font_ascent = 4;
        later.font_descent = 2;
        attempt
            .publish(later)
            .expect("publish later realization of the same face");

        let realized = attempt.face(face_id).expect("realized face");
        assert_eq!((realized.font_ascent, realized.font_descent), (4, 2));
        assert_eq!(realized.font_file_path.as_deref(), Some("/fonts/exact.ttf"));
    }

    #[test]
    fn retained_faces_occupy_their_slots_before_fresh_allocation() {
        let arena = FrameFaceArena::default();
        let mut first = arena.begin_attempt();
        let retained_id = first.reserve_dynamic_face();
        let mut retained_face = Face::new(retained_id);
        retained_face.foreground = Color::from_pixel(0x00112233);
        first
            .publish(retained_face.clone())
            .expect("publish retained face");
        let committed = first.commit();

        let mut next = committed.begin_attempt();
        next.admit_retained(committed.generation, [retained_id], &committed)
            .expect("admit retained face");

        let fresh_id = next.reserve_dynamic_face();
        assert_ne!(
            fresh_id, retained_id,
            "fresh allocation must not alias an admitted retained face"
        );
        assert_eq!(next.faces().get(&retained_id), Some(&retained_face));
    }

    #[test]
    fn invalidated_arena_rejects_stale_retained_handles_before_admission() {
        let arena = FrameFaceArena::default();
        let mut first = arena.begin_attempt();
        let retained_id = first.reserve_dynamic_face();
        first
            .publish(Face::new(retained_id))
            .expect("publish retained face");
        let committed = first.commit();
        let stale_generation = committed.generation();
        let invalidated = committed.invalidate();
        let mut next = invalidated.begin_attempt();

        assert_eq!(
            next.admit_retained(stale_generation, [retained_id], &invalidated),
            Err(FrameFaceReuseError::StaleGeneration {
                retained: stale_generation,
                current: invalidated.generation(),
            })
        );
        assert!(
            next.faces().is_empty(),
            "failed admission must not partially publish retained faces"
        );
    }

    #[test]
    fn retained_admission_cannot_overwrite_an_attempt_publication() {
        let arena = FrameFaceArena::default();
        let mut first = arena.begin_attempt();
        let face_id = first.reserve_dynamic_face();
        let mut retained = Face::new(face_id);
        retained.foreground = Color::from_pixel(0x00112233);
        first.publish(retained).expect("publish retained face");
        let committed = first.commit();

        let mut next = committed.begin_attempt();
        let mut fresh = Face::new(face_id);
        fresh.foreground = Color::from_pixel(0x00445566);
        next.publish(fresh.clone()).expect("publish fresh face");
        assert_eq!(
            next.admit_retained(committed.generation(), [face_id], &committed),
            Err(FrameFaceReuseError::ConflictingFace(face_id))
        );
        assert_eq!(
            next.face(face_id),
            Some(fresh),
            "failed retained admission must preserve the attempt publication"
        );
    }

    #[test]
    fn sealing_commits_the_finalized_face_table_for_future_replay() {
        let arena = FrameFaceArena::default();
        let mut attempt = arena.begin_attempt();
        let face_id = attempt.reserve_dynamic_face();
        attempt
            .publish(Face::new(face_id))
            .expect("publish semantic face");

        let mut finalized_faces = attempt.faces();
        finalized_faces
            .get_mut(&face_id)
            .expect("published face")
            .font_file_path = Some("/fonts/exact.ttf".to_owned());
        let sealed = attempt
            .seal(finalized_faces)
            .expect("sealing may enrich a published face");

        let mut replay = sealed.begin_attempt();
        replay
            .admit_retained(sealed.generation(), [face_id], &sealed)
            .expect("admit face from sealed arena");
        assert_eq!(
            replay.face(face_id).and_then(|face| face.font_file_path),
            Some("/fonts/exact.ttf".to_owned())
        );
    }

    #[test]
    fn sealing_advances_the_generation() {
        let arena = FrameFaceArena::default();
        let attempt = arena.begin_attempt();

        let sealed = attempt.seal(HashMap::new()).expect("seal empty attempt");

        assert_ne!(
            sealed.generation(),
            arena.generation(),
            "each accepted presentation needs a distinct retained-face generation"
        );
    }
}
