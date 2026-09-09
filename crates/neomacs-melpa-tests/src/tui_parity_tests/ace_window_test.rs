//! Real ace-window workflows on a PTY. No graphical assumptions and no
//! replacement of terminal-name: both editors use their actual TTY terminal.
use crate::{ACE_WINDOW_MELPA_PIN, CachedMelpaOracle};
use neomacs_melpa_test_support::{
    PackageTuiPair, PackageTuiScenario, PairTimeout, ReadinessCheckpoint,
};
use std::time::Duration;

const PRELUDE: &str = r#"
(require 'cl-lib)
(require 'ace-window)
(defvar aw364-buffers nil)
(defvar aw364-report-sequence 0)
(defun aw364-report ()
  (interactive)
  (message "AW364 %s candidates=%d initial=%S clean=%S report=%d"
           (buffer-name (window-buffer (selected-window)))
           (length (aw-window-list))
           (equal (terminal-name (selected-frame)) "initial_terminal")
           (cl-every (lambda (b) (not (buffer-modified-p b))) aw364-buffers)
           (cl-incf aw364-report-sequence)))
(defun aw364-two-windows ()
  (interactive)
  (delete-other-windows)
  (set-window-buffer (selected-window) (nth 0 aw364-buffers))
  (set-window-buffer (split-window-right) (nth 2 aw364-buffers))
  (setq aw-dispatch-always nil))
(defun aw364-setup ()
  (delete-other-windows)
  (setq aw-scope 'frame aw-keys '(?a ?s ?d ?f ?g ?h ?j ?k ?l)
        aw-dispatch-always t aw-background nil)
  (setq aw364-buffers
        (mapcar (lambda (name)
                  (let ((buffer (generate-new-buffer name)))
                    (with-current-buffer buffer
                      (insert name " unchanged\n")
                      (goto-char (point-min))
                      (set-buffer-modified-p nil))
                    buffer))
                '("AW-left" "AW-bottom" "AW-right")))
  (let* ((left (selected-window)) (right (split-window-right))
         (bottom (split-window left nil 'below)))
    (set-window-buffer left (nth 0 aw364-buffers))
    (set-window-buffer bottom (nth 1 aw364-buffers))
    (set-window-buffer right (nth 2 aw364-buffers))
    (select-window left))
  (global-set-key (kbd "C-c w") #'ace-window)
  (global-set-key (kbd "C-c i") #'aw364-report)
  (global-set-key (kbd "C-c 2") #'aw364-two-windows)
  (unless (and (not (display-graphic-p)) (= (length (aw-window-list)) 3))
    (error "Expected three actual TTY candidates, terminal=%S windows=%S"
           (terminal-name) (aw-window-list))))
(aw364-setup)
"#;

fn start(label: &str) -> PackageTuiPair {
    let oracle = CachedMelpaOracle::new(ACE_WINDOW_MELPA_PIN, "ace-window.el")
        .expect("prepare pinned ace-window and avy")
        .with_prelude(PRELUDE);
    let mut pair = PackageTuiScenario::new(label, oracle.prepared_packages())
        .spawn_when_ready(
            ReadinessCheckpoint::new(
                "real ace-window TTY candidates",
                PairTimeout::same(Duration::from_secs(30)),
            ),
            |grid| {
                [
                    "AW-left unchanged",
                    "AW-bottom unchanged",
                    "AW-right unchanged",
                ]
                .iter()
                .all(|text| grid.iter().any(|row| row.contains(text)))
            },
        )
        .expect("both editors display the three-window fixture");
    // Observe identity after startup warnings have been displayed, rather than
    // relying on a startup message that an idle warning can replace.
    expect_state(
        &mut pair,
        "AW364 AW-left candidates=3 initial=nil clean=t report=1",
    );
    pair
}

fn expect_state(pair: &mut PackageTuiPair, expected: &str) {
    pair.send_keys_both("C-c i");
    pair.drive_both(|session| {
        session.read_until(Duration::from_secs(10), |grid| {
            grid.iter().any(|row| row.contains(expected))
        });
        assert!(
            session.text_grid().iter().any(|row| row.contains(expected)),
            "{}: expected {expected:?}\n{}",
            session.name,
            session.text_grid().join("\n")
        );
    });
}

#[test]
fn ace_window_tui_selects_labelled_windows_and_cancels_without_editing() {
    let mut pair = start("ace-window-labels-and-cancel");
    pair.send_keys_both("C-c w d");
    expect_state(
        &mut pair,
        "AW364 AW-right candidates=3 initial=nil clean=t report=2",
    );
    pair.send_keys_both("C-c w s");
    expect_state(
        &mut pair,
        "AW364 AW-bottom candidates=3 initial=nil clean=t report=3",
    );
    pair.send_keys_both("C-c w C-g");
    expect_state(
        &mut pair,
        "AW364 AW-bottom candidates=3 initial=nil clean=t report=4",
    );
    pair.send_keys_both("C-c w a");
    expect_state(
        &mut pair,
        "AW364 AW-left candidates=3 initial=nil clean=t report=5",
    );
}

#[test]
fn ace_window_tui_two_windows_switch_without_a_label_prompt() {
    let mut pair = start("ace-window-two-window-switch");
    pair.send_keys_both("C-c 2");
    expect_state(
        &mut pair,
        "AW364 AW-left candidates=2 initial=nil clean=t report=2",
    );
    pair.send_keys_both("C-c w");
    expect_state(
        &mut pair,
        "AW364 AW-right candidates=2 initial=nil clean=t report=3",
    );
    pair.send_keys_both("C-c w");
    expect_state(
        &mut pair,
        "AW364 AW-left candidates=2 initial=nil clean=t report=4",
    );
}
