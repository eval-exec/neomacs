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
