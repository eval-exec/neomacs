//! Terminal smoke coverage for the public metric helpers used by Vertico.
//! Native Thin-only selection is covered by the GUI oracle, not this TTY case.

use std::time::Duration;

use super::super::scenario::{
    DisplayCheckpoint, PackageTuiScenario, PairTimeout, ReadinessCheckpoint,
};
use super::super::{COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, VERTICO_MELPA_PIN};
use super::harness::{both, wait_for};

const PRELUDE: &str = r#"
(require 'vertico)
(setq vertico-count 5
      vertico-sort-function #'identity)
(vertico-mode 1)
(switch-to-buffer (get-buffer-create "*tty-font-metrics*"))
(setq-local mode-line-format '(" Font metrics "))
(erase-buffer)
(let ((font-height (default-font-height))
      (line-height (default-line-height)))
  (unless (and (numberp font-height) (> font-height 0)
               (numberp line-height) (> line-height 0))
    (error "TTY metric helpers must return positive heights: %S %S"
           font-height line-height)))
(insert "TTY-METRICS-READY\n")
(goto-char (point-min))
(defun neomacs-tty-font-complete ()
  (interactive)
  (let ((selection (completing-read "TTY metric completion: " '("alpha" "beta" "gamma") nil t)))
    (unless (equal selection "alpha")
      (error "Unexpected completion: %S" selection))
    (erase-buffer)
    (insert "TTY-METRICS-PASSED\nAccepted: alpha\n")
    (goto-char (point-min))))
(global-set-key (kbd "C-c t") #'neomacs-tty-font-complete)
"#;

#[test]
fn tty_font_metric_helpers_allow_real_vertico_keyboard_completion() {
    let oracle = CachedMelpaOracle::new(VERTICO_MELPA_PIN, "vertico.el")
        .expect("prepare pinned actual Vertico")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare pinned Compat dependency")
        .with_prelude(PRELUDE);
    let mut pair = PackageTuiScenario::new("vertico-tty-font-metrics", oracle.prepared_packages())
        .spawn_when_ready(
            ReadinessCheckpoint::new(
                "positive TTY public font and line heights",
                // This debug runtime source-loads the package graph. The larger
                // startup allowance belongs to this case, not the shared harness.
                PairTimeout::per_editor(Duration::from_secs(15), Duration::from_secs(120)),
            ),
            |grid| grid.iter().any(|line| line.contains("TTY-METRICS-READY")),
        )
        .expect("spawn both editors with positive public TTY metric helpers");
    both(&mut pair, "open real Vertico completion", |session| {
        session.send_keys("C-c t");
        wait_for(
            session,
            Duration::from_secs(10),
            "the three actual Vertico candidates",
            |grid| {
                grid.iter()
                    .any(|row| row.contains("TTY metric completion:"))
                    && ["alpha", "beta", "gamma"]
                        .iter()
                        .all(|candidate| grid.iter().any(|row| row.trim() == *candidate))
            },
        );
    })
    .expect("both editors expose the real package's candidates");
    pair.assert_display(DisplayCheckpoint::new("real Vertico candidate window"));
    both(&mut pair, "accept alpha using the keyboard", |session| {
        session.send_key("RET");
        wait_for(
            session,
            Duration::from_secs(10),
            "accepted alpha and positive TTY metrics",
            |grid| {
                grid.iter().any(|row| row.contains("TTY-METRICS-PASSED"))
                    && grid.iter().any(|row| row.contains("Accepted: alpha"))
            },
        );
    })
    .expect("real keyboard input accepts alpha in both editors");
    pair.assert_display(DisplayCheckpoint::new(
        "accepted alpha with positive TTY metrics",
    ));
}
