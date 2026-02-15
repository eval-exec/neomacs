//! Font and face builtins for the Elisp interpreter.
//!
//! Font builtins:
//! - `fontp`, `font-spec`, `font-get`, `font-put`, `list-fonts`, `find-font`,
//!   `clear-font-cache`, `font-family-list`, `font-xlfd-name`
//!
//! Face builtins:
//! - `internal-make-lisp-face`, `internal-lisp-face-p`, `internal-copy-lisp-face`,
//!   `internal-set-lisp-face-attribute`, `internal-get-lisp-face-attribute`,
//!   `internal-merge-in-global-face`, `face-attribute-relative-p`,
//!   `merge-face-attribute`, `face-list`, `color-defined-p`, `color-values`,
//!   `defined-colors`, `face-id`, `face-font`, `internal-face-x-get-resource`,
//!   `internal-set-font-selection-order`,
//!   `internal-set-alternative-font-family-alist`,
//!   `internal-set-alternative-font-registry-alist`

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

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
const FIRST_DYNAMIC_FACE_ID: i64 = 133;

fn known_face_id(name: &str) -> Option<i64> {
    match name {
        "default" => Some(0),
        "bold" => Some(1),
        "italic" => Some(2),
        "underline" => Some(4),
        "highlight" => Some(12),
        "region" => Some(13),
        "mode-line" => Some(25),
        "mode-line-inactive" => Some(27),
        "fringe" => Some(40),
        "cursor" => Some(43),
        _ => None,
    }
}

const LISP_FACE_VECTOR_LEN: usize = 20;
const VALID_FACE_ATTRIBUTES: &[&str] = &[
    ":family",
    ":foundry",
    ":height",
    ":weight",
    ":slant",
    ":underline",
    ":overline",
    ":strike-through",
    ":box",
    ":inverse-video",
    ":foreground",
    ":distant-foreground",
    ":background",
    ":stipple",
    ":width",
    ":inherit",
    ":extend",
    ":font",
    ":fontset",
];
const DISCRETE_BOOLEAN_FACE_ATTRIBUTES: &[&str] = &[
    ":underline",
    ":overline",
    ":strike-through",
    ":inverse-video",
    ":extend",
];
const SET_ONLY_FACE_ATTRIBUTES: &[&str] = &[":bold", ":italic"];
const VALID_FACE_WEIGHTS: &[&str] = &[
    "ultra-light",
    "extra-light",
    "light",
    "semi-light",
    "normal",
    "semi-bold",
    "bold",
    "extra-bold",
    "ultra-bold",
];
const VALID_FACE_SLANTS: &[&str] = &[
    "normal",
    "italic",
    "oblique",
    "reverse-italic",
    "reverse-oblique",
];
const VALID_FACE_WIDTHS: &[&str] = &[
    "ultra-condensed",
    "extra-condensed",
    "condensed",
    "semi-condensed",
    "normal",
    "semi-expanded",
    "expanded",
    "extra-expanded",
    "ultra-expanded",
];

static CREATED_LISP_FACES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static CREATED_FACE_IDS: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
static NEXT_CREATED_FACE_ID: OnceLock<Mutex<i64>> = OnceLock::new();
#[derive(Default)]
struct FaceAttrState {
    selected_created: HashSet<String>,
    selected_overrides: std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    defaults_overrides: std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
}

static FACE_ATTR_STATE: OnceLock<Mutex<FaceAttrState>> = OnceLock::new();

fn created_lisp_faces() -> &'static Mutex<HashSet<String>> {
    CREATED_LISP_FACES.get_or_init(|| Mutex::new(HashSet::new()))
}

fn created_face_ids() -> &'static Mutex<HashMap<String, i64>> {
    CREATED_FACE_IDS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_created_face_id() -> &'static Mutex<i64> {
    NEXT_CREATED_FACE_ID.get_or_init(|| Mutex::new(FIRST_DYNAMIC_FACE_ID))
}

fn face_attr_state() -> &'static Mutex<FaceAttrState> {
    FACE_ATTR_STATE.get_or_init(|| Mutex::new(FaceAttrState::default()))
}

fn is_created_lisp_face(name: &str) -> bool {
    created_lisp_faces()
        .lock()
        .expect("poisoned")
        .contains(name)
}

fn mark_created_lisp_face(name: &str) {
    let inserted = created_lisp_faces()
        .lock()
        .expect("poisoned")
        .insert(name.to_string());
    if inserted {
        ensure_dynamic_face_id(name);
    }
}

fn ensure_dynamic_face_id(name: &str) {
    if known_face_id(name).is_some() {
        return;
    }
    let mut ids = created_face_ids().lock().expect("poisoned");
    if ids.contains_key(name) {
        return;
    }
    let mut next = next_created_face_id().lock().expect("poisoned");
    ids.insert(name.to_string(), *next);
    *next += 1;
}

fn dynamic_face_id(name: &str) -> Option<i64> {
    created_face_ids()
        .lock()
        .expect("poisoned")
        .get(name)
        .copied()
}

fn face_id_for_name(name: &str) -> Option<i64> {
    known_face_id(name).or_else(|| dynamic_face_id(name))
}

fn is_selected_created_lisp_face(name: &str) -> bool {
    face_attr_state()
        .lock()
        .expect("poisoned")
        .selected_created
        .contains(name)
}

fn mark_selected_created_lisp_face(name: &str) {
    face_attr_state()
        .lock()
        .expect("poisoned")
        .selected_created
        .insert(name.to_string());
}

fn face_exists_for_domain(name: &str, defaults_frame: bool) -> bool {
    if KNOWN_FACES.contains(&name) {
        return true;
    }
    if defaults_frame {
        is_created_lisp_face(name)
    } else {
        is_selected_created_lisp_face(name)
    }
}

