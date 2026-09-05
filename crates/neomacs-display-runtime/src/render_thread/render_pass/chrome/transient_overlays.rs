//! The overlays that exist because of something that just happened: the popup
//! menu, the tooltip, the IME pre-edit box, the visual-bell flash, the FPS
//! readout and the typing-speed readout.
//!
//! Owns: how each one is drawn, and the small pieces of state three of them
//! carry across frames — the bell's start time, the FPS counter's smoothed
//! frame time, the typing-speed window of keypress times.
//!
//! What makes an overlay transient: it is not part of the window's shape. It
//! appears over the picture in response to a menu opening, a bell ringing, a
//! key being pressed, and it goes away on its own. Three of them are still
//! *animating* while they are drawn — the bell ramps down, the FPS number and
//! the typing-speed number decay towards a target — so they report by setting
//! the frame-dirty flag rather than by scheduling anything themselves. That is
//! the whole reason this phase runs last in the draw order: there is nothing
//! after it to invalidate.
//!
//! Must not: own the order it is drawn in. The corner mask goes on between the
//! visual bell and the FPS readout, so this is not one contiguous run; `super`
//! keeps the sequence.

use crate::render_thread::child_frames::ChildFrameManager;
use crate::render_thread::cursor::CursorTarget;
use crate::render_thread::frame_windows::{GuiFrameRenderState, ImePreedit};
use crate::render_thread::state::TypingSpeedState;
use neomacs_renderer_wgpu::WgpuRenderer;

/// How long the visual-bell flash ramps down over.
const VISUAL_BELL_DURATION_SECS: f32 = 0.15;

struct GuiFrameImeOverlay<'a> {
    text: &'a str,
    x: f32,
    y: f32,
    height: f32,
}

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

/// Draw the three panels that sit over the picture without animating: the
/// popup menu, the tooltip, and the IME pre-edit box, in that order.
pub(super) fn draw_panels(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    surface_view: &wgpu::TextureView,
    width: u32,
    height: u32,
) {
    let ime_preedit = frame_ime_preedit_overlay(
        render.input_method.preedit(),
        render.cursor.target_cloned(),
        render.emacs_frame_id,
        &render.compositor.child_frames,
    );
    let popup_menu = render.overlays.popup_menu.as_ref();
    let tooltip = render.overlays.tooltip.as_ref();
    let glyph_atlas = render.compositor.glyph_atlas.as_mut().unwrap();

    if let Some(menu) = popup_menu {
        renderer.render_popup_menu(surface_view, menu, glyph_atlas, width, height);
    }

    if let Some(tooltip) = tooltip {
        renderer.render_tooltip(surface_view, tooltip, glyph_atlas, width, height);
    }

    if let Some(preedit) = ime_preedit {
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

/// Flash the whole window, fading out over [`VISUAL_BELL_DURATION_SECS`].
///
/// Clears its own start time when the ramp is done, and keeps the frame dirty
/// until then so the fade is actually drawn rather than jumping to its end.
pub(super) fn draw_visual_bell(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    surface_view: &wgpu::TextureView,
    width: u32,
    height: u32,
) {
    let Some(start) = render.overlays.visual_bell_start else {
        return;
    };
    let elapsed = renderer
        .frame_sample()
        .since_at_presentation(start)
        .as_secs_f32();
    let duration = VISUAL_BELL_DURATION_SECS;
    if elapsed < duration {
        let alpha = (1.0 - elapsed / duration) * 0.3;
        renderer.render_visual_bell(surface_view, width, height, alpha);
        render.compositor.dirty = true;
    } else {
        render.overlays.visual_bell_start = None;
    }
}

/// Draw the FPS and frame-cost readout, and keep asking for another frame
/// while it is shown — a counter that only updates when something else
/// happens to redraw is measuring the wrong thing.
pub(super) fn draw_fps(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    surface_view: &wgpu::TextureView,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    width: u32,
    height: u32,
) {
    if !render.overlays.fps.enabled {
        return;
    }

    let glyph_count = frame.glyphs.len();
    let window_count = frame.window_infos.len();
    let transition_count = render.compositor.transitions.active_count();

    let fps = &mut render.overlays.fps;
    let frame_time = fps.cpu_span_start.elapsed().as_secs_f32() * 1000.0;
    fps.frame_time_ms = fps.frame_time_ms * 0.9 + frame_time * 0.1;
    let stats_lines = vec![
        format!("{:.0} FPS | {:.1}ms", fps.display_value, fps.frame_time_ms),
        format!(
            "{}g {}w {}t  {}x{}",
            glyph_count, window_count, transition_count, width, height
        ),
    ];

    let glyph_atlas = render.compositor.glyph_atlas.as_mut().unwrap();
    renderer.render_fps_overlay(surface_view, &stats_lines, glyph_atlas, width, height);
    render.mark_dirty();
}

/// Draw the typing-speed readout, and keep the frame dirty while the displayed
/// figure is still decaying towards its target.
pub(super) fn draw_typing_speed(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    surface_view: &wgpu::TextureView,
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
) {
    if !renderer.effects.typing_speed.enabled {
        return;
    }

    let sample = renderer.frame_sample();
    let keep_redrawing = update_typing_speed_state(&mut render.overlays.typing_speed, sample);
    let displayed_wpm = render.overlays.typing_speed.displayed_wpm;
    let glyph_atlas = render.compositor.glyph_atlas.as_mut().unwrap();
    renderer.render_typing_speed(surface_view, frame, glyph_atlas, displayed_wpm);
    if keep_redrawing {
        render.compositor.dirty = true;
    }
}
