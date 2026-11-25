//! The per-terminal display registry (remote-display M3; DESIGN §7.1).
//!
//! `Context.display_terminals` occupies the slot that held the single
//! `Option<Box<dyn DisplayHost>>`. It owns every terminal's native backing on
//! the evaluator's main owner, outside World-carried state
//! (`docs/design/host-effect-projections.md`, "Existing storage and later
//! activation obligations"), and the per-process display services.
//!
//! The registry allocates no terminal numbers. Numbers stay GNU-visible
//! terminal ids owned by the terminal records (`ND/terminal/pure.rs`); the
//! registry mirrors the ones that have native backing.
//!
//! Routing, by call-site class:
//! 1. frame-addressed calls name the frame's terminal
//!    ([`DisplayTerminals::connection_mut`]);
//! 2. frame-less calls name [`DisplayTerminals::default_terminal`], GNU's
//!    `check_x_display_info (Qnil)`;
//! 3. "is there a display" asks [`DisplayTerminals::hosts_display`];
//! 4. settings reach every connection ([`DisplayTerminals::broadcast`]) and
//!    replay when a connection registers;
//! 5. layout reads [`DisplayTerminals::frame_display`]: the services plus the
//!    laid-out frame's terminal connection.

use neomacs_display_protocol::modifier_policy::ModifierPolicy;

use super::tty_host::{TERMINAL_ID, TerminalHost};
use super::{DisplayConnection, DisplayHost, DisplayServices};
use crate::host_effect::terminal_device::{
    TerminalDeviceEffect, TerminalDeviceReply, TerminalDeviceRequest,
};

/// The terminal identity lives with the other display ids in `neovm-value`,
/// where the host-effect vocabulary (`host_effect/contracts.rs`) can name it.
pub use neovm_value::display_vocab::TerminalNumber;

// The registry's initial terminal is the terminal records' initial terminal.
const _: () = assert!(TerminalNumber::INITIAL.get() == TERMINAL_ID);

/// The display object of one terminal.
///
/// Either a connection alone, or a [`DisplayHost`] that is also the process
/// display services. Only the registry's install path builds the second
/// form, so one services provider exists at a time.
pub struct TerminalDisplay(DisplayObject);

enum DisplayObject {
    Connection(Box<dyn DisplayConnection>),
    /// Also answers [`DisplayTerminals::services`] (`ProcessServices::OfTerminal`).
    Host(Box<dyn DisplayHost>),
}

impl TerminalDisplay {
    pub fn new(connection: Box<dyn DisplayConnection>) -> Self {
        Self(DisplayObject::Connection(connection))
    }

    fn with_services(host: Box<dyn DisplayHost>) -> Self {
        Self(DisplayObject::Host(host))
    }

    pub fn connection(&self) -> &(dyn DisplayConnection + 'static) {
        match &self.0 {
            DisplayObject::Connection(connection) => &**connection,
            DisplayObject::Host(host) => &**host,
        }
    }

    pub fn connection_mut(&mut self) -> &mut (dyn DisplayConnection + 'static) {
        match &mut self.0 {
            DisplayObject::Connection(connection) => &mut **connection,
            DisplayObject::Host(host) => &mut **host,
        }
    }

    fn services(&self) -> Option<&(dyn DisplayServices + 'static)> {
        match &self.0 {
            DisplayObject::Connection(_) => None,
            DisplayObject::Host(host) => Some(&**host),
        }
    }

    fn services_mut(&mut self) -> Option<&mut (dyn DisplayServices + 'static)> {
        match &mut self.0 {
            DisplayObject::Connection(_) => None,
            DisplayObject::Host(host) => Some(&mut **host),
        }
    }

    fn provides_services(&self) -> bool {
        matches!(self.0, DisplayObject::Host(_))
    }
}

/// A text terminal's native backing: its device (suspend, resume, delete,
/// raw output, winsize) and, for the process's primary TTY, the popup and
/// selection half that occupied the single display slot before M3.
///
/// The two halves have independent lifetimes, as the device record and the
/// display slot had before M3: deleting or resetting the terminal releases
/// the device and leaves the display half in place.
pub struct TtyHost {
    device: Option<Box<dyn TerminalHost>>,
    display: Option<TerminalDisplay>,
}

