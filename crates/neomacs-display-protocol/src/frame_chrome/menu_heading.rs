//! A heading owns both its menu action and the text geometry that places it.
use super::{BandRect, ChromeAction, PositionedChromeItem};
use crate::{
    MenuBarItem,
    font::{ResolvedFont, ResolvedGlyph},
};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedMenuLabel {
    glyphs: Vec<ResolvedGlyph>,
    fonts: Vec<ResolvedFont>,
    font_size: f32,
    width: f32,
}

impl ResolvedMenuLabel {
    pub fn new(glyphs: Vec<ResolvedGlyph>, fonts: Vec<ResolvedFont>, font_size: f32) -> Self {
        let width = glyphs.iter().map(|g| g.x + g.x_advance).fold(0.0, f32::max);
        Self {
            glyphs,
            fonts,
            font_size,
            width,
        }
    }
    pub fn glyphs(&self) -> &[ResolvedGlyph] {
        &self.glyphs
    }
    pub fn fonts(&self) -> &[ResolvedFont] {
        &self.fonts
    }
    pub fn font_size(&self) -> f32 {
        self.font_size
    }
    pub fn width(&self) -> f32 {
        self.width
    }
}

/// Pixel text is resolved by the layout font service. Terminal field widths
/// follow the terminal menu protocol and never enter the GPU text painter.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MenuHeadingText {
    Pixels(ResolvedMenuLabel),
    Cells { width: f32 },
}

impl MenuHeadingText {
    pub fn width(&self) -> f32 {
        match self {
            Self::Pixels(run) => run.width(),
            Self::Cells { width } => *width,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PositionedMenuHeading {
    positioned: PositionedChromeItem<MenuBarItem>,
    text: MenuHeadingText,
    padding: f32,
}

impl PositionedMenuHeading {
    /// Contents are consumed and measured together; callers cannot replace a
    /// label while retaining its old glyphs, hit rectangle, or popup anchor.
    pub fn measure(
        item: MenuBarItem,
        x: f32,
        height: f32,
        padding: f32,
        measure: impl FnOnce(&str) -> MenuHeadingText,
    ) -> Self {
        let text = measure(&item.label);
        let bounds = BandRect::new(x, 0.0, text.width() + 2.0 * padding, height)
            .expect("valid menu heading geometry");
        let action = ChromeAction::OpenMenu {
            index: item.index,
            key: item.key.clone(),
        };
        Self {
            positioned: PositionedChromeItem::new(bounds, item, action),
            text,
            padding,
        }
    }
    pub fn fit(mut self, available_width: f32) -> Option<Self> {
        let bounds = self.positioned.local_bounds.raw();
        let width = bounds.width.min(available_width);
        if width <= 0.0 {
            return None;
        }
        self.positioned.local_bounds =
            BandRect::new(bounds.x, bounds.y, width, bounds.height).ok()?;
        Some(self)
    }
    pub fn item(&self) -> &MenuBarItem {
        self.positioned.item()
    }
    pub fn local_bounds(&self) -> BandRect {
        self.positioned.local_bounds()
    }
    pub fn action(&self) -> Option<&ChromeAction> {
        self.positioned.action()
    }
    pub fn text(&self) -> &MenuHeadingText {
        &self.text
    }
    pub fn padding(&self) -> f32 {
        self.padding
    }
}
