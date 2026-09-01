//! GNU Emacs `xfaces.c` surface: face builtins, lface vectors, bootstrap vars.
//!
//! Owns the C-level face state (`face--new-frame-defaults`, the per-frame
//! face hash tables, created-face registries) plus the xfaces.c builtin
//! family: `internal-*-lisp-face*`, `face-attribute-relative-p`,
//! `merge-face-attribute`, `face-list`, `face-id`, `face-font`, the color
//! builtins, `x-load-color-file`, and the font-selection-order /
//! alternative-font alist setters. The face attribute *algebra* (the
//! `Face` value type, merge core, `FaceTable` derived cache) lives in
//! `crate::face`; font.c matching stays in `super::font`.

use crate::emacs_core::error::EvalResult;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use crate::emacs_core::intern::resolve_sym;
use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::{HashKey, HashTableTest, Value, ValueKind, list_to_vec};
use crate::face::{LFACE_VECTOR_SIZE, LFaceAttr};

/// Register bootstrap variables owned by the face subsystem.
pub fn register_bootstrap_vars(obarray: &mut Obarray) {
    obarray.set_symbol_value(
        "face--new-frame-defaults",
        bootstrap_face_new_frame_defaults_table(),
    );
    stamp_face_id_properties(obarray);
    // xfaces.c:7624-7751 DEFVAR cluster, GNU C inits.
    obarray.define_special_variable("face-default-stipple", Value::string("gray3"));
    obarray.define_special_variable("tty-defined-color-alist", Value::NIL);
    obarray.set_symbol_value("scalable-fonts-allowed", Value::NIL);
    obarray.define_special_variable("face-ignored-fonts", Value::NIL);
    obarray.define_special_variable("face-remapping-alist", Value::NIL);
    obarray.define_special_variable("face-font-rescale-alist", Value::NIL);
    obarray.define_int_variable("face-near-same-color-threshold", 30_000);
    obarray.define_special_variable("face-font-lax-matched-attributes", Value::T);
}

/// Backfill xfaces-owned bootstrap variables after loading a dump or partial
/// source bootstrap. GNU owns these in xfaces.c, so load/bootstrap glue should
/// delegate here instead of duplicating the values itself.
pub(crate) fn ensure_startup_compat_variables(eval: &mut crate::emacs_core::eval::Context) {
    match eval
        .obarray()
        .symbol_value("face--new-frame-defaults")
        .copied()
    {
        Some(table) if table.is_hash_table() => seed_face_new_frame_defaults_table(table),
        _ => eval.set_variable(
            "face--new-frame-defaults",
            bootstrap_face_new_frame_defaults_table(),
        ),
    }
    // Establish the `face` id property alongside the registry seed above, now
    // that every defface has run and all faces are known. This is the startup
    // sync point that fixes `face-id` for bootstrap faces (see
    // stamp_face_id_properties).
    stamp_face_id_properties(eval.obarray_mut());

    let defaults = [
        ("face-filters-always-match", Value::NIL),
        ("face-default-stipple", Value::string("gray3")),
        ("scalable-fonts-allowed", Value::NIL),
        ("face-ignored-fonts", Value::NIL),
        ("face-remapping-alist", Value::NIL),
        ("face-font-rescale-alist", Value::NIL),
        ("face-near-same-color-threshold", Value::fixnum(30_000)),
        ("face-font-lax-matched-attributes", Value::T),
    ];
    for (name, value) in defaults {
        if eval.obarray().symbol_value(name).is_none() {
            eval.set_variable(name, value);
        }
    }
}

pub(crate) fn builtin_frame_face_hash_table(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    crate::emacs_core::display::expect_args_range("frame--face-hash-table", &args, 0, 1)?;
    let frame_id = crate::emacs_core::window_cmds::resolve_frame_id_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        args.first(),
        "frame-live-p",
    )?;

    Ok(eval
        .frames
        .get(frame_id)
        .map(|frame| frame.face_hash_table())
        .unwrap_or(Value::hash_table(HashTableTest::Eq)))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn unspecified_face_attributes_vector() -> Value {
    Value::vector(vec![Value::symbol("unspecified"); LFACE_VECTOR_SIZE])
}

fn face_attr_key_name(value: &Value) -> Option<&str> {
    match value.kind() {
        ValueKind::Symbol(id) => Some(resolve_sym(id)),
        _ => None,
    }
}

pub(crate) fn builtin_face_attributes_as_vector(args: Vec<Value>) -> EvalResult {
    crate::emacs_core::display::expect_args("face-attributes-as-vector", &args, 1)?;

    let mut attrs = vec![Value::symbol("unspecified"); LFACE_VECTOR_SIZE];
    let Some(plist) = list_to_vec(&args[0]) else {
        return Ok(Value::vector(attrs));
    };

    let mut i = 0;
    while i + 1 < plist.len() {
        let Some(attr) = face_attr_key_name(&plist[i]).and_then(LFaceAttr::from_keyword) else {
            i += 2;
            continue;
        };
        let slot = attr.index();

        let value = plist[i + 1];
        match attr {
            LFaceAttr::Foreground | LFaceAttr::Background | LFaceAttr::DistantForeground
                if value.is_nil() => {}
            LFaceAttr::Stipple | LFaceAttr::Font | LFaceAttr::Inherit | LFaceAttr::Fontset => {}
            LFaceAttr::Box if value.is_t() => attrs[slot] = Value::fixnum(1),
            _ => attrs[slot] = value,
        }

        i += 2;
    }

    Ok(Value::vector(attrs))
}

pub(crate) fn init_frame_lisp_faces(frame: &mut crate::window::Frame) {
    let table = frame.face_hash_table();
    for face_name in all_defined_face_names_sorted_by_id_desc().iter() {
        insert_frame_face_hash_entry_if_absent(
            table,
            Value::symbol(face_name.as_str()),
            make_lisp_face_vector_for_frame(face_name.as_str()),
        );
    }
}

/// Stamp every defined face's numeric id onto its `face` symbol property, the
/// store that `face-id` / `(get FACE 'face)` read (faces.el `face-id`).
///
/// GNU assigns this property in `internal-make-lisp-face`, which `make-face`
/// invokes from its `(dolist (frame (frame-list)) ...)` loop. Neomacs registers
/// the standard faces during the bootstrap image build, before any frame
/// exists, so that loop is a no-op there and only the `face--new-frame-defaults`
/// registry entry (whose CAR holds the id) got populated -- leaving `face-id` to
/// signal "Not a face: nil" for every bootstrap face. Establishing the property
/// alongside the registry seed, from the same face set and id source
/// (`face_id_for_name`), keeps the entry and the id property from ever drifting.
pub(crate) fn stamp_face_id_properties(obarray: &mut Obarray) {
    for face_name in all_defined_face_names_sorted_by_id_desc().iter() {
        if let Some(face_id) = face_id_for_name(face_name.as_str()) {
            let _ = obarray.put_property(face_name.as_str(), "face", Value::fixnum(face_id));
        }
    }
}

pub(crate) fn seed_face_new_frame_defaults_table(table: Value) {
    let face_names = all_defined_face_names_sorted_by_id_desc();
    let face_entries: Vec<(Value, Value)> = face_names
        .iter()
        .filter_map(|face_name| {
            let face_id = face_id_for_name(face_name.as_str())?;
            Some((
                Value::symbol(face_name.as_str()),
                Value::cons(Value::fixnum(face_id), make_lisp_face_vector()),
            ))
        })
        .collect();

    for (key, value) in face_entries {
        insert_frame_face_hash_entry_if_absent(table, key, value);
    }
}

fn bootstrap_face_new_frame_defaults_table() -> Value {
    let table = Value::hash_table(HashTableTest::Eq);
    seed_face_new_frame_defaults_table(table);
    table
}

pub(crate) fn ensure_face_new_frame_defaults_entry(
    eval: &mut crate::emacs_core::eval::Context,
    face_name: &str,
) -> Option<Value> {
    let table = eval
        .obarray()
        .symbol_value("face--new-frame-defaults")
        .copied()?;
    // The table is fully seeded once at bootstrap (`register_bootstrap_vars`)
    // and again at startup (`ensure_startup_compat_variables`). Re-seeding here
    // -- on every ensure/lookup -- rebuilt every face's cons + lface vector only
    // to discard them in `insert_..._if_absent`, an O(faces) allocation storm
    // that made `internal-lisp-face-p` the top self-time hotspot. A genuine miss
    // for the single requested face is still handled by the create-on-miss path
    // below, so dropping the blanket re-seed changes no results.
    let key = Value::symbol(face_name);
    if let Some(entry) = lookup_frame_face_hash_entry(table, key) {
        return Some(entry);
    }

    let face_id = face_id_for_name(face_name)?;
    // Restore GNU's invariant that a known face carries its numeric id as the
    // `face` symbol property -- this is what `face-id` and `(get FACE 'face)`
    // read (faces.el `face-id`). GNU assigns it in `internal-make-lisp-face`,
    // invoked by `make-face`'s `(dolist (frame (frame-list)) ...)` loop. Neomacs
    // defines the standard faces during the bootstrap image build, before any
    // frame exists, so that loop is a no-op there and only the entry CAR held
    // the id. Stamp the property at the point the face first becomes known so
    // the id survives for bootstrap faces too, not just runtime `defface`s.
    eval.obarray_mut()
        .put_property(face_name, "face", Value::fixnum(face_id))
        .ok();
    let entry = Value::cons(Value::fixnum(face_id), make_lisp_face_vector());
    upsert_frame_face_hash_entry(table, key, entry);
    Some(entry)
}

/// Remove a face's entry from `face--new-frame-defaults`.
///
/// The table is the canonical existence store that `internal-lisp-face-p`'s fast
/// path reads (via [`lookup_face_new_frame_defaults_vector`]). Every OTHER face
/// predicate decides existence from the known/created-face set
/// (`is_known_lisp_face_name` UNION `CREATED_LISP_FACES`). Those two stores must
/// agree, so any code that removes a face from the created-face set
/// (`clear_created_lisp_face`, e.g. on source unload) MUST also call this, or the
/// predicate would keep reporting a stale face (a hit short-circuits the
/// known-set gate). Keeping creation (`ensure_face_new_frame_defaults_entry`) and
/// removal here is the single source of truth for table membership.
pub(crate) fn remove_face_new_frame_defaults_entry(
    eval: &crate::emacs_core::eval::Context,
    face_name: &str,
) {
    let Some(table) = eval
        .obarray()
        .symbol_value("face--new-frame-defaults")
        .copied()
    else {
        return;
    };
    if table.is_hash_table() {
        // Face keys are plain interned symbols; symbols_with_pos is irrelevant.
        let _ = crate::emacs_core::builtins::collections::builtin_remhash_values(
            Value::symbol(face_name),
            table,
            false,
        );
    }
}

pub(crate) fn face_new_frame_defaults_vector(
    eval: &mut crate::emacs_core::eval::Context,
    face_name: &str,
) -> Option<Value> {
    let entry = ensure_face_new_frame_defaults_entry(eval, face_name)?;
    if entry.is_cons() {
        Some(entry.cons_cdr())
    } else {
        None
    }
}

/// Pure, allocation-free read of the global `face--new-frame-defaults` table,
/// mirroring GNU's `lface_from_face_name` for the null-frame case: one
/// symbol-keyed hash lookup, no seeding and no create-on-miss. `key` must be an
/// already-interned symbol `Value`. Returns the lface vector (entry CDR), or
/// None when the face is absent. Callers that need create-on-miss (defface,
/// copy-face, make-lisp-face) use `face_new_frame_defaults_vector` instead.
pub(crate) fn lookup_face_new_frame_defaults_vector(
    eval: &crate::emacs_core::eval::Context,
    key: Value,
) -> Option<Value> {
    let table = eval
        .obarray()
        .symbol_value("face--new-frame-defaults")
        .copied()?;
    let entry = lookup_frame_face_hash_entry(table, key)?;
    if entry.is_cons() {
        Some(entry.cons_cdr())
    } else {
        None
    }
}

pub(crate) fn lookup_frame_face_hash_entry(table: Value, key: Value) -> Option<Value> {
    if !table.is_hash_table() {
        return None;
    }
    let hash_table = table.as_hash_table()?;
    let hash_key = match key.kind() {
        ValueKind::Symbol(id) => HashKey::Symbol(id),
        _ => return None,
    };
    hash_table.data.get(&hash_key).copied()
}

fn insert_frame_face_hash_entry_if_absent(table: Value, key: Value, value: Value) {
    if lookup_frame_face_hash_entry(table, key).is_none() {
        upsert_frame_face_hash_entry(table, key, value);
    } else {
        let _ = table.with_hash_table_mut(|hash_table| {
            let hash_key = match key.kind() {
                ValueKind::Symbol(id) => HashKey::Symbol(id),
                _ => unreachable!("face hash keys are symbols"),
            };
            hash_table.replace_key_snapshot(&hash_key, key);
        });
    }
}

pub(crate) fn upsert_frame_face_hash_entry(table: Value, key: Value, value: Value) {
    if !table.is_hash_table() {
        unreachable!("frame face hash table must be a hash table");
    };
    let _ = table.with_hash_table_mut(|hash_table| {
        let hash_key = match key.kind() {
            ValueKind::Symbol(id) => HashKey::Symbol(id),
            _ => unreachable!("face hash keys are symbols"),
        };
        // Use the O(1) puthash-style upsert; `ensure_hash_key_iterable`'s
        // duplicate scan is O(n) and made face realisation O(n^2).
        hash_table.upsert_iterable(hash_key, key, value);
    });
}

use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::OnceLock;

use num_enum::{IntoPrimitive, TryFromPrimitive};
use strum::{EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

use super::error::{Flow, LispCondition, signal};
use super::font::{
    alternative_font_family_alist, alternative_font_registry_alist,
    default_face_font_attr_affects_frame_font, face_remapping_for_current_buffer,
    font_name_for_face, font_name_value, font_string_text, font_value_fields, font_value_text,
    font_vector_get_flexible, frame_device_designator_p, frame_id_from_designator,
    frame_parameter_for_face_attribute, is_font, is_font_entity, is_font_spec,
    live_frame_designator_in_state, live_frame_font_attribute_fallback,
    opened_font_from_resolved_match, publish_face_attribute_to_frame_parameter, resolve_font_match,
    resolve_live_frame_font_request, sync_live_default_face_font_state, sync_live_frame_font_state,
};

use super::intern::intern;
use crate::emacs_core::SymId;
use crate::face::{
    BoxStyle, Face as RuntimeFace, FontSlant, FontWeight, FontWidth, LFACE_ATTRS, UnderlineStyle,
};
use crate::tagged::header::store_value_atomic;
use crate::window::{FrameId, FrameManager, FrameParam};
use neomacs_display_protocol::TerminalColor;

// ===========================================================================
// Face builtins (pure)
// ===========================================================================

/// Lisp face IDs assigned during GNU Emacs `-Q` loadup.
///
/// These IDs are the symbol `face` property returned by `(face-id FACE)`.
/// They are distinct from GNU's realized display-cache `enum face_id`.
#[repr(i64)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    EnumIter,
    EnumString,
    IntoPrimitive,
    IntoStaticStr,
    TryFromPrimitive,
)]
#[strum(serialize_all = "kebab-case")]
enum GnuBootstrapLispFaceId {
    Default = 0,
    Bold = 1,
    Italic = 2,
    BoldItalic = 3,
    Underline = 4,
    FixedPitch = 5,
    FixedPitchSerif = 6,
    VariablePitch = 7,
    VariablePitchText = 8,
    Shadow = 9,
    Link = 10,
    LinkVisited = 11,
    Highlight = 12,
    Region = 13,
    SecondarySelection = 14,
    TrailingWhitespace = 15,
    LineNumber = 16,
    LineNumberCurrentLine = 17,
    LineNumberMajorTick = 18,
    LineNumberMinorTick = 19,
    FillColumnIndicator = 20,
    EscapeGlyph = 21,
    Homoglyph = 22,
    NobreakSpace = 23,
    NobreakHyphen = 24,
    ModeLine = 25,
    ModeLineActive = 26,
    ModeLineInactive = 27,
    ModeLineHighlight = 28,
    ModeLineEmphasis = 29,
    ModeLineBufferId = 30,
    HeaderLine = 31,
    HeaderLineHighlight = 32,
    HeaderLineActive = 33,
    HeaderLineInactive = 34,
    VerticalBorder = 35,
    WindowDivider = 36,
    WindowDividerFirstPixel = 37,
    WindowDividerLastPixel = 38,
    InternalBorder = 39,
    ChildFrameBorder = 40,
    MinibufferPrompt = 41,
    Margin = 42,
    Fringe = 43,
    ScrollBar = 44,
    Border = 45,
    Cursor = 46,
    Mouse = 47,
    ToolBar = 48,
    TabBar = 49,
    TabLine = 50,
    TabLineActive = 51,
    TabLineInactive = 52,
    Menu = 53,
    HelpArgumentName = 54,
    HelpKeyBinding = 55,
    GlyphlessChar = 56,
    Error = 57,
    Warning = 58,
    Success = 59,
    ReadMultipleChoiceFace = 60,
    TtyMenuEnabledFace = 61,
    TtyMenuDisabledFace = 62,
    TtyMenuSelectedFace = 63,
    ShowParenMatch = 64,
    ShowParenMatchExpression = 65,
    ShowParenMismatch = 66,
    Button = 67,
    AbbrevTableName = 68,
    HelpForHelpHeader = 69,
    ConfusinglyReordered = 70,
    NextError = 71,
    NextErrorMessage = 72,
    SeparatorLine = 73,
    BlinkMatchingParenOffscreen = 74,
    CompletionsGroupTitle = 75,
    CompletionsGroupSeparator = 76,
    CompletionsAnnotations = 77,
    CompletionsHighlight = 78,
    CompletionsFirstDifference = 79,
    CompletionsCommonPart = 80,
    MinibufferNonselected = 81,
    FontLockCommentFace = 82,
    FontLockCommentDelimiterFace = 83,
    FontLockStringFace = 84,
    FontLockDocFace = 85,
    FontLockDocMarkupFace = 86,
    FontLockKeywordFace = 87,
    FontLockBuiltinFace = 88,
    FontLockFunctionNameFace = 89,
    FontLockFunctionCallFace = 90,
    FontLockVariableNameFace = 91,
    FontLockVariableUseFace = 92,
    FontLockTypeFace = 93,
    FontLockConstantFace = 94,
    FontLockWarningFace = 95,
    FontLockNegationCharFace = 96,
    FontLockPreprocessorFace = 97,
    FontLockRegexpFace = 98,
    FontLockRegexpGroupingBackslash = 99,
    FontLockRegexpGroupingConstruct = 100,
    FontLockEscapeFace = 101,
    FontLockNumberFace = 102,
    FontLockOperatorFace = 103,
    FontLockPropertyNameFace = 104,
    FontLockPropertyUseFace = 105,
    FontLockPunctuationFace = 106,
    FontLockBracketFace = 107,
    FontLockDelimiterFace = 108,
    FontLockMiscPunctuationFace = 109,
    MouseDragAndDropRegion = 110,
    Isearch = 111,
    IsearchFail = 112,
    LazyHighlight = 113,
    #[strum(to_string = "isearch-group-1")]
    IsearchGroup1 = 114,
    #[strum(to_string = "isearch-group-2")]
    IsearchGroup2 = 115,
    FileNameShadow = 116,
    TabBarTab = 117,
    TabBarTabInactive = 118,
    TabBarTabGroupCurrent = 119,
    TabBarTabGroupInactive = 120,
    TabBarTabUngrouped = 121,
    TabBarTabHighlight = 122,
    QueryReplace = 123,
    Match = 124,
    TabulatedListFakeHeader = 125,
    BufferMenuBuffer = 126,
    ElispSymbolAtMouse = 127,
    ElispFreeVariable = 128,
    ElispSpecialVariableDeclaration = 129,
    ElispCondition = 130,
    ElispMajorModeName = 131,
    ElispFace = 132,
    ElispSymbolRole = 133,
    ElispSymbolRoleDefinition = 134,
    ElispFunction = 135,
    ElispNonLocalExit = 136,
    ElispUnknownCall = 137,
    ElispMacro = 138,
    ElispSpecialForm = 139,
    ElispThrowTag = 140,
    ElispFeature = 141,
    ElispRx = 142,
    ElispTheme = 143,
    ElispBindingVariable = 144,
    ElispBoundVariable = 145,
    ElispShadowingVariable = 146,
    ElispShadowedVariable = 147,
    ElispVariableAtPoint = 148,
    ElispWarningType = 149,
    ElispFunctionPropertyDeclaration = 150,
    ElispThing = 151,
    ElispSlot = 152,
    ElispWidgetType = 153,
    ElispType = 154,
    ElispGroup = 155,
    ElispNnooBackend = 156,
    ElispAmpersand = 157,
    ElispConstant = 158,
    ElispDefun = 159,
    ElispDefmacro = 160,
    ElispDefvar = 161,
    ElispDefface = 162,
    ElispIcon = 163,
    ElispDeficon = 164,
    ElispOclosure = 165,
    ElispDefoclosure = 166,
    ElispCoding = 167,
    ElispDefcoding = 168,
    ElispCharset = 169,
    ElispDefcharset = 170,
    ElispCompletionCategory = 171,
    ElispCompletionCategoryDefinition = 172,
    VcStateBase = 173,
    VcUpToDateState = 174,
    VcNeedsUpdateState = 175,
    VcLockedState = 176,
    VcLocallyAddedState = 177,
    VcConflictState = 178,
    VcRemovedState = 179,
    VcMissingState = 180,
    VcEditedState = 181,
    VcIgnoredState = 182,
    ElispShorthandFontLockFace = 183,
    EldocHighlightFunctionArgument = 184,
    Tooltip = 185,
}

