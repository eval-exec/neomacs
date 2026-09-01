//! Fixed-layout pdump section for Lisp face definitions.
//!
//! GNU Emacs dumps Lisp face definition vectors through the regular object
//! graph and rebuilds realized frame face caches at runtime.  This section
//! carries Neomacs' logical face table separately from RuntimeState bincode.

use bytemuck::{Pod, Zeroable};

use super::DumpError;
use super::object_value_codec::{Cursor, write_bool, write_u8, write_u32, write_u64, write_value};
use super::types::{
    DumpBoxBorder, DumpBoxStyle, DumpColor, DumpFace, DumpFaceHeight, DumpFaceTable, DumpFontSlant,
    DumpSymId, DumpUnderline, DumpUnderlineStyle, DumpValue,
};

const FACE_MAGIC: [u8; 16] = *b"NEOFACE\0\0\0\0\0\0\0\0\0";
const FACE_FORMAT_VERSION: u32 = 3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct FaceHeader {
    magic: [u8; 16],
    version: u32,
    header_size: u32,
    face_id_count: u64,
    face_string_count: u64,
    payload_offset: u64,
    payload_len: u64,
}

const HEADER_SIZE: usize = std::mem::size_of::<FaceHeader>();

pub(crate) fn face_table_section_bytes(table: &DumpFaceTable) -> Result<Vec<u8>, DumpError> {
    let mut bytes = vec![0; HEADER_SIZE];

    for (sym, face) in &table.face_ids {
        write_u32(&mut bytes, sym.0);
        write_face(&mut bytes, face)?;
    }
    for (name, face) in &table.faces {
        write_string(&mut bytes, name)?;
        write_face(&mut bytes, face)?;
    }

    let payload_len = bytes.len() - HEADER_SIZE;
    let header = FaceHeader {
        magic: FACE_MAGIC,
        version: FACE_FORMAT_VERSION,
        header_size: HEADER_SIZE as u32,
        face_id_count: count_u64(table.face_ids.len(), "face id count")?,
        face_string_count: count_u64(table.faces.len(), "face string count")?,
        payload_offset: HEADER_SIZE as u64,
        payload_len: payload_len as u64,
    };
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    Ok(bytes)
}

pub(crate) fn load_face_table_section(section: &[u8]) -> Result<DumpFaceTable, DumpError> {
    let header = read_header(section)?;
    let payload_offset = usize::try_from(header.payload_offset)
        .map_err(|_| DumpError::ImageFormatError("face payload offset overflows usize".into()))?;
    let payload_len = usize::try_from(header.payload_len)
        .map_err(|_| DumpError::ImageFormatError("face payload length overflows usize".into()))?;
    let end = payload_offset
        .checked_add(payload_len)
        .ok_or_else(|| DumpError::ImageFormatError("face payload range overflows".into()))?;
    if payload_offset < HEADER_SIZE || end > section.len() {
        return Err(DumpError::ImageFormatError(
            "face payload range is outside section".into(),
        ));
    }

    let mut cursor = Cursor::new(&section[payload_offset..end]);
    let mut face_ids = Vec::with_capacity(to_usize(header.face_id_count, "face id count")?);
    for _ in 0..header.face_id_count {
        face_ids.push((
            DumpSymId(cursor.read_u32("face symbol")?),
            read_face(&mut cursor)?,
        ));
    }
    let mut faces = Vec::with_capacity(to_usize(header.face_string_count, "face string count")?);
    for _ in 0..header.face_string_count {
        faces.push((read_string(&mut cursor)?, read_face(&mut cursor)?));
    }

    if !cursor.is_empty() {
        return Err(DumpError::ImageFormatError(format!(
            "face section has {} trailing payload bytes",
            cursor.remaining()
        )));
    }

    Ok(DumpFaceTable { face_ids, faces })
}

