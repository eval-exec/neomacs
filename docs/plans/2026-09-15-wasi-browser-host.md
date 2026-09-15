# WASI-first evaluator host for neomacs-wasm

Status: initial browser experiment complete; production migration is **not**
implemented. The current browser build and native frontends are unchanged.

## Objective and constraints

Use WASI as broadly as its verified behavior permits for the evaluator's system
services: filesystem, clocks, randomness, environment, streams and HTTP. Replace
the corresponding custom imports when the standard implementation earns its
keep. Retain Neomacs-specific interfaces for startup assets, input, IME and frame
publication, and retain the current Rust renderer. `Context` remains confined to
the evaluator worker; a storage or network worker never owns Lisp values.

Default desktop Chrome and Firefox are deployment requirements. Installing an
extension or enabling a WebAssembly feature flag is not part of this design.
Required deployment headers for a SharedArrayBuffer fallback are a website
configuration concern, not permission to require browser preference changes.
`HOME` remains `/neomacs-fake`.

## Measured starting point

On 2026-09-15, a disposable Rust standard-library program was compiled using
Rust 1.96.1's `wasm32-wasip2` target and transpiled by Jco 1.34.0. The browser host
used published `@bytecodealliance/preview2-shim` 0.25.0, including its OPFS adapter.
Chrome 149.0.7827.200 and Firefox 152.0.4 were exercised headlessly using dedicated
profiles and their installed binaries, with no experimental WebAssembly flags.
The pages were not cross-origin isolated. This was a host-service probe, not a
running Neomacs evaluator.

| Observation | Chrome | Firefox |
| --- | --- | --- |
| Read a packaged Lisp file through `std::fs` | Passed | Passed |
| `HOME=/neomacs-fake` through `std::env` | Passed | Passed |
| Create directories, write/read UTF-8, seek, rename over an existing file, enumerate directory | Passed | Passed |
| Report existing-file and missing-file errors | Passed | Passed |
| Read saved data after explicit adapter flush and a full browser restart | Passed | Passed |
| Reject writes to a package mounted with the generic memory adapter | Not enforced | Not enforced |
| Data visible in OPFS after `sync_all`, before the guest returns | Failed | Failed |
| 200 ms sleep with synchronous Jco bindings and Promise-backed poll | Returned in about 0.5 ms | Returned in 0 ms |
| JSPI exposed by the browser | Yes | No |
| 200 ms sleep with Jco's JSPI/async-WASI bindings | About 200.9 ms | Not applicable: JSPI absent |

The clock probe exercised wall and monotonic reads. Constructing Rust's randomized
HashMap state did not trap; this is not a cryptographic-randomness conformance
test. HTTP, cancellation, quota exhaustion, cross-tab coordination and full
Elisp startup have not been tested with this host.

Separately, `cargo check -p neomacs-wasm-worker --target wasm32-wasip2 --locked`
passed. This only proves typechecking: the existing worker still imports
`neomacs_host`, and no component packaging or full worker startup was validated.

## What the failing observations mean

### Persistence must be acknowledged at the right point

The published OPFS adapter delegates guest operations to an in-memory tree.
Its descriptor `sync` and `syncData` methods do not persist that tree; mutation
notifications schedule an asynchronous flush in a microtask. In the probe, Rust
reported successful `sync_all`, then continued executing for 1.5 seconds. During
that interval, a separate browser task could not find the new file in OPFS.
Explicitly awaiting the adapter's flush after the guest returned allowed data
to survive restart.

This is unsuitable as an implicit persistence guarantee for a long-lived
synchronous editor command loop. A durable operation must not report successful
completion solely because an in-memory mutation succeeded. Preserve the ability
to distinguish buffered completion from an explicitly requested storage flush,
and deliver storage failures through the normal filesystem result.

GNU reference: `src/fileio.c`'s `write_region` handles `fsync` failures when
`write-region-inhibit-fsync` is nil. GNU defaults that variable to true; this is
not a claim that every ordinary GNU save requests a physical-device flush.
The browser requirement is an honest completion contract for the storage
operation requested, not a claim to stronger guarantees than OPFS provides.

### Asynchronous imports require actual suspension

The browser poll implementation returns Promises. Plain synchronous generated
bindings do not make a synchronous Rust guest await them. Enabling Jco's
`--async-mode jspi --async-wasi-imports --async-wasi-exports` made the sleep
work correctly in default Chrome. That configuration cannot be the sole path
while supported Firefox lacks JSPI.

