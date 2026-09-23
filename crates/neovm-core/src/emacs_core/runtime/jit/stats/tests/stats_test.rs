use super::*;

#[test]
fn jit_stats_bucket_index_boundaries() {
    assert_eq!(bucket_index(0), 0);
    assert_eq!(bucket_index(99), 0);
    assert_eq!(bucket_index(100), 1);
    assert_eq!(bucket_index(249), 1);
    assert_eq!(bucket_index(250), 2);
    assert_eq!(bucket_index(500), 3);
    assert_eq!(bucket_index(999), 3);
    assert_eq!(bucket_index(1_000), 4);
    assert_eq!(bucket_index(2_500), 5);
    assert_eq!(bucket_index(5_000), 6);
    assert_eq!(bucket_index(9_999), 6);
    assert_eq!(bucket_index(10_000), 7);
    assert_eq!(bucket_index(u64::MAX), 7);
}