const GNU_BOOTSTRAP_LISP_FACES: &[GnuBootstrapLispFaceId] = &[
    GnuBootstrapLispFaceId::Default,
    GnuBootstrapLispFaceId::Bold,
    GnuBootstrapLispFaceId::Italic,
    GnuBootstrapLispFaceId::BoldItalic,
    GnuBootstrapLispFaceId::Underline,
    GnuBootstrapLispFaceId::FixedPitch,
    GnuBootstrapLispFaceId::FixedPitchSerif,
    GnuBootstrapLispFaceId::VariablePitch,
    GnuBootstrapLispFaceId::VariablePitchText,
    GnuBootstrapLispFaceId::Shadow,
    GnuBootstrapLispFaceId::Link,
    GnuBootstrapLispFaceId::LinkVisited,
    GnuBootstrapLispFaceId::Highlight,
    GnuBootstrapLispFaceId::Region,
    GnuBootstrapLispFaceId::SecondarySelection,
    GnuBootstrapLispFaceId::TrailingWhitespace,
    GnuBootstrapLispFaceId::LineNumber,
    GnuBootstrapLispFaceId::LineNumberCurrentLine,
    GnuBootstrapLispFaceId::LineNumberMajorTick,
    GnuBootstrapLispFaceId::LineNumberMinorTick,
    GnuBootstrapLispFaceId::FillColumnIndicator,
    GnuBootstrapLispFaceId::EscapeGlyph,
    GnuBootstrapLispFaceId::Homoglyph,
    GnuBootstrapLispFaceId::NobreakSpace,
    GnuBootstrapLispFaceId::NobreakHyphen,
    GnuBootstrapLispFaceId::ModeLine,
    GnuBootstrapLispFaceId::ModeLineActive,
    GnuBootstrapLispFaceId::ModeLineInactive,
    GnuBootstrapLispFaceId::ModeLineHighlight,
    GnuBootstrapLispFaceId::ModeLineEmphasis,
    GnuBootstrapLispFaceId::ModeLineBufferId,
    GnuBootstrapLispFaceId::HeaderLine,
    GnuBootstrapLispFaceId::HeaderLineHighlight,
    GnuBootstrapLispFaceId::HeaderLineActive,
    GnuBootstrapLispFaceId::HeaderLineInactive,
    GnuBootstrapLispFaceId::VerticalBorder,
    GnuBootstrapLispFaceId::WindowDivider,
    GnuBootstrapLispFaceId::WindowDividerFirstPixel,
    GnuBootstrapLispFaceId::WindowDividerLastPixel,
    GnuBootstrapLispFaceId::InternalBorder,
    GnuBootstrapLispFaceId::ChildFrameBorder,
    GnuBootstrapLispFaceId::MinibufferPrompt,
    GnuBootstrapLispFaceId::Margin,
    GnuBootstrapLispFaceId::Fringe,
    GnuBootstrapLispFaceId::ScrollBar,
    GnuBootstrapLispFaceId::Border,
    GnuBootstrapLispFaceId::Cursor,
    GnuBootstrapLispFaceId::Mouse,
    GnuBootstrapLispFaceId::ToolBar,
    GnuBootstrapLispFaceId::TabBar,
    GnuBootstrapLispFaceId::TabLine,
    GnuBootstrapLispFaceId::TabLineActive,
    GnuBootstrapLispFaceId::TabLineInactive,
    GnuBootstrapLispFaceId::Menu,
    GnuBootstrapLispFaceId::HelpArgumentName,
    GnuBootstrapLispFaceId::HelpKeyBinding,
    GnuBootstrapLispFaceId::GlyphlessChar,
    GnuBootstrapLispFaceId::Error,
    GnuBootstrapLispFaceId::Warning,
    GnuBootstrapLispFaceId::Success,
    GnuBootstrapLispFaceId::ReadMultipleChoiceFace,
    GnuBootstrapLispFaceId::TtyMenuEnabledFace,
    GnuBootstrapLispFaceId::TtyMenuDisabledFace,
    GnuBootstrapLispFaceId::TtyMenuSelectedFace,
    GnuBootstrapLispFaceId::ShowParenMatch,
    GnuBootstrapLispFaceId::ShowParenMatchExpression,
    GnuBootstrapLispFaceId::ShowParenMismatch,
    GnuBootstrapLispFaceId::Button,
    GnuBootstrapLispFaceId::AbbrevTableName,
    GnuBootstrapLispFaceId::HelpForHelpHeader,
    GnuBootstrapLispFaceId::ConfusinglyReordered,
    GnuBootstrapLispFaceId::NextError,
    GnuBootstrapLispFaceId::NextErrorMessage,
    GnuBootstrapLispFaceId::SeparatorLine,
    GnuBootstrapLispFaceId::BlinkMatchingParenOffscreen,
    GnuBootstrapLispFaceId::CompletionsGroupTitle,
    GnuBootstrapLispFaceId::CompletionsGroupSeparator,
    GnuBootstrapLispFaceId::CompletionsAnnotations,
    GnuBootstrapLispFaceId::CompletionsHighlight,
    GnuBootstrapLispFaceId::CompletionsFirstDifference,
    GnuBootstrapLispFaceId::CompletionsCommonPart,
    GnuBootstrapLispFaceId::MinibufferNonselected,
    GnuBootstrapLispFaceId::FontLockCommentFace,
    GnuBootstrapLispFaceId::FontLockCommentDelimiterFace,
    GnuBootstrapLispFaceId::FontLockStringFace,
    GnuBootstrapLispFaceId::FontLockDocFace,
    GnuBootstrapLispFaceId::FontLockDocMarkupFace,
    GnuBootstrapLispFaceId::FontLockKeywordFace,
    GnuBootstrapLispFaceId::FontLockBuiltinFace,
    GnuBootstrapLispFaceId::FontLockFunctionNameFace,
    GnuBootstrapLispFaceId::FontLockFunctionCallFace,
    GnuBootstrapLispFaceId::FontLockVariableNameFace,
    GnuBootstrapLispFaceId::FontLockVariableUseFace,
    GnuBootstrapLispFaceId::FontLockTypeFace,
    GnuBootstrapLispFaceId::FontLockConstantFace,
    GnuBootstrapLispFaceId::FontLockWarningFace,
    GnuBootstrapLispFaceId::FontLockNegationCharFace,
    GnuBootstrapLispFaceId::FontLockPreprocessorFace,
    GnuBootstrapLispFaceId::FontLockRegexpFace,
    GnuBootstrapLispFaceId::FontLockRegexpGroupingBackslash,
    GnuBootstrapLispFaceId::FontLockRegexpGroupingConstruct,
    GnuBootstrapLispFaceId::FontLockEscapeFace,
    GnuBootstrapLispFaceId::FontLockNumberFace,
    GnuBootstrapLispFaceId::FontLockOperatorFace,
    GnuBootstrapLispFaceId::FontLockPropertyNameFace,
    GnuBootstrapLispFaceId::FontLockPropertyUseFace,
    GnuBootstrapLispFaceId::FontLockPunctuationFace,
    GnuBootstrapLispFaceId::FontLockBracketFace,
    GnuBootstrapLispFaceId::FontLockDelimiterFace,
    GnuBootstrapLispFaceId::FontLockMiscPunctuationFace,
    GnuBootstrapLispFaceId::MouseDragAndDropRegion,
    GnuBootstrapLispFaceId::Isearch,
    GnuBootstrapLispFaceId::IsearchFail,
    GnuBootstrapLispFaceId::LazyHighlight,
    GnuBootstrapLispFaceId::IsearchGroup1,
    GnuBootstrapLispFaceId::IsearchGroup2,
    GnuBootstrapLispFaceId::FileNameShadow,
    GnuBootstrapLispFaceId::TabBarTab,
    GnuBootstrapLispFaceId::TabBarTabInactive,
    GnuBootstrapLispFaceId::TabBarTabGroupCurrent,
    GnuBootstrapLispFaceId::TabBarTabGroupInactive,
    GnuBootstrapLispFaceId::TabBarTabUngrouped,
    GnuBootstrapLispFaceId::TabBarTabHighlight,
    GnuBootstrapLispFaceId::QueryReplace,
    GnuBootstrapLispFaceId::Match,
    GnuBootstrapLispFaceId::TabulatedListFakeHeader,
    GnuBootstrapLispFaceId::BufferMenuBuffer,
    GnuBootstrapLispFaceId::ElispSymbolAtMouse,
    GnuBootstrapLispFaceId::ElispFreeVariable,
    GnuBootstrapLispFaceId::ElispSpecialVariableDeclaration,
    GnuBootstrapLispFaceId::ElispCondition,
    GnuBootstrapLispFaceId::ElispMajorModeName,
    GnuBootstrapLispFaceId::ElispFace,
    GnuBootstrapLispFaceId::ElispSymbolRole,
    GnuBootstrapLispFaceId::ElispSymbolRoleDefinition,
    GnuBootstrapLispFaceId::ElispFunction,
    GnuBootstrapLispFaceId::ElispNonLocalExit,
    GnuBootstrapLispFaceId::ElispUnknownCall,
    GnuBootstrapLispFaceId::ElispMacro,
    GnuBootstrapLispFaceId::ElispSpecialForm,
    GnuBootstrapLispFaceId::ElispThrowTag,
    GnuBootstrapLispFaceId::ElispFeature,
    GnuBootstrapLispFaceId::ElispRx,
    GnuBootstrapLispFaceId::ElispTheme,
    GnuBootstrapLispFaceId::ElispBindingVariable,
    GnuBootstrapLispFaceId::ElispBoundVariable,
    GnuBootstrapLispFaceId::ElispShadowingVariable,
    GnuBootstrapLispFaceId::ElispShadowedVariable,
    GnuBootstrapLispFaceId::ElispVariableAtPoint,
    GnuBootstrapLispFaceId::ElispWarningType,
    GnuBootstrapLispFaceId::ElispFunctionPropertyDeclaration,
    GnuBootstrapLispFaceId::ElispThing,
    GnuBootstrapLispFaceId::ElispSlot,
    GnuBootstrapLispFaceId::ElispWidgetType,
    GnuBootstrapLispFaceId::ElispType,
    GnuBootstrapLispFaceId::ElispGroup,
    GnuBootstrapLispFaceId::ElispNnooBackend,
    GnuBootstrapLispFaceId::ElispAmpersand,
    GnuBootstrapLispFaceId::ElispConstant,
    GnuBootstrapLispFaceId::ElispDefun,
    GnuBootstrapLispFaceId::ElispDefmacro,
    GnuBootstrapLispFaceId::ElispDefvar,
    GnuBootstrapLispFaceId::ElispDefface,
    GnuBootstrapLispFaceId::ElispIcon,
    GnuBootstrapLispFaceId::ElispDeficon,
    GnuBootstrapLispFaceId::ElispOclosure,
    GnuBootstrapLispFaceId::ElispDefoclosure,
    GnuBootstrapLispFaceId::ElispCoding,
    GnuBootstrapLispFaceId::ElispDefcoding,
    GnuBootstrapLispFaceId::ElispCharset,
    GnuBootstrapLispFaceId::ElispDefcharset,
    GnuBootstrapLispFaceId::ElispCompletionCategory,
    GnuBootstrapLispFaceId::ElispCompletionCategoryDefinition,
    GnuBootstrapLispFaceId::VcStateBase,
    GnuBootstrapLispFaceId::VcUpToDateState,
    GnuBootstrapLispFaceId::VcNeedsUpdateState,
    GnuBootstrapLispFaceId::VcLockedState,
    GnuBootstrapLispFaceId::VcLocallyAddedState,
    GnuBootstrapLispFaceId::VcConflictState,
    GnuBootstrapLispFaceId::VcRemovedState,
    GnuBootstrapLispFaceId::VcMissingState,
    GnuBootstrapLispFaceId::VcEditedState,
    GnuBootstrapLispFaceId::VcIgnoredState,
    GnuBootstrapLispFaceId::ElispShorthandFontLockFace,
    GnuBootstrapLispFaceId::EldocHighlightFunctionArgument,
    GnuBootstrapLispFaceId::Tooltip,
];

const FIRST_DYNAMIC_FACE_ID: i64 = 186;

impl GnuBootstrapLispFaceId {
    fn from_name(name: &str) -> Option<Self> {
        // `name.parse()` (strum `FromStr`) is a linear scan over all ~183
        // variants.  Doom sets hundreds of mostly-non-builtin faces, so every
        // lookup scanned -- and failed -- the whole list (a startup hot spot).
        // Build the name->variant map once and look up in O(1).
        static BY_NAME: OnceLock<rustc_hash::FxHashMap<&'static str, GnuBootstrapLispFaceId>> =
            OnceLock::new();
        BY_NAME
            .get_or_init(|| {
                GnuBootstrapLispFaceId::iter()
                    .map(|variant| (<&'static str>::from(variant), variant))
                    .collect()
            })
            .get(name)
            .copied()
    }

    fn id(self) -> i64 {
        self.into()
    }

    fn name(self) -> &'static str {
        self.into()
    }
}

fn is_known_lisp_face_name(name: &str) -> bool {
    GnuBootstrapLispFaceId::from_name(name).is_some()
}

fn known_face_id(name: &str) -> Option<i64> {
    GnuBootstrapLispFaceId::from_name(name).map(GnuBootstrapLispFaceId::id)
}

const LISP_FACE_VECTOR_LEN: usize = LFACE_VECTOR_SIZE;

#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(prefix = ":", serialize_all = "kebab-case")]
enum SetFaceAttrAlias {
    Bold,
    Italic,
}

impl SetFaceAttrAlias {
    fn from_keyword(name: &str) -> Option<Self> {
        name.strip_prefix(':')?.parse().ok()
    }

