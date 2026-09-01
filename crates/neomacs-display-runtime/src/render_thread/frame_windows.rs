//! GUI frame window management for the render thread.
//!
//! GNU Emacs treats every top-level GUI frame as a native window. This module
//! owns the render-thread mapping between Emacs frame IDs and winit windows so
//! redraw, input, resize, focus, and destruction can be frame-addressed instead
//! of primary-window-addressed.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Fullscreen, Window, WindowId};

use super::child_frames::ChildFrameManager;
use super::cursor::{CursorState, CursorTarget};
use super::state::{
    FpsCounter, GuiChromeInteractionState, IdleDimState, ImeCursorArea, PendingPointerDamage,
    PointerAppearanceState, PresentedInteractionKey, PresentedPointerHit, PresentedPressCapture,
    TypingSpeedState, WindowChrome, effective_window_scale_factor, window_size_from_emacs_pixels,
};
#[cfg(feature = "neo-term")]
use super::terminal_expansion::{TerminalExpansion, TerminalExpansionUpdate};
use super::transitions::{TransitionState, clear_frame_transition_textures};
use super::x11_hints::apply_window_geometry_hints;
#[cfg(feature = "video")]
use crate::core::frame_glyphs::FrameGlyph;
use crate::core::frame_glyphs::FrameGlyphBuffer;
use neomacs_display_protocol::effect_config::IdleDimConfig;
#[cfg(feature = "video")]
use neomacs_display_protocol::types::VideoId;
use neomacs_display_protocol::{
    DeviceScale, FrameRect, GeometrySize, LogicalPixels, PresentMapping, PresentationExtent,
    PresentationId, PresentedHit, PresentedHitError, PresentedHitQuery, RetainedImageSet,
    SurfaceState,
};
use neomacs_renderer_wgpu::{
    PopupMenuState, RendererFrameEffects, TooltipState, WgpuGlyphAtlas, WgpuRenderer,
};
use neovm_core::window::GuiFrameGeometryHints;

use crate::thread_comm::WindowFullscreenMode;

