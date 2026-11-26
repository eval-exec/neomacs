use super::*;
use neomacs_display_protocol::{Color, ToolBarIconStyle, ToolBarImageSource};

#[derive(Default)]
struct TestImages {
    next: u32,
    live: HashSet<ImageId>,
}

impl ToolbarImageStore for TestImages {
    fn load(&mut self, _: &ToolBarIconKey) -> ImageId {
        self.next += 1;
        let image = ImageId::new(self.next);
        assert!(self.live.insert(image));
        image
    }
    fn retire(&mut self, image: ImageId) {
        assert!(self.live.remove(&image));
    }
}

fn key(size: u32, fg: Color, bg: Color, scale: f32, path: &str) -> ToolBarIconKey {
    ToolBarIconStyle::from_chrome_face(size, fg, bg).realize(
        ToolBarImageSource::ThemedVector { path: path.into() },
        DeviceScale::new(scale).unwrap(),
    )
}

#[test]
fn toolbar_reuses_unchanged_icons_and_retires_unreferenced_variants() {
    let original = key(24, Color::WHITE, Color::BLACK, 1.0, "open.svg");
    let mut resources = ToolbarResources::default();
    let mut images = TestImages::default();
    resources.reconcile(HashSet::from([original.clone()]), &mut images);
    let first = resources.textures()[&original];
    resources.reconcile(HashSet::from([original.clone()]), &mut images);
    assert_eq!(resources.textures()[&original], first);

    // Each is independently a new raster input: foreground, background,
    // font-derived icon geometry, fractional monitor scale, themed source.
    for changed in [
        key(24, Color::BLACK, Color::BLACK, 1.0, "open.svg"),
        key(24, Color::WHITE, Color::WHITE, 1.0, "open.svg"),
        key(32, Color::WHITE, Color::BLACK, 1.0, "open.svg"),
        key(24, Color::WHITE, Color::BLACK, 1.5, "open.svg"),
        key(24, Color::WHITE, Color::BLACK, 1.0, "dark/open.svg"),
    ] {
        // A second window may still need the previous appearance.
        resources.reconcile(
            HashSet::from([original.clone(), changed.clone()]),
            &mut images,
        );
        assert_eq!(resources.textures()[&original], first);
        assert_ne!(resources.textures()[&changed], first);
        assert_eq!(images.live.len(), 2);
        resources.reconcile(HashSet::from([original.clone()]), &mut images);
        assert_eq!(images.live, HashSet::from([first]));
    }
    resources.reconcile(HashSet::new(), &mut images);
    assert!(images.live.is_empty());
    assert!(resources.textures().is_empty());
}

#[test]
fn toolbar_decode_completion_redraws_without_an_evaluator_frame() {
    use neomacs_display_protocol::{
        FrameGlyphBuffer, ImageLayoutExtent, ImageLoadAttempt, ImageLoadToken, ImageMaskKind,
        ImageReportedExtent,
    };
    use neomacs_renderer_wgpu::{ImageCacheEvent, ImageMetadata};
    let mut app = super::super::tests::make_test_app();
    let icon = key(24, Color::WHITE, Color::BLACK, 1.0, "open.svg");
    let mut images = TestImages::default();
    app.toolbar
        .reconcile(HashSet::from([icon.clone()]), &mut images);
    let image = app.toolbar.textures()[&icon];
    let primary = app.frame_windows.primary_window_mut().unwrap();
    primary.render.compositor.current_frame = Some(FrameGlyphBuffer::with_size(800.0, 600.0));
    primary.render.set_dirty(false);

    let completion = |image| ImageCacheEvent::Ready {
        load: ImageLoadToken::new(image, ImageLoadAttempt::new(1).unwrap()),
        metadata: ImageMetadata {
            layout: ImageLayoutExtent::new(24, 24),
            reported: ImageReportedExtent::new(24, 24),
            background: 0,
            background_transparent: true,
            mask: ImageMaskKind::Clipping,
            embedded: Default::default(),
        },
    };
    app.handle_image_event(completion(ImageId::new(999)));
    assert!(
        !app.frame_windows
            .primary_window()
            .unwrap()
            .render
            .compositor
            .dirty,
        "unrelated image completions do not dirty toolbar chrome"
    );
    app.handle_image_event(completion(image));
    assert!(
        app.frame_windows
            .primary_window()
            .unwrap()
            .render
            .compositor
            .dirty,
        "chrome must redraw without another evaluator publication"
    );
}

