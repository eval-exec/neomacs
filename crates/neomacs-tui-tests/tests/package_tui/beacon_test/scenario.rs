use super::super::scenario::PackageTuiPair;
use super::harness::*;
use expect_test::expect;
use std::time::Duration;

pub(super) fn run_body(
    pair: &mut PackageTuiPair,
    mismatches: &mut Vec<String>,
) -> Result<(), String> {
    both(pair, "setup", |session| {
        invoke(session, "beacon359-tui-setup", "B359-SETUP")
    })?;
    record_pair(
        pair,
        "setup",
        "B359-SETUP",
        expect!["B359-SETUP cells=16777216 class=static-color defaults=t lighter=t focus=nil"],
        mismatches,
    );
    both(pair, "source provenance", |session| {
        invoke(session, "beacon359-tui-show-source", "B359-SOURCE")
    })?;
    record_pair(
        pair,
        "source provenance",
        "B359-SOURCE",
        expect![[r#"B359-SOURCE subject=beacon.el seq=t compile=compile.elc suffix=(".el")"#]],
        mismatches,
    );

    both(pair, "manual blink", |session| {
        invoke(session, "beacon359-tui-manual", "B359-MANUAL")
    })?;
    record_pair(
        pair,
        "manual",
        "B359-MANUAL",
        expect!["B359-MANUAL p=8 l=1 s=1 b=source o=7 t=t w=t n=1"],
        mismatches,
    );
    record_style(
        pair,
        "manual",
        "manual alpha",
        7,
        7,
        expect![[r#"
            [0;48;2;0;255;255ma[0;48;2;28;252;252ml[0;48;2;57;249;249mp[0;48;2;86;246;246mh[0;48;2;114;242;242ma[0;48;2;143;239;239m [0;48;2;172;236;236mb[0m
        "#]],
        mismatches,
    );

    both(pair, "EOL blink", |session| {
        invoke(session, "beacon359-tui-eol", "B359-EOL")
    })?;
    record_pair(
        pair,
        "EOL",
        "B359-EOL",
        expect!["B359-EOL p=41 l=2 s=1 b=source o=1 t=t w=t n=1"],
        mismatches,
    );
    record_style(
        pair,
        "EOL",
        "manual-eol",
        10,
        7,
        expect![[r#"
            [0;48;2;0;255;255m [0;48;2;28;252;252m [0;48;2;57;249;249m [0;48;2;86;246;246m [0;48;2;114;242;242m [0;48;2;143;239;239m [0;48;2;172;236;236m [0m
        "#]],
        mismatches,
    );

    both(pair, "natural timer start", |session| {
        invoke(session, "beacon359-tui-natural", "B359-NATURAL-START")
    })?;
    let _ = wait_for_pair_background(pair, "manual alpha", 7, 7, true);
    let _ = wait_for_pair_background(pair, "manual alpha", 7, 7, false);
    for session in [&mut pair.gnu, &mut pair.neo] {
        wait_for(
            session,
            Duration::from_secs(3),
            "natural timer terminal state",
            |grid| grid.iter().any(|row| row.contains("B359-NATURAL-DONE")),
        );
    }
    record_pair(
        pair,
        "natural timer",
        "B359-NATURAL-DONE",
        expect!["B359-NATURAL-DONE ovs=0 listed=nil timerp=t"],
        mismatches,
    );

    both(pair, "scroll setup", |session| {
        invoke(session, "beacon359-tui-prepare-scroll", "B359-SCROLL-READY")
    })?;
    both(pair, "real C-v", |session| {
        session.send_keys("C-v");
        wait_for(
            session,
            Duration::from_secs(8),
            "scroll redisplay observation",
            |grid| grid.iter().any(|row| row.contains("B359-SCROLL-AFTER ")),
        );
    })?;
    record_pair(
        pair,
        "scroll after redisplay",
        "B359-SCROLL-AFTER ",
        expect!["B359-SCROLL-AFTER p=761 l=20 s=20 b=scroll o=7 t=t w=t n=1"],
        mismatches,
    );
    record_style(
        pair,
        "scroll",
        "row 19 |",
        0,
        7,
        expect![[r#"
        [0;48;2;0;255;255mr[0;48;2;28;252;252mo[0;48;2;57;249;249mw[0;48;2;86;246;246m [0;48;2;114;242;242m1[0;48;2;143;239;239m9[0;48;2;172;236;236m [0m
    "#]],
        mismatches,
    );

    both(pair, "window setup", |session| {
        invoke(
            session,
            "beacon359-tui-prepare-windows",
            "B359-WINDOW-READY",
        )
    })?;
    both(pair, "real C-x o", |session| {
        session.send_keys("C-x o");
        wait_for(session, Duration::from_secs(8), "B359-WINDOW", |grid| {
            grid.iter().any(|row| row.contains("B359-WINDOW "))
        });
    })?;
    record_pair(
        pair,
        "window change",
        "B359-WINDOW ",
        expect!["B359-WINDOW p=1 l=1 s=1 b=source o=7 t=t w=t n=1"],
        mismatches,
    );
    let gnu_window = repeated_styled_spans(&pair.gnu, "manual alpha", 7);
    let neo_window = repeated_styled_spans(&pair.neo, "manual alpha", 7);
    if neo_window != gnu_window {
        mismatches.push(format!(
            "window-scoped rendering differs\nGNU:\n{gnu_window}\nNeo:\n{neo_window}"
        ));
    }
    expect![[r#"
        B359-WINDOW-CELLS-1 manual [0m
        B359-WINDOW-CELLS-2 [0;48;2;0;255;255mm[0;48;2;28;252;252ma[0;48;2;57;249;249mn[0;48;2;86;246;246mu[0;48;2;114;242;242ma[0;48;2;143;239;239ml[0;48;2;172;236;236m [0m"#]].assert_eq(&gnu_window);

    both(pair, "real buffer switch", |session| {
        session.send_keys("C-x b");
        wait_for(
            session,
            Duration::from_secs(8),
            "switch-to-buffer prompt",
            |grid| grid.iter().any(|row| row.contains("Switch to buffer")),
        );
        session.send_keys("C-q");
        session.send(b" ");
        session.send(b"*beacon359-other*");
        session.send_keys("RET");
        wait_for(session, Duration::from_secs(8), "B359-BUFFER", |grid| {
            grid.iter().any(|row| row.contains("B359-BUFFER "))
        });
    })?;
    record_pair(
        pair,
        "buffer switch",
        "B359-BUFFER ",
        expect!["B359-BUFFER p=37 l=2 s=1 b=other o=1 t=t w=t n=1"],
        mismatches,
    );

    both(pair, "next-line setup", |session| {
        invoke(session, "beacon359-tui-prepare-next", "B359-NEXT-READY")
    })?;
    record_pair(
        pair,
        "default command suppression prepared",
        "B359-NEXT-READY",
        expect!["B359-NEXT-READY p=861 l=24 s=4"],
        mismatches,
    );
    both(pair, "suppressed next line", |session| {
        session.send_keys("C-n");
        wait_for(
            session,
            Duration::from_secs(8),
            "suppressed next-line redisplay observation",
            |grid| grid.iter().any(|row| row.contains("B359-NEXT-AFTER ")),
        );
    })?;
    record_pair(
        pair,
        "default command suppression after redisplay",
        "B359-NEXT-AFTER ",
        expect!["B359-NEXT-AFTER p=901 l=25 before=4 after=15 delta=11 o=0 t=nil n=0"],
        mismatches,
    );

    let mut gnu_suppression = Vec::new();
    let mut neo_suppression = Vec::new();
    for (label, command) in [
        ("predicate", "beacon359-tui-configure-suppression"),
        ("major", "beacon359-tui-configure-major"),
        ("command", "beacon359-tui-configure-command"),
        ("local", "beacon359-tui-configure-local"),
        ("compilation", "beacon359-tui-configure-compilation"),
    ] {
        both(pair, label, |session| {
            invoke(session, command, "B359-SUPPRESS-READY")
        })?;
        push_rows(
            pair,
            "B359-SUPPRESS-READY",
            &mut gnu_suppression,
            &mut neo_suppression,
        );
        both(pair, &format!("{label} suppressed jump"), |session| {
            session.send_keys("C-c j");
            wait_for(session, Duration::from_secs(8), "B359-JUMP", |grid| {
                grid.iter().any(|row| row.contains("B359-JUMP "))
            });
        })?;
        push_rows(
            pair,
            "B359-JUMP ",
            &mut gnu_suppression,
            &mut neo_suppression,
        );
        both(pair, &format!("{label} recovery setup"), |session| {
            invoke(session, "beacon359-tui-recover", "B359-RECOVER-READY")
        })?;
        push_rows(
            pair,
            "B359-RECOVER-READY",
            &mut gnu_suppression,
            &mut neo_suppression,
        );
        both(pair, &format!("{label} recovery jump"), |session| {
            session.send_keys("C-c j");
            wait_for(session, Duration::from_secs(8), "B359-JUMP", |grid| {
                grid.iter().any(|row| row.contains("B359-JUMP "))
            });
        })?;
        push_rows(
            pair,
            "B359-JUMP ",
            &mut gnu_suppression,
            &mut neo_suppression,
        );
    }
    let gnu_suppression = gnu_suppression.join("\n");
    let neo_suppression = neo_suppression.join("\n");
    if neo_suppression != gnu_suppression {
        mismatches.push(format!(
            "suppression matrix differs\nGNU:\n{gnu_suppression}\nNeo:\n{neo_suppression}"
        ));
    }
    expect![[r#"
        B359-SUPPRESS-READY kind=predicate
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=0 t=nil n=0
        B359-RECOVER-READY mode=t local=nil
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=7 t=t n=1
        B359-SUPPRESS-READY kind=major
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=0 t=nil n=0
        B359-RECOVER-READY mode=t local=nil
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=7 t=t n=1
        B359-SUPPRESS-READY kind=command
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=0 t=nil n=0
        B359-RECOVER-READY mode=t local=nil
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=7 t=t n=1
        B359-SUPPRESS-READY kind=local mode=nil global-hook=1
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=0 t=nil n=0
        B359-RECOVER-READY mode=t local=nil
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=7 t=t n=1
        B359-SUPPRESS-READY kind=compilation mode=compilation-mode defaults=t
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=0 t=nil n=0
        B359-RECOVER-READY mode=t local=nil
        B359-JUMP p=141 l=6 m=nil a=nil r=nil o=7 t=t n=1"#]]
    .assert_eq(&gnu_suppression);

    both(pair, "mark setup", |session| {
        invoke(session, "beacon359-tui-prepare-mark", "B359-MARK-READY")
    })?;
    both(pair, "mark jump", |session| {
        session.send_keys("C-c j");
        wait_for(session, Duration::from_secs(8), "mark jump", |grid| {
            grid.iter().any(|row| row.contains("B359-JUMP "))
        });
    })?;
    record_pair(
        pair,
        "mark push",
        "B359-JUMP ",
        expect!["B359-JUMP p=141 l=6 m=1 a=nil r=(3) o=0 t=nil n=0"],
        mismatches,
    );

    both(pair, "mark/blink setup", |session| {
        invoke(
            session,
            "beacon359-tui-prepare-mark-blink",
            "B359-MARK-READY",
        )
    })?;
    both(pair, "mark/blink jump", |session| {
        session.send_keys("C-c j");
        wait_for(session, Duration::from_secs(8), "mark/blink jump", |grid| {
            grid.iter().any(|row| row.contains("B359-JUMP "))
        });
    })?;
    record_pair(
        pair,
        "mark/blink applied",
        "B359-JUMP ",
        expect!["B359-JUMP p=141 l=6 m=nil a=nil r=nil o=7 t=t n=1"],
        mismatches,
    );
    record_style(
        pair,
        "mark/blink applied",
        "row 02 |",
        0,
        7,
        expect![[r#"
            [0;48;2;255;0;255mr[0;48;2;252;28;252mo[0;48;2;249;57;249mw[0;48;2;246;86;246m [0;48;2;242;114;242m0[0;48;2;239;143;239m2[0;48;2;236;172;236m [0m
        "#]],
        mismatches,
    );

    both(pair, "active mark setup", |session| {
        invoke(
            session,
            "beacon359-tui-prepare-active-mark",
            "B359-MARK-READY",
        )
    })?;
    both(pair, "active mark jump", |session| {
        session.send_keys("C-c j");
        wait_for(
            session,
            Duration::from_secs(8),
            "active mark jump",
            |grid| grid.iter().any(|row| row.contains("B359-JUMP ")),
        );
    })?;
    record_pair(
        pair,
        "active mark preserved",
        "B359-JUMP ",
        expect!["B359-JUMP p=141 l=6 m=3 a=t r=nil o=0 t=nil n=0"],
        mismatches,
    );

    let mut gnu_horizontal = Vec::new();
    let mut neo_horizontal = Vec::new();
    for (label, command) in [
        ("horizontal alone", "beacon359-tui-prepare-horizontal-alone"),
        (
            "horizontal coupled",
            "beacon359-tui-prepare-horizontal-coupled",
        ),
    ] {
        both(pair, label, |session| {
            invoke(session, command, "B359-HORIZONTAL-READY")
        })?;
        push_rows(
            pair,
            "B359-HORIZONTAL-READY",
            &mut gnu_horizontal,
            &mut neo_horizontal,
        );
        both(pair, label, |session| {
            session.send_keys("C-c h");
            wait_for(session, Duration::from_secs(8), "B359-HORIZONTAL", |grid| {
                grid.iter().any(|row| row.contains("B359-HORIZONTAL "))
            });
        })?;
        push_rows(
            pair,
            "B359-HORIZONTAL ",
            &mut gnu_horizontal,
            &mut neo_horizontal,
        );
    }
    let gnu_horizontal = gnu_horizontal.join("\n");
    let neo_horizontal = neo_horizontal.join("\n");
    if neo_horizontal != gnu_horizontal {
        mismatches.push(format!(
            "horizontal coupling differs\nGNU:\n{gnu_horizontal}\nNeo:\n{neo_horizontal}"
        ));
    }
    expect![[r#"
        B359-HORIZONTAL-READY vertical=nil p=42 col=0
        B359-HORIZONTAL p=46 l=3 col=12 v=nil h=5 o=0 t=nil n=0
        B359-HORIZONTAL-READY vertical=1 p=42 col=0
        B359-HORIZONTAL p=46 l=3 col=12 v=1 h=5 o=2 t=t n=1"#]]
    .assert_eq(&gnu_horizontal);

    both(pair, "focus setup", |session| {
        invoke(session, "beacon359-tui-arm-focus", "B359-FOCUS-READY")
    })?;
    both(pair, "focus in input", |session| session.send(b"\x1b[I"))?;
    wait_for_pair_marker(pair, "B359-FOCUS-APPLIED n=1");
    record_pair(
        pair,
        "focus in state",
        "B359-FOCUS-APPLIED n=1",
        expect!["B359-FOCUS-APPLIED n=1 s=t r=1-2/2-3/3-4 b=t f=t w=t t=t"],
        mismatches,
    );
    let (gnu_focus_in, neo_focus_in) = wait_for_pair_background(pair, "manual alpha", 0, 3, true)
        .expect("focus-in must capture a real styled frame in both peers");
    record_captured_style(
        "focus in",
        gnu_focus_in,
        neo_focus_in,
        expect![[r#"
            [0;48;2;255;0;0mm[0;48;2;249;57;57ma[0;48;2;242;114;114mn[0m
        "#]],
        mismatches,
    );
    let _ = wait_for_pair_background(pair, "manual alpha", 0, 3, false);
    pair.gnu.clear_recent_output();
    pair.neo.clear_recent_output();
    both(pair, "focus out input", |session| session.send(b"\x1b[O"))?;
    wait_for_pair_marker(pair, "B359-FOCUS-APPLIED n=2");
    record_pair(
        pair,
        "focus out state",
        "B359-FOCUS-APPLIED n=2",
        expect!["B359-FOCUS-APPLIED n=2 s=nil r=1-2/2-3/3-4 b=t f=t w=t t=t"],
        mismatches,
    );
    let gnu_focus_out = emitted_styled_span(&pair.gnu, "man");
    let neo_focus_out = emitted_styled_span(&pair.neo, "man");
    record_captured_style(
        "focus out",
        gnu_focus_out,
        neo_focus_out,
        expect![[r#"
            [0;48;2;255;0;0mm[0;48;2;249;57;57ma[0;48;2;242;114;114mn[0m
        "#]],
        mismatches,
    );
    both(pair, "focus report", |session| {
        invoke(session, "beacon359-tui-focus-report", "B359-FOCUS-REPORT")
    })?;
    record_pair(
        pair,
        "focus",
        "B359-FOCUS-REPORT",
        expect!["B359-FOCUS-REPORT n=2 state=nil timerp=t listed=nil"],
        mismatches,
    );
    Ok(())
}

pub(super) fn run_cleanup(
    pair: &mut PackageTuiPair,
    mismatches: &mut Vec<String>,
) -> Result<(), String> {
    let mut errors = Vec::new();
    if let Err(error) = both(pair, "real focus restore", |session| {
        session.send(b"\x1b[O");
        session.read(Duration::from_millis(300));
    }) {
        errors.push(error);
    }
    if let Err(error) = both(pair, "cleanup", |session| {
        invoke(session, "beacon359-tui-cleanup", "B359-CLEAN")
    }) {
        errors.push(error);
    } else {
        record_pair(
            pair,
            "cleanup",
            "B359-CLEAN",
            expect!["B359-CLEAN ok=t errors=nil resources=nil windows=t variables=t"],
            mismatches,
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}
