//! This terminal's `tty-color-alist`, carried as data.
//!
//! GNU realizes a face colour string by calling Lisp: `map_tty_color`
//! (src/xfaces.c:6620-6694) looks the canonicalized NAME up in
//! `tty-color-alist`, and otherwise `load_color` -> `tty_lookup_color`
//! (src/xfaces.c:1083-1138) calls `tty-color-desc`, which falls back to
//! `tty-color-approximate`'s nearest search over the same list
//! (lisp/term/tty-colors.el:875-987).
//!
//! neovm-core does exactly that, because face realization has the evaluator.
//! The LAYOUT engine does not: it is pure over a snapshot of Lisp data and
//! calls no Lisp function anywhere, and it is where an ANONYMOUS attribute
//! plist -- `(:foreground "#5f8787")` on a text property, an overlay, or in
//! `face-remapping-alist` -- has its colours realized. GNU has no such split;
//! `merge_face_ref` folds a plist into the same lface vector and one
//! realization follows.
//!
//! Until that split is closed, the layout engine carries the palette itself and
//! runs GNU's search over it. That is one search over the terminal's REAL
//! palette, not a hardcoded table: `tty-color-define` moves it, and the answer
//! moves with it. The terminal writer holds neither the palette nor the search.

use crate::terminal_color::TerminalColor;

/// One `(NAME INDEX R G B)` row of `tty-color-alist`.
///
/// `rgb` is `None` for a row registered without RGB values, which
/// `tty-color-approximate` skips: "If the RGB values of the candidate color are
/// unknown, we never consider it for approximating another color"
/// (lisp/term/tty-colors.el:895-896).  Such a row is still reachable BY NAME,
/// which is the branch `map_tty_color` takes first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TtyPaletteEntry {
    pub name: String,
    pub index: i64,
    pub rgb: Option<(u8, u8, u8)>,
}

/// The terminal's registered colours, in the order `tty-color-alist` holds
/// them -- which decides exact ties, because the search keeps the FIRST
/// strictly-smaller distance (`(if (and (< dist best-distance) ...)`,
/// lisp/term/tty-colors.el:903).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TtyPalette {
    entries: Vec<TtyPaletteEntry>,
    /// Canonicalized name -> row, so the name branch is a lookup rather than a
    /// scan.  A 24-bit terminal registers every X colour name it knows -- 665
    /// rows measured on `xterm-256color` with `COLORTERM=truecolor` -- and this
    /// branch runs once per colour string of every anonymous attribute plist
    /// the layout engine realizes.
    by_name: std::collections::HashMap<String, usize>,
    /// `display-color-cells`, the number `tty-color-24bit` keys on
    /// (lisp/term/tty-colors.el:834).
    color_cells: i64,
}

impl TtyPalette {
    #[must_use]
    pub fn new(entries: Vec<TtyPaletteEntry>, color_cells: i64) -> Self {
        // `tty-modify-color-alist` replaces an existing row in place rather than
        // appending, so a name appears once; keep the FIRST if a malformed list
        // ever repeats one, which is what `assoc` would find.
        let mut by_name = std::collections::HashMap::with_capacity(entries.len());
        for (at, entry) in entries.iter().enumerate() {
            by_name.entry(entry.name.clone()).or_insert(at);
        }
        Self {
            entries,
            by_name,
            color_cells,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn entries(&self) -> &[TtyPaletteEntry] {
        &self.entries
    }

    /// `tty-color-canonicalize` (lisp/term/tty-colors.el:820-826): all-lower
    /// case with blanks removed, and only when there is something to change.
    #[must_use]
    pub fn canonicalize(name: &str) -> String {
        if name.chars().any(|c| c.is_ascii_uppercase() || c == ' ') {
            name.chars()
                .filter(|c| *c != ' ')
                .flat_map(char::to_lowercase)
                .collect()
        } else {
            name.to_owned()
        }
    }

    /// The `assoc` half of `tty-color-desc`: an exact match on the
    /// canonicalized name, which `map_tty_color` takes before anything else
    /// (src/xfaces.c:6640-6648).
    ///
    /// This is the half no RGB search can reproduce. `tty-color-define` can put
    /// a name at any index, including one its own RGB would never approximate
    /// to, and GNU emits that index.
    #[must_use]
    pub fn named(&self, name: &str) -> Option<(TerminalColor, (u8, u8, u8))> {
        let canonical = Self::canonicalize(name);
        let entry = &self.entries[*self.by_name.get(&canonical)?];
        let color = TerminalColor::from_tty_color_desc(entry.index, self.color_cells)?;
        Some((color, entry.rgb.unwrap_or((0, 0, 0))))
    }

    /// The rest of `tty-color-desc` for a colour given by its RGB: the 24-bit
    /// pixel on a direct-colour terminal (`tty-color-24bit`,
    /// lisp/term/tty-colors.el:829-838), otherwise `tty-color-approximate`'s
    /// nearest entry (:875-915).
    #[must_use]
    pub fn approximate(&self, r: u8, g: u8, b: u8) -> Option<(TerminalColor, (u8, u8, u8))> {
        if self.color_cells >= 16_777_216 {
            return Some((TerminalColor::Direct { r, g, b }, (r, g, b)));
        }
        let entry = self.nearest(r, g, b)?;
        let color = TerminalColor::from_tty_color_desc(entry.index, self.color_cells)?;
        Some((color, entry.rgb.unwrap_or((0, 0, 0))))
    }

    /// `tty-color-approximate` (lisp/term/tty-colors.el:875-915): the smallest
    /// squared 8-bit RGB distance over the WHOLE palette, skipping candidates
    /// that sit on the gray diagonal whenever the REQUESTED colour is 0.065
    /// radians or more off it (`tty-color-off-gray-diag`, :866-873).
    fn nearest(&self, r: u8, g: u8, b: u8) -> Option<&TtyPaletteEntry> {
        let favor_non_gray = off_gray_diagonal(r, g, b) >= 0.065;
        let mut best: Option<(&TtyPaletteEntry, u32)> = None;
        for entry in &self.entries {
            let Some((cr, cg, cb)) = entry.rgb else {
                continue;
            };
            if favor_non_gray && cr == cg && cg == cb {
                continue;
            }
            let difference = |lhs: u8, rhs: u8| -> u32 {
                let delta = i32::from(lhs) - i32::from(rhs);
                (delta * delta) as u32
            };
            let distance = difference(r, cr) + difference(g, cg) + difference(b, cb);
            if best.is_none_or(|(_, best_distance)| distance < best_distance) {
                best = Some((entry, distance));
            }
        }
        // GNU returns nil when every candidate was skipped, and its callers
        // treat that as "not resolved" rather than substituting anything.
        best.map(|(entry, _)| entry)
    }
}

/// Angle between a colour and the gray diagonal of the RGB cube, GNU
/// `tty-color-off-gray-diag` (lisp/term/tty-colors.el:866-873).
fn off_gray_diagonal(r: u8, g: u8, b: u8) -> f64 {
    let (r, g, b) = (f64::from(r), f64::from(g), f64::from(b));
    let magnitude = (3.0 * (r * r + g * g + b * b)).sqrt();
    if magnitude < 1.0 {
        return 0.0;
    }
    ((r + g + b) / magnitude).clamp(-1.0, 1.0).acos()
}
