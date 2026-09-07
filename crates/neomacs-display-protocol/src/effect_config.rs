//! Effect configuration types for the neomacs display engine.
//!
//! Each visual effect has its own config struct with all its parameters.
//! All configs are grouped in `EffectsConfig` which is shared between
//! `RenderApp` and `WgpuRenderer`.
use crate::types::FaceId;
use std::num::NonZeroU32;

/// A user-facing animation rate that is non-zero by construction.
///
/// The transparent representation keeps the effect registry's Elisp value an
/// integer while preventing an invalid zero cadence from entering a committed
/// [`EffectsConfig`]. Display-specific caps remain a render-runtime policy.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct FrameRate(NonZeroU32);

impl FrameRate {
    #[must_use]
    pub const fn new(fps: u32) -> Option<Self> {
        match NonZeroU32::new(fps) {
            Some(fps) => Some(Self(fps)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// Macro for defining effect config structs with Default implementations.
///
/// What one effect property is, in enough detail to build a typed
/// customization widget for it.
///
/// The registry can already *validate* all of this — `number_for_target` knows
/// which properties are unit intervals, which are percentages, which must be
/// non-negative integers, which are colours and which are durations. But it
/// knows it privately, inside a function that runs when a value arrives, so the
/// only thing Lisp can learn about a property is the shape of its current
/// value. That is why `neomacs-effects` is a `sexp` field: there is nothing to
/// build a `:type` out of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyKind {
    Boolean,
    /// A whole number. Fractions are refused by the registry, not rounded.
    Integer,
    Number,
    /// A number the registry clamps to `0.0..=1.0`.
    UnitInterval,
    /// A number the registry refuses above `100.0`.
    Percentage,
    /// Colour channels, accepted as `"#RRGGBB"` or a channel list.
    Color,
    /// A list of colours, such as a gradient's stops.
    ColorList,
    /// A duration, carried to Lisp as seconds.
    Seconds,
    /// One of a fixed set of symbols.
    Symbol(&'static [&'static str]),
}

/// The kind a property has by virtue of its Rust type.
///
/// A trait rather than a match inside the macro, so adding a property type
/// fails to compile until its kind is stated rather than silently degrading to
/// something vague.
pub trait EffectProperty {
    const KIND: PropertyKind;
}

impl EffectProperty for bool {
    const KIND: PropertyKind = PropertyKind::Boolean;
}
impl EffectProperty for f32 {
    const KIND: PropertyKind = PropertyKind::Number;
}
impl EffectProperty for u32 {
    const KIND: PropertyKind = PropertyKind::Integer;
}
impl EffectProperty for i32 {
    const KIND: PropertyKind = PropertyKind::Integer;
}
impl EffectProperty for usize {
    const KIND: PropertyKind = PropertyKind::Integer;
}
impl EffectProperty for std::time::Duration {
    const KIND: PropertyKind = PropertyKind::Seconds;
}
impl EffectProperty for (f32, f32, f32) {
    const KIND: PropertyKind = PropertyKind::Color;
}
impl EffectProperty for (f32, f32, f32, f32) {
    const KIND: PropertyKind = PropertyKind::Color;
}
impl EffectProperty for Vec<(f32, f32, f32, f32)> {
    const KIND: PropertyKind = PropertyKind::ColorList;
}
impl EffectProperty for FrameRate {
    /// Frames per second, and strictly positive — the registry refuses zero,
    /// which is why this is not merely an integer.
    const KIND: PropertyKind = PropertyKind::Integer;
}
impl EffectProperty for crate::types::FaceId {
    /// A face identifier, not a quantity. It is an integer only incidentally.
    const KIND: PropertyKind = PropertyKind::Integer;
}
/// The legal names of a symbol-valued property, straight from the enum.
///
/// `strum::VariantNames` rather than a hand-written list: a new variant then
/// appears in the widget automatically instead of being silently missing from
/// a menu the user picks from.
macro_rules! symbol_property {
    ($ty:ty) => {
        impl EffectProperty for $ty {
            const KIND: PropertyKind = PropertyKind::Symbol(<$ty as strum::VariantNames>::VARIANTS);
        }
    };
}

symbol_property!(crate::TransitionEasing);
symbol_property!(crate::TransitionEffect);
symbol_property!(crate::TransitionAxisPreference);
symbol_property!(crate::TransitionDirection);
symbol_property!(crate::CursorAnimStyle);
symbol_property!(crate::window_animation::MotionKind);

/// One property, as a customization widget would need it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PropertySchema {
    /// The name Lisp sees, kebab-cased.
    pub property: &'static str,
    pub kind: PropertyKind,
}

impl PropertySchema {
    /// Refine a type-derived kind with what the registry knows by name.
    ///
    /// `opacity: f32` is not merely a number: `number_for_target` clamps it to
    /// the unit interval, and a widget that does not say so lets a user set 5.0
    /// and wonder why it came back 1.0. The name rules live in
    /// `effect_command.rs` because that is where they are enforced; this is the
    /// one place they are *published*, and the test beside it pins the two
    /// together.
    #[must_use]
    pub const fn refined(self) -> Self {
        let kind = match self.kind {
            PropertyKind::Number => crate::effect_command::kind_for_number_property(self.property),
            other => other,
        };
        Self { kind, ..self }
    }
}

/// A `PROPERTIES` table for a config struct the `effect_config!` macro does not
/// declare.
///
/// The behavioural slots — the window-animation four, cursor blink and motion,
/// the buffer and scroll transitions — are hand-written because their defaults
/// differ per instance (`window-open` is an easing, `window-resize` a spring)
/// and the macro derives one `Default` from the field list. They still have to
/// publish a schema, so this states one from the same field-and-type pairs.
#[macro_export]
macro_rules! effect_schema {
    ($name:ty { $($field:ident : $ty:ty),* $(,)? }) => {
        impl $name {
            pub const PROPERTIES: &'static [$crate::effect_config::PropertySchema] = &[
                $($crate::effect_config::PropertySchema {
                    property: stringify!($field),
                    kind: <$ty as $crate::effect_config::EffectProperty>::KIND,
                }),*
            ];
        }
    };
}

macro_rules! effect_config {
    (
        $(#[$meta:meta])*
        $name:ident {
            $($field:ident : $ty:ty = $default:expr),*
            $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq)]
        #[derive(serde::Serialize, serde::Deserialize)]
        pub struct $name {
            $(pub $field: $ty),*
        }
        impl Default for $name {
            fn default() -> Self {
                Self {
                    $($field: $default),*
                }
            }
        }
        impl $name {
            /// Every property of this effect, with the kind a customization
            /// widget needs. Emitted from the same declaration that defines the
            /// fields, so the two cannot drift.
            pub const PROPERTIES: &'static [$crate::effect_config::PropertySchema] = &[
                $($crate::effect_config::PropertySchema {
                    property: stringify!($field),
                    kind: <$ty as $crate::effect_config::EffectProperty>::KIND,
                }),*
            ];
        }
    };
}

effect_config!(
    /// Configuration for the accent strip effect.
    AccentStripConfig {
        enabled: bool = false,
        width: f32 = 3.0,
    }
);

effect_config!(
    /// Configuration for the argyle pattern effect.
    ArgylePatternConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.5, 0.3, 0.3),
        diamond_size: f32 = 30.0,
        line_width: f32 = 1.0,
        opacity: f32 = 0.05,
    }
);