#[test]
fn toolbar_reloads_all_required_icons_after_device_loss() {
    let original = key(24, Color::WHITE, Color::BLACK, 1.0, "open.svg");
    let required = HashSet::from([original.clone()]);
    let mut resources = ToolbarResources::default();
    let mut old_device = TestImages::default();
    resources.reconcile(required.clone(), &mut old_device);
    resources.forget_after_device_loss();
    assert!(resources.textures().is_empty());
    let mut new_device = TestImages::default();
    resources.reconcile(required, &mut new_device);
    assert_eq!(
        new_device.live,
        HashSet::from([resources.textures()[&original]])
    );
}

#[test]
fn clear_image_cache_holds_toolbar_icons_until_a_post_clear_frame_arrives() {
    use crate::thread_comm::{AssetCommand, RenderCommand, ThreadComms};
    use neomacs_display_protocol::{FrameGlyphBuffer, PresentationId};
    use std::sync::{Arc, Condvar, Mutex};

    let (emacs, render) = ThreadComms::new().split();
    let mut app = RenderApp::new(
        render,
        800,
        600,
        "toolbar-clear-test".to_string(),
        Arc::new(crate::render_thread::ImageRenderState::default()),
        Arc::new((Mutex::new(Vec::new()), Condvar::new())),
        true,
        #[cfg(feature = "neo-term")]
        crate::terminal::new_shared_terminals(),
    );
    let icon = key(24, Color::WHITE, Color::BLACK, 1.0, "open.svg");
    let required = HashSet::from([icon.clone()]);
    let mut images = TestImages::default();
    app.toolbar.reconcile(required.clone(), &mut images);
    assert_eq!(app.toolbar.textures().len(), 1);
    assert_eq!(images.live.len(), 1);

    let mut frame = FrameGlyphBuffer::with_size(800.0, 600.0);
    frame.presentation_id = PresentationId::new(4);
    app.frame_windows
        .primary_window_mut()
        .unwrap()
        .render
        .compositor
        .current_frame = Some(frame);

    // Simulate ClearImageCache processed during about_to_wait before Lisp sends
    // PublishImageCacheUsage: the renderer dropped its textures and held
    // toolbar residency until a post-clear frame, so the same tick's
    // synchronize_toolbar_resources must NOT immediately reload toolbar icons
    // from the stale pre-clear frame.
    images.live.clear();
    emacs
        .cmd_tx
        .send(RenderCommand::Asset(AssetCommand::ClearImageCache {
            through: 0,
            cleared_after_presentation: 4,
        }))
        .unwrap();
    assert!(!app.process_commands());
    app.toolbar.reconcile(required.clone(), &mut images);
    assert!(
        app.toolbar.textures().is_empty(),
        "toolbar icons must stay unrealized after ClearImageCache until a post-clear frame arrives"
    );
    assert!(
        images.live.is_empty(),
        "reconcile must not reload toolbar icons into the cleared image cache before the next redisplay"
    );

    // Once a strictly newer post-clear presentation is published, reconcile
    // resumes and reloads the toolbar icons.
    app.toolbar.note_frame_published(PresentationId::new(5));
    app.toolbar.reconcile(required, &mut images);
    assert_eq!(app.toolbar.textures().len(), 1);
    assert_eq!(images.live.len(), 1);
}

