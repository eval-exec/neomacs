//! Emacs coding system support.
//!
//! Since Rust natively uses UTF-8, this module is primarily for API
//! compatibility. The coding system infrastructure tracks registered
//! systems and their aliases but all actual encoding/decoding passes
//! through as UTF-8 identity operations.
//!
//! Contains:
//! - CodingSystemManager: registry of coding systems, aliases, priority list
//! - CodingSystemInfo: per-system metadata (name, type, mnemonic, EOL)
//! - Pure builtins: coding-system-list, coding-system-aliases, coding-system-get,
//!   coding-system-put, coding-system-base, coding-system-eol-type,
//!   coding-system-type, coding-system-change-eol-conversion,
//!   coding-system-change-text-conversion,
//!   detect-coding-string, detect-coding-region, keyboard-coding-system,
//!   terminal-coding-system, set-keyboard-coding-system,
//!   set-terminal-coding-system, coding-system-priority-list

use super::error::{signal, EvalResult, Flow};
use super::value::*;
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Argument helpers (local to this module)
// ---------------------------------------------------------------------------

fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_min_args(name: &str, args: &[Value], min: usize) -> Result<(), Flow> {
    if args.len() < min {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

/// Extract a coding system name from a symbol or string argument.
fn coding_system_name(val: &Value) -> Result<String, Flow> {
    match val {
        Value::Symbol(s) => Ok(s.clone()),
        Value::Str(s) => Ok((**s).clone()),
        Value::Nil => Ok("nil".to_string()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("symbolp"), other.clone()],
        )),
    }
}

// ---------------------------------------------------------------------------
// EOL types
// ---------------------------------------------------------------------------

/// End-of-line conversion types matching Emacs conventions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EolType {
    /// LF (Unix) -- value 0
    Unix,
    /// CRLF (DOS/Windows) -- value 1
    Dos,
    /// CR (Classic Mac) -- value 2
    Mac,
    /// Undecided / detect automatically
    Undecided,
}

impl EolType {
    pub fn to_int(&self) -> i64 {
        match self {
            EolType::Unix => 0,
            EolType::Dos => 1,
            EolType::Mac => 2,
            EolType::Undecided => 0,
        }
    }

    pub fn to_symbol(&self) -> Value {
        match self {
            EolType::Unix => Value::symbol("unix"),
            EolType::Dos => Value::symbol("dos"),
            EolType::Mac => Value::symbol("mac"),
            EolType::Undecided => Value::symbol("undecided"),
        }
    }

