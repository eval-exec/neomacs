//! The FrameSync decoder (DESIGN §6.10, "Decoder").
//!
//! Every operation is applied in order, including those of a presentation
//! the caller will discard: the store is a decode dictionary, not display
//! state. At `Commit` the frame is rebuilt, sealed and validated; a failure
//! is a typed [`RejectReason`], never a panic.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use neomacs_display_protocol::font::{CharFontTable, ResolvedFontTable, ShapedClusterTable};
use neomacs_display_protocol::frame_chrome::{
    ChromeBandRequest, ChromeDisplayRow, FrameChromeContent, FrameSize,
};
use neomacs_display_protocol::input_progress::FrontierSnapshot;
use neomacs_display_protocol::scroll_coverage::{ScrollCoverage, ScrollCoverageRenewal};
use neomacs_display_protocol::{
    BackgroundItem, BorderItem, ContentTransitionHint, CursorItem, DisplayWindowId, EffectsConfig,
    FaceFillItem, FaceId, FrameChrome, FrameDisplayState, FrameDisplayStateParts, FrameFaceMap,
    FringeBitmapData, GlyphMatrix, LiveDisplayFrameId, MatrixRow, PhysCursor, PresentationId,
    PresentedFramePlacement, PresentedPointerSourceMap, Px, RowDamage, ScrollBarItem,
    SealedFramePresentation, WindowInfo, WindowMatrixEntry,
};

use super::mirror::RowMeta;
use super::{
    Begin, Changed, Chrome, ChromeContent, Coverage, Digest128, FactGroup, FrameFacts,
    FrameScalars, KeyedDelta, NonTextHitIndex, PresentBase, PresentOp, RejectReason, RowEdit,
    RowRef, RowSlot, RowsUpdate, SlotRef, StartRef, StoreLimits, TableDelta, WindowGeometry,
    WindowsUpdate, YRef, digest,
};
use crate::rows::{RowKey, RowPlacement, RowRuns};

/// A presentation rebuilt, sealed and validated.
#[derive(Debug)]
pub struct DecodedPresentation {
    pub frame: LiveDisplayFrameId,
    pub presentation: SealedFramePresentation,
}

#[derive(Debug)]
struct StoredRow {
    runs: Arc<RowRuns>,
    bytes: u64,
    meta: RowMeta,
    /// The last row built from this slot, reused while its placement holds.
    built: Option<(RowPlacement, MatrixRow)>,
}

#[derive(Debug)]
struct WindowState {
    geometry: WindowGeometry,
    rows: Vec<SlotRef>,
    /// Slots and tops of the last committed layout, for damage.
    committed: Vec<(RowSlot, u32)>,
}

/// One window's layout as of the open transaction, and the slots and tops of
/// its last committed layout.
type WindowSnapshot = (
    DisplayWindowId,
    WindowGeometry,
    Vec<SlotRef>,
    Vec<(RowSlot, u32)>,
);

#[derive(Debug, Default)]
struct FrameState {
    faces: FrameFaceMap,
    fonts: ResolvedFontTable,
    char_fonts: CharFontTable,
    shaped_clusters: ShapedClusterTable,
    fringe_bitmaps: BTreeMap<u16, FringeBitmapData>,
    cursor_effects: BTreeMap<DisplayWindowId, EffectsConfig>,
    scalars: Option<FrameScalars>,
    window_infos: Option<Vec<WindowInfo>>,
    transition_hints: Option<Vec<ContentTransitionHint>>,
    backgrounds: Option<Vec<BackgroundItem>>,
    face_fills: Option<Vec<FaceFillItem>>,
    borders: Option<Vec<BorderItem>>,
    cursors: Option<Vec<CursorItem>>,
    phys_cursor: Option<Option<PhysCursor>>,
    scroll_bars: Option<Vec<ScrollBarItem>>,
    chrome: Option<Chrome>,
    pointer_source: Option<PresentedPointerSourceMap>,
    regions: Option<NonTextHitIndex>,
    coverage: Option<Vec<Coverage>>,
    coverage_renewals: Option<Vec<ScrollCoverageRenewal>>,
    order: Vec<DisplayWindowId>,
    windows: BTreeMap<DisplayWindowId, WindowState>,
    coverage_cache: Vec<(Digest128, Arc<ScrollCoverage>)>,
}

