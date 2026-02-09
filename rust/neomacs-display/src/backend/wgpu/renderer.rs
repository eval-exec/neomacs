//! wgpu GPU-accelerated scene renderer.

use std::collections::HashMap;
use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::core::face::{BoxType, Face, FaceAttributes};
use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer};
use crate::core::scene::{CursorStyle, Scene};
use crate::core::types::{AnimatedCursor, Color, Rect};

use super::glyph_atlas::{GlyphKey, WgpuGlyphAtlas};
use super::image_cache::ImageCache;
#[cfg(feature = "video")]
use super::video_cache::VideoCache;
#[cfg(feature = "wpe-webkit")]
use super::webkit_cache::WgpuWebKitCache;
use super::vertex::{GlyphVertex, RectVertex, RoundedRectVertex, Uniforms};

/// GPU-accelerated renderer using wgpu.
pub struct WgpuRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    surface_format: wgpu::TextureFormat,
    rect_pipeline: wgpu::RenderPipeline,
    rounded_rect_pipeline: wgpu::RenderPipeline,
    corner_mask_pipeline: wgpu::RenderPipeline,
    glyph_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    opaque_image_pipeline: wgpu::RenderPipeline,
    glyph_bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    image_cache: ImageCache,
    #[cfg(feature = "video")]
    video_cache: VideoCache,
    #[cfg(feature = "wpe-webkit")]
    webkit_cache: WgpuWebKitCache,
    width: u32,
    height: u32,
    /// Display scale factor (physical pixels / logical pixels)
    scale_factor: f32,
    /// Scroll bar thumb corner radius ratio (0.0-1.0)
    scroll_bar_thumb_radius: f32,
    /// Scroll bar track opacity (0.0-1.0)
    scroll_bar_track_opacity: f32,
    /// Scroll bar hover brightness multiplier
    scroll_bar_hover_brightness: f32,
    /// Indent guide rendering enabled
    indent_guides_enabled: bool,
    /// Indent guide color (linear RGBA)
    indent_guide_color: (f32, f32, f32, f32),
    /// Rainbow indent guide enabled (overrides single color when active)
    indent_guide_rainbow_enabled: bool,
    /// Rainbow indent guide colors (linear RGBA, cycling by depth)
    indent_guide_rainbow_colors: Vec<(f32, f32, f32, f32)>,
    /// Current line highlight enabled
    line_highlight_enabled: bool,
    /// Current line highlight color (linear RGBA)
    line_highlight_color: (f32, f32, f32, f32),
    /// Visible whitespace enabled
    show_whitespace_enabled: bool,
    /// Visible whitespace color (linear RGBA)
    show_whitespace_color: (f32, f32, f32, f32),
    /// Inactive window dimming enabled
    inactive_dim_enabled: bool,
    /// Inactive window dimming opacity
    inactive_dim_opacity: f32,
    /// Per-window dim opacity for smooth fade transitions
    per_window_dim: std::collections::HashMap<i64, f32>,
    /// Last dim update time for smooth interpolation
    last_dim_tick: std::time::Instant,
    /// Flag: renderer needs continuous redraws (e.g. dim fade in progress)
    pub needs_continuous_redraw: bool,
    /// Mode-line separator style (0=none, 1=line, 2=shadow, 3=gradient)
    mode_line_separator_style: u32,
    /// Mode-line separator color (linear RGB)
    mode_line_separator_color: (f32, f32, f32),
    /// Mode-line separator height in pixels
    mode_line_separator_height: f32,
    /// Cursor glow enabled
    cursor_glow_enabled: bool,
    /// Cursor glow color (linear RGB)
    cursor_glow_color: (f32, f32, f32),
    /// Cursor glow radius
    cursor_glow_radius: f32,
    /// Cursor glow peak opacity
    cursor_glow_opacity: f32,
    /// Cursor pulse enabled (sinusoidal glow modulation)
    cursor_pulse_enabled: bool,
    /// Cursor pulse speed in Hz
    cursor_pulse_speed: f32,
    /// Cursor pulse minimum opacity multiplier (0.0-1.0)
    cursor_pulse_min_opacity: f32,
    /// Start time for pulse phase calculation
    cursor_pulse_start: std::time::Instant,
    /// Focus mode enabled (dim outside current paragraph)
    focus_mode_enabled: bool,
    /// Focus mode dimming opacity
    focus_mode_opacity: f32,
    /// Minimap enabled
    minimap_enabled: bool,
    /// Minimap column width in logical pixels
    minimap_width: f32,
    /// Typing ripple effect enabled
    typing_ripple_enabled: bool,
    /// Max ripple radius in pixels
    typing_ripple_max_radius: f32,
    /// Ripple duration in seconds
    typing_ripple_duration: f32,
    /// Active ripples: (center_x, center_y, spawn_instant)
    active_ripples: Vec<(f32, f32, std::time::Instant)>,
    /// Search highlight pulse enabled
    search_pulse_enabled: bool,
    /// Face ID of the isearch face
    search_pulse_face_id: u32,
    /// Pulse start time
    search_pulse_start: std::time::Instant,
    /// Vignette effect
    vignette_enabled: bool,
    vignette_intensity: f32,
    vignette_radius: f32,
    /// Zen mode (distraction-free centered content)
    zen_mode_enabled: bool,
    zen_mode_content_width_pct: f32,
    zen_mode_margin_opacity: f32,
    /// Background pattern style (0=none, 1=dots, 2=grid, 3=crosshatch)
    bg_pattern_style: u32,
    /// Pattern spacing in pixels
    bg_pattern_spacing: f32,
    /// Pattern color (sRGB)
    bg_pattern_color: (f32, f32, f32),
    /// Pattern opacity
    bg_pattern_opacity: f32,
    /// Line insertion/deletion animation
    line_animation_enabled: bool,
    line_animation_duration_ms: u32,
    active_line_anims: Vec<LineAnimEntry>,
    /// Header/mode-line shadow depth effect
    header_shadow_enabled: bool,
    header_shadow_intensity: f32,
    header_shadow_size: f32,
    /// Cursor color cycling (rainbow hue rotation)
    cursor_color_cycle_enabled: bool,
    cursor_color_cycle_speed: f32,
    cursor_color_cycle_saturation: f32,
    cursor_color_cycle_lightness: f32,
    cursor_color_cycle_start: std::time::Instant,
    /// Window switch highlight fade
    window_switch_fade_enabled: bool,
    window_switch_fade_duration_ms: u32,
    window_switch_fade_intensity: f32,
    /// Active window switch fades: (window_id, bounds, started, duration, intensity)
    active_window_fades: Vec<WindowFadeEntry>,
    /// Breadcrumb/path bar overlay
    breadcrumb_enabled: bool,
    breadcrumb_opacity: f32,
    /// Frosted glass effect on mode-lines
    /// Smooth border color transition
    border_transition_enabled: bool,
    border_transition_active_color: (f32, f32, f32),
    border_transition_duration: std::time::Duration,
    /// Per-window border transition state: (window_id, is_becoming_active, start_time)
    border_transitions: Vec<(i64, bool, std::time::Instant)>,
    /// Previous selected window for border transition detection
    prev_border_selected: i64,
    /// Buffer-local accent color strip
    accent_strip_enabled: bool,
    accent_strip_width: f32,
    /// Cursor trail fade (afterimage)
    cursor_trail_fade_enabled: bool,
    cursor_trail_fade_length: usize,
    cursor_trail_fade_duration: std::time::Duration,
    cursor_trail_positions: Vec<(f32, f32, f32, f32, std::time::Instant)>,
    cursor_trail_last_pos: (f32, f32),
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
    focus_ring_start: std::time::Instant,
    /// Window background tint based on file type
    window_mode_tint_enabled: bool,
    window_mode_tint_opacity: f32,
    /// Window watermark for empty buffers
    window_watermark_enabled: bool,
    window_watermark_opacity: f32,
    window_watermark_threshold: u32,
    /// Selection region glow
    region_glow_enabled: bool,
    region_glow_face_id: u32,
    region_glow_radius: f32,
    region_glow_opacity: f32,
    /// Idle screen dimming alpha (0.0 = no dim, >0 = overlay)
    idle_dim_alpha: f32,
    /// Noise/film grain overlay
    noise_grain_enabled: bool,
    noise_grain_intensity: f32,
    noise_grain_size: f32,
    noise_grain_frame: u32,
    /// Window padding gradient (inner edge shading)
    padding_gradient_enabled: bool,
    padding_gradient_color: (f32, f32, f32),
    padding_gradient_opacity: f32,
    padding_gradient_width: f32,
    frosted_glass_enabled: bool,
    frosted_glass_opacity: f32,
    frosted_glass_blur: f32,
    /// Title fade animation
    title_fade_enabled: bool,
    title_fade_duration_ms: u32,
    /// Previous breadcrumb text per window (window_id -> file_name)
    prev_breadcrumb_text: std::collections::HashMap<i64, String>,
    /// Active title fades (window_id -> (old_text, new_text, start_time))
    active_title_fades: Vec<TitleFadeEntry>,
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
    /// Mode-line content transition
    mode_line_transition_enabled: bool,
    mode_line_transition_duration_ms: u32,
    /// Per-window mode-line content hash for change detection
    prev_mode_line_hashes: std::collections::HashMap<i64, u64>,
    /// Active mode-line transition fades
    active_mode_line_fades: Vec<ModeLineFadeEntry>,
    /// Text fade-in animation
    text_fade_in_enabled: bool,
    text_fade_in_duration_ms: u32,
    /// Active text fade-in animations per window
    active_text_fades: Vec<TextFadeEntry>,
    /// Scroll line spacing animation (accordion effect)
    scroll_line_spacing_enabled: bool,
    scroll_line_spacing_max: f32,
    scroll_line_spacing_duration_ms: u32,
    /// Active scroll line spacing animations: (window_id, bounds, direction, started)
    active_scroll_spacings: Vec<ScrollSpacingEntry>,
    /// Cursor wake animation (pop/scale effect on blink-on)
    cursor_wake_enabled: bool,
    cursor_wake_duration_ms: u32,
    cursor_wake_scale: f32,
    /// Timestamp of last cursor wake trigger
    cursor_wake_started: Option<std::time::Instant>,
    /// Window content shadow/depth between split panes
    window_content_shadow_enabled: bool,
    window_content_shadow_size: f32,
    window_content_shadow_opacity: f32,
    /// Click halo effect
    click_halo_enabled: bool,
    click_halo_color: (f32, f32, f32),
    click_halo_duration_ms: u32,
    click_halo_max_radius: f32,
    click_halos: Vec<ClickHaloEntry>,
    /// Window edge snap indicator
    edge_snap_enabled: bool,
    edge_snap_color: (f32, f32, f32),
    edge_snap_duration_ms: u32,
    edge_snaps: Vec<EdgeSnapEntry>,
    /// Cursor crosshair guide lines
    cursor_crosshair_enabled: bool,
    cursor_crosshair_color: (f32, f32, f32),
    cursor_crosshair_opacity: f32,
    /// Inactive window stained glass effect
    stained_glass_enabled: bool,
    stained_glass_opacity: f32,
    stained_glass_saturation: f32,
    /// Focus gradient border
    focus_gradient_border_enabled: bool,
    focus_gradient_border_top_color: (f32, f32, f32),
    focus_gradient_border_bot_color: (f32, f32, f32),
    focus_gradient_border_width: f32,
    focus_gradient_border_opacity: f32,
    /// Cursor magnetism effect
    cursor_magnetism_enabled: bool,
    cursor_magnetism_color: (f32, f32, f32),
    cursor_magnetism_ring_count: u32,
    cursor_magnetism_duration_ms: u32,
    cursor_magnetism_opacity: f32,
    cursor_magnetism_entries: Vec<(f32, f32, std::time::Instant)>, // x, y, time
    /// Window depth shadow layers
    depth_shadow_enabled: bool,
    depth_shadow_layers: u32,
    depth_shadow_offset: f32,
    depth_shadow_color: (f32, f32, f32),
    depth_shadow_opacity: f32,
    /// Mode-line gradient background
    mode_line_gradient_enabled: bool,
    mode_line_gradient_left_color: (f32, f32, f32),
    mode_line_gradient_right_color: (f32, f32, f32),
    mode_line_gradient_opacity: f32,
    /// Window corner fold effect
    corner_fold_enabled: bool,
    corner_fold_size: f32,
    corner_fold_color: (f32, f32, f32),
    corner_fold_opacity: f32,
    /// Frosted window border effect
    frosted_border_enabled: bool,
    frosted_border_width: f32,
    frosted_border_opacity: f32,
    frosted_border_color: (f32, f32, f32),
    /// Line number pulse on cursor line
    line_number_pulse_enabled: bool,
    line_number_pulse_color: (f32, f32, f32),
    line_number_pulse_intensity: f32,
    line_number_pulse_cycle_ms: u32,
    /// Window breathing border animation
    breathing_border_enabled: bool,
    breathing_border_color: (f32, f32, f32),
    breathing_border_min_opacity: f32,
    breathing_border_max_opacity: f32,
    breathing_border_cycle_ms: u32,
    /// Window scanline (CRT) effect
    scanlines_enabled: bool,
    scanlines_spacing: u32,
    scanlines_opacity: f32,
    scanlines_color: (f32, f32, f32),
    /// Cursor comet tail effect
    cursor_comet_enabled: bool,
    cursor_comet_trail_length: u32,
    cursor_comet_fade_ms: u32,
    cursor_comet_color: (f32, f32, f32),
    cursor_comet_opacity: f32,
    cursor_comet_positions: Vec<(f32, f32, f32, f32, std::time::Instant)>, // x, y, w, h, time
    /// Cursor spotlight/radial gradient effect
    cursor_spotlight_enabled: bool,
    cursor_spotlight_radius: f32,
    cursor_spotlight_intensity: f32,
    cursor_spotlight_color: (f32, f32, f32),
    /// Cursor particle trail effect
    cursor_particles_enabled: bool,
    cursor_particles_color: (f32, f32, f32),
    cursor_particles_count: u32,
    cursor_particles_lifetime_ms: u32,
    cursor_particles_gravity: f32,
    cursor_particles: Vec<CursorParticle>,
    cursor_particles_prev_pos: Option<(f32, f32)>,
    /// Per-window rounded border
    window_border_radius_enabled: bool,
    window_border_radius: f32,
    window_border_width: f32,
    window_border_color: (f32, f32, f32),
    window_border_opacity: f32,
    /// Typing heat map overlay
    typing_heatmap_enabled: bool,
    typing_heatmap_color: (f32, f32, f32),
    typing_heatmap_fade_ms: u32,
    typing_heatmap_opacity: f32,
    typing_heatmap_entries: Vec<HeatMapEntry>,
    typing_heatmap_prev_cursor: Option<(f32, f32)>,
    /// Buffer modified border indicator
    modified_indicator_enabled: bool,
    modified_indicator_color: (f32, f32, f32),
    modified_indicator_width: f32,
    modified_indicator_opacity: f32,
    /// Scroll velocity fade overlay
    scroll_velocity_fade_enabled: bool,
    scroll_velocity_fade_max_opacity: f32,
    scroll_velocity_fade_ms: u32,
    scroll_velocity_fades: Vec<ScrollVelocityFadeEntry>,
    /// Mini-buffer completion highlight glow
    minibuffer_highlight_enabled: bool,
    minibuffer_highlight_color: (f32, f32, f32),
    minibuffer_highlight_opacity: f32,
    /// Smooth window padding transition on resize
    resize_padding_enabled: bool,
    resize_padding_duration_ms: u32,
    resize_padding_max: f32,
    resize_padding_started: Option<std::time::Instant>,
    /// Cursor error pulse (brief color flash on bell)
    cursor_error_pulse_enabled: bool,
    cursor_error_pulse_color: (f32, f32, f32),
    cursor_error_pulse_duration_ms: u32,
    cursor_error_pulse_started: Option<std::time::Instant>,
    /// Line wrap indicator overlay
    wrap_indicator_enabled: bool,
    wrap_indicator_color: (f32, f32, f32),
    wrap_indicator_opacity: f32,
    /// Per-window scroll momentum indicator
    scroll_momentum_enabled: bool,
    scroll_momentum_fade_ms: u32,
    scroll_momentum_width: f32,
    /// Active scroll momentum entries
    active_scroll_momentums: Vec<ScrollMomentumEntry>,
    /// Matrix/digital rain effect
    matrix_rain_enabled: bool,
    matrix_rain_color: (f32, f32, f32),
    matrix_rain_column_count: u32,
    matrix_rain_speed: f32,
    matrix_rain_opacity: f32,
    matrix_rain_columns: Vec<MatrixColumn>,
    /// Cursor elastic snap animation
    cursor_elastic_snap_enabled: bool,
    cursor_elastic_snap_overshoot: f32,
    cursor_elastic_snap_duration_ms: u32,
    /// Window frost/ice border effect
    frost_border_enabled: bool,
    frost_border_color: (f32, f32, f32),
    frost_border_width: f32,
    frost_border_opacity: f32,
    /// Cursor afterimage ghost effect
    cursor_ghost_enabled: bool,
    cursor_ghost_color: (f32, f32, f32),
    cursor_ghost_count: u32,
    cursor_ghost_fade_ms: u32,
    cursor_ghost_drift: f32,
    cursor_ghost_opacity: f32,
    cursor_ghost_entries: Vec<CursorGhostEntry>,
    /// Heat distortion/shimmer effect
    heat_distortion_enabled: bool,
    heat_distortion_intensity: f32,
    heat_distortion_speed: f32,
    heat_distortion_edge_width: f32,
    heat_distortion_opacity: f32,
    /// Cursor lighthouse beam effect
    cursor_lighthouse_enabled: bool,
    cursor_lighthouse_color: (f32, f32, f32),
    cursor_lighthouse_beam_width: f32,
    cursor_lighthouse_rotation_speed: f32,
    cursor_lighthouse_beam_length: f32,
    cursor_lighthouse_opacity: f32,
    /// Neon border effect
    neon_border_enabled: bool,
    neon_border_color: (f32, f32, f32),
    neon_border_intensity: f32,
    neon_border_flicker: f32,
    neon_border_thickness: f32,
    neon_border_opacity: f32,
    /// Cursor sonar ping effect
    cursor_sonar_ping_enabled: bool,
    cursor_sonar_ping_color: (f32, f32, f32),
    cursor_sonar_ping_ring_count: u32,
    cursor_sonar_ping_max_radius: f32,
    cursor_sonar_ping_duration_ms: u32,
    cursor_sonar_ping_opacity: f32,
    cursor_sonar_ping_entries: Vec<SonarPingEntry>,
    /// Lightning bolt effect
    lightning_bolt_enabled: bool,
    lightning_bolt_color: (f32, f32, f32),
    lightning_bolt_frequency: f32,
    lightning_bolt_intensity: f32,
    lightning_bolt_opacity: f32,
    lightning_bolt_last: std::time::Instant,
    lightning_bolt_segments: Vec<(f32, f32, f32, f32)>, // (x1, y1, x2, y2)
    lightning_bolt_age: f32,
    /// Cursor orbit particles
    cursor_orbit_particles_enabled: bool,
    cursor_orbit_particles_color: (f32, f32, f32),
    cursor_orbit_particles_count: u32,
    cursor_orbit_particles_radius: f32,
    cursor_orbit_particles_speed: f32,
    cursor_orbit_particles_opacity: f32,
    /// Plasma border effect
    plasma_border_enabled: bool,
    plasma_border_color1: (f32, f32, f32),
    plasma_border_color2: (f32, f32, f32),
    plasma_border_width: f32,
    plasma_border_speed: f32,
    plasma_border_opacity: f32,
    /// Cursor heartbeat pulse
    cursor_heartbeat_enabled: bool,
    cursor_heartbeat_color: (f32, f32, f32),
    cursor_heartbeat_bpm: f32,
    cursor_heartbeat_max_radius: f32,
    cursor_heartbeat_opacity: f32,
    /// Warp/distortion grid effect
    warp_grid_enabled: bool,
    warp_grid_color: (f32, f32, f32),
    warp_grid_density: u32,
    warp_grid_amplitude: f32,
    warp_grid_speed: f32,
    warp_grid_opacity: f32,
    /// Cursor DNA helix trail
    cursor_dna_helix_enabled: bool,
    cursor_dna_helix_color1: (f32, f32, f32),
    cursor_dna_helix_color2: (f32, f32, f32),
    cursor_dna_helix_radius: f32,
    cursor_dna_helix_speed: f32,
    cursor_dna_helix_opacity: f32,
    /// Prism/rainbow edge effect
    prism_edge_enabled: bool,
    prism_edge_width: f32,
    prism_edge_speed: f32,
    prism_edge_saturation: f32,
    prism_edge_opacity: f32,
    /// Cursor pendulum swing
    cursor_pendulum_enabled: bool,
    cursor_pendulum_color: (f32, f32, f32),
    cursor_pendulum_arc_length: f32,
    cursor_pendulum_damping: f32,
    cursor_pendulum_opacity: f32,
    cursor_pendulum_last_x: f32,
    cursor_pendulum_last_y: f32,
    cursor_pendulum_swing_start: Option<std::time::Instant>,
    /// Hex grid overlay
    hex_grid_enabled: bool,
    hex_grid_color: (f32, f32, f32),
    hex_grid_cell_size: f32,
    hex_grid_pulse_speed: f32,
    hex_grid_opacity: f32,
    /// Cursor sparkle burst
    cursor_sparkle_burst_enabled: bool,
    cursor_sparkle_burst_color: (f32, f32, f32),
    cursor_sparkle_burst_count: u32,
    cursor_sparkle_burst_radius: f32,
    cursor_sparkle_burst_opacity: f32,
    cursor_sparkle_burst_entries: Vec<SparkleBurstEntry>,
    /// Circuit board trace
    circuit_trace_enabled: bool,
    circuit_trace_color: (f32, f32, f32),
    circuit_trace_width: f32,
    circuit_trace_speed: f32,
    circuit_trace_opacity: f32,
    /// Cursor compass rose
    cursor_compass_enabled: bool,
    cursor_compass_color: (f32, f32, f32),
    cursor_compass_size: f32,
    cursor_compass_speed: f32,
    cursor_compass_opacity: f32,
    /// Topographic contour effect
    topo_contour_enabled: bool,
    topo_contour_color: (f32, f32, f32),
    topo_contour_spacing: f32,
    topo_contour_speed: f32,
    topo_contour_opacity: f32,
    /// Cursor metronome tick
    cursor_metronome_enabled: bool,
    cursor_metronome_color: (f32, f32, f32),
    cursor_metronome_tick_height: f32,
    cursor_metronome_fade_ms: u32,
    cursor_metronome_opacity: f32,
    cursor_metronome_last_x: f32,
    cursor_metronome_last_y: f32,
    cursor_metronome_tick_start: Option<std::time::Instant>,
    /// Constellation overlay
    constellation_enabled: bool,
    constellation_color: (f32, f32, f32),
    constellation_star_count: u32,
    constellation_connect_dist: f32,
    constellation_twinkle_speed: f32,
    constellation_opacity: f32,
    /// Cursor radar sweep
    cursor_radar_enabled: bool,
    cursor_radar_color: (f32, f32, f32),
    cursor_radar_radius: f32,
    cursor_radar_speed: f32,
    cursor_radar_opacity: f32,
    /// Kaleidoscope overlay
    kaleidoscope_enabled: bool,
    kaleidoscope_color: (f32, f32, f32),
    kaleidoscope_segments: u32,
    kaleidoscope_speed: f32,
    kaleidoscope_opacity: f32,
    /// Cursor ripple ring
    cursor_ripple_ring_enabled: bool,
    cursor_ripple_ring_color: (f32, f32, f32),
    cursor_ripple_ring_max_radius: f32,
    cursor_ripple_ring_count: u32,
    cursor_ripple_ring_speed: f32,
    cursor_ripple_ring_opacity: f32,
    cursor_ripple_ring_start: Option<std::time::Instant>,
    cursor_ripple_ring_last_x: f32,
    cursor_ripple_ring_last_y: f32,
    /// Noise field overlay
    noise_field_enabled: bool,
    noise_field_color: (f32, f32, f32),
    noise_field_scale: f32,
    noise_field_speed: f32,
    noise_field_opacity: f32,
    /// Cursor scope
    cursor_scope_enabled: bool,
    cursor_scope_color: (f32, f32, f32),
    cursor_scope_thickness: f32,
    cursor_scope_gap: f32,
    cursor_scope_opacity: f32,
    /// Spiral vortex overlay
    spiral_vortex_enabled: bool,
    spiral_vortex_color: (f32, f32, f32),
    spiral_vortex_arms: u32,
    spiral_vortex_speed: f32,
    spiral_vortex_opacity: f32,
    /// Cursor shockwave
    cursor_shockwave_enabled: bool,
    cursor_shockwave_color: (f32, f32, f32),
    cursor_shockwave_radius: f32,
    cursor_shockwave_decay: f32,
    cursor_shockwave_opacity: f32,
    cursor_shockwave_start: Option<std::time::Instant>,
    cursor_shockwave_last_x: f32,
    cursor_shockwave_last_y: f32,
    /// Diamond lattice overlay
    diamond_lattice_enabled: bool,
    diamond_lattice_color: (f32, f32, f32),
    diamond_lattice_cell_size: f32,
    diamond_lattice_shimmer_speed: f32,
    diamond_lattice_opacity: f32,
    /// Cursor gravity well
    cursor_gravity_well_enabled: bool,
    cursor_gravity_well_color: (f32, f32, f32),
    cursor_gravity_well_field_radius: f32,
    cursor_gravity_well_line_count: u32,
    cursor_gravity_well_opacity: f32,
    /// Wave interference overlay
    wave_interference_enabled: bool,
    wave_interference_color: (f32, f32, f32),
    wave_interference_wavelength: f32,
    wave_interference_source_count: u32,
    wave_interference_speed: f32,
    wave_interference_opacity: f32,
    /// Cursor portal
    cursor_portal_enabled: bool,
    cursor_portal_color: (f32, f32, f32),
    cursor_portal_radius: f32,
    cursor_portal_speed: f32,
    cursor_portal_opacity: f32,
    /// Chevron pattern overlay
    chevron_pattern_enabled: bool,
    chevron_pattern_color: (f32, f32, f32),
    chevron_pattern_spacing: f32,
    chevron_pattern_speed: f32,
    chevron_pattern_opacity: f32,
    /// Cursor bubble
    cursor_bubble_enabled: bool,
    cursor_bubble_color: (f32, f32, f32),
    cursor_bubble_count: u32,
    cursor_bubble_rise_speed: f32,
    cursor_bubble_opacity: f32,
    cursor_bubble_spawn_time: Option<std::time::Instant>,
    cursor_bubble_last_x: f32,
    cursor_bubble_last_y: f32,
    /// Sunburst pattern
    sunburst_pattern_enabled: bool,
    sunburst_pattern_color: (f32, f32, f32),
    sunburst_pattern_ray_count: u32,
    sunburst_pattern_speed: f32,
    sunburst_pattern_opacity: f32,
    /// Cursor firework
    cursor_firework_enabled: bool,
    cursor_firework_color: (f32, f32, f32),
    cursor_firework_particle_count: u32,
    cursor_firework_burst_radius: f32,
    cursor_firework_opacity: f32,
    cursor_firework_start: Option<std::time::Instant>,
    cursor_firework_last_x: f32,
    cursor_firework_last_y: f32,
    /// Honeycomb dissolve
    honeycomb_dissolve_enabled: bool,
    honeycomb_dissolve_color: (f32, f32, f32),
    honeycomb_dissolve_cell_size: f32,
    honeycomb_dissolve_speed: f32,
    honeycomb_dissolve_opacity: f32,
    /// Cursor tornado
    cursor_tornado_enabled: bool,
    cursor_tornado_color: (f32, f32, f32),
    cursor_tornado_radius: f32,
    cursor_tornado_particle_count: u32,
    cursor_tornado_opacity: f32,
    /// Moiré pattern
    moire_pattern_enabled: bool,
    moire_pattern_color: (f32, f32, f32),
    moire_pattern_line_spacing: f32,
    moire_pattern_angle_offset: f32,
    moire_pattern_speed: f32,
    moire_pattern_opacity: f32,
    /// Cursor lightning
    cursor_lightning_enabled: bool,
    cursor_lightning_color: (f32, f32, f32),
    cursor_lightning_bolt_count: u32,
    cursor_lightning_max_length: f32,
    cursor_lightning_opacity: f32,
    cursor_lightning_start: Option<std::time::Instant>,
    cursor_lightning_last_x: f32,
    cursor_lightning_last_y: f32,
    /// Dot matrix
    dot_matrix_enabled: bool,
    dot_matrix_color: (f32, f32, f32),
    dot_matrix_spacing: f32,
    dot_matrix_pulse_speed: f32,
    dot_matrix_opacity: f32,
    /// Cursor snowflake
    cursor_snowflake_enabled: bool,
    cursor_snowflake_color: (f32, f32, f32),
    cursor_snowflake_count: u32,
    cursor_snowflake_fall_speed: f32,
    cursor_snowflake_opacity: f32,
    cursor_snowflake_start: Option<std::time::Instant>,
    cursor_snowflake_last_x: f32,
    cursor_snowflake_last_y: f32,
    /// Concentric rings
    concentric_rings_enabled: bool,
    concentric_rings_color: (f32, f32, f32),
    concentric_rings_spacing: f32,
    concentric_rings_expansion_speed: f32,
    concentric_rings_opacity: f32,
    /// Cursor flame
    cursor_flame_enabled: bool,
    cursor_flame_color: (f32, f32, f32),
    cursor_flame_particle_count: u32,
    cursor_flame_height: f32,
    cursor_flame_opacity: f32,
    /// Zigzag pattern
    zigzag_pattern_enabled: bool,
    zigzag_pattern_color: (f32, f32, f32),
    zigzag_pattern_amplitude: f32,
    zigzag_pattern_frequency: f32,
    zigzag_pattern_speed: f32,
    zigzag_pattern_opacity: f32,
    /// Cursor crystal
    cursor_crystal_enabled: bool,
    cursor_crystal_color: (f32, f32, f32),
    cursor_crystal_facet_count: u32,
    cursor_crystal_radius: f32,
    cursor_crystal_opacity: f32,
    /// Basket weave overlay
    basket_weave_enabled: bool,
    basket_weave_color: (f32, f32, f32),
    basket_weave_strip_width: f32,
    basket_weave_strip_spacing: f32,
    basket_weave_opacity: f32,
    /// Cursor sparkler effect
    cursor_sparkler_enabled: bool,
    cursor_sparkler_color: (f32, f32, f32),
    cursor_sparkler_spark_count: u32,
    cursor_sparkler_burn_speed: f32,
    cursor_sparkler_opacity: f32,
    /// Fish scale overlay
    fish_scale_enabled: bool,
    fish_scale_color: (f32, f32, f32),
    fish_scale_size: f32,
    fish_scale_row_offset: f32,
    fish_scale_opacity: f32,
    /// Cursor plasma ball effect
    cursor_plasma_ball_enabled: bool,
    cursor_plasma_ball_color: (f32, f32, f32),
    cursor_plasma_ball_tendril_count: u32,
    cursor_plasma_ball_arc_speed: f32,
    cursor_plasma_ball_opacity: f32,
    /// Trefoil knot overlay
    trefoil_knot_enabled: bool,
    trefoil_knot_color: (f32, f32, f32),
    trefoil_knot_size: f32,
    trefoil_knot_rotation_speed: f32,
    trefoil_knot_opacity: f32,
    /// Cursor quill pen effect
    cursor_quill_pen_enabled: bool,
    cursor_quill_pen_color: (f32, f32, f32),
    cursor_quill_pen_trail_length: u32,
    cursor_quill_pen_ink_speed: f32,
    cursor_quill_pen_opacity: f32,
    /// Herringbone pattern overlay
    herringbone_pattern_enabled: bool,
    herringbone_pattern_color: (f32, f32, f32),
    herringbone_pattern_tile_width: f32,
    herringbone_pattern_tile_height: f32,
    herringbone_pattern_opacity: f32,
    /// Cursor aurora borealis effect
    cursor_aurora_borealis_enabled: bool,
    cursor_aurora_borealis_color: (f32, f32, f32),
    cursor_aurora_borealis_band_count: u32,
    cursor_aurora_borealis_shimmer_speed: f32,
    cursor_aurora_borealis_opacity: f32,
    /// Target reticle overlay
    target_reticle_enabled: bool,
    target_reticle_color: (f32, f32, f32),
    target_reticle_ring_count: u32,
    target_reticle_pulse_speed: f32,
    target_reticle_opacity: f32,
    /// Cursor feather effect
    cursor_feather_enabled: bool,
    cursor_feather_color: (f32, f32, f32),
    cursor_feather_count: u32,
    cursor_feather_drift_speed: f32,
    cursor_feather_opacity: f32,
    /// Plaid pattern overlay
    plaid_pattern_enabled: bool,
    plaid_pattern_color: (f32, f32, f32),
    plaid_pattern_band_width: f32,
    plaid_pattern_band_spacing: f32,
    plaid_pattern_opacity: f32,
    /// Cursor stardust effect
    cursor_stardust_enabled: bool,
    cursor_stardust_color: (f32, f32, f32),
    cursor_stardust_particle_count: u32,
    cursor_stardust_fall_speed: f32,
    cursor_stardust_opacity: f32,
    /// Brick wall overlay
    brick_wall_enabled: bool,
    brick_wall_color: (f32, f32, f32),
    brick_wall_width: f32,
    brick_wall_height: f32,
    brick_wall_opacity: f32,
    /// Cursor compass needle effect
    cursor_compass_needle_enabled: bool,
    cursor_compass_needle_color: (f32, f32, f32),
    cursor_compass_needle_length: f32,
    cursor_compass_needle_spin_speed: f32,
    cursor_compass_needle_opacity: f32,
    /// Sine wave overlay
    sine_wave_enabled: bool,
    sine_wave_color: (f32, f32, f32),
    sine_wave_amplitude: f32,
    sine_wave_wavelength: f32,
    sine_wave_speed: f32,
    sine_wave_opacity: f32,
    /// Cursor galaxy effect
    cursor_galaxy_enabled: bool,
    cursor_galaxy_color: (f32, f32, f32),
    cursor_galaxy_star_count: u32,
    cursor_galaxy_radius: f32,
    cursor_galaxy_opacity: f32,
    /// Rotating gear overlay
    rotating_gear_enabled: bool,
    rotating_gear_color: (f32, f32, f32),
    rotating_gear_size: f32,
    rotating_gear_speed: f32,
    rotating_gear_opacity: f32,
    /// Cursor prism effect
    cursor_prism_enabled: bool,
    cursor_prism_color: (f32, f32, f32),
    cursor_prism_ray_count: u32,
    cursor_prism_spread: f32,
    cursor_prism_opacity: f32,
    /// Crosshatch pattern overlay
    crosshatch_pattern_enabled: bool,
    crosshatch_pattern_color: (f32, f32, f32),
    crosshatch_pattern_line_spacing: f32,
    crosshatch_pattern_angle: f32,
    crosshatch_pattern_speed: f32,
    crosshatch_pattern_opacity: f32,
    /// Cursor moth effect
    cursor_moth_enabled: bool,
    cursor_moth_color: (f32, f32, f32),
    cursor_moth_count: u32,
    cursor_moth_wing_size: f32,
    cursor_moth_opacity: f32,
    /// Window edge glow on scroll boundaries
    edge_glow_enabled: bool,
    edge_glow_color: (f32, f32, f32),
    edge_glow_height: f32,
    edge_glow_opacity: f32,
    edge_glow_fade_ms: u32,
    edge_glow_entries: Vec<EdgeGlowEntry>,
    /// Window rain/drip effect
    rain_effect_enabled: bool,
    rain_effect_color: (f32, f32, f32),
    rain_effect_drop_count: u32,
    rain_effect_speed: f32,
    rain_effect_opacity: f32,
    rain_drops: Vec<RainDrop>,
    rain_last_spawn: std::time::Instant,
    /// Cursor ripple wave effect
    cursor_ripple_wave_enabled: bool,
    cursor_ripple_wave_color: (f32, f32, f32),
    cursor_ripple_wave_ring_count: u32,
    cursor_ripple_wave_max_radius: f32,
    cursor_ripple_wave_duration_ms: u32,
    cursor_ripple_wave_opacity: f32,
    cursor_ripple_waves: Vec<RippleWaveEntry>,
    /// Window aurora/northern lights effect
    aurora_enabled: bool,
    aurora_color1: (f32, f32, f32),
    aurora_color2: (f32, f32, f32),
    aurora_height: f32,
    aurora_speed: f32,
    aurora_opacity: f32,
    aurora_start: std::time::Instant,
}

/// Entry for an active scroll momentum indicator
struct ScrollMomentumEntry {
    window_id: i64,
    bounds: Rect,
    direction: i32, // 1 = down, -1 = up
    started: std::time::Instant,
    duration: std::time::Duration,
}

/// Entry for matrix rain column
struct MatrixColumn {
    x: f32,
    y: f32,
    speed: f32,
    length: f32,
}

/// Entry for cursor ghost afterimage
struct CursorGhostEntry {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    started: std::time::Instant,
}

/// Entry for cursor sonar ping
struct SonarPingEntry {
    cx: f32,
    cy: f32,
    started: std::time::Instant,
    duration: std::time::Duration,
}

struct SparkleBurstEntry {
    cx: f32,
    cy: f32,
    started: std::time::Instant,
    /// Random seed for particle directions
    seed: u32,
}

/// Entry for window edge glow (scroll boundary indicator)
struct EdgeGlowEntry {
    window_id: i64,
    bounds: Rect,
    at_top: bool,
    started: std::time::Instant,
    duration: std::time::Duration,
}

/// Entry for rain drop
struct RainDrop {
    x: f32,
    y: f32,
    speed: f32,
    length: f32,
    opacity: f32,
}

/// Entry for cursor ripple wave
struct RippleWaveEntry {
    x: f32,
    y: f32,
    started: std::time::Instant,
    duration: std::time::Duration,
}

/// Entry for window edge snap indicator
/// Entry for cursor particle effect
struct CursorParticle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    started: std::time::Instant,
    lifetime: std::time::Duration,
}

/// Entry for typing heat map (records where cursor was during edits)
struct HeatMapEntry {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    started: std::time::Instant,
}

struct EdgeSnapEntry {
    bounds: Rect,
    mode_line_height: f32,
    at_top: bool,
    at_bottom: bool,
    started: std::time::Instant,
    duration: std::time::Duration,
}

/// Entry for click halo effect
struct ClickHaloEntry {
    x: f32,
    y: f32,
    started: std::time::Instant,
    duration: std::time::Duration,
}

/// Entry for scroll velocity fade overlay
struct ScrollVelocityFadeEntry {
    window_id: i64,
    bounds: Rect,
    /// Scroll delta magnitude (characters scrolled)
    velocity: f32,
    started: std::time::Instant,
    duration: std::time::Duration,
}

/// Entry for an active window switch highlight fade
struct WindowFadeEntry {
    window_id: i64,
    bounds: Rect,
    started: std::time::Instant,
    duration: std::time::Duration,
    intensity: f32,
}

/// Entry for an active title/breadcrumb crossfade animation
struct TitleFadeEntry {
    window_id: i64,
    bounds: Rect,
    old_text: String,
    new_text: String,
    started: std::time::Instant,
    duration: std::time::Duration,
}

/// Entry for an active line insertion/deletion animation
struct LineAnimEntry {
    /// Window bounds where the animation is active
    window_bounds: Rect,
    /// Y position below which glyphs are displaced
    edit_y: f32,
    /// Initial Y offset (negative=insertion slide-down, positive=deletion slide-up)
    initial_offset: f32,
    /// When the animation started
    started: std::time::Instant,
    /// Duration of the animation
    duration: std::time::Duration,
}

/// Entry for an active mode-line content transition
struct ModeLineFadeEntry {
    window_id: i64,
    /// Mode-line area (y, height) within the window
    mode_line_y: f32,
    mode_line_h: f32,
    bounds_x: f32,
    bounds_w: f32,
    started: std::time::Instant,
    duration: std::time::Duration,
}

/// Entry for an active text fade-in animation
struct TextFadeEntry {
    window_id: i64,
    bounds: Rect,
    started: std::time::Instant,
    duration: std::time::Duration,
}

/// Entry for an active scroll line spacing animation
struct ScrollSpacingEntry {
    window_id: i64,
    bounds: Rect,
    /// +1 = scroll down (content moves up), -1 = scroll up
    direction: i32,
    started: std::time::Instant,
    duration: std::time::Duration,
}

impl WgpuRenderer {
    /// Create a new WgpuRenderer with its own GPU device.
    ///
    /// Returns an error if GPU initialization fails.
    /// Prefer `with_device()` when you already have a device/queue.
    pub fn new(
        surface: Option<wgpu::Surface<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        pollster::block_on(Self::new_async(surface, width, height))
    }

    /// Create a new WgpuRenderer using an existing device and queue.
    ///
    /// This is useful when you need to share the wgpu device with other components,
    /// such as when surfaces are created with a specific device.
    ///
    /// The `surface_format` parameter specifies the texture format for render pipelines.
    /// This must match the format of the surface being rendered to.
    pub fn with_device(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        width: u32,
        height: u32,
        surface_format: wgpu::TextureFormat,
        scale_factor: f32,
    ) -> Self {
        Self::create_renderer_internal(device, queue, None, Some(surface_format), width, height, scale_factor)
    }

    /// Internal helper that creates the renderer with the given device/queue.
    ///
    /// This handles pipeline and buffer creation, and is used by both `new_async`
    /// and `with_device`.
    ///
    /// The `surface_format` parameter specifies the texture format for render pipelines.
    /// If None, defaults to Bgra8UnormSrgb.
    fn create_renderer_internal(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        surface: Option<wgpu::Surface<'static>>,
        surface_format: Option<wgpu::TextureFormat>,
        width: u32,
        height: u32,
        scale_factor: f32,
    ) -> Self {
        // Create uniform buffer with logical size so vertex positions from Emacs map correctly
        let logical_w = width as f32 / scale_factor;
        let logical_h = height as f32 / scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // Create bind group
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Load rect shader
        let rect_shader_source = include_str!("shaders/rect.wgsl");
        let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rect Shader"),
            source: wgpu::ShaderSource::Wgsl(rect_shader_source.into()),
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Rect Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Determine the target format
        let target_format = surface_format
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);

        // Create rect pipeline
        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Rect Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rect_shader,
                entry_point: Some("vs_main"),
                buffers: &[RectVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &rect_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Load rounded rect shader (SDF-based rounded borders)
        let rounded_rect_shader_source = include_str!("shaders/rounded_rect.wgsl");
        let rounded_rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rounded Rect Shader"),
            source: wgpu::ShaderSource::Wgsl(rounded_rect_shader_source.into()),
        });

        let rounded_rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Rounded Rect Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rounded_rect_shader,
                entry_point: Some("vs_main"),
                buffers: &[RoundedRectVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &rounded_rect_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Corner mask pipeline: uses the same SDF rounded rect shader but with
        // a blend mode that multiplies the destination by the source alpha.
        // This clips window corners to a rounded shape.
        let corner_mask_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Corner Mask Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rounded_rect_shader,
                entry_point: Some("vs_main"),
                buffers: &[RoundedRectVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &rounded_rect_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState {
                        // dst = dst * src_alpha (mask mode)
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::SrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::SrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Load glyph shader
        let glyph_shader_source = include_str!("shaders/glyph.wgsl");
        let glyph_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Glyph Shader"),
            source: wgpu::ShaderSource::Wgsl(glyph_shader_source.into()),
        });

        // Glyph bind group layout (for per-glyph texture)
        let glyph_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Glyph Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Glyph pipeline layout (uniform + glyph texture)
        let glyph_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Glyph Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout, &glyph_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create glyph pipeline
        let glyph_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Glyph Pipeline"),
            layout: Some(&glyph_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &glyph_shader,
                entry_point: Some("vs_main"),
                buffers: &[GlyphVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &glyph_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Create image cache (also creates its bind group layout)
        let image_cache = ImageCache::new(&device);

        // Create video cache
        #[cfg(feature = "video")]
        let mut video_cache = VideoCache::new();
        #[cfg(feature = "video")]
        video_cache.init_gpu(&device);

        // Create webkit cache
        #[cfg(feature = "wpe-webkit")]
        let webkit_cache = WgpuWebKitCache::new(&device);

        // Load image shader
        let image_shader_source = include_str!("shaders/image.wgsl");
        let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Image Shader"),
            source: wgpu::ShaderSource::Wgsl(image_shader_source.into()),
        });

        // Image pipeline layout (uniform + image texture)
        let image_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Image Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout, image_cache.bind_group_layout()],
            push_constant_ranges: &[],
        });

        // Create image pipeline (similar to glyph but for RGBA textures)
        let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Image Pipeline"),
            layout: Some(&image_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &image_shader,
                entry_point: Some("vs_main"),
                buffers: &[GlyphVertex::desc()], // Reuse glyph vertex format
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &image_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Opaque image pipeline — for XRGB/BGRX DMA-BUF textures where alpha=0x00.
        // Uses fs_main_opaque which ignores texture alpha and uses vertex alpha instead.
        let opaque_image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Opaque Image Pipeline"),
            layout: Some(&image_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &image_shader,
                entry_point: Some("vs_main"),
                buffers: &[GlyphVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &image_shader,
                entry_point: Some("fs_main_opaque"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Create surface_config from format if we have a surface
        let surface_config = if let Some(ref s) = surface {
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: target_format,
                width,
                height,
                present_mode: wgpu::PresentMode::Fifo, // VSync
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            s.configure(&device, &config);
            Some(config)
        } else {
            None
        };

        Self {
            device,
            queue,
            surface,
            surface_config,
            surface_format: target_format,
            rect_pipeline,
            rounded_rect_pipeline,
            corner_mask_pipeline,
            glyph_pipeline,
            image_pipeline,
            opaque_image_pipeline,
            glyph_bind_group_layout,
            uniform_buffer,
            uniform_bind_group,
            image_cache,
            #[cfg(feature = "video")]
            video_cache,
            #[cfg(feature = "wpe-webkit")]
            webkit_cache,
            width,
            height,
            scale_factor,
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
            per_window_dim: std::collections::HashMap::new(),
            last_dim_tick: std::time::Instant::now(),
            needs_continuous_redraw: false,
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
            cursor_pulse_start: std::time::Instant::now(),
            focus_mode_enabled: false,
            focus_mode_opacity: 0.4,
            minimap_enabled: false,
            minimap_width: 80.0,
            typing_ripple_enabled: false,
            typing_ripple_max_radius: 40.0,
            typing_ripple_duration: 0.3,
            active_ripples: Vec::new(),
            search_pulse_enabled: false,
            search_pulse_face_id: 0,
            search_pulse_start: std::time::Instant::now(),
            vignette_enabled: false,
            vignette_intensity: 0.3,
            vignette_radius: 50.0,
            zen_mode_enabled: false,
            zen_mode_content_width_pct: 60.0,
            zen_mode_margin_opacity: 0.3,
            bg_pattern_style: 0,
            bg_pattern_spacing: 20.0,
            bg_pattern_color: (0.5, 0.5, 0.5),
            bg_pattern_opacity: 0.05,
            line_animation_enabled: false,
            line_animation_duration_ms: 150,
            active_line_anims: Vec::new(),
            header_shadow_enabled: false,
            header_shadow_intensity: 0.3,
            header_shadow_size: 6.0,
            cursor_color_cycle_enabled: false,
            cursor_color_cycle_speed: 0.5,
            cursor_color_cycle_saturation: 0.8,
            cursor_color_cycle_lightness: 0.6,
            cursor_color_cycle_start: std::time::Instant::now(),
            window_switch_fade_enabled: false,
            window_switch_fade_duration_ms: 200,
            window_switch_fade_intensity: 0.15,
            active_window_fades: Vec::new(),
            breadcrumb_enabled: false,
            breadcrumb_opacity: 0.7,
            border_transition_enabled: false,
            border_transition_active_color: (0.4, 0.6, 1.0),
            border_transition_duration: std::time::Duration::from_millis(200),
            border_transitions: Vec::new(),
            prev_border_selected: 0,
            accent_strip_enabled: false,
            accent_strip_width: 3.0,
            cursor_trail_fade_enabled: false,
            cursor_trail_fade_length: 8,
            cursor_trail_fade_duration: std::time::Duration::from_millis(300),
            cursor_trail_positions: Vec::new(),
            cursor_trail_last_pos: (0.0, 0.0),
            cursor_shadow_enabled: false,
            cursor_shadow_offset_x: 2.0,
            cursor_shadow_offset_y: 2.0,
            cursor_shadow_opacity: 0.3,
            focus_ring_enabled: false,
            focus_ring_color: (0.4, 0.6, 1.0),
            focus_ring_opacity: 0.5,
            focus_ring_dash_length: 8.0,
            focus_ring_speed: 40.0,
            focus_ring_start: std::time::Instant::now(),
            window_mode_tint_enabled: false,
            window_mode_tint_opacity: 0.03,
            window_watermark_enabled: false,
            window_watermark_opacity: 0.08,
            window_watermark_threshold: 10,
            region_glow_enabled: false,
            region_glow_face_id: 0,
            region_glow_radius: 6.0,
            region_glow_opacity: 0.3,
            idle_dim_alpha: 0.0,
            noise_grain_enabled: false,
            noise_grain_intensity: 0.03,
            noise_grain_size: 2.0,
            noise_grain_frame: 0,
            padding_gradient_enabled: false,
            padding_gradient_color: (0.0, 0.0, 0.0),
            padding_gradient_opacity: 0.15,
            padding_gradient_width: 8.0,
            frosted_glass_enabled: false,
            frosted_glass_opacity: 0.3,
            frosted_glass_blur: 4.0,
            title_fade_enabled: false,
            title_fade_duration_ms: 300,
            prev_breadcrumb_text: std::collections::HashMap::new(),
            active_title_fades: Vec::new(),
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
            mode_line_transition_enabled: false,
            mode_line_transition_duration_ms: 200,
            prev_mode_line_hashes: std::collections::HashMap::new(),
            active_mode_line_fades: Vec::new(),
            text_fade_in_enabled: false,
            text_fade_in_duration_ms: 150,
            active_text_fades: Vec::new(),
            scroll_line_spacing_enabled: false,
            scroll_line_spacing_max: 6.0,
            scroll_line_spacing_duration_ms: 200,
            active_scroll_spacings: Vec::new(),
            cursor_wake_enabled: false,
            cursor_wake_duration_ms: 120,
            cursor_wake_scale: 1.3,
            cursor_wake_started: None,
            window_content_shadow_enabled: false,
            window_content_shadow_size: 6.0,
            window_content_shadow_opacity: 0.15,
            click_halo_enabled: false,
            click_halo_color: (0.4, 0.6, 1.0),
            click_halo_duration_ms: 300,
            click_halo_max_radius: 30.0,
            click_halos: Vec::new(),
            edge_snap_enabled: false,
            edge_snap_color: (1.0, 0.5, 0.2),
            edge_snap_duration_ms: 200,
            edge_snaps: Vec::new(),
            cursor_crosshair_enabled: false,
            cursor_crosshair_color: (0.5, 0.5, 0.5),
            cursor_crosshair_opacity: 0.15,
            stained_glass_enabled: false,
            stained_glass_opacity: 0.08,
            stained_glass_saturation: 0.6,
            focus_gradient_border_enabled: false,
            focus_gradient_border_top_color: (0.3, 0.6, 1.0),
            focus_gradient_border_bot_color: (0.6, 0.3, 1.0),
            focus_gradient_border_width: 2.0,
            focus_gradient_border_opacity: 0.6,
            cursor_magnetism_enabled: false,
            cursor_magnetism_color: (0.4, 0.7, 1.0),
            cursor_magnetism_ring_count: 3,
            cursor_magnetism_duration_ms: 300,
            cursor_magnetism_opacity: 0.5,
            cursor_magnetism_entries: Vec::new(),
            depth_shadow_enabled: false,
            depth_shadow_layers: 3,
            depth_shadow_offset: 2.0,
            depth_shadow_color: (0.0, 0.0, 0.0),
            depth_shadow_opacity: 0.15,
            mode_line_gradient_enabled: false,
            mode_line_gradient_left_color: (0.2, 0.3, 0.5),
            mode_line_gradient_right_color: (0.5, 0.3, 0.2),
            mode_line_gradient_opacity: 0.3,
            corner_fold_enabled: false,
            corner_fold_size: 20.0,
            corner_fold_color: (0.6, 0.4, 0.2),
            corner_fold_opacity: 0.5,
            frosted_border_enabled: false,
            frosted_border_width: 4.0,
            frosted_border_opacity: 0.15,
            frosted_border_color: (1.0, 1.0, 1.0),
            line_number_pulse_enabled: false,
            line_number_pulse_color: (0.4, 0.6, 1.0),
            line_number_pulse_intensity: 0.3,
            line_number_pulse_cycle_ms: 2000,
            breathing_border_enabled: false,
            breathing_border_color: (0.5, 0.5, 0.5),
            breathing_border_min_opacity: 0.05,
            breathing_border_max_opacity: 0.3,
            breathing_border_cycle_ms: 3000,
            scanlines_enabled: false,
            scanlines_spacing: 2,
            scanlines_opacity: 0.08,
            scanlines_color: (0.0, 0.0, 0.0),
            cursor_comet_enabled: false,
            cursor_comet_trail_length: 5,
            cursor_comet_fade_ms: 300,
            cursor_comet_color: (0.5, 0.7, 1.0),
            cursor_comet_opacity: 0.6,
            cursor_comet_positions: Vec::new(),
            cursor_spotlight_enabled: false,
            cursor_spotlight_radius: 200.0,
            cursor_spotlight_intensity: 0.15,
            cursor_spotlight_color: (1.0, 1.0, 0.9),
            cursor_particles_enabled: false,
            cursor_particles_color: (1.0, 0.6, 0.2),
            cursor_particles_count: 6,
            cursor_particles_lifetime_ms: 800,
            cursor_particles_gravity: 120.0,
            cursor_particles: Vec::new(),
            cursor_particles_prev_pos: None,
            window_border_radius_enabled: false,
            window_border_radius: 8.0,
            window_border_width: 1.0,
            window_border_color: (0.5, 0.5, 0.5),
            window_border_opacity: 0.3,
            typing_heatmap_enabled: false,
            typing_heatmap_color: (1.0, 0.4, 0.1),
            typing_heatmap_fade_ms: 2000,
            typing_heatmap_opacity: 0.15,
            typing_heatmap_entries: Vec::new(),
            typing_heatmap_prev_cursor: None,
            modified_indicator_enabled: false,
            modified_indicator_color: (1.0, 0.6, 0.2),
            modified_indicator_width: 3.0,
            modified_indicator_opacity: 0.8,
            scroll_velocity_fade_enabled: false,
            scroll_velocity_fade_max_opacity: 0.15,
            scroll_velocity_fade_ms: 300,
            scroll_velocity_fades: Vec::new(),
            minibuffer_highlight_enabled: false,
            minibuffer_highlight_color: (0.4, 0.6, 1.0),
            minibuffer_highlight_opacity: 0.25,
            resize_padding_enabled: false,
            resize_padding_duration_ms: 200,
            resize_padding_max: 12.0,
            resize_padding_started: None,
            cursor_error_pulse_enabled: false,
            cursor_error_pulse_color: (1.0, 0.2, 0.2),
            cursor_error_pulse_duration_ms: 250,
            cursor_error_pulse_started: None,
            wrap_indicator_enabled: false,
            wrap_indicator_color: (0.5, 0.6, 0.8),
            wrap_indicator_opacity: 0.3,
            scroll_momentum_enabled: false,
            scroll_momentum_fade_ms: 300,
            scroll_momentum_width: 3.0,
            active_scroll_momentums: Vec::new(),
            matrix_rain_enabled: false,
            matrix_rain_color: (0.0, 0.8, 0.2),
            matrix_rain_column_count: 40,
            matrix_rain_speed: 150.0,
            matrix_rain_opacity: 0.12,
            matrix_rain_columns: Vec::new(),
            cursor_elastic_snap_enabled: false,
            cursor_elastic_snap_overshoot: 0.15,
            cursor_elastic_snap_duration_ms: 200,
            frost_border_enabled: false,
            frost_border_color: (0.7, 0.85, 1.0),
            frost_border_width: 6.0,
            frost_border_opacity: 0.2,
            cursor_ghost_enabled: false,
            cursor_ghost_color: (0.5, 0.5, 1.0),
            cursor_ghost_count: 4,
            cursor_ghost_fade_ms: 600,
            cursor_ghost_drift: 20.0,
            cursor_ghost_opacity: 0.4,
            cursor_ghost_entries: Vec::new(),
            heat_distortion_enabled: false,
            heat_distortion_intensity: 0.3,
            heat_distortion_speed: 1.0,
            heat_distortion_edge_width: 30.0,
            heat_distortion_opacity: 0.15,
            cursor_lighthouse_enabled: false,
            cursor_lighthouse_color: (1.0, 0.9, 0.3),
            cursor_lighthouse_beam_width: 15.0,
            cursor_lighthouse_rotation_speed: 0.5,
            cursor_lighthouse_beam_length: 200.0,
            cursor_lighthouse_opacity: 0.12,
            neon_border_enabled: false,
            neon_border_color: (0.0, 1.0, 0.8),
            neon_border_intensity: 0.6,
            neon_border_flicker: 0.1,
            neon_border_thickness: 3.0,
            neon_border_opacity: 0.4,
            cursor_sonar_ping_enabled: false,
            cursor_sonar_ping_color: (0.3, 0.7, 1.0),
            cursor_sonar_ping_ring_count: 3,
            cursor_sonar_ping_max_radius: 60.0,
            cursor_sonar_ping_duration_ms: 600,
            cursor_sonar_ping_opacity: 0.25,
            cursor_sonar_ping_entries: Vec::new(),
            lightning_bolt_enabled: false,
            lightning_bolt_color: (0.7, 0.8, 1.0),
            lightning_bolt_frequency: 1.0,
            lightning_bolt_intensity: 0.8,
            lightning_bolt_opacity: 0.3,
            lightning_bolt_last: std::time::Instant::now(),
            lightning_bolt_segments: Vec::new(),
            lightning_bolt_age: 0.0,
            cursor_orbit_particles_enabled: false,
            cursor_orbit_particles_color: (1.0, 0.8, 0.3),
            cursor_orbit_particles_count: 6,
            cursor_orbit_particles_radius: 25.0,
            cursor_orbit_particles_speed: 1.5,
            cursor_orbit_particles_opacity: 0.35,
            plasma_border_enabled: false,
            plasma_border_color1: (1.0, 0.2, 0.5),
            plasma_border_color2: (0.2, 0.5, 1.0),
            plasma_border_width: 4.0,
            plasma_border_speed: 1.0,
            plasma_border_opacity: 0.3,
            cursor_heartbeat_enabled: false,
            cursor_heartbeat_color: (1.0, 0.3, 0.3),
            cursor_heartbeat_bpm: 72.0,
            cursor_heartbeat_max_radius: 50.0,
            cursor_heartbeat_opacity: 0.2,
            warp_grid_enabled: false,
            warp_grid_color: (0.3, 0.5, 0.9),
            warp_grid_density: 20,
            warp_grid_amplitude: 5.0,
            warp_grid_speed: 1.0,
            warp_grid_opacity: 0.15,
            cursor_dna_helix_enabled: false,
            cursor_dna_helix_color1: (0.3, 0.9, 0.5),
            cursor_dna_helix_color2: (0.5, 0.3, 0.9),
            cursor_dna_helix_radius: 12.0,
            cursor_dna_helix_speed: 1.5,
            cursor_dna_helix_opacity: 0.3,
            prism_edge_enabled: false,
            prism_edge_width: 6.0,
            prism_edge_speed: 1.0,
            prism_edge_saturation: 0.8,
            prism_edge_opacity: 0.25,
            cursor_pendulum_enabled: false,
            cursor_pendulum_color: (0.9, 0.7, 0.3),
            cursor_pendulum_arc_length: 40.0,
            cursor_pendulum_damping: 0.5,
            cursor_pendulum_opacity: 0.3,
            cursor_pendulum_last_x: 0.0,
            cursor_pendulum_last_y: 0.0,
            cursor_pendulum_swing_start: None,
            hex_grid_enabled: false,
            hex_grid_color: (0.3, 0.6, 0.9),
            hex_grid_cell_size: 40.0,
            hex_grid_pulse_speed: 1.0,
            hex_grid_opacity: 0.1,
            cursor_sparkle_burst_enabled: false,
            cursor_sparkle_burst_color: (1.0, 0.85, 0.3),
            cursor_sparkle_burst_count: 12,
            cursor_sparkle_burst_radius: 30.0,
            cursor_sparkle_burst_opacity: 0.4,
            cursor_sparkle_burst_entries: Vec::new(),
            circuit_trace_enabled: false,
            circuit_trace_color: (0.2, 0.8, 0.4),
            circuit_trace_width: 2.0,
            circuit_trace_speed: 1.0,
            circuit_trace_opacity: 0.2,
            cursor_compass_enabled: false,
            cursor_compass_color: (0.9, 0.6, 0.2),
            cursor_compass_size: 20.0,
            cursor_compass_speed: 1.0,
            cursor_compass_opacity: 0.25,
            topo_contour_enabled: false,
            topo_contour_color: (0.4, 0.7, 0.5),
            topo_contour_spacing: 30.0,
            topo_contour_speed: 1.0,
            topo_contour_opacity: 0.1,
            cursor_metronome_enabled: false,
            cursor_metronome_color: (0.9, 0.5, 0.2),
            cursor_metronome_tick_height: 20.0,
            cursor_metronome_fade_ms: 300,
            cursor_metronome_opacity: 0.4,
            cursor_metronome_last_x: 0.0,
            cursor_metronome_last_y: 0.0,
            cursor_metronome_tick_start: None,
            constellation_enabled: false,
            constellation_color: (0.7, 0.8, 1.0),
            constellation_star_count: 50,
            constellation_connect_dist: 80.0,
            constellation_twinkle_speed: 1.0,
            constellation_opacity: 0.15,
            cursor_radar_enabled: false,
            cursor_radar_color: (0.2, 0.9, 0.4),
            cursor_radar_radius: 40.0,
            cursor_radar_speed: 1.5,
            cursor_radar_opacity: 0.2,
            kaleidoscope_enabled: false,
            kaleidoscope_color: (0.6, 0.3, 0.9),
            kaleidoscope_segments: 6,
            kaleidoscope_speed: 0.5,
            kaleidoscope_opacity: 0.1,
            cursor_ripple_ring_enabled: false,
            cursor_ripple_ring_color: (0.3, 0.8, 1.0),
            cursor_ripple_ring_max_radius: 60.0,
            cursor_ripple_ring_count: 3,
            cursor_ripple_ring_speed: 2.0,
            cursor_ripple_ring_opacity: 0.25,
            cursor_ripple_ring_start: None,
            cursor_ripple_ring_last_x: 0.0,
            cursor_ripple_ring_last_y: 0.0,
            noise_field_enabled: false,
            noise_field_color: (0.5, 0.7, 0.3),
            noise_field_scale: 50.0,
            noise_field_speed: 0.5,
            noise_field_opacity: 0.08,
            cursor_scope_enabled: false,
            cursor_scope_color: (1.0, 0.8, 0.2),
            cursor_scope_thickness: 1.0,
            cursor_scope_gap: 10.0,
            cursor_scope_opacity: 0.3,
            spiral_vortex_enabled: false,
            spiral_vortex_color: (0.4, 0.2, 0.8),
            spiral_vortex_arms: 4,
            spiral_vortex_speed: 0.5,
            spiral_vortex_opacity: 0.1,
            cursor_shockwave_enabled: false,
            cursor_shockwave_color: (1.0, 0.6, 0.2),
            cursor_shockwave_radius: 80.0,
            cursor_shockwave_decay: 2.0,
            cursor_shockwave_opacity: 0.3,
            cursor_shockwave_start: None,
            cursor_shockwave_last_x: 0.0,
            cursor_shockwave_last_y: 0.0,
            diamond_lattice_enabled: false,
            diamond_lattice_color: (0.7, 0.5, 0.9),
            diamond_lattice_cell_size: 30.0,
            diamond_lattice_shimmer_speed: 0.8,
            diamond_lattice_opacity: 0.08,
            cursor_gravity_well_enabled: false,
            cursor_gravity_well_color: (0.3, 0.6, 1.0),
            cursor_gravity_well_field_radius: 80.0,
            cursor_gravity_well_line_count: 8,
            cursor_gravity_well_opacity: 0.2,
            wave_interference_enabled: false,
            wave_interference_color: (0.3, 0.5, 0.9),
            wave_interference_wavelength: 30.0,
            wave_interference_source_count: 3,
            wave_interference_speed: 1.0,
            wave_interference_opacity: 0.08,
            cursor_portal_enabled: false,
            cursor_portal_color: (0.6, 0.2, 0.9),
            cursor_portal_radius: 30.0,
            cursor_portal_speed: 2.0,
            cursor_portal_opacity: 0.25,
            chevron_pattern_enabled: false,
            chevron_pattern_color: (0.4, 0.7, 0.5),
            chevron_pattern_spacing: 40.0,
            chevron_pattern_speed: 0.5,
            chevron_pattern_opacity: 0.06,
            cursor_bubble_enabled: false,
            cursor_bubble_color: (0.4, 0.8, 1.0),
            cursor_bubble_count: 6,
            cursor_bubble_rise_speed: 80.0,
            cursor_bubble_opacity: 0.2,
            cursor_bubble_spawn_time: None,
            cursor_bubble_last_x: 0.0,
            cursor_bubble_last_y: 0.0,
            sunburst_pattern_enabled: false,
            sunburst_pattern_color: (1.0, 0.8, 0.3),
            sunburst_pattern_ray_count: 12,
            sunburst_pattern_speed: 0.5,
            sunburst_pattern_opacity: 0.08,
            cursor_firework_enabled: false,
            cursor_firework_color: (1.0, 0.6, 0.2),
            cursor_firework_particle_count: 16,
            cursor_firework_burst_radius: 60.0,
            cursor_firework_opacity: 0.3,
            cursor_firework_start: None,
            cursor_firework_last_x: 0.0,
            cursor_firework_last_y: 0.0,
            honeycomb_dissolve_enabled: false,
            honeycomb_dissolve_color: (0.8, 0.6, 0.2),
            honeycomb_dissolve_cell_size: 30.0,
            honeycomb_dissolve_speed: 0.8,
            honeycomb_dissolve_opacity: 0.08,
            cursor_tornado_enabled: false,
            cursor_tornado_color: (0.5, 0.7, 1.0),
            cursor_tornado_radius: 40.0,
            cursor_tornado_particle_count: 12,
            cursor_tornado_opacity: 0.25,
            moire_pattern_enabled: false,
            moire_pattern_color: (0.5, 0.5, 0.8),
            moire_pattern_line_spacing: 8.0,
            moire_pattern_angle_offset: 5.0,
            moire_pattern_speed: 0.3,
            moire_pattern_opacity: 0.06,
            cursor_lightning_enabled: false,
            cursor_lightning_color: (0.6, 0.8, 1.0),
            cursor_lightning_bolt_count: 4,
            cursor_lightning_max_length: 50.0,
            cursor_lightning_opacity: 0.4,
            cursor_lightning_start: None,
            cursor_lightning_last_x: 0.0,
            cursor_lightning_last_y: 0.0,
            dot_matrix_enabled: false,
            dot_matrix_color: (0.3, 1.0, 0.3),
            dot_matrix_spacing: 12.0,
            dot_matrix_pulse_speed: 1.0,
            dot_matrix_opacity: 0.06,
            cursor_snowflake_enabled: false,
            cursor_snowflake_color: (0.8, 0.9, 1.0),
            cursor_snowflake_count: 8,
            cursor_snowflake_fall_speed: 30.0,
            cursor_snowflake_opacity: 0.3,
            cursor_snowflake_start: None,
            cursor_snowflake_last_x: 0.0,
            cursor_snowflake_last_y: 0.0,
            concentric_rings_enabled: false,
            concentric_rings_color: (0.4, 0.6, 1.0),
            concentric_rings_spacing: 30.0,
            concentric_rings_expansion_speed: 1.0,
            concentric_rings_opacity: 0.06,
            cursor_flame_enabled: false,
            cursor_flame_color: (1.0, 0.5, 0.1),
            cursor_flame_particle_count: 10,
            cursor_flame_height: 40.0,
            cursor_flame_opacity: 0.3,
            zigzag_pattern_enabled: false,
            zigzag_pattern_color: (0.6, 0.4, 1.0),
            zigzag_pattern_amplitude: 15.0,
            zigzag_pattern_frequency: 0.1,
            zigzag_pattern_speed: 1.0,
            zigzag_pattern_opacity: 0.06,
            cursor_crystal_enabled: false,
            cursor_crystal_color: (0.7, 0.9, 1.0),
            cursor_crystal_facet_count: 6,
            cursor_crystal_radius: 25.0,
            cursor_crystal_opacity: 0.3,
            basket_weave_enabled: false,
            basket_weave_color: (0.55, 0.4, 0.25),
            basket_weave_strip_width: 6.0,
            basket_weave_strip_spacing: 20.0,
            basket_weave_opacity: 0.05,
            cursor_sparkler_enabled: false,
            cursor_sparkler_color: (1.0, 0.85, 0.3),
            cursor_sparkler_spark_count: 12,
            cursor_sparkler_burn_speed: 1.0,
            cursor_sparkler_opacity: 0.25,
            fish_scale_enabled: false,
            fish_scale_color: (0.3, 0.6, 0.7),
            fish_scale_size: 16.0,
            fish_scale_row_offset: 0.5,
            fish_scale_opacity: 0.04,
            cursor_plasma_ball_enabled: false,
            cursor_plasma_ball_color: (0.7, 0.3, 1.0),
            cursor_plasma_ball_tendril_count: 6,
            cursor_plasma_ball_arc_speed: 1.0,
            cursor_plasma_ball_opacity: 0.2,
            trefoil_knot_enabled: false,
            trefoil_knot_color: (0.4, 0.6, 0.9),
            trefoil_knot_size: 80.0,
            trefoil_knot_rotation_speed: 1.0,
            trefoil_knot_opacity: 0.06,
            cursor_quill_pen_enabled: false,
            cursor_quill_pen_color: (0.3, 0.15, 0.05),
            cursor_quill_pen_trail_length: 8,
            cursor_quill_pen_ink_speed: 1.0,
            cursor_quill_pen_opacity: 0.2,
            herringbone_pattern_enabled: false,
            herringbone_pattern_color: (0.6, 0.5, 0.4),
            herringbone_pattern_tile_width: 20.0,
            herringbone_pattern_tile_height: 10.0,
            herringbone_pattern_opacity: 0.05,
            cursor_aurora_borealis_enabled: false,
            cursor_aurora_borealis_color: (0.2, 0.9, 0.5),
            cursor_aurora_borealis_band_count: 5,
            cursor_aurora_borealis_shimmer_speed: 1.0,
            cursor_aurora_borealis_opacity: 0.15,
            target_reticle_enabled: false,
            target_reticle_color: (0.2, 0.8, 0.2),
            target_reticle_ring_count: 3,
            target_reticle_pulse_speed: 1.0,
            target_reticle_opacity: 0.08,
            cursor_feather_enabled: false,
            cursor_feather_color: (0.9, 0.85, 0.7),
            cursor_feather_count: 4,
            cursor_feather_drift_speed: 1.0,
            cursor_feather_opacity: 0.18,
            plaid_pattern_enabled: false,
            plaid_pattern_color: (0.7, 0.3, 0.3),
            plaid_pattern_band_width: 4.0,
            plaid_pattern_band_spacing: 30.0,
            plaid_pattern_opacity: 0.05,
            cursor_stardust_enabled: false,
            cursor_stardust_color: (1.0, 0.9, 0.5),
            cursor_stardust_particle_count: 20,
            cursor_stardust_fall_speed: 1.0,
            cursor_stardust_opacity: 0.2,
            brick_wall_enabled: false,
            brick_wall_color: (0.6, 0.4, 0.3),
            brick_wall_width: 40.0,
            brick_wall_height: 20.0,
            brick_wall_opacity: 0.06,
            cursor_compass_needle_enabled: false,
            cursor_compass_needle_color: (1.0, 0.3, 0.3),
            cursor_compass_needle_length: 20.0,
            cursor_compass_needle_spin_speed: 2.0,
            cursor_compass_needle_opacity: 0.2,
            sine_wave_enabled: false,
            sine_wave_color: (0.3, 0.7, 1.0),
            sine_wave_amplitude: 20.0,
            sine_wave_wavelength: 80.0,
            sine_wave_speed: 1.0,
            sine_wave_opacity: 0.06,
            cursor_galaxy_enabled: false,
            cursor_galaxy_color: (0.8, 0.8, 1.0),
            cursor_galaxy_star_count: 30,
            cursor_galaxy_radius: 30.0,
            cursor_galaxy_opacity: 0.2,
            rotating_gear_enabled: false,
            rotating_gear_color: (0.6, 0.7, 0.8),
            rotating_gear_size: 40.0,
            rotating_gear_speed: 0.5,
            rotating_gear_opacity: 0.08,
            cursor_prism_enabled: false,
            cursor_prism_color: (1.0, 1.0, 1.0),
            cursor_prism_ray_count: 7,
            cursor_prism_spread: 30.0,
            cursor_prism_opacity: 0.15,
            crosshatch_pattern_enabled: false,
            crosshatch_pattern_color: (0.5, 0.6, 0.4),
            crosshatch_pattern_line_spacing: 20.0,
            crosshatch_pattern_angle: 45.0,
            crosshatch_pattern_speed: 0.3,
            crosshatch_pattern_opacity: 0.06,
            cursor_moth_enabled: false,
            cursor_moth_color: (0.9, 0.8, 0.5),
            cursor_moth_count: 5,
            cursor_moth_wing_size: 8.0,
            cursor_moth_opacity: 0.2,
            edge_glow_enabled: false,
            edge_glow_color: (0.4, 0.6, 1.0),
            edge_glow_height: 40.0,
            edge_glow_opacity: 0.3,
            edge_glow_fade_ms: 400,
            edge_glow_entries: Vec::new(),
            rain_effect_enabled: false,
            rain_effect_color: (0.5, 0.6, 0.8),
            rain_effect_drop_count: 30,
            rain_effect_speed: 120.0,
            rain_effect_opacity: 0.15,
            rain_drops: Vec::new(),
            rain_last_spawn: std::time::Instant::now(),
            cursor_ripple_wave_enabled: false,
            cursor_ripple_wave_color: (0.4, 0.6, 1.0),
            cursor_ripple_wave_ring_count: 3,
            cursor_ripple_wave_max_radius: 80.0,
            cursor_ripple_wave_duration_ms: 500,
            cursor_ripple_wave_opacity: 0.3,
            cursor_ripple_waves: Vec::new(),
            aurora_enabled: false,
            aurora_color1: (0.2, 0.8, 0.4),
            aurora_color2: (0.3, 0.4, 0.9),
            aurora_height: 60.0,
            aurora_speed: 1.0,
            aurora_opacity: 0.12,
            aurora_start: std::time::Instant::now(),
        }
    }

    /// Update inactive window dim config
    pub fn set_inactive_dim_config(&mut self, enabled: bool, opacity: f32) {
        self.inactive_dim_enabled = enabled;
        self.inactive_dim_opacity = opacity;
    }

    /// Update mode-line separator config
    pub fn set_mode_line_separator(&mut self, style: u32, color: (f32, f32, f32), height: f32) {
        self.mode_line_separator_style = style;
        self.mode_line_separator_color = color;
        self.mode_line_separator_height = height;
    }

    /// Update cursor glow config
    pub fn set_cursor_glow(&mut self, enabled: bool, color: (f32, f32, f32), radius: f32, opacity: f32) {
        self.cursor_glow_enabled = enabled;
        self.cursor_glow_color = color;
        self.cursor_glow_radius = radius;
        self.cursor_glow_opacity = opacity;
    }

    /// Update cursor pulse config
    pub fn set_cursor_pulse(&mut self, enabled: bool, speed: f32, min_opacity: f32) {
        self.cursor_pulse_enabled = enabled;
        self.cursor_pulse_speed = speed;
        self.cursor_pulse_min_opacity = min_opacity;
    }

    /// Update focus mode config
    pub fn set_focus_mode(&mut self, enabled: bool, opacity: f32) {
        self.focus_mode_enabled = enabled;
        self.focus_mode_opacity = opacity;
    }

    /// Update minimap config
    pub fn set_minimap(&mut self, enabled: bool, width: f32) {
        self.minimap_enabled = enabled;
        self.minimap_width = width;
    }

    /// Update vignette config
    pub fn set_vignette(&mut self, enabled: bool, intensity: f32, radius: f32) {
        self.vignette_enabled = enabled;
        self.vignette_intensity = intensity;
        self.vignette_radius = radius;
    }

    /// Update zen mode config
    pub fn set_zen_mode(&mut self, enabled: bool, content_width_pct: f32, margin_opacity: f32) {
        self.zen_mode_enabled = enabled;
        self.zen_mode_content_width_pct = content_width_pct;
        self.zen_mode_margin_opacity = margin_opacity;
    }

    /// Update background pattern config
    pub fn set_background_pattern(&mut self, style: u32, spacing: f32, r: f32, g: f32, b: f32, opacity: f32) {
        self.bg_pattern_style = style;
        self.bg_pattern_spacing = spacing;
        self.bg_pattern_color = (r, g, b);
        self.bg_pattern_opacity = opacity;
    }

    /// Update line animation config
    pub fn set_line_animation(&mut self, enabled: bool, duration_ms: u32) {
        self.line_animation_enabled = enabled;
        self.line_animation_duration_ms = duration_ms;
        if !enabled {
            self.active_line_anims.clear();
        }
    }

    /// Start a line animation for a window
    pub fn start_line_animation(&mut self, window_bounds: Rect, edit_y: f32, offset: f32, duration_ms: u32) {
        // Remove any existing animation for this window region
        self.active_line_anims.retain(|a| {
            (a.window_bounds.x - window_bounds.x).abs() > 1.0
            || (a.window_bounds.y - window_bounds.y).abs() > 1.0
        });
        self.active_line_anims.push(LineAnimEntry {
            window_bounds,
            edit_y,
            initial_offset: offset,
            started: std::time::Instant::now(),
            duration: std::time::Duration::from_millis(duration_ms as u64),
        });
        self.needs_continuous_redraw = true;
    }

    /// Compute Y offset for a glyph due to active line animations
    fn line_y_offset(&self, gx: f32, gy: f32) -> f32 {
        let mut offset = 0.0;
        for anim in &self.active_line_anims {
            let b = &anim.window_bounds;
            // Check if glyph is in this window and below the edit point
            if gx >= b.x && gx < b.x + b.width
                && gy >= b.y && gy < b.y + b.height
                && gy >= anim.edit_y
            {
                let elapsed = anim.started.elapsed();
                let t = (elapsed.as_secs_f32() / anim.duration.as_secs_f32()).min(1.0);
                // Ease-out quadratic: t * (2 - t)
                let eased = t * (2.0 - t);
                offset += anim.initial_offset * (1.0 - eased);
            }
        }
        // Scroll line spacing accordion effect
        let now = std::time::Instant::now();
        for entry in &self.active_scroll_spacings {
            let b = &entry.bounds;
            if gx >= b.x && gx < b.x + b.width
                && gy >= b.y && gy < b.y + b.height
            {
                let elapsed = now.duration_since(entry.started).as_secs_f32();
                let total = entry.duration.as_secs_f32();
                if elapsed < total {
                    let progress = elapsed / total;
                    let decay = 1.0 - progress;
                    let decay = decay * decay;
                    let norm = ((gy - b.y) / b.height).clamp(0.0, 1.0);
                    let edge_factor = if entry.direction > 0 {
                        1.0 - norm
                    } else {
                        norm
                    };
                    offset += self.scroll_line_spacing_max * decay * edge_factor;
                }
            }
        }
        offset
    }

    /// Update cursor color cycling config
    pub fn set_cursor_color_cycle(&mut self, enabled: bool, speed: f32, saturation: f32, lightness: f32) {
        self.cursor_color_cycle_enabled = enabled;
        self.cursor_color_cycle_speed = speed;
        self.cursor_color_cycle_saturation = saturation;
        self.cursor_color_cycle_lightness = lightness;
        if enabled {
            self.cursor_color_cycle_start = std::time::Instant::now();
        }
    }

    /// Update window switch fade config
    pub fn set_window_switch_fade(&mut self, enabled: bool, duration_ms: u32, intensity: f32) {
        self.window_switch_fade_enabled = enabled;
        self.window_switch_fade_duration_ms = duration_ms;
        self.window_switch_fade_intensity = intensity;
    }

    /// Update inactive window tint config
    pub fn set_inactive_tint(&mut self, enabled: bool, color: (f32, f32, f32), opacity: f32) {
        self.inactive_tint_enabled = enabled;
        self.inactive_tint_color = color;
        self.inactive_tint_opacity = opacity;
    }

    /// Update scroll progress indicator config
    pub fn set_scroll_progress(&mut self, enabled: bool, height: f32, color: (f32, f32, f32), opacity: f32) {
        self.scroll_progress_enabled = enabled;
        self.scroll_progress_height = height;
        self.scroll_progress_color = color;
        self.scroll_progress_opacity = opacity;
    }

    /// Update active window border glow config
    pub fn set_window_glow(&mut self, enabled: bool, color: (f32, f32, f32), radius: f32, intensity: f32) {
        self.window_glow_enabled = enabled;
        self.window_glow_color = color;
        self.window_glow_radius = radius;
        self.window_glow_intensity = intensity;
    }

    /// Update breadcrumb/path bar config
    pub fn set_breadcrumb(&mut self, enabled: bool, opacity: f32) {
        self.breadcrumb_enabled = enabled;
        self.breadcrumb_opacity = opacity;
    }

    /// Update border transition config
    pub fn set_border_transition(&mut self, enabled: bool, active_color: (f32, f32, f32), duration_ms: u32) {
        self.border_transition_enabled = enabled;
        self.border_transition_active_color = active_color;
        self.border_transition_duration = std::time::Duration::from_millis(duration_ms as u64);
        if !enabled {
            self.border_transitions.clear();
        }
    }

    /// Update accent strip config
    pub fn set_accent_strip(&mut self, enabled: bool, width: f32) {
        self.accent_strip_enabled = enabled;
        self.accent_strip_width = width;
    }

    /// Update cursor shadow config
    pub fn set_cursor_shadow(&mut self, enabled: bool, offset_x: f32, offset_y: f32, opacity: f32) {
        self.cursor_shadow_enabled = enabled;
        self.cursor_shadow_offset_x = offset_x;
        self.cursor_shadow_offset_y = offset_y;
        self.cursor_shadow_opacity = opacity;
    }

    /// Update cursor wake animation config
    pub fn set_cursor_wake(&mut self, enabled: bool, duration_ms: u32, scale: f32) {
        self.cursor_wake_enabled = enabled;
        self.cursor_wake_duration_ms = duration_ms;
        self.cursor_wake_scale = scale;
    }

    /// Trigger a cursor wake animation
    pub fn trigger_cursor_wake(&mut self, now: std::time::Instant) {
        self.cursor_wake_started = Some(now);
    }

    /// Get current cursor wake scale factor (1.0 = no scaling)
    fn cursor_wake_factor(&self) -> f32 {
        if !self.cursor_wake_enabled {
            return 1.0;
        }
        if let Some(started) = self.cursor_wake_started {
            let elapsed = started.elapsed().as_millis() as f32;
            let duration = self.cursor_wake_duration_ms as f32;
            if elapsed >= duration {
                return 1.0;
            }
            let t = elapsed / duration;
            // Ease-out: scale starts large and settles to 1.0
            let ease = t * (2.0 - t); // quadratic ease-out
            1.0 + (self.cursor_wake_scale - 1.0) * (1.0 - ease)
        } else {
            1.0
        }
    }

    /// Update window content shadow config
    pub fn set_window_content_shadow(&mut self, enabled: bool, size: f32, opacity: f32) {
        self.window_content_shadow_enabled = enabled;
        self.window_content_shadow_size = size;
        self.window_content_shadow_opacity = opacity;
    }

    /// Update minibuffer highlight config
    pub fn set_minibuffer_highlight(&mut self, enabled: bool, color: (f32, f32, f32), opacity: f32) {
        self.minibuffer_highlight_enabled = enabled;
        self.minibuffer_highlight_color = color;
        self.minibuffer_highlight_opacity = opacity;
    }

    /// Update edge snap config
    pub fn set_edge_snap(&mut self, enabled: bool, color: (f32, f32, f32), duration_ms: u32) {
        self.edge_snap_enabled = enabled;
        self.edge_snap_color = color;
        self.edge_snap_duration_ms = duration_ms;
    }

    /// Trigger edge snap indicator
    pub fn trigger_edge_snap(&mut self, bounds: Rect, mode_line_height: f32, at_top: bool, at_bottom: bool, now: std::time::Instant) {
        self.edge_snaps.push(EdgeSnapEntry {
            bounds,
            mode_line_height,
            at_top,
            at_bottom,
            started: now,
            duration: std::time::Duration::from_millis(self.edge_snap_duration_ms as u64),
        });
    }

    /// Update cursor crosshair config
    pub fn set_cursor_crosshair(&mut self, enabled: bool, color: (f32, f32, f32), opacity: f32) {
        self.cursor_crosshair_enabled = enabled;
        self.cursor_crosshair_color = color;
        self.cursor_crosshair_opacity = opacity;
    }

    /// Update stained glass config
    pub fn set_stained_glass(&mut self, enabled: bool, opacity: f32, saturation: f32) {
        self.stained_glass_enabled = enabled;
        self.stained_glass_opacity = opacity;
        self.stained_glass_saturation = saturation;
    }

    /// Update focus gradient border config
    pub fn set_focus_gradient_border(&mut self, enabled: bool, top_color: (f32, f32, f32), bot_color: (f32, f32, f32), width: f32, opacity: f32) {
        self.focus_gradient_border_enabled = enabled;
        self.focus_gradient_border_top_color = top_color;
        self.focus_gradient_border_bot_color = bot_color;
        self.focus_gradient_border_width = width;
        self.focus_gradient_border_opacity = opacity;
    }

    /// Update cursor magnetism config
    pub fn set_cursor_magnetism(&mut self, enabled: bool, color: (f32, f32, f32), ring_count: u32, duration_ms: u32, opacity: f32) {
        self.cursor_magnetism_enabled = enabled;
        self.cursor_magnetism_color = color;
        self.cursor_magnetism_ring_count = ring_count;
        self.cursor_magnetism_duration_ms = duration_ms;
        self.cursor_magnetism_opacity = opacity;
        if !enabled {
            self.cursor_magnetism_entries.clear();
        }
    }

    /// Update depth shadow config
    pub fn set_depth_shadow(&mut self, enabled: bool, layers: u32, offset: f32, color: (f32, f32, f32), opacity: f32) {
        self.depth_shadow_enabled = enabled;
        self.depth_shadow_layers = layers;
        self.depth_shadow_offset = offset;
        self.depth_shadow_color = color;
        self.depth_shadow_opacity = opacity;
    }

    /// Update mode-line gradient config
    pub fn set_mode_line_gradient(&mut self, enabled: bool, left_color: (f32, f32, f32), right_color: (f32, f32, f32), opacity: f32) {
        self.mode_line_gradient_enabled = enabled;
        self.mode_line_gradient_left_color = left_color;
        self.mode_line_gradient_right_color = right_color;
        self.mode_line_gradient_opacity = opacity;
    }

    /// Update corner fold config
    pub fn set_corner_fold(&mut self, enabled: bool, size: f32, color: (f32, f32, f32), opacity: f32) {
        self.corner_fold_enabled = enabled;
        self.corner_fold_size = size;
        self.corner_fold_color = color;
        self.corner_fold_opacity = opacity;
    }

    /// Update frosted border config
    pub fn set_frosted_border(&mut self, enabled: bool, width: f32, opacity: f32, color: (f32, f32, f32)) {
        self.frosted_border_enabled = enabled;
        self.frosted_border_width = width;
        self.frosted_border_opacity = opacity;
        self.frosted_border_color = color;
    }

    /// Update line number pulse config
    pub fn set_line_number_pulse(&mut self, enabled: bool, color: (f32, f32, f32), intensity: f32, cycle_ms: u32) {
        self.line_number_pulse_enabled = enabled;
        self.line_number_pulse_color = color;
        self.line_number_pulse_intensity = intensity;
        self.line_number_pulse_cycle_ms = cycle_ms;
    }

    /// Update breathing border config
    pub fn set_breathing_border(&mut self, enabled: bool, color: (f32, f32, f32), min_opacity: f32, max_opacity: f32, cycle_ms: u32) {
        self.breathing_border_enabled = enabled;
        self.breathing_border_color = color;
        self.breathing_border_min_opacity = min_opacity;
        self.breathing_border_max_opacity = max_opacity;
        self.breathing_border_cycle_ms = cycle_ms;
    }

    /// Update scanline config
    pub fn set_scanlines(&mut self, enabled: bool, spacing: u32, opacity: f32, color: (f32, f32, f32)) {
        self.scanlines_enabled = enabled;
        self.scanlines_spacing = spacing;
        self.scanlines_opacity = opacity;
        self.scanlines_color = color;
    }

    /// Update cursor comet tail config
    pub fn set_cursor_comet(&mut self, enabled: bool, trail_length: u32, fade_ms: u32, color: (f32, f32, f32), opacity: f32) {
        self.cursor_comet_enabled = enabled;
        self.cursor_comet_trail_length = trail_length;
        self.cursor_comet_fade_ms = fade_ms;
        self.cursor_comet_color = color;
        self.cursor_comet_opacity = opacity;
        if !enabled {
            self.cursor_comet_positions.clear();
        }
    }

    /// Update cursor spotlight config
    pub fn set_cursor_spotlight(&mut self, enabled: bool, radius: f32, intensity: f32, color: (f32, f32, f32)) {
        self.cursor_spotlight_enabled = enabled;
        self.cursor_spotlight_radius = radius;
        self.cursor_spotlight_intensity = intensity;
        self.cursor_spotlight_color = color;
    }

    /// Update cursor particle trail config
    pub fn set_cursor_particles(&mut self, enabled: bool, color: (f32, f32, f32), count: u32, lifetime_ms: u32, gravity: f32) {
        self.cursor_particles_enabled = enabled;
        self.cursor_particles_color = color;
        self.cursor_particles_count = count;
        self.cursor_particles_lifetime_ms = lifetime_ms;
        self.cursor_particles_gravity = gravity;
        if !enabled {
            self.cursor_particles.clear();
            self.cursor_particles_prev_pos = None;
        }
    }

    /// Update per-window rounded border config
    pub fn set_window_border_radius(&mut self, enabled: bool, radius: f32, border_width: f32, color: (f32, f32, f32), opacity: f32) {
        self.window_border_radius_enabled = enabled;
        self.window_border_radius = radius;
        self.window_border_width = border_width;
        self.window_border_color = color;
        self.window_border_opacity = opacity;
    }

    /// Update typing heat map config
    pub fn set_typing_heatmap(&mut self, enabled: bool, color: (f32, f32, f32), fade_ms: u32, opacity: f32) {
        self.typing_heatmap_enabled = enabled;
        self.typing_heatmap_color = color;
        self.typing_heatmap_fade_ms = fade_ms;
        self.typing_heatmap_opacity = opacity;
        if !enabled {
            self.typing_heatmap_entries.clear();
            self.typing_heatmap_prev_cursor = None;
        }
    }

    /// Update buffer modified border indicator config
    pub fn set_modified_indicator(&mut self, enabled: bool, color: (f32, f32, f32), width: f32, opacity: f32) {
        self.modified_indicator_enabled = enabled;
        self.modified_indicator_color = color;
        self.modified_indicator_width = width;
        self.modified_indicator_opacity = opacity;
    }

    /// Update click halo config
    pub fn set_click_halo(&mut self, enabled: bool, color: (f32, f32, f32), duration_ms: u32, max_radius: f32) {
        self.click_halo_enabled = enabled;
        self.click_halo_color = color;
        self.click_halo_duration_ms = duration_ms;
        self.click_halo_max_radius = max_radius;
    }

    /// Trigger click halo at position
    pub fn trigger_click_halo(&mut self, x: f32, y: f32, now: std::time::Instant) {
        self.click_halos.push(ClickHaloEntry {
            x,
            y,
            started: now,
            duration: std::time::Duration::from_millis(self.click_halo_duration_ms as u64),
        });
    }

    /// Update scroll velocity fade config
    pub fn set_scroll_velocity_fade(&mut self, enabled: bool, max_opacity: f32, fade_ms: u32) {
        self.scroll_velocity_fade_enabled = enabled;
        self.scroll_velocity_fade_max_opacity = max_opacity;
        self.scroll_velocity_fade_ms = fade_ms;
    }

    /// Trigger scroll velocity fade for a window
    pub fn trigger_scroll_velocity_fade(&mut self, window_id: i64, bounds: Rect, delta: f32, now: std::time::Instant) {
        // Replace existing entry for this window
        self.scroll_velocity_fades.retain(|e| e.window_id != window_id);
        self.scroll_velocity_fades.push(ScrollVelocityFadeEntry {
            window_id,
            bounds,
            velocity: delta,
            started: now,
            duration: std::time::Duration::from_millis(self.scroll_velocity_fade_ms as u64),
        });
    }

    /// Update resize padding config
    pub fn set_resize_padding(&mut self, enabled: bool, duration_ms: u32, max_padding: f32) {
        self.resize_padding_enabled = enabled;
        self.resize_padding_duration_ms = duration_ms;
        self.resize_padding_max = max_padding;
    }

    /// Trigger resize padding animation
    pub fn trigger_resize_padding(&mut self, now: std::time::Instant) {
        self.resize_padding_started = Some(now);
    }

    /// Get current resize padding amount (eases from max to 0)
    fn resize_padding_amount(&self) -> f32 {
        if let Some(started) = self.resize_padding_started {
            let elapsed = started.elapsed().as_millis() as f32;
            let duration = self.resize_padding_duration_ms as f32;
            if elapsed >= duration {
                return 0.0;
            }
            let t = elapsed / duration;
            let ease = t * (2.0 - t); // quadratic ease-out
            self.resize_padding_max * (1.0 - ease)
        } else {
            0.0
        }
    }

    /// Update cursor error pulse config
    pub fn set_cursor_error_pulse(&mut self, enabled: bool, r: f32, g: f32, b: f32, duration_ms: u32) {
        self.cursor_error_pulse_enabled = enabled;
        self.cursor_error_pulse_color = (r, g, b);
        self.cursor_error_pulse_duration_ms = duration_ms;
    }

    /// Trigger a cursor error pulse
    pub fn trigger_cursor_error_pulse(&mut self, now: std::time::Instant) {
        self.cursor_error_pulse_started = Some(now);
    }

    /// Get the cursor error pulse color override, if active
    fn cursor_error_pulse_override(&self) -> Option<Color> {
        if !self.cursor_error_pulse_enabled {
            return None;
        }
        if let Some(started) = self.cursor_error_pulse_started {
            let elapsed = started.elapsed().as_millis() as f32;
            let duration = self.cursor_error_pulse_duration_ms as f32;
            if elapsed >= duration {
                return None;
            }
            let t = elapsed / duration;
            // Flash: bright at start, fade out
            let alpha = (1.0 - t) * (1.0 - t);
            let (r, g, b) = self.cursor_error_pulse_color;
            Some(Color::new(r, g, b, alpha))
        } else {
            None
        }
    }

    /// Update wrap indicator config
    pub fn set_wrap_indicator(&mut self, enabled: bool, r: f32, g: f32, b: f32, opacity: f32) {
        self.wrap_indicator_enabled = enabled;
        self.wrap_indicator_color = (r, g, b);
        self.wrap_indicator_opacity = opacity;
    }

    /// Update scroll momentum indicator config
    pub fn set_scroll_momentum(&mut self, enabled: bool, fade_ms: u32, width: f32) {
        self.scroll_momentum_enabled = enabled;
        self.scroll_momentum_fade_ms = fade_ms;
        self.scroll_momentum_width = width;
    }

    /// Trigger a scroll momentum indicator for a window
    pub fn trigger_scroll_momentum(&mut self, window_id: i64, bounds: Rect, direction: i32, now: std::time::Instant) {
        self.active_scroll_momentums.retain(|e| e.window_id != window_id);
        self.active_scroll_momentums.push(ScrollMomentumEntry {
            window_id,
            bounds,
            direction,
            started: now,
            duration: std::time::Duration::from_millis(self.scroll_momentum_fade_ms as u64),
        });
    }

    /// Update matrix rain config
    pub fn set_matrix_rain(&mut self, enabled: bool, color: (f32, f32, f32), column_count: u32, speed: f32, opacity: f32) {
        self.matrix_rain_enabled = enabled;
        self.matrix_rain_color = color;
        self.matrix_rain_column_count = column_count;
        self.matrix_rain_speed = speed;
        self.matrix_rain_opacity = opacity;
        if !enabled {
            self.matrix_rain_columns.clear();
        }
    }

    /// Update cursor elastic snap config
    pub fn set_cursor_elastic_snap(&mut self, enabled: bool, overshoot: f32, duration_ms: u32) {
        self.cursor_elastic_snap_enabled = enabled;
        self.cursor_elastic_snap_overshoot = overshoot;
        self.cursor_elastic_snap_duration_ms = duration_ms;
    }

    /// Update frost border config
    pub fn set_frost_border_effect(&mut self, enabled: bool, color: (f32, f32, f32), width: f32, opacity: f32) {
        self.frost_border_enabled = enabled;
        self.frost_border_color = color;
        self.frost_border_width = width;
        self.frost_border_opacity = opacity;
    }

    /// Update cursor ghost config
    pub fn set_cursor_ghost(&mut self, enabled: bool, color: (f32, f32, f32), count: u32, fade_ms: u32, drift: f32, opacity: f32) {
        self.cursor_ghost_enabled = enabled;
        self.cursor_ghost_color = color;
        self.cursor_ghost_count = count;
        self.cursor_ghost_fade_ms = fade_ms;
        self.cursor_ghost_drift = drift;
        self.cursor_ghost_opacity = opacity;
    }

    /// Update edge glow config
    pub fn set_edge_glow(&mut self, enabled: bool, color: (f32, f32, f32), height: f32, opacity: f32, fade_ms: u32) {
        self.edge_glow_enabled = enabled;
        self.edge_glow_color = color;
        self.edge_glow_height = height;
        self.edge_glow_opacity = opacity;
        self.edge_glow_fade_ms = fade_ms;
    }

    /// Trigger edge glow for a window (at_top = beginning-of-buffer)
    pub fn trigger_edge_glow(&mut self, window_id: i64, bounds: Rect, at_top: bool, now: std::time::Instant) {
        self.edge_glow_entries.retain(|e| e.window_id != window_id || e.at_top != at_top);
        self.edge_glow_entries.push(EdgeGlowEntry {
            window_id,
            bounds,
            at_top,
            started: now,
            duration: std::time::Duration::from_millis(self.edge_glow_fade_ms as u64),
        });
    }

    /// Update rain effect config
    pub fn set_rain_effect(&mut self, enabled: bool, color: (f32, f32, f32), drop_count: u32, speed: f32, opacity: f32) {
        self.rain_effect_enabled = enabled;
        self.rain_effect_color = color;
        self.rain_effect_drop_count = drop_count;
        self.rain_effect_speed = speed;
        self.rain_effect_opacity = opacity;
        if !enabled {
            self.rain_drops.clear();
        }
    }

    /// Update cursor ripple wave config
    pub fn set_cursor_ripple_wave(&mut self, enabled: bool, color: (f32, f32, f32), ring_count: u32, max_radius: f32, duration_ms: u32, opacity: f32) {
        self.cursor_ripple_wave_enabled = enabled;
        self.cursor_ripple_wave_color = color;
        self.cursor_ripple_wave_ring_count = ring_count;
        self.cursor_ripple_wave_max_radius = max_radius;
        self.cursor_ripple_wave_duration_ms = duration_ms;
        self.cursor_ripple_wave_opacity = opacity;
    }

    /// Update aurora config
    pub fn set_aurora(&mut self, enabled: bool, color1: (f32, f32, f32), color2: (f32, f32, f32), height: f32, speed: f32, opacity: f32) {
        self.aurora_enabled = enabled;
        self.aurora_color1 = color1;
        self.aurora_color2 = color2;
        self.aurora_height = height;
        self.aurora_speed = speed;
        self.aurora_opacity = opacity;
    }

    /// Update heat distortion config
    pub fn set_heat_distortion(&mut self, enabled: bool, intensity: f32, speed: f32, edge_width: f32, opacity: f32) {
        self.heat_distortion_enabled = enabled;
        self.heat_distortion_intensity = intensity;
        self.heat_distortion_speed = speed;
        self.heat_distortion_edge_width = edge_width;
        self.heat_distortion_opacity = opacity;
    }

    /// Update cursor lighthouse config
    pub fn set_cursor_lighthouse(&mut self, enabled: bool, color: (f32, f32, f32), beam_width: f32, rotation_speed: f32, beam_length: f32, opacity: f32) {
        self.cursor_lighthouse_enabled = enabled;
        self.cursor_lighthouse_color = color;
        self.cursor_lighthouse_beam_width = beam_width;
        self.cursor_lighthouse_rotation_speed = rotation_speed;
        self.cursor_lighthouse_beam_length = beam_length;
        self.cursor_lighthouse_opacity = opacity;
    }

    /// Update neon border config
    pub fn set_neon_border(&mut self, enabled: bool, color: (f32, f32, f32), intensity: f32, flicker: f32, thickness: f32, opacity: f32) {
        self.neon_border_enabled = enabled;
        self.neon_border_color = color;
        self.neon_border_intensity = intensity;
        self.neon_border_flicker = flicker;
        self.neon_border_thickness = thickness;
        self.neon_border_opacity = opacity;
    }

    /// Update cursor sonar ping config
    pub fn set_cursor_sonar_ping(&mut self, enabled: bool, color: (f32, f32, f32), ring_count: u32, max_radius: f32, duration_ms: u32, opacity: f32) {
        self.cursor_sonar_ping_enabled = enabled;
        self.cursor_sonar_ping_color = color;
        self.cursor_sonar_ping_ring_count = ring_count;
        self.cursor_sonar_ping_max_radius = max_radius;
        self.cursor_sonar_ping_duration_ms = duration_ms;
        self.cursor_sonar_ping_opacity = opacity;
    }

    /// Trigger a sonar ping at cursor position
    pub fn trigger_sonar_ping(&mut self, cx: f32, cy: f32, now: std::time::Instant) {
        self.cursor_sonar_ping_entries.push(SonarPingEntry {
            cx,
            cy,
            started: now,
            duration: std::time::Duration::from_millis(self.cursor_sonar_ping_duration_ms as u64),
        });
    }

    /// Update lightning bolt config
    pub fn set_lightning_bolt(&mut self, enabled: bool, color: (f32, f32, f32), frequency: f32, intensity: f32, opacity: f32) {
        self.lightning_bolt_enabled = enabled;
        self.lightning_bolt_color = color;
        self.lightning_bolt_frequency = frequency;
        self.lightning_bolt_intensity = intensity;
        self.lightning_bolt_opacity = opacity;
        if !enabled {
            self.lightning_bolt_segments.clear();
        }
    }

    /// Update cursor orbit particles config
    pub fn set_cursor_orbit_particles(&mut self, enabled: bool, color: (f32, f32, f32), count: u32, radius: f32, speed: f32, opacity: f32) {
        self.cursor_orbit_particles_enabled = enabled;
        self.cursor_orbit_particles_color = color;
        self.cursor_orbit_particles_count = count;
        self.cursor_orbit_particles_radius = radius;
        self.cursor_orbit_particles_speed = speed;
        self.cursor_orbit_particles_opacity = opacity;
    }

    /// Update plasma border config
    pub fn set_plasma_border(&mut self, enabled: bool, color1: (f32, f32, f32), color2: (f32, f32, f32), width: f32, speed: f32, opacity: f32) {
        self.plasma_border_enabled = enabled;
        self.plasma_border_color1 = color1;
        self.plasma_border_color2 = color2;
        self.plasma_border_width = width;
        self.plasma_border_speed = speed;
        self.plasma_border_opacity = opacity;
    }

    /// Update cursor heartbeat config
    pub fn set_cursor_heartbeat(&mut self, enabled: bool, color: (f32, f32, f32), bpm: f32, max_radius: f32, opacity: f32) {
        self.cursor_heartbeat_enabled = enabled;
        self.cursor_heartbeat_color = color;
        self.cursor_heartbeat_bpm = bpm;
        self.cursor_heartbeat_max_radius = max_radius;
        self.cursor_heartbeat_opacity = opacity;
    }

    /// Update warp grid config
    pub fn set_warp_grid(&mut self, enabled: bool, color: (f32, f32, f32), density: u32, amplitude: f32, speed: f32, opacity: f32) {
        self.warp_grid_enabled = enabled;
        self.warp_grid_color = color;
        self.warp_grid_density = density;
        self.warp_grid_amplitude = amplitude;
        self.warp_grid_speed = speed;
        self.warp_grid_opacity = opacity;
    }

    /// Update cursor DNA helix config
    pub fn set_cursor_dna_helix(&mut self, enabled: bool, color1: (f32, f32, f32), color2: (f32, f32, f32), radius: f32, speed: f32, opacity: f32) {
        self.cursor_dna_helix_enabled = enabled;
        self.cursor_dna_helix_color1 = color1;
        self.cursor_dna_helix_color2 = color2;
        self.cursor_dna_helix_radius = radius;
        self.cursor_dna_helix_speed = speed;
        self.cursor_dna_helix_opacity = opacity;
    }

    /// Update prism edge config
    pub fn set_prism_edge(&mut self, enabled: bool, width: f32, speed: f32, saturation: f32, opacity: f32) {
        self.prism_edge_enabled = enabled;
        self.prism_edge_width = width;
        self.prism_edge_speed = speed;
        self.prism_edge_saturation = saturation;
        self.prism_edge_opacity = opacity;
    }

    /// Update cursor pendulum config
    pub fn set_cursor_pendulum(&mut self, enabled: bool, color: (f32, f32, f32), arc_length: f32, damping: f32, opacity: f32) {
        self.cursor_pendulum_enabled = enabled;
        self.cursor_pendulum_color = color;
        self.cursor_pendulum_arc_length = arc_length;
        self.cursor_pendulum_damping = damping;
        self.cursor_pendulum_opacity = opacity;
    }

    /// Update topo contour config
    pub fn set_topo_contour(&mut self, enabled: bool, color: (f32, f32, f32), spacing: f32, speed: f32, opacity: f32) {
        self.topo_contour_enabled = enabled;
        self.topo_contour_color = color;
        self.topo_contour_spacing = spacing;
        self.topo_contour_speed = speed;
        self.topo_contour_opacity = opacity;
    }

    /// Update cursor metronome config
    pub fn set_cursor_metronome(&mut self, enabled: bool, color: (f32, f32, f32), tick_height: f32, fade_ms: u32, opacity: f32) {
        self.cursor_metronome_enabled = enabled;
        self.cursor_metronome_color = color;
        self.cursor_metronome_tick_height = tick_height;
        self.cursor_metronome_fade_ms = fade_ms;
        self.cursor_metronome_opacity = opacity;
    }

    /// Update constellation config
    pub fn set_constellation(&mut self, enabled: bool, color: (f32, f32, f32), star_count: u32, connect_dist: f32, twinkle_speed: f32, opacity: f32) {
        self.constellation_enabled = enabled;
        self.constellation_color = color;
        self.constellation_star_count = star_count;
        self.constellation_connect_dist = connect_dist;
        self.constellation_twinkle_speed = twinkle_speed;
        self.constellation_opacity = opacity;
    }

    /// Update cursor radar config
    pub fn set_cursor_radar(&mut self, enabled: bool, color: (f32, f32, f32), radius: f32, speed: f32, opacity: f32) {
        self.cursor_radar_enabled = enabled;
        self.cursor_radar_color = color;
        self.cursor_radar_radius = radius;
        self.cursor_radar_speed = speed;
        self.cursor_radar_opacity = opacity;
    }

    /// Update kaleidoscope config
    pub fn set_kaleidoscope(&mut self, enabled: bool, color: (f32, f32, f32), segments: u32, speed: f32, opacity: f32) {
        self.kaleidoscope_enabled = enabled;
        self.kaleidoscope_color = color;
        self.kaleidoscope_segments = segments;
        self.kaleidoscope_speed = speed;
        self.kaleidoscope_opacity = opacity;
    }

    /// Update cursor ripple ring config
    pub fn set_cursor_ripple_ring(&mut self, enabled: bool, color: (f32, f32, f32), max_radius: f32, ring_count: u32, speed: f32, opacity: f32) {
        self.cursor_ripple_ring_enabled = enabled;
        self.cursor_ripple_ring_color = color;
        self.cursor_ripple_ring_max_radius = max_radius;
        self.cursor_ripple_ring_count = ring_count;
        self.cursor_ripple_ring_speed = speed;
        self.cursor_ripple_ring_opacity = opacity;
    }

    /// Update noise field config
    pub fn set_noise_field(&mut self, enabled: bool, color: (f32, f32, f32), scale: f32, speed: f32, opacity: f32) {
        self.noise_field_enabled = enabled;
        self.noise_field_color = color;
        self.noise_field_scale = scale;
        self.noise_field_speed = speed;
        self.noise_field_opacity = opacity;
    }

    /// Update cursor scope config
    pub fn set_cursor_scope(&mut self, enabled: bool, color: (f32, f32, f32), thickness: f32, gap: f32, opacity: f32) {
        self.cursor_scope_enabled = enabled;
        self.cursor_scope_color = color;
        self.cursor_scope_thickness = thickness;
        self.cursor_scope_gap = gap;
        self.cursor_scope_opacity = opacity;
    }

    /// Update spiral vortex config
    pub fn set_spiral_vortex(&mut self, enabled: bool, color: (f32, f32, f32), arms: u32, speed: f32, opacity: f32) {
        self.spiral_vortex_enabled = enabled;
        self.spiral_vortex_color = color;
        self.spiral_vortex_arms = arms;
        self.spiral_vortex_speed = speed;
        self.spiral_vortex_opacity = opacity;
    }

    /// Update cursor shockwave config
    pub fn set_cursor_shockwave(&mut self, enabled: bool, color: (f32, f32, f32), radius: f32, decay: f32, opacity: f32) {
        self.cursor_shockwave_enabled = enabled;
        self.cursor_shockwave_color = color;
        self.cursor_shockwave_radius = radius;
        self.cursor_shockwave_decay = decay;
        self.cursor_shockwave_opacity = opacity;
    }

    /// Update diamond lattice config
    pub fn set_diamond_lattice(&mut self, enabled: bool, color: (f32, f32, f32), cell_size: f32, shimmer_speed: f32, opacity: f32) {
        self.diamond_lattice_enabled = enabled;
        self.diamond_lattice_color = color;
        self.diamond_lattice_cell_size = cell_size;
        self.diamond_lattice_shimmer_speed = shimmer_speed;
        self.diamond_lattice_opacity = opacity;
    }

    /// Update cursor gravity well config
    pub fn set_cursor_gravity_well(&mut self, enabled: bool, color: (f32, f32, f32), field_radius: f32, line_count: u32, opacity: f32) {
        self.cursor_gravity_well_enabled = enabled;
        self.cursor_gravity_well_color = color;
        self.cursor_gravity_well_field_radius = field_radius;
        self.cursor_gravity_well_line_count = line_count;
        self.cursor_gravity_well_opacity = opacity;
    }

    /// Update wave interference config
    pub fn set_wave_interference(&mut self, enabled: bool, color: (f32, f32, f32), wavelength: f32, source_count: u32, speed: f32, opacity: f32) {
        self.wave_interference_enabled = enabled;
        self.wave_interference_color = color;
        self.wave_interference_wavelength = wavelength;
        self.wave_interference_source_count = source_count;
        self.wave_interference_speed = speed;
        self.wave_interference_opacity = opacity;
    }

    /// Update cursor portal config
    pub fn set_cursor_portal(&mut self, enabled: bool, color: (f32, f32, f32), radius: f32, speed: f32, opacity: f32) {
        self.cursor_portal_enabled = enabled;
        self.cursor_portal_color = color;
        self.cursor_portal_radius = radius;
        self.cursor_portal_speed = speed;
        self.cursor_portal_opacity = opacity;
    }

    /// Update chevron pattern config
    pub fn set_chevron_pattern(&mut self, enabled: bool, color: (f32, f32, f32), spacing: f32, speed: f32, opacity: f32) {
        self.chevron_pattern_enabled = enabled;
        self.chevron_pattern_color = color;
        self.chevron_pattern_spacing = spacing;
        self.chevron_pattern_speed = speed;
        self.chevron_pattern_opacity = opacity;
    }

    /// Update cursor bubble config
    pub fn set_cursor_bubble(&mut self, enabled: bool, color: (f32, f32, f32), count: u32, rise_speed: f32, opacity: f32) {
        self.cursor_bubble_enabled = enabled;
        self.cursor_bubble_color = color;
        self.cursor_bubble_count = count;
        self.cursor_bubble_rise_speed = rise_speed;
        self.cursor_bubble_opacity = opacity;
    }

    /// Update sunburst pattern config
    pub fn set_sunburst_pattern(&mut self, enabled: bool, color: (f32, f32, f32), ray_count: u32, speed: f32, opacity: f32) {
        self.sunburst_pattern_enabled = enabled;
        self.sunburst_pattern_color = color;
        self.sunburst_pattern_ray_count = ray_count;
        self.sunburst_pattern_speed = speed;
        self.sunburst_pattern_opacity = opacity;
    }

    /// Update cursor firework config
    pub fn set_cursor_firework(&mut self, enabled: bool, color: (f32, f32, f32), particle_count: u32, burst_radius: f32, opacity: f32) {
        self.cursor_firework_enabled = enabled;
        self.cursor_firework_color = color;
        self.cursor_firework_particle_count = particle_count;
        self.cursor_firework_burst_radius = burst_radius;
        self.cursor_firework_opacity = opacity;
    }

    /// Update honeycomb dissolve config
    pub fn set_honeycomb_dissolve(&mut self, enabled: bool, color: (f32, f32, f32), cell_size: f32, dissolve_speed: f32, opacity: f32) {
        self.honeycomb_dissolve_enabled = enabled;
        self.honeycomb_dissolve_color = color;
        self.honeycomb_dissolve_cell_size = cell_size;
        self.honeycomb_dissolve_speed = dissolve_speed;
        self.honeycomb_dissolve_opacity = opacity;
    }

    /// Update cursor tornado config
    pub fn set_cursor_tornado(&mut self, enabled: bool, color: (f32, f32, f32), radius: f32, particle_count: u32, opacity: f32) {
        self.cursor_tornado_enabled = enabled;
        self.cursor_tornado_color = color;
        self.cursor_tornado_radius = radius;
        self.cursor_tornado_particle_count = particle_count;
        self.cursor_tornado_opacity = opacity;
    }

    /// Update moiré pattern config
    pub fn set_moire_pattern(&mut self, enabled: bool, color: (f32, f32, f32), line_spacing: f32, angle_offset: f32, speed: f32, opacity: f32) {
        self.moire_pattern_enabled = enabled;
        self.moire_pattern_color = color;
        self.moire_pattern_line_spacing = line_spacing;
        self.moire_pattern_angle_offset = angle_offset;
        self.moire_pattern_speed = speed;
        self.moire_pattern_opacity = opacity;
    }

    /// Update cursor lightning config
    pub fn set_cursor_lightning(&mut self, enabled: bool, color: (f32, f32, f32), bolt_count: u32, max_length: f32, opacity: f32) {
        self.cursor_lightning_enabled = enabled;
        self.cursor_lightning_color = color;
        self.cursor_lightning_bolt_count = bolt_count;
        self.cursor_lightning_max_length = max_length;
        self.cursor_lightning_opacity = opacity;
    }

    /// Update dot matrix config
    pub fn set_dot_matrix(&mut self, enabled: bool, color: (f32, f32, f32), dot_spacing: f32, pulse_speed: f32, opacity: f32) {
        self.dot_matrix_enabled = enabled;
        self.dot_matrix_color = color;
        self.dot_matrix_spacing = dot_spacing;
        self.dot_matrix_pulse_speed = pulse_speed;
        self.dot_matrix_opacity = opacity;
    }

    /// Update cursor snowflake config
    pub fn set_cursor_snowflake(&mut self, enabled: bool, color: (f32, f32, f32), count: u32, fall_speed: f32, opacity: f32) {
        self.cursor_snowflake_enabled = enabled;
        self.cursor_snowflake_color = color;
        self.cursor_snowflake_count = count;
        self.cursor_snowflake_fall_speed = fall_speed;
        self.cursor_snowflake_opacity = opacity;
    }

    /// Update concentric rings config
    pub fn set_concentric_rings(&mut self, enabled: bool, color: (f32, f32, f32), ring_spacing: f32, expansion_speed: f32, opacity: f32) {
        self.concentric_rings_enabled = enabled;
        self.concentric_rings_color = color;
        self.concentric_rings_spacing = ring_spacing;
        self.concentric_rings_expansion_speed = expansion_speed;
        self.concentric_rings_opacity = opacity;
    }

    /// Update cursor flame config
    pub fn set_cursor_flame(&mut self, enabled: bool, color: (f32, f32, f32), particle_count: u32, height: f32, opacity: f32) {
        self.cursor_flame_enabled = enabled;
        self.cursor_flame_color = color;
        self.cursor_flame_particle_count = particle_count;
        self.cursor_flame_height = height;
        self.cursor_flame_opacity = opacity;
    }

    /// Update zigzag pattern config
    pub fn set_zigzag_pattern(&mut self, enabled: bool, color: (f32, f32, f32), amplitude: f32, frequency: f32, speed: f32, opacity: f32) {
        self.zigzag_pattern_enabled = enabled;
        self.zigzag_pattern_color = color;
        self.zigzag_pattern_amplitude = amplitude;
        self.zigzag_pattern_frequency = frequency;
        self.zigzag_pattern_speed = speed;
        self.zigzag_pattern_opacity = opacity;
    }

    /// Update cursor crystal config
    pub fn set_cursor_crystal(&mut self, enabled: bool, color: (f32, f32, f32), facet_count: u32, radius: f32, opacity: f32) {
        self.cursor_crystal_enabled = enabled;
        self.cursor_crystal_color = color;
        self.cursor_crystal_facet_count = facet_count;
        self.cursor_crystal_radius = radius;
        self.cursor_crystal_opacity = opacity;
    }

    /// Update basket weave config
    pub fn set_basket_weave(&mut self, enabled: bool, color: (f32, f32, f32), strip_width: f32, strip_spacing: f32, opacity: f32) {
        self.basket_weave_enabled = enabled;
        self.basket_weave_color = color;
        self.basket_weave_strip_width = strip_width;
        self.basket_weave_strip_spacing = strip_spacing;
        self.basket_weave_opacity = opacity;
    }

    /// Update cursor sparkler config
    pub fn set_cursor_sparkler(&mut self, enabled: bool, color: (f32, f32, f32), spark_count: u32, burn_speed: f32, opacity: f32) {
        self.cursor_sparkler_enabled = enabled;
        self.cursor_sparkler_color = color;
        self.cursor_sparkler_spark_count = spark_count;
        self.cursor_sparkler_burn_speed = burn_speed;
        self.cursor_sparkler_opacity = opacity;
    }

    /// Update fish scale config
    pub fn set_fish_scale(&mut self, enabled: bool, color: (f32, f32, f32), scale_size: f32, row_offset: f32, opacity: f32) {
        self.fish_scale_enabled = enabled;
        self.fish_scale_color = color;
        self.fish_scale_size = scale_size;
        self.fish_scale_row_offset = row_offset;
        self.fish_scale_opacity = opacity;
    }

    /// Update cursor plasma ball config
    pub fn set_cursor_plasma_ball(&mut self, enabled: bool, color: (f32, f32, f32), tendril_count: u32, arc_speed: f32, opacity: f32) {
        self.cursor_plasma_ball_enabled = enabled;
        self.cursor_plasma_ball_color = color;
        self.cursor_plasma_ball_tendril_count = tendril_count;
        self.cursor_plasma_ball_arc_speed = arc_speed;
        self.cursor_plasma_ball_opacity = opacity;
    }

    /// Update trefoil knot config
    pub fn set_trefoil_knot(&mut self, enabled: bool, color: (f32, f32, f32), knot_size: f32, rotation_speed: f32, opacity: f32) {
        self.trefoil_knot_enabled = enabled;
        self.trefoil_knot_color = color;
        self.trefoil_knot_size = knot_size;
        self.trefoil_knot_rotation_speed = rotation_speed;
        self.trefoil_knot_opacity = opacity;
    }

    /// Update cursor quill pen config
    pub fn set_cursor_quill_pen(&mut self, enabled: bool, color: (f32, f32, f32), trail_length: u32, ink_speed: f32, opacity: f32) {
        self.cursor_quill_pen_enabled = enabled;
        self.cursor_quill_pen_color = color;
        self.cursor_quill_pen_trail_length = trail_length;
        self.cursor_quill_pen_ink_speed = ink_speed;
        self.cursor_quill_pen_opacity = opacity;
    }

    /// Update herringbone pattern config
    pub fn set_herringbone_pattern(&mut self, enabled: bool, color: (f32, f32, f32), tile_width: f32, tile_height: f32, opacity: f32) {
        self.herringbone_pattern_enabled = enabled;
        self.herringbone_pattern_color = color;
        self.herringbone_pattern_tile_width = tile_width;
        self.herringbone_pattern_tile_height = tile_height;
        self.herringbone_pattern_opacity = opacity;
    }

    /// Update cursor aurora borealis config
    pub fn set_cursor_aurora_borealis(&mut self, enabled: bool, color: (f32, f32, f32), band_count: u32, shimmer_speed: f32, opacity: f32) {
        self.cursor_aurora_borealis_enabled = enabled;
        self.cursor_aurora_borealis_color = color;
        self.cursor_aurora_borealis_band_count = band_count;
        self.cursor_aurora_borealis_shimmer_speed = shimmer_speed;
        self.cursor_aurora_borealis_opacity = opacity;
    }

    /// Update target reticle config
    pub fn set_target_reticle(&mut self, enabled: bool, color: (f32, f32, f32), ring_count: u32, pulse_speed: f32, opacity: f32) {
        self.target_reticle_enabled = enabled;
        self.target_reticle_color = color;
        self.target_reticle_ring_count = ring_count;
        self.target_reticle_pulse_speed = pulse_speed;
        self.target_reticle_opacity = opacity;
    }

    /// Update cursor feather config
    pub fn set_cursor_feather(&mut self, enabled: bool, color: (f32, f32, f32), feather_count: u32, drift_speed: f32, opacity: f32) {
        self.cursor_feather_enabled = enabled;
        self.cursor_feather_color = color;
        self.cursor_feather_count = feather_count;
        self.cursor_feather_drift_speed = drift_speed;
        self.cursor_feather_opacity = opacity;
    }

    /// Update plaid pattern config
    pub fn set_plaid_pattern(&mut self, enabled: bool, color: (f32, f32, f32), band_width: f32, band_spacing: f32, opacity: f32) {
        self.plaid_pattern_enabled = enabled;
        self.plaid_pattern_color = color;
        self.plaid_pattern_band_width = band_width;
        self.plaid_pattern_band_spacing = band_spacing;
        self.plaid_pattern_opacity = opacity;
    }

    /// Update cursor stardust config
    pub fn set_cursor_stardust(&mut self, enabled: bool, color: (f32, f32, f32), particle_count: u32, fall_speed: f32, opacity: f32) {
        self.cursor_stardust_enabled = enabled;
        self.cursor_stardust_color = color;
        self.cursor_stardust_particle_count = particle_count;
        self.cursor_stardust_fall_speed = fall_speed;
        self.cursor_stardust_opacity = opacity;
    }

    /// Update brick wall config
    pub fn set_brick_wall(&mut self, enabled: bool, color: (f32, f32, f32), brick_width: f32, brick_height: f32, opacity: f32) {
        self.brick_wall_enabled = enabled;
        self.brick_wall_color = color;
        self.brick_wall_width = brick_width;
        self.brick_wall_height = brick_height;
        self.brick_wall_opacity = opacity;
    }

    /// Update cursor compass needle config
    pub fn set_cursor_compass_needle(&mut self, enabled: bool, color: (f32, f32, f32), needle_length: f32, spin_speed: f32, opacity: f32) {
        self.cursor_compass_needle_enabled = enabled;
        self.cursor_compass_needle_color = color;
        self.cursor_compass_needle_length = needle_length;
        self.cursor_compass_needle_spin_speed = spin_speed;
        self.cursor_compass_needle_opacity = opacity;
    }

    /// Update sine wave config
    pub fn set_sine_wave(&mut self, enabled: bool, color: (f32, f32, f32), amplitude: f32, wavelength: f32, speed: f32, opacity: f32) {
        self.sine_wave_enabled = enabled;
        self.sine_wave_color = color;
        self.sine_wave_amplitude = amplitude;
        self.sine_wave_wavelength = wavelength;
        self.sine_wave_speed = speed;
        self.sine_wave_opacity = opacity;
    }

    /// Update cursor galaxy config
    pub fn set_cursor_galaxy(&mut self, enabled: bool, color: (f32, f32, f32), star_count: u32, radius: f32, opacity: f32) {
        self.cursor_galaxy_enabled = enabled;
        self.cursor_galaxy_color = color;
        self.cursor_galaxy_star_count = star_count;
        self.cursor_galaxy_radius = radius;
        self.cursor_galaxy_opacity = opacity;
    }

    /// Update rotating gear config
    pub fn set_rotating_gear(&mut self, enabled: bool, color: (f32, f32, f32), gear_size: f32, rotation_speed: f32, opacity: f32) {
        self.rotating_gear_enabled = enabled;
        self.rotating_gear_color = color;
        self.rotating_gear_size = gear_size;
        self.rotating_gear_speed = rotation_speed;
        self.rotating_gear_opacity = opacity;
    }

    /// Update cursor prism config
    pub fn set_cursor_prism(&mut self, enabled: bool, color: (f32, f32, f32), ray_count: u32, spread: f32, opacity: f32) {
        self.cursor_prism_enabled = enabled;
        self.cursor_prism_color = color;
        self.cursor_prism_ray_count = ray_count;
        self.cursor_prism_spread = spread;
        self.cursor_prism_opacity = opacity;
    }

    /// Update crosshatch pattern config
    pub fn set_crosshatch_pattern(&mut self, enabled: bool, color: (f32, f32, f32), line_spacing: f32, angle: f32, speed: f32, opacity: f32) {
        self.crosshatch_pattern_enabled = enabled;
        self.crosshatch_pattern_color = color;
        self.crosshatch_pattern_line_spacing = line_spacing;
        self.crosshatch_pattern_angle = angle;
        self.crosshatch_pattern_speed = speed;
        self.crosshatch_pattern_opacity = opacity;
    }

    /// Update cursor moth config
    pub fn set_cursor_moth(&mut self, enabled: bool, color: (f32, f32, f32), moth_count: u32, wing_size: f32, opacity: f32) {
        self.cursor_moth_enabled = enabled;
        self.cursor_moth_color = color;
        self.cursor_moth_count = moth_count;
        self.cursor_moth_wing_size = wing_size;
        self.cursor_moth_opacity = opacity;
    }

    /// Update hex grid config
    pub fn set_hex_grid(&mut self, enabled: bool, color: (f32, f32, f32), cell_size: f32, pulse_speed: f32, opacity: f32) {
        self.hex_grid_enabled = enabled;
        self.hex_grid_color = color;
        self.hex_grid_cell_size = cell_size;
        self.hex_grid_pulse_speed = pulse_speed;
        self.hex_grid_opacity = opacity;
    }

    /// Update cursor sparkle burst config
    pub fn set_cursor_sparkle_burst(&mut self, enabled: bool, color: (f32, f32, f32), count: u32, radius: f32, opacity: f32) {
        self.cursor_sparkle_burst_enabled = enabled;
        self.cursor_sparkle_burst_color = color;
        self.cursor_sparkle_burst_count = count;
        self.cursor_sparkle_burst_radius = radius;
        self.cursor_sparkle_burst_opacity = opacity;
    }

    /// Update circuit trace config
    pub fn set_circuit_trace(&mut self, enabled: bool, color: (f32, f32, f32), width: f32, speed: f32, opacity: f32) {
        self.circuit_trace_enabled = enabled;
        self.circuit_trace_color = color;
        self.circuit_trace_width = width;
        self.circuit_trace_speed = speed;
        self.circuit_trace_opacity = opacity;
    }

    /// Update cursor compass config
    pub fn set_cursor_compass(&mut self, enabled: bool, color: (f32, f32, f32), size: f32, speed: f32, opacity: f32) {
        self.cursor_compass_enabled = enabled;
        self.cursor_compass_color = color;
        self.cursor_compass_size = size;
        self.cursor_compass_speed = speed;
        self.cursor_compass_opacity = opacity;
    }

    /// Update mode-line transition config
    pub fn set_mode_line_transition(&mut self, enabled: bool, duration_ms: u32) {
        self.mode_line_transition_enabled = enabled;
        self.mode_line_transition_duration_ms = duration_ms;
    }

    /// Get the mode-line transition alpha for a glyph at (x, y)
    fn mode_line_fade_alpha(&self, gx: f32, gy: f32) -> f32 {
        if !self.mode_line_transition_enabled || self.active_mode_line_fades.is_empty() {
            return 1.0;
        }
        let now = std::time::Instant::now();
        for entry in &self.active_mode_line_fades {
            if gx >= entry.bounds_x && gx < entry.bounds_x + entry.bounds_w
                && gy >= entry.mode_line_y && gy < entry.mode_line_y + entry.mode_line_h
            {
                let elapsed = now.duration_since(entry.started).as_secs_f32();
                let total = entry.duration.as_secs_f32();
                if elapsed < total {
                    let t = elapsed / total;
                    return t; // linear fade-in
                }
            }
        }
        1.0
    }

    /// Update text fade-in config
    pub fn set_text_fade_in(&mut self, enabled: bool, duration_ms: u32) {
        self.text_fade_in_enabled = enabled;
        self.text_fade_in_duration_ms = duration_ms;
    }

    /// Trigger a text fade-in animation for a window
    pub fn trigger_text_fade_in(&mut self, window_id: i64, bounds: Rect, now: std::time::Instant) {
        // Replace existing animation for this window
        self.active_text_fades.retain(|e| e.window_id != window_id);
        self.active_text_fades.push(TextFadeEntry {
            window_id,
            bounds,
            started: now,
            duration: std::time::Duration::from_millis(self.text_fade_in_duration_ms as u64),
        });
        self.needs_continuous_redraw = true;
    }

    /// Get the text fade-in alpha multiplier for a glyph at (x, y).
    /// Returns 1.0 if no fade is active, or 0.0-1.0 during fade-in.
    fn text_fade_alpha(&self, gx: f32, gy: f32) -> f32 {
        if !self.text_fade_in_enabled || self.active_text_fades.is_empty() {
            return 1.0;
        }
        let now = std::time::Instant::now();
        for entry in &self.active_text_fades {
            let b = &entry.bounds;
            if gx >= b.x && gx < b.x + b.width
                && gy >= b.y && gy < b.y + b.height
            {
                let elapsed = now.duration_since(entry.started).as_secs_f32();
                let total = entry.duration.as_secs_f32();
                if elapsed < total {
                    // Ease-in: start at 0, end at 1
                    let t = elapsed / total;
                    return t * t; // quadratic ease-in for smooth appearance
                }
            }
        }
        1.0
    }

    /// Update scroll line spacing config
    pub fn set_scroll_line_spacing(&mut self, enabled: bool, max_spacing: f32, duration_ms: u32) {
        self.scroll_line_spacing_enabled = enabled;
        self.scroll_line_spacing_max = max_spacing;
        self.scroll_line_spacing_duration_ms = duration_ms;
    }

    /// Trigger a scroll line spacing animation for a window
    pub fn trigger_scroll_line_spacing(&mut self, window_id: i64, bounds: Rect, direction: i32, now: std::time::Instant) {
        // Replace existing animation for this window
        self.active_scroll_spacings.retain(|e| e.window_id != window_id);
        self.active_scroll_spacings.push(ScrollSpacingEntry {
            window_id,
            bounds,
            direction,
            started: now,
            duration: std::time::Duration::from_millis(self.scroll_line_spacing_duration_ms as u64),
        });
        self.needs_continuous_redraw = true;
    }

    /// Update focus ring config
    pub fn set_focus_ring(&mut self, enabled: bool, color: (f32, f32, f32), opacity: f32, dash_length: f32, speed: f32) {
        self.focus_ring_enabled = enabled;
        self.focus_ring_color = color;
        self.focus_ring_opacity = opacity;
        self.focus_ring_dash_length = dash_length.max(2.0);
        self.focus_ring_speed = speed;
        if enabled {
            self.focus_ring_start = std::time::Instant::now();
        }
    }

    /// Update window mode tint config
    pub fn set_window_mode_tint(&mut self, enabled: bool, opacity: f32) {
        self.window_mode_tint_enabled = enabled;
        self.window_mode_tint_opacity = opacity;
    }

    /// Update window watermark config
    pub fn set_window_watermark(&mut self, enabled: bool, opacity: f32, threshold: u32) {
        self.window_watermark_enabled = enabled;
        self.window_watermark_opacity = opacity;
        self.window_watermark_threshold = threshold;
    }

    /// Update cursor trail fade config
    pub fn set_cursor_trail_fade(&mut self, enabled: bool, length: usize, fade_ms: u32) {
        self.cursor_trail_fade_enabled = enabled;
        self.cursor_trail_fade_length = length;
        self.cursor_trail_fade_duration = std::time::Duration::from_millis(fade_ms as u64);
        if !enabled {
            self.cursor_trail_positions.clear();
        }
    }

    /// Record a new cursor position for the trail
    pub fn record_cursor_trail(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if !self.cursor_trail_fade_enabled { return; }
        let dist = ((x - self.cursor_trail_last_pos.0).powi(2)
            + (y - self.cursor_trail_last_pos.1).powi(2)).sqrt();
        if dist < 2.0 { return; } // Skip tiny movements
        self.cursor_trail_positions.push((x, y, w, h, std::time::Instant::now()));
        self.cursor_trail_last_pos = (x, y);
        // Trim to max length
        while self.cursor_trail_positions.len() > self.cursor_trail_fade_length {
            self.cursor_trail_positions.remove(0);
        }
    }

    /// Update region glow config
    pub fn set_region_glow(&mut self, enabled: bool, face_id: u32, radius: f32, opacity: f32) {
        self.region_glow_enabled = enabled;
        self.region_glow_face_id = face_id;
        self.region_glow_radius = radius;
        self.region_glow_opacity = opacity;
    }

    /// Update idle dim alpha
    pub fn set_idle_dim_alpha(&mut self, alpha: f32) {
        self.idle_dim_alpha = alpha;
    }

    /// Update noise grain config
    pub fn set_noise_grain(&mut self, enabled: bool, intensity: f32, size: f32) {
        self.noise_grain_enabled = enabled;
        self.noise_grain_intensity = intensity;
        self.noise_grain_size = size;
    }

    /// Update padding gradient config
    pub fn set_padding_gradient(&mut self, enabled: bool, color: (f32, f32, f32), opacity: f32, width: f32) {
        self.padding_gradient_enabled = enabled;
        self.padding_gradient_color = color;
        self.padding_gradient_opacity = opacity;
        self.padding_gradient_width = width;
    }

    /// Update frosted glass config
    pub fn set_frosted_glass(&mut self, enabled: bool, opacity: f32, blur: f32) {
        self.frosted_glass_enabled = enabled;
        self.frosted_glass_opacity = opacity;
        self.frosted_glass_blur = blur;
    }

    /// Update title fade config
    pub fn set_title_fade(&mut self, enabled: bool, duration_ms: u32) {
        self.title_fade_enabled = enabled;
        self.title_fade_duration_ms = duration_ms;
        if !enabled {
            self.active_title_fades.clear();
        }
    }

    /// Start a window switch fade for a specific window
    pub fn start_window_fade(&mut self, window_id: i64, bounds: Rect) {
        // Remove any existing fade for this window
        self.active_window_fades.retain(|f| f.window_id != window_id);
        self.active_window_fades.push(WindowFadeEntry {
            window_id,
            bounds,
            started: std::time::Instant::now(),
            duration: std::time::Duration::from_millis(self.window_switch_fade_duration_ms as u64),
            intensity: self.window_switch_fade_intensity,
        });
    }

    /// Convert HSL to sRGB Color
    /// Scale a rectangle from its center by a given factor
    fn scale_rect(x: f32, y: f32, w: f32, h: f32, scale: f32) -> (f32, f32, f32, f32) {
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        let nw = w * scale;
        let nh = h * scale;
        (cx - nw * 0.5, cy - nh * 0.5, nw, nh)
    }

    fn hsl_to_color(h: f32, s: f32, l: f32) -> Color {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;
        let (r, g, b) = match (h * 6.0) as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        Color { r: r + m, g: g + m, b: b + m, a: 1.0 }
    }

    /// Update header/mode-line shadow config
    pub fn set_header_shadow(&mut self, enabled: bool, intensity: f32, size: f32) {
        self.header_shadow_enabled = enabled;
        self.header_shadow_intensity = intensity;
        self.header_shadow_size = size;
    }

    /// Update search pulse config
    pub fn set_search_pulse(&mut self, enabled: bool, face_id: u32) {
        self.search_pulse_enabled = enabled;
        self.search_pulse_face_id = face_id;
        if enabled {
            self.search_pulse_start = std::time::Instant::now();
        }
    }

    /// Update typing ripple config
    pub fn set_typing_ripple(&mut self, enabled: bool, max_radius: f32, duration_ms: u32) {
        self.typing_ripple_enabled = enabled;
        self.typing_ripple_max_radius = max_radius;
        self.typing_ripple_duration = duration_ms as f32 / 1000.0;
        if !enabled {
            self.active_ripples.clear();
        }
    }

    /// Spawn a new ripple at the given position
    pub fn spawn_ripple(&mut self, cx: f32, cy: f32) {
        if self.typing_ripple_enabled {
            self.active_ripples.push((cx, cy, std::time::Instant::now()));
        }
    }

    /// Update visible whitespace config
    pub fn set_show_whitespace_config(&mut self, enabled: bool, color: (f32, f32, f32, f32)) {
        self.show_whitespace_enabled = enabled;
        self.show_whitespace_color = color;
    }

    /// Update line highlight config
    pub fn set_line_highlight_config(&mut self, enabled: bool, color: (f32, f32, f32, f32)) {
        self.line_highlight_enabled = enabled;
        self.line_highlight_color = color;
    }

    /// Update indent guide config
    pub fn set_indent_guide_config(&mut self, enabled: bool, color: (f32, f32, f32, f32)) {
        self.indent_guides_enabled = enabled;
        self.indent_guide_color = color;
    }

    /// Update rainbow indent guide config
    pub fn set_indent_guide_rainbow(&mut self, enabled: bool, colors: Vec<(f32, f32, f32, f32)>) {
        self.indent_guide_rainbow_enabled = enabled;
        self.indent_guide_rainbow_colors = colors;
    }

    /// Update scroll bar rendering config
    pub fn set_scroll_bar_config(&mut self, thumb_radius: f32, track_opacity: f32,
                                  hover_brightness: f32) {
        self.scroll_bar_thumb_radius = thumb_radius;
        self.scroll_bar_track_opacity = track_opacity;
        self.scroll_bar_hover_brightness = hover_brightness;
    }

    async fn new_async(
        surface: Option<wgpu::Surface<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        // Create wgpu instance
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Request adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: crate::gpu_power_preference(),
                compatible_surface: surface.as_ref(),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "Failed to find a suitable GPU adapter".to_string())?;

        // Request device and queue
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Neomacs Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .map_err(|e| format!("Failed to create device: {}", e))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Configure surface if provided and extract format
        let surface_format = surface.as_ref().map(|s| {
            let caps = s.get_capabilities(&adapter);
            caps.formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0])
        });

        // Use the internal helper for pipeline/buffer creation (1.0 scale for standalone usage)
        Ok(Self::create_renderer_internal(device, queue, surface, surface_format, width, height, 1.0))
    }

    /// Resize the renderer's surface.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.width = width;
        self.height = height;

        // Update surface configuration
        if let (Some(surface), Some(config)) = (&self.surface, &mut self.surface_config) {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }

        // Update uniform buffer with logical size so vertex positions from Emacs map correctly
        let logical_w = width as f32 / self.scale_factor;
        let logical_h = height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// Update the display scale factor (for multi-monitor DPI changes)
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor;
    }

    /// Get the glyph bind group layout for creating glyph bind groups
    pub fn glyph_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.glyph_bind_group_layout
    }

    /// Render a scene to the configured surface.
    pub fn render(&mut self, scene: &Scene) {
        let surface = match &self.surface {
            Some(s) => s,
            None => return,
        };

        let output = match surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Lost) => {
                self.resize(self.width, self.height);
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

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.render_to_view(&view, scene);

        output.present();
    }

    /// Render a scene to a texture view.
    pub fn render_to_view(&self, view: &wgpu::TextureView, scene: &Scene) {
        // Collect all rectangles to render
        let mut vertices: Vec<RectVertex> = Vec::new();

        // 1. Draw scene background
        self.add_rect(
            &mut vertices,
            0.0,
            0.0,
            scene.width,
            scene.height,
            &scene.background,
        );

        // 2. For each window: draw background, then cursor if visible
        for window in &scene.windows {
            // Window background
            self.add_rect(
                &mut vertices,
                window.bounds.x,
                window.bounds.y,
                window.bounds.width,
                window.bounds.height,
                &window.background,
            );

            // Cursor
            if let Some(cursor) = &window.cursor {
                if cursor.visible {
                    let cursor_x = window.bounds.x + cursor.x;
                    let cursor_y = window.bounds.y + cursor.y;

                    match cursor.style {
                        CursorStyle::Box => {
                            // Filled box cursor
                            self.add_rect(
                                &mut vertices,
                                cursor_x,
                                cursor_y,
                                cursor.width,
                                cursor.height,
                                &cursor.color,
                            );
                        }
                        CursorStyle::Bar => {
                            // Thin vertical bar
                            self.add_rect(
                                &mut vertices,
                                cursor_x,
                                cursor_y,
                                2.0, // Bar width
                                cursor.height,
                                &cursor.color,
                            );
                        }
                        CursorStyle::Underline => {
                            // Horizontal line at bottom
                            self.add_rect(
                                &mut vertices,
                                cursor_x,
                                cursor_y + cursor.height - 2.0,
                                cursor.width,
                                2.0, // Underline thickness
                                &cursor.color,
                            );
                        }
                        CursorStyle::Hollow => {
                            // Hollow box (4 lines forming a rectangle)
                            let thickness = 1.0;
                            // Top
                            self.add_rect(
                                &mut vertices,
                                cursor_x,
                                cursor_y,
                                cursor.width,
                                thickness,
                                &cursor.color,
                            );
                            // Bottom
                            self.add_rect(
                                &mut vertices,
                                cursor_x,
                                cursor_y + cursor.height - thickness,
                                cursor.width,
                                thickness,
                                &cursor.color,
                            );
                            // Left
                            self.add_rect(
                                &mut vertices,
                                cursor_x,
                                cursor_y,
                                thickness,
                                cursor.height,
                                &cursor.color,
                            );
                            // Right
                            self.add_rect(
                                &mut vertices,
                                cursor_x + cursor.width - thickness,
                                cursor_y,
                                thickness,
                                cursor.height,
                                &cursor.color,
                            );
                        }
                    }
                }
            }
        }

        // 3. Draw borders
        for border in &scene.borders {
            self.add_rect(
                &mut vertices,
                border.x,
                border.y,
                border.width,
                border.height,
                &border.color,
            );
        }

        // Skip rendering if there's nothing to draw
        if vertices.is_empty() {
            return;
        }

        // Create vertex buffer
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Rect Vertex Buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        // Create command encoder and render pass
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Rect Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: scene.background.r as f64,
                            g: scene.background.g as f64,
                            b: scene.background.b as f64,
                            a: scene.background.a as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.rect_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.draw(0..vertices.len() as u32, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a scene to an offscreen texture.
    pub fn render_to_texture(&self, scene: &Scene) -> wgpu::Texture {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Offscreen Texture"),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.render_to_view(&view, scene);

        texture
    }

    /// Add a rectangle to the vertex list (6 vertices = 2 triangles).
    fn add_rect(
        &self,
        vertices: &mut Vec<RectVertex>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: &Color,
    ) {
        let color_arr = [color.r, color.g, color.b, color.a];

        let x0 = x;
        let y0 = y;
        let x1 = x + width;
        let y1 = y + height;

        // First triangle (top-left, top-right, bottom-left)
        vertices.push(RectVertex {
            position: [x0, y0],
            color: color_arr,
        });
        vertices.push(RectVertex {
            position: [x1, y0],
            color: color_arr,
        });
        vertices.push(RectVertex {
            position: [x0, y1],
            color: color_arr,
        });

        // Second triangle (top-right, bottom-right, bottom-left)
        vertices.push(RectVertex {
            position: [x1, y0],
            color: color_arr,
        });
        vertices.push(RectVertex {
            position: [x1, y1],
            color: color_arr,
        });
        vertices.push(RectVertex {
            position: [x0, y1],
            color: color_arr,
        });
    }

    /// Emit a single rounded-rectangle border as 6 vertices (one oversized quad).
    ///
    /// The quad is padded by 1px on each side so the SDF fragment shader has
    /// room for anti-aliased edges.  The shader carves out the interior, leaving
    /// only the border ring with rounded corners.
    fn add_rounded_rect(
        &self,
        vertices: &mut Vec<RoundedRectVertex>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        border_width: f32,
        corner_radius: f32,
        color: &Color,
    ) {
        let padding = 1.0; // extra pixels for SDF anti-aliasing fringe
        let x0 = x - padding;
        let y0 = y - padding;
        let x1 = x + width + padding;
        let y1 = y + height + padding;

        let rect_min = [x, y];
        let rect_max = [x + width, y + height];
        let params = [border_width, corner_radius];
        let color_arr = [color.r, color.g, color.b, color.a];

        let v = |px: f32, py: f32| RoundedRectVertex {
            position: [px, py],
            color: color_arr,
            rect_min,
            rect_max,
            params,
        };

        // Two triangles forming the quad
        vertices.push(v(x0, y0));
        vertices.push(v(x1, y0));
        vertices.push(v(x0, y1));
        vertices.push(v(x1, y0));
        vertices.push(v(x1, y1));
        vertices.push(v(x0, y1));
    }

    /// Add an arbitrary quad (4 corners) to the vertex list (6 vertices = 2 triangles).
    /// Corners order: [TL, TR, BR, BL].
    fn add_quad(
        &self,
        vertices: &mut Vec<RectVertex>,
        corners: &[(f32, f32); 4],
        color: &Color,
    ) {
        let color_arr = [color.r, color.g, color.b, color.a];
        let [tl, tr, br, bl] = *corners;

        // Triangle 1: TL, TR, BL
        vertices.push(RectVertex { position: [tl.0, tl.1], color: color_arr });
        vertices.push(RectVertex { position: [tr.0, tr.1], color: color_arr });
        vertices.push(RectVertex { position: [bl.0, bl.1], color: color_arr });

        // Triangle 2: TR, BR, BL
        vertices.push(RectVertex { position: [tr.0, tr.1], color: color_arr });
        vertices.push(RectVertex { position: [br.0, br.1], color: color_arr });
        vertices.push(RectVertex { position: [bl.0, bl.1], color: color_arr });
    }

    /// Get the wgpu device.
    pub fn device(&self) -> &Arc<wgpu::Device> {
        &self.device
    }

    /// Get the wgpu queue.
    pub fn queue(&self) -> &Arc<wgpu::Queue> {
        &self.queue
    }

    /// Get the current width.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get the current height.
    pub fn height(&self) -> u32 {
        self.height
    }

    // =========== Image Loading Methods ===========

    /// Load image from file path (async - returns immediately)
    /// Returns image ID, actual texture loads in background
    pub fn load_image_file(&mut self, path: &str, max_width: u32, max_height: u32) -> u32 {
        self.image_cache.load_file(path, max_width, max_height)
    }

    /// Load image from file path with a pre-allocated ID (for threaded mode)
    pub fn load_image_file_with_id(&mut self, id: u32, path: &str, max_width: u32, max_height: u32) {
        self.image_cache.load_file_with_id(id, path, max_width, max_height)
    }

    /// Load image from data (async - returns immediately)
    pub fn load_image_data(&mut self, data: &[u8], max_width: u32, max_height: u32) -> u32 {
        self.image_cache.load_data(data, max_width, max_height)
    }

    /// Load image from raw ARGB32 pixel data
    pub fn load_image_argb32(&mut self, data: &[u8], width: u32, height: u32, stride: u32) -> u32 {
        self.image_cache.load_raw_argb32(data, width, height, stride, 0, 0)
    }

    /// Load image from raw RGB24 pixel data
    pub fn load_image_rgb24(&mut self, data: &[u8], width: u32, height: u32, stride: u32) -> u32 {
        self.image_cache.load_raw_rgb24(data, width, height, stride, 0, 0)
    }

    /// Query image file dimensions (fast - reads header only, does not block)
    pub fn query_image_file_size(path: &str) -> Option<(u32, u32)> {
        ImageCache::query_file_dimensions(path).map(|d| (d.width, d.height))
    }

    /// Query image data dimensions (fast - reads header only)
    pub fn query_image_data_size(data: &[u8]) -> Option<(u32, u32)> {
        ImageCache::query_data_dimensions(data).map(|d| (d.width, d.height))
    }

    /// Get image dimensions (works for pending and loaded images)
    pub fn get_image_size(&self, id: u32) -> Option<(u32, u32)> {
        self.image_cache.get_dimensions(id).map(|d| (d.width, d.height))
    }

    /// Check if image is ready for rendering
    pub fn is_image_ready(&self, id: u32) -> bool {
        self.image_cache.is_ready(id)
    }

    /// Free an image from cache
    pub fn free_image(&mut self, id: u32) {
        self.image_cache.free(id)
    }

    /// Process pending decoded images (call each frame before rendering)
    pub fn process_pending_images(&mut self) {
        self.image_cache.process_pending(&self.device, &self.queue);
    }

    // =========== Video Loading Methods ===========

    /// Load video from file path (async - returns immediately)
    /// Returns video ID, frames decode in background
    #[cfg(feature = "video")]
    pub fn load_video_file(&mut self, path: &str) -> u32 {
        self.video_cache.load_file(path)
    }

    /// Get video dimensions
    #[cfg(feature = "video")]
    pub fn get_video_size(&self, id: u32) -> Option<(u32, u32)> {
        self.video_cache.get_dimensions(id)
    }

    /// Get video state
    #[cfg(feature = "video")]
    pub fn get_video_state(&self, id: u32) -> Option<super::video_cache::VideoState> {
        self.video_cache.get_state(id)
    }

    /// Play video
    #[cfg(feature = "video")]
    pub fn video_play(&mut self, id: u32) {
        self.video_cache.play(id)
    }

    /// Pause video
    #[cfg(feature = "video")]
    pub fn video_pause(&mut self, id: u32) {
        self.video_cache.pause(id)
    }

    /// Stop video
    #[cfg(feature = "video")]
    pub fn video_stop(&mut self, id: u32) {
        self.video_cache.stop(id)
    }

    /// Set video loop count (-1 for infinite)
    #[cfg(feature = "video")]
    pub fn video_set_loop(&mut self, id: u32, count: i32) {
        self.video_cache.set_loop(id, count)
    }

    /// Free a video from cache
    #[cfg(feature = "video")]
    pub fn free_video(&mut self, id: u32) {
        self.video_cache.remove(id)
    }

    /// Process pending decoded video frames (call each frame before rendering)
    #[cfg(feature = "video")]
    pub fn process_pending_videos(&mut self) {
        log::debug!("process_pending_videos called");
        // Use image_cache's bind_group_layout and sampler to ensure video bind groups
        // are compatible with the shared image/video rendering pipeline
        let layout = self.image_cache.bind_group_layout();
        let sampler = self.image_cache.sampler();
        self.video_cache.process_pending(&self.device, &self.queue, layout, sampler);
    }

    /// Check if any video is currently playing
    #[cfg(feature = "video")]
    pub fn has_playing_videos(&self) -> bool {
        self.video_cache.has_playing_videos()
    }

    /// Get cached video for rendering
    #[cfg(feature = "video")]
    pub fn get_video(&self, id: u32) -> Option<&super::video_cache::CachedVideo> {
        self.video_cache.get(id)
    }

    /// Update a webkit view in the cache from a DMA-BUF buffer.
    /// Returns true if successful.
    #[cfg(feature = "wpe-webkit")]
    pub fn update_webkit_view_dmabuf(
        &mut self,
        view_id: u32,
        buffer: super::external_buffer::DmaBufBuffer,
    ) -> bool {
        self.webkit_cache.update_view(view_id, buffer, &self.device, &self.queue)
    }

    /// Update a webkit view in the cache from pixel data.
    /// Returns true if successful.
    #[cfg(feature = "wpe-webkit")]
    pub fn update_webkit_view_pixels(
        &mut self,
        view_id: u32,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> bool {
        self.webkit_cache.update_view_from_pixels(view_id, width, height, pixels, &self.device, &self.queue)
    }

    /// Remove a webkit view from the cache.
    #[cfg(feature = "wpe-webkit")]
    pub fn remove_webkit_view(&mut self, view_id: u32) {
        self.webkit_cache.remove(view_id);
    }

    /// Process pending webkit frames from WPE views.
    /// NOTE: In threaded mode, frame processing is done in render_thread.rs
    /// which calls update_webkit_view_dmabuf/update_webkit_view_pixels directly.
    /// This method is kept for API compatibility but is a no-op.
    #[cfg(feature = "wpe-webkit")]
    pub fn process_webkit_frames(&mut self) {
        // In threaded mode, frame processing happens in render_thread.rs
        // The render thread calls update_webkit_view_dmabuf/update_webkit_view_pixels directly
    }

    /// Render floating webkit views to the screen.
    /// This draws the cached webkit textures at their specified positions.
    #[cfg(feature = "wpe-webkit")]
    pub fn render_floating_webkits(
        &self,
        view: &wgpu::TextureView,
        floating_webkits: &[crate::core::scene::FloatingWebKit],
    ) {
        use wgpu::util::DeviceExt;

        if floating_webkits.is_empty() {
            return;
        }

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Floating WebKit Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Floating WebKit Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Preserve existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.opaque_image_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            for fw in floating_webkits {
                log::debug!("Rendering floating webkit {} at ({}, {}) size {}x{}",
                           fw.webkit_id, fw.x, fw.y, fw.width, fw.height);

                if let Some(cached) = self.webkit_cache.get(fw.webkit_id) {
                    let vertices = [
                        GlyphVertex { position: [fw.x, fw.y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                        GlyphVertex { position: [fw.x + fw.width, fw.y], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                        GlyphVertex { position: [fw.x + fw.width, fw.y + fw.height], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                        GlyphVertex { position: [fw.x, fw.y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                        GlyphVertex { position: [fw.x + fw.width, fw.y + fw.height], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                        GlyphVertex { position: [fw.x, fw.y + fw.height], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                    ];

                    let webkit_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Floating WebKit Vertex Buffer"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });

                    render_pass.set_bind_group(1, &cached.bind_group, &[]);
                    render_pass.set_vertex_buffer(0, webkit_buffer.slice(..));
                    render_pass.draw(0..6, 0..1);
                } else {
                    log::debug!("WebKit {} not found in cache", fw.webkit_id);
                }
            }
        }

        self.queue.submit(Some(encoder.finish()));
    }

    /// Render frame glyphs to a texture view
    ///
    /// `surface_width` and `surface_height` should be the actual surface dimensions
    /// for correct coordinate transformation.
    pub fn render_frame_glyphs(
        &mut self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        faces: &HashMap<u32, Face>,
        surface_width: u32,
        surface_height: u32,
        cursor_visible: bool,
        animated_cursor: Option<AnimatedCursor>,
        mouse_pos: (f32, f32),
        background_gradient: Option<((f32, f32, f32), (f32, f32, f32))>,
    ) {
        log::debug!(
            "render_frame_glyphs: frame={}x{} surface={}x{}, {} glyphs, {} faces",
            frame_glyphs.width,
            frame_glyphs.height,
            surface_width,
            surface_height,
            frame_glyphs.glyphs.len(),
            faces.len(),
        );

        // Reset continuous redraw flag (will be set by dim fade or other animations)
        self.needs_continuous_redraw = false;

        // Clean up expired line animations
        self.active_line_anims.retain(|a| a.started.elapsed() < a.duration);
        if !self.active_line_anims.is_empty() {
            self.needs_continuous_redraw = true;
        }

        // Clean up expired mode-line transition fades
        self.active_mode_line_fades.retain(|e| e.started.elapsed() < e.duration);
        if !self.active_mode_line_fades.is_empty() {
            self.needs_continuous_redraw = true;
        }

        // Detect mode-line content changes and trigger transitions
        if self.mode_line_transition_enabled {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let now_ml = std::time::Instant::now();
            for info in &frame_glyphs.window_infos {
                if info.mode_line_height < 1.0 || info.is_minibuffer {
                    continue;
                }
                let ml_y = info.bounds.y + info.bounds.height - info.mode_line_height;
                // Hash overlay chars within mode-line area
                let mut hasher = DefaultHasher::new();
                for g in &frame_glyphs.glyphs {
                    if let FrameGlyph::Char { x, y, char: ch, is_overlay: true, .. } = g {
                        if *x >= info.bounds.x && *x < info.bounds.x + info.bounds.width
                            && *y >= ml_y && *y < ml_y + info.mode_line_height
                        {
                            ch.hash(&mut hasher);
                        }
                    }
                }
                let hash = hasher.finish();
                let prev = self.prev_mode_line_hashes.insert(info.window_id, hash);
                if let Some(prev_hash) = prev {
                    if prev_hash != hash {
                        self.active_mode_line_fades.retain(|e| e.window_id != info.window_id);
                        self.active_mode_line_fades.push(ModeLineFadeEntry {
                            window_id: info.window_id,
                            mode_line_y: ml_y,
                            mode_line_h: info.mode_line_height,
                            bounds_x: info.bounds.x,
                            bounds_w: info.bounds.width,
                            started: now_ml,
                            duration: std::time::Duration::from_millis(self.mode_line_transition_duration_ms as u64),
                        });
                        self.needs_continuous_redraw = true;
                    }
                }
            }
        }

        // Clean up expired text fade-in animations
        self.active_text_fades.retain(|e| e.started.elapsed() < e.duration);
        if !self.active_text_fades.is_empty() {
            self.needs_continuous_redraw = true;
        }

        // Clean up expired scroll line spacing animations
        let now_spacing = std::time::Instant::now();
        self.active_scroll_spacings.retain(|e| {
            now_spacing.duration_since(e.started) < e.duration
        });
        if !self.active_scroll_spacings.is_empty() {
            self.needs_continuous_redraw = true;
        }

        // Clear expired cursor wake animation
        if let Some(started) = self.cursor_wake_started {
            let dur = std::time::Duration::from_millis(self.cursor_wake_duration_ms as u64);
            if started.elapsed() >= dur {
                self.cursor_wake_started = None;
            } else {
                self.needs_continuous_redraw = true;
            }
        }

        // Clear expired cursor error pulse
        if let Some(started) = self.cursor_error_pulse_started {
            let dur = std::time::Duration::from_millis(self.cursor_error_pulse_duration_ms as u64);
            if started.elapsed() >= dur {
                self.cursor_error_pulse_started = None;
            } else {
                self.needs_continuous_redraw = true;
            }
        }

        // Clean up expired scroll momentum entries
        self.active_scroll_momentums.retain(|e| e.started.elapsed() < e.duration);
        if !self.active_scroll_momentums.is_empty() {
            self.needs_continuous_redraw = true;
        }

        // Advance glyph atlas generation for LRU tracking
        glyph_atlas.advance_generation();

        // Update uniforms with logical size for correct coordinate transformation
        // (Emacs sends positions in logical pixels; surface is in physical pixels)
        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Rendering order for correct z-layering (inverse video cursor):
        //   1. Non-overlay backgrounds (window bg, stretches, char bg)
        //   2. Cursor bg rect (inverse video background for filled box cursor)
        //   3. Animated cursor trail (behind text, for filled box cursor motion)
        //   4. Non-overlay text (with cursor_fg swap for char at cursor position)
        //   5. Overlay backgrounds (mode-line/echo bg)
        //   6. Overlay text (mode-line/echo text)
        //   7. Inline media (images, videos, webkits)
        //   8. Front cursors (bar, hbar, hollow) and borders
        //
        // Filled box cursor (style 0) is split across steps 2-4 for inverse video.
        // Bar/hbar/hollow cursors are drawn on top of text in step 8.

        // Find minimum Y of overlay chars (mode-line/echo-area) for clipping inline media
        let overlay_y: Option<f32> = frame_glyphs.glyphs.iter()
            .filter_map(|g| {
                if let FrameGlyph::Char { y, is_overlay: true, .. } = g {
                    if *y < frame_glyphs.height {
                        Some(*y)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .reduce(f32::min);
        log::trace!("Frame {}x{}, overlay_y={:?}", frame_glyphs.width, frame_glyphs.height, overlay_y);

        // --- Merge adjacent boxed glyphs into spans ---
        // All box faces get span-merged for proper border rendering.
        // Only faces with corner_radius > 0 get the SDF rounded rect treatment
        // (background suppression + SDF fill + SDF border).
        // Standard boxes (corner_radius=0) get merged rect borders drawn after text.
        struct BoxSpan {
            x: f32,
            y: f32,
            width: f32,
            height: f32,
            face_id: u32,
            is_overlay: bool,
            bg: Option<Color>,
        }
        let mut box_spans: Vec<BoxSpan> = Vec::new();

        for glyph in &frame_glyphs.glyphs {
            // Extract position info from both Char and Stretch glyphs with box faces
            let (gx, gy, gw, gh, gface_id, g_overlay, g_bg) = match glyph {
                FrameGlyph::Char { x, y, width, height, face_id, is_overlay, bg, .. } => {
                    (*x, *y, *width, *height, *face_id, *is_overlay, *bg)
                }
                FrameGlyph::Stretch { x, y, width, height, face_id, is_overlay, bg } => {
                    (*x, *y, *width, *height, *face_id, *is_overlay, Some(*bg))
                }
                _ => continue,
            };

            // Only include glyphs whose face has BOX attribute
            match faces.get(&gface_id) {
                Some(f) if f.attributes.contains(FaceAttributes::BOX) && f.box_line_width > 0 => {}
                _ => continue,
            };

            // Check if this glyph's face has rounded corners
            let is_rounded = faces.get(&gface_id)
                .map(|f| f.box_corner_radius > 0)
                .unwrap_or(false);

            let merged = if let Some(last) = box_spans.last_mut() {
                let same_row = (last.y - gy).abs() < 0.5
                    && (last.height - gh).abs() < 0.5;
                let same_overlay = last.is_overlay == g_overlay;
                let adjacent = (gx - (last.x + last.width)).abs() < 1.0;
                let same_face = last.face_id == gface_id;

                // Merge rules:
                // - Rounded boxes: only merge same face_id (keep separate boxes)
                // - Sharp overlay boxes (mode-line): merge across face_ids (continuity)
                // - Sharp non-overlay boxes (content): only merge same face_id
                let last_is_rounded = faces.get(&last.face_id)
                    .map(|f| f.box_corner_radius > 0)
                    .unwrap_or(false);
                let face_ok = if is_rounded || last_is_rounded {
                    same_face  // rounded: strict same-face merge
                } else if g_overlay {
                    true  // sharp overlay: merge across faces (mode-line)
                } else {
                    same_face  // sharp non-overlay: strict same-face merge
                };

                if same_row && same_overlay && adjacent && face_ok {
                    last.width = gx + gw - last.x;
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if !merged {
                box_spans.push(BoxSpan {
                    x: gx, y: gy, width: gw, height: gh,
                    face_id: gface_id, is_overlay: g_overlay,
                    bg: g_bg,
                });
            }
        }

        // Helper: test whether a glyph position overlaps any ROUNDED box span.
        // Only suppresses backgrounds for rounded boxes (corner_radius > 0).
        // Standard boxes (corner_radius=0) keep normal rect backgrounds.
        let box_margin: f32 = box_spans.iter()
            .filter_map(|s| faces.get(&s.face_id)
                .filter(|f| f.box_corner_radius > 0)
                .map(|f| f.box_line_width as f32))
            .fold(0.0_f32, f32::max);
        let overlaps_rounded_box_span = |gx: f32, gy: f32, g_overlay: bool, spans: &[BoxSpan]| -> bool {
            if box_margin <= 0.0 { return false; }
            spans.iter().any(|s| {
                // Only check rounded box spans with the same overlay status
                if s.is_overlay != g_overlay { return false; }
                let is_rounded = faces.get(&s.face_id)
                    .map(|f| f.box_corner_radius > 0)
                    .unwrap_or(false);
                if !is_rounded { return false; }
                gx >= s.x - box_margin - 0.5 && gx < s.x + s.width + box_margin + 0.5
                && gy >= s.y - box_margin - 0.5 && gy < s.y + s.height + box_margin + 0.5
            })
        };

        // --- Collect non-overlay backgrounds ---
        let mut non_overlay_rect_vertices: Vec<RectVertex> = Vec::new();

        // Background gradient (rendered behind everything)
        if let Some((top, bottom)) = background_gradient {
            let top_color = Color::new(top.0, top.1, top.2, 1.0).srgb_to_linear();
            let bot_color = Color::new(bottom.0, bottom.1, bottom.2, 1.0).srgb_to_linear();
            let tc = [top_color.r, top_color.g, top_color.b, top_color.a];
            let bc = [bot_color.r, bot_color.g, bot_color.b, bot_color.a];
            // Two triangles forming a fullscreen quad with gradient
            // Top-left, top-right, bottom-left (triangle 1)
            non_overlay_rect_vertices.push(RectVertex { position: [0.0, 0.0], color: tc });
            non_overlay_rect_vertices.push(RectVertex { position: [logical_w, 0.0], color: tc });
            non_overlay_rect_vertices.push(RectVertex { position: [0.0, logical_h], color: bc });
            // Top-right, bottom-right, bottom-left (triangle 2)
            non_overlay_rect_vertices.push(RectVertex { position: [logical_w, 0.0], color: tc });
            non_overlay_rect_vertices.push(RectVertex { position: [logical_w, logical_h], color: bc });
            non_overlay_rect_vertices.push(RectVertex { position: [0.0, logical_h], color: bc });
        }

        // Window backgrounds
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Background { bounds, color } = glyph {
                self.add_rect(
                    &mut non_overlay_rect_vertices,
                    bounds.x, bounds.y, bounds.width, bounds.height, color,
                );
            }
        }
        // Non-overlay stretches (skip those inside a box span)
        let has_line_anims = !self.active_line_anims.is_empty() || !self.active_scroll_spacings.is_empty();
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Stretch { x, y, width, height, bg, is_overlay, .. } = glyph {
                if !*is_overlay && !overlaps_rounded_box_span(*x, *y, false, &box_spans) {
                    let ya = if has_line_anims { *y + self.line_y_offset(*x, *y) } else { *y };
                    self.add_rect(&mut non_overlay_rect_vertices, *x, ya, *width, *height, bg);
                }
            }
        }
        // Non-overlay char backgrounds (skip boxed chars — they get rounded bg instead)
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Char { x, y, width, height, bg, is_overlay, .. } = glyph {
                if !*is_overlay {
                    if let Some(bg_color) = bg {
                        if !overlaps_rounded_box_span(*x, *y, false, &box_spans) {
                            let ya = if has_line_anims { *y + self.line_y_offset(*x, *y) } else { *y };
                            self.add_rect(&mut non_overlay_rect_vertices, *x, ya, *width, *height, bg_color);
                        }
                    }
                }
            }
        }

        // --- Current line highlight ---
        if self.line_highlight_enabled {
            let (lr, lg, lb, la) = self.line_highlight_color;
            let hl_color = Color::new(lr, lg, lb, la);

            // Find the active cursor (style != 3, which is hollow/inactive)
            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Cursor { y, height, style, .. } = glyph {
                    if *style != 3 {
                        // Find the window this cursor belongs to
                        for info in &frame_glyphs.window_infos {
                            if info.selected {
                                // Draw highlight across the window width (excluding mode-line)
                                let hl_y = *y;
                                let hl_h = *height;
                                self.add_rect(
                                    &mut non_overlay_rect_vertices,
                                    info.bounds.x, hl_y,
                                    info.bounds.width, hl_h,
                                    &hl_color,
                                );
                                break;
                            }
                        }
                        break;
                    }
                }
            }
        }

        // --- Indent guides ---
        if self.indent_guides_enabled {
            let (ig_r, ig_g, ig_b, ig_a) = self.indent_guide_color;
            let guide_color = Color::new(ig_r, ig_g, ig_b, ig_a);
            let guide_width = 1.0_f32;

            // Detect char_width from frame
            let char_w = frame_glyphs.char_width.max(1.0);
            let tab_w = 4; // default tab width; we infer from the glyph spacing

            // Collect row info: group chars by Y coordinate to find rows,
            // then detect indent (leading space/tab) per row.
            struct RowInfo {
                y: f32,
                height: f32,
                first_non_space_x: f32,
                text_start_x: f32, // leftmost char X in the row
            }
            let mut rows: Vec<RowInfo> = Vec::new();
            let mut current_row_y: f32 = -1.0;
            let mut current_row_h: f32 = 0.0;
            let mut first_non_space_x: f32 = f32::MAX;
            let mut text_start_x: f32 = f32::MAX;
            let mut has_chars = false;

            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Char { x, y, width, height, char: ch, is_overlay, .. } = glyph {
                    if *is_overlay { continue; }
                    let gy = *y;
                    if (gy - current_row_y).abs() > 0.5 {
                        // New row — save previous
                        if has_chars && first_non_space_x > text_start_x + char_w {
                            rows.push(RowInfo {
                                y: current_row_y,
                                height: current_row_h,
                                first_non_space_x,
                                text_start_x,
                            });
                        }
                        current_row_y = gy;
                        current_row_h = *height;
                        first_non_space_x = f32::MAX;
                        text_start_x = f32::MAX;
                        has_chars = false;
                    }
                    has_chars = true;
                    if *x < text_start_x { text_start_x = *x; }
                    if *ch != ' ' && *ch != '\t' && *x < first_non_space_x {
                        first_non_space_x = *x;
                    }
                }
            }
            // Save last row
            if has_chars && first_non_space_x > text_start_x + char_w {
                rows.push(RowInfo {
                    y: current_row_y,
                    height: current_row_h,
                    first_non_space_x,
                    text_start_x,
                });
            }

            // Draw guides at each tab-stop column within the indent region
            let tab_px = char_w * tab_w as f32;
            let use_rainbow = self.indent_guide_rainbow_enabled
                && !self.indent_guide_rainbow_colors.is_empty();
            for row in &rows {
                let mut col_x = row.text_start_x + tab_px;
                let mut depth: usize = 0;
                while col_x < row.first_non_space_x - 1.0 {
                    let color = if use_rainbow {
                        let (r, g, b, a) = self.indent_guide_rainbow_colors
                            [depth % self.indent_guide_rainbow_colors.len()];
                        Color::new(r, g, b, a)
                    } else {
                        guide_color
                    };
                    self.add_rect(
                        &mut non_overlay_rect_vertices,
                        col_x, row.y, guide_width, row.height,
                        &color,
                    );
                    col_x += tab_px;
                    depth += 1;
                }
            }
        }

        // --- Visible whitespace dots ---
        if self.show_whitespace_enabled {
            let (wr, wg, wb, wa) = self.show_whitespace_color;
            let ws_color = Color::new(wr, wg, wb, wa);
            let dot_size = 1.5_f32;

            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Char { char: ch, x, y, width, height, ascent, is_overlay, .. } = glyph {
                    if *is_overlay { continue; }
                    if *ch == ' ' {
                        // Centered dot for space
                        let dot_x = *x + (*width - dot_size) / 2.0;
                        let dot_y = *y + (*ascent - dot_size / 2.0);
                        self.add_rect(
                            &mut non_overlay_rect_vertices,
                            dot_x, dot_y, dot_size, dot_size,
                            &ws_color,
                        );
                    } else if *ch == '\t' {
                        // Small horizontal arrow for tab
                        let arrow_h = 1.5_f32;
                        let arrow_y = *y + (*ascent - arrow_h / 2.0);
                        let arrow_w = (*width - 4.0).max(4.0);
                        let arrow_x = *x + 2.0;
                        // Shaft
                        self.add_rect(
                            &mut non_overlay_rect_vertices,
                            arrow_x, arrow_y, arrow_w, arrow_h,
                            &ws_color,
                        );
                        // Arrowhead (small triangle approximated as 2 rects)
                        let tip_x = arrow_x + arrow_w;
                        self.add_rect(
                            &mut non_overlay_rect_vertices,
                            tip_x - 3.0, arrow_y - 1.5, 3.0, arrow_h + 3.0,
                            &ws_color,
                        );
                    }
                }
            }
        }

        // --- Collect overlay backgrounds ---
        let mut overlay_rect_vertices: Vec<RectVertex> = Vec::new();

        // Overlay stretches (skip those inside a box span)
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Stretch { x, y, width, height, bg, is_overlay, .. } = glyph {
                if *is_overlay && !overlaps_rounded_box_span(*x, *y, true, &box_spans) {
                    self.add_rect(&mut overlay_rect_vertices, *x, *y, *width, *height, bg);
                }
            }
        }
        // Overlay char backgrounds (skip those inside a box span)
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Char { x, y, width, height, bg, is_overlay, .. } = glyph {
                if *is_overlay {
                    if let Some(bg_color) = bg {
                        if !overlaps_rounded_box_span(*x, *y, true, &box_spans) {
                            self.add_rect(&mut overlay_rect_vertices, *x, *y, *width, *height, bg_color);
                        }
                    }
                }
            }
        }

        // === Collect cursor bg rect for inverse video (drawn before text) ===
        // For filled box cursor (style 0), we draw the cursor background BEFORE text
        // so the character under the cursor can be re-drawn with inverse colors on top.
        let mut cursor_bg_vertices: Vec<RectVertex> = Vec::new();

        // === Collect behind-text cursor shapes (animated trail for filled box) ===
        let mut behind_text_cursor_vertices: Vec<RectVertex> = Vec::new();

        // === Collect front cursors and borders (drawn after text) ===
        // Bar (1), hbar (2), hollow (3), borders — all drawn on top of text.
        // Filled box (0) is EXCLUDED here — handled by bg rect + trail + fg swap.
        let mut cursor_vertices: Vec<RectVertex> = Vec::new();

        // === Collect scroll bar thumbs (drawn as rounded rects) ===
        let mut scroll_bar_thumb_vertices: Vec<(f32, f32, f32, f32, f32, Color)> = Vec::new();

        for glyph in &frame_glyphs.glyphs {
            match glyph {
                FrameGlyph::Border {
                    x,
                    y,
                    width,
                    height,
                    color,
                } => {
                    self.add_rect(&mut cursor_vertices, *x, *y, *width, *height, color);
                }
                FrameGlyph::ScrollBar {
                    horizontal,
                    x,
                    y,
                    width,
                    height,
                    thumb_start,
                    thumb_size,
                    track_color,
                    thumb_color,
                } => {
                    // Draw scroll bar track (subtle, configurable opacity)
                    let subtle_track = Color::new(
                        track_color.r, track_color.g, track_color.b,
                        track_color.a * self.scroll_bar_track_opacity,
                    );
                    self.add_rect(&mut cursor_vertices, *x, *y, *width, *height, &subtle_track);

                    // Compute thumb bounds
                    let (tx, ty, tw, th) = if *horizontal {
                        (*x + *thumb_start, *y, *thumb_size, *height)
                    } else {
                        (*x, *y + *thumb_start, *width, *thumb_size)
                    };

                    // Check hover: brighten thumb if mouse is over the scroll bar area
                    let (mx, my) = mouse_pos;
                    let hovered = mx >= *x && mx <= *x + *width
                        && my >= *y && my <= *y + *height;
                    let bright = self.scroll_bar_hover_brightness;
                    let effective_thumb = if hovered {
                        Color::new(
                            (thumb_color.r * bright).min(1.0),
                            (thumb_color.g * bright).min(1.0),
                            (thumb_color.b * bright).min(1.0),
                            thumb_color.a.min(1.0),
                        )
                    } else {
                        *thumb_color
                    };

                    // Rounded thumb with configurable pill radius
                    let radius = tw.min(th) * self.scroll_bar_thumb_radius;
                    scroll_bar_thumb_vertices.push((tx, ty, tw, th, radius, effective_thumb));
                }
                FrameGlyph::Cursor {
                    window_id,
                    x,
                    y,
                    width,
                    height,
                    style,
                    color,
                } => {
                    // Compute effective cursor color (possibly overridden by color cycling)
                    let cycle_color;
                    let effective_color = if self.cursor_color_cycle_enabled && *style != 3 {
                        let elapsed = self.cursor_color_cycle_start.elapsed().as_secs_f32();
                        let hue = (elapsed * self.cursor_color_cycle_speed) % 1.0;
                        cycle_color = Self::hsl_to_color(hue, self.cursor_color_cycle_saturation, self.cursor_color_cycle_lightness);
                        self.needs_continuous_redraw = true;
                        &cycle_color
                    } else {
                        color
                    };
                    // Cursor error pulse: override color on bell
                    let error_pulse_color;
                    let effective_color = if let Some(pulse) = self.cursor_error_pulse_override() {
                        if *style != 3 {
                            error_pulse_color = pulse;
                            self.needs_continuous_redraw = true;
                            &error_pulse_color
                        } else {
                            effective_color
                        }
                    } else {
                        effective_color
                    };
                    // Cursor wake animation: scale factor for pop effect
                    let wake = self.cursor_wake_factor();
                    let wake_active = wake != 1.0 && *style != 3;
                    if wake_active {
                        self.needs_continuous_redraw = true;
                    }
                    if *style == 0 {
                        // Filled box cursor: split into bg rect + behind-text trail.
                        // The static cursor bg rect uses cursor_inverse info if available,
                        // otherwise falls back to the cursor color at the static position.
                        if cursor_visible {
                            if let Some(ref inv) = frame_glyphs.cursor_inverse {
                                // Draw cursor bg rect at static position (inverse video background)
                                let inv_color = if self.cursor_color_cycle_enabled {
                                    effective_color
                                } else {
                                    &inv.cursor_bg
                                };
                                if wake_active {
                                    let (sx, sy, sw, sh) = Self::scale_rect(inv.x, inv.y, inv.width, inv.height, wake);
                                    self.add_rect(&mut cursor_bg_vertices, sx, sy, sw, sh, inv_color);
                                } else {
                                    self.add_rect(&mut cursor_bg_vertices,
                                        inv.x, inv.y, inv.width, inv.height, inv_color);
                                }
                            } else {
                                // No inverse info — draw opaque cursor at static position
                                if wake_active {
                                    let (sx, sy, sw, sh) = Self::scale_rect(*x, *y, *width, *height, wake);
                                    self.add_rect(&mut cursor_bg_vertices, sx, sy, sw, sh, effective_color);
                                } else {
                                    self.add_rect(&mut cursor_bg_vertices, *x, *y, *width, *height, effective_color);
                                }
                            }

                            // Draw animated trail/rect behind text
                            let use_corners = if let Some(ref anim) = animated_cursor {
                                *window_id == anim.window_id && anim.corners.is_some()
                            } else {
                                false
                            };

                            if use_corners {
                                let anim = animated_cursor.as_ref().unwrap();
                                let corners = anim.corners.as_ref().unwrap();
                                self.add_quad(&mut behind_text_cursor_vertices, corners, effective_color);
                            } else if let Some(ref anim) = animated_cursor {
                                if *window_id == anim.window_id {
                                    self.add_rect(&mut behind_text_cursor_vertices,
                                        anim.x, anim.y, anim.width, anim.height, effective_color);
                                }
                            }
                        }
                    } else {
                        // Non-filled-box cursors: bar, hbar, hollow — drawn ON TOP of text
                        let use_corners = if let Some(ref anim) = animated_cursor {
                            *window_id == anim.window_id && *style != 3 && anim.corners.is_some()
                        } else {
                            false
                        };

                        if use_corners {
                            let anim = animated_cursor.as_ref().unwrap();
                            let corners = anim.corners.as_ref().unwrap();
                            if cursor_visible {
                                self.add_quad(&mut cursor_vertices, corners, effective_color);
                            }
                        } else {
                            let (cx, cy, cw, ch) = if let Some(ref anim) = animated_cursor {
                                if *window_id == anim.window_id && *style != 3 {
                                    (anim.x, anim.y, anim.width, anim.height)
                                } else {
                                    (*x, *y, *width, *height)
                                }
                            } else {
                                (*x, *y, *width, *height)
                            };

                            let should_draw = *style == 3 || cursor_visible;
                            if should_draw {
                                match style {
                                    1 => {
                                        // Bar (thin vertical line)
                                        if wake_active {
                                            let (sx, sy, sw, sh) = Self::scale_rect(cx, cy, 2.0, ch, wake);
                                            self.add_rect(&mut cursor_vertices, sx, sy, sw, sh, effective_color);
                                        } else {
                                            self.add_rect(&mut cursor_vertices, cx, cy, 2.0, ch, effective_color);
                                        }
                                    }
                                    2 => {
                                        // Underline (hbar at bottom)
                                        if wake_active {
                                            let (sx, sy, sw, sh) = Self::scale_rect(cx, cy + ch - 2.0, cw, 2.0, wake);
                                            self.add_rect(&mut cursor_vertices, sx, sy, sw, sh, effective_color);
                                        } else {
                                            self.add_rect(&mut cursor_vertices, cx, cy + ch - 2.0, cw, 2.0, effective_color);
                                        }
                                    }
                                    3 => {
                                        // Hollow box (4 border edges)
                                        self.add_rect(&mut cursor_vertices, cx, cy, cw, 1.0, effective_color);
                                        self.add_rect(&mut cursor_vertices, cx, cy + ch - 1.0, cw, 1.0, effective_color);
                                        self.add_rect(&mut cursor_vertices, cx, cy, 1.0, ch, effective_color);
                                        self.add_rect(&mut cursor_vertices, cx + cw - 1.0, cy, 1.0, ch, effective_color);
                                    }
                                    _ => {
                                        self.add_rect(&mut cursor_vertices, cx, cy, cw, ch, effective_color);
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Glyphs Encoder"),
            });

        // Render pass - Clear with frame background color since we rebuild
        // the entire frame from current_matrix each time (no incremental updates).
        let bg = &frame_glyphs.background;
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Frame Glyphs Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            // Pre-multiply RGB by alpha for correct compositing
                            r: (bg.r * bg.a) as f64,
                            g: (bg.g * bg.a) as f64,
                            b: (bg.b * bg.a) as f64,
                            a: bg.a as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // === Step 1: Draw non-overlay backgrounds ===
            if !non_overlay_rect_vertices.is_empty() {
                let rect_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Non-overlay Rect Buffer"),
                            contents: bytemuck::cast_slice(&non_overlay_rect_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                render_pass.set_pipeline(&self.rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, rect_buffer.slice(..));
                render_pass.draw(0..non_overlay_rect_vertices.len() as u32, 0..1);
            }

            // === Step 1a: Background pattern (dots/grid/crosshatch) ===
            if self.bg_pattern_style > 0 {
                let spacing = self.bg_pattern_spacing.max(4.0);
                let (pr, pg, pb) = self.bg_pattern_color;
                let alpha = self.bg_pattern_opacity;
                let pat_color = Color::new(pr, pg, pb, alpha);
                let frame_w = frame_glyphs.width;
                let frame_h = frame_glyphs.height;
                let mut pattern_vertices: Vec<RectVertex> = Vec::new();

                match self.bg_pattern_style {
                    1 => {
                        // Dots: small squares at grid intersections
                        let dot_size = 1.0;
                        let mut y = 0.0_f32;
                        while y < frame_h {
                            let mut x = 0.0_f32;
                            while x < frame_w {
                                self.add_rect(&mut pattern_vertices, x, y, dot_size, dot_size, &pat_color);
                                x += spacing;
                            }
                            y += spacing;
                        }
                    }
                    2 => {
                        // Grid: horizontal and vertical lines
                        let line_w = 1.0;
                        let mut y = 0.0_f32;
                        while y < frame_h {
                            self.add_rect(&mut pattern_vertices, 0.0, y, frame_w, line_w, &pat_color);
                            y += spacing;
                        }
                        let mut x = 0.0_f32;
                        while x < frame_w {
                            self.add_rect(&mut pattern_vertices, x, 0.0, line_w, frame_h, &pat_color);
                            x += spacing;
                        }
                    }
                    3 => {
                        // Crosshatch: diagonal lines (approximated with small segments)
                        let line_w = 1.0;
                        let step = 2.0;
                        let diag_spacing = spacing * 1.414; // sqrt(2) for diagonal
                        // Forward diagonals (top-left to bottom-right)
                        let mut offset = -frame_h;
                        while offset < frame_w {
                            let mut t = 0.0_f32;
                            while t < frame_h.min(frame_w) {
                                let px = offset + t;
                                let py = t;
                                if px >= 0.0 && px < frame_w && py >= 0.0 && py < frame_h {
                                    self.add_rect(&mut pattern_vertices, px, py, line_w, step, &pat_color);
                                }
                                t += step;
                            }
                            offset += diag_spacing;
                        }
                        // Back diagonals (top-right to bottom-left)
                        let mut offset = 0.0_f32;
                        while offset < frame_w + frame_h {
                            let mut t = 0.0_f32;
                            while t < frame_h.min(frame_w) {
                                let px = offset - t;
                                let py = t;
                                if px >= 0.0 && px < frame_w && py >= 0.0 && py < frame_h {
                                    self.add_rect(&mut pattern_vertices, px, py, line_w, step, &pat_color);
                                }
                                t += step;
                            }
                            offset += diag_spacing;
                        }
                    }
                    _ => {}
                }

                if !pattern_vertices.is_empty() {
                    let pattern_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Background Pattern Buffer"),
                            contents: bytemuck::cast_slice(&pattern_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, pattern_buffer.slice(..));
                    render_pass.draw(0..pattern_vertices.len() as u32, 0..1);
                }
            }

            // === Step 1b: Draw filled rounded rect backgrounds for ROUNDED boxed spans ===
            // Only for corner_radius > 0. Standard boxes use normal rect backgrounds.
            {
                let mut box_fill_vertices: Vec<RoundedRectVertex> = Vec::new();
                for span in &box_spans {
                    if span.is_overlay { continue; }
                    if let Some(ref bg_color) = span.bg {
                        if let Some(face) = faces.get(&span.face_id) {
                            if face.box_corner_radius <= 0 { continue; }
                            let radius = (face.box_corner_radius as f32)
                                .min(span.height * 0.45)
                                .min(span.width * 0.45);
                            // Use a border_width larger than half the rect to fill solid
                            let fill_bw = span.height.max(span.width);
                            self.add_rounded_rect(
                                &mut box_fill_vertices,
                                span.x, span.y, span.width, span.height,
                                fill_bw, radius, bg_color,
                            );
                        }
                    }
                }
                if !box_fill_vertices.is_empty() {
                    let fill_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Box Fill Buffer"),
                            contents: bytemuck::cast_slice(&box_fill_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rounded_rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, fill_buffer.slice(..));
                    render_pass.draw(0..box_fill_vertices.len() as u32, 0..1);
                }
            }

            // === Step 1c: Draw cursor glow effect (behind cursor and text) ===
            if self.cursor_glow_enabled && cursor_visible {
                // Find active cursor position (style != 3 = not hollow/inactive)
                let mut glow_pos: Option<(f32, f32, f32, f32)> = None;
                if let Some(ref anim) = animated_cursor {
                    glow_pos = Some((anim.x, anim.y, anim.width, anim.height));
                } else {
                    for glyph in &frame_glyphs.glyphs {
                        if let FrameGlyph::Cursor { x, y, width, height, style, .. } = glyph {
                            if *style != 3 {
                                glow_pos = Some((*x, *y, *width, *height));
                                break;
                            }
                        }
                    }
                }

                if let Some((cx, cy, cw, ch)) = glow_pos {
                    let (gr, gg, gb) = self.cursor_glow_color;
                    let radius = self.cursor_glow_radius;
                    let mut peak_alpha = self.cursor_glow_opacity;

                    // Apply pulse modulation if enabled
                    if self.cursor_pulse_enabled {
                        let elapsed = self.cursor_pulse_start.elapsed().as_secs_f32();
                        let phase = elapsed * self.cursor_pulse_speed * 2.0 * std::f32::consts::PI;
                        // Sine wave: maps [min_opacity..1.0] range
                        let t = (phase.sin() + 1.0) / 2.0; // 0.0 to 1.0
                        let factor = self.cursor_pulse_min_opacity + t * (1.0 - self.cursor_pulse_min_opacity);
                        peak_alpha *= factor;
                    }
                    let layers = (radius / 2.0).ceil() as i32;
                    let mut glow_vertices: Vec<RectVertex> = Vec::new();

                    // Center of cursor
                    let center_x = cx + cw / 2.0;
                    let center_y = cy + ch / 2.0;

                    for i in 0..layers {
                        let t = (i + 1) as f32 / layers as f32;
                        let r = radius * t;
                        let alpha = peak_alpha * (1.0 - t * t); // quadratic falloff
                        let c = Color::new(gr, gg, gb, alpha);
                        let gx = center_x - r;
                        let gy = center_y - r;
                        let gw = r * 2.0;
                        let gh = r * 2.0;
                        self.add_rect(&mut glow_vertices, gx, gy, gw, gh, &c);
                    }

                    if !glow_vertices.is_empty() {
                        let glow_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Cursor Glow Buffer"),
                                contents: bytemuck::cast_slice(&glow_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, glow_buffer.slice(..));
                        render_pass.draw(0..glow_vertices.len() as u32, 0..1);
                    }
                }
            }

            // === Step 1d: Draw cursor crosshair guide lines ===
            if self.cursor_crosshair_enabled && cursor_visible {
                let mut cross_pos: Option<(f32, f32, f32, f32)> = None;
                if let Some(ref anim) = animated_cursor {
                    cross_pos = Some((anim.x, anim.y, anim.width, anim.height));
                } else {
                    for glyph in &frame_glyphs.glyphs {
                        if let FrameGlyph::Cursor { x, y, width, height, style, .. } = glyph {
                            if *style != 3 {
                                cross_pos = Some((*x, *y, *width, *height));
                                break;
                            }
                        }
                    }
                }
                if let Some((cx, cy, cw, ch)) = cross_pos {
                    // Find the window that contains this cursor
                    let cursor_center_x = cx + cw / 2.0;
                    let cursor_center_y = cy + ch / 2.0;
                    if let Some(win_info) = frame_glyphs.window_infos.iter()
                        .find(|w| w.selected && !w.is_minibuffer)
                    {
                        let (cr, cg, cb) = self.cursor_crosshair_color;
                        let alpha = self.cursor_crosshair_opacity;
                        let c = Color::new(cr, cg, cb, alpha);
                        let mut cross_verts: Vec<RectVertex> = Vec::new();
                        let wb = &win_info.bounds;
                        // Don't extend into mode-line area
                        let win_bottom = wb.y + wb.height - win_info.mode_line_height;
                        // Horizontal line (full window width, 1px height at cursor center Y)
                        self.add_rect(&mut cross_verts, wb.x, cursor_center_y, wb.width, 1.0, &c);
                        // Vertical line (1px width, from window top to above mode-line)
                        self.add_rect(&mut cross_verts, cursor_center_x, wb.y, 1.0, win_bottom - wb.y, &c);
                        if !cross_verts.is_empty() {
                            let cross_buf = self.device.create_buffer_init(
                                &wgpu::util::BufferInitDescriptor {
                                    label: Some("Cursor Crosshair Buffer"),
                                    contents: bytemuck::cast_slice(&cross_verts),
                                    usage: wgpu::BufferUsages::VERTEX,
                                },
                            );
                            render_pass.set_pipeline(&self.rect_pipeline);
                            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                            render_pass.set_vertex_buffer(0, cross_buf.slice(..));
                            render_pass.draw(0..cross_verts.len() as u32, 0..1);
                        }
                    }
                }
            }

            // === Step 1e: Draw buffer modified border indicator ===
            if self.modified_indicator_enabled {
                let (mr, mg, mb) = self.modified_indicator_color;
                let malpha = self.modified_indicator_opacity;
                let mc = Color::new(mr, mg, mb, malpha);
                let mw = self.modified_indicator_width;
                let mut mod_verts: Vec<RectVertex> = Vec::new();
                for win_info in &frame_glyphs.window_infos {
                    if win_info.modified && !win_info.is_minibuffer {
                        let wb = &win_info.bounds;
                        // Draw a colored strip along the left edge of the window
                        let mode_line_h = win_info.mode_line_height;
                        let content_h = wb.height - mode_line_h;
                        if content_h > 0.0 {
                            self.add_rect(&mut mod_verts, wb.x, wb.y, mw, content_h, &mc);
                        }
                    }
                }
                if !mod_verts.is_empty() {
                    let mod_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Modified Indicator Buffer"),
                            contents: bytemuck::cast_slice(&mod_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, mod_buf.slice(..));
                    render_pass.draw(0..mod_verts.len() as u32, 0..1);
                }
            }

            // === Step 1f: Typing heat map overlay ===
            if self.typing_heatmap_enabled {
                let now = std::time::Instant::now();
                let fade_dur = std::time::Duration::from_millis(self.typing_heatmap_fade_ms as u64);

                // Detect cursor movement and record heat entry
                if let Some(ref anim) = animated_cursor {
                    let cur_pos = (anim.x, anim.y);
                    if let Some(prev_pos) = self.typing_heatmap_prev_cursor {
                        let dx = (cur_pos.0 - prev_pos.0).abs();
                        let dy = (cur_pos.1 - prev_pos.1).abs();
                        // Small movements (within ~2 char widths) indicate typing
                        if (dx > 0.5 || dy > 0.5) && dx < 200.0 && dy < 100.0 {
                            self.typing_heatmap_entries.push(HeatMapEntry {
                                x: prev_pos.0,
                                y: prev_pos.1,
                                width: anim.width,
                                height: anim.height,
                                started: now,
                            });
                        }
                    }
                    self.typing_heatmap_prev_cursor = Some(cur_pos);
                }

                // Prune expired entries
                self.typing_heatmap_entries.retain(|e| now.duration_since(e.started) < fade_dur);

                // Render heat entries
                if !self.typing_heatmap_entries.is_empty() {
                    let (hr, hg, hb) = self.typing_heatmap_color;
                    let max_alpha = self.typing_heatmap_opacity;
                    let mut heat_verts: Vec<RectVertex> = Vec::new();
                    for entry in &self.typing_heatmap_entries {
                        let elapsed = now.duration_since(entry.started);
                        let t = (elapsed.as_secs_f32() / fade_dur.as_secs_f32()).min(1.0);
                        let alpha = max_alpha * (1.0 - t);
                        if alpha > 0.001 {
                            let c = Color::new(hr, hg, hb, alpha);
                            self.add_rect(&mut heat_verts, entry.x, entry.y, entry.width, entry.height, &c);
                        }
                    }
                    if !heat_verts.is_empty() {
                        let heat_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Heat Map Buffer"),
                                contents: bytemuck::cast_slice(&heat_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, heat_buf.slice(..));
                        render_pass.draw(0..heat_verts.len() as u32, 0..1);
                    }

                    // Keep redrawing while entries exist for smooth fade
                    if !self.typing_heatmap_entries.is_empty() {
                        self.needs_continuous_redraw = true;
                    }
                }
            }

            // === Step 1g: Per-window rounded border ===
            if self.window_border_radius_enabled {
                let (wr, wg, wb) = self.window_border_color;
                let walpha = self.window_border_opacity;
                let wc = Color::new(wr, wg, wb, walpha);
                let radius = self.window_border_radius;
                let bw = self.window_border_width;
                let mut border_verts: Vec<RoundedRectVertex> = Vec::new();
                for win_info in &frame_glyphs.window_infos {
                    if !win_info.is_minibuffer {
                        let wb_bounds = &win_info.bounds;
                        let mode_h = win_info.mode_line_height;
                        let content_h = wb_bounds.height - mode_h;
                        if content_h > 0.0 {
                            self.add_rounded_rect(
                                &mut border_verts,
                                wb_bounds.x, wb_bounds.y,
                                wb_bounds.width, content_h,
                                bw, radius, &wc,
                            );
                        }
                    }
                }
                if !border_verts.is_empty() {
                    let border_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Window Border Radius Buffer"),
                            contents: bytemuck::cast_slice(&border_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rounded_rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, border_buf.slice(..));
                    render_pass.draw(0..border_verts.len() as u32, 0..1);
                }
            }

            // === Step 1h: Inactive window stained glass effect ===
            if self.stained_glass_enabled {
                let sg_opacity = self.stained_glass_opacity;
                let sg_sat = self.stained_glass_saturation;
                let mut sg_verts: Vec<RectVertex> = Vec::new();
                for win_info in &frame_glyphs.window_infos {
                    if !win_info.selected && !win_info.is_minibuffer {
                        let wb = &win_info.bounds;
                        let mode_h = win_info.mode_line_height;
                        let content_h = wb.height - mode_h;
                        if content_h > 0.0 {
                            // Derive hue from buffer_id hash
                            let hash = (win_info.buffer_id as u64).wrapping_mul(2654435761) >> 16;
                            let hue = (hash % 360) as f32;
                            // HSL to RGB (simplified, lightness=0.5)
                            let c = sg_sat;
                            let h_prime = hue / 60.0;
                            let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
                            let (r1, g1, b1) = match h_prime as u32 {
                                0 => (c, x, 0.0),
                                1 => (x, c, 0.0),
                                2 => (0.0, c, x),
                                3 => (0.0, x, c),
                                4 => (x, 0.0, c),
                                _ => (c, 0.0, x),
                            };
                            let m = 0.5 - c / 2.0;
                            let color = Color::new(r1 + m, g1 + m, b1 + m, sg_opacity);
                            self.add_rect(&mut sg_verts, wb.x, wb.y, wb.width, content_h, &color);
                        }
                    }
                }
                if !sg_verts.is_empty() {
                    let sg_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Stained Glass Buffer"),
                            contents: bytemuck::cast_slice(&sg_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, sg_buf.slice(..));
                    render_pass.draw(0..sg_verts.len() as u32, 0..1);
                }
            }

            // === Step 1i_focus: Focus gradient border ===
            if self.focus_gradient_border_enabled {
                let bw = self.focus_gradient_border_width;
                let (tr, tg, tb) = self.focus_gradient_border_top_color;
                let (br2, bg2, bb2) = self.focus_gradient_border_bot_color;
                let alpha = self.focus_gradient_border_opacity;
                let mut grad_verts: Vec<RectVertex> = Vec::new();
                for win_info in &frame_glyphs.window_infos {
                    if !win_info.selected || win_info.is_minibuffer { continue; }
                    let b = &win_info.bounds;
                    // Vertical gradient using horizontal strips
                    let strips = b.height.ceil() as u32;
                    for s in 0..strips {
                        let t = s as f32 / strips as f32;
                        let r = tr + (br2 - tr) * t;
                        let g = tg + (bg2 - tg) * t;
                        let b_color = tb + (bb2 - tb) * t;
                        let c = Color::new(r, g, b_color, alpha);
                        let y = b.y + s as f32;
                        // Left edge
                        self.add_rect(&mut grad_verts, b.x, y, bw, 1.0, &c);
                        // Right edge
                        self.add_rect(&mut grad_verts, b.x + b.width - bw, y, bw, 1.0, &c);
                    }
                    // Top
                    let tc = Color::new(tr, tg, tb, alpha);
                    self.add_rect(&mut grad_verts, b.x, b.y, b.width, bw, &tc);
                    // Bottom
                    let bc = Color::new(br2, bg2, bb2, alpha);
                    self.add_rect(&mut grad_verts, b.x, b.y + b.height - bw, b.width, bw, &bc);
                }
                if !grad_verts.is_empty() {
                    let grad_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Focus Gradient Border Buffer"),
                            contents: bytemuck::cast_slice(&grad_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, grad_buf.slice(..));
                    render_pass.draw(0..grad_verts.len() as u32, 0..1);
                }
            }

            // === Step 1i_depth: Window depth shadow layers ===
            if self.depth_shadow_enabled {
                let (dr, dg, db) = self.depth_shadow_color;
                let mut shadow_verts: Vec<RectVertex> = Vec::new();
                for win_info in &frame_glyphs.window_infos {
                    if win_info.is_minibuffer { continue; }
                    let b = &win_info.bounds;
                    for layer in (1..=self.depth_shadow_layers).rev() {
                        let off = layer as f32 * self.depth_shadow_offset;
                        let alpha = self.depth_shadow_opacity * (1.0 - (layer - 1) as f32 / self.depth_shadow_layers as f32);
                        let c = Color::new(dr, dg, db, alpha);
                        // Bottom edge shadow
                        self.add_rect(&mut shadow_verts, b.x + off, b.y + b.height, b.width, off, &c);
                        // Right edge shadow
                        self.add_rect(&mut shadow_verts, b.x + b.width, b.y + off, off, b.height, &c);
                    }
                }
                if !shadow_verts.is_empty() {
                    let shadow_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Depth Shadow Buffer"),
                            contents: bytemuck::cast_slice(&shadow_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, shadow_buf.slice(..));
                    render_pass.draw(0..shadow_verts.len() as u32, 0..1);
                }
            }

            // === Step 1i_modeline_grad: Mode-line gradient background ===
            if self.mode_line_gradient_enabled {
                let (lr, lg, lb) = self.mode_line_gradient_left_color;
                let (rr, rg, rb) = self.mode_line_gradient_right_color;
                let alpha = self.mode_line_gradient_opacity;
                let mut ml_verts: Vec<RectVertex> = Vec::new();
                for win_info in &frame_glyphs.window_infos {
                    if win_info.is_minibuffer { continue; }
                    let b = &win_info.bounds;
                    let ml_h = win_info.mode_line_height;
                    if ml_h < 1.0 { continue; }
                    let ml_y = b.y + b.height - ml_h;
                    // Horizontal gradient using vertical strips
                    let strips = b.width.ceil() as u32;
                    let strip_w = (b.width / strips.max(1) as f32).max(1.0);
                    let actual_strips = (b.width / strip_w).ceil() as u32;
                    for s in 0..actual_strips {
                        let t = s as f32 / actual_strips as f32;
                        let r = lr + (rr - lr) * t;
                        let g = lg + (rg - lg) * t;
                        let b_c = lb + (rb - lb) * t;
                        let c = Color::new(r, g, b_c, alpha);
                        self.add_rect(&mut ml_verts, b.x + s as f32 * strip_w, ml_y, strip_w, ml_h, &c);
                    }
                }
                if !ml_verts.is_empty() {
                    let ml_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Mode-line Gradient Buffer"),
                            contents: bytemuck::cast_slice(&ml_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, ml_buf.slice(..));
                    render_pass.draw(0..ml_verts.len() as u32, 0..1);
                }
            }

            // === Step 1i_magnetism: Cursor magnetism effect ===
            if self.cursor_magnetism_enabled {
                let now = std::time::Instant::now();
                let dur = std::time::Duration::from_millis(self.cursor_magnetism_duration_ms as u64);

                // Detect cursor jump (large movement) and record
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let should_add = self.cursor_magnetism_entries.last()
                        .map(|(px, py, _)| {
                            let dx = (cx - px).abs();
                            let dy = (cy - py).abs();
                            dx > 50.0 || dy > 20.0 // jump threshold
                        })
                        .unwrap_or(false);
                    if should_add {
                        self.cursor_magnetism_entries.push((cx, cy, now));
                    }
                    // Initial entry
                    if self.cursor_magnetism_entries.is_empty() {
                        self.cursor_magnetism_entries.push((cx, cy, now));
                    }
                }

                // Prune expired
                self.cursor_magnetism_entries.retain(|&(_, _, t)| now.duration_since(t) < dur);

                // Render collapsing rings
                if !self.cursor_magnetism_entries.is_empty() {
                    let (mr, mg, mb) = self.cursor_magnetism_color;
                    let max_alpha = self.cursor_magnetism_opacity;
                    let ring_count = self.cursor_magnetism_ring_count;
                    let mut mag_verts: Vec<RectVertex> = Vec::new();
                    for &(cx, cy, started) in &self.cursor_magnetism_entries {
                        let t = now.duration_since(started).as_secs_f32() / dur.as_secs_f32();
                        if t >= 1.0 { continue; }
                        let alpha = max_alpha * (1.0 - t);
                        for ring in 0..ring_count {
                            let ring_t = ring as f32 / ring_count as f32;
                            let radius = (1.0 - t) * (30.0 + ring_t * 40.0); // collapse inward
                            let c = Color::new(mr, mg, mb, alpha * (1.0 - ring_t * 0.5));
                            let line_w = 1.5;
                            // Approximate ring with 4 rects (top, bottom, left, right)
                            self.add_rect(&mut mag_verts, cx - radius, cy - radius, radius * 2.0, line_w, &c);
                            self.add_rect(&mut mag_verts, cx - radius, cy + radius - line_w, radius * 2.0, line_w, &c);
                            self.add_rect(&mut mag_verts, cx - radius, cy - radius, line_w, radius * 2.0, &c);
                            self.add_rect(&mut mag_verts, cx + radius - line_w, cy - radius, line_w, radius * 2.0, &c);
                        }
                    }
                    if !mag_verts.is_empty() {
                        let mag_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Cursor Magnetism Buffer"),
                                contents: bytemuck::cast_slice(&mag_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, mag_buf.slice(..));
                        render_pass.draw(0..mag_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Step 1i2: Window corner fold effect ===
            if self.corner_fold_enabled {
                let fold_size = self.corner_fold_size;
                let (fr, fg, fb) = self.corner_fold_color;
                let fold_alpha = self.corner_fold_opacity;
                let mut fold_verts: Vec<RectVertex> = Vec::new();
                for win_info in &frame_glyphs.window_infos {
                    if win_info.is_minibuffer { continue; }
                    let b = &win_info.bounds;
                    // Draw triangular fold in top-right corner
                    // Approximate triangle with thin horizontal strips
                    let steps = fold_size.ceil() as u32;
                    for i in 0..steps {
                        let t = i as f32 / steps as f32;
                        let strip_w = fold_size * (1.0 - t);
                        let strip_x = b.x + b.width - strip_w;
                        let strip_y = b.y + i as f32;
                        let alpha = fold_alpha * (1.0 - t * 0.3);
                        let c = Color::new(fr, fg, fb, alpha);
                        self.add_rect(&mut fold_verts, strip_x, strip_y, strip_w, 1.0, &c);
                    }
                }
                if !fold_verts.is_empty() {
                    let fold_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Corner Fold Buffer"),
                            contents: bytemuck::cast_slice(&fold_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, fold_buf.slice(..));
                    render_pass.draw(0..fold_verts.len() as u32, 0..1);
                }
            }

            // === Step 1i2: Frosted window border effect ===
            if self.frosted_border_enabled {
                let bw = self.frosted_border_width;
                let (fbr, fbg, fbb) = self.frosted_border_color;
                let layers = 4u32;
                let mut frost_verts: Vec<RectVertex> = Vec::new();
                for win_info in &frame_glyphs.window_infos {
                    if win_info.is_minibuffer { continue; }
                    let b = &win_info.bounds;
                    for layer in 0..layers {
                        let t = layer as f32 / layers as f32;
                        let layer_w = bw * (1.0 - t * 0.5);
                        let alpha = self.frosted_border_opacity * (1.0 - t * 0.7);
                        let c = Color::new(fbr, fbg, fbb, alpha);
                        let inset = layer as f32 * (bw / layers as f32);
                        // Top
                        self.add_rect(&mut frost_verts, b.x + inset, b.y + inset, b.width - inset * 2.0, layer_w / layers as f32, &c);
                        // Bottom
                        self.add_rect(&mut frost_verts, b.x + inset, b.y + b.height - inset - layer_w / layers as f32, b.width - inset * 2.0, layer_w / layers as f32, &c);
                        // Left
                        self.add_rect(&mut frost_verts, b.x + inset, b.y + inset, layer_w / layers as f32, b.height - inset * 2.0, &c);
                        // Right
                        self.add_rect(&mut frost_verts, b.x + b.width - inset - layer_w / layers as f32, b.y + inset, layer_w / layers as f32, b.height - inset * 2.0, &c);
                    }
                }
                if !frost_verts.is_empty() {
                    let frost_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Frosted Border Buffer"),
                            contents: bytemuck::cast_slice(&frost_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, frost_buf.slice(..));
                    render_pass.draw(0..frost_verts.len() as u32, 0..1);
                }
            }

            // === Step 1i3: Line number pulse on cursor line ===
            if self.line_number_pulse_enabled {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now();
                    let cycle = self.line_number_pulse_cycle_ms as f64 / 1000.0;
                    let elapsed = now.elapsed().as_secs_f64();
                    let phase = (elapsed % cycle) / cycle;
                    let pulse = ((phase * std::f64::consts::TAU).sin() * 0.5 + 0.5) as f32;
                    let alpha = self.line_number_pulse_intensity * pulse;
                    let (pr, pg, pb) = self.line_number_pulse_color;
                    if alpha > 0.001 {
                        for win_info in &frame_glyphs.window_infos {
                            if !win_info.selected { continue; }
                            let b = &win_info.bounds;
                            let gutter_width = 40.0_f32;
                            let c = Color::new(pr, pg, pb, alpha);
                            let mut pulse_verts: Vec<RectVertex> = Vec::new();
                            self.add_rect(&mut pulse_verts, b.x, anim.y, gutter_width, anim.height, &c);
                            if !pulse_verts.is_empty() {
                                let pulse_buf = self.device.create_buffer_init(
                                    &wgpu::util::BufferInitDescriptor {
                                        label: Some("Line Number Pulse Buffer"),
                                        contents: bytemuck::cast_slice(&pulse_verts),
                                        usage: wgpu::BufferUsages::VERTEX,
                                    },
                                );
                                render_pass.set_pipeline(&self.rect_pipeline);
                                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                                render_pass.set_vertex_buffer(0, pulse_buf.slice(..));
                                render_pass.draw(0..pulse_verts.len() as u32, 0..1);
                            }
                        }
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Step 1i4: Window breathing border animation ===
            if self.breathing_border_enabled {
                let now = std::time::Instant::now();
                let cycle = self.breathing_border_cycle_ms as f64 / 1000.0;
                let elapsed = now.elapsed().as_secs_f64();
                let phase = (elapsed % cycle) / cycle;
                let breath = ((phase * std::f64::consts::TAU).sin() * 0.5 + 0.5) as f32;
                let alpha = self.breathing_border_min_opacity + breath * (self.breathing_border_max_opacity - self.breathing_border_min_opacity);
                let (br, bg, bb) = self.breathing_border_color;
                let c = Color::new(br, bg, bb, alpha);
                let border_w = 2.0_f32;
                let mut breath_verts: Vec<RectVertex> = Vec::new();
                for win_info in &frame_glyphs.window_infos {
                    if win_info.is_minibuffer { continue; }
                    let b = &win_info.bounds;
                    self.add_rect(&mut breath_verts, b.x, b.y, b.width, border_w, &c);
                    self.add_rect(&mut breath_verts, b.x, b.y + b.height - border_w, b.width, border_w, &c);
                    self.add_rect(&mut breath_verts, b.x, b.y, border_w, b.height, &c);
                    self.add_rect(&mut breath_verts, b.x + b.width - border_w, b.y, border_w, b.height, &c);
                }
                if !breath_verts.is_empty() {
                    let breath_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Breathing Border Buffer"),
                            contents: bytemuck::cast_slice(&breath_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, breath_buf.slice(..));
                    render_pass.draw(0..breath_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Step 1i5: Window scanline (CRT) effect ===
            if self.scanlines_enabled {
                let spacing = self.scanlines_spacing.max(1) as f32;
                let (sr, sg, sb) = self.scanlines_color;
                let sa = self.scanlines_opacity;
                let mut sl_verts: Vec<RectVertex> = Vec::new();
                let c = Color::new(sr, sg, sb, sa);
                let mut y = 0.0_f32;
                let h = self.height() as f32;
                let w = self.width() as f32;
                while y < h {
                    self.add_rect(&mut sl_verts, 0.0, y, w, 1.0, &c);
                    y += spacing;
                }
                if !sl_verts.is_empty() {
                    let sl_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Scanlines Buffer"),
                            contents: bytemuck::cast_slice(&sl_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, sl_buf.slice(..));
                    render_pass.draw(0..sl_verts.len() as u32, 0..1);
                }
            }

            // === Step 1j: Cursor spotlight/radial gradient effect ===
            if self.cursor_spotlight_enabled {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let radius = self.cursor_spotlight_radius;
                    let intensity = self.cursor_spotlight_intensity;
                    let (spr, spg, spb) = self.cursor_spotlight_color;
                    // Approximate radial gradient with concentric rings
                    let rings = 20u32;
                    let mut spot_verts: Vec<RectVertex> = Vec::new();
                    for i in (0..rings).rev() {
                        let t = (i as f32 + 1.0) / rings as f32;
                        let r = radius * t;
                        let alpha = intensity * (1.0 - t) * (1.0 - t); // quadratic falloff
                        if alpha > 0.001 {
                            let c = Color::new(spr, spg, spb, alpha);
                            self.add_rect(&mut spot_verts, cx - r, cy - r, r * 2.0, r * 2.0, &c);
                        }
                    }
                    if !spot_verts.is_empty() {
                        let spot_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Cursor Spotlight Buffer"),
                                contents: bytemuck::cast_slice(&spot_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, spot_buf.slice(..));
                        render_pass.draw(0..spot_verts.len() as u32, 0..1);
                    }
                }
            }

            // === Step 1k: Cursor comet tail effect ===
            if self.cursor_comet_enabled {
                let now = std::time::Instant::now();
                let fade_dur = std::time::Duration::from_millis(self.cursor_comet_fade_ms as u64);

                // Record cursor position
                if let Some(ref anim) = animated_cursor {
                    // Only record if position changed
                    let should_add = self.cursor_comet_positions.last()
                        .map(|(px, py, _, _, _)| {
                            (anim.x - px).abs() > 0.5 || (anim.y - py).abs() > 0.5
                        })
                        .unwrap_or(true);
                    if should_add {
                        self.cursor_comet_positions.push((anim.x, anim.y, anim.width, anim.height, now));
                    }
                }

                // Prune old positions
                self.cursor_comet_positions.retain(|&(_, _, _, _, t)| now.duration_since(t) < fade_dur);

                // Keep only trail_length most recent
                let max = self.cursor_comet_trail_length as usize;
                if self.cursor_comet_positions.len() > max {
                    let drain_count = self.cursor_comet_positions.len() - max;
                    self.cursor_comet_positions.drain(0..drain_count);
                }

                // Render ghost cursors
                if !self.cursor_comet_positions.is_empty() {
                    let (cr, cg, cb) = self.cursor_comet_color;
                    let max_alpha = self.cursor_comet_opacity;
                    let count = self.cursor_comet_positions.len();
                    let mut comet_verts: Vec<RectVertex> = Vec::new();
                    for (i, &(px, py, pw, ph, t)) in self.cursor_comet_positions.iter().enumerate() {
                        let age = now.duration_since(t).as_secs_f32();
                        let time_fade = 1.0 - (age / fade_dur.as_secs_f32()).min(1.0);
                        let pos_fade = (i as f32 + 1.0) / count as f32;
                        let alpha = max_alpha * time_fade * pos_fade * 0.5;
                        if alpha > 0.001 {
                            let c = Color::new(cr, cg, cb, alpha);
                            self.add_rect(&mut comet_verts, px, py, pw, ph, &c);
                        }
                    }
                    if !comet_verts.is_empty() {
                        let comet_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Cursor Comet Buffer"),
                                contents: bytemuck::cast_slice(&comet_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, comet_buf.slice(..));
                        render_pass.draw(0..comet_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Step 1l: Cursor particle trail effect ===
            if self.cursor_particles_enabled {
                let now = std::time::Instant::now();
                let lifetime = std::time::Duration::from_millis(self.cursor_particles_lifetime_ms as u64);

                // Detect cursor movement and emit particles
                if let Some(ref anim) = animated_cursor {
                    let cur_pos = (anim.x + anim.width / 2.0, anim.y + anim.height / 2.0);
                    if let Some(prev_pos) = self.cursor_particles_prev_pos {
                        let dx = (cur_pos.0 - prev_pos.0).abs();
                        let dy = (cur_pos.1 - prev_pos.1).abs();
                        if dx > 1.0 || dy > 1.0 {
                            // Emit particles from cursor center
                            let seed = (now.elapsed().subsec_nanos() as u64).wrapping_mul(2654435761);
                            for i in 0..self.cursor_particles_count {
                                // Simple hash-based pseudo-random
                                let h = seed.wrapping_add(i as u64).wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                                let rx = ((h >> 16) & 0xFFFF) as f32 / 65535.0 - 0.5; // -0.5..0.5
                                let ry = ((h >> 32) & 0xFFFF) as f32 / 65535.0 - 0.5;
                                self.cursor_particles.push(CursorParticle {
                                    x: cur_pos.0,
                                    y: cur_pos.1,
                                    vx: rx * 80.0, // random horizontal velocity
                                    vy: ry * 60.0 - 30.0, // slight upward bias
                                    started: now,
                                    lifetime,
                                });
                            }
                        }
                    }
                    self.cursor_particles_prev_pos = Some(cur_pos);
                }

                // Prune expired particles
                self.cursor_particles.retain(|p| now.duration_since(p.started) < p.lifetime);

                // Render particles
                if !self.cursor_particles.is_empty() {
                    let (pr, pg, pb) = self.cursor_particles_color;
                    let gravity = self.cursor_particles_gravity;
                    let mut part_verts: Vec<RectVertex> = Vec::new();
                    for p in &self.cursor_particles {
                        let elapsed = now.duration_since(p.started).as_secs_f32();
                        let t = (elapsed / p.lifetime.as_secs_f32()).min(1.0);
                        let alpha = (1.0 - t) * 0.8;
                        if alpha > 0.001 {
                            let px = p.x + p.vx * elapsed;
                            let py = p.y + p.vy * elapsed + 0.5 * gravity * elapsed * elapsed;
                            let size = 2.0 * (1.0 - t) + 0.5; // shrink over time
                            let c = Color::new(pr, pg, pb, alpha);
                            self.add_rect(&mut part_verts, px - size / 2.0, py - size / 2.0, size, size, &c);
                        }
                    }
                    if !part_verts.is_empty() {
                        let part_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Cursor Particles Buffer"),
                                contents: bytemuck::cast_slice(&part_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, part_buf.slice(..));
                        render_pass.draw(0..part_verts.len() as u32, 0..1);
                    }

                    // Keep redrawing while particles exist
                    self.needs_continuous_redraw = true;
                }
            }

            // Matrix/digital rain effect
            if self.matrix_rain_enabled {
                let fw = self.width as f32 / self.scale_factor;
                let fh = self.height as f32 / self.scale_factor;
                let dt = 1.0 / 60.0_f32;
                let now_ns = std::time::Instant::now().elapsed().subsec_nanos() as u64;

                // Spawn columns if needed
                while self.matrix_rain_columns.len() < self.matrix_rain_column_count as usize {
                    let i = self.matrix_rain_columns.len() as u64;
                    let h = now_ns.wrapping_mul(2654435761).wrapping_add(i * 6364136223846793005);
                    let x = (i as f32 / self.matrix_rain_column_count as f32) * fw;
                    let y = -(((h >> 16) & 0xFFFF) as f32 / 65535.0) * fh;
                    let speed_var = 0.6 + ((h >> 32) & 0xFFFF) as f32 / 65535.0 * 0.8;
                    let length = 30.0 + ((h >> 48) & 0xFF) as f32 / 255.0 * 80.0;
                    self.matrix_rain_columns.push(MatrixColumn {
                        x, y,
                        speed: self.matrix_rain_speed * speed_var,
                        length,
                    });
                }

                // Update positions
                for col in &mut self.matrix_rain_columns {
                    col.y += col.speed * dt;
                    if col.y - col.length > fh {
                        let h = now_ns.wrapping_mul(6364136223846793005).wrapping_add((col.x * 1000.0) as u64);
                        col.y = -(((h >> 16) & 0xFFFF) as f32 / 65535.0) * 50.0;
                        let speed_var = 0.6 + ((h >> 32) & 0xFFFF) as f32 / 65535.0 * 0.8;
                        col.speed = self.matrix_rain_speed * speed_var;
                        col.length = 30.0 + ((h >> 48) & 0xFF) as f32 / 255.0 * 80.0;
                    }
                }

                // Render columns as vertical gradient strips
                let (mr, mg, mb) = self.matrix_rain_color;
                let mut matrix_verts: Vec<RectVertex> = Vec::new();
                for col in &self.matrix_rain_columns {
                    let segments = 10u32;
                    let seg_h = col.length / segments as f32;
                    for s in 0..segments {
                        let y = col.y - col.length + s as f32 * seg_h;
                        if y + seg_h < 0.0 || y > fh { continue; }
                        let frac = s as f32 / segments as f32;
                        let alpha = self.matrix_rain_opacity * frac; // brighter at bottom (head)
                        if alpha < 0.001 { continue; }
                        let c = Color::new(mr, mg, mb, alpha);
                        self.add_rect(&mut matrix_verts, col.x, y, 2.0, seg_h, &c);
                    }
                }
                if !matrix_verts.is_empty() {
                    let matrix_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Matrix Rain Buffer"),
                            contents: bytemuck::cast_slice(&matrix_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, matrix_buf.slice(..));
                    render_pass.draw(0..matrix_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // Frost/ice border effect
            if self.frost_border_enabled {
                let (fr, fg, fb) = self.frost_border_color;
                let bw = self.frost_border_width;
                let base_alpha = self.frost_border_opacity;
                let now_ns = std::time::Instant::now().elapsed().subsec_nanos();
                let mut frost_verts: Vec<RectVertex> = Vec::new();
                for info in &frame_glyphs.window_infos {
                    let b = &info.bounds;
                    // Create crystalline pattern along edges using pseudo-random strips
                    let segments = 30u32;
                    // Top edge
                    for s in 0..segments {
                        let frac = s as f32 / segments as f32;
                        let x = b.x + frac * b.width;
                        let h_seed = ((s as u64).wrapping_mul(2654435761).wrapping_add(info.window_id as u64)) & 0xFF;
                        let h = bw * (0.3 + (h_seed as f32 / 255.0) * 0.7);
                        let a = base_alpha * (0.4 + (h_seed as f32 / 255.0) * 0.6);
                        let seg_w = b.width / segments as f32;
                        let c = Color::new(fr, fg, fb, a);
                        self.add_rect(&mut frost_verts, x, b.y, seg_w, h, &c);
                    }
                    // Bottom edge
                    for s in 0..segments {
                        let frac = s as f32 / segments as f32;
                        let x = b.x + frac * b.width;
                        let h_seed = ((s as u64).wrapping_mul(6364136223846793005).wrapping_add(info.window_id as u64)) & 0xFF;
                        let h = bw * (0.3 + (h_seed as f32 / 255.0) * 0.7);
                        let a = base_alpha * (0.4 + (h_seed as f32 / 255.0) * 0.6);
                        let seg_w = b.width / segments as f32;
                        let c = Color::new(fr, fg, fb, a);
                        self.add_rect(&mut frost_verts, x, b.y + b.height - h, seg_w, h, &c);
                    }
                    // Left edge
                    let v_segments = 20u32;
                    for s in 0..v_segments {
                        let frac = s as f32 / v_segments as f32;
                        let y = b.y + frac * b.height;
                        let w_seed = ((s as u64).wrapping_mul(1442695040888963407).wrapping_add(info.window_id as u64)) & 0xFF;
                        let w = bw * (0.3 + (w_seed as f32 / 255.0) * 0.7);
                        let a = base_alpha * (0.4 + (w_seed as f32 / 255.0) * 0.6);
                        let seg_h = b.height / v_segments as f32;
                        let c = Color::new(fr, fg, fb, a);
                        self.add_rect(&mut frost_verts, b.x, y, w, seg_h, &c);
                    }
                    // Right edge
                    for s in 0..v_segments {
                        let frac = s as f32 / v_segments as f32;
                        let y = b.y + frac * b.height;
                        let w_seed = ((s as u64).wrapping_mul(3141592653589793238).wrapping_add(info.window_id as u64)) & 0xFF;
                        let w = bw * (0.3 + (w_seed as f32 / 255.0) * 0.7);
                        let a = base_alpha * (0.4 + (w_seed as f32 / 255.0) * 0.6);
                        let seg_h = b.height / v_segments as f32;
                        let c = Color::new(fr, fg, fb, a);
                        self.add_rect(&mut frost_verts, b.x + b.width - w, y, w, seg_h, &c);
                    }
                }
                if !frost_verts.is_empty() {
                    let frost_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Frost Border Buffer"),
                            contents: bytemuck::cast_slice(&frost_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, frost_buf.slice(..));
                    render_pass.draw(0..frost_verts.len() as u32, 0..1);
                }
            }

            // Cursor ghost afterimage effect
            if self.cursor_ghost_enabled {
                let now = std::time::Instant::now();
                let fade_dur = std::time::Duration::from_millis(self.cursor_ghost_fade_ms as u64);

                // Detect cursor movement and spawn ghost
                if let Some(ref anim) = animated_cursor {
                    let should_spawn = self.cursor_ghost_entries.is_empty() ||
                        self.cursor_ghost_entries.last().map_or(true, |last| {
                            let dx = (anim.x - last.x).abs();
                            let dy = (anim.y - last.y).abs();
                            (dx > 2.0 || dy > 2.0) && now.duration_since(last.started).as_millis() > 30
                        });
                    if should_spawn {
                        self.cursor_ghost_entries.push(CursorGhostEntry {
                            x: anim.x, y: anim.y,
                            width: anim.width, height: anim.height,
                            started: now,
                        });
                        // Trim to max count
                        while self.cursor_ghost_entries.len() > self.cursor_ghost_count as usize * 2 {
                            self.cursor_ghost_entries.remove(0);
                        }
                    }
                }

                // Prune expired
                self.cursor_ghost_entries.retain(|e| now.duration_since(e.started) < fade_dur);

                if !self.cursor_ghost_entries.is_empty() {
                    let (gr, gg, gb) = self.cursor_ghost_color;
                    let drift = self.cursor_ghost_drift;
                    let mut ghost_verts: Vec<RectVertex> = Vec::new();
                    for entry in &self.cursor_ghost_entries {
                        let elapsed = now.duration_since(entry.started).as_secs_f32();
                        let t = (elapsed / fade_dur.as_secs_f32()).min(1.0);
                        let alpha = self.cursor_ghost_opacity * (1.0 - t) * (1.0 - t);
                        if alpha < 0.001 { continue; }
                        // Drift upward and slightly expand
                        let dy = -drift * t;
                        let scale = 1.0 + t * 0.3;
                        let gw = entry.width * scale;
                        let gh = entry.height * scale;
                        let gx = entry.x - (gw - entry.width) / 2.0;
                        let gy = entry.y + dy - (gh - entry.height) / 2.0;
                        let c = Color::new(gr, gg, gb, alpha);
                        self.add_rect(&mut ghost_verts, gx, gy, gw, gh, &c);
                    }
                    if !ghost_verts.is_empty() {
                        let ghost_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Cursor Ghost Buffer"),
                                contents: bytemuck::cast_slice(&ghost_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, ghost_buf.slice(..));
                        render_pass.draw(0..ghost_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // Edge glow on scroll boundaries
            if self.edge_glow_enabled {
                let now = std::time::Instant::now();
                self.edge_glow_entries.retain(|e| now.duration_since(e.started) < e.duration);
                if !self.edge_glow_entries.is_empty() {
                    let (gr, gg, gb) = self.edge_glow_color;
                    let gh = self.edge_glow_height;
                    let mut glow_verts: Vec<RectVertex> = Vec::new();
                    for entry in &self.edge_glow_entries {
                        let t = now.duration_since(entry.started).as_secs_f32() / entry.duration.as_secs_f32();
                        let fade = (1.0 - t) * (1.0 - t);
                        let base_alpha = self.edge_glow_opacity * fade;
                        let steps = 20u32;
                        for i in 0..steps {
                            let frac = i as f32 / steps as f32;
                            let strip_h = gh / steps as f32;
                            let alpha = base_alpha * (1.0 - frac);
                            if alpha < 0.001 { continue; }
                            let y = if entry.at_top {
                                entry.bounds.y + frac * gh
                            } else {
                                entry.bounds.y + entry.bounds.height - gh + frac * gh
                            };
                            let c = Color::new(gr, gg, gb, alpha);
                            self.add_rect(&mut glow_verts, entry.bounds.x, y, entry.bounds.width, strip_h, &c);
                        }
                    }
                    if !glow_verts.is_empty() {
                        let glow_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Edge Glow Buffer"),
                                contents: bytemuck::cast_slice(&glow_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, glow_buf.slice(..));
                        render_pass.draw(0..glow_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // Rain/drip ambient effect
            if self.rain_effect_enabled {
                let now = std::time::Instant::now();
                let fw = self.width as f32 / self.scale_factor;
                let fh = self.height as f32 / self.scale_factor;
                let dt = 1.0 / 60.0_f32; // approximate frame delta

                // Spawn drops if needed
                while self.rain_drops.len() < self.rain_effect_drop_count as usize {
                    let seed = now.elapsed().subsec_nanos() as u64;
                    let h = seed.wrapping_mul(2654435761).wrapping_add(self.rain_drops.len() as u64 * 6364136223846793005);
                    let x = ((h >> 16) & 0xFFFF) as f32 / 65535.0 * fw;
                    let y = -(((h >> 32) & 0xFFFF) as f32) / 65535.0 * fh * 0.5; // start above screen
                    let speed_var = 0.7 + ((h >> 48) & 0xFFFF) as f32 / 65535.0 * 0.6;
                    let length = 8.0 + ((h >> 8) & 0xFF) as f32 / 255.0 * 16.0;
                    let op = self.rain_effect_opacity * (0.5 + ((h >> 4) & 0xFF) as f32 / 255.0 * 0.5);
                    self.rain_drops.push(RainDrop {
                        x, y,
                        speed: self.rain_effect_speed * speed_var,
                        length,
                        opacity: op,
                    });
                }

                // Update positions
                for drop in &mut self.rain_drops {
                    drop.y += drop.speed * dt;
                    if drop.y > fh {
                        let seed = now.elapsed().subsec_nanos() as u64;
                        let h = seed.wrapping_mul(2654435761).wrapping_add((drop.x * 1000.0) as u64);
                        drop.x = ((h >> 16) & 0xFFFF) as f32 / 65535.0 * fw;
                        drop.y = -drop.length;
                        let speed_var = 0.7 + ((h >> 48) & 0xFFFF) as f32 / 65535.0 * 0.6;
                        drop.speed = self.rain_effect_speed * speed_var;
                    }
                }

                // Render drops
                let (dr, dg, db) = self.rain_effect_color;
                let mut rain_verts: Vec<RectVertex> = Vec::new();
                for drop in &self.rain_drops {
                    let c = Color::new(dr, dg, db, drop.opacity);
                    self.add_rect(&mut rain_verts, drop.x, drop.y, 1.0, drop.length, &c);
                }
                if !rain_verts.is_empty() {
                    let rain_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Rain Buffer"),
                            contents: bytemuck::cast_slice(&rain_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, rain_buf.slice(..));
                    render_pass.draw(0..rain_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // Cursor ripple wave effect
            if self.cursor_ripple_wave_enabled {
                let now = std::time::Instant::now();

                // Detect cursor movement and spawn ripple
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    // Spawn a new ripple on each frame where cursor moves (debounced by checking recent entries)
                    let should_spawn = self.cursor_ripple_waves.is_empty() ||
                        self.cursor_ripple_waves.last().map_or(true, |last| {
                            let dx = (cx - last.x).abs();
                            let dy = (cy - last.y).abs();
                            (dx > 2.0 || dy > 2.0) && now.duration_since(last.started).as_millis() > 50
                        });
                    if should_spawn {
                        self.cursor_ripple_waves.push(RippleWaveEntry {
                            x: cx,
                            y: cy,
                            started: now,
                            duration: std::time::Duration::from_millis(self.cursor_ripple_wave_duration_ms as u64),
                        });
                    }
                }

                // Prune expired
                self.cursor_ripple_waves.retain(|e| now.duration_since(e.started) < e.duration);

                if !self.cursor_ripple_waves.is_empty() {
                    let (rr, rg, rb) = self.cursor_ripple_wave_color;
                    let max_r = self.cursor_ripple_wave_max_radius;
                    let ring_count = self.cursor_ripple_wave_ring_count;
                    let mut ripple_verts: Vec<RectVertex> = Vec::new();
                    for entry in &self.cursor_ripple_waves {
                        let t = now.duration_since(entry.started).as_secs_f32() / entry.duration.as_secs_f32();
                        let current_r = max_r * t;
                        let fade = 1.0 - t;
                        for ring in 0..ring_count {
                            let ring_r = current_r * (1.0 - ring as f32 * 0.2);
                            if ring_r < 1.0 { continue; }
                            let ring_alpha = self.cursor_ripple_wave_opacity * fade * (1.0 - ring as f32 / ring_count as f32);
                            if ring_alpha < 0.001 { continue; }
                            // Approximate circle with rect strips
                            let segments = 32u32;
                            for s in 0..segments {
                                let angle = s as f32 * std::f32::consts::TAU / segments as f32;
                                let next_angle = (s + 1) as f32 * std::f32::consts::TAU / segments as f32;
                                let x1 = entry.x + angle.cos() * ring_r;
                                let y1 = entry.y + angle.sin() * ring_r;
                                let x2 = entry.x + next_angle.cos() * ring_r;
                                let y2 = entry.y + next_angle.sin() * ring_r;
                                let mx = x1.min(x2);
                                let my = y1.min(y2);
                                let w = (x1 - x2).abs().max(1.5);
                                let h = (y1 - y2).abs().max(1.5);
                                let c = Color::new(rr, rg, rb, ring_alpha);
                                self.add_rect(&mut ripple_verts, mx, my, w, h, &c);
                            }
                        }
                    }
                    if !ripple_verts.is_empty() {
                        let ripple_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Cursor Ripple Buffer"),
                                contents: bytemuck::cast_slice(&ripple_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, ripple_buf.slice(..));
                        render_pass.draw(0..ripple_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // Aurora/northern lights effect
            if self.aurora_enabled {
                let now = std::time::Instant::now();
                let elapsed = now.duration_since(self.aurora_start).as_secs_f64() * self.aurora_speed as f64;
                let fw = self.width as f32 / self.scale_factor;
                let ah = self.aurora_height;
                let (r1, g1, b1) = self.aurora_color1;
                let (r2, g2, b2) = self.aurora_color2;
                let mut aurora_verts: Vec<RectVertex> = Vec::new();
                let strips = 60u32;
                let strip_w = fw / strips as f32;
                for i in 0..strips {
                    let x = i as f32 * strip_w;
                    let phase = elapsed + i as f64 * 0.15;
                    let wave1 = (phase * 0.7).sin() as f32 * 0.5 + 0.5;
                    let wave2 = (phase * 1.1 + 2.0).sin() as f32 * 0.5 + 0.5;
                    let blend = (phase * 0.3 + i as f64 * 0.1).sin() as f32 * 0.5 + 0.5;
                    let cr = r1 * (1.0 - blend) + r2 * blend;
                    let cg = g1 * (1.0 - blend) + g2 * blend;
                    let cb = b1 * (1.0 - blend) + b2 * blend;
                    let height = ah * (0.3 + 0.7 * wave1);
                    let alpha = self.aurora_opacity * (0.3 + 0.7 * wave2);
                    // Vertical gradient for each strip
                    let sub_strips = 8u32;
                    let sub_h = height / sub_strips as f32;
                    for j in 0..sub_strips {
                        let frac = j as f32 / sub_strips as f32;
                        let y = frac * height;
                        let strip_alpha = alpha * (1.0 - frac);
                        if strip_alpha < 0.001 { continue; }
                        let c = Color::new(cr, cg, cb, strip_alpha);
                        self.add_rect(&mut aurora_verts, x, y, strip_w + 1.0, sub_h + 0.5, &c);
                    }
                }
                if !aurora_verts.is_empty() {
                    let aurora_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Aurora Buffer"),
                            contents: bytemuck::cast_slice(&aurora_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, aurora_buf.slice(..));
                    render_pass.draw(0..aurora_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Heat distortion effect ===
            if self.heat_distortion_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let ew = self.heat_distortion_edge_width;
                let intensity = self.heat_distortion_intensity;
                let spd = self.heat_distortion_speed;
                let op = self.heat_distortion_opacity;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let mut heat_verts: Vec<RectVertex> = Vec::new();
                let strip_count = 12;
                for i in 0..strip_count {
                    let phase = i as f32 / strip_count as f32 * std::f32::consts::PI * 2.0;
                    let wave = ((now * spd * 3.0 + phase).sin() * 0.5 + 0.5) * intensity;
                    let alpha = op * wave;
                    let cr = 1.0;
                    let cg = 0.6 + wave * 0.4;
                    let cb = 0.2;
                    let ty = i as f32 * ew / strip_count as f32;
                    let fade = 1.0 - ty / ew;
                    let sh = ew / strip_count as f32;
                    let c = Color::new(cr, cg, cb, alpha * fade);
                    self.add_rect(&mut heat_verts, 0.0, ty, fw, sh, &c);
                    let by = fh - ew + ty;
                    self.add_rect(&mut heat_verts, 0.0, by, fw, sh, &c);
                    let lx = i as f32 * ew / strip_count as f32;
                    self.add_rect(&mut heat_verts, lx, 0.0, sh, fh, &c);
                    let rx = fw - ew + lx;
                    self.add_rect(&mut heat_verts, rx, 0.0, sh, fh, &c);
                }
                if !heat_verts.is_empty() {
                    let heat_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Heat Distortion Buffer"),
                            contents: bytemuck::cast_slice(&heat_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, heat_buf.slice(..));
                    render_pass.draw(0..heat_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor lighthouse beam effect ===
            if self.cursor_lighthouse_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let center_x = anim.x + anim.width / 2.0;
                    let center_y = anim.y + anim.height / 2.0;
                    let angle = now * self.cursor_lighthouse_rotation_speed * std::f32::consts::PI * 2.0;
                    let beam_half = (self.cursor_lighthouse_beam_width / 2.0).to_radians();
                    let beam_len = self.cursor_lighthouse_beam_length;
                    let (lr, lg, lb) = self.cursor_lighthouse_color;
                    let lop = self.cursor_lighthouse_opacity;
                    let mut beam_verts: Vec<RectVertex> = Vec::new();
                    let segments = 20;
                    for seg in 0..segments {
                        let t = seg as f32 / segments as f32;
                        let dist = t * beam_len;
                        let bw = 2.0 * dist * beam_half.sin().max(0.02);
                        let dx = angle.cos() * dist;
                        let dy = angle.sin() * dist;
                        let fade = 1.0 - t;
                        let seg_len = beam_len / segments as f32;
                        let rx = center_x + dx - bw / 2.0;
                        let ry = center_y + dy - seg_len / 2.0;
                        let c = Color::new(lr, lg, lb, lop * fade * fade);
                        self.add_rect(&mut beam_verts, rx, ry, bw, seg_len, &c);
                    }
                    if !beam_verts.is_empty() {
                        let beam_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Lighthouse Beam Buffer"),
                                contents: bytemuck::cast_slice(&beam_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, beam_buf.slice(..));
                        render_pass.draw(0..beam_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Neon border effect ===
            if self.neon_border_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (nr, ng, nb) = self.neon_border_color;
                let thick = self.neon_border_thickness;
                let intensity = self.neon_border_intensity;
                let flicker = self.neon_border_flicker;
                let nop = self.neon_border_opacity;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let flicker_val = if flicker > 0.0 {
                    let f1 = (now * 7.3).sin() * 0.5 + 0.5;
                    let f2 = (now * 13.7).sin() * 0.5 + 0.5;
                    1.0 - flicker * f1 * f2
                } else {
                    1.0
                };
                let final_op = nop * intensity * flicker_val;
                let glow_layers = 4;
                let mut neon_verts: Vec<RectVertex> = Vec::new();
                for layer in 0..glow_layers {
                    let expand = layer as f32 * thick * 0.8;
                    let layer_alpha = final_op / (1.0 + layer as f32 * 1.5);
                    let t = thick + expand;
                    let c = Color::new(nr, ng, nb, layer_alpha);
                    self.add_rect(&mut neon_verts, 0.0, 0.0, fw, t, &c);
                    self.add_rect(&mut neon_verts, 0.0, fh - t, fw, t, &c);
                    self.add_rect(&mut neon_verts, 0.0, t, t, fh - 2.0 * t, &c);
                    self.add_rect(&mut neon_verts, fw - t, t, t, fh - 2.0 * t, &c);
                }
                if !neon_verts.is_empty() {
                    let neon_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Neon Border Buffer"),
                            contents: bytemuck::cast_slice(&neon_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, neon_buf.slice(..));
                    render_pass.draw(0..neon_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor sonar ping effect ===
            if self.cursor_sonar_ping_enabled {
                let now = std::time::Instant::now();
                self.cursor_sonar_ping_entries.retain(|e| now.duration_since(e.started) < e.duration);
                let (pr, pg, pb) = self.cursor_sonar_ping_color;
                let ring_count = self.cursor_sonar_ping_ring_count;
                let max_r = self.cursor_sonar_ping_max_radius;
                let pop = self.cursor_sonar_ping_opacity;
                let mut ping_verts: Vec<RectVertex> = Vec::new();
                for entry in &self.cursor_sonar_ping_entries {
                    let elapsed = now.duration_since(entry.started).as_secs_f32();
                    let t = elapsed / entry.duration.as_secs_f32();
                    for ring_idx in 0..ring_count {
                        let ring_t = t - ring_idx as f32 * 0.15;
                        if ring_t < 0.0 || ring_t > 1.0 { continue; }
                        let radius = ring_t * max_r;
                        let fade = 1.0 - ring_t;
                        let ring_op = pop * fade * fade;
                        let ring_thick = 2.0;
                        let seg_count = 24;
                        for seg in 0..seg_count {
                            let a1 = seg as f32 / seg_count as f32 * std::f32::consts::PI * 2.0;
                            let a2 = (seg + 1) as f32 / seg_count as f32 * std::f32::consts::PI * 2.0;
                            let x1 = entry.cx + a1.cos() * radius;
                            let y1 = entry.cy + a1.sin() * radius;
                            let x2 = entry.cx + a2.cos() * radius;
                            let y2 = entry.cy + a2.sin() * radius;
                            let mx = x1.min(x2);
                            let my = y1.min(y2);
                            let sw = (x2 - x1).abs().max(ring_thick);
                            let sh = (y2 - y1).abs().max(ring_thick);
                            let c = Color::new(pr, pg, pb, ring_op);
                            self.add_rect(&mut ping_verts, mx, my, sw, sh, &c);
                        }
                    }
                }
                if !ping_verts.is_empty() {
                    let ping_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Sonar Ping Buffer"),
                            contents: bytemuck::cast_slice(&ping_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, ping_buf.slice(..));
                    render_pass.draw(0..ping_verts.len() as u32, 0..1);
                }
                if !self.cursor_sonar_ping_entries.is_empty() {
                    self.needs_continuous_redraw = true;
                }
            }

            // === Lightning bolt effect ===
            if self.lightning_bolt_enabled {
                let now = std::time::Instant::now();
                let dt = now.duration_since(self.lightning_bolt_last).as_secs_f32();
                self.lightning_bolt_last = now;
                self.lightning_bolt_age += dt;
                let bolt_interval = 1.0 / self.lightning_bolt_frequency;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                // Generate new bolt if timer expired
                if self.lightning_bolt_age >= bolt_interval || self.lightning_bolt_segments.is_empty() {
                    self.lightning_bolt_age = 0.0;
                    self.lightning_bolt_segments.clear();
                    let time_seed = now.duration_since(self.aurora_start).as_nanos() as u64;
                    // Random start point on frame edge
                    let h1 = time_seed.wrapping_mul(2654435761);
                    let start_x = (h1 & 0xFFFF) as f32 / 65535.0 * fw;
                    let mut x = start_x;
                    let mut y = 0.0_f32;
                    let seg_count = 8 + ((h1 >> 16) & 7) as u32;
                    for i in 0..seg_count {
                        let h2 = time_seed.wrapping_mul(6364136223846793005).wrapping_add(i as u64 * 2654435761);
                        let dx = ((h2 & 0xFFFF) as f32 / 65535.0 - 0.5) * 60.0;
                        let dy = ((h2 >> 16) & 0xFFFF) as f32 / 65535.0 * (fh / seg_count as f32) + 10.0;
                        let nx = (x + dx).clamp(0.0, fw);
                        let ny = (y + dy).clamp(0.0, fh);
                        self.lightning_bolt_segments.push((x, y, nx, ny));
                        x = nx;
                        y = ny;
                    }
                }
                // Render bolt segments
                let (lr, lg, lb) = self.lightning_bolt_color;
                let bolt_op = self.lightning_bolt_opacity * self.lightning_bolt_intensity;
                let fade = (1.0 - self.lightning_bolt_age / (bolt_interval * 0.8)).max(0.0);
                let mut bolt_verts: Vec<RectVertex> = Vec::new();
                for &(x1, y1, x2, y2) in &self.lightning_bolt_segments {
                    let dx = x2 - x1;
                    let dy = y2 - y1;
                    let len = (dx * dx + dy * dy).sqrt().max(1.0);
                    let thick = 2.0;
                    let mx = x1.min(x2);
                    let my = y1.min(y2);
                    let bw = (x2 - x1).abs().max(thick);
                    let bh = (y2 - y1).abs().max(thick);
                    let c = Color::new(lr, lg, lb, bolt_op * fade);
                    self.add_rect(&mut bolt_verts, mx, my, bw, bh, &c);
                    // Glow layer
                    let gc = Color::new(lr, lg, lb, bolt_op * fade * 0.3);
                    self.add_rect(&mut bolt_verts, mx - 2.0, my - 2.0, bw + 4.0, bh + 4.0, &gc);
                    let _ = len; // suppress unused warning
                }
                if !bolt_verts.is_empty() {
                    let bolt_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Lightning Bolt Buffer"),
                            contents: bytemuck::cast_slice(&bolt_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, bolt_buf.slice(..));
                    render_pass.draw(0..bolt_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor orbit particles effect ===
            if self.cursor_orbit_particles_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let (pr, pg, pb) = self.cursor_orbit_particles_color;
                    let pop = self.cursor_orbit_particles_opacity;
                    let count = self.cursor_orbit_particles_count;
                    let radius = self.cursor_orbit_particles_radius;
                    let speed = self.cursor_orbit_particles_speed;
                    let mut orbit_verts: Vec<RectVertex> = Vec::new();
                    for i in 0..count {
                        let phase = i as f32 / count as f32 * std::f32::consts::PI * 2.0;
                        let angle = now * speed * std::f32::consts::PI * 2.0 + phase;
                        let r_var = radius * (1.0 + 0.2 * (now * 2.0 + phase).sin());
                        let px = cx + angle.cos() * r_var;
                        let py = cy + angle.sin() * r_var;
                        let size = 3.0;
                        let c = Color::new(pr, pg, pb, pop);
                        self.add_rect(&mut orbit_verts, px - size / 2.0, py - size / 2.0, size, size, &c);
                        // Small glow
                        let gc = Color::new(pr, pg, pb, pop * 0.3);
                        self.add_rect(&mut orbit_verts, px - size, py - size, size * 2.0, size * 2.0, &gc);
                    }
                    if !orbit_verts.is_empty() {
                        let orbit_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Orbit Particles Buffer"),
                                contents: bytemuck::cast_slice(&orbit_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, orbit_buf.slice(..));
                        render_pass.draw(0..orbit_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Plasma border effect ===
            if self.plasma_border_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (r1, g1, b1) = self.plasma_border_color1;
                let (r2, g2, b2) = self.plasma_border_color2;
                let bw = self.plasma_border_width;
                let spd = self.plasma_border_speed;
                let pop = self.plasma_border_opacity;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let perimeter = 2.0 * (fw + fh);
                let seg_count = 80u32;
                let seg_len = perimeter / seg_count as f32;
                let mut plasma_verts: Vec<RectVertex> = Vec::new();
                for i in 0..seg_count {
                    let t = i as f32 / seg_count as f32;
                    let dist = t * perimeter;
                    let phase = now * spd * 3.0 + t * 12.0;
                    let blend = (phase.sin() * 0.5 + 0.5);
                    let cr = r1 * (1.0 - blend) + r2 * blend;
                    let cg = g1 * (1.0 - blend) + g2 * blend;
                    let cb = b1 * (1.0 - blend) + b2 * blend;
                    let pulse = 0.7 + 0.3 * (phase * 0.7).sin();
                    let c = Color::new(cr, cg, cb, pop * pulse);
                    // Map distance to position on border
                    if dist < fw {
                        // Top edge
                        self.add_rect(&mut plasma_verts, dist, 0.0, seg_len.min(fw - dist), bw, &c);
                    } else if dist < fw + fh {
                        // Right edge
                        let d = dist - fw;
                        self.add_rect(&mut plasma_verts, fw - bw, d, bw, seg_len.min(fh - d), &c);
                    } else if dist < 2.0 * fw + fh {
                        // Bottom edge
                        let d = dist - fw - fh;
                        self.add_rect(&mut plasma_verts, fw - d - seg_len.min(fw - d), fh - bw, seg_len.min(fw - d), bw, &c);
                    } else {
                        // Left edge
                        let d = dist - 2.0 * fw - fh;
                        self.add_rect(&mut plasma_verts, 0.0, fh - d - seg_len.min(fh - d), bw, seg_len.min(fh - d), &c);
                    }
                }
                if !plasma_verts.is_empty() {
                    let plasma_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Plasma Border Buffer"),
                            contents: bytemuck::cast_slice(&plasma_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, plasma_buf.slice(..));
                    render_pass.draw(0..plasma_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor heartbeat pulse effect ===
            if self.cursor_heartbeat_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let (hr, hg, hb) = self.cursor_heartbeat_color;
                    let hop = self.cursor_heartbeat_opacity;
                    let max_r = self.cursor_heartbeat_max_radius;
                    let beat_period = 60.0 / self.cursor_heartbeat_bpm;
                    let t_in_beat = (now % beat_period) / beat_period;
                    let mut heartbeat_verts: Vec<RectVertex> = Vec::new();
                    // Double pulse: first at t=0.0-0.15, second at t=0.2-0.35
                    let pulses = [(0.0_f32, 0.15_f32), (0.2, 0.35)];
                    for &(start, end) in &pulses {
                        if t_in_beat >= start && t_in_beat <= end {
                            let pt = (t_in_beat - start) / (end - start);
                            let radius = pt * max_r;
                            let fade = 1.0 - pt;
                            let ring_thick = 2.0;
                            let seg_count = 20;
                            for seg in 0..seg_count {
                                let a1 = seg as f32 / seg_count as f32 * std::f32::consts::PI * 2.0;
                                let a2 = (seg + 1) as f32 / seg_count as f32 * std::f32::consts::PI * 2.0;
                                let x1 = cx + a1.cos() * radius;
                                let y1 = cy + a1.sin() * radius;
                                let x2 = cx + a2.cos() * radius;
                                let y2 = cy + a2.sin() * radius;
                                let mx = x1.min(x2);
                                let my = y1.min(y2);
                                let sw = (x2 - x1).abs().max(ring_thick);
                                let sh = (y2 - y1).abs().max(ring_thick);
                                let c = Color::new(hr, hg, hb, hop * fade * fade);
                                self.add_rect(&mut heartbeat_verts, mx, my, sw, sh, &c);
                            }
                        }
                    }
                    if !heartbeat_verts.is_empty() {
                        let hb_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Heartbeat Pulse Buffer"),
                                contents: bytemuck::cast_slice(&heartbeat_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, hb_buf.slice(..));
                        render_pass.draw(0..heartbeat_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Topographic contour effect ===
            if self.topo_contour_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (tr, tg, tb) = self.topo_contour_color;
                let top = self.topo_contour_opacity;
                let spacing = self.topo_contour_spacing.max(5.0);
                let spd = self.topo_contour_speed;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let mut topo_verts: Vec<RectVertex> = Vec::new();
                let num_contours = (fh / spacing) as i32 + 2;
                let line_thick = 1.0;
                for i in 0..num_contours {
                    let base_y = i as f32 * spacing + (now * spd * 10.0) % spacing;
                    let num_seg = 40;
                    for seg in 0..num_seg {
                        let x = seg as f32 / num_seg as f32 * fw;
                        let wave = (x * 0.01 + i as f32 * 0.5 + now * spd * 0.3).sin() * spacing * 0.3
                            + (x * 0.02 + i as f32 * 1.2).cos() * spacing * 0.15;
                        let y = base_y + wave;
                        if y >= 0.0 && y < fh {
                            let seg_w = fw / num_seg as f32 + 1.0;
                            let alpha = top * (0.5 + 0.5 * (x * 0.005 + now * 0.5).sin());
                            let c = Color::new(tr, tg, tb, alpha);
                            self.add_rect(&mut topo_verts, x, y, seg_w, line_thick, &c);
                        }
                    }
                }
                if !topo_verts.is_empty() {
                    let tp_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Topo Contour Buffer"),
                            contents: bytemuck::cast_slice(&topo_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, tp_buf.slice(..));
                    render_pass.draw(0..topo_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor metronome tick effect ===
            if self.cursor_metronome_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y;
                    // Detect cursor move
                    if (cx - self.cursor_metronome_last_x).abs() > 1.0 || (cy - self.cursor_metronome_last_y).abs() > 1.0 {
                        self.cursor_metronome_tick_start = Some(std::time::Instant::now());
                        self.cursor_metronome_last_x = cx;
                        self.cursor_metronome_last_y = cy;
                    }
                    if let Some(start) = self.cursor_metronome_tick_start {
                        let elapsed = start.elapsed().as_secs_f32();
                        let duration = self.cursor_metronome_fade_ms as f32 / 1000.0;
                        if elapsed < duration {
                            let t = elapsed / duration;
                            let fade = 1.0 - t;
                            let (mr, mg, mb) = self.cursor_metronome_color;
                            let mop = self.cursor_metronome_opacity;
                            let th = self.cursor_metronome_tick_height;
                            let mut tick_verts: Vec<RectVertex> = Vec::new();
                            // Vertical tick line above cursor
                            let tick_w = 2.0;
                            let c = Color::new(mr, mg, mb, mop * fade);
                            self.add_rect(&mut tick_verts, cx - tick_w / 2.0, cy - th * (1.0 - t * 0.3), tick_w, th, &c);
                            // Small horizontal cap
                            let cap_w = 6.0;
                            let cc = Color::new(mr, mg, mb, mop * fade * 0.8);
                            self.add_rect(&mut tick_verts, cx - cap_w / 2.0, cy - th * (1.0 - t * 0.3), cap_w, 1.5, &cc);

                            let tk_buf = self.device.create_buffer_init(
                                &wgpu::util::BufferInitDescriptor {
                                    label: Some("Metronome Tick Buffer"),
                                    contents: bytemuck::cast_slice(&tick_verts),
                                    usage: wgpu::BufferUsages::VERTEX,
                                },
                            );
                            render_pass.set_pipeline(&self.rect_pipeline);
                            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                            render_pass.set_vertex_buffer(0, tk_buf.slice(..));
                            render_pass.draw(0..tick_verts.len() as u32, 0..1);
                            self.needs_continuous_redraw = true;
                        } else {
                            self.cursor_metronome_tick_start = None;
                        }
                    }
                }
            }

            // === Constellation overlay effect ===
            if self.constellation_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (sr, sg, sb) = self.constellation_color;
                let sop = self.constellation_opacity;
                let count = self.constellation_star_count.min(200);
                let conn_dist = self.constellation_connect_dist;
                let tspd = self.constellation_twinkle_speed;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let mut star_verts: Vec<RectVertex> = Vec::new();
                // Generate deterministic star positions
                let mut stars: Vec<(f32, f32)> = Vec::with_capacity(count as usize);
                for i in 0..count {
                    let mut h = i.wrapping_mul(2654435761);
                    h ^= h >> 16;
                    let x = (h as f32 / u32::MAX as f32) * fw;
                    h = h.wrapping_mul(0x45d9f3b);
                    h ^= h >> 16;
                    let y = (h as f32 / u32::MAX as f32) * fh;
                    stars.push((x, y));
                    // Twinkle
                    let twinkle = (0.4 + 0.6 * (now * tspd + i as f32 * 2.1).sin()).max(0.0);
                    let sz = 2.0 + twinkle * 2.0;
                    let c = Color::new(sr, sg, sb, sop * twinkle);
                    self.add_rect(&mut star_verts, x - sz / 2.0, y - sz / 2.0, sz, sz, &c);
                }
                // Connect nearby stars
                for i in 0..stars.len() {
                    for j in (i + 1)..stars.len().min(i + 10) {
                        let dx = stars[j].0 - stars[i].0;
                        let dy = stars[j].1 - stars[i].1;
                        let dist = (dx * dx + dy * dy).sqrt();
                        if dist < conn_dist && dist > 1.0 {
                            let alpha = sop * 0.3 * (1.0 - dist / conn_dist);
                            let c = Color::new(sr, sg, sb, alpha);
                            let mx = stars[i].0.min(stars[j].0);
                            let my = stars[i].1.min(stars[j].1);
                            let lw = dx.abs().max(1.0);
                            let lh = dy.abs().max(1.0);
                            self.add_rect(&mut star_verts, mx, my, lw, lh, &c);
                        }
                    }
                }
                if !star_verts.is_empty() {
                    let st_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Constellation Buffer"),
                            contents: bytemuck::cast_slice(&star_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, st_buf.slice(..));
                    render_pass.draw(0..star_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor radar sweep effect ===
            if self.cursor_radar_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let (rr, rg, rb) = self.cursor_radar_color;
                    let rop = self.cursor_radar_opacity;
                    let radius = self.cursor_radar_radius;
                    let spd = self.cursor_radar_speed;
                    let sweep_angle = now * spd * std::f32::consts::PI * 2.0;
                    let mut radar_verts: Vec<RectVertex> = Vec::new();
                    // Sweep beam line
                    let beam_segs = 15;
                    for seg in 0..beam_segs {
                        let t = seg as f32 / beam_segs as f32;
                        let r = t * radius;
                        let px = cx + sweep_angle.cos() * r;
                        let py = cy + sweep_angle.sin() * r;
                        let dot = 2.0;
                        let c = Color::new(rr, rg, rb, rop * (1.0 - t * 0.5));
                        self.add_rect(&mut radar_verts, px - dot / 2.0, py - dot / 2.0, dot, dot, &c);
                    }
                    // Fading trail (previous sweep positions)
                    let trail_count = 20;
                    for trail in 1..trail_count {
                        let trail_angle = sweep_angle - trail as f32 * 0.08;
                        let fade = 1.0 - trail as f32 / trail_count as f32;
                        for seg in 0..8 {
                            let t = seg as f32 / 8.0;
                            let r = t * radius;
                            let px = cx + trail_angle.cos() * r;
                            let py = cy + trail_angle.sin() * r;
                            let dot = 1.5;
                            let c = Color::new(rr, rg, rb, rop * fade * fade * 0.3);
                            self.add_rect(&mut radar_verts, px - dot / 2.0, py - dot / 2.0, dot, dot, &c);
                        }
                    }
                    // Concentric range rings
                    let ring_count = 3;
                    for ring in 1..=ring_count {
                        let ring_r = radius * ring as f32 / ring_count as f32;
                        let ring_segs = 24;
                        for seg in 0..ring_segs {
                            let a = seg as f32 / ring_segs as f32 * std::f32::consts::PI * 2.0;
                            let px = cx + a.cos() * ring_r;
                            let py = cy + a.sin() * ring_r;
                            let c = Color::new(rr, rg, rb, rop * 0.15);
                            self.add_rect(&mut radar_verts, px - 0.5, py - 0.5, 1.0, 1.0, &c);
                        }
                    }
                    if !radar_verts.is_empty() {
                        let rd_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Radar Sweep Buffer"),
                                contents: bytemuck::cast_slice(&radar_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, rd_buf.slice(..));
                        render_pass.draw(0..radar_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Kaleidoscope overlay effect ===
            if self.kaleidoscope_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (kr, kg, kb) = self.kaleidoscope_color;
                let kop = self.kaleidoscope_opacity;
                let segs = self.kaleidoscope_segments.max(3).min(12);
                let spd = self.kaleidoscope_speed;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let cx = fw / 2.0;
                let cy = fh / 2.0;
                let mut kal_verts: Vec<RectVertex> = Vec::new();
                let angle_step = std::f32::consts::PI * 2.0 / segs as f32;
                let max_r = (fw * fw + fh * fh).sqrt() * 0.5;
                for seg in 0..segs {
                    let base_angle = seg as f32 * angle_step + now * spd * 0.3;
                    let num_shapes = 8;
                    for s in 0..num_shapes {
                        let r = max_r * (s as f32 + 1.0) / num_shapes as f32 * 0.8;
                        let wobble = (now * spd * 0.7 + s as f32 * 1.3).sin() * 0.15;
                        let a = base_angle + wobble;
                        let px = cx + a.cos() * r;
                        let py = cy + a.sin() * r;
                        let sz = 4.0 + (now * spd + s as f32 * 0.7).sin().abs() * 8.0;
                        let alpha = kop * (0.3 + 0.7 * (1.0 - r / max_r));
                        let c = Color::new(kr, kg, kb, alpha);
                        self.add_rect(&mut kal_verts, px - sz / 2.0, py - sz / 2.0, sz, sz, &c);
                    }
                }
                if !kal_verts.is_empty() {
                    let kl_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Kaleidoscope Buffer"),
                            contents: bytemuck::cast_slice(&kal_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, kl_buf.slice(..));
                    render_pass.draw(0..kal_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor ripple ring effect ===
            if self.cursor_ripple_ring_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    // Detect cursor move
                    if (cx - self.cursor_ripple_ring_last_x).abs() > 1.0 || (cy - self.cursor_ripple_ring_last_y).abs() > 1.0 {
                        self.cursor_ripple_ring_start = Some(std::time::Instant::now());
                        self.cursor_ripple_ring_last_x = cx;
                        self.cursor_ripple_ring_last_y = cy;
                    }
                    if let Some(start) = self.cursor_ripple_ring_start {
                        let elapsed = start.elapsed().as_secs_f32();
                        let max_r = self.cursor_ripple_ring_max_radius;
                        let duration = max_r / (self.cursor_ripple_ring_speed * 60.0);
                        if elapsed < duration {
                            let t = elapsed / duration;
                            let (rr, rg, rb) = self.cursor_ripple_ring_color;
                            let rop = self.cursor_ripple_ring_opacity;
                            let count = self.cursor_ripple_ring_count.max(1).min(8);
                            let mut ring_verts: Vec<RectVertex> = Vec::new();
                            for ring in 0..count {
                                let ring_t = (t - ring as f32 * 0.15).max(0.0).min(1.0);
                                if ring_t <= 0.0 { continue; }
                                let radius = ring_t * max_r;
                                let fade = (1.0 - ring_t) * rop;
                                let ring_segs = 32;
                                for seg in 0..ring_segs {
                                    let a = seg as f32 / ring_segs as f32 * std::f32::consts::PI * 2.0;
                                    let px = cx + a.cos() * radius;
                                    let py = cy + a.sin() * radius;
                                    let dot = 2.0;
                                    let c = Color::new(rr, rg, rb, fade);
                                    self.add_rect(&mut ring_verts, px - dot / 2.0, py - dot / 2.0, dot, dot, &c);
                                }
                            }
                            if !ring_verts.is_empty() {
                                let rr_buf = self.device.create_buffer_init(
                                    &wgpu::util::BufferInitDescriptor {
                                        label: Some("Ripple Ring Buffer"),
                                        contents: bytemuck::cast_slice(&ring_verts),
                                        usage: wgpu::BufferUsages::VERTEX,
                                    },
                                );
                                render_pass.set_pipeline(&self.rect_pipeline);
                                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                                render_pass.set_vertex_buffer(0, rr_buf.slice(..));
                                render_pass.draw(0..ring_verts.len() as u32, 0..1);
                            }
                            self.needs_continuous_redraw = true;
                        } else {
                            self.cursor_ripple_ring_start = None;
                        }
                    }
                }
            }

            // === Noise field overlay effect ===
            if self.noise_field_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (nr, ng, nb) = self.noise_field_color;
                let nop = self.noise_field_opacity;
                let scale = self.noise_field_scale.max(10.0);
                let spd = self.noise_field_speed;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let mut noise_verts: Vec<RectVertex> = Vec::new();
                let step = scale;
                let cols = (fw / step) as i32 + 1;
                let rows = (fh / step) as i32 + 1;
                for row in 0..rows {
                    for col in 0..cols {
                        let x = col as f32 * step;
                        let y = row as f32 * step;
                        // Pseudo-noise using sin combinations
                        let n = ((x * 0.013 + now * spd * 0.7).sin()
                            * (y * 0.017 + now * spd * 0.5).cos()
                            * (x * 0.009 + y * 0.011 + now * spd * 0.3).sin())
                            .abs();
                        let alpha = nop * n;
                        if alpha > 0.005 {
                            let c = Color::new(nr, ng, nb, alpha);
                            self.add_rect(&mut noise_verts, x, y, step, step, &c);
                        }
                    }
                }
                if !noise_verts.is_empty() {
                    let nf_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Noise Field Buffer"),
                            contents: bytemuck::cast_slice(&noise_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, nf_buf.slice(..));
                    render_pass.draw(0..noise_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor scope effect ===
            if self.cursor_scope_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let (chr, chg, chb) = self.cursor_scope_color;
                    let chop = self.cursor_scope_opacity;
                    let thick = self.cursor_scope_thickness;
                    let gap = self.cursor_scope_gap;
                    let fw = self.width() as f32;
                    let fh = self.height() as f32;
                    let mut ch_verts: Vec<RectVertex> = Vec::new();
                    let c = Color::new(chr, chg, chb, chop);
                    // Horizontal line — left of cursor
                    if cx - gap > 0.0 {
                        self.add_rect(&mut ch_verts, 0.0, cy - thick / 2.0, cx - gap, thick, &c);
                    }
                    // Horizontal line — right of cursor
                    if cx + gap < fw {
                        self.add_rect(&mut ch_verts, cx + gap, cy - thick / 2.0, fw - (cx + gap), thick, &c);
                    }
                    // Vertical line — above cursor
                    if cy - gap > 0.0 {
                        self.add_rect(&mut ch_verts, cx - thick / 2.0, 0.0, thick, cy - gap, &c);
                    }
                    // Vertical line — below cursor
                    if cy + gap < fh {
                        self.add_rect(&mut ch_verts, cx - thick / 2.0, cy + gap, thick, fh - (cy + gap), &c);
                    }
                    if !ch_verts.is_empty() {
                        let ch_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Cursor Scope Buffer"),
                                contents: bytemuck::cast_slice(&ch_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, ch_buf.slice(..));
                        render_pass.draw(0..ch_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Spiral vortex overlay effect ===
            if self.spiral_vortex_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (vr, vg, vb) = self.spiral_vortex_color;
                let vop = self.spiral_vortex_opacity;
                let arms = self.spiral_vortex_arms.max(2).min(12);
                let spd = self.spiral_vortex_speed;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let cx = fw / 2.0;
                let cy = fh / 2.0;
                let max_r = (fw * fw + fh * fh).sqrt() * 0.5;
                let mut vortex_verts: Vec<RectVertex> = Vec::new();
                for arm in 0..arms {
                    let arm_offset = arm as f32 * std::f32::consts::PI * 2.0 / arms as f32;
                    let steps = 60;
                    for step in 0..steps {
                        let t = step as f32 / steps as f32;
                        let r = t * max_r;
                        let angle = arm_offset + t * 6.0 + now * spd;
                        let px = cx + angle.cos() * r;
                        let py = cy + angle.sin() * r;
                        let sz = 2.0 + t * 3.0;
                        let alpha = vop * (1.0 - t * 0.7);
                        let c = Color::new(vr, vg, vb, alpha);
                        self.add_rect(&mut vortex_verts, px - sz / 2.0, py - sz / 2.0, sz, sz, &c);
                    }
                }
                if !vortex_verts.is_empty() {
                    let vx_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Spiral Vortex Buffer"),
                            contents: bytemuck::cast_slice(&vortex_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, vx_buf.slice(..));
                    render_pass.draw(0..vortex_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor shockwave effect ===
            if self.cursor_shockwave_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    // Detect cursor move
                    if (cx - self.cursor_shockwave_last_x).abs() > 1.0 || (cy - self.cursor_shockwave_last_y).abs() > 1.0 {
                        self.cursor_shockwave_start = Some(std::time::Instant::now());
                        self.cursor_shockwave_last_x = cx;
                        self.cursor_shockwave_last_y = cy;
                    }
                    if let Some(start) = self.cursor_shockwave_start {
                        let elapsed = start.elapsed().as_secs_f32();
                        let max_r = self.cursor_shockwave_radius;
                        let duration = 1.0 / self.cursor_shockwave_decay;
                        if elapsed < duration {
                            let t = elapsed / duration;
                            let radius = t * max_r;
                            let (sr, sg, sb) = self.cursor_shockwave_color;
                            let sop = self.cursor_shockwave_opacity;
                            let fade = (1.0 - t) * (1.0 - t);
                            let mut sw_verts: Vec<RectVertex> = Vec::new();
                            // Ring at current radius
                            let ring_thick = 3.0 * (1.0 - t * 0.5);
                            let ring_segs = 48;
                            for seg in 0..ring_segs {
                                let a = seg as f32 / ring_segs as f32 * std::f32::consts::PI * 2.0;
                                let px = cx + a.cos() * radius;
                                let py = cy + a.sin() * radius;
                                let c = Color::new(sr, sg, sb, sop * fade);
                                self.add_rect(&mut sw_verts, px - ring_thick / 2.0, py - ring_thick / 2.0, ring_thick, ring_thick, &c);
                            }
                            // Inner glow
                            let inner_r = radius * 0.7;
                            for seg in 0..24 {
                                let a = seg as f32 / 24.0 * std::f32::consts::PI * 2.0;
                                let px = cx + a.cos() * inner_r;
                                let py = cy + a.sin() * inner_r;
                                let c = Color::new(sr, sg, sb, sop * fade * 0.3);
                                self.add_rect(&mut sw_verts, px - 1.0, py - 1.0, 2.0, 2.0, &c);
                            }
                            if !sw_verts.is_empty() {
                                let sw_buf = self.device.create_buffer_init(
                                    &wgpu::util::BufferInitDescriptor {
                                        label: Some("Shockwave Buffer"),
                                        contents: bytemuck::cast_slice(&sw_verts),
                                        usage: wgpu::BufferUsages::VERTEX,
                                    },
                                );
                                render_pass.set_pipeline(&self.rect_pipeline);
                                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                                render_pass.set_vertex_buffer(0, sw_buf.slice(..));
                                render_pass.draw(0..sw_verts.len() as u32, 0..1);
                            }
                            self.needs_continuous_redraw = true;
                        } else {
                            self.cursor_shockwave_start = None;
                        }
                    }
                }
            }

            // === Diamond lattice overlay effect ===
            if self.diamond_lattice_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (dr, dg, db) = self.diamond_lattice_color;
                let dop = self.diamond_lattice_opacity;
                let cell = self.diamond_lattice_cell_size.max(10.0);
                let shspd = self.diamond_lattice_shimmer_speed;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let mut dl_verts: Vec<RectVertex> = Vec::new();
                let half = cell / 2.0;
                let cols = (fw / cell) as i32 + 2;
                let rows = (fh / half) as i32 + 2;
                let line_thick = 1.0;
                for row in 0..rows {
                    for col in 0..cols {
                        let cx = col as f32 * cell + if row % 2 == 1 { half } else { 0.0 };
                        let cy = row as f32 * half;
                        let shimmer = (0.5 + 0.5 * ((cx * 0.02 + cy * 0.02 + now * shspd).sin())).max(0.0);
                        let alpha = dop * shimmer;
                        let c = Color::new(dr, dg, db, alpha);
                        // Draw diamond edges as small rects
                        // Top-right edge
                        let segs = 4;
                        for s in 0..segs {
                            let t = s as f32 / segs as f32;
                            let px = cx + t * half;
                            let py = cy - half + t * half;
                            self.add_rect(&mut dl_verts, px, py, line_thick, line_thick, &c);
                        }
                        // Bottom-right edge
                        for s in 0..segs {
                            let t = s as f32 / segs as f32;
                            let px = cx + half - t * half;
                            let py = cy + t * half;
                            self.add_rect(&mut dl_verts, px, py, line_thick, line_thick, &c);
                        }
                    }
                }
                if !dl_verts.is_empty() {
                    let dl_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Diamond Lattice Buffer"),
                            contents: bytemuck::cast_slice(&dl_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, dl_buf.slice(..));
                    render_pass.draw(0..dl_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor gravity well effect ===
            if self.cursor_gravity_well_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let (gr, gg, gb) = self.cursor_gravity_well_color;
                    let gop = self.cursor_gravity_well_opacity;
                    let field_r = self.cursor_gravity_well_field_radius;
                    let lines = self.cursor_gravity_well_line_count.max(4).min(24);
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let mut gw_verts: Vec<RectVertex> = Vec::new();
                    for line in 0..lines {
                        let base_angle = line as f32 * std::f32::consts::PI * 2.0 / lines as f32 + now * 0.2;
                        let steps = 20;
                        for step in 0..steps {
                            let t = step as f32 / steps as f32;
                            let r = field_r * (1.0 - t);
                            // Spiral inward with slight curve
                            let curve = t * t * 1.5;
                            let angle = base_angle + curve;
                            let px = cx + angle.cos() * r;
                            let py = cy + angle.sin() * r;
                            let sz = 1.5 + t * 2.0;
                            let alpha = gop * t * t;
                            let c = Color::new(gr, gg, gb, alpha);
                            self.add_rect(&mut gw_verts, px - sz / 2.0, py - sz / 2.0, sz, sz, &c);
                        }
                    }
                    if !gw_verts.is_empty() {
                        let gw_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Gravity Well Buffer"),
                                contents: bytemuck::cast_slice(&gw_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, gw_buf.slice(..));
                        render_pass.draw(0..gw_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Wave interference overlay effect ===
            if self.wave_interference_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (wr, wg, wb) = self.wave_interference_color;
                let wop = self.wave_interference_opacity;
                let wl = self.wave_interference_wavelength.max(10.0);
                let sources = self.wave_interference_source_count.max(2).min(6);
                let spd = self.wave_interference_speed;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let mut wave_verts: Vec<RectVertex> = Vec::new();
                // Generate deterministic source positions
                let mut src_pos: Vec<(f32, f32)> = Vec::new();
                for i in 0..sources {
                    let mut h = i.wrapping_mul(2654435761);
                    h ^= h >> 16;
                    let x = (h as f32 / u32::MAX as f32) * fw;
                    h = h.wrapping_mul(0x45d9f3b);
                    h ^= h >> 16;
                    let y = (h as f32 / u32::MAX as f32) * fh;
                    src_pos.push((x, y));
                }
                let step = wl * 0.5;
                let cols = (fw / step) as i32 + 1;
                let rows = (fh / step) as i32 + 1;
                for row in 0..rows {
                    for col in 0..cols {
                        let x = col as f32 * step;
                        let y = row as f32 * step;
                        let mut val = 0.0f32;
                        for src in &src_pos {
                            let dx = x - src.0;
                            let dy = y - src.1;
                            let dist = (dx * dx + dy * dy).sqrt();
                            val += (dist / wl * std::f32::consts::PI * 2.0 - now * spd * 3.0).sin();
                        }
                        let intensity = (val / sources as f32).abs();
                        let alpha = wop * intensity;
                        if alpha > 0.005 {
                            let c = Color::new(wr, wg, wb, alpha);
                            self.add_rect(&mut wave_verts, x, y, step, step, &c);
                        }
                    }
                }
                if !wave_verts.is_empty() {
                    let wi_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Wave Interference Buffer"),
                            contents: bytemuck::cast_slice(&wave_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, wi_buf.slice(..));
                    render_pass.draw(0..wave_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor portal effect ===
            if self.cursor_portal_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let (pr, pg, pb) = self.cursor_portal_color;
                    let pop = self.cursor_portal_opacity;
                    let radius = self.cursor_portal_radius;
                    let spd = self.cursor_portal_speed;
                    let mut portal_verts: Vec<RectVertex> = Vec::new();
                    // Outer swirling ring
                    let ring_segs = 36;
                    for seg in 0..ring_segs {
                        let a = seg as f32 / ring_segs as f32 * std::f32::consts::PI * 2.0;
                        let wobble = (a * 3.0 + now * spd * 2.0).sin() * 3.0;
                        let r = radius + wobble;
                        let px = cx + a.cos() * r;
                        let py = cy + a.sin() * r;
                        let sz = 2.5 + (a * 5.0 + now * spd * 3.0).sin().abs() * 1.5;
                        let alpha = pop * (0.6 + 0.4 * (a * 4.0 + now * spd).sin());
                        let c = Color::new(pr, pg, pb, alpha);
                        self.add_rect(&mut portal_verts, px - sz / 2.0, py - sz / 2.0, sz, sz, &c);
                    }
                    // Inner spiral
                    let spiral_segs = 20;
                    for seg in 0..spiral_segs {
                        let t = seg as f32 / spiral_segs as f32;
                        let a = t * std::f32::consts::PI * 4.0 + now * spd * 3.0;
                        let r = radius * (1.0 - t) * 0.8;
                        let px = cx + a.cos() * r;
                        let py = cy + a.sin() * r;
                        let sz = 1.5 * (1.0 - t);
                        let c = Color::new(pr, pg, pb, pop * t * 0.5);
                        self.add_rect(&mut portal_verts, px - sz / 2.0, py - sz / 2.0, sz, sz, &c);
                    }
                    if !portal_verts.is_empty() {
                        let pt_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Cursor Portal Buffer"),
                                contents: bytemuck::cast_slice(&portal_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, pt_buf.slice(..));
                        render_pass.draw(0..portal_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Chevron pattern overlay effect ===
            if self.chevron_pattern_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (cr, cg, cb) = self.chevron_pattern_color;
                let cop = self.chevron_pattern_opacity;
                let spacing = self.chevron_pattern_spacing.max(15.0);
                let spd = self.chevron_pattern_speed;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let mut chev_verts: Vec<RectVertex> = Vec::new();
                let rows = (fh / spacing) as i32 + 2;
                let line_thick = 1.0;
                for row in 0..rows {
                    let base_y = row as f32 * spacing + (now * spd * 20.0) % spacing;
                    let num_v = (fw / spacing) as i32 + 1;
                    for v in 0..num_v {
                        let vx = v as f32 * spacing;
                        // Draw V shape: left leg
                        let segs = 4;
                        for s in 0..segs {
                            let t = s as f32 / segs as f32;
                            let px = vx + t * spacing * 0.5;
                            let py = base_y - t * spacing * 0.5;
                            let shimmer = (0.5 + 0.5 * ((vx * 0.01 + now * spd).sin())).max(0.0);
                            let c = Color::new(cr, cg, cb, cop * shimmer);
                            self.add_rect(&mut chev_verts, px, py, line_thick * 2.0, line_thick, &c);
                        }
                        // Right leg
                        for s in 0..segs {
                            let t = s as f32 / segs as f32;
                            let px = vx + spacing * 0.5 + t * spacing * 0.5;
                            let py = base_y - spacing * 0.5 + t * spacing * 0.5;
                            let shimmer = (0.5 + 0.5 * ((vx * 0.01 + now * spd).sin())).max(0.0);
                            let c = Color::new(cr, cg, cb, cop * shimmer);
                            self.add_rect(&mut chev_verts, px, py, line_thick * 2.0, line_thick, &c);
                        }
                    }
                }
                if !chev_verts.is_empty() {
                    let cv_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Chevron Pattern Buffer"),
                            contents: bytemuck::cast_slice(&chev_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, cv_buf.slice(..));
                    render_pass.draw(0..chev_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor bubble effect ===
            if self.cursor_bubble_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    // Detect cursor move
                    if (cx - self.cursor_bubble_last_x).abs() > 1.0 || (cy - self.cursor_bubble_last_y).abs() > 1.0 {
                        self.cursor_bubble_spawn_time = Some(std::time::Instant::now());
                        self.cursor_bubble_last_x = cx;
                        self.cursor_bubble_last_y = cy;
                    }
                    if let Some(spawn) = self.cursor_bubble_spawn_time {
                        let elapsed = spawn.elapsed().as_secs_f32();
                        let duration = 1.5;
                        if elapsed < duration {
                            let (br, bg, bb) = self.cursor_bubble_color;
                            let bop = self.cursor_bubble_opacity;
                            let count = self.cursor_bubble_count.max(2).min(12);
                            let rspd = self.cursor_bubble_rise_speed;
                            let mut bub_verts: Vec<RectVertex> = Vec::new();
                            for i in 0..count {
                                let mut h = i.wrapping_mul(2654435761);
                                h ^= h >> 16;
                                let offset_x = (h as f32 / u32::MAX as f32 - 0.5) * 20.0;
                                h = h.wrapping_mul(0x45d9f3b);
                                h ^= h >> 16;
                                let offset_delay = (h as f32 / u32::MAX as f32) * 0.3;
                                let t = (elapsed - offset_delay).max(0.0) / duration;
                                if t <= 0.0 || t >= 1.0 { continue; }
                                let rise = t * rspd;
                                let bx = cx + offset_x + (t * 5.0 + i as f32).sin() * 4.0;
                                let by = cy - rise;
                                let sz = (3.0 + i as f32 * 0.5) * (1.0 - t * 0.5);
                                let fade = (1.0 - t) * bop;
                                let c = Color::new(br, bg, bb, fade);
                                // Draw circle approximation with ring of dots
                                let ring = 8;
                                for r in 0..ring {
                                    let a = r as f32 / ring as f32 * std::f32::consts::PI * 2.0;
                                    let px = bx + a.cos() * sz;
                                    let py = by + a.sin() * sz;
                                    self.add_rect(&mut bub_verts, px - 0.75, py - 0.75, 1.5, 1.5, &c);
                                }
                            }
                            if !bub_verts.is_empty() {
                                let bb_buf = self.device.create_buffer_init(
                                    &wgpu::util::BufferInitDescriptor {
                                        label: Some("Cursor Bubble Buffer"),
                                        contents: bytemuck::cast_slice(&bub_verts),
                                        usage: wgpu::BufferUsages::VERTEX,
                                    },
                                );
                                render_pass.set_pipeline(&self.rect_pipeline);
                                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                                render_pass.set_vertex_buffer(0, bb_buf.slice(..));
                                render_pass.draw(0..bub_verts.len() as u32, 0..1);
                            }
                            self.needs_continuous_redraw = true;
                        } else {
                            self.cursor_bubble_spawn_time = None;
                        }
                    }
                }
            }

            // === Sunburst pattern overlay effect ===
            if self.sunburst_pattern_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (cr, cg, cb) = self.sunburst_pattern_color;
                let ray_count = self.sunburst_pattern_ray_count.max(4) as f32;
                let speed = self.sunburst_pattern_speed;
                let opacity = self.sunburst_pattern_opacity;
                let mut sb_verts: Vec<RectVertex> = Vec::new();
                let width = self.width() as f32;
                let height = self.height() as f32;
                let cx = width / 2.0;
                let cy = height / 2.0;
                let max_radius = (cx * cx + cy * cy).sqrt();
                let rotation = now * speed;
                // Draw rays as thin triangular wedges from center
                for i in 0..self.sunburst_pattern_ray_count {
                    let angle = rotation + (i as f32 / ray_count) * std::f32::consts::TAU;
                    let half_width = std::f32::consts::PI / ray_count * 0.4;
                    let a1 = angle - half_width;
                    let a2 = angle + half_width;
                    // Draw ray as series of small rects along the ray direction
                    let steps = 30u32;
                    for s in 0..steps {
                        let t0 = s as f32 / steps as f32;
                        let t1 = (s + 1) as f32 / steps as f32;
                        let r0 = t0 * max_radius;
                        let r1 = t1 * max_radius;
                        let mid_angle = (a1 + a2) / 2.0;
                        let x0 = cx + mid_angle.cos() * r0;
                        let y0 = cy + mid_angle.sin() * r0;
                        let x1 = cx + mid_angle.cos() * r1;
                        let y1 = cy + mid_angle.sin() * r1;
                        let w = (r0 * (a2 - a1)).abs().max(1.0);
                        let mx = (x0 + x1) / 2.0;
                        let my = (y0 + y1) / 2.0;
                        let seg_len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt().max(1.0);
                        let fade = 1.0 - t0;
                        let alpha = opacity * fade;
                        let c = Color::new(cr, cg, cb, alpha);
                        self.add_rect(&mut sb_verts, mx - w / 2.0, my - seg_len / 2.0, w, seg_len, &c);
                    }
                }
                if !sb_verts.is_empty() {
                    let sb_buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("sunburst_pattern_vb"),
                        contents: bytemuck::cast_slice(&sb_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, sb_buf.slice(..));
                    render_pass.draw(0..sb_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor firework effect ===
            if self.cursor_firework_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    // Detect movement
                    let dx = cx - self.cursor_firework_last_x;
                    let dy = cy - self.cursor_firework_last_y;
                    if dx.abs() > 1.0 || dy.abs() > 1.0 {
                        self.cursor_firework_start = Some(std::time::Instant::now());
                        self.cursor_firework_last_x = cx;
                        self.cursor_firework_last_y = cy;
                    }
                    if let Some(start) = self.cursor_firework_start {
                        let elapsed = start.elapsed().as_secs_f32();
                        let duration = 0.6;
                        if elapsed < duration {
                            let t = elapsed / duration;
                            let (cr, cg, cb) = self.cursor_firework_color;
                            let opacity = self.cursor_firework_opacity;
                            let radius = self.cursor_firework_burst_radius;
                            let mut fw_verts: Vec<RectVertex> = Vec::new();
                            for i in 0..self.cursor_firework_particle_count {
                                let angle = (i as f32 / self.cursor_firework_particle_count as f32) * std::f32::consts::TAU;
                                // Add some variation using deterministic hash
                                let mut h = i.wrapping_mul(2654435761);
                                h ^= h >> 16;
                                let speed_var = 0.7 + (h % 60) as f32 / 100.0;
                                let dist = t * radius * speed_var;
                                let px = cx + angle.cos() * dist;
                                let py = cy + angle.sin() * dist;
                                let size = 3.0 * (1.0 - t);
                                let alpha = opacity * (1.0 - t * t);
                                let c = Color::new(cr, cg, cb, alpha);
                                self.add_rect(&mut fw_verts, px - size / 2.0, py - size / 2.0, size, size, &c);
                            }
                            if !fw_verts.is_empty() {
                                let fw_buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("cursor_firework_vb"),
                                    contents: bytemuck::cast_slice(&fw_verts),
                                    usage: wgpu::BufferUsages::VERTEX,
                                });
                                render_pass.set_pipeline(&self.rect_pipeline);
                                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                                render_pass.set_vertex_buffer(0, fw_buf.slice(..));
                                render_pass.draw(0..fw_verts.len() as u32, 0..1);
                            }
                            self.needs_continuous_redraw = true;
                        } else {
                            self.cursor_firework_start = None;
                        }
                    }
                }
            }

            // === Honeycomb dissolve overlay effect ===
            if self.honeycomb_dissolve_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (cr, cg, cb) = self.honeycomb_dissolve_color;
                let cell = self.honeycomb_dissolve_cell_size.max(8.0);
                let speed = self.honeycomb_dissolve_speed;
                let opacity = self.honeycomb_dissolve_opacity;
                let mut hc_verts: Vec<RectVertex> = Vec::new();
                let width = self.width() as f32;
                let height = self.height() as f32;
                // Hex grid layout
                let hex_h = cell * 0.866; // sqrt(3)/2
                let cols = (width / (cell * 1.5)) as u32 + 2;
                let rows = (height / hex_h) as u32 + 2;
                for row in 0..rows {
                    for col in 0..cols {
                        let offset_x = if row % 2 == 1 { cell * 0.75 } else { 0.0 };
                        let hx = col as f32 * cell * 1.5 + offset_x;
                        let hy = row as f32 * hex_h;
                        // Deterministic phase per cell
                        let mut h = ((row * 137 + col) as u64).wrapping_mul(2654435761);
                        h ^= h >> 16;
                        let phase = (h % 1000) as f32 / 1000.0 * std::f32::consts::TAU;
                        let dissolve = ((now * speed + phase).sin() * 0.5 + 0.5);
                        if dissolve > 0.1 {
                            let alpha = opacity * dissolve;
                            let cell_draw = cell * 0.4 * dissolve;
                            let c = Color::new(cr, cg, cb, alpha);
                            self.add_rect(&mut hc_verts, hx - cell_draw / 2.0, hy - cell_draw / 2.0, cell_draw, cell_draw, &c);
                        }
                    }
                }
                if !hc_verts.is_empty() {
                    let hc_buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("honeycomb_dissolve_vb"),
                        contents: bytemuck::cast_slice(&hc_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, hc_buf.slice(..));
                    render_pass.draw(0..hc_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor tornado effect ===
            if self.cursor_tornado_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let (cr, cg, cb) = self.cursor_tornado_color;
                    let radius = self.cursor_tornado_radius;
                    let opacity = self.cursor_tornado_opacity;
                    let mut tn_verts: Vec<RectVertex> = Vec::new();
                    for i in 0..self.cursor_tornado_particle_count {
                        let t = i as f32 / self.cursor_tornado_particle_count as f32;
                        // Particles spiral inward at different heights
                        let angle = now * 3.0 + t * std::f32::consts::TAU * 2.0;
                        let r = radius * (1.0 - t * 0.7);
                        let px = cx + angle.cos() * r;
                        let py = cy + angle.sin() * r - t * radius * 0.5;
                        let size = 3.0 * (1.0 - t * 0.5);
                        let alpha = opacity * (1.0 - t * 0.6);
                        let c = Color::new(cr, cg, cb, alpha);
                        self.add_rect(&mut tn_verts, px - size / 2.0, py - size / 2.0, size, size, &c);
                    }
                    // Add funnel outline rings
                    let ring_count = 5u32;
                    for r_idx in 0..ring_count {
                        let t = r_idx as f32 / ring_count as f32;
                        let ring_r = radius * (1.0 - t * 0.6);
                        let ring_y = cy - t * radius * 0.4;
                        let segments = 24u32;
                        for s in 0..segments {
                            let angle = (s as f32 / segments as f32) * std::f32::consts::TAU + now * 2.0 * (1.0 + t);
                            let px = cx + angle.cos() * ring_r;
                            let py = ring_y + angle.sin() * ring_r * 0.3;
                            let alpha = opacity * 0.4 * (1.0 - t);
                            let c = Color::new(cr, cg, cb, alpha);
                            self.add_rect(&mut tn_verts, px - 1.0, py - 1.0, 2.0, 2.0, &c);
                        }
                    }
                    if !tn_verts.is_empty() {
                        let tn_buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("cursor_tornado_vb"),
                            contents: bytemuck::cast_slice(&tn_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, tn_buf.slice(..));
                        render_pass.draw(0..tn_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Moiré pattern overlay effect ===
            if self.moire_pattern_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (cr, cg, cb) = self.moire_pattern_color;
                let spacing = self.moire_pattern_line_spacing.max(4.0);
                let angle_off = self.moire_pattern_angle_offset * std::f32::consts::PI / 180.0;
                let speed = self.moire_pattern_speed;
                let opacity = self.moire_pattern_opacity;
                let width = self.width() as f32;
                let height = self.height() as f32;
                let mut mo_verts: Vec<RectVertex> = Vec::new();
                let diagonal = (width * width + height * height).sqrt();
                // Two sets of parallel lines at different angles
                for layer in 0..2 {
                    let base_angle = if layer == 0 { now * speed } else { now * speed + angle_off };
                    let cos_a = base_angle.cos();
                    let sin_a = base_angle.sin();
                    let line_count = (diagonal / spacing) as i32 + 2;
                    for i in (-line_count)..line_count {
                        let offset = i as f32 * spacing;
                        // Line perpendicular to angle direction
                        let lx = width / 2.0 + cos_a * offset;
                        let ly = height / 2.0 + sin_a * offset;
                        // Draw as thin rect along the line direction
                        let line_w = 1.0;
                        let line_h = diagonal;
                        // Use sin_a/-cos_a for the perpendicular direction
                        let cx = lx - sin_a * line_h / 2.0;
                        let cy = ly + cos_a * line_h / 2.0;
                        // Approximate with axis-aligned rect
                        let alpha = opacity * 0.5;
                        let c = Color::new(cr, cg, cb, alpha);
                        self.add_rect(&mut mo_verts, lx - line_w / 2.0, ly - line_h / 2.0, line_w, line_h, &c);
                    }
                }
                if !mo_verts.is_empty() {
                    let mo_buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("moire_pattern_vb"),
                        contents: bytemuck::cast_slice(&mo_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, mo_buf.slice(..));
                    render_pass.draw(0..mo_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor lightning effect ===
            if self.cursor_lightning_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let dx = cx - self.cursor_lightning_last_x;
                    let dy = cy - self.cursor_lightning_last_y;
                    if dx.abs() > 1.0 || dy.abs() > 1.0 {
                        self.cursor_lightning_start = Some(std::time::Instant::now());
                        self.cursor_lightning_last_x = cx;
                        self.cursor_lightning_last_y = cy;
                    }
                    if let Some(start) = self.cursor_lightning_start {
                        let elapsed = start.elapsed().as_secs_f32();
                        let duration = 0.4;
                        if elapsed < duration {
                            let t = elapsed / duration;
                            let (cr, cg, cb) = self.cursor_lightning_color;
                            let opacity = self.cursor_lightning_opacity;
                            let max_len = self.cursor_lightning_max_length;
                            let mut lt_verts: Vec<RectVertex> = Vec::new();
                            for bolt in 0..self.cursor_lightning_bolt_count {
                                let mut h = bolt.wrapping_mul(2654435761);
                                h ^= h >> 16;
                                let angle = (h % 360) as f32 * std::f32::consts::PI / 180.0;
                                let segments = 6u32;
                                let mut px = cx;
                                let mut py = cy;
                                for seg in 0..segments {
                                    let seg_len = max_len / segments as f32;
                                    let jitter_angle = angle + ((h.wrapping_mul((seg + 1) as u32) >> 8) % 60) as f32 * 0.02 - 0.6;
                                    let nx = px + jitter_angle.cos() * seg_len;
                                    let ny = py + jitter_angle.sin() * seg_len;
                                    let alpha = opacity * (1.0 - t) * (1.0 - seg as f32 / segments as f32 * 0.5);
                                    let thickness = 2.0 * (1.0 - t * 0.5);
                                    let c = Color::new(cr, cg, cb, alpha);
                                    // Draw segment as small rect
                                    let mx = (px + nx) / 2.0;
                                    let my = (py + ny) / 2.0;
                                    self.add_rect(&mut lt_verts, mx - thickness / 2.0, my - thickness / 2.0, thickness, seg_len.max(thickness), &c);
                                    px = nx;
                                    py = ny;
                                    h = h.wrapping_mul(1103515245).wrapping_add(12345);
                                }
                            }
                            if !lt_verts.is_empty() {
                                let lt_buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("cursor_lightning_vb"),
                                    contents: bytemuck::cast_slice(&lt_verts),
                                    usage: wgpu::BufferUsages::VERTEX,
                                });
                                render_pass.set_pipeline(&self.rect_pipeline);
                                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                                render_pass.set_vertex_buffer(0, lt_buf.slice(..));
                                render_pass.draw(0..lt_verts.len() as u32, 0..1);
                            }
                            self.needs_continuous_redraw = true;
                        } else {
                            self.cursor_lightning_start = None;
                        }
                    }
                }
            }

            // === Dot matrix overlay effect ===
            if self.dot_matrix_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (cr, cg, cb) = self.dot_matrix_color;
                let spacing = self.dot_matrix_spacing.max(4.0);
                let pulse = self.dot_matrix_pulse_speed;
                let opacity = self.dot_matrix_opacity;
                let width = self.width() as f32;
                let height = self.height() as f32;
                let mut dm_verts: Vec<RectVertex> = Vec::new();
                let cols = (width / spacing) as u32 + 1;
                let rows = (height / spacing) as u32 + 1;
                for row in 0..rows {
                    for col in 0..cols {
                        let dx = col as f32 * spacing;
                        let dy = row as f32 * spacing;
                        let phase = (row as f32 * 0.3 + col as f32 * 0.2 + now * pulse).sin() * 0.5 + 0.5;
                        let dot_size = 2.0 * phase + 0.5;
                        let alpha = opacity * phase;
                        let c = Color::new(cr, cg, cb, alpha);
                        self.add_rect(&mut dm_verts, dx - dot_size / 2.0, dy - dot_size / 2.0, dot_size, dot_size, &c);
                    }
                }
                if !dm_verts.is_empty() {
                    let dm_buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("dot_matrix_vb"),
                        contents: bytemuck::cast_slice(&dm_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, dm_buf.slice(..));
                    render_pass.draw(0..dm_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor snowflake effect ===
            if self.cursor_snowflake_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let dx = cx - self.cursor_snowflake_last_x;
                    let dy = cy - self.cursor_snowflake_last_y;
                    if dx.abs() > 1.0 || dy.abs() > 1.0 {
                        self.cursor_snowflake_start = Some(std::time::Instant::now());
                        self.cursor_snowflake_last_x = cx;
                        self.cursor_snowflake_last_y = cy;
                    }
                    if let Some(start) = self.cursor_snowflake_start {
                        let elapsed = start.elapsed().as_secs_f32();
                        let duration = 2.0;
                        if elapsed < duration {
                            let t = elapsed / duration;
                            let (cr, cg, cb) = self.cursor_snowflake_color;
                            let opacity = self.cursor_snowflake_opacity;
                            let fall_speed = self.cursor_snowflake_fall_speed;
                            let mut sf_verts: Vec<RectVertex> = Vec::new();
                            for i in 0..self.cursor_snowflake_count {
                                let mut h = i.wrapping_mul(2654435761);
                                h ^= h >> 16;
                                let offset_x = ((h % 60) as f32 - 30.0);
                                let drift = ((h >> 8) % 20) as f32 * 0.1 - 1.0;
                                let fall_y = elapsed * fall_speed * (0.5 + (h % 100) as f32 / 100.0);
                                let px = cx + offset_x + drift * elapsed * 5.0;
                                let py = cy + fall_y;
                                let size = 3.0 + (h % 3) as f32;
                                let alpha = opacity * (1.0 - t);
                                let c = Color::new(cr, cg, cb, alpha);
                                // Simple snowflake: center + 6 arms as small rects
                                self.add_rect(&mut sf_verts, px - size / 2.0, py - 0.5, size, 1.0, &c);
                                self.add_rect(&mut sf_verts, px - 0.5, py - size / 2.0, 1.0, size, &c);
                                // Diagonal arms approximated
                                let arm = size * 0.35;
                                self.add_rect(&mut sf_verts, px - arm, py - arm, arm * 0.7, arm * 0.7, &c);
                                self.add_rect(&mut sf_verts, px + arm * 0.3, py + arm * 0.3, arm * 0.7, arm * 0.7, &c);
                            }
                            if !sf_verts.is_empty() {
                                let sf_buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("cursor_snowflake_vb"),
                                    contents: bytemuck::cast_slice(&sf_verts),
                                    usage: wgpu::BufferUsages::VERTEX,
                                });
                                render_pass.set_pipeline(&self.rect_pipeline);
                                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                                render_pass.set_vertex_buffer(0, sf_buf.slice(..));
                                render_pass.draw(0..sf_verts.len() as u32, 0..1);
                            }
                            self.needs_continuous_redraw = true;
                        } else {
                            self.cursor_snowflake_start = None;
                        }
                    }
                }
            }

            // === Concentric rings overlay effect ===
            if self.concentric_rings_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (cr, cg, cb) = self.concentric_rings_color;
                let spacing = self.concentric_rings_spacing.max(10.0);
                let speed = self.concentric_rings_expansion_speed;
                let opacity = self.concentric_rings_opacity;
                let width = self.width() as f32;
                let height = self.height() as f32;
                let cx = width / 2.0;
                let cy = height / 2.0;
                let max_radius = (cx * cx + cy * cy).sqrt();
                let mut cr_verts: Vec<RectVertex> = Vec::new();
                let ring_count = (max_radius / spacing) as u32 + 2;
                let phase = now * speed * spacing;
                for i in 0..ring_count {
                    let r = (i as f32 * spacing + phase) % (max_radius + spacing);
                    if r < 1.0 { continue; }
                    let thickness = 1.5;
                    let alpha = opacity * (1.0 - r / max_radius);
                    if alpha < 0.001 { continue; }
                    let c = Color::new(cr, cg, cb, alpha);
                    // Approximate circle with axis-aligned rect segments
                    let segments = (r * 0.5).max(16.0) as u32;
                    for s in 0..segments {
                        let angle = (s as f32 / segments as f32) * std::f32::consts::TAU;
                        let px = cx + angle.cos() * r;
                        let py = cy + angle.sin() * r;
                        self.add_rect(&mut cr_verts, px - thickness / 2.0, py - thickness / 2.0, thickness, thickness, &c);
                    }
                }
                if !cr_verts.is_empty() {
                    let cr_buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("concentric_rings_vb"),
                        contents: bytemuck::cast_slice(&cr_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, cr_buf.slice(..));
                    render_pass.draw(0..cr_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor flame effect ===
            if self.cursor_flame_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y;
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let (cr, cg, cb) = self.cursor_flame_color;
                    let opacity = self.cursor_flame_opacity;
                    let flame_h = self.cursor_flame_height;
                    let mut fl_verts: Vec<RectVertex> = Vec::new();
                    for i in 0..self.cursor_flame_particle_count {
                        let mut h = i.wrapping_mul(2654435761);
                        h ^= h >> 16;
                        let t_offset = (h % 100) as f32 / 100.0;
                        let flicker = ((now * 8.0 + t_offset * std::f32::consts::TAU).sin() * 0.5 + 0.5);
                        let rise = (now * 3.0 + t_offset * 10.0) % 1.0;
                        let px = cx + ((h % 20) as f32 - 10.0) * flicker;
                        let py = cy - rise * flame_h;
                        let size = 4.0 * (1.0 - rise) * flicker;
                        // Warm color gradient: red at base, yellow at tip
                        let rr = cr;
                        let gg = cg + rise * 0.3;
                        let bb = cb * (1.0 - rise);
                        let alpha = opacity * (1.0 - rise);
                        let c = Color::new(rr, gg.min(1.0), bb.max(0.0), alpha);
                        self.add_rect(&mut fl_verts, px - size / 2.0, py - size / 2.0, size, size, &c);
                    }
                    if !fl_verts.is_empty() {
                        let fl_buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("cursor_flame_vb"),
                            contents: bytemuck::cast_slice(&fl_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, fl_buf.slice(..));
                        render_pass.draw(0..fl_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Zigzag pattern overlay effect ===
            if self.zigzag_pattern_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (cr, cg, cb) = self.zigzag_pattern_color;
                let amplitude = self.zigzag_pattern_amplitude;
                let freq = self.zigzag_pattern_frequency;
                let speed = self.zigzag_pattern_speed;
                let opacity = self.zigzag_pattern_opacity;
                let width = self.width() as f32;
                let height = self.height() as f32;
                let mut zz_verts: Vec<RectVertex> = Vec::new();
                let line_spacing = 30.0f32;
                let line_count = (height / line_spacing) as u32 + 1;
                for line in 0..line_count {
                    let base_y = line as f32 * line_spacing;
                    let segments = (width / 4.0) as u32;
                    for s in 0..segments {
                        let x = s as f32 * 4.0;
                        let phase = now * speed + line as f32 * 0.5;
                        let zigzag = ((x * freq + phase).rem_euclid(2.0) - 1.0).abs() * 2.0 - 1.0;
                        let y = base_y + zigzag * amplitude;
                        let c = Color::new(cr, cg, cb, opacity);
                        self.add_rect(&mut zz_verts, x, y, 4.0, 1.0, &c);
                    }
                }
                if !zz_verts.is_empty() {
                    let zz_buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("zigzag_pattern_vb"),
                        contents: bytemuck::cast_slice(&zz_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, zz_buf.slice(..));
                    render_pass.draw(0..zz_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor crystal effect ===
            if self.cursor_crystal_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let (cr, cg, cb) = self.cursor_crystal_color;
                    let opacity = self.cursor_crystal_opacity;
                    let radius = self.cursor_crystal_radius;
                    let facets = self.cursor_crystal_facet_count.max(3);
                    let mut ct_verts: Vec<RectVertex> = Vec::new();
                    // Draw faceted crystal shape
                    for i in 0..facets {
                        let angle = (i as f32 / facets as f32) * std::f32::consts::TAU + now * 0.5;
                        let next_angle = ((i + 1) as f32 / facets as f32) * std::f32::consts::TAU + now * 0.5;
                        // Draw edge line from vertex to next vertex
                        let x1 = cx + angle.cos() * radius;
                        let y1 = cy + angle.sin() * radius;
                        let x2 = cx + next_angle.cos() * radius;
                        let y2 = cy + next_angle.sin() * radius;
                        let segments = 8u32;
                        for s in 0..segments {
                            let t = s as f32 / segments as f32;
                            let px = x1 + (x2 - x1) * t;
                            let py = y1 + (y2 - y1) * t;
                            let c = Color::new(cr, cg, cb, opacity);
                            self.add_rect(&mut ct_verts, px - 1.0, py - 1.0, 2.0, 2.0, &c);
                        }
                        // Draw line from center to vertex (inner facet)
                        for s in 0..6u32 {
                            let t = s as f32 / 6.0;
                            let px = cx + (x1 - cx) * t;
                            let py = cy + (y1 - cy) * t;
                            let alpha = opacity * 0.4 * (1.0 - t);
                            let c = Color::new(cr, cg, cb, alpha);
                            self.add_rect(&mut ct_verts, px - 0.5, py - 0.5, 1.0, 1.0, &c);
                        }
                    }
                    if !ct_verts.is_empty() {
                        let ct_buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("cursor_crystal_vb"),
                            contents: bytemuck::cast_slice(&ct_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, ct_buf.slice(..));
                        render_pass.draw(0..ct_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Basket weave overlay effect ===
            if self.basket_weave_enabled {
                let width = self.width() as f32;
                let height = self.height() as f32;
                let (wr, wg, wb) = self.basket_weave_color;
                let sw = self.basket_weave_strip_width;
                let spacing = self.basket_weave_strip_spacing;
                let alpha = self.basket_weave_opacity;
                let mut overlay_verts = Vec::new();

                // Horizontal strips
                let h_strips = (height / spacing) as i32 + 1;
                for i in 0..h_strips {
                    let y = i as f32 * spacing;
                    let block = (i % 2) as f32;
                    let cols = (width / spacing) as i32 + 1;
                    for j in 0..cols {
                        let x = j as f32 * spacing + block * spacing / 2.0;
                        let c = Color::new(wr, wg, wb, alpha);
                        self.add_rect(&mut overlay_verts, x, y, spacing / 2.0, sw, &c);
                    }
                }

                // Vertical strips
                let v_strips = (width / spacing) as i32 + 1;
                for j in 0..v_strips {
                    let x = j as f32 * spacing;
                    let block = (j % 2) as f32;
                    let rows = (height / spacing) as i32 + 1;
                    for i in 0..rows {
                        let y = i as f32 * spacing + block * spacing / 2.0;
                        let c = Color::new(wr, wg, wb, alpha * 0.7);
                        self.add_rect(&mut overlay_verts, x, y, sw, spacing / 2.0, &c);
                    }
                }

                if !overlay_verts.is_empty() {
                    let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("basket_weave_verts"),
                        contents: bytemuck::cast_slice(&overlay_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, buf.slice(..));
                    render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                }
            }

            // === Cursor sparkler effect ===
            if self.cursor_sparkler_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let (sr, sg, sb) = self.cursor_sparkler_color;
                    let spark_count = self.cursor_sparkler_spark_count;
                    let burn = self.cursor_sparkler_burn_speed;
                    let alpha = self.cursor_sparkler_opacity;
                    let mut overlay_verts = Vec::new();

                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;

                    for i in 0..spark_count {
                        let angle = (i as f32 / spark_count as f32) * std::f32::consts::TAU + now * burn * 2.0;
                        let life = ((now * burn * 3.0 + i as f32 * 1.7) % 1.0);
                        let r = 5.0 + life * 25.0;
                        let sx = cx + angle.cos() * r;
                        let sy = cy + angle.sin() * r;
                        let fade = (1.0 - life).max(0.0);
                        let size = 2.0 * fade + 0.5;

                        // Color shifts from white-hot to orange to dim
                        let heat = fade;
                        let cr = sr * heat + 1.0 * (1.0 - heat);
                        let cg = sg * heat;
                        let cb = sb * heat * 0.3;
                        let c = Color::new(cr.min(1.0), cg.min(1.0), cb.min(1.0), alpha * fade);
                        self.add_rect(&mut overlay_verts, sx - size / 2.0, sy - size / 2.0, size, size, &c);
                    }

                    if !overlay_verts.is_empty() {
                        let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("cursor_sparkler_verts"),
                            contents: bytemuck::cast_slice(&overlay_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, buf.slice(..));
                        render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                        self.needs_continuous_redraw = true;
                    }
                }
            }

            // === Fish scale overlay effect ===
            if self.fish_scale_enabled {
                let width = self.width() as f32;
                let height = self.height() as f32;
                let (fr, fg, fb) = self.fish_scale_color;
                let size = self.fish_scale_size;
                let offset = self.fish_scale_row_offset;
                let alpha = self.fish_scale_opacity;
                let mut overlay_verts = Vec::new();

                let rows = (height / (size * 0.75)) as i32 + 2;
                let cols = (width / size) as i32 + 2;

                for row in 0..rows {
                    let y = row as f32 * size * 0.75;
                    let x_off = if row % 2 == 1 { size * offset } else { 0.0 };

                    for col in 0..cols {
                        let cx = col as f32 * size + x_off;
                        let cy = y;

                        // Draw semicircle as series of small rects (arc approximation)
                        let segments = 10;
                        for s in 0..segments {
                            let angle = std::f32::consts::PI * s as f32 / segments as f32;
                            let ax = cx + angle.cos() * size / 2.0;
                            let ay = cy - angle.sin() * size / 2.0;
                            let c = Color::new(fr, fg, fb, alpha);
                            self.add_rect(&mut overlay_verts, ax - 0.5, ay - 0.5, 1.0, 1.0, &c);
                        }
                    }
                }

                if !overlay_verts.is_empty() {
                    let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("fish_scale_verts"),
                        contents: bytemuck::cast_slice(&overlay_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, buf.slice(..));
                    render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                }
            }

            // === Cursor plasma ball effect ===
            if self.cursor_plasma_ball_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let (pr, pg, pb) = self.cursor_plasma_ball_color;
                    let tendril_count = self.cursor_plasma_ball_tendril_count;
                    let arc_speed = self.cursor_plasma_ball_arc_speed;
                    let alpha = self.cursor_plasma_ball_opacity;
                    let mut overlay_verts = Vec::new();

                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;

                    for t in 0..tendril_count {
                        let base_angle = (t as f32 / tendril_count as f32) * std::f32::consts::TAU;
                        let segments = 15;

                        for s in 0..segments {
                            let frac = s as f32 / segments as f32;
                            let r = 3.0 + frac * 25.0;
                            let wobble = (now * arc_speed * 5.0 + t as f32 * 2.3 + frac * 4.0).sin() * 0.4;
                            let angle = base_angle + now * arc_speed + wobble;
                            let px = cx + angle.cos() * r;
                            let py = cy + angle.sin() * r;
                            let fade = 1.0 - frac * 0.7;
                            let size = 2.5 - frac * 1.0;

                            let c = Color::new(pr, pg, pb, alpha * fade);
                            if size > 0.5 {
                                self.add_rect(&mut overlay_verts, px - size / 2.0, py - size / 2.0, size, size, &c);
                            }
                        }
                    }

                    // Central glow
                    let glow = (now * arc_speed * 3.0).sin() * 0.3 + 0.7;
                    let c = Color::new(pr, pg, pb, alpha * glow * 0.5);
                    self.add_rect(&mut overlay_verts, cx - 4.0, cy - 4.0, 8.0, 8.0, &c);

                    if !overlay_verts.is_empty() {
                        let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("cursor_plasma_ball_verts"),
                            contents: bytemuck::cast_slice(&overlay_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, buf.slice(..));
                        render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                        self.needs_continuous_redraw = true;
                    }
                }
            }

            // === Trefoil knot overlay effect ===
            if self.trefoil_knot_enabled {
                let width = self.width() as f32;
                let height = self.height() as f32;
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (kr, kg, kb) = self.trefoil_knot_color;
                let knot_size = self.trefoil_knot_size;
                let rot_speed = self.trefoil_knot_rotation_speed;
                let alpha = self.trefoil_knot_opacity;
                let mut overlay_verts = Vec::new();

                let cx = width / 2.0;
                let cy = height / 2.0;
                let segments = 120;
                let seg_width = 3.0;

                for i in 0..segments {
                    let t = (i as f32 / segments as f32) * std::f32::consts::TAU;
                    let angle = now * rot_speed;
                    // Trefoil knot parametric equations
                    let x = (t.sin() + 2.0 * (2.0 * t).sin()) * knot_size * 0.5;
                    let y = (t.cos() - 2.0 * (2.0 * t).cos()) * knot_size * 0.5;
                    // Apply rotation
                    let rx = x * angle.cos() - y * angle.sin() + cx;
                    let ry = x * angle.sin() + y * angle.cos() + cy;

                    let phase = (t * 3.0 + now * 2.0).sin() * 0.3 + 0.7;
                    let c = Color::new(kr, kg, kb, alpha * phase);
                    self.add_rect(&mut overlay_verts, rx - seg_width / 2.0, ry - seg_width / 2.0, seg_width, seg_width, &c);
                }

                if !overlay_verts.is_empty() {
                    let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("trefoil_knot_verts"),
                        contents: bytemuck::cast_slice(&overlay_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, buf.slice(..));
                    render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                    self.needs_continuous_redraw = true;
                }
            }

            // === Cursor quill pen effect ===
            if self.cursor_quill_pen_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let (qr, qg, qb) = self.cursor_quill_pen_color;
                    let trail_len = self.cursor_quill_pen_trail_length;
                    let ink_speed = self.cursor_quill_pen_ink_speed;
                    let alpha = self.cursor_quill_pen_opacity;
                    let mut overlay_verts = Vec::new();

                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;

                    // Ink drips falling from cursor
                    for i in 0..trail_len {
                        let phase = now * ink_speed + i as f32 * 0.7;
                        let drip_y = cy + (i as f32 * 8.0) + (phase % 3.0) * 10.0;
                        let drip_x = cx + (phase * 1.3).sin() * 4.0;
                        let fade = 1.0 - (i as f32 / trail_len as f32);
                        let size = 3.0 - i as f32 * 0.2;
                        if size > 0.5 {
                            let c = Color::new(qr, qg, qb, alpha * fade);
                            self.add_rect(&mut overlay_verts, drip_x - size / 2.0, drip_y, size, size * 1.5, &c);
                        }
                    }

                    // Quill flourish curves
                    for f in 0..3 {
                        let angle_off = f as f32 * std::f32::consts::TAU / 3.0 + now * ink_speed * 0.5;
                        for j in 0..8 {
                            let t = j as f32 / 8.0;
                            let r = 10.0 + t * 15.0;
                            let a = angle_off + t * 1.5;
                            let fx = cx + a.cos() * r;
                            let fy = cy - 5.0 + a.sin() * r * 0.5;
                            let fade = 1.0 - t;
                            let c = Color::new(qr, qg, qb, alpha * fade * 0.6);
                            self.add_rect(&mut overlay_verts, fx - 1.0, fy - 1.0, 2.0, 2.0, &c);
                        }
                    }

                    if !overlay_verts.is_empty() {
                        let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("cursor_quill_pen_verts"),
                            contents: bytemuck::cast_slice(&overlay_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, buf.slice(..));
                        render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                        self.needs_continuous_redraw = true;
                    }
                }
            }

            // === Herringbone pattern overlay effect ===
            if self.herringbone_pattern_enabled {
                let width = self.width() as f32;
                let height = self.height() as f32;
                let (hr, hg, hb) = self.herringbone_pattern_color;
                let tw = self.herringbone_pattern_tile_width;
                let th = self.herringbone_pattern_tile_height;
                let alpha = self.herringbone_pattern_opacity;
                let mut overlay_verts = Vec::new();

                let line_w = 1.0;
                let cols = (width / tw) as i32 + 2;
                let rows = (height / th) as i32 + 2;

                for row in 0..rows {
                    for col in 0..cols {
                        let x = col as f32 * tw;
                        let y = row as f32 * th;
                        let even_row = row % 2 == 0;

                        if even_row {
                            // Draw V-shape pointing right
                            // Top diagonal: from (x, y) to (x + tw/2, y + th/2)
                            let steps = (tw / 2.0) as i32;
                            for s in 0..steps {
                                let t = s as f32 / steps as f32;
                                let px = x + t * tw / 2.0;
                                let py = y + t * th / 2.0;
                                let c = Color::new(hr, hg, hb, alpha);
                                self.add_rect(&mut overlay_verts, px, py, line_w, line_w, &c);
                            }
                            // Bottom diagonal: from (x, y + th) to (x + tw/2, y + th/2)
                            for s in 0..steps {
                                let t = s as f32 / steps as f32;
                                let px = x + t * tw / 2.0;
                                let py = y + th - t * th / 2.0;
                                let c = Color::new(hr, hg, hb, alpha);
                                self.add_rect(&mut overlay_verts, px, py, line_w, line_w, &c);
                            }
                        } else {
                            // Offset row: V-shape shifted
                            let offset = tw / 2.0;
                            let steps = (tw / 2.0) as i32;
                            for s in 0..steps {
                                let t = s as f32 / steps as f32;
                                let px = x + offset + t * tw / 2.0;
                                let py = y + t * th / 2.0;
                                let c = Color::new(hr, hg, hb, alpha);
                                self.add_rect(&mut overlay_verts, px, py, line_w, line_w, &c);
                            }
                            for s in 0..steps {
                                let t = s as f32 / steps as f32;
                                let px = x + offset + t * tw / 2.0;
                                let py = y + th - t * th / 2.0;
                                let c = Color::new(hr, hg, hb, alpha);
                                self.add_rect(&mut overlay_verts, px, py, line_w, line_w, &c);
                            }
                        }
                    }
                }

                if !overlay_verts.is_empty() {
                    let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("herringbone_pattern_verts"),
                        contents: bytemuck::cast_slice(&overlay_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, buf.slice(..));
                    render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                }
            }

            // === Cursor aurora borealis effect ===
            if self.cursor_aurora_borealis_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let (ar, ag, ab) = self.cursor_aurora_borealis_color;
                    let band_count = self.cursor_aurora_borealis_band_count;
                    let shimmer = self.cursor_aurora_borealis_shimmer_speed;
                    let alpha = self.cursor_aurora_borealis_opacity;
                    let mut overlay_verts = Vec::new();

                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y;

                    // Shimmering bands above cursor
                    for band in 0..band_count {
                        let band_y = cy - 10.0 - band as f32 * 12.0;
                        let band_width = 40.0 + band as f32 * 15.0;
                        let segments = 20;

                        for s in 0..segments {
                            let t = s as f32 / segments as f32;
                            let x = cx - band_width / 2.0 + t * band_width;
                            let wave = (t * std::f32::consts::TAU * 2.0 + now * shimmer * 3.0 + band as f32 * 0.8).sin();
                            let y = band_y + wave * 4.0;
                            let brightness = (t * std::f32::consts::PI).sin();
                            let hue_shift = (band as f32 * 0.3 + now * shimmer * 0.5).sin() * 0.3;

                            let cr = (ar + hue_shift).clamp(0.0, 1.0);
                            let cg = (ag + hue_shift * 0.5).clamp(0.0, 1.0);
                            let cb = (ab - hue_shift * 0.3).clamp(0.0, 1.0);

                            let h = 3.0 + wave.abs() * 2.0;
                            let fade = 1.0 - (band as f32 / band_count as f32) * 0.5;
                            let c = Color::new(cr, cg, cb, alpha * brightness * fade);
                            self.add_rect(&mut overlay_verts, x, y, band_width / segments as f32, h, &c);
                        }
                    }

                    if !overlay_verts.is_empty() {
                        let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("cursor_aurora_borealis_verts"),
                            contents: bytemuck::cast_slice(&overlay_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, buf.slice(..));
                        render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                        self.needs_continuous_redraw = true;
                    }
                }
            }

            // === Target reticle overlay effect ===
            if self.target_reticle_enabled {
                let width = self.width() as f32;
                let height = self.height() as f32;
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (tr, tg, tb) = self.target_reticle_color;
                let ring_count = self.target_reticle_ring_count;
                let pulse = self.target_reticle_pulse_speed;
                let opacity = self.target_reticle_opacity;
                let cx = width / 2.0;
                let cy = height / 2.0;
                let mut overlay_verts = Vec::new();
                for ring in 0..ring_count {
                    let base_r = 50.0 + ring as f32 * 60.0;
                    let r = base_r + (now * pulse + ring as f32 * 0.5).sin() * 10.0;
                    let segments = 60;
                    for s in 0..segments {
                        let angle = s as f32 * std::f32::consts::TAU / segments as f32;
                        let x = cx + angle.cos() * r;
                        let y = cy + angle.sin() * r;
                        let alpha = opacity * (1.0 - ring as f32 / ring_count as f32 * 0.5);
                        let c = Color::new(tr, tg, tb, alpha);
                        self.add_rect(&mut overlay_verts, x - 1.0, y - 1.0, 2.0, 2.0, &c);
                    }
                }
                // Crosshair lines
                let ch_c = Color::new(tr, tg, tb, opacity * 0.5);
                self.add_rect(&mut overlay_verts, cx - 1.0, 0.0, 2.0, height, &ch_c);
                self.add_rect(&mut overlay_verts, 0.0, cy - 1.0, width, 2.0, &ch_c);
                if !overlay_verts.is_empty() {
                    let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("target_reticle_verts"),
                        contents: bytemuck::cast_slice(&overlay_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, buf.slice(..));
                    render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                    self.needs_continuous_redraw = true;
                }
            }

            // === Cursor feather effect ===
            if self.cursor_feather_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let (fr, fg, fb) = self.cursor_feather_color;
                    let count = self.cursor_feather_count;
                    let drift = self.cursor_feather_drift_speed;
                    let opacity = self.cursor_feather_opacity;
                    let mut overlay_verts = Vec::new();
                    for i in 0..count {
                        let phase = i as f32 * std::f32::consts::TAU / count as f32;
                        let t = (now * drift + phase).rem_euclid(3.0);
                        let drift_x = (now * 0.5 + phase * 2.0).sin() * 15.0;
                        let drift_y = t * 20.0;
                        let fx = cx + drift_x;
                        let fy = cy - drift_y;
                        let alpha = opacity * (1.0 - t / 3.0);
                        let sway = (now * 2.0 + phase).sin() * 0.3;
                        // Feather quill (center line)
                        for s in 0..8 {
                            let st = s as f32 / 8.0;
                            let qx = fx + st * 6.0 * (1.0 + sway);
                            let qy = fy + st * 3.0;
                            let c = Color::new(fr, fg, fb, alpha * (1.0 - st * 0.3));
                            self.add_rect(&mut overlay_verts, qx, qy, 1.0, 1.0, &c);
                        }
                        // Feather barbs (side wisps)
                        for s in 0..5 {
                            let st = s as f32 / 5.0;
                            let bx = fx + st * 5.0;
                            let by_up = fy + st * 2.0 - 2.0;
                            let by_down = fy + st * 2.0 + 2.0;
                            let bc = Color::new(fr, fg, fb, alpha * 0.6);
                            self.add_rect(&mut overlay_verts, bx, by_up, 2.0, 1.0, &bc);
                            self.add_rect(&mut overlay_verts, bx, by_down, 2.0, 1.0, &bc);
                        }
                    }
                    if !overlay_verts.is_empty() {
                        let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("cursor_feather_verts"),
                            contents: bytemuck::cast_slice(&overlay_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, buf.slice(..));
                        render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                        self.needs_continuous_redraw = true;
                    }
                }
            }

            // === Plaid pattern overlay effect ===
            if self.plaid_pattern_enabled {
                let width = self.width() as f32;
                let height = self.height() as f32;
                let (pr, pg, pb) = self.plaid_pattern_color;
                let band_w = self.plaid_pattern_band_width;
                let spacing = self.plaid_pattern_band_spacing;
                let opacity = self.plaid_pattern_opacity;
                let mut overlay_verts = Vec::new();
                // Horizontal bands
                let h_count = (height / spacing) as i32 + 1;
                for i in 0..h_count {
                    let y = i as f32 * spacing;
                    let c = Color::new(pr, pg, pb, opacity);
                    self.add_rect(&mut overlay_verts, 0.0, y, width, band_w, &c);
                }
                // Vertical bands (lighter, for plaid cross effect)
                let v_count = (width / spacing) as i32 + 1;
                for i in 0..v_count {
                    let x = i as f32 * spacing;
                    let c = Color::new(pr * 0.8, pg * 0.8, pb, opacity * 0.7);
                    self.add_rect(&mut overlay_verts, x, 0.0, band_w, height, &c);
                }
                if !overlay_verts.is_empty() {
                    let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("plaid_pattern_verts"),
                        contents: bytemuck::cast_slice(&overlay_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, buf.slice(..));
                    render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                }
            }

            // === Cursor stardust effect ===
            if self.cursor_stardust_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let (sr, sg, sb) = self.cursor_stardust_color;
                    let count = self.cursor_stardust_particle_count;
                    let fall = self.cursor_stardust_fall_speed;
                    let opacity = self.cursor_stardust_opacity;
                    let mut overlay_verts = Vec::new();
                    for i in 0..count {
                        let phase = i as f32 * 2.71828;
                        let t = (now * fall + phase).rem_euclid(2.0);
                        let spread = (phase * 1.618).sin() * 15.0;
                        let x = cx + spread + (now * 0.3 + phase).sin() * 3.0;
                        let y = cy + t * 25.0;
                        let alpha = opacity * (1.0 - t / 2.0);
                        let twinkle = ((now * 5.0 + phase * 3.0).sin() * 0.5 + 0.5);
                        let size = 1.0 + twinkle * 2.0;
                        let c = Color::new(sr, sg * (0.8 + twinkle * 0.2), sb, alpha * twinkle);
                        self.add_rect(&mut overlay_verts, x - size / 2.0, y - size / 2.0, size, size, &c);
                    }
                    if !overlay_verts.is_empty() {
                        let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("cursor_stardust_verts"),
                            contents: bytemuck::cast_slice(&overlay_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, buf.slice(..));
                        render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                        self.needs_continuous_redraw = true;
                    }
                }
            }

            // === Brick wall overlay effect ===
            if self.brick_wall_enabled {
                let width = self.width() as f32;
                let height = self.height() as f32;
                let (br, bg, bb) = self.brick_wall_color;
                let bw = self.brick_wall_width;
                let bh = self.brick_wall_height;
                let opacity = self.brick_wall_opacity;
                let mut overlay_verts = Vec::new();
                let rows = (height / bh) as i32 + 1;
                let cols = (width / bw) as i32 + 2;
                let mortar = 2.0;
                for row in 0..rows {
                    let offset = if row % 2 == 1 { bw / 2.0 } else { 0.0 };
                    let y = row as f32 * bh;
                    // Horizontal mortar line
                    let mc = Color::new(br, bg, bb, opacity);
                    self.add_rect(&mut overlay_verts, 0.0, y, width, mortar, &mc);
                    for col in (-1)..cols {
                        let x = col as f32 * bw + offset;
                        // Vertical mortar line
                        self.add_rect(&mut overlay_verts, x, y, mortar, bh, &mc);
                    }
                }
                if !overlay_verts.is_empty() {
                    let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("brick_wall_verts"),
                        contents: bytemuck::cast_slice(&overlay_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, buf.slice(..));
                    render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                }
            }

            // === Cursor compass needle effect ===
            if self.cursor_compass_needle_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let (nr, ng, nb) = self.cursor_compass_needle_color;
                    let length = self.cursor_compass_needle_length;
                    let spin = self.cursor_compass_needle_spin_speed;
                    let opacity = self.cursor_compass_needle_opacity;
                    let angle = now * spin;
                    let mut overlay_verts = Vec::new();
                    // Needle segments (north half - colored)
                    for s in 0..10 {
                        let t = s as f32 / 10.0;
                        let x = cx + angle.cos() * length * t;
                        let y = cy + angle.sin() * length * t;
                        let w = 3.0 * (1.0 - t);
                        let c = Color::new(nr, ng, nb, opacity * (1.0 - t * 0.5));
                        self.add_rect(&mut overlay_verts, x - w / 2.0, y - w / 2.0, w, w, &c);
                    }
                    // South half - dimmer
                    for s in 0..10 {
                        let t = s as f32 / 10.0;
                        let x = cx - angle.cos() * length * t;
                        let y = cy - angle.sin() * length * t;
                        let w = 3.0 * (1.0 - t);
                        let c = Color::new(nr * 0.3, ng * 0.3, nb * 0.3, opacity * 0.5 * (1.0 - t));
                        self.add_rect(&mut overlay_verts, x - w / 2.0, y - w / 2.0, w, w, &c);
                    }
                    // Center pivot
                    let pc = Color::new(nr, ng, nb, opacity);
                    self.add_rect(&mut overlay_verts, cx - 2.0, cy - 2.0, 4.0, 4.0, &pc);
                    if !overlay_verts.is_empty() {
                        let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("cursor_compass_needle_verts"),
                            contents: bytemuck::cast_slice(&overlay_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, buf.slice(..));
                        render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                        self.needs_continuous_redraw = true;
                    }
                }
            }

            // === Sine wave overlay effect ===
            if self.sine_wave_enabled {
                let width = self.width() as f32;
                let height = self.height() as f32;
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (sr, sg, sb) = self.sine_wave_color;
                let amplitude = self.sine_wave_amplitude;
                let wavelength = self.sine_wave_wavelength;
                let speed = self.sine_wave_speed;
                let opacity = self.sine_wave_opacity;
                let mut overlay_verts = Vec::new();
                let wave_count = (height / 40.0) as i32 + 1;
                for w in 0..wave_count {
                    let base_y = w as f32 * 40.0 + 20.0;
                    let phase = now * speed + w as f32 * 0.5;
                    let steps = (width / 3.0) as i32;
                    for s in 0..steps {
                        let x = s as f32 * 3.0;
                        let y = base_y + (x / wavelength * std::f32::consts::TAU + phase).sin() * amplitude;
                        if y >= 0.0 && y <= height {
                            let c = Color::new(sr, sg, sb, opacity);
                            self.add_rect(&mut overlay_verts, x, y, 2.0, 2.0, &c);
                        }
                    }
                }
                if !overlay_verts.is_empty() {
                    let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("sine_wave_verts"),
                        contents: bytemuck::cast_slice(&overlay_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, buf.slice(..));
                    render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                    self.needs_continuous_redraw = true;
                }
            }

            // === Cursor galaxy effect ===
            if self.cursor_galaxy_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let (gr, gg, gb) = self.cursor_galaxy_color;
                    let star_count = self.cursor_galaxy_star_count;
                    let radius = self.cursor_galaxy_radius;
                    let opacity = self.cursor_galaxy_opacity;
                    let mut overlay_verts = Vec::new();
                    for i in 0..star_count {
                        let phase = i as f32 * std::f32::consts::TAU / star_count as f32;
                        // Spiral arm pattern
                        let arm_angle = now * 0.8 + phase;
                        let dist = (i as f32 / star_count as f32) * radius;
                        let spiral = arm_angle + dist * 0.1;
                        let x = cx + spiral.cos() * dist;
                        let y = cy + spiral.sin() * dist;
                        let brightness = 1.0 - (dist / radius) * 0.5;
                        let twinkle = ((now * 3.0 + phase * 7.0).sin() * 0.5 + 0.5) * 0.4 + 0.6;
                        let size = 1.0 + (1.0 - dist / radius) * 2.0;
                        let c = Color::new(gr * brightness, gg * brightness, gb, opacity * twinkle);
                        self.add_rect(&mut overlay_verts, x - size / 2.0, y - size / 2.0, size, size, &c);
                    }
                    if !overlay_verts.is_empty() {
                        let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("cursor_galaxy_verts"),
                            contents: bytemuck::cast_slice(&overlay_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, buf.slice(..));
                        render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                        self.needs_continuous_redraw = true;
                    }
                }
            }

            // === Rotating gear overlay effect ===
            if self.rotating_gear_enabled {
                let width = self.width() as f32;
                let height = self.height() as f32;
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (gr, gg, gb) = self.rotating_gear_color;
                let gear_size = self.rotating_gear_size;
                let speed = self.rotating_gear_speed;
                let opacity = self.rotating_gear_opacity;
                let cols = (width / (gear_size * 2.5)) as i32 + 1;
                let rows = (height / (gear_size * 2.5)) as i32 + 1;
                let mut overlay_verts = Vec::new();
                for row in 0..rows {
                    for col in 0..cols {
                        let cx = col as f32 * gear_size * 2.5 + gear_size;
                        let cy = row as f32 * gear_size * 2.5 + gear_size;
                        let dir = if (row + col) % 2 == 0 { 1.0 } else { -1.0 };
                        let angle_base = now * speed * dir;
                        let teeth = 8;
                        for t in 0..teeth {
                            let a1 = angle_base + t as f32 * std::f32::consts::TAU / teeth as f32;
                            let a2 = a1 + std::f32::consts::TAU / (teeth as f32 * 2.0);
                            let inner_r = gear_size * 0.6;
                            let outer_r = gear_size;
                            let x1 = cx + a1.cos() * outer_r;
                            let y1 = cy + a1.sin() * outer_r;
                            let x2 = cx + a2.cos() * outer_r;
                            let y2 = cy + a2.sin() * outer_r;
                            let tooth_w = ((x2 - x1).abs()).max(2.0);
                            let tooth_h = ((y2 - y1).abs()).max(2.0);
                            let c = Color::new(gr, gg, gb, opacity);
                            self.add_rect(&mut overlay_verts, x1.min(x2), y1.min(y2), tooth_w, tooth_h, &c);
                            // Inner ring segment
                            let ix1 = cx + a1.cos() * inner_r;
                            let iy1 = cy + a1.sin() * inner_r;
                            let ic = Color::new(gr, gg, gb, opacity * 0.7);
                            self.add_rect(&mut overlay_verts, ix1 - 1.0, iy1 - 1.0, 2.0, 2.0, &ic);
                        }
                    }
                }
                if !overlay_verts.is_empty() {
                    let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("rotating_gear_verts"),
                        contents: bytemuck::cast_slice(&overlay_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, buf.slice(..));
                    render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                    self.needs_continuous_redraw = true;
                }
            }

            // === Cursor prism effect ===
            if self.cursor_prism_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let ray_count = self.cursor_prism_ray_count;
                    let spread = self.cursor_prism_spread;
                    let opacity = self.cursor_prism_opacity;
                    let mut overlay_verts = Vec::new();
                    let rainbow: [(f32, f32, f32); 7] = [
                        (1.0, 0.0, 0.0), (1.0, 0.5, 0.0), (1.0, 1.0, 0.0),
                        (0.0, 1.0, 0.0), (0.0, 0.5, 1.0), (0.3, 0.0, 1.0), (0.5, 0.0, 0.5),
                    ];
                    for i in 0..ray_count {
                        let base_angle = now * 0.3 + i as f32 * std::f32::consts::TAU / ray_count as f32;
                        let (rr, rg, rb) = rainbow[i as usize % 7];
                        for seg in 0..20 {
                            let dist = seg as f32 * spread * 0.5 + 5.0;
                            let x = cx + base_angle.cos() * dist;
                            let y = cy + base_angle.sin() * dist;
                            let alpha = opacity * (1.0 - seg as f32 / 20.0);
                            let size = 2.0 + seg as f32 * 0.3;
                            let c = Color::new(rr, rg, rb, alpha);
                            self.add_rect(&mut overlay_verts, x - size / 2.0, y - size / 2.0, size, size, &c);
                        }
                    }
                    if !overlay_verts.is_empty() {
                        let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("cursor_prism_verts"),
                            contents: bytemuck::cast_slice(&overlay_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, buf.slice(..));
                        render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                        self.needs_continuous_redraw = true;
                    }
                }
            }

            // === Crosshatch pattern overlay effect ===
            if self.crosshatch_pattern_enabled {
                let width = self.width() as f32;
                let height = self.height() as f32;
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (cr, cg, cb) = self.crosshatch_pattern_color;
                let spacing = self.crosshatch_pattern_line_spacing;
                let angle_deg = self.crosshatch_pattern_angle;
                let speed = self.crosshatch_pattern_speed;
                let opacity = self.crosshatch_pattern_opacity;
                let angle_rad = angle_deg * std::f32::consts::PI / 180.0;
                let offset = now * speed * 20.0;
                let mut overlay_verts = Vec::new();
                let diag = (width * width + height * height).sqrt();
                let line_count = (diag / spacing) as i32 + 2;
                // First set of diagonal lines
                for i in (-line_count)..line_count {
                    let base = i as f32 * spacing + offset.rem_euclid(spacing);
                    let dx = angle_rad.cos();
                    let dy = angle_rad.sin();
                    let perp_x = -dy;
                    let perp_y = dx;
                    let center_x = width / 2.0 + perp_x * (base - diag / 2.0);
                    let center_y = height / 2.0 + perp_y * (base - diag / 2.0);
                    let steps = 30;
                    for s in 0..steps {
                        let t = (s as f32 / steps as f32 - 0.5) * diag;
                        let x = center_x + dx * t;
                        let y = center_y + dy * t;
                        if x >= -2.0 && x <= width + 2.0 && y >= -2.0 && y <= height + 2.0 {
                            let c = Color::new(cr, cg, cb, opacity);
                            self.add_rect(&mut overlay_verts, x, y, 1.0, 1.0, &c);
                        }
                    }
                }
                // Second set perpendicular
                let angle_rad2 = angle_rad + std::f32::consts::FRAC_PI_2;
                let dx2 = angle_rad2.cos();
                let dy2 = angle_rad2.sin();
                let perp_x2 = -dy2;
                let perp_y2 = dx2;
                for i in (-line_count)..line_count {
                    let base = i as f32 * spacing - offset.rem_euclid(spacing);
                    let center_x = width / 2.0 + perp_x2 * (base - diag / 2.0);
                    let center_y = height / 2.0 + perp_y2 * (base - diag / 2.0);
                    let steps = 30;
                    for s in 0..steps {
                        let t = (s as f32 / steps as f32 - 0.5) * diag;
                        let x = center_x + dx2 * t;
                        let y = center_y + dy2 * t;
                        if x >= -2.0 && x <= width + 2.0 && y >= -2.0 && y <= height + 2.0 {
                            let c = Color::new(cr, cg, cb, opacity);
                            self.add_rect(&mut overlay_verts, x, y, 1.0, 1.0, &c);
                        }
                    }
                }
                if !overlay_verts.is_empty() {
                    let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("crosshatch_pattern_verts"),
                        contents: bytemuck::cast_slice(&overlay_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, buf.slice(..));
                    render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                    self.needs_continuous_redraw = true;
                }
            }

            // === Cursor moth effect ===
            if self.cursor_moth_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let moth_count = self.cursor_moth_count;
                    let wing_size = self.cursor_moth_wing_size;
                    let (mr, mg, mb) = self.cursor_moth_color;
                    let opacity = self.cursor_moth_opacity;
                    let mut overlay_verts = Vec::new();
                    for i in 0..moth_count {
                        let phase = i as f32 * std::f32::consts::TAU / moth_count as f32;
                        let orbit_r = 15.0 + 10.0 * (now * 0.7 + phase).sin();
                        let orbit_angle = now * 1.5 + phase;
                        let mx = cx + orbit_angle.cos() * orbit_r;
                        let my = cy + orbit_angle.sin() * orbit_r;
                        // Wing flap
                        let flap = (now * 8.0 + phase * 3.0).sin().abs();
                        let wing_w = wing_size * flap;
                        let wing_h = wing_size * 0.6;
                        // Left wing
                        let c = Color::new(mr, mg, mb, opacity * flap);
                        self.add_rect(&mut overlay_verts, mx - wing_w, my - wing_h / 2.0, wing_w, wing_h, &c);
                        // Right wing
                        self.add_rect(&mut overlay_verts, mx, my - wing_h / 2.0, wing_w, wing_h, &c);
                        // Body
                        let bc = Color::new(mr * 0.7, mg * 0.7, mb * 0.7, opacity);
                        self.add_rect(&mut overlay_verts, mx - 1.0, my - wing_h * 0.4, 2.0, wing_h * 0.8, &bc);
                    }
                    if !overlay_verts.is_empty() {
                        let buf = self.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("cursor_moth_verts"),
                            contents: bytemuck::cast_slice(&overlay_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, buf.slice(..));
                        render_pass.draw(0..overlay_verts.len() as u32, 0..1);
                        self.needs_continuous_redraw = true;
                    }
                }
            }

            // === Hex grid overlay effect ===
            if self.hex_grid_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (hr, hg, hb) = self.hex_grid_color;
                let hop = self.hex_grid_opacity;
                let cell = self.hex_grid_cell_size.max(10.0);
                let pspd = self.hex_grid_pulse_speed;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let mut hex_verts: Vec<RectVertex> = Vec::new();
                let hex_h = cell;
                let hex_w = cell * 0.866;
                let cols = (fw / hex_w) as i32 + 2;
                let rows = (fh / (hex_h * 0.75)) as i32 + 2;
                let line_thick = 1.0;
                for row in 0..rows {
                    for col in 0..cols {
                        let cx = col as f32 * hex_w + if row % 2 == 1 { hex_w * 0.5 } else { 0.0 };
                        let cy = row as f32 * hex_h * 0.75;
                        let pulse = (0.6 + 0.4 * ((cx * 0.01 + cy * 0.01 + now * pspd).sin())).max(0.0);
                        let alpha = hop * pulse;
                        let c = Color::new(hr, hg, hb, alpha);
                        // Draw 6 edges of hexagon as small rects
                        for edge in 0..6 {
                            let a1 = edge as f32 / 6.0 * std::f32::consts::PI * 2.0 + std::f32::consts::PI / 6.0;
                            let a2 = (edge + 1) as f32 / 6.0 * std::f32::consts::PI * 2.0 + std::f32::consts::PI / 6.0;
                            let r = cell * 0.5;
                            let x1 = cx + a1.cos() * r;
                            let y1 = cy + a1.sin() * r;
                            let x2 = cx + a2.cos() * r;
                            let y2 = cy + a2.sin() * r;
                            let mx = x1.min(x2);
                            let my = y1.min(y2);
                            let ew = (x2 - x1).abs().max(line_thick);
                            let eh = (y2 - y1).abs().max(line_thick);
                            self.add_rect(&mut hex_verts, mx, my, ew, eh, &c);
                        }
                    }
                }
                if !hex_verts.is_empty() {
                    let hg_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Hex Grid Buffer"),
                            contents: bytemuck::cast_slice(&hex_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, hg_buf.slice(..));
                    render_pass.draw(0..hex_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor sparkle burst effect ===
            if self.cursor_sparkle_burst_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    // Detect cursor movement to spawn burst
                    if !self.cursor_sparkle_burst_entries.is_empty() || true {
                        // Check if cursor moved (using pendulum tracker as proxy)
                        let last = self.cursor_sparkle_burst_entries.last();
                        let should_spawn = match last {
                            Some(e) => (cx - e.cx).abs() > 2.0 || (cy - e.cy).abs() > 2.0,
                            None => true,
                        };
                        if should_spawn {
                            let seed = (cx as u32).wrapping_mul(31).wrapping_add(cy as u32).wrapping_mul(17).wrapping_add(
                                std::time::Instant::now().elapsed().subsec_nanos()
                            );
                            self.cursor_sparkle_burst_entries.push(SparkleBurstEntry {
                                cx, cy,
                                started: std::time::Instant::now(),
                                seed,
                            });
                            if self.cursor_sparkle_burst_entries.len() > 20 {
                                self.cursor_sparkle_burst_entries.remove(0);
                            }
                        }
                    }
                    let (sr, sg, sb) = self.cursor_sparkle_burst_color;
                    let sop = self.cursor_sparkle_burst_opacity;
                    let count = self.cursor_sparkle_burst_count;
                    let radius = self.cursor_sparkle_burst_radius;
                    let mut sparkle_verts: Vec<RectVertex> = Vec::new();
                    let duration = 0.4_f32;
                    self.cursor_sparkle_burst_entries.retain(|e| e.started.elapsed().as_secs_f32() < duration);
                    for entry in &self.cursor_sparkle_burst_entries {
                        let t = entry.started.elapsed().as_secs_f32() / duration;
                        let fade = 1.0 - t;
                        for i in 0..count {
                            let mut h = entry.seed.wrapping_add(i * 2654435761);
                            h ^= h >> 16;
                            h = h.wrapping_mul(0x45d9f3b);
                            let angle = (h as f32 / u32::MAX as f32) * std::f32::consts::PI * 2.0;
                            h = h.wrapping_mul(0x119de1f3);
                            let speed_var = 0.5 + (h as f32 / u32::MAX as f32) * 0.5;
                            let r = t * radius * speed_var;
                            let px = entry.cx + angle.cos() * r;
                            let py = entry.cy + angle.sin() * r;
                            let sz = 2.0 * fade;
                            let c = Color::new(sr, sg, sb, sop * fade * fade);
                            self.add_rect(&mut sparkle_verts, px - sz / 2.0, py - sz / 2.0, sz, sz, &c);
                        }
                    }
                    if !sparkle_verts.is_empty() {
                        let sp_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Sparkle Burst Buffer"),
                                contents: bytemuck::cast_slice(&sparkle_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, sp_buf.slice(..));
                        render_pass.draw(0..sparkle_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Circuit board trace effect ===
            if self.circuit_trace_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (cr, cg, cb) = self.circuit_trace_color;
                let cop = self.circuit_trace_opacity;
                let tw = self.circuit_trace_width;
                let spd = self.circuit_trace_speed;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let mut trace_verts: Vec<RectVertex> = Vec::new();
                // Generate pseudo-random circuit traces along edges
                let trace_count = 8;
                for i in 0..trace_count {
                    let seed = i * 7919 + 13;
                    let side = i % 4;
                    let offset = (seed as f32 * 0.618).fract();
                    let pulse = (0.5 + 0.5 * (now * spd * 2.0 + i as f32 * 1.5).sin()).max(0.0);
                    let alpha = cop * pulse;
                    let c = Color::new(cr, cg, cb, alpha);
                    match side {
                        0 => { // top
                            let x = offset * fw;
                            let len = 30.0 + (seed as f32 * 0.3).fract() * 60.0;
                            self.add_rect(&mut trace_verts, x, 0.0, len.min(fw - x), tw, &c);
                            // Right-angle turn down
                            let turn_len = 15.0 + (seed as f32 * 0.7).fract() * 25.0;
                            self.add_rect(&mut trace_verts, x + len.min(fw - x) - tw, 0.0, tw, turn_len, &c);
                            // Junction dot
                            let dot = tw * 2.0;
                            let dc = Color::new(cr, cg, cb, alpha * 1.5);
                            self.add_rect(&mut trace_verts, x - dot / 2.0, -dot / 4.0, dot, dot, &dc);
                        }
                        1 => { // right
                            let y = offset * fh;
                            let len = 30.0 + (seed as f32 * 0.5).fract() * 60.0;
                            self.add_rect(&mut trace_verts, fw - tw, y, tw, len.min(fh - y), &c);
                            let turn_len = 15.0 + (seed as f32 * 0.9).fract() * 25.0;
                            self.add_rect(&mut trace_verts, fw - turn_len, y, turn_len, tw, &c);
                        }
                        2 => { // bottom
                            let x = offset * fw;
                            let len = 30.0 + (seed as f32 * 0.4).fract() * 60.0;
                            self.add_rect(&mut trace_verts, x, fh - tw, len.min(fw - x), tw, &c);
                            let turn_len = 15.0 + (seed as f32 * 0.6).fract() * 25.0;
                            self.add_rect(&mut trace_verts, x, fh - turn_len, tw, turn_len, &c);
                        }
                        _ => { // left
                            let y = offset * fh;
                            let len = 30.0 + (seed as f32 * 0.8).fract() * 60.0;
                            self.add_rect(&mut trace_verts, 0.0, y, tw, len.min(fh - y), &c);
                            let turn_len = 15.0 + (seed as f32 * 0.2).fract() * 25.0;
                            self.add_rect(&mut trace_verts, 0.0, y + len.min(fh - y) - tw, turn_len, tw, &c);
                        }
                    }
                }
                if !trace_verts.is_empty() {
                    let tr_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Circuit Trace Buffer"),
                            contents: bytemuck::cast_slice(&trace_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, tr_buf.slice(..));
                    render_pass.draw(0..trace_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor compass rose effect ===
            if self.cursor_compass_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let (cr, cg, cb) = self.cursor_compass_color;
                    let cop = self.cursor_compass_opacity;
                    let size = self.cursor_compass_size;
                    let spd = self.cursor_compass_speed;
                    let rotation = now * spd;
                    let mut compass_verts: Vec<RectVertex> = Vec::new();
                    // 4 cardinal lines + 4 intermediate lines
                    for i in 0..8 {
                        let angle = rotation + i as f32 / 8.0 * std::f32::consts::PI * 2.0;
                        let is_cardinal = i % 2 == 0;
                        let len = if is_cardinal { size } else { size * 0.6 };
                        let thick = if is_cardinal { 2.0 } else { 1.0 };
                        let alpha = if is_cardinal { cop } else { cop * 0.6 };
                        let seg_count = 8;
                        for seg in 0..seg_count {
                            let t = seg as f32 / seg_count as f32;
                            let r = t * len;
                            let px = cx + angle.cos() * r;
                            let py = cy + angle.sin() * r;
                            let fade = 1.0 - t * 0.3;
                            let c = Color::new(cr, cg, cb, alpha * fade);
                            self.add_rect(&mut compass_verts, px - thick / 2.0, py - thick / 2.0, thick, thick, &c);
                        }
                        // Arrow tip for cardinal directions
                        if is_cardinal {
                            let tip_r = len * 0.9;
                            let tip_angle1 = angle + 0.15;
                            let tip_angle2 = angle - 0.15;
                            let tx1 = cx + tip_angle1.cos() * tip_r;
                            let ty1 = cy + tip_angle1.sin() * tip_r;
                            let tx2 = cx + tip_angle2.cos() * tip_r;
                            let ty2 = cy + tip_angle2.sin() * tip_r;
                            let tc = Color::new(cr, cg, cb, alpha * 0.8);
                            self.add_rect(&mut compass_verts, tx1 - 1.5, ty1 - 1.5, 3.0, 3.0, &tc);
                            self.add_rect(&mut compass_verts, tx2 - 1.5, ty2 - 1.5, 3.0, 3.0, &tc);
                        }
                    }
                    // Center dot
                    let center_c = Color::new(cr, cg, cb, cop);
                    self.add_rect(&mut compass_verts, cx - 2.0, cy - 2.0, 4.0, 4.0, &center_c);

                    if !compass_verts.is_empty() {
                        let comp_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Compass Rose Buffer"),
                                contents: bytemuck::cast_slice(&compass_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, comp_buf.slice(..));
                        render_pass.draw(0..compass_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Warp/distortion grid effect ===
            if self.warp_grid_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let (wr, wg, wb) = self.warp_grid_color;
                let wop = self.warp_grid_opacity;
                let density = self.warp_grid_density.max(2) as f32;
                let amp = self.warp_grid_amplitude;
                let spd = self.warp_grid_speed;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let cell_w = fw / density;
                let cell_h = fh / density;
                let mut warp_verts: Vec<RectVertex> = Vec::new();
                let line_thick = 1.0;
                // Vertical lines
                for i in 0..=(density as u32) {
                    let base_x = i as f32 * cell_w;
                    for seg in 0..20 {
                        let sy = seg as f32 / 20.0 * fh;
                        let dy = amp * (sy * 0.02 + now * spd * 2.0).sin();
                        let dx = amp * (sy * 0.015 + now * spd * 1.5 + base_x * 0.01).cos();
                        let c = Color::new(wr, wg, wb, wop * (0.5 + 0.5 * (sy * 0.03 + now).sin()));
                        self.add_rect(&mut warp_verts, base_x + dx, sy + dy, line_thick, fh / 20.0, &c);
                    }
                }
                // Horizontal lines
                for j in 0..=(density as u32) {
                    let base_y = j as f32 * cell_h;
                    for seg in 0..20 {
                        let sx = seg as f32 / 20.0 * fw;
                        let dx = amp * (sx * 0.02 + now * spd * 1.8).sin();
                        let dy = amp * (sx * 0.015 + now * spd * 1.3 + base_y * 0.01).cos();
                        let c = Color::new(wr, wg, wb, wop * (0.5 + 0.5 * (sx * 0.03 + now * 0.7).cos()));
                        self.add_rect(&mut warp_verts, sx + dx, base_y + dy, fw / 20.0, line_thick, &c);
                    }
                }
                if !warp_verts.is_empty() {
                    let wg_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Warp Grid Buffer"),
                            contents: bytemuck::cast_slice(&warp_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, wg_buf.slice(..));
                    render_pass.draw(0..warp_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor DNA helix trail effect ===
            if self.cursor_dna_helix_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    let (c1r, c1g, c1b) = self.cursor_dna_helix_color1;
                    let (c2r, c2g, c2b) = self.cursor_dna_helix_color2;
                    let dop = self.cursor_dna_helix_opacity;
                    let radius = self.cursor_dna_helix_radius;
                    let spd = self.cursor_dna_helix_speed;
                    let mut dna_verts: Vec<RectVertex> = Vec::new();
                    let num_nodes = 16;
                    let dot_size = 3.0;
                    for i in 0..num_nodes {
                        let t = i as f32 / num_nodes as f32;
                        let angle = now * spd * 3.0 + t * std::f32::consts::PI * 4.0;
                        let vert_offset = (t - 0.5) * anim.height * 2.0;
                        let fade = 1.0 - t;
                        // Strand 1
                        let x1 = cx + angle.cos() * radius;
                        let y1 = cy + vert_offset;
                        let c1 = Color::new(c1r, c1g, c1b, dop * fade);
                        self.add_rect(&mut dna_verts, x1 - dot_size / 2.0, y1 - dot_size / 2.0, dot_size, dot_size, &c1);
                        // Strand 2 (opposite phase)
                        let x2 = cx - angle.cos() * radius;
                        let y2 = cy + vert_offset;
                        let c2 = Color::new(c2r, c2g, c2b, dop * fade);
                        self.add_rect(&mut dna_verts, x2 - dot_size / 2.0, y2 - dot_size / 2.0, dot_size, dot_size, &c2);
                        // Cross-link every 4th node
                        if i % 4 == 0 {
                            let link_c = Color::new((c1r + c2r) * 0.5, (c1g + c2g) * 0.5, (c1b + c2b) * 0.5, dop * fade * 0.5);
                            let lx = x1.min(x2);
                            let lw = (x2 - x1).abs().max(1.0);
                            self.add_rect(&mut dna_verts, lx, y1 - 0.5, lw, 1.0, &link_c);
                        }
                    }
                    if !dna_verts.is_empty() {
                        let dna_buf = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("DNA Helix Buffer"),
                                contents: bytemuck::cast_slice(&dna_verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, dna_buf.slice(..));
                        render_pass.draw(0..dna_verts.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                }
            }

            // === Prism/rainbow edge effect ===
            if self.prism_edge_enabled {
                let now = std::time::Instant::now().duration_since(self.aurora_start).as_secs_f32();
                let pw = self.prism_edge_width;
                let pop = self.prism_edge_opacity;
                let sat = self.prism_edge_saturation;
                let spd = self.prism_edge_speed;
                let fw = self.width() as f32;
                let fh = self.height() as f32;
                let mut prism_verts: Vec<RectVertex> = Vec::new();
                let num_bands = 30;
                // Helper: HSV to RGB (simplified)
                let hsv_to_rgb = |h: f32, s: f32, v: f32| -> (f32, f32, f32) {
                    let h = h % 1.0;
                    let i = (h * 6.0).floor() as i32;
                    let f = h * 6.0 - i as f32;
                    let p = v * (1.0 - s);
                    let q = v * (1.0 - s * f);
                    let t = v * (1.0 - s * (1.0 - f));
                    match i % 6 {
                        0 => (v, t, p),
                        1 => (q, v, p),
                        2 => (p, v, t),
                        3 => (p, q, v),
                        4 => (t, p, v),
                        _ => (v, p, q),
                    }
                };
                // Top edge
                for i in 0..num_bands {
                    let t = i as f32 / num_bands as f32;
                    let hue = (t + now * spd * 0.2) % 1.0;
                    let (pr, pg, pb) = hsv_to_rgb(hue, sat, 1.0);
                    let x = t * fw;
                    let band_w = fw / num_bands as f32 + 1.0;
                    let c = Color::new(pr, pg, pb, pop);
                    self.add_rect(&mut prism_verts, x, 0.0, band_w, pw, &c);
                }
                // Bottom edge
                for i in 0..num_bands {
                    let t = i as f32 / num_bands as f32;
                    let hue = (t + now * spd * 0.2 + 0.5) % 1.0;
                    let (pr, pg, pb) = hsv_to_rgb(hue, sat, 1.0);
                    let x = t * fw;
                    let band_w = fw / num_bands as f32 + 1.0;
                    let c = Color::new(pr, pg, pb, pop);
                    self.add_rect(&mut prism_verts, x, fh - pw, band_w, pw, &c);
                }
                // Left edge
                for i in 0..num_bands {
                    let t = i as f32 / num_bands as f32;
                    let hue = (t + now * spd * 0.15 + 0.25) % 1.0;
                    let (pr, pg, pb) = hsv_to_rgb(hue, sat, 1.0);
                    let y = t * fh;
                    let band_h = fh / num_bands as f32 + 1.0;
                    let c = Color::new(pr, pg, pb, pop);
                    self.add_rect(&mut prism_verts, 0.0, y, pw, band_h, &c);
                }
                // Right edge
                for i in 0..num_bands {
                    let t = i as f32 / num_bands as f32;
                    let hue = (t + now * spd * 0.15 + 0.75) % 1.0;
                    let (pr, pg, pb) = hsv_to_rgb(hue, sat, 1.0);
                    let y = t * fh;
                    let band_h = fh / num_bands as f32 + 1.0;
                    let c = Color::new(pr, pg, pb, pop);
                    self.add_rect(&mut prism_verts, fw - pw, y, pw, band_h, &c);
                }
                if !prism_verts.is_empty() {
                    let prism_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Prism Edge Buffer"),
                            contents: bytemuck::cast_slice(&prism_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, prism_buf.slice(..));
                    render_pass.draw(0..prism_verts.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Cursor pendulum swing effect ===
            if self.cursor_pendulum_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let cx = anim.x + anim.width / 2.0;
                    let cy = anim.y + anim.height / 2.0;
                    // Detect cursor move
                    if (cx - self.cursor_pendulum_last_x).abs() > 1.0 || (cy - self.cursor_pendulum_last_y).abs() > 1.0 {
                        self.cursor_pendulum_swing_start = Some(std::time::Instant::now());
                        self.cursor_pendulum_last_x = cx;
                        self.cursor_pendulum_last_y = cy;
                    }
                    if let Some(start) = self.cursor_pendulum_swing_start {
                        let elapsed = start.elapsed().as_secs_f32();
                        let (pr, pg, pb) = self.cursor_pendulum_color;
                        let pop = self.cursor_pendulum_opacity;
                        let arc_len = self.cursor_pendulum_arc_length;
                        let damping = self.cursor_pendulum_damping;
                        let decay = (-elapsed * damping * 5.0).exp();
                        if decay > 0.01 {
                            let swing_angle = (elapsed * 8.0).sin() * decay * std::f32::consts::PI * 0.4;
                            let mut pend_verts: Vec<RectVertex> = Vec::new();
                            let num_seg = 12;
                            for seg in 0..num_seg {
                                let t = seg as f32 / num_seg as f32;
                                let r = t * arc_len;
                                let angle = swing_angle + std::f32::consts::PI * 0.5;
                                let px = cx + angle.cos() * r;
                                let py = cy + angle.sin() * r;
                                let fade = (1.0 - t) * decay;
                                let dot = 2.0 + t * 2.0;
                                let c = Color::new(pr, pg, pb, pop * fade);
                                self.add_rect(&mut pend_verts, px - dot / 2.0, py - dot / 2.0, dot, dot, &c);
                            }
                            // Bob at end
                            let bob_r = arc_len;
                            let bob_angle = swing_angle + std::f32::consts::PI * 0.5;
                            let bx = cx + bob_angle.cos() * bob_r;
                            let by = cy + bob_angle.sin() * bob_r;
                            let bob_size = 6.0;
                            let bc = Color::new(pr, pg, pb, pop * decay);
                            self.add_rect(&mut pend_verts, bx - bob_size / 2.0, by - bob_size / 2.0, bob_size, bob_size, &bc);

                            if !pend_verts.is_empty() {
                                let pend_buf = self.device.create_buffer_init(
                                    &wgpu::util::BufferInitDescriptor {
                                        label: Some("Pendulum Buffer"),
                                        contents: bytemuck::cast_slice(&pend_verts),
                                        usage: wgpu::BufferUsages::VERTEX,
                                    },
                                );
                                render_pass.set_pipeline(&self.rect_pipeline);
                                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                                render_pass.set_vertex_buffer(0, pend_buf.slice(..));
                                render_pass.draw(0..pend_verts.len() as u32, 0..1);
                            }
                            self.needs_continuous_redraw = true;
                        } else {
                            self.cursor_pendulum_swing_start = None;
                        }
                    }
                }
            }

            // === Step 2: Draw cursor bg rect (inverse video background) ===
            // Drawn after window/char backgrounds but before text, so the cursor
            // background color is visible behind the inverse-video character.
            // === Cursor drop shadow (drawn before cursor bg) ===
            if self.cursor_shadow_enabled && cursor_visible {
                if let Some(ref anim) = animated_cursor {
                    let sx = anim.x + self.cursor_shadow_offset_x;
                    let sy = anim.y + self.cursor_shadow_offset_y;
                    let shadow_alpha = self.cursor_shadow_opacity.clamp(0.0, 1.0);
                    let shadow_c = Color::new(0.0, 0.0, 0.0, shadow_alpha);
                    let mut shadow_verts: Vec<RectVertex> = Vec::new();
                    self.add_rect(&mut shadow_verts, sx, sy, anim.width, anim.height, &shadow_c);
                    let shadow_buf = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Cursor Shadow Buffer"),
                            contents: bytemuck::cast_slice(&shadow_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, shadow_buf.slice(..));
                    render_pass.draw(0..shadow_verts.len() as u32, 0..1);
                }
            }

            if !cursor_bg_vertices.is_empty() {
                let cursor_bg_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Cursor BG Rect Buffer"),
                            contents: bytemuck::cast_slice(&cursor_bg_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                render_pass.set_pipeline(&self.rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, cursor_bg_buffer.slice(..));
                render_pass.draw(0..cursor_bg_vertices.len() as u32, 0..1);
            }

            // === Step 3: Draw animated cursor trail behind text ===
            // The spring trail or animated rect for filled box cursor appears
            // behind text so characters remain readable during cursor motion.
            if !behind_text_cursor_vertices.is_empty() {
                let trail_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Behind-Text Cursor Buffer"),
                            contents: bytemuck::cast_slice(&behind_text_cursor_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                render_pass.set_pipeline(&self.rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, trail_buffer.slice(..));
                render_pass.draw(0..behind_text_cursor_vertices.len() as u32, 0..1);
            }

            // === Steps 4-6: Draw text and overlay in correct z-order ===
            // For each overlay pass:
            //   Pass 0 (non-overlay): draw buffer text (with cursor fg swap for inverse video)
            //   Pass 1 (overlay): draw overlay backgrounds first, then overlay text
            //
            // This ensures: non-overlay bg → cursor bg → trail → text → overlay bg → overlay text

            for overlay_pass in 0..2 {
                let want_overlay = overlay_pass == 1;

                // === Step 3: Draw overlay backgrounds before overlay text ===
                if want_overlay && !overlay_rect_vertices.is_empty() {
                    let rect_buffer =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Overlay Rect Buffer"),
                                contents: bytemuck::cast_slice(&overlay_rect_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            });

                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, rect_buffer.slice(..));
                    render_pass.draw(0..overlay_rect_vertices.len() as u32, 0..1);
                }

                // Draw filled rounded rect backgrounds for overlay ROUNDED boxed spans.
                if want_overlay {
                    let mut overlay_box_fill: Vec<RoundedRectVertex> = Vec::new();
                    for span in &box_spans {
                        if !span.is_overlay {
                            continue;
                        }
                        if let Some(ref bg_color) = span.bg {
                            if let Some(face) = faces.get(&span.face_id) {
                                if face.box_corner_radius <= 0 { continue; }
                                let radius = (face.box_corner_radius as f32)
                                    .min(span.height * 0.45)
                                    .min(span.width * 0.45);
                                let fill_bw = span.height.max(span.width);
                                self.add_rounded_rect(
                                    &mut overlay_box_fill,
                                    span.x, span.y, span.width, span.height,
                                    fill_bw, radius, bg_color,
                                );
                            }
                        }
                    }
                    if !overlay_box_fill.is_empty() {
                        let fill_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Overlay Box Fill Buffer"),
                                contents: bytemuck::cast_slice(&overlay_box_fill),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rounded_rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, fill_buffer.slice(..));
                        render_pass.draw(0..overlay_box_fill.len() as u32, 0..1);
                    }
                }

                let mut mask_data: Vec<(GlyphKey, [GlyphVertex; 6])> = Vec::new();
                let mut color_data: Vec<(GlyphKey, [GlyphVertex; 6])> = Vec::new();

                for glyph in &frame_glyphs.glyphs {
                    if let FrameGlyph::Char { char, x, y, width, ascent, fg, face_id, font_size, is_overlay, .. } = glyph {
                        if *is_overlay != want_overlay {
                            continue;
                        }

                        let key = GlyphKey {
                            charcode: *char as u32,
                            face_id: *face_id,
                            font_size_bits: font_size.to_bits(),
                        };

                        let face = faces.get(face_id);

                        if let Some(cached) = glyph_atlas.get_or_create(&self.device, &self.queue, &key, face) {
                            // Cached glyphs are rasterized at physical resolution (scale_factor).
                            // Divide bearing/size by scale_factor to get logical pixel positions
                            // that match Emacs coordinate space.
                            let sf = self.scale_factor;
                            let ya = if has_line_anims { *y + self.line_y_offset(*x, *y) } else { *y };
                            let glyph_x = *x + cached.bearing_x / sf;
                            let baseline = ya + *ascent;
                            let glyph_y = baseline - cached.bearing_y / sf;
                            let glyph_w = cached.width as f32 / sf;
                            let glyph_h = cached.height as f32 / sf;

                            // Determine effective foreground color.
                            // For the character under a filled box cursor, swap to
                            // cursor_fg (inverse video) when cursor is visible.
                            let effective_fg = if cursor_visible {
                                if let Some(ref inv) = frame_glyphs.cursor_inverse {
                                    // Match if char cell overlaps cursor inverse position
                                    if (*x - inv.x).abs() < 1.0 && (*y - inv.y).abs() < 1.0 {
                                        &inv.cursor_fg
                                    } else {
                                        fg
                                    }
                                } else {
                                    fg
                                }
                            } else {
                                fg
                            };

                            // Color glyphs use white vertex color (no tinting),
                            // mask glyphs use foreground color for tinting
                            let fade_alpha = self.text_fade_alpha(*x, *y) * self.mode_line_fade_alpha(*x, *y);
                            let color = if cached.is_color {
                                [1.0, 1.0, 1.0, fade_alpha]
                            } else {
                                [effective_fg.r, effective_fg.g, effective_fg.b, effective_fg.a * fade_alpha]
                            };

                            let vertices = [
                                GlyphVertex { position: [glyph_x, glyph_y], tex_coords: [0.0, 0.0], color },
                                GlyphVertex { position: [glyph_x + glyph_w, glyph_y], tex_coords: [1.0, 0.0], color },
                                GlyphVertex { position: [glyph_x + glyph_w, glyph_y + glyph_h], tex_coords: [1.0, 1.0], color },
                                GlyphVertex { position: [glyph_x, glyph_y], tex_coords: [0.0, 0.0], color },
                                GlyphVertex { position: [glyph_x + glyph_w, glyph_y + glyph_h], tex_coords: [1.0, 1.0], color },
                                GlyphVertex { position: [glyph_x, glyph_y + glyph_h], tex_coords: [0.0, 1.0], color },
                            ];

                            if cached.is_color {
                                color_data.push((key, vertices));
                            } else {
                                mask_data.push((key, vertices));
                            }
                        }
                    }
                }

                log::trace!("render_frame_glyphs: overlay={} {} mask glyphs, {} color glyphs",
                    want_overlay, mask_data.len(), color_data.len());

                // Draw mask glyphs with glyph pipeline (alpha tinted with foreground)
                // Sort by GlyphKey so identical characters batch into single draw calls,
                // significantly reducing GPU state changes (set_bind_group calls).
                if !mask_data.is_empty() {
                    mask_data.sort_by(|(a, _), (b, _)| {
                        a.face_id.cmp(&b.face_id)
                            .then(a.font_size_bits.cmp(&b.font_size_bits))
                            .then(a.charcode.cmp(&b.charcode))
                    });

                    render_pass.set_pipeline(&self.glyph_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

                    let all_vertices: Vec<GlyphVertex> = mask_data.iter()
                        .flat_map(|(_, verts)| verts.iter().copied())
                        .collect();

                    let glyph_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Glyph Vertex Buffer"),
                        contents: bytemuck::cast_slice(&all_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });

                    render_pass.set_vertex_buffer(0, glyph_buffer.slice(..));

                    // Batch consecutive glyphs sharing the same texture
                    let mut i = 0;
                    while i < mask_data.len() {
                        let (ref key, _) = mask_data[i];
                        if let Some(cached) = glyph_atlas.get(key) {
                            let batch_start = i;
                            i += 1;
                            while i < mask_data.len() && mask_data[i].0 == *key {
                                i += 1;
                            }
                            let vert_start = (batch_start * 6) as u32;
                            let vert_end = (i * 6) as u32;
                            render_pass.set_bind_group(1, &cached.bind_group, &[]);
                            render_pass.draw(vert_start..vert_end, 0..1);
                        } else {
                            i += 1;
                        }
                    }
                }

                // Draw color glyphs with image pipeline (direct RGBA, e.g. color emoji)
                if !color_data.is_empty() {
                    color_data.sort_by(|(a, _), (b, _)| {
                        a.face_id.cmp(&b.face_id)
                            .then(a.font_size_bits.cmp(&b.font_size_bits))
                            .then(a.charcode.cmp(&b.charcode))
                    });

                    render_pass.set_pipeline(&self.image_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

                    let all_vertices: Vec<GlyphVertex> = color_data.iter()
                        .flat_map(|(_, verts)| verts.iter().copied())
                        .collect();

                    let color_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Color Glyph Vertex Buffer"),
                        contents: bytemuck::cast_slice(&all_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });

                    render_pass.set_vertex_buffer(0, color_buffer.slice(..));

                    // Batch consecutive color glyphs sharing the same texture
                    let mut i = 0;
                    while i < color_data.len() {
                        let (ref key, _) = color_data[i];
                        if let Some(cached) = glyph_atlas.get(key) {
                            let batch_start = i;
                            i += 1;
                            while i < color_data.len() && color_data[i].0 == *key {
                                i += 1;
                            }
                            let vert_start = (batch_start * 6) as u32;
                            let vert_end = (i * 6) as u32;
                            render_pass.set_bind_group(1, &cached.bind_group, &[]);
                            render_pass.draw(vert_start..vert_end, 0..1);
                        } else {
                            i += 1;
                        }
                    }
                }

                // === Draw text decorations (underline, overline, strike-through) ===
                // Rendered after text so decorations appear on top of glyphs.
                // Box borders are handled separately via merged box_spans below.
                {
                    let mut decoration_vertices: Vec<RectVertex> = Vec::new();

                    for glyph in &frame_glyphs.glyphs {
                        if let FrameGlyph::Char {
                            x, y, width, height, ascent, fg,
                            face_id,
                            underline, underline_color,
                            strike_through, strike_through_color,
                            overline, overline_color,
                            is_overlay, ..
                        } = glyph {
                            if *is_overlay != want_overlay {
                                continue;
                            }

                            let ya = if has_line_anims { *y + self.line_y_offset(*x, *y) } else { *y };
                            let baseline_y = ya + *ascent;

                            // Get per-face font metrics for proper decoration positioning
                            let (ul_pos, ul_thick) = frame_glyphs.faces.get(face_id)
                                .map(|f| (f.underline_position as f32, f.underline_thickness as f32))
                                .unwrap_or((1.0, 1.0));

                            // --- Underline ---
                            if *underline > 0 {
                                let ul_color = underline_color.as_ref().unwrap_or(fg);
                                let ul_y = baseline_y + ul_pos;
                                let line_thickness = ul_thick.max(1.0);

                                match underline {
                                    1 => {
                                        // Single solid line
                                        self.add_rect(&mut decoration_vertices, *x, ul_y, *width, line_thickness, ul_color);
                                    }
                                    2 => {
                                        // Wave: smooth sine wave underline
                                        let amplitude: f32 = 2.0;
                                        let wavelength: f32 = 8.0;
                                        let seg_w: f32 = 1.0;
                                        let mut cx = *x;
                                        while cx < *x + *width {
                                            let sw = seg_w.min(*x + *width - cx);
                                            let phase = (cx - *x) * std::f32::consts::TAU / wavelength;
                                            let offset = phase.sin() * amplitude;
                                            self.add_rect(&mut decoration_vertices, cx, ul_y + offset, sw, line_thickness, ul_color);
                                            cx += seg_w;
                                        }
                                    }
                                    3 => {
                                        // Double line
                                        self.add_rect(&mut decoration_vertices, *x, ul_y, *width, line_thickness, ul_color);
                                        self.add_rect(&mut decoration_vertices, *x, ul_y + line_thickness + 1.0, *width, line_thickness, ul_color);
                                    }
                                    4 => {
                                        // Dots (dot size = thickness, gap = 2px)
                                        let mut cx = *x;
                                        while cx < *x + *width {
                                            let dw = line_thickness.min(*x + *width - cx);
                                            self.add_rect(&mut decoration_vertices, cx, ul_y, dw, line_thickness, ul_color);
                                            cx += line_thickness + 2.0;
                                        }
                                    }
                                    5 => {
                                        // Dashes (4px with 3px gap)
                                        let mut cx = *x;
                                        while cx < *x + *width {
                                            let dw = 4.0_f32.min(*x + *width - cx);
                                            self.add_rect(&mut decoration_vertices, cx, ul_y, dw, line_thickness, ul_color);
                                            cx += 7.0;
                                        }
                                    }
                                    _ => {
                                        // Fallback: single line
                                        self.add_rect(&mut decoration_vertices, *x, ul_y, *width, line_thickness, ul_color);
                                    }
                                }
                            }

                            // --- Overline ---
                            if *overline > 0 {
                                let ol_color = overline_color.as_ref().unwrap_or(fg);
                                self.add_rect(&mut decoration_vertices, *x, ya, *width, ul_thick.max(1.0), ol_color);
                            }

                            // --- Strike-through ---
                            if *strike_through > 0 {
                                let st_color = strike_through_color.as_ref().unwrap_or(fg);
                                // Position at ~1/3 of ascent above baseline (standard typographic position)
                                let st_y = baseline_y - *ascent / 3.0;
                                self.add_rect(&mut decoration_vertices, *x, st_y, *width, ul_thick.max(1.0), st_color);
                            }
                        }
                    }

                    if !decoration_vertices.is_empty() {
                        let decoration_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Decoration Rect Buffer"),
                            contents: bytemuck::cast_slice(&decoration_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, decoration_buffer.slice(..));
                        render_pass.draw(0..decoration_vertices.len() as u32, 0..1);
                    }
                }

                // === Draw box borders (merged spans) ===
                // Standard boxes (corner_radius=0): merged rect borders (top/bottom/left/right).
                // Rounded boxes (corner_radius>0): SDF border ring.
                {
                    // Sharp box borders as merged rect spans
                    let mut sharp_border_vertices: Vec<RectVertex> = Vec::new();
                    // Rounded box borders via SDF
                    let mut rounded_border_vertices: Vec<RoundedRectVertex> = Vec::new();

                    // Filter spans for this overlay pass
                    let pass_spans: Vec<usize> = box_spans.iter().enumerate()
                        .filter(|(_, s)| s.is_overlay == want_overlay)
                        .map(|(i, _)| i)
                        .collect();

                    for (idx_in_pass, &span_idx) in pass_spans.iter().enumerate() {
                        let span = &box_spans[span_idx];
                        if let Some(face) = faces.get(&span.face_id) {
                            let bx_color = face.box_color.as_ref().unwrap_or(&face.foreground);
                            let bw = face.box_line_width as f32;

                            if face.box_corner_radius > 0 {
                                // Rounded border via SDF
                                let radius = (face.box_corner_radius as f32)
                                    .min(span.height * 0.45)
                                    .min(span.width * 0.45);
                                self.add_rounded_rect(
                                    &mut rounded_border_vertices,
                                    span.x, span.y, span.width, span.height,
                                    bw, radius, bx_color,
                                );
                            } else {
                                // Sharp border — for overlay spans (mode-line), suppress
                                // internal left/right borders between adjacent spans for
                                // continuity. For non-overlay spans, always draw all 4 borders.
                                let suppress_internal = span.is_overlay;
                                let has_left_neighbor = suppress_internal && idx_in_pass > 0 && {
                                    let prev = &box_spans[pass_spans[idx_in_pass - 1]];
                                    (prev.y - span.y).abs() < 0.5
                                        && ((prev.x + prev.width) - span.x).abs() < 1.5
                                };
                                let has_right_neighbor = suppress_internal && idx_in_pass + 1 < pass_spans.len() && {
                                    let next = &box_spans[pass_spans[idx_in_pass + 1]];
                                    (next.y - span.y).abs() < 0.5
                                        && (next.x - (span.x + span.width)).abs() < 1.5
                                };

                                // Compute edge colors for 3D box types
                                let (top_left_color, bottom_right_color) = match face.box_type {
                                    BoxType::Raised3D => {
                                        let light = Color {
                                            r: (bx_color.r * 1.4).min(1.0),
                                            g: (bx_color.g * 1.4).min(1.0),
                                            b: (bx_color.b * 1.4).min(1.0),
                                            a: bx_color.a,
                                        };
                                        let dark = Color {
                                            r: bx_color.r * 0.6,
                                            g: bx_color.g * 0.6,
                                            b: bx_color.b * 0.6,
                                            a: bx_color.a,
                                        };
                                        (light, dark)
                                    }
                                    BoxType::Sunken3D => {
                                        let light = Color {
                                            r: (bx_color.r * 1.4).min(1.0),
                                            g: (bx_color.g * 1.4).min(1.0),
                                            b: (bx_color.b * 1.4).min(1.0),
                                            a: bx_color.a,
                                        };
                                        let dark = Color {
                                            r: bx_color.r * 0.6,
                                            g: bx_color.g * 0.6,
                                            b: bx_color.b * 0.6,
                                            a: bx_color.a,
                                        };
                                        (dark, light)
                                    }
                                    _ => (bx_color.clone(), bx_color.clone()),
                                };

                                // Top
                                self.add_rect(&mut sharp_border_vertices, span.x, span.y, span.width, bw, &top_left_color);
                                // Bottom
                                self.add_rect(&mut sharp_border_vertices, span.x, span.y + span.height - bw, span.width, bw, &bottom_right_color);
                                // Left (only if no adjacent span to the left on same row)
                                if !has_left_neighbor {
                                    self.add_rect(&mut sharp_border_vertices, span.x, span.y, bw, span.height, &top_left_color);
                                }
                                // Right (only if no adjacent span to the right on same row)
                                if !has_right_neighbor {
                                    self.add_rect(&mut sharp_border_vertices, span.x + span.width - bw, span.y, bw, span.height, &bottom_right_color);
                                }
                            }
                        }
                    }

                    // Draw sharp box borders
                    if !sharp_border_vertices.is_empty() {
                        let sharp_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Sharp Box Border Buffer"),
                                contents: bytemuck::cast_slice(&sharp_border_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, sharp_buffer.slice(..));
                        render_pass.draw(0..sharp_border_vertices.len() as u32, 0..1);
                    }

                    // Draw rounded box borders
                    if !rounded_border_vertices.is_empty() {
                        let rounded_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Rounded Box Border Buffer"),
                                contents: bytemuck::cast_slice(&rounded_border_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rounded_rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, rounded_buffer.slice(..));
                        render_pass.draw(0..rounded_border_vertices.len() as u32, 0..1);
                    }
                }
            }

            // Draw inline images
            render_pass.set_pipeline(&self.image_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Image { image_id, x, y, width, height } = glyph {
                    // Clip to mode-line boundary if needed
                    let (clipped_height, tex_v_max) = if let Some(oy) = overlay_y {
                        if *y + *height > oy {
                            let clipped = (oy - *y).max(0.0);
                            let v_max = if *height > 0.0 { clipped / *height } else { 1.0 };
                            (clipped, v_max)
                        } else {
                            (*height, 1.0)
                        }
                    } else {
                        (*height, 1.0)
                    };

                    // Skip if fully clipped
                    if clipped_height <= 0.0 {
                        continue;
                    }

                    log::debug!("Rendering image {} at ({}, {}) size {}x{} (clipped to {})",
                        image_id, x, y, width, height, clipped_height);
                    // Check if image texture is ready
                    if let Some(cached) = self.image_cache.get(*image_id) {
                        // Create vertices for image quad (white color = no tinting)
                        let vertices = [
                            GlyphVertex { position: [*x, *y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [*x + *width, *y], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [*x + *width, *y + clipped_height], tex_coords: [1.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [*x, *y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [*x + *width, *y + clipped_height], tex_coords: [1.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [*x, *y + clipped_height], tex_coords: [0.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                        ];

                        let image_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Image Vertex Buffer"),
                            contents: bytemuck::cast_slice(&vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                        render_pass.set_bind_group(1, &cached.bind_group, &[]);
                        render_pass.set_vertex_buffer(0, image_buffer.slice(..));
                        render_pass.draw(0..6, 0..1);
                    }
                }
            }

            // Draw inline videos
            #[cfg(feature = "video")]
            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Video { video_id, x, y, width, height } = glyph {
                    // Clip to mode-line boundary if needed
                    let (clipped_height, tex_v_max) = if let Some(oy) = overlay_y {
                        if *y + *height > oy {
                            let clipped = (oy - *y).max(0.0);
                            let v_max = if *height > 0.0 { clipped / *height } else { 1.0 };
                            (clipped, v_max)
                        } else {
                            (*height, 1.0)
                        }
                    } else {
                        (*height, 1.0)
                    };

                    // Skip if fully clipped
                    if clipped_height <= 0.0 {
                        continue;
                    }

                    // Check if video texture is ready
                    if let Some(cached) = self.video_cache.get(*video_id) {
                        log::trace!("Rendering video {} at ({}, {}) size {}x{} (clipped to {}), frame_count={}",
                            video_id, x, y, width, height, clipped_height, cached.frame_count);
                        if let Some(ref bind_group) = cached.bind_group {
                            // Create vertices for video quad (white color = no tinting)
                            let vertices = [
                                GlyphVertex { position: [*x, *y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x + *width, *y], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x + *width, *y + clipped_height], tex_coords: [1.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x, *y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x + *width, *y + clipped_height], tex_coords: [1.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x, *y + clipped_height], tex_coords: [0.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                            ];

                            let video_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Video Vertex Buffer"),
                                contents: bytemuck::cast_slice(&vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            });

                            render_pass.set_bind_group(1, bind_group, &[]);
                            render_pass.set_vertex_buffer(0, video_buffer.slice(..));
                            render_pass.draw(0..6, 0..1);
                        } else {
                            log::warn!("Video {} has no bind_group!", video_id);
                        }
                    } else {
                        log::warn!("Video {} not found in cache!", video_id);
                    }
                }
            }

            // Draw inline webkit views (use opaque pipeline — DMA-BUF XRGB has alpha=0)
            #[cfg(feature = "wpe-webkit")]
            {
                render_pass.set_pipeline(&self.opaque_image_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

                for glyph in &frame_glyphs.glyphs {
                    if let FrameGlyph::WebKit { webkit_id, x, y, width, height } = glyph {
                        // Clip to mode-line boundary if needed
                        log::trace!("WebKit clip check: webkit {} at y={}, height={}, y+h={}, overlay_y={:?}",
                            webkit_id, y, height, y + height, overlay_y);
                        let (clipped_height, tex_v_max) = if let Some(oy) = overlay_y {
                            if *y + *height > oy {
                                let clipped = (oy - *y).max(0.0);
                                let v_max = if *height > 0.0 { clipped / *height } else { 1.0 };
                                log::trace!("WebKit {} clipped: y={} + h={} > overlay_y={}, clipped_height={}",
                                    webkit_id, y, height, oy, clipped);
                                (clipped, v_max)
                            } else {
                                (*height, 1.0)
                            }
                        } else {
                            (*height, 1.0)
                        };

                        // Skip if fully clipped
                        if clipped_height <= 0.0 {
                            continue;
                        }

                        // Check if webkit texture is ready
                        if let Some(cached) = self.webkit_cache.get(*webkit_id) {
                            log::debug!("Rendering webkit {} at ({}, {}) size {}x{} (clipped to {})",
                                webkit_id, x, y, width, height, clipped_height);
                            // Create vertices for webkit quad (white color = no tinting)
                            let vertices = [
                                GlyphVertex { position: [*x, *y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x + *width, *y], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x + *width, *y + clipped_height], tex_coords: [1.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x, *y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x + *width, *y + clipped_height], tex_coords: [1.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x, *y + clipped_height], tex_coords: [0.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                            ];

                            let webkit_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("WebKit Vertex Buffer"),
                                contents: bytemuck::cast_slice(&vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            });

                            render_pass.set_bind_group(1, &cached.bind_group, &[]);
                            render_pass.set_vertex_buffer(0, webkit_buffer.slice(..));
                            render_pass.draw(0..6, 0..1);
                        } else {
                            log::debug!("WebKit {} not found in cache", webkit_id);
                        }
                    }
                }
            }

            // Draw cursors and borders (after text)
            if !cursor_vertices.is_empty() {
                let cursor_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Cursor Vertex Buffer"),
                            contents: bytemuck::cast_slice(&cursor_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                render_pass.set_pipeline(&self.rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, cursor_buffer.slice(..));
                render_pass.draw(0..cursor_vertices.len() as u32, 0..1);
            }

            // === Draw scroll bar thumbs as filled rounded rects ===
            if !scroll_bar_thumb_vertices.is_empty() {
                let mut rounded_verts: Vec<RoundedRectVertex> = Vec::new();
                for (tx, ty, tw, th, radius, color) in &scroll_bar_thumb_vertices {
                    // border_width = 0 triggers filled mode in the shader
                    self.add_rounded_rect(&mut rounded_verts, *tx, *ty, *tw, *th, 0.0, *radius, color);
                }
                let thumb_buffer = self.device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("Scroll Bar Thumb Buffer"),
                        contents: bytemuck::cast_slice(&rounded_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    },
                );
                render_pass.set_pipeline(&self.rounded_rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, thumb_buffer.slice(..));
                render_pass.draw(0..rounded_verts.len() as u32, 0..1);
            }

            // === Draw mode-line separators ===
            if self.mode_line_separator_style > 0 {
                let (cr, cg, cb) = self.mode_line_separator_color;
                let sep_h = self.mode_line_separator_height;
                let style = self.mode_line_separator_style;
                let mut sep_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.mode_line_height > 0.0 && !info.is_minibuffer {
                        let b = &info.bounds;
                        let sep_y = b.y + b.height - info.mode_line_height - sep_h;

                        match style {
                            1 => {
                                // Solid line
                                let c = Color::new(cr, cg, cb, 0.6);
                                self.add_rect(&mut sep_vertices, b.x, sep_y, b.width, 1.0, &c);
                            }
                            2 => {
                                // Shadow (gradient from opaque to transparent)
                                let layers = sep_h.ceil() as i32;
                                for i in 0..layers {
                                    let t = i as f32 / layers as f32;
                                    let alpha = 0.3 * (1.0 - t);
                                    let c = Color::new(cr, cg, cb, alpha);
                                    self.add_rect(&mut sep_vertices, b.x, sep_y + i as f32, b.width, 1.0, &c);
                                }
                            }
                            3 => {
                                // Gradient (solid to transparent, going upward)
                                let layers = sep_h.ceil() as i32;
                                for i in 0..layers {
                                    let t = (layers - 1 - i) as f32 / layers as f32;
                                    let alpha = 0.4 * t;
                                    let c = Color::new(cr, cg, cb, alpha);
                                    self.add_rect(&mut sep_vertices, b.x, sep_y + i as f32, b.width, 1.0, &c);
                                }
                            }
                            _ => {}
                        }
                    }
                }

                if !sep_vertices.is_empty() {
                    let sep_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Mode-line Separator Buffer"),
                            contents: bytemuck::cast_slice(&sep_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, sep_buffer.slice(..));
                    render_pass.draw(0..sep_vertices.len() as u32, 0..1);
                }
            }

            // === Buffer-local accent color strip ===
            if self.accent_strip_enabled {
                let strip_w = self.accent_strip_width.max(1.0);
                let mut strip_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }
                    let b = &info.bounds;

                    // Derive color from file extension hash
                    let ext = info.buffer_file_name.rsplit('.').next().unwrap_or("");
                    let (r, g, b_col) = Self::extension_to_color(ext);
                    let strip_h = b.height - info.mode_line_height;
                    if strip_h <= 0.0 { continue; }

                    let c = Color::new(r, g, b_col, 0.8);
                    self.add_rect(&mut strip_vertices, b.x, b.y, strip_w, strip_h, &c);
                }

                if !strip_vertices.is_empty() {
                    let strip_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Accent Strip Buffer"),
                            contents: bytemuck::cast_slice(&strip_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, strip_buffer.slice(..));
                    render_pass.draw(0..strip_vertices.len() as u32, 0..1);
                }
            }

            // === Window background tint based on file type ===
            if self.window_mode_tint_enabled {
                let tint_alpha = self.window_mode_tint_opacity.clamp(0.0, 1.0);
                let mut tint_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }
                    let b = &info.bounds;
                    let content_h = b.height - info.mode_line_height;
                    if content_h <= 0.0 { continue; }

                    let ext = info.buffer_file_name.rsplit('.').next().unwrap_or("");
                    if ext.is_empty() || ext == info.buffer_file_name { continue; }
                    let (r, g, b_col) = Self::extension_to_color(ext);
                    let c = Color::new(r, g, b_col, tint_alpha);
                    self.add_rect(&mut tint_vertices, b.x, b.y, b.width, content_h, &c);
                }

                if !tint_vertices.is_empty() {
                    let tint_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Mode Tint Buffer"),
                            contents: bytemuck::cast_slice(&tint_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, tint_buffer.slice(..));
                    render_pass.draw(0..tint_vertices.len() as u32, 0..1);
                }
            }

            // === Animated focus ring (marching ants) around selected window ===
            if self.focus_ring_enabled {
                let elapsed = self.focus_ring_start.elapsed().as_secs_f32();
                let offset = (elapsed * self.focus_ring_speed) % (self.focus_ring_dash_length * 2.0);
                let dash = self.focus_ring_dash_length;
                let period = dash * 2.0;
                let thickness = 2.0_f32;
                let (cr, cg, cb) = self.focus_ring_color;
                let alpha = self.focus_ring_opacity.clamp(0.0, 1.0);
                let c = Color::new(cr, cg, cb, alpha);

                let mut ring_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if !info.selected || info.is_minibuffer { continue; }
                    let b = &info.bounds;
                    let content_h = b.height - info.mode_line_height;
                    if content_h <= 0.0 { continue; }

                    // Helper: generate dashes along a line segment
                    let mut add_dashes = |x0: f32, y0: f32, x1: f32, y1: f32, horizontal: bool| {
                        let length = if horizontal { (x1 - x0).abs() } else { (y1 - y0).abs() };
                        let mut pos = -offset;
                        while pos < length {
                            let dash_start = pos.max(0.0);
                            let dash_end = (pos + dash).min(length);
                            if dash_end > dash_start {
                                if horizontal {
                                    self.add_rect(&mut ring_vertices,
                                        x0.min(x1) + dash_start, y0,
                                        dash_end - dash_start, thickness, &c);
                                } else {
                                    self.add_rect(&mut ring_vertices,
                                        x0, y0.min(y1) + dash_start,
                                        thickness, dash_end - dash_start, &c);
                                }
                            }
                            pos += period;
                        }
                    };

                    // Top edge
                    add_dashes(b.x, b.y, b.x + b.width, b.y, true);
                    // Bottom edge (above mode-line)
                    add_dashes(b.x, b.y + content_h - thickness, b.x + b.width, b.y + content_h - thickness, true);
                    // Left edge
                    add_dashes(b.x, b.y, b.x, b.y + content_h, false);
                    // Right edge
                    add_dashes(b.x + b.width - thickness, b.y, b.x + b.width - thickness, b.y + content_h, false);
                }

                if !ring_vertices.is_empty() {
                    self.needs_continuous_redraw = true;
                    let ring_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Focus Ring Buffer"),
                            contents: bytemuck::cast_slice(&ring_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, ring_buffer.slice(..));
                    render_pass.draw(0..ring_vertices.len() as u32, 0..1);
                }
            }

            // === Window padding gradient (inner edge shading for depth) ===
            if self.padding_gradient_enabled {
                let grad_w = self.padding_gradient_width.max(1.0);
                let peak_alpha = self.padding_gradient_opacity.clamp(0.0, 1.0);
                let (cr, cg, cb) = self.padding_gradient_color;
                let steps = (grad_w as i32).max(2);
                let mut grad_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }
                    let b = &info.bounds;
                    let content_h = b.height - info.mode_line_height;
                    if content_h <= 0.0 { continue; }

                    let step_size = grad_w / steps as f32;
                    for i in 0..steps {
                        let t = i as f32 / steps as f32;
                        let alpha = peak_alpha * (1.0 - t) * (1.0 - t);
                        if alpha < 0.005 { continue; }
                        let offset = i as f32 * step_size;
                        let c = Color::new(cr, cg, cb, alpha);

                        // Top edge
                        self.add_rect(&mut grad_vertices, b.x, b.y + offset, b.width, step_size, &c);
                        // Bottom edge (above mode-line)
                        let bot_y = b.y + content_h - offset - step_size;
                        if bot_y > b.y {
                            self.add_rect(&mut grad_vertices, b.x, bot_y, b.width, step_size, &c);
                        }
                        // Left edge
                        self.add_rect(&mut grad_vertices, b.x + offset, b.y, step_size, content_h, &c);
                        // Right edge
                        let right_x = b.x + b.width - offset - step_size;
                        if right_x > b.x {
                            self.add_rect(&mut grad_vertices, right_x, b.y, step_size, content_h, &c);
                        }
                    }
                }

                if !grad_vertices.is_empty() {
                    let grad_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Padding Gradient Buffer"),
                            contents: bytemuck::cast_slice(&grad_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, grad_buffer.slice(..));
                    render_pass.draw(0..grad_vertices.len() as u32, 0..1);
                }
            }

            // === Smooth border color transition on focus ===
            if self.border_transition_enabled && frame_glyphs.window_infos.len() > 1 {
                let now = std::time::Instant::now();
                let (ar, ag, ab) = self.border_transition_active_color;
                let duration = self.border_transition_duration;

                // Detect selection change
                let mut new_selected: Option<i64> = None;
                for info in &frame_glyphs.window_infos {
                    if info.selected && !info.is_minibuffer {
                        new_selected = Some(info.window_id);
                        break;
                    }
                }
                if let Some(sel_id) = new_selected {
                    if self.prev_border_selected != 0 && sel_id != self.prev_border_selected {
                        // Old window: becoming inactive (fade out)
                        self.border_transitions.retain(|&(wid, _, _)| wid != self.prev_border_selected && wid != sel_id);
                        self.border_transitions.push((self.prev_border_selected, false, now));
                        // New window: becoming active (fade in)
                        self.border_transitions.push((sel_id, true, now));
                    }
                    self.prev_border_selected = sel_id;
                }

                // Clean up expired transitions
                self.border_transitions.retain(|&(_, _, start)| now.duration_since(start) < duration);

                let border_thickness = 2.0_f32;
                let mut border_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }
                    let b = &info.bounds;
                    let content_h = b.height - info.mode_line_height;
                    if content_h <= 0.0 { continue; }

                    // Determine alpha: active=1.0, inactive=0.0, transitioning=interpolated
                    let alpha = if let Some(&(_, becoming_active, start)) = self.border_transitions.iter().find(|&&(wid, _, _)| wid == info.window_id) {
                        let t = (now.duration_since(start).as_secs_f32() / duration.as_secs_f32()).min(1.0);
                        let eased = t * (2.0 - t); // ease-out
                        if becoming_active { eased } else { 1.0 - eased }
                    } else if info.selected {
                        1.0_f32
                    } else {
                        0.0_f32
                    };

                    if alpha < 0.01 { continue; }

                    let c = Color::new(ar, ag, ab, alpha * 0.7);
                    // Top border
                    self.add_rect(&mut border_vertices, b.x, b.y, b.width, border_thickness, &c);
                    // Bottom border (above mode-line)
                    self.add_rect(&mut border_vertices, b.x, b.y + content_h - border_thickness, b.width, border_thickness, &c);
                    // Left border
                    self.add_rect(&mut border_vertices, b.x, b.y, border_thickness, content_h, &c);
                    // Right border
                    self.add_rect(&mut border_vertices, b.x + b.width - border_thickness, b.y, border_thickness, content_h, &c);
                }

                if !border_vertices.is_empty() {
                    let border_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Border Transition Buffer"),
                            contents: bytemuck::cast_slice(&border_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, border_buffer.slice(..));
                    render_pass.draw(0..border_vertices.len() as u32, 0..1);
                    self.needs_continuous_redraw = true;
                }
            }

            // === Frosted glass effect on mode-lines ===
            if self.frosted_glass_enabled {
                let frost_opacity = self.frosted_glass_opacity.clamp(0.0, 1.0);
                let blur_r = self.frosted_glass_blur.max(1.0);
                let mut frost_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.mode_line_height <= 0.0 || info.is_minibuffer {
                        continue;
                    }
                    let b = &info.bounds;
                    let ml_y = b.y + b.height - info.mode_line_height;
                    let ml_h = info.mode_line_height;

                    // Layer 1: Base frosted overlay (white, semi-transparent)
                    let base_color = Color::new(1.0, 1.0, 1.0, frost_opacity * 0.15);
                    self.add_rect(&mut frost_vertices, b.x, ml_y, b.width, ml_h, &base_color);

                    // Layer 2-5: Offset blur layers (semi-transparent white at offsets)
                    let offsets = [
                        (-blur_r, 0.0), (blur_r, 0.0),
                        (0.0, -blur_r * 0.5), (0.0, blur_r * 0.5),
                    ];
                    let layer_alpha = frost_opacity * 0.06;
                    for (dx, dy) in offsets {
                        let c = Color::new(1.0, 1.0, 1.0, layer_alpha);
                        // Clamp to mode-line bounds
                        let rx = (b.x + dx).max(b.x);
                        let ry = (ml_y + dy).max(ml_y);
                        let rw = b.width.min(b.x + b.width - rx);
                        let rh = ml_h.min(ml_y + ml_h - ry);
                        if rw > 0.0 && rh > 0.0 {
                            self.add_rect(&mut frost_vertices, rx, ry, rw, rh, &c);
                        }
                    }

                    // Layer 6: Grain/noise pattern (stipple with small rects)
                    let grain_size = 2.0_f32;
                    let grain_alpha = frost_opacity * 0.04;
                    let cols = (b.width / grain_size) as i32;
                    let rows = (ml_h / grain_size) as i32;
                    // Use a simple hash-based pseudo-random for grain pattern
                    for row in 0..rows {
                        for col in 0..cols {
                            // Simple hash: (row * 7919 + col * 104729) % 3 == 0
                            let hash = ((row as u64).wrapping_mul(7919) + (col as u64).wrapping_mul(104729)) % 5;
                            if hash == 0 {
                                let gx = b.x + col as f32 * grain_size;
                                let gy = ml_y + row as f32 * grain_size;
                                let c = Color::new(1.0, 1.0, 1.0, grain_alpha);
                                self.add_rect(&mut frost_vertices, gx, gy, grain_size, grain_size, &c);
                            }
                        }
                    }

                    // Top edge: bright line for glass edge highlight
                    let edge_c = Color::new(1.0, 1.0, 1.0, frost_opacity * 0.3);
                    self.add_rect(&mut frost_vertices, b.x, ml_y, b.width, 1.0, &edge_c);
                }

                if !frost_vertices.is_empty() {
                    let frost_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Frosted Glass Buffer"),
                            contents: bytemuck::cast_slice(&frost_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, frost_buffer.slice(..));
                    render_pass.draw(0..frost_vertices.len() as u32, 0..1);
                }
            }

            // === Noise/film grain texture overlay ===
            if self.noise_grain_enabled {
                let grain_size = self.noise_grain_size.max(1.0);
                let intensity = self.noise_grain_intensity.clamp(0.0, 1.0);
                let frame_w = surface_width as f32 / self.scale_factor;
                let frame_h = surface_height as f32 / self.scale_factor;
                let cols = (frame_w / grain_size) as i32;
                let rows = (frame_h / grain_size) as i32;
                let frame_seed = self.noise_grain_frame as u64;
                self.noise_grain_frame = self.noise_grain_frame.wrapping_add(1);

                let mut grain_vertices: Vec<RectVertex> = Vec::new();
                for row in 0..rows {
                    for col in 0..cols {
                        // Hash-based pseudo-random per grain cell, animated by frame
                        let hash = ((row as u64).wrapping_mul(7919)
                            .wrapping_add((col as u64).wrapping_mul(104729))
                            .wrapping_add(frame_seed.wrapping_mul(31337))) % 97;
                        if hash < 15 {
                            // Alternate black/white grains
                            let lum = if hash < 8 { 0.0 } else { 1.0 };
                            let alpha = intensity * (hash as f32 / 15.0) * 0.5;
                            let gx = col as f32 * grain_size;
                            let gy = row as f32 * grain_size;
                            let c = Color::new(lum, lum, lum, alpha);
                            self.add_rect(&mut grain_vertices, gx, gy, grain_size, grain_size, &c);
                        }
                    }
                }

                if !grain_vertices.is_empty() {
                    let grain_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Noise Grain Buffer"),
                            contents: bytemuck::cast_slice(&grain_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, grain_buffer.slice(..));
                    render_pass.draw(0..grain_vertices.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Idle screen dimming ===
            if self.idle_dim_alpha > 0.001 {
                let frame_w = surface_width as f32 / self.scale_factor;
                let frame_h = surface_height as f32 / self.scale_factor;
                let dim_c = Color::new(0.0, 0.0, 0.0, self.idle_dim_alpha);
                let mut dim_vertices: Vec<RectVertex> = Vec::new();
                self.add_rect(&mut dim_vertices, 0.0, 0.0, frame_w, frame_h, &dim_c);
                let dim_buffer = self.device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("Idle Dim Buffer"),
                        contents: bytemuck::cast_slice(&dim_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    },
                );
                render_pass.set_pipeline(&self.rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, dim_buffer.slice(..));
                render_pass.draw(0..dim_vertices.len() as u32, 0..1);
            }

            // === Focus mode: dim lines outside current paragraph ===
            if self.focus_mode_enabled {
                // Find active cursor Y position (style != 3)
                let mut cursor_y: Option<f32> = None;
                let mut cursor_h: f32 = 0.0;
                if let Some(ref anim) = animated_cursor {
                    cursor_y = Some(anim.y);
                    cursor_h = anim.height;
                } else {
                    for glyph in &frame_glyphs.glyphs {
                        if let FrameGlyph::Cursor { y, height, style, .. } = glyph {
                            if *style != 3 {
                                cursor_y = Some(*y);
                                cursor_h = *height;
                                break;
                            }
                        }
                    }
                }

                if let Some(cy) = cursor_y {
                    // Find selected window bounds
                    let mut sel_bounds: Option<&Rect> = None;
                    for info in &frame_glyphs.window_infos {
                        if info.selected && !info.is_minibuffer {
                            sel_bounds = Some(&info.bounds);
                            break;
                        }
                    }

                    if let Some(bounds) = sel_bounds {
                        let char_h = frame_glyphs.char_height.max(1.0);

                        // Collect unique row Y positions within the selected window
                        let mut row_ys: Vec<(f32, f32, bool)> = Vec::new(); // (y, height, has_non_space)
                        let mut last_y: f32 = -9999.0;
                        let mut last_h: f32 = 0.0;
                        let mut has_non_space = false;

                        for glyph in &frame_glyphs.glyphs {
                            if let FrameGlyph::Char { x, y, height, char: ch, is_overlay, .. } = glyph {
                                if *is_overlay { continue; }
                                if *x < bounds.x || *x >= bounds.x + bounds.width { continue; }
                                if *y < bounds.y || *y >= bounds.y + bounds.height { continue; }
                                if (*y - last_y).abs() > 0.5 {
                                    if last_y > 0.0 {
                                        row_ys.push((last_y, last_h, has_non_space));
                                    }
                                    last_y = *y;
                                    last_h = *height;
                                    has_non_space = false;
                                }
                                if *ch != ' ' && *ch != '\t' && *ch != '\n' {
                                    has_non_space = true;
                                }
                            }
                        }
                        if last_y > 0.0 {
                            row_ys.push((last_y, last_h, has_non_space));
                        }

                        // Find paragraph boundaries: blank lines around cursor
                        let cursor_row_idx = row_ys.iter().position(|(y, _, _)| (*y - cy).abs() < cursor_h);

                        if let Some(cursor_idx) = cursor_row_idx {
                            // Search backward for paragraph start (blank line or window top)
                            let mut para_start_y = bounds.y;
                            for i in (0..cursor_idx).rev() {
                                if !row_ys[i].2 { // blank line
                                    para_start_y = row_ys[i].0 + row_ys[i].1;
                                    break;
                                }
                            }

                            // Search forward for paragraph end (blank line or window bottom)
                            let mut para_end_y = bounds.y + bounds.height;
                            for i in (cursor_idx + 1)..row_ys.len() {
                                if !row_ys[i].2 { // blank line
                                    para_end_y = row_ys[i].0;
                                    break;
                                }
                            }

                            // Draw dim overlays above and below the paragraph
                            let dim_color = Color::new(0.0, 0.0, 0.0, self.focus_mode_opacity);
                            let mut focus_vertices: Vec<RectVertex> = Vec::new();

                            // Above paragraph
                            if para_start_y > bounds.y + 1.0 {
                                self.add_rect(&mut focus_vertices,
                                    bounds.x, bounds.y, bounds.width,
                                    para_start_y - bounds.y, &dim_color);
                            }
                            // Below paragraph
                            if para_end_y < bounds.y + bounds.height - 1.0 {
                                self.add_rect(&mut focus_vertices,
                                    bounds.x, para_end_y, bounds.width,
                                    bounds.y + bounds.height - para_end_y, &dim_color);
                            }

                            if !focus_vertices.is_empty() {
                                let focus_buffer = self.device.create_buffer_init(
                                    &wgpu::util::BufferInitDescriptor {
                                        label: Some("Focus Mode Buffer"),
                                        contents: bytemuck::cast_slice(&focus_vertices),
                                        usage: wgpu::BufferUsages::VERTEX,
                                    },
                                );
                                render_pass.set_pipeline(&self.rect_pipeline);
                                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                                render_pass.set_vertex_buffer(0, focus_buffer.slice(..));
                                render_pass.draw(0..focus_vertices.len() as u32, 0..1);
                            }
                        }
                    }
                }
            }

            // === Draw inactive window dimming overlays (with smooth fade) ===
            if self.inactive_dim_enabled && frame_glyphs.window_infos.len() > 1 {
                let now = std::time::Instant::now();
                let dt = now.duration_since(self.last_dim_tick).as_secs_f32().min(0.1);
                self.last_dim_tick = now;
                // Exponential interpolation speed (higher = faster fade)
                let fade_speed = 8.0;

                let mut dim_vertices: Vec<RectVertex> = Vec::new();
                let mut any_transitioning = false;
                for info in &frame_glyphs.window_infos {
                    let target = if info.selected { 0.0 } else { self.inactive_dim_opacity };
                    let current = self.per_window_dim.get(&info.window_id).copied().unwrap_or(target);
                    // Exponential interpolation toward target
                    let new_opacity = current + (target - current) * (1.0 - (-fade_speed * dt).exp());
                    // Snap to target when close enough
                    let new_opacity = if (new_opacity - target).abs() < 0.001 { target } else { new_opacity };
                    self.per_window_dim.insert(info.window_id, new_opacity);
                    if (new_opacity - target).abs() > 0.0005 {
                        any_transitioning = true;
                    }
                    if new_opacity > 0.001 {
                        let dim_color = Color::new(0.0, 0.0, 0.0, new_opacity);
                        let b = &info.bounds;
                        self.add_rect(&mut dim_vertices, b.x, b.y, b.width, b.height, &dim_color);
                    }
                }
                // Clean up windows that no longer exist
                let valid_ids: std::collections::HashSet<i64> = frame_glyphs.window_infos.iter()
                    .map(|i| i.window_id).collect();
                self.per_window_dim.retain(|k, _| valid_ids.contains(k));

                if !dim_vertices.is_empty() {
                    let dim_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Inactive Dim Buffer"),
                            contents: bytemuck::cast_slice(&dim_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, dim_buffer.slice(..));
                    render_pass.draw(0..dim_vertices.len() as u32, 0..1);
                }
                // Signal that we need continuous redraws during transition
                if any_transitioning {
                    self.needs_continuous_redraw = true;
                }
            }

            // === Inactive window color tint ===
            if self.inactive_tint_enabled && frame_glyphs.window_infos.len() > 1 {
                let (tr, tg, tb) = self.inactive_tint_color;
                let opacity = self.inactive_tint_opacity.clamp(0.0, 1.0);
                let mut tint_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.selected { continue; }
                    let b = &info.bounds;
                    let tint_color = Color::new(tr, tg, tb, opacity);
                    self.add_rect(&mut tint_vertices, b.x, b.y, b.width, b.height, &tint_color);
                }

                if !tint_vertices.is_empty() {
                    let tint_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Inactive Tint Buffer"),
                            contents: bytemuck::cast_slice(&tint_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, tint_buffer.slice(..));
                    render_pass.draw(0..tint_vertices.len() as u32, 0..1);
                }
            }

            // === Zen mode: draw margin overlays for centered content ===
            if self.zen_mode_enabled {
                let content_pct = self.zen_mode_content_width_pct.clamp(20.0, 100.0) / 100.0;
                let margin_alpha = self.zen_mode_margin_opacity;
                let dim_color = Color::new(0.0, 0.0, 0.0, margin_alpha);
                let mut zen_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }
                    let b = &info.bounds;
                    let content_h = b.height - info.mode_line_height;
                    if content_h < 10.0 { continue; }

                    let content_w = b.width * content_pct;
                    let margin_w = (b.width - content_w) / 2.0;

                    if margin_w > 2.0 {
                        // Left margin overlay
                        self.add_rect(&mut zen_vertices,
                            b.x, b.y, margin_w, content_h, &dim_color);
                        // Right margin overlay
                        self.add_rect(&mut zen_vertices,
                            b.x + b.width - margin_w, b.y, margin_w, content_h, &dim_color);
                    }

                    // Dim the mode-line slightly
                    if info.mode_line_height > 0.0 {
                        let ml_dim = Color::new(0.0, 0.0, 0.0, margin_alpha * 0.5);
                        self.add_rect(&mut zen_vertices,
                            b.x, b.y + content_h, b.width, info.mode_line_height, &ml_dim);
                    }
                }

                if !zen_vertices.is_empty() {
                    let zen_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Zen Mode Buffer"),
                            contents: bytemuck::cast_slice(&zen_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, zen_buffer.slice(..));
                    render_pass.draw(0..zen_vertices.len() as u32, 0..1);
                }
            }

            // === Cursor trail fade (afterimage ghost) ===
            if self.cursor_trail_fade_enabled && !self.cursor_trail_positions.is_empty() {
                let now = std::time::Instant::now();
                let fade_dur = self.cursor_trail_fade_duration;
                let mut trail_vertices: Vec<RectVertex> = Vec::new();

                // Remove expired positions
                self.cursor_trail_positions.retain(|&(_, _, _, _, t)| {
                    now.duration_since(t) < fade_dur
                });

                for &(tx, ty, tw, th, spawn) in &self.cursor_trail_positions {
                    let elapsed = now.duration_since(spawn).as_secs_f32();
                    let t = (elapsed / fade_dur.as_secs_f32()).min(1.0);
                    let alpha = 0.3 * (1.0 - t) * (1.0 - t);
                    if alpha < 0.005 { continue; }
                    let c = Color::new(0.5, 0.7, 1.0, alpha);
                    self.add_rect(&mut trail_vertices, tx, ty, tw, th, &c);
                }

                if !trail_vertices.is_empty() {
                    let trail_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Cursor Trail Buffer"),
                            contents: bytemuck::cast_slice(&trail_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, trail_buffer.slice(..));
                    render_pass.draw(0..trail_vertices.len() as u32, 0..1);
                    self.needs_continuous_redraw = true;
                }
            }

            // === Search highlight pulse (glow on isearch face glyphs) ===
            if self.search_pulse_enabled && self.search_pulse_face_id > 0 {
                let target_face = self.search_pulse_face_id;
                // Find bounding box of all glyphs with the isearch face
                let mut min_x = f32::MAX;
                let mut min_y = f32::MAX;
                let mut max_x = f32::MIN;
                let mut max_y = f32::MIN;
                let mut found = false;
                let mut match_bg: Option<Color> = None;

                for glyph in &frame_glyphs.glyphs {
                    if let FrameGlyph::Char { x, y, width, height, face_id, bg, .. } = glyph {
                        if *face_id == target_face {
                            min_x = min_x.min(*x);
                            min_y = min_y.min(*y);
                            max_x = max_x.max(*x + *width);
                            max_y = max_y.max(*y + *height);
                            found = true;
                            if match_bg.is_none() {
                                match_bg = bg.clone();
                            }
                        }
                    }
                }

                if found {
                    let elapsed = self.search_pulse_start.elapsed().as_secs_f32();
                    let phase = elapsed * 3.0 * std::f32::consts::PI; // 1.5 Hz
                    let pulse = (phase.sin() + 1.0) / 2.0; // 0..1

                    // Use the match background color or default to a warm highlight
                    let (pr, pg, pb) = match match_bg {
                        Some(c) => (c.r, c.g, c.b),
                        None => (1.0, 0.8, 0.3), // warm yellow
                    };

                    let glow_alpha = 0.15 + 0.2 * pulse;
                    let glow_pad = 4.0 + 3.0 * pulse; // expanding glow
                    let glow_color = Color::new(pr, pg, pb, glow_alpha);

                    let mut search_vertices: Vec<RectVertex> = Vec::new();
                    self.add_rect(&mut search_vertices,
                        min_x - glow_pad, min_y - glow_pad,
                        (max_x - min_x) + glow_pad * 2.0,
                        (max_y - min_y) + glow_pad * 2.0,
                        &glow_color);

                    if !search_vertices.is_empty() {
                        let search_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Search Pulse Buffer"),
                                contents: bytemuck::cast_slice(&search_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, search_buffer.slice(..));
                        render_pass.draw(0..search_vertices.len() as u32, 0..1);
                    }

                    self.needs_continuous_redraw = true;
                }
            }

            // === Selection region glow highlight ===
            if self.region_glow_enabled && self.region_glow_face_id > 0 {
                let target_face = self.region_glow_face_id;
                let glow_radius = self.region_glow_radius.max(1.0);
                let glow_opacity = self.region_glow_opacity.clamp(0.0, 1.0);

                // Collect per-row bounding boxes for region glyphs
                let mut row_bounds: Vec<(f32, f32, f32, f32)> = Vec::new(); // (x, y, w, h)
                let mut current_row_y: f32 = -9999.0;
                let mut row_min_x: f32 = f32::MAX;
                let mut row_max_x: f32 = f32::MIN;
                let mut row_h: f32 = 0.0;

                for glyph in &frame_glyphs.glyphs {
                    if let FrameGlyph::Char { x, y, width, height, face_id, .. } = glyph {
                        if *face_id == target_face {
                            if (*y - current_row_y).abs() > 1.0 {
                                // New row; flush previous
                                if row_min_x < row_max_x {
                                    row_bounds.push((row_min_x, current_row_y, row_max_x - row_min_x, row_h));
                                }
                                current_row_y = *y;
                                row_min_x = *x;
                                row_max_x = *x + *width;
                                row_h = *height;
                            } else {
                                row_min_x = row_min_x.min(*x);
                                row_max_x = row_max_x.max(*x + *width);
                                row_h = row_h.max(*height);
                            }
                        }
                    }
                }
                // Flush last row
                if row_min_x < row_max_x {
                    row_bounds.push((row_min_x, current_row_y, row_max_x - row_min_x, row_h));
                }

                if !row_bounds.is_empty() {
                    // Get region background color from face
                    let region_color = faces.get(&target_face)
                        .map(|f| (f.background.r, f.background.g, f.background.b))
                        .unwrap_or((0.4, 0.6, 1.0));

                    let mut glow_vertices: Vec<RectVertex> = Vec::new();
                    let steps = (glow_radius as i32).max(2);

                    for (rx, ry, rw, rh) in &row_bounds {
                        for i in 1..=steps {
                            let t = i as f32 / steps as f32;
                            let alpha = glow_opacity * (1.0 - t) * (1.0 - t);
                            if alpha < 0.005 { continue; }
                            let pad = t * glow_radius;
                            let c = Color::new(region_color.0, region_color.1, region_color.2, alpha);
                            self.add_rect(&mut glow_vertices,
                                rx - pad, ry - pad,
                                rw + pad * 2.0, rh + pad * 2.0,
                                &c);
                        }
                    }

                    if !glow_vertices.is_empty() {
                        let glow_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Region Glow Buffer"),
                                contents: bytemuck::cast_slice(&glow_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, glow_buffer.slice(..));
                        render_pass.draw(0..glow_vertices.len() as u32, 0..1);
                    }
                }
            }

            // === Typing ripple effect ===
            if self.typing_ripple_enabled && !self.active_ripples.is_empty() {
                let now = std::time::Instant::now();
                let duration = self.typing_ripple_duration;
                let max_r = self.typing_ripple_max_radius;

                // Remove expired ripples
                self.active_ripples.retain(|&(_, _, t)| now.duration_since(t).as_secs_f32() < duration);

                if !self.active_ripples.is_empty() {
                    let mut ripple_vertices: Vec<RectVertex> = Vec::new();
                    let segments = 32;

                    for &(cx, cy, spawn_t) in &self.active_ripples {
                        let elapsed = now.duration_since(spawn_t).as_secs_f32();
                        let t = (elapsed / duration).min(1.0);

                        // Eased expansion
                        let ease_t = 1.0 - (1.0 - t) * (1.0 - t); // ease-out quadratic
                        let radius = max_r * ease_t;
                        // Fade out
                        let alpha = 0.4 * (1.0 - t);
                        let ring_thickness = 1.5;

                        let color = Color::new(0.5, 0.7, 1.0, alpha);

                        // Draw ring as small rectangles at each angle
                        for i in 0..segments {
                            let angle = (i as f32 / segments as f32) * 2.0 * std::f32::consts::PI;
                            let px = cx + radius * angle.cos();
                            let py = cy + radius * angle.sin();
                            self.add_rect(&mut ripple_vertices,
                                px - ring_thickness / 2.0,
                                py - ring_thickness / 2.0,
                                ring_thickness,
                                ring_thickness,
                                &color);
                        }
                    }

                    if !ripple_vertices.is_empty() {
                        let ripple_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Ripple Buffer"),
                                contents: bytemuck::cast_slice(&ripple_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, ripple_buffer.slice(..));
                        render_pass.draw(0..ripple_vertices.len() as u32, 0..1);
                    }

                    self.needs_continuous_redraw = true;
                }
            }

            // === Minimap: code overview column on right side of each window ===
            if self.minimap_enabled {
                let minimap_w = self.minimap_width;
                let char_w = frame_glyphs.char_width.max(1.0);
                let char_h = frame_glyphs.char_height.max(1.0);
                // Scale factor: each source char maps to this many pixels in minimap
                let scale_x = 2.0_f32;
                let scale_y = 1.5_f32;

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }

                    let b = &info.bounds;
                    // Content area (excluding mode-line)
                    let content_h = b.height - info.mode_line_height;
                    if content_h < 10.0 { continue; }

                    // Minimap positioned at right edge of window content
                    let map_x = b.x + b.width - minimap_w;
                    let map_y = b.y;
                    let map_h = content_h;

                    // Semi-transparent background
                    let bg_color = Color::new(0.0, 0.0, 0.0, 0.15);
                    let mut minimap_vertices: Vec<RectVertex> = Vec::new();
                    self.add_rect(&mut minimap_vertices, map_x, map_y, minimap_w, map_h, &bg_color);

                    // Collect glyphs belonging to this window's content area
                    // and render each as a tiny colored rectangle
                    for glyph in &frame_glyphs.glyphs {
                        if let FrameGlyph::Char { x, y, width, height, fg, char: ch, is_overlay, .. } = glyph {
                            if *is_overlay { continue; }
                            if *ch == ' ' || *ch == '\t' || *ch == '\n' { continue; }
                            // Check glyph is in this window's content area
                            if *x < b.x || *x >= b.x + b.width - minimap_w { continue; }
                            if *y < b.y || *y >= b.y + content_h { continue; }

                            // Map glyph position to minimap coordinates
                            let rel_x = (*x - b.x) / char_w;
                            let rel_y = (*y - b.y) / char_h;
                            let mini_x = map_x + 2.0 + rel_x * scale_x;
                            let mini_y = map_y + rel_y * scale_y;

                            // Skip if outside minimap bounds
                            if mini_x >= map_x + minimap_w - 1.0 { continue; }
                            if mini_y >= map_y + map_h - 1.0 { continue; }

                            // Draw tiny colored block for each character
                            let dot_w = ((*width / char_w) * scale_x).max(1.0).min(scale_x * 2.0);
                            let dot_h = scale_y;
                            let dot_color = Color::new(fg.r, fg.g, fg.b, 0.7);
                            self.add_rect(&mut minimap_vertices, mini_x, mini_y, dot_w, dot_h, &dot_color);
                        }
                    }

                    // Viewport indicator: show where the visible portion is relative to full buffer
                    if info.buffer_size > 0 {
                        let start_frac = info.window_start as f32 / info.buffer_size as f32;
                        let end_frac = (info.window_end as f32 / info.buffer_size as f32).min(1.0);
                        let vp_y = map_y + start_frac * map_h;
                        let vp_h = ((end_frac - start_frac) * map_h).max(4.0);
                        let vp_color = Color::new(1.0, 1.0, 1.0, 0.1);
                        self.add_rect(&mut minimap_vertices, map_x, vp_y, minimap_w, vp_h, &vp_color);
                        // Left edge highlight for viewport indicator
                        let edge_color = Color::new(0.5, 0.7, 1.0, 0.4);
                        self.add_rect(&mut minimap_vertices, map_x, vp_y, 2.0, vp_h, &edge_color);
                    }

                    if !minimap_vertices.is_empty() {
                        let minimap_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Minimap Buffer"),
                                contents: bytemuck::cast_slice(&minimap_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, minimap_buffer.slice(..));
                        render_pass.draw(0..minimap_vertices.len() as u32, 0..1);
                    }
                }
            }

            // === Header/mode-line shadow depth effect ===
            if self.header_shadow_enabled {
                let shadow_size = self.header_shadow_size.max(1.0);
                let intensity = self.header_shadow_intensity.clamp(0.0, 1.0);
                let steps = 8;
                let mut shadow_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }
                    let b = &info.bounds;

                    // Shadow below header-line (if present): at the top of content area
                    // Header-line is at the very top of the window, same height as mode-line typically
                    // We detect header-line by checking for overlay glyphs at the window top
                    // For simplicity, we'll check if there are overlay glyphs near the window top
                    let mut header_bottom: Option<f32> = None;
                    for g in &frame_glyphs.glyphs {
                        if let FrameGlyph::Char { x, y, height, is_overlay: true, .. }
                            | FrameGlyph::Stretch { x, y, height, is_overlay: true, .. } = g
                        {
                            let gx = match g {
                                FrameGlyph::Char { x, .. } => *x,
                                FrameGlyph::Stretch { x, .. } => *x,
                                _ => continue,
                            };
                            let gy = match g {
                                FrameGlyph::Char { y, .. } => *y,
                                FrameGlyph::Stretch { y, .. } => *y,
                                _ => continue,
                            };
                            let gh = match g {
                                FrameGlyph::Char { height, .. } => *height,
                                FrameGlyph::Stretch { height, .. } => *height,
                                _ => continue,
                            };
                            // Check if this overlay glyph is at the top of this window
                            if gx >= b.x && gx < b.x + b.width
                                && (gy - b.y).abs() < 2.0
                            {
                                let bottom = gy + gh;
                                header_bottom = Some(header_bottom.map_or(bottom, |prev: f32| prev.max(bottom)));
                            }
                        }
                    }

                    // Draw downward shadow below header-line
                    if let Some(hb) = header_bottom {
                        for i in 0..steps {
                            let t = i as f32 / steps as f32;
                            let alpha = intensity * (1.0 - t) * (1.0 - t);
                            let strip_h = shadow_size / steps as f32;
                            let sy = hb + i as f32 * strip_h;
                            let color = Color { r: 0.0, g: 0.0, b: 0.0, a: alpha };
                            self.add_rect(&mut shadow_vertices, b.x, sy, b.width, strip_h, &color);
                        }
                    }

                    // Draw upward shadow above mode-line
                    if info.mode_line_height > 0.0 {
                        let ml_top = b.y + b.height - info.mode_line_height;
                        for i in 0..steps {
                            let t = i as f32 / steps as f32;
                            let alpha = intensity * (1.0 - t) * (1.0 - t);
                            let strip_h = shadow_size / steps as f32;
                            let sy = ml_top - (i as f32 + 1.0) * strip_h;
                            let color = Color { r: 0.0, g: 0.0, b: 0.0, a: alpha };
                            self.add_rect(&mut shadow_vertices, b.x, sy, b.width, strip_h, &color);
                        }
                    }
                }

                if !shadow_vertices.is_empty() {
                    let shadow_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Header Shadow Buffer"),
                        contents: bytemuck::cast_slice(&shadow_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, shadow_buffer.slice(..));
                    render_pass.draw(0..shadow_vertices.len() as u32, 0..1);
                }
            }

            // === Active window border glow ===
            if self.window_glow_enabled {
                let glow_radius = self.window_glow_radius.max(1.0);
                let intensity = self.window_glow_intensity.clamp(0.0, 1.0);
                let (cr, cg, cb) = self.window_glow_color;
                let steps = 10;
                let mut glow_vertices: Vec<RectVertex> = Vec::new();

                // Find the selected window
                for info in &frame_glyphs.window_infos {
                    if !info.selected || info.is_minibuffer { continue; }
                    let b = &info.bounds;

                    for i in 0..steps {
                        let t = (i + 1) as f32 / steps as f32;
                        // Quadratic falloff: brightest at edge, fading outward
                        let alpha = intensity * (1.0 - t) * (1.0 - t);
                        let offset = t * glow_radius;
                        let strip_w = glow_radius / steps as f32;
                        let color = Color::new(cr, cg, cb, alpha);

                        // Top edge (outside window, above)
                        self.add_rect(&mut glow_vertices,
                            b.x - offset, b.y - offset,
                            b.width + offset * 2.0, strip_w, &color);
                        // Bottom edge (outside window, below)
                        self.add_rect(&mut glow_vertices,
                            b.x - offset, b.y + b.height + offset - strip_w,
                            b.width + offset * 2.0, strip_w, &color);
                        // Left edge (outside window)
                        self.add_rect(&mut glow_vertices,
                            b.x - offset, b.y - offset,
                            strip_w, b.height + offset * 2.0, &color);
                        // Right edge (outside window)
                        self.add_rect(&mut glow_vertices,
                            b.x + b.width + offset - strip_w, b.y - offset,
                            strip_w, b.height + offset * 2.0, &color);
                    }
                }

                if !glow_vertices.is_empty() {
                    let glow_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Window Glow Buffer"),
                        contents: bytemuck::cast_slice(&glow_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, glow_buffer.slice(..));
                    render_pass.draw(0..glow_vertices.len() as u32, 0..1);
                }
            }

            // === Scroll progress indicator bar ===
            if self.scroll_progress_enabled {
                let bar_h = self.scroll_progress_height.max(1.0);
                let (cr, cg, cb) = self.scroll_progress_color;
                let opacity = self.scroll_progress_opacity.clamp(0.0, 1.0);
                let mut progress_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }
                    let b = &info.bounds;
                    let buf_size = info.buffer_size.max(1) as f32;

                    // Compute scroll fraction (0.0 = top, 1.0 = bottom)
                    let frac = (info.window_start as f32 / buf_size).clamp(0.0, 1.0);

                    // Compute visible fraction (how much of buffer is visible)
                    let visible = ((info.window_end - info.window_start).max(1) as f32 / buf_size)
                        .clamp(0.0, 1.0);

                    // Track background (subtle)
                    let track_color = Color::new(cr, cg, cb, opacity * 0.15);
                    self.add_rect(&mut progress_vertices, b.x, b.y, b.width, bar_h, &track_color);

                    // Progress thumb
                    let thumb_w = (visible * b.width).max(4.0);
                    let thumb_x = b.x + frac * (b.width - thumb_w);
                    let thumb_color = Color::new(cr, cg, cb, opacity);
                    self.add_rect(&mut progress_vertices, thumb_x, b.y, thumb_w, bar_h, &thumb_color);
                }

                if !progress_vertices.is_empty() {
                    let progress_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Scroll Progress Buffer"),
                        contents: bytemuck::cast_slice(&progress_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, progress_buffer.slice(..));
                    render_pass.draw(0..progress_vertices.len() as u32, 0..1);
                }
            }

            // === Window content shadow/depth effect ===
            if self.window_content_shadow_enabled && frame_glyphs.window_infos.len() > 1 {
                let shadow_size = self.window_content_shadow_size.max(1.0);
                let shadow_opacity = self.window_content_shadow_opacity.clamp(0.0, 1.0);
                let mut shadow_vertices: Vec<RectVertex> = Vec::new();
                let steps = 4;

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }
                    let b = &info.bounds;

                    // Inner shadow at each edge (gradient from dark to transparent)
                    for i in 0..steps {
                        let frac = i as f32 / steps as f32;
                        let alpha = shadow_opacity * (1.0 - frac) * (1.0 - frac);
                        let thickness = shadow_size / steps as f32;
                        let c = Color::new(0.0, 0.0, 0.0, alpha);

                        // Top inner shadow
                        self.add_rect(&mut shadow_vertices,
                            b.x, b.y + frac * shadow_size,
                            b.width, thickness, &c);
                        // Left inner shadow
                        self.add_rect(&mut shadow_vertices,
                            b.x + frac * shadow_size, b.y,
                            thickness, b.height, &c);
                        // Right inner shadow
                        self.add_rect(&mut shadow_vertices,
                            b.x + b.width - shadow_size + frac * shadow_size, b.y,
                            thickness, b.height, &c);
                        // Bottom inner shadow (above mode-line)
                        let content_bottom = b.y + b.height - info.mode_line_height;
                        self.add_rect(&mut shadow_vertices,
                            b.x, content_bottom - shadow_size + frac * shadow_size,
                            b.width, thickness, &c);
                    }
                }

                if !shadow_vertices.is_empty() {
                    let shadow_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Window Content Shadow Buffer"),
                        contents: bytemuck::cast_slice(&shadow_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, shadow_buffer.slice(..));
                    render_pass.draw(0..shadow_vertices.len() as u32, 0..1);
                }
            }

            // === Resize padding transition overlay ===
            {
                let pad = self.resize_padding_amount();
                if pad > 0.5 {
                    let bg = &frame_glyphs.background;
                    // Opaque background-colored rectangles at inner edges of each window
                    let mut pad_vertices: Vec<RectVertex> = Vec::new();
                    for info in &frame_glyphs.window_infos {
                        let b = &info.bounds;
                        // Top edge
                        self.add_rect(&mut pad_vertices, b.x, b.y, b.width, pad, bg);
                        // Bottom edge (above mode-line)
                        let content_bottom = b.y + b.height - info.mode_line_height;
                        self.add_rect(&mut pad_vertices, b.x, content_bottom - pad, b.width, pad, bg);
                        // Left edge
                        self.add_rect(&mut pad_vertices, b.x, b.y, pad, b.height - info.mode_line_height, bg);
                        // Right edge
                        self.add_rect(&mut pad_vertices, b.x + b.width - pad, b.y, pad, b.height - info.mode_line_height, bg);
                    }
                    if !pad_vertices.is_empty() {
                        let pad_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Resize Padding Buffer"),
                            contents: bytemuck::cast_slice(&pad_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, pad_buffer.slice(..));
                        render_pass.draw(0..pad_vertices.len() as u32, 0..1);
                    }
                    self.needs_continuous_redraw = true;
                } else if self.resize_padding_started.is_some() {
                    // Animation complete, clean up
                    self.resize_padding_started = None;
                }
            }

            // === Mini-buffer completion highlight ===
            if self.minibuffer_highlight_enabled {
                let (hr, hg, hb) = self.minibuffer_highlight_color;
                let h_opacity = self.minibuffer_highlight_opacity.clamp(0.0, 1.0);
                let mut highlight_vertices: Vec<RectVertex> = Vec::new();

                // Find minibuffer window
                if let Some(mb_info) = frame_glyphs.window_infos.iter().find(|w| w.is_minibuffer) {
                    let mb = &mb_info.bounds;
                    // Collect rows of glyphs with background color in the minibuffer
                    // Group by Y coordinate to find highlighted rows
                    let mut highlighted_rows: Vec<(f32, f32, f32, f32)> = Vec::new(); // (x_min, y, x_max, height)

                    for glyph in &frame_glyphs.glyphs {
                        if let FrameGlyph::Char { x, y, width, height, bg: Some(_), .. } = glyph {
                            // Check glyph is within minibuffer bounds
                            if *y >= mb.y && *y < mb.y + mb.height
                                && *x >= mb.x && *x < mb.x + mb.width
                            {
                                // Try to merge with existing row at same Y
                                let mut merged = false;
                                for row in &mut highlighted_rows {
                                    if (row.1 - *y).abs() < 1.0 {
                                        row.0 = row.0.min(*x);
                                        row.2 = row.2.max(*x + *width);
                                        row.3 = row.3.max(*height);
                                        merged = true;
                                        break;
                                    }
                                }
                                if !merged {
                                    highlighted_rows.push((*x, *y, *x + *width, *height));
                                }
                            }
                        }
                    }

                    // Draw glow overlay around each highlighted row
                    let glow_pad = 3.0_f32;
                    for (x_min, y, x_max, height) in &highlighted_rows {
                        let rx = (x_min - glow_pad).max(mb.x);
                        let ry = (y - glow_pad).max(mb.y);
                        let rw = (x_max - x_min + glow_pad * 2.0).min(mb.x + mb.width - rx);
                        let rh = (height + glow_pad * 2.0).min(mb.y + mb.height - ry);

                        // Soft glow: 3-step gradient outward
                        for step in 0..3 {
                            let s = step as f32;
                            let alpha = h_opacity * (1.0 - s / 3.0) * (1.0 - s / 3.0);
                            let c = Color::new(hr, hg, hb, alpha);
                            self.add_rect(&mut highlight_vertices,
                                rx - s, ry - s,
                                rw + s * 2.0, rh + s * 2.0, &c);
                        }
                    }
                }

                if !highlight_vertices.is_empty() {
                    let hl_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Minibuffer Highlight Buffer"),
                        contents: bytemuck::cast_slice(&highlight_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, hl_buffer.slice(..));
                    render_pass.draw(0..highlight_vertices.len() as u32, 0..1);
                }
            }

            // === Scroll velocity fade overlay ===
            if !self.scroll_velocity_fades.is_empty() {
                let max_op = self.scroll_velocity_fade_max_opacity.clamp(0.0, 1.0);
                let mut fade_vertices: Vec<RectVertex> = Vec::new();

                for entry in &self.scroll_velocity_fades {
                    let elapsed = entry.started.elapsed().as_millis() as f32;
                    let duration = entry.duration.as_millis() as f32;
                    if elapsed >= duration { continue; }

                    let t = elapsed / duration;
                    let fade = (1.0 - t) * (1.0 - t); // quadratic ease-out
                    // Velocity factor: higher scroll delta = stronger effect (clamped)
                    let vel_factor = (entry.velocity / 50.0).min(1.0);
                    let alpha = max_op * fade * vel_factor;
                    if alpha < 0.005 { continue; }

                    let b = &entry.bounds;
                    let c = Color::new(0.0, 0.0, 0.0, alpha);
                    self.add_rect(&mut fade_vertices, b.x, b.y, b.width, b.height, &c);
                }

                if !fade_vertices.is_empty() {
                    let fade_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Scroll Velocity Fade Buffer"),
                        contents: bytemuck::cast_slice(&fade_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, fade_buffer.slice(..));
                    render_pass.draw(0..fade_vertices.len() as u32, 0..1);
                }

                // Cleanup expired entries
                self.scroll_velocity_fades.retain(|e| {
                    e.started.elapsed() < e.duration
                });
                if !self.scroll_velocity_fades.is_empty() {
                    self.needs_continuous_redraw = true;
                }
            }

            // === Click halo effect ===
            if !self.click_halos.is_empty() {
                let (hr, hg, hb) = self.click_halo_color;
                let max_r = self.click_halo_max_radius;
                let mut halo_vertices: Vec<RectVertex> = Vec::new();
                let ring_steps = 8;

                for entry in &self.click_halos {
                    let elapsed = entry.started.elapsed().as_millis() as f32;
                    let duration = entry.duration.as_millis() as f32;
                    if elapsed >= duration { continue; }

                    let t = elapsed / duration;
                    let radius = max_r * t;
                    let alpha = 0.4 * (1.0 - t) * (1.0 - t);
                    if alpha < 0.005 { continue; }

                    // Draw expanding ring as concentric rectangles
                    let ring_w = 2.0_f32;
                    for step in 0..ring_steps {
                        let angle = step as f32 * std::f32::consts::TAU / ring_steps as f32;
                        let px = entry.x + radius * angle.cos();
                        let py = entry.y + radius * angle.sin();
                        let c = Color::new(hr, hg, hb, alpha);
                        self.add_rect(&mut halo_vertices, px - ring_w * 0.5, py - ring_w * 0.5, ring_w, ring_w, &c);
                    }
                }

                if !halo_vertices.is_empty() {
                    let halo_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Click Halo Buffer"),
                        contents: bytemuck::cast_slice(&halo_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, halo_buffer.slice(..));
                    render_pass.draw(0..halo_vertices.len() as u32, 0..1);
                }

                self.click_halos.retain(|e| e.started.elapsed() < e.duration);
                if !self.click_halos.is_empty() {
                    self.needs_continuous_redraw = true;
                }
            }

            // === Window edge snap indicator ===
            if !self.edge_snaps.is_empty() {
                let (er, eg, eb) = self.edge_snap_color;
                let mut snap_vertices: Vec<RectVertex> = Vec::new();
                let bar_h = 4.0_f32;
                let steps = 3;

                for entry in &self.edge_snaps {
                    let elapsed = entry.started.elapsed().as_millis() as f32;
                    let duration = entry.duration.as_millis() as f32;
                    if elapsed >= duration { continue; }

                    let t = elapsed / duration;
                    let alpha = 0.5 * (1.0 - t) * (1.0 - t);
                    if alpha < 0.005 { continue; }

                    let b = &entry.bounds;

                    if entry.at_top {
                        // Flash gradient at top edge
                        for i in 0..steps {
                            let frac = i as f32 / steps as f32;
                            let a = alpha * (1.0 - frac);
                            let c = Color::new(er, eg, eb, a);
                            self.add_rect(&mut snap_vertices,
                                b.x, b.y + frac * bar_h,
                                b.width, bar_h / steps as f32, &c);
                        }
                    }
                    if entry.at_bottom {
                        // Flash gradient at bottom edge (above mode-line)
                        let content_bottom = b.y + b.height - entry.mode_line_height;
                        for i in 0..steps {
                            let frac = i as f32 / steps as f32;
                            let a = alpha * (1.0 - frac);
                            let c = Color::new(er, eg, eb, a);
                            self.add_rect(&mut snap_vertices,
                                b.x, content_bottom - bar_h + frac * bar_h,
                                b.width, bar_h / steps as f32, &c);
                        }
                    }
                }

                if !snap_vertices.is_empty() {
                    let snap_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Edge Snap Buffer"),
                        contents: bytemuck::cast_slice(&snap_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, snap_buffer.slice(..));
                    render_pass.draw(0..snap_vertices.len() as u32, 0..1);
                }

                self.edge_snaps.retain(|e| e.started.elapsed() < e.duration);
                if !self.edge_snaps.is_empty() {
                    self.needs_continuous_redraw = true;
                }
            }

            // === Line wrap indicator overlay ===
            if self.wrap_indicator_enabled {
                let (wr, wg, wb) = self.wrap_indicator_color;
                let w_opacity = self.wrap_indicator_opacity.clamp(0.0, 1.0);
                let mut wrap_vertices: Vec<RectVertex> = Vec::new();

                // Detect right fringe wrap indicators (↵ U+21B5) and draw gradient
                for glyph in &frame_glyphs.glyphs {
                    if let FrameGlyph::Char { char: ch, x, y, height, .. } = glyph {
                        if *ch == '\u{21B5}' {
                            // Find which window this belongs to
                            for info in &frame_glyphs.window_infos {
                                if info.is_minibuffer { continue; }
                                let b = &info.bounds;
                                if *y >= b.y && *y < b.y + b.height
                                    && *x >= b.x && *x < b.x + b.width
                                {
                                    // Draw gradient fade at right edge of text area
                                    let grad_w = 20.0_f32.min(b.width * 0.1);
                                    let text_right = *x; // fringe indicator x is at right fringe
                                    let grad_x = text_right - grad_w;
                                    let steps = 5;
                                    for i in 0..steps {
                                        let frac = i as f32 / steps as f32;
                                        let step_alpha = w_opacity * frac * frac; // quadratic fade-in
                                        let sx = grad_x + frac * grad_w;
                                        let sw = grad_w / steps as f32;
                                        let c = Color::new(wr, wg, wb, step_alpha);
                                        self.add_rect(&mut wrap_vertices, sx, *y, sw, *height, &c);
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }

                if !wrap_vertices.is_empty() {
                    let wrap_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Wrap Indicator Buffer"),
                        contents: bytemuck::cast_slice(&wrap_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, wrap_buffer.slice(..));
                    render_pass.draw(0..wrap_vertices.len() as u32, 0..1);
                }
            }

            // === Scroll momentum indicator ===
            if !self.active_scroll_momentums.is_empty() {
                let bar_w = self.scroll_momentum_width.max(1.0);
                let mut momentum_vertices: Vec<RectVertex> = Vec::new();
                let now = std::time::Instant::now();

                for entry in &self.active_scroll_momentums {
                    let elapsed = now.duration_since(entry.started);
                    if elapsed >= entry.duration { continue; }
                    let t = elapsed.as_secs_f32() / entry.duration.as_secs_f32();
                    let alpha = (1.0 - t) * (1.0 - t); // quadratic fade-out
                    let b = &entry.bounds;
                    let content_h = b.height;

                    // Draw a gradient bar at the right edge of the window
                    let bar_x = b.x + b.width - bar_w - 2.0;
                    // Arrow pointing in scroll direction at the edge
                    if entry.direction > 0 {
                        // Scrolling down: bar at bottom edge, gradient fades upward
                        let arrow_h = (content_h * 0.15).min(40.0);
                        let steps = 8;
                        for i in 0..steps {
                            let frac = i as f32 / steps as f32;
                            let step_alpha = alpha * (1.0 - frac) * 0.6;
                            let sy = b.y + content_h - arrow_h + frac * arrow_h;
                            let sh = arrow_h / steps as f32;
                            let c = Color::new(0.5, 0.7, 1.0, step_alpha);
                            self.add_rect(&mut momentum_vertices, bar_x, sy, bar_w, sh, &c);
                        }
                    } else {
                        // Scrolling up: bar at top edge, gradient fades downward
                        let arrow_h = (content_h * 0.15).min(40.0);
                        let steps = 8;
                        for i in 0..steps {
                            let frac = i as f32 / steps as f32;
                            let step_alpha = alpha * (1.0 - frac) * 0.6;
                            let sy = b.y + frac * arrow_h;
                            let sh = arrow_h / steps as f32;
                            let c = Color::new(0.5, 0.7, 1.0, step_alpha);
                            self.add_rect(&mut momentum_vertices, bar_x, sy, bar_w, sh, &c);
                        }
                    }
                }

                if !momentum_vertices.is_empty() {
                    let momentum_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Scroll Momentum Buffer"),
                        contents: bytemuck::cast_slice(&momentum_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, momentum_buffer.slice(..));
                    render_pass.draw(0..momentum_vertices.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Vignette effect: darken edges of the frame ===
            if self.vignette_enabled {
                let frame_w = frame_glyphs.width;
                let frame_h = frame_glyphs.height;
                let intensity = self.vignette_intensity.clamp(0.0, 1.0);
                let radius_pct = self.vignette_radius.clamp(10.0, 100.0) / 100.0;

                // Number of gradient steps from edge inward
                let steps = 16;
                let mut vignette_vertices: Vec<RectVertex> = Vec::new();

                for i in 0..steps {
                    let t = i as f32 / steps as f32;
                    // Quadratic falloff for smooth gradient
                    let alpha = intensity * t * t;
                    let inset = (1.0 - t) * radius_pct * frame_w.min(frame_h) * 0.5;
                    let strip_w = inset / steps as f32;

                    if strip_w < 0.5 { continue; }

                    let color = Color::new(0.0, 0.0, 0.0, alpha);

                    // Top edge strip
                    let y = inset - strip_w;
                    if y >= 0.0 {
                        self.add_rect(&mut vignette_vertices, 0.0, y, frame_w, strip_w, &color);
                    }
                    // Bottom edge strip
                    let by = frame_h - inset;
                    if by < frame_h {
                        self.add_rect(&mut vignette_vertices, 0.0, by, frame_w, strip_w, &color);
                    }
                    // Left edge strip
                    let lx = inset - strip_w;
                    if lx >= 0.0 {
                        self.add_rect(&mut vignette_vertices, lx, 0.0, strip_w, frame_h, &color);
                    }
                    // Right edge strip
                    let rx = frame_w - inset;
                    if rx < frame_w {
                        self.add_rect(&mut vignette_vertices, rx, 0.0, strip_w, frame_h, &color);
                    }
                }

                if !vignette_vertices.is_empty() {
                    let vignette_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Vignette Buffer"),
                            contents: bytemuck::cast_slice(&vignette_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, vignette_buffer.slice(..));
                    render_pass.draw(0..vignette_vertices.len() as u32, 0..1);
                }
            }

            // === Window switch highlight fade ===
            if !self.active_window_fades.is_empty() {
                let mut fade_vertices: Vec<RectVertex> = Vec::new();
                let now = std::time::Instant::now();

                for fade in &self.active_window_fades {
                    let elapsed = now.duration_since(fade.started);
                    let t = (elapsed.as_secs_f32() / fade.duration.as_secs_f32()).min(1.0);
                    if t >= 1.0 { continue; }
                    // Ease-out: bright flash that fades smoothly
                    let eased = t * (2.0 - t);
                    let alpha = fade.intensity * (1.0 - eased);

                    let color = Color::new(1.0, 1.0, 1.0, alpha);
                    self.add_rect(
                        &mut fade_vertices,
                        fade.bounds.x, fade.bounds.y,
                        fade.bounds.width, fade.bounds.height,
                        &color,
                    );
                }

                // Clean up completed fades
                self.active_window_fades.retain(|f| {
                    f.started.elapsed().as_secs_f32() < f.duration.as_secs_f32()
                });

                if !self.active_window_fades.is_empty() {
                    self.needs_continuous_redraw = true;
                }

                if !fade_vertices.is_empty() {
                    let fade_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Window Switch Fade Buffer"),
                            contents: bytemuck::cast_slice(&fade_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, fade_buffer.slice(..));
                    render_pass.draw(0..fade_vertices.len() as u32, 0..1);
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    // ========================================================================
    // Offscreen texture management (for transitions)
    // ========================================================================

    /// Get the surface format
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }

    /// Get the image bind group layout (for creating bind groups for offscreen textures)
    pub fn image_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.image_cache.bind_group_layout()
    }

    /// Get the image sampler (for creating bind groups for offscreen textures)
    pub fn image_sampler(&self) -> &wgpu::Sampler {
        self.image_cache.sampler()
    }

    /// Get the uniform bind group (needed for composite rendering)
    pub fn uniform_bind_group(&self) -> &wgpu::BindGroup {
        &self.uniform_bind_group
    }

    /// Get the image pipeline (needed for blit and scroll slide)
    pub fn image_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.image_pipeline
    }

    /// Create an offscreen texture suitable for rendering a full frame
    pub fn create_offscreen_texture(&self, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Offscreen Frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        (tex, view)
    }

    /// Create a bind group for a texture view (usable with image_pipeline)
    pub fn create_texture_bind_group(&self, view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Offscreen Bind Group"),
            layout: self.image_cache.bind_group_layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(self.image_cache.sampler()),
                },
            ],
        })
    }

    /// Blit a texture to a target view (fullscreen quad)
    pub fn blit_texture_to_view(
        &self,
        src_bind_group: &wgpu::BindGroup,
        dst_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) {
        // Use logical dimensions for vertex positions since screen_size uniform is logical
        let w = width as f32 / self.scale_factor;
        let h = height as f32 / self.scale_factor;

        let vertices = [
            GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [w, 0.0], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [0.0, h], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
        ];

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blit Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Blit Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.image_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_bind_group(1, src_bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a crossfade transition within a scissor region
    /// Uses the image_pipeline to blend old and new textures
    pub fn render_crossfade(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        blend_t: f32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        // We render two passes: old texture with alpha (1-t), new texture with alpha t
        // Using scissor rect to constrain to the window bounds

        // Scissor rects operate in physical framebuffer pixels; bounds from Emacs are logical
        let sf = self.scale_factor;
        let sx = (bounds.x.max(0.0) * sf) as u32;
        let sy = (bounds.y.max(0.0) * sf) as u32;
        let sw = ((bounds.width * sf) as u32).min(surface_width.saturating_sub(sx));
        let sh = ((bounds.height * sf) as u32).min(surface_height.saturating_sub(sy));

        if sw == 0 || sh == 0 {
            return;
        }

        // Use logical dimensions for vertex positions since screen_size uniform is logical
        let w = surface_width as f32 / sf;
        let h = surface_height as f32 / sf;

        // Fullscreen quad with UV mapping
        let vertices = [
            GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [w, 0.0], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [0.0, h], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
        ];

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Crossfade Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Crossfade Encoder"),
        });

        {
            // Pass 1: Draw old texture with alpha (1 - blend_t)
            let old_alpha = 1.0 - blend_t;
            let old_vertices = [
                GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, old_alpha] },
                GlyphVertex { position: [w, 0.0], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, old_alpha] },
                GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, old_alpha] },
                GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, old_alpha] },
                GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, old_alpha] },
                GlyphVertex { position: [0.0, h], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, old_alpha] },
            ];
            let old_vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Crossfade Old VB"),
                contents: bytemuck::cast_slice(&old_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            // New texture with alpha blend_t
            let new_vertices = [
                GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, blend_t] },
                GlyphVertex { position: [w, 0.0], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, blend_t] },
                GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, blend_t] },
                GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, blend_t] },
                GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, blend_t] },
                GlyphVertex { position: [0.0, h], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, blend_t] },
            ];
            let new_vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Crossfade New VB"),
                contents: bytemuck::cast_slice(&new_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Crossfade Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_scissor_rect(sx, sy, sw, sh);
            render_pass.set_pipeline(&self.image_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            // Draw old with fading alpha
            render_pass.set_bind_group(1, old_bind_group, &[]);
            render_pass.set_vertex_buffer(0, old_vb.slice(..));
            render_pass.draw(0..6, 0..1);

            // Draw new with increasing alpha
            render_pass.set_bind_group(1, new_bind_group, &[]);
            render_pass.set_vertex_buffer(0, new_vb.slice(..));
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a scroll slide transition within a scissor region
    ///
    /// Uses content-region UV mapping so only the content area of each offscreen
    /// texture is sampled — the mode-line is never included in the sliding quads.
    pub fn render_scroll_slide(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        // Ease-out quadratic
        let eased_t = 1.0 - (1.0 - t).powi(2);
        let offset = bounds.height * eased_t;

        // Scissor rects operate in physical framebuffer pixels; bounds from Emacs are logical
        let sf = self.scale_factor;
        let sx = (bounds.x.max(0.0) * sf) as u32;
        let sy = (bounds.y.max(0.0) * sf) as u32;
        let sw = ((bounds.width * sf) as u32).min(surface_width.saturating_sub(sx));
        let sh = ((bounds.height * sf) as u32).min(surface_height.saturating_sub(sy));

        if sw == 0 || sh == 0 {
            return;
        }

        // Use logical dimensions for vertex positions since screen_size uniform is logical
        let w = surface_width as f32 / sf;
        let h = surface_height as f32 / sf;
        let dir = direction as f32;

        // UV coordinates for the content region within the full-frame texture.
        // bounds is already content-only (mode-line excluded by caller).
        let uv_left = bounds.x / w;
        let uv_top = bounds.y / h;
        let uv_right = (bounds.x + bounds.width) / w;
        let uv_bottom = (bounds.y + bounds.height) / h;

        // Old texture slides out by offset in direction
        let old_y_offset = -dir * offset;
        // New texture slides in from opposite side
        let new_y_offset = dir * (bounds.height - offset);

        // Build a content-region quad: position covers the content bounds shifted
        // by y_off, UV maps to exactly the content region in the full-frame texture.
        let make_quad = |y_off: f32| -> [GlyphVertex; 6] {
            let x0 = bounds.x;
            let x1 = bounds.x + bounds.width;
            let y0 = bounds.y + y_off;
            let y1 = bounds.y + bounds.height + y_off;
            [
                GlyphVertex { position: [x0, y0], tex_coords: [uv_left, uv_top], color: [1.0, 1.0, 1.0, 1.0] },
                GlyphVertex { position: [x1, y0], tex_coords: [uv_right, uv_top], color: [1.0, 1.0, 1.0, 1.0] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_right, uv_bottom], color: [1.0, 1.0, 1.0, 1.0] },
                GlyphVertex { position: [x0, y0], tex_coords: [uv_left, uv_top], color: [1.0, 1.0, 1.0, 1.0] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_right, uv_bottom], color: [1.0, 1.0, 1.0, 1.0] },
                GlyphVertex { position: [x0, y1], tex_coords: [uv_left, uv_bottom], color: [1.0, 1.0, 1.0, 1.0] },
            ]
        };

        let old_vertices = make_quad(old_y_offset);
        let new_vertices = make_quad(new_y_offset);

        let old_vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scroll Old VB"),
            contents: bytemuck::cast_slice(&old_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let new_vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scroll New VB"),
            contents: bytemuck::cast_slice(&new_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Scroll Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scroll Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_scissor_rect(sx, sy, sw, sh);
            render_pass.set_pipeline(&self.image_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            // Draw old texture sliding out
            render_pass.set_bind_group(1, old_bind_group, &[]);
            render_pass.set_vertex_buffer(0, old_vb.slice(..));
            render_pass.draw(0..6, 0..1);

            // Draw new texture sliding in
            render_pass.set_bind_group(1, new_bind_group, &[]);
            render_pass.set_vertex_buffer(0, new_vb.slice(..));
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Dispatch to the appropriate scroll effect renderer.
    ///
    /// This is the main entry point called by `render_transitions()` for each
    /// active scroll transition. It applies the easing function to `raw_t`,
    /// then delegates to the specific effect renderer.
    pub fn render_scroll_effect(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        raw_t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        effect: crate::core::scroll_animation::ScrollEffect,
        easing: crate::core::scroll_animation::ScrollEasing,
        surface_width: u32,
        surface_height: u32,
    ) {
        use crate::core::scroll_animation::ScrollEffect;

        let eased_t = easing.apply(raw_t);

        match effect {
            ScrollEffect::Slide => {
                // Use existing slide renderer (it has its own easing, pass raw_t)
                self.render_scroll_slide(
                    surface_view, old_bind_group, new_bind_group,
                    raw_t, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::Crossfade => {
                self.render_scroll_crossfade(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::ScaleZoom => {
                self.render_scroll_scale_zoom(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::FadeEdges => {
                self.render_scroll_fade_edges(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::Cascade => {
                self.render_scroll_cascade(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, elapsed_secs, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::Parallax => {
                self.render_scroll_parallax(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::Tilt => {
                self.render_scroll_tilt(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::PageCurl => {
                self.render_scroll_page_curl(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::CardFlip => {
                self.render_scroll_card_flip(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::CylinderRoll => {
                self.render_scroll_cylinder_roll(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::Wobbly => {
                self.render_scroll_wobbly(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, elapsed_secs, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::Wave => {
                self.render_scroll_wave(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, elapsed_secs, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::PerLineSpring => {
                self.render_scroll_per_line_spring(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, elapsed_secs, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::Liquid => {
                self.render_scroll_liquid(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, elapsed_secs, direction, bounds, surface_width, surface_height,
                );
            }

            // Post-processing effects: render slide first, then apply post-process
            ScrollEffect::MotionBlur
            | ScrollEffect::ChromaticAberration
            | ScrollEffect::GhostTrails
            | ScrollEffect::ColorTemperature
            | ScrollEffect::CRTScanlines
            | ScrollEffect::DepthOfField => {
                self.render_scroll_with_post_process(
                    surface_view, old_bind_group, new_bind_group,
                    raw_t, eased_t, elapsed_secs, direction, bounds,
                    effect, surface_width, surface_height,
                );
            }

            ScrollEffect::TypewriterReveal => {
                self.render_scroll_typewriter(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, elapsed_secs, direction, bounds, surface_width, surface_height,
                );
            }
        }
    }

    // ── Scroll Effect Implementations ─────────────────────────────────────

    /// Helper: compute scissor rect and content UV from bounds.
    fn scroll_scissor_and_uv(
        &self,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) -> Option<(u32, u32, u32, u32, f32, f32, f32, f32, f32, f32)> {
        let sf = self.scale_factor;
        let sx = (bounds.x.max(0.0) * sf) as u32;
        let sy = (bounds.y.max(0.0) * sf) as u32;
        let sw = ((bounds.width * sf) as u32).min(surface_width.saturating_sub(sx));
        let sh = ((bounds.height * sf) as u32).min(surface_height.saturating_sub(sy));
        if sw == 0 || sh == 0 {
            return None;
        }
        let w = surface_width as f32 / sf;
        let h = surface_height as f32 / sf;
        let uv_left = bounds.x / w;
        let uv_top = bounds.y / h;
        let uv_right = (bounds.x + bounds.width) / w;
        let uv_bottom = (bounds.y + bounds.height) / h;
        Some((sx, sy, sw, sh, w, h, uv_left, uv_top, uv_right, uv_bottom))
    }

    /// Helper: create a vertex buffer from GlyphVertex slice.
    fn create_scroll_vb(&self, vertices: &[GlyphVertex]) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scroll VB"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        })
    }

    /// Helper: submit a two-quad scroll render pass (old + new textures).
    fn submit_scroll_two_quad_pass(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        old_vertices: &[GlyphVertex],
        new_vertices: &[GlyphVertex],
        sx: u32, sy: u32, sw: u32, sh: u32,
    ) {
        use wgpu::util::DeviceExt;
        let old_vb = self.create_scroll_vb(old_vertices);
        let new_vb = self.create_scroll_vb(new_vertices);

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Scroll Effect Encoder"),
        });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scroll Effect Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rp.set_scissor_rect(sx, sy, sw, sh);
            rp.set_pipeline(&self.image_pipeline);
            rp.set_bind_group(0, &self.uniform_bind_group, &[]);

            rp.set_bind_group(1, old_bind_group, &[]);
            rp.set_vertex_buffer(0, old_vb.slice(..));
            rp.draw(0..old_vertices.len() as u32, 0..1);

            rp.set_bind_group(1, new_bind_group, &[]);
            rp.set_vertex_buffer(0, new_vb.slice(..));
            rp.draw(0..new_vertices.len() as u32, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Crossfade scroll: alpha blend old → new within content bounds.
    fn render_scroll_crossfade(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };
        let x0 = bounds.x;
        let y0 = bounds.y;
        let x1 = bounds.x + bounds.width;
        let y1 = bounds.y + bounds.height;
        let old_a = 1.0 - t;

        let old_verts = [
            GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, old_a] },
            GlyphVertex { position: [x1, y0], tex_coords: [uv_r, uv_t], color: [1.0, 1.0, 1.0, old_a] },
            GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, old_a] },
            GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, old_a] },
            GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, old_a] },
            GlyphVertex { position: [x0, y1], tex_coords: [uv_l, uv_b], color: [1.0, 1.0, 1.0, old_a] },
        ];
        let new_verts = [
            GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, t] },
            GlyphVertex { position: [x1, y0], tex_coords: [uv_r, uv_t], color: [1.0, 1.0, 1.0, t] },
            GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, t] },
            GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, t] },
            GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, t] },
            GlyphVertex { position: [x0, y1], tex_coords: [uv_l, uv_b], color: [1.0, 1.0, 1.0, t] },
        ];
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// ScaleZoom: old shrinks to 95% and fades; new zooms from 95% to 100%.
    fn render_scroll_scale_zoom(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };
        let cx = bounds.x + bounds.width / 2.0;
        let cy = bounds.y + bounds.height / 2.0;

        // Old: scale from 1.0 → 0.92, fade out
        let old_scale = 1.0 - t * 0.08;
        let old_a = 1.0 - t;
        let old_hw = bounds.width / 2.0 * old_scale;
        let old_hh = bounds.height / 2.0 * old_scale;

        // New: scale from 0.92 → 1.0, fade in
        let new_scale = 0.92 + t * 0.08;
        let new_hw = bounds.width / 2.0 * new_scale;
        let new_hh = bounds.height / 2.0 * new_scale;

        let make_quad = |hw: f32, hh: f32, alpha: f32| -> [GlyphVertex; 6] {
            let x0 = cx - hw;
            let y0 = cy - hh;
            let x1 = cx + hw;
            let y1 = cy + hh;
            [
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y0], tex_coords: [uv_r, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x0, y1], tex_coords: [uv_l, uv_b], color: [1.0, 1.0, 1.0, alpha] },
            ]
        };

        let old_verts = make_quad(old_hw, old_hh, old_a);
        let new_verts = make_quad(new_hw, new_hh, t);
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// FadeEdges: slide with soft fade at viewport top/bottom edges.
    fn render_scroll_fade_edges(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let offset = bounds.height * t;
        let num_strips = 16;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let fade_zone = 0.15; // fade over 15% of height at each edge

        let make_strips = |y_off: f32, is_old: bool| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let rel_y0 = bounds.y + i as f32 * strip_h + y_off;
                let rel_y1 = bounds.y + (i + 1) as f32 * strip_h + y_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                // Alpha based on distance from edge
                let center_y = (rel_y0 + rel_y1) / 2.0;
                let in_bounds_t = ((center_y - bounds.y) / bounds.height).clamp(0.0, 1.0);
                let edge_alpha = if in_bounds_t < fade_zone {
                    in_bounds_t / fade_zone
                } else if in_bounds_t > 1.0 - fade_zone {
                    (1.0 - in_bounds_t) / fade_zone
                } else {
                    1.0
                };
                let base_alpha = if is_old { 1.0 - t } else { t };
                let alpha = (base_alpha * edge_alpha).clamp(0.0, 1.0);

                let x0 = bounds.x;
                let x1 = bounds.x + bounds.width;
                let c = [1.0, 1.0, 1.0, alpha];
                verts.push(GlyphVertex { position: [x0, rel_y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, rel_y0], tex_coords: [uv_r, u0], color: c });
                verts.push(GlyphVertex { position: [x1, rel_y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, rel_y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, rel_y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, rel_y1], tex_coords: [uv_l, u1], color: c });
            }
            verts
        };

        let old_y_off = -dir * offset;
        let new_y_off = dir * (bounds.height - offset);
        let old_verts = make_strips(old_y_off, true);
        let new_verts = make_strips(new_y_off, false);
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Cascade: lines drop in with staggered delay (waterfall).
    fn render_scroll_cascade(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let dir = direction as f32;
        let stagger = 0.06; // 60ms stagger per line

        let make_cascade_strips = |bind: &wgpu::BindGroup, is_new: bool| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let line_delay = i as f32 * stagger;
                let line_t = ((t - line_delay / 1.0).max(0.0) / (1.0 - line_delay).max(0.01)).min(1.0);
                let eased = 1.0 - (1.0 - line_t).powi(2);

                let base_y = bounds.y + i as f32 * strip_h;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                let (y_off, alpha) = if is_new {
                    (dir * (bounds.height * (1.0 - eased)), eased)
                } else {
                    (-dir * (bounds.height * eased), 1.0 - eased)
                };

                let y0 = base_y + y_off;
                let y1 = base_y + strip_h + y_off;
                let x0 = bounds.x;
                let x1 = bounds.x + bounds.width;
                let c = [1.0, 1.0, 1.0, alpha];

                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y0], tex_coords: [uv_r, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, y1], tex_coords: [uv_l, u1], color: c });
            }
            verts
        };

        let old_verts = make_cascade_strips(old_bind_group, false);
        let new_verts = make_cascade_strips(new_bind_group, true);
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Parallax: layers scroll at different speeds for depth illusion.
    fn render_scroll_parallax(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        // Foreground scrolls at normal speed, "background" slower
        // We simulate by having old content move slower (0.7x) and new content normal
        let slow_t = t * 0.7;
        let slow_offset = bounds.height * slow_t;
        let fast_offset = bounds.height * t;

        let make_quad = |y_off: f32, alpha: f32| -> [GlyphVertex; 6] {
            let x0 = bounds.x;
            let y0 = bounds.y + y_off;
            let x1 = bounds.x + bounds.width;
            let y1 = bounds.y + bounds.height + y_off;
            [
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y0], tex_coords: [uv_r, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x0, y1], tex_coords: [uv_l, uv_b], color: [1.0, 1.0, 1.0, alpha] },
            ]
        };

        let old_verts = make_quad(-dir * slow_offset, 1.0 - t);
        let new_verts = make_quad(dir * (bounds.height - fast_offset), t);
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Tilt: subtle perspective tilt during scroll.
    fn render_scroll_tilt(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let offset = bounds.height * t;
        let tilt_strength = (1.0 - t) * dir; // Tilt decays as animation settles
        let max_tilt = bounds.height * 0.03; // 3% of height
        let num_strips = 12;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;

        let make_tilted = |y_base_off: f32| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let nt0 = i as f32 / num_strips as f32;
                let nt1 = (i + 1) as f32 / num_strips as f32;

                // Tilt: center stays, edges deflect
                let tilt0 = max_tilt * tilt_strength * (nt0 - 0.5) * 2.0;
                let tilt1 = max_tilt * tilt_strength * (nt1 - 0.5) * 2.0;

                // Horizontal squeeze at edges (perspective)
                let squeeze0 = 1.0 - (nt0 - 0.5).abs() * 0.02 * tilt_strength.abs();
                let squeeze1 = 1.0 - (nt1 - 0.5).abs() * 0.02 * tilt_strength.abs();

                let cx = bounds.x + bounds.width / 2.0;
                let hw0 = bounds.width / 2.0 * squeeze0;
                let hw1 = bounds.width / 2.0 * squeeze1;

                let y0 = bounds.y + i as f32 * strip_h + y_base_off + tilt0;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_base_off + tilt1;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                verts.push(GlyphVertex { position: [cx - hw0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [cx + hw0, y0], tex_coords: [uv_r, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [cx + hw1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [cx - hw0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [cx + hw1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [cx - hw1, y1], tex_coords: [uv_l, u1], color: [1.0; 4] });
            }
            verts
        };

        let old_verts = make_tilted(-dir * offset);
        let new_verts = make_tilted(dir * (bounds.height - offset));
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// PageCurl: page curls away revealing new content underneath.
    fn render_scroll_page_curl(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        use crate::core::scroll_animation::page_curl_transform;
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let num_strips = 24;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;

        // New content: flat, full opacity (drawn first, underneath)
        let new_verts: Vec<GlyphVertex> = {
            let x0 = bounds.x;
            let x1 = bounds.x + bounds.width;
            let y0 = bounds.y;
            let y1 = bounds.y + bounds.height;
            vec![
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0; 4] },
                GlyphVertex { position: [x1, y0], tex_coords: [uv_r, uv_t], color: [1.0; 4] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0; 4] },
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0; 4] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0; 4] },
                GlyphVertex { position: [x0, y1], tex_coords: [uv_l, uv_b], color: [1.0; 4] },
            ]
        };

        // Old content: curling away from bottom (or top for scroll up)
        let old_verts: Vec<GlyphVertex> = {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let nt = i as f32 / num_strips as f32;
                let nt1 = (i + 1) as f32 / num_strips as f32;

                let curl_pos = if direction > 0 { nt } else { 1.0 - nt };
                let (x_off, y_off, alpha) = page_curl_transform(curl_pos, t, bounds.height);

                let x0 = bounds.x + x_off;
                let x1 = bounds.x + bounds.width + x_off;
                let y0 = bounds.y + i as f32 * strip_h + y_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;
                let c = [1.0, 1.0, 1.0, alpha];

                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y0], tex_coords: [uv_r, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, y1], tex_coords: [uv_l, u1], color: c });
            }
            verts
        };

        // Draw new first (underneath), then old (curling on top)
        self.submit_scroll_two_quad_pass(
            surface_view, new_bind_group, old_bind_group,
            &new_verts, &old_verts, sx, sy, sw, sh,
        );
    }

    /// CardFlip: screenful flips like a card around X-axis.
    fn render_scroll_card_flip(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let cx = bounds.x + bounds.width / 2.0;
        let cy = bounds.y + bounds.height / 2.0;

        // Card flip: shrinks height to 0 at midpoint, then expands
        let angle = t * std::f32::consts::PI;
        let scale_y = angle.cos().abs().max(0.02);
        let hh = bounds.height / 2.0 * scale_y;
        let hw = bounds.width / 2.0;

        let (bind_group, alpha) = if t < 0.5 {
            (old_bind_group, 1.0)
        } else {
            (new_bind_group, 1.0)
        };

        let verts = [
            GlyphVertex { position: [cx - hw, cy - hh], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
            GlyphVertex { position: [cx + hw, cy - hh], tex_coords: [uv_r, uv_t], color: [1.0, 1.0, 1.0, alpha] },
            GlyphVertex { position: [cx + hw, cy + hh], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
            GlyphVertex { position: [cx - hw, cy - hh], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
            GlyphVertex { position: [cx + hw, cy + hh], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
            GlyphVertex { position: [cx - hw, cy + hh], tex_coords: [uv_l, uv_b], color: [1.0, 1.0, 1.0, alpha] },
        ];

        // Single texture pass
        let empty: [GlyphVertex; 0] = [];
        self.submit_scroll_two_quad_pass(
            surface_view, bind_group, bind_group,
            &verts, &empty, sx, sy, sw, sh,
        );
    }

    /// CylinderRoll: content wraps around a vertical cylinder.
    fn render_scroll_cylinder_roll(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let num_strips = 16;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let dir = direction as f32;
        let offset = bounds.height * t;
        let pi = std::f32::consts::PI;

        let make_cylinder = |y_base_off: f32, is_old: bool| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            let cx = bounds.x + bounds.width / 2.0;

            for i in 0..num_strips {
                let nt = (i as f32 + 0.5) / num_strips as f32;
                // Angle on the cylinder surface
                let angle = (nt - 0.5) * pi * 0.4 + dir * (1.0 - t) * pi * 0.2;

                let cos_a = angle.cos();
                let squeeze = cos_a.abs().max(0.4);
                let hw = bounds.width / 2.0 * squeeze;

                // Brightness based on angle (facing = bright, edge = dim)
                let brightness = (cos_a * 0.4 + 0.6).clamp(0.3, 1.0);
                let alpha = if is_old { (1.0 - t) * brightness } else { t * brightness };

                let y0 = bounds.y + i as f32 * strip_h + y_base_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_base_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;
                let c = [brightness, brightness, brightness, alpha];

                verts.push(GlyphVertex { position: [cx - hw, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [cx + hw, y0], tex_coords: [uv_r, u0], color: c });
                verts.push(GlyphVertex { position: [cx + hw, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [cx - hw, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [cx + hw, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [cx - hw, y1], tex_coords: [uv_l, u1], color: c });
            }
            verts
        };

        let old_verts = make_cylinder(-dir * offset, true);
        let new_verts = make_cylinder(dir * (bounds.height - offset), false);
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Wobbly/jelly: content deforms elastically during scroll.
    fn render_scroll_wobbly(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        use crate::core::scroll_animation::wobbly_deform;
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let offset = bounds.height * t;
        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let amplitude = bounds.width * 0.03; // 3% of width

        let make_wobbly = |y_base_off: f32| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let nt = i as f32 / num_strips as f32;
                let (dx, _dy) = wobbly_deform(i, num_strips, nt, t, dir, amplitude);

                let x0 = bounds.x + dx;
                let x1 = bounds.x + bounds.width + dx;
                let y0 = bounds.y + i as f32 * strip_h + y_base_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_base_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y0], tex_coords: [uv_r, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y1], tex_coords: [uv_l, u1], color: [1.0; 4] });
            }
            verts
        };

        let old_verts = make_wobbly(-dir * offset);
        let new_verts = make_wobbly(dir * (bounds.height - offset));
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Wave: horizontal sine-wave displacement during scroll.
    fn render_scroll_wave(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let offset = bounds.height * t;
        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let amplitude = bounds.width * 0.025;
        let damping = 1.0 - t;

        let make_wave = |y_base_off: f32| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let nt = i as f32 / num_strips as f32;
                let phase = nt * std::f32::consts::PI * 4.0 + elapsed_secs * 8.0;
                let dx = amplitude * phase.sin() * damping;

                let x0 = bounds.x + dx;
                let x1 = bounds.x + bounds.width + dx;
                let y0 = bounds.y + i as f32 * strip_h + y_base_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_base_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y0], tex_coords: [uv_r, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y1], tex_coords: [uv_l, u1], color: [1.0; 4] });
            }
            verts
        };

        let old_verts = make_wave(-dir * offset);
        let new_verts = make_wave(dir * (bounds.height - offset));
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// PerLineSpring: each line on own spring with stagger delay.
    fn render_scroll_per_line_spring(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let stagger = 0.015; // 15ms stagger per line

        let make_spring = |is_new: bool| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let line_start = i as f32 * stagger;
                let line_t = ((elapsed_secs - line_start).max(0.0) * 8.0).min(1.0);

                // Spring overshoot: goes past 1.0 then settles
                let omega = 10.0;
                let spring_t = if line_t >= 1.0 {
                    1.0
                } else {
                    let et = (-omega * line_t).exp();
                    1.0 - (1.0 + omega * line_t) * et
                };

                let line_offset = bounds.height * spring_t;
                let y_off = if is_new {
                    dir * (bounds.height - line_offset)
                } else {
                    -dir * line_offset
                };

                let x0 = bounds.x;
                let x1 = bounds.x + bounds.width;
                let y0 = bounds.y + i as f32 * strip_h + y_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y0], tex_coords: [uv_r, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y1], tex_coords: [uv_l, u1], color: [1.0; 4] });
            }
            verts
        };

        let old_verts = make_spring(false);
        let new_verts = make_spring(true);
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Liquid: noise-based UV warping, text ripples like water.
    fn render_scroll_liquid(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        use crate::core::scroll_animation::liquid_deform;
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let offset = bounds.height * t;
        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let amplitude = bounds.width * 0.04;

        let make_liquid = |y_base_off: f32| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let nt = i as f32 / num_strips as f32;
                let (dx, dy) = liquid_deform(i, num_strips, nt, t, elapsed_secs, amplitude);

                let x0 = bounds.x + dx;
                let x1 = bounds.x + bounds.width + dx;
                let y0 = bounds.y + i as f32 * strip_h + y_base_off + dy;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_base_off + dy;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y0], tex_coords: [uv_r, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y1], tex_coords: [uv_l, u1], color: [1.0; 4] });
            }
            verts
        };

        let old_verts = make_liquid(-dir * offset);
        let new_verts = make_liquid(dir * (bounds.height - offset));
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Post-processing scroll effects: render slide, then tint/distort via color manipulation.
    ///
    /// Since we don't have a separate post-process shader pipeline yet, we approximate
    /// post-processing effects by manipulating vertex colors during the slide transition.
    fn render_scroll_with_post_process(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        raw_t: f32,
        eased_t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        effect: crate::core::scroll_animation::ScrollEffect,
        surface_width: u32,
        surface_height: u32,
    ) {
        use crate::core::scroll_animation::ScrollEffect;

        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let offset = bounds.height * eased_t;
        let speed = (1.0 - eased_t); // High at start, low at end
        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;

        let make_postprocess = |y_base_off: f32, is_old: bool| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let nt = i as f32 / num_strips as f32;
                let nt_center = (nt - 0.5).abs() * 2.0; // 0 at center, 1 at edges

                let mut r = 1.0_f32;
                let mut g = 1.0_f32;
                let mut b = 1.0_f32;
                let mut alpha = 1.0_f32;
                let mut dx = 0.0_f32;

                match effect {
                    ScrollEffect::MotionBlur => {
                        // Simulate blur by reducing alpha at edges proportional to speed
                        let blur = speed * 0.4;
                        alpha = 1.0 - nt_center * blur;
                    }
                    ScrollEffect::ChromaticAberration => {
                        // Shift color channels based on position and speed
                        let shift = speed * 0.08;
                        r = 1.0 + shift * (nt - 0.5);
                        b = 1.0 - shift * (nt - 0.5);
                    }
                    ScrollEffect::GhostTrails => {
                        // Reduced alpha creates ghost-like transparency
                        let ghost = speed * 0.3;
                        alpha = 1.0 - ghost * nt_center;
                    }
                    ScrollEffect::ColorTemperature => {
                        // Warm (orange) scrolling down, cool (blue) scrolling up
                        let temp = dir * speed * 0.06;
                        r = (1.0 + temp).clamp(0.9, 1.1);
                        b = (1.0 - temp).clamp(0.9, 1.1);
                    }
                    ScrollEffect::CRTScanlines => {
                        // Scanline brightness modulation
                        let scanline = ((nt * num_strips as f32 * 2.0
                            + elapsed_secs * 20.0).sin() * 0.5 + 0.5);
                        let intensity = 1.0 - speed * 0.15 * scanline;
                        r = intensity;
                        g = intensity;
                        b = intensity;
                    }
                    ScrollEffect::DepthOfField => {
                        // Edges get dimmer (simulating blur)
                        let dof = speed * 0.3;
                        let brightness = 1.0 - nt_center * dof;
                        r = brightness;
                        g = brightness;
                        b = brightness;
                    }
                    _ => {}
                }

                let x0 = bounds.x + dx;
                let x1 = bounds.x + bounds.width + dx;
                let y0 = bounds.y + i as f32 * strip_h + y_base_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_base_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;
                let c = [r, g, b, alpha];

                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y0], tex_coords: [uv_r, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, y1], tex_coords: [uv_l, u1], color: c });
            }
            verts
        };

        let old_verts = make_postprocess(-dir * offset, true);
        let new_verts = make_postprocess(dir * (bounds.height - offset), false);
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// TypewriterReveal: new lines appear character-by-character (simulated with strips).
    fn render_scroll_typewriter(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let stagger = 0.04; // 40ms per line

        // Old content fades out quickly
        let old_verts: Vec<GlyphVertex> = {
            let alpha = (1.0 - t * 2.0).max(0.0);
            let x0 = bounds.x;
            let x1 = bounds.x + bounds.width;
            let y0 = bounds.y;
            let y1 = bounds.y + bounds.height;
            vec![
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y0], tex_coords: [uv_r, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x0, y1], tex_coords: [uv_l, uv_b], color: [1.0, 1.0, 1.0, alpha] },
            ]
        };

        // New content: each line reveals left-to-right with stagger
        let new_verts: Vec<GlyphVertex> = {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let line_delay = i as f32 * stagger;
                let line_t = ((t - line_delay).max(0.0) / (1.0 - line_delay).max(0.01)).min(1.0);

                // Reveal from left: only show portion of UV
                let reveal = line_t;
                let uv_reveal_right = uv_l + (uv_r - uv_l) * reveal;
                let x_right = bounds.x + bounds.width * reveal;

                let y0 = bounds.y + i as f32 * strip_h;
                let y1 = bounds.y + (i + 1) as f32 * strip_h;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;
                let alpha = line_t;

                verts.push(GlyphVertex { position: [bounds.x, y0], tex_coords: [uv_l, u0], color: [1.0, 1.0, 1.0, alpha] });
                verts.push(GlyphVertex { position: [x_right, y0], tex_coords: [uv_reveal_right, u0], color: [1.0, 1.0, 1.0, alpha] });
                verts.push(GlyphVertex { position: [x_right, y1], tex_coords: [uv_reveal_right, u1], color: [1.0, 1.0, 1.0, alpha] });
                verts.push(GlyphVertex { position: [bounds.x, y0], tex_coords: [uv_l, u0], color: [1.0, 1.0, 1.0, alpha] });
                verts.push(GlyphVertex { position: [x_right, y1], tex_coords: [uv_reveal_right, u1], color: [1.0, 1.0, 1.0, alpha] });
                verts.push(GlyphVertex { position: [bounds.x, y1], tex_coords: [uv_l, u1], color: [1.0, 1.0, 1.0, alpha] });
            }
            verts
        };

        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Render floating videos from the scene.
    ///
    /// This renders video frames at fixed screen positions (not inline with text).
    #[cfg(feature = "video")]
    pub fn render_floating_videos(
        &self,
        view: &wgpu::TextureView,
        floating_videos: &[crate::core::scene::FloatingVideo],
    ) {
        use wgpu::util::DeviceExt;

        if floating_videos.is_empty() {
            return;
        }

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Floating Video Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Floating Video Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Don't clear - render on top
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.image_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            for fv in floating_videos {
                log::debug!("Rendering floating video {} at ({}, {}) size {}x{}",
                           fv.video_id, fv.x, fv.y, fv.width, fv.height);

                if let Some(cached) = self.video_cache.get(fv.video_id) {
                    if let Some(ref bind_group) = cached.bind_group {
                        let vertices = [
                            GlyphVertex { position: [fv.x, fv.y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x + fv.width, fv.y], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x + fv.width, fv.y + fv.height], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x, fv.y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x + fv.width, fv.y + fv.height], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x, fv.y + fv.height], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                        ];

                        let video_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Floating Video Vertex Buffer"),
                            contents: bytemuck::cast_slice(&vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                        render_pass.set_bind_group(1, bind_group, &[]);
                        render_pass.set_vertex_buffer(0, video_buffer.slice(..));
                        render_pass.draw(0..6, 0..1);
                    } else {
                        log::debug!("Video {} has no bind_group yet", fv.video_id);
                    }
                } else {
                    log::debug!("Video {} not found in cache", fv.video_id);
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a WebKit view texture at the given bounds.
    ///
    /// This method renders the WebKit view content (from a wgpu texture)
    /// to the screen at the specified rectangle.
    ///
    /// # Arguments
    /// * `_encoder` - The command encoder to use for rendering
    /// * `_view` - The output texture view to render to
    /// * `_webkit_bind_group` - The bind group containing the WebKit texture
    /// * `_bounds` - The rectangle where the WebKit view should be rendered
    #[cfg(feature = "wpe-webkit")]
    pub fn render_webkit_view(
        &mut self,
        _encoder: &mut wgpu::CommandEncoder,
        _view: &wgpu::TextureView,
        _webkit_bind_group: &wgpu::BindGroup,
        _bounds: crate::core::types::Rect,
    ) {
        // TODO: Implement texture rendering
    }

    /// Render a popup menu overlay on top of all content.
    pub fn render_popup_menu(
        &self,
        view: &wgpu::TextureView,
        menu: &crate::render_thread::PopupMenuState,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Derive colors from face colors if provided, otherwise use defaults.
        let (fg_r, fg_g, fg_b) = menu.face_fg.unwrap_or((0.9, 0.9, 0.9));
        let (bg_r, bg_g, bg_b) = menu.face_bg.unwrap_or((0.15, 0.15, 0.18));

        let bg_color = Color::new(bg_r, bg_g, bg_b, 0.95).srgb_to_linear();
        let border_color = Color::new(
            (bg_r * 0.6 + 0.15).min(1.0),
            (bg_g * 0.6 + 0.15).min(1.0),
            (bg_b * 0.6 + 0.15).min(1.0),
            1.0,
        ).srgb_to_linear();
        let hover_color = Color::new(
            bg_r * 0.5 + fg_r * 0.3,
            bg_g * 0.5 + fg_g * 0.3,
            bg_b * 0.5 + fg_b * 0.3,
            0.9,
        ).srgb_to_linear();
        let text_color = {
            let c = Color::new(fg_r, fg_g, fg_b, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let disabled_color = {
            let c = Color::new(
                fg_r * 0.5 + bg_r * 0.5,
                fg_g * 0.5 + bg_g * 0.5,
                fg_b * 0.5 + bg_b * 0.5,
                1.0,
            ).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let separator_color = Color::new(
            bg_r * 0.7 + fg_r * 0.3,
            bg_g * 0.7 + fg_g * 0.3,
            bg_b * 0.7 + fg_b * 0.3,
            0.8,
        ).srgb_to_linear();
        let title_color = {
            let c = Color::new(
                fg_r * 0.8 + bg_r * 0.2,
                fg_g * 0.8 + bg_g * 0.2,
                fg_b * 0.85 + bg_b * 0.15,
                1.0,
            ).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let shortcut_color = {
            let c = Color::new(
                fg_r * 0.65 + bg_r * 0.35,
                fg_g * 0.65 + bg_g * 0.35,
                fg_b * 0.65 + bg_b * 0.35,
                1.0,
            ).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };

        let padding = 4.0_f32;
        let font_size = glyph_atlas.default_font_size();
        let char_width = font_size * 0.6;
        let font_size_bits = 0.0_f32.to_bits();

        // Render each panel (root + open submenus)
        let panels = menu.panels();
        for (panel_idx, panel) in panels.iter().enumerate() {
            let (mx, my, mw, mh) = panel.bounds;

            // === Pass 1: Background rectangles ===
            let mut rect_vertices: Vec<RectVertex> = Vec::new();

            // Drop shadow
            let shadow_layers = 4;
            for i in 1..=shadow_layers {
                let offset = i as f32 * 1.5;
                let alpha = 0.12 * (1.0 - (i - 1) as f32 / shadow_layers as f32);
                let shadow = Color::new(0.0, 0.0, 0.0, alpha);
                self.add_rect(&mut rect_vertices, mx + offset, my + offset, mw, mh, &shadow);
            }

            // Background
            self.add_rect(&mut rect_vertices, mx, my, mw, mh, &bg_color);

            // Border
            let bw = 1.0_f32;
            self.add_rect(&mut rect_vertices, mx, my, mw, bw, &border_color);
            self.add_rect(&mut rect_vertices, mx, my + mh - bw, mw, bw, &border_color);
            self.add_rect(&mut rect_vertices, mx, my, bw, mh, &border_color);
            self.add_rect(&mut rect_vertices, mx + mw - bw, my, bw, mh, &border_color);

            // Hover highlight
            if panel.hover_index >= 0 && (panel.hover_index as usize) < panel.item_indices.len() {
                let idx = panel.hover_index as usize;
                let iy = my + panel.item_offsets[idx];
                self.add_rect(&mut rect_vertices, mx + bw, iy, mw - 2.0 * bw, panel.item_height, &hover_color);
            }

            // Separators
            for (i, &item_idx) in panel.item_indices.iter().enumerate() {
                if menu.all_items[item_idx].separator {
                    let iy = my + panel.item_offsets[i] + 3.0;
                    self.add_rect(&mut rect_vertices, mx + 8.0, iy, mw - 16.0, 1.0, &separator_color);
                }
            }

            // Title separator (root panel only)
            if panel_idx == 0 {
                if menu.title.is_some() {
                    let sep_y = my + panel.item_height + 2.0;
                    self.add_rect(&mut rect_vertices, mx + 4.0, sep_y, mw - 8.0, 1.0, &separator_color);
                }
            }

            // Submit rect pass
            if !rect_vertices.is_empty() {
                let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Popup Menu Rect Buffer"),
                    contents: bytemuck::cast_slice(&rect_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Popup Menu Rect Encoder"),
                });
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Popup Menu Rect Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    pass.set_pipeline(&self.rect_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_vertex_buffer(0, rect_buffer.slice(..));
                    pass.draw(0..rect_vertices.len() as u32, 0..1);
                }
                self.queue.submit(Some(encoder.finish()));
            }

            // === Pass 2: Text glyphs ===
            let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();

            // Title (root panel only)
            if panel_idx == 0 {
                if let Some(ref title) = menu.title {
                    let tx = mx + padding * 2.0;
                    for (ci, ch) in title.chars().enumerate() {
                        let key = GlyphKey {
                            charcode: ch as u32,
                            face_id: 0,
                            font_size_bits,
                        };
                        glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                        overlay_glyphs.push((key, tx + (ci as f32) * char_width, my + padding, title_color));
                    }
                }
            }

            // Menu items
            for (i, &item_idx) in panel.item_indices.iter().enumerate() {
                let item = &menu.all_items[item_idx];
                if item.separator {
                    continue;
                }
                let iy = my + panel.item_offsets[i];
                let color = if !item.enabled { disabled_color } else { text_color };

                let label_x = mx + padding * 2.0;
                for (ci, ch) in item.label.chars().enumerate() {
                    let key = GlyphKey {
                        charcode: ch as u32,
                        face_id: 0,
                        font_size_bits,
                    };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    overlay_glyphs.push((key, label_x + (ci as f32) * char_width, iy + 2.0, color));
                }

                if !item.shortcut.is_empty() {
                    let shortcut_x = mx + mw - padding * 2.0 - (item.shortcut.len() as f32 * char_width);
                    for (ci, ch) in item.shortcut.chars().enumerate() {
                        let key = GlyphKey {
                            charcode: ch as u32,
                            face_id: 0,
                            font_size_bits,
                        };
                        glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                        overlay_glyphs.push((key, shortcut_x + (ci as f32) * char_width, iy + 2.0, shortcut_color));
                    }
                }

                if item.submenu {
                    let arrow_x = mx + mw - padding * 2.0 - char_width;
                    let key = GlyphKey {
                        charcode: '\u{25B8}' as u32,
                        face_id: 0,
                        font_size_bits,
                    };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    overlay_glyphs.push((key, arrow_x, iy + 2.0, text_color));
                }
            }

            self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
        }
    }

    /// Render a batch of overlay glyphs in a single render pass.
    ///
    /// Each entry is (GlyphKey, x, y, color). Glyphs are sorted by key
    /// so identical characters share a single bind_group switch, and all
    /// rendering happens in one encoder submit instead of one per glyph.
    fn render_overlay_glyphs(
        &self,
        view: &wgpu::TextureView,
        glyphs: &mut Vec<(GlyphKey, f32, f32, [f32; 4])>,
        glyph_atlas: &WgpuGlyphAtlas,
    ) {
        use wgpu::util::DeviceExt;

        if glyphs.is_empty() {
            return;
        }

        // Sort by key for batching consecutive same-texture draws
        glyphs.sort_by(|a, b| {
            a.0.face_id.cmp(&b.0.face_id)
                .then(a.0.font_size_bits.cmp(&b.0.font_size_bits))
                .then(a.0.charcode.cmp(&b.0.charcode))
        });

        // Build vertex buffer for all glyphs at once
        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(glyphs.len() * 6);
        let mut valid: Vec<bool> = Vec::with_capacity(glyphs.len());

        for (key, x, y, color) in glyphs.iter() {
            if let Some(cached) = glyph_atlas.get(key) {
                let gw = cached.width as f32;
                let gh = cached.height as f32;
                let gx = *x + cached.bearing_x;
                let gy = *y - cached.bearing_y + 14.0;

                vertices.extend_from_slice(&[
                    GlyphVertex { position: [gx, gy], tex_coords: [0.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy], tex_coords: [1.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy + gh], tex_coords: [1.0, 1.0], color: *color },
                    GlyphVertex { position: [gx, gy], tex_coords: [0.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy + gh], tex_coords: [1.0, 1.0], color: *color },
                    GlyphVertex { position: [gx, gy + gh], tex_coords: [0.0, 1.0], color: *color },
                ]);
                valid.push(true);
            } else {
                valid.push(false);
            }
        }

        if vertices.is_empty() {
            return;
        }

        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Overlay Glyph Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Overlay Glyph Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Overlay Glyph Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.image_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));

            // Batch draw calls: consecutive same-key glyphs share one bind_group
            let mut vert_idx = 0u32;
            let mut i = 0;
            while i < glyphs.len() {
                if !valid[i] {
                    i += 1;
                    continue;
                }
                let (ref key, _, _, _) = glyphs[i];
                if let Some(cached) = glyph_atlas.get(key) {
                    if cached.is_color {
                        pass.set_pipeline(&self.opaque_image_pipeline);
                    } else {
                        pass.set_pipeline(&self.image_pipeline);
                    }
                    pass.set_bind_group(1, &cached.bind_group, &[]);
                    let batch_start = vert_idx;
                    vert_idx += 6;
                    i += 1;
                    while i < glyphs.len() && valid[i] && glyphs[i].0 == *key {
                        vert_idx += 6;
                        i += 1;
                    }
                    pass.draw(batch_start..vert_idx, 0..1);
                } else {
                    i += 1;
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Render watermark text in windows with small/empty buffers.
    pub fn render_window_watermarks(
        &self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
    ) {
        use wgpu::util::DeviceExt;

        if !self.window_watermark_enabled { return; }

        let font_size = glyph_atlas.default_font_size();
        let scale = 3.0_f32;
        let char_width = font_size * 0.6 * scale;
        let char_height = font_size * scale;
        let font_size_bits = 0.0_f32.to_bits();
        let alpha = self.window_watermark_opacity.clamp(0.0, 1.0);

        let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4], f32)> = Vec::new();

        for info in &frame_glyphs.window_infos {
            if info.is_minibuffer { continue; }
            if info.buffer_size > self.window_watermark_threshold as i64 { continue; }

            let b = &info.bounds;
            let content_h = b.height - info.mode_line_height;
            if content_h < char_height * 1.5 { continue; }

            // Determine watermark text: use buffer file name basename, or fallback
            let text = if !info.buffer_file_name.is_empty() {
                let name = info.buffer_file_name.rsplit('/').next()
                    .unwrap_or(&info.buffer_file_name);
                name.to_string()
            } else {
                "empty".to_string()
            };

            // Truncate long names to fit window width
            let max_chars = ((b.width * 0.8) / char_width) as usize;
            let display_text: String = if text.len() > max_chars && max_chars > 3 {
                text.chars().take(max_chars - 2).collect::<String>() + ".."
            } else {
                text.clone()
            };

            let text_width = display_text.chars().count() as f32 * char_width;
            let start_x = b.x + (b.width - text_width) / 2.0;
            let start_y = b.y + (content_h - char_height) / 2.0;

            let color = [1.0, 1.0, 1.0, alpha];

            for (ci, ch) in display_text.chars().enumerate() {
                if ch == ' ' { continue; }
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: 0,
                    font_size_bits,
                };
                glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                overlay_glyphs.push((key, start_x + ci as f32 * char_width, start_y, color, scale));
            }
        }

        if overlay_glyphs.is_empty() { return; }

        // Sort by key for batching
        overlay_glyphs.sort_by(|a, b| {
            a.0.face_id.cmp(&b.0.face_id)
                .then(a.0.font_size_bits.cmp(&b.0.font_size_bits))
                .then(a.0.charcode.cmp(&b.0.charcode))
        });

        let sf = self.scale_factor;
        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(overlay_glyphs.len() * 6);
        let mut valid: Vec<bool> = Vec::with_capacity(overlay_glyphs.len());

        for (key, x, y, color, s) in overlay_glyphs.iter() {
            if let Some(cached) = glyph_atlas.get(key) {
                let gw = cached.width as f32 / sf * s;
                let gh = cached.height as f32 / sf * s;
                let gx = *x + cached.bearing_x / sf * s;
                let gy = *y + (char_height * 0.7) - cached.bearing_y / sf * s;

                vertices.extend_from_slice(&[
                    GlyphVertex { position: [gx, gy], tex_coords: [0.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy], tex_coords: [1.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy + gh], tex_coords: [1.0, 1.0], color: *color },
                    GlyphVertex { position: [gx, gy], tex_coords: [0.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy + gh], tex_coords: [1.0, 1.0], color: *color },
                    GlyphVertex { position: [gx, gy + gh], tex_coords: [0.0, 1.0], color: *color },
                ]);
                valid.push(true);
            } else {
                valid.push(false);
            }
        }

        if vertices.is_empty() { return; }

        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Watermark Glyph Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Watermark Glyph Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Watermark Glyph Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.image_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));

            let mut vert_idx = 0u32;
            let mut i = 0;
            while i < overlay_glyphs.len() {
                if !valid[i] {
                    i += 1;
                    continue;
                }
                let (ref key, _, _, _, _) = overlay_glyphs[i];
                if let Some(cached) = glyph_atlas.get(key) {
                    if cached.is_color {
                        pass.set_pipeline(&self.opaque_image_pipeline);
                    } else {
                        pass.set_pipeline(&self.image_pipeline);
                    }
                    pass.set_bind_group(1, &cached.bind_group, &[]);
                    let batch_start = vert_idx;
                    vert_idx += 6;
                    i += 1;
                    while i < overlay_glyphs.len() && valid[i] && overlay_glyphs[i].0 == *key {
                        vert_idx += 6;
                        i += 1;
                    }
                    pass.draw(batch_start..vert_idx, 0..1);
                } else {
                    i += 1;
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Render a tooltip overlay on top of the scene.
    pub fn render_tooltip(
        &self,
        view: &wgpu::TextureView,
        tooltip: &crate::render_thread::TooltipState,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let (tx, ty, tw, th) = tooltip.bounds;

        // Convert user-specified colors to linear space (surface is sRGB)
        let bg_color = Color::new(tooltip.bg.0, tooltip.bg.1, tooltip.bg.2, 0.95).srgb_to_linear();
        let border_color = Color::new(
            (tooltip.bg.0 * 0.6 + 0.15).min(1.0),
            (tooltip.bg.1 * 0.6 + 0.15).min(1.0),
            (tooltip.bg.2 * 0.6 + 0.15).min(1.0),
            1.0,
        ).srgb_to_linear();
        let text_color = {
            let c = Color::new(tooltip.fg.0, tooltip.fg.1, tooltip.fg.2, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };

        // === Pass 1: Background and border rectangles ===
        let mut rect_vertices: Vec<RectVertex> = Vec::new();

        // Drop shadow (layered for soft edge)
        let shadow_layers = 3;
        for i in 1..=shadow_layers {
            let offset = i as f32 * 1.0;
            let alpha = 0.10 * (1.0 - (i - 1) as f32 / shadow_layers as f32);
            let shadow = Color::new(0.0, 0.0, 0.0, alpha);
            self.add_rect(&mut rect_vertices,
                          tx + offset, ty + offset, tw, th, &shadow);
        }

        // Background
        self.add_rect(&mut rect_vertices, tx, ty, tw, th, &bg_color);

        // Border (1px)
        let bw = 1.0_f32;
        self.add_rect(&mut rect_vertices, tx, ty, tw, bw, &border_color); // top
        self.add_rect(&mut rect_vertices, tx, ty + th - bw, tw, bw, &border_color); // bottom
        self.add_rect(&mut rect_vertices, tx, ty, bw, th, &border_color); // left
        self.add_rect(&mut rect_vertices, tx + tw - bw, ty, bw, th, &border_color); // right

        if !rect_vertices.is_empty() {
            let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Tooltip Rect Buffer"),
                contents: bytemuck::cast_slice(&rect_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tooltip Rect Encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Tooltip Rect Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // === Pass 2: Collect all text glyphs and render batched ===
        let padding = 6.0_f32;
        let line_height = glyph_atlas.default_line_height();
        let char_width = glyph_atlas.default_font_size() * 0.6;
        let font_size_bits = 0.0_f32.to_bits();
        let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();

        for (line_idx, line) in tooltip.lines.iter().enumerate() {
            let ly = ty + padding + line_idx as f32 * line_height;
            for (ci, ch) in line.chars().enumerate() {
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: 0,
                    font_size_bits,
                };
                glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                overlay_glyphs.push((key, tx + padding + (ci as f32) * char_width, ly, text_color));
            }
        }

        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    /// Render a custom title bar overlay for borderless/undecorated windows.
    /// Draws a dark bar at the top with the window title and close/maximize/minimize buttons.
    pub fn render_custom_titlebar(
        &self,
        view: &wgpu::TextureView,
        title: &str,
        titlebar_height: f32,
        hover: u32,
        frame_bg: Option<(f32, f32, f32)>,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let tb_h = titlebar_height;
        let btn_w = 46.0_f32;

        // Derive colors from frame background (already in linear space) or fallback
        let bg_color = if let Some((r, g, b)) = frame_bg {
            // Slightly darken the frame bg for the title bar
            Color::new(r * 0.85, g * 0.85, b * 0.85, 0.95)
        } else {
            Color::new(0.12, 0.12, 0.14, 0.95).srgb_to_linear()
        };
        // Determine if theme is light or dark based on luminance
        let luminance = bg_color.r * 0.299 + bg_color.g * 0.587 + bg_color.b * 0.114;
        let is_light = luminance > 0.3;

        let border_color = if is_light {
            Color::new(bg_color.r * 0.8, bg_color.g * 0.8, bg_color.b * 0.8, 1.0)
        } else {
            Color::new(
                (bg_color.r + 0.05).min(1.0),
                (bg_color.g + 0.05).min(1.0),
                (bg_color.b + 0.05).min(1.0),
                1.0,
            )
        };
        let close_hover_color = Color::new(0.9, 0.2, 0.2, 0.9).srgb_to_linear();
        let btn_hover_color = if is_light {
            Color::new(0.0, 0.0, 0.0, 0.1)
        } else {
            Color::new(1.0, 1.0, 1.0, 0.1)
        };
        let text_color = if is_light {
            let c = Color::new(0.15, 0.15, 0.15, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        } else {
            let c = Color::new(0.8, 0.8, 0.82, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let btn_icon_color = if is_light {
            let c = Color::new(0.3, 0.3, 0.3, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        } else {
            let c = Color::new(0.7, 0.7, 0.72, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let close_icon_hover = {
            let c = Color::new(1.0, 1.0, 1.0, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };

        // === Pass 1: Background and button rectangles ===
        let mut rect_vertices: Vec<RectVertex> = Vec::new();

        // Title bar background
        self.add_rect(&mut rect_vertices, 0.0, 0.0, logical_w, tb_h, &bg_color);

        // Bottom border (1px)
        self.add_rect(&mut rect_vertices, 0.0, tb_h - 1.0, logical_w, 1.0, &border_color);

        // Button positions
        let close_x = logical_w - btn_w;
        let max_x = logical_w - btn_w * 2.0;
        let min_x = logical_w - btn_w * 3.0;

        // Button hover highlights
        // hover: 0=none, 2=close, 3=maximize, 4=minimize
        if hover == 2 {
            self.add_rect(&mut rect_vertices, close_x, 0.0, btn_w, tb_h, &close_hover_color);
        } else if hover == 3 {
            self.add_rect(&mut rect_vertices, max_x, 0.0, btn_w, tb_h, &btn_hover_color);
        } else if hover == 4 {
            self.add_rect(&mut rect_vertices, min_x, 0.0, btn_w, tb_h, &btn_hover_color);
        }

        // Subtle button separator lines
        let sep_color = Color::new(0.2, 0.2, 0.22, 0.5).srgb_to_linear();
        self.add_rect(&mut rect_vertices, close_x, 4.0, 1.0, tb_h - 8.0, &sep_color);
        self.add_rect(&mut rect_vertices, max_x, 4.0, 1.0, tb_h - 8.0, &sep_color);
        self.add_rect(&mut rect_vertices, min_x, 4.0, 1.0, tb_h - 8.0, &sep_color);

        // Render rect pass
        if !rect_vertices.is_empty() {
            let rect_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Titlebar Rect Buffer"),
                        contents: bytemuck::cast_slice(&rect_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });

            let mut encoder =
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Titlebar Rect Encoder"),
                    });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Titlebar Rect Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // === Pass 2: Title text and button icons ===
        let font_size = glyph_atlas.default_font_size();
        let char_width = font_size * 0.6;
        let font_size_bits = 0.0_f32.to_bits();
        let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();

        // Center title text
        let title_pixel_width = title.chars().count() as f32 * char_width;
        let title_x = (logical_w - title_pixel_width) / 2.0;
        let title_y = (tb_h - font_size) / 2.0;

        for (ci, ch) in title.chars().enumerate() {
            let key = GlyphKey {
                charcode: ch as u32,
                face_id: 0,
                font_size_bits,
            };
            glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
            overlay_glyphs.push((key, title_x + ci as f32 * char_width, title_y, text_color));
        }

        // Button icons: minimize (─), maximize (□), close (×)
        let btn_center_y = (tb_h - font_size) / 2.0;
        let min_color = if hover == 4 { text_color } else { btn_icon_color };
        let max_color = if hover == 3 { text_color } else { btn_icon_color };
        let close_color = if hover == 2 { close_icon_hover } else { btn_icon_color };

        // Minimize: ─ (U+2500)
        let min_icon_x = min_x + (btn_w - char_width) / 2.0;
        let min_key = GlyphKey { charcode: 0x2500, face_id: 0, font_size_bits };
        glyph_atlas.get_or_create(&self.device, &self.queue, &min_key, None);
        overlay_glyphs.push((min_key, min_icon_x, btn_center_y, min_color));

        // Maximize: □ (U+25A1)
        let max_icon_x = max_x + (btn_w - char_width) / 2.0;
        let max_key = GlyphKey { charcode: 0x25A1, face_id: 0, font_size_bits };
        glyph_atlas.get_or_create(&self.device, &self.queue, &max_key, None);
        overlay_glyphs.push((max_key, max_icon_x, btn_center_y, max_color));

        // Close: × (U+00D7)
        let close_icon_x = close_x + (btn_w - char_width) / 2.0;
        let close_key = GlyphKey { charcode: 0x00D7, face_id: 0, font_size_bits };
        glyph_atlas.get_or_create(&self.device, &self.queue, &close_key, None);
        overlay_glyphs.push((close_key, close_icon_x, btn_center_y, close_color));

        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    /// Render thin scroll position indicators on the right edge of each window.
    pub fn render_scroll_indicators(
        &self,
        view: &wgpu::TextureView,
        window_infos: &[crate::core::frame_glyphs::WindowInfo],
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        let indicator_width = 3.0_f32;
        let multi_window = window_infos.len() > 1;

        for info in window_infos {
            // Focus ring for selected window (only when multiple windows visible)
            if multi_window && info.selected {
                let b = &info.bounds;
                let bw = 2.0_f32;
                let accent = Color::new(0.3, 0.5, 0.9, 0.4).srgb_to_linear();
                // Top
                self.add_rect(&mut rect_vertices, b.x, b.y, b.width, bw, &accent);
                // Bottom (above mode-line)
                let bottom_y = b.y + b.height - info.mode_line_height - bw;
                self.add_rect(&mut rect_vertices, b.x, bottom_y, b.width, bw, &accent);
                // Left
                self.add_rect(&mut rect_vertices, b.x, b.y, bw, b.height - info.mode_line_height, &accent);
                // Right
                self.add_rect(&mut rect_vertices, b.x + b.width - bw, b.y, bw, b.height - info.mode_line_height, &accent);
            }

            // Skip windows with no meaningful buffer content for scroll indicator
            if info.buffer_size <= 1 {
                continue;
            }

            let b = &info.bounds;
            // Content area height (exclude mode-line)
            let content_h = b.height - info.mode_line_height;
            if content_h < 20.0 {
                continue;
            }

            // Scroll ratio: what fraction of the buffer is before window_start
            let start_ratio = (info.window_start as f32 - 1.0).max(0.0)
                / (info.buffer_size as f32 - 1.0).max(1.0);

            // Viewport ratio: what fraction of the buffer is visible
            let visible_chars = if info.window_end > 0 {
                (info.window_end - info.window_start).max(1) as f32
            } else {
                // Estimate: content_h worth of text
                content_h * 2.0 // rough chars estimate
            };
            let viewport_ratio = (visible_chars / info.buffer_size as f32).clamp(0.02, 1.0);

            // Indicator bar position and size
            let bar_h = (content_h * viewport_ratio).max(8.0).min(content_h);
            let bar_y = b.y + start_ratio * (content_h - bar_h);

            // Semi-transparent indicator color
            let color = Color::new(0.5, 0.5, 0.5, 0.25).srgb_to_linear();
            let x = b.x + b.width - indicator_width;

            self.add_rect(&mut rect_vertices, x, bar_y, indicator_width, bar_h, &color);
        }

        if rect_vertices.is_empty() {
            return;
        }

        let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scroll Indicator Buffer"),
            contents: bytemuck::cast_slice(&rect_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Scroll Indicator Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scroll Indicator Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.rect_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, rect_buffer.slice(..));
            pass.draw(0..rect_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Render IME preedit text at the cursor position with underline.
    pub fn render_ime_preedit(
        &self,
        view: &wgpu::TextureView,
        preedit_text: &str,
        cursor_x: f32,
        cursor_y: f32,
        cursor_height: f32,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        if preedit_text.is_empty() {
            return;
        }

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let char_width = glyph_atlas.default_font_size() * 0.6;
        let font_size_bits = 0.0_f32.to_bits();
        let text_len = preedit_text.chars().count();
        let preedit_width = text_len as f32 * char_width;

        // Background and underline rects
        let bg_color = Color::new(0.15, 0.15, 0.2, 0.95).srgb_to_linear();
        let underline_color = Color::new(0.4, 0.6, 1.0, 1.0).srgb_to_linear();

        let px = cursor_x;
        let py = cursor_y;
        let pw = preedit_width + 4.0;
        let ph = cursor_height;

        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        // Background
        self.add_rect(&mut rect_vertices, px, py, pw, ph, &bg_color);
        // Underline (2px at bottom)
        self.add_rect(&mut rect_vertices, px, py + ph - 2.0, pw, 2.0, &underline_color);

        if !rect_vertices.is_empty() {
            let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("IME Preedit Rect Buffer"),
                contents: bytemuck::cast_slice(&rect_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("IME Preedit Rect Encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("IME Preedit Rect Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // Text glyphs
        let text_color = {
            let c = Color::new(1.0, 1.0, 1.0, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();
        for (ci, ch) in preedit_text.chars().enumerate() {
            let key = GlyphKey {
                charcode: ch as u32,
                face_id: 0,
                font_size_bits,
            };
            glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
            overlay_glyphs.push((key, px + 2.0 + (ci as f32) * char_width, py, text_color));
        }
        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    /// Render a visual bell flash overlay (semi-transparent white rectangle fading out).
    /// Render an FPS counter overlay in the top-right corner.
    /// Render a corner mask to clip the window to a rounded rectangle.
    /// Uses dst = dst * src_alpha blend mode to zero out pixels outside
    /// the rounded rect shape.
    pub fn render_corner_mask(
        &self,
        view: &wgpu::TextureView,
        corner_radius: f32,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Filled rounded rect covering the whole frame with alpha=1 inside, 0 outside.
        // border_width=0 triggers filled mode in the shader.
        let mut vertices: Vec<RoundedRectVertex> = Vec::new();
        self.add_rounded_rect(
            &mut vertices,
            0.0, 0.0, logical_w, logical_h,
            0.0,            // border_width=0 → filled mode
            corner_radius,
            &Color::new(1.0, 1.0, 1.0, 1.0), // white, alpha=1
        );

        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Corner Mask Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Corner Mask Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Corner Mask Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.corner_mask_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Build breadcrumb display chars from a file path
    /// Map a file extension to a distinct HSL-based accent color (linear RGB)
    fn extension_to_color(ext: &str) -> (f32, f32, f32) {
        // Well-known extensions get specific colors
        match ext {
            "rs" => (0.8, 0.3, 0.1),  // Rust - orange
            "el" | "lisp" | "scm" => (0.6, 0.2, 0.8),  // Lisp - purple
            "c" | "h" => (0.2, 0.5, 0.8),  // C - blue
            "cpp" | "cc" | "hpp" => (0.2, 0.4, 0.7),  // C++ - darker blue
            "py" => (0.2, 0.6, 0.2),  // Python - green
            "js" | "jsx" => (0.9, 0.8, 0.2),  // JavaScript - yellow
            "ts" | "tsx" => (0.2, 0.5, 0.9),  // TypeScript - blue
            "rb" => (0.8, 0.2, 0.2),  // Ruby - red
            "go" => (0.0, 0.6, 0.7),  // Go - teal
            "java" => (0.7, 0.3, 0.1),  // Java - brown-orange
            "html" | "htm" => (0.9, 0.3, 0.2),  // HTML - red-orange
            "css" | "scss" => (0.2, 0.4, 0.9),  // CSS - blue
            "json" | "yaml" | "yml" | "toml" => (0.5, 0.5, 0.5),  // Config - gray
            "md" | "org" | "txt" => (0.4, 0.7, 0.4),  // Text - green
            "sh" | "bash" | "zsh" => (0.3, 0.7, 0.3),  // Shell - green
            _ => {
                // Hash-based color for unknown extensions
                let mut hash: u32 = 5381;
                for byte in ext.bytes() {
                    hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
                }
                let hue = (hash % 360) as f32 / 360.0;
                // Simple HSL to RGB (saturation=0.6, lightness=0.5)
                let s = 0.6_f32;
                let l = 0.5_f32;
                let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
                let x = c * (1.0 - ((hue * 6.0) % 2.0 - 1.0).abs());
                let m = l - c / 2.0;
                let (r, g, b) = match (hue * 6.0) as i32 {
                    0 => (c, x, 0.0),
                    1 => (x, c, 0.0),
                    2 => (0.0, c, x),
                    3 => (0.0, x, c),
                    4 => (x, 0.0, c),
                    _ => (c, 0.0, x),
                };
                (r + m, g + m, b + m)
            }
        }
    }

    fn breadcrumb_display_chars(path: &str) -> Vec<(char, bool)> {
        let separator = " \u{203A} "; // " › "
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if components.is_empty() {
            return Vec::new();
        }
        let show_start = if components.len() > 3 { components.len() - 3 } else { 0 };
        let shown = &components[show_start..];
        let mut display_chars: Vec<(char, bool)> = Vec::new();
        if show_start > 0 {
            display_chars.push(('\u{2026}', true));
            for c in separator.chars() {
                display_chars.push((c, true));
            }
        }
        for (i, comp) in shown.iter().enumerate() {
            if i > 0 {
                for c in separator.chars() {
                    display_chars.push((c, true));
                }
            }
            let is_last = i == shown.len() - 1;
            for c in comp.chars() {
                display_chars.push((c, !is_last));
            }
        }
        display_chars
    }

    /// Render breadcrumb/path bars for windows with file-backed buffers
    pub fn render_breadcrumbs(
        &mut self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
    ) {
        use wgpu::util::DeviceExt;

        if !self.breadcrumb_enabled {
            return;
        }

        let char_width = glyph_atlas.default_font_size() * 0.6;
        let line_height = glyph_atlas.default_line_height();
        let bar_height = line_height + 4.0;
        let padding_x = 6.0_f32;
        let opacity = self.breadcrumb_opacity.clamp(0.0, 1.0);

        // Detect title changes and start fade animations
        if self.title_fade_enabled {
            for info in &frame_glyphs.window_infos {
                if info.is_minibuffer || info.buffer_file_name.is_empty() {
                    continue;
                }
                let wid = info.window_id;
                let new_text = &info.buffer_file_name;
                let changed = match self.prev_breadcrumb_text.get(&wid) {
                    Some(old) => old != new_text,
                    None => false, // first time seeing this window, no fade
                };
                if changed {
                    let old_text = self.prev_breadcrumb_text.get(&wid).cloned().unwrap_or_default();
                    // Remove any existing fade for this window
                    self.active_title_fades.retain(|f| f.window_id != wid);
                    self.active_title_fades.push(TitleFadeEntry {
                        window_id: wid,
                        bounds: info.bounds,
                        old_text,
                        new_text: new_text.clone(),
                        started: std::time::Instant::now(),
                        duration: std::time::Duration::from_millis(self.title_fade_duration_ms as u64),
                    });
                }
                self.prev_breadcrumb_text.insert(wid, new_text.clone());
            }
            // Clean up expired fades
            self.active_title_fades.retain(|f| f.started.elapsed() < f.duration);
            if !self.active_title_fades.is_empty() {
                self.needs_continuous_redraw = true;
            }
        }

        let mut all_rect_vertices: Vec<RectVertex> = Vec::new();
        let mut all_text_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();
        let font_size_bits = 0.0_f32.to_bits();
        let text_color_base = [0.85_f32, 0.85, 0.85, 1.0];
        let sep_color_base = [0.5_f32, 0.5, 0.5, 1.0];

        for info in &frame_glyphs.window_infos {
            if info.is_minibuffer || info.buffer_file_name.is_empty() {
                continue;
            }

            let b = &info.bounds;

            // Check if this window has an active title fade
            let active_fade = self.active_title_fades.iter().find(|f| f.window_id == info.window_id);

            if let Some(fade) = active_fade {
                // Crossfade: render old text fading out, new text fading in
                let t = (fade.started.elapsed().as_secs_f32() / fade.duration.as_secs_f32()).min(1.0);
                // Ease-out quadratic
                let eased = t * (2.0 - t);
                let new_alpha = eased;
                let old_alpha = 1.0 - eased;

                // Background rect (full opacity)
                let display_chars_new = Self::breadcrumb_display_chars(&info.buffer_file_name);
                let display_chars_old = Self::breadcrumb_display_chars(&fade.old_text);
                let max_len = display_chars_new.len().max(display_chars_old.len());
                let bar_w = (max_len as f32 * char_width + padding_x * 2.0).min(b.width);
                let bar_x = b.x;
                let bar_y = b.y;

                let bg_color = Color::new(0.0, 0.0, 0.0, opacity);
                self.add_rect(&mut all_rect_vertices, bar_x, bar_y, bar_w, bar_height, &bg_color);
                let edge_color = Color::new(0.3, 0.3, 0.3, opacity * 0.5);
                self.add_rect(&mut all_rect_vertices, bar_x, bar_y + bar_height, bar_w, 1.0, &edge_color);

                let text_y = bar_y + 2.0;

                // Old text fading out
                for (ci, &(ch, is_dim)) in display_chars_old.iter().enumerate() {
                    let cx = bar_x + padding_x + ci as f32 * char_width;
                    if cx + char_width > bar_x + bar_w { break; }
                    let key = GlyphKey { charcode: ch as u32, face_id: 0, font_size_bits };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    let base = if is_dim { sep_color_base } else { text_color_base };
                    all_text_glyphs.push((key, cx, text_y,
                        [base[0], base[1], base[2], base[3] * old_alpha]));
                }

                // New text fading in
                for (ci, &(ch, is_dim)) in display_chars_new.iter().enumerate() {
                    let cx = bar_x + padding_x + ci as f32 * char_width;
                    if cx + char_width > bar_x + bar_w { break; }
                    let key = GlyphKey { charcode: ch as u32, face_id: 0, font_size_bits };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    let base = if is_dim { sep_color_base } else { text_color_base };
                    all_text_glyphs.push((key, cx, text_y,
                        [base[0], base[1], base[2], base[3] * new_alpha]));
                }
            } else {
                // Normal rendering (no active fade)
                let display_chars = Self::breadcrumb_display_chars(&info.buffer_file_name);
                if display_chars.is_empty() { continue; }

                let text_width = display_chars.len() as f32 * char_width;
                let bar_w = (text_width + padding_x * 2.0).min(b.width);
                let bar_x = b.x;
                let bar_y = b.y;

                let bg_color = Color::new(0.0, 0.0, 0.0, opacity);
                self.add_rect(&mut all_rect_vertices, bar_x, bar_y, bar_w, bar_height, &bg_color);
                let edge_color = Color::new(0.3, 0.3, 0.3, opacity * 0.5);
                self.add_rect(&mut all_rect_vertices, bar_x, bar_y + bar_height, bar_w, 1.0, &edge_color);

                let text_y = bar_y + 2.0;
                for (ci, &(ch, is_dim)) in display_chars.iter().enumerate() {
                    let cx = bar_x + padding_x + ci as f32 * char_width;
                    if cx + char_width > bar_x + bar_w { break; }
                    let key = GlyphKey { charcode: ch as u32, face_id: 0, font_size_bits };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    all_text_glyphs.push((key, cx, text_y,
                        if is_dim { sep_color_base } else { text_color_base }));
                }
            }
        }

        // Draw background rects
        if !all_rect_vertices.is_empty() {
            let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Breadcrumb Rect Buffer"),
                contents: bytemuck::cast_slice(&all_rect_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Breadcrumb Rect Encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Breadcrumb Rect Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
                pass.draw(0..all_rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // Draw text glyphs
        if !all_text_glyphs.is_empty() {
            self.render_overlay_glyphs(view, &mut all_text_glyphs, glyph_atlas);
        }
    }

    /// Render typing speed (WPM) indicator in the bottom-right of the selected window
    pub fn render_typing_speed(
        &self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        wpm: f32,
    ) {
        use wgpu::util::DeviceExt;

        // Find the selected window (non-minibuffer)
        let selected = frame_glyphs.window_infos.iter().find(|w| w.selected && !w.is_minibuffer);
        let info = match selected {
            Some(i) => i,
            None => return,
        };

        let wpm_int = wpm.round() as u32;
        let label = format!("{} WPM", wpm_int);

        let char_width = glyph_atlas.default_font_size() * 0.6;
        let line_height = glyph_atlas.default_line_height();
        let padding_x = 8.0_f32;
        let padding_y = 2.0_f32;
        let bar_w = label.len() as f32 * char_width + padding_x * 2.0;
        let bar_h = line_height + padding_y * 2.0;
        let b = &info.bounds;
        let bar_x = b.x + b.width - bar_w - 4.0;
        // Place just above the mode-line
        let bar_y = b.y + b.height - info.mode_line_height - bar_h - 2.0;

        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        let bg_color = Color::new(0.0, 0.0, 0.0, 0.6);
        self.add_rect(&mut rect_vertices, bar_x, bar_y, bar_w, bar_h, &bg_color);

        // Color the label based on WPM: gray→green→yellow→red
        let text_color = if wpm_int == 0 {
            [0.5, 0.5, 0.5, 0.8]
        } else if wpm_int < 40 {
            [0.4, 0.8, 0.4, 1.0] // green
        } else if wpm_int < 80 {
            [0.8, 0.8, 0.2, 1.0] // yellow
        } else {
            [1.0, 0.4, 0.2, 1.0] // orange-red
        };

        let font_size_bits = 0.0_f32.to_bits();
        let mut text_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();
        let text_y = bar_y + padding_y;
        for (ci, ch) in label.chars().enumerate() {
            let cx = bar_x + padding_x + ci as f32 * char_width;
            let key = GlyphKey {
                charcode: ch as u32,
                face_id: 0,
                font_size_bits,
            };
            glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
            text_glyphs.push((key, cx, text_y, text_color));
        }

        // Draw background
        if !rect_vertices.is_empty() {
            let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Typing Speed Rect Buffer"),
                contents: bytemuck::cast_slice(&rect_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Typing Speed Rect Encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Typing Speed Rect Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // Draw text
        if !text_glyphs.is_empty() {
            self.render_overlay_glyphs(view, &mut text_glyphs, glyph_atlas);
        }
    }

    pub fn render_fps_overlay(
        &self,
        view: &wgpu::TextureView,
        lines: &[String],
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        if lines.is_empty() {
            return;
        }

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let char_width = glyph_atlas.default_font_size() * 0.6;
        let line_height = glyph_atlas.default_line_height();
        let padding = 4.0_f32;
        let line_spacing = 2.0_f32;

        // Badge size: width = longest line, height = all lines
        let max_text_w = lines.iter()
            .map(|l| l.len() as f32 * char_width)
            .fold(0.0_f32, f32::max);
        let num_lines = lines.len() as f32;
        let badge_w = max_text_w + padding * 2.0;
        let badge_h = num_lines * line_height + (num_lines - 1.0) * line_spacing + padding * 2.0;
        let badge_x = logical_w - badge_w - 4.0;
        let badge_y = 4.0;

        // Background badge (semi-transparent dark)
        let bg = Color::new(0.0, 0.0, 0.0, 0.6);
        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        self.add_rect(&mut rect_vertices, badge_x, badge_y, badge_w, badge_h, &bg);

        let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("FPS Rect Buffer"),
            contents: bytemuck::cast_slice(&rect_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("FPS Rect Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("FPS Rect Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.rect_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, rect_buffer.slice(..));
            pass.draw(0..rect_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));

        // Text glyphs (green for good visibility)
        let text_color = [0.0_f32, 1.0, 0.0, 1.0]; // green in linear
        let font_size_bits = 0.0_f32.to_bits();
        let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();
        for (li, line) in lines.iter().enumerate() {
            let y = badge_y + padding + li as f32 * (line_height + line_spacing);
            for (ci, ch) in line.chars().enumerate() {
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: 0,
                    font_size_bits,
                };
                glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                overlay_glyphs.push((
                    key,
                    badge_x + padding + ci as f32 * char_width,
                    y,
                    text_color,
                ));
            }
        }
        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    pub fn render_visual_bell(
        &self,
        view: &wgpu::TextureView,
        surface_width: u32,
        surface_height: u32,
        alpha: f32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Semi-transparent white overlay in linear space
        let flash_color = Color::new(1.0, 1.0, 1.0, alpha).srgb_to_linear();

        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        self.add_rect(&mut rect_vertices, 0.0, 0.0, logical_w, logical_h, &flash_color);

        let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Visual Bell Buffer"),
            contents: bytemuck::cast_slice(&rect_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Visual Bell Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Visual Bell Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.rect_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, rect_buffer.slice(..));
            pass.draw(0..rect_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }
}
