use std::collections::HashMap;
use std::num::NonZeroU32;
use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use crossbeam_channel::{Receiver, Sender};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_allocators as gst_allocators;
use gstreamer_app as gst_app;
use gstreamer_pbutils as gst_pbutils;
use gstreamer_video as gst_video;
use neomacs_display_protocol::types::VideoId;

use crate::backend::{
    BackendEvent, BackendInbox, BackendPublisher, DecodedFrame, DecodedFrameTransfer,
    DecoderBackend, backend_bridge,
};
use crate::sampling::LinuxDrmDevice;
use crate::{
    BiPlanarVideoFormat, FrameTiming, FrameTransferPolicy, InitialPlayback, LoopMode, MediaTime,
    MissingVideoPlugin, MissingVideoPlugins, PackedVideoFormat, PixelAspectRatio, PlaybackAction,
    PlaybackEpoch, VideoChromaLocation, VideoColorPrimaries, VideoColorRange, VideoColorimetry,
    VideoCommand, VideoCommandError, VideoFrameFormat, VideoGeometry, VideoInstallerHint,
    VideoMatrixCoefficients, VideoRotation, VideoSessionState, VideoSource,
    VideoTransferCharacteristic, VideoTransferPath, VideoWake,
};

use super::frame::{
    CpuPackedSurface, DmaBufObject, DmaBufPlane, DmaBufSurface, LinuxFrameLease, LinuxFrameStorage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVideoFormatSupport {
    pub(crate) nv12: bool,
    pub(crate) p010: bool,
}

impl NativeVideoFormatSupport {
    pub(super) const fn new(nv12: bool, p010: bool) -> Self {
        Self { nv12, p010 }
    }
}

enum WorkerCommand {
    Play,
    Pause,
    Stop,
    Seek(MediaTime),
    SetRate(f64),
    SetLoop(LoopMode),
    SetPresentation(crate::PresentationVisibility),
    Close,
}

pub(crate) struct GstreamerDecoder {
    output: BackendPublisher<LinuxFrameLease>,
    incoming: BackendInbox<LinuxFrameLease>,
    workers: HashMap<VideoId, Worker>,
    worker_reaper: Option<Sender<Worker>>,
    reaper_join: Option<thread::JoinHandle<()>>,
    transfer_policy: FrameTransferPolicy,
    renderer_drm_device: Option<LinuxDrmDevice>,
    native_formats: NativeVideoFormatSupport,
}

struct Worker {
    commands: Sender<WorkerCommand>,
    shutting_down: Arc<AtomicBool>,
    join: thread::JoinHandle<()>,
}

struct WorkerStartup {
    id: VideoId,
    source: VideoSource,
    initial_playback: InitialPlayback,
    loop_mode: LoopMode,
    transfer_policy: FrameTransferPolicy,
    renderer_drm_device: Option<LinuxDrmDevice>,
    native_formats: NativeVideoFormatSupport,
}

impl Worker {
    fn begin_close(self, worker_reaper: &Sender<Self>) -> Result<(), String> {
        self.shutting_down.store(true, Ordering::Release);
        // A backend error may already have ended the thread. The command is a
        // wake-up hint, not an acknowledgement protocol. Joining belongs to
        // the dedicated reaper because command() runs on the render thread.
        let _ = self.commands.send(WorkerCommand::Close);
        worker_reaper
            .send(self)
            .map_err(|_| "GStreamer worker reaper has exited".to_string())
    }
}

impl GstreamerDecoder {
    pub(super) fn new(
        wake: VideoWake,
        transfer_policy: FrameTransferPolicy,
        renderer_drm_device: Option<LinuxDrmDevice>,
        native_formats: NativeVideoFormatSupport,
    ) -> Result<Self, String> {
        gst::init().map_err(|error| error.to_string())?;
        let (output, incoming) = backend_bridge(wake);
        let (worker_reaper, workers_to_reap) = crossbeam_channel::unbounded::<Worker>();
        let reaper_join = thread::Builder::new()
            .name("neomacs-video-reaper".into())
            .spawn(move || {
                for worker in workers_to_reap {
                    let _ = worker.join.join();
                }
            })
            .map_err(|error| format!("failed to spawn GStreamer worker reaper: {error}"))?;
        Ok(Self {
            output,
            incoming,
            workers: HashMap::new(),
            worker_reaper: Some(worker_reaper),
            reaper_join: Some(reaper_join),
            transfer_policy,
            renderer_drm_device,
            native_formats,
        })
    }

    fn open(
        &mut self,
        id: VideoId,
        source: VideoSource,
        initial_playback: InitialPlayback,
        loop_mode: LoopMode,
    ) -> Result<(), String> {
        if self.workers.contains_key(&id) {
            return Err(format!("video {} is already open", id.get()));
        }
        let (command_tx, command_rx) = crossbeam_channel::unbounded();
        let output = self.output.clone();
        let transfer_policy = self.transfer_policy;
        let renderer_drm_device = self.renderer_drm_device;
        let native_formats = self.native_formats;
        let shutting_down = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutting_down);
        let join = thread::Builder::new()
            .name(format!("neomacs-video-{}", id.get()))
            .spawn(move || {
                run_worker(
                    WorkerStartup {
                        id,
                        source,
                        initial_playback,
                        loop_mode,
                        transfer_policy,
                        renderer_drm_device,
                        native_formats,
                    },
                    command_rx,
                    output,
                    worker_shutdown,
                )
            })
            .map_err(|error| format!("failed to spawn GStreamer worker: {error}"))?;
        self.workers.insert(
            id,
            Worker {
                commands: command_tx,
                shutting_down,
                join,
            },
        );
        Ok(())
    }

    fn send(&mut self, id: VideoId, command: WorkerCommand) -> Result<(), String> {
        let worker = self
            .workers
            .get(&id)
            .ok_or_else(|| format!("video {} is not open", id.get()))?;
        worker
            .commands
            .send(command)
            .map_err(|_| format!("video {} worker has exited", id.get()))
    }
}

