//! Closed item kinds; presentation cannot make a separator actionable.
use super::MenuIndicator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAvailability {
    Enabled,
    Disabled,
}

impl From<bool> for MenuAvailability {
    fn from(enabled: bool) -> Self {
        if enabled {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItemKind {
    Command {
        availability: MenuAvailability,
        indicator: MenuIndicator,
    },
    Submenu {
        availability: MenuAvailability,
    },
    Label,
    Separator,
}

impl super::PopupMenuItem {
    pub fn enabled(&self) -> bool {
        match self.kind {
            MenuItemKind::Command { availability, .. } | MenuItemKind::Submenu { availability } => {
                availability == MenuAvailability::Enabled
            }
            MenuItemKind::Label | MenuItemKind::Separator => false,
        }
    }

    pub fn separator(&self) -> bool {
        matches!(self.kind, MenuItemKind::Separator)
    }
    pub fn submenu(&self) -> bool {
        matches!(self.kind, MenuItemKind::Submenu { .. })
    }

    pub fn indicator(&self) -> MenuIndicator {
        match self.kind {
            MenuItemKind::Command { indicator, .. } => indicator,
            MenuItemKind::Submenu { .. } | MenuItemKind::Label | MenuItemKind::Separator => {
                MenuIndicator::None
            }
        }
    }

    /// Native GUI activation is narrower than navigation: submenu headers
    /// can receive focus but only command leaves return an action.
    pub fn actionable(&self) -> bool {
        matches!(
            self.kind,
            MenuItemKind::Command {
                availability: MenuAvailability::Enabled,
                ..
            }
        )
    }
}
