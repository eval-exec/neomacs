//! Safe decoder for `FILE_NOTIFY_INFORMATION` records.
//!
//! The kernel buffer contains unaligned, variable-sized records. Decode its
//! integer fields from byte slices so no packed-struct references or pointer
//! arithmetic escape into the platform adapter.

use super::W32Action;
use std::ffi::OsString;
use std::path::PathBuf;

const HEADER_LEN: usize = 12;

pub(super) fn decode(buffer: &[u8]) -> Result<Vec<(W32Action, PathBuf)>, String> {
    let mut decoded = Vec::new();
    let mut offset = 0;
    loop {
        let header = buffer
            .get(offset..offset + HEADER_LEN)
            .ok_or_else(|| "truncated FILE_NOTIFY_INFORMATION header".to_owned())?;
        let next = read_u32(&header[0..4]) as usize;
        let action = match read_u32(&header[4..8]) {
            1 => W32Action::Added,
            2 => W32Action::Removed,
            3 => W32Action::Modified,
            4 => W32Action::RenamedFrom,
            5 => W32Action::RenamedTo,
            action => return Err(format!("unknown Windows file-notify action {action}")),
        };
        let name_len = read_u32(&header[8..12]) as usize;
        if !name_len.is_multiple_of(2) {
            return Err("odd FILE_NOTIFY_INFORMATION name length".to_owned());
        }
        let name_bytes = buffer
            .get(offset + HEADER_LEN..offset + HEADER_LEN + name_len)
            .ok_or_else(|| "truncated FILE_NOTIFY_INFORMATION name".to_owned())?;
        let utf16 = name_bytes
            .chunks_exact(2)
            .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
            .collect::<Vec<_>>();
        decoded.push((action, PathBuf::from(os_string_from_wide(&utf16))));

        if next == 0 {
            return Ok(decoded);
        }
        let record_len = HEADER_LEN
            .checked_add(name_len)
            .ok_or_else(|| "FILE_NOTIFY_INFORMATION record length overflow".to_owned())?;
        let next_offset = offset
            .checked_add(next)
            .ok_or_else(|| "FILE_NOTIFY_INFORMATION next offset overflow".to_owned())?;
        if !next.is_multiple_of(4) || next < record_len || next_offset > buffer.len() {
            return Err("invalid FILE_NOTIFY_INFORMATION next offset".to_owned());
        }
        offset = next_offset;
    }
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("caller passes four bytes"))
}

#[cfg(target_os = "windows")]
fn os_string_from_wide(wide: &[u16]) -> OsString {
    use std::os::windows::ffi::OsStringExt;
    OsString::from_wide(wide)
}

#[cfg(not(target_os = "windows"))]
fn os_string_from_wide(wide: &[u16]) -> OsString {
    OsString::from(String::from_utf16_lossy(wide))
}
