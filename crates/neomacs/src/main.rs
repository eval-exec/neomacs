//! Neomacs — standalone Rust binary
//!
//! Uses the neovm-core Elisp evaluator with a GNU Emacs-compatible command
//! loop.  The evaluator's `recursive_edit()` drives the main event loop:
//!
//!   read_char() → key-binding → command-execute → redisplay
//!
//! All editing commands, keybindings, and user customizations come from Elisp
//! (loaded .el files), just like GNU Emacs.  Only the core command loop and
//! low-level primitives are implemented in Rust.

// Several CLI/redisplay entry points here take many positional parameters;
// folding them into structs is a separate refactor, out of scope for the gate.
#![allow(clippy::too_many_arguments)]

// Global allocator: TiKV jemalloc is the Linux production default; mimalloc
// remains the default elsewhere and is available for Linux throughput
// comparisons. `--no-default-features` with neither explicit allocator feature
// uses the system allocator. These affect Rust allocations, not linked C
// libraries.
// `cfg(not(test))`: `lib.rs` re-includes this file as `pub mod startup`, so a
// test target that links the neomacs lib would otherwise declare a SECOND
// `#[global_allocator]` and fail to compile ("conflicts with global allocator
// in: neomacs"). The product binary is never built with `cfg(test)`, so its
// allocator selection is untouched; test binaries use the system allocator.
#[cfg(not(test))]
cfg_select! {
    any(
        all(feature = "mimalloc", feature = "jemalloc"),
        all(feature = "platform-allocator", feature = "mimalloc"),
        all(feature = "platform-allocator", feature = "jemalloc"),
    ) => {
        compile_error!(
            "features `platform-allocator`, `mimalloc`, and `jemalloc` are mutually exclusive"
        );
    }
    feature = "platform-allocator" => {
        #[global_allocator]
        static GLOBAL: neomacs_allocator::PlatformAllocator =
            neomacs_allocator::PLATFORM_ALLOCATOR;
    }
    all(feature = "mimalloc", target_os = "linux") => {
        // mimalloc initializes its environment options from an ELF constructor
        // with priority 101. Install Neomacs' default immediately before that:
        // `set_default` changes the compiled fallback without marking the
        // option initialized, so MIMALLOC_ARENA_EAGER_COMMIT can still override
        // it when mimalloc processes the environment.
        unsafe extern "C" fn configure_mimalloc_before_process_init() {
            // mimalloc option 4 is `mi_option_arena_eager_commit`.
            unsafe { libmimalloc_sys::mi_option_set_default(4, 0) };
        }

        #[used]
        #[unsafe(link_section = ".init_array.00100")]
        static CONFIGURE_MIMALLOC: unsafe extern "C" fn() =
            configure_mimalloc_before_process_init;

        #[global_allocator]
        static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
    }
    feature = "mimalloc" => {
        #[global_allocator]
        static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
    }
    feature = "jemalloc" => {
        #[global_allocator]
        static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
    }
    _ => {}
}

cfg_select! {
    all(
        target_os = "linux",
        any(feature = "platform-allocator", feature = "jemalloc"),
    ) => {
        // jemalloc reads this symbol before main. Two arenas for the main and
        // render workloads; _RJEM_MALLOC_CONF still overrides these defaults.
        //
        // Decay is DEFERRED (jemalloc's stock 10s), not immediate: with
        // decay 0 the pdump-load/startup churn issued 157 madvise
        // (MADV_DONTNEED) calls discarding 20.6 MiB that startup promptly
        // re-touched — ~1.4K extra minor faults per launch (measured
        // 11,200 -> ~9,780 with purging deferred). Startup's transient peak
        // is bounded by the explicit one-shot purge in
        // `jemalloc_release_startup_slack` once the evaluator is up.
        union JemallocConfigPointer {
            byte: &'static u8,
            c_char: &'static libc::c_char,
        }

        #[unsafe(export_name = "_rjem_malloc_conf")]
        pub static JEMALLOC_CONFIG: Option<&'static libc::c_char> = Some(unsafe {
            JemallocConfigPointer {
                byte: &b"dirty_decay_ms:10000,muzzy_decay_ms:10000,narenas:2\0"[0],
            }
            .c_char
        });
    }
    _ => {}
}

mod args;
mod build_info;
pub(crate) mod frame_layout;
mod image_catalog;
mod input_bridge;
mod secondary_tty;
mod termcap_input;
pub(crate) mod terminal_capabilities;
pub(crate) mod tty_frontend;
pub(crate) mod tty_init;

#[cfg(feature = "neo-term")]
use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use neomacs_app::frontend_event::FrontendEvent;
use neomacs_app::host::{ExecutionEngine, HostProfile};
use neomacs_display_protocol::{VideoId, VisualConfig, WebViewId};
use neomacs_display_runtime::display_scale::observe_event_loop_display;
#[cfg(not(feature = "neo-term"))]
use neomacs_display_runtime::render_thread::run_render_loop_current_thread;
#[cfg(feature = "neo-term")]
use neomacs_display_runtime::render_thread::run_render_loop_current_thread_with_terminals;
use neomacs_display_runtime::render_thread::{
    RenderEventLoop, RenderEventLoopProxy, RenderUserEvent, SharedImageRenderState,
    SharedMonitorInfo, build_render_event_loop,
};
use neomacs_display_runtime::shader_surface::{
    SurfaceChannelSource as RendererChannelSource, SurfaceShaderLanguage as RendererShaderLanguage,
    SurfaceUniformInit, validate_surface_glsl, validate_surface_wgsl,
};
#[cfg(feature = "video")]
use neomacs_display_runtime::thread_comm::VideoSessionCommand;
use neomacs_display_runtime::thread_comm::{
    AssetCommand, ClipboardCommand, ClipboardSelection, ConfigCommand, EmacsComms, FrameRef,
    FrameShaderAvailability, FrameShaderExecution, FrameShaderRequestId,
    InputEvent as DisplayInputEvent, LifecycleCommand, RenderCommand, SharedRenderCapabilities,
    SurfaceSource, ThreadComms, UiCommand, WindowCommand, WindowFullscreenMode, WindowVisibility,
};
#[cfg(feature = "neo-term")]
use neomacs_display_runtime::{
    terminal::{SharedTerminals, new_shared_terminals},
    thread_comm::TerminalCommand,
};
use neomacs_layout_engine::font::metrics::{FontMetricsService, SelectedFontInfo};
use neomacs_layout_engine::font::sizing::{
    FontSizing, FrameFontScalePolicy, ResolvedFrameFontScale, resolve_frame_font_scale,
};
use neomacs_layout_engine::gui_chrome::{
    collect_gui_menu_bar_items_for_frame, collect_gui_tool_bar_items_for_frame,
    compact_bar_mode_enabled,
};
#[cfg(feature = "video")]
use neomacs_video_model::{InitialPlayback, LoopMode, VideoSource};
use neomacs_video_model::{PlaybackAction, VideoOpenRequest};
use neomacs_webview::{
    BrowsingRelationship, NavigationTarget, ScriptRequest, ScriptRequestId, ScriptWorld,
    StoragePartition, WebContentSize, WebProfileId, WebViewCommand, WebViewCreate, WebViewPolicy,
};

use neovm_core::buffer::{BufferId, EmacsBytePos, EmacsByteRange, LispCharPos1};
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::builtins::set_neomacs_monitor_info;
use neovm_core::emacs_core::display::gui_window_system_symbol;
use neovm_core::emacs_core::display_host::{
    AvailableFontFamilyName, FontResolveRequest, FrameFontRequest, XwidgetScriptRequestId,
};
#[cfg(feature = "neo-term")]
use neovm_core::emacs_core::display_host::{
    TerminalCreateRequest, TerminalFloatPlacement, TerminalGridSize, TerminalId,
};
#[cfg(feature = "video")]
use neovm_core::emacs_core::eval::VideoResolveSource;
use neovm_core::emacs_core::eval::{
    FontEntityMetricsRequest, FontOtfCapability, FontSpecResolveRequest, GuiFrameHostSize,
    ResolvedFontEntityMetrics, ResolvedFontMatch, ResolvedFontSpecMatch, ResolvedFrameFont,
    ResolvedOpenedFont, ResolvedSurface, ResolvedVideo, ResolvedWebKit, ShaderSurfaceContent,
    ShaderSurfaceCreateRequest, ShaderSurfaceLanguage, ShaderSurfaceUniformInit,
    SurfaceChannelKind, SurfaceResolveRequest, VideoResolveRequest, WebKitResolveRequest,
    WebKitResolveSource,
};
use neovm_core::emacs_core::image_catalog::{ImageCatalog, ImageResolveRequest, ReadyImage};
use neovm_core::emacs_core::intern::intern;
use neovm_core::emacs_core::load::{
    LoadupDumpInvocation, LoadupDumpMode, LoadupInvocation, RuntimeImageRole,
    find_file_in_load_path, get_load_path, load_file,
};
#[cfg(test)]
use neovm_core::emacs_core::print_value_with_eval;
use neovm_core::emacs_core::terminal::pure::{
    TerminalRuntimeConfig, configure_terminal_runtime, ensure_terminal_runtime_owner,
    reset_terminal_host, reset_terminal_runtime, set_terminal_host,
};
use neovm_core::emacs_core::value::list_length;
use neovm_core::emacs_core::{
    Context, DisplayHost, GraphicalFaceAttribute, GuiFrameHostRequest, PopupMenuRequest,
};
use neovm_core::face::{FaceHeight, FontWeight, LFaceAttr};
use neovm_core::heap_types::LispString;
use neovm_core::window::{
    FrameDisplayIdentity, FrameFullscreen, FrameId, FrameParam, FrameVisibility, Window,
};

use image_catalog::AsyncImageCatalog;

#[derive(Debug, Clone, PartialEq, Eq)]
// Variants share a `Print*` prefix by design (all are print-and-exit CLI
// actions); the naming lint is allowed here.
#[allow(clippy::enum_variant_names)]
enum EarlyCliAction {
    PrintHelp { program: String },
    PrintVersion,
    PrintFingerprint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrontendKind {
    Gui,
    Tty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMode {
    Raw,
    BootstrapUse,
    FinalRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DumpImageKind {
    Bootstrap,
    Final,
}

/// Capabilities supplied by the native desktop application adapter.
///
/// Keep feature-to-capability translation at the product edge. The evaluator
/// and shared application modules must consume this typed profile as they move
/// behind the host boundary; they must not infer host semantics from Cargo
/// features themselves.
pub(crate) const fn desktop_host_profile() -> HostProfile {
    let execution = if cfg!(feature = "jit") {
        ExecutionEngine::NativeJit
    } else {
        ExecutionEngine::Interpreter
    };
    HostProfile::desktop(execution)
}

impl RuntimeMode {
    pub const fn binary_name(self) -> &'static str {
        match self {
            Self::Raw => "neomacs-temacs",
            Self::BootstrapUse => "bootstrap-neomacs",
            Self::FinalRun => "neomacs",
        }
    }

    pub const fn dump_image_kind(self) -> Option<DumpImageKind> {
        match self {
            Self::Raw => None,
            Self::BootstrapUse => Some(DumpImageKind::Bootstrap),
            Self::FinalRun => Some(DumpImageKind::Final),
        }
    }
}

fn runtime_mode_from_program_name(program: &str) -> RuntimeMode {
    let file_name = Path::new(program)
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new(program))
        .to_string_lossy();
    let file_name = file_name.strip_suffix(".exe").unwrap_or(&file_name);
    match file_name {
        "neomacs-temacs" => RuntimeMode::Raw,
        "bootstrap-neomacs" => RuntimeMode::BootstrapUse,
        _ => RuntimeMode::FinalRun,
    }
}

fn runtime_mode_from_argv<I, S>(args: I) -> RuntimeMode
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .next()
        .map(|arg| runtime_mode_from_program_name(arg.as_ref()))
        .unwrap_or(RuntimeMode::FinalRun)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartupOptions {
    frontend: FrontendKind,
    forwarded_args: Vec<String>,
    terminal_device: Option<String>,
    noninteractive: bool,
    temacs_mode: Option<LoadupDumpMode>,
    dump_file_override: Option<PathBuf>,
    /// Set by `-Q` (peek) and `-x` (consumed). Mirrors GNU
    /// `no_site_lisp` at emacs.c:2126/2135.
    no_site_lisp: bool,
    /// Set by `-nl` / `--no-loadup`. Mirrors GNU `no_loadup` at
    /// emacs.c:2031. Only meaningful in `RuntimeMode::Raw`, where it
    /// suppresses the `-l loadup` splice that would otherwise force
    /// `loadup.el` to run.
    no_loadup: bool,
    /// Set by `-no-build-details` / `--no-build-details`. Mirrors GNU
    /// `build_details` at emacs.c:2037 (where the negation is taken).
    /// When true, build-time strings (e.g. `emacs-build-time`) should
    /// be cleared rather than populated.
    no_build_details: bool,
}

/// Whether a run is interactive (a real terminal/GUI session) or batch
/// (`--batch`).  Carried in `BootstrapDisplayConfig` because `bootstrap_buffers`
/// runs before the obarray `noninteractive` value is seeded, so the obarray
/// slot still holds the stale pdump value at that point and cannot be trusted
/// to mark the initial frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Interactivity {
    Interactive,
    Batch,
}

impl Interactivity {
    fn from_noninteractive(noninteractive: bool) -> Self {
        if noninteractive {
            Interactivity::Batch
        } else {
            Interactivity::Interactive
        }
    }

