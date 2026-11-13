//! Conditional absence-only regression; explicit values are separate controls.
//! Pure OsStr input does not mutate environment or process policy caches.
use super::parse;

#[test]
fn promoted_numeric_padding_defaults_only_when_absent() {
    assert_eq!(parse(None), true);
}
