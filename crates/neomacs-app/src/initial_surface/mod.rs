//! GNU-shaped initial buffer and frame materialization shared by frontends.
//!
//! Runtime images intentionally do not own the live host surface. After an
//! image is restored, the application adapter supplies measured geometry,
//! font identity, and display facts here. This module establishes the single
//! initial `*scratch*`/minibuffer/frame graph used by desktop and mobile
//! frontends; host terminal libraries and command-line startup remain adapter
//! responsibilities.

use std::fmt::{Display, Formatter};

use neovm_core::buffer::{BufferId, EmacsBytePos, EmacsByteRange, LispCharPos1};
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::display::gui_window_system_symbol;
use neovm_core::emacs_core::eval::Context;
use neovm_core::face::{FaceAttrValue, LFaceAttr};
use neovm_core::window::{
    FrameDisplayIdentity, FrameId, FrameParam, FrameVisibility, Rect, Window,
};

use crate::frontend_event::FrontendScaleFactor;

/// Invalid geometry at the evaluator/frontend startup boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidInitialFrameMetrics {
    /// A live surface cannot have an empty extent.
    EmptyExtent,
    /// Character width must be finite and positive.
    CharacterWidth,
    /// Character height must be finite and positive.
    CharacterHeight,
    /// Font pixel size must be finite and positive.
    FontPixelSize,
}

impl Display for InvalidInitialFrameMetrics {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::EmptyExtent => "initial frame extent must be nonzero",
            Self::CharacterWidth => "initial character width must be finite and positive",
            Self::CharacterHeight => "initial character height must be finite and positive",
            Self::FontPixelSize => "initial font size must be finite and positive",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for InvalidInitialFrameMetrics {}

/// Host-measured geometry for the initial evaluator frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InitialFrameMetrics {
    width: u32,
    height: u32,
    character_width: f32,
    character_height: f32,
    font_pixel_size: f32,
}

impl InitialFrameMetrics {
    /// Validate one complete initial-frame measurement.
    pub fn new(
        width: u32,
        height: u32,
        character_width: f32,
        character_height: f32,
        font_pixel_size: f32,
    ) -> Result<Self, InvalidInitialFrameMetrics> {
        if width == 0 || height == 0 {
            return Err(InvalidInitialFrameMetrics::EmptyExtent);
        }
        if !is_positive_finite(character_width) {
            return Err(InvalidInitialFrameMetrics::CharacterWidth);
        }
        if !is_positive_finite(character_height) {
            return Err(InvalidInitialFrameMetrics::CharacterHeight);
        }
        if !is_positive_finite(font_pixel_size) {
            return Err(InvalidInitialFrameMetrics::FontPixelSize);
        }
        Ok(Self {
            width,
            height,
            character_width,
            character_height,
            font_pixel_size,
        })
    }
}

const fn is_positive_finite(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

/// Lisp-visible identity of the host-selected opening font.
#[derive(Clone, Copy, Debug)]
pub struct InitialFrameFont {
    parameter: Value,
    name: Value,
}

impl InitialFrameFont {
    /// Pair the exact `font-parameter` value with its public frame name.
    #[must_use]
    pub const fn new(parameter: Value, name: Value) -> Self {
        Self { parameter, name }
    }

    /// Use one host-selected family/name for both Lisp-visible font slots.
    ///
    /// Portable adapters use this when they have a stable public font name
    /// but no native opened-font object to expose as `font-parameter` yet.
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        let name = Value::string(name.into());
        Self {
            parameter: name,
            name,
        }
    }
}

/// GNU display class advertised by the opening frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitialDisplayType {
    Color,
    Monochrome,
}

impl InitialDisplayType {
    const fn symbol(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Monochrome => "mono",
        }
    }
}

/// Initial background classification before user frame parameters run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitialBackgroundMode {
    Light,
    Dark,
}