    fn is_batch(self) -> bool {
        matches!(self, Interactivity::Batch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BootstrapDisplayConfig {
    kind: BootstrapDisplayKind,
    color_cells: i64,
    background_mode: &'static str,
    interactivity: Interactivity,
}

/// Display kind and its scale facts available before a native window exists.
///
/// GUI startup retains only the resolved logical font rule. Device scale is
/// deliberately absent until winit realizes each particular window.
#[derive(Debug, Clone, Copy, PartialEq)]
enum BootstrapDisplayKind {
    Gui {
        frame_font_scale: ResolvedFrameFontScale,
    },
    Tty {
        font_sizing: FontSizing,
    },
}

impl BootstrapDisplayConfig {
    fn frontend(self) -> FrontendKind {
        match self.kind {
            BootstrapDisplayKind::Gui { .. } => FrontendKind::Gui,
            BootstrapDisplayKind::Tty { .. } => FrontendKind::Tty,
        }
    }

    fn font_sizing(self) -> FontSizing {
        match self.kind {
            BootstrapDisplayKind::Gui { frame_font_scale } => frame_font_scale.font_sizing(),
            BootstrapDisplayKind::Tty { font_sizing } => font_sizing,
        }
    }

    #[cfg(test)]
    fn frame_font_scale(self) -> Option<ResolvedFrameFontScale> {
        match self.kind {
            BootstrapDisplayKind::Gui { frame_font_scale } => Some(frame_font_scale),
            BootstrapDisplayKind::Tty { .. } => None,
        }
    }
}

fn gui_display_identity(
    wayland_display: Option<&str>,
    x_display: Option<&str>,
) -> FrameDisplayIdentity {
    let wayland_display = wayland_display
        .filter(|display| !display.is_empty())
        .map(FrameDisplayIdentity::wayland);
    let x_display = x_display
        .filter(|display| !display.is_empty())
        .map(FrameDisplayIdentity::x11);
    wayland_display.or(x_display).unwrap_or_default()
}

fn host_gui_display_identity() -> FrameDisplayIdentity {
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let x_display = std::env::var("DISPLAY").ok();
    gui_display_identity(wayland_display.as_deref(), x_display.as_deref())
}

const EARLY_HELP_BODY: &str = concat!(
    "Run Neomacs, the extensible, customizable, self-documenting real-time\n",
    "display editor.  The recommended way to start Neomacs for normal editing\n",
    "is with no options at all.\n",
    "\n",
    "Run M-x info RET m emacs RET m emacs invocation RET inside Emacs to\n",
    "read the main documentation for these command-line arguments.\n",
    "\n",
    "Initialization options:\n",
    "\n",
    "--batch                     do not do interactive display; implies -q\n",
    "--chdir DIR                 change to directory DIR\n",
    "--daemon, --bg-daemon[=NAME] start a (named) server in the background\n",
    "--fg-daemon[=NAME]          start a (named) server in the foreground\n",
    "--debug-init                enable Emacs Lisp debugger for init file\n",
    "--display, -d DISPLAY       use X server DISPLAY\n",
    "--no-build-details          do not add build details such as time stamps\n",
    "--no-desktop                do not load a saved desktop\n",
    "--no-init-file, -q          load neither ~/.emacs nor default.el\n",
    "--no-loadup, -nl            do not load loadup.el into bare Emacs\n",
    "--no-site-file              do not load site-start.el\n",
    "--no-x-resources            do not load X resources\n",
    "--no-site-lisp, -nsl        do not add site-lisp directories to load-path\n",
    "--no-splash                 do not display a splash screen on startup\n",
    "--no-window-system, -nw     do not communicate with X, ignoring $DISPLAY\n",
    "--init-directory=DIR        use DIR when looking for the Emacs init files.\n",
    "--quick, -Q                 equivalent to:\n",
    "                              -q --no-site-file --no-site-lisp --no-splash\n",
    "                              --no-x-resources\n",
    "--script FILE               run FILE as an Emacs Lisp script\n",
    "-x                          to be used in #!/usr/bin/emacs -x\n",
    "                              and has approximately the same meaning\n",
    "                              as -Q --script\n",
    "--terminal, -t DEVICE       use DEVICE for terminal I/O\n",
    "--user, -u USER             load ~USER/.emacs instead of your own\n",
    "\n",
    "Action options:\n",
    "\n",
    "FILE                    visit FILE\n",
    "+LINE                   go to line LINE in next FILE\n",
    "+LINE:COLUMN            go to line LINE, column COLUMN, in next FILE\n",
    "--directory, -L DIR     prepend DIR to load-path (with :DIR, append DIR)\n",
    "--eval EXPR             evaluate Emacs Lisp expression EXPR\n",
    "--execute EXPR          evaluate Emacs Lisp expression EXPR\n",
    "--file FILE             visit FILE\n",
    "--find-file FILE        visit FILE\n",
    "--funcall, -f FUNC      call Emacs Lisp function FUNC with no arguments\n",
    "--insert FILE           insert contents of FILE into current buffer\n",
    "--kill                  exit without asking for confirmation\n",
    "--load, -l FILE         load Emacs Lisp FILE using the load function\n",
    "--visit FILE            visit FILE\n",
    "\n",
    "Display options:\n",
    "\n",
    "--background-color, -bg COLOR   window background color\n",
    "--basic-display, -D             disable many display features;\n",
    "                                  used for debugging Emacs\n",
    "--border-color, -bd COLOR       main border color\n",
    "--border-width, -bw WIDTH       width of main border\n",
    "--cursor-color, -cr COLOR       color of the Emacs cursor indicating point\n",
    "--font, -fn FONT                default font; must be fixed-width\n",
    "--foreground-color, -fg COLOR   window foreground color\n",
    "--fullheight, -fh               make the first frame high as the screen\n",
    "--fullscreen, -fs               make the first frame fullscreen\n",
    "--fullwidth, -fw                make the first frame wide as the screen\n",
    "--maximized, -mm                make the first frame maximized\n",
    "--geometry, -g GEOMETRY         window geometry\n",
    "--iconic                        start Neomacs in iconified state\n",
    "--internal-border, -ib WIDTH    width between text and main border\n",
    "--line-spacing, -lsp PIXELS     additional space to put between lines\n",
    "--mouse-color, -ms COLOR        mouse cursor color in Neomacs window\n",
    "--name NAME                     title for initial Neomacs frame\n",
    "--no-blinking-cursor, -nbc      disable blinking cursor\n",
    "--reverse-video, -r, -rv        switch foreground and background\n",
    "--title, -T TITLE               title for initial Neomacs frame\n",
    "--vertical-scroll-bars, -vb     enable vertical scroll bars\n",
    "--xrm XRESOURCES                set additional X resources\n",
    "--parent-id XID                 set parent window\n",
    "--help                          display this help and exit\n",
    "--fingerprint                   output fingerprint and exit\n",
    "--version                       output version information and exit\n",
    "\n",
    "You can generally also specify long option names with a single -; for\n",
    "example, -batch as well as --batch.  You can use any unambiguous\n",
    "abbreviation for a --option.\n",
    "\n",
    "Various environment variables and window system resources also affect\n",
    "the operation of Neomacs.  See the main documentation.\n",
    "\n",
    "Report bugs to https://github.com/eval-exec/neomacs-windows/issues.\n",
);

const BOOTSTRAP_CORE_FEATURES: &[&str] = &[];

fn classify_early_cli_action(args: impl IntoIterator<Item = String>) -> Option<EarlyCliAction> {
    let mut args = args.into_iter();
    let program = args.next().unwrap_or_else(|| "neomacs".to_string());
    for arg in args {
        if arg == "--" {
            break;
        }
        match arg.as_str() {
            "--help" | "-help" => {
                return Some(EarlyCliAction::PrintHelp { program });
            }
            "--version" | "-version" => {
                return Some(EarlyCliAction::PrintVersion);
            }
            "--fingerprint" | "-fingerprint" => {
                return Some(EarlyCliAction::PrintFingerprint);
            }
            _ => {}
        }
    }
    None
}

fn render_help_text(program: &str) -> String {
    let mut out = String::new();
    let _ = write!(&mut out, "Usage: {program} [OPTION-OR-FILENAME]...\n\n");
    out.push_str(EARLY_HELP_BODY);
    out
}

fn render_version_text() -> String {
    let mut version = format!("Neomacs {}\n", neomacs_display_runtime::VERSION);
    build_info::write_build_provenance(&mut version);
    version.push_str("Standalone Rust binary for Neomacs (no C dependency)\n");
    version
}

fn render_fingerprint_text() -> String {
    format!("{}\n", neovm_core::emacs_core::pdump::fingerprint_hex())
}

fn render_startup_image_error(err: &neovm_core::emacs_core::error::EvalError) -> String {
    match err {
        neovm_core::emacs_core::error::EvalError::Signal {
            raw_data: Some(payload),
            ..
        } => payload
            .as_symbol_name()
            .map(str::to_owned)
            .or_else(|| payload.as_utf8_str().map(str::to_owned))
            .unwrap_or_else(|| format!("{err:?}")),
        _ => format!("{err:?}"),
    }
}

fn parse_startup_options(args: impl IntoIterator<Item = String>) -> Result<StartupOptions, String> {
    use args::{ArgMatch, argmatch, sort_args};

    // GNU `argmatch` works on a `(argc, argv)` pair plus a `*skipptr`
    // index that mirrors the consumed cursor in argv. We model the same
    // shape: `parsed[0]` is the program name (matching argv[0]) and
    // `parsed[1..]` are the user-supplied tokens. The `idx` cursor below
    // is `*skipptr` — `argmatch` looks at `parsed[idx + 1]` so an idx of
    // 0 means "look at the first user token".
    let mut parsed: Vec<String> = args.into_iter().collect();

    // GNU emacs.c:1502 — sort_args runs once before the main matching
    // pass so the parser walks argv in canonical priority order. This
    // also has the effect of moving option/value pairs in front of
    // file-name args, matching how lisp/startup.el's `command-line` and
    // `command-line-1` expect to see them regardless of how the user
    // typed them on the command line.
    sort_args(&mut parsed)?;

    let program = parsed
        .first()
        .cloned()
        .unwrap_or_else(|| "neomacs".to_string());
    let mut forwarded_args = vec![program];
    let mut frontend = FrontendKind::Gui;
    let mut terminal_device = None;
    let mut noninteractive = false;
    let mut temacs_mode = None;
    let mut dump_file_override = None;
    let mut no_site_lisp = false;
    let mut no_loadup = false;
    let mut no_build_details = false;
    let mut idx = 0usize;

    while idx + 1 < parsed.len() {
        // GNU walks argv left-to-right inside `main()` after `sort_args`
        // has reordered things. We don't reorder yet (that's Phase 2), so
        // we walk the original token order. Each `argmatch` call looks at
        // `parsed[idx + 1]`; on a match it advances `idx` past the
        // consumed entry/entries. On no-match we drop to the catch-all
        // forwarding branch and bump `idx` ourselves.
        let next = parsed[idx + 1].as_str();

        // `--` is the terminator: every following token is forwarded
        // verbatim and parsing stops here.
        if next == "--" {
            forwarded_args.extend(parsed[idx + 1..].iter().cloned());
            break;
        }

        // -chdir / --chdir DIR (GNU emacs.c:1538-1561). Must run before
        // any later parsing or file resolution: GNU calls chdir() at
        // line 1549, so subsequent file-name args see the new cwd.
        match argmatch(&parsed, &mut idx, "-chdir", Some("--chdir"), 4, true) {
            ArgMatch::Value(dir) => {
                if let Err(e) = std::env::set_current_dir(&dir) {
                    return Err(format!("neomacs: Can't chdir to {dir}: {e}"));
                }
                continue;
            }
            ArgMatch::MissingValue => {
                return Err("neomacs: option `-chdir' requires an argument".to_string());
            }
            ArgMatch::NoMatch => {}
            ArgMatch::Bare => unreachable!(),
        }

        // -nw / --no-window-system / --no-windows
        // (GNU emacs.c:1696-1697; the -nw row in standard_args[] declares
        // both long aliases with minlen 6.)
        match argmatch(
            &parsed,
            &mut idx,
            "-nw",
            Some("--no-window-system"),
            6,
            false,
        ) {
            ArgMatch::Bare => {
                frontend = FrontendKind::Tty;
                continue;
            }
            ArgMatch::NoMatch => {}
            ArgMatch::Value(_) | ArgMatch::MissingValue => unreachable!(),
        }
        match argmatch(&parsed, &mut idx, "-nw", Some("--no-windows"), 6, false) {
            ArgMatch::Bare => {
                frontend = FrontendKind::Tty;
                continue;
            }
            ArgMatch::NoMatch => {}
            ArgMatch::Value(_) | ArgMatch::MissingValue => unreachable!(),
        }

        // -batch / --batch (GNU emacs.c:1702)
        match argmatch(&parsed, &mut idx, "-batch", Some("--batch"), 5, false) {
            ArgMatch::Bare => {
                noninteractive = true;
                frontend = FrontendKind::Tty;
                continue;
            }
            ArgMatch::NoMatch => {}
            ArgMatch::Value(_) | ArgMatch::MissingValue => unreachable!(),
        }

        // -script FILE / --script FILE (GNU emacs.c:1708-1717). GNU
        // sets noninteractive, then rewrites the matched argv slot to
        // -scriptload (an internal flag picked up later by
        // lisp/startup.el's command-line-1) before re-sorting. We do
        // the same: noninteractive + push -scriptload FILE into the
        // forwarded args. Lisp's command-line-1 in startup.el:2841 will
        // pick it up and load FILE.
        match argmatch(&parsed, &mut idx, "-script", Some("--script"), 3, true) {
            ArgMatch::Value(script_file) => {
                noninteractive = true;
                frontend = FrontendKind::Tty;
                forwarded_args.push("-scriptload".to_string());
                forwarded_args.push(script_file);
                continue;
            }
            ArgMatch::MissingValue => {
                return Err("neomacs: option `-script' requires an argument".to_string());
            }
            ArgMatch::NoMatch => {}
            ArgMatch::Bare => unreachable!(),
        }

        // -x (GNU emacs.c:2132-2140). The `-x` form of shebang scripts:
        //   #!/usr/bin/neomacs -x
        // GNU sets noninteractive AND no_site_lisp, then rewrites argv
        // by replacing `-x` with the internal `-scripteval` flag.
        // lisp/startup.el:2841 picks up `-scripteval` and runs the
        // following file as evaluated text rather than loaded code.
        match argmatch(&parsed, &mut idx, "-x", None, 1, false) {
            ArgMatch::Bare => {
                noninteractive = true;
                frontend = FrontendKind::Tty;
                no_site_lisp = true;
                forwarded_args.push("-scripteval".to_string());
                continue;
            }
            ArgMatch::NoMatch => {}
            ArgMatch::Value(_) | ArgMatch::MissingValue => unreachable!(),
        }

        // -nl / --no-loadup (GNU emacs.c:2031-2032). Skip loading
        // loadup.el under RuntimeMode::Raw. Consumed entirely; no
        // forwarding.
        match argmatch(&parsed, &mut idx, "-nl", Some("--no-loadup"), 6, false) {
            ArgMatch::Bare => {
                no_loadup = true;
                continue;
            }
            ArgMatch::NoMatch => {}
            ArgMatch::Value(_) | ArgMatch::MissingValue => unreachable!(),
        }

        // -nsl / --no-site-lisp (GNU emacs.c:2034-2035). Drops site-lisp
        // directories from load-path before lread.c builds it.
        // Consumed entirely; no forwarding.
        match argmatch(&parsed, &mut idx, "-nsl", Some("--no-site-lisp"), 11, false) {
            ArgMatch::Bare => {
                no_site_lisp = true;
                continue;
            }
            ArgMatch::NoMatch => {}
            ArgMatch::Value(_) | ArgMatch::MissingValue => unreachable!(),
        }

        // -no-build-details / --no-build-details (GNU emacs.c:2037-2038).
        // Inverts the GNU `build_details` global; when set, build-time
        // strings (e.g. `emacs-build-time`) should be cleared.
        // Consumed entirely; no forwarding.
        match argmatch(
            &parsed,
            &mut idx,
            "-no-build-details",
            Some("--no-build-details"),
            7,
            false,
        ) {
            ArgMatch::Bare => {
                no_build_details = true;
                continue;
            }
            ArgMatch::NoMatch => {}
            ArgMatch::Value(_) | ArgMatch::MissingValue => unreachable!(),
        }

        // -temacs / --temacs (GNU emacs.c:1364). Forward the original
        // token(s) verbatim so any later consumer (Lisp or another
        // raw_loadup pass) sees the same shape GNU does — emacs.c
        // does NOT rewrite this slot, only the display slot.
        let pre_idx = idx;
        match argmatch(&parsed, &mut idx, "-temacs", Some("--temacs"), 8, true) {
            ArgMatch::Value(value) => {
                temacs_mode = Some(parse_temacs_mode(&value)?);
                for slot in &parsed[pre_idx + 1..=idx] {
                    forwarded_args.push(slot.clone());
                }
                continue;
            }
            ArgMatch::MissingValue => {
                return Err("neomacs: option `-temacs' requires an argument".to_string());
            }
            ArgMatch::NoMatch => {}
            ArgMatch::Bare => unreachable!(),
        }

        // -dump-file / --dump-file (GNU emacs.c:942, 991). Same forward-
        // verbatim treatment as --temacs.
        let pre_idx = idx;
        match argmatch(
            &parsed,
            &mut idx,
            "-dump-file",
            Some("--dump-file"),
            6,
            true,
        ) {
            ArgMatch::Value(value) => {
                dump_file_override = Some(PathBuf::from(&value));
                for slot in &parsed[pre_idx + 1..=idx] {
                    forwarded_args.push(slot.clone());
                }
                continue;
            }
            ArgMatch::MissingValue => {
                return Err("neomacs: option `-dump-file' requires an argument".to_string());
            }
            ArgMatch::NoMatch => {}
            ArgMatch::Bare => unreachable!(),
        }

        // -t / --terminal (GNU emacs.c:1665)
        match argmatch(&parsed, &mut idx, "-t", Some("--terminal"), 4, true) {
            ArgMatch::Value(device) => {
                frontend = FrontendKind::Tty;
                terminal_device = Some(device);
                continue;
            }
            ArgMatch::MissingValue => {
                return Err("neomacs: option `-t' requires an argument".to_string());
            }
            ArgMatch::NoMatch => {}
            ArgMatch::Bare => unreachable!(),
        }

        // -d / --display / -display (GNU emacs.c:2097-2099 — peek + roll
        // back). Our window backend uses winit which reads `DISPLAY` from
        // the environment, so we don't need to act on the value, but we
        // still consume it from argv structurally and re-forward it so
        // Lisp's `command-line-1` sees it where GNU does.
        match argmatch(&parsed, &mut idx, "-d", Some("--display"), 3, true) {
            ArgMatch::Value(value) => {
                forwarded_args.push("-d".to_string());
                forwarded_args.push(value);
                continue;
            }
            ArgMatch::MissingValue => {
                return Err("neomacs: option `-d' requires an argument".to_string());
            }
            ArgMatch::NoMatch => {}
            ArgMatch::Bare => unreachable!(),
        }
        // -display alone (no long form) — GNU emacs.c:2099 has lstr = 0
        // for this row. Use a None lstr to match.
        match argmatch(&parsed, &mut idx, "-display", None, 0, true) {
            ArgMatch::Value(value) => {
                forwarded_args.push("-display".to_string());
                forwarded_args.push(value);
                continue;
            }
            ArgMatch::MissingValue => {
                return Err("neomacs: option `-display' requires an argument".to_string());
            }
            ArgMatch::NoMatch => {}
            ArgMatch::Bare => unreachable!(),
        }

        // No flag matched at this position: forward verbatim.
        forwarded_args.push(parsed[idx + 1].clone());
        idx += 1;
    }

    if frontend == FrontendKind::Tty {
        // A TTY/batch session never opens the X display: GNU -batch/-nw do
        // not, and native display observation belongs to the GUI runtime.
        let mut tty_args = Vec::with_capacity(forwarded_args.len());
        if let Some(program) = forwarded_args.first() {
            tty_args.push(program.clone());
        }
        let mut arg_iter = forwarded_args.iter().skip(1);
        while let Some(arg) = arg_iter.next() {
            if matches!(
                arg.as_str(),
                "-d" | "-display" | "--display" | "--terminal" | "-t"
            ) {
                let _ = arg_iter.next();
                continue;
            }
            if arg.starts_with("--display=") || arg.starts_with("--terminal=") {
                continue;
            }
            tty_args.push(arg.clone());
        }
        forwarded_args = tty_args;
    }

    // -Q / --quick / -quick PEEK (GNU emacs.c:2123-2130). GNU walks
    // argv one more time looking for any of these three spellings; if
    // found, it sets `no_site_lisp = 1` and leaves the flag in argv so
    // lisp/startup.el (`command-line` at lisp/startup.el:1404) can also
    // act on it. Critically the flag is NOT consumed — only `no_site_lisp`
    // is updated as a side effect. This is the only "peek but do not
    // consume" idiom in GNU's parser.
    //
    // We replicate the same scan over `forwarded_args` (the survivors
    // of the consume pass) since that's what the rest of startup will
    // see. Skip if `no_site_lisp` is already set (e.g. by an earlier
    // -nsl or -x), matching GNU's `if (! no_site_lisp)` guard.
    if !no_site_lisp
        && forwarded_args
            .iter()
            .skip(1)
            .any(|a| a == "-Q" || a == "--quick" || a == "-quick")
    {
        no_site_lisp = true;
    }

    Ok(StartupOptions {
        frontend,
        forwarded_args,
        terminal_device,
        noninteractive,
        temacs_mode,
        dump_file_override,
        no_site_lisp,
        no_loadup,
        no_build_details,
    })
}

fn parse_temacs_mode(value: &str) -> Result<LoadupDumpMode, String> {
    match value {
        "pbootstrap" => Ok(LoadupDumpMode::Pbootstrap),
        "pdump" => Ok(LoadupDumpMode::Pdump),
        other => Err(format!("neomacs: invalid --temacs mode `{other}`")),
    }
}

fn bootstrap_tty_display_config(interactivity: Interactivity) -> BootstrapDisplayConfig {
    BootstrapDisplayConfig {
        kind: BootstrapDisplayKind::Tty {
            font_sizing: FontSizing::gnu_x11_fallback(),
        },
        color_cells: tty_init::detect_tty_color_cells(),
        background_mode: tty_init::detect_tty_background_mode(),
        interactivity,
    }
}

fn bootstrap_gui_display_config(
    interactivity: Interactivity,
    frame_font_scale: ResolvedFrameFontScale,
) -> BootstrapDisplayConfig {
    BootstrapDisplayConfig {
        kind: BootstrapDisplayKind::Gui { frame_font_scale },
        color_cells: 16777216,
        // GNU `frame--current-background-mode` defaults GUI frames to
        // `light` unless a real background color or terminal default says
        // otherwise. Live frame-parameter updates recompute this later.
        background_mode: "light",
        interactivity,
    }
}

impl BootstrapDisplayConfig {
    fn window_system_symbol(self) -> Option<&'static str> {
        match self.frontend() {
            FrontendKind::Gui => Some(gui_window_system_symbol()),
            FrontendKind::Tty => None,
        }
    }

    fn display_type_symbol(self) -> &'static str {
        if self.color_cells > 0 {
            "color"
        } else {
            "mono"
        }
    }
}

fn startup_dimensions(
    frontend: FrontendKind,
    frame_metrics: BootstrapFrameMetrics,
    noninteractive: bool,
) -> (u32, u32) {
    match frontend {
        FrontendKind::Gui => {
            // GNU gui_figure_window_size (frame.c) seeds the first GUI frame from
            // an 80x36 text grid, then adds the scroll bar, fringes, menu bar and
            // tool bar *outside* that text area. The window we request here is later
            // divided into chrome + text, so we must reserve BOTH the side and top
            // chrome up front — otherwise the scroll bar/fringes eat into the columns
            // (frame 78 wide) and the menu/tool bars eat into the lines (frame 33).
            //
            // Observed GNU `(frame-height)` is one LESS than its nominal geometry
            // rows — deterministic, not a WM trim: `-g 80x35`->34, `-g 80x36`->35,
            // `-g 80x40`->39, and the default (== -g 80x36) nets 35. GNU's default
            // GUI frame is therefore 80x35 of *counted* text; match that observable.
            let cols = 80u32;
            let text_rows = 35u32;
            // Side chrome the layout reserves outside the text columns: a default
            // vertical scroll bar (one char wide) plus the two 8px fringes.
            const DEFAULT_FRINGE_PX: f32 = 8.0;
            let side_chrome = frame_metrics.char_width + 2.0 * DEFAULT_FRINGE_PX;
            // Top chrome reserved above the text lines for a default GUI frame: a
            // one-line menu bar (char_height) plus the icon-height tool bar. Both
            // default on under -Q; if a user disables either this slightly
            // over-reserves — the same default-configuration assumption the side
            // chrome makes for the scroll bar. The tool-bar height mirrors GNU's
            // image + margin + relief model (window::default_gui_tool_bar_line_height).
            let menu_bar = frame_metrics.char_height;
            let tool_bar =
                neovm_core::window::default_gui_tool_bar_line_height(frame_metrics.font_pixel_size)
                    as f32;
            let top_chrome = menu_bar + tool_bar;
            let width = (cols as f32 * frame_metrics.char_width + side_chrome).round() as u32;
            let height = (text_rows as f32 * frame_metrics.char_height + top_chrome).round() as u32;
            (width.max(200), height.max(100))
        }
        FrontendKind::Tty => {
            if noninteractive {
                // GNU `make_frame` seeds the initial non-window frame with
                // total size 80x25, then gives the root window 24 lines plus
                // a one-line minibuffer.
                return (80, 25);
            }
            // TTY frames use 1x1 character cells (GNU Emacs frame.c:1184-1185),
            // so frame dimensions are in character cells, not pixels.
            let (cols, rows) = tty_init::query_terminal_size_cells().unwrap_or((80, 25));
            (cols, rows)
        }
    }
}

enum FrontendHandle {
    /// Single-thread TTY path: input reader only, rendering via TtyRif on eval thread.
    TtyRifInput(tty_frontend::TtyInputReader),
    Batch,
}

impl FrontendHandle {
    fn join(self) {
        match self {
            Self::TtyRifInput(handle) => handle.join(),
            Self::Batch => {}
        }
    }
}

#[derive(Clone)]
struct GuiEventLoopWaker {
    proxy: RenderEventLoopProxy,
}

impl GuiEventLoopWaker {
    fn new(proxy: RenderEventLoopProxy) -> Self {
        Self { proxy }
    }

