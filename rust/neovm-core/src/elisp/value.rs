//! Lisp value representation and fundamental operations.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Core value types
// ---------------------------------------------------------------------------

/// Runtime Lisp value.
///
/// Uses `Arc` for shared ownership (will be replaced by GC in a later phase).
#[derive(Clone, Debug)]
pub enum Value {
    Nil,
    /// `t` — the canonical true value.
    True,
    Int(i64),
    Float(f64),
    Symbol(String),
    Keyword(String),
    Str(Arc<String>),
    Cons(Arc<Mutex<ConsCell>>),
    Vector(Arc<Mutex<Vec<Value>>>),
    HashTable(Arc<Mutex<LispHashTable>>),
    Lambda(Arc<LambdaData>),
    Macro(Arc<LambdaData>),
    Char(char),
    /// Subr = built-in function reference (name).  Dispatched by the evaluator.
    Subr(String),
    /// Compiled bytecode function.
    ByteCode(Arc<super::bytecode::ByteCodeFunction>),
    /// Buffer reference (opaque id into the BufferManager).
    Buffer(crate::buffer::BufferId),
    /// Timer reference (opaque id into the TimerManager).
    Timer(u64),
}

#[derive(Clone, Debug)]
pub struct ConsCell {
    pub car: Value,
    pub cdr: Value,
}

/// Shared representation for lambda and macro bodies.
#[derive(Clone, Debug)]
pub struct LambdaData {
    pub params: LambdaParams,
    pub body: Vec<super::expr::Expr>,
    /// For lexical closures: captured environment snapshot.
    pub env: Option<Vec<HashMap<String, Value>>>,
    pub docstring: Option<String>,
}

/// Describes a lambda parameter list including &optional and &rest.
#[derive(Clone, Debug)]
pub struct LambdaParams {
    pub required: Vec<String>,
    pub optional: Vec<String>,
    pub rest: Option<String>,
}

impl LambdaParams {
    pub fn simple(names: Vec<String>) -> Self {
        Self {
            required: names,
            optional: Vec::new(),
            rest: None,
        }
    }

    /// Total minimum arity.
    pub fn min_arity(&self) -> usize {
        self.required.len()
    }

    /// Total maximum arity (None = unbounded due to &rest).
    pub fn max_arity(&self) -> Option<usize> {
        if self.rest.is_some() {
            None
        } else {
            Some(self.required.len() + self.optional.len())
        }
    }
}

/// Hash table with configurable test function.
#[derive(Clone, Debug)]
pub struct LispHashTable {
    pub test: HashTableTest,
    pub data: HashMap<HashKey, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HashTableTest {
    Eq,
    Eql,
    Equal,
}

/// Key type that supports hashing for `eq`, `eql`, and `equal` tests.
/// For simplicity, we normalize keys to a hashable representation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum HashKey {
    Nil,
    True,
    Int(i64),
    Float(u64), // bits
    Symbol(String),
    Keyword(String),
    Str(String),
    Char(char),
    /// Pointer identity for eq hash tables.
    Ptr(usize),
}

