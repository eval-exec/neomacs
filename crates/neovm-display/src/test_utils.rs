//! Display's local test-helper hub.
//!
//! TEST-SUPPORT-DESIGN rev 2 rule 8: every crate's test tree re-exports the
//! shared, upper-free `neovm_value::test_utils` helpers instead of copying
//! them. This keeps the historical `crate::test_utils::init_test_tracing()`
//! call sites in display's own tests resolving without a core edge (an upward
//! dependency is forbidden). Compiled only for display's tests and for an
//! upper crate's test-support build; absent from every product profile.

pub use neovm_value::test_utils::*;

/// Display-host recording fixture shared by display window-command tests and
/// the re-homed core host tests (P1 B1): one home, reachable under
/// test-support from above.
pub mod window_cmds_host {

    use crate::face::{FontSlant, FontWeight, FontWidth};
    use crate::window::FrameFullscreen;
    use neovm_eval::emacs_core::eval::DisplayHost;
    use neovm_eval::emacs_core::eval::GuiFrameHostRequest;
    use neovm_eval::emacs_core::eval::{FontPxProbeResult, GuiFrameHostSize, ResolvedFrameFont};
    use neovm_value::value::Value;
    use std::cell::Cell;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[derive(Clone, Default)]
    pub struct RecordingDisplayHost {
        pub alpha_operations: Rc<RefCell<Vec<(crate::window::FrameId, [f32; 2], f32)>>>,
        pub fail_alpha_sync: bool,
        pub fail_focus_sync: Rc<Cell<bool>>,
        pub alpha_limits: Rc<RefCell<Vec<f32>>>,
        pub realized: Rc<RefCell<Vec<GuiFrameHostRequest>>>,
        pub resized: Rc<RefCell<Vec<GuiFrameHostRequest>>>,
        /// Resize-rejection cell. `pub(crate)` in this test-support module;
        /// cross-crate reads go through [`Self::fail_resize`].
        pub(crate) fail_resize: Rc<Cell<bool>>,
        /// Recorded font requests. `pub(crate)` in this test-support module;
        /// cross-crate reads go through [`Self::font_requests`].
        pub(crate) font_requests: Rc<
            RefCell<
                Vec<(
                    crate::window::FrameId,
                    crate::emacs_core::display_host::FrameFontRequest,
                )>,
            >,
        >,
        pub destroyed_gui_frames: Rc<RefCell<Vec<crate::window::FrameId>>>,
        pub shown_child_frames: Rc<RefCell<Vec<crate::window::FrameId>>>,
        pub removed_child_frames: Rc<RefCell<Vec<crate::window::FrameId>>>,
        pub fullscreen_changes: Rc<RefCell<Vec<(crate::window::FrameId, FrameFullscreen)>>>,
        pub visibility_changes:
            Rc<RefCell<Vec<(crate::window::FrameId, crate::window::FrameVisibility)>>>,
        pub geometry_hints:
            Rc<RefCell<Vec<(crate::window::FrameId, crate::window::GuiFrameGeometryHints)>>>,
        pub primary_size: Option<GuiFrameHostSize>,
        pub resolved_frame_font: Option<ResolvedFrameFont>,
        pub initial_gui_defaults: Option<crate::emacs_core::display_host::InitialGuiFrameDefaults>,
        pub finished: Rc<RefCell<Vec<GuiFrameHostRequest>>>,
        pub positions: Rc<RefCell<Vec<(crate::window::FrameId, i32, i32)>>>,
    }

    impl RecordingDisplayHost {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn with_primary_size(width: u32, height: u32) -> Self {
            Self {
                primary_size: Some(GuiFrameHostSize { width, height }),
                ..Self::default()
            }
        }

        pub fn with_resolved_frame_font(resolved_frame_font: ResolvedFrameFont) -> Self {
            Self {
                resolved_frame_font: Some(resolved_frame_font),
                ..Self::default()
            }
        }

        /// Resize-rejection cell (VIS read arm, peer 5543). The field stays
        /// `pub(crate)`; this getter is the cross-crate route. Callers flip
        /// the cell through the returned `Rc`.
        pub fn fail_resize(&self) -> &Rc<Cell<bool>> {
            &self.fail_resize
        }

        /// Recorded font requests (VIS read arm, peer 5543). The field stays
        /// `pub(crate)`; this getter is the cross-crate route.
        pub fn font_requests(
            &self,
        ) -> &Rc<
            RefCell<
                Vec<(
                    crate::window::FrameId,
                    crate::emacs_core::display_host::FrameFontRequest,
                )>,
            >,
        > {
            &self.font_requests
        }
    }

    impl DisplayHost for RecordingDisplayHost {
        fn prepare_initial_gui_frame(
            &mut self,
        ) -> Result<Option<crate::emacs_core::display_host::InitialGuiFrameDefaults>, String>
        {
            Ok(self.initial_gui_defaults.clone())
        }

        fn finish_gui_frame_creation(
            &mut self,
            request: GuiFrameHostRequest,
        ) -> Result<(), String> {
            self.finished.borrow_mut().push(request);
            Ok(())
        }
        fn set_gui_frame_alpha_lower_limit(&mut self, limit: f32) {
            self.alpha_limits.borrow_mut().push(limit);
        }
        fn set_gui_frame_alpha(
            &mut self,
            frame: crate::window::FrameId,
            pair: [f32; 2],
            limit: f32,
        ) -> Result<(), String> {
            self.alpha_operations
                .borrow_mut()
                .push((frame, pair, limit));
            if self.fail_alpha_sync {
                return Err("opacity channel closed".into());
            }
            Ok(())
        }
        fn set_gui_frame_focus_redirects(
            &mut self,
            _redirects: Vec<(crate::window::FrameId, Option<crate::window::FrameId>)>,
        ) -> Result<(), String> {
            if self.fail_focus_sync.get() {
                return Err("focus channel closed".into());
            }
            Ok(())
        }
        fn realize_gui_frame(&mut self, request: GuiFrameHostRequest) -> Result<(), String> {
            self.realized.borrow_mut().push(request);
            Ok(())
        }

        fn resize_gui_frame(&mut self, request: GuiFrameHostRequest) -> Result<(), String> {
            self.resized.borrow_mut().push(request);
            if self.fail_resize.get() {
                return Err("resize rejected".into());
            }
            Ok(())
        }

        fn set_gui_frame_position(
            &mut self,
            frame_id: crate::window::FrameId,
            x: i32,
            y: i32,
        ) -> Result<(), String> {
            self.positions.borrow_mut().push((frame_id, x, y));
            Ok(())
        }

        fn set_gui_frame_fullscreen(
            &mut self,
            frame_id: crate::window::FrameId,
            fullscreen: FrameFullscreen,
        ) -> Result<(), String> {
            self.fullscreen_changes
                .borrow_mut()
                .push((frame_id, fullscreen));
            Ok(())
        }

        fn set_gui_frame_visibility(
            &mut self,
            frame_id: crate::window::FrameId,
            visibility: crate::window::FrameVisibility,
        ) -> Result<(), String> {
            self.visibility_changes
                .borrow_mut()
                .push((frame_id, visibility));
            Ok(())
        }

        fn set_gui_frame_geometry_hints(
            &mut self,
            frame_id: crate::window::FrameId,
            geometry_hints: crate::window::GuiFrameGeometryHints,
        ) -> Result<(), String> {
            self.geometry_hints
                .borrow_mut()
                .push((frame_id, geometry_hints));
            Ok(())
        }

        fn current_primary_window_size(&self) -> Option<GuiFrameHostSize> {
            self.primary_size
        }

        fn opening_gui_frame_pending(&self) -> bool {
            self.realized.borrow().is_empty()
        }

        fn destroy_gui_frame(&mut self, frame_id: crate::window::FrameId) -> Result<(), String> {
            self.destroyed_gui_frames.borrow_mut().push(frame_id);
            Ok(())
        }

