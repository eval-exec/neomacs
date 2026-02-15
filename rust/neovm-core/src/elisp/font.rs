//! Font and face builtins for the Elisp interpreter.
//!
//! Font builtins:
//! - `fontp`, `font-spec`, `font-get`, `font-put`, `list-fonts`, `find-font`,
//!   `clear-font-cache`, `font-family-list`, `font-xlfd-name`
//!
//! Face builtins:
//! - `internal-lisp-face-p`, `internal-copy-lisp-face`,
//!   `internal-set-lisp-face-attribute`, `internal-get-lisp-face-attribute`,
//!   `internal-merge-in-global-face`, `face-attribute-relative-p`,
//!   `merge-face-attribute`, `face-list`, `color-defined-p`, `color-values`,
//!   `defined-colors`, `face-id`, `face-font`, `internal-face-x-get-resource`,
//!   `internal-set-font-selection-order`,
//!   `internal-set-alternative-font-family-alist`,
//!   `internal-set-alternative-font-registry-alist`

use super::error::{signal, EvalResult, Flow};
use super::value::*;

// ---------------------------------------------------------------------------
// Argument helpers (local to this module)
// ---------------------------------------------------------------------------

fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_min_args(name: &str, args: &[Value], min: usize) -> Result<(), Flow> {
    if args.len() < min {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_max_args(name: &str, args: &[Value], max: usize) -> Result<(), Flow> {
    if args.len() > max {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Font-spec helpers
// ---------------------------------------------------------------------------

/// The tag keyword used to identify font-spec vectors: `:font-spec`.
const FONT_SPEC_TAG: &str = "font-spec";

/// Check whether a Value is a font-spec (a vector whose first element is
/// the keyword `:font-spec`).
fn is_font_spec(val: &Value) -> bool {
    match val {
        Value::Vector(v) => {
            let elems = v.lock().expect("poisoned");
            if elems.is_empty() {
                return false;
            }
            matches!(&elems[0], Value::Keyword(k) if k == FONT_SPEC_TAG)
        }
        _ => false,
    }
}

/// Extract a property from a font-spec vector.  The vector layout is:
/// `[:font-spec :family "Mono" :weight normal :slant italic :size 12 ...]`
/// Property keys are keywords; values follow immediately after each key.
fn font_spec_get(vec_elems: &[Value], prop: &Value) -> Value {
    // Skip the tag at index 0; scan remaining pairs.
    let mut i = 1;
    while i + 1 < vec_elems.len() {
        if keyword_eq(&vec_elems[i], prop) {
            return vec_elems[i + 1].clone();
        }
        i += 2;
    }
    Value::Nil
}

/// Set (or add) a property in a font-spec, returning a new vector.
fn font_spec_put(vec_elems: &[Value], prop: &Value, val: &Value) -> Vec<Value> {
    let mut result: Vec<Value> = Vec::with_capacity(vec_elems.len() + 2);
    // Copy the tag.
    if !vec_elems.is_empty() {
        result.push(vec_elems[0].clone());
    }
    let mut found = false;
    let mut i = 1;
    while i + 1 < vec_elems.len() {
        if keyword_eq(&vec_elems[i], prop) {
            result.push(vec_elems[i].clone());
            result.push(val.clone());
            found = true;
        } else {
            result.push(vec_elems[i].clone());
            result.push(vec_elems[i + 1].clone());
        }
        i += 2;
    }
    // Handle a trailing key without a value (shouldn't happen, but be safe).
    if i < vec_elems.len() {
        result.push(vec_elems[i].clone());
    }
    if !found {
        result.push(prop.clone());
        result.push(val.clone());
    }
    result
}

/// Compare two Values as keyword keys.  Accepts both `:family` (Keyword) and
/// `family` (Symbol) to be flexible.
fn keyword_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Keyword(ka), Value::Keyword(kb)) => ka == kb,
        (Value::Keyword(ka), Value::Symbol(sb)) => ka == sb,
        (Value::Symbol(sa), Value::Keyword(kb)) => sa == kb,
        (Value::Symbol(sa), Value::Symbol(sb)) => sa == sb,
        _ => false,
    }
}

/// Normalize a property name to a Keyword value.  If the user passes a symbol
/// like `family`, convert to `:family` keyword for consistent storage.
fn normalize_prop_key(val: &Value) -> Value {
    match val {
        Value::Keyword(_) => val.clone(),
        Value::Symbol(s) => Value::Keyword(s.clone()),
        _ => val.clone(),
    }
}

// ===========================================================================
// Font builtins (pure)
// ===========================================================================

/// `(fontp OBJECT &optional EXTRA-TYPE)` -- return t if OBJECT is a font-spec,
/// font-entity, or font-object.  We represent all of these as tagged vectors
/// with `:font-spec` keyword at position 0.
pub(crate) fn builtin_fontp(args: Vec<Value>) -> EvalResult {
    expect_max_args("fontp", &args, 2)?;
    expect_min_args("fontp", &args, 1)?;
    // Ignore EXTRA-TYPE for now; just check the tag.
    Ok(Value::bool(is_font_spec(&args[0])))
}

/// `(font-spec &rest ARGS)` -- create a font spec from keyword args.
///
/// Usage: `(font-spec :family "Monospace" :weight 'normal :size 12)`
///
/// Returns a vector `[:font-spec :family "Monospace" :weight normal :size 12]`.
pub(crate) fn builtin_font_spec(args: Vec<Value>) -> EvalResult {
    // Args should come in keyword-value pairs.
    if args.len() % 2 != 0 {
        return Err(signal(
            "error",
            vec![Value::string("font-spec requires keyword-value pairs")],
        ));
    }
    let mut elems: Vec<Value> = Vec::with_capacity(1 + args.len());
    elems.push(Value::Keyword(FONT_SPEC_TAG.to_string()));
    for pair in args.chunks(2) {
        let key = normalize_prop_key(&pair[0]);
        elems.push(key);
        elems.push(pair[1].clone());
    }
    Ok(Value::vector(elems))
}

/// `(font-get FONT PROP)` -- get a property value from a font-spec.
pub(crate) fn builtin_font_get(args: Vec<Value>) -> EvalResult {
    expect_args("font-get", &args, 2)?;
    let prop = normalize_prop_key(&args[1]);
    match &args[0] {
        Value::Vector(v) => {
            let elems = v.lock().expect("poisoned");
            Ok(font_spec_get(&elems, &prop))
        }
        // If not a vector, return nil (Emacs tolerates this for non-font objects).
        _ => Ok(Value::Nil),
    }
}

/// `(font-put FONT PROP VAL)` -- set a property in a font-spec, returning a
/// modified copy.
///
/// NOTE: Real Emacs mutates in-place; we return a new vector for safety in
/// our GC-free representation.  Callers should use the returned value.
pub(crate) fn builtin_font_put(args: Vec<Value>) -> EvalResult {
    expect_args("font-put", &args, 3)?;
    let prop = normalize_prop_key(&args[1]);
    match &args[0] {
        Value::Vector(v) => {
            let elems = v.lock().expect("poisoned");
            let new_elems = font_spec_put(&elems, &prop, &args[2]);
            Ok(Value::vector(new_elems))
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("fontp"), other.clone()],
        )),
    }
}

