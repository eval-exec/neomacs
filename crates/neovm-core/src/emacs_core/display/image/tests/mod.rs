use super::*;
use crate::emacs_core::eval::{DisplayHost, GuiFrameHostRequest};
use crate::emacs_core::image_catalog::{
    AxisSize, ImageAnimationInvalidation, ImageCatalog, ImageEmbeddedMetadata, ImageFrameDelay,
    ImageFrameIndex, ImageHeuristicMask, ImageId, ImageInvalidation, ImageInvalidationResult,
    ImageLayoutExtent, ImageLoadAttempt, ImageLoadToken, ImageLookup, ImageMaskKind,
    ImageMaskPolicy, ImageResolveRequest, ImageResolveSource, ImageSizeSpec, PendingImage,
    ReadyImage, ResolvedImageMetadata,
};
use crate::emacs_core::value::list_to_vec;
use crate::face::{Color, FaceTable};
use std::sync::{Arc, Mutex};

fn test_image_load(id: u32) -> ImageLoadToken {
    ImageLoadToken::new(
        ImageId::new(id),
        ImageLoadAttempt::new(1).expect("nonzero test attempt"),
    )
}

#[derive(Default)]
struct RecordingImageDisplayHost {
    requests: Arc<Mutex<Vec<ImageResolveRequest>>>,
    invalidations: Arc<Mutex<Vec<ImageInvalidation>>>,
    animation_invalidations: Arc<Mutex<Vec<ImageAnimationInvalidation>>>,
    clear_all_calls: Arc<Mutex<usize>>,
    /// Override resolved layout size (default 40×30).
    fixed_size: Option<(u32, u32)>,
    /// Override decoded alpha semantics (default clipping mask).
    fixed_mask: Option<ImageMaskKind>,
    /// Override decoder-owned metadata (default empty, like a plain PNG).
    fixed_embedded: ImageEmbeddedMetadata,
}

impl DisplayHost for RecordingImageDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resolve_image_sync(
        &self,
        request: ImageResolveRequest,
    ) -> Result<Option<ReadyImage>, String> {
        let (width, height) = self.fixed_size.unwrap_or((40, 30));
        let metadata = ResolvedImageMetadata::from_layout(
            ImageLayoutExtent::new(width, height),
            request.realization,
            0,
            true,
            self.fixed_mask.unwrap_or(ImageMaskKind::Clipping),
        )
        .with_embedded(self.fixed_embedded.clone());
        self.requests
            .lock()
            .expect("image requests lock")
            .push(request);
        Ok(Some(ReadyImage {
            load: test_image_load(9),
            metadata,
        }))
    }

    fn image_catalog(&self) -> Option<&dyn ImageCatalog> {
        Some(self)
    }
}

impl ImageCatalog for RecordingImageDisplayHost {
    fn lookup(&self, request: ImageResolveRequest) -> ImageLookup {
        self.requests
            .lock()
            .expect("image requests lock")
            .push(request);
        ImageLookup::Pending(PendingImage::new(
            test_image_load(9),
            ImageLayoutExtent::new(0, 0),
        ))
    }

    fn invalidate(&self, invalidation: ImageInvalidation) -> ImageInvalidationResult {
        if invalidation == ImageInvalidation::All {
            *self.clear_all_calls.lock().expect("image clear_all lock") += 1;
        }
        self.invalidations
            .lock()
            .expect("image invalidations lock")
            .push(invalidation);
        ImageInvalidationResult::Changed
    }

    fn invalidate_animation(
        &self,
        invalidation: ImageAnimationInvalidation,
    ) -> ImageInvalidationResult {
        self.animation_invalidations
            .lock()
            .expect("animation invalidations lock")
            .push(invalidation);
        ImageInvalidationResult::Changed
    }
}

// -----------------------------------------------------------------------
// image-type-available-p
// -----------------------------------------------------------------------

#[test]
fn image_type_domain_matches_gnu_available_symbols() {
    crate::test_utils::init_test_tracing();
    assert_eq!(ImageType::from_symbol_name("svg"), Some(ImageType::Svg));
    assert_eq!(ImageType::from_symbol_name("webp"), Some(ImageType::Webp));
    assert_eq!(ImageType::from_symbol_name("png"), Some(ImageType::Png));
    assert_eq!(ImageType::from_symbol_name("gif"), Some(ImageType::Gif));
    assert_eq!(ImageType::from_symbol_name("tiff"), Some(ImageType::Tiff));
    assert_eq!(ImageType::from_symbol_name("jpeg"), Some(ImageType::Jpeg));
    assert_eq!(ImageType::from_symbol_name("xpm"), Some(ImageType::Xpm));
    assert_eq!(ImageType::from_symbol_name("xbm"), Some(ImageType::Xbm));
    assert_eq!(ImageType::from_symbol_name("pbm"), Some(ImageType::Pbm));
    assert_eq!(ImageType::from_symbol_name("jpg"), None);
    assert_eq!(ImageType::from_symbol_name("bmp"), None);
    assert_eq!(ImageType::from_file_extension("bmp"), None);
    assert_eq!(ImageType::from_file_extension("ps"), None);
    assert_eq!(ImageType::from_file_extension("heic"), None);

    assert_eq!(normalize_image_type_name("JPG"), Some("jpeg"));
    assert_eq!(normalize_image_type_name("TIF"), Some("tiff"));
    assert_eq!(normalize_image_type_name("PNG"), Some("png"));
    assert_eq!(normalize_image_type_name("PS"), Some("postscript"));
    assert_eq!(normalize_image_type_name("HEIC"), Some("heic"));
    assert_eq!(normalize_image_type_name("HEIFS"), Some("heic"));
    assert_eq!(normalize_image_type_name("NEOMACS"), None);
    assert_eq!(ImageType::from_file_extension("jpg"), Some(ImageType::Jpeg));
    assert_eq!(
        ImageType::from_file_extension(".tif"),
        Some(ImageType::Tiff)
    );
    assert_eq!(ImageType::from_file_extension("SVGZ"), Some(ImageType::Svg));
    assert_eq!(
        ImageType::from_file_name("toolbar.SEARCH.PNG"),
        Some(ImageType::Png)
    );
    assert_eq!(
        ImageFilenameType::from_file_extension("bmp"),
        Some(ImageFilenameType::Bmp)
    );
    assert_eq!(
        ImageFilenameType::from_file_extension(".ps"),
        Some(ImageFilenameType::Postscript)
    );
    assert_eq!(
        ImageFilenameType::from_file_extension("heifs"),
        Some(ImageFilenameType::Heic)
    );
}

