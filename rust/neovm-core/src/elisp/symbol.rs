//! Obarray and symbol interning.
//!
//! In Emacs, symbols are unique objects stored in an "obarray" (hash table).
//! Each symbol has:
//! - A name (string)
//! - A value cell (variable binding)
//! - A function cell (function binding)
//! - A property list (plist)
//! - A `special` flag (for dynamic binding in lexical scope)

use super::value::Value;
use std::collections::{HashMap, HashSet};

/// Per-symbol metadata stored in the obarray.
#[derive(Clone, Debug)]
pub struct SymbolData {
    /// The symbol's name.
    pub name: String,
    /// Value cell (None = void/unbound).
    pub value: Option<Value>,
    /// Function cell (None = void-function).
    pub function: Option<Value>,
    /// Property list (flat alternating key-value pairs stored as HashMap).
    pub plist: HashMap<String, Value>,
    /// Whether this symbol is declared `special` (always dynamically bound).
    pub special: bool,
    /// Whether this symbol is a constant (defconst).
    pub constant: bool,
}

impl SymbolData {
    pub fn new(name: String) -> Self {
        Self {
            name,
            value: None,
            function: None,
            plist: HashMap::new(),
            special: false,
            constant: false,
        }
    }
}

/// The obarray — a table of interned symbols.
///
/// This is the central symbol registry. `intern` looks up or creates symbols,
/// ensuring that `(eq 'foo 'foo)` is always true.
#[derive(Clone, Debug)]
pub struct Obarray {
    symbols: HashMap<String, SymbolData>,
    function_unbound: HashSet<String>,
    function_epoch: u64,
}

impl Default for Obarray {
    fn default() -> Self {
        Self::new()
    }
}

impl Obarray {
    pub fn new() -> Self {
        let mut ob = Self {
            symbols: HashMap::new(),
            function_unbound: HashSet::new(),
            function_epoch: 0,
        };

        // Pre-intern fundamental symbols
        let mut t_sym = SymbolData::new("t".to_string());
        t_sym.value = Some(Value::True);
        t_sym.constant = true;
        t_sym.special = true;
        ob.symbols.insert("t".to_string(), t_sym);

        let mut nil_sym = SymbolData::new("nil".to_string());
        nil_sym.value = Some(Value::Nil);
        nil_sym.constant = true;
        nil_sym.special = true;
        ob.symbols.insert("nil".to_string(), nil_sym);

        ob
    }

    /// Intern a symbol: look up by name, creating if absent.
    /// Returns the symbol name (which is the key for identity).
    pub fn intern(&mut self, name: &str) -> String {
        if !self.symbols.contains_key(name) {
            self.symbols
                .insert(name.to_string(), SymbolData::new(name.to_string()));
        }
        name.to_string()
    }

    /// Look up a symbol without creating it. Returns None if not interned.
    pub fn intern_soft(&self, name: &str) -> Option<&SymbolData> {
        self.symbols.get(name)
    }

    /// Get symbol data (mutable). Interns the symbol if needed.
    pub fn get_or_intern(&mut self, name: &str) -> &mut SymbolData {
        if !self.symbols.contains_key(name) {
            self.symbols
                .insert(name.to_string(), SymbolData::new(name.to_string()));
        }
        self.symbols.get_mut(name).unwrap()
    }

    /// Get symbol data (immutable).
    pub fn get(&self, name: &str) -> Option<&SymbolData> {
        self.symbols.get(name)
    }

    /// Get symbol data (mutable).
    pub fn get_mut(&mut self, name: &str) -> Option<&mut SymbolData> {
        self.symbols.get_mut(name)
    }

    /// Get the value cell of a symbol.
    pub fn symbol_value(&self, name: &str) -> Option<&Value> {
        self.symbols.get(name).and_then(|s| s.value.as_ref())
    }

    /// Set the value cell of a symbol. Interns if needed.
    pub fn set_symbol_value(&mut self, name: &str, value: Value) {
        let sym = self.get_or_intern(name);
        sym.value = Some(value);
    }

    /// Get the function cell of a symbol.
    pub fn symbol_function(&self, name: &str) -> Option<&Value> {
        if self.function_unbound.contains(name) {
            return None;
        }
        self.symbols.get(name).and_then(|s| s.function.as_ref())
    }

    /// Set the function cell of a symbol (fset). Interns if needed.
    pub fn set_symbol_function(&mut self, name: &str, function: Value) {
        let sym = self.get_or_intern(name);
        sym.function = Some(function);
        self.function_unbound.remove(name);
        self.function_epoch = self.function_epoch.wrapping_add(1);
    }

    /// Remove the function cell (fmakunbound).
    pub fn fmakunbound(&mut self, name: &str) {
        let mut changed = self.function_unbound.insert(name.to_string());
        if let Some(sym) = self.symbols.get_mut(name) {
            changed |= sym.function.take().is_some();
        }
        if changed {
            self.function_epoch = self.function_epoch.wrapping_add(1);
        }
    }

    /// Remove the value cell (makunbound).
    pub fn makunbound(&mut self, name: &str) {
        if let Some(sym) = self.symbols.get_mut(name) {
            if !sym.constant {
                sym.value = None;
            }
        }
    }

    /// Check if a symbol is bound (has a value cell).
    pub fn boundp(&self, name: &str) -> bool {
        self.symbols.get(name).is_some_and(|s| s.value.is_some())
    }

    /// Check if a symbol has a function cell.
    pub fn fboundp(&self, name: &str) -> bool {
        if self.function_unbound.contains(name) {
            return false;
        }
        self.symbols
            .get(name)
            .and_then(|s| s.function.as_ref())
            .is_some_and(|f| !f.is_nil())
    }

