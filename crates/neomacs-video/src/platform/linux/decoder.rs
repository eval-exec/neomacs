use std::collections::HashMap;
use std::num::NonZeroU32;
use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_allocators as gst_allocators;
use gstreamer_app as gst_app;
use gstreamer_pbutils as gst_pbutils;
use gstreamer_video as gst_video;
use neomacs_display_protocol::types::VideoId;

use crate::backend::{
    BackendEvent, BackendInbox, BackendPublisher, DecodedFrame, DecodedFrameImport, DecoderBackend,
    DecoderOutputGeneration, DecoderOutputRejection, DecoderReconfiguration, backend_bridge,
};
use crate::sampling::LinuxDrmDevice;
use crate::{
    BiPlanarVideoFormat, FrameImportPolicy, FrameTiming, InitialPlayback, LoopMode, MediaTime,
    MissingVideoPlugin, MissingVideoPlugins, PackedVideoFormat, PixelAspectRatio, PlaybackAction,
    PlaybackEpoch, VideoChromaLocation, VideoColorPrimaries, VideoColorRange, VideoColorimetry,
    VideoCommand, VideoCommandError, VideoCompositorImport, VideoDecodeResidency, VideoFrameFormat,
    VideoDecoderIdentity, VideoDecoderKind, VideoGeometry, VideoInstallerHint,
    VideoMatrixCoefficients, VideoRotation, VideoSessionState, VideoSource,
    VideoTransferCharacteristic, VideoWake,
};

use super::frame::{
    CpuPackedSurface, DmaBufObject, DmaBufPlane, DmaBufSurface, LinuxFrameLease, LinuxFrameStorage,
};

const FALLBACK_NEGOTIATION_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVideoFormatSupport {
    pub(crate) nv12: bool,
    pub(crate) p010: bool,
}

impl NativeVideoFormatSupport {
    pub(super) const fn new(nv12: bool, p010: bool) -> Self {
        Self { nv12, p010 }
    }

    const fn any(self) -> bool {
        self.nv12 || self.p010
    }
}

/// Bounded state machine for modifier-bearing output negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DmaDrmNegotiation {
    /// Try the modern modifier-bearing form, validating the chosen fourcc.
    Preferred,
    /// A modern sample was not importable; advertise only explicit formats.
    LinearFallback,
}

enum WorkerCommand {
    Play,
    Pause,
    Stop,
    Seek(MediaTime),
    SetRate(f64),
    SetLoop(LoopMode),
    SetPresentation(crate::PresentationVisibility),
    InstallLinearFallback {
        generation: DecoderOutputGeneration,
    },
    Close,
}

pub(crate) struct GstreamerDecoder {
    output: BackendPublisher<LinuxFrameLease>,
    incoming: BackendInbox<LinuxFrameLease>,
    workers: HashMap<VideoId, Worker>,
    worker_reaper: Option<Sender<Worker>>,
    reaper_join: Option<thread::JoinHandle<()>>,
    import_policy: FrameImportPolicy,
    renderer_drm_device: Option<LinuxDrmDevice>,
    native_formats: NativeVideoFormatSupport,
}

struct Worker {
    commands: Sender<WorkerCommand>,
    shutting_down: Arc<AtomicBool>,
    linear_fallback_requested: Arc<AtomicBool>,
    join: thread::JoinHandle<()>,
}

