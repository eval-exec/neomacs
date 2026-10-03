use super::*;
use std::collections::VecDeque;

/// A reader that replays a script of chunks and would-block errors, so the
/// reply policy is testable without a socket.  GNU's client arms a read
/// timeout for every reply (`emacsclient.c:2211`: `-w` or `DEFAULT_TIMEOUT`)
/// but only fails on it before the first server byte (`:2225-2247`); these
/// tests pin that distinction.
enum Step {
    Data(&'static [u8]),
    WouldBlock,
}

struct ScriptedReader {
    steps: VecDeque<Step>,
}

impl ScriptedReader {
    fn new(steps: impl IntoIterator<Item = Step>) -> Self {
        Self {
            steps: steps.into_iter().collect(),
        }
    }
}

impl Read for ScriptedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.steps.pop_front() {
            Some(Step::Data(bytes)) => {
                assert!(
                    bytes.len() <= buf.len(),
                    "a script chunk must fit one read buffer"
                );
                buf[..bytes.len()].copy_from_slice(bytes);
                Ok(bytes.len())
            }
            Some(Step::WouldBlock) => Err(io::Error::new(io::ErrorKind::WouldBlock, "timed out")),
            None => Ok(0),
        }
    }
}

fn read_script(
    steps: impl IntoIterator<Item = Step>,
    args: &[&str],
) -> (Result<(), String>, String, String) {
    let mut reader = ScriptedReader::new(steps);
    let options = parse_options("client", args.iter().copied().map(OsString::from)).unwrap();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let result = read_responses(&mut reader, &options, None, &mut out, &mut err);
    (
        result,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

#[test]
fn a_quiet_stream_after_the_first_response_keeps_waiting() {
    let (result, out, err) = read_script(
        [
            Step::Data(b"-print hello\n"),
            Step::WouldBlock,
            Step::WouldBlock,
        ],
        &["-w", "1"],
    );
    assert!(result.is_ok(), "{result:?} (stderr: {err})");
    assert_eq!(out, "hello");
}

#[test]
fn an_explicit_reply_timeout_gives_up_before_the_first_response() {
    let (result, _out, err) = read_script([Step::WouldBlock], &["-w", "2"]);
    let message = result.expect_err("a silent server must fail an explicit -w");
    assert!(
        message.contains("Server not responding; timed out after 2 seconds"),
        "{message}"
    );
    assert!(
        err.contains("timed out after 2 seconds"),
        "GNU's notice must reach stderr: {err:?}"
    );
}

#[test]
fn without_an_explicit_timeout_a_late_first_response_still_arrives() {
    let (result, out, err) = read_script([Step::WouldBlock, Step::Data(b"-print late\n")], &[]);
    assert!(result.is_ok(), "{result:?} (stderr: {err})");
    assert_eq!(out, "late");
}

#[test]
fn a_partial_line_survives_a_read_timeout() {
    let (result, out, _err) = read_script(
        [
            Step::Data(b"-print par"),
            Step::WouldBlock,
            Step::Data(b"tial\n"),
        ],
        &["-w", "5"],
    );
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(out, "partial");
}

#[test]
fn a_zero_reply_timeout_is_rejected_like_gnu() {
    // GNU `emacsclient.c:540-549`: "Invalid timeout: \"0\"" then exit 1.
    for args in [
        vec!["-w", "0"],
        vec!["--timeout=0"],
        vec!["--timeout", "-1"],
        vec!["-w", "not-a-number"],
    ] {
        let error = parse_options("client", args.clone().into_iter().map(OsString::from))
            .expect_err("a zero or unparsable timeout must be rejected");
        assert!(
            error.starts_with("Invalid timeout: \""),
            "{args:?}: {error}"
        );
    }
    // `--startup-timeout` is a Neomacs extension; zero stays "unlimited".
    let options = parse_options("client", ["--startup-timeout", "0"].map(OsString::from)).unwrap();
    assert_eq!(options.startup_timeout, None);
}

#[test]
fn ordinary_startup_is_unlimited_and_reply_budget_is_independent() {
    for args in [vec![], vec!["-w", "1"]] {
        let options = parse_options("client", args.into_iter().map(OsString::from)).unwrap();
        assert_eq!(options.startup_timeout, None);
    }
    let options = parse_options(
        "client",
        ["-w", "1", "--startup-timeout=3"].map(OsString::from),
    )
    .unwrap();
    assert_eq!(options.timeout, Some(Duration::from_secs(1)));
    assert_eq!(options.startup_timeout, Some(Duration::from_secs(3)));
    let options = parse_options(
        "client",
        ["-w", "1", "--startup-timeout", "0"].map(OsString::from),
    )
    .unwrap();
    assert_eq!(options.timeout, Some(Duration::from_secs(1)));
    assert_eq!(options.startup_timeout, None);
    for args in [
        vec!["--startup-timeout"],
        vec!["--startup-timeout=-1"],
        vec!["--startup-timeout=bad"],
    ] {
        assert!(parse_options("client", args.into_iter().map(OsString::from)).is_err());
    }
}