        fn show_gui_child_frame(&mut self, frame_id: crate::window::FrameId) -> Result<(), String> {
            self.shown_child_frames.borrow_mut().push(frame_id);
            Ok(())
        }

        fn remove_gui_child_frame(
            &mut self,
            frame_id: crate::window::FrameId,
        ) -> Result<(), String> {
            self.removed_child_frames.borrow_mut().push(frame_id);
            Ok(())
        }

        fn resolve_frame_font(
            &mut self,
            frame_id: crate::window::FrameId,
            request: crate::emacs_core::display_host::FrameFontRequest,
        ) -> Result<Option<ResolvedFrameFont>, String> {
            self.font_requests.borrow_mut().push((frame_id, request));
            Ok(self.resolved_frame_font.clone())
        }
    }

    pub fn resolved_frame_font(
        family: &str,
        postscript_name: &str,
        height_tenths: i32,
        metrics: FontPxProbeResult,
    ) -> ResolvedFrameFont {
        ResolvedFrameFont {
            font: neovm_eval::emacs_core::eval::test_resolved_opened_font(
                family,
                None,
                None,
                FontWeight::NORMAL,
                FontSlant::Normal,
                FontWidth::Normal,
                Some(postscript_name),
                metrics,
                None,
            ),
            height_tenths,
        }
    }

    pub fn remapped_mono_font_metrics() -> ResolvedFrameFont {
        resolved_frame_font(
            "Remapped Mono",
            "RemappedMono-Regular",
            240,
            FontPxProbeResult {
                pixel_size: 28,
                height: 32,
                ascent: 24,
                descent: 8,
                max_width: 16,
                space_width: 16,
                average_width: 16,
            },
        )
    }