    pub fn suffix(&self) -> &'static str {
        match self {
            EolType::Unix => "-unix",
            EolType::Dos => "-dos",
            EolType::Mac => "-mac",
            EolType::Undecided => "",
        }
    }

    pub fn from_suffix(name: &str) -> Option<EolType> {
        if name.ends_with("-unix") {
            Some(EolType::Unix)
        } else if name.ends_with("-dos") {
            Some(EolType::Dos)
        } else if name.ends_with("-mac") {
            Some(EolType::Mac)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// CodingSystemInfo
// ---------------------------------------------------------------------------

/// Information about a single coding system.
#[derive(Clone, Debug)]
pub struct CodingSystemInfo {
    /// Canonical name of the coding system (e.g. "utf-8").
    pub name: String,
    /// Type category (e.g. "utf-8", "charset", "raw-text", "undecided").
    pub coding_type: String,
    /// Mnemonic character shown in the mode line.
    pub mnemonic: char,
    /// End-of-line conversion type.
    pub eol_type: EolType,
    /// Arbitrary property list for coding-system-get / coding-system-put.
    pub properties: HashMap<String, Value>,
}

impl CodingSystemInfo {
    fn new(name: &str, coding_type: &str, mnemonic: char, eol_type: EolType) -> Self {
        Self {
            name: name.to_string(),
            coding_type: coding_type.to_string(),
            mnemonic,
            eol_type,
            properties: HashMap::new(),
        }
    }

    /// Return the base name (strip -unix/-dos/-mac suffix).
    fn base_name(&self) -> &str {
        for suffix in &["-unix", "-dos", "-mac"] {
            if self.name.ends_with(suffix) {
                return &self.name[..self.name.len() - suffix.len()];
            }
        }
        &self.name
    }
}

// ---------------------------------------------------------------------------
// CodingSystemManager
// ---------------------------------------------------------------------------

/// Central registry for all coding systems and their aliases.
pub struct CodingSystemManager {
    /// Registered coding systems, keyed by canonical name.
    pub systems: HashMap<String, CodingSystemInfo>,
    /// Alias -> canonical name mapping.
    pub aliases: HashMap<String, String>,
    /// Detection priority list (ordered list of system names).
    pub priority: Vec<String>,
    /// Current keyboard coding system.
    keyboard_coding: String,
    /// Current terminal coding system.
    terminal_coding: String,
}

impl CodingSystemManager {
    /// Create a new manager pre-populated with the standard coding systems.
    pub fn new() -> Self {
        let mut mgr = Self {
            systems: HashMap::new(),
            aliases: HashMap::new(),
            priority: Vec::new(),
            keyboard_coding: "utf-8".to_string(),
            terminal_coding: "utf-8".to_string(),
        };

        // Register standard coding systems
        mgr.register(CodingSystemInfo::new(
            "utf-8",
            "utf-8",
            'U',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "utf-8-unix",
            "utf-8",
            'U',
            EolType::Unix,
        ));
        mgr.register(CodingSystemInfo::new(
            "utf-8-dos",
            "utf-8",
            'U',
            EolType::Dos,
        ));
        mgr.register(CodingSystemInfo::new(
            "utf-8-mac",
            "utf-8",
            'U',
            EolType::Mac,
        ));
        mgr.register(CodingSystemInfo::new(
            "latin-1",
            "charset",
            'l',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "ascii",
            "charset",
            'A',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "binary",
            "raw-text",
            '=',
            EolType::Unix,
        ));
        mgr.register(CodingSystemInfo::new(
            "raw-text",
            "raw-text",
            '=',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "undecided",
            "undecided",
            '-',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "emacs-internal",
            "utf-8",
            'U',
            EolType::Unix,
        ));
        mgr.register(CodingSystemInfo::new(
            "no-conversion",
            "raw-text",
            '=',
            EolType::Unix,
        ));

        // Common aliases
        mgr.aliases
            .insert("mule-utf-8".to_string(), "utf-8".to_string());
        mgr.aliases
            .insert("utf-8-emacs".to_string(), "utf-8".to_string());
        mgr.aliases
            .insert("iso-8859-1".to_string(), "latin-1".to_string());
        mgr.aliases
            .insert("us-ascii".to_string(), "ascii".to_string());
        mgr.aliases
            .insert("utf-8-auto".to_string(), "utf-8".to_string());
        mgr.aliases
            .insert("prefer-utf-8".to_string(), "utf-8".to_string());

        // Default priority list
        mgr.priority = vec![
            "utf-8".to_string(),
            "utf-8-unix".to_string(),
            "undecided".to_string(),
            "latin-1".to_string(),
            "ascii".to_string(),
            "raw-text".to_string(),
            "binary".to_string(),
            "no-conversion".to_string(),
        ];

        mgr
    }

    /// Register a coding system.
    fn register(&mut self, info: CodingSystemInfo) {
        self.systems.insert(info.name.clone(), info);
    }

    /// Resolve a name through the alias table to a canonical name.
    /// Returns either the input name (if it's a direct system) or the
    /// canonical name from the alias table.
    pub fn resolve<'a>(&'a self, name: &'a str) -> Option<&'a str> {
        if self.systems.contains_key(name) {
            Some(name)
        } else if let Some(canonical) = self.aliases.get(name) {
            if self.systems.contains_key(canonical.as_str()) {
                Some(canonical.as_str())
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Look up a coding system by name (resolving aliases).
    pub fn get(&self, name: &str) -> Option<&CodingSystemInfo> {
        let canonical = self.resolve(name)?;
        self.systems.get(canonical)
    }

    /// Look up a coding system mutably by name (resolving aliases).
    pub fn get_mut(&mut self, name: &str) -> Option<&mut CodingSystemInfo> {
        // Need to resolve first, then borrow mutably.
        let canonical = if self.systems.contains_key(name) {
            name.to_string()
        } else if let Some(c) = self.aliases.get(name) {
            c.clone()
        } else {
            return None;
        };
        self.systems.get_mut(&canonical)
    }

    /// Check if a name is a known coding system (or alias).
    pub fn is_known(&self, name: &str) -> bool {
        self.resolve(name).is_some()
    }

    /// Add an alias mapping.
    pub fn add_alias(&mut self, alias: &str, target: &str) {
        self.aliases.insert(alias.to_string(), target.to_string());
    }

    /// Get all aliases that point to a given canonical name.
    pub fn aliases_for(&self, canonical: &str) -> Vec<String> {
        // Resolve in case the caller passed an alias
        let target = if self.systems.contains_key(canonical) {
            canonical
        } else if let Some(c) = self.aliases.get(canonical) {
            c.as_str()
        } else {
            return Vec::new();
        };

        self.aliases
            .iter()
            .filter(|(_, v)| v.as_str() == target)
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// List all registered coding system names (canonical only).
    pub fn list_all(&self) -> Vec<String> {
        let mut names: Vec<String> = self.systems.keys().cloned().collect();
        names.sort();
        names
    }
}

impl Default for CodingSystemManager {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Pure builtins
// ===========================================================================

/// `(coding-system-list &optional BASE-ONLY)` -- return a list of all coding systems.
/// If BASE-ONLY is non-nil, only return base systems (no -unix/-dos/-mac variants).
pub(crate) fn builtin_coding_system_list(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    let base_only = args.first().is_some_and(|v| v.is_truthy());
    let names = mgr.list_all();
    let filtered: Vec<Value> = names
        .into_iter()
        .filter(|n| {
            if base_only {
                !n.ends_with("-unix") && !n.ends_with("-dos") && !n.ends_with("-mac")
            } else {
                true
            }
        })
        .map(|n| Value::symbol(n))
        .collect();
    Ok(Value::list(filtered))
}

/// `(coding-system-aliases CODING-SYSTEM)` -- return a list of aliases for a
/// coding system (including the name itself as the first element).
pub(crate) fn builtin_coding_system_aliases(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-aliases", &args, 1)?;
    let name = coding_system_name(&args[0])?;

    if !mgr.is_known(&name) {
        return Ok(Value::Nil);
    }

    let canonical = mgr.resolve(&name).unwrap_or(&name).to_string();
    let mut result = vec![Value::symbol(&canonical)];
    for alias in mgr.aliases_for(&canonical) {
        result.push(Value::symbol(alias));
    }
    Ok(Value::list(result))
}

/// `(coding-system-get CODING-SYSTEM PROP)` -- get a property of a coding system.
/// Recognized built-in properties: :name, :type, :mnemonic, :eol-type.
/// Other properties are looked up from the per-system property list.
pub(crate) fn builtin_coding_system_get(mgr: &CodingSystemManager, args: Vec<Value>) -> EvalResult {
    expect_args("coding-system-get", &args, 2)?;
    let name = coding_system_name(&args[0])?;
    let prop = coding_system_name(&args[1])?;

    let info = match mgr.get(&name) {
        Some(i) => i,
        None => return Ok(Value::Nil),
    };

    match prop.as_str() {
        ":name" | "name" => Ok(Value::symbol(&info.name)),
        ":type" | "type" | ":coding-type" | "coding-type" => Ok(Value::symbol(&info.coding_type)),
        ":mnemonic" | "mnemonic" => Ok(Value::Char(info.mnemonic)),
        ":eol-type" | "eol-type" => Ok(Value::Int(info.eol_type.to_int())),
        _ => Ok(info.properties.get(&prop).cloned().unwrap_or(Value::Nil)),
    }
}

/// `(coding-system-put CODING-SYSTEM PROP VAL)` -- set a property of a coding system.
pub(crate) fn builtin_coding_system_put(
    mgr: &mut CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-put", &args, 3)?;
    let name = coding_system_name(&args[0])?;
    let prop = coding_system_name(&args[1])?;
    let val = args[2].clone();

    let info = match mgr.get_mut(&name) {
        Some(i) => i,
        None => {
            return Err(signal(
                "coding-system-error",
                vec![Value::string(format!("Unknown coding system: {}", name))],
            ));
        }
    };

    // Built-in properties are stored in dedicated fields
    match prop.as_str() {
        ":mnemonic" | "mnemonic" => {
            if let Value::Char(c) = val {
                info.mnemonic = c;
            } else if let Value::Int(n) = val {
                if let Some(c) = char::from_u32(n as u32) {
                    info.mnemonic = c;
                }
            }
        }
        _ => {
            info.properties.insert(prop, val);
        }
    }

    Ok(Value::Nil)
}

/// `(coding-system-base CODING-SYSTEM)` -- return the base coding system
/// (stripping -unix, -dos, -mac suffixes).
pub(crate) fn builtin_coding_system_base(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-base", &args, 1)?;
    let name = coding_system_name(&args[0])?;

    if let Some(info) = mgr.get(&name) {
        Ok(Value::symbol(info.base_name()))
    } else {
        // Even for unknown systems, strip the suffix
        let base = strip_eol_suffix(&name);
        Ok(Value::symbol(base))
    }
}

/// `(coding-system-eol-type CODING-SYSTEM)` -- return the EOL type.
/// Returns 0 (unix), 1 (dos), 2 (mac), or a vector of three sub-coding-systems
/// if the EOL type is undecided.
pub(crate) fn builtin_coding_system_eol_type(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-eol-type", &args, 1)?;
    let name = coding_system_name(&args[0])?;

    if let Some(info) = mgr.get(&name) {
        match info.eol_type {
            EolType::Unix => Ok(Value::Int(0)),
            EolType::Dos => Ok(Value::Int(1)),
            EolType::Mac => Ok(Value::Int(2)),
            EolType::Undecided => {
                // Return a vector of [base-unix base-dos base-mac]
                let base = info.base_name().to_string();
                let vec = vec![
                    Value::symbol(format!("{}-unix", base)),
                    Value::symbol(format!("{}-dos", base)),
                    Value::symbol(format!("{}-mac", base)),
                ];
                Ok(Value::Vector(std::sync::Arc::new(std::sync::Mutex::new(
                    vec,
                ))))
            }
        }
    } else {
        Ok(Value::Nil)
    }
}

/// `(coding-system-type CODING-SYSTEM)` -- return the type symbol of the
/// coding system (e.g. utf-8, charset, raw-text, undecided).
pub(crate) fn builtin_coding_system_type(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-type", &args, 1)?;
    let name = coding_system_name(&args[0])?;

    if let Some(info) = mgr.get(&name) {
        Ok(Value::symbol(&info.coding_type))
    } else {
        Ok(Value::Nil)
    }
}

/// `(coding-system-change-eol-conversion CODING-SYSTEM EOL-TYPE)` -- return
/// a coding system derived from CODING-SYSTEM but with a different EOL type.
/// EOL-TYPE is 0 (unix), 1 (dos), or 2 (mac), or a symbol.
pub(crate) fn builtin_coding_system_change_eol_conversion(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-change-eol-conversion", &args, 2)?;
    let name = coding_system_name(&args[0])?;
    let eol = &args[1];

    let suffix = match eol {
        Value::Int(0) => "-unix",
        Value::Int(1) => "-dos",
        Value::Int(2) => "-mac",
        Value::Symbol(s) if s == "unix" => "-unix",
        Value::Symbol(s) if s == "dos" => "-dos",
        Value::Symbol(s) if s == "mac" => "-mac",
        _ => return Ok(args[0].clone()),
    };

    let base = if let Some(info) = mgr.get(&name) {
        info.base_name().to_string()
    } else {
        strip_eol_suffix(&name).to_string()
    };

    let derived = format!("{}{}", base, suffix);
    Ok(Value::symbol(derived))
}

/// `(coding-system-change-text-conversion CODING-SYSTEM TEXT-CODING)` -- return
/// a coding system derived from TEXT-CODING but preserving the EOL type of
/// CODING-SYSTEM.
pub(crate) fn builtin_coding_system_change_text_conversion(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-change-text-conversion", &args, 2)?;
    let name = coding_system_name(&args[0])?;
    let text_coding_name = coding_system_name(&args[1])?;

    // Determine EOL type from the original system
    let eol_suffix = if let Some(info) = mgr.get(&name) {
        match info.eol_type {
            EolType::Unix => "-unix",
            EolType::Dos => "-dos",
            EolType::Mac => "-mac",
            EolType::Undecided => "",
        }
    } else if let Some(eol) = EolType::from_suffix(&name) {
        eol.suffix()
    } else {
        ""
    };

    // Get the base of the text coding
    let text_base = if let Some(info) = mgr.get(&text_coding_name) {
        info.base_name().to_string()
    } else {
        strip_eol_suffix(&text_coding_name).to_string()
    };

    if eol_suffix.is_empty() {
        Ok(Value::symbol(text_base))
    } else {
        Ok(Value::symbol(format!("{}{}", text_base, eol_suffix)))
    }
}

/// `(coding-system-p OBJECT)` -- return t when OBJECT names a known coding
/// system or alias, nil otherwise.
pub(crate) fn builtin_coding_system_p(mgr: &CodingSystemManager, args: Vec<Value>) -> EvalResult {
    expect_args("coding-system-p", &args, 1)?;
    let known = match &args[0] {
        Value::Symbol(name) => mgr.is_known(name),
        Value::Nil => mgr.is_known("nil"),
        _ => false,
    };
    Ok(Value::bool(known))
}

/// `(check-coding-system CODING-SYSTEM)` -- validate CODING-SYSTEM.
/// Returns CODING-SYSTEM when valid, nil for nil, and signals on invalid input.
pub(crate) fn builtin_check_coding_system(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("check-coding-system", &args, 1)?;
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Symbol(name) => {
            if mgr.is_known(name) {
                Ok(args[0].clone())
            } else {
                Err(signal("coding-system-error", vec![args[0].clone()]))
            }
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("symbolp"), other.clone()],
        )),
    }
}

/// `(define-coding-system-alias ALIAS CODING-SYSTEM)` -- register ALIAS for
/// CODING-SYSTEM and return nil.
pub(crate) fn builtin_define_coding_system_alias(
    mgr: &mut CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("define-coding-system-alias", &args, 2)?;

    let alias = match &args[0] {
        Value::Symbol(name) => name.clone(),
        Value::Nil => "nil".to_string(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), other.clone()],
            ));
        }
    };

    let target = match &args[1] {
        Value::Symbol(name) => name.clone(),
        Value::Nil => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("coding-system-p"), Value::Nil],
            ));
        }
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), other.clone()],
            ));
        }
    };

    let canonical = mgr
        .resolve(&target)
        .ok_or_else(|| signal("coding-system-error", vec![Value::symbol(target.clone())]))?
        .to_string();
    mgr.add_alias(&alias, &canonical);
    Ok(Value::Nil)
}