impl TtyHost {
    pub fn new(device: Box<dyn TerminalHost>, display: Option<Box<dyn DisplayConnection>>) -> Self {
        Self {
            device: Some(device),
            display: display.map(TerminalDisplay::new),
        }
    }

    fn with_display_host(device: Box<dyn TerminalHost>, host: Box<dyn DisplayHost>) -> Self {
        Self {
            device: Some(device),
            display: Some(TerminalDisplay::with_services(host)),
        }
    }

    pub fn device_mut(&mut self) -> Option<&mut (dyn TerminalHost + 'static)> {
        self.device.as_deref_mut()
    }

    fn is_empty(&self) -> bool {
        self.device.is_none() && self.display.is_none()
    }

    pub fn display(&self) -> Option<&(dyn DisplayConnection + 'static)> {
        self.display.as_ref().map(TerminalDisplay::connection)
    }
}

/// One exhaustive enum over every terminal kind's native backing (D12).
pub enum TerminalBacking {
    /// GNU's initial terminal: no device and no display.
    Initial,
    /// A text terminal.
    Termcap(TtyHost),
    /// A window-system display connection.
    WindowSystem(TerminalDisplay),
}

impl TerminalBacking {
    pub fn window_system(connection: Box<dyn DisplayConnection>) -> Self {
        Self::WindowSystem(TerminalDisplay::new(connection))
    }

    fn display(&self) -> Option<&TerminalDisplay> {
        match self {
            Self::Initial => None,
            Self::Termcap(tty) => tty.display.as_ref(),
            Self::WindowSystem(display) => Some(display),
        }
    }

    fn display_mut(&mut self) -> Option<&mut TerminalDisplay> {
        match self {
            Self::Initial => None,
            Self::Termcap(tty) => tty.display.as_mut(),
            Self::WindowSystem(display) => Some(display),
        }
    }
}

/// Where the per-process display services live.
enum ProcessServices {
    Own(Box<dyn DisplayServices>),
    /// The display host on this terminal provides both halves.
    OfTerminal(TerminalNumber),
}

/// How the process display host attaches to its terminal.
pub enum DisplayHostPlacement {
    /// A window-system connection (the GUI host, and test hosts).
    WindowSystem,
    /// The popup and selection half of a text terminal, beside its device.
    Termcap(Box<dyn TerminalHost>),
}

/// A display setting every connection mirrors (class 4). Settings whose
/// source of truth is evaluator state (the visual configuration and
/// `ns-confirm-quit`) are replayed from that state, not stored here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BroadcastSetting {
    AlphaLowerLimit(f32),
    ScrollIndicators(bool),
    ExtraSpacing { line: f32, letter: f32 },
    ModifierPolicy(ModifierPolicy),
}

/// The last value of each [`BroadcastSetting`] that reached a connection.
#[derive(Default)]
struct PublishedSettings {
    alpha_lower_limit: Option<f32>,
    scroll_indicators: Option<bool>,
    extra_spacing: Option<(f32, f32)>,
    modifier_policy: Option<ModifierPolicy>,
}

impl PublishedSettings {
    fn record(&mut self, setting: BroadcastSetting) {
        match setting {
            BroadcastSetting::AlphaLowerLimit(limit) => self.alpha_lower_limit = Some(limit),
            BroadcastSetting::ScrollIndicators(enabled) => self.scroll_indicators = Some(enabled),
            BroadcastSetting::ExtraSpacing { line, letter } => {
                self.extra_spacing = Some((line, letter))
            }
            BroadcastSetting::ModifierPolicy(policy) => self.modifier_policy = Some(policy),
        }
    }

