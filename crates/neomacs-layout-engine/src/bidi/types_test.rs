use super::*;

// ===================================================================
// BidiClass::is_strong()
// ===================================================================

#[test]
fn is_strong_returns_true_for_l_r_al() {
    assert!(BidiClass::L.is_strong());
    assert!(BidiClass::R.is_strong());
    assert!(BidiClass::AL.is_strong());
}

#[test]
fn is_strong_returns_false_for_weak_types() {
    let weak = [
        BidiClass::EN,
        BidiClass::ES,
        BidiClass::ET,
        BidiClass::AN,
        BidiClass::CS,
        BidiClass::NSM,
        BidiClass::BN,
    ];
    for cls in &weak {
        assert!(!cls.is_strong(), "{:?} should not be strong", cls);
    }
}

#[test]
fn is_strong_returns_false_for_neutral_types() {
    let neutral = [BidiClass::B, BidiClass::S, BidiClass::WS, BidiClass::ON];
    for cls in &neutral {
        assert!(!cls.is_strong(), "{:?} should not be strong", cls);
    }
}

#[test]
fn is_strong_returns_false_for_explicit_types() {
    let explicit = [
        BidiClass::LRE,
        BidiClass::LRO,
        BidiClass::RLE,
        BidiClass::RLO,
        BidiClass::PDF,
        BidiClass::LRI,
        BidiClass::RLI,
        BidiClass::FSI,
        BidiClass::PDI,
    ];
    for cls in &explicit {
        assert!(!cls.is_strong(), "{:?} should not be strong", cls);
    }
}

// ===================================================================
// BidiClass::is_weak()
// ===================================================================

#[test]
fn is_weak_returns_true_for_all_weak_types() {
    assert!(BidiClass::EN.is_weak());
    assert!(BidiClass::ES.is_weak());
    assert!(BidiClass::ET.is_weak());
    assert!(BidiClass::AN.is_weak());
    assert!(BidiClass::CS.is_weak());
    assert!(BidiClass::NSM.is_weak());
    assert!(BidiClass::BN.is_weak());
}

#[test]
fn is_weak_returns_false_for_strong_types() {
    assert!(!BidiClass::L.is_weak());
    assert!(!BidiClass::R.is_weak());
    assert!(!BidiClass::AL.is_weak());
}

#[test]
fn is_weak_returns_false_for_neutral_types() {
    assert!(!BidiClass::B.is_weak());
    assert!(!BidiClass::S.is_weak());
    assert!(!BidiClass::WS.is_weak());
    assert!(!BidiClass::ON.is_weak());
}

#[test]
fn is_weak_returns_false_for_explicit_types() {
    let explicit = [
        BidiClass::LRE,
        BidiClass::LRO,
        BidiClass::RLE,
        BidiClass::RLO,
        BidiClass::PDF,
        BidiClass::LRI,
        BidiClass::RLI,
        BidiClass::FSI,
        BidiClass::PDI,
    ];
    for cls in &explicit {
        assert!(!cls.is_weak(), "{:?} should not be weak", cls);
    }
}

// ===================================================================
// BidiClass::is_neutral()
// ===================================================================

#[test]
fn is_neutral_returns_true_for_all_neutral_types() {
    assert!(BidiClass::B.is_neutral());
    assert!(BidiClass::S.is_neutral());
    assert!(BidiClass::WS.is_neutral());
    assert!(BidiClass::ON.is_neutral());
}

#[test]
fn is_neutral_returns_false_for_strong_types() {
    assert!(!BidiClass::L.is_neutral());
    assert!(!BidiClass::R.is_neutral());
    assert!(!BidiClass::AL.is_neutral());
}

#[test]
fn is_neutral_returns_false_for_weak_types() {
    let weak = [
        BidiClass::EN,
        BidiClass::ES,
        BidiClass::ET,
        BidiClass::AN,
        BidiClass::CS,
        BidiClass::NSM,
        BidiClass::BN,
    ];
    for cls in &weak {
        assert!(!cls.is_neutral(), "{:?} should not be neutral", cls);
    }
}

