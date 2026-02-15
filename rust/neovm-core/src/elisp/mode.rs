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

use super::value::Value;

// ---------------------------------------------------------------------------
// Font-lock
// ---------------------------------------------------------------------------

/// FontLock keyword pattern — describes one highlighting rule.
pub struct FontLockKeyword {
    /// Regex pattern to match.
    pub pattern: String,
    /// Face name to apply (e.g. "font-lock-keyword-face").
    pub face: String,
    /// Regex capture group (0 = whole match).
    pub group: usize,
    /// Whether to override existing fontification.
    pub override_: bool,
    /// Don't error if group doesn't match.
    pub laxmatch: bool,
}

/// Font-lock decoration level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontLockLevel {
    /// Minimal highlighting.
    Level1,
    /// Low highlighting.
    Level2,
    /// Medium highlighting (default).
    Level3,
    /// High highlighting.
    Level4,
}

impl Default for FontLockLevel {
    fn default() -> Self {
        FontLockLevel::Level3
    }
}

/// Font-lock configuration for a mode.
pub struct FontLockDefaults {
    /// Keyword rules for this mode.
    pub keywords: Vec<FontLockKeyword>,
    /// Whether pattern matching is case-insensitive.
    pub case_fold: bool,
    /// Optional syntax table name.
    pub syntax_table: Option<String>,
}

// ---------------------------------------------------------------------------
// Major mode
// ---------------------------------------------------------------------------

/// A major mode definition.
pub struct MajorMode {
    /// Symbol name, e.g. "emacs-lisp-mode".
    pub name: String,
    /// Human-readable name, e.g. "Emacs-Lisp".
    pub pretty_name: String,
    /// Parent mode this mode derives from (if any).
    pub parent: Option<String>,
    /// Hook variable name, e.g. "emacs-lisp-mode-hook".
    pub mode_hook: String,
    /// Name of the keymap associated with this mode.
    pub keymap_name: Option<String>,
    /// Name of the syntax table associated with this mode.
    pub syntax_table_name: Option<String>,
    /// Name of the abbrev table associated with this mode.
    pub abbrev_table_name: Option<String>,
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
    /// Symbol name, e.g. "auto-fill-mode".
    pub name: String,
    /// Mode-line lighter string, e.g. " Fill".
    pub lighter: Option<String>,
    /// Name of the keymap associated with this minor mode.
    pub keymap_name: Option<String>,
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
    /// Variable name.
    pub name: String,
    /// Default value.
    pub default_value: Value,
    /// Docstring.
    pub doc: Option<String>,
    /// Type specification.
    pub type_: CustomType,
    /// Customization group this variable belongs to.
    pub group: Option<String>,
    /// Name of the setter function (`:set`).
    pub set_function: Option<String>,
    /// Name of the getter function (`:get`).
    pub get_function: Option<String>,
    /// Tag for display purposes.
    pub tag: Option<String>,
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
    /// Group name.
    pub name: String,
    /// Docstring.
    pub doc: Option<String>,
    /// Parent group.
    pub parent: Option<String>,
    /// Member variable or sub-group names.
    pub members: Vec<String>,
}

// ---------------------------------------------------------------------------
// Mode-line format
// ---------------------------------------------------------------------------

