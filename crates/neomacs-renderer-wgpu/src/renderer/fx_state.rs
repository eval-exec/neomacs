//! Grouped per-effect animation state shared by `WgpuRenderer` and
//! `RendererFrameEffects`.
//!
//! Every effect family gets one struct holding exactly the state that
//! `take_frame_effects`/`apply_frame_effects` transfer between the renderer
//! and a frame's `RendererFrameEffects`. The families are collected into
//! [`EffectsState`], stored as a single field on both sides, so the transfer
//! is a whole-struct move instead of a per-field copy list.
//!
//! The seven free-running animation clocks in [`EffectClocks`] are the one
//! exception: the renderer always holds valid `EventTime`s while a fresh
//! (default) `RendererFrameEffects` holds `None`, which on apply preserves
//! the renderer's current clocks. Fields deliberately NOT transferred
//! (`aurora_start`, `render_start_time`, and the effect duration settings)
//! live directly on `WgpuRenderer`, outside these structs.

use neomacs_display_protocol::frame_time::{EventTime, observe_platform_now};
use neomacs_display_protocol::types::Rect;

/// Entry for an active scroll momentum indicator
pub(crate) struct ScrollMomentumEntry {
    pub(crate) window_id: i64,
    pub(crate) bounds: Rect,
    pub(crate) direction: i32, // 1 = down, -1 = up
    pub(crate) started: EventTime,
    pub(crate) duration: std::time::Duration,
}

/// Entry for matrix rain column
pub(crate) struct MatrixColumn {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) speed: f32,
    pub(crate) length: f32,
}

/// Entry for cursor ghost afterimage
pub(crate) struct CursorGhostEntry {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) started: EventTime,
}

/// Entry for cursor sonar ping
pub(crate) struct SonarPingEntry {
    pub(crate) cx: f32,
    pub(crate) cy: f32,
    pub(crate) started: EventTime,
    pub(crate) duration: std::time::Duration,
}

pub(crate) struct SparkleBurstEntry {
    pub(crate) cx: f32,
    pub(crate) cy: f32,
    pub(crate) started: EventTime,
    /// Random seed for particle directions
    pub(crate) seed: u32,
}

/// Entry for window edge glow (scroll boundary indicator)
pub(crate) struct EdgeGlowEntry {
    pub(crate) window_id: i64,
    pub(crate) bounds: Rect,
    pub(crate) at_top: bool,
    pub(crate) started: EventTime,
    pub(crate) duration: std::time::Duration,
}

/// Entry for rain drop
pub(crate) struct RainDrop {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) speed: f32,
    pub(crate) length: f32,
    pub(crate) opacity: f32,
}

/// Entry for cursor ripple wave
pub(crate) struct RippleWaveEntry {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) started: EventTime,
    pub(crate) duration: std::time::Duration,
}

/// Entry for cursor particle effect
pub(crate) struct CursorParticle {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) vx: f32,
    pub(crate) vy: f32,
    pub(crate) started: EventTime,
    pub(crate) lifetime: std::time::Duration,
}

/// Entry for typing heat map (records where cursor was during edits)
pub(crate) struct HeatMapEntry {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) started: EventTime,
}

/// Entry for window edge snap indicator
pub(crate) struct EdgeSnapEntry {
    pub(crate) bounds: Rect,
    pub(crate) mode_line_height: f32,
    pub(crate) at_top: bool,
    pub(crate) at_bottom: bool,
    pub(crate) started: EventTime,
    pub(crate) duration: std::time::Duration,
}

/// Entry for click halo effect
pub(crate) struct ClickHaloEntry {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) started: EventTime,
    pub(crate) duration: std::time::Duration,
}

/// Entry for scroll velocity fade overlay
pub(crate) struct ScrollVelocityFadeEntry {
    pub(crate) window_id: i64,
    pub(crate) bounds: Rect,
    /// Scroll delta magnitude (characters scrolled)
    pub(crate) velocity: f32,
    pub(crate) started: EventTime,
    pub(crate) duration: std::time::Duration,
}

/// Entry for an active window switch highlight fade
pub(crate) struct WindowFadeEntry {
    pub(crate) window_id: i64,
    pub(crate) bounds: Rect,
    pub(crate) started: EventTime,
    pub(crate) duration: std::time::Duration,
    pub(crate) intensity: f32,
}

