//! Effect configuration types for the neomacs display engine.
//!
//! Each visual effect has its own config struct with all its parameters.
//! All configs are grouped in `EffectsConfig` which is shared between
//! `RenderApp` and `WgpuRenderer`.

use std::time::Duration;

/// Macro for defining effect config structs with Default implementations.
///
macro_rules! effect_config {
    (
        $(#[$meta:meta])*
        $name:ident {
            $($field:ident : $ty:ty = $default:expr),*
            $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug)]
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
    /// Configuration for Gaussian blur post-processing effect.
    /// Uses a separable two-pass approach (horizontal + vertical) for efficiency.
    BlurConfig {
        enabled: bool = false,
        radius: f32 = 1.0,
        passes: u32 = 2,
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
    CursorColorCycleConfig {
        enabled: bool = false,
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
        face_id: u32 = 0,
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
        face_id: u32 = 0,
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
mod tests {
    use super::*;
    use std::time::Duration;

    // ── Helper: assert a config is Clone + Debug ───────────────────────
    fn assert_clone_debug<T: Clone + std::fmt::Debug>(v: &T) {
        let _ = v.clone();
        let _ = format!("{:?}", v);
    }

    // ── AccentStripConfig ──────────────────────────────────────────────
    #[test]
    fn accent_strip_defaults() {
        let c = AccentStripConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.width, 3.0);
        assert_clone_debug(&c);
    }

    // ── ArgylePatternConfig ────────────────────────────────────────────
    #[test]
    fn argyle_pattern_defaults() {
        let c = ArgylePatternConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.5, 0.3, 0.3));
        assert_eq!(c.diamond_size, 30.0);
        assert_eq!(c.line_width, 1.0);
        assert_eq!(c.opacity, 0.05);
        assert_clone_debug(&c);
    }

    // ── AuroraConfig ──────────────────────────────────────────────────
    #[test]
    fn aurora_defaults() {
        let c = AuroraConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color1, (0.2, 0.8, 0.4));
        assert_eq!(c.color2, (0.3, 0.4, 0.9));
        assert_eq!(c.height, 60.0);
        assert_eq!(c.speed, 1.0);
        assert_eq!(c.opacity, 0.12);
        assert_clone_debug(&c);
    }

    // ── BasketWeaveConfig ─────────────────────────────────────────────
    #[test]
    fn basket_weave_defaults() {
        let c = BasketWeaveConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.55, 0.4, 0.25));
        assert_eq!(c.strip_width, 6.0);
        assert_eq!(c.strip_spacing, 20.0);
        assert_eq!(c.opacity, 0.05);
        assert_clone_debug(&c);
    }

    // ── BgGradientConfig ──────────────────────────────────────────────
    #[test]
    fn bg_gradient_defaults() {
        let c = BgGradientConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.top, (0.0, 0.0, 0.0));
        assert_eq!(c.bottom, (0.0, 0.0, 0.0));
        assert_clone_debug(&c);
    }

    // ── BgPatternConfig ───────────────────────────────────────────────
    #[test]
    fn bg_pattern_defaults() {
        let c = BgPatternConfig::default();
        assert_eq!(c.style, 0);
        assert_eq!(c.spacing, 20.0);
        assert_eq!(c.color, (0.5, 0.5, 0.5));
        assert_eq!(c.opacity, 0.05);
        assert_clone_debug(&c);
    }

    // ── BorderTransitionConfig ────────────────────────────────────────
    #[test]
    fn border_transition_defaults() {
        let c = BorderTransitionConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.active_color, (0.4, 0.6, 1.0));
        assert_eq!(c.duration_ms, 200);
        assert_clone_debug(&c);
    }

    // ── BreadcrumbConfig ──────────────────────────────────────────────
    #[test]
    fn breadcrumb_defaults() {
        let c = BreadcrumbConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.opacity, 0.7);
        assert_clone_debug(&c);
    }

    // ── BreathingBorderConfig ─────────────────────────────────────────
    #[test]
    fn breathing_border_defaults() {
        let c = BreathingBorderConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.5, 0.5, 0.5));
        assert_eq!(c.min_opacity, 0.05);
        assert_eq!(c.max_opacity, 0.3);
        assert_eq!(c.cycle_ms, 3000);
        assert_clone_debug(&c);
    }

    // ── BrickWallConfig ───────────────────────────────────────────────
    #[test]
    fn brick_wall_defaults() {
        let c = BrickWallConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.6, 0.4, 0.3));
        assert_eq!(c.width, 40.0);
        assert_eq!(c.height, 20.0);
        assert_eq!(c.opacity, 0.06);
        assert_clone_debug(&c);
    }

    // ── CelticKnotConfig ──────────────────────────────────────────────
    #[test]
    fn celtic_knot_defaults() {
        let c = CelticKnotConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.0, 0.6, 0.3));
        assert_eq!(c.scale, 60.0);
        assert_eq!(c.weave_speed, 1.0);
        assert_eq!(c.opacity, 0.06);
        assert_clone_debug(&c);
    }

    // ── ChevronPatternConfig ──────────────────────────────────────────
    #[test]
    fn chevron_pattern_defaults() {
        let c = ChevronPatternConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.7, 0.5));
        assert_eq!(c.spacing, 40.0);
        assert_eq!(c.speed, 0.5);
        assert_eq!(c.opacity, 0.06);
        assert_clone_debug(&c);
    }

    // ── CircuitTraceConfig ────────────────────────────────────────────
    #[test]
    fn circuit_trace_defaults() {
        let c = CircuitTraceConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.2, 0.8, 0.4));
        assert_eq!(c.width, 2.0);
        assert_eq!(c.speed, 1.0);
        assert_eq!(c.opacity, 0.2);
        assert_clone_debug(&c);
    }

    // ── ClickHaloConfig ───────────────────────────────────────────────
    #[test]
    fn click_halo_defaults() {
        let c = ClickHaloConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.6, 1.0));
        assert_eq!(c.duration_ms, 300);
        assert_eq!(c.max_radius, 30.0);
        assert_clone_debug(&c);
    }

    // ── ConcentricRingsConfig ─────────────────────────────────────────
    #[test]
    fn concentric_rings_defaults() {
        let c = ConcentricRingsConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.6, 1.0));
        assert_eq!(c.spacing, 30.0);
        assert_eq!(c.expansion_speed, 1.0);
        assert_eq!(c.opacity, 0.06);
        assert_clone_debug(&c);
    }

    // ── ConstellationConfig ───────────────────────────────────────────
    #[test]
    fn constellation_defaults() {
        let c = ConstellationConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.7, 0.8, 1.0));
        assert_eq!(c.star_count, 50);
        assert_eq!(c.connect_dist, 80.0);
        assert_eq!(c.twinkle_speed, 1.0);
        assert_eq!(c.opacity, 0.15);
        assert_clone_debug(&c);
    }

    // ── CornerFoldConfig ──────────────────────────────────────────────
    #[test]
    fn corner_fold_defaults() {
        let c = CornerFoldConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.size, 20.0);
        assert_eq!(c.color, (0.6, 0.4, 0.2));
        assert_eq!(c.opacity, 0.5);
        assert_clone_debug(&c);
    }

    // ── CrosshatchPatternConfig ───────────────────────────────────────
    #[test]
    fn crosshatch_pattern_defaults() {
        let c = CrosshatchPatternConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.5, 0.6, 0.4));
        assert_eq!(c.line_spacing, 20.0);
        assert_eq!(c.angle, 45.0);
        assert_eq!(c.speed, 0.3);
        assert_eq!(c.opacity, 0.06);
        assert_clone_debug(&c);
    }

    // ── CursorAuroraBorealisConfig ────────────────────────────────────
    #[test]
    fn cursor_aurora_borealis_defaults() {
        let c = CursorAuroraBorealisConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.2, 0.9, 0.5));
        assert_eq!(c.band_count, 5);
        assert_eq!(c.shimmer_speed, 1.0);
        assert_eq!(c.opacity, 0.15);
        assert_clone_debug(&c);
    }

    // ── CursorBubbleConfig ────────────────────────────────────────────
    #[test]
    fn cursor_bubble_defaults() {
        let c = CursorBubbleConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.8, 1.0));
        assert_eq!(c.count, 6);
        assert_eq!(c.rise_speed, 80.0);
        assert_eq!(c.opacity, 0.2);
        assert_clone_debug(&c);
    }

    // ── CursorCandleFlameConfig ───────────────────────────────────────
    #[test]
    fn cursor_candle_flame_defaults() {
        let c = CursorCandleFlameConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.7, 0.2));
        assert_eq!(c.height, 20);
        assert_eq!(c.flicker_speed, 1.0);
        assert_eq!(c.opacity, 0.2);
        assert_clone_debug(&c);
    }

    // ── CursorColorCycleConfig ────────────────────────────────────────
    #[test]
    fn cursor_color_cycle_defaults() {
        let c = CursorColorCycleConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.speed, 0.5);
        assert_eq!(c.saturation, 0.8);
        assert_eq!(c.lightness, 0.6);
        assert_clone_debug(&c);
    }

    // ── CursorCometConfig ─────────────────────────────────────────────
    #[test]
    fn cursor_comet_defaults() {
        let c = CursorCometConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.trail_length, 5);
        assert_eq!(c.fade_ms, 300);
        assert_eq!(c.color, (0.5, 0.7, 1.0));
        assert_eq!(c.opacity, 0.6);
        assert_clone_debug(&c);
    }

    // ── CursorCompassConfig ───────────────────────────────────────────
    #[test]
    fn cursor_compass_defaults() {
        let c = CursorCompassConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.9, 0.6, 0.2));
        assert_eq!(c.size, 20.0);
        assert_eq!(c.speed, 1.0);
        assert_eq!(c.opacity, 0.25);
        assert_clone_debug(&c);
    }

    // ── CursorCompassNeedleConfig ─────────────────────────────────────
    #[test]
    fn cursor_compass_needle_defaults() {
        let c = CursorCompassNeedleConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.3, 0.3));
        assert_eq!(c.length, 20.0);
        assert_eq!(c.spin_speed, 2.0);
        assert_eq!(c.opacity, 0.2);
        assert_clone_debug(&c);
    }

    // ── CursorCrosshairConfig ─────────────────────────────────────────
    #[test]
    fn cursor_crosshair_defaults() {
        let c = CursorCrosshairConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.5, 0.5, 0.5));
        assert_eq!(c.opacity, 0.15);
        assert_clone_debug(&c);
    }

    // ── CursorCrystalConfig ───────────────────────────────────────────
    #[test]
    fn cursor_crystal_defaults() {
        let c = CursorCrystalConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.7, 0.9, 1.0));
        assert_eq!(c.facet_count, 6);
        assert_eq!(c.radius, 25.0);
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── CursorDnaHelixConfig ──────────────────────────────────────────
    #[test]
    fn cursor_dna_helix_defaults() {
        let c = CursorDnaHelixConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color1, (0.3, 0.9, 0.5));
        assert_eq!(c.color2, (0.5, 0.3, 0.9));
        assert_eq!(c.radius, 12.0);
        assert_eq!(c.speed, 1.5);
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── CursorElasticSnapConfig ───────────────────────────────────────
    #[test]
    fn cursor_elastic_snap_defaults() {
        let c = CursorElasticSnapConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.overshoot, 0.15);
        assert_eq!(c.duration_ms, 200);
        assert_clone_debug(&c);
    }

    // ── CursorErrorPulseConfig ────────────────────────────────────────
    #[test]
    fn cursor_error_pulse_defaults() {
        let c = CursorErrorPulseConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.2, 0.2));
        assert_eq!(c.duration_ms, 250);
        assert_clone_debug(&c);
    }

    // ── CursorFeatherConfig ───────────────────────────────────────────
    #[test]
    fn cursor_feather_defaults() {
        let c = CursorFeatherConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.9, 0.85, 0.7));
        assert_eq!(c.count, 4);
        assert_eq!(c.drift_speed, 1.0);
        assert_eq!(c.opacity, 0.18);
        assert_clone_debug(&c);
    }

    // ── CursorFireworkConfig ──────────────────────────────────────────
    #[test]
    fn cursor_firework_defaults() {
        let c = CursorFireworkConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.6, 0.2));
        assert_eq!(c.particle_count, 16);
        assert_eq!(c.burst_radius, 60.0);
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── CursorFlameConfig ─────────────────────────────────────────────
    #[test]
    fn cursor_flame_defaults() {
        let c = CursorFlameConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.5, 0.1));
        assert_eq!(c.particle_count, 10);
        assert_eq!(c.height, 40.0);
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── CursorGalaxyConfig ────────────────────────────────────────────
    #[test]
    fn cursor_galaxy_defaults() {
        let c = CursorGalaxyConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.8, 0.8, 1.0));
        assert_eq!(c.star_count, 30);
        assert_eq!(c.radius, 30.0);
        assert_eq!(c.opacity, 0.2);
        assert_clone_debug(&c);
    }

    // ── CursorGhostConfig ─────────────────────────────────────────────
    #[test]
    fn cursor_ghost_defaults() {
        let c = CursorGhostConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.5, 0.5, 1.0));
        assert_eq!(c.count, 4);
        assert_eq!(c.fade_ms, 600);
        assert_eq!(c.drift, 20.0);
        assert_eq!(c.opacity, 0.4);
        assert_clone_debug(&c);
    }

    // ── CursorGlowConfig ─────────────────────────────────────────────
    #[test]
    fn cursor_glow_defaults() {
        let c = CursorGlowConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.6, 1.0));
        assert_eq!(c.radius, 30.0);
        assert_eq!(c.opacity, 0.15);
        assert_clone_debug(&c);
    }

    // ── CursorGravityWellConfig ───────────────────────────────────────
    #[test]
    fn cursor_gravity_well_defaults() {
        let c = CursorGravityWellConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.3, 0.6, 1.0));
        assert_eq!(c.field_radius, 80.0);
        assert_eq!(c.line_count, 8);
        assert_eq!(c.opacity, 0.2);
        assert_clone_debug(&c);
    }

    // ── CursorHeartbeatConfig ─────────────────────────────────────────
    #[test]
    fn cursor_heartbeat_defaults() {
        let c = CursorHeartbeatConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.3, 0.3));
        assert_eq!(c.bpm, 72.0);
        assert_eq!(c.max_radius, 50.0);
        assert_eq!(c.opacity, 0.2);
        assert_clone_debug(&c);
    }

    // ── CursorLighthouseConfig ────────────────────────────────────────
    #[test]
    fn cursor_lighthouse_defaults() {
        let c = CursorLighthouseConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.9, 0.3));
        assert_eq!(c.beam_width, 15.0);
        assert_eq!(c.rotation_speed, 0.5);
        assert_eq!(c.beam_length, 200.0);
        assert_eq!(c.opacity, 0.12);
        assert_clone_debug(&c);
    }

    // ── CursorLightningConfig ─────────────────────────────────────────
    #[test]
    fn cursor_lightning_defaults() {
        let c = CursorLightningConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.6, 0.8, 1.0));
        assert_eq!(c.bolt_count, 4);
        assert_eq!(c.max_length, 50.0);
        assert_eq!(c.opacity, 0.4);
        assert_clone_debug(&c);
    }

    // ── CursorMagnetismConfig ─────────────────────────────────────────
    #[test]
    fn cursor_magnetism_defaults() {
        let c = CursorMagnetismConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.7, 1.0));
        assert_eq!(c.ring_count, 3);
        assert_eq!(c.duration_ms, 300);
        assert_eq!(c.opacity, 0.5);
        assert_clone_debug(&c);
    }

    // ── CursorMetronomeConfig ─────────────────────────────────────────
    #[test]
    fn cursor_metronome_defaults() {
        let c = CursorMetronomeConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.9, 0.5, 0.2));
        assert_eq!(c.tick_height, 20.0);
        assert_eq!(c.fade_ms, 300);
        assert_eq!(c.opacity, 0.4);
        assert_clone_debug(&c);
    }

    // ── CursorMothConfig ──────────────────────────────────────────────
    #[test]
    fn cursor_moth_defaults() {
        let c = CursorMothConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.9, 0.8, 0.5));
        assert_eq!(c.count, 5);
        assert_eq!(c.wing_size, 8.0);
        assert_eq!(c.opacity, 0.2);
        assert_clone_debug(&c);
    }

    // ── CursorMothFlameConfig ─────────────────────────────────────────
    #[test]
    fn cursor_moth_flame_defaults() {
        let c = CursorMothFlameConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.8, 0.7, 0.5));
        assert_eq!(c.moth_count, 5);
        assert_eq!(c.orbit_speed, 1.0);
        assert_eq!(c.opacity, 0.18);
        assert_clone_debug(&c);
    }

    // ── CursorOrbitParticlesConfig ────────────────────────────────────
    #[test]
    fn cursor_orbit_particles_defaults() {
        let c = CursorOrbitParticlesConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.8, 0.3));
        assert_eq!(c.count, 6);
        assert_eq!(c.radius, 25.0);
        assert_eq!(c.speed, 1.5);
        assert_eq!(c.opacity, 0.35);
        assert_clone_debug(&c);
    }

    // ── CursorParticlesConfig ─────────────────────────────────────────
    #[test]
    fn cursor_particles_defaults() {
        let c = CursorParticlesConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.6, 0.2));
        assert_eq!(c.count, 6);
        assert_eq!(c.lifetime_ms, 800);
        assert_eq!(c.gravity, 120.0);
        assert_clone_debug(&c);
    }

    // ── CursorPendulumConfig ──────────────────────────────────────────
    #[test]
    fn cursor_pendulum_defaults() {
        let c = CursorPendulumConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.9, 0.7, 0.3));
        assert_eq!(c.arc_length, 40.0);
        assert_eq!(c.damping, 0.5);
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── CursorPixelDustConfig ─────────────────────────────────────────
    #[test]
    fn cursor_pixel_dust_defaults() {
        let c = CursorPixelDustConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.8, 0.8, 0.6));
        assert_eq!(c.count, 15);
        assert_eq!(c.scatter_speed, 1.0);
        assert_eq!(c.opacity, 0.2);
        assert_clone_debug(&c);
    }

    // ── CursorPlasmaBallConfig ────────────────────────────────────────
    #[test]
    fn cursor_plasma_ball_defaults() {
        let c = CursorPlasmaBallConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.7, 0.3, 1.0));
        assert_eq!(c.tendril_count, 6);
        assert_eq!(c.arc_speed, 1.0);
        assert_eq!(c.opacity, 0.2);
        assert_clone_debug(&c);
    }

    // ── CursorPortalConfig ────────────────────────────────────────────
    #[test]
    fn cursor_portal_defaults() {
        let c = CursorPortalConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.6, 0.2, 0.9));
        assert_eq!(c.radius, 30.0);
        assert_eq!(c.speed, 2.0);
        assert_eq!(c.opacity, 0.25);
        assert_clone_debug(&c);
    }

    // ── CursorPrismConfig ─────────────────────────────────────────────
    #[test]
    fn cursor_prism_defaults() {
        let c = CursorPrismConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 1.0, 1.0));
        assert_eq!(c.ray_count, 7);
        assert_eq!(c.spread, 30.0);
        assert_eq!(c.opacity, 0.15);
        assert_clone_debug(&c);
    }

    // ── CursorPulseConfig ─────────────────────────────────────────────
    #[test]
    fn cursor_pulse_defaults() {
        let c = CursorPulseConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.speed, 1.0);
        assert_eq!(c.min_opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── CursorQuillPenConfig ──────────────────────────────────────────
    #[test]
    fn cursor_quill_pen_defaults() {
        let c = CursorQuillPenConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.3, 0.15, 0.05));
        assert_eq!(c.trail_length, 8);
        assert_eq!(c.ink_speed, 1.0);
        assert_eq!(c.opacity, 0.2);
        assert_clone_debug(&c);
    }

    // ── CursorRadarConfig ─────────────────────────────────────────────
    #[test]
    fn cursor_radar_defaults() {
        let c = CursorRadarConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.2, 0.9, 0.4));
        assert_eq!(c.radius, 40.0);
        assert_eq!(c.speed, 1.5);
        assert_eq!(c.opacity, 0.2);
        assert_clone_debug(&c);
    }

    // ── CursorRippleRingConfig ────────────────────────────────────────
    #[test]
    fn cursor_ripple_ring_defaults() {
        let c = CursorRippleRingConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.3, 0.8, 1.0));
        assert_eq!(c.max_radius, 60.0);
        assert_eq!(c.count, 3);
        assert_eq!(c.speed, 2.0);
        assert_eq!(c.opacity, 0.25);
        assert_clone_debug(&c);
    }

    // ── CursorRippleWaveConfig ────────────────────────────────────────
    #[test]
    fn cursor_ripple_wave_defaults() {
        let c = CursorRippleWaveConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.6, 1.0));
        assert_eq!(c.ring_count, 3);
        assert_eq!(c.max_radius, 80.0);
        assert_eq!(c.duration_ms, 500);
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── CursorScopeConfig ─────────────────────────────────────────────
    #[test]
    fn cursor_scope_defaults() {
        let c = CursorScopeConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.8, 0.2));
        assert_eq!(c.thickness, 1.0);
        assert_eq!(c.gap, 10.0);
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── CursorShadowConfig ────────────────────────────────────────────
    #[test]
    fn cursor_shadow_defaults() {
        let c = CursorShadowConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.offset_x, 2.0);
        assert_eq!(c.offset_y, 2.0);
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── CursorShockwaveConfig ─────────────────────────────────────────
    #[test]
    fn cursor_shockwave_defaults() {
        let c = CursorShockwaveConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.6, 0.2));
        assert_eq!(c.radius, 80.0);
        assert_eq!(c.decay, 2.0);
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── CursorSnowflakeConfig ─────────────────────────────────────────
    #[test]
    fn cursor_snowflake_defaults() {
        let c = CursorSnowflakeConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.8, 0.9, 1.0));
        assert_eq!(c.count, 8);
        assert_eq!(c.fall_speed, 30.0);
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── CursorSonarPingConfig ─────────────────────────────────────────
    #[test]
    fn cursor_sonar_ping_defaults() {
        let c = CursorSonarPingConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.3, 0.7, 1.0));
        assert_eq!(c.ring_count, 3);
        assert_eq!(c.max_radius, 60.0);
        assert_eq!(c.duration_ms, 600);
        assert_eq!(c.opacity, 0.25);
        assert_clone_debug(&c);
    }

    // ── CursorSparkleBurstConfig ──────────────────────────────────────
    #[test]
    fn cursor_sparkle_burst_defaults() {
        let c = CursorSparkleBurstConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.85, 0.3));
        assert_eq!(c.count, 12);
        assert_eq!(c.radius, 30.0);
        assert_eq!(c.opacity, 0.4);
        assert_clone_debug(&c);
    }

    // ── CursorSparklerConfig ──────────────────────────────────────────
    #[test]
    fn cursor_sparkler_defaults() {
        let c = CursorSparklerConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.85, 0.3));
        assert_eq!(c.spark_count, 12);
        assert_eq!(c.burn_speed, 1.0);
        assert_eq!(c.opacity, 0.25);
        assert_clone_debug(&c);
    }

    // ── CursorSpotlightConfig ─────────────────────────────────────────
    #[test]
    fn cursor_spotlight_defaults() {
        let c = CursorSpotlightConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.radius, 200.0);
        assert_eq!(c.intensity, 0.15);
        assert_eq!(c.color, (1.0, 1.0, 0.9));
        assert_clone_debug(&c);
    }

    // ── CursorStardustConfig ──────────────────────────────────────────
    #[test]
    fn cursor_stardust_defaults() {
        let c = CursorStardustConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.9, 0.5));
        assert_eq!(c.particle_count, 20);
        assert_eq!(c.fall_speed, 1.0);
        assert_eq!(c.opacity, 0.2);
        assert_clone_debug(&c);
    }

    // ── CursorTornadoConfig ───────────────────────────────────────────
    #[test]
    fn cursor_tornado_defaults() {
        let c = CursorTornadoConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.5, 0.7, 1.0));
        assert_eq!(c.radius, 40.0);
        assert_eq!(c.particle_count, 12);
        assert_eq!(c.opacity, 0.25);
        assert_clone_debug(&c);
    }

    // ── CursorTrailFadeConfig ─────────────────────────────────────────
    #[test]
    fn cursor_trail_fade_defaults() {
        let c = CursorTrailFadeConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.length, 8);
        assert_eq!(c.ms, 300);
        assert_clone_debug(&c);
    }

    // ── CursorWakeConfig ──────────────────────────────────────────────
    #[test]
    fn cursor_wake_defaults() {
        let c = CursorWakeConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.duration_ms, 120);
        assert_eq!(c.scale, 1.3);
        assert_clone_debug(&c);
    }

    // ── CursorWaterDropConfig ─────────────────────────────────────────
    #[test]
    fn cursor_water_drop_defaults() {
        let c = CursorWaterDropConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.3, 0.6, 0.9));
        assert_eq!(c.ripple_count, 4);
        assert_eq!(c.expand_speed, 1.0);
        assert_eq!(c.opacity, 0.15);
        assert_clone_debug(&c);
    }

    // ── DepthShadowConfig ─────────────────────────────────────────────
    #[test]
    fn depth_shadow_defaults() {
        let c = DepthShadowConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.layers, 3);
        assert_eq!(c.offset, 2.0);
        assert_eq!(c.color, (0.0, 0.0, 0.0));
        assert_eq!(c.opacity, 0.15);
        assert_clone_debug(&c);
    }

    // ── DiamondLatticeConfig ──────────────────────────────────────────
    #[test]
    fn diamond_lattice_defaults() {
        let c = DiamondLatticeConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.7, 0.5, 0.9));
        assert_eq!(c.cell_size, 30.0);
        assert_eq!(c.shimmer_speed, 0.8);
        assert_eq!(c.opacity, 0.08);
        assert_clone_debug(&c);
    }

    // ── DotMatrixConfig ───────────────────────────────────────────────
    #[test]
    fn dot_matrix_defaults() {
        let c = DotMatrixConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.3, 1.0, 0.3));
        assert_eq!(c.spacing, 12.0);
        assert_eq!(c.pulse_speed, 1.0);
        assert_eq!(c.opacity, 0.06);
        assert_clone_debug(&c);
    }

    // ── EdgeGlowConfig ────────────────────────────────────────────────
    #[test]
    fn edge_glow_defaults() {
        let c = EdgeGlowConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.6, 1.0));
        assert_eq!(c.height, 40.0);
        assert_eq!(c.opacity, 0.3);
        assert_eq!(c.fade_ms, 400);
        assert_clone_debug(&c);
    }

    // ── EdgeSnapConfig ────────────────────────────────────────────────
    #[test]
    fn edge_snap_defaults() {
        let c = EdgeSnapConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.5, 0.2));
        assert_eq!(c.duration_ms, 200);
        assert_clone_debug(&c);
    }

    // ── FishScaleConfig ───────────────────────────────────────────────
    #[test]
    fn fish_scale_defaults() {
        let c = FishScaleConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.3, 0.6, 0.7));
        assert_eq!(c.size, 16.0);
        assert_eq!(c.row_offset, 0.5);
        assert_eq!(c.opacity, 0.04);
        assert_clone_debug(&c);
    }

    // ── FocusGradientBorderConfig ─────────────────────────────────────
    #[test]
    fn focus_gradient_border_defaults() {
        let c = FocusGradientBorderConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.top_color, (0.3, 0.6, 1.0));
        assert_eq!(c.bot_color, (0.6, 0.3, 1.0));
        assert_eq!(c.width, 2.0);
        assert_eq!(c.opacity, 0.6);
        assert_clone_debug(&c);
    }

    // ── FocusModeConfig ───────────────────────────────────────────────
    #[test]
    fn focus_mode_defaults() {
        let c = FocusModeConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.opacity, 0.4);
        assert_clone_debug(&c);
    }

    // ── FocusRingConfig ───────────────────────────────────────────────
    #[test]
    fn focus_ring_defaults() {
        let c = FocusRingConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.6, 1.0));
        assert_eq!(c.opacity, 0.5);
        assert_eq!(c.dash_length, 8.0);
        assert_eq!(c.speed, 40.0);
        assert_clone_debug(&c);
    }

    // ── FrostBorderConfig ─────────────────────────────────────────────
    #[test]
    fn frost_border_defaults() {
        let c = FrostBorderConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.7, 0.85, 1.0));
        assert_eq!(c.width, 6.0);
        assert_eq!(c.opacity, 0.2);
        assert_clone_debug(&c);
    }

    // ── FrostedBorderConfig ───────────────────────────────────────────
    #[test]
    fn frosted_border_defaults() {
        let c = FrostedBorderConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.width, 4.0);
        assert_eq!(c.opacity, 0.15);
        assert_eq!(c.color, (1.0, 1.0, 1.0));
        assert_clone_debug(&c);
    }

    // ── FrostedGlassConfig ────────────────────────────────────────────
    #[test]
    fn frosted_glass_defaults() {
        let c = FrostedGlassConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.opacity, 0.3);
        assert_eq!(c.blur, 4.0);
        assert_clone_debug(&c);
    }

    // ── GuillocheConfig ───────────────────────────────────────────────
    #[test]
    fn guilloche_defaults() {
        let c = GuillocheConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.6, 0.4, 0.7));
        assert_eq!(c.curve_count, 8);
        assert_eq!(c.wave_freq, 1.0);
        assert_eq!(c.opacity, 0.05);
        assert_clone_debug(&c);
    }

    // ── HeaderShadowConfig ────────────────────────────────────────────
    #[test]
    fn header_shadow_defaults() {
        let c = HeaderShadowConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.intensity, 0.3);
        assert_eq!(c.size, 6.0);
        assert_clone_debug(&c);
    }

    // ── HeatDistortionConfig ──────────────────────────────────────────
    #[test]
    fn heat_distortion_defaults() {
        let c = HeatDistortionConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.intensity, 0.3);
        assert_eq!(c.speed, 1.0);
        assert_eq!(c.edge_width, 30.0);
        assert_eq!(c.opacity, 0.15);
        assert_clone_debug(&c);
    }

    // ── HerringbonePatternConfig ──────────────────────────────────────
    #[test]
    fn herringbone_pattern_defaults() {
        let c = HerringbonePatternConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.6, 0.5, 0.4));
        assert_eq!(c.tile_width, 20.0);
        assert_eq!(c.tile_height, 10.0);
        assert_eq!(c.opacity, 0.05);
        assert_clone_debug(&c);
    }

    // ── HexGridConfig ─────────────────────────────────────────────────
    #[test]
    fn hex_grid_defaults() {
        let c = HexGridConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.3, 0.6, 0.9));
        assert_eq!(c.cell_size, 40.0);
        assert_eq!(c.pulse_speed, 1.0);
        assert_eq!(c.opacity, 0.1);
        assert_clone_debug(&c);
    }

    // ── HoneycombDissolveConfig ───────────────────────────────────────
    #[test]
    fn honeycomb_dissolve_defaults() {
        let c = HoneycombDissolveConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.8, 0.6, 0.2));
        assert_eq!(c.cell_size, 30.0);
        assert_eq!(c.speed, 0.8);
        assert_eq!(c.opacity, 0.08);
        assert_clone_debug(&c);
    }

    // ── IdleDimConfig ─────────────────────────────────────────────────
    #[test]
    fn idle_dim_defaults() {
        let c = IdleDimConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.delay, Duration::from_secs(60));
        assert_eq!(c.opacity, 0.4);
        assert_eq!(c.fade_duration, Duration::from_millis(500));
        assert_clone_debug(&c);
    }

    // ── InactiveDimConfig ─────────────────────────────────────────────
    #[test]
    fn inactive_dim_defaults() {
        let c = InactiveDimConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.opacity, 0.15);
        assert_clone_debug(&c);
    }

    // ── InactiveTintConfig ────────────────────────────────────────────
    #[test]
    fn inactive_tint_defaults() {
        let c = InactiveTintConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.2, 0.1, 0.0));
        assert_eq!(c.opacity, 0.1);
        assert_clone_debug(&c);
    }

    // ── IndentGuidesConfig ────────────────────────────────────────────
    #[test]
    fn indent_guides_defaults() {
        let c = IndentGuidesConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.3, 0.3, 0.3, 0.3));
        assert_eq!(c.rainbow_enabled, false);
        assert!(c.rainbow_colors.is_empty());
        assert_clone_debug(&c);
    }

    // ── KaleidoscopeConfig ────────────────────────────────────────────
    #[test]
    fn kaleidoscope_defaults() {
        let c = KaleidoscopeConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.6, 0.3, 0.9));
        assert_eq!(c.segments, 6);
        assert_eq!(c.speed, 0.5);
        assert_eq!(c.opacity, 0.1);
        assert_clone_debug(&c);
    }

    // ── LightningBoltConfig ───────────────────────────────────────────
    #[test]
    fn lightning_bolt_defaults() {
        let c = LightningBoltConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.7, 0.8, 1.0));
        assert_eq!(c.frequency, 1.0);
        assert_eq!(c.intensity, 0.8);
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── LineAnimationConfig ───────────────────────────────────────────
    #[test]
    fn line_animation_defaults() {
        let c = LineAnimationConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.duration_ms, 150);
        assert_clone_debug(&c);
    }

    // ── LineHighlightConfig ───────────────────────────────────────────
    #[test]
    fn line_highlight_defaults() {
        let c = LineHighlightConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.2, 0.2, 0.3, 0.15));
        assert_clone_debug(&c);
    }

    // ── LineNumberPulseConfig ─────────────────────────────────────────
    #[test]
    fn line_number_pulse_defaults() {
        let c = LineNumberPulseConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.6, 1.0));
        assert_eq!(c.intensity, 0.3);
        assert_eq!(c.cycle_ms, 2000);
        assert_clone_debug(&c);
    }

    // ── MatrixRainConfig ──────────────────────────────────────────────
    #[test]
    fn matrix_rain_defaults() {
        let c = MatrixRainConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.0, 0.8, 0.2));
        assert_eq!(c.column_count, 40);
        assert_eq!(c.speed, 150.0);
        assert_eq!(c.opacity, 0.12);
        assert_clone_debug(&c);
    }

    // ── MinibufferHighlightConfig ─────────────────────────────────────
    #[test]
    fn minibuffer_highlight_defaults() {
        let c = MinibufferHighlightConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.6, 1.0));
        assert_eq!(c.opacity, 0.25);
        assert_clone_debug(&c);
    }

    // ── MinimapConfig ─────────────────────────────────────────────────
    #[test]
    fn minimap_defaults() {
        let c = MinimapConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.width, 80.0);
        assert_clone_debug(&c);
    }

    // ── ModeLineGradientConfig ────────────────────────────────────────
    #[test]
    fn mode_line_gradient_defaults() {
        let c = ModeLineGradientConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.left_color, (0.2, 0.3, 0.5));
        assert_eq!(c.right_color, (0.5, 0.3, 0.2));
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── ModeLineSeparatorConfig ───────────────────────────────────────
    #[test]
    fn mode_line_separator_defaults() {
        let c = ModeLineSeparatorConfig::default();
        assert_eq!(c.style, 0);
        assert_eq!(c.color, (0.0, 0.0, 0.0));
        assert_eq!(c.height, 3.0);
        assert_clone_debug(&c);
    }

    // ── ModeLineTransitionConfig ──────────────────────────────────────
    #[test]
    fn mode_line_transition_defaults() {
        let c = ModeLineTransitionConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.duration_ms, 200);
        assert_clone_debug(&c);
    }

    // ── ModifiedIndicatorConfig ───────────────────────────────────────
    #[test]
    fn modified_indicator_defaults() {
        let c = ModifiedIndicatorConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.6, 0.2));
        assert_eq!(c.width, 3.0);
        assert_eq!(c.opacity, 0.8);
        assert_clone_debug(&c);
    }

    // ── MoirePatternConfig ────────────────────────────────────────────
    #[test]
    fn moire_pattern_defaults() {
        let c = MoirePatternConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.5, 0.5, 0.8));
        assert_eq!(c.line_spacing, 8.0);
        assert_eq!(c.angle_offset, 5.0);
        assert_eq!(c.speed, 0.3);
        assert_eq!(c.opacity, 0.06);
        assert_clone_debug(&c);
    }

    // ── NeonBorderConfig ──────────────────────────────────────────────
    #[test]
    fn neon_border_defaults() {
        let c = NeonBorderConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.0, 1.0, 0.8));
        assert_eq!(c.intensity, 0.6);
        assert_eq!(c.flicker, 0.1);
        assert_eq!(c.thickness, 3.0);
        assert_eq!(c.opacity, 0.4);
        assert_clone_debug(&c);
    }

    // ── NoiseFieldConfig ──────────────────────────────────────────────
    #[test]
    fn noise_field_defaults() {
        let c = NoiseFieldConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.5, 0.7, 0.3));
        assert_eq!(c.scale, 50.0);
        assert_eq!(c.speed, 0.5);
        assert_eq!(c.opacity, 0.08);
        assert_clone_debug(&c);
    }

    // ── NoiseGrainConfig ──────────────────────────────────────────────
    #[test]
    fn noise_grain_defaults() {
        let c = NoiseGrainConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.intensity, 0.03);
        assert_eq!(c.size, 2.0);
        assert_clone_debug(&c);
    }

    // ── PaddingGradientConfig ─────────────────────────────────────────
    #[test]
    fn padding_gradient_defaults() {
        let c = PaddingGradientConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.0, 0.0, 0.0));
        assert_eq!(c.opacity, 0.15);
        assert_eq!(c.width, 8.0);
        assert_clone_debug(&c);
    }

    // ── PlaidPatternConfig ────────────────────────────────────────────
    #[test]
    fn plaid_pattern_defaults() {
        let c = PlaidPatternConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.7, 0.3, 0.3));
        assert_eq!(c.band_width, 4.0);
        assert_eq!(c.band_spacing, 30.0);
        assert_eq!(c.opacity, 0.05);
        assert_clone_debug(&c);
    }

    // ── PlasmaBorderConfig ────────────────────────────────────────────
    #[test]
    fn plasma_border_defaults() {
        let c = PlasmaBorderConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color1, (1.0, 0.2, 0.5));
        assert_eq!(c.color2, (0.2, 0.5, 1.0));
        assert_eq!(c.width, 4.0);
        assert_eq!(c.speed, 1.0);
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── PrismEdgeConfig ───────────────────────────────────────────────
    #[test]
    fn prism_edge_defaults() {
        let c = PrismEdgeConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.width, 6.0);
        assert_eq!(c.speed, 1.0);
        assert_eq!(c.saturation, 0.8);
        assert_eq!(c.opacity, 0.25);
        assert_clone_debug(&c);
    }

    // ── RainEffectConfig ──────────────────────────────────────────────
    #[test]
    fn rain_effect_defaults() {
        let c = RainEffectConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.5, 0.6, 0.8));
        assert_eq!(c.drop_count, 30);
        assert_eq!(c.speed, 120.0);
        assert_eq!(c.opacity, 0.15);
        assert_clone_debug(&c);
    }

    // ── RegionGlowConfig ─────────────────────────────────────────────
    #[test]
    fn region_glow_defaults() {
        let c = RegionGlowConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.face_id, 0);
        assert_eq!(c.radius, 6.0);
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── ResizePaddingConfig ───────────────────────────────────────────
    #[test]
    fn resize_padding_defaults() {
        let c = ResizePaddingConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.duration_ms, 200);
        assert_eq!(c.max, 12.0);
        assert_clone_debug(&c);
    }

    // ── RotatingGearConfig ────────────────────────────────────────────
    #[test]
    fn rotating_gear_defaults() {
        let c = RotatingGearConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.6, 0.7, 0.8));
        assert_eq!(c.size, 40.0);
        assert_eq!(c.speed, 0.5);
        assert_eq!(c.opacity, 0.08);
        assert_clone_debug(&c);
    }

    // ── ScanlinesConfig ───────────────────────────────────────────────
    #[test]
    fn scanlines_defaults() {
        let c = ScanlinesConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.spacing, 2);
        assert_eq!(c.opacity, 0.08);
        assert_eq!(c.color, (0.0, 0.0, 0.0));
        assert_clone_debug(&c);
    }

    // ── ScrollBarConfig ───────────────────────────────────────────────
    #[test]
    fn scroll_bar_defaults() {
        let c = ScrollBarConfig::default();
        assert_eq!(c.width, 0);
        assert_eq!(c.thumb_radius, 0.4);
        assert_eq!(c.track_opacity, 0.6);
        assert_eq!(c.hover_brightness, 1.4);
        assert_clone_debug(&c);
    }

    // ── ScrollLineSpacingConfig ───────────────────────────────────────
    #[test]
    fn scroll_line_spacing_defaults() {
        let c = ScrollLineSpacingConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.max, 6.0);
        assert_eq!(c.duration_ms, 200);
        assert_clone_debug(&c);
    }

    // ── ScrollMomentumConfig ──────────────────────────────────────────
    #[test]
    fn scroll_momentum_defaults() {
        let c = ScrollMomentumConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.fade_ms, 300);
        assert_eq!(c.width, 3.0);
        assert_clone_debug(&c);
    }

    // ── ScrollProgressConfig ──────────────────────────────────────────
    #[test]
    fn scroll_progress_defaults() {
        let c = ScrollProgressConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.height, 2.0);
        assert_eq!(c.color, (0.4, 0.6, 1.0));
        assert_eq!(c.opacity, 0.8);
        assert_clone_debug(&c);
    }

    // ── ScrollVelocityFadeConfig ──────────────────────────────────────
    #[test]
    fn scroll_velocity_fade_defaults() {
        let c = ScrollVelocityFadeConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.max_opacity, 0.15);
        assert_eq!(c.ms, 300);
        assert_clone_debug(&c);
    }

    // ── SearchPulseConfig ─────────────────────────────────────────────
    #[test]
    fn search_pulse_defaults() {
        let c = SearchPulseConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.face_id, 0);
        assert_clone_debug(&c);
    }

    // ── ShowWhitespaceConfig ──────────────────────────────────────────
    #[test]
    fn show_whitespace_defaults() {
        let c = ShowWhitespaceConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.4, 0.4, 0.3));
        assert_clone_debug(&c);
    }

    // ── SineWaveConfig ────────────────────────────────────────────────
    #[test]
    fn sine_wave_defaults() {
        let c = SineWaveConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.3, 0.7, 1.0));
        assert_eq!(c.amplitude, 20.0);
        assert_eq!(c.wavelength, 80.0);
        assert_eq!(c.speed, 1.0);
        assert_eq!(c.opacity, 0.06);
        assert_clone_debug(&c);
    }

    // ── SpiralVortexConfig ────────────────────────────────────────────
    #[test]
    fn spiral_vortex_defaults() {
        let c = SpiralVortexConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.2, 0.8));
        assert_eq!(c.arms, 4);
        assert_eq!(c.speed, 0.5);
        assert_eq!(c.opacity, 0.1);
        assert_clone_debug(&c);
    }

    // ── StainedGlassConfig ────────────────────────────────────────────
    #[test]
    fn stained_glass_defaults() {
        let c = StainedGlassConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.opacity, 0.08);
        assert_eq!(c.saturation, 0.6);
        assert_clone_debug(&c);
    }

    // ── SunburstPatternConfig ─────────────────────────────────────────
    #[test]
    fn sunburst_pattern_defaults() {
        let c = SunburstPatternConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.8, 0.3));
        assert_eq!(c.ray_count, 12);
        assert_eq!(c.speed, 0.5);
        assert_eq!(c.opacity, 0.08);
        assert_clone_debug(&c);
    }

    // ── TargetReticleConfig ───────────────────────────────────────────
    #[test]
    fn target_reticle_defaults() {
        let c = TargetReticleConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.2, 0.8, 0.2));
        assert_eq!(c.ring_count, 3);
        assert_eq!(c.pulse_speed, 1.0);
        assert_eq!(c.opacity, 0.08);
        assert_clone_debug(&c);
    }

    // ── TessellationConfig ────────────────────────────────────────────
    #[test]
    fn tessellation_defaults() {
        let c = TessellationConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.5, 0.5, 0.7));
        assert_eq!(c.tile_size, 40.0);
        assert_eq!(c.rotation, 0.0);
        assert_eq!(c.opacity, 0.04);
        assert_clone_debug(&c);
    }

    // ── TextFadeInConfig ──────────────────────────────────────────────
    #[test]
    fn text_fade_in_defaults() {
        let c = TextFadeInConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.duration_ms, 150);
        assert_clone_debug(&c);
    }

    // ── ThemeTransitionConfig ─────────────────────────────────────────
    #[test]
    fn theme_transition_defaults() {
        let c = ThemeTransitionConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.duration, Duration::from_millis(300));
        assert_clone_debug(&c);
    }

    // ── TitleFadeConfig ───────────────────────────────────────────────
    #[test]
    fn title_fade_defaults() {
        let c = TitleFadeConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.duration_ms, 300);
        assert_clone_debug(&c);
    }

    // ── TopoContourConfig ─────────────────────────────────────────────
    #[test]
    fn topo_contour_defaults() {
        let c = TopoContourConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.7, 0.5));
        assert_eq!(c.spacing, 30.0);
        assert_eq!(c.speed, 1.0);
        assert_eq!(c.opacity, 0.1);
        assert_clone_debug(&c);
    }

    // ── TrefoilKnotConfig ─────────────────────────────────────────────
    #[test]
    fn trefoil_knot_defaults() {
        let c = TrefoilKnotConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.6, 0.9));
        assert_eq!(c.size, 80.0);
        assert_eq!(c.rotation_speed, 1.0);
        assert_eq!(c.opacity, 0.06);
        assert_clone_debug(&c);
    }

    // ── TypingHeatmapConfig ───────────────────────────────────────────
    #[test]
    fn typing_heatmap_defaults() {
        let c = TypingHeatmapConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (1.0, 0.4, 0.1));
        assert_eq!(c.fade_ms, 2000);
        assert_eq!(c.opacity, 0.15);
        assert_clone_debug(&c);
    }

    // ── TypingRippleConfig ────────────────────────────────────────────
    #[test]
    fn typing_ripple_defaults() {
        let c = TypingRippleConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.max_radius, 40.0);
        assert_eq!(c.duration_ms, 300);
        assert_clone_debug(&c);
    }

    // ── TypingSpeedConfig ─────────────────────────────────────────────
    #[test]
    fn typing_speed_defaults() {
        let c = TypingSpeedConfig::default();
        assert_eq!(c.enabled, false);
        assert_clone_debug(&c);
    }

    // ── VignetteConfig ────────────────────────────────────────────────
    #[test]
    fn vignette_defaults() {
        let c = VignetteConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.intensity, 0.3);
        assert_eq!(c.radius, 50.0);
        assert_clone_debug(&c);
    }

    // ── WarpGridConfig ────────────────────────────────────────────────
    #[test]
    fn warp_grid_defaults() {
        let c = WarpGridConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.3, 0.5, 0.9));
        assert_eq!(c.density, 20);
        assert_eq!(c.amplitude, 5.0);
        assert_eq!(c.speed, 1.0);
        assert_eq!(c.opacity, 0.15);
        assert_clone_debug(&c);
    }

    // ── WaveInterferenceConfig ────────────────────────────────────────
    #[test]
    fn wave_interference_defaults() {
        let c = WaveInterferenceConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.3, 0.5, 0.9));
        assert_eq!(c.wavelength, 30.0);
        assert_eq!(c.source_count, 3);
        assert_eq!(c.speed, 1.0);
        assert_eq!(c.opacity, 0.08);
        assert_clone_debug(&c);
    }

    // ── WindowBorderRadiusConfig ──────────────────────────────────────
    #[test]
    fn window_border_radius_defaults() {
        let c = WindowBorderRadiusConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.radius, 8.0);
        assert_eq!(c.width, 1.0);
        assert_eq!(c.color, (0.5, 0.5, 0.5));
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── WindowContentShadowConfig ─────────────────────────────────────
    #[test]
    fn window_content_shadow_defaults() {
        let c = WindowContentShadowConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.size, 6.0);
        assert_eq!(c.opacity, 0.15);
        assert_clone_debug(&c);
    }

    // ── WindowGlowConfig ──────────────────────────────────────────────
    #[test]
    fn window_glow_defaults() {
        let c = WindowGlowConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.4, 0.6, 1.0));
        assert_eq!(c.radius, 8.0);
        assert_eq!(c.intensity, 0.4);
        assert_clone_debug(&c);
    }

    // ── WindowModeTintConfig ──────────────────────────────────────────
    #[test]
    fn window_mode_tint_defaults() {
        let c = WindowModeTintConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.opacity, 0.03);
        assert_clone_debug(&c);
    }

    // ── WindowSwitchFadeConfig ────────────────────────────────────────
    #[test]
    fn window_switch_fade_defaults() {
        let c = WindowSwitchFadeConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.duration_ms, 200);
        assert_eq!(c.intensity, 0.15);
        assert_clone_debug(&c);
    }

    // ── WindowWatermarkConfig ─────────────────────────────────────────
    #[test]
    fn window_watermark_defaults() {
        let c = WindowWatermarkConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.opacity, 0.08);
        assert_eq!(c.threshold, 10);
        assert_clone_debug(&c);
    }

    // ── WrapIndicatorConfig ───────────────────────────────────────────
    #[test]
    fn wrap_indicator_defaults() {
        let c = WrapIndicatorConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.5, 0.6, 0.8));
        assert_eq!(c.opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── ZenModeConfig ─────────────────────────────────────────────────
    #[test]
    fn zen_mode_defaults() {
        let c = ZenModeConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.content_width_pct, 60.0);
        assert_eq!(c.margin_opacity, 0.3);
        assert_clone_debug(&c);
    }

    // ── ZigzagPatternConfig ───────────────────────────────────────────
    #[test]
    fn zigzag_pattern_defaults() {
        let c = ZigzagPatternConfig::default();
        assert_eq!(c.enabled, false);
        assert_eq!(c.color, (0.6, 0.4, 1.0));
        assert_eq!(c.amplitude, 15.0);
        assert_eq!(c.frequency, 0.1);
        assert_eq!(c.speed, 1.0);
        assert_eq!(c.opacity, 0.06);
        assert_clone_debug(&c);
    }

    // ═══════════════════════════════════════════════════════════════════
    // EffectsConfig aggregate
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn effects_config_default_creates_all_sub_defaults() {
        let ec = EffectsConfig::default();
        // Spot-check that each sub-config has its expected default
        assert_eq!(ec.accent_strip.enabled, false);
        assert_eq!(ec.accent_strip.width, 3.0);
        assert_eq!(ec.argyle_pattern.diamond_size, 30.0);
        assert_eq!(ec.aurora.color1, (0.2, 0.8, 0.4));
        assert_eq!(ec.basket_weave.strip_width, 6.0);
        assert_eq!(ec.bg_gradient.top, (0.0, 0.0, 0.0));
        assert_eq!(ec.bg_pattern.style, 0);
        assert_eq!(ec.border_transition.duration_ms, 200);
        assert_eq!(ec.breadcrumb.opacity, 0.7);
        assert_eq!(ec.breathing_border.cycle_ms, 3000);
        assert_eq!(ec.brick_wall.width, 40.0);
        assert_eq!(ec.celtic_knot.scale, 60.0);
        assert_eq!(ec.chevron_pattern.spacing, 40.0);
        assert_eq!(ec.circuit_trace.width, 2.0);
        assert_eq!(ec.click_halo.max_radius, 30.0);
        assert_eq!(ec.concentric_rings.spacing, 30.0);
        assert_eq!(ec.constellation.star_count, 50);
        assert_eq!(ec.corner_fold.size, 20.0);
        assert_eq!(ec.crosshatch_pattern.angle, 45.0);
        assert_eq!(ec.cursor_aurora_borealis.band_count, 5);
        assert_eq!(ec.cursor_bubble.count, 6);
        assert_eq!(ec.cursor_candle_flame.height, 20);
        assert_eq!(ec.cursor_color_cycle.saturation, 0.8);
        assert_eq!(ec.cursor_comet.trail_length, 5);
        assert_eq!(ec.cursor_compass.size, 20.0);
        assert_eq!(ec.cursor_compass_needle.length, 20.0);
        assert_eq!(ec.cursor_crosshair.opacity, 0.15);
        assert_eq!(ec.cursor_crystal.facet_count, 6);
        assert_eq!(ec.cursor_dna_helix.radius, 12.0);
        assert_eq!(ec.cursor_elastic_snap.overshoot, 0.15);
        assert_eq!(ec.cursor_error_pulse.duration_ms, 250);
        assert_eq!(ec.cursor_feather.count, 4);
        assert_eq!(ec.cursor_firework.particle_count, 16);
        assert_eq!(ec.cursor_flame.particle_count, 10);
        assert_eq!(ec.cursor_galaxy.star_count, 30);
        assert_eq!(ec.cursor_ghost.count, 4);
        assert_eq!(ec.cursor_glow.radius, 30.0);
        assert_eq!(ec.cursor_gravity_well.field_radius, 80.0);
        assert_eq!(ec.cursor_heartbeat.bpm, 72.0);
        assert_eq!(ec.cursor_lighthouse.beam_length, 200.0);
        assert_eq!(ec.cursor_lightning.bolt_count, 4);
        assert_eq!(ec.cursor_magnetism.ring_count, 3);
        assert_eq!(ec.cursor_metronome.tick_height, 20.0);
        assert_eq!(ec.cursor_moth.wing_size, 8.0);
        assert_eq!(ec.cursor_moth_flame.moth_count, 5);
        assert_eq!(ec.cursor_orbit_particles.count, 6);
        assert_eq!(ec.cursor_particles.lifetime_ms, 800);
        assert_eq!(ec.cursor_pendulum.arc_length, 40.0);
        assert_eq!(ec.cursor_pixel_dust.count, 15);
        assert_eq!(ec.cursor_plasma_ball.tendril_count, 6);
        assert_eq!(ec.cursor_portal.radius, 30.0);
        assert_eq!(ec.cursor_prism.ray_count, 7);
        assert_eq!(ec.cursor_pulse.min_opacity, 0.3);
        assert_eq!(ec.cursor_quill_pen.trail_length, 8);
        assert_eq!(ec.cursor_radar.radius, 40.0);
        assert_eq!(ec.cursor_ripple_ring.count, 3);
        assert_eq!(ec.cursor_ripple_wave.ring_count, 3);
        assert_eq!(ec.cursor_scope.gap, 10.0);
        assert_eq!(ec.cursor_shadow.offset_x, 2.0);
        assert_eq!(ec.cursor_shockwave.decay, 2.0);
        assert_eq!(ec.cursor_snowflake.count, 8);
        assert_eq!(ec.cursor_sonar_ping.ring_count, 3);
        assert_eq!(ec.cursor_sparkle_burst.count, 12);
        assert_eq!(ec.cursor_sparkler.spark_count, 12);
        assert_eq!(ec.cursor_spotlight.radius, 200.0);
        assert_eq!(ec.cursor_stardust.particle_count, 20);
        assert_eq!(ec.cursor_tornado.particle_count, 12);
        assert_eq!(ec.cursor_trail_fade.length, 8);
        assert_eq!(ec.cursor_wake.duration_ms, 120);
        assert_eq!(ec.cursor_water_drop.ripple_count, 4);
        assert_eq!(ec.depth_shadow.layers, 3);
        assert_eq!(ec.diamond_lattice.cell_size, 30.0);
        assert_eq!(ec.dot_matrix.spacing, 12.0);
        assert_eq!(ec.edge_glow.height, 40.0);
        assert_eq!(ec.edge_snap.duration_ms, 200);
        assert_eq!(ec.fish_scale.size, 16.0);
        assert_eq!(ec.focus_gradient_border.width, 2.0);
        assert_eq!(ec.focus_mode.opacity, 0.4);
        assert_eq!(ec.focus_ring.dash_length, 8.0);
        assert_eq!(ec.frost_border.width, 6.0);
        assert_eq!(ec.frosted_border.width, 4.0);
        assert_eq!(ec.frosted_glass.blur, 4.0);
        assert_eq!(ec.guilloche.curve_count, 8);
        assert_eq!(ec.header_shadow.size, 6.0);
        assert_eq!(ec.heat_distortion.edge_width, 30.0);
        assert_eq!(ec.herringbone_pattern.tile_width, 20.0);
        assert_eq!(ec.hex_grid.cell_size, 40.0);
        assert_eq!(ec.honeycomb_dissolve.cell_size, 30.0);
        assert_eq!(ec.idle_dim.delay, Duration::from_secs(60));
        assert_eq!(ec.inactive_dim.opacity, 0.15);
        assert_eq!(ec.inactive_tint.opacity, 0.1);
        assert!(ec.indent_guides.rainbow_colors.is_empty());
        assert_eq!(ec.kaleidoscope.segments, 6);
        assert_eq!(ec.lightning_bolt.frequency, 1.0);
        assert_eq!(ec.line_animation.duration_ms, 150);
        assert_eq!(ec.line_highlight.color, (0.2, 0.2, 0.3, 0.15));
        assert_eq!(ec.line_number_pulse.cycle_ms, 2000);
        assert_eq!(ec.matrix_rain.column_count, 40);
        assert_eq!(ec.minibuffer_highlight.opacity, 0.25);
        assert_eq!(ec.minimap.width, 80.0);
        assert_eq!(ec.mode_line_gradient.opacity, 0.3);
        assert_eq!(ec.mode_line_separator.height, 3.0);
        assert_eq!(ec.mode_line_transition.duration_ms, 200);
        assert_eq!(ec.modified_indicator.width, 3.0);
        assert_eq!(ec.moire_pattern.line_spacing, 8.0);
        assert_eq!(ec.neon_border.thickness, 3.0);
        assert_eq!(ec.noise_field.scale, 50.0);
        assert_eq!(ec.noise_grain.intensity, 0.03);
        assert_eq!(ec.padding_gradient.width, 8.0);
        assert_eq!(ec.plaid_pattern.band_width, 4.0);
        assert_eq!(ec.plasma_border.width, 4.0);
        assert_eq!(ec.prism_edge.width, 6.0);
        assert_eq!(ec.rain_effect.drop_count, 30);
        assert_eq!(ec.region_glow.radius, 6.0);
        assert_eq!(ec.resize_padding.max, 12.0);
        assert_eq!(ec.rotating_gear.size, 40.0);
        assert_eq!(ec.scanlines.spacing, 2);
        assert_eq!(ec.scroll_bar.width, 0);
        assert_eq!(ec.scroll_line_spacing.max, 6.0);
        assert_eq!(ec.scroll_momentum.fade_ms, 300);
        assert_eq!(ec.scroll_progress.height, 2.0);
        assert_eq!(ec.scroll_velocity_fade.max_opacity, 0.15);
        assert_eq!(ec.search_pulse.face_id, 0);
        assert_eq!(ec.show_whitespace.color, (0.4, 0.4, 0.4, 0.3));
        assert_eq!(ec.sine_wave.amplitude, 20.0);
        assert_eq!(ec.spiral_vortex.arms, 4);
        assert_eq!(ec.stained_glass.saturation, 0.6);
        assert_eq!(ec.sunburst_pattern.ray_count, 12);
        assert_eq!(ec.target_reticle.ring_count, 3);
        assert_eq!(ec.tessellation.tile_size, 40.0);
        assert_eq!(ec.text_fade_in.duration_ms, 150);
        assert_eq!(ec.theme_transition.duration, Duration::from_millis(300));
        assert_eq!(ec.title_fade.duration_ms, 300);
        assert_eq!(ec.topo_contour.spacing, 30.0);
        assert_eq!(ec.trefoil_knot.size, 80.0);
        assert_eq!(ec.typing_heatmap.fade_ms, 2000);
        assert_eq!(ec.typing_ripple.max_radius, 40.0);
        assert_eq!(ec.typing_speed.enabled, false);
        assert_eq!(ec.vignette.intensity, 0.3);
        assert_eq!(ec.warp_grid.density, 20);
        assert_eq!(ec.wave_interference.source_count, 3);
        assert_eq!(ec.window_border_radius.radius, 8.0);
        assert_eq!(ec.window_content_shadow.size, 6.0);
        assert_eq!(ec.window_glow.radius, 8.0);
        assert_eq!(ec.window_mode_tint.opacity, 0.03);
        assert_eq!(ec.window_switch_fade.duration_ms, 200);
        assert_eq!(ec.window_watermark.threshold, 10);
        assert_eq!(ec.wrap_indicator.opacity, 0.3);
        assert_eq!(ec.zen_mode.content_width_pct, 60.0);
        assert_eq!(ec.zigzag_pattern.amplitude, 15.0);
    }

    #[test]
    fn effects_config_clone_is_independent() {
        let mut a = EffectsConfig::default();
        a.accent_strip.enabled = true;
        a.accent_strip.width = 99.0;
        a.aurora.opacity = 0.99;
        a.indent_guides.rainbow_colors.push((1.0, 0.0, 0.0, 1.0));

        let b = a.clone();
        assert_eq!(b.accent_strip.enabled, true);
        assert_eq!(b.accent_strip.width, 99.0);
        assert_eq!(b.aurora.opacity, 0.99);
        assert_eq!(b.indent_guides.rainbow_colors.len(), 1);
        assert_eq!(b.indent_guides.rainbow_colors[0], (1.0, 0.0, 0.0, 1.0));

        // Verify independence: mutating b should not affect a
        let mut b = b;
        b.accent_strip.width = 1.0;
        assert_eq!(a.accent_strip.width, 99.0);
    }

    #[test]
    fn effects_config_debug_format() {
        let ec = EffectsConfig::default();
        let dbg = format!("{:?}", ec);
        // Should contain the struct name and field references
        assert!(dbg.contains("EffectsConfig"));
        assert!(dbg.contains("accent_strip"));
        assert!(dbg.contains("zigzag_pattern"));
    }

    // ═══════════════════════════════════════════════════════════════════
    // Clone independence for individual configs
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn individual_config_clone_independence() {
        let mut a = CursorGlowConfig::default();
        a.enabled = true;
        a.radius = 100.0;
        let b = a.clone();
        assert_eq!(b.enabled, true);
        assert_eq!(b.radius, 100.0);
    }

    #[test]
    fn indent_guides_clone_with_vec() {
        let mut a = IndentGuidesConfig::default();
        a.rainbow_colors.push((1.0, 0.0, 0.0, 1.0));
        a.rainbow_colors.push((0.0, 1.0, 0.0, 0.5));
        let b = a.clone();
        assert_eq!(b.rainbow_colors.len(), 2);
        assert_eq!(b.rainbow_colors[0], (1.0, 0.0, 0.0, 1.0));
        assert_eq!(b.rainbow_colors[1], (0.0, 1.0, 0.0, 0.5));
    }

    // ═══════════════════════════════════════════════════════════════════
    // Mutation / field writability
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn config_fields_are_mutable() {
        let mut c = AuroraConfig::default();
        c.enabled = true;
        c.color1 = (1.0, 1.0, 1.0);
        c.color2 = (0.0, 0.0, 0.0);
        c.height = 999.0;
        c.speed = 42.0;
        c.opacity = 1.0;
        assert_eq!(c.enabled, true);
        assert_eq!(c.color1, (1.0, 1.0, 1.0));
        assert_eq!(c.height, 999.0);
    }

    // ═══════════════════════════════════════════════════════════════════
    // Edge cases: numeric fields sanity
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn all_opacity_defaults_are_in_zero_to_one() {
        // Spot-check a representative sample of opacity fields to ensure
        // they fall in the [0.0, 1.0] range.
        let ec = EffectsConfig::default();
        let opacities: Vec<f32> = vec![
            ec.argyle_pattern.opacity,
            ec.aurora.opacity,
            ec.basket_weave.opacity,
            ec.bg_pattern.opacity,
            ec.breadcrumb.opacity,
            ec.breathing_border.min_opacity,
            ec.breathing_border.max_opacity,
            ec.brick_wall.opacity,
            ec.celtic_knot.opacity,
            ec.chevron_pattern.opacity,
            ec.circuit_trace.opacity,
            ec.concentric_rings.opacity,
            ec.constellation.opacity,
            ec.corner_fold.opacity,
            ec.crosshatch_pattern.opacity,
            ec.cursor_aurora_borealis.opacity,
            ec.cursor_bubble.opacity,
            ec.cursor_candle_flame.opacity,
            ec.cursor_comet.opacity,
            ec.cursor_compass.opacity,
            ec.cursor_compass_needle.opacity,
            ec.cursor_crosshair.opacity,
            ec.cursor_crystal.opacity,
            ec.cursor_dna_helix.opacity,
            ec.cursor_feather.opacity,
            ec.cursor_firework.opacity,
            ec.cursor_flame.opacity,
            ec.cursor_galaxy.opacity,
            ec.cursor_ghost.opacity,
            ec.cursor_glow.opacity,
            ec.cursor_gravity_well.opacity,
            ec.cursor_heartbeat.opacity,
            ec.cursor_lighthouse.opacity,
            ec.cursor_lightning.opacity,
            ec.cursor_magnetism.opacity,
            ec.cursor_metronome.opacity,
            ec.cursor_moth.opacity,
            ec.cursor_moth_flame.opacity,
            ec.cursor_orbit_particles.opacity,
            ec.cursor_pendulum.opacity,
            ec.cursor_pixel_dust.opacity,
            ec.cursor_plasma_ball.opacity,
            ec.cursor_portal.opacity,
            ec.cursor_prism.opacity,
            ec.cursor_pulse.min_opacity,
            ec.cursor_quill_pen.opacity,
            ec.cursor_radar.opacity,
            ec.cursor_ripple_ring.opacity,
            ec.cursor_ripple_wave.opacity,
            ec.cursor_scope.opacity,
            ec.cursor_shadow.opacity,
            ec.cursor_shockwave.opacity,
            ec.cursor_snowflake.opacity,
            ec.cursor_sonar_ping.opacity,
            ec.cursor_sparkle_burst.opacity,
            ec.cursor_sparkler.opacity,
            ec.cursor_stardust.opacity,
            ec.cursor_tornado.opacity,
            ec.cursor_water_drop.opacity,
            ec.depth_shadow.opacity,
            ec.diamond_lattice.opacity,
            ec.dot_matrix.opacity,
            ec.edge_glow.opacity,
            ec.fish_scale.opacity,
            ec.focus_gradient_border.opacity,
            ec.focus_mode.opacity,
            ec.focus_ring.opacity,
            ec.frost_border.opacity,
            ec.frosted_border.opacity,
            ec.frosted_glass.opacity,
            ec.guilloche.opacity,
            ec.heat_distortion.opacity,
            ec.herringbone_pattern.opacity,
            ec.hex_grid.opacity,
            ec.honeycomb_dissolve.opacity,
            ec.idle_dim.opacity,
            ec.inactive_dim.opacity,
            ec.inactive_tint.opacity,
            ec.kaleidoscope.opacity,
            ec.lightning_bolt.opacity,
            ec.matrix_rain.opacity,
            ec.minibuffer_highlight.opacity,
            ec.mode_line_gradient.opacity,
            ec.modified_indicator.opacity,
            ec.moire_pattern.opacity,
            ec.neon_border.opacity,
            ec.noise_field.opacity,
            ec.noise_grain.intensity,
            ec.padding_gradient.opacity,
            ec.plaid_pattern.opacity,
            ec.plasma_border.opacity,
            ec.prism_edge.opacity,
            ec.rain_effect.opacity,
            ec.region_glow.opacity,
            ec.rotating_gear.opacity,
            ec.scanlines.opacity,
            ec.scroll_bar.track_opacity,
            ec.scroll_progress.opacity,
            ec.scroll_velocity_fade.max_opacity,
            ec.sine_wave.opacity,
            ec.spiral_vortex.opacity,
            ec.stained_glass.opacity,
            ec.sunburst_pattern.opacity,
            ec.target_reticle.opacity,
            ec.tessellation.opacity,
            ec.topo_contour.opacity,
            ec.trefoil_knot.opacity,
            ec.typing_heatmap.opacity,
            ec.vignette.intensity,
            ec.warp_grid.opacity,
            ec.wave_interference.opacity,
            ec.window_border_radius.opacity,
            ec.window_content_shadow.opacity,
            ec.window_mode_tint.opacity,
            ec.window_switch_fade.intensity,
            ec.window_watermark.opacity,
            ec.wrap_indicator.opacity,
            ec.zen_mode.margin_opacity,
            ec.zigzag_pattern.opacity,
        ];
        for (i, &o) in opacities.iter().enumerate() {
            assert!(
                (0.0..=1.0).contains(&o),
                "opacity index {} has value {} outside [0.0, 1.0]",
                i,
                o
            );
        }
    }

    #[test]
    fn all_enabled_fields_default_to_false() {
        let ec = EffectsConfig::default();
        // Exhaustive check of every config that has an `enabled` field
        let enabled_flags: Vec<bool> = vec![
            ec.accent_strip.enabled,
            ec.argyle_pattern.enabled,
            ec.aurora.enabled,
            ec.basket_weave.enabled,
            ec.bg_gradient.enabled,
            ec.border_transition.enabled,
            ec.breadcrumb.enabled,
            ec.breathing_border.enabled,
            ec.brick_wall.enabled,
            ec.celtic_knot.enabled,
            ec.chevron_pattern.enabled,
            ec.circuit_trace.enabled,
            ec.click_halo.enabled,
            ec.concentric_rings.enabled,
            ec.constellation.enabled,
            ec.corner_fold.enabled,
            ec.crosshatch_pattern.enabled,
            ec.cursor_aurora_borealis.enabled,
            ec.cursor_bubble.enabled,
            ec.cursor_candle_flame.enabled,
            ec.cursor_color_cycle.enabled,
            ec.cursor_comet.enabled,
            ec.cursor_compass.enabled,
            ec.cursor_compass_needle.enabled,
            ec.cursor_crosshair.enabled,
            ec.cursor_crystal.enabled,
            ec.cursor_dna_helix.enabled,
            ec.cursor_elastic_snap.enabled,
            ec.cursor_error_pulse.enabled,
            ec.cursor_feather.enabled,
            ec.cursor_firework.enabled,
            ec.cursor_flame.enabled,
            ec.cursor_galaxy.enabled,
            ec.cursor_ghost.enabled,
            ec.cursor_glow.enabled,
            ec.cursor_gravity_well.enabled,
            ec.cursor_heartbeat.enabled,
            ec.cursor_lighthouse.enabled,
            ec.cursor_lightning.enabled,
            ec.cursor_magnetism.enabled,
            ec.cursor_metronome.enabled,
            ec.cursor_moth.enabled,
            ec.cursor_moth_flame.enabled,
            ec.cursor_orbit_particles.enabled,
            ec.cursor_particles.enabled,
            ec.cursor_pendulum.enabled,
            ec.cursor_pixel_dust.enabled,
            ec.cursor_plasma_ball.enabled,
            ec.cursor_portal.enabled,
            ec.cursor_prism.enabled,
            ec.cursor_pulse.enabled,
            ec.cursor_quill_pen.enabled,
            ec.cursor_radar.enabled,
            ec.cursor_ripple_ring.enabled,
            ec.cursor_ripple_wave.enabled,
            ec.cursor_scope.enabled,
            ec.cursor_shadow.enabled,
            ec.cursor_shockwave.enabled,
            ec.cursor_snowflake.enabled,
            ec.cursor_sonar_ping.enabled,
            ec.cursor_sparkle_burst.enabled,
            ec.cursor_sparkler.enabled,
            ec.cursor_spotlight.enabled,
            ec.cursor_stardust.enabled,
            ec.cursor_tornado.enabled,
            ec.cursor_trail_fade.enabled,
            ec.cursor_wake.enabled,
            ec.cursor_water_drop.enabled,
            ec.depth_shadow.enabled,
            ec.diamond_lattice.enabled,
            ec.dot_matrix.enabled,
            ec.edge_glow.enabled,
            ec.edge_snap.enabled,
            ec.fish_scale.enabled,
            ec.focus_gradient_border.enabled,
            ec.focus_mode.enabled,
            ec.focus_ring.enabled,
            ec.frost_border.enabled,
            ec.frosted_border.enabled,
            ec.frosted_glass.enabled,
            ec.guilloche.enabled,
            ec.header_shadow.enabled,
            ec.heat_distortion.enabled,
            ec.herringbone_pattern.enabled,
            ec.hex_grid.enabled,
            ec.honeycomb_dissolve.enabled,
            ec.idle_dim.enabled,
            ec.inactive_dim.enabled,
            ec.inactive_tint.enabled,
            ec.indent_guides.enabled,
            ec.indent_guides.rainbow_enabled,
            ec.kaleidoscope.enabled,
            ec.lightning_bolt.enabled,
            ec.line_animation.enabled,
            ec.line_highlight.enabled,
            ec.line_number_pulse.enabled,
            ec.matrix_rain.enabled,
            ec.minibuffer_highlight.enabled,
            ec.minimap.enabled,
            ec.mode_line_gradient.enabled,
            ec.mode_line_transition.enabled,
            ec.modified_indicator.enabled,
            ec.moire_pattern.enabled,
            ec.neon_border.enabled,
            ec.noise_field.enabled,
            ec.noise_grain.enabled,
            ec.padding_gradient.enabled,
            ec.plaid_pattern.enabled,
            ec.plasma_border.enabled,
            ec.prism_edge.enabled,
            ec.rain_effect.enabled,
            ec.region_glow.enabled,
            ec.resize_padding.enabled,
            ec.rotating_gear.enabled,
            ec.scanlines.enabled,
            ec.scroll_line_spacing.enabled,
            ec.scroll_momentum.enabled,
            ec.scroll_progress.enabled,
            ec.scroll_velocity_fade.enabled,
            ec.search_pulse.enabled,
            ec.show_whitespace.enabled,
            ec.sine_wave.enabled,
            ec.spiral_vortex.enabled,
            ec.stained_glass.enabled,
            ec.sunburst_pattern.enabled,
            ec.target_reticle.enabled,
            ec.tessellation.enabled,
            ec.text_fade_in.enabled,
            ec.theme_transition.enabled,
            ec.title_fade.enabled,
            ec.topo_contour.enabled,
            ec.trefoil_knot.enabled,
            ec.typing_heatmap.enabled,
            ec.typing_ripple.enabled,
            ec.typing_speed.enabled,
            ec.vignette.enabled,
            ec.warp_grid.enabled,
            ec.wave_interference.enabled,
            ec.window_border_radius.enabled,
            ec.window_content_shadow.enabled,
            ec.window_glow.enabled,
            ec.window_mode_tint.enabled,
            ec.window_switch_fade.enabled,
            ec.window_watermark.enabled,
            ec.wrap_indicator.enabled,
            ec.zen_mode.enabled,
            ec.zigzag_pattern.enabled,
        ];
        for (i, &e) in enabled_flags.iter().enumerate() {
            assert_eq!(e, false, "enabled flag at index {} was true", i);
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    // Duration-typed fields
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn duration_fields_are_reasonable() {
        let idle = IdleDimConfig::default();
        assert!(idle.delay.as_secs() > 0);
        assert!(idle.fade_duration.as_millis() > 0);

        let theme = ThemeTransitionConfig::default();
        assert!(theme.duration.as_millis() > 0);
    }

    // ═══════════════════════════════════════════════════════════════════
    // Configs without `enabled` field
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn configs_without_enabled_field() {
        // BgPatternConfig, ModeLineSeparatorConfig, ScrollBarConfig
        // have no `enabled` field — verify they still construct correctly
        let bp = BgPatternConfig::default();
        assert_eq!(bp.style, 0);

        let mls = ModeLineSeparatorConfig::default();
        assert_eq!(mls.style, 0);

        let sb = ScrollBarConfig::default();
        assert_eq!(sb.width, 0);
    }

    // ═══════════════════════════════════════════════════════════════════
    // Color tuple fields sanity (all components in [0.0, 1.0])
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn color_components_in_valid_range() {
        // Spot-check a variety of color fields
        let colors: Vec<(f32, f32, f32)> = vec![
            ArgylePatternConfig::default().color,
            AuroraConfig::default().color1,
            AuroraConfig::default().color2,
            BasketWeaveConfig::default().color,
            CircuitTraceConfig::default().color,
            CursorGlowConfig::default().color,
            NeonBorderConfig::default().color,
            MatrixRainConfig::default().color,
            ZigzagPatternConfig::default().color,
        ];
        for (i, (r, g, b)) in colors.iter().enumerate() {
            assert!(
                (0.0..=1.0).contains(r) && (0.0..=1.0).contains(g) && (0.0..=1.0).contains(b),
                "color at index {} has component outside [0.0, 1.0]: ({}, {}, {})",
                i,
                r,
                g,
                b
            );
        }
    }
}

/// Container for all visual effect configurations.
///
/// Shared between `RenderApp` (stores for persistence/replay)
/// and `WgpuRenderer` (uses for actual rendering).
#[derive(Clone, Debug, Default)]
pub struct EffectsConfig {
    pub accent_strip: AccentStripConfig,
    pub argyle_pattern: ArgylePatternConfig,
    pub aurora: AuroraConfig,
    pub basket_weave: BasketWeaveConfig,
    pub bg_gradient: BgGradientConfig,
    pub bg_pattern: BgPatternConfig,
    pub blur: BlurConfig,
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