impl LispHashTable {
    pub fn new(test: HashTableTest) -> Self {
        Self {
            test,
            data: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Value constructors
// ---------------------------------------------------------------------------

impl Value {
    pub fn t() -> Self {
        Value::True
    }

    pub fn bool(b: bool) -> Self {
        if b { Value::True } else { Value::Nil }
    }

    pub fn symbol(s: impl Into<String>) -> Self {
        let s = s.into();
        if s == "nil" {
            Value::Nil
        } else if s == "t" {
            Value::True
        } else {
            Value::Symbol(s)
        }
    }

    pub fn keyword(s: impl Into<String>) -> Self {
        Value::Keyword(s.into())
    }

    pub fn string(s: impl Into<String>) -> Self {
        Value::Str(Arc::new(s.into()))
    }

    pub fn cons(car: Value, cdr: Value) -> Self {
        Value::Cons(Arc::new(Mutex::new(ConsCell { car, cdr })))
    }

    pub fn list(values: Vec<Value>) -> Self {
        values.into_iter().rev().fold(Value::Nil, |acc, item| {
            Value::cons(item, acc)
        })
    }

    pub fn vector(values: Vec<Value>) -> Self {
        Value::Vector(Arc::new(Mutex::new(values)))
    }

    pub fn hash_table(test: HashTableTest) -> Self {
        Value::HashTable(Arc::new(Mutex::new(LispHashTable::new(test))))
    }

    pub fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }

    pub fn is_truthy(&self) -> bool {
        !self.is_nil()
    }

    pub fn is_list(&self) -> bool {
        matches!(self, Value::Nil | Value::Cons(_))
    }

    pub fn is_cons(&self) -> bool {
        matches!(self, Value::Cons(_))
    }

    pub fn is_number(&self) -> bool {
        matches!(self, Value::Int(_) | Value::Float(_))
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, Value::Int(_))
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Value::Float(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Value::Str(_))
    }

    pub fn is_symbol(&self) -> bool {
        matches!(self, Value::Nil | Value::True | Value::Symbol(_))
    }

    pub fn is_keyword(&self) -> bool {
        matches!(self, Value::Keyword(_))
    }

    pub fn is_vector(&self) -> bool {
        matches!(self, Value::Vector(_))
    }

    pub fn is_char(&self) -> bool {
        matches!(self, Value::Char(_))
    }

    pub fn is_hash_table(&self) -> bool {
        matches!(self, Value::HashTable(_))
    }

    pub fn is_function(&self) -> bool {
        matches!(self, Value::Lambda(_) | Value::Subr(_) | Value::ByteCode(_))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "symbol",
            Value::True => "symbol",
            Value::Int(_) => "integer",
            Value::Float(_) => "float",
            Value::Symbol(_) => "symbol",
            Value::Keyword(_) => "symbol",
            Value::Str(_) => "string",
            Value::Cons(_) => "cons",
            Value::Vector(_) => "vector",
            Value::HashTable(_) => "hash-table",
            Value::Lambda(_) => "function",
            Value::Macro(_) => "macro",
            Value::Char(_) => "integer", // Emacs chars are integers
            Value::Subr(_) => "subr",
            Value::ByteCode(_) => "compiled-function",
            Value::Buffer(_) => "buffer",
            Value::Timer(_) => "timer",
        }
    }

