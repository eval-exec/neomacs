//! Renderer-owned state for snapshot-based window transitions.

use crate::core::frame_glyphs::{BufferTransitionTarget, ContentTransitionHint, FrameGlyphBuffer};
use neomacs_display_protocol::{
    DirectionlessTransitionEffect, DisplayWindowId, Rect, ResolvedTransitionEffect,
    TransitionEasing, TransitionPlan, TransitionPolicy,
};
use neomacs_renderer_wgpu::{
    BudgetExceeded, CompositionRing, SnapshotLease, SnapshotSize, WgpuRenderer,
};
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
    /// The composition this transition animates away from.
    ///
    /// Holding the lease is what keeps those pixels alive: the pool will not
    /// re-lease a slot anyone still names, so the ring allocates beside it
    /// instead of overwriting it. This used to be a full-frame
    /// `copy_texture_to_texture` into a third texture on every transition
    /// start, which existed only because the flip-flop would otherwise draw
    /// over the picture being faded from.
    old: SnapshotLease,
}

/// Window transition state.
///
/// Groups configuration, the composition ring, and active transition maps.
pub(crate) struct TransitionState {
    // Configuration
    pub(super) policy: TransitionPolicy,

    /// The pictures of the last two composed frames, leased from the pool.
    /// `None` until the first offscreen frame, and again after a device loss.
    pub(super) compositions: Option<CompositionRing>,

    // Active transitions
    active: HashMap<TransitionKey, ActiveTransition>,
}

