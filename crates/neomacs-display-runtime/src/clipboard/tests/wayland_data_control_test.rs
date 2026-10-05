use super::*;

fn offered(mimes: &[&str]) -> Vec<String> {
    mimes.iter().map(|mime| (*mime).to_owned()).collect()
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
fn transfer_reads_until_the_owner_closes_its_end() {
    use std::io::Write;
    let (mut reader, mut writer) = std::io::pipe().unwrap();
    let owner = std::thread::spawn(move || {
        writer.write_all(b"first ").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        writer.write_all(b"second").unwrap();
    });
    let bytes = read_to_end_before(&mut reader, Instant::now() + Duration::from_secs(5)).unwrap();
    owner.join().unwrap();
    assert_eq!(bytes, b"first second");
}

#[test]
fn stalled_selection_owner_times_out() {
    let (mut reader, writer) = std::io::pipe().unwrap();
    let started = Instant::now();
    let result = read_to_end_before(&mut reader, started + Duration::from_millis(50));
    assert_eq!(result, Err("data-control transfer timed out".to_owned()));
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(writer);
}
