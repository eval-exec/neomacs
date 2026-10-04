//! P3.5 G2 (`NEOMACS_LAYOUT_EDIT_SYNC=sync`): an edit replay synchronizes
//! its walk with the first unchanged row below the edit (GNU
//! `try_window_id`) instead of proving ahead that each changed line stays one
//! row. Every case here compares the incremental frame with a fresh full
//! layout of the same state -- glyphs, placement, positions, snapshots,
//! cursor and the published window end -- and checks how many rows were
//! walked (gate F3: at most 2 for a tab, CJK, `display` or newline edit).

use super::*;

#[cfg(test)]
#[path = "edit_sync_line_number_replay_test.rs"]
mod line_number_replay;

#[cfg(test)]
#[path = "edit_sync_join_budget_test.rs"]
mod join_budget;
use crate::incremental_layout::edit_sync::{
    EditSyncMode, set_edit_sync_mode_for_test, set_prove_first_for_test, set_shift_skip_for_test,
};

struct SyncGuard;

impl SyncGuard {
    fn set(mode: EditSyncMode) -> Self {
        set_edit_sync_mode_for_test(Some(mode));
        set_prove_first_for_test(Some(false));
        SyncGuard
    }
}

impl Drop for SyncGuard {
    fn drop(&mut self) {
        set_edit_sync_mode_for_test(None);
        set_prove_first_for_test(None);
    }
}

struct ShiftSkipGuard;

impl ShiftSkipGuard {
    fn on() -> Self {
        set_shift_skip_for_test(Some(true));
        Self
    }
}

impl Drop for ShiftSkipGuard {
    fn drop(&mut self) {
        set_shift_skip_for_test(None);
    }
}

/// Include currentness and the matrix row, not just the two Z offsets in
/// BackendLayoutTrace, when comparing incremental and full window ends.
fn selected_window_end(
    eval: &Context,
    frame_id: neovm_core::window::FrameId,
) -> neovm_core::window::WindowEndState {
    let frame = eval.frame_manager().get(frame_id).expect("frame");
    match frame
        .find_window(frame.selected_window)
        .expect("selected window")
    {
        neovm_core::window::Window::Leaf { window_end, .. } => *window_end,
        other => panic!("expected leaf window, got {other:?}"),
    }
}

/// Tab-indented Lisp, the P3.5 `typemid` shape: every other line starts with
/// a tab.
fn tabbed_source(lines: usize) -> String {
    (0..lines)
        .map(|i| {
            if i % 2 == 0 {
                format!("(defun f{i} (a b)\n")
            } else {
                format!("\t(+ a b {i}))\n")
            }
        })
        .collect()
}

struct SyncFrame {
    eval: Context,
    frame_id: neovm_core::window::FrameId,
    engine: LayoutEngine,
}

impl SyncFrame {
    /// A frame showing `text` with a fixed paragraph direction (GNU gives up
    /// `try_window_id` under an automatic one), point at `point_char`
    /// (0-based), and one accepted frame.
    fn new(text: &str, point_char: usize) -> Self {
        let (mut eval, frame_id, buf_id, _window) = incr_editing_frame(text, 800, 600);
        {
            let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
            buf.set_buffer_local("bidi-paragraph-direction", Value::symbol("left-to-right"));
        }
        eval.eval_str(&format!("(goto-char {})", point_char + 1))
            .expect("goto");
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame_id);
        Self {
            eval,
            frame_id,
            engine,
        }
    }

    /// Body rows of the selected window this frame laid out (not reused).
    fn main_relaid(&self) -> usize {
        let win = self
            .eval
            .frame_manager()
            .get(self.frame_id)
            .expect("frame")
            .selected_window;
        enabled_body_row_damage(&self.engine, win)
            .iter()
            .filter(|(_, damage)| damage.is_relaid())
            .count()
    }

    /// Apply `form`, lay out incrementally, and assert the result equals a
    /// fresh full layout of the same state. Returns the incremental stats.
    fn step(&mut self, form: &str) -> LayoutStats {
        self.eval.eval_str(form).unwrap_or_else(|error| {
            panic!("{form}: {error:?}");
        });
        self.engine.layout_frame_rust(&mut self.eval, self.frame_id);
        let stats = self.engine.last_layout_stats().clone();
        let incremental = selected_window_layout_trace(&self.eval, &self.engine, self.frame_id);
        let incremental_indices = enabled_row_indices(&self.eval, &self.engine, self.frame_id);
        let incremental_end = selected_window_end(&self.eval, self.frame_id);
        let mut fresh = LayoutEngine::new();
        fresh.layout_frame_rust(&mut self.eval, self.frame_id);
        let reference = selected_window_layout_trace(&self.eval, &fresh, self.frame_id);
        assert!(
            incremental_end.is_current(),
            "{form}: incremental end is stale"
        );
        assert_eq!(
            incremental_end,
            selected_window_end(&self.eval, self.frame_id),
            "{form}: complete window-end metadata differs ({stats:?})"
        );
        // The trace lists enabled rows in order but not WHERE they sit in the
        // matrix; a replay that leaves a gap would pass it.
        assert_eq!(
            incremental_indices,
            enabled_row_indices(&self.eval, &fresh, self.frame_id),
            "{form}: enabled matrix rows sit at different indices ({stats:?})"
        );
        assert_eq!(
            incremental, reference,
            "{form}: the incremental frame differs from a full layout ({stats:?})"
        );
        stats
    }
}

