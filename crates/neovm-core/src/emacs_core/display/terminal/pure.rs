//! Terminal/TTY builtins extracted from display.rs and builtins.rs.
//!
//! Provides the terminal runtime owner, terminal parameter storage,
//! and all terminal/tty query builtins.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{EvalResult, Flow, signal};
use crate::emacs_core::error::{expect_args, expect_args_range, expect_max_args};
use crate::emacs_core::value::*;
use crate::emacs_core::value::{ValueKind, VecLikeType};
use crate::window::FrameId;
use neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities;
use std::cell::RefCell;
use std::num::NonZeroU32;

// ---------------------------------------------------------------------------
// Thread-local terminal state
// ---------------------------------------------------------------------------

thread_local! {
    static TERMINAL_MANAGER: RefCell<TerminalManager> = RefCell::new(TerminalManager::new());
}

pub(crate) const TERMINAL_NAME: &str = "initial_terminal";
pub(crate) const TERMINAL_ID: u64 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalRuntime {
    active: bool,
    tty_type: Option<String>,
    color_cells: i64,
    controlling_tty: bool,
    suspended: bool,
    /// What this terminal can render, from its terminfo entry -- GNU's `TS_*`
    /// capability strings on `struct tty_display_info`. Answers
    /// `display-supports-face-attributes-p` (GNU `tty_capable_p`) with the same
    /// record the renderer emits from, so the predicate and the output cannot
    /// disagree about, say, whether this terminal has `sitm`.
    attribute_capabilities: TtyAttributeCapabilities,
}

impl TerminalRuntime {
    fn inactive() -> Self {
        Self {
            active: false,
            tty_type: None,
            color_cells: 0,
            controlling_tty: false,
            suspended: false,
            // GNU's initial terminal has no capability strings until a real
            // terminal is initialized from terminfo, which is why
            // `display-supports-face-attributes-p' answers nil for everything in
            // `--batch'.
            attribute_capabilities: TtyAttributeCapabilities::none(),
        }
    }

    fn supports_color(&self) -> bool {
        self.color_cells > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalRuntimeConfig {
    pub name: Option<String>,
    /// What kind of display this terminal drives -- GNU's argument to
    /// `create_terminal`.  There is no default: every construction site picks a
    /// constructor ([`TerminalRuntimeConfig::inactive`],
    /// [`TerminalRuntimeConfig::interactive`],
    /// [`TerminalRuntimeConfig::window_system`]) and thereby answers it.
    pub output_method: TerminalOutputMethod,
    pub tty_type: Option<String>,
    pub controlling_tty: bool,
    /// See [`TerminalRuntime::attribute_capabilities`]. Defaults to
    /// [`TtyAttributeCapabilities::none`] -- GNU learns these from terminfo when
    /// a terminal is initialized, and knows none before that -- so a caller that
    /// has read terminfo must pass them with
    /// [`TerminalRuntimeConfig::with_attribute_capabilities`].
    pub attribute_capabilities: TtyAttributeCapabilities,
}

pub trait TerminalHost {
    fn suspend_tty(&mut self) -> Result<(), String>;
    fn resume_tty(&mut self) -> Result<(), String>;
    fn delete_terminal(&mut self) -> Result<(), String> {
        Ok(())
    }
}

/// Validated identity of a text terminal a frontend must open for one frame.
///
/// The Lisp-facing `tty` and `tty-type` parameters are loose values; this is
/// the narrow, owned request that crosses from the VM into platform code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtyFrameOpenRequest {
    terminal_id: u64,
    frame_id: FrameId,
    device: String,
    terminal_type: String,
}

impl TtyFrameOpenRequest {
    pub fn new(
        terminal_id: u64,
        frame_id: FrameId,
        device: String,
        terminal_type: String,
    ) -> Result<Self, String> {
        if device.is_empty() {
            return Err("Invalid terminal device".to_string());
        }
        if terminal_type.is_empty() {
            return Err("Invalid terminal type".to_string());
        }
        Ok(Self {
            terminal_id,
            frame_id,
            device,
            terminal_type,
        })
    }

    pub fn terminal_id(&self) -> u64 {
        self.terminal_id
    }

    pub fn frame_id(&self) -> FrameId {
        self.frame_id
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    pub fn terminal_type(&self) -> &str {
        &self.terminal_type
    }
}

/// Character-cell dimensions of an opened TTY. Zero-sized terminals cannot
/// enter the frame model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TtyFrameSize {
    columns: NonZeroU32,
    rows: NonZeroU32,
}

impl TtyFrameSize {
    pub fn new(columns: u32, rows: u32) -> Option<Self> {
        Some(Self {
            columns: NonZeroU32::new(columns)?,
            rows: NonZeroU32::new(rows)?,
        })
    }

    pub fn columns(self) -> u32 {
        self.columns.get()
    }

    pub fn rows(self) -> u32 {
        self.rows.get()
    }
}

/// Resources returned only after platform code has successfully opened and
/// initialized a TTY.
pub struct OpenedTtyFrameHost {
    size: TtyFrameSize,
    attribute_capabilities: TtyAttributeCapabilities,
    host: Box<dyn TerminalHost>,
}

impl OpenedTtyFrameHost {
    pub fn new(
        size: TtyFrameSize,
        attribute_capabilities: TtyAttributeCapabilities,
        host: Box<dyn TerminalHost>,
    ) -> Self {
        Self {
            size,
            attribute_capabilities,
            host,
        }
    }
}

/// Frontend-owned factory for OS terminal resources.
///
/// `neovm-core` owns Lisp/frame/terminal identity; the binary owns file
/// descriptors, raw mode, input threads, and the renderer bound to them.
pub trait TtyFrameHostFactory {
    fn open_tty(&mut self, request: TtyFrameOpenRequest) -> Result<OpenedTtyFrameHost, String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeleteTerminalMode {
    Public { force_non_nil: bool },
    Noelisp,
}

impl DeleteTerminalMode {
    fn runs_hooks_immediately(self) -> bool {
        matches!(self, Self::Public { .. })
    }

