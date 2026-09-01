//! SVG measurement and rasterization through one cross-platform backend.
//!
//! Natural geometry and pixels must come from the same SVG implementation.
//! Otherwise a dimensionless document can be measured with one set of layout
//! rules and painted with another.

use resvg::{tiny_skia, usvg};
use std::borrow::Cow;
use std::io::Read;
use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, OnceLock};

use crate::image_cache::constrain_raster_extent;
use neomacs_display_protocol::{
    ImageColorContext, ImageNativeExtent, ImageRealization, ImageRotation, ImageSizeSpec,
    ResolvedImageGeometry,
};

pub(crate) struct DecodedSvg {
    pub(crate) geometry: ResolvedImageGeometry,
    pub(crate) rgba: Vec<u8>,
}

/// Filesystem authority available while parsing an in-memory SVG.
///
/// The isolated variant deliberately has no ambient current-directory
/// fallback.  A base URI is an explicit capability originating in GNU's image
/// spec and is the only way relative raster references can reach the filesystem.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum SvgResourceContext {
    #[default]
    Isolated,
    BaseUri(String),
}

/// Whether an SVG is being inspected intrinsically or materialized for an
/// Emacs face.  The enum makes it impossible for the paint path to
/// accidentally use the dimension-query defaults, which resolve an unbound
/// SVG `currentColor` to black.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SvgColorMode {
    Intrinsic,
    Face(ImageColorContext),
}

struct LoadedSvg {
    tree: usvg::Tree,
    natural_width: f64,
    natural_height: f64,
}

#[derive(Debug)]
struct RootGeometry {
    width: Option<f64>,
    height: Option<f64>,
    width_value_range: Option<Range<usize>>,
    height_value_range: Option<Range<usize>>,
    view_box: Option<(f64, f64)>,
    start_tag_insert_pos: usize,
    has_root_color: bool,
}

const DEFAULT_DPI: f64 = 96.0;
pub(crate) const MAX_SVG_INPUT_SIZE: usize = 8 * 1024 * 1024;
const MAX_SVG_RASTER_BYTES: usize = 64 * 1024 * 1024;
const FALLBACK_VIEWPORT_SIZE: f32 = 0.001;

pub(crate) fn query_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    catch_unwind(AssertUnwindSafe(|| query_dimensions_inner(data)))
        .ok()
        .flatten()
}

fn query_dimensions_inner(data: &[u8]) -> Option<(u32, u32)> {
    let loaded = load(data, &SvgResourceContext::Isolated, SvgColorMode::Intrinsic)?;
    Some((
        loaded.natural_width.ceil() as u32,
        loaded.natural_height.ceil() as u32,
    ))
}

pub(crate) fn decode(
    data: &[u8],
    size: ImageSizeSpec,
    rotation: ImageRotation,
    realization: ImageRealization,
    colors: ImageColorContext,
    resources: SvgResourceContext,
) -> Option<DecodedSvg> {
    catch_unwind(AssertUnwindSafe(|| {
        decode_inner(data, size, rotation, realization, colors, &resources)
    }))
    .ok()
    .flatten()
}

