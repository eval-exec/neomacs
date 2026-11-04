//! Keyboard-macro metadata shared with the evaluator.
//!
//! GNU keeps the live recording/playback state on the keyboard runtime
//! (`current_kboard` plus global execution vars) and layers richer kmacro UI
//! state on top. NeoVM mirrors that split: the keyboard owner handles current
//! recording/playback, while this manager keeps only the higher-level ring and
//! counter metadata.

use crate::emacs_char::to_utf8_lossy;
use crate::gc_trace::GcTrace;
use crate::heap_types::LispString;
use crate::value::Value;

/// Metadata manager for keyboard macros.
#[derive(Clone, Debug)]
pub struct KmacroManager {
    /// Ring of previously saved macros (most recent first).
    pub macro_ring: Vec<Vec<Value>>,
    /// Keyboard macro counter (for `kmacro-insert-counter`).
    pub counter: i64,
    /// Format string for the counter (printf-style, default "%d").
    pub counter_format: LispString,
}

impl Default for KmacroManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GcTrace for KmacroManager {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        let Self {
            macro_ring,
            counter: _,
            counter_format,
        } = self;
        for macro_entry in macro_ring {
            for value in macro_entry {
                roots.push(*value);
            }
        }
        counter_format.trace_roots(roots);
    }
}

impl KmacroManager {
    /// Create a new manager with default state.
    pub fn new() -> Self {
        Self {
            macro_ring: Vec::new(),
            counter: 0,
            counter_format: LispString::from_utf8("%d"),
        }
    }

    /// Format the counter using the current format string.
    pub fn format_counter(&self) -> String {
        // Support basic %d / %o / %x / %X formats.
        // For anything more complex, fall back to decimal.
        let fmt = to_utf8_lossy(self.counter_format.as_bytes());
        if fmt.contains("%d") {
            fmt.replace("%d", &self.counter.to_string())
        } else if fmt.contains("%o") {
            fmt.replace("%o", &format!("{:o}", self.counter))
        } else if fmt.contains("%x") {
            fmt.replace("%x", &format!("{:x}", self.counter))
        } else if fmt.contains("%X") {
            fmt.replace("%X", &format!("{:X}", self.counter))
        } else {
            // Fallback: just print the number.
            self.counter.to_string()
        }
    }
}
