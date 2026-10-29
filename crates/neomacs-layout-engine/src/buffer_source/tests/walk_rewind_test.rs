use super::*;

#[test]
fn word_rewind_rechecks_invisibility_but_character_retry_retains_the_stop() {
    let mut checkpoint = InvisibleTextScanCheckpoint::new(10);
    // Simulate scanning across an invisible span before a later overflow
    // chooses an earlier word boundary. Replaying that span must hide it again.
    checkpoint.record_next_visible(46);
    assert!(!checkpoint.should_check(40));
    BufferSourceRewind::WordWrap(DisplaySourceTextPosition::new(30, 30))
        .restore_invisible_checkpoint(&mut checkpoint);
    assert!(checkpoint.should_check(30));
    assert!(checkpoint.should_check(40));
    checkpoint.record_next_visible(46);
    BufferSourceRewind::CharacterWrap(DisplaySourceTextPosition::new(40, 40))
        .restore_invisible_checkpoint(&mut checkpoint);
    assert!(!checkpoint.should_check(40));
    assert!(checkpoint.should_check(46));
}
