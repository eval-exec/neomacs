# `neomacs-webview`: long-term architecture

Status: implemented foundation; policy/permission and recovery extensions remain

This document defines the intended boundary for Neomacs web content on Linux,
macOS, and Windows. The supporting platform research is recorded in
[`../research/2026-08-31-neomacs-webview-platform-constraints.md`](../research/2026-08-31-neomacs-webview-platform-constraints.md).

## Decision

Create one crate named `neomacs-webview`.

It will be a platform-erased, frontend-thread-affine service that owns:

- browser profiles, browser instances, and their asynchronous lifecycle;
- navigation, script evaluation, policy decisions, and normalized events;
- attachment of a browser instance to the active Neomacs presentation;
- platform input forwarding, focus, cursor, accessibility, and IME integration;
- Linux WPE buffer ownership, import, synchronization, and GPU recovery;
- macOS WKWebView and Windows WebView2 native/composition object lifetimes.

It will not own:

- GNU Emacs xwidget Lisp objects or compatibility semantics;
- redisplay, window layout, or the choice of which xwidget occurrence is active;
- the wgpu renderer or its pipelines;
- a second cross-thread command channel parallel to Neomacs' existing display
  communication.

The important abstraction is not "a browser that returns a texture." It is:

> A frontend-thread-affine web-content service that owns browser state and
> synchronizes one active presentation through the platform's native
> presentation mechanism.

That definition admits all three real backends without making callers branch
on WebKit, WKWebView, WebView2, DMA-BUF, NSView, HWND, or DirectComposition.

## Why this must be a deep module

Moving the existing `backend/wpe`, `backend/wkwebview`, and WebKit texture cache
into another directory would only move complexity. The new crate is worthwhile
only if it removes these decisions from its callers:

- whether a view is a wgpu texture, AppKit view, or DirectComposition visual;
- which thread or event context owns native objects;
- how creation races with window realization or destruction;
- how foreign frame buffers and fences are retained and released;
- how coordinates, clipping, scale, input, and focus reach each engine;
- how asynchronous callbacks are correlated and stale callbacks discarded;
- how permissions, new windows, downloads, and process failures are normalized.

The crate should expose a small control surface and hide a large implementation.

## Lessons retained from GNU Emacs

GNU Emacs separates `struct xwidget`, the browser model, from
`struct xwidget_view`, one redisplay occurrence with geometry, clip edges,
visibility, and a touched bit. Redisplay hides untouched views. That is the
right conceptual split and should remain visible in Neomacs' domain model.

GNU's implementations also show why the rendering mechanism must not define
the public abstraction:

- GTK creates an offscreen WebKit widget and copies its surface into a view.
- The macOS implementation uses a live native WKWebView and explicitly notes
  that only one view can be associated with one model.
- Both deliver JavaScript completion asynchronously through the Emacs event
  machinery rather than synchronously calling Lisp from a browser callback.

Neomacs should therefore distinguish:

1. an evaluator-owned **xwidget model**;
2. a `neomacs-webview`-owned **browser instance**;
3. a presentation-local **occurrence** selected by redisplay;
4. a platform-owned **attachment** of that browser instance to one host window.

The names are deliberately different. In particular, `XwidgetId` and
`WebViewId` must remain distinct types. Their current equal numeric values must
not be an invariant.

## Platform truth

The common interface must preserve, rather than obscure, these constraints.

### Linux

WPE produces buffers which the application composites. Linux is therefore the
only backend that naturally contributes browser draws to the wgpu render pass.
The crate owns DMA-BUF or shared-memory frame leases, import policy, fences,
texture bindings, and release acknowledgements.

The backend is WPEPlatform with its headless platform implementation. The old
libwpe/WPEBackend-fdo adapter has been removed rather than retained as a second
behavioral path.

### macOS

WKWebView is a live `NSView` and must be managed on the AppKit main thread. It
is placed inside an explicitly clipping native container view. Snapshot APIs
are useful for tests or thumbnails, not as an interactive rendering path.

### Windows

The chosen backend is `ICoreWebView2CompositionController`, with no child-HWND
fallback. It contributes a WebView2 visual subtree to a DirectComposition
target. Neomacs owns hit testing and forwards spatial input. COM objects are
created and used on the frontend thread's STA with a live message pump.