/// Entry for an active title/breadcrumb crossfade animation
pub(crate) struct TitleFadeEntry {
    pub(crate) window_id: i64,
    #[allow(dead_code)]
    pub(crate) bounds: Rect,
    pub(crate) old_text: String,
    #[allow(dead_code)]
    pub(crate) new_text: String,
    pub(crate) started: EventTime,
    pub(crate) duration: std::time::Duration,
}

/// Entry for an active line insertion/deletion animation
pub(crate) struct LineAnimEntry {
    /// Window bounds where the animation is active
    pub(crate) window_bounds: Rect,
    /// Y position below which glyphs are displaced
    pub(crate) edit_y: f32,
    /// Initial Y offset (negative=insertion slide-down, positive=deletion slide-up)
    pub(crate) initial_offset: f32,
    /// When the animation started
    pub(crate) started: EventTime,
    /// Duration of the animation
    pub(crate) duration: std::time::Duration,
}

/// Entry for an active mode-line content transition
pub(crate) struct ModeLineFadeEntry {
    pub(crate) window_id: i64,
    /// Mode-line area (y, height) within the window
    pub(crate) mode_line_y: f32,
    pub(crate) mode_line_h: f32,
    pub(crate) bounds_x: f32,
    pub(crate) bounds_w: f32,
    pub(crate) started: EventTime,
    pub(crate) duration: std::time::Duration,
}

/// Entry for an active text fade-in animation
pub(crate) struct TextFadeEntry {
    pub(crate) window_id: i64,
    pub(crate) bounds: Rect,
    pub(crate) started: EventTime,
    pub(crate) duration: std::time::Duration,
}

/// Entry for an active scroll line spacing animation
pub(crate) struct ScrollSpacingEntry {
    pub(crate) window_id: i64,
    pub(crate) bounds: Rect,
    /// +1 = scroll down (content moves up), -1 = scroll up
    pub(crate) direction: i32,
    pub(crate) started: EventTime,
    pub(crate) duration: std::time::Duration,
}

/// Per-window dimming state.
#[derive(Default)]
pub(crate) struct WindowDimState {
    /// Per-window dim opacity for smooth fade transitions
    pub(crate) per_window: std::collections::HashMap<i64, f32>,
    /// Idle screen dimming alpha (0.0 = no dim, >0 = overlay)
    pub(crate) idle_alpha: f32,
}

/// Typing ripple state: active ripples as (center_x, center_y, spawn_instant).
#[derive(Default)]
pub(crate) struct TypingRippleState {
    pub(crate) active: Vec<(f32, f32, EventTime)>,
}

/// Line insertion/deletion slide animations.
#[derive(Default)]
pub(crate) struct LineAnimState {
    pub(crate) active: Vec<LineAnimEntry>,
}

/// Window switch highlight fades.
#[derive(Default)]
pub(crate) struct WindowFadeState {
    pub(crate) active: Vec<WindowFadeEntry>,
}

/// Title/breadcrumb crossfades and previous text for change detection.
#[derive(Default)]
pub(crate) struct TitleFadeState {
    pub(crate) active: Vec<TitleFadeEntry>,
    /// Previous breadcrumb text per window (window_id -> file_name)
    pub(crate) prev_breadcrumb_text: std::collections::HashMap<i64, String>,
}

/// Active/inactive window border transitions.
#[derive(Default)]
pub(crate) struct BorderTransitionState {
    /// Per-window border transition state: (window_id, is_becoming_active, start_time)
    pub(crate) transitions: Vec<(i64, bool, EventTime)>,
    /// Previous selected window for border transition detection
    pub(crate) prev_selected: i64,
}

/// Mode-line content transition fades and content hashes for change detection.
#[derive(Default)]
pub(crate) struct ModeLineFadeState {
    pub(crate) active: Vec<ModeLineFadeEntry>,
    /// Per-window mode-line content hash for change detection
    pub(crate) prev_hashes: std::collections::HashMap<i64, u64>,
}

/// Text fade-in animations per window.
#[derive(Default)]
pub(crate) struct TextFadeState {
    pub(crate) active: Vec<TextFadeEntry>,
}

/// Scroll line spacing (accordion) animations.
#[derive(Default)]
pub(crate) struct ScrollSpacingState {
    pub(crate) active: Vec<ScrollSpacingEntry>,
}

