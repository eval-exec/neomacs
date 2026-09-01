//! Mode system — major and minor modes.
//!
//! Implements the Emacs mode system:
//! - Major mode registration and switching
//! - Minor mode tracking (global and buffer-local)
//! - Mode hooks (run on mode activation)
//! - Mode-line format composition
//! - Font-lock keyword compilation and application
//! - Defcustom/defgroup for user customization

use std::collections::HashMap;

use super::intern::intern;
use super::value::Value;
use crate::emacs_core::SymId;
use crate::gc_trace::GcTrace;
use crate::heap_types::LispString;

// ---------------------------------------------------------------------------
// Font-lock
// ---------------------------------------------------------------------------

/// FontLock keyword pattern — describes one highlighting rule.
pub struct FontLockKeyword {
    /// Regex pattern to match.
    pub pattern: LispString,
    /// Face symbol to apply (e.g. `font-lock-keyword-face`).
    pub face: SymId,
    /// Regex capture group (0 = whole match).
    pub group: usize,
    /// Whether to override existing fontification.
    pub override_: bool,
    /// Don't error if group doesn't match.
    pub laxmatch: bool,
}

/// Font-lock decoration level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FontLockLevel {
    /// Minimal highlighting.
    Level1,
    /// Low highlighting.
    Level2,
    /// Medium highlighting (default).
    #[default]
    Level3,
    /// High highlighting.
    Level4,
}

/// Font-lock configuration for a mode.
pub struct FontLockDefaults {
    /// Keyword rules for this mode.
    pub keywords: Vec<FontLockKeyword>,
    /// Whether pattern matching is case-insensitive.
    pub case_fold: bool,
    /// Optional syntax table name.
    pub syntax_table: Option<LispString>,
}

// ---------------------------------------------------------------------------
// Major mode
// ---------------------------------------------------------------------------

/// A major mode definition.
pub struct MajorMode {
    /// Human-readable name, e.g. "Emacs-Lisp".
    pub pretty_name: LispString,
    /// Parent mode this mode derives from (if any).
    pub parent: Option<Value>,
    /// Hook variable symbol, e.g. `emacs-lisp-mode-hook`.
    pub mode_hook: Value,
    /// Symbol naming the keymap associated with this mode.
    pub keymap_name: Option<Value>,
    /// Symbol naming the syntax table associated with this mode.
    pub syntax_table_name: Option<Value>,
    /// Symbol naming the abbrev table associated with this mode.
    pub abbrev_table_name: Option<Value>,
    /// Font-lock defaults for this mode.
    pub font_lock: Option<FontLockDefaults>,
    /// Lisp body to evaluate when the mode is entered.
    pub body: Option<Value>,
}

// ---------------------------------------------------------------------------
// Minor mode
// ---------------------------------------------------------------------------

/// A minor mode definition.
pub struct MinorMode {
    /// Mode-line lighter string, e.g. " Fill".
    pub lighter: Option<LispString>,
    /// Symbol naming the keymap associated with this minor mode.
    pub keymap_name: Option<Value>,
    /// Whether this is a global minor mode.
    pub global: bool,
    /// Lisp body to evaluate when toggling.
    pub body: Option<Value>,
}

// ---------------------------------------------------------------------------
// Custom variable / group (defcustom / defgroup)
// ---------------------------------------------------------------------------

/// A customizable variable registered via `defcustom`.
pub struct CustomVariable {
    /// Default value.
    pub default_value: Value,
    /// Docstring.
    pub doc: Option<LispString>,
    /// Type specification.
    pub type_: CustomType,
    /// Customization group symbol this variable belongs to.
    pub group: Option<Value>,
    /// Setter function symbol (`:set`).
    pub set_function: Option<Value>,
    /// Getter function symbol (`:get`).
    pub get_function: Option<Value>,
    /// Tag for display purposes.
    pub tag: Option<LispString>,
}

/// Type descriptor for a `defcustom` variable.
pub enum CustomType {
    Boolean,
    Integer,
    Float,
    String,
    Symbol,
    Sexp,
    Choice(Vec<(String, Value)>),
    List(Box<CustomType>),
    Alist(Box<CustomType>, Box<CustomType>),
    Plist(Box<CustomType>, Box<CustomType>),
    Color,
    Face,
    File,
    Directory,
    Function,
    Variable,
    Hook,
    Coding,
}