impl DecoderBackend for GstreamerDecoder {
    type Frame = LinuxFrameLease;

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
            VideoCommand::Playback { id, action } => {
                let command = match action {
                    PlaybackAction::Play => WorkerCommand::Play,
                    PlaybackAction::Pause => WorkerCommand::Pause,
                    PlaybackAction::Stop => WorkerCommand::Stop,
                    PlaybackAction::Seek(time) => WorkerCommand::Seek(time),
                    PlaybackAction::SetRate(rate) => WorkerCommand::SetRate(rate.get()),
                    PlaybackAction::SetLoop(mode) => WorkerCommand::SetLoop(mode),
                };
                self.send(id, command).map_err(Into::into)
            }
            VideoCommand::Presentation { id, visibility } => self
                .send(id, WorkerCommand::SetPresentation(visibility))
                .map_err(Into::into),
            VideoCommand::Close { id } => {
                let worker = self
                    .workers
                    .remove(&id)
                    .ok_or(crate::VideoCommandError::SessionNotOpen { id: id.get() })?;
                self.incoming.remove_frame(id);
                worker
                    .begin_close(
                        self.worker_reaper
                            .as_ref()
                            .expect("worker reaper exists until decoder teardown"),
                    )
                    .map_err(Into::into)
            }
        }
    }

    fn drain_events(&mut self) -> Vec<BackendEvent<Self::Frame>> {
        self.incoming.drain()
    }
}

fn run_worker(
    startup: WorkerStartup,
    commands: Receiver<WorkerCommand>,
    output: BackendPublisher<LinuxFrameLease>,
    shutting_down: Arc<AtomicBool>,
) {
    let id = startup.id;
    if let Err(error) = run_worker_inner(startup, &commands, &output, &shutting_down) {
        output.event(BackendEvent::Failed { id, error });
    }
}