#[test]
fn image_spec_key_domain_matches_gnu_image_keywords() {
    for (keyword, parsed) in [
        (":type", ImageSpecKey::Type),
        (":file", ImageSpecKey::File),
        (":data", ImageSpecKey::Data),
        (":width", ImageSpecKey::Width),
        (":height", ImageSpecKey::Height),
        (":foreground", ImageSpecKey::Foreground),
        (":background", ImageSpecKey::Background),
        (":ascent", ImageSpecKey::Ascent),
        (":margin", ImageSpecKey::Margin),
        (":relief", ImageSpecKey::Relief),
        (":conversion", ImageSpecKey::Conversion),
        (":color-symbols", ImageSpecKey::ColorSymbols),
        (":heuristic-mask", ImageSpecKey::HeuristicMask),
        (":index", ImageSpecKey::Index),
        (":crop", ImageSpecKey::Crop),
        (":rotation", ImageSpecKey::Rotation),
        (":matrix", ImageSpecKey::Matrix),
        (":scale", ImageSpecKey::Scale),
        (":transform-smoothing", ImageSpecKey::TransformSmoothing),
        (":color-adjustment", ImageSpecKey::ColorAdjustment),
        (":mask", ImageSpecKey::Mask),
        (":flip", ImageSpecKey::Flip),
        (":max-width", ImageSpecKey::MaxWidth),
        (":max-height", ImageSpecKey::MaxHeight),
        (":loader", ImageSpecKey::Loader),
        (":pt-width", ImageSpecKey::PtWidth),
        (":pt-height", ImageSpecKey::PtHeight),
        (":base-uri", ImageSpecKey::BaseUri),
        (":css", ImageSpecKey::Css),
        (":animate-buffer", ImageSpecKey::AnimateBuffer),
        (":animate-tardiness", ImageSpecKey::AnimateTardiness),
        (":animate-position", ImageSpecKey::AnimatePosition),
        (":format", ImageSpecKey::Format),
    ] {
        assert_eq!(
            ImageSpecKey::from_lisp_value(Value::keyword(keyword)),
            Some(parsed)
        );
        assert_eq!(parsed.keyword(), keyword);
        assert_eq!(parsed.value(), Value::keyword(keyword));
        assert!(parsed.is_value(Value::keyword(keyword)));
    }
    assert_eq!(ImageSpecKey::from_lisp_value(Value::symbol("type")), None);
}

#[test]
fn type_available_png() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type_available_p(vec![Value::symbol("png")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_truthy());
}

#[test]
fn type_available_jpeg() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type_available_p(vec![Value::symbol("jpeg")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_truthy());
}

#[test]
fn type_available_gif() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type_available_p(vec![Value::symbol("gif")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_truthy());
}

#[test]
fn type_available_svg() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type_available_p(vec![Value::symbol("svg")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_truthy());
}

#[test]
fn type_available_webp() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type_available_p(vec![Value::symbol("webp")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_truthy());
}

#[test]
fn type_available_neomacs() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type_available_p(vec![Value::symbol("neomacs")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn type_available_jpg_alias_is_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type_available_p(vec![Value::symbol("jpg")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn type_available_bmp_matches_gnu_linux_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type_available_p(vec![Value::symbol("bmp")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn type_available_unknown() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type_available_p(vec![Value::symbol("heic")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn type_available_wrong_type() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type_available_p(vec![Value::fixnum(42)]);
    assert!(result.is_err());
}

#[test]
fn type_available_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type_available_p(vec![]);
    assert!(result.is_err());
}

// -----------------------------------------------------------------------
// create-image
// -----------------------------------------------------------------------

#[test]
fn create_image_file() {
    crate::test_utils::init_test_tracing();
    let result = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]);
    assert!(result.is_ok());
    let spec = result.unwrap();
    assert!(is_image_spec(&spec));

    let plist = image_spec_plist(&spec);
    let img_type = plist_get(&plist, &Value::keyword("type"));
    assert_eq!(img_type.as_symbol_name(), Some("png"));

    let file = plist_get(&plist, &Value::keyword("file"));
    assert_eq!(file.as_utf8_str(), Some("test.png"));
}

#[test]
fn create_image_data() {
    crate::test_utils::init_test_tracing();
    let result = builtin_create_image(vec![
        Value::string("raw-png-data"),
        Value::symbol("png"),
        Value::T, // DATA-P
    ]);
    assert!(result.is_ok());
    let spec = result.unwrap();

    let plist = image_spec_plist(&spec);
    let data = plist_get(&plist, &Value::keyword("data"));
    assert_eq!(data.as_utf8_str(), Some("raw-png-data"));

    // Should NOT have :file.
    let file = plist_get(&plist, &Value::keyword("file"));
    assert!(file.is_nil());
}

#[test]
fn create_image_file_accepts_raw_unibyte_name() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(
        b"test-\xFF.png".to_vec(),
    ));
    let result = builtin_create_image(vec![raw, Value::symbol("png")]);
    assert!(result.is_ok());
    let spec = result.unwrap();
    assert!(is_image_spec(&spec));
}

#[test]
fn create_image_data_accepts_raw_unibyte_payload() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let result = builtin_create_image(vec![raw, Value::symbol("png"), Value::T]);
    assert!(result.is_ok());
    let spec = result.unwrap();
    assert!(is_image_spec(&spec));
}

#[test]
fn create_image_default_type() {
    crate::test_utils::init_test_tracing();
    let result = builtin_create_image(vec![Value::string("foo.png")]);
    assert!(result.is_ok());
    let spec = result.unwrap();

    let plist = image_spec_plist(&spec);
    let img_type = plist_get(&plist, &Value::keyword("type"));
    assert_eq!(img_type.as_symbol_name(), Some("png"));
}

#[test]
fn create_image_default_type_from_jpg_extension() {
    crate::test_utils::init_test_tracing();
    let result = builtin_create_image(vec![Value::string("foo.JPG")]);
    assert!(result.is_ok());
    let spec = result.unwrap();

    let plist = image_spec_plist(&spec);
    let img_type = plist_get(&plist, &Value::keyword("type"));
    assert_eq!(img_type.as_symbol_name(), Some("jpeg"));
}

#[test]
fn create_image_unknown_extension_type_is_nil_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = builtin_create_image(vec![Value::string("foo.unknown")]);
    assert!(result.is_ok());
    let spec = result.unwrap();

    let plist = image_spec_plist(&spec);
    let img_type = plist_get(&plist, &Value::keyword("type"));
    assert!(img_type.is_nil());
}

