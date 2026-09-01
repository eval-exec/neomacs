use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use expect_test::expect;
use neomacs_tui_tests::TuiSession;

use crate::{CachedMelpaOracle, MWIM_MELPA_PIN};

use super::support::PackageTuiPair;

const MWIM_VISUAL_TUI_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'timer)

(defvar mwim358-tui-baseline nil)
(defvar mwim358-tui-owned-buffers nil)
(defvar mwim358-tui-epoch 0)
(defvar mwim358-tui-wide-origin nil)

(defun mwim358-tui-window-state ()
  (mapcar
   (lambda (window)
     (list :window window
           :buffer (window-buffer window)
           :edges (window-edges window)
           :point (window-point window)
           :start (window-start window)
           :hscroll (window-hscroll window)
           :vscroll (window-vscroll window t)
           :dedicated (window-dedicated-p window)
           :parameters
           (sort (copy-tree (seq-filter #'cdr (window-parameters window)))
                 (lambda (left right)
                   (string< (symbol-name (car left))
                            (symbol-name (car right)))))
           :prev-buffers (copy-tree (window-prev-buffers window))
           :next-buffers (copy-tree (window-next-buffers window))
           :margins (window-margins window)
           :fringes (window-fringes window)
           :scroll-bars (window-scroll-bars window)))
   (window-list nil 'no-minibuf)))

(defun mwim358-tui-restore-windows ()
  (let ((configuration (plist-get mwim358-tui-baseline :configuration))
        (structure (plist-get mwim358-tui-baseline :windows)))
    (set-window-configuration configuration)
    (dolist (entry structure)
      (let ((window (plist-get entry :window)))
        (unless (window-live-p window)
          (error "MWIM TUI baseline window died: %S" window))
        (dolist (parameter (window-parameters window))
          (set-window-parameter window (car parameter) nil))
        (dolist (parameter (plist-get entry :parameters))
          (set-window-parameter window (car parameter) (cdr parameter)))
        (set-window-prev-buffers
         window (copy-tree (plist-get entry :prev-buffers)))
        (set-window-next-buffers
         window (copy-tree (plist-get entry :next-buffers)))
        (set-window-point window (plist-get entry :point))
        (set-window-start window (plist-get entry :start) 'noforce)
        (set-window-hscroll window (plist-get entry :hscroll))
        (set-window-vscroll window (plist-get entry :vscroll) t)))))

(defun mwim358-tui-snapshot-baseline ()
  ;; This runs as the first interactive test command, after terminal startup.
  (when mwim358-tui-baseline
    (error "MWIM TUI baseline was already captured"))
  (setq mwim358-tui-baseline
        (list :buffers (buffer-list)
              :processes (process-list)
              :timers (copy-sequence timer-list)
              :idle-timers (copy-sequence timer-idle-list)
              :buffer (current-buffer)
              :window (selected-window)
              :configuration (current-window-configuration)
              :windows (mwim358-tui-window-state))))

(defun mwim358-tui-observe-command ()
  (when (memq this-command
              '(mwim-beginning-of-line-or-code mwim-end-of-line-or-code))
    (cl-incf mwim358-tui-epoch)
    (message
     "MWIM-MOVE e=%d p=%d line=%d col=%d begin=%S end=%S mod=%S undo=%S"
     mwim358-tui-epoch (point) (line-number-at-pos) (current-column)
     mwim-beginning-of-line-function mwim-end-of-line-function
     (buffer-modified-p) buffer-undo-list)))

(defun mwim358-tui-setup ()
  (interactive)
  (let ((source (symbol-file 'mwim 'defun)))
    (unless (and (featurep 'mwim)
                 (package-built-in-p 'seq '(2 24))
                 source
                 (string-suffix-p "/mwim.el" source)
                 (equal load-suffixes '(".el")))
      (error "MWIM TUI activation boundary failed: mwim=%S seq=%S source=%S suffixes=%S"
             (featurep 'mwim) (package-built-in-p 'seq '(2 24))
             source load-suffixes)))
  (mwim358-tui-snapshot-baseline)
  (delete-other-windows)
  (select-window (split-window-right -24))
  (unless (and (= (window-width) 24) (= (window-body-width) 24))
    (error "MWIM TUI geometry mismatch: width=%S body=%S"
           (window-width) (window-body-width)))
  (let ((buffer (generate-new-buffer " *mwim358-visual*")))
    (push buffer mwim358-tui-owned-buffers)
    (switch-to-buffer buffer)
    (text-mode)
    (insert
     "  alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron\n\twide 界 cedar birch maple spruce willow aspen oak\n")
    (set-buffer-modified-p nil)
    (setq buffer-undo-list nil)
    (setq-local word-wrap t)
    (visual-line-mode 1)
    (setq-local mwim-beginning-of-line-function
                #'beginning-of-visual-line)
    (setq-local mwim-end-of-line-function #'end-of-visual-line)
    (let ((map (make-sparse-keymap)))
      (set-keymap-parent map (current-local-map))
      (define-key map (kbd "C-a") #'mwim-beginning-of-line-or-code)
      (define-key map (kbd "C-e") #'mwim-end-of-line-or-code)
      (use-local-map map))
    (add-hook 'post-command-hook #'mwim358-tui-observe-command nil t)
    (setq mwim358-tui-wide-origin
          (save-excursion
            (goto-char (point-min))
            (forward-line 1)
            (search-forward "maple")
            (point)))
    (goto-char 48)
    (set-window-point (selected-window) (point))
    (redisplay t)
    (message
     "MWIM-VISUAL-SETUP w=%d b=%d p=%d chars=%d begin=%S end=%S visual=%S wrap=%S"
     (window-width) (window-body-width) (point) (buffer-size)
     mwim-beginning-of-line-function mwim-end-of-line-function
     visual-line-mode word-wrap)))

(defun mwim358-tui-reset-middle ()
  (interactive)
  (goto-char 48)
  (set-window-point (selected-window) (point))
  (redisplay t)
  (message "MWIM-VISUAL-RESET-MIDDLE p=%d" (point)))

(defun mwim358-tui-reset-final ()
  (interactive)
  (goto-char 80)
  (set-window-point (selected-window) (point))
  (redisplay t)
  (message "MWIM-VISUAL-RESET-FINAL p=%d" (point)))

(defun mwim358-tui-use-wide-visual ()
  (interactive)
  (setq-local mwim-beginning-of-line-function
              #'beginning-of-visual-line)
  (setq-local mwim-end-of-line-function #'end-of-visual-line)
  (goto-char mwim358-tui-wide-origin)
  (set-window-point (selected-window) (point))
  (redisplay t)
  (message
   "MWIM-WIDE-VISUAL-RESET p=%d line=%d col=%d begin=%S end=%S"
   (point) (line-number-at-pos) (current-column)
   mwim-beginning-of-line-function mwim-end-of-line-function))

(defun mwim358-tui-reset-wide-visual ()
  (interactive)
  (goto-char mwim358-tui-wide-origin)
  (set-window-point (selected-window) (point))
  (redisplay t)
  (message "MWIM-WIDE-VISUAL-RESET-AGAIN p=%d line=%d col=%d"
           (point) (line-number-at-pos) (current-column)))

(defun mwim358-tui-use-logical ()
  (interactive)
  (setq-local mwim-beginning-of-line-function #'beginning-of-line)
  (setq-local mwim-end-of-line-function #'end-of-line)
  (goto-char mwim358-tui-wide-origin)
  (set-window-point (selected-window) (point))
  (redisplay t)
  (message
   "MWIM-LOGICAL-RESET p=%d line=%d col=%d begin=%S end=%S"
   (point) (line-number-at-pos) (current-column)
   mwim-beginning-of-line-function mwim-end-of-line-function))

(defun mwim358-tui-reset-logical ()
  (interactive)
  (goto-char mwim358-tui-wide-origin)
  (set-window-point (selected-window) (point))
  (redisplay t)
  (message "MWIM-LOGICAL-RESET-AGAIN p=%d line=%d col=%d"
           (point) (line-number-at-pos) (current-column)))

(defun mwim358-tui-cleanup ()
  (interactive)
  (let (errors state)
    (cl-labels
        ((attempt
          (phase function)
          (condition-case condition
              (funcall function)
            (t (push (list phase condition) errors))))
         (sweep
          (number)
          (dolist
              (process
               (seq-difference
                (process-list) (plist-get mwim358-tui-baseline :processes)
                #'eq))
            (attempt
             (list 'process number)
             (lambda ()
               (set-process-query-on-exit-flag process nil)
               (when (process-live-p process) (delete-process process)))))
          (dolist
              (timer
               (delete-dups
                (append
                 (seq-difference
                  timer-list (plist-get mwim358-tui-baseline :timers) #'eq)
                 (seq-difference
                  timer-idle-list
                  (plist-get mwim358-tui-baseline :idle-timers) #'eq))))
            (attempt (list 'timer number) (lambda () (cancel-timer timer))))
          (dolist
              (buffer
               (seq-difference
                (buffer-list) (plist-get mwim358-tui-baseline :buffers) #'eq))
            (attempt
             (list 'buffer number)
             (lambda ()
               (when (buffer-live-p buffer)
                 (set-buffer-modified-p nil)
                 (kill-buffer buffer)))))))
      (if (not mwim358-tui-baseline)
          (push '(baseline missing) errors)
        (attempt 'window-first #'mwim358-tui-restore-windows)
        (dotimes (number 2) (sweep number))
        (attempt 'window-final #'mwim358-tui-restore-windows)
        (attempt
         'select-baseline
         (lambda ()
           (let ((buffer (plist-get mwim358-tui-baseline :buffer))
                 (window (plist-get mwim358-tui-baseline :window)))
             (unless (and (buffer-live-p buffer) (window-live-p window))
               (error "MWIM TUI selected baseline state died"))
             (select-window window)
             (set-buffer buffer)))))
      (setq errors (nreverse errors))
      (setq state
            (list
             :new-buffers
             (seq-difference
              (buffer-list) (plist-get mwim358-tui-baseline :buffers) #'eq)
             :new-processes
             (seq-difference
              (process-list) (plist-get mwim358-tui-baseline :processes) #'eq)
             :new-timers
             (delete-dups
              (append
               (seq-difference
                timer-list (plist-get mwim358-tui-baseline :timers) #'eq)
               (seq-difference
                timer-idle-list
                (plist-get mwim358-tui-baseline :idle-timers) #'eq)))
             :owned-live (mapcar #'buffer-live-p mwim358-tui-owned-buffers)
             :windows (equal (mwim358-tui-window-state)
                             (plist-get mwim358-tui-baseline :windows))
             :configuration
             (compare-window-configurations
              (current-window-configuration)
              (plist-get mwim358-tui-baseline :configuration))
             :buffer (eq (current-buffer)
                         (plist-get mwim358-tui-baseline :buffer))
             :window (eq (selected-window)
                         (plist-get mwim358-tui-baseline :window))))
      (unless (and (null errors)
                   (null (plist-get state :new-buffers))
                   (null (plist-get state :new-processes))
                   (null (plist-get state :new-timers))
                   (not (memq t (plist-get state :owned-live)))
                   (plist-get state :windows)
                   (plist-get state :configuration)
                   (plist-get state :buffer)
                   (plist-get state :window))
        (error "MWIM TUI cleanup failure: errors=%S state=%S" errors state))
      (message "MWIM-VISUAL-CLEAN ok=t errors=nil resources=nil windows=t"))))
"####;

fn wait_for(
    session: &mut TuiSession,
    timeout: Duration,
    description: &str,
    predicate: impl Fn(&[String]) -> bool,
) {
    session.read_until(timeout, |grid| predicate(grid));
    let grid = session.text_grid();
    assert!(
        predicate(&grid),
        "{} timed out waiting for {description}:\n{}",
        session.name,
        grid.join("\n")
    );
}

fn invoke(session: &mut TuiSession, command: &str, ready: &str) {
    session.send_keys("M-x");
    wait_for(session, Duration::from_secs(8), "M-x prompt", |grid| {
        grid.iter().any(|row| row.contains("M-x"))
    });
    session.send(command.as_bytes());
    session.send_keys("RET");
    wait_for(session, Duration::from_secs(20), ready, |grid| {
        grid.iter().any(|row| row.contains(ready))
    });
}

fn panic_text(payload: Box<dyn Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|value| (*value).to_owned())
        })
        .unwrap_or_else(|| "non-string panic payload".to_owned())
}

fn catch_phase<T>(label: &str, phase: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(phase))
        .map_err(|payload| format!("{label}: {}", panic_text(payload)))
}

fn both(
    pair: &mut PackageTuiPair,
    label: &str,
    operation: impl Fn(&mut TuiSession) + Copy,
) -> Result<(), String> {
    let gnu = catch_phase(&format!("GNU {label}"), || operation(&mut pair.gnu));
    let neo = catch_phase(&format!("Neo {label}"), || operation(&mut pair.neo));
    let errors = [gnu.err(), neo.err()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn exact_row(session: &TuiSession, marker: &str) -> String {
    session
        .text_grid()
        .into_iter()
        .find(|row| row.contains(marker))
        .unwrap_or_else(|| panic!("{} did not render {marker:?}", session.name))
        .trim()
        .to_owned()
}

fn record_rows(
    pair: &PackageTuiPair,
    marker: &str,
    gnu_transcript: &mut Vec<String>,
    neo_transcript: &mut Vec<String>,
) {
    gnu_transcript.push(exact_row(&pair.gnu, marker));
    neo_transcript.push(exact_row(&pair.neo, marker));
}

fn visual_grid(session: &TuiSession) -> String {
    let grid = session.text_grid();
    let (_, cols) = session.screen_size();
    [
        "alpha beta",
        "delta epsilon",
        "theta iota",
        "lambda mu",
        "wide 界",
        "maple spruce",
        "willow aspen",
    ]
    .into_iter()
    .map(|needle| {
        let row = grid
            .iter()
            .position(|contents| contents.contains(needle))
            .unwrap_or_else(|| {
                panic!(
                    "{} did not render visual row {needle:?}:\n{}",
                    session.name,
                    grid.join("\n")
                )
            }) as u16;
        session
            .screen()
            .contents_between(row, cols - 24, row, cols)
            .trim_end()
            .to_owned()
    })
    .enumerate()
    .map(|(index, row)| format!("MWIM-GRID-{} {row:?}", index + 1))
    .collect::<Vec<_>>()
    .join("\n")
}

fn run_visual_body(pair: &mut PackageTuiPair) -> (String, String) {
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

fn run_visual_cleanup(pair: &mut PackageTuiPair) -> (String, String) {
    both(pair, "cleanup", |session| {
        invoke(session, "mwim358-tui-cleanup", "MWIM-VISUAL-CLEAN")
    })
    .expect("clean both real MWIM visual sessions");
    (
        exact_row(&pair.gnu, "MWIM-VISUAL-CLEAN"),
        exact_row(&pair.neo, "MWIM-VISUAL-CLEAN"),
    )
}

#[test]
fn mwim_real_visual_and_logical_line_keys_match_gnu() {
    let oracle = CachedMelpaOracle::new(MWIM_MELPA_PIN, "mwim.el")
        .expect("prepare exact shallow MWIM source below ./tmp")
        .with_prelude(MWIM_VISUAL_TUI_PRELUDE);
    let mut pair = PackageTuiPair::spawn("mwim-visual-lines", oracle.prepared_packages())
        .expect("spawn real MWIM visual PTY pair");

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
            expect![[r#"
                MWIM-VISUAL-SETUP w=24 b=24 p=48 chars=133 begin=beginning-of-visual-line end=end-of-visual-line visual=t wrap=t
                MWIM-GRID-1 "  alpha beta gamma"
                MWIM-GRID-2 "delta epsilon zeta eta"
                MWIM-GRID-3 "theta iota kappa"
                MWIM-GRID-4 "lambda mu nu xi omicron"
                MWIM-GRID-5 "        wide 界 cedar"
                MWIM-GRID-6 "birch maple spruce"
                MWIM-GRID-7 "willow aspen oak"
                MWIM-MOVE e=1 p=43 line=1 col=42 begin=beginning-of-visual-line end=end-of-visual-line mod=nil undo=nil
                MWIM-MOVE e=2 p=43 line=1 col=42 begin=beginning-of-visual-line end=end-of-visual-line mod=nil undo=nil
                MWIM-MOVE e=3 p=43 line=1 col=42 begin=beginning-of-visual-line end=end-of-visual-line mod=nil undo=nil
                MWIM-VISUAL-RESET-MIDDLE p=48
                MWIM-MOVE e=4 p=59 line=1 col=58 begin=beginning-of-visual-line end=end-of-visual-line mod=nil undo=nil
                MWIM-MOVE e=5 p=59 line=1 col=58 begin=beginning-of-visual-line end=end-of-visual-line mod=nil undo=nil
                MWIM-MOVE e=6 p=59 line=1 col=58 begin=beginning-of-visual-line end=end-of-visual-line mod=nil undo=nil
                MWIM-VISUAL-RESET-FINAL p=80
                MWIM-MOVE e=7 p=60 line=1 col=59 begin=beginning-of-visual-line end=end-of-visual-line mod=nil undo=nil
                MWIM-MOVE e=8 p=83 line=1 col=82 begin=beginning-of-visual-line end=end-of-visual-line mod=nil undo=nil
                MWIM-WIDE-VISUAL-RESET p=109 line=2 col=33 begin=beginning-of-visual-line end=end-of-visual-line
                MWIM-MOVE e=9 p=98 line=2 col=22 begin=beginning-of-visual-line end=end-of-visual-line mod=nil undo=nil
                MWIM-WIDE-VISUAL-RESET-AGAIN p=109 line=2 col=33
                MWIM-MOVE e=10 p=116 line=2 col=40 begin=beginning-of-visual-line end=end-of-visual-line mod=nil undo=nil
                MWIM-LOGICAL-RESET p=109 line=2 col=33 begin=beginning-of-line end=end-of-line
                MWIM-MOVE e=11 p=84 line=2 col=0 begin=beginning-of-line end=end-of-line mod=nil undo=nil
                MWIM-LOGICAL-RESET-AGAIN p=109 line=2 col=33
                MWIM-MOVE e=12 p=133 line=2 col=57 begin=beginning-of-line end=end-of-line mod=nil undo=nil"#]].assert_eq(&gnu);
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
