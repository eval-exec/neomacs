//! Active GNU 31.1 catches mode/header/tab signals and aborts on throws.
//! Expectations were captured in tmp/fx2-oracle/gnu-behavior.md.

use super::*;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{GlyphArea, GlyphType};
use neovm_core::buffer::EmacsBytePos;
use neovm_core::emacs_core::Context;
use neovm_core::window::FrameId;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone, Copy)]
enum ChromeLine {
    Mode,
    Header,
    Tab,
}

impl ChromeLine {
    fn variable(self) -> &'static str {
        match self {
            Self::Mode => "mode-line-format",
            Self::Header => "header-line-format",
            Self::Tab => "tab-line-format",
        }
    }

    fn role(self) -> GlyphRowRole {
        match self {
            Self::Mode => GlyphRowRole::ModeLine,
            Self::Header => GlyphRowRole::HeaderLine,
            Self::Tab => GlyphRowRole::TabLine,
        }
    }

    fn following(self) -> Option<Self> {
        match self {
            Self::Mode => None,
            Self::Header => Some(Self::Mode),
            Self::Tab => Some(Self::Header),
        }
    }
}

fn enable_flow() {
    // nextest isolates each test in a process. Set the immutable process
    // configuration before the first mode-line evaluation in that process.
    unsafe { std::env::set_var("NEOVM_MODE_LINE_FLOW", "1") };
}

fn frame_with_format(line: ChromeLine, format: &str) -> (Context, FrameId) {
    let mut eval = Context::new();
    eval.eval_str(
        "(setq noninteractive nil inhibit-redisplay nil \
               mode-line-format nil header-line-format nil tab-line-format nil \
               fx2-later nil fx2-other-line nil)",
    )
    .expect("display variables");
    let buffer = eval.buffer_manager().current_buffer().expect("buffer").id();
    let format = eval.eval_str(format).expect("chrome format");
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer).expect("buffer");
        buffer.insert("body\n");
        buffer.goto_emacs_byte_pos(EmacsBytePos::new(0));
        buffer.set_buffer_local(line.variable(), format);
    }
    let frame = eval
        .frame_manager_mut()
        .create_frame("mode-line-flow", 80, 24, buffer);
    // GNU keeps the inactive mini-window separate from the displayed buffer.
    let minibuffer = eval
        .buffer_manager()
        .find_buffer_by_name(" *Minibuf-0*")
        .unwrap_or_else(|| eval.buffer_manager_mut().create_buffer(" *Minibuf-0*"));
    if let Some(leaf) = eval
        .frame_manager_mut()
        .get_mut(frame)
        .expect("frame")
        .minibuffer_leaf
        .as_mut()
    {
        leaf.set_buffer(minibuffer);
    }
    eval.eval_str("(string-match \"ab\" \"abcd\")")
        .expect("original match data");
    (eval, frame)
}

fn assert_original_match_data(eval: &mut Context) {
    assert!(
        eval.eval_str("(equal (match-data t) '(0 2))")
            .expect("match data after redisplay")
            .is_truthy(),
        "GNU display_mode_line restores match data on normal and throw exits"
    );
}

#[test]
fn mode_line_flow_throws_abort_real_layout_before_publishing() {
    enable_flow();
    for line in [ChromeLine::Mode, ChromeLine::Header, ChromeLine::Tab] {
        let (mut eval, frame) = frame_with_format(
            line,
            r#"'("before"
                 (:eval (progn (string-match "bc" "abcd")
                               (throw 'escaped "escaped")))
                 (:eval (progn (setq fx2-later t) "after")))"#,
        );
        if let Some(following) = line.following() {
            eval.eval_str(&format!(
                "(setq {} '(:eval (progn (setq fx2-other-line t) \"later\")))",
                following.variable()
            ))
            .expect("following chrome row");
        }
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        let mut engine = LayoutEngine::new_without_font_metrics();
        eval.redisplay_fn = Some(Box::new(move |eval| {
            observed.set(observed.get() + 1);
            assert!(
                matches!(
                    engine.redisplay_frame_attempt(eval, frame),
                    FrameLayoutAttempt::Aborted
                ),
                "{} throw must abort the real frame attempt",
                line.variable()
            );
            assert!(
                eval.has_mode_line_display_flow(),
                "callback retains the exit"
            );
            assert!(engine.last_frame_display_state.is_none());
            let frame = eval.frame_manager().get(frame).expect("frame after throw");
            assert!(!frame.has_prepared_display_presentations());
            assert!(frame.active_presentation().is_none());
            assert_original_match_data(eval);
        }));

        // A real catch is necessary: GNU treats an unmatched throw as the
        // ordinary no-catch signal, which its safe evaluator suppresses.
        let caught = eval
            .eval_str("(catch 'escaped (redisplay t))")
            .expect("redisplay exit reaches the enclosing catch");
        assert_eq!(caught.as_utf8_str(), Some("escaped"));
        assert_eq!(calls.get(), 1);
        assert!(
            !eval.has_mode_line_display_flow(),
            "redisplay consumes the exit"
        );
        assert!(eval.redisplay_fn.is_some(), "callback is restored on exit");
        assert!(
            eval.eval_str("(and (null fx2-later) (null fx2-other-line))")
                .expect("later evaluations")
                .is_truthy(),
            "neither later elements nor later chrome rows run after a throw"
        );
    }
}

#[test]
fn mode_line_flow_signals_keep_rendered_rows_and_restore_match_data() {
    enable_flow();
    for line in [ChromeLine::Mode, ChromeLine::Header, ChromeLine::Tab] {
        for condition in ["(error \"fx2 signal\")", "(signal 'quit nil)"] {
            let (mut eval, frame) = frame_with_format(
                line,
                &format!(
                    r#"'("before" (:eval (progn (string-match "bc" "abcd") {condition}))
                         (:eval (progn (setq fx2-later t) "after")))"#
                ),
            );
            let FrameLayoutAttempt::Prepared(state) =
                LayoutEngine::new_without_font_metrics().redisplay_frame_attempt(&mut eval, frame)
            else {
                panic!("{} must continue after {condition}", line.variable());
            };
            assert!(!eval.has_mode_line_display_flow());
            let text: String = state
                .window_matrices
                .iter()
                .flat_map(|window| &window.matrix.rows)
                .filter(|row| row.enabled && row.role == line.role())
                .flat_map(|row| &row.glyphs[GlyphArea::Text.index()])
                .filter_map(|glyph| match glyph.glyph_type {
                    GlyphType::Char { ch } => Some(ch),
                    _ => None,
                })
                .collect();
            assert_eq!(text.trim_end(), "beforeafter", "{}", line.variable());
            assert!(eval.eval_str("fx2-later").expect("later :eval").is_truthy());
            assert_original_match_data(&mut eval);
        }
    }
}
