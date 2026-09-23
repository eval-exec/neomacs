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
#[path = "menu_bar/tests/menu_bar_test.rs"]
mod tests;