/// Matrix indices of the selected window's enabled rows.
fn enabled_row_indices(
    eval: &Context,
    engine: &LayoutEngine,
    frame_id: neovm_core::window::FrameId,
) -> Vec<usize> {
    let selected = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    engine
        .last_frame_display_state
        .as_ref()
        .expect("display state")
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected.0 as i64)
        .expect("selected window matrix")
        .matrix
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.enabled)
        .map(|(index, _)| index)
        .collect()
}

/// 0-based char position of the end of line `line` (0-based) in `text`.
fn end_of_line(text: &str, line: usize) -> usize {
    text.split_inclusive('\n')
        .take(line + 1)
        .map(|l| l.chars().count())
        .sum::<usize>()
        - 1
}

#[test]
fn typing_in_a_tab_indented_line_walks_only_its_rows() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let text = tabbed_source(80);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
    for form in ["(insert \"x\")", "(delete-region (1- (point)) (point))"] {
        let stats = frame.step(form);
        assert_eq!(stats.edit_windows, 1, "{form}: {stats:?}");
        assert!(frame.main_relaid() <= 2, "{form}: {stats:?}");
    }
}

#[test]
fn the_prove_path_still_relays_every_row_below_a_tab_line() {
    // The cliff this mode removes, pinned so the comparison stays honest.
    let _sync = SyncGuard::set(EditSyncMode::Prove);
    let text = tabbed_source(80);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
    let stats = frame.step("(insert \"x\")");
    assert!(frame.main_relaid() > 10, "{stats:?}");
}

#[test]
fn a_jit_lock_line_mark_walks_the_line_and_the_next() {
    // jit-lock marks [bol, next bol) `fontified nil` after each keystroke;
    // like GNU, the row after the line is walked too (its newline's
    // predecessor changed), and nothing else.
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let text = tabbed_source(80);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
    let stats = frame.step(
        "(progn (insert \"x\") \
                (put-text-property (line-beginning-position) (line-beginning-position 2) \
                                   'fontified nil))",
    );
    assert_eq!(stats.edit_windows, 1, "{stats:?}");
    assert!(frame.main_relaid() <= 2, "{stats:?}");
}

#[test]
fn typing_in_a_line_with_cjk_text_walks_only_its_rows() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let mut text = tabbed_source(80);
    let at = end_of_line(&text, 15);
    let byte_at = text
        .char_indices()
        .nth(at)
        .map(|(b, _)| b)
        .expect("line end");
    text.insert_str(byte_at, " ;; 中文字符");
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
    for form in [
        "(insert \"x\")",
        "(insert \"字\")",
        "(delete-region (- (point) 2) (point))",
    ] {
        let stats = frame.step(form);
        assert_eq!(stats.edit_windows, 1, "{form}: {stats:?}");
        assert!(frame.main_relaid() <= 2, "{form}: {stats:?}");
    }
}

