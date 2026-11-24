//! A caller-supplied base64 alphabet is part of the encoded `String`.

use neovm_value::strings_core::{B64_STD, B64_URL, base64_encode};

#[test]
fn base64_encode_refuses_a_non_ascii_alphabet() {
    let result = std::panic::catch_unwind(|| {
        // The old encoder built a `String` from these bytes. Forget it if
        // that still happens: this row only asks whether encoding was refused.
        std::mem::forget(base64_encode(b"abc", &[0xFF; 64], false, false));
    });
    assert!(
        result.is_err(),
        "accepted a base64 alphabet that is not UTF-8"
    );
}

#[test]
fn base64_encode_keeps_the_standard_and_url_tables() {
    assert_eq!(base64_encode(b"abc", B64_STD, false, false), "YWJj");
    assert_eq!(base64_encode(b"abc", B64_URL, false, false), "YWJj");
    assert_eq!(base64_encode(b"a", B64_STD, true, false), "YQ==");
}