#[test]
fn create_image_infers_unavailable_gnu_filename_types() {
    crate::test_utils::init_test_tracing();
    for (file_name, expected_type) in [
        ("foo.BMP", "bmp"),
        ("foo.ps", "postscript"),
        ("foo.heic", "heic"),
        ("foo.HEIFS", "heic"),
    ] {
        let result = builtin_create_image(vec![Value::string(file_name)]);
        assert!(result.is_ok());
        let spec = result.unwrap();

        let plist = image_spec_plist(&spec);
        let img_type = plist_get(&plist, &Value::keyword("type"));
        assert_eq!(
            img_type.as_symbol_name(),
            Some(expected_type),
            "type inferred for {file_name}"
        );
    }
}

#[test]
fn create_image_data_type_from_mime_hint() {
    crate::test_utils::init_test_tracing();
    let result = builtin_create_image(vec![
        Value::string("raw-image-bytes"),
        Value::NIL,
        Value::symbol("image/jpeg"),
    ]);
    assert!(result.is_ok());
    let spec = result.unwrap();

    let plist = image_spec_plist(&spec);
    let img_type = plist_get(&plist, &Value::keyword("type"));
    assert!(img_type.is_nil());
}

#[test]
fn create_image_with_props() {
    crate::test_utils::init_test_tracing();
    let result = builtin_create_image(vec![
        Value::string("icon.svg"),
        Value::symbol("svg"),
        Value::NIL,
        Value::keyword("width"),
        Value::fixnum(64),
        Value::keyword("height"),
        Value::fixnum(64),
    ]);
    assert!(result.is_ok());
    let spec = result.unwrap();

    let plist = image_spec_plist(&spec);
    let width = plist_get(&plist, &Value::keyword("width"));
    assert_eq!(width.as_int(), Some(64));

    let height = plist_get(&plist, &Value::keyword("height"));
    assert_eq!(height.as_int(), Some(64));
}

#[test]
fn create_image_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let result = builtin_create_image(vec![]);
    assert!(result.is_err());
}

#[test]
fn create_image_bad_type() {
    crate::test_utils::init_test_tracing();
    let result = builtin_create_image(vec![
        Value::string("test.png"),
        Value::fixnum(42), // not a symbol
    ]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "error"
    ));
}

// -----------------------------------------------------------------------
// image-size
// -----------------------------------------------------------------------

#[test]
fn image_size_pixels() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();

    let result = builtin_image_size(vec![spec, Value::T]);
    assert!(result.is_err());
}

#[test]
fn image_size_chars() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();

    let result = builtin_image_size(vec![spec]);
    assert!(result.is_err());
}

#[test]
fn image_size_not_image_spec() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_size(vec![Value::fixnum(42)]);
    assert!(result.is_err());
}

#[test]
fn image_size_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_size(vec![]);
    assert!(result.is_err());
}

// -----------------------------------------------------------------------
// image-mask-p
// -----------------------------------------------------------------------

#[test]
fn image_mask_p_batch_errors_without_window_system() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();

    let result = builtin_image_mask_p(vec![spec]);
    assert!(result.is_err());
}

#[test]
fn image_mask_p_not_image() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_mask_p(vec![Value::string("not an image")]);
    assert!(result.is_err());
}

#[test]
fn image_mask_p_resolves_image_and_reports_a_clipping_mask_on_gui_frame() {
    crate::test_utils::init_test_tracing();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol("neo")));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        requests: Arc::clone(&requests),
        ..Default::default()
    }));
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();

    // Recording host publishes an actual clipping-mask kind.
    let result = builtin_image_mask_p_in_context(&mut eval, vec![spec]).unwrap();

    assert_eq!(result, Value::T);
    assert_eq!(requests.lock().expect("image requests lock").len(), 1);
}

#[test]
fn image_mask_p_does_not_confuse_continuous_alpha_with_a_clipping_mask() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol("neo")));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        fixed_mask: Some(ImageMaskKind::AlphaChannel),
        ..Default::default()
    }));
    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::keyword(":type"),
        Value::symbol("png"),
        Value::keyword(":file"),
        Value::string("alpha.png"),
    ]);

    let result = builtin_image_mask_p_in_context(&mut eval, vec![spec]).unwrap();

    assert!(result.is_nil());
}

// -----------------------------------------------------------------------
// put-image
// -----------------------------------------------------------------------

#[test]
fn image_area_domain_matches_gnu_margin_symbols() {
    assert_eq!(
        ImageArea::from_symbol_value(Value::symbol("left-margin")),
        Some(ImageArea::LeftMargin)
    );
    assert_eq!(
        ImageArea::from_symbol_value(Value::symbol("right-margin")),
        Some(ImageArea::RightMargin)
    );
    assert_eq!(ImageArea::from_symbol_value(Value::NIL), None);
    assert_eq!(
        ImageArea::from_symbol_value(Value::symbol("Left-Margin")),
        None
    );
    assert_eq!(ImageArea::from_symbol_value(Value::symbol("center")), None);
}

#[test]
fn put_image_requires_image_and_point() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();

    let result = builtin_put_image(vec![spec, Value::fixnum(1)]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_truthy());
}

#[test]
fn put_image_accepts_char_point() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();

    let result = builtin_put_image(vec![spec, Value::char('a')]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_truthy());
}

#[test]
fn put_image_bad_point() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();

    let result = builtin_put_image(vec![spec, Value::string("not a point")]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
            && sig.data.first() == Some(&Value::symbol("integer-or-marker-p"))
    ));
}

#[test]
fn put_image_invalid_area() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();
    let result = builtin_put_image(vec![
        spec,
        Value::fixnum(1),
        Value::NIL,
        Value::symbol("center"),
    ]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "error"
            && sig.data.first() == Some(&Value::string("Invalid area center"))
    ));
}

#[test]
fn put_image_not_image() {
    crate::test_utils::init_test_tracing();
    let result = builtin_put_image(vec![Value::fixnum(1), Value::fixnum(1)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "error"
            && sig.data.first() == Some(&Value::string("Not an image: 1"))
    ));
}

// -----------------------------------------------------------------------
// insert-image
// -----------------------------------------------------------------------

#[test]
fn insert_image_without_position_returns_true() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();

    let result = builtin_insert_image(vec![spec]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Value::T);
}

#[test]
fn insert_image_not_image() {
    crate::test_utils::init_test_tracing();
    let result = builtin_insert_image(vec![Value::fixnum(42)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "error"
            && sig.data.first() == Some(&Value::string("Not an image: 42"))
    ));
}

#[test]
fn insert_image_invalid_area() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();
    let result = builtin_insert_image(vec![spec, Value::NIL, Value::symbol("center")]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "error"
            && sig.data.first() == Some(&Value::string("Invalid area center"))
    ));
}

