//! Render thread implementation.
//!
//! Owns winit event loop, wgpu, GLib/WebKit. Runs at native VSync.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopBuilder};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

#[cfg(target_os = "linux")]
use winit::platform::x11::EventLoopBuilderExtX11;
#[cfg(target_os = "linux")]
use winit::platform::wayland::EventLoopBuilderExtWayland;

use crate::backend::wgpu::{
    WgpuGlyphAtlas, WgpuRenderer,
    NEOMACS_CTRL_MASK, NEOMACS_META_MASK, NEOMACS_SHIFT_MASK, NEOMACS_SUPER_MASK,
};
use crate::core::face::Face;
use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer};
use crate::core::types::{
    AnimatedCursor, Color, CursorAnimStyle, Rect,
    ease_out_quad, ease_out_cubic, ease_out_expo, ease_in_out_cubic, ease_linear,
};
use crate::thread_comm::{InputEvent, PopupMenuItem, RenderCommand, RenderComms};

#[cfg(all(feature = "wpe-webkit", wpe_platform_available))]
use crate::backend::wpe::sys::platform as plat;

#[cfg(feature = "wpe-webkit")]
use crate::backend::wpe::{WpeBackend, WpeWebView};

// All GPU caches (image, video, webkit) are managed by WgpuRenderer

/// Shared storage for image dimensions accessible from both threads
pub type SharedImageDimensions = Arc<Mutex<HashMap<u32, (u32, u32)>>>;

/// Render thread state
pub struct RenderThread {
    handle: Option<JoinHandle<()>>,
}

impl RenderThread {
    /// Spawn the render thread
    pub fn spawn(
        comms: RenderComms,
        width: u32,
        height: u32,
        title: String,
        image_dimensions: SharedImageDimensions,
        #[cfg(feature = "neo-term")]
        shared_terminals: crate::terminal::SharedTerminals,
    ) -> Self {
        let handle = thread::spawn(move || {
            run_render_loop(
                comms, width, height, title, image_dimensions,
                #[cfg(feature = "neo-term")]
                shared_terminals,
            );
        });

        Self {
            handle: Some(handle),
        }
    }

    /// Wait for render thread to finish
    pub fn join(mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// State for an active crossfade transition
struct CrossfadeTransition {
    started: std::time::Instant,
    duration: std::time::Duration,
    bounds: Rect,
    effect: crate::core::scroll_animation::ScrollEffect,
    easing: crate::core::scroll_animation::ScrollEasing,
    old_texture: wgpu::Texture,
    old_view: wgpu::TextureView,
    old_bind_group: wgpu::BindGroup,
}

/// State for an active scroll slide transition
struct ScrollTransition {
    started: std::time::Instant,
    duration: std::time::Duration,
    bounds: Rect,
    direction: i32, // +1 = scroll down (content up), -1 = scroll up
    effect: crate::core::scroll_animation::ScrollEffect,
    easing: crate::core::scroll_animation::ScrollEasing,
    old_texture: wgpu::Texture,
    old_view: wgpu::TextureView,
    old_bind_group: wgpu::BindGroup,
}

#[cfg(feature = "wpe-webkit")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WebKitImportPolicy {
    /// Prefer raw pixel upload first, fallback to DMA-BUF.
    PixelsFirst,
    /// Prefer DMA-BUF import first, fallback to raw pixels.
    DmaBufFirst,
    /// Default compatibility mode (currently PixelsFirst).
    Auto,
}

#[cfg(feature = "wpe-webkit")]
impl WebKitImportPolicy {
    fn from_env() -> Self {
        match std::env::var("NEOMACS_WEBKIT_IMPORT").ok().as_deref() {
            Some("dmabuf-first") | Some("dmabuf") | Some("dma-buf-first") => {
                log::info!("NEOMACS_WEBKIT_IMPORT=dmabuf-first");
                Self::DmaBufFirst
            }
            Some("pixels-first") | Some("pixels") => {
                log::info!("NEOMACS_WEBKIT_IMPORT=pixels-first");
                Self::PixelsFirst
            }
            Some("auto") => {
                log::info!("NEOMACS_WEBKIT_IMPORT=auto (effective: pixels-first)");
                Self::Auto
            }
            Some(val) => {
                log::warn!(
                    "NEOMACS_WEBKIT_IMPORT={}: unrecognized value, defaulting to auto (effective: pixels-first)",
                    val
                );
                Self::Auto
            }
            None => {
                log::info!("NEOMACS_WEBKIT_IMPORT not set (effective: pixels-first)");
                Self::Auto
            }
        }
    }

    fn effective(self) -> Self {
        match self {
            Self::Auto => Self::PixelsFirst,
            other => other,
        }
    }
}

/// Target position/style for cursor animation
#[derive(Debug, Clone)]
struct CursorTarget {
    window_id: i32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    style: u8,
    color: Color,
}

/// Per-corner spring state for the 4-corner cursor trail animation.
/// Each corner has its own position, velocity, and spring frequency.
#[derive(Debug, Clone, Copy)]
struct CornerSpring {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    target_x: f32,
    target_y: f32,
    omega: f32,
}

/// State for an active popup menu
/// A single menu panel (used for both main menu and submenus)
pub(crate) struct MenuPanel {
    /// Position (logical pixels)
    pub(crate) x: f32,
    pub(crate) y: f32,
    /// Indices into the parent PopupMenuState.all_items for items shown in this panel
    pub(crate) item_indices: Vec<usize>,
    /// Currently hovered index within item_indices (-1 = none)
    pub(crate) hover_index: i32,
    /// Computed layout: (x, y, width, height) in logical pixels
    pub(crate) bounds: (f32, f32, f32, f32),
    /// Per-item Y offsets (relative to bounds.y)
    pub(crate) item_offsets: Vec<f32>,
    /// Item height
    pub(crate) item_height: f32,
}

pub(crate) struct PopupMenuState {
    /// All items (flat, at all depths)
    pub(crate) all_items: Vec<PopupMenuItem>,
    /// Optional title
    pub(crate) title: Option<String>,
    /// The main (root) menu panel
    pub(crate) root_panel: MenuPanel,
    /// Open submenu panels (stack: each level is one deeper)
    pub(crate) submenu_panels: Vec<MenuPanel>,
    /// Face foreground color (sRGB 0.0-1.0), None = default
    pub(crate) face_fg: Option<(f32, f32, f32)>,
    /// Face background color (sRGB 0.0-1.0), None = default
    pub(crate) face_bg: Option<(f32, f32, f32)>,
    /// Font metrics
    font_size: f32,
    line_height: f32,
}

impl PopupMenuState {
    fn layout_panel(
        x: f32, y: f32,
        all_items: &[PopupMenuItem],
        indices: &[usize],
        title: Option<&str>,
        font_size: f32, line_height: f32,
    ) -> MenuPanel {
        let padding = 4.0_f32;
        let item_height = line_height + 3.0;
        let separator_height = 8.0_f32;
        let title_height = if title.is_some() { item_height + separator_height } else { 0.0 };

        let mut total_h = padding + title_height;
        let mut offsets = Vec::with_capacity(indices.len());
        for &idx in indices {
            offsets.push(total_h);
            if all_items[idx].separator {
                total_h += separator_height;
            } else {
                total_h += item_height;
            }
        }
        total_h += padding;

        let char_width = font_size * 0.6;
        let min_width = 150.0_f32;
        let max_label_len = indices.iter()
            .map(|&idx| &all_items[idx])
            .filter(|i| !i.separator)
            .map(|i| {
                let extra = if i.shortcut.is_empty() { 0 } else { i.shortcut.len() + 4 };
                let arrow = if i.submenu { 3 } else { 0 };
                i.label.len() + extra + arrow
            })
            .max()
            .unwrap_or(10);
        let title_len = title.map(|t| t.len()).unwrap_or(0);
        let content_width = (max_label_len.max(title_len) as f32) * char_width;
        let total_w = (content_width + padding * 4.0).max(min_width);

        MenuPanel {
            x,
            y,
            item_indices: indices.to_vec(),
            hover_index: -1,
            bounds: (x, y, total_w, total_h),
            item_offsets: offsets,
            item_height,
        }
    }

    fn new(x: f32, y: f32, items: Vec<PopupMenuItem>, title: Option<String>,
           font_size: f32, line_height: f32) -> Self {
        // Collect top-level item indices (depth == 0)
        let root_indices: Vec<usize> = items.iter().enumerate()
            .filter(|(_, item)| item.depth == 0)
            .map(|(i, _)| i)
            .collect();

        let root_panel = Self::layout_panel(
            x, y, &items, &root_indices,
            title.as_deref(), font_size, line_height,
        );

        PopupMenuState {
            all_items: items,
            title,
            root_panel,
            submenu_panels: Vec::new(),
            face_fg: None,
            face_bg: None,
            font_size,
            line_height,
        }
    }

    /// Get the active panel (deepest open submenu, or root)
    fn active_panel(&self) -> &MenuPanel {
        self.submenu_panels.last().unwrap_or(&self.root_panel)
    }

    fn active_panel_mut(&mut self) -> &mut MenuPanel {
        self.submenu_panels.last_mut().unwrap_or(&mut self.root_panel)
    }

    /// Move hover in the active panel. Returns true if changed.
    fn move_hover(&mut self, direction: i32) -> bool {
        // Read panel state without mutable borrow
        let panel = self.active_panel();
        let len = panel.item_indices.len() as i32;
        if len == 0 {
            return false;
        }
        let current_hover = panel.hover_index;
        let indices: Vec<usize> = panel.item_indices.clone();

        let mut idx = current_hover + direction;
        for _ in 0..len {
            if idx < 0 { idx = len - 1; }
            if idx >= len { idx = 0; }
            let item_idx = indices[idx as usize];
            let item = &self.all_items[item_idx];
            if !item.separator && item.enabled {
                if idx != current_hover {
                    self.active_panel_mut().hover_index = idx;
                    return true;
                }
                return false;
            }
            idx += direction;
        }
        false
    }

    /// Open submenu for the currently hovered item (if it has one)
    fn open_submenu(&mut self) -> bool {
        let panel = self.active_panel();
        if panel.hover_index < 0 {
            return false;
        }
        let hover_idx = panel.hover_index as usize;
        if hover_idx >= panel.item_indices.len() {
            return false;
        }
        let parent_global_idx = panel.item_indices[hover_idx];
        let parent = &self.all_items[parent_global_idx];
        if !parent.submenu {
            return false;
        }
        let parent_depth = parent.depth;
        let child_depth = parent_depth + 1;

        // Collect children: items immediately after parent with depth == child_depth
        // until we see an item with depth <= parent_depth
        let mut child_indices = Vec::new();
        for i in (parent_global_idx + 1)..self.all_items.len() {
            let item = &self.all_items[i];
            if item.depth < child_depth {
                break;
            }
            if item.depth == child_depth {
                child_indices.push(i);
            }
        }

        if child_indices.is_empty() {
            return false;
        }

        // Position submenu to the right of the parent panel
        let (px, py, pw, _ph) = panel.bounds;
        let item_y = py + panel.item_offsets[hover_idx];
        let sub_x = px + pw - 2.0; // Overlap by 2px
        let sub_y = item_y;

        let sub_panel = Self::layout_panel(
            sub_x, sub_y, &self.all_items, &child_indices,
            None, self.font_size, self.line_height,
        );
        self.submenu_panels.push(sub_panel);
        true
    }

    /// Close the deepest open submenu. Returns true if one was closed.
    fn close_submenu(&mut self) -> bool {
        self.submenu_panels.pop().is_some()
    }

    /// Hit test across all panels (deepest first). Returns (panel_depth, item_global_index).
    /// panel_depth: 0 = root, 1+ = submenu level. Returns (-1, -1) for miss.
    fn hit_test_all(&self, mx: f32, my: f32) -> (i32, i32) {
        // Check submenu panels deepest first
        for (level, panel) in self.submenu_panels.iter().enumerate().rev() {
            let result = Self::hit_test_panel(panel, &self.all_items, mx, my);
            if result >= 0 {
                return ((level + 1) as i32, result);
            }
            // Check if inside panel bounds (even if not on an item)
            let (bx, by, bw, bh) = panel.bounds;
            if mx >= bx && mx <= bx + bw && my >= by && my <= by + bh {
                return ((level + 1) as i32, -1);
            }
        }
        // Check root panel
        let result = Self::hit_test_panel(&self.root_panel, &self.all_items, mx, my);
        if result >= 0 {
            return (0, result);
        }
        let (bx, by, bw, bh) = self.root_panel.bounds;
        if mx >= bx && mx <= bx + bw && my >= by && my <= by + bh {
            return (0, -1);
        }
        (-1, -1)
    }

    fn hit_test_panel(panel: &MenuPanel, all_items: &[PopupMenuItem], mx: f32, my: f32) -> i32 {
        let (bx, by, bw, _bh) = panel.bounds;
        if mx < bx || mx > bx + bw || my < by {
            return -1;
        }
        for (i, &offset_y) in panel.item_offsets.iter().enumerate() {
            let item_idx = panel.item_indices[i];
            let item = &all_items[item_idx];
            if item.separator {
                continue;
            }
            let iy = by + offset_y;
            let ih = panel.item_height;
            if my >= iy && my < iy + ih && mx >= bx && mx <= bx + bw {
                return i as i32;
            }
        }
        -1
    }

    /// Convenience: hit_test on the active panel only (for selection)
    fn hit_test(&self, mx: f32, my: f32) -> i32 {
        // Check all panels, return global item index of hit
        let (depth, local_idx) = self.hit_test_all(mx, my);
        if local_idx < 0 || depth < 0 {
            return -1;
        }
        let panel = if depth == 0 {
            &self.root_panel
        } else {
            &self.submenu_panels[(depth - 1) as usize]
        };
        if local_idx >= 0 && (local_idx as usize) < panel.item_indices.len() {
            let global_idx = panel.item_indices[local_idx as usize];
            let item = &self.all_items[global_idx];
            if item.enabled && !item.submenu {
                return global_idx as i32;
            }
        }
        -1
    }

    /// Get the items slice for rendering a panel.
    /// Returns: (items_ref, panel_ref) for iteration.
    pub(crate) fn panels(&self) -> Vec<&MenuPanel> {
        let mut panels = vec![&self.root_panel];
        for sub in &self.submenu_panels {
            panels.push(sub);
        }
        panels
    }
}

/// Application state for winit event loop
struct RenderApp {
    comms: RenderComms,
    window: Option<Arc<Window>>,
    current_frame: Option<FrameGlyphBuffer>,
    width: u32,
    height: u32,
    title: String,

    // wgpu state
    renderer: Option<WgpuRenderer>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    device: Option<Arc<wgpu::Device>>,
    queue: Option<Arc<wgpu::Queue>>,
    glyph_atlas: Option<WgpuGlyphAtlas>,

    // Face cache built from frame data
    faces: HashMap<u32, Face>,

    // Display scale factor (physical pixels / logical pixels)
    scale_factor: f64,

    // Current modifier state (NEOMACS_*_MASK flags)
    modifiers: u32,

    // Last known cursor position
    mouse_pos: (f32, f32),
    /// Whether the mouse cursor is hidden during keyboard input
    mouse_hidden_for_typing: bool,

    // Shared image dimensions (written here, read from main thread)
    image_dimensions: SharedImageDimensions,

    // Frame dirty flag: set when new frame data arrives, cleared after render
    frame_dirty: bool,

    // Cursor blink state (managed by render thread)
    cursor_blink_on: bool,
    cursor_blink_enabled: bool,
    last_cursor_toggle: std::time::Instant,
    cursor_blink_interval: std::time::Duration,

    // Cursor animation (smooth motion)
    cursor_anim_enabled: bool,
    cursor_anim_speed: f32,
    cursor_anim_style: CursorAnimStyle,
    cursor_anim_duration: f32, // seconds, for non-Exponential styles
    cursor_target: Option<CursorTarget>,
    cursor_current_x: f32,
    cursor_current_y: f32,
    cursor_current_w: f32,
    cursor_current_h: f32,
    cursor_animating: bool,
    last_anim_time: std::time::Instant,
    // For easing/linear styles: capture start position when animation begins
    cursor_start_x: f32,
    cursor_start_y: f32,
    cursor_start_w: f32,
    cursor_start_h: f32,
    cursor_anim_start_time: std::time::Instant,
    // For critically-damped spring: velocity per axis (rect-level, non-trail)
    cursor_velocity_x: f32,
    cursor_velocity_y: f32,
    cursor_velocity_w: f32,
    cursor_velocity_h: f32,
    // 4-corner spring trail state (TL, TR, BR, BL)
    cursor_corner_springs: [CornerSpring; 4],
    cursor_trail_size: f32,
    // Previous target center for computing travel direction
    cursor_prev_target_cx: f32,
    cursor_prev_target_cy: f32,

    // Cursor size transition (independent of position animation)
    cursor_size_transition_enabled: bool,
    cursor_size_transition_duration: f32, // seconds
    cursor_size_animating: bool,
    cursor_size_start_w: f32,
    cursor_size_start_h: f32,
    cursor_size_target_w: f32,
    cursor_size_target_h: f32,
    cursor_size_anim_start: std::time::Instant,

    // Per-window metadata from previous frame (for transition detection)
    prev_window_infos: HashMap<i64, crate::core::frame_glyphs::WindowInfo>,

    // Transition state
    crossfade_enabled: bool,
    crossfade_duration: std::time::Duration,
    crossfade_effect: crate::core::scroll_animation::ScrollEffect,
    crossfade_easing: crate::core::scroll_animation::ScrollEasing,
    scroll_enabled: bool,
    scroll_duration: std::time::Duration,
    scroll_effect: crate::core::scroll_animation::ScrollEffect,
    scroll_easing: crate::core::scroll_animation::ScrollEasing,

    // Double-buffer offscreen textures for transitions
    offscreen_a: Option<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
    offscreen_b: Option<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
    current_is_a: bool,

    // Active transitions
    crossfades: HashMap<i64, CrossfadeTransition>,
    scroll_slides: HashMap<i64, ScrollTransition>,

    // WebKit state (video cache is managed by renderer)
    #[cfg(feature = "wpe-webkit")]
    wpe_backend: Option<WpeBackend>,

    #[cfg(feature = "wpe-webkit")]
    webkit_views: HashMap<u32, WpeWebView>,

    #[cfg(feature = "wpe-webkit")]
    webkit_import_policy: WebKitImportPolicy,

    // Floating WebKit overlays (position/size from C side, rendered on render thread)
    #[cfg(feature = "wpe-webkit")]
    floating_webkits: Vec<crate::core::scene::FloatingWebKit>,

    // Terminal manager (neo-term)
    #[cfg(feature = "neo-term")]
    terminal_manager: crate::terminal::TerminalManager,
    #[cfg(feature = "neo-term")]
    shared_terminals: crate::terminal::SharedTerminals,

    // Active popup menu (shown by x-popup-menu)
    popup_menu: Option<PopupMenuState>,

    // Active tooltip overlay
    tooltip: Option<TooltipState>,

    // Visual bell state (flash overlay)
    visual_bell_start: Option<std::time::Instant>,

    // IME state
    ime_enabled: bool,
    ime_preedit_active: bool,
    ime_preedit_text: String,

    // UI overlay state
    scroll_indicators_enabled: bool,

    // Borderless window state
    decorations_enabled: bool,
    /// Resize edge under cursor (None = not on edge)
    resize_edge: Option<winit::window::ResizeDirection>,
    /// Window title for custom title bar
    window_title: String,
    /// Custom title bar height in logical pixels (0 = hidden)
    custom_titlebar_height: f32,
    /// Currently hovered title bar element (0=none, 1=drag, 2=close, 3=max, 4=min)
    titlebar_hover: u32,
    /// Last title bar click time (for double-click detection)
    last_titlebar_click: std::time::Instant,
    /// Whether the window is currently in fullscreen mode
    is_fullscreen: bool,
    /// Corner radius for borderless window rounded corners (0 = no rounding)
    corner_radius: f32,
    /// Whether to show the FPS counter overlay
    show_fps: bool,
    /// Frame time tracking for FPS counter
    fps_last_instant: std::time::Instant,
    fps_frame_count: u32,
    fps_display_value: f32,
    /// Frame time tracking (ms per frame, smoothed)
    fps_frame_time_ms: f32,
    /// Last render start time for measuring frame time
    fps_render_start: std::time::Instant,
    /// Extra line spacing in pixels (added between rows)
    extra_line_spacing: f32,
    /// Extra letter spacing in pixels (added between characters)
    extra_letter_spacing: f32,
    /// Background gradient (top and bottom colors, linear sRGB)
    bg_gradient_enabled: bool,
    bg_gradient_top: (f32, f32, f32),
    bg_gradient_bottom: (f32, f32, f32),