fn get_face_override(face_name: &str, attr: &str, defaults_frame: bool) -> Option<Value> {
    let state = face_attr_state().lock().expect("poisoned");
    let map = if defaults_frame {
        &state.defaults_overrides
    } else {
        &state.selected_overrides
    };
    map.get(face_name).and_then(|attrs| attrs.get(attr)).cloned()
}

fn set_face_override(face_name: &str, attr: &str, value: Value, defaults_frame: bool) {
    let mut state = face_attr_state().lock().expect("poisoned");
    let map = if defaults_frame {
        &mut state.defaults_overrides
    } else {
        &mut state.selected_overrides
    };
    map.entry(face_name.to_string())
        .or_default()
        .insert(attr.to_string(), value);
}

fn clear_face_overrides(face_name: &str, defaults_frame: bool) {
    let mut state = face_attr_state().lock().expect("poisoned");
    if defaults_frame {
        state.defaults_overrides.remove(face_name);
    } else {
        state.selected_overrides.remove(face_name);
    }
}

fn copy_defaults_overrides(src: &str, dst: &str) {
    let mut state = face_attr_state().lock().expect("poisoned");
    let copied = state.defaults_overrides.get(src).cloned();
    if let Some(attrs) = copied {
        state.defaults_overrides.insert(dst.to_string(), attrs);
    } else {
        state.defaults_overrides.remove(dst);
    }
}

fn merge_defaults_overrides_into_selected(face_name: &str) {
    let mut state = face_attr_state().lock().expect("poisoned");
    let defaults = state.defaults_overrides.get(face_name).cloned();
    if let Some(attrs) = defaults {
        let selected = state.selected_overrides.entry(face_name.to_string()).or_default();
        for (attr, value) in attrs {
            if matches!(&value, Value::Symbol(s) if s == "unspecified" || s == "relative") {
                continue;
            }
            selected.insert(attr, value);
        }
    }
}

fn symbol_name_for_face_value(face: &Value) -> Option<String> {
    match face {
        Value::Nil => Some("nil".to_string()),
        Value::True => Some("t".to_string()),
        Value::Symbol(name) => Some(name.clone()),
        _ => None,
    }
}

fn require_symbol_face_name(face: &Value) -> Result<String, Flow> {
    symbol_name_for_face_value(face).ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("symbolp"), face.clone()],
        )
    })
}

fn known_face_name(face: &Value) -> Option<String> {
    let name = match face {
        Value::Str(name) => name.as_str().to_string(),
        _ => symbol_name_for_face_value(face)?,
    };
    if KNOWN_FACES.contains(&name.as_str()) || is_created_lisp_face(&name) {
        Some(name)
    } else {
        None
    }
}

fn resolve_copy_source_face_symbol(face: &Value) -> Result<String, Flow> {
    let name = symbol_name_for_face_value(face).expect("checked symbol before resolve");
    if KNOWN_FACES.contains(&name.as_str()) || is_created_lisp_face(&name) {
        return Ok(name);
    }
    if face.is_nil() {
        return Err(signal("error", vec![Value::string("Invalid face")]));
    }
    Err(signal(
        "error",
        vec![Value::string("Invalid face"), face.clone()],
    ))
}

fn resolve_face_name_for_domain(face: &Value, defaults_frame: bool) -> Result<String, Flow> {
    match face {
        Value::Str(name) => {
            if face_exists_for_domain(name, defaults_frame) {
                Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("symbolp"), face.clone()],
                ))
            } else {
                Err(signal(
                    "error",
                    vec![Value::string("Invalid face"), Value::symbol(name.as_str())],
                ))
            }
        }
        Value::Nil | Value::True | Value::Symbol(_) => {
            let name = symbol_name_for_face_value(face).expect("symbol-like");
            if face_exists_for_domain(&name, defaults_frame) {
                Ok(name)
            } else if face.is_nil() {
                Err(signal("error", vec![Value::string("Invalid face")]))
            } else {
                Err(signal(
                    "error",
                    vec![Value::string("Invalid face"), face.clone()],
                ))
            }
        }
        _ => Err(signal(
            "error",
            vec![Value::string("Invalid face"), face.clone()],
        )),
    }
}

fn resolve_face_name_for_merge(face: &Value) -> Result<String, Flow> {
    match face {
        Value::Str(name) => {
            if face_exists_for_domain(name, true) {
                Ok((**name).clone())
            } else {
                Err(signal(
                    "error",
                    vec![Value::string("Invalid face"), Value::symbol(name.as_str())],
                ))
            }
        }
        Value::Nil | Value::True | Value::Symbol(_) => {
            let name = symbol_name_for_face_value(face).expect("symbol-like");
            if face_exists_for_domain(&name, true) {
                Ok(name)
            } else if face.is_nil() {
                Err(signal("error", vec![Value::string("Invalid face")]))
            } else {
                Err(signal(
                    "error",
                    vec![Value::string("Invalid face"), face.clone()],
                ))
            }
        }
        _ => Err(signal(
            "error",
            vec![Value::string("Invalid face"), face.clone()],
        )),
    }
}

fn make_lisp_face_vector() -> Value {
    let mut values = Vec::with_capacity(LISP_FACE_VECTOR_LEN);
    values.push(Value::symbol("face"));
    values.extend((1..LISP_FACE_VECTOR_LEN).map(|_| Value::symbol("unspecified")));
    Value::vector(values)
}

