use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::time::Duration;

use serde_json::json;

use super::{
    AsciicastV3Writer, CastEvent, CastHeader, RecordingIdentity, RecordingPolicy, SessionRecording,
    TerminalSize, TuiRecordingScope,
};

#[test]
fn asciicast_v3_serializes_the_complete_typed_terminal_event_stream() {
    let mut writer = AsciicastV3Writer::new(
        Vec::new(),
        CastHeader::new(
            TerminalSize::new(24, 80),
            "screen-256color",
            "search::incremental_search [GNU]",
        ),
    )
    .expect("write cast header");

    writer
        .write_event(
            Duration::from_millis(125),
            CastEvent::Output("\u{1b}[31mhello\u{1b}[0m".to_owned()),
        )
        .expect("write output event");
    writer
        .write_event(
            Duration::from_millis(20),
            CastEvent::Input("C-s".to_owned()),
        )
        .expect("write input event");
    writer
        .write_event(
            Duration::from_millis(5),
            CastEvent::Resize(TerminalSize::new(50, 160)),
        )
        .expect("write resize event");
    writer
        .write_event(
            Duration::ZERO,
            CastEvent::Marker("incremental search visible".to_owned()),
        )
        .expect("write marker event");
    writer
        .write_event(Duration::from_millis(1), CastEvent::Exit(0))
        .expect("write exit event");

    let output = String::from_utf8(writer.into_inner()).expect("cast is UTF-8");
    let lines = output
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid NDJSON line"))
        .collect::<Vec<serde_json::Value>>();

    assert_eq!(
        lines,
        vec![
            json!({
                "version": 3,
                "term": {"cols": 80, "rows": 24, "type": "screen-256color"},
                "title": "search::incremental_search [GNU]"
            }),
            json!([0.125, "o", "\u{1b}[31mhello\u{1b}[0m"]),
            json!([0.020, "i", "C-s"]),
            json!([0.005, "r", "160x50"]),
            json!([0.0, "m", "incremental search visible"]),
            json!([0.001, "x", "0"]),
        ]
    );
}

#[test]
fn recording_on_writes_a_replayable_cast_at_the_named_artifact_path() {
    let artifacts = tempfile::tempdir().expect("create artifact directory");
    let mut recording = SessionRecording::start(
        RecordingPolicy::On,
        artifacts.path(),
        TuiRecordingScope::new("neomacs-tui-tests", "search::incremental search/roundtrip")
            .session("GNU"),
        "screen-256color",
        TerminalSize::new(24, 80),
    );
    let path = recording
        .path()
        .expect("enabled recording has a path")
        .to_path_buf();
    let initial_cast = fs::read_to_string(&path).expect("header is durable immediately");
    assert_eq!(initial_cast.lines().count(), 1);

    // A terminal read may split one UTF-8 scalar across arbitrary PTY
    // chunks. The diagnostic projection must not corrupt it.
    if let SessionRecording::On(active) = &mut recording {
        active.last_flush -= Duration::from_secs(2);
    }
    recording.output(&[0xe7]);
    recording.output(&[0x95, 0x8c, b'!']);
    recording.flush_if_due();
    assert!(
        fs::read_to_string(&path)
            .expect("periodic output checkpoint")
            .contains("界!")
    );
    recording.input(b"\x1b[A");
    recording.resize(TerminalSize::new(50, 160));
    recording.marker("screen settled");
    recording.finish(0);

    assert_eq!(
        path,
        artifacts
            .path()
            .join("neomacs-tui-tests")
            .join("search-incremental-search-roundtrip")
            .join("gnu.cast")
    );
    let lines = fs::read_to_string(path)
        .expect("read cast")
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid cast line"))
        .collect::<Vec<serde_json::Value>>();
    assert_eq!(
        lines[0],
        json!({
            "version": 3,
            "term": {"cols": 80, "rows": 24, "type": "screen-256color"},
            "title": "search::incremental search/roundtrip [GNU]"
        })
    );
    assert_eq!(lines[1][1], "o");
    assert_eq!(lines[1][2], "界!");
    assert_eq!(lines[2][1], "i");
    assert_eq!(lines[2][2], "\u{1b}[A");
    assert_eq!(lines[3][1], "r");
    assert_eq!(lines[3][2], "160x50");
    assert_eq!(lines[4][1], "m");
    assert_eq!(lines[4][2], "screen settled");
    assert_eq!(lines[5][1], "x");
    assert_eq!(lines[5][2], "0");
}

#[test]
fn recording_off_creates_no_artifact_and_environment_values_are_closed() {
    let artifacts = tempfile::tempdir().expect("create artifact directory");
    let mut recording = SessionRecording::start(
        RecordingPolicy::Off,
        artifacts.path(),
        RecordingIdentity::new("neomacs-tui-tests", "editing", "NEO"),
        "screen-256color",
        TerminalSize::new(24, 80),
    );

    recording.output(b"ignored");
    recording.finish(0);

    assert_eq!(recording.path(), None);
    assert!(
        fs::read_dir(artifacts.path())
            .expect("read artifact root")
            .next()
            .is_none()
    );
    assert_eq!(RecordingPolicy::default(), RecordingPolicy::Off);
    assert_eq!(RecordingPolicy::parse(None), Ok(RecordingPolicy::Off));
    assert_eq!(
        RecordingPolicy::parse(Some(OsStr::new(""))),
        Ok(RecordingPolicy::Off)
    );
    assert_eq!(
        RecordingPolicy::parse(Some(OsStr::new("on"))),
        Ok(RecordingPolicy::On)
    );
    assert_eq!(
        RecordingPolicy::parse(Some(OsStr::new("off"))),
        Ok(RecordingPolicy::Off)
    );
    assert!(RecordingPolicy::parse(Some(OsStr::new("sometimes"))).is_err());
    assert!(RecordingPolicy::parse(Some(OsStr::from_bytes(b"on\xff"))).is_err());
}

#[test]
fn recording_creation_failure_degrades_to_off() {
    let root_file = tempfile::NamedTempFile::new().expect("create non-directory root");
    let mut recording = SessionRecording::start(
        RecordingPolicy::On,
        root_file.path(),
        RecordingIdentity::new("neomacs-tui-tests", "creation failure", "NEO"),
        "screen-256color",
        TerminalSize::new(24, 80),
    );

    recording.output(b"the parity path still runs");
    recording.finish(0);

    assert_eq!(recording.path(), None);
}
