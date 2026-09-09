use super::*;
use neomacs_display_protocol::menu::MenuItemId;

#[test]
fn repeated_motion_preserves_deadline_and_does_not_reshow_expired_help() {
    let mut help = HoverHelp::default();
    let now = Instant::now();
    let target = HelpTarget {
        panel: PanelId(1),
        item: MenuItemId(4),
    };
    let delay = Duration::from_millis(700);
    help.select(target, now, delay, Duration::ZERO, Duration::ZERO);
    help.select(
        target,
        now + Duration::from_millis(600),
        delay,
        Duration::ZERO,
        Duration::ZERO,
    );
    assert_eq!(help.take_due(now + delay), Some(target));
    help.select(
        target,
        now + Duration::from_secs(10),
        delay,
        Duration::ZERO,
        Duration::ZERO,
    );
    assert_eq!(help.take_due(now + Duration::from_secs(11)), None);
}

#[test]
fn cancellation_retires_old_target_and_recent_replacement_uses_short_delay() {
    let mut help = HoverHelp::default();
    let now = Instant::now();
    let old = HelpTarget {
        panel: PanelId(1),
        item: MenuItemId(4),
    };
    let new = HelpTarget {
        panel: PanelId(1),
        item: MenuItemId(5),
    };
    let delay = Duration::from_millis(700);
    let short = Duration::from_millis(100);
    help.select(old, now, delay, short, Duration::from_secs(1));
    help.cancel(true, now);
    assert_eq!(help.take_due(now + delay), None);
    help.select(new, now, delay, short, Duration::from_secs(1));
    assert_eq!(help.take_due(now + short), Some(new));
}
