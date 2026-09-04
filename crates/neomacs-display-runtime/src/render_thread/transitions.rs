//! Renderer-owned state for snapshot-based window transitions.

use crate::core::frame_glyphs::{
    BufferTransitionTarget, ContentTransitionHint, FrameGlyphBuffer, WindowEffectHint,
};
use neomacs_display_protocol::{
    DirectionlessTransitionEffect, DisplayWindowId, Rect, ResolvedTransitionEffect,
    TransitionEasing, TransitionPlan, TransitionPolicy,
};
use neomacs_renderer_wgpu::WgpuRenderer;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransitionSource {
    Buffer,
    Scroll,
    Theme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TransitionKey {
    Window(DisplayWindowId),
    Frame,
    Theme,
}

#[derive(Debug, Clone, PartialEq)]
struct PlannedTransition {
    key: TransitionKey,
    source: TransitionSource,
    plan: SynchronizedTransitionPlan,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TransitionRegionPlan {
    bounds: Rect,
    effect: ResolvedTransitionEffect,
}

/// A statically non-empty set of clips sharing one animation clock.
///
/// Duration and easing live on the group, so split-window regions cannot
/// silently drift apart in release builds.
#[derive(Debug, Clone, PartialEq)]
struct SynchronizedTransitionPlan {
    duration: std::time::Duration,
    easing: TransitionEasing,
    first_region: TransitionRegionPlan,
    additional_regions: Vec<TransitionRegionPlan>,
}

impl SynchronizedTransitionPlan {
    fn try_from_plans(plans: impl IntoIterator<Item = TransitionPlan>) -> Option<Self> {
        let mut plans = plans.into_iter();
        let first = plans.next()?;
        let additional_regions = plans
            .map(|plan| {
                (plan.duration == first.duration && plan.easing == first.easing).then_some(
                    TransitionRegionPlan {
                        bounds: plan.bounds,
                        effect: plan.effect,
                    },
                )
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            duration: first.duration,
            easing: first.easing,
            first_region: TransitionRegionPlan {
                bounds: first.bounds,
                effect: first.effect,
            },
            additional_regions,
        })
    }

    fn from_single(plan: TransitionPlan) -> Self {
        Self {
            duration: plan.duration,
            easing: plan.easing,
            first_region: TransitionRegionPlan {
                bounds: plan.bounds,
                effect: plan.effect,
            },
            additional_regions: Vec::new(),
        }
    }

    fn regions(&self) -> impl Iterator<Item = TransitionRegionPlan> + '_ {
        std::iter::once(self.first_region).chain(self.additional_regions.iter().copied())
    }

    fn region_count(&self) -> usize {
        1 + self.additional_regions.len()
    }
}

/// Renderer-owned state for any snapshot transition.
pub(super) struct ActiveTransition {
    source: TransitionSource,
    /// When this transition began, dated to the presentation time of the
    /// frame that started it. Progress is measured against the presentation
    /// time of each later frame, so the phase is correct when the pixels
    /// actually appear rather than when they were built.
    pub(super) started: neomacs_display_protocol::frame_time::EventTime,
    /// Regions share this transition's one clock and previous-frame snapshot.
    plan: SynchronizedTransitionPlan,
    // Snapshot handles retained for the transition's lifetime; sampling goes
    // through `old_bind_group`, so these are never read directly.
    #[allow(dead_code)]
    pub(super) old_texture: wgpu::Texture,
    #[allow(dead_code)]
    pub(super) old_view: wgpu::TextureView,
    pub(super) old_bind_group: wgpu::BindGroup,
}

/// Window transition state.
///
/// Groups configuration, double-buffer textures, and active transition maps.
pub(crate) struct TransitionState {
    // Configuration
    pub(super) policy: TransitionPolicy,

    // Double-buffer offscreen textures
    pub(super) offscreen_a: Option<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
    pub(super) offscreen_b: Option<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
    pub(super) current_is_a: bool,

    // Active transitions
    active: HashMap<TransitionKey, ActiveTransition>,
}

impl Default for TransitionState {
    fn default() -> Self {
        Self {
            policy: TransitionPolicy::default(),
            offscreen_a: None,
            offscreen_b: None,
            current_is_a: true,
            active: HashMap::new(),
        }
    }
}

impl TransitionState {
    pub(super) fn apply_policy(&mut self, policy: TransitionPolicy) {
        self.active.retain(|_, transition| match transition.source {
            TransitionSource::Buffer => policy.buffer.enabled,
            TransitionSource::Scroll => policy.scroll.enabled,
            TransitionSource::Theme => true,
        });
        self.policy = policy;
    }

    /// Check if any transitions are currently active
    pub(super) fn has_active(&self) -> bool {
        !self.active.is_empty()
    }

    pub(super) fn active_count(&self) -> usize {
        self.active.len()
    }
}

fn current_offscreen_view_and_bg(
    transitions: &TransitionState,
) -> Option<(&wgpu::TextureView, &wgpu::BindGroup)> {
    let (_, view, bg) = if transitions.current_is_a {
        transitions.offscreen_a.as_ref()?
    } else {
        transitions.offscreen_b.as_ref()?
    };
    Some((view, bg))
}

fn previous_offscreen(
    transitions: &TransitionState,
) -> Option<(&wgpu::Texture, &wgpu::TextureView, &wgpu::BindGroup)> {
    let (tex, view, bg) = if transitions.current_is_a {
        transitions.offscreen_b.as_ref()?
    } else {
        transitions.offscreen_a.as_ref()?
    };
    Some((tex, view, bg))
}

fn snapshot_prev_texture(
    renderer: &WgpuRenderer,
    transitions: &TransitionState,
    width: u32,
    height: u32,
) -> Option<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)> {
    let (prev_tex, _, _) = previous_offscreen(transitions)?;

    let (snap, snap_view) = renderer.create_offscreen_texture(width, height);

    let mut encoder = renderer
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Snapshot Copy Encoder"),
        });
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: prev_tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: &snap,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue().submit(std::iter::once(encoder.finish()));

    let snap_bg = renderer.create_texture_bind_group(&snap_view);
    Some((snap, snap_view, snap_bg))
}

