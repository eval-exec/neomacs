use super::{FpsCounter, RenderApp};
use crate::core::face::Face;
use crate::core::frame_glyphs::{DisplaySlotId, FrameGlyph, WindowCursor};
use crate::core::types::FaceId;
use std::collections::{BTreeMap, BTreeSet, HashMap};

const FACE_DIFF_SAMPLE_LIMIT: usize = 10;

/// Next unique generation for a frame scene and its derived render state.
pub(super) fn next_scene_generation() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static GENERATION: AtomicU64 = AtomicU64::new(1);
    GENERATION.fetch_add(1, Ordering::Relaxed)
}

/// Diagnostic classification of a face-table update. Purely observational:
/// no category clears the glyph atlas (entries are content-addressed — see
/// the comment at the update site in `refresh_faces_from_frames`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct FaceChangeSummary {
    pub(super) added: usize,
    pub(super) modified: usize,
    /// Subset of `modified` whose glyph-raster-relevant projection changed
    /// (the fields `glyph_font_identity` hashes: family, file path, weight,
    /// size, attributes, resolved font id). A high steady-state rate here
    /// signals face-id slots flapping between different realizations
    /// upstream — worth fixing for composed-cluster cache locality, not for
    /// correctness.
    pub(super) raster_modified: usize,
    pub(super) removed: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct FaceLabel {
    id: FaceId,
    name: String,
}

