//! Asset and embedded-content render commands.

use super::RenderApp;
use crate::thread_comm::AssetCommand;
#[cfg(feature = "video")]
use crate::thread_comm::MediaSource;
use neomacs_display_protocol::ImageId;

fn clear_image_terminals(shared: &super::SharedImageRenderState, image: ImageId) {
    shared.clear_image_terminals(image);
}

impl RenderApp {
    // WebView command fields (button/state/modifiers/keycode/script/frame id) are
    // consumed only inside the `webview` cfg blocks below.
    #[cfg_attr(not(feature = "webview"), allow(unused_variables))]
    // Video creation fields are likewise consumed only by the video build.
    #[cfg_attr(not(feature = "video"), allow(unused_variables))]
    pub(super) fn handle_asset(&mut self, cmd: AssetCommand) {
        match cmd {
            AssetCommand::ImageLoadFile {
                load,
                path,
                size,
                rotation,
                realization,
                colors,
                mask,
                frame,
                sequence,
            } => {
                clear_image_terminals(&self.image_metadata, load.image());
                tracing::info!("Loading image {}: {} (size {:?})", load, path, size);
                if let Some(ref mut renderer) = self.renderer {
                    renderer.load_image_file_with_id(
                        load,
                        &path,
                        size,
                        rotation,
                        realization,
                        colors,
                        mask,
                        frame,
                        sequence,
                    );
                } else {
                    tracing::warn!("Renderer not initialized, cannot load image {}", load);
                }
                self.publish_image_cache_usage();
            }
            AssetCommand::ImageLoadData {
                load,
                data,
                size,
                rotation,
                realization,
                colors,
                mask,
                frame,
                sequence,
            } => {
                clear_image_terminals(&self.image_metadata, load.image());
                let (data, resources) = match data {
                    neovm_core::emacs_core::image_catalog::ImageDataSource::Isolated(data) => {
                        (data, neomacs_renderer_wgpu::SvgResourceContext::Isolated)
                    }
                    neovm_core::emacs_core::image_catalog::ImageDataSource::WithBaseUri {
                        data,
                        base_uri,
                    } => (
                        data,
                        neomacs_renderer_wgpu::SvgResourceContext::BaseUri(
                            base_uri.as_utf8_str().unwrap_or_default().to_owned(),
                        ),
                    ),
                };
                tracing::info!(
                    "Loading image data {}: {} bytes (size {:?})",
                    load,
                    data.len(),
                    size
                );
                if let Some(ref mut renderer) = self.renderer {
                    renderer.load_image_data_with_id(
                        load,
                        &data,
                        size,
                        rotation,
                        realization,
                        colors,
                        mask,
                        frame,
                        sequence,
                        resources,
                    );
                } else {
                    tracing::warn!("Renderer not initialized, cannot load image data {}", load);
                }
                self.publish_image_cache_usage();
            }
            AssetCommand::ImageLoadArgb32 {
                load,
                data,
                width,
                height,
                stride,
            } => {
                clear_image_terminals(&self.image_metadata, load.image());
                tracing::debug!(
                    "Loading ARGB32 image {}: {}x{} stride={}",
                    load,
                    width,
                    height,
                    stride
                );
                if let Some(ref mut renderer) = self.renderer {
                    renderer.load_image_argb32_with_id(load, &data, width, height, stride);
                }
                self.publish_image_cache_usage();
            }
            AssetCommand::ImageLoadRgb24 {
                load,
                data,
                width,
                height,
                stride,
            } => {
                clear_image_terminals(&self.image_metadata, load.image());
                tracing::debug!(
                    "Loading RGB24 image {}: {}x{} stride={}",
                    load,
                    width,
                    height,
                    stride
                );
                if let Some(ref mut renderer) = self.renderer {
                    renderer.load_image_rgb24_with_id(load, &data, width, height, stride);
                }
                self.publish_image_cache_usage();
            }
            AssetCommand::ImageRetire { image } => {
                clear_image_terminals(&self.image_metadata, image);
                tracing::debug!("Retiring image {} after its last presentation", image);
                if let Some(ref mut renderer) = self.renderer {
                    renderer.retire_image(image);
                }
                self.publish_image_cache_usage();
            }
            AssetCommand::ImageSequenceRetire { retirement } => {
                tracing::debug!(?retirement, "retiring image sequence decoder state");
                if let Some(ref mut renderer) = self.renderer {
                    renderer.retire_image_sequence(retirement);
                }
                self.publish_image_cache_usage();
            }
            AssetCommand::DebugSimulateDeviceLoss => {
                tracing::warn!(
                    "simulating wgpu device loss (debug): rebuilding GPU state on the next pass"
                );
                self.device_lost.mark_lost_now();
                // Guarantee another event-loop pass observes the latch even
                // on an otherwise idle session: dirty content plus explicit
                // redraw requests keep the loop awake until recovery runs.
                self.frame_windows.mark_top_level_dirty();
                self.frame_windows
                    .for_each_top_level_window(|window_state| window_state.request_redraw());
            }
            AssetCommand::WebView(command) => {
                #[cfg(not(feature = "webview"))]
                {
                    tracing::warn!(
                        view = ?command.id(),
                        "this build has no WebView backend; dropping command"
                    );
                }
                #[cfg(feature = "webview")]
                {
                    let id = command.id();
                    let closing = matches!(command, neomacs_webview::WebViewCommand::Close { .. });
                    let Some(system) = self.webview_system.as_mut() else {
                        tracing::warn!(?id, "WebView system is not initialized");
                        return;
                    };
                    if let Err(error) = system.command(command) {
                        tracing::error!(?id, ?error, "WebView command failed");
                        return;
                    }
                    if closing {
                        if let Some(renderer) = self.renderer.as_mut() {
                            renderer.remove_webview(id);
                        }
                        if self
                            .focused_webview
                            .is_some_and(|target| target.view() == id)
                        {
                            self.focused_webview = None;
                        }
                        if self
                            .webview_pointer_capture
                            .is_some_and(|capture| capture.target.view() == id)
                        {
                            self.webview_pointer_capture = None;
                        }
                        self.frame_windows.mark_top_level_dirty();
                    }
                }
            }
            AssetCommand::VideoCreate {
                id,
                source,
                loop_count,
                autoplay,
            } => {
                tracing::info!("Loading video {}: {}", id, source.as_str());
                #[cfg(feature = "video")]
                if let Some(ref mut renderer) = self.renderer {
                    match source {
                        MediaSource::File(path) => {
                            renderer.load_video_file_with_id(id, &path, loop_count, autoplay);
                        }
                        MediaSource::Uri(uri) => {
                            renderer.load_video_uri_with_id(id, &uri, loop_count, autoplay);
                        }
                    }
                    tracing::info!("Video loaded with requested id {}", id);
                }
            }
            AssetCommand::VideoPlay { id } => {
                tracing::debug!("Playing video {}", id);
                #[cfg(feature = "video")]
                if let Some(ref mut renderer) = self.renderer {
                    renderer.video_play(id);
                }
            }
            AssetCommand::VideoPause { id } => {
                tracing::debug!("Pausing video {}", id);
                #[cfg(feature = "video")]
                if let Some(ref mut renderer) = self.renderer {
                    renderer.video_pause(id);
                }
            }
            AssetCommand::VideoDestroy { id } => {
                tracing::info!("Destroying video {}", id);
                #[cfg(feature = "video")]
                if let Some(ref mut renderer) = self.renderer {
                    renderer.free_video(id);
                }
            }
            AssetCommand::SurfaceCreate {
                id,
                source,
                width,
                height,
                animate,
                fps,
                recreatable,
            } => {
                if let Some(ref mut renderer) = self.renderer {
                    // Budget registration (physical bytes) and the eviction
                    // driver live in the renderer beside the caches
                    // (WgpuRenderer::register_surface_bytes).
                    let result = match &source {
                        crate::thread_comm::SurfaceSource::Shader {
                            language,
                            source,
                            uniforms,
                            channel0,
                        } => renderer.create_shader_surface(
                            id,
                            *language,
                            source,
                            uniforms,
                            width,
                            height,
                            animate,
                            fps,
                            *channel0,
                            recreatable,
                        ),
                        crate::thread_comm::SurfaceSource::Pixels { data } => {
                            renderer.create_pixel_surface(id, data, width, height)
                        }
                    };
                    if let Err(err) = result {
                        tracing::warn!("shader surface {id} create failed: {err}");
                        // Naga (on the Lisp thread) already accepted this
                        // shader, so the failure is a device-specific
                        // wgpu rejection the evaluator never saw. Report it
                        // back so Lisp can surface it, not just this log line.
                        self.comms.send_input(
                            crate::thread_comm::InputEvent::SurfaceCreateFailed { id, error: err },
                        );
                    }
                } else {
                    tracing::warn!("Renderer not initialized, cannot create surface {id}");
                }
            }
            AssetCommand::SurfaceSetUniform { id, name, value } => {
                if let Some(ref mut renderer) = self.renderer {
                    renderer.set_surface_uniform(id, &name, value);
                }
            }
            AssetCommand::SurfaceFree { id } => {
                if let Some(ref mut renderer) = self.renderer {
                    renderer.free_surface(id);
                }
            }
            AssetCommand::FrameShaderSet { request, composed } => match composed {
                Some((_source, _language, _uniforms))
                    if !self.render_policy.frame_post_disposition().is_enabled() =>
                {
                    tracing::warn!(
                        "Ignoring frame shader installation under the active render-quality policy"
                    );
                    let current = self.comms.capabilities.acknowledge_frame_shader(
                        request,
                        crate::thread_comm::FrameShaderExecution::SuppressedByQualityPolicy,
                    );
                    if current {
                        self.comms
                            .send_input(crate::thread_comm::InputEvent::FrameShaderFailed {
                                error:
                                    "frame shaders are disabled by the active render-quality policy"
                                        .to_owned(),
                            });
                    }
                }
                Some((source, language, uniforms)) => {
                    if let Some(renderer) = self.renderer.as_mut() {
                        match renderer.set_frame_post(language, &source, &uniforms) {
                            Ok(()) => {
                                self.comms.capabilities.acknowledge_frame_shader(
                                    request,
                                    crate::thread_comm::FrameShaderExecution::Installed,
                                );
                            }
                            Err(err) => {
                                tracing::warn!("frame shader install failed: {err}");
                                let current = self.comms.capabilities.acknowledge_frame_shader(
                                    request,
                                    crate::thread_comm::FrameShaderExecution::Rejected,
                                );
                                if current {
                                    self.comms.send_input(
                                        crate::thread_comm::InputEvent::FrameShaderFailed {
                                            error: err,
                                        },
                                    );
                                }
                            }
                        }
                    } else {
                        let error =
                            "renderer is not initialized for frame shader installation".to_owned();
                        let current = self.comms.capabilities.acknowledge_frame_shader(
                            request,
                            crate::thread_comm::FrameShaderExecution::Rejected,
                        );
                        if current {
                            self.comms.send_input(
                                crate::thread_comm::InputEvent::FrameShaderFailed { error },
                            );
                        }
                    }
                }
                None => {
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.clear_frame_post();
                    }
                    // Removing the shader also removes its continuous demand.
                    // Publish one ordinary scene repaint so the swapchain
                    // cannot retain the last post-processed pixels forever.
                    self.frame_windows.mark_top_level_dirty();
                    self.comms.capabilities.acknowledge_frame_shader(
                        request,
                        crate::thread_comm::FrameShaderExecution::Absent,
                    );
                }
            },
            AssetCommand::FrameShaderSetUniform {
                request,
                name,
                value,
            } => {
                if self.render_policy.frame_post_disposition().is_enabled() {
                    if self.comms.capabilities.frame_shader_execution(request)
                        == crate::thread_comm::FrameShaderExecution::Installed
                        && let Some(ref mut renderer) = self.renderer
                    {
                        renderer.set_frame_post_uniform(&name, value);
                    }
                } else {
                    let current = self.comms.capabilities.acknowledge_frame_shader(
                        request,
                        crate::thread_comm::FrameShaderExecution::SuppressedByQualityPolicy,
                    );
                    if current {
                        self.comms.send_input(
                            crate::thread_comm::InputEvent::FrameShaderFailed {
                            error: "frame shader uniform updates are disabled by the active render-quality policy"
                                .to_owned(),
                            },
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod image_terminal_tests {
    use super::*;
    use neovm_core::emacs_core::image_catalog::{
        ImageLoadAttempt, ImageLoadToken, ResolvedImageMetadata,
    };
    use std::sync::Arc;

    #[test]
    fn free_or_reload_clears_shared_image_terminal() {
        let shared: super::super::SharedImageRenderState =
            Arc::new(super::super::ImageRenderState::default());
        let image = ImageId::new(9);
        let first = ImageLoadToken::new(image, ImageLoadAttempt::new(1).unwrap());
        let second = ImageLoadToken::new(image, ImageLoadAttempt::new(2).unwrap());
        shared.publish_terminal(
            first,
            super::super::ImageDecodeTerminal::Failed("old failure".to_owned()),
        );

        clear_image_terminals(&shared, image);
        assert!(shared.terminal(first).is_none());

        shared.publish_terminal(
            second,
            super::super::ImageDecodeTerminal::Ready(
                ResolvedImageMetadata::layout_is_image_pixels(
                    2,
                    3,
                    0,
                    true,
                    neomacs_display_protocol::ImageMaskKind::Clipping,
                ),
            ),
        );
        clear_image_terminals(&shared, image);
        assert!(shared.terminal(second).is_none());

        shared.publish_terminal(
            second,
            super::super::ImageDecodeTerminal::Ready(
                ResolvedImageMetadata::layout_is_image_pixels(
                    5,
                    8,
                    0x12_34_56,
                    false,
                    neomacs_display_protocol::ImageMaskKind::None,
                ),
            ),
        );
        assert!(matches!(
            shared.terminal(second),
            Some(super::super::ImageDecodeTerminal::Ready(_))
        ));
    }
}