impl InitialBackgroundMode {
    const fn symbol(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

#[derive(Clone, Debug)]
enum InitialEditorSurfaceKind {
    Gui {
        device_scale: FrontendScaleFactor,
        display_identity: FrameDisplayIdentity,
        display_type: InitialDisplayType,
        background_mode: InitialBackgroundMode,
        font: InitialFrameFont,
    },
    Tty {
        initial_frame: bool,
    },
}

/// Complete host facts required to materialize the initial editor surface.
#[derive(Clone, Debug)]
pub struct InitialEditorSurfaceSpec {
    metrics: InitialFrameMetrics,
    kind: InitialEditorSurfaceKind,
}

impl InitialEditorSurfaceSpec {
    /// Describe a native-window opening frame.
    #[must_use]
    pub fn gui(
        metrics: InitialFrameMetrics,
        device_scale: FrontendScaleFactor,
        display_identity: FrameDisplayIdentity,
        display_type: InitialDisplayType,
        background_mode: InitialBackgroundMode,
        font: InitialFrameFont,
    ) -> Self {
        Self {
            metrics,
            kind: InitialEditorSurfaceKind::Gui {
                device_scale,
                display_identity,
                display_type,
                background_mode,
                font,
            },
        }
    }

    /// Describe a character-cell terminal opening frame.
    #[must_use]
    pub const fn tty(metrics: InitialFrameMetrics, initial_frame: bool) -> Self {
        Self {
            metrics,
            kind: InitialEditorSurfaceKind::Tty { initial_frame },
        }
    }
}

/// Stable identities established for one initial editor surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitialEditorSurface {
    scratch_buffer: BufferId,
    minibuffer: BufferId,
    frame: FrameId,
}

impl InitialEditorSurface {
    #[must_use]
    pub const fn scratch_buffer(self) -> BufferId {
        self.scratch_buffer
    }

    #[must_use]
    pub const fn minibuffer(self) -> BufferId {
        self.minibuffer
    }

