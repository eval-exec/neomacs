# Phase 1 Audit: Lisp VM Core

**Date**: 2026-03-28

## Scope

This phase is different from the others.

Neomacs is allowed to use a different internal VM architecture from GNU Emacs
for performance, JIT, and multithreading. The target here is **GNU-compatible
Lisp-visible semantics**, not a literal copy of GNU's VM internals.

So Phase 1 asks:

- Do the same Lisp programs evaluate the same way?
- Do they signal the same errors?
- Do they expose the same builtin/function/object shapes at the Lisp boundary?

It does **not** ask:

- Must Neomacs use the same evaluator, stack discipline, or GC structure as GNU?

## GNU source ownership

Primary GNU source files:

- `src/eval.c`
- `src/data.c`
- `src/alloc.c`
- `src/fns.c`
- `src/bytecode.c`
- `src/floatfns.c`
- `src/bignum.c`
- `src/lisp.h`

GNU uses these files together as one semantic core:

- `eval.c` owns special forms, dynamic control flow, and function calling.
- `data.c` owns core type predicates and low-level object operations.
- `alloc.c` owns allocation and GC-visible runtime behavior.
- `fns.c` owns many sequence, equality, and generic Lisp operations.
- `bytecode.c` owns the bytecode VM and byte-compiled function semantics.
- `floatfns.c` and `bignum.c` own numeric edge behavior.

## Neomacs source ownership

Primary Neomacs source files:

- `neovm-core/src/emacs_core/runtime/eval/mod.rs`
- `neovm-core/src/emacs_core/runtime/data/mod.rs`
- `neovm-core/src/emacs_core/runtime/alloc/mod.rs`
- `neovm-core/src/emacs_core/lisp/native/fns/mod.rs`
- `neovm-core/src/emacs_core/lisp/native/floatfns/mod.rs`
- `neovm-core/src/emacs_core/runtime/hashtab/mod.rs`
- `neovm-core/src/emacs_core/runtime/value/mod.rs`
- `neovm-core/src/emacs_core/runtime/symbol/mod.rs`
- `neovm-core/src/emacs_core/runtime/intern/mod.rs`
- `neovm-core/src/emacs_core/lisp/native/builtins/`
- `neovm-core/src/emacs_core/runtime/bytecode/`

Current design shape:

- `runtime/eval/mod.rs` is the main semantic hub through `Context`.
- `runtime/value`, `runtime/symbol`, and `runtime/intern` together define the
  runtime object boundary that Lisp code sees.
- `lisp/native/builtins` holds most primitive registrations and many core
  operations.
- `runtime/bytecode` contains a separate Rust compiler/decode/vm stack instead of
  copying GNU's `bytecode.c` structure.

## Audit result

Status is **partially compatible**.

Good:

- NeoVM already has broad oracle coverage in subsystem-owned `tests/`
  directories under `neovm-core/src/emacs_core/`.
- The Rust VM is rich enough to run large amounts of GNU Lisp.
- Bytecode, special forms, and core types exist as first-class runtime systems.

Not yet good enough:

- Builtin registration and bootstrap still rely on repair logic in places.
- Function-cell shape and bootstrap-time object shape are not always naturally
  preserved.
- GC-visible behavior is not yet documented as GNU-equal.
- Semantic equivalence is stronger in some areas than others; it is not yet a
  uniformly enforced contract.

## Long-term ideal design

The ideal long-term design is:

- Keep a **non-GNU internal VM architecture** if it is materially better for
  JIT, throughput, and concurrency.
- Make `neovm-core` the **only** semantic owner of Lisp evaluation, object
  model, bytecode semantics, builtin dispatch, and GC-visible behavior.
- Treat GNU Emacs as the semantic oracle at the Lisp boundary.

That means Neomacs should optimize freely internally, but must preserve:

- Lisp-visible error symbols and payloads
- builtin arity and special-form status
- `symbol-function` and callable object shape
- bytecode behavior
- observable GC/weak object semantics

## Required work

- Generate and enforce a GNU primitive manifest from GNU `DEFUN` / `defsubr`
  data.
- Keep expanding GNU-vs-NeoVM differential coverage for:
  special forms, `funcall`, `apply`, bytecode, weak objects, and GC-visible
  behavior.
- Remove bootstrap/runtime repair paths where they create semantic risk.
- Keep internal divergence only where it does not leak through the Lisp
  boundary.

## Exit criteria

- The Phase 1 boundary is semantically GNU-equal.
- Internal VM architecture may still differ.
- No higher phase depends on "special-case startup shims" to hide a VM semantic
  mismatch.
