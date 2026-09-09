//! Regression comparisons against ncurses tput, used by GNU through tparm.
#![forbid(unsafe_code)]
#![cfg(any(target_os = "linux", target_os = "macos"))]

use neomacs_terminfo::expand_numeric;
use std::process::Command;

fn compare(format: &str) {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("audit.src");
    std::fs::write(
        &src,
        format!("neo-audit|numeric parity audit,\n\tcolors#256, setaf={format},\n"),
    )
    .unwrap();
    let compiled = Command::new("tic")
        .args(["-x", "-o"])
        .arg(dir.path())
        .arg(src)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let oracle = Command::new("tput")
        .args(["-T", "neo-audit", "setaf", "7"])
        .env("TERMINFO", dir.path())
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let actual = expand_numeric(format.as_bytes(), [7, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(actual, Ok(oracle.stdout), "format: {format}");
}

#[test]
fn literal_divisor_control() {
    compare("%p1%{4}%/%d");
}
#[test]
fn computed_divisor() {
    compare("%p1%{2}%{2}%+%/%d");
}
#[test]
fn character_divisor() {
    compare("%p1%'A'%/%d");
}
#[test]
fn variable_divisor() {
    compare("%{4}%Pa%p1%ga%/%d");
}
#[test]
fn long_but_shallow_program() {
    compare(&format!("{}%p1%{{2}}%/%d", "%p1%d".repeat(20)));
}
#[test]
fn branch_constants_and_remainder() {
    for format in [
        "%?%p1%t%{4}%Pa%e%{4}%Pa%;%p1%ga%m%d",
        "%p1%?%p1%t%{4}%e%{4}%;%/%d",
        "%?%p1%t%p1%{2}%/%d%e%p1%{4}%/%d%;",
        "%?%p1%{0}%<%t%{4}%e%p1%{8}%<%t%{4}%e%{4}%;%Pa%p1%ga%/%d",
        "%p1%{2}%{4}%-%/%d",
        "%{7}%{0}%{1}%-%/%d",
        "%p1%{2}%{2}%-%/%d",
        "%p1%{2147483647}%{1}%&%/%d",
        "%?%p1%t%?%p1%t%{4}%e%{4}%;%e%{4}%;%Pa%p1%ga%/%d",
    ] {
        compare(format);
    }
}

#[test]
fn parameter_dependent_and_wrapping_divisors() {
    for format in [
        "%p1%p1%/%d",
        "%p1%{2147483647}%{2}%*%{1}%+%/%d",
        "%p1%?%p1%{8}%<%t%{2}%e%{4}%;%/%d",
        "%i%p1%p1%/%d",
        "%i%i%p1%p1%/%d",
        "%p1%{2}%Pa%ga%/%d",
        "%?%p1%t%{7}%e%{0}%{2147483647}%-%{1}%-%{0}%{1}%-%/%;%d%p1%{2}%/%d",
    ] {
        compare(format);
    }
    compare(&format!("{}%{{2}}%/%d", "%p1".repeat(20)));
}

#[test]
fn gnu_unmatched_terminator_control() {
    compare("%p1%d%;");
}

#[test]
#[ignore = "requires a populated host terminfo database"]
fn installed_rendering_capabilities_are_accepted() {
    use StringCapability::{Termcap, Terminfo};
    use neomacs_terminfo::{Database, Query, StringCapability};
    let capabilities = [
        Termcap("AF"),
        Termcap("AB"),
        Termcap("Sf"),
        Termcap("Sb"),
        Terminfo("Smulx"),
        Terminfo("setf24"),
        Terminfo("setb24"),
        Terminfo("setrgbf"),
        Terminfo("setrgbb"),
    ];
    let queries: Vec<_> = capabilities.iter().copied().map(Query::String).collect();
    let entries = Command::new("toe").arg("-a").output().unwrap();
    assert!(entries.status.success());
    let mut checked = 0;
    let mut unavailable = Vec::new();
    let mut unique = std::collections::BTreeSet::new();
    let mut rejected = Vec::new();
    for line in String::from_utf8(entries.stdout).unwrap().lines() {
        let Some(name) = line.split_whitespace().next() else {
            continue;
        };
        let db = match Database::load(name, &queries) {
            Ok(db) => db,
            Err(error) => {
                unavailable.push(format!("{name}: {error}"));
                continue;
            }
        };
        for capability in capabilities {
            let Some(format) = db.string(capability) else {
                continue;
            };
            checked += 1;
            if !unique.insert(format.to_vec()) {
                continue;
            }
            if let Err(error) = expand_numeric(format, [7, 17, 34, 0, 0, 0, 0, 0, 0]) {
                rejected.push(format!(
                    "{name} {capability:?}: {}: {error}",
                    String::from_utf8_lossy(format)
                ));
            }
        }
    }
    println!(
        "checked {checked} installed capability values, {} unique programs",
        unique.len()
    );
    println!(
        "{} entries could not be loaded: {}",
        unavailable.len(),
        unavailable.join("; ")
    );
    assert!(
        checked > 0,
        "host scan must not pass without checking capabilities"
    );
    assert!(
        rejected.is_empty(),
        "rejected installed formats:\n{}",
        rejected.join("\n")
    );
}
