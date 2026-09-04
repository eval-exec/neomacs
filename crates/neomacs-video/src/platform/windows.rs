//! Media Foundation playback and GPU-only D3D11-on-12 frame import.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use neomacs_display_protocol::types::VideoId;
use windows::Win32::Foundation::{RECT, RPC_E_CHANGED_MODE, S_FALSE};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    ID3D11Device, ID3D11DeviceContext, ID3D11Resource,
};
use windows::Win32::Graphics::Direct3D11on12::{
    D3D11_RESOURCE_FLAGS, D3D11On12CreateDevice, ID3D11On12Device,
};
use windows::Win32::Graphics::Direct3D12::{
    D3D12_CPU_PAGE_PROPERTY_UNKNOWN, D3D12_HEAP_FLAG_NONE, D3D12_HEAP_PROPERTIES,
    D3D12_HEAP_TYPE_DEFAULT, D3D12_MEMORY_POOL_UNKNOWN, D3D12_RESOURCE_DESC,
    D3D12_RESOURCE_DIMENSION_TEXTURE2D, D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
    D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_RENDER_TARGET,
    D3D12_TEXTURE_LAYOUT_UNKNOWN, ID3D12Device, ID3D12Resource,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_TYPELESS, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC,
};
use windows::Win32::Media::MediaFoundation::{
    CLSID_MFMediaEngineClassFactory, IMFAttributes, IMFDXGIDeviceManager, IMFMediaEngine,
    IMFMediaEngineClassFactory, IMFMediaEngineEx, IMFMediaEngineNotify, IMFMediaEngineNotify_Impl,
    MF_MEDIA_ENGINE_CALLBACK, MF_MEDIA_ENGINE_DXGI_MANAGER, MF_MEDIA_ENGINE_EVENT_CANPLAY,
    MF_MEDIA_ENGINE_EVENT_ENDED, MF_MEDIA_ENGINE_EVENT_ERROR, MF_MEDIA_ENGINE_EVENT_FORMATCHANGE,
    MF_MEDIA_ENGINE_EVENT_LOADEDMETADATA, MF_MEDIA_ENGINE_REAL_TIME_MODE,
    MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT, MF_MT_FRAME_SIZE, MF_MT_TRANSFER_FUNCTION,
    MF_MT_VIDEO_CHROMA_SITING, MF_MT_VIDEO_NOMINAL_RANGE, MF_MT_VIDEO_PRIMARIES, MF_MT_YUV_MATRIX,
    MF_VERSION, MFCreateAttributes, MFCreateDXGIDeviceManager, MFNominalRange_0_255,
    MFSTARTUP_FULL, MFShutdown, MFStartup, MFVideoChromaSubsampling_DV_PAL,
    MFVideoChromaSubsampling_MPEG1, MFVideoPrimaries_BT470_2_SysBG, MFVideoPrimaries_BT2020,
    MFVideoPrimaries_EBU3213, MFVideoPrimaries_SMPTE_C, MFVideoPrimaries_SMPTE170M,
    MFVideoTransFunc_2084, MFVideoTransFunc_HLG, MFVideoTransFunc_sRGB,
    MFVideoTransferMatrix_BT601, MFVideoTransferMatrix_BT2020_10, MFVideoTransferMatrix_BT2020_12,
};
use windows::Win32::System::Com::StructuredStorage::{
    PROPVARIANT, PropVariantClear, PropVariantToUInt32,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::core::{BSTR, IUnknown, Interface, implement};

use crate::backend::{
    BackendEvent, CompletedFrameImport, DecodedFrame, DecodedFrameImport, DecoderBackend,
    DecoderOutputGeneration, DecoderOutputRejection, DecoderReconfiguration, FrameImportOutcome,
    FrameImporter, ImportedFrame, Platform, ProductionPlatform, require_fixed_compositor_import,
};
use crate::sampling::{GpuVideoContext, PreparedBiPlanarTexture, PreparedSampledTexture};
use crate::surface_pool::{BoundedSurfacePool, SurfaceLease, SurfacePoolAcquire};
use crate::{
    BiPlanarVideoFormat, FrameTiming, GpuVideoFrame, InitialPlayback, LoopMode, MediaTime,
    PackedVideoFormat, PlaybackAction, PlaybackEpoch, VideoChromaLocation, VideoColorPrimaries,
    VideoColorRange, VideoColorimetry, VideoCommand, VideoCompositorImport, VideoDecodeBackend,
    VideoDecodeResidency, VideoFrameFormat, VideoGeometry, VideoInitError, VideoMatrixCoefficients,
    VideoSessionState, VideoSource, VideoTransferCharacteristic, VideoWake,
};

const EVENT_READY: u32 = 1 << 0;
const EVENT_ENDED: u32 = 1 << 1;
const EVENT_ERROR: u32 = 1 << 2;
const EVENT_FORMAT_CHANGED: u32 = 1 << 3;
const MAX_IN_FLIGHT_VIDEO_SURFACES: usize = 4;
const WINDOWS_MEDIA_POLL_INTERVAL: Duration = Duration::from_micros(8_000);

pub(crate) struct WindowsPlatform;

#[implement(IMFMediaEngineNotify)]
struct MediaEngineNotify {
    pending: Arc<AtomicU32>,
    wake: VideoWake,
}

impl IMFMediaEngineNotify_Impl for MediaEngineNotify_Impl {
    fn EventNotify(&self, event: u32, _param1: usize, _param2: u32) -> windows::core::Result<()> {
        let flag = media_engine_event_flag(event);
        if flag != 0 {
            self.pending.fetch_or(flag, Ordering::Release);
        }
        self.wake.notify();
        Ok(())
    }
}

const fn media_engine_event_flag(event: u32) -> u32 {
    if event == MF_MEDIA_ENGINE_EVENT_ERROR.0 as u32 {
        EVENT_ERROR
    } else if event == MF_MEDIA_ENGINE_EVENT_ENDED.0 as u32 {
        EVENT_ENDED
    } else if event == MF_MEDIA_ENGINE_EVENT_FORMATCHANGE.0 as u32 {
        EVENT_FORMAT_CHANGED
    } else if event == MF_MEDIA_ENGINE_EVENT_LOADEDMETADATA.0 as u32
        || event == MF_MEDIA_ENGINE_EVENT_CANPLAY.0 as u32
    {
        EVENT_READY
    } else {
        0
    }
}

#[derive(Clone)]
struct WindowsGpuBridge {
    d3d12_device: ID3D12Device,
    _d3d11_device: ID3D11Device,
    d3d11_context: ID3D11DeviceContext,
    on12: ID3D11On12Device,
    dxgi_manager: IMFDXGIDeviceManager,
    preferred_output_format: WindowsOutputFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowsOutputFormat {
    Nv12,
    Bgra8,
}

impl WindowsOutputFormat {
    fn select(device: &wgpu::Device) -> Self {
        if device
            .features()
            .contains(wgpu::Features::TEXTURE_FORMAT_NV12)
        {
            Self::Nv12
        } else {
            Self::Bgra8
        }
    }

    const fn media_engine_dxgi(self) -> windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT {
        match self {
            Self::Nv12 => DXGI_FORMAT_NV12,
            Self::Bgra8 => DXGI_FORMAT_B8G8R8A8_UNORM,
        }
    }

    const fn resource_dxgi(self) -> windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT {
        match self {
            Self::Nv12 => DXGI_FORMAT_NV12,
            // Media Engine renders UNORM while wgpu samples through an sRGB
            // view. A typeless resource is the legal common allocation.
            Self::Bgra8 => DXGI_FORMAT_B8G8R8A8_TYPELESS,
        }
    }

    const fn frame(self) -> VideoFrameFormat {
        match self {
            Self::Nv12 => VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12),
            Self::Bgra8 => VideoFrameFormat::Packed(PackedVideoFormat::Bgra8),
        }
    }

    const fn wgpu(self) -> wgpu::TextureFormat {
        match self {
            Self::Nv12 => wgpu::TextureFormat::NV12,
            Self::Bgra8 => wgpu::TextureFormat::Bgra8UnormSrgb,
        }
    }

    const fn from_frame(format: VideoFrameFormat) -> Option<Self> {
        match format {
            VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12) => Some(Self::Nv12),
            VideoFrameFormat::Packed(PackedVideoFormat::Bgra8) => Some(Self::Bgra8),
            VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010)
            | VideoFrameFormat::Packed(PackedVideoFormat::Rgba8) => None,
        }
    }

    const fn candidates(self) -> &'static [Self] {
        match self {
            Self::Nv12 => &[Self::Nv12, Self::Bgra8],
            Self::Bgra8 => &[Self::Bgra8],
        }
    }

    const fn fallback_after_rejection(self) -> Option<Self> {
        match self {
            Self::Nv12 => Some(Self::Bgra8),
            Self::Bgra8 => None,
        }
    }

    const fn completed_import(self) -> CompletedFrameImport {
        // Media Engine documents TransferVideoFrame as a blit, but does not
        // expose the number of bytes copied by the driver. Destination
        // allocation size is not an observed transfer count.
        CompletedFrameImport::GpuBlit {
            reported_bytes: None,
        }
    }
}