/// `(set-coding-system-priority &rest CODING-SYSTEMS)` -- move CODING-SYSTEMS
/// to the front of the detection priority list in order, keeping relative order
/// of the remaining systems.
pub(crate) fn builtin_set_coding_system_priority(
    mgr: &mut CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::Nil);
    }

    let mut requested: Vec<(String, String)> = Vec::with_capacity(args.len());
    for arg in &args {
        match arg {
            Value::Nil => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("coding-system-p"), Value::Nil],
                ));
            }
            Value::Symbol(name) => {
                let canonical = mgr
                    .resolve(name)
                    .ok_or_else(|| signal("coding-system-error", vec![arg.clone()]))?;
                requested.push((name.clone(), canonical.to_string()));
            }
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("symbolp"), other.clone()],
                ));
            }
        }
    }

    let mut seen_canonicals: HashSet<String> =
        HashSet::with_capacity(mgr.priority.len() + requested.len());
    let mut reordered: Vec<String> = Vec::with_capacity(mgr.priority.len() + requested.len());

    for (display, canonical) in requested {
        if seen_canonicals.insert(canonical) {
            reordered.push(display);
        }
    }

    for name in &mgr.priority {
        let canonical = mgr.resolve(name).unwrap_or(name.as_str()).to_string();
        if seen_canonicals.insert(canonical) {
            reordered.push(name.clone());
        }
    }

    mgr.priority = reordered;
    Ok(Value::Nil)
}