/// Native window/surface state for a top-level GUI frame.
pub(crate) struct GuiFrameNativeWindowState {
    pub window: Arc<Window>,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    /// Borderless native-window chrome state for this frame window.
    pub(super) chrome: WindowChrome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeTextInputPolicy {
    pub(super) ime_allowed_on_create: bool,
    pub(super) initial_cursor_area: ImeCursorArea,
}

impl NativeTextInputPolicy {
    pub(super) fn for_gui_frame() -> Self {
        Self {
            ime_allowed_on_create: true,
            initial_cursor_area: ImeCursorArea {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
        }
    }

    pub(super) fn apply_to_window(self, window: &Window) {
        window.set_ime_allowed(self.ime_allowed_on_create);
        window.set_ime_cursor_area(
            PhysicalPosition::new(
                self.initial_cursor_area.x as f64,
                self.initial_cursor_area.y as f64,
            ),
            PhysicalSize::new(
                self.initial_cursor_area.width as f64,
                self.initial_cursor_area.height as f64,
            ),
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ActivePresentationTransition {
    pub activated: neomacs_display_protocol::PresentationId,
    pub replaced: Option<neomacs_display_protocol::PresentationId>,
}

impl ActivePresentationTransition {
    pub(super) fn between(
        previous: Option<neomacs_display_protocol::PresentationId>,
        current: neomacs_display_protocol::PresentationId,
    ) -> Option<Self> {
        if current.get() == 0 || previous == Some(current) {
            return None;
        }
        Some(Self {
            activated: current,
            replaced: previous.filter(|presentation| presentation.get() != 0),
        })
    }
}

/// Frame-owned render, input, overlay, and transient visual state.
pub(crate) struct GuiFrameRenderState {
    /// The Emacs frame_id that owns this window (used for routing).
    pub emacs_frame_id: u64,
    /// Chromeless glyph composition and rendering state.
    pub compositor: FrameCompositor,
    /// The one resolved relationship between the live root surface and the
    /// immutable root presentation. Updated only at surface/presentation edges.
    present_state: GuiFramePresentState,
    /// GUI chrome (menu bar, tool bar, compact bar) for this frame window.
    pub chrome: ChromeState,
    /// Snapshot-qualified visual state selected from displayed pointer maps.
    pub(super) pointer_appearance: PointerAppearanceState,
    presented_press: Option<PresentedPressCapture>,
    /// Surface-space paint clips invalidated by transient pointer transitions.
    pending_pointer_damage: [Option<PendingPointerDamage>; 2],
    #[cfg(test)]
    pointer_damage_appearance_lookups: usize,
    /// Retirements pinned while a presented interaction still references them.
    deferred_pointer_retirements: Vec<u64>,
    /// Transient renderer-owned overlays (popup, tooltip, bell, fps, typing, idle).
    pub overlays: OverlayState,
    /// Intermediate composition target while a full-frame post shader is
    /// installed: the whole frame renders here, then the post pass shades it
    /// into the swapchain as the final step. Recreated on resize.
    pub(super) frame_post_src: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// The current native input-method composition, if any.
    ///
    /// `Option` is the active-state invariant: a preedit cannot be "active"
    /// while carrying an empty or stale string.  Each native preedit event
    /// atomically replaces this complete value.
    pub(super) input_method: InputMethodState,
    /// Text cursor animation and blink state for this frame window.
    pub(super) cursor: CursorState,
    /// Last known pointer position in this frame's logical coordinates.
    pub mouse_pos: (f32, f32),
    /// Whether the native window currently owns the pointer. The last position
    /// remains useful for input coordinates after leave, but must not reactivate
    /// a visual range on a captured release.
    pub(super) pointer_inside: bool,
}

/// Compile-time separation of a suspended surface from drawable geometry.
/// A drawable surface may await its first presentation, but it can never carry
/// a mapping resolved for a different surface.
#[derive(Clone, Copy, Debug, PartialEq)]
enum GuiFramePresentState {
    Suspended,
    Drawable {
        surface: neomacs_display_protocol::DrawableSurface,
        mapping: Option<PresentMapping>,
    },
}

/// GUI chrome state for a frame window.
#[derive(Default)]
pub(crate) struct ChromeState {
    pub interaction: GuiChromeInteractionState,
}

/// Transient overlay state for a frame window.
pub(crate) struct OverlayState {
    pub popup_menu: Option<PopupMenuState>,
    pub tooltip: Option<TooltipState>,
    pub visual_bell_start: Option<Instant>,
    pub(super) fps: FpsCounter,
    pub(super) typing_speed: TypingSpeedState,
    pub(super) idle_dim: IdleDimState,
}

/// One complete native input-method preedit update.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImePreedit {
    pub text: String,
    /// UTF-8 byte offsets supplied by winit.  A zero-width range is the IME
    /// caret; a non-empty range is the IME selection.
    pub cursor_range: Option<(usize, usize)>,
}

/// Native input-method state, kept separate from renderer-owned overlays.
/// Its methods are the only way to transition between inactive and preedit.
#[derive(Default)]
pub(super) struct InputMethodState {
    preedit: Option<ImePreedit>,
}

impl InputMethodState {
    pub(super) fn replace_preedit(&mut self, text: String, cursor_range: Option<(usize, usize)>) {
        self.preedit = (!text.is_empty()).then_some(ImePreedit { text, cursor_range });
    }

    pub(super) fn clear(&mut self) {
        self.preedit = None;
    }

    pub(super) fn preedit(&self) -> Option<&ImePreedit> {
        self.preedit.as_ref()
    }

    pub(super) fn has_preedit(&self) -> bool {
        self.preedit.is_some()
    }
}

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

impl ChromeState {
    pub fn dismiss_menus(&mut self) {
        self.interaction.menu_bar_active = None;
        self.interaction.compact_bar_menu_active = None;
    }

    /// Apply a mouse press on a chrome hit during a popup interaction.
    /// Dismisses popup-related state and records the interaction.
    pub fn press_with_popup(&mut self, press: &ChromePress) {
        self.interaction.menu_bar_active = None;
        self.interaction.compact_bar_menu_active = None;
        match press {
            ChromePress::MenuBar(idx) => self.interaction.menu_bar_active = Some(*idx),
            ChromePress::ToolBar(idx) => self.interaction.toolbar_pressed = Some(*idx),
        }
    }
}

/// Result of a chrome interaction press.
#[derive(Debug, Clone, Copy)]
// The shared `Bar` suffix is domain-meaningful (menu/tool/tab BARS); renaming the
// variants would obscure intent, so the naming lint is allowed here.
#[allow(clippy::enum_variant_names)]
pub(crate) enum ChromePress {
    MenuBar(u32),
    ToolBar(u32),
}

/// Per-window state for a top-level GUI frame.
///
/// The frame window lifecycle is modeled as an explicit state machine.
/// Before `resumed`, operations queue into [`FrameLifecycle::Pending`].
/// After the winit window is created, they apply directly through
/// [`FrameLifecycle::Active`].
pub(crate) struct GuiFrameWindowState {
    pub(super) lifecycle: FrameLifecycle,
    pub render: GuiFrameRenderState,
}

/// Window lifecycle state machine.
///
/// Mirrors GNU Emacs's frame lifecycle: created, mapped (active),
/// unmapped / destroyed.  Operations are deferred until the native
/// window exists, eliminating ad-hoc `if native.is_some()` checks
/// scattered across ~30 methods.
pub(super) enum FrameLifecycle {
    /// Window before `resumed` — all operations queue here.
    Pending {
        width: u32,
        height: u32,
        scale_factor: f64,
        mouse_hidden_for_typing: bool,
        ime_enabled: bool,
        last_ime_cursor_area: Option<ImeCursorArea>,
        chrome: WindowChrome,
        geometry_hints: Option<GuiFrameGeometryHints>,
    },
    /// Window with a live winit native window and wgpu surface.
    Active {
        native: GuiFrameNativeWindowState,
        mouse_hidden_for_typing: bool,
        ime_enabled: bool,
        last_ime_cursor_area: Option<ImeCursorArea>,
    },
}

impl GuiFrameRenderState {
    fn extend_retained_images(&self, retained: &mut RetainedImageSet) {
        if let Some(frame) = &self.compositor.current_frame {
            retained.extend(frame.referenced_images());
        }
        for entry in self.compositor.child_frames.frames.values() {
            retained.extend(entry.frame.referenced_images());
        }
    }

    #[cfg(feature = "video")]
    fn refresh_visible_videos(&mut self) {
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
    pub(super) fn presents_video(&self, id: VideoId) -> bool {
        self.compositor.visible_videos.contains(&id)
    }

    #[cfg(feature = "video")]
    pub(super) fn presented_video_ids(&self) -> impl Iterator<Item = VideoId> + '_ {
        self.compositor.visible_videos.iter().copied()
    }

    pub(super) fn new(
        emacs_frame_id: u64,
        device: &wgpu::Device,
        scale_factor: f64,
        fps_enabled: bool,
    ) -> Self {
        Self {
            emacs_frame_id,
            compositor: FrameCompositor {
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
                glyph_atlas: Some(WgpuGlyphAtlas::new_with_scale(device, scale_factor as f32)),
                dirty: false,
                cursor_dirty: false,
                visual_cursors: HashMap::new(),
                renderer_effects: RendererFrameEffects::default(),
                transitions: TransitionState::default(),
                retained_static: None,
            },
            present_state: GuiFramePresentState::Suspended,
            chrome: ChromeState::default(),
            pointer_appearance: PointerAppearanceState::default(),
            presented_press: None,
            pending_pointer_damage: [None; 2],
            #[cfg(test)]
            pointer_damage_appearance_lookups: 0,
            deferred_pointer_retirements: Vec::new(),
            overlays: OverlayState {
                popup_menu: None,
                tooltip: None,
                visual_bell_start: None,
                fps: FpsCounter {
                    enabled: fps_enabled,
                    ..FpsCounter::default()
                },
                typing_speed: TypingSpeedState::default(),
                idle_dim: IdleDimState::default(),
            },
            frame_post_src: None,
            input_method: InputMethodState::default(),
            cursor: CursorState::default(),
            mouse_pos: (0.0, 0.0),
            pointer_inside: false,
        }
    }

    pub(super) fn new_without_device(emacs_frame_id: u64, fps_enabled: bool) -> Self {
        Self {
            emacs_frame_id,
            compositor: FrameCompositor {
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
                glyph_atlas: None,
                dirty: false,
                cursor_dirty: false,
                visual_cursors: HashMap::new(),
                renderer_effects: RendererFrameEffects::default(),
                transitions: TransitionState::default(),
                retained_static: None,
            },
            present_state: GuiFramePresentState::Suspended,
            chrome: ChromeState::default(),
            pointer_appearance: PointerAppearanceState::default(),
            presented_press: None,
            pending_pointer_damage: [None; 2],
            #[cfg(test)]
            pointer_damage_appearance_lookups: 0,
            deferred_pointer_retirements: Vec::new(),
            overlays: OverlayState {
                popup_menu: None,
                tooltip: None,
                visual_bell_start: None,
                fps: FpsCounter {
                    enabled: fps_enabled,
                    ..FpsCounter::default()
                },
                typing_speed: TypingSpeedState::default(),
                idle_dim: IdleDimState::default(),
            },
            frame_post_src: None,
            input_method: InputMethodState::default(),
            cursor: CursorState::default(),
            mouse_pos: (0.0, 0.0),
            pointer_inside: false,
        }
    }

    pub(super) fn populate_glyph_atlas(&mut self, device: &wgpu::Device, scale_factor: f64) {
        if self.compositor.glyph_atlas.is_none() {
            self.compositor.glyph_atlas =
                Some(WgpuGlyphAtlas::new_with_scale(device, scale_factor as f32));
        }
    }

    fn mapping_for_current_frame(
        &self,
        surface: neomacs_display_protocol::DrawableSurface,
    ) -> Option<PresentMapping> {
        let frame = self.compositor.current_frame.as_ref()?;
        let logical_size =
            GeometrySize::<LogicalPixels>::from_px(frame.width, frame.height).ok()?;
        Some(PresentMapping::top_left_clip(
            surface,
            PresentationExtent::new(frame.presentation_id, logical_size),
        ))
    }

    pub(super) fn set_surface_state(&mut self, state: SurfaceState) {
        self.present_state = match state {
            SurfaceState::Suspended => GuiFramePresentState::Suspended,
            SurfaceState::Drawable(surface) => GuiFramePresentState::Drawable {
                surface,
                mapping: self.mapping_for_current_frame(surface),
            },
        };
    }

    fn refresh_present_mapping(&mut self) {
        let GuiFramePresentState::Drawable { surface, .. } = self.present_state else {
            return;
        };
        self.present_state = GuiFramePresentState::Drawable {
            surface,
            mapping: self.mapping_for_current_frame(surface),
        };
    }

    pub(super) const fn present_mapping(&self) -> Option<PresentMapping> {
        match self.present_state {
            GuiFramePresentState::Suspended => None,
            GuiFramePresentState::Drawable { mapping, .. } => mapping,
        }
    }

    pub(super) fn root_frame_point_from_surface(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let surface_point = neomacs_display_protocol::GeometryPoint::<
            neomacs_display_protocol::RootSurfaceSpace,
            LogicalPixels,
        >::from_px(x, y)
        .ok()?;
        self.present_mapping()?
            .frame_from_surface(surface_point)
            .map(|point| (point.x(), point.y()))
    }

    // Retained for focused render-state tests; production callers inspect the
    // retained frame through narrower accessors.
    #[allow(dead_code)]
    pub(super) fn current_frame_clone(&self) -> Option<FrameGlyphBuffer> {
        let mut frame = self.compositor.current_frame.clone()?;
        #[cfg(feature = "neo-term")]
        self.compositor.terminal_expansion.compose_into(&mut frame);
        Some(frame)
    }

    /// Whether the retained frame carries a theme-transition effect hint —
    /// `None` when there is no retained frame at all, so a caller can use `?`
    /// exactly as it would with [`Self::current_frame_clone`].
    ///
    /// The offscreen-compositing decision needs this single bit, and cloning a
    /// whole `FrameGlyphBuffer` (glyph vector, window info, cursors, hints,
    /// per-window effects map) to read it cost a full copy on every present,
    /// including cursor-only ones that never look at the glyphs.
    ///
    /// Must be consulted BEFORE [`Self::take_current_frame_for_render`], which
    /// drains the hints out of the retained frame.
    pub(super) fn current_frame_theme_transition_hint(&self) -> Option<bool> {
        self.compositor.current_frame.as_ref().map(|frame| {
            frame.effect_hints.iter().any(|hint| {
                matches!(
                    hint,
                    crate::core::frame_glyphs::WindowEffectHint::ThemeTransition { .. }
                )
            })
        })
    }

    /// Hit-tests pointer semantics from the immutable buffer currently shown
    /// for `target_frame_id`. Coordinates are local to that target frame.
    pub(super) fn presented_pointer_hit(
        &self,
        target_frame_id: u64,
        x: f32,
        y: f32,
    ) -> Result<Option<PresentedPointerHit>, PresentedHitError> {
        let frame = if target_frame_id == self.emacs_frame_id {
            self.compositor.current_frame.as_ref()
        } else {
            self.compositor
                .child_frames
                .frames
                .get(&target_frame_id)
                .map(|entry| &entry.frame)
        };
        let Some(frame) = frame else {
            return Ok(None);
        };
        let Some(hit) =
            frame.resolve_presented_hit(PresentedHitQuery::new(frame.presentation_id, x, y))?
        else {
            return Ok(None);
        };
        Ok(Some(PresentedPointerHit::new(
            target_frame_id,
            frame.presentation_id,
            hit.interaction(),
            hit.appearance(),
        )))
    }

    /// Resolve semantic geometry from the exact immutable frame presentation.
    pub(super) fn presented_region_hit(
        &self,
        target_frame_id: u64,
        presentation: PresentationId,
        x: f32,
        y: f32,
    ) -> Result<Option<PresentedHit>, PresentedHitError> {
        let frame = if target_frame_id == self.emacs_frame_id {
            self.compositor.current_frame.as_ref()
        } else {
            self.compositor
                .child_frames
                .frames
                .get(&target_frame_id)
                .map(|entry| &entry.frame)
        };
        let Some(frame) = frame else {
            return Ok(None);
        };
        frame
            .resolve_presented_hit(PresentedHitQuery::new(presentation, x, y))
            .map(|hit| hit.and_then(|hit| hit.semantic()))
    }

    pub(super) fn presented_region_observation(
        &self,
        target_frame_id: u64,
        x: f32,
        y: f32,
    ) -> Result<Option<(PresentationId, Option<PresentedHit>)>, PresentedHitError> {
        let presentation = if target_frame_id == self.emacs_frame_id {
            let Some(frame) = self.compositor.current_frame.as_ref() else {
                return Ok(None);
            };
            frame.presentation_id
        } else {
            let Some(frame) = self
                .compositor
                .child_frames
                .frames
                .get(&target_frame_id)
                .map(|entry| &entry.frame)
            else {
                return Ok(None);
            };
            frame.presentation_id
        };
        let hit = self.presented_region_hit(target_frame_id, presentation, x, y)?;
        Ok(Some((presentation, hit)))
    }

    fn presented_pointer_appearance_at(
        &self,
        target: Option<(u64, f32, f32)>,
    ) -> Option<super::state::PresentedAppearanceKey> {
        let (target_frame_id, x, y) = target?;
        match self.presented_pointer_hit(target_frame_id, x, y) {
            Ok(hit) => hit.and_then(|hit| hit.appearance_key()),
            Err(error) => {
                tracing::error!(?error, "rejecting incoherent presented pointer hit");
                None
            }
        }
    }

    fn active_pointer_damage(&mut self) -> Option<PendingPointerDamage> {
        let active = self.pointer_appearance.active()?;
        let key = active.key();
        let frame_and_offset = if key.frame_id() == 0 || key.frame_id() == self.emacs_frame_id {
            self.compositor
                .current_frame
                .as_ref()
                .map(|frame| (frame, 0.0, 0.0))
        } else {
            self.compositor
                .child_frames
                .frames
                .get(&key.frame_id())
                .map(|entry| (&entry.frame, entry.abs_x, entry.abs_y))
        };
        let (frame, offset_x, offset_y) = frame_and_offset?;
        if frame.presentation_id != active.presentation() {
            return None;
        }
        #[cfg(test)]
        {
            self.pointer_damage_appearance_lookups += 1;
        }
        let bounds = frame
            .presented_pointer()
            .appearance(active.appearance())?
            .damage_bounds();
        let rect = FrameRect::new(
            bounds.x() + offset_x,
            bounds.y() + offset_y,
            bounds.width(),
            bounds.height(),
        )
        .ok()?;
        Some(PendingPointerDamage::new(key, rect))
    }

    fn invalidate_pointer_damage_rows(&mut self, damage: PendingPointerDamage) {
        let key = damage.key();
        if key.frame_id() != 0 && key.frame_id() != self.emacs_frame_id {
            return;
        }
        let (Some(frame), Some(row_damage)) = (
            self.compositor.current_frame.as_ref(),
            self.compositor.current_row_damage.as_mut(),
        ) else {
            return;
        };
        if frame.presentation_id != key.presentation() {
            return;
        }
        if let Some(appearance) = frame.presented_pointer().appearance(key.appearance()) {
            row_damage.invalidate_pointer_rows(appearance.damage_rows());
        }
    }

    fn record_pointer_paint_transition(&mut self, before: Option<PendingPointerDamage>) {
        let after = self.active_pointer_damage();
        if let Some(damage) = before {
            self.invalidate_pointer_damage_rows(damage);
        }
        if let Some(damage) = after {
            self.invalidate_pointer_damage_rows(damage);
        }

        if self.pending_pointer_damage.iter().all(Option::is_none) {
            self.pending_pointer_damage[0] = before.or(after);
        }
        let initial = self.pending_pointer_damage[0];
        self.pending_pointer_damage[1] = after.filter(|damage| Some(*damage) != initial);
    }

    #[cfg(test)]
    pub(super) fn pointer_paint_damage(&self) -> [Option<FrameRect>; 2] {
        self.pending_pointer_damage
            .map(|damage| damage.map(PendingPointerDamage::rect))
    }

    #[cfg(test)]
    pub(super) const fn pointer_damage_appearance_lookups(&self) -> usize {
        self.pointer_damage_appearance_lookups
    }

    pub(super) fn finish_pointer_paint_render(&mut self) {
        self.pending_pointer_damage = [None; 2];
    }

    pub(super) fn has_pointer_paint_damage(&self) -> bool {
        self.pending_pointer_damage.iter().any(Option::is_some)
    }

    pub(super) fn capture_presented(&mut self, target: Option<PresentedInteractionKey>) {
        self.presented_press = Some(PresentedPressCapture::new(target));
    }

    pub(super) fn capture_presented_at(
        &mut self,
        target: PresentedInteractionKey,
        surface_origin: (f32, f32),
    ) {
        self.presented_press = Some(PresentedPressCapture::with_surface_origin(
            target,
            surface_origin,
        ));
    }

    pub(super) const fn presented_capture(&self) -> Option<PresentedPressCapture> {
        self.presented_press
    }

    pub(super) fn take_presented_capture(&mut self) -> Option<PresentedPressCapture> {
        self.presented_press.take()
    }

    pub(super) fn clear_presented_capture(&mut self) {
        self.presented_press = None;
    }

    /// Apply pointer motion to the immutable root or child presentation that
    /// currently owns the hit. Returns whether the selected paint changed.
    pub(super) fn update_presented_pointer_motion(
        &mut self,
        target: Option<(u64, f32, f32)>,
    ) -> bool {
        let appearance = self.presented_pointer_appearance_at(target);
        if !self.pointer_appearance.hover_would_change(appearance) {
            return false;
        }
        let before = self.active_pointer_damage();
        let changed = self.pointer_appearance.hover(appearance);
        if changed {
            self.record_pointer_paint_transition(before);
            self.mark_dirty();
        }
        changed
    }

    /// Apply a primary-button phase after resolving hover from the same
    /// displayed snapshot. Input capture remains owned by the interaction
    /// pipeline; this operation changes only renderer-facing appearance.
    pub(super) fn update_presented_pointer_button(
        &mut self,
        target: Option<(u64, f32, f32)>,
        pressed: bool,
    ) -> bool {
        let appearance = self.presented_pointer_appearance_at(target);
        if !self
            .pointer_appearance
            .button_would_change(appearance, pressed)
        {
            return false;
        }
        let before = self.active_pointer_damage();
        let mut changed = self.pointer_appearance.hover(appearance);
        changed |= if pressed {
            self.pointer_appearance.press()
        } else {
            self.pointer_appearance.release()
        };
        if changed {
            self.record_pointer_paint_transition(before);
            self.mark_dirty();
        }
        changed
    }

    /// Resolve the active runtime appearance only when it belongs to this
    /// exact immutable frame snapshot. Renderer entry points use this seam so
    /// stale state can never become an unqualified appearance ID.
    pub(super) fn pointer_selection_for(
        &self,
        frame: &FrameGlyphBuffer,
    ) -> Option<neomacs_display_protocol::PointerAppearanceSelection> {
        self.pointer_appearance.selection_for(frame)
    }

    pub(super) fn font_metrics(&self) -> (f32, f32, f32) {
        self.compositor
            .glyph_atlas
            .as_ref()
            .map_or((13.0, 17.0, 13.0 * 0.6), |atlas| {
                (
                    atlas.default_font_size(),
                    atlas.default_line_height(),
                    atlas.default_char_width(),
                )
            })
    }

    pub(super) fn set_popup_menu(&mut self, popup_menu: Option<PopupMenuState>) {
        if popup_menu.is_some() {
            self.update_presented_pointer_motion(None);
        }
        self.overlays.popup_menu = popup_menu;
        self.compositor.dirty = true;
    }

    pub(super) fn set_tooltip(&mut self, tooltip: Option<TooltipState>) {
        self.overlays.tooltip = tooltip;
        self.compositor.dirty = true;
    }

    /// Atomically replace the complete renderer-owned terminal contribution.
    ///
    /// The editor frame is never mutated. A real change advances the scene
    /// generation, disables row reuse, and requests a full repaint; an
    /// identical replacement has no side effects.
    #[cfg(feature = "neo-term")]
    pub(super) fn replace_terminal_expansion(
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
        self.compositor.current_scene_generation = super::frame_state::next_scene_generation();
        self.compositor.current_row_damage = None;
        self.compositor.dirty = true;
        TerminalExpansionUpdate::Replaced
    }

    pub(super) fn set_visual_bell_start(&mut self, start: Option<Instant>) {
        self.overlays.visual_bell_start = start;
        if start.is_some() {
            self.compositor.dirty = true;
        }
    }

    pub(super) fn set_ime_preedit(&mut self, text: String, cursor_range: Option<(usize, usize)>) {
        self.input_method.replace_preedit(text, cursor_range);
        self.compositor.dirty = true;
    }

    pub(super) fn clear_ime_preedit(&mut self) {
        self.input_method.clear();
        self.compositor.dirty = true;
    }

    pub(super) fn has_ime_preedit(&self) -> bool {
        self.input_method.has_preedit()
    }

    pub(super) fn record_typing_keypress(&mut self, now: Instant) {
        self.overlays.typing_speed.key_press_times.push(now);
        self.compositor.dirty = true;
    }

    pub(super) fn record_idle_activity(&mut self, now: Instant) {
        self.overlays.idle_dim.last_activity_time = now;
        self.compositor.dirty = true;
    }

    pub(super) fn dismiss_all_chrome_menus(&mut self) {
        self.overlays.popup_menu = None;
        self.chrome.interaction.menu_bar_active = None;
        self.chrome.interaction.compact_bar_menu_active = None;
        self.mark_dirty();
    }

    pub(super) fn mark_dirty(&mut self) {
        self.compositor.dirty = true;
    }

    pub(super) fn has_presentable_dirty_content(&self) -> bool {
        self.compositor.dirty && self.compositor.current_frame.is_some()
    }

    /// Whether only the cursor layer needs to reach the screen. Never true at
    /// the same time as a content repaint is owed: the caller checks
    /// [`Self::has_presentable_dirty_content`] first and that wins.
    pub(super) fn has_presentable_cursor_change(&self) -> bool {
        self.compositor.cursor_dirty && self.compositor.current_frame.is_some()
    }

    pub(super) fn begin_presentable_render(&mut self) {
        self.compositor.dirty = false;
        self.compositor.cursor_dirty = false;
    }

    pub(super) fn set_dirty(&mut self, dirty: bool) {
        self.compositor.dirty = dirty;
        if dirty {
            return;
        }
        // Starting a frame satisfies the cursor layer too: every render path
        // draws the cursor at its current blink state.
        self.compositor.cursor_dirty = false;
    }

    pub(super) fn clear_all_chrome_pressed(&mut self) {
        self.clear_presented_capture();
        self.chrome.interaction.compact_bar_tool_pressed = None;
        self.chrome.interaction.toolbar_pressed = None;
        self.chrome.interaction.toolbar_press_captured = false;
        self.mark_dirty();
    }

    pub(super) fn set_emacs_frame_id(&mut self, frame_id: u64) {
        self.emacs_frame_id = frame_id;
    }

    pub(super) fn set_mouse_pos(&mut self, pos: (f32, f32)) {
        self.mouse_pos = pos;
        self.pointer_inside = true;
    }

    pub(super) fn clear_pointer_hover(&mut self) -> bool {
        self.pointer_inside = false;
        let before = self.active_pointer_damage();
        let changed = self.pointer_appearance.hover(None);
        if changed {
            self.record_pointer_paint_transition(before);
            self.mark_dirty();
        }
        changed
    }

    pub(super) fn route_presentation_retirement(&mut self, presentation: u64) -> Option<u64> {
        let pinned = self
            .presented_capture()
            .and_then(|capture| capture.target())
            .is_some_and(|target| target.presentation().get() == presentation);
        if pinned {
            if !self.deferred_pointer_retirements.contains(&presentation) {
                self.deferred_pointer_retirements.push(presentation);
            }
            None
        } else {
            Some(presentation)
        }
    }

    pub(super) fn take_deferred_pointer_retirements(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.deferred_pointer_retirements)
    }

    pub(super) fn cancel_pointer_interaction(&mut self) -> (bool, Vec<u64>) {
        let previous_chrome = self.chrome.interaction;
        let before = self.active_pointer_damage();
        let visual_changed = self.pointer_appearance.cancel();
        self.pointer_inside = false;
        self.chrome.interaction.clear_menu_bar();
        self.clear_presented_capture();
        self.chrome.interaction.clear_toolbar();
        self.chrome.interaction.toolbar_press_captured = false;
        self.chrome.interaction.clear_compact_bar();
        let changed = visual_changed || self.chrome.interaction != previous_chrome;
        if changed {
            if visual_changed {
                self.record_pointer_paint_transition(before);
            }
            self.mark_dirty();
        }
        (changed, self.take_deferred_pointer_retirements())
    }

    pub(super) fn set_current_frame(
        &mut self,
        frame: Option<crate::core::frame_glyphs::FrameGlyphBuffer>,
        row_damage: Option<neomacs_renderer_wgpu::FrameRowDamage>,
    ) -> Option<ActivePresentationTransition> {
        let before = self.active_pointer_damage();
        let previous_presentation = self
            .compositor
            .current_frame
            .as_ref()
            .map(|frame| frame.presentation_id);
        let next_presentation = frame.as_ref().map(|frame| frame.presentation_id);
        let transition = next_presentation.and_then(|current| {
            ActivePresentationTransition::between(previous_presentation, current)
        });
        self.compositor.child_frames.set_root_frame(frame.as_ref());
        self.compositor.current_frame = frame;
        #[cfg(feature = "neo-term")]
        {
            self.compositor.terminal_expansion = TerminalExpansion::default();
        }
        #[cfg(feature = "video")]
        self.refresh_visible_videos();
        self.refresh_present_mapping();
        let appearance_changed = if let Some(previous) = previous_presentation
            && Some(previous) != next_presentation
        {
            self.pointer_appearance.retire(previous)
        } else {
            false
        };
        self.compositor.current_scene_generation = super::frame_state::next_scene_generation();
        self.compositor.current_row_damage = row_damage;
        if appearance_changed {
            self.record_pointer_paint_transition(before);
            self.compositor.dirty = true;
        }
        transition
    }

    pub(super) fn with_chrome_interaction_mut(
        &mut self,
        f: impl FnOnce(&mut GuiChromeInteractionState),
    ) -> bool {
        let previous = self.chrome.interaction;
        f(&mut self.chrome.interaction);
        let changed = self.chrome.interaction != previous;
        if changed {
            self.compositor.dirty = true;
        }
        changed
    }

    pub(super) fn update_popup_hover(&mut self, x: f32, y: f32) -> bool {
        let Some(menu) = self.overlays.popup_menu.as_mut() else {
            return false;
        };
        let dirty = menu.update_hover_at(x, y);
        if dirty {
            self.compositor.dirty = true;
        }
        dirty
    }

    pub(super) fn trigger_visual_bell(
        &mut self,
        cursor_error_pulse_enabled: bool,
        edge_snap_enabled: bool,
        edge_snap_duration_ms: u32,
        now: Instant,
    ) {
        self.overlays.visual_bell_start = Some(now);
        if cursor_error_pulse_enabled {
            self.compositor
                .renderer_effects
                .trigger_cursor_error_pulse(now);
        }
        if edge_snap_enabled {
            let selected_info = self.compositor.current_frame.as_ref().and_then(|frame| {
                frame
                    .window_infos
                    .iter()
                    .find(|info| info.selected && !info.is_minibuffer)
                    .cloned()
            });
            if let Some(info) = selected_info {
                let at_top = info.window_start <= 1;
                let at_bottom = info.window_end >= info.buffer_size;
                if at_top || at_bottom {
                    self.compositor.renderer_effects.trigger_edge_snap(
                        info.bounds,
                        info.mode_line_height,
                        at_top,
                        at_bottom,
                        now,
                        edge_snap_duration_ms,
                    );
                }
            }
        }
        self.compositor.dirty = true;
    }

    pub(super) fn take_current_frame_for_render(&mut self) -> Option<FrameGlyphBuffer> {
        let current_frame = self.compositor.current_frame.as_mut()?;
        let mut frame = Self::take_frame_for_render(current_frame);
        #[cfg(feature = "neo-term")]
        self.compositor.terminal_expansion.compose_into(&mut frame);
        Some(frame)
    }

    pub(super) fn take_frame_for_render(current_frame: &mut FrameGlyphBuffer) -> FrameGlyphBuffer {
        let (transition_hints, effect_hints) = current_frame.take_runtime_hints();
        let mut frame = current_frame.clone();
        frame.transition_hints = transition_hints;
        frame.effect_hints = effect_hints;
        frame
    }

    pub(super) fn tick_cursor_animation(&mut self) -> bool {
        let mut dirty = self.cursor.tick_animation();
        for cursor in self.compositor.visual_cursors.values_mut() {
            dirty |= cursor.tick_animation();
        }
        if dirty {
            self.compositor.dirty = true;
        }
        dirty
    }

    pub(super) fn tick_cursor_blink(
        &mut self,
        now: Instant,
        cursor_wake_enabled: bool,
        renderer: Option<&WgpuRenderer>,
    ) -> bool {
        if !self.cursor.blink_enabled || self.cursor.target_cloned().is_none() {
            return false;
        }
        if now.duration_since(self.cursor.last_blink_toggle) < self.cursor.blink_interval {
            return false;
        }
        let was_off = !self.cursor.blink_on;
        self.cursor.blink_on = !self.cursor.blink_on;
        self.cursor.last_blink_toggle = now;
        if was_off
            && self.cursor.blink_on
            && cursor_wake_enabled
            && let Some(renderer) = renderer
        {
            renderer.trigger_transient_cursor_wake(&mut self.compositor.renderer_effects, now);
        }
        // A blink changes the cursor layer and nothing else, so it asks for a
        // composite of the retained scene rather than a repaint. When the
        // toggle also triggers the cursor-wake effect above, that effect marks
        // the window dirty through mark_active_visuals_dirty on this same
        // about_to_wait pass, which outranks this and forces the full render
        // it needs.
        self.compositor.cursor_dirty = true;
        true
    }

    pub(super) fn force_cursor_blink_on(&mut self) -> bool {
        if !self.cursor.force_blink_on() {
            return false;
        }
        self.compositor.cursor_dirty = true;
        true
    }

    pub(super) fn mark_active_visuals_dirty(&mut self) -> bool {
        if !self.compositor.renderer_effects.needs_redraw()
            && !self.compositor.transitions.has_active()
        {
            return false;
        }
        self.compositor.dirty = true;
        true
    }

    pub(super) fn trigger_click_halo(&mut self, x: f32, y: f32, now: Instant, duration_ms: u32) {
        self.compositor
            .renderer_effects
            .trigger_click_halo(x, y, now, duration_ms);
        self.compositor.dirty = true;
    }

    pub(super) fn tick_cursor_size_animation(&mut self) -> bool {
        let mut dirty = self.cursor.tick_size_animation();
        for cursor in self.compositor.visual_cursors.values_mut() {
            dirty |= cursor.tick_size_animation();
        }
        if dirty {
            self.compositor.dirty = true;
        }
        dirty
    }

    pub(super) fn tick_idle_dim(&mut self, config: &IdleDimConfig) -> bool {
        let idle_time = self.overlays.idle_dim.last_activity_time.elapsed();
        let target_alpha = if idle_time >= config.delay {
            config.opacity
        } else {
            0.0
        };
        let diff = target_alpha - self.overlays.idle_dim.current_alpha;
        if diff.abs() > 0.001 {
            let fade_speed = if config.fade_duration.as_secs_f32() > 0.0 {
                1.0 / config.fade_duration.as_secs_f32() * 0.016
            } else {
                1.0
            };
            if diff > 0.0 {
                self.overlays.idle_dim.current_alpha = (self.overlays.idle_dim.current_alpha
                    + fade_speed * config.opacity)
                    .min(target_alpha);
            } else {
                self.overlays.idle_dim.current_alpha =
                    (self.overlays.idle_dim.current_alpha - fade_speed * config.opacity).max(0.0);
            }
            self.overlays.idle_dim.active = true;
            self.compositor.dirty = true;
            true
        } else if self.overlays.idle_dim.current_alpha > 0.001 {
            self.overlays.idle_dim.active = true;
            false
        } else {
            self.overlays.idle_dim.active = false;
            false
        }
    }

    pub(super) fn clear_idle_dim(&mut self) {
        self.overlays.idle_dim.active = false;
        self.overlays.idle_dim.current_alpha = 0.0;
    }

    pub(super) fn sync_visual_cursors_from_current_frame(
        &mut self,
        cursor_config: impl Fn(&mut CursorState),
    ) {
        let Some(current_frame) = self.compositor.current_frame.as_ref() else {
            self.compositor.visual_cursors.clear();
            return;
        };
        let mut live_visual_cursor_ids = HashSet::new();
        for cursor in &current_frame.window_cursors {
            if cursor.window_id.get() >= 0 {
                continue;
            }
            live_visual_cursor_ids.insert(cursor.window_id.get());
            let state = self
                .compositor
                .visual_cursors
                .entry(cursor.window_id.get())
                .or_default();
            cursor_config(state);
            let (_, target_moved) = state.set_target(CursorTarget {
                window_id: cursor.window_id.get(),
                x: cursor.x,
                y: cursor.y,
                width: cursor.width,
                height: cursor.height,
                style: cursor.style,
                frame_id: self.emacs_frame_id,
            });
            if target_moved {
                self.compositor.dirty = true;
            }
        }
        self.compositor
            .visual_cursors
            .retain(|id, _| live_visual_cursor_ids.contains(id));
    }

    pub(super) fn sync_cursor_config(&mut self, defaults: &CursorState, dirty: bool) {
        let config = defaults.config_snapshot();
        self.cursor.apply_config(config);
        for cursor in self.compositor.visual_cursors.values_mut() {
            cursor.apply_config(config);
        }
        if dirty {
            self.compositor.dirty = true;
        }
    }

    pub(super) fn apply_visual_cursor_animations(&mut self) {
        if self.compositor.visual_cursors.is_empty() {
            return;
        }
        let visual_cursor_rects: HashMap<i64, (f32, f32, f32, f32)> = self
            .compositor
            .visual_cursors
            .iter()
            .map(|(id, state)| {
                (
                    *id,
                    (
                        state.current_x,
                        state.current_y,
                        state.current_w,
                        state.current_h,
                    ),
                )
            })
            .collect();
        let Some(frame) = self.compositor.current_frame.as_mut() else {
            return;
        };
        for cursor in &mut frame.window_cursors {
            let Some((x, y, width, height)) = visual_cursor_rects.get(&cursor.window_id.get())
            else {
                continue;
            };
            cursor.x = *x;
            cursor.y = *y;
            cursor.width = *width;
            cursor.height = *height;
        }
    }

    pub(super) fn remove_child_frame(&mut self, frame_id: u64) -> bool {
        let before = self.active_pointer_damage();
        let removed_ids = self.compositor.child_frames.subtree_frame_ids(frame_id);
        let removed_presentations = removed_ids
            .iter()
            .filter_map(|id| self.compositor.child_frames.frames.get(id))
            .map(|entry| entry.frame.presentation_id)
            .collect::<Vec<_>>();
        self.compositor.hidden_child_frames.insert(frame_id);
        let removed = self.compositor.child_frames.remove_frame(frame_id);
        if removed {
            self.compositor
                .pending_child_frame_removals_to_present
                .push(frame_id);
        }
        tracing::info!(
            frame_id,
            removed,
            "child_frame_lifecycle: compositor_remove"
        );
        if removed {
            #[cfg(feature = "video")]
            self.refresh_visible_videos();
            self.compositor.dirty = true;
            for presentation in removed_presentations {
                if self.pointer_appearance.retire(presentation) {
                    self.record_pointer_paint_transition(before);
                }
            }
        }
        if self
            .cursor
            .target_cloned()
            .is_some_and(|target| removed_ids.contains(&target.frame_id))
        {
            self.cursor.clear_target();
            self.input_method.clear();
            self.compositor.dirty = true;
            return true;
        }
        removed
    }

    pub(super) fn displayed_presentations(&self) -> HashSet<u64> {
        let mut presentations = HashSet::new();
        if let Some(presentation) = self
            .compositor
            .current_frame
            .as_ref()
            .map(|frame| frame.presentation_id.get())
            .filter(|presentation| *presentation != 0)
        {
            presentations.insert(presentation);
        }
        presentations.extend(
            self.compositor
                .child_frames
                .frames
                .values()
                .map(|entry| entry.frame.presentation_id.get())
                .filter(|presentation| *presentation != 0),
        );
        presentations
    }

    #[allow(dead_code)] // direct single-child lookup remains covered by frame_windows tests
    pub(super) fn child_presentation(&self, frame_id: u64) -> Option<u64> {
        self.compositor
            .child_frames
            .frames
            .get(&frame_id)
            .map(|entry| entry.frame.presentation_id.get())
            .filter(|presentation| *presentation != 0)
    }

    pub(super) fn child_subtree_presentations(&self, frame_id: u64) -> Vec<u64> {
        self.compositor.child_frames.subtree_presentations(frame_id)
    }

    pub(super) fn show_child_frame(&mut self, frame_id: u64) -> bool {
        let changed = self.compositor.hidden_child_frames.remove(&frame_id);
        tracing::info!(frame_id, changed, "child_frame_lifecycle: compositor_show");
        changed
    }

    pub(super) fn update_child_frame(&mut self, frame: FrameGlyphBuffer) -> bool {
        let before = self.active_pointer_damage();
        let frame_id = frame.frame_placement.frame().get();
        if self.compositor.hidden_child_frames.contains(&frame_id) {
            tracing::debug!(
                frame_id,
                "ignoring child frame update while frame is explicitly hidden"
            );
            return false;
        }
        let previous_presentation = self
            .compositor
            .child_frames
            .frames
            .get(&frame_id)
            .map(|entry| entry.frame.presentation_id);
        let next_presentation = frame.presentation_id;
        let changed = self.compositor.child_frames.update_frame(frame);
        if changed {
            #[cfg(feature = "video")]
            self.refresh_visible_videos();
            self.compositor.dirty = true;
            if let Some(previous) = previous_presentation
                && previous != next_presentation
                && self.pointer_appearance.retire(previous)
            {
                self.record_pointer_paint_transition(before);
            }
        }
        changed
    }
}

impl FrameLifecycle {
    pub fn native(&self) -> Option<&GuiFrameNativeWindowState> {
        match self {
            Self::Active { native, .. } => Some(native),
            _ => None,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    pub fn window(&self) -> Option<&Arc<Window>> {
        self.native().map(|n| &n.window)
    }

    pub fn native_size(&self) -> (u32, u32) {
        match self {
            Self::Active { native, .. } => (native.width, native.height),
            Self::Pending { width, height, .. } => (*width, *height),
        }
    }

    pub fn scale_factor(&self) -> f64 {
        match self {
            Self::Active { native, .. } => native.scale_factor,
            Self::Pending { scale_factor, .. } => *scale_factor,
        }
    }

    pub fn chrome(&self) -> &WindowChrome {
        match self {
            Self::Active { native, .. } => &native.chrome,
            Self::Pending { chrome, .. } => chrome,
        }
    }

    pub fn chrome_mut(&mut self) -> &mut WindowChrome {
        match self {
            Self::Active { native, .. } => &mut native.chrome,
            Self::Pending { chrome, .. } => chrome,
        }
    }

    pub fn ime_enabled(&self) -> bool {
        match self {
            Self::Active { ime_enabled, .. } => *ime_enabled,
            Self::Pending { ime_enabled, .. } => *ime_enabled,
        }
    }

    pub fn set_ime_enabled(&mut self, enabled: bool) {
        match self {
            Self::Active {
                ime_enabled: ie, ..
            } => *ie = enabled,
            Self::Pending {
                ime_enabled: ie, ..
            } => *ie = enabled,
        }
    }

    pub fn mouse_hidden_for_typing(&self) -> bool {
        match self {
            Self::Active {
                mouse_hidden_for_typing: m,
                ..
            } => *m,
            Self::Pending {
                mouse_hidden_for_typing: m,
                ..
            } => *m,
        }
    }

    pub fn set_mouse_hidden_for_typing(&mut self, hidden: bool) {
        match self {
            Self::Active {
                mouse_hidden_for_typing: m,
                ..
            } => *m = hidden,
            Self::Pending {
                mouse_hidden_for_typing: m,
                ..
            } => *m = hidden,
        }
    }

    pub fn last_ime_cursor_area(&self) -> Option<ImeCursorArea> {
        match self {
            Self::Active {
                last_ime_cursor_area,
                ..
            } => *last_ime_cursor_area,
            Self::Pending {
                last_ime_cursor_area,
                ..
            } => *last_ime_cursor_area,
        }
    }

    pub fn geometry_hints(&self) -> Option<GuiFrameGeometryHints> {
        match self {
            Self::Pending { geometry_hints, .. } => *geometry_hints,
            _ => None,
        }
    }

    pub fn request_redraw(&self) {
        if let Self::Active { native, .. } = self {
            super::frame_stats::count(&super::frame_stats::REDRAW_REQUESTS);
            native.window.request_redraw();
        }
    }
}

impl GuiFrameWindowState {
    pub fn handle_resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let scale = DeviceScale::new(self.lifecycle.scale_factor() as f32)
            .expect("effective window scale is finite and positive");
        let surface_state = SurfaceState::from_device_size(width, height, scale)
            .expect("wgpu surface dimensions have finite logical extents");
        self.render.set_surface_state(surface_state);
        if matches!(surface_state, SurfaceState::Suspended) {
            return;
        }
        match &mut self.lifecycle {
            FrameLifecycle::Active { native, .. } => {
                native.width = width;
                native.height = height;
                native.surface_config.width = width;
                native.surface_config.height = height;
                native.surface.configure(device, &native.surface_config);
                clear_frame_transition_textures(&mut self.render.compositor.transitions);
                self.render.compositor.dirty = true;
            }
            FrameLifecycle::Pending {
                width: pw,
                height: ph,
                ..
            } => {
                *pw = width;
                *ph = height;
            }
        }
    }

    pub fn set_scale_factor(&mut self, scale_factor: f64) {
        let effective_scale = effective_window_scale_factor(scale_factor);
        match &mut self.lifecycle {
            FrameLifecycle::Active { native, .. } => {
                native.scale_factor = effective_scale;
                if let Some(atlas) = self.render.compositor.glyph_atlas.as_mut() {
                    atlas.set_scale_factor(effective_scale as f32);
                }
                self.render.compositor.dirty = true;
            }
            FrameLifecycle::Pending {
                scale_factor: sf, ..
            } => {
                *sf = effective_scale;
            }
        }
        let (width, height) = self.lifecycle.native_size();
        let scale = DeviceScale::new(effective_scale as f32)
            .expect("effective window scale is finite and positive");
        let surface_state = SurfaceState::from_device_size(width, height, scale)
            .expect("native surface dimensions have finite logical extents");
        self.render.set_surface_state(surface_state);
    }

    pub(super) fn set_title(&mut self, title: String) {
        match &mut self.lifecycle {
            FrameLifecycle::Active { native, .. } => {
                native.chrome.title = title.clone();
                native.window.set_title(&title);
                if !native.chrome.decorations_enabled {
                    self.render.compositor.dirty = true;
                }
            }
            FrameLifecycle::Pending { chrome, .. } => {
                chrome.title = title;
            }
        }
    }

    pub(super) fn set_fullscreen_mode(&mut self, mode: WindowFullscreenMode) {
        let FrameLifecycle::Active { native, .. } = &mut self.lifecycle else {
            return;
        };
        match mode {
            WindowFullscreenMode::Fullscreen | WindowFullscreenMode::Fullboth => {
                native
                    .window
                    .set_fullscreen(Some(Fullscreen::Borderless(None)));
                native.chrome.is_fullscreen = true;
            }
            WindowFullscreenMode::Maximized => {
                native.window.set_fullscreen(None);
                native.window.set_maximized(true);
                native.chrome.is_fullscreen = false;
            }
            WindowFullscreenMode::None => {
                native.window.set_fullscreen(None);
                native.window.set_maximized(false);
                native.chrome.is_fullscreen = false;
            }
            WindowFullscreenMode::Fullwidth | WindowFullscreenMode::Fullheight => {
                tracing::warn!(
                    "partial fullscreen modes are not implemented by the native window backend"
                );
            }
        }
        self.render.compositor.dirty = true;
    }

    pub(super) fn request_inner_size(&mut self, width: u32, height: u32) {
        match &mut self.lifecycle {
            FrameLifecycle::Active { native, .. } => {
                let size = window_size_from_emacs_pixels(width, height);
                let _ = native.window.request_inner_size(size);
            }
            FrameLifecycle::Pending {
                width: pw,
                height: ph,
                ..
            } => {
                *pw = width;
                *ph = height;
            }
        }
    }

    pub(super) fn apply_geometry_hints(&mut self, geometry_hints: GuiFrameGeometryHints) {
        match &mut self.lifecycle {
            FrameLifecycle::Active { native, .. } => {
                apply_window_geometry_hints(&native.window, geometry_hints);
            }
            FrameLifecycle::Pending {
                geometry_hints: gh, ..
            } => {
                *gh = Some(geometry_hints);
            }
        }
    }

    pub(super) fn set_decorations(&mut self, decorated: bool) {
        match &mut self.lifecycle {
            FrameLifecycle::Active { native, .. } => {
                native.chrome.decorations_enabled = decorated;
                native.window.set_decorations(decorated);
                self.render.compositor.dirty = true;
            }
            FrameLifecycle::Pending { chrome, .. } => {
                chrome.decorations_enabled = decorated;
            }
        }
    }

    pub(super) fn set_mouse_hidden_for_typing(&mut self, hidden: bool) {
        if let FrameLifecycle::Active {
            native,
            mouse_hidden_for_typing,
            ..
        } = &mut self.lifecycle
            && *mouse_hidden_for_typing != hidden
        {
            native.window.set_cursor_visible(!hidden);
        }
        self.lifecycle.set_mouse_hidden_for_typing(hidden);
    }

    pub(super) fn reset_ime_cursor_area(&mut self) {
        match &mut self.lifecycle {
            FrameLifecycle::Active {
                native,
                last_ime_cursor_area,
                ..
            } => {
                *last_ime_cursor_area = None;
                native.window.set_ime_cursor_area(
                    PhysicalPosition::new(0.0, 0.0),
                    PhysicalSize::new(1.0, 1.0),
                );
            }
            FrameLifecycle::Pending {
                last_ime_cursor_area,
                ..
            } => {
                *last_ime_cursor_area = None;
            }
        }
    }

    pub(super) fn update_ime_cursor_area(&mut self, area: ImeCursorArea) {
        match &mut self.lifecycle {
            FrameLifecycle::Active {
                native,
                last_ime_cursor_area,
                ..
            } => {
                if *last_ime_cursor_area == Some(area) {
                    return;
                }
                native.window.set_ime_cursor_area(
                    PhysicalPosition::new(area.x as f64, area.y as f64),
                    PhysicalSize::new(area.width as f64, area.height as f64),
                );
                *last_ime_cursor_area = Some(area);
            }
            FrameLifecycle::Pending {
                last_ime_cursor_area,
                ..
            } => {
                *last_ime_cursor_area = Some(area);
            }
        }
    }

    pub(super) fn clear_ime_preedit(&mut self) {
        self.render.clear_ime_preedit();
        self.reset_ime_cursor_area();
    }

    pub(super) fn remove_child_frame(&mut self, frame_id: u64) -> bool {
        let target_was_child = self
            .render
            .cursor
            .target_cloned()
            .is_some_and(|target| target.frame_id == frame_id);
        let changed = self.render.remove_child_frame(frame_id);
        if target_was_child {
            self.reset_ime_cursor_area();
        }
        changed
    }

    pub(super) fn show_child_frame(&mut self, frame_id: u64) -> bool {
        self.render.show_child_frame(frame_id)
    }

    pub(super) fn drag_resize_for_current_edge(&self) -> bool {
        let FrameLifecycle::Active { native, .. } = &self.lifecycle else {
            return false;
        };
        let Some(dir) = native.chrome.resize_edge else {
            return false;
        };
        let _ = native.window.drag_resize_window(dir);
        true
    }

    pub(super) fn handle_titlebar_action(&mut self, action: u32) -> bool {
        let FrameLifecycle::Active { native, .. } = &mut self.lifecycle else {
            return false;
        };
        match action {
            1 => {
                let now = Instant::now();
                if now
                    .duration_since(native.chrome.last_titlebar_click)
                    .as_millis()
                    < 400
                {
                    native.window.set_maximized(!native.window.is_maximized());
                } else {
                    let _ = native.window.drag_window();
                }
                native.chrome.last_titlebar_click = now;
                true
            }
            3 => {
                native.window.set_maximized(!native.window.is_maximized());
                true
            }
            4 => {
                native.window.set_minimized(true);
                true
            }
            _ => false,
        }
    }

    pub(super) fn drag_window(&self) {
        if let Some(native) = self.lifecycle.native() {
            let _ = native.window.drag_window();
        }
    }

    pub fn native_size(&self) -> (u32, u32) {
        self.lifecycle.native_size()
    }

    pub fn scale_factor(&self) -> f64 {
        self.lifecycle.scale_factor()
    }

    pub(super) fn chrome(&self) -> &WindowChrome {
        self.lifecycle.chrome()
    }

    pub(super) fn chrome_mut(&mut self) -> &mut WindowChrome {
        self.lifecycle.chrome_mut()
    }

    pub fn ime_enabled(&self) -> bool {
        self.lifecycle.ime_enabled()
    }

    pub fn set_ime_enabled(&mut self, enabled: bool) {
        self.lifecycle.set_ime_enabled(enabled);
    }

    pub fn request_redraw(&self) {
        self.lifecycle.request_redraw();
    }

    pub(super) fn has_presentable_dirty_content(&self) -> bool {
        self.lifecycle.is_active() && self.render.has_presentable_dirty_content()
    }

    pub(super) fn has_presentable_cursor_change(&self) -> bool {
        self.lifecycle.is_active() && self.render.has_presentable_cursor_change()
    }

    pub fn window(&self) -> Option<&Arc<Window>> {
        self.lifecycle.window()
    }
}

/// Key for frame-window lookup in the manager's `windows` HashMap.
///
/// Matches GNU Emacs convention: 0 is never a valid frame ID
/// (`frame_next_id = 1` in GNU Emacs `frame.c:343`).  The primary
/// frame starts under [`FrameKey::Pending`] and is re-keyed to
/// [`FrameKey::Adopted`] once `adopt_primary_frame_id` is called.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub(crate) enum FrameKey {
    /// Primary frame before Emacs assigns a real frame ID (bootstrap).
    Pending,
    /// Frame with a real Emacs-assigned frame ID.
    Adopted(u64),
}

impl FrameKey {
    pub(super) fn from_primary(emacs_id: Option<u64>) -> Self {
        match emacs_id {
            Some(id) => Self::Adopted(id),
            None => Self::Pending,
        }
    }
}

/// Manages top-level GUI frame windows in the render thread.
pub(crate) struct GuiFrameWindowManager {
    /// All top-level frame windows, keyed by [`FrameKey`].
    pub windows: HashMap<FrameKey, GuiFrameWindowState>,
    /// Winit WindowId → Emacs frame_id (reverse mapping for event dispatch)
    pub winit_to_emacs: HashMap<WindowId, u64>,
    /// Emacs frame_id adopted by the primary process window.
    pub primary_emacs_frame_id: Option<u64>,
    /// winit id of the primary process window.
    pub primary_winit_id: Option<WindowId>,
    /// Pending window creation requests (processed in resumed/about_to_wait)
    pub pending_creates: Vec<PendingWindow>,
    /// Pending window destruction requests
    pub pending_destroys: Vec<u64>,
    /// Native chrome defaults applied to future secondary frame windows.
    pub(super) chrome_defaults: WindowChrome,
    /// Whether future secondary frame windows should start with FPS enabled.
    pub(super) fps_enabled: bool,
}

/// A request to create a new OS window.
pub(crate) struct PendingWindow {
    pub emacs_frame_id: u64,
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub geometry_hints: GuiFrameGeometryHints,
}

impl GuiFrameWindowManager {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            winit_to_emacs: HashMap::new(),
            primary_emacs_frame_id: None,
            primary_winit_id: None,
            pending_creates: Vec::new(),
            pending_destroys: Vec::new(),
            chrome_defaults: WindowChrome::default(),
            fps_enabled: false,
        }
    }

