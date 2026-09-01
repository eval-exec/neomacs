# Static Subr Registry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace heap-allocated SubrObj with a global static subr table using immediate tagged values, fixing pdump stale subr objects.

**Architecture:** Add SubrEntry/SubrTable as a global registry keyed by SymId. Encode subr Values as immediates (sub-tag 1 under tag 111). Migrate all dispatch, creation, and introspection code from SubrObj heap pointers to global table lookups. Delete the old heap SubrObj infrastructure.

**Tech Stack:** Rust (neovm-core crate)

**Spec:** `docs/superpowers/specs/2026-04-16-static-subr-registry-design.md`

---

## File Map

| File | Changes |
|------|---------|
| `neovm-core/src/tagged/header.rs` | Keep SubrFn, SubrDispatchKind. Delete SubrObj. |
| `neovm-core/src/tagged/value.rs` | New immediate encoding. Replace as_subr_ref with as_subr_entry. |
| `neovm-core/src/tagged/gc.rs` | Delete alloc_subr, subr_registry, subr GC tracing/sweep. |
| `neovm-core/src/emacs_core/eval.rs` | New SubrEntry/SubrTable. Rewrite defsubr, dispatch, context methods. New ValueKind::Subr. |
| `neovm-core/src/emacs_core/bytecode/vm.rs` | Update subr checks to use new API. |
| `neovm-core/src/emacs_core/builtins_extra.rs` | Update subrp. |
| `neovm-core/src/emacs_core/subr/info.rs` | Update subr metadata lookups. |
| `neovm-core/src/emacs_core/fns.rs` | Update dispatch_subr calls. |
| `neovm-core/src/emacs_core/dired.rs` | Update dispatch_subr calls. |
| `neovm-core/src/emacs_core/lread.rs` | Update dispatch_subr calls. |
| `neovm-core/src/emacs_core/debug.rs` | Update subr printing. |
| `neovm-core/src/emacs_core/pdump/convert.rs` | Update subr serialization. |
| `neovm-core/src/emacs_core/print.rs` | Update subr printing. |

---

### Task 1: Add SubrEntry struct and global SubrTable

**Files:**
- Modify: `neovm-core/src/tagged/header.rs` (keep SubrFn, SubrDispatchKind; they move to be shared)
- Modify: `neovm-core/src/emacs_core/eval.rs` (add SubrEntry, global table, accessor functions)

- [ ] **Step 1: Define SubrEntry struct**

In `eval.rs`, add near the top (after imports):

```rust
/// Static subr entry — lives in global table, not on heap.
/// Matches GNU Emacs's static Lisp_Subr conceptually.
#[derive(Clone)]
pub(crate) struct SubrEntry {
    pub(crate) function: Option<SubrFn>,
    pub(crate) min_args: u16,
    pub(crate) max_args: Option<u16>,
    pub(crate) dispatch_kind: SubrDispatchKind,
    pub(crate) name_id: NameId,
}
```

- [ ] **Step 2: Add global SubrTable with thread-local storage**

Add thread-local global table and accessor functions:

```rust
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static SUBR_TABLE: RefCell<HashMap<SymId, SubrEntry>> = RefCell::new(HashMap::new());
}

pub(crate) fn register_subr_entry(sym_id: SymId, entry: SubrEntry) {
    SUBR_TABLE.with(|table| {
        table.borrow_mut().insert(sym_id, entry);
    });
}

pub(crate) fn lookup_subr_entry(sym_id: SymId) -> Option<SubrEntry> {
    SUBR_TABLE.with(|table| {
        table.borrow().get(&sym_id).cloned()
    })
}

pub(crate) fn with_subr_entry<R>(sym_id: SymId, f: impl FnOnce(&SubrEntry) -> R) -> Option<R> {
    SUBR_TABLE.with(|table| {
        table.borrow().get(&sym_id).map(f)
    })
}

pub(crate) fn clear_subr_table() {
    SUBR_TABLE.with(|table| table.borrow_mut().clear());
}
```

- [ ] **Step 3: Build and verify**

```bash
cargo build --workspace 2>&1 | tail -10
```

- [ ] **Step 4: Commit**

```bash
git add neovm-core/src/emacs_core/eval.rs neovm-core/src/tagged/header.rs
git commit -m "Add SubrEntry and global SubrTable"
```

---

### Task 2: New immediate Value encoding for subrs

**Files:**
- Modify: `neovm-core/src/tagged/value.rs` (new encoding, new methods)
- Modify: `neovm-core/src/emacs_core/value.rs` (ValueKind::Subr variant, update kind() method)

- [ ] **Step 1: Add immediate subr encoding to tagged/value.rs**

Add constants and methods to `TaggedValue`:

```rust
/// Immediate sub-tag for static subr values.
/// Encoding: [SymId << 8 | 0x0F]
/// Sub-tag 1 (bits 3-7 = 00001) + immediate tag (bits 0-2 = 111)
const SUBR_IMMEDIATE_TAG: usize = 0x0F;

impl TaggedValue {
    /// Create a subr value from a SymId (static, not heap-allocated).
    pub fn subr_from_sym_id(sym_id: SymId) -> Self {
        Self((sym_id.0 as usize) << 8 | SUBR_IMMEDIATE_TAG)
    }

    /// Check if this value is a static subr.
    pub fn is_subr_static(self) -> bool {
        self.0 & 0xFF == SUBR_IMMEDIATE_TAG
    }

    /// Extract the SymId from a static subr value.
    pub fn as_subr_sym_id_static(self) -> Option<SymId> {
        if self.is_subr_static() {
            Some(SymId((self.0 >> 8) as u32))
        } else {
            None
        }
    }
}
```

- [ ] **Step 2: Add ValueKind::Subr variant**

In the `ValueKind` enum (in `emacs_core/value.rs` or wherever it's defined), add:

```rust
Subr(SymId),
```

Update the `kind()` method to detect the new immediate encoding BEFORE the veclike check:

```rust
pub fn kind(self) -> ValueKind {
    // Check static subr FIRST (before other tag checks)
    if self.is_subr_static() {
        return ValueKind::Subr(self.as_subr_sym_id_static().unwrap());
    }
    // ... existing tag dispatch ...
}
```

- [ ] **Step 3: Update is_subr() to check both old and new encodings**

During the migration, both old heap subrs and new static subrs may exist. Update `is_subr()` to check both:

```rust
pub fn is_subr(self) -> bool {
    self.is_subr_static() || self.veclike_type() == Some(VecLikeType::Subr)
}
```

This dual check is temporary — once all old SubrObj code is removed, only `is_subr_static()` remains.

- [ ] **Step 4: Add as_subr_id() support for new encoding**

Update `as_subr_id()` to handle both:

```rust
pub fn as_subr_id(self) -> Option<SymId> {
    if let Some(sym_id) = self.as_subr_sym_id_static() {
        return Some(sym_id);
    }
    // Old path (to be removed)
    // ... existing code ...
}
```

- [ ] **Step 5: Build and verify**

```bash
cargo build --workspace 2>&1 | tail -10
```

- [ ] **Step 6: Commit**

```bash
git add neovm-core/src/tagged/value.rs neovm-core/src/emacs_core/value.rs
git commit -m "Add immediate subr value encoding with ValueKind::Subr"
```

---

### Task 3: Rewrite defsubr to use global table + new encoding

**Files:**
- Modify: `neovm-core/src/emacs_core/eval.rs` (defsubr_with_entry and related methods)

- [ ] **Step 1: Rewrite defsubr_with_entry**

Replace the current implementation that allocates SubrObj on the heap with one that inserts into the global table and sets the function cell to the new immediate value:

```rust
fn defsubr_with_entry(
    &mut self,
    name: &str,
    func: crate::tagged::header::SubrFn,
    min_args: u16,
    max_args: Option<u16>,
) {
    let (min_args, max_args, dispatch_kind) =
        super::subr_info::lookup_compat_subr_metadata(name, min_args, max_args);
    let sym_id = intern(name);
    let name_id = symbol_name_id(sym_id);

    // Insert into global subr table
    register_subr_entry(sym_id, SubrEntry {
        function: Some(func),
        min_args,
        max_args,
        dispatch_kind,
        name_id,
    });

    // Set the symbol's function cell to the new immediate subr value
    self.obarray.intern(name);
    self.obarray.set_symbol_function(name, Value::subr_from_sym_id(sym_id));
}
```

- [ ] **Step 2: Remove old Context subr methods that reference SubrObj**

Remove or replace:
- `subr_value()` — replace with global table lookup
- `subr_slot_mut()` — delete (no heap subr to mutate)
- `subr_ref()` — replace with global table lookup
- `register_subr_slot()` — delete
- `has_registered_subr()` — replace with global table check

Add replacement methods:

```rust
pub(crate) fn subr_dispatch_kind(&self, sym_id: SymId) -> Option<SubrDispatchKind> {
    with_subr_entry(sym_id, |e| e.dispatch_kind)
}
```

- [ ] **Step 3: Build and verify**

```bash
cargo build --workspace 2>&1 | tail -10
```

Expect compile errors from callers of removed methods — that's OK, they'll be fixed in subsequent tasks.

- [ ] **Step 4: Commit**

```bash
git add neovm-core/src/emacs_core/eval.rs
git commit -m "Rewrite defsubr to use global SubrTable with immediate values"
```

---

### Task 4: Migrate eval.rs dispatch and call site patterns

**Files:**
- Modify: `neovm-core/src/emacs_core/eval.rs` (all `ValueKind::Veclike(VecLikeType::Subr)` matches, dispatch functions)

- [ ] **Step 1: Rewrite dispatch_subr_value_internal**

Replace SubrObj dereference with global table lookup:

```rust
fn dispatch_subr_value_internal(
    &mut self,
    function: Value,
    args: Vec<Value>,
    wrong_arity_callee: Value,
) -> Option<EvalResult> {
    let sym_id = function.as_subr_id()?;
    let entry = lookup_subr_entry(sym_id)?;
    let func = entry.function?;
    // ... arity checking using entry.min_args, entry.max_args ...
    // ... dispatch via func ...
}
```

- [ ] **Step 2: Rewrite apply_subr_object to apply_subr_by_id**

Replace the method that takes a subr Value and dereferences the heap with one that looks up the global table by SymId:

```rust
fn apply_subr_by_id(
    &mut self,
    sym_id: SymId,
    args: Vec<Value>,
    rewrite_builtin_wrong_arity: bool,
) -> EvalResult {
    let entry = lookup_subr_entry(sym_id)
        .ok_or_else(|| signal("void-function", vec![Value::from_sym_id(sym_id)]))?;
    if entry.dispatch_kind == SubrDispatchKind::SpecialForm {
        return Err(signal("invalid-function", vec![Value::from_sym_id(sym_id)]));
    }
    if entry.dispatch_kind == SubrDispatchKind::ContextCallable {
        return self.apply_evaluator_callable_by_id(sym_id, args);
    }
    // ... arity check + dispatch via entry.function ...
}
```

- [ ] **Step 3: Update all ValueKind match patterns in eval.rs**

Replace every `ValueKind::Veclike(VecLikeType::Subr)` with `ValueKind::Subr(sym_id)`:

- Line ~413 (hashing): use sym_id directly
- Line ~6832 (callable check): use global table
- Line ~8837 (funcall dispatch): call apply_subr_by_id(sym_id, ...)
- Line ~9134 (named call resolution): match on Subr(sym_id) directly

- [ ] **Step 4: Update check_funcall_subr_arity_value**

Replace SubrObj field access with global table lookup.

- [ ] **Step 5: Update resolve_named_call_target_by_id**

The `NamedCallTarget::Subr(func)` variant currently stores a Value. Update to store `SymId` instead, or look up from global table when dispatching.

- [ ] **Step 6: Build — expect this to be iterative**

```bash
cargo build --workspace 2>&1 | tail -20
```

Fix remaining compile errors. This is the largest single task.

- [ ] **Step 7: Commit**

```bash
git add neovm-core/src/emacs_core/eval.rs
git commit -m "Migrate eval.rs dispatch to static subr table"
```

---

### Task 5: Migrate all other call sites

**Files:**
- Modify: `neovm-core/src/emacs_core/bytecode/vm.rs`
- Modify: `neovm-core/src/emacs_core/builtins_extra.rs`
- Modify: `neovm-core/src/emacs_core/subr/info.rs`
- Modify: `neovm-core/src/emacs_core/fns.rs`
- Modify: `neovm-core/src/emacs_core/dired.rs`
- Modify: `neovm-core/src/emacs_core/lread.rs`
- Modify: `neovm-core/src/emacs_core/debug.rs`
- Modify: `neovm-core/src/emacs_core/print.rs`

- [ ] **Step 1: Update bytecode VM**

In vm.rs, update `as_subr_id()` calls — these should work unchanged since we updated `as_subr_id()` to handle the new encoding. But verify any `VecLikeType::Subr` match patterns.

- [ ] **Step 2: Update builtins_extra.rs**

`subrp` at line 339: `args[0].as_subr_id().is_some()` — should work with new encoding. Verify.

- [ ] **Step 3: Update subr/info.rs**

Lines 343, 361, 529, 561: `args[0].as_subr_id().unwrap()` — should work. Arity computation at line 304 may need updating if it accessed SubrObj fields.

- [ ] **Step 4: Update fns.rs, dired.rs, lread.rs**

These use `dispatch_subr()` and `dispatch_subr_value()`. Update the dispatch methods if their signatures changed.

- [ ] **Step 5: Update debug.rs and print.rs**

Subr printing: extract SymId via `as_subr_id()`, resolve name via `resolve_sym()`.

- [ ] **Step 6: Build and verify**

```bash
cargo build --workspace 2>&1 | tail -15
```

- [ ] **Step 7: Commit**

```bash
git add neovm-core/src/
git commit -m "Migrate all call sites to static subr table"
```

---

### Task 6: Update pdump serialization

**Files:**
- Modify: `neovm-core/src/emacs_core/pdump/convert.rs`

- [ ] **Step 1: Update subr value serialization**

In convert.rs line 174: `let s = v.as_subr_id().unwrap();` — this should work since the new encoding still supports `as_subr_id()`. But verify the pdump value encoding/decoding handles the new immediate bit pattern.

The key change: pdump no longer needs to serialize SubrObj heap objects. A subr Value is just an immediate integer (SymId << 8 | 0x0F). When saving, store the raw bits. When loading, the bits are the same — no reconstruction needed.

- [ ] **Step 2: Remove DumpSubr and related pdump code**

Delete any `DumpSubr` struct, `dump_subr()`/`load_subr()` functions.

- [ ] **Step 3: Build and verify**

```bash
cargo build --workspace 2>&1 | tail -15
```

- [ ] **Step 4: Commit**

```bash
git add neovm-core/src/emacs_core/pdump/
git commit -m "Simplify pdump for static subr values"
```

---

### Task 7: Delete old SubrObj heap infrastructure

**Files:**
- Modify: `neovm-core/src/tagged/header.rs` (delete SubrObj struct)
- Modify: `neovm-core/src/tagged/value.rs` (remove old subr methods, thread-local registry)
- Modify: `neovm-core/src/tagged/gc.rs` (remove alloc_subr, subr_registry, GC tracing/sweep)

- [ ] **Step 1: Delete SubrObj from header.rs**

Remove the `SubrObj` struct definition (lines ~325-336). Keep `SubrFn` and `SubrDispatchKind` — they're used by SubrEntry.

Remove `VecLikeType::Subr` variant from the enum.

- [ ] **Step 2: Clean up tagged/value.rs**

Remove:
- `subr()` and `subr_name_id()` constructors (old encoding)
- `register_current_subr()`, `current_subr_value()`, `reset_current_subrs()` (thread-local registry)
- Old `as_subr_ref()` returning `&SubrObj`
- Remove the old-encoding fallback from `as_subr_id()` (keep only the new static path)
- Rename `is_subr_static()` to `is_subr()`, `as_subr_sym_id_static()` to `as_subr_sym_id()`

- [ ] **Step 3: Clean up tagged/gc.rs**

Remove:
- `subr_registry` and `subr_slot_registry` fields on `TaggedHeap`
- `alloc_subr()` method
- `subr_value()`, `subr_slot_mut()`, `register_subr_value()`, `clear_subr_registry()` methods
- GC root seeding for subr registry (lines ~1127-1130)
- SubrObj deallocation in sweep (line ~1438)
- Size computation for `VecLikeType::Subr` (line ~720)
- No-child marking for `VecLikeType::Subr` (line ~1344)

- [ ] **Step 4: Build and verify**

```bash
cargo build --workspace 2>&1 | tail -15
```

- [ ] **Step 5: Commit**

```bash
git add neovm-core/src/tagged/
git commit -m "Delete heap-based SubrObj infrastructure"
```

---

### Task 8: Test and verify

- [ ] **Step 1: Run tests**

```bash
cargo nextest run -p neovm-core 2>&1 > /tmp/test-results.txt; tail -20 /tmp/test-results.txt
```

- [ ] **Step 2: Fresh build**

```bash
GNU_EMACS=/home/exec/Projects/github.com/emacs-mirror/emacs/src/emacs cargo xtask fresh-build 2>&1 > /tmp/fresh-build.log; tail -5 /tmp/fresh-build.log
```

- [ ] **Step 3: TUI test — verify no void-function errors**

```bash
NEOMACS_RUNTIME_ROOT=. script -qec "timeout 10 target/debug/neomacs -Q -nw" /tmp/neomacs-test.log
```

Parse echo area for errors. Should show NO `void-function apply` or `void-function run-hook-wrapped`.

- [ ] **Step 4: Commit any final fixes**

```bash
git add neovm-core/src/
git commit -m "Fix static subr registry issues from testing"
```

---

## Execution Order

Tasks must be executed in order — each depends on the previous:
1. **Task 1** — additive (new SubrEntry/table, no breakage)
2. **Task 2** — additive (new encoding, dual-path compatibility)
3. **Task 3** — defsubr starts using new encoding (old code still compiles via dual path)
4. **Task 4** — largest task: migrate eval.rs dispatch (iterative compilation)
5. **Task 5** — migrate remaining call sites
6. **Task 6** — pdump simplification
7. **Task 7** — delete old infrastructure (only after everything uses new API)
8. **Task 8** — verify everything works end-to-end