/// `(list-fonts FONT-SPEC &optional FRAME MAXNUM PREFER)` -- batch stub.
pub(crate) fn builtin_list_fonts(args: Vec<Value>) -> EvalResult {
    expect_min_args("list-fonts", &args, 1)?;
    expect_max_args("list-fonts", &args, 4)?;
    if !is_font_spec(&args[0]) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("font-spec"), args[0].clone()],
        ));
    }
    Ok(Value::Nil)
}

/// `(find-font FONT-SPEC &optional FRAME)` -- batch stub.
pub(crate) fn builtin_find_font(args: Vec<Value>) -> EvalResult {
    expect_min_args("find-font", &args, 1)?;
    expect_max_args("find-font", &args, 2)?;
    if !is_font_spec(&args[0]) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("font-spec"), args[0].clone()],
        ));
    }
    Ok(Value::Nil)
}

/// `(clear-font-cache)` -- stub, return nil.
pub(crate) fn builtin_clear_font_cache(args: Vec<Value>) -> EvalResult {
    expect_max_args("clear-font-cache", &args, 0)?;
    Ok(Value::Nil)
}

/// `(font-family-list &optional FRAME)` -- batch stub.
pub(crate) fn builtin_font_family_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("font-family-list", &args, 1)?;
    if let Some(frame) = args.first() {
        if !frame.is_nil() {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("frame-live-p"), frame.clone()],
            ));
        }
    }
    Ok(Value::Nil)
}