fn decode_inner(
    data: &[u8],
    size: ImageSizeSpec,
    rotation: ImageRotation,
    realization: ImageRealization,
    colors: ImageColorContext,
    resources: &SvgResourceContext,
) -> Option<DecodedSvg> {
    let loaded = load(data, resources, SvgColorMode::Face(colors))?;
    // A vector document rasterizes straight to the requested size, so GNU's
    // `compute_image_size` is applied against its natural extent here rather
    // than by resampling afterwards. Pixel extents use layout×report scale so
    // `:scale default` on HiDPI recovers img->width without inverting ceil.
    let native_width = loaded.natural_width.ceil() as u32;
    let native_height = loaded.natural_height.ceil() as u32;
    let geometry = realization.resolve_geometry(
        size,
        ImageNativeExtent::new(native_width, native_height),
        ImageRotation::None,
    );
    // Match GNU `scale_image_size`: ceil so fractional device scales never
    // discard a partial SVG pixel. Constrain once more to the GPU limit.
    let geometry = geometry.with_raster(constrain_raster_extent(geometry.raster()));
    let (raster_width, raster_height) = geometry.raster().dimensions();
    let raster_bytes = (raster_width as usize)
        .checked_mul(raster_height as usize)?
        .checked_mul(4)?;
    if raster_bytes > MAX_SVG_RASTER_BYTES {
        return None;
    }

    let mut pixmap = tiny_skia::Pixmap::new(raster_width, raster_height)?;
    // Render in the document's measured coordinate space, then scale that
    // complete space into the constrained output. This keeps dimensionless
    // documents and GNU's generated outer-viewBox behavior coherent.
    let transform = tiny_skia::Transform::from_scale(
        raster_width as f32 / loaded.natural_width as f32,
        raster_height as f32 / loaded.natural_height as f32,
    );
    resvg::render(&loaded.tree, transform, &mut pixmap.as_mut());
    let rgba = pixmap.take_demultiplied();
    if rgba.len() != raster_bytes {
        return None;
    }

    // Rotate after painting, matching GNU's order (size, then turn).
    let rgba = match rotation {
        ImageRotation::None => rgba,
        turn => {
            let source = image::RgbaImage::from_raw(raster_width, raster_height, rgba)?;
            let turned = match turn {
                ImageRotation::Quarter => image::imageops::rotate90(&source),
                ImageRotation::Half => image::imageops::rotate180(&source),
                ImageRotation::ThreeQuarter => image::imageops::rotate270(&source),
                ImageRotation::None => unreachable!("handled above"),
            };
            turned.into_raw()
        }
    };
    Some(DecodedSvg {
        geometry: geometry.oriented(rotation),
        rgba,
    })
}

fn load(
    data: &[u8],
    resources: &SvgResourceContext,
    color_mode: SvgColorMode,
) -> Option<LoadedSvg> {
    let data = bounded_svg_data(data)?;
    let geometry = root_geometry(data.as_ref())?;
    if let Some((natural_width, natural_height)) = geometry.view_box_dimensions() {
        return load_with_dimensions(
            data,
            geometry,
            natural_width,
            natural_height,
            resources,
            color_mode,
        );
    }
    if let (Some(natural_width), Some(natural_height)) = (geometry.width, geometry.height)
        && valid_dimensions(natural_width, natural_height)
    {
        return load_with_dimensions(
            data,
            geometry,
            natural_width,
            natural_height,
            resources,
            color_mode,
        );
    }

    // A dimensionless SVG has no viewport in GNU/librsvg. Relative child
    // lengths therefore cannot establish its natural size. Measure absolute
    // content first, then give the original document that measured viewport
    // so percentages (typically a 100% background) paint correctly.
    let measurement_data = suppress_unresolved_percentages(data.as_ref())?;
    let options = svg_options(&geometry, resources);
    let measurement_tree = usvg::Tree::from_data(measurement_data.as_ref(), &options).ok()?;
    let (natural_width, natural_height) = fallback_dimensions(&measurement_tree)?;
    load_with_dimensions(
        data,
        geometry,
        natural_width,
        natural_height,
        resources,
        color_mode,
    )
}

fn load_with_dimensions(
    data: Cow<'_, [u8]>,
    geometry: RootGeometry,
    natural_width: f64,
    natural_height: f64,
    resources: &SvgResourceContext,
    color_mode: SvgColorMode,
) -> Option<LoadedSvg> {
    let data = inject_root_face_color(data, &geometry, color_mode);
    let (data, geometry) = normalize_root_dimensions(data, geometry, natural_width, natural_height);
    let options = svg_options(&geometry, resources);
    let tree = usvg::Tree::from_data(data.as_ref(), &options).ok()?;
    Some(LoadedSvg {
        tree,
        natural_width,
        natural_height,
    })
}

/// Establish GNU's face foreground as the root SVG `color` presentation
/// attribute. Descendants using `currentColor` inherit it, while an explicit
/// root attribute, inline style, or document stylesheet retains normal CSS
/// precedence and can override it.
fn inject_root_face_color<'a>(
    data: Cow<'a, [u8]>,
    geometry: &RootGeometry,
    color_mode: SvgColorMode,
) -> Cow<'a, [u8]> {
    let SvgColorMode::Face(colors) = color_mode else {
        return data;
    };
    if geometry.has_root_color {
        return data;
    }
    let color = format!(" color=\"#{:06x}\"", colors.foreground().rgb24());
    let mut painted = data.into_owned();
    painted.splice(
        geometry.start_tag_insert_pos..geometry.start_tag_insert_pos,
        color.bytes(),
    );
    Cow::Owned(painted)
}