    fn bypasses_active_terminal_check(self) -> bool {
        !matches!(
            self,
            Self::Public {
                force_non_nil: false
            }
        )
    }

    fn ignore_host_delete_errors(self) -> bool {
        matches!(self, Self::Noelisp)
    }
}

/// GNU's `enum output_method` (src/termhooks.h), as far as neomacs models it:
/// what KIND of display a terminal drives.
///
/// GNU tells these apart by allocating one `struct terminal` per display --
/// `init_initial_terminal` makes the `output_initial` one, `init_tty`
/// (src/term.c) an `output_termcap` one, `x_term_init` an `output_x_window`
/// one -- and deletes the initial terminal once a real one exists.  We keep ONE
/// record and re-describe it in place, so the kind has to be STATED rather than
/// inferred from what happens to be true of the record:
///
/// * not from the id -- GNU's tty terminal is `#<terminal 1 on /dev/tty>` and
///   ours is `#<terminal 0 on /dev/tty>`, because ours is the same record the
///   bootstrap started with;
/// * not from the name -- a window-system terminal keeps `"initial_terminal"`
///   when its display connection has no name to adopt;
/// * not from liveness or activity -- `terminal-live-p` deliberately reports
///   `output_initial` and `output_termcap` alike as `t` (src/terminal.c:456-459),
///   which is exactly why `turn-on-xterm-mouse-tracking-on-terminal`
///   (lisp/xt-mouse.el:510-512) needs a SECOND question to separate them.
///
/// That second question is `frame-initial-p`, and its terminal branch is one
/// comparison against this type: `t->type == output_initial`
/// (src/terminal.c:499).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalOutputMethod {
    /// GNU `output_initial`: the bootstrap terminal used during daemon mode,
    /// batch mode and the early stages of startup, and which holds the initial
    /// frame.
    Initial,
    /// GNU `output_termcap`: a text terminal on a tty device.
    Termcap,
    /// GNU `output_x_window` / `output_pgtk` / `output_ns` / …: a window-system
    /// display connection.
    WindowSystem,
}

impl TerminalOutputMethod {
    /// GNU `Fframe_initial_p`'s terminal branch: `t->type == output_initial`.
    fn is_initial(self) -> bool {
        matches!(self, Self::Initial)
    }
}

struct TerminalRecord {
    id: u64,
    name: String,
    handle: Value,
    params: Vec<(Value, Value)>,
    runtime: TerminalRuntime,
    /// GNU `struct terminal.type`.  See [`TerminalOutputMethod`].
    output_method: TerminalOutputMethod,
    deleted: bool,
    host: Option<Box<dyn TerminalHost>>,
}

impl TerminalRecord {
    fn new(id: u64, name: String) -> Self {
        Self {
            id,
            name,
            handle: terminal_handle_for_id(id),
            params: Vec::new(),
            runtime: TerminalRuntime::inactive(),
            // GNU's first terminal is `init_initial_terminal`'s, and every
            // record starts as that one until a display init re-describes it.
            output_method: TerminalOutputMethod::Initial,
            deleted: false,
            host: None,
        }
    }

    fn is_live(&self) -> bool {
        !self.deleted
    }

    fn is_active(&self) -> bool {
        if !self.is_live() {
            return false;
        }
        if self.runtime.controlling_tty || self.runtime.tty_type.is_some() {
            self.runtime.active && !self.runtime.suspended
        } else {
            true
        }
    }
}

struct TerminalManager {
    terminals: Vec<TerminalRecord>,
}

impl TerminalManager {
    fn new() -> Self {
        let mut this = Self {
            terminals: Vec::new(),
        };
        this.ensure_initial_terminal();
        this
    }

    fn ensure_initial_terminal(&mut self) -> &mut TerminalRecord {
        if let Some(idx) = self
            .terminals
            .iter()
            .position(|terminal| terminal.id == TERMINAL_ID)
        {
            if self.terminals[idx].deleted {
                self.terminals[idx].deleted = false;
                self.terminals[idx].runtime = TerminalRuntime::inactive();
                // Re-created from nothing is re-created as GNU's
                // `init_initial_terminal` terminal, whatever it drove before.
                self.terminals[idx].output_method = TerminalOutputMethod::Initial;
                self.terminals[idx].host = None;
            }
            return &mut self.terminals[idx];
        }
        self.terminals
            .push(TerminalRecord::new(TERMINAL_ID, TERMINAL_NAME.to_string()));
        self.terminals.last_mut().expect("initial terminal present")
    }

    fn reset_handles(&mut self) {
        for terminal in &mut self.terminals {
            terminal.handle = terminal_handle_for_id(terminal.id);
        }
    }

    fn get(&self, id: u64) -> Option<&TerminalRecord> {
        self.terminals.iter().find(|terminal| terminal.id == id)
    }

    fn get_mut(&mut self, id: u64) -> Option<&mut TerminalRecord> {
        self.terminals.iter_mut().find(|terminal| terminal.id == id)
    }

    fn find_by_handle(&self, value: &Value) -> Option<&TerminalRecord> {
        self.terminals
            .iter()
            .find(|terminal| eq_value(&terminal.handle, value))
    }

    fn live_terminals(&self) -> impl Iterator<Item = &TerminalRecord> {
        self.terminals.iter().filter(|terminal| terminal.is_live())
    }

    fn active_live_terminal_count(&self) -> usize {
        self.live_terminals()
            .filter(|terminal| terminal.is_active())
            .count()
    }

    fn live_terminal_ids_in_keyboard_poll_order(&self) -> Vec<u64> {
        self.terminals
            .iter()
            .rev()
            .filter(|terminal| terminal.is_live())
            .map(|terminal| terminal.id)
            .collect()
    }