fn plan_transition_hint(
    policy: &TransitionPolicy,
    hint: &ContentTransitionHint,
) -> Option<PlannedTransition> {
    match hint {
        ContentTransitionHint::BufferReplaced { target, intent } => {
            let key = match target {
                BufferTransitionTarget::Window { window_id, .. } => {
                    TransitionKey::Window(*window_id)
                }
                BufferTransitionTarget::Frame { .. } => TransitionKey::Frame,
            };
            let plan = SynchronizedTransitionPlan::try_from_plans(
                target
                    .regions()
                    .iter()
                    .map(|region| policy.buffer_plan(region.bounds(), *intent))
                    .collect::<Option<Vec<_>>>()?,
            )?;
            Some(PlannedTransition {
                key,
                source: TransitionSource::Buffer,
                plan,
            })
        }
    }
}

/// Plan a scroll transition from a measured viewport displacement.
///
/// Only an exact measurement animates. An ambiguous or non-overlapping scroll
/// has no honest distance to slide by, and inventing one is what the character
/// count estimate used to do — the previous presentation's pixels would be
/// dragged a distance unrelated to how far the text actually moved.
fn plan_scroll(
    policy: &TransitionPolicy,
    scroll: &crate::render_thread::frame_compositor::continuity::ScrollObservation,
) -> Option<PlannedTransition> {
    let bounds = scroll.region.bounds();
    if bounds.height < policy.minimum_scroll_region_height() {
        return None;
    }
    let pixels = scroll.displacement.exact_pixels()?;
    Some(PlannedTransition {
        key: TransitionKey::Window(scroll.window),
        source: TransitionSource::Scroll,
        plan: SynchronizedTransitionPlan::from_single(policy.scroll_plan(
            bounds,
            scroll.displacement.direction().transition_direction(),
            pixels,
        )?),
    })
}

