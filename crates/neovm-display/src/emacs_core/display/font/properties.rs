//! Font basic/native slots and GC-visible GNU FONT_EXTRA association lists.
//!
//! Legacy property vectors have a tag and complete key/value pairs. New
//! storage appends one cons root (nil . EXTRA), so the vector has even length.
//! Each font owns that root; its list spine and association cells may be shared.
use super::*;
use neovm_value::tagged::SymbolMode;

pub(super) enum FontProperties<'a> {
    LegacyFlat(&'a [Value]),
    WithExtraRoot { flat: &'a [Value], root: Value },
}

impl<'a> FontProperties<'a> {
    pub(super) fn decode(fields: &'a [Value]) -> Self {
        if fields.len() >= 2
            && fields.len().is_multiple_of(2)
            && let Some(&root) = fields.last()
            && root.is_cons()
            && root.cons_car().is_nil()
        {
            Self::WithExtraRoot {
                flat: &fields[..fields.len() - 1],
                root,
            }
        } else {
            Self::LegacyFlat(fields)
        }
    }
    pub(super) fn flat(&self) -> &'a [Value] {
        match self {
            Self::LegacyFlat(flat) | Self::WithExtraRoot { flat, .. } => flat,
        }
    }
    pub(super) fn root(&self) -> Option<Value> {
        match self {
            Self::LegacyFlat(_) => None,
            Self::WithExtraRoot { root, .. } => Some(*root),
        }
    }
    pub(super) fn extras(&self) -> Option<Value> {
        self.root().map(Value::cons_cdr)
    }
}

const BASIC: [&str; 12] = [
    "type", "foundry", "family", "adstyle", "registry", "weight", "slant", "width", "size", "dpi",
    "spacing", "avgwidth",
];
const CANONICAL: [(&str, &str); 16] = [
    (":type", "type"),
    (":foundry", "foundry"),
    (":family", "family"),
    (":adstyle", "adstyle"),
    (":registry", "registry"),
    (":weight", "weight"),
    (":slant", "slant"),
    (":width", "width"),
    (":size", "size"),
    (":dpi", "dpi"),
    (":spacing", "spacing"),
    (":avgwidth", "avgwidth"),
    (":lang", "lang"),
    (":script", "script"),
    (":otf", "otf"),
    (":name", "name"),
];

pub(super) fn canonical_name(key: Value, mode: SymbolMode) -> Option<&'static str> {
    let id = key.as_symbol_id(mode)?;
    CANONICAL
        .into_iter()
        .find_map(|(key, name)| (intern(key) == id).then_some(name))
}

pub(super) fn basic(key: Value, mode: SymbolMode) -> bool {
    canonical_name(key, mode).is_some_and(|name| BASIC.contains(&name))
}

pub(super) fn spec_only(key: Value, mode: SymbolMode) -> bool {
    canonical_name(key, mode).is_some()
}

fn same(left: Value, right: Value, mode: SymbolMode) -> bool {
    eq_value(&mode.unwrap(left), &mode.unwrap(right))
}

pub(super) fn assoc(mut extras: Value, key: Value, mode: SymbolMode) -> Option<Value> {
    while extras.is_cons() {
        let cell = extras.cons_car();
        if cell.is_cons() && same(cell.cons_car(), key, mode) {
            return Some(cell);
        }
        extras = extras.cons_cdr();
    }
    None
}

fn flat_get(flat: &[Value], key: Value, mode: SymbolMode) -> Option<Value> {
    flat.get(1..)
        .unwrap_or_default()
        .chunks_exact(2)
        .find_map(|pair| same(pair[0], key, mode).then_some(pair[1]))
}

pub(super) fn get(fields: &[Value], key: Value, mode: SymbolMode) -> Option<Value> {
    let view = FontProperties::decode(fields);
    let native = flat_get(view.flat(), key, mode);
    if basic(key, mode) {
        return native;
    }
    if key.as_symbol_id(mode) == Some(intern(":neomacs-entity-handle"))
        && !fields
            .first()
            .and_then(|tag| tag.as_bare_symbol_name())
            .is_some_and(|name| {
                name.trim_start_matches(':') == FONT_SPEC_TAG.trim_start_matches(':')
            })
    {
        // Neomacs owns this extension slot. User extras cannot provide an
        // entity/object identity, including when there is no native handle.
        return native;
    }
    match view {
        FontProperties::LegacyFlat(_) => native,
        FontProperties::WithExtraRoot { root, .. } => {
            assoc(root.cons_cdr(), key, mode).map(Value::cons_cdr)
        }
    }
}

pub(super) fn named(fields: &[Value], name: &str) -> Option<Value> {
    named_with_mode(fields, name, SymbolMode::Bare)
}

pub(super) fn named_with_mode(fields: &[Value], name: &str, mode: SymbolMode) -> Option<Value> {
    let name = name.trim_start_matches(':');
    let key = Value::keyword(name);
    if FontProperties::decode(fields).root().is_some() {
        // New font values use exact canonical keywords for native constraints.
        // Bare and uninterned lookalikes remain ordinary user extras.
        return flat_get(FontProperties::decode(fields).flat(), key, mode)
            .or_else(|| get(fields, key, mode));
    }
    legacy_named(fields, name)
}