/// Cursor trail state: recent positions as (x, y, w, h, instant).
#[derive(Default)]
pub(crate) struct CursorTrailState {
    pub(crate) positions: Vec<(f32, f32, f32, f32, EventTime)>,
    pub(crate) last_pos: (f32, f32),
}

/// Noise grain overlay frame counter.
#[derive(Default)]
pub(crate) struct NoiseGrainState {
    pub(crate) frame: u32,
}

/// Cursor wake animation trigger time.
#[derive(Default)]
pub(crate) struct CursorWakeState {
    pub(crate) started: Option<EventTime>,
}

/// Cursor magnetism entries as (x, y, time).
#[derive(Default)]
pub(crate) struct CursorMagnetismState {
    pub(crate) entries: Vec<(f32, f32, EventTime)>,
}

/// Cursor comet positions as (x, y, w, h, time).
#[derive(Default)]
pub(crate) struct CursorCometState {
    pub(crate) positions: Vec<(f32, f32, f32, f32, EventTime)>,
}

/// Cursor particle system state.
#[derive(Default)]
pub(crate) struct CursorParticlesState {
    pub(crate) entries: Vec<CursorParticle>,
    pub(crate) prev_pos: Option<(f32, f32)>,
}

/// Typing heat map state.
#[derive(Default)]
pub(crate) struct TypingHeatmapState {
    pub(crate) entries: Vec<HeatMapEntry>,
    pub(crate) prev_cursor: Option<(f32, f32)>,
}

/// Scroll velocity fade overlays.
#[derive(Default)]
pub(crate) struct ScrollVelocityState {
    pub(crate) fades: Vec<ScrollVelocityFadeEntry>,
}

/// Resize padding animation trigger time.
#[derive(Default)]
pub(crate) struct ResizePaddingState {
    pub(crate) started: Option<EventTime>,
}

/// Scroll momentum indicators.
#[derive(Default)]
pub(crate) struct ScrollMomentumState {
    pub(crate) active: Vec<ScrollMomentumEntry>,
}

/// Matrix rain columns.
#[derive(Default)]
pub(crate) struct MatrixRainState {
    pub(crate) columns: Vec<MatrixColumn>,
}

/// Cursor ghost afterimages.
#[derive(Default)]
pub(crate) struct CursorGhostState {
    pub(crate) entries: Vec<CursorGhostEntry>,
}

/// Cursor sonar pings.
#[derive(Default)]
pub(crate) struct SonarPingState {
    pub(crate) entries: Vec<SonarPingEntry>,
}

/// Lightning bolt effect: segments as (x1, y1, x2, y2) plus bolt age.
/// The regeneration clock lives in [`EffectClocks::lightning_bolt_last`].
#[derive(Default)]
pub(crate) struct LightningBoltState {
    pub(crate) segments: Vec<(f32, f32, f32, f32)>,
    pub(crate) age: f32,
}

/// Cursor pendulum swing state.
#[derive(Default)]
pub(crate) struct CursorPendulumState {
    pub(crate) last_x: f32,
    pub(crate) last_y: f32,
    pub(crate) swing_start: Option<EventTime>,
}

/// Cursor sparkle bursts.
#[derive(Default)]
pub(crate) struct SparkleBurstState {
    pub(crate) entries: Vec<SparkleBurstEntry>,
}

/// Cursor metronome tick state.
#[derive(Default)]
pub(crate) struct CursorMetronomeState {
    pub(crate) last_x: f32,
    pub(crate) last_y: f32,
    pub(crate) tick_start: Option<EventTime>,
}

/// Cursor ripple ring state.
#[derive(Default)]
pub(crate) struct RippleRingState {
    pub(crate) start: Option<EventTime>,
    pub(crate) last_x: f32,
    pub(crate) last_y: f32,
}

/// Cursor shockwave state.
#[derive(Default)]
pub(crate) struct ShockwaveState {
    pub(crate) start: Option<EventTime>,
    pub(crate) last_x: f32,
    pub(crate) last_y: f32,
}

/// Cursor bubble state.
#[derive(Default)]
pub(crate) struct BubbleState {
    pub(crate) spawn_time: Option<EventTime>,
    pub(crate) last_x: f32,
    pub(crate) last_y: f32,
}