#[test]
fn is_neutral_returns_false_for_explicit_types() {
    let explicit = [
        BidiClass::LRE,
        BidiClass::LRO,
        BidiClass::RLE,
        BidiClass::RLO,
        BidiClass::PDF,
        BidiClass::LRI,
        BidiClass::RLI,
        BidiClass::FSI,
        BidiClass::PDI,
    ];
    for cls in &explicit {
        assert!(!cls.is_neutral(), "{:?} should not be neutral", cls);
    }
}

// ===================================================================
// BidiClass::is_explicit()
// ===================================================================

#[test]
fn is_explicit_returns_true_for_all_explicit_types() {
    assert!(BidiClass::LRE.is_explicit());
    assert!(BidiClass::LRO.is_explicit());
    assert!(BidiClass::RLE.is_explicit());
    assert!(BidiClass::RLO.is_explicit());
    assert!(BidiClass::PDF.is_explicit());
    assert!(BidiClass::LRI.is_explicit());
    assert!(BidiClass::RLI.is_explicit());
    assert!(BidiClass::FSI.is_explicit());
    assert!(BidiClass::PDI.is_explicit());
}

#[test]
fn is_explicit_returns_false_for_strong_types() {
    assert!(!BidiClass::L.is_explicit());
    assert!(!BidiClass::R.is_explicit());
    assert!(!BidiClass::AL.is_explicit());
}

#[test]
fn is_explicit_returns_false_for_weak_and_neutral_types() {
    let non_explicit = [
        BidiClass::EN,
        BidiClass::ES,
        BidiClass::ET,
        BidiClass::AN,
        BidiClass::CS,
        BidiClass::NSM,
        BidiClass::BN,
        BidiClass::B,
        BidiClass::S,
        BidiClass::WS,
        BidiClass::ON,
    ];
    for cls in &non_explicit {
        assert!(!cls.is_explicit(), "{:?} should not be explicit", cls);
    }
}

// ===================================================================
// Every variant belongs to exactly one category
// ===================================================================

#[test]
fn every_variant_is_in_exactly_one_category() {
    let all_classes = [
        BidiClass::L,
        BidiClass::R,
        BidiClass::AL,
        BidiClass::EN,
        BidiClass::ES,
        BidiClass::ET,
        BidiClass::AN,
        BidiClass::CS,
        BidiClass::NSM,
        BidiClass::BN,
        BidiClass::B,
        BidiClass::S,
        BidiClass::WS,
        BidiClass::ON,
        BidiClass::LRE,
        BidiClass::LRO,
        BidiClass::RLE,
        BidiClass::RLO,
        BidiClass::PDF,
        BidiClass::LRI,
        BidiClass::RLI,
        BidiClass::FSI,
        BidiClass::PDI,
    ];
    for cls in &all_classes {
        let count = cls.is_strong() as u32
            + cls.is_weak() as u32
            + cls.is_neutral() as u32
            + cls.is_explicit() as u32;
        assert_eq!(
            count, 1,
            "{:?} belongs to {} categories (expected exactly 1)",
            cls, count
        );
    }
}

// ===================================================================
// BidiClass::is_isolate_initiator()
// ===================================================================

#[test]
fn is_isolate_initiator_for_lri_rli_fsi() {
    assert!(BidiClass::LRI.is_isolate_initiator());
    assert!(BidiClass::RLI.is_isolate_initiator());
    assert!(BidiClass::FSI.is_isolate_initiator());
}

#[test]
fn is_isolate_initiator_false_for_pdi() {
    // PDI is explicit but NOT an isolate initiator
    assert!(!BidiClass::PDI.is_isolate_initiator());
}

