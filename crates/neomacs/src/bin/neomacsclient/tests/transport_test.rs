use super::test_support::{Step, read_script};
use super::*;

fn options(args: &[&str]) -> Options {
    parse_options("client", args.iter().copied().map(OsString::from)).unwrap()
}

/// GNU resolves the transport once, before writing any token
/// (`lib-src/emacsclient.c:656-661`): `-c` without an available display is a
/// tty request, so a display-less terminal gets a frame instead of the
/// server's "Please specify display" error.
#[test]
fn a_frame_request_without_a_display_is_a_tty_request() {
    let plain = options(&["FILE"]);
    assert_eq!(plain.frame_transport(true), FrameTransport::None);
    assert_eq!(plain.frame_transport(false), FrameTransport::None);

    let create = options(&["-c", "FILE"]);
    assert_eq!(create.frame_transport(true), FrameTransport::Graphical);
    assert_eq!(create.frame_transport(false), FrameTransport::Tty);

    let tty = options(&["-t", "FILE"]);
    assert_eq!(tty.frame_transport(true), FrameTransport::Tty);
    assert_eq!(tty.frame_transport(false), FrameTransport::Tty);

    let eval = options(&["-e", "(+ 1 2)"]);
    assert_eq!(eval.frame_transport(true), FrameTransport::None);
}

/// GNU hands the server this client's tty identity whenever
/// `create_frame || !eval` (`emacsclient.c:2104-2113`): a daemon with no other
/// frame may have to occupy this tty even for a plain file request.  An
/// eval-only request must not.
#[test]
fn the_tty_identity_is_offered_for_frames_and_file_requests() {
    assert!(options(&["FILE"]).offers_tty_identity());
    assert!(options(&["-c", "FILE"]).offers_tty_identity());
    assert!(options(&["-t", "FILE"]).offers_tty_identity());
    assert!(!options(&["-e", "(+ 1 2)"]).offers_tty_identity());
}

/// A graphical request keeps `-window-system`; the display-less `-c` must not
/// send it, because GNU resolves that request to a tty first
/// (`emacsclient.c:2131-2132` sends it only for `create_frame && !tty`).
#[test]
fn only_a_graphical_transport_asks_for_the_window_system() {
    assert_eq!(
        options(&["-c", "FILE"]).frame_transport(true),
        FrameTransport::Graphical
    );
    assert_ne!(
        options(&["-c", "FILE"]).frame_transport(false),
        FrameTransport::Graphical
    );
}

/// GNU answers `-window-system-unsupported` with `nowait = false; tty = true`
/// and re-sends the request on the connection the server kept open
/// (`emacsclient.c:2275-2295`).
#[test]
fn the_server_can_ask_for_a_retry_on_the_terminal() {
    let (outcome, _out, _err) = read_script(
        [Step::Data(b"-window-system-unsupported \n")],
        &["-c", "FILE"],
    );
    assert_eq!(outcome.unwrap(), ReplyOutcome::WindowSystemUnsupported);
}

fn tty_identity() -> TtyIdentity {
    TtyIdentity {
        device: "/dev/pts/9".to_string(),
        terminal_type: "xterm-256color".to_string(),
    }
}

#[test]
fn a_retry_plan_drops_nowait_and_asks_for_no_window_system() {
    let options = options(&["-n", "-c", "FILE"]);
    let attempt = resolve_attempt(&options, true, Some(FrameTransport::Tty));
    assert_eq!(attempt.transport, FrameTransport::Tty);
    assert!(!attempt.nowait, "GNU clears nowait on the retry");

    let plan = build_request(&options, &attempt, Some(":0"), Ok(tty_identity())).unwrap();
    assert!(plan.request.contains("-tty "), "{}", plan.request);
    assert!(!plan.request.contains("-nowait"), "{}", plan.request);
    assert!(!plan.request.contains("-window-system"), "{}", plan.request);
    assert!(plan.tty.is_some());
}

/// GNU `find_tty` aborts when a tty frame was requested and this process has
/// no terminal; for every other request the identity is simply omitted.
#[test]
fn a_missing_terminal_is_fatal_only_for_a_tty_request() {
    let tty_request = options(&["-t", "FILE"]);
    let attempt = resolve_attempt(&tty_request, false, None);
    assert_eq!(attempt.transport, FrameTransport::Tty);
    let error = build_request(&tty_request, &attempt, None, Err("no terminal".into()))
        .expect_err("a tty request without a terminal must fail");
    assert!(error.contains("no terminal"), "{error}");

    let file_request = options(&["FILE"]);
    let attempt = resolve_attempt(&file_request, true, None);
    let plan = build_request(
        &file_request,
        &attempt,
        Some(":0"),
        Err("no terminal".into()),
    )
    .expect("a file request goes without a tty identity");
    assert!(plan.tty.is_none());
    assert!(!plan.request.contains("-tty "), "{}", plan.request);
}