fn normalize_face_attribute_name(attr: &Value) -> Result<String, Flow> {
    let name = match attr {
        Value::Symbol(name) => name.clone(),
        Value::Keyword(name) => {
            if name.starts_with(':') {
                name.clone()
            } else {
                format!(":{name}")
            }
        }
        Value::Nil | Value::True => attr.as_symbol_name().unwrap_or_default().to_string(),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), attr.clone()],
            ));
        }
    };

    if VALID_FACE_ATTRIBUTES.contains(&name.as_str()) {
        Ok(name)
    } else if attr.is_nil() {
        Err(signal(
            "error",
            vec![Value::string("Invalid face attribute name")],
        ))
    } else {
        Err(signal(
            "error",
            vec![Value::string("Invalid face attribute name"), attr.clone()],
        ))
    }
}

fn normalize_set_face_attribute_name(attr: &Value) -> Result<String, Flow> {
    let name = match attr {
        Value::Symbol(name) => name.clone(),
        Value::Keyword(name) => {
            if name.starts_with(':') {
                name.clone()
            } else {
                format!(":{name}")
            }
        }
        Value::Nil | Value::True => attr.as_symbol_name().unwrap_or_default().to_string(),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), attr.clone()],
            ));
        }
    };

    if VALID_FACE_ATTRIBUTES.contains(&name.as_str()) || SET_ONLY_FACE_ATTRIBUTES.contains(&name.as_str()) {
        Ok(name)
    } else if attr.is_nil() {
        Err(signal(
            "error",
            vec![Value::string("Invalid face attribute name")],
        ))
    } else {
        Err(signal(
            "error",
            vec![Value::string("Invalid face attribute name"), attr.clone()],
        ))
    }
}

fn default_face_attribute_value(attr: &str) -> Value {
    match attr {
        ":family" | ":foundry" => Value::string("default"),
        ":height" => Value::Int(1),
        ":weight" | ":slant" | ":width" => Value::symbol("normal"),
        ":underline" | ":overline" | ":strike-through" | ":box" | ":inverse-video"
        | ":stipple" | ":inherit" | ":extend" | ":fontset" => Value::Nil,
        ":foreground" => Value::string("unspecified-fg"),
        ":background" => Value::string("unspecified-bg"),
        ":distant-foreground" | ":font" => Value::symbol("unspecified"),
        _ => Value::symbol("unspecified"),
    }
}

fn lisp_face_attribute_base_value(face: &str, attr: &str, defaults_frame: bool) -> Value {
    if defaults_frame {
        return Value::symbol("unspecified");
    }
    if face == "default" {
        return default_face_attribute_value(attr);
    }
    match (face, attr) {
        ("bold", ":weight") => Value::symbol("bold"),
        ("italic", ":slant") => Value::symbol("italic"),
        ("underline", ":underline") => Value::True,
        ("highlight", ":inverse-video") => Value::True,
        ("region", ":inverse-video") => Value::True,
        ("mode-line", ":inverse-video") => Value::True,
        ("mode-line-inactive", ":inherit") => Value::symbol("mode-line"),
        ("fringe", ":background") => Value::string("gray"),
        ("cursor", ":background") => Value::string("white"),
        _ => Value::symbol("unspecified"),
    }
}

fn lisp_face_attribute_value(face: &str, attr: &str, defaults_frame: bool) -> Value {
    if let Some(value) = get_face_override(face, attr, defaults_frame) {
        return value;
    }
    lisp_face_attribute_base_value(face, attr, defaults_frame)
}

fn resolve_known_face_name_for_compare(face: &Value, defaults_frame: bool) -> Result<String, Flow> {
    match face {
        Value::Str(name) => {
            if face_exists_for_domain(name, defaults_frame) {
                Ok((**name).clone())
            } else {
                Err(signal(
                    "error",
                    vec![Value::string("Invalid face"), Value::symbol(name.as_str())],
                ))
            }
        }
        _ => resolve_face_name_for_domain(face, defaults_frame),
    }
}

fn face_attr_value_name(attr: &Value) -> Result<String, Flow> {
    match attr {
        Value::Keyword(name) => {
            if name.starts_with(':') {
                Ok(name.clone())
            } else {
                Ok(format!(":{name}"))
            }
        }
        Value::Symbol(name) => Ok(name.clone()),
        Value::Nil => Ok("nil".to_string()),
        Value::True => Ok("t".to_string()),
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("symbolp"), attr.clone()],
        )),
    }
}

fn frame_defaults_flag(frame: Option<&Value>) -> Result<bool, Flow> {
    match frame {
        None => Ok(false),
        Some(v) if v.is_nil() => Ok(false),
        Some(Value::True) => Ok(true),
        Some(v) => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("frame-live-p"), v.clone()],
        )),
    }
}

fn proper_list_to_vec_or_listp_error(value: &Value) -> Result<Vec<Value>, Flow> {
    let mut out = Vec::new();
    let mut cursor = value.clone();
    loop {
        match cursor {
            Value::Nil => return Ok(out),
            Value::Cons(cell) => {
                let cell = cell.lock().expect("poisoned");
                out.push(cell.car.clone());
                cursor = cell.cdr.clone();
            }
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("listp"), other],
                ));
            }
        }
    }
}

fn check_non_empty_string(value: &Value, empty_message: &str) -> Result<(), Flow> {
    match value {
        Value::Str(s) => {
            if s.is_empty() {
                Err(signal(
                    "error",
                    vec![Value::string(empty_message), value.clone()],
                ))
            } else {
                Ok(())
            }
        }
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), value.clone()],
        )),
    }
}

fn symbol_name_or_type_error(value: &Value) -> Result<String, Flow> {
    match value {
        Value::Nil => Ok("nil".to_string()),
        Value::True => Ok("t".to_string()),
        Value::Symbol(s) => Ok(s.clone()),
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("symbolp"), value.clone()],
        )),
    }
}

