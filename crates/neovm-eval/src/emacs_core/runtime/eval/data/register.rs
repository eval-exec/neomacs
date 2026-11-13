//! Register manager state (stage-4 K2 oracleb batch 7).
//!
//! The register content vocabulary and the manager's pure methods/GC trace
//! were moved here verbatim from `commands/register` per the design's
//! eval-data disposition for the register WorldSlot.  The Lisp entry bodies
//! stay in the core command module and reach the manager through the Context
//! field.

use neovm_value::gc_trace::GcTrace;
use neovm_value::heap_types::LispString;
use neovm_value::value::Value;
use std::collections::HashMap;

/// The different kinds of data that can be stored in a register.
#[derive(Clone, Debug)]
pub enum RegisterContent {
    /// Text string, including its text properties.
    Text(LispString),
    /// An integer value.
    Number(i64),
    /// A saved point location as a live marker, matching GNU register.el.
    Marker(Value),
    /// A rectangle (list of Lisp strings, one per line), matching GNU.
    Rectangle(Vec<LispString>),
    /// A saved window/frame configuration (opaque Lisp value).
    FrameConfig(Value),
    /// A file name (for `set-register` with file references).
    File(LispString),
    /// A keyboard macro (sequence of key events).
    KbdMacro(Vec<Value>),
}

impl RegisterContent {
    /// Return a short human-readable description of the content kind.
    #[doc(hidden)]
    pub fn description(&self) -> &str {
        match self {
            RegisterContent::Text(_) => "text",
            RegisterContent::Number(_) => "number",
            RegisterContent::Marker(_) => "marker",
            RegisterContent::Rectangle(_) => "rectangle",
            RegisterContent::FrameConfig(_) => "frame-config",
            RegisterContent::File(_) => "file",
            RegisterContent::KbdMacro(_) => "kbd-macro",
        }
    }
}

/// Central registry for all registers.
#[derive(Clone, Debug)]
pub struct RegisterManager {
    registers: HashMap<char, RegisterContent>,
}

impl Default for RegisterManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RegisterManager {
    /// Create a new empty register manager.
    pub fn new() -> Self {
        Self {
            registers: HashMap::new(),
        }
    }

    /// Store content in a register, replacing any previous content.
    pub fn set(&mut self, register: char, content: RegisterContent) {
        self.registers.insert(register, content);
    }

    /// Retrieve the content of a register, if any.
    pub fn get(&self, register: char) -> Option<&RegisterContent> {
        self.registers.get(&register)
    }

    /// Clear a single register.
    pub fn clear(&mut self, register: char) {
        self.registers.remove(&register);
    }

    /// Clear all registers.
    pub fn clear_all(&mut self) {
        self.registers.clear();
    }

    /// Return a sorted list of (register-char, description) pairs for all
    /// non-empty registers.
    pub fn list(&self) -> Vec<(char, &str)> {
        let mut entries: Vec<(char, &str)> = self
            .registers
            .iter()
            .map(|(&ch, content)| (ch, content.description()))
            .collect();
        entries.sort_by_key(|(ch, _)| *ch);
        entries
    }

    /// Convenience: get the text stored in a register, if it holds text.
    pub fn get_text(&self, register: char) -> Option<&str> {
        match self.registers.get(&register) {
            Some(RegisterContent::Text(s)) => s.as_utf8_str(),
            _ => None,
        }
    }

    /// Append (or prepend) text to a register that already holds text.
    /// If the register is empty or not text, it becomes a Text register
    /// containing just the new text.
    pub fn append_text(&mut self, register: char, text: &str, prepend: bool) {
        let make_text = |multibyte: bool| {
            crate::emacs_core::builtins::plain_str_to_lisp_string(text, multibyte)
        };
        match self.registers.get_mut(&register) {
            Some(RegisterContent::Text(existing)) => {
                let new = make_text(existing.is_multibyte() || !text.is_ascii());
                if prepend {
                    *existing = new.concat(existing);
                } else {
                    *existing = existing.concat(&new);
                }
            }
            _ => {
                self.registers
                    .insert(register, RegisterContent::Text(make_text(!text.is_ascii())));
            }
        }
    }

    // pdump accessors
    #[doc(hidden)]
    pub fn dump_registers(&self) -> &HashMap<char, RegisterContent> {
        &self.registers
    }
    #[doc(hidden)]
    pub fn from_dump(registers: HashMap<char, RegisterContent>) -> Self {
        Self { registers }
    }
}

impl GcTrace for RegisterManager {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        self.trace_roots_with(&mut |value| roots.push(value));
    }

    fn trace_roots_with(&self, visit: &mut dyn FnMut(Value)) {
        let Self { registers } = self;
        for content in registers.values() {
            match content {
                RegisterContent::Text(text) => text.trace_roots_with(visit),
                RegisterContent::Rectangle(lines) => {
                    for line in lines {
                        line.trace_roots_with(visit);
                    }
                }
                RegisterContent::File(file) => file.trace_roots_with(visit),
                RegisterContent::Marker(v) | RegisterContent::FrameConfig(v) => {
                    visit(*v);
                }
                RegisterContent::KbdMacro(keys) => {
                    for v in keys {
                        visit(*v);
                    }
                }
                RegisterContent::Number(_) => {}
            }
        }
    }
}
