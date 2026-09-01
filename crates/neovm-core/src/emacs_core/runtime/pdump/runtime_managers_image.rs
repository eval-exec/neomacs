//! Fixed-layout pdump section for residual runtime managers.
//!
//! GNU pdumper preserves global runtime structures by tracing roots,
//! remembered data, and direct dump records.  This section removes the final
//! file-pdump dependency on `DumpContextState` bincode by spelling Neomacs'
//! remaining non-heap runtime managers as explicit count-prefixed records.

use bytemuck::{Pod, Zeroable};

use super::DumpError;
use super::object_value_codec::{Cursor, write_bool, write_u8, write_u32, write_u64, write_value};
use super::types::{
    DumpAbbrev, DumpAbbrevManager, DumpAbbrevTable, DumpBookmark, DumpBookmarkManager,
    DumpContextState, DumpCustomManager, DumpFontLockDefaults, DumpFontLockKeyword,
    DumpFontRepertory, DumpFontSlant, DumpFontSpecEntry, DumpFontWidth, DumpFontsetData,
    DumpFontsetRangeEntry, DumpFontsetRegistry, DumpInteractiveRegistry, DumpInteractiveSpec,
    DumpKmacroManager, DumpLispString, DumpMajorMode, DumpMinorMode, DumpModeCustomGroup,
    DumpModeCustomType, DumpModeCustomVariable, DumpModeRegistry, DumpRectangleState,
    DumpRegisterContent, DumpRegisterManager, DumpStoredFontSpec, DumpSymId, DumpValue,
    DumpVariableWatcherList,
};

const RUNTIME_MANAGERS_MAGIC: [u8; 16] = *b"NEORUNTIME\0\0\0\0\0\0";
const RUNTIME_MANAGERS_FORMAT_VERSION: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct RuntimeManagersHeader {
    magic: [u8; 16],
    version: u32,
    header_size: u32,
    payload_offset: u64,
    payload_len: u64,
}

const HEADER_SIZE: usize = std::mem::size_of::<RuntimeManagersHeader>();

#[derive(Debug, Clone)]
pub(crate) struct RuntimeManagersState {
    pub custom: DumpCustomManager,
    pub modes: DumpModeRegistry,
    pub fontset_registry: DumpFontsetRegistry,
    pub abbrevs: DumpAbbrevManager,
    pub interactive: DumpInteractiveRegistry,
    pub rectangle: DumpRectangleState,
    pub kmacro: DumpKmacroManager,
    pub registers: DumpRegisterManager,
    pub bookmarks: DumpBookmarkManager,
    pub watchers: DumpVariableWatcherList,
}

impl RuntimeManagersState {
    pub(crate) fn from_context_state(state: &DumpContextState) -> Self {
        Self {
            custom: state.custom.clone(),
            modes: state.modes.clone(),
            fontset_registry: state.fontset_registry.clone(),
            abbrevs: state.abbrevs.clone(),
            interactive: state.interactive.clone(),
            rectangle: state.rectangle.clone(),
            kmacro: state.kmacro.clone(),
            registers: state.registers.clone(),
            bookmarks: state.bookmarks.clone(),
            watchers: state.watchers.clone(),
        }
    }

    pub(crate) fn install_into(self, state: &mut DumpContextState) {
        state.custom = self.custom;
        state.modes = self.modes;
        state.fontset_registry = self.fontset_registry;
        state.abbrevs = self.abbrevs;
        state.interactive = self.interactive;
        state.rectangle = self.rectangle;
        state.kmacro = self.kmacro;
        state.registers = self.registers;
        state.bookmarks = self.bookmarks;
        state.watchers = self.watchers;
    }
}

pub(crate) fn runtime_managers_section_bytes(
    managers: &RuntimeManagersState,
) -> Result<Vec<u8>, DumpError> {
    let mut bytes = vec![0; HEADER_SIZE];

    write_custom_manager(&mut bytes, &managers.custom)?;
    write_mode_registry(&mut bytes, &managers.modes)?;
    write_fontset_registry(&mut bytes, &managers.fontset_registry)?;
    write_abbrev_manager(&mut bytes, &managers.abbrevs)?;
    write_interactive_registry(&mut bytes, &managers.interactive)?;
    write_rectangle_state(&mut bytes, &managers.rectangle)?;
    write_kmacro_manager(&mut bytes, &managers.kmacro)?;
    write_register_manager(&mut bytes, &managers.registers)?;
    write_bookmark_manager(&mut bytes, &managers.bookmarks)?;
    write_variable_watchers(&mut bytes, &managers.watchers)?;

    let payload_len = bytes.len() - HEADER_SIZE;
    let header = RuntimeManagersHeader {
        magic: RUNTIME_MANAGERS_MAGIC,
        version: RUNTIME_MANAGERS_FORMAT_VERSION,
        header_size: HEADER_SIZE as u32,
        payload_offset: HEADER_SIZE as u64,
        payload_len: payload_len as u64,
    };
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    Ok(bytes)
}

pub(crate) fn load_runtime_managers_section(
    section: &[u8],
) -> Result<RuntimeManagersState, DumpError> {
    let header = read_header(section)?;
    let payload_offset = usize::try_from(header.payload_offset).map_err(|_| {
        DumpError::ImageFormatError("runtime-manager payload offset overflows usize".into())
    })?;
    let payload_len = usize::try_from(header.payload_len).map_err(|_| {
        DumpError::ImageFormatError("runtime-manager payload length overflows usize".into())
    })?;
    let end = payload_offset.checked_add(payload_len).ok_or_else(|| {
        DumpError::ImageFormatError("runtime-manager payload range overflows".into())
    })?;
    if payload_offset < HEADER_SIZE || end > section.len() {
        return Err(DumpError::ImageFormatError(
            "runtime-manager payload range is outside section".into(),
        ));
    }

    let mut cursor = Cursor::new(&section[payload_offset..end]);
    let managers = RuntimeManagersState {
        custom: read_custom_manager(&mut cursor)?,
        modes: read_mode_registry(&mut cursor)?,
        fontset_registry: read_fontset_registry(&mut cursor)?,
        abbrevs: read_abbrev_manager(&mut cursor)?,
        interactive: read_interactive_registry(&mut cursor)?,
        rectangle: read_rectangle_state(&mut cursor)?,
        kmacro: read_kmacro_manager(&mut cursor)?,
        registers: read_register_manager(&mut cursor)?,
        bookmarks: read_bookmark_manager(&mut cursor)?,
        watchers: read_variable_watchers(&mut cursor)?,
    };
    if !cursor.is_empty() {
        return Err(DumpError::ImageFormatError(format!(
            "runtime-manager section has {} trailing payload bytes",
            cursor.remaining()
        )));
    }
    Ok(managers)
}

fn read_header(section: &[u8]) -> Result<RuntimeManagersHeader, DumpError> {
    if section.len() < HEADER_SIZE {
        return Err(DumpError::ImageFormatError(format!(
            "runtime-manager section shorter than header: {} < {HEADER_SIZE}",
            section.len()
        )));
    }
    let header = *bytemuck::from_bytes::<RuntimeManagersHeader>(&section[..HEADER_SIZE]);
    if header.magic != RUNTIME_MANAGERS_MAGIC {
        return Err(DumpError::ImageFormatError(
            "runtime-manager section has bad magic".into(),
        ));
    }
    if header.version != RUNTIME_MANAGERS_FORMAT_VERSION {
        return Err(DumpError::UnsupportedVersion(header.version));
    }
    if header.header_size != HEADER_SIZE as u32 {
        return Err(DumpError::ImageFormatError(format!(
            "runtime-manager header size {} does not match runtime header size {HEADER_SIZE}",
            header.header_size
        )));
    }
    Ok(header)
}

