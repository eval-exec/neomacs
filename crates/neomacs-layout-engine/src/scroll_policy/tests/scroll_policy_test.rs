use super::*;

/// 16 single-letter lines: line N is the letter at charpos `2*(N-1)`, its
/// newline at `2*(N-1)+1`. Charpos 26 is line 14 ("n").
const LINES16: &[u8] = b"a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\np\n";

fn byte_at(text: &'static [u8]) -> impl Fn(i64) -> Option<u8> {
    move |charpos| text.get(charpos as usize).copied()
}

#[test]
fn resolve_follows_gnu_precedence() {
    assert_eq!(ScrollPolicy::resolve(0, 0), ScrollPolicy::Recenter);
    assert_eq!(
        ScrollPolicy::resolve(0, 5),
        ScrollPolicy::Step { lines: 5 },
        "scroll-step applies only when scroll-conservatively is 0"
    );
    assert_eq!(
        ScrollPolicy::resolve(10, 5),
        ScrollPolicy::Conservative { max_lines: 10 },
        "scroll-conservatively wins over scroll-step"
    );
    assert_eq!(
        ScrollPolicy::resolve(SCROLL_CONSERVATIVELY_LIMIT, 0),
        ScrollPolicy::Conservative { max_lines: 100 },
        "SCROLL_LIMIT itself still recenters on a far jump"
    );
    assert_eq!(
        ScrollPolicy::resolve(SCROLL_CONSERVATIVELY_LIMIT + 1, 0),
        ScrollPolicy::Unlimited
    );
}

#[test]
fn conservative_scrolls_by_dy_within_its_limit() {
    let policy = ScrollPolicy::Conservative { max_lines: 10 };

    assert_eq!(
        policy.forward_scroll(1, true, 21, 0),
        ForwardScroll::Advance { lines: 1 },
        "stepping one line off the bottom scrolls exactly one line"
    );
    assert_eq!(
        policy.forward_scroll(10, true, 21, 0),
        ForwardScroll::Advance { lines: 10 }
    );
}

#[test]
fn conservative_recenters_beyond_its_limit() {
    let policy = ScrollPolicy::Conservative { max_lines: 10 };

    // GNU `dy > scroll_max` -> SCROLLING_FAILED -> `recenter:` with
    // centering_position = window_box_height / 2.
    assert_eq!(
        policy.forward_scroll(11, false, 21, 0),
        ForwardScroll::Recenter {
            lines_above_point: 10
        }
    );
}

#[test]
fn default_policy_always_recenters_to_the_middle() {
    // GNU's defaults never enter try_scrolling at all, so even a one-line step
    // off the bottom goes to `recenter:`.
    assert_eq!(
        ScrollPolicy::Recenter.forward_scroll(1, true, 21, 0),
        ForwardScroll::Recenter {
            lines_above_point: 10
        }
    );
}

#[test]
fn unlimited_puts_point_on_the_last_row_instead_of_centering() {
    // scroll-conservatively > 100 disables centering (xdisp.c:21150): when even
    // the 100-line search fails, point lands on the last usable row.
    assert_eq!(
        ScrollPolicy::Unlimited.forward_scroll(5, true, 21, 0),
        ForwardScroll::Advance { lines: 5 }
    );
    assert_eq!(
        ScrollPolicy::Unlimited.forward_scroll(500, false, 21, 0),
        ForwardScroll::Recenter {
            lines_above_point: 20
        }
    );
}

#[test]
fn step_scrolls_a_fixed_amount_not_dy() {
    let policy = ScrollPolicy::Step { lines: 4 };

    assert_eq!(
        policy.forward_scroll(1, true, 21, 0),
        ForwardScroll::Advance { lines: 4 },
        "scroll-step scrolls by its own amount, not by the distance to point"
    );
    assert_eq!(
        policy.forward_scroll(5, true, 21, 0),
        ForwardScroll::Recenter {
            lines_above_point: 10
        },
        "a jump past scroll-step lines fails and recenters"
    );
}

#[test]
fn scroll_margin_lifts_the_bottom_usable_row() {
    assert_eq!(last_usable_row(21, 0), 20);
    assert_eq!(last_usable_row(21, 3), 17);
    // GNU caps the margin at a quarter of the window (window.c:5117), so the
    // top and bottom margins can never meet.
    assert_eq!(last_usable_row(21, 50), 15);
    assert_eq!(last_usable_row(1, 4), 0);
    assert_eq!(last_usable_row(0, 0), 0);
}