fn normalize_face_attr_for_set(face_name: &str, attr: &str, value: Value) -> Result<(String, Value), Flow> {
    let normalized = match attr {
        ":foreground" | ":background" | ":distant-foreground" if value.is_nil() => {
            Value::symbol("unspecified")
        }
        _ => value,
    };
    let is_reset_like =
        matches!(&normalized, Value::Symbol(s) if s == "unspecified" || s == ":ignore-defface" || s == "reset");

    match attr {
        ":family" | ":foundry" => {
            if !is_reset_like {
                match &normalized {
                    Value::Str(s) if !s.is_empty() => {}
                    Value::Str(_) => {
                        let msg = if attr == ":family" {
                            "Invalid face family"
                        } else {
                            "Invalid face foundry"
                        };
                        return Err(signal("error", vec![Value::string(msg), normalized]));
                    }
                    _ => {
                        return Err(signal(
                            "wrong-type-argument",
                            vec![Value::symbol("stringp"), normalized],
                        ));
                    }
                }
            }
        }
        ":height" => {
            if !is_reset_like {
                if face_name == "default" {
                    match &normalized {
                        Value::Int(n) if *n > 0 => {}
                        _ => {
                            return Err(signal(
                                "error",
                                vec![
                                    Value::string("Default face height not absolute and positive"),
                                    normalized,
                                ],
                            ));
                        }
                    }
                } else {
                    match &normalized {
                        Value::Int(n) if *n > 0 => {}
                        Value::Float(f) if *f > 0.0 => {}
                        _ => {
                            return Err(signal(
                                "error",
                                vec![
                                    Value::string("Face height does not produce a positive integer"),
                                    normalized,
                                ],
                            ));
                        }
                    }
                }
            }
        }
        ":weight" => {
            if !is_reset_like {
                let sym = symbol_name_or_type_error(&normalized)?;
                if !VALID_FACE_WEIGHTS.contains(&sym.as_str()) {
                    return Err(signal("error", vec![Value::string("Invalid face weight"), normalized]));
                }
            }
        }
        ":slant" => {
            if !is_reset_like {
                let sym = symbol_name_or_type_error(&normalized)?;
                if !VALID_FACE_SLANTS.contains(&sym.as_str()) {
                    return Err(signal("error", vec![Value::string("Invalid face slant"), normalized]));
                }
            }
        }
        ":width" => {
            if !is_reset_like {
                let sym = symbol_name_or_type_error(&normalized)?;
                if !VALID_FACE_WIDTHS.contains(&sym.as_str()) {
                    return Err(signal("error", vec![Value::string("Invalid face width"), normalized]));
                }
            }
        }
        ":foreground" => {
            if !is_reset_like {
                check_non_empty_string(&normalized, "Empty foreground color value")?;
            }
        }
        ":background" => {
            if !is_reset_like {
                check_non_empty_string(&normalized, "Empty background color value")?;
            }
        }
        ":distant-foreground" => {
            if !is_reset_like {
                check_non_empty_string(&normalized, "Empty distant-foreground color value")?;
            }
        }
        ":inverse-video" => {
            if !is_reset_like {
                let sym = symbol_name_or_type_error(&normalized)?;
                if sym != "t" && sym != "nil" {
                    return Err(signal(
                        "error",
                        vec![
                            Value::string("Invalid inverse-video face attribute value"),
                            normalized,
                        ],
                    ));
                }
            }
        }
        ":extend" => {
            if !is_reset_like {
                let sym = symbol_name_or_type_error(&normalized)?;
                if sym != "t" && sym != "nil" {
                    return Err(signal(
                        "error",
                        vec![Value::string("Invalid extend face attribute value"), normalized],
                    ));
                }
            }
        }
        ":inherit" => {
            let valid = match &normalized {
                Value::Nil | Value::True | Value::Symbol(_) => true,
                Value::Cons(_) => list_to_vec(&normalized)
                    .map(|vals| vals.iter().all(Value::is_symbol))
                    .unwrap_or(false),
                _ => false,
            };
            if !valid {
                let mut payload = vec![Value::string("Invalid face inheritance")];
                if let Some(vals) = list_to_vec(&normalized) {
                    payload.extend(vals);
                } else {
                    payload.push(normalized);
                }
                return Err(signal("error", payload));
            }
        }
        ":bold" => {
            let mapped = if normalized.is_nil() {
                Value::symbol("normal")
            } else {
                Value::symbol("bold")
            };
            return Ok((":weight".to_string(), mapped));
        }
        ":italic" => {
            let mapped = if normalized.is_nil() {
                Value::symbol("normal")
            } else {
                Value::symbol("italic")
            };
            return Ok((":slant".to_string(), mapped));
        }
        _ => {}
    }

    Ok((attr.to_string(), normalized))
}

/// `(internal-lisp-face-p FACE &optional FRAME)` -- return a face descriptor
/// vector for known faces, nil otherwise.
pub(crate) fn builtin_internal_lisp_face_p(args: Vec<Value>) -> EvalResult {
    expect_min_args("internal-lisp-face-p", &args, 1)?;
    expect_max_args("internal-lisp-face-p", &args, 2)?;
    if let Some(frame) = args.get(1) {
        if !frame.is_nil() {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("frame-live-p"), frame.clone()],
            ));
        }
    }
    if known_face_name(&args[0]).is_some() {
        Ok(make_lisp_face_vector())
    } else {
        Ok(Value::Nil)
    }
}