/// `(font-xlfd-name FONT &optional FOLD-WILDCARDS)` -- stub, return "*".
pub(crate) fn builtin_font_xlfd_name(args: Vec<Value>) -> EvalResult {
    expect_min_args("font-xlfd-name", &args, 1)?;
    expect_max_args("font-xlfd-name", &args, 2)?;
    Ok(Value::string("*"))
}

// ===========================================================================
// Face builtins (pure)
// ===========================================================================

/// Well-known face names returned by `face-list` and recognised by
/// `internal-lisp-face-p`.
const KNOWN_FACES: &[&str] = &[
    "default",
    "bold",
    "italic",
    "underline",
    "highlight",
    "region",
    "mode-line",
    "mode-line-inactive",
    "fringe",
    "cursor",
];

/// `(internal-lisp-face-p FACE &optional FRAME)` -- return t if FACE is a
/// symbol naming a known face.  For maximum compatibility we accept any
/// symbol as a valid face.
pub(crate) fn builtin_internal_lisp_face_p(args: Vec<Value>) -> EvalResult {
    expect_min_args("internal-lisp-face-p", &args, 1)?;
    expect_max_args("internal-lisp-face-p", &args, 2)?;
    // Accept any symbol as a face name (Emacs packages define many faces).
    Ok(Value::bool(args[0].is_symbol()))
}

/// `(internal-copy-lisp-face FROM TO FRAME NEW-FRAME)` -- stub, return TO.
pub(crate) fn builtin_internal_copy_lisp_face(args: Vec<Value>) -> EvalResult {
    expect_args("internal-copy-lisp-face", &args, 4)?;
    Ok(args[1].clone())
}

/// `(internal-set-lisp-face-attribute FACE ATTR VALUE &optional FRAME)` --
/// stub, return VALUE.
pub(crate) fn builtin_internal_set_lisp_face_attribute(args: Vec<Value>) -> EvalResult {
    expect_min_args("internal-set-lisp-face-attribute", &args, 3)?;
    expect_max_args("internal-set-lisp-face-attribute", &args, 4)?;
    Ok(args[2].clone())
}

/// `(internal-get-lisp-face-attribute FACE ATTR &optional FRAME)` -- stub,
/// return nil.
pub(crate) fn builtin_internal_get_lisp_face_attribute(args: Vec<Value>) -> EvalResult {
    expect_min_args("internal-get-lisp-face-attribute", &args, 2)?;
    expect_max_args("internal-get-lisp-face-attribute", &args, 3)?;
    Ok(Value::Nil)
}

/// `(internal-merge-in-global-face FACE FRAME)` -- stub, return nil.
pub(crate) fn builtin_internal_merge_in_global_face(args: Vec<Value>) -> EvalResult {
    expect_args("internal-merge-in-global-face", &args, 2)?;
    Ok(Value::Nil)
}

/// `(face-attribute-relative-p ATTRIBUTE VALUE)` -- return t if VALUE is the
/// symbol `unspecified` or the symbol `relative`.
pub(crate) fn builtin_face_attribute_relative_p(args: Vec<Value>) -> EvalResult {
    expect_args("face-attribute-relative-p", &args, 2)?;
    let is_relative = match &args[1] {
        Value::Symbol(s) => s == "unspecified" || s == "relative",
        _ => false,
    };
    Ok(Value::bool(is_relative))
}

/// `(merge-face-attribute ATTRIBUTE VALUE1 VALUE2)` -- return VALUE1 unless it
/// is the symbol `unspecified`, in which case return VALUE2.
pub(crate) fn builtin_merge_face_attribute(args: Vec<Value>) -> EvalResult {
    expect_args("merge-face-attribute", &args, 3)?;
    let v1_unspecified = match &args[1] {
        Value::Symbol(s) => s == "unspecified",
        _ => false,
    };
    if v1_unspecified {
        Ok(args[2].clone())
    } else {
        Ok(args[1].clone())
    }
}

