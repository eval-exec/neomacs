//! How far redisplay scrolls a window to bring point back into view.
//!
//! GNU makes this decision in one place — `try_scrolling` (src/xdisp.c:19359)
//! plus its `recenter:` fallback (src/xdisp.c:21108) — from
//! `scroll-conservatively`, `scroll-step` and `scroll-margin`. Two sites here
//! need the same answer: the pre-layout window-start estimate
//! (`display_buffer_window_source`) and the post-layout visibility retry
//! (`display_text_window_row_lifecycle`). [`ScrollPolicy`] is that one decode,
//! so both sites match on intent rather than re-deriving the precedence.
//!
//! Not modelled: `scroll-up-aggressively` / `scroll-down-aggressively` (GNU
//! xdisp.c:19402, 21144). Those are buffer-local and default to nil; a buffer
//! that sets them gets [`ScrollPolicy::Recenter`] here instead of the
//! fractional placement GNU would use.

use neovm_core::window::ScrollMargin;

/// GNU `SCROLL_LIMIT` (src/xdisp.c:19349): a `scroll-conservatively` above this
/// never recenters — redisplay then always scrolls minimally.
pub(crate) const SCROLL_CONSERVATIVELY_LIMIT: i64 = 100;

/// GNU's cap on how far `try_scrolling` will search for point
/// (`scroll_limit * frame_line_height`, xdisp.c:19391).
pub(crate) const SCROLL_SEARCH_LIMIT_LINES: i64 = SCROLL_CONSERVATIVELY_LIMIT;

/// The resolved scrolling behavior for one window, per GNU `try_scrolling`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollPolicy {
    /// `scroll-conservatively` in `1..=SCROLL_LIMIT`: scroll just far enough to
    /// show point, but give up (and recenter) beyond `max_lines`.
    Conservative { max_lines: i64 },
    /// `scroll-conservatively` above `SCROLL_LIMIT`: scroll just far enough to
    /// show point, searching up to [`SCROLL_SEARCH_LIMIT_LINES`]. When even
    /// that fails GNU does not centre point — it puts it on the last row
    /// (xdisp.c:21150-21181).
    Unlimited,
    /// `scroll-step` (with `scroll-conservatively` 0): always scroll by exactly
    /// this many lines, and give up beyond that (GNU
    /// `amount_to_scroll = scroll_max`, xdisp.c:19498).
    Step { lines: i64 },
    /// GNU's defaults (both variables 0): `try_scrolling` is never entered, so
    /// any off-screen point goes straight to `recenter:`.
    Recenter,
}

/// What to do with the window start to bring an off-the-bottom point back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ForwardScroll {
    /// GNU `amount_to_scroll`: move the window start down this many display
    /// lines, keeping the text that stays on screen where it is.
    Advance { lines: i64 },
    /// GNU `SCROLLING_FAILED` -> `recenter:`: abandon the old start and pick a
    /// new one relative to point, leaving this many display lines above it.
    Recenter { lines_above_point: i64 },
}

impl ScrollPolicy {
    /// Resolve GNU's precedence once (xdisp.c:19388-19408 for the amount,
    /// xdisp.c:21067-21078 for whether `try_scrolling` runs at all).
    pub(crate) fn resolve(scroll_conservatively: i64, scroll_step: i64) -> Self {
        if scroll_conservatively > SCROLL_CONSERVATIVELY_LIMIT {
            Self::Unlimited
        } else if scroll_conservatively > 0 {
            Self::Conservative {
                max_lines: scroll_conservatively,
            }
        } else if scroll_step > 0 {
            Self::Step { lines: scroll_step }
        } else {
            Self::Recenter
        }
    }

    pub(crate) fn from_window_params(params: &crate::types::WindowParams) -> Self {
        // GNU passes `SCROLL_LIMIT + 1` for a mini-window when
        // `scroll-minibuffer-conservatively` is set — which it is by default
        // (xdisp.c:21083, bug#44070). A minibuffer that recentered would hide
        // the completion candidates point is walking through.
        if params.is_minibuffer() && params.scroll_minibuffer_conservatively {
            return Self::Unlimited;
        }
        Self::resolve(params.scroll_conservatively, params.scroll_step)
    }

    /// How far to search below the window before declaring point unreachable.
    /// GNU bounds the same `move_it_to` scan by `scroll_max` (xdisp.c:19434).
    pub(crate) fn search_limit_lines(self) -> i64 {
        match self {
            Self::Conservative { max_lines } => max_lines,
            Self::Unlimited => SCROLL_SEARCH_LIMIT_LINES,
            Self::Step { lines } => lines,
            // `try_scrolling` is not entered at all, so nothing is searched:
            // any off-screen point recenters. One line is enough to detect it.
            Self::Recenter => 0,
        }
    }

