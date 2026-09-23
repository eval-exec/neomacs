use super::*;

const TRIVIAL_WGSL: &str = "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(u.iResolution.xy, u.iTime, 1.0);
}";
const TRIVIAL_GLSL: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    fragColor = vec4(iResolution.xy, iTime, 1.0);
}";

/// The uniform block naga built, as `(member name, byte offset)` plus the
/// struct's byte span — the layout the GPU will actually use.
fn naga_block_layout(module: &naga::Module) -> (Vec<(String, u32)>, u32) {
    let uniform = module
        .global_variables
        .iter()
        .map(|(_, global)| global)
        .find(|global| {
            global.space == naga::AddressSpace::Uniform
                && matches!(
                    module.types[global.ty].inner,
                    naga::TypeInner::Struct { .. }
                )
        })
        .expect("prelude declares a uniform block");
    match &module.types[uniform.ty].inner {
        naga::TypeInner::Struct { members, span } => (
            members
                .iter()
                .map(|member| {
                    (
                        member.name.clone().expect("block members are named"),
                        member.offset,
                    )
                })
                .collect(),
            *span,
        ),
        _ => unreachable!("filtered to structs above"),
    }
}

#[test]
fn both_preludes_and_the_packed_struct_put_every_member_at_the_same_byte_offset() {
    // If this is false the packers write correct values to the wrong
    // places: a shader reads another member's bytes, or a slot's worth of
    // garbage, and nothing else in the stack — naga, wgpu, the pipeline
    // build — notices, because each of the three layouts is internally
    // valid on its own.
    let wgsl = compose_surface_wgsl(TRIVIAL_WGSL, &[], SurfaceContract::V1);
    let wgsl_module = naga::front::wgsl::parse_str(&wgsl).expect("prelude parses");
    let (wgsl_members, wgsl_span) = naga_block_layout(&wgsl_module);

    let glsl = compose_surface_glsl(TRIVIAL_GLSL, &[], SurfaceContract::V1);
    let mut frontend = naga::front::glsl::Frontend::default();
    let options = naga::front::glsl::Options::from(naga::ShaderStage::Fragment);
    let glsl_module = frontend.parse(&options, &glsl).expect("prelude parses");
    let (glsl_members, glsl_span) = naga_block_layout(&glsl_module);

    let expected: Vec<(String, u32)> = SURFACE_UNIFORM_BLOCK
        .iter()
        .enumerate()
        .map(|(index, member)| (member.wgsl_name.to_owned(), member_offset(index) as u32))
        .collect();
    assert_eq!(wgsl_members, expected);
    assert_eq!(
        glsl_members
            .iter()
            .map(|(_, offset)| *offset)
            .collect::<Vec<_>>(),
        expected
            .iter()
            .map(|(_, offset)| *offset)
            .collect::<Vec<_>>(),
    );
    assert_eq!(wgsl_span as u64, SURFACE_UNIFORM_BYTES);
    assert_eq!(glsl_span as u64, SURFACE_UNIFORM_BYTES);
    assert_eq!(
        std::mem::offset_of!(SurfaceUniforms, custom) as u32,
        expected[CUSTOM_MEMBER].1
    );
}

#[test]
fn every_member_the_glsl_block_renames_is_handed_back_under_its_wgsl_name() {
    // A rename nothing maps back makes a Shadertoy shader that ports
    // cleanly in one dialect fail to compile in the other, naming an
    // identifier the user never wrote.
    let glsl = compose_surface_glsl(
        TRIVIAL_GLSL,
        &[("speed".to_owned(), 1u8)],
        SurfaceContract::V1,
    );
    for member in SURFACE_UNIFORM_BLOCK
        .iter()
        .filter(|member| member.wgsl_name != member.glsl_name)
    {
        let mapped = format!("#define {} ", member.wgsl_name);
        let accessed = format!("{}[", member.glsl_name);
        assert!(
            glsl.contains(&mapped) || glsl.contains(&accessed),
            "{} is declared as {} with no way back to it",
            member.wgsl_name,
            member.glsl_name
        );
    }
}

#[test]
fn composing_stamps_the_contract_into_the_source_naga_reports_against() {
    // The composed module is the only artifact a user or a diagnostic ever
    // sees; unstamped, a shader written for one prelude and compiled
    // against another produces errors with nothing in them naming which
    // contract was applied.
    for source in [
        compose_surface_wgsl(TRIVIAL_WGSL, &[], SurfaceContract::V1),
        compose_surface_glsl(TRIVIAL_GLSL, &[], SurfaceContract::V1),
    ] {
        assert!(
            source.contains("neomacs shader-surface prelude v1 (generated"),
            "prelude does not name its contract: {source}"
        );
    }
}