    /// SETTING reached no connection: nothing of its kind is replayed, as
    /// before the registry, when a publication without a display host was a
    /// no-op and a newly installed host received none of these settings.
    fn forget(&mut self, setting: BroadcastSetting) {
        match setting {
            BroadcastSetting::AlphaLowerLimit(_) => self.alpha_lower_limit = None,
            BroadcastSetting::ScrollIndicators(_) => self.scroll_indicators = None,
            BroadcastSetting::ExtraSpacing { .. } => self.extra_spacing = None,
            BroadcastSetting::ModifierPolicy(_) => self.modifier_policy = None,
        }
    }

    fn recorded(&self) -> impl Iterator<Item = BroadcastSetting> + '_ {
        [
            self.alpha_lower_limit
                .map(BroadcastSetting::AlphaLowerLimit),
            self.scroll_indicators
                .map(BroadcastSetting::ScrollIndicators),
            self.extra_spacing
                .map(|(line, letter)| BroadcastSetting::ExtraSpacing { line, letter }),
            self.modifier_policy.map(BroadcastSetting::ModifierPolicy),
        ]
        .into_iter()
        .flatten()
    }
}

fn apply_setting(
    connection: &mut dyn DisplayConnection,
    setting: BroadcastSetting,
) -> Result<(), String> {
    match setting {
        BroadcastSetting::AlphaLowerLimit(limit) => {
            connection.set_gui_frame_alpha_lower_limit(limit);
            Ok(())
        }
        BroadcastSetting::ScrollIndicators(enabled) => connection.set_scroll_indicators(enabled),
        BroadcastSetting::ExtraSpacing { line, letter } => {
            connection.set_extra_spacing(line, letter)
        }
        BroadcastSetting::ModifierPolicy(policy) => connection.set_modifier_policy(policy),
    }
}

/// What layout reads from a display (class 5): the process services and one
/// terminal's connection. It answers exactly as the single host did: media
/// requests that find no connection get the trait defaults' `Ok(None)`.
#[derive(Clone, Copy)]
pub struct FrameDisplay<'a> {
    services: &'a (dyn DisplayServices + 'a),
    connection: Option<&'a (dyn DisplayConnection + 'a)>,
}

impl<'a> FrameDisplay<'a> {
    /// One host object that provides both halves, viewed as a display.
    pub fn of_host(host: &'a (dyn DisplayHost + 'a)) -> Self {
        Self {
            services: host,
            connection: Some(host),
        }
    }

    pub fn services(self) -> &'a (dyn DisplayServices + 'a) {
        self.services
    }

    pub fn connection(self) -> Option<&'a (dyn DisplayConnection + 'a)> {
        self.connection
    }

    pub fn image_catalog(self) -> Option<&'a dyn super::image_catalog::ImageCatalog> {
        self.services.image_catalog()
    }

    pub fn image_catalog_shared(
        self,
    ) -> Option<std::rc::Rc<dyn super::image_catalog::ImageCatalog>> {
        self.services.image_catalog_shared()
    }

    pub fn request_video(
        self,
        request: super::super::VideoResolveRequest,
    ) -> Result<Option<super::super::ResolvedVideo>, String> {
        match self.connection {
            Some(connection) => connection.request_video(request),
            None => Ok(None),
        }
    }

    pub fn request_webkit(
        self,
        request: super::super::WebKitResolveRequest,
    ) -> Result<Option<super::super::ResolvedWebKit>, String> {
        match self.connection {
            Some(connection) => connection.request_webkit(request),
            None => Ok(None),
        }
    }

    pub fn request_surface(
        self,
        request: super::super::SurfaceResolveRequest,
    ) -> Result<Option<super::super::ResolvedSurface>, String> {
        match self.connection {
            Some(connection) => connection.request_surface(request),
            None => Ok(None),
        }
    }
}

/// The main-owned registry of terminal backings and process display services.
///
/// One pointer in the `Context` slot that held `Option<Box<dyn DisplayHost>>`;
/// the registry itself lives on the heap, so no inline Context field grows.
pub struct DisplayTerminals(Box<Registry>);

struct Registry {
    /// Present exactly when a process display is installed (class 3).
    services: Option<ProcessServices>,
    /// Registration order. The last entry that has a display is the head of
    /// GNU's `x_display_list`, which `x_term_init` pushes
    /// (`src/xterm.c:30940-30942`).
    terminals: Vec<(TerminalNumber, TerminalBacking)>,
    published: PublishedSettings,
}

