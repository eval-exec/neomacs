use super::*;

// ---------------------------------------------------------------
// Constructor & Default tests
// ---------------------------------------------------------------

#[test]
fn test_new_default_budget() {
    let budget = MediaBudget::new();
    assert_eq!(budget.max_limit(), 256 * 1024 * 1024);
    assert_eq!(budget.current_usage(), 0);
    assert!(!budget.is_over_budget());
}

#[test]
fn test_with_max_bytes() {
    let budget = MediaBudget::with_max_bytes(1024);
    assert_eq!(budget.max_limit(), 1024);
    assert_eq!(budget.current_usage(), 0);
}

#[test]
fn test_default_trait() {
    let budget = MediaBudget::default();
    assert_eq!(budget.max_limit(), 256 * 1024 * 1024);
    assert_eq!(budget.current_usage(), 0);
}

#[test]
fn test_zero_budget_limit() {
    let budget = MediaBudget::with_max_bytes(0);
    assert_eq!(budget.max_limit(), 0);
    assert_eq!(budget.current_usage(), 0);
    assert!(!budget.is_over_budget());
}

// ---------------------------------------------------------------
// MediaType ordering tests
// ---------------------------------------------------------------

#[test]
fn test_media_type_ordering() {
    // Image < Video < WebKit (lowest priority evicted first)
    assert!(MediaType::Image < MediaType::Video);
    assert!(MediaType::Video < MediaType::WebKit);
    assert!(MediaType::Image < MediaType::WebKit);
}

#[test]
fn test_media_type_equality() {
    assert_eq!(MediaType::Image, MediaType::Image);
    assert_eq!(MediaType::Video, MediaType::Video);
    assert_eq!(MediaType::WebKit, MediaType::WebKit);
    assert_ne!(MediaType::Image, MediaType::Video);
}

#[test]
fn test_media_type_clone_copy() {
    let t = MediaType::Image;
    let t2 = t; // Copy
    let t3 = t.clone(); // Clone
    assert_eq!(t, t2);
    assert_eq!(t, t3);
}

// ---------------------------------------------------------------
// Register tests
// ---------------------------------------------------------------

#[test]
fn test_register_single_item() {
    let mut budget = MediaBudget::with_max_bytes(1000);
    budget.register(MediaType::Image, 1, 500);
    assert_eq!(budget.current_usage(), 500);
}

#[test]
fn test_register_multiple_items_same_type() {
    let mut budget = MediaBudget::with_max_bytes(10000);
    budget.register(MediaType::Image, 1, 100);
    budget.register(MediaType::Image, 2, 200);
    budget.register(MediaType::Image, 3, 300);
    assert_eq!(budget.current_usage(), 600);
}

#[test]
fn test_register_multiple_items_different_types() {
    let mut budget = MediaBudget::with_max_bytes(10000);
    budget.register(MediaType::Image, 1, 100);
    budget.register(MediaType::Video, 2, 200);
    budget.register(MediaType::WebKit, 3, 300);
    assert_eq!(budget.current_usage(), 600);
}

#[test]
fn test_register_zero_size() {
    let mut budget = MediaBudget::with_max_bytes(1000);
    budget.register(MediaType::Image, 1, 0);
    assert_eq!(budget.current_usage(), 0);
    assert!(!budget.is_over_budget());
}

#[test]
fn test_register_exceeds_budget() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 200);
    assert_eq!(budget.current_usage(), 200);
    assert!(budget.is_over_budget());
}

#[test]
fn test_register_exact_budget() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 100);
    assert_eq!(budget.current_usage(), 100);
    assert!(!budget.is_over_budget());
}

#[test]
fn test_register_increments_access_counter() {
    let mut budget = MediaBudget::with_max_bytes(10000);
    budget.register(MediaType::Image, 1, 10);
    budget.register(MediaType::Image, 2, 10);
    budget.register(MediaType::Image, 3, 10);
    // Access counter should be 3 after 3 registrations
    assert_eq!(budget.access_counter, 3);
}