#[test]
fn insert_image_too_many_args() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();
    let result = builtin_insert_image(vec![
        spec,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    assert!(result.is_err());
}

// -----------------------------------------------------------------------
// remove-images
// -----------------------------------------------------------------------

#[test]
fn remove_images_no_error_for_default_args() {
    crate::test_utils::init_test_tracing();
    let result = builtin_remove_images(vec![Value::fixnum(1), Value::fixnum(100)]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn remove_images_accepts_char_positions() {
    crate::test_utils::init_test_tracing();
    let result = builtin_remove_images(vec![Value::char('a'), Value::char('z')]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn remove_images_bad_start() {
    crate::test_utils::init_test_tracing();
    let result = builtin_remove_images(vec![Value::string("x"), Value::fixnum(100)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
            && sig.data.first() == Some(&Value::symbol("integer-or-marker-p"))
    ));
}

#[test]
fn remove_images_bad_end() {
    crate::test_utils::init_test_tracing();
    let result = builtin_remove_images(vec![Value::fixnum(1), Value::string("x")]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
            && sig.data.first() == Some(&Value::symbol("integer-or-marker-p"))
    ));
}

#[test]
fn remove_images_bad_buffer() {
    crate::test_utils::init_test_tracing();
    let result = builtin_remove_images(vec![Value::fixnum(1), Value::fixnum(10), Value::fixnum(1)]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn remove_images_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let result = builtin_remove_images(vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

// -----------------------------------------------------------------------
// image-flush
// -----------------------------------------------------------------------

#[test]
fn image_flush_rejects_non_window_frame() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();

    let result = builtin_image_flush(vec![spec]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "error"
            && sig.data.first() == Some(&Value::string("Window system frame should be used"))
    ));
}

#[test]
fn image_flush_accepts_selected_neo_window_system_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*scratch*");
    let frame = eval.frames.create_frame("F1", 960, 640, buffer);
    eval.frames
        .get_mut(frame)
        .expect("test frame")
        .set_window_system(Some(Value::symbol("neo")));
    eval.set_display_host(Box::new(RecordingImageDisplayHost::default()));

    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();
    let result = builtin_image_flush_in_context(&mut eval, vec![spec]);

    assert!(
        result
            .expect("image-flush should accept GUI frame")
            .is_nil()
    );
}

#[test]
fn image_flush_lisp_call_accepts_selected_neo_window_system_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*scratch*");
    let frame = eval.frames.create_frame("F1", 960, 640, buffer);
    eval.frames
        .get_mut(frame)
        .expect("test frame")
        .set_window_system(Some(Value::symbol("neo")));
    eval.set_display_host(Box::new(RecordingImageDisplayHost::default()));

    let result = eval
        .eval_str(r#"(image-flush '(image :type png :file "test.png"))"#)
        .expect("Lisp image-flush should accept GUI frame");

    assert!(result.is_nil());
}

#[test]
fn image_flush_invalidates_exact_spec_across_its_face_variants() {
    crate::test_utils::init_test_tracing();
    let invalidations = Arc::new(Mutex::new(Vec::new()));
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*scratch*");
    let frame = eval.frames.create_frame("F1", 960, 640, buffer);
    eval.frames
        .get_mut(frame)
        .expect("test frame")
        .set_window_system(Some(Value::symbol("neo")));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        invalidations: Arc::clone(&invalidations),
        ..Default::default()
    }));

    let spec = builtin_create_image(vec![
        Value::string("/tmp/watched.svg"),
        Value::symbol("svg"),
    ])
    .unwrap();
    let media_generation = eval.media_generation();
    builtin_image_flush_in_context(&mut eval, vec![spec]).unwrap();

    assert!(matches!(
        invalidations
            .lock()
            .expect("image invalidations lock")
            .as_slice(),
        [ImageInvalidation::Spec { .. }]
    ));
    assert_ne!(
        eval.media_generation(),
        media_generation,
        "logical invalidation must dirty redisplay before renderer retirement"
    );
}

#[test]
fn image_flush_all_frames() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();
    let result = builtin_image_flush(vec![spec, Value::T]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn image_flush_all_frames_invalidates_spec_with_display_host() {
    crate::test_utils::init_test_tracing();
    let invalidations = Arc::new(Mutex::new(Vec::new()));
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*scratch*");
    let frame = eval.frames.create_frame("F1", 960, 640, buffer);
    eval.frames
        .get_mut(frame)
        .expect("test frame")
        .set_window_system(Some(Value::symbol("neo")));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        invalidations: Arc::clone(&invalidations),
        ..Default::default()
    }));

    let spec = builtin_create_image(vec![
        Value::string("/tmp/watched-all-frames.png"),
        Value::symbol("png"),
    ])
    .unwrap();
    builtin_image_flush_in_context(&mut eval, vec![spec, Value::T]).unwrap();

    assert!(matches!(
        invalidations
            .lock()
            .expect("image invalidations lock")
            .as_slice(),
        [ImageInvalidation::Spec { .. }]
    ));
}

#[test]
fn image_flush_non_t_frame_errors() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();
    let result = builtin_image_flush(vec![spec, Value::fixnum(1)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data.first() == Some(&Value::symbol("frame-live-p"))
    ));
}

#[test]
fn image_flush_not_image() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_flush(vec![Value::fixnum(42)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "error"
            && sig.data.first() == Some(&Value::string("Invalid image specification"))
    ));
}

// -----------------------------------------------------------------------
// clear-image-cache
// -----------------------------------------------------------------------

#[test]
fn clear_image_cache_no_args() {
    crate::test_utils::init_test_tracing();
    let result = builtin_clear_image_cache(vec![]);
    assert!(result.is_err());
}