    fn keyword(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SetFaceAttr {
    LFace(LFaceAttr),
    Alias(SetFaceAttrAlias),
}

impl SetFaceAttr {
    fn from_keyword(name: &str) -> Option<Self> {
        LFaceAttr::from_keyword(name)
            .map(Self::LFace)
            .or_else(|| SetFaceAttrAlias::from_keyword(name).map(Self::Alias))
    }
}

fn valid_face_weight_symbol(name: &str) -> bool {
    FontWeight::from_symbol(name).is_some()
}

fn valid_face_slant_symbol(name: &str) -> bool {
    FontSlant::from_symbol(name).is_some()
}

fn valid_face_width_symbol(name: &str) -> bool {
    FontWidth::from_symbol(name).is_some()
}

fn non_empty_lisp_string(value: Value) -> bool {
    value
        .as_lisp_string()
        .is_some_and(|string| !string.is_empty())
}

fn valid_face_underline_value(value: Value) -> bool {
    if value.is_nil() || value == Value::T {
        return true;
    }
    if matches!(value.kind(), ValueKind::String) {
        return non_empty_lisp_string(value);
    }
    if !value.is_cons() {
        return false;
    }

    let mut list = value;
    while list.is_cons() {
        let key = list.cons_car();
        if key.is_nil() {
            break;
        }
        list = list.cons_cdr();
        let val = if list.is_cons() {
            let value = list.cons_car();
            list = list.cons_cdr();
            value
        } else {
            Value::NIL
        };

        if key.is_nil() || (val.is_nil() && !key.is_symbol_named(":position")) {
            return false;
        }
        if key.is_symbol_named(":color")
            && !(val.is_symbol_named("foreground-color") || non_empty_lisp_string(val))
        {
            return false;
        }
        if key.is_symbol_named(":style")
            && val
                .as_symbol_name()
                .and_then(UnderlineStyle::from_symbol)
                .is_none()
        {
            return false;
        }
    }
    true
}

fn valid_box_line_width(value: Value) -> bool {
    if let Some(width) = value.as_fixnum() {
        return width != 0;
    }
    value.is_cons()
        && value.cons_car().as_fixnum().is_some_and(|width| width != 0)
        && value.cons_cdr().as_fixnum().is_some_and(|width| width != 0)
}

fn valid_face_box_value(value: Value) -> bool {
    if value == Value::T || value.is_nil() {
        return true;
    }
    if let Some(width) = value.as_fixnum() {
        return width != 0;
    }
    if matches!(value.kind(), ValueKind::String) {
        return non_empty_lisp_string(value);
    }
    if value.is_cons() && value.cons_car().is_fixnum() && value.cons_cdr().is_fixnum() {
        return true;
    }
    if !value.is_cons() {
        return false;
    }

    let mut list = value;
    while !list.is_nil() {
        if !list.is_cons() {
            return false;
        }
        let key = list.cons_car();
        list = list.cons_cdr();
        if !list.is_cons() {
            return false;
        }
        let val = list.cons_car();
        list = list.cons_cdr();

        if key.is_symbol_named(":line-width") {
            if !valid_box_line_width(val) {
                return false;
            }
        } else if key.is_symbol_named(":color") {
            if !val.is_nil() && !non_empty_lisp_string(val) {
                return false;
            }
        } else if key.is_symbol_named(":style") {
            if !val.is_nil()
                && val
                    .as_symbol_name()
                    .and_then(BoxStyle::from_symbol)
                    .is_none()
            {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}

#[derive(Default)]
struct FaceAttrState {
    selected_created: HashSet<SymId>,
    selected_overrides: HashMap<SymId, HashMap<LFaceAttr, Value>>,
    defaults_overrides: HashMap<SymId, HashMap<LFaceAttr, Value>>,
}

thread_local! {
    static CREATED_LISP_FACES: RefCell<HashSet<SymId>> = RefCell::new(HashSet::default());
    static CREATED_FACE_IDS: RefCell<HashMap<SymId, i64>> = RefCell::new(HashMap::default());
    static NEXT_CREATED_FACE_ID: RefCell<i64> = const { RefCell::new(FIRST_DYNAMIC_FACE_ID) };
    static FACE_ATTR_STATE: RefCell<FaceAttrState> = RefCell::new(FaceAttrState::default());
    /// Generation counter bumped whenever the defined-face set
    /// (`CREATED_LISP_FACES`) changes.  Keys `FACE_NAME_LIST_CACHE`.
    static FACE_SET_GENERATION: Cell<u64> = const { Cell::new(0) };
    /// Cached sorted face-name list, valid while `FACE_SET_GENERATION` is
    /// unchanged.  Doom calls `face-list` and seeds face tables hundreds of
    /// times during startup with an unchanging face set; recomputing the sort
    /// (with a per-comparison `face_id_for_name`) each time dominated the face
    /// path in the startup profile.
    static FACE_NAME_LIST_CACHE: RefCell<Option<(u64, Rc<[String]>)>> = const { RefCell::new(None) };
}

/// Invalidate the cached face-name list after the defined-face set changes.
fn bump_face_set_generation() {
    FACE_SET_GENERATION.with(|generation| generation.set(generation.get().wrapping_add(1)));
}

fn face_symbol_id(name: &str) -> SymId {
    intern(name)
}

fn face_attr_id(name: &str) -> SymId {
    intern(name)
}

pub(crate) fn clear_font_cache_state() {
    CREATED_LISP_FACES.with(|slot| slot.borrow_mut().clear());
    CREATED_FACE_IDS.with(|slot| slot.borrow_mut().clear());
    NEXT_CREATED_FACE_ID.with(|slot| *slot.borrow_mut() = FIRST_DYNAMIC_FACE_ID);
    FACE_ATTR_STATE.with(|slot| *slot.borrow_mut() = FaceAttrState::default());
    bump_face_set_generation();
}

/// Collect GC roots from face attribute overrides.
pub(crate) fn collect_font_gc_roots(roots: &mut Vec<Value>) {
    FACE_ATTR_STATE.with(|slot| {
        let state = slot.borrow();
        for attrs in state.selected_overrides.values() {
            roots.extend(attrs.values().copied());
        }
        for attrs in state.defaults_overrides.values() {
            roots.extend(attrs.values().copied());
        }
    });
}

fn is_created_lisp_face(name: &str) -> bool {
    is_created_lisp_face_id(face_symbol_id(name))
}

fn is_created_lisp_face_id(id: SymId) -> bool {
    CREATED_LISP_FACES.with(|slot| slot.borrow().contains(&id))
}

/// Restore the `CREATED_LISP_FACES` set from an evaluator's face table.
/// Called after pdump load to re-populate the thread-local face name set
/// that was lost during serialization.
pub(crate) fn restore_created_faces_from_table(face_names: &[String]) {
    CREATED_LISP_FACES.with(|slot| {
        let mut set = slot.borrow_mut();
        for name in face_names {
            if !is_known_lisp_face_name(name) {
                set.insert(face_symbol_id(name));
            }
        }
    });
    bump_face_set_generation();
}

fn mark_created_lisp_face(name: &str) {
    mark_created_lisp_face_id(face_symbol_id(name), name);
}

fn mark_created_lisp_face_id(id: SymId, name: &str) {
    let inserted = CREATED_LISP_FACES.with(|slot| slot.borrow_mut().insert(id));
    if inserted {
        ensure_dynamic_face_id(name);
        bump_face_set_generation();
    }
}

pub(crate) fn ensure_lisp_face_id_property(
    eval: &mut super::eval::Context,
    face_name: &str,
) -> Result<(), Flow> {
    ensure_dynamic_face_id(face_name);
    if let Some(face_id) = face_id_for_name(face_name) {
        eval.obarray_mut()
            .put_property(face_name, "face", Value::fixnum(face_id))?;
    }
    Ok(())
}

fn ensure_dynamic_face_id(name: &str) {
    if known_face_id(name).is_some() {
        return;
    }
    let face = face_symbol_id(name);
    CREATED_FACE_IDS.with(|slot| {
        let mut ids = slot.borrow_mut();
        if ids.contains_key(&face) {
            return;
        }
        NEXT_CREATED_FACE_ID.with(|next_slot| {
            let mut next = next_slot.borrow_mut();
            ids.insert(face, *next);
            *next += 1;
        });
    });
}

fn dynamic_face_id(name: &str) -> Option<i64> {
    CREATED_FACE_IDS.with(|slot| slot.borrow().get(&face_symbol_id(name)).copied())
}

pub(crate) fn face_id_for_name(name: &str) -> Option<i64> {
    if let Some(id) = known_face_id(name) {
        return Some(id);
    }
    if is_known_lisp_face_name(name) {
        ensure_dynamic_face_id(name);
    }
    dynamic_face_id(name)
}

pub(crate) fn all_defined_face_names_sorted_by_id_desc() -> Rc<[String]> {
    let generation = FACE_SET_GENERATION.with(|generation| generation.get());
    if let Some(cached) = FACE_NAME_LIST_CACHE.with(|cache| {
        cache
            .borrow()
            .as_ref()
            .filter(|(cached_generation, _)| *cached_generation == generation)
            .map(|(_, names)| Rc::clone(names))
    }) {
        return cached;
    }

    let names: Rc<[String]> = Rc::from(compute_face_names_sorted_by_id_desc());
    FACE_NAME_LIST_CACHE.with(|cache| {
        *cache.borrow_mut() = Some((generation, Rc::clone(&names)));
    });
    names
}

fn compute_face_names_sorted_by_id_desc() -> Vec<String> {
    // Dedup by interned symbol id (O(n)) rather than a linear string scan
    // (O(n^2)).  Bootstrap and created faces share the global obarray, so equal
    // names map to the same `SymId`.
    let mut seen: HashSet<SymId> = HashSet::default();
    let mut names: Vec<String> = Vec::new();
    for face in GNU_BOOTSTRAP_LISP_FACES.iter() {
        let name = face.name();
        if seen.insert(face_symbol_id(name)) {
            names.push(name.to_string());
        }
    }
    CREATED_LISP_FACES.with(|slot| {
        for symbol in slot.borrow().iter() {
            if seen.insert(*symbol) {
                names.push(resolve_sym(*symbol).to_string());
            }
        }
    });
    // Decorate-sort-undecorate: resolve each face id once (O(n)) instead of
    // inside the comparator (O(n log n) `face_id_for_name` lookups, which
    // dominated the face path in the startup profile).
    let mut keyed: Vec<(i64, String)> = names
        .into_iter()
        .map(|name| (face_id_for_name(&name).unwrap_or(i64::MAX), name))
        .collect();
    keyed.sort_by(|(left_id, left_name), (right_id, right_name)| {
        right_id
            .cmp(left_id)
            .then_with(|| left_name.cmp(right_name))
    });
    keyed.into_iter().map(|(_, name)| name).collect()
}

fn is_selected_created_lisp_face(name: &str) -> bool {
    is_selected_created_lisp_face_id(face_symbol_id(name))
}

fn is_selected_created_lisp_face_id(id: SymId) -> bool {
    FACE_ATTR_STATE.with(|slot| slot.borrow().selected_created.contains(&id))
}

fn mark_selected_created_lisp_face(name: &str) {
    mark_selected_created_lisp_face_id(face_symbol_id(name));
}

fn mark_selected_created_lisp_face_id(id: SymId) {
    FACE_ATTR_STATE.with(|slot| {
        slot.borrow_mut().selected_created.insert(id);
    });
}

fn face_exists_for_domain(name: &str, defaults_frame: bool) -> bool {
    face_exists_for_domain_id(face_symbol_id(name), name, defaults_frame)
}

/// [`face_exists_for_domain`] for callers that already hold the face's
/// interned id (the attribute setter runs this several times per call; the
/// registries are id-keyed, so only the static known-name map needs `name`).
fn face_exists_for_domain_id(id: SymId, name: &str, defaults_frame: bool) -> bool {
    if is_known_lisp_face_name(name) {
        return true;
    }
    // A face created via defface/internal-make-lisp-face exists for all
    // domains. GNU Emacs uses a single hash table for face lookup —
    // there is no distinction between "defaults" and "selected" existence.
    if is_created_lisp_face_id(id) {
        return true;
    }
    if !defaults_frame {
        is_selected_created_lisp_face_id(id)
    } else {
        false
    }
}

fn get_face_override(face_name: &str, attr: LFaceAttr, defaults_frame: bool) -> Option<Value> {
    let face = face_symbol_id(face_name);
    FACE_ATTR_STATE.with(|slot| {
        let state = slot.borrow();
        let map = if defaults_frame {
            &state.defaults_overrides
        } else {
            &state.selected_overrides
        };
        map.get(&face).and_then(|attrs| attrs.get(&attr)).copied()
    })
}

fn set_face_override(face_name: &str, attr: LFaceAttr, value: Value, defaults_frame: bool) {
    set_face_override_id(face_symbol_id(face_name), attr, value, defaults_frame);
}

fn set_face_override_id(face: SymId, attr: LFaceAttr, value: Value, defaults_frame: bool) {
    FACE_ATTR_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let map = if defaults_frame {
            &mut state.defaults_overrides
        } else {
            &mut state.selected_overrides
        };
        map.entry(face).or_default().insert(attr, value);
    });
}

fn clear_face_overrides(face_name: &str, defaults_frame: bool) {
    let face = face_symbol_id(face_name);
    FACE_ATTR_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        if defaults_frame {
            state.defaults_overrides.remove(&face);
        } else {
            state.selected_overrides.remove(&face);
        }
    });
}

pub(crate) fn clear_created_lisp_face(name: &str) {
    let face = face_symbol_id(name);
    CREATED_LISP_FACES.with(|slot| {
        slot.borrow_mut().remove(&face);
    });
    FACE_ATTR_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        state.selected_created.remove(&face);
        state.defaults_overrides.remove(&face);
        state.selected_overrides.remove(&face);
    });
}

fn merge_defaults_overrides_into_selected(face_name: &str) {
    let face = face_symbol_id(face_name);
    FACE_ATTR_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let defaults = state.defaults_overrides.get(&face).cloned();
        if let Some(attrs) = defaults {
            let selected = state.selected_overrides.entry(face).or_default();
            for (attr, value) in attrs {
                if value.is_symbol_named("unspecified") || value.is_symbol_named("relative") {
                    continue;
                }
                selected.insert(attr, value);
            }
        }
    });
}

fn symbol_name_for_face_value(face: &Value) -> Option<String> {
    match face.kind() {
        ValueKind::Nil => Some("nil".to_string()),
        ValueKind::T => Some("t".to_string()),
        ValueKind::Symbol(id) => Some(resolve_sym(id).to_owned()),
        _ => None,
    }
}

fn require_symbol_face_name(face: &Value) -> Result<String, Flow> {
    symbol_name_for_face_value(face).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *face],
        )
    })
}

/// An interned face name after following every `face-alias` edge.
///
/// Keeping this distinct from an arbitrary Lisp `Value` prevents face-table
/// callers from accidentally looking up an unresolved alias.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ResolvedFaceName(SymId);

impl ResolvedFaceName {
    fn from_symbol(value: Value) -> Option<Self> {
        value.as_symbol_id().map(Self)
    }

    fn symbol(self) -> Value {
        Value::from_sym_id(self.0)
    }