/// Cursor firework state.
#[derive(Default)]
pub(crate) struct FireworkState {
    pub(crate) start: Option<EventTime>,
    pub(crate) last_x: f32,
    pub(crate) last_y: f32,
}

/// Cursor lightning strike state.
#[derive(Default)]
pub(crate) struct CursorLightningState {
    pub(crate) start: Option<EventTime>,
    pub(crate) last_x: f32,
    pub(crate) last_y: f32,
}

/// Cursor snowflake state.
#[derive(Default)]
pub(crate) struct SnowflakeState {
    pub(crate) start: Option<EventTime>,
    pub(crate) last_x: f32,
    pub(crate) last_y: f32,
}

/// Window edge glows (scroll boundary indicators).
#[derive(Default)]
pub(crate) struct EdgeGlowState {
    pub(crate) entries: Vec<EdgeGlowEntry>,
}

/// Ambient rain drops. The spawn clock lives in
/// [`EffectClocks::rain_last_spawn`].
#[derive(Default)]
pub(crate) struct RainState {
    pub(crate) drops: Vec<RainDrop>,
}

/// Cursor ripple waves.
#[derive(Default)]
pub(crate) struct RippleWaveState {
    pub(crate) waves: Vec<RippleWaveEntry>,
}

/// Click halos.
#[derive(Default)]
pub(crate) struct ClickHaloState {
    pub(crate) halos: Vec<ClickHaloEntry>,
}

/// Window edge snap indicators.
#[derive(Default)]
pub(crate) struct EdgeSnapState {
    pub(crate) snaps: Vec<EdgeSnapEntry>,
}

/// Cursor error pulse trigger time.
#[derive(Default)]
pub(crate) struct ErrorPulseState {
    pub(crate) started: Option<EventTime>,
}

/// All per-effect animation state transferred between the renderer and a
/// frame's `RendererFrameEffects` as one unit.
#[derive(Default)]
pub(crate) struct EffectsState {
    /// Whether any fancy (animated) border styles are present in the current frame
    pub(crate) has_animated_borders: bool,
    pub(crate) dim: WindowDimState,
    pub(crate) typing_ripple: TypingRippleState,
    pub(crate) line_anim: LineAnimState,
    pub(crate) window_fade: WindowFadeState,
    pub(crate) title_fade: TitleFadeState,
    pub(crate) border_transition: BorderTransitionState,
    pub(crate) mode_line_fade: ModeLineFadeState,
    pub(crate) text_fade: TextFadeState,
    pub(crate) scroll_spacing: ScrollSpacingState,
    pub(crate) cursor_trail: CursorTrailState,
    pub(crate) noise_grain: NoiseGrainState,
    pub(crate) cursor_wake: CursorWakeState,
    pub(crate) cursor_magnetism: CursorMagnetismState,
    pub(crate) cursor_comet: CursorCometState,
    pub(crate) cursor_particles: CursorParticlesState,
    pub(crate) typing_heatmap: TypingHeatmapState,
    pub(crate) scroll_velocity: ScrollVelocityState,
    pub(crate) resize_padding: ResizePaddingState,
    pub(crate) scroll_momentum: ScrollMomentumState,
    pub(crate) matrix_rain: MatrixRainState,
    pub(crate) cursor_ghost: CursorGhostState,
    pub(crate) sonar_ping: SonarPingState,
    pub(crate) lightning_bolt: LightningBoltState,
    pub(crate) pendulum: CursorPendulumState,
    pub(crate) sparkle_burst: SparkleBurstState,
    pub(crate) metronome: CursorMetronomeState,
    pub(crate) ripple_ring: RippleRingState,
    pub(crate) shockwave: ShockwaveState,
    pub(crate) bubble: BubbleState,
    pub(crate) firework: FireworkState,
    pub(crate) cursor_lightning: CursorLightningState,
    pub(crate) snowflake: SnowflakeState,
    pub(crate) edge_glow: EdgeGlowState,
    pub(crate) rain: RainState,
    pub(crate) ripple_wave: RippleWaveState,
    pub(crate) click_halo: ClickHaloState,
    pub(crate) edge_snap: EdgeSnapState,
    pub(crate) error_pulse: ErrorPulseState,
}

