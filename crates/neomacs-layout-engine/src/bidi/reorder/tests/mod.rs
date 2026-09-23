use super::*;

#[test]
fn test_all_ltr() {
    let levels = vec![0, 0, 0, 0, 0];
    let order = reorder_visual(&levels);
    assert_eq!(order, vec![0, 1, 2, 3, 4]);
}

#[test]
fn test_all_rtl() {
    let levels = vec![1, 1, 1, 1, 1];
    let order = reorder_visual(&levels);
    assert_eq!(order, vec![4, 3, 2, 1, 0]);
}

#[test]
fn test_mixed_ltr_rtl() {
    // LTR LTR RTL RTL LTR
    let levels = vec![0, 0, 1, 1, 0];
    let order = reorder_visual(&levels);
    // RTL segment [2,3] is reversed
    assert_eq!(order, vec![0, 1, 3, 2, 4]);
}

#[test]
fn test_nested_levels() {
    // Level 0, 1, 2, 1, 0
    let levels = vec![0, 1, 2, 1, 0];
    let order = reorder_visual(&levels);
    // Level 2: reverse [2] (single, no change from reversal alone)
    // Level 1: reverse [1,2,3] → [3,2,1]
    // Result: [0, 3, 2, 1, 4]
    assert_eq!(order, vec![0, 3, 2, 1, 4]);
}

#[test]
fn test_empty() {
    let levels: Vec<u8> = vec![];
    let order = reorder_visual(&levels);
    assert!(order.is_empty());
}

#[test]
fn test_mirroring() {
    let chars = vec!['(', 'A', ')'];
    let levels = vec![1, 1, 1];
    let mirrored = apply_mirroring(&chars, &levels);
    assert_eq!(mirrored, vec![')', 'A', '(']);
}

#[test]
fn test_reorder_line() {
    // "Hello" + RTL "אב" + "World"
    let chars: Vec<char> = "Hello".chars().collect();
    let levels = vec![0, 0, 0, 0, 0];
    let result = reorder_line(&chars, &levels);
    assert_eq!(result, chars);
}

#[test]
fn test_reorder_with_brackets() {
    let chars = vec!['(', 'A', ')'];
    let levels = vec![1, 1, 1]; // All RTL
    let result = reorder_line(&chars, &levels);
    // Visual order: reversed, with mirroring
    // Logical: ( A ) at levels 1,1,1
    // Mirror: ) A (
    // Reorder: reverse all → ( A )
    assert_eq!(result, vec!['(', 'A', ')']);
}

#[test]
fn test_complex_reorder() {
    // LTR text with RTL embedded
    // Logical: A B [rtl]C D[/rtl] E F
    let levels = vec![0, 0, 1, 1, 0, 0];
    let order = reorder_visual(&levels);
    // RTL segment [2,3] reversed → [3,2]
    assert_eq!(order, vec![0, 1, 3, 2, 4, 5]);
}
