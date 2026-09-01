# Cross-platform GPU video pipeline reference projects

Research date: 2026-08-31

This note compares primary source and documentation from Chromium/Viz,
Firefox/WebRender, mpv/libplacebo, FFmpeg, Qt Multimedia, WebKit, and
GStreamer.  The question is which production designs Neomacs should learn from
when evolving from packed BGRA/RGBA GPU interop toward native NV12/P010 video
surfaces, low-copy platform interop, correct synchronization, and color-aware
composition.

## Executive finding

No project supplies one pipeline that Neomacs can copy wholesale.  The best
synthesis is:

1. **Chromium** for the frame/resource contract: one typed frame can carry
   native multiplanar storage, color/HDR metadata, an acquire dependency, and
   a release callback carrying the consumer's completion token.
2. **GStreamer `gtk4paintablesink`** for the closest Rust implementation:
   typed system-memory/GL/DMA-BUF variants, source-buffer leases, color/sync
   propagation, and exact fourcc/modifier negotiation.
3. **libplacebo/mpv** for the mapper and renderer boundary: describe planes, color
   representation, crop, and acquire/release behavior; let one renderer plan
   sampling, scaling, color conversion, tone mapping, and output conversion.
4. **Qt Multimedia** for a compact implementation close to Neomacs' scale:
   common plane descriptions and shaders above small VAAPI, VideoToolbox, and
   D3D11 adapters, with frame-slot-aware resource retention.
5. **FFmpeg hardware contexts** for the native-frame schema: distinguish the
   hardware surface type from its software pixel layout and carry all objects,
   layers, planes, offsets, pitches, modifiers, layouts, and synchronization.
6. **Firefox/WebRender and WebKit** for external-image host separation,
   cross-process native-handle transport, native-overlay opportunities, and
   explicit fallbacks.
7. **GStreamer core/GL** for capability negotiation and ordered fallback.
   `memory:DMABuf`, `DMA_DRM`, the
   DRM fourcc/modifier pair, and per-plane metadata must be negotiated rather
   than inferred.

The recurring invariant is more important than the API names:

> A decoded frame remains in its native GPU representation until a consumer
> chooses a measured path.  The frame owns its metadata and producer lease;
> reuse is allowed only after the consumer signals completion.

"Zero-copy sampling", "GPU conversion", and "direct scanout" are different
outcomes.  An imported NV12 frame can be sampled without a pixel copy but still
be composited by Neomacs.  A hardware video processor can copy/convert without
touching the CPU.  An overlay or direct-scanout plane can bypass Neomacs'
render pass entirely.  The API and diagnostics should keep these cases
distinct.

## Implementation status in Neomacs

The first native-YUV implementation described by this research now has one
renderer-facing packed/bi-planar sample contract and typed format,
colorimetry, geometry, transfer-path, and lifetime boundaries.

| Platform | Implemented path | Honest transfer classification |
| --- | --- | --- |
| Linux | Prefer the P010/NV12 `DMA_DRM` formats supported by the renderer, retain the GStreamer buffer, import the modifier-aware multi-planar Vulkan image, and sample Plane0/Plane1 in the final wgpu draw.  Packed DMA-BUF and CPU upload remain fallbacks. | `DirectExternalSurface` only when decoder and compositor DRM identities match exactly and no postprocessor is present; otherwise `GpuInteropCopy` or `CpuUpload` |
| macOS | Wait for the active AVFoundation track metadata, select matching NV12/P010 and limited/full-range CoreVideo output, wrap its luma/chroma planes as Metal textures, and retain the native lease through GPU retirement.  Reconfigure P010 to NV12 and then an explicit non-wide-color BGRA/sRGB fallback after importer rejection without replacing the player. | `GpuInteropCopy`: Metal plane wrapping does not map or upload pixels, but `AVPlayerItemVideoOutput` does not expose enough per-frame source identity to prove its decoder-side output was direct |
| Windows | Ask Media Engine frame-server mode for NV12 when wgpu exposes it, copy each successful stream tick immediately into one pooled D3D11-on-12 multi-planar texture, read and refresh Media Foundation color metadata, and sample its DXGI planes in the final draw.  If NV12 setup, allocation, wrapping, or transfer fails, recreate the Media Engine with a typed BGRA target while preserving playback state. | `GpuInteropCopy`: Microsoft documents `TransferVideoFrame` as a blit/copy even when its output remains NV12; the pooled destination lease freezes the pixels associated with the published PTS and records the copy before any later frame replacement or drop |

The shared shader handles NV12/P010 normalization, limited/full range,
BT.601/709/2020 matrices and primaries, sRGB/BT.709/PQ/HLG transfer functions,
and a bounded HDR-to-SDR mapping.  Diagnostics count actual direct, GPU-copy,
and CPU-upload outcomes, reported byte volume, and importer backpressure; they
do not invent byte counts for opaque driver-side conversions.