fn write_custom_manager(out: &mut Vec<u8>, manager: &DumpCustomManager) -> Result<(), DumpError> {
    write_sym_vec(out, &manager.auto_buffer_local_syms)?;
    write_string_vec(out, &manager.auto_buffer_local)?;
    Ok(())
}

fn read_custom_manager(cursor: &mut Cursor<'_>) -> Result<DumpCustomManager, DumpError> {
    Ok(DumpCustomManager {
        auto_buffer_local_syms: read_sym_vec(cursor)?,
        auto_buffer_local: read_string_vec(cursor)?,
    })
}

fn write_mode_registry(out: &mut Vec<u8>, registry: &DumpModeRegistry) -> Result<(), DumpError> {
    write_vec(
        out,
        &registry.major_modes,
        "major-mode count",
        |out, (sym, mode)| {
            write_sym(out, *sym);
            write_major_mode(out, mode)
        },
    )?;
    write_vec(
        out,
        &registry.minor_modes,
        "minor-mode count",
        |out, (sym, mode)| {
            write_sym(out, *sym);
            write_minor_mode(out, mode)
        },
    )?;
    write_vec(
        out,
        &registry.buffer_major_modes,
        "buffer-major-mode count",
        |out, (id, value)| {
            write_u64(out, *id);
            write_value(out, value)
        },
    )?;
    write_vec(
        out,
        &registry.buffer_minor_modes,
        "buffer-minor-mode count",
        |out, (id, values)| {
            write_u64(out, *id);
            write_values(out, values)
        },
    )?;
    write_values(out, &registry.global_minor_modes)?;
    write_lisp_string_value_pairs(out, &registry.auto_mode_alist_lisp)?;
    write_string_value_pairs(out, &registry.auto_mode_alist)?;
    write_vec(
        out,
        &registry.custom_variables,
        "custom-variable count",
        |out, (sym, variable)| {
            write_sym(out, *sym);
            write_mode_custom_variable(out, variable)
        },
    )?;
    write_vec(
        out,
        &registry.custom_groups,
        "custom-group count",
        |out, (sym, group)| {
            write_sym(out, *sym);
            write_mode_custom_group(out, group)
        },
    )?;
    write_value(out, &registry.fundamental_mode)?;
    Ok(())
}

fn read_mode_registry(cursor: &mut Cursor<'_>) -> Result<DumpModeRegistry, DumpError> {
    Ok(DumpModeRegistry {
        major_modes: read_vec(cursor, "major-mode count", |cursor| {
            Ok((read_sym(cursor)?, read_major_mode(cursor)?))
        })?,
        minor_modes: read_vec(cursor, "minor-mode count", |cursor| {
            Ok((read_sym(cursor)?, read_minor_mode(cursor)?))
        })?,
        buffer_major_modes: read_vec(cursor, "buffer-major-mode count", |cursor| {
            Ok((
                cursor.read_u64("buffer-major-mode buffer id")?,
                cursor.read_value()?,
            ))
        })?,
        buffer_minor_modes: read_vec(cursor, "buffer-minor-mode count", |cursor| {
            Ok((
                cursor.read_u64("buffer-minor-mode buffer id")?,
                read_values(cursor)?,
            ))
        })?,
        global_minor_modes: read_values(cursor)?,
        auto_mode_alist_lisp: read_lisp_string_value_pairs(cursor)?,
        auto_mode_alist: read_string_value_pairs(cursor)?,
        custom_variables: read_vec(cursor, "custom-variable count", |cursor| {
            Ok((read_sym(cursor)?, read_mode_custom_variable(cursor)?))
        })?,
        custom_groups: read_vec(cursor, "custom-group count", |cursor| {
            Ok((read_sym(cursor)?, read_mode_custom_group(cursor)?))
        })?,
        fundamental_mode: cursor.read_value()?,
    })
}

fn write_major_mode(out: &mut Vec<u8>, mode: &DumpMajorMode) -> Result<(), DumpError> {
    write_lisp_string(out, &mode.pretty_name)?;
    write_opt_value(out, mode.parent.as_ref())?;
    write_value(out, &mode.mode_hook)?;
    write_opt_value(out, mode.keymap_name.as_ref())?;
    write_opt_value(out, mode.syntax_table_name.as_ref())?;
    write_opt_value(out, mode.abbrev_table_name.as_ref())?;
    write_opt_font_lock_defaults(out, mode.font_lock.as_ref())?;
    write_opt_value(out, mode.body.as_ref())?;
    Ok(())
}

fn read_major_mode(cursor: &mut Cursor<'_>) -> Result<DumpMajorMode, DumpError> {
    Ok(DumpMajorMode {
        pretty_name: read_lisp_string(cursor)?,
        parent: read_opt_value(cursor)?,
        mode_hook: cursor.read_value()?,
        keymap_name: read_opt_value(cursor)?,
        syntax_table_name: read_opt_value(cursor)?,
        abbrev_table_name: read_opt_value(cursor)?,
        font_lock: read_opt_font_lock_defaults(cursor)?,
        body: read_opt_value(cursor)?,
    })
}

fn write_minor_mode(out: &mut Vec<u8>, mode: &DumpMinorMode) -> Result<(), DumpError> {
    write_opt_lisp_string(out, mode.lighter.as_ref())?;
    write_opt_value(out, mode.keymap_name.as_ref())?;
    write_bool(out, mode.global);
    write_opt_value(out, mode.body.as_ref())?;
    Ok(())
}

fn read_minor_mode(cursor: &mut Cursor<'_>) -> Result<DumpMinorMode, DumpError> {
    Ok(DumpMinorMode {
        lighter: read_opt_lisp_string(cursor)?,
        keymap_name: read_opt_value(cursor)?,
        global: cursor.read_bool("minor-mode global flag")?,
        body: read_opt_value(cursor)?,
    })
}

fn write_opt_font_lock_defaults(
    out: &mut Vec<u8>,
    defaults: Option<&DumpFontLockDefaults>,
) -> Result<(), DumpError> {
    match defaults {
        Some(defaults) => {
            write_bool(out, true);
            write_font_lock_defaults(out, defaults)?;
        }
        None => write_bool(out, false),
    }
    Ok(())
}

fn read_opt_font_lock_defaults(
    cursor: &mut Cursor<'_>,
) -> Result<Option<DumpFontLockDefaults>, DumpError> {
    if cursor.read_bool("font-lock defaults present")? {
        Ok(Some(read_font_lock_defaults(cursor)?))
    } else {
        Ok(None)
    }
}

fn write_font_lock_defaults(
    out: &mut Vec<u8>,
    defaults: &DumpFontLockDefaults,
) -> Result<(), DumpError> {
    write_vec(
        out,
        &defaults.keywords,
        "font-lock keyword count",
        write_font_lock_keyword,
    )?;
    write_bool(out, defaults.case_fold);
    write_opt_lisp_string(out, defaults.syntax_table_lisp.as_ref())?;
    write_opt_string(out, defaults.syntax_table.as_deref())?;
    Ok(())
}

