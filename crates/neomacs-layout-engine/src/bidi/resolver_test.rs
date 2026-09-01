use super::*;

#[test]
fn test_all_ltr() {
    let levels = resolve_levels("Hello World", BidiDir::LTR);
    assert!(levels.iter().all(|&l| l == 0));
}

#[test]
fn test_all_rtl_hebrew() {
    let levels = resolve_levels("\u{05D0}\u{05D1}\u{05D2}", BidiDir::RTL);
    // Hebrew letters at RTL paragraph level
    assert!(levels.iter().all(|&l| l == 1));
}

#[test]
fn test_mixed_ltr_rtl() {
    // "Hello" + Hebrew "שלום"
    let text = "Hello \u{05E9}\u{05DC}\u{05D5}\u{05DD}";
    let levels = resolve_levels(text, BidiDir::LTR);
    // First 6 chars (Hello + space) should be level 0
    for i in 0..6 {
        assert_eq!(levels[i], 0, "pos {}: expected 0, got {}", i, levels[i]);
    }
    // Hebrew should be level 1
    for i in 6..10 {
        assert_eq!(levels[i], 1, "pos {}: expected 1, got {}", i, levels[i]);
    }
}

#[test]
fn test_arabic_numbers() {
    // Arabic text with Arabic-Indic digits
    let text = "\u{0627}\u{0660}\u{0661}\u{0628}";
    let levels = resolve_levels(text, BidiDir::RTL);
    // Arabic letters: level 1, Arabic-Indic digits: level 2
    assert_eq!(levels[0], 1); // Arabic letter
    assert_eq!(levels[1], 2); // AN → level+2 at odd level → 1+1=2
    assert_eq!(levels[2], 2);
    assert_eq!(levels[3], 1);
}

#[test]
fn test_european_numbers_in_ltr() {
    let text = "abc 123 def";
    let levels = resolve_levels(text, BidiDir::LTR);
    // All LTR with EN → stays at level 0 (W7: EN→L when last strong is L)
    assert!(levels.iter().all(|&l| l == 0));
}

#[test]
fn test_paragraph_level_auto() {
    // First strong char is Hebrew → RTL paragraph
    let text = "\u{05D0}Hello";
    let levels = resolve_levels(text, BidiDir::Auto);
    assert_eq!(levels[0], 1); // Hebrew: RTL
    // "Hello" embedded in RTL context → level 2
    assert_eq!(levels[1], 2);
}

#[test]
fn test_paragraph_level_auto_ltr() {
    let text = "Hello\u{05D0}";
    let levels = resolve_levels(text, BidiDir::Auto);
    // First strong is L → LTR paragraph
    assert_eq!(levels[0], 0);
}

#[test]
fn test_empty() {
    let levels = resolve_levels("", BidiDir::LTR);
    assert!(levels.is_empty());
}

#[test]
fn test_whitespace_at_line_end() {
    // L1: Whitespace at line end resets to paragraph level
    let text = "\u{05D0}\u{05D1} ";
    let levels = resolve_levels(text, BidiDir::RTL);
    // Hebrew at level 1, trailing space reset to paragraph level (1)
    assert_eq!(levels[0], 1);
    assert_eq!(levels[1], 1);
    assert_eq!(levels[2], 1); // L1: reset to paragraph level
}

#[test]
fn test_brackets() {
    // Brackets should take direction from enclosed content
    let text = "Hello (\u{05D0}\u{05D1}) world";
    let levels = resolve_levels(text, BidiDir::LTR);
    // "Hello " = level 0
    // "(" = level 0 (N0b: L context around bracket pair with R inside → context)
    // Hebrew = level 1
    // ")" = level 0
    // " world" = level 0
    assert_eq!(levels[0], 0); // H
    assert_eq!(levels[7], 1); // Hebrew alef
}

#[test]
fn test_explicit_lre_pdf() {
    // LRE ... PDF embedding
    let text = "A\u{202A}B\u{202C}C";
    let levels = resolve_levels(text, BidiDir::LTR);
    assert_eq!(levels[0], 0); // A
    // LRE creates even level > 0, so level 2
    assert_eq!(levels[2], 2); // B (inside LRE)
    assert_eq!(levels[4], 0); // C (after PDF)
}

#[test]
fn test_explicit_rle_pdf() {
    // RLE ... PDF embedding
    let text = "A\u{202B}B\u{202C}C";
    let levels = resolve_levels(text, BidiDir::LTR);
    assert_eq!(levels[0], 0); // A
    // RLE creates odd level > 0, so level 1. B is L at odd level → I2 → level+1=2
    assert_eq!(levels[2], 2); // B (inside RLE, L char at odd level)
    assert_eq!(levels[4], 0); // C
}

#[test]
fn test_isolates_lri_pdi() {
    let text = "A\u{2066}B\u{2069}C";
    let levels = resolve_levels(text, BidiDir::LTR);
    assert_eq!(levels[0], 0); // A
    // LRI creates even level > 0, so level 2
    assert_eq!(levels[2], 2); // B (inside LRI)
    assert_eq!(levels[4], 0); // C (after PDI)
}

#[test]
fn test_nesting_depth() {
    // Test that deep nesting doesn't panic
    let mut text = String::from("A");
    for _ in 0..200 {
        text.push('\u{202A}'); // LRE
    }
    text.push('B');
    for _ in 0..200 {
        text.push('\u{202C}'); // PDF
    }
    text.push('C');
    let levels = resolve_levels(&text, BidiDir::LTR);
    assert_eq!(levels[0], 0); // A
}

#[test]
fn test_weak_rule_w4_es_between_en() {
    // "1+2" — ES between EN should become EN
    let text = "1+2";
    let levels = resolve_levels(text, BidiDir::LTR);
    // All should be level 0 (W7 converts EN→L when last strong is L/sos)
    assert!(levels.iter().all(|&l| l == 0));
}

#[test]
fn test_mirror_lookup() {
    use super::super::tables::bidi_mirror;
    assert_eq!(bidi_mirror('('), Some(')'));
    assert_eq!(bidi_mirror(')'), Some('('));
    assert_eq!(bidi_mirror('\u{27E8}'), Some('\u{27E9}'));
}