effect_config!(
    /// Configuration for the aurora effect.
    AuroraConfig {
        enabled: bool = false,
        color1: (f32, f32, f32) = (0.2, 0.8, 0.4),
        color2: (f32, f32, f32) = (0.3, 0.4, 0.9),
        height: f32 = 60.0,
        speed: f32 = 1.0,
        opacity: f32 = 0.12,
    }
);

effect_config!(
    /// Configuration for the basket weave effect.
    BasketWeaveConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.55, 0.4, 0.25),
        strip_width: f32 = 6.0,
        strip_spacing: f32 = 20.0,
        opacity: f32 = 0.05,
    }
);

effect_config!(
    /// Configuration for the bg gradient effect.
    BgGradientConfig {
        enabled: bool = false,
        top: (f32, f32, f32) = (0.0, 0.0, 0.0),
        bottom: (f32, f32, f32) = (0.0, 0.0, 0.0),
    }
);

effect_config!(
    /// Configuration for the bg pattern effect.
    BgPatternConfig {
        style: u32 = 0,
        spacing: f32 = 20.0,
        color: (f32, f32, f32) = (0.5, 0.5, 0.5),
        opacity: f32 = 0.05,
    }
);

effect_config!(
    /// Configuration for the border transition effect.
    BorderTransitionConfig {
        enabled: bool = false,
        active_color: (f32, f32, f32) = (0.4, 0.6, 1.0),
        duration_ms: u32 = 200,
    }
);

effect_config!(
    /// Configuration for the breadcrumb effect.
    BreadcrumbConfig {
        enabled: bool = false,
        opacity: f32 = 0.7,
    }
);

effect_config!(
    /// Configuration for the breathing border effect.
    BreathingBorderConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.5, 0.5, 0.5),
        min_opacity: f32 = 0.05,
        max_opacity: f32 = 0.3,
        cycle_ms: u32 = 3000,
    }
);

effect_config!(
    /// Configuration for the brick wall effect.
    BrickWallConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.6, 0.4, 0.3),
        width: f32 = 40.0,
        height: f32 = 20.0,
        opacity: f32 = 0.06,
    }
);

effect_config!(
    /// Configuration for the celtic knot effect.
    CelticKnotConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.0, 0.6, 0.3),
        scale: f32 = 60.0,
        weave_speed: f32 = 1.0,
        opacity: f32 = 0.06,
    }
);

effect_config!(
    /// Configuration for the chevron pattern effect.
    ChevronPatternConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.7, 0.5),
        spacing: f32 = 40.0,
        speed: f32 = 0.5,
        opacity: f32 = 0.06,
    }
);

effect_config!(
    /// Configuration for the circuit trace effect.
    CircuitTraceConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.2, 0.8, 0.4),
        width: f32 = 2.0,
        speed: f32 = 1.0,
        opacity: f32 = 0.2,
    }
);

effect_config!(
    /// Configuration for the click halo effect.
    ClickHaloConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.6, 1.0),
        duration_ms: u32 = 300,
        max_radius: f32 = 30.0,
    }
);

effect_config!(
    /// Configuration for the concentric rings effect.
    ConcentricRingsConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.6, 1.0),
        spacing: f32 = 30.0,
        expansion_speed: f32 = 1.0,
        opacity: f32 = 0.06,
    }
);

effect_config!(
    /// Configuration for the constellation effect.
    ConstellationConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.7, 0.8, 1.0),
        star_count: u32 = 50,
        connect_dist: f32 = 80.0,
        twinkle_speed: f32 = 1.0,
        opacity: f32 = 0.15,
    }
);

effect_config!(
    /// Configuration for the corner fold effect.
    CornerFoldConfig {
        enabled: bool = false,
        size: f32 = 20.0,
        color: (f32, f32, f32) = (0.6, 0.4, 0.2),
        opacity: f32 = 0.5,
    }
);

effect_config!(
    /// Configuration for the crosshatch pattern effect.
    CrosshatchPatternConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.5, 0.6, 0.4),
        line_spacing: f32 = 20.0,
        angle: f32 = 45.0,
        speed: f32 = 0.3,
        opacity: f32 = 0.06,
    }
);

effect_config!(
    /// Configuration for the cursor aurora borealis effect.
    CursorAuroraBorealisConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.2, 0.9, 0.5),
        band_count: u32 = 5,
        shimmer_speed: f32 = 1.0,
        opacity: f32 = 0.15,
    }
);

effect_config!(
    /// Configuration for the cursor bubble effect.
    CursorBubbleConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.8, 1.0),
        count: u32 = 6,
        rise_speed: f32 = 80.0,
        opacity: f32 = 0.2,
    }
);

effect_config!(
    /// Configuration for the cursor candle flame effect.
    CursorCandleFlameConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.7, 0.2),
        height: u32 = 20,
        flicker_speed: f32 = 1.0,
        opacity: f32 = 0.2,
    }
);

effect_config!(
    /// Configuration for the cursor color cycle effect.
    ///
    /// Enabled as a product default. Its standing compositor-only demand is
    /// capped below display cadence by default so the ambient animation does
    /// not make an otherwise idle editor present once per refresh.
    CursorColorCycleConfig {
        enabled: bool = true,
        fps: FrameRate = FrameRate::new(24).expect("24 Hz is non-zero"),
        speed: f32 = 0.5,
        saturation: f32 = 0.8,
        lightness: f32 = 0.6,
    }
);

effect_config!(
    /// Configuration for the cursor comet effect.
    CursorCometConfig {
        enabled: bool = false,
        trail_length: u32 = 5,
        fade_ms: u32 = 300,
        color: (f32, f32, f32) = (0.5, 0.7, 1.0),
        opacity: f32 = 0.6,
    }
);

effect_config!(
    /// Configuration for the cursor compass effect.
    CursorCompassConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.9, 0.6, 0.2),
        size: f32 = 20.0,
        speed: f32 = 1.0,
        opacity: f32 = 0.25,
    }
);

effect_config!(
    /// Configuration for the cursor compass needle effect.
    CursorCompassNeedleConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.3, 0.3),
        length: f32 = 20.0,
        spin_speed: f32 = 2.0,
        opacity: f32 = 0.2,
    }
);

effect_config!(
    /// Configuration for the cursor crosshair effect.
    CursorCrosshairConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.5, 0.5, 0.5),
        opacity: f32 = 0.15,
    }
);

effect_config!(
    /// Configuration for the cursor crystal effect.
    CursorCrystalConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.7, 0.9, 1.0),
        facet_count: u32 = 6,
        radius: f32 = 25.0,
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the cursor dna helix effect.
    CursorDnaHelixConfig {
        enabled: bool = false,
        color1: (f32, f32, f32) = (0.3, 0.9, 0.5),
        color2: (f32, f32, f32) = (0.5, 0.3, 0.9),
        radius: f32 = 12.0,
        speed: f32 = 1.5,
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the cursor elastic snap effect.
    CursorElasticSnapConfig {
        enabled: bool = false,
        overshoot: f32 = 0.15,
        duration_ms: u32 = 200,
    }
);

effect_config!(
    /// Configuration for the cursor error pulse effect.
    CursorErrorPulseConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.2, 0.2),
        duration_ms: u32 = 250,
    }
);

effect_config!(
    /// Configuration for the cursor feather effect.
    CursorFeatherConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.9, 0.85, 0.7),
        count: u32 = 4,
        drift_speed: f32 = 1.0,
        opacity: f32 = 0.18,
    }
);