impl Default for DisplayTerminals {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayTerminals {
    /// GNU `init_initial_terminal`: the registry starts with the initial
    /// terminal and no display.
    pub fn new() -> Self {
        Self(Box::new(Registry {
            services: None,
            terminals: vec![(TerminalNumber::INITIAL, TerminalBacking::Initial)],
            published: PublishedSettings::default(),
        }))
    }

    /// Class 3: whether a process display is installed. Before M3 this was
    /// `display_host.is_some()`; the primary display (the GUI host, or the
    /// primary TTY's popup half) and the services are installed together, so
    /// the answer is the same in every mode. M5 answers by terminal kind.
    pub fn hosts_display(&self) -> bool {
        self.0.services.is_some()
    }

    pub fn backing(&self, terminal: TerminalNumber) -> Option<&TerminalBacking> {
        self.0
            .terminals
            .iter()
            .find(|(number, _)| *number == terminal)
            .map(|(_, backing)| backing)
    }

    fn backing_mut(&mut self, terminal: TerminalNumber) -> Option<&mut TerminalBacking> {
        self.0
            .terminals
            .iter_mut()
            .find(|(number, _)| *number == terminal)
            .map(|(_, backing)| backing)
    }

    /// The process display services (fonts, images, the Rows identity).
    pub fn services(&self) -> Option<&(dyn DisplayServices + 'static)> {
        match self.0.services.as_ref()? {
            ProcessServices::Own(services) => Some(&**services),
            ProcessServices::OfTerminal(terminal) => self.backing(*terminal)?.display()?.services(),
        }
    }

    pub fn services_mut(&mut self) -> Option<&mut (dyn DisplayServices + 'static)> {
        let provider = match self.0.services.as_ref()? {
            ProcessServices::Own(_) => None,
            ProcessServices::OfTerminal(terminal) => Some(*terminal),
        };
        match provider {
            Some(terminal) => self.backing_mut(terminal)?.display_mut()?.services_mut(),
            None => match self.0.services.as_mut()? {
                ProcessServices::Own(services) => Some(&mut **services),
                ProcessServices::OfTerminal(_) => None,
            },
        }
    }

    /// Class 1: the display connection of TERMINAL, if it has one.
    pub fn connection(
        &self,
        terminal: TerminalNumber,
    ) -> Option<&(dyn DisplayConnection + 'static)> {
        self.backing(terminal)?
            .display()
            .map(TerminalDisplay::connection)
    }

    pub fn connection_mut(
        &mut self,
        terminal: TerminalNumber,
    ) -> Option<&mut (dyn DisplayConnection + 'static)> {
        self.backing_mut(terminal)?
            .display_mut()
            .map(TerminalDisplay::connection_mut)
    }

    /// Class 2: the terminal a frame-less display request addresses, GNU
    /// `check_x_display_info (Qnil)` (`src/xfns.c:176-185`, UNATTESTED):
    /// SELECTED (the selected frame's terminal) when it has a display, else
    /// the most recently registered terminal that has one. While the primary
    /// TTY's popup half counts as a display, this reproduces the single slot.
    pub fn default_terminal(&self, selected: Option<TerminalNumber>) -> Option<TerminalNumber> {
        if let Some(selected) = selected
            && self.connection(selected).is_some()
        {
            return Some(selected);
        }
        self.0
            .terminals
            .iter()
            .rev()
            .find(|(_, backing)| backing.display().is_some())
            .map(|(number, _)| *number)
    }

    /// Class 2: the connection of [`Self::default_terminal`].
    pub fn default_connection_mut(
        &mut self,
        selected: Option<TerminalNumber>,
    ) -> Option<&mut (dyn DisplayConnection + 'static)> {
        let terminal = self.default_terminal(selected)?;
        self.connection_mut(terminal)
    }

