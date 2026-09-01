//! Renderer-owned NeoTerm glyphs composed over an immutable editor frame.

use crate::core::face::Face;
use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer};
use crate::core::types::FaceId;
use std::collections::HashMap;

/// Renderer-owned face-id namespace reserved for synthesized terminal faces.
pub(super) const TERMINAL_FACE_ID_BASE: u32 = 0xF000_0000;
pub(super) const TERMINAL_FACE_ID_MASK: u32 = 0x0FFF_FFFF;

/// One complete, replaceable NeoTerm contribution to a frame scene.
///
/// Keeping this separate from [`FrameGlyphBuffer`] means render preparation
/// never needs to truncate an editor-owned glyph vector or remember which
/// face-table entries it injected.  Callers build a complete value and replace
/// it atomically; composition happens only on the cloned frame sent to wgpu.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct TerminalExpansion {
    glyphs: Vec<FrameGlyph>,
    faces: HashMap<FaceId, Face>,
}

impl TerminalExpansion {
    pub(super) fn new(glyphs: Vec<FrameGlyph>, faces: HashMap<FaceId, Face>) -> Self {
        Self { glyphs, faces }
    }

    /// Merge another generated layer while preserving a one-to-one mapping
    /// between face ids and face values.
    ///
    /// Independently built layers may choose the same deterministic candidate
    /// id for different face values. Resolve that collision here and rewrite
    /// the incoming glyph references before the layers become one scene.
    pub(super) fn merge(&mut self, mut other: Self) {
        if self.faces.is_empty() {
            self.glyphs.extend(other.glyphs);
            self.faces = other.faces;
            return;
        }
        if other.faces.is_empty() {
            self.glyphs.extend(other.glyphs);
            return;
        }
        let mut incoming_faces: Vec<_> = other.faces.drain().collect();
        incoming_faces.sort_unstable_by_key(|(face_id, _)| face_id.get());
        let mut remapped = HashMap::new();

        for (source_id, mut face) in incoming_faces {
            let mut target_id = source_id;
            loop {
                face.id = target_id;
                match self.faces.get(&target_id) {
                    Some(existing) if existing == &face => break,
                    Some(_) => target_id = next_terminal_face_id(target_id),
                    None => {
                        self.faces.insert(target_id, face);
                        break;
                    }
                }
            }
            if source_id != target_id {
                remapped.insert(source_id, target_id);
            }
        }

        if !remapped.is_empty() {
            for glyph in &mut other.glyphs {
                remap_generated_glyph_face(glyph, &remapped);
            }
        }
        self.glyphs.extend(other.glyphs);
    }

    pub(super) fn faces(&self) -> &HashMap<FaceId, Face> {
        &self.faces
    }

    #[cfg(test)]
    pub(super) fn glyphs(&self) -> &[FrameGlyph] {
        &self.glyphs
    }

    pub(super) fn compose_into(&self, frame: &mut FrameGlyphBuffer) {
        frame.glyphs.extend(self.glyphs.iter().cloned());
        frame
            .faces
            .extend(self.faces.iter().map(|(id, face)| (*id, face.clone())));
    }
}

pub(super) fn next_terminal_face_id(face_id: FaceId) -> FaceId {
    let offset = face_id.get().wrapping_add(1) & TERMINAL_FACE_ID_MASK;
    FaceId::new(TERMINAL_FACE_ID_BASE | offset)
}

fn remap_generated_glyph_face(glyph: &mut FrameGlyph, remapped: &HashMap<FaceId, FaceId>) {
    let Some(face_id) = glyph.face_id_mut() else {
        return;
    };
    if let Some(replacement) = remapped.get(face_id) {
        *face_id = *replacement;
    }
}

/// Observable result of an atomic terminal-expansion replacement.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminalExpansionUpdate {
    /// No editor frame exists to receive the expansion.
    NoFrame,
    /// The complete generated contribution was byte-for-byte unchanged.
    Unchanged,
    /// The visible scene changed and all dependent render state was invalidated.
    Replaced,
    /// A generated face attempted to occupy an editor-owned face slot.
    FaceIdCollision(FaceId),
}

#[cfg(test)]
#[path = "terminal_expansion_test.rs"]
mod tests;