#[derive(Debug)]
struct Open {
    begin: Begin,
    frontier: Option<Vec<FrontierSnapshot>>,
}

/// The receiver side of FrameSync for one session.
#[derive(Debug)]
pub struct FrameSyncDecoder {
    limits: StoreLimits,
    store: Vec<Option<StoredRow>>,
    store_bytes: u64,
    frames: BTreeMap<LiveDisplayFrameId, FrameState>,
    open: Option<Open>,
}

fn apply_delta<K: std::hash::Hash + Eq, V, S: std::hash::BuildHasher>(
    map: &mut std::collections::HashMap<K, V, S>,
    delta: KeyedDelta<K, V>,
) {
    for key in delta.remove {
        map.remove(&key);
    }
    for (key, value) in delta.upsert {
        map.insert(key, value);
    }
}

fn apply_ordered_delta<K: Ord, V>(map: &mut BTreeMap<K, V>, delta: KeyedDelta<K, V>) {
    for key in delta.remove {
        map.remove(&key);
    }
    for (key, value) in delta.upsert {
        map.insert(key, value);
    }
}

fn set<T>(slot: &mut Option<T>, change: Changed<T>) {
    if let Changed::Set(value) = change {
        *slot = Some(value);
    }
}

fn required<T: Clone>(value: &Option<T>, group: FactGroup) -> Result<T, RejectReason> {
    value.clone().ok_or(RejectReason::IncompleteKeyframe(group))
}

impl FrameSyncDecoder {
    #[must_use]
    pub fn new(limits: StoreLimits) -> Self {
        Self {
            limits,
            store: Vec::new(),
            store_bytes: 0,
            frames: BTreeMap::new(),
            open: None,
        }
    }

    /// Forget a frame whose window was destroyed.
    pub fn forget_frame(&mut self, frame: LiveDisplayFrameId) {
        self.frames.remove(&frame);
    }

    /// Apply one operation; `Commit` yields the rebuilt presentation.
    pub fn apply(&mut self, op: PresentOp) -> Result<Option<DecodedPresentation>, RejectReason> {
        let result = self.apply_op(op);
        if let Err(reason) = &result {
            tracing::warn!(%reason, "refused a peer presentation");
        }
        result
    }

    fn apply_op(&mut self, op: PresentOp) -> Result<Option<DecodedPresentation>, RejectReason> {
        match op {
            PresentOp::Begin(begin) => {
                if self.open.is_some() {
                    return Err(RejectReason::NestedBegin);
                }
                match begin.base {
                    PresentBase::Keyframe => {
                        self.frames.insert(begin.frame, FrameState::default());
                    }
                    PresentBase::Delta => {
                        if !self.frames.contains_key(&begin.frame) {
                            return Err(RejectReason::UnknownFrame);
                        }
                    }
                }
                self.open = Some(Open {
                    begin: *begin,
                    frontier: None,
                });
                Ok(None)
            }
            PresentOp::DefineRow { slot, key, row } => {
                self.transaction()?;
                self.define_row(slot, key, &row)?;
                Ok(None)
            }
            PresentOp::Tables(delta) => {
                let frame = self.open_frame()?;
                Self::apply_tables(frame, *delta);
                Ok(None)
            }
            PresentOp::Windows(update) => {
                let limits = self.limits;
                let frame = self.open_frame()?;
                Self::apply_windows(frame, *update, limits)?;
                Ok(None)
            }
            PresentOp::Facts(facts) => {
                let frontier = Self::apply_facts(self.open_frame()?, *facts)?;
                self.open.as_mut().expect("checked by open_frame").frontier = Some(frontier);
                Ok(None)
            }
            PresentOp::Commit { presentation } => {
                let open = self.open.take().ok_or(RejectReason::OutsideTransaction)?;
                if open.begin.presentation != presentation {
                    return Err(RejectReason::CommitMismatch);
                }
                let state = self.rebuild(&open)?;
                let sealed = SealedFramePresentation::seal(state)
                    .map_err(|error| RejectReason::Seal(format!("{error:?}")))?;
                sealed
                    .validate_for_materialize()
                    .map_err(|error| RejectReason::Materialize(error.to_string()))?;
                Ok(Some(DecodedPresentation {
                    frame: open.begin.frame,
                    presentation: sealed,
                }))
            }
            PresentOp::StoreReset => {
                if self.open.is_some() {
                    return Err(RejectReason::NestedBegin);
                }
                self.store.clear();
                self.store_bytes = 0;
                self.frames.clear();
                Ok(None)
            }
        }
    }

