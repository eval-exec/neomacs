//! U2.8: `skip-syntax-forward`/`-backward` parse their class string from its
//! bytes (GNU `skip_syntaxes`) instead of decoding it first; both parses
//! must name the same classes, and the builtins must answer alike either
//! way.

use super::*;

#[test]
fn byte_and_decoded_class_parses_agree() {
    let unibyte = neovm_value::heap_types::LispString::from_unibyte(vec![b'w', 0xA0, b'_', 0xFF]);
    let multibyte = neovm_value::heap_types::LispString::from_utf8("w\u{e9}_\u{6f22}^\"");
    let mut strings: Vec<Vec<u8>> = [
        "",
        "^",
        "w",
        "^w",
        "w_",
        "^w_",
        " -",
        "^ -.",
        "()'\"$\\/<>@!|",
        "^^w",
        "xyz",
        "w^",
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .collect();
    strings.push(unibyte.as_bytes().to_vec());
    strings.push(multibyte.as_bytes().to_vec());
    for bytes in strings {
        let decoded = neovm_value::emacs_char::to_utf8_lossy(&bytes);
        assert_eq!(
            SkipSyntaxClasses::parse_spec_bytes(&bytes),
            SkipSyntaxClasses::parse_str(&decoded),
            "{bytes:?}"
        );
    }
}
