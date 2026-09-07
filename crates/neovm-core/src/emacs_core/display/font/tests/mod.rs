use super::*;
use crate::buffer::{Buffer, CharPos0};
use crate::emacs_core::display_host::{AvailableFontFamilyName, FontResolveRequest, FrameFontSize};
use crate::emacs_core::eval::{
    Context, DisplayHost, FontEntityMetricsRequest, FontOtfCapability, FontPxProbeResult,
    FontSpecResolveRequest, GuiFrameHostRequest, ResolvedFontEntityMetrics, ResolvedFontMatch,
    ResolvedFontSpecMatch,
};
use crate::emacs_core::value::ValueKind;
use crate::emacs_core::xfaces::*;
use crate::heap_types::LispString;
use crate::test_utils::runtime_startup_eval_all;
use crate::window::FrameId;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

fn call_face_font(args: impl FnOnce() -> Vec<Value>) -> EvalResult {
    let mut eval = Context::new();
    let args = args();
    builtin_face_font(&mut eval, args)
}

macro_rules! call_font_builtin {
    ($builtin:ident, $args:expr) => {{
        let mut eval = Context::new();
        let args = $args;
        $builtin(&mut eval, args)
    }};
}

fn ensure_selected_gui_frame(eval: &mut Context) -> FrameId {
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(eval);
    let frame = eval
        .frame_manager_mut()
        .get_mut(frame_id)
        .expect("selected frame");
    frame.set_window_system(Some(Value::symbol("neo")));
    frame_id
}

fn resolved_font_match(
    family: &str,
    file: Option<&str>,
    postscript_name: Option<&str>,
    glyph_code: Option<u32>,
    metrics: FontPxProbeResult,
    capability: Option<FontOtfCapability>,
) -> ResolvedFontMatch {
    ResolvedFontMatch {
        font: crate::emacs_core::eval::test_resolved_opened_font(
            family,
            None,
            file,
            FontWeight::NORMAL,
            FontSlant::Normal,
            FontWidth::Normal,
            postscript_name,
            metrics,
            capability,
        ),
        glyph_code,
    }
}

fn buffer_char_pos_to_byte(buffer: &Buffer, char_pos: usize) -> usize {
    buffer
        .char_pos_to_emacs_byte_pos_clamped(CharPos0::new(char_pos))
        .get()
}

#[test]
fn raw_context_does_not_prebind_x_color_aliases() {
    crate::test_utils::init_test_tracing();
    let eval = Context::new();
    for name in ["x-defined-colors", "x-color-defined-p", "x-color-values"] {
        assert!(
            eval.obarray.symbol_function(name).is_none(),
            "{name} should come from GNU faces.el, not Context::new",
        );
    }
}

#[test]
fn named_font_string_parses_fractional_point_size() {
    let request = frame_font_request_from_named_font_string("CaskaydiaCove NF-10.5")
        .expect("font name should parse");
    assert_eq!(
        request
            .face()
            .family
            .as_ref()
            .and_then(|family| family.as_utf8_str()),
        Some("CaskaydiaCove NF")
    );
    assert_eq!(
        request.size(),
        FrameFontSize::points(10.5).expect("representable point size")
    );
}

#[test]
fn named_font_string_requires_a_representable_positive_point_size() {
    let below_precision =
        frame_font_request_from_named_font_string("Example-0.01").expect("font name should parse");
    assert_eq!(
        below_precision
            .face()
            .family
            .as_ref()
            .and_then(|family| family.as_utf8_str()),
        Some("Example-0.01")
    );
    assert_eq!(below_precision.size(), FrameFontSize::Default);

    let minimum =
        frame_font_request_from_named_font_string("Example-0.1").expect("font name should parse");
    assert_eq!(
        minimum
            .face()
            .family
            .as_ref()
            .and_then(|family| family.as_utf8_str()),
        Some("Example")
    );
    assert_eq!(
        minimum.size(),
        FrameFontSize::points(0.1).expect("minimum point size")
    );
}

#[test]
fn gnu_faces_el_defines_x_color_aliases() {
    crate::test_utils::init_test_tracing();
    let source =
        fs::read_to_string(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("lisp/faces.el"))
            .expect("read faces.el");
    assert!(
        source.contains(
            "(define-obsolete-function-alias 'x-defined-colors #'defined-colors \"30.1\")"
        ),
        "GNU faces.el should own the x-defined-colors alias",
    );
    assert!(
        source.contains(
            "(define-obsolete-function-alias 'x-color-defined-p #'color-defined-p \"30.1\")"
        ),
        "GNU faces.el should own the x-color-defined-p alias",
    );
    assert!(
        source.contains("(define-obsolete-function-alias 'x-color-values #'color-values \"30.1\")"),
        "GNU faces.el should own the x-color-values alias",
    );
}

#[test]
fn context_prebinds_gnu_font_style_tables() {
    crate::test_utils::init_test_tracing();
    let eval = Context::new();

    let weight_table = eval
        .obarray
        .symbol_value("font-weight-table")
        .copied()
        .expect("font-weight-table");
    let rows = weight_table.as_vector_data().expect("weight table vector");
    assert_eq!(rows.len(), 11);
    let ultra_light = rows[1].as_vector_data().expect("weight row");
    assert_eq!(ultra_light[0], Value::fixnum(40));
    assert_eq!(ultra_light[1].as_symbol_name(), Some("ultra-light"));
    assert_eq!(ultra_light[4].as_symbol_name(), Some("extralight"));

    let slant_table = eval
        .obarray
        .symbol_value("font-slant-table")
        .copied()
        .expect("font-slant-table");
    let rows = slant_table.as_vector_data().expect("slant table vector");
    assert_eq!(rows.len(), 5);
    let normal = rows[2].as_vector_data().expect("slant row");
    assert_eq!(normal[0], Value::fixnum(100));
    assert_eq!(normal[1].as_symbol_name(), Some("normal"));
    assert_eq!(normal[3].as_symbol_name(), Some("unspecified"));

    let width_table = eval
        .obarray
        .symbol_value("font-width-table")
        .copied()
        .expect("font-width-table");
    let rows = width_table.as_vector_data().expect("width table vector");
    assert_eq!(rows.len(), 9);
    let normal = rows[4].as_vector_data().expect("width row");
    assert_eq!(normal[0], Value::fixnum(100));
    assert_eq!(normal[1].as_symbol_name(), Some("normal"));
    assert_eq!(normal[4].as_symbol_name(), Some("unspecified"));
}