    #[must_use]
    pub const fn frame(self) -> FrameId {
        self.frame
    }
}

/// Establish GNU's initial live buffer/frame graph after image restoration.
pub fn prepare_initial_editor_surface(
    evaluator: &mut Context,
    spec: InitialEditorSurfaceSpec,
) -> InitialEditorSurface {
    prepare_initial_editor_surface_with_gui_setup(evaluator, spec, |_, _| {})
}

/// Establish the initial surface and run adapter GUI setup at GNU's frame seam.
///
/// The callback runs after the root window and its markers exist, but before
/// the host-selected font is re-seeded and realized. Desktop uses this seam
/// for the Lisp side of its reused GUI frame; hosts with no additional setup
/// use [`prepare_initial_editor_surface`].
pub fn prepare_initial_editor_surface_with_gui_setup(
    evaluator: &mut Context,
    spec: InitialEditorSurfaceSpec,
    gui_setup: impl FnOnce(&mut Context, FrameId),
) -> InitialEditorSurface {
    let InitialFrameMetrics {
        width,
        height,
        character_width,
        character_height,
        font_pixel_size,
    } = spec.metrics;
    let gui = matches!(spec.kind, InitialEditorSurfaceKind::Gui { .. });

    let find_or_create_buffer = |evaluator: &mut Context, name: &str| {
        evaluator
            .buffer_manager()
            .find_buffer_by_name(name)
            .unwrap_or_else(|| evaluator.buffer_manager_mut().create_buffer(name))
    };

    // Reuse GNU startup buffers instead of creating duplicate names on top of
    // cached runtime-image state.
    let scratch_buffer = find_or_create_buffer(evaluator, "*scratch*");
    let _ = evaluator
        .buffer_manager_mut()
        .clear_buffer_labeled_restrictions(scratch_buffer);
    if let Some(buffer) = evaluator.buffer_manager_mut().get_mut(scratch_buffer) {
        buffer.widen();
        // GNU startup.el owns initial-scratch-message expansion and insertion.
        buffer.goto_emacs_byte_pos(buffer.point_max_emacs_byte_pos());
    }
    evaluator.buffer_manager_mut().set_current(scratch_buffer);

    let minibuffer = find_or_create_buffer(evaluator, " *Minibuf-0*");
    let _ = evaluator
        .buffer_manager_mut()
        .clear_buffer_labeled_restrictions(minibuffer);
    let _ = evaluator
        .buffer_manager_mut()
        .configure_buffer_undo_list(minibuffer, Value::NIL);
    if let Some(buffer) = evaluator.buffer_manager_mut().get_mut(minibuffer) {
        buffer.widen();
        buffer.goto_emacs_byte_pos(EmacsBytePos::ZERO);
    }

    let messages = find_or_create_buffer(evaluator, "*Messages*");
    let _ = evaluator
        .buffer_manager_mut()
        .clear_buffer_labeled_restrictions(messages);
    if let Some(buffer) = evaluator.buffer_manager_mut().get_mut(messages) {
        buffer.widen();
        let length = buffer.total_emacs_byte_len().get();
        if length > 0 {
            buffer.delete_emacs_byte_range(EmacsByteRange::new(
                EmacsBytePos::ZERO,
                EmacsBytePos::new(length),
            ));
        }
        buffer.goto_emacs_byte_pos(EmacsBytePos::ZERO);
    }
    let _ = evaluator
        .buffer_manager_mut()
        .note_buffer_order_tail(messages);

    let selected = evaluator
        .frame_manager()
        .selected_frame()
        .map(|frame| frame.id);
    let reuse_selected = selected.is_some() && evaluator.frame_manager().frame_list().len() == 1;
    let frame = if reuse_selected {
        selected.expect("selected startup frame")
    } else {
        evaluator
            .frame_manager_mut()
            .create_frame("F1", width, height, scratch_buffer)
    };
    let _ = evaluator.frame_manager_mut().select_frame(frame);

    if let Some(frame_state) = evaluator.frame_manager_mut().get_mut(frame) {
        if !frame_state.buffer_list.contains(&scratch_buffer) {
            frame_state
                .buffer_list
                .retain(|buffer| *buffer != scratch_buffer);
            frame_state.buffer_list.insert(0, scratch_buffer);
        }
        frame_state
            .buried_buffer_list
            .retain(|buffer| *buffer != scratch_buffer);
    }

    let (initial_tty_frame, device_scale, display_identity, display_type, background_mode, font) =
        match spec.kind {
            InitialEditorSurfaceKind::Gui {
                device_scale,
                display_identity,
                display_type,
                background_mode,
                font,
            } => (
                false,
                Some(device_scale),
                display_identity,
                Some(display_type),
                Some(background_mode),
                font,
            ),
            InitialEditorSurfaceKind::Tty { initial_frame } => (
                initial_frame,
                None,
                FrameDisplayIdentity::default(),
                None,
                None,
                InitialFrameFont::new(Value::NIL, Value::string("fixed")),
            ),
        };
    let font_snapshot = font
        .parameter
        .as_vector_data()
        .map(|items| Value::vector(items.to_vec()))
        .unwrap_or(font.parameter);
    let root_scope = neovm_core::emacs_core::eval::save_scratch_gc_roots();
    neovm_core::emacs_core::eval::push_scratch_gc_root(font_snapshot);
    neovm_core::emacs_core::eval::push_scratch_gc_root(font.name);

    if !gui {
        let assigned = evaluator
            .frame_manager_mut()
            .assign_initial_tty_frame_name(frame);
        debug_assert!(assigned, "selected startup frame must remain live");
    }

    if let Some(frame_state) = evaluator.frame_manager_mut().get_mut(frame) {
        if gui {
            frame_state.set_generated_name_value(Value::string("F1"));
        }
        frame_state.clear_title();
        frame_state.icon_name = Value::NIL;
        frame_state.initial = initial_tty_frame;
        frame_state.width = width;
        frame_state.height = height;
        frame_state.visibility = FrameVisibility::Visible;
        if gui {
            frame_state.device_scale_factor = device_scale.expect("GUI device scale").get();
            frame_state.set_window_system(Some(Value::symbol(gui_window_system_symbol())));
            frame_state.install_gnu_gui_default_parameters();
            frame_state.set_display_identity(display_identity);
            frame_state.set_parameter(
                Value::symbol("display-type"),
                Value::symbol(display_type.expect("GUI display type").symbol()),
            );
            frame_state.set_parameter(
                Value::symbol("background-mode"),
                Value::symbol(background_mode.expect("GUI background mode").symbol()),
            );
        } else {
            frame_state.set_window_system(None);
            frame_state.set_display_identity(FrameDisplayIdentity::default());
            frame_state.remove_parameter(Value::symbol("display-type"));
            frame_state.remove_parameter(Value::symbol("background-mode"));
        }
        frame_state.set_known_parameter(FrameParam::Font, font.name);
        frame_state.set_parameter(Value::symbol("font-parameter"), font.parameter);
        frame_state.font_pixel_size = font_pixel_size;
        if gui {
            frame_state.char_width = character_width;
            frame_state.char_height = character_height;
        } else {
            frame_state.char_width = 1.0;
            frame_state.char_height = 1.0;
            if let Some(minibuffer_window) = frame_state.minibuffer_leaf.as_mut() {
                let bounds = *minibuffer_window.bounds();
                minibuffer_window.set_bounds(Rect::new(bounds.x, bounds.y, bounds.width, 1.0));
            }
        }
        frame_state.sync_tab_bar_height_from_parameters();
        if !gui {
            frame_state.set_parameter(FrameParam::MenuBarLines.symbol(), Value::fixnum(1));
        }
        frame_state.sync_menu_bar_height_from_parameters();
        frame_state.sync_tool_bar_height_from_parameters();
        if let Window::Leaf {
            buffer_id,
            window_start,
            point,
            ..
        } = &mut frame_state.root_window
        {
            *buffer_id = scratch_buffer;
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
        }
    }
    evaluator.create_window_markers_for_root(frame, scratch_buffer);

    if gui {
        gui_setup(evaluator, frame);
        if let Some(frame_state) = evaluator.frame_manager_mut().get_mut(frame) {
            frame_state.set_known_parameter(FrameParam::Font, font.name);
            frame_state.set_parameter(Value::symbol("font-parameter"), font_snapshot);
            frame_state.font_pixel_size = font_pixel_size;
            frame_state.char_width = character_width;
            frame_state.char_height = character_height;
        }
        neovm_core::emacs_core::font::seed_live_frame_default_face_from_font_parameter(
            evaluator, frame,
        );
        evaluator.sync_runtime_faces_for_frame(frame);
    } else {
        evaluator.set_face_attribute("default", LFaceAttr::Foreground, FaceAttrValue::Unspecified);
        evaluator.set_face_attribute("default", LFaceAttr::Background, FaceAttrValue::Unspecified);
    }
    neovm_core::emacs_core::eval::restore_scratch_gc_roots(root_scope);

    if let Some(frame_state) = evaluator.frame_manager_mut().get_mut(frame) {
        let minibuffer_height = frame_state.char_height.max(1.0);
        let minibuffer_y = height as f32 - minibuffer_height;
        if let Window::Leaf { bounds, .. } = &mut frame_state.root_window {
            bounds.height = minibuffer_y;
        }
        if let Some(minibuffer_window) = &mut frame_state.minibuffer_leaf
            && let Window::Leaf {
                buffer_id,
                window_start,
                point,
                bounds,
                ..
            } = minibuffer_window
        {
            *buffer_id = minibuffer;
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
            bounds.y = minibuffer_y;
            bounds.height = minibuffer_height;
            bounds.width = width as f32;
        }
    }
    evaluator.create_window_markers_for_minibuffer(frame, minibuffer);

    InitialEditorSurface {
        scratch_buffer,
        minibuffer,
        frame,
    }
}