The implemented adapter creates one shared DirectComposition device and a
topmost HWND target/root per registered host. Each WebView contributes a
clipped child visual and may migrate between host roots without recreating its
browser. The complete WebView plane overlays the HWND-backed wgpu swapchain;
the two do not yet share a DirectComposition tree. If arbitrary interleaving is
needed later, the display runtime must own a common tree and create the wgpu
surface from `SurfaceTargetUnsafe::CompositionVisual`.

### Common presentation guarantee

The portable guarantee is one clipped **embedded web plane** per visible web
view. It can be positioned among editor windows, but it is not an arbitrary
scene-graph node on macOS or Windows. Linux may have stronger composition
capabilities internally; portable behavior must not depend on them.

## Dependency direction

```text
neovm-core
  owns GNU xwidget objects and maps XwidgetId -> WebViewId
          |
          v
neomacs-display-runtime
  owns the event loop, active presentation, host presentation capability,
  and one WebViewSystem
          |
          +--------------------------+
          v                          v
neomacs-webview              neomacs-renderer-wgpu
  owns web state       <----- consumes PreparedWebViewDraws on Linux
          |
          v
neomacs-display-protocol
  owns IDs, presentation identity, and canonical geometry types
```

Allowed dependencies for `neomacs-webview` are:

- `neomacs-display-protocol` for strong IDs and presentation geometry;
- `winit` for a safely retained host-window lifetime;
- target-specific browser and native-window bindings.

It must not depend on `neovm-core`, `neomacs-layout-engine`,
`neomacs-display-runtime`, or `neomacs-renderer-wgpu`.

The renderer may depend on `neomacs-webview` only for owned Linux frame
handoffs, as it already does with `neomacs-video`. Native platform types never
cross this edge.

## Public facade

The public API should be concrete, not a public backend trait:

```rust
pub struct WebViewSystem {
    inner: WebViewSystemImpl<platform::CurrentPlatform>,
    _thread_affine: PhantomData<Rc<()>>,
}

impl WebViewSystem {
    pub fn new(
        config: WebViewSystemConfig,
        wake: WebViewWake,
    ) -> Result<Self, WebViewInitError>;

    pub fn register_host(
        &mut self,
        id: HostWindowId,
        host: WebViewHost,
    );

    pub fn unregister_host(&mut self, id: HostWindowId);

    pub fn command(
        &mut self,
        command: WebViewCommand,
    ) -> Result<(), WebViewCommandError>;

    pub fn synchronize_presentation(
        &mut self,
        scene: ResolvedWebViewScene,
    ) -> Result<WebViewPresentationEffects, WebViewPresentationError>;

    pub fn service(&mut self);
    pub fn take_frame(&mut self, id: WebViewId) -> Option<WebViewFrame>;
    pub fn drain_events(&mut self) -> Vec<WebViewEvent>;
}
```

The facade preserves these properties:

- `WebViewSystem` is explicitly `!Send + !Sync`; native objects cannot escape
  their owning frontend thread.
- `WebViewHost` is an owned opaque capability. It retains the
  `Arc<winit::window::Window>` and, on Windows if required, a slot in the
  display-owned DirectComposition tree. It exposes no raw NSView, HWND, or
  `IDCompositionVisual`.
- commands and events contain only owned, `Send` data.
- `take_frame` returns Linux WPE sampling resources and is empty on native
  presentation backends. The caller never switches on the backend.
- `WebViewWake` wakes the existing winit loop. The crate does not introduce a
  competing public proxy or event loop.

Backend information may appear in diagnostics and capability reports, but it
must not control ordinary caller flow.

Keep compile-time assertions next to the facade so these properties cannot
quietly regress:

```rust
assert_not_impl_any!(WebViewSystem: Send, Sync);
assert_impl_all!(WebViewCommand: Send, Sync);
assert_impl_all!(WebViewEvent: Send, Sync);
```

`WebViewHost` retains the winit window. The Windows adapter extracts its HWND
only after a visible presentation arrives and then creates the composition
controller; this host-late transition is encoded as `Creating(PendingCreate)`
rather than a nullable native controller.

## Compile-time platform seam

Follow the proven shape of `neomacs-video`:

```rust
struct WebViewSystemImpl<P: Platform> {
    platform: P,
    views: HashMap<WebViewId, ViewRecord<P>>,
    scenes: HashMap<HostWindowId, ResolvedWebViewScene>,
}

#[cfg(target_os = "linux")]
type CurrentPlatform = LinuxWpePlatform;

#[cfg(target_os = "macos")]
type CurrentPlatform = MacWkWebViewPlatform;

#[cfg(target_os = "windows")]
type CurrentPlatform = WindowsWebView2CompositionPlatform;
```

The `Platform` trait and all associated native types are private. A
`FakePlatform` implements the same private contract for state-machine tests.
Target `cfg` blocks select `CurrentPlatform` once; they should not be spread
through command dispatch, runtime state, renderer code, and evaluator code.

## Strong identities

Add or retain distinct newtypes for:

```rust
WebViewId
XwidgetId
WebProfileId
HostWindowId
WebViewGeneration
WebViewOccurrenceId
ScriptRequestId
NavigationId
PolicyDecisionId
PresentationId
GpuGeneration
```

Do not implement broad conversions between semantic IDs. In particular:

```rust
struct XwidgetRecord {
    webview: WebViewId,
    // GNU-compatible evaluator state...
}
```

is preferable to deriving `WebViewId` from the raw `XwidgetId` integer.

Generations qualify asynchronous callbacks. Request IDs correlate individual
operations. Presentation IDs qualify geometry and input. These solve three
different stale-data problems and must not be collapsed into one counter.

## Browser profiles and creation

Storage/process sharing is part of browser creation, not a later setter:

```rust
pub enum StoragePartition {
    Persistent(WebProfileId),
    Ephemeral(WebProfileId),
}

pub enum BrowsingRelationship {
    Independent,
    Related(WebViewId),
}

pub struct WebViewCreate {
    pub id: WebViewId,
    pub storage: StoragePartition,
    pub relationship: BrowsingRelationship,
    pub initial_size: WebContentSize,
    pub policy: WebViewPolicy,
    pub initial_navigation: Option<NavigationTarget>,
}
```

`Related(WebViewId)` expresses GNU's related-xwidget behavior without passing
a native browser pointer. A missing or incompatible related view is a typed
creation error.

The system config owns the root directory and policy for persistent profiles.
Callers select typed profile identities, not arbitrary platform paths.

## Lifecycle state machine

Native creation is asynchronous and can complete after the host, view, or GPU
generation has changed. Encode the legal states:

```rust
enum WebViewLifecycle<P: Platform> {
    Waiting {
        generation: WebViewGeneration,
        desired: DesiredWebView,
        prerequisites: MissingPrerequisites,
    },
    Creating {
        generation: WebViewGeneration,
        desired: DesiredWebView,
        create: P::PendingCreate,
    },
    Ready {
        generation: WebViewGeneration,
        desired: DesiredWebView,
        native: P::View,
    },
    Failed {
        generation: WebViewGeneration,
        desired: DesiredWebView,
        failure: WebViewFailure,
    },
    Closing {
        generation: WebViewGeneration,
        close: P::PendingClose,
    },
}
```

`MissingPrerequisites` is itself a typed set over `Profile`, `Host`, and `Gpu`.
Linux and macOS need not wait for a host merely because Windows composition
creation does. The state machine asks the selected private platform for its
real prerequisites.

There is no `HashMap<WebViewId, Option<NativeView>>` and no collection of
parallel booleans such as `created`, `visible`, and `destroying`.

Every native callback carries `(WebViewId, WebViewGeneration)`. A callback for
a replaced generation cannot mutate the new instance. Destruction cancels all
outstanding requests and makes late callbacks harmless.

### Commands before readiness

Do not preserve an arbitrary FIFO of native commands while waiting for a host.
Classify commands by semantics:

- convergent state, such as model size, latest navigation target, zoom, and
  desired focus, updates `DesiredWebView` and coalesces;
- history operations which require a ready page fail with `NotReady`;
- script and policy requests are bounded, individually identified, and receive
  a typed cancellation or rejection if they cannot run;
- close supersedes all pending work.

This prevents the current "create arrives before primary window" race without
replaying half of an obsolete lifecycle.

## Commands and events

The runtime communication collapses the many `WebKit*` variants into one
domain command and one domain event:

```rust
enum AssetCommand {
    // ...
    WebView(WebViewCommand),
}

enum InputEvent {
    // ...
    WebView(WebViewEvent),
}
```

Representative commands are:

```rust
pub enum WebViewCommand {
    Create(WebViewCreate),
    Close { id: WebViewId },
    SetModelSize { id: WebViewId, size: WebContentSize },
    Navigate { id: WebViewId, target: NavigationTarget },
    History { id: WebViewId, action: HistoryAction },
    EvaluateScript(ScriptRequest),
    Focus { id: WebViewId, focus: FocusIntent },
    Input(WebViewInput),
    ResolvePolicy(PolicyResponse),
}
```

Representative events are:

```rust
pub enum WebViewEvent {
    Ready { id: WebViewId, generation: WebViewGeneration },
    Failed { id: WebViewId, failure: WebViewFailure },
    Closed { id: WebViewId, generation: WebViewGeneration },
    NavigationChanged { id: WebViewId, state: NavigationState },
    TitleChanged { id: WebViewId, title: String },
    ScriptFinished {
        request: ScriptRequestId,
        result: Result<WebValue, ScriptError>,
    },
    PolicyRequested(PolicyRequest),
    ProcessFailed { id: WebViewId, failure: ProcessFailure },
    CursorChanged { id: WebViewId, cursor: WebCursor },
    FocusChanged { id: WebViewId, focused: bool },
}
```

The common script result is data, not an Objective-C or COM value:

```rust
pub enum WebValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<WebValue>),
    Object(BTreeMap<String, WebValue>),
}
```

The evaluator converts `WebValue` to Lisp after receiving the event. Native
callbacks only enqueue events and wake the frontend; they never call Lisp or
enter a nested event loop.

## Presentation is a snapshot, not resize/show commands

The active display presentation is already Neomacs' geometry authority. The
webview crate must consume a resolved snapshot derived from it, not maintain a
second layout model.

```rust
pub struct ResolvedWebViewScene {
    host: HostWindowId,
    presentation: PresentationId,
    placements: Box<[ResolvedWebViewPlacement]>,
}

pub struct ResolvedWebViewPlacement {
    view: WebViewId,
    occurrence: WebViewOccurrenceId,
    owner: DisplayWindowId,
    content_rect: RootSurfaceRect,
    visible_rect: RootSurfaceRect,
    content_offset: RootSurfaceVector,
    device_scale: DeviceScale,
}
```

These fields have private constructors. A compiler at the sealed-presentation
boundary validates that:

- every placement belongs to the same `PresentationId` and host;
- rectangles use the expected coordinate space and have finite, positive
  extents;
- `visible_rect` is the already-resolved intersection of all four clip edges;
- `content_offset` agrees with `content_rect` and `visible_rect`;
- a `WebViewId` occurs at most once in the portable presentation.

Absence from a scene means detached/hidden. There is no independently drifting
`visible: bool` command. `synchronize_presentation` diffs the complete scene and
performs attach, detach, reparent, resize, clip, visibility, ordering, and hit
region updates together.

The one-occurrence rule is the portable contract required by native WKWebView.
The GNU compatibility/display layer selects a stable active occurrence. The
webview crate rejects accidental duplicates rather than allowing Linux and
macOS to behave differently.

### Platform realization of one resolved placement

- WPE draws the browser texture into `visible_rect` with source coordinates
  derived from `content_offset`.
- macOS sizes an outer clipping NSView to `visible_rect`, explicitly enables
  clipping, and offsets the inner WKWebView by `content_offset`.
- Windows positions the WebView2 composition subtree, installs a rectangular
  DirectComposition clip, and uses the same offset for input conversion.

General clipping is computed once. Each adapter only converts the resolved
geometry into its coordinate system. AppKit point conversion and WebView2
logical/physical bounds modes remain private platform concerns.

## Input, focus, and activation

Pointer input is qualified by the same presentation used for hit testing:

```rust
pub struct WebViewPointerInput {
    pub presentation: PresentationId,
    pub view: WebViewId,
    pub occurrence: WebViewOccurrenceId,
    pub position: WebContentPoint,
    pub phase: PointerPhase,
    pub device: PointerDevice,
    pub buttons: PointerButtons,
    pub modifiers: Modifiers,
}
```

Stale-presentation input is rejected. The caller never constructs a WPE event
or WebView2 mouse flag.

- WPE and Windows receive host-forwarded pointer, scroll, key, touch, and pen
  input where supported.
- Windows additionally tracks mouse capture, leave synthesis, cursor changes,
  drag/drop, and its UI Automation provider.
