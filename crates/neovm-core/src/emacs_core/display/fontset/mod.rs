use super::charset::{charset_contains_char, charset_exists, charset_target_ranges};
use super::chartable::{for_each_non_nil_char_table_run, is_char_table};
use super::error::{Flow, signal};
use super::intern::{SymId, intern, resolve_sym, resolve_sym_lisp_string};
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::face::{FontSlant, FontWeight, FontWidth};
use crate::heap_types::LispString;
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use strum::{EnumString, IntoStaticStr};

pub const DEFAULT_FONTSET_NAME: &str = "-*-*-*-*-*-*-*-*-*-*-*-*-fontset-default";
pub const DEFAULT_FONTSET_ALIAS: &str = "fontset-default";

fn fontset_string_text(value: &Value) -> Option<String> {
    // Issue #131: read the value's real Emacs bytes (lossy UTF-8 view) rather than
    // the PUA-sentinel storage form. The remaining callers handle ASCII content
    // (XLFD fontset names, registry/lang/style codes), where this is exact; raw
    // font-family names are interned faithfully via intern_font_name_value.
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
}

fn fontset_name_lisp_string(name: &str) -> LispString {
    LispString::from_utf8(name)
}

fn fontset_name_runtime(name: &LispString) -> String {
    crate::emacs_core::emacs_char::to_utf8_lossy(name.as_bytes())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredFontSpec {
    pub family: Option<SymId>,
    pub registry: Option<SymId>,
    pub lang: Option<SymId>,
    pub weight: Option<FontWeight>,
    pub slant: Option<FontSlant>,
    pub width: Option<FontWidth>,
    pub repertory: Option<FontRepertory>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontSpecEntry {
    Font(StoredFontSpec),
    ExplicitNone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontRepertory {
    Charset(SymId),
    CharTableRanges(Vec<(u32, u32)>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum FontsetTarget {
    Range(u32, u32),
    Fallback,
}

#[derive(Clone, Debug)]
struct RangeEntry {
    from: u32,
    to: u32,
    entries: Vec<FontSpecEntry>,
}

#[derive(Clone, Debug, Default)]
struct FontsetData {
    ranges: Vec<RangeEntry>,
    fallback: Option<Vec<FontSpecEntry>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FontsetRangeEntrySnapshot {
    pub from: u32,
    pub to: u32,
    pub entries: Vec<FontSpecEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FontsetDataSnapshot {
    pub ranges: Vec<FontsetRangeEntrySnapshot>,
    pub fallback: Option<Vec<FontSpecEntry>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FontsetRegistrySnapshot {
    pub ordered_names: Vec<LispString>,
    pub alias_to_name: Vec<(LispString, LispString)>,
    pub fontsets: Vec<(LispString, FontsetDataSnapshot)>,
    pub generation: u64,
}

#[derive(Clone, Debug)]
struct FontsetRegistry {
    ordered_names: Vec<LispString>,
    alias_to_name: HashMap<LispString, LispString>,
    fontsets: HashMap<LispString, FontsetData>,
    generation: u64,
}

impl FontsetRegistry {
    // Fontset names are canonical Lisp strings owned by this registry; their
    // GC-aware representation is the required equality key.
    #[allow(clippy::mutable_key_type)]
    fn with_defaults() -> Self {
        let mut alias_to_name = HashMap::new();
        let default_alias = fontset_name_lisp_string(DEFAULT_FONTSET_ALIAS);
        let default_name = fontset_name_lisp_string(DEFAULT_FONTSET_NAME);
        alias_to_name.insert(default_alias, default_name.clone());
        let mut fontsets = HashMap::new();
        fontsets.insert(default_name.clone(), FontsetData::default());
        Self {
            ordered_names: vec![default_name],
            alias_to_name,
            fontsets,
            generation: 1,
        }
    }

    fn resolve_literal(&self, name: &str) -> Option<LispString> {
        let wanted = fontset_name_lisp_string(name);
        if self
            .ordered_names
            .iter()
            .any(|candidate| candidate == &wanted)
        {
            Some(wanted)
        } else {
            self.alias_to_name.get(&wanted).cloned()
        }
    }

    fn ensure_fontset(&mut self, name: &LispString) {
        self.fontsets.entry(name.clone()).or_default();
        if !self.ordered_names.iter().any(|candidate| candidate == name) {
            self.ordered_names.push(name.clone());
        }
    }

    fn register_fontset(&mut self, name: LispString, alias: Option<LispString>) -> LispString {
        self.ensure_fontset(&name);
        if let Some(alias_name) = alias {
            self.alias_to_name.insert(alias_name, name.clone());
        }
        name
    }

    fn replace_rules(
        &mut self,
        name: &LispString,
        rules: Vec<(FontsetTarget, Vec<FontSpecEntry>)>,
    ) {
        self.ensure_fontset(name);
        let mut data = FontsetData::default();
        for (target, entries) in rules {
            for entry in entries {
                data.update_target(target.clone(), entry, FontsetAddMode::Append);
            }
        }
        self.fontsets.insert(name.clone(), data);
        self.generation = self.generation.wrapping_add(1);
    }

    fn update_target(
        &mut self,
        name: &LispString,
        target: FontsetTarget,
        entry: FontSpecEntry,
        add: FontsetAddMode,
    ) {
        self.ensure_fontset(name);
        let data = self.fontsets.entry(name.clone()).or_default();
        data.update_target(target, entry, add);
        self.generation = self.generation.wrapping_add(1);
    }

    fn list_value(&self) -> Value {
        Value::list(
            self.ordered_names
                .iter()
                .cloned()
                .map(Value::heap_string)
                .collect(),
        )
    }

    fn alias_alist_value(&self) -> Value {
        let mut entries = Vec::new();
        for name in &self.ordered_names {
            for (alias, canonical) in &self.alias_to_name {
                if canonical == name {
                    entries.push(Value::cons(
                        Value::heap_string(name.clone()),
                        Value::heap_string(alias.clone()),
                    ));
                }
            }
        }
        Value::list(entries)
    }

    fn matching_entries_for_char(&self, name: &LispString, ch: char) -> Vec<FontSpecEntry> {
        let code = ch as u32;
        let Some(data) = self.fontsets.get(name) else {
            return Vec::new();
        };

        let mut entries = data.matching_entries_for_char(code);
        if entries.is_empty()
            && *name != fontset_name_lisp_string(DEFAULT_FONTSET_NAME)
            && let Some(default) = self
                .fontsets
                .get(&fontset_name_lisp_string(DEFAULT_FONTSET_NAME))
        {
            entries = default.matching_entries_for_char(code);
        }
        entries
    }
}

impl FontsetData {
    fn matching_entries_for_char(&self, code: u32) -> Vec<FontSpecEntry> {
        let mut entries = filter_entries_for_char(self.specific_entries_for_char(code), code);
        if let Some(fallback) = &self.fallback {
            entries.extend(filter_entries_for_char(fallback.clone(), code));
        }
        entries
    }

    fn update_target(&mut self, target: FontsetTarget, entry: FontSpecEntry, add: FontsetAddMode) {
        match target {
            FontsetTarget::Fallback => self.update_fallback(entry, add),
            FontsetTarget::Range(from, to) => self.update_range(from, to, entry, add),
        }
    }

    fn specific_entries_for_char(&self, code: u32) -> Vec<FontSpecEntry> {
        self.find_range(code)
            .map(|range| range.entries.clone())
            .unwrap_or_default()
    }

    fn find_range(&self, code: u32) -> Option<&RangeEntry> {
        let mut low = 0usize;
        let mut high = self.ranges.len();
        while low < high {
            let mid = low + (high - low) / 2;
            let range = &self.ranges[mid];
            if code < range.from {
                high = mid;
            } else if code > range.to {
                low = mid + 1;
            } else {
                return Some(range);
            }
        }
        None
    }

    fn update_fallback(&mut self, entry: FontSpecEntry, add: FontsetAddMode) {
        self.fallback = Some(apply_fontset_add(self.fallback.as_deref(), entry, add));
    }

    fn update_range(&mut self, from: u32, to: u32, entry: FontSpecEntry, add: FontsetAddMode) {
        let mut next = Vec::with_capacity(self.ranges.len() + 2);
        let mut cursor = from;

        for range in &self.ranges {
            if range.to < from {
                push_range_entry(&mut next, range.clone());
                continue;
            }
            if range.from > to {
                if cursor <= to {
                    push_range_entry(
                        &mut next,
                        RangeEntry {
                            from: cursor,
                            to,
                            entries: apply_fontset_add(None, entry.clone(), add),
                        },
                    );
                    cursor = to.saturating_add(1);
                }
                push_range_entry(&mut next, range.clone());
                continue;
            }

            if range.from < from {
                push_range_entry(
                    &mut next,
                    RangeEntry {
                        from: range.from,
                        to: from - 1,
                        entries: range.entries.clone(),
                    },
                );
            }

            if cursor < range.from {
                push_range_entry(
                    &mut next,
                    RangeEntry {
                        from: cursor,
                        to: range.from - 1,
                        entries: apply_fontset_add(None, entry.clone(), add),
                    },
                );
            }

            let overlap_from = range.from.max(from);
            let overlap_to = range.to.min(to);
            push_range_entry(
                &mut next,
                RangeEntry {
                    from: overlap_from,
                    to: overlap_to,
                    entries: apply_fontset_add(Some(&range.entries), entry.clone(), add),
                },
            );
            cursor = overlap_to.saturating_add(1);

            if range.to > to {
                push_range_entry(
                    &mut next,
                    RangeEntry {
                        from: to + 1,
                        to: range.to,
                        entries: range.entries.clone(),
                    },
                );
            }
        }

        if cursor <= to {
            push_range_entry(
                &mut next,
                RangeEntry {
                    from: cursor,
                    to,
                    entries: apply_fontset_add(None, entry, add),
                },
            );
        }

        self.ranges = next;
    }
}

impl StoredFontSpec {
    fn matches_char(&self, code: u32) -> bool {
        self.repertory
            .as_ref()
            .is_none_or(|repertory| repertory.matches_char(code))
    }
}

impl FontRepertory {
    fn matches_char(&self, code: u32) -> bool {
        match self {
            // GNU filters by charset repertory here. When Neomacs' charset
            // engine cannot yet answer membership for map/subset/superset
            // charsets, keep the candidate instead of producing a false
            // negative and dropping a valid font.
            Self::Charset(name) => charset_contains_char(resolve_sym(*name), code).unwrap_or(true),
            Self::CharTableRanges(ranges) => {
                ranges.iter().any(|(from, to)| code >= *from && code <= *to)
            }
        }
    }
}

pub fn repertory_target_ranges(repertory: &FontRepertory) -> Option<Vec<(u32, u32)>> {
    match repertory {
        FontRepertory::Charset(name) => charset_target_ranges(resolve_sym(*name)),
        FontRepertory::CharTableRanges(ranges) => Some(ranges.clone()),
    }
}

fn filter_entries_for_char(entries: Vec<FontSpecEntry>, code: u32) -> Vec<FontSpecEntry> {
    entries
        .into_iter()
        .filter(|entry| match entry {
            FontSpecEntry::ExplicitNone => true,
            FontSpecEntry::Font(spec) => spec.matches_char(code),
        })
        .collect()
}

fn apply_fontset_add(
    existing: Option<&[FontSpecEntry]>,
    entry: FontSpecEntry,
    add: FontsetAddMode,
) -> Vec<FontSpecEntry> {
    match add {
        FontsetAddMode::Overwrite => vec![entry],
        FontsetAddMode::Append => {
            let mut entries = existing.map(ToOwned::to_owned).unwrap_or_default();
            entries.push(entry);
            entries
        }
        FontsetAddMode::Prepend => {
            let mut entries = vec![entry];
            if let Some(existing) = existing {
                entries.extend_from_slice(existing);
            }
            entries
        }
    }
}

fn push_range_entry(ranges: &mut Vec<RangeEntry>, entry: RangeEntry) {
    if entry.from > entry.to {
        return;
    }
    if let Some(last) = ranges.last_mut()
        && last.entries == entry.entries
        && last.to.checked_add(1) == Some(entry.from)
    {
        last.to = entry.to;
        return;
    }
    ranges.push(entry);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
enum FontsetAddMode {
    #[strum(to_string = "overwrite")]
    Overwrite,
    #[strum(to_string = "append", serialize = ":append")]
    Append,
    #[strum(to_string = "prepend", serialize = ":prepend")]
    Prepend,
}

impl FontsetAddMode {
    fn from_lisp_value(add: Option<&Value>) -> Self {
        add.and_then(|value| value.as_symbol_name())
            .and_then(|name| name.parse().ok())
            .unwrap_or(Self::Overwrite)
    }
}

static FONTSET_REGISTRY: OnceLock<RwLock<FontsetRegistry>> = OnceLock::new();

fn registry() -> &'static RwLock<FontsetRegistry> {
    FONTSET_REGISTRY.get_or_init(|| RwLock::new(FontsetRegistry::with_defaults()))
}

pub(crate) fn reset_fontset_registry() {
    if let Ok(mut slot) = registry().write() {
        *slot = FontsetRegistry::with_defaults();
    }
}

pub(crate) fn snapshot_fontset_registry() -> FontsetRegistrySnapshot {
    registry()
        .read()
        .map(|slot| {
            let mut alias_to_name: Vec<_> = slot
                .alias_to_name
                .iter()
                .map(|(alias, name)| (alias.clone(), name.clone()))
                .collect();
            alias_to_name.sort_by(|(left_alias, left_name), (right_alias, right_name)| {
                left_alias
                    .as_bytes()
                    .cmp(right_alias.as_bytes())
                    .then_with(|| left_name.as_bytes().cmp(right_name.as_bytes()))
            });

            let mut fontsets: Vec<_> = slot
                .fontsets
                .iter()
                .map(|(name, data)| {
                    (
                        name.clone(),
                        FontsetDataSnapshot {
                            ranges: data
                                .ranges
                                .iter()
                                .map(|range| FontsetRangeEntrySnapshot {
                                    from: range.from,
                                    to: range.to,
                                    entries: range.entries.clone(),
                                })
                                .collect(),
                            fallback: data.fallback.clone(),
                        },
                    )
                })
                .collect();
            fontsets.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));

            FontsetRegistrySnapshot {
                ordered_names: slot.ordered_names.clone(),
                alias_to_name,
                fontsets,
                generation: slot.generation,
            }
        })
        .unwrap_or_else(|_| FontsetRegistrySnapshot {
            ordered_names: vec![fontset_name_lisp_string(DEFAULT_FONTSET_NAME)],
            alias_to_name: vec![(
                fontset_name_lisp_string(DEFAULT_FONTSET_ALIAS),
                fontset_name_lisp_string(DEFAULT_FONTSET_NAME),
            )],
            fontsets: vec![(
                fontset_name_lisp_string(DEFAULT_FONTSET_NAME),
                FontsetDataSnapshot::default(),
            )],
            generation: 1,
        })
}

#[allow(clippy::mutable_key_type)] // reconstructs maps keyed by canonical Lisp strings
pub(crate) fn restore_fontset_registry(snapshot: FontsetRegistrySnapshot) {
    let alias_to_name = snapshot.alias_to_name.into_iter().collect();
    let fontsets = snapshot
        .fontsets
        .into_iter()
        .map(|(name, data)| {
            (
                name,
                FontsetData {
                    ranges: data
                        .ranges
                        .into_iter()
                        .map(|range| RangeEntry {
                            from: range.from,
                            to: range.to,
                            entries: range.entries,
                        })
                        .collect(),
                    fallback: data.fallback,
                },
            )
        })
        .collect();
    let restored = FontsetRegistry {
        ordered_names: snapshot.ordered_names,
        alias_to_name,
        fontsets,
        generation: snapshot.generation.max(1),
    };
    if let Ok(mut slot) = registry().write() {
        *slot = restored;
    }
}

pub fn fontset_generation() -> u64 {
    registry().read().map(|slot| slot.generation).unwrap_or(0)
}

/// Mutation generation for `char-script-table` snapshots consumed outside
/// neovm-core. GNU's `face_for_char` consults that live table before fontset
/// fallback, so cached layout-side classifiers must observe its mutations.
pub fn char_script_table_generation() -> u64 {
    super::chartable::char_table_write_tick()
}

/// Effective ranges whose `char-script-table` value is `symbol`.
///
/// Owned numeric ranges keep the layout/font service independent of Lisp heap
/// lifetimes while preserving user changes to the live table.
pub fn symbol_script_ranges(char_script_table: Option<&Value>) -> Vec<(u32, u32)> {
    expand_script_symbol("symbol", char_script_table)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|target| match target {
            FontsetTarget::Range(from, to) => Some((from, to)),
            FontsetTarget::Fallback => None,
        })
        .collect()
}

pub(crate) fn fontset_alias_alist_startup_value() -> Value {
    registry()
        .read()
        .map(|slot| slot.alias_alist_value())
        .unwrap_or(Value::NIL)
}

pub(crate) fn fontset_list_value() -> Value {
    registry()
        .read()
        .map(|slot| slot.list_value())
        .unwrap_or(Value::NIL)
}

pub(crate) fn normalize_fontset_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

pub(crate) fn fontset_registry_alias_from_xlfd(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split('-').collect();
    if parts.len() < 15 || parts.first().copied() != Some("") {
        return None;
    }
    let registry = parts.get(parts.len() - 2)?;
    let encoding = parts.last()?;
    let alias = format!(
        "{}-{}",
        registry.to_ascii_lowercase(),
        encoding.to_ascii_lowercase()
    );
    if alias.starts_with("fontset-") && alias.len() >= 9 {
        Some(alias)
    } else {
        None
    }
}

fn wildcard_fontset_pattern_to_regexp(pattern: &str) -> LispString {
    let full_xlfd = pattern.bytes().filter(|byte| *byte == b'-').count() >= 14;
    let mut regexp = String::with_capacity(pattern.len() + 3);
    regexp.push('^');
    for ch in pattern.chars() {
        match ch {
            '*' if full_xlfd => regexp.push_str("[^-]*"),
            '*' => regexp.push_str(".*"),
            '?' => regexp.push('.'),
            '[' | '.' | '\\' | '+' | '^' | '$' => {
                regexp.push('\\');
                regexp.push(ch);
            }
            _ => regexp.push(ch),
        }
    }
    regexp.push('$');
    LispString::from_utf8(&regexp)
}

pub(crate) fn query_fontset_registry(pattern: &str, regexpp: bool) -> Option<String> {
    let pattern = normalize_fontset_name(pattern);
    registry().read().ok().and_then(|registry| {
        if regexpp || pattern.contains('*') || pattern.contains('?') {
            let regexp = if regexpp {
                LispString::from_utf8(&pattern)
            } else {
                wildcard_fontset_pattern_to_regexp(&pattern)
            };
            for name in &registry.ordered_names {
                let rendered = fontset_name_runtime(name);
                if crate::emacs_core::regex::predicate_match_ignore_case(&regexp, &rendered).ok()? {
                    return Some(rendered);
                }
            }
            for (alias, name) in &registry.alias_to_name {
                let rendered_alias = fontset_name_runtime(alias);
                if crate::emacs_core::regex::predicate_match_ignore_case(&regexp, &rendered_alias)
                    .ok()?
                {
                    return Some(fontset_name_runtime(name));
                }
            }
            return None;
        }

        if !pattern.contains('*') && !pattern.contains('?') {
            return registry
                .resolve_literal(&pattern)
                .map(|name| fontset_name_runtime(&name));
        }

        None
    })
}

pub(crate) fn resolve_fontset_name_arg(value: &Value) -> Result<String, Flow> {
    match value.kind() {
        ValueKind::Nil | ValueKind::T => Ok(DEFAULT_FONTSET_NAME.to_string()),
        ValueKind::String => {
            let requested =
                normalize_fontset_name(&fontset_string_text(value).expect("checked string"));
            if let Some(found) = query_fontset_registry(&requested, false) {
                return Ok(found);
            }
            // A plain font name is not a fontset name. The classic idiom
            // `(set-fontset-font (frame-parameter nil 'font) CHARSET FONT-SPEC)`
            // passes the frame's font; GNU's `check_fontset_name` resolves it to
            // the frame's fontset (the one used for display). neomacs renders
            // from the single default fontset, so map any font name there --
            // otherwise the spec lands on a fontset nothing consults and the
            // CJK font is silently ignored (issue #177). A real
            // `-...-fontset-NAME` XLFD (which contains "fontset") keeps its own
            // name.
            if requested.contains("fontset") {
                Ok(requested)
            } else {
                Ok(DEFAULT_FONTSET_NAME.to_string())
            }
        }
        ValueKind::Symbol(id) => {
            let requested = normalize_fontset_name(resolve_sym(id));
            Ok(query_fontset_registry(&requested, false).unwrap_or(requested))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )),
    }
}

pub fn matching_entries_for_char(ch: char) -> Vec<FontSpecEntry> {
    matching_entries_for_fontset(DEFAULT_FONTSET_NAME, ch)
}

pub fn matching_entries_for_fontset(name: &str, ch: char) -> Vec<FontSpecEntry> {
    let name = fontset_name_lisp_string(name);
    registry()
        .read()
        .map(|slot| slot.matching_entries_for_char(&name, ch))
        .unwrap_or_default()
}

pub(crate) fn fontset_font(name: &Value, ch: char, all: bool) -> Result<Value, Flow> {
    let fontset_name = resolve_fontset_name_arg(name)?;
    let entries = matching_entries_for_fontset(&fontset_name, ch);

    let mut patterns = Vec::new();
    for entry in entries {
        match entry {
            FontSpecEntry::ExplicitNone => return Ok(Value::NIL),
            FontSpecEntry::Font(spec) => {
                let family = spec
                    .family
                    .map(|sym| Value::string(resolve_sym(sym)))
                    .unwrap_or(Value::NIL);
                let registry = spec
                    .registry
                    .map(|sym| Value::string(resolve_sym(sym)))
                    .unwrap_or(Value::NIL);
                let pattern = Value::cons(family, registry);
                if !all {
                    return Ok(pattern);
                }
                patterns.push(pattern);
            }
        }
    }

    if all {
        Ok(Value::list(patterns))
    } else {
        Ok(Value::NIL)
    }
}

pub(crate) fn new_fontset(
    name: &str,
    fontlist: &Value,
    char_script_table: Option<&Value>,
    charset_script_alist: Option<&Value>,
    font_encoding_alist: Option<&Value>,
) -> Result<String, Flow> {
    let requested_name = normalize_fontset_name(name);
    let canonical_name =
        query_fontset_registry(&requested_name, false).unwrap_or_else(|| requested_name.clone());
    let alias = if canonical_name != requested_name {
        None
    } else {
        Some(
            fontset_registry_alias_from_xlfd(&canonical_name).ok_or_else(|| {
                signal(
                    "error",
                    vec![Value::string("Fontset name must be in XLFD format")],
                )
            })?,
        )
    };

    let mut rules = Vec::new();
    for entry in list_to_vec(fontlist) {
        let parts = list_to_vec(&entry);
        if parts.is_empty() {
            continue;
        }
        let targets = expand_target(&parts[0], char_script_table, charset_script_alist, false)?;
        let mut entries = Vec::new();
        for spec in parts.iter().skip(1) {
            entries.push(parse_font_spec_entry(spec, font_encoding_alist)?);
        }
        for target in targets {
            rules.push((target, entries.clone()));
        }
    }

    let mut slot = registry().write().map_err(|_| {
        signal(
            "error",
            vec![Value::string("Fontset registry lock poisoned")],
        )
    })?;
    let registered = slot.register_fontset(
        fontset_name_lisp_string(&canonical_name),
        alias.as_deref().map(fontset_name_lisp_string),
    );
    slot.replace_rules(&registered, rules);
    Ok(fontset_name_runtime(&registered))
}

pub(crate) fn set_fontset_font(
    fontset: &Value,
    characters: &Value,
    font_spec: &Value,
    add: Option<&Value>,
    char_script_table: Option<&Value>,
    charset_script_alist: Option<&Value>,
    font_encoding_alist: Option<&Value>,
) -> Result<Value, Flow> {
    let fontset_name = resolve_fontset_name_arg(fontset)?;
    let add_mode = FontsetAddMode::from_lisp_value(add);
    let entry = parse_font_spec_entry(font_spec, font_encoding_alist)?;
    let targets = expand_target(characters, char_script_table, charset_script_alist, true)?;

    let mut slot = registry().write().map_err(|_| {
        signal(
            "error",
            vec![Value::string("Fontset registry lock poisoned")],
        )
    })?;
    let canonical = slot.register_fontset(fontset_name_lisp_string(&fontset_name), None);
    for target in targets {
        slot.update_target(&canonical, target, entry.clone(), add_mode);
    }
    Ok(Value::NIL)
}

fn parse_font_spec_entry(
    value: &Value,
    font_encoding_alist: Option<&Value>,
) -> Result<FontSpecEntry, Flow> {
    match value.kind() {
        ValueKind::Nil => Ok(FontSpecEntry::ExplicitNone),
        ValueKind::Cons => {
            let pair_car = value.cons_car();
            let pair_cdr = value.cons_cdr();
            let mut spec = StoredFontSpec {
                family: intern_font_name_value(&pair_car),
                registry: value_text(&pair_cdr)
                    .map(|registry| intern(&registry.to_ascii_lowercase())),
                lang: None,
                weight: None,
                slant: None,
                width: None,
                repertory: None,
            };
            spec.repertory = resolve_font_repertory(&spec, font_encoding_alist).into_stored();
            Ok(FontSpecEntry::Font(spec))
        }
        ValueKind::String => {
            let mut spec = parse_font_name_string(value.as_lisp_string().expect("checked string"));
            spec.repertory = resolve_font_repertory(&spec, font_encoding_alist).into_stored();
            Ok(FontSpecEntry::Font(spec))
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = value.as_vector_data().unwrap().clone();
            let mut spec = parse_font_vector(&items);
            spec.repertory = resolve_font_repertory(&spec, font_encoding_alist).into_stored();
            Ok(FontSpecEntry::Font(spec))
        }
        _ => Err(signal(
            "font",
            vec![Value::string("Invalid font-spec"), *value],
        )),
    }
}

fn parse_font_vector(items: &[Value]) -> StoredFontSpec {
    let family = font_vector_get_flexible(items, "family")
        .and_then(|value| intern_font_name_value(&value))
        .or_else(|| {
            font_vector_get_flexible(items, "name")
                .and_then(|value| value.as_lisp_string().map(parse_font_name_string))
                .and_then(|spec| spec.family)
        });
    let registry = font_vector_get_flexible(items, "registry")
        .and_then(|value| value_text(&value))
        .map(|registry| intern(&registry.to_ascii_lowercase()))
        .or_else(|| {
            font_vector_get_flexible(items, "name")
                .and_then(|value| value.as_lisp_string().map(parse_font_name_string))
                .and_then(|spec| spec.registry)
        });
    let lang = font_vector_get_flexible(items, "lang")
        .and_then(|value| value_text(&value))
        .map(|lang| intern(&lang.to_ascii_lowercase()));
    let weight = font_vector_get_flexible(items, "weight")
        .and_then(|value| value_text(&value))
        .and_then(|weight| FontWeight::from_symbol(&weight));
    let slant = font_vector_get_flexible(items, "slant")
        .and_then(|value| value_text(&value))
        .and_then(|slant| FontSlant::from_symbol(&slant));
    let width = font_vector_get_flexible(items, "width")
        .and_then(|value| value_text(&value))
        .and_then(|width| FontWidth::from_symbol(&width));

    StoredFontSpec {
        family,
        registry,
        lang,
        weight,
        slant,
        width,
        repertory: None,
    }
}

/// Issue #131: intern a font family/name Value faithfully. String values keep
/// their real Emacs bytes (`intern_lisp_string`); symbol values are already
/// interned, so reuse their id. Avoids the PUA-sentinel storage round-trip.
fn intern_font_name_value(value: &Value) -> Option<crate::emacs_core::intern::SymId> {
    match value.kind() {
        ValueKind::String => value
            .as_lisp_string()
            .map(crate::emacs_core::intern::intern_lisp_string),
        ValueKind::Symbol(id) => Some(id),
        _ => None,
    }
}

/// Issue #131: parse an XLFD/font-name string over its real Emacs bytes and
/// intern the family/registry via `intern_lisp_string`, so a raw-unibyte family
/// name keeps its bytes instead of going through the PUA-sentinel storage form.
fn parse_font_name_string(name: &crate::heap_types::LispString) -> StoredFontSpec {
    let multibyte = name.is_multibyte();
    let intern_part = |bytes: &[u8]| -> crate::emacs_core::intern::SymId {
        let ls = if multibyte {
            crate::heap_types::LispString::from_emacs_bytes(bytes.to_vec())
        } else {
            crate::heap_types::LispString::from_unibyte(bytes.to_vec())
        };
        crate::emacs_core::intern::intern_lisp_string(&ls)
    };

    let trimmed = name.as_bytes().trim_ascii();
    if trimmed.first() == Some(&b'-') {
        let parts: Vec<&[u8]> = trimmed.split(|&b| b == b'-').collect();
        if parts.len() >= 15 {
            let family = parts
                .get(2)
                .copied()
                .filter(|value| !value.is_empty() && *value != b"*");
            let registry = if parts.len() >= 3 {
                let mut registry = parts[parts.len() - 2].to_vec();
                registry.push(b'-');
                registry.extend_from_slice(parts[parts.len() - 1]);
                if registry.contains(&b'*') {
                    None
                } else {
                    registry.make_ascii_lowercase();
                    Some(registry)
                }
            } else {
                None
            };
            return StoredFontSpec {
                family: family.map(intern_part),
                registry: registry.map(|registry| intern_part(&registry)),
                lang: None,
                weight: None,
                slant: None,
                width: None,
                repertory: None,
            };
        }
    }

    StoredFontSpec {
        family: (!trimmed.is_empty()).then(|| intern_part(trimmed)),
        registry: None,
        lang: None,
        weight: None,
        slant: None,
        width: None,
        repertory: None,
    }
}

/// Whether a GNU font definition admits every character or only a declared
/// repertory.  This is deliberately not an `Option<FontRepertory>` while
/// parsing: `None` from `find_font_encoding` means the restrictive ASCII
/// default, whereas an encoding entry whose repertory is nil means no
/// restriction.  Collapsing those states caused family-only font specs to
/// capture every private-use icon.
#[derive(Clone, Debug, PartialEq, Eq)]
enum FontRepertoryConstraint {
    Restricted(FontRepertory),
    Unrestricted,
}

impl FontRepertoryConstraint {
    fn into_stored(self) -> Option<FontRepertory> {
        match self {
            Self::Restricted(repertory) => Some(repertory),
            Self::Unrestricted => None,
        }
    }
}

fn resolve_font_repertory(
    spec: &StoredFontSpec,
    font_encoding_alist: Option<&Value>,
) -> FontRepertoryConstraint {
    let symbol_utf8 = |symbol: Option<SymId>| {
        symbol
            .map(resolve_sym_lisp_string)
            .map(|name| name.as_utf8_str())
            .unwrap_or(Some(""))
    };
    let (Some(family), Some(registry)) = (symbol_utf8(spec.family), symbol_utf8(spec.registry))
    else {
        return default_ascii_repertory();
    };
    let font_name = format!("{family}-{registry}");

    font_encoding_alist
        .and_then(|alist| lookup_font_encoding(alist, &font_name))
        // GNU font.c:find_font_encoding returns nil when no valid pattern
        // matches; Fset_fontset_font then uses Qascii for both encoding and
        // repertory.
        .unwrap_or_else(default_ascii_repertory)
}

fn default_ascii_repertory() -> FontRepertoryConstraint {
    FontRepertoryConstraint::Restricted(FontRepertory::Charset(intern("ascii")))
}

fn lookup_font_encoding(
    font_encoding_alist: &Value,
    font_name: &str,
) -> Option<FontRepertoryConstraint> {
    for entry in list_to_vec(font_encoding_alist) {
        if !entry.is_cons() {
            continue;
        };
        let pair_car = entry.cons_car();
        let pair_cdr = entry.cons_cdr();
        let Some(pattern) = pair_car.as_lisp_string() else {
            continue;
        };
        if crate::emacs_core::regex::predicate_match_ignore_case(pattern, font_name)
            .unwrap_or(false)
            && let Some(repertory) = font_encoding_repertory(&pair_cdr)
        {
            return Some(repertory);
        }
    }
    None
}

fn font_encoding_repertory(value: &Value) -> Option<FontRepertoryConstraint> {
    match value.kind() {
        ValueKind::Symbol(id) => {
            let name = resolve_sym(id);
            charset_exists(name).then_some(FontRepertoryConstraint::Restricted(
                FontRepertory::Charset(id),
            ))
        }
        ValueKind::Cons => {
            let pair_car = value.cons_car();
            let pair_cdr = value.cons_cdr();
            let encoding = pair_car.as_symbol_id()?;
            if !charset_exists(resolve_sym(encoding)) {
                return None;
            }
            if pair_cdr.is_nil() {
                Some(FontRepertoryConstraint::Unrestricted)
            } else {
                font_repertory_value(&pair_cdr).map(FontRepertoryConstraint::Restricted)
            }
        }
        _ => None,
    }
}

fn font_repertory_value(value: &Value) -> Option<FontRepertory> {
    match value.kind() {
        ValueKind::Symbol(id) => {
            charset_exists(resolve_sym(id)).then_some(FontRepertory::Charset(id))
        }
        ValueKind::Veclike(VecLikeType::Vector) if is_char_table(value) => {
            let mut ranges = Vec::new();
            for_each_non_nil_char_table_run(value, |key, _| {
                if let Some((from, to)) = value_to_range(&key) {
                    ranges.push((from, to));
                }
            });
            Some(FontRepertory::CharTableRanges(ranges))
        }
        _ => None,
    }
}

fn expand_target(
    target: &Value,
    char_script_table: Option<&Value>,
    charset_script_alist: Option<&Value>,
    enforce_ascii_rules: bool,
) -> Result<Vec<FontsetTarget>, Flow> {
    match target.kind() {
        ValueKind::Nil => Ok(vec![FontsetTarget::Fallback]),
        ValueKind::Fixnum(ch) => {
            let code = ch as u32;
            if enforce_ascii_rules && code < 0x80 {
                return Err(signal(
                    "error",
                    vec![Value::string("Can't set a font for partial ASCII range")],
                ));
            }
            Ok(vec![FontsetTarget::Range(code, code)])
        }
        ValueKind::Cons => {
            let pair_car = target.cons_car();
            let pair_cdr = target.cons_cdr();
            let from = expect_target_char(&pair_car)?;
            let to = expect_target_char(&pair_cdr)?;
            if from > to {
                return Ok(vec![FontsetTarget::Range(to, from)]);
            }
            if enforce_ascii_rules && from < 0x80 && !(from == 0 && to >= 0x7F) {
                return Err(signal(
                    "error",
                    vec![Value::string("Can't set a font for partial ASCII range")],
                ));
            }
            Ok(vec![FontsetTarget::Range(from, to)])
        }
        ValueKind::Symbol(id) => {
            let symbol_name = resolve_sym(id).to_string();
            let targets = expand_script_symbol(&symbol_name, char_script_table)
                .or_else(|| {
                    charset_target_ranges(resolve_sym(id)).map(|ranges| {
                        ranges
                            .into_iter()
                            .map(|(from, to)| FontsetTarget::Range(from, to))
                            .collect()
                    })
                })
                .or_else(|| {
                    charset_script_alist
                        .and_then(|alist| lookup_charset_script(alist, &symbol_name))
                        .and_then(|script| expand_script_symbol(&script, char_script_table))
                })
                .unwrap_or_default();
            if targets.is_empty() {
                return Err(signal(
                    "error",
                    vec![Value::string(format!(
                        "Invalid script or charset name: {symbol_name}"
                    ))],
                ));
            }
            Ok(targets)
        }
        _ => Err(signal(
            "error",
            vec![Value::string(
                "Invalid second argument for setting a font in a fontset",
            )],
        )),
    }
}

fn expand_script_symbol(
    name: &str,
    char_script_table: Option<&Value>,
) -> Option<Vec<FontsetTarget>> {
    let table = char_script_table?;
    let target = Value::symbol(name);
    let mut ranges = Vec::new();
    for_each_non_nil_char_table_run(table, |key, value| {
        if value != target {
            return;
        }
        if let Some((from, to)) = value_to_range(&key) {
            ranges.push(FontsetTarget::Range(from, to));
        }
    });
    (!ranges.is_empty()).then_some(ranges)
}

fn lookup_charset_script(alist: &Value, charset_name: &str) -> Option<String> {
    let target = Value::symbol(charset_name);
    let mut cursor = *alist;
    loop {
        match cursor.kind() {
            ValueKind::Nil => return None,
            ValueKind::Cons => {
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                if pair_car.is_cons() {
                    let entry_car = pair_car.cons_car();
                    let entry_cdr = pair_car.cons_cdr();
                    if entry_car == target {
                        return value_text(&entry_cdr);
                    }
                }
                cursor = pair_cdr;
            }
            _ => return None,
        }
    }
}

fn value_to_range(value: &Value) -> Option<(u32, u32)> {
    match value.kind() {
        ValueKind::Fixnum(ch) => Some((ch as u32, ch as u32)),
        ValueKind::Cons => {
            let pair_car = value.cons_car();
            let pair_cdr = value.cons_cdr();
            let from = expect_target_char(&pair_car).ok()?;
            let to = expect_target_char(&pair_cdr).ok()?;
            Some((from.min(to), from.max(to)))
        }
        _ => None,
    }
}

fn expect_target_char(value: &Value) -> Result<u32, Flow> {
    match value.kind() {
        ValueKind::Fixnum(ch) => Ok(ch as u32),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), *value],
        )),
    }
}