    fn name(self) -> &'static str {
        resolve_sym(self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResolvedFaceDesignator {
    Symbol(ResolvedFaceName),
    String(ResolvedFaceName),
    Other(Value),
}

impl ResolvedFaceDesignator {
    fn name(self) -> Option<ResolvedFaceName> {
        match self {
            Self::Symbol(name) | Self::String(name) => Some(name),
            Self::Other(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaceDesignatorKind {
    Symbol,
    String,
}

impl FaceDesignatorKind {
    fn resolved(self, name: ResolvedFaceName) -> ResolvedFaceDesignator {
        match self {
            Self::Symbol => ResolvedFaceDesignator::Symbol(name),
            Self::String => ResolvedFaceDesignator::String(name),
        }
    }
}

/// GNU's `resolve_face_name` uses two different cycle contracts: predicates and
/// attribute access signal, while create-on-miss paths fall back to `default`.
/// Make callers choose instead of hiding that semantic difference in a bool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaceAliasCyclePolicy {
    Signal,
    UseDefault,
}

fn face_alias_target(
    eval: &super::eval::Context,
    face: ResolvedFaceName,
) -> Option<ResolvedFaceName> {
    if face.symbol().is_nil() {
        return None;
    }
    let target = eval.obarray().get_property(face.name(), "face-alias")?;
    if target.is_nil() {
        return None;
    }
    ResolvedFaceName::from_symbol(target)
}

/// Follow `face-alias` properties exactly like GNU xfaces.c
/// `resolve_face_name`, including constant-space cycle detection.
fn resolve_face_designator(
    eval: &super::eval::Context,
    face: Value,
    cycle_policy: FaceAliasCyclePolicy,
) -> Result<ResolvedFaceDesignator, Flow> {
    let (kind, origin) = match face.kind() {
        ValueKind::String => {
            let name = font_string_text(&face).expect("checked string");
            (
                FaceDesignatorKind::String,
                ResolvedFaceName(
                    Value::symbol(&name)
                        .as_symbol_id()
                        .expect("interned face name must be a symbol"),
                ),
            )
        }
        _ => {
            let Some(name) = ResolvedFaceName::from_symbol(face) else {
                return Ok(ResolvedFaceDesignator::Other(face));
            };
            (FaceDesignatorKind::Symbol, name)
        }
    };

    let mut tortoise = origin;
    let mut hare = origin;
    loop {
        let face_name = hare;
        let Some(first_hop) = face_alias_target(eval, hare) else {
            return Ok(kind.resolved(face_name));
        };

        let face_name = first_hop;
        let Some(second_hop) = face_alias_target(eval, first_hop) else {
            return Ok(kind.resolved(face_name));
        };

        hare = second_hop;
        tortoise = face_alias_target(eval, tortoise)
            .expect("hare cannot advance twice unless tortoise can advance once");
        if hare == tortoise {
            return match cycle_policy {
                FaceAliasCyclePolicy::Signal => {
                    Err(signal(LispCondition::CircularList, vec![origin.symbol()]))
                }
                FaceAliasCyclePolicy::UseDefault => Ok(kind.resolved(ResolvedFaceName(
                    Value::symbol("default")
                        .as_symbol_id()
                        .expect("default must be an interned symbol"),
                ))),
            };
        }
    }
}

fn known_resolved_face_name(resolved: ResolvedFaceDesignator) -> Option<ResolvedFaceName> {
    let name = resolved.name()?;
    if is_known_lisp_face_name(name.name()) || is_created_lisp_face(name.name()) {
        Some(name)
    } else {
        None
    }
}

fn resolve_copy_source_face_symbol(
    eval: &super::eval::Context,
    face: &Value,
) -> Result<String, Flow> {
    let _ = require_symbol_face_name(face)?;
    let name = resolve_face_designator(eval, *face, FaceAliasCyclePolicy::Signal)?
        .name()
        .expect("a required symbol resolves to a named face");
    if is_known_lisp_face_name(name.name()) || is_created_lisp_face(name.name()) {
        return Ok(name.name().to_owned());
    }
    Err(invalid_face_error(*face))
}

fn resolve_face_name_for_domain(
    eval: &super::eval::Context,
    face: &Value,
    defaults_frame: bool,
) -> Result<String, Flow> {
    match resolve_face_designator(eval, *face, FaceAliasCyclePolicy::Signal)? {
        ResolvedFaceDesignator::String(name) => {
            if face_exists_for_domain(name.name(), defaults_frame) {
                Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("symbolp"), *face],
                ))
            } else {
                Err(signal(
                    "error",
                    vec![Value::string("Invalid face"), Value::symbol(name.name())],
                ))
            }
        }
        ResolvedFaceDesignator::Symbol(name) => {
            if face_exists_for_domain(name.name(), defaults_frame) {
                Ok(name.name().to_owned())
            } else {
                Err(invalid_face_error(*face))
            }
        }
        ResolvedFaceDesignator::Other(_) => Err(invalid_face_error(*face)),
    }
}

fn resolve_face_name_for_merge(eval: &super::eval::Context, face: &Value) -> Result<String, Flow> {
    match resolve_face_designator(eval, *face, FaceAliasCyclePolicy::Signal)? {
        ResolvedFaceDesignator::String(name) => {
            if face_exists_for_domain(name.name(), true) {
                Ok(name.name().to_owned())
            } else {
                Err(signal(
                    "error",
                    vec![Value::string("Invalid face"), Value::symbol(name.name())],
                ))
            }
        }
        ResolvedFaceDesignator::Symbol(name) => {
            if face_exists_for_domain(name.name(), true) {
                Ok(name.name().to_owned())
            } else {
                Err(invalid_face_error(*face))
            }
        }
        ResolvedFaceDesignator::Other(_) => Err(invalid_face_error(*face)),
    }
}

fn invalid_face_error(face: Value) -> Flow {
    let mut data = vec![Value::string("Invalid face")];
    if let Some(items) = list_to_vec(&face) {
        data.extend(items);
    } else {
        data.push(face);
    }
    signal("error", data)
}

/// The `unspecified' symbol used to fill empty Lisp face slots.  GNU keeps it
/// as the staticpro'd `Qunspecified' and never re-interns it; cache the
/// interned `SymId' once so realising a face does not re-intern "unspecified"
/// for every slot.  `make_lisp_face_vector' fills ~30 slots and Doom realises
/// hundreds of faces, so this was ~5% of startup CPU.  Safe at runtime: the
/// cache is first populated after the pdump is loaded (same pattern as
/// `cached_symbol_id!' in eval.rs).
fn unspecified_face_symbol() -> Value {
    static ID: OnceLock<SymId> = OnceLock::new();
    Value::from_sym_id(*ID.get_or_init(|| intern("unspecified")))
}

/// The leading `face' tag symbol stored in slot 0 of a Lisp face vector.
fn face_tag_symbol() -> Value {
    static ID: OnceLock<SymId> = OnceLock::new();
    Value::from_sym_id(*ID.get_or_init(|| intern("face")))
}

pub(crate) fn make_lisp_face_vector() -> Value {
    let unspecified = unspecified_face_symbol();
    let mut values = Vec::with_capacity(LISP_FACE_VECTOR_LEN);
    values.push(face_tag_symbol());
    values.extend((1..LISP_FACE_VECTOR_LEN).map(|_| unspecified));
    Value::vector(values)
}

fn reset_lisp_face_vector(vector: Value) {
    let unspecified = unspecified_face_symbol();
    let _ = vector.with_vector_data_mut(|slots| {
        if slots.len() != LISP_FACE_VECTOR_LEN {
            *slots = vec![unspecified; LISP_FACE_VECTOR_LEN];
        }
        store_value_atomic(&mut slots[0], face_tag_symbol());
        for slot in slots.iter_mut().take(LISP_FACE_VECTOR_LEN).skip(1) {
            store_value_atomic(slot, unspecified);
        }
    });
}

fn copy_lisp_face_vector_slots(from: Value, to: Value) {
    let Some(source) = from.as_vector_data() else {
        return;
    };
    let _ = to.replace_vector_data(source.clone());
}

fn lisp_face_vector_attr(vector: Value, attr: LFaceAttr) -> Option<Value> {
    vector
        .as_vector_data()
        .and_then(|slots| slots.get(attr as usize).copied())
}

pub(crate) fn set_lisp_face_vector_attr(vector: Value, attr: LFaceAttr, value: Value) {
    let _ = vector.set_vector_slot(attr as usize, value);
}

fn set_lisp_face_vector_attr_with_font_derivatives(
    face_name: &str,
    vector: Value,
    attr: LFaceAttr,
    attr_value: Value,
    font_derivation_value: Value,
) -> Result<(), Flow> {
    set_lisp_face_vector_attr(vector, attr, attr_value);
    if attr == LFaceAttr::Font && !is_reset_like_face_attr_value(&attr_value) {
        for (derived_attr, derived_value) in
            derived_face_attrs_from_font_value(&font_derivation_value)
        {
            let (canonical_attr, canonical_value) = normalize_face_attr_for_set(
                face_name,
                SetFaceAttr::LFace(derived_attr),
                derived_value,
            )?;
            set_lisp_face_vector_attr(vector, canonical_attr, canonical_value);
        }
    }
    Ok(())
}

fn sync_face_overrides_from_lisp_face_vector(face_name: &str, vector: Value, defaults_frame: bool) {
    clear_face_overrides(face_name, defaults_frame);
    let Some(slots) = vector.as_vector_data() else {
        return;
    };
    for attr in LFACE_ATTRS {
        let value = slots
            .get(attr as usize)
            .copied()
            .unwrap_or_else(|| Value::symbol("unspecified"));
        if !value.is_symbol_named("unspecified") {
            set_face_override(face_name, attr, value, defaults_frame);
        }
    }
}

fn make_lisp_face_vector_for_domain(face_name: &str, defaults_frame: bool) -> Value {
    let mut values = Vec::with_capacity(LISP_FACE_VECTOR_LEN);
    values.push(Value::symbol("face"));
    values.extend(
        LFACE_ATTRS
            .iter()
            .map(|attr| lisp_face_attribute_value(face_name, *attr, defaults_frame)),
    );
    Value::vector(values)
}

pub(crate) fn make_lisp_face_vector_for_frame(face_name: &str) -> Value {
    make_lisp_face_vector_for_domain(face_name, false)
}

fn face_hash_entry_lisp_vector(entry: Value) -> Option<Value> {
    if entry.is_vector() {
        Some(entry)
    } else if entry.is_cons() {
        let vector = entry.cons_cdr();
        vector.is_vector().then_some(vector)
    } else {
        None
    }
}

fn runtime_unspecified_lisp_face_attr(attr: LFaceAttr, value: Value) -> bool {
    value.is_symbol_named("unspecified")
        || value.is_symbol_named(":ignore-defface")
        || value.is_symbol_named("reset")
        || (attr == LFaceAttr::Foreground && value.as_utf8_str() == Some("unspecified-fg"))
        || (attr == LFaceAttr::Background && value.as_utf8_str() == Some("unspecified-bg"))
}

fn frame_lisp_face_table_entries(
    eval: &super::eval::Context,
    frame_id: FrameId,
) -> Vec<(String, Option<crate::face::LispFaceId>, Value)> {
    let Some(table) = eval
        .frames
        .get(frame_id)
        .map(|frame| frame.face_hash_table())
    else {
        return Vec::new();
    };
    let Some(hash_table) = table.as_hash_table() else {
        return Vec::new();
    };

    hash_table
        .data
        .iter()
        .filter_map(|(key, entry)| match key {
            HashKey::Symbol(symbol) => face_hash_entry_lisp_vector(*entry).map(|vector| {
                let lisp_face_id = entry
                    .is_cons()
                    .then(|| entry.cons_car().as_fixnum())
                    .flatten()
                    .or_else(|| {
                        eval.obarray()
                            .get_property_id(*symbol, intern("face"))
                            .and_then(|value| value.as_fixnum())
                    })
                    .and_then(crate::face::LispFaceId::new);
                (resolve_sym(*symbol).to_string(), lisp_face_id, vector)
            }),
            _ => None,
        })
        .collect()
}

fn frame_parameter_color_or_tty_default(
    eval: &super::eval::Context,
    frame_id: FrameId,
    param: FrameParam,
    tty_default: &str,
) -> Value {
    eval.frames
        .get(frame_id)
        .and_then(|frame| frame.known_parameter(param))
        .filter(|value| value.is_string())
        .unwrap_or_else(|| Value::string(tty_default))
}

fn default_face_has_explicit_font_attr(attr: LFaceAttr) -> bool {
    get_face_override("default", attr, false).is_some()
        || get_face_override("default", attr, true).is_some()
}

pub(crate) fn realize_default_lisp_face_for_frame(
    eval: &mut super::eval::Context,
    frame_id: FrameId,
) {
    let Some(vector) =
        ensure_frame_lisp_face_vector(eval, frame_id, "default", FrameFaceInitial::SelectedBase)
    else {
        return;
    };
    let Some(frame) = eval.frames.get(frame_id) else {
        return;
    };
    let window_system = frame.effective_window_system();

    if window_system.is_none() {
        set_lisp_face_vector_attr(vector, LFaceAttr::Family, Value::string("default"));
        set_lisp_face_vector_attr(vector, LFaceAttr::Foundry, Value::string("default"));
        set_lisp_face_vector_attr(vector, LFaceAttr::Width, Value::symbol("normal"));
        set_lisp_face_vector_attr(vector, LFaceAttr::Height, Value::fixnum(1));
        if lisp_face_vector_attr(vector, LFaceAttr::Weight)
            .is_none_or(|value| runtime_unspecified_lisp_face_attr(LFaceAttr::Weight, value))
        {
            set_lisp_face_vector_attr(vector, LFaceAttr::Weight, Value::symbol("normal"));
        }
        if lisp_face_vector_attr(vector, LFaceAttr::Slant)
            .is_none_or(|value| runtime_unspecified_lisp_face_attr(LFaceAttr::Slant, value))
        {
            set_lisp_face_vector_attr(vector, LFaceAttr::Slant, Value::symbol("normal"));
        }
        if lisp_face_vector_attr(vector, LFaceAttr::Fontset)
            .is_none_or(|value| runtime_unspecified_lisp_face_attr(LFaceAttr::Fontset, value))
        {
            set_lisp_face_vector_attr(vector, LFaceAttr::Fontset, Value::NIL);
        }
    } else {
        for attr in [
            LFaceAttr::Family,
            LFaceAttr::Foundry,
            LFaceAttr::Width,
            LFaceAttr::Height,
            LFaceAttr::Weight,
            LFaceAttr::Slant,
        ] {
            if default_face_has_explicit_font_attr(attr) {
                continue;
            }
            let fallback = live_frame_font_attribute_fallback(eval, frame_id, attr);
            if let Some(value) = fallback {
                set_lisp_face_vector_attr(vector, attr, value);
            }
        }
    }

    for attr in [
        LFaceAttr::Extend,
        LFaceAttr::Underline,
        LFaceAttr::Overline,
        LFaceAttr::StrikeThrough,
        LFaceAttr::Box,
        LFaceAttr::InverseVideo,
        LFaceAttr::Stipple,
    ] {
        if lisp_face_vector_attr(vector, attr)
            .is_none_or(|value| runtime_unspecified_lisp_face_attr(attr, value))
        {
            set_lisp_face_vector_attr(vector, attr, Value::NIL);
        }
    }

    if lisp_face_vector_attr(vector, LFaceAttr::Foreground)
        .is_none_or(|value| runtime_unspecified_lisp_face_attr(LFaceAttr::Foreground, value))
    {
        let value = frame_parameter_color_or_tty_default(
            eval,
            frame_id,
            FrameParam::ForegroundColor,
            "unspecified-fg",
        );
        set_lisp_face_vector_attr(vector, LFaceAttr::Foreground, value);
    }
    if lisp_face_vector_attr(vector, LFaceAttr::Background)
        .is_none_or(|value| runtime_unspecified_lisp_face_attr(LFaceAttr::Background, value))
    {
        let value = frame_parameter_color_or_tty_default(
            eval,
            frame_id,
            FrameParam::BackgroundColor,
            "unspecified-bg",
        );
        set_lisp_face_vector_attr(vector, LFaceAttr::Background, value);
    }
}

/// Palette resolved through the frame terminal's Lisp tty color table,
/// keyed by the color string exactly as it appears in the lface vector.
pub(crate) type TtyColorMap = rustc_hash::FxHashMap<String, crate::face::Color>;

/// The color-realization policy for converting lface color SPECS into
/// renderable colors -- the type-level analogue of GNU's realize step,
/// where the same lface vector realizes differently per frame class
/// (`realize_gui_face` loads X colors, `realize_tty_face` routes names
/// through `map_tty_color`). Threading this instead of an optional map
/// gives every conversion site one `resolve` choke point and makes it
/// impossible to consult the palette without saying which policy is in
/// force.
#[derive(Clone, Copy)]
pub(crate) enum FaceColorResolver<'a> {
    /// GUI frames and Lisp-query table builds: context-free X11/hex parse.
    Standard,
    /// TTY frames: the terminal's registered palette wins, then the
    /// standard parse as GNU's failed-tty_lookup_color fallback.
    TtyPalette(&'a TtyColorMap),
}

impl FaceColorResolver<'_> {
    /// Realize a color SPEC into render-layer pixels under this frame-class
    /// policy — the sole bridge from [`crate::face::SpecifiedColor`] to
    /// [`crate::face::RealizedColor`], mirroring GNU's realize_*_face step.
    fn realize(self, spec: &crate::face::SpecifiedColor) -> Option<crate::face::RealizedColor> {
        use crate::face::{Color, SpecifiedColor};
        match spec {
            SpecifiedColor::Named(s) => {
                let standard = || Color::from_name(s).or_else(|| Color::from_hex(s));
                match self {
                    Self::Standard => standard(),
                    Self::TtyPalette(palette) => palette.get(s.as_str()).copied().or_else(standard),
                }
            }
            SpecifiedColor::Rgb(r, g, b) => Some(Color::rgb(*r, *g, *b)),
            // Frame-default substitution happens before this boundary
            // (realize_default_lisp_face_for_frame rewrites the default
            // face's vector), so these realize to no color and downstream
            // frame defaults apply.
            SpecifiedColor::Unspecified
            | SpecifiedColor::FrameForeground
            | SpecifiedColor::FrameBackground => None,
        }
    }
}

/// GNU `tty_lookup_color` (xfaces.c:1083): resolve one color string through
/// the Lisp `tty-color-desc`, which canonicalizes the name, prefers the
/// palette registered by `xterm-register-default-colors`, and otherwise
/// approximates. Returns None when the machinery is not loaded or cannot
/// resolve the name -- callers then keep the context-free parse, mirroring
/// GNU's "not resolved" fallback rather than signalling.
///
/// `tty-color-desc` answers `(NAME INDEX R G B)` and GNU keeps the INDEX:
/// `tty_lookup_color` stores it as `tty_color->pixel` (xfaces.c:1102) and
/// `map_tty_color` puts it straight into the realized face's colour slot
/// (xfaces.c:6640-6648).  Keep it here for the same reason -- it is the number
/// the terminal writer must emit, and nothing downstream of Lisp can re-derive
/// it, because the palette it was searched in is `tty-color-alist`, per-terminal
/// Lisp data that `tty-color-define` can change.  The RGB is kept alongside for
/// the consumers that must show a colour rather than write one (snapshots,
/// `:distant-foreground` distance, the layout bridge's pixel).
fn tty_color_desc_rgb(
    eval: &mut super::eval::Context,
    name: &str,
    color_cells: i64,
) -> Option<crate::face::Color> {
    if !eval.obarray.fboundp("tty-color-desc") {
        return None;
    }
    let desc = eval
        .funcall_general(Value::symbol("tty-color-desc"), vec![Value::string(name)])
        .ok()?;
    let items = list_to_vec(&desc)?;
    if items.len() < 5 {
        return None;
    }
    let (r, g, b) = (
        items[2].as_fixnum()?,
        items[3].as_fixnum()?,
        items[4].as_fixnum()?,
    );
    // tty-color-alist stores 16-bit components (xterm-rgb-convert-to-16bit).
    let to8 = |v: i64| (v.clamp(0, 65535) / 257) as u8;
    let rgb = crate::face::Color::rgb(to8(r), to8(g), to8(b));
    // GNU checks the INDEX is a number and gives up on the whole descriptor
    // otherwise (`if (! FIXNUMP (XCAR (XCDR (color_desc)))) return false;`,
    // xfaces.c:1098-1099).
    let index = items[1].as_fixnum()?;
    Some(
        match TerminalColor::from_tty_color_desc(index, color_cells) {
            Some(terminal) => rgb.with_terminal(terminal),
            None => return None,
        },
    )
}

/// Collect every foreground/background/distant-foreground color string in the
/// frame's face vectors and resolve each through `tty-color-desc`, exactly as
/// GNU `map_tty_color` does per attribute during `realize_tty_face`. One
/// funcall per unique name per sync -- face sync runs on face changes, not per
/// redisplay frame.
fn build_tty_color_map(eval: &mut super::eval::Context, frame_id: FrameId) -> TtyColorMap {
    let mut names: Vec<String> = Vec::new();
    for (_face_name, _lisp_face_id, vector) in frame_lisp_face_table_entries(eval, frame_id) {
        let note = |s: Option<&str>, names: &mut Vec<String>| {
            if let Some(s) = s
                && !s.is_empty()
                && s != "unspecified-fg"
                && s != "unspecified-bg"
                && !names.iter().any(|n| n == s)
            {
                names.push(s.to_owned());
            }
        };
        for attr in [
            LFaceAttr::Foreground,
            LFaceAttr::Background,
            LFaceAttr::DistantForeground,
        ] {
            let Some(value) = lisp_face_vector_attr(vector, attr) else {
                continue;
            };
            let s = match value.kind() {
                ValueKind::Cons => value.cons_car().as_utf8_str(),
                _ => value.as_utf8_str(),
            };
            note(s, &mut names);
        }
        // GNU map_tty_color also resolves LFACE_UNDERLINE_INDEX: an
        // underline spec carries a color as a bare string or a plist
        // :color entry.
        if let Some(value) = lisp_face_vector_attr(vector, LFaceAttr::Underline) {
            note(value.as_utf8_str(), &mut names);
            if let Some(plist) = list_to_vec(&value) {
                let mut i = 0;
                while i + 1 < plist.len() {
                    if plist[i].as_symbol_name() == Some(":color") {
                        note(
                            plist[i + 1]
                                .as_utf8_str()
                                .or_else(|| plist[i + 1].as_symbol_name()),
                            &mut names,
                        );
                    }
                    i += 2;
                }
            }
        }
    }
    // `tty-color-24bit` keys on `(display-color-cells)` (tty-colors.el:834), so
    // the same number decides whether the INDEX `tty-color-desc` returns is a
    // palette subscript or a packed 24-bit pixel. Read it once per sync, not
    // once per colour.
    let color_cells = crate::emacs_core::terminal::pure::terminal_runtime_color_cells();
    let mut map = TtyColorMap::default();
    for name in names {
        if let Some(color) = tty_color_desc_rgb(eval, &name, color_cells) {
            map.insert(name, color);
        }
    }
    map
}

/// Read `tty-color-alist` -- the terminal's registered palette
/// (lisp/term/tty-colors.el:773-786) -- into the data form the layout engine
/// searches.
///
/// GNU never reads it this way: every C caller goes through `tty-color-desc`.
/// This snapshot exists only for the one realization path that has no evaluator
/// to call it with, and it is the SAME list, so `tty-color-define` moves both.
fn snapshot_tty_color_alist(
    eval: &mut super::eval::Context,
) -> neomacs_display_protocol::TtyPalette {
    use neomacs_display_protocol::{TtyPalette, TtyPaletteEntry};
    if !eval.obarray.fboundp("tty-color-alist") {
        return TtyPalette::default();
    }
    let Ok(alist) = eval.funcall_general(Value::symbol("tty-color-alist"), Vec::new()) else {
        return TtyPalette::default();
    };
    let Some(rows) = list_to_vec(&alist) else {
        return TtyPalette::default();
    };
    // tty-color-alist stores 16-bit components (xterm-rgb-convert-to-16bit),
    // and `tty-color-approximate` compares them shifted down by 8.
    let to8 = |v: i64| (v.clamp(0, 65535) / 257) as u8;
    let entries = rows
        .iter()
        .filter_map(|row| {
            let items = list_to_vec(row)?;
            if items.len() < 2 {
                return None;
            }
            let name = items[0].as_utf8_str()?.to_owned();
            let index = items[1].as_fixnum()?;
            // A row registered without RGB is never a candidate for
            // approximating another colour (lisp/term/tty-colors.el:895-896),
            // but it is still reachable by name.
            let component = |at: usize| items.get(at).and_then(|value| value.as_fixnum());
            let rgb = match (component(2), component(3), component(4)) {
                (Some(r), Some(g), Some(b)) => Some((to8(r), to8(g), to8(b))),
                _ => None,
            };
            Some(TtyPaletteEntry { name, index, rgb })
        })
        .collect();
    TtyPalette::new(
        entries,
        crate::emacs_core::terminal::pure::terminal_runtime_color_cells(),
    )
}

pub(crate) fn runtime_face_from_lisp_face_vector(face_name: &str, vector: Value) -> RuntimeFace {
    runtime_face_from_lisp_face_vector_resolved(face_name, vector, FaceColorResolver::Standard)
}

pub(crate) fn runtime_face_from_lisp_face_vector_resolved(
    face_name: &str,
    vector: Value,
    resolver: FaceColorResolver<'_>,
) -> RuntimeFace {
    let mut face = RuntimeFace::new(face_name);
    for attr in LFACE_ATTRS {
        let Some(value) = lisp_face_vector_attr(vector, attr) else {
            continue;
        };
        if runtime_unspecified_lisp_face_attr(attr, value) {
            continue;
        }
        if let Some(face_attr) = lisp_value_to_face_attr_resolved(attr, value, resolver) {
            face.set_attribute(attr, face_attr);
        }
    }
    face
}

/// Materialize a frame's authoritative Lisp face specifications into an
/// isolated runtime table.  This is a derived value: callers may use it for a
/// Lisp query or install it as redisplay's cache, but must never mutate it as
/// face-definition state.
pub(crate) fn runtime_face_table_from_frame_lisp_faces(
    eval: &super::eval::Context,
    frame_id: FrameId,
    preserve_default_baseline: bool,
) -> crate::face::FaceTable {
    runtime_face_table_from_frame_lisp_faces_resolved(
        eval,
        frame_id,
        preserve_default_baseline,
        FaceColorResolver::Standard,
    )
}

pub(crate) fn runtime_face_table_from_frame_lisp_faces_resolved(
    eval: &super::eval::Context,
    frame_id: FrameId,
    preserve_default_baseline: bool,
    resolver: FaceColorResolver<'_>,
) -> crate::face::FaceTable {
    // Preserve the already-established default face baseline.  In particular,
    // Lisp `font-at` queries retain relative inline heights until actual font
    // realization; replacing the baseline with the frame's concrete default
    // height here would prematurely collapse that semantic distinction.
    let mut table = if preserve_default_baseline {
        eval.face_table.clone()
    } else {
        crate::face::FaceTable::new()
    };
    for (face_name, lisp_face_id, vector) in frame_lisp_face_table_entries(eval, frame_id) {
        if preserve_default_baseline && face_name == "default" {
            continue;
        }
        let runtime_face =
            runtime_face_from_lisp_face_vector_resolved(&face_name, vector, resolver);
        if let Some(lisp_face_id) = lisp_face_id {
            table.define_lisp_face(lisp_face_id, &face_name, runtime_face);
        } else {
            table.define(&face_name, runtime_face);
        }
    }
    table
}

/// Rebuild the display-facing runtime face cache from GNU-shaped frame-local
/// Lisp face vectors.
///
/// GNU stores face definitions as Lisp vectors in `frame->face_hash_table` and
/// realizes renderable `struct face` entries from those vectors during
/// redisplay. Neomacs still has a Rust `FaceTable` for the layout bridge; keep
/// it as a derived cache so redisplay follows the same ownership boundary.
pub(crate) fn sync_runtime_face_table_from_frame_lisp_faces(
    eval: &mut super::eval::Context,
    frame_id: FrameId,
) {
    realize_default_lisp_face_for_frame(eval, frame_id);
    // GNU realize_tty_face resolves every face color name through the tty
    // color table (map_tty_color, xfaces.c:6620); do the same for TTY frames
    // so the renderer paints the registered palette (xterm "white" is
    // 229,229,229, not X11 255,255,255), not rgb.txt.
    let tty = eval
        .frames
        .get(frame_id)
        .is_some_and(|f| f.window_system.is_none());
    let tty_palette = tty.then(|| build_tty_color_map(eval, frame_id));
    let resolver = match &tty_palette {
        Some(palette) => FaceColorResolver::TtyPalette(palette),
        None => FaceColorResolver::Standard,
    };
    eval.face_table =
        runtime_face_table_from_frame_lisp_faces_resolved(eval, frame_id, false, resolver);
    // The palette travels with the table: everything downstream that realizes
    // one more face -- an anonymous attribute plist from a text property, an
    // overlay, or `face-remapping-alist` -- must use the same one.
    if tty {
        let palette = snapshot_tty_color_alist(eval);
        eval.face_table.set_tty_palette(palette);
    }
}

#[derive(Clone, Copy)]
pub(crate) enum FrameFaceInitial {
    Empty,
    SelectedBase,
}

fn ensure_global_lisp_face_vector(
    eval: &mut super::eval::Context,
    face_name: &str,
) -> Option<Value> {
    crate::emacs_core::xfaces::face_new_frame_defaults_vector(eval, face_name)
}

pub(crate) fn lookup_frame_lisp_face_vector(
    eval: &super::eval::Context,
    frame_id: FrameId,
    face_name: &str,
) -> Option<Value> {
    let table = eval.frames.get(frame_id)?.face_hash_table();
    crate::emacs_core::xfaces::lookup_frame_face_hash_entry(table, Value::symbol(face_name))
}

/// Symbol-keyed frame face lookup for callers that already hold the interned
/// face symbol, avoiding the `&str` -> `Value::symbol` re-intern in
/// `lookup_frame_lisp_face_vector`. `key` must be an interned symbol `Value`.
fn lookup_frame_lisp_face_vector_by_symbol(
    eval: &super::eval::Context,
    frame_id: FrameId,
    key: Value,
) -> Option<Value> {
    let table = eval.frames.get(frame_id)?.face_hash_table();
    crate::emacs_core::xfaces::lookup_frame_face_hash_entry(table, key)
}

/// Whether `face_ref` resolves to a named face available on `frame_id`.
///
/// Display code must ask the frame-local Lisp face table, rather than the
/// renderer's derived `FaceTable`: GNU's `merge_named_face` resolves aliases
/// and then consults the frame face hash while walking a `face` property.  Keep
/// that ownership boundary available to non-rendering display operations such
/// as `window-text-pixel-size` too.
fn display_named_face_exists(
    eval: &super::eval::Context,
    frame_id: FrameId,
    face_ref: Value,
) -> bool {
    let Ok(resolved) = resolve_face_designator(eval, face_ref, FaceAliasCyclePolicy::UseDefault)
    else {
        return false;
    };
    let Some(name) = resolved.name() else {
        return false;
    };

    lookup_frame_lisp_face_vector_by_symbol(eval, frame_id, name.symbol()).is_some()
        || known_resolved_face_name(resolved).is_some()
}

/// Return invalid atomic face references nested in a display `face` value, in
/// GNU merge order.
///
/// `merge_face_ref` accepts an atomic named face, a list of face references,
/// or an attribute plist.  Lists are merged from right to left; attribute
/// plists only contain another face reference in `:inherit`.  Keeping that
/// grammar here prevents headless measurement from mistaking a valid plist for
/// one invalid face while preserving the order of GNU's diagnostics.
pub(crate) fn invalid_display_face_references(
    eval: &super::eval::Context,
    frame_id: FrameId,
    face_ref: Value,
) -> Vec<Value> {
    fn collect(
        eval: &super::eval::Context,
        frame_id: FrameId,
        face_ref: Value,
        invalid: &mut Vec<Value>,
    ) {
        if face_ref.is_nil() {
            return;
        }
        if !face_ref.is_cons() {
            if !display_named_face_exists(eval, frame_id, face_ref) {
                invalid.push(face_ref);
            }
            return;
        }

        let first = face_ref.cons_car();
        if first.is_symbol_named("foreground-color") || first.is_symbol_named("background-color") {
            return;
        }

        if first
            .as_symbol_name()
            .is_some_and(|name| name.starts_with(':'))
        {
            let mut plist = face_ref;
            while plist.is_cons() && plist.cons_cdr().is_cons() {
                let keyword = plist.cons_car();
                let value = plist.cons_cdr().cons_car();
                if keyword.is_symbol_named(":inherit") {
                    collect(eval, frame_id, value, invalid);
                }
                plist = plist.cons_cdr().cons_cdr();
            }
            return;
        }

        // Earlier list elements take precedence, so GNU merges and diagnoses
        // the tail before the head.
        collect(eval, frame_id, face_ref.cons_cdr(), invalid);
        collect(eval, frame_id, first, invalid);
    }

    let mut invalid = Vec::new();
    collect(eval, frame_id, face_ref, &mut invalid);
    invalid
}

pub(crate) fn ensure_frame_lisp_face_vector(
    eval: &mut super::eval::Context,
    frame_id: FrameId,
    face_name: &str,
    initial: FrameFaceInitial,
) -> Option<Value> {
    ensure_frame_lisp_face_vector_by_symbol(
        eval,
        frame_id,
        Value::symbol(face_name),
        face_name,
        initial,
    )
}

/// [`ensure_frame_lisp_face_vector`] for callers that already hold the
/// interned face symbol (the attribute setter runs this per frame per call).
fn ensure_frame_lisp_face_vector_by_symbol(
    eval: &mut super::eval::Context,
    frame_id: FrameId,
    face_sym: Value,
    face_name: &str,
    initial: FrameFaceInitial,
) -> Option<Value> {
    if let Some(vector) = lookup_frame_lisp_face_vector_by_symbol(eval, frame_id, face_sym) {
        return Some(vector);
    }
    let vector = match initial {
        FrameFaceInitial::Empty => make_lisp_face_vector(),
        FrameFaceInitial::SelectedBase => make_lisp_face_vector_for_domain(face_name, false),
    };
    let frame = eval.frames.get_mut(frame_id)?;
    crate::emacs_core::xfaces::upsert_frame_face_hash_entry(
        frame.face_hash_table(),
        face_sym,
        vector,
    );
    Some(vector)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FontFaceRealizationTarget {
    NewFrameDefaults { reference_frame: FrameId },
    LiveFrame(FrameId),
}

impl FontFaceRealizationTarget {
    const fn reference_frame(self) -> FrameId {
        match self {
            Self::NewFrameDefaults { reference_frame } | Self::LiveFrame(reference_frame) => {
                reference_frame
            }
        }
    }

    const fn defaults_frame(self) -> bool {
        matches!(self, Self::NewFrameDefaults { .. })
    }
}

/// Mirror GNU's recursive `FRAME == 0` handling as an explicit target list.
/// Each live frame retains its own realization because its display metrics may
/// differ; new-frame defaults use the selected graphical frame as GNU does.
fn font_face_realization_targets(
    eval: &mut super::eval::Context,
    frame_arg: Option<&Value>,
) -> Vec<FontFaceRealizationTarget> {
    let selected = super::window_cmds::ensure_selected_frame_id(eval);
    match frame_arg {
        Some(frame) if frame.is_t() => vec![FontFaceRealizationTarget::NewFrameDefaults {
            reference_frame: selected,
        }],
        Some(frame) if frame.as_fixnum() == Some(0) => {
            let mut live_frames = eval.frames.frame_list();
            if let Some(index) = live_frames
                .iter()
                .position(|frame_id| *frame_id == selected)
            {
                live_frames.swap(0, index);
            }
            let mut targets = Vec::with_capacity(live_frames.len() + 1);
            targets.push(FontFaceRealizationTarget::NewFrameDefaults {
                reference_frame: selected,
            });
            targets.extend(
                live_frames
                    .into_iter()
                    .map(FontFaceRealizationTarget::LiveFrame),
            );
            targets
        }
        Some(frame) if live_frame_designator_in_state(&eval.frames, frame) => {
            let frame_id =
                frame_id_from_designator(frame).expect("live frame designator should decode");
            vec![FontFaceRealizationTarget::LiveFrame(frame_id)]
        }
        None | Some(_) => vec![FontFaceRealizationTarget::LiveFrame(selected)],
    }
}

fn set_realized_font_for_target(
    eval: &mut super::eval::Context,
    target: FontFaceRealizationTarget,
    face_symbol: Value,
    face_name: &str,
    font_value: Value,
) -> Result<(), Flow> {
    let vector = match target {
        FontFaceRealizationTarget::NewFrameDefaults { .. } => {
            ensure_global_lisp_face_vector(eval, face_name)
        }
        FontFaceRealizationTarget::LiveFrame(frame_id) => {
            let initial = if is_known_lisp_face_name(face_name) {
                FrameFaceInitial::SelectedBase
            } else {
                FrameFaceInitial::Empty
            };
            ensure_frame_lisp_face_vector_by_symbol(eval, frame_id, face_symbol, face_name, initial)
        }
    };
    if let Some(vector) = vector {
        set_lisp_face_vector_attr_with_font_derivatives(
            face_name,
            vector,
            LFaceAttr::Font,
            font_value,
            font_value,
        )?;
    }
    Ok(())
}

fn normalize_face_attribute_name(attr: &Value) -> Result<LFaceAttr, Flow> {
    let name = match attr.kind() {
        ValueKind::Symbol(id) => resolve_sym(id),
        ValueKind::Nil => "nil",
        ValueKind::T => "t",
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), *attr],
            ));
        }
    };

    if let Some(attr) = LFaceAttr::from_keyword(name) {
        Ok(attr)
    } else if attr.is_nil() {
        Err(signal(
            "error",
            vec![Value::string("Invalid face attribute name")],
        ))
    } else {
        Err(signal(
            "error",
            vec![Value::string("Invalid face attribute name"), *attr],
        ))
    }
}

