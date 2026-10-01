//! Input translation and window chrome hit-testing.

use crate::backend::wgpu::{
    NEOMACS_ALT_MASK, NEOMACS_CTRL_MASK, NEOMACS_HYPER_MASK, NEOMACS_META_MASK, NEOMACS_SUPER_MASK,
};
use winit::keyboard::{Key, NamedKey, NativeKey};

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
    /// Translate winit key to X11 keysym.
    ///
    /// Every key gets an identity here, the way GNU's backends hand
    /// `keyboard.c` whatever the toolkit reported and let `modify_event_symbol`
    /// name it.  Only a modifier is not a keystroke at all — those arrive
    /// through `ModifiersChanged` — and a key this table does not spell yet is
    /// logged rather than dropped in silence, so the gap is visible instead of
    /// looking like "unsupported".
    pub(super) fn translate_key(key: &Key) -> u32 {
        match key {
            Key::Named(named) => match named {
                // Function keys.  The X11 block is contiguous from XK_F1
                // (0xffbe) to XK_F35 (0xffe0), so all of them are ordinary
                // keys and not an allow-list.
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
                NamedKey::F13 => 0xffca,
                NamedKey::F14 => 0xffcb,
                NamedKey::F15 => 0xffcc,
                NamedKey::F16 => 0xffcd,
                NamedKey::F17 => 0xffce,
                NamedKey::F18 => 0xffcf,
                NamedKey::F19 => 0xffd0,
                NamedKey::F20 => 0xffd1,
                NamedKey::F21 => 0xffd2,
                NamedKey::F22 => 0xffd3,
                NamedKey::F23 => 0xffd4,
                NamedKey::F24 => 0xffd5,
                NamedKey::F25 => 0xffd6,
                NamedKey::F26 => 0xffd7,
                NamedKey::F27 => 0xffd8,
                NamedKey::F28 => 0xffd9,
                NamedKey::F29 => 0xffda,
                NamedKey::F30 => 0xffdb,
                NamedKey::F31 => 0xffdc,
                NamedKey::F32 => 0xffdd,
                NamedKey::F33 => 0xffde,
                NamedKey::F34 => 0xffdf,
                NamedKey::F35 => 0xffe0,
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
                // Other keys that already have a place in the X11 block.
                NamedKey::PrintScreen => 0xff61,
                NamedKey::ScrollLock => 0xff14,
                NamedKey::Pause => 0xff13,
                // The 0xff65-0xff69 misc-function block, which GNU spells
                // `undo`, `redo`, `menu`, `find` and `cancel`.
                NamedKey::Undo => 0xff65,
                NamedKey::Redo => 0xff66,
                NamedKey::ContextMenu => 0xff67,
                NamedKey::Find => 0xff68,
                NamedKey::Cancel => 0xff69,
                // The XF86 block: what the XF86Back/XF86Forward/XF86Copy keys
                // are on X11 and Wayland, and what GNU binds as <XF86Back>.
                NamedKey::BrowserBack => 0x1008ff26,
                NamedKey::BrowserForward => 0x1008ff27,
                NamedKey::Copy => 0x1008ff57,
                NamedKey::Cut => 0x1008ff58,
                NamedKey::Paste => 0x1008ff6d,
                // Everything else winit's xkb keymap can name — the media,
                // launch, browser, mail, power, IME, 3270 and ISO group
                // families.  Each value is the keysym winit itself matched to
                // produce this `NamedKey` (winit-common/src/xkb/keymap.rs), so
                // it is the keysym an X11/Wayland backend would have handed
                // GNU.  The trailing comment is the symbol that keysym becomes:
                // GNU's own table name where it has one (`henkan`, `kana-lock`,
                // `eisu-shift`), XKeysymToString's spelling otherwise
                // (`XF86AudioRaiseVolume`, `3270_Attn`).  So both
                // `(kbd "<XF86AudioRaiseVolume>")` and `(kbd "<henkan>")` are
                // what a GNU config would write.  Derivation:
                // docs/design/input-keysyms.md.
                NamedKey::AllCandidates => 0x00ff3d, // MultipleCandidate
                NamedKey::Alphanumeric => 0x00ff2f,  // eisu-shift
                NamedKey::Attn => 0x00fd0e,          // 3270_Attn
                NamedKey::AudioVolumeDown => 0x1008ff11, // XF86AudioLowerVolume
                NamedKey::AudioVolumeMute => 0x1008ff12, // XF86AudioMute
                NamedKey::AudioVolumeUp => 0x1008ff13, // XF86AudioRaiseVolume
                NamedKey::BrightnessDown => 0x1008ff03, // XF86MonBrightnessDown
                NamedKey::BrightnessUp => 0x1008ff02, // XF86MonBrightnessUp
                NamedKey::BrowserFavorites => 0x1008ff30, // XF86Favorites
                NamedKey::BrowserHome => 0x1008ff18, // XF86HomePage
                NamedKey::BrowserRefresh => 0x1008ff29, // XF86Refresh
                NamedKey::BrowserSearch => 0x1008ff1b, // XF86Search
                NamedKey::Clear => 0x00ff0b,         // clear
                NamedKey::Close => 0x1008ff56,       // XF86Close
                NamedKey::CodeInput => 0x00ff37,     // Codeinput
                NamedKey::Compose => 0x00ff20,       // Multi_key
                NamedKey::Convert => 0x00ff23,       // henkan
                NamedKey::CrSel => 0x00fd1c,         // 3270_CursorSelect
                NamedKey::Eject => 0x1008ff2c,       // XF86Eject
                NamedKey::EraseEof => 0x00fd06,      // 3270_EraseEOF
                NamedKey::ExSel => 0x00fd1b,         // 3270_ExSelect
                NamedKey::Execute => 0x00ff62,       // execute
                NamedKey::GroupFirst => 0x00fe0c,    // key-12
                NamedKey::GroupLast => 0x00fe0e,     // key-14
                NamedKey::GroupNext => 0x00fe08,     // key-8
                NamedKey::GroupPrevious => 0x00fe0a, // key-10
                NamedKey::Hankaku => 0x00ff29,       // hankaku
                NamedKey::Help => 0x00ff6a,          // help
                NamedKey::Hibernate => 0x1008ffa8,   // XF86Hibernate
                NamedKey::Hiragana => 0x00ff25,      // hiragana
                NamedKey::HiraganaKatakana => 0x00ff27, // hiragana-katakana
                NamedKey::KanaMode => 0x00ff2d,      // kana-lock
                NamedKey::KanjiMode => 0x00ff21,     // kanji
                NamedKey::LaunchApplication1 => 0x1008ff33, // XF86MyComputer
                NamedKey::LaunchApplication2 => 0x1008ff1d, // XF86Calculator
                NamedKey::LaunchCalendar => 0x1008ff20, // XF86Calendar
                NamedKey::LaunchMail => 0x1008ff19,  // XF86Mail
                NamedKey::LaunchMediaPlayer => 0x1008ff87, // XF86Video
                NamedKey::LaunchMusicPlayer => 0x1008ff92, // XF86Music
                NamedKey::LaunchPhone => 0x1008ff6e, // XF86Phone
                NamedKey::LaunchScreenSaver => 0x1008ff2d, // XF86ScreenSaver
                NamedKey::LaunchSpreadsheet => 0x1008ff5c, // XF86Excel
                NamedKey::LaunchWebBrowser => 0x1008ff2e, // XF86WWW
                NamedKey::LaunchWebCam => 0x1008ff8f, // XF86WebCam
                NamedKey::LaunchWordProcessor => 0x1008ff89, // XF86Word
                NamedKey::LogOff => 0x1008ff61,      // XF86LogOff
                NamedKey::MailForward => 0x1008ff90, // XF86MailForward
                NamedKey::MailReply => 0x1008ff72,   // XF86Reply
                NamedKey::MailSend => 0x1008ff7b,    // XF86Send
                NamedKey::MediaAudioTrack => 0x1008ff9b, // XF86AudioCycleTrack
                NamedKey::MediaFastForward => 0x1008ff97, // XF86AudioForward
                NamedKey::MediaPause => 0x1008ff31,  // XF86AudioPause
                NamedKey::MediaPlay => 0x1008ff14,   // XF86AudioPlay
                NamedKey::MediaRecord => 0x1008ff1c, // XF86AudioRecord
                NamedKey::MediaRewind => 0x1008ff3e, // XF86AudioRewind
                NamedKey::MediaStop => 0x1008ff15,   // XF86AudioStop
                NamedKey::MediaTrackNext => 0x1008ff17, // XF86AudioNext
                NamedKey::MediaTrackPrevious => 0x1008ff16, // XF86AudioPrev
                NamedKey::ModeChange => 0x00ff7e,    // Mode_switch
                NamedKey::New => 0x1008ff68,         // XF86New
                NamedKey::NonConvert => 0x00ff22,    // muhenkan
                NamedKey::Open => 0x1008ff6b,        // XF86Open
                NamedKey::Play => 0x00fd16,          // 3270_Play
                NamedKey::Power => 0x1008ff21,       // XF86PowerDown
                NamedKey::PreviousCandidate => 0x00ff3e, // PreviousCandidate
                NamedKey::RandomToggle => 0x1008ff99, // XF86AudioRandomPlay
                NamedKey::Romaji => 0x00ff24,        // romaji
                NamedKey::Save => 0x1008ff77,        // XF86Save
                NamedKey::Select => 0x00ff60,        // select
                NamedKey::SingleCandidate => 0x00ff3c, // SingleCandidate
                NamedKey::SpellCheck => 0x1008ff7c,  // XF86Spell
                NamedKey::SplitScreenToggle => 0x1008ff7d, // XF86SplitScreen
                NamedKey::Standby => 0x1008ff10,     // XF86Standby
                NamedKey::Subtitle => 0x1008ff9a,    // XF86Subtitle
                NamedKey::VideoModeNext => 0x1008fe22, // XF86Next_VMode
                NamedKey::WakeUp => 0x1008ff2b,      // XF86WakeUp
                NamedKey::Zenkaku => 0x00ff28,       // zenkaku
                NamedKey::ZenkakuHankaku => 0x00ff2a, // zenkaku-hankaku
                NamedKey::ZoomIn => 0x1008ff8b,      // XF86ZoomIn
                NamedKey::ZoomOut => 0x1008ff8c,     // XF86ZoomOut
                // Modifier keys are handled via ModifiersChanged, not as key
                // events.  They are listed explicitly so that "a modifier" and
                // "a key this table does not know" stay different answers.
                //
                // `Hyper` is legacy in the W3C spec (Meta is the modern key
                // value) but winit's xkb keymap still emits it for Hyper_L/R
                // (winit-common/src/xkb/keymap.rs:664-666), so it stays in
                // this arm rather than falling through to the "unmapped" log
                // below, which would fire on every Hyper keypress.
                #[allow(deprecated)]
                NamedKey::Shift
                | NamedKey::Control
                | NamedKey::Alt
                | NamedKey::AltGraph
                | NamedKey::CapsLock
                | NamedKey::Meta
                | NamedKey::Hyper
                | NamedKey::NumLock
                | NamedKey::Symbol
                | NamedKey::SymbolLock
                | NamedKey::Fn
                | NamedKey::FnLock => 0,
                // A key winit names and this block does not spell yet — the
                // media, launch and volume families, whose keysyms live in
                // the XF86 block.  Logged rather than silently zeroed: GNU
                // sees these as keysyms (XF86AudioPlay) and binds them, so
                // one landing here is a gap to close, not "unsupported".
                other => {
                    tracing::debug!("key has no keysym mapping yet: {:?}", other);
                    0
                }
            },
            Key::Character(c) => c.chars().next().map(|ch| ch as u32).unwrap_or(0),
            // A key winit could not name at all.  X11 and Wayland hand over
            // the raw keysym, which is already this port's identity; the
            // platforms whose native key is a scancode or a virtual-key code
            // get a reserved band, so the key keeps an identity and stays
            // bindable instead of being dropped.
            Key::Unidentified(native) => match native {
                NativeKey::Xkb(keysym) => *keysym,
                NativeKey::MacOS(scancode) => neovm_core::keyboard::native_key_macos(*scancode),
                NativeKey::Windows(virtual_key) => {
                    neovm_core::keyboard::native_key_windows(*virtual_key)
                }
                NativeKey::Android(keycode) => neovm_core::keyboard::native_key_android(*keycode),
                NativeKey::Unidentified => {
                    tracing::debug!("unidentified native key carries no identity");
                    0
                }
                other => {
                    tracing::debug!("native key kind has no identity yet: {:?}", other);
                    0
                }
            },
            // A dead key is compose state; its text arrives through the
            // committed-text path.
            Key::Dead(_) => 0,
        }
    }

    /// Prefer committed text over logical-key fallback for printable input
    /// when no command modifiers are active.
    ///
    /// GNU's shift-like vs control-like split (`src/nsterm.m:7318-7339`):
    /// any non-shift modifier bit makes the chord a command, which now
    /// includes the policy-cooked `A-' and `H-' bits — GNU cooks
    /// `parse_solitary_modifier("alt")` to a distinct modifier bit
    /// (`src/keyboard.c:7941`).
    pub(super) fn translate_committed_text(text: &str, modifiers: u32) -> Option<Vec<u32>> {
        let command_modifiers_active = modifiers
            & (NEOMACS_CTRL_MASK
                | NEOMACS_META_MASK
                | NEOMACS_SUPER_MASK
                | NEOMACS_ALT_MASK
                | NEOMACS_HYPER_MASK)
            != 0;
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
mod tests;
