// Several render entry points carry the recurring `bg_gradient` RGB-pair tuple
// parameter, which mirrors the renderer-wgpu API surface; a local type alias
// would not be reused, so the type-complexity lint is allowed module-wide.
#![allow(clippy::type_complexity)]

use super::child_frames::ChildFrameManager;
use super::cursor::CursorTarget;
use super::frame_sched::PresentResult;
use super::frame_windows::{
    FrameLifecycle, GuiFrameNativeWindowState, GuiFrameRenderState, GuiFrameWindowState,
};
use super::state::{
    ChildFrameStyle, FpsCounter, GuiChromeInteractionState, ToolbarResources, TypingSpeedState,
    WindowChrome,
};
use super::transitions::{
    detect_frame_transitions, ensure_frame_offscreen_textures, render_frame_transitions,
};
use super::{RenderApp, surface_readback};
use crate::core::types::DisplayFrameId;
use crate::thread_comm::{MenuBarItem, ToolBarItem};
use neomacs_display_protocol::frame_chrome::{FrameChromeContent, FrameRect, PositionedChromeItem};
use neomacs_renderer_wgpu::{PopupMenuState, TooltipState, WgpuGlyphAtlas, WgpuRenderer};

/// Flatten a protocol [`Color`] into the legacy `(r, g, b)` tuple the
/// renderer's chrome-overlay draw fns still take. Alpha is dropped: GUI
/// chrome colors are opaque sRGB. Follow-up: migrate the overlay draw
/// fns themselves to `Color` and delete this.
fn color_rgb_tuple(color: neomacs_display_protocol::types::Color) -> (f32, f32, f32) {
    (color.r, color.g, color.b)
}

type RenderedFrameSurface = (
    wgpu::SurfaceTexture,
    crate::core::frame_glyphs::FrameGlyphBuffer,
);

/// Failures before a frame reaches `present`, kept distinct until the frame
/// coordinator consumes them.  In particular, missing editor content is not a
/// GPU timeout and must not manufacture an expose-retry loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameRenderFailure {
    AwaitingContent,
    WindowNotReady,
    SurfaceLost,
    SurfaceTimeout,
    SurfaceOccluded,
}

impl FrameRenderFailure {
    const fn present_result(self) -> PresentResult {
        match self {
            Self::AwaitingContent => PresentResult::AwaitingContent,
            Self::WindowNotReady | Self::SurfaceTimeout => PresentResult::Timeout,
            Self::SurfaceLost => PresentResult::SurfaceLost,
            Self::SurfaceOccluded => PresentResult::Occluded,
        }
    }
}

#[cfg(test)]
mod retained_static_pointer_tests {
    use super::RenderApp;
    use crate::core::frame_glyphs::{CursorStyle, DisplaySlotId, FrameGlyphBuffer, WindowCursor};
    use crate::render_thread::state::{PointerAppearanceState, PresentedAppearanceKey};
    use neomacs_display_protocol::frame_chrome::PresentationId;
    use neomacs_display_protocol::{
        Color, DisplayWindowId, EffectsConfig, FrameRate, PointerAppearanceId,
    };

    fn filled_box_frame(effects: EffectsConfig) -> (FrameGlyphBuffer, DisplayWindowId) {
        let window_id = DisplayWindowId::new(1);
        let mut frame = FrameGlyphBuffer::with_size(20.0, 20.0);
        frame.window_cursors.push(WindowCursor {
            window_id,
            slot_id: DisplaySlotId::ZERO,
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
            style: CursorStyle::FilledBox,
            color: Color::WHITE,
            cursor_fg: Color::BLACK,
            ascent: 0.0,
            active: true,
        });
        frame.set_window_cursor_effects(window_id, effects);
        (frame, window_id)
    }

    #[test]
    fn hover_and_pressed_pointer_appearance_force_full_render_and_disable_cursor_cells() {
        let mut pointer = PointerAppearanceState::default();
        let mut frame = FrameGlyphBuffer::with_size(20.0, 20.0);
        frame.window_cursors.push(WindowCursor {
            window_id: DisplayWindowId::new(1),
            slot_id: DisplaySlotId::ZERO,
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
            style: CursorStyle::FilledBox,
            color: Color::WHITE,
            cursor_fg: Color::BLACK,
            ascent: 0.0,
            active: true,
        });
        assert!(RenderApp::retained_static_pointer_appearance_allowed(
            &pointer
        ));
        assert_eq!(
            RenderApp::build_filled_box_cursor_cells(&frame, 1.0, &pointer).len(),
            1
        );

        let key = PresentedAppearanceKey::new(
            PresentationId::new(7),
            PointerAppearanceId::try_from(0usize).unwrap(),
        );
        pointer.hover(Some(key));
        assert!(!RenderApp::retained_static_pointer_appearance_allowed(
            &pointer
        ));
        assert!(RenderApp::build_filled_box_cursor_cells(&frame, 1.0, &pointer).is_empty());

        pointer.press();
        assert!(!RenderApp::retained_static_pointer_appearance_allowed(
            &pointer
        ));
        assert!(RenderApp::build_filled_box_cursor_cells(&frame, 1.0, &pointer).is_empty());
    }

    #[test]
    fn retained_filled_box_cursor_cells_preserve_local_effect_profiles() {
        let pointer = PointerAppearanceState::default();

        for local_enabled in [false, true] {
            let mut local = EffectsConfig::cursor_profile_baseline();
            local.cursor_color_cycle.enabled = local_enabled;
            local.cursor_color_cycle.fps = FrameRate::new(12).unwrap();
            let (frame, window_id) = filled_box_frame(local.clone());

            let mut global = EffectsConfig::default();
            global.cursor_color_cycle.enabled = !local_enabled;
            global.cursor_color_cycle.fps = FrameRate::new(60).unwrap();

            let cells = RenderApp::build_filled_box_cursor_cells(&frame, 1.0, &pointer);
            assert_eq!(cells.len(), 1);
            assert_eq!(
                cells[0]
                    .mini
                    .effective_window_cursor_effects(window_id, &global),
                &local,
                "the retained cursor-only path must resolve the same local profile as the full renderer"
            );
        }
    }
}

pub(super) fn frame_chrome_toolbar_bounds(
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
) -> Option<FrameRect> {
    frame
        .frame_chrome
        .band(neomacs_display_protocol::frame_chrome::FrameChromeKind::ToolBar)
        .map(|band| band.bounds())
}

struct GuiFrameMenuBarOverlay<'a> {
    items: &'a [PositionedChromeItem<MenuBarItem>],
    bounds: FrameRect,
    fg: (f32, f32, f32),
    bg: (f32, f32, f32),
}

