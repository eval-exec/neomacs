//! Keep the default entry cache dense when the tier spine is disabled.

use super::*;

#[test]
fn tier2_off_cache_entry_stays_two_words() {
    assert_eq!(std::mem::size_of::<CacheEntry>(), 16);
    assert_eq!(std::mem::size_of::<Option<CacheEntry>>(), 16);
}
