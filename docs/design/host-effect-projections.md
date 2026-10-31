# Owner-selected host effects

Primitive bodies run on their caller, through any interpreter, VM, checked
callback, JIT or AOT route. Native effects are mediated at host-effect owners,
not by rewriting primitive tables or canonical SubrObj identities.

The direct projection belongs to evaluator-host main: the GUI evaluator OS
thread, or process main for TUI. Winit/AppKit objects remain on their existing
process-main render owner. Hosted Green workers will receive routed projections
at cold activation. There is no production activation in the contracts range;
`GreenStartError::ThreadAffineHost` remains enforced.

## Contracts and privacy

`neovm-core::host_effect::contracts` defines dormant closed request families,
owned native data and opaque registered-record references. No production
constructor can mint HostAffinity, a registered record or a live session.
HostAffinity is confined and neither Send, Sync, Clone nor Copy. The isolated
contract fixture lends a scoped token; it cannot attach a host or register roots.

Reply ports are linear and family-typed. Terminal settlement validates the
session/request identity of returned records, errors and signals. A rejected
reply retains both the original port and the payload. Terminal settlement is not
owner acknowledgement or root retirement. Root registration, heap retention,
transport, cancellation and host service are later implementation stages.

The native owner family includes core display/terminal effects, concrete
frontend GUI/TUI/image/redisplay owners, native display-runtime backend owners,
and cold startup/teardown. Ordinary Lisp primitive modules consume projection
traits and owned protocol data. They must not obtain a raw native sender,
window/proxy, clipboard backend or native runtime TLS slot.

RenderCommandSender keeps its raw channel private. RenderEventLoop and its wake
proxy expose owned lifecycle/protocol operations without a raw getter, Deref or
conversion. The TTY native sender and redisplay runtime TLS slot are private.
Existing font/image/clipboard services keep their native backend fields private;
their public opaque facades are distinct from raw backend handles. Architecture
source checks supplement Rust field/module privacy. They are not a claim of
whole-program Rust name or dataflow resolution.

## Existing storage and later activation obligations

Locations in this table describe the pre-activation source at commit
06046bed97eccc9caaef0cd4ccf6d0deacbca110. Privacy-only edits can shift lines;
owner/type names identify the same storage.

| Projection or data | Tracked source under crates/ | Storage and selection |
|---|---|---|
| DisplayHost | neovm-core/src/emacs_core/runtime/eval/mod.rs:3378 | Existing per-Context boxed trait slot; select direct/routed at activation |
| Redisplay, prepare, shaping, scroll, maintenance, snapshot callbacks | neovm-core/src/emacs_core/runtime/eval/mod.rs:3327-3377 | Existing per-Context callback boxes; routed steps must never move worker Lisp onto main |
| Layout query | neovm-core/src/emacs_core/runtime/eval/mod.rs:3016 | Existing per-Context WindowLayoutQueryAdapter |
| Secondary TTY opener | neovm-core/src/emacs_core/runtime/eval/mod.rs:3382 | Existing per-Context factory box |
| Input connection | neovm-core/src/emacs_core/runtime/eval/mod.rs:3031; src/host_input.rs:17 | Per-Context fixed receiver type, not a HostClient slot; later input registration must retain main consumption ownership |
| GUI native adapter | neomacs/src/main.rs:1259 | Private PrimaryWindowDisplayHost with native font/image/media state |
| Redisplay runtime | neomacs/src/frame_layout.rs:38 | Existing pointer-sized TLS handle to a boxed native runtime; retain on evaluator main |
| Native UI loop | neomacs-display-runtime/src/render_thread/state.rs:884; render_thread/bootstrap.rs:614 | Process-main native owner; reached by owned render commands |
| Native terminal backing | neovm-core/src/emacs_core/display/terminal/pure.rs:23,419 | TerminalManager TLS contains native host boxes; currently carried as one World value |
| Terminal carrier | neovm-core/src/emacs_core/runtime/eval/green/carried.rs:58; display/terminal/pure.rs:1771 | Existing exchange moves the entire manager; this is not already a per-owner direct/routed selector |
| Selection fallback and monitor data | neovm-core/src/emacs_core/lisp/native/builtins/stubs.rs:331,1598 | World-carried owned data; distinct from native clipboard connection |
| Warm carried state | neovm-core/src/emacs_core/runtime/eval/green/owner_carried.rs:21 | Profiler/hooks/dirty state; does not contain a host projection selector |
| Face, charset and terminal Lisp registries | neovm-core/src/emacs_core/runtime/eval/mod.rs:3239 | Context-selected HeapRegistrySlots; semantic World data, not native backend owners |

Before hosted activation, terminal native backing must remain on main instead
of travelling in the carried manager to worker TLS. A peer-approved cold
exchange/activation hook must select the projection using existing storage;
there is no new TLS word, inline Context/World field or hot owner check in this
range. Initial-terminal stdout and destructor effects must be included. Input
registration and mixed redisplay/selection OwnerStep integration also remain
later-stage obligations. The dormant contracts do not claim these are solved.

## Binding-publication guard

Op::Set, VarSet, VarBind, Unbind, interpreter binding/unwind, owner-mirror
restoration and the JIT variable shims remain unchanged. Their native effects
must terminate in the owner's activation-selected projection. In particular,
Context::sync_cached_runtime_binding_by_id invokes DisplayHost's alpha-limit
setter; no compiled-opcode exclusion or primitive-name allowlist replaces that
projection obligation.

A setter with no GNU Lisp-visible result/error may enqueue one session-FIFO
command without parking. It must precede later synchronous requests; a delivery
failure becomes HostClosure at the next synchronous request. Operations that
can return a value or signal in GNU remain synchronous. Service-only hops retain
the requester's logical run slice; only real GNU release points change owner
eligibility. Main service performs owned native work only. Request-induced Lisp
runs on its requesting worker through owned resumable steps; unsolicited native
callbacks retain main ownership.