#[test]
fn typing_in_a_line_with_a_display_string_walks_only_its_rows() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let text = tabbed_source(80);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
    frame.step(
        "(progn (insert \" ;; word\") \
                (put-text-property (- (point) 4) (point) 'display \"DISPLAYED\"))",
    );
    for form in ["(insert \"x\")", "(delete-region (1- (point)) (point))"] {
        let stats = frame.step(form);
        assert_eq!(stats.edit_windows, 1, "{form}: {stats:?}");
        assert!(frame.main_relaid() <= 2, "{form}: {stats:?}");
    }
}

#[test]
fn an_inserted_newline_shifts_the_rows_below_instead_of_walking_them() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let text = tabbed_source(80);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15) - 3);
    let stats = frame.step("(insert \"\\n\")");
    assert_eq!(stats.edit_windows, 1, "{stats:?}");
    assert!(frame.main_relaid() <= 2, "{stats:?}");
    assert!(stats.reused_shifted_rows > 0, "{stats:?}");
    let win = frame
        .eval
        .frame_manager()
        .get(frame.frame_id)
        .expect("frame")
        .selected_window;
    let damage = enabled_body_row_damage(&frame.engine, win);
    assert!(
        damage.iter().any(|(_, damage)| matches!(
            damage,
            RowDamage::ReusedShifted { dvpos } if dvpos.get() > 0.0
        )),
        "the rows below moved down: {damage:?}"
    );
}

#[test]
fn several_inserted_lines_and_a_wrapping_insert_stay_exact() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let text = tabbed_source(80);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 10));
    frame.step("(insert \"\\n\\t(one)\\n\\t(two)\\n\")");
    frame.step("(insert (make-string 150 ?w))");
}

#[test]
fn a_joined_line_falls_back_to_walking_the_rows_below() {
    // Rows moving UP need a walk at the window bottom, which the sync path
    // does not do yet: the walk runs on and stays exact.
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let text = tabbed_source(80);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
    let stats = frame.step("(delete-region (point) (1+ (point)))");
    assert_eq!(stats.edit_windows, 1, "{stats:?}");
}

#[test]
fn a_newline_that_pushes_point_off_the_window_falls_back_to_a_full_layout() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let text = tabbed_source(80);
    // Find the last visible row's line from a first frame.
    let mut frame = SyncFrame::new(&text, 0);
    let window_end = frame
        .eval
        .eval_str("(window-end nil t)")
        .expect("window end")
        .as_fixnum()
        .expect("fixnum") as usize;
    let last_line = text
        .chars()
        .take(window_end - 1)
        .filter(|&c| c == '\n')
        .count();
    let point = end_of_line(&text, last_line.saturating_sub(2));
    frame.step(&format!("(goto-char {})", point + 1));
    frame.step("(save-excursion (goto-char (point-min)) (forward-line 5) (insert \"\\n\\n\\n\"))");
}

#[test]
fn typing_on_the_first_visible_line_and_at_the_end_of_the_buffer_stay_exact() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let text = tabbed_source(12);
    let mut frame = SyncFrame::new(&text, 3);
    let stats = frame.step("(insert \"\\t\")");
    assert!(frame.main_relaid() <= 2, "{stats:?}");
    frame.step("(goto-char (point-max))");
    for form in [
        "(insert \"x\")",
        "(insert \"\\n\")",
        "(delete-region (- (point) 2) (point))",
    ] {
        frame.step(form);
    }
}

/// A small deterministic generator (the tests must not depend on a crate
/// feature for randomness).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// Random edits over the visible region, each checked against a full layout.
#[test]
fn random_edits_match_a_full_layout_after_every_step() {
    random_edits_in_mode(EditSyncMode::Sync);
}

#[test]
fn random_edits_match_a_full_layout_after_every_step_on_the_prove_path() {
    random_edits_in_mode(EditSyncMode::Prove);
}