    fn transaction(&self) -> Result<&Open, RejectReason> {
        self.open.as_ref().ok_or(RejectReason::OutsideTransaction)
    }

    fn open_frame(&mut self) -> Result<&mut FrameState, RejectReason> {
        let frame = self.transaction()?.begin.frame;
        self.frames
            .get_mut(&frame)
            .ok_or(RejectReason::UnknownFrame)
    }

    fn define_row(
        &mut self,
        slot: RowSlot,
        key: RowKey,
        row: &crate::payload::Payload<RowRuns>,
    ) -> Result<(), RejectReason> {
        if slot.0 >= self.limits.rows {
            return Err(RejectReason::SlotOutOfRange(slot.0));
        }
        let bytes = row
            .bytes()
            .map_err(|error| RejectReason::Payload(error.to_string()))?;
        if RowKey::of_bytes(&bytes) != key {
            return Err(RejectReason::KeyMismatch);
        }
        let runs = row
            .value()
            .map_err(|error| RejectReason::Payload(error.to_string()))?;
        let index = slot.0 as usize;
        if self.store.len() <= index {
            self.store.resize_with(index + 1, || None);
        }
        if let Some(previous) = self.store[index].take() {
            self.store_bytes -= previous.bytes;
        }
        let size = bytes.len() as u64;
        if self.store_bytes + size > self.limits.bytes {
            return Err(RejectReason::StoreOverflow);
        }
        self.store_bytes += size;
        self.store[index] = Some(StoredRow {
            meta: RowMeta::of(&runs),
            runs,
            bytes: size,
            built: None,
        });
        Ok(())
    }

    fn apply_tables(frame: &mut FrameState, delta: TableDelta) {
        let TableDelta {
            faces,
            fonts,
            char_fonts,
            shaped_clusters,
            fringe_bitmaps,
            cursor_effects,
        } = delta;
        apply_delta(&mut frame.faces, faces);
        apply_delta(&mut frame.fonts, fonts);
        apply_delta(
            &mut frame.char_fonts,
            KeyedDelta {
                upsert: char_fonts
                    .upsert
                    .into_iter()
                    .map(|(face, entries)| (face, entries.into_iter().collect()))
                    .collect(),
                remove: char_fonts.remove,
            },
        );
        apply_delta(
            &mut frame.shaped_clusters,
            KeyedDelta {
                upsert: shaped_clusters
                    .upsert
                    .into_iter()
                    .map(|(face, entries)| (face, entries.into_iter().collect()))
                    .collect(),
                remove: shaped_clusters.remove,
            },
        );
        apply_ordered_delta(&mut frame.fringe_bitmaps, fringe_bitmaps);
        apply_ordered_delta(&mut frame.cursor_effects, cursor_effects);
    }