    fn ensure_terminal(
        &mut self,
        id: u64,
        name: String,
        runtime: TerminalRuntime,
        output_method: TerminalOutputMethod,
    ) -> &mut TerminalRecord {
        if let Some(idx) = self.terminals.iter().position(|terminal| terminal.id == id) {
            let terminal = &mut self.terminals[idx];
            terminal.name = name;
            terminal.deleted = false;
            terminal.runtime = runtime;
            terminal.output_method = output_method;
            return terminal;
        }
        self.terminals.push(TerminalRecord {
            id,
            name,
            handle: terminal_handle_for_id(id),
            params: Vec::new(),
            runtime,
            output_method,
            deleted: false,
            host: None,
        });
        self.terminals.last_mut().expect("terminal present")
    }
}

impl TerminalRuntimeConfig {
    /// GNU `init_initial_terminal`: the display-less bootstrap terminal.  Also
    /// the terminal the GUI startup keeps for the hidden initial frame, which
    /// is the same thing GNU keeps it for.
    pub fn inactive() -> Self {
        Self {
            name: None,
            output_method: TerminalOutputMethod::Initial,
            tty_type: None,
            controlling_tty: false,
            attribute_capabilities: TtyAttributeCapabilities::none(),
        }
    }

    /// GNU `init_tty` (src/term.c): a text terminal on a tty device.
    ///
    /// The colour-cell count is NOT a separate parameter, and that is ledger
    /// 193's item 2 in one signature: it is `TN_max_colors`, which GNU
    /// computes once inside `init_tty`'s `op` gate and stores on the same
    /// `struct tty_display_info` as `TS_set_foreground`.  Taking it beside the
    /// capability record is what let this port answer it twice.
    pub fn interactive(
        tty_type: Option<String>,
        attribute_capabilities: TtyAttributeCapabilities,
    ) -> Self {
        Self {
            name: None,
            output_method: TerminalOutputMethod::Termcap,
            tty_type,
            controlling_tty: true,
            attribute_capabilities,
        }
    }

