//! Heading intent survives native focus changes and asynchronous popup cleanup.
use neomacs_display_protocol::menu::{MenuBarRequestId, MenuToken};
use winit::window::WindowId;

#[derive(Clone, Debug)]
pub(crate) struct MenuHeading {
    pub frame: u64,
    pub parent: WindowId,
    pub key: String,
    pub index: u32,
    pub compact: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HeadingAction {
    Keep,
    Close,
    Request(MenuBarRequestId),
}

struct RequestedHeading {
    heading: MenuHeading,
    id: MenuBarRequestId,
    shown: Option<MenuToken>,
}

#[derive(Default)]
pub(super) struct MenuBarTracking {
    requested: Option<RequestedHeading>,
}

impl MenuBarTracking {
    pub fn select(&mut self, heading: MenuHeading, pressed: bool) -> HeadingAction {
        if self
            .requested
            .as_ref()
            .is_some_and(|r| r.heading.frame == heading.frame && r.heading.key == heading.key)
        {
            if !pressed {
                return HeadingAction::Keep;
            }
            self.clear();
            return HeadingAction::Close;
        }
        let id = MenuBarRequestId::fresh();
        self.requested = Some(RequestedHeading {
            heading,
            id,
            shown: None,
        });
        HeadingAction::Request(id)
    }

    pub fn accepts(&self, id: MenuBarRequestId) -> bool {
        self.requested.as_ref().is_some_and(|r| r.id == id)
    }

    pub fn shown(&mut self, id: MenuBarRequestId, token: MenuToken) {
        if let Some(r) = &mut self.requested
            && r.id == id
        {
            r.shown = Some(token);
        }
    }

    pub fn owns_popup(&self, token: MenuToken) -> bool {
        self.requested
            .as_ref()
            .is_some_and(|r| r.shown == Some(token))
    }

    pub fn heading(&self) -> Option<&MenuHeading> {
        self.requested.as_ref().map(|r| &r.heading)
    }

    pub fn clear(&mut self) {
        self.requested = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn heading(key: &str) -> MenuHeading {
        MenuHeading {
            frame: 1,
            parent: WindowId::from_raw(1),
            key: key.into(),
            index: 5,
            compact: false,
        }
    }

    #[test]
    fn repeated_hover_is_idempotent_while_pending_and_shown() {
        let mut bar = MenuBarTracking::default();
        let HeadingAction::Request(id) = bar.select(heading("help"), true) else {
            panic!()
        };
        for _ in 0..100 {
            assert_eq!(bar.select(heading("help"), false), HeadingAction::Keep);
        }
        bar.shown(id, MenuToken::fresh());
        for _ in 0..100 {
            assert_eq!(bar.select(heading("help"), false), HeadingAction::Keep);
        }
    }

    #[test]
    fn a_b_a_switch_rejects_old_a_response() {
        let mut bar = MenuBarTracking::default();
        let HeadingAction::Request(old) = bar.select(heading("help"), true) else {
            panic!()
        };
        bar.select(heading("interactively"), false);
        let HeadingAction::Request(new) = bar.select(heading("help"), false) else {
            panic!()
        };
        assert!(!bar.accepts(old));
        assert!(bar.accepts(new));
        assert_eq!(bar.select(heading("help"), true), HeadingAction::Close);
        assert!(!bar.accepts(new));
    }
}
