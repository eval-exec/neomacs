//! Compositor-owned visual state for one top-level GUI frame.
//!
//! The compositor holds the immutable editor scene the render thread is
//! currently presenting, plus every piece of renderer-owned visual state
//! layered over it: retained scene textures, cursor visuals, child-frame
//! presentations, transitions, and media surfaces.
//!
//! GNU-compatible editor state stays authoritative and changes atomically;
//! nothing here mutates window geometry or re-runs Lisp. See
//! `tmp/neomacs-temporal-presentation-architecture.md` for the target
//! ownership model this module is being grown into.

use std::collections::{HashMap, HashSet};

use super::child_frames::ChildFrameManager;
use super::cursor::CursorState;
#[cfg(feature = "neo-term")]
use super::terminal_expansion::TerminalExpansion;
use super::transitions::TransitionState;
use crate::core::frame_glyphs::FrameGlyphBuffer;
#[cfg(feature = "video")]
use neomacs_display_protocol::types::VideoId;
use neomacs_renderer_wgpu::{RendererFrameEffects, WgpuGlyphAtlas};

mod child_frames;
pub(in crate::render_thread) mod continuity;
mod cursor;
mod media;
mod overlays;

/// Glyph composition and rendering state for a frame window.
pub(crate) struct FrameCompositor {
    pub current_frame: Option<FrameGlyphBuffer>,
    /// Video identities present in the root or any accepted child
    /// presentation. Rebuilt only when editor presentation data changes, so
    /// decoder wakeups do not rescan every glyph at video frame rate.
    #[cfg(feature = "video")]
    visible_videos: HashSet<VideoId>,
    /// Unique generation of the composed editor scene. Frame replacement and
    /// renderer-owned terminal replacement both advance it, so face
    /// aggregation and retained-static rendering cannot observe stale state.
    pub current_scene_generation: u64,
    /// Renderer-owned terminal contribution, kept out of the immutable editor
    /// frame and composed only into render clones.
    #[cfg(feature = "neo-term")]
    pub(super) terminal_expansion: TerminalExpansion,
    /// Row damage paired with `current_frame` (built from the same
    /// FrameDisplayState that frame was materialized from). Set only
    /// together with the frame via `set_current_frame` so a summary can
    /// never describe a different frame than the glyphs it accompanies.
    pub current_row_damage: Option<neomacs_renderer_wgpu::FrameRowDamage>,
    pub child_frames: ChildFrameManager,
    hidden_child_frames: HashSet<u64>,
    pub(super) pending_child_frame_removals_to_present: Vec<u64>,
    pub glyph_atlas: Option<WgpuGlyphAtlas>,
    /// Content of the scene changed: the next frame must repaint layers.
    pub dirty: bool,
    /// Only the cursor layer changed (a blink toggle). The composite fast
    /// path reproduces such a frame from the retained scene, so this asks for
    /// a cursor-only frame rather than a repaint. Kept separate from `dirty`
    /// because anything that sets `dirty` outranks it in the demand model.
    pub(super) cursor_dirty: bool,
    pub(super) visual_cursors: HashMap<i64, CursorState>,
    pub renderer_effects: RendererFrameEffects,
    pub transitions: TransitionState,
    /// Retained cursorless static scene for the compositor-only fast path
    /// (frame scheduling plan, Stage 4). Built lazily from the current frame
    /// and reused across cursor-only frames; invalidated on any full render.
    pub(super) retained_static: Option<RetainedStatic>,
    /// Scroll anchors of the presentation currently displayed.
    ///
    /// Retained instead of its glyph rows: a materialized frame carries no
    /// rows, and this is the whole of what measuring a scroll against the next
    /// presentation needs.
    pub(super) scroll_anchors: ScrollAnchorsByWindow,
    /// Facts measured at the most recent install, consumed exactly once.
    pub(super) pending: PendingContinuity,
}