#[test]
fn clear_image_cache_nil_filter_errors() {
    crate::test_utils::init_test_tracing();
    let result = builtin_clear_image_cache(vec![Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn clear_image_cache_with_filter() {
    crate::test_utils::init_test_tracing();
    let result = builtin_clear_image_cache(vec![Value::T]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn clear_image_cache_animation_cache_non_list() {
    crate::test_utils::init_test_tracing();
    let result = builtin_clear_image_cache(vec![Value::T, Value::T]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
            && sig.data.first() == Some(&Value::symbol("listp"))
    ));
}

#[test]
fn clear_image_cache_nil_second_arg_but_valid_filter() {
    crate::test_utils::init_test_tracing();
    let result = builtin_clear_image_cache(vec![Value::T, Value::NIL]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn clear_image_cache_with_animation_cache_list() {
    crate::test_utils::init_test_tracing();
    let cache_arg = Value::list(vec![Value::symbol("foo"), Value::symbol("bar")]);
    let result = builtin_clear_image_cache(vec![Value::T, cache_arg]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn clear_image_cache_nil_filter_clears_catalog_with_gui_host() {
    crate::test_utils::init_test_tracing();
    let clear_all_calls = Arc::new(Mutex::new(0usize));
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol("neo")));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        clear_all_calls: Arc::clone(&clear_all_calls),
        ..Default::default()
    }));

    builtin_clear_image_cache_in_context(&mut eval, vec![]).unwrap();
    builtin_clear_image_cache_in_context(&mut eval, vec![Value::NIL]).unwrap();
    builtin_clear_image_cache_in_context(&mut eval, vec![Value::T]).unwrap();

    assert_eq!(*clear_all_calls.lock().unwrap(), 3);
}

#[test]
fn clear_image_cache_filename_filter_invalidates_source() {
    crate::test_utils::init_test_tracing();
    let invalidations = Arc::new(Mutex::new(Vec::new()));
    let mut eval = Context::new();
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        invalidations: Arc::clone(&invalidations),
        ..Default::default()
    }));

    builtin_clear_image_cache_in_context(&mut eval, vec![Value::string("/tmp/only-this.png")])
        .unwrap();

    assert!(matches!(
        invalidations.lock().unwrap().as_slice(),
        [ImageInvalidation::Dependency(ImageResolveSource::File(path))]
            if path.as_utf8_str() == Some("/tmp/only-this.png")
    ));
}

#[test]
fn clear_image_cache_animation_filter_skips_image_clear() {
    crate::test_utils::init_test_tracing();
    let clear_all_calls = Arc::new(Mutex::new(0usize));
    let mut eval = Context::new();
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        clear_all_calls: Arc::clone(&clear_all_calls),
        ..Default::default()
    }));
    let anim = Value::list(vec![Value::symbol("anim"), Value::symbol("entry")]);
    builtin_clear_image_cache_in_context(&mut eval, vec![Value::T, anim]).unwrap();
    assert_eq!(*clear_all_calls.lock().unwrap(), 0);
}

#[test]
fn clear_image_cache_animation_filter_retires_only_its_sequence_source() {
    crate::test_utils::init_test_tracing();
    let image_invalidations = Arc::new(Mutex::new(Vec::new()));
    let animation_invalidations = Arc::new(Mutex::new(Vec::new()));
    let mut eval = Context::new();
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        invalidations: Arc::clone(&image_invalidations),
        animation_invalidations: Arc::clone(&animation_invalidations),
        ..Default::default()
    }));
    let image = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("gif"),
        Value::keyword("file"),
        Value::string("/tmp/animated.gif"),
        Value::keyword("index"),
        Value::fixnum(3),
    ]);

    builtin_clear_image_cache_in_context(&mut eval, vec![Value::NIL, image]).unwrap();

    assert!(image_invalidations.lock().unwrap().is_empty());
    assert!(matches!(
        animation_invalidations.lock().unwrap().as_slice(),
        [ImageAnimationInvalidation::Source(ImageResolveSource::File(path))]
            if path.as_utf8_str() == Some("/tmp/animated.gif")
    ));
}

#[test]
fn image_cache_size_is_zero() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_cache_size(vec![]);
    assert_eq!(result.unwrap(), Value::fixnum(0));
}

#[test]
fn image_c_variables_match_gnu_defaults() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        eval.eval_str(
            "(list (boundp 'image-cache-eviction-delay)
                   image-cache-eviction-delay
                   (boundp 'max-image-size)
                   max-image-size
                   (fboundp 'image-cache-size)
                   (boundp 'image-cache-size))"
        )
        .expect("image variables should be readable"),
        Value::list(vec![
            Value::T,
            Value::fixnum(300),
            Value::T,
            Value::make_float(10.0),
            Value::T,
            Value::NIL,
        ])
    );
}

#[test]
fn imagep_matches_image_spec_shape() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")])
        .expect("create-image should succeed");
    assert!(builtin_imagep(vec![spec]).unwrap().is_truthy());
    assert!(builtin_imagep(vec![Value::fixnum(1)]).unwrap().is_nil());
    assert!(
        builtin_imagep(vec![Value::list(vec![
            Value::symbol("image"),
            Value::keyword("type"),
            Value::symbol("png"),
        ])])
        .unwrap()
        .is_nil()
    );
    assert!(
        builtin_imagep(vec![Value::list(vec![
            Value::symbol("image"),
            Value::keyword("file"),
            Value::string("x.png"),
        ])])
        .unwrap()
        .is_nil()
    );
    assert!(
        builtin_imagep(vec![Value::list(vec![
            Value::symbol("image"),
            Value::symbol("type"),
            Value::symbol("png"),
            Value::symbol("file"),
            Value::string("x.png"),
        ])])
        .unwrap()
        .is_nil(),
        "GNU valid_image_p requires exact keyword symbols such as :type and :file"
    );
}

#[test]
fn image_metadata_non_spec_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_metadata(vec![Value::fixnum(1)]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn image_metadata_window_system_error_shape() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")])
        .expect("create-image should succeed");
    let result = builtin_image_metadata(vec![spec]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "error"
            && sig.data.first() == Some(&Value::string("Window system frame should be used"))
    ));
}

#[test]
fn image_metadata_second_arg_validates_frame_designator() {
    crate::test_utils::init_test_tracing();
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")])
        .expect("create-image should succeed");
    let result = builtin_image_metadata(vec![spec, Value::T]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
            && sig.data.first() == Some(&Value::symbol("frame-live-p"))
    ));
}

#[test]
fn image_metadata_returns_nil_on_gui_frame_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol("neo")));
    eval.set_display_host(Box::new(RecordingImageDisplayHost::default()));
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();

    // GNU image-metadata returns the decoder's lisp_data, nil for a plain
    // image; Neomacs matches that and surfaces geometry via
    // neomacs-image-extent instead.
    let meta = builtin_image_metadata_in_context(&mut eval, vec![spec]).unwrap();
    assert!(
        meta.is_nil(),
        "image-metadata should be nil like GNU, got {meta:?}"
    );
}

#[test]
fn image_metadata_converts_typed_animation_metadata_at_the_lisp_boundary() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol("neo")));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        fixed_embedded: ImageEmbeddedMetadata::animation(
            2,
            ImageFrameDelay::milliseconds(40, 1).unwrap(),
        ),
        ..RecordingImageDisplayHost::default()
    }));
    let spec =
        builtin_create_image(vec![Value::string("animated.gif"), Value::symbol("gif")]).unwrap();

    let metadata = builtin_image_metadata_in_context(&mut eval, vec![spec]).unwrap();
    let items = list_to_vec(&metadata).expect("GNU metadata plist");
    assert_eq!(
        items,
        vec![
            Value::symbol("count"),
            Value::fixnum(2),
            Value::symbol("delay"),
            Value::make_float(0.04),
        ]
    );
}