    pub(super) fn cursor_target_for_frame(
        emacs_frame_id: u64,
        frame: &FrameGlyphBuffer,
    ) -> Option<CursorTarget> {
        frame.active_cursor().map(|cursor| {
            // Slide toward the exact rect the static cursor is drawn at -- the
            // shared cursor_draw_rect -- not the grid-approximate cursor
            // geometry. Under scaled fonts the two diverge and the animated box
            // would strand itself as a second cursor.
            let (x, y, width, height) = frame.cursor_draw_rect(
                cursor.slot_id,
                cursor.style,
                cursor.ascent,
                (cursor.x, cursor.y, cursor.width, cursor.height),
            );
            CursorTarget {
                window_id: cursor.window_id.get(),
                x,
                y,
                width,
                height,
                style: cursor.style,
                frame_id: emacs_frame_id,
            }
        })
    }

    pub fn adopt_primary_frame_id(&mut self, emacs_frame_id: u64) {
        let old_key = FrameKey::from_primary(self.primary_emacs_frame_id);
        self.primary_emacs_frame_id = Some(emacs_frame_id);
        if let Some(window_state) = self.windows.remove(&old_key) {
            self.windows
                .insert(FrameKey::Adopted(emacs_frame_id), window_state);
        }
        if let Some(ws) = self.windows.get_mut(&FrameKey::Adopted(emacs_frame_id)) {
            ws.render.set_emacs_frame_id(emacs_frame_id);
        }
        self.sync_primary_mapping();
    }