#[test]
fn line_start_above_lands_on_a_line_beginning() {
    // Point at charpos 26 (line 14). Two lines above is line 12, which starts
    // at charpos 22 -- NOT charpos 21, the newline that ends line 11. Starting
    // on the newline would render an empty leading row and cost a line of text.
    assert_eq!(line_start_above(26, 2, 0, &byte_at(LINES16)), 22);
    assert_eq!(line_start_above(26, 0, 0, &byte_at(LINES16)), 26);
    assert_eq!(
        line_start_above(26, 99, 0, &byte_at(LINES16)),
        0,
        "clamps to the accessible start"
    );
}

#[test]
fn line_start_above_from_mid_line_uses_the_line_point_is_on() {
    // "hello\nworld\n": point at charpos 8 sits inside "world"; one line above
    // is the start of "hello" (charpos 0), not 8 - 1 lines of characters.
    const TEXT: &[u8] = b"hello\nworld\n";
    assert_eq!(line_start_above(8, 0, 0, &byte_at(TEXT)), 6);
    assert_eq!(line_start_above(8, 1, 0, &byte_at(TEXT)), 0);
}

#[test]
fn line_start_below_advances_whole_lines() {
    // From the buffer start, one line down is line 2 (charpos 2).
    assert_eq!(
        line_start_below(0, 1, LINES16.len() as i64, &byte_at(LINES16)),
        2
    );
    assert_eq!(
        line_start_below(0, 5, LINES16.len() as i64, &byte_at(LINES16)),
        10
    );
    assert_eq!(
        line_start_below(0, 0, LINES16.len() as i64, &byte_at(LINES16)),
        0
    );
    assert_eq!(
        line_start_below(0, 99, LINES16.len() as i64, &byte_at(LINES16)),
        LINES16.len() as i64,
        "clamps to the accessible end"
    );
}

#[test]
fn count_lines_bounded_reports_when_it_gave_up() {
    assert_eq!(count_lines_bounded(0, 8, 10, &byte_at(LINES16)), (4, true));
    let (lines, bounded) = count_lines_bounded(0, LINES16.len() as i64, 3, &byte_at(LINES16));
    assert!(!bounded, "scan stopped at the limit");
    assert_eq!(lines, 4, "reports the limit it tripped, not the true total");
}

#[test]
fn backward_minimal_scroll_puts_point_on_the_top_margin_row() {
    // GNU's backward branch scrolls just enough: point ends on the first row
    // the top `scroll-margin` allows, and the text below it stays put.
    for policy in [
        ScrollPolicy::Conservative { max_lines: 20 },
        ScrollPolicy::Unlimited,
    ] {
        assert_eq!(policy.backward_scroll(1, true, 21, 0), 0);
        assert_eq!(policy.backward_scroll(1, true, 21, 3), 3);
    }
}

#[test]
fn backward_recenters_when_the_jump_is_too_far() {
    // Conservative gives up past its limit and centres point (xdisp.c:21188)…
    assert_eq!(
        ScrollPolicy::Conservative { max_lines: 10 }.backward_scroll(50, false, 21, 0),
        10
    );
    // …but scroll-conservatively > 100 never centres: point stays at the top
    // margin even when the minimal scroll failed (xdisp.c:21183).
    assert_eq!(
        ScrollPolicy::Unlimited.backward_scroll(500, false, 21, 0),
        0
    );
}

#[test]
fn backward_default_policy_centers_point() {
    assert_eq!(ScrollPolicy::Recenter.backward_scroll(1, true, 21, 0), 10);
}

#[test]
fn backward_step_moves_the_start_by_scroll_step_not_to_point() {
    // GNU scrolls the start back by exactly `scroll-step` lines, so point ends
    // up `step - lines_back` rows below the new start rather than on top of it.
    let policy = ScrollPolicy::Step { lines: 3 };
    assert_eq!(policy.backward_scroll(1, true, 21, 0), 2);
    assert_eq!(policy.backward_scroll(3, true, 21, 0), 0);
}
