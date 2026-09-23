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
        ToolBarImageSource::File { path: path.into() },
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
