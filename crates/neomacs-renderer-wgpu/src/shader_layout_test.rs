use naga::proc::Layouter;

const BUILTIN_SHADERS: &[(&str, &str)] = &[
    ("glyph", include_str!("shaders/glyph.wgsl")),
    (
        "glyph_subpixel",
        include_str!("shaders/glyph_subpixel.wgsl"),
    ),
    ("image", include_str!("shaders/image.wgsl")),
    ("rect", include_str!("shaders/rect.wgsl")),
    ("rounded_rect", include_str!("shaders/rounded_rect.wgsl")),
    ("texture", include_str!("shaders/texture.wgsl")),
    (
        "video_biplanar",
        include_str!("shaders/video_biplanar.wgsl"),
    ),
];

#[test]
fn builtin_uniform_bindings_have_webgl2_portable_sizes() {
    for &(name, source) in BUILTIN_SHADERS {
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|error| panic!("{name} shader must parse: {error}"));
        let mut layouter = Layouter::default();
        layouter
            .update(module.to_ctx())
            .unwrap_or_else(|error| panic!("{name} shader types must have valid layouts: {error}"));
        let (_, uniform) = module
            .global_variables
            .iter()
            .find(|(_, variable)| {
                variable
                    .binding
                    .as_ref()
                    .is_some_and(|binding| binding.group == 0 && binding.binding == 0)
            })
            .unwrap_or_else(|| panic!("{name} shader must define group 0 binding 0"));
        let size = layouter[uniform.ty].size;

        assert_eq!(
            size % 16,
            0,
            "{name} shader's primary uniform is {size} bytes; WebGL2 requires a multiple of 16"
        );
    }
}
