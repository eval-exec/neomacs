//! Hover identity and deadlines, independent of native windows and rendering.
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PanelId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HelpTarget {
    pub panel: PanelId,
    pub item: neomacs_display_protocol::menu::MenuItemId,
}

#[derive(Default)]
enum HoverState {
    #[default]
    Idle,
    Waiting {
        target: HelpTarget,
        deadline: Instant,
    },
    /// Already offered; expiry must not rearm on identical pointer motion.
    Offered(HelpTarget),
}

#[derive(Default)]
pub(super) struct HoverHelp {
    state: HoverState,
    last_hidden: Option<Instant>,
}

impl HoverHelp {
    pub fn target(&self) -> Option<HelpTarget> {
        match self.state {
            HoverState::Idle => None,
            HoverState::Waiting { target, .. } | HoverState::Offered(target) => Some(target),
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        match self.state {
            HoverState::Waiting { deadline, .. } => Some(deadline),
            HoverState::Idle | HoverState::Offered(_) => None,
        }
    }

    pub fn cancel(&mut self, was_visible: bool, now: Instant) {
        self.state = HoverState::Idle;
        if was_visible {
            self.last_hidden = Some(now);
        }
    }

    pub fn select(
        &mut self,
        target: HelpTarget,
        now: Instant,
        delay: Duration,
        short: Duration,
        recent: Duration,
    ) {
        if self.target() == Some(target) {
            return;
        }
        let delay = if self
            .last_hidden
            .is_some_and(|hidden| now.saturating_duration_since(hidden) <= recent)
        {
            short
        } else {
            delay
        };
        self.state = HoverState::Waiting {
            target,
            deadline: now.checked_add(delay).unwrap_or(now),
        };
    }

    pub fn take_due(&mut self, now: Instant) -> Option<HelpTarget> {
        match self.state {
            HoverState::Waiting { target, deadline } if deadline <= now => {
                self.state = HoverState::Offered(target);
                Some(target)
            }
            HoverState::Idle | HoverState::Waiting { .. } | HoverState::Offered(_) => None,
        }
    }
}

#[cfg(test)]
#[path = "help_test.rs"]
mod tests;
