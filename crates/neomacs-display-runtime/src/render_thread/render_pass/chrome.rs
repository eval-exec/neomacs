//! Everything drawn *over* the composed editor picture, in window
//! coordinates: the frame's chrome bands and the transient overlays that
//! belong to the window rather than to any buffer.
//!
//! Owns: the custom titlebar, the menu / tool / compact bars, the popup menu,
//! the tooltip, the IME pre-edit box, the visual-bell flash, the rounded
//! corner mask, the FPS readout and the typing-speed readout — plus the
//! descriptor structs that carry each band's items and colors to the
//! renderer, and the `Color` -> `(r, g, b)` flattening the renderer's chrome
//! entry points still take.
//!
//! Must not: decide *whether* the frame is drawn, acquire or present a
//! surface, or choose a composition strategy. This pass runs once the target
//! view is already chosen, and it draws onto whatever view it is handed —
//! the offscreen composition slot on a transition frame, the swapchain view
//! otherwise. It is the last content-producing phase of the draw order, which
//! is why the two overlays that must keep animating (`visual_bell`,
//! `typing_speed`) report by setting the frame-dirty flag rather than by
//! scheduling anything themselves.

use super::RenderApp;
use crate::render_thread::child_frames::ChildFrameManager;
use crate::render_thread::cursor::CursorTarget;
use crate::render_thread::frame_windows::{
    GuiFrameNativeWindowState, GuiFrameRenderState, ImePreedit,
};
use crate::render_thread::state::{
    ChildFrameStyle, FpsCounter, GuiChromeInteractionState, ToolbarResources, TypingSpeedState,
    WindowChrome,
};
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

pub(in crate::render_thread) fn frame_chrome_toolbar_bounds(
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

fn update_typing_speed_state(
    state: &mut TypingSpeedState,
    sample: neomacs_display_protocol::frame_time::FrameSample,
) -> bool {
    let now = sample.frame_time();
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

fn frame_ime_preedit_overlay<'a>(
    preedit: Option<&'a ImePreedit>,
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
        let elapsed = renderer
            .frame_sample()
            .since_at_presentation(start)
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

    let frame_time = fps.cpu_span_start.elapsed().as_secs_f32() * 1000.0;
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
    let keep_redrawing = update_typing_speed_state(typing_speed, renderer.frame_sample());
    renderer.render_typing_speed(surface_view, frame, glyph_atlas, typing_speed.displayed_wpm);
    if keep_redrawing {
        *frame_dirty = true;
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_frame_window_overlays_with_toolbar_resources(
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
    RenderApp::render_frame_content_overlays(
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
    render_frame_chrome_overlays(
        renderer,
        surface_view,
        render.compositor.glyph_atlas.as_mut().unwrap(),
        GuiFrameChromeOverlays {
            native_chrome: &native.chrome,
            titlebar_background: Some((frame.background.r, frame.background.g, frame.background.b)),
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
            ime_preedit: frame_ime_preedit_overlay(
                render.input_method.preedit(),
                render.cursor.target_cloned(),
                render.emacs_frame_id,
                &render.compositor.child_frames,
            ),
        },
        native.width,
        native.height,
    );

    render_frame_visual_bell_overlay(
        renderer,
        surface_view,
        &mut render.overlays.visual_bell_start,
        &mut render.compositor.dirty,
        native.width,
        native.height,
    );

    render_frame_corner_mask(
        renderer,
        surface_view,
        &native.chrome,
        native.width,
        native.height,
    );

    if render_frame_fps_overlay(
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
        render_frame_typing_speed_overlay(
            renderer,
            surface_view,
            frame,
            render.compositor.glyph_atlas.as_mut().unwrap(),
            &mut render.overlays.typing_speed,
            &mut render.compositor.dirty,
        );
    }
}