#[test]
fn clear_image_cache_ignores_pre_clear_queued_frame_and_reloads_on_post_clear_frame() {
    use crate::thread_comm::{AssetCommand, RenderCommand, ThreadComms};
    use neomacs_display_protocol::glyph_matrix::FrameDisplayState;
    use neomacs_display_protocol::{
        DisplayFrameId, FrameGlyphBuffer, PresentationId, PresentedHitIndex,
        SealedFramePresentation,
    };
    use std::sync::{Arc, Condvar, Mutex};

    let sealed_primary = |presentation: u64| {
        let mut frame = FrameGlyphBuffer::with_size(800.0, 600.0);
        frame.presentation_id = PresentationId::new(presentation);
        frame.set_frame_identity(
            DisplayFrameId::new(0),
            DisplayFrameId::new(0),
            0.0,
            0.0,
            0,
            false,
            0.0,
            Color::BLACK,
            false,
            1.0,
        );
        let mut state = FrameDisplayState::from_frame_glyph_buffer(&frame);
        state.presented_hit_index =
            PresentedHitIndex::from_parts(state.presentation_id, vec![], vec![]).unwrap();
        SealedFramePresentation::seal(state).unwrap()
    };

    let (emacs, render) = ThreadComms::new().split();
    let mut app = RenderApp::new(
        render,
        800,
        600,
        "toolbar-preclear-frame-test".to_string(),
        Arc::new(crate::render_thread::ImageRenderState::default()),
        Arc::new((Mutex::new(Vec::new()), Condvar::new())),
        true,
        #[cfg(feature = "neo-term")]
        crate::terminal::new_shared_terminals(),
    );
    let icon = key(24, Color::WHITE, Color::BLACK, 1.0, "open.svg");
    let required = HashSet::from([icon.clone()]);
    let mut images = TestImages::default();
    app.toolbar.reconcile(required.clone(), &mut images);
    assert_eq!(images.live.len(), 1);

    // Model `(redisplay t)` (presentation 4 queued in frame_rx) immediately
    // followed by `(clear-image-cache t)` (cleared_after_presentation = 4),
    // as in `image-api-cancel`: ClearImageCache runs in process_commands()
    // before poll_frame() dequeues the already-queued pre-clear frame 4.
    emacs.frame_tx.submit(sealed_primary(4)).unwrap();

    images.live.clear();
    emacs
        .cmd_tx
        .send(RenderCommand::Asset(AssetCommand::ClearImageCache {
            through: 0,
            cleared_after_presentation: 4,
        }))
        .unwrap();
    assert!(!app.process_commands());
    app.poll_frame();
    app.toolbar.reconcile(required.clone(), &mut images);
    assert!(
        app.toolbar.textures().is_empty(),
        "a pre-clear frame queued in frame_rx (presentation <= fence) must not reload cleared toolbar icons"
    );
    assert!(images.live.is_empty());

    // A subsequent post-clear redisplay (presentation 5 > 4) resumes toolbar
    // realization and reloads the icons on reconcile.
    emacs.frame_tx.submit(sealed_primary(5)).unwrap();
    app.poll_frame();
    app.toolbar.reconcile(required, &mut images);
    assert_eq!(app.toolbar.textures().len(), 1);
    assert_eq!(images.live, HashSet::from([app.toolbar.textures()[&icon]]));
}

#[test]
fn clear_image_cache_fence_is_monotonic_and_preserved_across_device_loss() {
    use neomacs_display_protocol::PresentationId;

    let icon = key(24, Color::WHITE, Color::BLACK, 1.0, "open.svg");
    let required = HashSet::from([icon.clone()]);
    let mut resources = ToolbarResources::default();
    let mut images = TestImages::default();
    resources.reconcile(required.clone(), &mut images);
    assert_eq!(resources.textures().len(), 1);

    resources.clear_after_presentation(PresentationId::new(10));
    // A lower fence from a stale/synthetic clear must not regress the hold.
    resources.clear_after_presentation(PresentationId::new(3));
    // Device loss while cleared must not prematurely lift the clear hold.
    resources.forget_after_device_loss();

    let mut replacement_gpu = TestImages::default();
    resources.note_frame_published(PresentationId::new(10));
    resources.reconcile(required.clone(), &mut replacement_gpu);
    assert!(resources.textures().is_empty());
    assert!(replacement_gpu.live.is_empty());

    resources.note_frame_published(PresentationId::new(11));
    resources.reconcile(required, &mut replacement_gpu);
    assert_eq!(
        replacement_gpu.live,
        HashSet::from([resources.textures()[&icon]])
    );
}