struct GuiFrameToolBarOverlay<'a> {
    items: &'a [PositionedChromeItem<ToolBarItem>],
    bounds: FrameRect,
    fg: (f32, f32, f32),
    bg: (f32, f32, f32),
    toolbar: &'a ToolbarResources,
    icon_size: u32,
    padding: u32,
}

struct GuiFrameCompactBarOverlay<'a> {
    menu_items: &'a [PositionedChromeItem<MenuBarItem>],
    tool_items: &'a [PositionedChromeItem<ToolBarItem>],
    bounds: FrameRect,
    menu_fg: (f32, f32, f32),
    menu_bg: (f32, f32, f32),
    tool_fg: (f32, f32, f32),
    tool_bg: (f32, f32, f32),
    toolbar: &'a ToolbarResources,
    icon_size: u32,
    padding: u32,
}

struct GuiFrameImeOverlay<'a> {
    text: &'a str,
    x: f32,
    y: f32,
    height: f32,
}

struct GuiFrameChromeOverlays<'a> {
    native_chrome: &'a WindowChrome,
    titlebar_background: Option<(f32, f32, f32)>,
    chrome_interaction: GuiChromeInteractionState,
    menu_bar: Option<GuiFrameMenuBarOverlay<'a>>,
    tool_bar: Option<GuiFrameToolBarOverlay<'a>>,
    compact_bar: Option<GuiFrameCompactBarOverlay<'a>>,
    popup_menu: Option<&'a PopupMenuState>,
    tooltip: Option<&'a TooltipState>,
    ime_preedit: Option<GuiFrameImeOverlay<'a>>,
}

/// How long the visual-bell flash ramps down over.
const VISUAL_BELL_DURATION_SECS: f32 = 0.15;

impl RenderApp {
    fn update_typing_speed_state(state: &mut TypingSpeedState) -> bool {
        // Step 2.5 replaces this with the frame's sample.
        let now = neomacs_display_protocol::frame_time::observe_platform_now();
        let window_secs = 5.0_f64;
        state
            .key_press_times
            .retain(|t| now.saturating_since(*t).as_secs_f64() < window_secs);
        let count = state.key_press_times.len() as f64;
        let target_wpm = if count > 1.0 {
            let span = now.saturating_since(state.key_press_times[0]).as_secs_f64();
            if span > 0.1 {
                (count / span) * 60.0 / 5.0
            } else {
                0.0
            }
        } else {
            0.0
        };
        state.displayed_wpm += (target_wpm as f32 - state.displayed_wpm) * 0.15;
        if state.displayed_wpm < 0.5 {
            state.displayed_wpm = 0.0;
        }
        state.displayed_wpm > 0.5 || !state.key_press_times.is_empty()
    }

    fn render_frame_common_overlays(
        renderer: &mut WgpuRenderer,
        surface_view: &wgpu::TextureView,
        frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        width: u32,
        height: u32,
        scroll_indicators_enabled: bool,
    ) {
        if renderer.effects.breadcrumb.enabled {
            renderer.render_breadcrumbs(surface_view, frame, glyph_atlas);
        }

        if scroll_indicators_enabled {
            renderer.render_scroll_indicators(surface_view, &frame.window_infos, width, height);
        }

        if renderer.effects.window_watermark.enabled {
            renderer.render_window_watermarks(surface_view, frame, glyph_atlas);
        }
    }