    pub fn due_gnu_timer(callback: Value, args: Value) -> Value {
        let when = SystemTime::now()
            .checked_sub(Duration::from_millis(10))
            .expect("timer deadline should fit in system time")
            .duration_since(UNIX_EPOCH)
            .expect("timer deadline should be after unix epoch");
        let secs = when.as_secs() as i64;

        Value::vector(vec![
            Value::NIL,
            Value::fixnum(secs >> 16),
            Value::fixnum(secs & 0xFFFF),
            Value::fixnum(when.subsec_micros() as i64),
            Value::NIL,
            callback,
            args,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
        ])
    }
}

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
    use neovm_eval::emacs_core::eval::input_hooks::{InputHooks, SignalWakeSource};
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

    /// D3/T4: display-owned bootstrap variables are REAL; the remaining
    /// system/core names use the signed snapshot's typed default (the
    /// ABSENT-LAYER marker for Lisp_Object slots, the production default for
    /// forwarded typed slots such as read-process-output-max). T4 replaced
    /// the old name-only snapshot with this paired table.
    fn boot_register_core_bootstrap_vars(obarray: &mut neovm_value::symbol::Obarray) {
        crate::emacs_core::xfaces::register_bootstrap_vars(obarray);
        let mut absent = Vec::with_capacity(UPPER_BOOTSTRAP_VARS.len());
        for (name, default) in UPPER_BOOTSTRAP_VARS {
            if !XFACES_BOOTSTRAP_VARS.contains(name) {
                absent.push((*name, *default));
            }
        }
        s::bind_absent_bootstrap_vars(obarray, &absent);
    }

    /// The production arm is core's real platform hook; the display unit
    /// harness does not need the w32 symbol block, so this slot is a no-op.
    fn boot_register_platform_bootstrap_vars(_obarray: &mut neovm_value::symbol::Obarray) {}

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
            boot_register_platform_bootstrap_vars,
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
        SignalWakeSource::SelfPipe {
            read_fd: s::input_signal_wake_pipe_read_fd,
            drain: s::input_drain_signal_wake_pipe,
        },
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

    /// Build the display unit fixture Context: the canonical boot path for a
    /// cfg(test) display test.  The hook set publishes through the one entry
    /// (unit_runtime), so code reached from a builtin under test (the printer's
    /// terminal-handle path, face/fontset queries) sees the real installed
    /// lifecycle tables instead of the fail-closed expect.
    pub(crate) fn unit_context() -> Context {
        Context::new(unit_runtime())
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
pub(crate) use unit_runtime::{unit_context, unit_runtime};
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

/// A counting frontend still has to complete an accepted display transaction.
/// These fixtures measure repaint scheduling, not glyph metrics. Publish only
/// their trusted live viewports, retain fallback body/chrome measurement, then
/// acknowledge GNU ownership after validated prepare and renderer activation.
/// All state belongs to the exclusively borrowed Context/Frame; no policy or
/// Lisp values are cached globally or in thread-local storage.
///
/// This is the display-side copy of core's test fixture of the same name: the
/// fixture names only display and eval APIs, display has no core edge, and the
/// two scroll/popup tests that drive it live in this crate.
#[cfg(any(test, feature = "test-support"))]
pub mod mock_redisplay {
    use crate::window::DisplayFrameExt;
    use crate::window::{PresentedWindowRegions, WindowDisplaySnapshot};
    use neomacs_display_protocol::types::Rect;
    use neovm_eval::emacs_core::eval::Context;

    pub fn accept_all_frames(eval: &mut Context) {
        for frame_id in eval.frame_manager().frame_list() {
            let (windows, snapshots, buffers) = {
                let frame = eval.frame_manager().get(frame_id).expect("live mock frame");
                let windows = frame.all_leaf_ids();
                let mut snapshots = Vec::with_capacity(windows.len());
                let mut buffers = Vec::with_capacity(windows.len());
                for &window in &windows {
                    let leaf = frame.find_window(window).expect("live mock leaf");
                    let bounds = leaf.bounds();
                    snapshots.push(WindowDisplaySnapshot {
                        window_id: window,
                        regions: PresentedWindowRegions {
                            outer: Rect::new(bounds.x, bounds.y, bounds.width, bounds.height),
                            ..PresentedWindowRegions::default()
                        },
                        // No glyph/font measurement in a counter fixture. Preserve
                        // the real body's fallback dimensions for window hooks.
                        regions_materialized: false,
                        ..WindowDisplaySnapshot::default()
                    });
                    if let Some(buffer) = leaf.buffer_id() {
                        buffers.push(buffer);
                    }
                }
                (windows, snapshots, buffers)
            };
            let presentation =
                crate::window::geometry::PresentationId::new(eval.begin_interaction_presentation());
            let frame = eval
                .frame_manager_mut()
                .get_mut(frame_id)
                .expect("live mock frame");
            frame
                .prepare_live_window_presentation(presentation, snapshots)
                .expect("validated mock display preparation");
            frame
                .activate_display_presentation(presentation)
                .expect("mock renderer accepted presentation");
            assert_eq!(frame.active_presentation(), Some(presentation));
            assert!(
                windows
                    .iter()
                    .all(|&window| frame.redisplay_snapshot(window).is_some())
            );
            for buffer in buffers {
                eval.buffer_manager()
                    .get(buffer)
                    .expect("displayed mock buffer")
                    .reset_unchanged_region();
            }
            eval.note_gnu_frame_display_accepted(frame_id, windows);
            // The mock completed this physical frame draw, like the TTY renderer.
            let _ = eval.gnu_take_tty_frame_redraw(frame_id);
        }
    }
}

/// Shared upper-layer re-home projection setup (peer P1 precision): the
/// caller supplies the harness-booted Context, so display's unit runtime and
/// core's new_context use the same projection lines from one home.
pub mod upper_layer_helpers {
    use crate::window::DisplayFrameManagerExt;
    use crate::window::{FrameId, WindowId};
    use neovm_edit::buffer::BufferId;
    use neovm_eval::emacs_core::eval::Context;

    pub(crate) fn attempt_freshness_projections(
        mut eval: Context,
    ) -> (Context, BufferId, FrameId, WindowId) {
        let live = eval.buffer_manager().current_buffer().unwrap().id();
        let frame = eval
            .frame_manager_mut()
            .create_frame("attempt-reuse", 800, 600, live);
        let window = eval.frame_manager().get(frame).unwrap().selected_window;
        (eval, live, frame, window)
    }

    pub(crate) fn buffer_layout_projection_setup(mut eval: Context) -> (Context, BufferId) {
        eval.eval_str("(setq fontification-functions nil)").unwrap();
        let buffer = eval.buffer_manager().current_buffer().unwrap().id();
        (eval, buffer)
    }
}

/// Shared window-command upper-layer harness helpers (peer P1 precision): the
/// real helper bodies live here once; the caller supplies the harness-booted
/// Context, so display's unit-runtime tests and the core re-home share them.
pub mod window_cmds_upper_layer_helpers {
    use crate::window::DisplayFrameExt;
    use crate::window::DisplayFrameManagerExt;
    use neovm_eval::emacs_core::eval::Context;
    use neovm_eval::emacs_core::print::format_eval_result;
    use neovm_value::value::Value;

    pub(crate) fn eval_with_frame(
        make_context: impl FnOnce() -> Context,
        src: &str,
    ) -> Vec<String> {
        let mut ev = make_context();
        // Create a buffer for the initial window.
        let buf = ev.edit.buffers.create_buffer("*scratch*");
        ev.edit.buffers.set_current(buf);
        // Create a frame so window/frame builtins have something to work with.
        ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
        // Tests that exercise `make-frame` need a usable terminal (production
        // --batch deliberately has none, so frame creation errors like GNU).
        crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
        ev.eval_str_each(src)
            .iter()
            .map(format_eval_result)
            .collect()
    }

    pub(crate) fn eval_one_with_frame(make_context: impl FnOnce() -> Context, src: &str) -> String {
        eval_with_frame(make_context, src)
            .into_iter()
            .next()
            .unwrap()
    }

    pub(crate) fn eval_with_gui_frame(
        make_context: impl FnOnce() -> Context,
        src: &str,
    ) -> Vec<String> {
        let mut ev = make_context();
        let buf = ev.edit.buffers.create_buffer("*scratch*");
        ev.edit.buffers.set_current(buf);
        let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
        {
            let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
            frame.set_window_system(Some(Value::symbol("neo")));
        }
        ev.eval_str_each(src)
            .iter()
            .map(format_eval_result)
            .collect()
    }
}

/// Shared xfaces upper-layer helpers (peer P1 precision): real bodies
/// live here once; the caller supplies the harness-booted Context where one
/// is constructed.
pub mod xfaces_upper_layer_helpers {
    pub(crate) struct PointSizeFontHost;

    impl DisplayHost for PointSizeFontHost {
        fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }

        fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }

        fn resolve_font_for_char(
            &mut self,
            request: FontResolveRequest,
        ) -> Result<Option<ResolvedFontMatch>, String> {
            let height = match request.face.height {
                Some(FaceHeight::Absolute(height)) => f64::from(height),
                Some(FaceHeight::Relative(scale)) => 150.0 * scale,
                None => 150.0,
            };
            let pixel_size = (height * 96.0 / 720.0).round() as i32;
            Ok(Some(ResolvedFontMatch {
                font: neovm_eval::emacs_core::eval::test_resolved_opened_font(
                    "Test Mono",
                    None,
                    None,
                    FontWeight::NORMAL,
                    FontSlant::Normal,
                    FontWidth::Normal,
                    Some("TestMono"),
                    FontPxProbeResult {
                        pixel_size: pixel_size.try_into().unwrap(),
                        height: pixel_size + 4,
                        ascent: pixel_size,
                        descent: 4,
                        max_width: pixel_size / 2,
                        space_width: pixel_size / 2,
                        average_width: pixel_size / 2,
                    },
                    None,
                ),
                glyph_code: Some(request.character.code()),
            }))
        }
    }

    use crate::emacs_core::display_host::{FontResolveRequest, FrameFontRequest, FrameFontSize};
    use neovm_eval::emacs_core::eval::{DisplayHost, GuiFrameHostRequest, ResolvedFontMatch};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    pub fn frame_font_at_height(height_tenths: i32) -> ResolvedFrameFont {
        frame_font_at_height_with_line_height(height_tenths, 18)
    }

    pub fn frame_font_at_height_with_line_height(
        height_tenths: i32,
        line_height: i32,
    ) -> ResolvedFrameFont {
        ResolvedFrameFont {
            font: neovm_eval::emacs_core::eval::test_resolved_opened_font(
                "Monospace",
                None,
                None,
                FontWeight::NORMAL,
                FontSlant::Normal,
                FontWidth::Normal,
                Some("Monospace-Regular"),
                FontPxProbeResult {
                    pixel_size: 15,
                    height: line_height,
                    ascent: 14,
                    descent: 4,
                    max_width: 9,
                    space_width: 8,
                    average_width: 8,
                },
                None,
            ),
            height_tenths,
        }
    }

    pub struct CapturingFrameFontDisplayHost {
        pub(crate) requested_size: Rc<Cell<FrameFontSize>>,
        pub(crate) realized: ResolvedFrameFont,
    }

    pub struct FrameSpecificFontDisplayHost {
        pub(crate) selected_frame: FrameId,
        pub(crate) requests: Rc<RefCell<Vec<FrameId>>>,
    }

    impl DisplayHost for FrameSpecificFontDisplayHost {
        fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }

        fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }

        fn resolve_frame_font(
            &mut self,
            frame_id: FrameId,
            request: FrameFontRequest,
        ) -> Result<Option<ResolvedFrameFont>, String> {
            assert_eq!(
                request.size(),
                FrameFontSize::pixels(15).expect("positive pixel size")
            );
            self.requests.borrow_mut().push(frame_id);
            let height_tenths = if frame_id == self.selected_frame {
                // Cocoa-like logical coordinates.
                150
            } else {
                // Windows/Wayland-like logical coordinates.
                113
            };
            Ok(Some(frame_font_at_height(height_tenths)))
        }
    }

    impl DisplayHost for CapturingFrameFontDisplayHost {
        fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }

        fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }

        fn resolve_frame_font(
            &mut self,
            _frame_id: FrameId,
            request: FrameFontRequest,
        ) -> Result<Option<ResolvedFrameFont>, String> {
            self.requested_size.set(request.size());
            Ok(Some(self.realized.clone()))
        }
    }

    pub struct LiveFrameFontDisplayHost {
        pub(crate) realized: Option<ResolvedFrameFont>,
    }

    impl DisplayHost for LiveFrameFontDisplayHost {
        fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }

        fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }

        fn resolve_frame_font(
            &mut self,
            _frame_id: crate::window::FrameId,
            _request: crate::emacs_core::display_host::FrameFontRequest,
        ) -> Result<Option<ResolvedFrameFont>, String> {
            Ok(self.realized.clone())
        }

        fn resolve_font_for_char(
            &mut self,
            request: FontResolveRequest,
        ) -> Result<Option<ResolvedFontMatch>, String> {
            Ok(self.realized.clone().map(|resolved| ResolvedFontMatch {
                font: resolved.font,
                glyph_code: Some(request.character.code()),
            }))
        }
    }
    use crate::emacs_core::font::font_spec;
    use crate::emacs_core::window_cmds::ensure_selected_frame_id;
    use crate::face::{FaceHeight, FontSlant, FontWeight, FontWidth};
    use crate::window::DisplayFrameManagerExt;
    use crate::window::FrameId;
    use neovm_eval::emacs_core::eval::Context;
    use neovm_eval::emacs_core::eval::{FontPxProbeResult, ResolvedFrameFont};
    use neovm_value::value::Value;

    pub fn integer_font_spec() -> Value {
        font_spec(vec![
            Value::keyword("family"),
            Value::string("Monospace"),
            Value::keyword("size"),
            Value::fixnum(15),
        ])
        .expect("create font spec")
    }
    pub(crate) fn ensure_selected_gui_frame(eval: &mut Context) -> FrameId {
        let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(eval);
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame_id
    }
    pub fn resolved_frame_font(
        family: &str,
        postscript_name: &str,
        height_tenths: i32,
        metrics: FontPxProbeResult,
    ) -> ResolvedFrameFont {
        ResolvedFrameFont {
            font: neovm_eval::emacs_core::eval::test_resolved_opened_font(
                family,
                None,
                None,
                FontWeight::NORMAL,
                FontSlant::Normal,
                FontWidth::Normal,
                Some(postscript_name),
                metrics,
                None,
            ),
            height_tenths,
        }
    }
    pub(crate) fn font_remapping_context(make_context: impl FnOnce() -> Context) -> Context {
        let mut eval = make_context();
        let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .set_window_system(Some(neovm_value::value::Value::symbol("neo")));
        eval.eval_str("(internal-set-lisp-face-attribute 'default :height 150 nil)")
            .unwrap();
        eval.set_display_host(Box::new(PointSizeFontHost));
        eval
    }
}