macOS and Windows expose only a GPU-copy contract today, so a transfer policy
that requires a direct external surface rejects those platforms during video
system construction.  This prevents a decoder-side copy from occurring before
the common per-frame policy check can observe it.

Ordinary inline video performs conversion while drawing into the final target.
The legacy shader-surface channel interface can expose only one packed texture,
so a native bi-planar frame is materialized into a pooled sRGB texture only for
that consumer.  This keeps the compatibility cost local and makes it visible in
GPU resource accounting instead of silently dropping the channel.

Still-separate future optimizations include decoder-owned D3D11 surfaces on
Windows, exact producer/importer modifier-capability negotiation on Linux,
native overlay promotion, display-aware HDR output, and measured automatic
path selection.  These are not prerequisites for the common native-YUV
sampling seam and should not be mislabeled as already implemented.

Windows implementation references:

- [Media Engine frame-server output format](https://learn.microsoft.com/en-us/windows/win32/medfound/mf-media-engine-video-output-format)
- [`IMFMediaEngine::TransferVideoFrame` blit contract](https://learn.microsoft.com/en-us/windows/win32/api/mfmediaengine/nf-mfmediaengine-imfmediaengine-transfervideoframe)
- [Microsoft WebRTC sample configuring Media Engine for NV12](https://github.com/microsoft/WebRTC-universal-samples/blob/master/Samples/ChatterBox-Sample/ChatterBoxClient.Universal.BackgroundRenderer/Renderer.cpp)

## Ranked reference projects

| Rank | Project | Most reusable lesson | Important caveat |
| --- | --- | --- | --- |
| 1 | Chromium/Viz | Typed shared frames, acquire/release synchronization, multiplanar YUV resources, compositor and overlay fallback | Very large multi-process framework; copy the contracts, not `SharedImage` itself |
| 2 | GStreamer `gtk4paintablesink` | Closest Rust blueprint: typed memory domains, native-buffer leases, exact import-capability negotiation | GDK import can still hide a driver copy; it is not an explicit Vulkan synchronization reference |
| 3 | libplacebo/mpv | Deep mapper and color-aware renderer interfaces over arbitrary planes; explicit acquire/release and robust fallback | It owns a rendering stack and is not a drop-in abstraction for wgpu |
| 4 | Qt Multimedia | Small cross-platform texture-helper seam, native plane wrapping on macOS/Linux, common YUV/P010 shaders, frame-slot lifetime | Its Windows FFmpeg bridge currently performs extra texture copies |
| 5 | FFmpeg `AVHWFramesContext` | Mature vocabulary for devices, pools, hardware formats, plane layouts, mapping, transfer, and synchronization | It describes/interoperates with frames; it is not the compositor |
| 6 | Firefox/WebRender | Platform `RenderTextureHost` adapters, external-image locking, resource reuse, DirectComposition/Core Animation/Wayland paths | Some paths use video-processor blits or platform overlays rather than in-scene shader composition |
| 7 | WebKit/GStreamer | Native `CVPixelBuffer`/IOSurface transport and adaptive GStreamer caps/fallback construction | WebKit's rendering paths vary significantly by port and API |

## Chromium/Viz: the strongest end-to-end contract

Chromium's `VideoFrame` separates storage from semantic frame information.  A
frame may be CPU-backed, a GPU memory buffer, or an opaque `ClientSharedImage`;
multiplanar formats include NV12 and P010.  `WrapSharedImage` accepts the
producer's acquire sync token and a release callback.  The callback receives
the final consumer token, so the producer surface is not recycled merely when
the frame object leaves decoder code.

Sources:

- [Chromium `VideoFrame`](https://chromium.googlesource.com/chromium/src/+/HEAD/media/base/video_frame.h)
- [`VideoFrame` shared-image implementation](https://chromium.googlesource.com/chromium/src/media/+/refs/heads/main/base/video_frame.cc)
- [Acquire/release synchronization tests](https://chromium.googlesource.com/chromium/src/+/HEAD/media/base/video_frame_unittest.cc)

`VideoResourceUpdater` is a useful boundary between decoder-owned frames and
compositor-owned resources.  It passes existing hardware `SharedImage`s
through when possible, propagates color space and HDR metadata, waits on the
acquire token, and returns the final release token to the frame.  Copy/upload
paths exist, but are explicit alternatives and use reusable resources.

Source: [Chromium `VideoResourceUpdater`](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/media/renderers/video_resource_updater.cc).

The compositor representation remains YUV-aware.  `VideoLayerImpl` can emit a
YUV video quad referencing separate planes; `SkiaRenderer` consumes its video
color space and performs YUV/color processing during composition.  Eligible
native video resources can instead be promoted to an overlay, while ordinary
composition remains the fallback.

Sources:

- [`VideoLayerImpl`](https://chromium.googlesource.com/chromium/src/+/HEAD/cc/layers/video_layer_impl.cc)
- [`YUVVideoDrawQuad`](https://chromium.googlesource.com/chromium/src/+/HEAD/components/viz/common/quads/yuv_video_draw_quad.h)
- [Viz `SkiaRenderer`](https://chromium.googlesource.com/chromium/src/+/HEAD/components/viz/service/display/skia_renderer.cc)

The platform decoders preserve the same contract:

- Linux VAAPI exports decoded NV12 surfaces as native-pixmap DMA-BUF GPU
  buffers rather than downloading them.
- VideoToolbox wraps the decoded IOSurface as a multiplanar `SharedImage`,
  retains the `CVImageBuffer` until the release sync token completes, and
  forwards color and HDR metadata.
- D3D11 decoding wraps the picture-buffer texture as a `SharedImage`, attaches
  a release callback before returning the picture buffer to the decoder pool,
  and marks supported NV12/P010 resources as overlay candidates.

Sources:

- [Chromium VAAPI decoder](https://chromium.googlesource.com/chromium/src/+/HEAD/media/gpu/vaapi/vaapi_video_decoder.cc)
- [Chromium VideoToolbox frame converter](https://chromium.googlesource.com/chromium/src/+/HEAD/media/gpu/mac/video_toolbox_frame_converter.cc)
- [Chromium D3D11 decoder](https://chromium.googlesource.com/chromium/src/+/HEAD/media/gpu/windows/d3d11_video_decoder.cc)

Design lesson: Neomacs should copy the *shape* of `VideoFrame` plus
`VideoResourceUpdater`: an immutable semantic descriptor, opaque platform
storage, acquire synchronization, and a release lease.  It should not copy
Chromium's mailbox, command-buffer, or process architecture.

## libplacebo and mpv: the strongest renderer boundary

libplacebo's `pl_frame` describes multiple image planes plus the color
representation, color space, crop, optional ICC/LUT/HDR information, and
`acquire`/`release` callbacks.  The high-level `pl_render_image` interface
accepts that input and a target frame, then plans the required sampling,
scaling, color conversion, tone mapping, and output work.  Its documented
release callback is invoked after a plane is no longer used by the GPU,
including error paths.

Sources:

- [libplacebo high-level renderer and `pl_frame`](https://github.com/haasn/libplacebo/blob/master/src/include/libplacebo/renderer.h)
- [libplacebo FFmpeg frame interop](https://github.com/haasn/libplacebo/blob/master/src/include/libplacebo/utils/libav.h)
- [libplacebo API layering](https://github.com/haasn/libplacebo#api-overview)

mpv's default `gpu-next` output builds on that interface.  It keeps hardware
decode methods separate from their `-copy` counterparts, probes supported
interop at runtime, and falls back to software decoding after repeated
hardware failures.  This is a better policy model than labeling every
GPU-decoded path "zero copy".

Sources:

- [mpv `vo_gpu_next`](https://github.com/mpv-player/mpv/blob/master/video/out/vo_gpu_next.c)
- [mpv hardware-decoding options and fallbacks](https://github.com/mpv-player/mpv/blob/master/DOCS/man/options.rst)
- [mpv output-driver documentation](https://github.com/mpv-player/mpv/blob/master/DOCS/man/vo.rst)

mpv's `dmabuf-wayland` output is an instructive alternative: it can avoid
GPU-to-CPU copies and ask fixed-function hardware to scale/convert, improving
power use, but its documentation explicitly trades rendering quality and
flexibility for that efficiency.  In an editor, direct scanout is only legal
when no Neomacs content must blend above, transform with, clip, or filter the
video.  It should therefore be an optional terminal path, not the core frame
representation.

Design lesson: model a video input richly enough that a renderer can make one
global decision.  Do not force every backend to manufacture a compositor-ready
BGRA texture before the renderer sees the frame.

mpv's `ra_hwdec_mapper` is especially close to the seam Neomacs needs.  A
mapper owns the source frame, exposes up to four destination plane textures,
and has explicit map/unmap lifetime.  It may release the source early only if
the mapping operation made an independent copy.  `vo_gpu_next` acquires native
surfaces lazily, measures hardware mapping, carries color/HDR/chroma metadata,
and falls back to reusable software-plane uploads.

Sources:

- [mpv hardware-decoder mapper contract](https://github.com/mpv-player/mpv/blob/02a595ddc1b9b39aa7b0366cab58be3734a4a4eb/video/out/gpu/hwdec.h#L52-L131)
- [mpv lazy acquisition and upload fallback](https://github.com/mpv-player/mpv/blob/02a595ddc1b9b39aa7b0366cab58be3734a4a4eb/video/out/vo_gpu_next.c#L655-L793)

The platform mappers make the tradeoffs concrete:

- Linux imports every DMA-BUF plane with its fd, offset, pitch, and modifier.
- macOS creates one `CVMetalTexture` per `CVPixelBuffer` plane and retains the
  underlying objects.
- Vulkan wraps the real plane aspects and transfers layout/ownership through
  timeline semaphore values.
- Windows exposes NV12/P010 as R/RG planes, but defaults to one GPU copy;
  direct decoder-surface sampling is opt-in because padding and driver bugs can
  make nominal zero-copy unreliable.

Sources:

- [mpv DMA-BUF/libplacebo interop](https://github.com/mpv-player/mpv/blob/02a595ddc1b9b39aa7b0366cab58be3734a4a4eb/video/out/hwdec/dmabuf_interop_pl.c#L25-L137)
- [mpv VideoToolbox/Metal interop](https://github.com/mpv-player/mpv/blob/02a595ddc1b9b39aa7b0366cab58be3734a4a4eb/video/out/hwdec/hwdec_vt_pl.m#L214-L310)
- [mpv Vulkan frame interop](https://github.com/mpv-player/mpv/blob/02a595ddc1b9b39aa7b0366cab58be3734a4a4eb/video/out/hwdec/hwdec_vulkan.c#L230-L369)
- [mpv D3D11VA mapper](https://github.com/mpv-player/mpv/blob/02a595ddc1b9b39aa7b0366cab58be3734a4a4eb/video/out/d3d11/hwdec_d3d11va.c#L180-L295)
- [mpv direct-mode safety caveat](https://github.com/mpv-player/mpv/blob/02a595ddc1b9b39aa7b0366cab58be3734a4a4eb/DOCS/man/options.rst#L6482-L6492)

libplacebo does not promise that every frame becomes exactly one GPU pass.
Its renderer merges plane reconstruction and color decoding into generated
shader stages, but introduces intermediates when scaling quality, HDR work, or
other enabled features require them.  That is the correct goal for Neomacs:
minimize unnecessary materialization while preserving global optimization and
correctness.

Sources:

- [libplacebo renderer pipeline](https://github.com/haasn/libplacebo/blob/41ac2980e1a898f41d7c2e07999f9862ee89d99f/src/renderer.c#L1671-L2092)
- [libplacebo bounded lazy frame queue](https://github.com/haasn/libplacebo/blob/41ac2980e1a898f41d7c2e07999f9862ee89d99f/src/include/libplacebo/utils/frame_queue.h#L24-L217)

## FFmpeg: the best native-frame vocabulary

`AVHWFramesContext` distinguishes the hardware surface format from the actual
software pixel layout, owns a frame pool tied to a hardware device, and exposes
explicit mapping and transfer operations.  Compatibility is queried; a failed
hardware mapping is not silently treated as ordinary CPU memory.

Sources:

- [FFmpeg hardware-context API](https://ffmpeg.org/doxygen/trunk/hwcontext_8h_source.html)
- [`AVHWFramesContext`](https://ffmpeg.org/doxygen/trunk/structAVHWFramesContext.html)

FFmpeg's `AV_HWFRAME_MAP_DIRECT` flag is an important counterexample to
"zero-copy is always faster": it guarantees that map/unmap itself will not
copy, but the API documentation warns that direct mappings can be slower than
an indirect mapping.  Transfer policy must therefore be capability- and
measurement-driven, not encoded as a universal preference detached from the
workload.

Source: [FFmpeg direct hardware-frame mapping contract](https://github.com/FFmpeg/FFmpeg/blob/c9e36046a338638279782cba4fba3299bf65f46b/libavutil/hwcontext.h#L515-L574).

Its platform frame descriptions show which information a future Neomacs ABI
must not omit:

- `AVDRMFrameDescriptor` represents objects separately from layers and planes,
  with object indices, offsets, pitches, and format modifiers.  One fd per
  plane must not be assumed.
- `AVVkFrame` carries image layout, access state, per-image timeline semaphore,
  and the value consumers must wait for and increment when signaling.
- `AVD3D11VADeviceContext` makes device identity, bind flags, and locking of
  immediate/video contexts explicit.

Sources:

- [FFmpeg DRM frame descriptor](https://ffmpeg.org/doxygen/trunk/hwcontext__drm_8h_source.html)
- [`AVVkFrame` synchronization contract](https://www.ffmpeg.org/doxygen/8.0/structAVVkFrame.html)
- [`AVD3D11VADeviceContext`](https://ffmpeg.org/doxygen/trunk/structAVD3D11VADeviceContext.html)

Design lesson: the next Neomacs video ABI should use a tagged native-frame
descriptor with bounded arrays of objects and planes plus a platform-specific
synchronization payload.  A flat `{ fd, stride, offset }` tuple or a raw native
pointer is not a durable cross-platform contract.

## Qt Multimedia: the most directly reusable implementation

Qt uses one texture-description table for packed and multiplanar formats.
NV12 is two R8/RG8 planes and P010/P016 is two R16/RG16 planes.  Its shared
video shader selection handles NV12, P010/P016, BT.601/709/2020, full/video
range, PQ, and HLG rather than making each platform backend convert to BGRA.

Sources:

- [Qt `QVideoTextureHelper`](https://github.com/qt/qtmultimedia/blob/dev/src/multimedia/video/qvideotexturehelper.cpp)
- [`QVideoFrameFormat` color and pixel model](https://doc.qt.io/qt-6/qvideoframeformat.html)

The platform adapters are small:

- The VAAPI adapter exports a `VADRMPRIMESurfaceDescriptor`, imports every
  plane through EGL with its fd, offset, pitch, and modifier, and returns
  texture handles to the common renderer.
- The VideoToolbox adapter keeps the `CVPixelBuffer` alive and creates a Metal
  texture for every plane with `CVMetalTextureCacheCreateTextureFromImage`.
- The D3D11 adapter demonstrates a path *not* to copy: its device bridge copies
  a decoder-pool slice into a shared texture and then into another
  shader-resource texture.  It is compatible, but it is not zero-copy.

Sources:

- [Qt VAAPI texture adapter](https://github.com/qt/qtmultimedia/blob/dev/src/plugins/multimedia/ffmpeg/qffmpeghwaccel_vaapi.cpp)
- [Qt VideoToolbox/Metal adapter](https://github.com/qt/qtmultimedia/blob/dev/src/plugins/multimedia/ffmpeg/darwin/qffmpeghwaccel_videotoolbox.mm)
- [Qt D3D11 adapter](https://github.com/qt/qtmultimedia/blob/dev/src/plugins/multimedia/ffmpeg/qffmpeghwaccel_d3d11.cpp)

Qt's texture pool is also a useful small lifetime example.  It retains old
textures per RHI frame slot and releases upload source mappings only after
`endFrame`, rather than assuming command submission consumed the data
immediately.

Sources:

- [Qt video-frame texture pool](https://github.com/qt/qtmultimedia/blob/dev/src/multimedia/video/qvideoframetexturepool.cpp)
- [Qt source-frame retention through frame end](https://github.com/qt/qtmultimedia/blob/dev/src/multimedia/video/qvideoframetexturefromsource.cpp)

Design lesson: `QVideoTextureHelper` is the closest implementation-scale model
for Neomacs' `sampling` module.  Adopt the common format/color pipeline and
frame-retirement discipline, while avoiding Qt's D3D11 copy bridge.

## Firefox/WebRender: external hosts and native composition

Firefox places platform storage behind `RenderTextureHost` implementations.
Linux has a DMA-BUF host with plane-aware `Lock`/`Unlock`; macOS has an
IOSurface host; Windows has D3D11/DXGI and DirectComposition hosts.  A wrapper
keeps expensive host initialization reusable when multiple logical video
textures refer to the same underlying resource.

Sources:

- [WebRender texture-host implementations](https://searchfox.org/firefox-main/source/gfx/webrender_bindings/)
- [`RenderDMABUFTextureHost`](https://searchfox.org/firefox-main/source/gfx/webrender_bindings/RenderDMABUFTextureHost.h)
- [`RenderTextureHostWrapper`](https://searchfox.org/firefox-main/source/gfx/webrender_bindings/RenderTextureHostWrapper.h)

Firefox's Linux DMA-BUF surface keeps plane count, fds, strides, offsets,
format, and fence state together, and recognizes native NV12/P010 plane
layouts.  Its Windows `DCLayerTree` selects between ordinary composition and a
native video surface, maps YUV range/color space to DXGI color spaces, and uses
the video processor where conversion is necessary.  Its macOS native layer
retains an IOSurface use-count until presentation no longer needs it.

Sources:

- [Firefox `DMABufSurface`](https://searchfox.org/firefox-main/source/widget/gtk/DMABufSurface.cpp)
- [Firefox DirectComposition video path](https://searchfox.org/firefox-main/source/gfx/webrender_bindings/DCLayerTree.cpp)
- [Firefox Core Animation native layer](https://searchfox.org/firefox-main/source/gfx/layers/NativeLayerCA.h)

Design lesson: keep platform handle import and native-overlay promotion behind
the same external-image boundary.  A caller should request a prepared video
sample; it should not branch on DMA-BUF, IOSurface, or D3D11 types.

## WebKit: native transport plus adaptive GStreamer fallback

On Cocoa, `VideoFrameCV` preserves a `CVPixelBuffer` and platform color space.
For GPU-process transport, WebKit sends an IOSurface Mach right when the pixel
buffer is IOSurface-backed; otherwise it falls back to synchronized shared
memory.  The receiving side reconstructs a `CVPixelBuffer` from the IOSurface.

Sources:

- [WebKit `VideoFrameCV`](https://github.com/WebKit/WebKit/blob/main/Source/WebCore/platform/graphics/cv/VideoFrameCV.h)
- [WebKit `SharedVideoFrame`](https://github.com/WebKit/WebKit/blob/main/Source/WebKit/WebProcess/GPU/webrtc/SharedVideoFrame.cpp)

WebKit's Cocoa graphics path demonstrates direct plane use: it attaches the Y
and UV planes of a bi-planar IOSurface as separate textures and performs YUV
sampling in a shader.  Unsupported input formats are explicitly conformed to a
supported bi-planar format rather than being misinterpreted.

Source: [WebKit `GraphicsContextGLCVCocoa`](https://github.com/WebKit/WebKit/blob/main/Source/WebCore/platform/graphics/cv/GraphicsContextGLCVCocoa.mm).

On GStreamer ports, `VideoFrameGStreamer` distinguishes system, GL, and
DMA-BUF memory.  `GLVideoSinkGStreamer` prefers DMA-BUF and GL-memory caps; it
dynamically inserts `glupload`/`glcolorconvert` only when the producer cannot
supply a compatible GPU representation.  Its appsink holds one current buffer
and an upstream bounded queue, making backpressure policy visible.

Sources:

- [WebKit `VideoFrameGStreamer`](https://github.com/WebKit/WebKit/blob/main/Source/WebCore/platform/graphics/gstreamer/VideoFrameGStreamer.h)
- [WebKit GL video sink](https://github.com/WebKit/WebKit/blob/main/Source/WebCore/platform/graphics/gstreamer/GLVideoSinkGStreamer.cpp)

Design lesson: native handle transport needs a defined slower fallback, and
conversion elements should be inserted only after capability negotiation says
they are needed.

## GStreamer `gtk4paintablesink`: the closest Rust blueprint

The Rust `gtk4paintablesink` implementation is unusually close to Neomacs' use
case.  It represents mapped frames as typed system-memory, GL, or DMA-BUF
variants.  Its DMA-BUF plane descriptors retain the original `GstBuffer`
through the imported texture's release callback; its GL path transports
`GstGLSyncMeta`; and it maps CICP color state instead of discarding it.

Sources:

- [`gtk4paintablesink` typed frame model](https://github.com/GStreamer/gst-plugins-rs/blob/0e66dda74755c1adfaf5efb14e0cf4566469e644/video/gtk4/src/sink/frame.rs#L75-L96)
- [DMA-BUF, GL, and system-memory import ladder](https://github.com/GStreamer/gst-plugins-rs/blob/0e66dda74755c1adfaf5efb14e0cf4566469e644/video/gtk4/src/sink/frame.rs#L565-L905)
- [Native buffer leases, synchronization, and color state](https://github.com/GStreamer/gst-plugins-rs/blob/0e66dda74755c1adfaf5efb14e0cf4566469e644/video/gtk4/src/sink/frame.rs#L319-L625)

The sink queries the active renderer's DMA-BUF formats and modifiers and
advertises only those exact pairs upstream.  This is stronger than accepting
all `memory:DMABuf` caps and discovering at presentation time that the
modifier cannot be imported.

Source: [`gtk4paintablesink` DMA-BUF capability negotiation](https://github.com/GStreamer/gst-plugins-rs/blob/0e66dda74755c1adfaf5efb14e0cf4566469e644/video/gtk4/src/sink/imp.rs#L891-L953).

Design lesson: use its Rust enum/lease structure as the nearest implementation
reference.  Do not treat successful GDK DMA-BUF import as proof of direct
scanout or absence of a hidden driver copy; retain Neomacs' truthful transfer
diagnostics.

## GStreamer: negotiate the complete memory layout

GStreamer's DMA-BUF design treats DRM fourcc plus modifier as one negotiated
format.  `GstVideoInfoDmaDrm` carries both, while `GstVideoMeta` carries plane
offsets and strides.  The documentation warns that even mapping a *linear*
DMA-BUF can be slow because of memory type and coherency, and recommends
keeping the buffer in device memory.

Sources:

- [GStreamer DMA-BUF design](https://gstreamer.freedesktop.org/documentation/additional/design/dmabuf.html)
- [`GstVideoInfoDmaDrm`](https://gstreamer.freedesktop.org/documentation/video/video-info-dma-drm.html)
- [`glupload` accepted memory domains](https://gstreamer.freedesktop.org/documentation/opengl/glupload.html)

The key lesson for the Neomacs plugin caps is that `NV12` alone is
insufficient.  The downstream importer must advertise the exact
fourcc/modifier pairs it can import; negotiated caps and `GstVideoMeta` then
provide the objects/planes/strides/offsets.  Packed AR24/AB24 should remain a
fallback, not the preferred hardware-decoder output.

GStreamer also provides latency and resource-usage tracers.  They are useful
for separating decoder latency from queueing, conversion, and sink latency
before changing the pipeline.

Sources:

- [GStreamer latency tracer](https://gstreamer.freedesktop.org/documentation/coretracers/latency.html)
- [GStreamer tracing design](https://gstreamer.freedesktop.org/documentation/additional/design/tracing.html)

`GstGLUpload` is the best small reference for the fallback policy itself.  It
orders passthrough/direct import strategies before raw upload, and an
implementation selected during caps negotiation may still reject an
individual buffer, causing the uploader to advance/reconfigure rather than
misuse it.

Sources:

- [`GstGLUpload` strategy ordering](https://github.com/GStreamer/gstreamer/blob/6870264976525c6dc0a4c2bc52ba9bd83888e061/subprojects/gst-plugins-base/gst-libs/gst/gl/gstglupload.c#L3751-L3777)
- [`GstGLUpload` per-buffer fallback](https://github.com/GStreamer/gstreamer/blob/6870264976525c6dc0a4c2bc52ba9bd83888e061/subprojects/gst-plugins-base/gst-libs/gst/gl/gstglupload.c#L4082-L4140)

## wgpu constraints that shape the Neomacs design

wgpu 30 exposes NV12 and P010 texture formats and an `ExternalTexture` model.
The external-texture descriptor includes YUV, gamut, and transfer transforms,
but it consumes already-created plane texture views; it does not import a
CVPixelBuffer, DMA-BUF, or D3D11 decoder surface by itself.  In wgpu 30 the
`EXTERNAL_TEXTURE` feature is documented for DX12 and Metal, not Vulkan.

Sources:

- [wgpu `ExternalTextureFormat`](https://docs.rs/wgpu/latest/wgpu/enum.ExternalTextureFormat.html)
- [wgpu `ExternalTextureDescriptor`](https://docs.rs/wgpu/latest/wgpu/type.ExternalTextureDescriptor.html)
- [wgpu `EXTERNAL_TEXTURE` feature source](https://github.com/gfx-rs/wgpu/blob/v30.0.0/wgpu-types/src/features.rs#L1033-L1045)

The Vulkan DMA-BUF helper currently cannot describe a multiplanar NV12/P010
buffer: its API accepts only one offset and row pitch.  Upstream issue #9801
identifies the missing per-plane `VkSubresourceLayout` interface and describes
raw `ash` plus `texture_from_raw` as the present integration route.

Source: [wgpu multiplanar DMA-BUF issue #9801](https://github.com/gfx-rs/wgpu/issues/9801).

Design lesson: do not make wgpu's current capability set the semantic frame
model.  Keep raw Vulkan/Metal/D3D interop inside platform importers, then
produce a renderer-private packed or multiplanar sample.  Use wgpu
`ExternalTexture` on supported backends when it simplifies the implementation,
but preserve a custom two-plane shader path and a packed fallback.

## Is composing video in the wgpu shader the right architecture?

**Yes for inline editor video, with one important qualification:** the final
Neomacs wgpu composition pass is the right place to sample video, but the
texture reaching that pass should ideally still be native NV12/P010.  Painting
a packed BGRA/RGBA texture in wgpu is a correct compatibility path; forcing an
upstream full-frame YUV-to-packed conversion before that draw is the avoidable
cost.

At the start of this research, the distinction by platform was:

| Platform | Current Neomacs fast path | Ideal inline wgpu path | Useful non-wgpu terminal path |
| --- | --- | --- | --- |
| Linux | GStreamer produces packed AR24/AB24 DMA-BUF; Vulkan imports it; wgpu samples packed RGB | Negotiate NV12/P010 `DMA_DRM`, raw-Vulkan import the Y/UV planes and modifier, then convert YUV while scaling/clipping/blending in the final wgpu draw | Wayland/KMS overlay or direct scanout when nothing must blend above the video |
| macOS | AVFoundation is asked for BGRA; the `CVPixelBuffer` is wrapped as a Metal/wgpu texture | Request 420v/420f or 10-bit bi-planar output, wrap each plane with `CVMetalTextureCache`, then sample through wgpu `ExternalTexture` or a two-plane shader | Core Animation/AVSampleBufferDisplayLayer video layer when editor composition semantics permit |
| Windows | MediaEngine `TransferVideoFrame` blits/converts into a BGRA D3D resource sampled by wgpu | Obtain decoder-owned D3D11 NV12/P010 textures, expose R/RG plane views to D3D12/wgpu, retain/release with a fence, then convert in the final draw | DirectComposition video surface/overlay when eligible; one GPU copy remains a prudent fallback for affected drivers |

This answer is deliberately not "always one shader pass."  Ordinary inline
video with crop, transform, opacity, and UI blending should normally fuse YUV
decode with the final draw.  High-quality scaling, deinterlacing, HDR peak
detection/tone mapping, temporal filters, reuse by multiple consumers, or
native-overlay constraints may justify an intermediate or fixed-function video
processor.  libplacebo and mpv both make that a planned runtime decision.

An overlay/direct-scanout route can use less power than wgpu composition, but
it cannot be the semantic default for Neomacs: editor text, clipping, window
transforms, opacity, animation, and effects may need to compose with the video.
The correct architecture is therefore **wgpu composition as the universal
inline path, native overlays as an optional optimization, and packed GPU/CPU
fallbacks as truthful lower tiers**.

## Recommended Neomacs synthesis

### 1. Deepen the frame contract before optimizing platform code

Replace the packed-only sampling assumption with exhaustive types conceptually
equivalent to:

```rust
enum VideoFrameFormat {
    Packed(PackedFormat),
    BiPlanar420(BiPlanarFormat), // NV12 or P010
}

struct VideoColorimetry {
    matrix: MatrixCoefficients,
    range: ColorRange,
    primaries: ColorPrimaries,
    transfer: TransferFunction,
    chroma_siting: ChromaSiting,
}

enum PreparedVideoSample {
    Packed(PackedSample),
    BiPlanar(BiPlanarSample),
}
```

The native transport descriptor should separately represent memory objects and
image planes, including modifiers, offsets, strides, coded/visible geometry,
adapter identity, acquire synchronization, and an owned release lease.  Rust
exhaustiveness should force every importer and shader pipeline to handle or
reject each format.

### 2. Keep one renderer-facing seam

The renderer should receive `PreparedVideoSample`, not a platform enum.  The
sampling module chooses a packed or bi-planar bind-group/pipeline variant and
returns the same draw-level geometry and lifetime interface to
`neomacs-renderer-wgpu`.  DMA-BUF, CVPixelBuffer, IOSurface, D3D11, and COM
types remain private to platform adapters.

### 3. Implement native YUV in this order

1. **macOS first:** request 420v/420f and then P010-capable buffers, create
   R8/RG8 or R16/RG16 Metal plane textures through
   `CVMetalTextureCacheCreateTextureFromImage`, and retain the pixel buffer
   until GPU retirement.  Qt and Chromium provide direct templates.
2. **Linux second:** negotiate NV12/P010 `DMA_DRM` plus exact modifier, import
   every plane with raw Vulkan `ash`, and wrap safe renderer-facing plane views.
   Qt, GStreamer, FFmpeg, and Chromium collectively cover the descriptor and
   negotiation rules.  Packed DMA-BUF remains the first fallback.
3. **Windows third:** create the decoder on the compositor's adapter and expose
   decoder-owned NV12/P010 D3D11 textures.  Use shared handles/fences or a
   D3D11-on-D3D12 interop representation, and return the surface only after GPU
   completion.  Chromium is the target contract; Qt's copy bridge is a useful
   negative comparison.  MediaEngine `TransferVideoFrame` remains a compatible
   GPU-copy fallback until direct decoder surfaces replace it.

### 4. Make the fallback ladder an observed result

Preserve the current policy distinction and extend diagnostics:

1. native YUV sampled in the Neomacs compositor;
2. native overlay/direct scanout when composition constraints permit;
3. GPU video-processor or shader conversion into a packed texture;
4. CPU upload;
5. software decode if hardware decode fails.

Record the actual path, decoded format, modifier, adapter match, copied/uploaded
bytes, pool pressure, late/replaced frames, GPU duration, and end-to-end
latency.  Never report "zero copy" solely because decode was hardware
accelerated.

### 5. Optimize by eliminating intermediate materialization, not GPU work in general

YUV conversion arithmetic is usually inexpensive when fused with the final
draw.  The avoidable cost is reading native YUV, writing a full packed RGB
intermediate, synchronizing it, and reading it again for composition.  Some
fixed-function video processors or overlays can still win on power and
bandwidth, so choose from measured capabilities rather than declaring shader
conversion universally superior.

Benchmark at least 1080p60, 4K60 8-bit, 4K60 10-bit, multiple simultaneous
videos, integrated/discrete GPUs, window occlusion, scaling, editor overlays,
and HDR/SDR output.  The acceptance criteria should include color correctness,
late-frame rate, GPU time, memory bandwidth, CPU time, and power—not only
frames per second.

## Practical conclusion

The ideal Neomacs design is **Chromium's resource lifetime contract, mpv's
mapper seam, and libplacebo's color-aware renderer interface, expressed with
`gtk4paintablesink`-style Rust variants and leases at Qt's implementation
scale**.  FFmpeg/GStreamer supply the native descriptor, negotiation, and
fallback vocabulary; Firefox/WebKit demonstrate platform-host and overlay
paths.

The format/color/lifetime ABI and the three native sampling paths are now in
place.  The next follow-ups should be exact Linux modifier-capability
negotiation, decoder-owned Windows surface import, measured path selection,
and optional overlay promotion.  Each can deepen the shared contract without
forking the renderer-facing model by platform.
