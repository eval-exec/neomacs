//! Pixel-size font metric probe, porting GNU's cairo freetype driver.
//!
//! `font-info` on a font ENTITY opens the font at the entity's size — for a
//! scalable entity, `font_open_entity` (GNU src/font.c) bumps pixel size 0
//! upward until `average_width > 0 && height > 0`, which lands at 1px for
//! ordinary scalable fonts. The metrics GNU reports come from
//! `ftcrfont_open` (src/ftcrfont.c): a cairo scaled font's per-glyph
//! `x_advance` rounded via `lround` (cairo hint-metrics rounds advances to
//! integer pixels over FreeType's hinted loads) and `lround`ed cairo font
//! extents for ascent/descent.
//!
//! This module reproduces that with FreeType directly:
//! - per-glyph width = hinted `FT_Load_Char(FT_LOAD_DEFAULT)` advance,
//!   rounded from 26.6 to integer pixels (cairo hint-metrics equivalent);
//! - ascent/descent = rounded FT size metrics (what cairo's font extents
//!   report for an FT backend with hint-metrics on).
//!
//! Byte-exactness is enforced by tests against captured GNU output for
//! concrete font files; if a future font/hinting configuration diverges,
//! the tests say so instead of the probe silently guessing.
//!
//! The FreeType-backed probes live in a `cfg_select!` block: the real
//! implementations are compiled and used on `cfg(unix)` — i.e. BOTH Linux
//! and macOS — while only Windows (`cfg(not(unix))`, no system FreeType)
//! gets the `None`/empty stubs, so callers compile unchanged everywhere.
//! The GSUB/GPOS `otf_capability` reader uses ttf-parser and is fully
//! cross-platform (all three).

use neomacs_display_protocol::font::FontVariationCoord;

/// Metrics of one font file probed at an exact pixel size, shaped like the
/// `font-info` elements GNU fills in `ftcrfont_open` + `font_open_entity`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontPxMetrics {
    /// The pixel size the font actually opened at (`font_open_entity` may
    /// bump the requested size upward until the font is "manageable").
    pub pixel_size: u32,
    pub height: i32,
    pub ascent: i32,
    pub descent: i32,
    pub max_width: i32,
    pub space_width: i32,
    pub average_width: i32,
}

/// Hinted horizontal advances measured in device pixels for ASCII printable
/// glyphs of one exact opened font instance.
///
/// GNU's Cairo display backend stores these integer advances on the opened
/// font and xdisp uses them directly for ordinary characters.  Keeping the
/// coordinate domain in the type prevents a HiDPI device width from being
/// mistaken for logical layout geometry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceAsciiAdvances {
    device_pixel_size: u32,
    widths: [i32; 128],
}

impl DeviceAsciiAdvances {
    #[must_use]
    pub const fn device_pixel_size(&self) -> u32 {
        self.device_pixel_size
    }

    /// Convert one printable ASCII advance to the logical-pixel domain used
    /// by the display matrix.
    #[must_use]
    pub fn logical_advance(
        &self,
        ch: char,
        device_scale: neomacs_display_protocol::geometry::DeviceScale,
    ) -> Option<f32> {
        let index = usize::try_from(u32::from(ch)).ok()?;
        let width = *self.widths.get(index)?;
        (width > 0).then(|| width as f32 / device_scale.get())
    }
}

