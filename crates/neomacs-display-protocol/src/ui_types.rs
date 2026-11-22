//! Shared UI menu/toolbar/popup item types.

use strum::IntoStaticStr;

pub use crate::menu::PopupMenuItem;

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
    /// An ordinary GNU image source: the parsed `:image` file, or the GNU theme
    /// asset when the GNU icon theme is selected.  GNU `compute_image_size`
    /// semantics apply — the slot is a clamp, and the realized raster is never
    /// enlarged past its scaled native extent.
    GnuRaster { path: String },
    /// A Neomacs chrome-owned vector (themed or custom icon directory).  A
    /// vector has no meaningful native resolution, so it is realized at the
    /// slot's exact physical size instead of being stretched from its authored
    /// size.
    ThemedVector { path: String },
}

impl ToolBarImageSource {
    pub fn cache_key(&self) -> String {
        match self {
            Self::GnuRaster { path } => format!("gnu-raster:{path}"),
            Self::ThemedVector { path } => format!("themed-vector:{path}"),
        }
    }

    pub fn file_path(&self) -> Option<&str> {
        match self {
            Self::GnuRaster { path } | Self::ThemedVector { path } => Some(path),
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