fn read_header(section: &[u8]) -> Result<FaceHeader, DumpError> {
    if section.len() < HEADER_SIZE {
        return Err(DumpError::ImageFormatError(format!(
            "face section shorter than header: {} < {HEADER_SIZE}",
            section.len()
        )));
    }
    let header = *bytemuck::from_bytes::<FaceHeader>(&section[..HEADER_SIZE]);
    if header.magic != FACE_MAGIC {
        return Err(DumpError::ImageFormatError(
            "face section has bad magic".into(),
        ));
    }
    if header.version != FACE_FORMAT_VERSION {
        return Err(DumpError::UnsupportedVersion(header.version));
    }
    if header.header_size != HEADER_SIZE as u32 {
        return Err(DumpError::ImageFormatError(format!(
            "face header size {} does not match runtime header size {HEADER_SIZE}",
            header.header_size
        )));
    }
    Ok(header)
}

fn write_face(out: &mut Vec<u8>, face: &DumpFace) -> Result<(), DumpError> {
    write_opt_color(out, face.foreground);
    write_opt_color(out, face.background);
    write_opt_value(out, face.family_value.as_ref())?;
    write_opt_string(out, face.family.as_deref())?;
    write_opt_value(out, face.foundry_value.as_ref())?;
    write_opt_string(out, face.foundry.as_deref())?;
    write_opt_face_height(out, face.height.as_ref());
    write_opt_u16(out, face.weight);
    write_opt_font_slant(out, face.slant.as_ref());
    write_bool(out, face.underline_disabled);
    write_opt_underline(out, face.underline.as_ref())?;
    write_opt_bool(out, face.overline);
    write_opt_bool(out, face.strike_through);
    write_bool(out, face.box_disabled);
    write_opt_box_border(out, face.box_border.as_ref());
    write_opt_bool(out, face.inverse_video);
    write_opt_value(out, face.stipple_value.as_ref())?;
    write_opt_string(out, face.stipple.as_deref())?;
    write_opt_bool(out, face.extend);
    write_sym_vec(out, &face.inherit_syms)?;
    write_string_vec(out, &face.inherit)?;
    write_bool(out, face.overstrike);
    write_opt_value(out, face.doc_value.as_ref())?;
    write_opt_string(out, face.doc.as_deref())?;
    Ok(())
}

fn read_face(cursor: &mut Cursor<'_>) -> Result<DumpFace, DumpError> {
    Ok(DumpFace {
        foreground: read_opt_color(cursor)?,
        background: read_opt_color(cursor)?,
        family_value: read_opt_value(cursor)?,
        family: read_opt_string(cursor)?,
        foundry_value: read_opt_value(cursor)?,
        foundry: read_opt_string(cursor)?,
        height: read_opt_face_height(cursor)?,
        weight: read_opt_u16(cursor, "face weight")?,
        slant: read_opt_font_slant(cursor)?,
        underline_disabled: cursor.read_bool("face underline disabled")?,
        underline: read_opt_underline(cursor)?,
        overline: read_opt_bool(cursor)?,
        strike_through: read_opt_bool(cursor)?,
        box_disabled: cursor.read_bool("face box disabled")?,
        box_border: read_opt_box_border(cursor)?,
        inverse_video: read_opt_bool(cursor)?,
        stipple_value: read_opt_value(cursor)?,
        stipple: read_opt_string(cursor)?,
        extend: read_opt_bool(cursor)?,
        inherit_syms: read_sym_vec(cursor)?,
        inherit: read_string_vec(cursor)?,
        overstrike: cursor.read_bool("face overstrike")?,
        doc_value: read_opt_value(cursor)?,
        doc: read_opt_string(cursor)?,
    })
}

fn write_color(out: &mut Vec<u8>, color: DumpColor) {
    write_u8(out, color.r);
    write_u8(out, color.g);
    write_u8(out, color.b);
    write_u8(out, color.a);
}

fn read_color(cursor: &mut Cursor<'_>) -> Result<DumpColor, DumpError> {
    Ok(DumpColor {
        r: cursor.read_u8("color red")?,
        g: cursor.read_u8("color green")?,
        b: cursor.read_u8("color blue")?,
        a: cursor.read_u8("color alpha")?,
    })
}

