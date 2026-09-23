use super::*;

/// The invariant the whole module exists for: the text GNU's boundary
/// emits, and specifically NOT Rust's rendering of the same failure.
#[test]
fn strerror_text_is_gnus_without_rusts_os_error_suffix() {
    let text = emacs_strerror(libc::ENOENT);
    assert_eq!(text, "No such file or directory");
    assert!(
        !text.contains("os error"),
        "must not carry Rust's \"(os error N)\" suffix: {text}"
    );
}

/// The regression shape: a Rust `io::Error` rendered through `Errno` must
/// agree with `strerror`, while rendering it directly does not.
#[test]
fn errno_from_io_matches_strerror_where_rust_display_does_not() {
    let io = std::io::Error::from_raw_os_error(libc::ENOENT);
    assert_ne!(
        io.to_string(),
        emacs_strerror(libc::ENOENT),
        "precondition: Rust's Display must differ, or this test proves nothing"
    );
    assert_eq!(Errno::from_io(&io).message(), emacs_strerror(libc::ENOENT));
    assert_eq!(
        Errno::new(libc::ENOENT).to_string(),
        "No such file or directory"
    );
}
