//! P3.5 G2 (gate F3) in process: typing in a tab-indented Lisp buffer under
//! `NEOMACS_LAYOUT_EDIT_SYNC=sync`, through the real bootstrap evaluator
//! (emacs-lisp-mode, font-lock, jit-lock) and the real TTY renderer.
//!
//! Every frame of the incremental session is compared with a full layout of
//! the same state by a fresh redisplay runtime, rendered on its own TTY
//! model: the screens must be identical. Frames that the edit sync is meant
//! to make cheap -- a keystroke in a line with a tab, a CJK character or a
//! `display` string, and a typed newline -- must walk at most two rows of
//! the edited window (the mini-window stands still here).
//!
//! The knobs are read once per process; nextest runs each test in its own.

use super::super::frame_layout::{REDISPLAY_RUNTIME, current_layout_frame_id, run_tty_layout_tree};
use super::super::{Interactivity, bootstrap_buffers, bootstrap_tty_display_config};
use neomacs_display_runtime::backend::tty::rif::TtyRif;
use neomacs_display_runtime::backend::tty::rif::damage::{ScreenMatch, TtyDamageMode};
use neomacs_display_runtime::redisplay::{FrameLayoutPurpose, RedisplayRuntime};
use neomacs_layout_engine::incremental_layout::LayoutStats;
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::load::create_bootstrap_evaluator_cached_with_features;

const COLS: usize = 100;
const ROWS: usize = 36;

const SOURCE: &str = r#"(progn
  (switch-to-buffer (get-buffer-create "sync.el"))
  (erase-buffer)
  (dotimes (i 60)
    (insert (format ";;; Section %d -- a comment\n(defun sync-fn-%d (arg)\n\t\"Doc string %d.\"\n\t(let ((x (* arg %d)))\n\t  (when (> x 10)\n\t    (message \"big %%d\" x))\n\t  x))\n\n" i i i i)))
  (emacs-lisp-mode)
  (font-lock-ensure)
  (goto-char (point-min))
  (forward-line 11)
  (end-of-line))"#;

struct Session {
    eval: Context,
    incremental: TtyRif,
    damage: TtyRif,
}

impl Session {
    fn new() -> Self {
        neovm_core::logging::init_for_tests();
        // SAFETY: nextest runs every test in its own process, and nothing has
        // read these knobs yet.
        unsafe {
            std::env::set_var("NEOMACS_LAYOUT_EDIT_SYNC", "sync");
            std::env::set_var("NEOMACS_LAYOUT_MINI_STILL", "on");
            std::env::set_var("NEOMACS_MODE_LINE_GATE", "gnu");
            std::env::set_var("NEOMACS_LAYOUT_SCROLL_BACK", "on");
        }
        let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
            .expect("cached bootstrap evaluator");
        let _bootstrap = bootstrap_buffers(
            &mut eval,
            COLS as u32,
            ROWS as u32,
            bootstrap_tty_display_config(Interactivity::Interactive),
        );
        REDISPLAY_RUNTIME.with(RedisplayRuntime::disable_cosmic_metrics);
        let mut damage = TtyRif::new(COLS, ROWS);
        damage.set_damage_mode(TtyDamageMode::Verify);
        let mut session = Self {
            eval,
            incremental: TtyRif::new(COLS, ROWS),
            damage,
        };
        session.frame("startup");
        session
    }

    /// Lay out one frame incrementally, render it, and compare the screen
    /// with a fresh runtime's full layout of the same state.
    fn frame(&mut self, label: &str) -> LayoutStats {
        let (root, children) = run_tty_layout_tree(&mut self.eval).expect("a TTY presentation");
        let stats = REDISPLAY_RUNTIME.with(RedisplayRuntime::last_layout_stats);
        for rif in [&mut self.incremental, &mut self.damage] {
            rif.rasterize_presentations(&root, &children);
            rif.diff_and_render();
            let _ = rif.take_output();
        }
        let verify = self.damage.frame_stats().verify;
        assert_eq!(
            verify.false_negatives, 0,
            "{label}: the TTY damage path left a stale row"
        );
        assert_eq!(
            verify.screen_diff_rows, 0,
            "{label}: the TTY damage path's screen differs"
        );

        let reference_runtime = RedisplayRuntime::new_without_font_metrics();
        reference_runtime.disable_cosmic_metrics();
        let frame_id = current_layout_frame_id(&self.eval).expect("a selected frame");
        let reference = reference_runtime
            .prepare_frame(&mut self.eval, frame_id, FrameLayoutPurpose::Redisplay)
            .expect("a full layout")
            .activate(&mut self.eval)
            .expect("an active presentation");
        let mut reference_rif = TtyRif::new(COLS, ROWS);
        reference_rif.rasterize_presentations(&reference, &[]);
        reference_rif.diff_and_render();
        assert_ne!(
            self.incremental.compare_screen(&reference_rif),
            ScreenMatch::Different,
            "{label}: the incremental screen differs from a full layout's\nincremental:\n{}\nfull:\n{}",
            self.incremental.dump_desired().join("\n"),
            reference_rif.dump_desired().join("\n"),
        );
        stats
    }