    fn frame_ime_preedit_overlay<'a>(
        preedit: Option<&'a super::frame_windows::ImePreedit>,
        target: Option<CursorTarget>,
        root_frame_id: u64,
        child_frames: &ChildFrameManager,
    ) -> Option<GuiFrameImeOverlay<'a>> {
        let preedit = preedit?;

        let target = target?;
        let (offset_x, offset_y) = if target.frame_id != root_frame_id {
            child_frames
                .frames
                .get(&target.frame_id)
                .map(|entry| (entry.abs_x, entry.abs_y))
                .unwrap_or((0.0, 0.0))
        } else {
            (0.0, 0.0)
        };

        Some(GuiFrameImeOverlay {
            text: &preedit.text,
            x: target.x + offset_x,
            y: target.y + offset_y,
            height: target.height,
        })
    }

    fn render_frame_chrome_overlays(
        renderer: &mut WgpuRenderer,
        surface_view: &wgpu::TextureView,
        glyph_atlas: &mut WgpuGlyphAtlas,
        overlays: GuiFrameChromeOverlays<'_>,
        width: u32,
        height: u32,
    ) {
        if !overlays.native_chrome.decorations_enabled
            && !overlays.native_chrome.is_fullscreen
            && overlays.native_chrome.titlebar_height > 0.0
        {
            renderer.render_custom_titlebar(
                surface_view,
                &overlays.native_chrome.title,
                overlays.native_chrome.titlebar_height,
                overlays.native_chrome.titlebar_hover,
                overlays.titlebar_background,
                glyph_atlas,
                width,
                height,
            );
        }

        if let Some(menu_bar) = overlays.menu_bar {
            renderer.render_menu_bar(
                surface_view,
                menu_bar.items,
                menu_bar.bounds,
                menu_bar.fg,
                menu_bar.bg,
                overlays.chrome_interaction.menu_bar_hovered,
                overlays.chrome_interaction.menu_bar_active,
                glyph_atlas,
                width,
                height,
            );
        }

        if let Some(tool_bar) = overlays.tool_bar {
            renderer.render_toolbar(
                surface_view,
                tool_bar.items,
                tool_bar.bounds,
                tool_bar.fg,
                tool_bar.bg,
                &tool_bar.toolbar.icon_textures,
                overlays.chrome_interaction.toolbar_hovered,
                overlays.chrome_interaction.toolbar_pressed,
                tool_bar.icon_size,
                tool_bar.padding,
                width,
                height,
            );
        }

        if let Some(compact_bar) = overlays.compact_bar {
            renderer.render_compact_bar(
                surface_view,
                compact_bar.menu_items,
                compact_bar.tool_items,
                compact_bar.bounds,
                compact_bar.menu_fg,
                compact_bar.menu_bg,
                compact_bar.tool_fg,
                compact_bar.tool_bg,
                &compact_bar.toolbar.icon_textures,
                overlays.chrome_interaction.compact_bar_menu_hovered,
                overlays.chrome_interaction.compact_bar_menu_active,
                overlays.chrome_interaction.compact_bar_tool_hovered,
                overlays.chrome_interaction.compact_bar_tool_pressed,
                compact_bar.icon_size,
                compact_bar.padding,
                glyph_atlas,
                width,
                height,
            );
        }

        if let Some(menu) = overlays.popup_menu {
            renderer.render_popup_menu(surface_view, menu, glyph_atlas, width, height);
        }

        if let Some(tooltip) = overlays.tooltip {
            renderer.render_tooltip(surface_view, tooltip, glyph_atlas, width, height);
        }

        if let Some(preedit) = overlays.ime_preedit {
            renderer.render_ime_preedit(
                surface_view,
                preedit.text,
                preedit.x,
                preedit.y,
                preedit.height,
                glyph_atlas,
                width,
                height,
            );
        }
    }

    fn render_frame_corner_mask(
        renderer: &mut WgpuRenderer,
        surface_view: &wgpu::TextureView,
        chrome: &WindowChrome,
        width: u32,
        height: u32,
    ) {
        if !chrome.decorations_enabled && !chrome.is_fullscreen && chrome.corner_radius > 0.0 {
            renderer.render_corner_mask(surface_view, chrome.corner_radius, width, height);
        }
    }

    fn render_frame_visual_bell_overlay(
        renderer: &mut WgpuRenderer,
        surface_view: &wgpu::TextureView,
        visual_bell_start: &mut Option<neomacs_display_protocol::frame_time::EventTime>,
        frame_dirty: &mut bool,
        width: u32,
        height: u32,
    ) {
        if let Some(start) = *visual_bell_start {
            // Step 2.5 replaces this read with the frame's sample; today it is the
            // same wall-clock read `start.elapsed()` performed, just typed.
            let elapsed = neomacs_display_protocol::frame_time::observe_platform_now()
                .saturating_since(start)
                .as_secs_f32();
            let duration = VISUAL_BELL_DURATION_SECS;
            if elapsed < duration {
                let alpha = (1.0 - elapsed / duration) * 0.3;
                renderer.render_visual_bell(surface_view, width, height, alpha);
                *frame_dirty = true;
            } else {
                *visual_bell_start = None;
            }
        }
    }

    fn render_frame_fps_overlay(
        renderer: &mut WgpuRenderer,
        surface_view: &wgpu::TextureView,
        glyph_atlas: &mut WgpuGlyphAtlas,
        fps: &mut FpsCounter,
        glyph_count: usize,
        window_count: usize,
        transition_count: usize,
        width: u32,
        height: u32,
    ) -> bool {
        if !fps.enabled {
            return false;
        }

        let frame_time = fps.render_start.elapsed().as_secs_f32() * 1000.0;
        fps.frame_time_ms = fps.frame_time_ms * 0.9 + frame_time * 0.1;
        let stats_lines = vec![
            format!("{:.0} FPS | {:.1}ms", fps.display_value, fps.frame_time_ms),
            format!(
                "{}g {}w {}t  {}x{}",
                glyph_count, window_count, transition_count, width, height
            ),
        ];
        renderer.render_fps_overlay(surface_view, &stats_lines, glyph_atlas, width, height);
        true
    }

    fn render_frame_typing_speed_overlay(
        renderer: &mut WgpuRenderer,
        surface_view: &wgpu::TextureView,
        frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        typing_speed: &mut TypingSpeedState,
        frame_dirty: &mut bool,
    ) {
        let keep_redrawing = Self::update_typing_speed_state(typing_speed);
        renderer.render_typing_speed(surface_view, frame, glyph_atlas, typing_speed.displayed_wpm);
        if keep_redrawing {
            *frame_dirty = true;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_frame_window_contents_to_surface(
        renderer: &mut WgpuRenderer,
        window_state: &mut GuiFrameWindowState,
        bg_gradient: Option<((f32, f32, f32), (f32, f32, f32))>,
        child_frame_style: &ChildFrameStyle,
        scroll_indicators_enabled: bool,
        toolbar: &ToolbarResources,
        extra_line_spacing: f32,
        extra_letter_spacing: f32,
        compositor_only_hint: bool,
        render_policy: &super::render_quality::RenderQualityPolicy,
        device_lost: &mut super::device_loss::DeviceLossDetector,
    ) -> Result<RenderedFrameSurface, FrameRenderFailure> {
        Self::render_frame_window_contents_to_acquired_surface(
            renderer,
            window_state,
            bg_gradient,
            child_frame_style,
            scroll_indicators_enabled,
            toolbar,
            extra_line_spacing,
            extra_letter_spacing,
            None,
            compositor_only_hint,
            render_policy,
            device_lost,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_frame_window_contents_to_acquired_surface(
        renderer: &mut WgpuRenderer,
        window_state: &mut GuiFrameWindowState,
        bg_gradient: Option<((f32, f32, f32), (f32, f32, f32))>,
        child_frame_style: &ChildFrameStyle,
        scroll_indicators_enabled: bool,
        toolbar: &ToolbarResources,
        extra_line_spacing: f32,
        extra_letter_spacing: f32,
        output: Option<wgpu::SurfaceTexture>,
        compositor_only_hint: bool,
        render_policy: &super::render_quality::RenderQualityPolicy,
        device_lost: &mut super::device_loss::DeviceLossDetector,
    ) -> Result<RenderedFrameSurface, FrameRenderFailure> {
        let render = &mut window_state.render;
        let native = match &mut window_state.lifecycle {
            FrameLifecycle::Active { native, .. } => native,
            _ => return Err(FrameRenderFailure::WindowNotReady),
        };
        Self::update_fps_counter(&mut render.overlays.fps);
        // Read the one bit the offscreen decision needs straight out of the
        // retained frame. Missing content has its own typed outcome: the frame
        // channel, rather than an expose retry, is responsible for waking it.
        // Read it here, before `take_current_frame_for_render` below drains
        // the hints. The frame itself is taken once, after the surface is
        // acquired — the acquisition has several early-return paths, so
        // materializing it earlier was work thrown away outright on any
        // lost/outdated/occluded surface.
        let frame_has_theme_transition = render
            .current_frame_theme_transition_hint()
            .ok_or(FrameRenderFailure::AwaitingContent)?;
        let animated_cursor = render.cursor.animated_cursor();
        let root_animated_cursor = animated_cursor
            .filter(|cursor| cursor.frame_id == DisplayFrameId::new(render.emacs_frame_id));
        // The slide animation is composed at draw time: emit_cursor_visual reads
        // the interpolated rect from animated_cursor for the active window's
        // cursor. The frame's stored cursor geometry is no longer mutated here,
        // so the materialized frame stays a pure function of the layout snapshot.

        let feature_plan =
            render_policy.plan_frame(frame_has_theme_transition, renderer.has_frame_post());
        let need_offscreen = feature_plan.use_transition_offscreen;

        let output = if let Some(output) = output {
            output
        } else {
            match native.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(output)
                | wgpu::CurrentSurfaceTexture::Suboptimal(output) => {
                    device_lost.record_surface_acquired();
                    output
                }
                wgpu::CurrentSurfaceTexture::Lost => {
                    // A one-off Lost is a swapchain hiccup; an unbroken
                    // streak means the device itself is gone (TDR) and only
                    // a full GPU rebuild brings frames back.
                    if device_lost.record_surface_lost() {
                        tracing::error!(
                            "Surface for frame 0x{:x} lost {} times in a row: treating the wgpu device as lost",
                            render.emacs_frame_id,
                            super::device_loss::CONSECUTIVE_SURFACE_LOST_THRESHOLD
                        );
                    } else {
                        tracing::info!(
                            "Skipping redraw for frame 0x{:x}: surface lost",
                            render.emacs_frame_id
                        );
                    }
                    return Err(FrameRenderFailure::SurfaceLost);
                }
                wgpu::CurrentSurfaceTexture::Outdated => {
                    tracing::info!(
                        "Skipping redraw for frame 0x{:x}: surface outdated",
                        render.emacs_frame_id
                    );
                    return Err(FrameRenderFailure::SurfaceLost);
                }
                wgpu::CurrentSurfaceTexture::Timeout => {
                    return Err(FrameRenderFailure::SurfaceTimeout);
                }
                wgpu::CurrentSurfaceTexture::Occluded => {
                    return Err(FrameRenderFailure::SurfaceOccluded);
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    tracing::warn!(
                        "Surface validation error for frame 0x{:x}",
                        render.emacs_frame_id
                    );
                    return Err(FrameRenderFailure::SurfaceTimeout);
                }
            }
        };

        let present_mapping = render
            .present_mapping()
            .ok_or(FrameRenderFailure::AwaitingContent)?;
        let mut frame = render
            .take_current_frame_for_render()
            .ok_or(FrameRenderFailure::AwaitingContent)?;
        feature_plan.prepare_frame(&mut frame);
        render.begin_presentable_render();
        if extra_line_spacing != 0.0 || extra_letter_spacing != 0.0 {
            Self::apply_extra_spacing(
                &mut frame.glyphs,
                &mut frame.window_cursors,
                extra_line_spacing,
                extra_letter_spacing,
            );
        }

        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        // Full-frame post shader: compose the ENTIRE frame (content,
        // transitions, overlays, cursor) into an intermediate texture and
        // shade it into the swapchain as the LAST step, so every present is
        // uniformly post-processed (Ghostty semantics — cursor included) and
        // partial-damage frames cannot mix shaded and unshaded regions.
        let frame_post_active = feature_plan.apply_frame_post;
        let composition_view = if frame_post_active {
            Self::ensure_frame_post_src(renderer, render, native.width, native.height)
        } else {
            surface_view.clone()
        };
        let old_scale_factor = renderer.scale_factor();
        let old_width = renderer.width();
        let old_height = renderer.height();
        renderer.set_scale_factor(native.scale_factor as f32);
        renderer.resize(native.width, native.height);
        let cursor_visible = render.cursor.blink_on;

        // Stage 4 retained-static fast path: when the coordinator asked for a
        // compositor-only cursor frame and the scene is eligible (no
        // transition, no dynamic overlay, and every cursor is a clean
        // top-layer style), blit the retained cursorless scene and draw only
        // the cursor, skipping the glyph pipeline. The retained scene is built
        // once per scene generation and reused; any ineligibility falls
        // through to the full render below. The composite is proven
        // bit-identical to a full render (offscreen_frame::composite_matches_
        // full_render). Set NEOMACS_DISABLE_RETAINED_STATIC to force-disable.
        // Gate on an *active* transition, not on `need_offscreen`: the latter
        // is true whenever crossfade/scroll transitions are merely enabled
        // (the default), because the offscreen snapshot only has to be kept
        // current across scene commits. A compositor-only frame changes no
        // editor content, so it cannot start a transition, and the "before" snapshot
        // captured at the last scene-commit full render stays correct. Gating
        // on `!need_offscreen` here would disable the fast path entirely under
        // the default transition policy.
        if compositor_only_hint
            && !render.compositor.transitions.has_active()
            && !Self::window_has_active_overlays(render)
            && Self::retained_static_pointer_appearance_allowed(&render.pointer_appearance)
            && !render.has_pointer_paint_damage()
            && std::env::var_os("NEOMACS_DISABLE_RETAINED_STATIC").is_none()
        {
            let mouse_pos = render.mouse_pos;
            let generation = render.compositor.current_scene_generation;
            let retained_valid = matches!(
                &render.compositor.retained_static,
                Some(rs) if rs.generation == generation
                    && rs.width == native.width
                    && rs.height == native.height
            );
            if !retained_valid {
                Self::ensure_retained_static_texture(renderer, render, native.width, native.height);
                let retained_view = render
                    .compositor
                    .retained_static
                    .as_ref()
                    .expect("retained texture just ensured")
                    .view
                    .clone();
                // Render the full cursorless static scene into the retained
                // texture (this runs the glyph pipeline once per generation).
                Self::render_frame_window_contents(
                    renderer,
                    native,
                    render,
                    &retained_view,
                    &frame,
                    present_mapping,
                    false,
                    root_animated_cursor,
                    animated_cursor,
                    bg_gradient,
                    true,
                    child_frame_style,
                    scroll_indicators_enabled,
                    toolbar,
                );
                let cells = Self::build_filled_box_cursor_cells(
                    &frame,
                    native.scale_factor as f32,
                    &render.pointer_appearance,
                );
                if let Some(rs) = render.compositor.retained_static.as_mut() {
                    rs.generation = generation;
                    rs.cursor_cells = cells;
                }
                super::frame_stats::count(&super::frame_stats::RETAINED_STATIC_BUILDS);
            }
            if let Some(rs) = render.compositor.retained_static.as_ref() {
                renderer.blit_texture_to_view(
                    &rs.bind_group,
                    &composition_view,
                    native.width,
                    native.height,
                );
            }
            renderer.render_cursor_only(
                &composition_view,
                &frame,
                present_mapping,
                cursor_visible,
                animated_cursor,
                mouse_pos,
            );
            // Filled-box cursors are inverse-video: the retained scene has the
            // character in its normal color, so each filled-box cell (box plus
            // the character in cursor_fg) is redrawn over the composite from a
            // single-glyph mini-frame, scissored to the cell. Bit-identical to
            // the full render (offscreen_frame::filled_box_composite_matches_
            // full_render).
            if cursor_visible {
                Self::composite_filled_box_cursor_cells(
                    renderer,
                    render,
                    &composition_view,
                    present_mapping,
                    animated_cursor,
                    mouse_pos,
                );
            }
            super::frame_stats::count(&super::frame_stats::COMPOSITE_ONLY_FRAMES);
            if frame_post_active {
                renderer.frame_post_to_view(
                    &composition_view,
                    &surface_view,
                    native.width,
                    native.height,
                    mouse_pos,
                );
            }
            render.finish_pointer_paint_render();
            renderer.set_scale_factor(old_scale_factor);
            renderer.resize(old_width, old_height);
            return Ok((output, frame));
        }

        if need_offscreen {
            render.compositor.transitions.current_is_a =
                !render.compositor.transitions.current_is_a;
            ensure_frame_offscreen_textures(
                renderer,
                &mut render.compositor.transitions,
                native.width,
                native.height,
            );

            let current_view = if render.compositor.transitions.current_is_a {
                render
                    .compositor
                    .transitions
                    .offscreen_a
                    .as_ref()
                    .map(|(_, view, _)| view.clone())
            } else {
                render
                    .compositor
                    .transitions
                    .offscreen_b
                    .as_ref()
                    .map(|(_, view, _)| view.clone())
            };

            if let Some(current_view) = current_view {
                Self::render_frame_window_contents(
                    renderer,
                    native,
                    render,
                    &current_view,
                    &frame,
                    present_mapping,
                    cursor_visible,
                    root_animated_cursor,
                    animated_cursor,
                    bg_gradient,
                    false,
                    child_frame_style,
                    scroll_indicators_enabled,
                    toolbar,
                );
            }

            let current_bg = if render.compositor.transitions.current_is_a {
                render
                    .compositor
                    .transitions
                    .offscreen_a
                    .as_ref()
                    .map(|(_, _, bg)| bg.clone())
            } else {
                render
                    .compositor
                    .transitions
                    .offscreen_b
                    .as_ref()
                    .map(|(_, _, bg)| bg.clone())
            };

            renderer.with_frame_effects(&mut render.compositor.renderer_effects, |renderer| {
                detect_frame_transitions(
                    renderer,
                    &mut render.compositor.transitions,
                    &renderer.effects.clone(),
                    &mut frame,
                    &mut render.compositor.dirty,
                    native.width,
                    native.height,
                );
            });
            if render.compositor.renderer_effects.needs_redraw() {
                render.mark_dirty();
            }

            if let Some(current_bg) = current_bg {
                renderer.blit_texture_to_view(
                    &current_bg,
                    &composition_view,
                    native.width,
                    native.height,
                );
            }
            render_frame_transitions(
                renderer,
                &mut render.compositor.transitions,
                &composition_view,
                native.width,
                native.height,
            );
            if render.compositor.transitions.has_active() {
                render.mark_dirty();
            }
            Self::render_frame_window_overlays_with_toolbar_resources(
                renderer,
                native,
                render,
                &composition_view,
                &frame,
                cursor_visible,
                animated_cursor,
                child_frame_style,
                scroll_indicators_enabled,
                toolbar,
            );
        } else {
            Self::render_frame_window_contents(
                renderer,
                native,
                render,
                &composition_view,
                &frame,
                present_mapping,
                cursor_visible,
                root_animated_cursor,
                animated_cursor,
                bg_gradient,
                true,
                child_frame_style,
                scroll_indicators_enabled,
                toolbar,
            );
            renderer.with_frame_effects(&mut render.compositor.renderer_effects, |renderer| {
                detect_frame_transitions(
                    renderer,
                    &mut render.compositor.transitions,
                    &renderer.effects.clone(),
                    &mut frame,
                    &mut render.compositor.dirty,
                    native.width,
                    native.height,
                );
            });
            render.mark_active_visuals_dirty();
        }

        if frame_post_active {
            renderer.frame_post_to_view(
                &composition_view,
                &surface_view,
                native.width,
                native.height,
                render.mouse_pos,
            );
        }
        render.finish_pointer_paint_render();
        renderer.set_scale_factor(old_scale_factor);
        renderer.resize(old_width, old_height);
        Ok((output, frame))
    }

    #[allow(clippy::too_many_arguments)]
    fn render_frame_window_overlays_with_toolbar_resources(
        renderer: &mut WgpuRenderer,
        native: &GuiFrameNativeWindowState,
        render: &mut GuiFrameRenderState,
        surface_view: &wgpu::TextureView,
        frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
        cursor_visible: bool,
        animated_cursor: Option<crate::core::types::AnimatedCursor>,
        child_frame_style: &ChildFrameStyle,
        scroll_indicators_enabled: bool,
        toolbar: &ToolbarResources,
    ) {
        Self::render_frame_content_overlays(
            renderer,
            native,
            render,
            surface_view,
            frame,
            cursor_visible,
            animated_cursor,
            child_frame_style,
            scroll_indicators_enabled,
        );

        let menu_bar = frame.frame_chrome.bands().iter().find_map(|band| {
            let FrameChromeContent::MenuBar(content) = band.content() else {
                return None;
            };
            Some((band.bounds(), content))
        });
        let tool_bar_content = frame.frame_chrome.bands().iter().find_map(|band| {
            let FrameChromeContent::ToolBar(content) = band.content() else {
                return None;
            };
            Some(content)
        });
        let tool_bar = frame_chrome_toolbar_bounds(frame).zip(tool_bar_content);
        let compact_bar = frame.frame_chrome.bands().iter().find_map(|band| {
            let FrameChromeContent::CompactBar(content) = band.content() else {
                return None;
            };
            Some((band.bounds(), content))
        });
        Self::render_frame_chrome_overlays(
            renderer,
            surface_view,
            render.compositor.glyph_atlas.as_mut().unwrap(),
            GuiFrameChromeOverlays {
                native_chrome: &native.chrome,
                titlebar_background: Some((
                    frame.background.r,
                    frame.background.g,
                    frame.background.b,
                )),
                chrome_interaction: render.chrome.interaction,
                menu_bar: menu_bar.map(|(bounds, menu_bar)| GuiFrameMenuBarOverlay {
                    items: menu_bar.items(),
                    bounds,
                    fg: color_rgb_tuple(menu_bar.foreground()),
                    bg: color_rgb_tuple(menu_bar.background()),
                }),
                tool_bar: tool_bar.map(|(bounds, tool_bar)| GuiFrameToolBarOverlay {
                    items: tool_bar.items(),
                    bounds,
                    fg: color_rgb_tuple(tool_bar.foreground()),
                    bg: color_rgb_tuple(tool_bar.background()),
                    toolbar,
                    icon_size: tool_bar.icon_size(),
                    padding: tool_bar.padding(),
                }),
                compact_bar: compact_bar.map(|(bounds, compact_bar)| GuiFrameCompactBarOverlay {
                    menu_items: compact_bar.menu_items(),
                    tool_items: compact_bar.tool_items(),
                    bounds,
                    menu_fg: color_rgb_tuple(compact_bar.menu_foreground()),
                    menu_bg: color_rgb_tuple(compact_bar.menu_background()),
                    tool_fg: color_rgb_tuple(compact_bar.tool_foreground()),
                    tool_bg: color_rgb_tuple(compact_bar.tool_background()),
                    toolbar,
                    icon_size: compact_bar.icon_size(),
                    padding: compact_bar.padding(),
                }),
                popup_menu: render.overlays.popup_menu.as_ref(),
                tooltip: render.overlays.tooltip.as_ref(),
                ime_preedit: Self::frame_ime_preedit_overlay(
                    render.input_method.preedit(),
                    render.cursor.target_cloned(),
                    render.emacs_frame_id,
                    &render.compositor.child_frames,
                ),
            },
            native.width,
            native.height,
        );

        Self::render_frame_visual_bell_overlay(
            renderer,
            surface_view,
            &mut render.overlays.visual_bell_start,
            &mut render.compositor.dirty,
            native.width,
            native.height,
        );

        Self::render_frame_corner_mask(
            renderer,
            surface_view,
            &native.chrome,
            native.width,
            native.height,
        );

        if Self::render_frame_fps_overlay(
            renderer,
            surface_view,
            render.compositor.glyph_atlas.as_mut().unwrap(),
            &mut render.overlays.fps,
            frame.glyphs.len(),
            frame.window_infos.len(),
            render.compositor.transitions.active_count(),
            native.width,
            native.height,
        ) {
            render.mark_dirty();
        }

        if renderer.effects.typing_speed.enabled {
            Self::render_frame_typing_speed_overlay(
                renderer,
                surface_view,
                frame,
                render.compositor.glyph_atlas.as_mut().unwrap(),
                &mut render.overlays.typing_speed,
                &mut render.compositor.dirty,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_frame_root_glyphs(
        renderer: &mut WgpuRenderer,
        render: &mut GuiFrameRenderState,
        surface_view: &wgpu::TextureView,
        frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
        present_mapping: neomacs_display_protocol::PresentMapping,
        cursor_visible: bool,
        root_animated_cursor: Option<crate::core::types::AnimatedCursor>,
        bg_gradient: Option<((f32, f32, f32), (f32, f32, f32))>,
    ) {
        super::frame_stats::count(&super::frame_stats::ROOT_GLYPH_PASSES);
        let pointer_selection = render.pointer_selection_for(frame);
        if let Some(atlas) = render.compositor.glyph_atlas.as_mut() {
            atlas.set_current_frame_fonts(frame.font_bindings());
        }
        renderer.with_frame_effects(&mut render.compositor.renderer_effects, |renderer| {
            renderer.set_idle_dim_alpha(render.overlays.idle_dim.current_alpha);
            renderer.render_frame_glyphs(
                surface_view,
                frame,
                render.compositor.glyph_atlas.as_mut().unwrap(),
                present_mapping,
                cursor_visible,
                root_animated_cursor,
                render.mouse_pos,
                bg_gradient,
                pointer_selection,
                render.compositor.current_row_damage.as_ref(),
            );
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn render_frame_content_overlays(
        renderer: &mut WgpuRenderer,
        native: &GuiFrameNativeWindowState,
        render: &mut GuiFrameRenderState,
        surface_view: &wgpu::TextureView,
        frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
        cursor_visible: bool,
        animated_cursor: Option<crate::core::types::AnimatedCursor>,
        child_frame_style: &ChildFrameStyle,
        scroll_indicators_enabled: bool,
    ) {
        let pointer_appearance = render.pointer_appearance;
        renderer.with_frame_effects(&mut render.compositor.renderer_effects, |renderer| {
            for &child_id in render.compositor.child_frames.sorted_for_rendering() {
                if let Some(child_entry) = render.compositor.child_frames.frames.get(&child_id) {
                    if child_entry.frame.font_catalog_generation != frame.font_catalog_generation {
                        tracing::debug!(
                            frame_id = child_id,
                            child_generation = child_entry.frame.font_catalog_generation.get(),
                            root_generation = frame.font_catalog_generation.get(),
                            "skipping retained child frame from a stale font catalog generation"
                        );
                        continue;
                    }
                    let neomacs_display_protocol::PresentedClip::Rect(clip_in_root) =
                        child_entry.clip_in_root
                    else {
                        continue;
                    };
                    let pointer_selection = pointer_appearance.selection_for(&child_entry.frame);
                    if let Some(atlas) = render.compositor.glyph_atlas.as_mut() {
                        atlas.set_current_frame_fonts(child_entry.frame.font_bindings());
                    }
                    tracing::debug!(
                        parent_frame_id = render.emacs_frame_id,
                        frame_id = child_id,
                        x = child_entry.abs_x,
                        y = child_entry.abs_y,
                        width = child_entry.frame.width,
                        height = child_entry.frame.height,
                        glyphs = child_entry.frame.glyphs.len(),
                        "child_frame_lifecycle: render_child_frame_start"
                    );
                    renderer.render_child_frame(
                        surface_view,
                        &child_entry.frame,
                        child_entry.abs_x,
                        child_entry.abs_y,
                        clip_in_root,
                        render.compositor.glyph_atlas.as_mut().unwrap(),
                        native.width,
                        native.height,
                        cursor_visible,
                        animated_cursor.filter(|ac| ac.frame_id == DisplayFrameId::new(child_id)),
                        child_frame_style.corner_radius,
                        child_frame_style.shadow_enabled,
                        child_frame_style.shadow_layers,
                        child_frame_style.shadow_offset,
                        child_frame_style.shadow_opacity,
                        pointer_selection,
                    );
                    tracing::debug!(
                        parent_frame_id = render.emacs_frame_id,
                        frame_id = child_id,
                        "child_frame_lifecycle: render_child_frame_done"
                    );
                }
            }
        });
        if render.compositor.renderer_effects.needs_redraw() {
            render.mark_dirty();
        }

        if let Some(atlas) = render.compositor.glyph_atlas.as_mut() {
            atlas.set_current_frame_fonts(frame.font_bindings());
        }

        renderer.with_frame_effects(&mut render.compositor.renderer_effects, |renderer| {
            Self::render_frame_common_overlays(
                renderer,
                surface_view,
                frame,
                render.compositor.glyph_atlas.as_mut().unwrap(),
                native.width,
                native.height,
                scroll_indicators_enabled,
            );
        });
        if render.compositor.renderer_effects.needs_redraw() {
            render.mark_dirty();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_frame_window_contents(
        renderer: &mut WgpuRenderer,
        native: &GuiFrameNativeWindowState,
        render: &mut GuiFrameRenderState,
        surface_view: &wgpu::TextureView,
        frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
        present_mapping: neomacs_display_protocol::PresentMapping,
        cursor_visible: bool,
        root_animated_cursor: Option<crate::core::types::AnimatedCursor>,
        animated_cursor: Option<crate::core::types::AnimatedCursor>,
        bg_gradient: Option<((f32, f32, f32), (f32, f32, f32))>,
        include_overlays: bool,
        child_frame_style: &ChildFrameStyle,
        scroll_indicators_enabled: bool,
        toolbar: &ToolbarResources,
    ) {
        Self::render_frame_root_glyphs(
            renderer,
            render,
            surface_view,
            frame,
            present_mapping,
            cursor_visible,
            root_animated_cursor,
            bg_gradient,
        );
        let renderer_effects_still_active = render.compositor.renderer_effects.needs_redraw();

        if !include_overlays {
            render.set_dirty(renderer_effects_still_active);
            return;
        }

        Self::render_frame_window_overlays_with_toolbar_resources(
            renderer,
            native,
            render,
            surface_view,
            frame,
            cursor_visible,
            animated_cursor,
            child_frame_style,
            scroll_indicators_enabled,
            toolbar,
        );
        if renderer_effects_still_active {
            render.mark_dirty();
        }
    }

    /// Build a single-glyph mini-frame for each filled-box cursor in the frame
    /// (only the glyphs in that cursor's slot, with the frame's font tables),
    /// paired with the physical-pixel scissor rect for its cell. Called once
    /// per scene generation when the retained static scene is rebuilt; the
    /// results are reused across cursor-only frames so the font tables are not
    /// cloned every frame.
    fn build_filled_box_cursor_cells(
        frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
        scale: f32,
        pointer_appearance: &super::state::PointerAppearanceState,
    ) -> Vec<super::frame_windows::RetainedCursorCell> {
        use crate::core::frame_glyphs::{CursorStyle, FrameGlyphBuffer};
        if !Self::retained_static_pointer_appearance_allowed(pointer_appearance) {
            return Vec::new();
        }
        let mut cells = Vec::new();
        for cursor in &frame.window_cursors {
            if !matches!(cursor.style, CursorStyle::FilledBox) {
                continue;
            }
            let mut mini = FrameGlyphBuffer::with_size(frame.width, frame.height);
            mini.presentation_id = frame.presentation_id;
            mini.clone_font_bindings_from(frame);
            mini.background = frame.background;
            mini.background_alpha = frame.background_alpha;
            // Carry the parent's default glyph metrics: a metric-less frame
            // makes render_frame_glyphs fall back to invented defaults, and a
            // default-metric change clears the whole glyph atlas — so a bare
            // mini-frame silently evicted every cached glyph on each
            // filled-box cursor composite (defeating this path's "the glyph
            // is warm in the atlas" premise) and again on the next real frame.
            mini.font_pixel_size = frame.font_pixel_size;
            mini.char_height = frame.char_height;
            for glyph in &frame.glyphs {
                if glyph.slot_id() == Some(cursor.slot_id) {
                    mini.glyphs.push(glyph.clone());
                }
            }
            let mut only_cursor = cursor.clone();
            only_cursor.active = true;
            mini.window_cursors = vec![only_cursor];
            // The retained cursor cell is rendered independently from its
            // source frame. Preserve the source window's local profile so the
            // cursor-only and full-render paths resolve configuration through
            // the same local-over-global rule.
            if let Some(effects) = frame.window_cursor_effects(cursor.window_id) {
                mini.set_window_cursor_effects(cursor.window_id, effects.clone());
            }
            // The box covers the cell = the cursor rect; scissor to it in
            // physical pixels (glyph positions are logical, scaled by the
            // uniform, so the scissor rect is logical * scale).
            let scissor = (
                (cursor.x * scale).floor().max(0.0) as u32,
                (cursor.y * scale).floor().max(0.0) as u32,
                (cursor.width * scale).ceil().max(1.0) as u32,
                (cursor.height * scale).ceil().max(1.0) as u32,
            );
            cells.push(super::frame_windows::RetainedCursorCell { mini, scissor });
        }
        cells
    }

    /// Redraw each filled-box cursor's inverse-video cell over the composited
    /// scene, from the mini-frames retained for this generation. The retained
    /// static scene has the character in its normal color; a filled-box cursor
    /// covers that cell with a box in the cursor color and redraws the
    /// character in `cursor_fg`. Each cell renders scissored with
    /// `LoadOp::Load`, so no full-frame glyph work runs and the rest of the
    /// composite is preserved. The glyph is warm in the atlas from the retained
    /// build, so it is a cache hit; the box color is recomputed from the frame
    /// sample time, so it still cycles.
    fn composite_filled_box_cursor_cells(
        renderer: &mut WgpuRenderer,
        render: &mut GuiFrameRenderState,
        surface_view: &wgpu::TextureView,
        present_mapping: neomacs_display_protocol::PresentMapping,
        animated_cursor: Option<crate::core::types::AnimatedCursor>,
        mouse_pos: (f32, f32),
    ) {
        let Some(atlas) = render.compositor.glyph_atlas.as_mut() else {
            return;
        };
        let Some(retained) = render.compositor.retained_static.as_ref() else {
            return;
        };
        for cell in &retained.cursor_cells {
            atlas.set_current_frame_fonts(cell.mini.font_bindings());
            renderer.render_frame_cell_loaded(
                surface_view,
                &cell.mini,
                atlas,
                present_mapping,
                true,
                animated_cursor,
                mouse_pos,
                cell.scissor,
            );
        }
    }

    /// Whether any dynamic overlay is active. Overlays are not part of the
    /// retained static scene, so their presence forces the full render path.
    ///
    /// Idle dimming is included: it is a post-content overlay drawn *after* the
    /// cursor (glyphs.rs draw_post_content_effects, after draw_cursor_layer), so
    /// in a full render the cursor is dimmed too. The composite fast path draws
    /// the cursor over the already-dimmed retained scene, which would leave the
    /// cursor undimmed — and the retained scene's validity is not keyed on dim
    /// alpha. Falling back to the full render whenever dimming is active keeps
    /// both correct.
    fn window_has_active_overlays(render: &GuiFrameRenderState) -> bool {
        render.overlays.popup_menu.is_some()
            || render.overlays.tooltip.is_some()
            || render.overlays.visual_bell_start.is_some()
            || render.has_ime_preedit()
            || render.overlays.idle_dim.active
            // The FPS counter is redrawn from a live timer every frame; the
            // retained scene would freeze it, so a full render is required
            // while it is shown.
            || render.overlays.fps.enabled
    }

    fn retained_static_pointer_appearance_allowed(
        pointer_appearance: &super::state::PointerAppearanceState,
    ) -> bool {
        pointer_appearance.active().is_none()
    }

    /// Ensure the window's retained-static texture exists at `width`x`height`
    /// in the surface format, recreating it on a size change. Leaves the
    /// generation stamp untouched (the caller sets it after rendering).
    /// Ensure the intermediate composition texture for the full-frame post
    /// shader exists at the window's physical size; returns its view.
    fn ensure_frame_post_src(
        renderer: &mut WgpuRenderer,
        render: &mut GuiFrameRenderState,
        width: u32,
        height: u32,
    ) -> wgpu::TextureView {
        let needs_new = match &render.frame_post_src {
            Some((texture, _)) => texture.width() != width || texture.height() != height,
            None => true,
        };
        if needs_new {
            let (texture, view) = renderer.create_offscreen_texture(width, height);
            render.frame_post_src = Some((texture, view));
        }
        render
            .frame_post_src
            .as_ref()
            .expect("frame post src just ensured")
            .1
            .clone()
    }

    fn ensure_retained_static_texture(
        renderer: &WgpuRenderer,
        render: &mut GuiFrameRenderState,
        width: u32,
        height: u32,
    ) {
        let needs_new = match &render.compositor.retained_static {
            Some(rs) => rs.width != width || rs.height != height,
            None => true,
        };
        if !needs_new {
            return;
        }
        let texture = renderer.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("retained-static-scene"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: renderer.surface_format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = renderer.create_texture_bind_group(&view);
        render.compositor.retained_static = Some(super::frame_windows::RetainedStatic::new(
            texture, view, bind_group, width, height,
        ));
    }

    /// Render and present one top-level frame window, preserving the precise
    /// outcome for the frame coordinator.
    ///
    /// `compositor_only_hint` is set when the frame coordinator's plan is
    /// compositor-only; it enables the retained-static fast path (blit the
    /// retained scene, sample dynamic cursor/post state) when the scene is
    /// eligible, skipping the glyph pipeline.
    pub(super) fn render_frame_window_hinted(
        &mut self,
        emacs_frame_id: u64,
        compositor_only_hint: bool,
    ) -> PresentResult {
        self.render_frame_window_impl(emacs_frame_id, compositor_only_hint)
    }

    fn render_frame_window_impl(
        &mut self,
        emacs_frame_id: u64,
        compositor_only_hint: bool,
    ) -> PresentResult {
        if self.lifecycle_flags.shutdown_requested {
            return PresentResult::Skipped;
        }
        self.prepare_frame_state_for_render();

        let bg_gradient = if self.effects.bg_gradient.enabled {
            Some((
                self.effects.bg_gradient.top,
                self.effects.bg_gradient.bottom,
            ))
        } else {
            None
        };

        let is_primary_frame = self.frame_windows.is_primary_frame_id(emacs_frame_id);
        let Some(renderer) = self.renderer.as_mut() else {
            return PresentResult::Timeout;
        };
        let Some(window_state) = self.frame_windows.get_mut(emacs_frame_id) else {
            return PresentResult::Timeout;
        };
        #[cfg(feature = "video")]
        renderer.begin_video_surface_render();
        window_state
            .render
            .compositor
            .transitions
            .apply_policy(self.transition_policy);

        let rendered = Self::render_frame_window_contents_to_surface(
            renderer,
            window_state,
            bg_gradient,
            &self.child_frame_style,
            self.scroll_indicators_enabled,
            &self.toolbar,
            self.extra_line_spacing,
            self.extra_letter_spacing,
            compositor_only_hint,
            &self.render_policy,
            &mut self.device_lost,
        );
        let (output, frame) = match rendered {
            Ok(rendered) => rendered,
            Err(failure) => {
                #[cfg(feature = "video")]
                renderer.cancel_video_surface_render();
                return failure.present_result();
            }
        };
        if is_primary_frame {
            let (w, h) = self
                .frame_windows
                .get(emacs_frame_id)
                .map(|ws| ws.native_size())
                .unwrap_or((0, 0));
            surface_readback::maybe_log_first_frame_surface_readback(
                &mut self.debug_first_frame_readback_pending,
                &output.texture,
                renderer,
                &frame,
                w,
                h,
            );
            surface_readback::maybe_log_debug_surface_readback(
                &mut self.debug_surface_readback_frames_remaining,
                &output.texture,
                renderer,
                &frame,
                w,
                h,
            );
        }
        let (child_frame_ids, removed_child_frame_ids) = self
            .frame_windows
            .get_mut(emacs_frame_id)
            .map(|window_state| {
                let child_frame_ids = window_state
                    .render
                    .compositor
                    .child_frames
                    .sorted_for_rendering()
                    .to_vec();
                let removed_child_frame_ids = std::mem::take(
                    &mut window_state
                        .render
                        .compositor
                        .pending_child_frame_removals_to_present,
                );
                (child_frame_ids, removed_child_frame_ids)
            })
            .unwrap_or_default();
        if !child_frame_ids.is_empty() || !removed_child_frame_ids.is_empty() {
            tracing::debug!(
                parent_frame_id = emacs_frame_id,
                child_frame_ids = ?child_frame_ids,
                removed_child_frame_ids = ?removed_child_frame_ids,
                "child_frame_lifecycle: present_begin"
            );
        }
        // Let winit arm platform pacing (the Wayland surface frame
        // callback) for the upcoming present; a no-op elsewhere.
        if let Some(window) = self
            .frame_windows
            .get(emacs_frame_id)
            .and_then(|window_state| window_state.window())
        {
            window.pre_present_notify();
        }
        renderer.queue().present(output);
        #[cfg(feature = "video")]
        renderer.finish_presented_video_surface();
        super::frame_stats::note_present(
            neomacs_display_protocol::frame_time::observe_platform_now(),
        );

        if !child_frame_ids.is_empty() || !removed_child_frame_ids.is_empty() {
            tracing::debug!(
                parent_frame_id = emacs_frame_id,
                child_frame_ids = ?child_frame_ids,
                removed_child_frame_ids = ?removed_child_frame_ids,
                "child_frame_lifecycle: present_done"
            );
        }
        PresentResult::Presented
    }
}