fn run_worker_inner(
    startup: WorkerStartup,
    commands: &Receiver<WorkerCommand>,
    output: &BackendPublisher<LinuxFrameLease>,
    shutting_down: &AtomicBool,
) -> Result<(), crate::VideoCommandError> {
    let WorkerStartup {
        id,
        source,
        initial_playback,
        mut loop_mode,
        transfer_policy,
        renderer_drm_device,
        native_formats,
    } = startup;
    let uri = source_uri(source)?;
    let caps = preferred_sink_caps(transfer_policy, native_formats);
    let appsink = gst_app::AppSink::builder()
        .caps(&caps)
        .max_buffers(2)
        .drop(true)
        // Let GStreamer pace decoded output against its media clock. With an
        // unbounded-rate sink, a local file can decode to EOS and repeatedly
        // replace the one-slot mailbox before the compositor presents frame
        // one.
        .sync(true)
        .enable_last_sample(false)
        .build();
    let audio_sink = gst::ElementFactory::make("fakesink")
        .build()
        .map_err(|error| format!("failed to create audio sink: {error}"))?;
    let playbin_factory = if gst::ElementFactory::find("playbin3").is_some() {
        "playbin3"
    } else {
        "playbin"
    };
    let pipeline = gst::ElementFactory::make(playbin_factory)
        .property("uri", uri.as_str())
        .property("video-sink", &appsink)
        .property("audio-sink", &audio_sink)
        .build()
        .map_err(|error| format!("failed to create {playbin_factory}: {error}"))?;
    let bus = pipeline
        .bus()
        .ok_or_else(|| "GStreamer playback element has no bus".to_string())?;
    let initial_state = match initial_playback {
        InitialPlayback::Playing => gst::State::Playing,
        InitialPlayback::Paused => gst::State::Paused,
    };
    pipeline
        .set_state(initial_state)
        .map_err(|error| format!("failed to start GStreamer pipeline: {error:?}"))?;

    let mut announced = false;
    let mut playing = matches!(initial_playback, InitialPlayback::Playing);
    let mut presented = true;
    let mut need_preroll = !playing;
    let mut closed = false;
    let mut epoch = PlaybackEpoch::INITIAL;
    let mut rotation = VideoRotation::None;
    while !closed {
        while let Ok(command) = commands.try_recv() {
            closed = apply_command(
                id,
                command,
                &pipeline,
                &mut loop_mode,
                &mut playing,
                &mut presented,
                &mut need_preroll,
                &mut epoch,
                output,
            )?;
            if closed {
                break;
            }
        }
        if closed {
            break;
        }

        // Hidden and fully quiescent paused sessions block on their command
        // channel. They consume no decoder or polling cadence until the
        // compositor presents them again (or the user changes playback).
        if !presented || (!playing && !need_preroll) {
            match commands.recv() {
                Ok(command) => {
                    closed = apply_command(
                        id,
                        command,
                        &pipeline,
                        &mut loop_mode,
                        &mut playing,
                        &mut presented,
                        &mut need_preroll,
                        &mut epoch,
                        output,
                    )?;
                    continue;
                }
                Err(_) => break,
            }
        }

        // Tags are posted before decoded samples. Consume them before pulling
        // a frame so orientation participates in the very first published
        // geometry and Ready dimensions. Defer EOS until after the appsink is
        // drained so the terminal frame cannot be published after Ended.
        let mut reached_eos = false;
        // Missing-plugin messages describe a causally adjacent bus error, not
        // the lifetime of the playback session. Lexical poll scope makes it
        // impossible for a nonfatal old diagnostic to relabel a later error.
        let mut missing_plugins: Option<MissingVideoPlugins> = None;
        while let Some(message) = bus.pop() {
            if let Some(plugin) = missing_video_plugin(&message) {
                match &mut missing_plugins {
                    Some(plugins) => plugins.push(plugin),
                    slot @ None => *slot = Some(MissingVideoPlugins::new(plugin)),
                }
                continue;
            }
            match message.view() {
                gst::MessageView::Tag(tag) => {
                    if let Some(orientation) = tag.tags().get::<gst::tags::ImageOrientation>() {
                        rotation = rotation_from_gstreamer_tag(orientation.get());
                    }
                }
                gst::MessageView::Eos(..) => {
                    reached_eos = true;
                }
                gst::MessageView::Error(error) => {
                    return Err(classify_pipeline_error(
                        missing_plugins,
                        format!(
                            "GStreamer error from {:?}: {} ({:?})",
                            error.src().map(|source| source.path_string()),
                            error.error(),
                            error.debug()
                        ),
                    ));
                }
                _ => {}
            }
        }

        let sample = if need_preroll {
            appsink
                .try_pull_preroll(gst::ClockTime::from_mseconds(10))
                .inspect(|_| need_preroll = false)
        } else {
            appsink.try_pull_sample(gst::ClockTime::from_mseconds(10))
        };
        if let Some(sample) = sample
            && let Some(frame) = decode_sample(
                sample,
                shutting_down,
                epoch,
                rotation,
                transfer_policy,
                renderer_drm_device,
                pipeline_drm_identity(&pipeline),
            )?
        {
            if !announced {
                output.event(BackendEvent::Opened {
                    id,
                    width: frame.geometry.display_width,
                    height: frame.geometry.display_height,
                    initial_state: if playing {
                        VideoSessionState::Playing
                    } else {
                        VideoSessionState::Paused
                    },
                });
                announced = true;
            }
            output.frame(id, frame);
        }
        if reached_eos {
            if loop_mode.consume_replay() {
                epoch = epoch.next();
                output.event(BackendEvent::Looped {
                    id,
                    remaining: loop_mode,
                });
                pipeline
                    .seek_simple(
                        gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                        gst::ClockTime::ZERO,
                    )
                    .map_err(|error| format!("failed to loop video: {error}"))?;
            } else {
                playing = false;
                output.event(BackendEvent::Ended { id });
            }
        }
    }
    let _ = pipeline.set_state(gst::State::Null);
    output.event(BackendEvent::StateChanged {
        id,
        state: VideoSessionState::Closed,
    });
    Ok(())
}

fn missing_video_plugin(message: &gst::MessageRef) -> Option<MissingVideoPlugin> {
    let missing = gst_pbutils::MissingPluginMessage::parse(message).ok()?;
    Some(MissingVideoPlugin::new(
        missing.description().as_str(),
        Some(VideoInstallerHint::gstreamer(
            missing.installer_detail().to_string(),
        )),
    ))
}