fn random_edits_in_mode(mode: EditSyncMode) {
    let _sync = SyncGuard::set(mode);
    for seed in [1u64, 7, 42, 1234] {
        let mut rng = Lcg(seed);
        let text = tabbed_source(60);
        let mut frame = SyncFrame::new(&text, end_of_line(&text, 8));
        for step in 0..40 {
            let at = 1 + rng.below(600);
            let form = match rng.below(9) {
                0 => format!("(progn (goto-char (min (point-max) {at})) (insert \"x\"))"),
                1 => format!("(progn (goto-char (min (point-max) {at})) (insert \"\\t\"))"),
                2 => format!("(progn (goto-char (min (point-max) {at})) (insert \"中\"))"),
                3 => format!("(progn (goto-char (min (point-max) {at})) (insert \"\\n\"))"),
                4 => {
                    format!("(progn (goto-char (min (point-max) {at})) (insert \"ab\\n\\tcd\\n\"))")
                }
                5 => format!(
                    "(let ((p (min (1- (point-max)) {at}))) \
                       (goto-char p) (delete-region p (min (point-max) (+ p {}))))",
                    1 + rng.below(3)
                ),
                6 => format!(
                    "(let ((p (min (- (point-max) 3) {at}))) \
                       (put-text-property p (+ p 2) 'display \"<D>\"))"
                ),
                7 => format!(
                    "(let ((p (min (- (point-max) 6) {at}))) \
                       (put-text-property p (+ p 5) 'face 'bold))"
                ),
                _ => format!(
                    "(let ((p (min (1- (point-max)) {at}))) \
                       (goto-char p) \
                       (put-text-property (line-beginning-position) \
                                          (min (point-max) (line-beginning-position 2)) \
                                          'fontified nil) \
                       (insert \"y\"))"
                ),
            };
            let stats = frame.step(&form);
            assert!(
                stats.total_windows() > 0,
                "seed {seed} step {step}: {form}: {stats:?}"
            );
        }
    }
}

#[test]
fn zero_dy_shift_skip_preserves_the_complete_incremental_frame() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let _shift_skip = ShiftSkipGuard::on();
    let text = tabbed_source(80);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
    for form in ["(insert \"x\")", "(delete-region (1- (point)) (point))"] {
        let stats = frame.step(form);
        assert_eq!(stats.edit_windows, 1, "{form}: {stats:?}");
        assert!(frame.main_relaid() <= 2, "{form}: {stats:?}");
        assert_eq!(stats.reused_shifted_rows, 0, "{form}: {stats:?}");
    }
}

#[test]
fn nonzero_dy_shift_skip_preserves_shifted_row_provenance() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let _shift_skip = ShiftSkipGuard::on();
    let text = tabbed_source(80);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15) - 3);
    let stats = frame.step("(insert \"\\n\")");
    assert_eq!(stats.edit_windows, 1, "{stats:?}");
    assert!(frame.main_relaid() <= 2, "{stats:?}");
    assert!(stats.reused_shifted_rows > 0, "{stats:?}");
    let win = frame
        .eval
        .frame_manager()
        .get(frame.frame_id)
        .expect("frame")
        .selected_window;
    let damage = enabled_body_row_damage(&frame.engine, win);
    assert!(
        damage.iter().any(|(_, damage)| matches!(
            damage,
            RowDamage::ReusedShifted { dvpos } if dvpos.get() > 0.0
        )),
        "the rows below moved down: {damage:?}"
    );
}

#[test]
fn shift_skip_preserves_eob_and_window_end_metadata() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let _shift_skip = ShiftSkipGuard::on();
    // The suffix reaches the actual accessible end, so a synchronized frame
    // must retain the EOB row and publish the full-layout char/byte/index end.
    let text = tabbed_source(12);
    let mut frame = SyncFrame::new(&text, 3);
    frame.step("(insert \"x\")");
    frame.step("(goto-char (point-max))");
    for form in [
        "(insert \"中\")",
        "(insert \"\\n\")",
        "(delete-region (- (point) 2) (point))",
    ] {
        frame.step(form);
    }
}

#[test]
fn random_edits_with_shift_skip_match_a_full_layout_after_every_step() {
    let _shift_skip = ShiftSkipGuard::on();
    random_edits_in_mode(EditSyncMode::Sync);
}

struct SourceBudgetGuard;

impl SourceBudgetGuard {
    fn set(enabled: bool) -> Self {
        crate::buffer_source::window_source::set_sync_source_budget_for_test(Some(enabled));
        Self
    }
}

impl Drop for SourceBudgetGuard {
    fn drop(&mut self) {
        crate::buffer_source::window_source::set_sync_source_budget_for_test(None);
    }
}