/// `(internal-make-lisp-face FACE &optional FRAME)` -- create/reset FACE as a
/// Lisp face and return its attribute vector.
pub(crate) fn builtin_internal_make_lisp_face(args: Vec<Value>) -> EvalResult {
    expect_min_args("internal-make-lisp-face", &args, 1)?;
    expect_max_args("internal-make-lisp-face", &args, 2)?;
    let face_name = require_symbol_face_name(&args[0])?;
    if let Some(frame) = args.get(1) {
        if !frame.is_nil() {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("frame-live-p"), frame.clone()],
            ));
        }
    }
    mark_created_lisp_face(&face_name);
    clear_face_overrides(&face_name, true);
    Ok(make_lisp_face_vector())
}

/// `(internal-copy-lisp-face FROM TO FRAME NEW-FRAME)` -- stub, return TO.
pub(crate) fn builtin_internal_copy_lisp_face(args: Vec<Value>) -> EvalResult {
    expect_args("internal-copy-lisp-face", &args, 4)?;
    let _ = require_symbol_face_name(&args[0])?;
    let to_name = require_symbol_face_name(&args[1])?;
    if !matches!(args[2], Value::True) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("frame-live-p"), args[2].clone()],
        ));
    }
    let from_name = resolve_copy_source_face_symbol(&args[0])?;
    mark_created_lisp_face(&to_name);
    copy_defaults_overrides(&from_name, &to_name);
    Ok(args[1].clone())
}

/// `(internal-set-lisp-face-attribute FACE ATTR VALUE &optional FRAME)` --
/// set FACE attribute in selected-frame/default face domains.
pub(crate) fn builtin_internal_set_lisp_face_attribute(args: Vec<Value>) -> EvalResult {
    expect_min_args("internal-set-lisp-face-attribute", &args, 3)?;
    expect_max_args("internal-set-lisp-face-attribute", &args, 4)?;
    let face = &args[0];
    let face_name = require_symbol_face_name(face)?;
    let attr_name = normalize_set_face_attribute_name(&args[1])?;
    let value = args[2].clone();

    let apply_set = |defaults_frame: bool| -> Result<(), Flow> {
        if defaults_frame {
            if !face_exists_for_domain(&face_name, true) {
                if face.is_nil() {
                    return Err(signal("error", vec![Value::string("Invalid face")]));
                }
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid face"), face.clone()],
                ));
            }
        } else if !face_exists_for_domain(&face_name, false) {
            mark_selected_created_lisp_face(&face_name);
            mark_created_lisp_face(&face_name);
        }

        let (canonical_attr, canonical_value) =
            normalize_face_attr_for_set(&face_name, &attr_name, value.clone())?;
        set_face_override(&face_name, &canonical_attr, canonical_value, defaults_frame);
        Ok(())
    };

    match args.get(3) {
        None | Some(Value::Nil) => apply_set(false)?,
        Some(Value::True) => apply_set(true)?,
        Some(Value::Int(0)) => {
            apply_set(true)?;
            apply_set(false)?;
        }
        Some(other) => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("frame-live-p"), other.clone()],
            ));
        }
    }

    Ok(face.clone())
}

/// `(internal-get-lisp-face-attribute FACE ATTR &optional FRAME)` -- batch
/// semantics-compatible face attribute query for core predefined faces.
pub(crate) fn builtin_internal_get_lisp_face_attribute(args: Vec<Value>) -> EvalResult {
    expect_min_args("internal-get-lisp-face-attribute", &args, 2)?;
    expect_max_args("internal-get-lisp-face-attribute", &args, 3)?;
    let defaults_frame = if let Some(frame) = args.get(2) {
        if frame.is_nil() {
            false
        } else if matches!(frame, Value::True) {
            true
        } else {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("frame-live-p"), frame.clone()],
            ));
        }
    } else {
        false
    };

    let face_name = resolve_face_name_for_domain(&args[0], defaults_frame)?;

    let attr_name = normalize_face_attribute_name(&args[1])?;
    Ok(lisp_face_attribute_value(&face_name, &attr_name, defaults_frame))
}

/// `(internal-lisp-face-attribute-values ATTR)` -- return valid discrete values
/// for known boolean-like face attributes.
pub(crate) fn builtin_internal_lisp_face_attribute_values(args: Vec<Value>) -> EvalResult {
    expect_args("internal-lisp-face-attribute-values", &args, 1)?;
    let attr_name = face_attr_value_name(&args[0])?;
    if DISCRETE_BOOLEAN_FACE_ATTRIBUTES.contains(&attr_name.as_str()) {
        Ok(Value::list(vec![Value::True, Value::Nil]))
    } else {
        Ok(Value::Nil)
    }
}

/// `(internal-lisp-face-equal-p FACE1 FACE2 &optional FRAME)` -- return t if
/// FACE1 and FACE2 resolve to equal face attributes in the selected frame or in
/// default face definitions when FRAME is t.
pub(crate) fn builtin_internal_lisp_face_equal_p(args: Vec<Value>) -> EvalResult {
    expect_min_args("internal-lisp-face-equal-p", &args, 2)?;
    expect_max_args("internal-lisp-face-equal-p", &args, 3)?;
    let defaults_frame = frame_defaults_flag(args.get(2))?;
    let face1 = resolve_known_face_name_for_compare(&args[0], defaults_frame)?;
    let face2 = resolve_known_face_name_for_compare(&args[1], defaults_frame)?;
    for attr in VALID_FACE_ATTRIBUTES {
        let v1 = lisp_face_attribute_value(&face1, attr, defaults_frame);
        let v2 = lisp_face_attribute_value(&face2, attr, defaults_frame);
        if v1 != v2 {
            return Ok(Value::Nil);
        }
    }
    Ok(Value::True)
}

