//! The FrameSync encoder (DESIGN §6.10, "Encoder, per presentation").

use std::collections::{BTreeMap, BTreeSet};

use neomacs_display_protocol::frame_chrome::FrameChromeContent;
use neomacs_display_protocol::scroll_coverage::ScrollCoverage;
use neomacs_display_protocol::{
    DisplayWindowId, FaceId, FrameChrome, FrameDisplayStateView, GlyphMatrix, LiveDisplayFrameId,
    SealedFramePresentation, WindowMatrixEntry,
};
use serde::Serialize;

use super::mirror::{RowMeta, StoreMirror, UpdateRows};
use super::{
    Begin, Changed, Chrome, ChromeBand, ChromeContent, Coverage, Digest128, F32Bits, FrameFacts,
    FramePlacement, FrameScalars, KeyedDelta, NonTextHitIndex, PresentBase, PresentOp, RowEdit,
    RowRef, RowSlot, RowsUpdate, SlotRef, StartRef, StoreLimits, TableDelta, UpdateSeq,
    WindowGeometry, WindowLayout, WindowsUpdate, YRef, digest, sorted, sorted_char_fonts,
    sorted_shaped_clusters,
};
use crate::codec::WireError;
use crate::rows::RowPlacement;

/// Digests of the last-sent definitions of one keyed table.
type TableMirror<K> = BTreeMap<K, Digest128>;

#[derive(Debug, Default)]
struct TableDigests {
    faces: TableMirror<FaceId>,
    fonts: TableMirror<neomacs_display_protocol::font::ResolvedFontId>,
    char_fonts: TableMirror<FaceId>,
    shaped_clusters: TableMirror<FaceId>,
    fringe_bitmaps: TableMirror<u16>,
    cursor_effects: TableMirror<DisplayWindowId>,
}

#[derive(Debug, Default)]
struct FactDigests {
    scalars: Option<Digest128>,
    window_infos: Option<Digest128>,
    transition_hints: Option<Digest128>,
    backgrounds: Option<Digest128>,
    face_fills: Option<Digest128>,
    borders: Option<Digest128>,
    cursors: Option<Digest128>,
    phys_cursor: Option<Digest128>,
    scroll_bars: Option<Digest128>,
    chrome: Option<Digest128>,
    pointer_source: Option<Digest128>,
    regions: Option<Digest128>,
    coverage: Option<Digest128>,
    coverage_renewals: Option<Digest128>,
}

#[derive(Debug)]
struct WindowMirror {
    geometry: Digest128,
    rows: Vec<SlotRef>,
}

/// What the receiver holds for one frame.
#[derive(Debug, Default)]
struct FrameMirror {
    tables: TableDigests,
    facts: FactDigests,
    order: Vec<DisplayWindowId>,
    windows: BTreeMap<DisplayWindowId, WindowMirror>,
}

/// The sender side of FrameSync for one session.
#[derive(Debug)]
pub struct FrameSyncEncoder {
    store: StoreMirror,
    frames: BTreeMap<LiveDisplayFrameId, FrameMirror>,
    keyframe_due: BTreeSet<LiveDisplayFrameId>,
    next_update: u64,
}

/// A value sent only when its digest differs from the last-sent one.
fn changed<T: Serialize + Clone>(
    value: &T,
    last: &mut Option<Digest128>,
) -> Result<Changed<T>, WireError> {
    let now = digest(value)?;
    if *last == Some(now) {
        Ok(Changed::Same)
    } else {
        *last = Some(now);
        Ok(Changed::Set(value.clone()))
    }
}

/// Upserts for entries whose digest changed and removals for entries gone.
fn keyed_delta<K: Ord + Copy, V: Serialize + Clone>(
    entries: Vec<(K, V)>,
    mirror: &mut TableMirror<K>,
) -> Result<KeyedDelta<K, V>, WireError> {
    let mut delta = KeyedDelta::default();
    let mut present = BTreeSet::new();
    for (key, value) in entries {
        present.insert(key);
        let now = digest(&value)?;
        if mirror.get(&key) != Some(&now) {
            mirror.insert(key, now);
            delta.upsert.push((key, value));
        }
    }
    let gone: Vec<K> = mirror
        .keys()
        .filter(|key| !present.contains(*key))
        .copied()
        .collect();
    for key in gone {
        mirror.remove(&key);
        delta.remove.push(key);
    }
    Ok(delta)
}

