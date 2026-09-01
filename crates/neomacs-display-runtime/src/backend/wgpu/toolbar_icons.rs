//! Embedded SVG icon data for the GPU-rendered toolbar.
//!
//! Icons are JetBrains-style: 20x20 viewBox, 1.5px stroke, rounded caps,
//! outline-only (no fill), currentColor stroke.

/// Lookup embedded SVG data by Emacs icon name.
pub fn get_icon_svg(name: &str) -> Option<&'static [u8]> {
    match name {
        "new" => Some(include_bytes!("../../../icons/toolbar/new.svg")),
        "open" => Some(include_bytes!("../../../icons/toolbar/open.svg")),
        "diropen" => Some(include_bytes!("../../../icons/toolbar/diropen.svg")),
        "close" => Some(include_bytes!("../../../icons/toolbar/close.svg")),
        "save" => Some(include_bytes!("../../../icons/toolbar/save.svg")),
        "undo" => Some(include_bytes!("../../../icons/toolbar/undo.svg")),
        "redo" => Some(include_bytes!("../../../icons/toolbar/redo.svg")),
        "cut" => Some(include_bytes!("../../../icons/toolbar/cut.svg")),
        "copy" => Some(include_bytes!("../../../icons/toolbar/copy.svg")),
        "paste" => Some(include_bytes!("../../../icons/toolbar/paste.svg")),
        "search" | "search-replace" => Some(include_bytes!("../../../icons/toolbar/search.svg")),
        "help" => Some(include_bytes!("../../../icons/toolbar/help.svg")),
        "bookmark" => Some(include_bytes!("../../../icons/toolbar/bookmark.svg")),
        "spell" => Some(include_bytes!("../../../icons/toolbar/spell.svg")),
        _ => None,
    }
}