fn bounded_svg_data(data: &[u8]) -> Option<Cow<'_, [u8]>> {
    if data.len() > MAX_SVG_INPUT_SIZE {
        return None;
    }
    if !data.starts_with(&[0x1f, 0x8b]) {
        return Some(Cow::Borrowed(data));
    }

    let mut decoded = Vec::new();
    flate2::read::GzDecoder::new(data)
        .take((MAX_SVG_INPUT_SIZE + 1) as u64)
        .read_to_end(&mut decoded)
        .ok()?;
    (decoded.len() <= MAX_SVG_INPUT_SIZE).then_some(Cow::Owned(decoded))
}

fn normalize_root_dimensions(
    data: Cow<'_, [u8]>,
    mut geometry: RootGeometry,
    width: f64,
    height: f64,
) -> (Cow<'_, [u8]>, RootGeometry) {
    // Normalize both root dimensions so usvg constructs the same viewport that
    // NeoMacs exposes as the image's natural layout size. In particular, usvg
    // otherwise defaults a missing root dimension to 100% of the viewBox.
    let mut replacements = Vec::with_capacity(3);
    let mut missing = String::new();
    let width_text = format!("{width}px");
    let height_text = format!("{height}px");
    if let Some(range) = geometry.width_value_range.clone() {
        replacements.push((range, width_text));
    } else {
        missing.push_str(&format!(" width=\"{width_text}\""));
    }
    if let Some(range) = geometry.height_value_range.clone() {
        replacements.push((range, height_text));
    } else {
        missing.push_str(&format!(" height=\"{height_text}\""));
    }
    if !missing.is_empty() {
        replacements.push((
            geometry.start_tag_insert_pos..geometry.start_tag_insert_pos,
            missing,
        ));
    }

    let mut normalized = data.into_owned();
    replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, replacement) in replacements {
        normalized.splice(range, replacement.bytes());
    }
    geometry.width = Some(width);
    geometry.height = Some(height);
    geometry.width_value_range = None;
    geometry.height_value_range = None;
    (Cow::Owned(normalized), geometry)
}

fn suppress_unresolved_percentages(data: &[u8]) -> Option<Cow<'_, [u8]>> {
    let text = std::str::from_utf8(data).ok()?;
    let document = usvg::roxmltree::Document::parse(text).ok()?;
    let root = document.root_element();
    let mut ranges = Vec::new();
    for node in root
        .descendants()
        .filter(|node| node.is_element() && *node != root)
    {
        for attribute in node.attributes() {
            // The temporary viewport is deliberately tiny. Remove only
            // percentages that resolve against it; objectBoundingBox values
            // and locally resolved viewports must remain intact.
            if attribute.name() == "style" {
                let range = attribute.range_value();
                collect_inline_css_percentage_ranges(
                    &text[range.clone()],
                    range.start,
                    node,
                    &mut ranges,
                );
            } else if percentage_reference(node, attribute.name())
                == Some(PercentageReference::UnresolvedViewport)
                && svgtypes::Length::from_str(attribute.value())
                    .is_ok_and(|length| length.unit == svgtypes::LengthUnit::Percent)
            {
                ranges.push(attribute.range_value());
            }
        }
        if node.is_element() && node.tag_name().name() == "style" {
            for child in node.children().filter(|child| child.is_text()) {
                let range = child.range();
                collect_stylesheet_percentage_ranges(
                    &text[range.clone()],
                    range.start,
                    root,
                    &mut ranges,
                );
            }
        }
    }
    if ranges.is_empty() {
        return Some(Cow::Borrowed(data));
    }

    ranges.sort_by_key(|range| std::cmp::Reverse(range.start));
    ranges.dedup();
    let mut measurement = data.to_vec();
    for range in ranges {
        measurement.splice(range, b"0".iter().copied());
    }
    Some(Cow::Owned(measurement))
}

fn collect_inline_css_percentage_ranges(
    css: &str,
    source_offset: usize,
    node: usvg::roxmltree::Node<'_, '_>,
    ranges: &mut Vec<Range<usize>>,
) {
    collect_css_declaration_ranges(
        css,
        source_offset,
        simplecss::DeclarationTokenizer::from(css),
        |name| percentage_reference(node, name) == Some(PercentageReference::UnresolvedViewport),
        ranges,
    );
}