// Real FreeType impls on `cfg(unix)` (Linux and macOS); only Windows
// (`cfg(not(unix))`) gets the stubs.
std::cfg_select! {
    unix => {
        use freetype::Library;
        use freetype::face::LoadFlag;

        /// Probe `file`[`face_index`] like GNU `font_open_entity`: try
        /// `pixel_size`, bumping upward (at most 15 times) until average
        /// width and height are positive.
        pub fn probe_font_px_metrics(
            file: &str,
            face_index: u32,
            pixel_size: u32,
            wght: Option<f32>,
        ) -> Option<FontPxMetrics> {
            let library = Library::init().ok()?;
            let mut face = library.new_face(file, face_index as isize).ok()?;
            // Variable fonts: GNU probes the MATCHED named instance (the
            // cairo scaled font carries the fontconfig pattern's wght), not
            // the default instance — thin/bold advances differ from regular
            // at real sizes.
            if let Some(wght) = wght {
                apply_wght_axis(&library, &mut face, wght);
            }
            let start = pixel_size.max(1);
            for psize in start..=start + 15 {
                if let Some(metrics) = probe_at_exact_px(&face, psize)
                    && metrics.average_width > 0
                    && metrics.height > 0
                {
                    return Some(metrics);
                }
            }
            None
        }

        /// Open one exact Fontconfig/FreeType face at its device-pixel size
        /// and capture the per-glyph advances GNU's Cairo backend gives xdisp
        /// for ASCII printables.
        pub fn probe_device_ascii_advances(
            file: &str,
            face_selector: u32,
            device_pixel_size: u32,
            wght: Option<f32>,
        ) -> Option<DeviceAsciiAdvances> {
            let library = Library::init().ok()?;
            let mut face = library.new_face(file, face_selector as isize).ok()?;
            if let Some(wght) = wght {
                apply_wght_axis(&library, &mut face, wght);
            }
            let device_pixel_size = device_pixel_size.max(1);
            face.set_pixel_sizes(device_pixel_size, device_pixel_size).ok()?;

            let mut widths = [0; 128];
            let load_flags = LoadFlag::TARGET_LIGHT;
            for byte in 32u8..127 {
                if face.load_char(usize::from(byte), load_flags).is_err()
                    && face.load_glyph(0, load_flags).is_err()
                {
                    continue;
                }
                let advance = face.glyph().advance().x;
                widths[usize::from(byte)] = ((advance + 32) >> 6) as i32;
            }
            Some(DeviceAsciiAdvances {
                device_pixel_size,
                widths,
            })
        }

        /// The `wght` design-axis values of a variable font's fvar NAMED
        /// INSTANCES (OT axis units == CSS weight), sorted ascending and
        /// deduped. Empty for a non-variable font or one whose instances omit
        /// a `wght` axis.
        ///
        /// GNU/fontconfig expose named instances as separate font entities,
        /// so weight resolution snaps a request to the nearest instance
        /// rather than synthesizing an arbitrary axis value
        /// (font_match::resolve_requested_weight).
        pub fn named_instance_wght_values(file: &str, face_index: u32) -> Vec<u16> {
            use freetype::freetype_sys as ft;
            let Ok(library) = Library::init() else {
                return Vec::new();
            };
            let Ok(face) = library.new_face(file, face_index as isize) else {
                return Vec::new();
            };
            let mut weights = Vec::new();
            unsafe {
                let raw = face.raw() as *const ft::FT_FaceRec as *mut ft::FT_FaceRec;
                let mut mm: *mut ft::FT_MM_Var = std::ptr::null_mut();
                if ft::FT_Get_MM_Var(raw, &mut mm) != 0 || mm.is_null() {
                    return Vec::new();
                }
                let axis_count = (*mm).num_axis as usize;
                let axes = std::slice::from_raw_parts((*mm).axis, axis_count);
                let wght_tag = u32::from_be_bytes(*b"wght") as ft::FT_ULong;
                if let Some(wght_index) = axes.iter().position(|axis| axis.tag == wght_tag) {
                    let n = (*mm).num_namedstyles as usize;
                    let styles = std::slice::from_raw_parts((*mm).namedstyle, n);
                    for style in styles {
                        let coords = std::slice::from_raw_parts(style.coords, axis_count);
                        // FT_Fixed 16.16 → integer CSS weight.
                        let value = (coords[wght_index] as f64 / 65536.0).round();
                        if (1.0..=1000.0).contains(&value) {
                            weights.push(value as u16);
                        }
                    }
                }
                ft::FT_Done_MM_Var(library.raw(), mm);
            }
            weights.sort_unstable();
            weights.dedup();
            weights
        }

        /// Decode the complete axis tuple selected by a Fontconfig/FreeType
        /// encoded face selector. FreeType numbers named instances from one in
        /// bits 16-30; the raw collection face remains in bits 0-15.
        pub fn named_instance_variation_coords(
            file: &str,
            face_selector: u32,
        ) -> Vec<FontVariationCoord> {
            use freetype::freetype_sys as ft;
            let named_instance = ((face_selector >> 16) & 0x7fff) as usize;
            if named_instance == 0 {
                return Vec::new();
            }
            let Ok(library) = Library::init() else {
                return Vec::new();
            };
            let Ok(face) = library.new_face(file, (face_selector & 0xffff) as isize) else {
                return Vec::new();
            };
            unsafe {
                let raw = face.raw() as *const ft::FT_FaceRec as *mut ft::FT_FaceRec;
                let mut mm: *mut ft::FT_MM_Var = std::ptr::null_mut();
                if ft::FT_Get_MM_Var(raw, &mut mm) != 0 || mm.is_null() {
                    return Vec::new();
                }
                let axis_count = (*mm).num_axis as usize;
                let style_count = (*mm).num_namedstyles as usize;
                if named_instance > style_count {
                    ft::FT_Done_MM_Var(library.raw(), mm);
                    return Vec::new();
                }
                let axes = std::slice::from_raw_parts((*mm).axis, axis_count);
                let styles = std::slice::from_raw_parts((*mm).namedstyle, style_count);
                let coords = std::slice::from_raw_parts(
                    styles[named_instance - 1].coords,
                    axis_count,
                );
                let result = axes
                    .iter()
                    .zip(coords)
                    .filter(|&(axis, value)| *value != axis.def).filter_map(|(axis, value)| FontVariationCoord::try_new(axis.tag as u32, *value as f32 / 65536.0))
                    .collect();
                ft::FT_Done_MM_Var(library.raw(), mm);
                result
            }
        }

        /// PostScript name of the exact FreeType face or named instance.
        ///
        /// `face_selector` intentionally keeps Fontconfig's high named-instance
        /// bits; opening that selector is what makes FreeType report names such
        /// as `NotoSans-Bold` instead of the variable file's default name.
        pub fn postscript_name(file: &str, face_selector: u32) -> Option<String> {
            let library = Library::init().ok()?;
            library
                .new_face(file, face_selector as isize)
                .ok()?
                .postscript_name()
        }

        /// Set the `wght` design axis (OT axis units == CSS weight), leaving
        /// all other axes at their defaults. No-op for non-variable fonts.
        fn apply_wght_axis(library: &Library, face: &mut freetype::Face, wght: f32) {
            use freetype::freetype_sys as ft;
            unsafe {
                let raw = face.raw_mut() as *mut ft::FT_FaceRec;
                let mut mm: *mut ft::FT_MM_Var = std::ptr::null_mut();
                if ft::FT_Get_MM_Var(raw, &mut mm) != 0 || mm.is_null() {
                    return;
                }
                let axis_count = (*mm).num_axis as usize;
                let axes = std::slice::from_raw_parts((*mm).axis, axis_count);
                let mut coords: Vec<ft::FT_Fixed> = axes.iter().map(|axis| axis.def).collect();
                let wght_tag = u32::from_be_bytes(*b"wght") as ft::FT_ULong;
                for (i, axis) in axes.iter().enumerate() {
                    if axis.tag == wght_tag {
                        coords[i] = (f64::from(wght) * 65536.0) as ft::FT_Fixed;
                    }
                }
                let _ = ft::FT_Set_Var_Design_Coordinates(raw, axis_count as u32, coords.as_mut_ptr());
                ft::FT_Done_MM_Var(library.raw(), mm);
            }
        }

        fn probe_at_exact_px(face: &freetype::Face, pixel_size: u32) -> Option<FontPxMetrics> {
            face.set_pixel_sizes(pixel_size, pixel_size).ok()?;

            // ASCII printables loop (ftcrfont.c ftcrfont_open): per-glyph
            // width is the hinted advance rounded to integer pixels (cairo
            // lround of x_advance with hint-metrics on). Glyphs a char is
            // missing fall back to glyph id 0, mirroring the cairo
            // text_to_glyphs failure path.
            let mut max_width = 0i32;
            let mut space_width = 0i32;
            let mut average_width = 0i64;
            let mut n = 0i64;
            // Cairo under fontconfig's default hintstyle=hintslight loads
            // with FT_LOAD_TARGET_LIGHT: vertical-only hinting, so horizontal
            // advances stay fractional and hint-metrics rounding (lround)
            // decides the pixel width. Full bytecode hinting (LOAD_DEFAULT)
            // would widen some glyphs (Noto '@'-class) to 2px where GNU
            // reports 1.
            let load_flags = LoadFlag::TARGET_LIGHT;
            for c in 32u8..127 {
                if face.load_char(c as usize, load_flags).is_err()
                    && face.load_glyph(0, load_flags).is_err()
                {
                    continue;
                }
                // 26.6 fixed-point hinted advance → integer pixels, round
                // half up (lround semantics for the non-negative advances
                // fonts produce).
                let advance = face.glyph().advance().x;
                let this_width = ((advance + 32) >> 6) as i32;
                if this_width > 0 {
                    if this_width > max_width {
                        max_width = this_width;
                    }
                    if c == 32 {
                        space_width = this_width;
                    }
                    average_width += this_width as i64;
                    n += 1;
                }
            }
            if n > 0 {
                average_width /= n;
            }

            // Font extents (cairo_scaled_font_extents → lround): for an FT
            // backend with hint-metrics on these are the grid-fitted size
            // metrics.
            let size_metrics = face.size_metrics()?;
            let ascent = ((size_metrics.ascender + 32) >> 6) as i32;
            let descent = ((-size_metrics.descender + 32) >> 6) as i32;

            Some(FontPxMetrics {
                pixel_size,
                height: ascent + descent,
                ascent,
                descent,
                max_width,
                space_width,
                average_width: average_width as i32,
            })
        }

        // The probe tests exercise the FreeType functions against real fonts.
        #[cfg(test)]
        #[path = "probe_test.rs"]
        mod tests;
    }
    _ => {
        /// Non-unix stub: no FreeType, so no px-metric probe.
        pub fn probe_font_px_metrics(
            _file: &str,
            _face_index: u32,
            _pixel_size: u32,
            _wght: Option<f32>,
        ) -> Option<FontPxMetrics> {
            None
        }

        pub fn probe_device_ascii_advances(
            _file: &str,
            _face_selector: u32,
            _device_pixel_size: u32,
            _wght: Option<f32>,
        ) -> Option<DeviceAsciiAdvances> {
            None
        }

        /// Non-unix stub: no FreeType, so no named-instance enumeration.
        pub fn named_instance_wght_values(_file: &str, _face_index: u32) -> Vec<u16> {
            Vec::new()
        }

        pub fn named_instance_variation_coords(
            _file: &str,
            _face_selector: u32,
        ) -> Vec<FontVariationCoord> {
            Vec::new()
        }

        pub fn postscript_name(_file: &str, _face_selector: u32) -> Option<String> {
            None
        }
    }
}