fn source_budget_window_end(frame: &SyncFrame) -> neovm_core::window::WindowEndState {
    let live_frame = frame
        .eval
        .frame_manager()
        .get(frame.frame_id)
        .expect("frame");
    match live_frame
        .find_window(live_frame.selected_window)
        .expect("selected window")
    {
        neovm_core::window::Window::Leaf { window_end, .. } => *window_end,
        other => panic!("expected leaf window, got {other:?}"),
    }
}

fn source_budget_step(frame: &mut SyncFrame, form: &str) {
    frame
        .eval
        .eval_str(form)
        .unwrap_or_else(|error| panic!("{form}: {error:?}"));
    frame
        .engine
        .layout_frame_rust(&mut frame.eval, frame.frame_id);
    let incremental = selected_window_layout_trace(&frame.eval, &frame.engine, frame.frame_id);
    let incremental_indices = enabled_row_indices(&frame.eval, &frame.engine, frame.frame_id);
    let incremental_end = source_budget_window_end(frame);
    assert!(incremental_end.is_current(), "{form}: window end is stale");
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(&mut frame.eval, frame.frame_id);
    assert_eq!(
        incremental,
        selected_window_layout_trace(&frame.eval, &fresh, frame.frame_id),
        "{form}"
    );
    assert_eq!(
        incremental_indices,
        enabled_row_indices(&frame.eval, &fresh, frame.frame_id),
        "{form}"
    );
    assert_eq!(
        incremental_end,
        source_budget_window_end(frame),
        "{form}: complete window-end metadata"
    );
}

#[test]
fn source_budget_sync_edits_and_eob_match_complete_full_layouts() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let _budget = SourceBudgetGuard::set(true);
    let text = tabbed_source(80);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
    for form in [
        "(insert \"x\")",
        "(insert \"中\")",
        "(insert \"\\n\")",
        "(delete-region (- (point) 2) (point))",
        "(put-text-property (line-beginning-position) (line-beginning-position 2) 'fontified nil)",
    ] {
        source_budget_step(&mut frame, form);
    }
    let short = tabbed_source(12);
    let mut frame = SyncFrame::new(&short, 3);
    for form in [
        "(insert \"x\")",
        "(goto-char (point-max))",
        "(insert \"中\\n\")",
    ] {
        source_budget_step(&mut frame, form);
    }
}

#[test]
fn random_edits_with_source_budget_match_a_full_layout_after_every_step() {
    let _budget = SourceBudgetGuard::set(true);
    random_edits_in_mode(EditSyncMode::Sync);
}

#[test]
fn source_budget_retry_does_not_repeat_unmarking_fontification_callbacks() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    fn run(enabled: bool) -> (i64, u64) {
        let _budget = SourceBudgetGuard::set(enabled);
        crate::buffer_source::window_source::reset_sync_source_budget_retries_for_test();
        let text = tabbed_source(80);
        let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
        frame
            .eval
            .eval_str(
                "(progn (setq sync-budget-fontify-count 0) \
                    (setq fontification-functions \
                          (list (lambda (_position) \
                                  (setq sync-budget-fontify-count \
                                        (1+ sync-budget-fontify-count))))) \
                    (put-text-property (point) (1+ (point)) 'invisible t))",
            )
            .unwrap();
        frame
            .engine
            .layout_frame_rust(&mut frame.eval, frame.frame_id);
        let count = frame
            .eval
            .eval_str("sync-budget-fontify-count")
            .unwrap()
            .as_fixnum()
            .unwrap();
        let incremental = selected_window_layout_trace(&frame.eval, &frame.engine, frame.frame_id);
        let incremental_indices = enabled_row_indices(&frame.eval, &frame.engine, frame.frame_id);
        let incremental_end = source_budget_window_end(&frame);
        let retries = crate::buffer_source::window_source::sync_source_budget_retries_for_test();
        // Reference geometry must not add a second preparation's callback
        // counts to the physical-attempt count captured above.
        frame
            .eval
            .eval_str("(setq fontification-functions nil)")
            .unwrap();
        let mut fresh = LayoutEngine::new();
        fresh.layout_frame_rust(&mut frame.eval, frame.frame_id);
        assert_eq!(
            incremental,
            selected_window_layout_trace(&frame.eval, &fresh, frame.frame_id)
        );
        assert_eq!(
            incremental_indices,
            enabled_row_indices(&frame.eval, &fresh, frame.frame_id)
        );
        assert!(incremental_end.is_current());
        assert_eq!(incremental_end, source_budget_window_end(&frame));
        (count, retries)
    }
    let (without_budget, off_retries) = run(false);
    let (with_budget, on_retries) = run(true);
    assert!(
        without_budget > 0,
        "the unmarking callback must be exercised"
    );
    assert_eq!(off_retries, 0);
    assert_eq!(
        on_retries, 1,
        "the artificial horizon must exercise its local retry"
    );
    assert_eq!(with_budget, without_budget);
}