/// `(face-list &optional FRAME)` -- return list of known face names.
pub(crate) fn builtin_face_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("face-list", &args, 1)?;
    Ok(Value::list(
        KNOWN_FACES.iter().map(|s| Value::symbol(*s)).collect(),
    ))
}

/// `(color-defined-p COLOR &optional FRAME)` -- stub, return t (assume all
/// color names are supported).
pub(crate) fn builtin_color_defined_p(args: Vec<Value>) -> EvalResult {
    expect_min_args("color-defined-p", &args, 1)?;
    expect_max_args("color-defined-p", &args, 2)?;
    Ok(Value::True)
}

/// `(color-values COLOR &optional FRAME)` -- parse common color names and hex
/// colors to a list `(R G B)` with 16-bit component values (0..65535).
pub(crate) fn builtin_color_values(args: Vec<Value>) -> EvalResult {
    expect_min_args("color-values", &args, 1)?;
    expect_max_args("color-values", &args, 2)?;
    let color_name = match &args[0] {
        Value::Str(s) => (**s).clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), other.clone()],
            ));
        }
    };
    let lower = color_name.trim().to_lowercase();

    // Try hex format first: #RGB, #RRGGBB, #RRRRGGGGBBBB
    if lower.starts_with('#') {
        let hex = &lower[1..];
        return match hex.len() {
            3 => {
                // #RGB -> expand each digit to 16-bit
                let r = u16_from_hex_digit(hex.as_bytes()[0]);
                let g = u16_from_hex_digit(hex.as_bytes()[1]);
                let b = u16_from_hex_digit(hex.as_bytes()[2]);
                Ok(Value::list(vec![
                    Value::Int((r | (r << 4) | (r << 8) | (r << 12)) as i64),
                    Value::Int((g | (g << 4) | (g << 8) | (g << 12)) as i64),
                    Value::Int((b | (b << 4) | (b << 8) | (b << 12)) as i64),
                ]))
            }
            6 => {
                // #RRGGBB -> scale 8-bit to 16-bit (multiply by 257)
                let r = u16::from_str_radix(&hex[0..2], 16).unwrap_or(0) as i64 * 257;
                let g = u16::from_str_radix(&hex[2..4], 16).unwrap_or(0) as i64 * 257;
                let b = u16::from_str_radix(&hex[4..6], 16).unwrap_or(0) as i64 * 257;
                Ok(Value::list(vec![
                    Value::Int(r),
                    Value::Int(g),
                    Value::Int(b),
                ]))
            }
            12 => {
                // #RRRRGGGGBBBB -> already 16-bit
                let r = u16::from_str_radix(&hex[0..4], 16).unwrap_or(0) as i64;
                let g = u16::from_str_radix(&hex[4..8], 16).unwrap_or(0) as i64;
                let b = u16::from_str_radix(&hex[8..12], 16).unwrap_or(0) as i64;
                Ok(Value::list(vec![
                    Value::Int(r),
                    Value::Int(g),
                    Value::Int(b),
                ]))
            }
            _ => Ok(Value::Nil),
        };
    }

    // Named colors (basic set).
    let (r, g, b) = match lower.as_str() {
        "black" => (0, 0, 0),
        "white" => (65535, 65535, 65535),
        "red" => (65535, 0, 0),
        "green" => (0, 65535, 0),
        "blue" => (0, 0, 65535),
        "yellow" => (65535, 65535, 0),
        "cyan" => (0, 65535, 65535),
        "magenta" => (65535, 0, 65535),
        "gray" | "grey" => (48573, 48573, 48573),
        "dark gray" | "dark grey" | "darkgray" | "darkgrey" => (43690, 43690, 43690),
        "light gray" | "light grey" | "lightgray" | "lightgrey" => (55512, 55512, 55512),
        "orange" => (65535, 42405, 0),
        "pink" => (65535, 49344, 52171),
        "brown" => (42405, 10794, 10794),
        "purple" => (32896, 0, 32896),
        _ => return Ok(Value::Nil),
    };
    Ok(Value::list(vec![
        Value::Int(r),
        Value::Int(g),
        Value::Int(b),
    ]))
}