    fn apply_windows(
        frame: &mut FrameState,
        update: WindowsUpdate,
        limits: StoreLimits,
    ) -> Result<(), RejectReason> {
        let WindowsUpdate { order, changed } = update;
        let unique: BTreeSet<DisplayWindowId> = order.iter().copied().collect();
        if unique.len() != order.len() {
            return Err(RejectReason::DuplicateWindow);
        }
        for layout in changed {
            if !unique.contains(&layout.window) {
                return Err(RejectReason::UnknownWindow);
            }
            // The layout is checked before the frame changes: a refused one
            // leaves the window where the frame's order still names it.
            let previous = frame.windows.get(&layout.window);
            let geometry = match (layout.geometry, previous) {
                (Changed::Set(geometry), _) => geometry,
                (Changed::Same, Some(previous)) => previous.geometry,
                (Changed::Same, None) => return Err(RejectReason::UnknownWindow),
            };
            let old_rows = previous.map_or(&[][..], |previous| &previous.rows);
            let rows = match layout.rows {
                RowsUpdate::Same => old_rows.to_vec(),
                RowsUpdate::Script(script) => {
                    let mut rows = Vec::new();
                    for edit in script {
                        match edit {
                            RowEdit::Copy { from, len } => {
                                let end = (from as usize)
                                    .checked_add(len as usize)
                                    .filter(|end| *end <= old_rows.len())
                                    .ok_or(RejectReason::CopyOutOfRange)?;
                                rows.extend_from_slice(&old_rows[from as usize..end]);
                            }
                            RowEdit::Place(slot_ref) => {
                                if slot_ref.slot.0 >= limits.rows {
                                    return Err(RejectReason::SlotOutOfRange(slot_ref.slot.0));
                                }
                                rows.push(slot_ref);
                            }
                        }
                        if rows.len() > limits.rows as usize * 4 {
                            return Err(RejectReason::CopyOutOfRange);
                        }
                    }
                    rows
                }
            };
            match frame.windows.get_mut(&layout.window) {
                Some(state) => {
                    state.geometry = geometry;
                    state.rows = rows;
                }
                None => {
                    frame.windows.insert(
                        layout.window,
                        WindowState {
                            geometry,
                            rows,
                            committed: Vec::new(),
                        },
                    );
                }
            }
        }
        if order
            .iter()
            .any(|window| !frame.windows.contains_key(window))
        {
            return Err(RejectReason::UnknownWindow);
        }
        frame.windows.retain(|window, _| unique.contains(window));
        frame.order = order;
        Ok(())
    }

    fn apply_facts(
        frame: &mut FrameState,
        facts: FrameFacts,
    ) -> Result<Vec<FrontierSnapshot>, RejectReason> {
        let FrameFacts {
            scalars,
            window_infos,
            transition_hints,
            backgrounds,
            face_fills,
            borders,
            cursors,
            phys_cursor,
            scroll_bars,
            chrome,
            pointer_source,
            regions,
            coverage,
            coverage_renewals,
            frontier,
        } = facts;
        // One snapshot per input stream (M1 follow-up 3): checked before the
        // frame changes.
        let mut streams = BTreeSet::new();
        if let Some(repeated) = frontier
            .iter()
            .map(FrontierSnapshot::stream)
            .find(|stream| !streams.insert(*stream))
        {
            return Err(RejectReason::FrontierRepeatsStream(repeated.get()));
        }
        set(&mut frame.scalars, scalars);
        set(&mut frame.window_infos, window_infos);
        set(&mut frame.transition_hints, transition_hints);
        set(&mut frame.backgrounds, backgrounds);
        set(&mut frame.face_fills, face_fills);
        set(&mut frame.borders, borders);
        set(&mut frame.cursors, cursors);
        set(&mut frame.phys_cursor, phys_cursor);
        set(&mut frame.scroll_bars, scroll_bars);
        set(&mut frame.chrome, chrome);
        set(&mut frame.pointer_source, pointer_source);
        set(&mut frame.regions, regions);
        set(&mut frame.coverage, coverage);
        set(&mut frame.coverage_renewals, coverage_renewals);
        Ok(frontier)
    }

