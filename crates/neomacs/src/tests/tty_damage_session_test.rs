//! P3.5 F1 in process: the real layout engine's TTY presentations for a
//! scripted editing session, rendered by the full path (`off`), the damage
//! path (`on`) and `verify`, frame by frame.
//!
//! This is the `NEOMACS_TTY_DAMAGE=verify` experiment without a terminal: the
//! TUI suite runs the same comparison against a live binary.

use super::super::frame_layout::{REDISPLAY_RUNTIME, run_tty_layout_tree};
use super::super::{Interactivity, bootstrap_buffers, bootstrap_tty_display_config};
use neomacs_display_runtime::backend::tty::rif::TtyRif;
use neomacs_display_runtime::backend::tty::rif::damage::{ScreenMatch, TtyDamageMode};
use neomacs_display_runtime::redisplay::RedisplayRuntime;
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::load::create_bootstrap_evaluator_cached_with_features;

const COLS: usize = 100;
const ROWS: usize = 36;

struct Renderers {
    full: TtyRif,
    damage: TtyRif,
    verify: TtyRif,
    frames: Vec<FrameRecord>,
    churn: Vec<String>,
}

#[derive(Debug)]
struct FrameRecord {
    label: String,
    damage_frame: bool,
    full_reason: Option<neomacs_display_runtime::backend::tty::rif::damage::TtyFullFrameReason>,
    rows_repainted: u32,
    full_bytes: usize,
    damage_bytes: usize,
}

impl Renderers {
    fn new(silent: bool) -> Self {
        let make = |mode| {
            let mut rif = TtyRif::new(COLS, ROWS);
            rif.set_damage_mode(mode);
            rif.set_silent_frames(silent);
            rif
        };
        Self {
            full: make(TtyDamageMode::Off),
            damage: make(TtyDamageMode::On),
            verify: make(TtyDamageMode::Verify),
            frames: Vec::new(),
            churn: Vec::new(),
        }
    }

    fn frame(&mut self, eval: &mut Context, label: &str) {
        let models_identical = self.damage.compare_screen(&self.full) == ScreenMatch::Identical;
        let (root, children) = run_tty_layout_tree(eval).expect("a TTY presentation");
        for rif in [&mut self.full, &mut self.damage, &mut self.verify] {
            rif.rasterize_presentations(&root, &children);
            rif.diff_and_render();
        }
        let full = self.full.take_output();
        let damage = self.damage.take_output();
        let verified = self.verify.take_output();
        let comparison = self.verify.frame_stats().verify;
        assert_eq!(comparison.frames, 1, "{label}: the frame was verified");
        assert_eq!(
            verified, full,
            "{label}: verify writes the full path's bytes"
        );
        assert_eq!(
            comparison.false_negatives, 0,
            "{label}: the damage path left a stale row"
        );
        assert_eq!(
            comparison.screen_diff_rows, 0,
            "{label}: the damage path's screen differs"
        );
        assert_ne!(
            self.damage.compare_screen(&self.full),
            ScreenMatch::Different,
            "{label}: the on and off renderers show different screens"
        );
        // Byte-identical unless the full path's own screen model churned
        // (see `damage.rs`: rows it rewrote unchanged, or erased/written
        // labels its partial-row carry reset), this frame or before.
        if models_identical
            && comparison.redundant_rewrite_rows == 0
            && comparison.materialization_diff_rows == 0
        {
            assert_eq!(
                String::from_utf8_lossy(&damage),
                String::from_utf8_lossy(&full),
                "{label}: the damage path's bytes differ"
            );
        } else {
            self.churn.push(format!("{label}: {comparison:?}"));
        }
        let stats = self.damage.frame_stats();
        self.frames.push(FrameRecord {
            label: label.to_string(),
            damage_frame: stats.damage_frame,
            full_reason: stats.full_reason,
            rows_repainted: stats.rows_repainted,
            full_bytes: full.len(),
            damage_bytes: damage.len(),
        });
    }

    fn step(&mut self, eval: &mut Context, label: &str, lisp: &str) {
        eval.eval_str(lisp)
            .unwrap_or_else(|error| panic!("{label}: {lisp}: {error:?}"));
        self.frame(eval, label);
    }

    fn record(&self, label: &str) -> &FrameRecord {
        self.frames
            .iter()
            .rev()
            .find(|record| record.label == label)
            .unwrap_or_else(|| panic!("no frame {label}"))
    }
}

const SOURCE: &str = r#"(progn
  (switch-to-buffer (get-buffer-create "damage.el"))
  (erase-buffer)
  (dotimes (i 60)
    (insert (format ";;; Section %d -- a comment\n(defun damage-fn-%d (arg)\n\t\"Doc string %d.\"\n\t(let ((x (* arg %d)))\n\t  (when (> x 10)\n\t    (message \"big %%d\" x))\n\t  x))\n\n" i i i i)))
  (emacs-lisp-mode)
  (font-lock-ensure)
  (goto-char (point-min))
  (forward-line 3)
  (end-of-line))"#;