#[test]
fn image_metadata_preserves_gnu_default_animation_delay_marker() {
    crate::test_utils::init_test_tracing();
    let metadata = image_embedded_metadata_to_lisp(&ImageEmbeddedMetadata::animation(
        2,
        ImageFrameDelay::UseDefault,
    ));

    assert_eq!(
        list_to_vec(&metadata).expect("GNU metadata plist"),
        vec![
            Value::symbol("count"),
            Value::fixnum(2),
            Value::symbol("delay"),
            Value::T,
        ]
    );
}

#[test]
fn neomacs_image_extent_returns_size_plist_on_gui_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol("neo")));
    eval.set_display_host(Box::new(RecordingImageDisplayHost::default()));
    let spec = builtin_create_image(vec![Value::string("test.png"), Value::symbol("png")]).unwrap();

    let meta = builtin_neomacs_image_extent_in_context(&mut eval, vec![spec]).unwrap();
    let items = list_to_vec(&meta).expect("extent plist");
    assert!(
        items
            .windows(2)
            .any(|w| { w[0] == Value::keyword("width") && w[1] == Value::fixnum(40) })
    );
    assert!(
        items
            .windows(2)
            .any(|w| { w[0] == Value::keyword("height") && w[1] == Value::fixnum(30) })
    );
    // Without :scale default, layout equals image-pixel space.
    assert!(
        items
            .windows(2)
            .any(|w| { w[0] == Value::keyword("pixel-width") && w[1] == Value::fixnum(40) })
    );
    assert!(
        items
            .windows(2)
            .any(|w| { w[0] == Value::keyword("pixel-height") && w[1] == Value::fixnum(30) })
    );
    assert!(
        items
            .windows(2)
            .any(|w| { w[0] == Value::keyword("background-transparent") && w[1] == Value::T })
    );
}

// -----------------------------------------------------------------------
// image-type
// -----------------------------------------------------------------------

#[test]
fn image_type_png() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type(vec![Value::string("test.png")]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_symbol_name(), Some("png"));
}

#[test]
fn image_type_svg() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type(vec![Value::string("icon.svg")]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_symbol_name(), Some("svg"));
}

#[test]
fn image_type_not_image() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type(vec![Value::fixnum(42)]);
    assert!(result.is_err());
}

#[test]
fn image_type_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type(vec![]);
    assert!(result.is_err());
}

#[test]
fn image_type_from_filename_extension() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type(vec![Value::string("foo.JPG")]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_symbol_name(), Some("jpeg"));
}

#[test]
fn image_type_explicit_type() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type(vec![
        Value::string("no-extension"),
        Value::symbol("png"),
        Value::NIL,
    ]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_symbol_name(), Some("png"));
}

#[test]
fn image_type_unknown_signals() {
    crate::test_utils::init_test_tracing();
    let result = builtin_image_type(vec![Value::string("unknown.bin")]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "unknown-image-type"
    ));
}

// -----------------------------------------------------------------------
// image-transforms-p
// -----------------------------------------------------------------------

#[test]
fn image_transforms_p_reports_the_transforms_neomacs_implements() {
    // GNU returns the LIST of capabilities a window-system frame supports, not
    // `t` (src/image.c:12843). Neomacs scales images (`ImageScalePolicy` /
    // `ImageRealization`) but has no rotation in the pipeline, so it reports
    // `(scale)` — the same shape GNU's own scale-only build returns
    // (`HAVE_NTGUI` -> `list1 (Qscale)`, image.c:12867).
    //
    // Reporting this honestly is what lets telega.el past its startup
    // `cl-assert`: it accepts either imagemagick or native transforms, and
    // its own comment says imagemagick is NOT required when transforms exist.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol("neo")));

    let result = builtin_image_transforms_p(&mut eval, vec![]).expect("image-transforms-p");

    assert_eq!(
        list_to_vec(&result).expect("capability list"),
        vec![Value::symbol("scale"), Value::symbol("rotate90")],
        "a window-system frame advertises exactly what the pipeline implements"
    );
}

#[test]
fn image_transforms_p_is_nil_without_a_window_system() {
    // GNU gates the whole thing on FRAME_WINDOW_P: a TTY frame has no native
    // transforms. telega.el relies on this — it skips the requirement entirely
    // when there is no graphical frame.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;

    let result = builtin_image_transforms_p(&mut eval, vec![Value::make_frame(frame_id)])
        .expect("image-transforms-p");

    assert!(
        result.is_nil(),
        "TTY frames report no native transforms, got {result:?}"
    );
}

#[test]
fn image_transforms_p_with_frame() {
    crate::test_utils::init_test_tracing();
    // `nil` means "the selected frame"; a TTY selected frame reports nothing.
    let mut eval = crate::emacs_core::Context::new();
    crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let result = builtin_image_transforms_p(&mut eval, vec![Value::NIL]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn image_transforms_p_with_non_integer_or_small_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result = builtin_image_transforms_p(&mut eval, vec![Value::fixnum(1)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data
                    == vec![Value::symbol("frame-live-p"), Value::fixnum(1)]
    ));
}

#[test]
fn image_transforms_p_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result = builtin_image_transforms_p(&mut eval, vec![Value::NIL, Value::NIL]);
    assert!(result.is_err());
}

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

#[test]
fn plist_get_basic() {
    crate::test_utils::init_test_tracing();
    let plist = Value::list(vec![
        Value::keyword("type"),
        Value::symbol("png"),
        Value::keyword("file"),
        Value::string("test.png"),
    ]);
    let val = plist_get(&plist, &Value::keyword("type"));
    assert_eq!(val.as_symbol_name(), Some("png"));

    let file = plist_get(&plist, &Value::keyword("file"));
    assert_eq!(file.as_utf8_str(), Some("test.png"));
}

#[test]
fn plist_get_missing() {
    crate::test_utils::init_test_tracing();
    let plist = Value::list(vec![Value::keyword("type"), Value::symbol("png")]);
    let val = plist_get(&plist, &Value::keyword("missing"));
    assert!(val.is_nil());
}

#[test]
fn is_image_spec_valid() {
    crate::test_utils::init_test_tracing();
    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("png"),
        Value::keyword("file"),
        Value::string("test.png"),
    ]);
    assert!(is_image_spec(&spec));
}

#[test]
fn is_image_spec_rejects_bare_property_names_like_gnu() {
    crate::test_utils::init_test_tracing();
    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::symbol("type"),
        Value::symbol("png"),
        Value::symbol("file"),
        Value::string("test.png"),
    ]);
    assert!(!is_image_spec(&spec));
}