/// Free-running animation clocks. The renderer always holds valid instants;
/// a fresh `RendererFrameEffects` holds `None` so applying it preserves the
/// renderer's current clocks (see `apply_frame_effects`).
#[derive(Clone, Copy)]
pub(crate) struct EffectClocks {
    /// Last dim update time for smooth interpolation
    pub(crate) last_dim_tick: EventTime,
    /// Start time for pulse phase calculation
    pub(crate) cursor_pulse_start: EventTime,
    /// Search pulse start time
    pub(crate) search_pulse_start: EventTime,
    pub(crate) cursor_color_cycle_start: EventTime,
    pub(crate) focus_ring_start: EventTime,
    /// Last lightning bolt regeneration time
    pub(crate) lightning_bolt_last: EventTime,
    #[allow(dead_code)]
    pub(crate) rain_last_spawn: EventTime,
}

impl Default for EffectClocks {
    fn default() -> Self {
        // Adapter read: the renderer minting its own epoch at construction,
        // before any frame sample exists to date it to.
        let now = observe_platform_now();
        Self {
            last_dim_tick: now,
            cursor_pulse_start: now,
            search_pulse_start: now,
            cursor_color_cycle_start: now,
            focus_ring_start: now,
            lightning_bolt_last: now,
            rain_last_spawn: now,
        }
    }
}

/// Effect animation durations (configuration; NOT transferred by
/// frame-effects swaps).
pub(crate) struct EffectDurations {
    /// Ripple duration in seconds
    pub(crate) typing_ripple: f32,
    pub(crate) border_transition: std::time::Duration,
    pub(crate) cursor_trail_fade: std::time::Duration,
    pub(crate) scroll_line_spacing_ms: u32,
}

impl Default for EffectDurations {
    fn default() -> Self {
        Self {
            typing_ripple: 0.3,
            border_transition: std::time::Duration::from_millis(200),
            cursor_trail_fade: std::time::Duration::from_millis(300),
            scroll_line_spacing_ms: 200,
        }
    }
}

/// Ambient clocks shared by every frame context (NOT transferred by
/// frame-effects swaps): a child frame's effects read the same aurora phase
/// and render epoch as the primary frame.
pub(crate) struct AmbientClocks {
    pub(crate) aurora_start: EventTime,
    /// Start time for elapsed time calculation (used by fancy border effects)
    pub(crate) render_start_time: EventTime,
}

impl Default for AmbientClocks {
    fn default() -> Self {
        // Adapter read: the render epoch, minted once at construction.
        let now = observe_platform_now();
        Self {
            aurora_start: now,
            render_start_time: now,
        }
    }
}

/// Frame-local renderer effect queues that are rendered through `WgpuRenderer`.
#[derive(Default)]
pub struct RendererFrameEffects {
    pub(crate) fx: EffectsState,
    pub(crate) clocks: Option<EffectClocks>,
}

impl RendererFrameEffects {
    /// Whether the frame has any animating effect and therefore needs another
    /// frame. This is a pure poll of effect *state* — the demand is known
    /// before drawing, not latched during it (frame scheduling plan: demand
    /// exists before drawing). It is the union of the per-category predicates
    /// below, which the scheduler also uses to attribute the demand.
    pub fn needs_redraw(&self) -> bool {
        self.cursor_effects_active()
            || self.window_effects_active()
            || self.text_effects_active()
            || self.scroll_effects_active()
            || self.decorative_effects_active()
            || self.transient_effects_active()
    }

    /// Cursor-attached animations (wake, trail, magnetism, comet, particles,
    /// ghost, lightning, pendulum, sonar, sparkle, metronome).
    pub fn cursor_effects_active(&self) -> bool {
        self.fx.cursor_wake.started.is_some()
            || !self.fx.cursor_trail.positions.is_empty()
            || !self.fx.cursor_magnetism.entries.is_empty()
            || !self.fx.cursor_comet.positions.is_empty()
            || !self.fx.cursor_particles.entries.is_empty()
            || !self.fx.cursor_ghost.entries.is_empty()
            || self.fx.cursor_lightning.start.is_some()
            || self.fx.pendulum.swing_start.is_some()
            || !self.fx.sonar_ping.entries.is_empty()
            || !self.fx.sparkle_burst.entries.is_empty()
            || self.fx.metronome.tick_start.is_some()
    }