/// `(detect-coding-string STRING &optional HIGHEST)` -- detect the encoding of
/// a string. Since all strings in this runtime are UTF-8, always returns utf-8.
/// If HIGHEST is non-nil, return a single coding system; otherwise return a list.
pub(crate) fn builtin_detect_coding_string(
    _mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("detect-coding-string", &args, 1)?;
    match &args[0] {
        Value::Str(_) => {}
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), other.clone()],
            ));
        }
    }
    let highest = args.get(1).is_some_and(|v| v.is_truthy());
    if highest {
        Ok(Value::symbol("utf-8"))
    } else {
        Ok(Value::list(vec![Value::symbol("utf-8")]))
    }
}

/// `(detect-coding-region START END &optional HIGHEST)` -- detect the encoding
/// of a buffer region. Stub: always returns utf-8.
pub(crate) fn builtin_detect_coding_region(
    _mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("detect-coding-region", &args, 2)?;
    let highest = args.get(2).is_some_and(|v| v.is_truthy());
    if highest {
        Ok(Value::symbol("utf-8"))
    } else {
        Ok(Value::list(vec![Value::symbol("utf-8")]))
    }
}

/// `(keyboard-coding-system &optional TERMINAL)` -- return the current
/// keyboard coding system. The TERMINAL argument is ignored.
pub(crate) fn builtin_keyboard_coding_system(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    Ok(Value::symbol(&mgr.keyboard_coding))
}

