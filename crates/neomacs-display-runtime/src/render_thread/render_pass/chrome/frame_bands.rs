//! The frame's own furniture: the custom titlebar, the menu / tool / compact
//! bars, and the rounded corner mask.
//!
//! Owns: the descriptor structs that carry each band's items, bounds and
//! colors to the renderer, the read of `FrameChrome` that fills them from the
//! presentation being drawn, and the `Color` -> `(r, g, b)` flattening the
//! renderer's band entry points still take.
//!
//! What makes a band a band: it is part of the window's shape. It is there
//! because the frame is configured that way, it occupies reserved geometry the
//! layout engine already subtracted from the text area, and it is redrawn
//! every frame with the same content until the presentation changes it.
//! Nothing here animates, and nothing here asks for another frame — contrast
//! `transient_overlays`, whose members exist because of something that just
//! happened and several of which keep the frame dirty until they finish.
//!
//! Must not: own the order it is drawn in. The corner mask goes on *after* the
//! visual bell, so it cannot be drawn in the same pass as the bars; `super`
//! keeps that sequence.

use crate::render_thread::frame_windows::{GuiFrameNativeWindowState, GuiFrameRenderState};
use crate::render_thread::state::{ToolbarResources, WindowChrome};
use crate::thread_comm::{MenuBarItem, ToolBarItem};
use neomacs_display_protocol::frame_chrome::{FrameChromeContent, FrameRect, PositionedChromeItem};
use neomacs_renderer_wgpu::WgpuRenderer;

/// Flatten a protocol [`Color`] into the legacy `(r, g, b)` tuple the
/// renderer's chrome-overlay draw fns still take. Alpha is dropped: GUI
/// chrome colors are opaque sRGB. Follow-up: migrate the overlay draw
/// fns themselves to `Color` and delete this.
///
/// [`Color`]: neomacs_display_protocol::types::Color
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

/// Draw the titlebar and whichever bars this presentation declares, in the
/// order they stack: titlebar, menu bar, tool bar, compact bar.
pub(super) fn draw(
    renderer: &mut WgpuRenderer,
    native: &GuiFrameNativeWindowState,
    render: &mut GuiFrameRenderState,
    surface_view: &wgpu::TextureView,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    toolbar: &ToolbarResources,
) {
    let (width, height) = (native.width, native.height);
    let native_chrome = &native.chrome;
    let titlebar_background = (frame.background.r, frame.background.g, frame.background.b);

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

    let menu_bar = menu_bar.map(|(bounds, menu_bar)| GuiFrameMenuBarOverlay {
        items: menu_bar.items(),
        bounds,
        fg: color_rgb_tuple(menu_bar.foreground()),
        bg: color_rgb_tuple(menu_bar.background()),
    });
    let tool_bar = tool_bar.map(|(bounds, tool_bar)| GuiFrameToolBarOverlay {
        items: tool_bar.items(),
        bounds,
        fg: color_rgb_tuple(tool_bar.foreground()),
        bg: color_rgb_tuple(tool_bar.background()),
        toolbar,
        icon_size: tool_bar.icon_size(),
        padding: tool_bar.padding(),
    });
    let compact_bar = compact_bar.map(|(bounds, compact_bar)| GuiFrameCompactBarOverlay {
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
    });

    let interaction = render.chrome.interaction;
    let glyph_atlas = render.compositor.glyph_atlas.as_mut().unwrap();

    if !native_chrome.decorations_enabled
        && !native_chrome.is_fullscreen
        && native_chrome.titlebar_height > 0.0
    {
        renderer.render_custom_titlebar(
            surface_view,
            &native_chrome.title,
            native_chrome.titlebar_height,
            native_chrome.titlebar_hover,
            Some(titlebar_background),
            glyph_atlas,
            width,
            height,
        );
    }

    if let Some(menu_bar) = menu_bar {
        renderer.render_menu_bar(
            surface_view,
            menu_bar.items,
            menu_bar.bounds,
            menu_bar.fg,
            menu_bar.bg,
            interaction.menu_bar_hovered,
            interaction.menu_bar_active,
            glyph_atlas,
            width,
            height,
        );
    }

    if let Some(tool_bar) = tool_bar {
        renderer.render_toolbar(
            surface_view,
            tool_bar.items,
            tool_bar.bounds,
            tool_bar.fg,
            tool_bar.bg,
            &tool_bar.toolbar.icon_textures,
            interaction.toolbar_hovered,
            interaction.toolbar_pressed,
            tool_bar.icon_size,
            tool_bar.padding,
            width,
            height,
        );
    }

    if let Some(compact_bar) = compact_bar {
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
            interaction.compact_bar_menu_hovered,
            interaction.compact_bar_menu_active,
            interaction.compact_bar_tool_hovered,
            interaction.compact_bar_tool_pressed,
            compact_bar.icon_size,
            compact_bar.padding,
            glyph_atlas,
            width,
            height,
        );
    }
}

/// Cut the window's rounded corners out of whatever has been drawn so far.
///
/// Only for an undecorated, non-fullscreen window with a corner radius: a
/// decorated window's corners belong to the compositor, and a fullscreen one
/// has none.
pub(super) fn draw_corner_mask(
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