    /// Class 5: the display view for TERMINAL, present exactly when the
    /// process services are installed (the single host's presence).
    pub fn frame_display(&self, terminal: Option<TerminalNumber>) -> Option<FrameDisplay<'_>> {
        Some(FrameDisplay {
            services: self.services()?,
            connection: terminal.and_then(|terminal| self.connection(terminal)),
        })
    }

    /// The terminals that have a display connection, in registration order.
    pub fn connected_terminals(&self) -> Vec<TerminalNumber> {
        self.0
            .terminals
            .iter()
            .filter(|(_, backing)| backing.display().is_some())
            .map(|(number, _)| *number)
            .collect()
    }

    /// A text terminal's device host. Every device effect goes through
    /// [`Self::perform_tty_device`], which a Green worker reaches on main.
    fn tty_device_mut(
        &mut self,
        terminal: TerminalNumber,
    ) -> Option<&mut (dyn TerminalHost + 'static)> {
        match self.backing_mut(terminal)? {
            TerminalBacking::Termcap(tty) => tty.device_mut(),
            TerminalBacking::Initial | TerminalBacking::WindowSystem(_) => None,
        }
    }

    /// Give TERMINAL a text-terminal device, keeping a display half that is
    /// already there.
    pub fn set_tty_device(&mut self, terminal: TerminalNumber, device: Box<dyn TerminalHost>) {
        if let Some(TerminalBacking::Termcap(tty)) = self.backing_mut(terminal) {
            tty.device = Some(device);
            return;
        }
        self.set_backing(
            terminal,
            TerminalBacking::Termcap(TtyHost::new(device, None)),
        );
    }

    /// Perform REQUEST on TERMINAL's text-terminal device. Every owner's
    /// device request ends here, on the owner of this registry: directly, or
    /// served for a Green worker (`green/host_service/terminal_device.rs`).
    /// Callers outside this crate go through `Context::tty_device_request`,
    /// which routes a worker's request here on main.
    pub(crate) fn perform_tty_device(
        &mut self,
        terminal: TerminalNumber,
        request: TerminalDeviceRequest,
    ) -> TerminalDeviceReply {
        let effect = match request {
            TerminalDeviceRequest::Release => {
                self.release_terminal(terminal);
                return TerminalDeviceReply::Done;
            }
            TerminalDeviceRequest::Effect(effect) => effect,
        };
        let Some(device) = self.tty_device_mut(terminal) else {
            return TerminalDeviceReply::NoDevice;
        };
        let result = match effect {
            TerminalDeviceEffect::Suspend => device.suspend_tty(),
            TerminalDeviceEffect::Resume => device.resume_tty(),
            TerminalDeviceEffect::Write(bytes) => device.write_bytes(&bytes),
            TerminalDeviceEffect::Winsize => {
                return TerminalDeviceReply::Winsize(device.tty_winsize_cells());
            }
            TerminalDeviceEffect::Delete => device.delete_terminal(),
        };
        match result {
            Ok(()) => TerminalDeviceReply::Done,
            Err(message) => TerminalDeviceReply::HostError(message),
        }
    }

    /// TERMINAL was deleted: release its device (GNU `delete_terminal`).
    /// Display halves stay where the single slot kept them before M3; M5
    /// closes a deleted terminal's connection.
    fn release_terminal(&mut self, terminal: TerminalNumber) {
        let Some(index) = self
            .0
            .terminals
            .iter()
            .position(|(number, _)| *number == terminal)
        else {
            return;
        };
        let release = match &mut self.0.terminals[index].1 {
            TerminalBacking::Initial => true,
            TerminalBacking::Termcap(tty) => {
                tty.device = None;
                tty.is_empty()
            }
            TerminalBacking::WindowSystem(_) => false,
        };
        if release {
            self.0.terminals.remove(index);
        }
    }

    /// Class 4: apply SETTING to every connection; the first error is
    /// returned. A setting that some connection accepted replays into
    /// connections registered later. One that reached no connection (none
    /// exists yet, or each refused it) replays nowhere, as before the
    /// registry.
    pub fn broadcast(&mut self, setting: BroadcastSetting) -> Result<(), String> {
        let mut result = Ok(());
        let mut accepted = false;
        for (_, backing) in &mut self.0.terminals {
            if let Some(display) = backing.display_mut() {
                let applied = apply_setting(display.connection_mut(), setting);
                accepted |= applied.is_ok();
                if result.is_ok() {
                    result = applied;
                }
            }
        }
        if accepted {
            self.0.published.record(setting);
        } else {
            self.0.published.forget(setting);
        }
        result
    }