    fn stored(&self, slot: RowSlot) -> Result<&StoredRow, RejectReason> {
        self.store
            .get(slot.0 as usize)
            .and_then(Option::as_ref)
            .ok_or(RejectReason::UnknownSlot(slot.0))
    }

    /// The row in `slot` at `placement`, reusing the last one built there.
    fn row_at(
        &mut self,
        slot: RowSlot,
        placement: RowPlacement,
    ) -> Result<MatrixRow, RejectReason> {
        let stored = self
            .store
            .get_mut(slot.0 as usize)
            .and_then(Option::as_mut)
            .ok_or(RejectReason::UnknownSlot(slot.0))?;
        if let Some((built_at, row)) = &stored.built
            && built_at.same_as(&placement)
        {
            return Ok(row.clone());
        }
        let row = MatrixRow::new(stored.runs.to_row(&placement).map_err(RejectReason::Row)?);
        stored.built = Some((placement, row.clone()));
        Ok(row)
    }

    /// Resolve relative slot references to absolute placements.
    fn placements(&self, rows: &[SlotRef]) -> Result<Vec<(RowSlot, RowPlacement)>, RejectReason> {
        let mut out: Vec<(RowSlot, RowPlacement)> = Vec::with_capacity(rows.len());
        let mut previous: Option<(RowPlacement, RowMeta)> = None;
        for slot_ref in rows {
            let meta = self.stored(slot_ref.slot)?.meta;
            let pixel_y = match slot_ref.y {
                YRef::At(bits) => bits.get(),
                YRef::Follows => previous.as_ref().map_or(0.0, |(placement, meta)| {
                    placement.pixel_y + meta.height_px.get()
                }),
            };
            let start_charpos = match slot_ref.start {
                StartRef::At(start) => usize::try_from(start)
                    .map_err(|_| RejectReason::Row(crate::rows::RowError::Position))?,
                StartRef::Follows => {
                    let (placement, meta) = previous
                        .as_ref()
                        .ok_or(RejectReason::Row(crate::rows::RowError::Position))?;
                    if meta.continued {
                        placement.end_charpos
                    } else {
                        placement
                            .end_charpos
                            .checked_add(1)
                            .ok_or(RejectReason::Row(crate::rows::RowError::Position))?
                    }
                }
            };
            let end_charpos = usize::try_from(start_charpos as i128 + i128::from(slot_ref.span))
                .map_err(|_| RejectReason::Row(crate::rows::RowError::Position))?;
            let placement = RowPlacement {
                pixel_y,
                start_charpos,
                end_charpos,
                cursor_col: slot_ref.cursor_col,
                cursor_type: slot_ref.cursor_type,
            };
            out.push((slot_ref.slot, placement));
            previous = Some((placement, meta));
        }
        Ok(out)
    }

    fn matrix(
        &mut self,
        geometry: &WindowGeometry,
        rows: Vec<(RowSlot, RowPlacement)>,
        committed: &[(RowSlot, u32)],
    ) -> Result<GlyphMatrix, RejectReason> {
        let mut matrix = GlyphMatrix::new(0, usize::try_from(geometry.ncols).unwrap_or(usize::MAX));
        let mut built = Vec::with_capacity(rows.len());
        for (slot, placement) in &rows {
            built.push(self.row_at(*slot, *placement)?);
        }
        matrix.rows = built;
        matrix.nrows = usize::try_from(geometry.nrows).unwrap_or(usize::MAX);
        matrix.matrix_x = usize::try_from(geometry.matrix_x).unwrap_or(usize::MAX);
        matrix.matrix_y = usize::try_from(geometry.matrix_y).unwrap_or(usize::MAX);
        matrix.header_line = geometry.header_line;
        matrix.tab_line = geometry.tab_line;
        // Damage against the last committed layout of this window: the same
        // slot at the same top is reused, elsewhere reused and shifted.
        for (index, (slot, placement)) in rows.iter().enumerate() {
            let damage = match committed.iter().find(|(old, _)| old == slot) {
                Some((_, top)) if *top == placement.pixel_y.to_bits() => RowDamage::Reused,
                Some((_, top)) => RowDamage::ReusedShifted {
                    dvpos: Px(placement.pixel_y - f32::from_bits(*top)),
                },
                None => RowDamage::New,
            };
            matrix.set_row_damage(index, damage);
        }
        Ok(matrix)
    }