#[derive(Clone, Copy)]
struct WindowsPlaybackConfiguration {
    autoplay: bool,
    rate: f64,
    position: f64,
}

impl WindowsGpuBridge {
    fn new(gpu: &GpuVideoContext) -> Result<Self, String> {
        use wgpu::hal::api::Dx12;
        let (d3d12_device, command_queue) = unsafe {
            let hal = gpu
                .device()
                .as_hal::<Dx12>()
                .ok_or_else(|| "Media Foundation video requires wgpu's DX12 backend".to_string())?;
            (hal.raw_device().clone(), hal.raw_queue().clone())
        };
        let queues = [Some(command_queue.cast::<IUnknown>().map_err(|error| {
            format!("failed to expose the wgpu DX12 queue: {error}")
        })?)];
        let mut d3d11_device = None;
        let mut d3d11_context = None;
        unsafe {
            D3D11On12CreateDevice(
                &d3d12_device,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT.0,
                None,
                Some(&queues),
                0,
                Some(&mut d3d11_device),
                Some(&mut d3d11_context),
                None,
            )
        }
        .map_err(|error| format!("D3D11On12CreateDevice failed: {error}"))?;
        let d3d11_device =
            d3d11_device.ok_or_else(|| "D3D11On12CreateDevice returned no device".to_string())?;
        let d3d11_context =
            d3d11_context.ok_or_else(|| "D3D11On12CreateDevice returned no context".to_string())?;
        let on12 = d3d11_device
            .cast::<ID3D11On12Device>()
            .map_err(|error| format!("D3D11 device does not expose ID3D11On12Device: {error}"))?;

        let mut reset_token = 0;
        let mut dxgi_manager = None;
        unsafe { MFCreateDXGIDeviceManager(&mut reset_token, &mut dxgi_manager) }
            .map_err(|error| format!("MFCreateDXGIDeviceManager failed: {error}"))?;
        let dxgi_manager = dxgi_manager
            .ok_or_else(|| "MFCreateDXGIDeviceManager returned no manager".to_string())?;
        unsafe { dxgi_manager.ResetDevice(&d3d11_device, reset_token) }.map_err(|error| {
            format!("failed to bind the D3D11On12 device to Media Foundation: {error}")
        })?;

        Ok(Self {
            d3d12_device,
            _d3d11_device: d3d11_device,
            d3d11_context,
            on12,
            dxgi_manager,
            preferred_output_format: WindowsOutputFormat::select(gpu.device()),
        })
    }
}

struct MediaFoundationRuntime {
    uninitialize_com: bool,
}

impl MediaFoundationRuntime {
    fn start() -> Result<Self, String> {
        let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let uninitialize_com = if status.is_ok() {
            true
        } else if status == RPC_E_CHANGED_MODE {
            false
        } else {
            return Err(format!(
                "CoInitializeEx failed: {}",
                windows::core::Error::from_hresult(status)
            ));
        };
        if let Err(error) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) } {
            if uninitialize_com {
                unsafe { CoUninitialize() };
            }
            return Err(format!("MFStartup failed: {error}"));
        }
        Ok(Self { uninitialize_com })
    }
}

impl Drop for MediaFoundationRuntime {
    fn drop(&mut self) {
        let _ = unsafe { MFShutdown() };
        if self.uninitialize_com {
            unsafe { CoUninitialize() };
        }
    }
}

struct WindowsSession {
    engine: IMFMediaEngine,
    _notify: IMFMediaEngineNotify,
    pending: Arc<AtomicU32>,
    source: VideoSource,
    state: VideoSessionState,
    loop_mode: LoopMode,
    announced: bool,
    presented: bool,
    ended: bool,
    epoch: PlaybackEpoch,
    awaiting_frame: bool,
    colorimetry: Option<VideoColorimetry>,
    output_format: WindowsOutputFormat,
    output_generation: DecoderOutputGeneration,
}