// ---------------------------------------------------------------
// Unregister tests
// ---------------------------------------------------------------

#[test]
fn test_register_unregister() {
    let mut budget = MediaBudget::new();

    budget.register(MediaType::Image, 1, 1000);
    assert_eq!(budget.current_usage(), 1000);

    budget.unregister(MediaType::Image, 1);
    assert_eq!(budget.current_usage(), 0);
}

#[test]
fn test_unregister_nonexistent() {
    let mut budget = MediaBudget::with_max_bytes(1000);
    budget.register(MediaType::Image, 1, 100);
    budget.unregister(MediaType::Image, 99); // doesn't exist
    assert_eq!(budget.current_usage(), 100); // unchanged
}

#[test]
fn test_unregister_wrong_media_type() {
    let mut budget = MediaBudget::with_max_bytes(1000);
    budget.register(MediaType::Image, 1, 100);
    budget.unregister(MediaType::Video, 1); // wrong type
    assert_eq!(budget.current_usage(), 100); // unchanged
}

#[test]
fn test_double_unregister() {
    let mut budget = MediaBudget::with_max_bytes(1000);
    budget.register(MediaType::Image, 1, 100);
    budget.unregister(MediaType::Image, 1);
    assert_eq!(budget.current_usage(), 0);
    budget.unregister(MediaType::Image, 1); // second unregister
    assert_eq!(budget.current_usage(), 0); // still 0, no underflow
}

#[test]
fn test_unregister_from_empty_budget() {
    let mut budget = MediaBudget::with_max_bytes(1000);
    budget.unregister(MediaType::Image, 1);
    assert_eq!(budget.current_usage(), 0);
}

#[test]
fn test_unregister_partial() {
    let mut budget = MediaBudget::with_max_bytes(10000);
    budget.register(MediaType::Image, 1, 100);
    budget.register(MediaType::Video, 2, 200);
    budget.register(MediaType::WebKit, 3, 300);
    assert_eq!(budget.current_usage(), 600);

    budget.unregister(MediaType::Video, 2);
    assert_eq!(budget.current_usage(), 400);

    // Remaining entries still tracked
    budget.unregister(MediaType::Image, 1);
    assert_eq!(budget.current_usage(), 300);

    budget.unregister(MediaType::WebKit, 3);
    assert_eq!(budget.current_usage(), 0);
}

#[test]
fn test_unregister_saturating_sub() {
    // current_memory uses saturating_sub, so even if accounting
    // were somehow wrong, it shouldn't underflow
    let mut budget = MediaBudget::with_max_bytes(1000);
    budget.register(MediaType::Image, 1, 100);
    // Manually verify saturating behavior: unregister the item
    budget.unregister(MediaType::Image, 1);
    assert_eq!(budget.current_usage(), 0);
}

// ---------------------------------------------------------------
// Touch tests
// ---------------------------------------------------------------

#[test]
fn test_touch_updates_access() {
    let mut budget = MediaBudget::with_max_bytes(100);

    budget.register(MediaType::Image, 1, 30);
    budget.register(MediaType::Image, 2, 30);

    // Touch image 1 to make it more recent
    budget.touch(MediaType::Image, 1);

    // Candidates should evict image 2 first (older)
    let candidates = budget.get_eviction_candidates(50);
    assert_eq!(candidates[0], (MediaType::Image, 2));
}

#[test]
fn test_touch_nonexistent_item() {
    let mut budget = MediaBudget::with_max_bytes(1000);
    budget.register(MediaType::Image, 1, 100);
    let counter_before = budget.access_counter;
    budget.touch(MediaType::Image, 99); // doesn't exist
    // access_counter should NOT change for nonexistent items
    assert_eq!(budget.access_counter, counter_before);
    assert_eq!(budget.current_usage(), 100); // memory unchanged
}

#[test]
fn test_touch_preserves_memory() {
    let mut budget = MediaBudget::with_max_bytes(1000);
    budget.register(MediaType::Image, 1, 100);
    budget.register(MediaType::Video, 2, 200);
    assert_eq!(budget.current_usage(), 300);

    budget.touch(MediaType::Image, 1);
    assert_eq!(budget.current_usage(), 300); // unchanged
}