    fn row_refs(&mut self, rows: &[RowRef]) -> Result<Vec<MatrixRow>, RejectReason> {
        rows.iter()
            .map(|row| self.row_at(row.slot, row.placement))
            .collect()
    }

    fn chrome(&mut self, chrome: &Chrome) -> Result<FrameChrome, RejectReason> {
        if chrome.bands.is_empty() {
            return Ok(FrameChrome::default());
        }
        let mut height = 0.0f32;
        let mut requests = Vec::with_capacity(chrome.bands.len());
        for band in &chrome.bands {
            height += band.height;
            let content = match &band.content {
                ChromeContent::DisplayRow(row) => FrameChromeContent::DisplayRow(
                    ChromeDisplayRow::new(self.row_at(row.slot, row.placement)?.as_ref().clone()),
                ),
                ChromeContent::MenuBar(content) => FrameChromeContent::MenuBar(content.clone()),
                ChromeContent::ToolBar(content) => FrameChromeContent::ToolBar(content.clone()),
                ChromeContent::CompactBar(content) => {
                    FrameChromeContent::CompactBar(content.clone())
                }
            };
            requests.push(
                ChromeBandRequest::new(band.kind, band.height, content)
                    .with_hit_regions(band.hit_regions.clone()),
            );
        }
        let size = FrameSize::new(chrome.width, height)
            .map_err(|error| RejectReason::Chrome(format!("{error:?}")))?;
        let rebuilt = FrameChrome::layout(size, requests)
            .map_err(|error| RejectReason::Chrome(format!("{error:?}")))?;
        if rebuilt.bands().len() != chrome.bands.len() {
            return Err(RejectReason::Chrome("a band was dropped".into()));
        }
        Ok(rebuilt)
    }

    fn coverage(
        &mut self,
        frame: LiveDisplayFrameId,
        wire: &Coverage,
    ) -> Result<Arc<ScrollCoverage>, RejectReason> {
        let key = digest(wire).map_err(|error| RejectReason::Payload(error.to_string()))?;
        if let Some((_, cached)) = self
            .frames
            .get(&frame)
            .and_then(|state| state.coverage_cache.iter().find(|(known, _)| *known == key))
        {
            return Ok(Arc::clone(cached));
        }
        let rows = self.row_refs(&wire.rows)?;
        let faces: FrameFaceMap = wire.faces.iter().cloned().collect();
        for row in &rows {
            for face in row.referenced_face_ids() {
                if !faces.contains_key(&face) {
                    return Err(RejectReason::MissingFace(face.get()));
                }
            }
        }
        let mut matrix = GlyphMatrix::new(
            0,
            usize::try_from(wire.geometry.ncols).unwrap_or(usize::MAX),
        );
        matrix.rows = rows;
        matrix.nrows = usize::try_from(wire.geometry.nrows).unwrap_or(usize::MAX);
        matrix.matrix_x = usize::try_from(wire.geometry.matrix_x).unwrap_or(usize::MAX);
        matrix.matrix_y = usize::try_from(wire.geometry.matrix_y).unwrap_or(usize::MAX);
        matrix.header_line = wire.geometry.header_line;
        matrix.tab_line = wire.geometry.tab_line;
        let coverage = Arc::new(ScrollCoverage {
            epoch: wire.epoch,
            publication: wire.publication,
            predict_pixels: wire.predict_pixels,
            compositor_enabled: wire.compositor_enabled,
            anchor_row: usize::try_from(wire.anchor_row).unwrap_or(usize::MAX),
            viewport: wire.viewport,
            origin: wire.origin,
            content: WindowMatrixEntry {
                window_id: wire.window,
                matrix,
                pixel_bounds: wire.geometry.pixel_bounds,
                text_pixel_bounds: wire.geometry.text_pixel_bounds,
                text_clip_bounds: wire.geometry.text_clip_bounds,
                selected: wire.geometry.selected,
            },
            faces,
            fonts: wire.fonts.iter().cloned().collect(),
            char_fonts: wire
                .char_fonts
                .iter()
                .map(|(face, entries)| (*face, entries.iter().cloned().collect()))
                .collect(),
            shaped_clusters: wire
                .shaped_clusters
                .iter()
                .map(|(face, entries)| (*face, entries.iter().cloned().collect()))
                .collect(),
            hit_index: wire
                .hit_index
                .rebuild(wire.hit_presentation)
                .map_err(|error| RejectReason::HitIndex(format!("{error:?}")))?,
            pointer_source: wire.pointer_source.clone(),
        });
        Ok(coverage)
    }