fn normalize_set_face_attribute_name(attr: &Value) -> Result<SetFaceAttr, Flow> {
    let name = match attr.kind() {
        ValueKind::Symbol(id) => resolve_sym(id),
        ValueKind::Nil => "nil",
        ValueKind::T => "t",
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), *attr],
            ));
        }
    };

    if let Some(attr) = SetFaceAttr::from_keyword(name) {
        Ok(attr)
    } else if attr.is_nil() {
        Err(signal(
            "error",
            vec![Value::string("Invalid face attribute name")],
        ))
    } else {
        Err(signal(
            "error",
            vec![Value::string("Invalid face attribute name"), *attr],
        ))
    }
}

fn default_face_attribute_value(attr: LFaceAttr) -> Value {
    match attr {
        LFaceAttr::Family | LFaceAttr::Foundry => Value::string("default"),
        LFaceAttr::Height => Value::fixnum(1),
        LFaceAttr::Weight | LFaceAttr::Slant | LFaceAttr::Width => Value::symbol("normal"),
        LFaceAttr::Underline
        | LFaceAttr::Overline
        | LFaceAttr::StrikeThrough
        | LFaceAttr::Box
        | LFaceAttr::InverseVideo
        | LFaceAttr::Stipple
        | LFaceAttr::Inherit
        | LFaceAttr::Extend
        | LFaceAttr::Fontset => Value::NIL,
        LFaceAttr::Foreground => Value::string("unspecified-fg"),
        LFaceAttr::Background => Value::string("unspecified-bg"),
        LFaceAttr::DistantForeground | LFaceAttr::Font => Value::symbol("unspecified"),
    }
}

fn is_reset_like_face_attr_value(value: &Value) -> bool {
    value.as_symbol_id().is_some_and(|id| {
        let s = resolve_sym(id);
        s == "unspecified" || s == ":ignore-defface" || s == "reset"
    })
}

pub(crate) fn derived_face_attrs_from_font_value(value: &Value) -> Vec<(LFaceAttr, Value)> {
    if !is_font(value) {
        return Vec::new();
    }

    let unresolved_pixel_sized_font = is_font_spec(value) || is_font_entity(value);
    let Some(elems) = font_value_fields(value) else {
        return Vec::new();
    };
    let mut derived = Vec::new();

    for (field, attr) in [
        ("family", LFaceAttr::Family),
        ("foundry", LFaceAttr::Foundry),
    ] {
        if let Some(v) = font_vector_get_flexible(&elems, field)
            && let Some(text) = font_value_text(&v)
        {
            derived.push((attr, Value::string(text)));
        }
    }

    for (field, attr) in [
        ("weight", LFaceAttr::Weight),
        ("slant", LFaceAttr::Slant),
        ("width", LFaceAttr::Width),
    ] {
        if let Some(v) = font_vector_get_flexible(&elems, field) {
            derived.push((attr, v));
        }
    }

    if let Some(v) = font_vector_get_flexible(&elems, "height") {
        derived.push((LFaceAttr::Height, v));
    } else if let Some(v) = font_vector_get_flexible(&elems, "size") {
        match v.kind() {
            // An integer selector size is still in pixels. It has no
            // frame-independent face height; the opened font object supplies
            // that after resolution against the target frame.
            ValueKind::Fixnum(_) if unresolved_pixel_sized_font => {}
            // GNU floating selector sizes are points. The xfaces path stores
            // the whole-point value as tenths of a point.
            ValueKind::Float if unresolved_pixel_sized_font && v.xfloat() > 0.0 => {
                derived.push((LFaceAttr::Height, Value::fixnum(10 * (v.xfloat() as i64))));
            }
            _ => derived.push((LFaceAttr::Height, v)),
        }
    }

    derived
}

fn apply_derived_font_face_overrides(
    face_name: &str,
    font_value: &Value,
    defaults_frame: bool,
) -> Result<(), Flow> {
    for (attr_name, attr_value) in derived_face_attrs_from_font_value(font_value) {
        let (canonical_attr, canonical_value) =
            normalize_face_attr_for_set(face_name, SetFaceAttr::LFace(attr_name), attr_value)?;
        set_face_override(face_name, canonical_attr, canonical_value, defaults_frame);
    }
    Ok(())
}

fn lisp_face_attribute_base_value(face: &str, attr: LFaceAttr, defaults_frame: bool) -> Value {
    if defaults_frame {
        return Value::symbol("unspecified");
    }
    if face == "default" {
        return default_face_attribute_value(attr);
    }
    match (face, attr) {
        ("bold", LFaceAttr::Weight) => Value::symbol("bold"),
        ("italic", LFaceAttr::Slant) => Value::symbol("italic"),
        ("underline", LFaceAttr::Underline) => Value::T,
        ("highlight", LFaceAttr::InverseVideo) => Value::T,
        ("region", LFaceAttr::InverseVideo) => Value::T,
        ("mode-line", LFaceAttr::InverseVideo) => Value::T,
        ("mode-line-active", LFaceAttr::Inherit) => Value::symbol("mode-line"),
        ("mode-line-highlight", LFaceAttr::Inherit) => Value::symbol("highlight"),
        ("mode-line-emphasis", LFaceAttr::Weight) => Value::symbol("bold"),
        ("mode-line-buffer-id", LFaceAttr::Weight) => Value::symbol("bold"),
        ("mode-line-inactive", LFaceAttr::Inherit) => Value::symbol("mode-line"),
        ("header-line", LFaceAttr::Inherit) => Value::symbol("mode-line"),
        ("header-line-highlight", LFaceAttr::Inherit) => Value::symbol("mode-line-highlight"),
        ("header-line-active", LFaceAttr::Inherit) => Value::symbol("header-line"),
        ("header-line-inactive", LFaceAttr::Inherit) => Value::symbol("header-line"),
        ("fringe", LFaceAttr::Background) => Value::string("gray"),
        ("cursor", LFaceAttr::Background) => Value::string("white"),
        ("vertical-border", LFaceAttr::Inherit) => Value::symbol("mode-line-inactive"),
        ("tool-bar", LFaceAttr::Foreground) => Value::string("black"),
        ("tool-bar", LFaceAttr::Box) => Value::symbol("t"),
        ("tab-bar", LFaceAttr::Inherit) => Value::symbol("variable-pitch"),
        ("tab-line", LFaceAttr::Inherit) => Value::symbol("variable-pitch"),
        _ => Value::symbol("unspecified"),
    }
}

fn lisp_face_attribute_value(face: &str, attr: LFaceAttr, defaults_frame: bool) -> Value {
    if let Some(value) = get_face_override(face, attr, defaults_frame) {
        return value;
    }
    lisp_face_attribute_base_value(face, attr, defaults_frame)
}

fn resolve_known_face_name_for_compare(
    eval: &super::eval::Context,
    face: &Value,
    defaults_frame: bool,
) -> Result<String, Flow> {
    match resolve_face_designator(eval, *face, FaceAliasCyclePolicy::Signal)? {
        ResolvedFaceDesignator::Symbol(name) | ResolvedFaceDesignator::String(name) => {
            if face_exists_for_domain(name.name(), defaults_frame) {
                Ok(name.name().to_owned())
            } else {
                Err(signal(
                    "error",
                    vec![Value::string("Invalid face"), Value::symbol(name.name())],
                ))
            }
        }
        ResolvedFaceDesignator::Other(_) => Err(invalid_face_error(*face)),
    }
}

fn face_attr_value_name(attr: &Value) -> Result<SymId, Flow> {
    match attr.kind() {
        ValueKind::Symbol(id) => {
            let s = resolve_sym(id);
            if s.starts_with(':') {
                Ok(face_attr_id(s))
            } else {
                Ok(face_attr_id(&format!(":{s}")))
            }
        }
        ValueKind::Nil => Ok(face_attr_id("nil")),
        ValueKind::T => Ok(face_attr_id("t")),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *attr],
        )),
    }
}

fn frame_defaults_flag(frame: Option<&Value>) -> Result<bool, Flow> {
    match frame {
        None => Ok(false),
        Some(v) if v.is_nil() => Ok(false),
        Some(v) if v.is_t() => Ok(true),
        Some(v) if frame_device_designator_p(v) => Ok(false),
        Some(v) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *v],
        )),
    }
}

fn proper_list_to_vec_or_listp_error(value: &Value) -> Result<Vec<Value>, Flow> {
    let mut out = Vec::new();
    let mut cursor = *value;
    loop {
        match cursor.kind() {
            ValueKind::Nil => return Ok(out),
            ValueKind::Cons => {
                let cell_car = cursor.cons_car();
                let cell_cdr = cursor.cons_cdr();
                out.push(cell_car);
                cursor = cell_cdr;
            }
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), cursor],
                ));
            }
        }
    }
}

fn check_non_empty_string(value: &Value, empty_message: &str) -> Result<(), Flow> {
    match value.kind() {
        ValueKind::String => {
            if value
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload")
                .is_empty()
            {
                Err(signal("error", vec![Value::string(empty_message), *value]))
            } else {
                Ok(())
            }
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )),
    }
}

fn symbol_name_or_type_error(value: &Value) -> Result<String, Flow> {
    match value.kind() {
        ValueKind::Nil => Ok("nil".to_string()),
        ValueKind::T => Ok("t".to_string()),
        ValueKind::Symbol(id) => Ok(resolve_sym(id).to_owned()),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *value],
        )),
    }
}

fn normalize_face_attr_for_set(
    face_name: &str,
    attr: SetFaceAttr,
    value: Value,
) -> Result<(LFaceAttr, Value), Flow> {
    normalize_face_attr_for_set_with_eval(None, face_name, attr, value)
}

fn merge_face_height_value(
    eval: Option<&mut super::eval::Context>,
    from: Value,
    to: Value,
    invalid: Value,
) -> Value {
    match from.kind() {
        ValueKind::Fixnum(_) => from,
        ValueKind::Float => match to.kind() {
            ValueKind::Fixnum(height) => Value::fixnum((from.xfloat() * height as f64) as i64),
            ValueKind::Float => Value::make_float(from.xfloat() * to.xfloat()),
            _ if is_reset_like_face_attr_value(&to) => from,
            _ => invalid,
        },
        _ => {
            let Some(eval) = eval else {
                return invalid;
            };
            match eval.funcall_general(from, vec![to]) {
                Ok(result) if !to.is_fixnum() || result.is_fixnum() => result,
                Ok(_) | Err(_) => invalid,
            }
        }
    }
}

fn normalize_face_attr_for_set_with_eval(
    eval: Option<&mut super::eval::Context>,
    face_name: &str,
    attr: SetFaceAttr,
    value: Value,
) -> Result<(LFaceAttr, Value), Flow> {
    let attr_name = match attr {
        SetFaceAttr::LFace(attr) => attr.keyword(),
        SetFaceAttr::Alias(alias) => alias.keyword(),
    };
    let mut normalized = match attr_name {
        ":foreground" | ":background" | ":distant-foreground" if value.is_nil() => {
            Value::symbol("unspecified")
        }
        _ => value,
    };
    let is_reset_like = is_reset_like_face_attr_value(&normalized);

    match attr_name {
        ":family" | ":foundry" => {
            if !is_reset_like {
                match normalized.kind() {
                    ValueKind::String
                        if !normalized
                            .as_lisp_string()
                            .expect("ValueKind::String must carry LispString payload")
                            .is_empty() => {}
                    ValueKind::String => {
                        let msg = if attr_name == ":family" {
                            "Invalid face family"
                        } else {
                            "Invalid face foundry"
                        };
                        return Err(signal("error", vec![Value::string(msg), normalized]));
                    }
                    _ => {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("stringp"), normalized],
                        ));
                    }
                }
            }
        }
        ":height" => {
            if !is_reset_like {
                if face_name == "default" {
                    match normalized.kind() {
                        ValueKind::Fixnum(n) if n > 0 => {}
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
                    match normalized.kind() {
                        ValueKind::Fixnum(n) if n > 0 => {}
                        ValueKind::Float if normalized.xfloat() > 0.0 => {}
                        _ => {
                            let test = merge_face_height_value(
                                eval,
                                normalized,
                                Value::fixnum(10),
                                Value::NIL,
                            );
                            if test.as_int().is_none_or(|n| n <= 0) {
                                return Err(signal(
                                    "error",
                                    vec![
                                        Value::string(
                                            "Face height does not produce a positive integer",
                                        ),
                                        normalized,
                                    ],
                                ));
                            }
                        }
                    }
                }
            }
        }
        ":weight" => {
            if !is_reset_like {
                let sym = symbol_name_or_type_error(&normalized)?;
                if !valid_face_weight_symbol(&sym) {
                    return Err(signal(
                        "error",
                        vec![Value::string("Invalid face weight"), normalized],
                    ));
                }
            }
        }
        ":slant" => {
            if !is_reset_like {
                let sym = symbol_name_or_type_error(&normalized)?;
                if !valid_face_slant_symbol(&sym) {
                    return Err(signal(
                        "error",
                        vec![Value::string("Invalid face slant"), normalized],
                    ));
                }
            }
        }
        ":width" => {
            if !is_reset_like {
                let sym = symbol_name_or_type_error(&normalized)?;
                if !valid_face_width_symbol(&sym) {
                    return Err(signal(
                        "error",
                        vec![Value::string("Invalid face width"), normalized],
                    ));
                }
            }
        }
        ":foreground" | ":background" | ":distant-foreground" => {
            if !is_reset_like {
                // Doom themes and some Emacs themes store colours as cons cells
                // (dark . light).  Resolve to a plain string so downstream
                // consumers (lisp_value_to_face_attr) receive a valid colour.
                if let ValueKind::Cons = normalized.kind() {
                    normalized = normalized.cons_car();
                }
                let check_msg = match attr_name {
                    ":foreground" => "Empty foreground color value",
                    ":background" => "Empty background color value",
                    _ => "Empty distant-foreground color value",
                };
                check_non_empty_string(&normalized, check_msg)?;
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
                        vec![
                            Value::string("Invalid extend face attribute value"),
                            normalized,
                        ],
                    ));
                }
            }
        }
        ":underline" => {
            if !is_reset_like && !valid_face_underline_value(normalized) {
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid face underline"), normalized],
                ));
            }
        }
        ":box" => {
            // GNU xfaces.c `internal-set-lisp-face-attribute` (QCbox arm):
            // `t` means a simple box of width 1 in the face's foreground
            // color and is canonicalized to the fixnum 1 *before* validation
            // and storage, so `face-attribute` later reports 1, not t.
            if normalized == Value::T {
                normalized = Value::fixnum(1);
            }
            if !is_reset_like && !valid_face_box_value(normalized) {
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid face box"), normalized],
                ));
            }
        }
        ":inherit" => {
            // Accept any face_ref: nil / symbol / list of face_refs /
            // plist of attributes. Matches GNU `merge_face_ref`
            // (xfaces.c:2700-3025) which accepts any value and
            // dispatches by shape at resolution time.
            let valid = matches!(
                normalized.kind(),
                ValueKind::Nil | ValueKind::T | ValueKind::Symbol(_) | ValueKind::Cons
            );
            if !valid {
                let mut payload = vec![Value::string("Invalid face inheritance")];
                payload.push(normalized);
                return Err(signal("error", payload));
            }
        }
        ":bold" => {
            let mapped = if normalized.is_nil() {
                Value::symbol("normal")
            } else {
                Value::symbol("bold")
            };
            return Ok((LFaceAttr::Weight, mapped));
        }
        ":italic" => {
            let mapped = if normalized.is_nil() {
                Value::symbol("normal")
            } else {
                Value::symbol("italic")
            };
            return Ok((LFaceAttr::Slant, mapped));
        }
        _ => {}
    }

    match attr {
        SetFaceAttr::LFace(attr) => Ok((attr, normalized)),
        SetFaceAttr::Alias(_) => unreachable!("aliases returned above"),
    }
}