#[test]
fn source_budget_retry_restores_remapped_background_frame_artifacts() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let _budget = SourceBudgetGuard::set(true);
    let text = tabbed_source(80);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
    frame
        .eval
        .buffer_manager_mut()
        .current_buffer_mut()
        .expect("buffer")
        .set_buffer_local(
            "face-remapping-alist",
            Value::list(vec![Value::list(vec![
                Value::symbol("default"),
                Value::list(vec![Value::keyword("background"), Value::string("#245678")]),
                Value::symbol("default"),
            ])]),
        );
    frame
        .engine
        .layout_frame_rust(&mut frame.eval, frame.frame_id);
    crate::buffer_source::window_source::reset_sync_source_budget_retries_for_test();
    // Hiding a source newline without deleting it moves the rendered suffix
    // upward. This unknown/non-pure damage must still exhaust the cap and
    // restore its unpublished remapped face fill before the exact retry.
    frame
        .eval
        .eval_str("(put-text-property (point) (1+ (point)) 'invisible t)")
        .unwrap();
    frame
        .engine
        .layout_frame_rust(&mut frame.eval, frame.frame_id);
    assert_eq!(
        crate::buffer_source::window_source::sync_source_budget_retries_for_test(),
        1
    );
    let incremental = frame
        .engine
        .last_frame_display_state
        .as_ref()
        .expect("incremental frame")
        .clone();
    let selected = frame
        .eval
        .frame_manager()
        .get(frame.frame_id)
        .unwrap()
        .selected_window;
    let fills = incremental
        .face_fills
        .iter()
        .filter(|fill| fill.window_id.get() == selected.0 as i64)
        .collect::<Vec<_>>();
    assert_eq!(
        fills.len(),
        1,
        "one accepted body must publish exactly one window background fill"
    );
    let incremental_end = source_budget_window_end(&frame);
    let incremental_trace =
        selected_window_layout_trace(&frame.eval, &frame.engine, frame.frame_id);
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(&mut frame.eval, frame.frame_id);
    let full = fresh
        .last_frame_display_state
        .as_ref()
        .expect("reference frame");
    assert_eq!(
        incremental_trace,
        selected_window_layout_trace(&frame.eval, &fresh, frame.frame_id)
    );
    assert_eq!(incremental_end, source_budget_window_end(&frame));
    // Compare paint semantics by resolved face rather than attempt-local IDs.
    let fill_trace = |state: &neomacs_display_protocol::glyph_matrix::FrameDisplayState| {
        state
            .face_fills
            .iter()
            .map(|fill| {
                (
                    fill.window_id,
                    fill.row_role,
                    fill.clip_rect,
                    fill.bounds,
                    state.faces.get(&fill.face_id).cloned().map(|mut face| {
                        face.id = FaceId::new(0);
                        face.default_resolved_font_id = face
                            .default_resolved_font_id
                            .map(|_| neomacs_display_protocol::font::ResolvedFontId(0));
                        face
                    }),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(fill_trace(&incremental), fill_trace(full));
    assert_eq!(incremental.backgrounds.len(), full.backgrounds.len());
    assert_eq!(incremental.borders.len(), full.borders.len());
    assert_eq!(incremental.cursors.len(), full.cursors.len());
    assert_eq!(incremental.scroll_bars.len(), full.scroll_bars.len());
    assert_eq!(incremental.window_infos.len(), full.window_infos.len());
    assert_eq!(incremental.phys_cursor, full.phys_cursor);
    assert_eq!(
        incremental.cursor_effects_by_window,
        full.cursor_effects_by_window
    );
}

#[cfg(test)]
#[path = "edit_sync_source_budget_consumers.rs"]
mod source_budget_consumers;

#[cfg(test)]
#[path = "edit_sync_source_budget_replay_retry.rs"]
mod source_budget_replay_retry;