fn session(silent: bool) -> Renderers {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(
        &mut eval,
        COLS as u32,
        ROWS as u32,
        bootstrap_tty_display_config(Interactivity::Interactive),
    );
    REDISPLAY_RUNTIME.with(RedisplayRuntime::disable_cosmic_metrics);
    let mut r = Renderers::new(silent);
    r.frame(&mut eval, "startup");
    r.step(&mut eval, "load source", SOURCE);
    r.frame(&mut eval, "idle");
    r.frame(&mut eval, "idle again");
    r.step(&mut eval, "type", "(insert \"x\")");
    r.step(&mut eval, "type again", "(insert \"y\")");
    r.step(&mut eval, "delete", "(delete-char -1)");
    r.step(&mut eval, "cursor left", "(backward-char 3)");
    r.step(&mut eval, "cursor down", "(forward-line 1)");
    r.step(
        &mut eval,
        "type in a tab line",
        "(progn (back-to-indentation) (insert \"z\"))",
    );
    r.step(&mut eval, "message", "(message \"hello from verify\")");
    r.frame(&mut eval, "idle with message");
    r.step(&mut eval, "clear message", "(message nil)");
    r.step(&mut eval, "newline", "(progn (end-of-line) (newline))");
    r.step(&mut eval, "scroll", "(scroll-up 3)");
    r.step(&mut eval, "scroll back", "(scroll-down 1)");
    r.step(&mut eval, "split below", "(split-window-below)");
    r.step(&mut eval, "other window", "(other-window 1)");
    r.step(&mut eval, "type in other window", "(insert \"w\")");
    r.step(&mut eval, "split right", "(split-window-right)");
    r.step(&mut eval, "type in right window", "(insert \"v\")");
    r.step(&mut eval, "one window", "(delete-other-windows)");
    r.step(&mut eval, "line numbers", "(display-line-numbers-mode 1)");
    r.step(&mut eval, "type with line numbers", "(insert \"n\")");
    r.step(&mut eval, "move with line numbers", "(forward-line 2)");
    r.step(
        &mut eval,
        "no line numbers",
        "(display-line-numbers-mode -1)",
    );
    r.step(
        &mut eval,
        "region",
        "(progn (transient-mark-mode 1) (set-mark (point)) (forward-char 6))",
    );
    r.step(&mut eval, "region grows", "(forward-line 1)");
    r.step(&mut eval, "region goes", "(deactivate-mark)");
    r.step(&mut eval, "tab line", "(global-tab-line-mode 1)");
    r.step(&mut eval, "type with tab line", "(insert \"t\")");
    r.step(&mut eval, "no tab line", "(global-tab-line-mode -1)");
    r.step(&mut eval, "end of buffer", "(goto-char (point-max))");
    r.step(&mut eval, "type at end", "(insert \"end\")");
    r.step(&mut eval, "erase", "(erase-buffer)");
    r.frame(&mut eval, "idle at the end");
    r
}

#[test]
fn tty_damage_path_matches_the_full_path_over_a_real_session() {
    let r = session(false);
    let totals = r.verify.damage_verify_totals();
    assert_eq!(totals.false_negatives, 0);
    assert_eq!(totals.screen_diff_rows, 0);
    // The damage path engages wherever layout reuses rows: idle frames,
    // cursor motion, echo-area messages, a moving region. (Typing here makes
    // layout relay every row below the edit -- the source has tabs, the P3.5
    // stage-G edit-reuse cliff -- so those frames are wide damage.)
    for label in [
        "idle",
        "idle again",
        "cursor left",
        "cursor down",
        "message",
        "idle with message",
        "clear message",
        "region grows",
        "idle at the end",
    ] {
        let record = r.record(label);
        assert!(record.damage_frame, "{record:?}");
        assert!(record.rows_repainted <= 5, "{record:?}");
    }
    let print: Vec<String> = r
        .frames
        .iter()
        .map(|f| {
            format!(
                "{:<26} {:<6} {:<18} rows={:<3} bytes full={} damage={}",
                f.label,
                if f.damage_frame { "damage" } else { "full" },
                format!("{:?}", f.full_reason),
                f.rows_repainted,
                f.full_bytes,
                f.damage_bytes
            )
        })
        .collect();
    tracing::info!(
        "tty damage session:\n{}\nfull-path churn frames: {:#?}",
        print.join("\n"),
        r.churn
    );
}

#[test]
fn tty_damage_path_with_silent_frames_matches_over_a_real_session() {
    let r = session(true);
    let totals = r.verify.damage_verify_totals();
    assert_eq!(totals.false_negatives, 0);
    assert_eq!(totals.screen_diff_rows, 0);
    let idle = r.record("idle");
    assert_eq!(idle.damage_bytes, 0, "a silent idle frame writes nothing");
}
