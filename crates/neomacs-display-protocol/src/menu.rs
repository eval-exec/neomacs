//! Plain menu presentation data shared by the runtime and painter.

mod item;
pub use item::{MenuAvailability, MenuItemKind};

/// Correlates a native heading intent with the evaluator's menu response.
/// Distinct from MenuToken: one intent can produce multiple menu revisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuBarRequestId(pub u64);

impl MenuBarRequestId {
    pub fn fresh() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

/// Identifies one immutable revision of an evaluator-owned menu session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuToken {
    pub session: u64,
    pub revision: u64,
}

impl MenuToken {
    pub fn fresh() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            session: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            revision: 0,
        }
    }

    pub fn next_revision(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("menu revision exhausted");
    }
}

/// An item identity within a menu snapshot, independent of panel-local rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuItemId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuResult {
    pub token: MenuToken,
    pub item: Option<MenuItemId>,
}

impl MenuResult {
    pub fn from_index(token: MenuToken, index: i32) -> Self {
        Self {
            token,
            item: u32::try_from(index).ok().map(MenuItemId),
        }
    }

    pub fn index(self) -> i32 {
        self.item.map_or(-1, |item| item.0 as i32)
    }
}

/// Lisp-owned button state, distinct from runtime hover or keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCheckState {
    Off,
    On,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MenuIndicator {
    #[default]
    None,
    Toggle(MenuCheckState),
    Radio(MenuCheckState),
}

/// A single item in a popup menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupMenuItem {
    pub kind: MenuItemKind,
    /// Help belongs to the item snapshot, including disabled items.
    pub help: Option<String>,
    /// Display label for the item
    pub label: String,
    /// Keyboard shortcut text (e.g., "C-x C-s"), or empty
    pub shortcut: String,
    /// Nesting depth (0 = top-level, 1 = first submenu, etc.)
    pub depth: u32,
}

#[derive(Clone, Debug)]
pub struct MenuPanel {
    /// Position (logical pixels)
    pub x: f32,
    pub y: f32,
    /// Indices into the menu's item list for items shown in this panel
    pub item_indices: Vec<usize>,
    /// Currently hovered index within item_indices (-1 = none)
    pub hover_index: i32,
    /// Computed layout: (x, y, width, height) in logical pixels
    pub bounds: (f32, f32, f32, f32),
    /// Per-item Y offsets (relative to bounds.y)
    pub item_offsets: Vec<f32>,
    /// Item height
    pub item_height: f32,
}

impl MenuPanel {
    pub fn shortcut_right(&self, items: &[PopupMenuItem], advance: f32) -> f32 {
        let arrow = if self
            .item_indices
            .iter()
            .any(|&index| items[index].submenu())
        {
            3.0 * advance
        } else {
            0.0
        };
        self.bounds.0 + self.bounds.2 - 8.0 - arrow
    }

    /// A panel-wide gutter: ordinary labels align with check/radio labels.
    pub fn indicator_width(&self, items: &[PopupMenuItem]) -> f32 {
        if self
            .item_indices
            .iter()
            .any(|&index| items[index].indicator() != MenuIndicator::None)
        {
            self.item_height
        } else {
            0.0
        }
    }
}

/// One measured panel. Coordinates are local to its drawing target.
pub struct MenuPanelPaint<'a> {
    pub panel: &'a MenuPanel,
    pub all_items: &'a [PopupMenuItem],
    pub title: Option<&'a str>,
    pub face_fg: Option<(f32, f32, f32)>,
    pub face_bg: Option<(f32, f32, f32)>,
    pub font_face: Option<&'a crate::face::Face>,
}