/// `(internal-lisp-face-empty-p FACE &optional FRAME)` -- return t if FACE has
/// only unspecified attributes in selected/default face definitions.
pub(crate) fn builtin_internal_lisp_face_empty_p(args: Vec<Value>) -> EvalResult {
    expect_min_args("internal-lisp-face-empty-p", &args, 1)?;
    expect_max_args("internal-lisp-face-empty-p", &args, 2)?;
    let defaults_frame = frame_defaults_flag(args.get(1))?;
    let face = resolve_known_face_name_for_compare(&args[0], defaults_frame)?;
    for attr in VALID_FACE_ATTRIBUTES {
        let v = lisp_face_attribute_value(&face, attr, defaults_frame);
        if !matches!(v, Value::Symbol(ref name) if name == "unspecified") {
            return Ok(Value::Nil);
        }
    }
    Ok(Value::True)
}

/// `(internal-merge-in-global-face FACE FRAME)` -- merge concrete defaults-face
/// overrides into selected-frame face state.
pub(crate) fn builtin_internal_merge_in_global_face(args: Vec<Value>) -> EvalResult {
    expect_args("internal-merge-in-global-face", &args, 2)?;
    if !matches!(args[1], Value::Int(n) if n > 0) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("frame-live-p"), args[1].clone()],
        ));
    }
    let face_name = resolve_face_name_for_merge(&args[0])?;
    if !KNOWN_FACES.contains(&face_name.as_str()) {
        mark_created_lisp_face(&face_name);
        mark_selected_created_lisp_face(&face_name);
    }
    merge_defaults_overrides_into_selected(&face_name);
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

/// `(face-id FACE &optional FRAME)` -- return numeric face id for known and
/// dynamically created faces.
pub(crate) fn builtin_face_id(args: Vec<Value>) -> EvalResult {
    expect_min_args("face-id", &args, 1)?;
    expect_max_args("face-id", &args, 2)?;
    if matches!(&args[0], Value::Str(_)) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("symbolp"), args[0].clone()],
        ));
    }

    if let Some(name) = symbol_name_for_face_value(&args[0]) {
        if let Some(id) = face_id_for_name(&name) {
            return Ok(Value::Int(id));
        }
        if is_created_lisp_face(&name) {
            ensure_dynamic_face_id(&name);
            if let Some(id) = face_id_for_name(&name) {
                return Ok(Value::Int(id));
            }
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
    match &args[0] {
        Value::Str(name) => {
            if KNOWN_FACES.contains(&name.as_str()) {
                Ok(Value::Nil)
            } else {
                let payload = if name.is_empty() {
                    Value::symbol("##")
                } else {
                    Value::symbol(name.as_str())
                };
                Err(signal(
                    "error",
                    vec![Value::string("Invalid face"), payload],
                ))
            }
        }
        Value::Nil => Err(signal("error", vec![Value::string("Invalid face")])),
        Value::True | Value::Symbol(_) => {
            if let Some(name) = symbol_name_for_face_value(&args[0]) {
                if KNOWN_FACES.contains(&name.as_str()) {
                    return Ok(Value::Nil);
                }
            }
            Err(signal(
                "error",
                vec![Value::string("Invalid face"), args[0].clone()],
            ))
        }
        _ => Err(signal(
            "error",
            vec![Value::string("Invalid face"), args[0].clone()],
        )),
    }
}

/// `(internal-face-x-get-resource RESOURCE CLASS FRAME)` -- stub, return nil.
pub(crate) fn builtin_internal_face_x_get_resource(args: Vec<Value>) -> EvalResult {
    expect_args("internal-face-x-get-resource", &args, 3)?;
    Ok(Value::Nil)
}

/// `(internal-set-font-selection-order ORDER)` -- stub, return nil.
pub(crate) fn builtin_internal_set_font_selection_order(args: Vec<Value>) -> EvalResult {
    expect_args("internal-set-font-selection-order", &args, 1)?;
    let order = &args[0];
    if !order.is_nil() && !matches!(order, Value::Cons(_)) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("listp"), order.clone()],
        ));
    }

    let valid_keywords = [":width", ":height", ":weight", ":slant"];
    let valid = if let Some(values) = list_to_vec(order) {
        if values.len() == valid_keywords.len() {
            let mut seen = HashSet::new();
            values.iter().all(|value| {
                if let Value::Keyword(name) = value {
                    let key = if name.starts_with(':') {
                        name.clone()
                    } else {
                        format!(":{name}")
                    };
                    valid_keywords.contains(&key.as_str()) && seen.insert(key)
                } else {
                    false
                }
            })
        } else {
            false
        }
    } else {
        false
    };

    if valid {
        return Ok(Value::Nil);
    }

    if let Some(values) = list_to_vec(order) {
        if values.is_empty() {
            return Err(signal("error", vec![Value::string("Invalid font sort order")]));
        }
        let mut payload = vec![Value::string("Invalid font sort order")];
        payload.extend(values);
        return Err(signal("error", payload));
    }

    Err(signal(
        "error",
        vec![Value::string("Invalid font sort order"), order.clone()],
    ))
}

/// `(internal-set-alternative-font-family-alist ALIST)` -- stub, return nil.
pub(crate) fn builtin_internal_set_alternative_font_family_alist(args: Vec<Value>) -> EvalResult {
    expect_args("internal-set-alternative-font-family-alist", &args, 1)?;
    let entries = proper_list_to_vec_or_listp_error(&args[0])?;
    let mut normalized = Vec::with_capacity(entries.len());
    for entry in entries {
        let members = proper_list_to_vec_or_listp_error(&entry)?;
        let mut converted = Vec::with_capacity(members.len());
        for member in members {
            match member {
                Value::Str(s) => converted.push(Value::symbol((*s).clone())),
                other => {
                    return Err(signal(
                        "wrong-type-argument",
                        vec![Value::symbol("stringp"), other],
                    ));
                }
            }
        }
        normalized.push(Value::list(converted));
    }
    Ok(Value::list(normalized))
}