    #[allow(dead_code)] // used by the frame-window manager tests
    pub fn primary_frame_id(&self) -> Option<u64> {
        self.primary_emacs_frame_id
    }

    pub fn primary_event_frame_id(&self) -> u64 {
        self.primary_emacs_frame_id.unwrap_or(0)
    }

    pub fn is_primary_frame_id(&self, emacs_frame_id: u64) -> bool {
        emacs_frame_id == 0 || self.primary_emacs_frame_id == Some(emacs_frame_id)
    }

    fn primary_frame_key(&self) -> FrameKey {
        FrameKey::from_primary(self.primary_emacs_frame_id)
    }

    pub(super) fn primary_window(&self) -> Option<&GuiFrameWindowState> {
        self.windows.get(&self.primary_frame_key())
    }

    pub(super) fn primary_window_mut(&mut self) -> Option<&mut GuiFrameWindowState> {
        self.windows.get_mut(&self.primary_frame_key())
    }

    pub(super) fn set_primary_pending(&mut self, window_state: GuiFrameWindowState) {
        self.windows.insert(FrameKey::Pending, window_state);
        self.sync_primary_mapping();
    }

    pub(super) fn populate_primary_native(&mut self, native: GuiFrameNativeWindowState) {
        let key = self.primary_frame_key();
        if let Some(window_state) = self.windows.get_mut(&key) {
            let winit_id = native.window.id();
            let surface_state = SurfaceState::from_device_size(
                native.width,
                native.height,
                DeviceScale::new(native.scale_factor as f32)
                    .expect("effective window scale is finite and positive"),
            )
            .expect("native surface dimensions have finite logical extents");
            self.primary_winit_id = Some(winit_id);
            window_state.lifecycle = FrameLifecycle::Active {
                native,
                mouse_hidden_for_typing: window_state.lifecycle.mouse_hidden_for_typing(),
                ime_enabled: window_state.lifecycle.ime_enabled(),
                last_ime_cursor_area: window_state.lifecycle.last_ime_cursor_area(),
            };
            window_state.render.set_surface_state(surface_state);
            self.sync_primary_mapping();
        }
    }

