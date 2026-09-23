use super::*;

#[test]
fn compositor_width_reflows_measured_text_preserving_character_positions() {
    let request = TooltipRequest {
        text: "WiiW\n界".into(),
        ..Default::default()
    };
    let mut layout =
        TooltipLayout::measure_with(
            &request,
            8.0,
            16.0,
            |ch, _| {
                if ch == 'i' { 4.0 } else { 12.0 }
            },
        );
    layout.fit_surface(22.0, 64.0);
    assert_eq!(
        layout.lines.iter().map(|l| l.start).collect::<Vec<_>>(),
        [0, 2, 5]
    );
    assert_eq!(
        layout.lines.iter().map(|l| l.width).collect::<Vec<_>>(),
        [16.0, 16.0, 12.0]
    );
    assert_eq!(layout.lines[1].characters[0].x, 0.0);
    assert_eq!(layout.lines[1].characters[1].x, 4.0);
    assert_eq!(layout.extent(), (22.0, 64.0));
}

#[test]
fn wrapping_uses_measured_advances_not_character_count() {
    let request = TooltipRequest {
        text: "WiiW".into(),
        max_size: neomacs_display_protocol::tooltip::TooltipLimits::new(2, 4),
        ..Default::default()
    };
    let layout =
        TooltipLayout::measure_with(
            &request,
            8.0,
            16.0,
            |ch, _| {
                if ch == 'W' { 12.0 } else { 4.0 }
            },
        );
    assert_eq!(layout.lines.len(), 2);
    assert_eq!(layout.lines[0].width, 16.0);
    assert_eq!(layout.lines[1].width, 16.0);
    assert_eq!(layout.lines[0].characters[1].x, 12.0);
    assert_eq!(layout.lines[1].characters[1].x, 4.0);
}

#[test]
fn width_limit_wraps_instead_of_discarding_the_rest_of_the_line() {
    let request = TooltipRequest {
        text: "abcdef\n界λ".into(),
        max_size: neomacs_display_protocol::tooltip::TooltipLimits::new(3, 4),
        ..Default::default()
    };
    let layout = TooltipLayout::measure(&request, 8.0, 16.0);
    assert_eq!(
        layout
            .lines
            .iter()
            .map(|line| line
                .characters
                .iter()
                .map(|ch| ch.value)
                .collect::<String>())
            .collect::<Vec<_>>(),
        ["abc", "def", "界λ"]
    );
    assert_eq!(
        layout
            .lines
            .iter()
            .map(|line| line.start)
            .collect::<Vec<_>>(),
        [0, 3, 7]
    );
    assert_eq!(layout.extent(), (30.0, 54.0));
}

#[test]
fn tooltip_measurement_has_no_editor_window_clamp() {
    let request = TooltipRequest {
        text: "abcdefghijklmnopqrst".into(),
        ..Default::default()
    };
    let layout = TooltipLayout::measure(&request, 8.0, 16.0);
    assert_eq!(layout.extent(), (166.0, 22.0));
    assert_eq!((layout.bounds.0, layout.bounds.1), (0.0, 0.0));
}