#[test]
fn is_image_spec_bare_plist() {
    crate::test_utils::init_test_tracing();
    let spec = Value::list(vec![Value::keyword("type"), Value::symbol("png")]);
    assert!(!is_image_spec(&spec));
}

#[test]
fn is_image_spec_not_image() {
    crate::test_utils::init_test_tracing();
    assert!(!is_image_spec(&Value::fixnum(42)));
    assert!(!is_image_spec(&Value::NIL));
    assert!(!is_image_spec(&Value::string("not an image")));
}

#[test]
fn is_image_spec_empty_list() {
    crate::test_utils::init_test_tracing();
    let spec = Value::list(vec![]);
    assert!(!is_image_spec(&spec));
}

#[test]
fn is_image_spec_requires_supported_type_and_one_source() {
    crate::test_utils::init_test_tracing();
    let valid_file = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("png"),
        Value::keyword("file"),
        Value::string("x.png"),
    ]);
    assert!(is_image_spec(&valid_file));

    let valid_data = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("png"),
        Value::keyword("data"),
        Value::string("raw"),
    ]);
    assert!(is_image_spec(&valid_data));

    let unsupported_type = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("jpg"),
        Value::keyword("file"),
        Value::string("x.jpg"),
    ]);
    assert!(!is_image_spec(&unsupported_type));

    let both_sources = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("png"),
        Value::keyword("file"),
        Value::string("x.png"),
        Value::keyword("data"),
        Value::string("raw"),
    ]);
    assert!(!is_image_spec(&both_sources));

    let missing_source = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("png"),
    ]);
    assert!(!is_image_spec(&missing_source));
}

#[test]
fn image_spec_plist_with_image_prefix() {
    crate::test_utils::init_test_tracing();
    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("png"),
    ]);
    let plist = image_spec_plist(&spec);
    let val = plist_get(&plist, &Value::keyword("type"));
    assert_eq!(val.as_symbol_name(), Some("png"));
}

#[test]
fn image_spec_plist_bare() {
    crate::test_utils::init_test_tracing();
    let spec = Value::list(vec![Value::keyword("type"), Value::symbol("jpeg")]);
    let plist = image_spec_plist(&spec);
    let val = plist_get(&plist, &Value::keyword("type"));
    assert_eq!(val.as_symbol_name(), Some("jpeg"));
}

#[test]
fn round_trip_create_then_type() {
    crate::test_utils::init_test_tracing();
    // `create-image` keeps the explicit :type marker in the resulting spec.
    let spec =
        builtin_create_image(vec![Value::string("photo.jpg"), Value::symbol("jpeg")]).unwrap();
    let plist = image_spec_plist(&spec);
    let img_type = plist_get(&plist, &Value::keyword("type"));
    assert_eq!(img_type.as_symbol_name(), Some("jpeg"));
}

#[test]
fn image_types_bootstrap_list_matches_gnu_linux_order() {
    crate::test_utils::init_test_tracing();
    let eval = crate::emacs_core::Context::new();
    let image_types = eval
        .obarray()
        .symbol_value("image-types")
        .copied()
        .expect("image-types should be bound");
    assert_eq!(
        image_types,
        Value::list(vec![
            Value::symbol("svg"),
            Value::symbol("webp"),
            Value::symbol("png"),
            Value::symbol("gif"),
            Value::symbol("tiff"),
            Value::symbol("jpeg"),
            Value::symbol("xpm"),
            Value::symbol("xbm"),
            Value::symbol("pbm"),
        ])
    );
}

#[test]
fn round_trip_create_then_size() {
    crate::test_utils::init_test_tracing();
    // In batch, image-size requires a window-system frame.
    let spec =
        builtin_create_image(vec![Value::string("photo.jpg"), Value::symbol("jpeg")]).unwrap();

    let result = builtin_image_size(vec![spec, Value::T]);
    assert!(result.is_err());
}

#[test]
fn image_size_uses_display_host_resolution_in_gui_context() {
    crate::test_utils::init_test_tracing();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol("neo")));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        requests: Arc::clone(&requests),
        ..Default::default()
    }));
    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("png"),
        Value::keyword("file"),
        Value::string("/tmp/neomacs-image-size.png"),
        Value::keyword("max-width"),
        Value::fixnum(40),
        Value::keyword("max-height"),
        Value::fixnum(30),
    ]);

    let result = builtin_image_size_in_context(&mut eval, vec![spec, Value::T]).unwrap();
    assert_eq!(result.cons_car(), Value::fixnum(40));
    assert_eq!(result.cons_cdr(), Value::fixnum(30));

    let requests = requests.lock().expect("image requests lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].size,
        ImageSizeSpec::new(AxisSize::AtMost(40), AxisSize::AtMost(30)),
        ":max-width/:max-height stay CLAMPS; only :width/:height are targets"
    );
}

/// On HiDPI, decoded metadata is logical; PIXELS=t must report GNU/image
/// pixels (layout × device_scale, ceil) — e.g. splash 266@1.25 → 333.
#[test]
fn image_size_pixels_t_converts_logical_layout_to_device_pixels() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    {
        let frame = eval.frames.get_mut(frame_id).expect("selected frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.device_scale_factor = 1.25;
        frame.char_width = 8.0;
        frame.char_height = 16.0;
    }
    // Host returns logical 266×186 (333/1.25, 233/1.25 rounded).
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        requests: Default::default(),
        invalidations: Default::default(),
        animation_invalidations: Default::default(),
        clear_all_calls: Default::default(),
        fixed_size: Some((266, 186)),
        fixed_mask: None,
        fixed_embedded: ImageEmbeddedMetadata::default(),
    }));
    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("svg"),
        Value::keyword("file"),
        Value::string("/nonexistent/splash.svg"),
        Value::keyword("scale"),
        Value::symbol("default"),
    ]);
    let in_pixels = builtin_image_size_in_context(&mut eval, vec![spec.clone(), Value::T]).unwrap();
    assert_eq!(in_pixels.cons_car(), Value::fixnum(333)); // ceil(266*1.25)
    assert_eq!(in_pixels.cons_cdr(), Value::fixnum(233)); // ceil(186*1.25)

    let in_chars = builtin_image_size_in_context(&mut eval, vec![spec]).unwrap();
    // Logical ÷ logical cell (not device pixels ÷ device column).
    assert_eq!(in_chars.cons_car(), Value::make_float(266.0 / 8.0));
    assert_eq!(in_chars.cons_cdr(), Value::make_float(186.0 / 16.0));
}