    /// Note SETTING for replay without applying it; the caller applies it to
    /// every connection itself (binding publication, whose setter calls the
    /// host-effect guard certifies at the call site). Those setters cannot
    /// refuse, so the setting replays exactly when a connection exists.
    pub fn record_published(&mut self, setting: BroadcastSetting) {
        if self.has_connection() {
            self.0.published.record(setting);
        } else {
            self.0.published.forget(setting);
        }
    }

    fn has_connection(&self) -> bool {
        self.0
            .terminals
            .iter()
            .any(|(_, backing)| backing.display().is_some())
    }

    /// Replay every published setting into TERMINAL's connection.
    pub(crate) fn replay_published(&mut self, terminal: TerminalNumber) {
        let settings: Vec<BroadcastSetting> = self.0.published.recorded().collect();
        if let Some(connection) = self.connection_mut(terminal) {
            for setting in settings {
                // A replay has no Lisp caller to signal to; the connection
                // reports its own failures when Lisp next sets the value.
                let _ = apply_setting(connection, setting);
            }
        }
    }

    /// Set TERMINAL's backing; it becomes the most recently registered
    /// terminal. A backing that provided the process services takes them
    /// along when it is replaced.
    pub(crate) fn set_backing(
        &mut self,
        terminal: TerminalNumber,
        backing: TerminalBacking,
    ) -> Option<TerminalBacking> {
        let previous = self.remove(terminal);
        self.0.terminals.push((terminal, backing));
        previous
    }

    /// Forget TERMINAL's backing (the terminal was deleted).
    pub(crate) fn remove(&mut self, terminal: TerminalNumber) -> Option<TerminalBacking> {
        let index = self
            .0
            .terminals
            .iter()
            .position(|(number, _)| *number == terminal)?;
        let (_, backing) = self.0.terminals.remove(index);
        if backing
            .display()
            .is_some_and(TerminalDisplay::provides_services)
        {
            self.0.services = None;
        }
        Some(backing)
    }

    /// Install the process display as split halves: SERVICES, and BACKING on
    /// TERMINAL.
    pub(crate) fn install_split(
        &mut self,
        services: Box<dyn DisplayServices>,
        terminal: TerminalNumber,
        backing: TerminalBacking,
    ) {
        self.uninstall_display();
        self.set_backing(terminal, backing);
        self.0.services = Some(ProcessServices::Own(services));
    }

    /// Install HOST, which provides both halves, on TERMINAL.
    pub(crate) fn install_host(
        &mut self,
        terminal: TerminalNumber,
        placement: DisplayHostPlacement,
        host: Box<dyn DisplayHost>,
    ) {
        self.uninstall_display();
        let backing = match placement {
            DisplayHostPlacement::WindowSystem => {
                TerminalBacking::WindowSystem(TerminalDisplay::with_services(host))
            }
            DisplayHostPlacement::Termcap(device) => {
                TerminalBacking::Termcap(TtyHost::with_display_host(device, host))
            }
        };
        self.set_backing(terminal, backing);
        self.0.services = Some(ProcessServices::OfTerminal(terminal));
    }

    /// Drop the process display: the services and the display half of the
    /// terminal that provided them. A text terminal keeps its device.
    pub fn uninstall_display(&mut self) {
        match self.0.services.take() {
            None | Some(ProcessServices::Own(_)) => {}
            Some(ProcessServices::OfTerminal(terminal)) => {
                if let Some(backing) = self.backing_mut(terminal) {
                    match backing {
                        TerminalBacking::Termcap(tty) => tty.display = None,
                        TerminalBacking::WindowSystem(_) => *backing = TerminalBacking::Initial,
                        TerminalBacking::Initial => {}
                    }
                }
            }
        }
    }
}