    /// Extract as number (int or float).  Promotes int → float if needed.
    pub fn as_number_f64(&self) -> Option<f64> {
        match self {
            Value::Int(n) => Some(*n as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            Value::Char(c) => Some(*c as i64),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_symbol_name(&self) -> Option<&str> {
        match self {
            Value::Nil => Some("nil"),
            Value::True => Some("t"),
            Value::Symbol(s) => Some(s.as_str()),
            Value::Keyword(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Convert to hash key based on the hash table test.
    pub fn to_hash_key(&self, test: &HashTableTest) -> HashKey {
        match test {
            HashTableTest::Eq => self.to_eq_key(),
            HashTableTest::Eql => self.to_eql_key(),
            HashTableTest::Equal => self.to_equal_key(),
        }
    }

    fn to_eq_key(&self) -> HashKey {
        match self {
            Value::Nil => HashKey::Nil,
            Value::True => HashKey::True,
            Value::Int(n) => HashKey::Int(*n),
            Value::Float(f) => HashKey::Float(f.to_bits()),
            Value::Symbol(s) => HashKey::Symbol(s.clone()),
            Value::Keyword(s) => HashKey::Keyword(s.clone()),
            Value::Char(c) => HashKey::Char(*c),
            // For eq, use pointer identity
            Value::Cons(c) => HashKey::Ptr(Arc::as_ptr(c) as usize),
            Value::Vector(v) => HashKey::Ptr(Arc::as_ptr(v) as usize),
            Value::Str(s) => HashKey::Ptr(Arc::as_ptr(s) as usize),
            Value::Lambda(l) => HashKey::Ptr(Arc::as_ptr(l) as usize),
            Value::Macro(m) => HashKey::Ptr(Arc::as_ptr(m) as usize),
            Value::HashTable(h) => HashKey::Ptr(Arc::as_ptr(h) as usize),
            Value::Subr(n) => HashKey::Symbol(n.clone()),
            Value::ByteCode(b) => HashKey::Ptr(Arc::as_ptr(b) as usize),
            Value::Buffer(id) => HashKey::Int(id.0 as i64),
            Value::Timer(id) => HashKey::Int(*id as i64),
        }
    }

    fn to_eql_key(&self) -> HashKey {
        match self {
            // eql is like eq but also does value-equality for numbers
            Value::Int(n) => HashKey::Int(*n),
            Value::Float(f) => HashKey::Float(f.to_bits()),
            Value::Char(c) => HashKey::Char(*c),
            other => other.to_eq_key(),
        }
    }

    fn to_equal_key(&self) -> HashKey {
        match self {
            Value::Nil => HashKey::Nil,
            Value::True => HashKey::True,
            Value::Int(n) => HashKey::Int(*n),
            Value::Float(f) => HashKey::Float(f.to_bits()),
            Value::Symbol(s) => HashKey::Symbol(s.clone()),
            Value::Keyword(s) => HashKey::Keyword(s.clone()),
            Value::Str(s) => HashKey::Str((**s).clone()),
            Value::Char(c) => HashKey::Char(*c),
            // For compound types, fall back to eq identity
            other => other.to_eq_key(),
        }
    }
}

// ---------------------------------------------------------------------------
// Equality
// ---------------------------------------------------------------------------

/// `eq` — identity comparison.
pub fn eq_value(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Nil, Value::Nil) => true,
        (Value::True, Value::True) => true,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
        (Value::Char(a), Value::Char(b)) => a == b,
        (Value::Symbol(a), Value::Symbol(b)) => a == b,
        (Value::Keyword(a), Value::Keyword(b)) => a == b,
        (Value::Str(a), Value::Str(b)) => Arc::ptr_eq(a, b),
        (Value::Cons(a), Value::Cons(b)) => Arc::ptr_eq(a, b),
        (Value::Vector(a), Value::Vector(b)) => Arc::ptr_eq(a, b),
        (Value::Lambda(a), Value::Lambda(b)) => Arc::ptr_eq(a, b),
        (Value::Macro(a), Value::Macro(b)) => Arc::ptr_eq(a, b),
        (Value::HashTable(a), Value::HashTable(b)) => Arc::ptr_eq(a, b),
        (Value::Subr(a), Value::Subr(b)) => a == b,
        (Value::ByteCode(a), Value::ByteCode(b)) => Arc::ptr_eq(a, b),
        (Value::Buffer(a), Value::Buffer(b)) => a == b,
        (Value::Timer(a), Value::Timer(b)) => a == b,
        _ => false,
    }
}

/// `eql` — like `eq` but also value-equality for numbers of same type.
pub fn eql_value(left: &Value, right: &Value) -> bool {
    eq_value(left, right)
}

/// `equal` — structural comparison.
pub fn equal_value(left: &Value, right: &Value, depth: usize) -> bool {
    if depth > 4096 {
        return false;
    }
    match (left, right) {
        (Value::Nil, Value::Nil) => true,
        (Value::True, Value::True) => true,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
        (Value::Char(a), Value::Char(b)) => a == b,
        (Value::Symbol(a), Value::Symbol(b)) => a == b,
        (Value::Keyword(a), Value::Keyword(b)) => a == b,
        (Value::Str(a), Value::Str(b)) => **a == **b,
        (Value::Cons(a), Value::Cons(b)) => {
            if Arc::ptr_eq(a, b) {
                return true;
            }
            let a = a.lock().expect("poisoned");
            let b = b.lock().expect("poisoned");
            equal_value(&a.car, &b.car, depth + 1)
                && equal_value(&a.cdr, &b.cdr, depth + 1)
        }
        (Value::Vector(a), Value::Vector(b)) => {
            if Arc::ptr_eq(a, b) {
                return true;
            }
            let a = a.lock().expect("poisoned");
            let b = b.lock().expect("poisoned");
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|(x, y)| equal_value(x, y, depth + 1))
        }
        (Value::Lambda(a), Value::Lambda(b)) => Arc::ptr_eq(a, b),
        (Value::Macro(a), Value::Macro(b)) => Arc::ptr_eq(a, b),
        (Value::Subr(a), Value::Subr(b)) => a == b,
        (Value::ByteCode(a), Value::ByteCode(b)) => Arc::ptr_eq(a, b),
        (Value::Buffer(a), Value::Buffer(b)) => a == b,
        (Value::Timer(a), Value::Timer(b)) => a == b,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// List iteration helpers
// ---------------------------------------------------------------------------

/// Collect a proper list into a Vec.  Returns None if not a proper list.
pub fn list_to_vec(value: &Value) -> Option<Vec<Value>> {
    let mut result = Vec::new();
    let mut cursor = value.clone();
    loop {
        match cursor {
            Value::Nil => return Some(result),
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                result.push(pair.car.clone());
                cursor = pair.cdr.clone();
            }
            _ => return None,
        }
    }
}

/// Length of a list (counts cons cells).  Returns None if improper list detected.
pub fn list_length(value: &Value) -> Option<usize> {
    let mut len = 0;
    let mut cursor = value.clone();
    loop {
        match cursor {
            Value::Nil => return Some(len),
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                len += 1;
                cursor = pair.cdr.clone();
            }
            _ => return None,
        }
    }
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", super::print::print_value(self))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_constructors() {
        assert!(Value::Nil.is_nil());
        assert!(Value::t().is_truthy());
        assert!(Value::Int(42).is_integer());
        assert!(Value::Float(3.14).is_float());
        assert!(Value::string("hello").is_string());
        assert!(Value::Char('a').is_char());
        assert!(Value::symbol("foo").is_symbol());
        assert!(Value::keyword(":bar").is_keyword());
    }

    #[test]
    fn list_round_trip() {
        let lst = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let vec = list_to_vec(&lst).unwrap();
        assert_eq!(vec.len(), 3);
    }

    #[test]
    fn eq_identity() {
        assert!(eq_value(&Value::Nil, &Value::Nil));
        assert!(eq_value(&Value::Int(42), &Value::Int(42)));
        assert!(!eq_value(&Value::Int(1), &Value::Int(2)));
        assert!(eq_value(&Value::symbol("foo"), &Value::symbol("foo")));
    }

    #[test]
    fn equal_structural() {
        let a = Value::list(vec![Value::Int(1), Value::Int(2)]);
        let b = Value::list(vec![Value::Int(1), Value::Int(2)]);
        assert!(equal_value(&a, &b, 0));
        assert!(!eq_value(&a, &b));
    }

    #[test]
    fn string_equality() {
        let a = Value::string("hello");
        let b = Value::string("hello");
        assert!(equal_value(&a, &b, 0));
        // eq compares Arc pointers — different Arcs
        assert!(!eq_value(&a, &b));
    }

    #[test]
    fn lambda_params_arity() {
        let p = LambdaParams {
            required: vec!["a".into(), "b".into()],
            optional: vec!["c".into()],
            rest: None,
        };
        assert_eq!(p.min_arity(), 2);
        assert_eq!(p.max_arity(), Some(3));

        let p2 = LambdaParams {
            required: vec!["a".into()],
            optional: vec![],
            rest: Some("rest".into()),
        };
        assert_eq!(p2.min_arity(), 1);
        assert_eq!(p2.max_arity(), None);
    }
}