    /// Scroll bar config
    scroll_bar_width: i32,
    scroll_bar_thumb_radius: f32,
    scroll_bar_track_opacity: f32,
    scroll_bar_hover_brightness: f32,

    /// Indent guide config
    indent_guides_enabled: bool,
    indent_guide_color: (f32, f32, f32, f32),
    /// Rainbow indent guide colors (cycling by depth level)
    indent_guide_rainbow_enabled: bool,
    indent_guide_rainbow_colors: Vec<(f32, f32, f32, f32)>,

    /// Current line highlight config
    line_highlight_enabled: bool,
    line_highlight_color: (f32, f32, f32, f32),

    /// Visible whitespace config
    show_whitespace_enabled: bool,
    show_whitespace_color: (f32, f32, f32, f32),

    /// Inactive window dimming
    inactive_dim_enabled: bool,
    inactive_dim_opacity: f32,

    /// Mode-line separator style (0=none, 1=line, 2=shadow, 3=gradient)
    mode_line_separator_style: u32,
    mode_line_separator_color: (f32, f32, f32),
    mode_line_separator_height: f32,

    /// Cursor glow effect
    cursor_glow_enabled: bool,
    cursor_glow_color: (f32, f32, f32),
    cursor_glow_radius: f32,
    cursor_glow_opacity: f32,
    /// Cursor pulse animation (sinusoidal glow modulation)
    cursor_pulse_enabled: bool,
    cursor_pulse_speed: f32,
    cursor_pulse_min_opacity: f32,
    /// Focus mode (dim outside current paragraph)
    focus_mode_enabled: bool,
    focus_mode_opacity: f32,
    /// Minimap (code overview column)
    minimap_enabled: bool,
    minimap_width: f32,
    /// Typing ripple effect
    typing_ripple_enabled: bool,
    typing_ripple_max_radius: f32,
    typing_ripple_duration_ms: u32,
    /// Search highlight pulse
    search_pulse_enabled: bool,
    search_pulse_face_id: u32,
    /// Background pattern (0=none, 1=dots, 2=grid, 3=crosshatch)
    bg_pattern_style: u32,
    bg_pattern_spacing: f32,
    bg_pattern_color: (f32, f32, f32),
    bg_pattern_opacity: f32,
    /// Zen mode (centered distraction-free)
    zen_mode_enabled: bool,
    zen_mode_content_width_pct: f32,
    zen_mode_margin_opacity: f32,
    /// Vignette effect
    vignette_enabled: bool,
    vignette_intensity: f32,
    vignette_radius: f32,
    /// Line insertion/deletion animation
    line_animation_enabled: bool,
    line_animation_duration_ms: u32,
    /// Header/mode-line shadow depth effect
    header_shadow_enabled: bool,
    header_shadow_intensity: f32,
    header_shadow_size: f32,
    /// Cursor color cycling (rainbow hue rotation)
    cursor_color_cycle_enabled: bool,
    cursor_color_cycle_speed: f32,
    cursor_color_cycle_saturation: f32,
    cursor_color_cycle_lightness: f32,
    /// Window switch highlight fade
    window_switch_fade_enabled: bool,
    window_switch_fade_duration_ms: u32,
    window_switch_fade_intensity: f32,
    prev_selected_window_id: i64,
    /// Breadcrumb/path bar overlay
    breadcrumb_enabled: bool,
    breadcrumb_opacity: f32,
    /// Title fade animation for breadcrumbs
    title_fade_enabled: bool,
    title_fade_duration_ms: u32,
    /// Typing speed indicator
    typing_speed_enabled: bool,
    /// Smooth border color transition
    border_transition_enabled: bool,
    border_transition_active_color: (f32, f32, f32),
    border_transition_duration_ms: u32,
    /// Buffer-local accent color strip
    accent_strip_enabled: bool,
    accent_strip_width: f32,
    /// Cursor trail fade
    cursor_trail_fade_enabled: bool,
    cursor_trail_fade_length: u32,
    cursor_trail_fade_ms: u32,
    /// Cursor drop shadow
    cursor_shadow_enabled: bool,
    cursor_shadow_offset_x: f32,
    cursor_shadow_offset_y: f32,
    cursor_shadow_opacity: f32,
    /// Animated focus ring
    focus_ring_enabled: bool,
    focus_ring_color: (f32, f32, f32),
    focus_ring_opacity: f32,
    focus_ring_dash_length: f32,
    focus_ring_speed: f32,
    /// Window background tint based on file type
    window_mode_tint_enabled: bool,
    window_mode_tint_opacity: f32,
    /// Window watermark for empty buffers
    window_watermark_enabled: bool,
    window_watermark_opacity: f32,
    window_watermark_threshold: u32,
    /// Mode-line content transition
    mode_line_transition_enabled: bool,
    mode_line_transition_duration_ms: u32,
    /// Text fade-in animation for new content
    text_fade_in_enabled: bool,
    text_fade_in_duration_ms: u32,
    /// Scroll line spacing animation (accordion effect)
    scroll_line_spacing_enabled: bool,
    scroll_line_spacing_max: f32,
    scroll_line_spacing_duration: std::time::Duration,
    /// Cursor wake animation (pop/scale effect on blink-on)
    cursor_wake_enabled: bool,
    cursor_wake_duration_ms: u32,
    cursor_wake_scale: f32,
    /// Line wrap indicator overlay
    wrap_indicator_enabled: bool,
    wrap_indicator_color: (f32, f32, f32),
    wrap_indicator_opacity: f32,
    /// Per-window scroll momentum indicator
    scroll_momentum_enabled: bool,
    scroll_momentum_fade_ms: u32,
    scroll_momentum_width: f32,
    /// Selection region glow
    region_glow_enabled: bool,
    region_glow_face_id: u32,
    region_glow_radius: f32,
    region_glow_opacity: f32,
    /// Idle screen dimming
    idle_dim_enabled: bool,
    idle_dim_delay: std::time::Duration,
    idle_dim_opacity: f32,
    idle_dim_fade_duration: std::time::Duration,
    last_activity_time: std::time::Instant,
    idle_dim_current_alpha: f32, // current dimming alpha 0.0 (none) to opacity (full)
    idle_dim_active: bool,       // true when dimmed or fading
    /// Noise/film grain overlay
    noise_grain_enabled: bool,
    noise_grain_intensity: f32,
    noise_grain_size: f32,
    /// Window padding gradient
    padding_gradient_enabled: bool,
    padding_gradient_color: (f32, f32, f32),
    padding_gradient_opacity: f32,
    padding_gradient_width: f32,
    /// Frosted glass effect on mode-lines
    frosted_glass_enabled: bool,
    frosted_glass_opacity: f32,
    frosted_glass_blur: f32,
    /// Key press timestamps for WPM calculation
    key_press_times: Vec<std::time::Instant>,
    /// Smoothed WPM value for display
    displayed_wpm: f32,
    /// Active window border glow
    window_glow_enabled: bool,
    window_glow_color: (f32, f32, f32),
    window_glow_radius: f32,
    window_glow_intensity: f32,
    /// Scroll progress indicator bar
    scroll_progress_enabled: bool,
    scroll_progress_height: f32,
    scroll_progress_color: (f32, f32, f32),
    scroll_progress_opacity: f32,
    /// Inactive window color tint
    inactive_tint_enabled: bool,
    inactive_tint_color: (f32, f32, f32),
    inactive_tint_opacity: f32,
}

/// State for a tooltip displayed as GPU overlay
pub(crate) struct TooltipState {
    /// Position (logical pixels, near mouse cursor)
    pub(crate) x: f32,
    pub(crate) y: f32,
    /// Tooltip text (may be multi-line)
    pub(crate) lines: Vec<String>,
    /// Foreground color (sRGB)
    pub(crate) fg: (f32, f32, f32),
    /// Background color (sRGB)
    pub(crate) bg: (f32, f32, f32),
    /// Computed bounds (x, y, w, h)
    pub(crate) bounds: (f32, f32, f32, f32),
}

impl TooltipState {
    fn new(x: f32, y: f32, text: &str, fg: (f32, f32, f32), bg: (f32, f32, f32),
           screen_w: f32, screen_h: f32, font_size: f32, line_height: f32) -> Self {
        let padding = 6.0_f32;
        let char_width = font_size * 0.6;

        let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
        let max_line_len = lines.iter().map(|l| l.len()).max().unwrap_or(1);
        let w = (max_line_len as f32 * char_width + padding * 2.0).max(40.0);
        let h = lines.len() as f32 * line_height + padding * 2.0;

        // Position tooltip below and to the right of cursor, clamping to screen
        let mut tx = x + 10.0;
        let mut ty = y + 20.0;
        if tx + w > screen_w { tx = screen_w - w - 2.0; }
        if ty + h > screen_h { ty = y - h - 5.0; } // flip above cursor
        if tx < 0.0 { tx = 0.0; }
        if ty < 0.0 { ty = 0.0; }

        TooltipState {
            x: tx, y: ty, lines, fg, bg,
            bounds: (tx, ty, w, h),
        }
    }
}

impl RenderApp {
    fn new(
        comms: RenderComms,
        width: u32,
        height: u32,
        title: String,
        image_dimensions: SharedImageDimensions,
        #[cfg(feature = "neo-term")]
        shared_terminals: crate::terminal::SharedTerminals,
    ) -> Self {
        #[cfg(feature = "wpe-webkit")]
        let webkit_import_policy = WebKitImportPolicy::from_env();

        Self {
            comms,
            window: None,
            current_frame: None,
            width,
            height,
            title,
            scale_factor: 1.0,
            renderer: None,
            surface: None,
            surface_config: None,
            device: None,
            queue: None,
            glyph_atlas: None,
            faces: HashMap::new(),
            modifiers: 0,
            mouse_pos: (0.0, 0.0),
            mouse_hidden_for_typing: false,
            image_dimensions,
            frame_dirty: false,
            cursor_blink_on: true,
            cursor_blink_enabled: true,
            last_cursor_toggle: std::time::Instant::now(),
            cursor_blink_interval: std::time::Duration::from_millis(500),
            cursor_anim_enabled: true,
            cursor_anim_speed: 15.0,
            cursor_anim_style: CursorAnimStyle::CriticallyDampedSpring,
            cursor_anim_duration: 0.15,
            cursor_target: None,
            cursor_current_x: 0.0,
            cursor_current_y: 0.0,
            cursor_current_w: 0.0,
            cursor_current_h: 0.0,
            cursor_animating: false,
            last_anim_time: std::time::Instant::now(),
            cursor_start_x: 0.0,
            cursor_start_y: 0.0,
            cursor_start_w: 0.0,
            cursor_start_h: 0.0,
            cursor_anim_start_time: std::time::Instant::now(),
            cursor_velocity_x: 0.0,
            cursor_velocity_y: 0.0,
            cursor_velocity_w: 0.0,
            cursor_velocity_h: 0.0,
            cursor_corner_springs: [CornerSpring {
                x: 0.0, y: 0.0, vx: 0.0, vy: 0.0,
                target_x: 0.0, target_y: 0.0, omega: 26.7,
            }; 4],
            cursor_trail_size: 0.7,
            cursor_prev_target_cx: 0.0,
            cursor_prev_target_cy: 0.0,
            cursor_size_transition_enabled: false,
            cursor_size_transition_duration: 0.15,
            cursor_size_animating: false,
            cursor_size_start_w: 0.0,
            cursor_size_start_h: 0.0,
            cursor_size_target_w: 0.0,
            cursor_size_target_h: 0.0,
            cursor_size_anim_start: std::time::Instant::now(),
            prev_window_infos: HashMap::new(),
            crossfade_enabled: true,
            crossfade_duration: std::time::Duration::from_millis(200),
            crossfade_effect: crate::core::scroll_animation::ScrollEffect::Crossfade,
            crossfade_easing: crate::core::scroll_animation::ScrollEasing::EaseOutQuad,
            scroll_enabled: true,
            scroll_duration: std::time::Duration::from_millis(150),
            scroll_effect: crate::core::scroll_animation::ScrollEffect::default(),
            scroll_easing: crate::core::scroll_animation::ScrollEasing::default(),
            offscreen_a: None,
            offscreen_b: None,
            current_is_a: true,
            crossfades: HashMap::new(),
            scroll_slides: HashMap::new(),
            #[cfg(feature = "wpe-webkit")]
            wpe_backend: None,
            #[cfg(feature = "wpe-webkit")]
            webkit_views: HashMap::new(),
            #[cfg(feature = "wpe-webkit")]
            webkit_import_policy,
            #[cfg(feature = "wpe-webkit")]
            floating_webkits: Vec::new(),
            #[cfg(feature = "neo-term")]
            terminal_manager: crate::terminal::TerminalManager::new(),
            #[cfg(feature = "neo-term")]
            shared_terminals,
            popup_menu: None,
            tooltip: None,
            visual_bell_start: None,
            ime_enabled: false,
            ime_preedit_active: false,
            ime_preedit_text: String::new(),
            scroll_indicators_enabled: true,
            decorations_enabled: true,
            resize_edge: None,
            window_title: String::from("neomacs"),
            custom_titlebar_height: 30.0,
            titlebar_hover: 0,
            last_titlebar_click: std::time::Instant::now(),
            is_fullscreen: false,
            corner_radius: 0.0,
            show_fps: false,
            fps_last_instant: std::time::Instant::now(),
            fps_frame_count: 0,
            fps_display_value: 0.0,
            fps_frame_time_ms: 0.0,
            fps_render_start: std::time::Instant::now(),
            extra_line_spacing: 0.0,
            extra_letter_spacing: 0.0,
            bg_gradient_enabled: false,
            bg_gradient_top: (0.0, 0.0, 0.0),
            bg_gradient_bottom: (0.0, 0.0, 0.0),
            scroll_bar_width: 0,
            scroll_bar_thumb_radius: 0.4,
            scroll_bar_track_opacity: 0.6,
            scroll_bar_hover_brightness: 1.4,
            indent_guides_enabled: false,
            indent_guide_color: (0.3, 0.3, 0.3, 0.3),
            indent_guide_rainbow_enabled: false,
            indent_guide_rainbow_colors: Vec::new(),
            line_highlight_enabled: false,
            line_highlight_color: (0.2, 0.2, 0.3, 0.15),
            show_whitespace_enabled: false,
            show_whitespace_color: (0.4, 0.4, 0.4, 0.3),
            inactive_dim_enabled: false,
            inactive_dim_opacity: 0.15,
            mode_line_separator_style: 0,
            mode_line_separator_color: (0.0, 0.0, 0.0),
            mode_line_separator_height: 3.0,
            cursor_glow_enabled: false,
            cursor_glow_color: (0.4, 0.6, 1.0),
            cursor_glow_radius: 30.0,
            cursor_glow_opacity: 0.15,
            cursor_pulse_enabled: false,
            cursor_pulse_speed: 1.0,
            cursor_pulse_min_opacity: 0.3,
            focus_mode_enabled: false,
            focus_mode_opacity: 0.4,
            minimap_enabled: false,
            minimap_width: 80.0,
            typing_ripple_enabled: false,
            typing_ripple_max_radius: 40.0,
            typing_ripple_duration_ms: 300,
            search_pulse_enabled: false,
            search_pulse_face_id: 0,
            bg_pattern_style: 0,
            bg_pattern_spacing: 20.0,
            bg_pattern_color: (0.5, 0.5, 0.5),
            bg_pattern_opacity: 0.05,
            zen_mode_enabled: false,
            zen_mode_content_width_pct: 60.0,
            zen_mode_margin_opacity: 0.3,
            vignette_enabled: false,
            vignette_intensity: 0.3,
            vignette_radius: 50.0,
            line_animation_enabled: false,
            line_animation_duration_ms: 150,
            header_shadow_enabled: false,
            header_shadow_intensity: 0.3,
            header_shadow_size: 6.0,
            cursor_color_cycle_enabled: false,
            cursor_color_cycle_speed: 0.5,
            cursor_color_cycle_saturation: 0.8,
            cursor_color_cycle_lightness: 0.6,
            window_switch_fade_enabled: false,
            window_switch_fade_duration_ms: 200,
            window_switch_fade_intensity: 0.15,
            prev_selected_window_id: 0,
            breadcrumb_enabled: false,
            breadcrumb_opacity: 0.7,
            title_fade_enabled: false,
            title_fade_duration_ms: 300,
            typing_speed_enabled: false,
            key_press_times: Vec::new(),
            displayed_wpm: 0.0,
            border_transition_enabled: false,
            border_transition_active_color: (0.4, 0.6, 1.0),
            border_transition_duration_ms: 200,
            accent_strip_enabled: false,
            accent_strip_width: 3.0,
            cursor_trail_fade_enabled: false,
            cursor_trail_fade_length: 8,
            cursor_trail_fade_ms: 300,
            cursor_shadow_enabled: false,
            cursor_shadow_offset_x: 2.0,
            cursor_shadow_offset_y: 2.0,
            cursor_shadow_opacity: 0.3,
            focus_ring_enabled: false,
            focus_ring_color: (0.4, 0.6, 1.0),
            focus_ring_opacity: 0.5,
            focus_ring_dash_length: 8.0,
            focus_ring_speed: 40.0,
            window_mode_tint_enabled: false,
            window_mode_tint_opacity: 0.03,
            window_watermark_enabled: false,
            window_watermark_opacity: 0.08,
            window_watermark_threshold: 10,
            mode_line_transition_enabled: false,
            mode_line_transition_duration_ms: 200,
            text_fade_in_enabled: false,
            text_fade_in_duration_ms: 150,
            scroll_line_spacing_enabled: false,
            scroll_line_spacing_max: 6.0,
            scroll_line_spacing_duration: std::time::Duration::from_millis(200),
            cursor_wake_enabled: false,
            cursor_wake_duration_ms: 120,
            cursor_wake_scale: 1.3,
            wrap_indicator_enabled: false,
            wrap_indicator_color: (0.5, 0.6, 0.8),
            wrap_indicator_opacity: 0.3,
            scroll_momentum_enabled: false,
            scroll_momentum_fade_ms: 300,
            scroll_momentum_width: 3.0,
            region_glow_enabled: false,
            region_glow_face_id: 0,
            region_glow_radius: 6.0,
            region_glow_opacity: 0.3,
            idle_dim_enabled: false,
            idle_dim_delay: std::time::Duration::from_secs(60),
            idle_dim_opacity: 0.4,
            idle_dim_fade_duration: std::time::Duration::from_millis(500),
            last_activity_time: std::time::Instant::now(),
            idle_dim_current_alpha: 0.0,
            idle_dim_active: false,
            noise_grain_enabled: false,
            noise_grain_intensity: 0.03,
            noise_grain_size: 2.0,
            padding_gradient_enabled: false,
            padding_gradient_color: (0.0, 0.0, 0.0),
            padding_gradient_opacity: 0.15,
            padding_gradient_width: 8.0,
            frosted_glass_enabled: false,
            frosted_glass_opacity: 0.3,
            frosted_glass_blur: 4.0,
            window_glow_enabled: false,
            window_glow_color: (0.4, 0.6, 1.0),
            window_glow_radius: 8.0,
            window_glow_intensity: 0.4,
            scroll_progress_enabled: false,
            scroll_progress_height: 2.0,
            scroll_progress_color: (0.4, 0.6, 1.0),
            scroll_progress_opacity: 0.8,
            inactive_tint_enabled: false,
            inactive_tint_color: (0.2, 0.1, 0.0),
            inactive_tint_opacity: 0.1,
        }
    }

    /// Initialize wgpu with the window
    fn init_wgpu(&mut self, window: Arc<Window>) {
        log::info!("Initializing wgpu for render thread");

        // Create wgpu instance
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Create surface from window
        let surface = match instance.create_surface(window.clone()) {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to create wgpu surface: {:?}", e);
                return;
            }
        };