#[test]
fn test_touch_wrong_media_type() {
    let mut budget = MediaBudget::with_max_bytes(1000);
    budget.register(MediaType::Image, 1, 100);
    let counter_before = budget.access_counter;
    budget.touch(MediaType::Video, 1); // wrong type for id 1
    assert_eq!(budget.access_counter, counter_before);
}

#[test]
fn test_touch_reverses_eviction_order_within_same_type() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 25);
    budget.register(MediaType::Image, 2, 25);
    budget.register(MediaType::Image, 3, 25);

    // Initially order: 1, 2, 3 (by access time)
    // Touch 1 → makes it most recent: order becomes 2, 3, 1
    budget.touch(MediaType::Image, 1);

    let candidates = budget.get_eviction_candidates(30);
    // Should evict 2 first (oldest), then 3
    assert!(candidates.len() >= 1);
    assert_eq!(candidates[0], (MediaType::Image, 2));
    if candidates.len() >= 2 {
        assert_eq!(candidates[1], (MediaType::Image, 3));
    }
}

#[test]
fn test_touch_increments_access_counter() {
    let mut budget = MediaBudget::with_max_bytes(1000);
    budget.register(MediaType::Image, 1, 100); // counter = 1
    assert_eq!(budget.access_counter, 1);

    budget.touch(MediaType::Image, 1); // counter = 2
    assert_eq!(budget.access_counter, 2);

    budget.touch(MediaType::Image, 1); // counter = 3
    assert_eq!(budget.access_counter, 3);
}

// ---------------------------------------------------------------
// Eviction candidate tests
// ---------------------------------------------------------------

#[test]
fn test_budget_eviction_order() {
    let mut budget = MediaBudget::with_max_bytes(100);

    budget.register(MediaType::WebKit, 1, 30);
    budget.register(MediaType::Image, 2, 20);
    budget.register(MediaType::Video, 3, 25);
    budget.register(MediaType::Image, 4, 15);

    // Total = 90 bytes, need 50 more, target = 140
    // Need to free: 140 - 100 = 40 bytes
    // BTreeMap order: Image(2)=20, Image(4)=15, Video(3)=25, WebKit(1)=30
    // Need 3 items to free >= 40 bytes: 20 + 15 + 25 = 60
    let candidates = budget.get_eviction_candidates(50);

    assert_eq!(candidates.len(), 3);
    // Images evicted first (lowest priority), then Video
    assert_eq!(candidates[0], (MediaType::Image, 2));
    assert_eq!(candidates[1], (MediaType::Image, 4));
    assert_eq!(candidates[2], (MediaType::Video, 3));
}

#[test]
fn test_eviction_no_candidates_when_under_budget() {
    let mut budget = MediaBudget::with_max_bytes(1000);
    budget.register(MediaType::Image, 1, 100);
    budget.register(MediaType::Video, 2, 200);

    let candidates = budget.get_eviction_candidates(100);
    assert!(candidates.is_empty());
}

#[test]
fn test_eviction_no_candidates_for_zero_size() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 50);

    let candidates = budget.get_eviction_candidates(0);
    assert!(candidates.is_empty());
}

#[test]
fn test_eviction_exact_budget_boundary() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 50);

    // current=50, new_size=50, target=100, max=100 → no eviction needed
    let candidates = budget.get_eviction_candidates(50);
    assert!(candidates.is_empty());
}

#[test]
fn test_eviction_one_byte_over() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 50);

    // current=50, new_size=51, target=101, max=100 → need to free 1 byte
    let candidates = budget.get_eviction_candidates(51);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0], (MediaType::Image, 1));
}

#[test]
fn test_eviction_priority_image_before_video() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Video, 1, 60);
    budget.register(MediaType::Image, 2, 30);

    // current=90, new=20, target=110, need to free 10
    // BTreeMap order: Image(2, access=2) first → 30 bytes freed
    let candidates = budget.get_eviction_candidates(20);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0], (MediaType::Image, 2));
}