/// A format specification for mode-line rendering.
pub struct ModeLineFormat {
    pub elements: Vec<ModeLineElement>,
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
                    if let Some(mode) = registry.major_modes.get(mode_name) {
                        out.push_str(&mode.pretty_name);
                    } else {
                        out.push_str(mode_name);
                    }
                }
                ModeLineElement::MinorModes => {
                    for minor_name in registry.active_minor_modes(buffer_id) {
                        if let Some(mode) = registry.minor_modes.get(minor_name) {
                            if let Some(ref lighter) = mode.lighter {
                                out.push_str(lighter);
                            }
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
                    out.push_str("U");
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
    major_modes: HashMap<String, MajorMode>,
    /// All registered minor modes (name -> definition).
    minor_modes: HashMap<String, MinorMode>,
    /// Per-buffer active major mode (buffer_id -> mode name).
    buffer_major_modes: HashMap<u64, String>,
    /// Per-buffer active minor modes (buffer_id -> list of mode names).
    buffer_minor_modes: HashMap<u64, Vec<String>>,
    /// Globally active minor modes.
    global_minor_modes: Vec<String>,
    /// Filename pattern -> mode name for automatic mode selection.
    auto_mode_alist: Vec<(String, String)>,
    /// All registered custom variables.
    custom_variables: HashMap<String, CustomVariable>,
    /// All registered custom groups.
    custom_groups: HashMap<String, CustomGroup>,
    /// Name of the fundamental mode (always registered).
    fundamental_mode: String,
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
            fundamental_mode: "fundamental-mode".to_string(),
        };
        reg.register_fundamental_mode();
        reg
    }

    // -------------------------------------------------------------------
    // Major mode operations
    // -------------------------------------------------------------------

    /// Register a major mode definition.
    pub fn register_major_mode(&mut self, mode: MajorMode) {
        self.major_modes.insert(mode.name.clone(), mode);
    }

    /// Set the major mode for a buffer. Replaces any existing major mode.
    /// Returns an error if the mode is not registered.
    pub fn set_major_mode(&mut self, buffer_id: u64, mode_name: &str) -> Result<(), String> {
        if !self.major_modes.contains_key(mode_name) {
            return Err(format!("Unknown major mode: {}", mode_name));
        }
        self.buffer_major_modes
            .insert(buffer_id, mode_name.to_string());
        Ok(())
    }

    /// Return the active major mode name for a buffer (defaults to fundamental-mode).
    pub fn get_major_mode(&self, buffer_id: u64) -> &str {
        self.buffer_major_modes
            .get(&buffer_id)
            .map(|s| s.as_str())
            .unwrap_or(&self.fundamental_mode)
    }

    /// Look up the best-matching mode for a filename via `auto-mode-alist`.
    /// Patterns are matched as suffix (ending) of the filename, like Emacs.
    pub fn mode_for_file(&self, filename: &str) -> Option<&str> {
        for (pattern, mode_name) in &self.auto_mode_alist {
            if filename_matches_pattern(filename, pattern) {
                return Some(mode_name.as_str());
            }
        }
        None
    }

    /// Return the `MajorMode` definition for a mode name, if registered.
    pub fn get_major_mode_def(&self, mode_name: &str) -> Option<&MajorMode> {
        self.major_modes.get(mode_name)
    }

    /// Check whether `mode_name` is derived from `ancestor`.
    /// A mode derives from itself.
    pub fn derived_mode_p(&self, mode_name: &str, ancestor: &str) -> bool {
        let mut current = Some(mode_name.to_string());
        while let Some(name) = current {
            if name == ancestor {
                return true;
            }
            current = self.major_modes.get(&name).and_then(|m| m.parent.clone());
        }
        false
    }

    // -------------------------------------------------------------------
    // Minor mode operations
    // -------------------------------------------------------------------

    /// Register a minor mode definition.
    pub fn register_minor_mode(&mut self, mode: MinorMode) {
        self.minor_modes.insert(mode.name.clone(), mode);
    }

    /// Enable a minor mode in a specific buffer.
    pub fn enable_minor_mode(&mut self, buffer_id: u64, mode_name: &str) -> Result<(), String> {
        if !self.minor_modes.contains_key(mode_name) {
            return Err(format!("Unknown minor mode: {}", mode_name));
        }
        let modes = self
            .buffer_minor_modes
            .entry(buffer_id)
            .or_insert_with(Vec::new);
        if !modes.contains(&mode_name.to_string()) {
            modes.push(mode_name.to_string());
        }
        Ok(())
    }

    /// Disable a minor mode in a specific buffer.
    pub fn disable_minor_mode(&mut self, buffer_id: u64, mode_name: &str) {
        if let Some(modes) = self.buffer_minor_modes.get_mut(&buffer_id) {
            modes.retain(|m| m != mode_name);
        }
    }

    /// Toggle a minor mode in a specific buffer. Returns `Ok(true)` if the
    /// mode is now active, `Ok(false)` if it was disabled.
    pub fn toggle_minor_mode(&mut self, buffer_id: u64, mode_name: &str) -> Result<bool, String> {
        if !self.minor_modes.contains_key(mode_name) {
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
        // Check buffer-local first.
        if let Some(modes) = self.buffer_minor_modes.get(&buffer_id) {
            if modes.iter().any(|m| m == mode_name) {
                return true;
            }
        }
        // Check global.
        self.global_minor_modes.iter().any(|m| m == mode_name)
    }

    /// Return all active minor modes for a buffer (buffer-local + global).
    pub fn active_minor_modes(&self, buffer_id: u64) -> Vec<&str> {
        let mut result: Vec<&str> = Vec::new();
        // Global minor modes first (like Emacs).
        for name in &self.global_minor_modes {
            result.push(name.as_str());
        }
        // Then buffer-local, avoiding duplicates.
        if let Some(modes) = self.buffer_minor_modes.get(&buffer_id) {
            for name in modes {
                if !result.contains(&name.as_str()) {
                    result.push(name.as_str());
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
        if !self.minor_modes.contains_key(mode_name) {
            return Err(format!("Unknown minor mode: {}", mode_name));
        }
        if !self.global_minor_modes.contains(&mode_name.to_string()) {
            self.global_minor_modes.push(mode_name.to_string());
        }
        Ok(())
    }

    /// Disable a globally-active minor mode.
    pub fn disable_global_minor_mode(&mut self, mode_name: &str) {
        self.global_minor_modes.retain(|m| m != mode_name);
    }

    // -------------------------------------------------------------------
    // Auto-mode
    // -------------------------------------------------------------------

    /// Add an entry to the auto-mode-alist (pattern -> mode name).
    /// Patterns are suffix-matched against filenames (similar to Emacs
    /// `auto-mode-alist` regex patterns like `"\\.rs\\'"` which match file
    /// endings).  Here we use simple suffix matching: if the filename ends
    /// with `pattern`, it matches.
    pub fn add_auto_mode(&mut self, pattern: String, mode: String) {
        self.auto_mode_alist.push((pattern, mode));
    }

    // -------------------------------------------------------------------
    // Custom variables / groups
    // -------------------------------------------------------------------

    /// Register a custom variable.
    pub fn register_custom_variable(&mut self, var: CustomVariable) {
        if let Some(ref group_name) = var.group {
            if let Some(group) = self.custom_groups.get_mut(group_name) {
                if !group.members.contains(&var.name) {
                    group.members.push(var.name.clone());
                }
            }
        }
        self.custom_variables.insert(var.name.clone(), var);
    }

    /// Register a custom group.
    pub fn register_custom_group(&mut self, group: CustomGroup) {
        self.custom_groups.insert(group.name.clone(), group);
    }

    /// Look up a custom variable by name.
    pub fn get_custom_variable(&self, name: &str) -> Option<&CustomVariable> {
        self.custom_variables.get(name)
    }

    /// Look up a custom group by name.
    pub fn get_custom_group(&self, name: &str) -> Option<&CustomGroup> {
        self.custom_groups.get(name)
    }

    // -------------------------------------------------------------------
    // Font-lock
    // -------------------------------------------------------------------

    /// Return the font-lock keywords for a mode (walking the parent chain).
    pub fn font_lock_keywords(&self, mode_name: &str) -> Option<&[FontLockKeyword]> {
        let mut current = Some(mode_name.to_string());
        while let Some(name) = current {
            if let Some(mode) = self.major_modes.get(&name) {
                if let Some(ref fl) = mode.font_lock {
                    return Some(&fl.keywords);
                }
                current = mode.parent.clone();
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
            .get(major)
            .map(|m| m.pretty_name.as_str())
            .unwrap_or(major);

        let mut parts = vec![pretty.to_string()];

        for minor_name in self.active_minor_modes(buffer_id) {
            if let Some(mode) = self.minor_modes.get(minor_name) {
                if let Some(ref lighter) = mode.lighter {
                    parts.push(lighter.clone());
                }
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
            name: "fundamental-mode".to_string(),
            pretty_name: "Fundamental".to_string(),
            parent: None,
            mode_hook: "fundamental-mode-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        };
        self.major_modes.insert(mode.name.clone(), mode);
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
fn filename_matches_pattern(filename: &str, pattern: &str) -> bool {
    filename.ends_with(pattern)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // ModeRegistry basics
    // -------------------------------------------------------------------

    #[test]
    fn new_registry_has_fundamental_mode() {
        let reg = ModeRegistry::new();
        assert!(reg.major_modes.contains_key("fundamental-mode"));
    }

    #[test]
    fn default_major_mode_is_fundamental() {
        let reg = ModeRegistry::new();
        assert_eq!(reg.get_major_mode(1), "fundamental-mode");
    }

    // -------------------------------------------------------------------
    // Major mode registration and switching
    // -------------------------------------------------------------------

    #[test]
    fn register_and_set_major_mode() {
        let mut reg = ModeRegistry::new();
        reg.register_major_mode(MajorMode {
            name: "rust-mode".to_string(),
            pretty_name: "Rust".to_string(),
            parent: Some("prog-mode".to_string()),
            mode_hook: "rust-mode-hook".to_string(),
            keymap_name: Some("rust-mode-map".to_string()),
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        });

        assert!(reg.set_major_mode(1, "rust-mode").is_ok());
        assert_eq!(reg.get_major_mode(1), "rust-mode");
    }

    #[test]
    fn set_unknown_major_mode_fails() {
        let mut reg = ModeRegistry::new();
        let result = reg.set_major_mode(1, "nonexistent-mode");
        assert!(result.is_err());
    }

    #[test]
    fn set_major_mode_replaces_previous() {
        let mut reg = ModeRegistry::new();
        reg.register_major_mode(MajorMode {
            name: "text-mode".to_string(),
            pretty_name: "Text".to_string(),
            parent: None,
            mode_hook: "text-mode-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        });
        reg.register_major_mode(MajorMode {
            name: "org-mode".to_string(),
            pretty_name: "Org".to_string(),
            parent: Some("text-mode".to_string()),
            mode_hook: "org-mode-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        });

        reg.set_major_mode(1, "text-mode").unwrap();
        assert_eq!(reg.get_major_mode(1), "text-mode");

        reg.set_major_mode(1, "org-mode").unwrap();
        assert_eq!(reg.get_major_mode(1), "org-mode");
    }

    // -------------------------------------------------------------------
    // Minor mode operations
    // -------------------------------------------------------------------

    #[test]
    fn register_and_enable_minor_mode() {
        let mut reg = ModeRegistry::new();
        reg.register_minor_mode(MinorMode {
            name: "auto-fill-mode".to_string(),
            lighter: Some(" Fill".to_string()),
            keymap_name: None,
            global: false,
            body: None,
        });

        assert!(reg.enable_minor_mode(1, "auto-fill-mode").is_ok());
        assert!(reg.is_minor_mode_active(1, "auto-fill-mode"));
    }

    #[test]
    fn enable_unknown_minor_mode_fails() {
        let mut reg = ModeRegistry::new();
        let result = reg.enable_minor_mode(1, "nonexistent-mode");
        assert!(result.is_err());
    }

    #[test]
    fn disable_minor_mode() {
        let mut reg = ModeRegistry::new();
        reg.register_minor_mode(MinorMode {
            name: "flycheck-mode".to_string(),
            lighter: Some(" FlyC".to_string()),
            keymap_name: None,
            global: false,
            body: None,
        });

        reg.enable_minor_mode(1, "flycheck-mode").unwrap();
        assert!(reg.is_minor_mode_active(1, "flycheck-mode"));

        reg.disable_minor_mode(1, "flycheck-mode");
        assert!(!reg.is_minor_mode_active(1, "flycheck-mode"));
    }

    #[test]
    fn toggle_minor_mode() {
        let mut reg = ModeRegistry::new();
        reg.register_minor_mode(MinorMode {
            name: "linum-mode".to_string(),
            lighter: Some(" Ln".to_string()),
            keymap_name: None,
            global: false,
            body: None,
        });

        // Toggle on.
        let active = reg.toggle_minor_mode(1, "linum-mode").unwrap();
        assert!(active);
        assert!(reg.is_minor_mode_active(1, "linum-mode"));

        // Toggle off.
        let active = reg.toggle_minor_mode(1, "linum-mode").unwrap();
        assert!(!active);
        assert!(!reg.is_minor_mode_active(1, "linum-mode"));
    }

    #[test]
    fn toggle_unknown_minor_mode_fails() {
        let mut reg = ModeRegistry::new();
        let result = reg.toggle_minor_mode(1, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn active_minor_modes_lists_all() {
        let mut reg = ModeRegistry::new();
        reg.register_minor_mode(MinorMode {
            name: "mode-a".to_string(),
            lighter: Some(" A".to_string()),
            keymap_name: None,
            global: false,
            body: None,
        });
        reg.register_minor_mode(MinorMode {
            name: "mode-b".to_string(),
            lighter: Some(" B".to_string()),
            keymap_name: None,
            global: false,
            body: None,
        });

        reg.enable_minor_mode(1, "mode-a").unwrap();
        reg.enable_minor_mode(1, "mode-b").unwrap();

        let active = reg.active_minor_modes(1);
        assert_eq!(active.len(), 2);
        assert!(active.contains(&"mode-a"));
        assert!(active.contains(&"mode-b"));
    }

    #[test]
    fn enable_minor_mode_idempotent() {
        let mut reg = ModeRegistry::new();
        reg.register_minor_mode(MinorMode {
            name: "hl-line-mode".to_string(),
            lighter: None,
            keymap_name: None,
            global: false,
            body: None,
        });

        reg.enable_minor_mode(1, "hl-line-mode").unwrap();
        reg.enable_minor_mode(1, "hl-line-mode").unwrap();

        let active = reg.active_minor_modes(1);
        assert_eq!(active.len(), 1);
    }

    // -------------------------------------------------------------------
    // Global minor modes
    // -------------------------------------------------------------------

    #[test]
    fn global_minor_mode_active_in_all_buffers() {
        let mut reg = ModeRegistry::new();
        reg.register_minor_mode(MinorMode {
            name: "global-hl-line-mode".to_string(),
            lighter: Some(" HL".to_string()),
            keymap_name: None,
            global: true,
            body: None,
        });

        reg.enable_global_minor_mode("global-hl-line-mode").unwrap();

        // Active in any buffer, even ones we never explicitly set.
        assert!(reg.is_minor_mode_active(1, "global-hl-line-mode"));
        assert!(reg.is_minor_mode_active(99, "global-hl-line-mode"));
    }

    #[test]
    fn disable_global_minor_mode() {
        let mut reg = ModeRegistry::new();
        reg.register_minor_mode(MinorMode {
            name: "global-mode".to_string(),
            lighter: None,
            keymap_name: None,
            global: true,
            body: None,
        });

        reg.enable_global_minor_mode("global-mode").unwrap();
        assert!(reg.is_minor_mode_active(1, "global-mode"));

        reg.disable_global_minor_mode("global-mode");
        assert!(!reg.is_minor_mode_active(1, "global-mode"));
    }

    #[test]
    fn global_and_buffer_local_no_duplicates() {
        let mut reg = ModeRegistry::new();
        reg.register_minor_mode(MinorMode {
            name: "shared-mode".to_string(),
            lighter: Some(" S".to_string()),
            keymap_name: None,
            global: false,
            body: None,
        });

        reg.enable_global_minor_mode("shared-mode").unwrap();
        reg.enable_minor_mode(1, "shared-mode").unwrap();

        // Should only appear once.
        let active = reg.active_minor_modes(1);
        assert_eq!(active.iter().filter(|&&m| m == "shared-mode").count(), 1);
    }

    // -------------------------------------------------------------------
    // Auto-mode-alist
    // -------------------------------------------------------------------

    #[test]
    fn auto_mode_alist_suffix_match() {
        let mut reg = ModeRegistry::new();
        reg.register_major_mode(MajorMode {
            name: "rust-mode".to_string(),
            pretty_name: "Rust".to_string(),
            parent: None,
            mode_hook: "rust-mode-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        });
        reg.add_auto_mode(".rs".to_string(), "rust-mode".to_string());

        assert_eq!(reg.mode_for_file("main.rs"), Some("rust-mode"));
        assert_eq!(reg.mode_for_file("lib.rs"), Some("rust-mode"));
        assert_eq!(reg.mode_for_file("main.py"), None);
    }

    #[test]
    fn auto_mode_alist_first_match_wins() {
        let mut reg = ModeRegistry::new();
        reg.register_major_mode(MajorMode {
            name: "mode-a".to_string(),
            pretty_name: "A".to_string(),
            parent: None,
            mode_hook: "mode-a-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        });
        reg.register_major_mode(MajorMode {
            name: "mode-b".to_string(),
            pretty_name: "B".to_string(),
            parent: None,
            mode_hook: "mode-b-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        });
        reg.add_auto_mode(".txt".to_string(), "mode-a".to_string());
        reg.add_auto_mode(".txt".to_string(), "mode-b".to_string());

        assert_eq!(reg.mode_for_file("file.txt"), Some("mode-a"));
    }

    // -------------------------------------------------------------------
    // Mode-line rendering
    // -------------------------------------------------------------------

    #[test]
    fn mode_line_string_fundamental() {
        let reg = ModeRegistry::new();
        let s = reg.mode_line_string(1);
        assert_eq!(s, "(Fundamental)");
    }

    #[test]
    fn mode_line_string_with_minor_modes() {
        let mut reg = ModeRegistry::new();
        reg.register_minor_mode(MinorMode {
            name: "auto-fill-mode".to_string(),
            lighter: Some(" Fill".to_string()),
            keymap_name: None,
            global: false,
            body: None,
        });
        reg.enable_minor_mode(1, "auto-fill-mode").unwrap();

        let s = reg.mode_line_string(1);
        assert_eq!(s, "(Fundamental Fill)");
    }

    #[test]
    fn mode_line_format_render() {
        let reg = ModeRegistry::new();
        let fmt = ModeLineFormat::default_format();
        let rendered = fmt.render(1, &reg, "*scratch*", false, false, 1, 0, 0);
        assert!(rendered.contains("*scratch*"));
        assert!(rendered.contains("Fundamental"));
        assert!(rendered.contains("Top"));
        assert!(rendered.contains("--"));
    }

    #[test]
    fn mode_line_format_modified_and_readonly() {
        let reg = ModeRegistry::new();
        let fmt = ModeLineFormat::default_format();

        let rendered_mod = fmt.render(1, &reg, "buf", true, false, 10, 5, 50);
        assert!(rendered_mod.contains("**"));
        assert!(rendered_mod.contains("50%"));
        assert!(rendered_mod.contains("10:5"));

        let rendered_ro = fmt.render(1, &reg, "buf", false, true, 1, 0, 100);
        assert!(rendered_ro.contains("%%"));
        assert!(rendered_ro.contains("Bot"));
    }

    // -------------------------------------------------------------------
    // Font-lock keywords
    // -------------------------------------------------------------------

    #[test]
    fn font_lock_keywords_basic() {
        let mut reg = ModeRegistry::new();
        reg.register_major_mode(MajorMode {
            name: "lisp-mode".to_string(),
            pretty_name: "Lisp".to_string(),
            parent: None,
            mode_hook: "lisp-mode-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: Some(FontLockDefaults {
                keywords: vec![FontLockKeyword {
                    pattern: r"\b(defun|defvar)\b".to_string(),
                    face: "font-lock-keyword-face".to_string(),
                    group: 1,
                    override_: false,
                    laxmatch: false,
                }],
                case_fold: false,
                syntax_table: None,
            }),
            body: None,
        });

        let kws = reg.font_lock_keywords("lisp-mode").unwrap();
        assert_eq!(kws.len(), 1);
        assert_eq!(kws[0].face, "font-lock-keyword-face");
    }

    #[test]
    fn font_lock_keywords_inherit_from_parent() {
        let mut reg = ModeRegistry::new();

        // Parent with font-lock.
        reg.register_major_mode(MajorMode {
            name: "prog-mode".to_string(),
            pretty_name: "Prog".to_string(),
            parent: None,
            mode_hook: "prog-mode-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: Some(FontLockDefaults {
                keywords: vec![FontLockKeyword {
                    pattern: r"TODO".to_string(),
                    face: "font-lock-warning-face".to_string(),
                    group: 0,
                    override_: true,
                    laxmatch: false,
                }],
                case_fold: false,
                syntax_table: None,
            }),
            body: None,
        });

        // Child without font-lock — should inherit.
        reg.register_major_mode(MajorMode {
            name: "rust-mode".to_string(),
            pretty_name: "Rust".to_string(),
            parent: Some("prog-mode".to_string()),
            mode_hook: "rust-mode-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        });

        let kws = reg.font_lock_keywords("rust-mode").unwrap();
        assert_eq!(kws.len(), 1);
        assert_eq!(kws[0].pattern, "TODO");
    }

    #[test]
    fn font_lock_keywords_none() {
        let reg = ModeRegistry::new();
        assert!(reg.font_lock_keywords("fundamental-mode").is_none());
    }

    // -------------------------------------------------------------------
    // Custom variables and groups
    // -------------------------------------------------------------------

    #[test]
    fn register_custom_variable() {
        let mut reg = ModeRegistry::new();
        reg.register_custom_variable(CustomVariable {
            name: "indent-tabs-mode".to_string(),
            default_value: Value::True,
            doc: Some("Use tabs for indentation.".to_string()),
            type_: CustomType::Boolean,
            group: None,
            set_function: None,
            get_function: None,
            tag: None,
        });

        let var = reg.get_custom_variable("indent-tabs-mode").unwrap();
        assert_eq!(var.name, "indent-tabs-mode");
        assert!(var.default_value.is_truthy());
    }

    #[test]
    fn custom_variable_in_group() {
        let mut reg = ModeRegistry::new();
        reg.register_custom_group(CustomGroup {
            name: "editing".to_string(),
            doc: Some("Editing options.".to_string()),
            parent: None,
            members: vec![],
        });

        reg.register_custom_variable(CustomVariable {
            name: "fill-column".to_string(),
            default_value: Value::Int(70),
            doc: None,
            type_: CustomType::Integer,
            group: Some("editing".to_string()),
            set_function: None,
            get_function: None,
            tag: None,
        });

        let group = reg.get_custom_group("editing").unwrap();
        assert!(group.members.contains(&"fill-column".to_string()));
    }

    // -------------------------------------------------------------------
    // Mode inheritance (derived-mode-p)
    // -------------------------------------------------------------------

    #[test]
    fn derived_mode_p_self() {
        let mut reg = ModeRegistry::new();
        reg.register_major_mode(MajorMode {
            name: "text-mode".to_string(),
            pretty_name: "Text".to_string(),
            parent: None,
            mode_hook: "text-mode-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        });

        assert!(reg.derived_mode_p("text-mode", "text-mode"));
    }

    #[test]
    fn derived_mode_p_parent_chain() {
        let mut reg = ModeRegistry::new();
        reg.register_major_mode(MajorMode {
            name: "text-mode".to_string(),
            pretty_name: "Text".to_string(),
            parent: None,
            mode_hook: "text-mode-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        });
        reg.register_major_mode(MajorMode {
            name: "org-mode".to_string(),
            pretty_name: "Org".to_string(),
            parent: Some("text-mode".to_string()),
            mode_hook: "org-mode-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        });
        reg.register_major_mode(MajorMode {
            name: "org-journal-mode".to_string(),
            pretty_name: "Org-Journal".to_string(),
            parent: Some("org-mode".to_string()),
            mode_hook: "org-journal-mode-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        });

        assert!(reg.derived_mode_p("org-journal-mode", "text-mode"));
        assert!(reg.derived_mode_p("org-journal-mode", "org-mode"));
        assert!(reg.derived_mode_p("org-mode", "text-mode"));
        assert!(!reg.derived_mode_p("text-mode", "org-mode"));
    }

    #[test]
    fn derived_mode_p_unrelated() {
        let mut reg = ModeRegistry::new();
        reg.register_major_mode(MajorMode {
            name: "text-mode".to_string(),
            pretty_name: "Text".to_string(),
            parent: None,
            mode_hook: "text-mode-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        });
        reg.register_major_mode(MajorMode {
            name: "prog-mode".to_string(),
            pretty_name: "Prog".to_string(),
            parent: None,
            mode_hook: "prog-mode-hook".to_string(),
            keymap_name: None,
            syntax_table_name: None,
            abbrev_table_name: None,
            font_lock: None,
            body: None,
        });

        assert!(!reg.derived_mode_p("text-mode", "prog-mode"));
        assert!(!reg.derived_mode_p("prog-mode", "text-mode"));
    }

    // -------------------------------------------------------------------
    // Buffer removal
    // -------------------------------------------------------------------

    #[test]
    fn remove_buffer_cleans_up() {
        let mut reg = ModeRegistry::new();
        reg.register_minor_mode(MinorMode {
            name: "test-mode".to_string(),
            lighter: None,
            keymap_name: None,
            global: false,
            body: None,
        });

        reg.set_major_mode(1, "fundamental-mode").unwrap();
        reg.enable_minor_mode(1, "test-mode").unwrap();

        reg.remove_buffer(1);

        // Falls back to fundamental-mode (no entry).
        assert_eq!(reg.get_major_mode(1), "fundamental-mode");
        assert!(
            reg.active_minor_modes(1).is_empty()
                || reg
                    .active_minor_modes(1)
                    .iter()
                    .all(|m| { reg.global_minor_modes.contains(&m.to_string()) })
        );
    }

    // -------------------------------------------------------------------
    // FontLockLevel default
    // -------------------------------------------------------------------

    #[test]
    fn font_lock_level_default_is_level3() {
        let level = FontLockLevel::default();
        assert_eq!(level, FontLockLevel::Level3);
    }

    // -------------------------------------------------------------------
    // ModeLineFormat default
    // -------------------------------------------------------------------

    #[test]
    fn mode_line_format_default_has_elements() {
        let fmt = ModeLineFormat::default_format();
        assert!(!fmt.elements.is_empty());
    }

    // -------------------------------------------------------------------
    // Custom types
    // -------------------------------------------------------------------

    #[test]
    fn custom_type_choice() {
        let mut reg = ModeRegistry::new();
        reg.register_custom_variable(CustomVariable {
            name: "my-choice".to_string(),
            default_value: Value::symbol("fast"),
            doc: None,
            type_: CustomType::Choice(vec![
                ("fast".to_string(), Value::symbol("fast")),
                ("slow".to_string(), Value::symbol("slow")),
            ]),
            group: None,
            set_function: None,
            get_function: None,
            tag: None,
        });

        let var = reg.get_custom_variable("my-choice").unwrap();
        assert!(matches!(var.type_, CustomType::Choice(_)));
    }

    #[test]
    fn custom_type_nested_list() {
        let mut reg = ModeRegistry::new();
        reg.register_custom_variable(CustomVariable {
            name: "my-list".to_string(),
            default_value: Value::Nil,
            doc: None,
            type_: CustomType::List(Box::new(CustomType::String)),
            group: None,
            set_function: None,
            get_function: None,
            tag: None,
        });

        let var = reg.get_custom_variable("my-list").unwrap();
        assert!(matches!(var.type_, CustomType::List(_)));
    }
}