#[test]
fn font_spacing_codes_match_gnu_font_spacing() {
    crate::test_utils::init_test_tracing();

    let cases = [
        (FontSpacing::Proportional, 0, "p"),
        (FontSpacing::Dual, 90, "d"),
        (FontSpacing::Mono, 100, "m"),
        (FontSpacing::Charcell, 110, "c"),
    ];

    for (spacing, code, symbol) in cases {
        assert_eq!(spacing.gnu_code(), code);
        assert_eq!(FontSpacing::from_gnu_code(i64::from(code)), Some(spacing));
        assert_eq!(FontSpacing::from_symbol_name(symbol), Some(spacing));
        assert_eq!(
            FontSpacing::from_symbol_name(&symbol.to_ascii_uppercase()),
            Some(spacing)
        );
    }

    assert_eq!(FontSpacing::from_gnu_code(1), None);
    assert_eq!(FontSpacing::from_gnu_code(111), None);
    assert_eq!(FontSpacing::from_symbol_name("proportional"), None);
    assert_eq!(FontSpacing::from_symbol_name("dual"), None);
    assert_eq!(FontSpacing::from_symbol_name("mono"), None);
    assert_eq!(FontSpacing::from_symbol_name("charcell"), None);
    assert_eq!(FontSpacing::xlfd_letter_for_gnu_code(0), Some("p"));
    assert_eq!(FontSpacing::xlfd_letter_for_gnu_code(1), Some("d"));
    assert_eq!(FontSpacing::xlfd_letter_for_gnu_code(89), Some("d"));
    assert_eq!(FontSpacing::xlfd_letter_for_gnu_code(90), Some("d"));
    assert_eq!(FontSpacing::xlfd_letter_for_gnu_code(91), Some("m"));
    assert_eq!(FontSpacing::xlfd_letter_for_gnu_code(99), Some("m"));
    assert_eq!(FontSpacing::xlfd_letter_for_gnu_code(100), Some("m"));
    assert_eq!(FontSpacing::xlfd_letter_for_gnu_code(101), Some("c"));
    assert_eq!(FontSpacing::xlfd_letter_for_gnu_code(109), Some("c"));
    assert_eq!(FontSpacing::xlfd_letter_for_gnu_code(110), Some("c"));
    assert_eq!(FontSpacing::xlfd_letter_for_gnu_code(111), None);
}

#[test]
fn font_spec_spacing_symbols_normalize_to_gnu_codes() {
    crate::test_utils::init_test_tracing();

    for (symbol, expected) in [("p", 0), ("d", 90), ("m", 100), ("c", 110)] {
        let spec = font_spec(vec![Value::keyword("spacing"), Value::symbol(symbol)])
            .expect("font-spec accepts spacing symbol");
        assert_eq!(
            font_get(vec![spec, Value::keyword("spacing")]).unwrap(),
            Value::fixnum(expected)
        );
    }

    let spec = font_spec(vec![Value::keyword("spacing"), Value::fixnum(109)])
        .expect("GNU accepts non-negative spacing fixnums through charcell");
    assert_eq!(
        font_get(vec![spec, Value::keyword("spacing")]).unwrap(),
        Value::fixnum(109)
    );

    assert!(font_spec(vec![Value::keyword("spacing"), Value::fixnum(111)]).is_err());
    assert!(font_spec(vec![Value::keyword("spacing"), Value::symbol("mono")]).is_err());
    assert!(
        font_spec(vec![
            Value::keyword("spacing"),
            Value::symbol("proportional")
        ])
        .is_err()
    );
}

#[test]
fn gnu_font_style_tables_are_constant_symbols() {
    crate::test_utils::init_test_tracing();
    let eval = Context::new();
    for name in ["font-weight-table", "font-slant-table", "font-width-table"] {
        assert!(eval.obarray.is_constant(name), "{name} should be constant");
    }
    assert!(!eval.obarray.is_constant("font-log"));
    assert_eq!(
        eval.obarray
            .symbol_value("font-log")
            .copied()
            .expect("font-log"),
        Value::T
    );
}

// -----------------------------------------------------------------------
// Font builtins
// -----------------------------------------------------------------------

#[test]
fn font_c_pure_primitives_validate_and_project_font_values() {
    crate::test_utils::init_test_tracing();
    let font_object = build_font_object(&RuntimeFace::new("default"));
    let font_spec = Value::vector(vec![Value::keyword("font-spec")]);

    assert!(
        font_face_attributes(vec![font_object, Value::NIL])
            .expect("empty font-object attributes")
            .is_nil()
    );

    let named = font_face_attributes(vec![Value::string("Monospace-10"), Value::NIL])
        .expect("named font attributes");
    let items = list_to_vec(&named).expect("attribute plist");
    assert_eq!(items.len(), 4);
    assert!(items[0].is_symbol_named(":family"));
    assert_eq!(items[1].as_utf8_str(), Some("Monospace"));
    assert!(items[2].is_symbol_named(":height"));
    assert_eq!(items[3], Value::fixnum(100));

    assert_eq!(
        font_get_glyphs(vec![font_object, Value::fixnum(0), Value::fixnum(1)])
            .expect("font-get-glyphs"),
        Value::NIL
    );
    assert_eq!(
        font_has_char_p(vec![font_spec, Value::fixnum('a' as i64)]).expect("font-has-char-p"),
        Value::NIL
    );

    let err = font_match_p(vec![Value::NIL, font_spec]).unwrap_err();
    assert!(matches!(err, Flow::Signal(sig) if sig.symbol_name() == "wrong-type-argument"));
}

#[derive(Default)]
struct FontAtDisplayHost {
    matched: Option<ResolvedFontMatch>,
    metrics: Option<FontPxProbeResult>,
    capability: Option<FontOtfCapability>,
}

impl DisplayHost for FontAtDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resolve_font_for_char(
        &mut self,
        request: FontResolveRequest,
    ) -> Result<Option<ResolvedFontMatch>, String> {
        if request.character == crate::emacs_core::emacs_char::EmacsChar::from_char('好') {
            Ok(self.matched.clone())
        } else {
            Ok(None)
        }
    }

    fn probe_font_px_metrics(
        &mut self,
        _file: &str,
        _face_index: u32,
        _pixel_size: u32,
        _wght: Option<f32>,
    ) -> Result<Option<FontPxProbeResult>, String> {
        Ok(self.metrics)
    }

    fn font_otf_capability(
        &mut self,
        _file: &str,
        _face_index: u32,
    ) -> Result<Option<FontOtfCapability>, String> {
        Ok(self.capability.clone())
    }
}

struct CapturingFontAtDisplayHost {
    last_request: Rc<RefCell<Option<FontResolveRequest>>>,
}

impl DisplayHost for CapturingFontAtDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resolve_font_for_char(
        &mut self,
        request: FontResolveRequest,
    ) -> Result<Option<ResolvedFontMatch>, String> {
        *self.last_request.borrow_mut() = Some(request);
        Ok(None)
    }
}

struct CapturingFindFontDisplayHost {
    last_request: Rc<RefCell<Option<FontSpecResolveRequest>>>,
    matched: Option<ResolvedFontSpecMatch>,
}

struct NativeFontEntityDisplayHost {
    request: Rc<RefCell<Option<FontEntityMetricsRequest>>>,
    result: ResolvedFontEntityMetrics,
}

struct FontFamilyListDisplayHost {
    requested_frame: Rc<RefCell<Option<FrameId>>>,
    families: Vec<AvailableFontFamilyName>,
}

impl DisplayHost for FontFamilyListDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn list_font_families(
        &mut self,
        frame_id: FrameId,
    ) -> Result<Vec<AvailableFontFamilyName>, String> {
        *self.requested_frame.borrow_mut() = Some(frame_id);
        Ok(self.families.clone())
    }
}

impl DisplayHost for CapturingFindFontDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resolve_font_for_spec(
        &mut self,
        request: FontSpecResolveRequest,
    ) -> Result<Option<ResolvedFontSpecMatch>, String> {
        *self.last_request.borrow_mut() = Some(request);
        Ok(self.matched.clone())
    }
}

impl DisplayHost for NativeFontEntityDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn probe_font_entity_metrics(
        &mut self,
        request: FontEntityMetricsRequest,
    ) -> Result<Option<ResolvedFontEntityMetrics>, String> {
        *self.request.borrow_mut() = Some(request);
        Ok(Some(self.result.clone()))
    }
}