    /// GNU `x_term_init` / `pgtk_term_init`: a window-system display
    /// connection.  It carries no tty capabilities and no colour-cell count --
    /// those are terminfo facts about a text terminal -- but it is emphatically
    /// not the initial terminal, whether or not the display had a name to
    /// adopt.
    pub fn window_system() -> Self {
        Self {
            name: None,
            output_method: TerminalOutputMethod::WindowSystem,
            tty_type: None,
            controlling_tty: false,
            attribute_capabilities: TtyAttributeCapabilities::none(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Record what this terminal can render (its terminfo capabilities).
    pub fn with_attribute_capabilities(mut self, caps: TtyAttributeCapabilities) -> Self {
        self.attribute_capabilities = caps;
        self
    }
}

/// Re-describe the primary terminal as the display CONFIG names -- GNU's
/// `create_terminal (type, …)` plus the name that display init gives it.
///
/// A window-system terminal takes its name from its display connection (`":0"`,
/// `"wayland-0"`), not from the bootstrap `"initial_terminal"`: Elisp uses
/// `(terminal-name)` to tell a real display from the display-less initial one --
/// e.g. indent-bars' `indent-bars-reset-styles` skips recomputing bar colors on
/// a theme change while the terminal is still `"initial_terminal"`.  A config
/// with no name leaves the existing one alone, so a display that has no name to
/// give still gets its output method re-described.
pub fn configure_terminal_runtime(config: TerminalRuntimeConfig) {
    TERMINAL_MANAGER.with(|slot| {
        let mut manager = slot.borrow_mut();
        let terminal = manager.ensure_initial_terminal();
        if let Some(name) = config.name {
            terminal.name = name;
        }
        terminal.output_method = config.output_method;
        terminal.runtime = TerminalRuntime {
            active: config.controlling_tty
                || config.tty_type.is_some()
                || config.attribute_capabilities.color_cells() > 0,
            tty_type: config.tty_type,
            color_cells: config.attribute_capabilities.color_cells().max(0),
            controlling_tty: config.controlling_tty,
            suspended: false,
            attribute_capabilities: config.attribute_capabilities,
        };
    });
}

pub fn ensure_terminal_runtime_owner(
    id: u64,
    name: impl Into<String>,
    config: TerminalRuntimeConfig,
) -> Value {
    TERMINAL_MANAGER.with(|slot| {
        let mut manager = slot.borrow_mut();
        let output_method = config.output_method;
        let runtime = TerminalRuntime {
            active: config.controlling_tty
                || config.tty_type.is_some()
                || config.attribute_capabilities.color_cells() > 0,
            tty_type: config.tty_type,
            color_cells: config.attribute_capabilities.color_cells().max(0),
            controlling_tty: config.controlling_tty,
            suspended: false,
            attribute_capabilities: config.attribute_capabilities,
        };
        manager
            .ensure_terminal(id, name.into(), runtime, output_method)
            .handle
    })
}

pub(crate) fn next_terminal_id() -> u64 {
    TERMINAL_MANAGER.with(|slot| {
        slot.borrow()
            .terminals
            .iter()
            .map(|terminal| terminal.id)
            .max()
            .unwrap_or(TERMINAL_ID)
            .checked_add(1)
            .expect("terminal id exhausted")
    })
}

/// GNU `get_named_terminal`: find an active termcap terminal already owning
/// DEVICE so a second frame shares its renderer, input source, and kboard
/// instead of opening the same tty twice.
pub(crate) fn active_tty_terminal_id_by_name(device: &str) -> Option<u64> {
    TERMINAL_MANAGER.with(|slot| {
        slot.borrow()
            .terminals
            .iter()
            .find(|terminal| {
                terminal.output_method == TerminalOutputMethod::Termcap
                    && terminal.name == device
                    && terminal.is_active()
            })
            .map(|terminal| terminal.id)
    })
}

pub(crate) fn install_opened_tty(
    request: &TtyFrameOpenRequest,
    opened: OpenedTtyFrameHost,
) -> TtyFrameSize {
    let size = opened.size;
    TERMINAL_MANAGER.with(|slot| {
        let mut manager = slot.borrow_mut();
        let runtime = TerminalRuntime {
            active: true,
            tty_type: Some(request.terminal_type.clone()),
            color_cells: opened.attribute_capabilities.color_cells().max(0),
            controlling_tty: true,
            suspended: false,
            attribute_capabilities: opened.attribute_capabilities,
        };
        let terminal = manager.ensure_terminal(
            request.terminal_id,
            request.device.clone(),
            runtime,
            TerminalOutputMethod::Termcap,
        );
        terminal.host = Some(opened.host);
    });
    size
}

pub fn reset_terminal_runtime() {
    TERMINAL_MANAGER.with(|slot| {
        let mut manager = slot.borrow_mut();
        let terminal = manager.ensure_initial_terminal();
        terminal.name = TERMINAL_NAME.to_string();
        terminal.output_method = TerminalOutputMethod::Initial;
        terminal.runtime = TerminalRuntime::inactive();
    });
}

pub fn set_terminal_host(host: Box<dyn TerminalHost>) {
    TERMINAL_MANAGER.with(|slot| {
        let mut manager = slot.borrow_mut();
        manager.ensure_initial_terminal().host = Some(host);
    });
}

pub fn reset_terminal_host() {
    TERMINAL_MANAGER.with(|slot| {
        let mut manager = slot.borrow_mut();
        manager.ensure_initial_terminal().host = None;
    });
}

fn terminal_runtime() -> TerminalRuntime {
    TERMINAL_MANAGER.with(|slot| {
        slot.borrow()
            .get(TERMINAL_ID)
            .map(|terminal| terminal.runtime.clone())
            .unwrap_or_else(TerminalRuntime::inactive)
    })
}

pub(crate) fn terminal_runtime_color_cells() -> i64 {
    terminal_runtime().color_cells
}

pub(crate) fn terminal_runtime_supports_color() -> bool {
    terminal_runtime().supports_color()
}

/// What the current terminal can render -- GNU's `struct tty_display_info`
/// capability strings, the input to `tty_capable_p`.
pub(crate) fn terminal_runtime_attribute_capabilities() -> TtyAttributeCapabilities {
    terminal_runtime().attribute_capabilities
}

/// Clear cached terminal thread-locals (called from `reset_display_thread_locals`).
pub(crate) fn reset_terminal_thread_locals() {
    TERMINAL_MANAGER.with(|slot| *slot.borrow_mut() = TerminalManager::new());
}

/// Reset only the terminal handle (stale reference safety on heap reset).
/// Does NOT reset terminal params or runtime config.
pub(crate) fn reset_terminal_handle() {
    TERMINAL_MANAGER.with(|slot| slot.borrow_mut().reset_handles());
}

/// Collect GC roots from terminal thread-locals.
pub(crate) fn collect_terminal_gc_roots(roots: &mut Vec<Value>) {
    TERMINAL_MANAGER.with(|slot| {
        for terminal in &slot.borrow().terminals {
            roots.push(terminal.handle);
            for (k, v) in &terminal.params {
                roots.push(*k);
                roots.push(*v);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Terminal handle helpers
// ---------------------------------------------------------------------------

fn terminal_handle_for_id(id: u64) -> Value {
    Value::make_terminal(id)
}

pub(crate) fn terminal_handle_value() -> Value {
    terminal_handle_value_for_id(TERMINAL_ID).unwrap_or_else(|| terminal_handle_for_id(TERMINAL_ID))
}

pub(crate) fn terminal_handle_value_for_id(id: u64) -> Option<Value> {
    TERMINAL_MANAGER.with(|slot| slot.borrow().get(id).map(|terminal| terminal.handle))
}

pub(crate) fn is_terminal_handle(value: &Value) -> bool {
    terminal_handle_id(value).is_some()
}

pub(crate) fn terminal_handle_id(value: &Value) -> Option<u64> {
    TERMINAL_MANAGER.with(|slot| {
        slot.borrow()
            .find_by_handle(value)
            .map(|terminal| terminal.id)
    })
}

pub(crate) fn print_terminal_handle(value: &Value) -> Option<String> {
    TERMINAL_MANAGER.with(|slot| {
        slot.borrow()
            .find_by_handle(value)
            .map(|terminal| format!("#<terminal {} on {}>", terminal.id, terminal.name))
    })
}

// ---------------------------------------------------------------------------
// Terminal designator predicates
// ---------------------------------------------------------------------------

pub(crate) fn terminal_designator_p(value: &Value) -> bool {
    value.is_nil() || is_terminal_handle(value)
}

fn live_terminal_id_by_handle(value: &Value) -> Option<u64> {
    TERMINAL_MANAGER.with(|slot| {
        slot.borrow()
            .find_by_handle(value)
            .filter(|terminal| terminal.is_live())
            .map(|terminal| terminal.id)
    })
}

fn selected_terminal_id(eval: &crate::emacs_core::eval::Context) -> Option<u64> {
    eval.frames
        .selected_frame()
        .map(|frame| frame.terminal_id)
        .or_else(|| {
            TERMINAL_MANAGER.with(|slot| {
                slot.borrow()
                    .get(TERMINAL_ID)
                    .filter(|terminal| terminal.is_live())
                    .map(|terminal| terminal.id)
            })
        })
}

fn decode_terminal_id_eval(eval: &crate::emacs_core::eval::Context, value: &Value) -> Option<u64> {
    if value.is_nil() {
        return selected_terminal_id(eval);
    }
    if let Some(id) = live_terminal_id_by_handle(value) {
        return Some(id);
    }
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Frame) => eval
            .frames
            .get(crate::window::FrameId(value.as_frame_id().unwrap()))
            .and_then(|frame| {
                TERMINAL_MANAGER.with(|slot| {
                    slot.borrow()
                        .get(frame.terminal_id)
                        .filter(|terminal| terminal.is_live())
                        .map(|terminal| terminal.id)
                })
            }),
        _ => None,
    }
}

pub(crate) fn terminal_designator_eval_p(
    eval: &mut crate::emacs_core::eval::Context,
    value: &Value,
) -> bool {
    decode_terminal_id_eval(eval, value).is_some()
}

/// What a `frame-initial-p` argument turned out to be.
///
/// GNU's `Fframe_initial_p` (src/terminal.c:482-500) resolves its argument
/// twice: `FRAMEP` first, `decode_terminal` otherwise.  The two branches ask
/// different questions of different objects -- `FRAME_INITIAL_P (f)` of a frame,
/// `t->type == output_initial` of a terminal -- and `decode_terminal`
/// (src/terminal.c:223-233) answers NULL, never a signal, for everything else.
///
/// Naming the three outcomes is what keeps the frame-only reading from creeping
/// back: a port that transcribes only the `if (FRAMEP …)` body loses the branch
/// silently, because the `if` is the only trace of the `else`.  Here the subr
/// matches on this enum, so the terminal case cannot be dropped without the
/// compiler saying so, and there is nowhere left to put a raise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameOrTerminal {
    /// GNU `FRAMEP` + `FRAME_LIVE_P`: a live frame.
    Frame(crate::window::FrameId),
    /// GNU `decode_terminal`: a live terminal.  A deleted one does not qualify
    /// -- `delete_terminal` frees `t->name` and `decode_terminal`'s last line is
    /// `return t && t->name ? t : NULL`.
    Terminal(u64),
    /// GNU's NULL, and GNU's dead frame: the caller answers nil.
    Neither,
}

/// GNU `Fframe_initial_p`'s argument resolution, performed once.
///
/// nil resolves to the selected frame BEFORE the `FRAMEP` test, so nil always
/// takes the frame branch -- in batch we materialize that frame the same way
/// every other frame subr does.
pub(crate) fn decode_frame_or_terminal(
    eval: &mut crate::emacs_core::eval::Context,
    arg: Option<&Value>,
) -> FrameOrTerminal {
    let Some(value) = arg.filter(|value| !value.is_nil()) else {
        return FrameOrTerminal::Frame(
            crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state(
                &mut eval.frames,
                &mut eval.buffers,
            ),
        );
    };
    if let ValueKind::Veclike(VecLikeType::Frame) = value.kind() {
        let frame_id = crate::window::FrameId(value.as_frame_id().expect("frame value"));
        return if eval.frames.get(frame_id).is_some() {
            FrameOrTerminal::Frame(frame_id)
        } else {
            // GNU reaches `FRAME_LIVE_P (f)` here and answers nil.
            FrameOrTerminal::Neither
        };
    }
    match live_terminal_id_by_handle(value) {
        Some(id) => FrameOrTerminal::Terminal(id),
        None => FrameOrTerminal::Neither,
    }
}

pub(crate) fn expect_terminal_designator_eval(
    eval: &mut crate::emacs_core::eval::Context,
    value: &Value,
) -> Result<(), Flow> {
    if terminal_designator_eval_p(eval, value) {
        Ok(())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), *value],
        ))
    }
}

// ---------------------------------------------------------------------------
// Terminal parameter helpers
// ---------------------------------------------------------------------------

/// Fallback values for terminal parameters that GNU's own startup Lisp always
/// stores before anything reads them, so a bare `Context` (no `command-line`
/// pass) still answers like a booted GNU session.
///
/// GNU itself has NO terminal-parameter defaults: `terminal-parameter` is a
/// plain assq over the terminal's alist (src/terminal.c, store_terminal_param)
/// and every entry starts absent. In particular `normal-erase-is-backspace`
/// must NOT appear here: `normal-erase-is-backspace-setup-frame`
/// (lisp/simple.el:11097) is guarded by `(unless (terminal-parameter nil
/// 'normal-erase-is-backspace) ...)`, so a fabricated 0 permanently vetoes the
/// real decision `command-line` (lisp/startup.el:1638) makes AFTER
/// `init_sys_modes` publishes the tty's ERASE character -- the mode's
/// `:variable` setter stores the genuine 0/1 (DIVERGENCES.md entry 67).
fn terminal_parameter_default_value(key: &Value) -> Option<Value> {
    match key.as_symbol_name() {
        Some("keyboard-coding-saved-meta-mode") => Some(Value::list(vec![Value::T])),
        _ => None,
    }
}

fn terminal_parameter_default_entries() -> Vec<(Value, Value)> {
    vec![(
        Value::symbol("keyboard-coding-saved-meta-mode"),
        Value::list(vec![Value::T]),
    )]
}

fn lookup_terminal_parameter_value(params: &[(Value, Value)], key: &Value) -> Value {
    params
        .iter()
        .find_map(|(stored_key, stored_value)| {
            if eq_value(stored_key, key) {
                Some(*stored_value)
            } else {
                None
            }
        })
        .or_else(|| terminal_parameter_default_value(key))
        .unwrap_or(Value::NIL)
}

fn terminal_parameters_with_defaults(params: &[(Value, Value)]) -> Vec<(Value, Value)> {
    let mut merged = terminal_parameter_default_entries();
    for (key, value) in params {
        if let Some((_, existing_value)) = merged
            .iter_mut()
            .find(|(existing_key, _)| eq_value(existing_key, key))
        {
            *existing_value = *value;
        } else {
            merged.push((*key, *value));
        }
    }
    merged
}

fn expect_symbol_key(value: &Value) -> Result<Value, Flow> {
    match value.kind() {
        ValueKind::Nil | ValueKind::T | ValueKind::Symbol(_) => Ok(*value),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *value],
        )),
    }
}

