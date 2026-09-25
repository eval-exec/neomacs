//! Per-side physical modifier tracking.
//!
//! winit's `ModifiersState` only aggregates: "an option key is down" says
//! nothing about which one.  GNU's `ev_modifiers_helper` distinguishes left
//! from right through the per-event masks (`src/nsterm.m:371-395`), and
//! falls back to the left value when the window server does not
//! differentiate (`:387-391`).  This module reproduces that: the physical
//! modifier keys' own key events (which carry a `KeyLocation`) drive the
//! tracked sides, and aggregate updates without side knowledge leave the
//! fallback in place.

use neomacs_display_protocol::ModifierSideState;

/// Tracked press state for one modifier family's two physical keys.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
struct FamilySides {
    /// `None`: this side's key event has not been observed since the last
    /// reset (window focus changes, missed events), so it contributes the
    /// GNU fallback rather than a claim.
    left: Option<bool>,
    right: Option<bool>,
}

impl FamilySides {
    /// Combine tracked sides with the window server's aggregate answer.
    fn sides(&self, aggregate_down: bool) -> Option<ModifierSideState> {
        if !aggregate_down {
            return None;
        }
        match (self.left, self.right) {
            (Some(true), Some(true)) => Some(ModifierSideState::Both),
            (Some(true), _) => Some(ModifierSideState::LeftOnly),
            (_, Some(true)) => Some(ModifierSideState::RightOnly),
            // The family is down but no side was ever observed directly
            // (grab, focus hand-off): GNU's "use the left value" fallback.
            _ => Some(ModifierSideState::Unknown),
        }
    }

    fn observe(&mut self, side: TrackedSide, down: bool) {
        let slot = match side {
            TrackedSide::Left => &mut self.left,
            TrackedSide::Right => &mut self.right,
        };
        *slot = Some(down);
    }

    fn reset(&mut self) {
        self.left = None;
        self.right = None;
    }
}

/// Which side of a family a tracked key event belongs to.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TrackedSide {
    Left,
    Right,
}

/// Which family a physical modifier key belongs to, for side tracking.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TrackedModifier {
    Command,
    Option,
    Control,
}

/// Per-family side tracking for the whole keyboard.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ModifierSides {
    command: FamilySides,
    option: FamilySides,
    control: FamilySides,
}

impl ModifierSides {
    /// Record a physical modifier key's press or release.
    pub(crate) fn observe_key(&mut self, modifier: TrackedModifier, side: TrackedSide, down: bool) {
        let family = match modifier {
            TrackedModifier::Command => &mut self.command,
            TrackedModifier::Option => &mut self.option,
            TrackedModifier::Control => &mut self.control,
        };
        family.observe(side, down);
    }

    /// Forget every tracked side: the keyboard focus left the window.
    pub(crate) fn reset(&mut self) {
        self.command.reset();
        self.option.reset();
        self.control.reset();
    }

    /// The cooked per-family side facts for [`RawModifiers`].
    pub(crate) fn to_raw(
        self,
        shift_down: bool,
        ctrl_down: bool,
        command_down: bool,
        option_down: bool,
    ) -> neomacs_display_protocol::RawModifiers {
        neomacs_display_protocol::RawModifiers {
            shift: shift_down,
            ctrl: self.control.sides(ctrl_down),
            command: self.command.sides(command_down),
            option: self.option.sides(option_down),
        }
    }
}

#[cfg(test)]
mod tests;