    /// Decide the scroll for a point that sits `dy` display lines below the
    /// last row the bottom `scroll-margin` leaves usable.
    ///
    /// `dy` mirrors GNU's `dy` in `try_scrolling` (xdisp.c:19443); it is
    /// positive exactly when point is off the bottom. `dy_is_bounded` is false
    /// when the caller stopped counting at [`Self::search_limit_lines`] — GNU's
    /// `dy > scroll_max` give-up (xdisp.c:19445).
    pub(crate) fn forward_scroll(
        self,
        dy: i64,
        dy_is_bounded: bool,
        window_rows: usize,
        scroll_margin: ScrollMargin,
    ) -> ForwardScroll {
        let recenter = || ForwardScroll::Recenter {
            lines_above_point: self.recenter_lines_above_point(window_rows, scroll_margin),
        };
        if dy <= 0 {
            return recenter();
        }
        match self {
            // GNU: amount_to_scroll = min (max (dy, 1 line), conservatively).
            // `dy` can never exceed `max_lines` here because the caller stops
            // counting there, so the min() is only a guard.
            Self::Conservative { max_lines } if dy_is_bounded && dy <= max_lines => {
                ForwardScroll::Advance {
                    lines: dy.min(max_lines),
                }
            }
            Self::Unlimited if dy_is_bounded => ForwardScroll::Advance { lines: dy },
            // GNU: amount_to_scroll = scroll_max, i.e. exactly `scroll-step`
            // lines — not `dy`. Point may still be off-screen afterwards; GNU
            // lets the next redisplay pass deal with that.
            Self::Step { lines } if dy_is_bounded && dy <= lines => {
                ForwardScroll::Advance { lines }
            }
            _ => recenter(),
        }
    }

    /// Display lines to leave above point when it moved UP out of the window,
    /// per `try_scrolling`'s backward branch (xdisp.c:19558-19613).
    ///
    /// Minimal scrolling here means "just enough": point lands on the top
    /// scroll-margin row and the text below it stays put. `lines_back` is how
    /// far above the window start point sits, with `bounded` false when the
    /// caller stopped counting at [`Self::search_limit_lines`].
    pub(crate) fn backward_scroll(
        self,
        lines_back: i64,
        bounded: bool,
        window_rows: usize,
        scroll_margin: ScrollMargin,
    ) -> i64 {
        let margin = top_margin(window_rows, scroll_margin);
        let within_budget = bounded && lines_back <= self.search_limit_lines();
        match self {
            Self::Conservative { .. } | Self::Unlimited if within_budget => margin,
            // GNU scrolls the window start back by exactly `scroll-step` lines
            // (`amount_to_scroll = scroll_max`, xdisp.c:19615), so point lands
            // wherever that leaves it: `lines - lines_back` rows below the new
            // start, never above the top margin.
            Self::Step { lines } if within_budget => (lines - lines_back).max(margin),
            // `recenter:` again — except that scroll-conservatively > 100 puts
            // point at the top margin rather than the middle when it is
            // scrolling backward (`centering_position = margin`, xdisp.c:21183).
            Self::Unlimited => margin,
            _ => (window_rows as i64 / 2).max(0),
        }
    }

    /// GNU `recenter:`'s `centering_position` expressed in display lines above
    /// point (xdisp.c:21169-21188).
    fn recenter_lines_above_point(self, window_rows: usize, scroll_margin: ScrollMargin) -> i64 {
        let rows = window_rows as i64;
        match self {
            // scroll-conservatively > 100 never centres: the window start goes
            // back just far enough to put point on the last usable row.
            Self::Unlimited => last_usable_row(window_rows, scroll_margin),
            // "Set the window start half the height of the window backward
            // from point." (xdisp.c:21186)
            _ => (rows / 2).max(0),
        }
    }
}

/// Effective `scroll-margin` in rows for a window `window_rows` display rows
/// tall (GNU `window_scroll_margin`, src/window.c:6207).
pub(crate) fn top_margin(window_rows: usize, scroll_margin: ScrollMargin) -> i64 {
    scroll_margin.lines(window_rows as i64)
}

/// Index of the lowest row point may occupy given the bottom `scroll-margin`
/// (GNU `scroll_margin_y`, xdisp.c:19420). Rows are 0-based, so a margin-free
/// window of `window_rows` rows allows `window_rows - 1`.
pub(crate) fn last_usable_row(window_rows: usize, scroll_margin: ScrollMargin) -> i64 {
    scroll_margin.last_usable_line(window_rows as i64)
}