- WKWebView normally receives native responder-chain input; its adapter still
  reports activation and focus changes so Neomacs selects the correct editor
  window.

Focus must be explicit state such as `Editor`, `WebView(WebViewId)`, or `None`.
Tab traversal, accelerator arbitration, IME composition, and focus restoration
belong to adapter contract tests, not to best-effort click forwarding.

## Linux frame ownership and GPU integration

The browser crate owns the foreign-frame protocol because only it knows when
WPE permits a buffer acknowledgement. The renderer owns wgpu caching and
submission ordering. Their seam is ownership, not a WPE API: `DmaBufFrame`
carries one opaque, non-cloneable lease into the renderer, and the renderer
accepts it only as an uninspectable `Send + 'static` retained resource.

The two halves deliberately have different thread capabilities:

```rust
// Reactor-local: contains WPE/GObject pointers and is !Send + !Sync.
struct NativeWpeBufferLease { view: NonNull<WPEView>, buffer: NonNull<WPEBuffer> }

// Cross-thread: contains owned descriptors and a typed release command only.
struct DmaBufFrame { planes: Vec<DmaBufPlane>, lease: DmaBufFrameLease }

// Renderer-local: generic over the fence index and ignorant of WPE.
struct SubmissionRetirementQueue<SubmissionIndex> { /* FIFO worker */ }
```

Dropping the cross-thread lease posts `ReleaseFrame(FrameLeaseId)` to the WPE
reactor. Only that reactor can drop `NativeWpeBufferLease` and call
`wpe_view_buffer_rendered` plus `wpe_view_buffer_released`.

`PreparedWebViewDraws<'_>` borrows opaque sampling resources. It exposes no
DMA-BUF, EGL, Vulkan, GLib, WPE, COM, Objective-C, or native-window type.

`neomacs-video` already solves a related external-image problem. Do not create
two unrelated piles of unsafe Vulkan import code indefinitely. First preserve
the WebView-specific lease and release rules inside this crate. Once both real
callers prove the common shape, extract only format/modifier validation,
external-memory import, and GPU-retirement mechanics into a small deep
GPU-import module used by video and webview. Browser-buffer acknowledgement
and video-decoder frame policy remain in their domain crates.

On GPU recreation, `rebind_gpu` advances `GpuGeneration`, retires stale imports,
and retains logical browser/navigation state. WPE imports resume from the next
valid frame. macOS is unaffected; the Windows adapter performs its own
DirectComposition device-loss recovery behind the same service boundary.

WPE integration is owned by a dedicated reactor thread. That thread owns the
thread-default `GMainContext` and every WPE/GObject pointer for their complete
lifetime. It blocks in `g_main_context_iteration`; WebKit's file-descriptor
sources and timers therefore wake at their real readiness/deadline instead of
being sampled at frame cadence.

The UI side sends typed `WpeCommand` values through a queue and calls
`g_main_context_wakeup`. The reactor publishes typed lifecycle events and one
latest negotiated frame per `(WebViewId, WebViewGeneration)`. Publishing wakes
winit through `EventLoopProxy`; idle WebViews cause no periodic wakeup.

Linux resolves `WebViewFrameTransport::Auto` before creating views. The private
resolved enum has only `SoftwarePixels` and `DmaBuf`, so capture code cannot
emit both representations for one buffer. Software pixels are the reliable
automatic selection while wgpu imports external Vulkan images with an
uninitialized tracking layout. Explicit DMA-BUF may fall back to one software
representation when a particular buffer is not exportable.

Neomacs implements custom `WPEDisplay`, `WPEToplevel`, and `WPEView` GObject
subclasses in Rust using `glib::subclass`. The display delegates EGL, DRM,
keymap, clipboard, screen, and explicit-sync capabilities to WPE's headless
display, while its `create_view` vfunc returns the Rust-owned view. No C or C++
shim is part of Neomacs. Returning true from `render_buffer` transfers WPE's
two acknowledgements to one RAII lease, so the stock headless view's 60 Hz
release policy is not part of the lifetime model.