/// Helper: convert a single hex digit char to a 4-bit value.
fn u16_from_hex_digit(byte: u8) -> u16 {
    match byte {
        b'0'..=b'9' => (byte - b'0') as u16,
        b'a'..=b'f' => (byte - b'a' + 10) as u16,
        b'A'..=b'F' => (byte - b'A' + 10) as u16,
        _ => 0,
    }
}

/// `(defined-colors &optional FRAME)` -- return a list of defined color names.
pub(crate) fn builtin_defined_colors(args: Vec<Value>) -> EvalResult {
    expect_max_args("defined-colors", &args, 1)?;
    let colors = vec![
        "black",
        "white",
        "red",
        "green",
        "blue",
        "yellow",
        "cyan",
        "magenta",
        "gray",
        "grey",
        "dark gray",
        "light gray",
        "orange",
        "pink",
        "brown",
        "purple",
    ];
    Ok(Value::list(colors.into_iter().map(Value::string).collect()))
}

/// `(face-id FACE)` -- stub, return 0.
pub(crate) fn builtin_face_id(args: Vec<Value>) -> EvalResult {
    expect_args("face-id", &args, 1)?;
    if let Value::Symbol(name) = &args[0] {
        if KNOWN_FACES.contains(&name.as_str()) {
            return Ok(Value::Int(0));
        }
    }
    let rendered = super::print::print_value(&args[0]);
    Err(signal(
        "error",
        vec![Value::string(format!("Not a face: {rendered}"))],
    ))
}

/// `(face-font FACE &optional FRAME CHARACTER)` -- stub, return nil.
pub(crate) fn builtin_face_font(args: Vec<Value>) -> EvalResult {
    expect_min_args("face-font", &args, 1)?;
    expect_max_args("face-font", &args, 3)?;
    if let Value::Symbol(name) = &args[0] {
        if KNOWN_FACES.contains(&name.as_str()) {
            return Ok(Value::Nil);
        }
    }
    Err(signal(
        "error",
        vec![Value::string("Invalid face"), args[0].clone()],
    ))
}

/// `(internal-face-x-get-resource RESOURCE CLASS FRAME)` -- stub, return nil.
pub(crate) fn builtin_internal_face_x_get_resource(args: Vec<Value>) -> EvalResult {
    expect_args("internal-face-x-get-resource", &args, 3)?;
    Ok(Value::Nil)
}

/// `(internal-set-font-selection-order ORDER)` -- stub, return nil.
pub(crate) fn builtin_internal_set_font_selection_order(args: Vec<Value>) -> EvalResult {
    expect_args("internal-set-font-selection-order", &args, 1)?;
    Ok(Value::Nil)
}

/// `(internal-set-alternative-font-family-alist ALIST)` -- stub, return nil.
pub(crate) fn builtin_internal_set_alternative_font_family_alist(args: Vec<Value>) -> EvalResult {
    expect_args("internal-set-alternative-font-family-alist", &args, 1)?;
    Ok(Value::Nil)
}