#[test]
fn is_isolate_initiator_false_for_non_isolate_explicit() {
    assert!(!BidiClass::LRE.is_isolate_initiator());
    assert!(!BidiClass::LRO.is_isolate_initiator());
    assert!(!BidiClass::RLE.is_isolate_initiator());
    assert!(!BidiClass::RLO.is_isolate_initiator());
    assert!(!BidiClass::PDF.is_isolate_initiator());
}

#[test]
fn is_isolate_initiator_false_for_strong_weak_neutral() {
    let non_isolate = [
        BidiClass::L,
        BidiClass::R,
        BidiClass::AL,
        BidiClass::EN,
        BidiClass::ES,
        BidiClass::ET,
        BidiClass::AN,
        BidiClass::CS,
        BidiClass::NSM,
        BidiClass::BN,
        BidiClass::B,
        BidiClass::S,
        BidiClass::WS,
        BidiClass::ON,
    ];
    for cls in &non_isolate {
        assert!(
            !cls.is_isolate_initiator(),
            "{:?} should not be isolate initiator",
            cls
        );
    }
}

// ===================================================================
// BidiClass::is_removed_by_x9()
// ===================================================================

#[test]
fn is_removed_by_x9_for_embedding_override_and_bn() {
    // LRE, RLE, LRO, RLO, PDF, BN are removed by X9
    assert!(BidiClass::LRE.is_removed_by_x9());
    assert!(BidiClass::RLE.is_removed_by_x9());
    assert!(BidiClass::LRO.is_removed_by_x9());
    assert!(BidiClass::RLO.is_removed_by_x9());
    assert!(BidiClass::PDF.is_removed_by_x9());
    assert!(BidiClass::BN.is_removed_by_x9());
}

#[test]
fn is_removed_by_x9_false_for_isolate_types() {
    // Isolate types are NOT removed by X9
    assert!(!BidiClass::LRI.is_removed_by_x9());
    assert!(!BidiClass::RLI.is_removed_by_x9());
    assert!(!BidiClass::FSI.is_removed_by_x9());
    assert!(!BidiClass::PDI.is_removed_by_x9());
}

#[test]
fn is_removed_by_x9_false_for_strong_types() {
    assert!(!BidiClass::L.is_removed_by_x9());
    assert!(!BidiClass::R.is_removed_by_x9());
    assert!(!BidiClass::AL.is_removed_by_x9());
}

#[test]
fn is_removed_by_x9_false_for_non_bn_weak_types() {
    // BN is weak AND removed by X9, but other weak types are not
    assert!(!BidiClass::EN.is_removed_by_x9());
    assert!(!BidiClass::ES.is_removed_by_x9());
    assert!(!BidiClass::ET.is_removed_by_x9());
    assert!(!BidiClass::AN.is_removed_by_x9());
    assert!(!BidiClass::CS.is_removed_by_x9());
    assert!(!BidiClass::NSM.is_removed_by_x9());
}

#[test]
fn is_removed_by_x9_false_for_neutral_types() {
    assert!(!BidiClass::B.is_removed_by_x9());
    assert!(!BidiClass::S.is_removed_by_x9());
    assert!(!BidiClass::WS.is_removed_by_x9());
    assert!(!BidiClass::ON.is_removed_by_x9());
}

// ===================================================================
// BidiClass::to_strong_for_neutral()
// ===================================================================

#[test]
fn to_strong_for_neutral_maps_en_and_an_to_r() {
    assert_eq!(BidiClass::EN.to_strong_for_neutral(), BidiClass::R);
    assert_eq!(BidiClass::AN.to_strong_for_neutral(), BidiClass::R);
}

#[test]
fn to_strong_for_neutral_preserves_strong_types() {
    assert_eq!(BidiClass::L.to_strong_for_neutral(), BidiClass::L);
    assert_eq!(BidiClass::R.to_strong_for_neutral(), BidiClass::R);
    assert_eq!(BidiClass::AL.to_strong_for_neutral(), BidiClass::AL);
}

