#![forbid(unsafe_code)]
#![cfg(any(target_os = "linux", target_os = "macos"))]

use neomacs_terminfo::{Database, Padding, Query, expand_numeric};
use std::io::{self, Write};
use std::time::{Duration, Instant};

#[test]
fn native_padding_uses_each_device_and_restores_state_after_failure() {
    if std::env::var_os("NEOMACS_PADDING_CHILD").is_none() {
        let directory = tempfile::tempdir().unwrap();
        let status = std::process::Command::new("tic")
            .args(["-x", "-o"])
            .arg(directory.path())
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/padding.src"
            ))
            .status()
            .expect("native tic is required");
        assert!(status.success());
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native_padding_uses_each_device_and_restores_state_after_failure",
                "--nocapture",
            ])
            .env("NEOMACS_PADDING_CHILD", "1")
            .env("TERMINFO", directory.path())
            .env_remove("NCURSES_NO_PADDING")
            .env_remove("BAUDRATE")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let master =
        rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
            .unwrap();
    rustix::pty::grantpt(&master).unwrap();
    rustix::pty::unlockpt(&master).unwrap();
    let name = rustix::pty::ptsname(&master, Vec::new()).unwrap();
    let slave = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(name.to_str().unwrap())
        .unwrap();
    let mut termios = rustix::termios::tcgetattr(&slave).unwrap();
    termios.set_speed(9600).unwrap();
    rustix::termios::tcsetattr(&slave, rustix::termios::OptionalActions::Now, &termios).unwrap();

    let pads = Padding::new("neo-padding", &slave).unwrap();
    let flow = Padding::new("neo-padding-flow", &slave).unwrap();
    let high_threshold = Padding::new("neo-padding-high", &slave).unwrap();
    let sleep = Padding::new("neo-padding-sleep", &slave).unwrap();
    let snapshot = Database::load("neo-padding", &[Query::TermcapNumber("Co")]).unwrap();
    expand_numeric(b"%{42}%PA", [0; 9]).unwrap();
    let mut bytes = Vec::new();
    pads.write(&mut bytes, b"A$<10>B", 1).unwrap();
    // ncurses uses nine bits per padding character (BAUDBYTE), hence ten
    // padding bytes at 9600 baud for 10ms.
    assert_eq!(bytes, b"A..........B");
    bytes.clear();
    pads.write(&mut bytes, b"A$<10.5*>B", 3).unwrap();
    assert_eq!(bytes, [b"A".as_slice(), &vec![b'.'; 33], b"B"].concat());
    bytes.clear();
    flow.write(&mut bytes, b"A$<10>B", 1).unwrap();
    // Standalone ncurses tputs differs by version/build: its SP wrapper may
    // force optional padding even with xon/pb. GNU uses this same native API.
    assert!(bytes == b"AB" || bytes == b"A..........B");
    bytes.clear();
    high_threshold.write(&mut bytes, b"C$<10>D", 1).unwrap();
    assert!(bytes == b"CD" || bytes == b"C..........D");
    bytes.clear();
    flow.write(&mut bytes, b"A$<10/>B", 1).unwrap();
    assert_eq!(bytes, b"A..........B");
    let alternate = Padding::new("neo-padding-other", &slave).unwrap();
    let mut alternate_bytes = Vec::new();
    alternate
        .write(&mut alternate_bytes, b"A$<10/>B", 1)
        .unwrap();
    assert_eq!(alternate_bytes, b"A++++++++++B");
    let mut original_bytes = Vec::new();
    pads.write(&mut original_bytes, b"A$<10/>B", 1).unwrap();
    assert_eq!(original_bytes, b"A..........B");
    assert_eq!(snapshot.termcap_number("Co"), Some(8));
    assert_eq!(expand_numeric(b"%gA%d", [0; 9]).unwrap(), b"42");

    struct Timed {
        start: Instant,
        bytes: Vec<u8>,
        before: Option<Duration>,
        after: Option<Duration>,
        flushes: usize,
    }
    impl Write for Timed {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.contains(&b'A') {
                self.before = Some(self.start.elapsed());
            }
            if bytes.contains(&b'B') {
                self.after = Some(self.start.elapsed());
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }
    let mut timed = Timed {
        start: Instant::now(),
        bytes: Vec::new(),
        before: None,
        after: None,
        flushes: 0,
    };
    sleep.write(&mut timed, b"A$<20/>B", 1).unwrap();
    assert_eq!(timed.bytes, b"AB");
    assert!(timed.after.unwrap() - timed.before.unwrap() >= Duration::from_millis(18));
    assert!(timed.flushes >= 2);

    struct Fail;
    impl Write for Fail {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "fixture"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert_eq!(
        pads.write(&mut Fail, b"A$<1>B", 1).unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
    for sequence in [b"A$<999999999999>B".as_slice(), b"A$<999999**>B", b"A\0B"] {
        assert!(pads.write(&mut Vec::new(), sequence, 10_000).is_err());
    }
    struct Panics;
    impl Write for Panics {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            panic!("fixture writer panic")
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(std::panic::catch_unwind(|| pads.write(&mut Panics, b"A$<1>B", 1)).is_err());
    let mut recovered = Vec::new();
    pads.write(&mut recovered, b"A$<10>B", 1).unwrap();
    assert_eq!(recovered, b"A..........B");
    assert_eq!(expand_numeric(b"%gA%d", [0; 9]).unwrap(), b"42");
}
