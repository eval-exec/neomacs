#![forbid(unsafe_code)]
#![cfg(any(target_os = "linux", target_os = "macos"))]

use neomacs_terminfo::{Database, Error, FlagCapability, Query, StringCapability, expand_numeric};

fn params(values: &[i32]) -> [i32; 9] {
    let mut result = [0; 9];
    result[..values.len()].copy_from_slice(values);
    result
}

#[test]
fn numeric_formats_match_gnu_rendering_inputs() {
    for (format, values, expected) in [
        (
            b"\x1b[4:%p1%dm".as_slice(),
            vec![3],
            b"\x1b[4:3m".as_slice(),
        ),
        // GNU term.c deliberately includes an unmatched %; in its Tc literal.
        (
            b"\x1b[38;2;%p1%d;%p2%d;%p3%d%;m",
            vec![17, 34, 51],
            b"\x1b[38;2;17;34;51m",
        ),
        (
            b"%p1%{65536}%/%d;%p1%{256}%/%{255}%&%d;%p1%{255}%&%d",
            vec![0x123456],
            b"18;52;86",
        ),
        (b"%?%p1%{8}%<%t3%p1%d%e38;5;%p1%d%;", vec![123], b"38;5;123"),
        (b"%p1%!%d", vec![-1], b"0"),
        (b"%p1%p2%A%d", vec![-1, 1], b"1"),
        (b"%p1%{0}%/%d", vec![1], b"0"),
        (b"%p1%{256}%m%d", vec![257], b"1"),
        (b"%%{2147483648}", vec![], b"%{2147483648}"),
        (b"%p1%{1}%+%d", vec![i32::MAX], b"-2147483648"),
        (b"$%p1%d$<5>", vec![3], b"$3$<5>"),
        (b"%p9%d", vec![0, 0, 0, 0, 0, 0, 0, 0, 9], b"9"),
    ] {
        assert_eq!(
            expand_numeric(format, params(&values)).unwrap(),
            expected,
            "{format:?}"
        );
    }
}

#[test]
fn malformed_and_string_formats_cannot_reach_native_varargs() {
    for format in [
        b"%p1%s".as_slice(),
        b"%p1%p2%/%d",
        b"%p1%p2%m%d",
        b"%{0}%{2147483647}%-%{1}%-%{0}%{1}%-%/%d",
        b"%?%{0}%t%p1%s%;",
        b"%{2147483648}%/%d",
        b"%p1%:10.2s",
        b"%p1%l%d",
        b"%p0%d",
        b"%p",
        b"%{",
        b"%",
        b"a\0b",
        b"%p1%999999999999999999999999d",
    ] {
        assert_eq!(
            expand_numeric(format, params(&[1])),
            Err(Error::InvalidNumericFormat),
            "{format:?}"
        );
    }
}

#[test]
fn division_requires_room_for_the_literal_divisor() {
    let mut format = b"%p1".repeat(20);
    format.extend_from_slice(b"%{256}%/%d");
    assert_eq!(
        expand_numeric(&format, [0; 9]),
        Err(Error::InvalidNumericFormat)
    );
}

#[test]
fn native_variables_work_within_numeric_programs() {
    assert_eq!(
        expand_numeric(b"%p1%PA%gA%d", params(&[42])).unwrap(),
        b"42"
    );
}

