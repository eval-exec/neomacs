//! Permission to preserve the allocation when a frame parameter implies a resize.
//!
//! GNU Emacs a360712c, src/frame.c:189-216 separates this predicate from
//! adjust_frame_size's minimum-size overrides (885-927). Callers still enforce
//! those minima and choose the requested text size.

use crate::window::{Frame, FrameParam};
use neovm_value::error::Flow;
use neovm_value::list_scan::for_each_proper_list_tail_scan;
use neovm_value::tagged::SymbolMode;
use neovm_value::value::{Value, eq_value};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImpliedResizeReason {
    Font,
    ToolBarLines,
    MenuBarLines,
    InternalBorderWidth,
}

impl ImpliedResizeReason {
    pub(crate) fn parameter(self) -> Value {
        Value::symbol(match self {
            Self::Font => "font",
            Self::ToolBarLines => "tool-bar-lines",
            Self::MenuBarLines => "menu-bar-lines",
            Self::InternalBorderWidth => "internal-border-width",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResizeAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResizePolicy {
    PreserveAllocation,
    PreserveTextGrid,
}

/// The creation gates of GNU's frame_inhibit_resize, not window visibility.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CreationPhase {
    BeforeMakeFrame,
    /// GTK waits for the first toolbar allocation even after make-frame.
    /// A backend that projects its own chrome synchronously needs no such wait.
    AwaitingInitialToolBarAllocation,
    Made,
}

/// Return the parameter policy before the caller applies minimum-size overrides.
///
/// Ordinary inhibition cannot change a frame's requested creation geometry.
/// `force` alone bypasses that gate. Parameter lists use GNU's checked tail
/// traversal, including early membership success before an invalid tail.
pub(crate) fn policy(
    frame: &Frame,
    inhibit: Value,
    phase: CreationPhase,
    reason: ImpliedResizeReason,
    axis: ResizeAxis,
    symbols: SymbolMode,
) -> Result<ResizePolicy, Flow> {
    if eq_value(&symbols.unwrap(inhibit), &Value::symbol("force")) {
        return Ok(ResizePolicy::PreserveAllocation);
    }
    if phase != CreationPhase::Made {
        return Ok(ResizePolicy::PreserveTextGrid);
    }
    if eq_value(&symbols.unwrap(inhibit), &Value::T) {
        return Ok(ResizePolicy::PreserveAllocation);
    }
    if inhibit.is_cons() {
        let parameter = reason.parameter();
        let member = for_each_proper_list_tail_scan::<false, _>(inhibit, inhibit, |tail| {
            Ok(eq_value(&symbols.unwrap(tail.cons_car()), &parameter).then_some(tail))
        })?;
        if !member.is_nil() {
            return Ok(ResizePolicy::PreserveAllocation);
        }
    }
    let fullscreen = frame
        .known_parameter(FrameParam::Fullscreen)
        .unwrap_or(Value::NIL);
    let fills_axis = !fullscreen.is_nil()
        && !eq_value(
            &symbols.unwrap(fullscreen),
            &Value::symbol(match axis {
                ResizeAxis::Horizontal => "fullheight",
                ResizeAxis::Vertical => "fullwidth",
            }),
        );
    Ok(if fills_axis || frame.effective_window_system().is_none() {
        ResizePolicy::PreserveAllocation
    } else {
        ResizePolicy::PreserveTextGrid
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::DisplayFrameExt;
    use crate::window::{DisplayFrameManagerExt, FrameId, FrameManager};
    use neovm_edit::buffer::BufferId;

    fn policy(
        frame: &Frame,
        inhibit: Value,
        phase: CreationPhase,
        reason: ImpliedResizeReason,
        axis: ResizeAxis,
    ) -> ResizePolicy {
        super::policy(frame, inhibit, phase, reason, axis, SymbolMode::Bare).unwrap()
    }

    fn graphical_frame() -> (FrameManager, FrameId) {
        crate::test_utils::init_test_tracing();
        let mut frames = FrameManager::new();
        let id = frames.create_frame("implied-resize", 800, 600, BufferId(1));
        frames
            .get_mut(id)
            .unwrap()
            .set_window_system(Some(Value::symbol("neo")));
        (frames, id)
    }

    #[test]
    fn ordinary_inhibition_waits_for_frame_and_initial_toolbar_creation() {
        let (mut frames, id) = graphical_frame();
        let frame = frames.get_mut(id).unwrap();
        frame.set_known_parameter(FrameParam::Fullscreen, Value::symbol("fullboth"));
        for phase in [
            CreationPhase::BeforeMakeFrame,
            CreationPhase::AwaitingInitialToolBarAllocation,
        ] {
            for inhibit in [Value::NIL, Value::T] {
                for axis in [ResizeAxis::Horizontal, ResizeAxis::Vertical] {
                    assert_eq!(
                        policy(frame, inhibit, phase, ImpliedResizeReason::Font, axis),
                        ResizePolicy::PreserveTextGrid
                    );
                }
            }
        }
        assert_eq!(
            policy(
                frame,
                Value::T,
                CreationPhase::Made,
                ImpliedResizeReason::Font,
                ResizeAxis::Vertical,
            ),
            ResizePolicy::PreserveAllocation
        );
    }

    #[test]
    fn force_preserves_allocation_even_before_frame_creation_finishes() {
        let (frames, id) = graphical_frame();
        let frame = frames.get(id).unwrap();
        for phase in [
            CreationPhase::BeforeMakeFrame,
            CreationPhase::AwaitingInitialToolBarAllocation,
            CreationPhase::Made,
        ] {
            assert_eq!(
                policy(
                    frame,
                    Value::symbol("force"),
                    phase,
                    ImpliedResizeReason::ToolBarLines,
                    ResizeAxis::Vertical,
                ),
                ResizePolicy::PreserveAllocation
            );
        }
    }

    #[test]
    fn true_inhibits_each_parameter_on_both_axes_of_a_made_graphical_frame() {
        let (frames, id) = graphical_frame();
        let frame = frames.get(id).unwrap();
        for reason in [
            ImpliedResizeReason::Font,
            ImpliedResizeReason::ToolBarLines,
            ImpliedResizeReason::MenuBarLines,
            ImpliedResizeReason::InternalBorderWidth,
        ] {
            for axis in [ResizeAxis::Horizontal, ResizeAxis::Vertical] {
                assert_eq!(
                    policy(frame, Value::T, CreationPhase::Made, reason, axis),
                    ResizePolicy::PreserveAllocation
                );
            }
        }
    }

    #[test]
    fn inhibition_lists_match_the_changed_parameter_not_arbitrary_truthiness() {
        let (frames, id) = graphical_frame();
        let frame = frames.get(id).unwrap();
        let reasons = [
            ImpliedResizeReason::Font,
            ImpliedResizeReason::ToolBarLines,
            ImpliedResizeReason::MenuBarLines,
            ImpliedResizeReason::InternalBorderWidth,
        ];
        for selected in reasons {
            let inhibit = Value::list(vec![selected.parameter()]);
            for reason in reasons {
                assert_eq!(
                    policy(
                        frame,
                        inhibit,
                        CreationPhase::Made,
                        reason,
                        ResizeAxis::Vertical,
                    ),
                    if selected == reason {
                        ResizePolicy::PreserveAllocation
                    } else {
                        ResizePolicy::PreserveTextGrid
                    }
                );
            }
        }
        for inhibit in [Value::NIL, Value::fixnum(1), Value::symbol("font")] {
            assert_eq!(
                policy(
                    frame,
                    inhibit,
                    CreationPhase::Made,
                    ImpliedResizeReason::Font,
                    ResizeAxis::Vertical,
                ),
                ResizePolicy::PreserveTextGrid
            );
        }
    }

    #[test]
    fn fullscreen_inhibits_only_the_axis_it_fills() {
        let (mut frames, id) = graphical_frame();
        let frame = frames.get_mut(id).unwrap();
        for (fullscreen, horizontal, vertical) in [
            (Value::NIL, false, false),
            (Value::symbol("fullheight"), false, true),
            (Value::symbol("fullwidth"), true, false),
            (Value::symbol("fullboth"), true, true),
            (Value::symbol("maximized"), true, true),
        ] {
            frame.set_known_parameter(FrameParam::Fullscreen, fullscreen);
            for (axis, inhibited) in [
                (ResizeAxis::Horizontal, horizontal),
                (ResizeAxis::Vertical, vertical),
            ] {
                assert_eq!(
                    policy(
                        frame,
                        Value::NIL,
                        CreationPhase::Made,
                        ImpliedResizeReason::InternalBorderWidth,
                        axis,
                    ),
                    if inhibited {
                        ResizePolicy::PreserveAllocation
                    } else {
                        ResizePolicy::PreserveTextGrid
                    }
                );
            }
        }
    }

    #[test]
    fn terminal_allocation_is_inhibited_only_after_the_creation_gate() {
        let (mut frames, id) = graphical_frame();
        let frame = frames.get_mut(id).unwrap();
        frame.set_window_system(None);
        for (phase, expected) in [
            (
                CreationPhase::BeforeMakeFrame,
                ResizePolicy::PreserveTextGrid,
            ),
            (CreationPhase::Made, ResizePolicy::PreserveAllocation),
        ] {
            assert_eq!(
                policy(
                    frame,
                    Value::NIL,
                    phase,
                    ImpliedResizeReason::MenuBarLines,
                    ResizeAxis::Vertical,
                ),
                expected
            );
        }
    }

    #[test]
    fn malformed_policy_list_signals_only_if_membership_reaches_its_tail() {
        let (mut frames, id) = graphical_frame();
        let frame = frames.get_mut(id).unwrap();
        // GNU evaluates list membership before the fullscreen/TTY alternatives.
        frame.set_known_parameter(FrameParam::Fullscreen, Value::symbol("fullboth"));
        let inhibit = Value::cons(Value::symbol("font"), Value::fixnum(7));
        assert_eq!(
            policy(
                frame,
                inhibit,
                CreationPhase::Made,
                ImpliedResizeReason::Font,
                ResizeAxis::Vertical,
            ),
            ResizePolicy::PreserveAllocation
        );
        let error = super::policy(
            frame,
            inhibit,
            CreationPhase::Made,
            ImpliedResizeReason::ToolBarLines,
            ResizeAxis::Vertical,
            SymbolMode::Bare,
        )
        .unwrap_err();
        let signal = error.as_signal().unwrap();
        assert_eq!(signal.symbol_name(), "wrong-type-argument");
        assert_eq!(signal.data, vec![Value::symbol("listp"), inhibit]);
        assert_eq!(
            policy(
                frame,
                inhibit,
                CreationPhase::BeforeMakeFrame,
                ImpliedResizeReason::ToolBarLines,
                ResizeAxis::Vertical,
            ),
            ResizePolicy::PreserveTextGrid
        );
    }

    #[test]
    fn circular_policy_list_uses_shared_checked_membership() {
        let (frames, id) = graphical_frame();
        let frame = frames.get(id).unwrap();
        let inhibit = Value::cons(Value::symbol("font"), Value::NIL);
        inhibit.set_cdr(inhibit);
        assert_eq!(
            policy(
                frame,
                inhibit,
                CreationPhase::Made,
                ImpliedResizeReason::Font,
                ResizeAxis::Vertical,
            ),
            ResizePolicy::PreserveAllocation
        );
        let error = super::policy(
            frame,
            inhibit,
            CreationPhase::Made,
            ImpliedResizeReason::ToolBarLines,
            ResizeAxis::Vertical,
            SymbolMode::Bare,
        )
        .unwrap_err();
        assert_eq!(error.as_signal().unwrap().symbol_name(), "circular-list");
        for phase in [
            CreationPhase::BeforeMakeFrame,
            CreationPhase::AwaitingInitialToolBarAllocation,
        ] {
            assert_eq!(
                policy(
                    frame,
                    inhibit,
                    phase,
                    ImpliedResizeReason::ToolBarLines,
                    ResizeAxis::Vertical,
                ),
                ResizePolicy::PreserveTextGrid
            );
        }
    }

    #[test]
    fn positioned_parameter_and_policy_symbols_follow_the_owning_context_mode() {
        let mut eval =
            neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
        let (mut frames, id) = graphical_frame();
        let frame = frames.get_mut(id).unwrap();
        // SAFETY: this test owns this Context's World on this thread; these
        // calls allocate through its heap view without invoking Lisp.
        let (font, force, fullscreen) = unsafe {
            let heap = eval.tagged_heap_mut();
            (
                heap.alloc_symbol_with_pos(Value::symbol("font"), Value::fixnum(1)),
                heap.alloc_symbol_with_pos(Value::symbol("force"), Value::fixnum(2)),
                heap.alloc_symbol_with_pos(Value::symbol("fullheight"), Value::fixnum(3)),
            )
        };
        let inhibit = Value::list(vec![font]);
        for (symbols, expected) in [
            (SymbolMode::Bare, ResizePolicy::PreserveTextGrid),
            (SymbolMode::Positioned, ResizePolicy::PreserveAllocation),
        ] {
            assert_eq!(
                super::policy(
                    frame,
                    inhibit,
                    CreationPhase::Made,
                    ImpliedResizeReason::Font,
                    ResizeAxis::Vertical,
                    symbols,
                )
                .unwrap(),
                expected
            );
        }
        for (symbols, expected) in [
            (SymbolMode::Bare, ResizePolicy::PreserveTextGrid),
            (SymbolMode::Positioned, ResizePolicy::PreserveAllocation),
        ] {
            assert_eq!(
                super::policy(
                    frame,
                    force,
                    CreationPhase::BeforeMakeFrame,
                    ImpliedResizeReason::Font,
                    ResizeAxis::Vertical,
                    symbols,
                )
                .unwrap(),
                expected
            );
        }
        frame.set_known_parameter(FrameParam::Fullscreen, fullscreen);
        for (symbols, expected) in [
            (SymbolMode::Bare, ResizePolicy::PreserveAllocation),
            (SymbolMode::Positioned, ResizePolicy::PreserveTextGrid),
        ] {
            assert_eq!(
                super::policy(
                    frame,
                    Value::NIL,
                    CreationPhase::Made,
                    ImpliedResizeReason::Font,
                    ResizeAxis::Horizontal,
                    symbols,
                )
                .unwrap(),
                expected
            );
        }
    }
}