/// `(internal-set-alternative-font-registry-alist ALIST)` -- stub, return nil.
pub(crate) fn builtin_internal_set_alternative_font_registry_alist(args: Vec<Value>) -> EvalResult {
    expect_args("internal-set-alternative-font-registry-alist", &args, 1)?;
    let entries = proper_list_to_vec_or_listp_error(&args[0])?;
    for entry in entries {
        let _ = proper_list_to_vec_or_listp_error(&entry)?;
    }
    Ok(args[0].clone())
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
    fn internal_lisp_face_p_symbol_returns_face_vector() {
        let result = builtin_internal_lisp_face_p(vec![Value::symbol("default")]).unwrap();
        let values = match result {
            Value::Vector(v) => v.lock().expect("poisoned").clone(),
            _ => panic!("expected vector"),
        };
        assert_eq!(values.len(), LISP_FACE_VECTOR_LEN);
        assert_eq!(values[0].as_symbol_name(), Some("face"));
    }

    #[test]
    fn internal_lisp_face_p_non_symbol() {
        let result = builtin_internal_lisp_face_p(vec![Value::Int(42)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_lisp_face_p_nil_returns_nil() {
        let result = builtin_internal_lisp_face_p(vec![Value::Nil]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_lisp_face_p_rejects_non_nil_frame_designator() {
        let result = builtin_internal_lisp_face_p(vec![Value::symbol("default"), Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn internal_make_lisp_face_creates_symbol_visible_to_internal_lisp_face_p() {
        let name = Value::symbol("__neovm_make_face_unit_test");
        let made = builtin_internal_make_lisp_face(vec![name.clone()]).unwrap();
        assert!(matches!(made, Value::Vector(_)));
        let exists = builtin_internal_lisp_face_p(vec![name]).unwrap();
        assert!(matches!(exists, Value::Vector(_)));
    }

    #[test]
    fn internal_make_lisp_face_rejects_non_symbol_and_non_nil_frame() {
        assert!(builtin_internal_make_lisp_face(vec![Value::string("foo")]).is_err());
        assert!(builtin_internal_make_lisp_face(vec![Value::symbol("foo"), Value::Int(1)]).is_err());
    }

    #[test]
    fn internal_copy_lisp_face_returns_to_when_frame_t() {
        let result = builtin_internal_copy_lisp_face(vec![
            Value::symbol("bold"),
            Value::symbol("my-face"),
            Value::True,
            Value::Nil,
        ])
        .unwrap();
        assert_eq!(result.as_symbol_name(), Some("my-face"));
    }

    #[test]
    fn internal_copy_lisp_face_rejects_non_t_frame_designator() {
        let result = builtin_internal_copy_lisp_face(vec![
            Value::symbol("default"),
            Value::symbol("my-face"),
            Value::Nil,
            Value::Nil,
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn internal_copy_lisp_face_uses_symbol_checks_before_frame_checks() {
        let result = builtin_internal_copy_lisp_face(vec![
            Value::Int(1),
            Value::symbol("my-face"),
            Value::Nil,
            Value::Nil,
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn internal_set_lisp_face_attribute_returns_value() {
        let face = Value::symbol("__neovm_set_attr_unit_test");
        let result = builtin_internal_set_lisp_face_attribute(vec![
            face.clone(),
            Value::Keyword("foreground".to_string()),
            Value::string("white"),
        ])
        .unwrap();
        assert_eq!(result, face);
    }

    #[test]
    fn internal_get_lisp_face_attribute_default_foreground() {
        let result = builtin_internal_get_lisp_face_attribute(vec![
            Value::symbol("default"),
            Value::Keyword(":foreground".to_string()),
        ])
        .unwrap();
        assert_eq!(result.as_str(), Some("unspecified-fg"));
    }

    #[test]
    fn internal_get_lisp_face_attribute_mode_line_returns_unspecified() {
        let result = builtin_internal_get_lisp_face_attribute(vec![
            Value::symbol("mode-line"),
            Value::Keyword(":foreground".to_string()),
        ])
        .unwrap();
        assert_eq!(result.as_symbol_name(), Some("unspecified"));
    }

    #[test]
    fn internal_get_lisp_face_attribute_defaults_frame_returns_unspecified() {
        let result = builtin_internal_get_lisp_face_attribute(vec![
            Value::symbol("default"),
            Value::Keyword(":foreground".to_string()),
            Value::True,
        ])
        .unwrap();
        assert_eq!(result.as_symbol_name(), Some("unspecified"));
    }

    #[test]
    fn internal_get_lisp_face_attribute_invalid_face_errors() {
        let result = builtin_internal_get_lisp_face_attribute(vec![
            Value::symbol("unknown-face"),
            Value::Keyword(":foreground".to_string()),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn internal_get_lisp_face_attribute_invalid_attr_errors() {
        let wrong_type =
            builtin_internal_get_lisp_face_attribute(vec![Value::symbol("default"), Value::Int(1)]);
        assert!(wrong_type.is_err());

        let invalid_name = builtin_internal_get_lisp_face_attribute(vec![
            Value::symbol("default"),
            Value::symbol("bogus"),
        ]);
        assert!(invalid_name.is_err());
    }

    #[test]
    fn internal_lisp_face_attribute_values_discrete_boolean_attrs() {
        let result = builtin_internal_lisp_face_attribute_values(vec![Value::Keyword(
            ":underline".to_string(),
        )])
        .unwrap();
        let vals = list_to_vec(&result).expect("list");
        assert_eq!(vals, vec![Value::True, Value::Nil]);
    }

    #[test]
    fn internal_lisp_face_attribute_values_non_discrete_attr_is_nil() {
        let result = builtin_internal_lisp_face_attribute_values(vec![Value::Keyword(
            ":weight".to_string(),
        )])
        .unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_lisp_face_attribute_values_rejects_non_symbol() {
        let result = builtin_internal_lisp_face_attribute_values(vec![Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn internal_lisp_face_empty_p_selected_frame_default_is_not_empty() {
        let result = builtin_internal_lisp_face_empty_p(vec![Value::symbol("default")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_lisp_face_empty_p_accepts_string_face_name() {
        let result = builtin_internal_lisp_face_empty_p(vec![Value::string("default")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_lisp_face_empty_p_defaults_frame_is_empty() {
        let result =
            builtin_internal_lisp_face_empty_p(vec![Value::symbol("default"), Value::True]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn internal_lisp_face_empty_p_rejects_non_nil_non_t_frame_designator() {
        let result = builtin_internal_lisp_face_empty_p(vec![Value::symbol("default"), Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn internal_lisp_face_equal_p_selected_frame_distinguishes_faces() {
        let result = builtin_internal_lisp_face_equal_p(vec![
            Value::symbol("default"),
            Value::symbol("mode-line"),
        ])
        .unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_lisp_face_equal_p_defaults_frame_treats_faces_as_equal() {
        let result = builtin_internal_lisp_face_equal_p(vec![
            Value::symbol("default"),
            Value::symbol("mode-line"),
            Value::True,
        ])
        .unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn internal_lisp_face_equal_p_accepts_string_face_names() {
        let result = builtin_internal_lisp_face_equal_p(vec![
            Value::string("default"),
            Value::string("default"),
        ])
        .unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn internal_merge_in_global_face_rejects_non_frame_designator() {
        let result = builtin_internal_merge_in_global_face(vec![Value::symbol("default"), Value::Nil]);
        assert!(result.is_err());
    }

    #[test]
    fn internal_merge_in_global_face_copies_defaults_into_selected_face() {
        let face = Value::symbol("__neovm_merge_face_unit_test");
        let _ = builtin_internal_make_lisp_face(vec![face.clone()]).unwrap();
        let _ = builtin_internal_set_lisp_face_attribute(vec![
            face.clone(),
            Value::Keyword("foreground".to_string()),
            Value::string("white"),
            Value::True,
        ])
        .unwrap();
        let merged =
            builtin_internal_merge_in_global_face(vec![face.clone(), Value::Int(1)]).unwrap();
        assert!(merged.is_nil());
        let got = builtin_internal_get_lisp_face_attribute(vec![
            face.clone(),
            Value::Keyword(":foreground".to_string()),
        ])
        .unwrap();
        assert_eq!(got.as_str(), Some("white"));
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
    fn face_id_known_faces_use_oracle_ids() {
        let bold = builtin_face_id(vec![Value::symbol("bold")]).unwrap();
        assert_eq!(bold.as_int(), Some(1));
        let mode_line = builtin_face_id(vec![Value::symbol("mode-line")]).unwrap();
        assert_eq!(mode_line.as_int(), Some(25));
    }

    #[test]
    fn face_id_accepts_optional_frame_argument() {
        let result = builtin_face_id(vec![Value::symbol("default"), Value::Nil]).unwrap();
        assert_eq!(result.as_int(), Some(0));
    }

    #[test]
    fn face_id_assigns_dynamic_id_for_created_faces() {
        let face = Value::symbol("__neovm_face_id_dynamic_unit_test");
        let _ = builtin_internal_make_lisp_face(vec![face.clone()]).unwrap();
        let first = builtin_face_id(vec![face.clone()]).unwrap();
        let second = builtin_face_id(vec![face]).unwrap();
        assert_eq!(first, second);
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
    fn face_font_accepts_known_string_face() {
        let result = builtin_face_font(vec![Value::string("default")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn face_font_ignores_optional_arguments_for_known_face() {
        let result = builtin_face_font(vec![
            Value::symbol("default"),
            Value::Nil,
            Value::True,
        ])
        .unwrap();
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
        let result = builtin_internal_set_font_selection_order(vec![Value::list(vec![
            Value::Keyword(":width".to_string()),
            Value::Keyword(":height".to_string()),
            Value::Keyword(":weight".to_string()),
            Value::Keyword(":slant".to_string()),
        ])])
        .unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_set_font_selection_order_rejects_invalid_order() {
        let result =
            builtin_internal_set_font_selection_order(vec![Value::list(vec![Value::symbol("x")])]);
        assert!(result.is_err());
    }

    #[test]
    fn internal_set_alternative_font_family_alist_stub() {
        let result = builtin_internal_set_alternative_font_family_alist(vec![Value::Nil]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_set_alternative_font_family_alist_converts_strings_to_symbols() {
        let input = Value::list(vec![Value::list(vec![Value::string("Foo"), Value::string("Bar")])]);
        let result = builtin_internal_set_alternative_font_family_alist(vec![input]).unwrap();
        let outer = list_to_vec(&result).expect("outer list");
        let inner = list_to_vec(&outer[0]).expect("inner list");
        assert_eq!(inner[0].as_symbol_name(), Some("Foo"));
        assert_eq!(inner[1].as_symbol_name(), Some("Bar"));
    }

    #[test]
    fn internal_set_alternative_font_registry_alist_stub() {
        let result =
            builtin_internal_set_alternative_font_registry_alist(vec![Value::Nil]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn internal_set_alternative_font_registry_alist_preserves_values() {
        let input = Value::list(vec![Value::list(vec![Value::Int(1), Value::Int(2)])]);
        let result = builtin_internal_set_alternative_font_registry_alist(vec![input.clone()]).unwrap();
        assert_eq!(result, input);
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