    fn step(&mut self, label: &str, lisp: &str) -> LayoutStats {
        self.eval
            .eval_str(lisp)
            .unwrap_or_else(|error| panic!("{label}: {lisp}: {error:?}"));
        self.frame(label)
    }
}

#[test]
fn edit_sync_keystrokes_walk_only_the_edited_rows_and_match_a_full_layout() {
    let mut s = Session::new();
    s.step("load source", SOURCE);
    s.frame("idle");
    // Line 12 is a tab-indented line of the defun.
    // (label, form, whether the frame must walk at most two rows)
    let cases = [
        ("type in a tab line", "(insert \"x\")", true),
        ("type again", "(insert \"y\")", true),
        ("delete in a tab line", "(delete-char -1)", true),
        ("type CJK", "(insert \"字\")", true),
        (
            "add a display string",
            "(progn (insert \" ;; word\") (put-text-property (- (point) 4) (point) 'display \"DISPLAYED\"))",
            false,
        ),
        ("type after a display string", "(insert \"z\")", true),
        ("newline", "(newline)", true),
        ("type on the new line", "(insert \"q\")", true),
        ("delete on the new line", "(delete-char -1)", true),
        // Joining moves the rows below UP, which needs a walk at the window
        // bottom the sync does not do yet: exact, but not cheap.
        ("join the lines", "(delete-char -1)", false),
    ];
    let mut report = Vec::new();
    for (label, lisp, cheap) in cases {
        let stats = s.step(label, lisp);
        report.push(format!(
            "{label:<28} edit={} full={} relaid={} reused={} shifted={}",
            stats.edit_windows,
            stats.full_windows,
            stats.relaid_body_rows,
            stats.reused_rows,
            stats.reused_shifted_rows
        ));
        if cheap {
            assert_eq!(stats.edit_windows, 1, "{label}\n{}", report.join("\n"));
            assert!(
                stats.relaid_body_rows <= 2,
                "{label}: F3 wants at most 2 rows walked\n{}",
                report.join("\n")
            );
        }
        if label == "newline" {
            assert!(
                stats.reused_shifted_rows > 0,
                "{label}: the rows below moved down\n{}",
                report.join("\n")
            );
        }
    }
    tracing::info!("edit sync session:\n{}", report.join("\n"));
}

/// P3.5 G3 in process: scrolling back walks only the exposed rows and
/// reuses the rest shifted down, exactly as a full layout draws them.
#[test]
fn scrolling_back_walks_only_the_exposed_rows_and_matches_a_full_layout() {
    let mut s = Session::new();
    s.step("load source", SOURCE);
    s.step("scroll forward", "(scroll-up 12)");
    let mut report = Vec::new();
    for (label, lisp, exposed) in [
        ("scroll back one line", "(scroll-down 1)", 1),
        ("scroll back three lines", "(scroll-down 3)", 3),
        ("scroll forward again", "(scroll-up 2)", 2),
        ("scroll back two lines", "(scroll-down 2)", 2),
    ] {
        let stats = s.step(label, lisp);
        report.push(format!(
            "{label:<24} scroll={} full={} relaid={} shifted={}",
            stats.scroll_windows,
            stats.full_windows,
            stats.relaid_body_rows,
            stats.reused_shifted_rows
        ));
        assert_eq!(stats.scroll_windows, 1, "{label}\n{}", report.join("\n"));
        assert!(
            stats.relaid_body_rows <= exposed,
            "{label}: only the exposed rows are walked\n{}",
            report.join("\n")
        );
    }
    tracing::info!("scroll-back session:\n{}", report.join("\n"));
}