fn collect_stylesheet_percentage_ranges(
    css: &str,
    source_offset: usize,
    root: usvg::roxmltree::Node<'_, '_>,
    ranges: &mut Vec<Range<usize>>,
) {
    for rule in simplecss::StyleSheet::parse(css).rules {
        let matching_nodes: Vec<_> = root
            .descendants()
            .filter(|node| node.is_element() && *node != root)
            .filter(|node| rule.selector.matches(&XmlNode(*node)))
            .collect();
        collect_css_declaration_ranges(
            css,
            source_offset,
            rule.declarations.into_iter(),
            |name| {
                let mut references = matching_nodes
                    .iter()
                    .filter_map(|node| percentage_reference(*node, name))
                    .peekable();
                references.peek().is_some()
                    && references
                        .all(|reference| reference == PercentageReference::UnresolvedViewport)
            },
            ranges,
        );
    }
}

fn collect_css_declaration_ranges<'a>(
    css: &'a str,
    source_offset: usize,
    declarations: impl Iterator<Item = simplecss::Declaration<'a>>,
    should_suppress: impl Fn(&str) -> bool,
    ranges: &mut Vec<Range<usize>>,
) {
    let css_start = css.as_ptr() as usize;
    for declaration in declarations {
        if !should_suppress(declaration.name)
            || !svgtypes::Length::from_str(declaration.value)
                .is_ok_and(|length| length.unit == svgtypes::LengthUnit::Percent)
        {
            continue;
        }
        let Some(relative_start) = (declaration.value.as_ptr() as usize).checked_sub(css_start)
        else {
            continue;
        };
        if relative_start + declaration.value.len() > css.len() {
            continue;
        }
        let start = source_offset + relative_start;
        ranges.push(start..start + declaration.value.len());
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PercentageReference {
    UnresolvedViewport,
    ResolvedViewport,
    ObjectBoundingBox,
}

#[derive(Clone, Copy)]
enum ViewportBasis {
    Horizontal,
    Vertical,
    Both,
}

#[derive(Clone, Copy)]
enum CoordinateUnits {
    UserSpaceOnUse,
    ObjectBoundingBox,
}

fn percentage_reference(
    node: usvg::roxmltree::Node<'_, '_>,
    name: &str,
) -> Option<PercentageReference> {
    use CoordinateUnits::{ObjectBoundingBox as ObjectUnits, UserSpaceOnUse};
    use PercentageReference::ObjectBoundingBox;

    let element = node.tag_name().name();
    let property = name.to_ascii_lowercase();
    let basis = percentage_basis(property.as_str())?;
    let geometry_reference = || {
        if uses_object_bounding_box_coordinates(node) {
            ObjectBoundingBox
        } else {
            viewport_reference(node, basis)
        }
    };

    match (element, property.as_str()) {
        (_, "stroke-width" | "stroke-dashoffset") => Some(geometry_reference()),
        ("rect", "x" | "y" | "width" | "height" | "rx" | "ry")
        | ("image" | "use" | "svg" | "foreignObject", "x" | "y" | "width" | "height")
        | ("circle", "cx" | "cy" | "r")
        | ("ellipse", "cx" | "cy" | "rx" | "ry")
        | ("line", "x1" | "y1" | "x2" | "y2")
        | ("text" | "tspan", "x" | "y" | "dx" | "dy") => Some(geometry_reference()),
        ("filter", "x" | "y" | "width" | "height") => {
            Some(units_reference(node, "filterUnits", ObjectUnits, basis))
        }
        ("mask", "x" | "y" | "width" | "height") => {
            Some(units_reference(node, "maskUnits", ObjectUnits, basis))
        }
        ("pattern", "x" | "y" | "width" | "height") => {
            Some(units_reference(node, "patternUnits", ObjectUnits, basis))
        }
        ("linearGradient", "x1" | "y1" | "x2" | "y2")
        | ("radialGradient", "cx" | "cy" | "r" | "fx" | "fy" | "fr") => {
            Some(units_reference(node, "gradientUnits", ObjectUnits, basis))
        }
        (element, "x" | "y" | "width" | "height") if element.starts_with("fe") => {
            let filter = node
                .ancestors()
                .find(|ancestor| ancestor.has_tag_name("filter"));
            Some(filter.map_or_else(
                || viewport_reference(node, basis),
                |filter| units_reference(filter, "primitiveUnits", UserSpaceOnUse, basis),
            ))
        }
        _ => None,
    }
}

fn percentage_basis(name: &str) -> Option<ViewportBasis> {
    match name {
        "x" | "x1" | "x2" | "width" | "cx" | "rx" | "fx" | "dx" => Some(ViewportBasis::Horizontal),
        "y" | "y1" | "y2" | "height" | "cy" | "ry" | "fy" | "dy" => Some(ViewportBasis::Vertical),
        "r" | "fr" | "stroke-width" | "stroke-dashoffset" => Some(ViewportBasis::Both),
        _ => None,
    }
}

fn viewport_reference(
    node: usvg::roxmltree::Node<'_, '_>,
    basis: ViewportBasis,
) -> PercentageReference {
    let resolved = match basis {
        ViewportBasis::Horizontal => containing_viewport_axis_is_resolved(node, "width"),
        ViewportBasis::Vertical => containing_viewport_axis_is_resolved(node, "height"),
        ViewportBasis::Both => {
            containing_viewport_axis_is_resolved(node, "width")
                && containing_viewport_axis_is_resolved(node, "height")
        }
    };
    if resolved {
        PercentageReference::ResolvedViewport
    } else {
        PercentageReference::UnresolvedViewport
    }
}

fn containing_viewport_axis_is_resolved(
    node: usvg::roxmltree::Node<'_, '_>,
    dimension: &str,
) -> bool {
    let mut ancestor = node.parent_element();
    while let Some(candidate) = ancestor {
        match candidate.tag_name().name() {
            "svg" => return svg_viewport_axis_is_resolved(candidate, dimension),
            // A symbol establishes a local viewport when instantiated by use.
            // Marker and pattern intentionally do not establish viewports.
            "symbol" => return true,
            _ => ancestor = candidate.parent_element(),
        }
    }
    false
}

fn svg_viewport_axis_is_resolved(svg: usvg::roxmltree::Node<'_, '_>, dimension: &str) -> bool {
    match svg
        .attribute(dimension)
        .and_then(|value| svgtypes::Length::from_str(value).ok())
    {
        Some(length) if length.unit == svgtypes::LengthUnit::Percent => {
            containing_viewport_axis_is_resolved(svg, dimension)
        }
        Some(length) => length.number.is_finite() && length.number > 0.0,
        None if svg.parent_element().is_some() => {
            // A nested SVG defaults to 100% of its containing viewport.
            containing_viewport_axis_is_resolved(svg, dimension)
        }
        None => false,
    }
}

fn units_reference(
    node: usvg::roxmltree::Node<'_, '_>,
    attribute: &str,
    default: CoordinateUnits,
    basis: ViewportBasis,
) -> PercentageReference {
    let units = match node.attribute(attribute) {
        Some("userSpaceOnUse") => CoordinateUnits::UserSpaceOnUse,
        Some("objectBoundingBox") => CoordinateUnits::ObjectBoundingBox,
        _ => default,
    };
    match units {
        CoordinateUnits::UserSpaceOnUse => viewport_reference(node, basis),
        CoordinateUnits::ObjectBoundingBox => PercentageReference::ObjectBoundingBox,
    }
}

fn uses_object_bounding_box_coordinates(node: usvg::roxmltree::Node<'_, '_>) -> bool {
    node.ancestors().any(|ancestor| {
        (ancestor.has_tag_name("mask")
            && ancestor.attribute("maskContentUnits") == Some("objectBoundingBox"))
            || (ancestor.has_tag_name("pattern")
                && ancestor.attribute("patternContentUnits") == Some("objectBoundingBox"))
            || (ancestor.has_tag_name("clipPath")
                && ancestor.attribute("clipPathUnits") == Some("objectBoundingBox"))
    })
}

struct XmlNode<'a, 'input: 'a>(usvg::roxmltree::Node<'a, 'input>);

impl simplecss::Element for XmlNode<'_, '_> {
    fn parent_element(&self) -> Option<Self> {
        self.0.parent_element().map(XmlNode)
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        self.0.prev_sibling_element().map(XmlNode)
    }

    fn has_local_name(&self, local_name: &str) -> bool {
        self.0.tag_name().name() == local_name
    }

    fn attribute_matches(
        &self,
        local_name: &str,
        operator: simplecss::AttributeOperator<'_>,
    ) -> bool {
        self.0
            .attribute(local_name)
            .is_some_and(|value| operator.matches(value))
    }

    fn pseudo_class_matches(&self, class: simplecss::PseudoClass<'_>) -> bool {
        matches!(class, simplecss::PseudoClass::FirstChild)
            && self.0.prev_sibling_element().is_none()
    }
}

fn root_geometry(data: &[u8]) -> Option<RootGeometry> {
    let text = std::str::from_utf8(data).ok()?;
    let document = usvg::roxmltree::Document::parse(text).ok()?;
    let root = document.root_element();
    if root.tag_name().name() != "svg" {
        return None;
    }

    let width_attribute = root.attribute_node("width");
    let height_attribute = root.attribute_node("height");
    if width_attribute.is_some_and(|attribute| !valid_root_length(attribute.value()))
        || height_attribute.is_some_and(|attribute| !valid_root_length(attribute.value()))
    {
        return None;
    }
    let width = width_attribute.and_then(|attribute| absolute_length_in_pixels(attribute.value()));
    let height =
        height_attribute.and_then(|attribute| absolute_length_in_pixels(attribute.value()));
    let view_box = root
        .attribute("viewBox")
        .and_then(|value| svgtypes::ViewBox::from_str(value).ok())
        .map(|view_box| (view_box.w, view_box.h));
    let start_tag_insert_pos = find_start_tag_insert_pos(data, root.range().start)?;

    Some(RootGeometry {
        width,
        height,
        width_value_range: width_attribute.map(|attribute| attribute.range_value()),
        height_value_range: height_attribute.map(|attribute| attribute.range_value()),
        view_box,
        start_tag_insert_pos,
        has_root_color: root.attribute("color").is_some(),
    })
}

fn find_start_tag_insert_pos(data: &[u8], start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, byte) in data.get(start..)?.iter().copied().enumerate() {
        match (quote, byte) {
            (Some(expected), actual) if actual == expected => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => {
                let close = start + offset;
                return Some(if data.get(close.wrapping_sub(1)) == Some(&b'/') {
                    close - 1
                } else {
                    close
                });
            }
            _ => {}
        }
    }
    None
}

impl RootGeometry {
    fn view_box_dimensions(&self) -> Option<(f64, f64)> {
        let (view_width, view_height) = self.view_box?;
        let dimensions = match (self.width, self.height) {
            (Some(width), Some(height)) if valid_dimensions(width, height) => (width, height),
            (Some(width), _) if width.is_finite() && width > 0.0 => {
                (width, width * view_height / view_width)
            }
            (_, Some(height)) if height.is_finite() && height > 0.0 => {
                (height * view_width / view_height, height)
            }
            _ => (view_width, view_height),
        };
        valid_dimensions(dimensions.0, dimensions.1).then_some(dimensions)
    }
}

fn svg_options(geometry: &RootGeometry, resources: &SvgResourceContext) -> usvg::Options<'static> {
    let defaults = usvg::Options::default();
    let default_size =
        if geometry.view_box.is_none() && (geometry.width.is_none() || geometry.height.is_none()) {
            usvg::Size::from_wh(FALLBACK_VIEWPORT_SIZE, FALLBACK_VIEWPORT_SIZE).unwrap()
        } else {
            defaults.default_size
        };
    usvg::Options {
        dpi: DEFAULT_DPI as f32,
        fontdb: shared_font_database(),
        image_href_resolver: restricted_image_href_resolver(resources),
        default_size,
        ..defaults
    }
}

