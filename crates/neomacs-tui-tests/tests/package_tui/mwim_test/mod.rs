use std::time::Duration;

use expect_test::{expect, expect_file};

use super::{CachedMelpaOracle, MWIM_MELPA_PIN};

use super::scenario::{PackageTuiScenario, PairTimeout, ReadinessCheckpoint};

mod harness;
mod prelude;
mod scenario;

use harness::*;
use prelude::*;
use scenario::*;

#[test]
fn mwim_real_visual_and_logical_line_keys_match_gnu() {
    let oracle = CachedMelpaOracle::new(MWIM_MELPA_PIN, "mwim.el")
        .expect("prepare exact shallow MWIM source below ./tmp")
        .with_prelude(MWIM_VISUAL_TUI_PRELUDE);
    let mut pair = PackageTuiScenario::new("mwim-visual-lines", oracle.prepared_packages())
        .spawn_when_ready(
            ReadinessCheckpoint::new(
                "initial scratch buffer",
                PairTimeout::per_editor(Duration::from_secs(20), Duration::from_secs(30)),
            ),
            |grid| grid.iter().any(|row| row.contains("*scratch*")),
        )
        .expect("spawn ready real MWIM visual PTY pair");

    let body = catch_phase("MWIM visual body", || run_visual_body(&mut pair));
    let cleanup = catch_phase("MWIM visual cleanup", || run_visual_cleanup(&mut pair));
    let mut errors = Vec::new();

    match body {
        Ok((gnu, neo)) => {
            if neo != gnu {
                errors.push(format!(
                    "MWIM visual behavior differs\nGNU:\n{gnu}\nNeo:\n{neo}"
                ));
            }
            expect_file!["snapshots/visual_and_logical_keys.txt"].assert_eq(&gnu);
        }
        Err(error) => errors.push(error),
    }
    match cleanup {
        Ok((gnu, neo)) => {
            if neo != gnu {
                errors.push(format!(
                    "MWIM visual cleanup differs\nGNU: {gnu}\nNeo: {neo}"
                ));
            }
            expect!["MWIM-VISUAL-CLEAN ok=t errors=nil resources=nil windows=t"].assert_eq(&gnu);
        }
        Err(error) => errors.push(error),
    }

    assert!(errors.is_empty(), "{}", errors.join("\n\n"));
}