fn terminal_name_for_id(id: u64) -> Option<String> {
    TERMINAL_MANAGER.with(|slot| slot.borrow().get(id).map(|terminal| terminal.name.clone()))
}

fn terminal_runtime_for_id(id: u64) -> TerminalRuntime {
    TERMINAL_MANAGER.with(|slot| {
        slot.borrow()
            .get(id)
            .map(|terminal| terminal.runtime.clone())
            .unwrap_or_else(TerminalRuntime::inactive)
    })
}

/// GNU `t->type` for a terminal that still exists.
fn terminal_output_method_for_id(id: u64) -> Option<TerminalOutputMethod> {
    TERMINAL_MANAGER.with(|slot| slot.borrow().get(id).map(|terminal| terminal.output_method))
}

/// Mark the selected terminal as having a controlling tty, so it can host a
/// text-terminal frame. Used by tests that exercise `make-frame` /
/// `make-terminal-frame`, which in a real session run on an interactive
/// terminal (the production batch path deliberately has neither a controlling
/// tty nor a type, so frame creation errors like GNU).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn mark_selected_terminal_usable_for_test(eval: &crate::emacs_core::eval::Context) {
    if let Some(id) = decode_terminal_id_eval(eval, &Value::NIL) {
        TERMINAL_MANAGER.with(|slot| {
            if let Some(record) = slot.borrow_mut().get_mut(id) {
                record.runtime.controlling_tty = true;
            }
        });
    }
}

