//! Shared Lisp string escaping helpers.

/// Format a Rust string as an Emacs Lisp string literal.
pub(crate) fn format_lisp_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{0b}' => out.push_str("\\v"),
            '\u{0c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            '\u{07}' => out.push_str("\\a"),
            '\u{1b}' => out.push_str("\\e"),
            // NeoVM internal sentinel for Emacs raw-byte chars 0x80..0xFF.
            c if (c as u32) >= 0xE080 && (c as u32) <= 0xE0FF => {
                let byte = (c as u32) - 0xE000;
                out.push('\\');
                out.push_str(&format!("{:03o}", byte));
            }
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push('\\');
                out.push_str(&format!("{:03o}", c as u32));
            }
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_control_chars() {
        assert_eq!(format_lisp_string("\n\t"), "\"\\n\\t\"");
        assert_eq!(format_lisp_string("\u{7f}"), "\"\\177\"");
    }

    #[test]
    fn keeps_non_bmp_visible() {
        assert_eq!(format_lisp_string("\u{10ffff}"), "\"\u{10ffff}\"");
    }

    #[test]
    fn escapes_raw_byte_sentinel_as_octal() {
        let raw_377 = char::from_u32(0xE0FF).expect("valid sentinel scalar");
        assert_eq!(
            format_lisp_string(&raw_377.to_string()),
            "\"\\377\""
        );
    }
}