/// Shared upper-layer helpers for mode_line_gc_roots_test (peer P1 precision):
/// real bodies live here once; callers supply the harness Context where needed.
pub mod mode_line_gc_roots_test_upper_layer_helpers {
    use crate::emacs_core::xdisp::format_mode_line_for_display_with_sources;
    use neovm_value::value::Value;
    // Inspect every retained cons before following it; an added formatter-owned
    // property may precede the fixture key. Property order is not an ownership
    // contract, and the new ordinal's spine must survive collection too.
    pub(crate) fn assert_owned_property(
        eval: &Context,
        mut plist: Value,
        property: Value,
        expected: Value,
    ) {
        let mut found = false;
        while plist.is_cons() {
            assert!(
                eval.tagged_heap().owns_heap_value_for_test(plist),
                "retained property name cell"
            );
            let name = plist.cons_car();
            assert_ne!(
                name.bits(),
                Value::DEAD.bits(),
                "a retained property name was swept"
            );
            let rest = plist.cons_cdr();
            assert!(rest.is_cons(), "retained property value cell");
            assert!(
                eval.tagged_heap().owns_heap_value_for_test(rest),
                "retained property value ownership"
            );
            let value = rest.cons_car();
            assert_ne!(
                value.bits(),
                Value::DEAD.bits(),
                "a retained property value was swept"
            );
            if name.bits() == property.bits() {
                assert_eq!(value.bits(), expected.bits());
                found = true;
            }
            plist = rest.cons_cdr();
        }
        assert!(plist.is_nil(), "proper retained property list");
        assert!(
            found,
            "fixture property remains in the complete retained plist"
        );
    }

    #[derive(Clone, Copy)]
    pub enum PropertySource {
        PropertizeElement,
        CopiedString,
        NestedEval,
        FreshEvalString,
        DetachedFormatElements,
    }