fn shared_font_database() -> Arc<usvg::fontdb::Database> {
    static FONT_DATABASE: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    Arc::clone(FONT_DATABASE.get_or_init(|| {
        let mut database = usvg::fontdb::Database::new();
        database.load_system_fonts();
        Arc::new(database)
    }))
}

fn restricted_image_href_resolver(
    resources: &SvgResourceContext,
) -> usvg::ImageHrefResolver<'static> {
    let base_directory = match resources {
        SvgResourceContext::Isolated => None,
        SvgResourceContext::BaseUri(uri) => explicit_base_directory(uri),
    };
    usvg::ImageHrefResolver {
        resolve_data: Box::new(resolve_embedded_image),
        resolve_string: Box::new(move |href, _| {
            resolve_relative_raster(href, base_directory.as_deref())
        }),
    }
}

fn explicit_base_directory(uri: &str) -> Option<PathBuf> {
    // Network base URIs are never fetched. `file://` and ordinary local paths
    // are the filesystem forms GNU packages use here.
    let path = PathBuf::from(uri.strip_prefix("file://").unwrap_or(uri));
    let directory = if path.is_dir() || uri.ends_with(['/', '\\']) {
        path
    } else {
        path.parent()?.to_path_buf()
    };
    directory.canonicalize().ok()
}