    /// Window/border/chrome animations (window fade, title fade, border
    /// transition, mode-line fade, animated borders, edge glow).
    pub fn window_effects_active(&self) -> bool {
        self.fx.has_animated_borders
            || !self.fx.window_fade.active.is_empty()
            || !self.fx.title_fade.active.is_empty()
            || !self.fx.border_transition.transitions.is_empty()
            || !self.fx.mode_line_fade.active.is_empty()
            || !self.fx.edge_glow.entries.is_empty()
    }

    /// Text animations (typing ripple, line insert/delete slide, text fade-in,
    /// typing heat map).
    pub fn text_effects_active(&self) -> bool {
        !self.fx.typing_ripple.active.is_empty()
            || !self.fx.line_anim.active.is_empty()
            || !self.fx.text_fade.active.is_empty()
            || !self.fx.typing_heatmap.entries.is_empty()
    }

    /// Scroll animations (line-spacing slide, velocity fade, momentum).
    pub fn scroll_effects_active(&self) -> bool {
        !self.fx.scroll_spacing.active.is_empty()
            || !self.fx.scroll_velocity.fades.is_empty()
            || !self.fx.scroll_momentum.active.is_empty()
    }

    /// Decorative/ambient one-shots (matrix rain, lightning, ripple ring,
    /// shockwave, bubble, firework, snowflake, rain, ripple wave, resize
    /// padding).
    pub fn decorative_effects_active(&self) -> bool {
        !self.fx.matrix_rain.columns.is_empty()
            || !self.fx.lightning_bolt.segments.is_empty()
            || self.fx.ripple_ring.start.is_some()
            || self.fx.shockwave.start.is_some()
            || self.fx.bubble.spawn_time.is_some()
            || self.fx.firework.start.is_some()
            || self.fx.snowflake.start.is_some()
            || !self.fx.rain.drops.is_empty()
            || !self.fx.ripple_wave.waves.is_empty()
            || self.fx.resize_padding.started.is_some()
    }

    /// Short-lived interaction effects (click halo, edge snap, error pulse).
    pub fn transient_effects_active(&self) -> bool {
        self.has_transient_effects()
    }

    pub fn has_transient_effects(&self) -> bool {
        !self.fx.click_halo.halos.is_empty()
            || !self.fx.edge_snap.snaps.is_empty()
            || self.fx.error_pulse.started.is_some()
    }

    // TRIGGER SIGNATURE: `now` should widen to `EventTime`. It still takes a
    // raw `Instant` only because `neomacs-display-runtime` bridges its own
    // `EventTime` through `into_instant()` at the call site; once that crate
    // passes the `EventTime` straight through, drop the re-wrap below.
    pub fn trigger_click_halo(
        &mut self,
        x: f32,
        y: f32,
        now: std::time::Instant,
        duration_ms: u32,
    ) {
        self.fx.click_halo.halos.push(ClickHaloEntry {
            x,
            y,
            started: EventTime::from_observed_instant(now),
            duration: std::time::Duration::from_millis(duration_ms as u64),
        });
    }

    // TRIGGER SIGNATURE: `now` should widen to `EventTime`. It still takes a
    // raw `Instant` only because `neomacs-display-runtime` bridges its own
    // `EventTime` through `into_instant()` at the call site; once that crate
    // passes the `EventTime` straight through, drop the re-wrap below.
    pub fn trigger_cursor_wake(&mut self, now: std::time::Instant) {
        self.fx.cursor_wake.started = Some(EventTime::from_observed_instant(now));
    }

    // TRIGGER SIGNATURE: `now` should widen to `EventTime`. It still takes a
    // raw `Instant` only because `neomacs-display-runtime` bridges its own
    // `EventTime` through `into_instant()` at the call site; once that crate
    // passes the `EventTime` straight through, drop the re-wrap below.
    pub fn trigger_resize_padding(&mut self, now: std::time::Instant) {
        self.fx.resize_padding.started = Some(EventTime::from_observed_instant(now));
    }

    /// TRIGGER SIGNATURE: this should take an `EventTime` for when the keypress
    /// happened. It has no time parameter at all today, so it has to mint its
    /// own observation — the trigger moment is genuinely "now" here, not a
    /// frame's visuals, so the adapter read is honest, just less precise than
    /// the originating input event's stamp would be.
    pub fn spawn_ripple(&mut self, cx: f32, cy: f32) {
        self.fx
            .typing_ripple
            .active
            .push((cx, cy, observe_platform_now()));
    }

