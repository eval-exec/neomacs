# `neomacs-webview` platform constraints and long-term architecture

Research date: 2026-08-31

This note derives the constraints for a cross-platform WebView subsystem from
upstream platform documentation and source. Statements labelled
**Recommendation** or **Inference** are conclusions for Neomacs, not promises
made by an upstream project. The proposed crate name is `neomacs-webview`:
WebKit is used on Linux and macOS, while Windows uses WebView2.

## Executive conclusion

`neomacs-webview` should be a thread-affine WebView service with a typed,
asynchronous command/event protocol. It should not be a “URL to `wgpu::Texture`”
library. The three supported platforms expose fundamentally different
presentation primitives:

| Platform | Engine and public host primitive | Presentation ownership | Input |
|---|---|---|---|
| Linux | WPE WebKit, `WPEBuffer` | Neomacs imports and presents a leased DMA-BUF or shared-memory buffer | Neomacs forwards events |
| macOS | `WKWebView`, an AppKit `NSView` | AppKit owns a native view in the host view hierarchy | AppKit responder/hit-testing path |
| Windows | WebView2 `ICoreWebView2CompositionController` | Neomacs attaches a WebView visual to an app-owned composition tree | Neomacs forwards mouse/pointer input and manages focus |

The diagnostic/capability model must preserve those differences as a closed
enum, while ordinary command callers remain platform-erased. A texture-looking
interface would force snapshots on macOS, lose WebView2's intended composition
path on Windows, and conceal Linux buffer and fence lifetimes that are required
for correctness.

The durable object model is:

```text
WebProfile (storage, privacy, process sharing)
    └── WebView (page, navigation, script, policy)
            └── zero or one WebViewAttachment (window and presentation state)

UI/event-loop owner thread
    ├── !Send WebViewRuntime and all native objects
    ├── receives WebViewCommand through Neomacs' display command path
    └── publishes WebViewEvent without re-entering Lisp or rendering callbacks
```

On Windows, arbitrary interleaving between WebView content and GPU-drawn
Neomacs layers has one additional architectural prerequisite: the application
must own a common DirectComposition tree containing a `wgpu` visual and one or
more WebView2 visuals. This root cannot be hidden exclusively inside
`neomacs-webview`. If Neomacs does not adopt a shared tree, its truthful
capability is only “WebView above or below the complete GPU surface,” not
arbitrary WebView/GPU z-order.

## Scope and current Neomacs seam

The current implementation has platform code in
`neomacs-display-runtime/src/backend/wpe`,
`neomacs-display-runtime/src/backend/webkit`, and
`neomacs-display-runtime/src/backend/wkwebview`, while Linux frame import and
caching also reach into `neomacs-renderer-wgpu`. Web commands are flat variants
of display runtime's `AssetCommand`, and the macOS adapter converts only the
subset it supports. That shape makes adding a command a cross-module convention
rather than a compile-time-complete WebView protocol.

`neomacs-video` is the closest local precedent. It keeps platform objects
private, exposes typed commands, events, and state, uses a platform-selected
implementation, bounds frame delivery, records device generations, and
separates the UI-facing service from GPU draw preparation. A new WebView crate
should reuse those architectural lessons, but it cannot reuse the assumption
that every platform ultimately provides a sampled video frame.

The research covers rendering and presentation, clipping and z-order, input,
thread affinity, lifecycle, asynchronous operations, GPU interoperation,
deployment, process isolation, storage, and trust boundaries. It does not
define the Lisp/xwidget compatibility surface or page layout policy.

## Upstream constraints

### Linux: WPE WebKit is an application-presented buffer source

