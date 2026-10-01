use super::super::scenario::PackageTuiPair;
use super::harness::*;
use std::time::Duration;

pub(super) fn run_visual_body(pair: &mut PackageTuiPair) -> (String, String) {
    let mut gnu = Vec::new();
    let mut neo = Vec::new();

    both(pair, "setup", |session| {
        invoke(session, "mwim358-tui-setup", "MWIM-VISUAL-SETUP")
    })
    .expect("set up both real MWIM visual sessions");
    record_rows(pair, "MWIM-VISUAL-SETUP", &mut gnu, &mut neo);
    gnu.push(visual_grid(&pair.gnu));
    neo.push(visual_grid(&pair.neo));

    for epoch in 1..=3 {
        let marker = format!("MWIM-MOVE e={epoch} ");
        both(
            pair,
            &format!("visual beginning epoch {epoch}"),
            |session| {
                session.send_keys("C-a");
                wait_for(session, Duration::from_secs(8), &marker, |grid| {
                    grid.iter().any(|row| row.contains(&marker))
                });
            },
        )
        .expect("drive both public visual beginning keys");
        record_rows(pair, &marker, &mut gnu, &mut neo);
    }

    both(pair, "reset middle", |session| {
        invoke(
            session,
            "mwim358-tui-reset-middle",
            "MWIM-VISUAL-RESET-MIDDLE",
        )
    })
    .expect("reset both peers to the middle visual row");
    record_rows(pair, "MWIM-VISUAL-RESET-MIDDLE", &mut gnu, &mut neo);

    for epoch in 4..=6 {
        let marker = format!("MWIM-MOVE e={epoch} ");
        both(pair, &format!("visual end epoch {epoch}"), |session| {
            session.send_keys("C-e");
            wait_for(session, Duration::from_secs(8), &marker, |grid| {
                grid.iter().any(|row| row.contains(&marker))
            });
        })
        .expect("drive both public visual end keys");
        record_rows(pair, &marker, &mut gnu, &mut neo);
    }

    both(pair, "reset final before beginning", |session| {
        invoke(
            session,
            "mwim358-tui-reset-final",
            "MWIM-VISUAL-RESET-FINAL",
        )
    })
    .expect("reset both peers to the final visual row");
    record_rows(pair, "MWIM-VISUAL-RESET-FINAL", &mut gnu, &mut neo);
    both(pair, "final visual beginning", |session| {
        session.send_keys("C-a");
        wait_for(session, Duration::from_secs(8), "visual epoch 7", |grid| {
            grid.iter().any(|row| row.contains("MWIM-MOVE e=7 "))
        });
    })
    .expect("drive both final-row beginning keys");
    record_rows(pair, "MWIM-MOVE e=7 ", &mut gnu, &mut neo);

    both(pair, "reset final before end", |session| {
        invoke(
            session,
            "mwim358-tui-reset-final",
            "MWIM-VISUAL-RESET-FINAL",
        )
    })
    .expect("reset both peers before the final-row end key");
    both(pair, "final visual end", |session| {
        session.send_keys("C-e");
        wait_for(session, Duration::from_secs(8), "visual epoch 8", |grid| {
            grid.iter().any(|row| row.contains("MWIM-MOVE e=8 "))
        });
    })
    .expect("drive both final-row end keys");
    record_rows(pair, "MWIM-MOVE e=8 ", &mut gnu, &mut neo);

    both(pair, "select wide visual movers", |session| {
        invoke(
            session,
            "mwim358-tui-use-wide-visual",
            "MWIM-WIDE-VISUAL-RESET p=",
        )
    })
    .expect("select visual movers at the tab and wide-character origin");
    record_rows(pair, "MWIM-WIDE-VISUAL-RESET p=", &mut gnu, &mut neo);
    both(pair, "wide visual beginning", |session| {
        session.send_keys("C-a");
        wait_for(
            session,
            Duration::from_secs(8),
            "movement epoch 9",
            |grid| grid.iter().any(|row| row.contains("MWIM-MOVE e=9 ")),
        );
    })
    .expect("drive public visual beginning on tab and wide text");
    record_rows(pair, "MWIM-MOVE e=9 ", &mut gnu, &mut neo);
    both(pair, "reset wide visual before end", |session| {
        invoke(
            session,
            "mwim358-tui-reset-wide-visual",
            "MWIM-WIDE-VISUAL-RESET-AGAIN",
        )
    })
    .expect("reset the tab and wide-character visual origin");
    record_rows(pair, "MWIM-WIDE-VISUAL-RESET-AGAIN", &mut gnu, &mut neo);
    both(pair, "wide visual end", |session| {
        session.send_keys("C-e");
        wait_for(
            session,
            Duration::from_secs(8),
            "movement epoch 10",
            |grid| grid.iter().any(|row| row.contains("MWIM-MOVE e=10 ")),
        );
    })
    .expect("drive public visual end on tab and wide text");
    record_rows(pair, "MWIM-MOVE e=10 ", &mut gnu, &mut neo);

    both(pair, "select logical movers", |session| {
        invoke(session, "mwim358-tui-use-logical", "MWIM-LOGICAL-RESET p=")
    })
    .expect("select real logical movers in both displayed buffers");
    record_rows(pair, "MWIM-LOGICAL-RESET p=", &mut gnu, &mut neo);

    both(pair, "logical beginning", |session| {
        session.send_keys("C-a");
        wait_for(
            session,
            Duration::from_secs(8),
            "movement epoch 11",
            |grid| grid.iter().any(|row| row.contains("MWIM-MOVE e=11 ")),
        );
    })
    .expect("drive public logical beginning from the identical wide origin");
    record_rows(pair, "MWIM-MOVE e=11 ", &mut gnu, &mut neo);

    both(pair, "reset logical before end", |session| {
        invoke(
            session,
            "mwim358-tui-reset-logical",
            "MWIM-LOGICAL-RESET-AGAIN",
        )
    })
    .expect("reset both peers before logical end keys");
    record_rows(pair, "MWIM-LOGICAL-RESET-AGAIN", &mut gnu, &mut neo);
    both(pair, "logical end", |session| {
        session.send_keys("C-e");
        wait_for(
            session,
            Duration::from_secs(8),
            "movement epoch 12",
            |grid| grid.iter().any(|row| row.contains("MWIM-MOVE e=12 ")),
        );
    })
    .expect("drive public logical end from the identical wide origin");
    record_rows(pair, "MWIM-MOVE e=12 ", &mut gnu, &mut neo);

    (gnu.join("\n"), neo.join("\n"))
}

pub(super) fn run_visual_cleanup(pair: &mut PackageTuiPair) -> (String, String) {
    both(pair, "cleanup", |session| {
        invoke(session, "mwim358-tui-cleanup", "MWIM-VISUAL-CLEAN")
    })
    .expect("clean both real MWIM visual sessions");
    (
        exact_row(&pair.gnu, "MWIM-VISUAL-CLEAN"),
        exact_row(&pair.neo, "MWIM-VISUAL-CLEAN"),
    )
}