fn apply_transition_hint(
    renderer: &WgpuRenderer,
    transitions: &mut TransitionState,
    hint: &ContentTransitionHint,
    now: neomacs_display_protocol::frame_time::EventTime,
    width: u32,
    height: u32,
) {
    let Some(planned) = plan_transition_hint(&transitions.policy, hint) else {
        return;
    };
    transitions.active.remove(&planned.key);
    start_transition(
        renderer,
        transitions,
        planned.key,
        planned.source,
        planned.plan,
        now,
        width,
        height,
    );
}

#[allow(clippy::too_many_arguments)]
fn start_transition(
    renderer: &WgpuRenderer,
    transitions: &mut TransitionState,
    transition_key: TransitionKey,
    source: TransitionSource,
    plan: SynchronizedTransitionPlan,
    now: neomacs_display_protocol::frame_time::EventTime,
    width: u32,
    height: u32,
) {
    let Some((tex, view, bg)) = snapshot_prev_texture(renderer, transitions, width, height) else {
        return;
    };
    tracing::debug!(
        ?source,
        ?transition_key,
        effect = ?plan.first_region.effect,
        easing = ?plan.easing,
        region_count = plan.region_count(),
        "starting window transition"
    );
    transitions.active.insert(
        transition_key,
        ActiveTransition {
            source,
            started: now,
            plan,
            old_texture: tex,
            old_view: view,
            old_bind_group: bg,
        },
    );
}

fn apply_effect_hint(
    renderer: &mut WgpuRenderer,
    transitions: &mut TransitionState,
    effects: &neomacs_display_protocol::EffectsConfig,
    hint: &WindowEffectHint,
    now: neomacs_display_protocol::frame_time::EventTime,
    frame_dirty: &mut bool,
    width: u32,
    height: u32,
) {
    match hint {
        WindowEffectHint::TextFadeIn { window_id, bounds } => {
            if effects.text_fade_in.enabled {
                renderer.trigger_text_fade_in(window_id.get(), *bounds, now.into_instant());
            }
        }
        WindowEffectHint::ScrollLineSpacing {
            window_id,
            bounds,
            direction,
        } => {
            if effects.scroll_line_spacing.enabled {
                renderer.trigger_scroll_line_spacing(
                    window_id.get(),
                    *bounds,
                    *direction,
                    now.into_instant(),
                );
            }
        }
        WindowEffectHint::ScrollMomentum {
            window_id,
            bounds,
            direction,
        } => {
            if effects.scroll_momentum.enabled {
                renderer.trigger_scroll_momentum(
                    window_id.get(),
                    *bounds,
                    *direction,
                    now.into_instant(),
                );
            }
        }
        WindowEffectHint::ScrollVelocityFade {
            window_id,
            bounds,
            delta,
        } => {
            if effects.scroll_velocity_fade.enabled {
                renderer.trigger_scroll_velocity_fade(
                    window_id.get(),
                    *bounds,
                    *delta,
                    now.into_instant(),
                );
            }
        }
        WindowEffectHint::LineAnimation {
            bounds,
            edit_y,
            offset,
            ..
        } => {
            if effects.line_animation.enabled {
                renderer.start_line_animation(
                    *bounds,
                    *edit_y,
                    *offset,
                    effects.line_animation.duration_ms,
                );
            }
        }
        WindowEffectHint::WindowSwitchFade { window_id, bounds } => {
            if effects.window_switch_fade.enabled {
                renderer.start_window_fade(window_id.get(), *bounds);
                *frame_dirty = true;
            }
        }
        WindowEffectHint::ThemeTransition { bounds } => {
            if !effects.theme_transition.enabled {
                return;
            }
            if transitions.active.contains_key(&TransitionKey::Theme) {
                return;
            }
            let plan = TransitionPlan {
                duration: effects.theme_transition.duration,
                easing: effects.theme_transition.easing,
                bounds: *bounds,
                effect: ResolvedTransitionEffect::Directionless(
                    DirectionlessTransitionEffect::Crossfade,
                ),
            };
            start_transition(
                renderer,
                transitions,
                TransitionKey::Theme,
                TransitionSource::Theme,
                SynchronizedTransitionPlan::from_single(plan),
                now,
                width,
                height,
            );
        }
    }
}