The existing JSPI/worker-message/Atomics coordination is therefore still useful.
Reuse its ownership and wakeup model behind WASI interfaces rather than removing
it merely because the imports have standard names. Independently verify the
fallback; this experiment did not implement a WASI Atomics fallback.

### Package policy remains ours

The generic in-memory adapter used in the probe accepted mutation of the
packaged file. That demonstrates missing mount policy in the supplied probe,
not that WASI requires writable packages. Keep the existing immutable runtime
resource semantics, or explicitly enforce equivalent descriptor capabilities
before exposing packaged files through a WASI host.

## Intended module shape

```text
Shared Elisp VM and editor filesystem semantics
                    │
       target-selected system implementation
          ┌─────────┴──────────┐
          │                    │
   Native std/OS          WASI interfaces
                              │
                  Browser WASI host adapter
                     ┌────────┴─────────┐
                     │                  │
               Browser services   Storage/network workers
                                      │
                                 OPFS / Fetch
```

- Keep target selection at the existing host modules, using `cfg_select!` and
  distinguishing `target_os = "wasi"` from bare browser Wasm where necessary.
  Do not add runtime host parameters throughout the evaluator or capability
  features that merely restate the compilation target.
- Keep editor filesystem semantics at `EditorFileSystem`; prefer a reusable
  standard-library implementation for native and WASI where their behavior
  agrees. Do not pretend that Unix-only metadata calls work on WASI.
- Put evaluator-specific browser/WASI composition under the worker crate. Put
  JavaScript WASI implementations, generated-bindings integration and capability
  setup under a dedicated `crates/neomacs-wasm/web/wasi/` directory when adopted.
  Keep storage persistence and waiting implementations separate from loading the
  component. Do not introduce empty scaffolding before each slice is implemented.
- Retain the renderer/frontend artifact; experiment with the evaluator artifact's
  target and component export contract independently.

## Incremental migration gates

1. Make the observed persistence and wait failures permanent red conformance
   cases at the Rust-through-WASI browser interface. Enforce immutable package
   mounts. Implement acknowledged OPFS operations and verify both JSPI and
   non-JSPI paths without replacing live editor storage prematurely.
2. Establish the worker component's import/export contract and packaging. Prove
   startup, input waiting and clean shutdown with `Context` confined to its worker.
3. Select standard WASI-backed clocks, environment and randomness. Check elapsed
   time and timers, environment isolation, and the actual randomness interface;
   do not use a HashMap-construction probe as the final entropy test.
4. Route editor filesystem operations through the verified host. Reuse the same
   guest code on native and WASI where possible. Exercise save/reopen, themes,
   Dired, package loading, rename, quotas, persistence errors and worker shutdown.
5. Adopt `wasi:http` with existing request ownership, cancellation and completion
   behavior. CORS still applies. Raw TCP/SSH remain unsupported by an ordinary
   browser unless a separate transport is provided; WASI must report this honestly.
6. Remove superseded custom system imports after their replacements pass the
   browser and Elisp-level checks. Keep Neomacs-specific frame/input interfaces.

The user's direction is WASI-first, not an immediate switch to the off-the-shelf
adapter with the above unresolved behavior. No production dependencies were
changed for this experiment.

## Evidence and reproduction

Local disposable probe: `tmp/neomacs-wasi-host-prototype/` (`npm run probe`; use
`PROBE_JSPI=1 npm run probe` for the Chrome async-binding comparison). Preserve
its observations until the permanent conformance cases exist, then remove the
throwaway runner. Logs: `tmp/wasi-prototype-run-5.log`,
`tmp/wasi-prototype-run-jspi.log`, and `tmp/wasi-worker-target-check.log`.

- [Rust WASIp2 target](https://doc.rust-lang.org/rustc/platform-support/wasm32-wasip2.html)
- [Jco transpilation and browser imports](https://bytecodealliance.github.io/jco/transpiling.html)
- [WASI browser shim](https://github.com/bytecodealliance/jco/tree/main/packages/preview2-shim)
- [GNU write-region implementation](https://git.savannah.gnu.org/cgit/emacs.git/tree/src/fileio.c)

Implementation observations above were checked against the installed npm 0.25.0
files, not inferred solely from documentation on a moving upstream main branch.