pub(crate) struct WindowsDecoder {
    // Runtime drops after sessions are explicitly shut down by Drop.
    runtime: MediaFoundationRuntime,
    bridge: WindowsGpuBridge,
    capture: WindowsFrameCapture,
    wake: VideoWake,
    sessions: HashMap<VideoId, WindowsSession>,
    pending: Vec<BackendEvent<WindowsFrame>>,
}

impl WindowsDecoder {
    fn new(
        gpu: GpuVideoContext,
        bridge: WindowsGpuBridge,
        wake: VideoWake,
    ) -> Result<Self, String> {
        Ok(Self {
            runtime: MediaFoundationRuntime::start()?,
            capture: WindowsFrameCapture::new(gpu, bridge.clone()),
            bridge,
            wake,
            sessions: HashMap::new(),
            pending: Vec::new(),
        })
    }

    fn create_engine(
        &self,
        notify: &IMFMediaEngineNotify,
        candidates: &[WindowsOutputFormat],
    ) -> Result<(IMFMediaEngine, WindowsOutputFormat), String> {
        let factory: IMFMediaEngineClassFactory = unsafe {
            CoCreateInstance(&CLSID_MFMediaEngineClassFactory, None, CLSCTX_INPROC_SERVER)
        }
        .map_err(|error| format!("failed to create Media Engine factory: {error}"))?;
        let mut failures = Vec::new();
        for &output_format in candidates {
            let attempt =
                create_media_engine_attributes(notify, &self.bridge.dxgi_manager, output_format)
                    .and_then(|attributes| {
                        unsafe {
                            factory.CreateInstance(
                                MF_MEDIA_ENGINE_REAL_TIME_MODE.0 as u32,
                                &attributes,
                            )
                        }
                        .map_err(|error| format!("failed to create Media Engine: {error}"))
                    });
            match attempt {
                Ok(engine) => return Ok((engine, output_format)),
                Err(error) => failures.push(format!("{output_format:?}: {error}")),
            }
        }
        Err(format!(
            "Media Engine rejected every configured output format: {}",
            failures.join("; ")
        ))
    }

    fn open(
        &mut self,
        id: VideoId,
        source: VideoSource,
        initial: InitialPlayback,
        loop_mode: LoopMode,
    ) -> Result<(), String> {
        if self.sessions.contains_key(&id) {
            return Err(format!("video {} is already open", id.get()));
        }
        let pending = Arc::new(AtomicU32::new(0));
        let notify: IMFMediaEngineNotify = MediaEngineNotify {
            pending: Arc::clone(&pending),
            wake: self.wake.clone(),
        }
        .into();
        let (engine, output_format) =
            self.create_engine(&notify, self.bridge.preferred_output_format.candidates())?;
        let autoplay = matches!(initial, InitialPlayback::Playing);
        let source_bstr = source_bstr(source.clone())?;
        if let Err(error) = configure_media_engine(
            &engine,
            &source_bstr,
            WindowsPlaybackConfiguration {
                autoplay,
                rate: 1.0,
                position: 0.0,
            },
        ) {
            let _ = unsafe { engine.Shutdown() };
            return Err(error);
        }
        let state = if autoplay {
            VideoSessionState::Playing
        } else {
            VideoSessionState::Opening
        };
        self.sessions.insert(
            id,
            WindowsSession {
                engine,
                _notify: notify,
                pending,
                source,
                state,
                loop_mode,
                announced: false,
                presented: true,
                ended: false,
                epoch: PlaybackEpoch::INITIAL,
                awaiting_frame: true,
                colorimetry: None,
                output_format,
                output_generation: DecoderOutputGeneration::INITIAL,
            },
        );
        Ok(())
    }

    fn reconfigure_after_import_failure(
        &mut self,
        id: VideoId,
        rejection: &DecoderOutputRejection,
    ) -> Result<DecoderReconfiguration, String> {
        let (source, fallback, playback) = {
            let session = self
                .sessions
                .get(&id)
                .ok_or_else(|| format!("video {} is not open", id.get()))?;
            if rejection.generation < session.output_generation {
                return Ok(DecoderReconfiguration::Superseded);
            }
            if rejection.generation != session.output_generation {
                return Ok(DecoderReconfiguration::Unsupported);
            }
            if rejection.format != session.output_format.frame() {
                return Ok(DecoderReconfiguration::Unsupported);
            }
            let Some(fallback) = session.output_format.fallback_after_rejection() else {
                return Ok(DecoderReconfiguration::Unsupported);
            };
            (
                session.source.clone(),
                fallback,
                WindowsPlaybackConfiguration {
                    autoplay: session.state == VideoSessionState::Playing && session.presented,
                    rate: unsafe { session.engine.GetPlaybackRate() },
                    position: unsafe { session.engine.GetCurrentTime() },
                },
            )
        };

        let pending = Arc::new(AtomicU32::new(0));
        let notify: IMFMediaEngineNotify = MediaEngineNotify {
            pending: Arc::clone(&pending),
            wake: self.wake.clone(),
        }
        .into();
        let (engine, output_format) = self.create_engine(&notify, &[fallback])?;
        let source_bstr = source_bstr(source)?;
        if let Err(error) = configure_media_engine(&engine, &source_bstr, playback) {
            let _ = unsafe { engine.Shutdown() };
            return Err(error);
        }

        let session = self
            .sessions
            .get_mut(&id)
            .ok_or_else(|| format!("video {} closed during output reconfiguration", id.get()))?;
        let old_engine = std::mem::replace(&mut session.engine, engine);
        session._notify = notify;
        session.pending = pending;
        session.output_format = output_format;
        session.colorimetry = None;
        session.awaiting_frame = true;
        session.ended = false;
        session.output_generation = rejection.generation.next();
        if let Err(error) = unsafe { old_engine.Shutdown() } {
            tracing::debug!(
                video_id = id.get(),
                %error,
                "superseded Media Engine did not shut down cleanly"
            );
        }
        Ok(DecoderReconfiguration::Applied {
            generation: session.output_generation,
        })
    }