pub(super) fn detect_frame_transitions(
    renderer: &mut WgpuRenderer,
    transitions: &mut TransitionState,
    effects: &neomacs_display_protocol::EffectsConfig,
    frame: &mut FrameGlyphBuffer,
    scrolls: &[crate::render_thread::frame_compositor::continuity::ScrollObservation],
    frame_dirty: &mut bool,
    width: u32,
    height: u32,
) {
    let (transition_hints, effect_hints) = frame.take_runtime_hints();
    // One sample for the whole frame: a transition detected and rendered in the
    // same frame starts at progress 0 rather than at whatever the gap between
    // two clock reads happened to be.
    let now = renderer.frame_sample().presentation_time();

    for hint in &transition_hints {
        apply_transition_hint(renderer, transitions, hint, now, width, height);
    }
    // Scroll transitions come from a measurement the compositor made when the
    // presentation was installed, not from anything the producer declared.
    for scroll in scrolls {
        let Some(planned) = plan_scroll(&transitions.policy, scroll) else {
            continue;
        };
        transitions.active.remove(&planned.key);
        start_transition(
            renderer,
            transitions,
            planned.key,
            planned.source,
            planned.plan,
            now,
            width,
            height,
        );
    }
    for hint in &effect_hints {
        apply_effect_hint(
            renderer,
            transitions,
            effects,
            hint,
            now,
            frame_dirty,
            width,
            height,
        );
    }
}

pub(super) fn ensure_frame_offscreen_textures(
    renderer: &WgpuRenderer,
    transitions: &mut TransitionState,
    width: u32,
    height: u32,
) {
    if transitions.offscreen_a.is_some() && transitions.offscreen_b.is_some() {
        return;
    }
    if transitions.offscreen_a.is_none() {
        let (tex, view) = renderer.create_offscreen_texture(width, height);
        let bg = renderer.create_texture_bind_group(&view);
        transitions.offscreen_a = Some((tex, view, bg));
    }
    if transitions.offscreen_b.is_none() {
        let (tex, view) = renderer.create_offscreen_texture(width, height);
        let bg = renderer.create_texture_bind_group(&view);
        transitions.offscreen_b = Some((tex, view, bg));
    }
}

pub(super) fn clear_frame_transition_textures(transitions: &mut TransitionState) {
    transitions.offscreen_a = None;
    transitions.offscreen_b = None;
    transitions.active.clear();
}

pub(super) fn render_frame_transitions(
    renderer: &mut WgpuRenderer,
    transitions: &mut TransitionState,
    surface_view: &wgpu::TextureView,
    width: u32,
    height: u32,
) {
    let now = renderer.frame_sample().presentation_time();
    let current_bg = match current_offscreen_view_and_bg(transitions) {
        Some((_, bg)) => bg.clone(),
        None => return,
    };

    let mut completed = Vec::new();
    for (&transition_key, transition) in &transitions.active {
        let elapsed = now.saturating_since(transition.started);
        let raw_t = (elapsed.as_secs_f32() / transition.plan.duration.as_secs_f32()).min(1.0);
        let elapsed_secs = elapsed.as_secs_f32();

        for region in transition.plan.regions() {
            renderer.render_transition_effect(
                surface_view,
                &transition.old_bind_group,
                &current_bg,
                raw_t,
                elapsed_secs,
                &region.bounds,
                region.effect,
                transition.plan.easing,
                width,
                height,
            );
        }

        if raw_t >= 1.0 {
            completed.push(transition_key);
        }
    }
    for transition_key in completed {
        transitions.active.remove(&transition_key);
    }
}

// ==========================================================================
// Tests
// ==========================================================================

#[cfg(test)]
#[path = "transitions_test.rs"]
mod tests;