/// `(internal-lisp-face-p FACE &optional FRAME)` -- return a face descriptor
/// vector for known faces, nil otherwise.
pub(crate) fn builtin_internal_lisp_face_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("internal-lisp-face-p", &args, 1)?;
    expect_max_args("internal-lisp-face-p", &args, 2)?;

    // Fast path mirroring GNU's `Finternal_lisp_face_p`: resolve the
    // `face-alias` chain, then perform an allocation-free, symbol-keyed lookup
    // in the frame face table (2-arg live frame) or the global
    // `face--new-frame-defaults` table (null frame). GNU never allocates, seeds,
    // or creates here; neither does this path. The known-face/ensure gate is
    // retained only as a cold fallback for a table miss.
    let resolved = resolve_face_designator(eval, args[0], FaceAliasCyclePolicy::Signal)?;
    let key = resolved.name().map(ResolvedFaceName::symbol);

    if let Some(frame) = args.get(1)
        && !frame.is_nil()
    {
        if !live_frame_designator_in_state(&eval.frames, frame) {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("frame-live-p"), *frame],
            ));
        }
        let frame_id = frame_id_from_designator(frame)
            .expect("live frame designator should decode to frame id");
        if let Some(vector) =
            key.and_then(|k| lookup_frame_lisp_face_vector_by_symbol(eval, frame_id, k))
        {
            return Ok(vector);
        }
        return Ok(match known_resolved_face_name(resolved) {
            Some(face_name) => lookup_frame_lisp_face_vector(eval, frame_id, face_name.name())
                .unwrap_or(Value::NIL),
            None => Value::NIL,
        });
    }

    // Null-frame (or omitted-frame) global path.
    if let Some(vector) =
        key.and_then(|k| crate::emacs_core::xfaces::lookup_face_new_frame_defaults_vector(eval, k))
    {
        return Ok(vector);
    }
    Ok(match known_resolved_face_name(resolved) {
        Some(face_name) => {
            ensure_global_lisp_face_vector(eval, face_name.name()).unwrap_or(Value::NIL)
        }
        None => Value::NIL,
    })
}

/// Eval-backed version of `internal-make-lisp-face` that also ensures the face
/// exists in the evaluator's `FaceTable`.
pub(crate) fn builtin_internal_make_lisp_face(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("internal-make-lisp-face", &args, 1)?;
    expect_max_args("internal-make-lisp-face", &args, 2)?;
    let _ = require_symbol_face_name(&args[0])?;
    let face_name = resolve_face_designator(eval, args[0], FaceAliasCyclePolicy::UseDefault)?
        .name()
        .expect("a required symbol resolves to a named face")
        .name()
        .to_owned();
    if let Some(frame) = args.get(1)
        && !frame.is_nil()
        && !live_frame_designator_in_state(&eval.frames, frame)
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *frame],
        ));
    }
    mark_created_lisp_face(&face_name);
    ensure_lisp_face_id_property(eval, &face_name)?;
    let _ = ensure_global_lisp_face_vector(eval, &face_name);
    let result = if let Some(frame) = args.get(1).filter(|frame| !frame.is_nil()) {
        let frame_id = frame_id_from_designator(frame)
            .expect("validated frame designator should decode to frame id");
        let vector =
            ensure_frame_lisp_face_vector(eval, frame_id, &face_name, FrameFaceInitial::Empty)
                .unwrap_or_else(make_lisp_face_vector);
        reset_lisp_face_vector(vector);
        clear_face_overrides(&face_name, false);
        vector
    } else {
        let vector =
            ensure_global_lisp_face_vector(eval, &face_name).unwrap_or_else(make_lisp_face_vector);
        reset_lisp_face_vector(vector);
        clear_face_overrides(&face_name, true);
        vector
    };
    eval.face_table.ensure_face(&face_name);
    eval.face_change_count += 1;
    Ok(result)
}

/// Eval-backed version of `internal-copy-lisp-face`.
///
/// The copied Lisp vector remains authoritative; redisplay derives its
/// runtime representation after observing `face_change_count`.
pub(crate) fn builtin_internal_copy_lisp_face(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal-copy-lisp-face", &args, 4)?;
    let _ = require_symbol_face_name(&args[0])?;
    let _ = require_symbol_face_name(&args[1])?;
    let to_name = resolve_face_designator(eval, args[1], FaceAliasCyclePolicy::UseDefault)?
        .name()
        .expect("a required symbol resolves to a named face")
        .name()
        .to_owned();
    let copy_defaults_domain = args[2].is_t();
    if !copy_defaults_domain && !live_frame_designator_in_state(&eval.frames, &args[2]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), args[2]],
        ));
    }
    if !copy_defaults_domain
        && !args[3].is_nil()
        && !live_frame_designator_in_state(&eval.frames, &args[3])
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), args[3]],
        ));
    }
    let from_name = resolve_copy_source_face_symbol(eval, &args[0])?;
    mark_created_lisp_face(&to_name);
    ensure_lisp_face_id_property(eval, &to_name)?;
    let _ = ensure_global_lisp_face_vector(eval, &to_name);
    let (src_vector, dst_vector, defaults_frame) = if copy_defaults_domain {
        let src_vector = ensure_global_lisp_face_vector(eval, &from_name)
            .ok_or_else(|| invalid_face_error(args[0]))?;
        let dst_vector =
            ensure_global_lisp_face_vector(eval, &to_name).unwrap_or_else(make_lisp_face_vector);
        (src_vector, dst_vector, true)
    } else {
        let frame_id = frame_id_from_designator(&args[2])
            .expect("validated frame designator should decode to frame id");
        let new_frame_id = if args[3].is_nil() {
            frame_id
        } else {
            frame_id_from_designator(&args[3])
                .expect("validated frame designator should decode to frame id")
        };
        let src_vector = ensure_frame_lisp_face_vector(
            eval,
            frame_id,
            &from_name,
            FrameFaceInitial::SelectedBase,
        )
        .ok_or_else(|| invalid_face_error(args[0]))?;
        let dst_vector =
            ensure_frame_lisp_face_vector(eval, new_frame_id, &to_name, FrameFaceInitial::Empty)
                .unwrap_or_else(make_lisp_face_vector);
        (src_vector, dst_vector, false)
    };
    copy_lisp_face_vector_slots(src_vector, dst_vector);
    sync_face_overrides_from_lisp_face_vector(&to_name, dst_vector, defaults_frame);

    let result = args[1];

    eval.face_change_count += 1;

    Ok(result)
}

/// Eval-backed version of `internal-set-lisp-face-attribute`.
///
/// Like GNU Emacs' `Finternal_set_lisp_face_attribute`, this mutates the
/// authoritative Lisp face specification and marks face state changed.  It
/// deliberately does not materialize the display-facing `FaceTable` or a
/// per-frame runtime face: redisplay owns those derived representations.
pub(crate) fn builtin_internal_set_lisp_face_attribute(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    // Pure logic (FACE_ATTR_STATE storage + validation)
    expect_min_args("internal-set-lisp-face-attribute", &args, 3)?;
    expect_max_args("internal-set-lisp-face-attribute", &args, 4)?;
    let face = &args[0];
    let _ = require_symbol_face_name(face)?;
    let resolved_face = resolve_face_designator(eval, *face, FaceAliasCyclePolicy::Signal)?
        .name()
        .expect("a required symbol resolves to a named face");
    let face_name = resolved_face.name().to_owned();
    let face_symbol = resolved_face.symbol();
    // The chain below is id-keyed throughout; resolve the SymId once instead
    // of re-interning `face_name` at every registry touch (12,788 calls
    // during a batch startup's defface processing paid 3-5 interns each).
    let face_id = face_symbol
        .as_symbol_id()
        .expect("a required symbol face resolves to a symbol");
    let attr_name = normalize_set_face_attribute_name(&args[1])?;
    let value = args[2];
    if let Some(frame) = args.get(3)
        && !frame.is_nil()
        && !frame.is_t()
        && frame.as_fixnum() != Some(0)
        && !live_frame_designator_in_state(&eval.frames, frame)
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *frame],
        ));
    }

    let mut changed_live_frames = Vec::new();
    {
        let mut apply_set = |defaults_frame: bool| -> Result<(), Flow> {
            if defaults_frame {
                if !face_exists_for_domain_id(face_id, &face_name, true) {
                    if face.is_nil() {
                        return Err(signal("error", vec![Value::string("Invalid face")]));
                    }
                    return Err(signal(
                        "error",
                        vec![Value::string("Invalid face"), face_symbol],
                    ));
                }
            } else if !face_exists_for_domain_id(face_id, &face_name, false) {
                mark_selected_created_lisp_face_id(face_id);
                mark_created_lisp_face_id(face_id, &face_name);
                // GNU Emacs `Finternal_set_lisp_face_attribute` calls
                // `lface_from_face_name` which calls `Finternal_make_lisp_face`,
                // which stores the internal face ID as the symbol's `face`
                // property.  Without this `check-face` / `face-id` fail.
                ensure_lisp_face_id_property(eval, &face_name)?;
            }

            let (canonical_attr, mut canonical_value) =
                normalize_face_attr_for_set_with_eval(Some(eval), &face_name, attr_name, value)?;
            // GNU Emacs: when updating face--new-frame-defaults, convert
            // `unspecified' to `:ignore-defface' so the defface spec
            // doesn't override the explicitly unspecified value
            // (xfaces.c:3262, Finternal_set_lisp_face_attribute).
            if defaults_frame
                && is_reset_like_face_attr_value(&canonical_value)
                && canonical_value.is_symbol_named("unspecified")
            {
                canonical_value = Value::symbol(":ignore-defface");
            }
            set_face_override_id(face_id, canonical_attr, canonical_value, defaults_frame);
            if defaults_frame {
                if let Some(vector) = ensure_global_lisp_face_vector(eval, &face_name) {
                    set_lisp_face_vector_attr_with_font_derivatives(
                        &face_name,
                        vector,
                        canonical_attr,
                        canonical_value,
                        canonical_value,
                    )?;
                }
            } else {
                let frame_ids = match args.get(3) {
                    Some(v) if v.as_fixnum() == Some(0) => eval.frames.frame_list(),
                    Some(frame) if live_frame_designator_in_state(&eval.frames, frame) => {
                        frame_id_from_designator(frame)
                            .map(|frame_id| vec![frame_id])
                            .unwrap_or_default()
                    }
                    _ => vec![super::window_cmds::ensure_selected_frame_id(eval)],
                };
                let initial = if is_known_lisp_face_name(&face_name) {
                    FrameFaceInitial::SelectedBase
                } else {
                    FrameFaceInitial::Empty
                };
                for frame_id in frame_ids {
                    if let Some(vector) = ensure_frame_lisp_face_vector_by_symbol(
                        eval,
                        frame_id,
                        face_symbol,
                        &face_name,
                        initial,
                    ) {
                        let changed =
                            lisp_face_vector_attr(vector, canonical_attr) != Some(canonical_value);
                        set_lisp_face_vector_attr_with_font_derivatives(
                            &face_name,
                            vector,
                            canonical_attr,
                            canonical_value,
                            canonical_value,
                        )?;
                        if changed && !changed_live_frames.contains(&frame_id) {
                            changed_live_frames.push(frame_id);
                        }
                    }
                }
            }
            if canonical_attr == LFaceAttr::Font && !is_reset_like_face_attr_value(&canonical_value)
            {
                apply_derived_font_face_overrides(&face_name, &canonical_value, defaults_frame)?;
            }
            Ok(())
        };

        match args.get(3) {
            None => apply_set(false)?,
            Some(v) if v.is_nil() => apply_set(false)?,
            Some(v) if v.is_t() => apply_set(true)?,
            Some(v) if v.as_fixnum() == Some(0) => {
                apply_set(true)?;
                apply_set(false)?;
            }
            Some(_) => apply_set(false)?,
        }
    }

    let result = face_symbol;

    // Preserve GNU-visible live-frame font/default-parameter side effects,
    // but leave conversion to render-facing face attributes to redisplay.
    if args.len() >= 3 {
        let value = args[2];

        if let Ok(attr_name) = normalize_set_face_attribute_name(&args[1]) {
            let (canonical_attr, canonical_value) =
                normalize_face_attr_for_set_with_eval(Some(eval), &face_name, attr_name, value)?;
            let mut public_effective_value = canonical_value;
            if canonical_attr == LFaceAttr::Font {
                let targets = font_face_realization_targets(eval, args.get(3));
                let mut published_live_override = false;
                for target in targets {
                    let frame_id = target.reference_frame();
                    let resolution = resolve_live_frame_font_request(eval, frame_id, &value);
                    let effective_value = resolution.font_value;
                    set_realized_font_for_target(
                        eval,
                        target,
                        face_symbol,
                        &face_name,
                        effective_value,
                    )?;

                    // FACE_ATTR_STATE has one new-frame-default domain and one
                    // live-frame compatibility domain. Publish the selected
                    // frame first for FRAME=0; authoritative per-frame vectors
                    // above retain every distinct opened object and height.
                    let publish_override = target.defaults_frame() || !published_live_override;
                    if publish_override {
                        let defaults_frame = target.defaults_frame();
                        if effective_value != value {
                            set_face_override(
                                &face_name,
                                canonical_attr,
                                effective_value,
                                defaults_frame,
                            );
                        }
                        for (derived_attr, derived_value) in
                            derived_face_attrs_from_font_value(&effective_value)
                        {
                            set_face_override(
                                &face_name,
                                derived_attr,
                                derived_value,
                                defaults_frame,
                            );
                        }
                        if !defaults_frame {
                            published_live_override = true;
                        }
                        public_effective_value = effective_value;
                    }

                    if face_name == "default"
                        && let FontFaceRealizationTarget::LiveFrame(frame_id) = target
                    {
                        sync_live_frame_font_state(eval, frame_id, &value, &resolution);
                    }
                }
            } else if face_name == "default"
                && default_face_font_attr_affects_frame_font(canonical_attr)
            {
                // GNU handles FRAME=0 recursively, so each frame reopens the
                // font against its own display metrics after a family, height,
                // weight, slant, or width change.
                for frame_id in changed_live_frames.iter().copied() {
                    sync_live_default_face_font_state(eval, frame_id);
                }
            }

            if let Some(parameter) = frame_parameter_for_face_attribute(&face_name, canonical_attr)
                && !canonical_value.is_symbol_named("unspecified")
                && !canonical_value.is_symbol_named(":ignore-defface")
            {
                for frame_id in changed_live_frames.iter().copied() {
                    publish_face_attribute_to_frame_parameter(
                        eval,
                        frame_id,
                        parameter,
                        public_effective_value,
                    )?;
                }
            }
        }
    }

    eval.face_change_count += 1;

    Ok(result)
}

/// Convert a Lisp face attribute value to `FaceAttrValue` for `FaceTable`.
fn lisp_value_to_face_attr_resolved(
    attr: LFaceAttr,
    value: Value,
    resolver: FaceColorResolver<'_>,
) -> Option<crate::face::FaceAttrValue> {
    use crate::face::{
        BoxBorder, BoxStyle, FaceAttrValue, FaceHeight, FontSlant, FontWeight, FontWidth,
        SpecifiedColor, Underline, UnderlinePosition, UnderlineStyle,
    };

    // "unspecified" symbol = reset the attribute
    if value.is_symbol_named("unspecified") {
        return Some(FaceAttrValue::Unspecified);
    }

    match attr {
        LFaceAttr::Foreground | LFaceAttr::Background | LFaceAttr::DistantForeground => {
            let s = match value.kind() {
                ValueKind::Cons => value.cons_car().as_utf8_str(),
                _ => value.as_utf8_str().or_else(|| value.as_symbol_name()),
            };
            let s = s?;
            // Parse the lface string to a spec exactly once; realization —
            // the only spec-to-pixel bridge — happens here, at the boundary
            // where the runtime face table is built for a known frame class.
            Some(FaceAttrValue::Color(
                resolver.realize(&SpecifiedColor::parse(s))?,
            ))
        }
        LFaceAttr::Weight => {
            let name = value.as_symbol_name()?;
            Some(FaceAttrValue::Weight(FontWeight::from_symbol(name)?))
        }
        LFaceAttr::Slant => {
            let name = value.as_symbol_name()?;
            Some(FaceAttrValue::Slant(FontSlant::from_symbol(name)?))
        }
        LFaceAttr::Width => {
            let name = value.as_symbol_name()?;
            Some(FaceAttrValue::Width(FontWidth::from_symbol(name)?))
        }
        LFaceAttr::Height => match value.kind() {
            ValueKind::Fixnum(n) => Some(FaceAttrValue::Height(FaceHeight::Absolute(n as i32))),
            ValueKind::Float => Some(FaceAttrValue::Height(FaceHeight::Relative(value.xfloat()))),
            _ => None,
        },
        LFaceAttr::Family | LFaceAttr::Foundry => {
            if value.is_string() {
                Some(FaceAttrValue::Text(value))
            } else {
                None
            }
        }
        LFaceAttr::Underline => {
            if value.is_nil() {
                return Some(FaceAttrValue::Bool(false));
            }
            if value.is_t() {
                return Some(FaceAttrValue::Bool(true));
            }
            if let Some(s) = value.as_utf8_str() {
                let color = resolver.realize(&SpecifiedColor::parse(s));
                return Some(FaceAttrValue::Underline(Underline {
                    style: UnderlineStyle::Line,
                    color,
                    position: UnderlinePosition::FontMetric,
                }));
            }
            // Plist form: (:style STYLE :color COLOR :position POS)
            if let Some(plist) = super::value::list_to_vec(&value) {
                let mut style = UnderlineStyle::Line;
                let mut color = None;
                let mut position = UnderlinePosition::FontMetric;
                let mut i = 0;
                while i + 1 < plist.len() {
                    let key = plist[i].as_symbol_name().unwrap_or("");
                    let val = &plist[i + 1];
                    match key {
                        ":style" => {
                            style = val
                                .as_symbol_name()
                                .and_then(UnderlineStyle::from_symbol)
                                .unwrap_or(UnderlineStyle::Line);
                        }
                        ":color" => {
                            if let Some(s) = val.as_utf8_str().or_else(|| val.as_symbol_name()) {
                                color = resolver.realize(&SpecifiedColor::parse(s));
                            }
                        }
                        ":position" => {
                            position = UnderlinePosition::from_lisp(val);
                        }
                        _ => {}
                    }
                    i += 2;
                }
                return Some(FaceAttrValue::Underline(Underline {
                    style,
                    color,
                    position,
                }));
            }
            Some(FaceAttrValue::Bool(true))
        }
        LFaceAttr::Overline | LFaceAttr::StrikeThrough => {
            if value.is_nil() {
                return Some(FaceAttrValue::Bool(false));
            }
            if value.is_t() {
                return Some(FaceAttrValue::Bool(true));
            }
            if let Some(s) = value.as_utf8_str() {
                // GNU map_tty_color covers only fg/bg/underline; overline
                // and strike-through colors keep the context-free parse.
                let c = FaceColorResolver::Standard.realize(&SpecifiedColor::parse(s))?;
                return Some(FaceAttrValue::Color(c));
            }
            Some(FaceAttrValue::Bool(value.is_truthy()))
        }
        LFaceAttr::Box => {
            if value.is_nil() {
                return Some(FaceAttrValue::Unspecified);
            }
            if value.is_t() {
                return Some(FaceAttrValue::Box(BoxBorder {
                    color: None,
                    width: 1,
                    style: BoxStyle::Flat,
                }));
            }
            if let Some(n) = value.as_fixnum() {
                return Some(FaceAttrValue::Box(BoxBorder {
                    color: None,
                    width: n as i32,
                    style: BoxStyle::Flat,
                }));
            }
            // Color string shorthand. Box colors are not tty-mapped in GNU
            // (map_tty_color covers only fg/bg/underline).
            if let Some(s) = value.as_utf8_str() {
                let color = FaceColorResolver::Standard.realize(&SpecifiedColor::parse(s));
                return Some(FaceAttrValue::Box(BoxBorder {
                    color,
                    width: 1,
                    style: BoxStyle::Flat,
                }));
            }
            // Plist form: (:line-width WIDTH :color COLOR :style STYLE)
            if let Some(plist) = super::value::list_to_vec(&value) {
                let mut border = BoxBorder {
                    color: None,
                    width: 1,
                    style: BoxStyle::Flat,
                };
                let mut i = 0;
                while i + 1 < plist.len() {
                    let key = plist[i].as_symbol_name().unwrap_or("");
                    let val = &plist[i + 1];
                    match key {
                        ":line-width" => {
                            if let Some(n) = val.as_fixnum() {
                                border.width = n as i32;
                            }
                        }
                        ":color" => {
                            if let Some(s) = val.as_utf8_str().or_else(|| val.as_symbol_name()) {
                                border.color =
                                    FaceColorResolver::Standard.realize(&SpecifiedColor::parse(s));
                            }
                        }
                        ":style" => {
                            border.style = val
                                .as_symbol_name()
                                .and_then(BoxStyle::from_symbol)
                                .unwrap_or(BoxStyle::Flat);
                        }
                        _ => {}
                    }
                    i += 2;
                }
                return Some(FaceAttrValue::Box(border));
            }
            Some(FaceAttrValue::Box(BoxBorder {
                color: None,
                width: 1,
                style: BoxStyle::Flat,
            }))
        }
        LFaceAttr::InverseVideo | LFaceAttr::Extend => Some(FaceAttrValue::Bool(value.is_truthy())),
        LFaceAttr::Inherit => {
            // Store raw face_ref. Matches GNU's `LFACE_INHERIT_INDEX`
            // slot which holds any face_ref (symbol / list / plist);
            // `merge_face_ref` dispatches on shape at resolution time.
            if value.is_nil() || value.is_symbol_named("nil") {
                return Some(FaceAttrValue::Inherit(None));
            }
            Some(FaceAttrValue::Inherit(Some(value)))
        }
        LFaceAttr::Stipple => {
            // Store the raw stipple spec (a `(W H DATA)` cons, a bitmap file
            // string, or a symbol). GNU keeps it in `LFACE_STIPPLE_INDEX` and
            // realizes it to a pixmap at draw time; neomacs realizes it to a
            // `StipplePattern` in the layout bridge (`realize_face`).
            if value.is_nil() || value.is_symbol_named("nil") {
                Some(FaceAttrValue::Stipple(None))
            } else {
                Some(FaceAttrValue::Stipple(Some(value)))
            }
        }
        LFaceAttr::Font | LFaceAttr::Fontset => None,
    }
}
pub(crate) fn builtin_internal_get_lisp_face_attribute(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("internal-get-lisp-face-attribute", &args, 2)?;
    expect_max_args("internal-get-lisp-face-attribute", &args, 3)?;
    let defaults_frame = if let Some(frame) = args.get(2) {
        if frame.is_nil() {
            false
        } else if frame.is_t() {
            true
        } else if live_frame_designator_in_state(&eval.frames, frame) {
            false
        } else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("frame-live-p"), *frame],
            ));
        }
    } else {
        false
    };

    let face_name = resolve_face_name_for_domain(eval, &args[0], defaults_frame)?;
    let attr_name = normalize_face_attribute_name(&args[1])?;

    if defaults_frame {
        if let Some(vector) = ensure_global_lisp_face_vector(eval, &face_name)
            && let Some(value) = lisp_face_vector_attr(vector, attr_name)
        {
            return Ok(value);
        }
        return Ok(lisp_face_attribute_value(&face_name, attr_name, true));
    }

    let frame_id = match args.get(2) {
        None => Some(super::window_cmds::ensure_selected_frame_id(eval)),
        Some(v) if v.is_nil() => Some(super::window_cmds::ensure_selected_frame_id(eval)),
        Some(frame) if live_frame_designator_in_state(&eval.frames, frame) => {
            frame_id_from_designator(frame)
        }
        _ => None,
    };

    if face_name == "default"
        && get_face_override(&face_name, attr_name, false).is_none()
        && matches!(
            attr_name,
            LFaceAttr::Font
                | LFaceAttr::Family
                | LFaceAttr::Foundry
                | LFaceAttr::Weight
                | LFaceAttr::Slant
                | LFaceAttr::Width
                | LFaceAttr::Height
        )
        && let Some(frame_id) = frame_id
        && let Some(fallback) = live_frame_font_attribute_fallback(eval, frame_id, attr_name)
    {
        return Ok(fallback);
    }

    let lisp_value = frame_id
        .and_then(|frame_id| {
            let initial = if is_known_lisp_face_name(&face_name) {
                FrameFaceInitial::SelectedBase
            } else {
                FrameFaceInitial::Empty
            };
            ensure_frame_lisp_face_vector(eval, frame_id, &face_name, initial)
        })
        .and_then(|vector| lisp_face_vector_attr(vector, attr_name))
        .unwrap_or_else(|| lisp_face_attribute_value(&face_name, attr_name, false));
    // GNU `internal-get-lisp-face-attribute` (xfaces.c) returns the LISP face
    // attribute (`LFACE_*` of `lface_from_face_name`), never the *realized*
    // face. Do NOT fall back to the runtime realized face here: the realized
    // face on this batch/mono frame still carries colors realized for a
    // color-capable display during the bootstrap image build (e.g. `error`
    // :foreground "#ff0000"), whereas GNU returns `unspecified` because no
    // display clause of the defface spec matched a mono terminal. The lisp face
    // value above (frame lisp vector slot, falling back to the base/override
    // value) is the GNU-faithful answer.
    Ok(lisp_value)
}

