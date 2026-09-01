use std::time::Duration;

use expect_test::expect;
use neomacs_tui_tests::RawTerminalSnapshot;

use crate::{COMPAT_GNU_ELPA_PIN, CORFU_MELPA_PIN, CachedMelpaOracle};

use super::support::PackageTuiPair;

const CORFU_TUI_PRELUDE: &str = r#"
(require 'corfu)
(require 'corfu-echo)

(defun corfu380-tui-capf ()
  (let ((end (point))
        (start (save-excursion (skip-chars-backward "[:word:]-") (point))))
    (list start end '("café" "camel" "carbide")
          :annotation-function
          (lambda (candidate) (concat "  kind:" (substring candidate 0 2)))
          :company-docsig
          (lambda (candidate) (format "Documentation for %s 界" candidate)))))

(setq completion-at-point-functions '(corfu380-tui-capf)
      corfu-preselect 'prompt
      corfu-preview-current nil
      corfu-echo-delay 0
      corfu-count 3)
(corfu-echo-mode 1)
(corfu-mode 1)
(erase-buffer)
(insert "ca")
(keymap-global-set "C-c c" #'completion-at-point)
"#;

fn candidate_rows(grid: &[String]) -> Vec<(u16, String)> {
    grid.iter()
        .enumerate()
        .filter_map(|(row, contents)| {
            let trimmed = contents.trim_start();
            ["café", "camel", "carbide"]
                .iter()
                .any(|candidate| trimmed.starts_with(candidate))
                .then(|| (row as u16, trimmed.trim_end().to_owned()))
        })
        .collect()
}

fn candidate_row_range(grid: &[String]) -> std::ops::Range<u16> {
    let rows = candidate_rows(grid)
        .into_iter()
        .map(|(row, _)| row)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 3, "Corfu must render exactly three candidates");
    assert_eq!(rows[1], rows[0] + 1, "candidate rows must be contiguous");
    assert_eq!(rows[2], rows[1] + 1, "candidate rows must be contiguous");
    rows[0]..rows[2] + 1
}

fn painted_ansi_rows(snapshot: RawTerminalSnapshot) -> String {
    snapshot
        .ansi_grid()
        .lines()
        .map(|row| {
            let row = row
                .strip_suffix("\x1b[0m")
                .expect("raw row projection must end with an ANSI reset");
            format!("{}\x1b[0m\n", row.trim_end_matches(' '))
        })
        .collect::<String>()
        .replace('\x1b', "<ESC>")
}

#[test]
fn corfu_real_tty_popup_candidates_navigation_and_insertion_match_gnu() {
    let oracle = CachedMelpaOracle::new(CORFU_MELPA_PIN, "corfu.el")
        .expect("prepare revision-pinned Corfu source")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare exact Compat dependency")
        .with_prelude(CORFU_TUI_PRELUDE);
    let mut pair = PackageTuiPair::spawn("corfu-popup", oracle.prepared_packages())
        .expect("spawn package TUI pair");

    for session in [&mut pair.gnu, &mut pair.neo] {
        session.read_until(Duration::from_secs(20), |grid| {
            grid.iter().any(|row| row.trim_start().starts_with("ca"))
        });
        session.send_keys("C-c c");
        session.read_until(Duration::from_secs(8), |grid| {
            candidate_rows(grid).len() == 3
        });
    }

    let gnu_candidates = candidate_rows(&pair.gnu.text_grid())
        .into_iter()
        .map(|(_, candidate)| candidate)
        .collect::<Vec<_>>();
    let neo_candidates = candidate_rows(&pair.neo.text_grid())
        .into_iter()
        .map(|(_, candidate)| candidate)
        .collect::<Vec<_>>();
    assert_eq!(
        neo_candidates, gnu_candidates,
        "Corfu candidate rows differ"
    );
    let expected_candidates = expect![[r#"
        [
            "café     kind:ca",
            "camel    kind:ca",
            "carbide  kind:ca",
        ]
    "#]];
    expected_candidates.assert_debug_eq(&gnu_candidates);

    let gnu_popup = painted_ansi_rows(RawTerminalSnapshot::capture_rows(
        pair.gnu.screen(),
        candidate_row_range(&pair.gnu.text_grid()),
    ));
    let neo_popup = painted_ansi_rows(RawTerminalSnapshot::capture_rows(
        pair.neo.screen(),
        candidate_row_range(&pair.neo.text_grid()),
    ));
    assert_eq!(neo_popup, gnu_popup, "Corfu initial popup styling differs");
    let expected_popup = expect![[r#"
        <ESC>[0;48;2;25;26;27m <ESC>[0;38;2;173;216;230;48;2;25;26;27mca<ESC>[0;1;48;2;25;26;27mf<ESC>[0;48;2;25;26;27mé     kind:<ESC>[0;38;2;173;216;230;48;2;25;26;27mca<ESC>[0;48;2;25;26;27m <ESC>[0m<ESC>[0m
        <ESC>[0;48;2;25;26;27m <ESC>[0;38;2;173;216;230;48;2;25;26;27mca<ESC>[0;1;48;2;25;26;27mm<ESC>[0;48;2;25;26;27mel    kind:<ESC>[0;38;2;173;216;230;48;2;25;26;27mca<ESC>[0;48;2;25;26;27m <ESC>[0m<ESC>[0m
        <ESC>[0;48;2;25;26;27m <ESC>[0;38;2;173;216;230;48;2;25;26;27mca<ESC>[0;1;48;2;25;26;27mr<ESC>[0;48;2;25;26;27mbide  kind:<ESC>[0;38;2;173;216;230;48;2;25;26;27mca<ESC>[0;48;2;25;26;27m <ESC>[0m<ESC>[0m
    "#]];
    expected_popup.assert_eq(&gnu_popup);

    for session in [&mut pair.gnu, &mut pair.neo] {
        session.send_key("DOWN");
        session.read_until(Duration::from_secs(8), |grid| {
            candidate_rows(grid).len() == 3
                && grid
                    .iter()
                    .any(|row| row.contains("Documentation for café 界"))
        });
        assert!(
            session
                .text_grid()
                .iter()
                .any(|row| row.contains("Documentation for café 界")),
            "{} did not render Corfu echo documentation",
            session.name
        );
    }
    let gnu_selected = painted_ansi_rows(RawTerminalSnapshot::capture_rows(
        pair.gnu.screen(),
        candidate_row_range(&pair.gnu.text_grid()),
    ));
    let neo_selected = painted_ansi_rows(RawTerminalSnapshot::capture_rows(
        pair.neo.screen(),
        candidate_row_range(&pair.neo.text_grid()),
    ));
    assert_eq!(
        neo_selected, gnu_selected,
        "Corfu selected candidate styling differs"
    );
    let expected_selected = expect![[r#"
        <ESC>[0;38;2;229;229;229;48;2;0;65;94m <ESC>[0;38;2;173;216;230;48;2;0;65;94mca<ESC>[0;1;38;2;229;229;229;48;2;0;65;94mf<ESC>[0;38;2;229;229;229;48;2;0;65;94mé     kind:<ESC>[0;38;2;173;216;230;48;2;0;65;94mca<ESC>[0;38;2;229;229;229;48;2;0;65;94m <ESC>[0m<ESC>[0m
        <ESC>[0;48;2;25;26;27m <ESC>[0;38;2;173;216;230;48;2;25;26;27mca<ESC>[0;1;48;2;25;26;27mm<ESC>[0;48;2;25;26;27mel    kind:<ESC>[0;38;2;173;216;230;48;2;25;26;27mca<ESC>[0;48;2;25;26;27m <ESC>[0m<ESC>[0m
        <ESC>[0;48;2;25;26;27m <ESC>[0;38;2;173;216;230;48;2;25;26;27mca<ESC>[0;1;48;2;25;26;27mr<ESC>[0;48;2;25;26;27mbide  kind:<ESC>[0;38;2;173;216;230;48;2;25;26;27mca<ESC>[0;48;2;25;26;27m <ESC>[0m<ESC>[0m
    "#]];
    expected_selected.assert_eq(&gnu_selected);

    for session in [&mut pair.gnu, &mut pair.neo] {
        session.send_key("RET");
        session.read_until(Duration::from_secs(8), |grid| {
            grid.iter().any(|row| row.trim_start().starts_with("café"))
                && candidate_rows(grid).len() < 3
        });
        assert!(
            session
                .text_grid()
                .iter()
                .any(|row| row.trim_start().starts_with("café")),
            "{} did not insert the selected Corfu candidate",
            session.name
        );
    }
}