/// Whether the selected terminal can host a text-terminal frame: it has a
/// controlling tty or a known terminal type. GNU's `init_tty` signals
/// "Unknown terminal type" when neither holds (batch / no real terminal), which
/// is why `make-frame` / `make-terminal-frame` error in `--batch`.
pub(crate) fn selected_terminal_is_usable_tty(eval: &crate::emacs_core::eval::Context) -> bool {
    decode_terminal_id_eval(eval, &Value::NIL)
        .map(|id| {
            let runtime = terminal_runtime_for_id(id);
            runtime.controlling_tty || runtime.tty_type.is_some()
        })
        .unwrap_or(false)
}

fn terminal_params_for_id(id: u64) -> Vec<(Value, Value)> {
    TERMINAL_MANAGER.with(|slot| {
        slot.borrow()
            .get(id)
            .map(|terminal| terminal.params.clone())
            .unwrap_or_default()
    })
}

fn update_terminal_param(id: u64, key: Value, value: Value) -> Value {
    TERMINAL_MANAGER.with(|slot| {
        let mut manager = slot.borrow_mut();
        let Some(terminal) = manager.get_mut(id) else {
            return Value::NIL;
        };
        if let Some((_, stored_value)) = terminal
            .params
            .iter_mut()
            .find(|(stored_key, _)| eq_value(stored_key, &key))
        {
            let previous = *stored_value;
            *stored_value = value;
            return previous;
        }
        let previous = terminal_parameter_default_value(&key).unwrap_or(Value::NIL);
        terminal.params.push((key, value));
        previous
    })
}

fn with_terminal_host_for_id<R>(
    id: u64,
    f: impl FnOnce(&mut dyn TerminalHost) -> Result<R, String>,
) -> Result<R, Flow> {
    TERMINAL_MANAGER.with(|slot| {
        let mut manager = slot.borrow_mut();
        let Some(host) = manager
            .get_mut(id)
            .and_then(|terminal| terminal.host.as_deref_mut())
        else {
            return Err(signal(
                "error",
                vec![Value::string("TTY terminal host unavailable")],
            ));
        };
        f(host).map_err(|message| signal("error", vec![Value::string(message)]))
    })
}

fn delete_terminal_record(id: u64) {
    TERMINAL_MANAGER.with(|slot| {
        let mut manager = slot.borrow_mut();
        if let Some(terminal) = manager.get_mut(id) {
            terminal.deleted = true;
            terminal.runtime = TerminalRuntime::inactive();
            terminal.host = None;
        }
    });
}

pub(crate) fn live_terminal_ids_in_keyboard_poll_order() -> Vec<u64> {
    TERMINAL_MANAGER.with(|slot| slot.borrow().live_terminal_ids_in_keyboard_poll_order())
}

// ---------------------------------------------------------------------------
// Alist helper
// ---------------------------------------------------------------------------

pub(crate) fn make_alist(pairs: Vec<(Value, Value)>) -> Value {
    let entries: Vec<Value> = pairs.into_iter().map(|(k, v)| Value::cons(k, v)).collect();
    Value::list(entries)
}

// ---------------------------------------------------------------------------
// Argument helpers (local copies — identical to display.rs)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Terminal builtins
// ---------------------------------------------------------------------------

/// (terminal-name &optional TERMINAL) -> "initial_terminal"
///
/// Accepts live frame designators in addition to terminal designators.
pub(crate) fn builtin_terminal_name(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("terminal-name", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    Ok(Value::string(
        terminal_name_for_id(terminal_id).unwrap_or_else(|| TERMINAL_NAME.to_string()),
    ))
}

/// `(frame-initial-p &optional FRAME)` -- GNU `Fframe_initial_p`,
/// src/terminal.c:482-500.
///
/// FRAME is a frame OR a terminal, and GNU's doc string says both: "If FRAME is
/// a terminal object, return non-nil if it holds the initial frame."  The
/// terminal branch has a caller that depends on it --
/// `turn-on-xterm-mouse-tracking-on-terminal` (lisp/xt-mouse.el:508-512) hands
/// it a TERMINAL to skip "the initial terminal which is not a termcap device" --
/// and that caller runs during startup on every TERM matching
/// `xterm--auto-xt-mouse-allowed-types` (lisp/term/xterm.el:134-140).  It runs
/// inside `tty-run-terminal-initialization`, i.e. before `command-line-1`, so a
/// raise here does not merely print: it costs the whole command line, `-l` and
/// `--eval` included.
///
/// Nothing handed to this subr can raise.  GNU's `decode_terminal` answers NULL
/// for a non-designator and for a deleted terminal, and `FRAME_LIVE_P` covers a
/// dead frame, so every unusable argument answers nil.
pub(crate) fn builtin_frame_initial_p(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-initial-p", &args, 1)?;
    let initial = match decode_frame_or_terminal(eval, args.first()) {
        // GNU: `FRAME_LIVE_P (f) && FRAME_INITIAL_P (f)`; the resolution above
        // has already established the frame is live.
        FrameOrTerminal::Frame(frame_id) => {
            eval.frames.get(frame_id).is_some_and(|frame| frame.initial)
        }
        // GNU: `t->type == output_initial`.
        FrameOrTerminal::Terminal(terminal_id) => {
            terminal_output_method_for_id(terminal_id).is_some_and(TerminalOutputMethod::is_initial)
        }
        FrameOrTerminal::Neither => false,
    };
    Ok(Value::bool_val(initial))
}

/// (terminal-list) -> list of live terminal handles.
pub(crate) fn builtin_terminal_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("terminal-list", &args, 0)?;
    let terminals = TERMINAL_MANAGER.with(|slot| {
        slot.borrow()
            .live_terminals()
            .map(|terminal| terminal.handle)
            .collect::<Vec<_>>()
    });
    Ok(Value::list(terminals))
}

