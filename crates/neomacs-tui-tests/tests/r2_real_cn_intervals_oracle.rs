#![cfg(unix)]
//! Proposed, unexecuted live allocation oracle for real C-n + TUI redisplay.
use crate::support::{
    assert_pair_exact_display, boot_pair, eval_expression, read_both, send_both_raw,
    write_home_file,
};
use neomacs_tui_tests::TuiSession;
use std::fs;
use std::time::{Duration, Instant};

const KEYS: usize = 64;
const FIXTURE: &str = r#";;; -*- lexical-binding: t -*-
(defvar r2-key-buffer nil)
(defvar r2-key-armed nil)
(defvar r2-key-sequence 0)
(defvar r2-key-inflight nil)
(defvar r2-key-before nil)
(defvar r2-key-command-ok nil)
(defvar r2-key-event nil)
(defvar r2-key-redisplayed nil)
(defvar r2-key-await-timer nil)

(defun r2-key-path (kind sequence)
  (expand-file-name (format "r2-%s-%03d" kind sequence) (getenv "HOME")))

(defun r2-key-counters ()
  (let ((intervals intervals-consed)
        (memory-intervals (nth 5 (memory-use-counts))))
    (unless (= intervals memory-intervals)
      (error "intervals-consed and memory-use-counts disagree: %S %S"
             intervals memory-intervals))
    (list intervals memory-intervals gcs-done)))

(defun r2-key-property-string (kind)
  ;; Separate objects for each overlay. Each one retains 16 alternating face
  ;; intervals, including the visible source-kind delimiters.
  (let ((string (concat kind "[xxxxxxxxxxxx]" kind)))
    (unless (= (length string) 16) (error "bad source-string fixture"))
    (dotimes (index 16)
      (put-text-property index (1+ index) 'face
                         (if (= (% index 2) 0) 'bold 'font-lock-string-face)
                         string))
    string))

