//! Render-backend facts and product quality policy.
//!
//! Adapter classification is a fact about the active wgpu device.  Quality is
//! a product decision derived from that fact and the exact configuration the
//! user requested.  Keeping those concepts separate prevents render, asset,
//! and scheduling call sites from inventing their own `DeviceType::Cpu`
//! behavior.

use std::num::NonZeroU16;

use neomacs_display_protocol::{
    EffectOperation, EffectValue, FrameGlyphBuffer, TransitionPolicy, VisualConfig,
};

/// Coarse cost class of the active adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AdapterClass {
    /// No wgpu adapter has been selected yet.
    Pending,
    Hardware,
    /// A software rasterizer reported by wgpu as `DeviceType::Cpu`.
    Software,
}

/// Immutable facts projected from the active renderer backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RenderBackendProfile {
    adapter_class: AdapterClass,
}

impl RenderBackendProfile {
    pub(super) const fn pending() -> Self {
        Self {
            adapter_class: AdapterClass::Pending,
        }
    }

    pub(super) const fn hardware() -> Self {
        Self {
            adapter_class: AdapterClass::Hardware,
        }
    }

    pub(super) const fn software() -> Self {
        Self {
            adapter_class: AdapterClass::Software,
        }
    }

    pub(super) fn from_device_type(device_type: wgpu::DeviceType) -> Self {
        if device_type == wgpu::DeviceType::Cpu {
            Self::software()
        } else {
            Self::hardware()
        }
    }

    pub(super) const fn adapter_class(self) -> AdapterClass {
        self.adapter_class
    }
}

impl Default for RenderBackendProfile {
    fn default() -> Self {
        Self::pending()
    }
}

/// Effective quality mode selected for the active backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum QualityMode {
    Full,
    /// Hard compatibility mode for a wgpu software adapter: preserve editor
    /// correctness and static presentation while suppressing the current
    /// GPU/offscreen effect families and their standing demand.
    SoftwareCompatibility,
}

/// Whether an explicitly requested full-frame shader can be installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FramePostDisposition {
    Enabled,
    SuppressedByQualityPolicy,
}

impl FramePostDisposition {
    pub(super) const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// Immutable decisions needed to execute one committed frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RenderFeaturePlan {
    /// Compose this frame into the ring rather than straight onto the surface.
    ///
    /// Not only for transitions. Any effect that shows the frame as it was
    /// *before* a change needs the previous composition to exist already when
    /// the change lands — and a picture that was never composed offscreen was
    /// never kept. Deciding this from the arrival of the change is too late by
    /// exactly one frame: that is the frame whose picture is wanted.
    pub(super) compose_offscreen: bool,
    pub(super) accept_transition_hints: bool,
    pub(super) accept_derived_effects: bool,
    pub(super) accept_cursor_effects: bool,
    pub(super) apply_frame_post: bool,
}

impl RenderFeaturePlan {
    /// Materialize the execution view of a frame without changing the retained
    /// committed frame. Runtime hints are one-shot inputs; a suppressed family
    /// is consumed but cannot leak into renderer implementation code.
    pub(super) fn prepare_frame(self, frame: &mut FrameGlyphBuffer) {
        if !self.accept_transition_hints {
            frame.transition_hints.clear();
        }
        if !self.accept_cursor_effects {
            frame.cursor_effects_by_window.clear();
        }
    }
}

/// One negotiated policy is the source of truth for configuration, frame
/// planning, scheduler eligibility, and asset capability.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct RenderQualityPolicy {
    profile: RenderBackendProfile,
    mode: QualityMode,
    effective_visual_config: VisualConfig,
}

impl RenderQualityPolicy {
    pub(super) fn negotiate(profile: RenderBackendProfile, requested: &VisualConfig) -> Self {
        let mode = if profile.adapter_class() == AdapterClass::Software {
            QualityMode::SoftwareCompatibility
        } else {
            QualityMode::Full
        };
        let effective_visual_config = match mode {
            QualityMode::Full => requested.clone(),
            QualityMode::SoftwareCompatibility => software_compat_visual_config(requested),
        };
        Self {
            profile,
            mode,
            effective_visual_config,
        }
    }

    pub(super) const fn profile(&self) -> RenderBackendProfile {
        self.profile
    }

    pub(super) const fn mode(&self) -> QualityMode {
        self.mode
    }

    pub(super) const fn allows_dynamic_effects(&self) -> bool {
        matches!(self.mode, QualityMode::Full)
    }

    /// Policy-owned scheduler rate for dynamic visual work. `None` is the
    /// software-compatibility bound: no standing animation demand. Keeping
    /// the rate decision here prevents a future retained effect from silently
    /// inheriting full display cadence at an unrelated lifecycle call site.
    pub(super) const fn dynamic_animation_rate(
        &self,
        display_rate: NonZeroU16,
    ) -> Option<NonZeroU16> {
        match self.mode {
            QualityMode::Full => Some(display_rate),
            QualityMode::SoftwareCompatibility => None,
        }
    }

    pub(super) const fn frame_post_disposition(&self) -> FramePostDisposition {
        match self.mode {
            QualityMode::Full => FramePostDisposition::Enabled,
            QualityMode::SoftwareCompatibility => FramePostDisposition::SuppressedByQualityPolicy,
        }
    }

    /// One predicate owns both execution and continuous scheduler demand for
    /// the installed post shader, so suppression cannot stop drawing while
    /// accidentally leaving its clock alive (or vice versa).
    pub(super) const fn frame_post_active(&self, installed: bool) -> bool {
        self.frame_post_disposition().is_enabled() && installed
    }