    #[derive(Clone, Copy)]
    pub enum FormatEntry {
        LispString,
        Display,
    }
    use neovm_eval::emacs_core::eval::Context;
    pub(crate) fn assert_accumulated_property_survives(
        make_context: impl FnOnce() -> neovm_eval::emacs_core::eval::Context,
        generational: bool,
        source: PropertySource,
        entry: FormatEntry,
    ) {
        // Nextest runs each case in a fresh process. The constructor reads this
        // knob once; the explicit collection inside :eval is the trigger here.
        unsafe {
            std::env::set_var(
                "NEOVM_GC_GENERATIONAL",
                if generational { "1" } else { "0" },
            );
        }
        crate::test_utils::init_test_tracing();
        let mut eval = make_context();
        neovm_eval::test_support::authority::set_gc_stress_for_test(&mut eval, false);
        eval.set_gc_threshold(usize::MAX);
        eval.set_variable("noninteractive", Value::NIL);
        eval.gc_collect_exact();

        // Keep the input in the actual obarray, independently of the formatter's
        // Rust locals. The copied-string case also keeps the ORIGINAL plist alive;
        // only the freshly copied destination spine needs an additional root.
        let setup = match source {
            PropertySource::PropertizeElement => {
                "(setq u34-mode-line-gc-format
                        '((:propertize \"A\" u34-mode-line-gc-property 73)
                          (:eval (progn (garbage-collect) \"B\"))))"
            }
            PropertySource::CopiedString => {
                "(setq u34-mode-line-gc-format
                        (list (propertize \"A\" 'u34-mode-line-gc-property 73)
                              '(:eval (progn (garbage-collect) \"B\"))))"
            }
            PropertySource::NestedEval => {
                "(setq u34-mode-line-gc-format
                        '((:propertize \"A\" u34-mode-line-gc-property 73)
                          (1 (:propertize
                                ((:eval (progn (garbage-collect) \"B\")))
                                face bold))))"
            }
            PropertySource::FreshEvalString => {
                // Add no source span after collection: a missing source root must
                // reach the ownership probe before any source-value comparison.
                "(setq u34-mode-line-gc-format
                        '((:eval (propertize (concat \"A\")
                                             'u34-mode-line-gc-property 73))
                          (:eval (progn (garbage-collect) \"\"))))"
            }
            PropertySource::DetachedFormatElements => {
                "(setq u34-mode-line-gc-format
                        (list '(:propertize \"A\" u34-mode-line-gc-property 73)
                              '(:eval (progn
                                        (setcdr (cdr u34-mode-line-gc-format) nil)
                                        (garbage-collect)
                                        \"\"))
                              (concat \"B\")))"
            }
        };
        eval.eval_str(setup)
            .expect("install rooted mode-line format");
        let format = eval
            .obarray_view()
            .symbol_value_copied("u34-mode-line-gc-format")
            .expect("installed mode-line format");
        let before = eval.gc_count_for_test();
        let (rendered, sources) = match entry {
            FormatEntry::LispString => {
                // The Lisp entry runs the same evaluator-backed recursive walk.
                let value = eval
                    .eval_str("(format-mode-line u34-mode-line-gc-format)")
                    .expect("format-mode-line with a collecting :eval");
                (value, Vec::new())
            }
            FormatEntry::Display => {
                let output = format_mode_line_for_display_with_sources(
                    &mut eval,
                    format,
                    Value::NIL,
                    Value::NIL,
                    80,
                );
                let sources = output
                    .source_spans()
                    .iter()
                    .map(|span| span.source())
                    .collect::<Vec<_>>();
                (output.value(), sources)
            }
        };
        assert_eq!(
            eval.gc_count_for_test(),
            before + 1,
            "the later :eval must collect"
        );
        let expected = if matches!(
            source,
            PropertySource::DetachedFormatElements | PropertySource::FreshEvalString
        ) {
            "A"
        } else {
            "AB"
        };
        assert_eq!(rendered.as_utf8_str(), Some(expected));
        let string = rendered.as_lisp_string().expect("rendered string");
        let runs = string.intervals().object_interval_plist_runs_for_char_len(
            neovm_edit::buffer::CharLen::new(expected.len()),
        );
        let first = runs.first().expect("retained first interval");
        assert_eq!(first.start(), neovm_edit::buffer::CharPos0::ZERO);
        let plist = first.plist();
        assert!(plist.is_cons(), "the property spine must be retained");

        assert_owned_property(
            &eval,
            plist,
            Value::symbol("u34-mode-line-gc-property"),
            Value::fixnum(73),
        );
        // A previous :eval can contribute a fresh source string that is retained
        // only in the display sidecar after its temporary evaluator root ends.
        // Check the arena's allocation bitmap without reading the string payload.
        // The final rendered string is the only string allocation after :eval's
        // collection; reject its address too, in case it reused a swept source.
        for source_value in sources {
            assert_ne!(
                source_value.bits(),
                rendered.bits(),
                "a swept mode-line source was reused for the rendered string"
            );
            assert!(
                eval.tagged_heap().owns_heap_value_for_test(source_value),
                "the accumulated source string was swept during the later :eval"
            );
        }
    }
}

/// Shared upper-layer helpers for mode_line_flow_test (peer P1 precision):
/// real bodies live here once; callers supply the harness Context where needed.
pub mod mode_line_flow_test_upper_layer_helpers {
    use neovm_eval::emacs_core::eval::Context;
    use neovm_eval::emacs_core::eval::save_scratch_gc_roots;
    use neovm_value::value::Value;
    pub(crate) fn context(
        make_context: impl FnOnce() -> neovm_eval::emacs_core::eval::Context,
    ) -> Context {
        // nextest runs each test in its own process. This process knob contains
        // no Lisp state and is shared read-only by every mutator after first use.
        if std::env::var_os("NEOVM_MODE_LINE_FLOW").is_none() {
            unsafe { std::env::set_var("NEOVM_MODE_LINE_FLOW", "1") };
        }
        crate::test_utils::init_test_tracing();
        let mut eval = make_context();
        eval.set_variable("noninteractive", Value::NIL);
        eval
    }
}

/// Shared upper-layer helpers for mode_line_incremental_lifetimes_test (peer P1 precision):
/// real bodies live here once; callers supply the harness Context where needed.
pub mod mode_line_incremental_lifetimes_test_upper_layer_helpers {
    use crate::test_utils::xdisp_test_upper_layer_helpers::interactive_context;
    use neovm_edit::buffer::CharLen;
    use neovm_eval::emacs_core::eval::Context;
    use neovm_eval::emacs_core::eval::save_scratch_gc_roots;
    use neovm_value::value::Value;
    use neovm_value::value::get_string_text_properties_table_for_value;
    pub fn assert_live_interval_plists(eval: &Context, value: Value) {
        let string = value.as_lisp_string().expect("formatted string");
        let Some(table) = get_string_text_properties_table_for_value(value) else {
            return;
        };
        for run in table.object_interval_plist_runs_for_char_len(CharLen::new(string.schars())) {
            let mut plist = run.plist();
            while plist.is_cons() {
                assert!(
                    eval.tagged_heap().owns_heap_value_for_test(plist),
                    "GC reclaimed an accumulated plist cell"
                );
                assert!(
                    !plist.cons_car().is_dead(),
                    "accumulated property name was swept"
                );
                let tail = plist.cons_cdr();
                if !tail.is_cons() {
                    break;
                }
                assert!(
                    eval.tagged_heap().owns_heap_value_for_test(tail),
                    "GC reclaimed an accumulated property value cell"
                );
                assert!(
                    !tail.cons_car().is_dead(),
                    "accumulated property payload was swept"
                );
                plist = tail.cons_cdr();
            }
        }
    }
    pub(crate) fn context(
        make_context: impl FnOnce() -> neovm_eval::emacs_core::eval::Context,
    ) -> Context {
        let mut eval = interactive_context(make_context);
        neovm_eval::test_support::authority::set_gc_stress_for_test(&mut eval, false);
        eval.set_gc_threshold(usize::MAX);
        eval
    }
}

/// Shared upper-layer helpers for mode_line_element_attribution_test (peer P1 precision):
/// real bodies live here once; callers supply the harness Context where needed.
pub mod mode_line_element_attribution_test_upper_layer_helpers {
    use crate::window::DisplayFrameManagerExt;
    use neovm_eval::emacs_core::eval::Context;
    use neovm_eval::emacs_core::eval::save_scratch_gc_roots;
    use neovm_value::value::Value;
    pub(crate) fn context(
        make_context: impl FnOnce() -> neovm_eval::emacs_core::eval::Context,
    ) -> Context {
        crate::test_utils::init_test_tracing();
        let mut eval = make_context();
        neovm_eval::test_support::authority::set_gc_stress_for_test(&mut eval, false);
        eval.set_gc_threshold(usize::MAX);
        eval.set_variable("noninteractive", Value::NIL);
        let buffer = eval
            .edit
            .buffers
            .current_buffer_id()
            .expect("current buffer");
        let frame = eval
            .frame_manager_mut()
            .create_frame("element-attribution", 800, 600, buffer);
        assert!(eval.frame_manager_mut().select_frame(frame));
        eval.eval_str(
            r#"(defvar ep30-events nil)
               (defvar ep30-tail "OLD")
               (defvar ep30-live-list nil)
               (defvar ep30-effects 0)
               (defvar ep30-inner nil)
               (defvar ep30-inner-format '("i" "j"))
               (defvar ep30-format-string nil)
               (defvar ep30-collect nil)
               (fset 'ep30-callback '(lambda (kind)
                 (setq ep30-events (cons (list 'enter kind) ep30-events))
                 (prog1
                     (cond
                      ((eq kind 'nested) "EV")
                      ((eq kind 'mutate-value) (setq ep30-tail "NEW")
                       (if ep30-collect (garbage-collect)) "M")
                      ((eq kind 'mutate-spine)
                       (setcar (cdr (cdr ep30-live-list)) "NEW")
                       (if ep30-collect (garbage-collect)) "M")
                      ((eq kind 'reenter)
                       (setq ep30-inner (format-mode-line ep30-inner-format)) "E")
                      ((eq kind 'reenter-noprop)
                       (setq ep30-inner (format-mode-line '("i" "j") 0)) "E")
                      ((eq kind 'signal) (signal 'error '("EP30 expected signal")))
                      ((eq kind 'throw) (throw 'ep30-escape 'ep30-thrown))
                      ((eq kind 'reenter-throw)
                       (format-mode-line '("i" (:eval (ep30-callback 'throw)) "j")))
                      ((eq kind 'reenter-caught-throw)
                       (setq ep30-events
                             (cons
                              (list 'inner-caught
                                    (catch 'ep30-inner
                                      (format-mode-line
                                       '("i" "j" "k" (:eval (throw 'ep30-inner 'escaped)) "l"))))
                              ep30-events))
                       "E")
                      ((eq kind 'count) (setq ep30-effects (1+ ep30-effects)) "Q")
                      (t (error "Unknown attribution callback: %S" kind)))
                   (setq ep30-events (cons (list 'return kind) ep30-events)))))"#,
        )
        .expect("GNU callback fixture");
        eval
    }
    pub(crate) fn strict(eval: &mut Context, expression: &str, gnu: &str) {
        let roots = save_scratch_gc_roots();
        eval.eval_str(&format!("(setq ep30-actual {expression})"))
            .expect("format-mode-line returns");
        assert_eq!(
            save_scratch_gc_roots(),
            roots,
            "temporary roots: {expression}"
        );
        let equal = eval
            .eval_str(&format!("(equal-including-properties ep30-actual '{gnu})"))
            .expect("strict property comparison");
        assert!(
            equal.is_t(),
            "GNU properties differ for {expression}: {:?}",
            eval.eval_str("(prin1-to-string ep30-actual)")
                .map(|value| value.as_utf8_str().map(str::to_owned))
        );
    }
}

/// Shared upper-layer helpers for mode_line_plain_field_test (peer P1 precision):
/// real bodies live here once; callers supply the harness Context where needed.
pub mod mode_line_plain_field_test_upper_layer_helpers {
    pub(crate) struct PlainFieldGuard {
        previous: ModeLinePlainFieldPath,
    }

    impl PlainFieldGuard {
        pub(crate) fn set(path: ModeLinePlainFieldPath, properties: PropertyRead) -> Self {
            let previous = crate::emacs_core::xdisp::set_mode_line_plain_field_for_test(path);
            match properties {
                PropertyRead::Copied => {
                    crate::emacs_core::xdisp::set_mode_line_prop_borrow_for_test(Some(false))
                }
                PropertyRead::Borrowed => {
                    crate::emacs_core::xdisp::set_mode_line_prop_borrow_for_test(Some(true))
                }
            }
            Self { previous }
        }
    }

    impl Drop for PlainFieldGuard {
        fn drop(&mut self) {
            crate::emacs_core::xdisp::set_mode_line_prop_borrow_for_test(None);
            crate::emacs_core::xdisp::set_mode_line_plain_field_for_test(self.previous);
        }
    }

    pub(crate) fn plain_field_guard_for_test(
        path: ModeLinePlainFieldPath,
        properties: PropertyRead,
    ) -> impl Drop {
        PlainFieldGuard::set(path, properties)
    }

    use crate::emacs_core::xdisp::ModeLinePlainFieldPath;
    use crate::emacs_core::xdisp::format_mode_line_for_display_with_sources;
    use crate::test_utils::xdisp_test_upper_layer_helpers::interactive_context;
    use crate::window::DisplayFrameManagerExt;
    use neovm_eval::emacs_core::eval::Context;
    use neovm_value::value::Value;

    #[derive(Clone, Copy, Debug)]
    pub enum PropertyRead {
        Copied,
        Borrowed,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub(crate) struct StringTrace {
        bytes: Vec<u8>,
        multibyte: bool,
        char_len: usize,
        interval_plists: String,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub(crate) struct SourceTrace {
        output_start: usize,
        output_end: usize,
        source_start: usize,
        source_ordinal: usize,
        is_root: bool,
        is_input_source: bool,
        text: StringTrace,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub(crate) struct OutputTrace {
        text: StringTrace,
        sources: Vec<SourceTrace>,
        eval_count: i64,
        point: usize,
    }

    #[derive(Clone, Copy)]
    pub(crate) enum PropertyOrder {
        Exact,
        Canonical,
    }
    pub(crate) fn compare(
        mut make_context: impl FnMut() -> neovm_eval::emacs_core::eval::Context,
        setup: &str,
    ) {
        for properties in [PropertyRead::Copied, PropertyRead::Borrowed] {
            let temporary = output_trace(
                || make_context(),
                setup,
                ModeLinePlainFieldPath::Temporary,
                properties,
                PropertyOrder::Canonical,
            );
            let direct = output_trace(
                || make_context(),
                setup,
                ModeLinePlainFieldPath::Direct,
                properties,
                PropertyOrder::Canonical,
            );
            assert_eq!(
                direct, temporary,
                "properties={properties:?}; format setup: {setup}"
            );
        }
    }
    pub(crate) fn output_trace(
        make_context: impl FnOnce() -> neovm_eval::emacs_core::eval::Context,
        setup: &str,
        path: ModeLinePlainFieldPath,
        properties: PropertyRead,
        order: PropertyOrder,
    ) -> OutputTrace {
        crate::test_utils::init_test_tracing();
        let _guard = plain_field_guard_for_test(path, properties);
        let mut eval = interactive_context(make_context);
        let buffer_id = eval
            .edit
            .buffers
            .current_buffer()
            .expect("current buffer")
            .id();
        let frame_id = eval
            .frame_manager_mut()
            .create_frame("plain-field", 800, 600, buffer_id);
        let window_id = eval
            .frame_manager()
            .get(frame_id)
            .expect("frame")
            .selected_window;
        eval.edit
            .buffers
            .get_mut(buffer_id)
            .expect("buffer")
            .insert("first\nsecond\n");
        eval.eval_str("(setq d5-plain-field-evals 0)")
            .expect("eval counter");
        let raw = Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
            b'A', 0xff, b'B', b'%', b'b', b'Z', 0x80,
        ]));
        eval.set_variable("d5-plain-field-raw", raw);
        let format = eval.eval_str(setup).expect("format setup");
        eval.set_variable("d5-plain-field-format", format);
        let output = format_mode_line_for_display_with_sources(
            &mut eval,
            format,
            Value::make_window(window_id.0),
            Value::make_buffer(buffer_id),
            24,
        );
        // Fresh :eval strings are owned only by the output sidecar. Root the
        // complete sidecar before introspection calls that could collect.
        let root_scope = eval.save_specpdl_roots();
        eval.push_specpdl_root(output.value());
        for span in output.source_spans() {
            eval.push_specpdl_root(span.source());
        }
        let input_source = eval
            .obarray_view()
            .symbol_value_copied("d5-plain-field-source");
        let mut identities = Vec::new();
        let text = string_trace(&mut eval, output.value(), order);
        let sources = output
            .source_spans()
            .iter()
            .map(|span| {
                let source = span.source();
                let source_ordinal =
                    if let Some(index) = identities.iter().position(|seen| *seen == source) {
                        index
                    } else {
                        identities.push(source);
                        identities.len() - 1
                    };
                SourceTrace {
                    output_start: span.output_start(),
                    output_end: span.output_end(),
                    source_start: span.source_start(),
                    source_ordinal,
                    is_root: source == output.value(),
                    is_input_source: input_source == Some(source),
                    text: string_trace(&mut eval, source, PropertyOrder::Exact),
                }
            })
            .collect();
        let eval_count = eval
            .obarray_view()
            .symbol_value_copied("d5-plain-field-evals")
            .and_then(Value::as_fixnum)
            .expect("eval counter");
        let point = eval
            .edit
            .buffers
            .get(buffer_id)
            .expect("buffer")
            .point_char_pos()
            .get();
        eval.restore_specpdl_roots(root_scope);
        OutputTrace {
            text,
            sources,
            eval_count,
            point,
        }
    }
    pub(crate) fn string_trace(
        eval: &mut Context,
        value: Value,
        order: PropertyOrder,
    ) -> StringTrace {
        let string = value.as_lisp_string().expect("mode-line string");
        let bytes = string.as_bytes().to_vec();
        let multibyte = string.is_multibyte();
        let char_len = string.schars();
        // No interval borrow crosses these Lisp calls. Literal-only cases keep
        // exact plist order. Percent expansion's existing HashMap overlay order
        // is random even for two copied-baseline walks, so those comparisons sort
        // property pairs within each output run, retaining boundaries and values.
        // Original source strings always use Exact below.
        eval.set_variable("d5-plain-field-result", value);
        let inspect = match order {
            PropertyOrder::Exact => "(prin1-to-string (object-intervals d5-plain-field-result))",
            PropertyOrder::Canonical => {
                r#"(prin1-to-string
              (mapcar
               (lambda (run)
                 (let ((plist (nth 2 run)) (pairs nil))
                   (while plist
                     (setq pairs (cons (cons (car plist) (car (cdr plist))) pairs))
                     (setq plist (cdr (cdr plist))))
                   (list (car run) (car (cdr run))
                         (apply #'append
                                (mapcar (lambda (pair) (list (car pair) (cdr pair)))
                                        (sort pairs
                                              (lambda (a b)
                                                (string-lessp (prin1-to-string (car a))
                                                              (prin1-to-string (car b))))))))))
               (object-intervals d5-plain-field-result)))"#
            }
        };
        let interval_plists = eval
            .eval_str(inspect)
            .expect("object intervals")
            .as_utf8_str()
            .expect("printed intervals")
            .to_owned();
        StringTrace {
            bytes,
            multibyte,
            char_len,
            interval_plists,
        }
    }
}

/// Shared upper-layer helpers for mode_line_min_width_boundary_test (peer P1 precision):
/// real bodies live here once; callers supply the harness Context where needed.
pub mod mode_line_min_width_boundary_test_upper_layer_helpers {
    use crate::test_utils::xdisp_test_upper_layer_helpers::utf8_locale_terminal;
    use crate::window::DisplayFrameManagerExt;
    use neovm_eval::emacs_core::eval::Context;
    pub(crate) const GNU_ROWS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/emacs_core/display/xdisp/tests/mode_line_min_width_boundary_gnu_rows.tsv"
    ));
    /// Give the terminal UTF-8 keyboard and terminal codings, as mule.el's
    /// `set-locale-environment` does under a UTF-8 locale.  A bare context has
    /// GNU `create_terminal`'s defaults (`no-conversion` and `undecided`), since
    /// no startup Lisp has run.
    use crate::emacs_core::xdisp::format_mode_line_for_display_with_sources;
    use crate::test_utils::mode_line_numeric_padding_shared_helpers::PaddingPolicyGuard;
    use crate::test_utils::xdisp_test_upper_layer_helpers::interactive_context;
    use neovm_value::value::Value;
    pub(crate) fn compare_fresh_gnu_row(
        make_context: impl FnOnce() -> neovm_eval::emacs_core::eval::Context,
        kind: &str,
    ) {
        crate::test_utils::init_test_tracing();
        let _policy = PaddingPolicyGuard::set(true);
        let mut eval = interactive_context(make_context);
        utf8_locale_terminal(&mut eval);
        let buffer_id = eval.edit.buffers.current_buffer_id().expect("buffer");
        let frame_id = eval
            .frame_manager_mut()
            .create_frame("D5-F", 800, 600, buffer_id);
        let window_id = eval
            .frame_manager()
            .get(frame_id)
            .expect("frame")
            .selected_window;
        eval.edit
            .buffers
            .set_buffer_local_property(buffer_id, "mode-name", Value::string("MODE"))
            .expect("set mode name");
        eval.edit
            .buffers
            .set_buffer_local_property(
                buffer_id,
                "buffer-file-coding-system",
                Value::symbol("utf-8-unix"),
            )
            .expect("set buffer coding");
        eval.obarray_mut()
            .set_symbol_value("eol-mnemonic-unix", Value::string("EOL"));
        let format = eval.eval_str(format_for(kind)).expect("GNU format setup");
        let output = format_mode_line_for_display_with_sources(
            &mut eval,
            format,
            Value::make_window(window_id.0),
            Value::make_buffer(buffer_id),
            120,
        );
        assert_eq!(output.value().as_utf8_str(), Some(expected(kind)), "{kind}");
        let closing = output.source_spans().last().expect("closing source");
        assert_eq!(closing.source().as_utf8_str(), Some("|"));
        assert_eq!(closing.output_start(), expected(kind).chars().count() - 1);
    }
    pub(crate) fn expected(kind: &str) -> &str {
        GNU_ROWS
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .find_map(|(name, row)| (name == kind).then_some(row))
            .expect("fresh GNU case exists")
    }
    pub(crate) fn format_for(kind: &str) -> &'static str {
        match kind {
            "mode-name" => r#"'((:propertize "AB" display (min-width (10.0))) "%m" "|")"#,
            "frame-name" => r#"'((:propertize "AB" display (min-width (10.0))) "%F" "|")"#,
            "eol-indicator" => r#"'((:propertize "AB" display (min-width (10.0))) "%Z" "|")"#,
            "other-source-literal" => {
                r#"'((:propertize "AB" display (min-width (10.0))) "%l rest" "|")"#
            }
            "same-source-predecessor" => {
                r#"(let* ((width (list 10.0))
                           (display (list 'min-width width))
                           (source (copy-sequence "%m rest")))
                      (put-text-property 0 2 'display display source)
                      (list (list :propertize "AB" 'display display) source "|"))"#
            }
            "other-identity-predecessor" => {
                r#"(let* ((width (list 10.0))
                           (display (list 'min-width width))
                           (source (copy-sequence "%m rest")))
                      (put-text-property 0 2 'display (list 'min-width (list 10.0)) source)
                      (list (list :propertize "AB" 'display display) source "|"))"#
            }
            _ => panic!("unknown GNU boundary case"),
        }
    }
}

/// Shared upper-layer helpers for mode_line_dynamic_symbols_test (peer P1 precision):
/// real bodies live here once; callers supply the harness Context where needed.
pub mod mode_line_dynamic_symbols_test_upper_layer_helpers {
    use crate::emacs_core::xdisp::ModeLinePercentContext;
    use crate::emacs_core::xdisp::ModeLineRendered;
    use crate::emacs_core::xdisp::format_mode_line_for_target;
    use crate::emacs_core::xdisp::format_mode_line_recursive_in_state;
    use crate::emacs_core::xdisp::format_mode_line_recursive_in_state_with_eval_rooted;
    use crate::emacs_core::xdisp::mode_line_gc;
    use crate::emacs_core::xdisp::mode_line_string::Risk;
    use neovm_value::value::Value;
    #[derive(Clone, Copy)]
    pub enum Walker {
        Context,
        State,
        StateWithEval,
    }
    use neovm_eval::emacs_core::eval::Context;
    use neovm_value::heap_types::{GnuDeclaredCharacterCount, LispString};
    use neovm_value::intern::{
        intern_call_names, intern_uninterned, reset_intern_calls, restore_runtime_interner,
        unintern_canonical_id,
    };
    pub(crate) fn context(
        make_context: impl FnOnce() -> neovm_eval::emacs_core::eval::Context,
    ) -> Context {
        let mut ctx = make_context();
        neovm_eval::test_support::authority::set_gc_stress_for_test(&mut ctx, false);
        ctx.set_gc_threshold(usize::MAX);
        let buffer = ctx
            .edit
            .buffers
            .create_buffer("*ttykey-k5-dynamic-symbols*");
        ctx.edit.buffers.set_current(buffer);
        ctx
    }
    pub(crate) fn render(
        ctx: &mut Context,
        pctx: &ModeLinePercentContext,
        format: Value,
        walker: Walker,
    ) -> String {
        let mut rendered = ModeLineRendered::default();
        render_into(ctx, pctx, format, walker, &mut rendered);
        // Literal strings in these cases are property-free and contain no padding.
        rendered
            .text()
            .iter()
            .map(|code| char::from_u32(*code).expect("Unicode test text"))
            .collect()
    }
    pub(crate) fn render_into(
        ctx: &mut Context,
        pctx: &ModeLinePercentContext,
        format: Value,
        walker: Walker,
        rendered: &mut ModeLineRendered,
    ) {
        match walker {
            Walker::Context => {
                format_mode_line_for_target(
                    ctx,
                    pctx,
                    &format,
                    rendered,
                    0.into(),
                    Risk::Trusted,
                    None,
                )
                .expect("context walk");
            }
            Walker::State => {
                assert!(!format_mode_line_recursive_in_state(
                    ctx.variables(),
                    &[],
                    &ctx.edit.buffers,
                    &ctx.processes(),
                    pctx,
                    &format,
                    rendered,
                    0,
                    false,
                ));
            }
            Walker::StateWithEval => {
                let roots = mode_line_gc::ScratchRoots::new();
                format_mode_line_recursive_in_state_with_eval_rooted(
                    ctx.variables(),
                    &[],
                    &ctx.edit.buffers,
                    &ctx.processes(),
                    pctx,
                    &format,
                    rendered,
                    0,
                    false,
                    &mut |_, _| Ok(Value::NIL),
                    &roots,
                )
                .expect("split callback walk");
            }
        }
    }
}

/// Shared upper-layer helpers for xdisp_test (peer P1 precision):
/// real bodies live here once; callers supply the harness Context where needed.
pub mod xdisp_test_upper_layer_helpers {
    pub(crate) fn utf8_locale_terminal(eval: &mut Context) {
        eval.eval_str(
            "(progn (set-keyboard-coding-system-internal 'utf-8-unix)
                (set-terminal-coding-system-internal 'utf-8-unix))",
        )
        .expect("setting the terminal codings should evaluate");
    }
    use crate::emacs_core::image_catalog::{
        ImageCatalog, ImageId, ImageLayoutExtent, ImageLoadAttempt, ImageLoadToken, ImageLookup,
        ImageMaskKind, ImageResolveRequest, ImageSizeLimit, ReadyImage, ResolvedImageMetadata,
    };
    use crate::window::DisplayFrameManagerExt;
    use neovm_edit::buffer::buffer::BUFFER_SLOT_BUFFER_FILE_CODING_SYSTEM;
    use neovm_edit::buffer::{BufferTextBackendKind, CharPos0, LispCharPos1};
    use neovm_eval::emacs_core::eval::Context;
    use neovm_eval::emacs_core::eval::DisplayHost;
    use neovm_eval::emacs_core::eval::VariableView;
    use neovm_eval::emacs_core::eval::data::display_spec::display_prop_replacing_p;
    use neovm_value::error::FlowKind;
    use neovm_value::intern::intern;
    use neovm_value::value::Value;
    use neovm_value::value::{
        StringTextPropertyRun, ValueKind, get_string_text_properties_table_for_value,
        set_string_text_properties_for_value,
    };
    pub(crate) fn interactive_context(
        make_context: impl FnOnce() -> neovm_eval::emacs_core::eval::Context,
    ) -> Context {
        let mut eval = make_context();
        eval.set_variable("noninteractive", Value::NIL);
        eval
    }
    /// Build a context with a single TTY frame (char cell 1x1) whose selected
    /// window shows the current buffer, used by the `display`-spec measurement
    /// tests below.  Returns the selected-window id as a fixnum-able integer.
    pub(crate) fn pixel_size_tty_context(
        make_context: impl FnOnce() -> neovm_eval::emacs_core::eval::Context,
    ) -> (Context, i64) {
        let mut eval = interactive_context(make_context);
        let buf_id = eval
            .edit
            .buffers
            .current_buffer()
            .expect("current buffer")
            .id();
        let frame_id = eval
            .frame_manager_mut()
            .create_frame("xdisp-display-spec", 200, 24, buf_id);
        {
            let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
            frame.char_width = 1.0;
            frame.char_height = 1.0;
            frame.font_pixel_size = 1.0;
            frame.set_window_system(None);
        }
        let selected_window = eval
            .frame_manager()
            .get(frame_id)
            .expect("frame")
            .selected_window
            .0 as i64;
        (eval, selected_window)
    }
}

/// Shared upper-layer helpers for motion_test (peer P1 precision):
/// real bodies live here once; callers supply the harness Context where needed.
pub mod motion_test_upper_layer_helpers {
    use crate::window::{DisplayRowSnapshot, WindowDisplaySnapshot};
    use neovm_edit::buffer::LispCharPos1;
    pub fn eob_overlay_snapshot(two_lines: bool, extra_rows: usize) -> WindowDisplaySnapshot {
        use crate::window::{
            CompletedEobOverlayMotionExtent, DisplayRowEndSource, EobOverlayMotionCoverage,
        };

        let end = if two_lines { 8 } else { 4 };
        let mut rows = Vec::new();
        if two_lines {
            rows.push(row(0, 1, 4));
        }
        let mut source = row(rows.len() as i64, if two_lines { 5 } else { 1 }, end);
        source.end_source = DisplayRowEndSource::OverlayAfterString;
        rows.push(source);
        for _ in 0..extra_rows {
            let mut inserted = row(rows.len() as i64, end, end);
            inserted.end_source = DisplayRowEndSource::OverlayAfterString;
            rows.push(inserted);
        }
        rows.push(DisplayRowSnapshot {
            row: rows.len() as i64,
            ..Default::default()
        });
        WindowDisplaySnapshot {
            rows,
            eob_overlay_motion: EobOverlayMotionCoverage::Complete(
                CompletedEobOverlayMotionExtent::from_physical_rows(
                    LispCharPos1::new(end),
                    usize::from(two_lines),
                    usize::from(two_lines) + extra_rows + 1,
                )
                .expect("overlay traversal advances forward"),
            ),
            ..Default::default()
        }
    }
    pub fn row(index: i64, start: i64, end: i64) -> DisplayRowSnapshot {
        DisplayRowSnapshot {
            row: index,
            start_buffer_pos: Some(LispCharPos1::new(start)),
            end_buffer_pos: Some(LispCharPos1::new(end)),
            ..Default::default()
        }
    }
}

/// Shared upper-layer helpers for system_font_test (peer P1 precision):
/// real bodies live here once; callers supply the harness Context where needed.
pub mod system_font_test_upper_layer_helpers {
    use crate::emacs_core::display_host::FrameFontSize;
    use crate::emacs_core::display_host::{
        FrameFontRequest, SystemFontName, SystemFontRole, SystemFonts,
    };
    use crate::face::{FontSlant, FontWeight, FontWidth};
    use crate::test_utils::xfaces_upper_layer_helpers::ensure_selected_gui_frame;
    use crate::window::FrameId;
    use neovm_eval::emacs_core::eval::ResolvedFrameFont;
    use neovm_eval::emacs_core::eval::{
        Context, DisplayHost, FontPxProbeResult, GuiFrameHostRequest,
    };
    pub(crate) fn desktop_context(
        make_context: impl FnOnce() -> neovm_eval::emacs_core::eval::Context,
    ) -> Context {
        let mut eval = make_context();
        ensure_selected_gui_frame(&mut eval);
        eval.set_display_host(Box::new(DesktopFontHost(SystemFonts::new(
            SystemFontName::new("Ubuntu Mono 13".into()),
            SystemFontName::new("Ubuntu 10".into()),
        ))));
        eval
    }
    /// Substitute only the native desktop/font system. Lisp face updates, name
    /// parsing and default-face queries go through the real public interface.
    struct DesktopFontHost(SystemFonts);

    impl DisplayHost for DesktopFontHost {
        fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }
        fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }

        fn system_font(&self, role: SystemFontRole) -> Option<&SystemFontName> {
            self.0.get(role)
        }

        fn resolve_frame_font(
            &mut self,
            _: FrameId,
            request: FrameFontRequest,
        ) -> Result<Option<ResolvedFrameFont>, String> {
            let family = request
                .face()
                .family_runtime_string_owned()
                .unwrap_or("Fallback Mono".into());
            let pixels = match request.size() {
                FrameFontSize::Points(points) => (points.get() * 96.0 / 72.27).round() as u32,
                FrameFontSize::Pixels(pixels) => pixels.get(),
                FrameFontSize::Default => 13,
                FrameFontSize::Relative(scale) => (13.0 * scale.get()).round() as u32,
            };
            Ok(Some(ResolvedFrameFont {
                height_tenths: (f64::from(pixels) * 720.0 / 96.0).round() as i32,
                font: neovm_eval::emacs_core::eval::test_resolved_opened_font(
                    &family,
                    None,
                    None,
                    request.face().weight.unwrap_or(FontWeight::NORMAL),
                    request.face().slant.unwrap_or(FontSlant::Normal),
                    request.face().width.unwrap_or(FontWidth::Normal),
                    None,
                    FontPxProbeResult {
                        pixel_size: pixels,
                        height: pixels as i32 + 1,
                        ascent: pixels as i32,
                        descent: 1,
                        max_width: pixels as i32 / 2,
                        space_width: pixels as i32 / 2,
                        average_width: pixels as i32 / 2,
                    },
                    None,
                ),
            }))
        }
    }
}

/// Shared numeric-padding policy guard for the re-homed mode-line tests.
/// One home; the display unit tests and the core destinations share it.
pub mod mode_line_numeric_padding_shared_helpers {
    pub(crate) struct PaddingPolicyGuard(Option<bool>);
    impl PaddingPolicyGuard {
        pub(crate) fn set(value: bool) -> Self {
            Self(crate::emacs_core::xdisp::force_for_test(Some(value)))
        }
    }
    impl Drop for PaddingPolicyGuard {
        fn drop(&mut self) {
            crate::emacs_core::xdisp::force_for_test(self.0);
        }
    }
}

/// Shared terminal recording fixture for the moved idle-redisplay test.
pub mod terminal_test_upper_layer_helpers {
    use crate::emacs_core::terminal::pure::TerminalHost;
    use std::cell::RefCell;
    use std::rc::Rc;

    pub struct RecordingTerminalHost {
        pub(crate) log: Rc<RefCell<Vec<&'static str>>>,
    }
    impl TerminalHost for RecordingTerminalHost {
        fn suspend_tty(&mut self) -> Result<(), String> {
            self.log.borrow_mut().push("suspend");
            Ok(())
        }

        fn resume_tty(&mut self) -> Result<(), String> {
            self.log.borrow_mut().push("resume");
            Ok(())
        }

        fn delete_terminal(&mut self) -> Result<(), String> {
            self.log.borrow_mut().push("delete");
            Ok(())
        }
    }
}