    fn playback(&mut self, id: VideoId, action: PlaybackAction) -> Result<(), String> {
        let session = self
            .sessions
            .get_mut(&id)
            .ok_or_else(|| format!("video {} is not open", id.get()))?;
        unsafe {
            match action {
                PlaybackAction::Play => {
                    if session.presented {
                        windows_result("Media Foundation play failed", session.engine.Play())?;
                    }
                    session.state = VideoSessionState::Playing;
                    session.ended = false;
                }
                PlaybackAction::Pause => {
                    windows_result("Media Foundation pause failed", session.engine.Pause())?;
                    session.state = VideoSessionState::Paused;
                }
                PlaybackAction::Stop => {
                    windows_result("Media Foundation stop/pause failed", session.engine.Pause())?;
                    windows_result(
                        "Media Foundation stop/seek failed",
                        session.engine.SetCurrentTime(0.0),
                    )?;
                    session.state = VideoSessionState::Paused;
                    session.ended = false;
                    session.epoch = session.epoch.next();
                    session.awaiting_frame = true;
                }
                PlaybackAction::Seek(time) => {
                    windows_result(
                        "Media Foundation seek failed",
                        session
                            .engine
                            .SetCurrentTime(time.as_nanos() as f64 / 1_000_000_000.0),
                    )?;
                    session.ended = false;
                    session.epoch = session.epoch.next();
                    session.awaiting_frame = true;
                }
                PlaybackAction::SetRate(rate) => {
                    windows_result(
                        "Media Foundation playback-rate change failed",
                        session.engine.SetPlaybackRate(rate.get()),
                    )?;
                }
                PlaybackAction::SetLoop(mode) => session.loop_mode = mode,
            }
        }
        self.pending.push(BackendEvent::StateChanged {
            id,
            state: session.state,
        });
        Ok(())
    }

    fn poll_sessions(&mut self) {
        let mut events = Vec::new();
        let mut failed_sessions = Vec::new();
        for (&id, session) in &mut self.sessions {
            let flags = session.pending.swap(0, Ordering::AcqRel);
            if flags & EVENT_FORMAT_CHANGED != 0 {
                session.colorimetry = None;
            }
            if flags & EVENT_ERROR != 0 {
                let detail = unsafe { session.engine.GetError() }
                    .map(|error| {
                        format!(
                            "code {}, extended {:?}",
                            unsafe { error.GetErrorCode() },
                            unsafe { error.GetExtendedErrorCode() }.err()
                        )
                    })
                    .unwrap_or_else(|error| format!("unavailable error detail: {error}"));
                failed_sessions.push((id, format!("Media Foundation playback failed: {detail}")));
                continue;
            }

            if !session.announced && flags & EVENT_READY != 0 {
                let mut width = 0;
                let mut height = 0;
                match unsafe {
                    session
                        .engine
                        .GetNativeVideoSize(Some(&mut width), Some(&mut height))
                } {
                    Ok(()) if width != 0 && height != 0 => {
                        let geometry = geometry_from_media_engine(&session.engine, width, height);
                        let initial_state = match session.state {
                            VideoSessionState::Opening => VideoSessionState::Paused,
                            state => state,
                        };
                        session.state = initial_state;
                        session.announced = true;
                        events.push(BackendEvent::Opened {
                            id,
                            width: geometry.display_width,
                            height: geometry.display_height,
                            initial_state,
                        });
                    }
                    Ok(()) => {}
                    Err(error) => failed_sessions.push((
                        id,
                        format!("Media Foundation returned no native video size: {error}"),
                    )),
                }
            }

            if flags & EVENT_ENDED != 0 && !session.ended {
                if session.loop_mode.consume_replay() {
                    session.epoch = session.epoch.next();
                    session.awaiting_frame = true;
                    events.push(BackendEvent::Looped {
                        id,
                        remaining: session.loop_mode,
                    });
                    let replay = unsafe {
                        session.engine.SetCurrentTime(0.0).and_then(|()| {
                            if session.state == VideoSessionState::Playing && session.presented {
                                session.engine.Play()
                            } else {
                                Ok(())
                            }
                        })
                    };
                    if let Err(error) = replay {
                        failed_sessions.push((id, format!("failed to replay video: {error}")));
                    }
                } else {
                    session.ended = true;
                    session.state = VideoSessionState::Ended;
                    events.push(BackendEvent::Ended { id });
                }
            }

            // Event delivery remains live while hidden, but frame pulling is
            // presentation-scoped. A visible sibling's service cadence must
            // not call OnVideoStreamTick for this paused hidden session.
            if !session.presented {
                continue;
            }

            match unsafe { session.engine.OnVideoStreamTick() } {
                Ok(pts_100ns) => {
                    let mut width = 0;
                    let mut height = 0;
                    if unsafe {
                        session
                            .engine
                            .GetNativeVideoSize(Some(&mut width), Some(&mut height))
                    }
                    .is_ok()
                        && width != 0
                        && height != 0
                    {
                        session.awaiting_frame = false;
                        let geometry = geometry_from_media_engine(&session.engine, width, height);
                        let pts = MediaTime::from_nanos(pts_100ns.max(0) as u64 * 100);
                        // OnVideoStreamTick reports PTS only. Zero is the
                        // typed unknown duration; using the preceding PTS
                        // delta would describe the previous VFR frame.
                        let duration = MediaTime::ZERO;
                        if !session.announced {
                            let initial_state = match session.state {
                                VideoSessionState::Opening => VideoSessionState::Paused,
                                state => state,
                            };
                            session.state = initial_state;
                            session.announced = true;
                            events.push(BackendEvent::Opened {
                                id,
                                width,
                                height,
                                initial_state,
                            });
                        }
                        let colorimetry = match session.output_format {
                            WindowsOutputFormat::Nv12 => match session.colorimetry {
                                Some(colorimetry) => colorimetry,
                                None => {
                                    let colorimetry = media_engine_colorimetry(&session.engine);
                                    session.colorimetry = Some(colorimetry);
                                    colorimetry
                                }
                            },
                            WindowsOutputFormat::Bgra8 => VideoColorimetry::SRGB,
                        };
                        let format = session.output_format.frame();
                        // Transfer immediately after this successful tick.
                        // IMFMediaEngine exposes no frame token that could
                        // safely be deferred until the compositor PTS is due.
                        let captured =
                            self.capture
                                .capture(&session.engine, geometry, format, colorimetry);
                        let decoder_import = captured.decoder_import(session.output_format);
                        events.push(BackendEvent::Frame {
                            id,
                            frame: DecodedFrame {
                                lease: WindowsFrame { captured },
                                // Media Engine does not expose whether this
                                // session selected hardware or software
                                // decode. GPU-resident TransferVideoFrame is
                                // independent from that decoder fact.
                                decode_residency: VideoDecodeResidency::Unknown,
                                timing: FrameTiming {
                                    pts,
                                    duration,
                                    epoch: session.epoch,
                                },
                                geometry,
                                format,
                                colorimetry,
                                output_generation: session.output_generation,
                                decoder_import,
                            },
                        });
                    }
                }
                Err(error) if error.code() == S_FALSE => {}
                Err(error) => failed_sessions
                    .push((id, format!("Media Foundation video tick failed: {error}"))),
            }
        }
        for (id, message) in failed_sessions {
            if let Some(session) = self.sessions.remove(&id) {
                let _ = unsafe { session.engine.Shutdown() };
            }
            events.push(BackendEvent::Failed {
                id,
                error: message.into(),
            });
        }
        self.pending.extend(events);
    }
}