/// `(terminal-coding-system &optional TERMINAL)` -- return the current
/// terminal coding system. The TERMINAL argument is ignored.
pub(crate) fn builtin_terminal_coding_system(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    Ok(Value::symbol(&mgr.terminal_coding))
}

/// `(set-keyboard-coding-system CODING-SYSTEM &optional TERMINAL)` -- set the
/// keyboard coding system. Stub: records the value but has no functional effect.
pub(crate) fn builtin_set_keyboard_coding_system(
    mgr: &mut CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-keyboard-coding-system", &args, 1)?;
    let name = coding_system_name(&args[0])?;
    if name == "nil" || args[0].is_nil() {
        mgr.keyboard_coding = "utf-8".to_string();
    } else {
        mgr.keyboard_coding = name;
    }
    Ok(Value::Nil)
}

/// `(set-terminal-coding-system CODING-SYSTEM &optional TERMINAL)` -- set the
/// terminal coding system. Stub: records the value but has no functional effect.
pub(crate) fn builtin_set_terminal_coding_system(
    mgr: &mut CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-terminal-coding-system", &args, 1)?;
    let name = coding_system_name(&args[0])?;
    if name == "nil" || args[0].is_nil() {
        mgr.terminal_coding = "utf-8".to_string();
    } else {
        mgr.terminal_coding = name;
    }
    Ok(Value::Nil)
}