/// `(internal-lisp-face-attribute-values ATTR)` -- return valid discrete values
/// for known boolean-like face attributes.
pub(crate) fn builtin_internal_lisp_face_attribute_values(args: Vec<Value>) -> EvalResult {
    expect_args("internal-lisp-face-attribute-values", &args, 1)?;
    let attr_name = face_attr_value_name(&args[0])?;
    if LFaceAttr::from_keyword(resolve_sym(attr_name)).is_some_and(LFaceAttr::is_discrete_boolean) {
        Ok(Value::list(vec![Value::T, Value::NIL]))
    } else {
        Ok(Value::NIL)
    }
}

/// `(internal-lisp-face-equal-p FACE1 FACE2 &optional FRAME)` -- return t if
/// FACE1 and FACE2 resolve to equal face attributes in the selected frame or in
/// default face definitions when FRAME is t.
pub(crate) fn builtin_internal_lisp_face_equal_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("internal-lisp-face-equal-p", &args, 2)?;
    expect_max_args("internal-lisp-face-equal-p", &args, 3)?;
    let defaults_frame = frame_defaults_flag(args.get(2))?;
    let face1 = resolve_known_face_name_for_compare(eval, &args[0], defaults_frame)?;
    let face2 = resolve_known_face_name_for_compare(eval, &args[1], defaults_frame)?;
    for attr in LFACE_ATTRS {
        let v1 = lisp_face_attribute_value(&face1, attr, defaults_frame);
        let v2 = lisp_face_attribute_value(&face2, attr, defaults_frame);
        if v1 != v2 {
            return Ok(Value::NIL);
        }
    }
    Ok(Value::T)
}

/// `(internal-lisp-face-empty-p FACE &optional FRAME)` -- return t if FACE has
/// only unspecified attributes in selected/default face definitions.
pub(crate) fn builtin_internal_lisp_face_empty_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("internal-lisp-face-empty-p", &args, 1)?;
    expect_max_args("internal-lisp-face-empty-p", &args, 2)?;
    let defaults_frame = frame_defaults_flag(args.get(1))?;
    let face = resolve_known_face_name_for_compare(eval, &args[0], defaults_frame)?;
    for attr in LFACE_ATTRS {
        let v = lisp_face_attribute_value(&face, attr, defaults_frame);
        if !v.is_symbol_named("unspecified") {
            return Ok(Value::NIL);
        }
    }
    Ok(Value::T)
}

pub(crate) fn builtin_internal_merge_in_global_face(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal-merge-in-global-face", &args, 2)?;
    if !frame_device_designator_p(&args[1]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), args[1]],
        ));
    }
    let face_name = resolve_face_name_for_merge(eval, &args[0])?;
    if !is_known_lisp_face_name(&face_name) {
        mark_created_lisp_face(&face_name);
        mark_selected_created_lisp_face(&face_name);
    }
    merge_defaults_overrides_into_selected(&face_name);
    let frame_id = frame_id_from_designator(&args[1])
        .expect("validated frame designator should decode to frame id");
    if let (Some(global_vector), Some(local_vector)) = (
        ensure_global_lisp_face_vector(eval, &face_name),
        ensure_frame_lisp_face_vector(eval, frame_id, &face_name, FrameFaceInitial::Empty),
    ) {
        for attr in LFACE_ATTRS {
            let Some(global_value) = lisp_face_vector_attr(global_vector, attr) else {
                continue;
            };
            if global_value.is_symbol_named(":ignore-defface") {
                set_lisp_face_vector_attr(local_vector, attr, Value::symbol("unspecified"));
            } else if !global_value.is_symbol_named("unspecified") {
                set_lisp_face_vector_attr(local_vector, attr, global_value);
            }
        }
        sync_face_overrides_from_lisp_face_vector(&face_name, local_vector, false);
    }

    eval.face_change_count += 1;
    Ok(Value::NIL)
}

/// `(face-attribute-relative-p ATTRIBUTE VALUE)` -- return t if VALUE is the
/// value is a relative form for ATTRIBUTE.
pub(crate) fn builtin_face_attribute_relative_p(args: Vec<Value>) -> EvalResult {
    expect_args("face-attribute-relative-p", &args, 2)?;
    let value_is_relative_reset = args[1]
        .as_symbol_id()
        .or_else(|| args[1].as_keyword_id())
        .is_some_and(|id_| {
            matches!(
                resolve_sym(id_),
                "unspecified" | ":ignore-defface" | "ignore-defface"
            )
        });
    if value_is_relative_reset {
        return Ok(Value::T);
    }

    let height_attr = match args[0].kind() {
        ValueKind::Symbol(id) => {
            let n = resolve_sym(id);
            n == "height" || n == ":height"
        }
        _ => false,
    };
    if !height_attr {
        return Ok(Value::NIL);
    }

    Ok(Value::bool_val(
        !(args[1].is_fixnum() || args[1].as_char().is_some()),
    ))
}

/// `(merge-face-attribute ATTRIBUTE VALUE1 VALUE2)` -- return VALUE1 unless it
/// is the symbol `unspecified`, in which case return VALUE2.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_merge_face_attribute(args: Vec<Value>) -> EvalResult {
    expect_args("merge-face-attribute", &args, 3)?;
    Ok(merge_face_attribute_impl(None, &args))
}

pub(crate) fn builtin_merge_face_attribute_with_eval(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("merge-face-attribute", &args, 3)?;
    Ok(merge_face_attribute_impl(Some(eval), &args))
}

fn merge_face_attribute_impl(eval: Option<&mut super::eval::Context>, args: &[Value]) -> Value {
    let value1_is_relative_reset = args[1]
        .as_symbol_id()
        .or_else(|| args[1].as_keyword_id())
        .is_some_and(|id_| {
            matches!(
                resolve_sym(id_),
                "unspecified" | ":ignore-defface" | "ignore-defface"
            )
        });
    if value1_is_relative_reset {
        return args[2];
    }

    let height_attr = args[0]
        .as_symbol_id()
        .or_else(|| args[0].as_keyword_id())
        .is_some_and(|id_| matches!(resolve_sym(id_), "height" | ":height"));
    if height_attr {
        return merge_face_height_value(eval, args[1], args[2], args[1]);
    }

    args[1]
}

/// `(face-list &optional FRAME)` -- return list of known face names.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_face_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("face-list", &args, 1)?;
    Ok(Value::list(
        all_defined_face_names_sorted_by_id_desc()
            .iter()
            .map(|name| Value::symbol(name.as_str()))
            .collect(),
    ))
}

fn expect_color_string(value: &Value) -> Result<String, Flow> {
    match value.kind() {
        ValueKind::String => Ok(font_string_text(value).expect("checked string")),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )),
    }
}

fn expect_optional_color_frame_arg(args: &[Value], idx: usize) -> Result<(), Flow> {
    if let Some(frame) = args.get(idx)
        && !frame.is_nil()
        && !frame.is_frame()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("framep"), *frame],
        ));
    }
    Ok(())
}

fn selected_or_designated_live_frame_id(
    frames: &FrameManager,
    frame: Option<&Value>,
) -> Result<FrameId, Flow> {
    match frame {
        None => frames
            .selected_frame()
            .map(|frame| frame.id)
            .ok_or_else(|| signal("error", vec![Value::string("No selected frame")])),
        Some(v) if v.is_nil() => frames
            .selected_frame()
            .map(|frame| frame.id)
            .ok_or_else(|| signal("error", vec![Value::string("No selected frame")])),
        Some(value) if live_frame_designator_in_state(frames, value) => {
            Ok(frame_id_from_designator(value)
                .expect("live frame designator should decode to frame id"))
        }
        Some(other) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *other],
        )),
    }
}

fn graphic_color_target_frame_id(
    ctx: &super::eval::Context,
    frame: Option<&Value>,
) -> Result<Option<FrameId>, Flow> {
    let frame_id = selected_or_designated_live_frame_id(&ctx.frames, frame)?;
    Ok(ctx
        .frames
        .get(frame_id)
        .and_then(|frame| frame.effective_window_system())
        .filter(|window_system| super::display::gui_window_system_active_value(*window_system))
        .map(|_| frame_id))
}

fn parse_color_16bit_any(color_name: &str) -> Option<(i64, i64, i64)> {
    let lower = color_name.trim().to_lowercase();
    if let Some(hex) = lower.strip_prefix('#') {
        parse_hex_color_16bit(hex)
    } else {
        parse_named_color_16bit(&lower)
    }
}

/// `(color-defined-p COLOR &optional FRAME)` -- nil if unknown; otherwise truthy
/// for known RGB/hex and supported terminal color names.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_color_defined_p(args: Vec<Value>) -> EvalResult {
    expect_min_args("color-defined-p", &args, 1)?;
    expect_max_args("color-defined-p", &args, 2)?;
    expect_optional_color_device_arg(&args, 1)?;
    match args[0].kind() {
        ValueKind::String => Ok(Value::bool_val(
            !builtin_color_values(vec![args[0]])?.is_nil(),
        )),
        _ => Ok(Value::NIL),
    }
}

pub(crate) fn builtin_xw_color_defined_p_ctx(
    ctx: &super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("xw-color-defined-p", &args, 1)?;
    expect_max_args("xw-color-defined-p", &args, 2)?;
    expect_optional_color_frame_arg(&args, 1)?;
    if graphic_color_target_frame_id(ctx, args.get(1))?.is_none() {
        return Ok(Value::NIL);
    }
    let color_name = match args[0].kind() {
        ValueKind::String => font_string_text(&args[0]).expect("checked string"),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[0]],
            ));
        }
    };
    Ok(Value::bool_val(
        parse_color_16bit_any(&color_name).is_some(),
    ))
}

/// `(color-values COLOR &optional FRAME)` -- resolve COLOR and return a
/// terminal-compatible `(R G B)` list with 16-bit component values.
///
/// In batch/TTY compatibility mode we approximate resolved colors to the
/// nearest entry in the 8-color terminal palette.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_color_values(args: Vec<Value>) -> EvalResult {
    expect_min_args("color-values", &args, 1)?;
    expect_max_args("color-values", &args, 2)?;
    expect_optional_color_device_arg(&args, 1)?;
    let color_name = match args[0].kind() {
        ValueKind::String => font_string_text(&args[0]).expect("checked string"),
        _ => return Ok(Value::NIL),
    };
    let lower = color_name.trim().to_lowercase();
    let resolved = if let Some(hex) = lower.strip_prefix('#') {
        parse_hex_color_16bit(hex)
    } else {
        parse_named_color_16bit(&lower)
    };
    let Some((r, g, b)) = resolved.map(approximate_tty_color) else {
        return Ok(Value::NIL);
    };
    Ok(Value::list(vec![
        Value::fixnum(r),
        Value::fixnum(g),
        Value::fixnum(b),
    ]))
}

pub(crate) fn builtin_xw_color_values_ctx(
    ctx: &super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("xw-color-values", &args, 1)?;
    expect_max_args("xw-color-values", &args, 2)?;
    expect_optional_color_frame_arg(&args, 1)?;
    if graphic_color_target_frame_id(ctx, args.get(1))?.is_none() {
        return Ok(Value::NIL);
    }
    let color_name = match args[0].kind() {
        ValueKind::String => font_string_text(&args[0]).expect("checked string"),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[0]],
            ));
        }
    };
    let Some((r, g, b)) = parse_color_16bit_any(&color_name) else {
        return Ok(Value::NIL);
    };
    Ok(Value::list(vec![
        Value::fixnum(r),
        Value::fixnum(g),
        Value::fixnum(b),
    ]))
}

/// `(color-values-from-color-spec COLOR-SPEC)` -- parse hex color spec and
/// return raw `(R G B)` 16-bit channel values.
pub(crate) fn builtin_color_values_from_color_spec(args: Vec<Value>) -> EvalResult {
    expect_args("color-values-from-color-spec", &args, 1)?;
    let color_spec = expect_color_string(&args[0])?;
    let lower = color_spec.trim().to_lowercase();
    let Some(hex) = lower.strip_prefix('#') else {
        return Ok(Value::NIL);
    };
    let Some((r, g, b)) = parse_hex_color_16bit(hex) else {
        return Ok(Value::NIL);
    };
    Ok(Value::list(vec![
        Value::fixnum(r),
        Value::fixnum(g),
        Value::fixnum(b),
    ]))
}

/// `(color-gray-p COLOR &optional FRAME)` -- t if COLOR resolves to equal RGB
/// channels, nil otherwise.
pub(crate) fn builtin_color_gray_p(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("color-gray-p", &args, 1)?;
    expect_max_args("color-gray-p", &args, 2)?;
    let _ = expect_color_string(&args[0])?;
    expect_optional_color_frame_arg(&args, 1)?;
    // GNU `Fcolor_gray_p` -> `face_color_gray_p` (xfaces.c:1214-1235) resolves
    // the name through the frame's colour hook -- the same `tty-color-desc'
    // path as `color-values'/`color-distance', not a private name table -- and
    // treats an unresolvable colour as simply "not gray" (false), never an
    // error.
    let graphic = graphic_color_target_frame_id(ctx, args.get(1))
        .map(|id| id.is_some())
        .unwrap_or(false);
    let Ok((r, g, b)) = resolve_color_distance_rgb(ctx, &args[0], graphic) else {
        return Ok(Value::NIL);
    };
    Ok(Value::bool_val(color_is_gray(r, g, b)))
}

/// GNU `face_color_gray_p` (xfaces.c:1214-1235): a colour is "gray" if it is
/// close to black (every 16-bit channel < 5000) or its channels are within 5%
/// (`max/20`) of one another.
fn color_is_gray(r: i64, g: i64, b: i64) -> bool {
    if r < 5000 && g < 5000 && b < 5000 {
        return true;
    }
    (r - g).abs() < r.max(g) / 20 && (g - b).abs() < g.max(b) / 20 && (b - r).abs() < b.max(r) / 20
}

/// `(color-supported-p COLOR &optional FRAME BACKGROUND-P)` -- t if COLOR
/// resolves on this build's color parser.
pub(crate) fn builtin_color_supported_p(args: Vec<Value>) -> EvalResult {
    expect_min_args("color-supported-p", &args, 1)?;
    expect_max_args("color-supported-p", &args, 3)?;
    let color = expect_color_string(&args[0])?;
    expect_optional_color_frame_arg(&args, 1)?;
    let _ = args.get(2);
    Ok(Value::bool_val(parse_color_16bit_any(&color).is_some()))
}

fn expect_optional_color_distance_frame_arg(args: &[Value], idx: usize) -> Result<(), Flow> {
    if let Some(frame) = args.get(idx)
        && !frame.is_nil()
        && !frame.is_frame()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *frame],
        ));
    }
    Ok(())
}

fn invalid_color_error(value: &Value) -> Flow {
    signal("error", vec![Value::string("Invalid color"), *value])
}

/// Parse an `(R G B)` list of fixnums, mirroring GNU `parse_rgb_list`
/// (`src/xfaces.c`). Returns None unless the value is a 3+ element list whose
/// first three elements are fixnums.
fn parse_rgb_list(value: &Value) -> Option<(i64, i64, i64)> {
    let items = list_to_vec(value)?;
    if items.len() < 3 {
        return None;
    }
    Some((
        items[0].as_fixnum()?,
        items[1].as_fixnum()?,
        items[2].as_fixnum()?,
    ))
}