#[derive(Debug, PartialEq, Eq)]
struct ModifiedFaceDetail {
    id: FaceId,
    name: String,
    fields: Vec<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct FaceDiffDetails {
    added: Vec<FaceLabel>,
    modified: Vec<ModifiedFaceDetail>,
    removed: Vec<FaceLabel>,
    added_omitted: usize,
    modified_omitted: usize,
    removed_omitted: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct FaceSourceChange {
    frame_id: u64,
    old_ingest_sequences: Vec<u64>,
    new_ingest_sequences: Vec<u64>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct FaceSourceDiffDetails {
    changes: Vec<FaceSourceChange>,
    omitted: usize,
}

#[derive(Debug)]
struct FaceOccurrence {
    face_id: FaceId,
    frame_id: u64,
    face: Face,
    sort_key: String,
}

#[derive(Debug, PartialEq, Eq)]
struct FaceIdConflict {
    face_id: FaceId,
    first_frame_id: u64,
    conflicting_frame_id: u64,
    first_name: String,
    conflicting_name: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct FaceConflictDetails {
    conflicts: Vec<FaceIdConflict>,
    omitted: usize,
}

pub(super) fn summarize_face_changes(
    old: &HashMap<FaceId, Face>,
    new: &HashMap<FaceId, Face>,
) -> FaceChangeSummary {
    use neomacs_renderer_wgpu::glyph_atlas::glyph_font_identity;
    let mut summary = FaceChangeSummary::default();
    for (face_id, face) in new {
        match old.get(face_id) {
            None => summary.added += 1,
            Some(old_face) if old_face != face => {
                summary.modified += 1;
                if glyph_font_identity(Some(old_face)) != glyph_font_identity(Some(face)) {
                    summary.raster_modified += 1;
                }
            }
            Some(_) => {}
        }
    }
    summary.removed = old
        .keys()
        .filter(|face_id| !new.contains_key(face_id))
        .count();
    summary
}

fn face_name(face: &Face) -> String {
    face.lisp_name
        .clone()
        .unwrap_or_else(|| "<anonymous>".to_owned())
}

fn compact_json(value: &serde_json::Value) -> String {
    const VALUE_LIMIT: usize = 160;
    let rendered = value.to_string();
    if rendered.chars().count() <= VALUE_LIMIT {
        return rendered;
    }
    let prefix: String = rendered.chars().take(VALUE_LIMIT).collect();
    format!("{prefix}…")
}

fn changed_face_fields(old: &Face, new: &Face) -> Vec<String> {
    let Ok(serde_json::Value::Object(old_fields)) = serde_json::to_value(old) else {
        return vec!["<unable to serialize old face>".to_owned()];
    };
    let Ok(serde_json::Value::Object(new_fields)) = serde_json::to_value(new) else {
        return vec!["<unable to serialize new face>".to_owned()];
    };
    let field_names: BTreeSet<_> = old_fields.keys().chain(new_fields.keys()).collect();

    field_names
        .into_iter()
        .filter_map(|name| {
            let old_value = old_fields.get(name).unwrap_or(&serde_json::Value::Null);
            let new_value = new_fields.get(name).unwrap_or(&serde_json::Value::Null);
            (old_value != new_value).then(|| {
                format!(
                    "{name}={}->{}",
                    compact_json(old_value),
                    compact_json(new_value)
                )
            })
        })
        .collect()
}

pub(super) fn build_face_diff_details(
    old: &HashMap<FaceId, Face>,
    new: &HashMap<FaceId, Face>,
    limit: usize,
) -> FaceDiffDetails {
    let mut added: Vec<_> = new
        .iter()
        .filter(|(face_id, _)| !old.contains_key(face_id))
        .collect();
    added.sort_unstable_by_key(|(face_id, _)| face_id.get());

    let mut modified: Vec<_> = new
        .iter()
        .filter_map(|(face_id, face)| {
            old.get(face_id)
                .filter(|old_face| *old_face != face)
                .map(|old_face| (face_id, old_face, face))
        })
        .collect();
    modified.sort_unstable_by_key(|(face_id, _, _)| face_id.get());

    let mut removed: Vec<_> = old
        .iter()
        .filter(|(face_id, _)| !new.contains_key(face_id))
        .collect();
    removed.sort_unstable_by_key(|(face_id, _)| face_id.get());

    FaceDiffDetails {
        added_omitted: added.len().saturating_sub(limit),
        modified_omitted: modified.len().saturating_sub(limit),
        removed_omitted: removed.len().saturating_sub(limit),
        added: added
            .into_iter()
            .take(limit)
            .map(|(id, face)| FaceLabel {
                id: *id,
                name: face_name(face),
            })
            .collect(),
        modified: modified
            .into_iter()
            .take(limit)
            .map(|(id, old_face, new_face)| ModifiedFaceDetail {
                id: *id,
                name: face_name(new_face),
                fields: changed_face_fields(old_face, new_face),
            })
            .collect(),
        removed: removed
            .into_iter()
            .take(limit)
            .map(|(id, face)| FaceLabel {
                id: *id,
                name: face_name(face),
            })
            .collect(),
    }
}

pub(super) fn changed_face_sources(
    old: &[(u64, u64)],
    new: &[(u64, u64)],
    limit: usize,
) -> FaceSourceDiffDetails {
    fn by_frame(signature: &[(u64, u64)]) -> BTreeMap<u64, Vec<u64>> {
        let mut result = BTreeMap::<u64, Vec<u64>>::new();
        for &(frame_id, ingest_seq) in signature {
            result.entry(frame_id).or_default().push(ingest_seq);
        }
        result
    }

    let old_by_frame = by_frame(old);
    let new_by_frame = by_frame(new);
    let frame_ids: BTreeSet<_> = old_by_frame
        .keys()
        .chain(new_by_frame.keys())
        .copied()
        .collect();

    let changes: Vec<_> = frame_ids
        .into_iter()
        .filter_map(|frame_id| {
            let old_ingest_sequences = old_by_frame.get(&frame_id).cloned().unwrap_or_default();
            let new_ingest_sequences = new_by_frame.get(&frame_id).cloned().unwrap_or_default();
            (old_ingest_sequences != new_ingest_sequences).then_some(FaceSourceChange {
                frame_id,
                old_ingest_sequences,
                new_ingest_sequences,
            })
        })
        .collect();

    FaceSourceDiffDetails {
        omitted: changes.len().saturating_sub(limit),
        changes: changes.into_iter().take(limit).collect(),
    }
}

fn build_face_conflict_details(
    mut occurrences: Vec<FaceOccurrence>,
    limit: usize,
) -> FaceConflictDetails {
    occurrences.sort_unstable_by(|left, right| {
        (left.face_id.get(), left.frame_id, &left.sort_key).cmp(&(
            right.face_id.get(),
            right.frame_id,
            &right.sort_key,
        ))
    });

    let mut details = FaceConflictDetails::default();
    let mut group_start = 0;
    while group_start < occurrences.len() {
        let face_id = occurrences[group_start].face_id;
        let group_end = occurrences[group_start..]
            .iter()
            .position(|occurrence| occurrence.face_id != face_id)
            .map_or(occurrences.len(), |offset| group_start + offset);
        let first = &occurrences[group_start];
        for conflicting in &occurrences[group_start + 1..group_end] {
            if first.face != conflicting.face {
                if details.conflicts.len() < limit {
                    details.conflicts.push(FaceIdConflict {
                        face_id,
                        first_frame_id: first.frame_id,
                        conflicting_frame_id: conflicting.frame_id,
                        first_name: face_name(&first.face),
                        conflicting_name: face_name(&conflicting.face),
                    });
                } else {
                    details.omitted += 1;
                }
            }
        }
        group_start = group_end;
    }
    details
}

impl RenderApp {
    pub(super) fn prepare_frame_state_for_render(&mut self) {
        #[cfg(feature = "neo-term")]
        self.update_terminals();

        self.process_webkit_frames();
        self.process_shader_surfaces();
        self.synchronize_image_residency();
        self.process_pending_images();
        self.refresh_faces_from_frames();
        self.apply_primary_fallback_visual_cursor_animations();
        self.frame_windows
            .apply_top_level_visual_cursor_animations();
    }

    pub(super) fn update_fps_counter(fps: &mut FpsCounter) {
        if fps.enabled {
            fps.render_start = std::time::Instant::now();
            fps.frame_count += 1;
            let elapsed = fps.last_instant.elapsed();
            if elapsed.as_secs_f32() >= 1.0 {
                fps.display_value = fps.frame_count as f32 / elapsed.as_secs_f32();
                fps.frame_count = 0;
                fps.last_instant = std::time::Instant::now();
            }
        }
    }

    fn for_each_face_source(
        &self,
        mut visit: impl FnMut(u64, u64, &HashMap<FaceId, Face>, Option<&HashMap<FaceId, Face>>),
    ) {
        self.frame_windows
            .for_each_top_level_window(|window_state| {
                let compositor = &window_state.render.compositor;
                if let Some(frame) = compositor.current_frame.as_ref() {
                    visit(
                        frame.frame_placement.frame().get(),
                        compositor.current_scene_generation,
                        &frame.faces,
                        #[cfg(feature = "neo-term")]
                        Some(compositor.terminal_expansion.faces()),
                        #[cfg(not(feature = "neo-term"))]
                        None,
                    );
                }
                for entry in compositor.child_frames.frames.values() {
                    visit(entry.frame_id, entry.ingest_seq, &entry.frame.faces, None);
                }
            });
    }

    fn collect_face_id_conflicts(&self, limit: usize) -> FaceConflictDetails {
        let mut occurrences = Vec::new();
        self.for_each_face_source(|frame_id, _generation, frame_faces, terminal_faces| {
            for (face_id, face) in frame_faces
                .iter()
                .chain(terminal_faces.into_iter().flat_map(HashMap::iter))
            {
                occurrences.push(FaceOccurrence {
                    face_id: *face_id,
                    frame_id,
                    face: face.clone(),
                    sort_key: serde_json::to_string(face).unwrap_or_else(|_| format!("{face:?}")),
                });
            }
        });
        build_face_conflict_details(occurrences, limit)
    }

    fn refresh_faces_from_frames(&mut self) {
        // Cheap change detection first: every editor/terminal scene change and
        // child-frame install stamps a unique generation. The sorted
        // (frame_id, generation) signature therefore identifies the exact
        // face-source set. An unchanged signature means the aggregate face map
        // cannot have changed—the common case for cursor-blink and animation
        // renders, which used to rebuild and clone the whole map on every
        // rendered window.
        let mut signature: Vec<(u64, u64)> = Vec::new();
        self.for_each_face_source(|frame_id, generation, _faces, _terminal_faces| {
            signature.push((frame_id, generation));
        });
        signature.sort_unstable();
        if signature == self.faces_signature {
            return;
        }

        // One traversal covers every top-level window including the primary
        // (for_each_top_level_window iterates the full window map), so the
        // former second primary-window pass was pure duplication - and
        // panicked when no primary window existed.
        let mut faces = std::collections::HashMap::new();
        self.for_each_face_source(|_frame_id, _generation, frame_faces, terminal_faces| {
            for (face_id, face) in frame_faces
                .iter()
                .chain(terminal_faces.into_iter().flat_map(HashMap::iter))
            {
                faces.entry(*face_id).or_insert_with(|| face.clone());
            }
        });

        // NO face-table change clears the glyph atlas. Atlas entries are
        // content-addressed: single-glyph keys hash the raster-relevant face
        // projection (glyph_font_identity: family, file path, weight, size,
        // attributes, resolved font id) and exclude the face id; composed
        // keys carry (face_id, font_identity) together. A recolor cannot
        // touch a coverage mask (color is draw-time vertex data), an added
        // face either shares an identity (correct by definition) or mints
        // fresh keys, and even a face-id slot being reused for a different
        // realization lands on a different key in both directions. GNU keys
        // its font-backend rasters by font, not by realized face, for the
        // same reason. The previous clear-on-any-change here re-rasterized
        // the entire glyph set ~30x/second during typing: font-lock recolors
        // ~36 anonymous realizations per keystroke, and mode-line/default
        // realizations flap across shared face-id slots.
        let summary = summarize_face_changes(&self.faces, &faces);

        if tracing::enabled!(tracing::Level::DEBUG) {
            let details = build_face_diff_details(&self.faces, &faces, FACE_DIFF_SAMPLE_LIMIT);
            let source_changes =
                changed_face_sources(&self.faces_signature, &signature, FACE_DIFF_SAMPLE_LIMIT);
            let conflicts = self.collect_face_id_conflicts(FACE_DIFF_SAMPLE_LIMIT);
            tracing::debug!(
                event = "face_table_updated",
                faces_old = self.faces.len(),
                faces_new = faces.len(),
                faces_added = summary.added,
                faces_modified = summary.modified,
                faces_raster_modified = summary.raster_modified,
                faces_removed = summary.removed,
                added = ?details.added,
                added_omitted = details.added_omitted,
                modified = ?details.modified,
                modified_omitted = details.modified_omitted,
                removed = ?details.removed,
                removed_omitted = details.removed_omitted,
                source_changes = ?source_changes.changes,
                source_changes_omitted = source_changes.omitted,
                face_id_conflicts = ?conflicts.conflicts,
                face_id_conflicts_omitted = conflicts.omitted,
                "face-table source update (glyph atlases kept: entries are content-addressed)"
            );
        }
        self.faces = faces;
        self.faces_signature = signature;
    }

    fn apply_primary_fallback_visual_cursor_animations(&mut self) {
        if let Some(primary_frame) = self
            .frame_windows
            .primary_window_mut()
            .map(|ws| &mut ws.render)
        {
            primary_frame.apply_visual_cursor_animations();
        }
    }
}

impl RenderApp {
    pub(super) fn apply_extra_spacing(
        glyphs: &mut [FrameGlyph],
        cursors: &mut [WindowCursor],
        line_spacing: f32,
        letter_spacing: f32,
    ) {
        let mut last_y: f32 = f32::NEG_INFINITY;
        let mut row_index: i32 = -1;
        let mut char_in_row: i32 = 0;
        let mut last_window_y: f32 = f32::NEG_INFINITY;
        let mut slot_positions: HashMap<DisplaySlotId, (f32, f32)> = HashMap::new();

        for glyph in glyphs.iter_mut() {
            match glyph {
                FrameGlyph::Char {
                    x,
                    y,
                    row_role,
                    slot_id,
                    ..
                } => {
                    if row_role.is_chrome() {
                        continue;
                    }
                    if *y < last_window_y - 1.0 {
                        row_index = -1;
                        last_y = f32::NEG_INFINITY;
                    }
                    last_window_y = *y;

                    if (*y - last_y).abs() > 0.5 {
                        row_index += 1;
                        char_in_row = 0;
                        last_y = *y;
                    } else {
                        char_in_row += 1;
                    }
                    *y += row_index as f32 * line_spacing;
                    *x += char_in_row as f32 * letter_spacing;
                    slot_positions.insert(*slot_id, (*x, *y));
                }
                FrameGlyph::Stretch {
                    x,
                    y,
                    row_role,
                    slot_id,
                    ..
                } => {
                    if row_role.is_chrome() {
                        continue;
                    }
                    if *y < last_window_y - 1.0 {
                        row_index = -1;
                        last_y = f32::NEG_INFINITY;
                    }
                    last_window_y = *y;

                    if (*y - last_y).abs() > 0.5 {
                        row_index += 1;
                        char_in_row = 0;
                        last_y = *y;
                    } else {
                        char_in_row += 1;
                    }
                    *y += row_index as f32 * line_spacing;
                    *x += char_in_row as f32 * letter_spacing;
                    slot_positions.insert(*slot_id, (*x, *y));
                }
                FrameGlyph::Image {
                    x,
                    y,
                    row_role,
                    slot_id,
                    ..
                }
                | FrameGlyph::Video {
                    x,
                    y,
                    row_role,
                    slot_id,
                    ..
                }
                | FrameGlyph::Surface {
                    x,
                    y,
                    row_role,
                    slot_id,
                    ..
                } => {
                    if row_role.is_chrome() {
                        continue;
                    }
                    let Some(slot_id) = *slot_id else {
                        continue;
                    };
                    if *y < last_window_y - 1.0 {
                        row_index = -1;
                        last_y = f32::NEG_INFINITY;
                    }
                    last_window_y = *y;

                    if (*y - last_y).abs() > 0.5 {
                        row_index += 1;
                        char_in_row = 0;
                        last_y = *y;
                    } else {
                        char_in_row += 1;
                    }
                    *y += row_index as f32 * line_spacing;
                    *x += char_in_row as f32 * letter_spacing;
                    slot_positions.insert(slot_id, (*x, *y));
                }
                FrameGlyph::Xwidget {
                    presentation,
                    row_role,
                    slot_id,
                    ..
                } => {
                    if row_role.is_chrome() {
                        continue;
                    }
                    let Some(slot_id) = *slot_id else {
                        continue;
                    };
                    let origin = *presentation.origin();
                    let mut x = origin.x();
                    let mut y = origin.y();
                    if y < last_window_y - 1.0 {
                        row_index = -1;
                        last_y = f32::NEG_INFINITY;
                    }
                    last_window_y = y;

                    if (y - last_y).abs() > 0.5 {
                        row_index += 1;
                        char_in_row = 0;
                        last_y = y;
                    } else {
                        char_in_row += 1;
                    }
                    let dx = char_in_row as f32 * letter_spacing;
                    let dy = row_index as f32 * line_spacing;
                    x += dx;
                    y += dy;
                    let displacement = neomacs_display_protocol::SpaceTranslation::<
                        neomacs_display_protocol::FrameSpace,
                        neomacs_display_protocol::FrameSpace,
                        neomacs_display_protocol::LogicalPixels,
                    >::from_px(dx, dy)
                    .expect("finite display spacing produces a valid displacement");
                    *presentation = presentation
                        .translated_origin(displacement)
                        .expect("finite display spacing preserves valid xwidget geometry");
                    slot_positions.insert(slot_id, (x, y));
                }
                _ => {}
            }
        }

        // The active (selected window's) cursor is now in this list, so the
        // single loop adjusts it alongside the decorative cursors.
        for cursor in cursors.iter_mut() {
            if let Some((x, y)) = slot_positions.get(&cursor.slot_id).copied() {
                cursor.x = x;
                cursor.y = y;
            }
        }
    }
}

#[cfg(test)]
#[path = "frame_state_test.rs"]
mod tests;