impl Default for TransitionState {
    fn default() -> Self {
        Self {
            policy: TransitionPolicy::default(),
            compositions: None,
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

    /// Rotate this frame window's composition ring and hand back the slot the
    /// frame should compose into.
    ///
    /// A ring cut for a surface size that no longer exists is rebuilt rather
    /// than advanced, and is dropped *before* the new lease is asked for so
    /// the pool can re-cut those slots at the new size instead of allocating
    /// beside them. That also retires the unwritten contract the old
    /// `ensure_frame_offscreen_textures` depended on: it never checked the
    /// size of an existing texture, so correctness rested entirely on an
    /// external resize hook clearing it.
    pub(super) fn advance_compositions(
        &mut self,
        renderer: &mut WgpuRenderer,
        size: SnapshotSize,
    ) -> Result<SnapshotLease, BudgetExceeded> {
        match self.compositions.take() {
            Some(mut ring) if ring.size() == size => {
                let advanced = ring.advance(|| renderer.acquire_snapshot(size));
                let current = ring.current().clone();
                self.compositions = Some(ring);
                advanced.map(|()| current)
            }
            _ => {
                let ring = CompositionRing::new(renderer.acquire_snapshot(size)?);
                let current = ring.current().clone();
                self.compositions = Some(ring);
                Ok(current)
            }
        }
    }
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

/// Run the effects that draw *over* a scrolled window.
///
/// Unlike the slide, these sample no retained pixels, so they apply to any
/// window whose viewport moved — including one whose buffer-owned region
/// changed, where a slide has nothing safe to blit.
///
/// `same_buffer` gates them: replacing what a window shows is a content change,
/// not a scroll, and dragging a momentum glow across it would describe motion
/// that did not happen.
fn apply_scroll_effects(
    renderer: &mut WgpuRenderer,
    effects: &neomacs_display_protocol::EffectsConfig,
    scroll: &crate::render_thread::frame_compositor::continuity::ScrollObservation,
    now: neomacs_display_protocol::frame_time::EventTime,
) {
    if !scroll.same_buffer {
        return;
    }
    let window = scroll.window.get();
    let direction = scroll.displacement.direction().sign();
    if effects.scroll_line_spacing.enabled {
        renderer.trigger_scroll_line_spacing(window, scroll.bounds, direction, now.into_instant());
    }
    if effects.scroll_momentum.enabled {
        renderer.trigger_scroll_momentum(window, scroll.bounds, direction, now.into_instant());
    }
    if effects.scroll_velocity_fade.enabled {
        renderer.trigger_scroll_velocity_fade(
            window,
            scroll.bounds,
            velocity_fade_intensity(scroll),
            now.into_instant(),
        );
    }
}

/// How strongly to fade a window for how fast it scrolled, in `0.0..=1.0`.
///
/// The producer used to pass a raw count of characters scrolled, which the
/// renderer divided by a magic 50.0 to get an opacity. A character count is not
/// a distance: the same scroll produced a different fade in a narrow window
/// than a wide one, and a line of long lines faded differently from a line of
/// short ones.
///
/// A measured displacement is a distance, so it can be normalized honestly —
/// against roughly one viewport, so a full-page scroll saturates.
///
/// When the distance is not measurable the intensity saturates. That is
/// deliberate policy rather than a fabricated measurement: something moved the
/// viewport far enough that no row survived, or the layout changed underneath,
/// and both are the fast, disruptive motion this effect exists to soften. It
/// also keeps the effect firing where it fires today.
fn velocity_fade_intensity(
    scroll: &crate::render_thread::frame_compositor::continuity::ScrollObservation,
) -> f32 {
    let saturation = (scroll.bounds.height * VELOCITY_FADE_SATURATION_FRACTION).max(1.0);
    scroll
        .displacement
        .exact_pixels()
        .map_or(1.0, |pixels| (pixels / saturation).clamp(0.0, 1.0))
}

/// The share of a window's height a scroll must cover to fade at full strength.
const VELOCITY_FADE_SATURATION_FRACTION: f32 = 1.0;

/// Crossfade the frame because its theme changed.
///
/// Every condition the hint arm applied is kept: the config gate, the
/// re-entry guard that lets a running theme transition finish rather than
/// restarting it, and the configured duration and easing.
fn start_theme_transition(
    transitions: &mut TransitionState,
    effects: &neomacs_display_protocol::EffectsConfig,
    theme: crate::render_thread::frame_compositor::continuity::theme::ThemeChange,
    now: neomacs_display_protocol::frame_time::EventTime,
) {
    if !effects.theme_transition.enabled {
        return;
    }
    if transitions.active.contains_key(&TransitionKey::Theme) {
        return;
    }
    let plan = TransitionPlan {
        duration: effects.theme_transition.duration,
        easing: effects.theme_transition.easing,
        bounds: theme.bounds,
        effect: ResolvedTransitionEffect::Directionless(DirectionlessTransitionEffect::Crossfade),
    };
    start_transition(
        transitions,
        TransitionKey::Theme,
        TransitionSource::Theme,
        SynchronizedTransitionPlan::from_single(plan),
        now,
    );
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
    let bounds = scroll.transition?.region.bounds();
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
    transitions: &mut TransitionState,
    hint: &ContentTransitionHint,
    now: neomacs_display_protocol::frame_time::EventTime,
) {
    let Some(planned) = plan_transition_hint(&transitions.policy, hint) else {
        return;
    };
    transitions.active.remove(&planned.key);
    start_transition(transitions, planned.key, planned.source, planned.plan, now);
}

fn start_transition(
    transitions: &mut TransitionState,
    transition_key: TransitionKey,
    source: TransitionSource,
    plan: SynchronizedTransitionPlan,
    now: neomacs_display_protocol::frame_time::EventTime,
) {
    // No previous composition means there is nothing to animate away from —
    // the first offscreen frame of a window, or the first after a device
    // loss. Starting anyway would crossfade from an empty texture.
    let Some(old) = transitions
        .compositions
        .as_ref()
        .and_then(CompositionRing::previous)
        .cloned()
    else {
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
            old,
        },
    );
}

pub(super) fn detect_frame_transitions(
    renderer: &mut WgpuRenderer,
    transitions: &mut TransitionState,
    effects: &neomacs_display_protocol::EffectsConfig,
    frame: &mut FrameGlyphBuffer,
    pending: crate::render_thread::frame_compositor::PendingContinuity,
    frame_dirty: &mut bool,
) {
    let transition_hints = frame.take_transition_hints();
    // One sample for the whole frame: a transition detected and rendered in the
    // same frame starts at progress 0 rather than at whatever the gap between
    // two clock reads happened to be.
    let now = renderer.frame_sample().presentation_time();

    for hint in &transition_hints {
        apply_transition_hint(transitions, hint, now);
    }
    // Scroll transitions come from a measurement the compositor made when the
    // presentation was installed, not from anything the producer declared.
    if pending.accept_derived_effects
        && let Some(theme) = pending.theme
    {
        start_theme_transition(transitions, effects, theme, now);
    }
    if pending.accept_derived_effects
        && let Some(selection) = pending.selection
        && effects.window_switch_fade.enabled
    {
        renderer.start_window_fade(selection.window.get(), selection.bounds);
        *frame_dirty = true;
    }
    if pending.accept_derived_effects && effects.text_fade_in.enabled {
        for replaced in &pending.shown_text_replaced {
            renderer.trigger_text_fade_in(
                replaced.window.get(),
                replaced.bounds,
                now.into_instant(),
            );
        }
    }
    for scroll in &pending.scrolls {
        if pending.accept_derived_effects {
            apply_scroll_effects(renderer, effects, scroll, now);
        }
        let Some(planned) = plan_scroll(&transitions.policy, scroll) else {
            continue;
        };
        transitions.active.remove(&planned.key);
        start_transition(transitions, planned.key, planned.source, planned.plan, now);
    }
    if pending.accept_derived_effects && effects.line_animation.enabled {
        for reflow in &pending.reflows {
            renderer.start_line_animation(
                reflow.bounds,
                reflow.first_moved_y,
                reflow.pixels,
                effects.line_animation.duration_ms,
                now,
            );
            *frame_dirty = true;
        }
    }
}

pub(super) fn clear_frame_transition_textures(transitions: &mut TransitionState) {
    transitions.compositions = None;
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
    let Some(compositions) = transitions.compositions.as_ref() else {
        return;
    };
    let current_bg = compositions.current().bind_group().clone();

    let mut completed = Vec::new();
    for (&transition_key, transition) in &transitions.active {
        let elapsed = now.saturating_since(transition.started);
        let raw_t = (elapsed.as_secs_f32() / transition.plan.duration.as_secs_f32()).min(1.0);
        let elapsed_secs = elapsed.as_secs_f32();

        for region in transition.plan.regions() {
            renderer.render_transition_effect(
                surface_view,
                transition.old.bind_group(),
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