effect_config!(
    /// Configuration for the cursor firework effect.
    CursorFireworkConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.6, 0.2),
        particle_count: u32 = 16,
        burst_radius: f32 = 60.0,
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the cursor flame effect.
    CursorFlameConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.5, 0.1),
        particle_count: u32 = 10,
        height: f32 = 40.0,
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the cursor galaxy effect.
    CursorGalaxyConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.8, 0.8, 1.0),
        star_count: u32 = 30,
        radius: f32 = 30.0,
        opacity: f32 = 0.2,
    }
);

effect_config!(
    /// Configuration for the cursor ghost effect.
    CursorGhostConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.5, 0.5, 1.0),
        count: u32 = 4,
        fade_ms: u32 = 600,
        drift: f32 = 20.0,
        opacity: f32 = 0.4,
    }
);

effect_config!(
    /// Configuration for the cursor glow effect.
    CursorGlowConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.6, 1.0),
        radius: f32 = 30.0,
        opacity: f32 = 0.15,
    }
);

effect_config!(
    /// Configuration for the cursor gravity well effect.
    CursorGravityWellConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.3, 0.6, 1.0),
        field_radius: f32 = 80.0,
        line_count: u32 = 8,
        opacity: f32 = 0.2,
    }
);

effect_config!(
    /// Configuration for the cursor heartbeat effect.
    CursorHeartbeatConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.3, 0.3),
        bpm: f32 = 72.0,
        max_radius: f32 = 50.0,
        opacity: f32 = 0.2,
    }
);

effect_config!(
    /// Configuration for the cursor lighthouse effect.
    CursorLighthouseConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.9, 0.3),
        beam_width: f32 = 15.0,
        rotation_speed: f32 = 0.5,
        beam_length: f32 = 200.0,
        opacity: f32 = 0.12,
    }
);

effect_config!(
    /// Configuration for the cursor lightning effect.
    CursorLightningConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.6, 0.8, 1.0),
        bolt_count: u32 = 4,
        max_length: f32 = 50.0,
        opacity: f32 = 0.4,
    }
);

effect_config!(
    /// Configuration for the cursor magnetism effect.
    CursorMagnetismConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.7, 1.0),
        ring_count: u32 = 3,
        duration_ms: u32 = 300,
        opacity: f32 = 0.5,
    }
);

effect_config!(
    /// Configuration for the cursor metronome effect.
    CursorMetronomeConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.9, 0.5, 0.2),
        tick_height: f32 = 20.0,
        fade_ms: u32 = 300,
        opacity: f32 = 0.4,
    }
);

effect_config!(
    /// Configuration for the cursor moth effect.
    CursorMothConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.9, 0.8, 0.5),
        count: u32 = 5,
        wing_size: f32 = 8.0,
        opacity: f32 = 0.2,
    }
);

effect_config!(
    /// Configuration for the cursor moth flame effect.
    CursorMothFlameConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.8, 0.7, 0.5),
        moth_count: u32 = 5,
        orbit_speed: f32 = 1.0,
        opacity: f32 = 0.18,
    }
);

effect_config!(
    /// Configuration for the cursor orbit particles effect.
    CursorOrbitParticlesConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.8, 0.3),
        count: u32 = 6,
        radius: f32 = 25.0,
        speed: f32 = 1.5,
        opacity: f32 = 0.35,
    }
);

effect_config!(
    /// Configuration for the cursor particles effect.
    CursorParticlesConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.6, 0.2),
        count: u32 = 6,
        lifetime_ms: u32 = 800,
        gravity: f32 = 120.0,
    }
);

effect_config!(
    /// Configuration for the cursor pendulum effect.
    CursorPendulumConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.9, 0.7, 0.3),
        arc_length: f32 = 40.0,
        damping: f32 = 0.5,
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the cursor pixel dust effect.
    CursorPixelDustConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.8, 0.8, 0.6),
        count: u32 = 15,
        scatter_speed: f32 = 1.0,
        opacity: f32 = 0.2,
    }
);

effect_config!(
    /// Configuration for the cursor plasma ball effect.
    CursorPlasmaBallConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.7, 0.3, 1.0),
        tendril_count: u32 = 6,
        arc_speed: f32 = 1.0,
        opacity: f32 = 0.2,
    }
);

effect_config!(
    /// Configuration for the cursor portal effect.
    CursorPortalConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.6, 0.2, 0.9),
        radius: f32 = 30.0,
        speed: f32 = 2.0,
        opacity: f32 = 0.25,
    }
);

effect_config!(
    /// Configuration for the cursor prism effect.
    CursorPrismConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 1.0, 1.0),
        ray_count: u32 = 7,
        spread: f32 = 30.0,
        opacity: f32 = 0.15,
    }
);

effect_config!(
    /// Configuration for the cursor pulse effect.
    CursorPulseConfig {
        enabled: bool = false,
        speed: f32 = 1.0,
        min_opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the cursor quill pen effect.
    CursorQuillPenConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.3, 0.15, 0.05),
        trail_length: u32 = 8,
        ink_speed: f32 = 1.0,
        opacity: f32 = 0.2,
    }
);

effect_config!(
    /// Configuration for the cursor radar effect.
    CursorRadarConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.2, 0.9, 0.4),
        radius: f32 = 40.0,
        speed: f32 = 1.5,
        opacity: f32 = 0.2,
    }
);

effect_config!(
    /// Configuration for the cursor ripple ring effect.
    CursorRippleRingConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.3, 0.8, 1.0),
        max_radius: f32 = 60.0,
        count: u32 = 3,
        speed: f32 = 2.0,
        opacity: f32 = 0.25,
    }
);

effect_config!(
    /// Configuration for the cursor ripple wave effect.
    CursorRippleWaveConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.6, 1.0),
        ring_count: u32 = 3,
        max_radius: f32 = 80.0,
        duration_ms: u32 = 500,
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the cursor scope effect.
    CursorScopeConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.8, 0.2),
        thickness: f32 = 1.0,
        gap: f32 = 10.0,
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the cursor shadow effect.
    CursorShadowConfig {
        enabled: bool = false,
        offset_x: f32 = 2.0,
        offset_y: f32 = 2.0,
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the cursor shockwave effect.
    CursorShockwaveConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.6, 0.2),
        radius: f32 = 80.0,
        decay: f32 = 2.0,
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the cursor snowflake effect.
    CursorSnowflakeConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.8, 0.9, 1.0),
        count: u32 = 8,
        fall_speed: f32 = 30.0,
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the cursor sonar ping effect.
    CursorSonarPingConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.3, 0.7, 1.0),
        ring_count: u32 = 3,
        max_radius: f32 = 60.0,
        duration_ms: u32 = 600,
        opacity: f32 = 0.25,
    }
);

effect_config!(
    /// Configuration for the cursor sparkle burst effect.
    CursorSparkleBurstConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.85, 0.3),
        count: u32 = 12,
        radius: f32 = 30.0,
        opacity: f32 = 0.4,
    }
);

effect_config!(
    /// Configuration for the cursor sparkler effect.
    CursorSparklerConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.85, 0.3),
        spark_count: u32 = 12,
        burn_speed: f32 = 1.0,
        opacity: f32 = 0.25,
    }
);

effect_config!(
    /// Configuration for the cursor spotlight effect.
    CursorSpotlightConfig {
        enabled: bool = false,
        radius: f32 = 200.0,
        intensity: f32 = 0.15,
        color: (f32, f32, f32) = (1.0, 1.0, 0.9),
    }
);