fn read_font_lock_defaults(cursor: &mut Cursor<'_>) -> Result<DumpFontLockDefaults, DumpError> {
    Ok(DumpFontLockDefaults {
        keywords: read_vec(cursor, "font-lock keyword count", read_font_lock_keyword)?,
        case_fold: cursor.read_bool("font-lock case-fold flag")?,
        syntax_table_lisp: read_opt_lisp_string(cursor)?,
        syntax_table: read_opt_string(cursor)?,
    })
}

fn write_font_lock_keyword(
    out: &mut Vec<u8>,
    keyword: &DumpFontLockKeyword,
) -> Result<(), DumpError> {
    write_opt_lisp_string(out, keyword.pattern_lisp.as_ref())?;
    write_opt_string(out, keyword.pattern.as_deref())?;
    write_opt_sym(out, keyword.face_sym);
    write_opt_string(out, keyword.face.as_deref())?;
    write_usize(out, keyword.group)?;
    write_bool(out, keyword.override_);
    write_bool(out, keyword.laxmatch);
    Ok(())
}

fn read_font_lock_keyword(cursor: &mut Cursor<'_>) -> Result<DumpFontLockKeyword, DumpError> {
    Ok(DumpFontLockKeyword {
        pattern_lisp: read_opt_lisp_string(cursor)?,
        pattern: read_opt_string(cursor)?,
        face_sym: read_opt_sym(cursor)?,
        face: read_opt_string(cursor)?,
        group: cursor.read_usize("font-lock group")?,
        override_: cursor.read_bool("font-lock override flag")?,
        laxmatch: cursor.read_bool("font-lock laxmatch flag")?,
    })
}

fn write_mode_custom_variable(
    out: &mut Vec<u8>,
    variable: &DumpModeCustomVariable,
) -> Result<(), DumpError> {
    write_value(out, &variable.default_value)?;
    write_opt_lisp_string(out, variable.doc.as_ref())?;
    write_mode_custom_type(out, &variable.custom_type)?;
    write_opt_value(out, variable.group.as_ref())?;
    write_opt_value(out, variable.set_function.as_ref())?;
    write_opt_value(out, variable.get_function.as_ref())?;
    write_opt_lisp_string(out, variable.tag.as_ref())?;
    Ok(())
}

fn read_mode_custom_variable(cursor: &mut Cursor<'_>) -> Result<DumpModeCustomVariable, DumpError> {
    Ok(DumpModeCustomVariable {
        default_value: cursor.read_value()?,
        doc: read_opt_lisp_string(cursor)?,
        custom_type: read_mode_custom_type(cursor)?,
        group: read_opt_value(cursor)?,
        set_function: read_opt_value(cursor)?,
        get_function: read_opt_value(cursor)?,
        tag: read_opt_lisp_string(cursor)?,
    })
}

fn write_mode_custom_type(
    out: &mut Vec<u8>,
    custom_type: &DumpModeCustomType,
) -> Result<(), DumpError> {
    match custom_type {
        DumpModeCustomType::Boolean => write_u8(out, 0),
        DumpModeCustomType::Integer => write_u8(out, 1),
        DumpModeCustomType::Float => write_u8(out, 2),
        DumpModeCustomType::String => write_u8(out, 3),
        DumpModeCustomType::Symbol => write_u8(out, 4),
        DumpModeCustomType::Sexp => write_u8(out, 5),
        DumpModeCustomType::Choice(choices) => {
            write_u8(out, 6);
            write_vec(
                out,
                choices,
                "custom choice count",
                |out, (label, value)| {
                    write_string(out, label)?;
                    write_value(out, value)
                },
            )?;
        }
        DumpModeCustomType::List(element) => {
            write_u8(out, 7);
            write_mode_custom_type(out, element)?;
        }
        DumpModeCustomType::Alist(key, value) => {
            write_u8(out, 8);
            write_mode_custom_type(out, key)?;
            write_mode_custom_type(out, value)?;
        }
        DumpModeCustomType::Plist(key, value) => {
            write_u8(out, 9);
            write_mode_custom_type(out, key)?;
            write_mode_custom_type(out, value)?;
        }
        DumpModeCustomType::Color => write_u8(out, 10),
        DumpModeCustomType::Face => write_u8(out, 11),
        DumpModeCustomType::File => write_u8(out, 12),
        DumpModeCustomType::Directory => write_u8(out, 13),
        DumpModeCustomType::Function => write_u8(out, 14),
        DumpModeCustomType::Variable => write_u8(out, 15),
        DumpModeCustomType::Hook => write_u8(out, 16),
        DumpModeCustomType::Coding => write_u8(out, 17),
    }
    Ok(())
}

fn read_mode_custom_type(cursor: &mut Cursor<'_>) -> Result<DumpModeCustomType, DumpError> {
    Ok(match cursor.read_u8("custom type tag")? {
        0 => DumpModeCustomType::Boolean,
        1 => DumpModeCustomType::Integer,
        2 => DumpModeCustomType::Float,
        3 => DumpModeCustomType::String,
        4 => DumpModeCustomType::Symbol,
        5 => DumpModeCustomType::Sexp,
        6 => DumpModeCustomType::Choice(read_vec(cursor, "custom choice count", |cursor| {
            Ok((cursor.read_string()?, cursor.read_value()?))
        })?),
        7 => DumpModeCustomType::List(Box::new(read_mode_custom_type(cursor)?)),
        8 => DumpModeCustomType::Alist(
            Box::new(read_mode_custom_type(cursor)?),
            Box::new(read_mode_custom_type(cursor)?),
        ),
        9 => DumpModeCustomType::Plist(
            Box::new(read_mode_custom_type(cursor)?),
            Box::new(read_mode_custom_type(cursor)?),
        ),
        10 => DumpModeCustomType::Color,
        11 => DumpModeCustomType::Face,
        12 => DumpModeCustomType::File,
        13 => DumpModeCustomType::Directory,
        14 => DumpModeCustomType::Function,
        15 => DumpModeCustomType::Variable,
        16 => DumpModeCustomType::Hook,
        17 => DumpModeCustomType::Coding,
        other => {
            return Err(DumpError::ImageFormatError(format!(
                "unknown custom type tag {other}"
            )));
        }
    })
}

fn write_mode_custom_group(
    out: &mut Vec<u8>,
    group: &DumpModeCustomGroup,
) -> Result<(), DumpError> {
    write_opt_lisp_string(out, group.doc.as_ref())?;
    write_opt_value(out, group.parent.as_ref())?;
    write_values(out, &group.members)?;
    Ok(())
}

fn read_mode_custom_group(cursor: &mut Cursor<'_>) -> Result<DumpModeCustomGroup, DumpError> {
    Ok(DumpModeCustomGroup {
        doc: read_opt_lisp_string(cursor)?,
        parent: read_opt_value(cursor)?,
        members: read_values(cursor)?,
    })
}

fn write_fontset_registry(
    out: &mut Vec<u8>,
    registry: &DumpFontsetRegistry,
) -> Result<(), DumpError> {
    write_lisp_string_vec(out, &registry.ordered_names_lisp)?;
    write_vec(
        out,
        &registry.alias_to_name_lisp,
        "fontset Lisp alias count",
        |out, (alias, name)| {
            write_lisp_string(out, alias)?;
            write_lisp_string(out, name)
        },
    )?;
    write_vec(
        out,
        &registry.fontsets_lisp,
        "fontset Lisp table count",
        |out, (name, data)| {
            write_lisp_string(out, name)?;
            write_fontset_data(out, data)
        },
    )?;
    write_string_vec(out, &registry.ordered_names)?;
    write_vec(
        out,
        &registry.alias_to_name,
        "fontset string alias count",
        |out, (alias, name)| {
            write_string(out, alias)?;
            write_string(out, name)
        },
    )?;
    write_vec(
        out,
        &registry.fontsets,
        "fontset string table count",
        |out, (name, data)| {
            write_string(out, name)?;
            write_fontset_data(out, data)
        },
    )?;
    write_u64(out, registry.generation);
    Ok(())
}

