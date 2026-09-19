//! Immutable logical-pixel text geometry, shared by menu sizing and painting.
//! Raster-cache population must never change these positions during a redraw.

use super::PopupMenuItem;

#[derive(Clone, Debug)]
pub struct MenuTextRun {
    characters: Vec<(char, f32)>,
    width: f32,
}

impl MenuTextRun {
    pub fn characters(&self) -> &[(char, f32)] {
        &self.characters
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    fn measure(text: &str, advance: &mut impl FnMut(char) -> f32) -> Self {
        let mut width = 0.0;
        let characters = text
            .chars()
            .map(|ch| {
                let x = width;
                width += advance(ch);
                (ch, x)
            })
            .collect();
        Self { characters, width }
    }
}

#[derive(Clone, Debug)]
pub struct MenuItemText {
    pub label: MenuTextRun,
    pub shortcut: MenuTextRun,
}

#[derive(Clone, Debug)]
pub struct MenuTextLayout {
    items: Vec<MenuItemText>,
    title: Option<MenuTextRun>,
    space_advance: f32,
}

impl MenuTextLayout {
    /// Measure each scalar once through the same font source used for paint.
    /// The fallback is an owner-supplied logical space, never an atlas cache's
    /// first rasterized glyph. Zero advances (e.g. combining marks) are valid.
    pub fn measure(
        items: &[PopupMenuItem],
        title: Option<&str>,
        space_advance: f32,
        mut advance: impl FnMut(char) -> f32,
    ) -> Self {
        let mut advance = |ch| {
            let measured = advance(ch);
            if measured.is_finite() && measured >= 0.0 {
                measured
            } else {
                space_advance
            }
        };
        Self {
            items: items
                .iter()
                .map(|item| MenuItemText {
                    label: MenuTextRun::measure(&item.label, &mut advance),
                    shortcut: MenuTextRun::measure(&item.shortcut, &mut advance),
                })
                .collect(),
            title: title.map(|text| MenuTextRun::measure(text, &mut advance)),
            space_advance,
        }
    }

    pub fn item(&self, index: usize) -> &MenuItemText {
        &self.items[index]
    }
    pub fn title(&self) -> Option<&MenuTextRun> {
        self.title.as_ref()
    }
    pub fn space_advance(&self) -> f32 {
        self.space_advance
    }
}