effect_config!(
    /// Configuration for the cursor stardust effect.
    CursorStardustConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.9, 0.5),
        particle_count: u32 = 20,
        fall_speed: f32 = 1.0,
        opacity: f32 = 0.2,
    }
);

effect_config!(
    /// Configuration for the cursor tornado effect.
    CursorTornadoConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.5, 0.7, 1.0),
        radius: f32 = 40.0,
        particle_count: u32 = 12,
        opacity: f32 = 0.25,
    }
);

effect_config!(
    /// Configuration for the cursor trail fade effect.
    CursorTrailFadeConfig {
        enabled: bool = false,
        length: usize = 8,
        ms: u32 = 300,
    }
);

effect_config!(
    /// Configuration for the cursor wake effect.
    CursorWakeConfig {
        enabled: bool = false,
        duration_ms: u32 = 120,
        scale: f32 = 1.3,
    }
);

effect_config!(
    /// Configuration for the cursor water drop effect.
    CursorWaterDropConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.3, 0.6, 0.9),
        ripple_count: u32 = 4,
        expand_speed: f32 = 1.0,
        opacity: f32 = 0.15,
    }
);

effect_config!(
    /// Configuration for the depth shadow effect.
    DepthShadowConfig {
        enabled: bool = false,
        layers: u32 = 3,
        offset: f32 = 2.0,
        color: (f32, f32, f32) = (0.0, 0.0, 0.0),
        opacity: f32 = 0.15,
    }
);

effect_config!(
    /// Configuration for the diamond lattice effect.
    DiamondLatticeConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.7, 0.5, 0.9),
        cell_size: f32 = 30.0,
        shimmer_speed: f32 = 0.8,
        opacity: f32 = 0.08,
    }
);

effect_config!(
    /// Configuration for the dot matrix effect.
    DotMatrixConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.3, 1.0, 0.3),
        spacing: f32 = 12.0,
        pulse_speed: f32 = 1.0,
        opacity: f32 = 0.06,
    }
);

effect_config!(
    /// Configuration for the edge glow effect.
    EdgeGlowConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.6, 1.0),
        height: f32 = 40.0,
        opacity: f32 = 0.3,
        fade_ms: u32 = 400,
    }
);

effect_config!(
    /// Configuration for the edge snap effect.
    EdgeSnapConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.5, 0.2),
        duration_ms: u32 = 200,
    }
);

effect_config!(
    /// Configuration for the fish scale effect.
    FishScaleConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.3, 0.6, 0.7),
        size: f32 = 16.0,
        row_offset: f32 = 0.5,
        opacity: f32 = 0.04,
    }
);

effect_config!(
    /// Configuration for the focus gradient border effect.
    FocusGradientBorderConfig {
        enabled: bool = false,
        top_color: (f32, f32, f32) = (0.3, 0.6, 1.0),
        bot_color: (f32, f32, f32) = (0.6, 0.3, 1.0),
        width: f32 = 2.0,
        opacity: f32 = 0.6,
    }
);

effect_config!(
    /// Configuration for the focus mode effect.
    FocusModeConfig {
        enabled: bool = false,
        opacity: f32 = 0.4,
    }
);

effect_config!(
    /// Configuration for the focus ring effect.
    FocusRingConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.6, 1.0),
        opacity: f32 = 0.5,
        dash_length: f32 = 8.0,
        speed: f32 = 40.0,
    }
);

effect_config!(
    /// Configuration for the frost border effect.
    FrostBorderConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.7, 0.85, 1.0),
        width: f32 = 6.0,
        opacity: f32 = 0.2,
    }
);

effect_config!(
    /// Configuration for the frosted border effect.
    FrostedBorderConfig {
        enabled: bool = false,
        width: f32 = 4.0,
        opacity: f32 = 0.15,
        color: (f32, f32, f32) = (1.0, 1.0, 1.0),
    }
);

effect_config!(
    /// Configuration for the frosted glass effect.
    FrostedGlassConfig {
        enabled: bool = false,
        opacity: f32 = 0.3,
        blur: f32 = 4.0,
    }
);

effect_config!(
    /// Configuration for the guilloche effect.
    GuillocheConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.6, 0.4, 0.7),
        curve_count: u32 = 8,
        wave_freq: f32 = 1.0,
        opacity: f32 = 0.05,
    }
);

effect_config!(
    /// Configuration for the header shadow effect.
    HeaderShadowConfig {
        enabled: bool = false,
        intensity: f32 = 0.3,
        size: f32 = 6.0,
    }
);

effect_config!(
    /// Configuration for the heat distortion effect.
    HeatDistortionConfig {
        enabled: bool = false,
        intensity: f32 = 0.3,
        speed: f32 = 1.0,
        edge_width: f32 = 30.0,
        opacity: f32 = 0.15,
    }
);

effect_config!(
    /// Configuration for the herringbone pattern effect.
    HerringbonePatternConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.6, 0.5, 0.4),
        tile_width: f32 = 20.0,
        tile_height: f32 = 10.0,
        opacity: f32 = 0.05,
    }
);

effect_config!(
    /// Configuration for the hex grid effect.
    HexGridConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.3, 0.6, 0.9),
        cell_size: f32 = 40.0,
        pulse_speed: f32 = 1.0,
        opacity: f32 = 0.1,
    }
);

effect_config!(
    /// Configuration for the honeycomb dissolve effect.
    HoneycombDissolveConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.8, 0.6, 0.2),
        cell_size: f32 = 30.0,
        speed: f32 = 0.8,
        opacity: f32 = 0.08,
    }
);

effect_config!(
    /// Configuration for the idle dim effect.
    IdleDimConfig {
        enabled: bool = false,
        delay: std::time::Duration = std::time::Duration::from_secs(60),
        opacity: f32 = 0.4,
        fade_duration: std::time::Duration = std::time::Duration::from_millis(500),
    }
);

effect_config!(
    /// Configuration for the inactive dim effect.
    InactiveDimConfig {
        enabled: bool = false,
        opacity: f32 = 0.15,
    }
);

effect_config!(
    /// Configuration for the inactive tint effect.
    InactiveTintConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.2, 0.1, 0.0),
        opacity: f32 = 0.1,
    }
);

effect_config!(
    /// Configuration for the indent guides effect.
    IndentGuidesConfig {
        enabled: bool = false,
        color: (f32, f32, f32, f32) = (0.3, 0.3, 0.3, 0.3),
        rainbow_enabled: bool = false,
        rainbow_colors: Vec<(f32, f32, f32, f32)> = Vec::new(),
    }
);

effect_config!(
    /// Configuration for the kaleidoscope effect.
    KaleidoscopeConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.6, 0.3, 0.9),
        segments: u32 = 6,
        speed: f32 = 0.5,
        opacity: f32 = 0.1,
    }
);

effect_config!(
    /// Configuration for the lightning bolt effect.
    LightningBoltConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.7, 0.8, 1.0),
        frequency: f32 = 1.0,
        intensity: f32 = 0.8,
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the line animation effect.
    LineAnimationConfig {
        enabled: bool = false,
        duration_ms: u32 = 150,
    }
);

effect_config!(
    /// Configuration for the line highlight effect.
    LineHighlightConfig {
        enabled: bool = false,
        color: (f32, f32, f32, f32) = (0.2, 0.2, 0.3, 0.15),
    }
);

effect_config!(
    /// Configuration for the line number pulse effect.
    LineNumberPulseConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.6, 1.0),
        intensity: f32 = 0.3,
        cycle_ms: u32 = 2000,
    }
);

