use super::*;

#[test]
fn store_assigns_ids_computes_totals_and_evicts() {
    let mut s = CaptureStore::new(2);
    let a = s.store("x;y 3\nx;z 4".to_string()); // total 7
    let b = s.store("p 5".to_string()); // total 5
    assert_eq!(a, 1);
    assert_eq!(b, 2);
    assert_eq!(s.folded(a), Some("x;y 3\nx;z 4"));
    let c = s.store("q 1".to_string()); // evicts id 1
    assert_eq!(c, 3);
    assert_eq!(s.folded(1), None, "oldest should be evicted");
    assert_eq!(s.folded(2), Some("p 5"));
    let list = s.list();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].0, 2); // id
    assert_eq!(list[1].0, 3);
    assert_eq!(list[1].1, 1); // total_samples of "q 1"
}