fn next_start(previous: &RowPlacement, meta: &RowMeta) -> Option<usize> {
    if meta.continued {
        Some(previous.end_charpos)
    } else {
        previous.end_charpos.checked_add(1)
    }
}

/// The row's placement relative to the row before it.
pub(super) fn slot_ref(
    slot: RowSlot,
    placement: &RowPlacement,
    previous: Option<&(RowPlacement, RowMeta)>,
) -> Result<SlotRef, WireError> {
    let follows_y = match previous {
        Some((previous, meta)) => (previous.pixel_y + meta.height_px.get()).to_bits(),
        None => 0f32.to_bits(),
    };
    let y = if placement.pixel_y.to_bits() == follows_y {
        YRef::Follows
    } else {
        YRef::At(F32Bits::of(placement.pixel_y))
    };
    let start = match previous {
        Some((previous, meta)) if next_start(previous, meta) == Some(placement.start_charpos) => {
            StartRef::Follows
        }
        _ => StartRef::At(placement.start_charpos as u64),
    };
    let span = i64::try_from(placement.end_charpos as i128 - placement.start_charpos as i128)
        .map_err(|_| WireError::Encode("a row span does not fit 64 bits".into()))?;
    Ok(SlotRef {
        slot,
        y,
        start,
        span,
        cursor_col: placement.cursor_col,
        cursor_type: placement.cursor_type,
    })
}

/// An edit script turning `old` into `new`: runs of rows found in `old`
/// become copies, the rest are placed.
fn edit_script(old: &[SlotRef], new: &[SlotRef]) -> RowsUpdate {
    if old == new {
        return RowsUpdate::Same;
    }
    let mut positions: BTreeMap<_, Vec<usize>> = BTreeMap::new();
    for (index, row) in old.iter().enumerate() {
        positions.entry(row.index_key()).or_default().push(index);
    }
    let mut script: Vec<RowEdit> = Vec::new();
    let mut index = 0;
    let mut continue_at: Option<usize> = None;
    while index < new.len() {
        let mut best: Option<(usize, usize)> = None;
        let candidates = continue_at.into_iter().chain(
            positions
                .get(&new[index].index_key())
                .into_iter()
                .flatten()
                .take(8)
                .copied(),
        );
        for start in candidates {
            if old.get(start) != Some(&new[index]) {
                continue;
            }
            let len = old[start..]
                .iter()
                .zip(&new[index..])
                .take_while(|(a, b)| a == b)
                .count();
            if best.is_none_or(|(_, best_len)| len > best_len) {
                best = Some((start, len));
            }
        }
        match best {
            Some((from, len)) => {
                match script.last_mut() {
                    Some(RowEdit::Copy {
                        from: previous,
                        len: previous_len,
                    }) if *previous as usize + *previous_len as usize == from => {
                        *previous_len += len as u32;
                    }
                    _ => script.push(RowEdit::Copy {
                        from: from as u32,
                        len: len as u32,
                    }),
                }
                index += len;
                continue_at = Some(from + len);
            }
            None => {
                script.push(RowEdit::Place(new[index]));
                index += 1;
                continue_at = None;
            }
        }
    }
    RowsUpdate::Script(script)
}

fn window_geometry(entry: &WindowMatrixEntry) -> WindowGeometry {
    let WindowMatrixEntry {
        window_id: _,
        matrix,
        pixel_bounds,
        text_pixel_bounds,
        text_clip_bounds,
        selected,
    } = entry;
    let GlyphMatrix {
        rows: _,
        nrows,
        ncols,
        matrix_x,
        matrix_y,
        header_line,
        tab_line,
        ..
    } = matrix;
    WindowGeometry {
        pixel_bounds: *pixel_bounds,
        text_pixel_bounds: *text_pixel_bounds,
        text_clip_bounds: *text_clip_bounds,
        selected: *selected,
        nrows: *nrows as u64,
        ncols: *ncols as u64,
        matrix_x: *matrix_x as u64,
        matrix_y: *matrix_y as u64,
        header_line: *header_line,
        tab_line: *tab_line,
    }
}