effect_config!(
    /// Configuration for the matrix rain effect.
    MatrixRainConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.0, 0.8, 0.2),
        column_count: u32 = 40,
        speed: f32 = 150.0,
        opacity: f32 = 0.12,
    }
);

effect_config!(
    /// Configuration for the minibuffer highlight effect.
    MinibufferHighlightConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.6, 1.0),
        opacity: f32 = 0.25,
    }
);

effect_config!(
    /// Configuration for the minimap effect.
    MinimapConfig {
        enabled: bool = false,
        width: f32 = 80.0,
    }
);

effect_config!(
    /// Configuration for the mode line gradient effect.
    ModeLineGradientConfig {
        enabled: bool = false,
        left_color: (f32, f32, f32) = (0.2, 0.3, 0.5),
        right_color: (f32, f32, f32) = (0.5, 0.3, 0.2),
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the mode line separator effect.
    ModeLineSeparatorConfig {
        style: u32 = 0,
        color: (f32, f32, f32) = (0.0, 0.0, 0.0),
        height: f32 = 3.0,
    }
);

effect_config!(
    /// Configuration for the mode line transition effect.
    ModeLineTransitionConfig {
        enabled: bool = false,
        duration_ms: u32 = 200,
    }
);

effect_config!(
    /// Configuration for the modified indicator effect.
    ModifiedIndicatorConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.6, 0.2),
        width: f32 = 3.0,
        opacity: f32 = 0.8,
    }
);

effect_config!(
    /// Configuration for the moire pattern effect.
    MoirePatternConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.5, 0.5, 0.8),
        line_spacing: f32 = 8.0,
        angle_offset: f32 = 5.0,
        speed: f32 = 0.3,
        opacity: f32 = 0.06,
    }
);

effect_config!(
    /// Configuration for the neon border effect.
    NeonBorderConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.0, 1.0, 0.8),
        intensity: f32 = 0.6,
        flicker: f32 = 0.1,
        thickness: f32 = 3.0,
        opacity: f32 = 0.4,
    }
);

effect_config!(
    /// Configuration for the noise field effect.
    NoiseFieldConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.5, 0.7, 0.3),
        scale: f32 = 50.0,
        speed: f32 = 0.5,
        opacity: f32 = 0.08,
    }
);

effect_config!(
    /// Configuration for the noise grain effect.
    NoiseGrainConfig {
        enabled: bool = false,
        intensity: f32 = 0.03,
        size: f32 = 2.0,
    }
);

effect_config!(
    /// Configuration for the padding gradient effect.
    PaddingGradientConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.0, 0.0, 0.0),
        opacity: f32 = 0.15,
        width: f32 = 8.0,
    }
);

effect_config!(
    /// Configuration for the plaid pattern effect.
    PlaidPatternConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.7, 0.3, 0.3),
        band_width: f32 = 4.0,
        band_spacing: f32 = 30.0,
        opacity: f32 = 0.05,
    }
);

effect_config!(
    /// Configuration for the plasma border effect.
    PlasmaBorderConfig {
        enabled: bool = false,
        color1: (f32, f32, f32) = (1.0, 0.2, 0.5),
        color2: (f32, f32, f32) = (0.2, 0.5, 1.0),
        width: f32 = 4.0,
        speed: f32 = 1.0,
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the prism edge effect.
    PrismEdgeConfig {
        enabled: bool = false,
        width: f32 = 6.0,
        speed: f32 = 1.0,
        saturation: f32 = 0.8,
        opacity: f32 = 0.25,
    }
);

effect_config!(
    /// Configuration for the rain effect effect.
    RainEffectConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.5, 0.6, 0.8),
        drop_count: u32 = 30,
        speed: f32 = 120.0,
        opacity: f32 = 0.15,
    }
);

effect_config!(
    /// Configuration for the region glow effect.
    RegionGlowConfig {
        enabled: bool = false,
        face_id: FaceId = FaceId::new(0),
        radius: f32 = 6.0,
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the resize padding effect.
    ResizePaddingConfig {
        enabled: bool = false,
        duration_ms: u32 = 200,
        max: f32 = 12.0,
    }
);

effect_config!(
    /// Configuration for the rotating gear effect.
    RotatingGearConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.6, 0.7, 0.8),
        size: f32 = 40.0,
        speed: f32 = 0.5,
        opacity: f32 = 0.08,
    }
);

effect_config!(
    /// Configuration for the scanlines effect.
    ScanlinesConfig {
        enabled: bool = false,
        spacing: u32 = 2,
        opacity: f32 = 0.08,
        color: (f32, f32, f32) = (0.0, 0.0, 0.0),
    }
);

effect_config!(
    /// Configuration for the scroll bar effect.
    ScrollBarConfig {
        width: i32 = 0,
        thumb_radius: f32 = 0.4,
        track_opacity: f32 = 0.6,
        hover_brightness: f32 = 1.4,
    }
);

effect_config!(
    /// Configuration for the scroll line spacing effect.
    ScrollLineSpacingConfig {
        enabled: bool = false,
        max: f32 = 6.0,
        duration_ms: u32 = 200,
    }
);

effect_config!(
    /// Configuration for the scroll momentum effect.
    ScrollMomentumConfig {
        enabled: bool = false,
        fade_ms: u32 = 300,
        width: f32 = 3.0,
    }
);

effect_config!(
    /// Configuration for the scroll progress effect.
    ScrollProgressConfig {
        enabled: bool = false,
        height: f32 = 2.0,
        color: (f32, f32, f32) = (0.4, 0.6, 1.0),
        opacity: f32 = 0.8,
    }
);

effect_config!(
    /// Configuration for the scroll velocity fade effect.
    ScrollVelocityFadeConfig {
        enabled: bool = false,
        max_opacity: f32 = 0.15,
        ms: u32 = 300,
    }
);

effect_config!(
    /// Configuration for the search pulse effect.
    SearchPulseConfig {
        enabled: bool = false,
        face_id: FaceId = FaceId::new(0),
    }
);

effect_config!(
    /// Configuration for the show whitespace effect.
    ShowWhitespaceConfig {
        enabled: bool = false,
        color: (f32, f32, f32, f32) = (0.4, 0.4, 0.4, 0.3),
    }
);

effect_config!(
    /// Configuration for the sine wave effect.
    SineWaveConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.3, 0.7, 1.0),
        amplitude: f32 = 20.0,
        wavelength: f32 = 80.0,
        speed: f32 = 1.0,
        opacity: f32 = 0.06,
    }
);

effect_config!(
    /// Configuration for the spiral vortex effect.
    SpiralVortexConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.2, 0.8),
        arms: u32 = 4,
        speed: f32 = 0.5,
        opacity: f32 = 0.1,
    }
);

effect_config!(
    /// Configuration for the stained glass effect.
    StainedGlassConfig {
        enabled: bool = false,
        opacity: f32 = 0.08,
        saturation: f32 = 0.6,
    }
);

effect_config!(
    /// Configuration for the sunburst pattern effect.
    SunburstPatternConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.8, 0.3),
        ray_count: u32 = 12,
        speed: f32 = 0.5,
        opacity: f32 = 0.08,
    }
);

effect_config!(
    /// Configuration for the target reticle effect.
    TargetReticleConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.2, 0.8, 0.2),
        ring_count: u32 = 3,
        pulse_speed: f32 = 1.0,
        opacity: f32 = 0.08,
    }
);

effect_config!(
    /// Configuration for the tessellation effect.
    TessellationConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.5, 0.5, 0.7),
        tile_size: f32 = 40.0,
        rotation: f32 = 0.0,
        opacity: f32 = 0.04,
    }
);

