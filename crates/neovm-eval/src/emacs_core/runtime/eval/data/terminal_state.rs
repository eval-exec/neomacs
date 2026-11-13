//! Terminal Lisp registry state (stage-4 K2 oracleb batch 9).
//!
//! The Lisp-visible terminal registry (handles and parameters) and its pure
//! GC-root collection moved here from display/terminal/pure.rs.  The native
//! TerminalManager and its thread-locals stay in the display layer; the
//! Context/World state and the carrier hooks reach the registry through this
//! module and the cold terminal table.

use neovm_value::heap_registry::HeapRegistryHandle;
use neovm_value::value::Value;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

pub struct TerminalLispState {
    pub handle: Value,
    pub params: Vec<(Value, Value)>,
}

/// Only Lisp state travels with Context; native hosts and runtime state stay TLS.
#[derive(Default)]
pub struct TerminalLispRegistry {
    pub terminals: HashMap<u64, TerminalLispState>,
    // GNU prepends native terminals, then Fterminal_list reverses them again.
    // Preserve creation order independently of hash iteration and terminal ids.
    pub creation_order: Vec<u64>,
}

impl TerminalLispRegistry {
    pub fn ensure_terminal(&mut self, id: u64) -> &mut TerminalLispState {
        match self.terminals.entry(id) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                self.creation_order.push(id);
                entry.insert(TerminalLispState {
                    handle: Value::make_terminal(id),
                    params: Vec::new(),
                })
            }
        }
    }
}

pub type TerminalRegistryHandle = HeapRegistryHandle<TerminalLispRegistry>;

pub fn collect_terminal_registry_gc_roots(handle: &TerminalRegistryHandle, roots: &mut Vec<Value>) {
    for terminal in handle.borrow().terminals.values() {
        roots.push(terminal.handle);
        for (key, value) in &terminal.params {
            roots.push(*key);
            roots.push(*value);
        }
    }
}