fn bootstrap_eval_all(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

#[test]
fn fontp_on_non_font() {
    crate::test_utils::init_test_tracing();
    assert!(fontp(vec![Value::fixnum(42)]).unwrap().is_nil());
    assert!(fontp(vec![Value::string("hello")]).unwrap().is_nil());
}

#[test]
fn font_spec_basic() {
    crate::test_utils::init_test_tracing();
    let spec = font_spec(vec![
        Value::keyword("family"),
        Value::string("Monospace"),
        Value::keyword("size"),
        Value::fixnum(12),
    ])
    .unwrap();
    assert!(is_font_spec(&spec));
    assert!(fontp(vec![spec]).unwrap().is_truthy());
}

#[test]
fn font_info_opens_a_native_entity_without_a_file() {
    crate::test_utils::init_test_tracing();
    let request = Rc::new(RefCell::new(None));
    let mut eval = crate::emacs_core::Context::new();
    ensure_selected_gui_frame(&mut eval);
    eval.set_display_host(Box::new(NativeFontEntityDisplayHost {
        request: Rc::clone(&request),
        result: ResolvedFontEntityMetrics {
            metrics: FontPxProbeResult {
                pixel_size: 1,
                height: 2,
                ascent: 1,
                descent: 1,
                max_width: 1,
                space_width: 1,
                average_width: 1,
            },
            file: None,
            capability: None,
        },
    }));
    let entity = build_font_entity_for_spec_match(&ResolvedFontSpecMatch {
        foundry: None,
        family: LispString::from_utf8("Menlo"),
        registry: Some(LispString::from_utf8("iso10646-1")),
        file: None,
        weight: Some(FontWeight::NORMAL),
        slant: Some(FontSlant::Normal),
        width: Some(crate::face::FontWidth::Normal),
        spacing: Some(100),
        postscript_name: Some(LispString::from_utf8("Menlo-Regular")),
    });

    let info = font_info(&mut eval, vec![entity]).expect("font-info result");
    let values = info
        .as_vector_data()
        .expect("native entity must produce a font-info vector");
    assert_eq!(values[2].as_int(), Some(1));
    assert_eq!(values[3].as_int(), Some(2));
    assert!(values[12].is_nil());

    let request = request.borrow();
    let request = request.as_ref().expect("native entity probe request");
    assert_eq!(
        request.family.as_ref().and_then(LispString::as_utf8_str),
        Some("Menlo")
    );
    assert_eq!(
        request
            .postscript_name
            .as_ref()
            .and_then(LispString::as_utf8_str),
        Some("Menlo-Regular")
    );
}

#[test]
fn find_font_eval_requests_exact_registry_match_from_display_host() {
    crate::test_utils::init_test_tracing();
    let last_request = Rc::new(RefCell::new(None));
    let mut eval = crate::emacs_core::Context::new();
    ensure_selected_gui_frame(&mut eval);
    eval.set_display_host(Box::new(CapturingFindFontDisplayHost {
        last_request: Rc::clone(&last_request),
        matched: Some(ResolvedFontSpecMatch {
            foundry: None,
            family: LispString::from_utf8("Noto Sans Mono CJK SC"),
            registry: Some(LispString::from_utf8("iso10646-1")),
            file: Some(LispString::from_utf8("/tmp/NotoSansMonoCJKsc-Regular.otf")),
            weight: Some(FontWeight::NORMAL),
            slant: Some(FontSlant::Normal),
            width: Some(crate::face::FontWidth::Normal),
            spacing: None,
            postscript_name: Some(LispString::from_utf8("NotoSansMonoCJKsc-Regular")),
        }),
    }));

    let spec = font_spec(vec![
        Value::keyword("registry"),
        Value::string("gb2312.1980-0"),
        Value::keyword("weight"),
        Value::symbol("normal"),
        Value::keyword("width"),
        Value::symbol("expanded"),
    ])
    .unwrap();
    let font = find_font(&mut eval, vec![spec]).unwrap();

    assert_eq!(
        font_get(vec![font, Value::keyword("family")])
            .unwrap()
            .as_symbol_name(),
        Some("Noto Sans Mono CJK SC")
    );
    assert_eq!(
        font_get(vec![font, Value::keyword("registry")])
            .unwrap()
            .as_symbol_name(),
        Some("iso10646-1")
    );
    assert_eq!(
        font_get(vec![font, Value::keyword("file")])
            .unwrap()
            .as_utf8_str(),
        Some("/tmp/NotoSansMonoCJKsc-Regular.otf")
    );
    assert!(
        fontp(vec![font, Value::symbol("font-entity")])
            .unwrap()
            .is_truthy()
    );
    let info = font_info(&mut eval, vec![font]).unwrap();
    let values = info.as_vector_data().expect("font info vector");
    assert_eq!(
        values[12].as_utf8_str(),
        Some("/tmp/NotoSansMonoCJKsc-Regular.otf")
    );

    let request = last_request
        .borrow()
        .clone()
        .expect("display host should capture find-font request");
    assert_eq!(
        request.registry,
        Some(LispString::from_utf8("gb2312.1980-0"))
    );
    assert_eq!(request.family, None);
    assert_eq!(request.weight, Some(FontWeight::NORMAL));
    assert_eq!(request.width, Some(crate::face::FontWidth::Expanded));
}

#[test]
fn find_font_eval_returns_gnu_canonical_ultra_light_weight_symbol() {
    crate::test_utils::init_test_tracing();
    let last_request = Rc::new(RefCell::new(None));
    let mut eval = crate::emacs_core::Context::new();
    crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.set_display_host(Box::new(CapturingFindFontDisplayHost {
        last_request: Rc::clone(&last_request),
        matched: Some(ResolvedFontSpecMatch {
            foundry: None,
            family: LispString::from_utf8("JetBrains Mono"),
            registry: Some(LispString::from_utf8("iso10646-1")),
            file: None,
            weight: Some(FontWeight::ULTRA_LIGHT),
            slant: Some(FontSlant::Normal),
            width: Some(crate::face::FontWidth::Normal),
            spacing: Some(100),
            postscript_name: None,
        }),
    }));

    let spec = font_spec(vec![
        Value::keyword("family"),
        Value::string("JetBrains Mono"),
    ])
    .unwrap();
    let font = find_font(&mut eval, vec![spec]).unwrap();

    assert_eq!(
        font_get(vec![font, Value::keyword("weight")]).unwrap(),
        Value::symbol("ultra-light")
    );
}

#[test]
fn font_spec_odd_args_error() {
    crate::test_utils::init_test_tracing();
    let result = font_spec(vec![Value::keyword("family")]);
    assert!(result.is_err());
}

#[test]
fn font_spec_rejects_numeric_weight_like_gnu() {
    crate::test_utils::init_test_tracing();
    let encoded_thin = font_spec(vec![Value::keyword("weight"), Value::fixnum(0)]).unwrap();
    assert_eq!(
        font_get(vec![encoded_thin, Value::keyword("weight")])
            .unwrap()
            .as_symbol_name(),
        Some("thin")
    );

    let result = font_spec(vec![Value::keyword("weight"), Value::fixnum(700)]);
    assert!(result.is_err());
}

#[test]
fn font_spec_style_symbols_casefold_and_preserve_aliases_like_gnu() {
    crate::test_utils::init_test_tracing();

    let spec = font_spec(vec![
        Value::keyword("weight"),
        Value::symbol("BOLD"),
        Value::keyword("slant"),
        Value::symbol("ITALIC"),
        Value::keyword("width"),
        Value::symbol("EXTRA-EXPANDED"),
    ])
    .unwrap();
    assert_eq!(
        font_get(vec![spec, Value::keyword("weight")])
            .unwrap()
            .as_symbol_name(),
        Some("bold")
    );
    assert_eq!(
        font_get(vec![spec, Value::keyword("slant")])
            .unwrap()
            .as_symbol_name(),
        Some("italic")
    );
    assert_eq!(
        font_get(vec![spec, Value::keyword("width")])
            .unwrap()
            .as_symbol_name(),
        Some("extra-expanded")
    );

    let alias_spec = font_spec(vec![
        Value::keyword("weight"),
        Value::symbol("EXTRABOLD"),
        Value::keyword("slant"),
        Value::symbol("OT"),
        Value::keyword("width"),
        Value::symbol("WIDE"),
    ])
    .unwrap();
    assert_eq!(
        font_get(vec![alias_spec, Value::keyword("weight")])
            .unwrap()
            .as_symbol_name(),
        Some("extrabold")
    );
    assert_eq!(
        font_get(vec![alias_spec, Value::keyword("slant")])
            .unwrap()
            .as_symbol_name(),
        Some("ot")
    );
    assert_eq!(
        font_get(vec![alias_spec, Value::keyword("width")])
            .unwrap()
            .as_symbol_name(),
        Some("wide")
    );

    let put = font_put(vec![
        spec,
        Value::keyword("weight"),
        Value::symbol("EXTRABOLD"),
    ])
    .unwrap();
    assert_eq!(put.as_symbol_name(), Some("extrabold"));
    assert_eq!(
        font_get(vec![spec, Value::keyword("weight")])
            .unwrap()
            .as_symbol_name(),
        Some("extrabold")
    );

    assert!(font_spec(vec![Value::keyword("slant"), Value::symbol("roman")]).is_err());
}

#[test]
fn font_get_and_put() {
    crate::test_utils::init_test_tracing();
    let spec = font_spec(vec![Value::keyword("family"), Value::string("Monospace")]).unwrap();

    // Get existing property.
    let family = font_get(vec![spec, Value::keyword("family")]).unwrap();
    assert_eq!(family.as_symbol_name(), Some("Monospace"));

    // Get missing property.
    let missing = font_get(vec![spec, Value::keyword("size")]).unwrap();
    assert!(missing.is_nil());

    // Put returns VAL and mutates the original spec.
    let put_size = font_put(vec![spec, Value::keyword("size"), Value::fixnum(14)]).unwrap();
    assert_eq!(put_size.as_int(), Some(14));
    let size = font_get(vec![spec, Value::keyword("size")]).unwrap();
    assert_eq!(size.as_int(), Some(14));

    // Overwrite existing property.
    let put_family =
        font_put(vec![spec, Value::keyword("family"), Value::string("Serif")]).unwrap();
    assert_eq!(put_family.as_symbol_name(), Some("Serif"));
    let family2 = font_get(vec![spec, Value::keyword("family")]).unwrap();
    assert_eq!(family2.as_symbol_name(), Some("Serif"));
}

#[test]
fn font_get_symbol_key() {
    crate::test_utils::init_test_tracing();
    // Symbol key does not match keyword storage.
    let spec = font_spec(vec![Value::keyword("weight"), Value::symbol("bold")]).unwrap();
    let weight = font_get(vec![spec, Value::symbol("weight")]).unwrap();
    assert!(weight.is_nil());
}

#[test]
fn font_get_keyword_with_colon_matches_keyword_storage_without_colon() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let mut face = RuntimeFace::new("default");
    face.family = Some(Value::string("Hack"));
    let font = build_font_object_with_pixel_size(&face, Some(27));
    let family = font_get(vec![font, Value::keyword(":family")]).unwrap();
    let size = font_get(vec![font, Value::keyword(":size")]).unwrap();
    assert_eq!(family.as_symbol_name(), Some("Hack"));
    assert_eq!(size.as_int(), Some(27));
}