/// Buffer text as the line-boundary searches below read it.
///
/// GNU finds line boundaries with `find_newline` (src/search.c), a `memchr`
/// over the contiguous halves of the gap buffer that converts between
/// character and byte positions only where a search starts and stops. These
/// searches do the same through the buffer's own newline searches, so a
/// search across a long line costs a vectorized scan of its bytes and a few
/// position conversions, not a conversion and a byte read per character.
///
/// A newline is one byte in Emacs's internal encoding and never occurs inside
/// a multibyte sequence, so a byte search finds exactly the newline characters.
pub(crate) trait LineText {
    fn charpos_to_bytepos(&self, charpos: i64) -> i64;
    fn bytepos_to_charpos(&self, bytepos: i64) -> i64;
    /// Byte position just past the `n`th newline in bytes `from..to`, and how
    /// many newlines were crossed: `n` when that newline was found, fewer when
    /// `to` came first.
    fn after_nth_newline(&self, from: i64, to: i64, n: usize) -> (i64, usize);
    /// Byte position of the last newline in bytes `from..to`.
    fn last_newline_before(&self, from: i64, to: i64) -> Option<i64>;
}

/// Count newlines in `from..to`, stopping once `limit` is exceeded.
///
/// Returns `(count, bounded)`; `bounded` is false when the scan hit `limit`
/// without reaching `to`, i.e. the true count is larger. GNU bounds the
/// equivalent `move_it_to` search the same way so a far jump never walks the
/// whole buffer (xdisp.c:19434).
pub(crate) fn count_lines_bounded(
    from: i64,
    to: i64,
    limit: i64,
    text: &(impl LineText + ?Sized),
) -> (i64, bool) {
    if from >= to {
        return (0, true);
    }
    let wanted = limit.max(0).saturating_add(1);
    let n = usize::try_from(wanted).unwrap_or(usize::MAX);
    let (_, crossed) = text.after_nth_newline(
        text.charpos_to_bytepos(from),
        text.charpos_to_bytepos(to),
        n,
    );
    if crossed == n {
        (wanted, false)
    } else {
        (crossed as i64, true)
    }
}

/// Start of the line `lines_above` lines above `point`, clamped to
/// `accessible_start`.
///
/// Always a line beginning: GNU's window start is the start of a display line,
/// never the newline that ends the previous one (which would render as an empty
/// leading row and cost a line of text).
pub(crate) fn line_start_above(
    point: i64,
    lines_above: i64,
    accessible_start: i64,
    text: &(impl LineText + ?Sized),
) -> i64 {
    if point <= accessible_start {
        return accessible_start;
    }
    let start = text.charpos_to_bytepos(accessible_start);
    let line_start_at_or_before = |pos: i64| {
        text.last_newline_before(start, pos)
            .map_or(start, |newline| newline + 1)
    };
    let mut pos = line_start_at_or_before(text.charpos_to_bytepos(point));
    let mut moved = 0;
    while moved < lines_above && pos > start {
        // `pos - 1` is the newline that ends the line above.
        pos = line_start_at_or_before(pos - 1);
        moved += 1;
    }
    if pos == start {
        accessible_start
    } else {
        text.bytepos_to_charpos(pos)
    }
}

/// Start of the line `lines` lines below `from`, clamped to `accessible_end`.
///
/// This is GNU's `move_it_vertically (&it, amount_to_scroll)` applied to the
/// old window start (xdisp.c:19526): the window start moves *down* by the
/// scroll amount, so the rows that stay on screen keep their text.
pub(crate) fn line_start_below(
    from: i64,
    lines: i64,
    accessible_end: i64,
    text: &(impl LineText + ?Sized),
) -> i64 {
    if lines <= 0 || from >= accessible_end {
        return from;
    }
    let n = usize::try_from(lines).unwrap_or(usize::MAX);
    let (after, crossed) = text.after_nth_newline(
        text.charpos_to_bytepos(from),
        text.charpos_to_bytepos(accessible_end),
        n,
    );
    if crossed == n {
        text.bytepos_to_charpos(after)
    } else {
        accessible_end
    }
}

/// Unibyte ASCII test text: character and byte positions coincide.
#[cfg(test)]
impl LineText for &[u8] {
    fn charpos_to_bytepos(&self, charpos: i64) -> i64 {
        charpos.clamp(0, self.len() as i64)
    }

    fn bytepos_to_charpos(&self, bytepos: i64) -> i64 {
        bytepos
    }

    fn after_nth_newline(&self, from: i64, to: i64, n: usize) -> (i64, usize) {
        let bytes = self.get(from as usize..to as usize).unwrap_or_default();
        let newlines = bytes.iter().enumerate().filter(|&(_, &byte)| byte == b'\n');
        newlines
            .take(n)
            .fold((from, 0), |(_, crossed), (index, _)| {
                (from + index as i64 + 1, crossed + 1)
            })
    }

    fn last_newline_before(&self, from: i64, to: i64) -> Option<i64> {
        let bytes = self.get(from as usize..to as usize)?;
        bytes
            .iter()
            .rposition(|&byte| byte == b'\n')
            .map(|index| from + index as i64)
    }
}

#[cfg(test)]
#[path = "scroll_policy/tests/scroll_policy_test.rs"]
mod tests;