    pub(super) fn take_primary_window(&mut self) -> Option<GuiFrameWindowState> {
        let key = self.primary_frame_key();
        if let Some(winit_id) = self.primary_winit_id.take() {
            self.winit_to_emacs.remove(&winit_id);
        }
        self.windows.remove(&key)
    }

    pub fn is_primary_winit(&self, winit_id: WindowId) -> bool {
        self.primary_winit_id == Some(winit_id)
    }

    pub fn clear_primary_mapping(&mut self) {
        if let Some(winit_id) = self.primary_winit_id.take() {
            self.winit_to_emacs.remove(&winit_id);
        }
        self.primary_emacs_frame_id = None;
    }

    fn sync_primary_mapping(&mut self) {
        if let (Some(winit_id), Some(emacs_frame_id)) =
            (self.primary_winit_id, self.primary_emacs_frame_id)
        {
            self.winit_to_emacs.insert(winit_id, emacs_frame_id);
        }
    }

    /// Schedule a new window to be created on the next event loop iteration.
    pub fn request_create(
        &mut self,
        emacs_frame_id: u64,
        width: u32,
        height: u32,
        title: String,
        geometry_hints: GuiFrameGeometryHints,
    ) {
        self.pending_creates.push(PendingWindow {
            emacs_frame_id,
            width,
            height,
            title,
            geometry_hints,
        });
    }

