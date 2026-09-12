# WebAssembly stack management: what the primary sources actually say

Checked 2026-09-12. Scope: the WebAssembly core/JS-API specs, the WebAssembly `design`,
`proposals`, `tool-conventions`, `stack-switching`, `js-promise-integration` and
`exception-handling` repositories, LLVM/lld source and option tables, Binaryen source,
Emscripten settings and sources, V8 and Chromium source, and the official build
configuration of four production interpreters that ship on wasm. Blog posts are used only
to locate primary material and are labelled as secondary where cited.

Local corroboration used the pinned toolchain (`rust-toolchain.toml` → 1.96.1, `rustc
--version --verbose` → LLVM 22.1.2), the vendored `stacker`/`psm` sources under
`~/.cargo/registry`, and `crates/neovm-core/src/emacs_core/runtime/stack_growth/mod.rs`.
No builds were run and no source was modified.

---

## VERDICT

**Now, in order:**

1. **Delete the wasm no-op in our own `stack_growth` wrapper.** `stacker::maybe_grow` is
   *not* a no-op on wasm — our wrapper makes it one. `psm` ships a hand-written wasm
   object whose `rust_psm_on_stack` does `global.get __stack_pointer` /
   `global.set __stack_pointer`, and `stacker`'s wasm32 path allocates the replacement
   stack from the Rust global allocator. That is precisely a segmented shadow stack, and
   it is one `cfg_select` branch away. *Justification: verified in the exact vendored
   crate versions this repo pins — see §Contradictions.*

2. **Raise `-z stack-size` to 8–16 MiB.** Every production interpreter on wasm does this:
   CRuby documents 16 MiB, Pyodide ships 10 MiB, .NET defaults to 5 MiB. We are on the
   rustc default of 1 MiB. *Justification: it is the one-line change with direct
   prior-art precedent, and its documented costs (initial memory, code size) are both
   bounded and measurable.*

3. **Determine which of the two limits we are actually hitting, because the fixes are
   disjoint.** `RuntimeError: memory access out of bounds` means the linear-memory
   shadow stack, which (1) and (2) fix. `RangeError: Maximum call stack size exceeded`
   means the engine stack, which *no linker flag can touch* — and in a Worker, which is
   where we run, Chromium caps V8's stack at **500 KiB**, roughly half the main-thread
   ~984 KiB. *Justification: we have reported both errors; only one of them is
   addressable by stack sizing, so guessing wrong wastes the whole effort.*

4. **Make the recursion guard byte-aware rather than frame-aware.** `max-lisp-eval-depth`
   counts frames; both wasm limits are bytes. Probing the shadow-stack pointer (via
   `psm::stack_pointer()`, which works on wasm) and signalling `excessive-lisp-nesting`
   before the trap is the only way to keep the error *catchable*. *Justification: a trap
   is definitionally uncatchable inside wasm (§4), so the guard must fire first or not
   at all.*

**Long-term:**

- **Heap-allocated interpreter state / an explicit continuation stack is the real answer,
  and it is not folklore — but it is also not an official WebAssembly recommendation.**
  Go's wasm backend does exactly this in production, documented in its own compiler
  source. No spec or working-group document recommends it; the evidence is
  implementation practice, not standards guidance. *Justification: it is the only
  approach that removes the ceiling rather than raising it.*
- **The stack-switching proposal will not help with this.** This is the single most
  important distinction in the report and it holds up: the proposal is silent on linear
  memory (§5). It replaces JSPI's coroutine machinery, not the shadow stack.
- **memory64 is irrelevant** and **Asyncify is legacy**; JSPI already shipped.

---

## Contradictions of the stated premises

Two of the measured premises I was given need correcting, and both change the plan.

### C1. `stacker::maybe_grow` is not a no-op on wasm — our own wrapper is

The premise was "`stacker::maybe_grow` is a deliberate no-op on wasm". The no-op is in
*this repository*, at
`crates/neovm-core/src/emacs_core/runtime/stack_growth/mod.rs:24-36`:

```rust
std::cfg_select! {
    target_family = "wasm" => {
        /// Browser WebAssembly uses its engine-managed stack, so there is no
        /// native segmented-stack facility to invoke.
        #[inline]
        pub(crate) fn maybe_grow<R>(
            red_zone: usize,
            stack_size: usize,
            callback: impl FnOnce() -> R,
        ) -> R {
            let _ = (red_zone, stack_size);
            callback()
        }
    }
```

The comment's premise is incorrect on both halves. Recursive Rust frames consume the
*linear-memory* shadow stack, not only the engine-managed stack; and a segmented-stack
facility does exist for wasm32 in the crates we already pin.

`Cargo.lock` pins `stacker` 0.1.24 and `psm` 0.1.31. In `psm-0.1.31/build.rs:53`, the
architecture table reads:

```
("wasm32", _, _, _) => Some(("src/arch/wasm32.o", true)),
```

The second field is `canswitch`; when true, `build.rs:89-91` emits
`cargo:rustc-cfg=switchable_stack`, which is what selects the `yes` arm of
`psm_stack_manipulation!`. `psm-0.1.31/src/arch/wasm32.s` — shipped prebuilt as
`wasm32.o`, per the comment "this source is only here as a reference for how the
corresponding wasm32.o was generated" — is a direct manipulation of the LLVM shadow-stack
global:

```wat
.globaltype __stack_pointer, i32

rust_psm_on_stack:
.functype rust_psm_on_stack (i32, i32, i32, i32) -> ()
    # get our new stack argument, then save the old stack
    # pointer into that local
    local.get 3
    global.get __stack_pointer
    local.set 3
    global.set __stack_pointer
    ...
    # restore the stack pointer before returning
    local.get 3
    global.set __stack_pointer
```

And `stacker-0.1.24/src/lib.rs:142-144` routes wasm32 to
`alloc_stack_restore_guard.rs`, which takes the new stack from the global allocator
rather than from `mmap`.

Three caveats that are real and that I verified in the same sources:

- **No guard page on the new segment.** `alloc_stack_restore_guard.rs` says so directly:
  "On these platforms we do not use stack guards. this is very unfortunate, but there is
  not much we can do about it without OS support." Overrunning a *segment* therefore
  corrupts the heap silently, which is strictly worse than the trap we get today. The
  red-zone probe is the only protection, so `RED_ZONE` must exceed the largest
  uninstrumented frame run.
- **The first probe always grows.** `stacker`'s `STACK_LIMIT` is seeded from
  `backends::guess_os_stack_limit()`, and wasm falls to `backends/fallback.rs`, which is
  `pub unsafe fn guess_os_stack_limit() -> Option<usize> { None }`. `maybe_grow` then
  takes `None => false` for `enough_space` and unconditionally calls `grow()`. After the
  first switch, `set_stack_limit(Some(stack_base))` makes subsequent probes accurate. Net
  effect: one segment allocation per deep descent, not per call — but it *is* an
  unconditional allocation at the first probe.