#[test]
fn test_eviction_priority_video_before_webkit() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::WebKit, 1, 60);
    budget.register(MediaType::Video, 2, 30);

    // current=90, new=20, target=110, need to free 10
    // BTreeMap order: Video(2) before WebKit(1)
    let candidates = budget.get_eviction_candidates(20);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0], (MediaType::Video, 2));
}

#[test]
fn test_eviction_lru_within_same_type() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 30); // access=1
    budget.register(MediaType::Image, 2, 30); // access=2
    budget.register(MediaType::Image, 3, 30); // access=3

    // current=90, new=20, target=110, need to free 10
    // Within Image type, order by access time: 1(oldest) first
    let candidates = budget.get_eviction_candidates(20);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0], (MediaType::Image, 1));
}

#[test]
fn test_eviction_needs_all_entries() {
    let mut budget = MediaBudget::with_max_bytes(50);
    budget.register(MediaType::Image, 1, 20);
    budget.register(MediaType::Video, 2, 20);
    budget.register(MediaType::WebKit, 3, 20);

    // current=60, new=100, target=160, need to free=110
    // All entries sum to 60 bytes, which is less than 110
    // but we still get all 3 as candidates
    let candidates = budget.get_eviction_candidates(100);
    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[0], (MediaType::Image, 1));
    assert_eq!(candidates[1], (MediaType::Video, 2));
    assert_eq!(candidates[2], (MediaType::WebKit, 3));
}

#[test]
fn test_eviction_empty_budget() {
    let budget = MediaBudget::with_max_bytes(100);
    let candidates = budget.get_eviction_candidates(200);
    // No entries to evict, returns empty even though over budget
    assert!(candidates.is_empty());
}

#[test]
fn test_eviction_with_zero_size_entries() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 0); // zero size
    budget.register(MediaType::Image, 2, 80);
    budget.register(MediaType::Video, 3, 20);

    // current=100, new=10, target=110, need to free=10
    // First candidate is Image(1, access=1) with 0 bytes → not enough
    // Then Image(2, access=2) with 80 bytes → enough
    let candidates = budget.get_eviction_candidates(10);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0], (MediaType::Image, 1));
    assert_eq!(candidates[1], (MediaType::Image, 2));
}

#[test]
fn test_eviction_zero_budget_limit() {
    let mut budget = MediaBudget::with_max_bytes(0);
    budget.register(MediaType::Image, 1, 10);

    // current=10, new=5, target=15, need to free=15
    let candidates = budget.get_eviction_candidates(5);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0], (MediaType::Image, 1));
}

#[test]
fn test_eviction_after_unregister() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 50);
    budget.register(MediaType::Image, 2, 50);

    // current=100, at budget
    budget.unregister(MediaType::Image, 1);
    // current=50

    // Adding 50 more → target=100, exactly at budget → no eviction
    let candidates = budget.get_eviction_candidates(50);
    assert!(candidates.is_empty());

    // Adding 51 more → target=101, need to free 1
    let candidates = budget.get_eviction_candidates(51);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0], (MediaType::Image, 2));
}

#[test]
fn test_eviction_after_touch_reorders() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 30);
    budget.register(MediaType::Image, 2, 30);
    budget.register(MediaType::Image, 3, 30);

    // Touch item 1 → it becomes most recent
    budget.touch(MediaType::Image, 1);

    // current=90, new=20, target=110, need to free=10
    // LRU order: 2(access=2), 3(access=3), 1(access=4 after touch)
    let candidates = budget.get_eviction_candidates(20);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0], (MediaType::Image, 2));
}

// ---------------------------------------------------------------
// is_over_budget tests
// ---------------------------------------------------------------

#[test]
fn test_not_over_budget_when_empty() {
    let budget = MediaBudget::with_max_bytes(100);
    assert!(!budget.is_over_budget());
}

