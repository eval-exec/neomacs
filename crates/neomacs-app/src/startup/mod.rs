//! Host-neutral materialization of an interactive GUI startup session.

use std::path::{Path, PathBuf};

use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::error::EvalError;
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::terminal::pure::{
    TerminalRuntimeConfig, ensure_terminal_runtime_owner,
};
use neovm_core::window::FrameId;

use crate::initial_surface::InitialEditorSurface;

/// Host-owned identity and paths for one interactive GUI invocation.
#[derive(Clone, Debug)]
pub struct InteractiveGuiStartup {
    invocation_name: String,
    invocation_directory: PathBuf,
    default_directory: PathBuf,
    arguments: Vec<String>,
}

impl InteractiveGuiStartup {
    /// Describe an invocation before optional command-line arguments.
    #[must_use]
    pub fn new(
        invocation_name: impl Into<String>,
        invocation_directory: impl Into<PathBuf>,
        default_directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            invocation_name: invocation_name.into(),
            invocation_directory: invocation_directory.into(),
            default_directory: default_directory.into(),
            arguments: Vec::new(),
        }
    }

    /// Append arguments which GNU startup should process after argv[0].
    #[must_use]
    pub fn with_arguments(
        mut self,
        arguments: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.arguments = arguments.into_iter().map(Into::into).collect();
        self
    }
}

/// Establish the portable Lisp-visible half of one interactive GUI startup.
///
/// The native window, renderer, initial frame graph, and host paths already
/// exist when this runs. This function owns the corresponding GNU startup
/// invariants so platform adapters do not manipulate tagged Lisp values.
pub fn configure_interactive_gui_startup(
    evaluator: &mut Context,
    surface: InitialEditorSurface,
    startup: &InteractiveGuiStartup,
) -> Result<(), EvalError> {
    evaluator.set_variable("dump-mode", Value::NIL);
    evaluator.set_variable("neomacs--startup-gc-ceiling-active", Value::T);

    let mut argv = Vec::with_capacity(startup.arguments.len() + 1);
    argv.push(Value::string(startup.invocation_name.clone()));
    argv.extend(startup.arguments.iter().cloned().map(Value::string));
    evaluator.set_variable("command-line-args", Value::list(argv));
    evaluator.set_variable(
        "command-line-args-left",
        Value::list(
            startup
                .arguments
                .iter()
                .cloned()
                .map(Value::string)
                .collect(),
        ),
    );
    evaluator.set_variable("command-line-processed", Value::NIL);
    evaluator.set_variable("noninteractive", Value::NIL);
    evaluator.set_variable("undo-outer-limit", Value::fixnum(24_000_000));
    evaluator.set_variable("no-site-lisp", Value::NIL);
    evaluator.set_variable("build-details", Value::T);
    evaluator.set_variable(
        "invocation-name",
        Value::string(startup.invocation_name.clone()),
    );
    evaluator.set_variable(
        "invocation-directory",
        Value::unibyte_string(lisp_directory_name(&startup.invocation_directory)),
    );
    evaluator.set_variable(
        "default-directory",
        Value::unibyte_string(lisp_directory_name(&startup.default_directory)),
    );

    let frame = Value::make_frame(surface.frame().0);
    let terminal_frame = ensure_gnu_startup_terminal_frame(evaluator, surface.frame());
    evaluator.set_variable("terminal-frame", Value::make_frame(terminal_frame.0));
    evaluator.set_variable("frame-initial-frame", frame);
    evaluator.set_variable("default-minibuffer-frame", frame);
    evaluator.set_variable("inhibit-startup-screen", Value::T);

    evaluator.eval_str(
        "(progn
           (if (fboundp 'abbreviate-file-name)
               (setq default-directory (abbreviate-file-name default-directory)))
           (if (fboundp 'set-window-buffer)
               (set-window-buffer (selected-window) (current-buffer)))
           (if (fboundp 'frame-set-background-mode)
               (frame-set-background-mode (selected-frame) t))
           (if (fboundp 'face-set-after-frame-default)
               (face-set-after-frame-default (selected-frame))))",
    )?;
    Ok(())
}

fn ensure_gnu_startup_terminal_frame(evaluator: &mut Context, opening_frame: FrameId) -> FrameId {
    const STARTUP_TERMINAL_ID: u64 = 1;

    if let Some(existing) = evaluator
        .frame_manager()
        .frame_list()
        .into_iter()
        .find(|candidate| {
            *candidate != opening_frame
                && evaluator
                    .frame_manager()
                    .get(*candidate)
                    .is_some_and(|frame| {
                        !frame.visible && frame.effective_window_system().is_none()
                    })
        })
    {
        return existing;
    }

    let seed_buffer = evaluator
        .buffer_manager()
        .current_buffer_id()
        .or_else(|| evaluator.buffer_manager().find_buffer_by_name("*scratch*"))
        .unwrap_or_else(|| evaluator.buffer_manager_mut().create_buffer("*scratch*"));
    let (width, height, display_type, background_mode, environment) = evaluator
        .frame_manager()
        .get(opening_frame)
        .map(|frame| {
            (
                frame.width.max(1),
                frame.height.max(1),
                frame
                    .parameter("display-type")
                    .unwrap_or_else(|| Value::symbol("color")),
                frame
                    .parameter("background-mode")
                    .unwrap_or_else(|| Value::symbol("light")),
                frame.parameter("environment"),
            )
        })
        .unwrap_or((80, 25, Value::symbol("color"), Value::symbol("light"), None));
    ensure_terminal_runtime_owner(
        STARTUP_TERMINAL_ID,
        "startup_terminal",
        TerminalRuntimeConfig::inactive(),
    );
    let terminal_frame = evaluator.frame_manager_mut().create_frame_on_terminal(
        "Fstartup-tty",
        STARTUP_TERMINAL_ID,
        width,
        height,
        seed_buffer,
    );
    if let Some(frame) = evaluator.frame_manager_mut().get_mut(terminal_frame) {
        frame.visible = false;
        frame.set_window_system(None);
        frame.set_parameter(Value::symbol("display-type"), display_type);
        frame.set_parameter(Value::symbol("background-mode"), background_mode);
        if let Some(environment) = environment {
            frame.set_parameter(Value::symbol("environment"), environment);
        }
    }
    terminal_frame
}

fn lisp_directory_name(path: &Path) -> String {
    let mut name = path.to_string_lossy().replace('\\', "/");
    if !name.ends_with('/') {
        name.push('/');
    }
    name
}