#[test]
fn font_get_non_symbol_property_errors() {
    crate::test_utils::init_test_tracing();
    let spec = font_spec(vec![Value::keyword("weight"), Value::symbol("bold")]).unwrap();
    let result = font_get(vec![spec, Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn font_get_non_vector() {
    crate::test_utils::init_test_tracing();
    // font-get on a non-font value signals wrong-type-argument.
    let result = font_get(vec![Value::fixnum(42), Value::keyword("family")]);
    assert!(result.is_err());
}

#[test]
fn list_fonts_returns_list_or_nil() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(
        list_fonts,
        vec![Value::vector(vec![Value::keyword(FONT_SPEC_TAG)])]
    );
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn list_fonts_rejects_non_font_spec() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(list_fonts, vec![Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn eval_list_fonts_accepts_live_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0 as i64;
    let result = list_fonts(
        &mut eval,
        vec![
            Value::vector(vec![Value::keyword(FONT_SPEC_TAG)]),
            Value::fixnum(frame_id),
        ],
    )
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn find_font_returns_nil_for_font_spec() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(
        find_font,
        vec![Value::vector(vec![Value::keyword(FONT_SPEC_TAG)])]
    );
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn find_font_rejects_non_font_spec() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(find_font, vec![Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn eval_find_font_accepts_live_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0 as i64;
    let result = find_font(
        &mut eval,
        vec![
            Value::vector(vec![Value::keyword(FONT_SPEC_TAG)]),
            Value::fixnum(frame_id),
        ],
    )
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn clear_font_cache_returns_nil() {
    crate::test_utils::init_test_tracing();
    assert!(clear_font_cache(vec![]).unwrap().is_nil());
}

#[test]
fn clear_font_cache_rejects_arity() {
    crate::test_utils::init_test_tracing();
    assert!(clear_font_cache(vec![Value::NIL]).is_err());
}

#[test]
fn font_family_list_batch_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(font_family_list, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn font_family_list_rejects_non_nil_frame_designator() {
    crate::test_utils::init_test_tracing();
    let result = call_font_builtin!(font_family_list, vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn eval_font_family_list_accepts_live_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0 as i64;
    let result = font_family_list(&mut eval, vec![Value::fixnum(frame_id)]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn font_family_list_returns_the_selected_frames_platform_families() {
    crate::test_utils::init_test_tracing();
    let requested_frame = Rc::new(RefCell::new(None));
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = ensure_selected_gui_frame(&mut eval);
    eval.set_display_host(Box::new(FontFamilyListDisplayHost {
        requested_frame: Rc::clone(&requested_frame),
        families: vec![
            AvailableFontFamilyName::from_utf8("Zed Sans").expect("fixture family"),
            AvailableFontFamilyName::from_utf8("Alpha Mono").expect("fixture family"),
        ],
    }));

    let result = font_family_list(&mut eval, vec![Value::fixnum(frame_id.0 as i64)])
        .expect("font-family-list");
    let families = crate::emacs_core::value::list_to_vec(&result).expect("proper family list");

    assert_eq!(*requested_frame.borrow(), Some(frame_id));
    assert_eq!(
        families
            .iter()
            .map(|family| family.as_utf8_str().expect("family string"))
            .collect::<Vec<_>>(),
        vec!["Zed Sans", "Alpha Mono"]
    );
}

#[test]
fn font_xlfd_name_returns_xlfd() {
    crate::test_utils::init_test_tracing();
    let result = font_xlfd_name(vec![Value::vector(vec![Value::keyword(FONT_SPEC_TAG)])]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("-*-*-*-*-*-*-*-*-*-*-*-*-*-*"));
}

#[test]
fn font_xlfd_name_uses_gnu_spacing_buckets() {
    crate::test_utils::init_test_tracing();

    for (spacing, letter) in [
        (0, "p"),
        (1, "d"),
        (89, "d"),
        (90, "d"),
        (91, "m"),
        (99, "m"),
        (100, "m"),
        (101, "c"),
        (109, "c"),
        (110, "c"),
    ] {
        let spec = font_spec(vec![Value::keyword("spacing"), Value::fixnum(spacing)])
            .expect("valid spacing");
        let result = font_xlfd_name(vec![spec]).expect("xlfd");
        let expected = format!("-*-*-*-*-*-*-*-*-*-*-{letter}-*-*-*");
        assert_eq!(
            result.as_utf8_str(),
            Some(expected.as_str()),
            "spacing {spacing}"
        );
    }
}

#[test]
fn font_xlfd_name_too_many_args() {
    crate::test_utils::init_test_tracing();
    let result = font_xlfd_name(vec![
        Value::vector(vec![Value::keyword(FONT_SPEC_TAG)]),
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn close_font_requires_font_object() {
    crate::test_utils::init_test_tracing();
    let wrong_nil = close_font(vec![Value::NIL]).unwrap_err();
    match wrong_nil {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("font-object"), Value::NIL]);
        }
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }

    let wrong_spec = close_font(vec![font_spec(vec![]).unwrap()]).unwrap_err();
    match wrong_spec {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data[0], Value::symbol("font-object"));
        }
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }
}

#[test]
fn close_font_accepts_opaque_font_object_and_checks_arity() {
    crate::test_utils::init_test_tracing();
    let _eval = crate::emacs_core::Context::new(); // sets up heap
    let font_obj = build_font_object(&RuntimeFace::new("default"));
    assert!(close_font(vec![font_obj]).unwrap().is_nil());
    assert!(close_font(vec![font_obj, Value::NIL]).unwrap().is_nil());

    assert!(close_font(vec![]).is_err());
    assert!(close_font(vec![Value::NIL, Value::NIL, Value::NIL]).is_err());
}

#[test]
fn font_at_eval_returns_nil_on_terminal_frame_after_position_validation() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);

    eval.buffers
        .current_buffer_mut()
        .expect("current buffer for terminal font-at test")
        .insert("abc");

    assert!(
        font_at(&mut eval, vec![Value::fixnum(1)])
            .expect("valid terminal font-at should evaluate")
            .is_nil()
    );

    let err = font_at(&mut eval, vec![Value::fixnum(4)])
        .expect_err("out-of-range terminal font-at should still validate position");
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "args-out-of-range"),
        other => panic!("expected args-out-of-range signal, got {other:?}"),
    }
}

#[test]
fn font_at_eval_reads_source_style_inline_face_keywords() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    ensure_selected_gui_frame(&mut eval);
    eval.set_display_host(Box::new(FontAtDisplayHost {
        matched: Some(resolved_font_match(
            "JetBrains Mono",
            None,
            Some("JetBrainsMono-Regular"),
            Some(17),
            FontPxProbeResult {
                pixel_size: 14,
                height: 17,
                ascent: 13,
                descent: 4,
                max_width: 9,
                space_width: 8,
                average_width: 8,
            },
            None,
        )),
        ..Default::default()
    }));

    let buffer = eval
        .buffers
        .current_buffer_mut()
        .expect("current buffer for inline face font-at test");
    buffer.insert("a好b");
    let inline_face = Value::list(vec![
        Value::symbol(":family"),
        Value::string("JetBrains Mono"),
        Value::symbol(":height"),
        Value::make_float(1.2),
        Value::symbol(":weight"),
        Value::symbol("normal"),
    ]);
    let start = buffer_char_pos_to_byte(buffer, 0);
    let end = buffer_char_pos_to_byte(buffer, 3);
    buffer.text_props_put_property_in_emacs_byte_range(
        crate::buffer::EmacsByteRange::from_usize(start, end),
        Value::symbol("face"),
        inline_face,
    );

    let font = font_at(&mut eval, vec![Value::fixnum(2)]).unwrap();
    assert!(
        fontp(vec![font, Value::symbol("font-object")])
            .unwrap()
            .is_truthy()
    );
    assert_eq!(
        font_get(vec![font, Value::keyword("family")])
            .unwrap()
            .as_symbol_name(),
        Some("JetBrains Mono")
    );
    // After the specbind refactor, font-get :height returns the raw
    // float value from the face spec instead of converting to decipoints.
    let height = font_get(vec![font, Value::keyword("height")]).unwrap();
    match height.kind() {
        ValueKind::Float => {
            let v = height.as_float().unwrap();
            assert!((v - 1.2).abs() < 1e-9, "expected 1.2, got {v}");
        }
        other => panic!("expected Float(1.2), got {other:?}"),
    }
}

#[test]
fn font_at_eval_passes_inline_face_weight_and_family_to_display_host() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    ensure_selected_gui_frame(&mut eval);

    let captured = Rc::new(RefCell::new(None));
    eval.set_display_host(Box::new(CapturingFontAtDisplayHost {
        last_request: captured.clone(),
    }));

    let buffer = eval
        .buffers
        .current_buffer_mut()
        .expect("current buffer for captured font-at test");
    buffer.insert("ab");
    let inline_face = Value::list(vec![
        Value::symbol(":family"),
        Value::string("Noto Sans Mono"),
        Value::symbol(":height"),
        Value::make_float(0.9),
        Value::symbol(":weight"),
        Value::symbol("semi-bold"),
    ]);
    let start = buffer_char_pos_to_byte(buffer, 0);
    let end = buffer_char_pos_to_byte(buffer, 2);
    buffer.text_props_put_property_in_emacs_byte_range(
        crate::buffer::EmacsByteRange::from_usize(start, end),
        Value::symbol("face"),
        inline_face,
    );

    let unresolved = font_at(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert!(
        unresolved.is_nil(),
        "a missing host realization must not fabricate an opened font"
    );

    let request = captured
        .borrow()
        .clone()
        .expect("display host should capture font-at request");
    assert_eq!(
        request.character,
        crate::emacs_core::emacs_char::EmacsChar::from_char('a')
    );
    assert_eq!(
        request
            .faces
            .ascii_face
            .family_runtime_string_owned()
            .as_deref(),
        Some("Noto Sans Mono")
    );
    assert_eq!(request.faces.ascii_face.weight, Some(FontWeight::SEMI_BOLD));
    // After the specbind refactor, float heights are treated as relative
    // instead of being converted to absolute decipoints.
    assert_eq!(
        request.faces.ascii_face.height,
        Some(crate::face::FaceHeight::Relative(0.9))
    );
}

#[test]
fn font_at_eval_keeps_inline_primary_face_separate_from_realized_fontset_base() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    ensure_selected_gui_frame(&mut eval);
    assert!(eval.set_face_attribute(
        "default",
        crate::face::LFaceAttr::Family,
        crate::face::FaceAttrValue::Text(Value::string("JetBrainsMono Nerd Font")),
    ));

    let captured = Rc::new(RefCell::new(None));
    eval.set_display_host(Box::new(CapturingFontAtDisplayHost {
        last_request: captured.clone(),
    }));

    let buffer = eval
        .buffers
        .current_buffer_mut()
        .expect("current buffer for realized fontset font-at test");
    buffer.insert("\u{f48a}");
    buffer.text_props_put_property_in_emacs_byte_range(
        crate::buffer::EmacsByteRange::from_usize(0, buffer.total_emacs_byte_len().get()),
        Value::symbol("face"),
        Value::list(vec![
            Value::keyword("family"),
            Value::string("Symbols Nerd Font Mono"),
        ]),
    );

    assert!(
        font_at(&mut eval, vec![Value::fixnum(1)])
            .expect("font-at")
            .is_nil(),
        "the capturing host intentionally returns no opened font"
    );

    let request = captured
        .borrow()
        .clone()
        .expect("display host should capture font-at request");
    assert_eq!(
        request
            .faces
            .ascii_face
            .family_runtime_string_owned()
            .as_deref(),
        Some("Symbols Nerd Font Mono"),
        "the inline family realizes the ASCII/primary face"
    );
    assert_eq!(
        request
            .faces
            .fontset_base_face
            .family_runtime_string_owned()
            .as_deref(),
        Some("JetBrainsMono Nerd Font"),
        "GNU realizes the non-ASCII fontset from the frame default face instead of reusing the inline ASCII family"
    );
}

#[test]
fn font_at_eval_prefers_backend_selected_font_match_when_available() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    ensure_selected_gui_frame(&mut eval);
    eval.set_display_host(Box::new(FontAtDisplayHost {
        matched: Some(resolved_font_match(
            "Noto Sans Mono CJK SC",
            Some("/tmp/NotoSansMonoCJKsc-Regular.otf"),
            Some("NotoSansMonoCJKsc-Regular"),
            Some(0x2A),
            FontPxProbeResult {
                pixel_size: 15,
                height: 16,
                ascent: 12,
                descent: 4,
                max_width: 9,
                space_width: 8,
                average_width: 8,
            },
            None,
        )),
        ..Default::default()
    }));

    let buffer = eval
        .buffers
        .current_buffer_mut()
        .expect("current buffer for backend font-at test");
    buffer.insert("a好b");
    let inline_face = Value::list(vec![
        Value::symbol(":family"),
        Value::string("JetBrains Mono"),
        Value::symbol(":weight"),
        Value::symbol("normal"),
    ]);
    let start = buffer_char_pos_to_byte(buffer, 1);
    let end = buffer_char_pos_to_byte(buffer, 2);
    buffer.text_props_put_property_in_emacs_byte_range(
        crate::buffer::EmacsByteRange::from_usize(start, end),
        Value::symbol("face"),
        inline_face,
    );

    let font = font_at(&mut eval, vec![Value::fixnum(2)]).unwrap();
    assert_eq!(
        font_get(vec![font, Value::keyword("family")])
            .unwrap()
            .as_symbol_name(),
        Some("Noto Sans Mono CJK SC")
    );
    assert_eq!(
        font_get(vec![font, Value::keyword("file")])
            .unwrap()
            .as_utf8_str(),
        Some("/tmp/NotoSansMonoCJKsc-Regular.otf")
    );
    let info = font_info(&mut eval, vec![font]).unwrap();
    let values = info.as_vector_data().expect("font info vector");
    assert_eq!(
        values[12].as_utf8_str(),
        Some("/tmp/NotoSansMonoCJKsc-Regular.otf")
    );
}

#[test]
fn internal_char_font_returns_font_object_and_glyph_code() {
    // GNU `internal-char-font` returns (FONT-OBJECT . GLYPH-CODE); `describe-char`
    // uses it for the "display: by this font (glyph code)" line. The glyph code
    // is the font-driver glyph index the host reports for the character.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    ensure_selected_gui_frame(&mut eval);
    eval.set_display_host(Box::new(FontAtDisplayHost {
        matched: Some(resolved_font_match(
            "Noto Sans Mono CJK SC",
            Some("/tmp/NotoSansMonoCJKsc-Regular.otf"),
            Some("NotoSansMonoCJKsc-Regular"),
            Some(0x2A),
            FontPxProbeResult {
                pixel_size: 15,
                height: 16,
                ascent: 12,
                descent: 4,
                max_width: 9,
                space_width: 8,
                average_width: 8,
            },
            None,
        )),
        ..Default::default()
    }));
    let buffer = eval
        .buffers
        .current_buffer_mut()
        .expect("current buffer for internal-char-font test");
    buffer.insert("a好b");

    let result = internal_char_font(&mut eval, vec![Value::fixnum(2)]).unwrap();
    assert_eq!(
        font_get(vec![result.cons_car(), Value::keyword("family")])
            .unwrap()
            .as_symbol_name(),
        Some("Noto Sans Mono CJK SC")
    );
    assert_eq!(result.cons_cdr(), Value::fixnum(0x2A));
}

#[test]
fn internal_char_font_uses_explicit_character_at_a_buffer_position() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    ensure_selected_gui_frame(&mut eval);
    eval.set_display_host(Box::new(FontAtDisplayHost {
        matched: Some(resolved_font_match(
            "Noto Sans Mono CJK SC",
            None,
            Some("NotoSansMonoCJKsc-Regular"),
            Some(0x2A),
            FontPxProbeResult {
                pixel_size: 15,
                height: 16,
                ascent: 12,
                descent: 4,
                max_width: 9,
                space_width: 8,
                average_width: 8,
            },
            None,
        )),
        ..Default::default()
    }));
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("abc");

    let result = internal_char_font(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(i64::from('好' as u32))],
    )
    .unwrap();
    assert!(result.cons_car().is_font_object());
    assert_eq!(result.cons_cdr(), Value::fixnum(0x2A));
}