#[test]
fn test_not_over_budget_when_under() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 50);
    assert!(!budget.is_over_budget());
}

#[test]
fn test_not_over_budget_when_exactly_at_limit() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 100);
    assert!(!budget.is_over_budget());
}

#[test]
fn test_over_budget_when_exceeded() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 101);
    assert!(budget.is_over_budget());
}

#[test]
fn test_over_budget_from_multiple_registrations() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 60);
    assert!(!budget.is_over_budget());
    budget.register(MediaType::Video, 2, 41);
    assert!(budget.is_over_budget());
}

#[test]
fn test_back_under_budget_after_unregister() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 60);
    budget.register(MediaType::Video, 2, 60);
    assert!(budget.is_over_budget());

    budget.unregister(MediaType::Image, 1);
    assert!(!budget.is_over_budget());
}

// ---------------------------------------------------------------
// current_usage and max_limit accessor tests
// ---------------------------------------------------------------

#[test]
fn test_current_usage_tracks_correctly() {
    let mut budget = MediaBudget::with_max_bytes(10000);
    assert_eq!(budget.current_usage(), 0);

    budget.register(MediaType::Image, 1, 100);
    assert_eq!(budget.current_usage(), 100);

    budget.register(MediaType::Video, 2, 250);
    assert_eq!(budget.current_usage(), 350);

    budget.unregister(MediaType::Image, 1);
    assert_eq!(budget.current_usage(), 250);

    budget.unregister(MediaType::Video, 2);
    assert_eq!(budget.current_usage(), 0);
}

#[test]
fn test_max_limit_unchanged_by_operations() {
    let mut budget = MediaBudget::with_max_bytes(500);
    assert_eq!(budget.max_limit(), 500);

    budget.register(MediaType::Image, 1, 1000);
    assert_eq!(budget.max_limit(), 500); // unchanged

    budget.unregister(MediaType::Image, 1);
    assert_eq!(budget.max_limit(), 500); // unchanged
}

// ---------------------------------------------------------------
// Large value / stress tests
// ---------------------------------------------------------------

#[test]
fn test_large_memory_values() {
    let one_gb = 1024 * 1024 * 1024;
    let mut budget = MediaBudget::with_max_bytes(4 * one_gb);
    budget.register(MediaType::WebKit, 1, one_gb);
    budget.register(MediaType::WebKit, 2, one_gb);
    budget.register(MediaType::WebKit, 3, one_gb);
    assert_eq!(budget.current_usage(), 3 * one_gb);
    assert!(!budget.is_over_budget());

    budget.register(MediaType::WebKit, 4, one_gb);
    assert_eq!(budget.current_usage(), 4 * one_gb);
    assert!(!budget.is_over_budget());

    budget.register(MediaType::WebKit, 5, 1);
    assert!(budget.is_over_budget());
}

#[test]
fn test_many_small_entries() {
    let mut budget = MediaBudget::with_max_bytes(5000);
    for i in 0..100 {
        budget.register(MediaType::Image, i, 10);
    }
    assert_eq!(budget.current_usage(), 1000);
    assert!(!budget.is_over_budget());

    // Eviction should return oldest entries first
    let candidates = budget.get_eviction_candidates(4500);
    // need to free: 1000 + 4500 - 5000 = 500 bytes = 50 entries of 10 bytes
    assert_eq!(candidates.len(), 50);
    // First candidate should be the oldest (id=0)
    assert_eq!(candidates[0], (MediaType::Image, 0));
    assert_eq!(candidates[49], (MediaType::Image, 49));
}

// ---------------------------------------------------------------
// Same id, different media type tests
// ---------------------------------------------------------------

#[test]
fn test_same_id_different_media_types() {
    let mut budget = MediaBudget::with_max_bytes(10000);
    budget.register(MediaType::Image, 1, 100);
    budget.register(MediaType::Video, 1, 200);
    budget.register(MediaType::WebKit, 1, 300);
    assert_eq!(budget.current_usage(), 600);

    // Unregister only the Image with id=1
    budget.unregister(MediaType::Image, 1);
    assert_eq!(budget.current_usage(), 500);

    // Unregister only the Video with id=1
    budget.unregister(MediaType::Video, 1);
    assert_eq!(budget.current_usage(), 300);

    // Unregister only the WebKit with id=1
    budget.unregister(MediaType::WebKit, 1);
    assert_eq!(budget.current_usage(), 0);
}