For DMA-BUF, the render thread imports the external texture, records a copy to
a cache-owned texture, and submits it without waiting. It transfers the source
texture, descriptor owner, and complete `DmaBufFrame` into a FIFO submission
retirement worker. That worker waits for the exact `SubmissionIndex`;
retirement drops the cross-thread lease and wakes the WPE reactor with
`ReleaseFrame`. No global GLib source is paused, and neither the reactor nor
render thread blocks on GPU completion. A producer fence is currently probed
without blocking; GPU-side semaphore import is the future synchronization
enhancement.

Neither `Platform` nor `WebViewSystem` exposes a polling deadline. This is an
intentional compile-time boundary: native timers belong to the native reactor,
while rendering deadlines belong to the display scheduler. A permanent 16 ms
tick would conflate those clocks, consume CPU while idle, and add up to one tick
of avoidable input/network/compositor latency.

## Policy and security

Security defaults must be common even when native APIs differ:

- persistent and ephemeral storage are explicit;
- JavaScript enablement, developer tools, autoplay, and host messaging are
  creation policy, not accidental platform defaults;
- permissions default to denial unless a typed asynchronous request is
  resolved before its deadline;
- navigation, new-window, download, file-chooser, and external-protocol
  requests have typed decision IDs and guaranteed completion on every path;
- host-injected scripts use an isolated content world where supported;
- page-to-host messages are structured data and validated against origin and
  frame identity;
- local file navigation uses URL/path conversion APIs, never string
  concatenation of `file://`;
- release builds do not silently enable developer tools.

Do not add a public `native_handle()` or `platform_command(Any)` escape hatch.
A real missing capability should first be modeled in the common domain. A
target-only extension is acceptable only when a concrete use case cannot have
portable semantics.

## Failure and recovery

Failures are part of lifecycle, not log messages:

- browser process loss emits `ProcessFailed` and transitions through a typed
  recovery policy;
- host destruction detaches before the retained window is released;
- a create callback arriving after close is discarded by generation;
- pending scripts and decisions complete with typed cancellation;
- GPU loss invalidates Linux imported frames without destroying the browser
  model;
- DirectComposition device loss rebuilds the visual attachment without
  changing `WebViewId`;
- an unrecoverable backend failure leaves the evaluator-visible model intact
  and reports a reason suitable for Lisp and diagnostics.

Diagnostics should expose current lifecycle state, backend/capabilities,
profile identity without secrets, active presentation, queued request counts,
frame transfer path, imported bytes, and the last failure.

## Testing strategy

### 1. Pure state-machine tests

Run `WebViewSystemImpl<FakePlatform>` tests on every host:

- creation completes asynchronously;
- host registers after create;
- close occurs before create completion;
- stale generations cannot affect replacement views;
- desired state coalesces while waiting;
- non-convergent requests are bounded and cancelled;
- related-view/profile validation;
- script request/result correlation;
- policy timeout and completion on every path;
- active presentation replacement and stale input rejection;
- all-four-edge clipping and content offsets;
- duplicate occurrence rejection;
- detach, reparent, hide/show, process loss, and GPU recovery.

These tests should be written red first for each extraction or behavior change.

### 2. Shared adapter contract

One backend-neutral contract suite runs against each real adapter using local,
deterministic HTML:

- navigation, title, progress, history, and script results;
- resize/reflow and clip on all four sides;
- pointer coordinates, capture, leave, scroll, keyboard, focus, and IME;
- visibility, detach/reattach, host recreation, and process failure;
- persistent versus ephemeral storage;
- denied and allowed policy requests;
- accessibility hookup and capability reporting.

### 3. GUI tests

Add platform scenarios to `neomacs-gui-tests`. Do not assume native web views
appear in a wgpu readback on macOS or Windows. Use DOM/query instrumentation for
behavior and OS/window capture only where visual composition itself is under
test.

Add cross-target compile checks so target-specific native types cannot leak
through common modules. Run native adapter tests on each target in CI.

## Internal module shape

```text
neomacs-webview/src/
  lib.rs                 small public exports
  model.rs               IDs, commands, events, values, errors
  system.rs              platform-independent lifecycle state machine
  presentation.rs        validated scene and attachment diff
  backend.rs             private Platform contract
  platform/
    linux/
      mod.rs             profiles, policy, normalized events
      display.rs         headless WPEPlatform display
      engine.rs          GLib/WPEPlatform lifetime
      view.rs            browser, input, and owned frame leases
      sys.rs             private generated FFI boundary
    macos/
      mod.rs             WKWebView + clipping NSView adapter
    windows/
      mod.rs             WebView2 CompositionController + DComp child adapter
```