fn write_opt_color(out: &mut Vec<u8>, color: Option<DumpColor>) {
    match color {
        Some(color) => {
            write_bool(out, true);
            write_color(out, color);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_color(cursor: &mut Cursor<'_>) -> Result<Option<DumpColor>, DumpError> {
    if cursor.read_bool("optional color present")? {
        Ok(Some(read_color(cursor)?))
    } else {
        Ok(None)
    }
}

fn write_font_slant(out: &mut Vec<u8>, slant: &DumpFontSlant) {
    write_u8(out, (*slant).into());
}

fn read_font_slant(cursor: &mut Cursor<'_>) -> Result<DumpFontSlant, DumpError> {
    let tag = cursor.read_u8("font slant")?;
    DumpFontSlant::try_from(tag)
        .map_err(|_| DumpError::ImageFormatError(format!("unknown font slant tag {tag}")))
}

fn write_opt_font_slant(out: &mut Vec<u8>, slant: Option<&DumpFontSlant>) {
    match slant {
        Some(slant) => {
            write_bool(out, true);
            write_font_slant(out, slant);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_font_slant(cursor: &mut Cursor<'_>) -> Result<Option<DumpFontSlant>, DumpError> {
    if cursor.read_bool("optional font slant present")? {
        Ok(Some(read_font_slant(cursor)?))
    } else {
        Ok(None)
    }
}

const UNDERLINE_LINE: u8 = 0;
const UNDERLINE_WAVE: u8 = 1;
const UNDERLINE_DOT: u8 = 2;
const UNDERLINE_DASH: u8 = 3;
const UNDERLINE_DOUBLE_LINE: u8 = 4;

fn write_underline_style(out: &mut Vec<u8>, style: &DumpUnderlineStyle) {
    write_u8(
        out,
        match style {
            DumpUnderlineStyle::Line => UNDERLINE_LINE,
            DumpUnderlineStyle::Wave => UNDERLINE_WAVE,
            DumpUnderlineStyle::Dot => UNDERLINE_DOT,
            DumpUnderlineStyle::Dash => UNDERLINE_DASH,
            DumpUnderlineStyle::DoubleLine => UNDERLINE_DOUBLE_LINE,
        },
    );
}

fn read_underline_style(cursor: &mut Cursor<'_>) -> Result<DumpUnderlineStyle, DumpError> {
    match cursor.read_u8("underline style")? {
        UNDERLINE_LINE => Ok(DumpUnderlineStyle::Line),
        UNDERLINE_WAVE => Ok(DumpUnderlineStyle::Wave),
        UNDERLINE_DOT => Ok(DumpUnderlineStyle::Dot),
        UNDERLINE_DASH => Ok(DumpUnderlineStyle::Dash),
        UNDERLINE_DOUBLE_LINE => Ok(DumpUnderlineStyle::DoubleLine),
        other => Err(DumpError::ImageFormatError(format!(
            "unknown underline style tag {other}"
        ))),
    }
}

fn write_underline(out: &mut Vec<u8>, underline: &DumpUnderline) {
    write_underline_style(out, &underline.style);
    write_opt_color(out, underline.color);
    write_opt_i32(out, underline.position);
}

fn read_underline(cursor: &mut Cursor<'_>) -> Result<DumpUnderline, DumpError> {
    Ok(DumpUnderline {
        style: read_underline_style(cursor)?,
        color: read_opt_color(cursor)?,
        position: read_opt_i32(cursor, "underline position")?,
    })
}

fn write_opt_underline(
    out: &mut Vec<u8>,
    underline: Option<&DumpUnderline>,
) -> Result<(), DumpError> {
    match underline {
        Some(underline) => {
            write_bool(out, true);
            write_underline(out, underline);
        }
        None => write_bool(out, false),
    }
    Ok(())
}

fn read_opt_underline(cursor: &mut Cursor<'_>) -> Result<Option<DumpUnderline>, DumpError> {
    if cursor.read_bool("optional underline present")? {
        Ok(Some(read_underline(cursor)?))
    } else {
        Ok(None)
    }
}

const BOX_FLAT: u8 = 0;
const BOX_RAISED: u8 = 1;
const BOX_PRESSED: u8 = 2;

fn write_box_style(out: &mut Vec<u8>, style: &DumpBoxStyle) {
    write_u8(
        out,
        match style {
            DumpBoxStyle::Flat => BOX_FLAT,
            DumpBoxStyle::Raised => BOX_RAISED,
            DumpBoxStyle::Pressed => BOX_PRESSED,
        },
    );
}

fn read_box_style(cursor: &mut Cursor<'_>) -> Result<DumpBoxStyle, DumpError> {
    match cursor.read_u8("box style")? {
        BOX_FLAT => Ok(DumpBoxStyle::Flat),
        BOX_RAISED => Ok(DumpBoxStyle::Raised),
        BOX_PRESSED => Ok(DumpBoxStyle::Pressed),
        other => Err(DumpError::ImageFormatError(format!(
            "unknown box style tag {other}"
        ))),
    }
}

fn write_box_border(out: &mut Vec<u8>, border: &DumpBoxBorder) {
    write_opt_color(out, border.color);
    write_i32(out, border.width);
    write_box_style(out, &border.style);
}

fn read_box_border(cursor: &mut Cursor<'_>) -> Result<DumpBoxBorder, DumpError> {
    Ok(DumpBoxBorder {
        color: read_opt_color(cursor)?,
        width: read_i32(cursor, "box border width")?,
        style: read_box_style(cursor)?,
    })
}

fn write_opt_box_border(out: &mut Vec<u8>, border: Option<&DumpBoxBorder>) {
    match border {
        Some(border) => {
            write_bool(out, true);
            write_box_border(out, border);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_box_border(cursor: &mut Cursor<'_>) -> Result<Option<DumpBoxBorder>, DumpError> {
    if cursor.read_bool("optional box border present")? {
        Ok(Some(read_box_border(cursor)?))
    } else {
        Ok(None)
    }
}

const HEIGHT_ABSOLUTE: u8 = 0;
const HEIGHT_RELATIVE: u8 = 1;

fn write_face_height(out: &mut Vec<u8>, height: &DumpFaceHeight) {
    match height {
        DumpFaceHeight::Absolute(value) => {
            write_u8(out, HEIGHT_ABSOLUTE);
            write_i32(out, *value);
        }
        DumpFaceHeight::Relative(value) => {
            write_u8(out, HEIGHT_RELATIVE);
            write_f64(out, *value);
        }
    }
}

fn read_face_height(cursor: &mut Cursor<'_>) -> Result<DumpFaceHeight, DumpError> {
    match cursor.read_u8("face height tag")? {
        HEIGHT_ABSOLUTE => Ok(DumpFaceHeight::Absolute(read_i32(
            cursor,
            "absolute face height",
        )?)),
        HEIGHT_RELATIVE => Ok(DumpFaceHeight::Relative(read_f64(
            cursor,
            "relative face height",
        )?)),
        other => Err(DumpError::ImageFormatError(format!(
            "unknown face height tag {other}"
        ))),
    }
}

fn write_opt_face_height(out: &mut Vec<u8>, height: Option<&DumpFaceHeight>) {
    match height {
        Some(height) => {
            write_bool(out, true);
            write_face_height(out, height);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_face_height(cursor: &mut Cursor<'_>) -> Result<Option<DumpFaceHeight>, DumpError> {
    if cursor.read_bool("optional face height present")? {
        Ok(Some(read_face_height(cursor)?))
    } else {
        Ok(None)
    }
}

fn write_sym_vec(out: &mut Vec<u8>, syms: &[DumpSymId]) -> Result<(), DumpError> {
    write_len(out, syms.len(), "face inherit symbol count")?;
    for sym in syms {
        write_u32(out, sym.0);
    }
    Ok(())
}

fn read_sym_vec(cursor: &mut Cursor<'_>) -> Result<Vec<DumpSymId>, DumpError> {
    let len = read_len(cursor, "face inherit symbol count")?;
    let mut syms = Vec::with_capacity(len);
    for _ in 0..len {
        syms.push(DumpSymId(cursor.read_u32("face inherit symbol")?));
    }
    Ok(syms)
}

fn write_string_vec(out: &mut Vec<u8>, values: &[String]) -> Result<(), DumpError> {
    write_len(out, values.len(), "face inherit string count")?;
    for value in values {
        write_string(out, value)?;
    }
    Ok(())
}

fn read_string_vec(cursor: &mut Cursor<'_>) -> Result<Vec<String>, DumpError> {
    let len = read_len(cursor, "face inherit string count")?;
    let mut values = Vec::with_capacity(len);
    for _ in 0..len {
        values.push(read_string(cursor)?);
    }
    Ok(values)
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
    if cursor.read_bool("optional value present")? {
        Ok(Some(cursor.read_value()?))
    } else {
        Ok(None)
    }
}

fn write_opt_bool(out: &mut Vec<u8>, value: Option<bool>) {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_bool(out, value);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_bool(cursor: &mut Cursor<'_>) -> Result<Option<bool>, DumpError> {
    if cursor.read_bool("optional bool present")? {
        Ok(Some(cursor.read_bool("optional bool value")?))
    } else {
        Ok(None)
    }
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
    if cursor.read_bool("optional string present")? {
        Ok(Some(read_string(cursor)?))
    } else {
        Ok(None)
    }
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

fn read_opt_u16(cursor: &mut Cursor<'_>, what: &str) -> Result<Option<u16>, DumpError> {
    if cursor.read_bool("optional u16 present")? {
        Ok(Some(read_u16(cursor, what)?))
    } else {
        Ok(None)
    }
}

fn write_opt_i32(out: &mut Vec<u8>, value: Option<i32>) {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_i32(out, value);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_i32(cursor: &mut Cursor<'_>, what: &str) -> Result<Option<i32>, DumpError> {
    if cursor.read_bool("optional i32 present")? {
        Ok(Some(read_i32(cursor, what)?))
    } else {
        Ok(None)
    }
}

fn write_string(out: &mut Vec<u8>, value: &str) -> Result<(), DumpError> {
    write_len(out, value.len(), "string byte length")?;
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn read_string(cursor: &mut Cursor<'_>) -> Result<String, DumpError> {
    let len = read_len(cursor, "string byte length")?;
    let mut bytes = Vec::with_capacity(len);
    for _ in 0..len {
        bytes.push(cursor.read_u8("string byte")?);
    }
    String::from_utf8(bytes)
        .map_err(|err| DumpError::ImageFormatError(format!("invalid UTF-8 string: {err}")))
}

fn write_len(out: &mut Vec<u8>, len: usize, what: &str) -> Result<(), DumpError> {
    write_u64(out, count_u64(len, what)?);
    Ok(())
}

fn read_len(cursor: &mut Cursor<'_>, what: &str) -> Result<usize, DumpError> {
    to_usize(cursor.read_u64(what)?, what)
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    write_u32(out, u32::from(value));
}

fn read_u16(cursor: &mut Cursor<'_>, what: &str) -> Result<u16, DumpError> {
    let value = cursor.read_u32(what)?;
    u16::try_from(value)
        .map_err(|_| DumpError::ImageFormatError(format!("{what} overflows u16: {value}")))
}

fn write_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_ne_bytes());
}

fn read_i32(cursor: &mut Cursor<'_>, what: &str) -> Result<i32, DumpError> {
    let value = cursor.read_u32(what)?;
    Ok(i32::from_ne_bytes(value.to_ne_bytes()))
}

fn write_f64(out: &mut Vec<u8>, value: f64) {
    out.extend_from_slice(&value.to_ne_bytes());
}

fn read_f64(cursor: &mut Cursor<'_>, what: &str) -> Result<f64, DumpError> {
    Ok(f64::from_ne_bytes(cursor.read_u64(what)?.to_ne_bytes()))
}

fn count_u64(count: usize, what: &str) -> Result<u64, DumpError> {
    u64::try_from(count).map_err(|_| DumpError::SerializationError(format!("{what} overflows u64")))
}

fn to_usize(count: u64, what: &str) -> Result<usize, DumpError> {
    usize::try_from(count)
        .map_err(|_| DumpError::ImageFormatError(format!("{what} overflows usize")))
}

pub(crate) fn empty_face_table() -> DumpFaceTable {
    DumpFaceTable {
        face_ids: Vec::new(),
        faces: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::DumpHeapRef;
    use super::*;

    #[test]
    fn face_section_round_trips_face_table() {
        let table = DumpFaceTable {
            face_ids: vec![(
                DumpSymId(1),
                DumpFace {
                    foreground: Some(DumpColor {
                        r: 1,
                        g: 2,
                        b: 3,
                        a: 4,
                    }),
                    background: None,
                    family_value: Some(DumpValue::Str(DumpHeapRef { index: 5 })),
                    family: Some("legacy-family".into()),
                    foundry_value: Some(DumpValue::Symbol(DumpSymId(6))),
                    foundry: None,
                    height: Some(DumpFaceHeight::Relative(1.25)),
                    weight: Some(700),
                    slant: Some(DumpFontSlant::Italic),
                    underline_disabled: false,
                    underline: Some(DumpUnderline {
                        style: DumpUnderlineStyle::Wave,
                        color: Some(DumpColor {
                            r: 10,
                            g: 11,
                            b: 12,
                            a: 13,
                        }),
                        position: Some(2),
                    }),
                    overline: Some(true),
                    strike_through: Some(false),
                    box_disabled: false,
                    box_border: Some(DumpBoxBorder {
                        color: None,
                        width: -1,
                        style: DumpBoxStyle::Raised,
                    }),
                    inverse_video: Some(false),
                    stipple_value: Some(DumpValue::Nil),
                    stipple: Some("legacy-stipple".into()),
                    extend: Some(true),
                    inherit_syms: vec![DumpSymId(7), DumpSymId(8)],
                    inherit: vec!["legacy-inherit".into()],
                    overstrike: true,
                    doc_value: Some(DumpValue::True),
                    doc: Some("doc".into()),
                },
            )],
            faces: vec![(
                "legacy-face".into(),
                DumpFace {
                    foreground: None,
                    background: Some(DumpColor {
                        r: 20,
                        g: 21,
                        b: 22,
                        a: 23,
                    }),
                    family_value: None,
                    family: None,
                    foundry_value: None,
                    foundry: Some("foundry".into()),
                    height: Some(DumpFaceHeight::Absolute(120)),
                    weight: None,
                    slant: Some(DumpFontSlant::ReverseOblique),
                    underline_disabled: true,
                    underline: None,
                    overline: None,
                    strike_through: None,
                    box_disabled: true,
                    box_border: None,
                    inverse_video: None,
                    stipple_value: None,
                    stipple: None,
                    extend: None,
                    inherit_syms: Vec::new(),
                    inherit: Vec::new(),
                    overstrike: false,
                    doc_value: None,
                    doc: None,
                },
            )],
        };

        let bytes = face_table_section_bytes(&table).expect("encode face table");
        let decoded = load_face_table_section(&bytes).expect("decode face table");

        assert_eq!(format!("{decoded:?}"), format!("{table:?}"));
    }

    #[test]
    fn face_section_rejects_bad_magic() {
        let mut bytes = face_table_section_bytes(&empty_face_table()).expect("encode face table");
        bytes[0] ^= 1;
        let err = load_face_table_section(&bytes).expect_err("bad magic should fail");
        assert!(matches!(err, DumpError::ImageFormatError(_)));
    }
}