impl FrameSyncEncoder {
    #[must_use]
    pub fn new(limits: StoreLimits) -> Self {
        Self {
            store: StoreMirror::new(limits),
            frames: BTreeMap::new(),
            keyframe_due: BTreeSet::new(),
            next_update: 1,
        }
    }

    /// The next presentation of `frame` is a keyframe (after `DisplayReset`
    /// or `Rejected`).
    pub fn request_keyframe(&mut self, frame: LiveDisplayFrameId) {
        self.keyframe_due.insert(frame);
    }

    /// The frame's window was destroyed; nothing more is sent for it.
    pub fn forget_frame(&mut self, frame: LiveDisplayFrameId) {
        self.frames.remove(&frame);
        self.keyframe_due.remove(&frame);
    }

    /// Forget the store and every frame: the receiver is told `StoreReset`
    /// and every frame's next presentation is a keyframe.
    pub fn store_reset(&mut self) -> PresentOp {
        self.store.reset();
        self.frames.clear();
        self.keyframe_due.clear();
        PresentOp::StoreReset
    }

    fn rows_of_matrix(
        &mut self,
        matrix: &GlyphMatrix,
        update: &mut UpdateRows,
    ) -> Result<Vec<SlotRef>, WireError> {
        let mut refs = Vec::with_capacity(matrix.rows.len());
        let mut previous = None;
        for row in &matrix.rows {
            let (slot, placement, meta) = self.store.resolve(row, Some(row), update)?;
            refs.push(slot_ref(slot, &placement, previous.as_ref())?);
            previous = Some((placement, meta));
        }
        Ok(refs)
    }

    fn row_refs(
        &mut self,
        matrix: &GlyphMatrix,
        update: &mut UpdateRows,
    ) -> Result<Vec<RowRef>, WireError> {
        matrix
            .rows
            .iter()
            .map(|row| {
                let (slot, placement, _) = self.store.resolve(row, Some(row), update)?;
                Ok(RowRef { slot, placement })
            })
            .collect()
    }

    fn chrome(
        &mut self,
        chrome: &FrameChrome,
        update: &mut UpdateRows,
    ) -> Result<Chrome, WireError> {
        let bands = chrome
            .bands()
            .iter()
            .map(|band| {
                let content = match band.content() {
                    FrameChromeContent::DisplayRow(content) => {
                        let (slot, placement, _) =
                            self.store.resolve(content.row(), None, update)?;
                        ChromeContent::DisplayRow(RowRef { slot, placement })
                    }
                    FrameChromeContent::MenuBar(content) => ChromeContent::MenuBar(content.clone()),
                    FrameChromeContent::ToolBar(content) => ChromeContent::ToolBar(content.clone()),
                    FrameChromeContent::CompactBar(content) => {
                        ChromeContent::CompactBar(content.clone())
                    }
                };
                Ok(ChromeBand {
                    kind: band.kind(),
                    height: band.bounds().height(),
                    content,
                    hit_regions: band.hit_regions().to_vec(),
                })
            })
            .collect::<Result<Vec<_>, WireError>>()?;
        Ok(Chrome {
            width: chrome
                .bands()
                .first()
                .map_or(0.0, |band| band.bounds().width()),
            bands,
        })
    }

    fn coverage(
        &mut self,
        coverage: &ScrollCoverage,
        update: &mut UpdateRows,
    ) -> Result<Coverage, WireError> {
        let ScrollCoverage {
            epoch,
            publication,
            predict_pixels,
            compositor_enabled,
            anchor_row,
            viewport,
            origin,
            content,
            faces,
            fonts,
            char_fonts,
            shaped_clusters,
            hit_index,
            pointer_source,
        } = coverage;
        Ok(Coverage {
            epoch: *epoch,
            publication: *publication,
            predict_pixels: *predict_pixels,
            compositor_enabled: *compositor_enabled,
            anchor_row: *anchor_row as u64,
            viewport: *viewport,
            origin: *origin,
            window: content.window_id,
            geometry: window_geometry(content),
            rows: self.row_refs(&content.matrix, update)?,
            faces: sorted(faces.iter().map(|(id, face)| (*id, face.clone()))),
            fonts: sorted(fonts.iter().map(|(id, font)| (*id, font.clone()))),
            char_fonts: sorted_char_fonts(char_fonts),
            shaped_clusters: sorted_shaped_clusters(shaped_clusters),
            hit_presentation: hit_index.presentation(),
            hit_index: NonTextHitIndex::of(hit_index),
            pointer_source: pointer_source.clone(),
        })
    }

