use super::*;
use std::hash::{DefaultHasher, Hash, Hasher};

fn hash(value: &impl Hash) -> u64 {
    let mut state = DefaultHasher::new();
    value.hash(&mut state);
    state.finish()
}

/// A spelling is the cache-key identity of the Lisp string it was taken
/// from, so it hashes as `neovm_value`'s `LispString` does: the bytes, then
/// whether the string is multibyte. The evaluator-side test
/// `image_source_spellings_preserve_bytes_mode_and_decoder_input` compares a
/// spelling with its string directly; this one pins the shape where the type
/// lives, because this crate cannot name `LispString`.
#[test]
fn image_source_spelling_hashes_as_the_lisp_string_it_was_taken_from() {
    for (bytes, mode, multibyte) in [
        (b"/images/".to_vec(), ImageSpellingMode::Multibyte, true),
        (b"/images/".to_vec(), ImageSpellingMode::Unibyte, false),
        (
            "/images/字-é/".as_bytes().to_vec(),
            ImageSpellingMode::Multibyte,
            true,
        ),
        (vec![0xff, b'/'], ImageSpellingMode::Unibyte, false),
        (Vec::new(), ImageSpellingMode::Multibyte, true),
        (Vec::new(), ImageSpellingMode::Unibyte, false),
    ] {
        let spelling = ImageSourceSpelling::new(bytes.clone(), mode);
        assert_eq!(hash(&spelling), hash(&(bytes.as_slice(), multibyte)));
        assert_eq!(hash(&ImageBaseUri::new(spelling.clone())), hash(&spelling));
    }
    // Equality keeps the mode, as the hash does: the same bytes in the two
    // string modes are two spellings.
    assert_ne!(
        ImageSourceSpelling::new(b"/images/".to_vec(), ImageSpellingMode::Unibyte),
        ImageSourceSpelling::new(b"/images/".to_vec(), ImageSpellingMode::Multibyte),
    );
}
