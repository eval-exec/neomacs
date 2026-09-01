# NeoVM Tagged Pointer Conversion Guide

This document is a practical, action-oriented reference for converting the NeoVM
codebase from the `ObjId`-based `Value` enum to the `TaggedValue` tagged-pointer
system.  Every section gives **before** and **after** code, the mechanical rule,
and known gotchas.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Value Type Rename](#2-value-type-rename)
3. [Constructor Conversions](#3-constructor-conversions)
4. [Pattern Matching Conversions](#4-pattern-matching-conversions)
5. [Accessor Conversions](#5-accessor-conversions)
6. [Type Check Conversions](#6-type-check-conversions)
7. [Heap Allocation Changes](#7-heap-allocation-changes)
8. [Exhaustive Match via ValueKind](#8-exhaustive-match-via-valuekind)
9. [Eliminated Concepts](#9-eliminated-concepts)
10. [GC and Rooting Changes](#10-gc-and-rooting-changes)
11. [File-by-File Conversion Order](#11-file-by-file-conversion-order)
12. [HashKey and Equality](#12-hashkey-and-equality)
13. [Common Pitfalls](#13-common-pitfalls)

---

## 1. Architecture Overview

### Old system (`Value` enum + `ObjId`)

```
Value (enum, 16+ bytes)           ObjId (8 bytes)
  ├── Nil                           ├── index: u32
  ├── True                          └── generation: u32
  ├── Int(i64)
  ├── Float(f64, u32)             LispHeap
  ├── Symbol(SymId)                 ├── objects: Vec<HeapObject>
  ├── Keyword(SymId)                ├── generations: Vec<u32>
  ├── Char(char)                    ├── marks: Vec<bool>
  ├── Subr(SymId)                   └── free_list: Vec<u32>
  ├── Str(ObjId)       ──────►
  ├── Cons(ObjId)      ──────►   Thread-local access:
  ├── Vector(ObjId)    ──────►     with_heap(|h| ...)
  ├── Record(ObjId)    ──────►     with_heap_mut(|h| ...)
  ├── HashTable(ObjId) ──────►
  ├── Lambda(ObjId)    ──────►
  ├── Macro(ObjId)     ──────►
  ├── ByteCode(ObjId)  ──────►
  ├── Marker(ObjId)    ──────►
  ├── Overlay(ObjId)   ──────►
  ├── Buffer(BufferId)
  ├── Window(u64)
  ├── Frame(u64)
  └── Timer(u64)
```

### New system (`TaggedValue` tagged pointer)

```
TaggedValue (usize, 8 bytes on 64-bit)
  Low 3 bits = tag
  ┌────────────────────────────────────────────────────────────────┐
  │ Tag  Type        Payload                   Fast check         │
  │ 000  Symbol      sym_index << 3            (v & 7) == 0       │
  │ x01  Fixnum      integer << 2              (v & 3) == 1       │
  │ 010  Cons        pointer | 2               (v & 7) == 2       │
  │ 011  Veclike     pointer | 3               (v & 7) == 3       │
  │ 100  String      pointer | 4               (v & 7) == 4       │
  │ 110  Float       pointer | 6               (v & 7) == 6       │
  │ 111  Immediate   sub-tag in bits 3-7       (v & 7) == 7       │
  │      ├── 00000  Char     (21-bit codepoint in bits 8+)        │
  │      ├── 00001  Keyword  (SymId in bits 8+)                   │
  │      └── 00010  Subr     (SymId in bits 8+)                   │
  └────────────────────────────────────────────────────────────────┘

  Veclike sub-types (VecLikeType in VecLikeHeader):
    Vector, HashTable, Lambda, Macro, ByteCode,
    Record, Overlay, Marker, Buffer, Window, Frame, Timer

  Heap objects:
    ConsCell      — 16 bytes, no header, block-allocated
    StringObj     — GcHeader + LispString
    FloatObj      — GcHeader + f64
    VectorObj     — VecLikeHeader + Vec<TaggedValue>
    HashTableObj  — VecLikeHeader + LispHashTable
    LambdaObj     — VecLikeHeader + LambdaData
    MacroObj      — VecLikeHeader + LambdaData
    ByteCodeObj   — VecLikeHeader + ByteCodeFunction
    RecordObj     — VecLikeHeader + Vec<TaggedValue>
    OverlayObj    — VecLikeHeader + OverlayData
    MarkerObj     — VecLikeHeader + MarkerData
    BufferObj     — VecLikeHeader + BufferId
    WindowObj     — VecLikeHeader + u64
    FrameObj      — VecLikeHeader + u64
    TimerObj      — VecLikeHeader + u64

  Thread-local access:
    with_tagged_heap(|h| ...)    // replaces with_heap / with_heap_mut
```

Key differences:
- **8 bytes** per value instead of 16+ bytes (enum discriminant + payload).
- Heap types accessed via direct pointer dereference -- no `ObjId` lookup.
- Floats are heap-allocated (no inline `f64`); fixnums cover 62-bit range.
- `Buffer`, `Window`, `Frame`, `Timer` become veclike sub-types with heap
  allocation, unifying all heap types under the same GC model.
- No generation counter. No stale-handle panics. Dangling pointer = UB
  (prevented by correct GC rooting).

---

## 2. Value Type Rename

The type alias strategy: during migration, `Value` can be aliased to
`TaggedValue` so most call sites don't need renaming.

```rust
// In value.rs (after migration):
pub type Value = TaggedValue;
```

Files that reference `Value` directly generally need no change beyond ensuring
the import resolves to `TaggedValue`.

---

## 3. Constructor Conversions

### 3.1 Nil

```rust
// BEFORE
Value::Nil
let x = Value::Nil;

// AFTER
Value::NIL                    // associated constant, not a variant
let x = Value::NIL;
```

**Rule:** Find-and-replace `Value::Nil` with `Value::NIL`.
**Watch out for** pattern match arms -- those change differently (see Section 4).

### 3.2 True / t

```rust
// BEFORE
Value::True
Value::t()

// AFTER
Value::T                      // associated constant
Value::T                      // .t() method still exists as compat alias
```

**Rule:** Replace `Value::True` with `Value::T`.
`Value::t()` still works but is just `Self::T`.

### 3.3 Int (Fixnum)

```rust
// BEFORE
Value::Int(42)
Value::Int(n)               // constructor from variable

// AFTER
Value::fixnum(42)
Value::fixnum(n)
```

**Rule:** Replace `Value::Int(expr)` constructors with `Value::fixnum(expr)`.
Note: old `Value::Int` was `i64`. New `fixnum()` is also `i64` but with 62-bit
range. Values outside `[MOST_NEGATIVE_FIXNUM, MOST_POSITIVE_FIXNUM]` will
silently truncate via bit shifting -- add range checks if needed.

### 3.4 Float

```rust
// BEFORE
Value::Float(3.14, next_float_id())
// Often constructed as:
let f = Value::Float(val, next_float_id());

// AFTER
with_tagged_heap(|h| h.alloc_float(3.14))
// Or via a convenience method (to be added):
Value::make_float(3.14)
```

**Rule:** Floats become heap-allocated. Every `Value::Float(f, id)` constructor
becomes a heap allocation. The `next_float_id()` call is eliminated entirely.

**Key change:** Float identity is now pointer-based (like GNU Emacs), not ID-based.
Two `alloc_float(3.14)` calls produce *different* float objects (different
pointers), matching GNU `eq` semantics.

### 3.5 Symbol

```rust
// BEFORE
Value::Symbol(sym_id)
Value::symbol("defun")        // constructor that interns

// AFTER
Value::symbol(sym_id)         // from SymId (method, not variant)
Value::symbol_by_name("defun") // constructor that interns
```

**Rule:** `Value::Symbol(id)` (variant construction) becomes `Value::symbol(id)`
(method call). The `Value::symbol(string)` constructor is renamed to
`Value::symbol_by_name(string)` to avoid ambiguity.

**Watch out:** The old `Value::symbol("name")` accepted `impl AsRef<str>` and
also canonicalized `"nil"` -> `Value::Nil`, `"t"` -> `Value::True`, and
`:keyword` -> `Value::Keyword`. The new `Value::symbol(SymId)` does no such
canonicalization. Use `Value::symbol_by_name()` if you need the old behavior.

### 3.6 Keyword

```rust
// BEFORE
Value::Keyword(sym_id)
Value::keyword("test")

// AFTER
Value::keyword(sym_id)         // from SymId
Value::keyword_by_name("test") // from string
```

**Rule:** Same pattern as Symbol.

### 3.7 Char

```rust
// BEFORE
Value::Char('A')
Value::Char(c)

// AFTER
Value::char('A')
Value::char(c)
```

**Rule:** `Value::Char(expr)` -> `Value::char(expr)`.

### 3.8 Subr

```rust
// BEFORE
Value::Subr(sym_id)

// AFTER
Value::subr(sym_id)
```

**Rule:** `Value::Subr(id)` -> `Value::subr(id)`.

### 3.9 Str (String)

```rust
// BEFORE
Value::Str(obj_id)              // direct variant construction (rare)
Value::string("hello")          // constructor (common)
Value::multibyte_string("hi")
Value::unibyte_string("hi")

// AFTER
// Value::string("hello")       -- will call with_tagged_heap internally
// The low-level variant construction is eliminated entirely.
// String values come only from heap allocation:
with_tagged_heap(|h| h.alloc_string(LispString::new(s, multibyte)))
```

The high-level constructors (`Value::string()`, `Value::multibyte_string()`,
etc.) will be reimplemented as methods on `TaggedValue` that call
`with_tagged_heap(|h| h.alloc_string(...))` internally.

### 3.10 Cons

```rust
// BEFORE
Value::Cons(obj_id)            // direct variant construction (rare)
Value::cons(car, cdr)          // constructor (very common)
Value::list(vec![a, b, c])

// AFTER
with_tagged_heap(|h| h.alloc_cons(car, cdr))
// The Value::cons() convenience wrapper will be reimplemented:
// pub fn cons(car: Value, cdr: Value) -> Value {
//     with_tagged_heap(|h| h.alloc_cons(car, cdr))
// }
// Value::list() continues to work (calls cons() in a fold).
```

### 3.11 Vector, Record

```rust
// BEFORE
Value::Vector(obj_id)
Value::Record(obj_id)
Value::vector(vec![a, b])

// AFTER
with_tagged_heap(|h| h.alloc_vector(items))    // returns TaggedValue
with_tagged_heap(|h| h.alloc_record(items))
```

Both Vector and Record become veclike sub-types. Their `TaggedValue` has the
same `011` tag; the difference is `VecLikeType::Vector` vs `VecLikeType::Record`
in the header.

### 3.12 HashTable, Lambda, Macro, ByteCode

```rust
// BEFORE
Value::HashTable(obj_id)
Value::Lambda(obj_id)
Value::Macro(obj_id)
Value::ByteCode(obj_id)

// AFTER -- all become veclike allocations
with_tagged_heap(|h| h.alloc_hash_table(ht))
with_tagged_heap(|h| h.alloc_lambda(data))
with_tagged_heap(|h| h.alloc_macro(data))
with_tagged_heap(|h| h.alloc_bytecode(data))
```

High-level convenience methods on `TaggedValue` (e.g., `Value::make_lambda()`)
will wrap these.

### 3.13 Buffer, Window, Frame, Timer

```rust
// BEFORE
Value::Buffer(buffer_id)
Value::Window(id)
Value::Frame(id)
Value::Timer(id)

// AFTER -- become veclike sub-types
with_tagged_heap(|h| h.alloc_buffer(buffer_id))
with_tagged_heap(|h| h.alloc_window(id))
with_tagged_heap(|h| h.alloc_frame(id))
with_tagged_heap(|h| h.alloc_timer(id))
```

These were previously inline in the Value enum (no heap allocation). Now they
are heap-allocated veclike objects. This is a semantic change -- each creation
allocates. Callers that previously constructed these cheaply may need caching.

---

## 4. Pattern Matching Conversions

This is the largest category of changes. The old `Value` enum allowed direct
`match` with variant destructuring. The new `TaggedValue` is an opaque `usize`
-- you must use accessor methods or `value.kind()`.

### 4.1 Simple single-variant match

```rust
// BEFORE
match value {
    Value::Cons(id) => {
        let car = with_heap(|h| h.cons_car(id));
        // ...
    }
    _ => {}
}

// AFTER (preferred: use predicate + accessor)
if value.is_cons() {
    let car = value.cons_car();
    // ...
}
```

### 4.2 Extracting data from a variant

```rust
// BEFORE
match value {
    Value::Int(n) => do_something(n),
    _ => {}
}

// AFTER
if let Some(n) = value.as_fixnum() {
    do_something(n);
}
```

```rust
// BEFORE
match value {
    Value::Symbol(id) => use_sym(id),
    _ => {}
}

// AFTER
if let Some(id) = value.as_symbol_id() {
    use_sym(id);
}
```

```rust
// BEFORE
match value {
    Value::Float(f, _id) => use_float(f),
    _ => {}
}

// AFTER
if let Some(f) = value.as_float() {
    use_float(f);
}
```

```rust
// BEFORE
match value {
    Value::Char(c) => use_char(c),
    _ => {}
}

// AFTER
if let Some(c) = value.as_char() {
    use_char(c);
}
```

```rust
// BEFORE
match value {
    Value::Keyword(id) => use_kw(id),
    _ => {}
}

// AFTER
if let Some(id) = value.as_keyword_id() {
    use_kw(id);
}
```

```rust
// BEFORE
match value {
    Value::Subr(id) => use_subr(id),
    _ => {}
}

// AFTER
if let Some(id) = value.as_subr_id() {
    use_subr(id);
}
```

### 4.3 Multi-arm match -> `value.kind()`

For exhaustive or multi-arm matches, use `value.kind()`:

```rust
// BEFORE
match value {
    Value::Nil => handle_nil(),
    Value::True => handle_t(),
    Value::Int(n) => handle_int(n),
    Value::Float(f, _) => handle_float(f),
    Value::Symbol(id) => handle_sym(id),
    Value::Keyword(id) => handle_kw(id),
    Value::Char(c) => handle_char(c),
    Value::Subr(id) => handle_subr(id),
    Value::Cons(_) => handle_cons(value),
    Value::Str(_) => handle_str(value),
    Value::Vector(_) => handle_vec(value),
    Value::Record(_) => handle_rec(value),
    Value::HashTable(_) => handle_ht(value),
    Value::Lambda(_) => handle_lambda(value),
    Value::Macro(_) => handle_macro(value),
    Value::ByteCode(_) => handle_bc(value),
    Value::Marker(_) => handle_marker(value),
    Value::Overlay(_) => handle_overlay(value),
    Value::Buffer(_) => handle_buffer(value),
    Value::Window(_) => handle_window(value),
    Value::Frame(_) => handle_frame(value),
    Value::Timer(_) => handle_timer(value),
}

// AFTER
match value.kind() {
    ValueKind::Nil => handle_nil(),
    ValueKind::T => handle_t(),
    ValueKind::Fixnum(n) => handle_int(n),
    ValueKind::Float => handle_float(value.xfloat()),
    ValueKind::Symbol(id) => handle_sym(id),
    ValueKind::Keyword(id) => handle_kw(id),
    ValueKind::Char(c) => handle_char(c),
    ValueKind::Subr(id) => handle_subr(id),
    ValueKind::Cons => handle_cons(value),
    ValueKind::String => handle_str(value),
    ValueKind::Veclike(VecLikeType::Vector) => handle_vec(value),
    ValueKind::Veclike(VecLikeType::Record) => handle_rec(value),
    ValueKind::Veclike(VecLikeType::HashTable) => handle_ht(value),
    ValueKind::Veclike(VecLikeType::Lambda) => handle_lambda(value),
    ValueKind::Veclike(VecLikeType::Macro) => handle_macro(value),
    ValueKind::Veclike(VecLikeType::ByteCode) => handle_bc(value),
    ValueKind::Veclike(VecLikeType::Marker) => handle_marker(value),
    ValueKind::Veclike(VecLikeType::Overlay) => handle_overlay(value),
    ValueKind::Veclike(VecLikeType::Buffer) => handle_buffer(value),
    ValueKind::Veclike(VecLikeType::Window) => handle_window(value),
    ValueKind::Veclike(VecLikeType::Frame) => handle_frame(value),
    ValueKind::Veclike(VecLikeType::Timer) => handle_timer(value),
    ValueKind::Unknown => panic!("unknown value kind"),
}
```

**Key points:**
- `ValueKind::Fixnum(n)` carries the i64 value inline.
- `ValueKind::Float` does NOT carry the f64 -- use `value.xfloat()` separately.
- `ValueKind::Cons` and `ValueKind::String` carry no data -- use accessors.
- `ValueKind::Veclike(sub_type)` needs a nested match for the sub-type.
- `ValueKind::Unknown` should be unreachable in well-formed code.

### 4.4 Partial match (common 2-3 arm patterns)

```rust
// BEFORE
match value {
    Value::Nil => return default,
    Value::Cons(id) => {
        let pair = read_cons(id);
        process(pair.car, pair.cdr)
    }
    other => error(other),
}

// AFTER
if value.is_nil() {
    return default;
} else if value.is_cons() {
    let car = value.cons_car();
    let cdr = value.cons_cdr();
    process(car, cdr)
} else {
    error(value)
}
```

Or using `kind()` if you prefer match syntax:

```rust
match value.kind() {
    ValueKind::Nil => return default,
    ValueKind::Cons => {
        let car = value.cons_car();
        let cdr = value.cons_cdr();
        process(car, cdr)
    }
    _ => error(value),
}
```

### 4.5 `if let` patterns

```rust
// BEFORE
if let Value::Int(n) = value { ... }
if let Value::Symbol(id) = value { ... }
if let Value::Cons(id) = value { ... }
if let Value::Str(id) = value { ... }
if let Value::Float(f, _) = value { ... }
if let Value::Char(c) = value { ... }
if let Value::Keyword(id) = value { ... }
if let Value::Subr(id) = value { ... }

// AFTER
if let Some(n) = value.as_fixnum() { ... }
if let Some(id) = value.as_symbol_id() { ... }
if value.is_cons() { ... }                       // no id to extract
if value.is_string() { ... }                     // no id to extract
if let Some(f) = value.as_float() { ... }
if let Some(c) = value.as_char() { ... }
if let Some(id) = value.as_keyword_id() { ... }
if let Some(id) = value.as_subr_id() { ... }
```

### 4.6 `if let` on Cons with immediate destructure

```rust
// BEFORE
if let Value::Cons(id) = value {
    let car = with_heap(|h| h.cons_car(id));
    let cdr = with_heap(|h| h.cons_cdr(id));
    // ...
}

// AFTER
if value.is_cons() {
    let car = value.cons_car();
    let cdr = value.cons_cdr();
    // ...
}
```

### 4.7 Nested `if let` on Cons

```rust
// BEFORE
if let Value::Cons(outer) = value {
    let pair = read_cons(outer);
    if let Value::Cons(inner) = pair.car {
        let inner_pair = read_cons(inner);
        // use inner_pair.car, inner_pair.cdr
    }
}

// AFTER
if value.is_cons() {
    let car = value.cons_car();
    if car.is_cons() {
        let inner_car = car.cons_car();
        let inner_cdr = car.cons_cdr();
        // use inner_car, inner_cdr
    }
}
```

---

## 5. Accessor Conversions

### 5.1 Cons car/cdr

```rust
// BEFORE (heap-level, with ObjId)
with_heap(|h| h.cons_car(id))
with_heap(|h| h.cons_cdr(id))
with_heap_mut(|h| h.set_car(id, val))
with_heap_mut(|h| h.set_cdr(id, val))

// BEFORE (Value-level methods)
value.cons_car()
value.cons_cdr()
value.set_car(val)
value.set_cdr(val)

// AFTER (direct pointer dereference -- same method names)
value.cons_car()              // direct, no heap closure
value.cons_cdr()
value.set_car(val)
value.set_cdr(val)
```

The method signatures are the same, but the implementation changes from
`with_heap(|h| h.cons_car(*id))` to `unsafe { (*self.xcons_ptr()).car }`.
Most call sites need no change if they were already using the Value-level
methods.

Call sites using the `with_heap` pattern directly with an `ObjId` must be
refactored to use Value-level accessors.

### 5.2 read_cons() / ConsSnapshot

```rust
// BEFORE
let pair = read_cons(id);
use_value(pair.car);
use_value(pair.cdr);

// AFTER (read_cons and ConsSnapshot are eliminated)
let car = value.cons_car();
let cdr = value.cons_cdr();
use_value(car);
use_value(cdr);
```

If the code had `let pair = read_cons(id)` and then used `pair.car` /
`pair.cdr` in multiple places, just call `cons_car()` / `cons_cdr()` directly
on the tagged value.

**Warning:** If code was using `read_cons()` to snapshot both car and cdr
atomically before mutation, the new code must also read both before mutating:

```rust
// BEFORE
let pair = read_cons(id);  // snapshot
set_car(id, pair.cdr);     // swap car and cdr
set_cdr(id, pair.car);

// AFTER
let car = value.cons_car();
let cdr = value.cons_cdr();
value.set_car(cdr);
value.set_cdr(car);
```

### 5.3 Strings

```rust
// BEFORE
with_heap(|h| h.get_string(id))              // -> &str
with_heap(|h| h.get_lisp_string(id))         // -> &LispString
with_heap_mut(|h| h.get_string_mut(id))      // -> &mut String
with_heap(|h| h.string_is_multibyte(id))     // -> bool
value.as_str()                                // -> Option<&str>

// AFTER
value.as_str()                                // -> Option<&'static str>
// For LispString access:
unsafe { (*value.as_string_ptr().unwrap()).data.as_str() }
// For mutable string access:
unsafe { (*value.as_string_ptr().unwrap() as *mut StringObj).data.make_mut() }
// For multibyte check:
unsafe { (*value.as_string_ptr().unwrap()).data.multibyte }
```

`value.as_str()` continues to work with the same signature but the
implementation changes from ObjId lookup to direct pointer dereference.

For LispString-level access (multibyte flag, slicing, mutation), you dereference
the `StringObj` pointer directly. Consider adding convenience methods:

```rust
impl TaggedValue {
    pub fn as_lisp_string(&self) -> Option<&LispString> {
        self.as_string_ptr().map(|p| unsafe { &(*p).data })
    }
    pub fn string_is_multibyte(&self) -> bool {
        self.as_lisp_string().map_or(false, |s| s.multibyte)
    }
}
```

### 5.4 Vectors

```rust
// BEFORE
with_heap(|h| h.get_vector(id))             // -> &Vec<Value>
with_heap(|h| h.vector_ref(id, idx))        // -> Value
with_heap(|h| h.vector_len(id))             // -> usize
with_heap_mut(|h| h.vector_set(id, idx, v)) // -> ()
with_heap_mut(|h| h.get_vector_mut(id))     // -> &mut Vec<Value>

// AFTER -- cast veclike pointer to VectorObj
unsafe {
    let obj = value.as_veclike_ptr().unwrap() as *const VectorObj;
    let data: &Vec<TaggedValue> = &(*obj).data;
    // vector_ref:
    data[idx]
    // vector_len:
    data.len()
    // vector_set (via *mut):
    (*(obj as *mut VectorObj)).data[idx] = v;
}
```

Add convenience methods on `TaggedValue`:

```rust
impl TaggedValue {
    pub fn vector_ref(self, idx: usize) -> Self { ... }
    pub fn vector_set(self, idx: usize, val: Self) { ... }
    pub fn vector_len(self) -> usize { ... }
}
```

### 5.5 Hash tables

```rust
// BEFORE
with_heap(|h| h.get_hash_table(id))           // -> &LispHashTable
with_heap_mut(|h| h.get_hash_table_mut(id))   // -> &mut LispHashTable

// AFTER
unsafe {
    let obj = value.as_veclike_ptr().unwrap() as *const HashTableObj;
    let ht: &LispHashTable = &(*obj).table;
    // or mutable:
    let ht: &mut LispHashTable = &mut (*(obj as *mut HashTableObj)).table;
}
```

### 5.6 Lambda / Macro

```rust
// BEFORE
with_heap(|h| h.get_lambda(id))               // -> &LambdaData
with_heap(|h| h.get_macro_data(id))            // -> &LambdaData
value.get_lambda_data()                        // -> Option<&LambdaData>

// AFTER
unsafe {
    let obj = value.as_veclike_ptr().unwrap() as *const LambdaObj;
    let data: &LambdaData = &(*obj).data;
}
// Or for macros:
unsafe {
    let obj = value.as_veclike_ptr().unwrap() as *const MacroObj;
    let data: &LambdaData = &(*obj).data;
}
```

### 5.7 ByteCode

```rust
// BEFORE
with_heap(|h| h.get_bytecode(id))             // -> &ByteCodeFunction
value.get_bytecode_data()                      // -> Option<&ByteCodeFunction>

// AFTER
unsafe {
    let obj = value.as_veclike_ptr().unwrap() as *const ByteCodeObj;
    let data: &ByteCodeFunction = &(*obj).data;
}
```

### 5.8 Marker / Overlay

```rust
// BEFORE
with_heap(|h| h.get_marker(id))               // -> &MarkerData
with_heap(|h| h.get_overlay(id))               // -> &OverlayData

// AFTER
unsafe {
    let obj = value.as_veclike_ptr().unwrap() as *const MarkerObj;
    &(*obj).data
}
// Similarly for overlay:
unsafe {
    let obj = value.as_veclike_ptr().unwrap() as *const OverlayObj;
    &(*obj).data
}
```

### 5.9 `value.str_id()` -- eliminated

```rust
// BEFORE
if let Some(id) = value.str_id() {
    set_string_text_properties(id, runs);
}

// AFTER
// String identity is now pointer-based. Text property storage
// must be keyed by pointer address or by the value itself:
if value.is_string() {
    let ptr = value.as_string_ptr().unwrap() as usize;
    set_string_text_properties_by_ptr(ptr, runs);
}
```

The `str_id() -> Option<ObjId>` method is eliminated because there is no ObjId.
Any code that used ObjId as a map key for string properties must switch to using
the raw pointer value (obtainable via `value.bits()` or `as_string_ptr()`).

---

## 6. Type Check Conversions

### 6.1 `matches!()` patterns

```rust
// BEFORE                                      // AFTER
matches!(value, Value::Nil)                     value.is_nil()
matches!(value, Value::True)                    value.is_t()
matches!(value, Value::Cons(_))                 value.is_cons()
matches!(value, Value::Str(_))                  value.is_string()
matches!(value, Value::Int(_))                  value.is_fixnum()
matches!(value, Value::Float(_, _))             value.is_float()
matches!(value, Value::Symbol(_))               value.is_symbol()
matches!(value, Value::Keyword(_))              value.is_keyword()
matches!(value, Value::Char(_))                 value.is_char()
matches!(value, Value::Subr(_))                 value.is_subr()
matches!(value, Value::Vector(_))               value.is_vector()
matches!(value, Value::Record(_))               value.is_record()
matches!(value, Value::HashTable(_))            value.is_hash_table()
matches!(value, Value::Lambda(_))               value.veclike_type() == Some(VecLikeType::Lambda)
matches!(value, Value::Macro(_))                value.veclike_type() == Some(VecLikeType::Macro)
matches!(value, Value::ByteCode(_))             value.veclike_type() == Some(VecLikeType::ByteCode)
matches!(value, Value::Marker(_))               value.veclike_type() == Some(VecLikeType::Marker)
matches!(value, Value::Overlay(_))              value.veclike_type() == Some(VecLikeType::Overlay)
matches!(value, Value::Buffer(_))               value.veclike_type() == Some(VecLikeType::Buffer)
matches!(value, Value::Window(_))               value.veclike_type() == Some(VecLikeType::Window)
matches!(value, Value::Frame(_))                value.veclike_type() == Some(VecLikeType::Frame)
matches!(value, Value::Timer(_))                value.veclike_type() == Some(VecLikeType::Timer)
matches!(value, Value::Nil | Value::Cons(_))    value.is_list()
```

For the veclike sub-types used frequently, add convenience predicates:

```rust
impl TaggedValue {
    pub fn is_lambda(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Lambda)
    }
    pub fn is_macro(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Macro)
    }
    pub fn is_bytecode(self) -> bool {
        self.veclike_type() == Some(VecLikeType::ByteCode)
    }
    pub fn is_marker(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Marker)
    }
    pub fn is_overlay(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Overlay)
    }
    pub fn is_buffer(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Buffer)
    }
    pub fn is_window(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Window)
    }
    pub fn is_frame(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Frame)
    }
    pub fn is_timer(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Timer)
    }
}
```

### 6.2 Compound type checks

```rust
// BEFORE
matches!(value, Value::Nil | Value::True | Value::Symbol(_) | Value::Keyword(_))

// AFTER (value.is_symbol() already covers Nil and True because they use TAG_SYMBOL)
// BUT: Keyword uses TAG_IMMEDIATE, not TAG_SYMBOL!
// So the correct conversion depends on intent:

// "is this value a symbol in the Elisp sense?" (nil, t, interned symbols, keywords)
value.is_symbol() || value.is_keyword()    // nil/t are symbols; keywords are separate

// "is this a function?"
// BEFORE: matches!(value, Value::Lambda(_) | Value::Subr(_) | Value::ByteCode(_))
// AFTER:
value.is_function()   // covers subr + lambda + bytecode
```

**Important tag difference:** In the old system, `Value::Nil` and `Value::True`
were separate enum variants. In the new system, nil is `Symbol(0)` and t is
`Symbol(1)` -- both have tag `000`. So `value.is_symbol()` returns `true` for
nil and t. If you need to exclude nil/t, check explicitly:

```rust
// "is this a non-nil, non-t symbol?"
value.is_symbol() && !value.is_nil() && !value.is_t()
```

---

## 7. Heap Allocation Changes

### 7.1 Old pattern: `with_heap_mut`

```rust
// BEFORE
let id = with_heap_mut(|heap| heap.alloc_cons(car, cdr));
let value = Value::Cons(id);

// AFTER
let value = with_tagged_heap(|h| h.alloc_cons(car, cdr));
// alloc_cons returns TaggedValue directly, no separate id+wrapping step
```

### 7.2 Convenience constructors

The old `Value::cons()`, `Value::string()`, `Value::vector()`, etc. called
`with_heap_mut` internally. The new versions call `with_tagged_heap` internally.
Call sites using these convenience methods need no change beyond the type rename.

```rust
// These continue to work (implementation changes, API doesn't):
Value::cons(car, cdr)
Value::string("hello")
Value::list(vec![a, b, c])
Value::vector(vec![a, b])
Value::make_lambda(data)
Value::make_bytecode(bc)
Value::hash_table(HashTableTest::Eq)
```

### 7.3 Thread-local heap

```rust
// BEFORE
set_current_heap(&mut heap);    // sets CURRENT_HEAP thread-local
with_heap(|h| ...);             // reads via CURRENT_HEAP
with_heap_mut(|h| ...);         // writes via CURRENT_HEAP
clear_current_heap();

// AFTER
set_tagged_heap(&mut heap);     // sets TAGGED_HEAP thread-local
with_tagged_heap(|h| ...);      // unified read/write (always &mut)
```

`with_heap` and `with_heap_mut` are eliminated. There is only
`with_tagged_heap` which always gives `&mut TaggedHeap` (needed because
allocation mutates the heap and may trigger GC).

---

## 8. Exhaustive Match via ValueKind

When you need to handle all possible value types, use `value.kind()`:

```rust
match value.kind() {
    // === Immediates (no heap allocation) ===
    ValueKind::Nil => { /* nil */ }
    ValueKind::T => { /* t */ }
    ValueKind::Fixnum(n) => { /* integer n */ }
    ValueKind::Symbol(id) => { /* interned symbol */ }
    ValueKind::Char(c) => { /* unicode char */ }
    ValueKind::Keyword(id) => { /* keyword */ }
    ValueKind::Subr(id) => { /* builtin function */ }

    // === Heap-allocated (direct pointer) ===
    ValueKind::Cons => {
        let car = value.cons_car();
        let cdr = value.cons_cdr();
    }
    ValueKind::String => {
        let s = value.as_str().unwrap();
    }
    ValueKind::Float => {
        let f = value.xfloat();
    }

    // === Veclike sub-types ===
    ValueKind::Veclike(vt) => match vt {
        VecLikeType::Vector => { /* vector */ }
        VecLikeType::Record => { /* record */ }
        VecLikeType::HashTable => { /* hash table */ }
        VecLikeType::Lambda => { /* closure */ }
        VecLikeType::Macro => { /* macro */ }
        VecLikeType::ByteCode => { /* bytecode function */ }
        VecLikeType::Overlay => { /* overlay */ }
        VecLikeType::Marker => { /* marker */ }
        VecLikeType::Buffer => { /* buffer ref */ }
        VecLikeType::Window => { /* window ref */ }
        VecLikeType::Frame => { /* frame ref */ }
        VecLikeType::Timer => { /* timer ref */ }
    },

    ValueKind::Unknown => unreachable!("corrupt tagged value"),
}
```

**Performance note:** `value.kind()` decodes the tag into an enum. For hot
paths, prefer the direct predicate methods (`is_cons()`, `is_fixnum()`, etc.)
which compile to a single AND + CMP instruction. Use `kind()` only when you
actually need multi-way dispatch.

---

## 9. Eliminated Concepts

The following types/functions/patterns are **completely removed** in the tagged
pointer system:

### 9.1 `ObjId`

```rust
// ELIMINATED
pub struct ObjId {
    index: u32,
    generation: u32,
}
```

Every use of `ObjId` is replaced by either a `TaggedValue` (which embeds the
heap pointer in its bits) or a raw pointer to a heap object.

### 9.2 `with_heap` / `with_heap_mut`

```rust
// ELIMINATED
pub fn with_heap<R>(f: impl FnOnce(&LispHeap) -> R) -> R { ... }
pub fn with_heap_mut<R>(f: impl FnOnce(&mut LispHeap) -> R) -> R { ... }
```

Replaced by `with_tagged_heap(|h| ...)` for allocation operations, and by
direct pointer dereference for accessors (cons_car, as_str, etc.).

### 9.3 `ConsSnapshot` / `read_cons()`

```rust
// ELIMINATED
pub struct ConsSnapshot { pub car: Value, pub cdr: Value }
pub fn read_cons(id: ObjId) -> ConsSnapshot { ... }
```

Use `value.cons_car()` and `value.cons_cdr()` directly.

**Occurrence count:** ~319 uses of `read_cons()` across 66 files.

### 9.4 `HeapObject` enum

```rust
// ELIMINATED
pub enum HeapObject {
    Cons { car: Value, cdr: Value },
    Vector(Vec<Value>),
    HashTable(LispHashTable),
    Str(LispString),
    Lambda(LambdaData),
    Macro(LambdaData),
    ByteCode(ByteCodeFunction),
    Overlay(OverlayData),
    Marker(MarkerData),
    Free,
}
```

Replaced by concrete struct types (`ConsCell`, `StringObj`, `FloatObj`,
`VectorObj`, `HashTableObj`, `LambdaObj`, `MacroObj`, `ByteCodeObj`,
`RecordObj`, `OverlayObj`, `MarkerObj`, `BufferObj`, `WindowObj`, `FrameObj`,
`TimerObj`) that live at stable memory addresses.

### 9.5 `LispHeap` struct

```rust
// ELIMINATED
pub struct LispHeap {
    objects: Vec<HeapObject>,
    generations: Vec<u32>,
    marks: Vec<bool>,
    free_list: Vec<u32>,
    ...
}
```

Replaced by `TaggedHeap` which uses:
- Block allocator for cons cells (external mark bitmap).
- Intrusive linked list (`GcHeader.next`) for all other heap objects.
- System allocator (`Box`) for individual non-cons objects.

### 9.6 Generation checking

```rust
// ELIMINATED
fn check(&self, id: ObjId) {
    // Panic if generation mismatch (stale handle)
}
```

No generation counters exist. Correctness relies on the GC not collecting
reachable objects. Use-after-free is undefined behavior, not a checked panic.
This is the same model as GNU Emacs.

### 9.7 `next_float_id()` / float identity

```rust
// ELIMINATED
pub fn next_float_id() -> u32 { ... }
Value::Float(f64, u32)   // the u32 was the alloc ID
```

Float identity is now pointer-based. Each `alloc_float()` returns a unique
pointer. Two floats with the same `f64` value are `eq` only if they are the
same allocation (same pointer).

### 9.8 `value_objid()` / `push_value_ids()`

```rust
// ELIMINATED (in LispHeap)
fn value_objid(val: &Value) -> Option<ObjId> { ... }
fn push_value_ids(val: &Value, worklist: &mut Vec<ObjId>) { ... }
```

The GC now works directly with `TaggedValue`. Tracing checks
`value.is_heap_object()` and follows the pointer directly.

---

## 10. GC and Rooting Changes

### 10.1 Mark phase

```rust
// BEFORE (ObjId-based)
fn push_value_ids(val: &Value, worklist: &mut Vec<ObjId>) {
    match val {
        Value::Cons(id) | Value::Vector(id) | ... => worklist.push(*id),
        _ => {}
    }
}

// AFTER (tagged pointer)
fn push_if_heap(val: TaggedValue, queue: &mut Vec<TaggedValue>) {
    if val.is_heap_object() {
        queue.push(val);
    }
}
```

### 10.2 Conservative stack scanning

The old system scanned the stack for `(u32 index, u32 generation)` pairs.
The new system scans for `usize` values with valid heap-pointer tags
(`010`, `011`, `100`, `110`) that point into known heap regions.

### 10.3 temp_roots

```rust
// BEFORE
temp_roots: Vec<Value>,          // in eval.rs Context
temp_roots.push(value);
temp_roots.truncate(saved_len);

// AFTER -- same concept, different inner type
temp_roots: Vec<TaggedValue>,
temp_roots.push(value);
temp_roots.truncate(saved_len);
```

The `temp_roots` mechanism continues to exist for explicit GC rooting. The
only change is the element type.

### 10.4 Write barriers

```rust
// BEFORE
fn write_barrier(&mut self, id: ObjId) {
    if self.gc_phase == GcPhase::Marking && self.marks[id.index] {
        self.marks[id.index] = false;
        self.gray_queue.push(id);
    }
}

// AFTER
// For cons cells: mark the cons cell in the bitmap.
// For non-cons: use the GcHeader.marked flag.
// Write barrier behavior is the same (push to gray on mutation during mark).
```

---

## 11. File-by-File Conversion Order

Recommended order (dependencies flow downward):

### Phase 1: Core types (must be done atomically)

1. **`tagged/value.rs`** -- Add remaining convenience methods (constructor
   wrappers, accessor methods for veclike sub-types).

2. **`tagged/header.rs`** -- Complete, no changes needed.

3. **`tagged/gc.rs`** -- Complete Phase 2 TODOs:
   - Typed deallocation in `free_gc_object()`.
   - Tracing for HashTable, Lambda, Macro, ByteCode, Overlay children.

4. **`gc/types.rs`** -- Keep `LispString`, `OverlayData`, `MarkerData`.
   Remove `ObjId`, `HeapObject`.

5. **`gc/heap.rs`** -- Remove `LispHeap` entirely (replaced by `TaggedHeap`).

6. **`gc/mod.rs`** -- Re-export from `tagged/` instead of `gc/`.

7. **`emacs_core/runtime/value/mod.rs`** -- The big one:
   - Replace `pub enum Value { ... }` with `pub type Value = TaggedValue;`
   - Move convenience constructors to `TaggedValue` impl.
   - Remove `with_heap`, `with_heap_mut`, `set_current_heap`, etc.
   - Remove `ConsSnapshot`, `read_cons()`.
   - Update `PartialEq`, `Display`, all accessor methods.
   - Update `eq_value`, `eql_value`, `equal_value`.
   - Update `HashKey` to use pointers instead of ObjId.

### Phase 2: Evaluator

8. **`emacs_core/runtime/eval/mod.rs`** -- (~236 Value constructor matches, ~91 temp_roots)
   - Convert all `Value::Cons(id)` matches.
   - Convert all `read_cons()` calls.
   - Update `with_heap` / `with_heap_mut` calls.
   - Keep `temp_roots` (just change element type).

### Phase 3: Builtins (can be parallelized)

These files have the highest match counts and can be converted independently:

9. **`emacs_core/lisp/native/builtins/symbols.rs`** (~131 matches)
10. **`emacs_core/lisp/native/builtins/cons_list.rs`** (~93 matches)
11. **`emacs_core/editing/buffer/mod.rs`** (~127 matches)
12. **`emacs_core/lisp/native/builtins/stubs.rs`** (~77 matches)
13. **`emacs_core/lisp/native/builtins/arithmetic.rs`** (~66 matches)
14. **`emacs_core/lisp/native/builtins/collections.rs`** (~49 matches)
15. **`emacs_core/lisp/native/builtins/strings.rs`** (~59 matches)
16. **`emacs_core/lisp/native/builtins/types.rs`** (~61 matches)
17. **`emacs_core/lisp/native/builtins/misc_eval.rs`** (~59 matches)
18. **`emacs_core/lisp/native/builtins/search.rs`** (~43 matches)
19. **`emacs_core/lisp/native/builtins/misc_pure.rs`** (~18 matches)
20. **`emacs_core/lisp/native/builtins/higher_order.rs`** (~22 matches)
21. **`emacs_core/lisp/native/builtins/keymaps.rs`** (~20 matches)
22. **`emacs_core/lisp/native/builtins/hooks.rs`** (~4 matches)
23. **`emacs_core/lisp/native/builtins/mod.rs`** (~35 matches)

### Phase 4: Subsystems

24. **`emacs_core/runtime/bytecode/vm.rs`** (~123 matches) -- critical path
25. **`emacs_core/lisp/print/mod.rs`** (~84 matches)
26. **`emacs_core/lisp/reader/mod.rs`** (~52 matches)
27. **`emacs_core/commands/keymap/mod.rs`** (~87 matches)
28. **`emacs_core/display/xdisp/mod.rs`** (~102 matches)
29. **`emacs_core/text/syntax/mod.rs`** (~89 matches)
30. **`emacs_core/text/chartable/mod.rs`** (~76 matches)
31. **`emacs_core/commands/interactive/mod.rs`** (~64 matches)
32. **`emacs_core/display/font/mod.rs`** (~160 matches)
33. **`emacs_core/system/fileio/mod.rs`** (~64 matches)
34. **`emacs_core/lisp/cl_lib/mod.rs`** (~62 matches)
35. **`emacs_core/runtime/hashtab/mod.rs`** (~66 matches)
36. **`emacs_core/display/display/mod.rs`** (~58 matches)
37. **`emacs_core/system/process/mod.rs`** (~175 matches)
38. **`emacs_core/text/textprop/mod.rs`** (~56 matches)
39. **`emacs_core/lisp/native/fns/mod.rs`** (~35 matches)

### Phase 5: Remaining files

40. All remaining `.rs` files (mostly tests and smaller modules).

### Phase 6: Tests

41. **`emacs_core/lisp/native/builtins/tests/mod.rs`** (~734 matches -- the largest file)
42. All subsystem `tests/` files -- update patterns to match new API.

### Phase 7: pdump

43. **`emacs_core/runtime/pdump/convert.rs`** -- serialize/deserialize TaggedValue
    instead of Value enum + ObjId.
44. **`emacs_core/runtime/pdump/types.rs`** -- update pdump type definitions.

---

## 12. HashKey and Equality

### 12.1 HashKey changes

```rust
// BEFORE
HashKey::Str(ObjId)              // keyed by ObjId
HashKey::ObjId(u32, u32)         // identity by index+generation

// AFTER
HashKey::Str(TaggedValue)        // keyed by tagged pointer (contains string ptr)
HashKey::Ptr(usize)              // identity by raw bits (pointer value)
```

The `ObjId` variant in `HashKey` is replaced by pointer-based identity.

### 12.2 eq_value / eql_value / equal_value

```rust
// BEFORE (match on enum variants)
fn eq_value(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Nil, Value::Nil) => true,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Cons(a), Value::Cons(b)) => a == b,
        ...
    }
}

// AFTER (TaggedValue already derives Eq based on raw bits)
// eq is just: left.bits() == right.bits()
// But: we need special cases for float eql and string/cons equal.
fn eq_value(left: TaggedValue, right: TaggedValue) -> bool {
    left == right  // bitwise comparison -- handles symbols, fixnums, cons identity
}

fn eql_value(left: TaggedValue, right: TaggedValue) -> bool {
    if left == right { return true; }
    // Floats: different pointers but same f64 value
    if left.is_float() && right.is_float() {
        return left.xfloat().to_bits() == right.xfloat().to_bits();
    }
    false
}

fn equal_value(left: TaggedValue, right: TaggedValue, depth: usize) -> bool {
    if left == right { return true; }
    match (left.kind(), right.kind()) {
        (ValueKind::String, ValueKind::String) => {
            left.as_str() == right.as_str()
        }
        (ValueKind::Cons, ValueKind::Cons) => {
            equal_value(left.cons_car(), right.cons_car(), depth + 1)
            && equal_value(left.cons_cdr(), right.cons_cdr(), depth + 1)
        }
        (ValueKind::Float, ValueKind::Float) => {
            left.xfloat().to_bits() == right.xfloat().to_bits()
        }
        // ... vectors, records, etc.
        _ => false,
    }
}
```

**Key insight:** `TaggedValue` derives `PartialEq` and `Eq` via bitwise
comparison of the `usize`. This means:
- Fixnum equality works correctly (same bits = same value).
- Symbol equality works correctly (same SymId = same bits).
- Cons `eq` is pointer identity (correct).
- Float `eq` is pointer identity (correct -- different allocations are not `eq`).
- String `eq` is pointer identity (correct for `eq`; `equal` compares content).

---

## 13. Common Pitfalls

### 13.1 Nil is a symbol, not a special tag

```rust
// WRONG: assuming nil has its own tag
if value.tag() == TAG_SOME_SPECIAL_NIL { ... }

// RIGHT: nil is Symbol(0)
value.is_nil()    // checks value.0 == 0
value.is_symbol() // true for nil!
```

### 13.2 `Value::True` != `Value::T` naming

The old code uses `Value::True`. The new code uses `Value::T`. These are
different names for the same concept. Search-and-replace carefully:
- `Value::True` -> `Value::T`
- But NOT in strings: `"true"` stays `"true"`.
- In match arms: `Value::True =>` becomes `ValueKind::T =>`.

### 13.3 Keyword is an immediate, not a symbol

In the old system, `Value::Keyword(SymId)` and `Value::Symbol(SymId)` were
sibling enum variants. In the new system:
- Symbol uses tag `000` (same as nil/t).
- Keyword uses tag `111` (immediate) with sub-tag `00001`.

So `value.is_symbol()` returns `false` for keywords. If your code checked
`matches!(value, Value::Symbol(_) | Value::Keyword(_))`, you need:

```rust
value.is_symbol() || value.is_keyword()
```

### 13.4 Float allocation changes semantics

```rust
// BEFORE: no heap allocation
let a = Value::Float(3.14, next_float_id());
let b = Value::Float(3.14, next_float_id());
// a != b (different float IDs)

// AFTER: heap allocation
let a = with_tagged_heap(|h| h.alloc_float(3.14));
let b = with_tagged_heap(|h| h.alloc_float(3.14));
// a != b (different pointers) -- same semantics, but now heap-allocated
```

### 13.5 Buffer/Window/Frame/Timer now need heap allocation

These were previously inline in the Value enum (zero-cost construction).
Now they require heap allocation through the veclike system. If there are
hot paths that create these frequently, consider caching.

### 13.6 Unsafe code increases

The old system was safe Rust (checked via generation counters). The new
system uses unsafe pointer dereferences. Every cons/string/float/veclike
accessor is inherently unsafe. Wrap unsafe blocks tightly and document
invariants.

### 13.7 GC during accessor = dangling pointer

```rust
// DANGEROUS
let s = value.as_str().unwrap();   // borrows from heap
do_something_that_triggers_gc();    // GC may free the string object
use(s);                             // dangling pointer!

// SAFE
let s = value.as_str().unwrap().to_owned();  // copy the string data
do_something_that_triggers_gc();
use(&s);                                      // safe, we own the copy
```

This was also a concern in the old system but was masked by generation
counters catching the bug. In the new system it's UB.

### 13.8 Pattern: converting `match` in closures

```rust
// BEFORE
with_heap(|h| match value {
    Value::Cons(id) => Some(h.cons_car(id)),
    _ => None,
})

// AFTER (no closure needed)
if value.is_cons() { Some(value.cons_car()) } else { None }
```

The elimination of `with_heap` closures often simplifies code by removing
a nesting level.

### 13.9 Pattern: multi-value heap access in a single closure

```rust
// BEFORE (single heap borrow for multiple accesses)
with_heap(|h| {
    let a_car = h.cons_car(a_id);
    let b_car = h.cons_car(b_id);
    a_car == b_car
})

// AFTER (no closure needed -- direct access)
let a_car = a.cons_car();
let b_car = b.cons_car();
a_car == b_car
```

### 13.10 The `Value::bool()` method rename

```rust
// BEFORE
Value::bool(condition)    // returns Value::True or Value::Nil

// AFTER
Value::bool_val(condition)  // renamed to avoid conflict with Rust's bool type
// Or keep Value::bool() if the type alias makes it unambiguous
```

---

## Summary of Occurrence Counts

These counts indicate the scale of changes needed per pattern:

| Pattern | Occurrences | Files |
|---------|-------------|-------|
| `Value::Variant(...)` constructors/matches | ~8025 | 185 |
| `with_heap(...)` / `with_heap_mut(...)` | ~605 | 91 |
| `matches!(value, Value::...)` | ~507 | 81 |
| `read_cons(...)` | ~319 | 66 |
| `ObjId` references | ~241 | 32 |
| `if let Value::...` | ~191 | 60 |

Total estimated touch points: **~10,000** across **~185 files**.

---

## Quick Reference Card

| Old | New | Notes |
|-----|-----|-------|
| `Value::Nil` | `Value::NIL` | const |
| `Value::True` | `Value::T` | const |
| `Value::Int(n)` | `Value::fixnum(n)` | method |
| `Value::Float(f, id)` | `heap.alloc_float(f)` | heap-allocated |
| `Value::Symbol(id)` | `Value::symbol(id)` | method |
| `Value::Keyword(id)` | `Value::keyword(id)` | method |
| `Value::Char(c)` | `Value::char(c)` | method |
| `Value::Subr(id)` | `Value::subr(id)` | method |
| `Value::Cons(id)` | `heap.alloc_cons(car, cdr)` | returns TaggedValue |
| `Value::Str(id)` | `heap.alloc_string(ls)` | returns TaggedValue |
| `Value::Vector(id)` | `heap.alloc_vector(items)` | veclike |
| `Value::Record(id)` | `heap.alloc_record(items)` | veclike |
| `Value::HashTable(id)` | `heap.alloc_hash_table(ht)` | veclike |
| `Value::Lambda(id)` | `heap.alloc_lambda(data)` | veclike |
| `Value::Macro(id)` | `heap.alloc_macro(data)` | veclike |
| `Value::ByteCode(id)` | `heap.alloc_bytecode(data)` | veclike |
| `Value::Marker(id)` | `heap.alloc_marker(data)` | veclike |
| `Value::Overlay(id)` | `heap.alloc_overlay(data)` | veclike |
| `Value::Buffer(bid)` | `heap.alloc_buffer(bid)` | veclike (new) |
| `Value::Window(id)` | `heap.alloc_window(id)` | veclike (new) |
| `Value::Frame(id)` | `heap.alloc_frame(id)` | veclike (new) |
| `Value::Timer(id)` | `heap.alloc_timer(id)` | veclike (new) |
| `with_heap(\|h\| ...)` | direct access or `with_tagged_heap` | eliminated |
| `with_heap_mut(\|h\| ...)` | `with_tagged_heap(\|h\| ...)` | unified |
| `read_cons(id)` | `value.cons_car()` / `.cons_cdr()` | eliminated |
| `value.str_id()` | `value.as_string_ptr()` | pointer-based |
| `ObjId` | eliminated | no equivalent |
| `HeapObject` | concrete `*Obj` structs | per-type |
| `LispHeap` | `TaggedHeap` | new impl |
| `check(id)` | n/a (no generation check) | unsafe invariant |
| `ConsSnapshot` | n/a | eliminated |
| `next_float_id()` | n/a | eliminated |
