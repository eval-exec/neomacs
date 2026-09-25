//! P3.5 E2 (`NEOMACS_CHROME_MEMO`): a replay frame that evaluates the mode
//! line installs the previous row when the evaluation renders the same.
//!
//! Every step compares the incremental frame with a fresh full layout of the
//! same state -- glyphs with their resolved faces, placement, snapshots --
//! and the presented chrome strings, which must be THIS evaluation's (their
//! `help-echo` and keymaps feed mouse targets).

use super::*;
use crate::incremental_layout::chrome_memo::{
    CHROME_MEMO_VERIFY_MISMATCHES, ChromeMemoMode, set_chrome_memo_mode_for_test,
};

struct MemoGuard;

impl MemoGuard {
    fn set(mode: ChromeMemoMode) -> Self {
        set_chrome_memo_mode_for_test(Some(mode));
        MemoGuard
    }
}

impl Drop for MemoGuard {
    fn drop(&mut self) {
        set_chrome_memo_mode_for_test(None);
    }
}

const SOURCE_LINE: &str = "(defun f (a b) (+ a b))\n";

/// The default mode line's shape: a propertized buffer name with mouse
/// targets, a line number and a counting `:eval` that returns constant text
/// (so every evaluation is observable and renders the same).
const MODE_LINE: &str = r#"(set (make-local-variable 'mode-line-format)
  '("-" (:propertize "%b" face bold help-echo "Buffer name" mouse-face highlight
                     local-map (keymap (mouse-1 . ignore)))
    " L%l "
    (:eval (progn (setq neo-evals (1+ neo-evals))
                  (propertize "E" 'help-echo neo-help)))))"#;

struct MemoFrame {
    eval: Context,
    frame_id: neovm_core::window::FrameId,
    window: neovm_core::window::WindowId,
    engine: LayoutEngine,
}

impl MemoFrame {
    /// A frame showing `lines` source lines with [`MODE_LINE`], point at
    /// the start of line `point_line` (0-based), and one accepted frame.
    fn new(lines: usize, point_line: usize) -> Self {
        let (mut eval, frame_id, buf_id, window) =
            incr_editing_frame(&SOURCE_LINE.repeat(lines), 800, 600);
        {
            let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
            buf.set_buffer_local("bidi-paragraph-direction", Value::symbol("left-to-right"));
        }
        eval.eval_str("(setq neo-evals 0 neo-help \"Buffer name\")")
            .expect("vars");
        eval.eval_str(MODE_LINE).expect("mode line");
        eval.eval_str(&format!(
            "(goto-char {})",
            point_line * SOURCE_LINE.len() + 1
        ))
        .expect("goto");
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame_id);
        Self {
            eval,
            frame_id,
            window,
            engine,
        }
    }

    /// The selected window's presented chrome strings as (id, text, every
    /// non-nil help-echo).
    fn chrome_strings(&self) -> Vec<(u64, String, String)> {
        let frame = self.eval.frame_manager().get(self.frame_id).expect("frame");
        let snapshot = frame
            .redisplay_snapshot(self.window)
            .expect("display snapshot");
        snapshot
            .chrome_strings
            .iter()
            .map(|string| {
                let value = string.value();
                let help = value
                    .as_lisp_string()
                    .map(|lisp| {
                        (0..lisp.schars())
                            .filter_map(|pos| {
                                lisp.intervals().get_property_at_char_pos(
                                    neovm_core::buffer::CharPos0::new(pos),
                                    Value::symbol("help-echo"),
                                )
                            })
                            .filter(|help| !help.is_nil())
                            .map(|help| {
                                help.as_utf8_str()
                                    .map_or_else(|| format!("{help:?}"), str::to_string)
                            })
                            .collect::<Vec<_>>()
                            .join("|")
                    })
                    .unwrap_or_default();
                (
                    string.string_id().get(),
                    value.as_utf8_str().unwrap_or_default().to_string(),
                    help,
                )
            })
            .collect()
    }

    /// Apply `form`, lay out incrementally, and assert the result equals a
    /// fresh full layout of the same state. Returns the incremental stats.
    fn step(&mut self, form: &str) -> LayoutStats {
        self.eval
            .eval_str(form)
            .unwrap_or_else(|error| panic!("{form}: {error:?}"));
        self.engine.layout_frame_rust(&mut self.eval, self.frame_id);
        let stats = self.engine.last_layout_stats().clone();
        let incremental = selected_window_layout_trace(&self.eval, &self.engine, self.frame_id);
        let incremental_strings = self.chrome_strings();
        let mut fresh = LayoutEngine::new();
        fresh.layout_frame_rust(&mut self.eval, self.frame_id);
        let reference = selected_window_layout_trace(&self.eval, &fresh, self.frame_id);
        assert_eq!(
            incremental, reference,
            "{form}: the incremental frame differs from a full layout ({stats:?})"
        );
        assert_eq!(
            incremental_strings,
            self.chrome_strings(),
            "{form}: the presented chrome strings are not this evaluation's"
        );
        stats
    }
}

#[test]
fn an_evaluated_mode_line_that_renders_the_same_comes_from_the_memo() {
    let _memo = MemoGuard::set(ChromeMemoMode::On);
    let mut frame = MemoFrame::new(30, 10);
    // `set-buffer-modified-p` marks the mode line for update (GNU
    // `bset_update_mode_line`), so each typing frame evaluates it.
    let mut hits = 0;
    for step in 0..6 {
        let stats = frame.step("(progn (insert \"x\") (set-buffer-modified-p t))");
        assert!(
            stats.reused_chrome_rows == 0 && stats.relaid_chrome_rows > 0,
            "step {step} did not evaluate the mode line ({stats:?})"
        );
        hits += stats.chrome_memo_hits;
    }
    assert!(
        hits >= 4,
        "typing with an unchanged mode line hit {hits} times"
    );
}

#[test]
fn a_mode_line_whose_text_changed_renders_again() {
    let _memo = MemoGuard::set(ChromeMemoMode::On);
    let mut frame = MemoFrame::new(30, 10);
    frame.step("(progn (insert \"x\") (set-buffer-modified-p t))");
    // A newline moves point to line 12: `L%l` changes.
    let stats = frame.step("(progn (insert \"\\n\") (set-buffer-modified-p t))");
    assert_eq!(stats.chrome_memo_hits, 0, "{stats:?}");
}

#[test]
fn a_changed_help_echo_is_presented_from_this_evaluation() {
    let _memo = MemoGuard::set(ChromeMemoMode::On);
    let mut frame = MemoFrame::new(30, 10);
    frame.step("(progn (insert \"x\") (set-buffer-modified-p t))");
    let stats = frame
        .step("(progn (setq neo-help \"Another name\") (insert \"y\") (set-buffer-modified-p t))");
    assert_eq!(
        stats.chrome_memo_hits, 1,
        "help-echo does not reach the glyphs"
    );
    assert!(
        frame
            .chrome_strings()
            .iter()
            .any(|(_, _, help)| help.contains("Another name")),
        "{:?}",
        frame.chrome_strings()
    );
}

#[test]
fn a_mode_line_with_a_display_property_is_never_memoized() {
    let _memo = MemoGuard::set(ChromeMemoMode::On);
    let mut frame = MemoFrame::new(30, 10);
    frame
        .eval
        .eval_str(
            r#"(set (make-local-variable 'mode-line-format)
                 '("-" (:propertize " " display (space :align-to 20)) "%b"))"#,
        )
        .expect("mode line");
    for _ in 0..3 {
        let stats = frame.step("(progn (insert \"x\") (set-buffer-modified-p t))");
        assert_eq!(stats.chrome_memo_hits, 0, "{stats:?}");
    }
}

#[test]
fn a_scrolled_window_renders_an_unchanged_mode_line_from_the_memo() {
    let _memo = MemoGuard::set(ChromeMemoMode::On);
    let mut frame = MemoFrame::new(200, 0);
    frame
        .eval
        .eval_str(
            "(set (make-local-variable 'mode-line-format) '(\"-\" (:propertize \"%b\" face bold)))",
        )
        .expect("mode line");
    frame.step("(set-buffer-modified-p t)");
    let mut hits = 0;
    let mut scrolls = 0;
    for _ in 0..3 {
        let stats = frame.step("(progn (scroll-up 1) (set-buffer-modified-p t))");
        hits += stats.chrome_memo_hits;
        scrolls += stats.scroll_windows;
    }
    assert!(scrolls > 0, "the steps did not scroll through a replay");
    assert!(hits > 0, "no scroll frame hit the memo");
}

#[test]
fn verify_mode_renders_every_hit_and_finds_it_equal() {
    let _memo = MemoGuard::set(ChromeMemoMode::Verify);
    let mismatches = CHROME_MEMO_VERIFY_MISMATCHES.load(std::sync::atomic::Ordering::Relaxed);
    let mut frame = MemoFrame::new(30, 10);
    let mut hits = 0;
    for step in 0..4 {
        let stats = frame.step("(progn (insert \"x\") (set-buffer-modified-p t))");
        assert!(
            stats.reused_chrome_rows == 0 && stats.relaid_chrome_rows > 0,
            "step {step} did not evaluate the mode line ({stats:?})"
        );
        hits += stats.chrome_memo_hits;
    }
    assert!(hits >= 2, "verify compared {hits} hits");
    assert_eq!(
        CHROME_MEMO_VERIFY_MISMATCHES.load(std::sync::atomic::Ordering::Relaxed),
        mismatches
    );
}

#[test]
fn the_memo_off_renders_every_evaluation() {
    let _memo = MemoGuard::set(ChromeMemoMode::Off);
    let mut frame = MemoFrame::new(30, 10);
    for _ in 0..3 {
        let stats = frame.step("(progn (insert \"x\") (set-buffer-modified-p t))");
        assert_eq!(stats.chrome_memo_hits, 0);
    }
}