- **`psm` ships a precompiled object.** `wasm32.o` was assembled by an external
  `llvm-mc`; it must link cleanly against LLVM 22.1.2. This is the one part of the
  recommendation that needs an actual build to confirm.

### C2. `-z stack-size` *is* in the build — rustc injects it

The premise was "No `-z stack-size` anywhere in the build". True of our build
configuration, but rustc's wasm target spec supplies both flags unconditionally.
`compiler/rustc_target/src/spec/base/wasm.rs:10-30`:

```rust
// By default LLD only gives us one page of stack (64k) which is a
// little small. Default to a larger stack closer to other PC platforms
// (1MB) and users can always inject their own link-args to override this.
concat!($prefix, "-z"),
concat!($prefix, "stack-size=1048576"),
```

That is the exact provenance of the measured `0x100000`. The same block explains the
`--stack-first` observation:

```rust
// This has the unfortunate consequence that on stack overflows you
// corrupt static data and can cause some exceedingly weird bugs. To
// help detect this a little sooner we instead request that the stack is
// placed before static data.
//
// This means that we'll generate slightly larger binaries as references
// to static data will take more bytes in the ULEB128 encoding, but
// stack overflow will be guaranteed to trap as it underflows instead of
// corrupting static data.
concat!($prefix, "--stack-first"),
```

Source: <https://github.com/rust-lang/rust/blob/master/compiler/rustc_target/src/spec/base/wasm.rs>.
The practical consequence is good news: overriding it needs no build-script surgery, only
`-C link-arg=-zstack-size=<n>`, because rustc's own args are *pre*-link args.

---

## 1. Stack model — what lives where, and why the shadow stack is a convention

**The spec has one stack, and it is not in linear memory.** The core specification's
appendix on implementation limitations enumerates runtime limits over "the number of
[frames](../exec/runtime.html#syntax-frame) on the [stack](../exec/runtime.html#stack)",
"the number of [labels]…", "the number of [values]…" and "the number of [handlers]…"
(<https://webassembly.github.io/spec/core/appendix/implementation.html>). Frames, labels
and operands are abstract-machine entities; nothing in the core spec places them at an
address.

**The split between the two stacks is stated most precisely in the design repository.**
`Security.md:48-58`:

> Local variables with fixed scope and global variables are represented as fixed-type
> values stored by index. The former are initialized to zero by default and are stored in
> the protected call stack, whereas the latter are located in the global index space and
> can be imported from external modules. Local variables with unclear static scope (e.g.
> are used by the address-of operator, or are of type `struct` and returned by value) are
> stored in a separate user-addressable stack in linear memory at compile time.

So, precisely:

| Value class | Lives where |
| --- | --- |
| Declared `local`s and operands, frames, labels, return addresses | engine-managed "protected call stack", not addressable |
| `global`s | global index space, not addressable |
| Address-taken locals, `struct`s returned by value, `alloca`/VLAs, register spills that need addresses, anything a pointer can reach | the linear-memory stack |

`Rationale.md#Locals` names the mechanism and calls it a compiler construct, not a spec
one:

> Since WebAssembly's local variables are outside the address space, C/C++ compilers
> implement address-taken variables by creating a separate stack data structure within
> linear memory. This stack is sometimes called the "aliased" stack, since it is used for
> variables which may be pointed to by pointers.
>
> Since the aliased stack appears to the WebAssembly engine as normal memory, WebAssembly
> optimizations that would target the aliased stack need to be more general, and thus
> more complicated.

Sources: <https://github.com/WebAssembly/design/blob/main/Security.md>,
<https://github.com/WebAssembly/design/blob/main/Rationale.md>.

**Why it is a convention and not a spec concept.** Three reasons, all documented:

1. The engine cannot tell it apart from any other memory — "the aliased stack appears to
   the WebAssembly engine as normal memory" (Rationale.md, above). There is nothing for
   the spec to specify.
2. It is instead specified in the *tool* conventions. `__stack_pointer` appears in
   `tool-conventions/DynamicLinking.md:146-147` — "`env.__stack_pointer` - A mutable
   `i32` global representing the explicit stack pointer as an offset into the above
   memory" — and in `Linking.md:527-528`: "the `__stack_pointer` symbol may be provided
   at link-time". Both are tool-convention documents, not specifications.
   Sources: <https://github.com/WebAssembly/tool-conventions/blob/main/DynamicLinking.md>,
   <https://github.com/WebAssembly/tool-conventions/blob/main/Linking.md>.
3. Making the engine stack's depth observable was deliberately avoided. `Nondeterminism.md:40-42`
   files stack exhaustion under nondeterminism, not semantics: "Program stack may get
   exhausted (e.g., because function call depth is too big, or functions have too many
   locals, or infinite recursion). Note that this stack isn't located in the
   program-accessible linear memory."
   Source: <https://github.com/WebAssembly/design/blob/main/Nondeterminism.md>.

The spec editor restated the underlying reason this month, in
<https://github.com/WebAssembly/stack-switching/issues/156> (rossberg, comment of
2026-09-04):

> One question we got repeatedly in the past was why Wasm cannot make stack overflow
> deterministic, by configuring or prescribing stack size one way or the other. […] there
> is no portable metric for "stack size", not even approximate, not even for a single
> engine.

The consequence for us: a frame-count guard like `max-lisp-eval-depth` can never be
*calibrated* to the engine stack in a portable way. It can only be set conservatively.

---

## 2. Overflow semantics: `--stack-first`, the real default, and detection

### `--stack-first` — yes, trapping is the documented rationale

From lld's own source, `lld/wasm/Writer.cpp:338-348` (the `layoutMemory()` header):

```
// The default memory layout is as follows, from low to high.
//
//  - initialized data (starting at ctx.arg.globalBase)
//  - BSS data (not currently implemented in llvm)
//  - explicit stack (ctx.arg.ZStackSize)
//  - heap start / unallocated
//
// The --stack-first option means that stack is placed before any static data.
// This can be useful since it means that stack overflow traps immediately
// rather than overwriting global data, but also increases code size since all
// static data loads and stores requires larger offsets.
```

Source: <https://github.com/llvm/llvm-project/blob/main/lld/wasm/Writer.cpp>. So the
premise is confirmed by the linker's own comment, and rustc's comment (§C2) says the same
thing independently.

The mechanism in the placed-first layout is worth being precise about: `placeStack()`
starts from `memoryPtr = 0`, so `__stack_low` is 0 and `__stack_pointer` is initialised to
the stack size. Decrementing past 0 makes the `i32` stack pointer negative, which as an
unsigned linear-memory offset is enormous, and the access is out of bounds. rustc's
"guaranteed to trap as it underflows" is right for ordinary prologue arithmetic, but it is
an emergent property of `i32` wraparound rather than a guard page: there is no protected
region, only an unmapped-by-arithmetic one.

**`--stack-first` is now the lld default, including in the LLVM we ship.** `Options.td:253-255` (`release/22.x`; `256-258` on `main`):

```
defm stack_first: B<"stack-first",
    "Place stack at start of linear memory (default)",
    "Place the stack after static data region">;
```

and `Driver.cpp:598`: `ctx.arg.stackFirst = args.hasFlag(OPT_stack_first, OPT_no_stack_first, true);`.
(Line numbers here are from `release/22.x`, matching this repo's LLVM; they differ on `main`.)
I verified both on the `release/22.x` branch, matching this repo's LLVM 22.1.2. The change
landed in <https://github.com/llvm/llvm-project/pull/166998> ("[lld][WebAssembly] Default
to `--stack-first`", merged 2025-11-08).

The motivating issue, <https://github.com/llvm/llvm-project/issues/151015> (opened
2025-07-28), is the best single statement of the threat model and should be read in full.
Its load-bearing sentences:

> With stack protectors disabled by default, no dynamic stack overflow checks, a default
> stack size of 64K (the smallest of any non-bare-metal platform), and no memory
> protection, WebAssembly is uniquely vulnerable to stack smashing in a way that only the
> simplest microcontrollers that lack an MPU also are.

> Even ignoring the security aspect, the developer experience of encountering a stack
> overflow in the wild is confusing and difficult to recognize even for a professional
> systems developer […] and LLVM's suite of sanitizers isn't available to detect it when
> building for Wasm.

> Without inserting stack probes, using guard pages won't be enough to eliminate stack
> smashing for functions that use VLAs or alloca, so even if memory control was widely
> available, `--stack-first` has its merit until stack probes are implemented in LLVM for
> WebAssembly and enabled by default.

That last sentence answers a question the report brief did not ask but should have: **LLVM
has no stack probes for WebAssembly.** There is therefore no compiler-level equivalent of
the native stack-probe/guard-page mechanism, and none is imminent.

### wasm-ld's actual default `-z stack-size`

**64 KiB, and it has not changed.** `lld/wasm/Driver.cpp:640-641` (`release/22.x`):

```cpp
ctx.arg.zStackSize =
    args::getZOptionValue(args, OPT_z, "stack-size", WasmDefaultPageSize);
```

and `llvm/include/llvm/BinaryFormat/Wasm.h:34`: `const uint32_t WasmDefaultPageSize = 65536;`.
Issue #151015 independently describes it as "a default stack size of 64K (the smallest of
any non-bare-metal platform)". The 1 MiB in our binary is rustc's override (§C2), not
lld's default.