/// GNU includes 2×(:margin) and |:relief| in Fimage_size pixel extents.
#[test]
fn image_size_includes_margin_and_relief_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol("neo")));
    eval.set_display_host(Box::new(RecordingImageDisplayHost::default()));
    // Host returns 40×30; margin 5 → +10 each axis; relief 2 → +4 each.
    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("png"),
        Value::keyword("file"),
        Value::string("/nonexistent/margin-size.png"),
        Value::keyword("margin"),
        Value::fixnum(5),
        Value::keyword("relief"),
        Value::fixnum(2),
    ]);
    let result = builtin_image_size_in_context(&mut eval, vec![spec, Value::T]).unwrap();
    assert_eq!(result.cons_car(), Value::fixnum(40 + 2 * (5 + 2)));
    assert_eq!(result.cons_cdr(), Value::fixnum(30 + 2 * (5 + 2)));
}

/// GNU `Fimage_size`: PIXELS nil → floats in canonical character units
/// (`width / FRAME_COLUMN_WIDTH`, `height / FRAME_LINE_HEIGHT`); non-nil →
/// fixnum pixels. Regression for https://github.com/eval-exec/neomacs/issues/243.
#[test]
fn image_size_pixels_nil_returns_character_units_as_floats() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    {
        let frame = eval.frames.get_mut(frame_id).expect("selected frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.char_width = 10.0;
        frame.char_height = 20.0;
    }
    eval.set_display_host(Box::new(RecordingImageDisplayHost::default()));
    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("png"),
        Value::keyword("file"),
        Value::string("/tmp/neomacs-image-size-chars.png"),
    ]);

    // Recording host returns 40×30 metadata.
    let in_chars = builtin_image_size_in_context(&mut eval, vec![spec.clone()]).unwrap();
    assert_eq!(in_chars.cons_car(), Value::make_float(4.0)); // 40 / 10
    assert_eq!(in_chars.cons_cdr(), Value::make_float(1.5)); // 30 / 20

    let explicit_nil =
        builtin_image_size_in_context(&mut eval, vec![spec.clone(), Value::NIL]).unwrap();
    assert_eq!(explicit_nil.cons_car(), Value::make_float(4.0));
    assert_eq!(explicit_nil.cons_cdr(), Value::make_float(1.5));

    let in_pixels = builtin_image_size_in_context(&mut eval, vec![spec, Value::T]).unwrap();
    assert_eq!(in_pixels.cons_car(), Value::fixnum(40));
    assert_eq!(in_pixels.cons_cdr(), Value::fixnum(30));
}

#[test]
fn image_spec_parse_reduces_rotation_to_a_quarter_turn() {
    crate::test_utils::init_test_tracing();
    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("png"),
        Value::keyword("file"),
        Value::string("/tmp/x.png"),
        Value::keyword("rotation"),
        Value::fixnum(90),
    ]);
    let request = image_resolve_request_from_spec(&spec, ImageScaleEnvironment::default(), (0, 0))
        .expect("valid image spec");
    assert_eq!(request.rotation, ImageRotation::Quarter);
}

#[test]
fn image_spec_parses_frame_index_into_a_dedicated_type() {
    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("gif"),
        Value::keyword("file"),
        Value::string("/tmp/animated.gif"),
        Value::keyword("index"),
        Value::fixnum(7),
    ]);

    let request = image_resolve_request_from_spec(&spec, ImageScaleEnvironment::default(), (0, 0))
        .expect("valid image spec");

    assert_eq!(request.frame, ImageFrameIndex::new(7));
}

#[test]
fn image_spec_parses_gnu_mask_intent_into_a_closed_policy() {
    let request_for = |key: &str, value: Value| {
        let spec = Value::list(vec![
            Value::symbol("image"),
            Value::keyword("type"),
            Value::symbol("png"),
            Value::keyword("file"),
            Value::string("/tmp/x.png"),
            Value::keyword(key),
            value,
        ]);
        image_resolve_request_from_spec(&spec, ImageScaleEnvironment::default(), (0, 0))
            .expect("valid image spec")
            .mask
    };

    assert_eq!(request_for("mask", Value::NIL), ImageMaskPolicy::Suppress);
    assert_eq!(
        request_for("mask", Value::symbol("heuristic")),
        ImageMaskPolicy::Heuristic(ImageHeuristicMask::FourCorners)
    );
    assert_eq!(
        request_for(
            "mask",
            Value::list(vec![
                Value::symbol("heuristic"),
                Value::list(vec![
                    Value::fixnum(0xffff),
                    Value::fixnum(0x8000),
                    Value::fixnum(0),
                ]),
            ]),
        ),
        ImageMaskPolicy::Heuristic(ImageHeuristicMask::Rgb16([0xffff, 0x8000, 0]))
    );
    assert_eq!(
        request_for("heuristic-mask", Value::T),
        ImageMaskPolicy::Heuristic(ImageHeuristicMask::FourCorners)
    );

    let legacy_precedence = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("png"),
        Value::keyword("file"),
        Value::string("/tmp/x.png"),
        Value::keyword("mask"),
        Value::NIL,
        Value::keyword("heuristic-mask"),
        Value::T,
    ]);
    assert_eq!(
        image_resolve_request_from_spec(
            &legacy_precedence,
            ImageScaleEnvironment::default(),
            (0, 0),
        )
        .expect("valid image spec")
        .mask,
        ImageMaskPolicy::Heuristic(ImageHeuristicMask::FourCorners),
        "GNU's legacy non-nil :heuristic-mask takes precedence over :mask",
    );
}

/// GNU's `Fimage_size` calls `lookup_image (f, spec, -1)`, and `lookup_image`
/// maps a negative face id to `DEFAULT_FACE_ID` and keys the image cache on
/// THAT face's foreground/background (image.c: `search_image_cache` compares
/// `img->face_foreground`/`face_background`).
///
/// Neomacs hardcoded black colors here while the layout engine
/// built its request from the resolved face, so one image spec produced two
/// different cache keys — two catalog entries, two decodes and two GPU
/// textures for the same file.
#[test]
fn image_request_uses_the_default_face_colors_like_gnu() {
    let mut table = FaceTable::new();
    let mut default = table.resolve("default");
    default.foreground = Some(Color::rgb(0x11, 0x22, 0x33));
    default.background = Some(Color::rgb(0x44, 0x55, 0x66));
    table.define("default", default);

    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("png"),
        Value::keyword("file"),
        Value::string("/tmp/x.png"),
    ]);
    let request = image_resolve_request_from_spec(
        &spec,
        ImageScaleEnvironment::default(),
        table.default_face_colors(),
    )
    .expect("valid image spec");

    assert_eq!(request.colors.foreground().rgb24(), 0x00112233);
    assert_eq!(request.colors.background().rgb24(), 0x00445566);
}
