# Architecture

NEO Emacs rebuilds GNU Emacs as a layered Rust system: the Elisp runtime is a
self-contained core, editor subsystems are independent modules communicating through
defined APIs, and the rendering engine runs on a separate GPU thread.

## Current State

The shipped `neomacs` binary contains no C core. The Elisp runtime (evaluator,
bytecode VM, GC, portable dump), the editor subsystems (buffers, windows, frames,
keyboard, processes), the layout engine, and the wgpu rendering engine all run in
Rust. GNU Emacs serves as the behavioral test oracle: oracle suites, TUI grid
comparison tests, and GUI parity checks continuously diff NEO Emacs against GNU Emacs
to keep the rewrite honest.

### Workspace layout

All Cargo packages have one predictable home: `crates/<package-name>/`. The
workspace manifest lists those paths explicitly so adding, removing, or renaming
a package remains visible in review. GNU Emacs-derived runtime trees (`lisp/`,
`leim/`, `etc/`, `doc/`, and `test/`) stay at the repository root because they
are application resources rather than Rust packages.

Cargo does not expose a built-in workspace-root environment variable. The
workspace defines `CARGO_WORKSPACE_DIR` once in `.cargo/config.toml`; code that
needs repository resources uses that explicit compile-time input, while
crate-local fixtures continue to use `CARGO_MANIFEST_DIR`.

## Target Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        NEO Emacs (Rust)                         │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                  Elisp Runtime Core                      │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐       │   │
│  │  │ Evaluator   │  │ Bytecode VM │  │ GC/Allocator│       │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘       │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐       │   │
│  │  │ LispObject  │  │Symbol Table │  │ Type System │       │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘       │   │
│  └──────────────────────────────────────────────────────────┘   │
│                             │                                   │
│                             ▼                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                       Runtime API                        │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐       │   │
│  │  │register_type│  │register_root│  │define_func  │       │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘       │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐       │   │
│  │  │  run_hook   │  │  specbind   │  │signal_error │       │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘       │   │
│  └──────────────────────────────────────────────────────────┘   │
│                             │                                   │
│                             ▼                                   │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                     Editor Modules                        │  │
│  │  ┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐  ┌───────┐│  │
│  │  │ Buffer │  │ Window │  │ Frame  │  │Keyboard│  │Process││  │
│  │  └────────┘  └────────┘  └────────┘  └────────┘  └───────┘│  │
│  │  ┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐  ┌───────┐│  │
│  │  │ Font   │  │ Image  │  │File IO │  │ Reader │  │ Data  ││  │
│  │  └────────┘  └────────┘  └────────┘  └────────┘  └───────┘│  │
│  └───────────────────────────────────────────────────────────┘  │
│                             │                                   │
│                             ▼                                   │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                    Rendering Engine                       │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │  │
│  │  │Layout Engine│  │wgpu Renderer│  │ Animations  │        │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘        │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │  │
│  │  │    winit    │  │   WebKit    │  │ GStreamer   │        │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘        │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                      Threading                            │  │
│  │   ┌────────────┐                       ┌────────────┐     │  │
│  │   │EmacsThread │                       │RenderThread│     │  │
│  │   └────────────┘                       └────────────┘     │  │
│  │      │                                      ▲             │  │
│  │      ├── FrameGlyphBuffer (crossbeam) ──────┘             │  │
│  │      └── InputEvent (crossbeam) ────────────────────┐     │  │
│  │                                                     │     │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                       Backends                            │  │
│  │   ┌──────────┐         ┌──────────┐       ┌──────────┐    │  │
│  │   │  Vulkan  │         │  Metal   │       │ DX12/GL  │    │  │
│  │   └──────────┘         └──────────┘       └──────────┘    │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## Design Principles

- **Elisp Runtime Core** is a self-contained Rust crate. It owns LispObject, the
  evaluator, bytecode VM, GC, specpdl, and symbol table. It does NOT know about
  buffers, windows, frames, or any editor concept.
- **Runtime API** is a trait-based interface. Editor modules register their types
  (with GC trace descriptors), roots, and primitives. The GC traces registered
  types generically — no hardcoded `mark_kboards()` or `mark_terminals()`.
- **Editor Modules** are independent. Each owns its data structures and exposes
  them to Lisp through the Runtime API. Modules do not reach into each other's
  internals.
- **Rendering Engine** runs on a separate GPU thread, communicating via crossbeam
  channels (`FrameGlyphBuffer` down, `InputEvent` up).