#[test]
fn concurrent_expansion_preserves_owned_output() {
    std::thread::scope(|scope| {
        for number in 1..=8 {
            scope.spawn(move || {
                for _ in 0..200 {
                    assert_eq!(
                        expand_numeric(b"%p1%Pa%ga%d", params(&[number])).unwrap(),
                        number.to_string().as_bytes()
                    );
                }
            });
        }
    });
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn native_snapshots() {
    // Launch an isolated process instead of mutating TERMINFO while Rust test
    // threads run. Missing tic or malformed fixtures fail explicitly.
    if std::env::var_os("NEOMACS_TERMINFO_FIXTURE_CHILD").is_none() {
        let directory = tempfile::tempdir().unwrap();
        let tic = std::process::Command::new("tic")
            .args(["-x", "-o"])
            .arg(directory.path())
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/terminals.src"
            ))
            .output()
            .expect("ncurses tic is required for the fixture tests");
        assert!(
            tic.status.success(),
            "{}",
            String::from_utf8_lossy(&tic.stderr)
        );
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "native_snapshots", "--nocapture"])
            .env("NEOMACS_TERMINFO_FIXTURE_CHILD", "1")
            .env("TERMINFO", directory.path())
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
    use StringCapability::{Termcap, Terminfo};
    let mut queries = vec![
        Query::String(Termcap("md")),
        Query::String(Termcap("me")),
        Query::String(Terminfo("sgr0")),
        Query::String(Termcap("us")),
        Query::String(Terminfo("Smulx")),
        Query::String(Terminfo("smxx")),
        Query::String(Terminfo("colors")), // Wrong type: (char *) -1.
        Query::TermcapNumber("Co"),
        Query::TermcapNumber("NC"),
        Query::Flag(FlagCapability::Terminfo("RGB")),
        Query::Flag(FlagCapability::Terminfo("Tc")),
        Query::Flag(FlagCapability::Terminfo("colors")), // Wrong type: -1.
        Query::Flag(FlagCapability::Termcap("Su")),
    ];
    // Native reads total more than the old fixed 32KiB tgetstr arena.
    queries.extend(std::iter::repeat_n(Query::String(Termcap("md")), 2000));
    let first = Database::load("neo-pr363-a", &queries).unwrap();
    let second = Database::load("neo-pr363-b", &queries).unwrap();
    assert_eq!(
        first.string(Termcap("md")),
        Some(b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".as_slice())
    );
    assert_eq!(second.string(Termcap("md")), Some(b"BBBB".as_slice()));
    // ncurses removes the alternate-character-set reset for termcap me.
    assert_eq!(first.string(Termcap("me")), Some(b"\x1b[0m".as_slice()));
    assert_eq!(
        first.string(Terminfo("sgr0")),
        Some(b"\x1b[m\x0f".as_slice())
    );
    assert_eq!(first.termcap_number("Co"), Some(256));
    assert_eq!(second.termcap_number("Co"), Some(8));
    assert_eq!(
        first.string(Terminfo("Smulx")),
        Some(b"\x1b[4:%p1%dm".as_slice())
    );
    assert_eq!(second.string(Terminfo("Smulx")), None);
    assert_eq!(second.string(Termcap("us")), None);
    assert!(first.flag(FlagCapability::Terminfo("RGB")));
    assert!(first.flag(FlagCapability::Terminfo("Tc")));
    assert!(first.flag(FlagCapability::Termcap("Su")));
    assert!(!first.flag(FlagCapability::Terminfo("colors")));
    assert_eq!(first.string(Terminfo("colors")), None);
    assert!(matches!(
        Database::load("", &queries),
        Err(Error::InvalidName)
    ));
    assert!(matches!(
        Database::load("bad\0name", &queries),
        Err(Error::InvalidName)
    ));
    for _ in 0..30 {
        assert!(matches!(
            Database::load("neo-pr363-does-not-exist", &queries),
            Err(Error::TerminalNotFound)
        ));
        assert_eq!(
            Database::load("neo-pr363-a", &queries)
                .unwrap()
                .termcap_number("Co"),
            Some(256)
        );
    }
    std::thread::scope(|scope| {
        for (name, colors) in [("neo-pr363-a", 256), ("neo-pr363-b", 8)] {
            let queries = &queries;
            scope.spawn(move || {
                for _ in 0..20 {
                    let database = Database::load(name, queries).unwrap();
                    assert_eq!(database.termcap_number("Co"), Some(colors));
                }
            });
        }
    });
}