    /// Encode `presentation` of `frame` as one transaction against what the
    /// receiver holds.
    pub fn encode(
        &mut self,
        frame: LiveDisplayFrameId,
        presentation: &SealedFramePresentation,
    ) -> Result<Vec<PresentOp>, WireError> {
        let FrameDisplayStateView {
            scroll_input_policy,
            scroll_coverage,
            scroll_coverage_renewals,
            input_checkpoint,
            layout_scale,
            presentation_id,
            origin,
            frame_placement,
            presented_pointer_source,
            presented_hit_index,
            window_matrices,
            frame_chrome,
            frame_cols,
            frame_rows,
            frame_pixel_width,
            frame_pixel_height,
            char_width,
            char_height,
            font_pixel_size,
            background,
            faces,
            font_catalog_generation,
            desktop_font_render_options,
            fonts,
            char_fonts,
            shaped_clusters,
            undecorated,
            border_width,
            border_color,
            outer_border_width,
            outer_border_color,
            background_alpha,
            frame_alpha,
            no_accept_focus,
            window_infos,
            transition_hints,
            backgrounds,
            face_fills,
            borders,
            cursors,
            cursor_effects_by_window,
            scroll_bars,
            phys_cursor,
            fringe_bitmaps,
        } = presentation.state().view();

        let keyframe = self.keyframe_due.remove(&frame) || !self.frames.contains_key(&frame);
        if keyframe {
            self.frames.insert(frame, FrameMirror::default());
        }
        let update_seq = UpdateSeq(self.next_update);
        self.next_update += 1;
        let mut update = UpdateRows::default();

        // Rows first: every slot this update references is resolved (and
        // defined when the receiver lacks it) before anything names it.
        let mut layouts = Vec::with_capacity(window_matrices.len());
        for entry in window_matrices {
            let rows = self.rows_of_matrix(&entry.matrix, &mut update)?;
            layouts.push((entry.window_id, window_geometry(entry), rows));
        }
        let chrome = self.chrome(frame_chrome, &mut update)?;
        let coverage = scroll_coverage
            .iter()
            .map(|coverage| self.coverage(coverage, &mut update))
            .collect::<Result<Vec<_>, WireError>>()?;

        let mirror = self.frames.get_mut(&frame).expect("inserted above");
        let tables = TableDelta {
            faces: keyed_delta(
                sorted(faces.iter().map(|(id, face)| (*id, face.clone()))),
                &mut mirror.tables.faces,
            )?,
            fonts: keyed_delta(
                sorted(fonts.iter().map(|(id, font)| (*id, font.clone()))),
                &mut mirror.tables.fonts,
            )?,
            char_fonts: keyed_delta(sorted_char_fonts(char_fonts), &mut mirror.tables.char_fonts)?,
            shaped_clusters: keyed_delta(
                sorted_shaped_clusters(shaped_clusters),
                &mut mirror.tables.shaped_clusters,
            )?,
            fringe_bitmaps: keyed_delta(
                sorted(fringe_bitmaps.iter().map(|(id, bits)| (*id, bits.clone()))),
                &mut mirror.tables.fringe_bitmaps,
            )?,
            cursor_effects: keyed_delta(
                sorted(
                    cursor_effects_by_window
                        .iter()
                        .map(|(id, effects)| (*id, effects.clone())),
                ),
                &mut mirror.tables.cursor_effects,
            )?,
        };

        let order: Vec<DisplayWindowId> = layouts.iter().map(|(id, _, _)| *id).collect();
        let order_changed = mirror.order != order;
        mirror.order.clone_from(&order);
        let mut changed_layouts = Vec::new();
        let mut retained = BTreeMap::new();
        for (window, geometry, rows) in layouts {
            let geometry_digest = digest(&geometry)?;
            let previous = mirror.windows.remove(&window);
            let (geometry_change, rows_update) = match &previous {
                Some(previous) => (
                    if previous.geometry == geometry_digest {
                        Changed::Same
                    } else {
                        Changed::Set(geometry)
                    },
                    edit_script(&previous.rows, &rows),
                ),
                None => (Changed::Set(geometry), edit_script(&[], &rows)),
            };
            if geometry_change != Changed::Same || rows_update != RowsUpdate::Same {
                changed_layouts.push(WindowLayout {
                    window,
                    geometry: geometry_change,
                    rows: rows_update,
                });
            }
            retained.insert(
                window,
                WindowMirror {
                    geometry: geometry_digest,
                    rows,
                },
            );
        }
        mirror.windows = retained;

        let scalars = FrameScalars {
            scroll_input_policy: *scroll_input_policy,
            layout_scale: *layout_scale,
            origin: *origin,
            placement: FramePlacement {
                frame: frame_placement.frame(),
                parent: frame_placement.parent(),
                outer_in_parent: frame_placement.outer_in_parent(),
                z_order: frame_placement.z_order(),
            },
            frame_cols: *frame_cols as u64,
            frame_rows: *frame_rows as u64,
            frame_pixel_width: *frame_pixel_width,
            frame_pixel_height: *frame_pixel_height,
            char_width: *char_width,
            char_height: *char_height,
            font_pixel_size: *font_pixel_size,
            background: *background,
            font_catalog_generation: *font_catalog_generation,
            desktop_font_render_options: *desktop_font_render_options,
            undecorated: *undecorated,
            border_width: *border_width,
            border_color: *border_color,
            outer_border_width: *outer_border_width,
            outer_border_color: *outer_border_color,
            background_alpha: *background_alpha,
            frame_alpha: *frame_alpha,
            no_accept_focus: *no_accept_focus,
        };
        let facts = &mut mirror.facts;
        let frame_facts = FrameFacts {
            scalars: changed(&scalars, &mut facts.scalars)?,
            window_infos: changed(window_infos, &mut facts.window_infos)?,
            transition_hints: changed(transition_hints, &mut facts.transition_hints)?,
            backgrounds: changed(backgrounds, &mut facts.backgrounds)?,
            face_fills: changed(face_fills, &mut facts.face_fills)?,
            borders: changed(borders, &mut facts.borders)?,
            cursors: changed(cursors, &mut facts.cursors)?,
            phys_cursor: changed(phys_cursor, &mut facts.phys_cursor)?,
            scroll_bars: changed(scroll_bars, &mut facts.scroll_bars)?,
            chrome: changed(&chrome, &mut facts.chrome)?,
            pointer_source: changed(presented_pointer_source, &mut facts.pointer_source)?,
            regions: changed(
                &NonTextHitIndex::of(presented_hit_index),
                &mut facts.regions,
            )?,
            coverage: changed(&coverage, &mut facts.coverage)?,
            coverage_renewals: changed(scroll_coverage_renewals, &mut facts.coverage_renewals)?,
            frontier: input_checkpoint.clone(),
        };

        let mut ops = Vec::with_capacity(update.defines.len() + 5);
        ops.push(PresentOp::Begin(Box::new(Begin {
            frame,
            presentation: *presentation_id,
            update: update_seq,
            base: if keyframe {
                PresentBase::Keyframe
            } else {
                PresentBase::Delta
            },
            requires: Vec::new(),
        })));
        ops.append(&mut update.defines);
        if keyframe || !tables.is_empty() {
            ops.push(PresentOp::Tables(Box::new(tables)));
        }
        if keyframe || !changed_layouts.is_empty() || order_changed {
            ops.push(PresentOp::Windows(Box::new(WindowsUpdate {
                order,
                changed: changed_layouts,
            })));
        }
        ops.push(PresentOp::Facts(Box::new(frame_facts)));
        ops.push(PresentOp::Commit {
            presentation: *presentation_id,
        });
        Ok(ops)
    }
}
