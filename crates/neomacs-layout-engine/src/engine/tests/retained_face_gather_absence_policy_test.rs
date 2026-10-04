//! Pure absence-only policy regression; no environment or process cache writes.
use super::parse;

#[test]
fn retained_face_gather_defaults_on_only_when_absent() {
    assert!(parse(None));
}