#[test]
fn internal_char_font_returns_nil_when_current_buffer_is_not_displayed() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    ensure_selected_gui_frame(&mut eval);
    eval.set_display_host(Box::new(FontAtDisplayHost {
        matched: Some(resolved_font_match(
            "Noto Sans Mono CJK SC",
            None,
            Some("NotoSansMonoCJKsc-Regular"),
            Some(0x2A),
            FontPxProbeResult {
                pixel_size: 15,
                height: 16,
                ascent: 12,
                descent: 4,
                max_width: 9,
                space_width: 8,
                average_width: 8,
            },
            None,
        )),
        ..Default::default()
    }));
    let hidden = eval.buffers.create_buffer(" *internal-char-font-hidden*");
    eval.buffers.set_current(hidden);
    eval.buffers
        .current_buffer_mut()
        .expect("hidden current buffer")
        .insert("好");

    let result = internal_char_font(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert!(result.is_nil());

    let err = internal_char_font(
        &mut eval,
        vec![Value::fixnum(1), Value::symbol("not-a-character")],
    )
    .unwrap_err();
    assert!(matches!(
        err,
        Flow::Signal(sig)
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data == vec![Value::symbol("wholenump"), Value::symbol("not-a-character")]
    ));
}

#[test]
fn internal_char_font_returns_nil_when_the_driver_cannot_encode_the_character() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    ensure_selected_gui_frame(&mut eval);
    eval.set_display_host(Box::new(FontAtDisplayHost {
        matched: Some(resolved_font_match(
            "Noto Sans Mono CJK SC",
            None,
            Some("NotoSansMonoCJKsc-Regular"),
            None,
            FontPxProbeResult {
                pixel_size: 15,
                height: 16,
                ascent: 12,
                descent: 4,
                max_width: 9,
                space_width: 8,
                average_width: 8,
            },
            None,
        )),
        ..Default::default()
    }));

    let result = internal_char_font(
        &mut eval,
        vec![Value::NIL, Value::fixnum(i64::from('好' as u32))],
    )
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn internal_char_font_preserves_the_full_emacs_character_domain_to_the_host() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    ensure_selected_gui_frame(&mut eval);
    let captured = Rc::new(RefCell::new(None));
    eval.set_display_host(Box::new(CapturingFontAtDisplayHost {
        last_request: captured.clone(),
    }));

    let codes = [
        0x11_0000,
        crate::emacs_core::emacs_char::EmacsChar::from_byte8(0x80).code(),
    ];
    for code in codes {
        let result =
            internal_char_font(&mut eval, vec![Value::NIL, Value::fixnum(i64::from(code))])
                .unwrap();
        assert!(result.is_nil());
        assert_eq!(
            captured
                .borrow()
                .as_ref()
                .expect("font host request")
                .character,
            crate::emacs_core::emacs_char::EmacsChar::from_code(code).unwrap(),
        );
    }
}

