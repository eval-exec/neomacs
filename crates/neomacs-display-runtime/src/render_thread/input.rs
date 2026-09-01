//! Input translation and window chrome hit-testing.

use crate::backend::wgpu::{NEOMACS_CTRL_MASK, NEOMACS_META_MASK, NEOMACS_SUPER_MASK};
use winit::keyboard::{Key, NamedKey};

use super::RenderApp;
use super::frame_windows::GuiFrameWindowState;
use super::state::WindowChrome;
use crate::thread_comm::PopupAnchorRect;
use neomacs_display_protocol::frame_chrome::{ChromeAction, FramePoint, FrameRect};

pub(super) fn frame_chrome_hit(
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    x: f32,
    y: f32,
) -> Option<(&ChromeAction, FrameRect)> {
    frame.frame_chrome.hit_test(FramePoint::new(x, y))
}

pub(super) fn frame_chrome_owns_pointer(
    frame: &crate::core::frame_glyphs::FrameGlyphBuffer,
    x: f32,
    y: f32,
) -> bool {
    frame.frame_chrome.bands().iter().any(|band| {
        let bounds = band.bounds();
        x >= bounds.x()
            && x < bounds.x() + bounds.width()
            && y >= bounds.y()
            && y < bounds.y() + bounds.height()
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct MenuBarHit {
    pub(super) index: u32,
    pub(super) key: String,
    pub(super) menu_x: f32,
    pub(super) anchor: PopupAnchorRect,
}

impl RenderApp {
    /// Translate winit key to X11 keysym
    pub(super) fn translate_key(key: &Key) -> u32 {
        match key {
            Key::Named(named) => match named {
                // Function keys
                NamedKey::F1 => 0xffbe,
                NamedKey::F2 => 0xffbf,
                NamedKey::F3 => 0xffc0,
                NamedKey::F4 => 0xffc1,
                NamedKey::F5 => 0xffc2,
                NamedKey::F6 => 0xffc3,
                NamedKey::F7 => 0xffc4,
                NamedKey::F8 => 0xffc5,
                NamedKey::F9 => 0xffc6,
                NamedKey::F10 => 0xffc7,
                NamedKey::F11 => 0xffc8,
                NamedKey::F12 => 0xffc9,
                // Navigation
                NamedKey::Escape => 0xff1b,
                NamedKey::Enter => 0xff0d,
                NamedKey::Tab => 0xff09,
                NamedKey::Backspace => 0xff08,
                NamedKey::Delete => 0xffff,
                NamedKey::Insert => 0xff63,
                NamedKey::Home => 0xff50,
                NamedKey::End => 0xff57,
                NamedKey::PageUp => 0xff55,
                NamedKey::PageDown => 0xff56,
                NamedKey::ArrowLeft => 0xff51,
                NamedKey::ArrowUp => 0xff52,
                NamedKey::ArrowRight => 0xff53,
                NamedKey::ArrowDown => 0xff54,
                // Whitespace
                NamedKey::Space => 0x20,
                // Modifier keys are handled via ModifiersChanged, not as key events.
                // They fall through to the default `_ => 0` which suppresses them.
                // Other
                NamedKey::PrintScreen => 0xff61,
                NamedKey::ScrollLock => 0xff14,
                NamedKey::Pause => 0xff13,
                _ => 0,
            },
            Key::Character(c) => c.chars().next().map(|ch| ch as u32).unwrap_or(0),
            _ => 0,
        }
    }

    /// Prefer committed text over logical-key fallback for printable input
    /// when no command modifiers are active.
    pub(super) fn translate_committed_text(text: &str, modifiers: u32) -> Option<Vec<u32>> {
        let command_modifiers_active =
            modifiers & (NEOMACS_CTRL_MASK | NEOMACS_META_MASK | NEOMACS_SUPER_MASK) != 0;
        if command_modifiers_active {
            return None;
        }

        let keysyms: Vec<u32> = text
            .chars()
            .filter(|ch| !ch.is_control())
            .map(|ch| ch as u32)
            .filter(|keysym| *keysym != 0)
            .collect();

        if keysyms.is_empty() {
            None
        } else {
            Some(keysyms)
        }
    }

    /// Return whether a `KeyboardInput` event should use its committed-text
    /// payload before falling back to its logical key.
    ///
    /// GNU's GUI backends classify physical function keys like Backspace from
    /// their keysyms first. Some window systems also attach control text such
    /// as `\b` to that same key event; using the text first would turn
    /// Backspace into `C-h` and bypass GNU's `[backspace] -> DEL` translation.
    pub(super) fn should_use_committed_text(logical_key: &Key) -> bool {
        matches!(logical_key, Key::Character(_))
    }

    /// Extract a single control-character keysym from committed text.
    ///
    /// Some backends report `Ctrl+n` / `Ctrl+p` style input as a control-text
    /// payload even when modifier-state delivery is delayed relative to the key
    /// event. Preserve that byte so the keyboard layer can recover the GNU
    /// control event instead of silently degrading it into plain text.
    pub(super) fn translate_control_text(text: &str) -> Option<u32> {
        let mut chars = text.chars();
        let ch = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        if ch.is_control() {
            Some(ch as u32)
        } else {
            None
        }
    }

    /// Hit-test toolbar items. Returns the index of the item under (x, y), or None.
    #[cfg(test)]
    pub(super) fn toolbar_hit_test(&self, x: f32, y: f32) -> Option<u32> {
        let frame = self
            .frame_windows
            .primary_window()?
            .render
            .compositor
            .current_frame
            .as_ref()?;
        match frame_chrome_hit(frame, x, y)?.0 {
            ChromeAction::InvokeToolBarItem { index } => Some(*index),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn toolbar_y_origin(&self) -> f32 {
        self.primary_chrome_band_bounds(
            neomacs_display_protocol::frame_chrome::FrameChromeKind::ToolBar,
        )
        .map_or(0.0, |bounds| bounds.y())
    }

    /// Hit-test a tab-bar presentation target.
    #[cfg(test)]
    pub(super) fn tab_bar_hit_test(&self, x: f32, y: f32) -> Option<(u64, u32)> {
        Self::frame_window_tab_bar_hit_test(self.frame_windows.primary_window()?, x, y)
            .map(|target| (target.presentation().get(), target.interaction().get()))
    }

    /// Hit-test menu bar items. Returns the item under (x, y), or None.
    #[cfg(test)]
    pub(super) fn menu_bar_hit_test(&self, x: f32, _y: f32) -> Option<MenuBarHit> {
        self.primary_menu_hit_test(x, _y)
    }

    #[cfg(test)]
    fn primary_chrome_band_bounds(
        &self,
        kind: neomacs_display_protocol::frame_chrome::FrameChromeKind,
    ) -> Option<FrameRect> {
        self.frame_windows
            .primary_window()
            .and_then(|ws| ws.render.compositor.current_frame.as_ref())
            .and_then(|frame| frame.frame_chrome.band(kind))
            .map(|band| band.bounds())
    }

    #[cfg(test)]
    fn primary_menu_hit_test(&self, x: f32, y: f32) -> Option<MenuBarHit> {
        let frame = self
            .frame_windows
            .primary_window()?
            .render
            .compositor
            .current_frame
            .as_ref()?;
        let (ChromeAction::OpenMenu { index, key }, bounds) = frame_chrome_hit(frame, x, y)? else {
            return None;
        };
        Some(MenuBarHit {
            index: *index,
            key: key.clone(),
            menu_x: bounds.x(),
            anchor: PopupAnchorRect::new(bounds.x(), bounds.y(), bounds.width(), bounds.height()),
        })
    }

    /// Detect if the mouse is on a resize edge of a borderless window.
    /// Returns the resize direction if within the border zone, or None.
    pub(super) fn detect_resize_edge_for_chrome(
        chrome: &WindowChrome,
        logical_width: f32,
        logical_height: f32,
        x: f32,
        y: f32,
    ) -> Option<winit::window::ResizeDirection> {
        use winit::window::ResizeDirection;
        if chrome.decorations_enabled {
            return None;
        }
        let w = logical_width;
        let h = logical_height;
        let border = 5.0_f32;
        let on_left = x < border;
        let on_right = x >= w - border;
        let on_top = y < border;
        let on_bottom = y >= h - border;
        match (on_left, on_right, on_top, on_bottom) {
            (true, _, true, _) => Some(ResizeDirection::NorthWest),
            (_, true, true, _) => Some(ResizeDirection::NorthEast),
            (true, _, _, true) => Some(ResizeDirection::SouthWest),
            (_, true, _, true) => Some(ResizeDirection::SouthEast),
            (true, _, _, _) => Some(ResizeDirection::West),
            (_, true, _, _) => Some(ResizeDirection::East),
            (_, _, true, _) => Some(ResizeDirection::North),
            (_, _, _, true) => Some(ResizeDirection::South),
            _ => None,
        }
    }

    /// Detect if the mouse is on a resize edge of the primary borderless window.
    /// Returns the resize direction if within the border zone, or None.
    #[cfg(test)]
    pub(super) fn detect_resize_edge(
        &self,
        x: f32,
        y: f32,
    ) -> Option<winit::window::ResizeDirection> {
        let (logical_width, logical_height) =
            self.frame_windows
                .primary_window()
                .map_or((0.0, 0.0), |ws| {
                    let (w, h) = ws.native_size();
                    let s = ws.scale_factor() as f32;
                    (w as f32 / s, h as f32 / s)
                });
        Self::detect_resize_edge_for_chrome(
            self.frame_windows
                .primary_window()
                .expect("primary window state")
                .chrome(),
            logical_width,
            logical_height,
            x,
            y,
        )
    }

    /// Title bar button width in logical pixels.
    pub(super) const TITLEBAR_BUTTON_WIDTH: f32 = 46.0;

    /// Check if a point is in the custom title bar area.
    /// Returns: 0 = not in title bar, 1 = drag area, 2 = close, 3 = maximize, 4 = minimize
    pub(super) fn titlebar_hit_test_for_chrome(
        chrome: &WindowChrome,
        logical_width: f32,
        x: f32,
        y: f32,
    ) -> u32 {
        if chrome.decorations_enabled || chrome.is_fullscreen || chrome.titlebar_height <= 0.0 {
            return 0;
        }
        let w = logical_width;
        let tb_h = chrome.titlebar_height;
        if y >= tb_h {
            return 0; // Below title bar
        }
        // Buttons are on the right: [minimize] [maximize] [close]
        let btn_w = Self::TITLEBAR_BUTTON_WIDTH;
        let close_x = w - btn_w;
        let max_x = w - btn_w * 2.0;
        let min_x = w - btn_w * 3.0;
        if x >= close_x {
            2 // Close
        } else if x >= max_x {
            3 // Maximize
        } else if x >= min_x {
            4 // Minimize
        } else {
            1 // Drag area
        }
    }

    /// Check if a point is in the primary custom title bar area.
    /// Returns: 0 = not in title bar, 1 = drag area, 2 = close, 3 = maximize, 4 = minimize
    #[cfg(test)]
    pub(super) fn titlebar_hit_test(&self, x: f32, y: f32) -> u32 {
        let (logical_width, _) = self
            .frame_windows
            .primary_window()
            .map_or((0.0, 0.0), |ws| {
                let (w, h) = ws.native_size();
                let s = ws.scale_factor() as f32;
                (w as f32 / s, h as f32 / s)
            });
        Self::titlebar_hit_test_for_chrome(
            self.frame_windows
                .primary_window()
                .expect("primary window state")
                .chrome(),
            logical_width,
            x,
            y,
        )
    }

    pub(super) fn frame_window_titlebar_hit_test(
        window_state: &GuiFrameWindowState,
        x: f32,
        y: f32,
    ) -> u32 {
        Self::titlebar_hit_test_for_chrome(
            window_state.chrome(),
            window_state.native_size().0 as f32 / window_state.scale_factor() as f32,
            x,
            y,
        )
    }
}

#[cfg(test)]
#[path = "input_test.rs"]
mod tests;
