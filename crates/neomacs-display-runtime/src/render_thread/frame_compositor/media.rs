//! Media surfaces presented by the compositor: video identities visible in the
//! current scene, and the renderer-owned terminal contribution.
//!
//! These methods own `FrameCompositor::visible_videos`, which is why they live
//! inside this module rather than in `frame_windows`.

#[cfg(feature = "video")]
use std::collections::HashSet;

#[cfg(feature = "video")]
use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer};
use crate::render_thread::frame_windows::GuiFrameRenderState;
#[cfg(feature = "neo-term")]
use crate::render_thread::terminal_expansion::{TerminalExpansion, TerminalExpansionUpdate};
#[cfg(feature = "video")]
use neomacs_display_protocol::types::VideoId;

impl GuiFrameRenderState {
    #[cfg(feature = "video")]
    pub(in crate::render_thread) fn refresh_visible_videos(&mut self) {
        fn collect(frame: &FrameGlyphBuffer, output: &mut HashSet<VideoId>) {
            output.extend(frame.glyphs.iter().filter_map(|glyph| match glyph {
                FrameGlyph::Video { video_id, .. } => Some(*video_id),
                _ => None,
            }));
        }

        let mut visible = HashSet::new();
        if let Some(frame) = &self.compositor.current_frame {
            collect(frame, &mut visible);
        }
        for entry in self.compositor.child_frames.frames.values() {
            collect(&entry.frame, &mut visible);
        }
        self.compositor.visible_videos = visible;
    }

    #[cfg(feature = "video")]
    pub(in crate::render_thread) fn presents_video(&self, id: VideoId) -> bool {
        self.compositor.visible_videos.contains(&id)
    }

    #[cfg(feature = "video")]
    pub(in crate::render_thread) fn presented_video_ids(
        &self,
    ) -> impl Iterator<Item = VideoId> + '_ {
        self.compositor.visible_videos.iter().copied()
    }

    /// Atomically replace the complete renderer-owned terminal contribution.
    ///
    /// The editor frame is never mutated. A real change advances the scene
    /// generation, disables row reuse, and requests a full repaint; an
    /// identical replacement has no side effects.
    #[cfg(feature = "neo-term")]
    pub(in crate::render_thread) fn replace_terminal_expansion(
        &mut self,
        next: TerminalExpansion,
    ) -> TerminalExpansionUpdate {
        let Some(frame) = self.compositor.current_frame.as_ref() else {
            self.compositor.terminal_expansion = TerminalExpansion::default();
            return TerminalExpansionUpdate::NoFrame;
        };
        if let Some(face_id) = next
            .faces()
            .keys()
            .find(|face_id| frame.faces.contains_key(face_id))
            .copied()
        {
            tracing::error!(
                face_id = face_id.get(),
                "terminal expansion attempted to replace an editor-owned face"
            );
            return TerminalExpansionUpdate::FaceIdCollision(face_id);
        }
        if self.compositor.terminal_expansion == next {
            return TerminalExpansionUpdate::Unchanged;
        }
        self.compositor.terminal_expansion = next;
        self.compositor.current_scene_generation =
            crate::render_thread::frame_state::next_scene_generation();
        self.compositor.current_row_damage = None;
        self.compositor.dirty = true;
        TerminalExpansionUpdate::Replaced
    }
}