/// What the compositor measured when the current presentation was installed.
///
/// Taken by value when a frame consumes it, not borrowed. Producer effect hints
/// have always been one-shot — `take_runtime_hints` drains them as the frame is
/// taken for render — but these observations were not, and `detect_frame_transitions`
/// runs on *every* render pass, not once per install. A second pass over the same
/// retained presentation would re-arm every effect (each trigger drops its old
/// entry and pushes a fresh start time), and each re-arm reports `needs_redraw`,
/// which marks the frame dirty and schedules another pass. That is a loop that
/// sustains itself with no editor activity at all.
///
/// It is latent today only because scroll transitions default to disabled, so the
/// one existing consumer plans nothing. Draining here is what makes it safe to
/// move further effects onto this path.
#[derive(Default)]
pub(in crate::render_thread) struct PendingContinuity {
    pub(in crate::render_thread) scrolls: Vec<continuity::ScrollObservation>,
    /// Whether this frame's quality plan admits compositor-derived effects —
    /// the role `RenderFeaturePlan::accept_effect_hints` played for producer
    /// hints.
    pub(in crate::render_thread) accept_derived_effects: bool,
}

/// Scroll anchors keyed by the window that offered them.
pub(in crate::render_thread) type ScrollAnchorsByWindow = std::collections::HashMap<
    neomacs_display_protocol::types::DisplayWindowId,
    Vec<continuity::scroll::RowAnchor>,
>;

impl FrameCompositor {
    /// A compositor holding no scene yet.
    ///
    /// `glyph_atlas` is `None` before the wgpu device exists (and again after
    /// device loss); every other field starts empty, so this is the single
    /// construction path for both frame-render-state constructors.
    pub(super) fn new(glyph_atlas: Option<WgpuGlyphAtlas>) -> Self {
        Self {
            current_frame: None,
            #[cfg(feature = "video")]
            visible_videos: HashSet::new(),
            current_scene_generation: 0,
            #[cfg(feature = "neo-term")]
            terminal_expansion: TerminalExpansion::default(),
            current_row_damage: None,
            child_frames: ChildFrameManager::new(),
            hidden_child_frames: HashSet::new(),
            pending_child_frame_removals_to_present: Vec::new(),
            glyph_atlas,
            dirty: false,
            cursor_dirty: false,
            visual_cursors: HashMap::new(),
            renderer_effects: RendererFrameEffects::default(),
            transitions: TransitionState::default(),
            retained_static: None,
            scroll_anchors: ScrollAnchorsByWindow::default(),
            pending: PendingContinuity::default(),
        }
    }
}

/// A cursorless render of the current scene, retained across cursor-only
/// frames so an ambient cursor effect composites over it instead of
/// re-running the glyph pipeline. Validity is generation- and size-keyed.
pub(crate) struct RetainedStatic {
    #[allow(dead_code)]
    pub(super) texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    /// Image-pipeline bind group for blitting the texture to the surface.
    pub(super) bind_group: wgpu::BindGroup,
    /// `current_scene_generation` this was built from; a new scene commit
    /// bumps that stamp and invalidates this.
    pub(super) generation: u64,
    pub(super) width: u32,
    pub(super) height: u32,
    /// Per-filled-box-cursor single-glyph mini-frames, built once per
    /// generation so the composite path re-renders each cursor cell (box plus
    /// inverse-video character) without cloning the frame's font tables every
    /// frame. The box color still cycles: it is recomputed from the frame
    /// sample time inside `emit_cursor_visual`, not baked here.
    pub(super) cursor_cells: Vec<RetainedCursorCell>,
}

/// A retained single-glyph mini-frame for one filled-box cursor cell, plus the
/// physical-pixel scissor rect it is drawn within.
pub(crate) struct RetainedCursorCell {
    pub(super) mini: crate::core::frame_glyphs::FrameGlyphBuffer,
    pub(super) scissor: (u32, u32, u32, u32),
}

impl RetainedStatic {
    pub(super) fn new(
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        bind_group: wgpu::BindGroup,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            texture,
            view,
            bind_group,
            // Sentinel: no valid scene captured yet, forcing a build on the
            // first fast-path frame.
            generation: u64::MAX,
            width,
            height,
            cursor_cells: Vec::new(),
        }
    }
}