fn geometry_from_media_engine(
    engine: &IMFMediaEngine,
    coded_width: u32,
    coded_height: u32,
) -> VideoGeometry {
    let mut aspect_width = 0;
    let mut aspect_height = 0;
    let has_aspect =
        unsafe { engine.GetVideoAspectRatio(Some(&mut aspect_width), Some(&mut aspect_height)) }
            .is_ok()
            && aspect_width != 0
            && aspect_height != 0;
    let display_width = if has_aspect {
        u64::from(coded_height)
            .saturating_mul(u64::from(aspect_width))
            .saturating_add(u64::from(aspect_height) / 2)
            .checked_div(u64::from(aspect_height))
            .and_then(|width| u32::try_from(width).ok())
            .unwrap_or(coded_width)
            .max(1)
    } else {
        coded_width
    };
    VideoGeometry::with_visible_rect_and_display_size(
        coded_width,
        coded_height,
        crate::PixelRect {
            x: 0,
            y: 0,
            width: coded_width,
            height: coded_height,
        },
        display_width,
        coded_height,
        crate::VideoRotation::None,
    )
}

impl Drop for WindowsDecoder {
    fn drop(&mut self) {
        for (_, session) in self.sessions.drain() {
            let _ = unsafe { session.engine.Shutdown() };
        }
        let _ = &self.runtime;
    }
}

impl DecoderBackend for WindowsDecoder {
    type Frame = WindowsFrame;

    fn command(&mut self, command: VideoCommand) -> Result<(), crate::VideoCommandError> {
        match command {
            VideoCommand::Open {
                id,
                source,
                initial_playback,
                loop_mode,
            } => self
                .open(id, source, initial_playback, loop_mode)
                .map_err(Into::into),
            VideoCommand::Playback { id, action } => self.playback(id, action).map_err(Into::into),
            VideoCommand::Presentation { id, visibility } => {
                let session = self
                    .sessions
                    .get_mut(&id)
                    .ok_or_else(|| format!("video {} is not open", id.get()))?;
                let presented = matches!(visibility, crate::PresentationVisibility::Presented);
                if session.presented == presented {
                    return Ok(());
                }
                session.presented = presented;
                session.awaiting_frame = presented;
                unsafe {
                    if presented && session.state == VideoSessionState::Playing {
                        windows_result(
                            "Media Foundation visibility resume failed",
                            session.engine.Play(),
                        )?;
                    } else {
                        windows_result(
                            "Media Foundation visibility pause failed",
                            session.engine.Pause(),
                        )?;
                    }
                }
                Ok(())
            }
            VideoCommand::Close { id } => {
                let session = self
                    .sessions
                    .remove(&id)
                    .ok_or(crate::VideoCommandError::SessionNotOpen { id: id.get() })?;
                unsafe { session.engine.Shutdown() }
                    .map_err(|error| format!("Media Foundation shutdown failed: {error}"))?;
                self.pending.push(BackendEvent::StateChanged {
                    id,
                    state: VideoSessionState::Closed,
                });
                Ok(())
            }
        }
    }

    fn service(&mut self, _request: &crate::VideoServiceRequest) -> Vec<BackendEvent<Self::Frame>> {
        self.poll_sessions();
        std::mem::take(&mut self.pending)
    }

    fn reconfigure_after_import_failure(
        &mut self,
        id: VideoId,
        rejection: &DecoderOutputRejection,
    ) -> Result<DecoderReconfiguration, String> {
        WindowsDecoder::reconfigure_after_import_failure(self, id, rejection)
    }

    fn next_service_deadline(&self, now: Instant) -> Option<Instant> {
        self.sessions
            .values()
            .any(|session| {
                session.presented
                    && (matches!(
                        session.state,
                        VideoSessionState::Opening | VideoSessionState::Playing
                    ) || session.awaiting_frame)
            })
            .then_some(now + WINDOWS_MEDIA_POLL_INTERVAL)
    }

    fn surface_pool_diagnostics(&self) -> Option<crate::VideoSurfacePoolDiagnostics> {
        Some(
            self.capture
                .surfaces
                .diagnostics(crate::VideoSurfacePoolRole::CompositorImport),
        )
    }

    fn begin_measurement_epoch(&mut self) {
        self.pending.retain(|event| {
            event
                .measurement_epoch_disposition()
                .retains_event()
        });
        self.capture.surfaces.begin_measurement_epoch();
    }
}

fn configure_media_engine(
    engine: &IMFMediaEngine,
    source: &BSTR,
    playback: WindowsPlaybackConfiguration,
) -> Result<(), String> {
    // Keep replacement sessions stopped until their source position and rate
    // have been restored, avoiding a transient frame from timestamp zero.
    windows_result("failed to disable Media Engine autoplay", unsafe {
        engine.SetAutoPlay(false)
    })?;
    windows_result(
        "failed to disable Media Engine's untyped loop mode",
        unsafe { engine.SetLoop(false) },
    )?;
    windows_result("failed to mute inline Media Engine playback", unsafe {
        engine.SetMuted(true)
    })?;
    windows_result("failed to set the Media Engine source", unsafe {
        engine.SetSource(source)
    })?;
    windows_result("failed to load the Media Engine source", unsafe {
        engine.Load()
    })?;
    if playback.rate != 1.0 {
        windows_result("failed to restore Media Engine playback rate", unsafe {
            engine.SetPlaybackRate(playback.rate)
        })?;
    }
    if playback.position > 0.0 {
        windows_result("failed to restore Media Engine playback position", unsafe {
            engine.SetCurrentTime(playback.position)
        })?;
    }
    if playback.autoplay {
        windows_result("failed to start Media Engine playback", unsafe {
            engine.Play()
        })?;
    }
    Ok(())
}