    /// TRIGGER SIGNATURE: this should take an `EventTime` for when the cursor
    /// reached this position; with no time parameter it mints its own.
    pub fn record_cursor_trail(&mut self, x: f32, y: f32, w: f32, h: f32, length: usize) {
        let dist = ((x - self.fx.cursor_trail.last_pos.0).powi(2)
            + (y - self.fx.cursor_trail.last_pos.1).powi(2))
        .sqrt();
        if dist < 2.0 {
            return;
        }
        self.fx
            .cursor_trail
            .positions
            .push((x, y, w, h, observe_platform_now()));
        self.fx.cursor_trail.last_pos = (x, y);
        while self.fx.cursor_trail.positions.len() > length {
            self.fx.cursor_trail.positions.remove(0);
        }
    }

    // TRIGGER SIGNATURE: `now` should widen to `EventTime`. It still takes a
    // raw `Instant` only because `neomacs-display-runtime` bridges its own
    // `EventTime` through `into_instant()` at the call site; once that crate
    // passes the `EventTime` straight through, drop the re-wrap below.
    pub fn trigger_edge_snap(
        &mut self,
        bounds: Rect,
        mode_line_height: f32,
        at_top: bool,
        at_bottom: bool,
        now: std::time::Instant,
        duration_ms: u32,
    ) {
        self.fx.edge_snap.snaps.push(EdgeSnapEntry {
            bounds,
            mode_line_height,
            at_top,
            at_bottom,
            started: EventTime::from_observed_instant(now),
            duration: std::time::Duration::from_millis(duration_ms as u64),
        });
    }

    // TRIGGER SIGNATURE: `now` should widen to `EventTime`. It still takes a
    // raw `Instant` only because `neomacs-display-runtime` bridges its own
    // `EventTime` through `into_instant()` at the call site; once that crate
    // passes the `EventTime` straight through, drop the re-wrap below.
    pub fn trigger_cursor_error_pulse(&mut self, now: std::time::Instant) {
        self.fx.error_pulse.started = Some(EventTime::from_observed_instant(now));
    }
}

#[cfg(test)]
mod effect_category_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn default_effects_are_quiet() {
        let fx = RendererFrameEffects::default();
        assert!(!fx.needs_redraw());
        assert!(!fx.cursor_effects_active());
        assert!(!fx.window_effects_active());
        assert!(!fx.text_effects_active());
        assert!(!fx.scroll_effects_active());
        assert!(!fx.decorative_effects_active());
        assert!(!fx.transient_effects_active());
    }

    #[test]
    fn each_category_drives_needs_redraw() {
        let now = observe_platform_now();

        // Cursor: wake.
        let mut fx = RendererFrameEffects::default();
        fx.fx.cursor_wake.started = Some(now);
        assert!(fx.cursor_effects_active() && fx.needs_redraw());

        // Window: animated borders.
        let mut fx = RendererFrameEffects::default();
        fx.fx.has_animated_borders = true;
        assert!(fx.window_effects_active() && fx.needs_redraw());

        // Text: typing ripple.
        let mut fx = RendererFrameEffects::default();
        fx.fx.typing_ripple.active.push((0.0, 0.0, now));
        assert!(fx.text_effects_active() && fx.needs_redraw());

        // Scroll: velocity fade.
        let mut fx = RendererFrameEffects::default();
        fx.fx.scroll_velocity.fades.push(ScrollVelocityFadeEntry {
            window_id: 1,
            bounds: neomacs_display_protocol::types::Rect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            velocity: 1.0,
            started: now,
            duration: Duration::from_millis(100),
        });
        assert!(fx.scroll_effects_active() && fx.needs_redraw());

        // Decorative: ripple ring.
        let mut fx = RendererFrameEffects::default();
        fx.fx.ripple_ring.start = Some(now);
        assert!(fx.decorative_effects_active() && fx.needs_redraw());

        // Transient: error pulse.
        let mut fx = RendererFrameEffects::default();
        fx.fx.error_pulse.started = Some(now);
        assert!(fx.transient_effects_active() && fx.needs_redraw());
    }
}