        // Request adapter
        let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: crate::gpu_power_preference(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })) {
            Some(a) => a,
            None => {
                log::error!("Failed to find suitable GPU adapter");
                return;
            }
        };

        let adapter_info = adapter.get_info();
        log::info!(
            "wgpu adapter: {} (vendor={:04x}, device={:04x}, backend={:?})",
            adapter_info.name,
            adapter_info.vendor,
            adapter_info.device,
            adapter_info.backend
        );

        // Request device and queue
        let (device, queue) = match pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Neomacs Render Thread Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
            },
            None,
        )) {
            Ok((d, q)) => (d, q),
            Err(e) => {
                log::error!("Failed to create wgpu device: {:?}", e);
                return;
            }
        };

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Configure surface
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        // Prefer PreMultiplied alpha for window transparency support
        let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else {
            caps.alpha_modes[0]
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: self.width,
            height: self.height,
            present_mode: wgpu::PresentMode::Fifo, // VSync
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Create renderer with existing device and surface format
        let renderer = WgpuRenderer::with_device(
            device.clone(), queue.clone(),
            self.width, self.height,
            format,
            self.scale_factor as f32,
        );

        // Create glyph atlas with scale factor for crisp HiDPI text
        let glyph_atlas = WgpuGlyphAtlas::new_with_scale(&device, self.scale_factor as f32);

        log::info!(
            "wgpu initialized: {}x{}, format: {:?}",
            self.width,
            self.height,
            format
        );

        self.surface = Some(surface);
        self.surface_config = Some(config);
        self.device = Some(device.clone());
        self.queue = Some(queue);
        self.renderer = Some(renderer);
        self.glyph_atlas = Some(glyph_atlas);

        // Initialize WPE backend for WebKit
        #[cfg(feature = "wpe-webkit")]
        {
            use crate::backend::wgpu::get_render_node_from_adapter_info;

            // Get DRM render node from adapter to ensure WebKit uses the same GPU
            let render_node = get_render_node_from_adapter_info(&adapter_info)
                .map(|p| p.to_string_lossy().into_owned());

            log::info!("Initializing WPE backend (render_node: {:?})", render_node);

            // SAFETY: We pass null for egl_display_hint as WPE Platform API doesn't use it
            match unsafe { WpeBackend::new_with_device(std::ptr::null_mut(), render_node.as_deref()) } {
                Ok(backend) => {
                    log::info!("WPE backend initialized successfully");
                    self.wpe_backend = Some(backend);
                }
                Err(e) => {
                    log::warn!("Failed to initialize WPE backend: {:?}", e);
                }
            }
        }

        // All GPU caches (image, video, webkit) are managed by the renderer
        #[cfg(feature = "video")]
        log::info!("Video cache initialized");
    }

    /// Handle surface resize
    fn handle_resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.width = width;
        self.height = height;

        // Reconfigure surface
        if let (Some(surface), Some(config), Some(device)) =
            (&self.surface, &mut self.surface_config, &self.device)
        {
            config.width = width;
            config.height = height;
            surface.configure(device, config);
        }

        // Resize renderer
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(width, height);
        }

        // Invalidate offscreen textures (they reference old size)
        self.offscreen_a = None;
        self.offscreen_b = None;
        // Cancel active transitions (they reference old-sized textures)
        self.crossfades.clear();
        self.scroll_slides.clear();

        // Force immediate re-render with old frame at new surface size.
        // Ensures the window always shows content during resize
        // (background fills new area, old glyphs stay at their positions).
        self.frame_dirty = true;

        log::debug!("Surface resized to {}x{}", width, height);
    }


    /// Process pending commands from Emacs
    fn process_commands(&mut self) -> bool {
        let mut should_exit = false;

        while let Ok(cmd) = self.comms.cmd_rx.try_recv() {
            match cmd {
                RenderCommand::Shutdown => {
                    log::info!("Render thread received shutdown command");
                    should_exit = true;
                }
                RenderCommand::ScrollBlit { .. } => {
                    // No-op: scroll blitting is no longer needed with full-frame rendering.
                    // The entire frame is rebuilt from current_matrix each time.
                    log::debug!("ScrollBlit ignored (full-frame rendering mode)");
                }
                RenderCommand::ImageLoadFile { id, path, max_width, max_height } => {
                    log::info!("Loading image {}: {} (max {}x{})", id, path, max_width, max_height);
                    if let Some(ref mut renderer) = self.renderer {
                        renderer.load_image_file_with_id(id, &path, max_width, max_height);
                        // Get dimensions and notify Emacs
                        if let Some((w, h)) = renderer.get_image_size(id) {
                            // Store in shared map for main thread to read
                            if let Ok(mut dims) = self.image_dimensions.lock() {
                                dims.insert(id, (w, h));
                            }
                            // Send event to Emacs so it can trigger redisplay
                            self.comms.send_input(InputEvent::ImageDimensionsReady {
                                id,
                                width: w,
                                height: h,
                            });
                            log::debug!("Sent ImageDimensionsReady for image {}: {}x{}", id, w, h);
                        }
                    } else {
                        log::warn!("Renderer not initialized, cannot load image {}", id);
                    }
                }
                RenderCommand::ImageFree { id } => {
                    log::debug!("Freeing image {}", id);
                    if let Some(ref mut renderer) = self.renderer {
                        renderer.free_image(id);
                    }
                }
                RenderCommand::WebKitCreate { id, width, height } => {
                    log::info!("Creating WebKit view: id={}, {}x{}", id, width, height);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(ref backend) = self.wpe_backend {
                        if let Some(platform_display) = backend.platform_display() {
                            match WpeWebView::new(id, platform_display, width, height) {
                                Ok(view) => {
                                    self.webkit_views.insert(id, view);
                                    log::info!("WebKit view {} created successfully", id);
                                }
                                Err(e) => log::error!("Failed to create WebKit view {}: {:?}", id, e),
                            }
                        } else {
                            log::error!("WPE platform display not available");
                        }
                    } else {
                        log::warn!("WPE backend not initialized, cannot create WebKit view");
                    }
                }
                RenderCommand::WebKitLoadUri { id, url } => {
                    log::info!("Loading URL in WebKit view {}: {}", id, url);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get_mut(&id) {
                        if let Err(e) = view.load_uri(&url) {
                            log::error!("Failed to load URL in view {}: {:?}", id, e);
                        }
                    } else {
                        log::warn!("WebKit view {} not found", id);
                    }
                }
                RenderCommand::WebKitResize { id, width, height } => {
                    log::debug!("Resizing WebKit view {}: {}x{}", id, width, height);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get_mut(&id) {
                        view.resize(width, height);
                    }
                }
                RenderCommand::WebKitDestroy { id } => {
                    log::info!("Destroying WebKit view {}", id);
                    #[cfg(feature = "wpe-webkit")]
                    {
                        self.webkit_views.remove(&id);
                        // Clean up the renderer's webkit cache
                        if let Some(ref mut renderer) = self.renderer {
                            renderer.remove_webkit_view(id);
                        }
                    }
                }
                RenderCommand::WebKitClick { id, x, y, button } => {
                    log::debug!("WebKit click view {} at ({}, {}), button {}", id, x, y, button);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get(&id) {
                        view.click(x, y, button);
                    }
                }
                RenderCommand::WebKitPointerEvent { id, event_type, x, y, button, state, modifiers } => {
                    log::trace!("WebKit pointer event view {} type {} at ({}, {})", id, event_type, x, y);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get(&id) {
                        view.send_pointer_event(event_type, x, y, button, state, modifiers);
                    }
                }
                RenderCommand::WebKitScroll { id, x, y, delta_x, delta_y } => {
                    log::debug!("WebKit scroll view {} at ({}, {}), delta ({}, {})", id, x, y, delta_x, delta_y);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get(&id) {
                        view.scroll(x, y, delta_x, delta_y);
                    }
                }
                RenderCommand::WebKitKeyEvent { id, keyval, keycode, pressed, modifiers } => {
                    log::debug!("WebKit key event view {} keyval {} pressed {}", id, keyval, pressed);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get(&id) {
                        view.send_keyboard_event(keyval, keycode, pressed, modifiers);
                    }
                }
                RenderCommand::WebKitGoBack { id } => {
                    log::info!("WebKit go back view {}", id);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get_mut(&id) {
                        let _ = view.go_back();
                    }
                }
                RenderCommand::WebKitGoForward { id } => {
                    log::info!("WebKit go forward view {}", id);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get_mut(&id) {
                        let _ = view.go_forward();
                    }
                }
                RenderCommand::WebKitReload { id } => {
                    log::info!("WebKit reload view {}", id);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get_mut(&id) {
                        let _ = view.reload();
                    }
                }
                RenderCommand::WebKitExecuteJavaScript { id, script } => {
                    log::debug!("WebKit execute JS view {}", id);
                    #[cfg(feature = "wpe-webkit")]
                    if let Some(view) = self.webkit_views.get(&id) {
                        let _ = view.execute_javascript(&script);
                    }
                }
                RenderCommand::WebKitSetFloating { id, x, y, width, height } => {
                    log::info!("WebKit set floating: id={} at ({},{}) {}x{}", id, x, y, width, height);
                    #[cfg(feature = "wpe-webkit")]
                    {
                        self.floating_webkits.retain(|w| w.webkit_id != id);
                        self.floating_webkits.push(crate::core::scene::FloatingWebKit {
                            webkit_id: id, x, y, width, height,
                        });
                        self.frame_dirty = true;
                    }
                }
                RenderCommand::WebKitRemoveFloating { id } => {
                    log::info!("WebKit remove floating: id={}", id);
                    #[cfg(feature = "wpe-webkit")]
                    {
                        self.floating_webkits.retain(|w| w.webkit_id != id);
                        self.frame_dirty = true;
                    }
                }
                RenderCommand::VideoCreate { id, path } => {
                    log::info!("Loading video {}: {}", id, path);
                    #[cfg(feature = "video")]
                    if let Some(ref mut renderer) = self.renderer {
                        let video_id = renderer.load_video_file(&path);
                        log::info!("Video loaded with id {} (requested id was {})", video_id, id);
                    }
                }
                RenderCommand::VideoPlay { id } => {
                    log::debug!("Playing video {}", id);
                    #[cfg(feature = "video")]
                    if let Some(ref mut renderer) = self.renderer {
                        renderer.video_play(id);
                    }
                }
                RenderCommand::VideoPause { id } => {
                    log::debug!("Pausing video {}", id);
                    #[cfg(feature = "video")]
                    if let Some(ref mut renderer) = self.renderer {
                        renderer.video_pause(id);
                    }
                }
                RenderCommand::VideoDestroy { id } => {
                    log::info!("Destroying video {}", id);
                    #[cfg(feature = "video")]
                    if let Some(ref mut renderer) = self.renderer {
                        renderer.video_stop(id);
                    }
                }
                RenderCommand::SetMouseCursor { cursor_type } => {
                    if let Some(ref window) = self.window {
                        if cursor_type == 0 {
                            // Hidden/invisible cursor
                            window.set_cursor_visible(false);
                        } else {
                            use winit::window::CursorIcon;
                            window.set_cursor_visible(true);
                            let icon = match cursor_type {
                                2 => CursorIcon::Text,       // I-beam
                                3 => CursorIcon::Pointer,    // Hand/pointer
                                4 => CursorIcon::Crosshair,
                                5 => CursorIcon::EwResize,   // Horizontal resize
                                6 => CursorIcon::NsResize,   // Vertical resize
                                7 => CursorIcon::Wait,       // Hourglass
                                8 => CursorIcon::NwseResize, // NW-SE (top-left/bottom-right)
                                9 => CursorIcon::NeswResize, // NE-SW (top-right/bottom-left)
                                10 => CursorIcon::NeswResize,
                                11 => CursorIcon::NwseResize,
                                _ => CursorIcon::Default,    // Arrow
                            };
                            window.set_cursor(icon);
                        }
                    }
                }
                RenderCommand::WarpMouse { x, y } => {
                    if let Some(ref window) = self.window {
                        use winit::dpi::PhysicalPosition;
                        let pos = PhysicalPosition::new(x as f64, y as f64);
                        let _ = window.set_cursor_position(pos);
                    }
                }
                RenderCommand::SetWindowTitle { title } => {
                    self.window_title = title.clone();
                    if let Some(ref window) = self.window {
                        window.set_title(&title);
                    }
                    if !self.decorations_enabled {
                        self.frame_dirty = true;
                    }
                }
                RenderCommand::SetWindowFullscreen { mode } => {
                    if let Some(ref window) = self.window {
                        use winit::window::Fullscreen;
                        match mode {
                            3 => {
                                // FULLSCREEN_BOTH: borderless fullscreen
                                window.set_fullscreen(Some(Fullscreen::Borderless(None)));
                                self.is_fullscreen = true;
                            }
                            4 => {
                                // FULLSCREEN_MAXIMIZED
                                window.set_maximized(true);
                                self.is_fullscreen = false;
                            }
                            _ => {
                                // FULLSCREEN_NONE or partial: exit fullscreen
                                window.set_fullscreen(None);
                                window.set_maximized(false);
                                self.is_fullscreen = false;
                            }
                        }
                        self.frame_dirty = true;
                    }
                }
                RenderCommand::SetWindowMinimized { minimized } => {
                    if let Some(ref window) = self.window {
                        window.set_minimized(minimized);
                    }
                }
                RenderCommand::SetWindowPosition { x, y } => {
                    if let Some(ref window) = self.window {
                        window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                    }
                }
                RenderCommand::SetWindowSize { width, height } => {
                    if let Some(ref window) = self.window {
                        let size = winit::dpi::PhysicalSize::new(width, height);
                        let _ = window.request_inner_size(size);
                    }
                }
                RenderCommand::SetWindowDecorated { decorated } => {
                    self.decorations_enabled = decorated;
                    if let Some(ref window) = self.window {
                        window.set_decorations(decorated);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetCursorBlink { enabled, interval_ms } => {
                    log::debug!("Cursor blink: enabled={}, interval={}ms", enabled, interval_ms);
                    self.cursor_blink_enabled = enabled;
                    self.cursor_blink_interval = std::time::Duration::from_millis(interval_ms as u64);
                    if !enabled {
                        self.cursor_blink_on = true;
                        self.frame_dirty = true;
                    }
                }
                RenderCommand::SetCursorAnimation { enabled, speed } => {
                    log::debug!("Cursor animation: enabled={}, speed={}", enabled, speed);
                    self.cursor_anim_enabled = enabled;
                    self.cursor_anim_speed = speed;
                    if !enabled {
                        self.cursor_animating = false;
                    }
                }
                RenderCommand::SetAnimationConfig {
                    cursor_enabled, cursor_speed,
                    cursor_style, cursor_duration_ms,
                    crossfade_enabled, crossfade_duration_ms,
                    scroll_enabled, scroll_duration_ms,
                    scroll_effect, scroll_easing,
                    trail_size,
                    crossfade_effect, crossfade_easing,
                } => {
                    use crate::core::scroll_animation::{ScrollEffect, ScrollEasing};
                    let effect = ScrollEffect::ALL.get(scroll_effect as usize)
                        .copied().unwrap_or(ScrollEffect::Slide);
                    let easing = match scroll_easing {
                        0 => ScrollEasing::EaseOutQuad,
                        1 => ScrollEasing::EaseOutCubic,
                        2 => ScrollEasing::Spring,
                        3 => ScrollEasing::Linear,
                        4 => ScrollEasing::EaseInOutCubic,
                        _ => ScrollEasing::EaseOutQuad,
                    };
                    let cf_effect = ScrollEffect::ALL.get(crossfade_effect as usize)
                        .copied().unwrap_or(ScrollEffect::Crossfade);
                    let cf_easing = match crossfade_easing {
                        0 => ScrollEasing::EaseOutQuad,
                        1 => ScrollEasing::EaseOutCubic,
                        2 => ScrollEasing::Spring,
                        3 => ScrollEasing::Linear,
                        4 => ScrollEasing::EaseInOutCubic,
                        _ => ScrollEasing::EaseOutQuad,
                    };
                    log::debug!("Animation config: cursor={}/{}/style={:?}/{}ms/trail={}, crossfade={}/{}ms/effect={:?}/easing={:?}, scroll={}/{}ms/effect={:?}/easing={:?}",
                        cursor_enabled, cursor_speed, cursor_style, cursor_duration_ms, trail_size,
                        crossfade_enabled, crossfade_duration_ms, cf_effect, cf_easing,
                        scroll_enabled, scroll_duration_ms, effect, easing);
                    self.cursor_anim_enabled = cursor_enabled;
                    self.cursor_anim_speed = cursor_speed;
                    self.cursor_anim_style = cursor_style;
                    self.cursor_anim_duration = cursor_duration_ms as f32 / 1000.0;
                    self.cursor_trail_size = trail_size.clamp(0.0, 1.0);
                    self.crossfade_enabled = crossfade_enabled;
                    self.crossfade_duration = std::time::Duration::from_millis(crossfade_duration_ms as u64);
                    self.crossfade_effect = cf_effect;
                    self.crossfade_easing = cf_easing;
                    self.scroll_enabled = scroll_enabled;
                    self.scroll_duration = std::time::Duration::from_millis(scroll_duration_ms as u64);
                    self.scroll_effect = effect;
                    self.scroll_easing = easing;
                    if !cursor_enabled {
                        self.cursor_animating = false;
                    }
                    if !crossfade_enabled {
                        self.crossfades.clear();
                    }
                    if !scroll_enabled {
                        self.scroll_slides.clear();
                    }
                }
                #[cfg(feature = "neo-term")]
                RenderCommand::TerminalCreate { id, cols, rows, mode, shell } => {
                    let term_mode = match mode {
                        1 => crate::terminal::TerminalMode::Inline,
                        2 => crate::terminal::TerminalMode::Floating,
                        _ => crate::terminal::TerminalMode::Window,
                    };
                    match crate::terminal::TerminalView::new(
                        id, cols, rows, term_mode, shell.as_deref(),
                    ) {
                        Ok(view) => {
                            // Register term Arc in shared map for cross-thread access
                            if let Ok(mut shared) = self.shared_terminals.lock() {
                                shared.insert(id, view.term.clone());
                            }
                            self.terminal_manager.terminals.insert(id, view);
                            log::info!("Terminal {} created ({}x{}, {:?})", id, cols, rows, term_mode);
                        }
                        Err(e) => {
                            log::error!("Failed to create terminal {}: {}", id, e);
                        }
                    }
                }
                #[cfg(feature = "neo-term")]
                RenderCommand::TerminalWrite { id, data } => {
                    if let Some(view) = self.terminal_manager.get_mut(id) {
                        if let Err(e) = view.write(&data) {
                            log::warn!("Terminal {} write error: {}", id, e);
                        }
                    }
                }
                #[cfg(feature = "neo-term")]
                RenderCommand::TerminalResize { id, cols, rows } => {
                    if let Some(view) = self.terminal_manager.get_mut(id) {
                        view.resize(cols, rows);
                    }
                }
                #[cfg(feature = "neo-term")]
                RenderCommand::TerminalDestroy { id } => {
                    if let Ok(mut shared) = self.shared_terminals.lock() {
                        shared.remove(&id);
                    }
                    self.terminal_manager.destroy(id);
                    log::info!("Terminal {} destroyed", id);
                }
                #[cfg(feature = "neo-term")]
                RenderCommand::TerminalSetFloat { id, x, y, opacity } => {
                    if let Some(view) = self.terminal_manager.get_mut(id) {
                        view.float_x = x;
                        view.float_y = y;
                        view.float_opacity = opacity;
                    }
                }
                RenderCommand::ShowPopupMenu { x, y, items, title, fg, bg } => {
                    log::info!("ShowPopupMenu at ({}, {}) with {} items", x, y, items.len());
                    let (fs, lh) = self.glyph_atlas.as_ref()
                        .map(|a| (a.default_font_size(), a.default_line_height()))
                        .unwrap_or((13.0, 17.0));
                    let mut menu = PopupMenuState::new(x, y, items, title, fs, lh);
                    menu.face_fg = fg;
                    menu.face_bg = bg;
                    self.popup_menu = Some(menu);
                    self.frame_dirty = true;
                }
                RenderCommand::HidePopupMenu => {
                    log::info!("HidePopupMenu");
                    self.popup_menu = None;
                    self.frame_dirty = true;
                }
                RenderCommand::ShowTooltip { x, y, text, fg_r, fg_g, fg_b, bg_r, bg_g, bg_b } => {
                    log::debug!("ShowTooltip at ({}, {})", x, y);
                    let (fs, lh) = self.glyph_atlas.as_ref()
                        .map(|a| (a.default_font_size(), a.default_line_height()))
                        .unwrap_or((13.0, 17.0));
                    self.tooltip = Some(TooltipState::new(
                        x, y, &text,
                        (fg_r, fg_g, fg_b),
                        (bg_r, bg_g, bg_b),
                        self.width as f32 / self.scale_factor as f32,
                        self.height as f32 / self.scale_factor as f32,
                        fs, lh,
                    ));
                    self.frame_dirty = true;
                }
                RenderCommand::HideTooltip => {
                    log::debug!("HideTooltip");
                    self.tooltip = None;
                    self.frame_dirty = true;
                }
                RenderCommand::VisualBell => {
                    self.visual_bell_start = Some(std::time::Instant::now());
                    self.frame_dirty = true;
                }
                RenderCommand::RequestAttention { urgent } => {
                    if let Some(ref window) = self.window {
                        let attention = if urgent {
                            Some(winit::window::UserAttentionType::Critical)
                        } else {
                            Some(winit::window::UserAttentionType::Informational)
                        };
                        window.request_user_attention(attention);
                    }
                }
                RenderCommand::SetScrollIndicators { enabled } => {
                    self.scroll_indicators_enabled = enabled;
                    self.frame_dirty = true;
                }
                RenderCommand::SetTitlebarHeight { height } => {
                    self.custom_titlebar_height = height;
                    self.frame_dirty = true;
                }
                RenderCommand::SetShowFps { enabled } => {
                    self.show_fps = enabled;
                    self.frame_dirty = true;
                }
                RenderCommand::SetCornerRadius { radius } => {
                    self.corner_radius = radius;
                    self.frame_dirty = true;
                }
                RenderCommand::SetExtraSpacing { line_spacing, letter_spacing } => {
                    self.extra_line_spacing = line_spacing;
                    self.extra_letter_spacing = letter_spacing;
                    self.frame_dirty = true;
                }
                RenderCommand::SetBackgroundGradient {
                    enabled, top_r, top_g, top_b, bottom_r, bottom_g, bottom_b,
                } => {
                    self.bg_gradient_enabled = enabled;
                    self.bg_gradient_top = (top_r, top_g, top_b);
                    self.bg_gradient_bottom = (bottom_r, bottom_g, bottom_b);
                    self.frame_dirty = true;
                }
                RenderCommand::SetScrollBarConfig {
                    width, thumb_radius, track_opacity, hover_brightness,
                } => {
                    self.scroll_bar_width = width;
                    self.scroll_bar_thumb_radius = thumb_radius;
                    self.scroll_bar_track_opacity = track_opacity;
                    self.scroll_bar_hover_brightness = hover_brightness;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_scroll_bar_config(
                            thumb_radius, track_opacity, hover_brightness,
                        );
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetIndentGuideConfig {
                    enabled, r, g, b, opacity,
                } => {
                    // Convert sRGB to linear for GPU rendering
                    let c = crate::core::types::Color::new(r, g, b, opacity).srgb_to_linear();
                    self.indent_guides_enabled = enabled;
                    self.indent_guide_color = (c.r, c.g, c.b, c.a);
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_indent_guide_config(enabled, (c.r, c.g, c.b, c.a));
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetIndentGuideRainbow {
                    enabled, colors,
                } => {
                    // Convert sRGB colors to linear for GPU rendering
                    let linear_colors: Vec<(f32, f32, f32, f32)> = colors.iter().map(|(r, g, b, a)| {
                        let c = crate::core::types::Color::new(*r, *g, *b, *a).srgb_to_linear();
                        (c.r, c.g, c.b, c.a)
                    }).collect();
                    self.indent_guide_rainbow_enabled = enabled;
                    self.indent_guide_rainbow_colors = linear_colors.clone();
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_indent_guide_rainbow(enabled, linear_colors);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetLineHighlight {
                    enabled, r, g, b, opacity,
                } => {
                    let c = crate::core::types::Color::new(r, g, b, opacity).srgb_to_linear();
                    self.line_highlight_enabled = enabled;
                    self.line_highlight_color = (c.r, c.g, c.b, c.a);
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_line_highlight_config(enabled, (c.r, c.g, c.b, c.a));
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetShowWhitespace {
                    enabled, r, g, b, opacity,
                } => {
                    let c = crate::core::types::Color::new(r, g, b, opacity).srgb_to_linear();
                    self.show_whitespace_enabled = enabled;
                    self.show_whitespace_color = (c.r, c.g, c.b, c.a);
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_show_whitespace_config(enabled, (c.r, c.g, c.b, c.a));
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetInactiveDim { enabled, opacity } => {
                    self.inactive_dim_enabled = enabled;
                    self.inactive_dim_opacity = opacity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_inactive_dim_config(enabled, opacity);
                    }
                    self.frame_dirty = true;
                }

                RenderCommand::SetModeLineSeparator { style, r, g, b, height } => {
                    self.mode_line_separator_style = style;
                    self.mode_line_separator_color = (r, g, b);
                    self.mode_line_separator_height = height;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_mode_line_separator(style, (r, g, b), height);
                    }
                    self.frame_dirty = true;
                }

                RenderCommand::SetCursorGlow { enabled, r, g, b, radius, opacity } => {
                    self.cursor_glow_enabled = enabled;
                    self.cursor_glow_color = (r, g, b);
                    self.cursor_glow_radius = radius;
                    self.cursor_glow_opacity = opacity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_cursor_glow(enabled, (r, g, b), radius, opacity);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetCursorPulse { enabled, speed, min_opacity } => {
                    self.cursor_pulse_enabled = enabled;
                    self.cursor_pulse_speed = speed;
                    self.cursor_pulse_min_opacity = min_opacity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_cursor_pulse(enabled, speed, min_opacity);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetFocusMode { enabled, opacity } => {
                    self.focus_mode_enabled = enabled;
                    self.focus_mode_opacity = opacity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_focus_mode(enabled, opacity);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetMinimap { enabled, width } => {
                    self.minimap_enabled = enabled;
                    self.minimap_width = width;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_minimap(enabled, width);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetTypingRipple { enabled, max_radius, duration_ms } => {
                    self.typing_ripple_enabled = enabled;
                    self.typing_ripple_max_radius = max_radius;
                    self.typing_ripple_duration_ms = duration_ms;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_typing_ripple(enabled, max_radius, duration_ms);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetSearchPulse { enabled, face_id } => {
                    self.search_pulse_enabled = enabled;
                    self.search_pulse_face_id = face_id;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_search_pulse(enabled, face_id);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetBackgroundPattern { style, spacing, r, g, b, opacity } => {
                    self.bg_pattern_style = style;
                    self.bg_pattern_spacing = spacing;
                    self.bg_pattern_color = (r, g, b);
                    self.bg_pattern_opacity = opacity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_background_pattern(style, spacing, r, g, b, opacity);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetZenMode { enabled, content_width_pct, margin_opacity } => {
                    self.zen_mode_enabled = enabled;
                    self.zen_mode_content_width_pct = content_width_pct;
                    self.zen_mode_margin_opacity = margin_opacity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_zen_mode(enabled, content_width_pct, margin_opacity);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetVignette { enabled, intensity, radius } => {
                    self.vignette_enabled = enabled;
                    self.vignette_intensity = intensity;
                    self.vignette_radius = radius;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_vignette(enabled, intensity, radius);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetLineAnimation { enabled, duration_ms } => {
                    self.line_animation_enabled = enabled;
                    self.line_animation_duration_ms = duration_ms;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_line_animation(enabled, duration_ms);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetHeaderShadow { enabled, intensity, size } => {
                    self.header_shadow_enabled = enabled;
                    self.header_shadow_intensity = intensity;
                    self.header_shadow_size = size;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_header_shadow(enabled, intensity, size);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetCursorColorCycle { enabled, speed, saturation, lightness } => {
                    self.cursor_color_cycle_enabled = enabled;
                    self.cursor_color_cycle_speed = speed;
                    self.cursor_color_cycle_saturation = saturation;
                    self.cursor_color_cycle_lightness = lightness;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_cursor_color_cycle(enabled, speed, saturation, lightness);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetWindowSwitchFade { enabled, duration_ms, intensity } => {
                    self.window_switch_fade_enabled = enabled;
                    self.window_switch_fade_duration_ms = duration_ms;
                    self.window_switch_fade_intensity = intensity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_window_switch_fade(enabled, duration_ms, intensity);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetInactiveTint { enabled, color, opacity } => {
                    self.inactive_tint_enabled = enabled;
                    self.inactive_tint_color = color;
                    self.inactive_tint_opacity = opacity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_inactive_tint(enabled, color, opacity);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetScrollProgress { enabled, height, color, opacity } => {
                    self.scroll_progress_enabled = enabled;
                    self.scroll_progress_height = height;
                    self.scroll_progress_color = color;
                    self.scroll_progress_opacity = opacity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_scroll_progress(enabled, height, color, opacity);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetWindowGlow { enabled, color, radius, intensity } => {
                    self.window_glow_enabled = enabled;
                    self.window_glow_color = color;
                    self.window_glow_radius = radius;
                    self.window_glow_intensity = intensity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_window_glow(enabled, color, radius, intensity);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetBreadcrumb { enabled, opacity } => {
                    self.breadcrumb_enabled = enabled;
                    self.breadcrumb_opacity = opacity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_breadcrumb(enabled, opacity);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetTitleFade { enabled, duration_ms } => {
                    self.title_fade_enabled = enabled;
                    self.title_fade_duration_ms = duration_ms;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_title_fade(enabled, duration_ms);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetTypingSpeed { enabled } => {
                    self.typing_speed_enabled = enabled;
                    if !enabled {
                        self.key_press_times.clear();
                        self.displayed_wpm = 0.0;
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetBorderTransition { enabled, active_color, duration_ms } => {
                    self.border_transition_enabled = enabled;
                    self.border_transition_active_color = active_color;
                    self.border_transition_duration_ms = duration_ms;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_border_transition(enabled, active_color, duration_ms);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetAccentStrip { enabled, width } => {
                    self.accent_strip_enabled = enabled;
                    self.accent_strip_width = width;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_accent_strip(enabled, width);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetCursorTrailFade { enabled, length, fade_ms } => {
                    self.cursor_trail_fade_enabled = enabled;
                    self.cursor_trail_fade_length = length;
                    self.cursor_trail_fade_ms = fade_ms;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_cursor_trail_fade(enabled, length as usize, fade_ms);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetCursorShadow { enabled, offset_x, offset_y, opacity } => {
                    self.cursor_shadow_enabled = enabled;
                    self.cursor_shadow_offset_x = offset_x;
                    self.cursor_shadow_offset_y = offset_y;
                    self.cursor_shadow_opacity = opacity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_cursor_shadow(enabled, offset_x, offset_y, opacity);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetFocusRing { enabled, color, opacity, dash_length, speed } => {
                    self.focus_ring_enabled = enabled;
                    self.focus_ring_color = color;
                    self.focus_ring_opacity = opacity;
                    self.focus_ring_dash_length = dash_length;
                    self.focus_ring_speed = speed;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_focus_ring(enabled, color, opacity, dash_length, speed);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetWindowModeTint { enabled, opacity } => {
                    self.window_mode_tint_enabled = enabled;
                    self.window_mode_tint_opacity = opacity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_window_mode_tint(enabled, opacity);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetWindowWatermark { enabled, opacity, threshold } => {
                    self.window_watermark_enabled = enabled;
                    self.window_watermark_opacity = opacity;
                    self.window_watermark_threshold = threshold;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_window_watermark(enabled, opacity, threshold);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetModeLineTransition { enabled, duration_ms } => {
                    self.mode_line_transition_enabled = enabled;
                    self.mode_line_transition_duration_ms = duration_ms;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_mode_line_transition(enabled, duration_ms);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetTextFadeIn { enabled, duration_ms } => {
                    self.text_fade_in_enabled = enabled;
                    self.text_fade_in_duration_ms = duration_ms;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_text_fade_in(enabled, duration_ms);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetScrollLineSpacing { enabled, max_spacing, duration_ms } => {
                    self.scroll_line_spacing_enabled = enabled;
                    self.scroll_line_spacing_max = max_spacing;
                    self.scroll_line_spacing_duration = std::time::Duration::from_millis(duration_ms as u64);
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_scroll_line_spacing(enabled, max_spacing, duration_ms);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetCursorWake { enabled, duration_ms, scale } => {
                    self.cursor_wake_enabled = enabled;
                    self.cursor_wake_duration_ms = duration_ms;
                    self.cursor_wake_scale = scale;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_cursor_wake(enabled, duration_ms, scale);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetWrapIndicator { enabled, r, g, b, opacity } => {
                    self.wrap_indicator_enabled = enabled;
                    self.wrap_indicator_color = (r, g, b);
                    self.wrap_indicator_opacity = opacity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_wrap_indicator(enabled, r, g, b, opacity);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetScrollMomentum { enabled, fade_ms, width } => {
                    self.scroll_momentum_enabled = enabled;
                    self.scroll_momentum_fade_ms = fade_ms;
                    self.scroll_momentum_width = width;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_scroll_momentum(enabled, fade_ms, width);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetRegionGlow { enabled, face_id, radius, opacity } => {
                    self.region_glow_enabled = enabled;
                    self.region_glow_face_id = face_id;
                    self.region_glow_radius = radius;
                    self.region_glow_opacity = opacity;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_region_glow(enabled, face_id, radius, opacity);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetIdleDim { enabled, delay_secs, opacity, fade_ms } => {
                    self.idle_dim_enabled = enabled;
                    self.idle_dim_delay = std::time::Duration::from_secs_f32(delay_secs);
                    self.idle_dim_opacity = opacity;
                    self.idle_dim_fade_duration = std::time::Duration::from_millis(fade_ms as u64);
                    if !enabled {
                        self.idle_dim_current_alpha = 0.0;
                        self.idle_dim_active = false;
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetNoiseGrain { enabled, intensity, size } => {
                    self.noise_grain_enabled = enabled;
                    self.noise_grain_intensity = intensity;
                    self.noise_grain_size = size;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_noise_grain(enabled, intensity, size);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetPaddingGradient { enabled, color, opacity, width } => {
                    self.padding_gradient_enabled = enabled;
                    self.padding_gradient_color = color;
                    self.padding_gradient_opacity = opacity;
                    self.padding_gradient_width = width;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_padding_gradient(enabled, color, opacity, width);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetFrostedGlass { enabled, opacity, blur } => {
                    self.frosted_glass_enabled = enabled;
                    self.frosted_glass_opacity = opacity;
                    self.frosted_glass_blur = blur;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_frosted_glass(enabled, opacity, blur);
                    }
                    self.frame_dirty = true;
                }
                RenderCommand::SetCursorSizeTransition { enabled, duration_ms } => {
                    self.cursor_size_transition_enabled = enabled;
                    self.cursor_size_transition_duration = duration_ms as f32 / 1000.0;
                    if !enabled {
                        self.cursor_size_animating = false;
                    }
                    self.frame_dirty = true;
                }
            }
        }

        should_exit
    }

    /// Get latest frame from Emacs (non-blocking)
    fn poll_frame(&mut self) {
        // Get the newest frame, discarding older ones
        while let Ok(frame) = self.comms.frame_rx.try_recv() {
            self.current_frame = Some(frame);
            self.frame_dirty = true;
            // Reset blink to visible when new frame arrives (cursor just moved/redrawn)
            self.cursor_blink_on = true;
            self.last_cursor_toggle = std::time::Instant::now();
        }

        // Extract active cursor target for animation
        if let Some(ref frame) = self.current_frame {
            let active_cursor = frame.glyphs.iter().find_map(|g| match g {
                FrameGlyph::Cursor { window_id, x, y, width, height, style, color }
                    if *style != 3 => Some(CursorTarget {
                        window_id: *window_id,
                        x: *x, y: *y,
                        width: *width, height: *height,
                        style: *style,
                        color: *color,
                    }),
                _ => None,
            });

            if let Some(new_target) = active_cursor {
                let had_target = self.cursor_target.is_some();
                let target_moved = self.cursor_target.as_ref().map_or(true, |old| {
                    (old.x - new_target.x).abs() > 0.5
                    || (old.y - new_target.y).abs() > 0.5
                    || (old.width - new_target.width).abs() > 0.5
                    || (old.height - new_target.height).abs() > 0.5
                });

                if !had_target || !self.cursor_anim_enabled {
                    // First appearance or animation disabled: snap
                    self.cursor_current_x = new_target.x;
                    self.cursor_current_y = new_target.y;
                    self.cursor_current_w = new_target.width;
                    self.cursor_current_h = new_target.height;
                    self.cursor_animating = false;
                    // Snap corner springs to target corners
                    let corners = Self::cursor_target_corners(&new_target);
                    for i in 0..4 {
                        self.cursor_corner_springs[i].x = corners[i].0;
                        self.cursor_corner_springs[i].y = corners[i].1;
                        self.cursor_corner_springs[i].vx = 0.0;
                        self.cursor_corner_springs[i].vy = 0.0;
                        self.cursor_corner_springs[i].target_x = corners[i].0;
                        self.cursor_corner_springs[i].target_y = corners[i].1;
                    }
                    self.cursor_prev_target_cx = new_target.x + new_target.width / 2.0;
                    self.cursor_prev_target_cy = new_target.y + new_target.height / 2.0;
                } else if target_moved {
                    let now = std::time::Instant::now();
                    self.cursor_animating = true;
                    self.last_anim_time = now;
                    // Capture start position for easing/linear/spring styles
                    self.cursor_start_x = self.cursor_current_x;
                    self.cursor_start_y = self.cursor_current_y;
                    self.cursor_start_w = self.cursor_current_w;
                    self.cursor_start_h = self.cursor_current_h;
                    self.cursor_anim_start_time = now;
                    // For spring: reset velocities
                    self.cursor_velocity_x = 0.0;
                    self.cursor_velocity_y = 0.0;
                    self.cursor_velocity_w = 0.0;
                    self.cursor_velocity_h = 0.0;

                    // Set up 4-corner springs for trail effect (spring style only)
                    if self.cursor_anim_style == CursorAnimStyle::CriticallyDampedSpring {
                        let new_corners = Self::cursor_target_corners(&new_target);
                        let new_cx = new_target.x + new_target.width / 2.0;
                        let new_cy = new_target.y + new_target.height / 2.0;
                        let old_cx = self.cursor_prev_target_cx;
                        let old_cy = self.cursor_prev_target_cy;

                        // Travel direction (normalized)
                        let dx = new_cx - old_cx;
                        let dy = new_cy - old_cy;
                        let len = (dx * dx + dy * dy).sqrt();
                        let (dir_x, dir_y) = if len > 0.001 {
                            (dx / len, dy / len)
                        } else {
                            (1.0, 0.0)
                        };

                        // Corner direction vectors from center: TL(-1,-1), TR(1,-1), BR(1,1), BL(-1,1)
                        let corner_dirs: [(f32, f32); 4] = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];

                        // Compute dot products and rank corners
                        let mut dots: [(f32, usize); 4] = corner_dirs.iter().enumerate()
                            .map(|(i, (cx, cy))| (cx * dir_x + cy * dir_y, i))
                            .collect::<Vec<_>>()
                            .try_into()
                            .unwrap();
                        dots.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                        // dots[0] = most trailing (lowest dot), dots[3] = most leading (highest dot)

                        let base_dur = self.cursor_anim_duration; // seconds
                        for (rank, &(_dot, corner_idx)) in dots.iter().enumerate() {
                            let factor = 1.0 - self.cursor_trail_size * (rank as f32 / 3.0);
                            let duration_i = (base_dur * factor).max(0.01);
                            let omega_i = 4.0 / duration_i;

                            self.cursor_corner_springs[corner_idx].target_x = new_corners[corner_idx].0;
                            self.cursor_corner_springs[corner_idx].target_y = new_corners[corner_idx].1;
                            self.cursor_corner_springs[corner_idx].omega = omega_i;
                            // Don't reset velocity — preserve momentum from in-flight animation
                        }

                        self.cursor_prev_target_cx = new_cx;
                        self.cursor_prev_target_cy = new_cy;
                    }
                }

                // Spawn typing ripple when cursor moves (if enabled)
                if target_moved && had_target && self.typing_ripple_enabled {
                    if let Some(renderer) = self.renderer.as_mut() {
                        let cx = new_target.x + new_target.width / 2.0;
                        let cy = new_target.y + new_target.height / 2.0;
                        renderer.spawn_ripple(cx, cy);
                    }
                }

                // Record cursor trail fade position when cursor moves
                if target_moved && had_target && self.cursor_trail_fade_enabled {
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.record_cursor_trail(
                            self.cursor_current_x,
                            self.cursor_current_y,
                            self.cursor_current_w,
                            self.cursor_current_h,
                        );
                    }
                }

                // Update IME cursor area so candidate window follows text cursor
                if let Some(ref window) = self.window {
                    let x = (new_target.x as f64) * self.scale_factor;
                    let y = (new_target.y as f64 + new_target.height as f64) * self.scale_factor;
                    let w = new_target.width as f64 * self.scale_factor;
                    let h = new_target.height as f64 * self.scale_factor;
                    window.set_ime_cursor_area(
                        winit::dpi::PhysicalPosition::new(x, y),
                        winit::dpi::PhysicalSize::new(w, h),
                    );
                }

                // Detect cursor size change for smooth size transition
                if self.cursor_size_transition_enabled {
                    let dw = (new_target.width - self.cursor_size_target_w).abs();
                    let dh = (new_target.height - self.cursor_size_target_h).abs();
                    if dw > 2.0 || dh > 2.0 {
                        self.cursor_size_animating = true;
                        self.cursor_size_start_w = self.cursor_current_w;
                        self.cursor_size_start_h = self.cursor_current_h;
                        self.cursor_size_anim_start = std::time::Instant::now();
                    }
                    self.cursor_size_target_w = new_target.width;
                    self.cursor_size_target_h = new_target.height;
                }

                self.cursor_target = Some(new_target);
            }
        }
    }

    /// Compute the 4 target corners for a cursor based on its style.
    /// Returns [TL, TR, BR, BL] as (x, y) tuples.
    fn cursor_target_corners(target: &CursorTarget) -> [(f32, f32); 4] {
        match target.style {
            0 => {
                // Filled box: full rectangle
                let x0 = target.x;
                let y0 = target.y;
                let x1 = target.x + target.width;
                let y1 = target.y + target.height;
                [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
            }
            1 => {
                // Bar: thin vertical line (2px wide)
                let x0 = target.x;
                let y0 = target.y;
                let x1 = target.x + 2.0;
                let y1 = target.y + target.height;
                [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
            }
            2 => {
                // Underline: thin horizontal line at bottom (2px tall)
                let x0 = target.x;
                let y0 = target.y + target.height - 2.0;
                let x1 = target.x + target.width;
                let y1 = target.y + target.height;
                [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
            }
            _ => {
                // Default: full rectangle
                let x0 = target.x;
                let y0 = target.y;
                let x1 = target.x + target.width;
                let y1 = target.y + target.height;
                [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
            }
        }
    }

    /// Tick cursor animation, returns true if position changed (needs redraw)
    fn tick_cursor_animation(&mut self) -> bool {
        if !self.cursor_anim_enabled || !self.cursor_animating {
            return false;
        }
        let target = match self.cursor_target.as_ref() {
            Some(t) => t.clone(),
            None => return false,
        };

        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_anim_time).as_secs_f32();
        self.last_anim_time = now;

        match self.cursor_anim_style {
            CursorAnimStyle::Exponential => {
                let factor = 1.0 - (-self.cursor_anim_speed * dt).exp();
                let dx = target.x - self.cursor_current_x;
                let dy = target.y - self.cursor_current_y;
                let dw = target.width - self.cursor_current_w;
                let dh = target.height - self.cursor_current_h;
                self.cursor_current_x += dx * factor;
                self.cursor_current_y += dy * factor;
                self.cursor_current_w += dw * factor;
                self.cursor_current_h += dh * factor;
                if dx.abs() < 0.5 && dy.abs() < 0.5 && dw.abs() < 0.5 && dh.abs() < 0.5 {
                    self.snap_cursor(&target);
                }
            }
            CursorAnimStyle::CriticallyDampedSpring => {
                // 4-corner spring trail: each corner has its own omega (speed).
                // Leading corners (aligned with travel direction) have higher omega (faster).
                // Trailing corners have lower omega (slower), creating a stretching trail.
                let mut all_settled = true;
                for i in 0..4 {
                    let spring = &mut self.cursor_corner_springs[i];
                    let omega = spring.omega;
                    let exp_term = (-omega * dt).exp();

                    // Critically-damped spring per axis
                    // x(t) = (x0 + (v0 + omega*x0)*t) * exp(-omega*t)
                    let x0 = spring.x - spring.target_x;
                    let vx0 = spring.vx;
                    let new_x = (x0 + (vx0 + omega * x0) * dt) * exp_term;
                    spring.vx = ((vx0 + omega * x0) * exp_term)
                        - omega * (x0 + (vx0 + omega * x0) * dt) * exp_term;
                    spring.x = spring.target_x + new_x;

                    let y0 = spring.y - spring.target_y;
                    let vy0 = spring.vy;
                    let new_y = (y0 + (vy0 + omega * y0) * dt) * exp_term;
                    spring.vy = ((vy0 + omega * y0) * exp_term)
                        - omega * (y0 + (vy0 + omega * y0) * dt) * exp_term;
                    spring.y = spring.target_y + new_y;

                    let dist = (spring.x - spring.target_x).abs()
                        + (spring.y - spring.target_y).abs();
                    let vel = spring.vx.abs() + spring.vy.abs();
                    if dist > 0.5 || vel > 1.0 {
                        all_settled = false;
                    }
                }

                // Also update the rect-level position (bounding box of corners)
                let min_x = self.cursor_corner_springs.iter().map(|s| s.x).fold(f32::INFINITY, f32::min);
                let min_y = self.cursor_corner_springs.iter().map(|s| s.y).fold(f32::INFINITY, f32::min);
                let max_x = self.cursor_corner_springs.iter().map(|s| s.x).fold(f32::NEG_INFINITY, f32::max);
                let max_y = self.cursor_corner_springs.iter().map(|s| s.y).fold(f32::NEG_INFINITY, f32::max);
                self.cursor_current_x = min_x;
                self.cursor_current_y = min_y;
                self.cursor_current_w = max_x - min_x;
                self.cursor_current_h = max_y - min_y;

                if all_settled {
                    // Snap all corners to targets
                    let target_corners = Self::cursor_target_corners(&target);
                    for i in 0..4 {
                        self.cursor_corner_springs[i].x = target_corners[i].0;
                        self.cursor_corner_springs[i].y = target_corners[i].1;
                        self.cursor_corner_springs[i].vx = 0.0;
                        self.cursor_corner_springs[i].vy = 0.0;
                    }
                    self.snap_cursor(&target);
                }
            }
            style => {
                // Duration-based easing styles
                let elapsed = now.duration_since(self.cursor_anim_start_time).as_secs_f32();
                let raw_t = (elapsed / self.cursor_anim_duration).min(1.0);
                let t = match style {
                    CursorAnimStyle::EaseOutQuad => ease_out_quad(raw_t),
                    CursorAnimStyle::EaseOutCubic => ease_out_cubic(raw_t),
                    CursorAnimStyle::EaseOutExpo => ease_out_expo(raw_t),
                    CursorAnimStyle::EaseInOutCubic => ease_in_out_cubic(raw_t),
                    CursorAnimStyle::Linear => ease_linear(raw_t),
                    _ => raw_t, // unreachable
                };
                self.cursor_current_x = self.cursor_start_x + (target.x - self.cursor_start_x) * t;
                self.cursor_current_y = self.cursor_start_y + (target.y - self.cursor_start_y) * t;
                self.cursor_current_w = self.cursor_start_w + (target.width - self.cursor_start_w) * t;
                self.cursor_current_h = self.cursor_start_h + (target.height - self.cursor_start_h) * t;
                if raw_t >= 1.0 {
                    self.snap_cursor(&target);
                }
            }
        }

        true
    }

    /// Snap cursor to target and stop animating
    fn snap_cursor(&mut self, target: &CursorTarget) {
        self.cursor_current_x = target.x;
        self.cursor_current_y = target.y;
        self.cursor_current_w = target.width;
        self.cursor_current_h = target.height;
        self.cursor_animating = false;
    }

    /// Tick cursor size transition, returns true if size changed (needs redraw).
    /// Runs AFTER tick_cursor_animation() to override w/h with smooth size interpolation.
    fn tick_cursor_size_animation(&mut self) -> bool {
        if !self.cursor_size_transition_enabled || !self.cursor_size_animating {
            return false;
        }
        let elapsed = self.cursor_size_anim_start.elapsed().as_secs_f32();
        let raw_t = (elapsed / self.cursor_size_transition_duration).min(1.0);
        let t = raw_t * (2.0 - raw_t); // ease-out-quad
        self.cursor_current_w = self.cursor_size_start_w
            + (self.cursor_size_target_w - self.cursor_size_start_w) * t;
        self.cursor_current_h = self.cursor_size_start_h
            + (self.cursor_size_target_h - self.cursor_size_start_h) * t;
        if raw_t >= 1.0 {
            self.cursor_current_w = self.cursor_size_target_w;
            self.cursor_current_h = self.cursor_size_target_h;
            self.cursor_size_animating = false;
        }
        true
    }

    /// Check if any transitions are currently active
    fn has_active_transitions(&self) -> bool {
        !self.crossfades.is_empty() || !self.scroll_slides.is_empty()
    }

    /// Update cursor blink state, returns true if blink toggled
    fn tick_cursor_blink(&mut self) -> bool {
        if !self.cursor_blink_enabled || self.current_frame.is_none() {
            return false;
        }
        // Check if any cursor exists in the current frame
        let has_cursor = self.current_frame.as_ref()
            .map(|f| f.glyphs.iter().any(|g| matches!(g, crate::core::frame_glyphs::FrameGlyph::Cursor { .. })))
            .unwrap_or(false);
        if !has_cursor {
            return false;
        }
        let now = std::time::Instant::now();
        if now.duration_since(self.last_cursor_toggle) >= self.cursor_blink_interval {
            let was_off = !self.cursor_blink_on;
            self.cursor_blink_on = !self.cursor_blink_on;
            self.last_cursor_toggle = now;
            // Trigger wake animation when cursor becomes visible after blink-off
            if was_off && self.cursor_blink_on && self.cursor_wake_enabled {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.trigger_cursor_wake(now);
                }
            }
            true
        } else {
            false
        }
    }

    /// Pump GLib events (non-blocking) and update webkit views
    #[cfg(all(feature = "wpe-webkit", wpe_platform_available))]
    fn pump_glib(&mut self) {
        unsafe {
            // WPEViewHeadless attaches to thread-default context
            let thread_ctx = plat::g_main_context_get_thread_default();
            let ctx = if thread_ctx.is_null() {
                plat::g_main_context_default()
            } else {
                thread_ctx
            };

            // Non-blocking iteration - process all pending events
            while plat::g_main_context_iteration(ctx, 0) != 0 {}

            // Also check default context if different
            let default_ctx = plat::g_main_context_default();
            if default_ctx != ctx {
                while plat::g_main_context_iteration(default_ctx, 0) != 0 {}
            }
        }

        // Update all webkit views and send state change events
        for (id, view) in self.webkit_views.iter_mut() {
            let old_title = view.title.clone();
            let old_url = view.url.clone();
            let old_progress = view.progress;

            view.update();

            // Send state change events
            if view.title != old_title {
                if let Some(ref title) = view.title {
                    self.comms.send_input(InputEvent::WebKitTitleChanged {
                        id: *id,
                        title: title.clone(),
                    });
                }
            }
            if view.url != old_url {
                self.comms.send_input(InputEvent::WebKitUrlChanged {
                    id: *id,
                    url: view.url.clone(),
                });
            }
            if (view.progress - old_progress).abs() > 0.01 {
                self.comms.send_input(InputEvent::WebKitProgressChanged {
                    id: *id,
                    progress: view.progress,
                });
            }
        }
    }

    #[cfg(not(all(feature = "wpe-webkit", wpe_platform_available)))]
    fn pump_glib(&mut self) {}

    /// Process webkit frames and import to wgpu textures
    #[cfg(all(feature = "wpe-webkit", target_os = "linux"))]
    fn process_webkit_frames(&mut self) {
        use crate::backend::wpe::DmaBufData;
        use crate::backend::wgpu::external_buffer::DmaBufBuffer;

        // Get mutable reference to renderer - we need to update its internal webkit cache
        let renderer = match &mut self.renderer {
            Some(r) => r,
            None => {
                log::trace!("process_webkit_frames: no renderer available");
                return;
            }
        };

        if self.webkit_views.is_empty() {
            log::trace!("process_webkit_frames: no webkit views");
            return;
        }

        let policy = self.webkit_import_policy.effective();

        let try_upload_dmabuf = |renderer: &mut WgpuRenderer, view_id: u32, dmabuf: DmaBufData| -> bool {
            let num_planes = dmabuf.fds.len().min(4) as u32;
            let mut fds = [-1i32; 4];
            let mut strides = [0u32; 4];
            let mut offsets = [0u32; 4];

            for i in 0..num_planes as usize {
                fds[i] = dmabuf.fds[i];
                strides[i] = dmabuf.strides[i];
                offsets[i] = dmabuf.offsets[i];
            }

            let buffer = DmaBufBuffer::new(
                fds,
                strides,
                offsets,
                num_planes,
                dmabuf.width,
                dmabuf.height,
                dmabuf.fourcc,
                dmabuf.modifier,
            );

            renderer.update_webkit_view_dmabuf(view_id, buffer)
        };

        for (view_id, view) in &self.webkit_views {
            match policy {
                WebKitImportPolicy::DmaBufFirst => {
                    if let Some(dmabuf) = view.take_latest_dmabuf() {
                        if try_upload_dmabuf(renderer, *view_id, dmabuf) {
                            // Discard pending pixel fallback when DMA-BUF succeeds.
                            let _ = view.take_latest_pixels();
                            log::debug!("Imported DMA-BUF for webkit view {} (dmabuf-first)", view_id);
                        } else if let Some(raw_pixels) = view.take_latest_pixels() {
                            if renderer.update_webkit_view_pixels(*view_id, raw_pixels.width, raw_pixels.height, &raw_pixels.pixels) {
                                log::debug!("Uploaded pixels for webkit view {} (dmabuf-first fallback)", view_id);
                            } else {
                                log::warn!("Both DMA-BUF and pixel upload failed for webkit view {}", view_id);
                            }
                        } else {
                            log::warn!("Both DMA-BUF import and pixel fallback unavailable for webkit view {}", view_id);
                        }
                    } else if let Some(raw_pixels) = view.take_latest_pixels() {
                        if renderer.update_webkit_view_pixels(*view_id, raw_pixels.width, raw_pixels.height, &raw_pixels.pixels) {
                            log::debug!("Uploaded pixels for webkit view {} (dmabuf-first: no dmabuf frame)", view_id);
                        }
                    }
                }
                WebKitImportPolicy::PixelsFirst | WebKitImportPolicy::Auto => {
                    // Prefer pixel upload over DMA-BUF zero-copy.
                    //
                    // wgpu's create_texture_from_hal() always inserts textures with
                    // UNINITIALIZED tracking state, causing a second UNDEFINED layout
                    // transition that discards DMA-BUF content on AMD RADV (and
                    // potentially other drivers with compressed modifiers like DCC/CCS).
                    // Until wgpu supports pre-initialized HAL textures, pixel upload
                    // via wpe_buffer_import_to_pixels() is the reliable path.
                    if let Some(raw_pixels) = view.take_latest_pixels() {
                        // Drain any pending DMA-BUF so it doesn't accumulate
                        let _ = view.take_latest_dmabuf();
                        if renderer.update_webkit_view_pixels(*view_id, raw_pixels.width, raw_pixels.height, &raw_pixels.pixels) {
                            log::debug!("Uploaded pixels for webkit view {}", view_id);
                        }
                    }
                    // DMA-BUF zero-copy fallback (only if no pixel data available)
                    else if let Some(dmabuf) = view.take_latest_dmabuf() {
                        if try_upload_dmabuf(renderer, *view_id, dmabuf) {
                            log::debug!("Imported DMA-BUF for webkit view {} (pixels-first fallback)", view_id);
                        } else if let Some(raw_pixels) = view.take_latest_pixels() {
                            if renderer.update_webkit_view_pixels(*view_id, raw_pixels.width, raw_pixels.height, &raw_pixels.pixels) {
                                log::debug!("Uploaded pixels for webkit view {} (pixels-first second fallback)", view_id);
                            } else {
                                log::warn!("Both pixel and DMA-BUF import failed for webkit view {}", view_id);
                            }
                        } else {
                            log::warn!("Both pixel and DMA-BUF import failed for webkit view {}", view_id);
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(all(feature = "wpe-webkit", target_os = "linux")))]
    fn process_webkit_frames(&mut self) {}

    /// Process pending video frames
    #[cfg(feature = "video")]
    fn process_video_frames(&mut self) {
        log::trace!("process_video_frames called");
        if let Some(ref mut renderer) = self.renderer {
            renderer.process_pending_videos();
        }
    }

    #[cfg(not(feature = "video"))]
    fn process_video_frames(&mut self) {}

    /// Check if any video is currently playing (needs continuous rendering)
    #[cfg(feature = "video")]
    fn has_playing_videos(&self) -> bool {
        self.renderer.as_ref().map_or(false, |r| r.has_playing_videos())
    }

    #[cfg(not(feature = "video"))]
    fn has_playing_videos(&self) -> bool { false }

    /// Check if any WebKit view needs redraw
    #[cfg(feature = "wpe-webkit")]
    fn has_webkit_needing_redraw(&self) -> bool {
        self.webkit_views.values().any(|v| v.needs_redraw())
    }

    #[cfg(not(feature = "wpe-webkit"))]
    fn has_webkit_needing_redraw(&self) -> bool { false }

    /// Check if any terminal has pending content from PTY reader threads.
    #[cfg(feature = "neo-term")]
    fn has_terminal_activity(&self) -> bool {
        for view in self.terminal_manager.terminals.values() {
            if view.event_proxy.peek_wakeup() || view.dirty {
                return true;
            }
        }
        false
    }

    #[cfg(not(feature = "neo-term"))]
    fn has_terminal_activity(&self) -> bool { false }

    /// Process pending image uploads (decode → GPU texture)
    fn process_pending_images(&mut self) {
        if let Some(ref mut renderer) = self.renderer {
            renderer.process_pending_images();
        }
    }

    /// Ensure offscreen textures exist (lazily created)
    fn ensure_offscreen_textures(&mut self) {
        if self.offscreen_a.is_some() && self.offscreen_b.is_some() {
            return;
        }
        let renderer = match self.renderer.as_ref() {
            Some(r) => r,
            None => return,
        };
        let w = self.width;
        let h = self.height;

        if self.offscreen_a.is_none() {
            let (tex, view) = renderer.create_offscreen_texture(w, h);
            let bg = renderer.create_texture_bind_group(&view);
            self.offscreen_a = Some((tex, view, bg));
        }
        if self.offscreen_b.is_none() {
            let (tex, view) = renderer.create_offscreen_texture(w, h);
            let bg = renderer.create_texture_bind_group(&view);
            self.offscreen_b = Some((tex, view, bg));
        }
    }

    /// Get the "current" offscreen texture view and bind group
    fn current_offscreen_view_and_bg(&self) -> Option<(&wgpu::TextureView, &wgpu::BindGroup)> {
        let (_, ref view, ref bg) = if self.current_is_a {
            self.offscreen_a.as_ref()?
        } else {
            self.offscreen_b.as_ref()?
        };
        Some((view, bg))
    }

    /// Get the "previous" offscreen texture, view, and bind group
    fn previous_offscreen(&self) -> Option<(&wgpu::Texture, &wgpu::TextureView, &wgpu::BindGroup)> {
        let (ref tex, ref view, ref bg) = if self.current_is_a {
            self.offscreen_b.as_ref()?
        } else {
            self.offscreen_a.as_ref()?
        };
        Some((tex, view, bg))
    }

    /// Snapshot the previous offscreen texture into a new dedicated texture
    fn snapshot_prev_texture(&self) -> Option<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)> {
        let renderer = self.renderer.as_ref()?;
        let (prev_tex, _, _) = self.previous_offscreen()?;

        let (snap, snap_view) = renderer.create_offscreen_texture(self.width, self.height);

        // GPU copy
        let mut encoder = renderer.device().create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Snapshot Copy Encoder"),
        });
        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: prev_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: &snap,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        renderer.queue().submit(std::iter::once(encoder.finish()));

        let snap_bg = renderer.create_texture_bind_group(&snap_view);
        Some((snap, snap_view, snap_bg))
    }

    /// Detect transitions by comparing current and previous window infos
    fn detect_transitions(&mut self) {
        let frame = match self.current_frame.as_ref() {
            Some(f) => f,
            None => return,
        };

        let now = std::time::Instant::now();

        for info in &frame.window_infos {
            if let Some(prev) = self.prev_window_infos.get(&info.window_id) {
                if prev.buffer_id != 0 && info.buffer_id != 0 {
                    if prev.buffer_id != info.buffer_id {
                        // Text fade-in on buffer switch
                        if self.text_fade_in_enabled && !info.is_minibuffer {
                            if let Some(renderer) = self.renderer.as_mut() {
                                renderer.trigger_text_fade_in(info.window_id, info.bounds, now);
                            }
                        }
                        // Buffer switch → crossfade
                        // Suppress for minibuffer (small windows change buffers on every keystroke)
                        if self.crossfade_enabled && info.bounds.height >= 50.0 {
                            // Cancel existing transition for this window
                            self.crossfades.remove(&info.window_id);
                            self.scroll_slides.remove(&info.window_id);

                            if let Some((tex, view, bg)) = self.snapshot_prev_texture() {
                                log::debug!("Starting crossfade for window {} (buffer changed, effect={:?})", info.window_id, self.crossfade_effect);
                                self.crossfades.insert(info.window_id, CrossfadeTransition {
                                    started: now,
                                    duration: self.crossfade_duration,
                                    bounds: info.bounds,
                                    effect: self.crossfade_effect,
                                    easing: self.crossfade_easing,
                                    old_texture: tex,
                                    old_view: view,
                                    old_bind_group: bg,
                                });
                            }
                        }
                    } else if info.is_minibuffer
                        && (prev.bounds.height - info.bounds.height).abs() > 2.0
                    {
                        // Minibuffer height change → crossfade
                        if self.crossfade_enabled {
                            self.crossfades.remove(&info.window_id);
                            self.scroll_slides.remove(&info.window_id);

                            if let Some((tex, view, bg)) = self.snapshot_prev_texture() {
                                log::debug!("Starting minibuffer crossfade (height {} → {})",
                                    prev.bounds.height, info.bounds.height);
                                self.crossfades.insert(info.window_id, CrossfadeTransition {
                                    started: now,
                                    duration: std::time::Duration::from_millis(150),
                                    bounds: info.bounds,
                                    effect: self.crossfade_effect,
                                    easing: self.crossfade_easing,
                                    old_texture: tex,
                                    old_view: view,
                                    old_bind_group: bg,
                                });
                            }
                        }
                    } else if prev.window_start != info.window_start {
                        // Text fade-in on scroll
                        if self.text_fade_in_enabled && !info.is_minibuffer {
                            if let Some(renderer) = self.renderer.as_mut() {
                                renderer.trigger_text_fade_in(info.window_id, info.bounds, now);
                            }
                        }
                        // Scroll line spacing animation (accordion effect)
                        if self.scroll_line_spacing_enabled {
                            let dir = if info.window_start > prev.window_start { 1 } else { -1 };
                            if let Some(renderer) = self.renderer.as_mut() {
                                renderer.trigger_scroll_line_spacing(info.window_id, info.bounds, dir, now);
                            }
                        }
                        // Scroll momentum indicator
                        if self.scroll_momentum_enabled && !info.is_minibuffer {
                            let dir = if info.window_start > prev.window_start { 1 } else { -1 };
                            if let Some(renderer) = self.renderer.as_mut() {
                                renderer.trigger_scroll_momentum(info.window_id, info.bounds, dir, now);
                            }
                        }
                        // Scroll → slide (content area only, excluding mode-line)
                        let content_height = info.bounds.height - info.mode_line_height;
                        if self.scroll_enabled && content_height >= 50.0 {
                            // Cancel existing transition for this window
                            self.crossfades.remove(&info.window_id);
                            self.scroll_slides.remove(&info.window_id);

                            let dir = if info.window_start > prev.window_start { 1 } else { -1 };

                            // Use content-only bounds (exclude mode-line at bottom)
                            let content_bounds = Rect::new(
                                info.bounds.x, info.bounds.y,
                                info.bounds.width, content_height,
                            );

                            if let Some((tex, view, bg)) = self.snapshot_prev_texture() {
                                log::debug!("Starting scroll slide for window {} (dir={}, effect={:?}, content_h={})",
                                    info.window_id, dir, self.scroll_effect, content_height);
                                self.scroll_slides.insert(info.window_id, ScrollTransition {
                                    started: now,
                                    duration: self.scroll_duration,
                                    bounds: content_bounds,
                                    direction: dir,
                                    effect: self.scroll_effect,
                                    easing: self.scroll_easing,
                                    old_texture: tex,
                                    old_view: view,
                                    old_bind_group: bg,
                                });
                            }
                        }
                    } else if (prev.char_height - info.char_height).abs() > 1.0 {
                        // Font size changed (text-scale-adjust) → crossfade
                        if self.crossfade_enabled {
                            self.crossfades.remove(&info.window_id);
                            self.scroll_slides.remove(&info.window_id);

                            if let Some((tex, view, bg)) = self.snapshot_prev_texture() {
                                log::debug!("Starting font-size crossfade for window {} (char_height {} → {})",
                                    info.window_id, prev.char_height, info.char_height);
                                self.crossfades.insert(info.window_id, CrossfadeTransition {
                                    started: now,
                                    duration: std::time::Duration::from_millis(200),
                                    bounds: info.bounds,
                                    effect: self.crossfade_effect,
                                    easing: self.crossfade_easing,
                                    old_texture: tex,
                                    old_view: view,
                                    old_bind_group: bg,
                                });
                            }
                        }
                    } else if self.line_animation_enabled
                        && prev.buffer_size != info.buffer_size
                        && !info.is_minibuffer
                    {
                        // Buffer size changed with same window_start → line insertion/deletion
                        // Find cursor Y from frame glyphs as the edit point
                        let mut cursor_y: Option<f32> = None;
                        for g in &frame.glyphs {
                            if let crate::core::frame_glyphs::FrameGlyph::Cursor { x, y, style, .. } = g {
                                // Check cursor is within this window
                                if *x >= info.bounds.x && *x < info.bounds.x + info.bounds.width
                                    && *y >= info.bounds.y && *y < info.bounds.y + info.bounds.height
                                    && *style != 3
                                {
                                    cursor_y = Some(*y);
                                    break;
                                }
                            }
                        }
                        if let Some(edit_y) = cursor_y {
                            let ch = info.char_height;
                            let delta = info.buffer_size - prev.buffer_size;
                            // Positive delta = insertion (lines move down), negative = deletion (lines move up)
                            let offset = if delta > 0 { -ch } else { ch };
                            if let Some(renderer) = self.renderer.as_mut() {
                                renderer.start_line_animation(
                                    info.bounds,
                                    edit_y + ch, // animate rows below cursor
                                    offset,
                                    self.line_animation_duration_ms,
                                );
                            }
                        }
                    } else if (prev.bounds.width - info.bounds.width).abs() > 2.0
                        || (prev.bounds.height - info.bounds.height).abs() > 2.0
                    {
                        // Window resized (balance-windows, divider drag) → crossfade
                        if self.crossfade_enabled && !info.is_minibuffer {
                            self.crossfades.remove(&info.window_id);
                            self.scroll_slides.remove(&info.window_id);

                            // Use full-frame crossfade (window_id 0) since
                            // all windows resize together during balance
                            let full_bounds = Rect::new(0.0, 0.0, frame.width, frame.height);
                            if !self.crossfades.contains_key(&0) {
                                if let Some((tex, view, bg)) = self.snapshot_prev_texture() {
                                    log::debug!("Starting window-resize crossfade (bounds changed)");
                                    self.crossfades.insert(0, CrossfadeTransition {
                                        started: now,
                                        duration: std::time::Duration::from_millis(150),
                                        bounds: full_bounds,
                                        effect: self.crossfade_effect,
                                        easing: self.crossfade_easing,
                                        old_texture: tex,
                                        old_view: view,
                                        old_bind_group: bg,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Detect window split/delete (window count or IDs changed)
        if self.crossfade_enabled && !self.prev_window_infos.is_empty() {
            let curr_ids: std::collections::HashSet<i64> = frame.window_infos.iter()
                .filter(|i| !i.is_minibuffer)
                .map(|i| i.window_id)
                .collect();
            let prev_non_mini: std::collections::HashSet<i64> = self.prev_window_infos.iter()
                .filter(|(_, v)| !v.is_minibuffer)
                .map(|(k, _)| *k)
                .collect();

            if prev_non_mini != curr_ids && prev_non_mini.len() > 0 && curr_ids.len() > 0 {
                // Window layout changed — full-frame crossfade
                // Use a synthetic window_id (0) for the full-frame transition
                let full_bounds = Rect::new(0.0, 0.0, frame.width, frame.height);
                if let Some((tex, view, bg)) = self.snapshot_prev_texture() {
                    log::debug!("Starting window split/delete crossfade ({} → {} windows)",
                        prev_non_mini.len(), curr_ids.len());
                    self.crossfades.insert(0, CrossfadeTransition {
                        started: now,
                        duration: std::time::Duration::from_millis(200),
                        bounds: full_bounds,
                        effect: self.crossfade_effect,
                        easing: self.crossfade_easing,
                        old_texture: tex,
                        old_view: view,
                        old_bind_group: bg,
                    });
                }
            }
        }

        // Detect window switch (selected window changed) → highlight fade
        if self.window_switch_fade_enabled {
            let mut new_selected: Option<(i64, Rect)> = None;
            for info in &frame.window_infos {
                if info.selected && !info.is_minibuffer {
                    new_selected = Some((info.window_id, info.bounds));
                    break;
                }
            }
            if let Some((wid, bounds)) = new_selected {
                if self.prev_selected_window_id != 0 && wid != self.prev_selected_window_id {
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.start_window_fade(wid, bounds);
                        self.frame_dirty = true;
                    }
                }
                self.prev_selected_window_id = wid;
            }
        }

        // Update prev_window_infos from current frame
        self.prev_window_infos.clear();
        for info in &frame.window_infos {
            self.prev_window_infos.insert(info.window_id, info.clone());
        }
    }

    /// Render active transitions on top of the surface
    fn render_transitions(&mut self, surface_view: &wgpu::TextureView) {
        let now = std::time::Instant::now();
        let renderer = match self.renderer.as_ref() {
            Some(r) => r,
            None => return,
        };

        // Get current offscreen bind group for "new" texture
        let current_bg = match self.current_offscreen_view_and_bg() {
            Some((_, bg)) => bg as *const wgpu::BindGroup,
            None => return,
        };

        // Render crossfades (using per-transition effect/easing)
        let mut completed_crossfades = Vec::new();
        for (&wid, transition) in &self.crossfades {
            let elapsed = now.duration_since(transition.started);
            let raw_t = (elapsed.as_secs_f32() / transition.duration.as_secs_f32()).min(1.0);
            let elapsed_secs = elapsed.as_secs_f32();

            // SAFETY: current_bg is valid for the duration of this function
            renderer.render_scroll_effect(
                surface_view,
                &transition.old_bind_group,
                unsafe { &*current_bg },
                raw_t,
                elapsed_secs,
                1, // direction: forward
                &transition.bounds,
                transition.effect,
                transition.easing,
                self.width,
                self.height,
            );

            if raw_t >= 1.0 {
                completed_crossfades.push(wid);
            }
        }
        for wid in completed_crossfades {
            self.crossfades.remove(&wid);
        }

        // Render scroll slides
        let mut completed_scrolls = Vec::new();
        for (&wid, transition) in &self.scroll_slides {
            let elapsed = now.duration_since(transition.started);
            let raw_t = (elapsed.as_secs_f32() / transition.duration.as_secs_f32()).min(1.0);
            let elapsed_secs = elapsed.as_secs_f32();

            renderer.render_scroll_effect(
                surface_view,
                &transition.old_bind_group,
                unsafe { &*current_bg },
                raw_t,
                elapsed_secs,
                transition.direction,
                &transition.bounds,
                transition.effect,
                transition.easing,
                self.width,
                self.height,
            );

            if raw_t >= 1.0 {
                completed_scrolls.push(wid);
            }
        }
        for wid in completed_scrolls {
            self.scroll_slides.remove(&wid);
        }
    }

    /// Update terminal content and expand Terminal glyphs into renderable cells.
    #[cfg(feature = "neo-term")]
    fn update_terminals(&mut self) {
        use crate::terminal::TerminalMode;

        // Get frame font metrics for terminal cell sizing.
        // These come from FRAME_COLUMN_WIDTH / FRAME_LINE_HEIGHT / FRAME_FONT->pixel_size.
        let (cell_w, cell_h, font_size, frame_w, frame_h) = if let Some(ref frame) = self.current_frame {
            (frame.char_width, frame.char_height, frame.font_pixel_size,
             frame.width, frame.height)
        } else {
            (8.0, 16.0, 14.0, self.width as f32, self.height as f32)
        };
        let ascent = cell_h * 0.8;

        // Auto-resize Window-mode terminals to fit the frame area.
        // Reserve space for mode-line (~1 row) and echo area (~1 row).
        let term_area_height = (frame_h - cell_h * 2.0).max(cell_h);
        let target_cols = (frame_w / cell_w).floor() as u16;
        let target_rows = (term_area_height / cell_h).floor() as u16;

        if target_cols > 0 && target_rows > 0 {
            for id in self.terminal_manager.ids() {
                if let Some(view) = self.terminal_manager.get_mut(id) {
                    if view.mode != TerminalMode::Window {
                        continue;
                    }
                    // Resize if grid dimensions changed
                    if let Some(content) = view.content() {
                        if content.cols as u16 != target_cols || content.rows as u16 != target_rows {
                            view.resize(target_cols, target_rows);
                        }
                    }
                }
            }
        }

        // Update all terminal content (check for PTY data)
        self.terminal_manager.update_all();

        // Check for exited terminals and notify Emacs
        for id in self.terminal_manager.ids() {
            if let Some(view) = self.terminal_manager.get_mut(id) {
                if view.event_proxy.is_exited() && !view.exit_notified {
                    view.exit_notified = true;
                    self.comms.send_input(InputEvent::TerminalExited { id });
                }
            }
        }

        // Expand FrameGlyph::Terminal entries (placed by C redisplay) into cells
        if let Some(ref mut frame) = self.current_frame {
            let mut extra_glyphs = Vec::new();

            for glyph in &frame.glyphs {
                if let FrameGlyph::Terminal { terminal_id, x, y, width, height } = glyph {
                    if let Some(view) = self.terminal_manager.get(*terminal_id) {
                        if let Some(content) = view.content() {
                            extra_glyphs.push(FrameGlyph::Stretch {
                                x: *x, y: *y, width: *width, height: *height,
                                bg: content.default_bg, face_id: 0, is_overlay: false,
                            });

                            Self::expand_terminal_cells(
                                content, *x, *y, cell_w, cell_h, ascent, font_size,
                                false, 1.0, &mut extra_glyphs,
                            );
                        }
                    }
                }
            }

            if !extra_glyphs.is_empty() {
                frame.glyphs.extend(extra_glyphs);
                self.frame_dirty = true;
            }
        }

        // Render Window-mode terminals as overlays covering the frame body.
        if let Some(ref mut frame) = self.current_frame {
            let mut win_glyphs = Vec::new();
            for id in self.terminal_manager.ids() {
                if let Some(view) = self.terminal_manager.get(id) {
                    if view.mode != TerminalMode::Window {
                        continue;
                    }
                    if let Some(content) = view.content() {
                        let x = 0.0_f32;
                        let y = 0.0_f32;
                        let width = content.cols as f32 * cell_w;
                        let height = content.rows as f32 * cell_h;

                        // Terminal background
                        win_glyphs.push(FrameGlyph::Stretch {
                            x, y, width, height,
                            bg: content.default_bg, face_id: 0, is_overlay: true,
                        });

                        Self::expand_terminal_cells(
                            content, x, y, cell_w, cell_h, ascent, font_size,
                            true, 1.0, &mut win_glyphs,
                        );
                    }
                }
            }

            if !win_glyphs.is_empty() {
                frame.glyphs.extend(win_glyphs);
                self.frame_dirty = true;
            }
        }

        // Render floating terminals
        if let Some(ref mut frame) = self.current_frame {
            let mut float_glyphs = Vec::new();
            for id in self.terminal_manager.ids() {
                if let Some(view) = self.terminal_manager.get(id) {
                    if view.mode != TerminalMode::Floating {
                        continue;
                    }
                    if let Some(content) = view.content() {
                        let x = view.float_x;
                        let y = view.float_y;
                        let width = content.cols as f32 * cell_w;
                        let height = content.rows as f32 * cell_h;

                        let mut bg = content.default_bg;
                        bg.a = view.float_opacity;
                        float_glyphs.push(FrameGlyph::Stretch {
                            x, y, width, height, bg, face_id: 0, is_overlay: true,
                        });

                        Self::expand_terminal_cells(
                            content, x, y, cell_w, cell_h, ascent, font_size,
                            true, view.float_opacity, &mut float_glyphs,
                        );
                    }
                }
            }

            if !float_glyphs.is_empty() {
                frame.glyphs.extend(float_glyphs);
                self.frame_dirty = true;
            }
        }
    }

    /// Expand terminal content cells into FrameGlyph entries.
    #[cfg(feature = "neo-term")]
    fn expand_terminal_cells(
        content: &crate::terminal::content::TerminalContent,
        origin_x: f32,
        origin_y: f32,
        cell_w: f32,
        cell_h: f32,
        ascent: f32,
        font_size: f32,
        is_overlay: bool,
        opacity: f32,
        out: &mut Vec<FrameGlyph>,
    ) {
        use alacritty_terminal::term::cell::Flags as CellFlags;

        for cell in &content.cells {
            let cx = origin_x + cell.col as f32 * cell_w;
            let cy = origin_y + cell.row as f32 * cell_h;

            if cell.bg != content.default_bg {
                let mut bg = cell.bg;
                bg.a *= opacity;
                out.push(FrameGlyph::Stretch {
                    x: cx, y: cy, width: cell_w, height: cell_h,
                    bg, face_id: 0, is_overlay,
                });
            }

            if cell.c != ' ' && cell.c != '\0' {
                let mut fg = cell.fg;
                fg.a *= opacity;
                out.push(FrameGlyph::Char {
                    char: cell.c,
                    x: cx, y: cy,
                    width: cell_w, height: cell_h,
                    ascent, fg,
                    bg: None, face_id: 0,
                    bold: cell.flags.contains(CellFlags::BOLD),
                    italic: cell.flags.contains(CellFlags::ITALIC),
                    font_size,
                    underline: if cell.flags.contains(CellFlags::UNDERLINE) { 1 } else { 0 },
                    underline_color: None,
                    strike_through: if cell.flags.contains(CellFlags::STRIKEOUT) { 1 } else { 0 },
                    strike_through_color: None,
                    overline: 0, overline_color: None,
                    is_overlay,
                });
            }
        }

        // Terminal cursor
        if content.cursor.visible {
            let cx = origin_x + content.cursor.col as f32 * cell_w;
            let cy = origin_y + content.cursor.row as f32 * cell_h;
            let mut fg = content.default_fg;
            fg.a *= opacity;
            out.push(FrameGlyph::Border {
                x: cx, y: cy, width: cell_w, height: cell_h,
                color: fg,
            });
        }
    }

    /// Apply extra line spacing and letter spacing to glyph positions.
    /// Groups glyphs by Y position (rows) and applies cumulative offsets.
    fn apply_extra_spacing(
        glyphs: &mut [FrameGlyph],
        line_spacing: f32,
        letter_spacing: f32,
    ) {
        use crate::core::frame_glyphs::FrameGlyph;

        let mut last_y: f32 = f32::NEG_INFINITY;
        let mut row_index: i32 = -1;
        let mut char_in_row: i32 = 0;
        let mut last_window_y: f32 = f32::NEG_INFINITY;

        for glyph in glyphs.iter_mut() {
            match glyph {
                FrameGlyph::Char { x, y, is_overlay, .. } => {
                    if *is_overlay { continue; }
                    // Detect window boundary: Y jumps backwards
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
                }
                FrameGlyph::Stretch { x, y, is_overlay, .. } => {
                    if *is_overlay { continue; }
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
                }
                FrameGlyph::Cursor { y, x, .. } => {
                    // Apply same row-based Y offset to cursor
                    if (*y - last_y).abs() < 0.5 {
                        *y += row_index.max(0) as f32 * line_spacing;
                        *x += char_in_row as f32 * letter_spacing;
                    }
                }
                _ => {}
            }
        }
    }

    fn render(&mut self) {
        // Early return checks
        if self.current_frame.is_none()
            || self.surface.is_none()
            || self.renderer.is_none()
            || self.glyph_atlas.is_none()
        {
            return;
        }

        // FPS tracking
        if self.show_fps {
            self.fps_render_start = std::time::Instant::now();
            self.fps_frame_count += 1;
            let elapsed = self.fps_last_instant.elapsed();
            if elapsed.as_secs_f32() >= 1.0 {
                self.fps_display_value =
                    self.fps_frame_count as f32 / elapsed.as_secs_f32();
                self.fps_frame_count = 0;
                self.fps_last_instant = std::time::Instant::now();
            }
        }

        // Update terminals (expand terminal glyphs into renderable cells)
        #[cfg(feature = "neo-term")]
        self.update_terminals();

        // Process webkit frames (import DMA-BUF to textures)
        self.process_webkit_frames();

        // Process video frames
        self.process_video_frames();

        // Process pending image uploads (decoded images → GPU textures)
        self.process_pending_images();

        // Update faces from frame data (the frame carries the full face map
        // set by the FFI side, including box/underline/overline attributes).
        if let Some(ref frame) = self.current_frame {
            // Use full face data from frame (set by neomacs_display_set_face FFI)
            for (face_id, face) in &frame.faces {
                self.faces.insert(*face_id, face.clone());
            }
            // Also update font families from the per-glyph font cache
            for (face_id, font_family) in &frame.face_fonts {
                if let Some(face) = self.faces.get_mut(face_id) {
                    face.font_family = font_family.clone();
                }
            }
        }

        // Apply extra spacing adjustments to glyph positions
        if self.extra_line_spacing != 0.0 || self.extra_letter_spacing != 0.0 {
            if let Some(ref mut frame) = self.current_frame {
                Self::apply_extra_spacing(
                    &mut frame.glyphs,
                    self.extra_line_spacing,
                    self.extra_letter_spacing,
                );
            }
        }

        // Get surface texture
        let Some(surface) = self.surface.as_ref() else {
            return;
        };
        let output = match surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Lost) => {
                // Reconfigure surface
                let (w, h) = (self.width, self.height);
                self.handle_resize(w, h);
                return;
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                log::error!("Out of GPU memory");
                return;
            }
            Err(e) => {
                log::warn!("Surface error: {:?}", e);
                return;
            }
        };

        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Build animated cursor override if applicable
        let animated_cursor = if let (true, Some(target)) =
            (self.cursor_anim_enabled, self.cursor_target.as_ref())
        {
            let corners = if self.cursor_anim_style == CursorAnimStyle::CriticallyDampedSpring
                && self.cursor_animating
            {
                Some([
                    (self.cursor_corner_springs[0].x, self.cursor_corner_springs[0].y),
                    (self.cursor_corner_springs[1].x, self.cursor_corner_springs[1].y),
                    (self.cursor_corner_springs[2].x, self.cursor_corner_springs[2].y),
                    (self.cursor_corner_springs[3].x, self.cursor_corner_springs[3].y),
                ])
            } else {
                None
            };
            Some(AnimatedCursor {
                window_id: target.window_id,
                x: self.cursor_current_x,
                y: self.cursor_current_y,
                width: self.cursor_current_w,
                height: self.cursor_current_h,
                corners,
            })
        } else {
            None
        };

        // Build background gradient option
        let bg_gradient = if self.bg_gradient_enabled {
            Some((self.bg_gradient_top, self.bg_gradient_bottom))
        } else {
            None
        };

        // Check if we need offscreen rendering (for transitions)
        let need_offscreen = self.crossfade_enabled || self.scroll_enabled;

        if need_offscreen {
            // Swap: previous ← current
            self.current_is_a = !self.current_is_a;

            // Ensure offscreen textures exist
            self.ensure_offscreen_textures();

            // Render frame to current offscreen texture
            if let Some((current_view, _)) = self.current_offscreen_view_and_bg()
                .map(|(v, bg)| (v as *const wgpu::TextureView, bg))
            {
                let frame = self.current_frame.as_ref().expect("checked in render");
                let renderer = self.renderer.as_mut().expect("checked in render");
                let glyph_atlas = self.glyph_atlas.as_mut().expect("checked in render");
                renderer.set_idle_dim_alpha(self.idle_dim_current_alpha);

                // SAFETY: current_view is valid for the duration of this block
                renderer.render_frame_glyphs(
                    unsafe { &*current_view },
                    frame,
                    glyph_atlas,
                    &self.faces,
                    self.width,
                    self.height,
                    self.cursor_blink_on,
                    animated_cursor,
                    self.mouse_pos,
                    bg_gradient,
                );
            }

            // Detect transitions (compare window_infos)
            self.detect_transitions();

            // Blit current offscreen to surface
            if let Some((_, current_bg)) = self.current_offscreen_view_and_bg()
                .map(|(v, bg)| (v, bg as *const wgpu::BindGroup))
            {
                let renderer = self.renderer.as_ref().expect("checked in render");
                renderer.blit_texture_to_view(
                    unsafe { &*current_bg },
                    &surface_view,
                    self.width,
                    self.height,
                );
            }

            // Composite active transitions on top
            self.render_transitions(&surface_view);
        } else {
            // Simple path: render directly to surface
            let frame = self.current_frame.as_ref().expect("checked in render");
            let renderer = self.renderer.as_mut().expect("checked in render");
            let glyph_atlas = self.glyph_atlas.as_mut().expect("checked in render");
            renderer.set_idle_dim_alpha(self.idle_dim_current_alpha);

            renderer.render_frame_glyphs(
                &surface_view,
                frame,
                glyph_atlas,
                &self.faces,
                self.width,
                self.height,
                self.cursor_blink_on,
                animated_cursor,
                self.mouse_pos,
                bg_gradient,
            );
        }

        // Render breadcrumb/path bar overlay
        if self.breadcrumb_enabled {
            if let (Some(ref mut renderer), Some(ref mut glyph_atlas), Some(ref frame)) =
                (&mut self.renderer, &mut self.glyph_atlas, &self.current_frame)
            {
                renderer.render_breadcrumbs(&surface_view, frame, glyph_atlas);
            }
        }

        // Render scroll position indicators and focus ring
        if self.scroll_indicators_enabled {
            if let (Some(ref renderer), Some(ref frame)) =
                (&self.renderer, &self.current_frame)
            {
                renderer.render_scroll_indicators(
                    &surface_view, &frame.window_infos,
                    self.width, self.height,
                );
            }
        }

        // Render window watermarks for empty/small buffers
        if self.window_watermark_enabled {
            if let (Some(ref renderer), Some(ref mut glyph_atlas), Some(ref frame)) =
                (&self.renderer, &mut self.glyph_atlas, &self.current_frame)
            {
                renderer.render_window_watermarks(&surface_view, frame, glyph_atlas);
            }
        }

        // Render custom title bar when decorations are disabled (not in fullscreen)
        if !self.decorations_enabled && !self.is_fullscreen && self.custom_titlebar_height > 0.0 {
            if let (Some(ref renderer), Some(ref mut glyph_atlas)) =
                (&self.renderer, &mut self.glyph_atlas)
            {
                let frame_bg = self.current_frame.as_ref()
                    .map(|f| (f.background.r, f.background.g, f.background.b));
                renderer.render_custom_titlebar(
                    &surface_view,
                    &self.window_title,
                    self.custom_titlebar_height,
                    self.titlebar_hover,
                    frame_bg,
                    glyph_atlas,
                    self.width,
                    self.height,
                );
            }
        }

        // Render floating WebKit overlays on top of everything
        #[cfg(feature = "wpe-webkit")]
        if !self.floating_webkits.is_empty() {
            if let Some(ref renderer) = self.renderer {
                renderer.render_floating_webkits(&surface_view, &self.floating_webkits);
            }
        }

        // Render popup menu overlay (topmost layer)
        if let Some(ref menu) = self.popup_menu {
            if let (Some(ref renderer), Some(ref mut glyph_atlas)) =
                (&self.renderer, &mut self.glyph_atlas)
            {
                renderer.render_popup_menu(&surface_view, menu, glyph_atlas, self.width, self.height);
            }
        }

        // Render tooltip overlay (above everything including popup menu)
        if let Some(ref tip) = self.tooltip {
            if let (Some(ref renderer), Some(ref mut glyph_atlas)) =
                (&self.renderer, &mut self.glyph_atlas)
            {
                renderer.render_tooltip(&surface_view, tip, glyph_atlas, self.width, self.height);
            }
        }

        // Render IME preedit text overlay at cursor position
        if self.ime_preedit_active && !self.ime_preedit_text.is_empty() {
            if let (Some(ref renderer), Some(ref mut glyph_atlas), Some(ref target)) =
                (&self.renderer, &mut self.glyph_atlas, &self.cursor_target)
            {
                renderer.render_ime_preedit(
                    &surface_view,
                    &self.ime_preedit_text,
                    target.x,
                    target.y,
                    target.height,
                    glyph_atlas,
                    self.width,
                    self.height,
                );
            }
        }

        // Render visual bell flash overlay (above everything)
        if let Some(start) = self.visual_bell_start {
            let elapsed = start.elapsed().as_secs_f32();
            let duration = 0.15; // 150ms flash
            if elapsed < duration {
                let alpha = (1.0 - elapsed / duration) * 0.3; // max 30% opacity, fading out
                if let Some(ref renderer) = self.renderer {
                    renderer.render_visual_bell(
                        &surface_view,
                        self.width, self.height,
                        alpha,
                    );
                }
                self.frame_dirty = true; // Keep redrawing during animation
            } else {
                self.visual_bell_start = None;
            }
        }

        // Render FPS counter overlay (topmost) with profiling stats
        if self.show_fps {
            // Measure frame time
            let frame_time = self.fps_render_start.elapsed().as_secs_f32() * 1000.0;
            // Exponential moving average (smooth over ~10 frames)
            self.fps_frame_time_ms = self.fps_frame_time_ms * 0.9 + frame_time * 0.1;

            // Gather stats
            let glyph_count = self.current_frame.as_ref()
                .map(|f| f.glyphs.len())
                .unwrap_or(0);
            let window_count = self.current_frame.as_ref()
                .map(|f| f.window_infos.len())
                .unwrap_or(0);
            let transition_count = self.crossfades.len() + self.scroll_slides.len();

            // Build multi-line stats text
            let stats_lines = vec![
                format!("{:.0} FPS | {:.1}ms", self.fps_display_value, self.fps_frame_time_ms),
                format!("{}g {}w {}t  {}x{}", glyph_count, window_count,
                    transition_count, self.width, self.height),
            ];

            if let (Some(ref renderer), Some(ref mut glyph_atlas)) =
                (&self.renderer, &mut self.glyph_atlas)
            {
                renderer.render_fps_overlay(
                    &surface_view,
                    &stats_lines,
                    glyph_atlas,
                    self.width,
                    self.height,
                );
            }
        }

        // Render typing speed indicator
        if self.typing_speed_enabled {
            let now = std::time::Instant::now();
            let window_secs = 5.0_f64;
            // Remove key presses older than the window
            self.key_press_times.retain(|t| now.duration_since(*t).as_secs_f64() < window_secs);
            // Calculate chars/second, then WPM (5 chars per word, * 60 for minutes)
            let count = self.key_press_times.len() as f64;
            let target_wpm = if count > 1.0 {
                let span = now.duration_since(self.key_press_times[0]).as_secs_f64();
                if span > 0.1 {
                    (count / span) * 60.0 / 5.0
                } else {
                    0.0
                }
            } else {
                0.0
            };
            // Exponential smoothing
            let alpha = 0.15_f32;
            self.displayed_wpm += (target_wpm as f32 - self.displayed_wpm) * alpha;
            if self.displayed_wpm < 0.5 { self.displayed_wpm = 0.0; }

            if let (Some(ref renderer), Some(ref mut glyph_atlas), Some(ref frame)) =
                (&self.renderer, &mut self.glyph_atlas, &self.current_frame)
            {
                renderer.render_typing_speed(&surface_view, frame, glyph_atlas, self.displayed_wpm);
            }
            // Keep redrawing while WPM is decaying
            if self.displayed_wpm > 0.5 || !self.key_press_times.is_empty() {
                self.frame_dirty = true;
            }
        }

        // Render corner mask for rounded window corners (borderless only, not fullscreen)
        if !self.decorations_enabled && !self.is_fullscreen && self.corner_radius > 0.0 {
            if let Some(ref renderer) = self.renderer {
                renderer.render_corner_mask(
                    &surface_view,
                    self.corner_radius,
                    self.width,
                    self.height,
                );
            }
        }

        // Present the frame
        output.present();
    }

    /// Set the window icon from the embedded Emacs icon PNG.
    fn set_window_icon(window: &Window) {
        let icon_bytes = include_bytes!("../../../etc/images/icons/hicolor/128x128/apps/emacs.png");
        if let Ok(img) = image::load_from_memory(icon_bytes) {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            if let Ok(icon) = winit::window::Icon::from_rgba(rgba.into_raw(), w, h) {
                window.set_window_icon(Some(icon));
            }
        }
    }

    /// Translate winit key to X11 keysym
    fn translate_key(key: &Key) -> u32 {
        match key {
            Key::Named(named) => match named {
                // Function keys
                NamedKey::F1 => 0xffbe,
                NamedKey::F2 => 0xffbf,
                NamedKey::F3 => 0xffc0,
                NamedKey::F4 => 0xffc1,
                NamedKey::F5 => 0xffc2,
                NamedKey::F6 => 0xffc3,
                NamedKey::F7 => 0xffc4,
                NamedKey::F8 => 0xffc5,
                NamedKey::F9 => 0xffc6,
                NamedKey::F10 => 0xffc7,
                NamedKey::F11 => 0xffc8,
                NamedKey::F12 => 0xffc9,
                // Navigation
                NamedKey::Escape => 0xff1b,
                NamedKey::Enter => 0xff0d,
                NamedKey::Tab => 0xff09,
                NamedKey::Backspace => 0xff08,
                NamedKey::Delete => 0xffff,
                NamedKey::Insert => 0xff63,
                NamedKey::Home => 0xff50,
                NamedKey::End => 0xff57,
                NamedKey::PageUp => 0xff55,
                NamedKey::PageDown => 0xff56,
                NamedKey::ArrowLeft => 0xff51,
                NamedKey::ArrowUp => 0xff52,
                NamedKey::ArrowRight => 0xff53,
                NamedKey::ArrowDown => 0xff54,
                // Whitespace
                NamedKey::Space => 0x20,
                // Modifier keys are handled via ModifiersChanged, not as key events.
                // They fall through to the default `_ => 0` which suppresses them.
                // Other
                NamedKey::PrintScreen => 0xff61,
                NamedKey::ScrollLock => 0xff14,
                NamedKey::Pause => 0xff13,
                _ => 0,
            },
            Key::Character(c) => {
                c.chars().next().map(|ch| ch as u32).unwrap_or(0)
            }
            _ => 0,
        }
    }

    /// Detect if the mouse is on a resize edge of a borderless window.
    /// Returns the resize direction if within the border zone, or None.
    fn detect_resize_edge(
        &self,
        x: f32,
        y: f32,
    ) -> Option<winit::window::ResizeDirection> {
        use winit::window::ResizeDirection;
        if self.decorations_enabled {
            return None;
        }
        let w = self.width as f32;
        let h = self.height as f32;
        let border = 5.0_f32;
        let on_left = x < border;
        let on_right = x >= w - border;
        let on_top = y < border;
        let on_bottom = y >= h - border;
        match (on_left, on_right, on_top, on_bottom) {
            (true, _, true, _) => Some(ResizeDirection::NorthWest),
            (_, true, true, _) => Some(ResizeDirection::NorthEast),
            (true, _, _, true) => Some(ResizeDirection::SouthWest),
            (_, true, _, true) => Some(ResizeDirection::SouthEast),
            (true, _, _, _) => Some(ResizeDirection::West),
            (_, true, _, _) => Some(ResizeDirection::East),
            (_, _, true, _) => Some(ResizeDirection::North),
            (_, _, _, true) => Some(ResizeDirection::South),
            _ => None,
        }
    }

    /// Title bar button width in logical pixels.
    const TITLEBAR_BUTTON_WIDTH: f32 = 46.0;

    /// Check if a point is in the custom title bar area.
    /// Returns: 0 = not in title bar, 1 = drag area, 2 = close, 3 = maximize, 4 = minimize
    fn titlebar_hit_test(&self, x: f32, y: f32) -> u32 {
        if self.decorations_enabled || self.is_fullscreen || self.custom_titlebar_height <= 0.0 {
            return 0;
        }
        let w = self.width as f32 / self.scale_factor as f32;
        let tb_h = self.custom_titlebar_height;
        if y >= tb_h {
            return 0; // Below title bar
        }
        // Buttons are on the right: [minimize] [maximize] [close]
        let btn_w = Self::TITLEBAR_BUTTON_WIDTH;
        let close_x = w - btn_w;
        let max_x = w - btn_w * 2.0;
        let min_x = w - btn_w * 3.0;
        if x >= close_x {
            2 // Close
        } else if x >= max_x {
            3 // Maximize
        } else if x >= min_x {
            4 // Minimize
        } else {
            1 // Drag area
        }
    }
}

impl ApplicationHandler for RenderApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            // Use LogicalSize so winit applies the display scale
            let attrs = Window::default_attributes()
                .with_title(&self.title)
                .with_inner_size(winit::dpi::LogicalSize::new(self.width, self.height))
                .with_transparent(true);

            match event_loop.create_window(attrs) {
                Ok(window) => {
                    let window = Arc::new(window);

                    // Read scale factor once at launch
                    self.scale_factor = window.scale_factor();
                    log::info!("Display scale factor: {}", self.scale_factor);

                    // Update width/height to physical pixels for surface config
                    let phys = window.inner_size();
                    self.width = phys.width;
                    self.height = phys.height;
                    log::info!("Render thread: window created (physical {}x{})", self.width, self.height);

                    // Initialize wgpu with the window
                    self.init_wgpu(window.clone());

                    // Enable IME input for CJK and compose support
                    window.set_ime_allowed(true);

                    // Set window icon from embedded Emacs icon
                    Self::set_window_icon(&window);

                    self.window = Some(window);
                }
                Err(e) => {
                    log::error!("Failed to create window: {:?}", e);
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                log::info!("Window close requested");
                self.comms.send_input(InputEvent::WindowClose);
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                log::info!("WindowEvent::Resized: {}x{}", size.width, size.height);

                // Handle wgpu surface resize
                self.handle_resize(size.width, size.height);

                // Notify Emacs of the resize in logical pixels
                let logical_w = (size.width as f64 / self.scale_factor) as u32;
                let logical_h = (size.height as f64 / self.scale_factor) as u32;
                log::info!("Sending WindowResize event to Emacs: {}x{} (logical)", logical_w, logical_h);
                self.comms.send_input(InputEvent::WindowResize {
                    width: logical_w,
                    height: logical_h,
                });
            }

            WindowEvent::Focused(focused) => {
                self.comms.send_input(InputEvent::WindowFocus { focused });
            }

            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key, state, ..
                    },
                ..
            } => {
                // If popup menu is active, handle keyboard navigation
                if self.popup_menu.is_some() && state == ElementState::Pressed {
                    match logical_key.as_ref() {
                        Key::Named(NamedKey::Escape) => {
                            self.comms.send_input(InputEvent::MenuSelection { index: -1 });
                            self.popup_menu = None;
                            self.frame_dirty = true;
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            if let Some(ref mut menu) = self.popup_menu {
                                if menu.move_hover(1) {
                                    self.frame_dirty = true;
                                }
                            }
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            if let Some(ref mut menu) = self.popup_menu {
                                if menu.move_hover(-1) {
                                    self.frame_dirty = true;
                                }
                            }
                        }
                        Key::Named(NamedKey::Enter) => {
                            if let Some(ref mut menu) = self.popup_menu {
                                let panel = menu.active_panel();
                                let hi = panel.hover_index;
                                if hi >= 0 && (hi as usize) < panel.item_indices.len() {
                                    let global_idx = panel.item_indices[hi as usize];
                                    if menu.all_items[global_idx].submenu {
                                        // Open submenu instead of selecting
                                        if menu.open_submenu() {
                                            self.frame_dirty = true;
                                        }
                                    } else {
                                        self.comms.send_input(InputEvent::MenuSelection { index: global_idx as i32 });
                                        self.popup_menu = None;
                                        self.frame_dirty = true;
                                    }
                                } else {
                                    self.comms.send_input(InputEvent::MenuSelection { index: -1 });
                                    self.popup_menu = None;
                                    self.frame_dirty = true;
                                }
                            }
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            if let Some(ref mut menu) = self.popup_menu {
                                if menu.open_submenu() {
                                    self.frame_dirty = true;
                                }
                            }
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            if let Some(ref mut menu) = self.popup_menu {
                                if menu.close_submenu() {
                                    self.frame_dirty = true;
                                }
                            }
                        }
                        Key::Named(NamedKey::Home) => {
                            if let Some(ref mut menu) = self.popup_menu {
                                menu.active_panel_mut().hover_index = -1;
                                if menu.move_hover(1) {
                                    self.frame_dirty = true;
                                }
                            }
                        }
                        Key::Named(NamedKey::End) => {
                            if let Some(ref mut menu) = self.popup_menu {
                                let len = menu.active_panel().item_indices.len() as i32;
                                menu.active_panel_mut().hover_index = len;
                                if menu.move_hover(-1) {
                                    self.frame_dirty = true;
                                }
                            }
                        }
                        _ => {} // Swallow other keys
                    }
                } else if self.ime_preedit_active {
                    // When IME preedit is active, suppress character
                    // keys to avoid double input.  The committed text
                    // will arrive via Ime::Commit instead.
                } else {
                    let keysym = Self::translate_key(&logical_key);
                    if keysym != 0 {
                        // Hide mouse cursor on keyboard input
                        if state == ElementState::Pressed && !self.mouse_hidden_for_typing {
                            if let Some(ref window) = self.window {
                                window.set_cursor_visible(false);
                                self.mouse_hidden_for_typing = true;
                            }
                        }
                        // Track key presses for typing speed indicator
                        if self.typing_speed_enabled && state == ElementState::Pressed {
                            self.key_press_times.push(std::time::Instant::now());
                        }
                        // Track activity for idle dimming
                        if self.idle_dim_enabled {
                            self.last_activity_time = std::time::Instant::now();
                        }
                        self.comms.send_input(InputEvent::Key {
                            keysym,
                            modifiers: self.modifiers,
                            pressed: state == ElementState::Pressed,
                        });
                    }
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                // If popup menu is active, handle clicks for it
                if let Some(ref mut menu) = self.popup_menu {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        let idx = menu.hit_test(self.mouse_pos.0, self.mouse_pos.1);
                        if idx >= 0 {
                            // Regular item selected
                            self.comms.send_input(InputEvent::MenuSelection { index: idx });
                            self.popup_menu = None;
                            self.frame_dirty = true;
                        } else {
                            // Check if click is on a submenu item (which hit_test returns -1 for)
                            let (depth, local_idx) = menu.hit_test_all(self.mouse_pos.0, self.mouse_pos.1);
                            if depth >= 0 && local_idx >= 0 {
                                let panel = if depth == 0 {
                                    &menu.root_panel
                                } else {
                                    &menu.submenu_panels[(depth - 1) as usize]
                                };
                                let global_idx = panel.item_indices[local_idx as usize];
                                if menu.all_items[global_idx].submenu {
                                    // Clicked a submenu item — keep menu open, submenu auto-opened on hover
                                    self.frame_dirty = true;
                                } else {
                                    // Clicked outside or on a disabled item — cancel
                                    self.comms.send_input(InputEvent::MenuSelection { index: -1 });
                                    self.popup_menu = None;
                                    self.frame_dirty = true;
                                }
                            } else {
                                // Clicked outside all panels — cancel
                                self.comms.send_input(InputEvent::MenuSelection { index: -1 });
                                self.popup_menu = None;
                                self.frame_dirty = true;
                            }
                        }
                    } else if state == ElementState::Pressed {
                        // Any other button cancels the menu
                        self.comms.send_input(InputEvent::MenuSelection { index: -1 });
                        self.popup_menu = None;
                        self.frame_dirty = true;
                    }
                } else if state == ElementState::Pressed
                    && button == MouseButton::Left
                    && self.resize_edge.is_some()
                {
                    // Borderless: initiate window resize drag
                    if let (Some(dir), Some(ref window)) =
                        (self.resize_edge, self.window.as_ref())
                    {
                        let _ = window.drag_resize_window(dir);
                    }
                } else if state == ElementState::Pressed
                    && button == MouseButton::Left
                    && self.titlebar_hit_test(self.mouse_pos.0, self.mouse_pos.1) > 0
                {
                    // Custom title bar click
                    match self.titlebar_hit_test(self.mouse_pos.0, self.mouse_pos.1) {
                        1 => {
                            // Drag area: double-click toggles maximize
                            let now = std::time::Instant::now();
                            if now.duration_since(self.last_titlebar_click).as_millis() < 400 {
                                if let Some(ref window) = self.window {
                                    window.set_maximized(!window.is_maximized());
                                }
                            } else if let Some(ref window) = self.window {
                                let _ = window.drag_window();
                            }
                            self.last_titlebar_click = now;
                        }
                        2 => {
                            // Close button
                            self.comms.send_input(InputEvent::WindowClose);
                        }
                        3 => {
                            // Maximize/restore toggle
                            if let Some(ref window) = self.window {
                                if window.is_maximized() {
                                    window.set_maximized(false);
                                } else {
                                    window.set_maximized(true);
                                }
                            }
                        }
                        4 => {
                            // Minimize
                            if let Some(ref window) = self.window {
                                window.set_minimized(true);
                            }
                        }
                        _ => {}
                    }
                } else if state == ElementState::Pressed
                    && button == MouseButton::Left
                    && !self.decorations_enabled
                    && (self.modifiers & NEOMACS_SUPER_MASK) != 0
                {
                    // Borderless: Super+click to drag-move window
                    if let Some(ref window) = self.window {
                        let _ = window.drag_window();
                    }
                } else {
                    let btn = match button {
                        MouseButton::Left => 1,
                        MouseButton::Middle => 2,
                        MouseButton::Right => 3,
                        MouseButton::Back => 4,
                        MouseButton::Forward => 5,
                        MouseButton::Other(n) => n as u32,
                    };
                    self.comms.send_input(InputEvent::MouseButton {
                        button: btn,
                        x: self.mouse_pos.0,
                        y: self.mouse_pos.1,
                        pressed: state == ElementState::Pressed,
                        modifiers: self.modifiers,
                    });
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                // Convert to logical pixels for Emacs
                let lx = (position.x / self.scale_factor) as f32;
                let ly = (position.y / self.scale_factor) as f32;
                self.mouse_pos = (lx, ly);
                // Track activity for idle dimming
                if self.idle_dim_enabled {
                    self.last_activity_time = std::time::Instant::now();
                }

                // Restore mouse cursor visibility when mouse moves
                if self.mouse_hidden_for_typing {
                    if let Some(ref window) = self.window {
                        window.set_cursor_visible(true);
                    }
                    self.mouse_hidden_for_typing = false;
                }

                // Borderless resize edge detection
                let edge = self.detect_resize_edge(lx, ly);
                if edge != self.resize_edge {
                    self.resize_edge = edge;
                    if let Some(ref window) = self.window {
                        use winit::window::CursorIcon;
                        let icon = match edge {
                            Some(dir) => CursorIcon::from(dir),
                            None => CursorIcon::Default,
                        };
                        window.set_cursor(icon);
                    }
                }

                // Update title bar hover state and cursor
                if !self.decorations_enabled {
                    let new_hover = self.titlebar_hit_test(lx, ly);
                    if new_hover != self.titlebar_hover {
                        self.titlebar_hover = new_hover;
                        self.frame_dirty = true;
                        // Set cursor icon based on title bar region
                        if self.resize_edge.is_none() {
                            if let Some(ref window) = self.window {
                                use winit::window::CursorIcon;
                                let icon = match new_hover {
                                    2 | 3 | 4 => CursorIcon::Pointer, // buttons
                                    _ => CursorIcon::Default,
                                };
                                window.set_cursor(icon);
                            }
                        }
                    }
                }

                // Update popup menu hover state (multi-panel)
                if let Some(ref mut menu) = self.popup_menu {
                    let (hit_depth, hit_local) = menu.hit_test_all(lx, ly);
                    if hit_depth >= 0 {
                        // Close deeper submenus if hovering on a shallower panel
                        let target_depth = hit_depth as usize;
                        while menu.submenu_panels.len() > target_depth {
                            menu.submenu_panels.pop();
                            self.frame_dirty = true;
                        }
                        // Update hover in the target panel
                        let panel = if target_depth == 0 {
                            &mut menu.root_panel
                        } else {
                            &mut menu.submenu_panels[target_depth - 1]
                        };
                        if hit_local != panel.hover_index {
                            panel.hover_index = hit_local;
                            self.frame_dirty = true;
                            // Auto-open submenu on hover
                            if hit_local >= 0 && (hit_local as usize) < panel.item_indices.len() {
                                let global_idx = panel.item_indices[hit_local as usize];
                                if menu.all_items[global_idx].submenu {
                                    menu.open_submenu();
                                }
                            }
                        }
                    }
                } else {
                    self.comms.send_input(InputEvent::MouseMove {
                        x: lx,
                        y: ly,
                        modifiers: self.modifiers,
                    });
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy, pixel_precise) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => {
                        (x, y, false)
                    }
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        // Pass raw logical pixel deltas for touchpad
                        ((pos.x / self.scale_factor) as f32,
                         (pos.y / self.scale_factor) as f32,
                         true)
                    }
                };
                self.comms.send_input(InputEvent::MouseScroll {
                    delta_x: dx,
                    delta_y: dy,
                    x: self.mouse_pos.0,
                    y: self.mouse_pos.1,
                    modifiers: self.modifiers,
                    pixel_precise,
                });
            }

            WindowEvent::RedrawRequested => {
                self.render();
                self.frame_dirty = false;
            }

            WindowEvent::ModifiersChanged(mods) => {
                let state = mods.state();
                self.modifiers = 0;
                if state.shift_key() {
                    self.modifiers |= NEOMACS_SHIFT_MASK;
                }
                if state.control_key() {
                    self.modifiers |= NEOMACS_CTRL_MASK;
                }
                if state.alt_key() {
                    self.modifiers |= NEOMACS_META_MASK;
                }
                if state.super_key() {
                    self.modifiers |= NEOMACS_SUPER_MASK;
                }
            }

            WindowEvent::Ime(ime_event) => {
                match ime_event {
                    winit::event::Ime::Enabled => {
                        self.ime_enabled = true;
                        log::debug!("IME enabled");
                    }
                    winit::event::Ime::Disabled => {
                        self.ime_enabled = false;
                        log::debug!("IME disabled");
                    }
                    winit::event::Ime::Commit(text) => {
                        // Send each committed character as an individual
                        // key event to Emacs (no modifiers — IME already
                        // composed the final characters)
                        for ch in text.chars() {
                            let keysym = ch as u32;
                            if keysym != 0 {
                                self.comms.send_input(InputEvent::Key {
                                    keysym,
                                    modifiers: 0,
                                    pressed: true,
                                });
                            }
                        }
                    }
                    winit::event::Ime::Preedit(text, cursor_range) => {
                        // Track whether preedit is active to suppress
                        // raw KeyboardInput during IME composition
                        self.ime_preedit_active = !text.is_empty();
                        self.ime_preedit_text = text.clone();

                        // Update IME cursor area so the OS positions the
                        // candidate window near the text cursor
                        if let Some(ref window) = self.window {
                            if let Some(ref target) = self.cursor_target {
                                let x = (target.x as f64) * self.scale_factor;
                                let y = (target.y as f64 + target.height as f64) * self.scale_factor;
                                let w = target.width as f64 * self.scale_factor;
                                let h = target.height as f64 * self.scale_factor;
                                window.set_ime_cursor_area(
                                    winit::dpi::PhysicalPosition::new(x, y),
                                    winit::dpi::PhysicalSize::new(w, h),
                                );
                            }
                        }
                        if !text.is_empty() {
                            log::trace!("IME preedit: '{}' cursor: {:?}", text, cursor_range);
                        }
                        self.frame_dirty = true;
                    }
                }
            }

            WindowEvent::DroppedFile(path) => {
                if let Some(path_str) = path.to_str() {
                    log::info!("File dropped: {}", path_str);
                    self.comms.send_input(InputEvent::FileDrop {
                        paths: vec![path_str.to_string()],
                        x: self.mouse_pos.0,
                        y: self.mouse_pos.1,
                    });
                }
            }

            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                log::info!("Scale factor changed: {} -> {}", self.scale_factor, scale_factor);
                self.scale_factor = scale_factor;
                // Update renderer's scale factor
                if let Some(ref mut renderer) = self.renderer {
                    renderer.set_scale_factor(scale_factor as f32);
                }
                // Clear glyph atlas so text re-rasterizes at new DPI
                if let Some(ref mut atlas) = self.glyph_atlas {
                    atlas.set_scale_factor(scale_factor as f32);
                }
                self.frame_dirty = true;
                // The Resized event will follow, which handles surface reconfiguration
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Check for shutdown
        if self.process_commands() {
            event_loop.exit();
            return;
        }

        // Get latest frame from Emacs
        self.poll_frame();

        // Pump GLib for WebKit
        self.pump_glib();

        // Update cursor blink state
        if self.tick_cursor_blink() {
            self.frame_dirty = true;
        }

        // Tick cursor animation
        if self.tick_cursor_animation() {
            self.frame_dirty = true;
        }

        // Tick cursor size transition (runs after position animation, overrides w/h)
        if self.tick_cursor_size_animation() {
            self.frame_dirty = true;
        }

        // Tick idle dimming
        if self.idle_dim_enabled {
            let idle_time = self.last_activity_time.elapsed();
            let target_alpha = if idle_time >= self.idle_dim_delay {
                self.idle_dim_opacity
            } else {
                0.0
            };
            let diff = target_alpha - self.idle_dim_current_alpha;
            if diff.abs() > 0.001 {
                let fade_speed = if self.idle_dim_fade_duration.as_secs_f32() > 0.0 {
                    1.0 / self.idle_dim_fade_duration.as_secs_f32() * 0.016
                } else {
                    1.0
                };
                if diff > 0.0 {
                    self.idle_dim_current_alpha = (self.idle_dim_current_alpha + fade_speed * self.idle_dim_opacity).min(target_alpha);
                } else {
                    self.idle_dim_current_alpha = (self.idle_dim_current_alpha - fade_speed * self.idle_dim_opacity).max(0.0);
                }
                self.idle_dim_active = true;
                self.frame_dirty = true;
            } else if self.idle_dim_current_alpha > 0.001 {
                self.idle_dim_active = true;
                self.frame_dirty = true;
            } else {
                self.idle_dim_active = false;
            }
        }

        // Keep dirty if cursor pulse is active (needs continuous redraw)
        if self.cursor_pulse_enabled && self.cursor_glow_enabled {
            self.frame_dirty = true;
        }

        // Keep dirty if renderer signals need for continuous redraws (dim fade)
        if let Some(ref renderer) = self.renderer {
            if renderer.needs_continuous_redraw {
                self.frame_dirty = true;
            }
        }

        // Keep dirty if transitions are active
        if self.has_active_transitions() {
            self.frame_dirty = true;
        }

        // Check for terminal PTY activity
        if self.has_terminal_activity() {
            self.frame_dirty = true;
        }

        // Determine if continuous rendering is needed
        let has_active_content = self.has_webkit_needing_redraw() || self.has_playing_videos();

        // Request redraw when we have new frame data, cursor blink toggled,
        // or webkit/video content changed
        if self.frame_dirty || has_active_content {
            if let Some(ref window) = self.window {
                window.request_redraw();
            }
        }

        // Use WaitUntil with smart timeouts instead of Poll to save CPU.
        // Window events (key, mouse, resize) still wake immediately.
        let now = std::time::Instant::now();
        let next_wake = if self.frame_dirty || has_active_content
            || self.cursor_animating || self.cursor_size_animating
            || self.idle_dim_active || self.has_active_transitions()
        {
            // Active rendering: cap at ~240fps to avoid spinning
            now + std::time::Duration::from_millis(4)
        } else if self.cursor_blink_enabled {
            // Idle with cursor blink: wake at next toggle time
            self.last_cursor_toggle + self.cursor_blink_interval
        } else {
            // Fully idle: poll for new Emacs frames at 60fps
            now + std::time::Duration::from_millis(16)
        };
        event_loop.set_control_flow(ControlFlow::WaitUntil(next_wake));
    }
}

/// Run the render loop (called on render thread)
fn run_render_loop(
    comms: RenderComms,
    width: u32,
    height: u32,
    title: String,
    image_dimensions: SharedImageDimensions,
    #[cfg(feature = "neo-term")]
    shared_terminals: crate::terminal::SharedTerminals,
) {
    log::info!("Render thread starting");

    // CRITICAL: Set up a dedicated GMainContext for WebKit before any WebKit initialization.
    // This ensures WebKit attaches its GLib sources (IPC sockets, etc.) to this context,
    // not the default context. Only the render thread will dispatch events from this context,
    // preventing the Emacs main thread's xg_select from dispatching WebKit callbacks.
    #[cfg(all(feature = "wpe-webkit", wpe_platform_available))]
    let webkit_main_context = unsafe {
        let ctx = plat::g_main_context_new();
        if !ctx.is_null() {
            // Acquire the context so we can dispatch on it
            plat::g_main_context_acquire(ctx);
            // Push as thread-default - WebKit will attach sources here
            plat::g_main_context_push_thread_default(ctx);
            log::info!("Created dedicated GMainContext for WebKit: {:?}", ctx);
        } else {
            log::warn!("Failed to create dedicated GMainContext for WebKit");
        }
        ctx
    };

    // Use any_thread() since we're running on a non-main thread
    #[cfg(target_os = "linux")]
    let event_loop = {
        let mut builder = EventLoopBuilder::new();
        // Try Wayland first, fall back to X11
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            EventLoopBuilderExtWayland::with_any_thread(&mut builder, true);
        } else {
            EventLoopBuilderExtX11::with_any_thread(&mut builder, true);
        }
        builder.build().expect("Failed to create event loop")
    };
    #[cfg(not(target_os = "linux"))]
    let event_loop = EventLoop::new().expect("Failed to create event loop");

    // Start with WaitUntil to avoid busy-polling; about_to_wait() adjusts dynamically
    event_loop.set_control_flow(ControlFlow::WaitUntil(
        std::time::Instant::now() + std::time::Duration::from_millis(16),
    ));

    let mut app = RenderApp::new(
        comms, width, height, title, image_dimensions,
        #[cfg(feature = "neo-term")]
        shared_terminals,
    );

    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("Event loop error: {:?}", e);
    }

    log::info!("Render thread exiting");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thread_comm::ThreadComms;

    #[test]
    fn test_translate_key_named() {
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Escape)), 0xff1b);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Enter)), 0xff0d);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Tab)), 0xff09);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Backspace)), 0xff08);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Delete)), 0xffff);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Home)), 0xff50);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::End)), 0xff57);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::PageUp)), 0xff55);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::PageDown)), 0xff56);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::ArrowLeft)), 0xff51);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::ArrowUp)), 0xff52);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::ArrowRight)), 0xff53);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::ArrowDown)), 0xff54);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Space)), 0x20);
    }

    #[test]
    fn test_translate_key_character() {
        assert_eq!(
            RenderApp::translate_key(&Key::Character("a".into())),
            'a' as u32
        );
        assert_eq!(
            RenderApp::translate_key(&Key::Character("A".into())),
            'A' as u32
        );
        assert_eq!(
            RenderApp::translate_key(&Key::Character("1".into())),
            '1' as u32
        );
    }

    #[test]
    fn test_translate_key_function_keys() {
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::F1)), 0xffbe);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::F12)), 0xffc9);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Insert)), 0xff63);
        assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::PrintScreen)), 0xff61);
    }

    #[test]
    fn test_translate_key_unknown() {
        // Unknown named keys should return 0
        assert_eq!(RenderApp::translate_key(&Key::Dead(None)), 0);
    }

    #[test]
    fn test_render_thread_creation() {
        // Just test that ThreadComms can be created and split
        let comms = ThreadComms::new().expect("Failed to create ThreadComms");
        let (emacs, render) = comms.split();

        // Verify we can access the channels
        assert!(emacs.input_rx.is_empty());
        assert!(render.cmd_rx.is_empty());
    }
}