struct WorkerStartup {
    id: VideoId,
    source: VideoSource,
    initial_playback: InitialPlayback,
    loop_mode: LoopMode,
    import_policy: FrameImportPolicy,
    renderer_drm_device: Option<LinuxDrmDevice>,
    native_formats: NativeVideoFormatSupport,
    linear_fallback_requested: Arc<AtomicBool>,
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
        import_policy: FrameImportPolicy,
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
            import_policy,
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
        let import_policy = self.import_policy;
        let renderer_drm_device = self.renderer_drm_device;
        let native_formats = self.native_formats;
        let shutting_down = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutting_down);
        let linear_fallback_requested = Arc::new(AtomicBool::new(false));
        let worker_fallback_requested = Arc::clone(&linear_fallback_requested);
        let join = thread::Builder::new()
            .name(format!("neomacs-video-{}", id.get()))
            .spawn(move || {
                run_worker(
                    WorkerStartup {
                        id,
                        source,
                        initial_playback,
                        loop_mode,
                        import_policy,
                        renderer_drm_device,
                        native_formats,
                        linear_fallback_requested: worker_fallback_requested,
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
                linear_fallback_requested,
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

    fn service(&mut self, _request: &crate::VideoServiceRequest) -> Vec<BackendEvent<Self::Frame>> {
        self.incoming.drain()
    }

    fn reconfigure_after_import_failure(
        &mut self,
        id: VideoId,
        rejection: &DecoderOutputRejection,
    ) -> Result<DecoderReconfiguration, String> {
        let worker = self
            .workers
            .get(&id)
            .ok_or_else(|| format!("video {} is not open", id.get()))?;
        let transition = request_linear_fallback(
            &worker.linear_fallback_requested,
            rejection.generation,
        );
        if let DecoderReconfiguration::Applied { generation } = transition {
            worker
                .commands
                .send(WorkerCommand::InstallLinearFallback { generation })
                .map_err(|_| format!("video {} worker has exited", id.get()))?;
        }
        Ok(transition)
    }

    fn begin_measurement_epoch(&mut self) {
        self.incoming.begin_measurement_epoch();
    }
}

fn request_linear_fallback(
    requested: &AtomicBool,
    rejected_generation: DecoderOutputGeneration,
) -> DecoderReconfiguration {
    if rejected_generation != DecoderOutputGeneration::INITIAL {
        return DecoderReconfiguration::Unsupported;
    }
    match requested.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire) {
        Ok(false) => DecoderReconfiguration::Applied {
            generation: rejected_generation.next(),
        },
        Err(true) => DecoderReconfiguration::Superseded,
        Ok(true) | Err(false) => unreachable!("boolean compare_exchange returned an invalid state"),
    }
}

#[allow(clippy::too_many_arguments)]
fn install_linear_fallback(
    appsink: &gst_app::AppSink,
    sink_pad: &gst::Pad,
    import_policy: FrameImportPolicy,
    native_formats: NativeVideoFormatSupport,
    negotiation: &mut DmaDrmNegotiation,
    output_generation: &mut DecoderOutputGeneration,
    deadline: &mut Option<Instant>,
    generation: DecoderOutputGeneration,
) -> Result<(), String> {
    if generation != output_generation.next() {
        return Err(format!(
            "invalid Linux video output generation transition from {output_generation:?} to {generation:?}"
        ));
    }
    let fallback_caps = preferred_sink_caps(
        import_policy,
        native_formats,
        DmaDrmNegotiation::LinearFallback,
    );
    appsink.set_caps(Some(&fallback_caps));
    if !sink_pad.push_event(gst::event::Reconfigure::new()) {
        return Err("GStreamer rejected explicit video output renegotiation".to_owned());
    }
    *negotiation = DmaDrmNegotiation::LinearFallback;
    *output_generation = generation;
    *deadline = Some(Instant::now() + FALLBACK_NEGOTIATION_TIMEOUT);
    Ok(())
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
        import_policy,
        renderer_drm_device,
        native_formats,
        linear_fallback_requested,
    } = startup;
    let uri = source_uri(source)?;
    let mut dma_drm_negotiation = DmaDrmNegotiation::Preferred;
    let caps = preferred_sink_caps(import_policy, native_formats, dma_drm_negotiation);
    let appsink = gst_app::AppSink::builder()
        .caps(&caps)
        .max_buffers(2)
        .drop(true)
        // Let GStreamer pace decoded output against its media clock. With an
        // unbounded-rate sink, a local file can decode to EOS and repeatedly
        // outrun the bounded presentation queue before the compositor presents
        // frame one.
        .sync(true)
        .enable_last_sample(false)
        .build();
    let sink_pad = appsink
        .static_pad("sink")
        .ok_or_else(|| "GStreamer appsink has no static sink pad".to_owned())?;
    let _allocation_probe = sink_pad
        .add_probe(gst::PadProbeType::QUERY_DOWNSTREAM, |_, info| {
            if let Some(query) = info.query_mut()
                && let gst::QueryViewMut::Allocation(allocation) = query.view_mut()
            {
                advertise_required_video_meta(allocation);
            }
            gst::PadProbeReturn::Ok
        })
        .ok_or_else(|| "failed to observe GStreamer appsink allocation queries".to_owned())?;
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
    let mut output_generation = DecoderOutputGeneration::INITIAL;
    let mut fallback_deadline = None;
    let mut fallback_announcement_pending = false;
    let mut selected_decoder: Option<VideoDecoderIdentity> = None;
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
                &appsink,
                &sink_pad,
                import_policy,
                native_formats,
                &mut dma_drm_negotiation,
                &mut output_generation,
                &mut fallback_deadline,
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
                        &appsink,
                        &sink_pad,
                        import_policy,
                        native_formats,
                        &mut dma_drm_negotiation,
                        &mut output_generation,
                        &mut fallback_deadline,
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

        if fallback_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(
                "GStreamer did not produce the requested explicit video output before the renegotiation deadline"
                    .into(),
            );
        }

        // Capture the observation generation before potentially blocking in
        // GStreamer. The publisher rejects this work if an acknowledged
        // measurement boundary crosses while the pull/decode is in flight.
        let measurement_epoch = output.measurement_epoch();
        let sample = if need_preroll {
            appsink
                .try_pull_preroll(gst::ClockTime::from_mseconds(10))
                .inspect(|_| need_preroll = false)
        } else {
            appsink.try_pull_sample(gst::ClockTime::from_mseconds(10))
        };
        if let Some(sample) = sample {
            let caps = sample
                .caps()
                .ok_or_else(|| "decoded video sample has no caps".to_owned())?;
            if dma_drm_negotiation == DmaDrmNegotiation::LinearFallback
                && is_modern_dma_drm(caps)?
            {
                // AppSink can still contain buffers queued under the old caps
                // when the RECONFIGURE event is processed. Those buffers are
                // from the superseded generation; they cannot consume the one
                // bounded fallback or turn a successful transition terminal.
                need_preroll = !playing;
                continue;
            }
            if let Some(rejected) = rejected_dma_drm_format(caps, native_formats)? {
                match request_linear_fallback(
                    &linear_fallback_requested,
                    output_generation,
                ) {
                    DecoderReconfiguration::Applied { generation } => {
                        install_linear_fallback(
                            &appsink,
                            &sink_pad,
                            import_policy,
                            native_formats,
                            &mut dma_drm_negotiation,
                            &mut output_generation,
                            &mut fallback_deadline,
                            generation,
                        )?;
                        need_preroll = !playing;
                        fallback_announcement_pending = true;
                        tracing::warn!(
                            video_id = id.get(),
                            drm_format = rejected,
                            "renegotiating unsupported modifier-bearing video output to an explicit DMA-BUF format"
                        );
                    }
                    DecoderReconfiguration::Superseded => {}
                    DecoderReconfiguration::Unsupported => {
                        return Err(format!(
                            "GStreamer has no lower output tier after rejecting DMA_DRM format {rejected:?}"
                        )
                        .into());
                    }
                }
                continue;
            }
            if fallback_deadline.is_some() {
                fallback_deadline = None;
            }
            if fallback_announcement_pending {
                output.event(BackendEvent::OutputReconfigured {
                    id,
                    generation: output_generation,
                });
                fallback_announcement_pending = false;
            }
            let decoder = match &selected_decoder {
                Some(decoder) => decoder.clone(),
                None => {
                    let decoder = pipeline_decoder_identity(&pipeline)?;
                    selected_decoder = Some(decoder.clone());
                    decoder
                }
            };
            let Some(frame) = decode_sample(
                sample,
                shutting_down,
                epoch,
                output_generation,
                rotation,
                import_policy,
                renderer_drm_device,
                pipeline_drm_identity(&pipeline),
                decoder.kind,
            )?
            else {
                continue;
            };
            if !announced {
                output.event(BackendEvent::DecoderSelected {
                    id,
                    decoder: decoder.clone(),
                });
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
            output.frame(measurement_epoch, id, frame);
        }
        reject_incomplete_fallback_at_eos(reached_eos, fallback_deadline)?;
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

fn reject_incomplete_fallback_at_eos(
    reached_eos: bool,
    fallback_deadline: Option<Instant>,
) -> Result<(), String> {
    if reached_eos && fallback_deadline.is_some() {
        Err("GStreamer reached end of stream before producing the requested fallback output".into())
    } else {
        Ok(())
    }
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
    appsink: &gst_app::AppSink,
    sink_pad: &gst::Pad,
    import_policy: FrameImportPolicy,
    native_formats: NativeVideoFormatSupport,
    dma_drm_negotiation: &mut DmaDrmNegotiation,
    output_generation: &mut DecoderOutputGeneration,
    fallback_deadline: &mut Option<Instant>,
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
        WorkerCommand::InstallLinearFallback { generation } => {
            install_linear_fallback(
                appsink,
                sink_pad,
                import_policy,
                native_formats,
                dma_drm_negotiation,
                output_generation,
                fallback_deadline,
                generation,
            )?;
            *need_preroll = !*playing;
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
    policy: FrameImportPolicy,
    native_formats: NativeVideoFormatSupport,
    dma_drm_negotiation: DmaDrmNegotiation,
) -> gst::Caps {
    let mut builder = gst::Caps::builder_full();
    // Modern GStreamer includes a hardware/driver-specific modifier in the
    // `drm-format` string. Caps cannot express "these fourccs with any
    // modifier", so the generic form is followed by validation of the actual
    // negotiated sample. An unsupported fourcc causes exactly one transition
    // to the explicitly constrained linear/packed caps below.
    //
    // Requiring a bare "NV12" or "P010" drm-format would exclude the real
    // modifier-bearing output advertised by VA decoders.
    if native_formats.any() && dma_drm_negotiation == DmaDrmNegotiation::Preferred {
        builder = builder.structure_with_features(
            gst::Structure::builder("video/x-raw")
                .field("format", "DMA_DRM")
                .build(),
            gst::CapsFeatures::new(["memory:DMABuf"]),
        );
    }
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
            .field("format", gst::List::new(["BGRA", "RGBA"]))
            .field("colorimetry", "sRGB")
            .build(),
        gst::CapsFeatures::new(["memory:DMABuf"]),
    );
    if matches!(policy, FrameImportPolicy::AllowCpuUpload) {
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

/// Return the modern DRM format that requires bounded fallback negotiation.
///
/// `None` means either legacy caps or a format the renderer can import. A
/// syntactically valid but unknown fourcc is a negotiation miss rather than a
/// terminal parser error, because the explicit lower tier may still play it.
fn rejected_dma_drm_format(
    caps: &gst::CapsRef,
    native_formats: NativeVideoFormatSupport,
) -> Result<Option<String>, String> {
    let Some(structure) = caps.structure(0) else {
        return Err("decoded video caps have no structure".to_owned());
    };
    if structure.get::<String>("format").as_deref() != Ok("DMA_DRM") {
        return Ok(None);
    }
    let drm_format = structure
        .get::<String>("drm-format")
        .map_err(|error| format!("DMA_DRM caps have no drm-format: {error}"))?;
    let parsed = ParsedDrmFormat::parse_unvalidated(&drm_format)?;
    let supported = match frame_format_from_fourcc(parsed.fourcc) {
        Ok(VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)) => native_formats.nv12,
        Ok(VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010)) => native_formats.p010,
        Ok(VideoFrameFormat::Packed(_)) => true,
        Err(_) => false,
    };
    Ok((!supported).then_some(drm_format))
}

fn is_modern_dma_drm(caps: &gst::CapsRef) -> Result<bool, String> {
    let Some(structure) = caps.structure(0) else {
        return Err("decoded video caps have no structure".to_owned());
    };
    Ok(structure.get::<String>("format").as_deref() == Ok("DMA_DRM"))
}

/// Tell hardware decoders that downstream preserves per-plane video layout.
///
/// GStreamer 1.20's appsink predates the dedicated propose-allocation
/// callback. A downstream-query pad probe gives us the same safe allocation
/// seam without raising Neomacs's minimum GStreamer ABI.
fn advertise_required_video_meta(query: &mut gst::query::Allocation) {
    if query.find_allocation_meta::<gst_video::VideoMeta>().is_none() {
        query.add_allocation_meta::<gst_video::VideoMeta>(None);
    }
}

#[allow(clippy::too_many_arguments)]
fn decode_sample(
    sample: gst::Sample,
    shutting_down: &AtomicBool,
    epoch: PlaybackEpoch,
    output_generation: DecoderOutputGeneration,
    rotation: VideoRotation,
    import_policy: FrameImportPolicy,
    renderer_drm_device: Option<LinuxDrmDevice>,
    pipeline_drm_topology: PipelineDrmTopology,
    decoder_kind: VideoDecoderKind,
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
        let compositor_import =
            dma_buf_compositor_import(renderer_drm_device, pipeline_drm_topology)?;
        let decode_residency = match (
            decoder_kind,
            renderer_drm_device,
            pipeline_drm_topology.decoder,
        ) {
            (
                VideoDecoderKind::Hardware,
                Some(renderer),
                PipelineDrmIdentity::Single(decoder),
            ) if renderer == decoder => {
                VideoDecodeResidency::HardwareDecoderReportsRendererDevice
            }
            (VideoDecoderKind::Hardware, _, _) => VideoDecodeResidency::HardwareUnverified,
            (VideoDecoderKind::Software, _, _) => VideoDecodeResidency::Software,
            (VideoDecoderKind::Unknown, _, _) => VideoDecodeResidency::Unknown,
        };
        if !import_policy.permits(compositor_import) {
            return Err(format!(
                "decoded video requires {compositor_import:?}, forbidden by {import_policy:?}"
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
            },
            decode_residency,
            timing,
            geometry,
            format,
            colorimetry,
            output_generation,
            decoder_import: DecodedFrameImport::Deferred,
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
    if !import_policy.permits(VideoCompositorImport::CpuUpload) {
        return Err(format!(
            "decoded video requires {:?}, forbidden by {import_policy:?}",
            VideoCompositorImport::CpuUpload
        )
        .into());
    }
    Ok(Some(DecodedFrame {
        lease: LinuxFrameLease {
            _sample: sample,
            storage: LinuxFrameStorage::CpuPacked(storage),
        },
        decode_residency: match decoder_kind {
            VideoDecoderKind::Hardware => VideoDecodeResidency::HardwareUnverified,
            VideoDecoderKind::Software => VideoDecodeResidency::Software,
            VideoDecoderKind::Unknown => VideoDecodeResidency::Unknown,
        },
        timing,
        geometry,
        format,
        colorimetry: VideoColorimetry::SRGB,
        output_generation,
        decoder_import: DecodedFrameImport::Deferred,
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
        let parsed = Self::parse_unvalidated(value)?;
        // Reject unknown layouts before constructing a VideoInfo whose plane
        // contract the importer cannot uphold.
        frame_format_from_fourcc(parsed.fourcc)?;
        Ok(parsed)
    }

    fn parse_unvalidated(value: &str) -> Result<Self, String> {
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
        Ok(Self {
            fourcc: u32::from_le_bytes(bytes),
            modifier,
        })
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
    /// final DMA-BUF surface.
    surface_path: PipelineDrmIdentity,
    inspection_failed: bool,
}

fn decoder_kind_from_factory_class(class: &str) -> VideoDecoderKind {
    let classes: Vec<_> = class.split('/').collect();
    if classes.contains(&"Hardware") {
        VideoDecoderKind::Hardware
    } else if classes.contains(&"Decoder") && classes.contains(&"Video") {
        VideoDecoderKind::Software
    } else {
        VideoDecoderKind::Unknown
    }
}

/// Inspect the instantiated pipeline, not registry rank or a desired factory.
/// Decodebin is free to choose, and only its concrete VideoDecoder child is
/// evidence of what this session actually uses.
fn pipeline_decoder_identity(pipeline: &gst::Element) -> Result<VideoDecoderIdentity, String> {
    let bin = pipeline
        .downcast_ref::<gst::Bin>()
        .ok_or_else(|| "GStreamer playback element is not a bin".to_owned())?;
    let mut elements = bin.iterate_recurse();
    let mut selected = None;
    loop {
        match elements.next() {
            Ok(Some(element)) if element.is::<gst_video::VideoDecoder>() => {
                let factory = element.factory().ok_or_else(|| {
                    format!("selected video decoder {} has no factory", element.name())
                })?;
                let identity = VideoDecoderIdentity {
                    factory: factory.name().to_string(),
                    plugin: factory
                        .plugin_name()
                        .map(|name| name.to_string())
                        .unwrap_or_else(|| "unknown".to_owned()),
                    kind: decoder_kind_from_factory_class(factory.klass()),
                };
                match &selected {
                    None => selected = Some(identity),
                    Some(previous) if previous == &identity => {}
                    Some(previous) => {
                        return Err(format!(
                            "GStreamer selected multiple video decoders: {} and {}",
                            previous.factory, identity.factory
                        ));
                    }
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => {
                return selected.ok_or_else(|| {
                    "GStreamer pipeline contains no selected video decoder".to_owned()
                });
            }
            Err(gst::IteratorError::Error) => {
                return Err("GStreamer decoder inspection failed".to_owned());
            }
            Err(gst::IteratorError::Resync) => {
                selected = None;
                elements.resync();
            }
        }
    }
}

impl PipelineDrmTopology {
    const UNKNOWN: Self = Self {
        decoder: PipelineDrmIdentity::Unknown,
        surface_path: PipelineDrmIdentity::Unknown,
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

fn dma_buf_compositor_import(
    renderer: Option<LinuxDrmDevice>,
    pipeline: PipelineDrmTopology,
) -> Result<VideoCompositorImport, crate::VideoCommandError> {
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
    // The compositor imports and samples the published DMA-BUF itself. Any
    // conversion that produced that surface belongs to decoder provenance;
    // it cannot truthfully turn this direct import into a compositor blit.
    Ok(VideoCompositorImport::BorrowedNativeSurface)
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
mod tests;