// -----------------------------------------------------------------------
// Face builtins
// -----------------------------------------------------------------------

#[test]
fn font_info_eval_accepts_font_object_on_live_gui_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.window_system = Some(Value::symbol("neo"));
        frame.font_pixel_size = 18.0;
        frame.char_width = 9.0;
        frame.char_height = 18.0;
    }
    eval.set_display_host(Box::new(FontAtDisplayHost {
        matched: Some(resolved_font_match(
            "Test Mono",
            None,
            Some("TestMono-Regular"),
            Some(11),
            FontPxProbeResult {
                pixel_size: 18,
                height: 18,
                ascent: 14,
                descent: 4,
                max_width: 9,
                space_width: 9,
                average_width: 9,
            },
            None,
        )),
        ..Default::default()
    }));
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("好");

    let font = font_at(&mut eval, vec![Value::fixnum(1)]).unwrap();
    let info = font_info(&mut eval, vec![font]).unwrap();
    if !info.is_vector() {
        panic!("expected font info vector");
    };
    let values = info.as_vector_data().unwrap().clone();
    assert_eq!(values.len(), 14);
    assert_eq!(values[3].as_int(), Some(18));
    assert_eq!(values[10].as_int(), Some(9));
    assert_eq!(values[11].as_int(), Some(9));
}