fn legacy_named(fields: &[Value], name: &str) -> Option<Value> {
    fields
        .get(1..)
        .unwrap_or_default()
        .chunks_exact(2)
        .find_map(|pair| {
            let key = pair[0].as_bare_symbol_name()?;
            (key.trim_start_matches(':') == name).then_some(pair[1])
        })
}

pub(super) fn native_named(fields: &[Value], name: &str) -> Option<Value> {
    let view = FontProperties::decode(fields);
    if view.root().is_some() {
        flat_get(view.flat(), Value::keyword(name), SymbolMode::Bare)
    } else {
        legacy_named(fields, name.trim_start_matches(':'))
    }
}

pub(super) fn set_native(fields: &mut Vec<Value>, key: Value, value: Value, mode: SymbolMode) {
    let flat_len = FontProperties::decode(fields).flat().len();
    if let Some(index) = fields[1..flat_len]
        .chunks_exact(2)
        .position(|pair| same(pair[0], key, mode))
        .map(|index| 1 + 2 * index)
    {
        fields[index + 1] = value;
        return;
    }
    let root = (flat_len != fields.len()).then(|| fields.pop().unwrap());
    fields.extend([key, value]);
    if let Some(root) = root {
        fields.push(root);
    }
}

pub(super) fn replace_root(fields: &mut Vec<Value>, extras: Value) {
    if FontProperties::decode(fields).root().is_some() {
        fields.pop();
    }
    fields.push(Value::cons(Value::NIL, extras));
}

pub(super) fn ensure_root(fields: &mut Vec<Value>, mode: SymbolMode) -> Value {
    if let Some(root) = FontProperties::decode(fields).root() {
        return root;
    }
    let complete = 1 + fields.len().saturating_sub(1) / 2 * 2;
    fields.truncate(complete);
    let root = Value::cons(Value::NIL, Value::NIL);
    // A legacy spec may contain old flat user properties. Move, rather than
    // duplicate, those references; native entity/object overlays stay flat.
    if fields
        .first()
        .and_then(|tag| tag.as_bare_symbol_name())
        .is_some_and(|name| name.trim_start_matches(':') == FONT_SPEC_TAG.trim_start_matches(':'))
    {
        let mut flat = vec![fields[0]];
        for pair in fields[1..].chunks_exact(2) {
            if basic(pair[0], mode) {
                flat.extend_from_slice(pair);
            } else if assoc(root.cons_cdr(), pair[0], mode).is_none() {
                set_extra(root, pair[0], pair[1], mode);
            }
        }
        *fields = flat;
    }
    fields.push(root);
    root
}

fn chars(text: &LispString) -> impl Iterator<Item = u32> + '_ {
    let mut bytes = text.as_bytes();
    std::iter::from_fn(move || {
        let first = *bytes.first()?;
        let (code, length) = if text.is_multibyte() {
            let (code, length) = neovm_value::emacs_char::string_char(bytes);
            (code.code(), length.max(1))
        } else {
            (u32::from(first), 1)
        };
        bytes = &bytes[length..];
        Some(code)
    })
}

fn less(left: Value, right: Value, mode: SymbolMode) -> bool {
    match (
        left.as_symbol_lisp_string(mode),
        right.as_symbol_lisp_string(mode),
    ) {
        (Some(left), Some(right)) => chars(left).cmp(chars(right)).is_lt(),
        _ => false,
    }
}

pub(super) fn set_extra(root: Value, key: Value, value: Value, mode: SymbolMode) {
    if let Some(cell) = assoc(root.cons_cdr(), key, mode) {
        cell.set_cdr(value);
        return;
    }
    let mut tail = root.cons_cdr();
    let mut previous = None;
    while tail.is_cons() && !less(key, tail.cons_car().cons_car(), mode) {
        previous = Some(tail);
        tail = tail.cons_cdr();
    }
    let entry = Value::cons(Value::cons(key, value), tail);
    if let Some(previous) = previous {
        previous.set_cdr(entry);
    } else {
        root.set_cdr(entry);
    }
}

pub(super) fn remove_extra(root: Value, key: Value, mode: SymbolMode) {
    let mut tail = root.cons_cdr();
    let mut previous: Option<Value> = None;
    while tail.is_cons() {
        let cell = tail.cons_car();
        if same(cell.cons_car(), key, mode) {
            if let Some(previous) = previous {
                previous.set_cdr(tail.cons_cdr());
            } else {
                root.set_cdr(tail.cons_cdr());
            }
            return;
        }
        previous = Some(tail);
        tail = tail.cons_cdr();
    }
}

pub(super) fn set_user(fields: &mut Vec<Value>, key: Value, value: Value, mode: SymbolMode) {
    if basic(key, mode) {
        set_native(
            fields,
            Value::from_sym_id(key.as_symbol_id(mode).unwrap()),
            value,
            mode,
        );
    } else {
        let root = ensure_root(fields, mode);
        set_extra(root, key, value, mode);
    }
}

pub(super) fn copy_outer(mut extras: Value) -> Value {
    let mut cells = Vec::new();
    while extras.is_cons() {
        cells.push(extras.cons_car());
        extras = extras.cons_cdr();
    }
    Value::list(cells)
}

pub(super) fn copy_cells(mut extras: Value) -> Value {
    let mut cells = Vec::new();
    while extras.is_cons() {
        let cell = extras.cons_car();
        cells.push(Value::cons(cell.cons_car(), cell.cons_cdr()));
        extras = extras.cons_cdr();
    }
    Value::list(cells)
}