    /// Get a property from the symbol's plist.
    pub fn get_property(&self, name: &str, prop: &str) -> Option<&Value> {
        self.symbols.get(name).and_then(|s| s.plist.get(prop))
    }

    /// Set a property on the symbol's plist.
    pub fn put_property(&mut self, name: &str, prop: &str, value: Value) {
        let sym = self.get_or_intern(name);
        sym.plist.insert(prop.to_string(), value);
    }

    /// Get the symbol's full plist as a flat list.
    pub fn symbol_plist(&self, name: &str) -> Value {
        match self.symbols.get(name) {
            Some(sym) if !sym.plist.is_empty() => {
                let mut items = Vec::new();
                for (k, v) in &sym.plist {
                    items.push(Value::symbol(k.clone()));
                    items.push(v.clone());
                }
                Value::list(items)
            }
            _ => Value::Nil,
        }
    }

    /// Mark a symbol as special (dynamically bound).
    pub fn make_special(&mut self, name: &str) {
        self.get_or_intern(name).special = true;
    }

    /// Check if a symbol is special.
    pub fn is_special(&self, name: &str) -> bool {
        self.symbols.get(name).is_some_and(|s| s.special)
    }

    /// Check if a symbol is a constant.
    pub fn is_constant(&self, name: &str) -> bool {
        self.symbols.get(name).is_some_and(|s| s.constant)
    }

    /// Follow function indirection (defalias chains).
    /// Returns the final function value, following symbol aliases.
    pub fn indirect_function(&self, name: &str) -> Option<Value> {
        let mut current = name;
        let mut depth = 0;
        loop {
            if depth > 100 {
                return None; // Circular alias chain
            }
            let func = self.symbols.get(current)?.function.as_ref()?;
            match func {
                Value::Symbol(next) => {
                    current = next.as_str();
                    depth += 1;
                }
                _ => return Some(func.clone()),
            }
        }
    }

    /// Number of interned symbols.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// All interned symbol names.
    pub fn all_symbols(&self) -> Vec<&str> {
        self.symbols.keys().map(|s| s.as_str()).collect()
    }

    /// Remove a symbol from the obarray.  Returns `true` if it was present.
    pub fn unintern(&mut self, name: &str) -> bool {
        let removed_symbol = self.symbols.remove(name).is_some();
        let removed_unbound = self.function_unbound.remove(name);
        if removed_symbol || removed_unbound {
            self.function_epoch = self.function_epoch.wrapping_add(1);
        }
        removed_symbol
    }

    /// Monotonic epoch for function-cell mutations.
    pub fn function_epoch(&self) -> u64 {
        self.function_epoch
    }

    /// True when `fmakunbound` explicitly masked this symbol's fallback function definition.
    pub fn is_function_unbound(&self, name: &str) -> bool {
        self.function_unbound.contains(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_creates_symbol() {
        let mut ob = Obarray::new();
        ob.intern("foo");
        assert!(ob.intern_soft("foo").is_some());
        assert!(ob.intern_soft("bar").is_none());
    }

    #[test]
    fn symbol_value_cell() {
        let mut ob = Obarray::new();
        assert!(!ob.boundp("x"));
        ob.set_symbol_value("x", Value::Int(42));
        assert!(ob.boundp("x"));
        assert_eq!(ob.symbol_value("x").unwrap().as_int(), Some(42));
    }

    #[test]
    fn symbol_function_cell() {
        let mut ob = Obarray::new();
        assert!(!ob.fboundp("f"));
        let start_epoch = ob.function_epoch();
        ob.set_symbol_function("f", Value::Subr("+".to_string()));
        assert!(ob.fboundp("f"));
        assert!(ob.function_epoch() > start_epoch);
        let after_set_epoch = ob.function_epoch();
        ob.fmakunbound("f");
        assert!(!ob.fboundp("f"));
        assert!(ob.function_epoch() > after_set_epoch);
    }

    #[test]
    fn fmakunbound_masks_builtin_fallback_name() {
        let mut ob = Obarray::new();
        let start_epoch = ob.function_epoch();
        ob.fmakunbound("car");
        assert!(ob.is_function_unbound("car"));
        assert!(!ob.fboundp("car"));
        assert!(ob.symbol_function("car").is_none());
        assert!(ob.function_epoch() > start_epoch);

        ob.set_symbol_function("car", Value::Subr("car".to_string()));
        assert!(!ob.is_function_unbound("car"));
        assert!(ob.fboundp("car"));
    }

    #[test]
    fn symbol_properties() {
        let mut ob = Obarray::new();
        ob.put_property("foo", "doc", Value::string("A function."));
        assert_eq!(
            ob.get_property("foo", "doc").unwrap().as_str(),
            Some("A function.")
        );
    }

    #[test]
    fn special_flag() {
        let mut ob = Obarray::new();
        assert!(!ob.is_special("x"));
        ob.make_special("x");
        assert!(ob.is_special("x"));
    }

    #[test]
    fn indirect_function_follows_chain() {
        let mut ob = Obarray::new();
        ob.set_symbol_function("real-fn", Value::Subr("+".to_string()));
        // alias -> real-fn
        ob.set_symbol_function("alias", Value::Symbol("real-fn".to_string()));
        let resolved = ob.indirect_function("alias").unwrap();
        assert!(matches!(resolved, Value::Subr(ref n) if n == "+"));
    }

    #[test]
    fn t_and_nil_are_preinterned() {
        let ob = Obarray::new();
        assert!(ob.is_constant("t"));
        assert!(ob.is_constant("nil"));
        assert!(ob.is_special("t"));
        assert!(ob.is_special("nil"));
    }

    #[test]
    fn makunbound_doesnt_touch_constants() {
        let mut ob = Obarray::new();
        ob.makunbound("t");
        assert!(ob.boundp("t")); // t is constant, can't unbind
    }
}