#[test]
fn query_font_uses_stored_metrics_when_file_probe_is_unavailable() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.window_system = Some(Value::symbol("neo"));
        frame.font_pixel_size = 18.0;
        frame.char_width = 9.0;
        frame.char_height = 18.0;
    }
    eval.set_display_host(Box::new(FontAtDisplayHost {
        matched: Some(resolved_font_match(
            "Noto Sans Mono CJK SC",
            Some("/tmp/NotoSansMonoCJKsc-Regular.otf"),
            Some("NotoSansMonoCJKsc-Regular"),
            Some(0x2A),
            FontPxProbeResult {
                pixel_size: 17,
                height: 18,
                ascent: 13,
                descent: 5,
                max_width: 19,
                space_width: 11,
                average_width: 12,
            },
            Some(FontOtfCapability {
                gsub: vec![("latn".to_owned(), vec![(None, vec!["liga".to_owned()])])],
                gpos: Vec::new(),
            }),
        )),
        // The file exists in the object identity, but a later host probe is
        // unavailable. `query-font` must read the opened object, not retry it.
        metrics: None,
        capability: None,
    }));
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("a好b");

    let font = font_at(&mut eval, vec![Value::fixnum(2)]).unwrap();
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("selected frame")
        .window_system = None;
    let query = query_font(&mut eval, vec![font]).unwrap();
    let values = query.as_vector_data().expect("query-font vector");

    assert_eq!(values.len(), 9);
    assert!(
        values[0]
            .as_utf8_str()
            .is_some_and(|name| name.contains("-17-"))
    );
    assert_eq!(
        values[1].as_utf8_str(),
        Some("/tmp/NotoSansMonoCJKsc-Regular.otf")
    );
    for (index, expected) in [(2, 17), (3, 19), (4, 13), (5, 5), (6, 11), (7, 12)] {
        assert_eq!(values[index].as_int(), Some(expected), "slot {index}");
    }
    assert_eq!(values[8].cons_car().as_symbol_name(), Some("opentype"));
    assert!(values[8].cons_cdr().cons_car().is_cons());

    let err = query_font(&mut eval, vec![Value::NIL]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.data, vec![Value::symbol("font-object"), Value::NIL]),
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }

    let fabricated = Value::vector(vec![Value::keyword(FONT_OBJECT_TAG)]);
    let err = query_font(&mut eval, vec![fabricated]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.data, vec![Value::symbol("font-object"), fabricated]),
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }
}

#[test]
fn query_font_uses_opened_object_metrics_without_a_font_file() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = ensure_selected_gui_frame(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.font_pixel_size = 31.0;
        frame.char_width = 16.0;
        frame.char_height = 34.0;
    }
    eval.set_display_host(Box::new(FontAtDisplayHost {
        matched: Some(resolved_font_match(
            "Memory Sans",
            None,
            Some("MemorySans-Regular"),
            Some(7),
            FontPxProbeResult {
                pixel_size: 17,
                height: 21,
                ascent: 15,
                descent: 6,
                max_width: 19,
                space_width: 8,
                average_width: 10,
            },
            None,
        )),
        metrics: Some(FontPxProbeResult {
            pixel_size: 17,
            height: 21,
            ascent: 15,
            descent: 6,
            max_width: 19,
            space_width: 8,
            average_width: 10,
        }),
        capability: None,
    }));
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("好");

    let font = font_at(&mut eval, vec![Value::fixnum(1)]).unwrap();
    let query = query_font(&mut eval, vec![font]).unwrap();
    let values = query.as_vector_data().expect("query-font vector");

    assert!(values[1].is_nil(), "a memory font may have no filename");
    for (index, expected) in [(2, 17), (3, 19), (4, 15), (5, 6), (6, 8), (7, 10)] {
        assert_eq!(values[index].as_int(), Some(expected), "slot {index}");
    }
}

#[test]
fn opened_font_retains_exact_backend_identity_and_variations() {
    use neomacs_display_protocol::font::{
        FontFileAsset, FontOutlineAsset, FontReplay, FontSlantKind, FontVariationCoord,
        ResolvedFont, ResolvedFontId, ResolvedFontIdentity,
    };

    crate::test_utils::init_test_tracing();
    let identity = ResolvedFontIdentity::from_file_with_variations(
        "./tmp/VariableSans.ttc",
        3,
        Some("VariableSans-Semibold".to_string()),
        vec![FontVariationCoord::try_new(u32::from_be_bytes(*b"wght"), 620.0).unwrap()],
    );
    let metrics = FontPxProbeResult {
        pixel_size: 19,
        height: 24,
        ascent: 18,
        descent: 6,
        max_width: 14,
        space_width: 9,
        average_width: 11,
    };
    let matched = ResolvedFontMatch {
        font: crate::emacs_core::eval::ResolvedOpenedFont {
            resolved: ResolvedFont {
                id: ResolvedFontId(41),
                identity: identity.clone(),
                replay: FontReplay::Swash {
                    asset: FontOutlineAsset::File(
                        FontFileAsset::new("./tmp/VariableSans.ttc", 3).expect("fixture path"),
                    ),
                },
                family: "Variable Sans".to_string(),
                full_name: Some("Variable Sans Semibold".to_string()),
                postscript_name: Some("VariableSans-Semibold".to_string()),
                weight: 620,
                slant: FontSlantKind::Normal,
                width: 5,
                pixel_size: 19.0,
                ascent_px: 18.0,
                descent_px: 6.0,
                space_advance_px: 9.0,
                glyph_advance: Default::default(),
            },
            foundry: Some(LispString::from_utf8("TEST")),
            slant: FontSlant::Normal,
            metrics,
            capability: None,
        },
        glyph_code: Some(17),
    };

    let object = opened_font_from_resolved_match(&RuntimeFace::new("default"), &matched);
    assert_eq!(
        object.as_font_data().expect("native font payload").identity,
        identity
    );
    assert_eq!(
        font_get(vec![object, Value::keyword("full-name")])
            .unwrap()
            .as_utf8_str(),
        Some("Variable Sans Semibold")
    );
    assert_eq!(
        font_get(vec![object, Value::keyword("postscript-name")])
            .unwrap()
            .as_utf8_str(),
        Some("VariableSans-Semibold")
    );
}