fn create_media_engine_attributes(
    notify: &IMFMediaEngineNotify,
    dxgi_manager: &IMFDXGIDeviceManager,
    output_format: WindowsOutputFormat,
) -> Result<IMFAttributes, String> {
    let mut attributes = None;
    unsafe { MFCreateAttributes(&mut attributes, 3) }
        .map_err(|error| format!("MFCreateAttributes failed: {error}"))?;
    let attributes =
        attributes.ok_or_else(|| "MFCreateAttributes returned no attributes".to_string())?;
    windows_result("failed to install the Media Engine callback", unsafe {
        attributes.SetUnknown(&MF_MEDIA_ENGINE_CALLBACK, notify)
    })?;
    windows_result("failed to install the Media Engine DXGI manager", unsafe {
        attributes.SetUnknown(&MF_MEDIA_ENGINE_DXGI_MANAGER, dxgi_manager)
    })?;
    // Frame-server mode has no HWND/visual target, so Microsoft requires an
    // explicit render-target format before CreateInstance. Prefer native
    // NV12 so TransferVideoFrame does not also perform a YUV-to-RGB
    // conversion. The operation remains a documented GPU blit, not a direct
    // decoder-surface import.
    windows_result(
        "failed to configure the Media Engine frame-server output format",
        unsafe {
            attributes.SetUINT32(
                &MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT,
                output_format.media_engine_dxgi().0 as u32,
            )
        },
    )?;
    Ok(attributes)
}

fn media_engine_colorimetry(engine: &IMFMediaEngine) -> VideoColorimetry {
    let Ok(engine) = engine.cast::<IMFMediaEngineEx>() else {
        return VideoColorimetry::BT709_LIMITED;
    };
    let Ok(stream_count) = (unsafe { engine.GetNumberOfStreams() }) else {
        return VideoColorimetry::BT709_LIMITED;
    };
    let Some(video_stream) =
        (0..stream_count).find(|&stream| has_stream_attribute(&engine, stream, &MF_MT_FRAME_SIZE))
    else {
        return VideoColorimetry::BT709_LIMITED;
    };

    media_foundation_colorimetry(MediaFoundationColorMetadata {
        primaries: stream_attribute_u32(&engine, video_stream, &MF_MT_VIDEO_PRIMARIES),
        transfer: stream_attribute_u32(&engine, video_stream, &MF_MT_TRANSFER_FUNCTION),
        matrix: stream_attribute_u32(&engine, video_stream, &MF_MT_YUV_MATRIX),
        range: stream_attribute_u32(&engine, video_stream, &MF_MT_VIDEO_NOMINAL_RANGE),
        chroma_siting: stream_attribute_u32(&engine, video_stream, &MF_MT_VIDEO_CHROMA_SITING),
    })
}

/// Raw Media Foundation values are isolated at the platform boundary. The
/// rest of the compositor only observes the closed, typed color model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct MediaFoundationColorMetadata {
    primaries: Option<u32>,
    transfer: Option<u32>,
    matrix: Option<u32>,
    range: Option<u32>,
    chroma_siting: Option<u32>,
}

fn media_foundation_colorimetry(metadata: MediaFoundationColorMetadata) -> VideoColorimetry {
    let primaries = match metadata.primaries {
        Some(value) if value == MFVideoPrimaries_BT2020.0 as u32 => VideoColorPrimaries::Bt2020,
        Some(value)
            if value == MFVideoPrimaries_SMPTE170M.0 as u32
                || value == MFVideoPrimaries_SMPTE_C.0 as u32 =>
        {
            VideoColorPrimaries::Bt601_525
        }
        Some(value)
            if value == MFVideoPrimaries_BT470_2_SysBG.0 as u32
                || value == MFVideoPrimaries_EBU3213.0 as u32 =>
        {
            VideoColorPrimaries::Bt601_625
        }
        _ => VideoColorPrimaries::Bt709,
    };
    let transfer = match metadata.transfer {
        Some(value) if value == MFVideoTransFunc_sRGB.0 as u32 => VideoTransferCharacteristic::Srgb,
        Some(value) if value == MFVideoTransFunc_2084.0 as u32 => VideoTransferCharacteristic::Pq,
        Some(value) if value == MFVideoTransFunc_HLG.0 as u32 => VideoTransferCharacteristic::Hlg,
        _ => VideoTransferCharacteristic::Bt709,
    };
    let matrix = match metadata.matrix {
        Some(value) if value == MFVideoTransferMatrix_BT601.0 as u32 => {
            VideoMatrixCoefficients::Bt601
        }
        Some(value)
            if value == MFVideoTransferMatrix_BT2020_10.0 as u32
                || value == MFVideoTransferMatrix_BT2020_12.0 as u32 =>
        {
            VideoMatrixCoefficients::Bt2020NonConstantLuminance
        }
        _ => VideoMatrixCoefficients::Bt709,
    };
    let range = match metadata.range {
        Some(value) if value == MFNominalRange_0_255.0 as u32 => VideoColorRange::Full,
        _ => VideoColorRange::Limited,
    };
    let chroma_location = match metadata.chroma_siting {
        Some(value) if value == MFVideoChromaSubsampling_DV_PAL.0 as u32 => {
            VideoChromaLocation::TopLeft
        }
        Some(value) if value == MFVideoChromaSubsampling_MPEG1.0 as u32 => {
            VideoChromaLocation::Center
        }
        _ => VideoChromaLocation::Left,
    };
    VideoColorimetry {
        primaries,
        transfer,
        matrix,
        range,
        chroma_location,
    }
}

fn stream_attribute_u32(
    engine: &IMFMediaEngineEx,
    stream: u32,
    key: &windows::core::GUID,
) -> Option<u32> {
    let mut value = take_stream_attribute(engine, stream, key)?;
    let converted = unsafe { PropVariantToUInt32(&value) }.ok();
    let _ = unsafe { PropVariantClear(&mut value) };
    converted
}

fn has_stream_attribute(engine: &IMFMediaEngineEx, stream: u32, key: &windows::core::GUID) -> bool {
    let Some(mut value) = take_stream_attribute(engine, stream, key) else {
        return false;
    };
    let _ = unsafe { PropVariantClear(&mut value) };
    true
}

fn take_stream_attribute(
    engine: &IMFMediaEngineEx,
    stream: u32,
    key: &windows::core::GUID,
) -> Option<PROPVARIANT> {
    unsafe { engine.GetStreamAttribute(stream, key) }.ok()
}