fn resolve_relative_raster(href: &str, base_directory: Option<&Path>) -> Option<usvg::ImageKind> {
    let base_directory = base_directory?;
    let relative = Path::new(href);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    let candidate = base_directory.join(relative).canonicalize().ok()?;
    if !candidate.starts_with(base_directory) {
        return None;
    }
    let mut data = Vec::new();
    std::fs::File::open(candidate)
        .ok()?
        .take((MAX_SVG_INPUT_SIZE + 1) as u64)
        .read_to_end(&mut data)
        .ok()?;
    if data.len() > MAX_SVG_INPUT_SIZE {
        return None;
    }
    validate_embedded_raster(&data)?;
    raster_image_from_magic(Arc::new(data))
}

pub(crate) fn resolve_embedded_image(
    mime: &str,
    data: Arc<Vec<u8>>,
    options: &usvg::Options<'_>,
) -> Option<usvg::ImageKind> {
    if data.len() > MAX_SVG_INPUT_SIZE {
        return None;
    }

    let raster = match mime {
        "image/jpg" | "image/jpeg" => Some(usvg::ImageKind::JPEG(Arc::clone(&data))),
        "image/png" => Some(usvg::ImageKind::PNG(Arc::clone(&data))),
        "image/gif" => Some(usvg::ImageKind::GIF(Arc::clone(&data))),
        "image/webp" => Some(usvg::ImageKind::WEBP(Arc::clone(&data))),
        "text/plain" => raster_image_from_magic(Arc::clone(&data)),
        _ => None,
    };
    if let Some(raster) = raster {
        validate_embedded_raster(&data)?;
        return Some(raster);
    }

    let data = bounded_svg_data(&data)?;
    usvg::Tree::from_data_nested(data.as_ref(), options)
        .ok()
        .map(usvg::ImageKind::SVG)
}