    /// A renderer-global post pipeline creates demand only for windows with a
    /// complete retained presentation. Otherwise an `AwaitingContent` result
    /// would be re-requested forever by the standing clock.
    pub(super) const fn frame_post_scheduler_active(
        &self,
        installed: bool,
        window_has_presentation: bool,
    ) -> bool {
        self.frame_post_active(installed) && window_has_presentation
    }

    pub(super) const fn effective_visual_config(&self) -> &VisualConfig {
        &self.effective_visual_config
    }

    /// How panes travel under this policy.
    pub(super) fn pane_motion(&self) -> neomacs_display_protocol::motion_spec::MotionSpec {
        // A reduced-quality plan declines every compositor-derived effect; a
        // pane morph is one, and one of the most expensive.
        if self.mode == QualityMode::Full {
            self.effective_visual_config.pane_motion.movement()
        } else {
            neomacs_display_protocol::motion_spec::MotionSpec::Instant
        }
    }

    pub(super) fn transition_policy(&self) -> TransitionPolicy {
        TransitionPolicy::from(&self.effective_visual_config)
    }

    pub(super) fn plan_frame(
        &self,
        frame_has_theme_transition: bool,
        frame_post_installed: bool,
    ) -> RenderFeaturePlan {
        let full = self.mode == QualityMode::Full;
        RenderFeaturePlan {
            compose_offscreen: full
                && (self.transition_policy().needs_offscreen()
                    || self.effective_visual_config.pane_motion.enabled
                    || frame_has_theme_transition),
            accept_transition_hints: full,
            accept_derived_effects: full,
            accept_cursor_effects: full,
            apply_frame_post: self.frame_post_active(frame_post_installed),
        }
    }
}

fn software_compat_visual_config(requested: &VisualConfig) -> VisualConfig {
    let mut effective = requested.clone();
    effective.cursor_motion.enabled = false;
    effective.cursor_size_transition.enabled = false;
    effective.buffer_transition.enabled = false;
    effective.scroll_transition.enabled = false;
    // A software adapter redraws the whole surface for every frame of a morph.
    // Disabling it there is the honest setting, and it also costs nothing: the
    // compositor builds no motion at all for a disabled spec.
    effective.pane_motion.enabled = false;

    let disable_effects = effective
        .effects
        .effect_names()
        .into_iter()
        .filter(|name| {
            effective
                .effects
                .effect_values(name)
                .is_ok_and(|properties| {
                    properties.iter().any(|(property, _)| property == "enabled")
                })
        })
        .map(|name| EffectOperation::set(name, [("enabled", EffectValue::Bool(false))]))
        .collect::<Vec<_>>();
    effective.effects = effective
        .effects
        .apply_effects(&disable_effects)
        .expect("effect registry must be able to disable every enabled effect");
    effective.effects.bg_pattern.style = 0;
    effective.effects.mode_line_separator.style = 0;
    effective.effects.scroll_bar.width = 0;
    effective
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_profile_projects_wgpu_device_cost_class_once() {
        assert_eq!(
            RenderBackendProfile::from_device_type(wgpu::DeviceType::Cpu).adapter_class(),
            AdapterClass::Software
        );
        for device_type in [
            wgpu::DeviceType::Other,
            wgpu::DeviceType::IntegratedGpu,
            wgpu::DeviceType::DiscreteGpu,
            wgpu::DeviceType::VirtualGpu,
        ] {
            assert_eq!(
                RenderBackendProfile::from_device_type(device_type).adapter_class(),
                AdapterClass::Hardware
            );
        }
    }

    #[test]
    fn software_compat_policy_preserves_request_and_owns_every_feature_decision() {
        let mut requested = VisualConfig::default();
        requested.cursor_motion.enabled = true;
        requested.cursor_size_transition.enabled = true;
        requested.buffer_transition.enabled = true;
        requested.scroll_transition.enabled = true;
        requested.effects.cursor_glow.enabled = true;
        requested.effects.bg_pattern.style = 1;

        let policy = RenderQualityPolicy::negotiate(RenderBackendProfile::software(), &requested);

        assert_eq!(policy.mode(), QualityMode::SoftwareCompatibility);
        assert_eq!(
            policy.dynamic_animation_rate(NonZeroU16::new(144).unwrap()),
            None,
            "software compatibility has a hard zero-cadence bound"
        );
        assert!(!policy.effective_visual_config().cursor_motion.enabled);
        assert!(!policy.effective_visual_config().effects.cursor_glow.enabled);
        assert_eq!(requested.effects.bg_pattern.style, 1);
        assert_eq!(
            policy.frame_post_disposition(),
            FramePostDisposition::SuppressedByQualityPolicy
        );
        assert!(!policy.frame_post_scheduler_active(true, true));
        assert_eq!(
            policy.plan_frame(true, true),
            RenderFeaturePlan {
                compose_offscreen: false,
                accept_transition_hints: false,
                accept_derived_effects: false,
                accept_cursor_effects: false,
                apply_frame_post: false,
            }
        );
    }

    #[test]
    fn full_policy_preserves_requested_features() {
        let requested = VisualConfig::default();
        let policy = RenderQualityPolicy::negotiate(RenderBackendProfile::hardware(), &requested);

        assert_eq!(policy.mode(), QualityMode::Full);
        assert_eq!(policy.effective_visual_config(), &requested);
        assert_eq!(
            policy.frame_post_disposition(),
            FramePostDisposition::Enabled
        );
        assert!(policy.frame_post_scheduler_active(true, true));
        assert!(
            !policy.frame_post_scheduler_active(true, false),
            "an awaiting-content window must not inherit global shader demand"
        );
        assert_eq!(
            policy.dynamic_animation_rate(NonZeroU16::new(144).unwrap()),
            NonZeroU16::new(144)
        );
    }
}