effect_config!(
    /// Configuration for the text fade in effect.
    TextFadeInConfig {
        enabled: bool = false,
        duration_ms: u32 = 150,
    }
);

effect_config!(
    /// Configuration for the theme transition effect.
    ThemeTransitionConfig {
        enabled: bool = false,
        duration: std::time::Duration = std::time::Duration::from_millis(300),
        easing: crate::TransitionEasing = crate::TransitionEasing::EaseOutQuad,
    }
);

effect_config!(
    /// Configuration for the title fade effect.
    TitleFadeConfig {
        enabled: bool = false,
        duration_ms: u32 = 300,
    }
);

effect_config!(
    /// Configuration for the topo contour effect.
    TopoContourConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.7, 0.5),
        spacing: f32 = 30.0,
        speed: f32 = 1.0,
        opacity: f32 = 0.1,
    }
);

effect_config!(
    /// Configuration for the trefoil knot effect.
    TrefoilKnotConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.6, 0.9),
        size: f32 = 80.0,
        rotation_speed: f32 = 1.0,
        opacity: f32 = 0.06,
    }
);

effect_config!(
    /// Configuration for the typing heatmap effect.
    TypingHeatmapConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (1.0, 0.4, 0.1),
        fade_ms: u32 = 2000,
        opacity: f32 = 0.15,
    }
);

effect_config!(
    /// Configuration for the typing ripple effect.
    TypingRippleConfig {
        enabled: bool = false,
        max_radius: f32 = 40.0,
        duration_ms: u32 = 300,
    }
);

effect_config!(
    /// Configuration for the typing speed effect.
    TypingSpeedConfig {
        enabled: bool = false,
    }
);

effect_config!(
    /// Configuration for the vignette effect.
    VignetteConfig {
        enabled: bool = false,
        intensity: f32 = 0.3,
        radius: f32 = 50.0,
    }
);

effect_config!(
    /// Configuration for the warp grid effect.
    WarpGridConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.3, 0.5, 0.9),
        density: u32 = 20,
        amplitude: f32 = 5.0,
        speed: f32 = 1.0,
        opacity: f32 = 0.15,
    }
);

effect_config!(
    /// Configuration for the wave interference effect.
    WaveInterferenceConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.3, 0.5, 0.9),
        wavelength: f32 = 30.0,
        source_count: u32 = 3,
        speed: f32 = 1.0,
        opacity: f32 = 0.08,
    }
);

effect_config!(
    /// Configuration for the window border radius effect.
    WindowBorderRadiusConfig {
        enabled: bool = false,
        radius: f32 = 8.0,
        width: f32 = 1.0,
        color: (f32, f32, f32) = (0.5, 0.5, 0.5),
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the window content shadow effect.
    WindowContentShadowConfig {
        enabled: bool = false,
        size: f32 = 6.0,
        opacity: f32 = 0.15,
    }
);

effect_config!(
    /// Configuration for the window glow effect.
    WindowGlowConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.4, 0.6, 1.0),
        radius: f32 = 8.0,
        intensity: f32 = 0.4,
    }
);

effect_config!(
    /// Configuration for the window mode tint effect.
    WindowModeTintConfig {
        enabled: bool = false,
        opacity: f32 = 0.03,
    }
);

effect_config!(
    /// Configuration for the window switch fade effect.
    WindowSwitchFadeConfig {
        enabled: bool = false,
        duration_ms: u32 = 200,
        intensity: f32 = 0.15,
    }
);

effect_config!(
    /// Configuration for the window watermark effect.
    WindowWatermarkConfig {
        enabled: bool = false,
        opacity: f32 = 0.08,
        threshold: u32 = 10,
    }
);

effect_config!(
    /// Configuration for the wrap indicator effect.
    WrapIndicatorConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.5, 0.6, 0.8),
        opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the zen mode effect.
    ZenModeConfig {
        enabled: bool = false,
        content_width_pct: f32 = 60.0,
        margin_opacity: f32 = 0.3,
    }
);

effect_config!(
    /// Configuration for the zigzag pattern effect.
    ZigzagPatternConfig {
        enabled: bool = false,
        color: (f32, f32, f32) = (0.6, 0.4, 1.0),
        amplitude: f32 = 15.0,
        frequency: f32 = 0.1,
        speed: f32 = 1.0,
        opacity: f32 = 0.06,
    }
);

#[cfg(test)]
#[path = "effect_config_test.rs"]
mod tests;