#[test]
fn to_strong_for_neutral_preserves_other_types() {
    // Other weak, neutral, and explicit types pass through unchanged
    let others = [
        BidiClass::ES,
        BidiClass::ET,
        BidiClass::CS,
        BidiClass::NSM,
        BidiClass::BN,
        BidiClass::B,
        BidiClass::S,
        BidiClass::WS,
        BidiClass::ON,
        BidiClass::LRE,
        BidiClass::PDI,
    ];
    for cls in &others {
        assert_eq!(
            cls.to_strong_for_neutral(),
            *cls,
            "{:?} should pass through unchanged",
            cls
        );
    }
}

// ===================================================================
// BidiDir
// ===================================================================

#[test]
fn bidi_dir_base_level_ltr_is_0() {
    assert_eq!(BidiDir::LTR.base_level(), 0);
}

#[test]
fn bidi_dir_base_level_rtl_is_1() {
    assert_eq!(BidiDir::RTL.base_level(), 1);
}

#[test]
fn bidi_dir_base_level_auto_is_0() {
    // Auto defaults to LTR (level 0) per UAX#9
    assert_eq!(BidiDir::Auto.base_level(), 0);
}

#[test]
fn bidi_dir_codes_match_gnu_bidi_dir_t() {
    let directions = [(BidiDir::Auto, 0), (BidiDir::LTR, 1), (BidiDir::RTL, 2)];
    for (direction, code) in directions {
        assert_eq!(direction.gnu_dir_code(), code);
        assert_eq!(BidiDir::from_gnu_dir_code(code), Some(direction));
    }
    assert_eq!(BidiDir::from_gnu_dir_code(3), None);
}

// ===================================================================
// BracketType
// ===================================================================

#[test]
fn bracket_type_none() {
    let bt = BracketType::None;
    assert_eq!(bt, BracketType::None);
}

#[test]
fn bracket_type_open_stores_closing() {
    let bt = BracketType::Open(')');
    match bt {
        BracketType::Open(closing) => assert_eq!(closing, ')'),
        _ => panic!("Expected Open"),
    }
}

#[test]
fn bracket_type_close_stores_opening() {
    let bt = BracketType::Close('(');
    match bt {
        BracketType::Close(opening) => assert_eq!(opening, '('),
        _ => panic!("Expected Close"),
    }
}

// ===================================================================
// Override
// ===================================================================

#[test]
fn override_variants_are_distinct() {
    assert_ne!(Override::Neutral, Override::LTR);
    assert_ne!(Override::Neutral, Override::RTL);
    assert_ne!(Override::LTR, Override::RTL);
}

#[test]
fn override_codes_match_gnu_bidi_dir_t() {
    let overrides = [
        (Override::Neutral, 0),
        (Override::LTR, 1),
        (Override::RTL, 2),
    ];
    for (override_status, code) in overrides {
        assert_eq!(override_status.gnu_dir_code(), code);
        assert_eq!(Override::from_gnu_dir_code(code), Some(override_status));
    }
    assert_eq!(Override::from_gnu_dir_code(3), None);
}

// ===================================================================
// DirectionalStatus
// ===================================================================

#[test]
fn directional_status_default_construction() {
    let ds = DirectionalStatus {
        level: 0,
        override_status: Override::Neutral,
        isolate_status: false,
    };
    assert_eq!(ds.level, 0);
    assert_eq!(ds.override_status, Override::Neutral);
    assert!(!ds.isolate_status);
}

#[test]
fn directional_status_rtl_override_isolate() {
    let ds = DirectionalStatus {
        level: 1,
        override_status: Override::RTL,
        isolate_status: true,
    };
    assert_eq!(ds.level, 1);
    assert_eq!(ds.override_status, Override::RTL);
    assert!(ds.isolate_status);
}

// ===================================================================
// ResolvedChar
// ===================================================================

#[test]
fn resolved_char_stores_fields() {
    let rc = ResolvedChar {
        ch: 'A',
        original_class: BidiClass::L,
        level: 0,
    };
    assert_eq!(rc.ch, 'A');
    assert_eq!(rc.original_class, BidiClass::L);
    assert_eq!(rc.level, 0);
}

