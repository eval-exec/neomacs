//! Display's local test-helper hub.
//!
//! TEST-SUPPORT-DESIGN rev 2 rule 8: every crate's test tree re-exports the
//! shared, upper-free `neovm_value::test_utils` helpers instead of copying
//! them. This keeps the historical `crate::test_utils::init_test_tracing()`
//! call sites in display's own tests resolving without a core edge (an upward
//! dependency is forbidden). Compiled only for display's tests and for an
//! upper crate's test-support build; absent from every product profile.

pub use neovm_value::test_utils::*;

/// The display crate's cfg(test) unit runtime (H5a/T5, display D1-D5).
///
/// Display links value/edit/text/eval only: it has no neovm-system and no
/// neovm-core edge. One set per crate: display and eval (and below) are REAL;
/// every system/core slot is an ABSENT-LAYER refusal or one of the signed
/// absent-by-construction bodies. The refusal bodies come from the ONE shared
/// `eval_absent_layer_slots!` macro (D4/T5); no second refusal body is copied.
#[cfg(test)]
mod unit_runtime {
    #![allow(unexpected_cfgs)]

    use neovm_eval::emacs_core::eval::Context;
    use neovm_eval::emacs_core::eval::buffer_kill::BufferKillHooks;
    use neovm_eval::emacs_core::eval::call_interactively::CallInteractivelyHooks;
    use neovm_eval::emacs_core::eval::case_tables::CaseTableHooks;
    use neovm_eval::emacs_core::eval::hooks_proof::{
        DisplayServiceHooks, EvalHookSet, EvalHooksInstalled, FileNotifyHostQueries,
        FrontendServiceHooks, TimerServiceHooks, WaitServiceHooks,
    };
    use neovm_eval::emacs_core::eval::input_hooks::InputHooks;
    use neovm_eval::emacs_core::eval::kmacro_hooks::KmacroHooks;
    use neovm_eval::emacs_core::eval::lifecycle::{
        BootHooks, CommandSeamHooks, DbusbindHooks, DynamicModuleHooks, FileIoHooks, FontsetHooks,
        LifecycleHooks, TerminalHooks, WindowTreeHooks, WorldStateHooks, XfacesHooks,
    };
    use neovm_eval::emacs_core::eval::perf::PerfHooks;
    use neovm_eval::emacs_core::eval::process_hooks::{MainWaitHooks, ProcessHooks, SelectHooks};
    use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};
    use neovm_eval::test_utils::CanonicalEvalStep;
    use neovm_value::display_vocab::FrameId;
    use neovm_value::error::EvalResult;
    use neovm_value::value::Value;
    use std::sync::OnceLock;

    // D4/T5: the one shared refusal building-block set. System/core slots only.
    neovm_eval::eval_absent_layer_slots!();
    use absent_layer_slots as s;

    // H3 upper names (system/core) in production registration order; display
    // filters out names it registers for real below.
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../neovm-eval/src/upper_subrs.generated.rs"
    ));
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../neovm-eval/src/upper_bootstrap_vars.generated.rs"
    ));

    /// The display-owned bootstrap variables xfaces registers for real (D3).
    const XFACES_BOOTSTRAP_VARS: &[&str] = &[
        "face--new-frame-defaults",
        "face-default-stipple",
        "tty-defined-color-alist",
        "scalable-fonts-allowed",
        "face-ignored-fonts",
        "face-remapping-alist",
        "face-font-rescale-alist",
        "face-near-same-color-threshold",
        "face-font-lax-matched-attributes",
    ];

    /// D3: display-owned bootstrap variables are REAL; only the system/core
    /// names are bound to their absent-layer markers.
    fn boot_register_core_bootstrap_vars(obarray: &mut neovm_value::symbol::Obarray) {
        crate::emacs_core::xfaces::register_bootstrap_vars(obarray);
        for name in UPPER_BOOTSTRAP_VAR_NAMES {
            if XFACES_BOOTSTRAP_VARS.contains(name) {
                continue;
            }
            s::bind_absent_bootstrap_vars(obarray, &[*name]);
        }
    }

    /// The signed canonical plan (gapsurvey T6 r2): the full name table and
    /// the step plan. Test-only data inclusion, source-level, no re-export.
    const CANONICAL_NAMES: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../neovm-eval/src/canonical_subr_names.tsv"
    ));
    const CANONICAL_PLAN: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../neovm-eval/src/canonical_subr_step_plan.tsv"
    ));

    struct CanonicalName {
        name: &'static str,
        min: u16,
        max: Option<u16>,
        cfg: &'static str,
    }

    struct CanonicalStep {
        kind: CanonicalStepKind,
        id: &'static str,
        first: usize,
        count: usize,
    }

    /// The plan's step kinds. Exhaustive: the walk cannot fall through a
    /// string catch-all for a step kind.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum CanonicalStepKind {
        EvalCall,
        DisplaySegment,
        UpperSegment,
    }

    impl CanonicalStepKind {
        fn from_label(label: &str) -> Option<Self> {
            Some(match label {
                "eval_call" => Self::EvalCall,
                "display_segment" => Self::DisplaySegment,
                "upper_segment" => Self::UpperSegment,
                _ => return None,
            })
        }
    }

    /// The display-owned canonical plan steps, in plan order.
    ///
    /// `from_id` parses the checked-in plan, `ALL` is checked against the
    /// plan's display steps in the walk, and the registration match is
    /// exhaustive over the variants: a new display step must be added here,
    /// not handled by a string catch-all.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum DisplayStep {
        TerminalShaderVideoXwidget,
        BufferTextPixelSize,
        FormatModeLine,
        SelectedWindow,
        SelectedFrame,
        NextFrame,
        XOpenConnection,
        FaceFont,
        FrameParameter,
        InternalLispFaceP,
        SetFrameSizeAndPositionPixelwise,
        FontGetSystemFont,
        FrameAncestorP,
        FrameInternalBorderWidth,
        FrameRightDividerWidth,
        FrameWindowStateChange,
        HandleSwitchFrame,
        MakeFrameInvisible,
        NeomacsFrameSnapshot,
        RedirectFrameFocus,
        ResizeMiniWindowInternal,
        SetFrameWindowStateChange,
        SetWindowNewNormal,
        TtyFrameAt,
        WindowBottomDividerWidth,
        Font,
        NeomacsFrameEdges,
        EffectsModifiersChrome,
        WindowDiscardBufferFromWindow,
        WindowTextPixelSize,
        ActiveMinibufferWindow,
        InternalLispFaceEqualP,
        MergeFaceAttribute,
        ColorDistance,
        InternalCopyLispFace,
        RedrawDisplay,
        ForceWindowUpdate,
        XFocusFrame,
        XPopupDialog,
        DeleteTerminal,
        ImageTransformsP,
        CurrentBidiParagraphDirection,
        MoveToWindowLine,
    }

    impl DisplayStep {
        const ALL: [Self; 43] = [
            Self::TerminalShaderVideoXwidget,
            Self::BufferTextPixelSize,
            Self::FormatModeLine,
            Self::SelectedWindow,
            Self::SelectedFrame,
            Self::NextFrame,
            Self::XOpenConnection,
            Self::FaceFont,
            Self::FrameParameter,
            Self::InternalLispFaceP,
            Self::SetFrameSizeAndPositionPixelwise,
            Self::FontGetSystemFont,
            Self::FrameAncestorP,
            Self::FrameInternalBorderWidth,
            Self::FrameRightDividerWidth,
            Self::FrameWindowStateChange,
            Self::HandleSwitchFrame,
            Self::MakeFrameInvisible,
            Self::NeomacsFrameSnapshot,
            Self::RedirectFrameFocus,
            Self::ResizeMiniWindowInternal,
            Self::SetFrameWindowStateChange,
            Self::SetWindowNewNormal,
            Self::TtyFrameAt,
            Self::WindowBottomDividerWidth,
            Self::Font,
            Self::NeomacsFrameEdges,
            Self::EffectsModifiersChrome,
            Self::WindowDiscardBufferFromWindow,
            Self::WindowTextPixelSize,
            Self::ActiveMinibufferWindow,
            Self::InternalLispFaceEqualP,
            Self::MergeFaceAttribute,
            Self::ColorDistance,
            Self::InternalCopyLispFace,
            Self::RedrawDisplay,
            Self::ForceWindowUpdate,
            Self::XFocusFrame,
            Self::XPopupDialog,
            Self::DeleteTerminal,
            Self::ImageTransformsP,
            Self::CurrentBidiParagraphDirection,
            Self::MoveToWindowLine,
        ];

        const fn id(self) -> &'static str {
            match self {
                Self::TerminalShaderVideoXwidget => "terminal_shader_video_xwidget",
                Self::BufferTextPixelSize => "buffer_text_pixel_size",
                Self::FormatModeLine => "format_mode_line",
                Self::SelectedWindow => "selected_window",
                Self::SelectedFrame => "selected_frame",
                Self::NextFrame => "next_frame",
                Self::XOpenConnection => "x_open_connection",
                Self::FaceFont => "face_font",
                Self::FrameParameter => "frame_parameter",
                Self::InternalLispFaceP => "internal_lisp_face_p",
                Self::SetFrameSizeAndPositionPixelwise => "set_frame_size_and_position_pixelwise",
                Self::FontGetSystemFont => "font_get_system_font",
                Self::FrameAncestorP => "frame_ancestor_p",
                Self::FrameInternalBorderWidth => "frame_internal_border_width",
                Self::FrameRightDividerWidth => "frame_right_divider_width",
                Self::FrameWindowStateChange => "frame_window_state_change",
                Self::HandleSwitchFrame => "handle_switch_frame",
                Self::MakeFrameInvisible => "make_frame_invisible",
                Self::NeomacsFrameSnapshot => "neomacs_frame_snapshot",
                Self::RedirectFrameFocus => "redirect_frame_focus",
                Self::ResizeMiniWindowInternal => "resize_mini_window_internal",
                Self::SetFrameWindowStateChange => "set_frame_window_state_change",
                Self::SetWindowNewNormal => "set_window_new_normal",
                Self::TtyFrameAt => "tty_frame_at",
                Self::WindowBottomDividerWidth => "window_bottom_divider_width",
                Self::Font => "font",
                Self::NeomacsFrameEdges => "neomacs_frame_edges",
                Self::EffectsModifiersChrome => "effects_modifiers_chrome",
                Self::WindowDiscardBufferFromWindow => "window_discard_buffer_from_window",
                Self::WindowTextPixelSize => "window_text_pixel_size",
                Self::ActiveMinibufferWindow => "active_minibuffer_window",
                Self::InternalLispFaceEqualP => "internal_lisp_face_equal_p",
                Self::MergeFaceAttribute => "merge_face_attribute",
                Self::ColorDistance => "color_distance",
                Self::InternalCopyLispFace => "internal_copy_lisp_face",
                Self::RedrawDisplay => "redraw_display",
                Self::ForceWindowUpdate => "force_window_update",
                Self::XFocusFrame => "x_focus_frame",
                Self::XPopupDialog => "x_popup_dialog",
                Self::DeleteTerminal => "delete_terminal",
                Self::ImageTransformsP => "image_transforms_p",
                Self::CurrentBidiParagraphDirection => "current_bidi_paragraph_direction",
                Self::MoveToWindowLine => "move_to_window_line",
            }
        }

        fn from_id(id: &str) -> Option<Self> {
            Some(match id {
                "terminal_shader_video_xwidget" => Self::TerminalShaderVideoXwidget,
                "buffer_text_pixel_size" => Self::BufferTextPixelSize,
                "format_mode_line" => Self::FormatModeLine,
                "selected_window" => Self::SelectedWindow,
                "selected_frame" => Self::SelectedFrame,
                "next_frame" => Self::NextFrame,
                "x_open_connection" => Self::XOpenConnection,
                "face_font" => Self::FaceFont,
                "frame_parameter" => Self::FrameParameter,
                "internal_lisp_face_p" => Self::InternalLispFaceP,
                "set_frame_size_and_position_pixelwise" => Self::SetFrameSizeAndPositionPixelwise,
                "font_get_system_font" => Self::FontGetSystemFont,
                "frame_ancestor_p" => Self::FrameAncestorP,
                "frame_internal_border_width" => Self::FrameInternalBorderWidth,
                "frame_right_divider_width" => Self::FrameRightDividerWidth,
                "frame_window_state_change" => Self::FrameWindowStateChange,
                "handle_switch_frame" => Self::HandleSwitchFrame,
                "make_frame_invisible" => Self::MakeFrameInvisible,
                "neomacs_frame_snapshot" => Self::NeomacsFrameSnapshot,
                "redirect_frame_focus" => Self::RedirectFrameFocus,
                "resize_mini_window_internal" => Self::ResizeMiniWindowInternal,
                "set_frame_window_state_change" => Self::SetFrameWindowStateChange,
                "set_window_new_normal" => Self::SetWindowNewNormal,
                "tty_frame_at" => Self::TtyFrameAt,
                "window_bottom_divider_width" => Self::WindowBottomDividerWidth,
                "font" => Self::Font,
                "neomacs_frame_edges" => Self::NeomacsFrameEdges,
                "effects_modifiers_chrome" => Self::EffectsModifiersChrome,
                "window_discard_buffer_from_window" => Self::WindowDiscardBufferFromWindow,
                "window_text_pixel_size" => Self::WindowTextPixelSize,
                "active_minibuffer_window" => Self::ActiveMinibufferWindow,
                "internal_lisp_face_equal_p" => Self::InternalLispFaceEqualP,
                "merge_face_attribute" => Self::MergeFaceAttribute,
                "color_distance" => Self::ColorDistance,
                "internal_copy_lisp_face" => Self::InternalCopyLispFace,
                "redraw_display" => Self::RedrawDisplay,
                "force_window_update" => Self::ForceWindowUpdate,
                "x_focus_frame" => Self::XFocusFrame,
                "x_popup_dialog" => Self::XPopupDialog,
                "delete_terminal" => Self::DeleteTerminal,
                "image_transforms_p" => Self::ImageTransformsP,
                "current_bidi_paragraph_direction" => Self::CurrentBidiParagraphDirection,
                "move_to_window_line" => Self::MoveToWindowLine,
                _ => return None,
            })
        }
    }

    fn canonical_names() -> Vec<CanonicalName> {
        let mut lines = CANONICAL_NAMES.lines();
        assert_eq!(
            lines.next(),
            Some("name\tmin\tmax\tlayer\tcfg"),
            "canonical_subr_names.tsv header"
        );
        lines
            .map(|line| {
                let fields: Vec<&'static str> = line.split('\t').collect();
                assert_eq!(fields.len(), 5, "malformed canonical names row: {line}");
                CanonicalName {
                    name: fields[0],
                    min: fields[1].parse().expect("canonical name min"),
                    max: if fields[2] == "-" {
                        None
                    } else {
                        Some(fields[2].parse().expect("canonical name max"))
                    },
                    cfg: fields[4],
                }
            })
            .collect()
    }

    fn canonical_steps() -> Vec<CanonicalStep> {
        let mut lines = CANONICAL_PLAN.lines();
        assert_eq!(
            lines.next(),
            Some("step\tkind\tid\tlayer\tfirst\tcount\tmarker"),
            "canonical_subr_step_plan.tsv header"
        );
        lines
            .map(|line| {
                let fields: Vec<&'static str> = line.split('\t').collect();
                assert_eq!(fields.len(), 7, "malformed canonical plan row: {line}");
                CanonicalStep {
                    kind: CanonicalStepKind::from_label(fields[1])
                        .unwrap_or_else(|| panic!("unknown canonical step kind {:?}", fields[1])),
                    id: fields[2],
                    first: fields[4].parse().expect("canonical step first"),
                    count: fields[5].parse().expect("canonical step count"),
                }
            })
            .collect()
    }

    fn cfg_active(label: &str) -> bool {
        neovm_eval::host_cfg::HostCfgClass::from_label(label)
            .unwrap_or_else(|| panic!("unknown canonical cfg label {label:?}"))
            .is_active()
    }

    /// D1/T6 r2: walk the signed canonical plan in order. Every eval step
    /// calls the same production registrar; every display segment calls the
    /// plan-typed display registrar; every upper segment registers its
    /// cfg-active names as the signed absent-layer stand-ins. This is the one
    /// registration order core production uses (the T6 trace gate compares it).
    fn register_core_subrs(ctx: &mut Context) {
        let names = canonical_names();
        let steps = canonical_steps();
        let mut eval_steps = Vec::new();
        let mut display_steps = Vec::new();
        // LEAD 05:0xZ (peer): the walk may register an upper row with the
        // plan's arity only if it proves the plan against the COMPILED H3
        // stand-in table, so the T6 equality cannot hold by construction:
        //   1. every host-active row's plan (min, max) equals its compiled
        //      entry's (min, max), and a host-inactive row is not compiled;
        //   2. every compiled entry is accounted for EXACTLY ONCE: an upper
        //      entry by its one upper_segment registration, a display entry
        //      by the one REAL display row its segment registers below.
        // The display rows remain in the checked-in snapshot table (the
        // split made them REAL, not absent), so the ledger is a partition,
        // not "upper rows only".
        let mut compiled = std::collections::BTreeMap::new();
        for (index, entry) in UPPER_SUBRS.iter().enumerate() {
            assert!(
                compiled.insert(entry.0, index).is_none(),
                "duplicate compiled upper stand-in {:?}",
                entry.0
            );
        }
        let mut consumed = vec![false; UPPER_SUBRS.len()];
        let mut display_owned = vec![false; UPPER_SUBRS.len()];
        // Account every display row against the compiled table before the
        // ordered walk: present iff host-active, arity equal to the plan, and
        // exactly one REAL registration site (its display segment).
        for step in &steps {
            if step.kind != CanonicalStepKind::DisplaySegment {
                continue;
            }
            for row in &names[step.first - 1..step.first - 1 + step.count] {
                match compiled.get(row.name) {
                    Some(&index) => {
                        let (_, min, max, _) = UPPER_SUBRS[index];
                        assert!(
                            cfg_active(row.cfg),
                            "host-inactive display row {:?} is compiled",
                            row.name
                        );
                        assert_eq!(
                            (min, max),
                            (row.min, row.max),
                            "plan/compiled arity mismatch for display row {:?}",
                            row.name
                        );
                        assert!(
                            !consumed[index] && !display_owned[index],
                            "compiled stand-in {:?} accounted twice",
                            row.name
                        );
                        display_owned[index] = true;
                    }
                    None => assert!(
                        !cfg_active(row.cfg),
                        "host-active display row {:?} has no compiled stand-in",
                        row.name
                    ),
                }
            }
        }
        for step in &steps {
            match step.kind {
                CanonicalStepKind::EvalCall => {
                    let eval_step = CanonicalEvalStep::from_id(step.id)
                        .unwrap_or_else(|| panic!("unknown canonical eval step id {:?}", step.id));
                    eval_steps.push(eval_step);
                    match eval_step {
                        CanonicalEvalStep::Lcms => {
                            neovm_eval::emacs_core::builtins::lcms::register_subrs(ctx)
                        }
                        CanonicalEvalStep::Data => {
                            neovm_eval::emacs_core::data::register_subrs(ctx)
                        }
                        CanonicalEvalStep::Ordinary => {
                            neovm_eval::emacs_core::eval::register_subrs(ctx)
                        }
                        CanonicalEvalStep::Fboundp => {
                            ctx.register_subr(neovm_eval::emacs_core::eval::FBOUNDP_SUBR);
                        }
                        CanonicalEvalStep::FileNotify => {
                            std::cfg_select! {
                                any(target_os = "linux", target_os = "macos", target_os = "windows") => {
                                    neovm_eval::emacs_core::builtins::file_notify::register_subrs(ctx);
                                }
                                _ => {}
                            }
                        }
                        CanonicalEvalStep::EventProperties => {
                            // Same production initialization the C14 chain performs;
                            // it registers no subr (the plan step has count 0).
                            // SAFETY: compatibility setup only initializes event symbol
                            // properties; it does not call Lisp or mutate canonical
                            // function/debugger metadata.
                            unsafe {
                                ctx.obarray_view_mut().with_legacy_value_write(
                                neovm_eval::emacs_core::builtins::symbols::init_event_symbol_properties,
                            );
                            }
                        }
                        CanonicalEvalStep::Public => {
                            neovm_eval::emacs_core::eval::register_public_subrs(ctx)
                        }
                    }
                }
                CanonicalStepKind::DisplaySegment => {
                    let display_step = DisplayStep::from_id(step.id).unwrap_or_else(|| {
                        panic!("unknown canonical display step id {:?}", step.id)
                    });
                    display_steps.push(display_step);
                    match display_step {
                        DisplayStep::TerminalShaderVideoXwidget => {
                            crate::emacs_core::display_subrs::register_terminal_shader_video_xwidget(
                            ctx,
                        );
                        }
                        DisplayStep::BufferTextPixelSize => {
                            crate::emacs_core::display_subrs::register_buffer_text_pixel_size(ctx);
                        }
                        DisplayStep::FormatModeLine => {
                            crate::emacs_core::display_subrs::register_format_mode_line(ctx);
                        }
                        DisplayStep::SelectedWindow => {
                            crate::emacs_core::display_subrs::register_selected_window(ctx);
                        }
                        DisplayStep::SelectedFrame => {
                            crate::emacs_core::display_subrs::register_selected_frame(ctx);
                        }
                        DisplayStep::NextFrame => {
                            crate::emacs_core::display_subrs::register_next_frame(ctx);
                        }
                        DisplayStep::XOpenConnection => {
                            crate::emacs_core::display_subrs::register_x_open_connection(ctx);
                        }
                        DisplayStep::FaceFont => {
                            crate::emacs_core::display_subrs::register_face_font(ctx);
                        }
                        DisplayStep::FrameParameter => {
                            crate::emacs_core::display_subrs::register_frame_parameter(ctx);
                        }
                        DisplayStep::InternalLispFaceP => {
                            crate::emacs_core::display_subrs::register_internal_lisp_face_p(ctx);
                        }
                        DisplayStep::SetFrameSizeAndPositionPixelwise => {
                            crate::emacs_core::display_subrs::register_set_frame_size_and_position_pixelwise(ctx);
                        }
                        DisplayStep::FontGetSystemFont => {
                            crate::emacs_core::display_subrs::register_font_get_system_font(ctx);
                        }
                        DisplayStep::FrameAncestorP => {
                            crate::emacs_core::display_subrs::register_frame_ancestor_p(ctx);
                        }
                        DisplayStep::FrameInternalBorderWidth => {
                            crate::emacs_core::display_subrs::register_frame_internal_border_width(
                                ctx,
                            );
                        }
                        DisplayStep::FrameRightDividerWidth => {
                            crate::emacs_core::display_subrs::register_frame_right_divider_width(
                                ctx,
                            );
                        }
                        DisplayStep::FrameWindowStateChange => {
                            crate::emacs_core::display_subrs::register_frame_window_state_change(
                                ctx,
                            );
                        }
                        DisplayStep::HandleSwitchFrame => {
                            crate::emacs_core::display_subrs::register_handle_switch_frame(ctx);
                        }
                        DisplayStep::MakeFrameInvisible => {
                            crate::emacs_core::display_subrs::register_make_frame_invisible(ctx);
                        }
                        DisplayStep::NeomacsFrameSnapshot => {
                            crate::emacs_core::display_subrs::register_neomacs_frame_snapshot(ctx);
                        }
                        DisplayStep::RedirectFrameFocus => {
                            crate::emacs_core::display_subrs::register_redirect_frame_focus(ctx);
                        }
                        DisplayStep::ResizeMiniWindowInternal => {
                            crate::emacs_core::display_subrs::register_resize_mini_window_internal(
                                ctx,
                            );
                        }
                        DisplayStep::SetFrameWindowStateChange => {
                            crate::emacs_core::display_subrs::register_set_frame_window_state_change(
                            ctx,
                        );
                        }
                        DisplayStep::SetWindowNewNormal => {
                            crate::emacs_core::display_subrs::register_set_window_new_normal(ctx);
                        }
                        DisplayStep::TtyFrameAt => {
                            crate::emacs_core::display_subrs::register_tty_frame_at(ctx);
                        }
                        DisplayStep::WindowBottomDividerWidth => {
                            crate::emacs_core::display_subrs::register_window_bottom_divider_width(
                                ctx,
                            );
                        }
                        DisplayStep::Font => {
                            crate::emacs_core::display_subrs::register_font(ctx);
                        }
                        DisplayStep::NeomacsFrameEdges => {
                            crate::emacs_core::display_subrs::register_neomacs_frame_edges(ctx);
                        }
                        DisplayStep::EffectsModifiersChrome => {
                            crate::emacs_core::display_subrs::register_effects_modifiers_chrome(
                                ctx,
                            );
                        }
                        DisplayStep::WindowDiscardBufferFromWindow => {
                            crate::emacs_core::display_subrs::register_window_discard_buffer_from_window(ctx);
                        }
                        DisplayStep::WindowTextPixelSize => {
                            crate::emacs_core::display_subrs::register_window_text_pixel_size(ctx);
                        }
                        DisplayStep::ActiveMinibufferWindow => {
                            crate::emacs_core::display_subrs::register_active_minibuffer_window(
                                ctx,
                            );
                        }
                        DisplayStep::InternalLispFaceEqualP => {
                            crate::emacs_core::display_subrs::register_internal_lisp_face_equal_p(
                                ctx,
                            );
                        }
                        DisplayStep::MergeFaceAttribute => {
                            crate::emacs_core::display_subrs::register_merge_face_attribute(ctx);
                        }
                        DisplayStep::ColorDistance => {
                            crate::emacs_core::display_subrs::register_color_distance(ctx);
                        }
                        DisplayStep::InternalCopyLispFace => {
                            crate::emacs_core::display_subrs::register_internal_copy_lisp_face(ctx);
                        }
                        DisplayStep::RedrawDisplay => {
                            crate::emacs_core::display_subrs::register_redraw_display(ctx);
                        }
                        DisplayStep::ForceWindowUpdate => {
                            crate::emacs_core::display_subrs::register_force_window_update(ctx);
                        }
                        DisplayStep::XFocusFrame => {
                            crate::emacs_core::display_subrs::register_x_focus_frame(ctx);
                        }
                        DisplayStep::XPopupDialog => {
                            crate::emacs_core::display_subrs::register_x_popup_dialog(ctx);
                        }
                        DisplayStep::DeleteTerminal => {
                            crate::emacs_core::display_subrs::register_delete_terminal(ctx);
                        }
                        DisplayStep::ImageTransformsP => {
                            crate::emacs_core::display_subrs::register_image_transforms_p(ctx);
                        }
                        DisplayStep::CurrentBidiParagraphDirection => {
                            crate::emacs_core::display_subrs::register_current_bidi_paragraph_direction(
                            ctx,
                        );
                        }
                        DisplayStep::MoveToWindowLine => {
                            crate::emacs_core::display_subrs::register_move_to_window_line(ctx);
                        }
                    }
                }
                CanonicalStepKind::UpperSegment => {
                    for row in &names[step.first - 1..step.first - 1 + step.count] {
                        if !cfg_active(row.cfg) {
                            assert!(
                                compiled.get(row.name).is_none(),
                                "host-inactive upper row {:?} is compiled",
                                row.name
                            );
                            continue;
                        }
                        let &index = compiled.get(row.name).unwrap_or_else(|| {
                            panic!(
                                "host-active upper row {:?} has no compiled stand-in",
                                row.name
                            )
                        });
                        let (_, min, max, function) = UPPER_SUBRS[index];
                        assert_eq!(
                            (min, max),
                            (row.min, row.max),
                            "plan/compiled arity mismatch for upper row {:?}",
                            row.name
                        );
                        assert!(
                            !consumed[index] && !display_owned[index],
                            "compiled stand-in {:?} consumed twice",
                            row.name
                        );
                        consumed[index] = true;
                        ctx.register_subr(SubrSpec::new(
                            row.name,
                            NativeFn::ContextVec(function),
                            SubrArity::new(row.min, row.max),
                        ));
                    }
                }
            }
        }
        assert_eq!(
            eval_steps,
            CanonicalEvalStep::ALL.to_vec(),
            "the plan's eval steps do not match the exhaustive CanonicalEvalStep order"
        );
        assert_eq!(
            display_steps,
            DisplayStep::ALL.to_vec(),
            "the plan's display steps do not match the exhaustive DisplayStep order"
        );
        let plan_eval_ids: Vec<&str> = steps
            .iter()
            .filter(|step| step.kind == CanonicalStepKind::EvalCall)
            .map(|step| step.id)
            .collect();
        let enum_eval_ids: Vec<&str> = CanonicalEvalStep::ALL
            .iter()
            .map(|step| step.id())
            .collect();
        assert_eq!(
            plan_eval_ids, enum_eval_ids,
            "the plan's eval ids disagree with the exhaustive CanonicalEvalStep ids"
        );
        let plan_display_ids: Vec<&str> = steps
            .iter()
            .filter(|step| step.kind == CanonicalStepKind::DisplaySegment)
            .map(|step| step.id)
            .collect();
        let enum_display_ids: Vec<&str> = DisplayStep::ALL.iter().map(|step| step.id()).collect();
        assert_eq!(
            plan_display_ids, enum_display_ids,
            "the plan's display ids disagree with the exhaustive DisplayStep ids"
        );
        for (index, entry) in UPPER_SUBRS.iter().enumerate() {
            assert!(
                consumed[index] || display_owned[index],
                "compiled stand-in {:?} was never accounted for",
                entry.0
            );
            assert!(
                !(consumed[index] && display_owned[index]),
                "compiled stand-in {:?} is both consumed and display-owned",
                entry.0
            );
        }
        assert_eq!(
            consumed.iter().filter(|flag| **flag).count()
                + display_owned.iter().filter(|flag| **flag).count(),
            UPPER_SUBRS.len(),
            "compiled upper table must be accounted for exactly once"
        );
    }

    fn seed_batch_startup_frame(ctx: &mut Context) -> FrameId {
        let (frames, buffers) = ctx.frame_manager_and_buffers_mut();
        crate::emacs_core::window_cmds::seed_batch_startup_frame_in_state(frames, buffers)
    }

    fn configure_window_system_runtime(
        identity: neomacs_display_protocol::GraphicalDisplayIdentity,
    ) {
        use crate::emacs_core::terminal::pure::{
            TerminalRuntimeConfig, configure_terminal_runtime,
        };
        configure_terminal_runtime(TerminalRuntimeConfig::window_system(identity));
    }

    fn terminal_park_carrier_tls() {
        crate::emacs_core::terminal::pure::park_carrier_tls()
    }

    fn terminal_reset_thread_locals() {
        crate::emacs_core::terminal::pure::reset_terminal_thread_locals()
    }

    fn terminal_reset_handle() {
        crate::emacs_core::terminal::pure::reset_terminal_handle()
    }

    fn terminal_world_state_placeholder() -> Box<dyn std::any::Any> {
        crate::emacs_core::terminal::pure::world_state_placeholder()
    }

    fn terminal_swap_world_state(slot: &mut dyn std::any::Any) {
        crate::emacs_core::terminal::pure::swap_world_state(slot)
    }

    fn live_terminal_ids_in_keyboard_poll_order() -> Vec<u64> {
        crate::emacs_core::terminal::pure::live_terminal_ids_in_keyboard_poll_order()
    }

    fn char_property_window_lookup(
        frames: Option<&neovm_eval::emacs_core::eval::data::window_state::FrameManager>,
        window: neovm_edit::emacs_core::display_metrics::WindowId,
    ) -> neovm_edit::emacs_core::textprop::WindowBufferLookup {
        match frames {
            Some(frames) => {
                neovm_eval::emacs_core::eval::data::window_state::char_property_window_buffer(
                    frames, window,
                )
            }
            None => neovm_edit::emacs_core::textprop::WindowBufferLookup::NotAvailable,
        }
    }

    fn apply_mini_window_resize(
        frame: &mut neovm_eval::emacs_core::eval::data::window_state::Frame,
        resize: neovm_eval::emacs_core::eval::data::window_state::MiniWindowResize,
    ) {
        use crate::window::DisplayFrameExt;
        frame.apply_mini_window_resize(resize);
    }

    fn frame_parameter_entry(eval: &mut Context, frame: Value, key: Value) -> EvalResult {
        crate::emacs_core::frame::builtin_frame_parameter(eval, &[frame, key])
    }

    fn frontend_apply_xwidget_frontend_event(
        ctx: &mut Context,
        event: &neovm_eval::emacs_core::eval::keyboard::FrontendWebViewEvent,
    ) -> Result<bool, neovm_value::error::Flow> {
        use crate::emacs_core::xwidget::XwidgetContextExt;
        ctx.apply_xwidget_frontend_event(event)
    }

    fn display_compositor_scrolling_enabled(
        ctx: &Context,
        window: neovm_edit::emacs_core::display_metrics::WindowId,
    ) -> bool {
        use crate::emacs_core::neo::scroll::ScrollContextExt;
        ctx.compositor_scrolling_enabled(window)
    }

    fn display_permits_compositor_pixel_scroll(
        ctx: &Context,
        window: neovm_edit::emacs_core::display_metrics::WindowId,
    ) -> bool {
        use crate::emacs_core::neo::scroll::ScrollContextExt;
        ctx.permits_compositor_pixel_scroll(window)
    }

    fn display_redisplay_buffer_layout_inputs(
        ctx: &Context,
        buffer: neovm_edit::buffer::BufferId,
    ) -> Option<neovm_eval::emacs_core::eval::data::layout_input::BufferLayoutInputState> {
        use crate::window::display::WindowDisplayContextExt;
        ctx.redisplay_buffer_layout_inputs(buffer)
    }

    // Display-owned lifecycle groups are REAL (D1). The system/core groups
    // (dynamic-module, sqlite/dbusbind/timefns World state, fileio, the command
    // seams and the two system resets) stay the signed absent-layer bodies.
    static LIFECYCLE: LifecycleHooks = LifecycleHooks::new(
        DynamicModuleHooks::new(
            s::dynamic_module_reset,
            Some(s::dynamic_module_fingerprint),
            s::dynamic_module_panic_message,
            s::dynamic_module_load_module,
            s::dynamic_module_apply_module_function,
            s::dynamic_module_module_call_active,
            s::dynamic_module_collect_gc_roots,
        ),
        WorldStateHooks::new(
            s::sqlite_world_state_placeholder,
            s::sqlite_world_state_swap,
        ),
        DbusbindHooks::new(
            WorldStateHooks::new(
                s::dbusbind_world_state_placeholder,
                s::dbusbind_world_state_swap,
            ),
            s::dbusbind_drain_events,
            s::dbusbind_has_active_watches,
            s::dbusbind_reset_thread_locals,
        ),
        WorldStateHooks::new(
            s::timefns_world_state_placeholder,
            s::timefns_world_state_swap,
        ),
        BootHooks::new(
            crate::emacs_core::xfaces::ensure_startup_compat_variables,
            crate::emacs_core::xfaces::clear_created_lisp_face,
            crate::emacs_core::xfaces::remove_face_new_frame_defaults_entry,
            crate::emacs_core::xfaces::restore_created_faces_from_table,
            crate::emacs_core::display::gui_window_system_symbol,
            s::boot_apply_post_image_init,
            s::boot_apply_lread_constants,
            s::boot_install_host_environment_snapshot,
            s::boot_shell_file_name_install,
            s::boot_shell_file_name_install_dump_default,
            seed_batch_startup_frame,
            boot_register_core_bootstrap_vars,
            crate::emacs_core::xfaces::init_frame_lisp_faces,
            crate::emacs_core::xfaces::realization::prepare_runtime_faces,
            crate::emacs_core::xfaces::sync_runtime_face_table_from_frame_lisp_faces,
            crate::emacs_core::xfaces::ensure_lisp_face_id_property,
            crate::emacs_core::xfaces::clear_font_cache_state,
        ),
        FileIoHooks::new(
            s::fileio_expand_file_name_entry,
            s::fileio_dispatch_file_handler,
            s::fileio_resolve_filename,
            s::fileio_lock_file,
            s::fileio_unlock_file,
        ),
        TerminalHooks::new(
            crate::emacs_core::terminal::pure::terminal_handle_value,
            crate::emacs_core::terminal::pure::delete_terminal_noelisp_owned,
            crate::emacs_core::terminal::pure::builtin_frame_terminal,
            crate::emacs_core::terminal::pure::print_terminal_handle,
            configure_window_system_runtime,
            terminal_park_carrier_tls,
            Some(crate::emacs_core::terminal::pure::carrier_tls_fingerprint),
            terminal_reset_thread_locals,
            terminal_reset_handle,
            terminal_world_state_placeholder,
            terminal_swap_world_state,
            live_terminal_ids_in_keyboard_poll_order,
            crate::emacs_core::terminal::pure::current_terminal_registry_handle,
            crate::emacs_core::terminal::pure::install_terminal_registry_handle,
            crate::emacs_core::terminal::pure::install_owner_terminal_registry_handle,
        ),
        FontsetHooks::new(
            crate::emacs_core::fontset::fontset_alias_alist_startup_value,
            crate::emacs_core::fontset::fontset_list_value,
            crate::emacs_core::fontset::reset_fontset_registry,
            crate::emacs_core::fontset::snapshot_fontset_registry,
            crate::emacs_core::fontset::restore_fontset_registry,
        ),
        CommandSeamHooks::new(
            s::command_window_configuration_capture,
            s::command_window_configuration_restore,
            s::command_run_redisplay_window_change_hooks,
            s::command_unwind_minibuffer_session,
            s::command_unwind_minibuffer_buffer,
            s::command_get_char_property,
            s::command_try_completion,
            s::command_all_completions,
            s::command_test_completion,
            s::command_menu_bar_top_level_items,
            s::command_key_binding,
            s::command_where_is_internal,
        ),
        WindowTreeHooks::new(
            char_property_window_lookup,
            crate::window::window_resize_check,
            crate::window::window_resize_apply,
            apply_mini_window_resize,
            crate::window::resize_pixelwise_with_buffer_constraints,
            frame_parameter_entry,
        ),
        XfacesHooks::new(
            crate::emacs_core::xfaces::park_carrier_tls,
            Some(crate::emacs_core::xfaces::carrier_tls_fingerprint),
            crate::emacs_core::xfaces::current_font_registry_handle,
            crate::emacs_core::xfaces::install_font_registry_handle,
        ),
        s::timefns_reset,
        s::sqlite_reset,
    );

    static SELECT: SelectHooks = SelectHooks::new(
        s::select_handle,
        s::select_notify,
        s::select_nudge,
        s::select_control_source,
        s::select_wait_until,
        s::select_arm,
        s::select_wait_registered,
        s::select_cancel_arm,
    );

    static MAIN_WAIT: MainWaitHooks =
        MainWaitHooks::new(s::main_wait_prepare, s::main_wait_wait, s::main_wait_cancel);

    static PROCESS: ProcessHooks = ProcessHooks::new(
        s::process_unlink,
        s::process_delete,
        s::process_open_channel,
        s::process_print_handle,
        s::process_wait_notifier,
        s::process_terminate_and_defer_reaping,
        SELECT,
        MAIN_WAIT,
    );

    static INPUT: InputHooks = InputHooks::new(
        s::input_interrupt_for_input_event_if_requested,
        s::input_os_signal_pending,
        s::input_take_termination_signal,
        s::input_drain_pending_os_signals,
        s::input_signal_wake_pipe_read_fd,
        s::input_drain_signal_wake_pipe,
    );

    static CALL_INTERACTIVELY: CallInteractivelyHooks =
        CallInteractivelyHooks::new(s::call_interactively_resolve);

    static CASE_TABLES: CaseTableHooks = CaseTableHooks::new(
        s::case_tables_canon,
        s::case_tables_upcase,
        s::case_tables_sync,
        s::case_tables_standard,
        s::case_tables_override_for_current_buffer,
    );

    static BUFFER_KILL: BufferKillHooks = BufferKillHooks::new(s::buffer_kill_before);

    static KMACRO: KmacroHooks = KmacroHooks::new(
        s::kmacro_start,
        s::kmacro_end,
        s::kmacro_call_last,
        s::kmacro_execute,
        s::kmacro_store_event,
    );

    static PERF: PerfHooks =
        PerfHooks::new(s::perf_record, s::perf_reset, Some(s::perf_fingerprint));

    static WAIT: WaitServiceHooks = WaitServiceHooks {
        service_input_pending_with_timers: s::wait_service_input_pending_with_timers,
        service_read_char_preliminary_redisplay: s::wait_service_read_char_preliminary_redisplay,
        service_timers_without_redisplay: s::wait_service_timers_without_redisplay,
        wait_for_command_input_with_idle_policy: s::wait_for_command_input_with_idle_policy,
        wait_for_resize_ack_until: s::wait_for_resize_ack_until,
    };

    static TIMER: TimerServiceHooks = TimerServiceHooks {
        next_ordinary_gnu_timer_timeout_before: s::timer_next_ordinary_gnu_timer_timeout_before,
        next_idle_gnu_timer_timeout: s::timer_next_idle_gnu_timer_timeout,
    };

    static FRONTEND: FrontendServiceHooks = FrontendServiceHooks {
        apply_xwidget_frontend_event: frontend_apply_xwidget_frontend_event,
    };

    static DISPLAY_SERVICE: DisplayServiceHooks = DisplayServiceHooks {
        compositor_scrolling_enabled: display_compositor_scrolling_enabled,
        permits_compositor_pixel_scroll: display_permits_compositor_pixel_scroll,
        redisplay_buffer_layout_inputs: display_redisplay_buffer_layout_inputs,
    };

    static FILE_NOTIFY: FileNotifyHostQueries = FileNotifyHostQueries {
        expand_file_name: s::file_notify_expand_file_name,
        directory_file_name: s::file_notify_directory_file_name,
        file_exists_p: s::file_notify_file_exists_p,
    };

    /// The one cfg(test) display unit runtime (H5a). The set is assembled here
    /// because the display-query descriptor lives in this crate (a function,
    /// not a const initializer); `perf_enabled` is false, so the PANIC perf
    /// slots are unreachable, and the sqlite installer is the recorded
    /// ASSERT-EMPTY publication (H2a). The proof is minted only by the one
    /// production publication entry after the full set installs, and cached:
    /// every constructor switch shares this one proof.
    pub(crate) fn unit_runtime() -> EvalHooksInstalled {
        static SET: OnceLock<EvalHookSet> = OnceLock::new();
        static PROOF: OnceLock<EvalHooksInstalled> = OnceLock::new();
        let set = SET.get_or_init(|| {
            EvalHookSet::new(
                &LIFECYCLE,
                &PROCESS,
                &INPUT,
                &CALL_INTERACTIVELY,
                &CASE_TABLES,
                &BUFFER_KILL,
                &KMACRO,
                crate::emacs_core::xdisp::canonical_display_query_hooks(),
                false,
                &PERF,
                register_core_subrs,
                s::absent_sqlite_installer,
                &WAIT,
                &TIMER,
                &FRONTEND,
                &DISPLAY_SERVICE,
                &FILE_NOTIFY,
            )
        });
        *PROOF.get_or_init(|| {
            neovm_eval::emacs_core::eval::hooks_proof::install_eval_hooks(set)
                .expect("the display unit hook set publishes at first use")
        })
    }
    /// T7 construction census (LEAD 06:4xZ (3)): plain
    /// `Context::new(unit_runtime())` plus one trivial form must reach only
    /// absent-by-construction UNLINKED slots. A construction-reached Refused
    /// slot is a P2 bug - construction must never PANIC - and a new slot
    /// reached by construction changes this pinned list in the same pick.
    mod construction_census {
        use super::absent_layer_slots::{AbsentLayerHit, take_absent_layer_hits_for_test};
        use super::unit_runtime;

        const EXPECTED_UNLINKED: &[&str] = &[
            "ABSENT-LAYER system::sqlite.reset",
            "ABSENT-LAYER system::dbusbind.reset_thread_locals",
            "ABSENT-LAYER system::timefns.reset",
            "ABSENT-LAYER system::input.signal_wake_pipe_read_fd",
        ];

        #[test]
        fn construction_reaches_no_panicking_absent_layer_slot() {
            let _ = take_absent_layer_hits_for_test();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut context = neovm_eval::emacs_core::eval::Context::new(unit_runtime());
                context
                    .eval_str("(+ 1 2)")
                    .map(|_| ())
                    .map_err(|error| format!("{error:?}"))
            }));
            let hits = take_absent_layer_hits_for_test();
            let refused: Vec<_> = hits
                .iter()
                .filter(|(kind, _)| *kind == AbsentLayerHit::Refused)
                .collect();
            match &outcome {
                Ok(Ok(())) => {}
                Ok(Err(error)) => panic!("the trivial form failed: {error}"),
                Err(_) => panic!("construction panicked; refused hits: {refused:?}"),
            }
            assert!(
                refused.is_empty(),
                "construction reached PANIC slot(s): {refused:?}"
            );
            let unlinked: Vec<&str> = hits
                .iter()
                .filter_map(|(kind, marker)| (*kind == AbsentLayerHit::Unlinked).then_some(*marker))
                .collect();
            assert_eq!(
                unlinked, EXPECTED_UNLINKED,
                "the construction-reached UNLINKED slot list changed"
            );
        }
    }
}

