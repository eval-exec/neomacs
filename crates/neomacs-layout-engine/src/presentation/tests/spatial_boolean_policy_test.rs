//! Common boolean ON spellings preserve the selected presentation policy.
use super::*;

#[test]
fn chrome_position_policy_accepts_common_boolean_on_values() {
    assert_eq!(
        parse_chrome_position_source(None),
        ChromePositionSource::Rows
    );
    for value in ["rows", "on", "1", "true", "yes", " TRUE ", "Yes"] {
        assert_eq!(
            parse_chrome_position_source(Some(value)),
            ChromePositionSource::Rows,
            "{value}"
        );
    }
    for value in ["frame", "off", "0", "false", "no", "unknown", ""] {
        assert_eq!(
            parse_chrome_position_source(Some(value)),
            ChromePositionSource::Frame,
            "{value}"
        );
    }
}

#[test]
fn text_position_policy_accepts_common_boolean_on_values() {
    assert_eq!(
        parse_presented_text_positions_mode(None),
        PresentedTextPositionsMode::Lazy
    );
    for value in ["lazy", "rows", "on", "1", "true", "yes", " TRUE ", "Yes"] {
        assert_eq!(
            parse_presented_text_positions_mode(Some(value)),
            PresentedTextPositionsMode::Lazy,
            "{value}"
        );
    }
    for value in ["eager", "off", "0", "false", "no", "unknown", ""] {
        assert_eq!(
            parse_presented_text_positions_mode(Some(value)),
            PresentedTextPositionsMode::Eager,
            "{value}"
        );
    }
}