/// A customization group registered via `defgroup`.
pub struct CustomGroup {
    /// Docstring.
    pub doc: Option<LispString>,
    /// Parent group symbol.
    pub parent: Option<Value>,
    /// Member variable or sub-group symbols.
    pub members: Vec<Value>,
}

// ---------------------------------------------------------------------------
// Mode-line format
// ---------------------------------------------------------------------------

/// A format specification for mode-line rendering.
pub struct ModeLineFormat {
    pub elements: Vec<ModeLineElement>,
}

fn mode_symbol(name: &str) -> Value {
    Value::symbol(name)
}

fn mode_symbol_id(name: &str) -> SymId {
    intern(name)
}

fn mode_symbol_name(value: Value) -> &'static str {
    value
        .as_symbol_name()
        .expect("mode identity should be stored as a symbol")
}

fn mode_display_text(value: &LispString) -> String {
    // Issue #131: mode-line display text is a faithful UTF-8 rendering of the
    // string's Emacs bytes (a real glyph survives; eight-bit becomes U+FFFD),
    // not the retired in-Unicode storage sentinels.
    crate::emacs_core::emacs_char::to_utf8_lossy(value.as_bytes())
}

/// Individual element in a mode-line format.
pub enum ModeLineElement {
    /// Literal text.
    Literal(String),
    /// Buffer name (%b).
    BufferName,
    /// Current major mode name.
    ModeName,
    /// Active minor modes.
    MinorModes,
    /// Cursor position as line:col.
    Position,
    /// Percentage through the buffer (XX%).
    Percentage,
    /// Modified indicator (** or --).
    Modified,
    /// Read-only indicator (%% or --).
    ReadOnly,
    /// Buffer encoding.
    Encoding,
    /// End-of-line convention (:LF, :CRLF, :CR).
    Eol,
    /// Custom elisp expression to evaluate.
    Custom(String),
}

impl ModeLineFormat {
    /// Return the standard Emacs-like default mode-line format.
    pub fn default_format() -> Self {
        ModeLineFormat {
            elements: vec![
                ModeLineElement::Literal(" ".to_string()),
                ModeLineElement::Modified,
                ModeLineElement::Literal(" ".to_string()),
                ModeLineElement::BufferName,
                ModeLineElement::Literal("  ".to_string()),
                ModeLineElement::Position,
                ModeLineElement::Literal("  ".to_string()),
                ModeLineElement::Percentage,
                ModeLineElement::Literal("  (".to_string()),
                ModeLineElement::ModeName,
                ModeLineElement::MinorModes,
                ModeLineElement::Literal(")".to_string()),
            ],
        }
    }