#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use unit_runtime::unit_runtime;

/// Display-side T6 pin: a fresh display unit-runtime context registers exactly
/// the shared expectation builder's sequence (the plan's host-active rows plus
/// the event marker). This is display's half of the literal-equality gate;
/// core's production trace and eval's unit runtime are the other halves.
///
/// Executed evidence (LEAD 06:4xZ): on the real plan and compiled table the
/// trace equals the builder (GREEN). Planting one extra registration AFTER
/// every walk assert - so only this equality can catch it - turns the
/// assertion RED with `Subr { name: "oracleb-planted-extra", min: 0,
/// max: None }` present in the divergent trace.
#[cfg(test)]
mod plan_walk_trace {
    use neovm_eval::emacs_core::eval::Context;
    use neovm_eval::emacs_core::subr::{
        reset_registered_subr_trace_for_test, take_registered_subr_trace_for_test,
    };

    #[test]
    fn the_unit_runtime_walks_the_canonical_plan_in_host_active_order() {
        reset_registered_subr_trace_for_test();
        let _context = Context::new(crate::test_utils::unit_runtime());
        let trace = take_registered_subr_trace_for_test();
        assert_eq!(
            trace,
            neovm_eval::test_utils::canonical_host_active_trace_for_test(),
            "the display unit runtime's registration trace diverges from the canonical plan"
        );
    }
}