fn windows_result<T>(context: &str, result: windows::core::Result<T>) -> Result<T, String> {
    result.map_err(|error| format!("{context}: {error}"))
}

fn source_bstr(source: VideoSource) -> Result<BSTR, String> {
    match source {
        VideoSource::File(path) => url::Url::from_file_path(&path)
            .map(|url| BSTR::from(url.as_str()))
            .map_err(|()| format!("cannot convert video path {} to a file URL", path.display())),
        VideoSource::Uri(uri) => Ok(BSTR::from(uri)),
    }
}

/// Affine compositor-owned copy of one exact IMFMediaEngine tick.
pub(crate) struct WindowsFrame {
    captured: CapturedWindowsFrame,
}

enum CapturedWindowsFrame {
    Surface(Box<SurfaceLease<WindowsSurfaceKey, WindowsSurface>>),
    Backpressured,
    Rejected(String),
}

impl CapturedWindowsFrame {
    const fn decoder_import(&self, output_format: WindowsOutputFormat) -> DecodedFrameImport {
        match self {
            Self::Surface(_) => DecodedFrameImport::Completed(output_format.completed_import()),
            Self::Backpressured | Self::Rejected(_) => DecodedFrameImport::Deferred,
        }
    }
}

struct WindowsFrameCapture {
    gpu: GpuVideoContext,
    bridge: WindowsGpuBridge,
    surfaces: BoundedSurfacePool<WindowsSurfaceKey, WindowsSurface>,
}

pub(crate) struct WindowsImporter {
    gpu: GpuVideoContext,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct WindowsSurfaceKey {
    width: u32,
    height: u32,
    format: VideoFrameFormat,
    colorimetry: VideoColorimetry,
}

#[derive(Clone)]
enum PreparedWindowsSample {
    Packed(PreparedSampledTexture),
    BiPlanar(PreparedBiPlanarTexture),
}

struct WindowsSurface {
    _resource: ID3D12Resource,
    wrapped: ID3D11Resource,
    sampled: PreparedWindowsSample,
}

impl WindowsFrameCapture {
    fn new(gpu: GpuVideoContext, bridge: WindowsGpuBridge) -> Self {
        Self {
            gpu,
            bridge,
            surfaces: BoundedSurfacePool::new(MAX_IN_FLIGHT_VIDEO_SURFACES),
        }
    }

    fn allocate_surface(
        &self,
        geometry: VideoGeometry,
        format: VideoFrameFormat,
        colorimetry: VideoColorimetry,
    ) -> Result<WindowsSurface, String> {
        use wgpu::hal::api::Dx12;
        let output_format = WindowsOutputFormat::from_frame(format)
            .ok_or_else(|| format!("unsupported Media Engine output format {format:?}"))?;
        let width = geometry.coded_width;
        let height = geometry.coded_height;
        let dxgi_format = output_format.resource_dxgi();
        let wgpu_format = output_format.wgpu();
        let heap = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Alignment: 0,
            Width: width as u64,
            Height: height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: dxgi_format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
        };
        let mut resource = None;
        unsafe {
            self.bridge
                .d3d12_device
                .CreateCommittedResource::<ID3D12Resource>(
                    &heap,
                    D3D12_HEAP_FLAG_NONE,
                    &desc,
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                    None,
                    &mut resource,
                )
        }
        .map_err(|error| format!("failed to allocate the D3D12 video texture: {error}"))?;
        let resource = resource.ok_or_else(|| "D3D12 returned no video texture".to_string())?;

        let flags = D3D11_RESOURCE_FLAGS {
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            ..Default::default()
        };
        let mut wrapped = None;
        unsafe {
            self.bridge.on12.CreateWrappedResource::<_, ID3D11Resource>(
                &resource,
                &flags,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                &mut wrapped,
            )
        }
        .map_err(|error| format!("failed to wrap the D3D12 video texture for D3D11: {error}"))?;
        let wrapped =
            wrapped.ok_or_else(|| "D3D11On12 returned no wrapped video texture".to_string())?;
        let hal_texture = unsafe {
            wgpu_hal::dx12::Device::texture_from_raw(
                resource.clone(),
                wgpu_format,
                wgpu::TextureDimension::D2,
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                1,
                1,
            )
        };
        let texture = unsafe {
            self.gpu.device().create_texture_from_hal::<Dx12>(
                hal_texture,
                &wgpu::TextureDescriptor {
                    label: Some("Neomacs Media Foundation GPU video surface"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu_format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::TextureUses::RESOURCE,
            )
        };
        let sampled = match format {
            VideoFrameFormat::Packed(_) => PreparedWindowsSample::Packed(
                self.gpu.prepare_texture(
                    texture,
                    format
                        .allocation_bytes(geometry)
                        .map_err(|error| error.to_string())?,
                ),
            ),
            VideoFrameFormat::BiPlanar420(format) => PreparedWindowsSample::BiPlanar(
                self.gpu
                    .prepare_multi_planar_texture(texture, format, colorimetry, geometry)?,
            ),
        };
        Ok(WindowsSurface {
            _resource: resource,
            wrapped,
            sampled,
        })
    }

    fn capture(
        &self,
        engine: &IMFMediaEngine,
        geometry: VideoGeometry,
        format: VideoFrameFormat,
        colorimetry: VideoColorimetry,
    ) -> CapturedWindowsFrame {
        let width = geometry.coded_width;
        let height = geometry.coded_height;
        let key = WindowsSurfaceKey {
            width,
            height,
            format,
            colorimetry,
        };
        let surface = match self.surfaces.acquire(key) {
            SurfacePoolAcquire::Reused(lease) => lease,
            SurfacePoolAcquire::Allocate(reservation) => {
                let surface = match self.allocate_surface(geometry, format, colorimetry) {
                    Ok(surface) => surface,
                    Err(error) => return CapturedWindowsFrame::Rejected(error),
                };
                reservation.fulfill(surface)
            }
            SurfacePoolAcquire::Backpressured => return CapturedWindowsFrame::Backpressured,
        };

        let resources = [Some(surface.value().wrapped.clone())];
        unsafe { self.bridge.on12.AcquireWrappedResources(&resources) };
        let rect = RECT {
            left: 0,
            top: 0,
            right: width as i32,
            bottom: height as i32,
        };
        let transfer =
            unsafe { engine.TransferVideoFrame(&surface.value().wrapped, None, &rect, None) };
        unsafe {
            self.bridge.on12.ReleaseWrappedResources(&resources);
            self.bridge.d3d11_context.Flush();
        }
        match transfer {
            Ok(()) => CapturedWindowsFrame::Surface(Box::new(surface)),
            Err(error) => CapturedWindowsFrame::Rejected(format!(
                "Media Engine GPU frame transfer failed: {error}"
            )),
        }
    }
}

impl WindowsImporter {
    fn new(gpu: GpuVideoContext) -> Self {
        Self { gpu }
    }
}

impl FrameImporter<WindowsFrame> for WindowsImporter {
    type Sampled = GpuVideoFrame;