/// Container for all visual effect configurations.
///
/// Shared between `RenderApp` (stores for persistence/replay)
/// and `WgpuRenderer` (uses for actual rendering).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EffectsConfig {
    pub accent_strip: AccentStripConfig,
    pub argyle_pattern: ArgylePatternConfig,
    pub aurora: AuroraConfig,
    pub basket_weave: BasketWeaveConfig,
    pub bg_gradient: BgGradientConfig,
    pub bg_pattern: BgPatternConfig,
    pub border_transition: BorderTransitionConfig,
    pub breadcrumb: BreadcrumbConfig,
    pub breathing_border: BreathingBorderConfig,
    pub brick_wall: BrickWallConfig,
    pub celtic_knot: CelticKnotConfig,
    pub chevron_pattern: ChevronPatternConfig,
    pub circuit_trace: CircuitTraceConfig,
    pub click_halo: ClickHaloConfig,
    pub concentric_rings: ConcentricRingsConfig,
    pub constellation: ConstellationConfig,
    pub corner_fold: CornerFoldConfig,
    pub crosshatch_pattern: CrosshatchPatternConfig,
    pub cursor_aurora_borealis: CursorAuroraBorealisConfig,
    pub cursor_bubble: CursorBubbleConfig,
    pub cursor_candle_flame: CursorCandleFlameConfig,
    pub cursor_color_cycle: CursorColorCycleConfig,
    pub cursor_comet: CursorCometConfig,
    pub cursor_compass: CursorCompassConfig,
    pub cursor_compass_needle: CursorCompassNeedleConfig,
    pub cursor_crosshair: CursorCrosshairConfig,
    pub cursor_crystal: CursorCrystalConfig,
    pub cursor_dna_helix: CursorDnaHelixConfig,
    pub cursor_elastic_snap: CursorElasticSnapConfig,
    pub cursor_error_pulse: CursorErrorPulseConfig,
    pub cursor_feather: CursorFeatherConfig,
    pub cursor_firework: CursorFireworkConfig,
    pub cursor_flame: CursorFlameConfig,
    pub cursor_galaxy: CursorGalaxyConfig,
    pub cursor_ghost: CursorGhostConfig,
    pub cursor_glow: CursorGlowConfig,
    pub cursor_gravity_well: CursorGravityWellConfig,
    pub cursor_heartbeat: CursorHeartbeatConfig,
    pub cursor_lighthouse: CursorLighthouseConfig,
    pub cursor_lightning: CursorLightningConfig,
    pub cursor_magnetism: CursorMagnetismConfig,
    pub cursor_metronome: CursorMetronomeConfig,
    pub cursor_moth: CursorMothConfig,
    pub cursor_moth_flame: CursorMothFlameConfig,
    pub cursor_orbit_particles: CursorOrbitParticlesConfig,
    pub cursor_particles: CursorParticlesConfig,
    pub cursor_pendulum: CursorPendulumConfig,
    pub cursor_pixel_dust: CursorPixelDustConfig,
    pub cursor_plasma_ball: CursorPlasmaBallConfig,
    pub cursor_portal: CursorPortalConfig,
    pub cursor_prism: CursorPrismConfig,
    pub cursor_pulse: CursorPulseConfig,
    pub cursor_quill_pen: CursorQuillPenConfig,
    pub cursor_radar: CursorRadarConfig,
    pub cursor_ripple_ring: CursorRippleRingConfig,
    pub cursor_ripple_wave: CursorRippleWaveConfig,
    pub cursor_scope: CursorScopeConfig,
    pub cursor_shadow: CursorShadowConfig,
    pub cursor_shockwave: CursorShockwaveConfig,
    pub cursor_snowflake: CursorSnowflakeConfig,
    pub cursor_sonar_ping: CursorSonarPingConfig,
    pub cursor_sparkle_burst: CursorSparkleBurstConfig,
    pub cursor_sparkler: CursorSparklerConfig,
    pub cursor_spotlight: CursorSpotlightConfig,
    pub cursor_stardust: CursorStardustConfig,
    pub cursor_tornado: CursorTornadoConfig,
    pub cursor_trail_fade: CursorTrailFadeConfig,
    pub cursor_wake: CursorWakeConfig,
    pub cursor_water_drop: CursorWaterDropConfig,
    pub depth_shadow: DepthShadowConfig,
    pub diamond_lattice: DiamondLatticeConfig,
    pub dot_matrix: DotMatrixConfig,
    pub edge_glow: EdgeGlowConfig,
    pub edge_snap: EdgeSnapConfig,
    pub fish_scale: FishScaleConfig,
    pub focus_gradient_border: FocusGradientBorderConfig,
    pub focus_mode: FocusModeConfig,
    pub focus_ring: FocusRingConfig,
    pub frost_border: FrostBorderConfig,
    pub frosted_border: FrostedBorderConfig,
    pub frosted_glass: FrostedGlassConfig,
    pub guilloche: GuillocheConfig,
    pub header_shadow: HeaderShadowConfig,
    pub heat_distortion: HeatDistortionConfig,
    pub herringbone_pattern: HerringbonePatternConfig,
    pub hex_grid: HexGridConfig,
    pub honeycomb_dissolve: HoneycombDissolveConfig,
    pub idle_dim: IdleDimConfig,
    pub inactive_dim: InactiveDimConfig,
    pub inactive_tint: InactiveTintConfig,
    pub indent_guides: IndentGuidesConfig,
    pub kaleidoscope: KaleidoscopeConfig,
    pub lightning_bolt: LightningBoltConfig,
    pub line_animation: LineAnimationConfig,
    pub line_highlight: LineHighlightConfig,
    pub line_number_pulse: LineNumberPulseConfig,
    pub matrix_rain: MatrixRainConfig,
    pub minibuffer_highlight: MinibufferHighlightConfig,
    pub minimap: MinimapConfig,
    pub mode_line_gradient: ModeLineGradientConfig,
    pub mode_line_separator: ModeLineSeparatorConfig,
    pub mode_line_transition: ModeLineTransitionConfig,
    pub modified_indicator: ModifiedIndicatorConfig,
    pub moire_pattern: MoirePatternConfig,
    pub neon_border: NeonBorderConfig,
    pub noise_field: NoiseFieldConfig,
    pub noise_grain: NoiseGrainConfig,
    pub padding_gradient: PaddingGradientConfig,
    pub plaid_pattern: PlaidPatternConfig,
    pub plasma_border: PlasmaBorderConfig,
    pub prism_edge: PrismEdgeConfig,
    pub rain_effect: RainEffectConfig,
    pub region_glow: RegionGlowConfig,
    pub resize_padding: ResizePaddingConfig,
    pub rotating_gear: RotatingGearConfig,
    pub scanlines: ScanlinesConfig,
    pub scroll_bar: ScrollBarConfig,
    pub scroll_line_spacing: ScrollLineSpacingConfig,
    pub scroll_momentum: ScrollMomentumConfig,
    pub scroll_progress: ScrollProgressConfig,
    pub scroll_velocity_fade: ScrollVelocityFadeConfig,
    pub search_pulse: SearchPulseConfig,
    pub show_whitespace: ShowWhitespaceConfig,
    pub sine_wave: SineWaveConfig,
    pub spiral_vortex: SpiralVortexConfig,
    pub stained_glass: StainedGlassConfig,
    pub sunburst_pattern: SunburstPatternConfig,
    pub target_reticle: TargetReticleConfig,
    pub tessellation: TessellationConfig,
    pub text_fade_in: TextFadeInConfig,
    pub theme_transition: ThemeTransitionConfig,
    pub title_fade: TitleFadeConfig,
    pub topo_contour: TopoContourConfig,
    pub trefoil_knot: TrefoilKnotConfig,
    pub typing_heatmap: TypingHeatmapConfig,
    pub typing_ripple: TypingRippleConfig,
    pub typing_speed: TypingSpeedConfig,
    pub vignette: VignetteConfig,
    pub warp_grid: WarpGridConfig,
    pub wave_interference: WaveInterferenceConfig,
    pub window_border_radius: WindowBorderRadiusConfig,
    pub window_content_shadow: WindowContentShadowConfig,
    pub window_glow: WindowGlowConfig,
    pub window_mode_tint: WindowModeTintConfig,
    pub window_switch_fade: WindowSwitchFadeConfig,
    pub window_watermark: WindowWatermarkConfig,
    pub wrap_indicator: WrapIndicatorConfig,
    pub zen_mode: ZenModeConfig,
    pub zigzag_pattern: ZigzagPatternConfig,
}

/// Look an effect's schema up by the name Lisp uses.
///
/// A table rather than something derived from `EffectsConfig`, because that
/// struct is hand-written and serde reflection carries values, not types. The
/// risk of a hand-maintained table is that it drifts from the struct beside it,
/// so `every_effect_name_has_a_schema` walks `effect_names()` and fails if any
/// effect is missing one — which is the same guarantee at a fraction of the
/// churn of macro-generating a 149-field struct.
#[must_use]
pub fn schema_for(effect: &str) -> Option<&'static [PropertySchema]> {
    let rust_name = effect.replace('-', "_");
    schema_by_field(&rust_name)
}

macro_rules! effect_schema_table {
    ($($field:ident => $ty:ty,)*) => {
        fn schema_by_field(field: &str) -> Option<&'static [PropertySchema]> {
            match field {
                $(stringify!($field) => Some(<$ty>::PROPERTIES),)*
                // The behavioural slots are not fields of `EffectsConfig`; they
                // sit beside it on `VisualConfig`.
                "cursor_blink" => Some(crate::visual_config::CursorBlinkConfig::PROPERTIES),
                "cursor_motion" => Some(crate::visual_config::CursorMotionConfig::PROPERTIES),
                "cursor_size_transition" => {
                    Some(crate::visual_config::CursorSizeTransitionConfig::PROPERTIES)
                }
                "buffer_transition" => Some(crate::visual_config::BufferTransitionConfig::PROPERTIES),
                "scroll_transition" => Some(crate::visual_config::ScrollTransitionConfig::PROPERTIES),
                "window_animations" => {
                    Some(crate::window_animation::WindowAnimationsConfig::PROPERTIES)
                }
                "window_open" | "window_close" | "window_resize" | "window_movement" => {
                    Some(crate::window_animation::WindowAnimation::PROPERTIES)
                }
                _ => None,
            }
        }
    };
}