/// Resolve a `color-distance` argument to a 16-bit `(R G B)` triple, mirroring
/// GNU `Fcolor_distance` (`src/xfaces.c:4792`): an `(R G B)` list parses
/// directly via `parse_rgb_list`; a string is resolved through the frame
/// terminal's `defined_color_hook`. On a graphic frame that hook parses the
/// raw RGB; on a TTY frame (`tty_defined_color`) it calls `tty_lookup_color`,
/// which dispatches to the Lisp `tty-color-desc` to find the nearest entry in
/// the active terminal palette. We reproduce that TTY path so batch results
/// match GNU (e.g. "#808080" and "#c0c0c0" both quantize to white).
fn resolve_color_distance_rgb(
    ctx: &mut super::eval::Context,
    value: &Value,
    graphic: bool,
) -> Result<(i64, i64, i64), Flow> {
    if let Some(rgb) = parse_rgb_list(value) {
        return Ok(rgb);
    }
    if !value.is_string() {
        return Err(invalid_color_error(value));
    }
    let color = font_string_text(value).expect("checked string");
    if graphic {
        return parse_color_16bit_any(&color).ok_or_else(|| invalid_color_error(value));
    }
    // TTY frame: resolve via `tty-color-desc' -> (NAME INDEX R G B), exactly as
    // GNU's `tty_lookup_color' does. GNU guards this with `Ffboundp
    // (Qtty_color_desc)' and treats a failed lookup as "not resolved" (false),
    // never an error; mirror that so a bare environment (e.g. unit tests
    // without term/tty-colors.el loaded, where the call may signal) falls back
    // to a coarse quantization instead of propagating the signal.
    if ctx.obarray.fboundp("tty-color-desc")
        && let Ok(desc) = ctx.funcall_general(Value::symbol("tty-color-desc"), vec![*value])
        && let Some(items) = list_to_vec(&desc)
        && items.len() >= 5
        && let (Some(r), Some(g), Some(b)) = (
            items[2].as_fixnum(),
            items[3].as_fixnum(),
            items[4].as_fixnum(),
        )
    {
        return Ok((r, g, b));
    }
    // GNU consults the terminal-default sentinels only AFTER the palette
    // lookup has failed to resolve a pixel (src/xfaces.c:1160-1167), so the
    // order here matters: a palette entry named `unspecified-bg' would win.
    if let Some(default) = TtyDefaultColor::from_name(&color) {
        return Ok(default.rgb());
    }
    parse_color_16bit_any(&color)
        .map(approximate_tty_color)
        .ok_or_else(|| invalid_color_error(value))
}

/// The two colour names that stand for "whatever this terminal's default is",
/// which `face-foreground'/`face-background' return for an unspecified face
/// attribute on a TTY frame.
///
/// GNU keeps them out of the colour tables entirely: `tty_defined_color'
/// (src/xfaces.c:1143-1174) seeds its `Emacs_Color' with
/// `pixel = FACE_TTY_DEFAULT_COLOR' and RGB (0, 0, 0) (:1150-1153), tries
/// `tty_lookup_color', and only if that left the pixel unresolved maps these
/// two names to `FACE_TTY_DEFAULT_FG_COLOR'/`FACE_TTY_DEFAULT_BG_COLOR'
/// (:1163-1166).  Assigning the pixel is what makes the lookup succeed
/// (:1170-1171) -- the RGB triple is never touched, which is why GNU measures
/// both sentinels as black rather than as the terminal's actual colours.
///
/// This is deliberately NOT wired into `tty-color-alist' or the Lisp
/// `color-values'/`color-defined-p', which answer nil for these names in GNU
/// too: the sentinel branch exists only in the C `defined_color_hook'.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TtyDefaultColor {
    /// `unspecified-fg' -> `FACE_TTY_DEFAULT_FG_COLOR'.
    Foreground,
    /// `unspecified-bg' -> `FACE_TTY_DEFAULT_BG_COLOR'.
    Background,
}

impl TtyDefaultColor {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "unspecified-fg" => Some(Self::Foreground),
            "unspecified-bg" => Some(Self::Background),
            _ => None,
        }
    }

    /// GNU's zero defaults, left untouched by the sentinel branch
    /// (src/xfaces.c:1151-1153).  Both sentinels therefore carry the same
    /// colour value, so the distance between them is zero.
    pub(crate) fn rgb(self) -> (i64, i64, i64) {
        (0, 0, 0)
    }
}

fn color_distance_metric(lhs: (i64, i64, i64), rhs: (i64, i64, i64)) -> i64 {
    // GNU `color_distance` (xfaces.c): the Riemersma colour metric over 16-bit
    // channels (the inputs here are already 0..65535). This is a more even
    // approximation of L*u*v* than the 8-bit redmean variant.
    // See https://www.compuphase.com/cmetric.htm
    let r = lhs.0 - rhs.0;
    let g = lhs.1 - rhs.1;
    let b = lhs.2 - rhs.2;
    let r_mean = (lhs.0 + rhs.0) >> 1;
    ((((2 * 65536 + r_mean) * r * r) >> 16)
        + 4 * g * g
        + (((2 * 65536 + 65535 - r_mean) * b * b) >> 16))
        >> 16
}

/// `(color-distance COLOR1 COLOR2 &optional FRAME METRIC)` -- return a
/// perceptual distance between colors. Mirrors GNU `Fcolor_distance`.
pub(crate) fn builtin_color_distance(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("color-distance", &args, 2)?;
    expect_max_args("color-distance", &args, 4)?;
    expect_optional_color_distance_frame_arg(&args, 2)?;
    // GNU resolves the frame's terminal type (graphic vs TTY) to pick the
    // colour-definition hook. When no frame is available (e.g. a bare
    // headless context), default to the TTY path, which is also the
    // batch/`--batch' default.
    let graphic = graphic_color_target_frame_id(ctx, args.get(2))
        .map(|id| id.is_some())
        .unwrap_or(false);
    let lhs = resolve_color_distance_rgb(ctx, &args[0], graphic)?;
    let rhs = resolve_color_distance_rgb(ctx, &args[1], graphic)?;
    if let Some(metric) = args.get(3).filter(|m| !m.is_nil()) {
        // GNU calls METRIC with two (RED GREEN BLUE) lists.
        let metric = *metric;
        return ctx.funcall_general(
            metric,
            vec![
                Value::list(vec![
                    Value::fixnum(lhs.0),
                    Value::fixnum(lhs.1),
                    Value::fixnum(lhs.2),
                ]),
                Value::list(vec![
                    Value::fixnum(rhs.0),
                    Value::fixnum(rhs.1),
                    Value::fixnum(rhs.2),
                ]),
            ],
        );
    }
    Ok(Value::fixnum(color_distance_metric(lhs, rhs)))
}

fn parse_hex_color_16bit(hex: &str) -> Option<(i64, i64, i64)> {
    match hex.len() {
        3 => {
            let r = i64::from(hex[0..1].chars().next()?.to_digit(16)? as u16);
            let g = i64::from(hex[1..2].chars().next()?.to_digit(16)? as u16);
            let b = i64::from(hex[2..3].chars().next()?.to_digit(16)? as u16);
            Some((
                r | (r << 4) | (r << 8) | (r << 12),
                g | (g << 4) | (g << 8) | (g << 12),
                b | (b << 4) | (b << 8) | (b << 12),
            ))
        }
        6 => Some((
            i64::from(u16::from_str_radix(&hex[0..2], 16).ok()?) * 257,
            i64::from(u16::from_str_radix(&hex[2..4], 16).ok()?) * 257,
            i64::from(u16::from_str_radix(&hex[4..6], 16).ok()?) * 257,
        )),
        12 => Some((
            i64::from(u16::from_str_radix(&hex[0..4], 16).ok()?),
            i64::from(u16::from_str_radix(&hex[4..8], 16).ok()?),
            i64::from(u16::from_str_radix(&hex[8..12], 16).ok()?),
        )),
        _ => None,
    }
}

fn parse_named_color_16bit(name: &str) -> Option<(i64, i64, i64)> {
    let color = crate::face::Color::from_name(name)?;
    Some((
        i64::from(color.r) * 257,
        i64::from(color.g) * 257,
        i64::from(color.b) * 257,
    ))
}

fn approximate_tty_color((r, g, b): (i64, i64, i64)) -> (i64, i64, i64) {
    // Emacs batch/TTY behavior is effectively a coarse 8-color quantization.
    // A narrow channel spread is treated as gray, otherwise channels are
    // quantized relative to the local min/max midpoint.
    const GRAY_BAND: i64 = 0x1111;
    const BRIGHT_THRESHOLD: i64 = 0x8888;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max - min <= GRAY_BAND {
        return if max >= BRIGHT_THRESHOLD {
            (65535, 65535, 65535)
        } else {
            (0, 0, 0)
        };
    }

    let mid = (max + min) / 2;
    (
        if r >= mid { 65535 } else { 0 },
        if g >= mid { 65535 } else { 0 },
        if b >= mid { 65535 } else { 0 },
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn invalid_get_device_terminal_error(value: &Value) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "Invalid argument {} in 'get-device-terminal'",
            super::print::print_value(value)
        ))],
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn color_device_designator_p(value: &Value) -> bool {
    match value.kind() {
        ValueKind::Nil => true,
        _ => frame_device_designator_p(value),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_optional_color_device_arg(args: &[Value], idx: usize) -> Result<(), Flow> {
    if let Some(value) = args.get(idx)
        && !color_device_designator_p(value)
    {
        return Err(invalid_get_device_terminal_error(value));
    }
    Ok(())
}

/// `(defined-colors &optional FRAME)` -- return a list of defined color names.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_defined_colors(args: Vec<Value>) -> EvalResult {
    expect_max_args("defined-colors", &args, 1)?;
    expect_optional_color_device_arg(&args, 0)?;
    let colors = vec![
        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
    ];
    Ok(Value::list(colors.into_iter().map(Value::string).collect()))
}

/// `(face-id FACE &optional FRAME)` -- return numeric face id for known and
/// dynamically created faces.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_face_id(args: Vec<Value>) -> EvalResult {
    expect_min_args("face-id", &args, 1)?;
    expect_max_args("face-id", &args, 2)?;
    if args[0].is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    }

    if let Some(name) = symbol_name_for_face_value(&args[0]) {
        if let Some(id) = face_id_for_name(&name) {
            return Ok(Value::fixnum(id));
        }
        if is_created_lisp_face(&name) {
            ensure_dynamic_face_id(&name);
            if let Some(id) = face_id_for_name(&name) {
                return Ok(Value::fixnum(id));
            }
        }
    }
    let rendered = super::print::print_value(&args[0]);
    Err(signal(
        "error",
        vec![Value::string(format!("Not a face: {rendered}"))],
    ))
}

pub(crate) fn builtin_face_font(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("face-font", &args, 1)?;
    expect_max_args("face-font", &args, 3)?;

    let defaults_frame = args.get(1).is_some_and(|v| v.is_t());
    if defaults_frame {
        let face_name = resolve_face_name_for_domain(eval, &args[0], true)?;
        let mut styles = Vec::new();
        let weight = lisp_face_attribute_value(&face_name, LFaceAttr::Weight, true);
        if matches!(weight.as_symbol_name(), Some(name) if name != "normal" && name != "unspecified")
        {
            styles.push(Value::symbol("bold"));
        }
        let slant = lisp_face_attribute_value(&face_name, LFaceAttr::Slant, true);
        if matches!(slant.as_symbol_name(), Some(name) if name != "normal" && name != "unspecified")
        {
            styles.push(Value::symbol("italic"));
        }
        return if styles.is_empty() {
            Ok(Value::NIL)
        } else {
            Ok(Value::list(styles))
        };
    }

    let frame_id = match args.get(1) {
        None => super::window_cmds::ensure_selected_frame_id(eval),
        Some(v) if v.is_nil() => super::window_cmds::ensure_selected_frame_id(eval),
        Some(frame) if live_frame_designator_in_state(&eval.frames, frame) => {
            frame_id_from_designator(frame)
                .expect("live frame designator should decode to frame id")
        }
        Some(other) => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("frame-live-p"), *other],
            ));
        }
    };
    let frame = eval
        .frames
        .get(frame_id)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
    if frame.window_system.is_none() {
        // GNU `Fface_font` (xfaces.c) calls `lookup_named_face (..., true)`,
        // which signals "Invalid face" only when the name does not resolve to
        // any face. A face created at runtime via `make-face`/`defface` is a
        // valid (but on a TTY frame, unrealized) face, so it must return nil
        // rather than error. Use the full existence check, not just the
        // bootstrap built-in table.
        return match resolve_face_designator(eval, args[0], FaceAliasCyclePolicy::Signal)? {
            ResolvedFaceDesignator::Symbol(name) | ResolvedFaceDesignator::String(name) => {
                if face_exists_for_domain(name.name(), false) {
                    Ok(Value::NIL)
                } else if name.symbol().is_nil() {
                    Err(signal("error", vec![Value::string("Invalid face")]))
                } else {
                    Err(signal(
                        "error",
                        vec![Value::string("Invalid face"), name.symbol()],
                    ))
                }
            }
            ResolvedFaceDesignator::Other(value) => {
                Err(signal("error", vec![Value::string("Invalid face"), value]))
            }
        };
    }

    let face_name = resolve_face_name_for_domain(eval, &args[0], false)?;
    let remapping = face_remapping_for_current_buffer(eval);
    let face = if remapping.is_empty() {
        eval.face_table.resolve(&face_name)
    } else {
        eval.face_table
            .resolve_with_remapping(&face_name, &remapping)
    };
    if let Some(character) = args.get(2).filter(|value| !value.is_nil()) {
        let code = super::builtins::expect_character_code(character)? as u32;
        let Some(ch) = crate::emacs_core::emacs_char::EmacsChar::from_code(code) else {
            return Ok(font_name_for_face(&face));
        };
        let fontset_base_face = eval.face_table.resolve("default");
        if let Some(matched) = resolve_font_match(eval, frame_id, ch, &face, &fontset_base_face) {
            return Ok(
                font_name_value(&opened_font_from_resolved_match(&face, &matched))
                    .unwrap_or(Value::NIL),
            );
        }
    }

    Ok(font_name_for_face(&face))
}

/// `(internal-face-x-get-resource RESOURCE CLASS FRAME)` -- validate arguments and
/// return nil (font resource lookup is not implemented).
pub(crate) fn builtin_internal_face_x_get_resource(args: Vec<Value>) -> EvalResult {
    expect_min_args("internal-face-x-get-resource", &args, 2)?;
    expect_max_args("internal-face-x-get-resource", &args, 3)?;
    for arg in args.iter().take(2) {
        if !arg.is_string() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), *arg],
            ));
        }
    }
    Ok(Value::NIL)
}

/// `(internal-set-font-selection-order ORDER)` -- validate order list shape and return nil.
pub(crate) fn builtin_internal_set_font_selection_order(args: Vec<Value>) -> EvalResult {
    expect_args("internal-set-font-selection-order", &args, 1)?;
    let order = &args[0];
    if !order.is_nil() && !order.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *order],
        ));
    }

    let valid_keywords = [":width", ":height", ":weight", ":slant"];
    let valid = if let Some(values) = list_to_vec(order) {
        if values.len() == valid_keywords.len() {
            let mut seen = HashSet::default();
            values.iter().all(|value| {
                if let Some(id) = value.as_keyword_id() {
                    let s = resolve_sym(id);
                    let key = if s.starts_with(':') {
                        s.to_owned()
                    } else {
                        format!(":{s}")
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
        return Ok(Value::NIL);
    }

    if let Some(values) = list_to_vec(order) {
        if values.is_empty() {
            return Err(signal(
                "error",
                vec![Value::string("Invalid font sort order")],
            ));
        }
        let mut payload = vec![Value::string("Invalid font sort order")];
        payload.extend(values);
        return Err(signal("error", payload));
    }

    Err(signal(
        "error",
        vec![Value::string("Invalid font sort order"), *order],
    ))
}

/// `(internal-set-alternative-font-family-alist ALIST)` -- normalize string
/// entries to symbols and return the normalized list.
pub(crate) fn builtin_internal_set_alternative_font_family_alist(args: Vec<Value>) -> EvalResult {
    expect_args("internal-set-alternative-font-family-alist", &args, 1)?;
    let entries = proper_list_to_vec_or_listp_error(&args[0])?;
    let mut normalized = Vec::with_capacity(entries.len());
    let mut alist = Vec::with_capacity(entries.len());
    for entry in entries {
        let members = proper_list_to_vec_or_listp_error(&entry)?;
        let mut converted = Vec::with_capacity(members.len());
        let mut names = Vec::with_capacity(members.len());
        for member in members {
            match member.kind() {
                ValueKind::String => {
                    // Issue #131: intern the family name faithfully (real Emacs
                    // bytes) rather than via the PUA-sentinel storage form.
                    let sym = crate::emacs_core::intern::intern_lisp_string(
                        member.as_lisp_string().expect("checked string"),
                    );
                    converted.push(Value::from_sym_id(sym));
                    names.push(sym);
                }
                _other => {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("stringp"), member],
                    ));
                }
            }
        }
        if let Some(name) = names.first().copied() {
            alist.push((name, names));
        }
        normalized.push(Value::list(converted));
    }
    if let Ok(mut state) = alternative_font_family_alist().write() {
        *state = alist;
    }
    clear_font_cache_state();
    Ok(Value::list(normalized))
}

/// `(internal-set-alternative-font-registry-alist ALIST)` -- downcase string
/// entries and return the normalized list.
pub(crate) fn builtin_internal_set_alternative_font_registry_alist(args: Vec<Value>) -> EvalResult {
    expect_args("internal-set-alternative-font-registry-alist", &args, 1)?;
    let entries = proper_list_to_vec_or_listp_error(&args[0])?;
    let mut normalized = Vec::with_capacity(entries.len());
    let mut alist = Vec::with_capacity(entries.len());
    for entry in entries {
        let members = proper_list_to_vec_or_listp_error(&entry)?;
        let mut converted = Vec::with_capacity(members.len());
        let mut names = Vec::with_capacity(members.len());
        for member in members {
            let downcased = crate::emacs_core::builtins::builtin_downcase(vec![member])?;
            if let Some(text) = downcased.as_lisp_string() {
                names.push(text.clone());
            }
            converted.push(downcased);
        }
        if names.len() == converted.len()
            && let Some(name) = names.first().cloned()
        {
            alist.push((name, names));
        }
        normalized.push(Value::list(converted));
    }
    if let Ok(mut state) = alternative_font_registry_alist().write() {
        *state = alist;
    }
    clear_font_cache_state();
    Ok(Value::list(normalized))
}

// ---------------------------------------------------------------------------
// xfaces.c: x-load-color-file
// ---------------------------------------------------------------------------

/// `(x-load-color-file FILENAME)` — read an RGB color file (rgb.txt format)
/// and return an alist of `(NAME R G B)` entries.
///
/// Each line has the format `R G B  name` where R/G/B are 0-255 decimal.
/// Lines starting with `!` or `#` are comments and are skipped.
pub(crate) fn builtin_x_load_color_file(args: Vec<Value>) -> EvalResult {
    expect_args("x-load-color-file", &args, 1)?;
    let filename = match args[0].kind() {
        ValueKind::String => font_string_text(&args[0]).expect("checked string"),
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[0]],
            ));
        }
    };

    // Expand the filename (resolve ~, relative paths, etc.)
    let expanded = super::fileio::expand_file_name(&filename, None);
    let contents = match std::fs::read_to_string(&expanded) {
        Ok(s) => s,
        Err(_) => return Ok(Value::NIL),
    };

    let mut result = Value::NIL;
    // Build alist in reverse order, then reverse (or build in correct order
    // by collecting into vec and reversing).
    let mut entries: Vec<Value> = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('!') || trimmed.starts_with('#') {
            continue;
        }
        // Parse: R G B  color-name
        let mut parts = trimmed.splitn(4, char::is_whitespace);
        let r_str = match parts.next() {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };
        // Skip whitespace between fields
        let g_str = loop {
            match parts.next() {
                Some(s) if !s.is_empty() => break s,
                Some(_) => continue,
                None => break "",
            }
        };
        if g_str.is_empty() {
            continue;
        }
        let b_str = loop {
            match parts.next() {
                Some(s) if !s.is_empty() => break s,
                Some(_) => continue,
                None => break "",
            }
        };
        if b_str.is_empty() {
            continue;
        }
        let name_part = loop {
            match parts.next() {
                Some(s) if !s.is_empty() => break s,
                Some(_) => continue,
                None => break "",
            }
        };
        if name_part.is_empty() {
            continue;
        }

        let r: u16 = match r_str.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let g: u16 = match g_str.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let b: u16 = match b_str.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Scale 0-255 to 0-65535 (same as Emacs: val * 257)
        let r16 = (r as i64) * 257;
        let g16 = (g as i64) * 257;
        let b16 = (b as i64) * 257;

        // Build (NAME R G B) as a proper list
        let color_entry = Value::cons(
            Value::string(name_part),
            Value::cons(
                Value::fixnum(r16),
                Value::cons(
                    Value::fixnum(g16),
                    Value::cons(Value::fixnum(b16), Value::NIL),
                ),
            ),
        );
        entries.push(color_entry);
    }

    // Build alist from entries (preserve file order)
    for entry in entries.into_iter().rev() {
        result = Value::cons(entry, result);
    }

    Ok(result)
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/builtins.rs"]
mod builtins_test;

#[cfg(test)]
#[path = "tests/font_size_test.rs"]
mod font_size_test;