/// One langsys of an OpenType script: `None` tag = the default langsys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtfLangSys {
    pub tag: Option<String>,
    pub features: Vec<String>,
}

/// One script of a GSUB/GPOS table with its langsyses (default first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtfScript {
    pub tag: String,
    pub lang_syses: Vec<OtfLangSys>,
}

/// GSUB/GPOS capability of a font file, shaped like GNU's
/// `hbfont_otf_capability` (src/hbfont.c): per table, scripts in table
/// order; per script the default langsys first (tag `None`) then named
/// langsyses in table order, langsyses without features skipped; features
/// are the langsys's feature indices mapped to tags in index order. Tags
/// keep their trailing spaces ("MKD ").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OtfCapability {
    pub gsub: Vec<OtfScript>,
    pub gpos: Vec<OtfScript>,
}

pub fn otf_capability(file: &str, face_index: u32) -> Option<OtfCapability> {
    let data = std::fs::read(file).ok()?;
    let face = ttf_parser::Face::parse(&data, face_index).ok()?;
    let tables = face.tables();
    Some(OtfCapability {
        gsub: tables.gsub.map(otf_table_scripts).unwrap_or_default(),
        gpos: tables.gpos.map(otf_table_scripts).unwrap_or_default(),
    })
}

fn otf_table_scripts(table: ttf_parser::opentype_layout::LayoutTable) -> Vec<OtfScript> {
    // GNU only reports a table with at least one feature tag.
    if table.features.is_empty() {
        return Vec::new();
    }
    let feature_tag = |index: u16| -> Option<String> {
        table
            .features
            .get(index)
            .map(|feature| tag_string(feature.tag))
    };
    let mut scripts = Vec::new();
    for script in table.scripts {
        let mut lang_syses = Vec::new();
        let mut push_langsys =
            |tag: Option<String>, lang: ttf_parser::opentype_layout::LanguageSystem| {
                let features: Vec<String> = lang
                    .feature_indices
                    .into_iter()
                    .filter_map(feature_tag)
                    .collect();
                if !features.is_empty() {
                    lang_syses.push(OtfLangSys { tag, features });
                }
            };
        if let Some(default) = script.default_language {
            push_langsys(None, default);
        }
        for lang in script.languages {
            push_langsys(Some(tag_string(lang.tag)), lang);
        }
        scripts.push(OtfScript {
            tag: tag_string(script.tag),
            lang_syses,
        });
    }
    scripts
}

fn tag_string(tag: ttf_parser::Tag) -> String {
    String::from_utf8_lossy(&tag.to_bytes()).into_owned()
}