fn read_fontset_registry(cursor: &mut Cursor<'_>) -> Result<DumpFontsetRegistry, DumpError> {
    Ok(DumpFontsetRegistry {
        ordered_names_lisp: read_lisp_string_vec(cursor)?,
        alias_to_name_lisp: read_vec(cursor, "fontset Lisp alias count", |cursor| {
            Ok((read_lisp_string(cursor)?, read_lisp_string(cursor)?))
        })?,
        fontsets_lisp: read_vec(cursor, "fontset Lisp table count", |cursor| {
            Ok((read_lisp_string(cursor)?, read_fontset_data(cursor)?))
        })?,
        ordered_names: read_string_vec(cursor)?,
        alias_to_name: read_vec(cursor, "fontset string alias count", |cursor| {
            Ok((cursor.read_string()?, cursor.read_string()?))
        })?,
        fontsets: read_vec(cursor, "fontset string table count", |cursor| {
            Ok((cursor.read_string()?, read_fontset_data(cursor)?))
        })?,
        generation: cursor.read_u64("fontset generation")?,
    })
}

fn write_fontset_data(out: &mut Vec<u8>, data: &DumpFontsetData) -> Result<(), DumpError> {
    write_vec(out, &data.ranges, "fontset range count", |out, range| {
        write_fontset_range(out, range)
    })?;
    match &data.fallback {
        Some(entries) => {
            write_bool(out, true);
            write_font_spec_entries(out, entries)?;
        }
        None => write_bool(out, false),
    }
    Ok(())
}

fn read_fontset_data(cursor: &mut Cursor<'_>) -> Result<DumpFontsetData, DumpError> {
    Ok(DumpFontsetData {
        ranges: read_vec(cursor, "fontset range count", read_fontset_range)?,
        fallback: if cursor.read_bool("fontset fallback present")? {
            Some(read_font_spec_entries(cursor)?)
        } else {
            None
        },
    })
}

fn write_fontset_range(out: &mut Vec<u8>, range: &DumpFontsetRangeEntry) -> Result<(), DumpError> {
    write_u32(out, range.from);
    write_u32(out, range.to);
    write_font_spec_entries(out, &range.entries)?;
    Ok(())
}

fn read_fontset_range(cursor: &mut Cursor<'_>) -> Result<DumpFontsetRangeEntry, DumpError> {
    Ok(DumpFontsetRangeEntry {
        from: cursor.read_u32("fontset range start")?,
        to: cursor.read_u32("fontset range end")?,
        entries: read_font_spec_entries(cursor)?,
    })
}

fn write_font_spec_entries(
    out: &mut Vec<u8>,
    entries: &[DumpFontSpecEntry],
) -> Result<(), DumpError> {
    write_vec(out, entries, "font spec entry count", |out, entry| {
        match entry {
            DumpFontSpecEntry::Font(spec) => {
                write_u8(out, 0);
                write_stored_font_spec(out, spec)?;
            }
            DumpFontSpecEntry::ExplicitNone => write_u8(out, 1),
        }
        Ok(())
    })
}

fn read_font_spec_entries(cursor: &mut Cursor<'_>) -> Result<Vec<DumpFontSpecEntry>, DumpError> {
    read_vec(cursor, "font spec entry count", |cursor| {
        Ok(match cursor.read_u8("font spec entry tag")? {
            0 => DumpFontSpecEntry::Font(read_stored_font_spec(cursor)?),
            1 => DumpFontSpecEntry::ExplicitNone,
            other => {
                return Err(DumpError::ImageFormatError(format!(
                    "unknown font spec entry tag {other}"
                )));
            }
        })
    })
}

fn write_stored_font_spec(out: &mut Vec<u8>, spec: &DumpStoredFontSpec) -> Result<(), DumpError> {
    write_opt_sym(out, spec.family_sym);
    write_opt_string(out, spec.family.as_deref())?;
    write_opt_sym(out, spec.registry_sym);
    write_opt_string(out, spec.registry.as_deref())?;
    write_opt_sym(out, spec.lang_sym);
    write_opt_string(out, spec.lang.as_deref())?;
    write_opt_u16(out, spec.weight);
    write_opt_font_slant(out, spec.slant);
    write_opt_font_width(out, spec.width);
    write_opt_font_repertory(out, spec.repertory.as_ref())?;
    Ok(())
}

fn read_stored_font_spec(cursor: &mut Cursor<'_>) -> Result<DumpStoredFontSpec, DumpError> {
    Ok(DumpStoredFontSpec {
        family_sym: read_opt_sym(cursor)?,
        family: read_opt_string(cursor)?,
        registry_sym: read_opt_sym(cursor)?,
        registry: read_opt_string(cursor)?,
        lang_sym: read_opt_sym(cursor)?,
        lang: read_opt_string(cursor)?,
        weight: read_opt_u16(cursor)?,
        slant: read_opt_font_slant(cursor)?,
        width: read_opt_font_width(cursor)?,
        repertory: read_opt_font_repertory(cursor)?,
    })
}

fn write_opt_font_repertory(
    out: &mut Vec<u8>,
    repertory: Option<&DumpFontRepertory>,
) -> Result<(), DumpError> {
    match repertory {
        Some(repertory) => {
            write_bool(out, true);
            write_font_repertory(out, repertory)?;
        }
        None => write_bool(out, false),
    }
    Ok(())
}

fn read_opt_font_repertory(
    cursor: &mut Cursor<'_>,
) -> Result<Option<DumpFontRepertory>, DumpError> {
    if cursor.read_bool("font repertory present")? {
        Ok(Some(read_font_repertory(cursor)?))
    } else {
        Ok(None)
    }
}

fn write_font_repertory(out: &mut Vec<u8>, repertory: &DumpFontRepertory) -> Result<(), DumpError> {
    match repertory {
        DumpFontRepertory::Charset(name) => {
            write_u8(out, 0);
            write_string(out, name)?;
        }
        DumpFontRepertory::CharTableRanges(ranges) => {
            write_u8(out, 1);
            write_vec(
                out,
                ranges,
                "font repertory range count",
                |out, (from, to)| {
                    write_u32(out, *from);
                    write_u32(out, *to);
                    Ok(())
                },
            )?;
        }
        DumpFontRepertory::CharsetSym(sym) => {
            write_u8(out, 2);
            write_sym(out, *sym);
        }
    }
    Ok(())
}

fn read_font_repertory(cursor: &mut Cursor<'_>) -> Result<DumpFontRepertory, DumpError> {
    Ok(match cursor.read_u8("font repertory tag")? {
        0 => DumpFontRepertory::Charset(cursor.read_string()?),
        1 => DumpFontRepertory::CharTableRanges(read_vec(
            cursor,
            "font repertory range count",
            |cursor| {
                Ok((
                    cursor.read_u32("font repertory range start")?,
                    cursor.read_u32("font repertory range end")?,
                ))
            },
        )?),
        2 => DumpFontRepertory::CharsetSym(read_sym(cursor)?),
        other => {
            return Err(DumpError::ImageFormatError(format!(
                "unknown font repertory tag {other}"
            )));
        }
    })
}