WPE is WebKit's official toolkit-independent embedded port. Its documented
architecture deliberately leaves final presentation to a backend/application:
the page representation is delivered to the host, historically usually as an
EGL image, and the host presents it. Because WPE supplies no widget, the host
also relays keyboard, pointer, and touch input
([WPE architecture](https://wpewebkit.org/about/architecture.html)).

WPEPlatform is the replacement integration layer. Its `WPEDisplay` manages
`WPEView`s, and it exchanges DMA-BUF or shared-memory `WPEBuffer`s. Upstream's
current transition guidance says WPEPlatform exists in WPE WebKit 2.52 but is
not yet stable/default, is expected to replace libwpe in 2.54, and is available
with built-in Wayland, DRM/KMS, and headless implementations. WPE itself
currently supports Linux-based operating systems
([WPE FAQ](https://wpewebkit.org/about/faq.html#what-is-the-wpeplatform-api)).

**Recommendation:** target WPEPlatform as the long-term adapter, but isolate
all WPE C symbols and version selection inside `platform::linux`. This upstream
API transition is time-sensitive. Configure/build detection must record the
actual WPE and WPEPlatform versions; public Neomacs types must not expose either
the legacy libwpe backend API or preview WPEPlatform types.

The current upstream `WPEView` implementation makes the buffer ownership
protocol explicit. If a platform accepts a buffer for rendering, it must
eventually report that rendering occurred and report when the buffer is no
longer used. View dimensions are logical, while scale is a separate property;
input enters through the view's event API
([`WPEView.cpp`](https://github.com/WebKit/WebKit/blob/main/Source/WebKit/WPEPlatform/wpe/WPEView.cpp)).
`WPEBuffer` also carries rendering/release fences and supports EGL-image or CPU
pixel import. Its CPU byte view is borrowed from the buffer rather than copied
([`WPEBuffer.cpp`](https://github.com/WebKit/WebKit/blob/main/Source/WebKit/WPEPlatform/wpe/WPEBuffer.cpp)).
The DMA-BUF subtype exposes format, planes, file descriptors, offsets, strides,
and modifiers
([`WPEBufferDMABuf.cpp`](https://github.com/WebKit/WebKit/blob/main/Source/WebKit/WPEPlatform/wpe/WPEBufferDMABuf.cpp)).

**Recommendation:** a Linux frame is a lease, not a bag of copied integer file
descriptors. The adapter must retain the native buffer or equivalent ownership,
use owned file descriptors, preserve producer and consumer fence semantics,
and release the buffer only when the GPU no longer uses it. The precise native
notification calls depend on the selected WPEPlatform version, but the Rust
state machine must make an early or duplicate release impossible.

```rust
enum LinuxFrame {
    DmaBuf(DmaBufFrameLease),
    SharedMemory(SharedMemoryFrameLease),
}

enum TransferPath {
    ZeroCopyDmaBuf,
    CpuUpload,
}
```

The transfer path must be observable and policy-controlled. Falling back to a
CPU upload may be correct, but silently doing so would hide a large performance
and memory-bandwidth change.

WPE/WebKit APIs use GLib's asynchronous model. A thread-default `GMainContext`
is inherited by asynchronous operations, and callbacks are dispatched by that
context. A main context can be acquired by only one thread at a time
([GLib main contexts](https://docs.gtk.org/glib/main-loop.html),
[`GMainContext`](https://docs.gtk.org/glib/struct.MainContext.html)). WebKit's
JavaScript evaluation API is asynchronous and completed with a corresponding
finish operation
([`webkit_web_view_evaluate_javascript`](https://webkitgtk.org/reference/webkit2gtk/stable/method.WebView.evaluate_javascript.html)).

**Recommendation:** the Linux adapter owns a known GLib main context on the
same long-lived thread that owns its WebKit objects. It must not depend on an
ambient default context or allow unrelated threads to iterate it concurrently.

WebKit makes profiles/process state broader than a single view.
`WebKitWebContext` manages aspects shared by multiple views and supports both
ephemeral construction and an explicit website-data manager. Current WebKitGTK
uses multiple secondary processes as its supported process model and exposes
sandbox state
([`WebKitWebContext`](https://webkitgtk.org/reference/webkit2gtk/stable/class.WebContext.html)).
Policy decisions may be held for asynchronous completion, and must eventually
be explicitly used, ignored, or downloaded
([`decide-policy`](https://webkitgtk.org/reference/webkit2gtk/stable/signal.WebView.decide-policy.html)).
The embedder can terminate a view's web process and observe termination
([`terminate_web_process`](https://webkitgtk.org/reference/webkit2gtk/stable/method.WebView.terminate_web_process.html)).

**Inference:** profile, policy-decision, and process-failure concepts belong in
the cross-platform domain model rather than being Linux details.

### macOS: `WKWebView` is a native AppKit view

`WKWebView` is the supported object for displaying interactive web content and
is an AppKit `NSView`
([`WKWebView`](https://developer.apple.com/documentation/webkit/wkwebview)).
AppKit view and window work is main-actor/main-thread work
([Apple UI framework threading overview](https://developer.apple.com/documentation/technologyoverviews/uikit-appkit),
[`NSView`](https://developer.apple.com/documentation/appkit/nsview)). The
`raw-window-handle` AppKit handle reinforces the same boundary for Rust: it
contains an `NSView` pointer, must be used on the main thread, and is neither
`Send` nor `Sync`
([`AppKitWindowHandle`](https://docs.rs/raw-window-handle/0.6.2/raw_window_handle/struct.AppKitWindowHandle.html)).

A `WKWebView` participates in native geometry, clipping, ordering, hit testing,
focus, accessibility, and input. AppKit offers explicit sibling placement for
subviews and view hit-testing
([`addSubview(_:positioned:relativeTo:)`](https://developer.apple.com/documentation/appkit/nsview/addsubview%28_%3Apositioned%3Arelativeto%3A%29),
[`hitTest(_:)`](https://developer.apple.com/documentation/appkit/nsview/hittest%28_%3A%29)).
Clipping is a view property and must be set deliberately rather than inferred
from a renderer scissor
([`clipsToBounds`](https://developer.apple.com/documentation/appkit/nsview/clipstobounds)).
AppKit geometry is expressed in points and windows expose a backing scale
factor for conversion to pixels
([`backingScaleFactor`](https://developer.apple.com/documentation/appkit/nswindow/backingscalefactor)).

**Recommendation:** the macOS presentation variant owns a retained native
subview attachment and updates it only on the main thread. It must receive
logical bounds, an explicit clip, visibility, and native sibling-order intent.
A `wgpu` render-pass scissor does not clip a sibling `NSView`.

The public API for capturing a `WKWebView` is an asynchronous snapshot that
returns an image
([`takeSnapshot`](https://developer.apple.com/documentation/webkit/wkwebview/takesnapshot%28with%3Acompletionhandler%3A%29)).
Apple does not document this as a continuous live GPU-texture export path.

**Recommendation:** never implement ordinary macOS presentation by repeatedly
snapshotting a `WKWebView` into `wgpu`. Snapshots are suitable for testing,
printing, thumbnails, or transitions, not the live abstraction.

JavaScript evaluation is asynchronous
([`evaluateJavaScript`](https://developer.apple.com/documentation/webkit/wkwebview/evaluatejavascript%28_%3Acompletionhandler%3A%29)).
Navigation policy is also a completion-handler protocol
([`WKNavigationDelegate` policy decision](https://developer.apple.com/documentation/webkit/wknavigationdelegate/webview%28_%3Adecidepolicyfor%3Adecisionhandler%3A%29-2ni62)).
Configuration is supplied when the view is constructed, while website data
stores distinguish persistent and nonpersistent data
([`WKWebViewConfiguration`](https://developer.apple.com/documentation/webkit/wkwebviewconfiguration),
[`WKWebsiteDataStore`](https://developer.apple.com/documentation/webkit/wkwebsitedatastore)).
Web content executes outside the UI process; navigation delegates report
termination of a web-content process
([`WKNavigationDelegate`](https://developer.apple.com/documentation/webkit/wknavigationdelegate)).

WebKit content worlds isolate application-authored scripts from page scripts
([`WKContentWorld`](https://developer.apple.com/documentation/webkit/wkcontentworld)).

**Inference:** a portable API must distinguish configuration/profile creation
from view creation, make script results and navigation policy asynchronous,
and report process failure. Script injection intended as an application bridge
should use an isolated content world where the OS API supports it.

### Windows: WebView2 visual hosting is retained composition

The selected Windows host is `ICoreWebView2CompositionController`, not the
windowed WebView2 controller. In visual hosting, the application receives no
child `HWND`; it is responsible for connecting the WebView to a visual tree,
hit testing, coordinate conversion, forwarding input, and layout
([windowed versus visual hosting](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/windowed-vs-visual-hosting)).

The composition controller's `RootVisualTarget` accepts an
`IDCompositionVisual` (or a Windows Composition visual). Bounds, visibility,
focus, rasterization scale, and parent-window notification remain controller
responsibilities. Mouse and pointer input enter through composition-controller
methods and are expressed relative to the WebView client area; the host must
also model capture and pointer-leave behavior
([`ICoreWebView2CompositionController`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2compositioncontroller)).
Composition-controller creation is asynchronous, requires a real parent
window, and can fail if that parent is destroyed before completion
([`CreateCoreWebView2CompositionController`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2environment3#createcorewebview2compositioncontroller)).

DirectComposition uses a retained visual tree. Sibling order establishes
z-order, a clip affects a visual and its subtree, and `Commit` atomically
applies a batch of visual changes
([DirectComposition architecture](https://learn.microsoft.com/en-us/windows/win32/directcomp/architecture-and-components),
[`IDCompositionVisual::AddVisual`](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositionvisual-addvisual),
[`IDCompositionVisual::SetClip`](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositionvisual-setclip%28idcompositionclip%29)).

`wgpu` 30 exposes the matching integration point. Its DX12
`DxgiFromVisual` swap-chain mode is documented for callers that manage their
own DirectComposition tree, and directs the caller to create the composition
and pass a visual as `SurfaceTargetUnsafe::CompositionVisual`
([`Dx12SwapchainKind::DxgiFromVisual`](https://docs.rs/wgpu-types/30.0.0/wgpu_types/enum.Dx12SwapchainKind.html#variant.DxgiFromVisual),
[`SurfaceTargetUnsafe::CompositionVisual`](https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu/src/api/surface.rs#L438-L444)).

**Inference:** a shared DirectComposition tree is the only documented path in
the chosen stack for robust arbitrary sibling ordering between the live `wgpu`
surface and live WebView2 visuals. The long-term ownership should be:

```text
window/display presentation owner
    └── DirectComposition root
        ├── wgpu CompositionVisual (surface target)
        ├── WebView2 visual A
        ├── optional GPU overlay visual
        └── WebView2 visual B
```

`neomacs-webview` should own each WebView2 controller and its WebView child
visual, but the display/window layer should own the device/root transaction
boundary and hand the crate an opaque attachment capability. A future tiny
`neomacs-native-compositor` module could own this seam if both the renderer and
WebView crate need it. The web crate must not take over the renderer's surface
lifetime merely to attach browser content.

**Recommendation:** prototype a `wgpu` `CompositionVisual` and WebView2 visual
as siblings before freezing the cross-platform attachment API. If that
prototype is deferred, expose a reduced Windows stacking capability rather
than implying arbitrary interleaving.

WebView2 uses a single-threaded apartment UI thread with a message pump. All
WebView2 calls and callbacks occur on that thread. Blocking it can deadlock,
and nested message-loop reentrancy from callbacks is unsupported
([WebView2 threading model](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/threading-model)).
Bounds may be supplied as raw pixels or as logical bounds multiplied by the
rasterization scale, depending on `BoundsMode`; applications must update scale
when it changes
([`ICoreWebView2Controller3`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2controller3)).

**Recommendation:** construct and use COM/WebView2 objects only on the
winit/window owner thread after initializing an STA there. Never block waiting
for a WebView2 callback and never call Lisp or the renderer synchronously from
one. Commands received from other threads are drained by the normal event-loop
wakeup path.

The composition controller can expose an accessibility provider to the host
([`ICoreWebView2CompositionController2`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2compositioncontroller2)).

**Recommendation:** accessibility-provider attachment is part of “ready to
present” on Windows, not optional polish. A composition-hosted page that draws
and receives mouse events but is absent from the automation tree is not a
complete WebView implementation.

WebView2 uses the Microsoft Edge multiprocess model
([WebView2 process model](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model)).
Its user-data folder contains browser data including cookies, permissions, and
cache; environment/user-data choices affect process sharing
([user-data folders](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/user-data-folder)).
Microsoft's security guidance says to treat WebView content as untrusted,
validate origins, prefer narrowly defined structured messages, and disable
unused script, messaging, and host-object surfaces
([WebView2 security](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security)).

WebView2 requires a Runtime. Microsoft recommends Evergreen for most apps; a
fixed-version distribution is much larger and places servicing responsibility
on the application. Applications must detect/install a usable runtime and
feature-detect newer COM interfaces rather than assuming the installed browser
version
([Evergreen versus Fixed Version](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/evergreen-vs-fixed-version),
[WebView2 distribution](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)).

**Recommendation:** make Runtime discovery an explicit initialization result.
Report the installed version and capability set, support Evergreen deployment
first, and use `QueryInterface`-style feature detection inside the adapter.
“Compiled with WebView2” must not mean “available at runtime.”

## Cross-platform domain model

### One thread-affine runtime, one existing shareable command path

All three adapters need an owner thread/event-loop context, although the OS
reason differs. macOS requires the OS main thread, Windows requires an STA UI
thread with a message pump, and Linux requires disciplined ownership of its
GLib main context. Winit's `EventLoop` is itself neither `Send` nor `Sync` and
provides an `EventLoopProxy` specifically for cross-thread event delivery
([`winit::EventLoop`](https://docs.rs/winit/0.30.12/winit/event_loop/struct.EventLoop.html),
[`winit::EventLoopProxy`](https://docs.rs/winit/0.30.12/winit/event_loop/struct.EventLoopProxy.html)).
Although `winit::Window` is `Send + Sync`, its macOS documentation says some
calls are dispatched to the main thread and can block
([`winit::Window`](https://docs.rs/winit/0.30.12/winit/window/struct.Window.html)).

**Recommendation:** encode the ownership invariant rather than documenting it
only in comments.

```rust
pub struct WebViewRuntime {
    platform: CurrentPlatform,
    // Prevent accidental transfer even if a wrapper crate marks a native
    // pointer Send.
    thread_affinity: PhantomData<Rc<()>>,
}
```

`WebViewRuntime` is deliberately `!Send + !Sync`. `WebViewCommand` and
`WebViewEvent` are owned `Send` values nested in Neomacs' existing display
communication, which is the shareable proxy. The crate should not add a second
public command queue. `CurrentPlatform` is a compile-time selected private
enum/newtype rather than an optional field for every OS. There is no generic
“browser thread”: the display/event-loop owner is the browser owner where
required.

Native callbacks append typed events to a queue and wake the consumer. They do
not re-enter Lisp, perform redisplay, or synchronously send a command back into
the native WebView. This rule avoids WebView2's documented reentrancy failure
mode and makes the other adapters deterministic too.

### Profiles, views, and attachments are different lifetimes

A browsing data/process context is not a view, and a page can exist while
temporarily hidden or unattached during window changes. Model those separately:

```rust
pub struct WebProfileId(NonZeroU64);
pub struct WebViewId(NonZeroU64);
pub struct WindowId(/* opaque display-window identity */);

pub enum StorageMode {
    Persistent { profile: WebProfileId },
    Ephemeral,
}

pub struct WebProfileConfig {
    pub storage: StorageMode,
    pub trust: TrustPolicy,
    pub locale: Option<String>,
}

pub struct WebViewConfig {
    pub profile: WebProfileId,
    pub initial_content: InitialContent,
    pub preferences: WebPreferences,
}
```

The mapping is deliberately semantic rather than one-to-one with native
objects: WPE may use a `WebKitWebContext` and website-data manager, macOS uses a
`WKWebsiteDataStore` and configuration, and Windows uses an environment,
user-data directory, and profile. Platform adapters may share native process
infrastructure as their engine permits; callers choose storage/trust behavior,
not a native “process pool.”

Each `WebView` has at most one `WebViewAttachment`. Attaching means supplying a
window-specific presentation capability and participating in the next scene
snapshot. Detaching does not imply destroying page/profile state.

### Presentation differences are an enum, not nullable fields

```rust
pub enum PresentationModel {
    SampledBuffer,
    NativeView,
    CompositionVisual,
}

pub enum InputRouting {
    NativeView,
    HostForwarded,
}

pub struct WebViewCapabilities {
    pub presentation: PresentationModel,
    pub input: InputRouting,
    pub accessibility: AccessibilityCapability,
    pub transfer: Option<TransferPath>,
    pub engine: EngineVersion,
}
```

Use separate target-gated private attachment variants containing native
handles. Do not expose a public enum whose variants contain `WPEBuffer`,
`NSView`, or COM pointers; those would leak unsafe lifetimes and platform
dependencies through the crate boundary. The public capability enum is for
policy, diagnostics, tests, and layout decisions.

Logical layout units and physical pixels must also be distinct types:

```rust
pub struct LogicalPoint { pub x: f64, pub y: f64 }
pub struct LogicalSize { pub width: f64, pub height: f64 }
pub struct LogicalRect { pub origin: LogicalPoint, pub size: LogicalSize }
pub struct PhysicalSize { pub width: u32, pub height: u32 }
pub struct ScaleFactor(NonZeroFiniteF64);
```

**Recommendation:** validate finite, nonnegative geometry at construction.
Converting between logical and physical coordinates must require an explicit
`ScaleFactor`; raw `(f32, f32)` tuples should never cross the crate boundary.
This matches WPE's separate logical size/scale, AppKit points/backing scale,
and WebView2's explicit bounds mode/rasterization scale.

### Apply a complete presentation scene, not resize commands

Web content is positioned by redisplay output. A stream of independent
`Resize`, `Move`, `Show`, `Clip`, and `Raise` commands can expose mixtures from
different frames during resize. It also makes “not drawn in this frame” hard to
distinguish from “leave the old native view visible.”

**Recommendation:** reconcile one complete scene snapshot after layout:

```rust
pub struct WebViewScene {
    pub window: WindowId,
    pub epoch: SceneEpoch,
    pub scale: ScaleFactor,
    pub views: Vec<PresentedWebView>,
}

pub struct PresentedWebView {
    pub id: WebViewId,
    pub generation: Generation,
    pub bounds: LogicalRect,
    pub clip: Option<LogicalRect>,
    pub visibility: Visibility,
    pub stacking: StackingSlot,
}
```

The runtime rejects an older epoch, applies all native-view/visual mutations,
and commits once where the platform supports transactions. Views omitted from
the current scene are hidden or detached according to an explicit policy;
they do not remain at stale geometry. Linux uses the same snapshot to decide
which leased frame the GPU scene samples. macOS updates subviews. Windows
updates visual/controller properties and commits the DirectComposition batch.

`StackingSlot` should describe a small capability-aware layer vocabulary, not
an unconstrained integer that every platform appears to honor. For example:

```rust
pub enum StackingSlot {
    BehindGpuContent,
    Inline { order: u32 },
    AboveGpuContent,
}
```

**Inference:** `Inline` can only be supported where WebViews and relevant GPU
layers share a native composition hierarchy. The runtime must reject or
degrade an unsupported slot explicitly and report the actual capability.

### Input routing belongs with the attachment

macOS normally uses native `NSView` hit testing and responder routing. WPE and
composition-hosted WebView2 require host forwarding. A uniform API should
express common semantic input without pretending the native path consumes it:

```rust
pub enum WebViewInput {
    PointerMoved { position: LogicalPoint, device: PointerDevice },
    PointerButton { position: LogicalPoint, button: PointerButton, state: ButtonState },
    PointerLeft,
    Wheel { position: LogicalPoint, delta: WheelDelta },
    Key(KeyEvent),
    Ime(ImeEvent),
    FocusChanged(bool),
    Touch(TouchEvent),
    Pen(PenEvent),
}
```

For `HostForwarded`, the crate owns attachment-local hit testing, coordinate
translation, capture/leave bookkeeping, and conversion to the engine's native
event. For `NativeView`, the event is normally not submitted through this API;
the attachment reports that the OS owns spatial routing. This prevents double
delivery on macOS.

Keyboard, IME, drag/drop, touch, pen, focus, cursor, and accessibility must be
tracked as explicit capability/test areas. A pointer-click-only adapter is a
prototype, not implementation parity.

## Lifecycle and asynchronous protocol

### Generational state machine

All engines have asynchronous operations, and WebView2 creation itself is
asynchronous. A view may be destroyed and its numeric identity reused before a
late callback arrives. Use a generation in every native callback and
request/decision identifier in every asynchronous exchange:

```rust
pub struct Generation(NonZeroU64);
pub struct RequestId(NonZeroU64);
pub struct DecisionId(NonZeroU64);

enum WebViewSlot {
    Creating {
        generation: Generation,
        pending: BoundedPendingCommands,
    },
    Ready {
        generation: Generation,
        view: PlatformView,
        attachment: Option<PlatformAttachment>,
    },
    Failed {
        generation: Generation,
        reason: WebViewFailure,
    },
    Closing {
        generation: Generation,
    },
}
```

The runtime ignores callbacks for a noncurrent generation. Destroying a view
removes event handlers, resolves/cancels native completion obligations, closes
the native controller/view, releases buffer leases, and emits one terminal
event. Commands accepted during creation use a small bounded queue with
coalescing for replaceable state such as scene/size; they never accumulate
without limit.

Make invalid transitions unrepresentable within each state rather than using
one struct full of `Option` fields. A `ReadyView` has a native view; a
`CreatingView` cannot accidentally evaluate JavaScript against a null pointer;
a `ClosingView` cannot accept a new navigation.

### Commands and events

Replace the flat `AssetCommand::WebKitCreate`, `WebKitLoadUri`, and related
variants with a single nested protocol, ideally re-exported directly from the
new crate:

```rust
pub enum WebViewCommand {
    CreateProfile { id: WebProfileId, config: WebProfileConfig },
    DestroyProfile { id: WebProfileId },
    Create { id: WebViewId, config: WebViewConfig },
    Destroy { id: WebViewId },
    Navigate { id: WebViewId, target: NavigationTarget },
    History { id: WebViewId, action: HistoryAction },
    EvaluateScript { id: WebViewId, request: RequestId, script: ScriptRequest },
    ResolveDecision { decision: DecisionId, response: DecisionResponse },
    ApplyScene(WebViewScene),
    Input { id: WebViewId, event: WebViewInput },
}

pub enum WebViewEvent {
    ProfileReady { id: WebProfileId, generation: Generation },
    Created { id: WebViewId, generation: Generation, capabilities: WebViewCapabilities },
    CreationFailed { id: WebViewId, generation: Generation, error: WebViewFailure },
    Navigation(NavigationEvent),
    TitleChanged { id: WebViewId, title: String },
    UrlChanged { id: WebViewId, url: String },
    ScriptFinished { id: WebViewId, request: RequestId, result: ScriptResult },
    DecisionRequested(DecisionRequest),
    ProcessFailed { id: WebViewId, generation: Generation, failure: ProcessFailure },
    Closed { id: WebViewId, generation: Generation },
}
```

Nested exhaustive enums turn backend incompleteness into compiler-visible
matches. Platform-specific capability differences remain explicit errors or
capabilities; silently dropping an unrecognized WebView command is forbidden.

JavaScript is always asynchronous at the public boundary, even if a future
backend sometimes returns immediately. Its result type distinguishes a valid
JavaScript value, a JavaScript exception, cancellation because the view died,
and engine/serialization failure. Do not return arbitrary platform objects;
use a bounded structured value representation.

Navigation, new-window, permission, authentication, and download decisions
must be typed variants rather than strings. A `DecisionId` is single-use. An
internal decision guard owns the native completion obligation and guarantees
exactly one completion on explicit response, timeout, shutdown, or dropped
receiver. Defaults are policy-specific: permissions and privileged bridge
requests default deny; ordinary navigation uses a separately configured
policy. This design fits WebKit and WebView2 completion-handler APIs without
blocking the native callback.

## GPU interoperability and frame flow

Linux's DMA-BUF import crosses `wgpu`'s safe abstraction boundary. The public
`Device::create_texture_from_hal` operation is unsafe and requires that the
texture come from the same backend device, match the supplied descriptor, be
initialized, and be in the expected state
([`wgpu::Device::create_texture_from_hal`](https://docs.rs/wgpu/30.0.1/wgpu/struct.Device.html#method.create_texture_from_hal)).
External Vulkan memory also has explicit import/ownership rules in `wgpu-hal`
([wgpu-hal Vulkan source](https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-hal/src/vulkan/mod.rs)).

**Recommendation:** isolate all unsafe WPE-buffer import in one small module
whose safe result retains the lease for at least as long as the imported GPU
resource. Validate backend, device generation, format/modifier/plane support,
dimensions, descriptor agreement, synchronization fences, and ownership before
constructing a texture. On device loss, invalidate every imported texture and
re-import only a newly delivered buffer against the new device.

The web crate should receive a narrow GPU context/capability from the renderer
and return a platform-neutral prepared sampled resource. It should not expose
raw DMA-BUF metadata to display runtime. Conversely, it should not own the
general renderer or command encoder.

Neomacs already needs external-GPU-resource logic for video. Duplicating
Vulkan/Metal/DX12 ownership rules in two crates would be dangerous.

**Recommendation:** after the WebView seam is working, extract a small deep
GPU-import module shared by `neomacs-video` and `neomacs-webview`, or have both
call a renderer-owned importer through the same narrow interface. Share only
the unsafe import/lifetime machinery; WebView buffer-release policy and video
frame policy remain in their domain crates.

Frame delivery must be bounded. A latest-frame mailbox is appropriate for live
page presentation: a new undisplayed frame replaces an older pending frame,
while the currently submitted frame remains leased until GPU completion. Track
replaced/dropped frames for diagnostics. Do not queue every browser frame
behind a slow redisplay.

There is no corresponding generic live texture path on macOS or Windows in the
selected public APIs. Their native view/visual objects never pass through this
GPU importer.

## Security, storage, and process recovery

Web content must be considered hostile unless a stronger policy is explicitly
selected. Configuration should make trust and bridge authority visible:

```rust
pub enum TrustPolicy {
    UntrustedWeb,
    TrustedApplication {
        allowed_origins: NonEmpty<OriginPattern>,
        bridge: BridgePolicy,
    },
}

pub struct BridgePolicy {
    pub allowed_messages: BTreeSet<MessageKind>,
    pub max_message_bytes: NonZeroUsize,
    pub frames: FramePolicy,
}
```

Defaults:

- no unrestricted native host-object exposure;
- no navigation-policy bypass or TLS-error bypass;
- permission requests deny unless explicitly handled;
- validate current origin and frame for every bridge message;
- use structured, size-bounded messages instead of evaluating message text;
- put application scripts in an isolated content world where supported;
- disable script or web messaging when a view does not need it; and
- never infer persistence from a directory that happened to exist.

Persistent profiles need an application-owned, versioned directory policy and
ephemeral profiles must be explicit. A profile cannot be destroyed while live
views refer to it; represent this through ownership/refcounts and return a
typed error instead of deleting data opportunistically.

Engine process crashes are normal recoverable events, not Rust panics. Emit a
typed `ProcessFailed` event with the engine's failure category. Recovery uses a
serializable manifest—profile identity, last committed navigation/source, view
preferences, and attachment intent—not retained native pointers. Whether to
reload automatically is caller policy because replaying a POST, local form
state, or privileged application page has different consequences.

## Crate boundary

`neomacs-webview` should own:

- cross-platform IDs, configurations, commands, events, lifecycle states, and
  errors;
- profile/storage/trust policy and decision-token semantics;
- native WPE, WKWebView, and WebView2 runtime/view/controller objects;
- per-view attachment state and platform input translation;
- WPE buffer leases, synchronization, and WebView-specific prepared frames;
- callback registration/removal and process-failure normalization;
- runtime capability/version detection and diagnostics.

It should not own:

- Lisp primitives, xwidget compatibility, or buffer/window layout policy;
- winit's top-level event loop or Neomacs's top-level windows;
- general scene construction, renderer passes, or frame scheduling;
- arbitrary `wgpu` surface/device recovery policy;
- the application-wide DirectComposition root and transaction boundary; or
- native WebKit/COM/WPE handles in its public API.

A narrow integration surface is sufficient:

```rust
pub trait WebViewWake: Send + Sync {
    fn wake(&self);
}

impl WebViewRuntime {
    pub fn new(config: RuntimeConfig, host: PlatformHostCapability) -> Result<Self, InitError>;
    pub fn command(&mut self, command: WebViewCommand) -> Result<(), CommandError>;
    pub fn synchronize_presentation(&mut self, scene: WebViewScene) -> Result<(), SceneError>;
    pub fn service(&mut self, now: Instant) -> WebViewServiceResult;
    pub fn prepare_sampled_views(&mut self, gpu: &GpuContext) -> PreparedWebViews;
}
```

The concrete API need not use these exact method names. The important depth is
that callers express intent and consume normalized events; platform callbacks,
COM interfaces, Objective-C objects, GLib ownership, and buffer fences remain
behind the module.

## Deployment and diagnostics

Compile-time support and runtime availability are separate:

```rust
pub enum WebViewAvailability {
    Available(WebViewCapabilities),
    NotBuilt,
    MissingRuntime { remediation: RuntimeRemediation },
    UnsupportedGraphics { reason: GraphicsCapabilityError },
    InitializationFailed { error: InitError },
}
```

- Linux records the discovered WPE WebKit/WPEPlatform ABI, selected display
  implementation, DMA-BUF format/modifier support, and active transfer path.
- macOS records the OS/WebKit capability level but needs no bundled browser
  runtime because WKWebView is a system framework.
- Windows records the WebView2 Runtime version, Evergreen/fixed deployment
  mode, supported COM interface levels, composition mode, and accessibility
  availability.

Per-profile/view diagnostics should include lifecycle state and generation,
storage mode, engine/runtime version, presentation/input capabilities, current
scene epoch, process-failure/recovery counts, queued/coalesced commands, and—on
Linux—received, replaced, displayed, and released frames plus transfer path.
These are structured snapshot data, not log parsing.

## Test architecture

Most correctness can be tested without a browser by injecting a private mock
platform behind the same runtime state machine:

1. Creation moves `Creating -> Ready` exactly once, desired pre-ready state
   coalesces, and non-convergent requests complete or cancel in defined order.
2. A callback from an old generation cannot mutate a recreated view.
3. A decision token completes exactly once across respond, timeout, shutdown,
   and drop paths.
4. Scene epochs reject stale geometry; omission hides/detaches a previously
   shown view.
5. Logical-to-local coordinate conversion respects bounds, clip, and scale.
6. Profile destruction fails while views retain it; ephemeral/persistent
   policies never alias.
7. Process failure releases attachments/frames and produces a recoverable
   manifest without retaining native objects.
8. The command/event enums are exhaustively matched in each compiled adapter.

Platform contract tests then cover native obligations:

- Linux: DMA-BUF and shared-memory leases, fence order, multi-plane/modifier
  rejection, CPU fallback, frame replacement, and device-generation loss.
- macOS: main-thread enforcement, view insertion/removal, clipping, scale,
  focus, policy callback completion, process termination, and no snapshot-based
  live path.
- Windows: STA enforcement, async destruction during creation, visual-tree
  attachment, one DirectComposition commit per scene, clip/z-order, DPI,
  capture/leave, focus, accessibility provider, missing Runtime, and process
  failure.

End-to-end GUI tests should exercise rapid resize, scroll clipping, hide/show,
window destruction during creation, multiple WebViews, WebView/GPU stacking,
pointer/keyboard/IME focus, DPI changes, navigation-policy timeouts, and forced
web-process termination. Pixel tests alone are insufficient for native
view/visual backends; tests must also query attachment geometry, focus,
accessibility, lifecycle, and scene epoch.

## Migration sequence

1. Add the platform-neutral IDs, geometry newtypes, profile/view/attachment
   model, command/event enums, mock runtime, and red state-machine tests.
2. Move current WPE and WKWebView code behind the crate without intentional
   behavior changes; keep native types private.
3. Replace flat `AssetCommand::WebKit*` variants with one nested
   `AssetCommand::WebView(WebViewCommand)` and make every backend match
   exhaustively.
4. Replace individual resize/show behavior with complete scene snapshots and
   scene-epoch tests.
5. Make Linux frames owned leases, preserve fences, bound delivery, expose the
   transfer path, and connect device-generation recovery.
6. Prototype the shared Windows DirectComposition tree with a `wgpu`
   `CompositionVisual`, WebView2 composition visual, clipping, input, and
   accessibility before stabilizing attachment capabilities.
7. Implement the WebView2 adapter and Evergreen Runtime diagnostics.
8. Extract only the proven common unsafe GPU-import seam from video/WebView
   code, then remove transitional duplicate adapters.

Each step can be landed with its own tests. Windows composition feasibility is
an architectural spike, not a reason to contaminate the public API with raw
COM objects.

## Decisions and deliberately open questions

The research supports these decisions now:

- use the crate name `neomacs-webview`;
- use WPE WebKit/WPEPlatform on Linux, native WKWebView on macOS, and
  `ICoreWebView2CompositionController` on Windows;
- make the runtime thread-affine and communicate through typed asynchronous
  commands/events;
- separate profiles, views, and window attachments;
- expose presentation/input/capability enums rather than a false common
  texture type;
- update presentation from versioned complete scene snapshots;
- make Linux buffer leases and async decision tokens explicit state machines;
- keep native engine handles private; and
- require an app-owned shared composition hierarchy for arbitrary Windows
  WebView/GPU interleaving.

Questions to settle with prototypes or product policy rather than assumptions:

1. Does Neomacs need arbitrary GPU/WebView interleaving, or are three coarse
   stacking slots sufficient?
2. Which WPEPlatform release/ABI is the minimum supported Linux baseline while
   the upstream 2.52-to-2.54 transition completes?
3. Which DMA-BUF formats/modifiers and CPU fallback policy define “supported”
   for the first Linux release?
4. What exact URL/origin, permission, download, new-window, and application
   bridge policies belong in the Lisp-facing layer?
5. Should the common DirectComposition owner remain inside display runtime or
   become a small `neomacs-native-compositor` crate after the Windows spike?

Those choices change supported behavior or deployment policy. They should not
be guessed by a platform adapter.