    /// Schedule a window for destruction.
    pub fn request_destroy(&mut self, emacs_frame_id: u64) {
        self.pending_destroys.push(emacs_frame_id);
    }

    /// Process pending window creations. Must be called from the event loop
    /// (requires ActiveEventLoop for window creation).
    pub fn process_creates(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_icon: &mut crate::window_icon::WindowIconService,
        instance: &wgpu::Instance,
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
    ) {
        let pending = std::mem::take(&mut self.pending_creates);
        for req in pending {
            if self
                .windows
                .contains_key(&FrameKey::Adopted(req.emacs_frame_id))
            {
                tracing::warn!("Window for frame {} already exists", req.emacs_frame_id);
                continue;
            }

            let attrs = Window::default_attributes()
                .with_title(&req.title)
                .with_inner_size(window_size_from_emacs_pixels(req.width, req.height))
                .with_transparent(true)
                .with_decorations(self.chrome_defaults.decorations_enabled);
            let attrs = crate::window_identity::apply_platform_window_identity(attrs);

            match event_loop.create_window(attrs) {
                Ok(window) => {
                    let window = Arc::new(window);
                    window_icon.apply(&window);
                    let raw_scale_factor = window.scale_factor();
                    let scale_factor = effective_window_scale_factor(raw_scale_factor);
                    let phys = window.inner_size();

                    // Create surface for this window using the primary display-bound instance.
                    let surface = match instance.create_surface(window.clone()) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!(
                                "Failed to create surface for frame {}: {:?}",
                                req.emacs_frame_id,
                                e
                            );
                            continue;
                        }
                    };

                    // Configure surface
                    let caps = surface.get_capabilities(adapter);
                    let format = caps
                        .formats
                        .iter()
                        .copied()
                        .find(|f| f.is_srgb())
                        .unwrap_or(caps.formats[0]);
                    let alpha_mode = if caps
                        .alpha_modes
                        .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
                    {
                        wgpu::CompositeAlphaMode::PreMultiplied
                    } else {
                        caps.alpha_modes[0]
                    };
                    let config = wgpu::SurfaceConfiguration {
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        format,
                        color_space: wgpu::SurfaceColorSpace::Auto,
                        width: phys.width,
                        height: phys.height,
                        present_mode: wgpu::PresentMode::Fifo,
                        alpha_mode,
                        view_formats: vec![],
                        desired_maximum_frame_latency: 2,
                    };
                    surface.configure(device, &config);

                    NativeTextInputPolicy::for_gui_frame().apply_to_window(&window);
                    apply_window_geometry_hints(&window, req.geometry_hints);

                    let winit_id = window.id();
                    tracing::info!(
                        "Created window for frame {} (winit {:?}, {}x{}, raw_scale={}, effective_scale={})",
                        req.emacs_frame_id,
                        winit_id,
                        phys.width,
                        phys.height,
                        raw_scale_factor,
                        scale_factor
                    );

                    self.winit_to_emacs.insert(winit_id, req.emacs_frame_id);
                    let chrome = WindowChrome {
                        title: req.title.clone(),
                        titlebar_hover: 0,
                        resize_edge: None,
                        last_titlebar_click: Instant::now(),
                        ..self.chrome_defaults.clone()
                    };
                    let mut render = GuiFrameRenderState::new(
                        req.emacs_frame_id,
                        device,
                        scale_factor,
                        self.fps_enabled,
                    );
                    render.set_surface_state(
                        SurfaceState::from_device_size(
                            phys.width,
                            phys.height,
                            DeviceScale::new(scale_factor as f32)
                                .expect("effective window scale is finite and positive"),
                        )
                        .expect("native surface dimensions have finite logical extents"),
                    );
                    self.windows.insert(
                        FrameKey::Adopted(req.emacs_frame_id),
                        GuiFrameWindowState {
                            lifecycle: FrameLifecycle::Active {
                                native: GuiFrameNativeWindowState {
                                    window,
                                    surface,
                                    surface_config: config,
                                    width: phys.width,
                                    height: phys.height,
                                    scale_factor,
                                    chrome: chrome.clone(),
                                },
                                mouse_hidden_for_typing: false,
                                ime_enabled: false,
                                last_ime_cursor_area: None,
                            },
                            render,
                        },
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to create window for frame {}: {:?}",
                        req.emacs_frame_id,
                        e
                    );
                }
            }
        }
    }

    /// Process pending window destructions.
    pub fn process_destroys(&mut self) {
        let pending = std::mem::take(&mut self.pending_destroys);
        for frame_id in pending {
            if let Some(state) = self.windows.remove(&FrameKey::Adopted(frame_id)) {
                if let Some(native) = state.lifecycle.native() {
                    self.winit_to_emacs.remove(&native.window.id());
                }
                tracing::info!("Destroyed window for frame {}", frame_id);
            }
        }
    }

    /// Drop all windows and their wgpu surfaces (for clean shutdown).
    pub fn destroy_all(&mut self) {
        self.pending_creates.clear();
        self.pending_destroys.clear();
        self.winit_to_emacs.clear();
        self.primary_winit_id = None;
        self.primary_emacs_frame_id = None;
        self.windows.clear();
    }

    /// Look up the Emacs frame_id for a winit WindowId.
    pub fn emacs_frame_for_winit(&self, winit_id: WindowId) -> Option<u64> {
        self.winit_to_emacs.get(&winit_id).copied()
    }

    pub fn event_frame_for_winit(&self, winit_id: WindowId) -> Option<u64> {
        if self.is_primary_winit(winit_id) {
            Some(self.primary_event_frame_id())
        } else {
            self.emacs_frame_for_winit(winit_id)
        }
    }

    /// Get a window state by Emacs frame_id.
    pub fn get(&self, emacs_frame_id: u64) -> Option<&GuiFrameWindowState> {
        if self.is_primary_frame_id(emacs_frame_id) {
            self.primary_window()
        } else {
            self.windows.get(&FrameKey::Adopted(emacs_frame_id))
        }
    }

    /// Get a mutable window state by Emacs frame_id.
    pub fn get_mut(&mut self, emacs_frame_id: u64) -> Option<&mut GuiFrameWindowState> {
        if self.is_primary_frame_id(emacs_frame_id) {
            self.primary_window_mut()
        } else {
            self.windows.get_mut(&FrameKey::Adopted(emacs_frame_id))
        }
    }

    /// Resolve the native top-level window that owns a presented frame.
    /// `frame_id` may name the top-level frame itself or any installed child
    /// in its immutable ancestry scene.
    pub(super) fn get_mut_by_presented_frame(
        &mut self,
        frame_id: u64,
    ) -> Option<&mut GuiFrameWindowState> {
        let owner = if self.is_primary_frame_id(frame_id) {
            Some(self.primary_frame_key())
        } else if self.windows.contains_key(&FrameKey::Adopted(frame_id)) {
            Some(FrameKey::Adopted(frame_id))
        } else {
            self.windows.iter().find_map(|(key, window)| {
                window
                    .render
                    .compositor
                    .child_frames
                    .frames
                    .contains_key(&frame_id)
                    .then_some(*key)
            })
        }?;
        self.windows.get_mut(&owner)
    }

    pub(super) fn has_presented_frame(&self, frame_id: u64) -> bool {
        self.windows.values().any(|window| {
            window
                .render
                .compositor
                .current_frame
                .as_ref()
                .is_some_and(|frame| frame.frame_placement.frame().get() == frame_id)
                || window
                    .render
                    .compositor
                    .child_frames
                    .frames
                    .contains_key(&frame_id)
        })
    }

    /// Get a window state by winit WindowId.
    pub fn get_by_winit(&self, winit_id: WindowId) -> Option<&GuiFrameWindowState> {
        if self.primary_winit_id == Some(winit_id) {
            return self.primary_window();
        }
        self.winit_to_emacs
            .get(&winit_id)
            .and_then(|id| self.windows.get(&FrameKey::Adopted(*id)))
    }

    /// Get a mutable window state by winit WindowId.
    pub fn get_by_winit_mut(&mut self, winit_id: WindowId) -> Option<&mut GuiFrameWindowState> {
        if self.primary_winit_id == Some(winit_id) {
            return self.primary_window_mut();
        }
        self.winit_to_emacs
            .get(&winit_id)
            .copied()
            .and_then(move |id| self.windows.get_mut(&FrameKey::Adopted(id)))
    }

    pub(super) fn for_each_top_level_window(&self, mut f: impl FnMut(&GuiFrameWindowState)) {
        for window_state in self.windows.values() {
            f(window_state);
        }
    }

    /// Complete image residency fence owned by accepted root and child
    /// presentations across every native window.
    pub(super) fn retained_images(&self) -> RetainedImageSet {
        let mut retained = RetainedImageSet::default();
        self.for_each_top_level_window(|window_state| {
            window_state.render.extend_retained_images(&mut retained);
        });
        retained
    }

    pub(super) fn for_each_top_level_window_mut(
        &mut self,
        mut f: impl FnMut(&mut GuiFrameWindowState),
    ) {
        for window_state in self.windows.values_mut() {
            f(window_state);
        }
    }

    pub(super) fn mark_top_level_dirty(&mut self) {
        self.for_each_top_level_window_mut(|window_state| {
            window_state.render.compositor.dirty = true;
        });
    }

    pub(super) fn mark_active_top_level_visuals_dirty(&mut self) -> bool {
        let mut dirty = false;
        self.for_each_top_level_window_mut(|window_state| {
            dirty |= window_state.render.mark_active_visuals_dirty();
        });
        dirty
    }

    pub(super) fn tick_top_level_cursor_blinks(
        &mut self,
        now: Instant,
        cursor_wake_enabled: bool,
        renderer: Option<&WgpuRenderer>,
    ) -> bool {
        let primary_winit_id = self.primary_winit_id;
        let mut dirty = false;
        self.for_each_top_level_window_mut(|window_state| {
            let is_primary = window_state
                .lifecycle
                .native()
                .is_some_and(|n| primary_winit_id == Some(n.window.id()));
            dirty |= window_state.render.tick_cursor_blink(
                now,
                cursor_wake_enabled && is_primary,
                renderer,
            );
        });
        dirty
    }

    pub(super) fn tick_top_level_cursor_animations(&mut self) -> bool {
        let mut dirty = false;
        self.for_each_top_level_window_mut(|window_state| {
            dirty |= window_state.render.tick_cursor_animation();
        });
        dirty
    }

    pub(super) fn tick_top_level_cursor_size_animations(&mut self) -> bool {
        let mut dirty = false;
        self.for_each_top_level_window_mut(|window_state| {
            dirty |= window_state.render.tick_cursor_size_animation();
        });
        dirty
    }

    pub(super) fn tick_top_level_idle_dim(&mut self, config: &IdleDimConfig) -> bool {
        let mut dirty = false;
        self.for_each_top_level_window_mut(|window_state| {
            dirty |= window_state.render.tick_idle_dim(config);
        });
        dirty
    }

    pub(super) fn clear_top_level_idle_dim(&mut self) {
        self.for_each_top_level_window_mut(|window_state| {
            window_state.render.clear_idle_dim();
        });
    }

    pub(super) fn set_top_level_titlebar_height(&mut self, height: f32) {
        self.chrome_defaults.titlebar_height = height;
        self.for_each_top_level_window_mut(|window_state| {
            window_state.chrome_mut().titlebar_height = height;
            window_state.render.compositor.dirty = true;
        });
    }

    pub(super) fn set_top_level_corner_radius(&mut self, radius: f32) {
        self.chrome_defaults.corner_radius = radius;
        self.for_each_top_level_window_mut(|window_state| {
            window_state.chrome_mut().corner_radius = radius;
            window_state.render.compositor.dirty = true;
        });
    }

    pub(super) fn set_top_level_fps_enabled(&mut self, enabled: bool) {
        self.fps_enabled = enabled;
        self.for_each_top_level_window_mut(|window_state| {
            window_state.render.overlays.fps.enabled = enabled;
            window_state.render.compositor.dirty = true;
        });
    }

    pub(super) fn set_top_level_decorations(&mut self, decorated: bool) {
        self.chrome_defaults.decorations_enabled = decorated;
        self.for_each_top_level_window_mut(|window_state| {
            window_state.set_decorations(decorated);
        });
    }

    pub(super) fn hide_top_level_popup_menus(&mut self) {
        self.for_each_top_level_window_mut(|window_state| {
            if window_state.render.overlays.popup_menu.is_some() {
                window_state.render.overlays.popup_menu = None;
                window_state.render.compositor.dirty = true;
            }
        });
    }

    pub(super) fn hide_top_level_tooltips(&mut self) {
        self.for_each_top_level_window_mut(|window_state| {
            if window_state.render.overlays.tooltip.is_some() {
                window_state.render.overlays.tooltip = None;
                window_state.render.compositor.dirty = true;
            }
        });
    }

    pub(super) fn remove_child_frame_from_top_level_windows(&mut self, frame_id: u64) -> bool {
        let mut changed = false;
        self.for_each_top_level_window_mut(|window_state| {
            changed |= window_state.remove_child_frame(frame_id);
        });
        changed
    }

    pub(super) fn show_child_frame_in_top_level_windows(&mut self, frame_id: u64) -> bool {
        let mut changed = false;
        self.for_each_top_level_window_mut(|window_state| {
            changed |= window_state.show_child_frame(frame_id);
        });
        changed
    }

    pub(super) fn sync_top_level_cursor_config(&mut self, defaults: &CursorState, dirty: bool) {
        self.for_each_top_level_window_mut(|window_state| {
            window_state.render.sync_cursor_config(defaults, dirty);
        });
    }

    /// Tick child frame counters. The primary removal path is
    /// `remove_child_frame` triggered by
    /// `set-frame-parameter 'visibility nil` → `set_frame_visibility`
    /// → `notify_gui_child_frame_hidden` → `RemoveChildFrame`.
    ///
    /// `prune_stale` is intentionally NOT called here: it would
    /// incorrectly remove visible-but-idle child frames (e.g. a
    /// static posframe tooltip) that haven't received a
    /// `FrameDisplayState` update in a while. The root-cause fix
    /// ensures explicit removal on every visibility change.
    pub(super) fn tick_top_level_child_frames(&mut self) {
        self.for_each_top_level_window_mut(|window_state| {
            window_state.render.compositor.child_frames.tick();
        });
    }

    pub(super) fn force_top_level_cursor_blink_on(&mut self) -> bool {
        let mut dirty = false;
        self.for_each_top_level_window_mut(|window_state| {
            dirty |= window_state.render.force_cursor_blink_on();
        });
        dirty
    }

    pub(super) fn apply_top_level_transition_policy(
        &mut self,
        policy: neomacs_display_protocol::TransitionPolicy,
    ) {
        self.for_each_top_level_window_mut(|window_state| {
            window_state
                .render
                .compositor
                .transitions
                .apply_policy(policy);
        });
    }

    /// Discard renderer-owned animation timelines for every top-level window.
    ///
    /// A quality-policy downgrade must remove the state that advertises frame
    /// demand, not merely disable the emitters that would eventually drain it.
    pub(super) fn discard_top_level_renderer_effects(&mut self) {
        self.for_each_top_level_window_mut(|window_state| {
            window_state.render.compositor.renderer_effects =
                neomacs_renderer_wgpu::RendererFrameEffects::default();
        });
    }

    /// Drop every GPU-resident object owned by per-window render state after
    /// the wgpu device was lost. CPU state — `current_frame`, row damage,
    /// child frames, overlays, cursors, floating-WebView rects — is kept: the
    /// next redraw re-renders the same scene on the rebuilt device.
    pub(super) fn clear_gpu_resident_state(&mut self) {
        self.for_each_top_level_window_mut(|window_state| {
            let render = &mut window_state.render;
            // Glyph atlas textures (recreated against the new device by
            // populate_glyph_atlas / recreate_secondary_native_surfaces).
            render.compositor.glyph_atlas = None;
            // Retained cursorless scene: texture + view + bind group.
            render.compositor.retained_static = None;
            // Transition snapshots: offscreen_a/offscreen_b plus each
            // crossfade/scroll-slide's old_texture/old_view/old_bind_group.
            clear_frame_transition_textures(&mut render.compositor.transitions);
            // Full-frame post shader composition target.
            render.frame_post_src = None;
            render.compositor.dirty = true;
        });
    }

    /// Recreate the wgpu surface of every non-primary Active window on a new
    /// instance/device after device-loss recovery (the primary window is
    /// rebuilt by `init_wgpu` via `populate_primary_native`). Surfaces are
    /// instance-owned, so old ones can never be configured against the new
    /// device. Also repopulates the glyph atlases cleared by
    /// `clear_gpu_resident_state`.
    pub(super) fn recreate_secondary_native_surfaces(
        &mut self,
        instance: &wgpu::Instance,
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
    ) {
        let primary_key = self.primary_frame_key();
        for (key, window_state) in self.windows.iter_mut() {
            if *key == primary_key {
                continue;
            }
            let FrameLifecycle::Active { native, .. } = &mut window_state.lifecycle else {
                continue;
            };
            let surface = match instance.create_surface(native.window.clone()) {
                Ok(surface) => surface,
                Err(error) => {
                    tracing::error!(
                        ?key,
                        ?error,
                        "device-loss recovery: failed to recreate window surface"
                    );
                    continue;
                }
            };
            let caps = surface.get_capabilities(adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0]);
            let alpha_mode = if caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
            {
                wgpu::CompositeAlphaMode::PreMultiplied
            } else {
                caps.alpha_modes[0]
            };
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                color_space: wgpu::SurfaceColorSpace::Auto,
                width: native.width,
                height: native.height,
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(device, &config);
            // Replacing the fields drops the old-instance surface in place.
            native.surface = surface;
            native.surface_config = config;
            let scale_factor = native.scale_factor;
            window_state
                .render
                .populate_glyph_atlas(device, scale_factor);
            window_state.render.compositor.dirty = true;
        }
    }

    pub(super) fn apply_top_level_visual_cursor_animations(&mut self) {
        self.for_each_top_level_window_mut(|window_state| {
            window_state.render.apply_visual_cursor_animations();
        });
    }

    pub fn count(&self) -> usize {
        self.windows.len()
    }
}

#[cfg(test)]
#[path = "frame_windows_test.rs"]
mod tests;
