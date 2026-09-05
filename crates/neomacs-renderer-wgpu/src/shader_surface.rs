//! Shader-surface WGSL/GLSL composition and validation.
//!
//! A shader surface (`docs/display-engine/SHADER_SURFACES.md`) is a texture the
//! compositor renders from a user-supplied fragment shader written against a
//! Shadertoy-compatible contract. Two source dialects are accepted
//! ([`SurfaceShaderLanguage`]):
//!
//! - WGSL — the user defines
//!   `fn mainImage(fragCoord: vec2<f32>) -> vec4<f32>`;
//! - GLSL (Shadertoy/Ghostty dialect) — the user defines
//!   `void mainImage(out vec4 fragColor, in vec2 fragCoord)` reading `iTime`,
//!   `iResolution`, `iMouse`, `iFrame`, `iTimeDelta`, `texture(iChannel0, uv)`.
//!
//! Each dialect gets a generated prelude (uniform block, channel bindings,
//! per-uniform accessor functions, entry point that flips `fragCoord` to
//! Shadertoy's y-up convention) declaring the *same* GPU interface at set=0
//! binding=0, with the channel texture and sampler at bindings 1 and 2. The
//! WGSL struct, the GLSL std140 block and the CPU-side [`SurfaceUniforms`] the
//! packers fill are all generated from the single [`surface_uniform_block!`]
//! invocation below, so the three cannot describe different buffers.
//! Composition is deterministic so the Lisp thread can validate with naga and
//! the render thread can compile the identical source.

/// Source language of a user surface shader.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceShaderLanguage {
    Wgsl,
    Glsl,
}

/// Which generated prelude a user shader was written against.
///
/// A user shader is source, never a compiled artifact — nothing in the
/// workspace persists a composed module, and `ShaderSurfaceCache::create_shader`
/// keeps only the pipeline — so the prelude *is* the whole compatibility
/// surface, and until now it had no name anything could disagree about.
///
/// An enum rather than a version number because a number admits contracts for
/// which no prelude exists, which then needs a runtime check and an error for
/// it; here [`compose_surface_wgsl`] and [`compose_surface_glsl`] match, so a
/// contract without a prelude to compose it against does not compile.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SurfaceContract {
    /// `docs/display-engine/SHADER_SURFACES.md`: `iResolution` / `iMouse` /
    /// `iTime` / `iTimeDelta` / `iFrame` plus eight user `vec4` slots in one
    /// uniform block at `@group(0) @binding(0)`, `iChannel0` and its sampler
    /// at bindings 1-2, Shadertoy y-up `fragCoord`, and one `u_<name>()`
    /// accessor per user uniform in declaration order.
    #[default]
    V1,
}

/// Number of user `vec4<f32>` uniform slots in the prelude's `custom` array.
pub const SURFACE_USER_UNIFORM_SLOTS: usize = 8;

/// How one uniform-block member is laid out and spelled in each dialect.
#[derive(Clone, Copy)]
enum MemberType {
    Vec4,
    F32,
    Vec4Array(usize),
}

impl MemberType {
    const fn std140_align(self) -> usize {
        match self {
            Self::F32 => 4,
            Self::Vec4 | Self::Vec4Array(_) => 16,
        }
    }

    const fn std140_size(self) -> usize {
        match self {
            Self::F32 => 4,
            Self::Vec4 => 16,
            Self::Vec4Array(slots) => 16 * slots,
        }
    }

    fn wgsl(self) -> String {
        match self {
            Self::Vec4 => "vec4<f32>".to_owned(),
            Self::F32 => "f32".to_owned(),
            Self::Vec4Array(slots) => format!("array<vec4<f32>, {slots}>"),
        }
    }