#[test]
fn font_info_eval_reports_font_vector_file_slot_on_live_gui_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.window_system = Some(Value::symbol("neo"));
        frame.font_pixel_size = 18.0;
        frame.char_width = 9.0;
        frame.char_height = 18.0;
    }

    let mut face = RuntimeFace::new("default");
    face.family = Some(Value::symbol("Noto Sans"));
    let font = finish_opened_font(
        font_object_property_fields(&face, Some(18)),
        Some(&LispString::from_utf8("/tmp/NotoSans.ttf")),
        None,
        OpenedFontMetrics::fallback(18),
        Value::NIL,
        neomacs_display_protocol::font::ResolvedFontIdentity::from_file(
            "/tmp/NotoSans.ttf",
            0,
            None,
        ),
    );
    let info = font_info(&mut eval, vec![font]).unwrap();
    let values = info.as_vector_data().expect("font info vector");

    assert_eq!(values.len(), 14);
    assert_eq!(values[12].as_utf8_str(), Some("/tmp/NotoSans.ttf"));
}

#[test]
fn font_shape_gstring_rejects_invalid_shape_and_accepts_valid_opened_font() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let invalid = Value::vector(vec![Value::fixnum(0)]);
    let err = font_shape_gstring(&mut eval, vec![invalid, Value::NIL]).unwrap_err();
    assert!(matches!(err, Flow::Signal(sig) if sig.symbol_name() == "error"));

    let font = build_font_object(&RuntimeFace::new("default"));
    let gstring = crate::emacs_core::composite::composition_get_gstring(
        &mut eval,
        vec![
            Value::fixnum(0),
            Value::fixnum(2),
            font,
            Value::string("ab"),
        ],
    )
    .expect("composition-get-gstring should build a GNU-shaped gstring");
    assert!(crate::emacs_core::composite::composition_gstring_p(
        &eval, gstring
    ));
    assert!(
        font_shape_gstring(&mut eval, vec![gstring, Value::NIL])
            .expect("valid uncached glyph string")
            .is_nil()
    );
}

#[test]
fn set_face_attribute_accepts_only_gnu_underline_style_symbols() {
    crate::test_utils::init_test_tracing();
    let rendered = bootstrap_eval_all(
        r#"(list
             (condition-case nil
                 (progn
                   (set-face-attribute 'default nil :underline '(:style dots))
                   'ok)
               (error 'error))
             (condition-case nil
                 (progn
                   (set-face-attribute 'default nil :underline '(:style dash))
                   'ok)
               (error 'error)))"#,
    );

    assert_eq!(rendered, vec!["OK (ok error)".to_string()]);
}

#[test]
fn set_face_attribute_accepts_only_gnu_box_style_symbols() {
    crate::test_utils::init_test_tracing();
    let rendered = bootstrap_eval_all(
        r#"(list
             (condition-case nil
                 (progn
                   (set-face-attribute 'default nil :box '(:style flat-button))
                   'ok)
               (error 'error))
             (condition-case nil
                 (progn
                   (set-face-attribute 'default nil :box '(:style flat))
                   'ok)
               (error 'error))
             (condition-case nil
                 (progn
                   (set-face-attribute 'default nil :box 0)
                   'ok)
               (error 'error)))"#,
    );

    assert_eq!(rendered, vec!["OK (ok error error)".to_string()]);
}

#[test]
fn face_font_returns_nil_for_known_faces() {
    crate::test_utils::init_test_tracing();
    let result = call_face_font(|| vec![Value::symbol("default")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn face_font_accepts_known_string_face() {
    crate::test_utils::init_test_tracing();
    let result = call_face_font(|| vec![Value::string("default")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn face_font_ignores_optional_arguments_for_known_face() {
    crate::test_utils::init_test_tracing();
    let result = call_face_font(|| vec![Value::symbol("default"), Value::NIL, Value::T]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn face_font_rejects_invalid_face() {
    crate::test_utils::init_test_tracing();
    let result = call_face_font(|| vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn tty_default_face_color_realization_normalizes_font_attributes_like_gnu() {
    crate::test_utils::init_test_tracing();
    let rendered = bootstrap_eval_all(
        r##"(progn
             (face-spec-reset-face 'default (selected-frame))
             (set-face-attribute 'default (selected-frame)
                                 :foreground "#ffffff"
                                 :background nil
                                 :family "Terminus"
                                 :foundry "xos4"
                                 :slant 'normal
                                 :weight 'normal
                                 :height 130
                                 :width 'normal)
             (list (face-attribute 'default :family nil 'default)
                   (face-attribute 'default :foundry nil 'default)
                   (face-attribute 'default :height nil 'default)))"##,
    );

    // GNU xfaces.c routes a default-face color change through the frame
    // parameter path.  `update_face_from_frame_parameter' then calls
    // `realize_basic_faces', whose TTY branch canonicalizes the live frame's
    // family/foundry/width/height before the remaining attributes are applied.
    assert_eq!(
        rendered,
        vec![r#"OK ("default" "default" 130)"#.to_string()]
    );
}

#[test]
fn bootstrap_set_face_attribute_updates_live_mode_line_face() {
    crate::test_utils::init_test_tracing();
    let rendered = bootstrap_eval_all(
        r#"(list
             (assq :background face-x-resources)
             (progn
               (set-face-attribute 'mode-line (selected-frame)
                                   :background "grey75"
                                   :foreground "black")
               (face-background 'mode-line nil t))
             (let* ((table (frame--face-hash-table (selected-frame)))
                    (face (gethash 'mode-line table)))
               (list (aref face 9) (aref face 10))))"#,
    );

    assert_eq!(
        rendered,
        vec!["OK ((:background (\".attributeBackground\" . \"Face.AttributeBackground\")) \"grey75\" (\"black\" \"grey75\"))".to_string()]
    );
}

#[test]
fn bootstrap_frame_face_hash_table_is_frame_owned_object() {
    crate::test_utils::init_test_tracing();
    let rendered = bootstrap_eval_all(
        r#"(let ((a (frame--face-hash-table (selected-frame)))
                 (b (frame--face-hash-table (selected-frame))))
             (eq a b))"#,
    );

    assert_eq!(rendered, vec!["OK t".to_string()]);
}

// -----------------------------------------------------------------------
// Arity checks
// -----------------------------------------------------------------------

#[test]
fn fontp_too_many_args() {
    crate::test_utils::init_test_tracing();
    let result = fontp(vec![Value::NIL, Value::NIL, Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn fontp_no_args() {
    crate::test_utils::init_test_tracing();
    let result = fontp(vec![]);
    assert!(result.is_err());
}

#[test]
fn font_get_wrong_arity() {
    crate::test_utils::init_test_tracing();
    assert!(font_get(vec![Value::NIL]).is_err());
    assert!(font_get(vec![Value::NIL, Value::NIL, Value::NIL]).is_err());
}

#[test]
fn font_put_wrong_arity() {
    crate::test_utils::init_test_tracing();
    assert!(font_put(vec![Value::NIL, Value::NIL]).is_err());
}