    fn compositor_import(&self, _frame: &DecodedFrame<WindowsFrame>) -> VideoCompositorImport {
        VideoCompositorImport::GpuBlit
    }

    fn import(
        &mut self,
        frame: DecodedFrame<WindowsFrame>,
    ) -> Result<FrameImportOutcome<Self::Sampled>, String> {
        let rejected = frame.format;
        let surface = match frame.lease.captured {
            CapturedWindowsFrame::Backpressured => {
                return Ok(FrameImportOutcome::Backpressured);
            }
            CapturedWindowsFrame::Rejected(reason)
                if rejected == VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12) =>
            {
                return Ok(FrameImportOutcome::ReconfigureDecoder {
                    rejection: DecoderOutputRejection {
                        generation: frame.output_generation,
                        format: rejected,
                        reason,
                    },
                });
            }
            CapturedWindowsFrame::Rejected(reason) => return Err(reason),
            CapturedWindowsFrame::Surface(surface) => *surface,
        };
        let prepared = surface.value().sampled.clone();
        let transfer = frame
            .decoder_import
            .completed()
            .expect("a captured Media Engine surface completed its transfer");
        let sampled = match prepared {
            PreparedWindowsSample::Packed(prepared) => {
                self.gpu
                    .wrap_prepared_texture(frame.geometry, prepared, surface)
            }
            PreparedWindowsSample::BiPlanar(prepared) => {
                self.gpu
                    .wrap_prepared_bi_planar_texture(frame.geometry, prepared, surface)
            }
        };
        Ok(FrameImportOutcome::Ready(ImportedFrame {
            sampled,
            completed_import: transfer,
        }))
    }
}

impl Platform for WindowsPlatform {
    const BACKEND: VideoDecodeBackend = VideoDecodeBackend::MediaFoundation;
    type Frame = WindowsFrame;
    type Sampled = GpuVideoFrame;
    type Decoder = WindowsDecoder;
    type Importer = WindowsImporter;
}

impl ProductionPlatform for WindowsPlatform {
    fn create(
        gpu: GpuVideoContext,
        policy: crate::FrameImportPolicy,
        wake: VideoWake,
    ) -> Result<(Self::Decoder, Self::Importer), VideoInitError> {
        require_fixed_compositor_import(
            VideoDecodeBackend::MediaFoundation,
            policy,
            VideoCompositorImport::GpuBlit,
        )?;
        let bridge = WindowsGpuBridge::new(&gpu).map_err(|message| VideoInitError::Backend {
            backend: VideoDecodeBackend::MediaFoundation,
            message,
        })?;
        let decoder = WindowsDecoder::new(gpu.clone(), bridge, wake).map_err(|message| {
            VideoInitError::Backend {
                backend: VideoDecodeBackend::MediaFoundation,
                message,
            }
        })?;
        let importer = WindowsImporter::new(gpu);
        Ok((decoder, importer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_foundation_defaults_to_limited_bt709() {
        assert_eq!(
            media_foundation_colorimetry(MediaFoundationColorMetadata::default()),
            VideoColorimetry::BT709_LIMITED
        );
    }

    #[test]
    fn media_foundation_maps_hdr10_metadata() {
        assert_eq!(
            media_foundation_colorimetry(MediaFoundationColorMetadata {
                primaries: Some(MFVideoPrimaries_BT2020.0 as u32),
                transfer: Some(MFVideoTransFunc_2084.0 as u32),
                matrix: Some(MFVideoTransferMatrix_BT2020_10.0 as u32),
                range: Some(MFNominalRange_0_255.0 as u32),
                chroma_siting: Some(MFVideoChromaSubsampling_DV_PAL.0 as u32),
            }),
            VideoColorimetry {
                primaries: VideoColorPrimaries::Bt2020,
                transfer: VideoTransferCharacteristic::Pq,
                matrix: VideoMatrixCoefficients::Bt2020NonConstantLuminance,
                range: VideoColorRange::Full,
                chroma_location: VideoChromaLocation::TopLeft,
            }
        );
    }

    #[test]
    fn output_format_keeps_native_and_packed_types_consistent() {
        assert_eq!(
            WindowsOutputFormat::Nv12.frame(),
            VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)
        );
        assert_eq!(WindowsOutputFormat::Nv12.wgpu(), wgpu::TextureFormat::NV12);
        assert_eq!(
            WindowsOutputFormat::Nv12.candidates(),
            [WindowsOutputFormat::Nv12, WindowsOutputFormat::Bgra8]
        );
        assert_eq!(
            WindowsOutputFormat::Nv12.fallback_after_rejection(),
            Some(WindowsOutputFormat::Bgra8)
        );
        assert_eq!(WindowsOutputFormat::Bgra8.fallback_after_rejection(), None);
        assert_eq!(
            WindowsOutputFormat::Bgra8.frame(),
            VideoFrameFormat::Packed(PackedVideoFormat::Bgra8)
        );
        assert_eq!(
            WindowsOutputFormat::Bgra8.media_engine_dxgi(),
            DXGI_FORMAT_B8G8R8A8_UNORM
        );
        assert_eq!(
            WindowsOutputFormat::Bgra8.resource_dxgi(),
            DXGI_FORMAT_B8G8R8A8_TYPELESS
        );
        assert_eq!(
            WindowsOutputFormat::Bgra8.wgpu(),
            wgpu::TextureFormat::Bgra8UnormSrgb
        );
        assert_eq!(
            WindowsOutputFormat::Nv12.completed_import(),
            CompletedFrameImport::GpuBlit {
                reported_bytes: None
            }
        );
    }

    #[test]
    fn format_change_event_invalidates_cached_stream_metadata() {
        assert_eq!(
            media_engine_event_flag(MF_MEDIA_ENGINE_EVENT_FORMATCHANGE.0 as u32),
            EVENT_FORMAT_CHANGED
        );
    }
}