fn raster_image_from_magic(data: Arc<Vec<u8>>) -> Option<usvg::ImageKind> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(usvg::ImageKind::PNG(data))
    } else if data.starts_with(&[0xff, 0xd8]) {
        Some(usvg::ImageKind::JPEG(data))
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some(usvg::ImageKind::GIF(data))
    } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        Some(usvg::ImageKind::WEBP(data))
    } else {
        None
    }
}

fn validate_embedded_raster(data: &[u8]) -> Option<()> {
    let size = imagesize::blob_size(data).ok()?;
    let bytes = size.width.checked_mul(size.height)?.checked_mul(4)?;
    (size.width <= 4096 && size.height <= 4096 && bytes <= MAX_SVG_RASTER_BYTES).then_some(())
}

fn fallback_dimensions(tree: &usvg::Tree) -> Option<(f64, f64)> {
    if tree.root().children().is_empty() {
        return None;
    }

    // GNU's fallback is the visible layer geometry in a viewport with no
    // intrinsic dimensions. Include a positive origin offset by using the
    // layer box's right/bottom edges, as the librsvg implementation did.
    let ink = tree.root().abs_layer_bounding_box();
    let width = f64::from(ink.right());
    let height = f64::from(ink.bottom());
    valid_dimensions(width, height).then_some((width, height))
}

fn absolute_length_in_pixels(value: &str) -> Option<f64> {
    let length = svgtypes::Length::from_str(value).ok()?;
    let pixels = match length.unit {
        svgtypes::LengthUnit::None | svgtypes::LengthUnit::Px => length.number,
        svgtypes::LengthUnit::In => length.number * DEFAULT_DPI,
        svgtypes::LengthUnit::Cm => length.number * DEFAULT_DPI / 2.54,
        svgtypes::LengthUnit::Mm => length.number * DEFAULT_DPI / 25.4,
        svgtypes::LengthUnit::Pt => length.number * DEFAULT_DPI / 72.0,
        svgtypes::LengthUnit::Pc => length.number * DEFAULT_DPI / 6.0,
        _ => return None,
    };
    pixels.is_finite().then_some(pixels)
}

fn valid_root_length(value: &str) -> bool {
    svgtypes::Length::from_str(value)
        .ok()
        .is_some_and(|length| length.number.is_finite() && length.number > 0.0)
}

fn valid_dimensions(width: f64, height: f64) -> bool {
    width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0
}