    fn rebuild(&mut self, open: &Open) -> Result<FrameDisplayState, RejectReason> {
        let frame_id = open.begin.frame;
        let presentation: PresentationId = open.begin.presentation;
        let frontier = open
            .frontier
            .clone()
            .ok_or(RejectReason::IncompleteKeyframe(FactGroup::Facts))?;
        let frame = self
            .frames
            .get(&frame_id)
            .ok_or(RejectReason::UnknownFrame)?;
        let scalars = required(&frame.scalars, FactGroup::Scalars)?;
        let chrome_wire = required(&frame.chrome, FactGroup::Chrome)?;
        let coverage_wire = required(&frame.coverage, FactGroup::Coverage)?;
        let regions = required(&frame.regions, FactGroup::Regions)?;
        let order = frame.order.clone();
        let layouts: Vec<WindowSnapshot> = order
            .iter()
            .map(|window| {
                let state = &frame.windows[window];
                (
                    *window,
                    state.geometry,
                    state.rows.clone(),
                    state.committed.clone(),
                )
            })
            .collect();

        let mut window_matrices = Vec::with_capacity(layouts.len());
        let mut committed = Vec::with_capacity(layouts.len());
        for (window, geometry, rows, previous) in layouts {
            let placements = self.placements(&rows)?;
            committed.push((
                window,
                placements
                    .iter()
                    .map(|(slot, placement)| (*slot, placement.pixel_y.to_bits()))
                    .collect::<Vec<_>>(),
            ));
            let matrix = self.matrix(&geometry, placements, &previous)?;
            window_matrices.push(WindowMatrixEntry {
                window_id: window,
                matrix,
                pixel_bounds: geometry.pixel_bounds,
                text_pixel_bounds: geometry.text_pixel_bounds,
                text_clip_bounds: geometry.text_clip_bounds,
                selected: geometry.selected,
            });
        }
        let frame_chrome = self.chrome(&chrome_wire)?;
        let mut scroll_coverage = Vec::with_capacity(coverage_wire.len());
        let mut coverage_cache = Vec::with_capacity(coverage_wire.len());
        for wire in &coverage_wire {
            let coverage = self.coverage(frame_id, wire)?;
            coverage_cache.push((
                digest(wire).map_err(|error| RejectReason::Payload(error.to_string()))?,
                Arc::clone(&coverage),
            ));
            scroll_coverage.push(coverage);
        }

        let frame = self.frames.get_mut(&frame_id).expect("checked above");
        for (window, rows) in committed {
            if let Some(state) = frame.windows.get_mut(&window) {
                state.committed = rows;
            }
        }
        frame.coverage_cache = coverage_cache;

        // Every face a row or a face fill names must be defined.
        let referenced = window_matrices
            .iter()
            .flat_map(|entry| entry.matrix.rows.iter())
            .flat_map(|row| row.referenced_face_ids().collect::<Vec<FaceId>>())
            .chain(
                frame_chrome
                    .bands()
                    .iter()
                    .flat_map(|band| match band.content() {
                        FrameChromeContent::DisplayRow(content) => {
                            content.row().referenced_face_ids().collect::<Vec<FaceId>>()
                        }
                        FrameChromeContent::MenuBar(_)
                        | FrameChromeContent::ToolBar(_)
                        | FrameChromeContent::CompactBar(_) => Vec::new(),
                    }),
            )
            .chain(frame.face_fills.iter().flatten().map(|fill| fill.face_id));
        for face in referenced {
            if !frame.faces.contains_key(&face) {
                return Err(RejectReason::MissingFace(face.get()));
            }
        }

        let FrameScalars {
            scroll_input_policy,
            layout_scale,
            origin,
            placement,
            frame_cols,
            frame_rows,
            frame_pixel_width,
            frame_pixel_height,
            char_width,
            char_height,
            font_pixel_size,
            background,
            font_catalog_generation,
            desktop_font_render_options,
            undecorated,
            border_width,
            border_color,
            outer_border_width,
            outer_border_color,
            background_alpha,
            frame_alpha,
            no_accept_focus,
        } = scalars;
        let parts = FrameDisplayStateParts {
            scroll_input_policy,
            scroll_coverage,
            scroll_coverage_renewals: required(
                &frame.coverage_renewals,
                FactGroup::CoverageRenewals,
            )?,
            input_checkpoint: frontier,
            layout_scale,
            presentation_id: presentation,
            origin,
            frame_placement: PresentedFramePlacement::new(
                placement.frame,
                presentation,
                placement.parent,
                placement.outer_in_parent,
                placement.z_order,
            ),
            presented_pointer_source: required(&frame.pointer_source, FactGroup::PointerSource)?,
            presented_hit_index: regions
                .rebuild(presentation)
                .map_err(|error| RejectReason::HitIndex(format!("{error:?}")))?,
            window_matrices,
            frame_chrome,
            frame_cols: usize::try_from(frame_cols).unwrap_or(usize::MAX),
            frame_rows: usize::try_from(frame_rows).unwrap_or(usize::MAX),
            frame_pixel_width,
            frame_pixel_height,
            char_width,
            char_height,
            font_pixel_size,
            background,
            faces: frame.faces.clone(),
            font_catalog_generation,
            desktop_font_render_options,
            fonts: frame.fonts.clone(),
            char_fonts: frame.char_fonts.clone(),
            shaped_clusters: frame.shaped_clusters.clone(),
            undecorated,
            border_width,
            border_color,
            outer_border_width,
            outer_border_color,
            background_alpha,
            frame_alpha,
            no_accept_focus,
            window_infos: required(&frame.window_infos, FactGroup::WindowInfos)?,
            transition_hints: required(&frame.transition_hints, FactGroup::TransitionHints)?,
            backgrounds: required(&frame.backgrounds, FactGroup::Backgrounds)?,
            face_fills: required(&frame.face_fills, FactGroup::FaceFills)?,
            borders: required(&frame.borders, FactGroup::Borders)?,
            cursors: required(&frame.cursors, FactGroup::Cursors)?,
            cursor_effects_by_window: frame
                .cursor_effects
                .iter()
                .map(|(window, effects)| (*window, effects.clone()))
                .collect(),
            scroll_bars: required(&frame.scroll_bars, FactGroup::ScrollBars)?,
            phys_cursor: required(&frame.phys_cursor, FactGroup::PhysCursor)?,
            fringe_bitmaps: frame
                .fringe_bitmaps
                .iter()
                .map(|(index, bitmap)| (*index, bitmap.clone()))
                .collect(),
        };
        Ok(FrameDisplayState::from_parts(parts))
    }
}