Use the workspace's `objc2-web-kit` bindings on macOS. On Windows, keep the
generated WebView2 COM bindings (for example `webview2-com`) and the workspace
`windows` DirectComposition bindings private to `platform/windows`; neither
dependency may appear in a common public type.

Avoid public files named after one engine. The crate vocabulary is `webview`;
engine names belong under private `platform` modules.

## Migration status

Steps 1–7 and 9 below are implemented. Step 8 has its composition-controller,
placement, mouse, and focus foundation; cursor negotiation, capture/leave,
touch/pen, IME, and UI Automation integration remain part of the hardening in
step 10.

1. Add the crate skeleton, pure domain types, private `Platform` trait,
   `FakePlatform`, and red/green lifecycle tests. Do not move native code yet.
2. Add `WebViewId` and an explicit `XwidgetId -> WebViewId` association. Remove
   assumptions that equal raw integers imply identity.
3. Introduce `AssetCommand::WebView(WebViewCommand)` and
   `InputEvent::WebView(WebViewEvent)` adapters around existing behavior.
4. Move the existing macOS WKWebView implementation behind `WebViewSystem`,
   retaining the current host-late behavior while replacing its raw command
   queue with convergent desired state.
5. Move the Linux WPE backend, buffer ownership, import policy, and WebKit wgpu
   cache behind the same system. Make the renderer consume prepared draws.
6. Compile and synchronize `ResolvedWebViewScene` from the active sealed
   presentation. Delete independent floating/resize/clip state and fix all-axis
   clipping at this single boundary.
7. Wire real navigation, title, progress, load, process, and script-result
   events through the normalized event path.
8. Implement the Windows WebView2 CompositionController backend. Its initial
   slice includes topmost-plane composition, raw-pixel bounds, clipping, mouse
   input, and focus. Complete cursor, DPI-change, capture/leave, touch/pen,
   IME, drag/drop, and accessibility integration before calling the native
   adapter contract complete.
9. Replace the Linux legacy WPE integration with WPEPlatform and remove the
   transitional adapter.
10. Complete policy/security, profile isolation, recovery, diagnostics, and the
    shared native adapter contract suite; then delete obsolete backend code and
    feature gates from display runtime and renderer crates.

## Rejected shapes

### A public `EmbeddedWebView` backend trait

Rejected. Callers have only one production backend per target, and a public
trait would expose platform-driven methods or an artificial lowest common
denominator. Keep the trait private and test the generic state machine.

### `enum Presentation { Texture, NativeView, CompositionVisual }` in caller code

Rejected. This makes every caller understand platform strategy. Prepared draws
can be empty on native targets while `synchronize_presentation` handles native
attachments internally.

### A browser worker thread on every platform

Rejected. AppKit requires the main thread and WebView2 requires the owning STA
and window message pump. Linux may use a private helper for buffer work if
measurement proves it valuable, but it cannot alter the public model.

### Treating WKWebView/WebView2 snapshots as live textures

Rejected. Snapshot APIs do not preserve interactive latency, video, input,
accessibility, or native browser UI semantics.

### Keeping independent `WebKitResize`, `WebKitSetFloating`, and visibility state

Rejected. Independent commands race during resize and duplicate the active
presentation's geometry. A complete resolved scene is the unit of change.

### Encoding dynamic browser readiness as public Rust typestate handles

Rejected. Commands cross an asynchronous display boundary and identities live
longer than any borrowed handle. Internal enums and generational IDs provide
compile-time exhaustiveness without fighting the actual messaging model.

## Architectural acceptance criteria

The extraction is complete only when:

- common callers contain no `cfg(target_os)` for web behavior;
- common callers contain no WPE, WebKit, Objective-C, COM, HWND, or DComp type;
- `XwidgetId` cannot be used where `WebViewId` is required;
- native views and foreign WPE frames have one explicit owner;
- stale create/script/process/frame callbacks are rejected by typed identity;
- one active presentation produces geometry, clipping, visibility, and input;
- renderer code samples opaque prepared draws and does not own WPE buffers;
- macOS and Windows do not pretend to be wgpu textures;
- platform policies have common defaults and typed asynchronous decisions;
- the pure state machine and shared adapter contract cover lifecycle races and
  recovery before the old scattered paths are removed.
