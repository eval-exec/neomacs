use super::*;

#[test]
fn fontification_merge_preserves_callback_admission_after_a_marked_position() {
    use RedisplayFontificationOutcome::{CallbacksUnchanged, Fontified, Unchanged};
    assert_eq!(Unchanged.merge(CallbacksUnchanged), CallbacksUnchanged);
    assert_eq!(CallbacksUnchanged.merge(Unchanged), CallbacksUnchanged);
    assert_eq!(CallbacksUnchanged.merge(Fontified), Fontified);
    assert_eq!(Fontified.merge(CallbacksUnchanged), Fontified);
}