// ---------------------------------------------------------------
// Combined workflow / integration-style tests
// ---------------------------------------------------------------

#[test]
fn test_register_touch_evict_unregister_workflow() {
    let mut budget = MediaBudget::with_max_bytes(200);

    // Register items of various types
    budget.register(MediaType::Image, 1, 50); // access=1
    budget.register(MediaType::Image, 2, 50); // access=2
    budget.register(MediaType::Video, 3, 50); // access=3
    budget.register(MediaType::WebKit, 4, 50); // access=4
    assert_eq!(budget.current_usage(), 200);
    assert!(!budget.is_over_budget());

    // Touch image 1 to protect it
    budget.touch(MediaType::Image, 1); // access=5

    // Need to add 50 more bytes: target=250, need_to_free=50
    // BTreeMap order: Image(2, access=2), Image(1, access=5), Video(3, access=3), WebKit(4, access=4)
    // Evict Image(2) first → 50 bytes freed, enough
    let candidates = budget.get_eviction_candidates(50);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0], (MediaType::Image, 2));

    // Perform the eviction
    budget.unregister(MediaType::Image, 2);
    assert_eq!(budget.current_usage(), 150);

    // Now register the new item
    budget.register(MediaType::WebKit, 5, 50);
    assert_eq!(budget.current_usage(), 200);
    assert!(!budget.is_over_budget());
}

#[test]
fn test_eviction_cross_type_priority() {
    let mut budget = MediaBudget::with_max_bytes(100);

    // Register in reverse priority order
    budget.register(MediaType::WebKit, 1, 30); // access=1
    budget.register(MediaType::Video, 2, 30); // access=2
    budget.register(MediaType::Image, 3, 30); // access=3

    // current=90, new=30, target=120, need=20
    // BTreeMap order by key (MediaType, access, id):
    //   (Image, 3, 3)=30 → first (Image has lowest MediaType)
    //   (Video, 2, 2)=30 → second
    //   (WebKit, 1, 1)=30 → third
    // Evict Image(3)=30 bytes, enough
    let candidates = budget.get_eviction_candidates(30);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0], (MediaType::Image, 3));
}

#[test]
fn test_eviction_does_not_mutate_budget() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 80);

    let candidates = budget.get_eviction_candidates(50);
    assert_eq!(candidates.len(), 1);

    // get_eviction_candidates is &self, so budget should be unchanged
    assert_eq!(budget.current_usage(), 80);
    assert_eq!(budget.entries.len(), 1);
}

#[test]
fn test_register_after_full_drain() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Image, 1, 50);
    budget.register(MediaType::Video, 2, 50);
    assert_eq!(budget.current_usage(), 100);

    // Drain everything
    budget.unregister(MediaType::Image, 1);
    budget.unregister(MediaType::Video, 2);
    assert_eq!(budget.current_usage(), 0);

    // Re-register fresh items
    budget.register(MediaType::WebKit, 3, 100);
    assert_eq!(budget.current_usage(), 100);
    assert!(!budget.is_over_budget());
}

#[test]
fn test_budget_entry_debug() {
    // Verify BudgetEntry derives Debug
    let entry = BudgetEntry {
        media_type: MediaType::Image,
        id: 42,
        size_bytes: 1024,
        last_access: 7,
    };
    let debug_str = format!("{:?}", entry);
    assert!(debug_str.contains("Image"));
    assert!(debug_str.contains("42"));
    assert!(debug_str.contains("1024"));
}

#[test]
fn test_media_type_debug() {
    assert_eq!(format!("{:?}", MediaType::Image), "Image");
    assert_eq!(format!("{:?}", MediaType::Video), "Video");
    assert_eq!(format!("{:?}", MediaType::WebKit), "WebKit");
}

