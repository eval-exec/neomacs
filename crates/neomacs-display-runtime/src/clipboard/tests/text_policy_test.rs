//! The text policy's decision table, pinned.
//!
//! Every row here is behaviour the backends must agree on: the MIME preference
//! and decode rules mirror smithay-clipboard 0.7.3 (`mime.rs::find_allowed`,
//! `state.rs` post-processing), and the classifiers are the only place its and
//! arboard's native errors become clipboard state.

use super::*;
use std::cell::Cell;
use std::io;

fn offered(mimes: &[&str]) -> Vec<String> {
    mimes.iter().map(|mime| (*mime).to_owned()).collect()
}

fn no_offer() -> io::Result<String> {
    Err(io::Error::other("selection is empty"))
}

fn no_text_mime() -> io::Result<String> {
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "supported mime-type is not found",
    ))
}

#[test]
fn utf8_types_win_in_offer_order_and_plain_text_is_a_fallback() {
    assert_eq!(
        TextMime::choose(&offered(&[
            "text/plain",
            "UTF8_STRING",
            "text/plain;charset=utf-8"
        ])),
        Some(TextMime::Utf8String)
    );
    assert_eq!(
        TextMime::choose(&offered(&["TEXT", "text/plain;charset=utf-8"])),
        Some(TextMime::TextPlainUtf8)
    );
    assert_eq!(
        TextMime::choose(&offered(&["image/png", "text/plain"])),
        Some(TextMime::TextPlain)
    );
}

#[test]
fn offers_without_a_text_type_have_no_text() {
    assert_eq!(TextMime::choose(&offered(&[])), None);
    assert_eq!(
        TextMime::choose(&offered(&[
            "image/png",
            "text/plain;charset=iso-8859-1",
            "STRING"
        ])),
        None
    );
}

#[test]
fn plain_text_line_ends_are_normalized_like_smithay_clipboard() {
    let bytes = b"a\r\nb\rc\n";
    assert_eq!(TextMime::TextPlainUtf8.decode(bytes), "a\nb\nc\n");
    assert_eq!(TextMime::TextPlain.decode(bytes), "a\nb\nc\n");
    assert_eq!(TextMime::Utf8String.decode(bytes), "a\r\nb\rc\n");
}

#[test]
fn invalid_utf8_is_decoded_lossily() {
    assert_eq!(TextMime::Utf8String.decode(b"ok\xff"), "ok\u{fffd}");
}

#[test]
fn smithay_absences_keep_their_two_reasons() {
    assert_eq!(
        classify_smithay(&no_offer().unwrap_err()),
        Some(TextAbsence::NoSelection)
    );
    assert_eq!(
        classify_smithay(&no_text_mime().unwrap_err()),
        Some(TextAbsence::TargetUnavailable)
    );
}

#[test]
fn smithay_failures_are_not_absences() {
    for message in [
        "client doesn't have focus",
        "no events received on any seat",
        "active seat lost",
        "clipboard is dead.",
    ] {
        assert_eq!(
            classify_smithay(&io::Error::other(message)),
            None,
            "{message} is a failure, not an absence"
        );
    }
}

#[test]
fn arboard_content_not_available_is_an_indeterminate_absence() {
    // arboard reports empty and unreadable-format with one error, so the
    // policy must not claim to know which one it was.
    assert_eq!(
        classify_arboard(&arboard::Error::ContentNotAvailable),
        Some(TextAbsence::Indeterminate)
    );
    for err in [
        arboard::Error::ClipboardNotSupported,
        arboard::Error::ClipboardOccupied,
        arboard::Error::ConversionFailure,
    ] {
        assert_eq!(classify_arboard(&err), None, "{err:?} is a failure");
    }
}

#[test]
fn smithay_text_is_returned_without_consulting_data_control() {
    let consulted = Cell::new(false);
    let result = text_or_fallback(Ok("native".to_owned()), || {
        consulted.set(true);
        Ok(Some(TextRead::Text("data-control".to_owned())))
    });
    assert_eq!(result, Ok(TextRead::Text("native".to_owned())));
    assert!(!consulted.get());
}

#[test]
fn missing_selection_offer_reads_through_data_control() {
    // Hyprland delivers the selection only to winit's data device, so
    // smithay-clipboard's device never holds an offer.
    let result = text_or_fallback(no_offer(), || {
        Ok(Some(TextRead::Text("foreign".to_owned())))
    });
    assert_eq!(result, Ok(TextRead::Text("foreign".to_owned())));
}