/// (selected-terminal) -> currently selected terminal handle.
#[cfg(test)]
pub(crate) fn builtin_selected_terminal(args: Vec<Value>) -> EvalResult {
    expect_args("selected-terminal", &args, 0)?;
    Ok(terminal_handle_value())
}

/// (frame-terminal &optional FRAME) -> opaque terminal handle.
///
/// Accepts live frame designators in addition to nil.
pub(crate) fn builtin_frame_terminal(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-terminal", &args, 1)?;
    let terminal_id = if let Some(frame) = args.first() {
        if frame.is_nil() {
            selected_terminal_id(eval)
        } else {
            match frame.kind() {
                ValueKind::Veclike(VecLikeType::Frame) => eval
                    .frames
                    .get(crate::window::FrameId(frame.as_frame_id().unwrap()))
                    .map(|frame| frame.terminal_id),
                _ => None,
            }
        }
    } else {
        selected_terminal_id(eval)
    };
    let Some(terminal_id) = terminal_id else {
        let bad = args.first().copied().unwrap_or(Value::NIL);
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), bad],
        ));
    };
    Ok(terminal_handle_value_for_id(terminal_id).unwrap_or_else(terminal_handle_value))
}

/// (terminal-live-p TERMINAL) -> t
///
/// In GNU Emacs, terminal-live-p returns the terminal type symbol
/// (e.g. 'x, 'w32) for GUI terminals, or t for TTY.  This is used
/// by framep-on-display to determine the window system type.
pub(crate) fn builtin_terminal_live_p(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("terminal-live-p", &args, 1, 1)?;
    let Some(terminal_id) = decode_terminal_id_eval(eval, &args[0]) else {
        return Ok(Value::NIL);
    };
    let runtime = terminal_runtime_for_id(terminal_id);
    let mut terminal_has_frame = false;
    let window_system = eval
        .frames
        .frame_list()
        .into_iter()
        .filter_map(|frame_id| eval.frames.get(frame_id))
        .filter(|frame| frame.terminal_id == terminal_id)
        .find_map(|frame| {
            terminal_has_frame = true;
            frame.effective_window_system()
        });
    // Return the window system type so framep-on-display works correctly.
    if let Some(window_system) = window_system {
        Ok(window_system)
    } else if terminal_has_frame || runtime.controlling_tty || runtime.tty_type.is_some() {
        Ok(Value::T)
    } else if crate::emacs_core::display::x_window_system_active(eval) {
        Ok(Value::symbol(
            crate::emacs_core::display::gui_window_system_symbol(),
        ))
    } else {
        Ok(Value::T)
    }
}

/// (terminal-parameter TERMINAL PARAMETER) -> value
///
/// Accepts live frame designators in addition to terminal designators.
pub(crate) fn builtin_terminal_parameter(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("terminal-parameter", &args, 2)?;
    let Some(terminal_id) = decode_terminal_id_eval(eval, &args[0]) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), args[0]],
        ));
    };
    let key = expect_symbol_key(&args[1])?;
    Ok(lookup_terminal_parameter_value(
        &terminal_params_for_id(terminal_id),
        &key,
    ))
}

/// (terminal-parameters &optional TERMINAL) -> alist of terminal parameters
///
/// Accepts live frame designators in addition to terminal designators.
pub(crate) fn builtin_terminal_parameters(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("terminal-parameters", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    let merged = terminal_parameters_with_defaults(&terminal_params_for_id(terminal_id));
    Ok(make_alist(merged))
}

/// (set-terminal-parameter TERMINAL PARAMETER VALUE) -> previous value
///
/// Accepts live frame designators in addition to terminal designators.
pub(crate) fn builtin_set_terminal_parameter(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-terminal-parameter", &args, 3)?;
    let Some(terminal_id) = decode_terminal_id_eval(eval, &args[0]) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), args[0]],
        ));
    };
    if args[1].is_string() {
        return Ok(Value::NIL);
    }
    let key = args[1];
    Ok(update_terminal_param(terminal_id, key, args[2]))
}

// ---------------------------------------------------------------------------
// TTY builtins (we are not a TTY, so these return nil)
// ---------------------------------------------------------------------------

/// (tty-type &optional TERMINAL) -> nil
pub(crate) fn builtin_tty_type(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("tty-type", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    Ok(terminal_runtime_for_id(terminal_id)
        .tty_type
        .map(Value::string)
        .unwrap_or(Value::NIL))
}

/// (tty-top-frame &optional TERMINAL) -> nil
pub(crate) fn builtin_tty_top_frame(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("tty-top-frame", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    let runtime = terminal_runtime_for_id(terminal_id);
    if !runtime.active {
        return Ok(Value::NIL);
    }
    let top = eval
        .frames
        .top_frame_on_terminal(terminal_id)
        .map(|frame_id| Value::make_frame(frame_id.0))
        .unwrap_or(Value::NIL);
    Ok(top)
}

/// (tty-display-color-p &optional TERMINAL) -> nil
pub(crate) fn builtin_tty_display_color_p(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("tty-display-color-p", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    Ok(Value::bool_val(
        terminal_runtime_for_id(terminal_id).supports_color(),
    ))
}

/// (tty-display-color-cells &optional TERMINAL) -> 0
pub(crate) fn builtin_tty_display_color_cells(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("tty-display-color-cells", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    Ok(Value::fixnum(
        terminal_runtime_for_id(terminal_id).color_cells,
    ))
}

/// (tty-no-underline &optional TERMINAL) -> nil
pub(crate) fn builtin_tty_no_underline(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("tty-no-underline", &args, 1)?;
    if let Some(terminal) = args.first()
        && decode_terminal_id_eval(eval, terminal).is_none()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), *terminal],
        ));
    }
    Ok(Value::NIL)
}