// ---------------------------------------------------------------
// Eviction-driver-shaped tests (mirror the RenderApp SurfaceCreate
// driver in render_thread/asset_commands.rs: the new surface is
// registered first, then candidates are queried with new_size = 0
// and evicted one at a time until back under budget)
// ---------------------------------------------------------------

#[test]
fn test_surface_sorts_after_all_other_media_types() {
    // The driver relies on surfaces being offered last: everything the
    // budget would rather evict (images, video frames, webkit views)
    // precedes them in the candidate list.
    assert!(MediaType::Image < MediaType::Surface);
    assert!(MediaType::Video < MediaType::Surface);
    assert!(MediaType::WebKit < MediaType::Surface);

    let mut budget = MediaBudget::with_max_bytes(50);
    budget.register(MediaType::Surface, 1, 40); // older
    budget.register(MediaType::Image, 2, 40); // newer, but lower type
    // current=80, new_size=0 → need_to_free=30; the Image leads despite
    // being more recently registered.
    let candidates = budget.get_eviction_candidates(0);
    assert_eq!(candidates[0], (MediaType::Image, 2));
}

#[test]
fn test_driver_candidates_lru_first_newest_last() {
    let mut budget = MediaBudget::with_max_bytes(50);
    budget.register(MediaType::Surface, 10, 30); // access=1, oldest
    budget.register(MediaType::Surface, 11, 30); // access=2
    budget.register(MediaType::Surface, 12, 30); // access=3, "just created"
    assert!(budget.is_over_budget());

    // current=90, need_to_free=40 → the LRU-first prefix [10, 11] covers
    // it; the just-created surface (highest last_access) is not offered.
    let candidates = budget.get_eviction_candidates(0);
    assert_eq!(
        candidates,
        vec![(MediaType::Surface, 10), (MediaType::Surface, 11)]
    );
    assert!(!candidates.contains(&(MediaType::Surface, 12)));
}

#[test]
fn test_driver_scenario_evict_requery_until_under_budget() {
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Surface, 1, 40);
    budget.register(MediaType::Surface, 2, 40);
    assert!(!budget.is_over_budget());

    // The create that lands over budget is already registered when the
    // driver runs, so it queries with new_size = 0.
    budget.register(MediaType::Surface, 3, 40);
    assert!(budget.is_over_budget());

    // Shortfall is 120 - 100 = 20; the oldest surface alone covers it.
    let candidates = budget.get_eviction_candidates(0);
    assert_eq!(candidates, vec![(MediaType::Surface, 1)]);

    // Driver evicts the victim and re-checks.
    budget.unregister(MediaType::Surface, 1);
    assert!(!budget.is_over_budget());
    assert_eq!(budget.current_usage(), 80);
    assert!(budget.get_eviction_candidates(0).is_empty());
}

#[test]
fn test_driver_scenario_giant_create_offers_itself_last() {
    // A single surface larger than the whole budget: the driver's
    // "never evict the surface just created" guard is what stops the
    // loop, because the budget itself will offer every entry including
    // the new one.
    let mut budget = MediaBudget::with_max_bytes(100);
    budget.register(MediaType::Surface, 1, 30);
    budget.register(MediaType::Surface, 2, 150); // the giant new create
    assert!(budget.is_over_budget());

    // need_to_free = 80: the old surface's 30 bytes are not enough, so
    // the prefix extends to the just-created giant.
    let candidates = budget.get_eviction_candidates(0);
    assert_eq!(
        candidates,
        vec![(MediaType::Surface, 1), (MediaType::Surface, 2)]
    );

    // Driver skips id 2 (just created), evicts id 1, re-checks: still
    // over budget, and the only remaining candidate is the new surface
    // itself → the driver breaks.
    budget.unregister(MediaType::Surface, 1);
    assert!(budget.is_over_budget());
    assert_eq!(
        budget.get_eviction_candidates(0),
        vec![(MediaType::Surface, 2)]
    );
}
