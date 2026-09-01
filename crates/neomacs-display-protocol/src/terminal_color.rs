//! The colour a REALIZED face carries on a terminal frame.
//!
//! GNU's realized face stores one number per colour slot -- `face->foreground`,
//! `face->background`, `face->underline_color`, all `unsigned long`
//! (src/dispextern.h:1919-1936) -- and `realize_tty_face` fills it through
//! `map_tty_color` (src/xfaces.c:6620-6694), which takes the INDEX part of
//! `tty-color-desc`'s `(NAME INDEX R G B)` and throws the RGB away.  The writer
//! then hands exactly that number to terminfo `setaf`/`setab`
//! (`turn_on_face`, src/term.c:2093-2117) and never looks at a colour again.
//!
//! So a terminal colour is not an RGB triple that someone quantizes later; it is
//! the number Lisp already computed.  That matters because the palette
//! `tty-color-desc` searched is Lisp DATA: `tty-color-alist` is registered per
//! terminal by `lisp/term/<TERM>.el` and can be changed at any time by
//! `tty-color-define` (lisp/term/tty-colors.el:839-861).  Nothing outside Lisp
//! can re-derive that number.

/// The number GNU's realized face carries for a terminal frame.
///
/// GNU packs both readings into the one `unsigned long` and disambiguates them
/// at the writer with `tty->TF_rgb_separate`; naming them as variants here makes
/// the two spellings unconfusable -- an [`Indexed`](Self::Indexed) colour cannot
/// be emitted as `38;2;R;G;B`, and a [`Direct`](Self::Direct) colour cannot be
/// emitted as `38;5;N`.
///
/// There is deliberately NO constructor from an RGB triple.  The only way to
/// obtain a value is [`TerminalColor::from_tty_color_desc`], i.e. from the
/// number `tty-color-desc` returned, so no layer downstream of face realization
/// can invent an index by quantizing on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TerminalColor {
    /// An entry of this terminal's `tty-color-alist`: the INDEX
    /// `tty-color-desc` returned (lisp/term/tty-colors.el:975-987), which came
    /// either from an exact name match or from `tty-color-approximate`'s search
    /// over the registered palette (:875-915).
    Indexed(u16),
    /// A direct 24-bit colour.  On a terminal whose `display-color-cells` is
    /// 16777216, `tty-color-desc` answers `tty-color-24bit`'s packed
    /// `0xRRGGBB` pixel in the INDEX position (lisp/term/tty-colors.el:829-838),
    /// and GNU's `TF_rgb_separate` `setaf` splits it back into three channels.
    Direct { r: u8, g: u8, b: u8 },
}

impl TerminalColor {
    /// Read the INDEX element of a `tty-color-desc` answer under a terminal
    /// reporting `color_cells` cells.
    ///
    /// The 16777216 test is `tty-color-24bit`'s own
    /// (lisp/term/tty-colors.el:834): above it the INDEX *is* the packed pixel,
    /// below it the INDEX is a palette subscript.  Returns `None` for a value
    /// that is not a colour at all -- a negative number, or one wider than the
    /// palette subscript a terminfo `setaf` can take -- so a malformed Lisp
    /// answer cannot become a colour.
    #[must_use]
    pub fn from_tty_color_desc(index: i64, color_cells: i64) -> Option<Self> {
        if index < 0 {
            return None;
        }
        if color_cells >= 16_777_216 {
            let pixel = u32::try_from(index).ok()?;
            return Some(Self::Direct {
                r: ((pixel >> 16) & 0xFF) as u8,
                g: ((pixel >> 8) & 0xFF) as u8,
                b: (pixel & 0xFF) as u8,
            });
        }
        u16::try_from(index).ok().map(Self::Indexed)
    }

    /// The single number GNU's realized face carries for this colour.
    ///
    /// `setaf`/`setab` take it apart with `TF_rgb_separate` (src/term.c:2098),
    /// but `Setulc` does not: GNU installs one fixed string for the underline
    /// colour and passes the realized slot to it whole (src/term.c:4708),
    ///
    /// ```text
    ///   \e[58:2::%p1%{65536}%/%d:%p1%{256}%/%{255}%&%d:%p1%{255}%&%dm
    /// ```
    ///
    /// which divides that one parameter into three channels regardless of what
    /// the terminal's colour depth made it mean.  Recovering the number is
    /// therefore how the underline colour is written, and the conflation it
    /// produces below 24-bit colour is GNU's, measured: on TERM=tmux-256color
    /// with no COLORTERM, `(:underline (:color "red"))` realizes to palette
    /// subscript 1 and GNU emits `ESC[58:2::0:0:1m`.
    #[must_use]
    pub fn realized_pixel(self) -> u32 {
        match self {
            Self::Indexed(index) => u32::from(index),
            Self::Direct { r, g, b } => (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
        }
    }
}