(defun r2-key-finish (sequence)
  ;; The host writes SEEN only AFTER it has parsed the unique marker from
  ;; real terminal output. Do not trust redisplay's return as a paint fence.
  (when (file-exists-p (r2-key-path "seen" sequence))
    (cancel-timer r2-key-await-timer)
    (setq r2-key-await-timer nil)
    (with-current-buffer r2-key-buffer
      (unless (and (= sequence r2-key-sequence)
                   (= sequence r2-key-inflight)
                   (eq (window-buffer (selected-window)) r2-key-buffer))
        (error "lost R2 key/paint ownership"))
      (let ((after (r2-key-counters)))
        ;; Freeze counters BEFORE temp-buffer serialization or file writes.
        (let ((fields (list sequence (point) (line-number-at-pos) (current-column)
                            r2-key-command-ok r2-key-event r2-key-redisplayed
                            (nth 0 r2-key-before) (nth 1 r2-key-before)
                            (nth 0 after) (nth 1 after)
                            (nth 2 r2-key-before) (nth 2 after))))
          (with-temp-file (r2-key-path "ack" sequence)
            (insert (mapconcat #'number-to-string fields " ") "\n"))))
      (setq r2-key-inflight nil))))

(defun r2-key-paint (sequence)
  (with-current-buffer r2-key-buffer
    (unless (= sequence r2-key-inflight) (error "stale R2 paint"))
    (message "R2-ACK-%03d" sequence)
    (setq r2-key-redisplayed (if (redisplay t) 1 0))
    (setq r2-key-await-timer
          (run-with-timer 0.01 0.01 #'r2-key-finish sequence))))

(defun r2-key-pre-command ()
  (when (and r2-key-armed (eq this-command 'next-line))
    (when r2-key-inflight (error "next key arrived before prior paint ack"))
    (setq r2-key-before (r2-key-counters))))

(defun r2-key-post-command ()
  (when (and r2-key-armed (eq this-command 'next-line))
    (setq r2-key-sequence (1+ r2-key-sequence)
          r2-key-inflight r2-key-sequence
          r2-key-command-ok (if (eq this-command 'next-line) 1 0)
          r2-key-event last-command-event)
    (run-with-idle-timer 0.03 nil #'r2-key-paint r2-key-sequence)))

(defun r2-key-warm-ready ()
  (dotimes (_ 3) (redisplay t))
  (message "R2-WARM-READY")
  (redisplay t)
  (setq r2-key-armed t)
  (with-temp-file (r2-key-path "ready" 0) (insert "ready\n")))

(defun r2-key-arm ()
  (interactive)
  (unless (= (point) (point-min)) (error "R2 must start at buffer beginning"))
  (run-with-idle-timer 0.03 nil #'r2-key-warm-ready))

(defun r2-key-setup ()
  (delete-other-windows)
  (switch-to-buffer (get-buffer-create " *r2-real-key*"))
  (fundamental-mode)
  (setq r2-key-buffer (current-buffer))
  (erase-buffer)
  (remove-overlays)
  (dotimes (index 128) (insert (format "R2-%03d @ body\n" (1+ index))))
  (goto-char (point-min))
  ;; Every text line carries both sources, including any viewport at line 65.
  ;; 128 lines x two distinct strings x 16 property intervals = 4,096.
  (dotimes (_ 128)
    (let* ((start (line-beginning-position))
           (before (make-overlay start (1+ start)))
           (display (make-overlay (+ start 7) (+ start 8))))
      (overlay-put before 'before-string (r2-key-property-string "B"))
      (overlay-put display 'display (r2-key-property-string "D")))
    (forward-line 1))
  (goto-char (point-min))
  (setq-local mode-line-format nil header-line-format nil tab-line-format nil
              truncate-lines t word-wrap nil line-move-visual nil
              bidi-paragraph-direction 'left-to-right)
  (setq next-line-add-newlines nil echo-keystrokes 0 message-log-max nil
        gc-cons-threshold most-positive-fixnum)
  (set-window-start nil (point-min))
  (set-window-hscroll nil 0)
  (add-hook 'pre-command-hook #'r2-key-pre-command nil t)
  (add-hook 'post-command-hook #'r2-key-post-command nil t)
  (global-set-key (kbd "<f8>") #'r2-key-arm)
  nil)
(r2-key-setup)
"#;

#[derive(Debug)]
struct Ack(Vec<i64>);
impl Ack {
    fn read(session: &TuiSession, sequence: usize) -> Option<Self> {
        let text =
            fs::read_to_string(session.home_dir().join(format!("r2-ack-{sequence:03}"))).ok()?;
        if !text.ends_with('\n') {
            return None;
        }
        let fields = text
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<Vec<i64>, _>>()
            .ok()?;
        (fields.len() == 13 && fields[0] == sequence as i64).then_some(Self(fields))
    }
    fn checked_delta(&self, sequence: usize) -> i64 {
        let f = &self.0;
        assert_eq!(
            f[1],
            1 + 14 * sequence as i64,
            "real C-n point endpoint: {self:?}"
        );
        assert_eq!(f[2], sequence as i64 + 1, "one real line per key: {self:?}");
        assert_eq!(f[3], 0, "preserve column zero: {self:?}");
        assert_eq!(
            (f[4], f[5], f[6]),
            (1, 14, 1),
            "next-line/C-n/redisplay witness: {self:?}"
        );
        assert_eq!(f[7], f[8], "before counter agreement: {self:?}");
        assert_eq!(f[9], f[10], "after counter agreement: {self:?}");
        assert_eq!(
            f[11], f[12],
            "unexpected GC inside key/paint budget: {self:?}"
        );
        let delta = f[9] - f[7];
        assert!(delta >= 0, "counter wrapped or reset: {self:?}");
        delta
    }
}

fn wait_pair(
    gnu: &mut TuiSession,
    neo: &mut TuiSession,
    label: &str,
    ready: impl Fn(&TuiSession) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !ready(gnu) || !ready(neo) {
        assert!(
            Instant::now() < deadline,
            "{label} timeout\nGNU:\n{}\nNeo:\n{}",
            gnu.text_grid().join("\n"),
            neo.text_grid().join("\n")
        );
        read_both(gnu, neo, Duration::from_millis(20));
    }
}

fn assert_source_grid(session: &TuiSession) {
    assert!(
        session
            .text_grid()
            .iter()
            .any(|line| line.contains("B[") && line.contains("D[") && line.contains("R2-")),
        "both propertized source kinds must actually reach the grid:\n{}",
        session.text_grid().join("\n")
    );
}

#[test]
fn r2_real_cn_redisplay_intervals_per_key_match_live_gnu() {
    neomacs_tui_tests::assert_neomacs_dumped_runtime(&neomacs_tui_tests::neomacs_binary());
    eprintln!("GNU oracle: {}", neomacs_tui_tests::gnu_oracle_stamp());
    let (mut gnu, mut neo) = boot_pair("");
    for session in [&gnu, &neo] {
        write_home_file(session, "r2-real-key.el", FIXTURE);
    }
    eval_expression(&mut gnu, &mut neo, r#"(load "~/r2-real-key.el" nil t)"#);
    send_both_raw(&mut gnu, &mut neo, &neomacs_tui_tests::emacs_key("F8"));
    wait_pair(&mut gnu, &mut neo, "warm", |session| {
        session.home_dir().join("r2-ready-000").exists()
            && session
                .text_grid()
                .iter()
                .any(|line| line.contains("R2-WARM-READY"))
    });
    assert_source_grid(&gnu);
    assert_source_grid(&neo);
    assert_pair_exact_display("r2_real_cn_warm_sources", &gnu, &neo);

    let mut gnu_deltas = Vec::new();
    let mut neo_deltas = Vec::new();
    for sequence in 1..=KEYS {
        // One literal C-n; never a key burst, keyboard macro, or Lisp motion loop.
        send_both_raw(&mut gnu, &mut neo, b"\x0e");
        let marker = format!("R2-ACK-{sequence:03}");
        wait_pair(&mut gnu, &mut neo, "paint marker", |session| {
            session
                .text_grid()
                .iter()
                .any(|line| line.contains(&marker))
        });
        assert_source_grid(&gnu);
        assert_source_grid(&neo);
        assert_pair_exact_display(&format!("r2_real_cn_step_{sequence:03}"), &gnu, &neo);
        // Out-of-band host acknowledgement: counters cannot be finalized
        // until the real PTY grid contains this sequence's rendered marker.
        for session in [&gnu, &neo] {
            fs::write(
                session.home_dir().join(format!("r2-seen-{sequence:03}")),
                "seen\n",
            )
            .unwrap();
        }
        wait_pair(&mut gnu, &mut neo, "allocation ack", |session| {
            Ack::read(session, sequence).is_some()
        });
        let g = Ack::read(&gnu, sequence).unwrap();
        let n = Ack::read(&neo, sequence).unwrap();
        gnu_deltas.push(g.checked_delta(sequence));
        neo_deltas.push(n.checked_delta(sequence));
        assert_eq!(
            &n.0[0..7],
            &g.0[0..7],
            "live command endpoint parity at key {sequence}"
        );
    }
    let gnu_total: i64 = gnu_deltas.iter().sum();
    let neo_total: i64 = neo_deltas.iter().sum();
    eprintln!(
        "real C-n intervals: GNU {gnu_total}/{KEYS} = {:.3}/key; Neo {neo_total}/{KEYS} = {:.3}/key\nGNU {gnu_deltas:?}\nNeo {neo_deltas:?}",
        gnu_total as f64 / KEYS as f64,
        neo_total as f64 / KEYS as f64
    );
    // Compare against this run's live GNU population without an allowance.
    assert!(
        neo_total <= gnu_total,
        "Neo interval allocation exceeds live GNU: GNU={gnu_total}, Neo={neo_total}, keys={KEYS}"
    );
}
