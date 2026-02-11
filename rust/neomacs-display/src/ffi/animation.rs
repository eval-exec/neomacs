//! Animation FFI functions
//!
//! Smooth scroll, cursor blink, mouse cursor, popup menus, tooltips,
//! visual effects, effect_setter! macro and all effect configuration functions,
//! animation config, and animation stubs.

use super::*;

// ============================================================================
// Animation
// ============================================================================

/// Start smooth scroll animation
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_smooth_scroll(
    handle: *mut NeomacsDisplay,
    window_id: c_int,
    from_offset: f32,
    to_offset: f32,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    display.animations.animate_scroll(window_id, from_offset, to_offset);
}

/// Reset cursor blink (call when cursor moves)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_reset_cursor_blink(handle: *mut NeomacsDisplay) {
    if handle.is_null() {
        return;
    }

    // No-op: blink is now managed entirely by the render thread.
    // New frames reset the blink timer automatically in poll_frame().
}

/// Set mouse pointer cursor shape.
/// Types: 0=hidden, 1=default/arrow, 2=text/ibeam, 3=hand/pointer,
///        4=crosshair, 5=h-resize, 6=v-resize, 7=hourglass,
///        8=nwse-resize, 9=nesw-resize, 10=nesw-resize, 11=nwse-resize
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_mouse_cursor(
    _handle: *mut NeomacsDisplay,
    cursor_type: c_int,
) {
    let cmd = RenderCommand::SetMouseCursor {
        cursor_type,
    };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Warp (move) the mouse pointer to the given pixel position.
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_warp_mouse(
    _handle: *mut NeomacsDisplay,
    x: c_int,
    y: c_int,
) {
    let cmd = RenderCommand::WarpMouse {
        x,
        y,
    };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Popup menu item passed from C.
#[repr(C)]
pub struct CPopupMenuItem {
    pub label: *const c_char,
    pub shortcut: *const c_char,
    pub enabled: c_int,
    pub separator: c_int,
    pub submenu: c_int,
    pub depth: c_int,
}

/// Show a popup menu at position (x, y) with the given items.
/// The render thread will display the menu and send a MenuSelection event.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_show_popup_menu(
    _handle: *mut NeomacsDisplay,
    x: c_int,
    y: c_int,
    items: *const CPopupMenuItem,
    item_count: c_int,
    title: *const c_char,
    fg_color: u32,
    bg_color: u32,
) {
    let mut menu_items = Vec::new();
    for i in 0..item_count as usize {
        let item = &*items.add(i);
        let label = if item.label.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(item.label)
                .to_string_lossy()
                .into_owned()
        };
        let shortcut = if item.shortcut.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(item.shortcut)
                .to_string_lossy()
                .into_owned()
        };
        menu_items.push(PopupMenuItem {
            label,
            shortcut,
            enabled: item.enabled != 0,
            separator: item.separator != 0,
            submenu: item.submenu != 0,
            depth: item.depth as u32,
        });
    }

    let title_str = if title.is_null() {
        None
    } else {
        Some(
            std::ffi::CStr::from_ptr(title)
                .to_string_lossy()
                .into_owned(),
        )
    };

    // Convert 0xRRGGBB colors to sRGB float tuples
    let fg = if fg_color != 0 {
        Some((
            ((fg_color >> 16) & 0xFF) as f32 / 255.0,
            ((fg_color >> 8) & 0xFF) as f32 / 255.0,
            (fg_color & 0xFF) as f32 / 255.0,
        ))
    } else {
        None
    };
    let bg = if bg_color != 0 {
        Some((
            ((bg_color >> 16) & 0xFF) as f32 / 255.0,
            ((bg_color >> 8) & 0xFF) as f32 / 255.0,
            (bg_color & 0xFF) as f32 / 255.0,
        ))
    } else {
        None
    };

    let cmd = RenderCommand::ShowPopupMenu {
        x: x as f32,
        y: y as f32,
        items: menu_items,
        title: title_str,
        fg,
        bg,
    };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Hide the active popup menu.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_hide_popup_menu(
    _handle: *mut NeomacsDisplay,
) {
    let cmd = RenderCommand::HidePopupMenu;
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Show a tooltip at the given position with specified colors.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_show_tooltip(
    _handle: *mut NeomacsDisplay,
    x: f32,
    y: f32,
    text: *const c_char,
    fg_r: f32, fg_g: f32, fg_b: f32,
    bg_r: f32, bg_g: f32, bg_b: f32,
) {
    let text_str = if text.is_null() {
        return;
    } else {
        match CStr::from_ptr(text).to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return,
        }
    };
    let cmd = RenderCommand::ShowTooltip {
        x, y, text: text_str,
        fg_r, fg_g, fg_b,
        bg_r, bg_g, bg_b,
    };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Hide the active tooltip.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_hide_tooltip(
    _handle: *mut NeomacsDisplay,
) {
    let cmd = RenderCommand::HideTooltip;
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Trigger visual bell flash effect.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_visual_bell(
    _handle: *mut NeomacsDisplay,
) {
    let cmd = RenderCommand::VisualBell;
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Remove a child frame from the render thread.
/// Called when a child frame is deleted or unparented.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_remove_child_frame(
    _handle: *mut NeomacsDisplay,
    frame_id: u64,
) {
    let cmd = RenderCommand::RemoveChildFrame { frame_id };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Configure child frame visual style (drop shadow, rounded corners).
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_child_frame_style(
    _handle: *mut NeomacsDisplay,
    corner_radius: f32,
    shadow_enabled: c_int,
    shadow_layers: c_int,
    shadow_offset: f32,
    shadow_opacity: f32,
) {
    let cmd = RenderCommand::SetChildFrameStyle {
        corner_radius,
        shadow_enabled: shadow_enabled != 0,
        shadow_layers: shadow_layers.max(0) as u32,
        shadow_offset,
        shadow_opacity,
    };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Request window attention (urgency hint / taskbar flash).
/// If urgent is non-zero, uses Critical attention type; otherwise Informational.
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_request_attention(
    _handle: *mut NeomacsDisplay,
    urgent: c_int,
) {
    let cmd = RenderCommand::RequestAttention { urgent: urgent != 0 };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Enable or disable scroll indicators and focus ring.
/// enabled: non-zero = on, zero = off.
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_scroll_indicators(
    _handle: *mut NeomacsDisplay,
    enabled: c_int,
) {
    let cmd = RenderCommand::SetScrollIndicators { enabled: enabled != 0 };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Set the custom title bar height (0 = hidden)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_titlebar_height(
    _handle: *mut NeomacsDisplay,
    height: c_int,
) {
    let cmd = RenderCommand::SetTitlebarHeight { height: height as f32 };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Toggle FPS counter overlay
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_show_fps(
    _handle: *mut NeomacsDisplay,
    enabled: c_int,
) {
    let cmd = RenderCommand::SetShowFps { enabled: enabled != 0 };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Set window corner radius for borderless mode (0 = square corners)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_corner_radius(
    _handle: *mut NeomacsDisplay,
    radius: c_int,
) {
    let cmd = RenderCommand::SetCornerRadius { radius: radius as f32 };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Set extra line spacing and letter spacing (in pixels)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_extra_spacing(
    _handle: *mut NeomacsDisplay,
    line_spacing: c_int,
    letter_spacing: c_int,
) {
    let cmd = RenderCommand::SetExtraSpacing {
        line_spacing: line_spacing as f32,
        letter_spacing: letter_spacing as f32,
    };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Enable or disable font ligature support.
/// When enabled, the layout engine groups same-face character runs and emits
/// them as composed glyphs so that cosmic-text/HarfBuzz can perform ligature
/// substitution (e.g., -> becomes an arrow in JetBrains Mono).
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_ligatures_enabled(
    _handle: *mut NeomacsDisplay,
    enabled: c_int,
) {
    let on = enabled != 0;

    // Set on the layout engine (Emacs thread).
    // If the engine isn't initialized yet (e.g., called from init.el before
    // first layout frame), store as pending value that will be applied on init.
    use crate::ffi::layout::{LAYOUT_ENGINE, PENDING_LIGATURES_ENABLED};
    if let Some(ref mut engine) = *std::ptr::addr_of_mut!(LAYOUT_ENGINE) {
        engine.ligatures_enabled = on;
    }
    // Always store pending so engine init picks it up even if set before creation
    *std::ptr::addr_of_mut!(PENDING_LIGATURES_ENABLED) = Some(on);

    // Also send to render thread for logging/future use
    let cmd = RenderCommand::SetLigaturesEnabled { enabled: on };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

// ============================================================================
// effect_setter! macro and invocations
// ============================================================================

/// Set background gradient (top and bottom colors, sRGB 0-255)
/// Macro to generate FFI effect setter functions.
///
/// Generates a `#[no_mangle] pub unsafe extern "C" fn` that wraps the closure body
/// in `RenderCommand::UpdateEffect(EffectUpdater(...))` boilerplate.
///
/// Usage:
/// ```ignore
/// effect_setter!(neomacs_display_set_foo(enabled: c_int, r: c_int) |effects| {
///     effects.foo.enabled = enabled != 0;
///     effects.foo.color_r = r as f32 / 255.0;
/// });
/// ```
macro_rules! effect_setter {
    ($fn_name:ident($($param:ident : $ptype:ty),* $(,)?) |$eff:ident| { $($body:tt)* }) => {
        #[no_mangle]
        pub unsafe extern "C" fn $fn_name(
            _handle: *mut NeomacsDisplay,
            $($param: $ptype),*
        ) {
            let cmd = RenderCommand::UpdateEffect(EffectUpdater(Box::new(move |$eff| {
                $($body)*
            })));
            if let Some(ref state) = THREADED_STATE {
                let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            }
        }
    };
}


effect_setter!(neomacs_display_set_background_gradient(enabled: c_int, top_r: c_int, top_g: c_int, top_b: c_int, bottom_r: c_int, bottom_g: c_int, bottom_b: c_int) |effects| {
        effects.bg_gradient.enabled = enabled != 0;
                    effects.bg_gradient.top = (top_r as f32 / 255.0, top_g as f32 / 255.0, top_b as f32 / 255.0);
                    effects.bg_gradient.bottom = (bottom_r as f32 / 255.0, bottom_g as f32 / 255.0, bottom_b as f32 / 255.0);
});

/// Configure scroll bar appearance
effect_setter!(neomacs_display_set_scroll_bar_config(width: c_int, thumb_radius: c_int, track_opacity: c_int, hover_brightness: c_int) |effects| {
        effects.scroll_bar.width = width;
                    effects.scroll_bar.thumb_radius = thumb_radius as f32 / 100.0;
                    effects.scroll_bar.track_opacity = track_opacity as f32 / 100.0;
                    effects.scroll_bar.hover_brightness = hover_brightness as f32 / 100.0;
});


/// Configure indent guide rendering
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_indent_guides(
    _handle: *mut NeomacsDisplay,
    enabled: c_int,
    r: c_int, g: c_int, b: c_int,
    opacity: c_int,
) {
    let c = crate::core::types::Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, opacity as f32 / 100.0).srgb_to_linear();
    let cmd = RenderCommand::UpdateEffect(EffectUpdater(Box::new(move |effects| {
            effects.indent_guides.enabled = enabled != 0;
            effects.indent_guides.color = (c.r, c.g, c.b, c.a);
        })));
        if let Some(ref state) = THREADED_STATE {
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
        }
}

/// Configure rainbow indent guide colors (up to 6 cycling colors by depth)
/// Each color is passed as R,G,B (0-255) with opacity (0-100).
/// num_colors specifies how many color slots are used (max 6).
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_indent_guide_rainbow(
    _handle: *mut NeomacsDisplay,
    enabled: c_int,
    num_colors: c_int,
    r1: c_int, g1: c_int, b1: c_int, o1: c_int,
    r2: c_int, g2: c_int, b2: c_int, o2: c_int,
    r3: c_int, g3: c_int, b3: c_int, o3: c_int,
    r4: c_int, g4: c_int, b4: c_int, o4: c_int,
    r5: c_int, g5: c_int, b5: c_int, o5: c_int,
    r6: c_int, g6: c_int, b6: c_int, o6: c_int,
) {
    let all = [
        (r1, g1, b1, o1), (r2, g2, b2, o2), (r3, g3, b3, o3),
        (r4, g4, b4, o4), (r5, g5, b5, o5), (r6, g6, b6, o6),
    ];
    let n = (num_colors as usize).min(6);
    let colors: Vec<(f32, f32, f32, f32)> = all[..n].iter().map(|(r, g, b, o)| {
        (*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, *o as f32 / 100.0)
    }).collect();
    let cmd = RenderCommand::SetIndentGuideRainbow {
        enabled: enabled != 0,
        colors,
    };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Configure current line highlight rendering
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_line_highlight(
    _handle: *mut NeomacsDisplay,
    enabled: c_int,
    r: c_int, g: c_int, b: c_int,
    opacity: c_int,
) {
    let c = crate::core::types::Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, opacity as f32 / 100.0).srgb_to_linear();
    let cmd = RenderCommand::UpdateEffect(EffectUpdater(Box::new(move |effects| {
            effects.line_highlight.enabled = enabled != 0;
            effects.line_highlight.color = (c.r, c.g, c.b, c.a);
        })));
        if let Some(ref state) = THREADED_STATE {
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
        }
}

/// Configure visible whitespace rendering
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_show_whitespace(
    _handle: *mut NeomacsDisplay,
    enabled: c_int,
    r: c_int, g: c_int, b: c_int,
    opacity: c_int,
) {
    let c = crate::core::types::Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, opacity as f32 / 100.0).srgb_to_linear();
    let cmd = RenderCommand::UpdateEffect(EffectUpdater(Box::new(move |effects| {
            effects.show_whitespace.enabled = enabled != 0;
            effects.show_whitespace.color = (c.r, c.g, c.b, c.a);
        })));
        if let Some(ref state) = THREADED_STATE {
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
        }
}

// The remaining effect_setter! invocations and manual effect functions are
// included verbatim from the original ffi.rs. Due to the large number of
// these (100+ effects), they are included via a separate include mechanism.
// For the split, we keep them all in this file.

// --- effect_setter! invocations (alphabetical-ish, matching original order) ---

effect_setter!(neomacs_display_set_inactive_dim(enabled: c_int, opacity: c_int) |effects| {
        effects.inactive_dim.enabled = enabled != 0;
                    effects.inactive_dim.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_cursor_glow(enabled: c_int, r: c_int, g: c_int, b: c_int, radius: c_int, opacity: c_int) |effects| {
        effects.cursor_glow.enabled = enabled != 0;
                    effects.cursor_glow.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_glow.radius = radius as f32;
                    effects.cursor_glow.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_cursor_pulse(enabled: c_int, speed: c_int, min_opacity: c_int) |effects| {
        effects.cursor_pulse.enabled = enabled != 0;
                    effects.cursor_pulse.speed = speed as f32 / 100.0;
                    effects.cursor_pulse.min_opacity = min_opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_focus_mode(enabled: c_int, opacity: c_int) |effects| {
        effects.focus_mode.enabled = enabled != 0;
                    effects.focus_mode.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_minimap(enabled: c_int, width: c_int) |effects| {
        effects.minimap.enabled = enabled != 0;
                    effects.minimap.width = width as f32;
});

effect_setter!(neomacs_display_set_typing_ripple(enabled: c_int, max_radius: c_int, duration_ms: c_int) |effects| {
        effects.typing_ripple.enabled = enabled != 0;
                    effects.typing_ripple.max_radius = max_radius as f32;
                    effects.typing_ripple.duration_ms = duration_ms as u32;
});

effect_setter!(neomacs_display_set_search_pulse(enabled: c_int, face_id: c_int) |effects| {
        effects.search_pulse.enabled = enabled != 0;
                    effects.search_pulse.face_id = face_id as u32;
});

effect_setter!(neomacs_display_set_zen_mode(enabled: c_int, content_width_pct: c_int, margin_opacity: c_int) |effects| {
        effects.zen_mode.enabled = enabled != 0;
                    effects.zen_mode.content_width_pct = content_width_pct as f32;
                    effects.zen_mode.margin_opacity = margin_opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_background_pattern(style: c_int, spacing: c_int, r: c_int, g: c_int, b: c_int, opacity: c_int) |effects| {
        effects.bg_pattern.style = style as u32;
                    effects.bg_pattern.spacing = spacing as f32;
                    effects.bg_pattern.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.bg_pattern.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_cursor_color_cycle(enabled: c_int, speed: c_int, saturation: c_int, lightness: c_int) |effects| {
        effects.cursor_color_cycle.enabled = enabled != 0;
                    effects.cursor_color_cycle.speed = speed as f32 / 100.0;
                    effects.cursor_color_cycle.saturation = saturation as f32 / 100.0;
                    effects.cursor_color_cycle.lightness = lightness as f32 / 100.0;
});

effect_setter!(neomacs_display_set_header_shadow(enabled: c_int, intensity: c_int, size: c_int) |effects| {
        effects.header_shadow.enabled = enabled != 0;
                    effects.header_shadow.intensity = intensity as f32 / 100.0;
                    effects.header_shadow.size = size as f32;
});

effect_setter!(neomacs_display_set_line_animation(enabled: c_int, duration_ms: c_int) |effects| {
        effects.line_animation.enabled = enabled != 0;
                    effects.line_animation.duration_ms = duration_ms as u32;
});

effect_setter!(neomacs_display_set_vignette(enabled: c_int, intensity: c_int, radius: c_int) |effects| {
        effects.vignette.enabled = enabled != 0;
                    effects.vignette.intensity = intensity as f32 / 100.0;
                    effects.vignette.radius = radius as f32;
});

effect_setter!(neomacs_display_set_inactive_tint(enabled: c_int, r: c_int, g: c_int, b: c_int, opacity: c_int) |effects| {
        effects.inactive_tint.enabled = enabled != 0;
                    effects.inactive_tint.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.inactive_tint.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_scroll_progress(enabled: c_int, height: c_int, r: c_int, g: c_int, b: c_int, opacity: c_int) |effects| {
        effects.scroll_progress.enabled = enabled != 0;
                    effects.scroll_progress.height = height as f32;
                    effects.scroll_progress.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.scroll_progress.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_window_glow(enabled: c_int, r: c_int, g: c_int, b: c_int, radius: c_int, intensity: c_int) |effects| {
        effects.window_glow.enabled = enabled != 0;
                    effects.window_glow.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.window_glow.radius = radius as f32;
                    effects.window_glow.intensity = intensity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_breadcrumb(enabled: c_int, opacity: c_int) |effects| {
        effects.breadcrumb.enabled = enabled != 0;
                    effects.breadcrumb.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_border_transition(enabled: c_int, r: c_int, g: c_int, b: c_int, duration_ms: c_int) |effects| {
        effects.border_transition.enabled = enabled != 0;
                    effects.border_transition.active_color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.border_transition.duration_ms = duration_ms as u32;
});

effect_setter!(neomacs_display_set_accent_strip(enabled: c_int, width: c_int) |effects| {
        effects.accent_strip.enabled = enabled != 0;
                    effects.accent_strip.width = width as f32;
});

effect_setter!(neomacs_display_set_frosted_glass(enabled: c_int, opacity: c_int, blur: c_int) |effects| {
        effects.frosted_glass.enabled = enabled != 0;
                    effects.frosted_glass.opacity = opacity as f32 / 100.0;
                    effects.frosted_glass.blur = blur as f32;
});

effect_setter!(neomacs_display_set_typing_speed(enabled: c_int) |effects| {
        effects.typing_speed.enabled = enabled != 0;
});

effect_setter!(neomacs_display_set_title_fade(enabled: c_int, duration_ms: c_int) |effects| {
        effects.title_fade.enabled = enabled != 0;
                    effects.title_fade.duration_ms = duration_ms as u32;
});

effect_setter!(neomacs_display_set_region_glow(enabled: c_int, face_id: c_int, radius: c_int, opacity: c_int) |effects| {
        effects.region_glow.enabled = enabled != 0;
                    effects.region_glow.face_id = face_id as u32;
                    effects.region_glow.radius = radius as f32;
                    effects.region_glow.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_cursor_shadow(enabled: c_int, offset_x: c_int, offset_y: c_int, opacity: c_int) |effects| {
        effects.cursor_shadow.enabled = enabled != 0;
                    effects.cursor_shadow.offset_x = offset_x as f32;
                    effects.cursor_shadow.offset_y = offset_y as f32;
                    effects.cursor_shadow.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_focus_ring(enabled: c_int, r: c_int, g: c_int, b: c_int, opacity: c_int, dash_length: c_int, speed: c_int) |effects| {
        effects.focus_ring.enabled = enabled != 0;
                    effects.focus_ring.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.focus_ring.opacity = opacity as f32 / 100.0;
                    effects.focus_ring.dash_length = dash_length as f32;
                    effects.focus_ring.speed = speed as f32;
});

effect_setter!(neomacs_display_set_window_mode_tint(enabled: c_int, opacity: c_int) |effects| {
        effects.window_mode_tint.enabled = enabled != 0;
                    effects.window_mode_tint.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_window_watermark(enabled: c_int, opacity: c_int, threshold: c_int) |effects| {
        effects.window_watermark.enabled = enabled != 0;
                    effects.window_watermark.opacity = opacity as f32 / 100.0;
                    effects.window_watermark.threshold = threshold as u32;
});


/// Configure cursor trail fade effect
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_cursor_trail_fade(
    _handle: *mut NeomacsDisplay,
    enabled: c_int,
    length: c_int,
    fade_ms: c_int,
) {
    let cmd = RenderCommand::UpdateEffect(EffectUpdater(Box::new(move |effects| {
            effects.cursor_trail_fade.enabled = enabled != 0;
            effects.cursor_trail_fade.length = length as u32 as usize;
            effects.cursor_trail_fade.ms = fade_ms as u32;
        })));
        if let Some(ref state) = THREADED_STATE {
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
        }
}

/// Configure idle screen dimming after inactivity
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_idle_dim(
    _handle: *mut NeomacsDisplay,
    enabled: c_int,
    delay_secs: c_int,
    opacity: c_int,
    fade_ms: c_int,
) {
    let cmd = RenderCommand::UpdateEffect(EffectUpdater(Box::new(move |effects| {
            effects.idle_dim.enabled = enabled != 0;
            effects.idle_dim.delay = std::time::Duration::from_secs_f32(delay_secs as f32);
            effects.idle_dim.opacity = opacity as f32 / 100.0;
            effects.idle_dim.fade_duration = std::time::Duration::from_millis(fade_ms as u32 as u64);
        })));
        if let Some(ref state) = THREADED_STATE {
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
        }
}

effect_setter!(neomacs_display_set_noise_grain(enabled: c_int, intensity: c_int, size: c_int) |effects| {
        effects.noise_grain.enabled = enabled != 0;
                    effects.noise_grain.intensity = intensity as f32 / 100.0;
                    effects.noise_grain.size = size as f32;
});

effect_setter!(neomacs_display_set_mode_line_transition(enabled: c_int, duration_ms: c_int) |effects| {
        effects.mode_line_transition.enabled = enabled != 0;
                    effects.mode_line_transition.duration_ms = duration_ms as u32;
});

effect_setter!(neomacs_display_set_cursor_wake(enabled: c_int, duration_ms: c_int, scale_pct: c_int) |effects| {
        effects.cursor_wake.enabled = enabled != 0;
                    effects.cursor_wake.duration_ms = duration_ms as u32;
                    effects.cursor_wake.scale = scale_pct as f32 / 100.0;
});

effect_setter!(neomacs_display_set_window_content_shadow(enabled: c_int, size: c_int, opacity: c_int) |effects| {
        effects.window_content_shadow.enabled = enabled != 0;
                    effects.window_content_shadow.size = size as f32;
                    effects.window_content_shadow.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_edge_snap(enabled: c_int, r: c_int, g: c_int, b: c_int, duration_ms: c_int) |effects| {
        effects.edge_snap.enabled = enabled != 0;
                    effects.edge_snap.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.edge_snap.duration_ms = duration_ms as u32;
});

effect_setter!(neomacs_display_set_cursor_crosshair(enabled: c_int, r: c_int, g: c_int, b: c_int, opacity: c_int) |effects| {
        effects.cursor_crosshair.enabled = enabled != 0;
                    effects.cursor_crosshair.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_crosshair.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_modified_indicator(enabled: c_int, r: c_int, g: c_int, b: c_int, width: c_int, opacity: c_int) |effects| {
        effects.modified_indicator.enabled = enabled != 0;
                    effects.modified_indicator.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.modified_indicator.width = width as f32;
                    effects.modified_indicator.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_stained_glass(enabled: c_int, opacity: c_int, saturation: c_int) |effects| {
        effects.stained_glass.enabled = enabled != 0;
                    effects.stained_glass.opacity = opacity as f32 / 100.0;
                    effects.stained_glass.saturation = saturation as f32 / 100.0;
});

effect_setter!(neomacs_display_set_focus_gradient_border(enabled: c_int, top_r: c_int, top_g: c_int, top_b: c_int, bot_r: c_int, bot_g: c_int, bot_b: c_int, width: c_int, opacity: c_int) |effects| {
        effects.focus_gradient_border.enabled = enabled != 0;
                    effects.focus_gradient_border.top_color = (top_r as f32 / 255.0, top_g as f32 / 255.0, top_b as f32 / 255.0);
                    effects.focus_gradient_border.bot_color = (bot_r as f32 / 255.0, bot_g as f32 / 255.0, bot_b as f32 / 255.0);
                    effects.focus_gradient_border.width = width as f32;
                    effects.focus_gradient_border.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_cursor_magnetism(enabled: c_int, r: c_int, g: c_int, b: c_int, ring_count: c_int, duration_ms: c_int, opacity: c_int) |effects| {
        effects.cursor_magnetism.enabled = enabled != 0;
                    effects.cursor_magnetism.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_magnetism.ring_count = ring_count as u32;
                    effects.cursor_magnetism.duration_ms = duration_ms as u32;
                    effects.cursor_magnetism.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_depth_shadow(enabled: c_int, layers: c_int, offset: c_int, r: c_int, g: c_int, b: c_int, opacity: c_int) |effects| {
        effects.depth_shadow.enabled = enabled != 0;
                    effects.depth_shadow.layers = layers as u32;
                    effects.depth_shadow.offset = offset as f32;
                    effects.depth_shadow.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.depth_shadow.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_mode_line_gradient(enabled: c_int, left_r: c_int, left_g: c_int, left_b: c_int, right_r: c_int, right_g: c_int, right_b: c_int, opacity: c_int) |effects| {
        effects.mode_line_gradient.enabled = enabled != 0;
                    effects.mode_line_gradient.left_color = (left_r as f32 / 255.0, left_g as f32 / 255.0, left_b as f32 / 255.0);
                    effects.mode_line_gradient.right_color = (right_r as f32 / 255.0, right_g as f32 / 255.0, right_b as f32 / 255.0);
                    effects.mode_line_gradient.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_corner_fold(enabled: c_int, size: c_int, r: c_int, g: c_int, b: c_int, opacity: c_int) |effects| {
        effects.corner_fold.enabled = enabled != 0;
                    effects.corner_fold.size = size as f32;
                    effects.corner_fold.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.corner_fold.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_frosted_border(enabled: c_int, width: c_int, opacity: c_int, r: c_int, g: c_int, b: c_int) |effects| {
        effects.frosted_border.enabled = enabled != 0;
                    effects.frosted_border.width = width as f32;
                    effects.frosted_border.opacity = opacity as f32 / 100.0;
                    effects.frosted_border.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
});

effect_setter!(neomacs_display_set_line_number_pulse(enabled: c_int, r: c_int, g: c_int, b: c_int, intensity: c_int, cycle_ms: c_int) |effects| {
        effects.line_number_pulse.enabled = enabled != 0;
                    effects.line_number_pulse.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.line_number_pulse.intensity = intensity as f32 / 100.0;
                    effects.line_number_pulse.cycle_ms = cycle_ms as u32;
});

effect_setter!(neomacs_display_set_breathing_border(enabled: c_int, r: c_int, g: c_int, b: c_int, min_opacity: c_int, max_opacity: c_int, cycle_ms: c_int) |effects| {
        effects.breathing_border.enabled = enabled != 0;
                    effects.breathing_border.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.breathing_border.min_opacity = min_opacity as f32 / 100.0;
                    effects.breathing_border.max_opacity = max_opacity as f32 / 100.0;
                    effects.breathing_border.cycle_ms = cycle_ms as u32;
});

effect_setter!(neomacs_display_set_scanlines(enabled: c_int, spacing: c_int, opacity: c_int, r: c_int, g: c_int, b: c_int) |effects| {
        effects.scanlines.enabled = enabled != 0;
                    effects.scanlines.spacing = spacing as u32;
                    effects.scanlines.opacity = opacity as f32 / 100.0;
                    effects.scanlines.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
});

effect_setter!(neomacs_display_set_cursor_comet(enabled: c_int, trail_length: c_int, fade_ms: c_int, r: c_int, g: c_int, b: c_int, opacity: c_int) |effects| {
        effects.cursor_comet.enabled = enabled != 0;
                    effects.cursor_comet.trail_length = trail_length as u32;
                    effects.cursor_comet.fade_ms = fade_ms as u32;
                    effects.cursor_comet.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_comet.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_cursor_spotlight(enabled: c_int, radius: c_int, intensity: c_int, r: c_int, g: c_int, b: c_int) |effects| {
        effects.cursor_spotlight.enabled = enabled != 0;
                    effects.cursor_spotlight.radius = radius as f32;
                    effects.cursor_spotlight.intensity = intensity as f32 / 100.0;
                    effects.cursor_spotlight.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
});

effect_setter!(neomacs_display_set_cursor_particles(enabled: c_int, r: c_int, g: c_int, b: c_int, count: c_int, lifetime_ms: c_int, gravity: c_int) |effects| {
        effects.cursor_particles.enabled = enabled != 0;
                    effects.cursor_particles.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_particles.count = count as u32;
                    effects.cursor_particles.lifetime_ms = lifetime_ms as u32;
                    effects.cursor_particles.gravity = gravity as f32;
});

effect_setter!(neomacs_display_set_window_border_radius(enabled: c_int, radius: c_int, border_width: c_int, r: c_int, g: c_int, b: c_int, opacity: c_int) |effects| {
        effects.window_border_radius.enabled = enabled != 0;
                    effects.window_border_radius.radius = radius as f32;
                    effects.window_border_radius.width = border_width as f32;
                    effects.window_border_radius.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.window_border_radius.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_typing_heatmap(enabled: c_int, r: c_int, g: c_int, b: c_int, fade_ms: c_int, opacity: c_int) |effects| {
        effects.typing_heatmap.enabled = enabled != 0;
                    effects.typing_heatmap.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.typing_heatmap.fade_ms = fade_ms as u32;
                    effects.typing_heatmap.opacity = opacity as f32 / 100.0;
});


/// Configure smooth theme transition (crossfade on background color change)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_theme_transition(
    _handle: *mut NeomacsDisplay,
    enabled: c_int,
    duration_ms: c_int,
) {
    let cmd = RenderCommand::UpdateEffect(EffectUpdater(Box::new(move |effects| {
            effects.theme_transition.enabled = enabled != 0;
            effects.theme_transition.duration = std::time::Duration::from_millis(duration_ms as u32 as u64);
        })));
        if let Some(ref state) = THREADED_STATE {
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
        }
}

effect_setter!(neomacs_display_set_click_halo(enabled: c_int, r: c_int, g: c_int, b: c_int, duration_ms: c_int, max_radius: c_int) |effects| {
        effects.click_halo.enabled = enabled != 0;
                    effects.click_halo.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.click_halo.duration_ms = duration_ms as u32;
                    effects.click_halo.max_radius = max_radius as f32;
});

effect_setter!(neomacs_display_set_scroll_velocity_fade(enabled: c_int, max_opacity: c_int, fade_ms: c_int) |effects| {
        effects.scroll_velocity_fade.enabled = enabled != 0;
                    effects.scroll_velocity_fade.max_opacity = max_opacity as f32 / 100.0;
                    effects.scroll_velocity_fade.ms = fade_ms as u32;
});

effect_setter!(neomacs_display_set_minibuffer_highlight(enabled: c_int, r: c_int, g: c_int, b: c_int, opacity: c_int) |effects| {
        effects.minibuffer_highlight.enabled = enabled != 0;
                    effects.minibuffer_highlight.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.minibuffer_highlight.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_resize_padding(enabled: c_int, duration_ms: c_int, max_padding: c_int) |effects| {
        effects.resize_padding.enabled = enabled != 0;
                    effects.resize_padding.duration_ms = duration_ms as u32;
                    effects.resize_padding.max = max_padding as f32;
});

effect_setter!(neomacs_display_set_cursor_error_pulse(enabled: c_int, r: c_int, g: c_int, b: c_int, duration_ms: c_int) |effects| {
        effects.cursor_error_pulse.enabled = enabled != 0;
                    effects.cursor_error_pulse.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_error_pulse.duration_ms = duration_ms as u32;
});

effect_setter!(neomacs_display_set_wrap_indicator(enabled: c_int, r: c_int, g: c_int, b: c_int, opacity: c_int) |effects| {
        effects.wrap_indicator.enabled = enabled != 0;
                    effects.wrap_indicator.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.wrap_indicator.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_scroll_momentum(enabled: c_int, fade_ms: c_int, width: c_int) |effects| {
        effects.scroll_momentum.enabled = enabled != 0;
                    effects.scroll_momentum.fade_ms = fade_ms as u32;
                    effects.scroll_momentum.width = width as f32;
});

effect_setter!(neomacs_display_set_text_fade_in(enabled: c_int, duration_ms: c_int) |effects| {
        effects.text_fade_in.enabled = enabled != 0;
                    effects.text_fade_in.duration_ms = duration_ms as u32;
});

effect_setter!(neomacs_display_set_scroll_line_spacing(enabled: c_int, max_spacing: c_int, duration_ms: c_int) |effects| {
        effects.scroll_line_spacing.enabled = enabled != 0;
                    effects.scroll_line_spacing.max = max_spacing as f32;
                    effects.scroll_line_spacing.duration_ms = duration_ms as u32;
});

effect_setter!(neomacs_display_set_padding_gradient(enabled: c_int, r: c_int, g: c_int, b: c_int, opacity: c_int, width: c_int) |effects| {
        effects.padding_gradient.enabled = enabled != 0;
                    effects.padding_gradient.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.padding_gradient.opacity = opacity as f32 / 100.0;
                    effects.padding_gradient.width = width as f32;
});


/// Configure smooth cursor size transition on text-scale-adjust
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_cursor_size_transition(
    _handle: *mut NeomacsDisplay,
    enabled: c_int,
    duration_ms: c_int,
) {
    let cmd = RenderCommand::SetCursorSizeTransition {
        enabled: enabled != 0,
        duration_ms: duration_ms as u32,
    };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

effect_setter!(neomacs_display_set_window_switch_fade(enabled: c_int, duration_ms: c_int, intensity: c_int) |effects| {
        effects.window_switch_fade.enabled = enabled != 0;
                    effects.window_switch_fade.duration_ms = duration_ms as u32;
                    effects.window_switch_fade.intensity = intensity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_mode_line_separator(style: c_int, r: c_int, g: c_int, b: c_int, height: c_int) |effects| {
        effects.mode_line_separator.style = style as u32;
                    effects.mode_line_separator.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.mode_line_separator.height = height as f32;
});


/// Set the window title (threaded mode)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_title(
    _handle: *mut NeomacsDisplay,
    title: *const c_char,
) {
    let title_str = if title.is_null() {
        "Emacs".to_string()
    } else {
        CStr::from_ptr(title).to_string_lossy().into_owned()
    };
    let cmd = RenderCommand::SetWindowTitle { title: title_str };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Set fullscreen mode (threaded mode)
/// mode: 0=none, 1=width, 2=height, 3=both, 4=maximized
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_fullscreen(
    _handle: *mut NeomacsDisplay,
    mode: c_int,
) {
    let cmd = RenderCommand::SetWindowFullscreen { mode: mode as u32 };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Minimize/iconify the window (threaded mode)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_minimized(
    _handle: *mut NeomacsDisplay,
    minimized: c_int,
) {
    let cmd = RenderCommand::SetWindowMinimized { minimized: minimized != 0 };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Set window position (threaded mode)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_position(
    _handle: *mut NeomacsDisplay,
    x: c_int,
    y: c_int,
) {
    let cmd = RenderCommand::SetWindowPosition { x, y };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Request window inner size change (threaded mode)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_request_size(
    _handle: *mut NeomacsDisplay,
    width: c_int,
    height: c_int,
) {
    let cmd = RenderCommand::SetWindowSize { width: width as u32, height: height as u32 };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Set window decorations (threaded mode)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_decorated(
    _handle: *mut NeomacsDisplay,
    decorated: c_int,
) {
    let cmd = RenderCommand::SetWindowDecorated { decorated: decorated != 0 };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Configure cursor blinking (enable/disable and interval)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_cursor_blink(
    handle: *mut NeomacsDisplay,
    enabled: c_int,
    interval_ms: c_int,
) {
    if handle.is_null() {
        return;
    }

    let cmd = RenderCommand::SetCursorBlink {
        enabled: enabled != 0,
        interval_ms: if interval_ms > 0 { interval_ms as u32 } else { 500 },
    };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Configure cursor animation (smooth motion)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_cursor_animation(
    _handle: *mut NeomacsDisplay,
    enabled: c_int,
    speed: f32,
) {
    let cmd = RenderCommand::SetCursorAnimation {
        enabled: enabled != 0,
        speed: if speed > 0.0 { speed } else { 15.0 },
    };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Configure all animation settings
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_animation_config(
    _handle: *mut NeomacsDisplay,
    cursor_enabled: c_int,
    cursor_speed: f32,
    cursor_style: u8,
    cursor_duration_ms: u32,
    crossfade_enabled: c_int,
    crossfade_duration_ms: u32,
    scroll_enabled: c_int,
    scroll_duration_ms: u32,
    scroll_effect: u32,
    scroll_easing: u32,
    trail_size: f32,
    crossfade_effect: u32,
    crossfade_easing: u32,
) {
    use crate::core::types::CursorAnimStyle;
    let cmd = RenderCommand::SetAnimationConfig {
        cursor_enabled: cursor_enabled != 0,
        cursor_speed: if cursor_speed > 0.0 { cursor_speed } else { 15.0 },
        cursor_style: CursorAnimStyle::from_u8(cursor_style),
        cursor_duration_ms: if cursor_duration_ms > 0 { cursor_duration_ms } else { 150 },
        crossfade_enabled: crossfade_enabled != 0,
        crossfade_duration_ms: if crossfade_duration_ms > 0 { crossfade_duration_ms } else { 200 },
        scroll_enabled: scroll_enabled != 0,
        scroll_duration_ms: if scroll_duration_ms > 0 { scroll_duration_ms } else { 150 },
        scroll_effect,
        scroll_easing,
        trail_size: if trail_size >= 0.0 { trail_size } else { 0.7 },
        crossfade_effect,
        crossfade_easing,
    };
    if let Some(ref state) = THREADED_STATE {
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Check if animations are active
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_has_animations(handle: *mut NeomacsDisplay) -> c_int {
    if handle.is_null() {
        return 0;
    }

    let display = &mut *handle;
    display.animations.has_active_animations() as c_int
}

// ============================================================================
// Animation FFI functions (stubs - no GTK4 backend)
// ============================================================================

/// Set an animation configuration option (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_animation_option(
    _handle: *mut NeomacsDisplay,
    _key: *const c_char,
    _value: *const c_char,
) -> c_int {
    0
}

/// Get an animation configuration option (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_get_animation_option(
    _handle: *mut NeomacsDisplay,
    _key: *const c_char,
) -> *mut c_char {
    ptr::null_mut()
}

/// Free a string returned by neomacs_display_get_animation_option
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_free_string(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}

/// Update cursor animation state (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_update_animation(
    _handle: *mut NeomacsDisplay,
    _dt: c_double,
) -> c_int {
    0
}

/// Check if animation needs continuous redraw (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_animation_active(
    _handle: *mut NeomacsDisplay,
) -> c_int {
    0
}

/// Trigger a buffer transition animation (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_start_buffer_transition(
    _handle: *mut NeomacsDisplay,
    _effect: *const c_char,
    _duration_ms: c_int,
) -> c_int {
    0
}

/// Prepare for buffer transition (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_prepare_buffer_transition(
    _handle: *mut NeomacsDisplay,
) -> c_int {
    0
}

/// Trigger buffer transition animation (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_trigger_buffer_transition(
    _handle: *mut NeomacsDisplay,
) -> c_int {
    0
}

/// Check if buffer transition is ready (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_has_transition_snapshot(
    _handle: *mut NeomacsDisplay,
) -> c_int {
    0
}

// ============================================================================
// Additional effect_setter! invocations (from post-threaded section)
// ============================================================================

effect_setter!(neomacs_display_set_matrix_rain(enabled: c_int, r: c_int, g: c_int, b: c_int, column_count: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.matrix_rain.enabled = enabled != 0;
                    effects.matrix_rain.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.matrix_rain.column_count = column_count as u32;
                    effects.matrix_rain.speed = speed as f32;
                    effects.matrix_rain.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_cursor_elastic_snap(enabled: c_int, overshoot: c_int, duration_ms: c_int) |effects| {
        effects.cursor_elastic_snap.enabled = enabled != 0;
                    effects.cursor_elastic_snap.overshoot = overshoot as f32 / 100.0;
                    effects.cursor_elastic_snap.duration_ms = duration_ms as u32;
});

effect_setter!(neomacs_display_set_frost_border_effect(enabled: c_int, r: c_int, g: c_int, b: c_int, width: c_int, opacity: c_int) |effects| {
        effects.frost_border.enabled = enabled != 0;
                    effects.frost_border.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.frost_border.width = width as f32;
                    effects.frost_border.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_cursor_ghost(enabled: c_int, r: c_int, g: c_int, b: c_int, count: c_int, fade_ms: c_int, drift: c_int, opacity: c_int) |effects| {
        effects.cursor_ghost.enabled = enabled != 0;
                    effects.cursor_ghost.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_ghost.count = count as u32;
                    effects.cursor_ghost.fade_ms = fade_ms as u32;
                    effects.cursor_ghost.drift = drift as f32;
                    effects.cursor_ghost.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_edge_glow(enabled: c_int, r: c_int, g: c_int, b: c_int, height: c_int, opacity: c_int, fade_ms: c_int) |effects| {
        effects.edge_glow.enabled = enabled != 0;
                    effects.edge_glow.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.edge_glow.height = height as f32;
                    effects.edge_glow.opacity = opacity as f32 / 100.0;
                    effects.edge_glow.fade_ms = fade_ms as u32;
});

effect_setter!(neomacs_display_set_rain_effect(enabled: c_int, r: c_int, g: c_int, b: c_int, drop_count: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.rain_effect.enabled = enabled != 0;
                    effects.rain_effect.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.rain_effect.drop_count = drop_count as u32;
                    effects.rain_effect.speed = speed as f32;
                    effects.rain_effect.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_cursor_ripple_wave(enabled: c_int, r: c_int, g: c_int, b: c_int, ring_count: c_int, max_radius: c_int, duration_ms: c_int, opacity: c_int) |effects| {
        effects.cursor_ripple_wave.enabled = enabled != 0;
                    effects.cursor_ripple_wave.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_ripple_wave.ring_count = ring_count as u32;
                    effects.cursor_ripple_wave.max_radius = max_radius as f32;
                    effects.cursor_ripple_wave.duration_ms = duration_ms as u32;
                    effects.cursor_ripple_wave.opacity = opacity as f32 / 100.0;
});

effect_setter!(neomacs_display_set_aurora(enabled: c_int, r1: c_int, g1: c_int, b1: c_int, r2: c_int, g2: c_int, b2: c_int, height: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.aurora.enabled = enabled != 0;
                    effects.aurora.color1 = (r1 as f32 / 255.0, g1 as f32 / 255.0, b1 as f32 / 255.0);
                    effects.aurora.color2 = (r2 as f32 / 255.0, g2 as f32 / 255.0, b2 as f32 / 255.0);
                    effects.aurora.height = height as f32;
                    effects.aurora.speed = speed as f32 / 100.0;
                    effects.aurora.opacity = opacity as f32 / 100.0;
});

// The remaining ~100 effect_setter! invocations with #[cfg(feature = "winit-backend")]
// are included inline. They follow the exact same pattern from the original file.

include!("animation_effects.rs");
