#![cfg(unix)]
//! Fresh live GNU31.1 expectations for the TTY current-matrix posn contract.
//! Boundary, stale-tail and split cases compare the new sidecar's TEXT
//! object dimensions, canonical iterator offsets and source identity to fresh
//! GNU, retaining full captures. Main's documented col-row, hidden-EOB,
//! unavailable-walk and named-chrome residuals are checked against the same
//! binary with the extent knob OFF; this lane cannot change those residuals.
//! Other active cases retain their full normalized ten-cell comparisons.

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};
use std::path::PathBuf;
use std::time::Duration;

const PROBE_EL: &str = r##";;; -*- lexical-binding: t -*-
(require 'json)
(defvar d5-extent-results nil)
(defvar d5-extent-contract-results nil)
(defvar d5-extent-residual-results nil)
(defun d5-extent-main-residual (label posn)
  ;; Only paths proven unchanged between e3941a387f and817b belong here.
  ;; The Rust test also requires these values to equal the explicit OFF arm.
  (let* ((case (getenv "NEOMACS_POSN_EXTENT_ORACLE_CASE"))
         (stage (car-safe label))
         (query (car-safe (cdr-safe label)))
         (hidden-eob (and (equal case "stale-tail")
                          (memq (car-safe stage) '(changed-hscroll new-matrix))
                          (memq query '(point back))
                          (= (nth 2 label) (point-max))))
         (chrome (memq (nth 1 posn) '(mode-line header-line tab-line)))
         (unavailable (eq stage 'unavailable)))
    (cond (chrome 'chrome)
          (hidden-eob 'hidden-eob)
          (unavailable 'unavailable)
          (t 'col-row))))
(defun d5-extent-project-contract (label posn)
  (let ((residual (d5-extent-main-residual label posn)))
    (cond
     ;; Named chrome's physical extent/STRING cells are residual209; the
     ;; TEXT sidecar deliberately does not consume or publish those cells.
     ((eq residual 'chrome) 'preexisting-chrome-source-and-physical-geometry)
     ((eq residual 'hidden-eob) 'preexisting-hidden-eob-visibility)
     (t (list (nth 0 posn) (nth 1 posn) (nth 2 posn) (nth 4 posn)
              (nth 5 posn)
              ;; No canonical row producer ran at this deliberate seam.
              ;; Main's fallback reports the clicked row below EOB; its
              ;; offset is unchanged by the new matrix extent sidecar.
              (if (eq residual 'unavailable)
                  'preexisting-unavailable-walk-offset (nth 8 posn))
              (nth 9 posn))))))
(defun d5-extent-project-residual (label posn)
  (let ((residual (d5-extent-main-residual label posn)))
    (cond ((eq residual 'chrome) posn)
          ((eq residual 'hidden-eob)
           (list (nth 0 posn) (nth 1 posn) (nth 2 posn)
                 (nth 4 posn) (nth 5 posn) (nth 6 posn)))
          ((eq residual 'unavailable) (list (nth 6 posn) (nth 8 posn)))
          (t (list (nth 6 posn))))))
(defun d5-extent-normalize (value)
  (cond ((windowp value) (list 'window (buffer-name (window-buffer value))))
        ((framep value) 'frame)
        ((consp value) (cons (d5-extent-normalize (car value))
                             (d5-extent-normalize (cdr value))))
        ((vectorp value) (apply #'vector (mapcar #'d5-extent-normalize value)))
        (t value)))
(defun d5-extent-body-posn (posn)
  ;; Only editor-specific window identity is normalized. Computed posn time
  ;; is zero and all object dimensions, source values and coordinates remain.
  (d5-extent-normalize posn))
(defun d5-extent-area-fields (posn)
  ;; Source identity is already a separate ledger residual. Preserve the full
  ;; ten cells in FULL; this projection tests the new matrix extent contract.
  (list (nth 1 posn) (nth 2 posn) (nth 5 posn) (nth 6 posn)
        (nth 8 posn) (nth 9 posn)))
(defun d5-extent-record (label posn &optional area)
  (push (list label (if area (d5-extent-area-fields posn)
                     (d5-extent-body-posn posn))
              (d5-extent-body-posn posn)) d5-extent-results)
  (let ((residual (d5-extent-main-residual label posn)))
    ;; The hidden-EOB visibility gap also creates an extra conditional back
    ;; record in Neo. Preserve it in FULL and the ON/OFF residual invariant.
    (unless (and (eq residual 'hidden-eob) (eq (cadr label) 'back))
      (push (d5-extent-normalize
             (list label (d5-extent-project-contract label posn)))
            d5-extent-contract-results))
    (push (d5-extent-normalize
           (list label (d5-extent-project-residual label posn)))
          d5-extent-residual-results)))
(defun d5-extent-setup (text)
  ;; Fix the plain terminal origin before both editors prepare their matrix.
  (menu-bar-mode -1)
  (set-frame-parameter nil 'menu-bar-lines 0)
  (delete-other-windows)
  (switch-to-buffer (get-buffer-create "d5-extent-body"))
  (erase-buffer) (insert text) (goto-char (point-min))
  (setq-local mode-line-format nil)
  (setq-local header-line-format nil)
  (setq-local tab-line-format nil)
  (setq-local truncate-lines nil)
  (setq-local word-wrap nil)
  (set-window-margins nil 0 0)
  (set-window-hscroll nil 0)
  (set-window-start nil (point-min))
  (redisplay t))
(defun d5-extent-grid (label window)
  (dolist (y '(0 1 2 3 6 10))
    (dolist (x '(0 1 2 3 7 15 60 118))
      (d5-extent-record (list label 'xy x y) (posn-at-x-y x y window))))
  (with-current-buffer (window-buffer window)
    (let ((positions (delete-dups (list (point-min) (min (+ (point-min) 1) (point-max))
                                      (min (+ (point-min) 3) (point-max))
                                      (max (point-min) (1- (point-max))) (point-max)))))
      (dolist (p positions)
        (let* ((posn (posn-at-point p window)) (xy (and posn (posn-x-y posn))))
          (d5-extent-record (list label 'point p) posn)
          (when xy (d5-extent-record (list label 'back p)
                    (posn-at-x-y (car xy) (cdr xy) window))))))))
(let ((case (getenv "NEOMACS_POSN_EXTENT_ORACLE_CASE")))
  (cond
   ((equal case "popup")
    (d5-extent-setup (apply #'concat (make-list (/ 2097152 81)
                      (concat (make-string 80 ?a) "\n"))))
    (goto-char 10) (redisplay t)
    (let* ((window (selected-window)) (posn (posn-at-point 10 window))
           (xy (and posn (posn-x-y posn))))
      (d5-extent-record 'popup-point posn)
      (d5-extent-record 'popup-back (and xy (posn-at-x-y (car xy) (cdr xy) window)))
      (push (list 'popup-visible (pos-visible-in-window-p 10 window t)) d5-extent-results)
      (push (list 'popup-size (window-text-pixel-size window 1 nil nil 12)) d5-extent-results)))
   ((equal case "line-end")
    (dolist (text '("" "a" "a\n" "abcd\nef\n"))
      (d5-extent-setup text)
      (d5-extent-grid (list 'text text) (selected-window))))
   ((equal case "wide-tab")
    (d5-extent-setup (concat "a\tb" (string #x754c) "e" (string #x0301) "z\n"))
    (setq-local tab-width 4)
    (redisplay t)
    (d5-extent-grid 'tab-wide-combining (selected-window))
    (compose-region 5 7)
    (redisplay t)
    (d5-extent-grid 'explicit-composition (selected-window)))
   ((equal case "stale-tail")
    (dolist (colored '(nil t))
      (d5-extent-setup "a\n")
      (when colored
        (put-text-property 1 3 'face '(:background "red" :extend t))
        (redisplay t))
      ;; Retained current matrix is a short row. The LIVE walk must read its
      ;; actual used tail cells after text grows, before any new redisplay.
      (goto-char 2) (insert "bcdefghijklmnopqrstuvwxyz")
      (d5-extent-grid (list 'old-short colored) (selected-window))
      (setq-local truncate-lines t)
      (set-window-hscroll nil 9)
      (d5-extent-grid (list 'changed-hscroll colored) (selected-window))
      (redisplay t)
      (d5-extent-grid (list 'new-matrix colored) (selected-window))))
   ((equal case "split-cold")
    (d5-extent-setup "abcdef\nghijkl\n")
    (let ((lower (split-window-below)))
      ;; The canonical live query is available before this sibling's own
      ;; redisplay. GNU TTY current matrices can inherit accepted frame cells
      ;; through fake_current_matrices; the TEXT extent comparison keeps this
      ;; lane-owned requirement strict.
      (d5-extent-grid 'cold lower)
      ;; Same deliberate unavailable-producer seam as BOUNDED's14/14 proof.
      (let ((noninteractive t))
        (dotimes (i 32)
          (d5-extent-record (list 'unavailable i)
            (posn-at-x-y (+ 2 (% i 8)) (% i 4) lower))))
      (with-current-buffer (window-buffer lower)
        (setq-local tab-line-format '("TAB"))
        (setq-local header-line-format '("HEADER")))
      (redisplay t)
      (d5-extent-grid 'lower-with-top-chrome lower)
      (with-current-buffer (window-buffer lower) (goto-char 2) (insert "-new-longer-"))
      (d5-extent-grid 'lower-stale lower)))
   ((equal case "chrome-margin")
    (d5-extent-setup "abc\ndef\n")
    (setq-local mode-line-format '("MODE"))
    (setq-local header-line-format '("HEADER"))
    (setq-local tab-line-format '("TAB"))
    (set-window-margins nil 2 2)
    (redisplay t)
    (let* ((window (selected-window)) (height (window-total-height window)))
      (dolist (y (list 0 1 (- height 1)))
        (dolist (x '(0 1 3 8 30 118))
          (d5-extent-record (list 'chrome x y) (posn-at-x-y x y window t) t)))
      (dolist (x '(0 1 2 117 118 119))
        (d5-extent-record (list 'margin x) (posn-at-x-y x 2 window t) t)))
    ;; A colored margin tests GNU's conditional used-area fill policy too.
    ;; This is a strict diagnostic for any pre-existing producer omission.
    (set-face-attribute 'margin nil :background "red")
    (redisplay t)
    (dolist (x '(0 1 118 119))
      (d5-extent-record (list 'colored-margin x) (posn-at-x-y x 2 (selected-window) t) t)))
   ((equal case "mouse")
    (d5-extent-setup "abcdef\n")
    (xterm-mouse-mode 1)
    (let* ((edges (window-edges (selected-window)))
           (coordinates (list (list (+ (nth 0 edges) 1) (nth 1 edges))
                              (list (+ (nth 0 edges) 39) (nth 1 edges))
                              (list (+ (nth 0 edges) 1) (1+ (nth 1 edges)))))
           (i 0))
      (with-temp-file (getenv "NEOMACS_POSN_EXTENT_ORACLE_READY")
        (insert (json-encode (vconcat (mapcar #'vconcat coordinates)))))
      (while (< i (length coordinates))
        (let ((event (with-timeout (60 (error "No decoded mouse event for extent oracle"))
                       (read-key nil t))))
          (unless event (error "No mouse event for extent oracle"))
          (when (and (mouse-event-p event) (eq (car-safe event) 'mouse-1))
            (let ((posn (copy-sequence (event-start event)))
                  (coordinate (nth i coordinates)))
              ;; Actual input timestamps differ across processes. Preserve
              ;; every other posn cell, including anchors and object extent.
              (setcar (nthcdr 3 posn) 0)
              (d5-extent-record (list 'mouse i) posn)
              (d5-extent-record (list 'query i)
                (posn-at-x-y (car coordinate) (cadr coordinate) (selected-frame))))
            (setq i (1+ i)))))))
   (t (error "Unknown extent case %S" case))))
(let ((print-level nil) (print-length nil))
  (with-temp-file (getenv "NEOMACS_POSN_EXTENT_ORACLE_OUT")
    (prin1 (nreverse d5-extent-results) (current-buffer)) (insert "\n"))
  (with-temp-file (getenv "NEOMACS_POSN_EXTENT_ORACLE_CONTRACT")
    (prin1 (nreverse d5-extent-contract-results) (current-buffer)) (insert "\n"))
  (with-temp-file (getenv "NEOMACS_POSN_EXTENT_ORACLE_RESIDUAL")
    (prin1 (nreverse d5-extent-residual-results) (current-buffer)) (insert "\n")))
(kill-emacs 0)
"##;

struct ProbeCapture {
    full: String,
    contract: String,
    residual: String,
}

fn run_probe(program: PathBuf, case: &str, gnu: bool, setting: &str) -> ProbeCapture {
    let home = neomacs_tui_tests::TuiTempDirectory::new("neomacs-posn-extent-oracle-");
    let script = home.path().join("posn-extent-oracle.el");
    let out = home.path().join("posn-extent-oracle.out");
    let contract = home.path().join("posn-extent-oracle.contract");
    let residual = home.path().join("posn-extent-oracle.residual");
    let ready = home.path().join("posn-extent-oracle.ready");
    std::fs::write(&script, PROBE_EL).expect("write extent probe");
    let mut launch = TuiLaunch::new(program.as_os_str()).arg("-nw").arg("-Q");
    if gnu {
        launch = launch.arg("-no-comp-spawn")
            .arg("--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent))");
    }
    launch = launch
        .arg("-l")
        .arg(&script)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("NEOMACS_POSN_EXTENT_ORACLE_CASE", case)
        .env("NEOMACS_POSN_EXTENT_ORACLE_OUT", &out)
        .env("NEOMACS_POSN_EXTENT_ORACLE_CONTRACT", &contract)
        .env("NEOMACS_POSN_EXTENT_ORACLE_RESIDUAL", &residual)
        .env("NEOMACS_POSN_EXTENT_ORACLE_READY", &ready)
        .env("NEOMACS_POSN_OBJECT_EXTENT", setting);
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        if gnu { "GNU" } else { "Neomacs" },
        TuiTerminalConfig::new("xterm-256color", 40, 120),
    );
    if case == "mouse" {
        session.read_until(Duration::from_secs(60), |_| ready.exists());
        assert!(
            ready.exists(),
            "{} mouse probe did not publish coordinates; grid:\n{}",
            if gnu { "GNU" } else { "Neomacs" },
            session.text_grid().join("\n")
        );
        let coordinates: Vec<[i64; 2]> =
            serde_json::from_str(&std::fs::read_to_string(&ready).expect("mouse ready file"))
                .expect("mouse coordinates");
        for [x, y] in coordinates {
            session.send(
                format!("\x1b[<0;{};{}M\x1b[<0;{};{}m", x + 1, y + 1, x + 1, y + 1).as_bytes(),
            );
            session.read(Duration::from_millis(50));
        }
    }
    assert_eq!(
        session.run_to_completion(Duration::from_secs(90)),
        TuiProcessOutcome::Exited,
        "{} {case}: probe failed; grid:\n{}",
        if gnu { "GNU" } else { "Neomacs" },
        session.text_grid().join("\n")
    );
    let capture = ProbeCapture {
        full: std::fs::read_to_string(out).expect("full probe output"),
        contract: std::fs::read_to_string(contract).expect("owned contract output"),
        residual: std::fs::read_to_string(residual).expect("main residual output"),
    };
    // Full evidence survives temporary fixture cleanup even when the scoped
    // contract passes. These diagnostic files are never repository inputs.
    let artifacts =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tmp/posn-object-extent-captures");
    std::fs::create_dir_all(&artifacts).expect("create TMP extent capture directory");
    let editor = if gnu { "gnu" } else { "neo" };
    for (kind, contents) in [
        ("full", &capture.full),
        ("contract", &capture.contract),
        ("residual", &capture.residual),
    ] {
        std::fs::write(
            artifacts.join(format!("{case}-{editor}-{setting}-{kind}.el")),
            contents,
        )
        .expect("retain complete extent evidence");
    }
    capture
}

fn compare_extent_contract(case: &str) {
    let gnu = run_probe(PathBuf::from("emacs"), case, true, "on");
    let baseline = run_probe(neomacs_tui_tests::neomacs_binary(), case, false, "off");
    let neo = run_probe(neomacs_tui_tests::neomacs_binary(), case, false, "on");
    assert_eq!(
        neo.residual, baseline.residual,
        "{case}: extent ON changed a documented main residual; ON full:\n{}\nOFF full:\n{}",
        neo.full, baseline.full,
    );
    assert_eq!(
        neo.contract, gnu.contract,
        "{case}: live GNU TEXT matrix extent/offset/source contract; Neo full:\n{}\nGNU full:\n{}\nOFF full:\n{}",
        neo.full, gnu.full, baseline.full,
    );
}

fn compare(case: &str) {
    let gnu = run_probe(PathBuf::from("emacs"), case, true, "on");
    let neo = run_probe(neomacs_tui_tests::neomacs_binary(), case, false, "on");
    assert_eq!(
        neo.full, gnu.full,
        "{case}: live GNU expectation, including full captured posns"
    );
}

#[test]
fn tty_posn_line_end_newline_empty_and_nonempty_eob_like_gnu() {
    compare_extent_contract("line-end");
}
#[test]
fn tty_posn_tab_wide_and_composition_cells_like_gnu() {
    compare("wide-tab");
}
#[test]
fn tty_posn_stale_short_matrix_default_and_colored_tails_like_gnu() {
    compare_extent_contract("stale-tail");
}
#[test]
fn tty_posn_cold_unavailable_and_split_top_chrome_origins_like_gnu() {
    compare_extent_contract("split-cold");
}
#[test]
fn tty_actual_mouse_and_query_current_matrix_dimensions_like_gnu() {
    compare("mouse");
}

#[test]
fn tty_canonical_company_corfu_popup_queries_like_gnu() {
    compare("popup");
}
