//! GNU try_window_id GIVE_UP(24): a retained numbered row is position-
//! dependent body content, not text whose absolute positions merely shift.
//! Exercise the real incremental renderer and compare every glyph/face,
//! snapshot, cursor, row index and window end with a fresh complete layout.
use super::*;
use crate::incremental_layout::lazy_proof_test_support::{self as probes, Guard};

fn source() -> String {
    (0..80)
        .map(|line| format!("(message \"line {line:02}\")\n"))
        .collect()
}

fn numbered_frame(mode: &str, loaded: bool) -> SyncFrame {
    let text = source();
    let at = end_of_line(&text, 4) - 4;
    let mut frame = if loaded {
        // Undo is Lisp-only. Use the actual bootstrapped primitive-undo,
        // rather than reproducing its insert/delete effects in the test.
        let mut eval = create_bootstrap_evaluator_cached_with_features(&["x", "neomacs"])
            .expect("bootstrap for GNU Lisp undo");
        apply_runtime_startup_state(&mut eval).expect("runtime startup state");
        eval.eval_str("(switch-to-buffer (get-buffer-create \"*sync-numbered-undo*\"))")
            .unwrap();
        convert_current_buffer_text_backend(&mut eval, BufferTextBackendKind::GapBuffer);
        insert_fragmented_current_buffer_text(&mut eval, &text);
        let buffer = eval.buffer_manager().current_buffer().unwrap().id();
        let frame_id =
            eval.frame_manager_mut()
                .create_frame("sync-numbered-undo", 800, 600, buffer);
        bind_minibuffer_buffer(&mut eval, frame_id);
        assert!(eval.frame_manager_mut().select_frame(frame_id));
        eval.eval_str(&format!("(goto-char {})", at + 1)).unwrap();
        SyncFrame {
            eval,
            frame_id,
            engine: LayoutEngine::new(),
        }
    } else {
        SyncFrame::new(&text, at)
    };
    assert!(frame.eval.frame_manager_mut().select_frame(frame.frame_id));
    frame
        .eval
        .eval_str(&format!(
            "(setq major-mode 'emacs-lisp-mode \
                   bidi-paragraph-direction 'left-to-right \
                   display-line-numbers {mode} \
                   display-line-numbers-current-absolute t \
                   mode-line-format nil header-line-format nil tab-line-format nil)"
        ))
        .unwrap();
    frame
        .engine
        .layout_frame_rust(&mut frame.eval, frame.frame_id);
    frame
}

fn edits(mode: &str, operation: &str) {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let _lazy = Guard::set(true);
    let mut frame = numbered_frame(mode, matches!(operation, "undo" | "yank"));
    match operation {
        "RET" => {
            frame.step("(insert \"\\n\")");
        }
        "join" => {
            frame.step("(progn (end-of-line) (delete-region (point) (1+ (point))))");
        }
        "yank" => {
            // Invoke the actual Lisp yank command, including mark placement.
            frame.step("(progn (kill-new \"one\\ntwo\\nthree\\n\") (yank))");
        }
        "undo" => {
            frame
                .eval
                .eval_str("(progn (buffer-enable-undo) (setq buffer-undo-list nil))")
                .unwrap();
            frame.step("(progn (insert \"\\n\") (undo-boundary))");
            frame.step("(primitive-undo 1 (cdr buffer-undo-list))");
            assert_eq!(
                frame
                    .eval
                    .eval_str("(buffer-string)")
                    .unwrap()
                    .as_str_owned(),
                Some(source()),
                "actual GNU Lisp undo restores the pre-RET buffer"
            );
        }
        other => panic!("unknown operation {other}"),
    }
    // Stale rows otherwise survive subsequent Sync edits indefinitely.
    frame.step("(insert \"x\")");
    frame.step("(delete-region (1- (point)) (point))");
    assert_eq!(
        probes::counts().sync_admissions,
        0,
        "{mode} {operation}: numbered body rows must not enter Sync"
    );
}

#[test]
fn absolute_ret_and_follow_up_typing_match_fresh_gutters() {
    edits("t", "RET");
}

#[test]
fn absolute_join_and_follow_up_typing_match_fresh_gutters() {
    edits("t", "join");
}

#[test]
fn absolute_multiline_yank_and_follow_up_typing_match_fresh_gutters() {
    edits("t", "yank");
}

#[test]
fn absolute_undo_and_follow_up_typing_match_fresh_gutters() {
    edits("t", "undo");
}

#[test]
fn relative_ret_and_follow_up_typing_match_fresh_gutters() {
    edits("'relative", "RET");
}

#[test]
fn relative_join_and_follow_up_typing_match_fresh_gutters() {
    edits("'relative", "join");
}

#[test]
fn relative_multiline_yank_and_follow_up_typing_match_fresh_gutters() {
    edits("'relative", "yank");
}

#[test]
fn relative_undo_and_follow_up_typing_match_fresh_gutters() {
    edits("'relative", "undo");
}

#[test]
fn visual_ret_and_follow_up_typing_match_fresh_gutters() {
    edits("'visual", "RET");
}

#[test]
fn visual_join_and_follow_up_typing_match_fresh_gutters() {
    edits("'visual", "join");
}

#[test]
fn visual_multiline_yank_and_follow_up_typing_match_fresh_gutters() {
    edits("'visual", "yank");
}

#[test]
fn visual_undo_and_follow_up_typing_match_fresh_gutters() {
    edits("'visual", "undo");
}