Documented constraints on the value, all from `Writer.cpp:352-378` (`main`):

- 16-byte alignment is enforced: `error("stack size must be " + Twine(stackAlignment) + "-byte aligned")`, with `static constexpr int stackAlignment = 16;`.
- With `--stack-first`, `--global-base` may not be below the stack: `error("--global-base cannot be less than stack size when --stack-first is used")`.
- No ceiling is documented beyond having to fit in linear memory. lld's docs state that
  initial memory is "the sum of stack, static data and heap sizes" by default
  (<https://lld.llvm.org/WebAssembly.html>), so raising the stack raises the module's
  initial memory 1:1 — a 16 MiB stack means ~15 MiB more address space reserved and
  zero-filled at instantiation.
- Code size grows, per the `layoutMemory()` comment quoted above, because static data moves to
  higher addresses and its ULEB128 offsets lengthen. Issue #151015 quantifies this as
  "no code size overhead […] when otherwise default linker options are used" and "well
  below 1% size overhead" in conjunction with `--compress-relocations`.

**A documentation discrepancy worth knowing about:** the rendered lld manual at
<https://lld.llvm.org/WebAssembly.html> describes `--stack-first` as "Place stack at start
of linear memory rather than after data" with no mention that it is now the default, and
omits `-z stack-size` entirely. `Options.td` is authoritative; the `.rst` is stale. I
confirmed the same staleness on `release/22.x`.

### Detecting shadow-stack overflow before it traps

There is **no standard mechanism** — no guard page, no stack probe, no spec-level hook.
Issue #151015 says as much ("no dynamic stack overflow checks"), and guard pages depend on
the [memory control](https://github.com/WebAssembly/memory-control) proposal, which is
Phase 1 (§5). What exists is toolchain instrumentation.

**What Emscripten's check actually does.** From the settings reference
(<https://emscripten.org/docs/tools_reference/settings_reference.html>), `STACK_OVERFLOW_CHECK`:

> - 0: Stack overflows are not checked.
> - 1: Adds a security cookie at the top of the stack, which is checked at end of each
>   tick and at exit (practically zero performance overhead)
> - 2: Same as above, but also runs a binaryen pass which adds a check to all stack
>   pointer assignments. Has a small performance cost.

It defaults to 1 whenever `ASSERTIONS=1`, which is itself the default below `-O1`.
`STACK_SIZE` is documented with the blunt warning: "There is no way to enlarge the stack,
so this value must be large enough for the program's requirements. If assertions are on,
we will assert on not exceeding this, otherwise, it will fail silently. Default value:
64*1024".

Level 1 is *post-hoc*: `src/runtime_stack_check.js` writes `0x02135467`/`0x89BACDFE` to
the final two words of the stack and compares them later, aborting with "Stack overflow!
Stack cookie has been overwritten at …". It detects that overflow happened; it does not
prevent the corruption, and it cannot fire at the moment of the overflow.
Source: <https://github.com/emscripten-core/emscripten/blob/main/src/runtime_stack_check.js>.

**Level 2 has a plain-Rust equivalent, and this is the useful finding.** The Binaryen pass
Emscripten invokes is toolchain-agnostic — it keys off `__stack_pointer`, which our module
has. `binaryen/src/passes/StackCheck.cpp:17-21`:

```
// Enforce stack pointer limits.  This pass will add checks around all
// assignments to the __stack_pointer global that LLVM uses for its
// shadow stack.
```

The pass adds `__stack_base`/`__stack_limit` mutable globals, rewrites every
`global.set $__stack_pointer` into a bounds check followed by the set, and exports
`__set_stack_limits(base, limit)` for the embedder to call at startup. With
`--pass-arg=stack-check-handler@<name>` it imports `env.<name>` and calls it on breach —
"If we imported a handler, call it. That can show a nice error in JS. Otherwise, just
trap." Source: <https://github.com/WebAssembly/binaryen/blob/main/src/passes/StackCheck.cpp>.

For us that means `wasm-opt --stack-check` over a plain `wasm32-unknown-unknown` artifact
converts a wild out-of-bounds access into a deterministic call to a handler of our
choosing, at every stack-pointer assignment. The documented cost is "a small performance
cost"; on an interpreter that is a per-call cost and should be measured before shipping,
but as a *diagnostic* build it is free of judgement calls.

Two things that sound like solutions and are not:

- **`__stack_chk_fail` / `-fstack-protector`** guards against *buffer overflow within a
  frame*, not stack exhaustion. Issue #151015 lists "stack protectors disabled by
  default" as a contributing factor to stack *smashing*, which is the buffer-overflow
  problem. It would not catch deep recursion.
- **`__stack_low` / `__stack_high`** are available from the linker but not in our binary.
  `lld/wasm/Driver.cpp:1030-1031` (`main`) creates them via `addDataLayoutSymbol`, which is
  `symtab->addOptionalDataSymbol(s)` — defined only if referenced. A `strings` scan of
  `tmp/neomacs-wasm-dist/builds/*/neomacs_wasm_worker.wasm` finds `__data_end`,
  `__heap_base` and `__stack_pointer` but neither `__stack_low` nor `__stack_high`, as
  expected. Referencing them from Rust would cause lld to define them, which is the clean
  way to get the real bounds for a hand-rolled guard instead of hard-coding 1 MiB.

---

## 3. Engine stack vs shadow stack under JSPI

### They are genuinely independent, and each has its own failure signature

| | Shadow stack | Engine stack |
| --- | --- | --- |
| Where | linear memory, `[0, stack-size)` under `--stack-first` | engine-managed, outside linear memory |
| Sized by | `-z stack-size` (1 MiB here) | V8 flags / embedder (`SetStackLimit`) |
| Consumed by | address-taken locals, spills, `alloca` | frames, declared locals, operands, labels |
| Breach looks like | `WebAssembly.RuntimeError: memory access out of bounds` | `RangeError: Maximum call stack size exceeded` |
| Spec status of breach | a trap (out-of-bounds access) | nondeterministic resource exhaustion |

Exhausting one while the other has room is not merely possible, it is the normal case:
they are consumed at different rates by different value classes, and a function with many
declared-but-not-address-taken locals consumes only the engine stack, while one with a
large address-taken buffer consumes almost only the shadow stack. `Nondeterminism.md`
confirms the engine stack "isn't located in the program-accessible linear memory" (§1),
and the JS-API spec routes the two breaches to different error classes (§4).

### V8's JSPI side stack: size, configurability, and one nasty default

That JSPI runs wasm on a second stack is confirmed by V8's own flag descriptions in
`src/flags/flag-definitions.h:2058-2070`:

```cpp
DEFINE_DEBUG_BOOL(trace_wasm_stack_switching, false,
                  "trace wasm stack switching")
DEFINE_BOOL(stress_wasm_stack_switching, false,
            "Always run wasm on a secondary stack, even when it is called "
            "with a regular (non-JSPI) export")
DEFINE_INT(wasm_stack_switching_stack_size, V8_DEFAULT_STACK_SIZE_KB,
           "default size of stacks for wasm stack-switching (in kB)")
DEFINE_INT(wasm_stack_pool_capacity_mb, 500,
           "default capacity for the wasm stack pool in MB, -1 for unlimited")
// 1 will be rounded up to the smallest possible initial stack size, which
// depends on the stack limit margin and the platform's page size.
DEFINE_VALUE_IMPLICATION(wasm_growable_stacks,
  wasm_stack_switching_stack_size, 1)
```

So: **configurable via `--wasm-stack-switching-stack-size=<kB>`, defaulting to
`V8_DEFAULT_STACK_SIZE_KB`.** That constant, from `src/common/globals.h:175-208`, is
**984** on the default path ("Slightly less than 1MB, since Windows' default stack size
for the main execution thread is 1MB"), 864 on ARM, 472 on ia32, 960 under ASan. The
accompanying `V8_STACK_LIMIT_MARGIN_KB` (40 KB by default) is "an extra safety headroom
between the JS/Wasm stack limit and the estimated system stack limit […] needed to safely
run C++ code or builtins without stack checks when JS/Wasm is close to the stack limit".

**The growable side stack is experimental and off by default.** `growable_stacks` is
declared in `src/flags/feature-flags.h:118` as
`WASM_FEATURE(growable_stacks, "growable stacks for jspi")`, inside the block whose
heading comment begins at line 62:

```
// Experimental features (disabled by default).
// These features are still in active development and not stable enough for
// fuzzing or developer testing yet.
#define FOREACH_EXPERIMENTAL_FEATURE_FLAG(...)
```

So in stock Chrome the JSPI secondary stack is a **fixed** allocation, not a growable one.
By contrast `jspi` itself sits in `src/wasm/wasm-features.h` under "Features that are
always enabled and do not have a flag", which is the cleanest available confirmation that
JSPI is fully shipped in V8.
Sources: <https://github.com/v8/v8/blob/main/src/flags/flag-definitions.h>,
<https://github.com/v8/v8/blob/main/src/common/globals.h>,
<https://github.com/v8/v8/blob/main/src/flags/feature-flags.h>,
<https://github.com/v8/v8/blob/main/src/wasm/wasm-features.h>.

### The Worker penalty — directly relevant, since we run in a Worker

Chromium sets a *separate, smaller* V8 stack limit for workers.
`third_party/blink/renderer/bindings/core/v8/v8_initializer.cc:1155-1178`:

```cpp
// Stack size for workers is limited to 500KB because default stack size for
// secondary threads is 512KB on macOS. See GetDefaultThreadStackSize() in
// base/threading/platform_thread_apple.mm for details.
//
// For 32-bit Windows, the stack region always starts with an odd number of
// reserved pages, followed by two guard pages, followed by the committed
// memory for the stack, and the worker stack size need to be reduced
// (https://crbug.com/1412239).
#if defined(ARCH_CPU_32_BITS) && BUILDFLAG(IS_WIN)
static const int kWorkerMaxStackSize = 492 * 1024;
#else
static const int kWorkerMaxStackSize = 500 * 1024;
#endif

void V8Initializer::InitializeWorker(v8::Isolate* isolate) {
  ...
  isolate->SetStackLimit(GetCurrentStackPosition() - kWorkerMaxStackSize);
```

Source: <https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/bindings/core/v8/v8_initializer.cc>
(read via the GitHub mirror). V8's own comment at `flag-definitions.h:3221-3229` points at
this exact function as "a place where custom thread stack sizes are configured" and warns
"be careful about using this process-wide flag".

**A hypothesis this raises about our measurements, which I flag as unverified inference:**
dying at 400 nested macro-expansion levels with a 1 MiB shadow stack implies ~2.6 KiB of
shadow stack per level, which is large for an interpreter loop; the same 400 levels against
a 500 KiB engine limit implies ~1.25 KiB per level of engine frame, which is very ordinary
for a Rust interpreter with several wasm frames per Lisp level. If the observed Worker
death is a `RangeError`, then **`-z stack-size` will not move the number at all** and the
whole shadow-stack line of attack is misdirected. Distinguishing them is cheap — catch the
error in the Worker and read its constructor name — and should be done before changing any
linker flag.

### JSPI standardization status and version floor

- **Phase 5** ("The Feature is Standardized"), per `proposals/README.md`, in the list
  annotated "_These proposals have not yet been merged to the spec._"
  Source: <https://github.com/WebAssembly/proposals/blob/main/README.md>.
- **Chrome 137, Firefox 139**: "JSPI is available in Chrome 137, and in Firefox 139."
  Source: <https://v8.dev/blog/jspi> (V8 team blog — vendor-official, not a spec).
- MDN's `WebAssembly.Suspending` page cites the spec as
  <https://webassembly.github.io/js-promise-integration/js-api/#suspending>.

### The reentrancy hazard the JSPI proposal itself warns about

`proposals/js-promise-integration/Overview.md`, "Supporting Responsive Applications with
Reentrancy":

> In fact, our example above is already technically re-entrant! We can call
> `promise_update` even before other calls to `promise_update` have returned. However,
> JSPI does not guarantee that the updates are completed in any particular order: it is up
> to the application developer to ensure that this is safe.

> Not all applications can equally tolerate being reentrant in this way. Certainly,
> languages in the C family do not make this straightforward. In fact, an application would
> typically have to have been engineered appropriately, by, for example, ensuring that each
> call to a suspending import does not interfere with globally shared state.

Source: <https://github.com/WebAssembly/js-promise-integration/blob/main/proposals/js-promise-integration/Overview.md>.

This matters to us specifically because **`__stack_pointer` is globally shared state.**
Nothing in the JSPI spec algorithms saves or restores module globals across a suspension —
the proposal's spec section defines `Suspending`/`promising` purely in terms of promise
plumbing. With ~30 blocking imports, any two concurrently-suspended computations share one
shadow-stack region and one stack pointer. My reading of the mechanics is that they do not
*clobber* each other, because each suspended frame holds its own stack-pointer value in a
wasm local and the newer computation grows downward from wherever the suspended one left
off — but the 1 MiB is then a *shared* budget across all suspended computations, not a
per-computation one. **I could not verify this from a primary source and it is inference;
see the gap list.** It is worth an experiment before relying on deep recursion inside a
suspended computation.

---

## 4. Traps are not catchable — confirmed, with one correction about instance state

**Not catchable inside wasm, surfaces as `RuntimeError`.** The JS-API spec, §7 "Error
Condition Mappings to JavaScript":

> Running WebAssembly programs encounter certain events which halt execution of the
> WebAssembly code. WebAssembly code (currently) has no way to catch these conditions and
> thus an exception will necessarily propagate to the enclosing non-WebAssembly caller
> (whether it is a browser, JavaScript or another runtime system) where it is handled like
> a normal JavaScript exception.

For the error class: "If ret is error, throw an exception. This exception should be a
WebAssembly `RuntimeError` exception, unless otherwise indicated by the WebAssembly error
mapping." MDN puts it plainly: "The `WebAssembly.RuntimeError` object is the error type
that is thrown whenever WebAssembly specifies a trap."
Sources: <https://webassembly.github.io/spec/js-api/index.html>,
<https://developer.mozilla.org/en-US/docs/WebAssembly/Reference/JavaScript_interface/RuntimeError>.

**This is why `panic::set_hook` cannot see it.** A Rust panic is a Rust-level control-flow
mechanism that runs Rust code (the hook) before unwinding or aborting. A trap is an
engine-level termination of the wasm computation: no wasm instruction executes after it, so
no Rust code — hook, `Drop`, `catch_unwind` landing pad — can run. The trap is delivered to
the JS frame that called in.

**Correction to the premise about instance state.** The premise was that the instance is
poisoned. The spec says the opposite, in the same section:

> Because JavaScript exceptions can be handled, and JavaScript can continue to call
> WebAssembly exports after a trap has been handled, traps do not, in general, prevent
> future execution.

So at the level the spec cares about, neither the module nor the instance is poisoned: the
instance remains callable, its memory and globals persist exactly as the trap left them.
What *is* broken is our own invariants — a shadow-stack underflow trap leaves
`__stack_pointer` at a nonsense value and every Rust data structure mid-mutation, with no
`Drop` having run. The recovery story is therefore: the *engine* lets you call back in; the
*program* has no idea what state it is in. Restarting the interpreter inside the same
instance is only safe if you can re-establish every invariant from scratch, which for an
Emacs-shaped heap effectively means it is not safe. Discarding the instance is the honest
response, and that is a statement about our program, not about wasm.

**The exception-handling proposal does not change this — and the "stack overflow" case is
called out explicitly.** `proposals/exception-handling/Exceptions.md:188-209`:

> #### Traps
>
> Catch clauses handle exceptions generated by the `throw` instruction, but do not catch
> traps. The rationale for this is that in general traps are not locally recoverable and
> are not needed to be handled in local scopes like try blocks.

> 1. In order to be consistent before and after a trap reaches a JavaScript frame, the
>    `try_table` instruction does not catch exceptions generated from traps.
> 2. The `try_table` instruction does not catch JavaScript exceptions generated from stack
>    overflow and out of memory.

> Traps currently generate instances of `WebAssembly.RuntimeError`, but this detail is not
> used to decide type. Implementations are supposed to specially mark non-catchable
> exceptions.

Source: <https://github.com/WebAssembly/exception-handling/blob/main/proposals/exception-handling/Exceptions.md>.

The premise was right, and the second clause strengthens it: even where a *breach of the
engine stack* reaches wasm as a JS exception (which `try_table` can otherwise catch),
`try_table` is specified not to catch it. Neither breach is catchable in-module by any
shipped or proposed mechanism. Exception handling is itself finished — Phase 5, merged for
spec 3.0 per `finished-proposals.md` — so this is settled, not pending.

---

## 5. The long-term answer, ranked

### 5.1 Raise `-z stack-size` — a stopgap, but the correct first move

Documented tradeoffs and constraints are in §2: 16-byte alignment, `--global-base`
interaction, initial memory rising 1:1, slightly larger code from longer ULEB128 offsets,
and no documented ceiling other than fitting in linear memory. There is no official
guidance on *what* value to choose; the only evidence is what shipping runtimes picked
(§6): 5 MiB, 10 MiB, 16 MiB.

It is explicitly a stopgap: it moves the wall, and the wall is still a trap.

### 5.2 Stack switching / typed continuations — **does not address the shadow stack**

This is the most important distinction in the report, and it survives scrutiny.

- **Phase 3** ("Implementation Phase"), per `proposals/README.md`.
- **The Explainer does not mention linear memory at all.** A case-insensitive count of
  "memory" in `proposals/stack-switching/Explainer.md` returns **0**. Greps for "shadow
  stack", "stack pointer", "linear memory", "`__stack`" and "toolchain" all return nothing.
  The document's own framing is entirely about engine-level control flow: "This proposal
  adds typed stack-switching to WebAssembly, enabling a single WebAssembly instance to
  manage multiple execution stacks concurrently. The primary use-case for stack-switching
  is to add direct support for modular compilation of advanced non-local control flow
  idioms, e.g. coroutines, async/await, generators, lightweight threads, and so forth."
  Source: <https://github.com/WebAssembly/stack-switching/blob/main/proposals/stack-switching/Explainer.md>.
- **What it gives an interpreter:** first-class one-shot continuations — `cont.new`,
  `resume`, `suspend`, `switch` — i.e. a principled replacement for JSPI and for Asyncify,
  and a native implementation path for generators, green threads and `call/cc`-shaped
  control. For an Emacs-shaped interpreter that is real value for *concurrency* and for
  non-local exits.
- **What it does not give:** any relief on shadow-stack depth. Each continuation gets its
  own *engine* stack; the linear-memory stack remains one region governed by one
  `__stack_pointer` global, and per-continuation shadow stacks remain the toolchain's
  problem.

The cleanest existing proof of that last point is Emscripten's fiber API, which has
supported stack switching (via Asyncify) for years and requires the *caller* to supply
**both** stacks, separately, per fiber. `system/include/emscripten/fiber.h`:

```c
typedef struct emscripten_fiber_s {
  void *stack_base;             /** Where the C stack starts (NOTE: grows down). */
  void *stack_limit;            /** Where the C stack ends. */
  void *stack_ptr;              /** Current position in the C stack. */
  ...
  asyncify_data_t asyncify_data;
} emscripten_fiber_t;

void emscripten_fiber_init(
  emscripten_fiber_t * _Nonnull fiber,
  em_arg_callback_func entry_func,
  void *entry_func_arg,
  void * _Nonnull c_stack,
  size_t c_stack_size,
  void * _Nonnull asyncify_stack,
  size_t asyncify_stack_size
);
```

Source: <https://github.com/emscripten-core/emscripten/blob/main/system/include/emscripten/fiber.h>.
The "C stack" is the linear-memory shadow stack and the toolchain/application allocates and
sizes it. Nothing in the stack-switching proposal changes who owns that.

Corroborating this from the other direction, the proposal's active discussion of stack
sizing — issue #156, "Customizing `cont.new` with stack size", opened 2026-09-03 — is
entirely about the *engine* stack per continuation, and even there the champion notes "the
toolchain may not know how much space to allocate for the stack. In particular, WebAssembly
does not expose this information -- for good reasons." Source:
<https://github.com/WebAssembly/stack-switching/issues/156>.

**Verdict: stopgap-irrelevant. It is the right long-term answer to JSPI, and no answer at
all to our depth problem.**

### 5.3 Explicit continuation-passing / heap-allocated interpreter state — the real fix, but not an *official* recommendation

I could find **no** WebAssembly specification, design document, or working-group text that
recommends this for interpreters. So it is not "the documented recommendation". Calling it
folklore is also wrong, though: it is documented *implementation practice* in a
first-party compiler, at production scale.

Go's wasm backend does exactly this, and says so in its own compiler source,
`src/cmd/compile/internal/wasm/ssa.go:56-83`:

```
   Threads:

   Wasm doesn't (yet) have threads. We have to simulate threads by
   keeping goroutine stacks in linear memory and unwinding
   the Wasm stack each time we want to switch goroutines.

   To support unwinding a stack, each function call returns on the Wasm
   stack a boolean that tells the function whether it should return
   immediately or not. When returning immediately, a return address
   is left on the top of the Go stack indicating where the goroutine
   should be resumed.

   Stack pointer:

   There is a single global stack pointer which records the stack pointer
   used by the currently active goroutine. This is just an address in
   linear memory where the Go runtime is maintaining the stack for that
   goroutine.
   ...
   Calling convention:

   All Go arguments and return values are passed on the Go stack, not
   the wasm stack. In addition, return addresses are pushed on the
   Go stack at every call point.
```

Source: <https://github.com/golang/go/blob/master/src/cmd/compile/internal/wasm/ssa.go>.
The prologue emitted in `src/cmd/internal/obj/wasm/wasmobj.go` still calls
`runtime.morestack`, so Go's growable stacks work on wasm the same way they do natively:
the runtime owns the stack, in linear memory, and grows it. Go consequently does not have
a shadow-stack ceiling at all — it never adopted the LLVM shadow-stack convention.

Guile Hoot is the nearest Lisp-family data point: it CPS-converts late in the compiler and
represents Scheme data in WasmGC rather than on any linear-memory stack. The available
write-ups are the implementer's blog (<https://wingolog.org/archives/2024/05/27/cps-in-hoot>)
and the project page (<https://www.spritely.institute/hoot/>) — **secondary sources**, and I
did not find a primary design document stating a shadow-stack rationale, so I am not
resting anything on it.

**Verdict: this is the ideal long-term shape, on the evidence of Go's production practice
rather than on any official recommendation.** For us the intermediate form is cheaper than
a full CPS rewrite: the `continuation` module already exists
(`crates/neovm-core/src/emacs_core/runtime/eval/continuation/`), and moving more of the
evaluator's recursion into it converts native frames into heap objects, which relieves
*both* limits simultaneously. That is the only measure in this document with that property.

### 5.4 memory64 / wasm64 — irrelevant here

Finished (Phase 5, merged for spec 3.0, per `finished-proposals.md`) and shipped in V8
(`memory64` is in the always-enabled list in `wasm-features.h`). It raises the addressable
limit from 4 GiB — "4GB is the largest amount of memory possible with 32-bit pointers,
which is what WebAssembly currently supports" — at the cost that "pointers take twice as
much memory" (<https://v8.dev/blog/4gb-wasm-memory>, V8 blog, secondary).

None of that bears on us: a 1 MiB-to-16 MiB stack region is nowhere near the 32-bit
ceiling, so 64-bit addressing buys no stack headroom. I found no official material
connecting memory64 to stack sizing, which is itself the answer.

### 5.5 Asyncify vs JSPI — already settled in JSPI's favour

Emscripten documents both and frames the difference as mechanism and code size: Asyncify
"automatically transforms your compiled code into a form that can be paused and resumed …
This works in most environments, but can cause the Wasm output to be much larger", whereas
JSPI "Uses the VM's support for JavaScript Promise Integration (JSPI) … The code size will
remain the same." Source:
<https://github.com/emscripten-core/emscripten/blob/main/site/source/docs/porting/asyncify.rst>.

Asyncify also introduces a *third* stack to size and overflow — `ASYNCIFY_STACK_SIZE`, "the
region used to store unwind/rewind info. This must be large enough to store the call stack
and locals. If it is too small, you will see a wasm trap due to executing an 'unreachable'
instruction", default **4096** bytes — and its own reentrancy rule: "It is *not* safe to
start an async operation while another is already running." We are already on JSPI, which
is Phase 5 and unflagged in V8; there is no reason to revisit this.

---

## 6. Prior art: what shipping interpreters actually do

The striking thing is the unanimity on the immediate question. Every interpreter that
compiles *to* the LLVM shadow-stack convention raises the stack far above 1 MiB, and the one
that does not use that convention (Go) built its own growable stacks instead.

| Runtime | Shadow-stack size | Source |
| --- | --- | --- |
| rustc default (us) | **1 MiB** | `rustc_target/src/spec/base/wasm.rs` |
| wasm-ld default | 64 KiB | `lld/wasm/Driver.cpp` + `Wasm.h` |
| .NET / Blazor | **5 MB** (default) | `dotnet/runtime` `WasmApp.Common.targets` |
| Pyodide / CPython | **10 MB** | `pyodide` `Makefile.envs` |
| CRuby (ruby.wasm) | **16 MiB** (documented remedy) | `ruby/ruby` `wasm/README.md` |
| Go | n/a — own growable stacks in linear memory | `cmd/compile/internal/wasm/ssa.go` |

**CRuby** is the closest match to our symptom, and its README names the exact error we see:

> If you got `Out of bounds memory access` while running the produced ruby, you may need to
> increase the maximum size of stack.
>
> ```console
> $ ./configure LDFLAGS="-Xlinker -zstack-size=16777216" \
>   --host wasm32-unknown-wasi \
> ```

Source: <https://github.com/ruby/ruby/blob/master/wasm/README.md>. That is a mature
interpreter treating shadow-stack exhaustion as a build-configuration matter, with a 16×
increase over our current value, and documenting the out-of-bounds trap as its signature.

**Pyodide** ships `-s STACK_SIZE=10MB` unconditionally in its main-module link flags
(`Makefile.envs`, `MAIN_MODULE_LDFLAGS_COMMON`, alongside `INITIAL_MEMORY=31457280` and
`ALLOW_MEMORY_GROWTH=1`). Source:
<https://github.com/pyodide/pyodide/blob/main/Makefile.envs>.

Pyodide is also the clearest case of a project that hit the *other* wall and documented it.
Its remaining stack-depth limit is the engine stack: the official Pyodide blog reports that
"In Chromium, the stack is 984 kilobytes", that v0.16 had "only enough stack space for a
call depth of 120", and that v0.19 reached the CPython default recursion limit of 1000 by
shrinking CPython's *frame size* rather than by enlarging any stack —
"Although the JavaScript stack limit in web browsers cannot be changed, the stack frame
size of CPython can be optimized". Source:
<https://blog.pyodide.org/posts/function-pointer-cast-handling/> (project blog — official
but secondary). The 984 KB figure independently corroborates
`V8_DEFAULT_STACK_SIZE_KB = 984` from V8's source (§3), which is a satisfying cross-check
of two unrelated sources.

Their open issue <https://github.com/pyodide/pyodide/issues/5987> ("Stack Management
Issue(s?)", opened 2025-11-05, still open as of this check) shows how nasty the engine-stack
side is in practice: with `sys.setrecursionlimit(3010)` and a 3000-deep recursion, the
recursion *succeeds*, then a `RangeError: Maximum call stack size exceeded` fires during
unwinding and the following statement never runs; and a second run of the same code
misbehaves differently from the first. This is a directly relevant warning for us — with
10 MB of shadow stack they are nowhere near the linear-memory limit, and the failures are
engine-stack failures that survive across `runPython` calls.

**.NET** exposes the knob as an MSBuild property, documented in
`src/mono/wasm/build/WasmApp.Common.targets:67-68`:

```
      - $(EmccStackSize)                    - Stack size. Default value: 5MB.
                                              Corresponds to `-s STACK_SIZE=...` emcc arg.
```

Source: <https://github.com/dotnet/runtime/blob/main/src/mono/wasm/build/WasmApp.Common.targets>.

**Go** is the outlier that proves the long-term point (§5.3): by keeping goroutine stacks in
linear memory under runtime control with `morestack`, it has neither a fixed shadow-stack
ceiling nor a dependence on the engine stack for Go frames. It is the only one of these
runtimes that solved the problem rather than sizing around it.

**What nobody documents:** I found no official guidance from any of these projects on how to
*detect* impending shadow-stack exhaustion from inside the guest and turn it into a clean
language-level error. Pyodide's approach is to keep CPython's own frame-count limit and try
to make frames small; CRuby's and .NET's is to make the stack big. Our requirement —
`excessive-lisp-nesting` as a catchable Lisp error at a byte-calibrated depth — appears to
be unaddressed prior art, which is consistent with §2: there is no standard pre-trap
detection mechanism to build it on, only `wasm-opt --stack-check` and hand-rolled probes.

---

## What the official material SAYS vs what I am inferring

### Stated in primary sources

- Declared locals/operands/frames live on an engine-managed "protected call stack";
  address-taken locals and `struct`-by-value live in "a separate user-addressable stack in
  linear memory" (design/`Security.md`).
- The linear-memory stack is a compiler construct, indistinguishable from other memory to
  the engine (design/`Rationale.md`); `__stack_pointer` is defined in tool-conventions,
  not the spec.
- Engine-stack exhaustion is classified as nondeterministic resource exhaustion, and that
  stack "isn't located in the program-accessible linear memory" (design/`Nondeterminism.md`).
- There is no portable metric for stack size, "not even approximate, not even for a single
  engine" (spec editor, stack-switching#156).
- `--stack-first` exists so that "stack overflow traps immediately rather than overwriting
  global data", at a code-size cost (lld `Writer.cpp`); it is the lld default as of
  LLVM 22 (`Options.td`, `Driver.cpp`, PR #166998).
- wasm-ld's default `-z stack-size` is `WasmDefaultPageSize` = 65536, unchanged; rustc
  overrides it to 1048576 and also passes `--stack-first` (rustc `base/wasm.rs`).
- WebAssembly has "no dynamic stack overflow checks"; stack probes are not implemented in
  LLVM for WebAssembly; sanitizers are unavailable (llvm-project#151015).
- Emscripten's `STACK_OVERFLOW_CHECK=1` is a stack cookie checked at end of tick and exit;
  `=2` adds a Binaryen pass checking all stack-pointer assignments (Emscripten settings
  reference, `runtime_stack_check.js`).
- Binaryen's `StackCheck` pass instruments "all assignments to the `__stack_pointer` global
  that LLVM uses for its shadow stack", imports a handler and exports `__set_stack_limits`
  (`StackCheck.cpp`).
- JSPI runs wasm on a secondary stack; its size is `--wasm-stack-switching-stack-size`,
  defaulting to `V8_DEFAULT_STACK_SIZE_KB` (984 on x64); growable JSPI stacks are an
  experimental, off-by-default V8 feature (V8 `flag-definitions.h`, `globals.h`,
  `feature-flags.h`).
- Chromium limits V8's stack in workers to 500 KiB (492 KiB on 32-bit Windows)
  (Blink `v8_initializer.cc`).
- JSPI is Phase 5, shipped unflagged in V8, available in Chrome 137 / Firefox 139.
- Wasm code "has no way to catch these conditions"; traps surface as `RuntimeError`; and
  "traps do not, in general, prevent future execution" (JS-API spec §7).
- `try_table` does not catch traps, and does not catch stack overflow or OOM even when they
  arrive as JS exceptions (exception-handling `Exceptions.md`).
- The stack-switching Explainer never mentions memory; Emscripten's fiber API requires the
  application to supply a C stack *and* an Asyncify stack per fiber.
- Memory control (guard pages) is Phase 1 (`proposals/README.md`).
- Production stack sizes: .NET 5 MB, Pyodide 10 MB, CRuby 16 MiB documented remedy; Go keeps
  goroutine stacks in linear memory with `morestack`.
- `psm` supports wasm32 with `canswitch = true` and a prebuilt object that switches
  `__stack_pointer`; `stacker` routes wasm32 to an allocator-backed stack guard with no
  guard pages; `guess_os_stack_limit()` is `None` on wasm.

### My inference, not stated anywhere

- **That the 400-level Worker failure may be engine-stack-bound rather than
  shadow-stack-bound.** This follows from arithmetic over the 500 KiB Worker limit and is
  the single assumption most worth testing before acting.
- **That concurrently-suspended JSPI computations share the 1 MiB shadow-stack budget
  without clobbering each other.** Reasoned from the fact that the JSPI spec algorithms do
  not touch module globals and that each frame caches its stack pointer in a local. Not
  stated by any source; worth an experiment.
- **That Blink's 500 KiB worker `SetStackLimit` does not apply to the JSPI secondary stack**,
  which is separately allocated and sized by `--wasm-stack-switching-stack-size`. Plausible
  from the code structure, unverified.
- **That `wasm-opt --stack-check` will work on our plain `wasm32-unknown-unknown` artifact.**
  The pass keys only off the `__stack_pointer` global, which our module has, so this should
  hold — but no document promises it for non-Emscripten modules, and the per-assignment
  cost on an interpreter is unmeasured.
- **That enabling `stacker` on wasm is net-positive.** The mechanism is verified; the
  judgement that segment allocation cost and the absence of guard pages are acceptable is
  mine.
- **That instance-level recovery after a shadow-stack trap is unsafe for us.** The spec
  permits calling back in; the claim that *our* invariants cannot be re-established is a
  statement about this codebase.
- **The read that `--stack-first`'s trapping is `i32` wraparound rather than a guard
  region.** Consistent with `placeStack()` starting at `memoryPtr = 0` and with rustc's
  "guaranteed to trap as it underflows", but no source spells out the mechanism.
- **That deepening the existing `continuation` module is the cheapest path to the Go-style
  answer.** Entirely a judgement about our architecture.

---

## Could not verify from a primary source

1. **Chrome's actual per-Worker wasm stack budget end to end.** I have Blink's
   `kWorkerMaxStackSize = 500 * 1024` and V8's `V8_DEFAULT_STACK_SIZE_KB = 984`, but not a
   document reconciling them, nor confirmation of which applies to a JSPI'd export running
   in a module Worker. The chromium.googlesource.com raw endpoint failed for me; I read the
   file through the GitHub mirror, so line numbers may drift from tip-of-tree.
2. **Whether V8 saves/restores `__stack_pointer` across a JSPI suspension.** Neither the
   proposal, the V8 blog, nor the V8 flags say. I did not find the relevant V8
   stack-switching implementation files.
3. **Whether the JSPI secondary stack's exhaustion reports as `RangeError` or as something
   else.** No source found; the JS-API spec only says the class is implementation-defined
   and notes "implementations have been observed to throw `RangeError`, `InternalError` or
   `Error`."
4. **The overhead of `wasm-opt --stack-check` on an interpreter workload.** Emscripten says
   "a small performance cost" for `STACK_OVERFLOW_CHECK=2`; no numbers, and none for
   interpreter-shaped code.
5. **Whether `psm`'s prebuilt `wasm32.o` links cleanly against LLVM 22.1.2.** The object
   was assembled by an external `llvm-mc` of unstated version. This needs a build, which was
   out of scope.
6. **Any official WebAssembly guidance for interpreters on wasm.** I found no spec, design
   doc, or WG document addressing interpreter stack strategy at all. The absence is itself a
   finding: the §5.3 recommendation rests on Go's practice, not on standards guidance.
7. **A primary design document for Guile Hoot's stack strategy.** Only the implementer's
   blog and the project page, both secondary.
8. **Emscripten's `STACK_OVERFLOW_CHECK` default.** The settings reference is
   self-contradictory: the prose says "Building with ASSERTIONS=1 causes
   STACK_OVERFLOW_CHECK default to 1 […] this setting also effectively defaults to 1,
   absent any other settings", while the machine-generated "Default value:" line beneath it
   reads 0. I did not resolve which governs.
9. **Whether `__stack_low`/`__stack_high` can be referenced from Rust without a build
   script.** lld defines them on reference (`addOptionalDataSymbol`), and they are absent
   from our artifact as expected, but I did not verify the Rust-side `extern` declaration
   that would pull them in.