/// (controlling-tty-p &optional TERMINAL) -> nil
pub(crate) fn builtin_controlling_tty_p(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("controlling-tty-p", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    Ok(Value::bool_val(
        terminal_runtime_for_id(terminal_id).controlling_tty,
    ))
}

/// (suspend-tty &optional TTY) -> error in GUI/non-text terminal context.
pub(crate) fn builtin_suspend_tty(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("suspend-tty", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    let runtime = terminal_runtime_for_id(terminal_id);
    if !runtime.active {
        return Err(signal(
            "error",
            vec![Value::string(
                "Attempt to suspend a non-text terminal device",
            )],
        ));
    }

    if runtime.suspended {
        return Ok(Value::NIL);
    }

    let terminal = terminal_handle_value_for_id(terminal_id).unwrap_or_else(terminal_handle_value);
    let hook_sym =
        crate::emacs_core::hook_runtime::hook_symbol_by_name(eval, "suspend-tty-functions");
    let _ = crate::emacs_core::hook_runtime::run_named_hook(eval, hook_sym, &[terminal])?;
    with_terminal_host_for_id(terminal_id, |host| host.suspend_tty())?;
    TERMINAL_MANAGER.with(|slot| {
        let mut manager = slot.borrow_mut();
        if let Some(terminal) = manager.get_mut(terminal_id) {
            terminal.runtime.suspended = true;
        }
    });
    Ok(Value::NIL)
}

/// (resume-tty &optional TTY) -> error in GUI/non-text terminal context.
pub(crate) fn builtin_resume_tty(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("resume-tty", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    let runtime = terminal_runtime_for_id(terminal_id);
    if !runtime.active {
        return Err(signal(
            "error",
            vec![Value::string(
                "Attempt to resume a non-text terminal device",
            )],
        ));
    }

    if !runtime.suspended {
        return Ok(Value::NIL);
    }

    with_terminal_host_for_id(terminal_id, |host| host.resume_tty())?;
    TERMINAL_MANAGER.with(|slot| {
        let mut manager = slot.borrow_mut();
        if let Some(terminal) = manager.get_mut(terminal_id) {
            terminal.runtime.suspended = false;
        }
    });
    let terminal = terminal_handle_value_for_id(terminal_id).unwrap_or_else(terminal_handle_value);
    let hook_sym =
        crate::emacs_core::hook_runtime::hook_symbol_by_name(eval, "resume-tty-functions");
    let _ = crate::emacs_core::hook_runtime::run_named_hook(eval, hook_sym, &[terminal])?;
    Ok(Value::NIL)
}

// ---------------------------------------------------------------------------
// Builtins moved from builtins.rs
// ---------------------------------------------------------------------------

pub(crate) fn delete_terminal_owned(
    eval: &mut crate::emacs_core::eval::Context,
    terminal_id: u64,
    mode: DeleteTerminalMode,
) -> EvalResult {
    let active_live_count =
        TERMINAL_MANAGER.with(|slot| slot.borrow().active_live_terminal_count());
    if !mode.bypasses_active_terminal_check() && active_live_count <= 1 {
        return Err(signal(
            "error",
            vec![Value::string(
                "Attempt to delete the sole active display terminal",
            )],
        ));
    }
    let terminal = terminal_handle_value_for_id(terminal_id).unwrap_or_else(terminal_handle_value);
    if mode.runs_hooks_immediately() {
        let hook_sym =
            crate::emacs_core::hook_runtime::hook_symbol_by_name(eval, "delete-terminal-functions");
        let _ = crate::emacs_core::hook_runtime::safe_run_named_hook(eval, hook_sym, &[terminal])?;
    } else {
        eval.queue_pending_safe_hook("delete-terminal-functions", &[terminal]);
    }
    let host_delete = TERMINAL_MANAGER.with(|slot| {
        let mut manager = slot.borrow_mut();
        let Some(host) = manager
            .get_mut(terminal_id)
            .and_then(|terminal| terminal.host.as_deref_mut())
        else {
            return Ok(());
        };
        host.delete_terminal()
    });
    if let Err(message) = host_delete {
        if mode.ignore_host_delete_errors() {
            tracing::warn!(
                "terminal owner: ignoring host delete failure during noelisp teardown: {}",
                message
            );
        } else {
            return Err(signal("error", vec![Value::string(message)]));
        }
    }

    let frames_to_delete = eval
        .frames
        .frame_list()
        .into_iter()
        .filter(|frame_id| {
            eval.frames
                .get(*frame_id)
                .is_some_and(|frame| frame.terminal_id == terminal_id)
        })
        .collect::<Vec<_>>();
    for frame_id in frames_to_delete {
        let _ = crate::emacs_core::window_cmds::delete_frame_owned(
            eval,
            frame_id,
            crate::emacs_core::window_cmds::DeleteFrameMode::Noelisp,
        )?;
    }
    delete_terminal_record(terminal_id);
    eval.command_loop
        .keyboard
        .delete_terminal_kboard(terminal_id);
    if eval.frames.selected_frame().is_none()
        && let Some(next_selected) = eval.frames.frame_list().into_iter().next()
    {
        let _ = eval.frames.select_frame(next_selected);
    }
    eval.sync_keyboard_terminal_owner();
    Ok(Value::NIL)
}

pub(crate) fn delete_terminal_noelisp_owned(
    eval: &mut crate::emacs_core::eval::Context,
    terminal_id: u64,
) -> EvalResult {
    delete_terminal_owned(eval, terminal_id, DeleteTerminalMode::Noelisp)
}

/// (delete-terminal &optional TERMINAL FORCE) -> nil or error
pub(crate) fn builtin_delete_terminal(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("delete-terminal", &args, 0, 2)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Ok(Value::NIL);
    };
    let force_non_nil = args.get(1).copied().unwrap_or(Value::NIL).is_truthy();
    delete_terminal_owned(
        eval,
        terminal_id,
        DeleteTerminalMode::Public { force_non_nil },
    )
}