    fn wake(&self) {
        if let Err(err) = self.proxy.send_event(RenderUserEvent::Wake) {
            tracing::debug!("GUI event loop wake dropped after loop closed: {err}");
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct EvaluatorExit {
    exit_code: i32,
    restart: bool,
}

impl EvaluatorExit {
    const OK: Self = Self {
        exit_code: 0,
        restart: false,
    };
}

const GUI_EVALUATOR_THREAD_STACK_SIZE: usize = 64 * 1024 * 1024;
const CLIPBOARD_REPLY_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(feature = "video")]
const VIDEO_DIAGNOSTICS_REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// FIFO cap for the declarative-surface memo: each resolved entry keeps a
/// live GPU texture on the render thread, so the memo must not grow without
/// bound.
const RESOLVED_SURFACE_MEMO_CAP: usize = 64;

/// Stable identities for declarative video specs.
///
/// This deliberately is not an eviction cache. A visible frame may refer to
/// any number of these ids, and the evaluator does not receive an accepted-
/// presentation lifetime with which it could prove an id dead. Native surface
/// pressure is bounded in `neomacs-video`; destroying a session requires an
/// explicit owner/lifetime signal rather than guessing from insertion order.
#[cfg(feature = "video")]
#[derive(Default)]
struct ResolvedVideoRegistry {
    entries: HashMap<VideoResolveRequest, ResolvedVideo>,
}

#[cfg(feature = "video")]
impl ResolvedVideoRegistry {
    fn get(&self, request: &VideoResolveRequest) -> Option<ResolvedVideo> {
        self.entries.get(request).cloned()
    }

    fn insert(&mut self, request: VideoResolveRequest, resolved: ResolvedVideo) {
        self.entries.insert(request, resolved);
    }
}

/// Memo for declarative `(surface :shader …)` display specs, FIFO-bounded at
/// [`RESOLVED_SURFACE_MEMO_CAP`] entries.
///
/// Inserting past the cap evicts the oldest entry and reports its surface id
/// so the caller can queue `AssetCommand::SurfaceFree` for the GPU objects.
/// Evicting a spec that is still displayed is safe: the resolver re-runs on
/// every redisplay walk of a visible spec, so an evicted-but-visible spec
/// simply re-creates its surface on the next walk — eviction costs a
/// re-resolve, never correctness.
#[derive(Default)]
struct ResolvedSurfaceMemo {
    entries: HashMap<SurfaceResolveRequest, Option<ResolvedSurface>>,
    /// Insertion order of `entries` keys, oldest first.
    order: VecDeque<SurfaceResolveRequest>,
}

impl ResolvedSurfaceMemo {
    fn get(&self, request: &SurfaceResolveRequest) -> Option<Option<ResolvedSurface>> {
        self.entries.get(request).cloned()
    }

    /// Insert a resolution, evicting the oldest entry once the cap is
    /// reached. Returns the surface id of an evicted *resolved* entry so the
    /// caller can free its GPU objects (failed resolutions memoize `None`
    /// and have nothing to free).
    fn insert(
        &mut self,
        request: SurfaceResolveRequest,
        resolved: Option<ResolvedSurface>,
    ) -> Option<u32> {
        if let std::collections::hash_map::Entry::Occupied(mut entry) =
            self.entries.entry(request.clone())
        {
            // Re-resolution of a known spec: refresh the value in place and
            // keep the FIFO position so `order` never holds duplicate keys.
            entry.insert(resolved);
            return None;
        }
        let evicted = if self.entries.len() >= RESOLVED_SURFACE_MEMO_CAP {
            self.order.pop_front().and_then(|oldest| {
                self.entries
                    .remove(&oldest)
                    .flatten()
                    .map(|old| old.surface_id)
            })
        } else {
            None
        };
        self.order.push_back(request.clone());
        self.entries.insert(request, resolved);
        evicted
    }
}

struct PrimaryWindowDisplayHost {
    cmd_tx: crossbeam_channel::Sender<RenderCommand>,
    render_waker: Option<GuiEventLoopWaker>,
    font_sizing: FontSizing,
    primary_window_adopted: bool,
    primary_frame_id: Option<neovm_core::window::FrameId>,
    last_window_titles: Mutex<HashMap<neovm_core::window::FrameId, LispString>>,
    font_metrics: Option<FontMetricsService>,
    primary_window_size: SharedPrimaryWindowSize,
    image_catalog: Rc<AsyncImageCatalog>,
    #[cfg(feature = "video")]
    resolved_videos: Mutex<ResolvedVideoRegistry>,
    resolved_webkits: Mutex<HashMap<WebKitResolveRequest, ResolvedWebKit>>,
    resolved_surfaces: Mutex<ResolvedSurfaceMemo>,
    /// Renderer-published effective availability. Requested shader state is
    /// retained separately so hardware recovery can restore it.
    render_capabilities: Arc<SharedRenderCapabilities>,
    /// The exact shader requested by Lisp. This is one transactionally
    /// updated value so installation state, source, and live uniforms cannot
    /// drift. It survives temporary quality-policy suppression and device
    /// loss; renderer state does not.
    requested_frame_shader: Mutex<Option<RequestedFrameShader>>,
    #[cfg(feature = "neo-term")]
    terminal_state: TerminalHostState,
}

#[derive(Clone)]
struct RequestedFrameShader {
    request: FrameShaderRequestId,
    source: String,
    language: RendererShaderLanguage,
    uniforms: Vec<SurfaceUniformInit>,
}

impl RequestedFrameShader {
    fn as_render_command_payload(
        &self,
    ) -> (String, RendererShaderLanguage, Vec<SurfaceUniformInit>) {
        (self.source.clone(), self.language, self.uniforms.clone())
    }

    fn update_uniform(&mut self, name: &str, value: [f32; 4]) {
        if let Some(uniform) = self
            .uniforms
            .iter_mut()
            .find(|uniform| uniform.name == name)
        {
            uniform.value = value;
        }
    }
}

/// Per-editor neo-term ownership state. IDs and lifecycle records must not be
/// process-global: independent editor instances in one process remain isolated.
#[cfg(feature = "neo-term")]
struct TerminalHostState {
    shared: SharedTerminals,
    next_id: Cell<Option<TerminalId>>,
}

#[cfg(feature = "neo-term")]
impl TerminalHostState {
    fn new(shared: SharedTerminals) -> Self {
        Self {
            shared,
            next_id: Cell::new(TerminalId::new(HOST_TERMINAL_ID_START)),
        }
    }

    fn allocate(&self) -> Result<TerminalId, String> {
        let id = self
            .next_id
            .get()
            .ok_or_else(|| "neo-term id allocator exhausted".to_owned())?;
        self.next_id
            .set(id.get().checked_add(1).and_then(TerminalId::new));
        Ok(id)
    }

    fn require_active(&self, id: TerminalId) -> Result<(), String> {
        self.shared.require_active(id)
    }

    fn visible_text(&self, id: TerminalId) -> Result<Option<String>, String> {
        self.shared.visible_text(id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PrimaryWindowSize {
    width: u32,
    height: u32,
}

type SharedPrimaryWindowSize = Arc<Mutex<PrimaryWindowSize>>;

#[cfg(feature = "video")]
const HOST_VIDEO_ID_START: u32 = 0x5000_0000;
#[cfg(feature = "video")]
static HOST_VIDEO_ID_ALLOCATOR: AtomicU32 = AtomicU32::new(HOST_VIDEO_ID_START);
const HOST_WEBKIT_ID_START: u32 = 0x6000_0000;
static HOST_WEBKIT_ID_ALLOCATOR: AtomicU32 = AtomicU32::new(HOST_WEBKIT_ID_START);

#[cfg(feature = "video")]
fn next_host_video_id() -> VideoId {
    VideoId::new(HOST_VIDEO_ID_ALLOCATOR.fetch_add(1, Ordering::Relaxed))
}

#[cfg(feature = "video")]
fn video_open_request(request: &VideoResolveRequest) -> Result<VideoOpenRequest, String> {
    let source = match &request.source {
        VideoResolveSource::File(path) => VideoSource::File(
            path.as_utf8_str()
                .ok_or_else(|| "video file path must be UTF-8".to_owned())?
                .into(),
        ),
        VideoResolveSource::Uri(uri) => VideoSource::Uri(
            uri.as_utf8_str()
                .ok_or_else(|| "video URI must be UTF-8".to_owned())?
                .to_owned(),
        ),
    };
    Ok(VideoOpenRequest {
        source,
        loop_mode: LoopMode::from_legacy(request.loop_count).map_err(|error| error.to_string())?,
        initial_playback: if request.autoplay {
            InitialPlayback::Playing
        } else {
            InitialPlayback::Paused
        },
    })
}

fn next_host_webkit_id() -> WebViewId {
    WebViewId::new(HOST_WEBKIT_ID_ALLOCATOR.fetch_add(1, Ordering::Relaxed))
}

fn webview_create(id: WebViewId, width: u32, height: u32) -> WebViewCreate {
    WebViewCreate {
        id,
        storage: StoragePartition::Persistent(WebProfileId::new(1)),
        relationship: BrowsingRelationship::Independent,
        initial_size: WebContentSize::new(width.max(1), height.max(1))
            .expect("max(1) produces valid webview dimensions"),
        policy: WebViewPolicy::default(),
        initial_navigation: None,
    }
}

const HOST_SURFACE_ID_START: u32 = 0x7000_0000;
static HOST_SURFACE_ID_ALLOCATOR: AtomicU32 = AtomicU32::new(HOST_SURFACE_ID_START);

fn next_host_surface_id() -> u32 {
    HOST_SURFACE_ID_ALLOCATOR.fetch_add(1, Ordering::Relaxed)
}

#[cfg(feature = "neo-term")]
const HOST_TERMINAL_ID_START: u32 = 0x4000_0000;

fn renderer_shader_language(language: ShaderSurfaceLanguage) -> RendererShaderLanguage {
    match language {
        ShaderSurfaceLanguage::Wgsl => RendererShaderLanguage::Wgsl,
        ShaderSurfaceLanguage::Glsl => RendererShaderLanguage::Glsl,
    }
}

fn renderer_channel_source(
    channel: Option<(SurfaceChannelKind, u32)>,
) -> Option<RendererChannelSource> {
    channel.map(|(kind, id)| match kind {
        SurfaceChannelKind::Surface => RendererChannelSource::Surface(id),
        SurfaceChannelKind::Image => {
            RendererChannelSource::Image(neomacs_display_protocol::ImageId::new(id))
        }
        SurfaceChannelKind::Video => RendererChannelSource::Video(id),
    })
}

/// Validate + compose a user surface shader in either dialect on the Lisp
/// thread (errors become Lisp signals); returns the composed module source.
fn validate_surface_shader(
    language: ShaderSurfaceLanguage,
    source: &str,
    uniforms: &[(String, u8)],
) -> Result<String, String> {
    match language {
        ShaderSurfaceLanguage::Wgsl => validate_surface_wgsl(source, uniforms),
        ShaderSurfaceLanguage::Glsl => validate_surface_glsl(source, uniforms),
    }
}

fn read_primary_window_size(shared: &SharedPrimaryWindowSize) -> PrimaryWindowSize {
    match shared.lock() {
        Ok(state) => *state,
        Err(poisoned) => *poisoned.into_inner(),
    }
}

fn prime_initial_monitor_snapshot(shared: &SharedMonitorInfo) {
    let (lock, cvar) = &**shared;
    let monitors = match lock.lock() {
        Ok(guard) => {
            if guard.is_empty() {
                match cvar.wait_timeout(guard, Duration::from_secs(2)) {
                    Ok((guard, _)) => guard.clone(),
                    Err(poisoned) => {
                        let (guard, _) = poisoned.into_inner();
                        guard.clone()
                    }
                }
            } else {
                guard.clone()
            }
        }
        Err(poisoned) => poisoned.into_inner().clone(),
    };

    if !monitors.is_empty() {
        set_neomacs_monitor_info(input_bridge::convert_monitor_infos(&monitors));
    }
}

fn record_primary_window_resize(shared: &SharedPrimaryWindowSize, event: &DisplayInputEvent) {
    let DisplayInputEvent::Frontend(FrontendEvent::ViewportChanged(viewport)) = event else {
        return;
    };

    if viewport.target().get() != 0 || viewport.width() == 0 || viewport.height() == 0 {
        return;
    }

    match shared.lock() {
        Ok(mut state) => {
            state.width = viewport.width();
            state.height = viewport.height();
        }
        Err(poisoned) => {
            let mut state = poisoned.into_inner();
            state.width = viewport.width();
            state.height = viewport.height();
        }
    }
}

impl PrimaryWindowDisplayHost {
    fn synchronized_font_metrics(&mut self) -> &mut FontMetricsService {
        let service = self
            .font_metrics
            .get_or_insert_with(FontMetricsService::new);
        let _ = service.synchronize_font_catalog();
        service
    }

    fn frame_ref_for_gui_frame(&self, frame_id: FrameId) -> FrameRef {
        if !self.primary_window_adopted || self.primary_frame_id == Some(frame_id) {
            FrameRef::Primary
        } else {
            FrameRef::Frame(frame_id.0)
        }
    }

    fn send_render_command(
        &self,
        command: RenderCommand,
        error_context: &str,
    ) -> Result<(), String> {
        self.cmd_tx
            .send(command)
            .map_err(|err| format!("{error_context}: {err}"))?;
        if let Some(waker) = &self.render_waker {
            waker.wake();
        }
        Ok(())
    }

    fn await_clipboard_reply<T>(
        &self,
        command: ClipboardCommand,
        reply: crossbeam_channel::Receiver<Result<T, String>>,
        operation: &str,
    ) -> Result<T, String> {
        self.send_render_command(RenderCommand::Clipboard(command), operation)?;
        reply
            .recv_timeout(CLIPBOARD_REPLY_TIMEOUT)
            .map_err(|err| format!("{operation}: {err}"))?
    }
}

fn render_fullscreen_mode(fullscreen: FrameFullscreen) -> WindowFullscreenMode {
    match fullscreen {
        FrameFullscreen::Fullboth => WindowFullscreenMode::Fullboth,
        FrameFullscreen::Fullscreen => WindowFullscreenMode::Fullscreen,
        FrameFullscreen::Fullwidth => WindowFullscreenMode::Fullwidth,
        FrameFullscreen::Fullheight => WindowFullscreenMode::Fullheight,
        FrameFullscreen::Maximized => WindowFullscreenMode::Maximized,
    }
}

impl DisplayHost for PrimaryWindowDisplayHost {
    fn supports_graphical_face_attribute(&self, attribute: GraphicalFaceAttribute) -> bool {
        neomacs_display_runtime::supports_graphical_face_attribute(attribute)
    }

    fn realize_gui_frame(&mut self, request: GuiFrameHostRequest) -> Result<(), String> {
        let title_string = request.title.as_utf8_str().unwrap_or("Neomacs").to_owned();
        tracing::debug!(
            "PrimaryWindowDisplayHost::realize_gui_frame fid=0x{:x} adopted={} size={}x{} title={}",
            request.frame_id.0,
            self.primary_window_adopted,
            request.width,
            request.height,
            title_string
        );
        if !self.primary_window_adopted {
            let fullscreen_frame = FrameRef::Primary;
            self.send_render_command(
                RenderCommand::Window(WindowCommand::SetWindowTitle {
                    title: title_string.clone(),
                }),
                "failed to update primary window title",
            )?;
            self.send_render_command(
                RenderCommand::Window(WindowCommand::SetFrameGeometryHints {
                    frame: FrameRef::Primary,
                    geometry_hints: request.geometry_hints,
                }),
                "failed to update primary window geometry hints",
            )?;
            self.send_render_command(
                RenderCommand::Window(WindowCommand::AdoptPrimaryFrame {
                    frame: FrameRef::Frame(request.frame_id.0),
                }),
                "failed to adopt primary GUI frame",
            )?;
            // The opening GUI frame adopts the already-existing primary host
            // window. Do not push stale Lisp bootstrap dimensions back into
            // that window during adoption; host resize events remain the
            // source of truth until the window is fully realized.
            self.primary_window_adopted = true;
            self.primary_frame_id = Some(request.frame_id);
            if let Some(fullscreen) = request.fullscreen {
                self.send_render_command(
                    RenderCommand::Window(WindowCommand::SetWindowFullscreen {
                        frame: fullscreen_frame,
                        mode: render_fullscreen_mode(fullscreen),
                    }),
                    "failed to set primary GUI frame fullscreen mode",
                )?;
            }
        } else {
            let fullscreen_frame = FrameRef::Frame(request.frame_id.0);
            self.send_render_command(
                RenderCommand::Window(WindowCommand::CreateWindow {
                    frame: FrameRef::Frame(request.frame_id.0),
                    width: request.width,
                    height: request.height,
                    title: title_string,
                    geometry_hints: request.geometry_hints,
                }),
                "failed to create additional GUI window",
            )?;
            if let Some(fullscreen) = request.fullscreen {
                self.send_render_command(
                    RenderCommand::Window(WindowCommand::SetWindowFullscreen {
                        frame: fullscreen_frame,
                        mode: render_fullscreen_mode(fullscreen),
                    }),
                    "failed to set GUI frame fullscreen mode",
                )?;
            }
        }
        self.last_window_titles
            .lock()
            .map_err(|err| format!("failed to cache GUI frame title: {err}"))?
            .insert(request.frame_id, request.title);
        Ok(())
    }

    fn list_font_families(
        &mut self,
        _frame_id: FrameId,
    ) -> Result<Vec<AvailableFontFamilyName>, String> {
        Ok(self
            .synchronized_font_metrics()
            .list_font_families()
            .into_iter()
            .filter_map(|family| AvailableFontFamilyName::from_utf8(family.as_str()))
            .collect())
    }

    fn set_clipboard_text(&mut self, text: Option<&str>) -> Result<(), String> {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        self.await_clipboard_reply(
            ClipboardCommand::SetText {
                selection: ClipboardSelection::Clipboard,
                text: text.map(str::to_owned),
                expires_at: Instant::now() + CLIPBOARD_REPLY_TIMEOUT,
                reply: reply_tx,
            },
            reply_rx,
            "failed to set system clipboard",
        )
    }

    fn clipboard_text(&mut self) -> Result<Option<String>, String> {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        self.await_clipboard_reply(
            ClipboardCommand::GetText {
                selection: ClipboardSelection::Clipboard,
                expires_at: Instant::now() + CLIPBOARD_REPLY_TIMEOUT,
                reply: reply_tx,
            },
            reply_rx,
            "failed to read system clipboard",
        )
    }

    fn set_primary_selection_text(&mut self, text: Option<&str>) -> Result<(), String> {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        self.await_clipboard_reply(
            ClipboardCommand::SetText {
                selection: ClipboardSelection::Primary,
                text: text.map(str::to_owned),
                expires_at: Instant::now() + CLIPBOARD_REPLY_TIMEOUT,
                reply: reply_tx,
            },
            reply_rx,
            "failed to set PRIMARY selection",
        )
    }

    fn primary_selection_text(&mut self) -> Result<Option<String>, String> {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        self.await_clipboard_reply(
            ClipboardCommand::GetText {
                selection: ClipboardSelection::Primary,
                expires_at: Instant::now() + CLIPBOARD_REPLY_TIMEOUT,
                reply: reply_tx,
            },
            reply_rx,
            "failed to read PRIMARY selection",
        )
    }

    fn opening_gui_frame_pending(&self) -> bool {
        !self.primary_window_adopted
    }

    fn remove_gui_child_frame(
        &mut self,
        frame_id: neovm_core::window::FrameId,
    ) -> Result<(), String> {
        tracing::info!(
            frame_id = frame_id.0,
            "child_frame_lifecycle: host_send_remove"
        );
        self.send_render_command(
            RenderCommand::Window(WindowCommand::RemoveChildFrame {
                frame_id: frame_id.0,
            }),
            "failed to remove GUI child frame",
        )
    }

    fn show_gui_child_frame(
        &mut self,
        frame_id: neovm_core::window::FrameId,
    ) -> Result<(), String> {
        tracing::info!(
            frame_id = frame_id.0,
            "child_frame_lifecycle: host_send_show"
        );
        self.send_render_command(
            RenderCommand::Window(WindowCommand::ShowChildFrame {
                frame_id: frame_id.0,
            }),
            "failed to show GUI child frame",
        )
    }

    fn destroy_gui_frame(&mut self, frame_id: neovm_core::window::FrameId) -> Result<(), String> {
        let frame = if self.primary_frame_id == Some(frame_id) {
            self.primary_frame_id = None;
            FrameRef::Primary
        } else {
            FrameRef::Frame(frame_id.0)
        };
        self.last_window_titles
            .lock()
            .map_err(|err| format!("failed to forget GUI frame title: {err}"))?
            .remove(&frame_id);
        self.send_render_command(
            RenderCommand::Window(WindowCommand::DestroyWindow { frame }),
            "failed to destroy GUI frame window",
        )
    }

    fn show_popup_menu(&mut self, menu: PopupMenuRequest) -> Result<(), String> {
        let frame = if self.primary_frame_id == Some(menu.frame_id) {
            FrameRef::Primary
        } else {
            FrameRef::Frame(menu.frame_id.0)
        };
        let items = menu
            .entries
            .into_iter()
            .map(|entry| neomacs_display_protocol::ui_types::PopupMenuItem {
                label: entry.label,
                shortcut: entry.shortcut,
                enabled: entry.enabled,
                separator: entry.separator,
                submenu: entry.submenu,
                depth: entry.depth,
            })
            .collect();
        self.send_render_command(
            RenderCommand::Ui(UiCommand::ShowPopupMenu {
                frame,
                placement: menu.placement,
                items,
                title: menu.title,
                fg: None,
                bg: None,
            }),
            "failed to show popup menu",
        )
    }

    fn hide_popup_menu(&mut self) -> Result<(), String> {
        self.send_render_command(
            RenderCommand::Ui(UiCommand::HidePopupMenu),
            "failed to hide popup menu",
        )
    }

    fn resize_gui_frame(&mut self, request: GuiFrameHostRequest) -> Result<(), String> {
        let frame = if self.primary_frame_id == Some(request.frame_id) {
            FrameRef::Primary
        } else {
            FrameRef::Frame(request.frame_id.0)
        };
        tracing::debug!(
            "PrimaryWindowDisplayHost::resize_gui_frame fid=0x{:x} route=0x{:x} size={}x{}",
            request.frame_id.0,
            frame.raw_id(),
            request.width,
            request.height
        );
        self.send_render_command(
            RenderCommand::Window(WindowCommand::ResizeWindow {
                frame,
                width: request.width,
                height: request.height,
                geometry_hints: request.geometry_hints,
            }),
            "failed to resize GUI frame",
        )?;
        Ok(())
    }

    fn set_gui_frame_geometry_hints(
        &mut self,
        frame_id: neovm_core::window::FrameId,
        geometry_hints: neovm_core::window::GuiFrameGeometryHints,
    ) -> Result<(), String> {
        let frame = self.frame_ref_for_gui_frame(frame_id);
        self.send_render_command(
            RenderCommand::Window(WindowCommand::SetFrameGeometryHints {
                frame,
                geometry_hints,
            }),
            "failed to update GUI frame geometry hints",
        )?;
        Ok(())
    }

    fn set_gui_frame_fullscreen(
        &mut self,
        frame_id: neovm_core::window::FrameId,
        fullscreen: FrameFullscreen,
    ) -> Result<(), String> {
        let frame = self.frame_ref_for_gui_frame(frame_id);
        self.send_render_command(
            RenderCommand::Window(WindowCommand::SetWindowFullscreen {
                frame,
                mode: render_fullscreen_mode(fullscreen),
            }),
            "failed to set GUI frame fullscreen mode",
        )?;
        Ok(())
    }

    fn set_gui_frame_visibility(
        &mut self,
        frame_id: neovm_core::window::FrameId,
        visibility: FrameVisibility,
    ) -> Result<(), String> {
        let frame = self.frame_ref_for_gui_frame(frame_id);
        let visibility = match visibility {
            FrameVisibility::Visible => WindowVisibility::Visible,
            FrameVisibility::Iconified => WindowVisibility::Iconified,
            FrameVisibility::Invisible => WindowVisibility::Invisible,
        };
        self.send_render_command(
            RenderCommand::Window(WindowCommand::SetWindowVisibility { frame, visibility }),
            "failed to update GUI frame visibility",
        )
    }

    fn set_gui_frame_undecorated(
        &mut self,
        _frame_id: neovm_core::window::FrameId,
        undecorated: bool,
    ) -> Result<(), String> {
        self.send_render_command(
            RenderCommand::Window(WindowCommand::SetWindowDecorations {
                decorated: !undecorated,
            }),
            "failed to update GUI frame decorations",
        )
    }

    fn set_gui_frame_title(
        &mut self,
        frame_id: neovm_core::window::FrameId,
        title: LispString,
    ) -> Result<(), String> {
        let mut cached_titles = self
            .last_window_titles
            .lock()
            .map_err(|err| format!("failed to cache GUI frame title: {err}"))?;
        if cached_titles
            .get(&frame_id)
            .is_some_and(|cached| cached == &title)
        {
            return Ok(());
        }
        cached_titles.insert(frame_id, title.clone());
        drop(cached_titles);

        let title_string = title.as_utf8_str().unwrap_or("Neomacs").to_owned();
        let frame = if self.primary_frame_id == Some(frame_id) {
            FrameRef::Primary
        } else {
            FrameRef::Frame(frame_id.0)
        };
        self.send_render_command(
            RenderCommand::Window(WindowCommand::SetFrameWindowTitle {
                frame,
                title: title_string,
            }),
            "failed to update GUI frame title",
        )?;
        Ok(())
    }

    fn current_primary_window_size(&self) -> Option<GuiFrameHostSize> {
        if self.primary_window_adopted {
            return None;
        }
        let state = read_primary_window_size(&self.primary_window_size);
        Some(GuiFrameHostSize {
            width: state.width,
            height: state.height,
        })
    }

    fn set_visual_config(&mut self, config: VisualConfig) -> Result<(), String> {
        self.send_render_command(
            RenderCommand::Config(ConfigCommand::SetVisualConfig(config)),
            "failed to set visual configuration",
        )
    }

    fn resolve_font_for_char(
        &mut self,
        request: FontResolveRequest,
    ) -> Result<Option<ResolvedFontMatch>, String> {
        // cosmic-text/fontdb consume Unicode scalar values. Keep the full
        // Emacs character in the protocol and reject unsupported raw-byte or
        // non-Unicode codes only at this explicit backend boundary.
        let Some(character) = request.character.as_rust_char() else {
            return Ok(None);
        };
        let requested_family_storage = request.faces.ascii_face.family_runtime_string_owned();
        let requested_family = requested_family_storage.as_deref().unwrap_or("Monospace");
        let fontset_base_family_storage = request
            .faces
            .fontset_base_face
            .family_runtime_string_owned();
        let fontset_base_family = fontset_base_family_storage
            .as_deref()
            .unwrap_or("Monospace");
        let requested_weight = request
            .faces
            .ascii_face
            .weight
            .unwrap_or(FontWeight::NORMAL)
            .css_weight();
        let requested_italic = request
            .faces
            .ascii_face
            .slant
            .map(|slant| slant.is_italic())
            .unwrap_or(false);
        let font_size = self
            .font_sizing
            .font_size_px_for_face(&request.faces.ascii_face);
        let selected = self
            .synchronized_font_metrics()
            .select_font_for_realized_face_char(
                character,
                neomacs_layout_engine::font::metrics::RealizedFaceFontSelection::new(
                    neomacs_layout_engine::font::metrics::PrimaryFontFamily::new(requested_family),
                    neomacs_layout_engine::font::metrics::FontsetBaseFamily::new(
                        fontset_base_family,
                    ),
                    requested_weight,
                    requested_italic,
                    font_size,
                ),
            );
        tracing::debug!(
            target: "neomacs::font_at",
            character = request.character.code(),
            requested_family,
            requested_weight,
            requested_italic,
            font_size,
            request_faces = ?request.faces,
            selected = ?selected,
            "display host resolved font-at request"
        );
        Ok(selected.map(|font| {
            let glyph_code = font.glyph_code;
            ResolvedFontMatch {
                glyph_code,
                font: core_opened_font_from_selection(font, |file, face_index| {
                    self.font_otf_capability(file, face_index).ok().flatten()
                }),
            }
        }))
    }

    fn resolve_frame_font(
        &mut self,
        frame_id: FrameId,
        request: FrameFontRequest,
    ) -> Result<Option<ResolvedFrameFont>, String> {
        // Every frame in this host shares one frontend/display connection.
        // Its logical point policy is therefore shared just like GNU's
        // display-level FRAME_RES; backing/device scale remains frame-local
        // and is applied later by the renderer.
        let font_sizing = self.font_sizing;
        let face = request.face();
        let requested_family_storage = face.family_runtime_string_owned();
        let requested_family = requested_family_storage.as_deref().unwrap_or("Monospace");
        let requested_weight = face.weight.unwrap_or(FontWeight::NORMAL).css_weight();
        let requested_italic = face.slant.map(|slant| slant.is_italic()).unwrap_or(false);
        let Some(font_size) = font_sizing.font_size_px_for_request(request.size()) else {
            return Ok(None);
        };
        let selected = self.synchronized_font_metrics().select_font_for_char(
            'M',
            requested_family,
            requested_weight,
            requested_italic,
            font_size.get(),
        );
        let Some(font) = selected else {
            return Ok(None);
        };
        let height_tenths =
            font_sizing.face_height_tenths_for_layout_pixels(font.metrics.pixel_size.max(1));
        tracing::debug!(
            frame_id = frame_id.0,
            requested_size = ?request.size(),
            realized_pixel_size = font.metrics.pixel_size,
            height_tenths,
            "resolved frame-local font geometry"
        );
        Ok(Some(ResolvedFrameFont {
            height_tenths,
            font: core_opened_font_from_selection(font, font_otf_capability_for_file),
        }))
    }

    fn resolve_font_for_spec(
        &mut self,
        request: FontSpecResolveRequest,
    ) -> Result<Option<ResolvedFontSpecMatch>, String> {
        let family = request
            .family
            .as_ref()
            .and_then(LispString::as_utf8_str)
            .and_then(neomacs_layout_engine::font_backend::FontFamilyName::new);
        let mut query = neomacs_layout_engine::font::resolver::FontEntityQuery::new(family);
        if let Some(registry) = request.registry.as_ref().and_then(LispString::as_utf8_str) {
            query = query.with_registry(registry);
        }
        if let Some(language) = request.lang.as_ref().and_then(LispString::as_utf8_str) {
            query = query.with_language(language);
        }
        if let Some(weight) = request.weight {
            query = query.with_weight(weight.css_weight());
        }
        if let Some(slant) = request.slant {
            query = query.with_slant(slant);
        }
        if let Some(width) = request.width {
            query = query.with_width(width);
        }
        let entity = self.synchronized_font_metrics().resolve_font_entity(&query);
        Ok(entity.map(|entity| ResolvedFontSpecMatch {
            family: LispString::from_utf8(entity.matched.family()),
            foundry: entity
                .matched
                .metadata
                .foundry
                .as_ref()
                .map(|foundry| LispString::from_utf8(foundry)),
            registry: entity
                .registry
                .as_ref()
                .map(|registry| LispString::from_utf8(registry)),
            file: entity
                .matched
                .identity
                .file_path
                .as_ref()
                .map(|file| LispString::from_utf8(file)),
            weight: entity.matched.weight().map(FontWeight::from_css_weight),
            slant: Some(entity.matched.slant()),
            width: entity.matched.metadata.width,
            spacing: entity.matched.metadata.spacing,
            postscript_name: entity
                .matched
                .identity
                .postscript_name
                .as_ref()
                .map(|name| LispString::from_utf8(name)),
        }))
    }

    fn probe_font_px_metrics(
        &mut self,
        file: &str,
        face_index: u32,
        pixel_size: u32,
        wght: Option<f32>,
    ) -> Result<Option<neovm_core::emacs_core::eval::FontPxProbeResult>, String> {
        Ok(neomacs_layout_engine::font::probe::probe_font_px_metrics(
            file, face_index, pixel_size, wght,
        )
        .map(core_font_px_metrics))
    }

    fn probe_font_entity_metrics(
        &mut self,
        request: FontEntityMetricsRequest,
    ) -> Result<Option<ResolvedFontEntityMetrics>, String> {
        let pixel_size = request.pixel_size.max(1);
        let family = request
            .family
            .as_ref()
            .and_then(LispString::as_utf8_str)
            .and_then(neomacs_layout_engine::font_backend::FontFamilyName::new);
        let mut query = neomacs_layout_engine::font::resolver::FontEntityQuery::new(family);
        if let Some(registry) = request.registry.as_ref().and_then(LispString::as_utf8_str) {
            query = query.with_registry(registry);
        }
        if let Some(postscript_name) = request
            .postscript_name
            .as_ref()
            .and_then(LispString::as_utf8_str)
        {
            query = query.with_postscript_name(postscript_name);
        }
        if let Some(weight) = request.weight {
            query = query.with_weight(weight.css_weight());
        }
        if let Some(slant) = request.slant {
            query = query.with_slant(slant);
        }
        if let Some(width) = request.width {
            query = query.with_width(width);
        }

        if let Some(opened) = self
            .synchronized_font_metrics()
            .open_font_entity(&query, pixel_size)
        {
            let file = opened
                .entity
                .matched
                .file_path()
                .map(|file| LispString::from_utf8(file));
            let capability = font_otf_capability_for_asset(&opened.entity.matched.asset);
            return Ok(Some(ResolvedFontEntityMetrics {
                metrics: core_font_px_metrics(opened.metrics),
                file,
                capability,
            }));
        }

        // Compatibility fallback for callers that only have a standalone
        // font file. Native entities must take the path above so a collection
        // face or named variation is not silently reopened as face zero.
        let Some(file) = request.file.as_ref().and_then(LispString::as_utf8_str) else {
            return Ok(None);
        };
        let Some(metrics) = neomacs_layout_engine::font::probe::probe_font_px_metrics(
            file,
            0,
            pixel_size,
            request.weight.map(|weight| f32::from(weight.css_weight())),
        ) else {
            return Ok(None);
        };
        let capability = font_otf_capability_for_file(file, 0);
        Ok(Some(ResolvedFontEntityMetrics {
            metrics: core_font_px_metrics(metrics),
            file: request.file,
            capability,
        }))
    }

    fn font_otf_capability(
        &mut self,
        file: &str,
        face_index: u32,
    ) -> Result<Option<neovm_core::emacs_core::eval::FontOtfCapability>, String> {
        Ok(font_otf_capability_for_file(file, face_index))
    }

    fn resolve_image_sync(
        &self,
        request: ImageResolveRequest,
    ) -> Result<Option<ReadyImage>, String> {
        self.image_catalog.resolve_sync(request)
    }

    fn image_catalog(&self) -> Option<&dyn ImageCatalog> {
        Some(&*self.image_catalog)
    }

    fn image_catalog_shared(&self) -> Option<Rc<dyn ImageCatalog>> {
        Some(self.image_catalog.clone())
    }

    fn reconcile_image_catalog_for_media_rebuild(
        &self,
        event: neovm_core::emacs_core::image_catalog::ImageStateEvent,
    ) {
        self.image_catalog.reconcile_renderer_state(event);
    }

    fn request_video(&self, request: VideoResolveRequest) -> Result<Option<ResolvedVideo>, String> {
        cfg_select! {
            feature = "video" => {
                {
                    let cache = match self.resolved_videos.lock() {
                        Ok(cache) => cache,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    if let Some(video) = cache.get(&request) {
                        return Ok(Some(video));
                    }
                }

                let video_id = next_host_video_id();
                let open = video_open_request(&request)?;
                self.send_render_command(
                    RenderCommand::Asset(AssetCommand::Video(VideoSessionCommand::Open {
                        id: video_id,
                        request: open,
                    })),
                    "failed to queue video create",
                )?;

                let resolved = ResolvedVideo { video_id };
                match self.resolved_videos.lock() {
                    Ok(mut cache) => cache.insert(request, resolved.clone()),
                    Err(poisoned) => poisoned.into_inner().insert(request, resolved.clone()),
                }
                Ok(Some(resolved))
            }
            _ => {
                let _ = request;
                Ok(None)
            }
        }
    }

    fn create_video(&self, request: VideoOpenRequest) -> Result<VideoId, String> {
        cfg_select! {
            feature = "video" => {
                let id = next_host_video_id();
                self.send_render_command(
                    RenderCommand::Asset(AssetCommand::Video(VideoSessionCommand::Open {
                        id,
                        request,
                    })),
                    "failed to queue video open",
                )?;
                Ok(id)
            }
            _ => {
                let _ = request;
                Err("native video support is not compiled into this Neomacs build".to_owned())
            }
        }
    }

    fn control_video(&self, id: VideoId, action: PlaybackAction) -> Result<(), String> {
        cfg_select! {
            feature = "video" => {
                self.send_render_command(
                    RenderCommand::Asset(AssetCommand::Video(VideoSessionCommand::Control {
                        id,
                        action,
                    })),
                    "failed to queue video control",
                )
            }
            _ => {
                let _ = (id, action);
                Err("native video support is not compiled into this Neomacs build".to_owned())
            }
        }
    }

    fn destroy_video(&self, id: VideoId) -> Result<(), String> {
        cfg_select! {
            feature = "video" => {
                self.send_render_command(
                    RenderCommand::Asset(AssetCommand::Video(VideoSessionCommand::Close { id })),
                    "failed to queue video close",
                )
            }
            _ => {
                let _ = id;
                Ok(())
            }
        }
    }

    fn video_diagnostics(&self) -> Result<neomacs_video_model::VideoDiagnostics, String> {
        cfg_select! {
            feature = "video" => {
                let (reply, received) = crossbeam_channel::bounded(1);
                self.send_render_command(
                    RenderCommand::Asset(AssetCommand::Video(
                        VideoSessionCommand::Diagnostics { reply },
                    )),
                    "failed to request video diagnostics",
                )?;
                received
                    .recv_timeout(VIDEO_DIAGNOSTICS_REPLY_TIMEOUT)
                    .map_err(|error| format!("video diagnostics reply failed: {error}"))?
            }
            _ => {
                Err("native video support is not compiled into this Neomacs build".to_owned())
            }
        }
    }

    fn begin_video_measurement_epoch(
        &self,
    ) -> Result<neomacs_video_model::VideoDiagnostics, String> {
        cfg_select! {
            feature = "video" => {
                let (reply, received) = crossbeam_channel::bounded(1);
                self.send_render_command(
                    RenderCommand::Asset(AssetCommand::Video(
                        VideoSessionCommand::BeginMeasurementEpoch { reply },
                    )),
                    "failed to begin video measurement epoch",
                )?;
                received
                    .recv_timeout(VIDEO_DIAGNOSTICS_REPLY_TIMEOUT)
                    .map_err(|error| format!("video measurement epoch reply failed: {error}"))?
            }
            _ => {
                Err("native video support is not compiled into this Neomacs build".to_owned())
            }
        }
    }

    fn request_webkit(
        &self,
        request: WebKitResolveRequest,
    ) -> Result<Option<ResolvedWebKit>, String> {
        {
            let cache = match self.resolved_webkits.lock() {
                Ok(cache) => cache,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let Some(webkit) = cache.get(&request).cloned() {
                return Ok(Some(webkit));
            }
        }

        let webview_id = next_host_webkit_id();

        let navigation = match &request.source {
            WebKitResolveSource::Uri(uri) => NavigationTarget::Uri(
                uri.as_utf8_str()
                    .ok_or_else(|| "WebView URI is not valid UTF-8".to_owned())?
                    .to_owned(),
            ),
            WebKitResolveSource::File(path) => NavigationTarget::File(std::path::PathBuf::from(
                path.as_utf8_str()
                    .ok_or_else(|| "WebView file path is not valid UTF-8".to_owned())?,
            )),
        };
        let mut create = webview_create(webview_id, request.width, request.height);
        create.initial_navigation = Some(navigation);
        self.send_render_command(
            RenderCommand::Asset(AssetCommand::WebView(WebViewCommand::Create(create))),
            "failed to queue WebView create",
        )?;

        let resolved = ResolvedWebKit { webview_id };
        match self.resolved_webkits.lock() {
            Ok(mut cache) => {
                cache.insert(request, resolved.clone());
            }
            Err(poisoned) => {
                let mut cache = poisoned.into_inner();
                cache.insert(request, resolved.clone());
            }
        }
        Ok(Some(resolved))
    }

    fn request_surface(
        &self,
        request: SurfaceResolveRequest,
    ) -> Result<Option<ResolvedSurface>, String> {
        {
            let cache = match self.resolved_surfaces.lock() {
                Ok(cache) => cache,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let Some(resolved) = cache.get(&request) {
                return Ok(resolved);
            }
        }

        let uniforms: Vec<SurfaceUniformInit> = request
            .uniforms
            .iter()
            .map(|(name, bits, components)| SurfaceUniformInit {
                name: name.clone(),
                value: [
                    f32::from_bits(bits[0]),
                    f32::from_bits(bits[1]),
                    f32::from_bits(bits[2]),
                    f32::from_bits(bits[3]),
                ],
                components: *components,
            })
            .collect();
        // Redisplay cannot signal: a WGSL failure logs once and memoizes as
        // unresolved (the spec renders nothing, like a broken image).
        let names: Vec<(String, u8)> = uniforms
            .iter()
            .map(|u| (u.name.clone(), u.components))
            .collect();
        let resolved = match validate_surface_shader(request.language, &request.source, &names) {
            Ok(_) => {
                let surface_id = next_host_surface_id();
                self.send_render_command(
                    RenderCommand::Asset(AssetCommand::SurfaceCreate {
                        id: surface_id,
                        source: SurfaceSource::Shader {
                            language: renderer_shader_language(request.language),
                            source: request.source.clone(),
                            uniforms,
                            channel0: renderer_channel_source(request.channel0),
                        },
                        width: request.width,
                        height: request.height,
                        animate: request.animate,
                        fps: request.fps,
                        // Safe to evict under media-budget pressure: the
                        // declarative resolver re-runs on every redisplay
                        // walk of a visible spec, so an evicted-but-visible
                        // surface is recreated on the next walk (same
                        // argument as the memo's own FIFO eviction below) —
                        // the user sees at most a one-frame re-resolve, never
                        // a permanently blank quad.
                        recreatable: true,
                    }),
                    "failed to queue declarative surface create",
                )?;
                Some(ResolvedSurface { surface_id })
            }
            Err(err) => {
                tracing::warn!("declarative surface spec rejected: {err}");
                None
            }
        };

        let evicted_surface_id = match self.resolved_surfaces.lock() {
            Ok(mut cache) => cache.insert(request, resolved.clone()),
            Err(poisoned) => poisoned.into_inner().insert(request, resolved.clone()),
        };
        if let Some(id) = evicted_surface_id {
            // The memo hit its FIFO cap: free the evicted entry's GPU
            // objects. If that spec is still displayed somewhere, the next
            // redisplay walk re-resolves (and re-creates) it, so eviction is
            // never observable — only a one-off re-resolve cost.
            self.send_render_command(
                RenderCommand::Asset(AssetCommand::SurfaceFree { id }),
                "failed to queue evicted declarative surface free",
            )?;
        }
        Ok(resolved)
    }

    fn create_webkit_xwidget(&self, id: WebViewId, width: u32, height: u32) -> Result<(), String> {
        self.send_render_command(
            RenderCommand::Asset(AssetCommand::WebView(WebViewCommand::Create(
                webview_create(id, width, height),
            ))),
            "failed to queue WebView xwidget create",
        )
    }

    fn load_webkit_xwidget_uri(&self, id: WebViewId, uri: LispString) -> Result<(), String> {
        let url = String::from_utf8_lossy(uri.as_bytes()).into_owned();
        self.send_render_command(
            RenderCommand::Asset(AssetCommand::WebView(WebViewCommand::Navigate {
                id,
                target: NavigationTarget::Uri(url),
            })),
            "failed to queue WebView xwidget navigation",
        )
    }

    fn execute_webkit_xwidget_script(
        &self,
        id: WebViewId,
        request: XwidgetScriptRequestId,
        script: LispString,
    ) -> Result<(), String> {
        let script = String::from_utf8_lossy(script.as_bytes()).into_owned();
        let request = ScriptRequestId::new(request.get());
        self.send_render_command(
            RenderCommand::Asset(AssetCommand::WebView(WebViewCommand::EvaluateScript(
                ScriptRequest {
                    request,
                    view: id,
                    source: script,
                    world: ScriptWorld::Page,
                },
            ))),
            "failed to queue WebView xwidget script",
        )
    }

    fn resize_webkit_xwidget(&self, id: WebViewId, width: u32, height: u32) -> Result<(), String> {
        self.send_render_command(
            RenderCommand::Asset(AssetCommand::WebView(WebViewCommand::SetModelSize {
                id,
                size: WebContentSize::new(width.max(1), height.max(1))
                    .expect("max(1) produces valid webview dimensions"),
            })),
            "failed to queue WebView xwidget resize",
        )
    }

    fn destroy_webkit_xwidget(&self, id: WebViewId) -> Result<(), String> {
        self.send_render_command(
            RenderCommand::Asset(AssetCommand::WebView(WebViewCommand::Close { id })),
            "failed to queue WebView xwidget close",
        )
    }

    fn create_shader_surface(&self, request: ShaderSurfaceCreateRequest) -> Result<u32, String> {
        let source = match request.content {
            ShaderSurfaceContent::Shader {
                language,
                source,
                uniforms,
                channel0,
            } => {
                let uniforms: Vec<SurfaceUniformInit> = uniforms
                    .into_iter()
                    .map(|u| SurfaceUniformInit {
                        name: u.name,
                        value: u.value,
                        components: u.components,
                    })
                    .collect();
                // Validate synchronously on the Lisp thread so compile errors
                // become Lisp errors with naga diagnostics.
                let names: Vec<(String, u8)> = uniforms
                    .iter()
                    .map(|u| (u.name.clone(), u.components))
                    .collect();
                validate_surface_shader(language, &source, &names)?;
                SurfaceSource::Shader {
                    language: renderer_shader_language(language),
                    source,
                    uniforms,
                    channel0: renderer_channel_source(channel0),
                }
            }
            ShaderSurfaceContent::Pixels { data } => SurfaceSource::Pixels { data },
        };
        let surface_id = next_host_surface_id();
        self.send_render_command(
            RenderCommand::Asset(AssetCommand::SurfaceCreate {
                id: surface_id,
                source,
                width: request.width,
                height: request.height,
                animate: request.animate,
                fps: request.fps,
                // Never evict imperative surfaces: Lisp holds the bare id
                // (`neomacs-surface-create`) and nothing would ever recreate
                // the texture — eviction would blank it permanently.
                recreatable: false,
            }),
            "failed to queue surface create",
        )?;
        Ok(surface_id)
    }

    fn set_shader_surface_uniform(
        &self,
        id: u32,
        name: &str,
        value: [f32; 4],
    ) -> Result<(), String> {
        self.send_render_command(
            RenderCommand::Asset(AssetCommand::SurfaceSetUniform {
                id,
                name: name.to_owned(),
                value,
            }),
            "failed to queue surface uniform update",
        )
    }

    fn destroy_shader_surface(&self, id: u32) -> Result<(), String> {
        self.send_render_command(
            RenderCommand::Asset(AssetCommand::SurfaceFree { id }),
            "failed to queue surface destroy",
        )
    }

    #[cfg(feature = "neo-term")]
    fn create_terminal(&self, request: TerminalCreateRequest) -> Result<TerminalId, String> {
        let id = self.terminal_state.allocate()?;
        let reservation = self.terminal_state.shared.reserve(id)?;
        self.send_render_command(
            RenderCommand::Terminal(TerminalCommand::TerminalCreate {
                id,
                size: request.size,
                target: request.target,
                shell: request.shell,
            }),
            "failed to queue terminal create",
        )?;
        reservation.commit();
        Ok(id)
    }

    #[cfg(feature = "neo-term")]
    fn write_terminal(&self, id: TerminalId, data: Vec<u8>) -> Result<(), String> {
        self.terminal_state.require_active(id)?;
        self.send_render_command(
            RenderCommand::Terminal(TerminalCommand::TerminalWrite { id, data }),
            "failed to queue terminal input",
        )
    }

    #[cfg(feature = "neo-term")]
    fn resize_terminal(&self, id: TerminalId, size: TerminalGridSize) -> Result<(), String> {
        self.terminal_state.require_active(id)?;
        self.send_render_command(
            RenderCommand::Terminal(TerminalCommand::TerminalResize { id, size }),
            "failed to queue terminal resize",
        )
    }

    #[cfg(feature = "neo-term")]
    fn destroy_terminal(&self, id: TerminalId) -> Result<(), String> {
        let transition = self.terminal_state.shared.begin_destroy(id)?;
        self.send_render_command(
            RenderCommand::Terminal(TerminalCommand::TerminalDestroy { id }),
            "failed to queue terminal destroy",
        )?;
        transition.commit();
        Ok(())
    }

    #[cfg(feature = "neo-term")]
    fn set_floating_terminal(
        &self,
        id: TerminalId,
        placement: TerminalFloatPlacement,
    ) -> Result<(), String> {
        self.terminal_state.require_active(id)?;
        self.send_render_command(
            RenderCommand::Terminal(TerminalCommand::TerminalSetFloat { id, placement }),
            "failed to queue terminal placement",
        )
    }

    #[cfg(feature = "neo-term")]
    fn terminal_text(&self, id: TerminalId) -> Result<Option<String>, String> {
        self.terminal_state.visible_text(id)
    }

    fn set_frame_shader(
        &self,
        source: Option<(String, ShaderSurfaceLanguage, Vec<ShaderSurfaceUniformInit>)>,
    ) -> Result<(), String> {
        let validated = match source {
            // Validate + compose on the Lisp thread so compile errors signal
            // synchronously; the renderer receives the finished module with
            // the uniform accessors already composed in — `FramePost` only
            // records the name -> slot table and initial values.
            Some((source, language, uniforms)) => {
                let uniforms: Vec<SurfaceUniformInit> = uniforms
                    .into_iter()
                    .map(|u| SurfaceUniformInit {
                        name: u.name,
                        value: u.value,
                        components: u.components,
                    })
                    .collect();
                let names: Vec<(String, u8)> = uniforms
                    .iter()
                    .map(|u| (u.name.clone(), u.components))
                    .collect();
                Some((
                    validate_surface_shader(language, &source, &names)?,
                    renderer_shader_language(language),
                    uniforms,
                ))
            }
            None => None,
        };
        if validated.is_some()
            && self.render_capabilities.frame_shader_availability()
                == FrameShaderAvailability::SuppressedByQualityPolicy
        {
            return Err(
                "frame shaders are disabled by the active render-quality policy".to_owned(),
            );
        }
        let prepared = self
            .render_capabilities
            .prepare_frame_shader_request(validated.is_some());
        let request = prepared.id();
        let requested = validated.map(|(source, language, uniforms)| RequestedFrameShader {
            request,
            source,
            language,
            uniforms,
        });
        let composed = requested
            .as_ref()
            .map(RequestedFrameShader::as_render_command_payload);
        self.send_render_command(
            RenderCommand::Asset(AssetCommand::FrameShaderSet { request, composed }),
            "failed to queue frame shader update",
        )?;
        prepared.commit();
        // Publish requested state only after the command has been accepted.
        // A full channel must not make Lisp believe an unqueued shader exists.
        match self.requested_frame_shader.lock() {
            Ok(mut current) => *current = requested,
            Err(poisoned) => *poisoned.into_inner() = requested,
        }
        Ok(())
    }

    fn set_frame_shader_uniform(&self, name: &str, value: [f32; 4]) -> Result<(), String> {
        let request = {
            let requested = match self.requested_frame_shader.lock() {
                Ok(requested) => requested,
                Err(poisoned) => poisoned.into_inner(),
            };
            requested
                .as_ref()
                .map(|requested| requested.request)
                .ok_or_else(|| "no frame shader installed".to_owned())?
        };
        match self.render_capabilities.frame_shader_execution(request) {
            FrameShaderExecution::Rejected | FrameShaderExecution::Absent => {
                return Err("no frame shader installed".to_owned());
            }
            FrameShaderExecution::SuppressedByQualityPolicy => {
                return Err(
                    "frame shaders are disabled by the active render-quality policy".to_owned(),
                );
            }
            FrameShaderExecution::Pending | FrameShaderExecution::Installed => {}
        }
        if self.render_capabilities.frame_shader_availability()
            == FrameShaderAvailability::SuppressedByQualityPolicy
        {
            return Err(
                "frame shaders are disabled by the active render-quality policy".to_owned(),
            );
        }
        self.send_render_command(
            RenderCommand::Asset(AssetCommand::FrameShaderSetUniform {
                request,
                name: name.to_owned(),
                value,
            }),
            "failed to queue frame shader uniform update",
        )?;
        // Keep the declarative request exact so device recovery replays the
        // latest live uniform values, not the original installation values.
        let mut requested = match self.requested_frame_shader.lock() {
            Ok(requested) => requested,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(requested) = requested.as_mut() {
            requested.update_uniform(name, value);
        }
        Ok(())
    }

    /// The render thread lost its wgpu device and rebuilt the GPU stack
    /// (`DisplayInputEvent::DisplayReset`). Every renderer-side media object
    /// is gone; re-resolve what this host owns.
    fn display_reset(&self) {
        tracing::warn!("display reset: re-resolving GPU-resident media after device loss");

        // Video sessions are restored by the authoritative native video
        // system with their original IDs. Keep evaluator memoization so the
        // retained presentation continues to reference those exact sessions;
        // clearing it here would create duplicate, orphaned decoders.
        match self.resolved_surfaces.lock() {
            Ok(mut memo) => *memo = ResolvedSurfaceMemo::default(),
            Err(poisoned) => *poisoned.into_inner() = ResolvedSurfaceMemo::default(),
        }

        // WebKit WPE views live outside the renderer (only their textures
        // died) and would survive; destroy the old views so re-creation on
        // the next walk does not leak living views.
        let old_webkits: Vec<WebViewId> = {
            let mut cache = match self.resolved_webkits.lock() {
                Ok(cache) => cache,
                Err(poisoned) => poisoned.into_inner(),
            };
            cache.drain().map(|(_, webkit)| webkit.webview_id).collect()
        };
        for id in old_webkits {
            if let Err(error) = self.send_render_command(
                RenderCommand::Asset(AssetCommand::WebView(WebViewCommand::Close { id })),
                "failed to queue stale WebKit destroy after display reset",
            ) {
                tracing::warn!(?id, %error, "display reset");
            }
        }

        // The frame shader is Lisp-visible state, not a cache: re-install
        // the exact composed module the renderer had.
        let requested_frame_shader = match self.requested_frame_shader.lock() {
            Ok(requested) => requested.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        let composed = requested_frame_shader
            .as_ref()
            .map(RequestedFrameShader::as_render_command_payload);
        if composed.is_some()
            && self.render_capabilities.frame_shader_availability()
                != FrameShaderAvailability::SuppressedByQualityPolicy
            && let Err(error) = self.send_render_command(
                RenderCommand::Asset(AssetCommand::FrameShaderSet {
                    request: requested_frame_shader
                        .as_ref()
                        .expect("composed shader has a request")
                        .request,
                    composed,
                }),
                "failed to re-send frame shader after display reset",
            )
        {
            tracing::warn!(%error, "display reset");
        }

        // Re-upload every known image under its existing id: published
        // frames keep referencing those ids, so the renderer's kept CPU
        // frame re-textures as soon as the decodes land.
        self.image_catalog.invalidate_all();
    }

    fn debug_lose_device(&self) {
        if let Err(error) = self.send_render_command(
            RenderCommand::Asset(AssetCommand::DebugSimulateDeviceLoss),
            "failed to queue simulated device loss",
        ) {
            tracing::warn!(%error, "debug_lose_device");
        }
    }
}

fn frame_host_title(eval: &mut Context, frame_id: FrameId) -> LispString {
    let Some((selected_window_id, buffer_id, fallback_title, target_cols)) =
        eval.frame_manager().get(frame_id).map(|frame| {
            let fallback_title = frame.host_title_lisp_string();
            let buffer_id = match frame.selected_window() {
                Some(Window::Leaf { buffer_id, .. }) => Some(*buffer_id),
                _ => None,
            };
            let target_cols = if frame.char_width > 0.0 {
                ((frame.width as f32) / frame.char_width.max(1.0))
                    .floor()
                    .max(1.0) as usize
            } else {
                frame.width.max(1) as usize
            };
            (
                frame.selected_window,
                buffer_id,
                fallback_title,
                target_cols.max(1),
            )
        })
    else {
        return LispString::from_utf8("Neomacs");
    };

    let format = eval
        .obarray()
        .symbol_value("frame-title-format")
        .copied()
        .unwrap_or(Value::NIL);
    if format.is_nil() {
        return fallback_title;
    }

    let rendered = neovm_core::emacs_core::xdisp::format_mode_line_for_display(
        eval,
        format,
        Value::make_window(selected_window_id.0),
        buffer_id.map(Value::make_buffer).unwrap_or(Value::NIL),
        target_cols,
    );
    rendered.as_lisp_string().cloned().unwrap_or(fallback_title)
}

fn adopt_existing_primary_gui_frame(eval: &mut Context) -> Result<(), String> {
    if eval
        .display_host
        .as_ref()
        .is_none_or(|host| !host.opening_gui_frame_pending())
    {
        return Ok(());
    }
    let Some((frame_id, width, height)) = eval
        .frame_manager()
        .selected_frame()
        .map(|frame| (frame.id, frame.width, frame.height))
    else {
        return Ok(());
    };
    let fullscreen = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.known_parameter(FrameParam::Fullscreen))
        .and_then(|value| FrameFullscreen::from_symbol_value(&value));
    let title = frame_host_title(eval, frame_id);
    let geometry_hints = eval
        .frame_manager()
        .get(frame_id)
        .map(|frame| frame.gui_geometry_hints())
        .ok_or_else(|| "selected GUI frame disappeared before adoption".to_string())?;
    let Some(host) = eval.display_host.as_mut() else {
        return Ok(());
    };
    host.realize_gui_frame(GuiFrameHostRequest {
        frame_id,
        width,
        height,
        title,
        geometry_hints,
        fullscreen,
    })
}

fn sync_live_gui_frame_titles(eval: &mut Context) {
    let frame_ids = eval.frame_manager().frame_list();
    for frame_id in frame_ids {
        let is_gui_frame = eval.frame_manager().get(frame_id).is_some_and(|frame| {
            frame.effective_window_system().is_some() && frame.parent_frame.as_frame_id().is_none()
        });
        if !is_gui_frame {
            continue;
        }
        let title = frame_host_title(eval, frame_id);
        if let Some(host) = eval.display_host.as_mut() {
            let _ = host.set_gui_frame_title(frame_id, title);
        }
    }
}

fn seed_gnu_default_gui_chrome_modes(eval: &mut Context) {
    eval.set_variable("menu-bar-mode", Value::T);
    eval.set_variable("tool-bar-mode", Value::T);
    eval.set_variable("compact-bar-mode", Value::NIL);
}

fn run_reused_gui_startup_frame_lisp(eval: &mut Context, frame_id: FrameId, body: &str) {
    let frame_value = Value::make_frame(frame_id.0);
    let previous = eval
        .obarray()
        .symbol_value("neomacs--reused-gui-startup-frame")
        .copied();
    eval.set_variable("neomacs--reused-gui-startup-frame", frame_value);
    let result = eval.eval_str(body);
    match previous {
        Some(value) => eval.set_variable("neomacs--reused-gui-startup-frame", value),
        None => {
            let _ = eval.eval_str("(makunbound 'neomacs--reused-gui-startup-frame)");
        }
    }
    result.expect("GNU GUI startup frame Lisp initialization should succeed");
}

fn initialize_reused_gui_startup_frame(eval: &mut Context, frame_id: FrameId) {
    seed_gnu_default_gui_chrome_modes(eval);

    // GNU startup calls `window-system-initialization`, then
    // `frame-initialize`; the opening GUI frame is created through
    // `faces.el:x-create-frame-with-faces`.  Neomacs creates the host
    // GUI frame before Lisp startup and reuses it as `frame-initial-frame`,
    // so run the frame-local Lisp side effects that GNU's creation path
    // applies after `x-create-frame` returns.
    run_reused_gui_startup_frame_lisp(
        eval,
        frame_id,
        r#"
        (when (frame-live-p neomacs--reused-gui-startup-frame)
          ;; GNU faces.el:x-create-frame-with-faces calls this before face
          ;; recalculation.  It installs x-alternatives-map on the frame's
          ;; terminal, including [M-backspace] -> M-DEL.
          (when (fboundp 'x-setup-function-keys)
            (x-setup-function-keys neomacs--reused-gui-startup-frame))
          ;; The reused startup frame bypasses GNU's normal
          ;; faces.el:x-create-frame-with-faces creation path.  Run the
          ;; frame-local face finalization from that path so frame parameters
          ;; such as foreground-color/background-color realize into the
          ;; default face cache before redisplay.
          (when (fboundp 'frame-set-background-mode)
            (frame-set-background-mode neomacs--reused-gui-startup-frame t))
          (when (fboundp 'face-set-after-frame-default)
            (face-set-after-frame-default
             neomacs--reused-gui-startup-frame
             (frame-parameters neomacs--reused-gui-startup-frame))))
        "#,
    );
    // The GNU face-finalization loop above mutates authoritative frame-local
    // Lisp specifications.  Bootstrap chrome is the first consumer of their
    // derived runtime form, so materialize once at this explicit read seam.
    eval.sync_runtime_faces_for_frame(frame_id);
    sync_selected_gui_chrome_state(eval);
}

/// GNU `tool-bar-mode` (tool-bar.el) runs `tool-bar-setup` once, when the
/// mode is enabled on a graphic display and the default `tool-bar-map` is
/// still the empty `(keymap)` it was created with. Neomacs creates the host
/// GUI frame before Lisp startup, so that one-time check is re-asked at the
/// chrome sync instead. It runs on every published frame, so it is three
/// direct reads rather than a parsed and evaluated probe form.
fn ensure_gnu_tool_bar_setup(eval: &mut Context) {
    let needs_setup = eval.obarray().fboundp("tool-bar-setup")
        && eval
            .obarray()
            .symbol_value("tool-bar-mode")
            .is_some_and(|value| value.is_truthy())
        && eval
            .obarray()
            .default_value_id(intern("tool-bar-map"))
            .is_some_and(|map| list_length(map) == Some(1));
    if !needs_setup {
        return;
    }
    if let Err(err) = eval.eval_str("(tool-bar-setup)") {
        tracing::warn!("failed running GNU tool-bar setup: {err}");
    }
}

fn throw_on_input_active(eval: &Context) -> bool {
    eval.obarray()
        .symbol_value("throw-on-input")
        .is_some_and(|value| value.is_truthy())
}

fn sync_selected_gui_chrome_state(eval: &mut Context) {
    // GUI chrome collection evaluates dynamic menu/tool-bar forms and this
    // host callback cannot propagate their non-local control flow. Preserve
    // the last snapshot until GNU's `while-no-input` scope has finished.
    if throw_on_input_active(eval) {
        return;
    }

    let menu_enabled = !eval
        .obarray()
        .symbol_value("menu-bar-mode")
        .copied()
        .unwrap_or(Value::NIL)
        .is_nil();
    let tool_enabled = !eval
        .obarray()
        .symbol_value("tool-bar-mode")
        .copied()
        .unwrap_or(Value::NIL)
        .is_nil();
    if tool_enabled {
        ensure_gnu_tool_bar_setup(eval);
    }
    let selected_gui_frame_id = eval
        .frame_manager()
        .selected_frame()
        .filter(|frame| frame.effective_window_system().is_some())
        .map(|frame| frame.id);
    let menu_items = if menu_enabled {
        selected_gui_frame_id
            .map(|frame_id| collect_gui_menu_bar_items_for_frame(eval, frame_id))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    // The frame-context collector is the one GNU-shaped source
    // (`update_tool_bar` selects the frame's window and buffer first); it
    // also retains its result across frames until GNU's rebuild predicate
    // fires, so the layout's own collection of the same frame is a cache hit.
    let tool_items = if tool_enabled {
        selected_gui_frame_id
            .map(|frame_id| collect_gui_tool_bar_items_for_frame(eval, frame_id))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let compact_bar_enabled =
        compact_bar_mode_enabled(eval) && (!menu_items.is_empty() || !tool_items.is_empty());

    let mut geometry_hints = None;
    if let Some(frame) = eval.frame_manager_mut().selected_frame_mut() {
        if frame.effective_window_system().is_none() {
            return;
        }
        // A shown GUI frame realizes its menu/tab/tool bars into the frame's
        // top margin (GNU's FRAME_TOP_MARGIN), so the window text area — and
        // the windows' tab/header lines — must sit below them.  The reused
        // initial GUI frame is created after `run()`'s interactive
        // `displays_chrome` pass, so it would otherwise keep the default
        // `false` and lay the root window at y=0, hidden behind the bars.
        // Mark it here (the GUI-only chrome sync) before the height-driven
        // `sync_window_area_bounds`, so the reflow reserves the chrome rows.
        frame.displays_chrome = true;
        frame.set_parameter(
            FrameParam::MenuBarLines.symbol(),
            Value::fixnum(if menu_items.is_empty() || compact_bar_enabled {
                0
            } else {
                1
            }),
        );
        frame.set_parameter(
            FrameParam::ToolBarLines.symbol(),
            Value::fixnum(if tool_items.is_empty() || compact_bar_enabled {
                0
            } else {
                1
            }),
        );
        frame.set_parameter(
            Value::symbol("compact-bar-lines"),
            Value::fixnum(if compact_bar_enabled { 1 } else { 0 }),
        );
        frame.sync_menu_bar_height_from_parameters();
        frame.sync_tool_bar_height_from_parameters();
        frame.sync_compact_bar_height_from_parameters();
        geometry_hints = Some((frame.id, frame.gui_geometry_hints()));
    }

    if let Some((frame_id, hints)) = geometry_hints
        && let Some(host) = eval.display_host.as_mut()
    {
        let _ = host.set_gui_frame_geometry_hints(frame_id, hints);
    }
}

fn gui_frame_font_scale_from_observation(
    observation: neomacs_display_protocol::DisplayObservation,
) -> ResolvedFrameFontScale {
    let profile = resolve_frame_font_scale(observation, FrameFontScalePolicy::Automatic);
    tracing::info!(
        ?observation,
        source = ?profile.source(),
        logical_dpi = profile.font_sizing().layout_dpi(),
        "resolved GUI frame font scale"
    );
    profile
}

/// One-shot release of the dirty pages startup churned through, now that the
/// image is instantiated and the big free waves are over. With deferred decay
/// (see `JEMALLOC_CONFIG`) jemalloc would only purge lazily on later
/// allocation ticks — an idle session would sit on the slack indefinitely
/// (the gc-cons-threshold batch timer taught us not to rely on "later").
#[cfg(all(
    target_os = "linux",
    any(feature = "platform-allocator", feature = "jemalloc"),
))]
fn jemalloc_release_startup_slack() {
    // MALLCTL_ARENAS_ALL is 4096 in jemalloc's public mallctl namespace.
    let name = c"arena.4096.purge";
    // SAFETY: a valid NUL-terminated mallctl command with no in/out params.
    unsafe {
        tikv_jemalloc_sys::mallctl(
            name.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        );
    }
}

#[cfg(not(all(
    target_os = "linux",
    any(feature = "platform-allocator", feature = "jemalloc"),
)))]
fn jemalloc_release_startup_slack() {}

fn create_startup_evaluator_for_mode(mode: RuntimeMode, startup: &StartupOptions) -> Context {
    let evaluator = match mode {
        RuntimeMode::Raw => raw_source_bootstrap_evaluator(),
        RuntimeMode::BootstrapUse => {
            neovm_core::emacs_core::load::load_runtime_image_with_features(
                RuntimeImageRole::Bootstrap,
                BOOTSTRAP_CORE_FEATURES,
                startup.dump_file_override.as_deref(),
            )
            .unwrap_or_else(|err| {
                panic!(
                    "bootstrap image should load: {}",
                    render_startup_image_error(&err)
                )
            })
        }
        RuntimeMode::FinalRun => {
            // Degrade by image availability instead of panicking on a
            // missing dump: a debug build after cargo clean (or before the
            // release-only cargo xtask fresh-build pipeline has run) has no
            // final image, which used to abort every debug launch. A dump
            // that exists but fails to load still panics loudly - that is
            // corruption, not absence. An explicit --dump-file keeps the
            // strict behavior.
            if startup.dump_file_override.is_some()
                || neovm_core::emacs_core::load::runtime_image_available(RuntimeImageRole::Final)
            {
                neovm_core::emacs_core::load::load_runtime_image_with_features(
                    RuntimeImageRole::Final,
                    BOOTSTRAP_CORE_FEATURES,
                    startup.dump_file_override.as_deref(),
                )
                .unwrap_or_else(|err| {
                    panic!(
                        "final image should load: {}",
                        render_startup_image_error(&err)
                    )
                })
            } else if neovm_core::emacs_core::load::runtime_image_available(
                RuntimeImageRole::Bootstrap,
            ) {
                tracing::warn!(
                    "final runtime image not found; falling back to the bootstrap \
                     image (slower startup, compile-main lisp loads from source). \
                     Run cargo xtask fresh-build --release to build runtime images."
                );
                neovm_core::emacs_core::load::load_runtime_image_with_features(
                    RuntimeImageRole::Bootstrap,
                    BOOTSTRAP_CORE_FEATURES,
                    None,
                )
                .unwrap_or_else(|err| {
                    panic!(
                        "bootstrap image should load: {}",
                        render_startup_image_error(&err)
                    )
                })
            } else {
                tracing::warn!(
                    "no runtime images found; bootstrapping from lisp sources \
                     (slow startup). Run cargo xtask fresh-build --release to \
                     build runtime images."
                );
                let mut evaluator = raw_source_bootstrap_evaluator();
                neovm_core::emacs_core::load::activate_runtime_evaluator(
                    &mut evaluator,
                    RuntimeImageRole::Final,
                )
                .unwrap_or_else(|err| {
                    panic!(
                        "source fallback should activate as a final runtime: {}",
                        render_startup_image_error(&err)
                    )
                });
                evaluator
            }
        }
    };
    // Image instantiation and bootstrap loading are the startup allocation
    // churn; release their dirty-page slack once, here, for every mode.
    jemalloc_release_startup_slack();
    evaluator
}

fn raw_source_bootstrap_evaluator() -> Context {
    // A source preload is image construction, not a user session.  The core
    // API's preload-only variant has no argv field, so user startup options
    // cannot reach loadup.el's final `(eval top-level t)`.  Context's
    // command-line-processed=t bootstrap default makes that tail inert; the
    // outer command loop starts the one real session after the final GUI/TTY
    // terminal has been installed.
    let invocation = source_bootstrap_loadup_invocation();
    neovm_core::emacs_core::load::create_bootstrap_evaluator_for_loadup(
        BOOTSTRAP_CORE_FEATURES,
        &invocation,
    )
    .expect("raw bootstrap should succeed")
}

fn source_bootstrap_loadup_invocation() -> LoadupInvocation {
    LoadupInvocation::PreloadOnly
}

fn raw_loadup_command_line(startup: &StartupOptions, dump_mode: LoadupDumpMode) -> Vec<String> {
    // A dump exits from loadup.el before its `(eval top-level t)` tail, so its
    // build argv is safe to expose in full.  Preload-only construction has no
    // command-line surface at all and therefore cannot call this function.
    let mut args = startup.forwarded_args.clone();
    if args.is_empty() {
        args.push(RuntimeMode::Raw.binary_name().to_string());
    }

    // GNU emacs.c:2578 — `if (!no_loadup) ... loadup.el`. We achieve the
    // same effect at the argv level by skipping the `-l loadup` splice
    // when --no-loadup is set. The `--temacs=...` mode below still
    // appends so that the rest of dump bookkeeping continues to run.
    let has_internal_loadup_marker =
        matches!(args.get(1).map(String::as_str), Some("-l" | "--load"))
            && args.get(2).map(String::as_str) == Some("loadup");
    if !startup.no_loadup && !has_internal_loadup_marker {
        args.splice(1..1, ["-l".to_string(), "loadup".to_string()]);
    }

    let has_temacs_mode = args
        .iter()
        .any(|arg| arg == "-temacs" || arg == "--temacs" || arg.starts_with("--temacs="));
    if !has_temacs_mode {
        args.push(format!("--temacs={}", dump_mode.as_gnu_string()));
    }

    args
}

fn raw_dump_loadup_invocation(
    startup: &StartupOptions,
    dump_mode: LoadupDumpMode,
) -> LoadupInvocation {
    LoadupInvocation::Dump(LoadupDumpInvocation::new(
        dump_mode,
        raw_loadup_command_line(startup, dump_mode),
    ))
}

fn load_neomacs_gui_term_layer(evaluator: &mut Context) {
    if evaluator
        .eval_str("(featurep 'neo-win)")
        .is_ok_and(|value| !value.is_nil())
    {
        return;
    }

    evaluator
        .eval_str("(provide 'neomacs)")
        .expect("GUI terminal layer should advertise the Neomacs backend");

    let load_path = get_load_path(evaluator.obarray(), evaluator.buffers.current_buffer());
    for library in ["term/common-win", "term/neo-win"] {
        let Some(path) = find_file_in_load_path(library, &load_path) else {
            panic!("required GUI terminal library should be found: {library}");
        };
        tracing::info!("Loading Neomacs GUI terminal layer: {library}");
        load_file(evaluator, &path).unwrap_or_else(|err| {
            panic!("failed to load {library}: {err:?}");
        });
    }
}

fn run_gui_main_thread(
    event_loop: RenderEventLoop,
    mode: RuntimeMode,
    startup: StartupOptions,
    width: u32,
    height: u32,
    bootstrap_display: BootstrapDisplayConfig,
) {
    let render_waker = GuiEventLoopWaker::new(event_loop.create_proxy());

    let comms = ThreadComms::new();
    let (emacs_comms, render_comms) = comms.split();
    let primary_window_size: SharedPrimaryWindowSize =
        Arc::new(Mutex::new(PrimaryWindowSize { width, height }));
    let gui_image_metadata: SharedImageRenderState =
        Arc::new(neomacs_display_runtime::render_thread::ImageRenderState::default());
    let shared_monitors: SharedMonitorInfo = Arc::new((Mutex::new(Vec::new()), Condvar::new()));
    #[cfg(feature = "neo-term")]
    let shared_terminals = new_shared_terminals();

    let evaluator_handle = spawn_gui_evaluator_worker(
        mode,
        startup,
        width,
        height,
        bootstrap_display,
        emacs_comms,
        Arc::clone(&primary_window_size),
        Arc::clone(&gui_image_metadata),
        Arc::clone(&shared_monitors),
        #[cfg(feature = "neo-term")]
        shared_terminals.clone(),
        render_waker.clone(),
    );

    tracing::info!(
        "GUI event loop entering on OS main thread ({}x{})",
        width,
        height
    );
    let render_result = {
        #[cfg(feature = "neo-term")]
        {
            run_render_loop_current_thread_with_terminals(
                event_loop,
                render_comms,
                width,
                height,
                "Neomacs".to_string(),
                Arc::clone(&gui_image_metadata),
                Arc::clone(&shared_monitors),
                shared_terminals,
            )
        }
        #[cfg(not(feature = "neo-term"))]
        {
            run_render_loop_current_thread(
                event_loop,
                render_comms,
                width,
                height,
                "Neomacs".to_string(),
                Arc::clone(&gui_image_metadata),
                Arc::clone(&shared_monitors),
            )
        }
    };
    if let Err(err) = &render_result {
        tracing::error!("GUI event loop exited with error: {err}");
    }

    let evaluator_exit = match evaluator_handle.join() {
        Ok(exit) => exit,
        Err(payload) => {
            std::panic::resume_unwind(payload);
        }
    };

    if evaluator_exit.restart {
        tracing::warn!("restart requested via kill-emacs, but restart is not implemented yet");
    }
    if evaluator_exit.exit_code != 0 {
        std::process::exit(evaluator_exit.exit_code);
    }
    if render_result.is_err() {
        std::process::exit(1);
    }
}

fn spawn_gui_evaluator_worker(
    mode: RuntimeMode,
    startup: StartupOptions,
    width: u32,
    height: u32,
    bootstrap_display: BootstrapDisplayConfig,
    emacs_comms: EmacsComms,
    primary_window_size: SharedPrimaryWindowSize,
    gui_image_metadata: SharedImageRenderState,
    shared_monitors: SharedMonitorInfo,
    #[cfg(feature = "neo-term")] shared_terminals: SharedTerminals,
    render_waker: GuiEventLoopWaker,
) -> std::thread::JoinHandle<EvaluatorExit> {
    let cmd_tx_for_panic = emacs_comms.cmd_tx.clone();
    let render_waker_for_panic = render_waker.clone();
    std::thread::Builder::new()
        .name("neomacs-evaluator".to_string())
        // GNU grows the main C stack before Lisp startup.  In the GUI
        // topology the Lisp evaluator is the Emacs main thread semantically,
        // but it runs on a Rust worker so winit can own the OS main thread.
        // Give that worker an explicit native stack instead of relying on
        // pthread defaults chosen before increase_stack_limit runs.
        .stack_size(GUI_EVALUATOR_THREAD_STACK_SIZE)
        .spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_gui_evaluator_worker(
                    mode,
                    startup,
                    width,
                    height,
                    bootstrap_display,
                    emacs_comms,
                    primary_window_size,
                    gui_image_metadata,
                    shared_monitors,
                    #[cfg(feature = "neo-term")]
                    shared_terminals,
                    render_waker,
                )
            }));
            match outcome {
                Ok(exit) => exit,
                Err(payload) => {
                    let _ = cmd_tx_for_panic
                        .try_send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown));
                    render_waker_for_panic.wake();
                    std::panic::resume_unwind(payload);
                }
            }
        })
        .expect("Failed to spawn GUI evaluator worker")
}

/// R2-C3: prepopulate the AOT preload into the JIT cache so the loadup set serves
/// NATIVE FROM CALL 1, before the first dispatch (recursive-edit). Runs ONLY for
/// the production `neomacs` image (`RuntimeMode::FinalRun`) on the eval thread
/// that owns the thread-local `COMPILED` cache. It self-gates internally on
/// `NEOVM_AOT` (a no-op unless enabled) and on the preload being present + valid
/// (a missing/stale preload is a clean skip→JIT), so this call is non-fatal and
/// pays nothing in the default (AOT-off) configuration. Loud on `NEOVM_AOT=force`
/// (so the bench/gate can confirm it ran), quiet otherwise.
#[cfg(feature = "jit")]
fn maybe_prepopulate_aot(mode: RuntimeMode, evaluator: &Context) {
    if mode != RuntimeMode::FinalRun {
        return;
    }
    // LAZY prewarm: mark preload members so dispatch serves them from call 1;
    // each leaf is built by the cache-miss AOT consult on first use. The
    // eager prepopulate (~16.5ms up front for ~1.2k leaves) remains available
    // to tests/benchmarks via prepopulate_aot_from_preload.
    let t0 = std::time::Instant::now();
    let (candidates, marked) =
        neovm_core::emacs_core::jit::aot::mark_preload_members_prewarmed(evaluator);
    if std::env::var_os("NEOVM_AOT_TIMING").is_some() {
        eprintln!(
            "AOT_TIMING lazy-prewarm: {:?} (candidates={candidates} marked={marked})",
            t0.elapsed(),
        );
    }
    if marked > 0 {
        let force = matches!(std::env::var("NEOVM_AOT").as_deref(), Ok("force"));
        if force {
            tracing::info!(
                "AOT preload: marked {marked} / {candidates} loadup fns prewarmed —                  native from call 1, leaves load lazily"
            );
        } else {
            tracing::debug!("AOT preload: marked {marked} / {candidates} loadup fns prewarmed");
        }
    }
}

/// No-op when the `jit` feature is off (no AOT producer/loader compiled in).
#[cfg(not(feature = "jit"))]
fn maybe_prepopulate_aot(_mode: RuntimeMode, _evaluator: &Context) {}

/// R2 increment C — at shutdown (post-`recursive_edit`, the `Context` still alive on
/// THIS eval thread), persist this session's proven-hot JIT leaves to
/// `NEOVM_AOT_DIR` so the NEXT session serves them native + speculative from call 1.
/// Self-gates internally on `NEOVM_AOT_PGO` + `NEOVM_AOT_DIR` (a no-op — and zero
/// cost — in the default config, so this call is non-fatal and pays nothing off the
/// PGO path) and on `RuntimeMode::FinalRun` (the production image). MUST run on the
/// eval thread that owns the thread-local `COMPILED` cache — this is that thread.
/// `kill-emacs` unwinds `recursive_edit` back to here, so no kill-emacs-hook wiring
/// is needed. Emit-side only: it WRITES `.so`s; the load path is unchanged.
#[cfg(feature = "jit")]
fn maybe_drain_aot_pgo(mode: RuntimeMode, evaluator: &Context) {
    if mode != RuntimeMode::FinalRun {
        return;
    }
    let n = neovm_core::emacs_core::jit::aot::drain_aot_pgo(evaluator);
    if n > 0 {
        tracing::info!(
            "AOT-PGO: persisted {n} hot JIT leaf .so(s) to NEOVM_AOT_DIR for the next session"
        );
    }
}

/// No-op when the `jit` feature is off (no AOT producer compiled in).
#[cfg(not(feature = "jit"))]
fn maybe_drain_aot_pgo(_mode: RuntimeMode, _evaluator: &Context) {}

fn run_gui_evaluator_worker(
    mode: RuntimeMode,
    startup: StartupOptions,
    width: u32,
    height: u32,
    bootstrap_display: BootstrapDisplayConfig,
    emacs_comms: EmacsComms,
    primary_window_size: SharedPrimaryWindowSize,
    gui_image_metadata: SharedImageRenderState,
    shared_monitors: SharedMonitorInfo,
    #[cfg(feature = "neo-term")] shared_terminals: SharedTerminals,
    render_waker: GuiEventLoopWaker,
) -> EvaluatorExit {
    let mut evaluator = create_startup_evaluator_for_mode(mode, &startup);
    evaluator.setup_thread_locals();
    reset_terminal_host();
    reset_terminal_runtime();
    // GNU's `x_term_init'/`pgtk_term_init' create an output_x_window/output_pgtk
    // terminal and name it after the display connection (":0", "wayland-0", …),
    // not "initial_terminal". Both halves matter: Elisp that keys off
    // `(terminal-name)` to detect a real display — e.g. indent-bars' theme-reset
    // guard — needs the name, and `frame-initial-p` needs the terminal to say it
    // is not the initial one even on a display with no name to adopt. Read the
    // display the same way the frame's `display` parameter does.
    let mut display_terminal = TerminalRuntimeConfig::window_system();
    if let Some(display_name) = host_gui_display_identity().native_display() {
        display_terminal = display_terminal.with_name(display_name);
    }
    configure_terminal_runtime(display_terminal);
    evaluator.set_variable("dump-mode", Value::NIL);
    // GNU's window-system terminal inits do not measure a line speed, they
    // assert one: `baud_rate = 19200' in `x_term_init' (src/xterm.c:32279) and
    // in `pgtk_term_init' (src/pgtkterm.c:7034). Same place, same constant.
    evaluator.set_variable("baud-rate", Value::fixnum(19200));
    load_neomacs_gui_term_layer(&mut evaluator);
    tracing::info!("GUI evaluator context initialized");

    let _bootstrap = bootstrap_buffers(&mut evaluator, width, height, bootstrap_display);
    let frame_id = evaluator
        .frame_manager()
        .selected_frame()
        .expect("No selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut evaluator, frame_id, &startup);
    maybe_install_startup_phase_trace(&mut evaluator);

    evaluator.set_display_host(Box::new(PrimaryWindowDisplayHost {
        cmd_tx: emacs_comms.cmd_tx.clone(),
        render_waker: Some(render_waker.clone()),
        font_sizing: bootstrap_display.font_sizing(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(HashMap::new()),
        font_metrics: None,
        primary_window_size: Arc::clone(&primary_window_size),
        image_catalog: Rc::new(AsyncImageCatalog::new(
            emacs_comms.cmd_tx.clone(),
            Some(render_waker.clone()),
            Arc::clone(&gui_image_metadata),
        )),
        #[cfg(feature = "video")]
        resolved_videos: Mutex::new(ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(HashMap::new()),
        resolved_surfaces: Mutex::new(ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::clone(&emacs_comms.capabilities),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: TerminalHostState::new(shared_terminals),
    }));
    adopt_existing_primary_gui_frame(&mut evaluator)
        .expect("bootstrap GUI frame adoption should succeed");

    prime_initial_monitor_snapshot(&shared_monitors);

    let (input_tx, input_rx) = crossbeam_channel::unbounded();
    let secondary_ttys = secondary_tty::SecondaryTtyRegistry::default();
    let display_input_rx = emacs_comms.input_rx;
    let primary_window_size_for_input = Arc::clone(&primary_window_size);
    let quit_requested = Arc::clone(&evaluator.quit_requested);
    // Cross-platform wakeup: wake the evaluator's wait loop AFTER queueing input
    // so it drains the channel immediately. Correct ordering (post-send) and
    // works on every OS, unlike the Unix-only wakeup pipe.
    let input_notifier = evaluator.wait_notifier();
    let secondary_input_tx = input_tx.clone();
    let secondary_input_notifier = input_notifier.clone();
    let secondary_quit_requested = Arc::clone(&quit_requested);
    std::thread::Builder::new()
        .name("input-bridge".to_string())
        .spawn(move || {
            while let Ok(event) = display_input_rx.recv() {
                let should_log = input_bridge::should_log_display_event(&event);
                if should_log {
                    tracing::debug!("input-bridge: received display event {:?}", event);
                }
                record_primary_window_resize(&primary_window_size_for_input, &event);
                let mut queued_input = false;
                let mut evaluator_disconnected = false;
                for kb_event in input_bridge::convert_display_event(&event) {
                    if should_log {
                        tracing::debug!(
                            "input-bridge: converted display event {:?} to keyboard event {:?}",
                            event,
                            kb_event
                        );
                    }
                    if kb_event.requests_default_quit() {
                        quit_requested.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    if input_tx.send(kb_event).is_err() {
                        evaluator_disconnected = true;
                        break;
                    }
                    queued_input = true;
                }
                if evaluator_disconnected {
                    break;
                }
                if queued_input {
                    if let Some(notifier) = &input_notifier
                        && let Err(error) = notifier.notify()
                    {
                        tracing::error!(%error, "input bridge failed to wake evaluator");
                    }
                }
            }
        })
        .expect("Failed to spawn input bridge thread");

    evaluator.init_input_system(input_rx);
    evaluator.set_tty_frame_host_factory(Box::new(secondary_tty::SecondaryTtyFactory::new(
        secondary_ttys.clone(),
        secondary_input_tx,
        secondary_input_notifier,
        secondary_quit_requested,
    )));
    install_diagnostics_eval_hooks(&mut evaluator);

    frame_layout::REDISPLAY_RUNTIME.with(|runtime| {
        runtime.use_scalable_metrics(bootstrap_display.font_sizing());
    });
    let frame_tx = emacs_comms.frame_tx;
    let initial_frame_tx = frame_tx.clone();
    let redisplay_waker = render_waker.clone();
    let secondary_ttys_for_redisplay = secondary_ttys.clone();
    evaluator.redisplay_fn = Some(Box::new(move |eval: &mut Context| {
        if !secondary_ttys_for_redisplay.render_selected(eval) {
            publish_gui_frame(eval, &frame_tx, Some(&redisplay_waker));
        }
    }));
    frame_layout::install_frame_snapshot_fn(&mut evaluator);
    frame_layout::install_window_layout_query_fn(&mut evaluator);
    publish_gui_frame(&mut evaluator, &initial_frame_tx, Some(&render_waker));

    if let Some(buf) = evaluator.buffer_manager_mut().current_buffer_mut() {
        let mut ul = buf.get_undo_list();
        neovm_core::buffer::undo_list_boundary(&mut ul);
        buf.set_undo_list(ul);
    }

    neovm_core::emacs_core::load::maybe_run_after_pdump_load_hook(&mut evaluator);
    // R2-C3: native-from-call-1 — prepopulate the AOT preload before first dispatch.
    maybe_prepopulate_aot(mode, &evaluator);
    tracing::info!("Entering GNU command loop on GUI evaluator worker...");
    let exit_status = evaluator.recursive_edit();
    if exit_status.is_ok() {
        tracing::info!("Command loop exited normally");
    } else {
        tracing::warn!("Command loop exited with error");
    }

    tracing::info!("GUI evaluator shutting down render loop...");
    let _ = emacs_comms
        .cmd_tx
        .try_send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown));
    render_waker.wake();

    // R2 increment C: persist this session's proven-hot JIT leaves before exit
    // (Context still alive on this eval thread; runs BEFORE the shutdown-request
    // early return so it fires on kill-emacs too). No-op unless NEOVM_AOT_PGO set.
    maybe_drain_aot_pgo(mode, &evaluator);

    if let Some(request) = evaluator.shutdown_request() {
        let exit = EvaluatorExit {
            exit_code: request.exit_code,
            restart: request.restart,
        };
        leak_evaluator_for_process_exit(evaluator);
        return exit;
    }

    leak_evaluator_for_process_exit(evaluator);
    EvaluatorExit::OK
}

/// Assemble a diagnostics metrics snapshot from the live producers.
///
/// Reads only process-global published atomics (frame scheduling + GC), so it
/// is safe to call from the diagnostics thread with no VM access.
fn build_metrics_snapshot() -> neomacs_diagnostics::MetricsSnapshot {
    use neomacs_diagnostics::metrics::{FrameMetrics, GcMetrics, WindowFrameMetrics};

    let f = neomacs_display_runtime::frame_metrics_snapshot();
    let g = neovm_core::emacs_core::gc_stats::snapshot();
    // Per-native-window demand attribution plus the process-wide union of
    // active reasons (design doc, Observability).
    let per_window = neomacs_display_runtime::window_frame_metrics_snapshot();
    let active_union: std::collections::BTreeSet<&str> = per_window
        .iter()
        .flat_map(|w| w.active_reasons.iter().copied())
        .collect();
    let windows = per_window
        .iter()
        .map(|w| {
            (
                w.window,
                WindowFrameMetrics {
                    active_reasons: w.active_reasons.iter().map(|r| (*r).to_owned()).collect(),
                    demand_reasons: neomacs_display_runtime::DEMAND_REASON_NAMES
                        .iter()
                        .zip(w.demand_reasons.iter())
                        .filter(|(_, count)| **count > 0)
                        .map(|(name, count)| ((*name).to_owned(), *count))
                        .collect(),
                },
            )
        })
        .collect();

    neomacs_diagnostics::MetricsSnapshot {
        frame: FrameMetrics {
            presents: f.presents,
            scene_commits: f.scene_commits,
            wakeups: f.wakeups,
            deadline_serviced_redraws: f.deadline_serviced_redraws,
            last_commit_to_present_us: f.last_commit_to_present_us,
            max_commit_to_present_us: f.max_commit_to_present_us,
            frame_p50_us: neomacs_diagnostics::metrics::percentile_from_buckets(
                &f.frame_time_buckets,
                &neomacs_display_runtime::FRAME_TIME_BUCKET_UPPER_US,
                0.50,
            ),
            frame_p95_us: neomacs_diagnostics::metrics::percentile_from_buckets(
                &f.frame_time_buckets,
                &neomacs_display_runtime::FRAME_TIME_BUCKET_UPPER_US,
                0.95,
            ),
            frame_p99_us: neomacs_diagnostics::metrics::percentile_from_buckets(
                &f.frame_time_buckets,
                &neomacs_display_runtime::FRAME_TIME_BUCKET_UPPER_US,
                0.99,
            ),
            composite_only_frames: f.composite_only_frames,
            retained_static_builds: f.retained_static_builds,
            demand_reasons: neomacs_display_runtime::DEMAND_REASON_NAMES
                .iter()
                .zip(f.demand_reasons.iter())
                .map(|(name, count)| ((*name).to_owned(), *count))
                .collect(),
            unattributed_presents: f.unattributed_present_attempts,
            active_reasons: active_union.iter().map(|r| (*r).to_owned()).collect(),
            windows,
        },
        gc: GcMetrics {
            collections: g.collections,
            live_bytes: g.live_bytes,
            total_allocated_bytes: g.total_allocated_bytes,
            cons_cells: g.cons_cells,
            strings: g.strings,
            vector_cells: g.vector_cells,
        },
    }
}

/// Reply timeout for a diagnostics profile-capture round-trip to the Lisp
/// thread. Bounds the wait so a stuck editor yields a 503, never a hang.
const DIAG_REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// The eval-thread task Receiver, published for whichever Context path (GUI
/// worker or TTY) boots. Taken once by that path via `init_eval_task_system`.
static DIAG_TASK_RX: std::sync::Mutex<
    Option<crossbeam_channel::Receiver<neovm_core::emacs_core::eval::EvalThreadTask>>,
> = std::sync::Mutex::new(None);

/// The Lisp thread's wait notifier, so the diagnostics thread can wake it after
/// queueing a task. It can be absent only when the platform poller could not
/// be created; [`ensure_diag_notifier`] also retries publication from tasks
/// that run on the Lisp thread.
static DIAG_NOTIFIER: std::sync::OnceLock<neovm_core::emacs_core::process::WaitNotifier> =
    std::sync::OnceLock::new();

/// Set once an interactive Context has installed the eval-task channel, so the
/// diagnostics server knows a capture can be serviced. Decoupled from
/// `DIAG_NOTIFIER` because the notifier may not exist yet (see above); a queued
/// task is still drained at the next `read_char` iteration (timer/input),
/// so an active editor is profilable even before the notifier is published.
static DIAG_INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Wake the Lisp thread if its notifier has been published (best-effort — an
/// active editor also drains queued tasks on its next loop iteration).
fn diag_wake_lisp() {
    if let Some(notifier) = DIAG_NOTIFIER.get()
        && let Err(error) = notifier.notify()
    {
        tracing::warn!(%error, "diagnostics task failed to wake evaluator");
    }
}

/// Publish the Context's wait notifier if available and not yet set. Called from
/// task closures, which run on the Lisp thread during `recursive_edit`.
fn ensure_diag_notifier(ctx: &Context) {
    if DIAG_NOTIFIER.get().is_none()
        && let Some(notifier) = ctx.wait_notifier()
    {
        let _ = DIAG_NOTIFIER.set(notifier);
    }
}

/// Hand this Context the eval-thread task channel and (best-effort) publish its
/// wait notifier, so the diagnostics server can drive on-demand profile
/// captures. Called from each interactive Context path right after the input
/// system is initialized. The channel Receiver is inert if diagnostics is off.
fn install_diagnostics_eval_hooks(evaluator: &mut Context) {
    if let Some(notifier) = evaluator.wait_notifier() {
        let _ = DIAG_NOTIFIER.set(notifier);
    }
    if let Some(rx) = DIAG_TASK_RX.lock().unwrap().take() {
        evaluator.init_eval_task_system(rx);
    }
    DIAG_INSTALLED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Bridges diagnostics profile requests onto the Lisp thread: queue an
/// [`neovm_core::emacs_core::eval::EvalThreadTask`] and wake the Lisp thread,
/// which services it at its next safe point.
struct DiagProfileCtrl {
    task_tx: crossbeam_channel::Sender<neovm_core::emacs_core::eval::EvalThreadTask>,
}

impl neomacs_diagnostics::ProfileController for DiagProfileCtrl {
    fn is_live(&self) -> bool {
        DIAG_INSTALLED.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn start(&self, interval_ns: u64) -> Result<bool, String> {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        self.task_tx
            .send(Box::new(move |ctx: &mut Context| {
                ensure_diag_notifier(ctx);
                let started = ctx.diagnostics_cpu_profile_start(interval_ns);
                let _ = reply_tx.send(started);
            }))
            .map_err(|_| "eval task channel closed".to_string())?;
        diag_wake_lisp();
        reply_rx
            .recv_timeout(DIAG_REPLY_TIMEOUT)
            .map_err(|_| "start timed out".to_string())
    }

    fn stop_and_fold(&self) -> Result<String, String> {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        self.task_tx
            .send(Box::new(move |ctx: &mut Context| {
                ensure_diag_notifier(ctx);
                let folded = ctx.diagnostics_cpu_profile_stop_fold();
                let _ = reply_tx.send(folded);
            }))
            .map_err(|_| "eval task channel closed".to_string())?;
        diag_wake_lisp();
        reply_rx
            .recv_timeout(DIAG_REPLY_TIMEOUT)
            .map_err(|_| "capture timed out".to_string())
    }

    fn abort(&self) {
        let _ = self.task_tx.send(Box::new(|ctx: &mut Context| {
            ensure_diag_notifier(ctx);
            ctx.diagnostics_cpu_profile_abort();
        }));
        diag_wake_lisp();
    }
}

/// Start the diagnostics HTTP server if `NEOMACS_DIAGNOSTICS_PORT` names a valid
/// TCP port. Off by default. Best-effort: any failure is logged and ignored so
/// diagnostics never blocks editor startup. `task_tx` is dropped here when
/// disabled, leaving the eval-thread channel inert.
fn maybe_start_diagnostics(
    task_tx: crossbeam_channel::Sender<neovm_core::emacs_core::eval::EvalThreadTask>,
) {
    let Ok(raw) = std::env::var("NEOMACS_DIAGNOSTICS_PORT") else {
        return;
    };
    let Some(port) = neomacs_diagnostics::port_from_str(&raw) else {
        tracing::error!("NEOMACS_DIAGNOSTICS_PORT={raw:?} is not a valid TCP port; ignoring");
        return;
    };
    let provider = std::sync::Arc::new(build_metrics_snapshot);
    let controller: std::sync::Arc<dyn neomacs_diagnostics::ProfileController> =
        std::sync::Arc::new(DiagProfileCtrl { task_tx });
    match neomacs_diagnostics::spawn(
        neomacs_diagnostics::DiagnosticsConfig { port },
        provider,
        Some(controller),
    ) {
        Ok(_handle) => tracing::info!("neomacs diagnostics enabled on 127.0.0.1:{port}"),
        Err(e) => tracing::error!("failed to start diagnostics server: {e}"),
    }
}

pub fn run(mode: RuntimeMode) {
    let process_started_at = Instant::now();
    let process_args = std::env::args_os().collect::<Vec<_>>();

    // Always enable full backtraces for debugging low-level runtime crashes.
    if std::env::var("RUST_BACKTRACE").is_err() {
        unsafe {
            std::env::set_var("RUST_BACKTRACE", "1");
        }
    }

    // Raise RLIMIT_STACK before anything deep runs. Deep Elisp evaluation
    // chains (startup.el → normal-top-level → command-line → init → Doom
    // hooks) recurse on the native stack here and exhaust the usual 8 MB.
    //
    // GNU also raises it in main(), but computes its target rather than
    // choosing one: `emacs_re_max_failures * ratio + extra`, rounded to a page
    // and sized for regex-emacs.c's backtrack stack (src/emacs.c:1563-1623),
    // which is 9788 KiB on this machine against the 128 MiB below. That is a
    // deliberately different policy, and it is OBSERVABLE from Lisp, so it is
    // written down here rather than left to be rediscovered: glibc derives
    // `_SC_ARG_MAX` from RLIMIT_STACK as `MAX (131072, MIN (stack / 4, 6 MiB))`,
    // and `syms_of_callproc` initializes `command-line-max-length` to
    // `sysconf (_SC_ARG_MAX) / 4` (src/callproc.c:2246-2252) AFTER this point
    // in GNU's own startup (src/emacs.c:2172). 128 MiB lands on glibc's 6 MiB
    // cap, so that variable reads 1572864 here where GNU reads 626432 -- one
    // declaration, two stack policies. Both numbers are correct reports of the
    // editor that produced them; see ledger entry 168 item 2, and
    // `oracle_command_line_max_length_is_derived_from_this_editors_stack_rlimit`,
    // which pins the derivation rather than either number.
    increase_stack_limit();

    // Initialize the C library locale from the environment (GNU emacs.c main()
    // calls `setlocale (LC_ALL, "")` + `fixup_locale ()`). This makes
    // locale-aware C-library facilities — notably `wcscoll`/`towlower` behind
    // `string-collate-lessp`/`string-collate-equalp` — honor the user's
    // collation locale instead of defaulting to the "C" (code-point) locale.
    initialize_system_locale();

    // Make the editor immune to a child process suspending it via job control
    // (issue #132): children are spawned in their own process group
    // (callproc::new_child_command), and we ignore SIGTTOU so terminal output
    // from a momentarily-backgrounded neomacs never stops the whole editor.
    install_job_control_signal_hygiene();

    // Handle --help / --version with no logging side effects (so e.g.
    // `NEOMACS_LOG_TO_FILE=1 neomacs --help` does not create a stray
    // neomacs-{pid}.log file).
    if let Some(action) = classify_early_cli_action(std::env::args()) {
        match action {
            EarlyCliAction::PrintHelp { program } => {
                print!("{}", render_help_text(&program));
            }
            EarlyCliAction::PrintVersion => {
                print!("{}", render_version_text());
            }
            EarlyCliAction::PrintFingerprint => {
                print!("{}", render_fingerprint_text());
            }
        }
        return;
    }

    // Parse argv before initializing tracing so we know whether this is a
    // GUI or TTY run — logging policy differs between the two (under TTY
    // any tracing output would smash the alt-screen redisplay engine).
    // `parse_startup_options` emits no tracing events, so delaying init
    // past it costs no diagnostics.
    let startup = parse_startup_options(std::env::args()).unwrap_or_else(|message| {
        eprintln!("{message}");
        std::process::exit(1);
    });

    // Initialize tracing with a writer target appropriate to the
    // binary:
    //
    // - `neomacs-temacs` (RuntimeMode::Raw) and `bootstrap-neomacs`
    //   (RuntimeMode::BootstrapUse) are build-time utilities whose
    //   stdout is captured by the xtask driver — they MUST log to
    //   stdout so the build log shows what they are doing. Frontend
    //   is always `Tty` for them (they run with --batch), but they
    //   have no TUI redisplay engine fighting for the pty, so
    //   stdout logging is safe and useful.
    //
    // - `neomacs` (RuntimeMode::FinalRun) is the user-facing binary.
    //   Under a GUI frontend, stdout is captured to a file by the
    //   calling shell (e.g. `> /tmp/neomacs-gui.log 2>&1`), so
    //   LogTarget::Stdout is fine. Under a TTY frontend (`-nw`,
    //   `--batch`), stdout is the alt-screen pty the redisplay
    //   engine is drawing into, so LogTarget::File routes tracing
    //   to a file instead.
    //
    // In all cases `NEOMACS_LOG_FILE=<path>` overrides the file path
    // (and, for LogTarget::Stdout, also adds a file layer alongside
    // stdout).
    let log_target = match mode {
        RuntimeMode::Raw | RuntimeMode::BootstrapUse => neovm_core::logging::LogTarget::Stdout,
        RuntimeMode::FinalRun => match startup.frontend {
            FrontendKind::Gui => neovm_core::logging::LogTarget::Stdout,
            FrontendKind::Tty => neovm_core::logging::LogTarget::File,
        },
    };
    let _logging_guard = neovm_core::logging::init(log_target);

    if mode == RuntimeMode::Raw
        && let Some(temacs_mode) = startup.temacs_mode
    {
        run_temacs_dump_mode(temacs_mode, &startup);
        return;
    }

    tracing::info!(
        host_profile = ?desktop_host_profile(),
        "{} {} starting (pure Rust, backend={}, pid={}, mode={:?}, image={:?})",
        mode.binary_name(),
        neomacs_display_runtime::VERSION,
        neomacs_display_runtime::CORE_BACKEND,
        std::process::id(),
        mode,
        mode.dump_image_kind()
    );
    tracing::info!("Startup frontend: {:?}", startup.frontend);
    if let Some(device) = startup.terminal_device.as_deref() {
        tracing::warn!(
            "terminal device {:?} requested; using current tty until explicit device handoff lands",
            device
        );
    }

    // Winit can fall back from the environment's preferred Linux backend.
    // Construct it before font metrics so the bootstrap frame follows the
    // backend that was actually selected, not a DISPLAY/WAYLAND_DISPLAY guess.
    let gui_event_loop = if startup.frontend == FrontendKind::Gui {
        Some(build_render_event_loop().unwrap_or_else(|err| {
            eprintln!("neomacs: failed to build GUI event loop: {err}");
            std::process::exit(1);
        }))
    } else {
        None
    };
    let interactivity = Interactivity::from_noninteractive(startup.noninteractive);
    let bootstrap_display = if let Some(event_loop) = gui_event_loop.as_ref() {
        let observation = observe_event_loop_display(event_loop);
        bootstrap_gui_display_config(
            interactivity,
            gui_frame_font_scale_from_observation(observation),
        )
    } else {
        debug_assert_eq!(startup.frontend, FrontendKind::Tty);
        bootstrap_tty_display_config(interactivity)
    };
    // For TTY, frame dimensions are in character cells (1x1), so we
    // don't need to scan the system font database for font metrics.
    // This avoids ~500ms of FontMetricsService initialization at
    // startup. GUI mode computes real pixel dimensions from font
    // metrics via bootstrap_frame_metrics().
    let frame_metrics = bootstrap_frame_metrics_for_display(bootstrap_display);
    let (width, height) =
        startup_dimensions(startup.frontend, frame_metrics, startup.noninteractive);

    // Optional localhost performance diagnostics server (off unless
    // NEOMACS_DIAGNOSTICS_PORT is set). Started before the GUI/TTY fork; the
    // eval-thread task channel's Receiver is published for whichever Context
    // path boots (see install_diagnostics_eval_hooks), so on-demand profile
    // captures can reach the Lisp thread.
    let (diag_task_tx, diag_task_rx) =
        crossbeam_channel::unbounded::<neovm_core::emacs_core::eval::EvalThreadTask>();
    *DIAG_TASK_RX.lock().unwrap() = Some(diag_task_rx);
    maybe_start_diagnostics(diag_task_tx);

    if startup.frontend == FrontendKind::Gui {
        run_gui_main_thread(
            gui_event_loop.expect("GUI frontend constructed an event loop"),
            mode,
            startup,
            width,
            height,
            bootstrap_display,
        );
        log_clean_process_exit(process_started_at, &process_args);
        return;
    }

    // 2. Initialize the evaluator from the canonical bootstrap surface.
    //    GNU loads the dumped bootstrap image here, then lets the outer
    //    command loop evaluate `top-level`/`normal-top-level`.
    let mut evaluator = create_startup_evaluator_for_mode(mode, &startup);
    evaluator.setup_thread_locals();
    if tty_init::should_enable_live_tty_io(&startup) {
        reset_terminal_host();
        configure_terminal_runtime(tty_init::detect_tty_runtime(&startup));
        // GNU `init_sys_modes' (src/sysdep.c:1130) publishes the terminal's
        // ERASE character here, read from the termios it saved before touching
        // the terminal modes. Do the same while stdout is still cooked and
        // `tty_init_terminal' has not entered raw mode, so the value describes
        // the user's stty setting. `normal-erase-is-backspace-setup-frame'
        // (lisp/simple.el) reads it during startup to decide whether Backspace
        // deletes or opens the help prefix.
        evaluator.set_variable(
            "tty-erase-char",
            tty_init::tty_erase_char_value(tty_init::detect_tty_erase_char()),
        );
        // GNU `init_tty' calls `init_baud_rate (fileno (tty->input))'
        // (src/term.c:4755) while setting the terminal up, which is the only
        // thing that ever writes the `baud-rate' DEFVAR_INT on a tty. Read the
        // line speed here, from the same still-cooked stdin the ERASE character
        // came from. Under `--batch' GNU creates no tty terminal at all, so
        // this must NOT run there -- `baud-rate' stays at the 0 the bootstrap
        // seeded, which is what GNU reports.
        evaluator.set_variable("baud-rate", Value::fixnum(tty_init::detect_baud_rate()));
    } else {
        reset_terminal_host();
        reset_terminal_runtime();
    }
    // GNU Emacs does NOT disable GC during startup — GC runs normally.
    // The bc_buf refactor and conservative stack scanning ensure all
    // bytecode VM values are reachable during collection.
    evaluator.set_variable("dump-mode", Value::NIL);
    tracing::info!("Context initialized");

    // 3. Bootstrap the host-side initial frame/buffers.
    let _bootstrap = bootstrap_buffers(&mut evaluator, width, height, bootstrap_display);
    let frame_id = evaluator
        .frame_manager()
        .selected_frame()
        .expect("No selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut evaluator, frame_id, &startup);
    if tty_init::should_enable_live_tty_io(&startup) {
        termcap_input::seed_input_decode_map_from_terminal(&mut evaluator);
    }

    maybe_install_startup_phase_trace(&mut evaluator);

    // 4. Create communication channels before entering GNU's outer
    //    recursive-edit command loop. GNU evaluates `top-level` from that
    //    outer loop, not directly from `main`.
    let comms = ThreadComms::new();
    let (emacs_comms, render_comms) = comms.split();
    let primary_window_size: SharedPrimaryWindowSize =
        Arc::new(Mutex::new(PrimaryWindowSize { width, height }));
    let tty_popup_force_full_redraw = Arc::new(AtomicBool::new(false));
    let secondary_ttys = secondary_tty::SecondaryTtyRegistry::default();
    if tty_init::should_enable_live_tty_io(&startup) {
        set_terminal_host(Box::new(tty_frontend::TtyTerminalHost {
            cmd_tx: emacs_comms.cmd_tx.clone(),
        }));
        evaluator.set_display_host(Box::new(tty_frontend::TtyPopupDisplayHost::new(
            tty_popup_force_full_redraw.clone(),
        )));
    }

    // 5. Spawn the frontend loop matching the requested startup mode.
    let frontend = if startup.noninteractive {
        // Batch mode: no terminal I/O, matching GNU which skips
        // init_display() for --batch (emacs.c:1835).
        tracing::info!("TTY batch mode — skipping terminal init");
        FrontendHandle::Batch
    } else {
        // Single-thread TTY path: terminal init here, rendering via TtyRif
        // on the evaluator thread, input reader on a background thread.
        // GNU init_tty parity: a terminal that cannot position the cursor
        // cannot run a full-screen editor — refuse while stdout is still
        // cooked, before any raw-mode or alternate-screen byte.
        if let Err(diagnostic) = tty_init::tty_check_terminal_powerful_enough() {
            eprintln!("{diagnostic}");
            std::process::exit(1);
        }
        tty_init::tty_init_terminal();
        let input_reader = tty_frontend::TtyInputReader::spawn(render_comms);
        tracing::info!("TTY frontend spawned (TtyRif single-thread redisplay)");
        FrontendHandle::TtyRifInput(input_reader)
    };

    // 6. Create input bridge: convert display runtime events → keyboard events.
    //
    // GNU Emacs does NOT initialize terminal I/O in --batch mode.
    // The evaluator runs without any input receiver, so
    // `input_rx.is_none()` correctly signals batch mode throughout
    // the keyboard/command-loop code. This prevents blocking on
    // `rx.recv()` in read_char_with_timeout and avoids spawning
    // unnecessary threads.
    if !startup.noninteractive {
        // This is an interactively displayed session, so its frames show the
        // menu / tab / tool bars, which occupy rows of the window text area.
        // Mark each frame and recompute its window geometry so windows (and
        // their tab / header lines) render below the chrome.  Batch sessions
        // skip this block, leaving `displays_chrome` false so their
        // `window-edges` stay GNU-batch-compatible (root at line 0).
        for frame in evaluator.frame_manager_mut().frames_mut() {
            frame.displays_chrome = true;
            frame.sync_window_area_bounds();
        }

        let (input_tx, input_rx) = crossbeam_channel::unbounded();
        let display_input_rx = emacs_comms.input_rx;
        let primary_window_size_for_input = Arc::clone(&primary_window_size);
        // Shared quit-request flag. When the bridge sees `C-g` it flips
        // this so the evaluator's `maybe_quit` can observe it without
        // waiting for `read_char` to drain the channel. Mirrors GNU's
        // synchronous keystroke path (`keyboard.c:3812` sets Vquit_flag
        // immediately); Rust can't longjmp into the evaluator, so we
        // poll an atomic instead.
        let quit_requested = Arc::clone(&evaluator.quit_requested);
        // Cross-platform wakeup (post-send): see the matching comment on the
        // other input-bridge path.
        let input_notifier = evaluator.wait_notifier();
        let secondary_input_tx = input_tx.clone();
        let secondary_input_notifier = input_notifier.clone();
        let secondary_quit_requested = Arc::clone(&quit_requested);
        std::thread::Builder::new()
            .name("input-bridge".to_string())
            .spawn(move || {
                while let Ok(event) = display_input_rx.recv() {
                    let should_log = input_bridge::should_log_display_event(&event);
                    if should_log {
                        tracing::debug!("input-bridge: received display event {:?}", event);
                    }
                    record_primary_window_resize(&primary_window_size_for_input, &event);
                    let mut queued_input = false;
                    let mut evaluator_disconnected = false;
                    for kb_event in input_bridge::convert_display_event(&event) {
                        if should_log {
                            tracing::debug!(
                                "input-bridge: converted display event {:?} to keyboard event {:?}",
                                event,
                                kb_event
                            );
                        }
                        if kb_event.requests_default_quit() {
                            quit_requested.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        if input_tx.send(kb_event).is_err() {
                            evaluator_disconnected = true;
                            break;
                        }
                        queued_input = true;
                    }
                    if evaluator_disconnected {
                        break; // Context dropped
                    }
                    if queued_input {
                        if let Some(notifier) = &input_notifier
                            && let Err(error) = notifier.notify()
                        {
                            tracing::error!(%error, "input bridge failed to wake evaluator");
                        }
                    }
                }
            })
            .expect("Failed to spawn input bridge thread");

        // 7. Connect evaluator to input system
        evaluator.init_input_system(input_rx);
        evaluator.set_tty_frame_host_factory(Box::new(secondary_tty::SecondaryTtyFactory::new(
            secondary_ttys.clone(),
            secondary_input_tx,
            secondary_input_notifier,
            secondary_quit_requested,
        )));
        install_diagnostics_eval_hooks(&mut evaluator);
    }

    // 8. Set up redisplay callback (layout engine + TTY RIF render).
    frame_layout::install_tty_redisplay_callback_with_popup_redraw(
        &mut evaluator,
        &startup,
        Some(tty_popup_force_full_redraw),
        Some(Box::new(move |eval| secondary_ttys.render_selected(eval))),
    );

    // Add undo boundary after startup so initial content isn't undoable
    if let Some(buf) = evaluator.buffer_manager_mut().current_buffer_mut() {
        let mut ul = buf.get_undo_list();
        neovm_core::buffer::undo_list_boundary(&mut ul);
        buf.set_undo_list(ul);
    }

    // 9. Enter GNU's outer command loop. This mirrors src/emacs.c, which
    //     enters recursive-edit and lets the outer command loop evaluate the
    //     `top-level` startup form before reading interactive input.
    neovm_core::emacs_core::load::maybe_run_after_pdump_load_hook(&mut evaluator);
    // R2-C3: native-from-call-1 — prepopulate the AOT preload before first dispatch.
    maybe_prepopulate_aot(mode, &evaluator);
    tracing::info!("Entering GNU command loop (recursive-edit)...");
    let exit_status = evaluator.recursive_edit();
    if exit_status.is_ok() {
        tracing::info!("Command loop exited normally");
    } else {
        tracing::warn!("Command loop exited with error");
    }

    // 11. Shutdown
    tracing::info!("Shutting down...");
    let _ = emacs_comms.cmd_tx.try_send(
        neomacs_display_runtime::thread_comm::RenderCommand::Lifecycle(LifecycleCommand::Shutdown),
    );
    frontend.join();
    if tty_init::should_enable_live_tty_io(&startup) {
        tty_init::tty_shutdown_terminal();
    }
    log_clean_process_exit(process_started_at, &process_args);

    // R2 increment C: persist this session's proven-hot JIT leaves before exit
    // (Context still alive on this eval thread; runs BEFORE the shutdown-request
    // early return so it fires on kill-emacs too). No-op unless NEOVM_AOT_PGO set.
    maybe_drain_aot_pgo(mode, &evaluator);

    if let Some(request) = evaluator.shutdown_request() {
        if request.restart {
            tracing::warn!("restart requested via kill-emacs, but restart is not implemented yet");
        }
        if request.exit_code != 0 {
            std::process::exit(request.exit_code);
        }
    }

    leak_evaluator_for_process_exit(evaluator);
}

fn leak_evaluator_for_process_exit(evaluator: Context) {
    // GNU Emacs does not walk the Lisp heap and free every object on normal
    // process exit; the OS reclaims the image.  Keep Rust from doing a deep
    // `Context` destructor pass after the command loop has already shut down.
    std::mem::forget(evaluator);
}

fn log_clean_process_exit(started_at: Instant, args: &[OsString]) {
    tracing::info!(
        duration_ms = %started_at.elapsed().as_millis(),
        command_args = ?args,
        "Neomacs exited cleanly"
    );
}

// ---------------------------------------------------------------------------
// Bootstrap dump mode
// ---------------------------------------------------------------------------

fn run_temacs_dump_mode(dump_mode: LoadupDumpMode, startup: &StartupOptions) {
    // Logging is already initialized by `run()` before this function is
    // called; calling `init()` again here is redundant (it would be a
    // no-op anyway because the global subscriber is set once).
    tracing::info!(
        "{} {} starting raw loadup dump (dump-mode={}, pid={})",
        RuntimeMode::Raw.binary_name(),
        neomacs_display_runtime::VERSION,
        dump_mode.as_gnu_string(),
        std::process::id()
    );

    let invocation = raw_dump_loadup_invocation(startup, dump_mode);
    let eval = neovm_core::emacs_core::load::create_bootstrap_evaluator_for_loadup(
        BOOTSTRAP_CORE_FEATURES,
        &invocation,
    )
    .expect("temacs bootstrap dump should succeed");

    if let Some(request) = eval.shutdown_request()
        && request.exit_code != 0
    {
        std::process::exit(request.exit_code);
    }
}

#[allow(dead_code)]
fn main() {
    neomacs_display_runtime::macos_bundle_runtime::configure_before_threads();

    // Before the evaluator can build an image, mark this as a shipped editor.
    // It must do what GNU does about bytecode older than its source -- name the
    // file and start anyway (`src/lread.c:1379`) -- rather than refuse, which is
    // what every OTHER process linking neovm-core now does by default.
    //
    // The default is inverted deliberately (ledger 206).  Ledger 202 asked
    // `cfg!(test)`, a fact about a compilation unit, so the refusal was live
    // for neovm-core's own 482 in-process tests and dark for the 62 in this
    // crate and the 13 in neomacs-layout-engine, which link neovm-core as an
    // ordinary dependency.  Asking the PROCESS instead means a test binary in
    // any crate is covered without opting in, and only the editor opts out.
    //
    // `bootstrap-neomacs` and `neomacs-temacs` are byte copies of this binary
    // and so run this line too -- they must, because `fresh-build` drives them
    // across a tree whose `.elc` are mid-recompile.
    neovm_core::emacs_core::load::announce_shipped_editor_process();
    run(runtime_mode_from_argv(std::env::args()));
}

// ---------------------------------------------------------------------------
// Bootstrap helpers
// ---------------------------------------------------------------------------

struct BootstrapResult {
    #[allow(dead_code)]
    scratch_id: BufferId,
    #[allow(dead_code)]
    minibuf_id: BufferId,
}

#[derive(Clone, Copy, Debug)]
struct BootstrapFrameMetrics {
    char_width: f32,
    char_height: f32,
    font_pixel_size: f32,
}

fn font_weight_symbol(weight: FontWeight) -> &'static str {
    weight.symbol_name()
}

fn startup_font_weight_symbol(weight: FontWeight) -> &'static str {
    match weight {
        FontWeight::Normal => "regular",
        _ => font_weight_symbol(weight),
    }
}

fn font_otf_capability_for_file(
    file: &str,
    face_index: u32,
) -> Option<neovm_core::emacs_core::eval::FontOtfCapability> {
    neomacs_layout_engine::font::probe::otf_capability(file, face_index)
        .map(core_font_otf_capability)
}

fn font_otf_capability_for_asset(
    asset: &neomacs_display_protocol::font::FontOutlineAsset,
) -> Option<neovm_core::emacs_core::eval::FontOtfCapability> {
    match asset {
        neomacs_display_protocol::font::FontOutlineAsset::File(file) => {
            font_otf_capability_for_file(file.path(), file.face_index())
        }
        neomacs_display_protocol::font::FontOutlineAsset::Memory(memory) => {
            neomacs_layout_engine::font::probe::otf_capability_from_bytes(
                memory.bytes(),
                memory.face_index(),
            )
            .map(core_font_otf_capability)
        }
    }
}

fn core_font_otf_capability(
    caps: neomacs_layout_engine::font::probe::OtfCapability,
) -> neovm_core::emacs_core::eval::FontOtfCapability {
    let side = |scripts: Vec<neomacs_layout_engine::font::probe::OtfScript>| {
        scripts
            .into_iter()
            .map(|script| {
                (
                    script.tag,
                    script
                        .lang_syses
                        .into_iter()
                        .map(|lang| (lang.tag, lang.features))
                        .collect(),
                )
            })
            .collect()
    };
    neovm_core::emacs_core::eval::FontOtfCapability {
        gsub: side(caps.gsub),
        gpos: side(caps.gpos),
    }
}

fn core_font_px_metrics(
    metrics: neomacs_layout_engine::font::probe::FontPxMetrics,
) -> neovm_core::emacs_core::eval::FontPxProbeResult {
    neovm_core::emacs_core::eval::FontPxProbeResult {
        pixel_size: metrics.pixel_size,
        height: metrics.height,
        ascent: metrics.ascent,
        descent: metrics.descent,
        max_width: metrics.max_width,
        space_width: metrics.space_width,
        average_width: metrics.average_width,
    }
}

/// Cross the layout/core boundary for one exact host-selected font.
///
/// Keeping this projection in one place makes the Lisp font object, frame
/// geometry, glyph lookup, and OTF capability describe the same realization.
fn core_opened_font_from_selection(
    font: SelectedFontInfo,
    mut capability_for_file: impl FnMut(&str, u32) -> Option<FontOtfCapability>,
) -> ResolvedOpenedFont {
    let identity = &font.resolved.identity;
    let capability = identity
        .file_path
        .as_deref()
        .and_then(|file| capability_for_file(file, identity.file_face_index()));
    ResolvedOpenedFont {
        resolved: font.resolved,
        foundry: font.foundry.as_deref().map(LispString::from_utf8),
        slant: font.slant,
        metrics: core_font_px_metrics(font.metrics),
        capability,
    }
}

fn bootstrap_default_font_parameter(font_pixel_size: f32) -> Value {
    let mut metrics_svc = FontMetricsService::new();
    let selected = metrics_svc.select_font_for_char('M', "Monospace", 400, false, font_pixel_size);
    let mut face = neovm_core::face::Face::new("default");
    face.height = Some(FaceHeight::Absolute(100));

    let Some(font) = selected else {
        // An unresolved selector is not an opened font.  Keep the public
        // bootstrap name until the display host can publish an exact object.
        return bootstrap_default_font_name(font_pixel_size);
    };
    let matched = ResolvedFontMatch {
        glyph_code: None,
        font: core_opened_font_from_selection(font, font_otf_capability_for_file),
    };
    neovm_core::emacs_core::font::opened_font_from_resolved_match(&face, &matched)
}

fn bootstrap_default_font_name(font_pixel_size: f32) -> Value {
    let mut metrics_svc = FontMetricsService::new();
    let selected = metrics_svc.select_font_for_char('M', "Monospace", 400, false, font_pixel_size);
    let rounded_pixel_size = font_pixel_size.max(1.0).round() as i64;

    let family = selected
        .as_ref()
        .map(|font| font.resolved.family.as_str())
        .unwrap_or("Monospace");
    let weight = selected
        .as_ref()
        .map(|font| startup_font_weight_symbol(FontWeight::from_css_weight(font.resolved.weight)))
        .unwrap_or("regular");
    let slant = selected
        .as_ref()
        .map(|font| font.slant.symbol_name())
        .unwrap_or("normal");

    Value::string(format!(
        "-*-{family}-{weight}-{slant}-*-*-{rounded_pixel_size}-*-*-*-*-*-*-*"
    ))
}

fn bootstrap_frame_metrics() -> BootstrapFrameMetrics {
    bootstrap_frame_metrics_for_font_sizing(FontSizing::native_gui())
}

fn bootstrap_frame_metrics_for_font_sizing(font_sizing: FontSizing) -> BootstrapFrameMetrics {
    let font_pixel_size = font_sizing.face_height_to_layout_pixels(100);
    let mut metrics_svc = FontMetricsService::new();
    let metrics = metrics_svc.font_metrics("Monospace", 400, false, font_pixel_size);
    BootstrapFrameMetrics {
        char_width: metrics.char_width.max(1.0),
        char_height: metrics.line_height.max(1.0),
        font_pixel_size,
    }
}

fn bootstrap_frame_metrics_for_frontend(frontend: FrontendKind) -> BootstrapFrameMetrics {
    if frontend == FrontendKind::Tty {
        BootstrapFrameMetrics {
            char_width: 1.0,
            char_height: 1.0,
            font_pixel_size: 16.0,
        }
    } else {
        bootstrap_frame_metrics()
    }
}

fn bootstrap_frame_metrics_for_display(display: BootstrapDisplayConfig) -> BootstrapFrameMetrics {
    if display.frontend() == FrontendKind::Tty {
        bootstrap_frame_metrics_for_frontend(FrontendKind::Tty)
    } else {
        bootstrap_frame_metrics_for_font_sizing(display.font_sizing())
    }
}

fn bootstrap_buffers(
    eval: &mut Context,
    width: u32,
    height: u32,
    display: BootstrapDisplayConfig,
) -> BootstrapResult {
    let frame_metrics = bootstrap_frame_metrics_for_display(display);
    let gui_display_identity =
        (display.frontend() == FrontendKind::Gui).then(host_gui_display_identity);
    let find_or_create_buffer = |eval: &mut Context, name: &str| {
        eval.buffer_manager()
            .find_buffer_by_name(name)
            .unwrap_or_else(|| eval.buffer_manager_mut().create_buffer(name))
    };

    // Reuse GNU startup buffers instead of creating duplicate names on top of
    // cached bootstrap state.
    let scratch_id = find_or_create_buffer(eval, "*scratch*");
    let _ = eval
        .buffer_manager_mut()
        .clear_buffer_labeled_restrictions(scratch_id);
    if let Some(buf) = eval.buffer_manager_mut().get_mut(scratch_id) {
        buf.widen();
        // Don't insert scratch content here. GNU Emacs populates
        // *scratch* from startup.el:2948 via
        //   (insert (substitute-command-keys initial-scratch-message))
        // which handles \\[...] key-binding expansion and backtick →
        // curly-quote conversion via text-quoting-style. Hardcoding
        // the content in Rust bypassed both of those, producing bare
        // "C-x C-f" instead of quoted "'C-x C-f'".
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    // Set *scratch* as the current buffer
    eval.buffer_manager_mut().set_current(scratch_id);

    let mini_id = find_or_create_buffer(eval, " *Minibuf-0*");
    let _ = eval
        .buffer_manager_mut()
        .clear_buffer_labeled_restrictions(mini_id);
    let _ = eval
        .buffer_manager_mut()
        .configure_buffer_undo_list(mini_id, Value::NIL);
    if let Some(buf) = eval.buffer_manager_mut().get_mut(mini_id) {
        buf.widen();
        buf.goto_emacs_byte_pos(EmacsBytePos::new(0));
    }

    let msg_id = find_or_create_buffer(eval, "*Messages*");
    let _ = eval
        .buffer_manager_mut()
        .clear_buffer_labeled_restrictions(msg_id);
    if let Some(buf) = eval.buffer_manager_mut().get_mut(msg_id) {
        buf.widen();
        let len = buf.total_emacs_byte_len().get();
        if len > 0 {
            buf.delete_emacs_byte_range(EmacsByteRange::new(
                EmacsBytePos::ZERO,
                EmacsBytePos::new(len),
            ));
        }
        buf.goto_emacs_byte_pos(EmacsBytePos::new(0));
    }
    let _ = eval.buffer_manager_mut().note_buffer_order_tail(msg_id);

    let frame_id = {
        let frame_manager = eval.frame_manager();
        let selected = frame_manager.selected_frame().map(|frame| frame.id);
        let should_reuse_existing = selected.is_some() && frame_manager.frame_list().len() == 1;
        (selected, should_reuse_existing)
    };
    let frame_id = if frame_id.1 {
        let frame_id = frame_id.0.expect("selected startup frame");
        tracing::info!(
            "Reusing existing startup frame {:?} as bootstrap frame ({}x{})",
            frame_id,
            width,
            height
        );
        frame_id
    } else {
        let frame_id = eval
            .frame_manager_mut()
            .create_frame("F1", width, height, scratch_id);
        tracing::info!(
            "Created frame {:?} ({}x{}) with *scratch*={:?}",
            frame_id,
            width,
            height,
            scratch_id
        );
        frame_id
    };
    let _ = eval.frame_manager_mut().select_frame(frame_id);

    // GNU's startup selects `*scratch*' in the initial window and
    // `record_buffer' (src/buffer.c) puts it at the front of the frame's
    // `buffer_list', so `(frame-parameter nil 'buffer-list)' returns
    // `("*scratch*")' immediately after startup.  The frame's `buffer_list' is
    // not serialized in the pdump, so seed it here -- the runtime point where
    // the initial frame is established showing `*scratch*' -- mirroring GNU's
    // early `record_buffer'.  No `buffer-list-update-hook' runs (it predates
    // any user hook registration, as in GNU).
    if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id) {
        if !frame.buffer_list.contains(&scratch_id) {
            frame.buffer_list.retain(|bid| *bid != scratch_id);
            frame.buffer_list.insert(0, scratch_id);
        }
        frame.buried_buffer_list.retain(|bid| *bid != scratch_id);
    }

    // Seed frame parameters so GNU Lisp startup sees the correct host surface.
    //
    // Use the authoritative `startup.noninteractive` flag, NOT the obarray
    // `noninteractive` value: the latter is only seeded later (see below, where
    // we `set_variable("noninteractive", ...)`), so at this point it still
    // holds the stale value baked into the batch-built pdump (t). Reading it
    // here marked the interactive TTY frame as the initial frame, which made
    // `frame-initial-p' return t and `debug' (debug.el) take its
    // non-interactive `message'-only branch instead of displaying *Backtrace*.
    let initial_tty_frame =
        display.frontend() == FrontendKind::Tty && display.interactivity.is_batch();
    // Preserve the host-selected bootstrap font across GNU face
    // finalization.  That Lisp pass may update live frame font state while it
    // computes specifications, but the opening host frame's font and geometry
    // remain the startup policy inputs until normal user configuration runs.
    let (bootstrap_font, bootstrap_font_name) = if display.frontend() == FrontendKind::Tty {
        (Value::NIL, Value::string("fixed"))
    } else {
        (
            bootstrap_default_font_parameter(frame_metrics.font_pixel_size),
            bootstrap_default_font_name(frame_metrics.font_pixel_size),
        )
    };
    let bootstrap_font_snapshot = bootstrap_font
        .as_vector_data()
        .map(|items| Value::vector(items.to_vec()))
        .unwrap_or(bootstrap_font);
    let bootstrap_root_scope = neovm_core::emacs_core::eval::save_scratch_gc_roots();
    neovm_core::emacs_core::eval::push_scratch_gc_root(bootstrap_font_snapshot);
    neovm_core::emacs_core::eval::push_scratch_gc_root(bootstrap_font_name);
    if display.frontend() == FrontendKind::Tty {
        // GNU `make_initial_frame` resets its dedicated `tty_frame_count` and
        // assigns F1.  The reused pdump surrogate's internal FrameId is not a
        // presentation ordinal.
        let assigned = eval
            .frame_manager_mut()
            .assign_initial_tty_frame_name(frame_id);
        debug_assert!(assigned, "selected startup frame must remain live");
    }
    if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id) {
        // Font parameter resolution creates a FontMetricsService which
        // scans the system font database (~500ms). Skip for TTY where
        // font parameters are unused — TTY uses 1x1 character cells.
        // Reused startup frames must be normalized back to GNU's initial-frame
        // surface: generated name (e.g. "F1"), nil title, nil icon-name.
        if display.frontend() != FrontendKind::Tty {
            // This is likewise the one initial GUI frame, whose existing
            // bootstrap contract is F1. Terminal F<n> allocation is owned by
            // FrameManager's GNU-shaped tty_frame_count analogue above.
            frame.set_generated_name_value(Value::string("F1"));
        }
        frame.clear_title();
        frame.icon_name = Value::NIL;
        frame.initial = initial_tty_frame;
        frame.width = width;
        frame.height = height;
        frame.visibility = FrameVisibility::Visible;
        if let Some(window_system) = display.window_system_symbol() {
            frame.set_window_system(Some(Value::symbol(window_system)));
            frame.install_gnu_gui_default_parameters();
        } else {
            frame.set_window_system(None);
        }
        if display.frontend() == FrontendKind::Gui {
            frame.set_display_identity(gui_display_identity.clone().unwrap_or_default());
            frame.set_parameter(
                Value::symbol("display-type"),
                Value::symbol(display.display_type_symbol()),
            );
            frame.set_parameter(
                Value::symbol("background-mode"),
                Value::symbol(display.background_mode),
            );
        } else {
            frame.set_display_identity(FrameDisplayIdentity::default());
            frame.remove_parameter(Value::symbol("display-type"));
            frame.remove_parameter(Value::symbol("background-mode"));
        }
        frame.set_known_parameter(FrameParam::Font, bootstrap_font_name);
        frame.set_parameter(Value::symbol("font-parameter"), bootstrap_font);
        // GNU frame.c: initial frame title is NULL (unset). The %F
        // mode-line construct falls through to frame->name ("F1") when
        // title is unset. Don't set a title here — let %F show the
        // frame name, matching GNU behaviour.

        frame.font_pixel_size = frame_metrics.font_pixel_size;
        if display.frontend() == FrontendKind::Tty {
            // TTY frames use 1x1 character cell metrics
            // (GNU Emacs frame.c:1184-1185: column_width=1, line_height=1).
            frame.char_width = 1.0;
            frame.char_height = 1.0;
            // The minibuffer was created with a pixel height (16.0) in Frame::new.
            // For TTY, resize it to 1 row (char_height=1.0) before sync.
            if let Some(mini) = frame.minibuffer_leaf.as_mut() {
                let b = *mini.bounds();
                mini.set_bounds(neovm_core::window::Rect::new(b.x, b.y, b.width, 1.0));
            }
        } else {
            frame.char_width = frame_metrics.char_width;
            frame.char_height = frame_metrics.char_height;
        }
        frame.sync_tab_bar_height_from_parameters();
        // Match GNU `frame.c:1307-1309` (TTY frame init):
        //   FRAME_MENU_BAR_LINES (f) = NILP (Vmenu_bar_mode) ? 0 : 1;
        // On TTY frames neomacs has no per-frame default-frame-alist
        // bridge yet, so seed the parameter directly here when the
        // frontend is TTY before calling `sync_menu_bar_height_from_parameters`.
        // The GUI path has its own menu bar pipeline (see
        // `neomacs-display-runtime`) and never goes through this code,
        // so we only need to set the parameter for `FrontendKind::Tty`.
        if display.frontend() == FrontendKind::Tty {
            frame.set_parameter(
                FrameParam::MenuBarLines.symbol(),
                neovm_core::emacs_core::Value::fixnum(1),
            );
        }
        frame.sync_menu_bar_height_from_parameters();
        frame.sync_tool_bar_height_from_parameters();
        if let Window::Leaf {
            buffer_id,
            window_start,
            point,
            ..
        } = &mut frame.root_window
        {
            *buffer_id = scratch_id;
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
        }
    }
    eval.create_window_markers_for_root(frame_id, scratch_id);
    if display.frontend() == FrontendKind::Gui {
        initialize_reused_gui_startup_frame(eval, frame_id);
    } else {
        eval.set_face_attribute(
            "default",
            LFaceAttr::Foreground,
            neovm_core::face::FaceAttrValue::Unspecified,
        );
        eval.set_face_attribute(
            "default",
            LFaceAttr::Background,
            neovm_core::face::FaceAttrValue::Unspecified,
        );
    }

    if display.window_system_symbol().is_some() {
        if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id) {
            frame.set_known_parameter(FrameParam::Font, bootstrap_font_name);
            frame.set_parameter(Value::symbol("font-parameter"), bootstrap_font_snapshot);
            frame.font_pixel_size = frame_metrics.font_pixel_size;
            frame.char_width = frame_metrics.char_width;
            frame.char_height = frame_metrics.char_height;
        }
        neovm_core::emacs_core::font::seed_live_frame_default_face_from_font_parameter(
            eval, frame_id,
        );
        // Bootstrap callers inspect the default face and establish initial
        // geometry before the normal redisplay callback exists.  Materialize
        // the newly seeded specification once for those consumers.
        eval.sync_runtime_faces_for_frame(frame_id);
    }
    neovm_core::emacs_core::eval::restore_scratch_gc_roots(bootstrap_root_scope);

    // Fix window geometry: root window takes frame height minus minibuffer.
    if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id) {
        let mini_h = frame.char_height.max(1.0);
        let mini_y = height as f32 - mini_h;
        if let Window::Leaf { bounds, .. } = &mut frame.root_window {
            bounds.height = mini_y;
        }
        if let Some(mini_leaf) = &mut frame.minibuffer_leaf
            && let Window::Leaf {
                buffer_id,
                window_start,
                point,
                bounds,
                ..
            } = mini_leaf
        {
            *buffer_id = mini_id;
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
            bounds.y = mini_y;
            bounds.height = mini_h;
            bounds.width = width as f32;
        }
    }
    eval.create_window_markers_for_minibuffer(frame_id, mini_id);

    BootstrapResult {
        scratch_id,
        minibuf_id: mini_id,
    }
}

fn configure_gnu_startup_state(eval: &mut Context, frame_id: FrameId, startup: &StartupOptions) {
    // Doom and similar configs deliberately raise `gc-cons-threshold` during
    // startup. Keep the measured arena-fragmentation ceiling active through
    // GNU's complete `normal-top-level`; startup.el clears this private flag
    // after the final startup/window hooks and a bounded settling window.
    //
    // INTERACTIVE SESSIONS ONLY. In a noninteractive (`--batch`) session the
    // user's whole script runs INSIDE `normal-top-level` (`command-line`
    // processes `-l`/`--eval`) and the settling timer never fires (no command
    // loop), so the 4 MB ceiling silently overrode `gc-cons-threshold` for the
    // entire run — 45 collections per 64 MB of consing at ANY setting, where
    // GNU (which has no ceiling) runs none; byte-compile drivers and every
    // batch benchmark paid it. GNU semantics rule in batch: the user's
    // threshold is the threshold.
    eval.set_variable(
        "neomacs--startup-gc-ceiling-active",
        if startup.noninteractive {
            Value::NIL
        } else {
            Value::T
        },
    );
    let argv_strings = startup.forwarded_args.to_vec();
    let argv = argv_strings
        .iter()
        .cloned()
        .map(Value::string)
        .collect::<Vec<_>>();
    let argv_left = argv_strings
        .iter()
        .skip(1)
        .cloned()
        .map(Value::string)
        .collect::<Vec<_>>();
    let invocation_directory = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("/"));
    let invocation_name = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "neomacs".to_string());
    let invocation_directory = ensure_dir_string(&invocation_directory);

    eval.set_variable("command-line-args", Value::list(argv));
    eval.set_variable("command-line-args-left", Value::list(argv_left));
    eval.set_variable("command-line-processed", Value::NIL);
    eval.set_variable(
        "noninteractive",
        if startup.noninteractive {
            Value::T
        } else {
            Value::NIL
        },
    );
    if startup.noninteractive {
        // GNU emacs.c raises this after initialization for batch jobs so
        // short-lived noninteractive commands spend less time in GC.
        eval.set_variable("gc-cons-percentage", Value::make_float(1.0));
    } else {
        // GNU `syms_of_undo' (src/undo.c:459-474) gives `undo-outer-limit' a
        // 24MB default and `--batch' clears it (src/emacs.c:1700-1707). A bare
        // evaluator is a batch evaluator, so neovm-core defaults it to nil and
        // the 24MB last-ditch limit is installed here for real sessions.
        eval.set_variable("undo-outer-limit", Value::fixnum(24_000_000));
    }
    // Mirror GNU's C-side `no_site_lisp` / `build_details` globals as
    // Lisp variables. GNU itself does not expose them as Lisp vars (the
    // load-path / version code reads the C globals directly), but
    // surfacing them here lets oracle tests verify the parsed value
    // and lets future load-path or version code observe the choice
    // without re-walking argv. Defaults match GNU: no_site_lisp=false
    // means site-lisp is included; build-details=t means build-time
    // strings are populated.
    eval.set_variable(
        "no-site-lisp",
        if startup.no_site_lisp {
            Value::T
        } else {
            Value::NIL
        },
    );
    eval.set_variable(
        "build-details",
        if startup.no_build_details {
            Value::NIL
        } else {
            Value::T
        },
    );
    let (terminal_frame, frame_initial_frame, default_minibuffer_frame) = match startup.frontend {
        FrontendKind::Gui => {
            let terminal_frame_id = ensure_gnu_startup_terminal_frame(eval, frame_id);
            let window_system = Value::symbol(gui_window_system_symbol());
            eval.set_variable("window-system", window_system);
            eval.set_variable("initial-window-system", window_system);
            eval.set_variable(
                "frame-initial-frame-alist",
                opening_frame_initial_alist(eval, window_system),
            );
            (
                Value::make_frame(terminal_frame_id.0),
                Value::make_frame(frame_id.0),
                Value::make_frame(frame_id.0),
            )
        }
        FrontendKind::Tty => {
            eval.set_variable("window-system", Value::NIL);
            eval.set_variable("initial-window-system", Value::NIL);
            if tty_init::should_enable_live_tty_io(startup) {
                seed_live_tty_frame_parameters(eval, frame_id, startup);
            }
            (Value::make_frame(frame_id.0), Value::NIL, Value::NIL)
        }
    };
    eval.set_variable("invocation-name", Value::string(invocation_name));
    eval.set_variable(
        "invocation-directory",
        Value::unibyte_string(invocation_directory),
    );
    let cwd = std::env::current_dir()
        .map(|p| ensure_dir_string(&p))
        .unwrap_or_else(|_| "/".to_string());
    eval.set_variable("default-directory", Value::unibyte_string(cwd));
    if let Err(error) =
        eval.eval_str("(setq default-directory (abbreviate-file-name default-directory))")
    {
        tracing::warn!(?error, "failed to abbreviate startup default-directory");
    }
    if let Err(error) = eval.eval_str("(set-window-buffer (selected-window) (current-buffer))") {
        tracing::warn!(?error, "failed to record initially displayed buffer");
    }
    eval.set_variable("terminal-frame", terminal_frame);
    eval.set_variable("frame-initial-frame", frame_initial_frame);
    eval.set_variable("default-minibuffer-frame", default_minibuffer_frame);
    // COMPUTE the initial frame's display-derived parameters, and realize its
    // faces from their `defface` specs -- GNU's two calls, in GNU's order.
    //
    // `tty-set-up-initial-frame-faces` (lisp/faces.el:2409-2416) is
    //     (frame-set-background-mode frame t) (face-set-after-frame-default frame)
    // and C reaches it from `init_faces_initial` (src/dispnew.c:7178), called
    // by `init_display` (src/dispnew.c:7413-7422) after the pdump is loaded.
    // `x-create-frame-with-faces` (lisp/faces.el:2242-2243) makes the SAME two
    // calls for a window-system frame, so one pair serves both frontends here.
    //
    // `frame-set-background-mode` (lisp/frame.el:1526) is what DERIVES
    // `background-mode' and `display-type'; nothing in Rust may seed them,
    // because GNU's `make_initial_frame' (src/frame.c:1423) does not and our
    // `faces.el' load must not be able to see them (DIVERGENCES.md 157).
    //
    // This must run here — after `bootstrap_buffers` has finalized the frame —
    // because a realization done earlier does not survive the frame
    // re-bootstrap. Without it the frame-local face vectors are never populated
    // from the specs, so stock faces fall through to their bootstrap `Color`
    // defaults: `(face-attribute 'elisp-condition :foreground (selected-frame))`
    // returned the realized hex "#ff0000" instead of GNU's literal spec value
    // "red", `tab-bar` background "grey50" instead of "grey", and `:foreground
    // reset` faces reported `unspecified`. `face-set-after-frame-default` stores
    // each spec value verbatim (color parsing happens only at display time),
    // matching GNU's lface storage.
    if let Err(error) = eval.eval_str(
        "(progn
           (when (fboundp 'frame-set-background-mode)
             (frame-set-background-mode (selected-frame) t))
           (when (fboundp 'face-set-after-frame-default)
             (face-set-after-frame-default (selected-frame))))",
    ) {
        tracing::warn!(
            ?error,
            "failed to realize initial-frame faces from defface specs"
        );
    }
    // Skip the splash screen — its fill-region is extremely slow through
    // with_mirrored_evaluator.  Users who want it can set this to nil in
    // their init file.
    eval.set_variable("inhibit-startup-screen", Value::T);
}

fn seed_live_tty_frame_parameters(eval: &mut Context, frame_id: FrameId, startup: &StartupOptions) {
    let tty_name = tty_init::detect_tty_name(startup);
    let tty_type = tty_init::detect_tty_type();
    if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id) {
        frame.set_parameter(Value::symbol("tty"), Value::string(tty_name));
        if let Some(tty_type) = tty_type {
            frame.set_parameter(Value::symbol("tty-type"), Value::string(tty_type));
        } else {
            frame.remove_parameter(Value::symbol("tty-type"));
        }
    }
    // NEITHER `background-mode' slot is written here.
    //
    // The FRAME parameter is DERIVED, by `frame-set-background-mode'
    // (lisp/frame.el:1526); seeding it directly is DIVERGENCES.md 157's bug.
    // Entry 157 moved the detected value onto GNU's own channel instead -- the
    // TERMINAL parameter that `frame-terminal-default-bg-mode'
    // (lisp/frame.el:1588-1599) reads -- and recorded that the HEURISTIC still
    // diverged, for a later entry.  DIVERGENCES.md 174 is that entry, and the
    // answer is that the value must not be invented at all.
    //
    // `frame-terminal-default-bg-mode' is the FIRST clause of the `cond' in
    // `frame--current-background-mode' (lisp/frame.el:1505-1524), so a non-nil
    // terminal parameter WINS over the background colour and the tty type --
    // and it wins permanently, which is why a light theme could never move the
    // mode.  GNU's only writer of that slot is `xterm--set-background-mode'
    // (lisp/term/xterm.el:1309-1316), from a real OSC-11 reply; `COLORFGBG'
    // appears nowhere in GNU.  With no reply the slot stays nil and GNU derives
    // the mode from the colour, defaulting to `light' for a tty type matching
    // "^\(xterm\|rxvt\|dtterm\|eterm\)" and `dark' otherwise.
    //
    // `display-type' needs no input at all: `frame-set-background-mode'
    // computes it as `color' iff `(tty-display-color-p frame)', which is t for
    // a live colour terminal.
}

fn ensure_gnu_startup_terminal_frame(eval: &mut Context, opening_frame_id: FrameId) -> FrameId {
    const GUI_STARTUP_TERMINAL_ID: u64 = 1;

    if let Some(existing) = eval
        .frame_manager()
        .frame_list()
        .into_iter()
        .find(|candidate| {
            *candidate != opening_frame_id
                && eval.frame_manager().get(*candidate).is_some_and(|frame| {
                    frame.visibility.is_invisible() && frame.effective_window_system().is_none()
                })
        })
    {
        return existing;
    }

    let seed_buffer_id = if let Some(id) = eval.buffer_manager().current_buffer_id() {
        id
    } else if let Some(id) = eval.buffer_manager().find_buffer_by_name("*scratch*") {
        id
    } else {
        eval.buffer_manager_mut().create_buffer("*scratch*")
    };
    let (width, height, environment) = eval
        .frame_manager()
        .get(opening_frame_id)
        .map(|frame| {
            (
                frame.width.max(1),
                frame.height.max(1),
                frame.parameter("environment"),
            )
        })
        .unwrap_or((80, 25, None));
    ensure_terminal_runtime_owner(
        GUI_STARTUP_TERMINAL_ID,
        "startup_terminal",
        TerminalRuntimeConfig::inactive(),
    );
    let terminal_frame_id = eval.frame_manager_mut().create_frame_on_terminal(
        "Fstartup-tty",
        GUI_STARTUP_TERMINAL_ID,
        width,
        height,
        seed_buffer_id,
    );
    if let Some(frame) = eval.frame_manager_mut().get_mut(terminal_frame_id) {
        frame.visibility = FrameVisibility::Invisible;
        frame.set_window_system(None);
        // Keep display-type and background-mode so defface spec
        // conditions like (class color) (background dark) match
        // during early face resolution — critical for font-lock
        // and org-mode face colours.
        frame.set_parameter(Value::symbol("display-type"), Value::symbol("color"));
        frame.set_parameter(
            Value::symbol("background-mode"),
            Value::symbol(tty_init::detect_tty_background_mode()),
        );
        if let Some(environment) = environment {
            frame.set_parameter(Value::symbol("environment"), environment);
        }
    }
    terminal_frame_id
}

fn opening_frame_initial_alist(eval: &Context, window_system: Value) -> Value {
    let mut params = vec![Value::cons(Value::symbol("window-system"), window_system)];
    for symbol_name in ["initial-frame-alist", "default-frame-alist"] {
        if let Some(value) = eval.obarray().symbol_value(symbol_name)
            && let Some(items) = neovm_core::emacs_core::value::list_to_vec(value)
        {
            params.extend(items);
        }
    }
    Value::list(params)
}

#[cfg(test)]
fn run_gnu_startup(eval: &mut Context) {
    increase_stack_limit();
    stacker::grow(64 * 1024 * 1024, || run_gnu_startup_inner(eval));
}

#[cfg(test)]
fn run_gnu_startup_inner(eval: &mut Context) {
    eval.setup_thread_locals();
    maybe_install_startup_phase_trace(eval);
    eval.eval_str(
        r#"
        (progn
          (defun neomacs--test-exit-startup-recursive-edit ()
            (remove-hook 'window-setup-hook
                         #'neomacs--test-exit-startup-recursive-edit)
            (kill-emacs 0))
          (add-hook 'window-setup-hook
                    #'neomacs--test-exit-startup-recursive-edit))
        "#,
    )
    .expect("startup exit helper should install");
    let top_level = eval.obarray().symbol_value("top-level").cloned();
    tracing::info!("top-level variable before startup: {:?}", top_level);

    let (_tx, rx) = crossbeam_channel::unbounded();

    eval.init_input_system(rx);

    let result = eval.recursive_edit();

    if let Err(other) = result {
        let last_phase = eval
            .obarray()
            .symbol_value("neomacs--startup-last-phase")
            .cloned()
            .map(|value| print_value_with_eval(eval, &value));
        let last_call = eval
            .obarray()
            .symbol_value("neomacs--startup-last-call")
            .cloned()
            .map(|value| print_value_with_eval(eval, &value));
        panic!(
            "GNU startup via recursive_edit failed: {other} last-phase={last_phase:?} last-call={last_call:?}"
        );
    }
}

fn maybe_install_startup_phase_trace(eval: &mut Context) {
    if std::env::var("NEOMACS_TRACE_STARTUP_PHASES").unwrap_or_default() != "1" {
        return;
    }
    let trace_dir = Path::new("tmp");
    if let Err(err) = std::fs::create_dir_all(trace_dir) {
        tracing::warn!("startup trace directory creation failed: {err}");
        return;
    }
    let source = r##"
        (progn
          (defvar neomacs--startup-last-phase nil)
          (defvar neomacs--startup-last-call nil)
          (defvar neomacs--startup-trace-active nil)
          (with-temp-buffer
            (write-region (point-min) (point-max)
                          "./tmp/neomacs-startup-phases.trace" nil 'silent))
          (defun neomacs--startup-trace-around (name orig &rest args)
            (if neomacs--startup-trace-active
                (apply orig args)
              (let ((neomacs--startup-trace-active t))
                (setq neomacs--startup-last-phase name)
                (setq neomacs--startup-last-call (cons name args))
                (with-temp-buffer
                  (insert (format "enter %S %S\n" name args))
                  (append-to-file (point-min) (point-max)
                                  "./tmp/neomacs-startup-phases.trace"))
                (prog1
                    (apply orig args)
                  (with-temp-buffer
                    (insert (format "leave %S\n" name))
                    (append-to-file (point-min) (point-max)
                                    "./tmp/neomacs-startup-phases.trace"))))))
          (dolist (fn '(set-locale-environment
                        handle-args-function
                        x-handle-args
                        x-open-connection
                        create-default-fontset
                        create-fontset-from-fontset-spec
                        create-fontset-from-x-resource
                        neomacs--setup-cursor-blink
                        neomacs--setup-animations
                        pixel-scroll-precision-mode
                        frame-initialize
                        startup--setup-quote-display
                        normal-erase-is-backspace-setup-frame
                        tty-register-default-colors
                        startup--load-user-init-file
                        custom-reevaluate-setting
                        tty-run-terminal-initialization
                        display-startup-echo-area-message
                        command-line-1
                        display-startup-screen
                        frame-notice-user-settings))
            (when (fboundp fn)
              (advice-add fn :around
                          (eval `(lambda (orig &rest args)
                                   (apply #'neomacs--startup-trace-around
                                          ',fn orig args)))))))
    "##;
    if let Err(err) = eval.eval_str(source) {
        tracing::warn!("startup trace helper install failed: {err:?}");
    }
}

fn ensure_dir_string(path: &Path) -> String {
    let mut dir = path.to_string_lossy().to_string();
    if !dir.ends_with('/') {
        dir.push('/');
    }
    dir
}

fn publish_gui_frame(
    evaluator: &mut Context,
    frame_tx: &crossbeam_channel::Sender<neomacs_display_protocol::SealedFramePresentation>,
    render_waker: Option<&GuiEventLoopWaker>,
) {
    evaluator.setup_thread_locals();
    sync_selected_gui_chrome_state(evaluator);
    if !throw_on_input_active(evaluator) {
        // Title formatting may evaluate Lisp mode-line forms too.
        sync_live_gui_frame_titles(evaluator);
    }

    let result = frame_layout::publish_visible_frames(evaluator, |display_state| {
        match frame_tx.try_send(display_state) {
            Ok(()) => true,
            Err(error) => {
                tracing::debug!(
                    "discarded GUI presentation because render submission failed: {error}"
                );
                false
            }
        }
    });

    if result.published() > 0
        && let Some(waker) = render_waker
    {
        waker.wake();
    }
}

/// Raise the process stack size limit, as GNU Emacs's `main` does at
/// `src/emacs.c:1563-1623` -- but to a flat target rather than GNU's computed
/// one, and the difference is measurable in `command-line-max-length`. See the
/// call site for the derivation.
#[cfg(unix)]
fn increase_stack_limit() {
    const TARGET_STACK_MB: u64 = 128;
    let target = TARGET_STACK_MB * 1024 * 1024;
    unsafe {
        let mut rlim = std::mem::MaybeUninit::<libc::rlimit>::uninit();
        if libc::getrlimit(libc::RLIMIT_STACK, rlim.as_mut_ptr()) == 0 {
            let mut rlim = rlim.assume_init();
            if rlim.rlim_cur < target as libc::rlim_t {
                rlim.rlim_cur = std::cmp::min(target as libc::rlim_t, rlim.rlim_max);
                let _ = libc::setrlimit(libc::RLIMIT_STACK, &rlim);
            }
        }
    }
}

#[cfg(not(unix))]
fn increase_stack_limit() {}

/// Job-control signal hygiene so a child process can never suspend the editor
/// (issue #132). Mirrors GNU Emacs, which is never stopped by SIGTTOU.
///
/// Children are spawned in their own process group (callproc::new_child_command
/// → setpgid), so a child's SIGTSTP stays in its own group. The remaining
/// vector is SIGTTOU: when an interactive subprocess (`bash -i`) grabs the
/// controlling terminal's foreground process group, neomacs's own terminal
/// output (redisplay) from a momentarily-backgrounded state would raise SIGTTOU
/// and stop the whole process. Ignoring SIGTTOU makes those writes succeed
/// instead — exactly how a terminal editor must behave. SIGTSTP is left intact
/// so deliberate `C-z` suspension still works.
#[cfg(unix)]
fn install_job_control_signal_hygiene() {
    // SIG_IGN is idempotent and process-global; safe to set once at startup.
    unsafe {
        libc::signal(libc::SIGTTOU, libc::SIG_IGN);
    }
}

#[cfg(not(unix))]
fn install_job_control_signal_hygiene() {}

/// Initialize the C library locale from the environment, matching GNU Emacs's
/// `emacs.c` main(): it calls `setlocale (LC_ALL, "")` (unless `LC_ALL` is the
/// "C" locale) followed by `fixup_locale ()`, which forces `LC_NUMERIC` back to
/// "C" so the Elisp reader/printer keep parsing numbers locale-independently.
///
/// Without this, the C library stays in its default "C" locale, so the
/// `wcscoll`/`wcscoll_l` and `towlower`/`towlower_l` calls behind
/// `string-collate-lessp`/`string-collate-equalp` collate by raw code point
/// (e.g. "A" < "a", diacritics sorted after all ASCII) instead of honoring the
/// user's collation locale (`en_US.UTF-8`, etc.), diverging from GNU.
#[cfg(unix)]
fn initialize_system_locale() {
    // GNU skips the setlocale when LC_ALL=="C" (emacs.c:1646-1648); the C
    // library is already in that locale, so the call is a no-op there.
    let lc_all_is_c = std::env::var_os("LC_ALL")
        .map(|v| v == *std::ffi::OsStr::new("C"))
        .unwrap_or(false);
    let empty = std::ffi::CString::new("").expect("empty CString");
    let c_locale = std::ffi::CString::new("C").expect("\"C\" CString");
    unsafe {
        if !lc_all_is_c {
            // setlocale (LC_ALL, "") — pick up the environment's locale.
            libc::setlocale(libc::LC_ALL, empty.as_ptr());
        }
        // fixup_locale (): the Emacs Lisp reader needs LC_NUMERIC == "C" so
        // numbers are read and printed with '.' as the decimal separator
        // regardless of the user's locale (emacs.c:3192-3199).
        libc::setlocale(libc::LC_NUMERIC, c_locale.as_ptr());
    }
}

#[cfg(not(unix))]
fn initialize_system_locale() {}

#[cfg(test)]
#[path = "main_test.rs"]
mod tests;
