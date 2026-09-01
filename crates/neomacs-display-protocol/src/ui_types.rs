//! Shared UI menu/toolbar/popup item types.

use strum::IntoStaticStr;

/// A single item in a popup menu.
#[derive(Debug, Clone)]
pub struct PopupMenuItem {
    /// Display label for the item
    pub label: String,
    /// Keyboard shortcut text (e.g., "C-x C-s"), or empty
    pub shortcut: String,
    /// Whether the item is enabled (selectable)
    pub enabled: bool,
    /// Whether this is a separator line
    pub separator: bool,
    /// Whether this is a submenu header (has children)
    pub submenu: bool,
    /// Nesting depth (0 = top-level, 1 = first submenu, etc.)
    pub depth: u32,
}

/// A top-level menu bar item (e.g., "File", "Edit", "Tools").
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MenuBarItem {
    pub index: u32,
    pub label: String,
    pub key: String,
}

/// Image backing for a toolbar item.
///
/// GNU Emacs keeps the parsed `:image` property as an image specification.
/// The display protocol mirrors that shape by transporting the resolved image
/// source, instead of replacing it with a frontend-private icon name.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ToolBarImageSource {
    File { path: String },
}

impl ToolBarImageSource {
    pub fn cache_key(&self) -> String {
        match self {
            Self::File { path } => format!("file:{path}"),
        }
    }

    pub fn file_path(&self) -> Option<&str> {
        match self {
            Self::File { path } => Some(path),
        }
    }
}

/// GNU toolbar item type.  The C redisplay path stores this in
/// `TOOL_BAR_ITEM_TYPE`.  Wrapping is a separate GNU slot
/// (`TOOL_BAR_ITEM_WRAP`) and is represented by [`ToolBarItem::wrap`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr, serde::Serialize, serde::Deserialize,
)]
pub enum ToolBarItemType {
    #[strum(to_string = "button")]
    Button,
    #[strum(to_string = "separator")]
    Separator,
    #[strum(to_string = ":radio")]
    Radio,
    #[strum(to_string = ":toggle")]
    Toggle,
}

impl ToolBarItemType {
    pub fn gnu_type_name(self) -> &'static str {
        self.into()
    }
}

/// A single toolbar item.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ToolBarItem {
    pub index: u32,
    pub key: String,
    pub image: Option<ToolBarImageSource>,
    pub label: String,
    pub help: String,
    pub enabled: bool,
    pub selected: bool,
    pub item_type: ToolBarItemType,
    pub wrap: bool,
}

impl ToolBarItem {
    pub fn is_separator(&self) -> bool {
        self.item_type == ToolBarItemType::Separator
    }
}

/// A single tab bar item.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TabBarItem {
    pub index: u32,
    pub label: String,
    pub help: String,
    pub enabled: bool,
    pub selected: bool,
    pub is_separator: bool,
}