    /// Render the mode-line to a string for the given buffer.
    #[allow(clippy::too_many_arguments)] // mode-line fields mirror the renderer's public inputs
    pub fn render(
        &self,
        buffer_id: u64,
        registry: &ModeRegistry,
        buffer_name: &str,
        modified: bool,
        read_only: bool,
        line: usize,
        col: usize,
        percent: u8,
    ) -> String {
        let mut out = String::new();
        for elem in &self.elements {
            match elem {
                ModeLineElement::Literal(s) => out.push_str(s),
                ModeLineElement::BufferName => out.push_str(buffer_name),
                ModeLineElement::ModeName => {
                    let mode_name = registry.get_major_mode(buffer_id);
                    if let Some(mode) = registry.major_modes.get(&mode_symbol_id(mode_name)) {
                        out.push_str(&mode_display_text(&mode.pretty_name));
                    } else {
                        out.push_str(mode_name);
                    }
                }
                ModeLineElement::MinorModes => {
                    for minor_name in registry.active_minor_modes(buffer_id) {
                        if let Some(mode) = registry.minor_modes.get(&mode_symbol_id(minor_name))
                            && let Some(ref lighter) = mode.lighter
                        {
                            out.push_str(&mode_display_text(lighter));
                        }
                    }
                }
                ModeLineElement::Position => {
                    out.push_str(&format!("{}:{}", line, col));
                }
                ModeLineElement::Percentage => {
                    if percent == 0 {
                        out.push_str("Top");
                    } else if percent >= 100 {
                        out.push_str("Bot");
                    } else {
                        out.push_str(&format!("{}%", percent));
                    }
                }
                ModeLineElement::Modified => {
                    if read_only {
                        out.push_str("%%");
                    } else if modified {
                        out.push_str("**");
                    } else {
                        out.push_str("--");
                    }
                }
                ModeLineElement::ReadOnly => {
                    if read_only {
                        out.push_str("%%");
                    } else {
                        out.push_str("--");
                    }
                }
                ModeLineElement::Encoding => {
                    out.push('U');
                }
                ModeLineElement::Eol => {
                    out.push_str(":LF");
                }
                ModeLineElement::Custom(expr) => {
                    // Custom expressions require an evaluator — just show the raw form here.
                    out.push_str(&format!("[{}]", expr));
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// ModeRegistry — central manager
// ---------------------------------------------------------------------------

/// Central registry for all mode-related state.
pub struct ModeRegistry {
    /// All registered major modes (name -> definition).
    major_modes: HashMap<SymId, MajorMode>,
    /// All registered minor modes (name -> definition).
    minor_modes: HashMap<SymId, MinorMode>,
    /// Per-buffer active major mode (buffer_id -> mode symbol).
    buffer_major_modes: HashMap<u64, Value>,
    /// Per-buffer active minor modes (buffer_id -> list of mode symbols).
    buffer_minor_modes: HashMap<u64, Vec<Value>>,
    /// Globally active minor modes.
    global_minor_modes: Vec<Value>,
    /// Filename pattern -> mode symbol for automatic mode selection.
    auto_mode_alist: Vec<(LispString, Value)>,
    /// All registered custom variables.
    custom_variables: HashMap<SymId, CustomVariable>,
    /// All registered custom groups.
    custom_groups: HashMap<SymId, CustomGroup>,
    /// Symbol of the fundamental mode (always registered).
    fundamental_mode: Value,
}

impl ModeRegistry {
    /// Create a new registry with `fundamental-mode` pre-registered.
    pub fn new() -> Self {
        let mut reg = ModeRegistry {
            major_modes: HashMap::new(),
            minor_modes: HashMap::new(),
            buffer_major_modes: HashMap::new(),
            buffer_minor_modes: HashMap::new(),
            global_minor_modes: Vec::new(),
            auto_mode_alist: Vec::new(),
            custom_variables: HashMap::new(),
            custom_groups: HashMap::new(),
            fundamental_mode: mode_symbol("fundamental-mode"),
        };
        reg.register_fundamental_mode();
        reg
    }

    // -------------------------------------------------------------------
    // Major mode operations
    // -------------------------------------------------------------------

    /// Register a major mode definition.
    pub fn register_major_mode(&mut self, name: &str, mode: MajorMode) {
        self.major_modes.insert(mode_symbol_id(name), mode);
    }

    /// Set the major mode for a buffer. Replaces any existing major mode.
    /// Returns an error if the mode is not registered.
    pub fn set_major_mode(&mut self, buffer_id: u64, mode_name: &str) -> Result<(), String> {
        if !self.major_modes.contains_key(&mode_symbol_id(mode_name)) {
            return Err(format!("Unknown major mode: {}", mode_name));
        }
        self.buffer_major_modes
            .insert(buffer_id, mode_symbol(mode_name));
        Ok(())
    }

    /// Return the active major mode name for a buffer (defaults to fundamental-mode).
    pub fn get_major_mode(&self, buffer_id: u64) -> &str {
        self.buffer_major_modes
            .get(&buffer_id)
            .copied()
            .map(mode_symbol_name)
            .unwrap_or_else(|| mode_symbol_name(self.fundamental_mode))
    }

    /// Look up the best-matching mode for a filename via `auto-mode-alist`.
    /// Patterns are matched as suffix (ending) of the filename, like Emacs.
    pub fn mode_for_file(&self, filename: &str) -> Option<&str> {
        for (pattern, mode_name) in &self.auto_mode_alist {
            if filename_matches_pattern(filename, pattern) {
                return Some(mode_symbol_name(*mode_name));
            }
        }
        None
    }

    /// Return the `MajorMode` definition for a mode name, if registered.
    pub fn get_major_mode_def(&self, mode_name: &str) -> Option<&MajorMode> {
        self.major_modes.get(&mode_symbol_id(mode_name))
    }

    /// Check whether `mode_name` is derived from `ancestor`.
    /// A mode derives from itself.
    pub fn derived_mode_p(&self, mode_name: &str, ancestor: &str) -> bool {
        let mut current = Some(mode_symbol(mode_name));
        while let Some(name) = current {
            let name_str = mode_symbol_name(name);
            if name_str == ancestor {
                return true;
            }
            current = self
                .major_modes
                .get(&mode_symbol_id(name_str))
                .and_then(|m| m.parent);
        }
        false
    }

    // -------------------------------------------------------------------
    // Minor mode operations
    // -------------------------------------------------------------------

    /// Register a minor mode definition.
    pub fn register_minor_mode(&mut self, name: &str, mode: MinorMode) {
        self.minor_modes.insert(mode_symbol_id(name), mode);
    }

    /// Enable a minor mode in a specific buffer.
    pub fn enable_minor_mode(&mut self, buffer_id: u64, mode_name: &str) -> Result<(), String> {
        if !self.minor_modes.contains_key(&mode_symbol_id(mode_name)) {
            return Err(format!("Unknown minor mode: {}", mode_name));
        }
        let mode_symbol = mode_symbol(mode_name);
        let modes = self.buffer_minor_modes.entry(buffer_id).or_default();
        if !modes.contains(&mode_symbol) {
            modes.push(mode_symbol);
        }
        Ok(())
    }

    /// Disable a minor mode in a specific buffer.
    pub fn disable_minor_mode(&mut self, buffer_id: u64, mode_name: &str) {
        if let Some(modes) = self.buffer_minor_modes.get_mut(&buffer_id) {
            let mode_symbol = mode_symbol(mode_name);
            modes.retain(|m| *m != mode_symbol);
        }
    }

    /// Toggle a minor mode in a specific buffer. Returns `Ok(true)` if the
    /// mode is now active, `Ok(false)` if it was disabled.
    pub fn toggle_minor_mode(&mut self, buffer_id: u64, mode_name: &str) -> Result<bool, String> {
        if !self.minor_modes.contains_key(&mode_symbol_id(mode_name)) {
            return Err(format!("Unknown minor mode: {}", mode_name));
        }
        if self.is_minor_mode_active(buffer_id, mode_name) {
            self.disable_minor_mode(buffer_id, mode_name);
            Ok(false)
        } else {
            self.enable_minor_mode(buffer_id, mode_name)?;
            Ok(true)
        }
    }

    /// Check if a minor mode is active in a buffer (buffer-local or global).
    pub fn is_minor_mode_active(&self, buffer_id: u64, mode_name: &str) -> bool {
        let mode_symbol = mode_symbol(mode_name);
        // Check buffer-local first.
        if let Some(modes) = self.buffer_minor_modes.get(&buffer_id)
            && modes.contains(&mode_symbol)
        {
            return true;
        }
        // Check global.
        self.global_minor_modes.contains(&mode_symbol)
    }

    /// Return all active minor modes for a buffer (buffer-local + global).
    pub fn active_minor_modes(&self, buffer_id: u64) -> Vec<&str> {
        let mut result: Vec<&str> = Vec::new();
        // Global minor modes first (like Emacs).
        for name in &self.global_minor_modes {
            result.push(mode_symbol_name(*name));
        }
        // Then buffer-local, avoiding duplicates.
        if let Some(modes) = self.buffer_minor_modes.get(&buffer_id) {
            for name in modes {
                let name = mode_symbol_name(*name);
                if !result.contains(&name) {
                    result.push(name);
                }
            }
        }
        result
    }

    // -------------------------------------------------------------------
    // Global minor modes
    // -------------------------------------------------------------------

    /// Enable a minor mode globally.
    pub fn enable_global_minor_mode(&mut self, mode_name: &str) -> Result<(), String> {
        if !self.minor_modes.contains_key(&mode_symbol_id(mode_name)) {
            return Err(format!("Unknown minor mode: {}", mode_name));
        }
        let mode_symbol = mode_symbol(mode_name);
        if !self.global_minor_modes.contains(&mode_symbol) {
            self.global_minor_modes.push(mode_symbol);
        }
        Ok(())
    }

    /// Disable a globally-active minor mode.
    pub fn disable_global_minor_mode(&mut self, mode_name: &str) {
        let mode_symbol = mode_symbol(mode_name);
        self.global_minor_modes.retain(|m| *m != mode_symbol);
    }

    // -------------------------------------------------------------------
    // Auto-mode
    // -------------------------------------------------------------------

    /// Add an entry to the auto-mode-alist (pattern -> mode name).
    /// Patterns are suffix-matched against filenames (similar to Emacs
    /// `auto-mode-alist` regex patterns like `"\\.rs\\'"` which match file
    /// endings).  Here we use simple suffix matching: if the filename ends
    /// with `pattern`, it matches.
    pub fn add_auto_mode(&mut self, pattern: LispString, mode: Value) {
        self.auto_mode_alist.push((pattern, mode));
    }

    // -------------------------------------------------------------------
    // Custom variables / groups
    // -------------------------------------------------------------------

    /// Register a custom variable.
    pub fn register_custom_variable(&mut self, name: &str, var: CustomVariable) {
        if let Some(group_name) = var.group
            && let Some(group_symbol) = group_name.as_symbol_id()
            && let Some(group) = self.custom_groups.get_mut(&group_symbol)
        {
            let member = mode_symbol(name);
            if !group.members.contains(&member) {
                group.members.push(member);
            }
        }
        self.custom_variables.insert(mode_symbol_id(name), var);
    }

    /// Register a custom group.
    pub fn register_custom_group(&mut self, name: &str, group: CustomGroup) {
        self.custom_groups.insert(mode_symbol_id(name), group);
    }

    /// Look up a custom variable by name.
    pub fn get_custom_variable(&self, name: &str) -> Option<&CustomVariable> {
        self.custom_variables.get(&mode_symbol_id(name))
    }

    /// Look up a custom group by name.
    pub fn get_custom_group(&self, name: &str) -> Option<&CustomGroup> {
        self.custom_groups.get(&mode_symbol_id(name))
    }

    // -------------------------------------------------------------------
    // Font-lock
    // -------------------------------------------------------------------

    /// Return the font-lock keywords for a mode (walking the parent chain).
    pub fn font_lock_keywords(&self, mode_name: &str) -> Option<&[FontLockKeyword]> {
        let mut current = Some(mode_symbol(mode_name));
        while let Some(name) = current {
            if let Some(mode) = self
                .major_modes
                .get(&mode_symbol_id(mode_symbol_name(name)))
            {
                if let Some(ref fl) = mode.font_lock {
                    return Some(&fl.keywords);
                }
                current = mode.parent;
            } else {
                break;
            }
        }
        None
    }

    // -------------------------------------------------------------------
    // Mode-line
    // -------------------------------------------------------------------

    /// Produce a simple mode-line string for a buffer.
    ///
    /// This is a convenience that builds the string from the major mode's
    /// pretty name and the lighters of active minor modes.
    pub fn mode_line_string(&self, buffer_id: u64) -> String {
        let major = self.get_major_mode(buffer_id);
        let pretty = self
            .major_modes
            .get(&mode_symbol_id(major))
            .map(|m| mode_display_text(&m.pretty_name))
            .unwrap_or_else(|| major.to_string());

        let mut parts = vec![pretty];

        for minor_name in self.active_minor_modes(buffer_id) {
            if let Some(mode) = self.minor_modes.get(&mode_symbol_id(minor_name))
                && let Some(ref lighter) = mode.lighter
            {
                parts.push(mode_display_text(lighter));
            }
        }

        format!("({})", parts.join(""))
    }

    // -------------------------------------------------------------------
    // Clean up
    // -------------------------------------------------------------------

    /// Remove all mode state associated with a buffer (e.g. when the buffer
    /// is killed).
    pub fn remove_buffer(&mut self, buffer_id: u64) {
        self.buffer_major_modes.remove(&buffer_id);
        self.buffer_minor_modes.remove(&buffer_id);
    }

    // -------------------------------------------------------------------
    // Internal
    // -------------------------------------------------------------------

    /// Pre-register the fundamental mode.
    fn register_fundamental_mode(&mut self) {
        let mode = MajorMode {
            pretty_name: LispString::from_utf8("Fundamental"),
            parent: None,
            mode_hook: mode_symbol("fundamental-mode-hook"),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        };
        self.major_modes
            .insert(mode_symbol_id("fundamental-mode"), mode);
    }

    // pdump accessors
    pub(crate) fn dump_major_modes(&self) -> &HashMap<SymId, MajorMode> {
        &self.major_modes
    }
    pub(crate) fn dump_minor_modes(&self) -> &HashMap<SymId, MinorMode> {
        &self.minor_modes
    }
    pub(crate) fn dump_buffer_major_modes(&self) -> &HashMap<u64, Value> {
        &self.buffer_major_modes
    }
    pub(crate) fn dump_buffer_minor_modes(&self) -> &HashMap<u64, Vec<Value>> {
        &self.buffer_minor_modes
    }
    pub(crate) fn dump_global_minor_modes(&self) -> &[Value] {
        &self.global_minor_modes
    }
    pub(crate) fn dump_auto_mode_alist(&self) -> &[(LispString, Value)] {
        &self.auto_mode_alist
    }
    pub(crate) fn dump_custom_variables(&self) -> &HashMap<SymId, CustomVariable> {
        &self.custom_variables
    }
    pub(crate) fn dump_custom_groups(&self) -> &HashMap<SymId, CustomGroup> {
        &self.custom_groups
    }
    pub(crate) fn dump_fundamental_mode(&self) -> Value {
        self.fundamental_mode
    }
    #[allow(clippy::too_many_arguments)] // restores the manager's independent persisted tables
    pub(crate) fn from_dump(
        major_modes: HashMap<SymId, MajorMode>,
        minor_modes: HashMap<SymId, MinorMode>,
        buffer_major_modes: HashMap<u64, Value>,
        buffer_minor_modes: HashMap<u64, Vec<Value>>,
        global_minor_modes: Vec<Value>,
        auto_mode_alist: Vec<(LispString, Value)>,
        custom_variables: HashMap<SymId, CustomVariable>,
        custom_groups: HashMap<SymId, CustomGroup>,
        fundamental_mode: Value,
    ) -> Self {
        Self {
            major_modes,
            minor_modes,
            buffer_major_modes,
            buffer_minor_modes,
            global_minor_modes,
            auto_mode_alist,
            custom_variables,
            custom_groups,
            fundamental_mode,
        }
    }
}

impl Default for ModeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Pattern matching helper
// ---------------------------------------------------------------------------

/// Simple suffix-match for auto-mode-alist patterns.
///
/// If `pattern` starts with '.', we check if `filename` ends with `pattern`.
/// Otherwise we check if `filename` ends with `pattern` OR equals `pattern`.
fn filename_matches_pattern(filename: &str, pattern: &LispString) -> bool {
    filename.ends_with(&mode_display_text(pattern))
}

// ---------------------------------------------------------------------------
// GcTrace
// ---------------------------------------------------------------------------

impl GcTrace for ModeRegistry {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        for mode in self.major_modes.values() {
            if let Some(parent) = mode.parent {
                roots.push(parent);
            }
            roots.push(mode.mode_hook);
            if let Some(keymap_name) = mode.keymap_name {
                roots.push(keymap_name);
            }
            if let Some(syntax_table_name) = mode.syntax_table_name {
                roots.push(syntax_table_name);
            }
            if let Some(abbrev_table_name) = mode.abbrev_table_name {
                roots.push(abbrev_table_name);
            }
            if let Some(body) = &mode.body {
                roots.push(*body);
            }
        }
        for mode in self.minor_modes.values() {
            if let Some(keymap_name) = mode.keymap_name {
                roots.push(keymap_name);
            }
            if let Some(body) = &mode.body {
                roots.push(*body);
            }
        }
        for mode in self.buffer_major_modes.values() {
            roots.push(*mode);
        }
        for modes in self.buffer_minor_modes.values() {
            roots.extend(modes.iter().copied());
        }
        roots.extend(self.global_minor_modes.iter().copied());
        roots.extend(self.auto_mode_alist.iter().map(|(_, mode)| *mode));
        roots.push(self.fundamental_mode);
        for var in self.custom_variables.values() {
            roots.push(var.default_value);
            if let Some(group) = var.group {
                roots.push(group);
            }
            if let Some(set_function) = var.set_function {
                roots.push(set_function);
            }
            if let Some(get_function) = var.get_function {
                roots.push(get_function);
            }
            if let CustomType::Choice(choices) = &var.type_ {
                for (_, v) in choices {
                    roots.push(*v);
                }
            }
        }
        for group in self.custom_groups.values() {
            if let Some(parent) = group.parent {
                roots.push(parent);
            }
            roots.extend(group.members.iter().copied());
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