fn write_opt_font_width(out: &mut Vec<u8>, width: Option<DumpFontWidth>) {
    match width {
        Some(width) => {
            write_bool(out, true);
            write_font_width(out, width);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_font_width(cursor: &mut Cursor<'_>) -> Result<Option<DumpFontWidth>, DumpError> {
    if cursor.read_bool("font width present")? {
        Ok(Some(read_font_width(cursor)?))
    } else {
        Ok(None)
    }
}

fn write_font_width(out: &mut Vec<u8>, width: DumpFontWidth) {
    write_u8(out, width.into());
}

fn read_font_width(cursor: &mut Cursor<'_>) -> Result<DumpFontWidth, DumpError> {
    let tag = cursor.read_u8("font width tag")?;
    DumpFontWidth::try_from(tag)
        .map_err(|_| DumpError::ImageFormatError(format!("unknown font width tag {tag}")))
}

fn write_opt_font_slant(out: &mut Vec<u8>, slant: Option<DumpFontSlant>) {
    match slant {
        Some(slant) => {
            write_bool(out, true);
            write_font_slant(out, slant);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_font_slant(cursor: &mut Cursor<'_>) -> Result<Option<DumpFontSlant>, DumpError> {
    if cursor.read_bool("font slant present")? {
        Ok(Some(read_font_slant(cursor)?))
    } else {
        Ok(None)
    }
}

fn write_font_slant(out: &mut Vec<u8>, slant: DumpFontSlant) {
    write_u8(out, slant.into());
}

fn read_font_slant(cursor: &mut Cursor<'_>) -> Result<DumpFontSlant, DumpError> {
    let tag = cursor.read_u8("font slant tag")?;
    DumpFontSlant::try_from(tag)
        .map_err(|_| DumpError::ImageFormatError(format!("unknown font slant tag {tag}")))
}

fn write_abbrev_manager(out: &mut Vec<u8>, manager: &DumpAbbrevManager) -> Result<(), DumpError> {
    write_vec(
        out,
        &manager.tables_syms,
        "abbrev symbol table count",
        |out, (sym, table)| {
            write_sym(out, *sym);
            write_abbrev_table(out, table)
        },
    )?;
    write_vec(
        out,
        &manager.tables,
        "abbrev string table count",
        |out, (name, table)| {
            write_string(out, name)?;
            write_abbrev_table(out, table)
        },
    )?;
    write_opt_sym(out, manager.global_table_sym);
    write_lisp_string(out, &manager.global_table_name)?;
    write_bool(out, manager.abbrev_mode);
    Ok(())
}

fn read_abbrev_manager(cursor: &mut Cursor<'_>) -> Result<DumpAbbrevManager, DumpError> {
    Ok(DumpAbbrevManager {
        tables_syms: read_vec(cursor, "abbrev symbol table count", |cursor| {
            Ok((read_sym(cursor)?, read_abbrev_table(cursor)?))
        })?,
        tables: read_vec(cursor, "abbrev string table count", |cursor| {
            Ok((cursor.read_string()?, read_abbrev_table(cursor)?))
        })?,
        global_table_sym: read_opt_sym(cursor)?,
        global_table_name: read_lisp_string(cursor)?,
        abbrev_mode: cursor.read_bool("abbrev-mode flag")?,
    })
}

fn write_abbrev_table(out: &mut Vec<u8>, table: &DumpAbbrevTable) -> Result<(), DumpError> {
    write_lisp_string(out, &table.name)?;
    write_vec(
        out,
        &table.abbrevs,
        "abbrev count",
        |out, (name, abbrev)| {
            write_lisp_string(out, name)?;
            write_abbrev(out, abbrev)
        },
    )?;
    write_opt_lisp_string(out, table.parent.as_ref())?;
    write_bool(out, table.case_fixed);
    write_bool(out, table.enable_quoting);
    Ok(())
}

fn read_abbrev_table(cursor: &mut Cursor<'_>) -> Result<DumpAbbrevTable, DumpError> {
    Ok(DumpAbbrevTable {
        name: read_lisp_string(cursor)?,
        abbrevs: read_vec(cursor, "abbrev count", |cursor| {
            Ok((read_lisp_string(cursor)?, read_abbrev(cursor)?))
        })?,
        parent: read_opt_lisp_string(cursor)?,
        case_fixed: cursor.read_bool("abbrev case-fixed flag")?,
        enable_quoting: cursor.read_bool("abbrev enable-quoting flag")?,
    })
}

fn write_abbrev(out: &mut Vec<u8>, abbrev: &DumpAbbrev) -> Result<(), DumpError> {
    write_lisp_string(out, &abbrev.expansion)?;
    write_opt_lisp_string(out, abbrev.hook.as_ref())?;
    write_usize(out, abbrev.count)?;
    write_bool(out, abbrev.system);
    Ok(())
}

fn read_abbrev(cursor: &mut Cursor<'_>) -> Result<DumpAbbrev, DumpError> {
    Ok(DumpAbbrev {
        expansion: read_lisp_string(cursor)?,
        hook: read_opt_lisp_string(cursor)?,
        count: cursor.read_usize("abbrev count")?,
        system: cursor.read_bool("abbrev system flag")?,
    })
}

fn write_interactive_registry(
    out: &mut Vec<u8>,
    registry: &DumpInteractiveRegistry,
) -> Result<(), DumpError> {
    write_vec(
        out,
        &registry.specs,
        "interactive spec count",
        |out, (sym, spec)| {
            write_sym(out, *sym);
            write_value(out, &spec.spec)
        },
    )
}

fn read_interactive_registry(
    cursor: &mut Cursor<'_>,
) -> Result<DumpInteractiveRegistry, DumpError> {
    Ok(DumpInteractiveRegistry {
        specs: read_vec(cursor, "interactive spec count", |cursor| {
            Ok((
                read_sym(cursor)?,
                DumpInteractiveSpec {
                    spec: cursor.read_value()?,
                },
            ))
        })?,
    })
}

fn write_rectangle_state(out: &mut Vec<u8>, state: &DumpRectangleState) -> Result<(), DumpError> {
    write_lisp_string_vec(out, &state.killed)
}

fn read_rectangle_state(cursor: &mut Cursor<'_>) -> Result<DumpRectangleState, DumpError> {
    Ok(DumpRectangleState {
        killed: read_lisp_string_vec(cursor)?,
    })
}

fn write_kmacro_manager(out: &mut Vec<u8>, manager: &DumpKmacroManager) -> Result<(), DumpError> {
    write_values(out, &manager.current_macro)?;
    write_opt_value_vec(out, manager.last_macro.as_deref())?;
    write_vec(
        out,
        &manager.macro_ring,
        "kmacro ring count",
        |out, values| write_values(out, values),
    )?;
    write_i64(out, manager.counter);
    write_opt_lisp_string(out, manager.counter_format_lisp.as_ref())?;
    write_opt_string(out, manager.counter_format.as_deref())?;
    Ok(())
}

fn read_kmacro_manager(cursor: &mut Cursor<'_>) -> Result<DumpKmacroManager, DumpError> {
    Ok(DumpKmacroManager {
        current_macro: read_values(cursor)?,
        last_macro: read_opt_value_vec(cursor)?,
        macro_ring: read_vec(cursor, "kmacro ring count", read_values)?,
        counter: cursor.read_i64("kmacro counter")?,
        counter_format_lisp: read_opt_lisp_string(cursor)?,
        counter_format: read_opt_string(cursor)?,
    })
}

fn write_register_manager(
    out: &mut Vec<u8>,
    manager: &DumpRegisterManager,
) -> Result<(), DumpError> {
    write_vec(
        out,
        &manager.registers,
        "register count",
        |out, (name, content)| {
            write_char(out, *name);
            write_register_content(out, content)
        },
    )
}

fn read_register_manager(cursor: &mut Cursor<'_>) -> Result<DumpRegisterManager, DumpError> {
    Ok(DumpRegisterManager {
        registers: read_vec(cursor, "register count", |cursor| {
            Ok((
                read_char(cursor, "register name")?,
                read_register_content(cursor)?,
            ))
        })?,
    })
}

fn write_register_content(
    out: &mut Vec<u8>,
    content: &DumpRegisterContent,
) -> Result<(), DumpError> {
    match content {
        DumpRegisterContent::Text {
            data,
            size,
            size_byte,
        } => {
            write_u8(out, 0);
            write_bytes(out, data)?;
            write_usize(out, *size)?;
            write_i64(out, *size_byte);
        }
        DumpRegisterContent::Number(value) => {
            write_u8(out, 1);
            write_i64(out, *value);
        }
        DumpRegisterContent::Marker(value) => {
            write_u8(out, 2);
            write_value(out, value)?;
        }
        DumpRegisterContent::Rectangle(lines) => {
            write_u8(out, 3);
            write_lisp_string_vec(out, lines)?;
        }
        DumpRegisterContent::FrameConfig(value) => {
            write_u8(out, 4);
            write_value(out, value)?;
        }
        DumpRegisterContent::File(file) => {
            write_u8(out, 5);
            write_lisp_string(out, file)?;
        }
        DumpRegisterContent::KbdMacro(values) => {
            write_u8(out, 6);
            write_values(out, values)?;
        }
    }
    Ok(())
}

fn read_register_content(cursor: &mut Cursor<'_>) -> Result<DumpRegisterContent, DumpError> {
    Ok(match cursor.read_u8("register content tag")? {
        0 => DumpRegisterContent::Text {
            data: cursor.read_bytes()?,
            size: cursor.read_usize("register text char size")?,
            size_byte: cursor.read_i64("register text byte size")?,
        },
        1 => DumpRegisterContent::Number(cursor.read_i64("register number")?),
        2 => DumpRegisterContent::Marker(cursor.read_value()?),
        3 => DumpRegisterContent::Rectangle(read_lisp_string_vec(cursor)?),
        4 => DumpRegisterContent::FrameConfig(cursor.read_value()?),
        5 => DumpRegisterContent::File(read_lisp_string(cursor)?),
        6 => DumpRegisterContent::KbdMacro(read_values(cursor)?),
        other => {
            return Err(DumpError::ImageFormatError(format!(
                "unknown register content tag {other}"
            )));
        }
    })
}

fn write_bookmark_manager(
    out: &mut Vec<u8>,
    manager: &DumpBookmarkManager,
) -> Result<(), DumpError> {
    write_vec(
        out,
        &manager.bookmarks_lisp,
        "Lisp bookmark count",
        |out, (name, bookmark)| {
            write_lisp_string(out, name)?;
            write_bookmark(out, bookmark)
        },
    )?;
    write_vec(
        out,
        &manager.bookmarks,
        "string bookmark count",
        |out, (name, bookmark)| {
            write_string(out, name)?;
            write_bookmark(out, bookmark)
        },
    )?;
    write_lisp_string_vec(out, &manager.recent)?;
    Ok(())
}

fn read_bookmark_manager(cursor: &mut Cursor<'_>) -> Result<DumpBookmarkManager, DumpError> {
    Ok(DumpBookmarkManager {
        bookmarks_lisp: read_vec(cursor, "Lisp bookmark count", |cursor| {
            Ok((read_lisp_string(cursor)?, read_bookmark(cursor)?))
        })?,
        bookmarks: read_vec(cursor, "string bookmark count", |cursor| {
            Ok((cursor.read_string()?, read_bookmark(cursor)?))
        })?,
        recent: read_lisp_string_vec(cursor)?,
    })
}

fn write_bookmark(out: &mut Vec<u8>, bookmark: &DumpBookmark) -> Result<(), DumpError> {
    write_lisp_string(out, &bookmark.name)?;
    write_opt_lisp_string(out, bookmark.filename.as_ref())?;
    write_usize(out, bookmark.position)?;
    write_opt_lisp_string(out, bookmark.front_context.as_ref())?;
    write_opt_lisp_string(out, bookmark.rear_context.as_ref())?;
    write_opt_lisp_string(out, bookmark.annotation.as_ref())?;
    write_opt_lisp_string(out, bookmark.handler.as_ref())?;
    Ok(())
}

fn read_bookmark(cursor: &mut Cursor<'_>) -> Result<DumpBookmark, DumpError> {
    Ok(DumpBookmark {
        name: read_lisp_string(cursor)?,
        filename: read_opt_lisp_string(cursor)?,
        position: cursor.read_usize("bookmark position")?,
        front_context: read_opt_lisp_string(cursor)?,
        rear_context: read_opt_lisp_string(cursor)?,
        annotation: read_opt_lisp_string(cursor)?,
        handler: read_opt_lisp_string(cursor)?,
    })
}

fn write_variable_watchers(
    out: &mut Vec<u8>,
    watchers: &DumpVariableWatcherList,
) -> Result<(), DumpError> {
    write_vec(
        out,
        &watchers.watchers,
        "variable watcher count",
        |out, (sym, values)| {
            write_sym(out, *sym);
            write_values(out, values)
        },
    )
}

fn read_variable_watchers(cursor: &mut Cursor<'_>) -> Result<DumpVariableWatcherList, DumpError> {
    Ok(DumpVariableWatcherList {
        watchers: read_vec(cursor, "variable watcher count", |cursor| {
            Ok((read_sym(cursor)?, read_values(cursor)?))
        })?,
    })
}

fn write_lisp_string_value_pairs(
    out: &mut Vec<u8>,
    pairs: &[(DumpLispString, DumpValue)],
) -> Result<(), DumpError> {
    write_vec(
        out,
        pairs,
        "Lisp string/value pair count",
        |out, (string, value)| {
            write_lisp_string(out, string)?;
            write_value(out, value)
        },
    )
}

fn read_lisp_string_value_pairs(
    cursor: &mut Cursor<'_>,
) -> Result<Vec<(DumpLispString, DumpValue)>, DumpError> {
    read_vec(cursor, "Lisp string/value pair count", |cursor| {
        Ok((read_lisp_string(cursor)?, cursor.read_value()?))
    })
}

fn write_string_value_pairs(
    out: &mut Vec<u8>,
    pairs: &[(String, DumpValue)],
) -> Result<(), DumpError> {
    write_vec(
        out,
        pairs,
        "string/value pair count",
        |out, (string, value)| {
            write_string(out, string)?;
            write_value(out, value)
        },
    )
}

fn read_string_value_pairs(cursor: &mut Cursor<'_>) -> Result<Vec<(String, DumpValue)>, DumpError> {
    read_vec(cursor, "string/value pair count", |cursor| {
        Ok((cursor.read_string()?, cursor.read_value()?))
    })
}

fn write_values(out: &mut Vec<u8>, values: &[DumpValue]) -> Result<(), DumpError> {
    write_vec(out, values, "value count", write_value)
}

fn read_values(cursor: &mut Cursor<'_>) -> Result<Vec<DumpValue>, DumpError> {
    read_vec(cursor, "value count", |cursor| cursor.read_value())
}

fn write_opt_value(out: &mut Vec<u8>, value: Option<&DumpValue>) -> Result<(), DumpError> {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_value(out, value)?;
        }
        None => write_bool(out, false),
    }
    Ok(())
}

fn read_opt_value(cursor: &mut Cursor<'_>) -> Result<Option<DumpValue>, DumpError> {
    if cursor.read_bool("value present")? {
        Ok(Some(cursor.read_value()?))
    } else {
        Ok(None)
    }
}

fn write_opt_value_vec(out: &mut Vec<u8>, values: Option<&[DumpValue]>) -> Result<(), DumpError> {
    match values {
        Some(values) => {
            write_bool(out, true);
            write_values(out, values)?;
        }
        None => write_bool(out, false),
    }
    Ok(())
}

fn read_opt_value_vec(cursor: &mut Cursor<'_>) -> Result<Option<Vec<DumpValue>>, DumpError> {
    if cursor.read_bool("value vector present")? {
        Ok(Some(read_values(cursor)?))
    } else {
        Ok(None)
    }
}

fn write_sym_vec(out: &mut Vec<u8>, syms: &[DumpSymId]) -> Result<(), DumpError> {
    write_vec(out, syms, "symbol count", |out, sym| {
        write_sym(out, *sym);
        Ok(())
    })
}

fn read_sym_vec(cursor: &mut Cursor<'_>) -> Result<Vec<DumpSymId>, DumpError> {
    read_vec(cursor, "symbol count", read_sym)
}

fn write_sym(out: &mut Vec<u8>, sym: DumpSymId) {
    write_u32(out, sym.0);
}

fn read_sym(cursor: &mut Cursor<'_>) -> Result<DumpSymId, DumpError> {
    Ok(DumpSymId(cursor.read_u32("symbol id")?))
}

fn write_opt_sym(out: &mut Vec<u8>, sym: Option<DumpSymId>) {
    match sym {
        Some(sym) => {
            write_bool(out, true);
            write_sym(out, sym);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_sym(cursor: &mut Cursor<'_>) -> Result<Option<DumpSymId>, DumpError> {
    if cursor.read_bool("symbol present")? {
        Ok(Some(read_sym(cursor)?))
    } else {
        Ok(None)
    }
}

fn write_lisp_string_vec(out: &mut Vec<u8>, values: &[DumpLispString]) -> Result<(), DumpError> {
    write_vec(out, values, "Lisp string count", write_lisp_string)
}

fn read_lisp_string_vec(cursor: &mut Cursor<'_>) -> Result<Vec<DumpLispString>, DumpError> {
    read_vec(cursor, "Lisp string count", read_lisp_string)
}

fn write_lisp_string(out: &mut Vec<u8>, value: &DumpLispString) -> Result<(), DumpError> {
    write_bytes(out, &value.data)?;
    write_usize(out, value.size)?;
    write_i64(out, value.size_byte);
    Ok(())
}

fn read_lisp_string(cursor: &mut Cursor<'_>) -> Result<DumpLispString, DumpError> {
    Ok(DumpLispString {
        data: cursor.read_bytes()?,
        size: cursor.read_usize("Lisp string char size")?,
        size_byte: cursor.read_i64("Lisp string byte size")?,
    })
}

fn write_opt_lisp_string(
    out: &mut Vec<u8>,
    value: Option<&DumpLispString>,
) -> Result<(), DumpError> {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_lisp_string(out, value)?;
        }
        None => write_bool(out, false),
    }
    Ok(())
}

fn read_opt_lisp_string(cursor: &mut Cursor<'_>) -> Result<Option<DumpLispString>, DumpError> {
    if cursor.read_bool("Lisp string present")? {
        Ok(Some(read_lisp_string(cursor)?))
    } else {
        Ok(None)
    }
}

fn write_string_vec(out: &mut Vec<u8>, values: &[String]) -> Result<(), DumpError> {
    write_vec(out, values, "string count", |out, value| {
        write_string(out, value)
    })
}

fn read_string_vec(cursor: &mut Cursor<'_>) -> Result<Vec<String>, DumpError> {
    read_vec(cursor, "string count", |cursor| cursor.read_string())
}

fn write_opt_string(out: &mut Vec<u8>, value: Option<&str>) -> Result<(), DumpError> {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_string(out, value)?;
        }
        None => write_bool(out, false),
    }
    Ok(())
}

fn read_opt_string(cursor: &mut Cursor<'_>) -> Result<Option<String>, DumpError> {
    if cursor.read_bool("string present")? {
        Ok(Some(cursor.read_string()?))
    } else {
        Ok(None)
    }
}

fn write_string(out: &mut Vec<u8>, value: &str) -> Result<(), DumpError> {
    write_bytes(out, value.as_bytes())
}

fn write_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), DumpError> {
    write_len(out, bytes.len(), "byte payload length")?;
    out.extend_from_slice(bytes);
    Ok(())
}

fn write_vec<T>(
    out: &mut Vec<u8>,
    values: &[T],
    what: &str,
    mut write_one: impl FnMut(&mut Vec<u8>, &T) -> Result<(), DumpError>,
) -> Result<(), DumpError> {
    write_len(out, values.len(), what)?;
    for value in values {
        write_one(out, value)?;
    }
    Ok(())
}

fn read_vec<T>(
    cursor: &mut Cursor<'_>,
    what: &str,
    mut read_one: impl FnMut(&mut Cursor<'_>) -> Result<T, DumpError>,
) -> Result<Vec<T>, DumpError> {
    let len = cursor.read_len(what)?;
    let mut values = Vec::with_capacity(len);
    for _ in 0..len {
        values.push(read_one(cursor)?);
    }
    Ok(values)
}

fn write_opt_u16(out: &mut Vec<u8>, value: Option<u16>) {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_u16(out, value);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_u16(cursor: &mut Cursor<'_>) -> Result<Option<u16>, DumpError> {
    if cursor.read_bool("u16 present")? {
        Ok(Some(cursor.read_u16("u16 value")?))
    } else {
        Ok(None)
    }
}

fn write_char(out: &mut Vec<u8>, value: char) {
    write_u32(out, value as u32);
}

fn read_char(cursor: &mut Cursor<'_>, what: &str) -> Result<char, DumpError> {
    let raw = cursor.read_u32(what)?;
    char::from_u32(raw)
        .ok_or_else(|| DumpError::ImageFormatError(format!("{what} has invalid scalar {raw}")))
}

fn write_len(out: &mut Vec<u8>, len: usize, what: &str) -> Result<(), DumpError> {
    let len = u64::try_from(len)
        .map_err(|_| DumpError::SerializationError(format!("{what} overflows u64")))?;
    write_u64(out, len);
    Ok(())
}

fn write_usize(out: &mut Vec<u8>, value: usize) -> Result<(), DumpError> {
    let value = u64::try_from(value)
        .map_err(|_| DumpError::SerializationError("usize value overflows u64".into()))?;
    write_u64(out, value);
    Ok(())
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_ne_bytes());
}

fn write_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_ne_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lisp_string(bytes: &[u8]) -> DumpLispString {
        DumpLispString {
            data: bytes.to_vec(),
            size: bytes.len(),
            size_byte: bytes.len() as i64,
        }
    }

    #[test]
    fn runtime_managers_section_round_trips_representative_state() {
        let managers = RuntimeManagersState {
            custom: DumpCustomManager {
                auto_buffer_local_syms: vec![DumpSymId(1)],
                auto_buffer_local: vec!["legacy-local".to_string()],
            },
            modes: DumpModeRegistry {
                major_modes: vec![(
                    DumpSymId(2),
                    DumpMajorMode {
                        pretty_name: lisp_string(b"Probe"),
                        parent: Some(DumpValue::Symbol(DumpSymId(3))),
                        mode_hook: DumpValue::Nil,
                        keymap_name: Some(DumpValue::Symbol(DumpSymId(4))),
                        syntax_table_name: None,
                        abbrev_table_name: None,
                        font_lock: Some(DumpFontLockDefaults {
                            keywords: vec![DumpFontLockKeyword {
                                pattern_lisp: Some(lisp_string(b"rx")),
                                pattern: None,
                                face_sym: Some(DumpSymId(5)),
                                face: None,
                                group: 1,
                                override_: true,
                                laxmatch: false,
                            }],
                            case_fold: true,
                            syntax_table_lisp: None,
                            syntax_table: Some("syntax".to_string()),
                        }),
                        body: Some(DumpValue::Int(9)),
                    },
                )],
                minor_modes: vec![(
                    DumpSymId(6),
                    DumpMinorMode {
                        lighter: Some(lisp_string(b" L")),
                        keymap_name: None,
                        global: false,
                        body: Some(DumpValue::True),
                    },
                )],
                buffer_major_modes: vec![(7, DumpValue::Symbol(DumpSymId(8)))],
                buffer_minor_modes: vec![(7, vec![DumpValue::Symbol(DumpSymId(9))])],
                global_minor_modes: vec![DumpValue::Symbol(DumpSymId(10))],
                auto_mode_alist_lisp: vec![(
                    lisp_string(b"\\.rs\\'"),
                    DumpValue::Symbol(DumpSymId(11)),
                )],
                auto_mode_alist: vec![("legacy".to_string(), DumpValue::Nil)],
                custom_variables: vec![(
                    DumpSymId(12),
                    DumpModeCustomVariable {
                        default_value: DumpValue::Int(1),
                        doc: Some(lisp_string(b"doc")),
                        custom_type: DumpModeCustomType::Choice(vec![(
                            "one".to_string(),
                            DumpValue::Int(1),
                        )]),
                        group: None,
                        set_function: None,
                        get_function: None,
                        tag: None,
                    },
                )],
                custom_groups: vec![(
                    DumpSymId(13),
                    DumpModeCustomGroup {
                        doc: None,
                        parent: Some(DumpValue::Symbol(DumpSymId(14))),
                        members: vec![DumpValue::Symbol(DumpSymId(15))],
                    },
                )],
                fundamental_mode: DumpValue::Symbol(DumpSymId(16)),
            },
            fontset_registry: DumpFontsetRegistry {
                ordered_names_lisp: vec![lisp_string(b"default")],
                alias_to_name_lisp: vec![(lisp_string(b"alias"), lisp_string(b"default"))],
                fontsets_lisp: vec![(
                    lisp_string(b"default"),
                    DumpFontsetData {
                        ranges: vec![DumpFontsetRangeEntry {
                            from: 0,
                            to: 127,
                            entries: vec![DumpFontSpecEntry::Font(DumpStoredFontSpec {
                                family_sym: Some(DumpSymId(17)),
                                family: None,
                                registry_sym: None,
                                registry: Some("iso10646-1".to_string()),
                                lang_sym: None,
                                lang: None,
                                weight: Some(400),
                                slant: Some(DumpFontSlant::Normal),
                                width: Some(DumpFontWidth::Normal),
                                repertory: Some(DumpFontRepertory::CharsetSym(DumpSymId(18))),
                            })],
                        }],
                        fallback: Some(vec![DumpFontSpecEntry::ExplicitNone]),
                    },
                )],
                ordered_names: vec!["legacy-default".to_string()],
                alias_to_name: vec![("legacy-alias".to_string(), "legacy-default".to_string())],
                fontsets: Vec::new(),
                generation: 4,
            },
            abbrevs: DumpAbbrevManager {
                tables_syms: vec![(
                    DumpSymId(19),
                    DumpAbbrevTable {
                        name: lisp_string(b"table"),
                        abbrevs: vec![(
                            lisp_string(b"btw"),
                            DumpAbbrev {
                                expansion: lisp_string(b"by the way"),
                                hook: None,
                                count: 3,
                                system: false,
                            },
                        )],
                        parent: None,
                        case_fixed: true,
                        enable_quoting: false,
                    },
                )],
                tables: Vec::new(),
                global_table_sym: Some(DumpSymId(19)),
                global_table_name: lisp_string(b"table"),
                abbrev_mode: true,
            },
            interactive: DumpInteractiveRegistry {
                specs: vec![(
                    DumpSymId(20),
                    DumpInteractiveSpec {
                        spec: DumpValue::Int(1),
                    },
                )],
            },
            rectangle: DumpRectangleState {
                killed: vec![lisp_string(b"rect")],
            },
            kmacro: DumpKmacroManager {
                current_macro: vec![DumpValue::Int(1)],
                last_macro: Some(vec![DumpValue::Int(2)]),
                macro_ring: vec![vec![DumpValue::Int(3)]],
                counter: 5,
                counter_format_lisp: Some(lisp_string(b"%d")),
                counter_format: None,
            },
            registers: DumpRegisterManager {
                registers: vec![('a', DumpRegisterContent::File(lisp_string(b"/tmp/a")))],
            },
            bookmarks: DumpBookmarkManager {
                bookmarks_lisp: vec![(
                    lisp_string(b"home"),
                    DumpBookmark {
                        name: lisp_string(b"home"),
                        filename: Some(lisp_string(b"/tmp/home")),
                        position: 12,
                        front_context: None,
                        rear_context: None,
                        annotation: Some(lisp_string(b"note")),
                        handler: None,
                    },
                )],
                bookmarks: Vec::new(),
                recent: vec![lisp_string(b"home")],
            },
            watchers: DumpVariableWatcherList {
                watchers: vec![(DumpSymId(21), vec![DumpValue::Symbol(DumpSymId(22))])],
            },
        };

        let bytes = runtime_managers_section_bytes(&managers).expect("encode runtime managers");
        let loaded = load_runtime_managers_section(&bytes).expect("decode runtime managers");
        assert_eq!(loaded.custom.auto_buffer_local_syms.len(), 1);
        assert_eq!(loaded.modes.major_modes.len(), 1);
        assert_eq!(loaded.fontset_registry.fontsets_lisp.len(), 1);
        assert_eq!(loaded.abbrevs.tables_syms.len(), 1);
        assert_eq!(loaded.interactive.specs.len(), 1);
        assert_eq!(loaded.rectangle.killed.len(), 1);
        assert_eq!(loaded.kmacro.counter, 5);
        assert_eq!(loaded.registers.registers.len(), 1);
        assert_eq!(loaded.bookmarks.bookmarks_lisp.len(), 1);
        assert_eq!(loaded.watchers.watchers.len(), 1);
    }
}