/// `(internal-set-alternative-font-registry-alist ALIST)` -- stub, return nil.
pub(crate) fn builtin_internal_set_alternative_font_registry_alist(args: Vec<Value>) -> EvalResult {
    expect_args("internal-set-alternative-font-registry-alist", &args, 1)?;
    Ok(Value::Nil)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Font builtins
    // -----------------------------------------------------------------------

    #[test]
    fn fontp_on_non_font() {
        assert_eq!(builtin_fontp(vec![Value::Int(42)]).unwrap().is_nil(), true);
        assert_eq!(
            builtin_fontp(vec![Value::string("hello")])
                .unwrap()
                .is_nil(),
            true
        );
    }

    #[test]
    fn font_spec_basic() {
        let spec = builtin_font_spec(vec![
            Value::Keyword("family".to_string()),
            Value::string("Monospace"),
            Value::Keyword("size".to_string()),
            Value::Int(12),
        ])
        .unwrap();
        assert!(is_font_spec(&spec));
        assert!(builtin_fontp(vec![spec]).unwrap().is_truthy());
    }

    #[test]
    fn font_spec_odd_args_error() {
        let result = builtin_font_spec(vec![Value::Keyword("family".to_string())]);
        assert!(result.is_err());
    }

    #[test]
    fn font_get_and_put() {
        let spec = builtin_font_spec(vec![
            Value::Keyword("family".to_string()),
            Value::string("Monospace"),
        ])
        .unwrap();

        // Get existing property.
        let family =
            builtin_font_get(vec![spec.clone(), Value::Keyword("family".to_string())]).unwrap();
        assert_eq!(family.as_str(), Some("Monospace"));

        // Get missing property.
        let missing =
            builtin_font_get(vec![spec.clone(), Value::Keyword("size".to_string())]).unwrap();
        assert!(missing.is_nil());

        // Put new property.
        let spec2 = builtin_font_put(vec![
            spec.clone(),
            Value::Keyword("size".to_string()),
            Value::Int(14),
        ])
        .unwrap();
        let size =
            builtin_font_get(vec![spec2.clone(), Value::Keyword("size".to_string())]).unwrap();
        assert_eq!(size.as_int(), Some(14));

        // Put overwriting existing property.
        let spec3 = builtin_font_put(vec![
            spec2,
            Value::Keyword("family".to_string()),
            Value::string("Serif"),
        ])
        .unwrap();
        let family2 = builtin_font_get(vec![spec3, Value::Keyword("family".to_string())]).unwrap();
        assert_eq!(family2.as_str(), Some("Serif"));
    }

    #[test]
    fn font_get_symbol_key() {
        // font-get should accept a symbol key and match against keyword storage.
        let spec = builtin_font_spec(vec![
            Value::Keyword("weight".to_string()),
            Value::symbol("bold"),
        ])
        .unwrap();
        let weight = builtin_font_get(vec![spec, Value::symbol("weight")]).unwrap();
        assert_eq!(weight.as_symbol_name(), Some("bold"));
    }

    #[test]
    fn font_get_non_vector() {
        // font-get on a non-vector should return nil, not error.
        let result = builtin_font_get(vec![Value::Int(42), Value::Keyword("family".to_string())]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn list_fonts_stub() {
        let result = builtin_list_fonts(vec![Value::vector(vec![Value::Keyword(
            FONT_SPEC_TAG.to_string(),
        )])]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn list_fonts_rejects_non_font_spec() {
        let result = builtin_list_fonts(vec![Value::Nil]);
        assert!(result.is_err());
    }

    #[test]
    fn find_font_stub() {
        let result = builtin_find_font(vec![Value::vector(vec![Value::Keyword(
            FONT_SPEC_TAG.to_string(),
        )])]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn find_font_rejects_non_font_spec() {
        let result = builtin_find_font(vec![Value::Nil]);
        assert!(result.is_err());
    }

    #[test]
    fn clear_font_cache_stub() {
        assert!(builtin_clear_font_cache(vec![]).unwrap().is_nil());
    }

    #[test]
    fn font_family_list_batch_returns_nil() {
        let result = builtin_font_family_list(vec![]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn font_family_list_rejects_non_nil_frame_designator() {
        let result = builtin_font_family_list(vec![Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn font_xlfd_name_stub() {
        let result = builtin_font_xlfd_name(vec![Value::vector(vec![Value::Keyword(
            FONT_SPEC_TAG.to_string(),
        )])])
        .unwrap();
        assert_eq!(result.as_str(), Some("*"));
    }

    // -----------------------------------------------------------------------
    // Face builtins
    // -----------------------------------------------------------------------

    #[test]
    fn internal_lisp_face_p_symbol() {
        let result = builtin_internal_lisp_face_p(vec![Value::symbol("default")]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn internal_lisp_face_p_non_symbol() {
        let result = builtin_internal_lisp_face_p(vec![Value::Int(42)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_lisp_face_p_nil_is_symbol() {
        // nil is a symbol in Emacs.
        let result = builtin_internal_lisp_face_p(vec![Value::Nil]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn internal_copy_lisp_face_returns_to() {
        let result = builtin_internal_copy_lisp_face(vec![
            Value::symbol("bold"),
            Value::symbol("my-face"),
            Value::Nil,
            Value::Nil,
        ])
        .unwrap();
        assert_eq!(result.as_symbol_name(), Some("my-face"));
    }

    #[test]
    fn internal_set_lisp_face_attribute_returns_value() {
        let result = builtin_internal_set_lisp_face_attribute(vec![
            Value::symbol("default"),
            Value::Keyword("foreground".to_string()),
            Value::string("white"),
        ])
        .unwrap();
        assert_eq!(result.as_str(), Some("white"));
    }

    #[test]
    fn internal_get_lisp_face_attribute_returns_nil() {
        let result = builtin_internal_get_lisp_face_attribute(vec![
            Value::symbol("default"),
            Value::Keyword("foreground".to_string()),
        ])
        .unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_merge_in_global_face_stub() {
        let result =
            builtin_internal_merge_in_global_face(vec![Value::symbol("default"), Value::Nil])
                .unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn face_attribute_relative_p_unspecified() {
        let result = builtin_face_attribute_relative_p(vec![
            Value::Keyword("foreground".to_string()),
            Value::symbol("unspecified"),
        ])
        .unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn face_attribute_relative_p_concrete() {
        let result = builtin_face_attribute_relative_p(vec![
            Value::Keyword("foreground".to_string()),
            Value::string("white"),
        ])
        .unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn merge_face_attribute_non_unspecified() {
        let result = builtin_merge_face_attribute(vec![
            Value::Keyword("foreground".to_string()),
            Value::string("red"),
            Value::string("blue"),
        ])
        .unwrap();
        assert_eq!(result.as_str(), Some("red"));
    }

    #[test]
    fn merge_face_attribute_unspecified() {
        let result = builtin_merge_face_attribute(vec![
            Value::Keyword("foreground".to_string()),
            Value::symbol("unspecified"),
            Value::string("blue"),
        ])
        .unwrap();
        assert_eq!(result.as_str(), Some("blue"));
    }

    #[test]
    fn face_list_returns_known_faces() {
        let result = builtin_face_list(vec![]).unwrap();
        let faces = list_to_vec(&result).unwrap();
        let names: Vec<&str> = faces.iter().filter_map(|v| v.as_symbol_name()).collect();
        assert!(names.contains(&"default"));
        assert!(names.contains(&"bold"));
        assert!(names.contains(&"cursor"));
        assert!(names.contains(&"mode-line"));
    }

    #[test]
    fn color_defined_p_always_true() {
        let result = builtin_color_defined_p(vec![Value::string("anything")]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn color_values_named_black() {
        let result = builtin_color_values(vec![Value::string("black")]).unwrap();
        let rgb = list_to_vec(&result).unwrap();
        assert_eq!(rgb.len(), 3);
        assert_eq!(rgb[0].as_int(), Some(0));
        assert_eq!(rgb[1].as_int(), Some(0));
        assert_eq!(rgb[2].as_int(), Some(0));
    }

    #[test]
    fn color_values_named_white() {
        let result = builtin_color_values(vec![Value::string("white")]).unwrap();
        let rgb = list_to_vec(&result).unwrap();
        assert_eq!(rgb[0].as_int(), Some(65535));
        assert_eq!(rgb[1].as_int(), Some(65535));
        assert_eq!(rgb[2].as_int(), Some(65535));
    }

    #[test]
    fn color_values_hex_rrggbb() {
        // #FF0000 -> red (255*257 = 65535, 0, 0)
        let result = builtin_color_values(vec![Value::string("#FF0000")]).unwrap();
        let rgb = list_to_vec(&result).unwrap();
        assert_eq!(rgb[0].as_int(), Some(65535));
        assert_eq!(rgb[1].as_int(), Some(0));
        assert_eq!(rgb[2].as_int(), Some(0));
    }

    #[test]
    fn color_values_hex_short() {
        // #F00 -> red
        let result = builtin_color_values(vec![Value::string("#F00")]).unwrap();
        let rgb = list_to_vec(&result).unwrap();
        assert!(rgb[0].as_int().unwrap() > 0);
        assert_eq!(rgb[1].as_int(), Some(0));
        assert_eq!(rgb[2].as_int(), Some(0));
    }

    #[test]
    fn color_values_hex_12digit() {
        // #FFFF00000000
        let result = builtin_color_values(vec![Value::string("#FFFF00000000")]).unwrap();
        let rgb = list_to_vec(&result).unwrap();
        assert_eq!(rgb[0].as_int(), Some(65535));
        assert_eq!(rgb[1].as_int(), Some(0));
        assert_eq!(rgb[2].as_int(), Some(0));
    }

    #[test]
    fn color_values_unknown_returns_nil() {
        let result = builtin_color_values(vec![Value::string("nonexistent-color")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn color_values_wrong_type() {
        let result = builtin_color_values(vec![Value::Int(42)]);
        assert!(result.is_err());
    }

    #[test]
    fn defined_colors_returns_list() {
        let result = builtin_defined_colors(vec![]).unwrap();
        assert!(result.is_list());
        assert!(!result.is_nil());
    }

    #[test]
    fn face_id_stub() {
        let result = builtin_face_id(vec![Value::symbol("default")]).unwrap();
        assert_eq!(result.as_int(), Some(0));
    }

    #[test]
    fn face_id_rejects_invalid_face() {
        let result = builtin_face_id(vec![Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn face_font_stub() {
        let result = builtin_face_font(vec![Value::symbol("default")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn face_font_rejects_invalid_face() {
        let result = builtin_face_font(vec![Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn internal_face_x_get_resource_stub() {
        let result = builtin_internal_face_x_get_resource(vec![
            Value::string("font"),
            Value::string("Font"),
            Value::Nil,
        ])
        .unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_set_font_selection_order_stub() {
        let result = builtin_internal_set_font_selection_order(vec![Value::Nil]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_set_alternative_font_family_alist_stub() {
        let result = builtin_internal_set_alternative_font_family_alist(vec![Value::Nil]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_set_alternative_font_registry_alist_stub() {
        let result =
            builtin_internal_set_alternative_font_registry_alist(vec![Value::Nil]).unwrap();
        assert!(result.is_nil());
    }

    // -----------------------------------------------------------------------
    // Arity checks
    // -----------------------------------------------------------------------

    #[test]
    fn fontp_too_many_args() {
        let result = builtin_fontp(vec![Value::Nil, Value::Nil, Value::Nil]);
        assert!(result.is_err());
    }

    #[test]
    fn fontp_no_args() {
        let result = builtin_fontp(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn font_get_wrong_arity() {
        assert!(builtin_font_get(vec![Value::Nil]).is_err());
        assert!(builtin_font_get(vec![Value::Nil, Value::Nil, Value::Nil]).is_err());
    }

    #[test]
    fn font_put_wrong_arity() {
        assert!(builtin_font_put(vec![Value::Nil, Value::Nil]).is_err());
    }

    #[test]
    fn face_attribute_relative_p_wrong_arity() {
        assert!(builtin_face_attribute_relative_p(vec![Value::Nil]).is_err());
    }

    #[test]
    fn merge_face_attribute_wrong_arity() {
        assert!(builtin_merge_face_attribute(vec![Value::Nil, Value::Nil]).is_err());
    }

    #[test]
    fn color_values_case_insensitive() {
        let result = builtin_color_values(vec![Value::string("RED")]).unwrap();
        let rgb = list_to_vec(&result).unwrap();
        assert_eq!(rgb[0].as_int(), Some(65535));
        assert_eq!(rgb[1].as_int(), Some(0));
        assert_eq!(rgb[2].as_int(), Some(0));
    }

    #[test]
    fn color_values_hex_lowercase() {
        let result = builtin_color_values(vec![Value::string("#ff8000")]).unwrap();
        let rgb = list_to_vec(&result).unwrap();
        assert_eq!(rgb[0].as_int(), Some(255 * 257)); // 65535
        assert_eq!(rgb[1].as_int(), Some(128 * 257)); // 32896
        assert_eq!(rgb[2].as_int(), Some(0));
    }
}
