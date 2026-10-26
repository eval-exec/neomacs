//! GNU frame.c's shared candidate walk for next-frame and previous-frame.

use crate::emacs_core::error::{EvalResult, expect_max_args};
use crate::emacs_core::intern::intern;
use crate::emacs_core::{Context, Value};
use crate::tagged::SymbolMode;
use crate::window::{Frame, FrameId, FrameManager, WindowId};

#[derive(Clone, Copy)]
enum Direction {
    Next,
    Previous,
}

impl Direction {
    const fn name(self) -> &'static str {
        match self {
            Self::Next => "next-frame",
            Self::Previous => "previous-frame",
        }
    }

    fn advance(self, index: usize, len: usize) -> usize {
        match self {
            Self::Next => (index + 1) % len,
            Self::Previous if index == 0 => len - 1,
            Self::Previous => index - 1,
        }
    }
}

/// Decode MINIFRAME once, keeping window identity and visibility separate.
#[derive(Clone, Copy)]
enum CandidateFilter {
    Ordinary,
    Visible,
    VisibleOrIconified,
    Minibuffer {
        window: WindowId,
        owner: Option<FrameId>,
    },
    All,
}

impl CandidateFilter {
    fn from_lisp(value: Value, mode: SymbolMode, frames: &FrameManager) -> Self {
        if value.is_nil() {
            Self::Ordinary
        } else if value.as_symbol_id(mode) == Some(intern("visible")) {
            Self::Visible
        } else if let Some(window) = value.as_window_id().map(WindowId) {
            Self::Minibuffer {
                window,
                owner: frames.window_owner_frame_id(window),
            }
        } else if value.as_int() == Some(0) {
            Self::VisibleOrIconified
        } else {
            Self::All
        }
    }

    fn admits(self, frame: &Frame) -> bool {
        match self {
            Self::Ordinary => frame.minibuffer_window != Some(frame.root_window().id()),
            Self::Visible => frame.visibility.is_visible(),
            Self::VisibleOrIconified => frame.visibility.is_visible_or_iconified(),
            Self::Minibuffer { window, owner } => {
                frame.minibuffer_window == Some(window)
                    || owner.is_some_and(|owner| {
                        frame.id == owner || frame.focus_frame.as_frame_id() == Some(owner.0)
                    })
            }
            Self::All => true,
        }
    }
}

fn same_input_owner(origin: &Frame, candidate: &Frame) -> bool {
    // GNU compares the keyboard for GUI frames and the tty for text frames.
    // This runtime parks exactly one kboard per terminal_id. There is no
    // separate cross-terminal shared-keyboard identity to infer here.
    origin.terminal_id == candidate.terminal_id
        && origin.effective_window_system().is_some()
            == candidate.effective_window_system().is_some()
}

fn cycle(eval: &mut Context, args: Vec<Value>, direction: Direction) -> EvalResult {
    expect_max_args(direction.name(), &args, 2)?;
    let origin = crate::emacs_core::window_cmds::resolve_frame_id_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        args.first(),
        crate::emacs_core::window_cmds::FrameDomain::Live,
    )?;
    let frames = &eval.frames;
    let source = frames.get(origin).expect("validated live frame");
    let filter = CandidateFilter::from_lisp(
        args.get(1).copied().unwrap_or(Value::NIL),
        eval.symbol_mode(),
        frames,
    );
    let ordered = frames.frame_list();
    let mut index = ordered
        .iter()
        .position(|id| *id == origin)
        .expect("live origin is in the frame list");
    // The origin is returned only as a fallback, even when it would fail its
    // own filter. Each other candidate is visited once before wrapping back.
    for _ in 1..ordered.len() {
        index = direction.advance(index, ordered.len());
        let candidate = frames.get(ordered[index]).expect("frame list is live");
        if same_input_owner(source, candidate)
            && !candidate
                .parameter("no-other-frame")
                .is_some_and(|value| value.is_truthy())
            && filter.admits(candidate)
        {
            return Ok(Value::make_frame(candidate.id.0));
        }
    }
    Ok(Value::make_frame(origin.0))
}

/// `(next-frame &optional FRAME MINIFRAME)` traverses frame-list forwards.
pub(crate) fn builtin_next_frame(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    cycle(eval, args, Direction::Next)
}

/// `(previous-frame &optional FRAME MINIFRAME)` traverses frame-list backwards.
pub(crate) fn builtin_previous_frame(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    cycle(eval, args, Direction::Previous)
}