#[test]
fn resolved_char_rtl() {
    let rc = ResolvedChar {
        ch: '\u{0627}', // Arabic Alef
        original_class: BidiClass::AL,
        level: 1,
    };
    assert_eq!(rc.original_class, BidiClass::AL);
    assert_eq!(rc.level, 1);
}

// ===================================================================
// Constants
// ===================================================================

#[test]
fn max_depth_is_125() {
    assert_eq!(MAX_DEPTH, 125);
}

#[test]
fn max_bpa_stack_is_63() {
    assert_eq!(MAX_BPA_STACK, 63);
}

// ===================================================================
// BidiClass GNU type codes
// ===================================================================

#[test]
fn bidi_class_codes_match_gnu_bidi_type_t() {
    let classes = [
        (BidiClass::L, 1),
        (BidiClass::R, 2),
        (BidiClass::EN, 3),
        (BidiClass::AN, 4),
        (BidiClass::BN, 5),
        (BidiClass::B, 6),
        (BidiClass::AL, 7),
        (BidiClass::LRE, 8),
        (BidiClass::LRO, 9),
        (BidiClass::RLE, 10),
        (BidiClass::RLO, 11),
        (BidiClass::PDF, 12),
        (BidiClass::LRI, 13),
        (BidiClass::RLI, 14),
        (BidiClass::FSI, 15),
        (BidiClass::PDI, 16),
        (BidiClass::ES, 17),
        (BidiClass::ET, 18),
        (BidiClass::CS, 19),
        (BidiClass::NSM, 20),
        (BidiClass::S, 21),
        (BidiClass::WS, 22),
        (BidiClass::ON, 23),
    ];

    assert_eq!(BidiClass::from_gnu_type_code(0), None);
    for (class, code) in classes {
        assert_eq!(class.gnu_type_code(), code);
        assert_eq!(BidiClass::from_gnu_type_code(code), Some(class));
    }
    assert_eq!(BidiClass::from_gnu_type_code(24), None);
}

// ===================================================================
// BidiClass traits: Clone, Copy, PartialEq, Eq, Hash, Debug
// ===================================================================

#[test]
fn bidi_class_clone_and_copy() {
    let a = BidiClass::L;
    let b = a; // Copy
    let c = a.clone(); // Clone
    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn bidi_class_equality() {
    assert_eq!(BidiClass::L, BidiClass::L);
    assert_ne!(BidiClass::L, BidiClass::R);
}

#[test]
fn bidi_class_debug_format() {
    let dbg = format!("{:?}", BidiClass::LRI);
    assert_eq!(dbg, "LRI");
}

#[test]
fn bidi_class_hash_works_in_hashmap() {
    use std::collections::HashMap;
    let mut map = HashMap::new();
    map.insert(BidiClass::L, "left");
    map.insert(BidiClass::R, "right");
    assert_eq!(map.get(&BidiClass::L), Some(&"left"));
    assert_eq!(map.get(&BidiClass::R), Some(&"right"));
    assert_eq!(map.get(&BidiClass::AL), None);
}

// ===================================================================
// Edge case: BN is both weak and removed by X9
// ===================================================================

#[test]
fn bn_is_weak_and_removed_by_x9() {
    assert!(BidiClass::BN.is_weak());
    assert!(BidiClass::BN.is_removed_by_x9());
    // BN is NOT explicit, even though other X9-removed types are
    assert!(!BidiClass::BN.is_explicit());
}

// ===================================================================
// Edge case: PDF is explicit and removed by X9, but not isolate
// ===================================================================

#[test]
fn pdf_is_explicit_and_removed_by_x9_but_not_isolate() {
    assert!(BidiClass::PDF.is_explicit());
    assert!(BidiClass::PDF.is_removed_by_x9());
    assert!(!BidiClass::PDF.is_isolate_initiator());
}
