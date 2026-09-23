//! The fixed xterm 256-color palette used by terminal emulators.
//!
//! This is distinct from [`crate::TtyPalette`], which carries GNU Emacs's
//! mutable, terminal-specific `tty-color-alist`. This module resolves an index
//! after a terminal has already emitted it using xterm's standard RGB table.

/// Resolve one xterm 256-color palette index to its 8-bit RGB components.
#[must_use]
pub const fn xterm_256_rgb(index: u8) -> (u8, u8, u8) {
    const ANSI: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 0, 0),
        (0, 205, 0),
        (205, 205, 0),
        (0, 0, 238),
        (205, 0, 205),
        (0, 205, 205),
        (229, 229, 229),
        (127, 127, 127),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (92, 92, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];
    const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];

    match index {
        0..=15 => ANSI[index as usize],
        16..=231 => {
            let offset = index - 16;
            (
                CUBE[(offset / 36) as usize],
                CUBE[((offset % 36) / 6) as usize],
                CUBE[(offset % 6) as usize],
            )
        }
        232..=255 => {
            let gray = 8 + (index - 232) * 10;
            (gray, gray, gray)
        }
    }
}

#[cfg(test)]
mod tests;