effect_schema_table! {
    accent_strip => AccentStripConfig,
    argyle_pattern => ArgylePatternConfig,
    aurora => AuroraConfig,
    basket_weave => BasketWeaveConfig,
    bg_gradient => BgGradientConfig,
    bg_pattern => BgPatternConfig,
    border_transition => BorderTransitionConfig,
    breadcrumb => BreadcrumbConfig,
    breathing_border => BreathingBorderConfig,
    brick_wall => BrickWallConfig,
    celtic_knot => CelticKnotConfig,
    chevron_pattern => ChevronPatternConfig,
    circuit_trace => CircuitTraceConfig,
    click_halo => ClickHaloConfig,
    concentric_rings => ConcentricRingsConfig,
    constellation => ConstellationConfig,
    corner_fold => CornerFoldConfig,
    crosshatch_pattern => CrosshatchPatternConfig,
    cursor_aurora_borealis => CursorAuroraBorealisConfig,
    cursor_bubble => CursorBubbleConfig,
    cursor_candle_flame => CursorCandleFlameConfig,
    cursor_color_cycle => CursorColorCycleConfig,
    cursor_comet => CursorCometConfig,
    cursor_compass => CursorCompassConfig,
    cursor_compass_needle => CursorCompassNeedleConfig,
    cursor_crosshair => CursorCrosshairConfig,
    cursor_crystal => CursorCrystalConfig,
    cursor_dna_helix => CursorDnaHelixConfig,
    cursor_elastic_snap => CursorElasticSnapConfig,
    cursor_error_pulse => CursorErrorPulseConfig,
    cursor_feather => CursorFeatherConfig,
    cursor_firework => CursorFireworkConfig,
    cursor_flame => CursorFlameConfig,
    cursor_galaxy => CursorGalaxyConfig,
    cursor_ghost => CursorGhostConfig,
    cursor_glow => CursorGlowConfig,
    cursor_gravity_well => CursorGravityWellConfig,
    cursor_heartbeat => CursorHeartbeatConfig,
    cursor_lighthouse => CursorLighthouseConfig,
    cursor_lightning => CursorLightningConfig,
    cursor_magnetism => CursorMagnetismConfig,
    cursor_metronome => CursorMetronomeConfig,
    cursor_moth => CursorMothConfig,
    cursor_moth_flame => CursorMothFlameConfig,
    cursor_orbit_particles => CursorOrbitParticlesConfig,
    cursor_particles => CursorParticlesConfig,
    cursor_pendulum => CursorPendulumConfig,
    cursor_pixel_dust => CursorPixelDustConfig,
    cursor_plasma_ball => CursorPlasmaBallConfig,
    cursor_portal => CursorPortalConfig,
    cursor_prism => CursorPrismConfig,
    cursor_pulse => CursorPulseConfig,
    cursor_quill_pen => CursorQuillPenConfig,
    cursor_radar => CursorRadarConfig,
    cursor_ripple_ring => CursorRippleRingConfig,
    cursor_ripple_wave => CursorRippleWaveConfig,
    cursor_scope => CursorScopeConfig,
    cursor_shadow => CursorShadowConfig,
    cursor_shockwave => CursorShockwaveConfig,
    cursor_snowflake => CursorSnowflakeConfig,
    cursor_sonar_ping => CursorSonarPingConfig,
    cursor_sparkle_burst => CursorSparkleBurstConfig,
    cursor_sparkler => CursorSparklerConfig,
    cursor_spotlight => CursorSpotlightConfig,
    cursor_stardust => CursorStardustConfig,
    cursor_tornado => CursorTornadoConfig,
    cursor_trail_fade => CursorTrailFadeConfig,
    cursor_wake => CursorWakeConfig,
    cursor_water_drop => CursorWaterDropConfig,
    depth_shadow => DepthShadowConfig,
    diamond_lattice => DiamondLatticeConfig,
    dot_matrix => DotMatrixConfig,
    edge_glow => EdgeGlowConfig,
    edge_snap => EdgeSnapConfig,
    fish_scale => FishScaleConfig,
    focus_gradient_border => FocusGradientBorderConfig,
    focus_mode => FocusModeConfig,
    focus_ring => FocusRingConfig,
    frost_border => FrostBorderConfig,
    frosted_border => FrostedBorderConfig,
    frosted_glass => FrostedGlassConfig,
    guilloche => GuillocheConfig,
    header_shadow => HeaderShadowConfig,
    heat_distortion => HeatDistortionConfig,
    herringbone_pattern => HerringbonePatternConfig,
    hex_grid => HexGridConfig,
    honeycomb_dissolve => HoneycombDissolveConfig,
    idle_dim => IdleDimConfig,
    inactive_dim => InactiveDimConfig,
    inactive_tint => InactiveTintConfig,
    indent_guides => IndentGuidesConfig,
    kaleidoscope => KaleidoscopeConfig,
    lightning_bolt => LightningBoltConfig,
    line_animation => LineAnimationConfig,
    line_highlight => LineHighlightConfig,
    line_number_pulse => LineNumberPulseConfig,
    matrix_rain => MatrixRainConfig,
    minibuffer_highlight => MinibufferHighlightConfig,
    minimap => MinimapConfig,
    mode_line_gradient => ModeLineGradientConfig,
    mode_line_separator => ModeLineSeparatorConfig,
    mode_line_transition => ModeLineTransitionConfig,
    modified_indicator => ModifiedIndicatorConfig,
    moire_pattern => MoirePatternConfig,
    neon_border => NeonBorderConfig,
    noise_field => NoiseFieldConfig,
    noise_grain => NoiseGrainConfig,
    padding_gradient => PaddingGradientConfig,
    plaid_pattern => PlaidPatternConfig,
    plasma_border => PlasmaBorderConfig,
    prism_edge => PrismEdgeConfig,
    rain_effect => RainEffectConfig,
    region_glow => RegionGlowConfig,
    resize_padding => ResizePaddingConfig,
    rotating_gear => RotatingGearConfig,
    scanlines => ScanlinesConfig,
    scroll_bar => ScrollBarConfig,
    scroll_line_spacing => ScrollLineSpacingConfig,
    scroll_momentum => ScrollMomentumConfig,
    scroll_progress => ScrollProgressConfig,
    scroll_velocity_fade => ScrollVelocityFadeConfig,
    search_pulse => SearchPulseConfig,
    show_whitespace => ShowWhitespaceConfig,
    sine_wave => SineWaveConfig,
    spiral_vortex => SpiralVortexConfig,
    stained_glass => StainedGlassConfig,
    sunburst_pattern => SunburstPatternConfig,
    target_reticle => TargetReticleConfig,
    tessellation => TessellationConfig,
    text_fade_in => TextFadeInConfig,
    theme_transition => ThemeTransitionConfig,
    title_fade => TitleFadeConfig,
    topo_contour => TopoContourConfig,
    trefoil_knot => TrefoilKnotConfig,
    typing_heatmap => TypingHeatmapConfig,
    typing_ripple => TypingRippleConfig,
    typing_speed => TypingSpeedConfig,
    vignette => VignetteConfig,
    warp_grid => WarpGridConfig,
    wave_interference => WaveInterferenceConfig,
    window_border_radius => WindowBorderRadiusConfig,
    window_content_shadow => WindowContentShadowConfig,
    window_glow => WindowGlowConfig,
    window_mode_tint => WindowModeTintConfig,
    window_switch_fade => WindowSwitchFadeConfig,
    window_watermark => WindowWatermarkConfig,
    wrap_indicator => WrapIndicatorConfig,
    zen_mode => ZenModeConfig,
    zigzag_pattern => ZigzagPatternConfig,
}
