use super::*;

#[test]
fn image_frame_index_survives_layout_parsing_and_realization() {
    let mut eval = neovm_core::emacs_core::Context::new();
    eval.setup_thread_locals();
    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("gif"),
        Value::keyword("file"),
        Value::string("/tmp/animated.gif"),
        Value::keyword("index"),
        Value::fixnum(7),
    ]);

    let request = parse_display_image_layout(&spec, 0, 0)
        .expect("valid image spec")
        .into_resolve_request(
            ImageScaleEnvironment::default(),
            DisplayImageDimensionEnvironment::new(14.0, 18.0, 7.0),
        );

    assert_eq!(request.frame, ImageFrameIndex::new(7));
}