    const fn glsl(self) -> &'static str {
        match self {
            Self::Vec4 | Self::Vec4Array(_) => "vec4",
            Self::F32 => "float",
        }
    }

    /// GLSL spells an array as `type name[n]`, so the count rides after the
    /// member name instead of inside the type.
    fn glsl_array_suffix(self) -> String {
        match self {
            Self::Vec4Array(slots) => format!("[{slots}]"),
            Self::Vec4 | Self::F32 => String::new(),
        }
    }
}

/// One member of the shader-surface uniform block.
struct UniformMember {
    wgsl_name: &'static str,
    /// A GLSL block declares its members as bare globals, so a member whose
    /// Shadertoy name is reserved for a different type (`iResolution` is a
    /// `vec3`, `iFrame` an `int`) or would collide with the user's own code
    /// is declared under another name and handed back through a `#define`.
    /// Only the spelling differs — never the position in the block.
    glsl_name: &'static str,
    ty: MemberType,
    /// Emitted above the member in both preludes. The composed source is what
    /// naga prints diagnostics against and what a shader author reads, so a
    /// member's meaning is documented for them and for Rust from one place.
    doc: &'static [&'static str],
}

/// Declare the uniform block once, in every form that has to agree about it.
///
/// The block used to be written out five times — a WGSL struct, a GLSL std140
/// block, a hand-computed byte count, and two `[f32; 44]` packers addressed by
/// literal index — with nothing tying them together. Only growing the block
/// past the byte count was caught by anything: every reorder *within* 176
/// bytes compiled, validated, ran, and read garbage.
///
/// This expands to the `#[repr(C)]` struct the packers fill, the table both
/// preludes are generated from, and a const block asserting that the std140
/// offsets the table implies are the offsets the Rust struct actually has —
/// so a member added, reordered or retyped in one form fails the build rather
/// than shifting a slot nobody notices.
macro_rules! surface_uniform_block {
    ($(
        $(#[doc = $doc:literal])*
        $vis:vis $field:ident: $rust_ty:ty => $wgsl_name:literal / $glsl_name:literal, $ty:expr;
    )+) => {
        /// The uniform block as the CPU-side packers see it.
        ///
        /// Built with [`bytemuck::Zeroable::zeroed`] plus field assignment
        /// rather than a struct literal: the padding member is private, so no
        /// other module can spell a value that disagrees with the prelude, and
        /// the `Pod` derive refuses to compile a member that would open an
        /// implicit hole instead of an explicit one.
        #[repr(C)]
        #[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
        pub struct SurfaceUniforms {
            $($(#[doc = $doc])* $vis $field: $rust_ty,)+
        }

        const SURFACE_UNIFORM_BLOCK: &[UniformMember] = &[
            $(UniformMember {
                wgsl_name: $wgsl_name,
                glsl_name: $glsl_name,
                ty: $ty,
                doc: &[$($doc),*],
            },)+
        ];

        const _: () = {
            let mut index = 0;
            $(
                assert!(member_offset(index) == std::mem::offset_of!(SurfaceUniforms, $field));
                index += 1;
            )+
            assert!(index == SURFACE_UNIFORM_BLOCK.len());
            assert!(SURFACE_UNIFORM_BYTES as usize == surface_uniform_block_span());
        };
    };
}

surface_uniform_block! {
    /// xy: surface size in physical pixels; z: scale factor.
    pub i_resolution: [f32; 4] => "iResolution" / "iResolutionV", MemberType::Vec4;

    /// xy: hover position in physical px (origin bottom-left, y-up); persists
    /// at the last hover position when the pointer leaves.
    /// zw: click state — the press position (same mapping as xy), positive
    /// while a button is held over the surface, negated after release, 0 until
    /// the first click ever.
    pub i_mouse: [f32; 4] => "iMouse" / "iMouse", MemberType::Vec4;

    /// Seconds the surface has been animating, not wall-clock time.
    pub i_time: f32 => "iTime" / "iTime", MemberType::F32;

    /// Seconds since this surface last rendered, so a `:fps`-capped surface
    /// plays at the right speed rather than in slow motion.
    pub i_time_delta: f32 => "iTimeDelta" / "iTimeDelta", MemberType::F32;

    /// Frames this surface has rendered, wrapping.
    pub i_frame: f32 => "iFrame" / "iFrameF", MemberType::F32;

    /// Explicit padding to the 16-byte alignment std140 gives the array below.
    pad0: f32 => "_neo_pad0" / "_neo_pad0", MemberType::F32;

    /// User uniform slots, in the order the shader declared them; reached
    /// through the generated `u_<name>()` accessors, never by index.
    pub custom: [[f32; 4]; SURFACE_USER_UNIFORM_SLOTS]
        => "custom" / "neo_custom", MemberType::Vec4Array(SURFACE_USER_UNIFORM_SLOTS);
}

/// Members the prelude bodies name outside the block declaration itself.
const RESOLUTION_MEMBER: usize = 0;
const FRAME_MEMBER: usize = 4;
const CUSTOM_MEMBER: usize = 6;

const _: () = {
    // Reordering the block would leave these indices pointing at a member of
    // the same shape under a different name, and a `#define iResolution
    // (iMouse.xyz)` compiles, validates and renders the wrong thing.
    assert!(member_is(RESOLUTION_MEMBER, "iResolution"));
    assert!(member_is(FRAME_MEMBER, "iFrame"));
    assert!(member_is(CUSTOM_MEMBER, "custom"));
};

const fn member_is(index: usize, wgsl_name: &str) -> bool {
    let declared = SURFACE_UNIFORM_BLOCK[index].wgsl_name.as_bytes();
    let wanted = wgsl_name.as_bytes();
    if declared.len() != wanted.len() {
        return false;
    }
    let mut i = 0;
    while i < declared.len() {
        if declared[i] != wanted[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// std140 byte offset of member `index`, by the rounding rule naga applies to
/// both preludes.
const fn member_offset(index: usize) -> usize {
    let mut offset: usize = 0;
    let mut i = 0;
    while i <= index {
        let align = SURFACE_UNIFORM_BLOCK[i].ty.std140_align();
        offset = offset.div_ceil(align) * align;
        if i == index {
            return offset;
        }
        offset += SURFACE_UNIFORM_BLOCK[i].ty.std140_size();
        i += 1;
    }
    offset
}

/// Byte span of the whole block, rounded up to its own 16-byte alignment the
/// way std140 rounds a uniform block.
const fn surface_uniform_block_span() -> usize {
    let last = SURFACE_UNIFORM_BLOCK.len() - 1;
    let end = member_offset(last) + SURFACE_UNIFORM_BLOCK[last].ty.std140_size();
    end.div_ceil(16) * 16
}

/// Uniform-buffer size in bytes, read off the struct rather than restated.
/// A buffer smaller than the block is the one layout mistake wgpu rejects, and
/// it used to be the only one anything caught.
pub const SURFACE_UNIFORM_BYTES: u64 = size_of::<SurfaceUniforms>() as u64;

/// Why a user shader was refused before any GPU saw it.
///
/// [`TooManyUniforms`](Self::TooManyUniforms) is the one refusal a caller can
/// act on differently — it is about the request, not the source — and it was
/// previously distinguishable only by matching the text of a `format!`.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ShaderValidationError {
    #[error("too many uniforms: {given} given, {slots} slots available", slots = SURFACE_USER_UNIFORM_SLOTS)]
    TooManyUniforms { given: usize },
    /// naga's span-annotated diagnostic, kept as text: it is already the
    /// explanation, and re-typing it would discard the spans that make it one.
    #[error("{0}")]
    Rejected(String),
}

/// What `iChannel0` samples: another shader surface's texture, a decoded
/// image, or a (playing) video's current frame. Resolved per pass so late
/// creation / decode completion / per-frame video uploads are picked up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SurfaceChannelSource {
    Surface(u32),
    Image(neomacs_display_protocol::types::ImageId),
    Video(u32),
}

/// Initial value for one user uniform: Lisp `(name . value)` pairs arrive as
/// a name, up to four components, and the component count (1..=4) that picks
/// the accessor's WGSL type (f32/vec2/vec3/vec4).
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceUniformInit {
    pub name: String,
    pub value: [f32; 4],
    pub components: u8,
}

/// WGSL identifier for a user uniform name: non-alphanumerics become `_`,
/// prefixed `u_` (`speed` -> `u_speed`, `my-color` -> `u_my_color`).
pub fn uniform_accessor_name(name: &str) -> String {
    let mut ident = String::with_capacity(name.len() + 2);
    ident.push_str("u_");
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            ident.push(ch);
        } else {
            ident.push('_');
        }
    }
    ident
}

fn accessor_return(components: u8) -> (&'static str, &'static str) {
    match components {
        1 => ("f32", ".x"),
        2 => ("vec2<f32>", ".xy"),
        3 => ("vec3<f32>", ".xyz"),
        _ => ("vec4<f32>", ""),
    }
}

fn member_doc(member: &UniformMember, out: &mut String) {
    for line in member.doc {
        // Rust doc comments keep the space after `///`; shader comments do not
        // want it doubled.
        out.push_str("    // ");
        out.push_str(line.trim_start());
        out.push('\n');
    }
}

fn wgsl_uniform_struct() -> String {
    let mut out = String::from("struct NeoUniforms {\n");
    for member in SURFACE_UNIFORM_BLOCK {
        member_doc(member, &mut out);
        out.push_str(&format!(
            "    {}: {},\n",
            member.wgsl_name,
            member.ty.wgsl()
        ));
    }
    out.push_str("}\n");
    out
}

fn glsl_uniform_block() -> String {
    let mut out = String::from("layout(set = 0, binding = 0, std140) uniform NeoUniforms {\n");
    for member in SURFACE_UNIFORM_BLOCK {
        member_doc(member, &mut out);
        out.push_str(&format!(
            "    {} {}{};\n",
            member.ty.glsl(),
            member.glsl_name,
            member.ty.glsl_array_suffix()
        ));
    }
    out.push_str("};\n");
    out
}

/// Compose the full WGSL module: generated prelude + the user source.
///
/// `uniforms` lists `(name, components)` in slot order. WGSL module-scope
/// declarations are order-independent, so the prelude's fragment entry may
/// call `mainImage` before the user source defines it.
pub fn compose_surface_wgsl(
    user_source: &str,
    uniforms: &[(String, u8)],
    contract: SurfaceContract,
) -> String {
    match contract {
        SurfaceContract::V1 => compose_surface_wgsl_v1(user_source, uniforms),
    }
}

fn compose_surface_wgsl_v1(user_source: &str, uniforms: &[(String, u8)]) -> String {
    let mut src = String::with_capacity(user_source.len() + 1024);
    src.push_str("// ---- neomacs shader-surface prelude v1 (generated) ----\n");
    src.push_str(&wgsl_uniform_struct());
    src.push_str(
        "@group(0) @binding(0) var<uniform> u: NeoUniforms;\n\
         // Channel input (Shadertoy-style): another surface's texture bound\n\
         // via `:channel0 ID`; unbound channels sample transparent black.\n\
         @group(0) @binding(1) var iChannel0: texture_2d<f32>;\n\
         @group(0) @binding(2) var iChannel0Sampler: sampler;\n",
    );
    let custom = SURFACE_UNIFORM_BLOCK[CUSTOM_MEMBER].wgsl_name;
    for (slot, (name, components)) in uniforms.iter().enumerate().take(SURFACE_USER_UNIFORM_SLOTS) {
        let (ret, swizzle) = accessor_return(*components);
        let ident = uniform_accessor_name(name);
        src.push_str(&format!(
            "fn {ident}() -> {ret} {{ return u.{custom}[{slot}]{swizzle}; }}\n"
        ));
    }
    let resolution = SURFACE_UNIFORM_BLOCK[RESOLUTION_MEMBER].wgsl_name;
    src.push_str(&format!(
        "@vertex\n\
         fn neo_vs_main(@builtin(vertex_index) neo_vi: u32) -> @builtin(position) vec4<f32> {{\n\
         \x20   var neo_pos = array<vec2<f32>, 3>(\n\
         \x20       vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));\n\
         \x20   return vec4<f32>(neo_pos[neo_vi], 0.0, 1.0);\n\
         }}\n\
         @fragment\n\
         fn neo_fs_main(@builtin(position) neo_pos: vec4<f32>) -> @location(0) vec4<f32> {{\n\
         \x20   // Shadertoy fragCoord convention: origin bottom-left, y up.\n\
         \x20   return mainImage(vec2<f32>(neo_pos.x, u.{resolution}.y - neo_pos.y));\n\
         }}\n\
         // ---- user source ----\n"
    ));
    src.push_str(user_source);
    src.push('\n');
    src
}

/// Compose and validate a user shader with naga (parse + full validation, no
/// GPU device needed). Returns the composed module source on success — the
/// Lisp thread turns the error into a signal from `neomacs-surface-create`.
pub fn validate_surface_wgsl(
    user_source: &str,
    uniforms: &[(String, u8)],
    contract: SurfaceContract,
) -> Result<String, ShaderValidationError> {
    if uniforms.len() > SURFACE_USER_UNIFORM_SLOTS {
        return Err(ShaderValidationError::TooManyUniforms {
            given: uniforms.len(),
        });
    }
    let source = compose_surface_wgsl(user_source, uniforms, contract);
    let module = naga::front::wgsl::parse_str(&source).map_err(|err| {
        ShaderValidationError::Rejected(err.emit_to_string_with_path(&source, "surface.wgsl"))
    })?;
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    );
    validator.validate(&module).map_err(|err| {
        ShaderValidationError::Rejected(err.emit_to_string_with_path(&source, "surface.wgsl"))
    })?;
    Ok(source)
}

fn glsl_accessor_return(components: u8) -> (&'static str, &'static str) {
    match components {
        1 => ("float", ".x"),
        2 => ("vec2", ".xy"),
        3 => ("vec3", ".xyz"),
        _ => ("vec4", ""),
    }
}

/// Compose the full GLSL (Vulkan, `#version 450`) fragment shader: generated
/// prelude + the user source + a `main` footer.
///
/// The user writes the Shadertoy/Ghostty contract —
/// `void mainImage(out vec4 fragColor, in vec2 fragCoord)` — against
/// Shadertoy's names: `iTime`, `iTimeDelta`, `iMouse` (direct block members),
/// `iResolution` / `iFrame` (`#define`s over the differently typed members the
/// shared block declares), and `texture(iChannel0, uv)` (a `#define` expanding
/// to naga's Vulkan-GLSL `sampler2D(texture2D, sampler)` combined constructor
/// over the separate bindings 1 and 2 — naga registers the pairing and
/// `texture`/`textureLod`/`textureSize` all accept it).
///
/// Unlike WGSL, GLSL declarations are order-dependent, so the prelude comes
/// first and the `main` footer (the only prelude code calling `mainImage`)
/// last. Deliberately **no** `mainImage` prototype in the prelude: naga's
/// glsl-in treats a called-but-undefined prototype as an empty function and
/// validates it, which would turn a missing or mistyped `mainImage` into a
/// silently blank surface; without the prototype it is a parse error naming
/// `mainImage` at the footer call site.
pub fn compose_surface_glsl(
    user_source: &str,
    uniforms: &[(String, u8)],
    contract: SurfaceContract,
) -> String {
    match contract {
        SurfaceContract::V1 => compose_surface_glsl_v1(user_source, uniforms),
    }
}

fn compose_surface_glsl_v1(user_source: &str, uniforms: &[(String, u8)]) -> String {
    let mut src = String::with_capacity(user_source.len() + 1536);
    src.push_str(
        "#version 450\n\
         // ---- neomacs shader-surface prelude v1 (generated, Shadertoy GLSL dialect) ----\n\
         // Some members are spelled differently here than in the WGSL prelude\n\
         // and are reached through the #defines below. Only the spelling\n\
         // differs: both blocks are generated from one declaration, so the two\n\
         // dialects address the same buffer byte for byte.\n",
    );
    src.push_str(&glsl_uniform_block());
    src.push_str(
        "// Channel input (Shadertoy-style): another surface's texture bound\n\
         // via `:channel0 ID`; unbound channels sample transparent black.\n\
         layout(set = 0, binding = 1) uniform texture2D iChannel0Tex;\n\
         layout(set = 0, binding = 2) uniform sampler iChannel0Sampler;\n",
    );
    let resolution = SURFACE_UNIFORM_BLOCK[RESOLUTION_MEMBER].glsl_name;
    let frame = SURFACE_UNIFORM_BLOCK[FRAME_MEMBER].glsl_name;
    src.push_str(&format!(
        "#define iResolution ({resolution}.xyz)\n\
         #define iFrame int({frame})\n\
         #define iChannel0 sampler2D(iChannel0Tex, iChannel0Sampler)\n"
    ));
    let custom = SURFACE_UNIFORM_BLOCK[CUSTOM_MEMBER].glsl_name;
    for (slot, (name, components)) in uniforms.iter().enumerate().take(SURFACE_USER_UNIFORM_SLOTS) {
        let (ret, swizzle) = glsl_accessor_return(*components);
        let ident = uniform_accessor_name(name);
        src.push_str(&format!(
            "{ret} {ident}() {{ return {custom}[{slot}]{swizzle}; }}\n"
        ));
    }
    src.push_str(
        "layout(location = 0) out vec4 neo_frag_color;\n\
         // ---- user source ----\n",
    );
    src.push_str(user_source);
    src.push_str(&format!(
        "\n\
         // ---- footer (generated) ----\n\
         void main() {{\n\
         \x20   // Shadertoy fragCoord convention: origin bottom-left, y up.\n\
         \x20   mainImage(neo_frag_color, vec2(gl_FragCoord.x, {resolution}.y - gl_FragCoord.y));\n\
         }}\n"
    ));
    src
}

/// Compose and validate a Shadertoy-dialect GLSL user shader with naga
/// (glsl-in parse as a fragment stage + full validation, no GPU device
/// needed). Returns the composed `#version 450` source on success — the mirror
/// of [`validate_surface_wgsl`] for [`SurfaceShaderLanguage::Glsl`].
pub fn validate_surface_glsl(
    user_source: &str,
    uniforms: &[(String, u8)],
    contract: SurfaceContract,
) -> Result<String, ShaderValidationError> {
    if uniforms.len() > SURFACE_USER_UNIFORM_SLOTS {
        return Err(ShaderValidationError::TooManyUniforms {
            given: uniforms.len(),
        });
    }
    let source = compose_surface_glsl(user_source, uniforms, contract);
    let mut frontend = naga::front::glsl::Frontend::default();
    let options = naga::front::glsl::Options::from(naga::ShaderStage::Fragment);
    let module = frontend.parse(&options, &source).map_err(|err| {
        ShaderValidationError::Rejected(err.emit_to_string_with_path(&source, "surface.frag"))
    })?;
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    );
    validator.validate(&module).map_err(|err| {
        ShaderValidationError::Rejected(err.emit_to_string_with_path(&source, "surface.frag"))
    })?;
    Ok(source)
}

#[cfg(test)]
mod tests {
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
}