fn list_to_vec(value: &Value) -> Vec<Value> {
    let mut cursor = *value;
    let mut items = Vec::new();
    loop {
        match cursor.kind() {
            ValueKind::Nil => return items,
            ValueKind::Cons => {
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                items.push(pair_car);
                cursor = pair_cdr;
            }
            _ => {
                items.push(cursor);
                return items;
            }
        }
    }
}

fn value_text(value: &Value) -> Option<String> {
    match value.kind() {
        ValueKind::String => fontset_string_text(value),
        ValueKind::Symbol(id) => Some(resolve_sym(id).to_string()),
        _ => None,
    }
}

fn font_vector_get_flexible(items: &[Value], prop: &str) -> Option<Value> {
    let prop_norm = prop.trim_start_matches(':');
    let mut index = 1usize;
    while index + 1 < items.len() {
        let key_norm = match items[index].kind() {
            ValueKind::Symbol(id) => resolve_sym(id).trim_start_matches(':'),
            _ => {
                index += 2;
                continue;
            }
        };
        if key_norm == prop_norm {
            return Some(items[index + 1]);
        }
        index += 2;
    }
    None
}

/// The Neomacs counterpart of GNU's `syms_of_fontset` (`src/fontset.c:2155`).
///
/// GNU calls it from every window-system branch of `main` -- X, NS, Haiku, w32,
/// pgtk and Android (`src/emacs.c:2377`, `2403`, `2426`, `2435`, `2447`,
/// `2453`) -- so a fontset variable exists in every GUI-capable build, and
/// `lisp/cus-start.el:942-944` probes exactly that with "any function from
/// fontset.c will do", `(fboundp 'new-fontset)`.
pub fn register_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
    // fontset.c:2237, `Vvertical_centering_font_regexp = Qnil'.
    //
    // The regexp both editors report is NOT this initializer: it comes from
    // `lisp/international/fontset.el:1266', which `loadup.el' preloads in a
    // window-system build.  What the C declaration supplies is the
    // `declared_special' bit, which no `setq' in Lisp can add -- fontset.el's
    // `(defvar vertical-centering-font-regexp)' on line 1259 is the valueless
    // form, which marks the variable special only within that file.  So a
    // Neomacs that seeded the value and skipped the declaration matched GNU on
    // `symbol-value' and still bound it lexically under `let'.
    obarray.define_special_variable("vertical-centering-font-regexp", Value::NIL);
    // fontset.c:2225 DEFVAR_LISP, `Valternate_fontname_alist = Qnil'.  The one
    // name of `syms_of_fontset''s eight this port did not declare (entry 173).
    obarray.define_special_variable("alternate-fontname-alist", Value::NIL);
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