/// `(coding-system-priority-list &optional HIGHESTP)` -- return the current
/// priority list of coding systems for detection. If HIGHESTP is non-nil,
/// return only the highest-priority system.
pub(crate) fn builtin_coding_system_priority_list(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    let highest_only = args.first().is_some_and(|v| v.is_truthy());
    if highest_only {
        if let Some(first) = mgr.priority.first() {
            Ok(Value::list(vec![Value::symbol(first)]))
        } else {
            Ok(Value::Nil)
        }
    } else {
        let items: Vec<Value> = mgr.priority.iter().map(|n| Value::symbol(n)).collect();
        Ok(Value::list(items))
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Strip -unix, -dos, or -mac suffix from a coding system name.
fn strip_eol_suffix(name: &str) -> &str {
    for suffix in &["-unix", "-dos", "-mac"] {
        if name.ends_with(suffix) {
            return &name[..name.len() - suffix.len()];
        }
    }
    name
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn mgr() -> CodingSystemManager {
        CodingSystemManager::new()
    }

    // ----- CodingSystemManager construction -----

    #[test]
    fn new_manager_has_standard_systems() {
        let m = mgr();
        assert!(m.is_known("utf-8"));
        assert!(m.is_known("utf-8-unix"));
        assert!(m.is_known("utf-8-dos"));
        assert!(m.is_known("utf-8-mac"));
        assert!(m.is_known("latin-1"));
        assert!(m.is_known("ascii"));
        assert!(m.is_known("binary"));
        assert!(m.is_known("raw-text"));
        assert!(m.is_known("undecided"));
        assert!(m.is_known("emacs-internal"));
        assert!(m.is_known("no-conversion"));
    }

    #[test]
    fn aliases_resolve() {
        let m = mgr();
        assert!(m.is_known("iso-8859-1")); // alias for latin-1
        assert!(m.is_known("us-ascii")); // alias for ascii
        assert!(m.is_known("mule-utf-8")); // alias for utf-8
        assert_eq!(m.resolve("iso-8859-1"), Some("latin-1"));
        assert_eq!(m.resolve("us-ascii"), Some("ascii"));
    }

    #[test]
    fn unknown_system_not_known() {
        let m = mgr();
        assert!(!m.is_known("martian-encoding"));
        assert_eq!(m.resolve("martian-encoding"), None);
    }

    #[test]
    fn add_alias_works() {
        let mut m = mgr();
        m.add_alias("my-utf8", "utf-8");
        assert!(m.is_known("my-utf8"));
        assert_eq!(m.resolve("my-utf8"), Some("utf-8"));
    }

    // ----- CodingSystemInfo -----

    #[test]
    fn base_name_strips_suffix() {
        let info = CodingSystemInfo::new("utf-8-unix", "utf-8", 'U', EolType::Unix);
        assert_eq!(info.base_name(), "utf-8");

        let info2 = CodingSystemInfo::new("utf-8", "utf-8", 'U', EolType::Undecided);
        assert_eq!(info2.base_name(), "utf-8");
    }

    // ----- coding-system-list -----

    #[test]
    fn coding_system_list_all() {
        let m = mgr();
        let result = builtin_coding_system_list(&m, vec![]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert!(items.len() >= 11); // at least the 11 pre-registered systems
    }

    #[test]
    fn coding_system_list_base_only() {
        let m = mgr();
        let result = builtin_coding_system_list(&m, vec![Value::True]).unwrap();
        let items = list_to_vec(&result).unwrap();
        // Should not contain utf-8-unix, utf-8-dos, utf-8-mac
        for item in &items {
            if let Value::Symbol(s) = item {
                assert!(
                    !s.ends_with("-unix") && !s.ends_with("-dos") && !s.ends_with("-mac"),
                    "base-only list should not contain: {}",
                    s
                );
            }
        }
    }

    // ----- coding-system-aliases -----

    #[test]
    fn coding_system_aliases_found() {
        let m = mgr();
        let result = builtin_coding_system_aliases(&m, vec![Value::symbol("utf-8")]).unwrap();
        let items = list_to_vec(&result).unwrap();
        // First element should be the canonical name
        assert!(matches!(&items[0], Value::Symbol(s) if s == "utf-8"));
        // Should include aliases like mule-utf-8
        assert!(items.len() > 1);
    }

    #[test]
    fn coding_system_aliases_unknown() {
        let m = mgr();
        let result = builtin_coding_system_aliases(&m, vec![Value::symbol("nonexistent")]).unwrap();
        assert!(result.is_nil());
    }

    // ----- coding-system-get -----

    #[test]
    fn coding_system_get_name() {
        let m = mgr();
        let result =
            builtin_coding_system_get(&m, vec![Value::symbol("utf-8"), Value::symbol(":name")])
                .unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "utf-8"));
    }

    #[test]
    fn coding_system_get_type() {
        let m = mgr();
        let result =
            builtin_coding_system_get(&m, vec![Value::symbol("latin-1"), Value::symbol(":type")])
                .unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "charset"));
    }

    #[test]
    fn coding_system_get_mnemonic() {
        let m = mgr();
        let result =
            builtin_coding_system_get(&m, vec![Value::symbol("utf-8"), Value::symbol(":mnemonic")])
                .unwrap();
        assert!(matches!(result, Value::Char('U')));
    }

    #[test]
    fn coding_system_get_eol_type() {
        let m = mgr();
        let result = builtin_coding_system_get(
            &m,
            vec![Value::symbol("utf-8-unix"), Value::symbol(":eol-type")],
        )
        .unwrap();
        assert!(eq_value(&result, &Value::Int(0)));
    }

    #[test]
    fn coding_system_get_unknown_prop() {
        let m = mgr();
        let result = builtin_coding_system_get(
            &m,
            vec![Value::symbol("utf-8"), Value::symbol(":nonexistent")],
        )
        .unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn coding_system_get_unknown_system() {
        let m = mgr();
        let result =
            builtin_coding_system_get(&m, vec![Value::symbol("bogus"), Value::symbol(":name")])
                .unwrap();
        assert!(result.is_nil());
    }

    // ----- coding-system-put -----

    #[test]
    fn coding_system_put_custom_prop() {
        let mut m = mgr();
        let result = builtin_coding_system_put(
            &mut m,
            vec![
                Value::symbol("utf-8"),
                Value::symbol(":charset-list"),
                Value::list(vec![Value::symbol("unicode")]),
            ],
        )
        .unwrap();
        assert!(result.is_nil());

        // Verify it was stored
        let get_result = builtin_coding_system_get(
            &m,
            vec![Value::symbol("utf-8"), Value::symbol(":charset-list")],
        )
        .unwrap();
        assert!(!get_result.is_nil());
    }

    #[test]
    fn coding_system_put_mnemonic() {
        let mut m = mgr();
        builtin_coding_system_put(
            &mut m,
            vec![
                Value::symbol("utf-8"),
                Value::symbol(":mnemonic"),
                Value::Char('X'),
            ],
        )
        .unwrap();

        let result =
            builtin_coding_system_get(&m, vec![Value::symbol("utf-8"), Value::symbol(":mnemonic")])
                .unwrap();
        assert!(matches!(result, Value::Char('X')));
    }

    #[test]
    fn coding_system_put_unknown_system_errors() {
        let mut m = mgr();
        let result = builtin_coding_system_put(
            &mut m,
            vec![Value::symbol("bogus"), Value::symbol(":foo"), Value::Int(1)],
        );
        assert!(result.is_err());
    }

    // ----- coding-system-base -----

    #[test]
    fn coding_system_base_with_suffix() {
        let m = mgr();
        let result = builtin_coding_system_base(&m, vec![Value::symbol("utf-8-unix")]).unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "utf-8"));
    }

    #[test]
    fn coding_system_base_without_suffix() {
        let m = mgr();
        let result = builtin_coding_system_base(&m, vec![Value::symbol("utf-8")]).unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "utf-8"));
    }

    #[test]
    fn coding_system_base_unknown_still_strips() {
        let m = mgr();
        let result = builtin_coding_system_base(&m, vec![Value::symbol("foo-bar-unix")]).unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "foo-bar"));
    }

    // ----- coding-system-eol-type -----

    #[test]
    fn eol_type_unix() {
        let m = mgr();
        let result = builtin_coding_system_eol_type(&m, vec![Value::symbol("utf-8-unix")]).unwrap();
        assert!(eq_value(&result, &Value::Int(0)));
    }

    #[test]
    fn eol_type_dos() {
        let m = mgr();
        let result = builtin_coding_system_eol_type(&m, vec![Value::symbol("utf-8-dos")]).unwrap();
        assert!(eq_value(&result, &Value::Int(1)));
    }

    #[test]
    fn eol_type_mac() {
        let m = mgr();
        let result = builtin_coding_system_eol_type(&m, vec![Value::symbol("utf-8-mac")]).unwrap();
        assert!(eq_value(&result, &Value::Int(2)));
    }

    #[test]
    fn eol_type_undecided_returns_vector() {
        let m = mgr();
        let result = builtin_coding_system_eol_type(&m, vec![Value::symbol("utf-8")]).unwrap();
        // Should be a vector of [utf-8-unix utf-8-dos utf-8-mac]
        if let Value::Vector(v) = result {
            let locked = v.lock().unwrap();
            assert_eq!(locked.len(), 3);
            assert!(matches!(&locked[0], Value::Symbol(s) if s == "utf-8-unix"));
            assert!(matches!(&locked[1], Value::Symbol(s) if s == "utf-8-dos"));
            assert!(matches!(&locked[2], Value::Symbol(s) if s == "utf-8-mac"));
        } else {
            panic!("expected vector for undecided eol-type");
        }
    }

    #[test]
    fn eol_type_unknown_returns_nil() {
        let m = mgr();
        let result =
            builtin_coding_system_eol_type(&m, vec![Value::symbol("nonexistent")]).unwrap();
        assert!(result.is_nil());
    }

    // ----- coding-system-type -----

    #[test]
    fn coding_system_type_utf8() {
        let m = mgr();
        let result = builtin_coding_system_type(&m, vec![Value::symbol("utf-8")]).unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "utf-8"));
    }

    #[test]
    fn coding_system_type_raw_text() {
        let m = mgr();
        let result = builtin_coding_system_type(&m, vec![Value::symbol("raw-text")]).unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "raw-text"));
    }

    #[test]
    fn coding_system_type_unknown() {
        let m = mgr();
        let result = builtin_coding_system_type(&m, vec![Value::symbol("bogus")]).unwrap();
        assert!(result.is_nil());
    }

    // ----- coding-system-change-eol-conversion -----

    #[test]
    fn change_eol_by_int() {
        let m = mgr();
        let result = builtin_coding_system_change_eol_conversion(
            &m,
            vec![Value::symbol("utf-8"), Value::Int(1)],
        )
        .unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "utf-8-dos"));
    }

    #[test]
    fn change_eol_by_symbol() {
        let m = mgr();
        let result = builtin_coding_system_change_eol_conversion(
            &m,
            vec![Value::symbol("utf-8-unix"), Value::symbol("mac")],
        )
        .unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "utf-8-mac"));
    }

    #[test]
    fn change_eol_strips_existing_suffix() {
        let m = mgr();
        let result = builtin_coding_system_change_eol_conversion(
            &m,
            vec![Value::symbol("utf-8-dos"), Value::Int(0)],
        )
        .unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "utf-8-unix"));
    }

    // ----- coding-system-change-text-conversion -----

    #[test]
    fn change_text_conversion_preserves_eol() {
        let m = mgr();
        let result = builtin_coding_system_change_text_conversion(
            &m,
            vec![Value::symbol("utf-8-unix"), Value::symbol("latin-1")],
        )
        .unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "latin-1-unix"));
    }

    #[test]
    fn change_text_conversion_undecided_eol() {
        let m = mgr();
        let result = builtin_coding_system_change_text_conversion(
            &m,
            vec![Value::symbol("utf-8"), Value::symbol("latin-1")],
        )
        .unwrap();
        // utf-8 has undecided eol -> no suffix
        assert!(matches!(result, Value::Symbol(s) if s == "latin-1"));
    }

    // ----- detect-coding-string -----

    #[test]
    fn detect_coding_string_highest() {
        let m = mgr();
        let result =
            builtin_detect_coding_string(&m, vec![Value::string("hello"), Value::True]).unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "utf-8"));
    }

    #[test]
    fn detect_coding_string_list() {
        let m = mgr();
        let result = builtin_detect_coding_string(&m, vec![Value::string("hello")]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], Value::Symbol(s) if s == "utf-8"));
    }

    #[test]
    fn detect_coding_string_wrong_type() {
        let m = mgr();
        let result = builtin_detect_coding_string(&m, vec![Value::Int(42)]);
        assert!(result.is_err());
    }

    // ----- detect-coding-region -----

    #[test]
    fn detect_coding_region_highest() {
        let m = mgr();
        let result =
            builtin_detect_coding_region(&m, vec![Value::Int(1), Value::Int(100), Value::True])
                .unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "utf-8"));
    }

    #[test]
    fn detect_coding_region_list() {
        let m = mgr();
        let result =
            builtin_detect_coding_region(&m, vec![Value::Int(1), Value::Int(100)]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 1);
    }

    // ----- keyboard/terminal coding system -----

    #[test]
    fn keyboard_coding_system_default() {
        let m = mgr();
        let result = builtin_keyboard_coding_system(&m, vec![]).unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "utf-8"));
    }

    #[test]
    fn terminal_coding_system_default() {
        let m = mgr();
        let result = builtin_terminal_coding_system(&m, vec![]).unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "utf-8"));
    }

    #[test]
    fn set_keyboard_coding_system() {
        let mut m = mgr();
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("latin-1")]).unwrap();
        let result = builtin_keyboard_coding_system(&m, vec![]).unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "latin-1"));
    }

    #[test]
    fn set_terminal_coding_system() {
        let mut m = mgr();
        builtin_set_terminal_coding_system(&mut m, vec![Value::symbol("ascii")]).unwrap();
        let result = builtin_terminal_coding_system(&m, vec![]).unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "ascii"));
    }

    #[test]
    fn set_keyboard_coding_nil_resets_to_utf8() {
        let mut m = mgr();
        builtin_set_keyboard_coding_system(&mut m, vec![Value::symbol("latin-1")]).unwrap();
        builtin_set_keyboard_coding_system(&mut m, vec![Value::Nil]).unwrap();
        let result = builtin_keyboard_coding_system(&m, vec![]).unwrap();
        assert!(matches!(result, Value::Symbol(s) if s == "utf-8"));
    }

    // ----- coding-system-priority-list -----

    #[test]
    fn priority_list_full() {
        let m = mgr();
        let result = builtin_coding_system_priority_list(&m, vec![]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert!(!items.is_empty());
        // First should be utf-8
        assert!(matches!(&items[0], Value::Symbol(s) if s == "utf-8"));
    }

    #[test]
    fn priority_list_highest() {
        let m = mgr();
        let result = builtin_coding_system_priority_list(&m, vec![Value::True]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], Value::Symbol(s) if s == "utf-8"));
    }

    // ----- EolType -----

    #[test]
    fn eol_type_to_int() {
        assert_eq!(EolType::Unix.to_int(), 0);
        assert_eq!(EolType::Dos.to_int(), 1);
        assert_eq!(EolType::Mac.to_int(), 2);
        assert_eq!(EolType::Undecided.to_int(), 0);
    }

    #[test]
    fn eol_type_from_suffix() {
        assert_eq!(EolType::from_suffix("utf-8-unix"), Some(EolType::Unix));
        assert_eq!(EolType::from_suffix("utf-8-dos"), Some(EolType::Dos));
        assert_eq!(EolType::from_suffix("utf-8-mac"), Some(EolType::Mac));
        assert_eq!(EolType::from_suffix("utf-8"), None);
    }

    // ----- strip_eol_suffix -----

    #[test]
    fn strip_eol_suffix_works() {
        assert_eq!(strip_eol_suffix("utf-8-unix"), "utf-8");
        assert_eq!(strip_eol_suffix("utf-8-dos"), "utf-8");
        assert_eq!(strip_eol_suffix("utf-8-mac"), "utf-8");
        assert_eq!(strip_eol_suffix("utf-8"), "utf-8");
        assert_eq!(strip_eol_suffix("latin-1"), "latin-1");
    }

    // ----- argument validation -----

    #[test]
    fn coding_system_get_wrong_arg_count() {
        let m = mgr();
        let result = builtin_coding_system_get(&m, vec![Value::symbol("utf-8")]);
        assert!(result.is_err());
    }

    #[test]
    fn coding_system_base_wrong_arg_count() {
        let m = mgr();
        let result = builtin_coding_system_base(&m, vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn coding_system_aliases_wrong_arg_count() {
        let m = mgr();
        let result = builtin_coding_system_aliases(&m, vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn coding_system_p_reads_runtime_aliases() {
        let mut m = mgr();
        let before = builtin_coding_system_p(&m, vec![Value::symbol("vm-utf8")]).unwrap();
        assert!(before.is_nil());

        builtin_define_coding_system_alias(
            &mut m,
            vec![Value::symbol("vm-utf8"), Value::symbol("utf-8")],
        )
        .unwrap();
        let after = builtin_coding_system_p(&m, vec![Value::symbol("vm-utf8")]).unwrap();
        assert!(after.is_truthy());
    }

    #[test]
    fn check_coding_system_signals_unknown_symbols() {
        let m = mgr();
        let result = builtin_check_coding_system(&m, vec![Value::symbol("vm-no-such")]);
        match result {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "coding-system-error");
                assert_eq!(sig.data, vec![Value::symbol("vm-no-such")]);
            }
            other => panic!("expected coding-system-error signal, got {other:?}"),
        }
    }

    #[test]
    fn set_coding_system_priority_reorders_front_in_arg_order() {
        let mut m = mgr();
        builtin_set_coding_system_priority(
            &mut m,
            vec![Value::symbol("raw-text"), Value::symbol("utf-8")],
        )
        .unwrap();

        let list = builtin_coding_system_priority_list(&m, vec![]).unwrap();
        let items = list_to_vec(&list).unwrap();
        assert!(matches!(&items[0], Value::Symbol(s) if s == "raw-text"));
        assert!(matches!(&items[1], Value::Symbol(s) if s == "utf-8"));
    }

    #[test]
    fn set_coding_system_priority_rejects_nil_payload() {
        let mut m = mgr();
        let result = builtin_set_coding_system_priority(&mut m, vec![Value::Nil]);
        match result {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("coding-system-p"), Value::Nil]);
            }
            other => panic!("expected wrong-type-argument signal, got {other:?}"),
        }
    }
}