#[test]
fn missing_text_mime_reads_through_data_control() {
    let result = text_or_fallback(no_text_mime(), || {
        Ok(Some(TextRead::Text("text from data-control".to_owned())))
    });
    assert_eq!(
        result,
        Ok(TextRead::Text("text from data-control".to_owned()))
    );
}

#[test]
fn empty_transfer_is_text_not_a_missing_selection() {
    let result = text_or_fallback(Ok(String::new()), || {
        panic!("an empty transfer must not consult data-control")
    });
    assert_eq!(result, Ok(TextRead::Text(String::new())));
}

#[test]
fn seat_and_focus_errors_are_not_masked_by_data_control() {
    for message in [
        "client doesn't have focus",
        "no events received on any seat",
    ] {
        let result = text_or_fallback(Err(io::Error::other(message)), || {
            panic!("{message} must not consult data-control")
        });
        assert_eq!(result, Err(message.to_owned()));
    }
}

#[test]
fn a_failed_fallback_keeps_smithays_absence() {
    let result = text_or_fallback(no_offer(), || {
        Err("data-control transfer timed out".to_owned())
    });
    assert_eq!(result, Ok(TextRead::NoSelection));
}

#[test]
fn a_compositor_without_data_control_keeps_smithays_absence() {
    let result = text_or_fallback(no_text_mime(), || Ok(None));
    assert_eq!(result, Ok(TextRead::TargetUnavailable));
}

#[test]
fn a_clipboard_absence_may_be_answered_by_data_control() {
    let consulted = Cell::new(false);
    let result = wayland_read(ClipboardSelection::Clipboard, no_offer(), || {
        consulted.set(true);
        Ok(Some(TextRead::Text("foreign".to_owned())))
    });
    assert!(consulted.get(), "CLIPBOARD must consult the fallback");
    assert_eq!(result, Ok(TextRead::Text("foreign".to_owned())));
}

#[test]
fn a_primary_absence_never_consults_data_control() {
    let result = wayland_read(ClipboardSelection::Primary, no_offer(), || {
        panic!("PRIMARY must not consult the data-control fallback")
    });
    assert_eq!(result, Ok(TextRead::NoSelection));
}

#[test]
fn primary_text_is_returned_without_consulting_data_control() {
    let result = wayland_read(
        ClipboardSelection::Primary,
        Ok("selected".to_owned()),
        || panic!("PRIMARY text must not consult the data-control fallback"),
    );
    assert_eq!(result, Ok(TextRead::Text("selected".to_owned())));
}

#[test]
fn only_text_reaches_the_evaluator_facing_wire() {
    assert_eq!(
        TextRead::Text("x".to_owned()).into_option(),
        Some("x".to_owned())
    );
    assert_eq!(
        TextRead::Text(String::new()).into_option(),
        Some(String::new())
    );
    assert_eq!(TextRead::NoSelection.into_option(), None);
    assert_eq!(TextRead::TargetUnavailable.into_option(), None);
    assert_eq!(TextRead::Indeterminate.into_option(), None);
}

#[test]
fn text_mime_spellings_round_trip_exactly() {
    for (mime, spelling) in [
        (TextMime::TextPlainUtf8, "text/plain;charset=utf-8"),
        (TextMime::Utf8String, "UTF8_STRING"),
        (TextMime::TextPlain, "text/plain"),
    ] {
        assert_eq!(mime.as_str(), spelling);
        assert_eq!(spelling.parse::<TextMime>(), Ok(mime));
        assert_eq!(TextMime::choose(&offered(&[spelling])), Some(mime));
    }
}

#[test]
fn text_mime_recognition_does_not_accept_aliases_or_normalize_names() {
    for spelling in [
        "TextPlainUtf8",
        "Utf8String",
        "TextPlain",
        "utf8_string",
        "TEXT/PLAIN",
        "text/plain;charset=UTF-8",
        " text/plain",
        "text/plain ",
        "text/plain; charset=utf-8",
    ] {
        assert!(spelling.parse::<TextMime>().is_err(), "{spelling}");
        assert_eq!(TextMime::choose(&offered(&[spelling])), None, "{spelling}");
    }
}