fn classify_pipeline_error(
    missing_plugins: Option<MissingVideoPlugins>,
    backend_message: impl Into<String>,
) -> VideoCommandError {
    match missing_plugins {
        Some(plugins) => VideoCommandError::MissingPlugins { plugins },
        None => VideoCommandError::Backend {
            message: backend_message.into(),
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_command(
    id: VideoId,
    command: WorkerCommand,
    pipeline: &gst::Element,
    loop_mode: &mut LoopMode,
    playing: &mut bool,
    presented: &mut bool,
    need_preroll: &mut bool,
    epoch: &mut PlaybackEpoch,
    output: &BackendPublisher<LinuxFrameLease>,
) -> Result<bool, String> {
    let state = match command {
        WorkerCommand::Play => {
            if *presented {
                pipeline
                    .set_state(gst::State::Playing)
                    .map_err(|error| format!("failed to play video: {error:?}"))?;
            }
            *playing = true;
            *need_preroll = false;
            Some(VideoSessionState::Playing)
        }
        WorkerCommand::Pause => {
            pipeline
                .set_state(gst::State::Paused)
                .map_err(|error| format!("failed to pause video: {error:?}"))?;
            *playing = false;
            *need_preroll = *presented;
            Some(VideoSessionState::Paused)
        }
        WorkerCommand::Stop => {
            pipeline
                .set_state(gst::State::Paused)
                .map_err(|error| format!("failed to stop video: {error:?}"))?;
            pipeline
                .seek_simple(gst::SeekFlags::FLUSH, gst::ClockTime::ZERO)
                .map_err(|error| format!("failed to rewind stopped video: {error}"))?;
            *playing = false;
            *need_preroll = *presented;
            *epoch = epoch.next();
            Some(VideoSessionState::Paused)
        }
        WorkerCommand::Seek(position) => {
            pipeline
                .seek_simple(
                    gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                    gst::ClockTime::from_nseconds(position.as_nanos()),
                )
                .map_err(|error| format!("failed to seek video: {error}"))?;
            if !*playing {
                *need_preroll = true;
            }
            *epoch = epoch.next();
            None
        }
        WorkerCommand::SetRate(new_rate) => {
            let position = pipeline
                .query_position::<gst::ClockTime>()
                .unwrap_or(gst::ClockTime::ZERO);
            pipeline
                .seek(
                    new_rate,
                    gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                    gst::SeekType::Set,
                    position,
                    gst::SeekType::None,
                    gst::ClockTime::NONE,
                )
                .map_err(|error| format!("failed to change video rate: {error}"))?;
            None
        }
        WorkerCommand::SetLoop(mode) => {
            *loop_mode = mode;
            None
        }
        WorkerCommand::SetPresentation(visibility) => {
            *presented = matches!(visibility, crate::PresentationVisibility::Presented);
            if *presented {
                pipeline
                    .set_state(if *playing {
                        gst::State::Playing
                    } else {
                        gst::State::Paused
                    })
                    .map_err(|error| {
                        format!("failed to resume visible video pipeline: {error:?}")
                    })?;
                *need_preroll = !*playing;
            } else {
                pipeline.set_state(gst::State::Paused).map_err(|error| {
                    format!("failed to suspend hidden video pipeline: {error:?}")
                })?;
                *need_preroll = false;
            }
            None
        }
        WorkerCommand::Close => return Ok(true),
    };
    if let Some(state) = state {
        output.event(BackendEvent::StateChanged { id, state });
    }
    Ok(false)
}

impl Drop for GstreamerDecoder {
    fn drop(&mut self) {
        let worker_reaper = self
            .worker_reaper
            .as_ref()
            .expect("worker reaper exists until decoder teardown");
        for (_, worker) in self.workers.drain() {
            let _ = worker.begin_close(worker_reaper);
        }
        drop(self.worker_reaper.take());
        if let Some(reaper_join) = self.reaper_join.take() {
            let _ = reaper_join.join();
        }
    }
}

fn source_uri(source: VideoSource) -> Result<String, String> {
    match source {
        VideoSource::File(path) => gst::glib::filename_to_uri(path, None)
            .map(String::from)
            .map_err(|error| format!("invalid video path: {error}")),
        VideoSource::Uri(uri) => Ok(uri),
    }
}

fn preferred_sink_caps(
    policy: FrameTransferPolicy,
    native_formats: NativeVideoFormatSupport,
) -> gst::Caps {
    let mut builder = gst::Caps::builder_full();
    let mut native_drm_formats = Vec::with_capacity(2);
    if native_formats.p010 {
        native_drm_formats.push("P010");
    }
    if native_formats.nv12 {
        native_drm_formats.push("NV12");
    }
    if !native_drm_formats.is_empty() {
        let legacy_formats: Vec<_> = native_drm_formats
            .iter()
            .map(|format| match *format {
                "P010" => "P010_10LE",
                format => format,
            })
            .collect();
        builder = builder.structure_with_features(
            gst::Structure::builder("video/x-raw")
                .field("format", "DMA_DRM")
                // Prefer the hardware decoder's native two-plane surfaces.
                .field("drm-format", gst::List::new(native_drm_formats))
                .build(),
            gst::CapsFeatures::new(["memory:DMABuf"]),
        );
        // GStreamer 1.20 represents linear DMA-BUF surfaces with the ordinary
        // video format in caps. Keep this after the 1.24 DMA_DRM form so newer
        // runtimes can still negotiate explicit modifiers, while the release
        // binary remains compatible with the 1.20 API/ABI baseline.
        builder = builder.structure_with_features(
            gst::Structure::builder("video/x-raw")
                .field("format", gst::List::new(legacy_formats))
                .build(),
            gst::CapsFeatures::new(["memory:DMABuf"]),
        );
    }
    let builder = builder.structure_with_features(
        gst::Structure::builder("video/x-raw")
            .field("format", "DMA_DRM")
            // Packed DMA-BUF remains an interop fallback. Requiring sRGB
            // here is part of its contract: the packed sampling pipeline
            // has no YUV transfer/color transform.
            .field("drm-format", gst::List::new(["AR24", "AB24"]))
            .field("colorimetry", "sRGB")
            .build(),
        gst::CapsFeatures::new(["memory:DMABuf"]),
    );
    let builder = builder.structure_with_features(
        gst::Structure::builder("video/x-raw")
            .field("format", gst::List::new(["BGRA", "RGBA"]))
            .field("colorimetry", "sRGB")
            .build(),
        gst::CapsFeatures::new(["memory:DMABuf"]),
    );
    if matches!(policy, FrameTransferPolicy::AllowCpuUpload) {
        builder
            .structure(
                gst::Structure::builder("video/x-raw")
                    .field("format", gst::List::new(["RGBA", "BGRA"]))
                    .field("colorimetry", "sRGB")
                    .build(),
            )
            .build()
    } else {
        builder.build()
    }
}

fn decode_sample(
    sample: gst::Sample,
    shutting_down: &AtomicBool,
    epoch: PlaybackEpoch,
    rotation: VideoRotation,
    transfer_policy: FrameTransferPolicy,
    renderer_drm_device: Option<LinuxDrmDevice>,
    pipeline_drm_topology: PipelineDrmTopology,
) -> Result<Option<DecodedFrame<LinuxFrameLease>>, crate::VideoCommandError> {
    let caps = sample
        .caps()
        .ok_or_else(|| "decoded video sample has no caps".to_string())?;
    let buffer = sample
        .buffer()
        .ok_or_else(|| "decoded video sample has no buffer".to_string())?;
    let timing = FrameTiming {
        pts: MediaTime::from_nanos(buffer.pts().map_or(0, |time| time.nseconds())),
        duration: MediaTime::from_nanos(buffer.duration().map_or(0, |time| time.nseconds())),
        epoch,
    };

    if let Some(dmabuf) = dma_buf_video_info(caps)? {
        let info = dmabuf.info;
        let geometry =
            geometry_from_info(&info, buffer.meta::<gst_video::VideoCropMeta>(), rotation);
        let surface = extract_dmabuf(buffer, &info, dmabuf.fourcc, dmabuf.modifier)?;
        let format = frame_format_from_fourcc(dmabuf.fourcc)?;
        let transfer_path =
            dma_buf_transfer_path(renderer_drm_device, pipeline_drm_topology, format)?;
        if !transfer_policy.permits(transfer_path) {
            return Err(format!(
                "decoded video requires {transfer_path:?}, forbidden by {transfer_policy:?}"
            )
            .into());
        }
        if !wait_for_decoder_write(&surface, shutting_down)? {
            return Ok(None);
        }
        let colorimetry = colorimetry_from_video_info(&info, format);
        return Ok(Some(DecodedFrame {
            lease: LinuxFrameLease {
                _sample: sample,
                storage: LinuxFrameStorage::DmaBuf(surface),
                transfer_path,
            },
            timing,
            geometry,
            format,
            colorimetry,
            decoder_transfer: DecodedFrameTransfer::Deferred,
        }));
    }

    let info = gst_video::VideoInfo::from_caps(caps)
        .map_err(|error| format!("invalid packed video caps: {error}"))?;
    let format = match info.format() {
        gst_video::VideoFormat::Rgba => VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
        gst_video::VideoFormat::Bgra => VideoFrameFormat::Packed(PackedVideoFormat::Bgra8),
        format => return Err(format!("unsupported packed video format {format:?}").into()),
    };
    let geometry = geometry_from_info(&info, buffer.meta::<gst_video::VideoCropMeta>(), rotation);
    let bytes = {
        let map = buffer
            .map_readable()
            .map_err(|error| format!("failed to map packed video sample: {error}"))?;
        map.as_slice().to_vec()
    };
    let storage = CpuPackedSurface {
        bytes,
        stride: u32::try_from(info.stride()[0])
            .map_err(|_| "negative video row stride is unsupported".to_string())?,
    };
    if !transfer_policy.permits(VideoTransferPath::CpuUpload) {
        return Err(format!(
            "decoded video requires {:?}, forbidden by {transfer_policy:?}",
            VideoTransferPath::CpuUpload
        )
        .into());
    }
    Ok(Some(DecodedFrame {
        lease: LinuxFrameLease {
            _sample: sample,
            storage: LinuxFrameStorage::CpuPacked(storage),
            transfer_path: VideoTransferPath::CpuUpload,
        },
        timing,
        geometry,
        format,
        colorimetry: VideoColorimetry::SRGB,
        decoder_transfer: DecodedFrameTransfer::Deferred,
    }))
}

const DRM_FORMAT_MOD_LINEAR: u64 = 0;

struct DmaBufVideoInfo {
    info: gst_video::VideoInfo,
    fourcc: u32,
    modifier: u64,
}

/// Decode both generations of GStreamer's DMA-BUF caps without linking to the
/// 1.24-only `GstVideoInfoDmaDrm` symbols. The caps vocabulary is data: a 1.20
/// binary can accept the newer representation when run with newer plugins,
/// while the legacy representation keeps hardware decode usable on 1.20.
fn dma_buf_video_info(caps: &gst::CapsRef) -> Result<Option<DmaBufVideoInfo>, String> {
    let Some(features) = caps.features(0) else {
        return Ok(None);
    };
    if !features.contains("memory:DMABuf") {
        return Ok(None);
    }
    let structure = caps
        .structure(0)
        .ok_or_else(|| "DMA-BUF caps have no structure".to_owned())?;
    let format = structure
        .get::<String>("format")
        .map_err(|error| format!("DMA-BUF caps have no string format: {error}"))?;

    if format == "DMA_DRM" {
        let drm_format = structure
            .get::<String>("drm-format")
            .map_err(|error| format!("DMA_DRM caps have no drm-format: {error}"))?;
        let parsed = ParsedDrmFormat::parse(&drm_format)?;
        let mut legacy_structure = structure.to_owned();
        legacy_structure.set("format", parsed.gstreamer_format());
        legacy_structure.remove_field("drm-format");
        let legacy_caps = gst::Caps::builder_full()
            .structure_with_features(legacy_structure, features.to_owned())
            .build();
        let info = gst_video::VideoInfo::from_caps(&legacy_caps)
            .map_err(|error| format!("invalid DMA_DRM video info: {error}"))?;
        return Ok(Some(DmaBufVideoInfo {
            info,
            fourcc: parsed.fourcc,
            modifier: parsed.modifier,
        }));
    }

    let info = gst_video::VideoInfo::from_caps(caps)
        .map_err(|error| format!("invalid legacy DMA-BUF video info: {error}"))?;
    let fourcc = fourcc_from_video_format(info.format())?;
    Ok(Some(DmaBufVideoInfo {
        info,
        fourcc,
        modifier: DRM_FORMAT_MOD_LINEAR,
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedDrmFormat {
    fourcc: u32,
    modifier: u64,
}

impl ParsedDrmFormat {
    fn parse(value: &str) -> Result<Self, String> {
        let (fourcc, modifier) = match value.split_once(':') {
            Some((fourcc, modifier)) => {
                let modifier = modifier.strip_prefix("0x").unwrap_or(modifier);
                let modifier = u64::from_str_radix(modifier, 16)
                    .map_err(|_| format!("invalid DRM modifier in {value:?}"))?;
                (fourcc, modifier)
            }
            None => (value, DRM_FORMAT_MOD_LINEAR),
        };
        let bytes: [u8; 4] = fourcc
            .as_bytes()
            .try_into()
            .map_err(|_| format!("invalid DRM fourcc in {value:?}"))?;
        let parsed = Self {
            fourcc: u32::from_le_bytes(bytes),
            modifier,
        };
        // Reject unknown layouts before constructing a VideoInfo whose plane
        // contract the importer cannot uphold.
        frame_format_from_fourcc(parsed.fourcc)?;
        Ok(parsed)
    }

    fn gstreamer_format(self) -> &'static str {
        match self.fourcc {
            0x3432_5241 => "BGRA",
            0x3432_4241 => "RGBA",
            0x3231_564e => "NV12",
            0x3031_3050 => "P010_10LE",
            _ => unreachable!("ParsedDrmFormat accepts only importer-supported fourcc values"),
        }
    }
}

fn fourcc_from_video_format(format: gst_video::VideoFormat) -> Result<u32, String> {
    match format {
        gst_video::VideoFormat::Bgra => Ok(0x3432_5241),
        gst_video::VideoFormat::Rgba => Ok(0x3432_4241),
        gst_video::VideoFormat::Nv12 => Ok(0x3231_564e),
        gst_video::VideoFormat::P01010le => Ok(0x3031_3050),
        format => Err(format!("unsupported DMA-BUF video format {format:?}")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PipelineDrmIdentity {
    Unknown,
    Single(LinuxDrmDevice),
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PipelineDrmTopology {
    /// Devices reported specifically by decoder elements.
    decoder: PipelineDrmIdentity,
    /// Devices reported by any element that can participate in producing the
    /// final packed DMA-BUF, including converters and upload/postprocess nodes.
    surface_path: PipelineDrmIdentity,
    /// An explicit video transform sits between decode and the sink.  Even on
    /// the same adapter, its output must not be reported as the decoder's
    /// direct external surface.
    postprocess: bool,
    inspection_failed: bool,
}

impl PipelineDrmTopology {
    const UNKNOWN: Self = Self {
        decoder: PipelineDrmIdentity::Unknown,
        surface_path: PipelineDrmIdentity::Unknown,
        postprocess: false,
        inspection_failed: false,
    };
}

impl PipelineDrmIdentity {
    fn observe(self, device: LinuxDrmDevice) -> Self {
        match self {
            Self::Unknown => Self::Single(device),
            Self::Single(existing) if existing == device => self,
            Self::Single(_) | Self::Conflict => Self::Conflict,
        }
    }
}

fn pipeline_drm_identity(pipeline: &gst::Element) -> PipelineDrmTopology {
    let Some(bin) = pipeline.downcast_ref::<gst::Bin>() else {
        return PipelineDrmTopology::UNKNOWN;
    };
    let mut elements = bin.iterate_recurse();
    let mut topology = PipelineDrmTopology::UNKNOWN;
    loop {
        match elements.next() {
            Ok(Some(element)) => {
                topology.postprocess |= element_may_postprocess_video(&element);
                if element.find_property("device-path").is_none() {
                    continue;
                }
                let Ok(path) = element
                    .property_value("device-path")
                    .get::<Option<String>>()
                else {
                    continue;
                };
                let Some(path) = path else {
                    continue;
                };
                if let Some(device) = LinuxDrmDevice::from_path(std::path::Path::new(&path)) {
                    topology.surface_path = topology.surface_path.observe(device);
                    if element.is::<gst_video::VideoDecoder>() {
                        topology.decoder = topology.decoder.observe(device);
                    }
                }
            }
            Ok(None) => return topology,
            Err(gst::IteratorError::Error) => {
                topology.inspection_failed = true;
                return topology;
            }
            Err(gst::IteratorError::Resync) => elements.resync(),
        }
    }
}

fn element_may_postprocess_video(element: &gst::Element) -> bool {
    if element.is::<gst_video::VideoDecoder>() {
        return false;
    }
    element
        .metadata(gst::ELEMENT_METADATA_KLASS)
        .is_some_and(|class| {
            class.split('/').any(|component| {
                matches!(
                    component,
                    "Converter" | "Filter" | "Effect" | "Mixer" | "Compositor" | "Scaler"
                )
            }) && class.split('/').any(|component| component == "Video")
        })
}

fn dma_buf_transfer_path(
    renderer: Option<LinuxDrmDevice>,
    pipeline: PipelineDrmTopology,
    format: VideoFrameFormat,
) -> Result<VideoTransferPath, crate::VideoCommandError> {
    if pipeline.inspection_failed {
        return Err(crate::VideoCommandError::AdapterMismatch {
            details: "GStreamer pipeline device inspection failed before the DMA-BUF producer topology could be proven".into(),
        });
    }
    for (role, identity) in [
        ("decoder", pipeline.decoder),
        ("DMA-BUF surface path", pipeline.surface_path),
    ] {
        match (renderer, identity) {
            (_, PipelineDrmIdentity::Conflict) => {
                return Err(crate::VideoCommandError::AdapterMismatch {
                    details: format!(
                        "video pipeline {role} spans multiple DRM render nodes; cross-adapter DMA-BUF import is unsupported"
                    ),
                });
            }
            (Some(renderer), PipelineDrmIdentity::Single(decoder)) if renderer != decoder => {
                return Err(crate::VideoCommandError::AdapterMismatch {
                    details: format!(
                        "video pipeline {role} DRM device {decoder:?} does not match compositor device {renderer:?}; cross-adapter DMA-BUF import is unsupported"
                    ),
                });
            }
            _ => {}
        }
    }
    let native_planes = matches!(format, VideoFrameFormat::BiPlanar420(_));
    let same_proven_device = matches!(
        (renderer, pipeline.decoder, pipeline.surface_path),
        (
            Some(renderer),
            PipelineDrmIdentity::Single(decoder),
            PipelineDrmIdentity::Single(surface),
        ) if renderer == decoder && renderer == surface
    );
    Ok(
        if native_planes && same_proven_device && !pipeline.postprocess {
            VideoTransferPath::DirectExternalSurface
        } else {
            VideoTransferPath::GpuInteropCopy
        },
    )
}

fn colorimetry_from_video_info(
    info: &gst_video::VideoInfo,
    format: VideoFrameFormat,
) -> VideoColorimetry {
    if matches!(format, VideoFrameFormat::Packed(_)) {
        return VideoColorimetry::SRGB;
    }
    let source = info.colorimetry();
    let primaries = match source.primaries() {
        gst_video::VideoColorPrimaries::Bt2020 => VideoColorPrimaries::Bt2020,
        gst_video::VideoColorPrimaries::Bt470m
        | gst_video::VideoColorPrimaries::Smpte170m
        | gst_video::VideoColorPrimaries::Smpte240m => VideoColorPrimaries::Bt601_525,
        gst_video::VideoColorPrimaries::Bt470bg | gst_video::VideoColorPrimaries::Ebu3213 => {
            VideoColorPrimaries::Bt601_625
        }
        _ => VideoColorPrimaries::Bt709,
    };
    let transfer = match source.transfer() {
        gst_video::VideoTransferFunction::Srgb => VideoTransferCharacteristic::Srgb,
        gst_video::VideoTransferFunction::Smpte2084 => VideoTransferCharacteristic::Pq,
        gst_video::VideoTransferFunction::AribStdB67 => VideoTransferCharacteristic::Hlg,
        _ => VideoTransferCharacteristic::Bt709,
    };
    let matrix = match source.matrix() {
        gst_video::VideoColorMatrix::Rgb => VideoMatrixCoefficients::Identity,
        gst_video::VideoColorMatrix::Bt601
        | gst_video::VideoColorMatrix::Fcc
        | gst_video::VideoColorMatrix::Smpte240m => VideoMatrixCoefficients::Bt601,
        gst_video::VideoColorMatrix::Bt2020 => VideoMatrixCoefficients::Bt2020NonConstantLuminance,
        _ => VideoMatrixCoefficients::Bt709,
    };
    let range = match source.range() {
        gst_video::VideoColorRange::Range0_255 => VideoColorRange::Full,
        _ => VideoColorRange::Limited,
    };
    let chroma_site = info.chroma_site();
    let chroma_location = if chroma_site.contains(gst_video::VideoChromaSite::DV) {
        VideoChromaLocation::TopLeft
    } else if chroma_site.contains(gst_video::VideoChromaSite::JPEG) {
        VideoChromaLocation::Center
    } else {
        VideoChromaLocation::Left
    };
    VideoColorimetry {
        primaries,
        transfer,
        matrix,
        range,
        chroma_location,
    }
}

/// GStreamer/media drivers commonly publish producer completion through the
/// DMA-BUF reservation object. Vulkan is explicitly synchronized, so wait on
/// that implicit write fence on the decoder worker before the render thread
/// imports the memory. This never blocks the UI thread.
fn wait_for_decoder_write(
    surface: &DmaBufSurface,
    shutting_down: &AtomicBool,
) -> Result<bool, String> {
    let mut fds: Vec<_> = surface
        .objects
        .iter()
        .map(|object| libc::pollfd {
            fd: std::os::fd::AsRawFd::as_raw_fd(&object.fd),
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    fds.sort_unstable_by_key(|fd| fd.fd);
    fds.dedup_by_key(|fd| fd.fd);
    loop {
        if shutting_down.load(Ordering::Acquire) {
            return Ok(false);
        }
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, 100) };
        if ready > 0 {
            if retain_unready_decoder_writes(&mut fds)? {
                return Ok(true);
            }
            continue;
        }
        if ready == 0 {
            continue;
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(format!("waiting for DMA-BUF decoder fence failed: {error}"));
        }
    }
}

/// Remove DMA-BUF objects whose producer fence has completed. `poll(2)`
/// returns when any descriptor becomes ready, but a disjoint multi-planar
/// image is safe to import only after every backing object is readable.
fn retain_unready_decoder_writes(fds: &mut Vec<libc::pollfd>) -> Result<bool, String> {
    let error_events = libc::POLLERR | libc::POLLHUP | libc::POLLNVAL;
    if let Some(fd) = fds.iter().find(|fd| fd.revents & error_events != 0) {
        return Err(format!(
            "waiting for DMA-BUF object {} failed with poll events {:#x}",
            fd.fd, fd.revents
        ));
    }
    fds.retain(|fd| fd.revents & libc::POLLIN == 0);
    for fd in fds.iter_mut() {
        fd.revents = 0;
    }
    Ok(fds.is_empty())
}

fn extract_dmabuf(
    buffer: &gst::BufferRef,
    info: &gst_video::VideoInfo,
    fourcc: u32,
    modifier: u64,
) -> Result<DmaBufSurface, String> {
    let meta = buffer.meta::<gst_video::VideoMeta>();
    let offsets = meta.as_ref().map_or(info.offset(), |meta| meta.offset());
    let strides = meta.as_ref().map_or(info.stride(), |meta| meta.stride());
    let n_planes = meta
        .as_ref()
        .map_or(info.n_planes(), |meta| meta.n_planes()) as usize;
    if n_planes == 0 || n_planes > 4 || buffer.n_memory() == 0 {
        return Err(format!("invalid DMA-BUF plane count {n_planes}"));
    }
    let memory_layout = DmaBufMemoryLayout::classify(buffer.n_memory(), n_planes)?;

    let object_count = match memory_layout {
        DmaBufMemoryLayout::Shared => 1,
        DmaBufMemoryLayout::PerPlane => n_planes,
    };
    let mut objects = Vec::with_capacity(object_count);
    for memory_index in 0..object_count {
        let memory = buffer.peek_memory(memory_index);
        let raw_fd =
            if let Some(memory) = memory.downcast_memory_ref::<gst_allocators::DmaBufMemory>() {
                memory.fd()
            } else if let Some(memory) = memory.downcast_memory_ref::<gst_allocators::FdMemory>() {
                memory.fd()
            } else {
                return Err(format!("DMA-BUF object {memory_index} is not fd-backed"));
            };
        let duplicated = unsafe { libc::dup(raw_fd) };
        if duplicated < 0 {
            return Err(format!(
                "failed to duplicate DMA-BUF fd for object {memory_index}"
            ));
        }
        objects.push(DmaBufObject {
            // SAFETY: `dup` returned a new owned descriptor above.
            fd: unsafe { OwnedFd::from_raw_fd(duplicated) },
            modifier,
        });
    }
    let planes = (0..n_planes)
        .map(|plane| {
            Ok(DmaBufPlane {
                object_index: memory_layout.memory_index(plane),
                stride: u32::try_from(strides[plane])
                    .map_err(|_| format!("negative stride for DMA-BUF plane {plane}"))?,
                offset: u32::try_from(offsets[plane])
                    .map_err(|_| format!("offset too large for DMA-BUF plane {plane}"))?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(DmaBufSurface {
        objects,
        planes,
        fourcc,
    })
}

#[derive(Clone, Copy)]
enum DmaBufMemoryLayout {
    Shared,
    PerPlane,
}

impl DmaBufMemoryLayout {
    fn classify(memory_count: usize, plane_count: usize) -> Result<Self, String> {
        match memory_count {
            1 => Ok(Self::Shared),
            count if count >= plane_count => Ok(Self::PerPlane),
            count => Err(format!(
                "DMA-BUF advertises {plane_count} planes but supplies only {count} memory objects"
            )),
        }
    }

    const fn memory_index(self, plane: usize) -> usize {
        match self {
            Self::Shared => 0,
            Self::PerPlane => plane,
        }
    }
}

fn geometry_from_info(
    info: &gst_video::VideoInfo,
    crop: Option<gst::MetaRef<'_, gst_video::VideoCropMeta>>,
    rotation: VideoRotation,
) -> VideoGeometry {
    let par = info.par();
    let numerator =
        NonZeroU32::new(u32::try_from(par.numer()).unwrap_or(1)).unwrap_or(NonZeroU32::MIN);
    let denominator =
        NonZeroU32::new(u32::try_from(par.denom()).unwrap_or(1)).unwrap_or(NonZeroU32::MIN);
    let visible_rect = crop.map_or(
        crate::PixelRect {
            x: 0,
            y: 0,
            width: info.width(),
            height: info.height(),
        },
        |crop| {
            let (x, y, width, height) = crop.rect();
            crate::PixelRect {
                x,
                y,
                width,
                height,
            }
        },
    );
    VideoGeometry::with_pixel_aspect_ratio(
        info.width(),
        info.height(),
        visible_rect,
        PixelAspectRatio {
            numerator,
            denominator,
        },
        rotation,
    )
}

fn rotation_from_gstreamer_tag(orientation: &str) -> VideoRotation {
    match orientation {
        "rotate-90" => VideoRotation::Clockwise90,
        "rotate-180" => VideoRotation::Clockwise180,
        "rotate-270" => VideoRotation::Clockwise270,
        _ => VideoRotation::None,
    }
}

fn frame_format_from_fourcc(fourcc: u32) -> Result<VideoFrameFormat, String> {
    match fourcc {
        0x3432_5241 => Ok(VideoFrameFormat::Packed(PackedVideoFormat::Bgra8)),
        0x3432_4241 => Ok(VideoFrameFormat::Packed(PackedVideoFormat::Rgba8)),
        0x3231_564e => Ok(VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)),
        0x3031_3050 => Ok(VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010)),
        _ => Err(format!("unsupported DRM video format {fourcc:#010x}")),
    }
}

#[cfg(test)]
#[path = "decoder_test.rs"]
mod tests;
