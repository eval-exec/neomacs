use super::{alternate_editor_tokens, fail_or_alternate, parse_options};
use std::ffi::OsString;

/// Every expectation below is GNU's own output: the tokenizer is
/// `fail()`'s loop (`lib-src/emacsclient.c:750-768`), and the quoting cases
/// marked *(probed)* were confirmed against the real 31.1 client.
#[test]
fn tokenization_matches_gnu_source_not_shell_syntax() {
    for (input, expected) in [
        ("  editor  fixed  ", vec!["editor", "fixed"]),
        (
            "\"editor with space\" \"fixed with space\"",
            vec!["editor with space", "fixed with space"],
        ),
        (
            "editor 'a b' a\\ b $HOME ; *.txt",
            vec!["editor", "'a", "b'", "a\\", "b", "$HOME", ";", "*.txt"],
        ),
        ("editor\targ next\narg", vec!["editor\targ", "next\narg"]),
        ("\"a b\"tail \"a\"\"b\"", vec!["a b", "tail", "a", "b"]),
        (
            "editor \"unterminated value",
            vec!["editor", "unterminated value"],
        ),
        // Probed GNU: a quote-run produces no empty token (`strspn` skips it
        // together with the spaces around it).
        ("editor \"\" next", vec!["editor", "next"]),
        ("/bin/echo A \"\" B", vec!["/bin/echo", "A", "B"]),
        // Probed GNU: an opening quote runs to the next quote *or the end*.
        ("/bin/echo \"\"foo bar", vec!["/bin/echo", "foo bar"]),
        ("/bin/echo \"a\"\"b\"", vec!["/bin/echo", "a", "b"]),
        ("   ", vec![]),
    ] {
        assert_eq!(alternate_editor_tokens(input), expected, "{input:?}");
    }
}

/// GNU `execvp`s the alternate editor and only prints
/// `error executing alternate editor "%s"` when that fails
/// (`emacsclient.c:772-777`); the exit status is then the editor's own, not a
/// collapsed 1.  This pins the failure path, which is the only one a test can
/// observe without replacing itself with the editor.
#[test]
fn a_failed_exec_reports_gnu_message() {
    let options = parse_options(
        "client",
        ["-a", "/nonexistent/neomacs-test-editor", "FILE"].map(OsString::from),
    )
    .unwrap();
    let error = fail_or_alternate("client", &options, "can't connect")
        .expect_err("a nonexistent alternate editor cannot exec");
    assert!(
        error.contains("error executing alternate editor \"/nonexistent/neomacs-test-editor\""),
        "{error}"
    );
}

/// The empty alternate (`-a ''`) is the automatic-daemon-start marker and
/// never reaches the tokenizer.
#[test]
fn an_empty_alternate_is_the_auto_start_marker() {
    let options = parse_options("client", ["-a", "", "FILE"].map(OsString::from)).unwrap();
    let error = fail_or_alternate("client", &options, "can't connect").unwrap_err();
    assert!(error.contains("automatic daemon startup"), "{error}");
}