### Elisp core source ownership

GNU Emacs' `src/` layout reflects its history as a collection of large C
translation units. Neomacs keeps the behavioral boundaries but uses a directory
per Rust subsystem:

```text
crates/neovm-core/src/emacs_core/
├── commands/  ├── display/  ├── editing/  ├── lisp/
├── runtime/   ├── system/   ├── text/     └── tests/
```

Each subsystem directory owns its implementation, private helpers, and tests.
The root `mod.rs` is the stable facade: it maps physical ownership to existing
paths such as `crate::emacs_core::eval`, so reorganizing files does not create a
workspace-wide API migration. New production Rust files must live below an
owning subsystem directory; architectural tests reject loose root and domain
files. See `crates/neovm-core/src/emacs_core/README.md` for the complete rules.

### Rust-backed Elisp functions

Rust-backed Elisp functions are declared with `SubrSpec`. A declaration keeps
the Lisp name, Rust function shape, observable arity, evaluator dispatch kind,
interactive contract, and startup policy together. `Context::register_subr` is
the only installation path into the static `SymId` registry used by the
evaluator, bytecode VM, JIT, and portable dumps.

Vector and slice entrypoints spell two independent contracts explicitly:
`NativeFn` identifies their Rust ABI (`ContextVec`, `ContextSlice`, or
`NoContextVec`), while `SubrArity` identifies their Lisp-visible argument
counts. Fixed-slot entrypoints instead use `SubrSpec::fixed0` through
`SubrSpec::fixed3`. Each constructor accepts only its exact Rust function
pointer type and derives the maximum Lisp arity; a closed `FixedMinN` enum
admits only minimum arities valid for that maximum. A fixed native function
can therefore no longer be paired with contradictory arity metadata.
Compile-fail doctests pin wrong function widths and wrong minimum-width types.

Subsystem-owned implementations live in their subsystem's `mod.rs`; their
declarations live in a sibling `subrs.rs`. The `define_subrs!` macro builds a
const `SubrBatch` and the subsystem's `register_subrs` function from the same
data. Its const constructor verifies the declaration's `subrs.rs` location,
with a compile-fail doctest pinning that placement rule,
and each batch is the executable value installed by production startup. The
test-only root catalog uses those same compiled batches to check the localized
inventory, duplicate Lisp names, and the batch-install trace produced by a real
`Context::new` startup. These checks operate on executable declaration data and
startup behavior rather than trying to infer architecture by parsing Rust
source syntax.

The not-yet-localized GNU compatibility surface is isolated as an ordered,
declaration-only manifest in
`crates/neovm-core/src/emacs_core/lisp/native/builtins/subrs/mod.rs`. Its order is a
startup compatibility boundary, so declarations move from it incrementally as
their owning subsystem gains a `subrs.rs`; new subsystem work does not add
registrations there.

The remaining historical data/evaluator registration milestones retain their
reviewed Neomacs order through an internal typestate sequence. This is a
Neomacs compatibility constraint, not a claim that GNU Emacs has corresponding
startup phases.

Private adapters in localized subsystems use names from their Rust domain
vocabulary. They do not repeat the Lisp identity with a `builtin_` prefix: the
descriptor already carries the Lisp-visible name. GNU-port names in the legacy
manifest remain unchanged until their declarations move to the owning module.

## Why Rust?

- **Memory safety** without garbage collection
- **Zero-cost abstractions** for high-performance rendering
- **Excellent FFI** with C libraries (GStreamer, WebKit, VA-API)
- **Modern tooling** (Cargo, async, traits)
- **Growing ecosystem** for graphics (wgpu, winit, cosmic-text)

## Why wgpu?

- **Cross-platform** — single API for Vulkan, Metal, DX12, and OpenGL
- **Safe Rust API** — no unsafe Vulkan/Metal code in application
- **WebGPU standard** — future-proof API design
- **Active development** — used by Firefox, Bevy, and many others

## Further Reading

- [The Rust display engine](rust-display-engine.md) — design document for the
  layout/rendering rewrite that replaced `xdisp.c`
- [Elisp core analysis](elisp-core-analysis.md) — in-depth analysis of the GNU Emacs
  C architecture, why it's hard to rewrite, and why Elisp is slow
- [Elisp VM design](elisp-vm-design.md) — the Rust Elisp virtual machine
- [GC design](neovm-gc-design.md) — the Rust garbage collector
